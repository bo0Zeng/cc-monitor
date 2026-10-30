/**
 * 计划页与一格详情共用的几件小排版：状态图标 · 类色 · 阶段徽标 · 无图那一屏。
 */
import { button } from "../kit/button";
import { emptyState } from "../kit/empty";
import type { IconName } from "../kit/icon";
import { copyText } from "../copy-table";
import type { PlanSlice } from "../plan-reads";
import type { PlanLook } from "../generated/PlanLook";
import s from "./plan-bits.module.css";

export const STATUS_ICON: Record<PlanLook, IconName> = { done: "success", open: "ring", undecided: "question", none: "note" };

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

/**
 * 新登记的仓库还没有图那一屏：「名字 · 领域 · 无图」＋ 一行怎么让它有 ＋［起新会话…］（全产品那一个框，工作目录预填工作区根）
 * ＋ project 的交接说明那一块（`handoff`：计划页照 pb 给的那一句拼好交进来；新会话开口说它，认领码找人要）。
 * `newSession` 收要预填的目录。
 */
export function freshPane(slice: Pick<PlanSlice, "name" | "domain">, workspace: string, newSession: (cwd: string) => void, handoff?: HTMLElement): HTMLElement {
  const go = button({ label: copyText("plan.fresh.newSession"), icon: "plus", onClick: () => newSession(workspace) });
  const box = emptyState({
    icon: "plan",
    text: slice.domain ? copyText("plan.fresh.title", { name: slice.name, domain: slice.domain }) : copyText("plan.fresh.titleBare", { name: slice.name }),
    hint: copyText("plan.fresh.hint"),
    action: go,
  });
  box.dataset.view = "fresh";
  if (handoff) box.appendChild(handoff);
  return box;
}
