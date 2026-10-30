/**
 * 每台机器状态成品的推送：各台（含本机，键 `<local>`）是本机后端的 `changed {machine, key: 那台, body: 成品}`
 * （订上 · 又接上 · 漏了几格 · 帧里没带成品 ⇒ 整份重问 `machine-read`）；哪一态、那一句、修法都是后端定的（`deploy::machine`）。
 * 本机后端起没起来是壳自己的事实，走壳的 `local-backend` 事件（{@link LocalBackend}，与 `backend_status` 的 `local` 同一份类型），不冒充哪一台的成品。
 * 那台一变就来一帧，界面不轮询；设置窗与主窗口都从这里订。
 */
import { listen } from "@tauri-apps/api/event";
import { pushedProducts } from "./changed-stream";
import { subscribeTopics } from "./events";
import { budgetWithin, callTyped, noteMachine } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { commands } from "./ipc/commands";
import type { LocalBackend } from "./generated/LocalBackend";
import type { MachineState } from "./generated/MachineState";
import type { EmptyFace } from "./generated/EmptyFace";
import { LOCAL_BACKEND } from "./generated/shell-names";

/** `machine-read` 等多久。 */
const READ_BUDGET_MS = 5_000;

/**
 * 订本机后端起没起来：壳每推一帧回调一次；订上之后再问一次 `backend_status` 的 `local`（起来那一趟的几帧多半早于订上，
 * 起不来那一句只在那几帧里）。推来的先到 ⇒ 问回来的不盖它。回退订函数。
 */
export function onLocalBackend(cb: (local: LocalBackend) => void): () => void {
  let off: (() => void) | null = null;
  let dropped = false;
  let pushed = false;
  try {
    listen<LocalBackend>(LOCAL_BACKEND, (e) => {
      pushed = true;
      cb(e.payload);
    }).then(
      (u) => {
        if (dropped) u();
        else off = u;
      },
      (e: unknown) => console.warn("[local-backend] 订不上推送：", e),
    );
  } catch (e) {
    // 没有事件通道（宿主不是壳）⇒ 只靠打开 / 刷新时问 `backend_status`。
    console.warn("[local-backend] 订不上推送：", e);
  }
  commands.backend_status({ origin: LOCAL_ORIGIN }).then(
    (s) => {
      if (!dropped && !pushed && s.local) cb(s.local);
    },
    (e: unknown) => console.warn("[local-backend] 问不到壳那一格：", e),
  );
  return () => {
    dropped = true;
    off?.();
  };
}

/**
 * 问一次本机后端各台此刻的成品（`machine-read`，含本机），每台先记下名字与离线那一句；问不到 ⇒ 抛。
 * 只要此刻那一份、不跟着变的（起新会话框的机器清单）用它；要跟着变的订 {@link onMachineState}。
 */
export async function readMachines(): Promise<Record<string, MachineState>> {
  const r = await callTyped(LOCAL_ORIGIN, "machine-read", {}, budgetWithin(READ_BUDGET_MS));
  for (const [origin, m] of Object.entries(r.machines)) noteMachine(origin, m);
  return r.machines;
}

/**
 * 订一份：每来一台的一帧回调一次（先记下那台的名字与离线那一句，`machineName` · `offlineSaid` 照它）；
 * 给了 `face` ⇒ 首屏那一句（`EmptyFace`，本机后端合各台写好的，`changed {empty}` · `machine-read` 回包的 `empty`）每来一次也回调。回退订函数。
 */
export function onMachineState(each: (origin: string, machine: MachineState) => void, face?: (f: EmptyFace) => void): () => void {
  const cb = (origin: string, machine: MachineState): void => {
    noteMachine(origin, machine);
    each(origin, machine);
  };
  let unsub: (() => void) | null = null;
  let dropped = false;
  const readAll = async (): Promise<void> => {
    try {
      const r = await callTyped(LOCAL_ORIGIN, "machine-read", {}, budgetWithin(READ_BUDGET_MS));
      if (dropped) return;
      for (const [origin, m] of Object.entries(r.machines)) cb(origin, m);
      face?.(r.empty);
    } catch (e) {
      console.warn("[machine-state] 问不到远端各台：", e);
    }
  };
  subscribeTopics([LOCAL_ORIGIN], face ? ["machine", "empty"] : ["machine"], (_o, changes) => {
    if (dropped) return;
    const c = changes.get("machine");
    const e = changes.get("empty");
    const machines = c ? pushedProducts(c, "machine") : null;
    const faces = e ? pushedProducts(e, "empty") : null;
    if (machines?.all || (machines?.ask.length ?? 0) > 0 || faces?.all || (faces?.ask.length ?? 0) > 0) void readAll();
    // 帧里那一份就是 `machine-read` 回包里那一台 · 那一句。
    for (const [key, m] of machines?.got ?? []) if (key !== null && key !== "") cb(key, m);
    const f = faces?.got.get(null);
    if (f) face?.(f);
  })
    .then(
      (stop) => {
        if (dropped) return stop();
        unsub = stop;
        // 订上之后问一次：那台早已接着时壳不再交 `seen`（只在接没接着变了时说），等它就一台都拿不到。
        void readAll();
      },
      (e: unknown) => console.warn("[machine-state] 订不上各台：", e),
    );
  return () => {
    dropped = true;
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
