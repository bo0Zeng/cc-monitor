/**
 * **大折叠（按段）**：一轮里连续的工具调用 · 思考 · 派 agent 与交回 · 后台通知 · 重试 · 来话收进一行
 * 「过程 · 工具 ×8 · 思考 ×1 · agent ×2 · 失败 ×1 · 02:01–02:04 · 3m02s」；带正文的（中间说的话与结论）和停下这一轮的那一条
 * （中断标记 / 报错卡）露着、把过程隔成几段，一段一行。正在跑的那一轮的尾巴那一段也一样折着，行上写「现在：…」「等你批准：…」。
 * 一轮的边界、哪几条露着、怎么划段、行上的字与语气、右端那一截，**都是后端的 `history-turns`**（`session-reads.ts::readTurns`，
 * 划段只住 `observe/recognize/turns.rs`）；本模块只做两件事：要那份成品（续取从还没收尾的那一轮的 `at` 起，整轮重算），以及按它给流里已建好的卡排版。
 *
 * # 排版的口径（界面不判谁是过程、不自己划段）
 *
 * - 顺着流的顶层子节点走：一张卡的 `data-id` 是某一轮开头 / 露着的某一条 / 某一段的 `until` ⇒ 它是边界，露着；某一段的 `after` 是它 ⇒ 那一段的行插在它后面、
 *   之后的卡归这一段，直到下一个边界。不是哪一段的 `after` ⇒ 之后的卡不归段（露着）。
 * - 遇到骨架占位：归哪一段、藏不藏不顺着 DOM 猜，按账本认（`foldSpans`：一段 ＝ 后端给的 `after` 之后到 `until` 之前的那几行）——
 *   整块落在某一段里 ⇒ 折着就藏（高 0、骨架也不物化它，同一个口径交给骨架）；里面夹着边界 ⇒ 露着；之后的卡归占位末尾所在的那一段
 *   （那一段的 `after` 还没建 ⇒ 行等它建出来再画）；骨架没接上 ⇒ 之后不再归段，直到认出下一个边界。
 * - ESC 回退的折叠段之后不再归段（宁可不折，不折错）。
 * - 过程行插在那一段的 `after` 那张卡后面；它不是时间线条目（`RecordTimeline` 按后继锚插入，不受它影响），
 *   `BranchFolder` 认的是 `data-id`，过程行没有。
 *
 * # 折着的不建 DOM（骨架接上了的那一段历史）
 *
 * 折着的段（`foldSpans` 的口径）交给骨架（`SkeletonView.setFolds`）：账本按 0 高、滚到那里也不物化，
 * 每段的 `after` 那一行多算一条过程行的高 ⇒ 滚动条按「人话 ＋ 正文 ＋ 每段一行」估。展开 ⇒ 撤掉那一段、叫骨架物化露出来的部分。
 * 已经建了的卡（尾部窗口直渲的 · 展开过又收起的）照样由本模块藏（`display:none`），收起后不拆。
 *
 * # 展开状态
 *
 * 默认展开与否是每扇窗一个开关（会话头「⋯」·`Ctrl+O`，`LS_KEYS.processExpanded`），正在跑的与收尾了的同一个默认；
 * 点过程行 ⇒ 这一段单独记住（这扇窗的内存，不落盘；键是段的 `after`），实时更新与跑完都不重置——产品从不替人开合，
 * 除了跳卡 / 查找落在折着的过程里（`revealCard` 发 `PROC_REVEAL_EVENT`，本模块展开那一段、记成手动开）。
 * 切开关 ⇒ 各段单独记的作废（`Ctrl+O` ＝ 全部收起 / 全部展开）。
 *
 * 本模块不排定时器（`polling_registry`）：触发全靠宿主（新记录到了 · 批结束 · 会话事实变了 · 切 tab）与 DOM 变动（`MutationObserver`）。
 */
import { readTurns, type TurnsResult } from "./session-reads";
import type { FoldSegment } from "./generated/FoldSegment";
import type { TurnRow } from "./generated/TurnRow";
import type { TurnSpan } from "./generated/TurnSpan";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { clockNow } from "./duration-format";
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
/** 过程行占的高（还没建出一条可量时用）：28 高 ＋ 12 下边距。 */
const LINE_PX_FALLBACK = 40;
/** 连续要不到几次就不再要（同大纲 / 事实的口径）。 */
const MAX_FAILURES = 3;

