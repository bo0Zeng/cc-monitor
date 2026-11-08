/**
 * 「需手动」的账：本机后端 `sessions-needs-merge` 自己问到各台的 `sessions-needs`（别的台经它的连接问那台的常驻）、合成一份 ＋ 总数再答
 * （判 · 排 · 数 · 问哪台没问到全在后端；问哪几台也是后端照它自己的机器表定；这里只把合好的那一份交出去）。侧栏「需手动 N」· 浮层 · `Ctrl+J` · 窗口标题都读它。
 *
 * 什么时候问：起来 · 那台接上 · 开「需手动」浮层 · `Ctrl+J` · 那台推 `changed {needs}`（连着来的合成一问）·
 * 一问没问到之后自己退避再问（[`RETRY_FIRST_MS`] 起、翻倍、封顶 [`RETRY_MAX_MS`]；问到了归零）。在读的时候旧的一份照留（入口照上一份的数），读回来才换。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL, type Machine } from "./ipc/origin";
import type { NeedsMerged } from "./generated/NeedsMerged";
import type { NeedsRow } from "./generated/NeedsRow";
import type { ManualItem } from "./generated/ManualItem";
import type { NeedsCell } from "./generated/NeedsCell";
import type { NeedsClock } from "./generated/NeedsClock";

/** 簿上一条在等的会话：哪台 · 那一行 · 那台写好的会走的那一句（没有 ⇒ `null`）· 收到这一份的时刻（本机钟）。 */
export interface BookSession {
  origin: Machine;
  session: NeedsRow;
  clock: NeedsClock | null;
  receivedAt: number;
}

/** 本机后端问到各台（各台读内存与几份小文件 ＋ 回程）再合：等这么久。 */
const ASK_BUDGET_MS = 20_000;
/** 推送 · 会话帧连着来时合成一次重问。 */
const POKE_MS = 300;
/** 没问到之后头一回隔多久再问；之后每回翻倍。 */
const RETRY_FIRST_MS = 1_000;
/** 退避的封顶。 */
const RETRY_MAX_MS = 30_000;

/** 本机后端照自己的机器表问到各台、合一份。 */
export function mergeNeeds(): Promise<NeedsMerged> {
  return callTyped(LOCAL, "sessions-needs-merge", {}, budgetWithin(ASK_BUDGET_MS));
}

/** 计划里要人看的一条：哪台 · 哪个工作区 · 哪一片 · 落在哪一格（后端给的编号与「编号 标题」；没有格可落 ⇒ `null`，进那一片的概览）。 */
export interface PlanNeedItem {
  origin: Machine;
  workspace: string;
  slice: string;
  cell: NeedsCell | null;
}

/** 计划页开了那一片之后选中哪一格：那一条的格在这一片上 ⇒ 它的编号；没有格 · 这一片上已没有那一格 ⇒ `null`（概览）。 */
export function planCellIn(slice: { cells: readonly { id: string }[] }, it: PlanNeedItem): string | null {
  const id = it.cell?.id ?? null;
  return id !== null && slice.cells.some((c) => c.id === id) ? id : null;
}

const EMPTY: NeedsMerged = { items: [], total: 0, unreadable: [], down: [], head: "", sure: true };

export class NeedsBook {
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** 没问到之后排着的那一次再问。 */
  private retry: ReturnType<typeof setTimeout> | null = null;
  /** 此刻的退避（ms；0 ＝ 上一问问到了）。 */
  private backoff = 0;
  private readonly listeners = new Set<() => void>();
  private seq = 0;
  private merged: NeedsMerged = EMPTY;
  /** 收到 `merged` 那一刻（本机钟）：会走的那一句从它起接着走。 */
  private at = 0;

  constructor(private readonly merge: () => Promise<NeedsMerged> = mergeNeeds) {}

  /** 再问一次（起来 · 机器表变了 · 开浮层 · `Ctrl+J` 那一下；叠着问 ⇒ 只收最后一问）。 */
  again(): Promise<void> {
    return this.ask();
  }

  /** 哪台可能变了（推送 · 会话帧）：稍等一下合成一次重问。 */
  poke(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    // 调度：合批 —— 推送 · 会话帧连着来时合成一次重问。
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.ask();
    }, POKE_MS);
  }

  private async ask(): Promise<void> {
    const my = ++this.seq;
    if (this.retry !== null) clearTimeout(this.retry);
    this.retry = null;
    let next: NeedsMerged;
    try {
      next = await this.merge();
    } catch (e) {
      console.warn("[needs] 本机后端没合成，照上一份：", e);
      if (my === this.seq) this.retryLater();
      return;
    }
    if (my !== this.seq) return;
    this.backoff = 0;
    this.merged = next;
    this.at = Date.now();
    this.emit();
  }

  /** 没问到：退避一段再问（翻倍、封顶）。 */
  private retryLater(): void {
    this.backoff = this.backoff === 0 ? RETRY_FIRST_MS : Math.min(this.backoff * 2, RETRY_MAX_MS);
    // 调度：一次性 —— 一问没问到，退避一段自己再问一次（问到了不再排）。
    this.retry = setTimeout(() => {
      this.retry = null;
      void this.ask();
    }, this.backoff);
  }

  /** 各台加起来几项（入口 · 窗口标题）。 */
  total(): number {
    return this.merged.total;
  }

  /** 在等的会话，先答的在前：各带那台写好的会走的那一句（`clock`，没有 ⇒ `null`）与收到这一份的时刻（本机钟）。 */
  sessions(): BookSession[] {
    const out: BookSession[] = [];
    for (const { origin, item } of this.merged.items) if (item.kind === "session") out.push({ origin, session: item.session, clock: item.clock ?? null, receivedAt: this.at });
    return out;
  }

  /** 计划里要人看的那几条，照合好的先后。 */
  plans(): PlanNeedItem[] {
    const out: PlanNeedItem[] = [];
    for (const { origin, item } of this.merged.items) if (item.kind === "plan") out.push({ origin, workspace: item.workspace, slice: item.slice, cell: item.cell ?? null });
    return out;
  }

  /** 会话外的每一项（带它是哪台的）。 */
  manual(): { origin: Machine; item: ManualItem }[] {
    const out: { origin: Machine; item: ManualItem }[] = [];
    for (const { origin, item } of this.merged.items) if (item.kind === "manual") out.push({ origin, item: item.manual });
    return out;
  }

  /** 会话外那一截读不出的那几台（各带核心写好的那一行与那台那一句）。 */
  unreadable(): { origin: Machine; line: string; said: string }[] {
    return this.merged.unreadable.map((u) => ({ origin: u.origin, line: u.line, said: u.said }));
  }

  /** 这一问没问到的那几台（不计；各带后端那一句）。 */
  offline(): { origin: Machine; said: string }[] {
    return this.merged.down.map((d) => ({ origin: d.origin, said: d.said }));
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit(): void {
    for (const f of this.listeners) f();
  }
}
