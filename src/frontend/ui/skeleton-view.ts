/**
 * 〔`设计/10` 骨架 · 子步 4〕**骨架层**：拿到索引就画出总高与滚动条，**只物化可见区**。
 *
 * # 形状
 *
 * 已渲染的卡照旧由 `RecordTimeline` 按 seq 排；**没物化的那些 seq 区间**各由一块占位
 * （`.stream-skeleton-gap`，内联高度 = `SkeletonLedger.heightOf(lo, hi)`）顶住。
 * 占位本身也是 timeline 里的一个条目（`seq = lo - 0.5`，`kind: "card"`），于是：
 * - 岛里的卡按 seq 二分插入时**自然落在占位之后、下一个已渲染条目之前**；
 * - 工具组后处理合并看的左邻居（`peekPrev`）遇到占位就停 —— 岛的第一张工具卡**不会**
 *   跨过一段没渲染的历史并进上面那个工具组；
 * - `BranchFolder` 把没有 `data-uuid` 的元素当断 run（它的头注第 12 行）—— 占位不会被折进回退段。
 *
 * # 🔴 「骨架快」的主语是「只物化可见区」
 *
 * 占位只是一个有高度的空 div，**快的是它后面那件事**：滚动时只把**与视口相交的那一段**
 * （± `OVERSCAN` 屏）交给宿主建卡，其余的一张卡都不建。先全量渲染再盖一层骨架是假骨架 ——
 * 判据 `tests/frontend/ui/skeleton-view.vitest.ts` 的「只物化可见区」那一组钉的就是这件事。
 *
 * # 滚动补偿（手动，同 `fillAbove` 的纪律）
 *
 * 物化会把一段估高换成真高。规则：**视口里若有已渲染的卡，就钉住最上面那张的屏幕位置**；
 * 视口整个落在占位里就不补偿（占位上半段高度不变 ⇒ 视口停在它估算的那一行）。
 * 补偿期间关原生 `overflow-anchor`（WebView2 原生锚定与手动补偿叠加会 double-shift；
 * WebKitGTK 本来就没有），`finally` 里还原。
 *
 * # 买不到
 *
 * - **精度**：占位高是第一级粗估，滚到那里、建了卡才换成真高（`applyIntrinsicSize` ＋ `contain-intrinsic-size: auto`）；
 *   〔RENDER2〕视口上下几屏之内的占位行由 Worker 精算（第二级，`applyRefined` · `nearbyUnrefined`，宿主 `tab-stream-view.ts`），
 *   更远的一直是第一级。
 * - **正文仍从前端账本取**：宿主的 `materialize` 今天从 `TailWindow.pending` 拿 payload（重放已经推过来了）；
 *   没到的行先空着，到了由宿主按 `isPending` 判定直接建卡。「骨架不带正文、按偏移取」那一半
 *   （`read_session_range`）命令已通，宿主侧的接线留给下一刀。
 * - 本模块**不排任何定时器**（`polling_registry` 对 rAF/setTimeout 逐文件计数）：
 *   触发全靠宿主转来的 scroll / resize 事件，一次调用内有界地多跑几轮收敛。
 */
import type { RecordTimeline } from "./record-timeline";
import { SkeletonLedger } from "./live-window";
import type { SkeletonFacts } from "./height-estimate";
import { copyText } from "./copy-table";

/** 视口上下各多物化多少屏（相对视口高）。 */
const OVERSCAN = 0.5;
/** 一轮最多物化多少行 —— 估高荒谬地偏小时（一屏装下几千行），宁可分几轮也别一次建几千张卡。 */
const MAX_ROWS_PER_PASS = 300;
/** 一次 `fillVisible` 最多跑几轮（每轮之后真高替换估高，视口里可能又露出占位）。 */
const MAX_PASSES = 4;

export const SKELETON_GAP_CLASS = "stream-skeleton-gap";

export interface SkeletonHost {
  /** 把 seq ∈ `[lo, hi)` 的记录建成卡（宿主自己决定正文从哪来；拿不到的行跳过）。 */
  materialize(lo: number, hi: number): void;
}

interface Gap {
  lo: number;
  hi: number;
  el: HTMLElement;
}

