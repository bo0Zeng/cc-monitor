/**
 * **大折叠（按轮）**：两句人说的话之间，除了人说的，其余连续的东西收进一行「过程 · 工具 ×8 · 思考 ×1 · agent ×2 · 失败 ×1 · 02:01–02:04 · 3m02s」；
 * 只露这一段的**结尾**（结论正文；没有 ⇒ 停下它的中断标记 / 报错卡）。正在跑的那一轮也一样折着，行上写「现在：…」「等你批准：…」。
 * 一轮的边界、结尾是哪几条、行上的字与语气、右端那一截，**都是后端的 `history-turns`**（`session-reads.ts::readTurns`）；
 * 本模块只做两件事：要那份成品（续取从还没收尾的那一轮的 `at` 起，整轮重算），以及按它给流里已建好的卡排版。
 *
 * # 排版的口径（界面不判谁是过程）
 *
 * - 顺着流的顶层子节点走：遇到 `data-id` 等于某一轮 `uuid` 的那张卡 ＝ 那一轮开头；之后的卡属于这一轮，直到下一轮开头。
 * - 这一轮有过程行（后端给的 `parts` 非空）⇒ 开头之后、结尾以外的顶层卡全收进去；没有 ⇒ 一张不折。
 * - 遇到骨架占位：归哪一轮、藏不藏不顺着 DOM 猜，按账本那一段认（`turnSpans`：开头之后、下一轮开头之前、结尾以外的行是过程）——
 *   整块是某一轮的过程 ⇒ 折着就藏（高 0、骨架也不物化它，同一个口径交给骨架）；里面有哪一轮的开头 ⇒ 后面的卡归最后那个开头的轮
 *   （开头那张还没建 ⇒ 行等它建出来再画）；骨架没接上认不出 ⇒ 之后不再归轮，直到认出下一轮开头。
 * - ESC 回退的折叠段之后不再归轮（宁可不折，不折错；它后面的占位照样按账本认）。
 * - 过程行插在那一轮开头那张卡后面；它不是时间线条目（`RecordTimeline` 按后继锚插入，不受它影响），
 *   `BranchFolder` 认的是 `data-id`，过程行没有。
 * - 「暂定结论」被降级（露着的最后一段正文，Claude 又调了工具 ⇒ 它成了中间的话）：收尾那一刻视口正落在它上面 ⇒ 先不收，
 *   等滚出去（宿主转来的 scroll）或切走再收。
 *
 * # 折着的不建 DOM（骨架接上了的那一段历史）
 *
 * 折着的轮的过程行（`turnSpans` 的口径）交给骨架（`SkeletonView.setFolds`）：账本按 0 高、滚到那里也不物化，
 * 带过程行的轮的开头多算一条过程行的高 ⇒ 滚动条按「人话 ＋ 一行 ＋ 结尾」估。展开 ⇒ 撤掉那一段、叫骨架物化露出来的部分。
 * 已经建了的卡（尾部窗口直渲的 · 展开过又收起的）照样由本模块藏（`display:none`），收起后不拆。
 *
 * # 展开状态
 *
 * 默认展开与否是每扇窗一个开关（会话头「⋯」·`Ctrl+O`，`LS_KEYS.processExpanded`），正在跑的与收尾了的同一个默认；
 * 点过程行 ⇒ 这一轮单独记住（这扇窗的内存，不落盘），实时更新与跑完都不重置——产品从不替你开合，
 * 除了跳卡 / 查找落在折着的过程里（`revealCard` 发 `PROC_REVEAL_EVENT`，本模块展开那一轮、记成手动开）。
 * 切开关 ⇒ 各轮单独记的作废（`Ctrl+O` ＝ 全部收起 / 全部展开）。
 *
 * 本模块不排定时器（`polling_registry`）：触发全靠宿主（新记录到了 · 批结束 · 会话事实变了 · scroll · 切 tab）与 DOM 变动（`MutationObserver`）。
 */
import { readTurns, type TurnSummary, type TurnsResult, type TurnSpan } from "./session-reads";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { fmtStepDur } from "./cards/step-line";
import { foldCaret } from "./kit/fold";
import { spinner } from "./kit/progress";
import { statusDot } from "./kit/status-dot";
import { SKELETON_GAP_CLASS } from "./skeleton-view";
import type { TurnFolds } from "./live-window";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";

