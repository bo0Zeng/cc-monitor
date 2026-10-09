/**
 * 图标：全产品一套 Phosphor 线形（MIT，npm 包 `@phosphor-icons/core`；文件窗口用的 `egui-phosphor` 是同一套）。
 *
 * - 只用线形一套（标过星的那颗也是线形 `star`，由所在控件涂 `--accent`；文件窗口同）。
 * - 只打包下面登记的这些（逐个 `?raw` 导入；判据钉「产物里的图标 == 这张表 == 代码里叫到的名字」）。
 * - `currentColor` 上色：默认 `--text-2`，悬停 / 当前由所在控件改 `color`。
 * - 纯装饰的图标读屏器不念（`aria-hidden`）；单独成钮的图标由按钮带读屏名（文案表 `aria` 档），图标自己不带字。
 * - 不拼 HTML：从 svg 文本里取出路径数据，用 `createElementNS` 建。
 * 判据：`tests/frontend/ui/icons-one-source.vitest.ts`。
 */
import gearSix from "@phosphor-icons/core/assets/regular/gear-six.svg?raw";
import clockCounterClockwise from "@phosphor-icons/core/assets/regular/clock-counter-clockwise.svg?raw";
import squaresFour from "@phosphor-icons/core/assets/regular/squares-four.svg?raw";
import folder from "@phosphor-icons/core/assets/regular/folder.svg?raw";
import x from "@phosphor-icons/core/assets/regular/x.svg?raw";
import check from "@phosphor-icons/core/assets/regular/check.svg?raw";
import caretRight from "@phosphor-icons/core/assets/regular/caret-right.svg?raw";
import caretLeft from "@phosphor-icons/core/assets/regular/caret-left.svg?raw";
import info from "@phosphor-icons/core/assets/regular/info.svg?raw";
import warning from "@phosphor-icons/core/assets/regular/warning.svg?raw";
import warningCircle from "@phosphor-icons/core/assets/regular/warning-circle.svg?raw";
import checkCircle from "@phosphor-icons/core/assets/regular/check-circle.svg?raw";
import magnifyingGlass from "@phosphor-icons/core/assets/regular/magnifying-glass.svg?raw";
import tray from "@phosphor-icons/core/assets/regular/tray.svg?raw";
import arrowsLeftRight from "@phosphor-icons/core/assets/regular/arrows-left-right.svg?raw";
import dotsSixVertical from "@phosphor-icons/core/assets/regular/dots-six-vertical.svg?raw";
import caretDown from "@phosphor-icons/core/assets/regular/caret-down.svg?raw";
import arrowsClockwise from "@phosphor-icons/core/assets/regular/arrows-clockwise.svg?raw";
import arrowSquareOut from "@phosphor-icons/core/assets/regular/arrow-square-out.svg?raw";
import dotsThree from "@phosphor-icons/core/assets/regular/dots-three.svg?raw";
import pushPin from "@phosphor-icons/core/assets/regular/push-pin.svg?raw";
import files from "@phosphor-icons/core/assets/regular/files.svg?raw";
import copySvg from "@phosphor-icons/core/assets/regular/copy.svg?raw";
import question from "@phosphor-icons/core/assets/regular/question.svg?raw";
import brain from "@phosphor-icons/core/assets/regular/brain.svg?raw";
import xCircle from "@phosphor-icons/core/assets/regular/x-circle.svg?raw";
import arrowLeft from "@phosphor-icons/core/assets/regular/arrow-left.svg?raw";
import funnel from "@phosphor-icons/core/assets/regular/funnel.svg?raw";
import desktop from "@phosphor-icons/core/assets/regular/desktop.svg?raw";
import star from "@phosphor-icons/core/assets/regular/star.svg?raw";
import chat from "@phosphor-icons/core/assets/regular/chat.svg?raw";
import list from "@phosphor-icons/core/assets/regular/list.svg?raw";
import plus from "@phosphor-icons/core/assets/regular/plus.svg?raw";
import listChecks from "@phosphor-icons/core/assets/regular/list-checks.svg?raw";
import robot from "@phosphor-icons/core/assets/regular/robot.svg?raw";
import gauge from "@phosphor-icons/core/assets/regular/gauge.svg?raw";
import command from "@phosphor-icons/core/assets/regular/command.svg?raw";
import circleDashed from "@phosphor-icons/core/assets/regular/circle-dashed.svg?raw";
import circleNotch from "@phosphor-icons/core/assets/regular/circle-notch.svg?raw";
import terminalWindow from "@phosphor-icons/core/assets/regular/terminal-window.svg?raw";
import arrowDown from "@phosphor-icons/core/assets/regular/arrow-down.svg?raw";
import plug from "@phosphor-icons/core/assets/regular/plug.svg?raw";
import textT from "@phosphor-icons/core/assets/regular/text-t.svg?raw";
import slidersHorizontal from "@phosphor-icons/core/assets/regular/sliders-horizontal.svg?raw";
import appWindow from "@phosphor-icons/core/assets/regular/app-window.svg?raw";
import shareNetwork from "@phosphor-icons/core/assets/regular/share-network.svg?raw";
import keyboard from "@phosphor-icons/core/assets/regular/keyboard.svg?raw";
import cornersOut from "@phosphor-icons/core/assets/regular/corners-out.svg?raw";
import minus from "@phosphor-icons/core/assets/regular/minus.svg?raw";
import sidebarSimple from "@phosphor-icons/core/assets/regular/sidebar-simple.svg?raw";
import magnifyingGlassPlus from "@phosphor-icons/core/assets/regular/magnifying-glass-plus.svg?raw";
import magnifyingGlassMinus from "@phosphor-icons/core/assets/regular/magnifying-glass-minus.svg?raw";
import arrowsOutLineVertical from "@phosphor-icons/core/assets/regular/arrows-out-line-vertical.svg?raw";
import userCircle from "@phosphor-icons/core/assets/regular/user-circle.svg?raw";
import bell from "@phosphor-icons/core/assets/regular/bell.svg?raw";
import arrowCounterClockwise from "@phosphor-icons/core/assets/regular/arrow-counter-clockwise.svg?raw";
import gitFork from "@phosphor-icons/core/assets/regular/git-fork.svg?raw";
import arrowBendDownLeft from "@phosphor-icons/core/assets/regular/arrow-bend-down-left.svg?raw";
import stack from "@phosphor-icons/core/assets/regular/stack.svg?raw";
import file from "@phosphor-icons/core/assets/regular/file.svg?raw";
import circleHalf from "@phosphor-icons/core/assets/regular/circle-half.svg?raw";
import circle from "@phosphor-icons/core/assets/regular/circle.svg?raw";
import note from "@phosphor-icons/core/assets/regular/note.svg?raw";
import stop from "@phosphor-icons/core/assets/regular/stop.svg?raw";
import s from "./icon.module.css";

