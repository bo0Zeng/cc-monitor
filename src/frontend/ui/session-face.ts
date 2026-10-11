/**
 * **一个会话在主窗口上读成什么**：状态点 · 状态一句 · 需手动 · 悬停卡那一句 · 标题拆成几段。
 * 标签页行、悬停卡、会话头、「需手动」那几处都从这里取，不各拼一份。
 *
 * 只排版：两轴状态（`tab-session-state.ts`）、活动信号（`Tab.activity`）、会话事实（`needs` · `pending` · `lastSay`）
 * 都是后端给的事实；这里只挑哪一样露、怎么写。唯一的「合」：活动信号说不在等了，手上那份 `needs` 当场不认
 * （活动信号比会话事实来得早；两者对不上时以它为准，不留一条已经答完的「需手动」）；说在等、会话事实还没到 ⇒
 * 先写「需手动」（分不出是哪种，不猜）。
 */
import type { DotState } from "./kit/status-dot";
import type { BackgroundWork, Needs } from "./session-reads";
import type { Tab } from "./tab-model";
import { projectNameFromCwd } from "./tab-model";
import { sessionDot } from "./session-status";
import { isLive } from "./tab-session-state";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { dotLabel } from "./session-words";
import { clockNow, spanNow, waitedNow } from "./duration-format";

/** 此刻在等你（活着 ＋ 活动信号说在等人）⇒ 等的是什么；不在等 ⇒ `null`。会话事实还没到 ⇒ 种类判不出，字照抄活动信号带来的那个。 */
export function needsOf(tab: Tab): Needs | null {
  if (!isLive(tab.state) || tab.activity?.doing !== "needs_you") return null;
  return tab.needs ?? { kind: "unknown", tool: null, call: null, what: null, sinceMs: null, text: tab.activity.text, head: tab.activity.text, headCode: null, tone: tab.activity.tone, rank: Number.MAX_SAFE_INTEGER, waitedMs: null, waitedText: null, receivedAt: 0 };
}

/**
 * 状态点的名字（读屏名 / 悬停名）：活着 ⇒ 那台核心写好的那个字（活动信号带来的，照抄）；
 * 活动信号说不清、或不是活着的那几种 ⇒ 状态点那一档的名字（`session-words.ts::dotLabel`）。
 */
export function stateWord(tab: Tab): string {
  const text = isLive(tab.state) ? tab.activity?.text : null;
  return text ?? dotLabel(dotOf(tab));
}

/** 状态点：颜色 ＝ 在干什么，形状 ＝ 进程还在不在。 */
export function dotOf(tab: Tab): DotState {
  return sessionDot(tab.state, tab.activity?.tone ?? null);
}

/** 后台任务运行中那一句此刻的字：有会走的那一句 ⇒ 时长那一截按此刻填（那一个读口，与核心对同一份金样）；否则照抄核心写好的那一句。 */
export function backgroundLine(b: BackgroundWork, now: number): string {
  return b.clock ? clockNow(b.clock, now) : b.text;
}

/** 时刻（毫秒，后端解好的）⇒ 距 `now` 多久（那一个读口）；没有 ⇒ `null`。 */
export function sinceText(atMs: number | null, now: number): string | null {
  return atMs === null ? null : spanNow(atMs, now);
}

/** 这个会话在哪台：本机写「本机」，远端写机器名。 */
export function machineOf(tab: Tab): string {
  return isRemoteOrigin(tab.origin) ? tab.origin : copyText("sessionFace.machine.local");
}

/**
 * 状态一句（会话头 · 悬停卡第三行）：`运行中 · Bash 2m` · `等批准 · 2m` · `后台任务运行中 · make test-all · 12m` · `空闲 · 完成 3m 前` · `已结束` ·
 * `Claude 已退出` · `状态不明 · gpu-01 不可见` · `记录已不在`。`needs` ＝ 这一句是不是「需手动」（琥珀）。
 */
export function stateLine(tab: Tab, now: number): { text: string; needs: boolean } {
  const n = needsOf(tab);
  if (n) {
    const waited = waitedNow(n, now);
    const word = n.text;
    return { text: waited ? copyText("sessionFace.state.waiting", { kind: word, waited }) : word, needs: true };
  }
  switch (dotOf(tab)) {
    case "running": {
      const p = tab.pending[0];
      if (!p) return { text: stateWord(tab), needs: false };
      const dur = sinceText(p.atMs, now);
      return { text: dur ? copyText("sessionFace.state.runningFor", { tool: p.name, dur }) : copyText("sessionFace.state.running", { tool: p.name }), needs: false };
    }
    case "background": {
      // 那一句由核心写（命令 · 几条 · 时长）；时长那一截按会走的那一句走字（`fmtDur`，与核心对同一份金样）。拿不到 ⇒ 核心写的那个字。
      const b = tab.backgroundWork;
      return { text: b ? backgroundLine(b, now) : stateWord(tab), needs: false };
    }
    case "idle": {
      const ago = sinceText(tab.lastSay?.atMs ?? null, now);
      if (!ago) return { text: stateWord(tab), needs: false };
      return { text: tab.unread > 0 ? copyText("sessionFace.state.idleUnseen", { ago }) : copyText("sessionFace.state.idleSeen", { ago }), needs: false };
    }
    case "unknown":
      return { text: copyText("sessionFace.state.unseen", { machine: machineOf(tab) }), needs: false };
    case "exited":
      return { text: copyText("sessionState.reconnectable.tooltip"), needs: false };
    case "ended":
      return { text: copyText("sessionState.ended.tooltip"), needs: false };
    case "gone":
      return { text: copyText("sessionState.gone.tooltip"), needs: false };
    case "needs-you":
    case "failed":
      return { text: stateWord(tab), needs: false };
  }
}

