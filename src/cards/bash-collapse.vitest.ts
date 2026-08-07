// 〔audit-0805 08-06〕**bash 输出卡的「超长折叠」是一条用户看得见的承诺，而它此前零覆盖。**
//
// # 怎么发现的
//
// 全局变异抽样（Phase G 的那一项）打到前端：把 `cards/bash.ts` 的
// `OUTPUT_COLLAPSE_LINES` 30→3、`OUTPUT_HEAD_LINES` 20→2，
// **`npm test` 17/17 与 `npx vitest run` 1273/1273 全绿** —— 两个门禁一起放过。
//
// 同批四条里另两条（`DEFAULT_MAX_LINES` / `BLOCK_LOOKAHEAD_LINES`）**当场被逮**，
// 所以这不是「前端没有测试」，是**折叠这一支恰好长在没人去的那一层**：
// `bash.test.ts` 是 node 纯函数套件，而 `bash.ts` 自己写着
// 「DOM 渲染（下面依赖 document，node 纯函数测试不要碰）」—— 于是没有任何 vitest 接手。
//
// # 为什么不直接断言 30 和 20
//
// 那会让判据成为那两个数的**第二份副本**：改常量时判据跟着改，等于没有判据
// （`doc_claim_registry` 的头注记着同一个坑，`STATUS_CELLS` 第一版就是这么失效的）。
//
// ⇒ 本文件用**夹逼锚点**：不写阈值，只钉两端明显该成立的性质 ——
// 「明显短」的输出不许被折叠 · 「明显长」的输出必须折叠、且头部要**够长**。
// 30→3 会让前者红，20→2 会让后者红，而合法地微调阈值（比如 30→28）不会误红。
import { describe, it, expect } from "vitest";
import { buildBashOutputCard } from "./bash";

const ts = "2026-08-06T00:00:00.000Z";
const fmt = () => "00:00";
const linesOf = (n: number) =>
  Array.from({ length: n }, (_, i) => `line-${i + 1}`).join("\n");

/** 折叠时才会出现的两个标记（沿 `block-body-show-full` 惯例）。 */
const WRAP = ".block-body-truncated-wrap";
const EXPAND = ".block-body-show-full";

describe("bash 输出卡：超长折叠（夹逼锚点，不复制阈值）", () => {
  it("★ 明显短的输出（5 行）不许被折叠 —— 折叠阈值被调得过小时这条红", () => {
    const card = buildBashOutputCard({ stdout: linesOf(5), stderr: "" }, ts, fmt);
    expect(card.querySelector(WRAP), "5 行就折叠 = 阈值被调小了").toBeNull();
    expect(card.querySelector(EXPAND)).toBeNull();
    // 全文要在，一行都不许少。
    expect(card.textContent).toContain("line-1");
    expect(card.textContent).toContain("line-5");
  });

  it("★ 明显长的输出（200 行）必须折叠，且给出展开入口", () => {
    const card = buildBashOutputCard({ stdout: linesOf(200), stderr: "" }, ts, fmt);
    expect(card.querySelector(WRAP), "200 行都不折叠 = 折叠这一支断了").not.toBeNull();
    expect(card.querySelector(EXPAND), "折叠了却没有展开入口 = 内容被吃掉").not.toBeNull();
  });

  it("★ 折叠后展示的头部要够长（≥10 行）—— 头部行数被调得过小时这条红", () => {
    const card = buildBashOutputCard({ stdout: linesOf(200), stderr: "" }, ts, fmt);
    const pre = card.querySelector(`${WRAP} pre`);
    expect(pre, "折叠壳里没有 pre").not.toBeNull();
    const shown = (pre?.textContent ?? "").split("\n").filter((l) => l.length > 0);
    // 下界：10 行。它远低于今天的取值，合法微调不会误红；
    // 上界：必须真的比全文少，否则「折叠」名存实亡。
    expect(shown.length, `折叠后只展示了 ${shown.length} 行，太少`).toBeGreaterThanOrEqual(10);
    expect(shown.length, "折叠后展示的行数没有比全文少").toBeLessThan(200);
    // 头部要是**开头那几行**，不是随便截的一段。
    expect(shown[0]).toBe("line-1");
  });

  it("★ 反向自检：夹具真的走到了两条分支（否则上面三条可能都在同一支上空转）", () => {
    const short = buildBashOutputCard({ stdout: linesOf(5), stderr: "" }, ts, fmt);
    const long = buildBashOutputCard({ stdout: linesOf(200), stderr: "" }, ts, fmt);
    // 一个有壳、一个没有 —— 两条分支各被走到一次。
    expect(short.querySelector(WRAP)).toBeNull();
    expect(long.querySelector(WRAP)).not.toBeNull();
  });
});
