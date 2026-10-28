/**
 * **待发**：说出去、会话那台还没回执的那几句，整个主窗口一份（键 ＝ 那台 ＋ 会话；落本地存储，重开还在）。
 * 说一句 ＝ 先进待发（这一端铸编号）再交（`session-queue-put`）：回执到了 ⇒ 摘掉；那台拒了（不是离线）⇒ 摘掉、回那台那一句；
 * 没回执（断线 · 期限到了 · 那台离线）⇒ 留着，**那台接上那一下**（[machineUp]）照原编号把它名下各会话的待发都重交一次 ——
 * 不挂在打开哪条会话 · 哪一页上（同手机 A2-18-8），那台按编号去重、只收一条。在飞的那一句不重交。
 * 草稿与待发是这一端手上正在改的（R1），其余各站（排第 n · 未送达 · 已送）照那台的队列名单。
 */
import type { Rewind } from "./generated/Rewind";
import type { Origin } from "./ipc/origin";
import { CHANNEL_PUT, loadPending, outgoing, pendingKey, putOnce, savePending, type Outgoing, type PutEnd, type QueuePutFn } from "./session-queue";

export class Outbox {
  private readonly waiting: Map<string, Outgoing[]> = loadPending();
  /** 此刻在飞的编号（交出去、还没回来）。 */
  private readonly flying = new Set<string>();
  private readonly subs = new Set<() => void>();
  private readonly settledSubs = new Set<(origin: Origin, o: Outgoing, end: PutEnd) => void>();

  constructor(private readonly put: QueuePutFn = CHANNEL_PUT) {}

  /** 那条会话此刻的待发（先交的在前）。 */
  of(origin: Origin, sid: string): readonly Outgoing[] {
    return this.waiting.get(pendingKey(origin, sid)) ?? [];
  }

  /** 待发变了叫一声（进 · 摘 · 撤）；回的函数退订。 */
  onChange(fn: () => void): () => void {
    this.subs.add(fn);
    return () => this.subs.delete(fn);
  }

  /** 一句有了结局（回执到了 · 那台拒了；不论是头一次交还是接上重交的）叫一声；回的函数退订。 */
  onSettled(fn: (origin: Origin, o: Outgoing, end: PutEnd) => void): () => void {
    this.settledSubs.add(fn);
    return () => this.settledSubs.delete(fn);
  }

  /**
   * 说一句：进待发、交一次，回这一次的结局。待发里已有同一句（没回执又点了一次）⇒ 照它的编号再交，不另起一条；
   * 编辑已发的那一句带 `rewind`（同一句同一个回退才算同一句）。
   */
  async say(origin: Origin, sid: string, text: string, attachments: string[], rewind?: Rewind): Promise<PutEnd> {
    const key = pendingKey(origin, sid);
    const list = this.waiting.get(key) ?? [];
    const same = (x: Outgoing): boolean =>
      x.text === text && (rewind === undefined ? x.rewind === undefined : x.rewind?.to === rewind.to && x.rewind.how === rewind.how && x.rewind.when === rewind.when);
    let o = list.find(same);
    if (o === undefined) {
      o = outgoing(sid, text, attachments, rewind);
      this.waiting.set(key, [...list, o]);
      this.changed();
    }
    return this.deliver(origin, key, o);
  }

  /** 撤待发里那一句（本地拿掉；万一已交到那台，照编号请它撤是调用方的事）。 */
  drop(origin: Origin, sid: string, id: string): void {
    const key = pendingKey(origin, sid);
    const o = this.waiting.get(key)?.find((x) => x.id === id);
    if (o !== undefined) this.remove(key, o);
  }

  /** 那台接上了：它名下各会话的待发照原编号各交一次（在飞的不重交）。 */
  machineUp(origin: Origin): void {
    for (const [k, list] of this.waiting) if (k.startsWith(`${origin}\u0000`)) for (const o of list) if (!this.flying.has(o.id)) void this.deliver(origin, k, o);
  }

  private async deliver(origin: Origin, key: string, o: Outgoing): Promise<PutEnd> {
    this.flying.add(o.id);
    let end: PutEnd;
    try {
      end = await putOnce(this.put, origin, o);
    } finally {
      this.flying.delete(o.id);
    }
    if (end.at !== "none") {
      this.remove(key, o);
      for (const f of this.settledSubs) f(origin, o, end);
    }
    return end;
  }

  private remove(key: string, o: Outgoing): void {
    const left = (this.waiting.get(key) ?? []).filter((x) => x !== o);
    if (left.length === (this.waiting.get(key) ?? []).length) return;
    if (left.length > 0) this.waiting.set(key, left);
    else this.waiting.delete(key);
    this.changed();
  }

  private changed(): void {
    savePending(this.waiting);
    for (const f of this.subs) f();
  }
}

/** 主窗口那一份（输入框 · 编辑已发的一句 · 那台接上都用它）。 */
export const outbox = new Outbox();
