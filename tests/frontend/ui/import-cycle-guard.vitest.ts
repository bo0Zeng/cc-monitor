/**
 * E80：**生产代码里不许有运行期 import 环。**
 *
 * # 为什么本仓需要自己写一条
 *
 * `eslint.config.js` 只有 `js.configs.recommended` + `tseslint.configs.recommended`，
 * **没有 `eslint-plugin-import` / `import/no-cycle`** ⇒ 环在本仓是**结构性不可见**的。
 * 2026-08-01 的 Phase G 审计（代码工程视角）自己写了 import 图 + DFS 才扫出一条真的：
 * `remote-section.ts ⇄ machine-card.ts` —— `machine-card` 是从 `remote-section` 抽出去的，
 * 却回头 import 上层的**值**（`describeStage`）。
 *
 * 那条今天不炸，只是因为两边的用点都在方法体里、模块求值期不触发（TDZ 型隐患）。
 * 「今天不炸」不是安全，是**没人知道它在**。
 *
 * # 为什么不直接装 `eslint-plugin-import`
 *
 * 本会话在册的红线里有「不装包」。而这条判据本身很短（下面 ~60 行），
 * 装一个插件换来的额外能力（解析 `exports` 映射、monorepo 别名）本仓一样都用不上。
 * **如果哪天要装，这条守卫应当被它取代而不是并存** —— 两条判据并存必然漂。
 *
 * # 判据的边界（说清楚，别让人以为它等价于 `import/no-cycle`）
 *
 * - 只看**相对路径** import（`./x` / `../y`）。裸包名（`@tauri-apps/api`）不参与 —— 那是外部依赖。
 * - **`import type` 不算边**：TS 编译期擦除，运行期不存在这条依赖。
 *   （审计实测本仓还有一条纯 type-only 的 `cards/index.ts ⇄ cards/subagent.ts`，
 *   它**不该**被这条守卫拦 —— 拦了就是逼人为一条不存在的运行期依赖做重构。）
 * - `export … from "../src/x"` 是**运行期**再导出，算边。
 * - 不解析动态 `import()`（本仓生产侧没有）。
 *
 * # 〔FE1 · 第四波 4D〕第二条：**连类型边一起算**的全图，强连通分量 == 豁免表（两向）
 *
 * 上面那条「`import type` 不算边」的理由对**运行期**成立；但审计 B §4 现打出一个 7 模块的类型环
 * （`accounts · accounts-decode · config · history-reads · ipc/chan · ipc/chan-caller · ipc/commands`），
 * 只靠 `ipc/commands.ts` 为一个返回类型回头 `import type` 账号域闭合 —— **通信层在类型上依赖账号域**，
 * 那是分层反了，不是「不存在的依赖」。FE1 的题面要「import 图无环（现打全图）」、「类型住被依赖的一侧」。
 * ⇒ 第二条判据量全图（值边 ＋ 类型边），强连通分量集合 == [`TYPE_CYCLE_EXEMPT`]（两向：新长一个环 ⇒ 红；
 * 豁免的那个环被拆了而表没摘 ⇒ 红）。守的要求：`设计/01 §5` D1「一个判定只有一个家」（类型住被依赖的一侧，
 * 一个形状不在两个域各有一份说法）。
 */
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve, dirname, relative } from "node:path";
import { stripComments } from "../../test-support/strip-comments";

const REPO_ROOT = resolve(__dirname, "../../..");
const SRC = join(REPO_ROOT, "src");
// 〔RE〕前端 TS 住 `src/frontend/ui/`；人群仍扫整棵 `src/`（通信层那份 `chan.ts` 住 `src/comms/inward/`）。
const UI = join(SRC, "frontend", "ui");

/**
 * 生产 .ts（排掉测试与生成物；生成物是**叶子**，不会成环）。
 *
 * 🔴 `K-R93`（09-12）：原句写的是「生成物是叶子**类型**」—— 那半句今天过期了。
 * `src/frontend/ui/generated/agent-profile-table.ts` 是一份**值**表（agent 画像，源在
 * `src/frontend/shell/src/adapter.rs`），于是生成物第一次成为**运行期** import 的目标。
 * `K-R95`（09-12）又加了一份（`launch-render-facts.ts`，源在
 * `src/frontend/shell/src/launch_wire.rs`）——下面那条自检的数因此是 2 不是 1。
 * 「不会成环」那一半**仍然成立且现在被机检**：见下面自检里那条「生成物真的是叶子」。
 */
function productionTsFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "generated" || name === "test-support") continue;
      productionTsFiles(p, out);
      continue;
    }
    if (!name.endsWith(".ts")) continue;
    if (name.endsWith(".vitest.ts") || name.endsWith(".test.ts") || name.endsWith(".d.ts")) continue;
    out.push(p);
  }
  return out;
}

/**
 * 抠出一个文件的**运行期**相对 import 目标（绝对路径，已补 `.ts` / `/index.ts`）。
 *
 * 剥注释是必须的：本仓注释里成篇地写 `import { x } from "../src/y"` 当例子
 * （`ipc/commands.ts` 的头注就是），不剥的话图里会多出根本不存在的边。
 */
function runtimeDeps(file: string): string[] {
  const code = stripComments(readFileSync(file, "utf8"), "ts");
  const out: string[] = [];
  // `import … from "../src/x"` / `export … from "../src/x"`；`import type` / `export type` 排除。
  const re = /(?:^|\n)\s*(import|export)\s+([^;]*?)\s*from\s*["'](\.[^"']+)["']/g;
  for (const m of code.matchAll(re)) {
    const clause = m[2];
    // 整句 type-only：`import type {…}` / `export type {…}`
    if (/^type\b/.test(clause.trim())) continue;
    // 逐项 type-only 且没有值项：`import { type A, type B }`
    const braced = /^\{([\s\S]*)\}$/.exec(clause.trim());
    if (braced) {
      const items = braced[1]
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      if (items.length > 0 && items.every((s) => /^type\s/.test(s))) continue;
    }
    const spec = m[3].replace(/\.ts$/, "");
    const base = resolve(dirname(file), spec);
    for (const cand of [`${base}.ts`, join(base, "index.ts")]) {
      try {
        if (statSync(cand).isFile()) {
          out.push(cand);
          break;
        }
      } catch {
        /* 下一个候选 */
      }
    }
  }
  return out;
}

/**
 * 〔FE1〕抠出一个文件的**全部**相对 import 目标（值边 ＋ 类型边 ＋ `export … from`）。
 * 与 [`runtimeDeps`] 同一套抠法，只是不跳过 type-only。
 */
function allDeps(file: string): string[] {
  const code = stripComments(readFileSync(file, "utf8"), "ts");
  const out = new Set<string>();
  const re = /(?:^|\n)\s*(import|export)\s+([^;]*?)\s*from\s*["'](\.[^"']+)["']/g;
  for (const m of code.matchAll(re)) {
    const spec = m[3].replace(/\.ts$/, "");
    const base = resolve(dirname(file), spec);
    for (const cand of [`${base}.ts`, join(base, "index.ts")]) {
      try {
        if (statSync(cand).isFile()) {
          out.add(cand);
          break;
        }
      } catch {
        /* 下一个候选 */
      }
    }
  }
  return [...out];
}

/** 〔FE1〕强连通分量（Tarjan）：只回「真成环」的那些（≥2 个成员，或自指）。每个分量内部按路径排序。 */
function cycleComponents(graph: Map<string, string[]>): string[][] {
  let index = 0;
  const idx = new Map<string, number>();
  const low = new Map<string, number>();
  const onStack = new Set<string>();
  const stack: string[] = [];
  const out: string[][] = [];
  const strong = (v: string): void => {
    idx.set(v, index);
    low.set(v, index);
    index += 1;
    stack.push(v);
    onStack.add(v);
    for (const w of graph.get(v) ?? []) {
      if (!graph.has(w)) continue;
      if (!idx.has(w)) {
        strong(w);
        low.set(v, Math.min(low.get(v)!, low.get(w)!));
      } else if (onStack.has(w)) {
        low.set(v, Math.min(low.get(v)!, idx.get(w)!));
      }
    }
    if (low.get(v) === idx.get(v)) {
      const comp: string[] = [];
      let w: string;
      do {
        w = stack.pop()!;
        onStack.delete(w);
        comp.push(w);
      } while (w !== v);
      if (comp.length > 1 || (graph.get(v) ?? []).includes(v)) out.push(comp.sort());
    }
  };
  for (const v of [...graph.keys()].sort()) if (!idx.has(v)) strong(v);
  return out.sort((a, b) => a[0].localeCompare(b[0]));
}

/**
 * 〔FE1〕全图（含类型边）里**今天还在**的环 —— 逐条写清为什么还在、归谁拆。不是豁免清单：两向相等，
 * 拆掉了不摘 ⇒ 红；新长一个 ⇒ 红。
 */