export const PROC_LINE_CLASS = "proc-line";
/** 展开的长过程末尾那一行「‹ 收起这段过程」。 */
export const PROC_TAIL_CLASS = "proc-tail";
/** 跳卡 / 查找落在折着的过程里：目标所在的那张顶层卡上发这个事件（冒泡，`detail` ＝ 那一轮的 uuid），本模块同步展开那一轮。 */
export const PROC_REVEAL_EVENT = "proc-reveal";
/** ESC 回退段的外壳。 */
const FOLD_WRAP_CLASS = "branch-fold-wrap";
/** 过程里出了错的那一处（点「失败 ×N」滚到第一处）。 */
const FAILED_SELECTOR = '.step-line[data-state="failed"], .card-api-error, [data-failed="1"]';
/** 过程行占的高（还没建出一条可量时用）：稿上的 28 高 ＋ 12 下边距。 */
const LINE_PX_FALLBACK = 40;
/** 连续要不到几次就不再要（同大纲 / 事实的口径）。 */
const MAX_FAILURES = 3;

/** 这份会话在哪（路径要等首条行回填 ⇒ 每次现取；拿不到 ⇒ 这一趟不要）。 */
export type TurnsWhere = () => { origin: Origin; jsonlPath: string } | null;
type Read = (origin: Origin, path: string, from: number) => Promise<TurnsResult>;
/**
 * 这个 tab 的骨架（没接上 ⇒ `null`）：占位里有没有下一轮的开头按账本的 uuid→seq 认；折着的轮的过程那一段告诉它（按 0 高、不物化）；
 * 展开一轮后叫它物化露出来的那段。
 */
export type TurnsSkeleton = () => {
  ledger: TurnsLedger;
  fillVisible(): number;
  setFolds(f: TurnFolds): void;
} | null;
/** 骨架账本里本模块要的那几样：uuid→seq（认开头 / 结尾）、账本尾（最后一轮到哪）。 */
type TurnsLedger = { uuidToSeq: ReadonlyMap<string, number>; endSeq: number };

/** 「显示系统注入」这扇窗的开关（缺省不露）。 */
export function injectedShownDefault(): boolean {
  return safeGet(LS_KEYS.showInjected) === "1";
}

export function setInjectedShownDefault(on: boolean): void {
  safeSet(LS_KEYS.showInjected, on ? "1" : "0");
}

/** 「过程默认展开」这扇窗的开关（缺省收起）。 */
export function processExpandedDefault(): boolean {
  return safeGet(LS_KEYS.processExpanded) === "1";
}

export function setProcessExpandedDefault(on: boolean): void {
  safeSet(LS_KEYS.processExpanded, on ? "1" : "0");
}

const hasLine = (t: TurnSummary): boolean => t.parts.length > 0;
const isOurs = (el: Element): boolean => el.classList.contains(PROC_LINE_CLASS) || el.classList.contains(PROC_TAIL_CLASS);

/** `revealCard` 用：`el` 落在某一轮折着的过程里 ⇒ 让那一轮展开（同步）。 */
export function revealProcessOf(el: HTMLElement, container: HTMLElement): void {
  let top: HTMLElement | null = el;
  while (top && top.parentElement && top.dataset.procOf === undefined && top !== container) top = top.parentElement;
  const turn = top?.dataset.procOf;
  if (top && turn !== undefined && top.classList.contains("proc-hidden")) {
    top.dispatchEvent(new CustomEvent(PROC_REVEAL_EVENT, { bubbles: true, detail: turn }));
  }
}

