/**
 * 号名 · 语义位名那两个小读口 ＋ 每号那一格「5h」（`quota-read` 回包的类型从 Rust 生成，`cmd-wire::quota`）。
 *
 * 额度的字全由核心写：每号几行 `rows`（`faces/quota_rows.rs`）· 一个语义位那一格 `slots[].text` / `tone`（`show.rs::slot_words`）·
 * 时刻 `…Text` 与距今 `…RelText`（出口那一遍）。这里只照抄，不按码取字、不算距今。
 */
import { copyText } from "./copy-table";
import type { QuotaNames } from "./generated/QuotaNames";
import type { QuotaRead } from "./generated/QuotaRead";
import type { SlotShow } from "./generated/SlotShow";

export type { QuotaRead } from "./generated/QuotaRead";
export type { QuotaAccount } from "./generated/QuotaAccount";

/** 最近一份成品带来的那张（`quota-read` 收下时交进来）。 */
let names: QuotaNames = { accounts: {}, slots: {} };

/** 收下成品上的 `names`（形状由握手时的指纹核过）。 */
export function takeNames(v: QuotaNames): void {
  names = v;
}

/** 号在界面上叫什么：照核心那张表；不在表里的号就叫它自己的名字。 */
export function accountLabel(account: string): string {
  return names.accounts[account] ?? account;
}

/** 语义位的字：照核心那张表；不在表里的照原样。 */
export function slotLabel(slot: string): string {
  return names.slots[slot] ?? slot;
}

/** 一个语义位那一格的字（核心写好的 `slots[].text`）；这个号没有这一格 ⇒ `—`。 */
export function slotText(q: { slots: SlotShow[] }, slot: string): string {
  return q.slots.find((v) => v.slot === slot)?.text ?? copyText("acct.val.none");
}

/**
 * 一个号的 5h 那一格（恢复菜单 · 新会话框每个号后面那一格）：核心写好的字（`faces/quota_rows.rs::with_rows`）。
 * 额度账读不出 ⇒ 顶上那一格（`5h 读不到`；原因与复制详情在账号面板「当前」那一条）。
 * 没问过 · 账上没有这个号 · 按量号 ⇒ `null`（不出这一格）。
 */
export function fiveHourCell(r: QuotaRead | null | undefined, agent: string, account: string): string | null {
  if (!r) return null;
  return r.fiveHour ?? r.accounts.find((x) => x.account === account && x.agent === agent)?.fiveHour ?? null;
}