const TYPE_CYCLE_EXEMPT: ReadonlyArray<readonly [members: readonly string[], why: string]> = [
  // 〔FE1 子步 5〕`accounts-decode ⇄ accounts` 那一行摘了：`accounts.ts` 拆成模型（纯）＋ 读面（`account-reads.ts`）之后，
  //   值 import 解码器的是读面，解码器 `import type` 的是模型 ⇒ 环断（子步 4 登记时写明归本路摘）。
  [
    ["src/frontend/ui/cards/index.ts", "src/frontend/ui/cards/subagent.ts"],
    "纯类型边闭合（`cards/subagent.ts` 回头 `import type { JsonlRecord, RenderContext, RenderResult } from \"./index\"`）；" +
      "上面那条运行期判据的头注点过名。不在 FE1 写区：拆法是把那三个类型挪进卡片系的一个叶子，归卡片那一片的主人。",
  ],
  // 〔LR2〕`launch-dimensions ⇄ launch-plan` 那一行摘了：IR 的类型拆进纯类型叶子 `src/frontend/ui/launch-types.ts`，
  //   `launch-dimensions.ts` 改 `import type { LaunchDimension } from "./launch-types.ts"` ⇒ 环断（本行登记时写明归 LR2 摘）。
];

/** 返回找到的第一个环（按文件顺序确定性遍历），没有则 null。 */
function findCycle(graph: Map<string, string[]>): string[] | null {
  const WHITE = 0,
    GREY = 1,
    BLACK = 2;
  const color = new Map<string, number>();
  const stack: string[] = [];
  let found: string[] | null = null;

  const visit = (n: string): void => {
    if (found) return;
    color.set(n, GREY);
    stack.push(n);
    for (const next of graph.get(n) ?? []) {
      if (found) break;
      const c = color.get(next) ?? WHITE;
      if (c === GREY) {
        found = [...stack.slice(stack.indexOf(next)), next];
        break;
      }
      if (c === WHITE) visit(next);
    }
    stack.pop();
    color.set(n, BLACK);
  };

  for (const n of [...graph.keys()].sort()) {
    if ((color.get(n) ?? WHITE) === WHITE) visit(n);
    if (found) break;
  }
  return found;
}

describe("E80：生产代码不许有运行期 import 环", () => {
  const files = productionTsFiles(SRC).sort();
  const graph = new Map<string, string[]>(files.map((f) => [f, runtimeDeps(f)]));
  const rel = (p: string) => relative(REPO_ROOT, p).replace(/\\/g, "/");
  /** `src/frontend/ui/generated/` 下那份生成物 —— 上面的遍历刻意不收它们，但它们**能被 import**。 */
  const isGenerated = (p: string) => rel(p).startsWith("src/frontend/ui/generated/");

  it("★ 反向自检：图真的建起来了（否则下面那条恒绿）", () => {
    expect(files.length, "一个生产 .ts 都没扫到 —— 遍历坏了").toBeGreaterThan(100);
    const edges = [...graph.values()].reduce((n, v) => n + v.length, 0);
    expect(edges, "图里一条边都没有 —— import 抠法坏了").toBeGreaterThan(200);
    // 抠出来的目标必须都是真文件（`statSync` 已经保证，这里再钉一次口径）
    for (const [, deps] of graph) {
      for (const d of deps) {
        expect(
          files.includes(d) || d.endsWith("index.ts") || isGenerated(d),
          `import 目标 ${rel(d)} 既不在扫到的清单里、也不是 index/生成物`,
        ).toBe(true);
      }
    }
    // `K-R93`：生成物**真的是叶子**（它们自己不 import 任何相对路径）—— 上面那条
    // 放它们过关的**唯一理由**就是这个，所以在这里把它钉住，而不是当成一句注释。
    const generatedTargets = [...new Set([...graph.values()].flat())].filter(isGenerated);
    expect(
      generatedTargets.length,
      "今天有 3 份生成物被**运行期** import（`agent-profile-table`，K-R93；" +
        "`launch-render-facts`，K-R95；`judgment-rules`，〔DUP1〕模型名的放行式子，零 import 的叶子）—— " +
        "这个数变了就在这里红一次，好让新的那一份也过一遍「它是不是叶子」",
    ).toBe(3);
    for (const g of generatedTargets) {
      expect(runtimeDeps(g), `${rel(g)} 不再是叶子 —— 它开始 import 别人了，可能成环`).toEqual([]);
    }
  });

  it("★★ 零环", () => {
    const cycle = findCycle(graph);
    expect(
      cycle && cycle.map(rel),
      "发现运行期 import 环。**叶子模块回头 import 上层的值**是最常见的形状"
        + "（某模块从另一个抽出来、又反过来用它的东西）。修法通常是"
        + "「把只有一个消费者的东西搬到那个消费者身边」，而不是加一层间接。",
    ).toBeNull();
  });

  it("★ 判据真的会抓人：给它一条人造的环", () => {
    const a = "/fake/a.ts";
    const b = "/fake/b.ts";
    const fake = new Map<string, string[]>([
      [a, [b]],
      [b, [a]],
    ]);
    expect(findCycle(fake)).toEqual([a, b, a]);
    // 自指也算
    expect(findCycle(new Map([[a, [a]]]))).toEqual([a, a]);
    // 无环不该误报
    expect(findCycle(new Map([[a, [b]], [b, []]]))).toBeNull();
  });

  it("★ `import type` 不算边（运行期擦除；拦它等于逼人为不存在的依赖做重构）", () => {
    // 用真文件核：`machine-card.ts` 现在只 type-import 生成物，不该因此多出边。
    const mc = join(UI, "settings", "machine-card.ts");
    const deps = graph.get(mc) ?? [];
    expect(deps.some((d) => d.includes("generated"))).toBe(false);
  });
});

