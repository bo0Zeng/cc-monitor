/**
 * 弹出菜单（C12）：全产品一个实现 —— 右键菜单、按钮下拉、选主机、选账号都走它。定位、关法、键盘、压进 Esc 栈都在这里。
 *
 * - 锚在一点（右键）或一个触发物（按钮）上；躲窗口边：放不下就翻到另一侧，翻过去还放不下贴边内缩 8px。
 * - 关法：Esc（弹层栈，一次只关最上一层）· 点外面 · 再点触发物 · 选了一项。同一时刻只开一个。
 * - 键盘：↑↓ 走（跳过不可选）、Enter / 空格选、→ 开子菜单、← 收、Home / End 到两头；先判输入法组字。
 * - 项：28 高、图标 16、右侧键位；危险项红字放最后一组；不可选的灰着、悬停说为什么；有子菜单的悬停 150ms 展开、离开 250ms 收起。
 * - 异步就绪：开着时可按 id 换项 / 追加 / 摘掉；代次（[`menuGeneration`]）让在飞的就绪回调认得「还是不是这一代菜单」。
 */
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { icon, type IconName } from "./icon";
import s from "./menu.module.css";
import { hideTooltips } from "./tooltip";
import { spinner } from "./progress";

export interface MenuItem {
  id?: string;
  label: string;
  icon?: IconName;
  /** 右侧灰字：键位 / 补充（邮箱等）；`detailTone: "warn"` ＝ 那段是警示态（琥珀）。 */
  detail?: string;
  detailTone?: "warn";
  /** 右侧灰字跟着同一菜单里的单选走：开的时候与每点一次单选之后各取一次（缺 ⇒ 用 `detail`）。 */
  detailOf?: () => string;
  /** 紧跟在字后面的一段灰字（邮箱）。 */
  note?: string;
  /** 字前面的身份块（账号头像 · 状态点）。 */
  avatar?: HTMLElement;
  /** 第二行（引用的那一句 / 命令）：等宽小字，至多两行、余下省略；有它这一项就是两行高。 */
  body?: string;
  /** 当前项：左侧对勾 ＋ 600 字重（下拉、选账号）。 */
  checked?: boolean;
  /** `false` ＝ 灰着不可点；为什么写进 `why`（第二行 12px）。 */
  enabled?: boolean;
  /** 灰着的那一项为什么（第二行，`gpu-01 不可见`）；不写成「X（为什么）」。 */
  why?: string;
  /** 还在问（那台答回来才定能不能点）：先画出项、右侧 14px 转圈、不可点。 */
  pending?: boolean;
  danger?: boolean;
  title?: string;
  onClick?: () => void;
  /** 有子菜单时 `onClick` 不用：这一级只负责展开。 */
  submenu?: MenuItem[];
  /** 分隔线（其余字段不看）。 */
  divider?: boolean;
  /** 组名那一行（只读的小字，`label` 是组名；其余字段不看）。 */
  heading?: boolean;
  /**
   * 单选组的组名：同一层里同组名的几项是一组单选，点一项 ⇒ 勾挪到它、菜单不关、再调 `onClick`。
   * 当前勾在哪一项由 `checked: true` 给。
   */
  radio?: string;
}

export type MenuAnchor = { x: number; y: number } | { el: HTMLElement; align?: "start" | "end" };

interface Open {
  root: HTMLElement;
  anchor: MenuAnchor;
  items: Map<string, HTMLElement>;
  layer: OverlayHandle;
  timers: ReturnType<typeof setTimeout>[];
  onClose?: () => void;
}

const EDGE = 8;
const OPEN_SUB_MS = 150;
const CLOSE_SUB_MS = 250;

let current: Open | null = null;
/** 跟着单选走的那几格右侧灰字（`detailOf`）。 */
const liveDetails = new WeakMap<HTMLElement, () => string>();
let generation = 0;

/** 这一代菜单的代次（开 / 关菜单都自增）。 */
export function menuGeneration(): number {
  return generation;
}

/** 此刻开着没有（tab 栏多选那一层见菜单开着就让 Esc 给它）。 */
export function menuOpen(): boolean {
  return current !== null;
}

