/**
 * **消息流底下的输入框**（说一句只在这一处，R19：聊天是主体、终端是逃生口；手机聊天屏同形 A2-18-8）。跟着当前标签页走。
 *
 * - 一行：［＋］附件 · 字 · ➤；Enter 发、Shift+Enter 换行，组字时回车归输入法；Esc 焦点还给消息流。没发的字按会话各记一份。
 * - 一轮在跑（脸上 `can.stop`，那台判好）、框里没字也没附件 ⇒ ➤ 换成 ■（同手机，二选一）：点了交 `session-interrupt`（不进队、不靠持有输入）；
 *   送到了 · 不知道送没送到 ⇒ 不说（看记录）；那台拒了 · 没送成 ⇒ 框下写那台那一句。说不了（框灰着）也照样能停。
 * - 上方一排（`queue-row.ts`）：那台推来的整份队列名单（那台写的那一句 ·［撤回］）＋ 这一端的待发（`outbox.ts`：在飞 ⇒「发送中 · 等确认」；那台拒了 ⇒ 那台那一句（错误色）；
 *   交了没回执 ⇒ 那台离线照它成品的 `heldSaid`、连着说「排队 · 无应答」）；名单的 `sent` 里有那一句 ⇒ 同回执到了、不再画；
 *   ［撤回］待发等那台答（撤着落盘，重开 · 断过 ⇒ 那台接上照原编号接着问，见 `outbox.ts`）：撤成了（或那台拒过的）字回到输入框（框里已有别的字 ⇒ 另起一行接在后面），
 *   没撤成 ⇒ 拿掉那一行、字不回框、框下照摆撤回包里那台那一句；没答复 ⇒ 字不回框、框下写那一句（首问拿掉那一行；接着问的仍撤着，那台断着时那一行照它成品的 `withdrawHeld`）。
 *   那台离线（机器成品，宿主经 `machineChanged` 交来）⇒ 照手上那一份淡着画、排头写名单是上次所知；
 *   再上一排是附件（`attach-row.ts`，拖到主区上（宿主经 `takeDrops` 交落点）· 粘贴 · ［＋］进同一排）。
 * - 交出只走 `session-queue-put`（经待发）：回执到了（或名单 `sent` 里有它 · 撤的那一问没撤成）⇒ 字与 chip 一起清、框下不另写（那一句此刻什么样只在那一排照名单说）；那台拒了 ⇒ 字留着，那一句留在那一排、那一行写那台那一句（错误色，撤或再发）；
 *   没回执 ⇒ 留在待发（那一排那一行说，框下不另起一行），那台接上照原编号重交（不挂在这条会话开没开着）。
 * - 能不能说只读脸上的 `can.say`（那台判好、`why` 写好）：不能 ⇒ 框灰着、占位就是那一句；去终端只有会话头「终端」那一颗，框旁不另放。
 *   那台离线照常可写可发（成待发，占位写「连上后送」）；已结束能恢复的 ⇒ 可写，发一句 ＝ 先恢复（落后台）再把这句交进它的队。
 */
import { copyText } from "./copy-table";
import { machineName, offlineSaid, settleTyped } from "./control-said";
import { canResume, canStop, isLive } from "./face-can";
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { sentOf, type TerminalSent } from "./terminal-reads";
import type { Tab } from "./tab-model";
import type { Machine } from "./ipc/origin";
import type { QueueList } from "./generated/QueueList";
import type { Rewind } from "./generated/Rewind";
import { hasEditDraft } from "./said-edit";
import { AttachDraft, channelIo, type AttachIo } from "./attach-draft";
import { AttachRow } from "./attach-row";
import { QueueRow, type PendingLine } from "./queue-row";
import { outbox as mainOutbox, type Outbox, type Settle, type Withdrawn } from "./outbox";
import { CHANNEL_CANCEL, CHANNEL_READ, CHANNEL_WATCH, pendingKey, type QueueCancelFn, type QueueMoved, type QueueReadFn, type QueueWatchFn } from "./session-queue";
import { el } from "./kit/el";
import { button, setBusy, setDisabled } from "./kit/button";
import { copyDetailButton, detailOf, saidOf } from "./kit/detail";
import { LS_KEYS, safeGetJson, safeSetJson } from "./local-storage";
import s from "./composer.module.css";

