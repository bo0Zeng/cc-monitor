/**
 * 「需手动」的账：本机后端 `sessions-needs-merge` 自己问到各台的 `sessions-needs`（别的台经它的连接问那台的常驻）、合成一份 ＋ 总数再答
 * （判 · 排 · 数 · 问哪台没问到全在后端；这里只记要问哪几台、只把合好的那一份交出去）。侧栏「需手动 N」· 浮层 · `Ctrl+J` · 窗口标题都读它。
 *
 * 什么时候问：起来 · 那台接上 · 开「需手动」浮层 · `Ctrl+J` · 那台推 `changed {needs}`（连着来的合成一问）·
 * 一问没问到之后自己退避再问（[`RETRY_FIRST_MS`] 起、翻倍、封顶 [`RETRY_MAX_MS`]；问到了归零）。在读的时候旧的一份照留（入口照上一份的数），读回来才换。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { NeedsMerged } from "./generated/NeedsMerged";
import type { NeedsRow } from "./generated/NeedsRow";
import type { ManualItem } from "./generated/ManualItem";

/** 本机后端问到各台（各台读内存与几份小文件 ＋ 回程）再合：等这么久。 */
const ASK_BUDGET_MS = 20_000;
/** 推送 · 会话帧连着来时合成一次重问。 */
const POKE_MS = 300;
/** 没问到之后头一回隔多久再问；之后每回翻倍。 */
const RETRY_FIRST_MS = 1_000;
/** 退避的封顶。 */
const RETRY_MAX_MS = 30_000;

/** 本机后端问到这几台、合一份。 */
export function mergeNeeds(machines: Origin[]): Promise<NeedsMerged> {
  return callTyped(LOCAL_ORIGIN, "sessions-needs-merge", { machines }, budgetWithin(ASK_BUDGET_MS));
}

/** 计划里要人看的一格：哪台 · 哪个工作区 · 哪一片 · 那一片里第几条。 */
export interface PlanNeedItem {
  origin: Origin;
  workspace: string;
  slice: string;
  k: number;
}

const EMPTY: NeedsMerged = { items: [], total: 0, unreadable: [], down: [] };

export class NeedsBook {
  /** 要问的那几台（问过的都记着，照先来的先后）。 */
  private readonly origins: Origin[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** 没问到之后排着的那一次再问。 */
  private retry: ReturnType<typeof setTimeout> | null = null;
  /** 此刻的退避（ms；0 ＝ 上一问问到了）。 */
  private backoff = 0;
  private readonly listeners = new Set<() => void>();
  private seq = 0;
  private merged: NeedsMerged = EMPTY;

  constructor(private readonly merge: (machines: Origin[]) => Promise<NeedsMerged> = mergeNeeds) {}

  /** 那一台记进要问的、再问一次（叠着问 ⇒ 只收最后一问）。 */
  refresh(origin: Origin): Promise<void> {
    return this.refreshAll([origin]);
  }

  /** 这几台都记进要问的、问一次。 */
  refreshAll(origins: readonly Origin[]): Promise<void> {
    for (const o of origins) if (!this.origins.includes(o)) this.origins.push(o);
    return this.ask();
  }

  /** 那台从机器表里删了：不再问它，剩下的再问一次。 */
  drop(origin: Origin): Promise<void> {
    const i = this.origins.indexOf(origin);
    if (i < 0) return Promise.resolve();
    this.origins.splice(i, 1);
    return this.ask();
  }

  /** 记着的每一台再问一次（开浮层 · `Ctrl+J` 那一下）。 */
  again(): Promise<void> {
    return this.ask();
  }

  /** 那台可能变了（推送 · 会话帧）：稍等一下合成一次重问。 */
  poke(origin: Origin): void {
    if (!this.origins.includes(origin)) this.origins.push(origin);
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
      next = this.origins.length === 0 ? EMPTY : await this.merge([...this.origins]);
    } catch (e) {
      console.warn("[needs] 本机后端没合成，照上一份：", e);
      if (my === this.seq) this.retryLater();
      return;
    }
    if (my !== this.seq) return;
    this.backoff = 0;
    this.merged = next;
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

  /** 这一问没问到的那几台（不计；各带后端那一句）。 */
  offline(): { origin: Origin; said: string }[] {
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
