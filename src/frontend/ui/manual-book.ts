/**
 * 会话外「需手动」的账：每台最后一次 `manual-list` 读到的那一份（后端判、后端写句子；这里只记、只数）。
 *
 * 什么时候问：起来每台问一次 · 开「需手动」浮层时问一次 · 那台推 `changed {manual}` 再问那一台。
 * 在读的时候旧的一份照留（入口照上一份的数、浮层里旧行不动），读回来才换。
 */
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { readManual, type ManualItem, type ManualRead } from "./login-reads";

/** 一台此刻的样子（还没读回来过 ⇒ 不在账上）。 */
export type ManualState = Exclude<ManualRead, { t: "old" }>;

export class ManualBook {
  private readonly by = new Map<Origin, ManualState>();
  private readonly seq = new Map<Origin, number>();
  private readonly listeners = new Set<() => void>();
  /** 问过哪几台（开浮层那一下全都再问一次）。 */
  private readonly known = new Set<Origin>();

  constructor(private readonly read: (origin: Origin) => Promise<ManualRead> = readManual) {}

  /** 问那一台一次（问回来之前旧的一份照留；同一台叠着问 ⇒ 只收最后一问）。 */
  async refresh(origin: Origin): Promise<void> {
    this.known.add(origin);
    const my = (this.seq.get(origin) ?? 0) + 1;
    this.seq.set(origin, my);
    const got = await this.read(origin);
    if (this.seq.get(origin) !== my) return;
    if (got.t === "old") {
      if (!this.by.delete(origin)) return;
    } else {
      this.by.set(origin, got);
    }
    this.emit();
  }

  /** 每一台都问一次。 */
  refreshAll(origins: readonly Origin[]): Promise<void> {
    return Promise.all(origins.map((o) => this.refresh(o))).then(() => undefined);
  }

  /** 问过的每一台再问一次（开「需手动」浮层那一下）。 */
  again(): Promise<void> {
    return this.refreshAll([...this.known]);
  }

  /** 次序：本机在前、别的照交进来的先后。 */
  states(): [Origin, ManualState][] {
    return [...this.by.entries()].sort(([a], [b]) => Number(isLocalOrigin(b)) - Number(isLocalOrigin(a)));
  }

  /** 读到了的每一项（带它是哪台问来的）。 */
  items(): { origin: Origin; item: ManualItem }[] {
    const out: { origin: Origin; item: ManualItem }[] = [];
    for (const [origin, st] of this.states()) if (st.t === "ok") for (const item of st.items) out.push({ origin, item });
    return out;
  }

  /** 进「需手动 N」的那几条（断开 · 无法读取的那台不计）。 */
  count(): number {
    return this.items().length;
  }

  /** 有没有哪台回了话、清单却无法读取（入口去数字、虚线框）。 */
  anyUnreadable(): boolean {
    for (const st of this.by.values()) if (st.t === "unreadable") return true;
    return false;
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit(): void {
    for (const f of this.listeners) f();
  }
}
