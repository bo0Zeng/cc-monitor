/**
 * 计划这一侧的账：每台最后一次 `plan-list`（计划页开页先拿它画）· 每个会话接手的那一块（`bySession`，会话头那一枚标读它）。
 * 「需手动」里的计划项不在这里数：各台 `sessions-needs` 那一份里有（`needs-book.ts`）。
 *
 * 计划页开着时它问的 `plan-list` 也交进来（同一份）；计划页关着时：起来问一次、那台推「计划变了」且数变了再问、会话结束再问一次。
 * 这里不判任何东西，只记、只数。
 */
import type { Origin } from "./ipc/origin";
import { fetchPlanList, type PlanList, type PlanSessionBlock } from "./plan-reads";
import type { PlanMoved } from "./changed-stream";

export class PlanNeedsBook {
  private readonly lists = new Map<Origin, PlanList>();
  private readonly listeners = new Set<() => void>();

  /** 那台的一份 `plan-list`（计划页问到的也交进来）。 */
  take(origin: Origin, list: PlanList): void {
    this.lists.set(origin, list);
    this.emit();
  }

  /** 问那台一次；问不到 ⇒ 那台的数作废（不留旧数冒充）。 */
  refresh(origin: Origin): Promise<void> {
    return fetchPlanList(origin, false).then(
      (list) => this.take(origin, list),
      () => {
        if (this.lists.delete(origin)) this.emit();
      },
    );
  }

  /** 那台推「这几个工作区的计划变了」：数跟记的不一样（或可能漏了）⇒ 重问那台。 */
  onChanged(origin: Origin, change: { moved: readonly PlanMoved[]; all: boolean }): void {
    const list = this.lists.get(origin);
    const differs = (m: PlanMoved): boolean => {
      const ws = list?.workspaces.find((w) => w.workspace === m.workspace);
      return !ws || m.needs === null || ws.needCount !== m.needs;
    };
    if (change.all || change.moved.some(differs)) void this.refresh(origin);
  }

  /** 那台最后一次答的 `plan-list`（计划页开页时先拿它画，问回来再换）。 */
  list(origin: Origin): PlanList | null {
    return this.lists.get(origin) ?? null;
  }

  /** 这个会话接手的那一块（会话头那一枚标）；不是哪一块的接手 ⇒ `null`。 */
  sessionBlock(origin: Origin, sid: string): (PlanSessionBlock & { workspace: string }) | null {
    for (const ws of this.lists.get(origin)?.workspaces ?? []) {
      const b = ws.bySession[sid];
      if (b) return { ...b, workspace: ws.workspace };
    }
    return null;
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit(): void {
    for (const f of this.listeners) f();
  }
}
