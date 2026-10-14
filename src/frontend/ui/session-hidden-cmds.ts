/**
 * **会话隐藏：交给那台的几条命令**（改单个那一格 · 这台偏好那两格）。形状与解码住 `session-hidden.ts`。
 * 〔对类型〕补丁与偏好那两格照设计稿手写、经 `as unknown as` 交出去；后端那一路的生成物进来以后删掉这两处转换。
 */
import { annotate } from "./history-reads";
import type { Origin } from "./ipc/origin";
import { setPrefs, type PrefsSet } from "./machine-prefs";
import type { Patch } from "./generated/Patch";
import { placesOf, type HideSet, type TabPlace, type TabPlaces } from "./session-hidden";

/** 改单个那一格（注解住会话所在那台）。成不成都由推帧那一格说了算：界面不先挪。 */
export async function setHide(origin: Origin, sid: string, to: HideSet): Promise<void> {
  await annotate(origin, sid, { hide: to } as unknown as Patch);
}

/** 改那台的一格，回写完读回的两格。 */
export async function setPlace(origin: Origin, which: keyof TabPlaces, to: TabPlace): Promise<TabPlaces> {
  const r = await setPrefs(origin, { [which]: to } as unknown as PrefsSet);
  return placesOf(r.prefs);
}
