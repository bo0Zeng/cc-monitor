/**
 * 下拉：外观同输入框 ＋ 右侧向下箭头，选项前可带一块身份（账号头像 · 机器标记）。面板就是弹出菜单（`menu.ts`）：
 * 宽同框、当前项左侧对勾、不可选的灰着并说为什么。
 *
 * - 键盘：焦点在框上 ↑↓ 直接换值不展开（跳过不可选）；Enter / 空格 / Alt+↓ 展开；面板里 ↑↓ 走、Enter 选、Esc 只收面板。
 *   `closedKeys: "open"`（选一项就写盘的那种，如会话的轮换来源）：合着时方向键一概不换值，↓ 只展开 —— 只有在面板里明确点一项才回调。
 * - 换值（点选或 ↑↓）⇒ `onChange(value)`；`setValue` 不回调。
 * 判据：`tests/frontend/ui/kit/select.vitest.ts`。
 */
import { icon } from "./icon";
import { closeMenu, menuAnchoredOn, openMenu, type MenuItem } from "./menu";
import s from "./select.module.css";

export interface SelectOption {
  value: string;
  label: string;
  /** 字前面那一块（每次画都调一次：同一个元素不能同时挂在框上和面板里）。 */
  lead?: () => HTMLElement;
  /** 紧跟在字后面的灰字（`默认 · 5h 63%`）。 */
  note?: string;
  /** `false` ＝ 灰着不可选；为什么写进 `why`（面板右侧灰字）。 */
  enabled?: boolean;
  why?: string;
}

export interface SelectSpec {
  /** 读屏名。 */
  label: string;
  options: SelectOption[];
  value?: string;
  onChange?: (value: string) => void;
  /** 合着时方向键做什么：`step`（缺省）↑↓ 直接换值 · `open` 只有 ↓ 展开、别的一概不做。 */
  closedKeys?: "step" | "open";
}

export interface SelectHandle {
  /** 框本身（一个按钮）。 */
  el: HTMLButtonElement;
  value(): string;
  setValue(value: string): void;
  setOptions(options: SelectOption[], value?: string): void;
  setDisabled(disabled: boolean): void;
}

export function select(spec: SelectSpec): SelectHandle {
  let options = spec.options;
  let value = spec.value ?? options.find((o) => o.enabled !== false)?.value ?? "";
  const el = document.createElement("button");
  el.type = "button";
  el.className = s.select;
  el.setAttribute("aria-haspopup", "listbox");
  el.setAttribute("aria-label", spec.label);
  const lead = document.createElement("span");
  lead.className = s.selectLead;
  const text = document.createElement("span");
  text.className = s.selectText;
  const note = document.createElement("span");
  note.className = s.selectNote;
  el.append(lead, text, note, icon("caretDown", "compact"));

  const current = (): SelectOption | undefined => options.find((o) => o.value === value);
  const paint = (): void => {
    const o = current();
    lead.replaceChildren(...(o?.lead ? [o.lead()] : []));
    text.textContent = o?.label ?? "";
    note.textContent = o?.note ?? "";
    el.dataset.value = value;
  };
  const pick = (v: string): void => {
    if (v === value) return;
    value = v;
    paint();
    spec.onChange?.(v);
  };
  const open = (): void => {
    const items: MenuItem[] = options.map((o) => ({
      id: o.value,
      label: o.label,
      note: o.note,
      avatar: o.lead?.(),
      checked: o.value === value,
      enabled: o.enabled !== false,
      detail: o.enabled === false ? o.why : undefined,
      onClick: () => pick(o.value),
    }));
    openMenu({ el }, items, { label: spec.label, width: el.getBoundingClientRect().width, onClose: () => el.focus() });
  };
  const step = (dir: 1 | -1): void => {
    const live = options.filter((o) => o.enabled !== false);
    if (live.length === 0) return;
    const i = live.findIndex((o) => o.value === value);
    const to = i < 0 ? (dir > 0 ? 0 : live.length - 1) : Math.min(live.length - 1, Math.max(0, i + dir));
    pick(live[to].value);
  };
  el.addEventListener("click", open);
  el.addEventListener("keydown", (ev) => {
    if (ev.isComposing || menuAnchoredOn(el)) return;
    if (spec.closedKeys === "open") {
      if (ev.key === "ArrowDown") {
        ev.preventDefault();
        open();
      }
      return;
    }
    if (ev.key === "ArrowDown" && ev.altKey) {
      ev.preventDefault();
      open();
    } else if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      step(ev.key === "ArrowDown" ? 1 : -1);
    }
  });
  paint();
  return {
    el,
    value: () => value,
    setValue: (v) => {
      value = v;
      paint();
    },
    setOptions: (next, v) => {
      if (menuAnchoredOn(el)) closeMenu();
      options = next;
      value = v ?? (next.some((o) => o.value === value) ? value : (next.find((o) => o.enabled !== false)?.value ?? ""));
      paint();
    },
    setDisabled: (d) => {
      el.disabled = d;
    },
  };
}
