/**
 * 底部抽屉的「终端」页：当前标签页那个会话所在终端的一屏（这一版是快照）＋ 一行输入 ＋ 几颗常用键。跟着当前标签页走。
 *
 * - 找终端：问那台 `terminals-list`，按会话 ID 认出那一行；没有那一行 ⇒ 已结束的写「已结束 · 无终端」；活着、不在 tmux 里的（容器事实 none）
 *   写「不在 tmux 里」（↗ 真能用时给「切到终端」）；在 tmux 里却认不出的写「非 cc-monitor 启动」。
 * - 画面：带颜色（`terminal-screen.ts` 照后端给的颜色段画）。开到这一页 / 切到这个标签页时抓一次，同时订那台的实时画面（`terminal-follow.ts`）：
 *   订上了 ⇒ 头上「● 实时」、画面跟着变、送完不再重抓；那台只能快照 ⇒ 多一枚「仅快照」，照旧送完 0.5 · 1.5 · 3 秒各再抓一次、其余时候点「重新看」；
 *   实时断了 ⇒ 画面留着变淡、头上退回「画面几点 ＋ 重新看」、头下一条原因 ＋［重新接上］（不自己重连）。收起 / 换页 / 切标签页即退订。开着不轮询。
 * - 往上翻了不拽回：底部出「回到最新」。
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
import { renderScreen, screenPre } from "./terminal-screen";
import { startFollow, type Follow, type FollowEvents, type FollowStop } from "./terminal-follow";
import { listTerminals, previewShot, sendToTerminal, type TerminalRow, type TerminalSend, type TerminalSent, type TerminalShot } from "./terminal-reads";
import type { Origin } from "./ipc/origin";
import { button, setBusy, setButtonLabel, setDisabled } from "./kit/button";
import { banner } from "./kit/banner";
import { copyDetailButton, detailOf } from "./kit/detail";
import { emptyState } from "./kit/empty";
import { statusDot, setDot } from "./kit/status-dot";
import { spinner } from "./kit/progress";
import s from "./terminal-page.module.css";

/** 终端页要的几条读写（缺省走通道；测试换成假的）。 */
export interface TerminalReads {
  list(origin: Origin, label: string): Promise<TerminalRow[]>;
  shot(origin: Origin, terminal: string, label: string): Promise<TerminalShot>;
  send(origin: Origin, terminal: string, what: TerminalSend, seen: string | null, label: string): Promise<TerminalSent>;
  /** 订实时画面。 */
  follow(origin: Origin, terminal: string, events: FollowEvents): Follow;
}

export const CHANNEL_READS: TerminalReads = {
  list: listTerminals,
  shot: (origin, terminal, label) => previewShot(origin, { terminal }, label),
  send: sendToTerminal,
  follow: (origin, terminal, events) => startFollow(origin, terminal, events),
};

/** 实时那一格：没订 · 正在接 · 实时中 · 停了（那一句 ＋ 是不是那台断开）· 那台只能快照（那一句）。 */
type Live = { at: "off" } | { at: "joining" } | { at: "on" } | { at: "stopped"; said: string; offline: boolean } | { at: "snapOnly"; said: string };

/** 停了的那一格 ⇒ 实时那一格。 */
function liveOf(why: FollowStop): Live {
  return why.kind === "snapshotOnly" ? { at: "snapOnly", said: why.said } : { at: "stopped", said: why.said, offline: why.offline };
}

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

