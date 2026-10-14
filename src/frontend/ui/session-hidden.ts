/**
 * **会话隐藏 · 血缘：界面这一侧读的那几格与交的那几条命令**（叶子模块）。
 *
 * 判谁隐藏、为什么隐藏全在后端（`observe/session_hidden.rs`，五档顺序：单个点的 → 起会话时的 `--tab` → 分身按这台偏好 →
 * 有主会话按这台偏好「子会话」→ 不隐藏）。界面只照帧上那一格画：
 * - 会话宣告（`live`）· 推帧主题「会话隐藏」各带一格 `hidden: {by, said} | null`（不隐藏 ⇒ 缺或 `null`）；
 * - 会话事实（`history-facts`）多两格：`startedBy`（由谁启动）· `children`（它起的子会话）。
 * 改单个那一格：`history-annotate {sid, patch: {hide: "on" | "off" | "rule"}}`（`rule` ＝ 摘掉单个那一格、回到规则）。
 * 这台偏好两格：`prefs.json` 的 `childTab` · `bgTab`（`show | hide`）。
 *
 * 交命令的那一半住 `session-hidden-cmds.ts`（这里零 IPC，`tab-session-facts.ts` 等纯模块也读）。
 * 〔对类型〕这几格的形状照设计稿 `会话隐藏.md` §4.1 · §4.3 手写在这里，后端那一路（生成物）进来以后这里改成从生成物 re-export、
 * 下面四处 `as unknown as` 一并删掉（主题名 · 注解补丁 · 偏好两格 · 读回的偏好）。解码器照旧留着：帧上那一格读不懂 ⇒ 当没隐藏。
 */
import type { Topic } from "./generated/Topic";

/** 为什么隐藏（后端判的那一档）：单个点的 · 别名写的 · 子会话按这台偏好 · 分身按这台偏好。 */
export type HiddenBy = "self" | "alias" | "child" | "bg";

/** 隐藏那一格：哪一档 ＋ 后端写好的那一句（悬停里照抄）。 */
export interface SessionHidden {
  by: HiddenBy;
  said: string;
}

const BY: ReadonlySet<string> = new Set<HiddenBy>(["self", "alias", "child", "bg"]);

/** 帧上那一格 ⇒ {@link SessionHidden}；缺 · `null` · 读不懂 ⇒ `null`（当没隐藏：宁可多占一格，不把会话藏丢）。 */
export function hiddenOf(v: unknown): SessionHidden | null {
  if (v === null || typeof v !== "object") return null;
  const o = v as { by?: unknown; said?: unknown };
  if (typeof o.by !== "string" || !BY.has(o.by) || typeof o.said !== "string") return null;
  return { by: o.by as HiddenBy, said: o.said };
}

/** 两格比一比（变了才重画）。 */
export function sameHidden(a: SessionHidden | null, b: SessionHidden | null): boolean {
  return a === b || (a !== null && b !== null && a.by === b.by && a.said === b.said);
}

/** 推帧主题「会话隐藏」（`changed/session_hidden`：`key` ＝ 会话，`body` ＝ 那一格）。 */
export const HIDDEN_TOPIC = "session_hidden" as unknown as Topic;

/** 由谁启动（主会话）：会话 id ＋ 后端写好的名字。 */
export interface LineageParent {
  sid: string;
  title: string;
}

/** 它起的一个子会话：会话 id · 名字 · 活不活 · 隐藏那一格（没隐藏 ⇒ `null`）。 */
export interface LineageChild {
  sid: string;
  title: string;
  live: boolean;
  hidden: SessionHidden | null;
}

/** 会话事实里血缘那两格。 */
export interface SessionLineage {
  startedBy: LineageParent | null;
  children: LineageChild[];
}

export const NO_LINEAGE: SessionLineage = { startedBy: null, children: [] };

/** 会话事实（`history-facts` 那一份）⇒ 血缘两格；缺 · 读不懂 ⇒ 没有。 */
export function lineageOf(facts: unknown): SessionLineage {
  if (facts === null || typeof facts !== "object") return NO_LINEAGE;
  const f = facts as { startedBy?: unknown; children?: unknown };
  const p = f.startedBy as { sid?: unknown; title?: unknown } | null | undefined;
  const startedBy = p && typeof p === "object" && typeof p.sid === "string" && typeof p.title === "string" ? { sid: p.sid, title: p.title } : null;
  const children: LineageChild[] = [];
  if (Array.isArray(f.children)) {
    for (const c of f.children as unknown[]) {
      const o = c as { sid?: unknown; title?: unknown; live?: unknown; hidden?: unknown } | null;
      if (!o || typeof o.sid !== "string" || typeof o.title !== "string") continue;
      children.push({ sid: o.sid, title: o.title, live: o.live === true, hidden: hiddenOf(o.hidden) });
    }
  }
  return startedBy === null && children.length === 0 ? NO_LINEAGE : { startedBy, children };
}

/** 两份血缘比一比。 */
export function sameLineage(a: SessionLineage, b: SessionLineage): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** 单个那一格：隐藏 · 取消隐藏（不按规则）· 回到规则。 */
export type HideSet = "on" | "off" | "rule";

/** 这台偏好里的两格：子会话 · 分身会话默认进不进标签栏。 */
export type TabPlace = "show" | "hide";
export interface TabPlaces {
  childTab: TabPlace;
  bgTab: TabPlace;
}

/** 后端缺省（稿 §4.2：子会话隐藏 · 分身显示）；读回的偏好里没有这两格（老后端）⇒ 照它画。 */
const PLACE_DEFAULT: TabPlaces = { childTab: "hide", bgTab: "show" };

function placeOf(v: unknown, d: TabPlace): TabPlace {
  return v === "show" || v === "hide" ? v : d;
}

/** 读回的那份偏好 ⇒ 两格。 */
export function placesOf(prefs: unknown): TabPlaces {
  const p = (prefs ?? {}) as { childTab?: unknown; bgTab?: unknown };
  return { childTab: placeOf(p.childTab, PLACE_DEFAULT.childTab), bgTab: placeOf(p.bgTab, PLACE_DEFAULT.bgTab) };
}