export class SkeletonView {
  private gaps: Gap[] = [];
  private disposed = false;

  constructor(
    readonly ledger: SkeletonLedger,
    private scrollEl: HTMLElement,
    private timeline: RecordTimeline,
    private host: SkeletonHost,
  ) {}

  /**
   * 骨架接上：`[0, renderedFrom)` 画成一块占位（`renderedFrom` = 已渲染后缀的最低 seq）。
   * 返回占位的像素高（0 ⇒ 没什么要顶的，本视图保持空）。
   */
  attach(renderedFrom: number): number {
    if (renderedFrom <= this.ledger.base) return 0;
    this.addGap(this.ledger.base, renderedFrom);
    return this.pendingHeight;
  }

  /**
   * 〔U3b〕骨架接上：任意一组**不相交**的 seq 区间画成占位（查看器用 —— 它首屏除了尾巴，
   * 可能还有一个深链岛，已渲染集不是后缀）。空区间与越出账本的部分照收（高按账本算，越界为 0）。
   *
   * 视口稳定：**钉住视口里最上面那张已渲染卡的屏幕位置**（占位可能同时插在它上方和下方，
   * 按 ΔscrollHeight 补偿只对「全插在上方」成立）；视口里没有已渲染卡就不补偿。
   * 同一个同步任务里测 → 插 → 回写，期间关原生锚定、`finally` 还原（同 `fillAbove` 的纪律）。
   * 返回占位的像素高合计。
   */
  attachGaps(gaps: ReadonlyArray<readonly [number, number]>): number {
    const el = this.scrollEl;
    const anchor = this.firstVisibleIn(el.querySelector<HTMLElement>(".stream-content"));
    const anchorTop = anchor ? anchor.getBoundingClientRect().top : 0;
    try {
      el.style.overflowAnchor = "none";
      for (const [lo, hi] of gaps) if (hi > lo) this.addGap(lo, hi);
      if (anchor && anchor.isConnected) {
        const delta = anchor.getBoundingClientRect().top - anchorTop;
        if (delta !== 0) el.scrollTop += delta;
      }
    } finally {
      el.style.overflowAnchor = "";
    }
    return this.pendingHeight;
  }

  /** 这个 seq 还在占位里（没物化）吗 —— 宿主据此决定迟到的行直接建卡还是收纳。 */
  isPending(seq: number): boolean {
    return this.gaps.some((g) => seq >= g.lo && seq < g.hi);
  }

  /** 占位里还有多少行 */
  get pendingRows(): number {
    return this.gaps.reduce((s, g) => s + (g.hi - g.lo), 0);
  }

  /** 占位的像素高合计 */
  get pendingHeight(): number {
    return this.gaps.reduce((s, g) => s + this.ledger.heightOf(g.lo, g.hi), 0);
  }

  get gapCount(): number {
    return this.gaps.length;
  }

  /** 索引续传接上新行之后，重算每块占位的高（行没变，高可能从 0 变成估值）。 */
  refreshHeights(): void {
    for (const g of this.gaps) g.el.style.height = `${this.ledger.heightOf(g.lo, g.hi)}px`;
  }

  /**
   * 〔RENDER2 · `设计/10 §2.5b` 第二级〕Worker 精算回来的高换进账本、占位跟着改高。视口钉法同物化：
   * 视口里有已渲染的卡 ⇒ 钉住它的屏幕位置；视口整个落在占位里 ⇒ 钉住那块占位的顶（它上面的改动不许把视口推走）。
   */
  applyRefined(entries: Iterable<readonly [number, number]>): void {
    if (this.disposed || !this.ledger.refine(entries)) return;
    const el = this.scrollEl;
    const anchor = this.visibleRenderedAnchor() ?? this.visibleGap();
    const anchorTop = anchor ? anchor.getBoundingClientRect().top : 0;
    try {
      el.style.overflowAnchor = "none";
      this.refreshHeights();
      if (anchor && anchor.isConnected) {
        const delta = anchor.getBoundingClientRect().top - anchorTop;
        if (delta !== 0) el.scrollTop += delta;
      }
    } finally {
      el.style.overflowAnchor = "";
    }
  }