/**
 * 打到一半的字：按「那台 ＋ 会话」一格，落本地存储（重开还在；这扇窗的界面便利，读不到 ⇒ 框空）。空串 ⇒ 删那一格。
 * 每次改都整份写回（格数就是开过几条会话，小）。
 */
class DraftBook {
  private readonly m: Map<string, string>;
  constructor() {
    const saved = safeGetJson<Record<string, unknown>>(LS_KEYS.composerDrafts);
    this.m = new Map(Object.entries(saved ?? {}).filter((e): e is [string, string] => typeof e[1] === "string" && e[1] !== ""));
  }
  get(key: string): string | undefined {
    return this.m.get(key);
  }
  set(key: string, text: string): void {
    if ((this.m.get(key) ?? "") === text) return;
    if (text === "") this.m.delete(key);
    else this.m.set(key, text);
    safeSetJson(LS_KEYS.composerDrafts, Object.fromEntries(this.m));
  }
  delete(key: string): void {
    this.set(key, "");
  }
}

/** 输入框要的几口（缺省走通道；测试换成假的）。 */
export interface ComposerReads {
  /** 会话在那台时附件那几口（字节交壳 · 名单交本机后端）。 */
  io(origin: Machine): AttachIo;
  /** 读那条会话此刻的名单（`session-queue-read`）。 */
  read: QueueReadFn;
  /** 撤队里一条（`session-queue-cancel`）。 */
  cancel: QueueCancelFn;
  /** 订那台的 `changed {session_queue}`。 */
  watch: QueueWatchFn;
}

export const CHANNEL_COMPOSER: ComposerReads = { io: channelIo, read: CHANNEL_READ, cancel: CHANNEL_CANCEL, watch: CHANNEL_WATCH };

/** ■：中断那条会话这一轮（`session-interrupt`）；回包同送字送键。没送成 ⇒ 抛（那一句后端写好）。 */
export type InterruptFn = (origin: Machine, sid: string) => Promise<TerminalSent>;

/** ■ 的期限（同送字送键）。 */
const INTERRUPT_BUDGET_MS = 20_000;

export const CHANNEL_INTERRUPT: InterruptFn = async (origin, sid) =>
  sentOf(await settleTyped(origin, "session-interrupt", callTyped(origin, "session-interrupt", { sid }, budgetWithin(INTERRUPT_BUDGET_MS))));

export interface ComposerHost {
  /** 此刻的标签页（占位标签页显着 · 没有 ⇒ `null`，框收起）。 */
  active(): Tab | null;
  /** 已结束的会话里发一句：先恢复它（落后台）；成了回 `true`。 */
  resume(sid: string): Promise<boolean>;
  /** Esc：焦点还给消息流。 */
  focusStream(): void;
  /** 框多高（消息流底下让出这么多）。 */
  room?(px: number): void;
}

/** 框最多长到几行（再多框内滚）。 */
const MAX_ROWS = 8;

/** 框下那一行：■ 没送成 · 撤回没成那几句（那台写好的，可带复制详情）。 */
type Note = { text: string; detail?: string };

export class Composer {
  readonly el: HTMLElement;
  private readonly box: HTMLTextAreaElement;
  private readonly sendBtn: HTMLButtonElement;
  private readonly sendLabel = copyText("mobChat.input.send");
  /** ■（一轮在跑、框里没字没附件时顶替 ➤）。 */
  private readonly stopBtn: HTMLButtonElement;
  private readonly stopLabel = copyText("composer.input.stop");
  private stopping = false;
  /** 输入那一行：［＋］· 字 · ➤ / ■。 */
  private readonly outRow: HTMLElement;
  private readonly noteEl: HTMLElement;
  private readonly queueRow: QueueRow;
  private readonly attach: AttachRow;
  /** 此刻对着的那条会话（键 ＝ 那台 ＋ 会话）。 */
  private key: string | null = null;
  private readonly drafts = new DraftBook();
  private readonly attachDrafts = new Map<string, AttachDraft>();
  /** 此刻那条会话的名单（那台那一份；还没读到 ⇒ `null`）与它是哪一条。 */
  private queue: QueueList | null = null;
  private queueOf: string | null = null;
  /** 手上的名单换过几次（[`readQueue`] 认答复是不是问出去之后又换过）。 */
  private listGen = 0;
  /** 重问了几次（[`readQueue`] 只认最后那一问的答复）。 */
  private askSeq = 0;
  private watching: { origin: Machine; stop: () => void } | null = null;
  /** 此刻离线的那几台 ⇒ 连接层写好的离线那一句（机器成品 `offlineSaid`）。 */
  private readonly offline = new Map<string, string>();
  /** 各台待发交了没回执时那一句（机器成品 `heldSaid`，拨号那一端写的）。 */
  private readonly held = new Map<string, string>();
  /** 各台待发撤着、那台断着时那一句（机器成品 `withdrawHeld`）。 */
  private readonly withdrawHeld = new Map<string, string>();
  private note: Note | null = null;
  private sending = false;
  /** ➤ 此刻交着的那一句的编号（进待发之后、那一问回来之前）：它有了结局 ⇒ ➤ 当场解开。 */
  private saying: string | null = null;
  private lastRoom = -1;

