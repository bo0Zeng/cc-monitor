/**
 * **起会话用哪个号 —— 界面这一侧只说要什么**（跟随 / 账号 0 / 用户点名）。判定住会话所在那台的后端（`session-open` 的 `account` 那一格）：
 * 跟随 ＝ 这条会话上次用的号 → 那台的默认号 → 不指定；选不了 ⇒ 那台不起、回 `account_unavailable` 带替代，那一句也是那台写的。
 */
import type { AccountAsk } from "./generated/AccountAsk";
import type { AccountUnavailable } from "./generated/AccountUnavailable";

export type { AccountAsk, AccountUnavailable };

/**
 * **用户点名的那一个号**（菜单里点的 · 选择框里点的 · 分叉沿用源会话的，那台推出的名字）—— 线上 `named` 只从这里出。
 * `null` = 账号 0；名字 ⇒ 按名字（那台判它选不选得了）。
 */
export function chosenAccount(name: string | null): AccountAsk {
  return name === null ? { kind: "base" } : { kind: "named", name };
}
