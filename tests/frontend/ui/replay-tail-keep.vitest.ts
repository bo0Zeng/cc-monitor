// @vitest-environment node
/**
 * 〔U3b · `设计/10` 步 8〕重放缓冲「只留尾巴」的条数**不是拍的**：它等于前端开一个 tab 时
 * 不滚动最多会建的条数（`tab-stream-view.ts`〔U2 前住 `tabs.ts`〕的 `materializeUntilFilled`：`MATERIALIZE_TAIL_K` × 轮数上限）。
 * 〔W5-RENDER R7〕轮数上限从循环里的字面量 `4` 挪成了静态常量 `MATERIALIZE_ROUNDS_PER_CALL`，语义随之收窄成
 * 「**一次同步调用**最多跑几轮」（补不满一屏时下一帧接着补，不再到此为止）—— 等式照旧：重放尾巴 == 首屏那一下的上限。
 * 本条现在抽常量的值，并核循环确实用的是它（恰好一处），两样缺一都红。
 * 两边各在自己的源码里（Rust 常量 / TS 静态字段 ＋ 循环上界），本条现读两边、对等式。
 *
 * 买到：改了任一边（比如把首屏物化改成 5 轮）而没回来想「重放尾巴够不够」⇒ 红。
 * **买不到**：那条等式本身对不对（依据是 `REPLAY_TAIL_KEEP` 头注里的读数，不是机检）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const read = (p: string) => readFileSync(resolve(REPO_ROOT, p), "utf8");

/** 恰好一处匹配，取第一组；0 处或多处都红（反空真：没抽到不许当成 NaN 比下去）。 */
function only(src: string, re: RegExp, what: string): number {
  const hits = [...src.matchAll(new RegExp(re, "g"))];
  expect(hits.length, `${what}：应恰好一处，实得 ${hits.length}`).toBe(1);
  return Number(hits[0][1]);
}

describe("〔U3b〕重放尾巴 == 首屏物化上限", () => {
  it("REPLAY_TAIL_KEEP == MATERIALIZE_TAIL_K × materializeUntilFilled 的轮数上限", () => {
    const rs = read("src/frontend/shell/src/event_replay.rs");
    // 〔U2 · 第三波〕物化那一族（`MATERIALIZE_TAIL_K` 与 `materializeUntilFilled`）随实时流视图从
    //   `src/frontend/ui/tabs.ts` 搬进了 `src/frontend/ui/tab-stream-view.ts`，两处写法逐字未动 ⇒ 这里只换住址，两条抽取正则一字不改。
    const ts = read("src/frontend/ui/tab-stream-view.ts");
    const keep = only(rs, /pub const REPLAY_TAIL_KEEP: usize = (\d+);/, "Rust 那一侧");
    const k = only(ts, /private static readonly MATERIALIZE_TAIL_K = (\d+);/, "MATERIALIZE_TAIL_K");
    const rounds = only(
      ts,
      /private static readonly MATERIALIZE_ROUNDS_PER_CALL = (\d+);/,
      "MATERIALIZE_ROUNDS_PER_CALL",
    );
    const body = ts.slice(ts.indexOf("private materializeUntilFilled("));
    const loop = body.slice(0, body.indexOf("\n  }\n"));
    const uses = [...loop.matchAll(/for \(let round = 0; round < TabStreamView\.MATERIALIZE_ROUNDS_PER_CALL; round\+\+\)/g)];
    expect(uses.length, "materializeUntilFilled 的循环没用 MATERIALIZE_ROUNDS_PER_CALL 当上界（恰好一处）").toBe(1);
    expect(keep).toBe(k * rounds);
  });
});
