/**
 * 骨架占位：首次加载超过 300ms 的列表画 3–5 行与真行同高同间距的灰条，不转圈；数据到了原位替换。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「转圈 · 计量条 · 徽标 · 键帽 · 骨架 · 状态点」那一节。
 */
import s from "./skeleton.module.css";

export function skeletonRows(n = 4): HTMLDivElement {
  const root = document.createElement("div");
  root.className = s.skelSkeleton;
  root.setAttribute("aria-hidden", "true");
  for (let i = 0; i < Math.max(3, Math.min(5, n)); i++) {
    const row = document.createElement("div");
    row.className = s.skelRow;
    const a = document.createElement("i");
    a.className = s.skelBar;
    a.dataset.w = String(i % 3);
    row.appendChild(a);
    root.appendChild(row);
  }
  return root;
}