export class TurnFold {
  private turns: TurnSummary[] = [];
  private byUuid = new Map<string, TurnSummary>();
  private lines = new Map<string, HTMLButtonElement>();
  private tails = new Map<string, HTMLButtonElement>();
  /** 这一轮单独点过：`true` 展开 / `false` 收起。 */
  private overrides = new Map<string, boolean>();
  /** 上一次排版时露着当结尾的那几张卡（据此认出「暂定结论被降级」）。 */
  private shownEnding = new Set<string>();
  /** 被降级、但那一刻视口落在它上面 ⇒ 先不收的卡（uuid；等滚出去 / 切走）。 */
  private held = new Set<string>();
  private inflight = false;
  private again = false;
  private gen = 0;
  private failures = 0;
  private gaveUp = false;
  private readonly mo: MutationObserver;
  /** 流长高 / 变矮 ⇒ 展开那几轮左边的竖线跟着改高（不排定时器）。 */
  private readonly ro: ResizeObserver | null;
  /** 上一次排版时每一轮过程里的最后一张卡（竖线画到它底下、收起行接在它后面）。 */
  private lastOf = new Map<string, HTMLElement>();
  private expandedDefault: boolean;
  /**
   * 量过的过程行高（`linePx`）。每趟排版都会改过程行上的字（DOM 刚动过），这时再量就是一次强制排版 ⇒ 量出来就记着，
   * 流尺寸变了（字号 / 缩放 / 列宽 —— `ro`）才作废重量。还没量到（一条都没建出来）⇒ `null`，用稿上的值、不记。
   */
  private linePxSeen: number | null = null;
  /** 默认开关换了、还没按新的排（`isStale`）。 */
  private staleDefault = false;