/** 这份会话在哪（路径要等首条行回填 ⇒ 每次现取；拿不到 ⇒ 这一趟不要）。 */
export type TurnsWhere = () => { origin: Origin; jsonlPath: string } | null;
type Read = (origin: Origin, path: string, from: number) => Promise<TurnsResult>;
/**
 * 这个 tab 的骨架（没接上 ⇒ `null`）：占位里有没有边界按后端给的行号认；折着的段告诉它（按 0 高、不物化）；
 * 展开一段后叫它物化露出来的那部分。
 */
export type TurnsSkeleton = () => {
  ledger: TurnsLedger;
  fillVisible(): number;
  setFolds(f: TurnFolds): void;
} | null;
/** 骨架账本里本模块要的那几样：账本的头尾（段的两头行号是后端给的，落在账本外的那一段不折；`until` 是 `null` 的那一段到账本尾）。 */
type TurnsLedger = { base: number; endSeq: number };

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

const isOurs = (el: Element): boolean => el.classList.contains(PROC_LINE_CLASS) || el.classList.contains(PROC_TAIL_CLASS);

/** `revealCard` 用：`el` 落在某一段折着的过程里 ⇒ 让那一段展开（同步）。 */
export function revealProcessOf(el: HTMLElement, container: HTMLElement): void {
  let top: HTMLElement | null = el;
  while (top && top.parentElement && top.dataset.procOf === undefined && top !== container) top = top.parentElement;
  const key = top?.dataset.procOf;
  if (top && key !== undefined && top.classList.contains("proc-hidden")) {
    top.dispatchEvent(new CustomEvent(PROC_REVEAL_EVENT, { bubbles: true, detail: key }));
  }
}

/** 一段过程：键是它的 `after`（一条记录只会是一段的 `after`）。 */
interface FoldRef {
  key: string;
  turn: TurnRow;
  fold: FoldSegment;
}

export class TurnFold {
  private turns: TurnRow[] = [];
  /** 边界：每一轮的开头 ＋ 露着的那几条 ＋ 每一段的 `until`（uuid，都是后端给的）。 */
  private bounds = new Set<string>();
  /** 落成了人话的排队插话（排队卡的记录 id，后端 `landed`）。 */
  private landed = new Set<string>();
  /** 段的 `after` ⇒ 那一段。 */
  private byAfter = new Map<string, FoldRef>();
  private lines = new Map<string, HTMLButtonElement>();
  private tails = new Map<string, HTMLButtonElement>();
  /** 这一段单独点过：`true` 展开 / `false` 收起。 */
  private overrides = new Map<string, boolean>();
  private inflight = false;
  private again = false;
  private gen = 0;
  private failures = 0;
  private gaveUp = false;
  private readonly mo: MutationObserver;
  /** 流长高 / 变矮 ⇒ 展开那几段左边的竖线跟着改高（不排定时器）。 */
  private readonly ro: ResizeObserver | null;
  /** 上一次排版时每一段过程里的最后一张卡（竖线画到它底下、收起行接在它后面）。 */
  private lastOf = new Map<string, HTMLElement>();
  private expandedDefault: boolean;
  /**
   * 量过的过程行高（`linePx`）。每趟排版都会改过程行上的字（DOM 刚动过），这时再量就是一次强制排版 ⇒ 量出来就记着，
   * 流尺寸变了（字号 / 缩放 / 列宽 —— `ro`）才作废重量。还没量到（一条都没建出来）⇒ `null`，用 28 ＋ 12、不记。
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
  get all(): readonly TurnRow[] {
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
        // 续点不在了：回来的从 0 起 ⇒ `replaceFrom(0, …)` 把手上的全换掉（连同三张表）
      }
      if (!res.available) {
        if (++this.failures >= MAX_FAILURES) this.gaveUp = true;
        return;
      }
      this.failures = 0;
      this.replaceFrom(res.from, res.turns);
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

  /**
   * 正文重来、轮次接着续（宿主 `restartTail`）：接过上一代手上的那份成品与各段单独点过的开合，下一次续取从还没收尾那一轮起，
   * 不从 0 再要。上一代在途那一趟作废（它已 `dispose`），这一代之后照常 `refresh`。
   */
  adopt(prev: TurnFold): void {
    this.turns = prev.turns;
    this.bounds = prev.bounds;
    this.landed = prev.landed;
    this.byAfter = prev.byAfter;
    this.overrides = new Map(prev.overrides);
    this.expandedDefault = prev.expandedDefault;
    this.onTurns?.();
  }