/** 此刻开着的菜单是不是锚在这个触发物上（再点触发物 ＝ 关）。 */
export function menuAnchoredOn(el: HTMLElement): boolean {
  return current !== null && "el" in current.anchor && current.anchor.el === el;
}

/** 纯函数：在 (x, y) 处放一块 w×h，躲开视口边。 */
export function placeAt(x: number, y: number, w: number, h: number, vw: number, vh: number): { left: number; top: number } {
  const fits = (at: number, size: number, room: number): boolean => at + size <= room - EDGE;
  let left = fits(x, w, vw) ? x : x - w;
  let top = fits(y, h, vh) ? y : y - h;
  left = Math.max(EDGE, Math.min(left, vw - EDGE - w));
  top = Math.max(EDGE, Math.min(top, vh - EDGE - h));
  return { left, top };
}

function place(o: Open): void {
  const { width, height } = o.root.getBoundingClientRect();
  let x: number;
  let y: number;
  if ("el" in o.anchor) {
    const r = o.anchor.el.getBoundingClientRect();
    x = o.anchor.align === "end" ? r.right - width : r.left;
    y = r.bottom + 4;
    if (y + height > window.innerHeight - EDGE) y = r.top - 4 - height;
  } else {
    x = o.anchor.x;
    y = o.anchor.y;
  }
  const at = placeAt(x, y, width, height, window.innerWidth, window.innerHeight);
  o.root.style.left = `${at.left}px`;
  o.root.style.top = `${at.top}px`;
}

function focusables(panel: HTMLElement): HTMLButtonElement[] {
  return [...panel.querySelectorAll<HTMLButtonElement>(':scope > [role^="menuitem"], :scope > [role="none"] > [role^="menuitem"]')].filter(
    (b) => !b.disabled,
  );
}

