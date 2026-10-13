/**
 * 额度与账号几处界面的**排版模型**（纯函数）：状态栏那颗按钮 · 它的悬停卡。
 *
 * 输入全是后端的成品（`rotation-view` 一个会话那一份：开会话的号 `start` · 在用的号 `current` · 换号记录 `log`；
 * `quota-read` 那台一份）；这里只按词选字、拼格，不判「能不能换 / 该不该换 / 快满没有」。号那一段照 `quota-read` 每号的 `rows`。
 */
import { copyText } from "./copy-table";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { accountLabel, type QuotaRead, type QuotaReadAccount } from "./acct-words";
import type { Usage } from "./generated/Usage";
import type { SessionRotationEntry } from "./app-store";

/** 采样那台在界面上叫什么。 */
export function machineLabel(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("acct.machine.here") : origin;
}

/** 按钮那一格的语气（核心写的 `usage.tone`）。 */
export type ChipTone = Usage["tone"];

/** 状态栏那颗按钮这一刻画什么。 */
export interface ChipModel {
  /** 头像按哪个号取色（`_` ＝ 没指定号；`null` ＝ 不知道是哪个号：中转没见过、归属也没查到）。 */
  account: string | null;
  name: string;
  swapped: boolean;
  /** 窗口那一格（`5h`；按量号没有）。 */
  window: string | null;
  /** 用量那一格（`63%` · `✕` · `超额` · `—` · `按量`）；没走中转 ⇒ `null`（只画名字）。 */
  value: string | null;
  /** 被拒时几点重置（`↻19:00`）。 */
  reset: string | null;
  tone: ChipTone;
  stale: boolean;
}

/** 这台额度账里这个号那一条（这个会话那一家的、或只认号名的头一条）。 */
function ledgerOfAny(quota: QuotaRead | null, account: string): QuotaReadAccount | undefined {
  return quota?.accounts.find((a) => a.account === account);
}

/**
 * ★ 本会话那颗按钮。`entry` ＝ 这个会话在它那台的轮换（还没问到 ⇒ `undefined`；中转没见过 ⇒ `read: null`）；`fallback` ＝ tab 栏徽章那份
 * 快照里这个会话归属的号（中转没见过这个会话时只能画它）；都没有 ⇒ `null`（不画）。用量那一格照 `quota-read` 里在用的号那一条。
 */
export function sessionChip(entry: SessionRotationEntry | undefined, fallback: string | null, quota: QuotaRead | null): ChipModel | null {
  if (!entry) {
    if (fallback === null) return null;
    return { account: fallback, name: accountLabel(fallback), swapped: false, window: null, value: null, reset: null, tone: "plain", stale: false };
  }
  const read = entry.read;
  if (read === null) {
    // 中转没见过这个会话：有归属就画那个号（无用量）；归属也不知道 ⇒ 照实写「未实时显示」。
    const name = fallback === null ? copyText("acct.hover.noRelay") : accountLabel(fallback);
    return { account: fallback, name, swapped: false, window: null, value: null, reset: null, tone: "plain", stale: false };
  }
  const cur = read.current;
  const led = ledgerOfAny(quota, cur);
  const u = led?.usage;
  return {
    account: cur,
    name: accountLabel(cur),
    swapped: cur !== read.start,
    window: u?.window ?? null,
    value: u?.value ?? null,
    reset: u?.reset ?? null,
    tone: u?.tone ?? "plain",
    stale: led?.stale ?? false,
  };
}

/**
 * ★ 本会话悬停卡的行：那个号在 `quota-read` 里那一段（与金样同一个行模型）；轮换换过号 ⇒ 末尾一行「从哪个号换过来」（开会话的号）。
 * 中转没见过这个会话 ⇒ `用量 — 未实时显示` · `切换 仅重启切换`。
 */
export function sessionHoverRows(entry: SessionRotationEntry | undefined, fallback: string | null, quota: QuotaRead | null): string[][] | null {
  const read = entry?.read;
  if (!read) {
    if (fallback === null) return entry ? [[copyText("acct.hover.noRelay")], [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")]] : null;
    const known = quota?.accounts.find((a) => a.account === fallback) ?? quota?.unseen.find((u) => u.account === fallback);
    // 首行照核心那一段的头两格（名 · 类型）。
    const head = [accountLabel(fallback)];
    const kind = known?.rows[0]?.[1]?.text;
    if (kind !== undefined) head.push(kind);
    return [
      head,
      [copyText("acct.hover.usage"), copyText("acct.val.none"), copyText("acct.hover.noRelay")],
      [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")],
    ];
  }
  const cur = read.current;
  const seen = ledgerOfAny(quota, cur);
  const unseen = seen ? undefined : quota?.unseen.find((u) => u.account === cur);
  // 号那一段照核心写好的几行；采样那一格后面接上是哪台采的（问的是哪台、那台叫什么是这一侧的事）。
  const rows = (seen ?? unseen)?.rows.map((r) => r.map((c) => c.text)) ?? [[accountLabel(cur)]];
  if (seen && entry) rows[rows.length - 1].push(machineLabel(entry.origin));
  if (read.start !== cur) rows.push([copyText("rot.hover.from", { account: accountLabel(read.start) })]);
  return rows;
}
