/**
 * 〔U2 · 第三波〕命令面板里「把对应终端窗口拉到前台」（↗）那一项 —— **只在 ↗ 真能用的机器上列出来**。
 *
 * LF1（第二波 T4）已经让 tab 上那颗 ↗ 按钮在非 Windows 上不渲（`terminal-front.ts`），
 * 快捷键 / 命令面板走到 `TabManager.bringActiveTerminalToFront` 时说一句实话、不发 IPC。
 * 但命令面板里那一项**仍然列着** —— 用户在 Linux 上按 Ctrl+K 能看到一个「选了必然只得到一句"不能"」的命令，
 * 那是「装得能用」的另一种形状。⇒ 与按钮同一道门（`terminalFrontAvailable`）：不是 Windows 就不列。
 * 快捷键那一路不动（它没有「列出来」这回事，走到了照旧说实话）。
 *
 * 失败方向照 `terminal-front.ts`：测不出 OS ⇒ 照常列出（错藏的代价是 Windows 用户找不到它）。
 */
import { terminalFrontAvailable } from "./terminal-front";

/** ↗ 那一项：本机 ↗ 真能用 ⇒ `[item]`，否则 `[]`（调用方展开进命令表，位置不变）。 */
export function terminalFrontCommand<C>(item: C): C[] {
  return terminalFrontAvailable() ? [item] : [];
}
