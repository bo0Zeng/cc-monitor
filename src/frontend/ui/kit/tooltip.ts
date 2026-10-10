/**
 * 悬停提示：悬停或键盘焦点停 500ms 出现，指针在同一组里移到下一个立刻换；离开即消。
 * 键盘走行（↑↓ / Tab 换焦点）不套「同一组立刻换」：每换一个宿主先收起、重新等 500ms，停够了才出（走一路不逐行弹卡）。
 *
 * - 第一行写这个东西是什么 / 现在怎样，第二行起才是补充；不放能点的东西。
 * - 挂 `document.body`（脱离带 `transform` 的祖先，`fixed` 才按视口算）、只在显示期间存在：
 *   宿主被销毁时提示本就不在 DOM 里；正显示着宿主没了 ⇒ 下一次显示前扫掉（残留上限 1 条）。
 * - 摆在哪由 `place.ts` 那一处算（缺省上方居中、卡式右侧顶对齐 / 左侧居中；放不下翻到另一侧，贴边内缩 8px）。
 * 判据：`tests/frontend/ui/kit/tooltip.vitest.ts`。
 */
import s from "./tooltip.module.css";
import { placeFloat, putAt, type Align, type Side } from "./place";

/** 悬停多久出现；离开后多久内移到下一个仍算「同一组」。 */
export const TOOLTIP_DELAY_MS = 500;
const GROUP_GRACE_MS = 300;
const GAP = 6;
/** 三种摆法 ⇒ 哪一侧、怎么对齐。 */
const PLACEMENT: Record<NonNullable<TooltipOpts["placement"]>, { side: Side; align: Align }> = {
  above: { side: "above", align: "center" },
  right: { side: "right", align: "start" },
  left: { side: "left", align: "center" },
};

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

/** 此刻压着悬停提示的那几层（右键菜单 / 下拉开着）：它们开着时，只有宿主在这些层里面的提示才出（菜单里那几项的说明照常）。 */
const holders = new Set<HTMLElement>();

/**
 * 一层浮层（菜单）开着期间压住悬停提示：已出的收掉；等着出的、指针再动想出的，都不出 —— 悬停卡不许压在菜单上。
 * 宿主在 `layer` 里面的照常出。回收手（浮层关掉时调一次）。
 */
export function holdTooltips(layer: HTMLElement): () => void {
  holders.add(layer);
  hideTooltips();
  return () => void holders.delete(layer);
}

/** 宿主此刻能不能出提示：没有浮层压着 · 或宿主就在压着的那一层里。 */
function mayShow(host: HTMLElement): boolean {
  if (holders.size === 0) return true;
  for (const layer of holders) if (layer.isConnected && layer.contains(host)) return true;
  for (const layer of [...holders]) if (!layer.isConnected) holders.delete(layer);
  return holders.size === 0;
}

/** 收起挂在 `host` 上、此刻显示着的那一条（宿主的说明变了 / 不再适用时：按钮从禁用恢复可点）。 */
export function hideTooltipOf(host: HTMLElement): void {
  for (const [tip, owner] of [...live]) if (owner === host) hiders.get(tip)?.();
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
function controller(content: (host: HTMLElement) => TipContent, opts: TooltipOpts): { arm(host: HTMLElement, by?: "pointer" | "focus"): void; leave(): void; hide(): void; host(): HTMLElement | null } {
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
    // 调度：一次性 —— 卡式离开宿主与卡 120ms 才收，移进卡即清
    closer = setTimeout(hide, CARD_CLOSE_MS);
  }
  const show = (): void => {
    sweep();
    for (const [other, h] of [...hiders]) {
      if (!other.isConnected) hiders.delete(other);
      else if (other !== tip) h();
    }
    if (!host?.isConnected || !mayShow(host)) return;
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
    putAt(tip, placeFloat({ rect: box, ...PLACEMENT[opts.placement ?? "above"], gap: GAP }, r, view));
    tip.style.visibility = "";
  };
  const arm = (h: HTMLElement, by: "pointer" | "focus" = "pointer"): void => {
    clearCloser();
    clearTimer();
    const same = h === host;
    host = h;
    if (same && shown()) return;
    if (by === "focus" && !opts.immediate) {
      // 键盘走行：换到别的宿主先收起这张（以及别处出着的），停够 500ms 才出。
      hide();
      hideTooltips();
      // 调度：一次性 —— 焦点停 500ms 才出提示，焦点移走即清
      timer = setTimeout(show, TOOLTIP_DELAY_MS);
      return;
    }
    // 钟往回拨了（系统改时间）那一下不算「刚收过」。
    const since = Date.now() - lastHiddenAt;
    const inGroup = (since >= 0 && since < GROUP_GRACE_MS) || [...hiders.keys()].some((t) => t.isConnected);
    if (opts.immediate || inGroup) return show();
    // 调度：一次性 —— 悬停 500ms 才出提示，离开即清
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
  host.addEventListener("focusin", () => c.arm(host, "focus"));
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
    if (el) c.arm(el, "focus");
  });
  root.addEventListener("focusout", (e) => {
    if (hit(e.target) && hit(e.target) !== hit(e.relatedTarget)) c.hide();
  });
}

