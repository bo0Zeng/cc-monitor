/**
 * 额度与账号几处界面的**排版模型**（纯函数）：状态栏那颗按钮 · 它的悬停卡 · 记录与换号条的那一句 · 提示条那一句。
 *
 * 输入全是后端的成品（`rotation-session-read` 一个会话那一份 ＋ `quota-read` 那台一份）；这里只按词选字、拼格，不判
 * 「能不能换 / 该不该换 / 快满没有」。时刻照后端写好的那一格（`…Text`），距今只经 `quota-lines.ts` 的 [`fmtRel`]（与金样同一处）。
 */
import { copyText } from "./copy-table";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { accountLabel, fmtRel, seenBlock, slotLabel, unseenBlock, type QuotaRead, type QuotaReadAccount } from "./quota-lines";
import type { QuotaShow } from "./generated/QuotaShow";
import type { SessionRotationState } from "./generated/SessionRotationState";
import type { SwitchRecord } from "./generated/SwitchRecord";
import type { SessionRotationEntry } from "./app-store";

/** 采样那台在界面上叫什么。 */
export function machineLabel(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("acct.machine.here") : origin;
}

export type ChipTone = "neutral" | "near" | "refused" | "over";

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

/** `quota-read` 里那个号那一条。 */
export function ledgerOf(quota: QuotaRead | null, agent: string, account: string): QuotaReadAccount | undefined {
  return quota?.accounts.find((a) => a.agent === agent && a.account === account);
}

/** 一个号的显示态 ⇒ 窗口 · 值 · 重置 · 色（按钮与「下一个」那一格同一套）。 */
export function usageOf(
  q: QuotaShow,
  reading: QuotaReadAccount["reading"],
): { window: string | null; value: string; reset: string | null; tone: ChipTone } {
  if (q.kind === "api") {
    if (q.state !== "refused") return { window: null, value: copyText("acct.kind.api"), reset: null, tone: "neutral" };
    const at = reading?.resetsAtText;
    return { window: null, value: copyText("acct.val.refusedOnly"), reset: at === undefined ? null : copyText("acct.reset.at", { at }), tone: "refused" };
  }
  const w = q.limiting ?? "5h";
  const slot = q.slots.find((s) => s.slot === w);
  const window = slotLabel(w);
  const resetAt = slot?.resetsAt !== undefined ? slot.resetsAtText : reading?.resetsAtText;
  switch (q.state) {
    case "refused":
      return {
        window,
        value: slot?.full ? copyText("acct.val.full") : slot?.pct === undefined ? copyText("acct.val.refusedOnly") : copyText("acct.val.refusedPct", { pct: slot.pct }),
        reset: resetAt === undefined ? null : copyText("acct.reset.at", { at: resetAt }),
        tone: "refused",
      };
    case "overageInUse":
      return { window, value: copyText("acct.val.over"), reset: null, tone: "over" };
    case "resetSinceSeen":
    case "unseen":
      return { window, value: copyText("acct.val.none"), reset: null, tone: "neutral" };
    default:
      break;
  }
  if (slot?.pct === undefined) return { window, value: copyText("acct.val.none"), reset: null, tone: "neutral" };
  if (slot.full) return { window, value: copyText("acct.val.full"), reset: null, tone: "refused" };
  return { window, value: copyText("acct.val.pct", { pct: slot.pct }), reset: null, tone: q.state === "near" ? "near" : "neutral" };
}

/**
 * ★ 本会话那颗按钮。`entry` ＝ 这个会话在它那台的轮换格（还没问到 ⇒ `undefined`）；`fallback` ＝ tab 栏徽章那份快照里
 * 这个会话归属的号（中转没见过这个会话时只能画它）；都没有 ⇒ `null`（不画）。
 */
export function sessionChip(entry: SessionRotationEntry | undefined, fallback: string | null, quota: QuotaRead | null): ChipModel | null {
  const read = entry?.read;
  if (!read) {
    if (fallback === null) return null;
    return { account: fallback, name: accountLabel(fallback), swapped: false, window: null, value: null, reset: null, tone: "neutral", stale: false };
  }
  if (read.state === "absent") {
    // 中转没见过这个会话：有归属就画那个号（无用量）；归属也不知道 ⇒ 照实写「未实时显示」，点开仍是本会话面板（只剩重启切换）。
    const name = fallback === null ? copyText("acct.hover.noRelay") : accountLabel(fallback);
    return { account: fallback, name, swapped: false, window: null, value: null, reset: null, tone: "neutral", stale: false };
  }
  const cur = read.account.current;
  const u = usageOf(read.quota, ledgerOf(quota, read.agent, cur)?.reading);
  return {
    account: cur,
    name: accountLabel(cur),
    swapped: cur !== read.account.start,
    window: u.window,
    value: u.value,
    reset: u.reset,
    tone: u.tone,
    stale: read.quota.stale,
  };
}

