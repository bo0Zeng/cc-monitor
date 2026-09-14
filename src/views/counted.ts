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

/**
 * `Counted<T>` 的**产出侧**形状：「不知道」一律规范成 `null`，`undefined` **产不出来**。
 *
 * # 🔴 为什么入口与出口不是同一个类型
 *
 * 这条不对称就是 `K-R118` 那 6 条 `TS2322` 的病灶（缺陷引入于 `K-R92` `e707acd`，
 * 09-12 进来、09-14 才被「真去编一次发版产物」逮到）。**两侧各有各的现打理由**：
 *
 * - **入口宽是对的**：`Counted<T>` 连 `undefined` 一起收，理由逐字写在它自己头上
 *   （旧后端 / 旧缓存的行可能整个缺这一格）；而且这件事**有判据在守** ——
 *   `counted.vitest.ts` 里 `liveRank(undefined)` / `starRank(undefined)` /
 *   `bumpCounted(undefined, +1)` 三处逐字喂的就是 `undefined`。
 *   ⇒ 把 `undefined` 从 `Counted<T>` 里删掉是最省事的一刀，而它删的是那条被守着的入口语义。
 * - **出口必须窄**：线上那几格（`HistoryProject.starredCount` / `hiddenCount`，
 *   由 `ts-rs` 从 Rust `Option<u32>` 生成）逐字是 `T | null` —— **一个 `undefined` 都装不下**，
 *   而那份 `.ts` 是生成物（门禁 `generated` 那一格盯着它，改不得也不该改）。
 *   而 {@link bumpCounted} 的函数体本来就只走 `null` 与 `number` 两条返回路径 ——
 *   它承诺过一个自己**从来产不出、也没人接得住**的 `undefined`。
 *
 * ⇒ 该改的既不是入口那个类型、也不是线上那个字段，是**返回位**。
 *
 * ⚠ 它由 `Counted<T>` **派生**，不另写一份 `T | null` 的字面量：`Counted<T>` 哪天改了，
 * 本别名自动跟着改，不会长成第二份要人去同步的定义。
 */
export type CountedOut<T> = Exclude<Counted<T>, undefined>;

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
 *
 * ⚠ **入参 `Counted`、返回 `CountedOut`，这条不对称是刻意的**（`K-R118`）：
 * 收得宽（旧行可能整个缺这一格），产得窄（线上那几格是 `T | null`，装不下 `undefined`），
 * 而本函数的两条返回路径本来就只产 `null` 与 `number`。理由全文住 {@link CountedOut}。
 */
export function bumpCounted(
  c: Counted<number>,
  delta: number,
  floor = 0,
): CountedOut<number> {
  if (!isKnown(c)) return null;
  return Math.max(floor, c + delta);
}
