/**
 * **tier-2 那张手写顶栏清单，与从 `main.ts` 派生出来的那一份，必须逐项相等。**
 *
 * # 🔴 它治的是一条真发生过、而且**没有任何门禁跑得到**的红
 *
 * 2026-09-21 现打逮到：`tests/e2e/tier2/test/shell-smoke.spec.mjs` 的 `TOPBAR` 数组
 * 仍列着 `.usage-trigger` 并断言它 `isClickable`，而那个入口在
 * （2026-09-18）里随整轴退役 ⇒ 生产里 **0 处**。
 * 那是一条**必然失败**的断言。
 *
 * **它为什么能静默活三天**：tier-2 这一档要**真 Windows ＋ WebView2 ＋ session-1 hop**
 * ⇒ 不在 `npm test`、不在 `cargo test`、不在 `tests/scripts/gate.sh` 里。
 *
 * 🔴 **而它的姊妹判据同拍就改对了** —— `tests/frontend/ui/topbar-icons.vitest.ts` 那条地板
 * 09-18 就 6 → 5 了。差别只有一个：**它的人群是从 `main.ts` 现场派生的，腐不了**；
 * 而 tier-2 那张是**手写**的。⇒ **一份派生、一份手写，分叉只会发生在手写那一份。**
 *
 * # 它买到什么
 *
 * 把那张手写表的**前提**拉进本地门禁：清单本身对不对，现在有人看着了。
 * ⇒ **那一档跑不到，但那张表不会再悄悄腐。**
 *
 * # ⚠ 它买不到什么（逐条写明，别把射程读宽一格）
 *
 * - **不买「tier-2 那一档真的跑过」** —— 它照旧要真 Windows。本条只对账清单。
 * - **不买那张表里 `.status-cmdk` 那一项** —— 它不是 `*-trigger`，派生器抽不到它
 *   （派生器的正则逐字是 `className = "([a-z-]*trigger)"`）。⇒ 它在下面被**显式登记**
 *   为「非 trigger 的那一项」，而那一行本身是手写的、**没有第二个源可对**。
 * - **不买「这些按钮在真浏览器里点得动」** —— 那正是 tier-2 那一档的活。
 * - **不买 `main.ts` 之外的顶栏入口** —— 派生器只读 `main.ts`。哪天顶栏搬家，
 *   派生器会零命中，而下面那条自检会红（不会零命中地绿）。
 */
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

function read(rel: string): string {
  return readFileSync(`${REPO_ROOT}/${rel}`, "utf8");
}

/**
 * 顶栏按钮的 class 全集 —— **从源码派生**，不是手写清单。
 *
 * ⚠ 这个派生法与 `tests/frontend/ui/topbar-icons.vitest.ts::triggerClasses` **刻意同形**：
 * 同一件事只有一种取法，否则两把尺子会各自漂。
 */
function derivedTriggers(main: string): string[] {
  return [...main.matchAll(/className = "([a-z-]*trigger)"/g)].map((m) => m[1]);
}

/** tier-2 那张手写表里**不是** `*-trigger` 的那几项 —— 派生器抽不到它们。 */
const NOT_A_TRIGGER = [".status-cmdk"];

/** 从 tier-2 那份 spec 里把 `TOPBAR` 数组的内容取出来。 */
function tier2Topbar(spec: string): string[] {
  const m = spec.match(/const TOPBAR = \[([^\]]*)\]/);
  if (!m) return [];
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
}

describe("P13 tier-2 顶栏清单与源码对账", () => {
  it("抽取器自己先要抽得到东西（否则下面两条零命中地绿）", () => {
    const derived = derivedTriggers(read("src/frontend/ui/main.ts"));
    const listed = tier2Topbar(read("tests/e2e/tier2/test/shell-smoke.spec.mjs"));
    // 🔴 反空真：两侧都必须非空。地板在这里是对的用法 ——
    //    它挡的是「抽取器坏了」，而「清单对不对」由下面那条**相等**断言管。
    expect(
      derived.length,
      "从 `src/frontend/ui/main.ts` 抽不到任何 `*-trigger` —— 命名约定变了，还是顶栏搬家了？\n" +
        "改了就把这里的派生器与 `tests/frontend/ui/topbar-icons.vitest.ts` 的那一份**一起**改。",
    ).toBeGreaterThan(0);
    expect(
      listed.length,
      "从 tier-2 那份 spec 里抽不到 `TOPBAR` 数组 —— 它改名或改形了？",
    ).toBeGreaterThan(0);
  });

  it("🔴 tier-2 手写的那张表，恰好等于「派生出来的 trigger ＋ 显式登记的非 trigger 项」", () => {
    const derived = derivedTriggers(read("src/frontend/ui/main.ts")).map((c) => `.${c}`);
    const listed = tier2Topbar(read("tests/e2e/tier2/test/shell-smoke.spec.mjs"));

    const want = [...derived, ...NOT_A_TRIGGER].sort();
    const got = [...listed].sort();

    const extra = got.filter((x) => !want.includes(x));
    const missing = want.filter((x) => !got.includes(x));

    // 🔴 **相等断言，不是地板** —— 地板在「变少」方向是瞎的，而这条红正是「多列了一个」。
    expect(
      got,
      "tier-2 那张手写顶栏表与源码对不上了。\n" +
        `  它多列了（生产里没有 ⇒ 那一档一跑必挂）：${JSON.stringify(extra)}\n` +
        `  它漏了（真有这个按钮而没人在真浏览器里验过）：${JSON.stringify(missing)}\n` +
        "⚠ **那一档跑不到本地门禁里**（要真 Windows ＋ WebView2 ＋ session-1 hop）\n" +
        "⇒ 这条红是它唯一的早期信号。改法：把 spec 里的 `TOPBAR` 与 `n 顶栏钮` 那两处一起改。",
    ).toEqual(want);
  });

  it("spec 头注与用例名里那个数，等于表里真的按钮数", () => {
    const spec = read("tests/e2e/tier2/test/shell-smoke.spec.mjs");
    const listed = tier2Topbar(spec);
    const buttons = listed.filter((x) => !NOT_A_TRIGGER.includes(x)).length;
    // 那两处散文各写一次这个数 ⇒ 它有两个住址，而住址多了就会漂。
    const claimed = [...spec.matchAll(/(\d+) 顶栏钮/g)].map((m) => Number(m[1]));
    expect(
      claimed.length,
      "spec 里找不到「n 顶栏钮」那句话 —— 它改措辞了？那本条从此零命中地绿。",
    ).toBeGreaterThan(0);
    for (const n of claimed) {
      expect(
        n,
        `spec 的散文说「${n} 顶栏钮」，而 \`TOPBAR\` 里真的按钮是 ${buttons} 个` +
          `（${JSON.stringify(listed)}，其中 ${JSON.stringify(NOT_A_TRIGGER)} 不算按钮）。\n` +
          "⚠ 这一形 2026-09-21 现打真的存在过：表里 7 项、散文写 6。",
      ).toBe(buttons);
    }
  });
});