/**
 * 全产品的 `title` 属性改走本模块的悬停提示：系统自己画的那种提示（WebKitGTK 黑底白字 · WebView2 各版本各样）不跟主题、
 * 不按 500ms 节奏、也不摆在宿主上方。带 `title` 的元素一挂上（或代码改了它的 `title`）就把它挪进 `data-kit-title`（系统提示就不出了），
 * 再按这里的节奏出提示；指针 / 焦点进来那一刻再兜一次。三个窗口的入口各装一次（`entry-common.ts`）。
 * 读屏（按 accname 判，{@link adoptTitle}）：已有可访问名的不覆盖，那句挂成说明（`aria-description`），与名字相同就不挂；
 * 只有 title 能当名字的（图标按钮 · 可聚焦的一行）才把它写成 `aria-label`。不等悬停：读屏不悬停。
 */
export function adoptNativeTitles(doc: Document = document): void {
  const hostOf = (t: EventTarget | null): HTMLElement | null => {
    if (!(t instanceof Element) || typeof t.closest !== "function") return null;
    return t.closest<HTMLElement>("[title], [data-kit-title]");
  };
  const take = adoptTitle;
  const c = controller((h) => {
    const t = h.dataset.kitTitle ?? null;
    return t !== null && shownInFull(h, t) ? null : t;
  }, {});
  const enter = (e: Event): void => {
    const el = hostOf(e.target);
    if (!el) return;
    take(el);
    c.arm(el, e.type === "focusin" ? "focus" : "pointer");
  };
  const out = (e: Event & { relatedTarget?: EventTarget | null }): void => {
    const from = hostOf(e.target);
    if (from && from !== hostOf(e.relatedTarget ?? null)) c.leave();
  };
  // 一挂上 / 代码改了 title 就接管（读屏不悬停）；指针 / 焦点进来那一下是兜底。
  for (const el of doc.querySelectorAll<HTMLElement>("[title]")) adoptTitle(el);
  if (typeof MutationObserver === "function" && doc.documentElement) {
    new MutationObserver((records) => {
      for (const r of records) {
        if (r.type === "attributes") {
          if (r.target instanceof HTMLElement) adoptTitle(r.target);
          continue;
        }
        for (const n of r.addedNodes) {
          if (!(n instanceof HTMLElement)) continue;
          if (n.hasAttribute("title")) adoptTitle(n);
          for (const el of n.querySelectorAll<HTMLElement>("[title]")) adoptTitle(el);
        }
      }
    }).observe(doc.documentElement, { subtree: true, childList: true, attributes: true, attributeFilter: ["title"] });
  }
  doc.addEventListener("mouseover", enter, true);
  doc.addEventListener("focusin", enter, true);
  doc.addEventListener("mouseout", out, true);
  doc.addEventListener("focusout", (e) => {
    const from = hostOf(e.target);
    if (from && from !== hostOf(e.relatedTarget)) c.hide();
  }, true);
  doc.addEventListener("keydown", (e) => {
    if (e.key === "Escape") c.hide();
  }, true);
}

/**
 * 接管一个元素的 `title`（挪进 `data-kit-title`），读屏那一面按 accname 摆：
 * - 名字来自别处（labelledby · aria-label · 名字来自内容的角色带着字 · 表单控件的 label）⇒ 名字不动；那句与名字不同才挂成说明。
 * - 名字只能来自 title：可聚焦的、或没有字的 ⇒ 写成 `aria-label`（不另挂说明）；不可聚焦、自己有字的（头上那枚标签）⇒ 字照读、那句挂成说明。
 * 自己写上的 `aria-label` / `aria-description` 记了号，再接管或 title 被撤掉（`data-kit-title` 也没了）时先撤掉。
 */
