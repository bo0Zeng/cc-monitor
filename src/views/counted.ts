/**
 * `K-R92`：**一个数「算过了」还是「不知道」** —— 前端这一端的读法。
 *
 * # 题面
 *
 * 后端 `remote_history.rs` 的 `Counted<T>{ Known(T), Unknown(WhyUnknown) }` 分得开
 * 「查过了是 0」与「压根不知道」。`K-R83` 落地时线上那一格（`HistoryProject`）还是
 * `u32`/`bool`，把它又压回去了；`K-R92` 把线上那一格改成 `Option` ⇒ 过线之后是 `null`。
 *
 * 🔴 **只改 Rust 侧不管这一端，「分得开」这句话在真正读它的那一端就不成立** ——
 * 类型是分开了，而消费方仍旧把 `null` 读成 `0`（`Number(null) === 0`、`null > 0 === false`、
 * `null + 1 === 1`，JS 这三处都会安静地帮你把「不知道」变成一个数）。
 * 本模块就是那三处的**唯一读法口**，判据住 `counted.vitest.ts`。
 *
 * ⚠ **本模块不管界面怎么显示「不知道」**（chip 文案 / 徽标）—— 那是 `K-R66` 的面。
 * `K-R92` 逐字只到数据层。
 */

/**
 * 一个可能没算过的数。`null` = **不知道**（后端的 `Unknown` 过线就是它）；
 * `T` = 算过了，就是这个值（**含真的是 0 / 真的没活**）。
 *
 * ⚠ `undefined` 一并当「不知道」收：旧后端 / 旧缓存的行可能整个缺这一格，
 * 而「字段不在」与「字段是 null」在这件事上是同一句话。
 */
export type Counted<T> = T | null | undefined;

/** 算过了没有。`true` ⇒ 后面那个值可以拿去算。 */
export function isKnown<T>(c: Counted<T>): c is T {
  return c !== null && c !== undefined;
}

/**
 * 三态排序档：**确定有(2) > 不知道(1) > 确定没有(0)**。
 *
 * # 🔴 为什么「不知道」在中间
 *
 * 排最前 = 冒充「活着」；排最后 = 断言「它比一个已经确定没活的项目更不像活着」。
 * 两句都没人查过。**说不出口的话不许说** ⇒ 它自成一档。
 *
 * ⚠ 与后端 `history.rs` 的 `live_rank` / `star_rank` 是同一套档位，两侧各有判据钉着。
 */
export function liveRank(c: Counted<boolean>): number {
  if (!isKnown(c)) return 1;
  return c ? 2 : 0;
}

/** 同 {@link liveRank}：**有星标(2) > 不知道(1) > 查过了一个都没有(0)**。 */
export function starRank(c: Counted<number>): number {
  if (!isKnown(c)) return 1;
  return c > 0 ? 2 : 0;
}

/**
 * 给一个数加减。**「不知道」加减之后还是「不知道」** —— 你不知道的数，加一还是不知道。
 *
 * 失效方向（本函数存在的理由）：`proj.starredCount += 1` 在 `starredCount` 是 `null` 时
 * 会算出 `1`，于是「不知道」被一次 star 操作**变成了一个看起来是真值的数**，
 * 而且从此再也回不去 —— 那正是本件在治的那一形，只是发生在前端。
 *
 * `floor` 对齐原来的 `Math.max(0, n - 1)`：计数不会是负的。
 */
export function bumpCounted(
  c: Counted<number>,
  delta: number,
  floor = 0,
): Counted<number> {
  if (!isKnown(c)) return null;
  return Math.max(floor, c + delta);
}
