/**
 * 〔MIG-3a · 子步 3 · `设计/01 §6.7a` 规矩 1 · 主会话 09-27 裁 ⑯〕cc-bus 装到本机**走通道，本机后端出成品**：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 装的是哪一版（三态） | `cc-bus-install-state` | `{state}` / `{state:"drifted", differing, missing}` |
 * | 装（幂等 · 覆盖前整目录备份 · 记进 skill 装记录） | `cc-bus-install` | `{dest, written, unchanged, backup, recordFailed}` |
 *
 * 从前是 monitor 的两条 Tauri 命令（`deploy_local_cc_bus` / `cc_bus_install_state`）：monitor 读盘判、算好经本机后端写。
 * 资产的装不算部署 —— 判 · 写 · 记都进了后端（`src/backend/assets/cc_bus_install.rs`）。这里只按形状严格收。
 * 「装出来的 cc-spawn 在这台跑不跑得起来」（`ccm` 够不够新）是 monitor 探本机 `ccm` 的事，另问 `commands.cc_bus_ccm_precheck`。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 三态：刻意不合并（「没装」「最新」「装了但不是这一版（差几个）」）。 */
export type CcBusInstallState =
  | { state: "not_installed" }
  | { state: "up_to_date" }
  | { state: "drifted"; differing: number; missing: number };

/** 一次装的结果。「没变」与「装好了」是两种结果（`written === 0` 就是没变）。 */
export interface CcBusInstalled {
  dest: string;
  written: number;
  unchanged: number;
  backup: string | null;
  /** 装好了但没记进装记录时那一句（这一趟装的卸不掉）；`null` = 记上了 / 没东西要记。 */
  recordFailed: string | null;
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const count = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v) && v >= 0;
const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const bad = (): Error => new Error(copyText("ccBusInstallReads.reply.badShape"));

/** `cc-bus-install-state` 的成品。严格收。 */
export function decodeCcBusInstallState(v: unknown): CcBusInstallState {
  if (!isObj(v)) throw bad();
  if (sameKeys(v, ["state"]) && (v.state === "not_installed" || v.state === "up_to_date")) return { state: v.state };
  if (sameKeys(v, ["state", "differing", "missing"]) && v.state === "drifted" && count(v.differing) && count(v.missing))
    return { state: "drifted", differing: v.differing, missing: v.missing };
  throw bad();
}

/** `cc-bus-install` 的成品。严格收。 */
export function decodeCcBusInstalled(v: unknown): CcBusInstalled {
  if (
    !isObj(v) ||
    !sameKeys(v, ["dest", "written", "unchanged", "backup", "recordFailed"]) ||
    typeof v.dest !== "string" ||
    !count(v.written) ||
    !count(v.unchanged) ||
    !optStr(v.backup) ||
    !optStr(v.recordFailed)
  )
    throw bad();
  return { dest: v.dest, written: v.written, unchanged: v.unchanged, backup: v.backup, recordFailed: v.recordFailed };
}

/** 读一趟 skill 目录 / 写几十个小文件：秒级；给 30 秒。 */
const CC_BUS_BUDGET_MS = 30_000;
const said = (e: unknown): Error => new Error(saidOf(e, copyText("mcpReads.backend.tooOld")));

/** 本机装的是哪一版。问不出来 ⇒ 抛（不许说成「没装」）。 */
export async function readCcBusInstallState(): Promise<CcBusInstallState> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(CC_BUS_BUDGET_MS);
    return decodeCcBusInstallState(readJson(await chan.call(LOCAL_ORIGIN, "cc-bus-install-state", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 装到本机（用户显式点了那颗按钮才调）。 */
export async function installCcBus(): Promise<CcBusInstalled> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(CC_BUS_BUDGET_MS);
    return decodeCcBusInstalled(readJson(await chan.call(LOCAL_ORIGIN, "cc-bus-install", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
