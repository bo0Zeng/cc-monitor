/**
 * 〔「符号也进表」〕CSS 伪元素里的那几个符号从文案表来：
 * 起步时设成根元素上的自定义属性，CSS 写 `content: var(--mark-…)`。CSS 读不了表，这是它拿到表里那一格的一条路。
 * 三个窗口都装（`entry-common.ts`）；某个窗口的 CSS 用不到的那几格设了也无妨。
 */
import { copyText } from "./copy-table";

/** 一段文字写成 CSS 字符串（`content:` 收的那一形）。 */
function cssString(text: string): string {
  return `"${text.replace(/[\\"]/g, (c) => `\\${c}`).replace(/\n/g, "\\A ")}"`;
}

export function installCssMarks(root: HTMLElement = document.documentElement): void {
  root.style.setProperty("--mark-ended", cssString(copyText("cssMarks.tab.ended")));
  root.style.setProperty("--mark-branch", cssString(copyText("cssMarks.branch.mark")));
  root.style.setProperty("--mark-chosen", cssString(copyText("cssMarks.ask.chosen")));
  root.style.setProperty("--mark-closed", cssString(copyText("cssMarks.fold.closed")));
  root.style.setProperty("--mark-open", cssString(copyText("cssMarks.fold.open")));
  root.style.setProperty("--mark-bg-cell", cssString(copyText("cssMarks.cell.background")));
  root.style.setProperty("--mark-dir", cssString(copyText("cssMarks.data.dir")));
  root.style.setProperty("--mark-file", cssString(copyText("cssMarks.data.file")));
}
