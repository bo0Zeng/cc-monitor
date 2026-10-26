/**
 * 每台机器状态成品的推送：本机那一行是壳的 `machine-state` 事件（{@link MachineStatePayload}，与 `backend_status` 的 `machine` 同一份类型）；
 * 远端各台是本机后端的 `changed {machine, key: 那台, body: 成品}`（订上 · 又接上 · 漏了几格 · 帧里没带成品 ⇒ 整份重问 `machine-read`）。
 * 那台一变就来一帧，界面不轮询；设置窗与主窗口都从这里订。哪一态、那一句、修法都是后端定的（`deploy::machine`）。
 */
import { listen } from "@tauri-apps/api/event";
import { pushedProducts } from "./changed-stream";
import { subscribeTopics } from "./events";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import type { MachineState } from "./generated/MachineState";
import type { MachineStatePayload } from "./generated/MachineStatePayload";

/** 壳那一侧的事件名（`ui_contract::events::MACHINE_STATE`）。 */
export const MACHINE_STATE_EVENT = "machine-state";

/** `machine-read` 等多久。 */
const READ_BUDGET_MS = 5_000;

/** 订一份：每来一台的一帧回调一次。回退订函数。 */
export function onMachineState(cb: (origin: string, machine: MachineState) => void): () => void {
  let off: (() => void) | null = null;
  let unsub: (() => void) | null = null;
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
  subscribeTopics([LOCAL_ORIGIN], ["machine"], (_o, changes) => {
    const c = changes.get("machine");
    if (dropped || !c) return;
    const { got, ask, all } = pushedProducts(c, "machine");
    if (all || ask.length > 0) void readAll();
    // 帧里那一份就是 `machine-read` 回包里那一台。
    for (const [key, m] of got) if (key !== null && key !== "") cb(key, m);
  })
    .then(
      (stop) => {
        if (dropped) return stop();
        unsub = stop;
        // 订上之后问一次：那台早已接着时壳不再交 `seen`（只在接没接着变了时说），等它就一台都拿不到。
        void readAll();
      },
      (e: unknown) => console.warn("[machine-state] 订不上远端各台：", e),
    );
  return () => {
    dropped = true;
    off?.();
    unsub?.();
  };
}

/** 那台变成连着的那一刻（起着 / 断开之后接上）叫一次；连着时再来的帧不叫。回退订函数。 */
export function onMachineUp(cb: (origin: string) => void): () => void {
  const up = new Set<string>();
  return onMachineState((origin, m) => {
    if (m.state !== "up") {
      up.delete(origin);
      return;
    }
    if (up.has(origin)) return;
    up.add(origin);
    cb(origin);
  });
}