/** 换过号的那一刻与原号（首行尾 `⇄ 14:20 ← work`）：最后一条落到此刻这个号上的记录；没有 ⇒ 起始号。 */
export function swappedFrom(read: Extract<SessionRotationState, { state: "present" }>): { at: string; from: string } | null {
  const cur = read.account.current;
  if (cur === read.account.start) return null;
  const last = [...read.account.history].reverse().find((h) => h.to === cur && h.from !== h.to);
  return { at: (last ? last.atText : read.account.sinceText) ?? "", from: last?.from ?? read.account.start };
}

/**
 * ★ 本会话悬停卡的行：那个号在 `quota-read` 里那一段（与金样同一个行模型）＋ 会话那两样（首行尾 `⇄` · `下一个`）。
 * 中转没见过这个会话 ⇒ `用量 — 未实时显示` · `切换 仅重启切换`。
 */
export function sessionHoverRows(entry: SessionRotationEntry | undefined, fallback: string | null, quota: QuotaRead | null): string[][] | null {
  const read = entry?.read;
  if (!read || read.state === "absent") {
    if (fallback === null) return read ? [[copyText("acct.hover.noRelay")], [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")]] : null;
    const known = quota?.accounts.find((a) => a.account === fallback) ?? quota?.unseen.find((u) => u.account === fallback);
    const head = [accountLabel(fallback)];
    if (known) head.push(known.kind === "api" ? copyText("acct.kind.api") : copyText("acct.kind.sub"));
    return [
      head,
      [copyText("acct.hover.usage"), copyText("acct.val.none"), copyText("acct.hover.noRelay")],
      [copyText("acct.hover.switch"), copyText("acct.hover.restartOnly")],
    ];
  }
  const now = entry.now;
  const cur = read.account.current;
  const seen = ledgerOf(quota, read.agent, cur);
  const block = seen
    ? seenBlock(seen, now, machineLabel(entry.origin))
    : unseenBlock({ agent: read.agent, account: cur, kind: read.quota.kind, login: read.quota.login });
  const rows = block.rows.map((r) => [...r]);
  const from = swappedFrom(read);
  if (from) rows[0].push(copyText("acct.hover.from", { at: from.at, name: accountLabel(from.from) }));
  if (read.quota.kind === "sub") {
    const next = nextRow(read, quota);
    const at = rows.length > 1 && rows[rows.length - 1][0] === copyText("acct.row.seen") ? rows.length - 1 : rows.length;
    rows.splice(at, 0, next);
  }
  return rows;
}

/** `下一个  team  5h 4%` / `下一个  —  轮换内无未满`。 */
function nextRow(read: Extract<SessionRotationState, { state: "present" }>, quota: QuotaRead | null): string[] {
  const label = copyText("acct.hover.next");
  if (read.next === undefined) return [label, copyText("acct.val.none"), copyText("acct.hover.nextNone")];
  const led = ledgerOf(quota, read.agent, read.next);
  if (!led) return [label, accountLabel(read.next), copyText("acct.val.none")];
  const u = usageOf(led, led.reading);
  return [label, accountLabel(read.next), usageText(u)];
}

/** 记录那一条的原因（`work 5h ✕` · `手动 · 热切换` …）＋ 那一刻原号几点重置（`↻19:00`，记下就不变）。 */
export function whyOf(h: SwitchRecord): { why: string; reset: string | null } {
  const reset = h.fromResetsAtText === undefined ? null : copyText("acct.reset.at", { at: h.fromResetsAtText });
  const from = accountLabel(h.from);
  const w = h.why;
  // 光字符串的那几种先判完（`in` 不许碰到字符串）；两段末尾各一处 `never`：`SwitchWhy` 多了一种而这里没接 ⇒ 编不过。
  if (typeof w === "string") {
    switch (w) {
      case "manualHot":
        return { why: copyText("acct.hist.manualHot"), reset: null };
      case "manualRestart":
        return { why: copyText("acct.hist.manualRestart"), reset: null };
      case "toOverage":
        return { why: copyText("acct.hist.toOverage"), reset };
      case "preempt":
        return { why: copyText("acct.hist.preempt", { name: accountLabel(h.to) }), reset: null };
      case "leaveFallback":
        return { why: copyText("acct.hist.leaveFallback", { name: accountLabel(h.to) }), reset: null };
      default: {
        const left: never = w;
        return left;
      }
    }
  }
  if ("full" in w) return { why: w.full.w ? copyText("acct.hist.full", { name: from, w: slotLabel(w.full.w) }) : copyText("acct.hist.fullAny", { name: from }), reset };
  if ("threshold" in w) return { why: copyText("acct.hist.pct", { name: from, n: w.threshold.n }), reset };
  if ("held" in w) return { why: copyText("acct.hist.held", { name: from, n: w.held.n }), reset };
  if ("stint" in w) return { why: copyText("acct.hist.stint", { w: slotLabel(w.stint.w), n: w.stint.n }), reset: null };
  if ("wait" in w) return { why: copyText("acct.hist.wait", { name: accountLabel(w.wait.account), instead: accountLabel(w.wait.instead) }), reset };
  if ("skipped" in w) return { why: copyText("acct.hist.skipped", { name: accountLabel(w.skipped.account), reason: unreadyLabel(w.skipped.reason, w.skipped.account) }), reset: null };
  const left: never = w;
  return left;
}

function unreadyLabel(reason: string, account: string): string {
  if (reason === "needsLogin") return copyText("acct.reason.needsLogin", { name: accountLabel(account) });
  if (reason === "needsKey") return copyText("acct.reason.needsKey", { name: accountLabel(account) });
  return copyText("acct.reason.unknown");
}

/** 「热切换」不成立 / 切换没成的原因码 ⇒ 原因词。 */
export function reasonLabel(code: string, ctx: { agent: string; target: string }): string {
  switch (code) {
    case "noRelay":
      return copyText("acct.reason.noRelay");
    case "ended":
      return copyText("acct.reason.ended", { ended: copyText("sessionState.ended.name") });
    case "machineNotMulti":
      return copyText("acct.reason.notMulti");
    case "agentHasNoAccounts":
      return copyText("acct.reason.noAccounts", { agent: ctx.agent });
    case "targetNeedsLogin":
      return copyText("acct.reason.needsLogin", { name: accountLabel(ctx.target) });
    case "targetNeedsKey":
      return copyText("acct.reason.needsKey", { name: accountLabel(ctx.target) });
    case "ioFailed":
      return copyText("acct.reason.ioFailed");
    case "account_unavailable":
      return copyText("acct.reason.accountUnavailable", { name: accountLabel(ctx.target) });
    case "ambiguous":
      return copyText("acct.reason.ambiguous");
    case "not_in_terminal":
      return copyText("acct.reason.notInTerminal");
    case "stop_failed":
      return copyText("acct.reason.stopFailed");
    case "session_already_live":
      return copyText("acct.reason.alreadyLive");
    case "start_failed":
      return copyText("acct.reason.startFailed");
    case "notArrived":
      return copyText("acct.reason.notArrived");
    case "shutting_down":
      return copyText("acct.reason.shuttingDown");
    default:
      return copyText("acct.reason.unknown");
  }
}

/**
 * 本会话发不出去时那一条（会话头下的提示条）；能发 ⇒ `null`。`blocked` 是后端给的事实（最早回来的那个号与时刻）；
 * 画的那一刻那个时刻已过 ⇒ 中性的 `↻.. 已过 · 可续发`。
 */
export function bannerOf(entry: SessionRotationEntry | undefined, nowSecs: number): { text: string; tone: "warn" | "neutral" } | null {
  const read = entry?.read;
  if (!read || read.state === "absent" || read.blocked === undefined) return null;
  const e = read.blocked.earliest;
  if (e === undefined) return { text: copyText("acct.banner.allFullNoAt"), tone: "warn" };
  const name = accountLabel(e.account);
  const at = e.atText ?? "";
  const rel = fmtRel(e.at, nowSecs);
  if (rel === null) return { text: copyText("acct.banner.back", { name, at }), tone: "neutral" };
  const last = read.account.history[read.account.history.length - 1];
  if (last && typeof last.why === "object" && "held" in last.why) {
    return { text: copyText("acct.banner.held", { n: last.why.held.n, name, at, rel: rel.slice(1) }), tone: "warn" };
  }
  if (e.account === read.account.current) {
    const w = slotLabel(read.quota.limiting ?? "5h");
    return { text: copyText("acct.banner.full", { name, w, at, rel: rel.slice(1) }), tone: "warn" };
  }
  return { text: copyText("acct.banner.allFull", { name, at, rel: rel.slice(1) }), tone: "warn" };
}

/** 标签页上被卡住的会话：标题后红字 `✕ 5h`（与「等批准」同位）＋ 悬停多一行 `状态 ✕ 5h · 最早 team ↻18:10`。能发 ⇒ `null`。 */
export function tabBlockedOf(entry: SessionRotationEntry | undefined): { text: string; hover: string } | null {
  const read = entry?.read;
  if (!read || read.state === "absent" || read.blocked === undefined) return null;
  const w = slotLabel(read.quota.limiting ?? "5h");
  const e = read.blocked.earliest;
  const text = copyText("acct.tab.blocked", { w });
  if (e === undefined) return { text, hover: text };
  return { text, hover: copyText("acct.tab.hover", { w, name: accountLabel(e.account), at: e.atText ?? "" }) };
}

/** 一个号用量的一格字（下拉项右侧 · 切换那颗按钮）：`5h 63%` · 用满 `5h ✕ ↻19:00` · 被拒 `5h 58% · 被拒 ↻19:00` · `按量`。 */
export function usageText(u: { window: string | null; value: string; reset: string | null }): string {
  if (u.window === null) return u.reset === null ? u.value : copyText("acct.usage.wv", { w: u.value, v: u.reset });
  return u.reset === null ? copyText("acct.usage.wv", { w: u.window, v: u.value }) : copyText("acct.usage.wvr", { w: u.window, v: u.value, r: u.reset });
}
