/**
 * 按钮（C1）与切换按钮（C21）：全产品一个家。
 *
 * - 四层：主（一个视图最多一颗）· 次（默认）· 危险（只在确认框里）· 危险文字（列表 / 菜单里的危险入口）· 幽灵 · 图标。
 * - 态都放 `data-*` / `aria-*`：禁用用 `aria-disabled`（悬停提示还在，说为什么）；进行中字换成「正在…」＋ 转圈、宽度不变、不可再点。
 * - 纯图标按钮必须带读屏名（文案表 `aria` 档）与悬停提示。
 */
import { icon, type IconName } from "./icon";
import { spinner } from "./progress";
import s from "./button.module.css";

export type ButtonKind = "primary" | "secondary" | "danger" | "danger-text" | "ghost" | "icon";
export type ButtonSize = "regular" | "compact";

export interface ButtonSpec {
  /** 按钮字（动词或动宾）。图标按钮这里给读屏名。 */
  label: string;
  kind?: ButtonKind;
  size?: ButtonSize;
  icon?: IconName;
  /** 图标放在字后面（展开 / 收起那种「字 ＋ 折叠号」）。 */
  iconAfter?: boolean;
  /** 悬停提示（图标按钮必给；键位由调用方按当前键位拼）。 */
  hint?: string;
  onClick?: (ev: MouseEvent) => void;
}

/** 按钮 → 它的字那一格（进行中换字用；不靠按类名回查）。 */
const labels = new WeakMap<HTMLButtonElement, HTMLElement>();

export function button(spec: ButtonSpec): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = s.btnButton;
  const kind = spec.kind ?? "secondary";
  b.dataset.kind = kind;
  b.dataset.size = spec.size ?? "regular";
  const ic = spec.icon ? icon(spec.icon, spec.size === "compact" ? "compact" : "regular") : null;
  if (ic && !spec.iconAfter) b.appendChild(ic);
  const text = document.createElement("span");
  text.className = s.btnLabel;
  labels.set(b, text);
  if (kind === "icon") {
    b.setAttribute("aria-label", spec.label);
  } else {
    text.textContent = spec.label;
    b.appendChild(text);
  }
  if (ic && spec.iconAfter) b.appendChild(ic);
  if (spec.hint) b.title = spec.hint;
  const click = spec.onClick;
  b.addEventListener("click", (ev) => {
    if (b.getAttribute("aria-disabled") === "true" || b.dataset.busy === "true") {
      ev.preventDefault();
      ev.stopImmediatePropagation();
      return;
    }
    click?.(ev);
  });
  return b;
}

/** 禁用 / 解禁。`why` 进悬停提示（说为什么、怎么才能用）；`null` ⇒ 解禁并还原原提示。 */
export function setDisabled(b: HTMLButtonElement, why: string | null): void {
  if (why === null) {
    b.removeAttribute("aria-disabled");
    b.title = b.dataset.hint ?? b.title;
    delete b.dataset.hint;
    return;
  }
  if (b.getAttribute("aria-disabled") !== "true") b.dataset.hint = b.title;
  b.setAttribute("aria-disabled", "true");
  b.title = why;
}

/** 换按钮上的字（进行中时换的是还原后的那一份）。 */
export function setButtonLabel(b: HTMLButtonElement, text: string): void {
  if (b.dataset.busy === "true") {
    b.dataset.label = text;
    return;
  }
  const label = labels.get(b);
  if (label && label.textContent !== text) label.textContent = text;
}

/** 进行中：字换成 `busyLabel` ＋ 转圈，宽度钉在原宽；`null` ⇒ 还原。 */
export function setBusy(b: HTMLButtonElement, busyLabel: string | null): void {
  const label = labels.get(b);
  if (busyLabel === null) {
    if (b.dataset.busy !== "true") return;
    delete b.dataset.busy;
    b.querySelector(`[data-role="spin"]`)?.remove();
    if (label && b.dataset.label !== undefined) label.textContent = b.dataset.label;
    delete b.dataset.label;
    b.style.minWidth = "";
    return;
  }
  if (b.dataset.busy === "true") return;
  b.style.minWidth = `${b.offsetWidth}px`;
  b.dataset.busy = "true";
  if (label) {
    b.dataset.label = label.textContent ?? "";
    label.textContent = busyLabel;
  }
  const sp = spinner();
  sp.dataset.role = "spin";
  b.insertBefore(sp, b.firstChild);
}

/** 切换按钮（C21）：开着 = 叠层底 ＋ 图标强调色，`aria-pressed`；不与「选中」同形。 */
export function toggleButton(spec: ButtonSpec & { pressed: boolean; onToggle: (pressed: boolean) => void }): HTMLButtonElement {
  const b = button({
    ...spec,
    kind: spec.kind ?? "ghost",
    onClick: () => {
      const next = b.getAttribute("aria-pressed") !== "true";
      b.setAttribute("aria-pressed", String(next));
      spec.onToggle(next);
    },
  });
  b.setAttribute("aria-pressed", String(spec.pressed));
  return b;
}

/** 一行按钮：右对齐、取消在左、确认在右、宽度按字长。 */
export function buttonRow(...buttons: HTMLElement[]): HTMLDivElement {
  const row = document.createElement("div");
  row.className = s.btnRow;
  row.append(...buttons);
  return row;
}
