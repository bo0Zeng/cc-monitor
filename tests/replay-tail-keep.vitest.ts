// @vitest-environment node
/**
 * 〔U3b · `设计/10` 步 8〕重放缓冲「只留尾巴」的条数**不是拍的**：它等于前端开一个 tab 时
 * 不滚动最多会建的条数（`tabs.ts` 的 `materializeUntilFilled`：`MATERIALIZE_TAIL_K` × 轮数上限）。
 * 两边各在自己的源码里（Rust 常量 / TS 静态字段 ＋ 循环上界），本条现读两边、对等式。
 *
 * 买到：改了任一边（比如把首屏物化改成 5 轮）而没回来想「重放尾巴够不够」⇒ 红。
 * **买不到**：那条等式本身对不对（依据是 `REPLAY_TAIL_KEEP` 头注里的读数，不是机检）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

const read = (p: string) => readFileSync(resolve(REPO_ROOT, p), "utf8");

/** 恰好一处匹配，取第一组；0 处或多处都红（反空真：没抽到不许当成 NaN 比下去）。 */
function only(src: string, re: RegExp, what: string): number {
  const hits = [...src.matchAll(new RegExp(re, "g"))];
  expect(hits.length, `${what}：应恰好一处，实得 ${hits.length}`).toBe(1);
  return Number(hits[0][1]);
}

describe("〔U3b〕重放尾巴 == 首屏物化上限", () => {
  it("REPLAY_TAIL_KEEP == MATERIALIZE_TAIL_K × materializeUntilFilled 的轮数上限", () => {
    const rs = read("src/bridge/src/event_replay.rs");
    const ts = read("src/tabs.ts");
    const keep = only(rs, /pub const REPLAY_TAIL_KEEP: usize = (\d+);/, "Rust 那一侧");
    const k = only(ts, /private static readonly MATERIALIZE_TAIL_K = (\d+);/, "MATERIALIZE_TAIL_K");
    const body = ts.slice(ts.indexOf("private materializeUntilFilled("));
    const rounds = only(
      body.slice(0, body.indexOf("\n  }\n")),
      /for \(let round = 0; round < (\d+); round\+\+\)/,
      "materializeUntilFilled 的轮数上限",
    );
    expect(keep).toBe(k * rounds);
  });
});