describe("〔FE1〕全图（值边 ＋ 类型边）的环 == 登记表（两向）", () => {
  const files = productionTsFiles(SRC).sort();
  const graph = new Map<string, string[]>(files.map((f) => [f, allDeps(f)]));
  const rel = (p: string) => relative(REPO_ROOT, p).replace(/\\/g, "/");

  it("★ 反向自检：全图比运行期图多出类型边（否则下面那条量的就是同一张图）", () => {
    const all = [...graph.values()].reduce((n, v) => n + v.length, 0);
    const runtime = files.reduce((n, f) => n + runtimeDeps(f).length, 0);
    expect(all, "全图一条边都没有 —— 抠法坏了").toBeGreaterThan(200);
    expect(all, "全图与运行期图一样大 —— 类型边没抠进来").toBeGreaterThan(runtime);
  });

  it("★★ 环的集合 == 登记表（新长的环 ⇒ 红；拆掉了没摘 ⇒ 红）", () => {
    const got = cycleComponents(graph).map((c) => c.map(rel).join(" ⇄ "));
    const want = TYPE_CYCLE_EXEMPT.map(([m]) => [...m].sort().join(" ⇄ ")).sort();
    expect(
      got,
      "import 全图（含 `import type`）的环变了。新长的那一个：类型该住**被依赖的一侧**" +
        "（通常是挪进一个零 import 的叶子，照 `src/frontend/ui/apikey-reads.ts` 收 `ApikeyRoutingView` 的做法），别在两个域之间来回 import。",
    ).toEqual(want);
  });

  it("★ 判据真的会抓人：人造一条类型回边，多出一个环；自指也算", () => {
    const a = "/fake/a.ts";
    const b = "/fake/b.ts";
    const c = "/fake/c.ts";
    expect(cycleComponents(new Map([[a, [b]], [b, [c]], [c, []]]))).toEqual([]);
    expect(cycleComponents(new Map([[a, [b]], [b, [c]], [c, [a]]]))).toEqual([[a, b, c]]);
    expect(cycleComponents(new Map([[a, [a]]]))).toEqual([[a]]);
    // 真图上注一条回边（`ipc/commands.ts` 回头依赖账号域的读面 —— 读面本来就依赖包装层），必须多出一个环。
    //   ⚠ 注的是 `account-reads.ts` 而不是 `accounts.ts`：子步 5 之后账号模型（`accounts.ts`）零 IO、不依赖包装层，
    //   往它身上注一条边成不了环 —— 那恰恰是拆分要的结果，不是判据瞎了。
    const injected = new Map(graph);
    const commands = join(UI, "ipc", "commands.ts");
    const accounts = join(UI, "account-reads.ts");
    injected.set(commands, [...(injected.get(commands) ?? []), accounts]);
    expect(
      cycleComponents(injected).some((c) => c.includes(commands) && c.includes(accounts)),
      "注了 `ipc/commands.ts → account-reads.ts` 这条回边，判据却没看见它们成环",
    ).toBe(true);
    expect(cycleComponents(graph).some((c) => c.includes(commands))).toBe(false);
  });

  it("每条登记都写清为什么还在、归谁拆", () => {
    for (const [m, why] of TYPE_CYCLE_EXEMPT) {
      expect(why.length, `${m.join(" ⇄ ")} 的理由像占位`).toBeGreaterThan(40);
      expect(/写区|归/.test(why), `${m.join(" ⇄ ")} 没说归谁拆`).toBe(true);
    }
  });
});
