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
export function __liveTooltipCountForTests(): number {
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
 * 卡锚在宿主右侧、顶对齐（标签页悬停卡）：右边放不下翻到左侧；上下夹进视口（纯函数，判据直接调）。
 */
export function placeCardRight(host: DOMRect, tip: { width: number; height: number }, view: { width: number; height: number }): { left: number; top: number } {
  let left = host.right + GAP;
  if (left + tip.width > view.width - EDGE) left = Math.max(EDGE, host.left - GAP - tip.width);
  let top = host.top;
  if (top + tip.height > view.height - EDGE) top = Math.max(EDGE, view.height - EDGE - tip.height);
  return { left, top };
}

/**
 * 卡锚在宿主左侧、竖直居中（消息流右缘的轮次刻度）：左边放不下翻到右侧；上下夹进视口（纯函数，判据直接调）。
 */
export function placeCardLeft(host: DOMRect, tip: { width: number; height: number }, view: { width: number; height: number }): { left: number; top: number } {
  let left = host.left - GAP - tip.width;
  if (left < EDGE) left = Math.min(host.right + GAP, view.width - EDGE - tip.width);
  let top = host.top + host.height / 2 - tip.height / 2;
  top = Math.max(EDGE, Math.min(top, view.height - EDGE - tip.height));
  return { left, top };
}

/** 卡式（`hold`）离开宿主与卡之后多久关。 */
export const CARD_CLOSE_MS = 120;

/** 每条正显示着的提示怎么收（全局收起用：右键菜单弹出时）。 */
const hiders = new Map<HTMLElement, () => void>();

/** 收起此刻显示着的全部悬停提示（右键菜单 / 下拉弹出时调：提示不许压在菜单上）。 */
export function hideTooltips(): void {
  for (const [t, h] of [...hiders]) {
    if (t.isConnected) h();
    else hiders.delete(t);
  }
}

export interface TooltipOpts {
  /** 不等 500ms（信息图标那种点名要看的）。 */
  immediate?: boolean;
  /** `right` ＝ 卡锚在宿主右侧、顶对齐（标签页栏的悬停卡）；`left` ＝ 左侧、竖直居中（轮次刻度）；缺省在上方居中。 */
  placement?: "above" | "right" | "left";
  /** 卡可以被指针移进去（离开宿主与卡 [`CARD_CLOSE_MS`] 后才关）；卡里仍不放能点的东西。 */
  hold?: boolean;
  /** 卡的宽（px）；缺省按内容。 */
  width?: () => number | null;
}

type TipContent = string | HTMLElement | null;

/**
 * 一条提示的控制器：显示 / 收起 / 进出宿主。宿主在 `arm` 时给（委托式里每次指针进的是哪一行就是哪一个）。
 * `attachTooltip`（一个宿主四个监听器）与 `delegateTooltip`（一个容器四个监听器、管它里面的每一行）共用这一份。
 */
function controller(content: (host: HTMLElement) => TipContent, opts: TooltipOpts): { arm(host: HTMLElement): void; leave(): void; hide(): void; host(): HTMLElement | null } {
  let tip: HTMLElement | null = null;
  let host: HTMLElement | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let closer: ReturnType<typeof setTimeout> | null = null;
  const clearTimer = (): void => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const clearCloser = (): void => {
    if (closer !== null) clearTimeout(closer);
    closer = null;
  };
  const shown = (): boolean => tip?.isConnected === true;
  function hide(): void {
    clearCloser();
    clearTimer();
    if (!tip?.isConnected) return;
    live.delete(tip);
    hiders.delete(tip);
    tip.remove();
    lastHiddenAt = Date.now();
  }
  function leave(): void {
    clearTimer();
    if (!opts.hold || !shown()) return hide();
    clearCloser();
    closer = setTimeout(hide, CARD_CLOSE_MS);
  }
  const show = (): void => {
    sweep();
    for (const [other, h] of [...hiders]) {
      if (!other.isConnected) hiders.delete(other);
      else if (other !== tip) h();
    }
    if (!host?.isConnected) return;
    const t = content(host);
    if (t === "" || t === null) return hide();
    tip ??= document.createElement("div");
    tip.className = typeof t === "string" ? s.tip : opts.hold ? `${s.tip} ${s.tipCard} ${s.tipHold}` : `${s.tip} ${s.tipCard}`;
    tip.setAttribute("role", "tooltip");
    if (typeof t === "string") tip.textContent = t;
    else tip.replaceChildren(t);
    const w = opts.width?.() ?? null;
    tip.style.width = w === null ? "" : `${w}px`;
    if (!tip.isConnected) {
      document.body.appendChild(tip);
      live.set(tip, host);
      hiders.set(tip, hide);
      if (opts.hold && tip.dataset.held !== "1") {
        tip.dataset.held = "1";
        tip.addEventListener("mouseenter", clearCloser);
        tip.addEventListener("mouseleave", leave);
      }
    }
    tip.style.visibility = "hidden";
    const r = tip.getBoundingClientRect();
    const view = { width: window.innerWidth, height: window.innerHeight };
    const box = host.getBoundingClientRect();
    const at = opts.placement === "right" ? placeCardRight(box, r, view) : opts.placement === "left" ? placeCardLeft(box, r, view) : placeTip(box, r, view);
    tip.style.left = `${at.left}px`;
    tip.style.top = `${at.top}px`;
    tip.style.visibility = "";
  };
  const arm = (h: HTMLElement): void => {
    clearCloser();
    clearTimer();
    const same = h === host;
    host = h;
    if (same && shown()) return;
    const inGroup = Date.now() - lastHiddenAt < GROUP_GRACE_MS || [...hiders.keys()].some((t) => t.isConnected);
    if (opts.immediate || inGroup) return show();
    timer = setTimeout(show, TOOLTIP_DELAY_MS);
  };
  return { arm, leave, hide, host: () => host };
}

/**
 * 给 `host` 挂提示。`text` 可以是函数（显示那一刻现取：键位改了跟着变）。
 * 函数回一个元素 ⇒ 悬停卡（加长版：表格排的「键  值」，不放能点的东西）；回空串 / `null` ⇒ 这一次不出。
 * 同一时刻只显示一条：别的正开着 ⇒ 这一条立刻出、那一条收起（在一列标签页上下移时卡跟着换，不再等）。
 */
export function attachTooltip(
  host: HTMLElement,
  text: string | (() => TipContent),
  opts: TooltipOpts = {},
): void {
  const c = controller(() => (typeof text === "function" ? text() : text), opts);
  host.addEventListener("mouseenter", () => c.arm(host));
  host.addEventListener("mouseleave", () => c.leave());
  host.addEventListener("focusin", () => c.arm(host));
  host.addEventListener("focusout", () => c.hide());
  host.addEventListener("keydown", (e) => {
    if (e.key === "Escape") c.hide();
  });
}

/**
 * **委托式**：`root` 里每个合 `selector` 的元素都有提示，监听器只挂在 `root` 上（一列标签页零个逐行监听器）。
 * 指针从一行移到下一行 ⇒ 卡跟着换（同一组立刻换）；`content(el)` 显示那一刻现取，回 `null` ⇒ 这一行不出。
 */
export function delegateTooltip(root: HTMLElement, selector: string, content: (el: HTMLElement) => TipContent, opts: TooltipOpts = {}): void {
  const c = controller(content, opts);
  const hit = (t: EventTarget | null): HTMLElement | null => {
    if (!(t instanceof Element) || typeof t.closest !== "function") return null;
    const el = t.closest<HTMLElement>(selector);
    return el && root.contains(el) ? el : null;
  };
  root.addEventListener("mouseover", (e) => {
    const el = hit(e.target);
    if (el) c.arm(el);
  });
  root.addEventListener("mouseout", (e) => {
    const from = hit(e.target);
    if (from && from !== hit(e.relatedTarget)) c.leave();
  });
  root.addEventListener("focusin", (e) => {
    const el = hit(e.target);
    if (el) c.arm(el);
  });
  root.addEventListener("focusout", (e) => {
    if (hit(e.target) && hit(e.target) !== hit(e.relatedTarget)) c.hide();
  });
}
