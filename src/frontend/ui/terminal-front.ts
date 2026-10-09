/**
 * ↗「切到对应终端窗口」在哪些机器上是真的：最后一跳是 Win32（`EnumWindows` 找窗口、`SetForegroundWindow` 拉起来），
 * 非 Windows 上 Rust 侧那几跳是恒失败的桩 ⇒ 不是 Windows 就不画按钮；快捷键与命令面板走到
 * `tabs.ts::bringActiveTerminalToFront` 时说一句实话，不发 IPC。本机 Linux / Wayland 的拉前没量过，不假装覆盖。
 * 测不出 OS ⇒ 照常显示（同 `settings/host-os.ts::hostOsAllows`：错藏起来 Windows 用户就找不到这颗按钮）。
 */
import { hostOsAllows, type HostOs } from "./settings/host-os";
import { copyText } from "./copy-table";

/** ↗ 真能用的 OS。**只有一个** —— 见头注。 */
export const TERMINAL_FRONT_HOST_OS: readonly HostOs[] = ["windows"];

/** 这台机器上 ↗ 是不是真的。 */
export function terminalFrontAvailable(): boolean {
  return hostOsAllows(TERMINAL_FRONT_HOST_OS);
}

/** 快捷键 / 命令面板走到 ↗ 而本机不是 Windows 时说的话（标题 · 正文）。 */
export const TERMINAL_FRONT_UNAVAILABLE_TITLE = copyText("terminalFront.unavailable.title");
export const TERMINAL_FRONT_UNAVAILABLE_DETAIL = copyText("terminalFront.unavailable.detail");
