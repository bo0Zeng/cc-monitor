/**
 * 空态（C16）：32px 线形图标 ＋ 一句 `无 X` ＋ 一句怎么让它有 ＋ 至多一颗次按钮。
 * 「筛选没有结果」与「真的没有」是两句话：前者给［清除过滤］。
 */
import { button } from "./button";
import { icon, type IconName } from "./icon";
import { copyText } from "../copy-table";
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

/** 筛选没有结果：`无匹配「x」` ＋［清除过滤］。 */
export function noMatch(query: string, clear: () => void): HTMLDivElement {
  return emptyState({
    icon: "search",
    text: copyText("kit.empty.noMatch", { query }),
    action: button({ label: copyText("kit.empty.clearFilter"), onClick: clear }),
  });
}
