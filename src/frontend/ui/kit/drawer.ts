/**
 * 抽屉（C11）：从右侧滑出、占满高，凸起层底 ＋ 左外角圆 ＋ 模态投影；不加全窗遮罩（内容区压暗一层，仍看得见）。
 *
 * Esc 关（压进弹层栈）；有没存的输入时先问 `放弃填写`（`isDirty` 由调用方给）；关后焦点回触发处。
 */
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { button } from "./button";
import { confirmDialog } from "./dialog";
import { copyText } from "../copy-table";
import s from "./drawer.module.css";

export interface DrawerSpec {
  title: string;
  body: HTMLElement;
  /** 宽：360–480（默认 440，额度稿的账号面板）。 */
  width?: number;
  isDirty?: () => boolean;
  onClose?: () => void;
}

export interface DrawerHandle {
  root: HTMLElement;
  close(): Promise<boolean>;
}

export function openDrawer(spec: DrawerSpec): DrawerHandle {
  const before = document.activeElement;
  const dim = document.createElement("div");
  dim.className = s.drawerDim;
  const panel = document.createElement("aside");
  panel.className = s.drawer;
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", spec.title);
  panel.style.width = `${Math.max(360, Math.min(480, spec.width ?? 440))}px`;
  const head = document.createElement("div");
  head.className = s.drawerHead;
  const h = document.createElement("h2");
  h.className = s.drawerTitle;
  h.textContent = spec.title;
  const x = button({ label: copyText("kit.drawer.close"), kind: "icon", icon: "close", hint: copyText("kit.drawer.close"), onClick: () => void close() });
  head.append(h, x);
  const body = document.createElement("div");
  body.className = s.drawerBody;
  body.appendChild(spec.body);
  panel.append(head, body);
  let closed = false;
  const layer: OverlayHandle = {
    handleEsc: () => {
      void close();
      return true;
    },
  };
  const close = async (): Promise<boolean> => {
    if (closed) return true;
    if (spec.isDirty?.()) {
      const ok = await confirmDialog({ title: copyText("kit.drawer.discardTitle"), action: copyText("kit.drawer.discard") });
      if (!ok) return false;
    }
    closed = true;
    dispatcher.popOverlay(layer);
    dim.remove();
    panel.remove();
    if (before instanceof HTMLElement && before.isConnected) before.focus();
    spec.onClose?.();
    return true;
  };
  dispatcher.pushOverlay(layer);
  document.body.append(dim, panel);
  (panel.querySelector<HTMLElement>("input, textarea, select") ?? x).focus();
  return { root: panel, close };
}
