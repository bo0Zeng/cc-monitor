/**
 * ↗「切到对应终端窗口」在这台上是不是真的：壳说了算（`platform/host_facts.rs` 的 `terminal_front`，起页时注入），界面不按系统猜。
 * 不是真的 ⇒ 不画按钮；快捷键与命令面板走到 `tabs.ts::bringActiveTerminalToFront` 时说一句实话，不发 IPC。
 * 读不到壳那一份 ⇒ 照常显示（错藏起来用户就找不到这颗按钮，`settings/host-os.ts::UNKNOWN_HOST`）。
 */
import { hostFacts } from "./settings/host-os";
import { copyText } from "./copy-table";

/** 这台机器上 ↗ 是不是真的。 */
export function terminalFrontAvailable(): boolean {
  return hostFacts().terminalFront;
}

/** 快捷键 / 命令面板走到 ↗ 而本机不是 Windows 时说的话（标题 · 正文）。 */
export const TERMINAL_FRONT_UNAVAILABLE_TITLE = copyText("terminalFront.unavailable.title");
export const TERMINAL_FRONT_UNAVAILABLE_DETAIL = copyText("terminalFront.unavailable.detail");