  constructor(
    private readonly host: ComposerHost,
    private readonly outbox: Outbox = mainOutbox,
    private readonly reads: ComposerReads = CHANNEL_COMPOSER,
    private readonly interrupt: InterruptFn = CHANNEL_INTERRUPT,
  ) {
    this.el = el("div", s.composer);
    this.el.dataset.role = "composer";
    const col = el("div", s.composerCol);
    this.queueRow = new QueueRow({ cancel: (id, text) => void this.cancelQueued(id, text), withdraw: (id) => this.withdraw(id) });
    this.attach = new AttachRow({
      draft: () => this.attachDraft(),
      target: () => {
        const t = this.host.active();
        return { title: t ? t.title : "", machine: t ? machineName(t.origin) : "" };
      },
      offline: () => this.offlineLine(),
      blocked: () => this.sayNo(),
    });
    this.box = el("textarea", s.composerBox);
    this.box.rows = 1;
    this.box.setAttribute("aria-label", copyText("composer.input.aria"));
    this.box.title = copyText("composer.input.keysHint");
    this.box.addEventListener("keydown", (ev) => this.onKey(ev));
    this.box.addEventListener("input", () => this.onInput());
    this.box.addEventListener("paste", (ev) => {
      if (this.attach.paste(ev.clipboardData)) ev.preventDefault();
    });
    this.sendBtn = button({ label: this.sendLabel, kind: "icon", size: "compact", icon: "send", onClick: () => void this.send() });
    this.sendBtn.dataset.role = "composer-send";
    this.stopBtn = button({ label: this.stopLabel, kind: "icon", size: "compact", icon: "stop", onClick: () => void this.stop() });
    this.stopBtn.dataset.role = "composer-stop";
    this.outRow = el("div", s.composerRow);
    this.outRow.append(this.attach.plus, this.box, this.sendBtn, this.stopBtn);
    this.noteEl = el("div", s.composerNote);
    col.append(this.queueRow.el, this.attach.row, this.outRow, this.noteEl);
    this.el.append(col);
    this.outbox.onChange(() => this.paint());
    this.outbox.onSettled((origin, o, end) => this.settled(pendingKey(origin, o.sid), o, end));
    this.outbox.onWithdrawn((origin, o, w) => this.withdrawn(origin, o, w));
    // 构造里不画：宿主那一头（标签页栏）此刻可能还没建好，第一次画由宿主叫 `render()`（同会话头 · 答题块）。
  }

  /** 当前标签页换了 / 它的脸变了（宿主跟着标签页栏一起叫）。 */
  render(): void {
    const t = this.host.active();
    const key = t ? pendingKey(t.origin, t.sessionId) : null;
    if (key !== this.key) {
      if (this.key !== null) this.drafts.set(this.key, this.box.value);
      this.key = key;
      this.queue = null;
      this.queueOf = null;
      this.note = null;
      this.sending = false;
      this.saying = null;
      this.stopping = false;
      this.box.value = key === null ? "" : (this.drafts.get(key) ?? "");
      this.onInput();
      if (t) {
        this.watch(t.origin);
        void this.readQueue();
      } else this.unwatch();
    }
    this.paint();
  }

