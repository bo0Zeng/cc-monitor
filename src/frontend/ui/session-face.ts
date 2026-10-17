/**
 * **一个会话在主窗口上读成什么**：状态点 · 状态一句 · 需手动 · 悬停卡那一句 · 标题拆成几段。
 * 标签页行、悬停卡、会话头、「需手动」那几处都从这里取，不各拼一份。
 *
 * 只排版：点 · 短名 · 状态一句是那台后端判好的整份脸（`Tab.face`，经 `face-can.ts::faceOf` 读）；会走的那一句与需手动的种类在
 * 会话事实里（`Tab.line` · `Tab.needs`），只在它写的那一态（`line.for`）与脸上那一态相同时用 —— 两份是同一刻的；对不上就用脸上那一句。
 */
import type { DotState } from "./kit/status-dot";
import type { BackgroundWork, Needs } from "./session-reads";
import type { FactsLine } from "./generated/FactsLine";
import type { Tab } from "./tab-model";
import { projectNameFromCwd } from "./tab-model";
import { faceOf } from "./face-can";
import { clockNow, spanNow } from "./duration-format";

/** 会话事实里那一句此刻作数：写它时那一态与脸上那一态相同。不作数 ⇒ `null`。 */
function lineOf(tab: Tab): FactsLine | null {
  const f = faceOf(tab);
  return f.live.is === "yes" && tab.line !== null && tab.line.for === f.live.activity ? tab.line : null;
}

/** 此刻在等人 ⇒ 等的是什么（会话事实那一份，与脸同一刻才认）；不在等 ⇒ `null`。 */
export function needsOf(tab: Tab): Needs | null {
  return lineOf(tab)?.for === "needs_you" ? tab.needs : null;
}

/** 状态点的名字（读屏名 / 悬停名）：脸上的短名。 */
export function stateWord(tab: Tab): string {
  return faceOf(tab).name;
}

/** 状态点（那台判好的那一颗）。 */
export function dotOf(tab: Tab): DotState {
  return faceOf(tab).dot;
}

/** 后台任务运行中那一句此刻的字：有会走的那一句 ⇒ 时长那一截按此刻填（那一个读口，与核心对同一份金样）；否则照抄核心写好的那一句。 */
export function backgroundLine(b: BackgroundWork, now: number): string {
  return b.clock ? clockNow(b.clock, now) : b.text;
}

/** 时刻（毫秒，后端解好的）⇒ 距 `now` 多久（那一个读口）；没有 ⇒ `null`。 */
export function sinceText(atMs: number | null, now: number): string | null {
  return atMs === null ? null : spanNow(atMs, now);
}

/**
 * 状态一句（会话头 · 悬停卡第三行）：会话事实那一句（会走的那一截按此刻填）与脸同一刻 ⇒ 用它；否则脸上那一句。
 * `needs` ＝ 这一句是不是「需手动」（琥珀）。
 */
export function stateLine(tab: Tab, now: number): { text: string; needs: boolean } {
  const f = faceOf(tab);
  const l = lineOf(tab);
  return { text: l ? (l.clock ? clockNow(l.clock, now) : l.text) : f.line, needs: f.dot === "needs-you" };
}

/** 悬停卡里那一句 peek（会话事实写的：在等 ⇒ 等的那一句 · 在跑 ⇒ 那一步 · 空闲 / 后台 ⇒ 最后一句）；不作数 / 没有 ⇒ `null`（不出这一行）。 */
export function peekLine(tab: Tab): string | null {
  return lineOf(tab)?.peek ?? null;
}

/** 标题拆成几段（标签页行 · 会话头）：项目名（`--text-2`）· 标题 · 后台（齿轮）· 分叉来的（`↳`）。 */
export interface TitleParts {
  proj: string | null;
  title: string;
  bg: boolean;
  forked: boolean;
}

export function titleParts(tab: Tab): TitleParts {
  const bg = tab.background;
  const forked = tab.forkedFromSessionId !== null;
  const name = tab.label ?? tab.sessionId.slice(0, 8);
  return { proj: tab.labelProject, title: bg ? (tab.bgName ?? name) : name, bg, forked };
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
  const name = (tab.projectDir ? projectNameFromCwd(tab.projectDir) : null) ?? tab.label ?? tab.sessionId;
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
