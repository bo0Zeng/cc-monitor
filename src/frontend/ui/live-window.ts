/**
 * 实时 Tab 尾部优先的窗口账本（纯数据，零 DOM；查看器不留正文，只经骨架按视口取）。
 *
 * 没接骨架的 tab：已渲染集恒为按 seq 连续的尾后缀 [floor, +∞)（单洞不变量）⇒ 一个 floor 水位 ＋ 待渲染数组就够。
 * 重放到达序是末块先发、块内升序，pending 是「按块降序、块内升序」的非连续集合；唯一的消费方式 takeTail（取 seq 最高的 k 条）前惰性 sort 一次。
 *
 * 接了骨架的 tab：骨架（`skeleton-view.ts`）按可见区取岛（`takeRange`），已渲染集 = 尾后缀 ∪ 若干岛，
 * 「哪些 seq 还没物化」以骨架的占位集（`SkeletonView.isPending`）为准，本类只是「还没建卡的 payload 放哪」的账本。
 */
import type { JsonlLinePayload } from "./events";
import {
  estimateFromFacts,
  skeletonKind,
  type SkeletonFacts,
  type SkeletonKind,
} from "./height-estimate";

/**
 * **一个 tab 见过的行号**（入口按 seq 去重的那一格），存成升序、互不相邻的半开区间 `[lo, hi)`。
 * 段数的上界：不可显示的行由 monitor 以 `skipped_from` 告知、取回的整段按 `[from, next)` 记 ⇒ 一份收全了的会话收成一段；
 * 余下的段只来自真没到过的洞（重放在途 · 丢格）与「最后一个可显示行之后那一截不可显示的」，不随会话长度涨。
 */
export class SeqSet {
  /** `[lo0, hi0, lo1, hi1, …]`，升序、段与段之间至少隔一个号。 */
  private edges: number[] = [];

  /** 段数（读数与判据用）。 */
  get segments(): number {
    return this.edges.length / 2;
  }

  get isEmpty(): boolean {
    return this.edges.length === 0;
  }

  /** 见过的最大行号（空 ⇒ -1）。 */
  get max(): number {
    return this.edges.length === 0 ? -1 : this.edges[this.edges.length - 1] - 1;
  }

  /** 见过的、比 `x` 小的最大行号（没有 ⇒ -1）。 */
  maxBelow(x: number): number {
    const i = this.segAtOrAfter(x - 1);
    if (i < this.edges.length / 2 && this.edges[2 * i] <= x - 1) return x - 1;
    return i === 0 ? -1 : this.edges[2 * i - 1] - 1;
  }

  /** 第一个 `hi > x` 的段的下标（段号，不是 edges 下标）。 */
  private segAtOrAfter(x: number): number {
    let l = 0;
    let r = this.edges.length / 2;
    while (l < r) {
      const m = (l + r) >>> 1;
      if (this.edges[2 * m + 1] <= x) l = m + 1;
      else r = m;
    }
    return l;
  }

  has(seq: number): boolean {
    const i = this.segAtOrAfter(seq);
    return i < this.edges.length / 2 && this.edges[2 * i] <= seq;
  }

  add(seq: number): void {
    this.addRange(seq, seq + 1);
  }

  /** 记下 `[lo, hi)`（空区间不动）；与相交 / 相邻的段并成一段。 */
  addRange(lo: number, hi: number): void {
    if (hi <= lo) return;
    // 第一个 hi >= lo 的段（相邻也并）与第一个 lo > hi 的段之间的全部并掉。
    const a = this.segAtOrAfter(lo - 1);
    let b = a;
    const n = this.edges.length / 2;
    while (b < n && this.edges[2 * b] <= hi) b++;
    if (a < b) {
      lo = Math.min(lo, this.edges[2 * a]);
      hi = Math.max(hi, this.edges[2 * b - 1]);
    }
    this.edges.splice(2 * a, 2 * (b - a), lo, hi);
  }

  clear(): void {
    this.edges = [];
  }
}

/** 升序 pending 里第一个 `seq >= x` 的下标 */
function lowerBound(arr: JsonlLinePayload[], x: number): number {
  let l = 0;
  let r = arr.length;
  while (l < r) {
    const m = (l + r) >>> 1;
    if (arr[m].seq < x) l = m + 1;
    else r = m;
  }
  return l;
}

