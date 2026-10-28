/**
 * 底部抽屉的「终端」页：当前标签页那个会话所在终端的一屏（这一版是快照）＋ 一行输入 ＋ 几颗常用键。跟着当前标签页走；
 * 占位标签页显着时看它的那个终端（没报到的会话与已有会话看画面是同一页）。
 *
 * - 找终端：问那台 `terminals-list`，按会话 ID 认出那一行（占位那一形按终端句柄认）；没有那一行 ⇒ 已结束的写「已结束 · 无终端」；活着的（不在任何终端里 ·
 *   在终端里却认不出，对用户是同一件事）写「非 cc-monitor 起的」（↗ 真能用时给「切到终端」）。
 * - 画面：带颜色（`terminal-screen.ts` 照后端给的颜色段画）。开到这一页 / 切到这个标签页时抓一次，同时订那台的实时画面（`terminal-follow.ts`）：
 *   订上了 ⇒ 头上「● 实时」、画面跟着变、送完不再重抓；那台只能快照 ⇒ 多一枚「仅快照」，照旧送完 0.5 · 1.5 · 3 秒各再抓一次、其余时候点「重新看」；
 *   实时断了 ⇒ 画面留着变淡、头上退回「画面几点 ＋ 重新看」、头下一条原因 ＋［重新接上］（流断了不自己重连）；断在「那台断开」的，
 *   那台自己重连回来（机器状态推「已连上」，宿主转给 {@link TerminalPage.machineUp}）⇒ 自动重新订上、输入框跟着恢复。收起 / 换页 / 切标签页即退订。开着不轮询。
 * - 往上翻了不拽回：底部出「回到最新」。
 * - 能不能送、输入会不会和别的终端窗口混在一起：只读名单里后端给的 `can.input` · `clients` · `input`，界面不判。
 * - 托管那一形（输入只收持有者的）：「送往 …」那一行右边写后端那一句（输入在此处 · 在手机 · 在终端窗口 …）＋［接管输入］（不是这一端持有）/［交还］
 *   （`terminal-take-input` · `terminal-release-input`，回的那一格就地换上；实时流每一帧也带此刻的持有者，就地换）。不是这一端持有 ⇒ 按键一排 · Ctrl+回车灰着，
 *   按键尾巴写那一句 ＋「接管后可按键」、输入框占位说直接打字需接管；回车（说一句）照旧投进队。tmux 那一形不出这两颗。
 * - 送字带上看到的那一屏的指纹：画面已经变了 ⇒ 后端不送，这里重抓给人看。不知道送没送到 ⇒ 照实说、不重发。
 * - 输入框：回车说一句（投进输入队，见下）· Shift+回车换行 · Ctrl+回车只往终端里打字、不补回车（`terminal-input`）；组字时回车归输入法；
 *   Esc 只把焦点还给消息流（不清字、不收抽屉）。没送的字按会话各记一份，切回来还在。常用键一律往终端里送（按键不是说一句）。
 * - 页头右边「切到终端」只在 ↗ 真能用时出（同会话头那一道）。
 * - 附件（`attach-row.ts` · `attach-draft.ts`）：输入框上方一排 chip，［＋］· 拖到面板上 · 粘贴进同一排；按会话各记一份草稿。
 *   附件没传完 / 有失败 ⇒［送出］灰、悬停说为什么。那台连上 ⇒ 断着的接着传。
 * - 输入行送出的那一句（带不带附件）＝ 说一句：投进那条会话的输入队（`session-queue.ts`，编号随那一句交）；会话那台落盘回执到了 ⇒ 字与 chip 一起清；
 *   那台拒了 ⇒ 原样留着；没回执 ⇒ 待发（落本地存储，重开还在）在队那一排写「发送中」，那台连上照同一个编号再交。认不出会话（占位那一形没挂会话）⇒ 往终端里打字。
 * - 队那一排（`queue-row.ts`）：会话那台推来的整份名单（`changed {session_queue}`，不各自记）＋ 这一端的待发；每条可撤（`session-queue-cancel`）。
 */
