/**
 * 停下 / 重启 / 更新 / 卸载那台之前「会打断什么」：只问本机后端一条 `machine-interrupts`（`machine` = 那台，`act` = 停多久）——
 * 它经那台的常驻流问到那台的活会话（经它中转的 · 活着的），再数自己账上通往那台的端口转发，并好、排成对话框里的两行（中断 · 保留）写好再答（`rows`）。
 * 重启 cc-monitor：同一条带 `appExit` 与 `fleet`，另带壳现读的各文件窗口那两个数（`filewin_interrupts`，问不到就不带 ⇒ 后端说数不出）。
 * 什么都不会断 ⇒ 空，调用方就不弹框；本机后端问不到 ⇒ 框里只说一句「问不到」。这里不判、不拼，只照摆。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL, isLocalOrigin } from "../ipc/origin";
import { copyText } from "../copy-table";
import { commands } from "../ipc/commands";
import type { DialogRow } from "../kit/dialog";
import type { InterruptAct } from "../generated/InterruptAct";
import type { MachineInterruptsReply } from "../generated/MachineInterruptsReply";
import type { MachineInterruptsReq } from "../generated/MachineInterruptsReq";

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

/** 回包里写好的那两行照摆；问不到 ⇒ 只那一句（与壳关主窗那一下同一句）。 */
function rowsOf(got: MachineInterruptsReply | null): DialogRow[] {
  if (got === null) return [{ label: copyText("quitNow.dialog.unknown"), items: [] }];
  return got.rows.map((r) => ({ label: r.label, items: [...r.items] }));
}

/** 停 / 重启 / 更新 / 卸载那台之前：会打断什么那两行（本机那一台不带名字）。 */
export async function askInterruptRows(origin: string, act: InterruptAct): Promise<DialogRow[]> {
  return rowsOf(await askHub(isLocalOrigin(origin) ? { act } : { machine: origin, act }));
}

/** 本机账上通往那台的端口转发几条（删那一台的那一句用）；问不到 ⇒ 0（那一句不带）。 */
export async function askForwards(origin: string): Promise<number> {
  return (await askHub({ machine: origin }))?.forwards ?? 0;
}

/** 各文件窗口手上的（壳现读那张表）；问不到 ⇒ `null`（不带，后端说数不出）。 */
async function askFileWindows(): Promise<{ unsaved: number; running: number } | null> {
  try {
    return await commands.filewin_interrupts();
  } catch (e) {
    console.warn("[interrupts] 问不到文件窗口会打断什么：", e);
    return null;
  }
}

/**
 * 「现在重启 cc-monitor」之前：问本机后端一条（`appExit` ＋ `fleet` ＋ 文件窗口那两个数）——它照自己的机器表问到各台、加起来、排好两行。
 */
export async function askAppExitInterruptRows(): Promise<DialogRow[]> {
  const files = await askFileWindows();
  const req: MachineInterruptsReq = { appExit: true, fleet: true, act: "restart" };
  if (files !== null) req.files = { unsaved: files.unsaved, running: files.running };
  return rowsOf(await askHub(req));
}
