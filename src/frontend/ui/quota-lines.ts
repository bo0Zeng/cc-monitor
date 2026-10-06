/**
 * 额度的**行模型**：`quota-read` 一份回包 ＋ 此刻 ＋ 时区偏移 ⇒ 每号几行（悬停卡那几行）。
 *
 * - 只排版、不判：「快满 · 被拒 · 超额在兜 · 上一窗已过 · 数旧 · 卡人的窗口」都是回包里后端给的词，这里照词选字。
 * - 终端 `--text`（后端 CLI 那个口旁边的 `quota_text.rs`）排同一份回包；两边锁同一份金样
 *   `tests/__fixtures__/quota-text.golden.json`，逐字对拍 —— 想改写法先改金样。
 * - 时刻写法（当天 `HH:MM` · 非当天 `MM-DD HH:MM` · 非当年 `YYYY-MM-DD HH:MM` · 距今 `+1h50m` / `+3d`）只住
 *   [`fmtAt`] / [`fmtRel`]；会话那几行（`⇄ 14:20 ← work`）也用它们。
 */
import { copyText } from "./copy-table";
import type { LoginState } from "./generated/LoginState";
import type { QuotaKind } from "./generated/QuotaKind";
import type { QuotaState } from "./generated/QuotaState";
import type { SlotShow } from "./generated/SlotShow";

/** `quota-read` 里一个出过数的号（只取排版用得着的几格）。 */
export interface QuotaReadAccount {
  agent: string;
  account: string;
  seenAt: number;
  reading?: { resetsAt?: number };
  kind: QuotaKind;
  state: QuotaState;
  stale: boolean;
  limiting?: string;
  slots: SlotShow[];
  login: LoginState;
  subId?: string;
}

/** `quota-read` 里账号库有、额度账上没出过数的号。 */
export interface QuotaReadUnseen {
  agent: string;
  account: string;
  kind: QuotaKind;
  login: LoginState;
  subId?: string;
}

/** `quota-read` 的回包（`IPC-PROTOCOL.md` `quota-read` 那一节）。 */
export interface QuotaRead {
  state: "present" | "absent" | "unreadable";
  reason: string | null;
  path: string | null;
  now: number;
  accounts: QuotaReadAccount[];
  unseen: QuotaReadUnseen[];
  usableNow: string[];
  earliestReturn: { account: string; at: number } | null;
}

/** 一个号那一段：首行 `名 类型 [标签]`，其余每行一组格（键 · 值 · ↻ · 距今，缺的格不出）。 */
export interface QuotaBlock {
  agent: string;
  account: string;
  rows: string[][];
}

const DAY = 86_400;

/** 天数（自 1970-01-01）⇒ 年月日（Howard Hinnant 的 civil_from_days）。 */
function civil(days: number): { y: number; m: number; d: number } {
  const z = days + 719_468;
  const era = Math.floor(z / 146_097);
  const doe = z - era * 146_097;
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36_524) - Math.floor(doe / 146_096)) / 365);
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  return { y: era * 400 + yoe + (m <= 2 ? 1 : 0), m, d };
}

const two = (n: number): string => String(n).padStart(2, "0");

/** 一个时刻（unix 秒）按此刻与时区偏移（分钟，东正）写：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年 `YYYY-MM-DD HH:MM`。 */
export function fmtAt(t: number, now: number, tzOffsetMin: number): string {
  const local = t + tzOffsetMin * 60;
  const here = now + tzOffsetMin * 60;
  const day = Math.floor(local / DAY);
  const today = Math.floor(here / DAY);
  const secs = local - day * DAY;
  const hm = `${two(Math.floor(secs / 3600))}:${two(Math.floor((secs % 3600) / 60))}`;
  if (day === today) return hm;
  const a = civil(day);
  const md = `${two(a.m)}-${two(a.d)} ${hm}`;
  return a.y === civil(today).y ? md : `${a.y}-${md}`;
}

