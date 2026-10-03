/**
 * Tauri 命令的默认答法（monitor 那一侧的命令：读配置、问机器清单 …）。
 */
import type { World } from "./types";

export type CommandHandler = (args: Record<string, unknown>, world: World) => unknown;

export function defaultCommands(): Record<string, CommandHandler> {
  return {
    load_config: (_a, w) => w.config,
    patch_config: () => null,
    backend_machines: (_a, w) => w.machines,
    frontend_perf_log: () => null,
  };
}
