/**
 * 悬停提示（C20）：悬停或键盘焦点停 500ms 出现，同一组里移到下一个立刻换；离开即消。
 *
 * - 第一行写这个东西是什么 / 现在怎样，第二行起才是补充；不放能点的东西。
 * - 挂 `document.body`（脱离带 `transform` 的祖先，`fixed` 才按视口算）、只在显示期间存在：
 *   宿主被销毁时提示本就不在 DOM 里；正显示着宿主没了 ⇒ 下一次显示前扫掉（残留上限 1 条）。
 * - 躲窗口边：放不下就翻到另一侧，贴边内缩 8px。
 */
import s from "./tooltip.module.css";

/** 悬停多久出现；离开后多久内移到下一个仍算「同一组」。 */
export const TOOLTIP_DELAY_MS = 500;
const GROUP_GRACE_MS = 300;
const EDGE = 8;
const GAP = 6;

const live = new Map<HTMLElement, HTMLElement>();
let lastHiddenAt = Number.NEGATIVE_INFINITY;

function sweep(): void {
  for (const [tip, owner] of [...live]) {
    if (!owner.isConnected) {
      tip.remove();
      live.delete(tip);
    }
  }
}

/** 仅供测试：此刻挂在 body 上的提示条数。 */
export function liveTooltipCount(): number {
  return live.size;
}

/** 摆到宿主上方居中；上面放不下翻到下方；左右夹进视口（纯函数，判据直接调）。 */
export function placeTip(host: DOMRect, tip: { width: number; height: number }, view: { width: number; height: number }): { left: number; top: number } {
  let left = host.left + host.width / 2 - tip.width / 2;
  let top = host.top - tip.height - GAP;
  if (top < EDGE) top = host.bottom + GAP;
  if (top + tip.height > view.height - EDGE) top = Math.max(EDGE, view.height - EDGE - tip.height);
  left = Math.max(EDGE, Math.min(left, view.width - EDGE - tip.width));
  return { left, top };
}

/**
 * 给 `host` 挂提示。`text` 可以是函数（显示那一刻现取：键位改了跟着变）。
 * 函数回一个元素 ⇒ 悬停卡（加长版：表格排的「键  值」，不放能点的东西）；回空串 / `null` ⇒ 这一次不出。
 * `immediate` ＝ 不等 500ms（信息图标那种点名要看的）。
 */
export function attachTooltip(
  host: HTMLElement,
  text: string | (() => string | HTMLElement | null),
  opts: { immediate?: boolean } = {},
): void {
  let tip: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const show = (): void => {
    sweep();
    const t = typeof text === "function" ? text() : text;
    if (t === "" || t === null) return;
    tip ??= document.createElement("div");
    tip.className = typeof t === "string" ? s.tip : `${s.tip} ${s.tipCard}`;
    tip.setAttribute("role", "tooltip");
    if (typeof t === "string") tip.textContent = t;
    else tip.replaceChildren(t);
    if (!tip.isConnected) {
      document.body.appendChild(tip);
      live.set(tip, host);
    }
    tip.style.visibility = "hidden";
    const r = tip.getBoundingClientRect();
    const at = placeTip(host.getBoundingClientRect(), r, { width: window.innerWidth, height: window.innerHeight });
    tip.style.left = `${at.left}px`;
    tip.style.top = `${at.top}px`;
    tip.style.visibility = "";
  };
  const arm = (): void => {
    if (timer !== null) clearTimeout(timer);
    const inGroup = Date.now() - lastHiddenAt < GROUP_GRACE_MS;
    if (opts.immediate || inGroup) return show();
    timer = setTimeout(show, TOOLTIP_DELAY_MS);
  };
  const hide = (): void => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    if (!tip?.isConnected) return;
    live.delete(tip);
    tip.remove();
    lastHiddenAt = Date.now();
  };
  host.addEventListener("mouseenter", arm);
  host.addEventListener("mouseleave", hide);
  host.addEventListener("focusin", arm);
  host.addEventListener("focusout", hide);
}
