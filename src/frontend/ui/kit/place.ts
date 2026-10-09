/**
 * 浮层定位（C12「躲窗口边」）：全产品**只这一处**算浮在内容上的东西摆在哪 —— 菜单 · 子菜单 · 浮层面板 · ↗ 的「查找终端…」·
 * 悬停提示与悬停卡都调 [`placeFloat`]。
 *
 * - 锚在一点（右键）：从那一点往右下放；放不下就翻到左 / 上。
 * - 锚在触发物上：先放在指定那一侧（下 / 上 / 右 / 左）隔 `gap`；那一侧放不下翻到对面；两边都放不下 ⇒ 挑空多的那边。
 *   另一条轴按对齐（左端 / 居中 / 右端，侧放时是顶 / 中 / 底）：左端对齐放不下 ⇒ 改右端对齐（贴着触发物往里收），反之亦然。
 * - 最后夹进窗口、贴边内缩 8px。
 *
 * 右端对齐算出的是左边那条边，不是一个点：不能再按右键菜单的规矩往左整翻一个宽（否则贴着窗口右边的触发物，菜单落到窗口中间）。
 */

/** 外接框（`DOMRect` 的那四条边）。 */
export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export type Side = "below" | "above" | "right" | "left";
export type Align = "start" | "center" | "end";
export type FloatAnchor = { x: number; y: number } | { rect: Box; side: Side; align: Align; gap: number };

/** 贴窗口边留多少。 */
export const EDGE = 8;

function clamp(v: number, size: number, room: number): number {
  return Math.max(EDGE, Math.min(v, room - EDGE - size));
}

/** 主轴：`near` ＝ 触发物这一侧的边、`far` ＝ 对面那条边；先放 `prefer` 那一侧，放不下翻，都放不下挑空多的。 */
function mainAxis(lo: number, hi: number, size: number, room: number, gap: number, after: boolean): number {
  const afterAt = hi + gap;
  const beforeAt = lo - gap - size;
  const fitsAfter = afterAt + size <= room - EDGE;
  const fitsBefore = beforeAt >= EDGE;
  const roomAfter = room - EDGE - afterAt;
  const roomBefore = lo - gap - EDGE;
  if (after) return fitsAfter || (!fitsBefore && roomAfter >= roomBefore) ? afterAt : beforeAt;
  return fitsBefore || (!fitsAfter && roomBefore >= roomAfter) ? beforeAt : afterAt;
}

/** 交叉轴：按对齐放；左（顶）端对齐放不下 ⇒ 右（底）端对齐，反之亦然。 */
function crossAxis(lo: number, hi: number, size: number, room: number, align: Align): number {
  const start = lo;
  const end = hi - size;
  if (align === "center") return (lo + hi) / 2 - size / 2;
  if (align === "start") return start + size > room - EDGE ? end : start;
  return end < EDGE ? start : end;
}

/** 纯函数：一块 `size` 大的浮层锚在 `anchor` 上、窗口 `view` 大 ⇒ 左上角放哪（视口坐标）。 */
export function placeFloat(anchor: FloatAnchor, size: { width: number; height: number }, view: { width: number; height: number }): { left: number; top: number } {
  const { width: w, height: h } = size;
  let left: number;
  let top: number;
  if ("x" in anchor) {
    left = anchor.x + w <= view.width - EDGE ? anchor.x : anchor.x - w;
    top = anchor.y + h <= view.height - EDGE ? anchor.y : anchor.y - h;
  } else {
    const r = anchor.rect;
    if (anchor.side === "below" || anchor.side === "above") {
      top = mainAxis(r.top, r.bottom, h, view.height, anchor.gap, anchor.side === "below");
      left = crossAxis(r.left, r.right, w, view.width, anchor.align);
    } else {
      left = mainAxis(r.left, r.right, w, view.width, anchor.gap, anchor.side === "right");
      top = crossAxis(r.top, r.bottom, h, view.height, anchor.align);
    }
  }
  return { left: clamp(left, w, view.width), top: clamp(top, h, view.height) };
}

/** 触发物此刻的外接框；它已不在 DOM 里（开着时被重画掉）⇒ 用上次量到的（不让浮层飞到左上角）。 */
export function anchorBox(el: HTMLElement, last: Box | null): Box {
  if (!el.isConnected && last) return last;
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
}

/** 把浮层摆到触发物旁（量它自己的大小、按窗口算），回这次用的触发物外接框（下次重排时触发物没了就用它）。 */
export function placeBeside(float: HTMLElement, anchor: HTMLElement, at: { side: Side; align: Align; gap: number }, last: Box | null = null): Box {
  const rect = anchorBox(anchor, last);
  const { width, height } = float.getBoundingClientRect();
  putAt(float, placeFloat({ rect, ...at }, { width, height }, { width: window.innerWidth, height: window.innerHeight }));
  return rect;
}

/** 写进 `left / top`（`position: fixed` 的浮层）。 */
export function putAt(float: HTMLElement, at: { left: number; top: number }): void {
  float.style.left = `${at.left}px`;
  float.style.top = `${at.top}px`;
}
