/**
 * 抽屉：从右侧滑出、占满高，凸起层底 ＋ 左外角圆 ＋ 模态投影；不加全窗遮罩（内容区压暗一层，仍看得见）。
 *
 * Esc 关（压进弹层栈）；有没存的输入时先问 `放弃填写`（`isDirty` 由调用方给）；关后焦点回触发处。
 * 压暗那一层不接指针：左边内容照样能滚、能读（改的都是立刻生效的设置，不挡人）。
 */
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { button } from "./button";
import { confirmDialog } from "./dialog";
import { setDrawerRight } from "./toast";
import { copyText } from "../copy-table";
import s from "./drawer.module.css";

export interface DrawerSpec {
  title: string;
  /** 标题右边一段灰字（对象 · 机器：`orders · 本机`）。 */
  sub?: string;
  body: HTMLElement;
  /** 宽：360–480（默认 440，额度稿的账号面板）。 */
  width?: number;
  /** 离窗口底边留多少 px 不盖（主窗口的状态栏：再点状态栏那颗按钮就能关）。 */
  bottom?: number;
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
  const width = Math.max(360, Math.min(480, spec.width ?? 440));
  panel.style.width = `${width}px`;
  // 开着时 toast 让到抽屉左边（不压抽屉的底栏）；关了还原。
  setDrawerRight(width);
  if (spec.bottom !== undefined) {
    panel.style.bottom = `${spec.bottom}px`;
    dim.style.bottom = `${spec.bottom}px`;
  }
  const head = document.createElement("div");
  head.className = s.drawerHead;
  const h = document.createElement("h2");
  h.className = s.drawerTitle;
  h.textContent = spec.title;
  if (spec.sub !== undefined) {
    const sub = document.createElement("span");
    sub.className = s.drawerSub;
    sub.textContent = spec.sub;
    h.appendChild(sub);
  }
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
    setDrawerRight(null);
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