import { countText } from "./format";
import type { Tab } from "./tab-model";
import { copyText } from "./copy-table";
import { machineName, offlineSaid } from "./control-said";
import { canResume, hasTerminal, machineOf } from "./face-can";
import { terminalFrontAvailable } from "./terminal-front";
import { dotOf, fullTitle, stateWord } from "./session-face";
import { renderScreen, screenPre } from "./terminal-screen";
import { startFollow, type Follow, type FollowEvents, type FollowStop } from "./terminal-follow";
import {
  holderOf,
  listTerminals,
  previewShot,
  releaseInput,
  sendToTerminal,
  takeInput,
  type TerminalRow,
  type TerminalSend,
  type TerminalSent,
  type TerminalShot,
} from "./terminal-reads";
import type { TermInput } from "./generated/TermInput";
import { AttachDraft, channelIo, type AttachIo } from "./attach-draft";
import { AttachRow } from "./attach-row";
import {
  CHANNEL_CANCEL,
  CHANNEL_PUT,
  CHANNEL_READ,
  CHANNEL_WATCH,
  loadPending,
  outgoing,
  pendingKey,
  putOnce,
  savePending,
  type Outgoing,
  type PutEnd,
  type QueueCancelFn,
  type QueueMoved,
  type QueuePutFn,
  type QueueReadFn,
  type QueueWatchFn,
} from "./session-queue";
import { QueueRow } from "./queue-row";
import type { QueueList } from "./generated/QueueList";
import type { Rewind } from "./generated/Rewind";
import type { Origin } from "./ipc/origin";
import { el } from "./kit/el";
import { button, setBusy, setButtonLabel, setDisabled } from "./kit/button";
import { banner } from "./kit/banner";
import { copyDetailButton, detailOf, saidOf } from "./kit/detail";
import { emptyState } from "./kit/empty";
import { statusDot, setDot } from "./kit/status-dot";
import { spinner } from "./kit/progress";
import s from "./terminal-page.module.css";

/** 终端页要的几条读写（缺省走通道；测试换成假的）。 */
export interface TerminalReads {
  list(origin: Origin): Promise<TerminalRow[]>;
  shot(origin: Origin, terminal: string): Promise<TerminalShot>;
  send(origin: Origin, terminal: string, what: TerminalSend, seen: string | null): Promise<TerminalSent>;
  /** 订实时画面。 */
  follow(origin: Origin, terminal: string, events: FollowEvents): Follow;
}

export const CHANNEL_READS: TerminalReads = {
  list: listTerminals,
  shot: (origin, terminal) => previewShot(origin, { terminal }),
  send: sendToTerminal,
  follow: (origin, terminal, events) => startFollow(origin, terminal, events),
};

/** 托管终端的接管 · 交还（缺省走通道；测试换成假的）：回那一行此刻的输入那一格。 */
export interface InputHolds {
  take(origin: Origin, terminal: string): Promise<TermInput>;
  release(origin: Origin, terminal: string): Promise<TermInput>;
}

export const CHANNEL_HOLDS: InputHolds = { take: takeInput, release: releaseInput };

/** 说一句那一路要的几口（缺省走通道；测试换成假的）。 */
export interface AttachReads {
  /** 投一句进那条会话的输入队（`session-queue-put`）。 */
  put: QueuePutFn;
  /** 那台的附件那几口（判收不收 · 逐块传 · 提交 · 撤）。 */
  io(origin: Origin): AttachIo;
  /** 读那条会话此刻的名单（`session-queue-read`）。 */
  read: QueueReadFn;
  /** 撤队里一条（`session-queue-cancel`）。 */
  cancel: QueueCancelFn;
  /** 订那台的 `changed {session_queue}`。 */
  watch: QueueWatchFn;
}

export const CHANNEL_ATTACH: AttachReads = { put: CHANNEL_PUT, io: channelIo, read: CHANNEL_READ, cancel: CHANNEL_CANCEL, watch: CHANNEL_WATCH };

/** 实时那一格：没订 · 正在接 · 实时中 · 停了（那一句 ＋ 是不是那台断开）· 那台只能快照（那一句）。 */
type Live = { at: "off" } | { at: "joining" } | { at: "on" } | { at: "stopped"; said: string; offline: boolean } | { at: "snapOnly"; said: string };

/** 停了的那一格 ⇒ 实时那一格。 */
function liveOf(why: FollowStop): Live {
  return why.kind === "snapshotOnly" ? { at: "snapOnly", said: why.said } : { at: "stopped", said: why.said, offline: why.offline };
}

/** 看的是谁：当前标签页那个会话 · 占位标签页的那个终端（哪台 · 句柄 · tmux 会话名 · 标题）。 */
export type Watched = { kind: "tab"; tab: Tab } | { kind: "slot"; origin: Origin; terminal: string; name: string; title: string };

/** 按会话各记一份的那几样（没送的字）用的键：标签页 ⇒ 会话 ID；占位 ⇒ 那台 ＋ 句柄。 */
function keyOf(w: Watched): string {
  return w.kind === "tab" ? w.tab.sessionId : `\u0000${w.origin}\u0000${w.terminal}`;
}
const originOf = (w: Watched): Origin => (w.kind === "tab" ? w.tab.origin : w.origin);
const labelOf = (w: Watched): string => (w.kind === "tab" ? fullTitle(w.tab) : w.title);
const machineWord = (w: Watched): string => (w.kind === "tab" ? machineOf(w.tab) : machineName(w.origin));