function makeItem(o: Open, it: MenuItem): HTMLElement {
  if (it.divider) {
    const sep = document.createElement("div");
    sep.className = s.menuDivider;
    sep.setAttribute("role", "separator");
    return sep;
  }
  if (it.heading) {
    const h = document.createElement("div");
    h.className = s.menuHeading;
    h.setAttribute("role", "presentation");
    h.textContent = it.label;
    return h;
  }
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = s.menuItem;
  const radio = it.radio !== undefined;
  btn.setAttribute("role", it.checked !== undefined || radio ? "menuitemradio" : "menuitem");
  if (it.checked !== undefined || radio) btn.setAttribute("aria-checked", String(it.checked === true));
  if (radio) btn.dataset.radio = it.radio;
  if (it.danger) btn.dataset.variant = "danger";
  const lead = document.createElement("span");
  lead.className = s.menuLead;
  if (it.checked) lead.appendChild(icon("check"));
  else if (it.icon) lead.appendChild(icon(it.icon));
  const label = document.createElement("span");
  label.className = s.menuLabel;
  label.dataset.part = "label";
  label.textContent = it.label;
  btn.appendChild(lead);
  if (it.avatar) btn.appendChild(it.avatar);
  btn.appendChild(label);
  if (it.note) {
    const n = document.createElement("span");
    n.className = s.menuNote;
    n.textContent = it.note;
    btn.appendChild(n);
  }
  const detailText = it.detailOf ? it.detailOf() : it.detail;
  if (detailText) {
    const d = document.createElement("span");
    d.className = s.menuDetail;
    d.dataset.part = "detail";
    if (it.detailTone) d.dataset.intent = it.detailTone;
    d.textContent = detailText;
    btn.appendChild(d);
    if (it.detailOf) liveDetails.set(d, it.detailOf);
  }
  if (it.pending) btn.appendChild(spinner());
  if (it.body || it.why) {
    btn.dataset.twoLine = "true";
    const b = document.createElement("span");
    b.className = it.body ? s.menuBody : s.menuWhy;
    b.dataset.part = it.body ? "body" : "why";
    b.textContent = it.body ?? it.why ?? "";
    btn.appendChild(b);
  }
  if (it.title) btn.title = it.title;
  const enabled = it.enabled !== false && !it.pending;
  btn.disabled = !enabled;

  if (it.submenu && it.submenu.length > 0) {
    btn.setAttribute("aria-haspopup", "menu");
    btn.appendChild(icon("caretRight", "compact"));
    const wrap = document.createElement("div");
    wrap.className = s.menuWrap;
    wrap.setAttribute("role", "none");
    const fly = document.createElement("div");
    fly.className = s.menu;
    fly.dataset.sub = "true";
    fly.setAttribute("role", "menu");
    for (const sub of it.submenu) fly.appendChild(makeItem(o, sub));
    wrap.append(btn, fly);
    if (!enabled) return wrap;
    let openT: ReturnType<typeof setTimeout> | null = null;
    let closeT: ReturnType<typeof setTimeout> | null = null;
    const clear = (): void => {
      if (openT !== null) clearTimeout(openT);
      if (closeT !== null) clearTimeout(closeT);
      openT = closeT = null;
    };
    const openSub = (focusFirst: boolean): void => {
      clear();
      const r = wrap.getBoundingClientRect();
      const w = fly.getBoundingClientRect().width || 160;
      if (r.right + w > window.innerWidth - EDGE) fly.dataset.flip = "true";
      else delete fly.dataset.flip;
      wrap.dataset.subOpen = "true";
      btn.setAttribute("aria-expanded", "true");
      if (focusFirst) focusables(fly)[0]?.focus();
    };
    const closeSub = (): void => {
      clear();
      delete wrap.dataset.subOpen;
      btn.setAttribute("aria-expanded", "false");
    };
    wrap.addEventListener("mouseenter", () => {
      clear();
      openT = setTimeout(() => openSub(false), OPEN_SUB_MS);
      o.timers.push(openT);
    });
    wrap.addEventListener("mouseleave", () => {
      clear();
      closeT = setTimeout(closeSub, CLOSE_SUB_MS);
      o.timers.push(closeT);
    });
    btn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      if (wrap.dataset.subOpen === "true") closeSub();
      else openSub(false);
    });
    btn.addEventListener("keydown", (ev) => {
      if (ev.isComposing) return;
      if (ev.key === "ArrowRight") {
        ev.preventDefault();
        ev.stopPropagation();
        openSub(true);
      }
    });
    fly.addEventListener("keydown", (ev) => {
      if (ev.isComposing) return;
      if (ev.key === "ArrowLeft") {
        ev.preventDefault();
        ev.stopPropagation();
        closeSub();
        btn.focus();
      }
    });
    return wrap;
  }
  if (enabled && radio) {
    btn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      checkRadio(btn);
      it.onClick?.();
      for (const d of o.root.querySelectorAll<HTMLElement>(`[data-part="detail"]`)) {
        const f = liveDetails.get(d);
        if (f) d.textContent = f();
      }
    });
  } else if (enabled) {
    btn.addEventListener("click", () => {
      closeMenu();
      it.onClick?.();
    });
  }
  return btn;
}

/** 单选组里勾挪到这一项（同一层、同组名的其余几项去勾）。 */
function checkRadio(btn: HTMLButtonElement): void {
  const panel = btn.parentElement;
  if (!panel) return;
  for (const b of panel.querySelectorAll<HTMLButtonElement>(":scope > [data-radio]")) {
    if (b.dataset.radio !== btn.dataset.radio) continue;
    const on = b === btn;
    b.setAttribute("aria-checked", String(on));
    // 字前面那一格是每一项的第一个孩子（`makeItem` 先放它）。
    b.firstElementChild?.replaceChildren(...(on ? [icon("check")] : []));
  }
}

function onPointer(ev: PointerEvent): void {
  const o = current;
  if (!o) return;
  const t = ev.target as Node | null;
  if (t && o.root.contains(t)) return;
  if (t && "el" in o.anchor && o.anchor.el.contains(t)) return;
  closeMenu();
}

