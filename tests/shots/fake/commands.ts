/**
 * Tauri 命令的默认答法（monitor 那一侧的命令：读配置、问机器清单 …）。
 */
import type { World } from "./types";
import { copyText } from "../../../src/frontend/ui/copy-table";

export type CommandHandler = (args: Record<string, unknown>, world: World) => unknown;

export function defaultCommands(): Record<string, CommandHandler> {
  return {
    load_config: (_a, w) => w.config,
    patch_config: () => null,
    backend_machines: (_a, w) => w.machines,
    frontend_perf_log: () => null,
    // 写剪贴板（经壳）：截图里没有剪贴板 ⇒ 当作写不进，点［复制详情］就地展开那一段（看得见复制出去的是什么）。
    clipboard_write: () => Promise.reject({ said: copyText("rsShellCmd.clipboard.failed"), detail: "原话：截图里没有剪贴板" }),
    // 本机开一个终端窗口跑那一行：截图里不真开，当作开了。
    open_local_terminal: () => "opened",
    open_terminal_window: () => "opened",
    // 在新窗口里看一个会话：截图里不真开窗。
    open_session_in_new_window: () => null,
    // 设置 → 通用「终端」那一行的事实（Linux 桌面：自动挑到 ptyxis）。
    terminal_choices: () => ({ applies: true, auto: "ptyxis", found: ["ptyxis", "alacritty"], setting: "" }),
  };
}
