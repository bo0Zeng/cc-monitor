/**
 * 只读历史会话查看器：历史页点一条时建，按那台后端读出的记录行渲染（与实时 tab 同一套卡）。
 * 与 `TabManager` 无关：不建 Tab、不碰实时流；关掉即释放。
 */

import { speakerNameOf } from "../agent-profile";
import type { Origin } from "../ipc/origin";
import { MessageStream } from "../stream";
import {
  type LineRecord,
  type RenderContext,
  reconcilePendingToolResults,
} from "../cards";
import { BranchFolder } from "../branch-fold";
import { RecordTimeline } from "../record-timeline";
import { releaseEnhanceRoot } from "../render";
import { renderStreamRecord, type StreamSink } from "../render-stream-record";
// 查看器 ＝ 骨架 ＋ 按视口取：与实时 tab **同一个** `SkeletonView`（占位 ＋ 只物化可见区）、同一条取法（`rowRuns` → `readRange`）。
import { SkeletonView, ledgerFromIndex, rowRuns } from "../skeleton-view";
import type { SkeletonLedger } from "../live-window";
import { skeletonKind } from "../height-estimate";
import { findInSession, readSessionIndex } from "../session-reads";
import { readBranch, readLines, readRange, readRecordById } from "../record-reads";
import { followSession, type FollowEvent } from "../events";
import { attachBranchButton } from "../branch-button";
import { openNewSession } from "../new-session";
// 大纲的清单问后端要（判定只住后端），实时 tab 用的是同一个类
import { OutlineSource } from "./outline-source";
// 「你说过的话」清单界面与实时 tab 同一个类（`UserInputPanel`），这里只换开法：工具行一颗按钮 ＋ kit 浮层。
import { UserInputPanel, type JumpResult } from "./user-input-panel";
// 按轮折叠与主窗口同一个（`turn-fold.ts`）。
import { TurnFold, revealProcessOf } from "../turn-fold";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { banner } from "../kit/banner";
import { copyDetailButton, detailOf } from "../kit/detail";
import { skeletonRows } from "../kit/skeleton";
import { emptyState } from "../kit/empty";
import { openPopover, closePopover, popoverOpenOn } from "../kit/popover";
import { FindStrip } from "../find-strip";
import sv from "./session-viewer.module.css";
import { copyText } from "../copy-table";

/**
 * 在一条消息流里按 uuid 找到那张卡、展开挡着它的折叠（过程折叠 · `<details>` · ESC 回退段）、滚过去并闪一下；找不到 ⇒ `null`、什么都不做（兜底归调用方）。
 * 导出给 `tabs.ts` 的实时窗口共用：两条路的卡由同一个 `renderStreamRecord` 建、`data-id` 由同一处写。
 */
export function revealCard(container: HTMLElement, uuid: string): HTMLElement | null {
  // CSS.escape 防 uuid 里有特殊字符破坏选择器
  const key = CSS.escape(uuid);
  // 卡找不到 ⇒ 再找「被并进工具组 / 被注入进 tool_use」的那一块（`data-member-id`）
  const el =
    container.querySelector<HTMLElement>(`[data-id="${key}"]`) ??
    container.querySelector<HTMLElement>(`[data-member-id="${key}"]`);
  if (!el) return null;
  // 落在某一轮折着的过程里 ⇒ 先让那一轮展开（记成手动开；`turn-fold.ts`）。
  revealProcessOf(el, container);
  // 展开所有折叠祖先：ESC 回退段是 `div.branch-fold-wrap` ＋ `.expanded`（不是 `<details>`），不展开的话卡被 0fr 裁掉、闪了也看不见。
  let p: HTMLElement | null = el.parentElement;
  while (p && p !== container) {
    if (p instanceof HTMLDetailsElement) p.open = true;
    if (p.classList.contains("branch-fold-wrap") && !p.classList.contains("expanded")) {
      p.classList.add("expanded");
      p.querySelector(".branch-fold-header")?.setAttribute("aria-expanded", "true");
    }
    p = p.parentElement;
  }
  el.scrollIntoView({ block: "center" });
  // 调度：一次性 —— 首次落点按 content-visibility 的估值几何；双 rAF 后周边材料化成真实尺寸，再落一次才准。
  requestAnimationFrame(() => requestAnimationFrame(() => el.scrollIntoView({ block: "center" })));
  el.classList.add("search-hit-flash");
  // 动画结束后移除 class（再次跳同一条还能重放）
  // 调度：一次性 —— 1.5s 后摘掉搜索命中的闪烁
  window.setTimeout(() => el.classList.remove("search-hit-flash"), 1500);
  return el;
}

/**
 * 历史会话的 jsonl 文件名**就是** sid（口径同 `remote_history::jsonl_stem`）。
 * 远端分叉只认 sid，而查看器手上只有路径，所以从路径取。取不出来 → 空串，
 * 后端的白名单会拒（fail-closed，不会拿一个残缺 id 去跑）。
 */
function sidFromJsonlPath(p: string): string {
  const name = p.split(/[\\/]/).pop() ?? "";
  return name.endsWith(".jsonl") ? name.slice(0, -".jsonl".length) : "";
}

interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  /** 文件内单调的 seq；按它排进时间线。 */
  seq: number;
  record: LineRecord;
}

/** 内容头：内容由宿主按那一行的事实拼好，查看器只摆位置。 */
export interface ViewerHead {
  /** 标题左边那一格（窄档历史页的「← 列表」）。 */
  lead?: HTMLElement;
  /** 标题右边的徽标（在跑 · Codex · 分身 …）。 */
  badges?: HTMLElement[];
  /** 第一行右端：［恢复 ▾］· 新窗口 ·［⋯］。 */
  actions?: HTMLElement;
  /** 第二行：项目 · 机器 · 路径 · 时间段（查看器在后面接「· {n} 条」）。 */
  meta?: HTMLElement[];
}

/** 头第二行里的路径那一格（等宽、放不下省略）。 */
export function viewerPath(text: string): HTMLElement {
  const span = document.createElement("span");
  span.className = sv.svPath;
  span.textContent = text;
  span.title = text;
  return span;
}

