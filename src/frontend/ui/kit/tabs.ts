/**
 * 页内分栏与分段按钮：同一套键盘（←→ 换、Home / End 到两头），样子与用法分开。
 *
 * - 分栏：换整块内容；下划线式，当前 2px 强调色底边。
 * - 分段按钮：只换一个值、立刻生效；外框一圈，当前段叠层底 ＋ 底边强调色。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「分栏 · 分段按钮」那一节。
 */
import s from "./tabs.module.css";

export interface TabsSpec<K extends string> {
  items: { key: K; label: string }[];
  current: K;
  onChange: (key: K) => void;
  /** 分栏的读屏名 / 分段按钮的读屏名（文案表）。 */
  label: string;
}

function strip<K extends string>(kind: "tabs" | "segmented", spec: TabsSpec<K>): HTMLDivElement {
  const root = document.createElement("div");
  root.className = kind === "tabs" ? s.tabs : s.tabsSegmented;
  root.setAttribute("role", kind === "tabs" ? "tablist" : "radiogroup");
  root.setAttribute("aria-label", spec.label);
  const buttons = spec.items.map(({ key, label }) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = s.tabsItem;
    b.textContent = label;
    b.dataset.key = key;
    b.setAttribute("role", kind === "tabs" ? "tab" : "radio");
    b.addEventListener("click", () => pick(key, false));
    return b;
  });
  const mark = (key: K): void => {
    for (const b of buttons) {
      const on = b.dataset.key === key;
      b.setAttribute(kind === "tabs" ? "aria-selected" : "aria-checked", String(on));
      b.tabIndex = on ? 0 : -1;
    }
  };
  let cur = spec.current;
  const pick = (key: K, focus: boolean): void => {
    const changed = key !== cur;
    cur = key;
    mark(key);
    if (focus) buttons.find((b) => b.dataset.key === key)?.focus();
    if (changed) spec.onChange(key);
  };
  root.addEventListener("keydown", (ev) => {
    if (ev.isComposing) return;
    const i = spec.items.findIndex((it) => it.key === cur);
    const n = spec.items.length;
    const to =
      ev.key === "ArrowRight" ? (i + 1) % n : ev.key === "ArrowLeft" ? (i - 1 + n) % n : ev.key === "Home" ? 0 : ev.key === "End" ? n - 1 : -1;
    if (to < 0) return;
    ev.preventDefault();
    pick(spec.items[to].key, true);
  });
  root.append(...buttons);
  mark(cur);
  return root;
}

export function tabs<K extends string>(spec: TabsSpec<K>): HTMLDivElement {
  return strip("tabs", spec);
}

export function segmented<K extends string>(spec: TabsSpec<K>): HTMLDivElement {
  return strip("segmented", spec);
}