  /**
   * 〔RENDER2 · 第二级「按需 ＋ 后台」〕视口上下各 `screens` 屏之内、还在占位里、没精算过的行（按 seq 升序）。
   * 视口拿不到布局 ⇒ 空（不猜）。远处的一行都不给 —— 「全量精算」那一版不建（`设计/10 §2.5b` 规模那一段）。
   */
  nearbyUnrefined(screens: number): number[] {
    const view = this.scrollEl.getBoundingClientRect();
    if (view.height <= 0) return [];
    const top = view.top - view.height * screens;
    const bottom = view.bottom + view.height * screens;
    const out: number[] = [];
    for (const g of this.gaps) {
      const r = g.el.getBoundingClientRect();
      if (r.bottom <= top || r.top >= bottom || r.height <= 0) continue;
      const i = this.ledger.seqAt(g.lo, g.hi, Math.max(0, top - r.top));
      const j = Math.min(g.hi, this.ledger.seqAt(g.lo, g.hi, Math.min(r.height, bottom - r.top)) + 1);
      for (let s = i; s < j; s++) if (!this.ledger.isRefined(s)) out.push(s);
    }
    return out;
  }

  /** 与视口相交的第一块占位（视口里没有已渲染卡时拿它当钉子）。 */
  private visibleGap(): HTMLElement | null {
    const view = this.scrollEl.getBoundingClientRect();
    if (view.height <= 0) return null;
    for (const g of this.gaps) {
      const r = g.el.getBoundingClientRect();
      if (r.bottom > view.top && r.top < view.bottom) return g.el;
    }
    return null;
  }

  /**
   * **只物化可见区**：找出与视口（± `OVERSCAN` 屏）相交的占位，只把相交那一段交给宿主建卡。
   * 返回本次物化的行数（含不建卡的零高行）。
   *
   * 视口拿不到布局（高 0：tab 不可见 / 测试环境没给几何）⇒ 什么都不做 —— **不猜**。
   */
  fillVisible(): number {
    if (this.disposed) return 0;
    let total = 0;
    for (let pass = 0; pass < MAX_PASSES && this.gaps.length > 0; pass++) {
      const n = this.fillOnce();
      if (n === 0) break;
      total += n;
    }
    return total;
  }

  /** 跳转用：确保 seq 附近 `±radius` 行已物化（不管在不在视口里）。 */
  ensure(seq: number, radius = 30): void {
    const lo = Math.max(this.ledger.base, seq - radius);
    const hi = Math.min(this.ledger.endSeq, seq + radius + 1);
    const ranges: Array<[Gap, number, number]> = [];
    for (const g of this.gaps) {
      const a = Math.max(g.lo, lo);
      const b = Math.min(g.hi, hi);
      if (b > a) ranges.push([g, a, b]);
    }
    this.materializeRanges(ranges, false);
  }

  dispose(): void {
    this.disposed = true;
    for (const g of this.gaps) {
      this.timeline.removeByElement(g.el);
      g.el.remove();
    }
    this.gaps = [];
  }

  // ── 内部 ──────────────────────────────────────────────────────────────

  private fillOnce(): number {
    const view = this.scrollEl.getBoundingClientRect();
    if (view.height <= 0) return 0;
    const over = view.height * OVERSCAN;
    const top = view.top - over;
    const bottom = view.bottom + over;
    const ranges: Array<[Gap, number, number]> = [];
    for (const g of this.gaps) {
      const r = g.el.getBoundingClientRect();
      if (r.bottom <= top || r.top >= bottom || r.height <= 0) continue;
      const y0 = Math.max(0, top - r.top);
      const y1 = Math.min(r.height, bottom - r.top);
      const i = this.ledger.seqAt(g.lo, g.hi, y0);
      let j = Math.min(g.hi, this.ledger.seqAt(g.lo, g.hi, y1) + 1);
      if (j - i > MAX_ROWS_PER_PASS) {
        // 估高荒谬地偏小：从**靠近已渲染内容的那一端**取（占位下沿贴着已渲染的尾巴时从下往上）
        const nearBottom = y1 >= r.height - 1;
        if (nearBottom) {
          ranges.push([g, j - MAX_ROWS_PER_PASS, j]);
          continue;
        }
        j = i + MAX_ROWS_PER_PASS;
      }
      if (j > i) ranges.push([g, i, j]);
    }
    return this.materializeRanges(ranges, true);
  }