/** 送字那一步的结局一句：送到了（一会儿就走）· 没送成（可带重试）。 */
type Note = { text: string; tone: "ok" | "error"; retry?: TerminalSend; detail?: string };

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
  /** 画面上此刻画着的是哪一张（同一张不重画）。 */
  private painted: TerminalShot | null = null;
  /** 名单那一步的结局：还在问 · 认出来了 · 名单里没有 · 问不到（那一句）。 */
  private phase: "loading" | "ready" | "none" | "error" = "loading";
  private error = "";
  /** 那条错误条的复制详情（出错那一端写好）；没有 ⇒ 空。 */
  private errorDetail = "";
  /** 抓屏那一步出的错（画面留着、降不透明度）。 */
  private shotError: string | null = null;
  private shotDetail = "";
  /** 送字那一步的结局一句（`null` ＝ 没有）。 */
  private note: Note | null = null;
  private sending = false;
  /** 没送的字，按会话各记一份。 */
  private readonly drafts = new Map<string, string>();
  /** 每一次名单 / 抓屏的代次：慢的那次回来不许盖掉后发的那次、不许落到切走之后的会话上。 */
  private seq = 0;
  private timers: ReturnType<typeof setTimeout>[] = [];
  /** 实时画面那一格与它的订阅。 */
  private live: Live = { at: "off" };
  private follow: Follow | null = null;
  private noteTimer: ReturnType<typeof setTimeout> | null = null;

  private readonly head: HTMLElement;
  private readonly dot: HTMLSpanElement;
  private readonly title: HTMLElement;
  private readonly where: HTMLElement;
  private readonly tag: HTMLElement;
  /** 连着几个终端窗口 / 后台。 */
  private readonly tag2: HTMLElement;
  private readonly at: HTMLElement;
  /** 「画面几点 ＋ 重新看」那一组（实时中整组收起）。 */
  private readonly snapGroup: HTMLElement;
  /** 「● 实时」/「接入实时」。 */
  private readonly liveTag: HTMLElement;
  /** 「仅快照」（悬停说为什么）。 */
  private readonly snapTag: HTMLElement;
  /** 画面那一格外面的那一层（「回到最新」浮在它右下）。 */
  private readonly screenWrap: HTMLElement;
  /** 「回到最新」外面那一格（定位 · 收起都在它身上：kit 按钮自带 display，`hidden` 切不动它）。 */
  private readonly followBox: HTMLElement;
  private readonly frontBtn: HTMLButtonElement;
  private readonly bar: HTMLElement;
  private readonly screen: HTMLElement;
  private readonly pre: HTMLPreElement;
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
    this.liveTag = el("span");
    this.liveTag.className = s.termLive;
    this.liveTag.title = copyText("terminal.head.liveHint");
    this.snapTag = el("span", copyText("terminal.head.snapshotOnly"));
    this.snapTag.className = s.termTag;
    const sp = el("span");
    sp.className = s.termSp;
    this.frontBtn = button({ label: copyText("terminal.head.front"), kind: "ghost", size: "compact", icon: "front", onClick: () => this.sid !== null && this.host.front(this.sid) });
    this.snapGroup = el("span");
    this.snapGroup.className = s.termSnap;
    this.snapGroup.append(this.at, again);
    this.head.append(this.dot, this.title, this.where, this.tag, this.tag2, this.liveTag, this.snapGroup, this.snapTag, sp, this.frontBtn);
    this.bar = el("div");
    this.bar.className = s.termBar;
    // 画面：外面一格管版位与滚动（本页的类），里面那块 `pre` 只挂画面的类（`terminal-screen.ts`）。
    this.screen = el("div");
    this.screen.className = s.termScreen;
    this.screen.tabIndex = 0;
    this.pre = screenPre();
    this.screen.appendChild(this.pre);
    this.screen.addEventListener("scroll", () => this.syncFollowBtn());
    this.followBox = el("div");
    this.followBox.className = s.termFollow;
    this.followBox.hidden = true;
    this.followBox.appendChild(button({ label: copyText("terminal.screen.follow"), size: "compact", icon: "arrowDown", onClick: () => this.toLatest() }));
    this.screenWrap = el("div");
    this.screenWrap.className = s.termScreenWrap;
    this.screenWrap.append(this.screen, this.followBox);

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

  /** 抽屉开到 / 离开这一页（宿主调）。开到的那一刻抓一次、订实时；离开即退订。 */
  setVisible(on: boolean): void {
    if (on === this.visible) return;
    this.visible = on;
    if (on) void this.refresh();
    else {
      this.stopTimers();
      this.stopLive();
    }
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
    this.stopLive();
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
        this.stopLive();
        this.row = null;
        this.phase = "none";
        this.paint();
        return;
      }
      if (hits.length > 1) {
        this.row = null;
        this.phase = "error";
        this.error = copyText("terminal.why.ambiguous");
        this.errorDetail = "";
        this.paint();
        return;
      }
      if (this.row?.terminal !== hits[0].terminal) this.stopLive();
      this.row = hits[0];
      this.phase = "ready";
    } catch (e) {
      if (mine !== this.seq) return;
      this.phase = "error";
      this.error = saidOfControl(e);
      this.errorDetail = detailOf(e);
      this.paint();
      return;
    }
    // 抓屏失败（没有 tmux server 之类）⇒ 不订实时：订也会被同样拒，叠出第二条警告；［刷新］抓到了再订。
    const shot = await this.capture(mine);
    if (shot && mine === this.seq && this.visible && this.live.at === "off") this.startLive();
  }

  /** 订这个终端的实时画面（已经订着 ⇒ 先退）。 */
  private startLive(): void {
    this.stopLive();
    const row = this.row;
    if (row === null || this.origin === undefined || !this.visible) return;
    this.live = { at: "joining" };
    let f: Follow | null = null;
    f = this.reads.follow(this.origin, row.terminal, {
      screen: (shot) => {
        if (this.follow !== f && f !== null) return;
        this.live = { at: "on" };
        this.shotError = null;
        this.applyShot(shot);
      },
      stop: (why) => {
        if (this.follow !== f && f !== null) return;
        this.follow = null;
        this.live = liveOf(why);
        this.paint();
      },
    });
    // 订阅这一问是同步交回句柄的；交回之前就停了（那一格已落 `stopped` / `snapOnly`）⇒ 不记这个句柄。
    const now = this.live as Live;
    if (now.at === "joining" || now.at === "on") this.follow = f;
    this.paint();
  }

  /** 退订（没订着 ⇒ 什么都不做）。 */
  private stopLive(): void {
    this.follow?.stop();
    this.follow = null;
    this.live = { at: "off" };
  }

  /** 实时中 ⇒ 送完不用再抓（画面自己会变）。 */
  private get isLive(): boolean {
    return this.live.at === "on" || this.live.at === "joining";
  }

  /** 只重抓一屏（「重新看」· 送完之后那几拍）。还没认出终端 ⇒ 整个重来。 */
  async recapture(): Promise<void> {
    if (this.row === null) return this.refresh();
    await this.capture(++this.seq);
  }

  /** 抓一屏；抓到了（或实时那一帧更新）⇒ `true`，失败 ⇒ `false`。 */
  private async capture(mine: number): Promise<boolean> {
    const tab = this.host.active();
    const row = this.row;
    if (!tab || row === null || this.origin === undefined) return false;
    try {
      const shot = await this.reads.shot(this.origin, row.terminal, fullTitle(tab));
      if (mine !== this.seq) return true;
      if (this.live.at === "on") return true; // 实时那一帧比这一张新
      this.shotError = null;
      this.applyShot(shot);
      return true;
    } catch (e) {
      if (mine !== this.seq) return false;
      this.shotError = saidOfControl(e);
      this.shotDetail = detailOf(e);
      this.paint();
      return false;
    }
  }

  /** 换上一屏：原来贴着底就接着贴底；往上翻着就不动（底部出「回到最新」）。 */
  private applyShot(shot: TerminalShot): void {
    const stick = this.shot === null || this.atBottom();
    this.shot = shot;
    this.paint();
    if (stick) this.screen.scrollTop = this.screen.scrollHeight;
    this.syncFollowBtn();
  }

  private atBottom(): boolean {
    return this.screen.scrollTop + this.screen.clientHeight >= this.screen.scrollHeight - 4;
  }

  private syncFollowBtn(): void {
    this.followBox.hidden = this.shot === null || this.atBottom();
  }

  private toLatest(): void {
    this.screen.scrollTop = this.screen.scrollHeight;
    this.syncFollowBtn();
  }

  private stopTimers(): void {
    for (const t of this.timers) clearTimeout(t);
    this.timers = [];
  }

  private schedule(): void {
    this.stopTimers();
    // 调度：一次性 —— 送字送键之后 0.5 · 1.5 · 3 秒各抓一屏；换会话 / 收起即清，不轮询
    for (const ms of RECAPTURE_AFTER_MS) this.timers.push(setTimeout(() => void this.recapture(), ms));
  }

  private showNote(note: Note | null): void {
    this.note = note;
    if (this.noteTimer !== null) clearTimeout(this.noteTimer);
    this.noteTimer = null;
    if (note?.tone === "ok") {
      // 调度：一次性 —— 「已送达」2 秒后收
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
    let detail = "";
    try {
      sent = await this.reads.send(this.origin, row.terminal, what, this.shot?.screen ?? null, fullTitle(tab));
    } catch (e) {
      failed = saidOfControl(e);
      detail = detailOf(e);
    }
    if (sid !== this.sid) return false;
    this.sending = false;
    if (failed !== null) {
      this.showNote({ text: failed, tone: "error", retry: what, detail });
      return false;
    }
    if (sent?.result === "delivered") {
      this.showNote({ text: copyText("terminal.input.delivered"), tone: "ok" });
      if (!this.isLive) this.schedule();
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
    this.showNote({ text: copyText("terminal.input.failed", { why: sent?.result === "refused" ? sent.said : copyText("terminal.why.other") }), tone: "error", retry: what });
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
      // 活着、容器事实是「不在任何 tmux 里」（后端 `session_added.container` = none）：照实说，不当成别人起的。
      const outside = tab.state.liveness === "live" && tab.state.recoverability === "resumable";
      const sid = this.sid;
      this.body.replaceChildren(
        ended
          ? emptyState({ icon: "terminal", text: copyText("terminal.empty.ended", { state: copyText("sessionState.ended.name") }), hint: copyText("terminal.empty.endedHint") })
          : outside
            ? emptyState({
                icon: "terminal",
                text: copyText("terminal.empty.notInTmux"),
                hint: copyText("terminal.empty.notInTmuxHint"),
                ...(terminalFrontAvailable() ? { action: button({ label: copyText("terminal.head.front"), size: "compact", icon: "front", onClick: () => this.host.front(sid) }) } : {}),
              })
            : emptyState({ icon: "terminal", text: copyText("terminal.empty.notOurs"), hint: copyText("terminal.empty.notOursHint") }),
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
      this.body.replaceChildren(
        banner("error", this.error, [button({ label: copyText("terminal.state.refresh"), size: "compact", onClick: () => void this.refresh() })], this.errorDetail),
      );
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
    this.at.textContent = this.shot ? copyText("terminal.head.snapAt", { time: this.shot.atText }) : "";
    const live = this.isLive;
    this.snapGroup.hidden = live;
    this.liveTag.hidden = !live;
    this.liveTag.dataset.state = this.live.at;
    this.liveTag.textContent = this.live.at === "on" ? copyText("terminal.head.live") : copyText("terminal.head.joining");
    this.snapTag.hidden = this.live.at !== "snapOnly";
    if (this.live.at === "snapOnly") this.snapTag.title = this.live.said;
    this.frontBtn.style.display = terminalFrontAvailable() && hasTerminal(tab.state) ? "" : "none";

    const bars: HTMLElement[] = [];
    if (row.programExited) {
      const line = el("div", copyText("terminal.bar.programExited", { state: copyText("sessionState.reconnectable.name") }));
      line.className = s.termBarLine;
      bars.push(line);
    }
    if (this.live.at === "stopped") {
      bars.push(banner("warn", copyText("terminal.bar.liveStopped", { why: this.live.said }), [button({ label: copyText("terminal.bar.liveRetry"), size: "compact", onClick: () => this.startLive() })]));
    }
    if (this.shotError !== null)
      bars.push(banner("warn", this.shotError, [button({ label: copyText("terminal.state.refresh"), size: "compact", onClick: () => void this.refresh() })], this.shotDetail));
    this.bar.replaceChildren(...bars);
    if (this.painted !== this.shot) {
      renderScreen(this.pre, this.shot?.lines ?? []);
      this.painted = this.shot;
    }
    this.screen.dataset.stale = String(this.shotError !== null || this.live.at === "stopped");

    this.to.textContent = copyText("terminal.input.to", { title: fullTitle(tab), machine: machineOf(tab) });
    this.owner.textContent = row.clients > 0 && row.input === "shared" ? copyText("terminal.input.shared", { n: row.clients }) : row.clients === 0 ? copyText("terminal.input.nobody") : "";
    const no = row.inputNo ?? (this.live.at === "stopped" && this.live.offline ? this.live.said : null);
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
      // 那一端写了详情 ⇒ 跟［复制详情］（与同一个原因的那条黄条同一颗）。
      const copy = copyDetailButton(this.note.text, this.note.detail ?? "");
      if (copy) this.noteEl.appendChild(copy);
    }
    if (this.body.firstChild !== this.head) this.body.replaceChildren(this.head, this.bar, this.screenWrap, this.inputArea);
  }
}
