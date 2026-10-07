/**
 * 本机自己的那几格偏好（远端每台的在机器表里，本机不在机器表 ⇒ 住这里）：今天只有恢复命令那一格覆盖
 *（本机「这台上的 cc-monitor → 恢复命令」；空 ＝ 用通用页那一格）。
 */
import { loadConfig, patchConfig, setAt } from "./config";

const KEY_LOCAL_RESUME = "localResumeCommand";

/** 从一份已读回的配置里取本机那一格恢复命令；缺 / 类型不对 ⇒ 空。 */
export function localResumeCommandIn(cfg: Record<string, unknown>): string {
  const v = cfg[KEY_LOCAL_RESUME];
  return typeof v === "string" ? v : "";
}

export async function getLocalResumeCommand(): Promise<string> {
  try {
    return localResumeCommandIn((await loadConfig()) as Record<string, unknown>);
  } catch (e) {
    console.warn("getLocalResumeCommand failed:", e);
    return "";
  }
}

/** 只写这一个键。 */
export async function setLocalResumeCommand(cmd: string): Promise<void> {
  await patchConfig([setAt([KEY_LOCAL_RESUME], cmd.trim())]);
}