/**
 * **账本（还没上屏的那些）的上界**：超过 {@link PENDING_CAP} 条就只留 seq 最高的
 * {@link PENDING_KEEP} 条，其余出账 —— 它们往上翻到时按行号取回（`fetchBelow`；接了骨架的按字节，`fetchMissingRows`）。
 *
 * 订阅侧缓冲都要有上界（没有的话一份 4 万行的会话，后台 tab 里就驻留 4 万个 payload）。
 * 取数：一次物化最多 600 条（`materializeUntilFilled` 150 × 4）、上翻一批 200 条 ⇒ 留 2000 条够首屏 ＋ 七八次上翻不用等 IPC；
 * 摊还余量 1000（与 monitor 那一侧 `TRIM_SLACK` 同一个道理：每来一条都修会让收纳路付 O(n)）。
 */
export const PENDING_KEEP = 2000;
export const PENDING_CAP = 3000;

/**
 * **账本之下还有没有行**。
 *
 * seq 就是行号、从 0 起 ⇒ 渲染窗口最老那一条的 seq > 0 且账本空了 ⇒ 下面**可能**还有
 * （被 monitor 的重放缓冲修剪掉的 · 被本账本修剪掉的 · 或只是几条不显示的记录）。
 * 不需要谁来告诉「修剪过」：修没修剪，做法一样 —— 按行号问一次（`read_session_lines`）。
 *
 * - `maybe`：还没问过（或上次问回来之后还没到 0）；
 * - `fetching`：问着；
 * - `none`：问到了第 0 行 —— 到顶了；
 * - `failed`：问不动（老后端不认 / 断了），带一句给人看的原因。第一次失败回 `maybe`
 *   （下一次上翻触发再问一次），**连续**第二次才落这里；此后不自动重问（上翻逐 scroll 事件触发，无界重问是重试环），
 *   切走再切回来（`retryBelow`）才再问一次。
 */
export type BelowState =
  | { kind: "maybe" }
  | { kind: "fetching" }
  | { kind: "none" }
  | { kind: "failed"; reason: string };

/** 一批的第二道闸：每条的分量（调用方给）与这一批的上限。 */
export interface TakeBudget {
  weight: (p: JsonlLinePayload) => number;
  max: number;
}

export class TailWindow {
  /** 窗口低水位;null = virgin(该 tab 尚未渲染任何 content 记录) */
  private floor: number | null = null;
  /** 未渲染 payload;尾追加免排序,乱序块标 dirty 惰性 sort */
  private pending: JsonlLinePayload[] = [];
  private dirty = false;
  /** 账本之下还有没有（见 {@link BelowState}） */
  private below: BelowState = { kind: "maybe" };
  /**
   * 按行号往下已经问到了第几行（上一问的 `from`）：下一问的上界是它与 floor 里小的那个。
   * **不能只看 floor**：问回来的那一段若全是不显示的记录，floor 不动 ⇒ 按 floor 算的下一问原地重问，
   * 而这一问又是在上一问的回调里同步发起的 ⇒ 一个不让出的无限循环（死值验 K5 首刀现打：vitest worker OOM）。
   * `null` = 没问过，或账本出过账（那些行要重新问，从 floor 起算）。
   */
  private askedDownTo: number | null = null;
  /**
   * 上一问的上界。下一问的上界必须**严格更小**，否则不问 —— 兜住「上界没往下走」的任何一种写法
   * （那一形在这里是一个同步发起的无限循环，不是慢一点）。与 {@link askedDownTo} 同时复位。
   */
  private lastUntil: number | null = null;
  /** 这一串失败里已经自动重问过一次了（问回来 / 切回来清掉）。 */
  private belowRetried = false;

  get floorSeq(): number | null {
    return this.floor;
  }

  /** 账本之下的状态（哨兵那句话按它说）。 */
  get belowState(): BelowState {
    return this.below;
  }

  /**
   * 该不该按行号往下问：账本空了、渲染窗口最老那一条不是第 0 行、还没问到顶、此刻没在问、上次没失败。
   * 问的区间是 `[max(0, 上界 − batch), 上界)`，上界 = min(floor, 上一问的 from)（{@link belowRange}）。
   */
  get wantsBelow(): boolean {
    return (
      this.pending.length === 0 && this.floor !== null && this.floor > 0 && this.below.kind === "maybe"
    );
  }

  /** 要问的那一段 `[from, until)`（`until` = min(渲染窗口最老那一条, 上一问的 from)）。没得问 ⇒ `null`。 */
  belowRange(batch: number): { from: number; until: number } | null {
    if (!this.wantsBelow || this.floor === null) return null;
    const until = Math.min(this.floor, this.askedDownTo ?? this.floor);
    if (until <= 0 || (this.lastUntil !== null && until >= this.lastUntil)) return null;
    return { from: Math.max(0, until - Math.max(1, batch)), until };
  }

