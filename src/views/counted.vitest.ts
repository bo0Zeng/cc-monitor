/**
 * `KR92D1` 第 ④ 刀的落点：**TS 侧把「不知道」当 `0` 参与排序或求和 ⇒ 必须红。**
 *
 * # 为什么这一族判据必须在这一端
 *
 * `KR92D1` 的失效方向逐字：🔴 **只改 Rust 侧不管 TS 侧** —— 那样类型是分开了，
 * 而**消费方仍旧把它读成 0**，「分得开」这句话在真正读它的那一端不成立。
 *
 * JS 会**安静地**帮你做那件事，三处各有一种做法：
 * `Number(null) === 0` · `null > 0 === false` · `null + 1 === 1`。
 * 前两处让「不知道」与「查过了是 0」在排序里同档，第三处让一次 star 操作把
 * 「不知道」变成一个看起来是真值的数。
 *
 * ⚠ 本文件**不判界面长什么样**（chip 文案 / 徽标是 `K-R66` 的面），只判
 * 「不知道」有没有被当成 0 拿去**排序 / 加减**。
 */
import { describe, it, expect } from "vitest";
import { liveRank, starRank, bumpCounted, isKnown } from "./counted";

describe("KR92D1 ④：不知道不是 0", () => {
  it("排序档：不知道自成一档，既不冒充「有」也不被当成「没有」", () => {
    // ★ 本条的牙：这两个 `not.toBe` 就是第 ④ 刀。把 `liveRank` 换回
    //   `Number(c)`（`Number(null) === 0`），「不知道」当场与「确定没活」同档 ⇒ 红。
    expect(liveRank(null)).not.toBe(liveRank(false));
    expect(starRank(null)).not.toBe(starRank(0));

    // 档位的顺序：确定有 > 不知道 > 确定没有。
    expect(liveRank(true)).toBeGreaterThan(liveRank(null));
    expect(liveRank(null)).toBeGreaterThan(liveRank(false));
    expect(starRank(3)).toBeGreaterThan(starRank(null));
    expect(starRank(null)).toBeGreaterThan(starRank(0));

    // `undefined`（旧后端 / 旧缓存整个缺这一格）与 `null` 同档 —— 都是「不知道」。
    expect(liveRank(undefined)).toBe(liveRank(null));
    expect(starRank(undefined)).toBe(starRank(null));
  });

  it("排序：一整趟排下来，不知道排在确定没活的前面、确定活着的后面", () => {
    // 用与 `history.ts::renderList` 逐字同一套比较式（那里就是这三行）。
    type P = { id: string; hasLive: boolean | null; starredCount: number | null };
    const cmp = (a: P, b: P) =>
      liveRank(b.hasLive) - liveRank(a.hasLive) ||
      starRank(b.starredCount) - starRank(a.starredCount);
    const rows: P[] = [
      { id: "确定没活", hasLive: false, starredCount: 0 },
      { id: "不知道", hasLive: null, starredCount: null },
      { id: "确定活着", hasLive: true, starredCount: 0 },
    ];
    expect(rows.slice().sort(cmp).map((r) => r.id)).toEqual([
      "确定活着",
      "不知道",
      "确定没活",
    ]);
  });

  it("求和：不知道加减之后还是不知道（一次 star 不许把它变成 1）", () => {
    // 🔴 第 ④ 刀的另一半：`null + 1 === 1`。写成 `proj.starredCount += 1` 就是这一形。
    expect(bumpCounted(null, +1)).toBeNull();
    expect(bumpCounted(undefined, +1)).toBeNull();
    expect(bumpCounted(null, -1)).toBeNull();
    // 算过了的那一档照常加减，并且不会变成负数（对齐原来的 `Math.max(0, n - 1)`）。
    expect(bumpCounted(2, +1)).toBe(3);
    expect(bumpCounted(0, -1)).toBe(0);
    expect(bumpCounted(1, -1)).toBe(0);
  });

  it("isKnown 分得开三档：算过的 0 是「算过的」", () => {
    expect(isKnown(0)).toBe(true);
    expect(isKnown(false)).toBe(true);
    expect(isKnown(null)).toBe(false);
    expect(isKnown(undefined)).toBe(false);
  });
});