  /**
   * 一台的状态成品来了（宿主跟着机器条一起交）：离线 ⇒ 记下连接层那句离线，名单照手上那一份淡着画；连着 ⇒ 拿掉。
   * 待发没回执那一行照它的 `heldSaid`，撤着的那一行照它的 `withdrawHeld`。
   */
  machineChanged(origin: string, m: { readonly offline: boolean; readonly offlineSaid: string; readonly heldSaid: string; readonly withdrawHeld: string }): void {
    const now = m.offline ? m.offlineSaid : undefined;
    if (this.offline.get(origin) === now && this.held.get(origin) === m.heldSaid && this.withdrawHeld.get(origin) === m.withdrawHeld) return;
    if (now === undefined) this.offline.delete(origin);
    else this.offline.set(origin, now);
    this.held.set(origin, m.heldSaid);
    this.withdrawHeld.set(origin, m.withdrawHeld);
    if (this.host.active()?.origin === origin) this.paint();
  }

  /** 宿主交来的拖放落点（主区：消息流 ＋ 这个框）：拖进来的文件进这一排、焦点进框。收件只这一处，入口只决定落点。 */
  takeDrops(zone: HTMLElement): void {
    this.attach.dropOn(zone, () => this.focus());
  }

  /** 焦点进框（说不了 ⇒ 不进）。 */
  focus(): void {
    if (!this.box.disabled) this.box.focus();
  }

  /** 框里按 Esc：焦点还给消息流。是这一下就回 `true`。 */
  escFromInput(): boolean {
    if (document.activeElement !== this.box) return false;
    this.host.focusStream();
    return true;
  }

  /** 此刻说不了的那一句（脸上 `can.say` 的 `why`）；说得了 · 还没收到脸（框收着）⇒ `null`。 */
  private sayNo(): string | null {
    const say = this.host.active()?.face?.can.say;
    return say?.can === "no" ? say.why : null;
  }

  /** 此刻那条会话的附件草稿（没有 ⇒ 现建）。 */
  private attachDraft(): AttachDraft | null {
    const t = this.host.active();
    if (!t || this.key === null) return null;
    let d = this.attachDrafts.get(this.key);
    if (d === undefined) {
      const mine = (d = new AttachDraft(t.sessionId, this.reads.io(t.origin), () => {
        if (this.attachDraft() === mine) this.paint();
      }));
      this.attachDrafts.set(this.key, d);
    }
    return d;
  }

  /** 那台断着时的那一句（连接层写好的离线那一句）。 */
  private offlineLine(): string {
    const t = this.host.active();
    return t ? (offlineSaid(t.origin) ?? machineName(t.origin)) : "";
  }

  /** 发框里那一句：已结束能恢复的先恢复（落后台）；再交进那条会话的队（经待发）。 */
  private async send(): Promise<void> {
    const t = this.host.active();
    if (!t || this.sending || this.sayNo() !== null) return;
    const text = this.box.value;
    const draft = this.attachDraft();
    const chips = draft !== null && draft.chips().length > 0;
    if (text.trim() === "" && !chips) return;
    if (chips && draft.blocker(this.offlineLine()) !== null) return;
    const key = this.key;
    this.sending = true;
    this.paint();
    if (!isLive(t.face) && canResume(t.face) && !(await this.host.resume(t.sessionId))) {
      if (key === this.key) this.sending = false;
      this.paint();
      return;
    }
    // 全落好了才有编号（本机后端那一排）；还有没落好的 ⇒ 不送，照那一排的 blocker 画。
    const ids = chips ? await draft.ids() : [];
    if (ids === null) {
      if (key === this.key) this.sending = false;
      this.paint();
      return;
    }
    const said = this.outbox.say(t.origin, t.sessionId, text, ids);
    // 进待发是当场的：记下交着的是哪一句（编号）。
    const id = this.outbox.sameAs(t.origin, t.sessionId, text)?.id ?? null;
    this.saying = id;
    await said;
    // 那一句先有了结局（名单说已送 · 撤成了）⇒ ➤ 已解开、可能已在交下一句：不动它。
    if (key !== this.key || (id !== null && this.saying !== id)) return;
    this.sending = false;
    this.saying = null;
    // 没回执 ⇒ 字留着，那一排那一行换成那台的 heldSaid（接上重交）；有结局的由 [settled] 说。
    this.paint();
  }

