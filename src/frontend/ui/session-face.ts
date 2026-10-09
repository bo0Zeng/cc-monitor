/**
 * **一个会话在主窗口上读成什么**：状态点 · 状态一句 · 需要你 · 悬停卡那一句 · 标题拆成几段。
 * 标签页行、悬停卡、会话头、「需要你」那几处都从这里取，不各拼一份。
 *
 * 只排版：两轴状态（`tab-session-state.ts`）、活动信号（`Tab.activity`）、会话事实（`needs` · `pending` · `lastSay`）
 * 都是后端给的事实；这里只挑哪一样露、怎么写。唯一的「合」：活动信号说不在等了，手上那份 `needs` 当场不认
 * （活动信号比会话事实来得早；两者对不上时以它为准，不留一条已经答完的「需要你」）；说在等、会话事实还没到 ⇒
 * 先写「需要你」（分不出是哪种，不猜）。
 */
import type { DotState } from "./kit/status-dot";
import type { Needs } from "./session-reads";
import type { Tab } from "./tab-model";
import { projectNameFromCwd } from "./tab-model";
import { activityFace } from "./session-status";
import { isLive } from "./tab-session-state";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { dotLabel } from "./session-words";
import { fmtDur } from "./quota-lines";

/** 此刻在等你（活着 ＋ 活动信号说在等人）⇒ 等的是什么；不在等 ⇒ `null`。会话事实还没到 ⇒ 种类判不出，字照抄活动信号带来的那个。 */
export function needsOf(tab: Tab): Needs | null {
  if (!isLive(tab.state) || tab.activity?.doing !== "needs_you") return null;
  return tab.needs ?? { kind: "unknown", tool: null, call: null, what: null, sinceMs: null, text: tab.activity.text ?? "", tone: tab.activity.tone ?? "need" };
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
  const s = tab.state;
  switch (s.liveness) {
    case "unseen":
      return "unknown";
    case "dead":
      return s.recoverability === "attachable" ? "exited" : s.recoverability === "gone" ? "gone" : "ended";
    case "live":
      return activityFace(tab.activity?.doing ?? null).dot;
  }
}

/** 时刻（ISO / epoch ms）⇒ 距 `now` 多久（`fmtDur`）；读不出 ⇒ `null`。 */
export function sinceText(at: string | number | null, now: number): string | null {
  if (at === null) return null;
  const t = typeof at === "number" ? at : Date.parse(at);
  if (!Number.isFinite(t)) return null;
  return fmtDur((now - t) / 1000);
}

/** 这个会话在哪台：本机写「本机」，远端写机器名。 */
export function machineOf(tab: Tab): string {
  return isRemoteOrigin(tab.origin) ? tab.origin : copyText("sessionFace.machine.local");
}

/**
 * 状态一句（会话头 · 悬停卡第三行）：`运行中 · Bash 2m` · `等批准 · 2m` · `空闲 · 完成 3m 前` · `已结束` ·
 * `Claude 已退出` · `状态不明 · gpu-01 不可见` · `记录已不在`。`needs` ＝ 这一句是不是「需要你」（琥珀）。
 */
export function stateLine(tab: Tab, now: number): { text: string; needs: boolean } {
  const n = needsOf(tab);
  if (n) {
    const waited = sinceText(n.sinceMs, now);
    const word = n.text;
    return { text: waited ? copyText("sessionFace.state.waiting", { kind: word, waited }) : word, needs: true };
  }
  switch (dotOf(tab)) {
    case "running": {
      const p = tab.pending[0];
      if (!p) return { text: stateWord(tab), needs: false };
      const dur = sinceText(p.at, now);
      return { text: dur ? copyText("sessionFace.state.runningFor", { tool: p.name, dur }) : copyText("sessionFace.state.running", { tool: p.name }), needs: false };
    }
    case "idle": {
      const ago = sinceText(tab.lastSay?.at ?? null, now);
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

/** 悬停卡里那一句 peek：在等你 ⇒ 它在等的那一句；在跑 ⇒ 正在做的那一步；空闲 ⇒ 它最后一句。拿不到 ⇒ `null`（不出这一行）。 */
export function peekLine(tab: Tab): string | null {
  const n = needsOf(tab);
  if (n) return n.what;
  const d = dotOf(tab);
  if (d === "running") return tab.pending[0]?.what ?? null;
  if (d === "idle") return tab.lastSay?.text ?? null;
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

/** 收着的组头上那一格汇总：几个在等你、几个在跑（按状态点数；空闲 · 已结束 · 状态不明 · Claude 已退出不算）。 */
export function groupSummary(members: readonly Tab[]): { needs: number; running: number } {
  let needs = 0;
  let running = 0;
  for (const t of members) {
    const d = dotOf(t);
    if (d === "needs-you") needs++;
    else if (d === "running") running++;
  }
  return { needs, running };
}

/** 窄窗那一格的两个字母：项目目录名里的头两个字母（小写）；没有字母 ⇒ 头两个字。 */
export function abbrOf(tab: Tab): string {
  const name = (tab.projectDir ? projectNameFromCwd(tab.projectDir) : null) ?? tab.aiTitle ?? tab.sessionId;
  const letters = name.replace(/[^a-z]/gi, "");
  return (letters.length >= 2 ? letters : name).slice(0, 2).toLowerCase();
}

/**
 * `Ctrl+J` 的顺序：在等你的里面，等得最久的在前（不知道何时起等的排后，同档按条上看到的顺序）。
 * `tabs` 按条上看到的顺序给。
 */
export function needsOrder(tabs: readonly Tab[]): string[] {
  return tabs
    .map((t, i) => ({ t, i, n: needsOf(t) }))
    .filter((x) => x.n !== null)
    .sort((a, b) => {
      const sa = a.n!.sinceMs ?? Number.POSITIVE_INFINITY;
      const sb = b.n!.sinceMs ?? Number.POSITIVE_INFINITY;
      return sa === sb ? a.i - b.i : sa - sb;
    })
    .map((x) => x.t.sessionId);
}

/** 按一下 `Ctrl+J` 落到哪：当前就是在等你的 ⇒ 下一个（转回头）；否则第一个。没有在等你的 ⇒ `null`。 */
export function nextNeeds(order: readonly string[], active: string | null): string | null {
  if (order.length === 0) return null;
  const i = active === null ? -1 : order.indexOf(active);
  return i < 0 ? order[0] : order[(i + 1) % order.length];
}
