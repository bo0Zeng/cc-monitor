/**
 * Batch13-F40a:live Tab 尾部优先的窗口账本(纯数据,零 DOM,对标 render-window.ts)。
 *
 * live 与 viewer(F39 UnrenderedRanges)的关键差异:live 无深链入口,已渲染集
 * **恒为按 seq 连续的尾后缀 [floor, +∞)**——无岛、无中缝(单洞不变量)。
 * 所以不用区间代数,一个 floor 水位 + 待渲染数组就够。
 *
 * 启动重放到达序:同 session 末块先发、块内 seq 升序。尾块被 admit 直渲(进步式
 * 首屏),更早的块整块 defer——pending 此刻是「按块降序、块内升序」的非连续集合,
 * 但唯一消费方式是 takeTail(取 seq 最高的 k 条),消费前惰性 sort 一次即可,
 * 洞的几何从不需要被查询。补批后窗口仍是后缀。
 *
 * 若未来 live 出现深链跳转需求(跳到任意历史位置),本结构需升级为
 * UnrenderedRanges 语义(区间集,render-window.ts 已有现成实现)——单洞后缀
 * 不变量在"跳到中部"时不再成立,届时窗口=多区间、fill=按洞取段。
 *
 * 〔`设计/10` 骨架 · 2026-09-24〕**上面预言的那次升级发生了，但只在骨架接上的 tab 上。**
 * 骨架（`skeleton-view.ts`）按可见区取**岛**（`takeRange`），于是已渲染集 = 尾后缀 ∪ 若干岛，
 * 单洞后缀不变量**不再成立**；「哪些 seq 还没物化」的真相源换成骨架的占位集（`SkeletonView.isPending`），
 * 本类退成「还没建卡的 payload 放哪」的账本。没接上骨架的 tab（没索引 / 老后端 / seq 对不上）
 * 仍是原来的单洞后缀，行为逐字不变。⚠ `INVARIANTS.md §21.3` 与 `ARCHITECTURE.md` 那两处
 * 「单洞后缀」的描述**还没同步**（不在本刀写区）。
 */
import type { JsonlLinePayload } from "./events";
import {
  estimateFromFacts,
  skeletonKind,
  type SkeletonFacts,
  type SkeletonKind,
} from "./height-estimate";

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
 * 〔CF2 · 第四波 4B〕**账本（还没上屏的那些）的上界**：超过 {@link PENDING_CAP} 条就只留 seq 最高的
 * {@link PENDING_KEEP} 条，其余出账 —— 它们往上翻到时按行号取回（`fetchBelow`；接了骨架的按字节，`fetchMissingRows`）。
 *
 * `设计/05 §3.3.4` 逐字「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界」—— 这本账本就是 webview 这一侧的订阅缓冲：
 * 原来没接骨架的 tab 在这里驻留整段历史（一份 4 万行的会话，后台 tab 里 4 万个 payload）。
 * 取数：一次物化最多 600 条（`materializeUntilFilled` 150 × 4）、上翻一批 200 条 ⇒ 留 2000 条够首屏 ＋ 七八次上翻不用等 IPC；
 * 摊还余量 1000（与 monitor 那一侧 `TRIM_SLACK` 同一个道理：每来一条都修会让收纳路付 O(n)）。
 */
export const PENDING_KEEP = 2000;
export const PENDING_CAP = 3000;

/**
 * 〔CF2 · 第四波 4B〕**账本之下还有没有行**（`调研/第四波记录/CF2.md §1.4`）。
 *
 * seq 就是行号、从 0 起 ⇒ 渲染窗口最老那一条的 seq > 0 且账本空了 ⇒ 下面**可能**还有
 * （被 monitor 的重放缓冲修剪掉的 · 被本账本修剪掉的 · 或只是几条不显示的记录）。
 * 不需要谁来告诉「修剪过」：修没修剪，做法一样 —— 按行号问一次（`read_session_lines`）。
 *
 * - `maybe`：还没问过（或上次问回来之后还没到 0）；
 * - `fetching`：问着；
 * - `none`：问到了第 0 行 —— 到顶了；
 * - `failed`：问不动（老后端不认 / 断了），带一句给人看的原因。**不自动重问**：上翻是逐 scroll 事件触发的，
 *   失败了自动重问就是一个无界的重试环；切走再切回来（`retryBelow`）才再问一次。
 */
export type BelowState =
  | { kind: "maybe" }
  | { kind: "fetching" }
  | { kind: "none" }
  | { kind: "failed"; reason: string };

export class TailWindow {
  /** 窗口低水位;null = virgin(该 tab 尚未渲染任何 content 记录) */
  private floor: number | null = null;
  /** 未渲染 payload;尾追加免排序,乱序块标 dirty 惰性 sort */
  private pending: JsonlLinePayload[] = [];
  private dirty = false;
  /** 〔CF2〕账本之下还有没有（见 {@link BelowState}） */
  private below: BelowState = { kind: "maybe" };

  get floorSeq(): number | null {
    return this.floor;
  }

  /** 〔CF2〕账本之下的状态（哨兵那句话按它说）。 */
  get belowState(): BelowState {
    return this.below;
  }

  /**
   * 〔CF2〕该不该按行号往下问：账本空了、渲染窗口最老那一条不是第 0 行、还没问到顶、此刻没在问、上次没失败。
   * 问的区间是 `[max(0, floor − batch), floor)`（{@link belowRange}）。
   */
  get wantsBelow(): boolean {
    return (
      this.pending.length === 0 && this.floor !== null && this.floor > 0 && this.below.kind === "maybe"
    );
  }

