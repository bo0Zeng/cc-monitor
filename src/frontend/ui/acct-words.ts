/**
 * `quota-read` 回包的形状 ＋ 号名 · 语义位名那两个小读口。
 *
 * 额度的字全由核心写：每号几行 `rows`（`faces/quota_rows.rs`）· 一个语义位那一格 `slots[].text` / `tone`（`show.rs::slot_words`）·
 * 时刻 `…Text` 与距今 `…RelText`（出口那一遍）。这里只照抄，不按码取字、不算距今。
 */
import { copyText } from "./copy-table";
import type { LoginState } from "./generated/LoginState";
import type { QuotaKind } from "./generated/QuotaKind";
import type { QuotaState } from "./generated/QuotaState";
import type { SlotShow } from "./generated/SlotShow";

/** 行里的一格（核心写好的字 ＋ 语气）。 */
export interface RowCell {
  text: string;
  tone: "plain" | "fail" | "warn" | "need" | "now" | "busy";
}

/** 开窗那一判（quota-warm 照它办；界面不读）。 */
export interface Warm {
  act: "send" | "wait";
  at?: number;
  atText?: string;
  atRelText?: string;
  text: string;
}

/** `quota-read` 里一个出过数的号。 */
export interface QuotaReadAccount {
  agent: string;
  account: string;
  seenAt: number;
  seenAtText?: string;
  reading?: { resetsAt?: number; resetsAtText?: string; resetsAtRelText?: string };
  kind: QuotaKind;
  state: QuotaState;
  stale: boolean;
  limiting?: string;
  slots: SlotShow[];
  login: LoginState;
  subId?: string;
  /** 这个号那一段（首行 名 · 类型 · 标签；其余每行一组格）。 */
  rows: RowCell[][];
  warm: Warm;
}

/** `quota-read` 里账号库有、额度账上没出过数的号。 */
export interface QuotaReadUnseen {
  agent: string;
  account: string;
  kind: QuotaKind;
  login: LoginState;
  subId?: string;
  rows: RowCell[][];
  warm: Warm;
}

/** `quota-read` 的回包（`IPC-PROTOCOL.md` `quota-read` 那一节）。 */
export interface QuotaRead {
  state: "present" | "absent" | "unreadable";
  reason: string | null;
  /** 只在 `unreadable` 时有：复制详情（`reason` 那一句不带原话）。 */
  detail?: string | null;
  path: string | null;
  now: number;
  accounts: QuotaReadAccount[];
  unseen: QuotaReadUnseen[];
  usableNow: string[];
  earliestReturn: { account: string; at: number; atText?: string; atRelText?: string } | null;
  /** 读不出 / 一个号都没有时那一句。 */
  text?: string;
}

/** 号在界面上叫什么（`_` ＝ 起会话时没说是哪个号 ⇒ `~/.claude`）。 */
export function accountLabel(account: string): string {
  return account === "_" ? copyText("acct.home.name") : account;
}

/** 语义位的字（后端给 `5h` / `7d`；认不出的照原样）。 */
export function slotLabel(slot: string): string {
  if (slot === "5h") return copyText("acct.slot.fiveHour");
  if (slot === "7d") return copyText("acct.slot.sevenDay");
  return slot;
}

/** 一个语义位那一格的字（核心写好的 `slots[].text`）；这个号没有这一格 ⇒ `—`。 */
export function slotText(q: { slots: SlotShow[] }, slot: string): string {
  return q.slots.find((v) => v.slot === slot)?.text ?? copyText("acct.val.none");
}

/**
 * 一个号的 5h 那一格（恢复菜单里每个号后面那一格）：`5h 41%` · 卡着的照核心写的字（`5h ✕` …）。
 * 额度账读不出 ⇒ `5h 读不到`（原因与复制详情在账号面板「当前」那一条）。
 * 没问过 · 账上没有这个号、只在账号库里（没出过数）、按量号（没有分窗口）⇒ `null`（不出这一格）。
 */
export function fiveHourCell(r: QuotaRead | null | undefined, agent: string, account: string): string | null {
  if (!r) return null;
  if (r.state === "unreadable")
    return copyText("resumeMenu.account.quota", { slot: slotLabel("5h"), value: copyText("acct.val.unreadable") });
  const a = r.accounts.find((x) => x.account === account && x.agent === agent);
  if (!a || a.kind === "api") return null;
  return copyText("resumeMenu.account.quota", { slot: slotLabel("5h"), value: slotText(a, "5h") });
}
