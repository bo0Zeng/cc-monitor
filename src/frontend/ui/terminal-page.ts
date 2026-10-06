/**
 * 底部抽屉的「终端」页：当前标签页那个会话所在终端的一屏（这一版是快照）＋ 一行输入 ＋ 几颗常用键。跟着当前标签页走。
 *
 * - 找终端：问那台 `terminals-list`，按会话 ID 认出那一行；没有那一行 ⇒ 已结束的写「已结束 · 无终端」，活着的写「非 cc-monitor 启动」。
 * - 画面：开到这一页 / 切到这个标签页时抓一次；送字送键之后 0.5 · 1.5 · 3 秒各再抓一次；其余时候点「重新看」。开着不轮询。
 * - 能不能送、输入会不会和别的终端窗口混在一起：只读名单里后端给的 `can.input` · `clients` · `input`，界面不判。
 * - 送字带上看到的那一屏的指纹：画面已经变了 ⇒ 后端不送，这里重抓给人看。不知道送没送到 ⇒ 照实说、不重发。
 * - 输入框：回车送字并补回车 · Shift+回车换行 · Ctrl+回车只送字；组字时回车归输入法；Esc 只把焦点还给消息流（不清字、不收抽屉）。
 *   没送的字按会话各记一份，切回来还在。
 * - 页头右边「切到终端」只在 ↗ 真能用时出（同会话头那一道）。
 */
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";
import { saidOfControl, machineName } from "./control-said";
import { canResume, hasTerminal } from "./tab-session-state";
import { terminalFrontAvailable } from "./terminal-front";
import { dotLabel, dotOf, fullTitle, machineOf } from "./session-face";
import { listTerminals, previewShot, sendToTerminal, type TerminalRow, type TerminalSend, type TerminalSent, type TerminalShot } from "./terminal-reads";
import type { Origin } from "./ipc/origin";
import { button, setBusy, setButtonLabel, setDisabled } from "./kit/button";
import { banner } from "./kit/banner";
import { emptyState } from "./kit/empty";
import { statusDot, setDot } from "./kit/status-dot";
import { spinner } from "./kit/progress";
import s from "./terminal-page.module.css";

/** 终端页要的三条读写（缺省走通道；测试换成假的）。 */
export interface TerminalReads {
  list(origin: Origin, label: string): Promise<TerminalRow[]>;
  shot(origin: Origin, terminal: string, label: string): Promise<TerminalShot>;
  send(origin: Origin, terminal: string, what: TerminalSend, seen: string | null, label: string): Promise<TerminalSent>;
}

export const CHANNEL_READS: TerminalReads = {
  list: listTerminals,
  shot: (origin, terminal, label) => previewShot(origin, { terminal }, label),
  send: sendToTerminal,
};

export interface TerminalPageHost {
  active(): Tab | null;
  /** ↗ 切到这个会话的终端窗口。 */
  front(sid: string): void;
  /** Esc：焦点还给消息流。 */
  focusStream(): void;
}

/** 送字 / 送键之后再抓的几个时刻（ms）。 */
export const RECAPTURE_AFTER_MS = [500, 1500, 3000] as const;
/** 「已送达」停留多久。 */
const DELIVERED_MS = 2000;
/** 超过这么多字，送出键写出字数。 */
const LONG_TEXT = 2000;
/** 输入框最多长到几行（再多框内滚）。 */
const MAX_INPUT_ROWS = 6;

/** 常用键：页上的字 · 送什么。 */
const KEYS: readonly { label: string; what: TerminalSend }[] = [
  { label: copyText("terminal.key.esc"), what: { key: "esc" } },
  { label: copyText("terminal.key.ctrlC"), what: { key: "ctrl-c" } },
  { label: copyText("terminal.key.up"), what: { key: "up" } },
  { label: copyText("terminal.key.down"), what: { key: "down" } },
  { label: copyText("terminal.key.tab"), what: { key: "tab" } },
  { label: copyText("terminal.key.enter"), what: { key: "enter" } },
  { label: "1", what: { text: "1", enter: false } },
  { label: "2", what: { text: "2", enter: false } },
  { label: "3", what: { text: "3", enter: false } },
];

