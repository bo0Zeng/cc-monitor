/**
 * 命令面板里「切到终端」（↗）那一项：与标签页上那颗 ↗ 同一道门（`terminalFrontAvailable`）——
 * 只有 cc-monitor 跑在 Windows 上才能用；别的系统上照样列出来、灰着，第二行写 `仅 Windows`。
 * 测不出 OS ⇒ 当能用（错灰的代价是 Windows 用户点不了它）。
 */
import { terminalFrontAvailable } from "./terminal-front";
import { copyText } from "./copy-table";

/** ↗ 那一项：能用 ⇒ 原样；不能用 ⇒ 同一项带上 `disabled`（调用方展开进命令表，位置不变）。 */
export function terminalFrontCommand<C extends object>(item: C): (C & { disabled?: string })[] {
  return terminalFrontAvailable() ? [item] : [{ ...item, disabled: copyText("terminalFront.unavailable.short") }];
}
