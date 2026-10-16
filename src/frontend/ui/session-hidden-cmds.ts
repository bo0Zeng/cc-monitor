/**
 * **会话隐藏：交给那台的几条命令**（改单个那一格 · 这台偏好那两格）。形状住 `session-hidden.ts`（后端生成的类型）。
 * 同一条注解命令顺带「没人答时自动答」那一格（会话头菜单交的）。
 */
import { annotate } from "./history-reads";
import type { Origin } from "./ipc/origin";
import { setPrefs } from "./machine-prefs";
import type { Hide } from "./generated/Hide";
import { placesOf, type HideSet, type TabPlace, type TabPlaces } from "./session-hidden";

/**
 * 改单个那一格（注解住会话所在那台）。成不成都由推帧那一格说了算：界面不先挪。标签栏 · 历史页同这一个口。
 * 回改之前那一格（后端读写那一处给的）：撤销「隐藏」照它交回去（`session-hidden.ts::undoHide`）。
 */
export async function setHide(origin: Origin, sid: string, to: HideSet): Promise<Hide | null> {
  return (await annotate(origin, sid, { hide: to })).prev;
}

/** 改那台的一格，回写完读回的两格。 */
export async function setPlace(origin: Origin, which: keyof TabPlaces, to: TabPlace): Promise<TabPlaces> {
  const r = await setPrefs(origin, { [which]: to });
  return placesOf(r.prefs);
}

/** 「没人答时自动答」那一格（`0` ＝ 关；那一句不动）。改完那台推 `changed {needs}`，会话事实跟着重读。 */
export async function setAutoAnswer(origin: Origin, sid: string, min: number): Promise<void> {
  await annotate(origin, sid, { autoAnswer: { afterMin: min } });
}