/** 后端给的「送不了」原因码 ⇒ 一个词。 */
function whyWord(code: string): string {
  switch (code) {
    case "not-yours":
      return copyText("terminal.why.notYours");
    case "not-managed":
      return copyText("terminal.why.notManaged");
    case "not-known":
    case "ended":
      return copyText("terminal.why.gone");
    case "ambiguous":
      return copyText("terminal.why.ambiguous");
    default:
      return copyText("terminal.why.other");
  }
}

/** 送字那一步的结局一句：送到了（一会儿就走）· 没送成（可带重试）。 */
type Note = { text: string; tone: "ok" | "error"; retry?: TerminalSend };

function clock(sec: number): string {
  const d = new Date(sec * 1000);
  const p = (n: number): string => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (text !== undefined) e.textContent = text;
  return e;
}

export class TerminalPage {
  readonly el: HTMLElement;
  private visible = false;
  /** 正在看的会话（`null` ＝ 没有当前标签页）。 */
  private sid: string | null = null;
  private origin: Origin | undefined = undefined;
  private row: TerminalRow | null = null;
  private shot: TerminalShot | null = null;
  /** 名单那一步的结局：还在问 · 认出来了 · 名单里没有 · 问不到（那一句）。 */
  private phase: "loading" | "ready" | "none" | "error" = "loading";
  private error = "";
  /** 抓屏那一步出的错（画面留着、降不透明度）。 */
  private shotError: string | null = null;
  /** 送字那一步的结局一句（`null` ＝ 没有）。 */
  private note: Note | null = null;
  private sending = false;
  /** 没送的字，按会话各记一份。 */
  private readonly drafts = new Map<string, string>();
  /** 每一次名单 / 抓屏的代次：慢的那次回来不许盖掉后发的那次、不许落到切走之后的会话上。 */
  private seq = 0;
  private timers: ReturnType<typeof setTimeout>[] = [];
  private noteTimer: ReturnType<typeof setTimeout> | null = null;

  private readonly head: HTMLElement;
  private readonly dot: HTMLSpanElement;
  private readonly title: HTMLElement;
  private readonly where: HTMLElement;
  private readonly tag: HTMLElement;
  /** 连着几个终端窗口 / 后台。 */
  private readonly tag2: HTMLElement;
  private readonly at: HTMLElement;
  private readonly frontBtn: HTMLButtonElement;
  private readonly bar: HTMLElement;
  private readonly screen: HTMLPreElement;
  private readonly inputArea: HTMLElement;
  private readonly to: HTMLElement;
  private readonly owner: HTMLElement;
  private readonly box: HTMLTextAreaElement;
  private readonly sendBtn: HTMLButtonElement;
  private readonly keyBtns: HTMLButtonElement[] = [];
  private readonly noteEl: HTMLElement;
  private readonly body: HTMLElement;