  /** 此刻是 ■ 不是 ➤：一轮在跑（脸上 `can.stop`）、框里没字、也没附件（同手机）。 */
  private showsStop(t: Tab): boolean {
    const draft = this.attachDraft();
    return canStop(t.face) && this.box.value.trim() === "" && (draft === null || draft.chips().length === 0);
  }

  /** ■：交 `session-interrupt`；送到了 · 不知道 ⇒ 不说（看记录）；那台拒了 · 没送成 ⇒ 写那台那一句。 */
  private async stop(): Promise<void> {
    const t = this.host.active();
    if (!t || this.stopping || !this.showsStop(t)) return;
    const key = this.key;
    this.stopping = true;
    this.paint();
    let note: Note | null = null;
    try {
      const sent = await this.interrupt(t.origin, t.sessionId);
      if (sent.result === "refused") note = { text: sent.said };
    } catch (e) {
      note = { text: saidOf(e), detail: detailOf(e) };
    }
    if (key !== this.key) return;
    this.stopping = false;
    if (note !== null) this.showNote(note);
    else this.paint();
  }

  /**
   * 一句有了结局（头一次交 · 接上重交的都到这里）：是此刻对着的那条会话 ⇒ 回执到了 · 名单说已送：清框（框里还是那一句）与附件、清掉框下那一行、解开 ➤，只有回执才重问名单；
   * 那台拒了 ⇒ 字留着；那一句留在那一排那一行说（标失败，可撤可重交），框下不另写。编辑已发的那一句由编辑框说，这里不管。
   */
  private settled(key: string, o: { id: string; text: string; rewind?: unknown }, end: Settle): void {
    if (o.rewind !== undefined) return;
    // 那台说已送进会话（名单的 `sent` · 撤的那一问没撤成）同回执到了：清框 · 清附件；名单已在手上，不再重问。
    const landed = end.at === "receipt" || end.at === "sent";
    if (key !== this.key) {
      if (landed && this.drafts.get(key) === o.text) this.drafts.delete(key);
      if (landed) this.attachDrafts.get(key)?.clear();
      return;
    }
    if (!landed) return;
    this.unlockSend(key, o);
    if (this.box.value === o.text) {
      this.box.value = "";
      this.drafts.delete(key);
      this.onInput();
    }
    this.attachDrafts.get(key)?.clear();
    // 那一句此刻什么样只在框上那一排说（照那台的名单）；回执那一句不写到框下 —— 它说的是落盘那一刻，送到之后就是假话。
    this.showNote(null);
    if (end.at === "receipt") void this.readQueue();
  }

  /** 撤待发里那一句：交待发去问那台（见 [`Outbox.withdraw`]），结局由 [withdrawn] 说。 */
  private withdraw(id: string): void {
    const t = this.host.active();
    if (t) void this.outbox.withdraw(t.origin, t.sessionId, id);
  }

  /**
   * 撤一句有了结局（点了撤的 · 那台接上接着问的，不论此刻对着哪条会话）：撤成了（或那台拒过的）⇒ 字回到那条会话的输入框；
   * 没撤成（在送 · 送过）⇒ 当送进去了、字不回框，框下写撤回包里那台那一句；那一问没答复 ⇒ 字不回框，框下写那一问回来的那一句。
   * 编辑已发的那一句字留在编辑草稿，不回输入框；撤成了而草稿已不在 ⇒ 回输入框。
   */
  private withdrawn(origin: Machine, o: { id: string; sid: string; text: string; rewind?: Rewind }, w: Withdrawn): void {
    const key = pendingKey(origin, o.sid);
    if (w.at === "lost") {
      if (key === this.key) this.showNote({ text: w.said, detail: w.detail });
      return;
    }
    if (w.queue !== null) this.gotQueue(origin, o.sid, w.queue);
    if (w.at === "in" && key === this.key) this.showNote({ text: w.said });
    // 编辑那一句的字留在它的编辑草稿；草稿已不在（回执先到、照它删了）⇒ 回输入框，不丢（同手机）。
    if (w.at === "back") {
      this.unlockSend(key, o);
      if (o.rewind === undefined || !hasEditDraft({ origin, sid: o.sid, id: o.rewind.to })) this.recall(o.text, key);
    }
    this.paint();
  }