export interface TerminalPageHost {
  active(): Watched | null;
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

/** 页内元素 id 的流水号（「送往 …」那一行给输入框当名字）。 */
let pageSeq = 0;

export class TerminalPage {
  readonly el: HTMLElement;
  private visible = false;
  /** 正在看的那一个（`keyOf`；`null` ＝ 没有当前标签页）。 */
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
  /** 接管 · 交还那一下在飞。 */
  private holding = false;
  /** 没送的字，按会话各记一份。 */
  private readonly drafts = new Map<string, string>();
  /** 没送的附件，按会话各记一份（键 ＝ 那台 ＋ 会话 ID）。 */
  private readonly attachDrafts = new Map<string, AttachDraft>();
  /** 交出去、会话那台还没回执的那几句，按会话各一份（键 ＝ 那台 ＋ 会话 ID，同附件草稿）：落本地存储，那台连上照同一个编号再交。 */
  private readonly waiting: Map<string, Outgoing[]> = loadPending();
  /** 此刻看着的那条会话的名单（会话那台那一份；还没读到 ⇒ `null`）与它是哪一条（键同上）。 */
  private queue: QueueList | null = null;
  private queueOf: string | null = null;
  /** 这一页交过的编号（本地存储里挂回来的、还没交过的那几句开页时交一次）。 */
  private readonly tried = new Set<string>();
  /** 订着哪一台的 `changed {session_queue}` 与退订。 */
  private watching: { origin: Origin; stop: () => void } | null = null;
  private readonly queueRow: QueueRow;
  private readonly attach: AttachRow;
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
  /** ［接管输入］/［交还］（托管那一形才出）。 */
  private readonly holdBtn: HTMLButtonElement;
  private readonly box: HTMLTextAreaElement;
  private readonly sendBtn: HTMLButtonElement;
  private readonly keyBtns: HTMLButtonElement[] = [];
  /** 按键那一排的尾巴：别端持有时写那一句 ＋「接管后可按键」。 */
  private readonly keysWhy: HTMLElement;
  private readonly noteEl: HTMLElement;
  private readonly body: HTMLElement;

  constructor(
    private readonly host: TerminalPageHost,
    private readonly reads: TerminalReads = CHANNEL_READS,
    private readonly attachReads: AttachReads = CHANNEL_ATTACH,
    private readonly holds: InputHolds = CHANNEL_HOLDS,
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
    this.snapTag = el("span", "", copyText("terminal.head.snapshotOnly"));
    this.snapTag.className = s.termTag;
    const sp = el("span");
    sp.className = s.termSp;
    this.frontBtn = button({ label: copyText("terminal.head.front"), kind: "ghost", size: "compact", icon: "front", onClick: () => this.frontActive() });
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
    this.holdBtn = button({ label: copyText("terminal.input.take"), kind: "ghost", size: "compact", onClick: () => void this.hold() });
    toRow.append(this.to, toSp, this.owner, this.holdBtn);
    const boxRow = el("div");
    boxRow.className = s.termBoxRow;
    this.box = el("textarea");
    this.box.className = s.termBox;
    this.box.rows = 1;
    // 读屏名 ＝ 头上「送往 …」那一行；占位与悬停只当说明（否则同一句既当名字又当说明，念两遍）。
    this.to.id = `term-to-${++pageSeq}`;
    this.box.setAttribute("aria-labelledby", this.to.id);
    this.box.addEventListener("keydown", (ev) => this.onBoxKey(ev));
    this.box.addEventListener("input", () => this.onBoxInput());
    this.box.addEventListener("paste", (ev) => {
      if (this.attach.paste(ev.clipboardData)) ev.preventDefault();
    });
    this.sendBtn = button({ label: copyText("terminal.input.send"), size: "compact", onClick: () => this.sendBox(true) });
    this.attach = new AttachRow({
      draft: () => this.attachDraft(),
      target: () => {
        const w = this.host.active();
        return { title: w ? labelOf(w) : "", machine: w ? machineWord(w) : "" };
      },
      offline: () => this.offlineLine(),
      blocked: () => this.inputNo(),
    });
    boxRow.append(this.attach.plus, this.box, this.sendBtn);
    const keys = el("div");
    keys.className = s.termKeys;
    const keysLabel = el("span", "", copyText("terminal.input.keys"));
    keysLabel.className = s.termKeysLabel;
    keys.appendChild(keysLabel);
    for (const k of KEYS) {
      const b = button({ label: k.label, size: "compact", hint: copyText("terminal.input.keyHint", { key: k.label }), onClick: () => void this.send(k.what) });
      this.keyBtns.push(b);
      keys.appendChild(b);
    }
    this.keysWhy = el("span");
    this.keysWhy.className = s.termKeysLabel;
    keys.appendChild(this.keysWhy);
    this.noteEl = el("div");
    this.noteEl.className = s.termNote;
    this.queueRow = new QueueRow({ cancel: (id) => void this.cancelQueued(id), cancelPending: (id) => this.cancelWaiting(id) });
    this.inputArea.append(toRow, this.queueRow.el, this.attach.row, boxRow, keys, this.noteEl);
    this.body = el("div");
    this.body.className = s.termBody;
    this.el.append(this.body);
    this.attach.dropOn(this.el);
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
      this.unwatch();
    }
  }

