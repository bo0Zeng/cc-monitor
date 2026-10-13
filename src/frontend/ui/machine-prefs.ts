/**
 * **那台的偏好**（每一家的启动命令 · 恢复进不进 tmux · 每个号的模型 · 上下文上限表）：住那台后端（`~/.cc-monitor/prefs.json`，
 * 后端 `control/machine_prefs.rs`），界面只经 `prefs-read` / `prefs-set` 读写，不存副本。手机直连那台、读的是同一份。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { PrefsReply } from "./generated/PrefsReply";
import type { PrefsSet } from "./generated/PrefsSet";

export type { PrefsReply, PrefsSet };

/** 一问的期限：读写一份小文件 ＋ 回程。 */
const PREFS_BUDGET_MS = 10_000;

/** 读那台的偏好（读不懂 ⇒ `state: "unreadable"`、照缺省那一份）。通道不通 ⇒ 抛。 */
export async function readPrefs(origin: Origin): Promise<PrefsReply> {
  return callTyped(origin, "prefs-read", {}, budgetWithin(PREFS_BUDGET_MS));
}

/** 改那台的偏好（只动给了的那几节；`models` 里 `null` ⇒ 删那一条），回写完读回的那一份。拒 / 通道不通 ⇒ 抛。 */
export async function setPrefs(origin: Origin, patch: PrefsSet): Promise<PrefsReply> {
  return callTyped(origin, "prefs-set", patch, budgetWithin(PREFS_BUDGET_MS));
}