  /** 开始问 `[…, until)`。 */
  markFetchingBelow(until: number): void {
    this.below = { kind: "fetching" };
    this.lastUntil = until;
  }

  /** 问回来了：问的是从第 `from` 行起 ⇒ `from == 0` 就到顶了，否则还可能有。 */
  markFetchedBelow(from: number): void {
    this.below = from <= 0 ? { kind: "none" } : { kind: "maybe" };
    this.askedDownTo = from;
    this.belowRetried = false;
  }

  /** 问不动。这一串里第一次 ⇒ 回 `maybe`（下一次触发再问同一段），第二次才 `failed`。 */
  markBelowFailed(reason: string): void {
    if (!this.belowRetried) {
      this.belowRetried = true;
      this.below = { kind: "maybe" };
      this.lastUntil = null; // 失败的那一问没取回东西：再问同一段是本意
      return;
    }
    this.below = { kind: "failed", reason };
  }

  /** 失败过的，允许再问一次（切走再切回来时调；其余状态原样）。 */
  retryBelow(): void {
    if (this.below.kind === "failed") {
      this.below = { kind: "maybe" };
      this.lastUntil = null; // 失败的那一问没取回东西：再问同一段是本意
      this.belowRetried = false;
    }
  }

  /**
   * 会话流里丢过格（`gap`）⇒ 账本里还没上屏的那些**不可信**（它们之间可能夹着洞，而 seq 里本来就有
   * 不显示的记录占的号，前端从 seq 看不出哪里缺）⇒ 整份出账，之后往上翻按行号重新取（`below` 回到 `maybe`）。
   * 返回丢掉的条数。
   */
  dropPending(): number {
    const n = this.pending.length;
    this.pending = [];
    this.dirty = false;
    this.askedDownTo = null;
    this.lastUntil = null;
    if (this.below.kind === "none" || this.below.kind === "failed") this.below = { kind: "maybe" };
    return n;
  }

  /** 压低水位(幂等取 min——物化/直渲只会让窗口向下扩) */
  pinFloor(seq: number): void {
    this.floor = this.floor === null ? seq : Math.min(this.floor, seq);
  }

  /** 该 seq 是否属于渲染窗口(virgin 时恒 false,由 caller 决定钉 floor 直渲还是收纳) */
  admit(seq: number): boolean {
    return this.floor !== null && seq >= this.floor;
  }

  /**
   * 收纳一条未渲染 payload。到达序通常块内升序 → 尾追加免排序。
   * 账本超过 {@link PENDING_CAP} ⇒ 只留 seq 最高的 {@link PENDING_KEEP} 条（出账的那些往上翻时按行号取回；
   * 「下面还有」这件事随之回到 `maybe`）。
   */
  defer(p: JsonlLinePayload): void {
    const last = this.pending[this.pending.length - 1];
    if (last !== undefined && p.seq < last.seq) this.dirty = true;
    this.pending.push(p);
    if (this.pending.length > PENDING_CAP) {
      this.keepHighest(PENDING_KEEP);
      this.askedDownTo = null; // 出账的那些要重新问（从 floor 起算）
      this.lastUntil = null;
      if (this.below.kind === "none") this.below = { kind: "maybe" };
    }
  }

  /**
   * 弹出 pending 中 seq 最高的 ≤k 条(升序返回,已出账),并把 floor 压到取出段
   * 的最低 seq——上翻补批/物化的口粮。空账返回 [],floor 不动。
   */
  takeTail(k: number, budget?: TakeBudget): JsonlLinePayload[] {
    if (this.pending.length === 0 || k <= 0) return [];
    if (this.dirty) {
      this.pending.sort((a, b) => a.seq - b.seq);
      this.dirty = false;
    }
    let from = Math.max(0, this.pending.length - k);
    if (budget) {
      // 条数与分量双闸、先到先停；至少取一条（单条超预算也要能前进）。
      let used = 0;
      let i = this.pending.length;
      while (i > from) {
        const w = budget.weight(this.pending[i - 1]);
        if (i < this.pending.length && used + w > budget.max) break;
        used += w;
        i--;
      }
      from = i;
    }
    const taken = this.pending.splice(from);
    if (taken.length > 0) this.pinFloor(taken[0].seq);
    return taken;
  }

