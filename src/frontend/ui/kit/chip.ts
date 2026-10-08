/**
 * chip（C6）：状态栏 / 筛选胶囊，紧凑档。只显示「现在是什么」，有事时才上色；没内容就不画（调用方不建）。
 * 点开浮层的 chip 是触发器：`aria-expanded` 跟着浮层开关。
 */
import { icon, type IconName } from "./icon";
import s from "./chip.module.css";

export type ChipTone = "neutral" | "warn" | "error" | "success";

export interface ChipSpec {
  text: string;
  tone?: ChipTone;
  icon?: IconName;
  hint?: string;
  onClick?: () => void;
}

export function chip(spec: ChipSpec): HTMLElement {
  const c = document.createElement(spec.onClick ? "button" : "span");
  if (c instanceof HTMLButtonElement) c.type = "button";
  c.className = s.chip;
  c.dataset.intent = spec.tone ?? "neutral";
  if (spec.icon) c.appendChild(icon(spec.icon, "compact"));
  const t = document.createElement("span");
  t.textContent = spec.text;
  c.appendChild(t);
  if (spec.hint) c.title = spec.hint;
  if (spec.onClick) c.addEventListener("click", spec.onClick);
  return c;
}

/** 浮层开着 / 关着（触发器的样子跟着变，再点一次关由浮层那边管）。 */
export function setChipOpen(c: HTMLElement, open: boolean): void {
  c.setAttribute("aria-expanded", String(open));
}