export interface ViewerOptions {
  jsonlPath: string;
  /** 顶栏标题（后端给的显示标题）。 */
  displayTitle: string;
  head?: ViewerHead;
  /** 从全文搜索跳进来时命中的那条 uuid：读完定位到它（展开所在折叠段）、居中并闪一下，不贴底。 */
  scrollToUuid?: string;
  /** 哪台机器的会话（本机 ＝ `LOCAL_ORIGIN`）。必填：「没说」不许当成本机。 */
  origin: Origin;
  /** 这个会话是哪一家（历史清单那一行的 `agent`，后端给的）：卡头那一家的名字按它取。 */
  agent?: string | null;
  /** 会话工作目录（历史条目的 projectPath）：分叉出新会话时作起始目录；缺了要多问一次。 */
  cwd?: string;
  /** 不挂「从这一轮分叉」：子 agent 的记录不是可分叉的会话。 */
  suppressBranch?: boolean;
  /**
   * 跟着这个会话长（在跑的会话；与独立查看窗同一条订阅 `session-lines/<sid>`）：先订、再读，读完之后流里来的接在后面。
   * `live` = 打开那一刻它在不在跑（之后跟流里的起停）；`onLive` = 起了 / 结束了（宿主据此换头上的按钮）。
   */
  follow?: { sid: string; live: boolean; onLive?: (live: boolean) => void };
  /** 底一行怎么说（缺 ⇒ `{n} 条` / `{n} 条 · 上翻加载更早` / `运行中 · 实时`）。 */
  statusOf?: (s: ViewerStatus) => string;
}

/** 底一行要说的几件事。`following` = 真订着那一条流、那台看得见。 */
export interface ViewerStatus {
  n: number;
  more: boolean;
  live: boolean;
  following: boolean;
}

/** 历史页里那一个查看器的底一行。 */
function pageStatus(s: ViewerStatus): string {
  if (s.live && s.following) return copyText("sessionViewer.status.live");
  return s.more ? copyText("sessionViewer.status.more", { n: s.n }) : copyText("sessionViewer.status.all", { n: s.n });
}

const TAIL_INITIAL = 150; // 首屏取回、渲染的末尾条数（建卡的那几类）
const ISLAND_RADIUS = 100; // 深链岛：命中那一条上下各取几条

/** 从 `from`（不含）往前数 `k` 条建卡的行，回最早那一条的 seq（不够 ⇒ 账本第一行）。 */
function cardsBack(ledger: SkeletonLedger, from: number, k: number): number {
  let n = 0;
  for (let q = from - 1; q >= ledger.base; q--) {
    const f = ledger.factsOf(q);
    if (f && skeletonKind(f) !== "none" && ++n === k) return q;
  }
  return ledger.base;
}

/** 从 `at`（含）往后数 `k` 条建卡的行，回最后那一条的下一行（不够 ⇒ 账本末尾）。 */
function cardsAhead(ledger: SkeletonLedger, at: number, k: number): number {
  let n = 0;
  for (let q = at; q < ledger.endSeq; q++) {
    const f = ledger.factsOf(q);
    if (f && skeletonKind(f) !== "none" && ++n === k) return q + 1;
  }
  return ledger.endSeq;
}

/**
 * 某一条显示不了 ⇒ 卡的位置上画「这一条显示不了」［复制详情］；原因进日志，不在状态行报数，详情交那一条的原文 ＋ 原因。
 */
function brokenCard(p: JsonlLinePayload, err: unknown): HTMLElement {
  const card = document.createElement("div");
  card.className = sv.svBroken;
  card.dataset.role = "broken";
  card.dataset.seq = String(p.seq);
  const t = document.createElement("span");
  t.textContent = copyText("sessionViewer.card.broken");
  const detail = `seq=${p.seq}\n${String(err)}\n${JSON.stringify(p.record, null, 2)}`;
  // 全产品那一颗［复制详情］（反馈 · 复制不了的回落都在那里）：首行是卡上那句，下面是 seq · 原因 · 那条原文（仍是用户自己的记录）。
  const copy = copyDetailButton(t.textContent, detail);
  card.dataset.detailHost = "";
  card.append(icon("warning", "compact"), t, ...(copy ? [copy] : []));
  return card;
}

/** 独立查看窗那一形：头由窗口的细顶栏担（这里不画）；「你说过的话」是左边一栏；底一行写进窗口的状态栏 `foot`。 */
export interface ViewerShape {
  window?: { foot: HTMLElement };
}