  get pendingCount(): number {
    return this.pending.length;
  }

  /** 账本里这几个 seq 的 payload（不出账；没有的跳过）。第二级估高借正文用，看完就丢。 */
  peekSeqs(seqs: ReadonlySet<number>): JsonlLinePayload[] {
    return this.pending.filter((p) => seqs.has(p.seq));
  }

  /**
   * 只留 seq 最高的 `keep` 条，其余**出账丢弃**；返回丢掉的条数。
   *
   * 只许在**骨架接上之后**调：丢掉的那些从此只能按偏移要回来（`read_session_range`）。
   * 没骨架的 tab 调了它，上翻到头就没了。
   */
  keepHighest(keep: number): number {
    const k = Math.max(0, keep);
    if (this.pending.length <= k) return 0;
    if (this.dirty) {
      this.pending.sort((a, b) => a.seq - b.seq);
      this.dirty = false;
    }
    return this.pending.splice(0, this.pending.length - k).length;
  }

  /**
   * 〔骨架〕只读看几条 pending（**不出账**）—— 骨架接上之前用它对拍
   * 「seq 与索引行号是不是同一个空间」（截断重读换过 seq 的会话对不上，不许硬接）。
   */
  peek(n: number): readonly JsonlLinePayload[] {
    return this.pending.slice(0, Math.max(0, n));
  }

  /**
   * 〔骨架〕弹出 pending 里 seq ∈ `[lo, hi)` 的那些（升序、出账）。
   *
   * **不动 floor** —— 这是骨架「只物化可见区」取的**岛**，不是后缀；单洞后缀不变量从骨架接上那一刻起
   * 不再成立（见文件头注最后一段预言的那次升级）。`takeTail` 仍然可用：pending 恒在 floor 之下，
   * 被岛取走的行不在 pending 里，`takeTail` 取到的仍是 floor 之下最近的那几条。
   */
  takeRange(lo: number, hi: number): JsonlLinePayload[] {
    if (this.pending.length === 0 || hi <= lo) return [];
    if (this.dirty) {
      this.pending.sort((a, b) => a.seq - b.seq);
      this.dirty = false;
    }
    const first = lowerBound(this.pending, lo);
    const last = lowerBound(this.pending, hi);
    return last > first ? this.pending.splice(first, last - first) : [];
  }

  /** Tab 关闭时调:pending 持整段历史 payload(大会话数十 MB 级),必须断引用 */
  dispose(): void {
    this.pending = [];
    this.dirty = false;
  }

  /**
   * 把一条**见过**（`seenSeqs` 里有）而此刻不在账本里的记录放回账本 —— 按行号取回来的那一段里，
   * 早先被修剪出账本的那些（旁路账早记过了，不能再过一遍 `onLine`）。与 {@link defer} 同一个口，
   * 只是不许重复：已在渲染窗口里的、已在账本里的同一 seq 都不再放。
   */
  restore(p: JsonlLinePayload): void {
    if (this.floor !== null && p.seq >= this.floor) return;
    if (this.pending.some((q) => q.seq === p.seq)) return;
    this.defer(p);
  }
}

// ═══════════════════════════════════════════════════════════════════════
// 〔骨架〕**骨架账本**：拿到后端索引就知道「一共多少条、每条大概多高、
// uuid 在哪一条」—— 不持有任何正文。
// ═══════════════════════════════════════════════════════════════════════

/**
 * 骨架账本（纯数据，零 DOM）。
 *
 * # 它替掉的是什么
 *
 * `TailWindow` 只会 `takeTail(k)`，**问不出「第 500 条在哪」**，也不知道一共有多少条
 * ⇒ 滚动条只算得出已建卡的那一段 ⇒ 「往上翻才显示」。
 * 本账本回答三件 `TailWindow` 答不了的事：
 * 1. **总条数与总高**（`endSeq` / `heightOf`）—— 画骨架的总高与滚动条；
 * 2. **某个像素落在哪一条**（`seqAt`）—— 只物化可见区用；
 * 3. **某个 uuid 是第几条**（`uuidToSeq`）—— 跳转用。
 *
 * # 坐标
 *
 * 下标就是 **seq**（与 watcher / `--read-session-tail` 同一个行号空间，`IPC-PROTOCOL.md §10.3`）。
 * `base` = 第一行的 seq（冷启动 0；续传是上次的 `endSeq`，由 `append` 接上）。
 *
 * # 买不到
 *
 * - **精度**：高度是第一级粗估（`estimateFromFacts`）。已渲染的卡由 `contain-intrinsic-size: auto`
 *   记住真值，那部分不走这里；这里只管**没渲染**的那些占多高。
 * - **截断重写**：后端的 jsonl 读者截断重读会换新 seq（INVARIANTS §25；本机远端同一个读者），那之后 seq 与行号不再相等，
 *   本账本对不上 —— 调用方见 `endSeq` 与实到的 seq 对不上时应当丢掉骨架（不许硬对）。
 */