export function adoptTitle(el: HTMLElement): void {
  const t = el.getAttribute("title");
  if (t === null) {
    if (el.dataset.kitTitle === undefined) dropAdopted(el);
    return;
  }
  dropAdopted(el);
  el.removeAttribute("title");
  if (t.trim() === "") {
    delete el.dataset.kitTitle;
    return;
  }
  el.dataset.kitTitle = t;
  const name = nameNotFromTitle(el);
  const text = norm(el.textContent ?? "");
  if (name !== "") {
    if (norm(t) !== name) setOwn(el, "aria-description", t);
  } else if (focusable(el) || text === "") {
    setOwn(el, "aria-label", t);
  } else if (norm(t) !== text) {
    setOwn(el, "aria-description", t);
  }
}

/** 收起 `adoptTitle` 自己写上的那几格（宿主的 title 被代码撤掉 / 换了）。 */
export function dropAdopted(el: HTMLElement): void {
  for (const a of (el.dataset.kitAria ?? "").split(" ")) if (a) el.removeAttribute(a);
  delete el.dataset.kitAria;
}

function setOwn(el: HTMLElement, attr: "aria-label" | "aria-description", v: string): void {
  el.setAttribute(attr, v);
  el.dataset.kitAria = [...new Set([...(el.dataset.kitAria ?? "").split(" ").filter(Boolean), attr])].join(" ");
}

const norm = (x: string): string => x.replace(/\s+/g, " ").trim();

/** 可聚焦（读屏落焦点时要念名字）。 */
function focusable(el: HTMLElement): boolean {
  return el.matches("button, a[href], input, textarea, select, summary, [tabindex], [contenteditable=\"true\"]");
}

/** 不算 title 时的可访问名（accname 1.2 与 title 相关的那几步；名字来自内容取文字）。没有 ⇒ 空串。 */
function nameNotFromTitle(el: HTMLElement): string {
  const by = el.getAttribute("aria-labelledby")?.trim();
  if (by) return norm(by.split(/\s+/).map((id) => el.ownerDocument.getElementById(id)?.textContent ?? "").join(" "));
  const label = el.getAttribute("aria-label");
  if (label?.trim() && !(el.dataset.kitAria ?? "").includes("aria-label")) return norm(label);
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) {
    return norm([...(el.labels ?? [])].map((l) => l.textContent ?? "").join(" "));
  }
  if (nameFromElsewhere(el)) return norm(el.textContent ?? "");
  return "";
}

/** 名字来自内容的那几种（accname 1.2）：按钮 · 链接 · 菜单项 · 选项 · 标签页 · 格。 */
const NAME_FROM_CONTENT_TAGS = new Set(["BUTTON", "A", "SUMMARY", "OPTION"]);
const NAME_FROM_CONTENT_ROLES = new Set(["button", "link", "menuitem", "menuitemradio", "menuitemcheckbox", "option", "tab", "tooltip", "cell", "row"]);

/** 这个元素的可访问名是不是来自 title 以外的地方（labelledby · aria-label · 名字来自内容的角色带着字）。 */
function nameFromElsewhere(el: HTMLElement): boolean {
  if (el.getAttribute("aria-labelledby")?.trim()) return true;
  if (el.getAttribute("aria-label")?.trim()) return true;
  const role = el.getAttribute("role");
  const fromContent = NAME_FROM_CONTENT_TAGS.has(el.tagName) || (role !== null && NAME_FROM_CONTENT_ROLES.has(role));
  return fromContent && (el.textContent ?? "").trim() !== "";
}

/** 提示要说的那句已经整句显示在宿主里、没被截断 ⇒ 不再弹一遍（历史页会话名那种）。 */
function shownInFull(host: HTMLElement, text: string): boolean {
  const want = text.trim();
  // 最里层那个整句等于它的元素（截断看的是它自己的宽）；没有 ⇒ 宿主自己。
  const leaf = [...host.querySelectorAll<HTMLElement>("*")].find((el) => el.children.length === 0 && (el.textContent ?? "").trim() === want);
  const el = leaf ?? ((host.textContent ?? "").trim() === want ? host : null);
  return el !== null && el.scrollWidth <= el.clientWidth + 1;
}
