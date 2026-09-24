/**
 * 〔C4a · `设计/05 §8` 步 2 的 TS 那一半〕**「这一趟问的是哪台机器」在 TS 侧的唯一判定处。**
 *
 * # 它治什么
 *
 * 步 2 之前 TS 侧「本机」有**三种**写法：`null`（tab / 设置共用 store / 分叉流…）·
 * `"<local>"`（`backend-policy.ts::LOCAL_ORIGIN`，与 Rust 跨语言对拍）· `"__local__"`
 * （`accounts.ts` 自己那个同名常量）。三种写法之间靠散在各处的 `?? LOCAL_ORIGIN` /
 * `origin === null` / `origin ? … : …` 互相翻译 —— 而 `"<local>"` 是**真值**，
 * 一处真值判断没改就是「本机被当成远端」，`tsc` 一个字都不会说。
 *
 * ⇒ 从本文件起只剩一种：**本机 = `LOCAL_ORIGIN`（`"<local>"`）**，是一个具名的 origin，
 * 不是「没说」（`INVARIANTS §40`：本地 ＝ 不走 ssh 的远端）。
 * 「是不是本机」只在这里判（[`isLocalOrigin`] / [`isRemoteOrigin`]），调用处不许自己比。
 *
 * # 那个值不住这里
 *
 * `LOCAL_ORIGIN` 的家仍是 `backend-policy.ts`（Rust 侧 `origin.rs::LOCAL` /
 * `inbound_client::LOCAL_ORIGIN` 由 `origin_tests.rs::the_sentinel_agrees_with_the_two_existing_homes`
 * 逐字节对拍的正是那一行）。本文件只**转出**它，不再写一份字面量。
 *
 * # ⚠ 射程：出方向那一半
 *
 * Rust **出方向**的载荷（`JsonlLinePayload` / `HistoryProject` / `HistorySessionEntry` /
 * `SessionHits` 一族）今天仍是 `Option<String>`：缺省 = 本机。那几份住
 * `src/bridge/src/{bridge,history,search}.rs`，不在步 2 的写区里。
 * ⇒ 它们进 TS 的那一下由 [`originFromWire`] 收成一个表示 —— **全仓唯一**一处表示法转换。
 */
import { LOCAL_ORIGIN } from "../backend-policy";
import type { Origin } from "../generated/Origin";

export { LOCAL_ORIGIN };
export type { Origin };

/** 这是本机那台吗。 */
export function isLocalOrigin(origin: Origin): boolean {
  return origin === LOCAL_ORIGIN;
}

/** 这是一台远端吗（= 不是本机）。 */
export function isRemoteOrigin(origin: Origin): boolean {
  return origin !== LOCAL_ORIGIN;
}

/**
 * Rust **出方向**线上值 ⇒ `Origin`。缺省（`undefined`）= 本机 —— 那是 Rust 那侧
 * `skip_serializing_if = "Option::is_none"` 的线上形状，不是「没说」。
 *
 * ⚠ 只收 `undefined`，**不收 `null`**：出方向那一族在 TS 类型上全是 `origin?: string`；
 * 收 `null` 就等于在这里替一个已经不存在的旧形留门（`no-legacy-compat`）。
 */
export function originFromWire(wire: string | undefined): Origin {
  return wire ?? LOCAL_ORIGIN;
}
