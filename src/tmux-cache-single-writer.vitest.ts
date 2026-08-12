/**
 * audit-0805 F14 第五刀（报告 I9′）：**tmux 会话列表的取数点登记表**。
 *
 * # 报告说的是「3 写 1 读」，但那不是毛病所在
 *
 * 一个缓存有多个写者、少数读者，本身没什么不对。实测下来真正的毛病是：
 * `TabManager` 里**四处**各自 `invoke("list_remote_tmux")`，其中三处顺手写了缓存、
 * **一处没写** —— 而没写的那处（`awaitExitFor` 的轮询 tick）恰好是**唯一会反复取数的**：
 * 它每秒查一遍同一个 origin，一次都不喂缓存。
 *
 * ⇒ 病根是「**取数**」与「**写缓存**」是两件可以分开做的事。
 * 只要还能「取而不写」，下一个新增的取数点就会再漏一次 —— 这正是定框 **E3** 说的
 * 「权威源恰好一个」：不是「把三处都改对」，是让它**只剩一处**。
 *
 * 现在 `TabManager.fetchTmuxFresh` 是类内唯一取数点，取数与写缓存在语法上是同一件事。
 *
 * # 这张表为什么带例外列
 *
 * 全仓还有三处取数点**够不到那个缓存**（它是 `TabManager` 私有）。
 * 与其假装没有，不如**逐条登记 + 写清为什么**：新增第四处没登记的 ⇒ 本条红。
 * ⚠ 登记表不是豁免清单 —— 每条例外都得说清「为什么它没法走唯一取数点」。
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";

const SRC = resolve(__dirname);

/** 两种写法都要认：直接 `invoke("list_remote_tmux")` 与经 `commands.` 包装。 */
const PATTERNS = [
  /invoke<[^>]*>\(\s*"list_remote_tmux"/g,
  /commands\.list_remote_tmux\(/g,
];

/**
 * 够不到 `TabManager` 私有缓存、因而**允许取而不写**的取数点。
 *
 * 每条必须写清「为什么」。想加第四条之前先问：它是不是本来就该走 `fetchTmuxFresh`。
 */
const EXEMPT: ReadonlyArray<readonly [file: string, why: string]> = [
  [
    "ipc/commands.ts",
    "IPC 包装本体 —— 它就是那个 `invoke`，不是调用方。缓存策略不该住在包装层" +
      "（住进去等于让每个调用方都被动吃 8s 陈旧数据，包括 attach/kill 这些对新鲜度最敏感的）。",
  ],
  [
    "fork-flow.ts",
    "另一个模块，够不到 `TabManager` 的私有 `tmuxCache`。⚠ 它 `.catch(() => null)` " +
      "把失败压成了「没有会话」—— 与 `fetchTmuxFresh` 刻意保留的三态相反。要收编得先把缓存" +
      "搬成模块级 store（`tmux-sessions.ts` 头注已经点过这件事），那是另一件。",
  ],
  [
    "settings/machine-card.ts",
    "同上，另一个模块。⚠ **报告 I9′ 与核实台账都漏了这一处** —— 台账只点了 " +
      "`tabs.ts:3360` 与 `fork-flow.ts:135`。登记在这里，免得下次又漏。",
  ],
  [
    "remote-launch-run.ts",
    "另一个模块，够不到 `TabManager` 的私有 `tmuxCache`（同 `fork-flow.ts` / `machine-card.ts`）。" +
      "用途：`runNewSessionRemote` 起新会话前查一次「哪些名字被占了」，喂给 `mintTmuxName` 避让 —— " +
      "**这一次取数的意义就是要最新的**（拿 8s 前的快照去避让，正好会让到一个刚被占掉的名字）。" +
      "⚠ 它与 `machine-card.ts` 那处是**同一件事的两个入口**（历史页右键 / 设置面板「开新 Claude」)，" +
      "两处都在做「派生名 → 铸名」；等 `tmux-sessions.ts` 头注说的模块级 store 落地，" +
      "该一起收编，不是单独收这一个。",
  ],
  [
    "tabs.ts::explainBringFrontFailure",
    "`tabs.ts` 里的**模块级导出函数**（不是 `TabManager` 方法）⇒ 语法上就够不到 `this.tmuxCache`。" +
      "它只在**拉前失败之后**查一次用于分档诊断，happy path 零开销，取而不写代价可忽略。",
  ],
] as const;

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      walk(p, out);
      continue;
    }
    if (!name.endsWith(".ts")) continue;
    if (name.includes(".vitest.") || name.includes(".test.") || name.endsWith(".d.ts")) continue;
    out.push(p);
  }
  return out;
}

