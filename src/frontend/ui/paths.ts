/**
 * 设置里「Claude 数据目录」那一格：住**本机后端**的偏好（`prefs.json` 的 `agentHomes`，按家；后端 `control/machine_prefs.rs`），
 * 界面只经 `prefs-read` / `prefs-set` 读写，不存副本。改动后**要重启本机后端才生效**（后端起来时读一次）。
 */

import { DEFAULT_AGENT } from "./agent-profile";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { readPrefs, setPrefs } from "./machine-prefs";

/** 本机设了的 Claude 数据目录（没设 ⇒ null）。通道不通 ⇒ 抛。 */
export async function readClaudeDir(): Promise<string | null> {
  const v = (await readPrefs(LOCAL_ORIGIN)).prefs.agentHomes[DEFAULT_AGENT];
  return typeof v === "string" && v.trim() ? v : null;
}

/** 改本机的 Claude 数据目录。传 null ⇒ 删掉那一格（回到环境变量 / 默认）。拒 / 通道不通 ⇒ 抛。 */
export async function setClaudeDirOverride(dir: string | null): Promise<void> {
  await setPrefs(LOCAL_ORIGIN, { agentHomes: { [DEFAULT_AGENT]: dir?.trim() ?? "" } });
}