  constructor(
    private readonly content: HTMLElement,
    private readonly scroller: HTMLElement,
    private readonly where: TurnsWhere,
    private readonly read: Read = readTurns,
    private readonly skeleton: TurnsSkeleton = () => null,
  ) {
    this.expandedDefault = processExpandedDefault();
    // 卡进出流（实时到达 · 上翻补批 · 骨架物化 · ESC 折叠重排）⇒ 重排一遍；排版自己插的过程行 / 收起行不算。
    this.mo = new MutationObserver((recs) => {
      if (recs.some((r) => [...r.addedNodes, ...r.removedNodes].some((n) => !(n instanceof HTMLElement && isOurs(n))))) this.apply();
    });
    this.mo.observe(content, { childList: true });
    this.ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => {
      this.linePxSeen = null;
      this.placeTailsAndRules(this.lastOf);
    });
    this.ro?.observe(content);
    content.addEventListener(PROC_REVEAL_EVENT, this.onReveal);
  }

  /** 轮变了（刻度据此重排）。 */
  onTurns: (() => void) | null = null;

  /** 已知的轮（刻度 · 大纲用同一份）。 */
  get all(): readonly TurnSummary[] {
    return this.turns;
  }

  /** 又长了 / 会话事实变了：从还没收尾的那一轮（都收尾了 ⇒ 最后一轮）的开头再要一次；在途 ⇒ 回来后只补一趟。 */
  async refresh(): Promise<void> {
    if (this.gaveUp) return;
    if (this.inflight) {
      this.again = true;
      return;
    }
    const where = this.where();
    if (!where) return;
    this.inflight = true;
    const gen = this.gen;
    try {
      const open = this.turns.find((t) => !t.done) ?? this.turns[this.turns.length - 1];
      const from = open?.at ?? 0;
      let res = await this.read(where.origin, where.jsonlPath, from);
      if (gen !== this.gen) return;
      // 续点不在了（文件被截断 / 重写）⇒ 从 0 整份重要。
      if (!res.available && from > 0) {
        res = await this.read(where.origin, where.jsonlPath, 0);
        if (gen !== this.gen) return;
        if (res.available) this.turns = [];
      }
      if (!res.available) {
        if (++this.failures >= MAX_FAILURES) this.gaveUp = true;
        return;
      }
      this.failures = 0;
      this.turns = [...this.turns.filter((t) => t.at < res.from), ...res.turns];
      this.byUuid = new Map(this.turns.map((t) => [t.uuid, t]));
      this.apply();
      this.onTurns?.();
    } finally {
      if (gen === this.gen) {
        this.inflight = false;
        if (this.again) {
          this.again = false;
          void this.refresh();
        }
      }
    }
  }

  /** `Ctrl+O` / 会话头开关：默认展开与否；各轮单独记的作废。 */
  setDefault(expanded: boolean, later = false): void {
    this.expandedDefault = expanded;
    this.overrides.clear();
    this.held.clear();
    if (later) this.staleDefault = true;
    else this.apply();
  }

  /**
   * 默认开关换了、这一份还没按新的排（后台 tab：`setDefault(…, true)` 只记一笔）。看不见的 tab 二十几个一起排，
   * WebKitGTK 上 `Ctrl+O` 那一下要好几秒；宿主空闲时一个一个排掉（`flushStale`），切进来时还没排到就当场排。
   */
  get isStale(): boolean {
    return this.staleDefault;
  }

  flushStale(): void {
    if (this.staleDefault) this.apply();
  }

  /** scroll 监听（宿主挂 / 摘同一个引用）。 */
  readonly releaseOnScroll = (): void => this.release();

  /** 宿主转来的 scroll / 切走：先不收的那几张，视口已经不在它上面 ⇒ 收。 */
  release(force = false): void {
    if (this.held.size === 0) return;
    for (const uuid of [...this.held]) if (force || !this.cardInView(uuid)) this.held.delete(uuid);
    this.apply();
  }

  /** 按手上的那份成品给流里的卡排版（O(顶层卡数)）。 */
  apply(): void {
    this.staleDefault = false;
    const seen = new Set<string>();
    const ending = new Set<string>();
    /** 每一轮过程里的最后一张卡（收起行接在它后面）。 */
    const lastOf = new Map<string, HTMLElement>();
    let cur: TurnSummary | null = null;
    let curEnding = new Set<string>();
    const spans = this.turnSpans();
    const heads: Head[] | null = spans && spans.map((r) => ({ seq: r.head, uuid: r.turn.uuid }));
    for (const el of Array.from(this.content.children)) {
      if (!(el instanceof HTMLElement) || isOurs(el)) continue;
      const uuid = el.getAttribute("data-id");
      const turn = uuid ? this.byUuid.get(uuid) : undefined;
      if (turn) {
        cur = hasLine(turn) ? turn : null;
        curEnding = new Set(turn.ending);
        if (cur) {
          seen.add(turn.uuid);
          this.placeLine(el, turn);
        }
        this.unmark(el);
        continue;
      }
      // 骨架占位：归哪一轮、藏不藏按账本那一段认（`turnSpans`，与交给骨架按 0 高的同一个口径），不顺着 DOM 猜：
      // 整块落在某一轮的过程行里 ⇒ 它是那一轮的过程（折着就藏、骨架也不物化它）；里面有哪一轮的开头 ⇒ 后面的卡归最后那个开头的轮
      // （开头那张还没建 ⇒ 行先不画，建出来再画）；含那一轮的结尾行 ⇒ 露着（结尾要物化出来）；骨架没接上认不出 ⇒ 之后不再归轮。
      if (el.classList.contains(SKELETON_GAP_CLASS)) {
        const lo = Number(el.dataset.skeletonLo);
        const hi = Number(el.dataset.skeletonHi);
        const inside = heads === null ? undefined : lastHeadIn(el, heads);
        const owner = spans && inside === null ? spanAt(spans, lo) : null;
        const t = inside === null ? owner?.turn : inside === undefined ? undefined : this.byUuid.get(inside);
        cur = t && hasLine(t) ? t : null;
        curEnding = new Set(t?.ending ?? []);
        if (cur && owner && hi <= owner.next && !owner.endings.some((e) => e >= lo && e < hi)) {
          this.mark(el, cur);
          lastOf.set(cur.uuid, el);
        } else this.unmark(el);
        continue;
      }
      if (el.classList.contains(FOLD_WRAP_CLASS)) cur = null;
      if (!cur || (uuid !== null && curEnding.has(uuid))) {
        if (uuid !== null && cur) ending.add(uuid);
        this.unmark(el);
        continue;
      }
      this.mark(el, cur);
      lastOf.set(cur.uuid, el);
    }
    this.shownEnding = ending;
    for (const [uuid, line] of this.lines) {
      if (seen.has(uuid)) continue;
      line.remove();
      this.lines.delete(uuid);
    }
    this.lastOf = lastOf;
    this.foldSkeleton(spans);
    this.placeTailsAndRules(lastOf);
    this.mo.takeRecords();
  }

  /** 关 tab / 内容整份重来：断观察、在途那趟作废。 */
  dispose(): void {
    this.gen++;
    this.mo.disconnect();
    this.ro?.disconnect();
    this.content.removeEventListener(PROC_REVEAL_EVENT, this.onReveal);
    this.lines.clear();
    this.tails.clear();
  }

  /**
   * **每一轮在账本里占哪几行**（骨架没接上 ⇒ `null`）：开头的 seq（升序）、到下一轮开头（没有 ⇒ 账本尾）为止、其中结尾是哪几行。
   * 「哪几行是过程」只住这里：开头之后、下一轮开头之前、结尾以外的行。账本按 0 高的那几段（`foldSkeleton`）与占位藏不藏（`apply`）
   * 都从它推出来 ⇒ 两边说法不会分家。
   */
  private turnSpans(ledger: TurnsLedger | null = this.skeleton()?.ledger ?? null): TurnSpanRow[] | null {
    if (!ledger) return null;
    const seq = ledger.uuidToSeq;
    const rows: TurnSpanRow[] = [];
    for (const turn of this.turns) {
      const head = seq.get(turn.uuid);
      if (head !== undefined) rows.push({ turn, head, next: 0, endings: [] });
    }
    rows.sort((a, b) => a.head - b.head);
    rows.forEach((r, i) => {
      r.next = rows[i + 1]?.head ?? ledger.endSeq;
      r.endings = r.turn.ending
        .map((u) => seq.get(u))
        .filter((e): e is number => e !== undefined && e > r.head && e < r.next)
        .sort((a, b) => a - b);
    });
    return rows;
  }

  /** 折着的轮的过程行（`turnSpans` 的口径）交给骨架：按 0 高、滚到那里也不物化；带过程行的开头多算一条行高。 */
  private foldSkeleton(spans: readonly TurnSpanRow[] | null): void {
    const sk = this.skeleton();
    if (!sk || !spans) return;
    sk.setFolds(this.foldsOf(spans));
  }

  /**
   * **骨架接上之前**先把折叠交给它的账本（宿主 `attachSkeleton` 在插占位之前调）：占位插进去就是折后的高。
   * 不先交 ⇒ 占位按「没折」的高插进去、补可见区去建折着的过程行，DOM 变动回调排完版账本才拿到折叠 ⇒ 占位改高、按视口钉，
   * 冷切停住那一帧多排两次版。之后那一趟排版交的是同一份 ⇒ 账本说没变、什么都不动。
   */
  seedFolds(ledger: TurnsLedger & { setFolds(f: TurnFolds): boolean }): void {
    const spans = this.turnSpans(ledger);
    if (spans) ledger.setFolds(this.foldsOf(spans));
  }

  private foldsOf(spans: readonly TurnSpanRow[]): TurnFolds {
    const folded: Array<[number, number]> = [];
    const lines: number[] = [];
    for (const r of spans) {
      if (!hasLine(r.turn)) continue;
      lines.push(r.head);
      if (!this.expanded(r.turn)) folded.push(...processRanges(r));
    }
    return { folded, lines, linePx: this.linePx() };
  }

  /** 一条过程行在流里占多高（量第一条建出来的：盒高 ＋ 下边距；还没有 ⇒ 稿上的 28 ＋ 12）。量过就记着（`linePxSeen`）。 */
  private linePx(): number {
    if (this.linePxSeen !== null) return this.linePxSeen;
    const line = this.lines.values().next().value;
    if (line?.isConnected) {
      const h = line.getBoundingClientRect().height;
      if (h > 0) return (this.linePxSeen = h + parseFloat(getComputedStyle(line).marginBottom || "0"));
    }
    return LINE_PX_FALLBACK;
  }

  /** 这一轮从折着变成展开 ⇒ 露出来的占位要物化（没有 scroll 事件来叫）。 */
  private opened(): void {
    this.skeleton()?.fillVisible();
  }

  private readonly onReveal = (e: Event): void => {
    const uuid = (e as CustomEvent<string>).detail;
    if (!this.byUuid.has(uuid)) return;
    this.overrides.set(uuid, true);
    this.apply();
    this.opened();
  };

  private expanded(turn: TurnSummary): boolean {
    return this.overrides.get(turn.uuid) ?? this.expandedDefault;
  }

  /** 归属没变的卡一个属性都不写：`data-proc-of` / 工具组的 `open` 都有样式挂着，照写一遍同样的值也让那张卡重算样式（每次卡进出流全部过程卡一起算）。 */
  private mark(el: HTMLElement, turn: TurnSummary): void {
    if (el.dataset.procOf !== turn.uuid) el.dataset.procOf = turn.uuid;
    const uuid = el.getAttribute("data-id");
    let open = this.expanded(turn);
    // 暂定结论被降级：此刻视口正落在它上面 ⇒ 先不收。
    if (!open && uuid !== null && (this.held.has(uuid) || (this.shownEnding.has(uuid) && this.inView(el)))) {
      this.held.add(uuid);
      open = true;
    }
    el.classList.toggle("proc-hidden", !open);
    // 过程里的工具组不再另起一层「工具 ×N」：展开的过程里它整组摊开（收着那一行由 CSS 藏掉）。
    if (open && el instanceof HTMLDetailsElement && el.classList.contains("card-tool-group") && !el.open) el.open = true;
  }

  private unmark(el: HTMLElement): void {
    if (el.dataset.procOf === undefined) return;
    delete el.dataset.procOf;
    el.classList.remove("proc-hidden");
  }

  /** 过程行紧跟在那一轮开头那张卡后面（已在就原地重写字）。 */
  private placeLine(head: HTMLElement, turn: TurnSummary): void {
    let line = this.lines.get(turn.uuid);
    if (!line) {
      line = document.createElement("button");
      line.type = "button";
      line.className = PROC_LINE_CLASS;
      line.dataset.turn = turn.uuid;
      line.addEventListener("click", (e) => this.onLineClick(turn.uuid, e));
      this.lines.set(turn.uuid, line);
    }
    if (head.nextElementSibling !== line) head.after(line);
    paintLine(line, turn, this.expanded(turn), Date.now());
  }

  /**
   * 展开的那几轮：过程高过一屏 ⇒ 末尾一行「‹ 收起这段过程」（别的轮的收起行摘掉）；过程行下面那道竖线画到这一轮过程（含收起行）的底。
   * 先量后写、成块：量一下写一下 ＝ 每一轮都逼一次整页重排（`Ctrl+O` 全展开时几百轮，一下几百毫秒）。
   * 新插进去 / 挪了位置的收起行要插完才量得到 ⇒ 再量一块；没动的沿用头一块的读数（它前面插 / 摘别的轮的收起行，
   * 过程行与它一起挪，差不变；竖线是绝对定位的，插它不动排版）。
   */
  private placeTailsAndRules(lastOf: Map<string, HTMLElement>): void {
    const open: Array<{ uuid: string; line: HTMLElement; last: HTMLElement; span: number; tailSpan: number | null }> = [];
    for (const [uuid, line] of this.lines) {
      const turn = this.byUuid.get(uuid);
      const last = lastOf.get(uuid);
      if (turn && last && this.expanded(turn)) open.push({ uuid, line, last, span: 0, tailSpan: null });
    }
    // 头一块：量（视口高 · 每一轮过程的跨度 · 已在原位的收起行底）
    const view = open.length > 0 ? this.scroller.clientHeight : 0;
    for (const o of open) {
      const lineBottom = o.line.getBoundingClientRect().bottom;
      o.span = o.last.getBoundingClientRect().bottom - lineBottom;
      const tail = this.tails.get(o.uuid);
      if (tail && o.last.nextElementSibling === tail) o.tailSpan = tail.getBoundingClientRect().bottom - lineBottom;
    }
    // 写：收起行进出
    const keep = new Set<string>();
    const fresh: typeof open = [];
    for (const o of open) {
      if (view === 0 || o.span <= view) continue;
      keep.add(o.uuid);
      let tail = this.tails.get(o.uuid);
      if (!tail) {
        const uuid = o.uuid;
        tail = document.createElement("button");
        tail.type = "button";
        tail.className = PROC_TAIL_CLASS;
        tail.append(foldCaret(), copyText("stream.proc.collapseTail"));
        tail.addEventListener("click", () => this.collapseFromTail(uuid));
        this.tails.set(uuid, tail);
      }
      if (o.last.nextElementSibling !== tail) {
        o.last.after(tail);
        fresh.push(o);
      }
    }
    for (const [uuid, tail] of this.tails) {
      if (keep.has(uuid)) continue;
      tail.remove();
      this.tails.delete(uuid);
    }
    // 写：竖线进出
    const ruled: Array<[HTMLElement, (typeof open)[number]]> = [];
    const byUuid = new Map(open.map((o) => [o.uuid, o]));
    for (const [uuid, line] of this.lines) {
      let rule = line.querySelector<HTMLElement>(":scope > .proc-rule");
      const o = byUuid.get(uuid);
      if (!o) {
        rule?.remove();
        continue;
      }
      if (!rule) {
        rule = document.createElement("span");
        rule.className = "proc-rule";
        line.appendChild(rule);
      }
      ruled.push([rule, o]);
    }
    // 第二块：只量刚插进去的收起行
    for (const o of fresh) o.tailSpan = this.tails.get(o.uuid)!.getBoundingClientRect().bottom - o.line.getBoundingClientRect().bottom;
    for (const [rule, o] of ruled) rule.style.height = `${Math.max(0, keep.has(o.uuid) ? (o.tailSpan ?? o.span) : o.span)}px`;
  }

  /** 末尾那一行收起：收完把折叠行放到收起行原来的屏幕位置（视口不跳）。 */
  private collapseFromTail(uuid: string): void {
    const tail = this.tails.get(uuid);
    const line = this.lines.get(uuid);
    if (!tail || !line) return;
    const before = tail.getBoundingClientRect().top;
    this.overrides.set(uuid, false);
    this.apply();
    this.scroller.scrollTop += line.getBoundingClientRect().top - before;
  }

  private onLineClick(uuid: string, e: MouseEvent): void {
    const turn = this.byUuid.get(uuid);
    if (!turn) return;
    // 点「失败 ×N」那一段 ⇒ 展开并滚到第一处失败；整行其余地方点 ＝ 开合。
    const onFail = e.target instanceof Element && e.target.closest(".proc-fails") !== null;
    if (onFail) {
      this.overrides.set(uuid, true);
      this.apply();
      this.opened();
      this.firstFailure(uuid)?.scrollIntoView({ block: "center" });
      return;
    }
    const open = !this.expanded(turn);
    this.overrides.set(uuid, open);
    for (const u of [...this.held]) if (this.cardOf(u)?.dataset.procOf === uuid) this.held.delete(u);
    this.apply();
    if (open) this.opened();
  }

  private firstFailure(uuid: string): HTMLElement | null {
    for (const el of Array.from(this.content.children)) {
      if (!(el instanceof HTMLElement) || el.dataset.procOf !== uuid) continue;
      if (el.matches(FAILED_SELECTOR)) return el;
      const hit = el.querySelector<HTMLElement>(FAILED_SELECTOR);
      if (hit) return hit;
    }
    return null;
  }

  /** 流里 `data-id` 是它的那张顶层卡。 */
  private cardOf(uuid: string): HTMLElement | null {
    for (const el of Array.from(this.content.children)) if (el instanceof HTMLElement && el.getAttribute("data-id") === uuid) return el;
    return null;
  }

  private cardInView(uuid: string): boolean {
    const el = this.cardOf(uuid);
    return el !== null && this.inView(el);
  }

  /** 这张卡与视口相交。 */
  private inView(el: HTMLElement): boolean {
    const box = this.scroller.getBoundingClientRect();
    if (box.height === 0) return false; // 后台 tab：不在眼前
    const r = el.getBoundingClientRect();
    return r.bottom > box.top && r.top < box.bottom;
  }
}

