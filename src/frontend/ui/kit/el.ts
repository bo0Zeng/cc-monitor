/**
 * 建一个元素：标签 ＋ 类名 ＋ 字。全产品只这一份（判据 `tests/frontend/ui/kit/single-home.vitest.ts`「按钮只经 kit/button · el() 只留 kit 那一份」）。
 * 按钮（主 · 次 · 危险 · 幽灵 · 图标那几层）走 `kit/button.ts`，不拿它自己拼。
 */
export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}