  /** 〔CF2〕要问的那一段 `[from, until)`（`until` = 渲染窗口最老那一条）。没得问 ⇒ `null`。 */
  belowRange(batch: number): { from: number; until: number } | null {
    if (!this.wantsBelow || this.floor === null) return null;
    return { from: Math.max(0, this.floor - Math.max(1, batch)), until: this.floor };
  }

  /** 〔CF2〕开始问。 */
  markFetchingBelow(): void {
    this.below = { kind: "fetching" };
  }

  /** 〔CF2〕问回来了：问的是从第 `from` 行起 ⇒ `from == 0` 就到顶了，否则还可能有。 */
  markFetchedBelow(from: number): void {
    this.below = from <= 0 ? { kind: "none" } : { kind: "maybe" };
  }

  /** 〔CF2〕问不动。 */
  markBelowFailed(reason: string): void {
    this.below = { kind: "failed", reason };
  }

  /** 〔CF2〕失败过的，允许再问一次（切走再切回来时调；其余状态原样）。 */
  retryBelow(): void {
    if (this.below.kind === "failed") this.below = { kind: "maybe" };
  }

  /**
   * 〔CF2〕会话流里丢过格（`gap`）⇒ 账本里还没上屏的那些**不可信**（它们之间可能夹着洞，而 seq 里本来就有
   * 不显示的记录占的号，前端从 seq 看不出哪里缺）⇒ 整份出账，之后往上翻按行号重新取（`below` 回到 `maybe`）。
   * 返回丢掉的条数。
   */
  dropPending(): number {
    const n = this.pending.length;
    this.pending = [];
    this.dirty = false;
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
   * 〔CF2〕账本超过 {@link PENDING_CAP} ⇒ 只留 seq 最高的 {@link PENDING_KEEP} 条（出账的那些往上翻时按行号取回；
   * 「下面还有」这件事随之回到 `maybe`）。
   */
  defer(p: JsonlLinePayload): void {
    const last = this.pending[this.pending.length - 1];
    if (last !== undefined && p.seq < last.seq) this.dirty = true;
    this.pending.push(p);
    if (this.pending.length > PENDING_CAP) {
      this.keepHighest(PENDING_KEEP);
      if (this.below.kind === "none") this.below = { kind: "maybe" };
    }
  }

  /**
   * 弹出 pending 中 seq 最高的 ≤k 条(升序返回,已出账),并把 floor 压到取出段
   * 的最低 seq——上翻补批/物化的口粮。空账返回 [],floor 不动。
   */
  takeTail(k: number): JsonlLinePayload[] {
    if (this.pending.length === 0 || k <= 0) return [];
    if (this.dirty) {
      this.pending.sort((a, b) => a.seq - b.seq);
      this.dirty = false;
    }
    const taken = this.pending.splice(Math.max(0, this.pending.length - k));
    if (taken.length > 0) this.pinFloor(taken[0].seq);
    return taken;
  }

  get pendingCount(): number {
    return this.pending.length;
  }

  /**
   * 〔U3b · `设计/10` 步 8〕只留 seq 最高的 `keep` 条，其余**出账丢弃**；返回丢掉的条数。
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
   * 〔`设计/10` 骨架 · 子步 4〕只读看几条 pending（**不出账**）—— 骨架接上之前用它对拍
   * 「seq 与索引行号是不是同一个空间」（截断重读换过 seq 的会话对不上，不许硬接）。
   */
  peek(n: number): readonly JsonlLinePayload[] {
    return this.pending.slice(0, Math.max(0, n));
  }

  /**
   * 〔`设计/10` 骨架 · 子步 3〕弹出 pending 里 seq ∈ `[lo, hi)` 的那些（升序、出账）。
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
   * 〔CF2〕把一条**见过**（`seenSeqs` 里有）而此刻不在账本里的记录放回账本 —— 按行号取回来的那一段里，
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
// 〔`设计/10` 骨架 · 子步 2〕**骨架账本**：拿到后端索引就知道「一共多少条、每条大概多高、
// uuid 在哪一条」—— 不持有任何正文。
// ═══════════════════════════════════════════════════════════════════════

/**
 * 骨架账本（纯数据，零 DOM）。
 *
 * # 它替掉的是什么
 *
 * `TailWindow` 只会 `takeTail(k)`，**问不出「第 500 条在哪」**，也不知道一共有多少条
 * ⇒ 滚动条只算得出已建卡的那一段 ⇒ 「往上翻才显示」（`设计/10 §2.1` 第 3 条）。
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
 * - **截断重写**：本地 watcher 截断重读会换新 seq（INVARIANTS §25），那之后 seq 与行号不再相等，
 *   本账本对不上 —— 调用方见 `endSeq` 与实到的 seq 对不上时应当丢掉骨架（不许硬对）。
 */
export class SkeletonLedger {
  readonly base: number;
  private rows: SkeletonFacts[] = [];
  /** prefix[i] = seq [base, base+i) 的估高之和；长度 = 行数 + 1 */
  private prefix: number[] = [0];
  private kinds: SkeletonKind[] = [];
  private colW: number | undefined;
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
      this.prefix.push(this.prefix[this.prefix.length - 1] + h);
      if (r.u) this.uuidToSeq.set(r.u, seq);
    }
  }

  /** 列宽变了：整份重估（O(n)，纯算术）。 */
  relayout(colW: number | undefined): void {
    const rows = this.rows;
    this.colW = colW;
    this.rows = [];
    this.kinds = [];
    this.prefix = [0];
    this.uuidToSeq.clear();
    this.append(rows);
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
