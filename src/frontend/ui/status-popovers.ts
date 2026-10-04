/**
 * 状态栏上点开的那几块浮层（任务 · 子 agent）：同一时刻只开一块 —— 一块看得见了，别的收起。
 * 「开着」按展开算，不按此刻看不看得见：展开着、只因当前 tab 没内容而藏着的那块也收（切回来不会把刚开的那块顶掉）。
 */
export interface StatusPopover {
  collapse(): void;
}

const expanded = new Set<StatusPopover>();

/** `p` 展开 / 收起了（此刻看不看得见都算）。 */
export function popoverExpanded(p: StatusPopover, on: boolean): void {
  if (on) expanded.add(p);
  else expanded.delete(p);
}

/** `p` 刚变得看得见 ⇒ 别的展开着的收起。 */
export function popoverShown(p: StatusPopover): void {
  for (const other of [...expanded]) if (other !== p) other.collapse();
}
