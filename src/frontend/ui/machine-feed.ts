/**
 * 每台机器状态成品的推送：本机那一行是壳的 `machine-state` 事件（`{origin, machine}`，与 `backend_status` 那一格同形）；
 * 远端各台是本机后端的 `changed {machine, key: 那台, body: 成品}`（订上 · 又接上 · 漏了几格 ⇒ 整份重问 `machine-read`）。
 * 那台一变就来一帧，界面不轮询；设置窗与主窗口都从这里订。
 */
import { listen } from "@tauri-apps/api/event";
import { chan, type Sub } from "../../comms/inward/chan";
import { CHANGED_WINDOW, changedItems, changedStream } from "./changed-stream";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { decodeMachineState, type MachineState } from "./machine-state-decode";

/** 壳那一侧的事件名（`ui_contract::events::MACHINE_STATE`）。 */
export const MACHINE_STATE_EVENT = "machine-state";

/** `machine-read` 等多久。 */
const READ_BUDGET_MS = 5_000;

/** 订一份：每来一台的一帧回调一次（收不下 ⇒ `machine` 为 `null`）。回退订函数。 */
export function onMachineState(cb: (origin: string, machine: MachineState | null) => void): () => void {
  let off: (() => void) | null = null;
  let sub: Sub | null = null;
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
  const readAll = async (): Promise<void> => {
    try {
      const r = await callTyped(LOCAL_ORIGIN, "machine-read", {}, budgetWithin(READ_BUDGET_MS));
      for (const [origin, m] of Object.entries(r?.machines ?? {})) if (!dropped) cb(origin, decodeMachineState(m));
    } catch (e) {
      console.warn("[machine-state] 问不到远端各台：", e);
    }
  };
  chan
    .subscribe(LOCAL_ORIGIN, changedStream("machine"), null, CHANGED_WINDOW, (items) => {
      const { cells, all, frames } = changedItems(items);
      if (frames > 0) sub?.want(frames);
      if (all) void readAll();
      for (const c of cells) if (c.key !== null && c.key !== "") cb(c.key, decodeMachineState(c.body));
    })
    .then(
      (s) => {
        if (dropped) return s.stop();
        sub = s;
        // 订上之后问一次：那台早已接着时壳不再交 `seen`（只在接没接着变了时说），等它就一台都拿不到。
        void readAll();
      },
      (e: unknown) => console.warn("[machine-state] 订不上远端各台：", e),
    );
  return () => {
    dropped = true;
    off?.();
    sub?.stop();
  };
}

/** 那台变成连着的那一刻（起着 / 断开之后接上）叫一次；连着时再来的帧不叫。回退订函数。 */
export function onMachineUp(cb: (origin: string) => void): () => void {
  const up = new Set<string>();
  return onMachineState((origin, m) => {
    if (m?.state !== "up") {
      up.delete(origin);
      return;
    }
    if (up.has(origin)) return;
    up.add(origin);
    cb(origin);
  });
}
