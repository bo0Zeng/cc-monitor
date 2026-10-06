/**
 * 图标：全产品一套 Phosphor 线形（MIT，npm 包 `@phosphor-icons/core`；文件窗口用的 `egui-phosphor` 是同一套）。
 *
 * - 只打包下面登记的这些（逐个 `?raw` 导入；判据钉「产物里的图标 == 这张表 == 代码里叫到的名字」）。
 * - `currentColor` 上色：默认 `--text-2`，悬停 / 当前由所在控件改 `color`。
 * - 纯装饰的图标读屏器不念（`aria-hidden`）；单独成钮的图标由按钮带读屏名（文案表 `aria` 档），图标自己不带字。
 * - 不拼 HTML：从 svg 文本里取出路径数据，用 `createElementNS` 建。
 */
import gearSix from "@phosphor-icons/core/assets/regular/gear-six.svg?raw";
import clockCounterClockwise from "@phosphor-icons/core/assets/regular/clock-counter-clockwise.svg?raw";
import squaresFour from "@phosphor-icons/core/assets/regular/squares-four.svg?raw";
import folder from "@phosphor-icons/core/assets/regular/folder.svg?raw";
import keyboard from "@phosphor-icons/core/assets/regular/keyboard.svg?raw";
import x from "@phosphor-icons/core/assets/regular/x.svg?raw";
import check from "@phosphor-icons/core/assets/regular/check.svg?raw";
import caretRight from "@phosphor-icons/core/assets/regular/caret-right.svg?raw";
import info from "@phosphor-icons/core/assets/regular/info.svg?raw";
import warning from "@phosphor-icons/core/assets/regular/warning.svg?raw";
import warningCircle from "@phosphor-icons/core/assets/regular/warning-circle.svg?raw";
import checkCircle from "@phosphor-icons/core/assets/regular/check-circle.svg?raw";
import magnifyingGlass from "@phosphor-icons/core/assets/regular/magnifying-glass.svg?raw";
import tray from "@phosphor-icons/core/assets/regular/tray.svg?raw";
import arrowsLeftRight from "@phosphor-icons/core/assets/regular/arrows-left-right.svg?raw";
import dotsSixVertical from "@phosphor-icons/core/assets/regular/dots-six-vertical.svg?raw";
import caretDown from "@phosphor-icons/core/assets/regular/caret-down.svg?raw";
import s from "./icon.module.css";

const SVG = {
  settings: gearSix,
  history: clockCounterClockwise,
  grid: squaresFour,
  folder: folder,
  keyboard: keyboard,
  close: x,
  check: check,
  caretRight: caretRight,
  info: info,
  warning: warning,
  error: warningCircle,
  success: checkCircle,
  search: magnifyingGlass,
  empty: tray,
  swap: arrowsLeftRight,
  drag: dotsSixVertical,
  caretDown: caretDown,
} as const;

export type IconName = keyof typeof SVG;
/** 三档：常规 16 · 紧凑 14 · 空态插画位 32。 */
export type IconSize = "regular" | "compact" | "empty";

const NS = "http://www.w3.org/2000/svg";

function pathOf(raw: string): string {
  const m = /<path d="([^"]+)"\/>/.exec(raw);
  if (!m) throw new Error("icon: not a single-path svg");
  return m[1];
}

/** 一个图标元素（`<svg>`）。 */
export function icon(name: IconName, size: IconSize = "regular"): SVGSVGElement {
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 256 256");
  svg.setAttribute("fill", "currentColor");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  svg.classList.add(s.icon);
  svg.dataset.size = size;
  svg.dataset.icon = name;
  const path = document.createElementNS(NS, "path");
  path.setAttribute("d", pathOf(SVG[name]));
  svg.appendChild(path);
  return svg;
}

/** 登记了哪些（判据读）。 */
export const ICON_NAMES = Object.keys(SVG) as IconName[];
