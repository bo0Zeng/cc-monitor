/**
 * `设计/80` LF1（第二波 T4）：**↗「切到对应终端窗口」在哪些机器上是真的**。
 *
 * # 为什么要这一道门
 *
 * ↗ 的最后一跳是 Win32：`EnumWindows` 找窗口、`SetForegroundWindow` 把它拉起来。
 * 非 Windows 上那几跳在 Rust 侧是**恒失败的桩**（`bind.rs::find_window_by_marker_substr`
 * 恒 `None`、`verify_binding` / `activate` 恒 `Err`）⇒ Linux 上这颗按钮**每点必败**，
 * 却长得和 Windows 上那颗一模一样 —— 那是「装得能用」。
 * `设计/80 §0.4` 第三条非目标逐字：本机 Linux / Wayland 的拉前**没量过，不假装覆盖**。
 *
 * # 形状
 *
 * - 按钮：不是 Windows 就**不渲**（`tabs.ts` 建 tab 那一处只留一行调用）。
 * - 快捷键与命令面板（它们住 `main.ts`，不归本件管）：走到 `tabs.ts::bringActiveTerminalToFront`
 *   时说一句实话，不发 IPC、不装作试过。
 *
 * # 失败方向：测不出 OS ⇒ **照常显示**
 *
 * 与 `settings/host-os.ts::hostOsAllows` 同一条理由（两种错的代价不对称）：错判成非 Windows
 * 而藏起来，Windows 用户就**再也找不到**这颗按钮；错判成 Windows 而显示，就是加这道门之前的样子。
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