  /** 交着的那一句有了结局（送进去了 · 撤成了）⇒ ➤ 解开，不等它那一问回来；别的句子的结局不动它。 */
  private unlockSend(key: string, o: { id: string; rewind?: unknown }): void {
    if (key === this.key && o.rewind === undefined && this.saying === o.id) {
      this.sending = false;
      this.saying = null;
    }
  }

  /** 撤名单里那一条，照回的整份名单画；撤成了 ⇒ 字回到输入框（`recall`，编辑已发的那一句没有）；没成 ⇒ 写那台那一句。 */
  private async cancelQueued(id: string, recall: string | null): Promise<void> {
    const t = this.host.active();
    if (!t) return;
    const key = pendingKey(t.origin, t.sessionId);
    try {
      const r = await this.reads.cancel(t.origin, t.sessionId, id);
      if (r.cancelled && recall !== null && key === this.key) this.recall(recall);
      this.gotQueue(t.origin, t.sessionId, r.queue);
      if (!r.cancelled && key === this.key) this.showNote({ text: r.said });
    } catch (e) {
      this.showNote({ text: saidOf(e), detail: detailOf(e) });
    }
  }

  /**
   * 撤回的那一句回到输入框：框空 ⇒ 就是它；框里已是它 ⇒ 不动；框里有别的字 ⇒ 另起一行接在后面，不丢字。
   * 是别的会话的（答复回来时已切走）⇒ 照同一条接进那条会话的草稿。
   */
  private recall(text: string, key: string | null = this.key): void {
    const here = key === this.key;
    const now = here ? this.box.value : key === null ? "" : (this.drafts.get(key) ?? "");
    if (now === text || key === null) return;
    const next = now === "" ? text : `${now}\n${text}`;
    if (!here) {
      this.drafts.set(key, next);
      return;
    }
    this.box.value = next;
    this.onInput();
  }

  /**
   * 重问名单：只认最后问的那一次、且问出去之后手上的名单没换过（那台推来的 · 撤回来的）—— 别的答复都可能是更早那一刻读的
   * （几问同时在飞 · 问与推走两条路，跨台时先后不定），拿它盖掉新的，送到摘掉的那一条就又回到那一排、再也没人撤。
   */
  private async readQueue(): Promise<void> {
    const t = this.host.active();
    if (!t) return;
    const ask = ++this.askSeq;
    const gen = this.listGen;
    let q: QueueList;
    try {
      q = await this.reads.read(t.origin, t.sessionId);
    } catch {
      // 断着 · 那台没有这一条：照旧画手上那一份。
      return;
    }
    // 晚到的答复不画，`sent` 照交：送过的不会变回没送。
    if (ask === this.askSeq && gen === this.listGen) this.gotQueue(t.origin, t.sessionId, q);
    else this.outbox.wentIn(t.origin, t.sessionId, q.sent);
  }

  /** 那条会话的一份名单（推来的 · 重问的 · 撤回包里的）：`sent` 交待发（送进去了的离开待发）；是此刻对着的那条 ⇒ 照它画。 */
  private gotQueue(origin: Machine, sid: string, q: QueueList): void {
    this.outbox.wentIn(origin, sid, q.sent);
    const key = pendingKey(origin, sid);
    if (key !== this.key) return;
    this.listGen += 1;
    this.queue = q;
    this.queueOf = key;
    this.paint();
  }

  private watch(origin: Machine): void {
    if (this.watching?.origin === origin) return;
    this.unwatch();
    this.watching = { origin, stop: this.reads.watch(origin, (m) => this.queueMoved(origin, m)) };
  }

  private unwatch(): void {
    this.watching?.stop();
    this.watching = null;
  }

  private queueMoved(origin: Machine, m: QueueMoved): void {
    const t = this.host.active();
    if (!t || t.origin !== origin) return;
    // 同一帧里别的会话的名单也把 `sent` 交待发（那几条会话的待发不等打开才收）。
    for (const [sid, q] of m.got) if (sid !== t.sessionId) this.outbox.wentIn(origin, sid, q.sent);
    const got = m.got.get(t.sessionId);
    if (got !== undefined) this.gotQueue(origin, t.sessionId, got);
    else if (m.all || m.ask.includes(t.sessionId)) void this.readQueue();
  }

  private showNote(note: Note | null): void {
    this.note = note;
    this.paint();
  }