  constructor(
    private readonly host: TerminalPageHost,
    private readonly reads: TerminalReads = CHANNEL_READS,
  ) {
    this.el = el("div");
    this.el.className = s.term;
    this.head = el("div");
    this.head.className = s.termHead;
    this.dot = statusDot("running", copyText("sessionFace.dot.running"), "compact");
    this.title = el("span");
    this.title.className = s.termTitle;
    this.where = el("span");
    this.where.className = s.termWhere;
    this.tag = el("span");
    this.tag.className = s.termTag;
    this.tag2 = el("span");
    this.tag2.className = s.termTag;
    this.at = el("span");
    this.at.className = s.termAt;
    const again = button({ label: copyText("terminal.head.recapture"), kind: "ghost", size: "compact", onClick: () => void this.recapture() });
    const sp = el("span");
    sp.className = s.termSp;
    this.frontBtn = button({ label: copyText("terminal.head.front"), kind: "ghost", size: "compact", icon: "front", onClick: () => this.sid !== null && this.host.front(this.sid) });
    this.head.append(this.dot, this.title, this.where, this.tag, this.tag2, this.at, again, sp, this.frontBtn);
    this.bar = el("div");
    this.bar.className = s.termBar;
    this.screen = el("pre");
    this.screen.className = s.termScreen;
    this.screen.tabIndex = 0;

    this.inputArea = el("div");
    this.inputArea.className = s.termInput;
    const toRow = el("div");
    toRow.className = s.termToRow;
    this.to = el("span");
    this.to.className = s.termTo;
    this.owner = el("span");
    this.owner.className = s.termOwner;
    const toSp = el("span");
    toSp.className = s.termSp;
    toRow.append(this.to, toSp, this.owner);
    const boxRow = el("div");
    boxRow.className = s.termBoxRow;
    this.box = el("textarea");
    this.box.className = s.termBox;
    this.box.rows = 1;
    this.box.addEventListener("keydown", (ev) => this.onBoxKey(ev));
    this.box.addEventListener("input", () => this.onBoxInput());
    this.sendBtn = button({ label: copyText("terminal.input.send"), size: "compact", onClick: () => this.sendBox(true) });
    boxRow.append(this.box, this.sendBtn);
    const keys = el("div");
    keys.className = s.termKeys;
    const keysLabel = el("span", copyText("terminal.input.keys"));
    keysLabel.className = s.termKeysLabel;
    keys.appendChild(keysLabel);
    for (const k of KEYS) {
      const b = button({ label: k.label, size: "compact", hint: copyText("terminal.input.keyHint", { key: k.label }), onClick: () => void this.send(k.what) });
      this.keyBtns.push(b);
      keys.appendChild(b);
    }
    this.noteEl = el("div");
    this.noteEl.className = s.termNote;
    this.inputArea.append(toRow, boxRow, keys, this.noteEl);
    this.body = el("div");
    this.body.className = s.termBody;
    this.el.append(this.body);
    this.paint();
  }

  /** 抽屉开到 / 离开这一页（宿主调）。开到的那一刻抓一次。 */
  setVisible(on: boolean): void {
    if (on === this.visible) return;
    this.visible = on;
    if (on) void this.refresh();
    else this.stopTimers();
  }

  /** 当前标签页换了 / 它的状态变了：换了会话 ⇒ 字按会话存取、开着就重新找终端抓一屏；同一个会话 ⇒ 只重画页头。 */
  sessionChanged(): void {
    const tab = this.host.active();
    const sid = tab?.sessionId ?? null;
    if (sid === this.sid) {
      this.paint();
      return;
    }
    if (this.sid !== null) this.drafts.set(this.sid, this.box.value);
    this.stopTimers();
    this.seq++;
    this.sid = sid;
    this.origin = tab?.origin;
    this.row = null;
    this.shot = null;
    this.shotError = null;
    this.note = null;
    this.sending = false;
    this.phase = "loading";
    this.box.value = sid === null ? "" : (this.drafts.get(sid) ?? "");
    this.onBoxInput();
    this.paint();
    if (this.visible) void this.refresh();
  }

  /** 找终端、抓一屏（名单也重问：终端可能刚起 / 刚没）。 */
  async refresh(): Promise<void> {
    const tab = this.host.active();
    if (!tab || tab.sessionId !== this.sid || this.origin === undefined) {
      this.paint();
      return;
    }
    const mine = ++this.seq;
    const origin = this.origin;
    const label = fullTitle(tab);
    if (this.row === null) {
      this.phase = "loading";
      this.paint();
    }
    try {
      const rows = await this.reads.list(origin, label);
      if (mine !== this.seq) return;
      const hits = rows.filter((r) => r.sid === this.sid);
      if (hits.length === 0) {
        this.row = null;
        this.phase = "none";
        this.paint();
        return;
      }
      if (hits.length > 1) {
        this.row = null;
        this.phase = "error";
        this.error = whyWord("ambiguous");
        this.paint();
        return;
      }
      this.row = hits[0];
      this.phase = "ready";
    } catch (e) {
      if (mine !== this.seq) return;
      this.phase = "error";
      this.error = saidOfControl(e);
      this.paint();
      return;
    }
    await this.capture(mine);
  }

  /** 只重抓一屏（「重新看」· 送完之后那几拍）。还没认出终端 ⇒ 整个重来。 */
  async recapture(): Promise<void> {
    if (this.row === null) return this.refresh();
    await this.capture(++this.seq);
  }

