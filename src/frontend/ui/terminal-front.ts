/**
 * ↗「切到对应终端窗口」在哪些机器上是真的：最后一跳 Windows 走 Win32（`EnumWindows` 找窗口、`SetForegroundWindow` 拉起来），
 * Linux 的 X11 会话走 EWMH；Linux 的 Wayland 会话由壳照实说「这个桌面上切不了」并给［在 cc-monitor 里打开］（结局族 `desktop-wont-switch`）。
 * macOS 上 Rust 侧那几跳是恒失败的桩 ⇒ 不画按钮；快捷键与命令面板走到 `tabs.ts::bringActiveTerminalToFront` 时说一句实话，不发 IPC。
 * 测不出 OS ⇒ 照常显示（同 `settings/host-os.ts::hostOsAllows`：错藏起来用户就找不到这颗按钮）。
 */
import { hostOsAllows, type HostOs } from "./settings/host-os";
import { copyText } from "./copy-table";

/** ↗ 真能用的 OS —— 见头注。 */
export const TERMINAL_FRONT_HOST_OS: readonly HostOs[] = ["windows", "linux"];

/** 这台机器上 ↗ 是不是真的。 */
export function terminalFrontAvailable(): boolean {
  return hostOsAllows(TERMINAL_FRONT_HOST_OS);
}

/** 快捷键 / 命令面板走到 ↗ 而本机不在上面那几种里时说的话（标题 · 正文）。 */
export const TERMINAL_FRONT_UNAVAILABLE_TITLE = copyText("terminalFront.unavailable.title");
export const TERMINAL_FRONT_UNAVAILABLE_DETAIL = copyText("terminalFront.unavailable.detail");
