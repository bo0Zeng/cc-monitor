/**
 * 计划这一侧「需手动」的账：每台最后一次 `plan-list` 里每个工作区每一片的 `needCount`（后端判、后端数，不含 agent 提问那一种 ——
 * 那一条算在会话上，只算一次）· 每个会话接手的那一块（`bySession`，会话头那一枚标读它）。
 *
 * 计划页开着时它问的 `plan-list` 也交进来（同一份）；计划页关着时：起来问一次、那台推「计划变了」且数变了再问、会话结束再问一次。
 * 这里不判任何东西，只记、只数。
 */
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { fetchPlanList, type PlanList, type PlanSessionBlock } from "./plan-reads";
import type { PlanMoved } from "./quota-stream";

/** 一条计划项：哪台 · 哪个工作区 · 哪一片 · 那一片里第几条。 */
export interface PlanNeedItem {
  origin: Origin;
  workspace: string;
  slice: string;
  k: number;
}

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

  /** 全部计划项，次序：本机在前、别的机器照交进来的先后；工作区 · 片照 `plan-list` 的先后。 */
  items(): PlanNeedItem[] {
    const origins = [...this.lists.keys()].sort((a, b) => Number(isLocalOrigin(b)) - Number(isLocalOrigin(a)));
    const out: PlanNeedItem[] = [];
    for (const origin of origins)
      for (const ws of this.lists.get(origin)?.workspaces ?? [])
        for (const sl of ws.slices) for (let k = 0; k < sl.needCount; k++) out.push({ origin, workspace: ws.workspace, slice: sl.name, k });
    return out;
  }

  /** 那台最后一次答的 `plan-list`（计划页开页时先拿它画，问回来再换）。 */
  list(origin: Origin): PlanList | null {
    return this.lists.get(origin) ?? null;
  }

  total(): number {
    return this.items().length;
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