function onKey(ev: KeyboardEvent): void {
  const o = current;
  if (!o || ev.isComposing || ev.keyCode === 229) return;
  const panel = (document.activeElement?.closest('[role="menu"]') as HTMLElement | null) ?? o.root;
  const f = focusables(panel);
  if (f.length === 0) return;
  const i = f.indexOf(document.activeElement as HTMLButtonElement);
  let to = -1;
  if (ev.key === "ArrowDown") to = i < 0 ? 0 : (i + 1) % f.length;
  else if (ev.key === "ArrowUp") to = i < 0 ? f.length - 1 : (i - 1 + f.length) % f.length;
  else if (ev.key === "Home") to = 0;
  else if (ev.key === "End") to = f.length - 1;
  if (to < 0) return;
  ev.preventDefault();
  f[to].focus();
}

/**
 * 开一个菜单（先关掉开着的那个）。锚在触发物上且它开着 ⇒ 当作「再点一次」：关掉、不再开。
 * @returns 开没开（`false` ＝ 这一下是关）。
 */
export function openMenu(anchor: MenuAnchor, items: MenuItem[], opts: { onClose?: () => void; label?: string; width?: number } = {}): boolean {
  if ("el" in anchor && menuAnchoredOn(anchor.el)) {
    closeMenu();
    return false;
  }
  closeMenu();
  // 菜单弹出时收起悬停提示 / 悬停卡：不许压在菜单第一项上。
  hideTooltips();
  const root = document.createElement("div");
  root.className = s.menu;
  root.setAttribute("role", "menu");
  if (opts.label) root.setAttribute("aria-label", opts.label);
  if ("el" in anchor && anchor.el.closest('[aria-modal="true"]')) root.dataset.overModal = "true";
  if (opts.width !== undefined) {
    root.style.width = `${opts.width}px`;
    root.style.maxWidth = `min(${opts.width}px, calc(100vw - ${2 * EDGE}px))`;
  }
  const layer: OverlayHandle = {
    handleEsc: () => {
      closeMenu();
      return true;
    },
  };
  const o: Open = { root, anchor, items: new Map(), layer, timers: [], onClose: opts.onClose };
  current = o;
  for (const it of items) {
    const el = makeItem(o, it);
    if (it.id) o.items.set(it.id, el);
    root.appendChild(el);
  }
  document.body.appendChild(root);
  place(o);
  generation++;
  dispatcher.pushOverlay(layer);
  if ("el" in anchor) anchor.el.setAttribute("aria-expanded", "true");
  // 锚在触发物上的：点触发物本身不算「点外面」⇒ 当场挂。右键开的：下一拍再挂（开菜单这一下自己的 pointerdown 不算）。
  if ("el" in anchor) window.addEventListener("pointerdown", onPointer, true);
  else
    o.timers.push(
      setTimeout(() => {
        if (current !== o) return;
        window.addEventListener("pointerdown", onPointer, true);
      }, 0),
    );
  root.addEventListener("keydown", onKey);
  if (!("x" in anchor)) focusables(root)[0]?.focus();
  return true;
}

export function closeMenu(): void {
  const o = current;
  if (!o) return;
  current = null;
  for (const t of o.timers) clearTimeout(t);
  o.root.remove();
  generation++;
  dispatcher.popOverlay(o.layer);
  window.removeEventListener("pointerdown", onPointer, true);
  if ("el" in o.anchor) o.anchor.el.setAttribute("aria-expanded", "false");
  o.onClose?.();
}

/** 开着的菜单里按 id 换一项（原项展开着子菜单 ⇒ 换上去的照样展开）。菜单已关 / 无此 id ⇒ 不做。 */
export function updateMenuItem(id: string, item: MenuItem): void {
  const o = current;
  const old = o?.items.get(id);
  if (!o || !old) return;
  const el = makeItem(o, item);
  if (old.dataset.subOpen === "true") el.dataset.subOpen = "true";
  o.items.delete(id);
  o.items.set(item.id ?? id, el);
  old.replaceWith(el);
  place(o);
}

export function removeMenuItem(id: string): void {
  const o = current;
  const old = o?.items.get(id);
  if (!o || !old) return;
  old.remove();
  o.items.delete(id);
}