  private async capture(mine: number): Promise<void> {
    const tab = this.host.active();
    const row = this.row;
    if (!tab || row === null || this.origin === undefined) return;
    try {
      const shot = await this.reads.shot(this.origin, row.terminal, fullTitle(tab));
      if (mine !== this.seq) return;
      const stick = this.shot === null || this.screen.scrollTop + this.screen.clientHeight >= this.screen.scrollHeight - 4;
      this.shot = shot;
      this.shotError = null;
      this.paint();
      if (stick) this.screen.scrollTop = this.screen.scrollHeight;
    } catch (e) {
      if (mine !== this.seq) return;
      this.shotError = saidOfControl(e);
      this.paint();
    }
  }

  private stopTimers(): void {
    for (const t of this.timers) clearTimeout(t);
    this.timers = [];
  }

  private schedule(): void {
    this.stopTimers();
    for (const ms of RECAPTURE_AFTER_MS) this.timers.push(setTimeout(() => void this.recapture(), ms));
  }

  private showNote(note: Note | null): void {
    this.note = note;
    if (this.noteTimer !== null) clearTimeout(this.noteTimer);
    this.noteTimer = null;
    if (note?.tone === "ok") {
      this.noteTimer = setTimeout(() => {
        this.note = null;
        this.paint();
      }, DELIVERED_MS);
    }
    this.paint();
  }

  /** 送一段字或一颗键。能不能送由名单里后端那一格说了算；送完按回话说一句、该重抓就重抓。 */
  async send(what: TerminalSend): Promise<boolean> {
    const tab = this.host.active();
    const row = this.row;
    if (!tab || row === null || this.origin === undefined || this.sending || row.inputNo !== null) return false;
    const sid = this.sid;
    this.sending = true;
    this.paint();
    let sent: TerminalSent | null = null;
    let failed: string | null = null;
    try {
      sent = await this.reads.send(this.origin, row.terminal, what, this.shot?.screen ?? null, fullTitle(tab));
    } catch (e) {
      failed = saidOfControl(e);
    }
    if (sid !== this.sid) return false;
    this.sending = false;
    if (failed !== null) {
      this.showNote({ text: failed, tone: "error", retry: what });
      return false;
    }
    if (sent?.result === "delivered") {
      this.showNote({ text: copyText("terminal.input.delivered"), tone: "ok" });
      this.schedule();
      return true;
    }
    if (sent?.result === "unsure") {
      this.showNote({ text: copyText("terminal.input.unsure", { machine: machineName(this.origin) }), tone: "error" });
      void this.recapture();
      return false;
    }
    const why = sent?.result === "refused" ? sent.why : "";
    if (why === "screen-changed") {
      this.showNote({ text: copyText("terminal.input.screenChanged"), tone: "error" });
      void this.recapture();
      return false;
    }
    this.showNote({ text: copyText("terminal.input.failed", { why: whyWord(why) }), tone: "error", retry: what });
    if (why === "not-known" || why === "ended") void this.refresh();
    return false;
  }

  /** 送框里那段字（`enter` ＝ 之后补一个回车）。送到了才清框。 */
  private sendBox(enter: boolean): void {
    const text = this.box.value;
    if (text === "") return;
    void this.send({ text, enter }).then((ok) => {
      if (ok && this.box.value === text) {
        this.box.value = "";
        if (this.sid !== null) this.drafts.delete(this.sid);
        this.onBoxInput();
      }
    });
  }

  private onBoxKey(ev: KeyboardEvent): void {
    if (ev.isComposing || ev.keyCode === 229) return; // 组字中的键归输入法
    if (ev.key === "Enter" && !ev.shiftKey) {
      ev.preventDefault();
      this.sendBox(!ev.ctrlKey);
    }
  }

  private onBoxInput(): void {
    const lines = this.box.value.split("\n").length;
    this.box.rows = Math.min(MAX_INPUT_ROWS, Math.max(1, lines));
    const n = [...this.box.value].length;
    setButtonLabel(this.sendBtn, n > LONG_TEXT ? copyText("terminal.input.sendLong", { n: n.toLocaleString("en-US") }) : copyText("terminal.input.send"));
  }

  /** 输入框里按 Esc：焦点还给消息流（不清字、不收抽屉）。是这一下就回 `true`。 */
  escFromInput(): boolean {
    if (document.activeElement !== this.box) return false;
    this.host.focusStream();
    return true;
  }

