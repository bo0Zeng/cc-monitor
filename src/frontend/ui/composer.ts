/**
 * **消息流底下的输入框**（说一句只在这一处，R19：聊天是主体、终端是逃生口；手机聊天屏同形 A2-18-8）。跟着当前标签页走。
 *
 * - 一行：［＋］附件 · 字 · ➤；Enter 发、Shift+Enter 换行，组字时回车归输入法；Esc 焦点还给消息流。没发的字按会话各记一份。
 * - 上方一排（`queue-row.ts`）：这一端的待发（`outbox.ts`，发送中 ·［撤回］）＋ 那台推来的整份队列名单（排第 n · 谁发的 ·［撤回］）；
 *   再上一排是附件（`attach-row.ts`，拖到框上 · 粘贴 · ［＋］进同一排）。
 * - 交出只走 `session-queue-put`（经待发）：回执到了 ⇒ 字与 chip 一起清、框下写那台那一句；那台拒了 ⇒ 字留着、写那台那一句；
 *   没回执 ⇒ 留在待发（那一排「发送中」），那台接上照原编号重交（不挂在这条会话开没开着）。
 * - 能不能说只读脸上的 `can.say`（那台判好、`why` 写好）：不能 ⇒ 框灰着、占位就是那一句，还有终端可去 ⇒ 给［切到终端］。
 *   那台离线照常可写可发（成待发，占位写「连上后送」）；已结束能恢复的 ⇒ 可写，发一句 ＝ 先恢复（落后台）再把这句交进它的队。
 */
import { copyText } from "./copy-table";
import { machineName, offlineSaid } from "./control-said";
import { canResume, hasTerminal, isLive, machineOf } from "./face-can";
import { fullTitle } from "./session-face";
import { terminalFrontAvailable } from "./terminal-front";
import type { Tab } from "./tab-model";
import type { Origin } from "./ipc/origin";
import type { QueueList } from "./generated/QueueList";
import { AttachDraft, channelIo, type AttachIo } from "./attach-draft";
import { AttachRow } from "./attach-row";
import { QueueRow } from "./queue-row";
import { outbox as mainOutbox, type Outbox } from "./outbox";
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
  /** 那台的附件那几口（判收不收 · 逐块传 · 提交 · 撤）。 */
  io(origin: Origin): AttachIo;
  /** 读那条会话此刻的名单（`session-queue-read`）。 */
  read: QueueReadFn;
  /** 撤队里一条（`session-queue-cancel`）。 */
  cancel: QueueCancelFn;
  /** 订那台的 `changed {session_queue}`。 */
  watch: QueueWatchFn;
}

export const CHANNEL_COMPOSER: ComposerReads = { io: channelIo, read: CHANNEL_READ, cancel: CHANNEL_CANCEL, watch: CHANNEL_WATCH };

export interface ComposerHost {
  /** 此刻的标签页（占位标签页显着 · 没有 ⇒ `null`，框收起）。 */
  active(): Tab | null;
  /** 已结束的会话里发一句：先恢复它（落后台）；成了回 `true`。 */
  resume(sid: string): Promise<boolean>;
  /** ↗ 切到这条会话的终端窗口（说不了时那条出路）。 */
  front(sid: string): void;
  /** Esc：焦点还给消息流。 */
  focusStream(): void;
  /** 框多高（消息流底下让出这么多）。 */
  room?(px: number): void;
}

/** 框最多长到几行（再多框内滚）。 */
const MAX_ROWS = 8;

/** 发完那一句（那台写好的）：成 · 没成（可带复制详情）。 */
type Note = { text: string; tone: "ok" | "error"; detail?: string };

export class Composer {
  readonly el: HTMLElement;
  private readonly box: HTMLTextAreaElement;
  private readonly sendBtn: HTMLButtonElement;
  private readonly sendLabel = copyText("composer.input.send");
  /** 说不了时那条出路［切到终端］（还有终端可去才出）。 */
  private readonly frontBtn: HTMLButtonElement;
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
  private watching: { origin: Origin; stop: () => void } | null = null;
  private note: Note | null = null;
  private sending = false;
  private lastRoom = -1;

