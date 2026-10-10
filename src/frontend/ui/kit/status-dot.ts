/**
 * 状态点：会话 · agent · 机器 · 任务全产品一套。颜色 ＝ 现在在干什么，形状 ＝ 进程还在不在。
 *
 * 每个点带悬停提示与读屏名（调用方给人话状态名，文案表那几个）；只有「运行中」会动（减弱动效时常亮）。
 * `up` 是机器「已连接」：实心成功色、不呼吸（呼吸只给在跑的会话 / agent）。
 * 窗口不在前台（失焦 / 看不见）时不呼吸：点照旧是绿的，只是不闪（`installDotRest`）。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「转圈 · 计量条 · 徽标 · 键帽 · 骨架 · 状态点」那一节 · `status-dot-rest.vitest.ts`。
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

/** 这一段动画是不是运行中那颗点的呼吸（样式里只有它会动）。 */
function isBreath(a: Animation): boolean {
  const t = (a.effect as KeyframeEffect | null)?.target;
  return t instanceof Element && t.classList.contains(s.dot) && t.getAttribute("data-state") === "running";
}

/** 停在起点（不透明，颜色照旧）。 */
function rest(a: Animation): void {
  a.pause();
  a.currentTime = 0;
}

/**
 * 窗口失焦 / 看不见 ⇒ 运行中的点不呼吸（停在起点，颜色照旧），回到前台接着呼吸：呼吸每一帧都要重画，开着不看也一直在耗 CPU。
 * 只动那几段动画本身（不往根元素上写标记：WebKit 上根上一改整页重算样式，切回窗口那一下要多挨一两百毫秒）；
 * 不在前台时新冒出来的呼吸（点刚变成运行中 / 刚挂进页面）一冒出来就停。
 * 只认窗口那一层的焦点（页里元素之间换焦点不算）；装上那一刻按当时的状态办。返回值摘监听（判据用）。
 */
export function installDotRest(): () => void {
  let away = false;
  const mark = (next: boolean): void => {
    if (next === away) return;
    away = next;
    for (const a of document.getAnimations?.() ?? []) {
      if (!isBreath(a)) continue;
      if (next) rest(a);
      else a.play();
    }
  };
  const hidden = (): boolean => document.visibilityState === "hidden";
  // 失焦 / 得焦按事件本身认（事件到的那一刻 `hasFocus()` 各引擎未必已经翻过来）；
  // window 上不挂捕获：元素上的 focus / blur 不冒泡，到不了 window ⇒ 这里只收到窗口自己那一层的
  const onBlur = (): void => mark(true);
  const onFocus = (): void => mark(hidden());
  const onVisibility = (): void => mark(hidden() || !document.hasFocus());
  const onStart = (e: Event): void => {
    if (!away || !(e.target instanceof Element)) return;
    for (const a of e.target.getAnimations?.() ?? []) if (isBreath(a)) rest(a);
  };
  window.addEventListener("blur", onBlur);
  window.addEventListener("focus", onFocus);
  document.addEventListener("visibilitychange", onVisibility);
  document.addEventListener("animationstart", onStart, true);
  onVisibility();
  return () => {
    window.removeEventListener("blur", onBlur);
    window.removeEventListener("focus", onFocus);
    document.removeEventListener("visibilitychange", onVisibility);
    document.removeEventListener("animationstart", onStart, true);
  };
}
