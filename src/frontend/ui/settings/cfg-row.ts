/**
 * 「别名与配置文件」那一栏里的一行：名字 · 一句现状（带一个点）· 一个主动作 · 展开箭头；点开就地做。
 * 收着时右边那颗主动作只把这一行展开到要做的地方；展开后它收掉，不出现两颗一样的（`机器配置.md` §2 方案 A）。
 */
import { icon } from "../kit/icon";

/** 一行的点：好 · 要看一眼 · 没开。 */
export type CfgDot = "ok" | "warn" | "off";

export interface CfgRow {
  element: HTMLElement;
  body: HTMLElement;
  setStatus(dot: CfgDot, text: string): void;
  /** 收着时右边那颗主动作（展开后收掉，不出现两颗一样的）；`null` = 没有。 */
  setAction(b: HTMLElement | null): void;
  setOpen(open: boolean): void;
  isOpen(): boolean;
  setName(name: string): void;
}

/** 一行：名字 · 一句现状（带一个点）· 一个主动作 · 展开箭头；点开就地做。 */
export function cfgRow(name: string, open: boolean, onToggle?: (open: boolean) => void): CfgRow {
  const row = document.createElement("div");
  row.className = "cfg-row";
  const head = document.createElement("div");
  head.className = "cfg-head";
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "cfg-toggle";
  const nm = document.createElement("span");
  nm.className = "cfg-name";
  nm.textContent = name;
  const st = document.createElement("span");
  st.className = "cfg-status";
  const dot = document.createElement("span");
  dot.className = "cfg-dot";
  const stText = document.createElement("span");
  st.append(dot, stText);
  toggle.append(nm, st);
  const action = document.createElement("span");
  action.className = "cfg-action";
  const chev = icon("caretRight", "compact");
  chev.classList.add("cfg-chev");
  // 箭头排在主动作右边，点它与点名字一样（读屏只认那颗按钮）。
  chev.addEventListener("click", () => toggle.click());
  head.append(toggle, action, chev);
  const body = document.createElement("div");
  body.className = "cfg-body";
  row.append(head, body);
  let actionEl: HTMLElement | null = null;
  const set = (o: boolean): void => {
    toggle.setAttribute("aria-expanded", String(o));
    body.hidden = !o;
    action.replaceChildren(...(!o && actionEl ? [actionEl] : []));
  };
  toggle.addEventListener("click", () => {
    const o = body.hidden;
    set(o);
    onToggle?.(o);
  });
  set(open);
  return {
    element: row,
    body,
    setStatus(d, text) {
      dot.dataset.dot = d;
      stText.textContent = text;
    },
    setAction(b) {
      actionEl = b;
      set(!body.hidden);
    },
    setOpen(o) {
      if (o !== !body.hidden) {
        set(o);
        onToggle?.(o);
      }
    },
    isOpen: () => !body.hidden,
    setName(n) {
      nm.textContent = n;
    },
  };
}
