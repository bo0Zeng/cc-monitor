/**
 * 状态点（V10）：会话 · agent · 机器 · 任务全产品一套。颜色 ＝ 现在在干什么，形状 ＝ 进程还在不在。
 *
 * 每个点带悬停提示与读屏名（调用方给人话状态名，文案表那几个）；只有「运行中」会动（减弱动效时常亮）。
 */
import s from "./status-dot.module.css";

export type DotState = "running" | "needs-you" | "idle" | "exited" | "ended" | "unknown" | "gone" | "failed";

export function statusDot(state: DotState, label: string, size: "regular" | "compact" = "regular"): HTMLSpanElement {
  const d = document.createElement("span");
  d.className = s.dot;
  d.setAttribute("role", "img");
  setDot(d, state, label);
  d.dataset.size = size;
  return d;
}

export function setDot(d: HTMLElement, state: DotState, label: string): void {
  d.dataset.state = state;
  d.setAttribute("aria-label", label);
  d.title = label;
}
