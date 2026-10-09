/**
 * 空态：32px 线形图标 ＋ 一句 `无 X` ＋ 一句怎么让它有 ＋ 至多一颗次按钮。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「错误条 · 空态」那一节。
 */
import { icon, type IconName } from "./icon";
import s from "./empty.module.css";

export interface EmptySpec {
  text: string;
  hint?: string;
  icon?: IconName;
  action?: HTMLButtonElement;
}

export function emptyState(spec: EmptySpec): HTMLDivElement {
  const e = document.createElement("div");
  e.className = s.empty;
  e.appendChild(icon(spec.icon ?? "empty", "empty"));
  const t = document.createElement("div");
  t.className = s.emptyText;
  t.textContent = spec.text;
  e.appendChild(t);
  if (spec.hint) {
    const h = document.createElement("div");
    h.className = s.emptyHint;
    h.textContent = spec.hint;
    e.appendChild(h);
  }
  if (spec.action) e.appendChild(spec.action);
  return e;
}
