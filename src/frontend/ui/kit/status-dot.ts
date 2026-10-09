/**
 * 状态点：会话 · agent · 机器 · 任务全产品一套。颜色 ＝ 现在在干什么，形状 ＝ 进程还在不在。
 *
 * 每个点带悬停提示与读屏名（调用方给人话状态名，文案表那几个）；只有「运行中」会动（减弱动效时常亮）。
 * `up` 是机器「已连接」：实心成功色、不呼吸（呼吸只给在跑的会话 / agent）。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「转圈 · 计量条 · 徽标 · 键帽 · 骨架 · 状态点」那一节。
 */
import s from "./status-dot.module.css";

export type DotState = "running" | "needs-you" | "idle" | "exited" | "ended" | "unknown" | "gone" | "failed";

/** 机器的点：`up` 已连接 · `failed` 离线 · `needs-you` 要你处理 · `exited` 已停用 · `unknown` 连接中 / 状态不明。 */
export type MachineDotState = "up" | "failed" | "needs-you" | "exited" | "unknown";

export function statusDot(state: DotState | MachineDotState, label: string, size: "regular" | "compact" = "regular"): HTMLSpanElement {
  const d = document.createElement("span");
  d.className = s.dot;
  d.setAttribute("role", "img");
  setDot(d, state, label);
  d.dataset.size = size;
  return d;
}

export function setDot(d: HTMLElement, state: DotState | MachineDotState, label: string): void {
  d.dataset.state = state;
  d.setAttribute("aria-label", label);
  d.title = label;
}