export class SessionViewer {
  private root: HTMLElement;
  private shape: ViewerShape;
  /** 窗口那一形的左栏与它的组名（`你说过的话 · N`）。 */
  private sideBody: HTMLElement | null = null;
  private sideHead: HTMLElement | null = null;
  private toolsEl!: HTMLElement;
  private streamEl!: HTMLElement;
  private stream: MessageStream | null = null;
  /**
   * 骨架层：先要这份会话的骨架索引，没取回的 seq 区间由占位顶住（滚动条一开始就是全会话的），
   * 滚到哪、跳到哪就按索引里的字节边界取哪一段（`rowRuns` → `readRange`，与实时 tab 同一条）。正文不整份读、不驻留。
   */
  private skeleton: SkeletonView | null = null;
  /** 在途的「按偏移取一段」：跳转 · 首屏等它们落完。 */
  private readonly inflight = new Set<Promise<void>>();
  /** 已经画进来的行号（取回的 · 跟着长接上的）。 */
  private readonly drawn = new Set<number>();
  /** 跟着长接上的、骨架索引之后的最后一行（没有 ⇒ 索引的最后一行）。 */
  private liveLast = -1;
  /** 会话里进界面的条数（骨架里有 `t` 的行 ＋ 跟着长接上的）。 */
  private total = 0;
  private renderCtx: RenderContext | null = null;
  private renderSink: StreamSink | null = null;
  private folder: BranchFolder | null = null;
  /** 按轮折叠（与主窗口同一个）：一轮的边界与结论问后端 `history-turns`。 */
  private turnFold: TurnFold | null = null;
  /** 读取世代号：异步间隙（rAF / 通道）之后核对，换了会话的残余操作直接丢掉。 */
  private loadGeneration = 0;
  private onScrollFill = (): void => {
    void this.maybeFillAbove();
  };
  private titleEl!: HTMLElement;
  private leadEl!: HTMLElement;
  private badgesEl!: HTMLElement;
  private actionsEl!: HTMLElement;
  private metaEl!: HTMLElement;
  private countEl!: HTMLElement;
  private bannerEl!: HTMLElement;
  private loadingEl!: HTMLElement;
  /** 读到了、一条可显示的消息都没有 ⇒ 消息流的位置换成这块空态。 */
  private emptyEl!: HTMLElement;
  private statusEl!: HTMLElement;
  /** 「你说过的话」那份清单（实时 tab 同一个类）；开法换成工具行的按钮 ＋ 浮层。 */
  private said!: UserInputPanel;
  private saidBtn!: HTMLButtonElement;
  private saidText!: HTMLElement;
  /** 清单平时住这里（收着）；开浮层时搬进浮层里那一格（`saidPop`，给它一个宽度）。 */
  private saidHold!: HTMLElement;
  private saidPop!: HTMLElement;
  /** 大纲的数据源；`where` 在 `load` 时换成这一份会话。 */
  private outline!: OutlineSource;
  private outlineWhere: { origin: string; jsonlPath: string } | null = null;
  /** 会话内查找：工具行的框 ＋ 框下就地展开的命中清单（主窗口的会话内查找复用同一个，`find-strip.ts`）。 */
  private find!: FindStrip;
  /** 跟着长那一条订阅（`null` = 没在跟）。 */
  private followSub: { stop(): void } | null = null;
  /** 读完之前流里先来的行（读完再按 `seq` 接上）。 */
  private followBuf: JsonlLinePayload[] = [];
  /** 读完之前流里先来的主线外清单（比冷读那一份新：读完就用它）。 */
  private followOff: string[] | null = null;
  private loaded = false;
  private live = false;
  private following = false;
  private statusOf: (s: ViewerStatus) => string = pageStatus;
  private onLive: ((live: boolean) => void) | undefined;
  /** 「↓ 新内容」：不在底部时来了新内容 ⇒ 出；点它或滚回底部 ⇒ 收。 */
  private newPill!: HTMLButtonElement;
  private opts: ViewerOptions | null = null;
  constructor(shape: ViewerShape = {}) {
    this.shape = shape;
    this.root = this.build();
  }

