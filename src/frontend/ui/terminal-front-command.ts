/**
 * 命令面板里「切到终端」（↗）那一项：与标签页上那颗 ↗ 同一道（`face-can.ts::frontWhy`）——
 * 一直列着；这台 ↗ 不是真的 · 当前会话那台说没有终端可去 ⇒ 灰着，第二行写那一句（A2-17-12-1）。
 */
import { frontWhy } from "./face-can";
import type { SessionFace } from "./generated/SessionFace";

/** ↗ 那一项：能用 ⇒ 原样；不能用 ⇒ 同一项带上 `disabled`（调用方展开进命令表，位置不变）。`face` ＝ 当前会话的脸。 */
export function terminalFrontCommand<C extends object>(item: C, face: SessionFace | null): (C & { disabled?: string })[] {
  const why = frontWhy(face);
  return why === null ? [item] : [{ ...item, disabled: why }];
}