/** 距今（只写未来）：`+12m` · `+1h50m` · `+2h` · `+3d`（满 24h 只写天）；已过 ⇒ `null`。分钟向上取整。 */
export function fmtRel(t: number, now: number): string | null {
  const d = t - now;
  if (d <= 0) return null;
  if (d >= DAY) return `+${Math.floor(d / DAY)}d`;
  const mins = Math.ceil(d / 60);
  if (mins < 60) return `+${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? `+${h}h` : `+${h}h${m}m`;
}

/** 时长（秒）：`45s` · `6m` · `1h50m` · `2h` · `3d`（满 24h 只写天）。负数当 0。 */
export function fmtDur(secs: number): string {
  const d = Math.max(0, Math.floor(secs));
  if (d < 60) return `${d}s`;
  if (d >= DAY) return `${Math.floor(d / DAY)}d`;
  const mins = Math.floor(d / 60);
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? `${h}h` : `${h}h${m}m`;
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

/** 首行：名 · 类型 · 没指定号 / 登录拿不到时的标签。 */
function head(account: string, kind: QuotaKind, login: LoginState): string[] {
  const row = [accountLabel(account), kind === "api" ? copyText("acct.kind.api") : copyText("acct.kind.sub")];
  if (account === "_") row.push(copyText("acct.tag.home"));
  if (login === "needsLogin") row.push(copyText("acct.tag.login"));
  if (login === "needsKey") row.push(copyText("acct.tag.key"));
  return row;
}

/** 重置那一格与距今那一格（没给时刻 ⇒ 都不出；已过 ⇒ `↻.. 已过`、没有距今）。 */
function resetCells(at: number | undefined, now: number, tz: number): string[] {
  if (at === undefined) return [];
  const s = fmtAt(at, now, tz);
  const rel = fmtRel(at, now);
  return rel === null ? [copyText("acct.reset.past", { at: s })] : [copyText("acct.reset.at", { at: s }), rel];
}

/** 一个语义位那一行。卡着的窗口按显示态换字（被拒 `✕` · 超额在兜 `超额` · 上一窗已过 `—`）。 */
function slotRow(a: QuotaReadAccount, slot: string, now: number, tz: number): string[] {
  const s = a.slots.find((x) => x.slot === slot);
  if (!s) return [slotLabel(slot), copyText("acct.val.none")];
  const here = a.limiting === slot;
  let value: string;
  if (here && a.state === "refused") value = copyText("acct.val.refused");
  else if (here && a.state === "overageInUse") value = copyText("acct.val.over");
  else if (here && a.state === "resetSinceSeen") value = copyText("acct.val.none");
  else if (s.pct === undefined) value = copyText("acct.val.none");
  else if (s.pct > 100) value = copyText("acct.val.refused");
  else value = copyText("acct.val.pct", { pct: s.pct });
  return [slotLabel(slot), value, ...resetCells(s.resetsAt, now, tz)];
}

/** 被拒 / 被拒过、却没有分窗口的数可画（按量号 · 被拒时回包没带那一族头）⇒ `状态` 一行。 */
function stateRow(a: QuotaReadAccount, now: number, tz: number): string[] | null {
  if (a.state !== "refused" && a.state !== "resetSinceSeen") return null;
  const value = a.state === "refused" ? copyText("acct.val.refused") : copyText("acct.val.none");
  return [copyText("acct.row.state"), value, ...resetCells(a.reading?.resetsAt, now, tz)];
}

/** 采样那一行：几点 · 哪台；数旧 ⇒ 几点 · 旧。 */
function seenRow(at: number, stale: boolean, now: number, tz: number, machine: string): string[] {
  const s = fmtAt(at, now, tz);
  return [copyText("acct.row.seen"), stale ? copyText("acct.seen.staleShort", { at: s }) : copyText("acct.seen.at", { at: s, machine })];
}

/** 按量号没有分窗口的限额：`5h/7d — 无限额`。 */
function noLimitRow(): string[] {
  return [copyText("acct.slot.both"), copyText("acct.val.none"), copyText("acct.val.noLimit")];
}

/** 出过数的号那一段。 */
export function seenBlock(a: QuotaReadAccount, now: number, tz: number, machine: string): QuotaBlock {
  const rows = [head(a.account, a.kind, a.login)];
  const windowed = a.limiting !== undefined && a.slots.some((s) => s.slot === a.limiting);
  if (a.kind === "api") {
    rows.push(stateRow(a, now, tz) ?? noLimitRow());
  } else {
    rows.push(slotRow(a, "5h", now, tz), slotRow(a, "7d", now, tz));
    const st = windowed ? null : stateRow(a, now, tz);
    if (st) rows.push(st);
    rows.push(
      a.state === "overageInUse"
        ? [copyText("acct.row.over"), copyText("acct.val.overOn"), copyText("acct.val.overBilled")]
        : [copyText("acct.row.over"), copyText("acct.val.none")],
    );
  }
  rows.push(seenRow(a.seenAt, a.stale, now, tz, machine));
  return { agent: a.agent, account: a.account, rows };
}

/** 没出过数的号那一段：`— 无采样`（按量号 `5h/7d — 无采样`）。 */
export function unseenBlock(u: QuotaReadUnseen): QuotaBlock {
  const none = copyText("acct.val.none");
  const unseen = copyText("acct.seen.none");
  const rows = [head(u.account, u.kind, u.login)];
  if (u.kind === "api") rows.push([copyText("acct.slot.both"), none, unseen]);
  else rows.push([slotLabel("5h"), none, unseen], [slotLabel("7d"), none]);
  return { agent: u.agent, account: u.account, rows };
}

/** ★ 整份回包 ⇒ 每号一段（先出过数的、再没出过的，各按回包的次序）。读不出 ⇒ 空（调用方另说「读取失败」）。 */
export function quotaBlocks(r: QuotaRead, tzOffsetMin: number, machine: string): QuotaBlock[] {
  if (r.state === "unreadable") return [];
  return [
    ...r.accounts.map((a) => seenBlock(a, r.now, tzOffsetMin, machine)),
    ...r.unseen.map((u) => unseenBlock(u)),
  ];
}

/**
 * 一个号的 5h 那一格（恢复菜单里每个号后面那一格）：`5h 41%` · 卡着的照显示态换字（`5h ✕` …）。
 * 账上没有这个号、只在账号库里（没出过数）、按量号（没有分窗口）⇒ `null`（不出这一格）。
 */
export function fiveHourCell(r: QuotaRead | null | undefined, agent: string, account: string): string | null {
  if (!r || r.state === "unreadable") return null;
  const a = r.accounts.find((x) => x.account === account && x.agent === agent);
  if (!a || a.kind === "api") return null;
  const [slot, value] = slotRow(a, "5h", r.now, 0);
  return copyText("resumeMenu.account.quota", { slot, value });
}
