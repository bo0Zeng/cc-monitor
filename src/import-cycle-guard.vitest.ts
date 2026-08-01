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
 * - `export … from "./x"` 是**运行期**再导出，算边。
 * - 不解析动态 `import()`（本仓生产侧没有）。
 */
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve, dirname, relative } from "node:path";
import { stripComments } from "./test-support/strip-comments";

const REPO_ROOT = resolve(__dirname, "..");
const SRC = join(REPO_ROOT, "src");

/** 生产 .ts（排掉测试与生成物；生成物是叶子类型、不会成环）。 */
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
 * 剥注释是必须的：本仓注释里成篇地写 `import { x } from "./y"` 当例子
 * （`ipc/commands.ts` 的头注就是），不剥的话图里会多出根本不存在的边。
 */
function runtimeDeps(file: string): string[] {
  const code = stripComments(readFileSync(file, "utf8"), "ts");
  const out: string[] = [];
  // `import … from "./x"` / `export … from "./x"`；`import type` / `export type` 排除。
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

  it("★ 反向自检：图真的建起来了（否则下面那条恒绿）", () => {
    expect(files.length, "一个生产 .ts 都没扫到 —— 遍历坏了").toBeGreaterThan(100);
    const edges = [...graph.values()].reduce((n, v) => n + v.length, 0);
    expect(edges, "图里一条边都没有 —— import 抠法坏了").toBeGreaterThan(200);
    // 抠出来的目标必须都是真文件（`statSync` 已经保证，这里再钉一次口径）
    for (const [, deps] of graph) {
      for (const d of deps) expect(files.includes(d) || d.endsWith("index.ts")).toBe(true);
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
    const mc = join(SRC, "settings", "machine-card.ts");
    const deps = graph.get(mc) ?? [];
    expect(deps.some((d) => d.includes("generated"))).toBe(false);
  });
});
