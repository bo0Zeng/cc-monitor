/**
 * **待发**：说出去、会话那台还没回执的那几句，整个主窗口一份（键 ＝ 那台 ＋ 会话；落本地存储，重开还在）。
 * 说一句 ＝ 先进待发（这一端铸编号）再交（`session-queue-put`）：回执到了，或那台名单的 `sent` 里有它（[wentIn]）⇒ 摘掉；
 * 那台拒了（不是离线）⇒ 留着、记上那台那一句（[Pending.refused]），
 * 接上不重交，撤了或同一句再发（照原编号重交）才离开（同手机 `ChatOutbox`；编辑已发的那一句也一样）；
 * 没回执（断线 · 期限到了 · 那台离线）⇒ 留着，**那台接上那一下**（[machineUp]）照原编号把它名下各会话的待发都重交一次 ——
 * 不挂在打开哪条会话 · 哪一页上（同手机 A2-18-8），那台按编号去重、只收一条。在飞的那一句不重交。
 * 草稿与待发是这一端手上正在改的（R1），其余各态（排着 · 等需手动 · 未送达 · 已送）照那台的队列名单。
 * 撤待发里一句（[withdraw]，同手机 `ChatOutbox.cancel`）也在这里问那台：撤着随待发落盘（[Pending.withdrawing]），窗口重开 · 那台断过 ⇒
 * 那台接上那一下照原编号接着问撤没撤成（那台已记撤过 ⇒ 答撤成了），不重交；结局交 [onWithdrawn] 订的那几处（输入框回字 · 框下那一句）；编辑框照 [editHeld] 跟着待发变了解开。
 */
import type { QueueList } from "./generated/QueueList";
import type { Rewind } from "./generated/Rewind";
import type { Machine } from "./ipc/origin";
import { detailOf, saidOf } from "./kit/detail";
import {
  CHANNEL_CANCEL,
  CHANNEL_PUT,
  loadPending,
  outgoing,
  pendingKey,
  putOnce,
  savePending,
  type Outgoing,
  type PutEnd,
  type QueueCancelFn,
  type QueuePutFn,
} from "./session-queue";

/**
 * 待发里的一句：[refused] ＝ 那台拒了，记那台写好的那一句；[withdrawing] ＝ 撤着（点了撤、那台还没答成：接上不重交、照原编号接着问）。
 * 两格都随待发落本地存储；没有 ⇒ 不在。
 */
export type Pending = Outgoing & { refused?: string; withdrawing?: true };

/** 一句的结局：交那一问的（[PutEnd]）· 那台说它已送进会话（`sent`：名单的 `sent` 里有它 · 撤的那一问说没撤成）。 */
export type Settle = Exclude<PutEnd, { at: "cancelled" }> | { at: "sent" };

/**
 * 撤一句的结局（[Outbox.withdraw]）：撤成了（那台答撤成了 · 拒过的不问那台、`queue` 为 `null`）⇒ `back`，字回框；
 * 没撤成（在送 · 送过）⇒ `in`，当送进去了，`said` 是撤回包里那台写好的那一句；那一问没答复 ⇒ `lost`，`said` 是那一问回来的那一句（首问 ⇒ 本地拿掉；接着问的 ⇒ 仍撤着）。
 */
export type Withdrawn = { at: "back"; queue: QueueList | null } | { at: "in"; said: string; queue: QueueList } | { at: "lost"; said: string; detail: string };

export class Outbox {
  private readonly waiting: Map<string, Pending[]> = loadPending();
  /** 此刻在飞的编号（交出去、还没回来）。 */
  private readonly flying = new Set<string>();
  /** 撤那一问此刻在路上的编号：再撤不另问。 */
  private readonly asking = new Set<string>();
  /** 在飞时已有了结局的编号（名单说已送 · 撤成了）：那一问回来不再动待发、不再叫结局。 */
  private readonly ended = new Map<string, "sent" | "cancelled">();
  private readonly subs = new Set<() => void>();
  private readonly settledSubs = new Set<(origin: Machine, o: Outgoing, end: Settle) => void>();
  private readonly withdrawnSubs = new Set<(origin: Machine, o: Pending, w: Withdrawn) => void>();

  constructor(
    private readonly put: QueuePutFn = CHANNEL_PUT,
    private readonly cancel: QueueCancelFn = CHANNEL_CANCEL,
  ) {}

  /** 那条会话此刻的待发（先交的在前）。 */
  of(origin: Machine, sid: string): readonly Pending[] {
    return this.waiting.get(pendingKey(origin, sid)) ?? [];
  }

