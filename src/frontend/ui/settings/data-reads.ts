/**
 * 「文件与数据」那一份成品（那台后端的 `data-report`，typed：形状由握手指纹核过）：按机器问。
 * 本机那一栏先问 monitor 一次它自己进程的那几条事实（`footprint_client_facts`），原样带给本机后端（同足迹那一问）。
 * 设置窗左栏角标与主窗口状态栏那一枚读同一个数（[`choresOf`]），界面不另数。
 */
import { budgetWithin, callTyped, ReplyUnreadable, saidFrom } from "../ipc/chan-caller";
import type { ChoresMarkReq } from "../generated/ChoresMarkReq";
import type { DataReport } from "../generated/DataReport";
import type { Chore } from "../generated/Chore";
import type { FootprintClient } from "../generated/FootprintClient";
import { commands } from "../ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { exactKeys, isObj } from "../ipc/decode";
import { recallSeen, rememberSeen } from "../last-seen";

export type { DataReport } from "../generated/DataReport";
export type { Chore } from "../generated/Chore";
export type { ChoreKind } from "../generated/ChoreKind";
export type { ChoreState } from "../generated/ChoreState";
export type { OwnItem } from "../generated/OwnItem";

/** 一次 stat 一批、读几份小文件：给 30 秒。 */
const DATA_BUDGET_MS = 30_000;
/** 记一个选择：读—改—写一份小文件，10 秒。 */
const MARK_BUDGET_MS = 10_000;

// ── 上次值那一份（本机后端记着的、那台上一次读成的应答）：可能是旧版那台记下的形状，握手指纹管不到 ⇒ 这里按形状收。
//    现问的那一份不经这里（typed：形状由握手指纹核过）。

const bad = (): never => {
  throw new ReplyUnreadable("data-report reply shape");
};
const str = (v: unknown): string => (typeof v === "string" ? v : bad());

function undoOk(v: unknown): boolean {
  if (v === null) return true;
  if (!isObj(v) || typeof v.page !== "string") return false;
  return Object.keys(v).every((k) => k === "page" || ((k === "tab" || k === "anchor") && typeof v[k] === "string"));
}

const KINDS: readonly string[] = ["must", "install", "decide", "installOptional", "optional"];
const STATES: readonly string[] = ["todo", "done", "expired", "blocked", "declined"];
const ACTIONS: readonly string[] = ["copyCommand", "copySnippet", "decide", "locate", "how", "installFirst"];
const CHORE_KEYS = ["id", "kind", "state", "name", "loc", "said", "why", "steps", "diff", "copy", "whole", "wholeCovers", "file", "go", "howUrl", "mask", "action"];
const strOrNull = (v: unknown): boolean => v === null || typeof v === "string";
const oneOf = (v: unknown, set: readonly string[]): boolean => typeof v === "string" && set.includes(v);
const strs = (v: unknown): boolean => Array.isArray(v) && v.every((x) => typeof x === "string");

function choreOk(c: unknown): c is Chore {
  if (!isObj(c) || !exactKeys(c, CHORE_KEYS) || !Array.isArray(c.diff)) return false;
  const diffOk = c.diff.every(
    (d) => isObj(d) && exactKeys(d, ["n", "op", "text"]) && (d.n === null || (typeof d.n === "number" && Number.isInteger(d.n) && d.n > 0)) && oneOf(d.op, ["same", "del", "add"]) && typeof d.text === "string",
  );
  return (
    diffOk &&
    ["id", "name", "loc", "said", "why"].every((k) => typeof c[k] === "string") &&
    oneOf(c.kind, KINDS) &&
    oneOf(c.state, STATES) &&
    oneOf(c.action, ACTIONS) &&
    strs(c.steps) &&
    strs(c.wholeCovers) &&
    ["copy", "whole", "file", "howUrl", "mask"].every((k) => strOrNull(c[k])) &&
    undoOk(c.go)
  );
}

/** 上次值那一份 ⇒ 成品；多一格缺一格 · 类型不对 ⇒ 抛。 */
export function decodeDataReport(v: unknown): DataReport {
  if (!isObj(v) || !exactKeys(v, ["home", "changedFiles", "todo", "tmux", "chores", "own"])) return bad();
  if (!Array.isArray(v.changedFiles) || !Array.isArray(v.todo) || !Array.isArray(v.own)) return bad();
  if (v.tmux !== null && typeof v.tmux !== "boolean") return bad();
  if (typeof v.chores !== "number" || !Number.isInteger(v.chores) || v.chores < 0) return bad();
  str(v.home);
  for (const c of v.changedFiles) if (!isObj(c) || !exactKeys(c, ["path", "what", "undo"]) || typeof c.path !== "string" || typeof c.what !== "string" || !undoOk(c.undo)) return bad();
  if (!v.todo.every(choreOk)) return bad();
  for (const o of v.own) {
    if (!isObj(o) || !exactKeys(o, ["id", "what", "path", "dir", "class", "exists", "size"])) return bad();
    if (typeof o.id !== "string" || typeof o.what !== "string" || typeof o.path !== "string" || !oneOf(o.class, ["truth", "cache"])) return bad();
    if (typeof o.dir !== "boolean" || typeof o.exists !== "boolean") return bad();
    if (o.size !== null && !(typeof o.size === "number" && Number.isInteger(o.size) && o.size >= 0)) return bad();
  }
  return v as unknown as DataReport;
}

/** 记下「待办」里的一个选择（那台后端写它自己的 `~/.cc-monitor/chores.json`）。失败抛一句人话。 */
export async function markChore(origin: Origin, args: ChoresMarkReq): Promise<void> {
  const target = isLocalOrigin(origin) ? LOCAL_ORIGIN : origin;
  try {
    await callTyped(target, "chores-mark", args, budgetWithin(MARK_BUDGET_MS));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
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
    throw new Error(saidFrom(e, origin));
  }
  void rememberSeen(origin, "data", report);
  return report;
}

/** 那台上次读成的那一份（本机后端记着的，跨重启还在）；没有 / 形状对不上 ⇒ `null`。 */
export async function recallDataReport(origin: Origin): Promise<{ report: DataReport; atMs: number } | null> {
  const seen = await recallSeen(origin, "data");
  if (seen === null) return null;
  try {
    return { report: decodeDataReport(seen.value), atMs: seen.atMs };
  } catch {
    return null;
  }
}

/** 那台进角标的件数（问不到 ⇒ `null`，不当 0）。设置窗角标与主窗口状态栏读这一个。 */
export async function choresOf(origin: Origin): Promise<number | null> {
  try {
    return (await readDataReport(origin)).chores;
  } catch {
    return null;
  }
}
