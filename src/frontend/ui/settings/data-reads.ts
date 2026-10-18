/**
 * 「文件与数据」那一份成品（那台后端的 `data-report`，typed：形状由握手指纹核过）：按机器问。
 * 本机那一栏先问 monitor 一次它自己进程的那几条事实（`footprint_client_facts`），原样带给本机后端（同足迹那一问）。
 * 几台一起（「文件与数据」页 · 状态栏「待办 N」）：本机后端 `chores-merge` 自己问到各台的 `data-report`、件数合成一份再答（[`readChores`]），
 * 界面不逐台问、不另数。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import type { ChoresMerged } from "../generated/ChoresMerged";
import type { ChoresMarkReq } from "../generated/ChoresMarkReq";
import type { DataReport } from "../generated/DataReport";
import type { FootprintClient } from "../generated/FootprintClient";
import { commands } from "../ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { recallSeen, rememberSeen } from "../last-seen";
import { saidOf } from "../kit/detail";

export type { DataReport } from "../generated/DataReport";
export type { Chore } from "../generated/Chore";
export type { ChoreKind } from "../generated/ChoreKind";
export type { ChoreState } from "../generated/ChoreState";
export type { ChoreCounts } from "../generated/ChoreCounts";
export type { OwnItem } from "../generated/OwnItem";

/** 一次 stat 一批、读几份小文件：给 30 秒。 */
const DATA_BUDGET_MS = 30_000;
/** 记一个选择：读—改—写一份小文件，10 秒。 */
const MARK_BUDGET_MS = 10_000;

/** 记下「待办」里的一个选择（那台后端写它自己的 `~/.cc-monitor/chores.json`）。失败抛一句人话。 */
export async function markChore(origin: Origin, args: ChoresMarkReq): Promise<void> {
  const target = isLocalOrigin(origin) ? LOCAL_ORIGIN : origin;
  try {
    await callTyped(target, "chores-mark", args, budgetWithin(MARK_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
}

/** 问那台（本机带 monitor 那几条事实）。问不到 / 形状不对 ⇒ 抛一句人话。 */
export async function readDataReport(origin: Origin): Promise<DataReport> {
  const local = isLocalOrigin(origin);
  // monitor 那几条事实本侧不认识形状、原样带过去（monitor `footprint_client.rs` 写，后端按 `FootprintClient` 收，多一格就 bad_args）。
  const req = local ? { client: (await commands.footprint_client_facts()) as unknown as FootprintClient } : {};
  let report: DataReport;
  try {
    report = await callTyped(local ? LOCAL_ORIGIN : origin, "data-report", req, budgetWithin(DATA_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
  void rememberSeen(origin, "data", report);
  return report;
}

/** 那台上次读成的那一份（本机后端记着的，跨重启还在；后端按回包类型核过）；没有 / 存档不是这一版 ⇒ `null`。 */
export async function recallDataReport(origin: Origin): Promise<{ report: DataReport; atMs: number } | null> {
  const seen = await recallSeen(origin, "data");
  if (seen === null) return null;
  if (seen.state === "unreadable") {
    console.warn(`[data] ${origin} 记下的上次值不是这一版，读不懂`);
    return null;
  }
  return { report: seen.value, atMs: seen.atMs };
}

/**
 * 问本机后端：它自己问到这几台的 `data-report`（本机那一台带 monitor 那几条事实，别的台经它的连接问那台的常驻）、件数合成一份；
 * 每台读成的那一份记成那台「上次的」。没问到的那台带那一句（`said`）。本机后端问不到 ⇒ 抛一句人话。
 * 期限：各台同时问，一台一次 stat 一批、读几份小文件 ＋ 回程，30 秒。
 */
export async function readChores(machines: Origin[]): Promise<ChoresMerged> {
  const client = machines.some(isLocalOrigin) ? ((await commands.footprint_client_facts()) as unknown as FootprintClient) : null;
  let got: ChoresMerged;
  try {
    got = await callTyped(LOCAL_ORIGIN, "chores-merge", { machines, client }, budgetWithin(DATA_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
  for (const m of got.machines) if (m.report) void rememberSeen(m.origin, "data", m.report);
  return got;
}
