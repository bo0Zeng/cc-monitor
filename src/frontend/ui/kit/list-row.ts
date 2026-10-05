/**
 * 列表行（C8）：28 高；悬停叠层、选中强调色淡底、当前（打开着的）左 2px 强调色条。
 * 行尾动作常态隐藏、悬停或焦点在行上时出现，占位不变（`visibility`）。长名字中间省略、悬停给全名。
 */
import { icon, type IconName } from "./icon";
import { copyText } from "../copy-table";
import s from "./list-row.module.css";

export interface ListRowSpec {
  name: string;
  icon?: IconName;
  meta?: string;
  actions?: HTMLElement[];
  onOpen?: () => void;
}

export function listRow(spec: ListRowSpec): HTMLDivElement {
  const r = document.createElement("div");
  r.className = s.listRowRow;
  r.tabIndex = 0;
  r.setAttribute("role", "option");
  if (spec.icon) r.appendChild(icon(spec.icon));
  const n = document.createElement("span");
  n.className = s.listRowName;
  n.textContent = middleEllipsis(spec.name, 60);
  n.title = spec.name;
  r.appendChild(n);
  if (spec.meta !== undefined) {
    const m = document.createElement("span");
    m.className = s.listRowMeta;
    m.textContent = spec.meta;
    r.appendChild(m);
  }
  if (spec.actions?.length) {
    const a = document.createElement("span");
    a.className = s.listRowActions;
    a.append(...spec.actions);
    r.appendChild(a);
  }
  const open = spec.onOpen;
  if (open) {
    r.addEventListener("dblclick", open);
    r.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) open();
    });
  }
  return r;
}

/** 选中 · 当前（两件事分开）。 */
export function setRowState(r: HTMLElement, st: { selected?: boolean; current?: boolean }): void {
  if (st.selected !== undefined) r.setAttribute("aria-selected", String(st.selected));
  if (st.current !== undefined) {
    if (st.current) r.setAttribute("aria-current", "true");
    else r.removeAttribute("aria-current");
  }
}

/** 中间省略：留开头与尾部（扩展名与末几个字），超过 `max` 才省。 */
export function middleEllipsis(name: string, max: number): string {
  const chars = [...name];
  if (chars.length <= max) return name;
  const tail = Math.min(Math.ceil((max - 1) / 3), 16);
  return chars.slice(0, max - 1 - tail).join("") + copyText("kit.text.ellipsis") + chars.slice(chars.length - tail).join("");
}