interface Head {
  seq: number;
  uuid: string;
}

/** 一轮在账本里：开头 · 下一轮开头（半开上界）· 结尾那几行（升序）。 */
interface TurnSpanRow {
  turn: TurnSummary;
  head: number;
  next: number;
  endings: number[];
}

/** 这一轮的过程行：`(head, next)` 里扣掉结尾那几行。 */
function processRanges(r: TurnSpanRow): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  let at = r.head + 1;
  for (const e of r.endings) {
    if (e > at) out.push([at, e]);
    at = Math.max(at, e + 1);
  }
  if (r.next > at) out.push([at, r.next]);
  return out;
}

/** `seq` 落在哪一轮里（开头 < seq < 下一轮开头）；在第一轮开头之前 ⇒ `null`。 */
function spanAt(spans: readonly TurnSpanRow[], seq: number): TurnSpanRow | null {
  let l = 0;
  let r = spans.length;
  while (l < r) {
    const m = (l + r) >>> 1;
    if (spans[m].head < seq) l = m + 1;
    else r = m;
  }
  const s = spans[l - 1];
  return s !== undefined && seq < s.next ? s : null;
}

/** 占位 `[lo, hi)` 里最后一个开头是哪一轮（`heads` 按 seq 升序）：没有 ⇒ `null`；占位的区间读不出 ⇒ `undefined`（当作认不出）。 */
function lastHeadIn(gap: HTMLElement, heads: readonly Head[]): string | null | undefined {
  const lo = Number(gap.dataset.skeletonLo);
  const hi = Number(gap.dataset.skeletonHi);
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return undefined;
  // 第一个 seq ≥ hi 的位置，前一个若 ≥ lo 就是占位里最后那个开头。
  let l = 0;
  let r = heads.length;
  while (l < r) {
    const m = (l + r) >>> 1;
    if (heads[m].seq < hi) l = m + 1;
    else r = m;
  }
  const h = heads[l - 1];
  return h !== undefined && h.seq >= lo ? h.uuid : null;
}

