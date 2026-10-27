/**
 * 开一扇只装终端面的窗（「终端」视图上「新窗口」）：一个终端一扇（窗口名由壳按「那台 ＋ 句柄」定，已开着就拉前）。
 */
import { commands } from "./ipc/commands";
import type { Origin } from "./ipc/origin";
import { machineName } from "./control-said";
import { copyText } from "./copy-table";

export interface OpenTerminalWindow {
  origin: Origin;
  /** 名单里的句柄。 */
  terminal: string;
  /** tmux 会话名。 */
  name: string;
  /** 名单那一行的标题。 */
  title: string;
}

/** 系统标题：标题 · 终端 · 那台。 */
export function terminalWindowTitle(o: OpenTerminalWindow): string {
  return copyText("terminalWindow.page.title", { title: o.title, machine: machineName(o.origin) });
}

export async function openTerminalWindow(o: OpenTerminalWindow): Promise<void> {
  await commands.open_terminal_in_new_window({ ...o, windowTitle: terminalWindowTitle(o) });
}