  /** 窗口那一形：收起 / 展开左边「你说过的话」那一栏（窄于 760 默认收着）。 */
  toggleSide(): void {
    const body = this.sideBody;
    if (!body) return;
    const shown = body.dataset.side === "open" || (body.dataset.side !== "closed" && window.innerWidth >= 760);
    body.dataset.side = shown ? "closed" : "open";
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 头下面那一条（恢复失败 · 读不出 …，宿主的错误条也挂这里）；`null` ⇒ 收掉。 */
  showBanner(el: HTMLElement | null): void {
    this.bannerEl.replaceChildren(...(el ? [el] : []));
    this.bannerEl.hidden = el === null;
  }

  /** 读取中（骨架）开 / 关。 */
  private setLoading(on: boolean): void {
    this.loadingEl.hidden = !on;
  }

  /**
   * 空态开 / 关：读完了（`loaded`）、没有还没取的段、消息流里一张卡都没有 ⇒ 开；否则关。
   * 在跑的会话还会长 ⇒ 空态多一句「新消息到达后显示」。
   */
  private syncEmpty(): void {
    const none =
      this.loaded &&
      !!this.stream &&
      this.stream.contentElement.childElementCount === 0 &&
      (this.skeleton?.pendingRows ?? 0) === 0;
    if (none) {
      const hint = this.live ? copyText("sessionViewer.empty.liveHint") : undefined;
      const e = emptyState({ icon: "chat", text: copyText("sessionViewer.empty.none"), hint });
      this.emptyEl.replaceChildren(e);
    }
    this.emptyEl.hidden = !none;
    this.streamEl.hidden = none;
  }

  /** 「你说过的话」清单面板露 / 收（`.user-inputs` 不写 display，`hidden` 管得住）。 */
  private showSaidPanel(on: boolean): void {
    this.said.panel.hidden = !on;
  }

  /** 换头（同一份会话、事实变了：标星 · 改标题 · 在跑变已结束）。不重读记录。 */
  setHead(title: string, head: ViewerHead | undefined): void {
    this.titleEl.textContent = title;
    this.titleEl.title = title;
    this.leadEl.replaceChildren(...(head?.lead ? [head.lead] : []));
    this.badgesEl.replaceChildren(...(head?.badges ?? []));
    this.actionsEl.replaceChildren(...(head?.actions ? [head.actions] : []));
    this.metaEl.replaceChildren(...(head?.meta ?? []), this.countEl);
  }

  /**
   * 骨架 ＋ 按视口取：先要骨架索引（整份会话的行 → 字节边界 · 类 · 估高料），整份画成占位；
   * 首屏只按偏移取末尾 `TAIL_INITIAL` 条（深链再取命中那一条附近一段），落完折一次、贴底或定位；
   * 之后滚到哪段占位就取哪段（`maybeFillAbove` → `fillVisible` → `materialize` → `fetchRows`）。
   * 索引要不到 ⇒ 读不出那一条（不退回整份读）。`dispose()` 让世代号递增，在途的段与异步残余都按世代号丢掉。
   */
  async load(opts: ViewerOptions): Promise<void> {
    this.setHead(opts.displayTitle, opts.head);
    this.countEl.textContent = "";
    this.showBanner(null);

    this.disposeStream();
    this.outlineWhere = { origin: opts.origin, jsonlPath: opts.jsonlPath };
    const gen = ++this.loadGeneration;
    this.opts = opts;
    this.statusOf = opts.statusOf ?? pageStatus;
    this.onLive = opts.follow?.onLive;
    this.live = opts.follow?.live ?? false;
    this.loaded = false;
    this.syncEmpty();
    this.followBuf = [];
    this.followOff = null;
    this.showNewPill(false);
    // 先订、再读：读的这段时间里写出来的行在流里等着，读完按 `seq` 接上（一行都不漏、重叠的去重）。
    if (opts.follow) {
      const sub = await followSession(opts.origin, opts.follow.sid, (e) => this.onFollow(gen, e));
      if (this.loadGeneration !== gen) {
        sub.stop();
        return;
      }
      this.followSub = sub;
      this.following = true;
    }
    this.streamEl.replaceChildren();
    this.stream = new MessageStream(this.streamEl);
    this.turnFold = new TurnFold(this.stream.contentElement, this.streamEl, () => this.outlineWhere, undefined, () => this.skeleton);
    this.streamEl.addEventListener("scroll", this.turnFold.releaseOnScroll, { passive: true });

    // 读取中：骨架；状态行不说话。
    this.setLoading(true);
    this.statusEl.textContent = "";

    // 高亮按需（lazy hljs）
    const ctx: RenderContext = {
      parentPath: opts.jsonlPath,
      speaker: speakerNameOf(opts.agent ?? null),
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
      // 远端会话展开子 agent 要带上 origin
      origin: opts.origin,
      // 出口省掉的正文展开那一下：按骨架里那一行的偏移取回全文
      fullRecord: (id) => readRecordById(opts.origin, opts.jsonlPath, this.skeleton?.ledger ?? null, id),
      lazy: true,
    };
    const timeline = new RecordTimeline(this.stream);
    const sink: StreamSink = {
      timeline,
      enhanceRoot: this.streamEl, // IO 的 root = 查看器自己的滚动容器
      // 每张 user / assistant 卡挂「从这一轮分叉」（远端走后端的 `--fork-session`，只认 sid）；子 agent 记录不挂。
      onCardRendered: opts.suppressBranch
        ? undefined
        : (el, msg) =>
            this.attachBranchButton(el, msg, opts.jsonlPath, opts.origin),
    };
    this.renderCtx = ctx;
    this.renderSink = sink;
    this.drawn.clear();
    this.liveLast = -1;
    this.total = 0;

    const origin = opts.origin;
    // 主线外清单（回退掉的那几条）与索引并行冷读一次；读不到 ⇒ 不折（之后跟着长的由流里的 `branch` 格说）。
    const branchP = readBranch(origin, opts.jsonlPath).catch((e: unknown) => {
      console.warn("[session-viewer] 主线外清单没读到（不折）：", e);
      return null;
    });

    try {
      // 经通道问那台后端（`session-reads.ts`）：要不到 ⇒ `available:false`，它自己不抛。本机与远端同一条路（`origin` 必填）。
      const res = await readSessionIndex(origin, opts.jsonlPath, 0);
      if (this.loadGeneration !== gen || !this.stream) return;
      const got = ledgerFromIndex(res);
      if (!got.ok) throw new Error(got.reason);
      const ledger = got.ledger;
      for (let q = ledger.base; q < ledger.endSeq; q++) if (ledger.factsOf(q)?.t !== undefined) this.total++;
      this.folder = new BranchFolder(this.stream.contentElement);
      const view = new SkeletonView(ledger, this.streamEl, sink.timeline, {
        materialize: (lo, hi) => this.fetchRows(gen, view, lo, hi),
      });
      this.skeleton = view;
      // 折叠先交给账本：占位插进去就是折后的高（同主窗口 `TabStreamView.attachSkeleton`）
      this.turnFold?.seedFolds(ledger);
      view.attachGaps([[ledger.base, ledger.endSeq]]);
      // 首屏：尾巴（＋ 深链岛）
      view.ensureRange(cardsBack(ledger, ledger.endSeq, TAIL_INITIAL), ledger.endSeq);
      const target = opts.scrollToUuid !== undefined ? ledger.uuidToSeq.get(opts.scrollToUuid) : undefined;
      if (target !== undefined && view.isPending(target)) {
        view.ensureRange(cardsBack(ledger, target, ISLAND_RADIUS), cardsAhead(ledger, target, ISLAND_RADIUS));
      }
      const [branch] = await Promise.all([branchP, this.settled()]);
      if (this.loadGeneration !== gen || !this.stream) return;
      this.setLoading(false);
      const off = this.followOff ?? branch?.off;
      this.followOff = null;
      if (off) this.folder.setOff(new Set(off));
      this.rebuildFold();
      this.countEl.textContent = copyText("sessionViewer.head.count", { n: this.total });
      this.updateStatus();
      // 清单建在这里：面板默认收着 ⇒ 对下面的定位 / 贴底零布局影响（滚之前插一块可见的东西会把落点顶歪）。
      this.rebuildUserInputs();
      void this.turnFold?.refresh();
      // 从搜索结果跳进来 ⇒ 定位到命中消息；否则贴底。
      if (opts.scrollToUuid) {
        void Promise.resolve(this.scrollToMessage(opts.scrollToUuid)).catch((e: unknown) => console.warn("[session-viewer] 定位没取到：", e));
      } else {
        this.stream?.scrollToBottom();
      }
      // 滚到哪段占位就取哪段（dispose 时随 streamEl 替换自然解绑）
      this.streamEl.addEventListener("scroll", this.onScrollFill, { passive: true });
      // 调度：一次性 —— 首屏落完之后视口里若还露着占位（尾巴不足一屏），主动物化一次；没布局 ⇒ 什么都不做
      requestAnimationFrame(() => void this.maybeFillAbove());
      // 读的这段时间里流里先来的行接上（索引里已经有的按 `seq` 去掉）。
      this.loaded = true;
      const early = this.followBuf;
      this.followBuf = [];
      if (early.length > 0) this.appendLive(early);
      this.syncEmpty();
    } catch (e) {
      if (this.loadGeneration !== gen) return;
      this.setLoading(false);
      // 读不出 ⇒ 头下面一条错误条 ＋［重试］；状态行不报。
      const retry = button({ label: copyText("sessionViewer.load.retry"), size: "compact", onClick: () => void this.load(opts) });
      this.showBanner(banner("error", copyText("sessionViewer.load.failed", { why: String(e) }), [retry], detailOf(e)));
    }
  }

  /** 在途的段都落完（失败的那一个原样抛出：首屏读不出就是读不出）。 */
  private async settled(): Promise<void> {
    const done = await Promise.allSettled([...this.inflight]);
    const failed = done.find((r): r is PromiseRejectedResult => r.status === "rejected");
    if (failed) throw failed.reason instanceof Error ? failed.reason : new Error(String(failed.reason));
  }

  /**
   * 骨架交来 `[lo, hi)`：按索引里的字节边界取要建卡的那几段（`rowRuns`，已经画过的切段），落地按 seq 插进时间线（视口钉住）、
   * 配对工具结果、重折。取不到 ⇒ 那一段放回占位（下次滚到 / 跳到再取），失败原样交给等它的人。
   */
  private fetchRows(gen: number, view: SkeletonView, lo: number, hi: number): void {
    const o = this.opts;
    if (!o) return;
    for (const run of rowRuns(view.ledger, lo, hi, (q) => this.drawn.has(q))) {
      const p: Promise<void> = readRange(o.origin, o.jsonlPath, run.offset, run.until, run.a).then(
        (rows) => {
          if (this.loadGeneration !== gen || this.skeleton !== view) return;
          view.pinned(() => this.drawRows(rows));
          this.updateStatus();
          this.syncEmpty();
        },
        (e: unknown) => {
          if (this.loadGeneration === gen && this.skeleton === view) view.restore(run.a, run.b);
          console.warn(`[session-viewer] 按偏移取正文失败 [${run.a},${run.b})：`, e);
          throw e instanceof Error ? e : new Error(String(e));
        },
      );
      this.inflight.add(p);
      p.catch(() => {});
      void p.finally(() => this.inflight.delete(p)).catch(() => {});
    }
  }

  /** 取回的 / 跟着长来的几行画进去（按 seq 插；画过的跳过）：配对工具结果、重折。 */
  private drawRows(rows: JsonlLinePayload[]): void {
    if (!this.stream || !this.renderCtx || !this.renderSink) return;
    const fresh = rows.filter((p) => !this.drawn.has(p.seq));
    if (fresh.length === 0) return;
    // 二分插入只能在摊平的 DOM 上做：邻居若已被折叠层收编，insertBefore 会 NotFoundError ⇒ 先摊平，批后重折。
    this.folder?.unwrapAll();
    // 批内暂停逐卡贴底：逐卡读 scrollHeight 就是逐卡一次强制 reflow；批末按粘底状态一次贴底。
    this.stream.batchInsert(() => {
      for (const p of fresh) {
        this.drawn.add(p.seq);
        this.renderOne(p);
      }
    });
    // 段缝落在 tool_use / tool_result 中间时 result 先成了孤儿卡；另一段补出 tool_use 后必须回填合并，孤儿卡出 DOM 的同时出账。
    for (const el of reconcilePendingToolResults(this.renderCtx)) this.renderSink.timeline.removeByElement(el);
    this.rebuildFold();
  }

  /** 画一条；显示不了 ⇒ 卡位上画「这一条显示不了」［复制详情］，原因进日志（不在状态行报数）。 */
  private renderOne(p: JsonlLinePayload): void {
    try {
      renderStreamRecord(p, this.renderCtx!, this.renderSink!);
    } catch (err) {
      console.error("[session-viewer] renderStreamRecord 抛错", p, err);
      const tl = this.renderSink!.timeline;
      if (!tl.has(p.seq)) tl.insert({ seq: p.seq, element: brokenCard(p, err), kind: "card" });
    }
  }

  /** 给一张 said / reply 卡挂「从这一轮分叉」（按钮本体 `branch-button.ts`，与实时 tab 同一份）：这里只管该不该挂、成功之后干什么。 */
  private attachBranchButton(
    cardEl: HTMLElement,
    record: LineRecord,
    jsonlPath: string,
    origin: Origin,
  ): void {
    if (record.t !== "said" && record.t !== "reply") return;
    attachBranchButton(cardEl, {
      uuid: record.id,
      // 历史会话的文件名就是 sid（`sidFromJsonlPath`）；与实时 tab 开同一个框（分叉记录在框里点［新建］才写）。
      onFork: (at) =>
        void openNewSession({ origin, fork: { sid: sidFromJsonlPath(jsonlPath), uuid: at, title: this.titleEl.textContent ?? "" } }),
    });
  }

  /** 增量批后按清单重折（没渲染的卡不在 DOM，自然跳过）。 */
  private rebuildFold(): void {
    this.folder?.rebuildNow();
  }

  /** 底一行只说条数：`{n} 条` / `{n} 条 · 上翻加载更早`（显示不了的那一条在卡位上说）。 */
  private updateStatus(): void {
    // 顶上还有没取的（上翻能补）⇒ 说一句；只剩深链岛与尾段之间的缝 ⇒ 不说。
    const sk = this.skeleton;
    const more = sk !== null && sk.isPending(sk.ledger.base);
    this.statusEl.textContent = this.statusOf({ n: this.total, more, live: this.live, following: this.following && this.followSub !== null });
  }

  // ==== 跟着长（在跑的会话） ====

  /** 流里那一个会话的事（换了会话之后迟到的不认）。 */
  private onFollow(gen: number, e: FollowEvent): void {
    if (this.loadGeneration !== gen) return;
    if (e.t === "lines") {
      if (this.loaded) this.appendLive(e.lines);
      else this.followBuf.push(...e.lines);
    } else if (e.t === "gap") {
      if (this.loaded) void this.catchUp(gen);
    } else if (e.t === "live") {
      if (this.live === e.live) return;
      this.live = e.live;
      if (this.loaded) this.updateStatus();
      this.syncEmpty();
      this.onLive?.(e.live);
    } else if (e.t === "branch") {
      // 主线外清单（整份）：读完了就当场重折；还没读完 ⇒ 记着，读完用它（比冷读那一份新）。
      if (this.loaded && this.folder) this.folder.setOff(new Set(e.off));
      else this.followOff = e.off;
    } else if (e.t === "sight") {
      this.following = e.seen;
      if (this.loaded) this.updateStatus();
      // 又看得见了 ⇒ 看不见那段时间里写出来的按行号补上。
      if (e.seen && this.loaded) void this.catchUp(gen);
    }
  }

  /** 已经有的最后一行的行号（骨架索引的最后一行，或跟着长接上的更后面那一行；没有 ⇒ -1）。 */
  private lastSeq(): number {
    return Math.max(this.liveLast, (this.skeleton?.ledger.endSeq ?? 0) - 1);
  }

  /** 流里丢了行 / 断过 ⇒ 从已有的最后一行之后按行号读到末尾，接上。 */
  private async catchUp(gen: number): Promise<void> {
    const o = this.opts;
    if (!o) return;
    let from = this.lastSeq() + 1;
    try {
      for (;;) {
        const page = await readLines(o.origin, o.jsonlPath, from, undefined, 15_000);
        if (this.loadGeneration !== gen || !this.stream) return;
        if (page.payloads.length > 0) this.appendLive(page.payloads);
        if (page.eof || page.next <= from) return;
        from = page.next;
      }
    } catch (e) {
      console.warn("[session-viewer] 跟着长：补行没读到", e);
    }
  }

  /**
   * 新来的几行接在后面（`seq` 不比已有的最后一行大的去掉）：逐条画、配对工具结果、重折；
   * 在底部 ⇒ 跟着贴底（`MessageStream` 自己贴）；往上翻了 ⇒ 不拽人，出「↓ 新内容」。
   */
  private appendLive(lines: JsonlLinePayload[]): void {
    if (!this.stream || !this.renderCtx || !this.renderSink) return;
    let last = this.lastSeq();
    const fresh = [...lines].sort((a, b) => a.seq - b.seq).filter((p) => (p.seq > last ? ((last = p.seq), true) : false));
    if (fresh.length === 0) return;
    const atBottom = this.stream.stuckToBottom;
    this.liveLast = last;
    this.total += fresh.length;
    this.drawRows(fresh);
    this.countEl.textContent = copyText("sessionViewer.head.count", { n: this.total });
    this.updateStatus();
    this.rebuildUserInputs();
    void this.turnFold?.refresh();
    if (!atBottom) this.showNewPill(true);
    this.syncEmpty();
  }

  private showNewPill(on: boolean): void {
    if (this.newPill) this.newPill.hidden = !on;
  }

  /** 滚动 / 首屏之后：只物化与视口相交的那段占位（取回由 `fetchRows` 做，不自链）。 */
  private maybeFillAbove(): void {
    if (this.skeleton && this.skeleton.fillVisible() > 0) this.updateStatus();
  }

  /**
   * 滚到指定 uuid 的卡并闪一下（`revealCard`），返回落到的那张卡 / `null`（同主窗口 `TabStreamView.jumpInTab`）：
   * 还在占位里 ⇒ 经骨架物化它附近那一段、**等在途的段落完**再找卡（取不到 ⇒ reject 带原因）；找不到就退到贴底。
   */
  private scrollToMessage(uuid: string): JumpResult {
    const sk = this.skeleton;
    const seq = sk?.ledger.uuidToSeq.get(uuid);
    if (sk && seq !== undefined && sk.isPending(seq)) {
      sk.ensure(seq, ISLAND_RADIUS);
      this.updateStatus();
      if (this.inflight.size > 0) {
        const gen = this.loadGeneration;
        return Promise.allSettled([...this.inflight]).then((done) => {
          if (this.loadGeneration !== gen) return null;
          const el = this.reveal(uuid);
          const failed = done.find((r): r is PromiseRejectedResult => r.status === "rejected");
          if (!el && failed) throw failed.reason instanceof Error ? failed.reason : new Error(String(failed.reason));
          return el;
        });
      }
    }
    return this.reveal(uuid);
  }

  private reveal(uuid: string): HTMLElement | null {
    const el = revealCard(this.streamEl, uuid);
    if (!el) this.stream?.scrollToBottom();
    return el;
  }


  // ==== 「你说过的话」 ====

  /**
   * 清单**问后端要**（`history-user-inputs`），不扫 `payloads`：判定只住后端，两个宿主（本查看器 / 实时 tab）走同一个 `OutlineSource`。
   * 界面与「跳完回头核一次落点」那一段住 `user-input-panel.ts`（实时窗口同一份）。
   */
  private rebuildUserInputs(): void {
    void this.outline.refresh();
  }

  /** 工具行那颗按钮跟着清单走：`你说过的话 · {n}`；0 条 / 要不到 ⇒ 灰着（原因挂在悬停上，同 `UserInputPanel` 的口径）。 */
  private syncSaid(): void {
    const n = this.said.panel.children.length;
    if (this.sideHead) this.sideHead.textContent = n > 0 ? copyText("sessionViewer.tools.said", { n }) : copyText("sessionViewer.tools.saidNone");
    // 宽档写全（`你说过的话 · 12`）；中档、窄档只剩数字（图标在前面，乙4-⑧）。
    const full = document.createElement("span");
    full.className = sv.svSaidFull;
    full.dataset.part = "full";
    full.textContent = n > 0 ? copyText("sessionViewer.tools.said", { n }) : copyText("sessionViewer.tools.saidNone");
    const short = document.createElement("span");
    short.className = sv.svSaidShort;
    short.textContent = String(n);
    this.saidText.replaceChildren(full, short);
    this.saidBtn.disabled = n === 0;
    this.saidBtn.title = this.said.toggle.title;
    if (n === 0 && popoverOpenOn(this.saidBtn)) closePopover();
  }

  /** 开「你说过的话」浮层（再点一次 ⇒ 收）：当前读到的那一句高亮、滚到它。清单平时收在查看器里（`saidHold`），浮层关了就回去。 */
  private openSaid(): void {
    if (this.sideBody) return; // 窗口那一形：清单常在左边一栏
    const cur = this.currentSaid();
    for (const row of this.said.panel.querySelectorAll<HTMLElement>(".user-input-row")) {
      if (row === cur) row.setAttribute("aria-current", "true");
      else row.removeAttribute("aria-current");
    }
    this.showSaidPanel(true);
    this.saidPop.appendChild(this.said.panel);
    const opened = openPopover(this.saidBtn, this.saidPop, {
      label: copyText("sessionViewer.tools.saidLabel"),
      align: "start",
      onClose: () => {
        this.showSaidPanel(false);
        this.saidHold.appendChild(this.said.panel);
        this.saidBtn.setAttribute("aria-expanded", "false");
      },
    });
    if (!opened) return;
    this.saidBtn.setAttribute("aria-expanded", "true");
    cur?.scrollIntoView({ block: "nearest" });
  }

  /**
   * 视口顶上那一句之前最近的一句「你说的」（没有渲染出来的不算）。
   * 清单行按 uuid 记一张表，流里的卡按文档顺序扫一遍对上（不逐行到流里找卡：几百句 × 整条流的查找，每帧几百毫秒）；
   * 对上的那几张按上沿二分（卡按文档顺序上下排着）。
   */
  private currentSaid(): HTMLElement | null {
    const rows = new Map<string, HTMLElement>();
    for (const row of this.said.panel.querySelectorAll<HTMLElement>(".user-input-row")) {
      const uuid = row.dataset.inputUuid;
      if (uuid && !rows.has(uuid)) rows.set(uuid, row);
    }
    if (rows.size === 0) return null;
    const pairs: Array<[HTMLElement, HTMLElement]> = [];
    const taken = new Set<string>();
    for (const card of this.streamEl.querySelectorAll<HTMLElement>("[data-id]")) {
      const uuid = card.getAttribute("data-id") ?? "";
      const row = rows.get(uuid);
      if (row && !taken.has(uuid)) {
        taken.add(uuid);
        pairs.push([card, row]);
      }
    }
    if (pairs.length === 0) return null;
    const top = this.streamEl.getBoundingClientRect().top + 8;
    let lo = 0;
    let hi = pairs.length - 1;
    let at = 0;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (pairs[mid][0].getBoundingClientRect().top <= top) {
        at = mid;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    return pairs[at][1];
  }

  /** Ctrl+F（动作 `session.find`）落在查看器上：焦点进查找框、全选（窗口那一形平时不露工具行，这时露出来）。 */
  openFind(): void {
    this.toolsEl.hidden = false;
    this.find.focus();
  }

  /** 主动释放（HistoryView 卸载本组件时调） */
  dispose(): void {
    this.disposeStream();
  }

  private disposeStream(): void {
    this.followSub?.stop();
    this.followSub = null;
    this.following = false;
    this.followBuf = [];
    this.followOff = null;
    this.loaded = false;
    this.streamEl?.removeEventListener("scroll", this.onScrollFill);
    if (this.turnFold) {
      this.streamEl.removeEventListener("scroll", this.turnFold.releaseOnScroll);
      this.turnFold.dispose(); // 断观察、在途那趟作废
      this.turnFold = null;
    }
    if (this.streamEl) releaseEnhanceRoot(this.streamEl); // 上一个会话的卡随 IO 一起放掉
    if (this.stream) {
      this.stream.dispose();
      this.stream = null;
    }
    // 骨架随会话走
    this.skeleton?.dispose();
    this.skeleton = null;
    this.inflight.clear();
    this.drawn.clear();
    this.liveLast = -1;
    this.total = 0;
    this.renderCtx = null;
    this.renderSink = null;
    this.folder = null;
    // 清单也跟着释放：留着就是上一个会话的句子挂在下一个会话上、点下去找不到卡。`reset` 同时让在途那趟回来后不许回写。
    this.outline?.reset();
    if (this.said && popoverOpenOn(this.saidBtn)) closePopover();
    // 查找那一半同理：结果清空、在途那趟作废、收起。
    this.find?.reset();
  }


  // === DOM ===

  /** 头两行（标题 ＋ 徽标 ｜ 恢复 ▾ · 新窗口 · ⋯；项目 · 机器 · 路径 · 时间段 · 条数）· 错误条 · 工具行 · 消息流 · 底一行。 */
  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "session-viewer";

    const head = document.createElement("div");
    head.className = sv.svHead;
    const top = document.createElement("div");
    top.className = sv.svHeadTop;
    this.titleEl = document.createElement("div");
    this.titleEl.className = sv.svTitle;
    this.titleEl.dataset.role = "title";
    this.badgesEl = document.createElement("span");
    this.badgesEl.className = sv.svBadges;
    this.actionsEl = document.createElement("div");
    this.actionsEl.className = sv.svActions;
    this.leadEl = document.createElement("span");
    this.leadEl.className = sv.svLead;
    top.append(this.leadEl, this.titleEl, this.badgesEl, this.actionsEl);
    this.metaEl = document.createElement("div");
    this.metaEl.className = sv.svMeta;
    this.metaEl.dataset.role = "meta";
    this.countEl = document.createElement("span");
    this.metaEl.appendChild(this.countEl);
    head.append(top, this.metaEl);
    view.appendChild(head);

    this.bannerEl = document.createElement("div");
    this.bannerEl.className = sv.svBanner;
    view.appendChild(this.bannerEl);
    this.showBanner(null);

    // 工具行：「你说过的话 · N ▾」（浮层列出清单，点一句跳过去）· 会话内查找（Ctrl+F）。
    const tools = document.createElement("div");
    tools.className = sv.svTools;
    this.toolsEl = tools;
    this.said = new UserInputPanel({
      jumpTo: (uuid) => {
        closePopover();
        return this.scrollToMessage(uuid);
      },
      unjumpableHint: copyText("sessionViewer.build.unjumpable"),
      // 开合归本查看器（工具行那颗按钮 ＋ kit 浮层）；面板自带的开关不挂。
      openOutline: () => this.openSaid(),
    });
    this.saidBtn = button({ label: copyText("sessionViewer.tools.saidLabel"), icon: "list", size: "compact", onClick: () => this.openSaid() });
    this.saidText = document.createElement("span");
    this.saidBtn.querySelector("span")?.replaceWith(this.saidText);
    this.saidBtn.appendChild(icon("caretDown", "compact"));
    this.saidBtn.setAttribute("aria-haspopup", "dialog");
    this.saidBtn.setAttribute("aria-expanded", "false");
    this.saidBtn.dataset.role = "said";
    this.saidPop = document.createElement("div");
    this.saidPop.className = sv.svSaidPop;
    // 清单平时住这一格（清单自己收着 ⇒ 这一格不占地方）。
    this.saidHold = document.createElement("div");
    this.showSaidPanel(false);
    this.saidHold.appendChild(this.said.panel);
    view.appendChild(this.saidHold);
    // 清单是 `OutlineSource` 往面板里写的（整表 / 增量 / 要不到）⇒ 面板一变，按钮跟着变。
    new MutationObserver(() => this.syncSaid()).observe(this.said.panel, { childList: true });
    new MutationObserver(() => this.syncSaid()).observe(this.said.toggle, { attributes: true, attributeFilter: ["title"] });
    this.outline = new OutlineSource(this.said, () => this.outlineWhere);
    this.syncSaid();

    // 查找：问那台后端 `history-find`（经通道，`session-reads.ts::findInSession`），问的是查看器此刻这一份会话；
    // 跳与「你说过的话」同一个住址（`scrollToMessage`）。
    this.find = new FindStrip({
      search: async (query, includeTools, skip) => {
        const where = this.outlineWhere;
        if (!where) return { available: false, reason: "", hits: [], total: 0 };
        return findInSession(where.origin, where.jsonlPath, query, includeTools, skip);
      },
      jumpTo: (uuid) => this.scrollToMessage(uuid),
      unjumpableHint: copyText("sessionViewer.build.unjumpable"),
    });
    tools.append(this.saidBtn, this.find.box);
    view.appendChild(tools);
    // 命中清单：工具行正下方就地展开。
    view.appendChild(this.find.strip);

    // 读取中的骨架：外面包一层（kit 骨架自己写了 display，`hidden` 切在这一层上）。
    this.loadingEl = document.createElement("div");
    this.loadingEl.className = sv.svLoading;
    this.loadingEl.appendChild(skeletonRows(5));
    view.appendChild(this.loadingEl);
    this.setLoading(false);

    // 消息流容器（与实时 Tab 用相同的 .stream 样式）
    this.streamEl = document.createElement("div");
    this.streamEl.className = "stream session-viewer-stream";
    view.appendChild(this.streamEl);
    this.emptyEl = document.createElement("div");
    this.emptyEl.className = sv.svEmpty;
    this.emptyEl.dataset.role = "empty";
    this.emptyEl.hidden = true;
    view.appendChild(this.emptyEl);
    // 滚回底部 ⇒「↓ 新内容」收起（贴底与否由 `MessageStream` 按滚动判）。
    this.streamEl.addEventListener(
      "scroll",
      () => {
        if (this.stream?.stuckToBottom) this.showNewPill(false);
      },
      { passive: true },
    );

    // 「↓ 新内容」：浮在消息流底边的正中（零高的一行，按钮往上浮）。
    const pillRow = document.createElement("div");
    pillRow.className = sv.svPillRow;
    this.newPill = button({
      label: copyText("sessionViewer.stream.newContent"),
      icon: "arrowDown",
      size: "compact",
      onClick: () => {
        this.stream?.scrollToBottom();
        this.showNewPill(false);
      },
    });
    this.newPill.classList.add(sv.svPill);
    this.newPill.dataset.role = "new-content";
    this.newPill.hidden = true;
    pillRow.appendChild(this.newPill);
    view.appendChild(pillRow);

    this.statusEl = document.createElement("div");
    this.statusEl.className = sv.svFoot;
    this.statusEl.dataset.role = "status";
    view.appendChild(this.statusEl);

    if (this.shape.window) this.reshapeForWindow(view, head, tools, pillRow, this.shape.window.foot);
    return view;
  }

  /**
   * 窗口那一形：头不画（窗口的细顶栏担）；工具行平时收着（Ctrl+F 才露）；「你说过的话」常在左边一栏（当前读到的那句高亮）；
   * 底一行写进窗口的状态栏。
   */
  private reshapeForWindow(view: HTMLElement, head: HTMLElement, tools: HTMLElement, pillRow: HTMLElement, foot: HTMLElement): void {
    head.hidden = true;
    tools.hidden = true;
    this.saidBtn.remove();
    const body = document.createElement("div");
    body.className = sv.svWinBody;
    const side = document.createElement("aside");
    side.className = sv.svSide;
    side.dataset.role = "side";
    const sideHead = document.createElement("div");
    sideHead.className = sv.svSideHead;
    side.append(sideHead, this.said.panel);
    this.showSaidPanel(true);
    const main = document.createElement("div");
    main.className = sv.svWinMain;
    main.append(this.bannerEl, tools, this.find.strip, this.loadingEl, this.streamEl, this.emptyEl, pillRow);
    body.append(side, main);
    this.statusEl.remove();
    foot.replaceChildren();
    foot.dataset.role = "status";
    this.statusEl = foot;
    view.appendChild(body);
    this.sideBody = body;
    this.sideHead = sideHead;
    this.syncSaid();
    // 读到哪一句：滚动时把视口顶上那一句之前最近的一句标成当前。一帧里来好几个 scroll ⇒ 帧里量一趟。
    let queued = false;
    this.streamEl.addEventListener(
      "scroll",
      () => {
        if (queued) return;
        queued = true;
        // 调度：合批 —— 一帧里的几个 scroll 合成一次量
        requestAnimationFrame(() => {
          queued = false;
          const cur = this.currentSaid();
          for (const row of this.said.panel.querySelectorAll<HTMLElement>(".user-input-row")) {
            if (row === cur) row.setAttribute("aria-current", "true");
            else row.removeAttribute("aria-current");
          }
        });
      },
      { passive: true },
    );
  }
}