  /** ↗ 切到当前标签页那个会话的终端窗口（占位那一形没有）。 */
  private frontActive(): void {
    const w = this.host.active();
    if (w?.kind === "tab") this.host.front(w.tab.sessionId);
  }

  /** 当前标签页换了 / 它的状态变了：换了会话 ⇒ 字按会话存取、开着就重新找终端抓一屏；同一个会话 ⇒ 只重画页头。 */
  sessionChanged(): void {
    const w = this.host.active();
    const sid = w ? keyOf(w) : null;
    if (sid === this.sid) {
      this.paint();
      return;
    }
    if (this.sid !== null) this.drafts.set(this.sid, this.box.value);
    this.stopTimers();
    this.stopLive();
    this.seq++;
    this.sid = sid;
    this.origin = w ? originOf(w) : undefined;
    this.row = null;
    this.shot = null;
    this.shotError = null;
    this.note = null;
    this.sending = false;
    this.phase = "loading";
    this.queue = null;
    this.queueOf = null;
    this.box.value = sid === null ? "" : (this.drafts.get(sid) ?? "");
    this.onBoxInput();
    this.paint();
    if (this.visible) void this.refresh();
  }

  /** 一台机器连上了（宿主转来机器状态推送里的「已连上」）：实时正停在「这台断开」⇒ 重找终端、重新订上（输入框随之恢复）；别的情形不动。 */
  machineUp(origin: string): void {
    for (const [k, d] of this.attachDrafts) if (k.startsWith(`${origin}\u0000`)) d.resume();
    for (const [k, list] of this.waiting) if (k.startsWith(`${origin}\u0000`)) for (const o of list) void this.putWaiting(origin as Origin, k, o);
    if (this.visible && this.origin === origin) void this.readQueue();
    if (!this.visible || this.origin !== origin || this.live.at !== "stopped" || !this.live.offline) return;
    this.live = { at: "off" };
    void this.refresh();
  }