/** 〔大折叠〕账本要知道的折叠：折着的过程那几段（seq 半开区间）· 带过程行的轮的开头 · 过程行多高。 */
export interface TurnFolds {
  folded: ReadonlyArray<readonly [number, number]>;
  lines: readonly number[];
  linePx: number;
}

export class SkeletonLedger {
  readonly base: number;
  private rows: SkeletonFacts[] = [];
  /** prefix[i] = seq [base, base+i) 的估高之和；长度 = 行数 + 1 */
  private prefix: number[] = [0];
  private kinds: SkeletonKind[] = [];
  /** 每行第一级粗估（`prefix` 由它与 `refined` 合出来）。 */
  private est: number[] = [];
  /** 〔第二级〕Worker 精算回来的高（seq → px，当前列宽下）；有它就用它、没有用第一级。 */
  private refined = new Map<number, number>();
  private colW: number | undefined;
  /** 〔大折叠〕折着的过程（seq 半开区间，升序不相交）：这些行按 0 高（不建卡、不占滚动条），展开了才回到估高。 */
  private folded: ReadonlyArray<readonly [number, number]> = [];
  /** 〔大折叠〕带过程行的轮的开头（seq）：那一行多算一条过程行的高（`linePx`）。 */
  private lines = new Set<number>();
  private linePx = 0;
  /** uuid → seq（无 uuid 的行不占） */
  readonly uuidToSeq = new Map<string, number>();

  constructor(base: number, rows: SkeletonFacts[], colW?: number) {
    this.base = base;
    this.colW = colW;
    this.append(rows);
  }

  /** 续传：把 `[endSeq, …)` 的新行接到尾巴上（索引的 `from` 必须是上次的 `end`）。 */
  append(rows: SkeletonFacts[]): void {
    for (const r of rows) {
      const seq = this.base + this.rows.length;
      const prev = this.lastCardKind();
      const kind = skeletonKind(r);
      const h = estimateFromFacts(r, prev, this.colW);
      this.rows.push(r);
      this.kinds.push(kind);
      this.est.push(h);
      this.prefix.push(this.prefix[this.prefix.length - 1] + this.rowHeight(seq, h));
      if (r.u) this.uuidToSeq.set(r.u, seq);
    }
  }

  /**
   * 列宽变了：整份重估（O(n)，纯算术）。精算过的那几行在新列宽下作废 —— 返回它们（「列宽变化只重算已精算过的」，
   * 调用方把它们重交 Worker）。调用方：`SkeletonView.relayout`（宿主在消息流尺寸变了、现量列宽变了时调）。
   */
  relayout(colW: number | undefined): number[] {
    const rows = this.rows;
    const redo = [...this.refined.keys()];
    this.colW = colW;
    this.rows = [];
    this.kinds = [];
    this.est = [];
    this.refined.clear();
    this.prefix = [0];
    this.uuidToSeq.clear();
    this.append(rows);
    return redo;
  }

  /** 当前列宽（`undefined` = 模块量出来的那个）。 */
  get columnWidth(): number | undefined {
    return this.colW;
  }

  /** 这一行精算过没有。 */
  isRefined(seq: number): boolean {
    return this.refined.has(seq);
  }

  /**
   * 〔第二级〕Worker 精算回来的高换进账本（越界 / 不建卡的行不收）；从改动的最低那一行起重合一次前缀和。
   * 返回有没有哪一行真的变了。
   */
  refine(entries: Iterable<readonly [number, number]>): boolean {
    let lowest = Infinity;
    for (const [seq, h] of entries) {
      const i = seq - this.base;
      if (i < 0 || i >= this.rows.length || this.kinds[i] === "none" || !(h >= 0)) continue;
      if (this.refined.get(seq) === h) continue;
      this.refined.set(seq, h);
      lowest = Math.min(lowest, i);
    }
    if (lowest === Infinity) return false;
    this.rebuildFrom(lowest);
    return true;
  }

