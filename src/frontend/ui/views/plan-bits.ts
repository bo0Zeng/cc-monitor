/**
 * 计划页与一格详情共用的几件小排版：状态图标 · 类色 · 阶段徽标。
 */
import type { IconName } from "../kit/icon";
import type { StatusLook } from "./plan-model";
import s from "./plan-bits.module.css";

export const STATUS_ICON: Record<StatusLook, IconName> = { done: "success", open: "ring", dropped: "prohibit" };

/** 类色：第 i 档（`data-kc` ⇒ `plan-bits.module.css` 把 `--kc` 指到令牌 `--plan-kind-i`）；`null` ⇒ 不设（画淡色）。 */
export function setKindColor(el: HTMLElement, slot: number | null): void {
  if (slot !== null) el.dataset.kc = String(slot);
}

/** 阶段 / 领域那一枚小框。 */
export function phaseBadge(t: string): HTMLElement {
  const e = document.createElement("span");
  e.className = s.pbPhase;
  e.textContent = t;
  return e;
}
