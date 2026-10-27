/**
 * 一台机器的状态点：照那台成品的态挑（判定在后端 `deploy::machine`，这里只挑样子）。
 * 设置 › 机器卡与命令面板的「机器」条目共用这一张表。
 */
import type { MachineStateKind } from "./generated/MachineStateKind";
import type { MachineDotState } from "./kit/status-dot";

const DOT: Record<MachineStateKind, MachineDotState> = {
  up: "up",
  newer: "up",
  connecting: "unknown",
  installing: "unknown",
  updating: "unknown",
  unknown: "unknown",
  down: "failed",
  host_key_changed: "failed",
  unsupported: "failed",
  needs_update: "needs-you",
  disabled: "exited",
};

/** 那一态画哪种点。 */
export function machineDot(state: MachineStateKind): MachineDotState {
  return DOT[state];
}
