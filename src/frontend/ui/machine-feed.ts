/**
 * 每台机器状态成品的推送：本机那一行是壳的 `machine-state` 事件（{@link MachineStatePayload}，与 `backend_status` 的 `machine` 同一份类型）；
 * 远端各台是本机后端的 `changed {machine, key: 那台, body: 成品}`（接上 · 漏了几格 · 帧里没带成品 ⇒ 整份重问 `machine-read`）。
 * 那台一变就来一帧，界面不轮询；设置窗与主窗口都从这里订。哪一态、那一句、修法都是后端定的（`deploy::machine`）。
 */
import { listen } from "@tauri-apps/api/event";
import { chan, type Sub } from "../../comms/inward/chan";
import { CHANGED_WINDOW, changedItems, changedStream } from "./changed-stream";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import type { MachineRead } from "./generated/MachineRead";
import type { MachineState } from "./generated/MachineState";
import type { MachineStatePayload } from "./generated/MachineStatePayload";

/** 壳那一侧的事件名（`ui_contract::events::MACHINE_STATE`）。 */
export const MACHINE_STATE_EVENT = "machine-state";

/** `machine-read` 等多久。 */
const READ_BUDGET_MS = 5_000;

/** 订一份：每来一台的一帧回调一次。回退订函数。 */
export function onMachineState(cb: (origin: string, machine: MachineState) => void): () => void {
  let off: (() => void) | null = null;
  let sub: Sub | null = null;
  let dropped = false;
  try {
    listen<MachineStatePayload>(MACHINE_STATE_EVENT, (e) => cb(e.payload.origin, e.payload.machine)).then(
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
      for (const [origin, m] of Object.entries(r.machines)) if (!dropped) cb(origin, m);
    } catch (e) {
      console.warn("[machine-state] 问不到远端各台：", e);
    }
  };
  chan
    .subscribe(LOCAL_ORIGIN, changedStream("machine"), null, CHANGED_WINDOW, (items) => {
      const { cells, all, frames } = changedItems(items);
      if (frames > 0) sub?.want(frames);
      const keyed = cells.flatMap((c) => (c.key !== null && c.key !== "" ? [{ key: c.key, body: c.body }] : []));
      if (all || keyed.some((c) => c.body === undefined)) void readAll();
      // 帧里那一份就是 `machine-read` 回包里那一台。
      for (const c of keyed) if (c.body !== undefined) cb(c.key, c.body as MachineRead["machines"][string]);
    })
    .then(
      (s) => {
        if (dropped) s.stop();
        else sub = s;
      },
      (e: unknown) => console.warn("[machine-state] 订不上远端各台：", e),
    );
  return () => {
    dropped = true;
    off?.();
    sub?.stop();
  };
}