/** 剥掉整行注释 —— 本文件自己的头注里就写着 `list_remote_tmux`。 */
function stripLineComments(src: string): string {
  return src
    .split("\n")
    .filter((l) => {
      const t = l.trimStart();
      return !(t.startsWith("//") || t.startsWith("*") || t.startsWith("/*"));
    })
    .join("\n");
}

function fetchSites(): { file: string; count: number }[] {
  const out: { file: string; count: number }[] = [];
  for (const p of walk(SRC).sort()) {
    const src = stripLineComments(readFileSync(p, "utf8"));
    let n = 0;
    for (const re of PATTERNS) n += (src.match(re) ?? []).length;
    if (n > 0) out.push({ file: p.slice(SRC.length + 1).replace(/\\/g, "/"), count: n });
  }
  return out;
}

describe("tmux 会话列表的取数点（audit-0805 F14 第五刀，E3）", () => {
  it("★ 取数点一个都不许漏登记", () => {
    const sites = fetchSites();
    const total = sites.reduce((a, s) => a + s.count, 0);
    // 抽取器自检：扫不到东西时下面的对拍会两边都空、静默变绿。
    expect(
      total,
      `全仓只扫到 ${total} 个 list_remote_tmux 取数点（实测应为 5）—— 抽取器坏了`,
    ).toBeGreaterThanOrEqual(4);

    const exemptFiles = new Set(EXEMPT.map(([f]) => f.split("::")[0]));
    const unregistered = sites.filter(
      (s) => s.file !== "tabs.ts" && !exemptFiles.has(s.file),
    );
    expect(
      unregistered.map((s) => `${s.file}（${s.count} 处）`),
      "有 list_remote_tmux 取数点没登记。**这张表不是豁免清单** —— " +
        "先问它能不能走 `TabManager.fetchTmuxFresh`（取数与写缓存是同一件事）；" +
        "真够不到就在 EXEMPT 里写清为什么",
    ).toEqual([]);
  });

  it("★ TabManager 类内只剩一个取数点，且它就是写缓存的那一个", () => {
    const src = stripLineComments(readFileSync(join(SRC, "tabs.ts"), "utf8"));
    const fetches = (src.match(PATTERNS[0]) ?? []).length;
    const writes = (src.match(/this\.tmuxCache\.set\(/g) ?? []).length;

    expect(
      fetches,
      `tabs.ts 里有 ${fetches} 个取数点。允许两个：类内唯一的 \`fetchTmuxFresh\`，` +
        "外加模块级 `explainBringFrontFailure`（够不到私有缓存，已登记例外）。" +
        "多出来的那个 —— 它是不是本来就该走 `fetchTmuxFresh`？" +
        "★ 报告 I9′ 那条毛病（取而不写）就是这么来的：取数与写缓存是两件能分开做的事。",
    ).toBe(2);

    expect(
      writes,
      `tabs.ts 里有 ${writes} 处写缓存。收成一处之后就该恒为 1 —— ` +
        "多出一处意味着又有人在取数点之外单独写缓存了，「取数即写缓存」这条就不再成立",
    ).toBe(1);
  });

  it("每条例外都得说清为什么，不许只登记文件名", () => {
    for (const [file, why] of EXEMPT) {
      expect(why.length, `${file} 的例外理由太短，像是占位`).toBeGreaterThan(30);
      expect(
        /够不到|包装本体|另一个模块|模块级/.test(why),
        `${file} 的例外理由没说清它为什么走不了唯一取数点：「${why}」`,
      ).toBe(true);
    }
  });

  it("例外表不许长草 —— 登记的文件必须真的还在取数", () => {
    const sites = new Map(fetchSites().map((s) => [s.file, s.count]));
    for (const [entry] of EXEMPT) {
      const file = entry.split("::")[0];
      expect(sites.has(file), `例外表里的 ${file} 已经不取数了 —— 删掉这条，别留僵尸账`).toBe(
        true,
      );
    }
  });
});