/** 悬停卡里那一句 peek：在等你 ⇒ 它在等的那一句；在跑 ⇒ 正在做的那一步；空闲 · 后台任务运行中 ⇒ 它最后一句。拿不到 ⇒ `null`（不出这一行）。 */
export function peekLine(tab: Tab): string | null {
  const n = needsOf(tab);
  if (n) return n.what;
  const d = dotOf(tab);
  if (d === "running") return tab.pending[0]?.what ?? null;
  if (d === "idle" || d === "background") return tab.lastSay?.text ?? null;
  return null;
}

/** 标题拆成几段（标签页行 · 会话头）：项目名（`--text-2`）· 标题 · 后台（齿轮）· 分叉来的（`↳`）。 */
export interface TitleParts {
  proj: string | null;
  title: string;
  bg: boolean;
  forked: boolean;
}

export function titleParts(tab: Tab): TitleParts {
  const proj = tab.projectDir ? projectNameFromCwd(tab.projectDir) : null;
  const bg = tab.background;
  const forked = tab.forkedFromSessionId !== null;
  const named = bg ? (tab.bgName ?? tab.aiTitle) : tab.aiTitle;
  if (named) return { proj, title: named, bg, forked };
  return { proj: null, title: proj ?? tab.sessionId.slice(0, 8), bg, forked };
}

/** 一行写完的标题（会话头 · 悬停卡第一行）：`项目 标题`。 */
export function fullTitle(tab: Tab): string {
  const p = titleParts(tab);
  const t = `${p.forked ? "↳ " : ""}${p.title}`;
  return p.proj ? `${p.proj} ${t}` : t;
}

/** 收着的组头上那一格汇总：几个在等你、几个在跑、几个后台任务运行中（按状态点数；空闲 · 已结束 · 状态不明 · Claude 已退出不算）。 */
export function groupSummary(members: readonly Tab[]): { needs: number; running: number; background: number; needsWord: string; runningWord: string; backgroundWord: string } {
  let needs = 0;
  let running = 0;
  let background = 0;
  // 那几颗点的名字照抄组员身上核心写的字（同一种语气的字是同一个），不按点自己取字。
  let needsWord = "";
  let runningWord = "";
  let backgroundWord = "";
  for (const t of members) {
    const d = dotOf(t);
    if (d === "needs-you") {
      needs++;
      needsWord ||= stateWord(t);
    } else if (d === "running") {
      running++;
      runningWord ||= stateWord(t);
    } else if (d === "background") {
      background++;
      backgroundWord ||= stateWord(t);
    }
  }
  return { needs, running, background, needsWord, runningWord, backgroundWord };
}

/** 窄窗那一格的两个字母：项目目录名里的头两个字母（小写）；没有字母 ⇒ 头两个字。 */
export function abbrOf(tab: Tab): string {
  const name = (tab.projectDir ? projectNameFromCwd(tab.projectDir) : null) ?? tab.aiTitle ?? tab.sessionId;
  const letters = name.replace(/[^a-z]/gi, "");
  return (letters.length >= 2 ? letters : name).slice(0, 2).toLowerCase();
}

/**
 * 两条「需手动」先答哪个 —— 照核心那一处（Rust `facts_query::needs_first`，对同一份金样 `needs-order.golden.json`）：
 * 危险度在前（`rank`，核心判好的）；同一档里等得久的在前；不知道等了多久的排这一档最后。
 * 「等了多久」＝ 那台答出那一刻算好的 `waitedMs` ＋ 本机从收到起走过的（各在一台钟上量，不跨机器比时刻）。
 */
export function needsFirst(a: Needs, b: Needs, now: number): number {
  if (a.rank !== b.rank) return a.rank - b.rank;
  const wa = a.waitedMs === null ? null : a.waitedMs + Math.max(0, now - a.receivedAt);
  const wb = b.waitedMs === null ? null : b.waitedMs + Math.max(0, now - b.receivedAt);
  if (wa === null || wb === null) return wa === wb ? 0 : wa === null ? 1 : -1;
  return wb - wa;
}

/** `Ctrl+J` 的顺序：需手动的那几个按 [`needsFirst`] 排，同档按条上看到的顺序。`tabs` 按条上看到的顺序给。 */
export function needsOrder(tabs: readonly Tab[], now: number): string[] {
  return tabs
    .map((t, i) => ({ t, i, n: needsOf(t) }))
    .filter((x): x is { t: Tab; i: number; n: Needs } => x.n !== null)
    .sort((a, b) => needsFirst(a.n, b.n, now) || a.i - b.i)
    .map((x) => x.t.sessionId);
}

/**
 * 「需手动」的下一站：会话在前（`order`）、计划项在后（`planCount` 条）。站在计划项上 ⇒ `planAt` 是第几条，否则看 `active` 那个会话。
 * 走到尽头绕回开头；两边都空 ⇒ `null`。
 */
export function nextNeedsStep(order: readonly string[], active: string | null, planAt: number | null, planCount: number): { sid: string } | { plan: number } | null {
  const first = (): { sid: string } | { plan: number } | null => (order.length > 0 ? { sid: order[0] } : planCount > 0 ? { plan: 0 } : null);
  if (planAt !== null && planAt < planCount) return planAt + 1 < planCount ? { plan: planAt + 1 } : first();
  const i = active === null ? -1 : order.indexOf(active);
  if (i < 0) return first();
  if (i + 1 < order.length) return { sid: order[i + 1] };
  return planCount > 0 ? { plan: 0 } : first();
}