  private onKey(ev: KeyboardEvent): void {
    if (ev.isComposing || ev.keyCode === 229) return; // 组字中的键归输入法
    if (ev.key === "Enter" && !ev.shiftKey && !ev.ctrlKey && !ev.altKey && !ev.metaKey) {
      ev.preventDefault();
      void this.send();
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      this.host.focusStream();
    }
  }

  private onInput(): void {
    if (this.key !== null) this.drafts.set(this.key, this.box.value);
    const lines = this.box.value.split("\n").length;
    this.box.rows = Math.min(MAX_ROWS, Math.max(1, lines));
    const t = this.host.active();
    if (t !== null) this.paintSendOrStop(t);
    this.measure();
  }

  private paint(): void {
    const t = this.host.active();
    // 还没收到这条会话的脸 ⇒ 不知道能不能说，框先收着（同状态点：不画）。
    const say = t?.face?.can.say;
    this.el.hidden = t === null || say === undefined;
    if (t === null || say === undefined) {
      this.measure();
      return;
    }
    const no = this.sayNo();
    this.box.disabled = no !== null;
    // 占位就是那台写好的那一句：说得了 ⇒ `hint`（发消息 · 已结束会先恢复 · 连上后送）；说不了 ⇒ `why`。
    this.box.placeholder = say.can === "yes" ? say.hint : say.why;
    setDisabled(this.sendBtn, no ?? this.attachDraft()?.blocker(this.offlineLine()) ?? null);
    setBusy(this.sendBtn, this.sending ? this.sendLabel : null);
    this.paintSendOrStop(t);
    this.attach.paint();
    this.queueRow.paint(this.queueOf === this.key ? this.queue : null, this.pendingLines(t.origin, t.sessionId), this.offline.has(t.origin));
    // 说不了时框里那一句占位就是那台写的出路；去终端只有会话头「终端」那一颗，框旁不另放。
    const parts: HTMLElement[] = [];
    if (this.note) {
      parts.push(el("span", s.composerNoteError, this.note.text));
      const copy = copyDetailButton(this.note.text, this.note.detail ?? "");
      if (copy) parts.push(copy);
    }
    this.noteEl.replaceChildren(...parts);
    this.measure();
  }

  /**
   * 那条会话的待发那几行：在飞 ⇒「发送中 · 等确认」；交了没回执：那台此刻离线 ⇒ 那台成品的 `heldSaid`；那台连着（超时 · 回包读不出）⇒
   * 「排队 · 无应答」（不借 heldSaid：连着时说离线是假话）。两种都是接上照原编号重交。那台拒了 ⇒ 那台写的那一句（错误色），接上不重交。
   * 撤着、此刻没在问的（上一回撤着没等到答复），那台此刻离线 ⇒ 那台成品的 `withdrawHeld`（淡着）：它不再交，借 `heldSaid` 说补交是假话（同手机）。
   */
  private pendingLines(origin: Machine, sid: string): PendingLine[] {
    const down = this.offline.has(origin);
    const ended = (): string => (down ? this.held.get(origin) : undefined) ?? copyText("beQueue.said.noAnswer");
    return this.outbox.of(origin, sid).map((o) => {
      const line = { id: o.id, text: o.text };
      if (this.outbox.inFlight(o.id)) return { ...line, said: copyText("beQueue.said.confirming"), held: false, refused: false };
      if (o.refused !== undefined) return { ...line, said: o.refused, held: false, refused: true };
      const parked = down && this.outbox.parked(o.id) ? this.withdrawHeld.get(origin) : undefined;
      return { ...line, said: parked ?? ended(), held: true, refused: false };
    });
  }

  /** ➤ 与 ■ 二选一（打字 · 删字时也跟着换，不等整份重画）。 */
  private paintSendOrStop(t: Tab): void {
    const stop = this.showsStop(t);
    this.sendBtn.style.display = stop ? "none" : "";
    this.stopBtn.style.display = stop ? "" : "none";
    setBusy(this.stopBtn, this.stopping ? this.stopLabel : null);
  }

  /** 量自己多高、交宿主（变了才交）。 */
  private measure(): void {
    const h = this.el.hidden ? 0 : this.el.offsetHeight;
    if (h === this.lastRoom) return;
    this.lastRoom = h;
    this.host.room?.(h);
  }
}
