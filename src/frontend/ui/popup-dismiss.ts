/**
 * 锚在一颗按钮上的小选单怎么关：点外面 · 再点那颗按钮（按钮自己的点击去收，不算「外面」）· Esc。
 * Esc 走快捷键的弹层栈（选单开着时压在最上面），一下 Esc 只关它，不连带清多选 / 关查找。
 */
import { dispatcher, type OverlayHandle } from "./keybindings/registry";

/** 选单挂上之后调；返回值在选单关掉时调（摘监听、出栈）。 */
export function armPopupDismiss(menu: HTMLElement, trigger: HTMLElement, close: () => void): () => void {
  const onPointer = (e: Event): void => {
    const t = e.target;
    if (t instanceof Node && (menu.contains(t) || trigger.contains(t))) return;
    close();
  };
  const layer: OverlayHandle = {
    handleEsc: () => {
      close();
      return true;
    },
  };
  document.addEventListener("pointerdown", onPointer, true);
  dispatcher.pushOverlay(layer);
  return () => {
    document.removeEventListener("pointerdown", onPointer, true);
    dispatcher.popOverlay(layer);
  };
}