  constructor(
    private readonly host: ComposerHost,
    private readonly outbox: Outbox = mainOutbox,
    private readonly reads: ComposerReads = CHANNEL_COMPOSER,
  ) {
    this.el = el("div", s.composer);
    this.el.dataset.role = "composer";
    const col = el("div", s.composerCol);
    this.queueRow = new QueueRow({ cancel: (id) => void this.cancelQueued(id), cancelPending: (id) => this.cancelPending(id) });
    this.attach = new AttachRow({
      draft: () => this.attachDraft(),
      target: () => {
        const t = this.host.active();
        return { title: t ? fullTitle(t) : "", machine: t ? machineOf(t) : "" };
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
    this.outRow = el("div", s.composerRow);
    this.outRow.append(this.attach.plus, this.box, this.sendBtn);
    this.noteEl = el("div", s.composerNote);
    this.frontBtn = button({
      label: copyText("terminal.head.front"),
      kind: "ghost",
      size: "compact",
      icon: "front",
      onClick: () => {
        const t = this.host.active();
        if (t) this.host.front(t.sessionId);
      },
    });
    col.append(this.queueRow.el, this.attach.row, this.outRow, this.noteEl);
    this.el.append(col);
    this.attach.dropOn(this.el);
    this.outbox.onChange(() => this.paint());
    this.outbox.onSettled((origin, o, end) => this.settled(pendingKey(origin, o.sid), o, end));
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
      this.box.value = key === null ? "" : (this.drafts.get(key) ?? "");
      this.onInput();
      if (t) {
        this.watch(t.origin);
        void this.readQueue();
      } else this.unwatch();
    }
    this.paint();
  }

  /** 一台接上了：断着的附件接着传、名单重问（待发的重交在 `outbox` 那一份，宿主另叫）。 */
  machineUp(origin: string): void {
    for (const [k, d] of this.attachDrafts) if (k.startsWith(`${origin}\u0000`)) d.resume();
    const t = this.host.active();
    if (t && t.origin === origin) void this.readQueue();
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
    const end = await this.outbox.say(t.origin, t.sessionId, text, chips ? draft.ids() : []);
    if (key !== this.key) return;
    this.sending = false;
    // 没回执 ⇒ 字留着、写那一句（那一排「发送中」，接上重交）；有结局的由 [settled] 说。
    if (end.at === "none") this.showNote({ text: end.said, tone: "error" });
    else this.paint();
  }

  /**
   * 一句有了结局（头一次交 · 接上重交的都到这里）：是此刻对着的那条会话 ⇒ 回执到了清框（框里还是那一句）与附件、写那台那一句、重问名单；
   * 那台拒了 ⇒ 字留着、写那台那一句。编辑已发的那一句由编辑框说，这里不管。
   */
  private settled(key: string, o: { text: string; rewind?: unknown }, end: { at: string; said: string }): void {
    if (o.rewind !== undefined) return;
    if (key !== this.key) {
      if (end.at === "receipt" && this.drafts.get(key) === o.text) this.drafts.delete(key);
      if (end.at === "receipt") this.attachDrafts.get(key)?.clear();
      return;
    }
    if (end.at !== "receipt") {
      this.showNote({ text: end.said, tone: "error" });
      return;
    }
    if (this.box.value === o.text) {
      this.box.value = "";
      this.drafts.delete(key);
      this.onInput();
    }
    this.attachDrafts.get(key)?.clear();
    void this.readQueue();
    this.showNote({ text: end.said, tone: "ok" });
  }

  /** 撤待发里那一句：本地拿掉；万一已交到那台，照编号请它撤（没撤成 ⇒ 写那台那一句，不吞）。 */
  private cancelPending(id: string): void {
    const t = this.host.active();
    if (!t) return;
    this.outbox.drop(t.origin, t.sessionId, id);
    this.sending = false;
    const key = pendingKey(t.origin, t.sessionId);
    void this.reads.cancel(t.origin, t.sessionId, id).then(
      (r) => this.gotQueue(key, r.queue),
      (e: unknown) => this.showNote({ text: saidOf(e), tone: "error", detail: detailOf(e) }),
    );
  }

  /** 撤名单里那一条，照回的整份名单画；没成 ⇒ 写那台那一句。 */
  private async cancelQueued(id: string): Promise<void> {
    const t = this.host.active();
    if (!t) return;
    try {
      const r = await this.reads.cancel(t.origin, t.sessionId, id);
      this.gotQueue(pendingKey(t.origin, t.sessionId), r.queue);
    } catch (e) {
      this.showNote({ text: saidOf(e), tone: "error", detail: detailOf(e) });
    }
  }

  private async readQueue(): Promise<void> {
    const t = this.host.active();
    if (!t) return;
    try {
      this.gotQueue(pendingKey(t.origin, t.sessionId), await this.reads.read(t.origin, t.sessionId));
    } catch {
      // 断着 · 那台没有这一条：照旧画手上那一份。
    }
  }

  private gotQueue(key: string, q: QueueList): void {
    if (key !== this.key) return;
    this.queue = q;
    this.queueOf = key;
    this.paint();
  }

  private watch(origin: Origin): void {
    if (this.watching?.origin === origin) return;
    this.unwatch();
    this.watching = { origin, stop: this.reads.watch(origin, (m) => this.queueMoved(origin, m)) };
  }

  private unwatch(): void {
    this.watching?.stop();
    this.watching = null;
  }

  private queueMoved(origin: Origin, m: QueueMoved): void {
    const t = this.host.active();
    if (!t || t.origin !== origin) return;
    const got = m.got.get(t.sessionId);
    if (got !== undefined) this.gotQueue(pendingKey(origin, t.sessionId), got);
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
    this.attach.paint();
    this.queueRow.paint(this.queueOf === this.key ? this.queue : null, this.outbox.of(t.origin, t.sessionId));
    // 说不了、还有终端可去 ⇒ 那条出路（同会话头 ↗ 那一道）。
    this.frontBtn.style.display = no !== null && hasTerminal(t.face) && terminalFrontAvailable() ? "" : "none";
    const parts: HTMLElement[] = [this.frontBtn];
    if (this.note) {
      parts.push(el("span", this.note.tone === "ok" ? s.composerNoteOk : s.composerNoteError, this.note.text));
      const copy = copyDetailButton(this.note.text, this.note.detail ?? "");
      if (copy) parts.push(copy);
    }
    this.noteEl.replaceChildren(...parts);
    this.measure();
  }

  /** 量自己多高、交宿主（变了才交）。 */
  private measure(): void {
    const h = this.el.hidden ? 0 : this.el.offsetHeight;
    if (h === this.lastRoom) return;
    this.lastRoom = h;
    this.host.room?.(h);
  }
}
