/**
 * 徽标 · 文字标记 · 键帽。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「转圈 · 计量条 · 徽标 · 键帽 · 骨架 · 状态点」那一节。
 *
 * 计数为 0 不画（返回 `null`）、超过 99 写 `99+`；需手动的计数琥珀底。键帽按当前键位渲染（调用方给已拼好的键位）。
 */
import s from "./badge.module.css";

export function countBadge(n: number, tone: "neutral" | "warn" = "neutral"): HTMLSpanElement | null {
  if (n <= 0) return null;
  const b = document.createElement("span");
  b.className = s.badgeCount;
  b.dataset.intent = tone;
  b.textContent = n > 99 ? "99+" : String(n);
  return b;
}

/** 文字标记（`远端` · `只读` · `后台`）：只描边不填底。 */
export function tag(text: string): HTMLSpanElement {
  const t = document.createElement("span");
  t.className = s.badgeTag;
  t.textContent = text;
  return t;
}

export function kbd(chord: string): HTMLElement {
  const k = document.createElement("kbd");
  k.className = s.badgeKbd;
  k.textContent = chord;
  return k;
}
