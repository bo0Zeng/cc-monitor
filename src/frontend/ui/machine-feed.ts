/**
 * 每台机器状态成品的推送（壳的 `machine-state` 事件：`{origin, machine}`，`machine` 与 `backend_status` 那一格同形）。
 * 那台一变就来一帧（正在连 / 装 / 更新、连上、没连上、指纹不对 ……），界面不轮询；设置窗与主窗口都从这里订。
 */
import { listen } from "@tauri-apps/api/event";
import { decodeMachineState, type MachineState } from "./machine-state-decode";

/** 壳那一侧的事件名（`ui_contract::events::MACHINE_STATE`）。 */
export const MACHINE_STATE_EVENT = "machine-state";

/** 订一份：每来一帧回调一次（收不下 ⇒ `machine` 为 `null`）。回退订函数。 */
export function onMachineState(cb: (origin: string, machine: MachineState | null) => void): () => void {
  let off: (() => void) | null = null;
  let dropped = false;
  try {
    listen<{ origin?: unknown; machine?: unknown }>(MACHINE_STATE_EVENT, (e) => {
      const origin = e.payload?.origin;
      if (typeof origin !== "string" || origin === "") return;
      cb(origin, decodeMachineState(e.payload.machine));
    }).then(
      (u) => {
        if (dropped) u();
        else off = u;
      },
      (e: unknown) => console.warn("[machine-state] 订不上推送：", e),
    );
  } catch (e) {
    // 没有事件通道（宿主不是壳）⇒ 只靠打开 / 刷新时重问。
    console.warn("[machine-state] 订不上推送：", e);
  }
  return () => {
    dropped = true;
    off?.();
  };
}