/** 右端那一截：后端写好的字，`{dur}` 填用时（`to` 缺 ⇒ 到 `now`）。 */
export function fillDur(span: TurnSpan, now: number): string {
  if (!span.text.includes("{dur}") || span.from === null) return span.text;
  return span.text.replace("{dur}", fmtStepDur((span.to ?? now) - span.from));
}

/** 每条过程行上一次画的是什么（`paintLine` 据此跳过没变的）。 */
const painted = new WeakMap<HTMLElement, string>();

/**
 * 过程行：`›`（＋ 在跑转圈 / 在等你琥珀点）＋ 后端写好的各段（按语气上色、段间分隔）＋ 靠右那一截。
 * 要画的跟上一次一字不差 ⇒ 不动（每趟排版都会走到这里；整行拆了重建会作废这几行的样式与排版，在跑的转圈也从头转）。
 */
export function paintLine(line: HTMLElement, turn: TurnSummary, open: boolean, now: number): void {
  const dur = fillDur(turn.span, now);
  const sep = copyText("kit.text.sep");
  const key = JSON.stringify([open, turn.phase, turn.parts, dur, sep]);
  if (painted.get(line) === key) return;
  painted.set(line, key);
  line.setAttribute("aria-expanded", String(open));
  line.dataset.phase = turn.phase;
  line.replaceChildren();
  line.appendChild(foldCaret());
  // 在跑：转圈取「运行中」那一色（规范 V10 · --success），用 currentColor 跟着 .proc-run。
  if (turn.phase === "running") {
    const run = document.createElement("span");
    run.className = "proc-run";
    run.appendChild(spinner());
    line.appendChild(run);
  }
  if (turn.phase === "awaiting") line.appendChild(statusDot("needs-you", turn.parts[turn.parts.length - 1]?.text ?? "", "compact"));
  const text = document.createElement("span");
  text.className = "proc-text";
  line.appendChild(text);
  for (const p of turn.parts) {
    if (text.childNodes.length > 0) text.append(sep);
    const s = document.createElement("span");
    if (p.tone === "fail") s.className = "proc-fails";
    if (p.tone === "now") s.className = "proc-now";
    if (p.tone === "need") s.className = "proc-need";
    s.textContent = p.text;
    text.appendChild(s);
  }
  text.title = text.textContent ?? "";
  const span = document.createElement("span");
  span.className = "proc-span";
  span.textContent = dur;
  line.appendChild(span);
}