  /** 按此刻的状态重画。 */
  private paint(): void {
    const tab = this.host.active();
    if (!tab || this.sid === null) {
      this.body.replaceChildren(emptyState({ icon: "terminal", text: copyText("terminal.empty.noSession") }));
      return;
    }
    if (this.phase === "none") {
      const ended = canResume(tab.state); // 已结束（能恢复的那一态）
      this.body.replaceChildren(
        emptyState({
          icon: "terminal",
          text: ended ? copyText("terminal.empty.ended", { state: copyText("sessionState.ended.name") }) : copyText("terminal.empty.notOurs"),
          hint: ended ? copyText("terminal.empty.endedHint") : copyText("terminal.empty.notOursHint"),
        }),
      );
      return;
    }
    if (this.phase === "loading") {
      const l = el("div");
      l.className = s.termLoading;
      l.append(spinner(), el("span", copyText("terminal.state.loading")));
      this.body.replaceChildren(l);
      return;
    }
    if (this.phase === "error") {
      this.body.replaceChildren(banner("error", this.error, [button({ label: copyText("terminal.state.refresh"), size: "compact", onClick: () => void this.refresh() })]));
      return;
    }
    const row = this.row as TerminalRow;
    const d = dotOf(tab);
    setDot(this.dot, d, dotLabel(d));
    this.title.textContent = fullTitle(tab);
    this.where.textContent = tab.projectDir ? copyText("terminal.head.where", { dir: tab.projectDir, machine: machineOf(tab) }) : machineOf(tab);
    this.tag.textContent = row.tmuxName !== "" ? copyText("terminal.tag.tmux", { name: row.tmuxName }) : "";
    this.tag.style.display = row.tmuxName !== "" ? "" : "none";
    this.tag2.textContent = row.clients > 0 ? copyText("terminal.tag.windows", { n: row.clients }) : copyText("terminal.tag.background");
    this.at.textContent = this.shot ? copyText("terminal.head.snapAt", { time: clock(this.shot.at) }) : "";
    this.frontBtn.style.display = terminalFrontAvailable() && hasTerminal(tab.state) ? "" : "none";

    const bars: HTMLElement[] = [];
    if (row.programExited) {
      const line = el("div", copyText("terminal.bar.programExited", { state: copyText("sessionState.reconnectable.name") }));
      line.className = s.termBarLine;
      bars.push(line);
    }
    if (this.shotError !== null) bars.push(banner("warn", this.shotError, [button({ label: copyText("terminal.state.refresh"), size: "compact", onClick: () => void this.refresh() })]));
    this.bar.replaceChildren(...bars);
    this.screen.textContent = this.shot?.text ?? "";
    this.screen.dataset.stale = String(this.shotError !== null);

    this.to.textContent = copyText("terminal.input.to", { title: fullTitle(tab), machine: machineOf(tab) });
    this.owner.textContent = row.clients > 0 && row.input === "shared" ? copyText("terminal.input.shared", { n: row.clients }) : row.clients === 0 ? copyText("terminal.input.nobody") : "";
    const no = row.inputNo === null ? null : whyWord(row.inputNo);
    this.box.disabled = no !== null || this.sending;
    this.box.placeholder = no !== null ? copyText("terminal.input.readOnly", { why: no }) : copyText("terminal.input.placeholder");
    setDisabled(this.sendBtn, no);
    setBusy(this.sendBtn, this.sending ? copyText("terminal.input.send") : null);
    for (const b of this.keyBtns) setDisabled(b, no);
    this.noteEl.replaceChildren();
    if (this.note) {
      const t = el("span", this.note.text);
      t.className = this.note.tone === "ok" ? s.termNoteOk : s.termNoteError;
      this.noteEl.appendChild(t);
      const retry = this.note.retry;
      if (retry) this.noteEl.appendChild(button({ label: copyText("terminal.input.retry"), kind: "ghost", size: "compact", onClick: () => void this.send(retry) }));
    }
    if (this.body.firstChild !== this.head) this.body.replaceChildren(this.head, this.bar, this.screen, this.inputArea);
  }
}
