/**
 * 监控板的读口：本机后端 `monitor-board` 问到各台、合成一份再答（各台一组 · 组内排好 · 组头与顶上那一句写好）。
 * 界面只交「这一窗此刻列着哪几条」，版面照回包摆。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { settleTyped } from "./control-said";
import type { MonitorAsk } from "./generated/MonitorAsk";
import type { MonitorBoard } from "./generated/MonitorBoard";

/** 本机后端问到各台再合：等这么久。 */
const BOARD_BUDGET_MS = 20_000;

/** 问这几条、合一份。失败 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function readMonitorBoard(sessions: readonly MonitorAsk[]): Promise<MonitorBoard> {
  const req = { sessions: [...sessions] };
  return settleTyped(LOCAL_ORIGIN, "monitor-board", callTyped(LOCAL_ORIGIN, "monitor-board", req, budgetWithin(BOARD_BUDGET_MS)));
}
