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
import type { Usage } from "./generated/Usage";

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
  /** 用量那一格（窗口 · 值 · 重置 · 连成的一句 · 语气，核心写）。 */
  usage: Usage;
  /** 这个号那一段（首行 名 · 类型 · 标签；其余每行一组格）。 */
  rows: RowCell[][];
  warm: Warm;
  /** 「5h 那一格」写好的字（`5h 41%` …）；按量号 ⇒ `null`。 */
  fiveHour: string | null;
}

/** `quota-read` 里账号库有、额度账上没出过数的号。 */
export interface QuotaReadUnseen {
  agent: string;
  account: string;
  kind: QuotaKind;
  login: LoginState;
  subId?: string;
  /** 用量那一格（没出过数：`5h —` / `按量`；没有重置那一格）。 */
  usage: Omit<Usage, "reset">;
  rows: RowCell[][];
  warm: Warm;
  /** 没出过数 ⇒ 恒 `null`（不出那一格）。 */
  fiveHour: null;
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
  /** 读不出 ⇒「5h 那一格」写好的字（`5h 读不到`）；否则 `null`（各号自己那一格）。 */
  fiveHour: string | null;
}

/** 核心写好的号名 / 位名（成品上的 `names`，`accounts/quota/name_words.rs`）：`accounts` 只列与原名不同的号，`slots` 列每个语义位。 */
export interface Names {
  accounts: Record<string, string>;
  slots: Record<string, string>;
}

/** 最近一份成品带来的那张（出号名的四件成品 `quota-read` · `rotation-rules-read` · `rotation-session-read` · `rotation-plan` 解码时交进来）。 */
let names: Names = { accounts: {}, slots: {} };

/** 解码器交进来成品上的 `names`（形状不对 ⇒ 不收、留着上一份）。 */
export function takeNames(v: unknown): void {
  if (typeof v !== "object" || v === null) return;
  const { accounts, slots } = v as Partial<Names>;
  if (typeof accounts !== "object" || accounts === null || typeof slots !== "object" || slots === null) return;
  names = { accounts, slots };
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