  /**
   * 续取回来的那几轮换进手上那一份：`at ≥ from` 的旧轮连同它们记进三张表的那几格摘掉，新来的记进去 ——
   * 只碰变了的那几轮（续取一般就是还没收尾的那一轮），不按全部轮重建（几万轮的会话每批新行一次）。
   */
  private replaceFrom(from: number, fresh: readonly TurnRow[]): void {
    const keep = this.turns.findIndex((t) => t.at >= from);
    const cut = keep < 0 ? this.turns.length : keep;
    for (const t of this.turns.slice(cut)) this.forget(t);
    this.turns = [...this.turns.slice(0, cut), ...fresh];
    for (const t of fresh) this.remember(t);
  }

  /** 一轮记进三张表（边界 · 落成人话的排队 · 段）。`indexed` 记碰过几轮（判据数它）。 */
  private remember(turn: TurnRow): void {
    this.indexed++;
    this.bounds.add(turn.uuid);
    for (const u of turn.shown) this.bounds.add(u);
    for (const l of turn.landed) this.landed.add(l);
    for (const fold of turn.folds) {
      if (fold.until !== null) this.bounds.add(fold.until);
      this.byAfter.set(fold.after, { key: fold.after, turn, fold });
    }
  }

  private forget(turn: TurnRow): void {
    this.indexed++;
    this.bounds.delete(turn.uuid);
    for (const u of turn.shown) this.bounds.delete(u);
    for (const l of turn.landed) this.landed.delete(l);
    for (const fold of turn.folds) {
      if (fold.until !== null) this.bounds.delete(fold.until);
      if (this.byAfter.get(fold.after)?.turn === turn) this.byAfter.delete(fold.after);
    }
  }

  /** 到此刻记表 / 摘表碰过几轮（判据：续取一次只碰变了的那几轮）。 */
  indexed = 0;