  private materializeRanges(ranges: Array<[Gap, number, number]>, compensate: boolean): number {
    if (ranges.length === 0) return 0;
    const el = this.scrollEl;
    const anchor = compensate ? this.visibleRenderedAnchor() : null;
    const anchorTop = anchor ? anchor.getBoundingClientRect().top : 0;
    let n = 0;
    try {
      el.style.overflowAnchor = "none";
      for (const [g, i, j] of ranges) {
        this.host.materialize(i, j);
        this.splitGap(g, i, j);
        n += j - i;
      }
      if (anchor && anchor.isConnected) {
        const delta = anchor.getBoundingClientRect().top - anchorTop;
        if (delta !== 0) el.scrollTop += delta;
      }
    } finally {
      el.style.overflowAnchor = "";
    }
    return n;
  }

  /** 视口里最上面那张**已渲染**的元素（不是占位）；没有 ⇒ null。 */
  private visibleRenderedAnchor(): HTMLElement | null {
    return this.firstVisibleIn(this.gaps[0]?.el.parentElement ?? null);
  }

  private firstVisibleIn(content: HTMLElement | null): HTMLElement | null {
    if (!content) return null;
    const view = this.scrollEl.getBoundingClientRect();
    if (view.height <= 0) return null;
    for (const child of Array.from(content.children)) {
      const c = child as HTMLElement;
      if (c.classList.contains(SKELETON_GAP_CLASS)) continue;
      const r = c.getBoundingClientRect();
      if (r.bottom > view.top && r.top < view.bottom) return c;
    }
    return null;
  }

  /** `[i, j)` 已物化：占位 `[lo, hi)` 收成 `[lo, i)` ＋ `[j, hi)`（空的那半拆掉）。 */
  private splitGap(g: Gap, i: number, j: number): void {
    const { lo, hi } = g;
    const idx = this.gaps.indexOf(g);
    if (idx < 0) return;
    if (i > lo) {
      g.hi = i;
      g.el.dataset.skeletonHi = String(i);
      g.el.style.height = `${this.ledger.heightOf(lo, i)}px`;
    } else {
      this.timeline.removeByElement(g.el);
      g.el.remove();
      this.gaps.splice(idx, 1);
    }
    if (j < hi) this.addGap(j, hi);
  }

  private addGap(lo: number, hi: number): void {
    const el = document.createElement("div");
    el.className = SKELETON_GAP_CLASS;
    el.setAttribute("aria-hidden", "true");
    el.dataset.skeletonLo = String(lo);
    el.dataset.skeletonHi = String(hi);
    el.style.height = `${this.ledger.heightOf(lo, hi)}px`;
    // `.stream-content > *` 统一挂了 `content-visibility: auto` ＋ 120px 兜底；占位的高度是显式的，
    // 不要容器尺寸约束来插手（styles.css 在 U1 手里拆，这里只能内联）。
    el.style.contentVisibility = "visible";
    this.timeline.insert({ seq: lo - 0.5, element: el, kind: "card", toolGroup: null });
    const gap: Gap = { lo, hi, el };
    const at = this.gaps.findIndex((x) => x.lo > lo);
    if (at < 0) this.gaps.push(gap);
    else this.gaps.splice(at, 0, gap);
  }
}

/** 从后端拿骨架索引的结局。 */
export type LedgerFetch =
  | { ok: true; ledger: SkeletonLedger; end: number }
  | { ok: false; reason: string };

/**
 * 把一次骨架索引的回包（〔C4b〕`session-reads.ts::readSessionIndex`）折成账本。`base` = 这份索引第一行的 seq
 * （冷启动 0；续传 = 旧账本的 `endSeq`，由调用方 `append`）。
 */
export function ledgerFromIndex(
  res: { available: boolean; reason?: string; end: number; rows: SkeletonFacts[] },
  base = 0,
): LedgerFetch {
  if (!res.available) return { ok: false, reason: res.reason ?? copyText("skeletonView.ledgerFromIndex.noIndex") };
  return { ok: true, ledger: new SkeletonLedger(base, res.rows), end: res.end };
}
