/**
 * 一台机器的状态点：后端给的那一档（`MachineDot`，判定在后端 `deploy::machine`）⇒ 点的样子名，一档对一样，这里不另判。
 * 设置 › 机器卡、命令面板的「机器」条目与本机后端没起来那一格共用这一张表。
 */
import type { MachineDot } from "./generated/MachineDot";
import type { MachineDotState } from "./kit/status-dot";

const DOT: Record<MachineDot, MachineDotState> = {
  up: "up",
  pending: "unknown",
  warn: "needs-you",
  failed: "failed",
  off: "exited",
};

/** 那一档画哪种点。 */
export function machineDot(dot: MachineDot): MachineDotState {
  return DOT[dot];
}
