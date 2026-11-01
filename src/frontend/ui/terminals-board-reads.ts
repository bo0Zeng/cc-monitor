/**
 * 「终端」视图那张名单的读写口：本机后端 `terminals-board` 问到各台、合成一份再答（每台一组 · 行排好字写好 · 筛选四格带数）；
 * 结束几行：交本机后端一条 `terminals-end`（按名单里的句柄，跨几台也只这一发；两种宿主同一条，有没有会话都一样），回包照摆。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { settleTyped } from "./control-said";
import { copyText } from "./copy-table";
import { endTerminals } from "./terminal-reads";
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { failToast } from "./kit/toast";
import { askEnd, sayReply } from "./end-confirm";
import type { TerminalsBoard } from "./generated/TerminalsBoard";
import type { BoardRow } from "./generated/BoardRow";

/** 本机后端问到各台（各台起一两发 tmux ＋ 回程）再合：等这么久。 */
const BOARD_BUDGET_MS = 20_000;

/** 问这几台、合一份（同抽屉终端页那一问，不自报前端）。失败 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function readBoard(machines: readonly Origin[]): Promise<TerminalsBoard> {
  const req = { machines: [...machines] };
  return settleTyped(LOCAL_ORIGIN, "terminals-board", callTyped(LOCAL_ORIGIN, "terminals-board", req, budgetWithin(BOARD_BUDGET_MS)));
}

/** 名单里的一行连它在哪台。 */
export interface BoardPick {
  origin: Origin;
  row: BoardRow;
}

/** 结束这几行：先确认（结束会话那一个框），再交本机后端一条 `terminals-end`；那一句 · 逐条 · 复制详情照回包摆。问不到本机后端 ⇒ 那一句。 */
export async function endRows(items: readonly BoardPick[], ask: ConfirmFn = confirmDialog): Promise<void> {
  if (!(await askEnd(items.map((i) => i.row.title), ask))) return;
  try {
    sayReply(await endTerminals(items.map((i) => ({ machine: i.origin, terminal: i.row.terminal }))));
  } catch (e) {
    failToast(copyText("terminalsView.select.end"), e);
  }
}
