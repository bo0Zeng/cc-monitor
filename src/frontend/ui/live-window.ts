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
import type { HistoryIndex } from "./generated/HistoryIndex";
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
// 每条在记录里的哪一段字节」—— 不持有任何正文。
// ═══════════════════════════════════════════════════════════════════════

type Typed = Float64Array | Uint32Array | Uint8Array;

/** 一列只往后加的数（翻倍扩容）：骨架账本按列存，几十万行不建几十万个小对象。 */
class Column<T extends Typed> {
  private data: T;
  length = 0;

  constructor(private readonly make: (n: number) => T) {
    this.data = make(64);
  }

  push(v: number): void {
    if (this.length === this.data.length) {
      const next = this.make(this.data.length * 2);
      next.set(this.data);
      this.data = next;
    }
    this.data[this.length++] = v;
  }

  at(i: number): number {
    return this.data[i];
  }

  /** 升序列里 `< v` 的有几个（稀疏表按行号找项用）。 */
  lowerBound(v: number): number {
    let l = 0;
    let r = this.length;
    while (l < r) {
      const m = (l + r) >>> 1;
      if (this.data[m] < v) l = m + 1;
      else r = m;
    }
    return l;
  }
}

const f64 = (n: number): Float64Array => new Float64Array(n);
const u32 = (n: number): Uint32Array => new Uint32Array(n);
const u8 = (n: number): Uint8Array => new Uint8Array(n);

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
 * 3. **某一条在记录里的哪一段字节**（`factsOf`）—— 按偏移取正文用。
 * 跳转的目标由后端直接给行号（轮次 · 过程段 · 命中 · 大纲），本账本不认 uuid。
 *
 * # 存法
 *
 * 按列收后端索引（帧面 `history-index` 本来按列排）：每行都有的几格各一列定长数组，稀疏的几格（正文那五个数 · 来源）
 * 各一张按行号升序的小表；`factsOf(seq)` 现拼一行的料。
 *
 * # 坐标
 *
 * 下标就是 **seq**（与 watcher / `history-tail` 同一个行号空间，`IPC-PROTOCOL.md §10.3`）。
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
  /** 每行：起点字节 · 字节长 · 记录类（`classes` 里的号，从 1 起；0 ＝ 不进界面）· 折叠单元数。 */
  private off = new Column(f64);
  private len = new Column(u32);
  private cls = new Column(u8);
  private fd = new Column(u32);
  /** 本账本里出现过的记录类（各页的 `classes` 并成一张）。 */
  private classes: string[] = [];
  /** 正文那五个数：哪几行（下标，升序）＋ 五列。 */
  private bodyAt = new Column(u32);
  private body = [new Column(u32), new Column(u32), new Column(u32), new Column(u32), new Column(u32)] as const;
  /** 来源：哪几行（下标，升序）＋ 那个词。 */
  private spAt = new Column(u32);
  private spV: string[] = [];
  /** prefix[i] = seq [base, base+i) 的估高之和；长度 = 行数 + 1 */
  private prefix: number[] = [0];
  private kinds: SkeletonKind[] = [];
  /** 每行第一级粗估（`prefix` 由它与 `refined` 合出来）。 */
  private est: number[] = [];
  /** 〔第二级〕Worker 精算回来的高 · 交回占位前量的真高（seq → px，当前列宽下）；有它就用它、没有用第一级。 */
  private refined = new Map<number, number>();
  private colW: number | undefined;
  /** 〔大折叠〕折着的过程（seq 半开区间，升序不相交）：这些行按 0 高（不建卡、不占滚动条），展开了才回到估高。 */
  private folded: ReadonlyArray<readonly [number, number]> = [];
  /** 〔大折叠〕带过程行的轮的开头（seq）：那一行多算一条过程行的高（`linePx`）。 */
  private lines = new Set<number>();
  private linePx = 0;
  /**
   * 〔列宽变了、还没按新列宽估的行〕视口附近之外的那两截：`[0, staleBelow)` 与 `[staleAbove, staleEnd)`（下标），
   * 照旧按上一个列宽的估值占着；[`settle`] 一段一段补估（离视口近的先）。
   */
  private staleBelow = 0;
  private staleAbove = 0;
  private staleEnd = 0;
  /** 到此刻按第一级估过几行（判据：改列宽当场只估视口附近那几行）。 */
  estimated = 0;

  constructor(base: number, pages: readonly HistoryIndex[], colW?: number) {
    this.base = base;
    this.colW = colW;
    this.append(pages);
  }

  /** 续传：把 `[endSeq, …)` 的新行接到尾巴上（索引的 `from` 必须是上次的 `end`；几页按序）。 */
  append(pages: readonly HistoryIndex[]): void {
    for (const page of pages) this.appendPage(page);
  }

  private appendPage(v: HistoryIndex): void {
    const first = this.off.length;
    const cls = v.classes.map((c) => {
      const k = this.classes.indexOf(c);
      return (k < 0 ? this.classes.push(c) : k + 1);
    });
    let at = v.from;
    for (let i = 0; i < v.count; i++) {
      at += v.o[i];
      this.off.push(at);
      this.len.push(v.n[i]);
      at += v.n[i];
      this.cls.push(v.t[i] > 0 ? cls[v.t[i] - 1] : 0);
      this.fd.push(v.fd[i]);
    }
    let i = first - 1;
    v.body.at.forEach((g, j) => {
      i += g + 1;
      this.bodyAt.push(i);
      const cols = [v.body.ch, v.body.cj, v.body.pl, v.body.cb, v.body.cl];
      this.body.forEach((c, k) => c.push(cols[k][j]));
    });
    i = first - 1;
    v.sp.at.forEach((g, j) => {
      i += g + 1;
      this.spAt.push(i);
      this.spV.push(v.sp.v[j]);
    });
    for (let k = first; k < this.off.length; k++) {
      const seq = this.base + k;
      const f = this.factsOf(seq)!;
      const h = estimateFromFacts(f, this.colW);
      this.estimated++;
      this.kinds.push(skeletonKind(f));
      this.est.push(h);
      this.prefix.push(this.prefix[this.prefix.length - 1] + this.rowHeight(seq, h));
    }
  }

  /**
   * 列宽变了：`near`（seq 半开区间，视口附近）那几行当场按新列宽重估，其余的先照旧按上一个列宽的估值占着、
   * 由 [`settle`] 一段一段补（不给 `near` ⇒ 整份当场重估）。前缀和整份重合一次（只是加法）。
   * 精算过的那几行在新列宽下作废 —— 返回它们（「列宽变化只重算已精算过的」，调用方把它们重交 Worker）。
   * 调用方：`SkeletonView.relayout`（宿主在消息流尺寸变了、现量列宽变了时调）。
   */
  relayout(colW: number | undefined, near?: readonly [number, number]): number[] {
    const redo = [...this.refined.keys()];
    this.colW = colW;
    this.refined.clear();
    const n = this.off.length;
    const lo = near ? this.clampIdx(near[0]) : 0;
    const hi = near ? Math.max(lo, this.clampIdx(near[1])) : n;
    for (let i = lo; i < hi; i++) this.reestimate(i);
    this.staleBelow = lo;
    this.staleAbove = hi;
    this.staleEnd = n;
    this.rebuildFrom(0);
    return redo;
  }

  /** 还有几行没按当前列宽估（[`relayout`] 留下的）。 */
  get staleRows(): number {
    return this.staleBelow + (this.staleEnd - this.staleAbove);
  }

  /** 按当前列宽补估至多 `max` 行没估的（离视口近的先：上面那一截往尾推、下面那一截往 0 退，轮流）。回：有没有哪一行变了高。 */
  settle(max: number): boolean {
    let lowest = Infinity;
    let k = 0;
    while (k < max && (this.staleBelow > 0 || this.staleAbove < this.staleEnd)) {
      if (this.staleAbove < this.staleEnd) {
        const i = this.staleAbove++;
        if (this.reestimate(i) && i < lowest) lowest = i;
        k++;
      }
      if (k < max && this.staleBelow > 0) {
        const i = --this.staleBelow;
        if (this.reestimate(i) && i < lowest) lowest = i;
        k++;
      }
    }
    if (lowest === Infinity) return false;
    this.rebuildFrom(lowest);
    return true;
  }

  /** 一行按当前列宽重估第一级；回：变没变。 */
  private reestimate(i: number): boolean {
    const h = estimateFromFacts(this.factsOf(this.base + i)!, this.colW);
    this.estimated++;
    if (this.est[i] === h) return false;
    this.est[i] = h;
    return true;
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
    return this.take(entries, false);
  }

  /**
   * 建过的卡量出来的真高（交回占位之前，`SkeletonView.release`）换进同一张表：不建卡的行也收（注入细条有高），
   * 折着的行不收，过程行那一条的高扣掉（它另算）。返回有没有哪一行变了。
   */
  measured(entries: Iterable<readonly [number, number]>): boolean {
    return this.take(entries, true);
  }

  private take(entries: Iterable<readonly [number, number]>, measured: boolean): boolean {
    let lowest = Infinity;
    for (const [seq, px] of entries) {
      const i = seq - this.base;
      if (i < 0 || i >= this.off.length || !(px >= 0)) continue;
      if (measured ? this.isFolded(seq) : this.kinds[i] === "none") continue;
      const h = measured && this.lines.has(seq) ? Math.max(0, px - this.linePx) : px;
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
    // 动到的最低一行：新旧两份折叠的开头（·行号变了再加新旧两份过程行的开头）。逐个比，不展开成实参 ——
    // 这几份各约一轮一个，几万轮时合计十几万个号，超过 V8（Windows 的 WebView2）一次调用的实参上限会抛 RangeError。
    let lowest = Infinity;
    for (const [a] of next) if (a < lowest) lowest = a;
    for (const [a] of prev) if (a < lowest) lowest = a;
    if (!sameLines) {
      for (const x of lines) if (x < lowest) lowest = x;
      for (const x of this.lines) if (x < lowest) lowest = x;
    }
    this.folded = next;
    this.lines = lines;
    this.linePx = f.linePx;
    this.rebuildFrom(lowest === Infinity ? 0 : Math.max(0, lowest - this.base));
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
    for (let i = lowest; i < this.off.length; i++) {
      this.prefix[i + 1] = this.prefix[i] + this.rowHeight(this.base + i, this.est[i]);
    }
  }

  /** 行数 */
  get count(): number {
    return this.off.length;
  }

  /** 最后一行之后的 seq（半开上界） */
  get endSeq(): number {
    return this.base + this.off.length;
  }

  /** 全会话估高 */
  get totalHeight(): number {
    return this.prefix[this.prefix.length - 1];
  }

  /** seq 那一行的料（越界 ⇒ undefined；现拼，不留）。 */
  factsOf(seq: number): SkeletonFacts | undefined {
    const i = seq - this.base;
    if (!(i >= 0 && i < this.off.length)) return undefined;
    const f: SkeletonFacts = { o: this.off.at(i), n: this.len.at(i) };
    const c = this.cls.at(i);
    if (c > 0) f.t = this.classes[c - 1] as SkeletonFacts["t"];
    const fd = this.fd.at(i);
    if (fd > 0) f.fd = fd;
    const b = this.bodyAt.lowerBound(i);
    if (b < this.bodyAt.length && this.bodyAt.at(b) === i) {
      const [ch, cj, pl, cb, cl] = this.body.map((col) => col.at(b));
      if (ch > 0) f.ch = ch;
      if (cj > 0) f.cj = cj;
      if (pl > 0) f.pl = pl;
      if (cb > 0) f.cb = cb;
      if (cl > 0) f.cl = cl;
    }
    const s = this.spAt.lowerBound(i);
    if (s < this.spAt.length && this.spAt.at(s) === i) f.sp = this.spV[s];
    return f;
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
    return Math.min(Math.max(seq - this.base, 0), this.off.length);
  }
}
