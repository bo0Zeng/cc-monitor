/**
 * 「需手动」的账：每台最后一次 `sessions-needs` 答的那一份整份（在等的会话 · 计划里要人看的格 · 会话外的），
 * 交本机后端 `sessions-needs-merge` 合成一份 ＋ 总数（判 · 排 · 数全在后端；这里只记、只把合好的那一份交出去）。
 * 侧栏「需手动 N」· 浮层 · `Ctrl+J` · 窗口标题都读它。
 *
 * 什么时候问一台：起来 · 那台接上 · 开「需手动」浮层 · `Ctrl+J` · 那台推 `changed {needs | plan | manual}` · 条上那台某个会话进出「需手动」。
 * 在读的时候旧的一份照留（入口照上一份的数），读回来才换。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, isOldBackend } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { NeedsList } from "./generated/NeedsList";
import type { NeedsMerged } from "./generated/NeedsMerged";
import type { NeedsRow } from "./generated/NeedsRow";
import type { ManualItem } from "./generated/ManualItem";

/** 问一台的整份 · 本机合一份：都是读内存与几份小文件 ＋ 回程。 */
const ASK_BUDGET_MS = 20_000;
/** 推送 · 会话帧连着来时合成一次重问。 */
const POKE_MS = 300;

/** 一台此刻：读到了 · 那台断开（不计、浮层底下说一句）· 问不到 / 不认这条（不计、不画）。 */
export type NeedsRead = { t: "ok"; list: NeedsList } | { t: "offline" } | { t: "none" };

/** 问那台它的需手动整份。 */
export async function readNeeds(origin: Origin): Promise<NeedsRead> {
  try {
    return { t: "ok", list: await callTyped(origin, "sessions-needs", {}, budgetWithin(ASK_BUDGET_MS)) };
  } catch (e) {
    if (!isOldBackend(e) && e instanceof ChanError && e.error.layer === "hop") return { t: "offline" };
    console.warn(`[needs] ${origin} 的需手动问不到：`, e);
    return { t: "none" };
  }
}

/** 本机后端合一份（各台原样交）。 */
export function mergeNeeds(machines: { origin: Origin; list: NeedsList }[]): Promise<NeedsMerged> {
  return callTyped(LOCAL_ORIGIN, "sessions-needs-merge", { machines }, budgetWithin(ASK_BUDGET_MS));
}

/** 计划里要人看的一格：哪台 · 哪个工作区 · 哪一片 · 那一片里第几条。 */
export interface PlanNeedItem {
  origin: Origin;
  workspace: string;
  slice: string;
  k: number;
}

const EMPTY: NeedsMerged = { items: [], total: 0, unreadable: [] };

export class NeedsBook {
  private readonly by = new Map<Origin, Exclude<NeedsRead, { t: "none" }>>();
  private readonly seq = new Map<Origin, number>();
  private readonly timers = new Map<Origin, ReturnType<typeof setTimeout>>();
  private readonly listeners = new Set<() => void>();
  private mergeSeq = 0;
  private merged: NeedsMerged = EMPTY;

  constructor(
    private readonly read: (origin: Origin) => Promise<NeedsRead> = readNeeds,
    private readonly merge: (machines: { origin: Origin; list: NeedsList }[]) => Promise<NeedsMerged> = mergeNeeds,
  ) {}

  /** 问那一台一次，再合一份（同一台叠着问 ⇒ 只收最后一问）。 */
  async refresh(origin: Origin): Promise<void> {
    const my = (this.seq.get(origin) ?? 0) + 1;
    this.seq.set(origin, my);
    const got = await this.read(origin);
    if (this.seq.get(origin) !== my) return;
    if (got.t === "none") this.by.delete(origin);
    else this.by.set(origin, got);
    await this.remerge();
  }

  /** 每一台都问一次。 */
  refreshAll(origins: readonly Origin[]): Promise<void> {
    return Promise.all(origins.map((o) => this.refresh(o))).then(() => undefined);
  }

  /** 问过的每一台再问一次（开浮层 · `Ctrl+J` 那一下）。 */
  again(): Promise<void> {
    return this.refreshAll([...this.seq.keys()]);
  }

  /** 那台可能变了（推送 · 会话帧）：稍等一下合成一次重问。 */
  poke(origin: Origin): void {
    clearTimeout(this.timers.get(origin));
    // 调度：合批 —— 推送 · 会话帧连着来时合成那台一次重问。
    const t = setTimeout(() => {
      this.timers.delete(origin);
      void this.refresh(origin);
    }, POKE_MS);
    this.timers.set(origin, t);
  }

  private async remerge(): Promise<void> {
    const my = ++this.mergeSeq;
    const machines: { origin: Origin; list: NeedsList }[] = [];
    for (const [origin, st] of this.by) if (st.t === "ok") machines.push({ origin, list: st.list });
    let next: NeedsMerged;
    try {
      next = machines.length === 0 ? EMPTY : await this.merge(machines);
    } catch (e) {
      console.warn("[needs] 本机合不成，照上一份：", e);
      return;
    }
    if (my !== this.mergeSeq) return;
    this.merged = next;
    this.emit();
  }

  /** 各台加起来几项（入口 · 窗口标题）。 */
  total(): number {
    return this.merged.total;
  }

  /** 在等的会话，先答的在前。 */
  sessions(): { origin: Origin; session: NeedsRow }[] {
    const out: { origin: Origin; session: NeedsRow }[] = [];
    for (const { origin, item } of this.merged.items) if (item.kind === "session") out.push({ origin, session: item.session });
    return out;
  }

  /** 计划里要人看的那几格，照合好的先后。 */
  plans(): PlanNeedItem[] {
    const out: PlanNeedItem[] = [];
    for (const { origin, item } of this.merged.items) if (item.kind === "plan") out.push({ origin, workspace: item.workspace, slice: item.slice, k: item.k });
    return out;
  }

  /** 会话外的每一项（带它是哪台的）。 */
  manual(): { origin: Origin; item: ManualItem }[] {
    const out: { origin: Origin; item: ManualItem }[] = [];
    for (const { origin, item } of this.merged.items) if (item.kind === "manual") out.push({ origin, item: item.manual });
    return out;
  }

  /** 会话外那一截读不出的那几台（各带那台那一句）。 */
  unreadable(): { origin: Origin; said: string }[] {
    return this.merged.unreadable.map((u) => ({ origin: u.origin, said: u.said }));
  }

  /** 此刻断开的那几台（不计）。 */
  offline(): Origin[] {
    return [...this.by.entries()].filter(([, st]) => st.t === "offline").map(([o]) => o);
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit(): void {
    for (const f of this.listeners) f();
  }
}
