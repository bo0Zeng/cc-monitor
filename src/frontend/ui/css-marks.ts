/**
 * 〔「符号也进表」〕CSS 伪元素里要的那几样，起步时设成根元素上的自定义属性：
 * - 分隔点（句中符号，在符号表里）从文案表来：`content: var(--mark-sep)`。
 * - 图标从 `kit/icon.ts` 的登记表来（Phosphor 线形，规范 V9）：`--icon-…` 是一份 mask 地址，
 *   CSS 写 `content: ""` ＋ `background: currentColor` ＋ `mask: var(--icon-…) center / contain no-repeat`。
 * 三个窗口都装（`entry-common.ts`）；某个窗口的 CSS 用不到的那几格设了也无妨。
 */
import { copyText } from "./copy-table";
import { iconMaskUrl } from "./kit/icon";

/** 一段文字写成 CSS 字符串（`content:` 收的那一形）。 */
function cssString(text: string): string {
  return `"${text.replace(/[\\"]/g, (c) => `\\${c}`).replace(/\n/g, "\\A ")}"`;
}

export function installCssMarks(root: HTMLElement = document.documentElement): void {
  root.style.setProperty("--mark-sep", cssString(copyText("cssMarks.sep.dot")));
  root.style.setProperty("--icon-caret-right", iconMaskUrl("caretRight"));
  root.style.setProperty("--icon-check", iconMaskUrl("check"));
  root.style.setProperty("--icon-branch-off", iconMaskUrl("branchOff"));
  root.style.setProperty("--icon-background", iconMaskUrl("background"));
  root.style.setProperty("--icon-folder", iconMaskUrl("folder"));
  root.style.setProperty("--icon-file", iconMaskUrl("file"));
}
