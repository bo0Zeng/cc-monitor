/**
 * 停下 / 重启 / 更新 / 卸载那台之前「会打断什么」：只问本机后端一条 `machine-interrupts`（`machine` = 那台）——
 * 它经那台的常驻流问到那台的活会话（经它中转的 · 活着的），再数自己账上通往那台的端口转发，并好再答；排成对话框里的两行（中断 · 保留）。
 * 退出 / 重启 cc-monitor：同一条带 `appExit` 与各台名单，本机后端问到各台、加起来答；另有一份各文件窗口手上没存的 · 在跑的（壳现读它那张表，`filewin_interrupts`）。
 * 什么都不会断 ⇒ 空，调用方就不弹框。这里不判什么，只排版。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL, isLocalOrigin } from "../ipc/origin";
import { copyText } from "../copy-table";
import { commands } from "../ipc/commands";
import type { DialogRow } from "../kit/dialog";
import type { MachineInterruptsReply } from "../generated/MachineInterruptsReply";
import type { MachineInterruptsReq } from "../generated/MachineInterruptsReq";

/** 那几份事实并起来（问不到的那一格是 `null`，不当成 0）。 */
export interface Interrupts {
  relayedSessions: number | null;
  relayedMaybe: number;
  liveStreams: number;
  forwards: number | null;
  /** 文件窗口里没存的改动几份（只有退出 / 重启 cc-monitor 那一问数它；别的问不带这一格）。 */
  unsaved?: number | null;
  /** 文件窗口「进度」表里还在跑的几趟（同上）。 */
  filewinRunning?: number | null;
}

/** 那一问的期限：本机后端问到那台（那台扫一遍活会话 ＋ 回程）再答。 */
const INTERRUPTS_BUDGET_MS = 10_000;

/** 问本机后端那一条；问不到 ⇒ `null`。 */
async function askHub(req: MachineInterruptsReq): Promise<MachineInterruptsReply | null> {
  try {
    return await callTyped(LOCAL, "machine-interrupts", req, budgetWithin(INTERRUPTS_BUDGET_MS));
  } catch (e) {
    console.warn("[interrupts] 本机后端问不到会打断什么：", e);
    return null;
  }
}

/** 后端那一份 ⇒ 界面那几格（本机后端问不到 ⇒ 会话与转发都 `null`，说数不出）。 */
function fromHub(got: MachineInterruptsReply | null): Interrupts {
  return {
    relayedSessions: got?.relayedSessions ?? null,
    relayedMaybe: got?.relayedMaybe ?? 0,
    liveStreams: got?.liveStreams ?? 0,
    forwards: got === null ? null : got.forwards,
  };
}

/** 那台的会话三格 ＋ 本机账上通往那台的转发：问本机后端一条（本机那一台不带名字）。 */
export async function askInterrupts(origin: string): Promise<Interrupts> {
  return fromHub(await askHub(isLocalOrigin(origin) ? {} : { machine: origin }));
}

/** 各文件窗口手上的（壳现读那张表）；问不到 ⇒ `null`（说数不出）。 */
async function askFileWindows(): Promise<{ unsaved: number; running: number } | null> {
  try {
    return await commands.filewin_interrupts();
  } catch (e) {
    console.warn("[interrupts] 问不到文件窗口会打断什么：", e);
    return null;
  }
}

/**
 * 「现在重启 cc-monitor」之前：问本机后端一条（`appExit` ＋ 各台名单）——它问到各台（各台照它自己的退出行为答会不会跟着停）、加起来；
 * 远端问不到 ⇒ 那台本来就没连着，不算（后端那一侧定）。本机后端问不到 ⇒ 说数不出。文件窗口那两格随 cc-monitor 一起没（照算）。
 */
export async function askAppExitInterrupts(remotes: readonly string[]): Promise<Interrupts> {
  const [files, got] = await Promise.all([askFileWindows(), askHub({ appExit: true, machines: [...remotes] })]);
  return {
    ...fromHub(got),
    unsaved: files?.unsaved ?? null,
    filewinRunning: files?.running ?? null,
  };
}

/** 那一下要停多久：`stop` / `uninstall` 停到再启动；`restart` / `update` 停几秒。 */
export type InterruptAct = "stop" | "restart" | "update" | "uninstall";

/** 会断的那几项排成「中断 · 保留」两行；一项都没有 ⇒ `[]`。 */
export function interruptRows(i: Interrupts | null, machine: string, act: InterruptAct): DialogRow[] {
  const brief = act === "restart" || act === "update";
  const items: string[] = [];
  if (i !== null && (i.unsaved === null || i.filewinRunning === null)) items.push(copyText("interrupts.item.filewinUnknown"));
  if (i?.unsaved != null && i.unsaved > 0) items.push(copyText("interrupts.item.unsaved", { n: i.unsaved }));
  if (i?.filewinRunning != null && i.filewinRunning > 0) items.push(copyText("interrupts.item.filewinRunning", { n: i.filewinRunning }));
  if (i === null || i.relayedSessions === null) items.push(copyText("interrupts.item.relayedUnknown"));
  else if (i.relayedSessions > 0) items.push(copyText("interrupts.item.relayed", { n: i.relayedSessions }));
  if (i !== null && i.relayedMaybe > 0) items.push(copyText("interrupts.item.relayedMaybe", { n: i.relayedMaybe }));
  if (i !== null && i.liveStreams > 0) {
    items.push(brief ? copyText("interrupts.item.liveBrief", { machine }) : copyText("interrupts.item.liveGone", { machine }));
  }
  if (i !== null && i.forwards === null) items.push(copyText("interrupts.item.forwardsUnknown"));
  else if (i !== null && i.forwards !== null && i.forwards > 0) items.push(copyText("interrupts.item.forwards", { n: i.forwards }));
  if (items.length === 0) return [];
  return [
    { label: brief ? copyText("interrupts.row.brief") : copyText("interrupts.row.untilStart"), items },
    { label: copyText("interrupts.row.keep"), items: [copyText("interrupts.item.sessionsRun")] },
  ];
}
