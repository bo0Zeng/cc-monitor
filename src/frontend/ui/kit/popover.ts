/**
 * 浮层面板：锚在一个触发物上、里面摆一组控件（筛选那种「勾几个、选几个，改了立刻生效、面板不关」）。
 * 菜单是「选一项就关」，这里是「点外面 / Esc / 再点触发物才关」—— 两件事分开，定位与关法同一套（[`placeAt`]、Esc 栈）。
 *
 * - 同一时刻只开一个；开新的先关旧的。打开时收起悬停提示，焦点进面板第一个可聚焦的控件；关了焦点回触发物。
 * - Esc：弹层栈（一次只关最上一层）；输入法组字时不算。
 */
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { placeAt } from "./menu";
import { hideTooltips } from "./tooltip";
import s from "./popover.module.css";

interface Open {
  root: HTMLElement;
  anchor: HTMLElement;
  layer: OverlayHandle;
  onClose?: () => void;
}

let current: Open | null = null;

function onPointer(ev: PointerEvent): void {
  const o = current;
  if (!o) return;
  const t = ev.target as Node | null;
  if (t && (o.root.contains(t) || o.anchor.contains(t))) return;
  closePopover();
}

/** 这个触发物上开着一个面板吗（再点它 ⇒ 关）。 */
export function popoverOpenOn(anchor: HTMLElement): boolean {
  return current?.anchor === anchor;
}

/**
 * 在 `anchor` 下方（默认右端对齐，`align: "start"` 左端对齐）开一个面板，装 `content`。已在它上面开着 ⇒ 关掉、回 `false`。
 * `label` = 读屏名（文案表）。
 */
export function openPopover(anchor: HTMLElement, content: HTMLElement, opts: { label: string; onClose?: () => void; align?: "start" | "end" }): boolean {
  if (popoverOpenOn(anchor)) {
    closePopover();
    return false;
  }
  closePopover();
  hideTooltips();
  const root = document.createElement("div");
  root.className = s.popover;
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-label", opts.label);
  root.appendChild(content);
  const layer: OverlayHandle = {
    handleEsc: () => {
      closePopover();
      return true;
    },
  };
  current = { root, anchor, layer, onClose: opts.onClose };
  document.body.appendChild(root);
  const r = anchor.getBoundingClientRect();
  const { width, height } = root.getBoundingClientRect();
  // 默认右端与触发物对齐；`start` ＝ 左端对齐（触发物在一行的左头时）。
  const x = opts.align === "start" ? r.left : r.right - width;
  const at = placeAt(x, r.bottom + 4, width, height, window.innerWidth, window.innerHeight);
  root.style.left = `${at.left}px`;
  root.style.top = `${at.top}px`;
  dispatcher.pushOverlay(layer);
  anchor.setAttribute("aria-expanded", "true");
  window.addEventListener("pointerdown", onPointer, true);
  root.querySelector<HTMLElement>("button, input, [tabindex]")?.focus();
  return true;
}

/** 关掉开着的那个面板（没有就什么都不做）。 */
export function closePopover(): void {
  const o = current;
  if (!o) return;
  current = null;
  o.root.remove();
  dispatcher.popOverlay(o.layer);
  window.removeEventListener("pointerdown", onPointer, true);
  o.anchor.setAttribute("aria-expanded", "false");
  if (o.root.contains(document.activeElement) || document.activeElement === document.body) o.anchor.focus();
  o.onClose?.();
}