  /** 待发变了叫一声（进 · 摘 · 撤 · 一句起飞 · 落地）；回的函数退订。 */
  onChange(fn: () => void): () => void {
    this.subs.add(fn);
    return () => this.subs.delete(fn);
  }

  /** 一句有了结局（回执到了 · 那台拒了 · 那台说已送进会话；不论是头一次交还是接上重交的）叫一声，一句只叫一次；回的函数退订。 */
  onSettled(fn: (origin: Machine, o: Outgoing, end: Settle) => void): () => void {
    this.settledSubs.add(fn);
    return () => this.settledSubs.delete(fn);
  }

  /**
   * 说一句：进待发、交一次，回这一次的结局。待发里已有同一句（没回执 · 被拒了又点了一次）⇒ 照它的编号再交，不另起一条；
   * 编辑已发的那一句带 `rewind`（同一句同一个回退才算同一句）。
   */
  async say(origin: Machine, sid: string, text: string, attachments: string[], rewind?: Rewind): Promise<PutEnd> {
    const key = pendingKey(origin, sid);
    const list = this.waiting.get(key) ?? [];
    let o: Pending | undefined = this.sameAs(origin, sid, text, rewind);
    if (o === undefined) {
      o = outgoing(sid, text, attachments, rewind);
      this.waiting.set(key, [...list, o]);
      this.changed();
    } else if (o.refused !== undefined) {
      delete o.refused;
      this.changed();
    }
    return this.deliver(origin, key, o);
  }

  /** 待发里跟这一句算同一句的那一条（同字；编辑已发的那一句还要同一个回退；撤着的不算）；[say] 照它认，不另起一条。 */
  sameAs(origin: Machine, sid: string, text: string, rewind?: Rewind): Pending | undefined {
    return this.of(origin, sid).find(
      (x) =>
        x.withdrawing !== true &&
        x.text === text &&
        (rewind === undefined ? x.rewind === undefined : x.rewind?.to === rewind.to && x.rewind.how === rewind.how && x.rewind.when === rewind.when),
    );
  }

  /** 撤一句有了结局叫一声（点了撤的 · 接上接着问的都到这里）；回的函数退订。 */
  onWithdrawn(fn: (origin: Machine, o: Pending, w: Withdrawn) => void): () => void {
    this.withdrawnSubs.add(fn);
    return () => this.withdrawnSubs.delete(fn);
  }

  /**
   * 撤待发里那一句。那台拒过的（没进它的队）⇒ 本地拿掉、当场撤成了，不问那台。别的先记撤着（落盘：重开 · 断过也不重交），照编号请那台撤，答了才定：
   * 撤成了 ⇒ 拿掉；没撤成 ⇒ 当送进去了（[wentIn]）；没答复 ⇒ 首问本地拿掉（送没送进去说不清，在飞的那一问回来照常叫结局），
   * 接着问的（上一回撤着没等到答复）仍撤着、下一回接上再问。不在待发里 · 那一问正在路上 ⇒ `null`，不另问。结局也交 [onWithdrawn] 订的那几处。
   */
  async withdraw(origin: Machine, sid: string, id: string): Promise<Withdrawn | null> {
    const key = pendingKey(origin, sid);
    const o = this.waiting.get(key)?.find((x) => x.id === id);
    if (o === undefined || this.asking.has(id)) return null;
    if (o.refused !== undefined) {
      this.leave(key, id);
      return this.withdrawn(origin, o, { at: "back", queue: null });
    }
    const resumed = o.withdrawing === true;
    o.withdrawing = true;
    this.asking.add(id);
    this.changed();
    let w: Withdrawn;
    try {
      const r = await this.cancel(origin, sid, id);
      this.asking.delete(id);
      if (r.cancelled) {
        // 在飞的那一问回来不再叫结局（字已回框，回执晚到不清它）。
        if (this.leave(key, id) && this.flying.has(id)) this.ended.set(id, "cancelled");
        w = { at: "back", queue: r.queue };
      } else {
        this.wentIn(origin, sid, [id]);
        w = { at: "in", said: r.said, queue: r.queue };
      }
    } catch (e) {
      this.asking.delete(id);
      if (resumed) this.ring();
      else this.leave(key, id);
      w = { at: "lost", said: saidOf(e), detail: detailOf(e) };
    }
    return this.withdrawn(origin, o, w);
  }