  /** 找终端、抓一屏（名单也重问：终端可能刚起 / 刚没）。 */
  async refresh(): Promise<void> {
    const w = this.host.active();
    if (!w || keyOf(w) !== this.sid || this.origin === undefined) {
      this.paint();
      return;
    }
    const mine = ++this.seq;
    const origin = this.origin;
    if (this.row === null) {
      this.phase = "loading";
      this.paint();
    }
    try {
      const rows = await this.reads.list(origin);
      if (mine !== this.seq) return;
      const hits = rows.filter((r) => (w.kind === "tab" ? r.sid === w.tab.sessionId : r.terminal === w.terminal));
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
      this.watch(origin);
      void this.readQueue();
      // 上次开着时没等到回执的那几句（本地存储里的）：这一页还没交过 ⇒ 照同一个编号交一次。
      const to = this.sayingTo();
      if (to !== null) for (const o of this.waiting.get(pendingKey(origin, to)) ?? []) if (!this.tried.has(o.id)) void this.putWaiting(origin, pendingKey(origin, to), o);
    } catch (e) {
      if (mine !== this.seq) return;
      this.phase = "error";
      this.error = saidOf(e);
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
      screen: (shot, holder) => {
        if (this.follow !== f && f !== null) return;
        this.live = { at: "on" };
        this.shotError = null;
        // 托管那一形：帧里带此刻的持有者（持有者变了画面没变也来一帧）⇒「送往 …」那一行 · 按键就地换。
        if (holder !== undefined && this.row?.terminal === row.terminal) this.row = { ...this.row, holder };
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
    const w = this.host.active();
    const row = this.row;
    if (!w || row === null || this.origin === undefined) return false;
    try {
      const shot = await this.reads.shot(this.origin, row.terminal);
      if (mine !== this.seq) return true;
      if (this.live.at === "on") return true; // 实时那一帧比这一张新
      this.shotError = null;
      this.applyShot(shot);
      return true;
    } catch (e) {
      if (mine !== this.seq) return false;
      this.shotError = saidOf(e);
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

  /** 送一段字或一颗键。能不能送由名单里后端那一格说了算（托管那一形不是这一端持有 ⇒ 不送，见 [heldNo]）；送完按回话说一句、该重抓就重抓。 */
  async send(what: TerminalSend): Promise<boolean> {
    const row = this.row;
    if (row === null || this.heldNo() !== null) return false;
    const seen = this.shot?.screen ?? null;
    const sent = await this.deliver((origin) => this.reads.send(origin, row.terminal, what, seen), what);
    return sent?.result === "delivered";
  }

  /** 托管那一形、不是这一端持有 ⇒ 按键 · Ctrl+回车不送的那一句（名单 / 实时流那一句 ＋「接管后可按键」）；别的 ⇒ `null`。 */
  private heldNo(): string | null {
    const h = this.row?.holder;
    return h !== undefined && !h.mine ? [h.said, copyText("terminal.input.afterTake")].join(copyText("kit.text.sep")) : null;
  }

  /** ［接管输入］/［交还］：交那台，回的那一格就地换上（不重读名单）。没做成 ⇒ 说那台写好的那一句。 */
  private async hold(): Promise<void> {
    const row = this.row;
    const origin = this.origin;
    const sid = this.sid;
    if (row === null || row.holder === undefined || origin === undefined || this.holding) return;
    const mine = row.holder.mine;
    this.holding = true;
    this.paint();
    try {
      const now = await (mine ? this.holds.release(origin, row.terminal) : this.holds.take(origin, row.terminal));
      if (sid === this.sid && this.row?.terminal === row.terminal) this.row = { ...this.row, holder: holderOf(now) };
    } catch (e) {
      if (sid === this.sid) this.showNote({ text: saidOf(e), tone: "error", detail: detailOf(e) });
    }
    this.holding = false;
    this.paint();
  }

  /**
   * 送一下（送字送键 · 带附件说一句同一条路）并照回话说一句：送到了（一会儿就走）· 不知道 · 被拒（下一步照后端判好的 `acts` 摆：
   * 给不给［重试］· 重抓一屏 · 重读名单）。没送成 · 送不了 · 送的时候切走了 ⇒ `null`。
   */
  private async deliver(go: (origin: Origin) => Promise<TerminalSent>, retry: TerminalSend | null): Promise<TerminalSent | null> {
    const w = this.host.active();
    const row = this.row;
    const origin = this.origin;
    if (!w || row === null || origin === undefined || this.sending || row.inputNo !== null) return null;
    const sid = this.sid;
    this.sending = true;
    this.paint();
    let sent: TerminalSent | null = null;
    let failed: string | null = null;
    let detail = "";
    try {
      sent = await go(origin);
    } catch (e) {
      failed = saidOf(e);
      detail = detailOf(e);
    }
    if (sid !== this.sid) return null;
    this.sending = false;
    const again = retry === null ? {} : { retry };
    if (failed !== null || sent === null) {
      this.showNote({ text: failed ?? copyText("reason.code.refused"), tone: "error", ...again, detail });
      return null;
    }
    if (sent.result === "delivered") {
      this.showNote({ text: copyText("terminal.input.delivered"), tone: "ok" });
      if (!this.isLive) this.schedule();
      return sent;
    }
    const text = sent.result === "unsure" ? copyText("terminal.input.unsure", { machine: machineName(origin) }) : copyText("terminal.input.failed", { why: sent.said });
    this.showNote({ text, tone: "error", ...(sent.acts.includes("retry") ? again : {}) });
    if (sent.acts.includes("recapture")) void this.recapture();
    if (sent.acts.includes("relist")) void this.refresh();
    return sent;
  }

  /** 此刻那条会话的附件草稿（没有 ⇒ 现建）；认不出会话（占位 · 名单里没挂会话）⇒ `null`。 */
  private attachDraft(): AttachDraft | null {
    const w = this.host.active();
    const sid = this.row?.sid ?? (w?.kind === "tab" ? w.tab.sessionId : null);
    if (!w || this.origin === undefined || sid === null || this.phase !== "ready") return null;
    const key = `${this.origin}\u0000${sid}`;
    let d = this.attachDrafts.get(key);
    if (d === undefined) {
      const mine = d = new AttachDraft(sid, this.attachReads.io(this.origin), () => {
        if (this.attachDraft() === mine) this.paint();
      });
      this.attachDrafts.set(key, d);
    }
    return d;
  }

  /** 那台断着时的那一句（连接层写好的离线那一句）：chip 上跟它，［送出］悬停也是它。 */
  private offlineLine(): string {
    return this.origin === undefined ? "" : (offlineSaid(this.origin) ?? machineName(this.origin));
  }

  /** 输入此刻送不了的那一句（名单里后端那一格 · 实时停在那台断开）；送得了 ⇒ `null`。 */
  private inputNo(): string | null {
    const row = this.row;
    if (row === null) return copyText("terminal.state.loading");
    return row.inputNo ?? (this.live.at === "stopped" && this.live.offline ? this.live.said : null);
  }

  /**
   * 说一句：投进那条会话的输入队。那一句先成待发（这一端铸编号，落本地存储），交出去；会话那台落了盘回执到了 ⇒ 字与 chip 一起清、
   * 照那台写的那一句说一下；那台拒了 ⇒ 原样留着、说那台那一句；没回执（断线 · 期限到了）⇒ 留在待发（队那一排「发送中」），那台连上照同一个编号再交。
   * 待发里已有同一句（没回执又点了一次送出）⇒ 照它的编号再交，不另起一条。
   */
  private async say(origin: Origin, sid: string, text: string, ids: string[]): Promise<void> {
    if (this.sending) return;
    const key = pendingKey(origin, sid);
    const list = this.waiting.get(key) ?? [];
    let o = list.find((x) => x.text === text);
    if (o === undefined) {
      o = outgoing(sid, text, ids);
      this.waiting.set(key, [...list, o]);
      savePending(this.waiting);
    }
    this.sending = true;
    this.paint();
    await this.putWaiting(origin, key, o);
  }

  /**
   * 编辑已发的那一句（`said-edit.ts` 确认那一下）：带 `rewind` 进同一份待发、交一次，回这一次的结局。没回执 ⇒ 留在待发（队那一排「发送中」），
   * 那台连上照同一个编号再交；待发里已有同一句同一个回退 ⇒ 照它的编号再交。输入框与附件草稿不动（那是另一句）。
   */
  async sayEdited(origin: Origin, sid: string, text: string, rewind: Rewind): Promise<PutEnd> {
    const key = pendingKey(origin, sid);
    const list = this.waiting.get(key) ?? [];
    const same = (x: Outgoing): boolean => x.text === text && x.rewind?.to === rewind.to && x.rewind.how === rewind.how && x.rewind.when === rewind.when;
    let o = list.find(same);
    if (o === undefined) {
      o = outgoing(sid, text, [], rewind);
      this.waiting.set(key, [...list, o]);
      savePending(this.waiting);
    }
    this.paint();
    return (await this.putWaiting(origin, key, o)) ?? { at: "none", said: copyText("terminal.queue.sending") };
  }

  /** 待发里拿掉那一句（落本地存储）。 */
  private dropWaiting(key: string, o: Outgoing): void {
    const left = (this.waiting.get(key) ?? []).filter((x) => x !== o);
    if (left.length > 0) this.waiting.set(key, left);
    else this.waiting.delete(key);
    savePending(this.waiting);
  }

  /** 撤待发里那一句：本地拿掉；万一已经交到那台了，照编号请它撤（送过的撤不了，那台说了算）。 */
  private cancelWaiting(id: string): void {
    const origin = this.origin;
    const sid = this.sayingTo();
    if (origin === undefined || sid === null) return;
    const key = pendingKey(origin, sid);
    const o = this.waiting.get(key)?.find((x) => x.id === id);
    if (o === undefined) return;
    this.dropWaiting(key, o);
    this.sending = false;
    this.paint();
    void this.attachReads.cancel(origin, sid, id).then(
      (r) => this.gotQueue(key, r.queue),
      () => {},
    );
  }

  /** 撤名单里那一条（照编号请会话那台撤），照回的整份名单画；没成 ⇒ 说那台那一句。 */
  private async cancelQueued(id: string): Promise<void> {
    const origin = this.origin;
    const sid = this.sayingTo();
    if (origin === undefined || sid === null) return;
    try {
      const r = await this.attachReads.cancel(origin, sid, id);
      this.gotQueue(pendingKey(origin, sid), r.queue);
    } catch (e) {
      this.showNote({ text: saidOf(e), tone: "error", detail: detailOf(e) });
    }
  }

  /** 问一次此刻那条会话的名单；问不到 ⇒ 名单留着上一份（推送或下一次重问补上）。 */
  private async readQueue(): Promise<void> {
    const origin = this.origin;
    const sid = this.sayingTo();
    if (origin === undefined || sid === null) return;
    try {
      this.gotQueue(pendingKey(origin, sid), await this.attachReads.read(origin, sid));
    } catch {
      // 断着 · 那台没有这一条：照旧画手上那一份。
    }
  }

  /** 会话那台那一份名单到了（问回来的 · 推来的 · 撤完回的）：是此刻看着的那一条才换。 */
  private gotQueue(key: string, q: QueueList): void {
    if (this.origin === undefined || this.sayingTo() === null || pendingKey(this.origin, this.sayingTo() as string) !== key) return;
    this.queue = q;
    this.queueOf = key;
    this.paint();
  }

  /** 订那台的 `changed {session_queue}`（已订着那一台 ⇒ 不动）。 */
  private watch(origin: Origin): void {
    if (this.watching?.origin === origin) return;
    this.unwatch();
    this.watching = { origin, stop: this.attachReads.watch(origin, (m) => this.queueMoved(origin, m)) };
  }

  private unwatch(): void {
    this.watching?.stop();
    this.watching = null;
  }

  /** 那台推来一批：看着的那一条带了名单 ⇒ 换；没带 · 可能漏了 ⇒ 重问。 */
  private queueMoved(origin: Origin, m: QueueMoved): void {
    const sid = this.sayingTo();
    if (this.origin !== origin || sid === null) return;
    const got = m.got.get(sid);
    if (got !== undefined) this.gotQueue(pendingKey(origin, sid), got);
    else if (m.all || m.ask.includes(sid)) void this.readQueue();
  }

  /** 此刻说一句是说给哪条会话（名单里挂着的那一条 · 标签页那一条）；认不出 ⇒ `null`。 */
  private sayingTo(): string | null {
    const w = this.host.active();
    return this.row?.sid ?? (w?.kind === "tab" ? w.tab.sessionId : null);
  }

  /** 交待发里那一句一次（同一个编号交几次那台都只收一条），照结局清框或留着。 */
  private async putWaiting(origin: Origin, key: string, o: Outgoing): Promise<PutEnd | null> {
    this.tried.add(o.id);
    const end = await putOnce(this.attachReads.put, origin, o);
    if (!(this.waiting.get(key) ?? []).includes(o)) return null;
    const here = this.origin === origin && this.sayingTo() === o.sid;
    const edited = o.rewind !== undefined;
    if (here && !edited) this.sending = false;
    // 没回执 ⇒ 留在待发（那台连上再交）；那台拒了 ⇒ 不留。两样都照那一句说、字留着（编辑的那一句由编辑框说）。
    if (end.at !== "receipt") {
      if (end.at === "refused") this.dropWaiting(key, o);
      if (here && !edited) this.showNote({ text: end.said, tone: "error" });
      this.paint();
      return end;
    }
    this.dropWaiting(key, o);
    if (here) void this.readQueue();
    if (edited) {
      this.paint();
      return end;
    }
    if (here && this.box.value === o.text) {
      this.box.value = "";
      if (this.sid !== null) this.drafts.delete(this.sid);
      this.onBoxInput();
    }
    this.attachDrafts.get(key)?.clear();
    if (here) {
      this.showNote({ text: end.said, tone: "ok" });
      if (!this.isLive) this.schedule();
    }
    return end;
  }

  /**
   * 送框里那段字。`say` ⇒ 说一句：投进那条会话的输入队、编号随那一句交（[say]；草稿里有附件 ⇒ 传完了才送，附件编号随它交）。
   * 只打字（Ctrl+回车）· 认不出会话（占位那一形没挂会话）⇒ 往这个终端里打字（`terminal-input`；说一句的那一下补回车），送到了才清框。
   */
  private sendBox(say: boolean): void {
    const text = this.box.value;
    const draft = this.attachDraft();
    const chips = draft !== null && draft.chips().length > 0;
    const origin = this.origin;
    const sid = this.sayingTo();
    if (chips || (say && origin !== undefined && sid !== null && this.row !== null)) {
      if (chips && draft.blocker(this.offlineLine()) !== null) return;
      if (text === "" && !chips) return;
      if (origin !== undefined && sid !== null) void this.say(origin, sid, text, chips ? draft.ids() : []);
      return;
    }
    const enter = say;
    if (text === "") return;
    // 往终端里打字要持有输入：不是这一端持有 ⇒ 一问都不发，提示那一行说那一句（字留着）。
    const held = this.heldNo();
    if (held !== null) {
      this.showNote({ text: held, tone: "error" });
      return;
    }
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
    setButtonLabel(this.sendBtn, n > LONG_TEXT ? copyText("terminal.input.sendLong", { n: countText(n) }) : copyText("terminal.input.send"));
  }

  /** 输入框里按 Esc：焦点还给消息流（不清字、不收抽屉）。是这一下就回 `true`。 */
  escFromInput(): boolean {
    if (document.activeElement !== this.box) return false;
    this.host.focusStream();
    return true;
  }

  /** 按此刻的状态重画。 */
  private paint(): void {
    const w = this.host.active();
    if (!w || this.sid === null) {
      this.body.replaceChildren(emptyState({ icon: "terminal", text: copyText("terminal.empty.noSession") }));
      return;
    }
    if (this.phase === "none" && w.kind === "slot") {
      this.body.replaceChildren(emptyState({ icon: "terminal", text: copyText("launch.slot.tmuxGone", { machine: machineName(w.origin), name: w.name }) }));
      return;
    }
    if (this.phase === "none" && w.kind === "tab") {
      const tab = w.tab;
      const sid = tab.sessionId;
      const ended = canResume(tab.face); // 已结束（能恢复的那一态）
      // 活着、名单里没有：不在任何终端里 · 在终端里却认不出，对用户是同一件事（非 cc-monitor 起的）；↗ 真能用时给［切到终端］。
      this.body.replaceChildren(
        ended
          ? emptyState({ icon: "terminal", text: copyText("terminal.empty.ended", { state: copyText("sessionState.ended.name") }), hint: copyText("terminal.empty.endedHint") })
          : emptyState({
              icon: "terminal",
              text: copyText("terminal.empty.notOurs"),
              hint: copyText("terminal.empty.notOursHint"),
              ...(terminalFrontAvailable() ? { action: button({ label: copyText("terminal.head.front"), size: "compact", icon: "front", onClick: () => this.host.front(sid) }) } : {}),
            }),
      );
      return;
    }
    if (this.phase === "loading") {
      const l = el("div");
      l.className = s.termLoading;
      l.append(spinner(), el("span", "", copyText("terminal.state.loading")));
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
    const tab = w.kind === "tab" ? w.tab : null;
    if (tab) setDot(this.dot, dotOf(tab), stateWord(tab));
    else setDot(this.dot, "unknown", copyText("launch.placeholder.title"));
    this.title.textContent = labelOf(w);
    this.where.textContent = tab?.projectDir ? copyText("terminal.head.where", { dir: tab.projectDir, machine: machineOf(tab) }) : machineWord(w);
    this.tag.textContent = row.name !== "" ? copyText("terminal.tag.tmux", { name: row.name }) : "";
    this.tag.style.display = row.name !== "" ? "" : "none";
    this.tag2.textContent = row.clients > 0 ? copyText("terminal.tag.windows", { n: row.clients }) : copyText("terminal.tag.background");
    this.at.textContent = this.shot ? copyText("terminal.head.snapAt", { time: this.shot.atText }) : "";
    const live = this.isLive;
    this.snapGroup.hidden = live;
    this.liveTag.hidden = !live;
    this.liveTag.dataset.state = this.live.at;
    this.liveTag.textContent = this.live.at === "on" ? copyText("terminal.head.live") : copyText("terminal.head.joining");
    this.snapTag.hidden = this.live.at !== "snapOnly";
    if (this.live.at === "snapOnly") this.snapTag.title = this.live.said;
    this.frontBtn.style.display = tab && terminalFrontAvailable() && hasTerminal(tab.face) ? "" : "none";

    const bars: HTMLElement[] = [];
    if (row.programExited) {
      const line = el("div", "", copyText("terminal.bar.programExited", { state: copyText("sessionState.reconnectable.name") }));
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

    this.to.textContent = copyText("terminal.input.to", { title: labelOf(w), machine: machineWord(w) });
    this.owner.textContent =
      row.holder !== undefined
        ? row.holder.said
        : row.clients > 0 && row.input === "shared"
          ? copyText("terminal.input.shared", { n: row.clients })
          : row.clients === 0
            ? copyText("terminal.input.nobody")
            : "";
    // 托管那一形才有［接管输入］/［交还］（看这一端持有没有）；名单说送不了的（别的前端的 · 不是我们起的）不给。
    this.holdBtn.style.display = row.holder !== undefined ? "" : "none";
    const holdLabel = row.holder?.mine === true ? copyText("terminal.input.release") : copyText("terminal.input.take");
    setButtonLabel(this.holdBtn, holdLabel);
    setDisabled(this.holdBtn, row.inputNo);
    setBusy(this.holdBtn, this.holding ? holdLabel : null);
    const no = this.inputNo();
    this.box.disabled = no !== null || this.sending;
    const held = this.heldNo();
    this.box.placeholder =
      no !== null ? copyText("terminal.input.readOnly", { why: no }) : held !== null ? copyText("terminal.input.placeholderHeld") : copyText("chat.input.placeholder");
    // 占位只写短的那句；快捷键说明挂悬停（送不了时键不起作用，不挂）。
    this.box.title = no !== null ? "" : copyText("terminal.input.keysHint");
    // 附件没传完 / 有失败 ⇒ 送出灰着、悬停说为什么（字照样能打）。
    setDisabled(this.sendBtn, no ?? this.attachDraft()?.blocker(this.offlineLine()) ?? null);
    this.attach.paint();
    const to = this.sayingTo();
    const qKey = this.origin !== undefined && to !== null ? pendingKey(this.origin, to) : null;
    this.queueRow.paint(qKey !== null && this.queueOf === qKey ? this.queue : null, qKey === null ? [] : (this.waiting.get(qKey) ?? []));
    setBusy(this.sendBtn, this.sending ? copyText("terminal.input.send") : null);
    for (const b of this.keyBtns) setDisabled(b, no ?? held);
    this.keysWhy.textContent = no === null && held !== null ? held : "";
    this.noteEl.replaceChildren();
    if (this.note) {
      const t = el("span", "", this.note.text);
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
