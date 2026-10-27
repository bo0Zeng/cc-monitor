/**
 * 开一扇只装终端面的窗（「终端」视图上「新窗口」）：一个终端一扇（窗口名由壳按「那台 ＋ 句柄」定，已开着就拉前）。
 */
import { commands } from "./ipc/commands";
import type { Origin } from "./ipc/origin";

export interface OpenTerminalWindow {
  origin: Origin;
  /** 名单里的句柄。 */
  terminal: string;
  /** tmux 会话名。 */
  name: string;
  /** 名单那一行的标题。 */
  title: string;
  /** 系统标题（后端写好的那一格）。 */
  windowTitle: string;
}

export async function openTerminalWindow(o: OpenTerminalWindow): Promise<void> {
  await commands.open_terminal_in_new_window(o);
}