  /** 撤着、此刻没在问的那一句（上一回撤着没等到答复）：那一行照那台的 `withdrawHeld`，接上照原编号接着问。 */
  parked(id: string): boolean {
    return !this.asking.has(id) && [...this.waiting.values()].some((l) => l.some((x) => x.id === id && x.withdrawing === true));
  }

  /** 编辑 [to] 那一句的上一版还撤着（那台可能已退过、送过）：编辑框的［回退并发送］灰着。那台拒过的没进它的队、退不了，不算。 */
  editHeld(origin: Machine, sid: string, to: string): boolean {
    return this.of(origin, sid).some((x) => x.withdrawing === true && x.refused === undefined && x.rewind?.to === to);
  }

  private withdrawn(origin: Machine, o: Pending, w: Withdrawn): Withdrawn {
    for (const f of this.withdrawnSubs) f(origin, o, w);
    return w;
  }

  /**
   * 那台说这几个编号已送进那条会话（名单的 `sent` · 撤的那一问说没撤成）：待发里的那几句当回执到了 —— 离开待发、本地存储删掉、叫一声结局（`sent`）；
   * 在飞的那一问回来不再挂回、不再叫，`say` 回送进去了。不在待发里的编号不管。
   */
  wentIn(origin: Machine, sid: string, ids: readonly string[]): void {
    const key = pendingKey(origin, sid);
    const gone = (this.waiting.get(key) ?? []).filter((o) => ids.includes(o.id));
    for (const o of gone) {
      this.leave(key, o.id);
      if (this.flying.has(o.id)) this.ended.set(o.id, "sent");
      for (const f of this.settledSubs) f(origin, o, { at: "sent" });
    }
  }

  /** 那台接上了：它名下各会话的待发照原编号各交一次（在飞的 · 那台拒了的不重交）；撤着、此刻没在问的照原编号接着请它撤（不重交）。 */
  machineUp(origin: Machine): void {
    for (const [k, list] of this.waiting)
      if (k.startsWith(`${origin}\u0000`))
        for (const o of list) {
          if (this.flying.has(o.id) || this.asking.has(o.id) || o.refused !== undefined) continue;
          if (o.withdrawing === true) void this.withdraw(origin, o.sid, o.id);
          else void this.deliver(origin, k, o);
        }
  }

  /** 那一句此刻在飞（交出去、还没回来）：那一行说「发送中 · 等确认」；不在飞 ⇒ 拒了的照 `refused`，交了没回执的照那台的 `heldSaid`。 */
  inFlight(id: string): boolean {
    return this.flying.has(id);
  }

  private async deliver(origin: Machine, key: string, o: Pending): Promise<PutEnd> {
    this.flying.add(o.id);
    this.ring();
    let end: Awaited<ReturnType<typeof putOnce>>;
    try {
      end = await putOnce(this.put, origin, o);
    } finally {
      this.flying.delete(o.id);
      this.ring();
    }
    // 那一问在飞时已有了结局 ⇒ 这一问回来什么都不动：名单说已送 ⇒ 回送进去了；撤成了 ⇒ 回撤了（编辑已发的那一句，编辑框照它留着字、不挂待发）。
    const was = this.ended.get(o.id);
    this.ended.delete(o.id);
    // 回执没到 ⇒ 那台没写回执那一句，这一端不替它写（回执那一句哪儿都不画）。
    if (was === "sent") return end.at === "receipt" ? end : { at: "receipt", said: "" };
    if (was === "cancelled") return { at: "cancelled" };
    if (end.at === "receipt") this.leave(key, o.id);
    else if (end.at === "refused" && this.waiting.get(key)?.includes(o) === true) {
      o.refused = end.said;
      this.changed();
    }
    if (end.at !== "none") for (const f of this.settledSubs) f(origin, o, end);
    return end;
  }

  /** 那一句离开待发（撤着随它一起没了）；原来就不在 ⇒ `false`。 */
  private leave(key: string, id: string): boolean {
    const list = this.waiting.get(key) ?? [];
    const left = list.filter((x) => x.id !== id);
    if (left.length === list.length) return false;
    if (left.length > 0) this.waiting.set(key, left);
    else this.waiting.delete(key);
    this.changed();
    return true;
  }

  private changed(): void {
    savePending(this.waiting);
    this.ring();
  }

  /** 叫订的那几处（待发变了 · 一句起飞 · 落地）。 */
  private ring(): void {
    for (const f of this.subs) f();
  }
}

/** 主窗口那一份（输入框 · 编辑已发的一句 · 那台接上都用它）。 */
export const outbox = new Outbox();