  /**
   * 〔大折叠〕折着的过程那几段（seq 半开区间）与带过程行的轮的开头换成这一组：折着的行按 0 高，开头那一行多算 `linePx`
   * （过程行本身的高）。从变动的最低那一行起重合前缀和。返回有没有变（同一组 ⇒ `false`，什么都不动）。
   */
  setFolds(f: TurnFolds): boolean {
    const next = f.folded.filter(([a, b]) => b > a).map(([a, b]) => [a, b] as const).sort((x, y) => x[0] - y[0]);
    const prev = this.folded;
    const lines = new Set(f.lines);
    const sameFold = next.length === prev.length && next.every(([a, b], i) => a === prev[i][0] && b === prev[i][1]);
    const sameLines = f.linePx === this.linePx && lines.size === this.lines.size && [...lines].every((x) => this.lines.has(x));
    if (sameFold && sameLines) return false;
    const touched = [...next.map(([a]) => a), ...prev.map(([a]) => a)];
    if (!sameLines) touched.push(...lines, ...this.lines);
    this.folded = next;
    this.lines = lines;
    this.linePx = f.linePx;
    this.rebuildFrom(touched.length === 0 ? 0 : Math.max(0, Math.min(...touched) - this.base));
    return true;
  }

  /** 折着的那几段（`SkeletonView` 物化时扣掉它们）。 */
  get foldedRanges(): ReadonlyArray<readonly [number, number]> {
    return this.folded;
  }

  /** 一行算多高：折着 ⇒ 0；精算过 ⇒ 精算；否则第一级；带过程行的轮的开头再加一条过程行。 */
  private rowHeight(seq: number, est: number): number {
    if (this.isFolded(seq)) return 0;
    return (this.refined.get(seq) ?? est) + (this.lines.has(seq) ? this.linePx : 0);
  }

  private isFolded(seq: number): boolean {
    let l = 0;
    let r = this.folded.length;
    while (l < r) {
      const m = (l + r) >>> 1;
      if (this.folded[m][1] <= seq) l = m + 1;
      else r = m;
    }
    return l < this.folded.length && this.folded[l][0] <= seq;
  }

  private rebuildFrom(lowest: number): void {
    for (let i = lowest; i < this.rows.length; i++) {
      this.prefix[i + 1] = this.prefix[i] + this.rowHeight(this.base + i, this.est[i]);
    }
  }

  private lastCardKind(): SkeletonKind {
    for (let i = this.kinds.length - 1; i >= 0; i--) {
      if (this.kinds[i] !== "none") return this.kinds[i];
    }
    return "none";
  }

  /** 行数 */
  get count(): number {
    return this.rows.length;
  }

  /** 最后一行之后的 seq（半开上界） */
  get endSeq(): number {
    return this.base + this.rows.length;
  }

  /** 全会话估高 */
  get totalHeight(): number {
    return this.prefix[this.prefix.length - 1];
  }

  /** seq 那一行的料（越界 ⇒ undefined） */
  factsOf(seq: number): SkeletonFacts | undefined {
    return this.rows[seq - this.base];
  }

  /** seq 区间 `[lo, hi)` 的估高（越界部分按 0 算） */
  heightOf(lo: number, hi: number): number {
    const a = this.clampIdx(lo);
    const b = this.clampIdx(hi);
    return b > a ? this.prefix[b] - this.prefix[a] : 0;
  }

  /**
   * 在 `[lo, hi)` 这一段里，距段顶 `y` 像素处落在哪一条（返回 seq）。
   * `y < 0` ⇒ `lo`；`y ≥ 段高` ⇒ `hi - 1`；空段 ⇒ `lo`。零高行（不建卡的）不会被单独命中，
   * 返回的是**覆盖该像素**的那一条（二分找 prefix 里第一个 > 目标的位置）。
   */
  seqAt(lo: number, hi: number, y: number): number {
    const a = this.clampIdx(lo);
    const b = this.clampIdx(hi);
    if (b <= a) return lo;
    const target = this.prefix[a] + Math.max(0, y);
    // 找最小的 i ∈ [a, b) 使 prefix[i+1] > target
    let l = a;
    let r = b - 1;
    while (l < r) {
      const mid = (l + r) >>> 1;
      if (this.prefix[mid + 1] > target) r = mid;
      else l = mid + 1;
    }
    return this.base + l;
  }

  private clampIdx(seq: number): number {
    return Math.min(Math.max(seq - this.base, 0), this.rows.length);
  }
}
