/**
 * **会话隐藏：交给那台的几条命令**（改单个那一格 · 这台偏好那两格）。形状住 `session-hidden.ts`（后端生成的类型）。
 */
import { annotate } from "./history-reads";
import type { Origin } from "./ipc/origin";
import { setPrefs } from "./machine-prefs";
import { placesOf, type HideSet, type TabPlace, type TabPlaces } from "./session-hidden";

/** 改单个那一格（注解住会话所在那台）。成不成都由推帧那一格说了算：界面不先挪。标签栏 · 历史页同这一个口。 */
export async function setHide(origin: Origin, sid: string, to: HideSet): Promise<void> {
  await annotate(origin, sid, { hide: to });
}

/** 改那台的一格，回写完读回的两格。 */
export async function setPlace(origin: Origin, which: keyof TabPlaces, to: TabPlace): Promise<TabPlaces> {
  const r = await setPrefs(origin, { [which]: to });
  return placesOf(r.prefs);
}
