/**
 * **会话隐藏 · 血缘：界面这一侧读的那几格与交的那几条命令**（叶子模块）。
 *
 * 判谁隐藏、为什么隐藏全在后端（`control/session_hidden.rs`，五档顺序：单个点的 → 起会话时的 `--tab` → 分身按这台偏好 →
 * 有主会话按这台偏好「子会话」→ 不隐藏）。界面只照帧上那一格画，形状全是后端生成的类型：
 * - 会话宣告（`live`，壳把 `session_added.hidden` 原样转来）· 历史清单每行 · 需手动每行 · 会话事实的血缘各带一格 `hidden: Hidden | null`；
 * - 推帧主题 `session_hidden`：`key` ＝ 会话，`body` ＝ `{hidden}`（不隐藏了 ⇒ `null`）；
 * - 会话事实（`history-facts`）的 `startedBy`（由谁启动）· `children`（它起的子会话），都是 `Kin`。
 * 改单个那一格：`history-annotate {sid, patch: {hide: "on" | "off" | "rule"}}`（`rule` ＝ 摘掉单个那一格、回到规则）。
 * 这台偏好两格：`prefs.json` 的 `childTab` · `bgTab`（`show | hide`）；缺省只住后端，界面只画 `prefs-read` 读回的那一份。
 *
 * 交命令的那一半住 `session-hidden-cmds.ts`（这里零 IPC，`tab-session-facts.ts` 等纯模块也读）。
 */
import type { Changed } from "./changed-stream";
import type { Hidden } from "./generated/Hidden";
import type { HideAsk } from "./generated/HideAsk";
import type { Kin } from "./generated/Kin";
import type { Prefs } from "./generated/Prefs";
import type { SessionFacts } from "./generated/SessionFacts";
import type { Tab } from "./generated/Tab";

/** 隐藏那一格：哪一档（`self` · `alias` · `launch` · `child` · `bg`）＋ 后端写好的那一句（悬停里照抄）。 */
export type SessionHidden = Hidden;

/** 两格比一比（变了才重画）。 */
export function sameHidden(a: SessionHidden | null, b: SessionHidden | null): boolean {
  return a === b || (a !== null && b !== null && a.by === b.by && a.said === b.said);
}

/** 推帧主题「会话隐藏」（`changed/session_hidden`）一批 ⇒ 会话 ⇒ 那一格（`body {hidden}`；`null` ＝ 不隐藏了）。没 `key` · 没 `body` 的那一格不收（等那台重宣告）。 */
export function hiddenPushed(c: Changed): Map<string, SessionHidden | null> {
  const got = new Map<string, SessionHidden | null>();
  for (const cell of c.cells) {
    if (cell.key === null || cell.body === undefined || cell.body === null) continue;
    got.set(cell.key, (cell.body as { hidden: SessionHidden | null }).hidden);
  }
  return got;
}

/** 会话事实里血缘那两格。 */
export interface SessionLineage {
  startedBy: Kin | null;
  children: Kin[];
}

export const NO_LINEAGE: SessionLineage = { startedBy: null, children: [] };

/** 会话事实（`history-facts` 那一份）⇒ 血缘两格。 */
export function lineageOf(facts: Pick<SessionFacts, "startedBy" | "children">): SessionLineage {
  return facts.startedBy === null && facts.children.length === 0 ? NO_LINEAGE : { startedBy: facts.startedBy, children: facts.children };
}

/** 两份血缘比一比。 */
export function sameLineage(a: SessionLineage, b: SessionLineage): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** 单个那一格：隐藏 · 取消隐藏（不按规则）· 回到规则。 */
export type HideSet = HideAsk;

/** 这台偏好里的两格：子会话 · 分身会话默认进不进标签栏。 */
export type TabPlace = Tab;
export type TabPlaces = Pick<Prefs, "childTab" | "bgTab">;

/** 读回的那份偏好 ⇒ 两格（后端总回这两格的实际值）。 */
export function placesOf(prefs: Prefs): TabPlaces {
  return { childTab: prefs.childTab, bgTab: prefs.bgTab };
}