const SVG = {
  settings: gearSix,
  history: clockCounterClockwise,
  grid: squaresFour,
  folder: folder,
  close: x,
  check: check,
  caretRight: caretRight,
  caretLeft: caretLeft,
  info: info,
  warning: warning,
  error: warningCircle,
  success: checkCircle,
  search: magnifyingGlass,
  empty: tray,
  swap: arrowsLeftRight,
  drag: dotsSixVertical,
  caretDown: caretDown,
  refresh: arrowsClockwise,
  front: arrowSquareOut,
  more: dotsThree,
  pin: pushPin,
  files: files,
  copy: copySvg,
  question: question,
  brain: brain,
  failed: xCircle,
  back: arrowLeft,
  filter: funnel,
  machine: desktop,
  star: star,
  chat: chat,
  list: list,
  plus: plus,
  tasks: listChecks,
  agent: robot,
  context: gauge,
  command: command,
  pending: circleDashed,
  inProgress: circleNotch,
  terminal: terminalWindow,
  arrowDown: arrowDown,
  plug: plug,
  text: textT,
  sliders: slidersHorizontal,
  popOut: appWindow,
  bus: shareNetwork,
  keyboard: keyboard,
  fullscreen: cornersOut,
  minimize: minus,
  sidebar: sidebarSimple,
  zoomIn: magnifyingGlassPlus,
  zoomOut: magnifyingGlassMinus,
  reset: arrowCounterClockwise,
  expand: arrowsOutLineVertical,
  account: userCircle,
  bell: bell,
  fork: gitFork,
  branchOff: arrowBendDownLeft,
  background: stack,
  file: file,
  half: circleHalf,
  ring: circle,
  note: note,
  stop: stop,
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

/**
 * CSS 伪元素画图标用的那一形（`<details>` 摘要的箭头之类没有元素可挂）：`url("data:…")`，
 * 由 `css-marks.ts` 设成根上的 `--icon-…`，CSS 写 `mask: var(--icon-…)` ＋ `background: currentColor`。
 */
export function iconMaskUrl(name: IconName): string {
  return `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns="${NS}" viewBox="0 0 256 256"><path d="${pathOf(SVG[name])}"/></svg>`)}")`;
}