  /** `Ctrl+O` / 会话头开关：默认展开与否；各段单独记的作废。 */
  setDefault(expanded: boolean, later = false): void {
    this.expandedDefault = expanded;
    this.overrides.clear();
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

  /** 按手上的那份成品给流里的卡排版（O(顶层卡数)）。 */
  apply(): void {
    this.staleDefault = false;
    const seen = new Set<string>();
    /** 每一段过程里的最后一张卡（收起行接在它后面）。 */
    const lastOf = new Map<string, HTMLElement>();
    let cur: FoldRef | null = null;
    const spans = this.foldSpans();
    for (const el of Array.from(this.content.children)) {
      if (!(el instanceof HTMLElement) || isOurs(el)) continue;
      // 排队插话已经落成了人话（后端判）：不再单独出。
      const queued = el.dataset.queuedId;
      if (queued !== undefined && el.classList.contains("queued-landed") !== this.landed.has(queued)) el.classList.toggle("queued-landed");
      const uuid = el.getAttribute("data-id");
      if (uuid !== null && (this.bounds.has(uuid) || this.byAfter.has(uuid))) {
        cur = this.byAfter.get(uuid) ?? null;
        if (cur) {
          seen.add(cur.key);
          this.placeLine(el, cur);
        }
        this.unmark(el);
        continue;
      }
      // 骨架占位：归哪一段、藏不藏按账本认（`foldSpans`，与交给骨架按 0 高的同一个口径），不顺着 DOM 猜。
      if (el.classList.contains(SKELETON_GAP_CLASS)) {
        const lo = Number(el.dataset.skeletonLo);
        const hi = Number(el.dataset.skeletonHi);
        if (!spans || !Number.isFinite(lo) || !Number.isFinite(hi)) {
          cur = null;
          this.unmark(el);
          continue;
        }
        const owner = spanAt(spans, lo);
        if (owner && hi <= owner.end) {
          this.mark(el, owner.ref);
          lastOf.set(owner.ref.key, el);
        } else this.unmark(el);
        cur = spanAt(spans, hi)?.ref ?? null;
        continue;
      }
      if (el.classList.contains(FOLD_WRAP_CLASS)) cur = null;
      if (!cur) {
        this.unmark(el);
        continue;
      }
      this.mark(el, cur);
      lastOf.set(cur.key, el);
    }
    for (const [key, line] of this.lines) {
      if (seen.has(key)) continue;
      line.remove();
      this.lines.delete(key);
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
   * **每一段在账本里占哪几行**（骨架没接上 ⇒ `null`，按起点升序）：照后端那一段两头的行号取 ——
   * `after` 那一行之后、到 `until` 那一行之前（`until` 是 `null` ⇒ 到账本尾）；两头有一头落在账本外 ⇒ 那一段不折。
   * 账本按 0 高的那几段（`foldSkeleton`）与占位藏不藏（`apply`）都从它来 ⇒ 两边说法不会分家。
   */
  private foldSpans(ledger: TurnsLedger | null = this.skeleton()?.ledger ?? null): FoldSpanRow[] | null {
    if (!ledger) return null;
    const inside = (s: number): boolean => s >= ledger.base && s < ledger.endSeq;
    const rows: FoldSpanRow[] = [];
    for (const ref of this.byAfter.values()) {
      const at = ref.fold.afterLine;
      const until = ref.fold.untilLine;
      const end = until ?? ledger.endSeq;
      if (!inside(at) || (until !== null && !inside(until)) || end <= at) continue;
      rows.push({ ref, line: at, start: at + 1, end });
    }
    return rows.sort((a, b) => a.start - b.start);
  }

  /** 折着的段（`foldSpans` 的口径）交给骨架：按 0 高、滚到那里也不物化；每段的 `after` 那一行多算一条行高。 */
  private foldSkeleton(spans: readonly FoldSpanRow[] | null): void {
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
    const spans = this.foldSpans(ledger);
    if (spans) ledger.setFolds(this.foldsOf(spans));
  }

  private foldsOf(spans: readonly FoldSpanRow[]): TurnFolds {
    const folded: Array<[number, number]> = [];
    const lines: number[] = [];
    for (const r of spans) {
      lines.push(r.line);
      if (!this.expanded(r.ref) && r.end > r.start) folded.push([r.start, r.end]);
    }
    return { folded, lines: lines.sort((a, b) => a - b), linePx: this.linePx() };
  }
  /** 一条过程行在流里占多高（量第一条建出来的：盒高 ＋ 下边距；还没有 ⇒ 28 ＋ 12）。量过就记着（`linePxSeen`）。 */
  private linePx(): number {
    if (this.linePxSeen !== null) return this.linePxSeen;
    const line = this.lines.values().next().value;
    if (line?.isConnected) {
      const h = line.getBoundingClientRect().height;
      if (h > 0) return (this.linePxSeen = h + parseFloat(getComputedStyle(line).marginBottom || "0"));
    }
    return LINE_PX_FALLBACK;
  }

  /** 这一段从折着变成展开 ⇒ 露出来的占位要物化（没有 scroll 事件来叫）。 */
  private opened(): void {
    this.skeleton()?.fillVisible();
  }

  private readonly onReveal = (e: Event): void => {
    const key = (e as CustomEvent<string>).detail;
    if (!this.byAfter.has(key)) return;
    this.overrides.set(key, true);
    this.apply();
    this.opened();
  };

  private expanded(ref: FoldRef): boolean {
    return this.overrides.get(ref.key) ?? this.expandedDefault;
  }

  /** 归属没变的卡一个属性都不写：`data-proc-of` 有样式挂着，照写一遍同样的值也让那张卡重算样式（每次卡进出流全部过程卡一起算）。 */
  private mark(el: HTMLElement, ref: FoldRef): void {
    if (el.dataset.procOf !== ref.key) el.dataset.procOf = ref.key;
    el.classList.toggle("proc-hidden", !this.expanded(ref));
  }

  private unmark(el: HTMLElement): void {
    if (el.dataset.procOf === undefined) return;
    delete el.dataset.procOf;
    el.classList.remove("proc-hidden");
  }

  /** 过程行紧跟在那一段的 `after` 那张卡后面（已在就原地重写字）。 */
  private placeLine(anchor: HTMLElement, ref: FoldRef): void {
    let line = this.lines.get(ref.key);
    if (!line) {
      const key = ref.key;
      line = document.createElement("button");
      line.type = "button";
      line.className = PROC_LINE_CLASS;
      line.dataset.fold = key;
      line.addEventListener("click", (e) => this.onLineClick(key, e));
      this.lines.set(key, line);
    }
    if (anchor.nextElementSibling !== line) anchor.after(line);
    paintLine(line, ref.fold, this.expanded(ref), Date.now());
  }
  /**
   * 展开的那几段：过程高过一屏 ⇒ 末尾一行「‹ 收起这段过程」（别的段的收起行摘掉）；过程行下面那道竖线画到这一段过程（含收起行）的底。
   * 先量后写、成块：量一下写一下 ＝ 每一段都逼一次整页重排（`Ctrl+O` 全展开时几百段，一下几百毫秒）。
   * 新插进去 / 挪了位置的收起行要插完才量得到 ⇒ 再量一块；没动的沿用头一块的读数（它前面插 / 摘别的段的收起行，
   * 过程行与它一起挪，差不变；竖线是绝对定位的，插它不动排版）。
   */
  private placeTailsAndRules(lastOf: Map<string, HTMLElement>): void {
    const open: Array<{ uuid: string; line: HTMLElement; last: HTMLElement; span: number; tailSpan: number | null }> = [];
    for (const [uuid, line] of this.lines) {
      const ref = this.byAfter.get(uuid);
      const last = lastOf.get(uuid);
      if (ref && last && this.expanded(ref)) open.push({ uuid, line, last, span: 0, tailSpan: null });
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
    const ref = this.byAfter.get(uuid);
    if (!ref) return;
    // 点「失败 ×N」那一段 ⇒ 展开并滚到第一处失败；整行其余地方点 ＝ 开合。
    const onFail = e.target instanceof Element && e.target.closest(".proc-fails") !== null;
    if (onFail) {
      this.overrides.set(uuid, true);
      this.apply();
      this.opened();
      this.firstFailure(uuid)?.scrollIntoView({ block: "center" });
      return;
    }
    const open = !this.expanded(ref);
    this.overrides.set(uuid, open);
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

}

/** 一段在账本里：`after` 那一行 · 过程那几行 `[start, end)`。 */
interface FoldSpanRow {
  ref: FoldRef;
  line: number;
  start: number;
  end: number;
}

/** `seq` 落在哪一段里（`start ≤ seq < end`；各段不相交、按起点升序）；不在任何一段里 ⇒ `null`。 */
function spanAt(spans: readonly FoldSpanRow[], seq: number): FoldSpanRow | null {
  let l = 0;
  let r = spans.length;
  while (l < r) {
    const m = (l + r) >>> 1;
    if (spans[m].start <= seq) l = m + 1;
    else r = m;
  }
  const s = spans[l - 1];
  return s !== undefined && seq < s.end ? s : null;
}

/** 右端那一截：后端写好的字，`{dur}` 填用时（`to` 缺 ⇒ 到 `now`）。 */
export function fillDur(span: TurnSpan, now: number): string {
  return span.from === null ? span.text : clockNow({ text: span.text, from: span.from, to: span.to }, now);
}

/** 每条过程行上一次画的是什么（`paintLine` 据此跳过没变的）。 */
const painted = new WeakMap<HTMLElement, string>();

/**
 * 过程行：`›`（＋ 在跑转圈 / 在等你琥珀点）＋ 后端写好的各段（按语气上色、段间分隔）＋ 靠右那一截。
 * 要画的跟上一次一字不差 ⇒ 不动（每趟排版都会走到这里；整行拆了重建会作废这几行的样式与排版，在跑的转圈也从头转）。
 */
export function paintLine(line: HTMLElement, fold: FoldSegment, open: boolean, now: number): void {
  const dur = fillDur(fold.span, now);
  const sep = copyText("kit.text.sep");
  const key = JSON.stringify([open, fold.phase, fold.parts, dur, sep]);
  if (painted.get(line) === key) return;
  painted.set(line, key);
  line.setAttribute("aria-expanded", String(open));
  line.dataset.phase = fold.phase;
  line.replaceChildren();
  line.appendChild(foldCaret());
  // 在跑：转圈取「运行中」那一色（--success），用 currentColor 跟着 .proc-run。
  if (fold.phase === "running") {
    const run = document.createElement("span");
    run.className = "proc-run";
    run.appendChild(spinner());
    line.appendChild(run);
  }
  if (fold.phase === "awaiting") line.appendChild(statusDot("needs-you", fold.parts[fold.parts.length - 1]?.text ?? "", "compact"));
  const text = document.createElement("span");
  text.className = "proc-text";
  line.appendChild(text);
  for (const p of fold.parts) {
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
