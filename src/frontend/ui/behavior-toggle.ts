/**
 * 快捷键翻「自动跟随」「自动切到前台」：写盘 → 主窗口用上 → 右下角说一句翻成了什么 → 告诉设置窗（开着的那一页开关跟着变）。
 */
import { emit } from "@tauri-apps/api/event";
import { getBehavior, setBehavior, type BehaviorConfig } from "./behavior";
import { BEHAVIOR_TOGGLED_EVENT, type BehaviorToggled } from "./settings/events";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";

export type BehaviorSwitch = "autoFollowUserActive" | "bringMonitorToFrontOnUserActive";

/** 翻完之后那一句。 */
function said(which: BehaviorSwitch, on: boolean): string {
  if (which === "autoFollowUserActive") return on ? copyText("behaviorToggle.autoFollow.on") : copyText("behaviorToggle.autoFollow.off");
  return on ? copyText("behaviorToggle.bringFront.on") : copyText("behaviorToggle.bringFront.off");
}

/** 翻一格；`apply` = 主窗口用上新值（`TabManager.applyBehavior`）。 */
export async function flipBehavior(which: BehaviorSwitch, apply: (b: BehaviorConfig) => void): Promise<void> {
  const cur = await getBehavior();
  const next: BehaviorConfig = { ...cur, [which]: !cur[which] };
  await setBehavior(next);
  apply(next);
  toast(said(which, next[which]), "", { level: "info" });
  const payload: BehaviorToggled = {
    autoFollowUserActive: next.autoFollowUserActive,
    bringMonitorToFrontOnUserActive: next.bringMonitorToFrontOnUserActive,
  };
  await emit(BEHAVIOR_TOGGLED_EVENT, payload).catch((e: unknown) => console.warn("[behavior] 通知不到设置窗：", e));
}
