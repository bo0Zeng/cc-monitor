/**
 * ESC 回退分支的折叠 UI：消息流容器（卡片已按 jsonl 顺序摆好）＋ 主线 uuid 集合 ⇒ 把连续的 off-main 卡包进 `.branch-fold-wrap`。
 *
 * - 段：带 data-uuid 且不在主线的卡连成一段；在主线上的卡与没有 data-uuid 的元素断段。
 *   分支变化不是局部的（一条新 user 记录可能让一大段卡转 off-main），重折按段差量做，见 `rebuild`。
 * - 折叠状态保留：wrap 的展开态记在 `foldExpanded`（key = 段首条 uuid）。
 * - 每帧账本（文件末尾）：`computeMain()` 每次的 N / ms / 来源 / 帧号、`rebuild()` 搬了几个节点、快路命中，
 *   挂 `window.__ccmPerf.branchLedger`；只写账本，不回流到折叠决策。
 */

import { computeMainBranch, exemptQueuedLeaves, setsEqual, type BranchRecord } from "./branching";
import { copyText } from "./copy-table";
import { icon } from "./kit/icon";

const FOLD_WRAP_CLASS = "branch-fold-wrap";
const FOLD_HEADER_CLASS = "branch-fold-header";
const FOLD_ARROW_CLASS = "branch-fold-arrow";
const FOLD_BODY_CLASS = "branch-fold-body";
const FOLD_BODY_INNER_CLASS = "branch-fold-body-inner";

/**
 * 跟随一个具体的 stream 容器，管它的 fold 重建。
 *
 * 每个 Tab / SessionViewer 持有一个实例。
 */
export class BranchFolder {
  /** stream 容器（卡片直接挂在它的 children 上，可能跟 fold-wrap 混合） */
  private container: HTMLElement;
  /** records 集合（caller push 进来，按 jsonl 顺序）。computeMainBranch 用 */
  private records: BranchRecord[] = [];
  /** 本会话 queue-operation enqueue 的 content 集合（trim 后）：被消费的队列消息（永久裸 user 叶）豁免折叠用。 */
  private queuedContents = new Set<string>();
  /**
   * 已见 uuid 集，recordAdded 拒重：投递层是 at-least-once（src/doc/INVARIANTS.md § 25），重复记录会毒化 Kahn 拓扑、大段误折叠。
   * computeMainBranch 入口也去重；这里挡住还免得 records 被重投无界增长。
   */
  private seenUuids = new Set<string>();
  /** 上次重建用的 mainBranch；判等避免无 diff 时空重排 */
  private lastMainBranch: Set<string> = new Set();
  /** 折叠 ID（每个 fold 的第一条 uuid） → 用户是否手动展开了 */
  private foldExpanded = new Map<string, boolean>();
  /** 每个 wrap 包它时有几条（标题上那个数）—— 差量重折判「这个 wrap 还原样可用吗」用 */
  private wrapSize = new WeakMap<Element, number>();
  /**
   * batch 模式：
   * - true（重放期）：recordAdded 只 push 不算，到 flushPending() 才一次性算主线 ＋ rebuild（批量灌几千条时省 O(N²)）；
   * - false（live，默认）：每条 1 帧内反映折叠状态（帧末合批）。
   * TabManager：setBatchMode(true) → 灌 records → flushPending() → setBatchMode(false)。
   */
  private batchMode = false;
  /**
   * 秤 4 仪表状态（四个字段，**只被账本读**，折叠决策一个都不看）。
   *
   * - `ledgerParentSeen`：已经至少有过一个 child 的 parent uuid。影子快路判
   *   「新记录的 parent **原本无 child**」要的就是这个 —— 有了 child 再来一个
   *   就是 fork 点，那正是 §2.7 里走不了 O(1) 的那 ~3%。
   * - `ledgerShadowAdd`：上一次**真算**之后，影子快路自己 `add` 进主线的 uuid。
   *   档 1 的快路是「只 `add(uuid)`」，所以同一帧里后来的记录要能看见前面那几条
   *   ——否则量到的是「帧合批把快路废掉了多少」而不是谓词本身。两个数都要，
   *   判据里分成两种到达节奏各量一次（见 `tests/frontend/ui/scale4-frame-ledger.vitest.ts`）。
   * - `ledgerSinceCompute` / `ledgerMissSinceCompute`：上次真算之后来了几条、
   *   其中几条没命中。全命中 ⇒ 档 1 能把这次 O(N) 整个跳掉（账本记 `computesSkippable`）。
   */
  private ledgerParentSeen = new Set<string>();
  private ledgerShadowAdd = new Set<string>();
  private ledgerSinceCompute = 0;
  private ledgerMissSinceCompute = 0;
  /** 上次真算之后队列豁免集合变过（档 1 快路不认这种帧）。 */
  private queuedDirty = false;
  /**
   * 上次算出的主线（`lastMainBranch`）之后，records 与队列豁免都没变过 ⇒ 主线照用上次的。
   * 骨架往下物化一段 / 上翻补一批之后的重折（`rebuildNow`）不来新记录（卡是早就登记过的那些记录建出来的），
   * 每次都整份重算主线 ＝ 长会话里往下翻一下就 O(全会话) 一次。
   */
  private mainFresh = false;

  constructor(container: HTMLElement) {
    this.container = container;
  }

  /**
   * 卡片刚 append 完之后调一次。caller 给出 uuid / parentUuid / timestamp（用于
   * 主线识别）。
   *
   * batch 模式下：只 push，不算主线，不 rebuild —— 等 flushPending。
   * live 模式下：立即算主线，如变化 rebuild。
   */
  recordAdded(rec: BranchRecord): void {
    if (this.seenUuids.has(rec.uuid)) return; // 重投拒收（见字段注释）
    // 秤 4 仪表：**必须在 push 之前**——它读的是「这条进来之前」的态（parent 在不在、
    // parent 原本有没有 child）。放到 push 之后就会看见自己，判定恒变。
    this.noteFastPathShadow(rec);
    this.seenUuids.add(rec.uuid);
    this.records.push(rec);
    this.mainFresh = false;
    if (rec.parentUuid) this.ledgerParentSeen.add(rec.parentUuid); // 秤 4 仪表
    if (this.batchMode) return; // batch 模式：延后到 flush
    this.scheduleLiveRecompute();
  }

  /**
   * §2.7 档 1「脏标记快路」的**判定**（已不只是影子：帧末那一次全命中 ⇒ `fastMain` 跳掉全量重算）。
   * 这里只记「这条走不走得了 O(1)」与账本；折叠结果由帧末那一次定。
   *
   * 档 1 的谓词逐字是「新记录的 parent 是当前主线叶子且原本无 child ⇒ 只 `add(uuid)`」，
   * 拆成四问，**顺序即优先级**（一条记录只记一个未命中原因，记最先踩到的那个）：
   *  1. 没有 parentUuid ⇒ 它是 root，快路无从谈起（`noParent`）
   *  2. parentUuid 不在已见集合里 ⇒ 链断，`computeMainBranch` 会把它当 root（`parentUnknown`）
   *  3. parent 已经有 child ⇒ **这就是 fork 点**，主线赢家要重选（`parentHasChild`）
   *  4. parent 不在主线集合里 ⇒ 接在旧分支上，快路的 `add` 会把它错标成主线（`parentOffMain`）
   * 四问都过 ⇒ 命中。
   */
  private noteFastPathShadow(rec: BranchRecord): void {
    const p = rec.parentUuid;
    let miss: FastPathMissReason | null;
    if (!p) miss = "noParent";
    else if (!this.seenUuids.has(p)) miss = "parentUnknown";
    else if (this.ledgerParentSeen.has(p)) miss = "parentHasChild";
    else if (!this.lastMainBranch.has(p) && !this.ledgerShadowAdd.has(p)) miss = "parentOffMain";
    else miss = null;

    this.ledgerSinceCompute++;
    if (miss === null) this.ledgerShadowAdd.add(rec.uuid);
    else this.ledgerMissSinceCompute++;

    const led = branchLedger();
    if (!led) return;
    led.recordsAdded++;
    if (miss === null) led.fastPathHit++;
    else {
      led.fastPathMiss++;
      led.fastPathMissBy[miss]++;
    }
    if (led.fastPathSamples.length < LEDGER_SAMPLE_CAP) {
      led.fastPathSamples.push({ hit: miss === null, miss, frame: led.frames });
    } else {
      led.fastPathSamplesDropped++;
    }
  }

  /**
   * live 模式的帧末合批：`computeMain()` 与 `rebuild()` 都是 O(N)，逐条算 ⇒ O(N²)；契约是「每条 1 帧内反映折叠状态」，帧内算一次就够。
   * 排一次位（`liveScheduled`），没有 rAF 时 `setTimeout` 兜底。`pendingLive` 单独记：调用方中途同步刷过
   * （`flushPending` / `rebuildNow` / `setRecordsAndRebuild`）就清掉，免得帧末再白算一遍。
   */
  private liveScheduled = false;
  private pendingLive = false;
  private disposed = false;

  private scheduleLiveRecompute(): void {
    this.pendingLive = true;
    if (this.liveScheduled) return;
    this.liveScheduled = true;
    const run = (): void => {
      // 秤 4 仪表：帧号在**本帧回调结束时**加一（三个 return 点都要走到，故用 finally）。
      // 这样「这一帧里来的记录」与「这一帧里那次真算」拿到同一个帧号。
      try {
        this.liveScheduled = false;
        if (this.disposed || this.batchMode || !this.pendingLive) return;
        this.pendingLive = false;
        const next = this.fastMain() ?? this.computeMain("live-frame");
        this.mainFresh = true; // 这一刻的 records 算出来的就是 next（等不等于上次都一样）
        if (setsEqual(next, this.lastMainBranch)) return;
        this.lastMainBranch = next;
        this.rebuild();
      } finally {
        bumpLedgerFrame();
      }
    };
    if (typeof requestAnimationFrame === "function") {
      // 调度：合批 —— live 模式主线重算排到帧末，排一次位、回调里不再排
      requestAnimationFrame(run);
    } else {
      // 调度：合批 —— 上面那一处在没有 rAF 时的兜底，0ms 一次
      setTimeout(run, 0);
    }
  }

  /**
   * 〔档 1〕**脏标记快路**：上次真算之后来的记录**全都**接在主线叶子上、父亲原本无 child
   * （影子判定全命中，`noteFastPathShadow`），队列豁免也没变 ⇒ 主线 = 上次 ∪ 它们，不重扫 `computeMainBranch`。
   * 有一条没命中（分叉 / 链断 / 接在旧分支上）或豁免变了 ⇒ `null`，照旧全量算。
   * verify 档（判据专用）照旧全量算并逐元素比影子结果（`computeMain`），不走这里。
   */
  private fastMain(): Set<string> | null {
    if (this.ledgerSinceCompute === 0 || this.ledgerMissSinceCompute > 0 || this.queuedDirty) return null;
    const led = branchLedger();
    if (led?.verify) return null;
    const next = new Set(this.lastMainBranch);
    for (const u of this.ledgerShadowAdd) next.add(u);
    this.ledgerSinceCompute = 0;
    this.ledgerShadowAdd.clear();
    if (led) led.computesSkipped++;
    return next;
  }

  /**
   * 帧末那次合批**还欠着吗**。供调用方在需要立刻看到最终态时先同步刷一把。
   *
   * ⚠ 不导出「取消」——取消等于把这一批记录的折叠结果丢掉。
   */
  hasPendingLiveRecompute(): boolean {
    return this.pendingLive;
  }

  /**
   * 批量场景（session-viewer 一次 load 全部历史）：先 push 所有 records，
   * 然后调一次 rebuildAll。比逐条 recordAdded 省一堆中间 rebuild。
   */
  setRecordsAndRebuild(records: ReadonlyArray<BranchRecord>): void {
    this.pendingLive = false; // 同上
    // 与 recordAdded 同等拒重（去重后存，保首见）
    this.seenUuids = new Set();
    this.records = [];
    this.ledgerParentSeen = new Set(); // 秤 4 仪表：随 records 整批重建
    for (const r of records) {
      if (!this.seenUuids.has(r.uuid)) {
        this.seenUuids.add(r.uuid);
        this.records.push(r);
        if (r.parentUuid) this.ledgerParentSeen.add(r.parentUuid); // 秤 4 仪表
      }
    }
    const led = branchLedger();
    if (led) led.recordsBulkSet += this.records.length;
    this.lastMainBranch = this.computeMain("set-records");
    this.mainFresh = true;
    this.rebuild();
  }

  /** 登记一条进过输入队列的消息内容（enqueue 记录到达时调）。 */
  addQueuedContent(content: string): void {
    const t = content.trim();
    if (!t || this.queuedContents.has(t)) return;
    this.queuedContents.add(t);
    this.queuedDirty = true; // 豁免变了 ⇒ 下一次不走档 1 快路
    this.mainFresh = false;
    // 已渲染状态下追加豁免可能改变折叠结果（queue-operation 行可能晚于 user 行到达）。
    // 每条 enqueue 都喂一次 ⇒ 排进同一个帧末合批（`scheduleLiveRecompute`），一帧内来多少条都只算一次；
    // 豁免在本帧末生效。要立刻看到最终态走 `rebuildNow()` / `flushPending()`，问还欠不欠走 `hasPendingLiveRecompute()`。
    if (!this.batchMode) this.scheduleLiveRecompute();
  }

  /**
   * 主线计算统一入口：computeMainBranch ＋ 队列消息豁免。
   * 每帧账本夹在这里（`via` 只是账本的一列，计算本身不看），夹的是两段合起来的墙钟。
   */
  private computeMain(via: BranchComputeVia): Set<string> {
    const led = branchLedger();
    const t0 = led ? nowMs() : 0;
    const next = exemptQueuedLeaves(
      this.records,
      computeMainBranch(this.records),
      this.queuedContents,
    );
    if (led) {
      const ms = nowMs() - t0;
      led.computes++;
      led.computeMs += ms;
      if (led.computeSamples.length < LEDGER_SAMPLE_CAP) {
        led.computeSamples.push({ n: this.records.length, ms, via, frame: led.frames });
      } else {
        led.computeSamplesDropped++;
      }
      if (this.ledgerSinceCompute === 0) {
        // 这次真算之前一条新记录都没来：addQueuedContent / rebuildNow / 重复 flush，
        // 以及 setRecordsAndRebuild（它绕开 recordAdded 整批换 records）都落这里。
        led.computesNoArrivals++;
      } else if (this.ledgerMissSinceCompute === 0) {
        led.computesSkippable++;
        if (led.verify) {
          // verify 档（判据专用，生产默认关）：把影子快路**算出来的主线**与真算的结果
          // 逐元素比一次。全命中那些次要是比不上，说明档 1 的谓词本身是错的 ——
          // 「快」而「不对」比慢要坏得多，这一格专门盯它。
          const predicted = new Set(this.lastMainBranch);
          for (const u of this.ledgerShadowAdd) predicted.add(u);
          if (setsEqual(predicted, next)) led.fastPathVerified++;
          else led.fastPathWrong++;
        }
      } else {
        led.computesUnskippable++;
      }
    }
    this.ledgerSinceCompute = 0;
    this.ledgerMissSinceCompute = 0;
    this.ledgerShadowAdd.clear();
    this.queuedDirty = false;
    return next;
  }

  /**
   * 切换 batch 模式。切到 batch 后到 flushPending 之间的 recordAdded
   * 都不会触发计算 / rebuild。切回 live 不会自动 flush，需 caller 显式调 flushPending。
   */
  setBatchMode(enabled: boolean): void {
    this.batchMode = enabled;
  }

  /**
   * 在 batch 模式累计完后调一次，计算最新主线并 rebuild。
   * 也可在 live 模式手动调（等价于 setRecordsAndRebuild 但保持现有 records）。
   */
  flushPending(): void {
    this.pendingLive = false; // 已同步刷过 ⇒ 帧末那次别再白算一遍 O(N)
    const next = this.mainFresh ? this.lastMainBranch : this.computeMain("flush");
    this.mainFresh = true;
    if (setsEqual(next, this.lastMainBranch)) return;
    this.lastMainBranch = next;
    this.rebuild();
  }

  /**
   * 物化 / 上翻补批后的无条件重折：`flushPending` 在主线没变时会跳过 rebuild，而插卡前摊平过的折叠段就一直摊着。
   * 增量渲染（插卡前必须 unwrapAll）插完一律走这里。
   */
  rebuildNow(): void {
    this.pendingLive = false; // 已同步刷过 ⇒ 帧末那次别再白算一遍 O(N)
    if (!this.mainFresh) this.lastMainBranch = this.computeMain("rebuild-now");
    this.mainFresh = true;
    this.rebuild();
  }

  /** Tab 销毁时调，断 GC 引用 */
  dispose(): void {
    // 已排程的帧末重算不能在 dispose 之后还去动 DOM（容器可能已被摘掉）。
    // 只置标志、不取消回调 —— rAF 的 handle 类型在两种环境下不一致，
    // 而一个「醒来发现自己该闭嘴」的回调比一个可能取消错对象的 handle 安全。
    this.disposed = true;
    this.pendingLive = false;
    this.mainFresh = false;
    this.records = [];
    this.seenUuids.clear();
    this.lastMainBranch = new Set();
    this.foldExpanded.clear();
    // 秤 4 仪表：跟着一起断引用，别让一个死 Tab 的账本状态活下去
    this.ledgerParentSeen.clear();
    this.ledgerShadowAdd.clear();
    this.ledgerSinceCompute = 0;
    this.ledgerMissSinceCompute = 0;
  }

  // === 内部 DOM 操作 ===

  /**
   * 按主线集合重折 fold 结构 —— 按段差量，不全量解开重包。
   *
   * 1. 逻辑序列：顶层子节点依次读；遇到 wrap 就读它 inner 里的卡（不搬）。
   * 2. 目标段：逻辑序列里连续的 off-main 卡（带 `data-uuid` 且不在主线；无 `data-uuid` 的元素断段）。
   * 3. 现存 wrap 若**恰好**等于某个目标段（inner 的卡 == 段成员、同序）⇒ 原地不动（展开态、DOM 都不碰）；
   *    其余 wrap 解开（卡搬回 wrap 所在位置）；没有现成 wrap 的目标段新包一个。
   *
   * DOM 写只落在归属真变了的段上（主线在尾巴上长一条时折叠卡一张都不搬）；扫描是 O(顶层子节点 ＋ 折叠卡数) 次读。
   * 等价：差量结果与「平铺容器上从零折一遍」逐字相同（`tests/frontend/ui/branch-fold-batching.vitest.ts`「C1」随机操作序列）。
   */
  private rebuild(): void {
    const mainSet = this.lastMainBranch;
    // 第 1 步：逻辑序列（wrap 展开读，不搬）
    const items: Array<{ el: HTMLElement; wrap: HTMLElement | null }> = [];
    const innerCount = new Map<HTMLElement, number>();
    for (const child of Array.from(this.container.children)) {
      const el = child as HTMLElement;
      if (el.classList.contains(FOLD_WRAP_CLASS)) {
        const inner = el.querySelector(`.${FOLD_BODY_INNER_CLASS}`);
        const kids = inner ? Array.from(inner.children) : [];
        innerCount.set(el, kids.length);
        for (const k of kids) items.push({ el: k as HTMLElement, wrap: el });
      } else {
        items.push({ el, wrap: null });
      }
    }

    // 第 2 步：目标段
    const runs: Array<Array<{ el: HTMLElement; wrap: HTMLElement | null; uuid: string }>> = [];
    let cur: (typeof runs)[number] | null = null;
    for (const it of items) {
      const uuid = it.el.getAttribute("data-uuid");
      if (uuid && !mainSet.has(uuid)) {
        if (!cur) {
          cur = [];
          runs.push(cur);
        }
        cur.push({ ...it, uuid });
      } else {
        cur = null;
      }
    }

    // 第 3 步：恰好等于目标段的现存 wrap 留着；其余解开；缺的新包
    const kept = new Set<HTMLElement>();
    const todo: typeof runs = [];
    for (const run of runs) {
      const w = run[0].wrap;
      // 「恰好等于」：段成员全在 w 里、w 里没有别的卡 —— 还要 w 的键（首条 uuid）与包它时记下的条数都没变：
      // 有卡插进段首（`insertNode` 插在锚点前，锚点在段里就插进段里）会让键过期；段内一增一减会让标题上的条数过期。
      if (
        w &&
        !kept.has(w) &&
        run.every((it) => it.wrap === w) &&
        innerCount.get(w) === run.length &&
        w.getAttribute("data-fold-key") === run[0].uuid &&
        this.wrapSize.get(w) === run.length
      ) {
        kept.add(w);
      } else {
        todo.push(run);
      }
    }
    const undone = this.unwrapFolds([...innerCount.keys()].filter((w) => !kept.has(w)));
    let wrapped = 0;
    for (const run of todo) {
      wrapped += this.wrapRun(run[0].el, run[run.length - 1].el, run.map((it) => it.uuid));
    }

    // 秤 4 仪表：这次 rebuild 搬了几个节点（解开搬回顶层 + 包进 wrap，两段都算）
    const led = branchLedger();
    if (!led) return;
    led.rebuilds++;
    led.rebuildNodesMoved += undone.moved + wrapped;
    if (led.rebuildSamples.length < LEDGER_SAMPLE_CAP) {
      led.rebuildSamples.push({
        scanned: items.length,
        unwrapped: undone.moved,
        unwrappedWraps: undone.wraps,
        wrapped,
        wraps: todo.length,
        frame: led.frames,
      });
    } else {
      led.rebuildSamplesDropped++;
    }
  }

  /** 把所有现存 fold-wrap 解开 */
  /**
   * 往折叠后的 DOM 里二分插入前先摊平：timeline 的邻居可能已被搬进 fold wrap（不是 container 的直接子节点），insertBefore 会 NotFoundError。
   * 插完由 setRecordsAndRebuild 重折。
   */
  unwrapAll(): void {
    const undone = this.unwrapAllFolds();
    // 秤 4 仪表：这一支是**不经 rebuild 的裸摊平**（增量插卡前的准备），
    // 与 rebuild 里那次分开记，否则「rebuild 搬了几个」会被它掺进去。
    const led = branchLedger();
    if (led) {
      led.standaloneUnwraps++;
      led.standaloneUnwrapNodesMoved += undone.moved;
    }
  }

  /** 返回值只给秤 4 仪表用：解开了几个 wrap、搬回顶层几个节点。 */
  private unwrapAllFolds(): { wraps: number; moved: number } {
    return this.unwrapFolds(Array.from(this.container.querySelectorAll(`:scope > .${FOLD_WRAP_CLASS}`)));
  }

  /** 解开给定的这几个 wrap（差量重折只解归属变了的那几个）。 */
  private unwrapFolds(wraps: ReadonlyArray<Element>): { wraps: number; moved: number } {
    let moved = 0;
    for (const wrap of wraps) {
      const inner = wrap.querySelector(`.${FOLD_BODY_INNER_CLASS}`);
      // 记下展开状态，下次重建可继承
      const firstUuid = wrap.getAttribute("data-fold-key");
      if (firstUuid) {
        const expanded = wrap.classList.contains("expanded");
        this.foldExpanded.set(firstUuid, expanded);
      }
      if (inner) {
        // 把 inner 里的卡片 move 回 wrap 的位置（在 container 上）
        const cards = Array.from(inner.children);
        moved += cards.length; // 秤 4 仪表
        for (const card of cards) {
          this.container.insertBefore(card, wrap);
        }
      }
      wrap.remove();
    }
    return { wraps: wraps.length, moved };
  }

  /**
   * 把 [start, end] 这一段连续元素包到 fold-wrap 里。
   * 返回值只给每帧账本用：搬进 wrap 的节点数。
   */
  private wrapRun(start: HTMLElement, end: HTMLElement, uuids: string[]): number {
    const foldKey = uuids[0]; // 用第一条 uuid 当稳定 key
    const expanded = this.foldExpanded.get(foldKey) ?? false; // 默认折叠

    const wrap = document.createElement("div");
    wrap.className = FOLD_WRAP_CLASS;
    wrap.setAttribute("data-fold-key", foldKey);
    this.wrapSize.set(wrap, uuids.length);
    // 折叠态真高 ≈ 34px（header 一行）；兜底 120px 偏大 3 倍。展开后由 content-visibility 的 auto 记住真高。
    wrap.style.setProperty("contain-intrinsic-size", "auto 34px");
    if (expanded) wrap.classList.add("expanded");

    const header = document.createElement("div");
    header.className = FOLD_HEADER_CLASS;
    header.setAttribute("role", "button");
    header.setAttribute("tabindex", "0");
    header.setAttribute("aria-expanded", expanded ? "true" : "false");

    const arrow = document.createElement("span");
    arrow.className = FOLD_ARROW_CLASS;
    arrow.appendChild(icon("caretRight", "compact"));
    header.appendChild(arrow);

    const title = document.createElement("span");
    title.className = "branch-fold-title";
    title.textContent = copyText("branchFold.wrapRun.summary", { n: uuids.length });
    header.appendChild(title);

    const toggleFn = () => {
      const nowExpanded = !wrap.classList.contains("expanded");
      wrap.classList.toggle("expanded", nowExpanded);
      header.setAttribute("aria-expanded", nowExpanded ? "true" : "false");
      this.foldExpanded.set(foldKey, nowExpanded);
    };
    header.addEventListener("click", toggleFn);
    header.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggleFn();
      }
    });

    const body = document.createElement("div");
    body.className = FOLD_BODY_CLASS;
    const inner = document.createElement("div");
    inner.className = FOLD_BODY_INNER_CLASS;
    body.appendChild(inner);

    // 把 wrap 插到 start 位置，然后把 [start, end] 全部 move 进 inner
    this.container.insertBefore(wrap, start);
    wrap.appendChild(header);
    wrap.appendChild(body);

    // 收集 start 到 end 之间的所有节点（包含两端）
    const toMove: HTMLElement[] = [];
    let cursor: ChildNode | null = start;
    while (cursor) {
      const next: ChildNode | null = cursor.nextSibling;
      toMove.push(cursor as HTMLElement);
      if (cursor === end) break;
      cursor = next;
    }
    for (const node of toMove) {
      inner.appendChild(node);
    }
    return toMove.length; // 秤 4 仪表
  }
}

// ===========================================================================
// === 每帧账本：主线算了几次 · 每次的 N 与 ms · rebuild 搬了几个节点 · 快路命中率 ===
// ===========================================================================
//
// 装在 `computeMain()`（时间）· `rebuild()`（搬动数）· `recordAdded()`（快路判定），挂 `window.__ccmPerf.branchLedger`。
// 没有 `window.__ccmPerf`（非浏览器 / 没跑 main.ts）整套不启，只维护两个 O(1) 的簿记（中途打开时读数才对）。
//
// 量不到的：
//  1. 帧号是本模块看到的 rAF tick 数，不是真实帧边界；同步路径的样本一律盖「当前帧号」⇒「一帧几次」只在 live 合批那条路上可信。
//  2. ms 是 node 的还是 WebView2 的，账本自己不知道。
//  3. 快路全命中的那一帧由 `fastMain` 真跳掉（`computesSkipped`），verify 档照旧全量算来比对，且只在整段全命中的那些次比。
//  4. 搬动数只数 move 次数，不数浏览器的布局 / 重绘代价。
//  5. `computeMs` 夹的是 `computeMainBranch` ＋ `exemptQueuedLeaves`（后者在 `queuedContents` 空时是一句 `return`）。

/** 账本里三组样本的条数上限。**超出只丢样本、不丢计数**（计数是独立累加的）。 */
const LEDGER_SAMPLE_CAP = 5000;

/** 一次 `computeMain()` 是被谁叫起来的。只是账本的一列，计算本身不看。 */
export type BranchComputeVia = "live-frame" | "flush" | "set-records" | "rebuild-now";

/** 影子快路的未命中原因，四问的顺序即优先级（见 `noteFastPathShadow` 头注）。 */
export type FastPathMissReason = "noParent" | "parentUnknown" | "parentHasChild" | "parentOffMain";

export interface BranchComputeSample {
  /** 这次真算手里有几条 records（= `computeMainBranch` 的入参长度，§2.7 的 N） */
  n: number;
  /** wall time，毫秒 */
  ms: number;
  via: BranchComputeVia;
  /** 本模块观察到的第几个 rAF tick（近似，见上面诚实段第 1 条） */
  frame: number;
}

export interface BranchRebuildSample {
  /** 这次 rebuild 顺序扫了几个 container 直接子节点 */
  scanned: number;
  /** 解开旧 fold、搬回容器顶层的节点数 */
  unwrapped: number;
  /** 解开了几个旧 wrap */
  unwrappedWraps: number;
  /** 包进新 fold 的节点数 */
  wrapped: number;
  /** 建了几个新 wrap */
  wraps: number;
  frame: number;
}

export interface BranchFastPathSample {
  hit: boolean;
  /** 命中时为 null */
  miss: FastPathMissReason | null;
  frame: number;
}

/** 秤 4 的账本。挂在 `window.__ccmPerf.branchLedger`。 */
export interface BranchFrameLedger {
  /** `computeMain()` 被夹到的次数 */
  computes: number;
  /** 上面那些次的累计 wall time（ms） */
  computeMs: number;
  computeSamples: BranchComputeSample[];
  computeSamplesDropped: number;

  /** `rebuild()` 跑了几次 / 累计搬了几个节点 */
  rebuilds: number;
  rebuildNodesMoved: number;
  rebuildSamples: BranchRebuildSample[];
  rebuildSamplesDropped: number;

  /** 不经 rebuild 的裸 `unwrapAll()`（增量插卡前摊平）次数与搬动数 */
  standaloneUnwraps: number;
  standaloneUnwrapNodesMoved: number;

  /** 本模块观察到的 rAF tick 数（近似帧号，见诚实段第 1 条） */
  frames: number;

  /** 经 `recordAdded` 进来的新记录数（= fastPathHit + fastPathMiss） */
  recordsAdded: number;
  /** 经 `setRecordsAndRebuild` 整批换进来的记录数（**不参与命中率**） */
  recordsBulkSet: number;

  /** 影子快路：命中 / 未命中 / 未命中原因分布 */
  fastPathHit: number;
  fastPathMiss: number;
  fastPathMissBy: Record<FastPathMissReason, number>;
  fastPathSamples: BranchFastPathSample[];
  fastPathSamplesDropped: number;

  /** 这次真算之前那一段记录全部命中 ⇒ 快路能整个跳掉这次 O(N)（只在 verify 档下还会真算到这一格） */
  computesSkippable: number;
  /** 档 1 快路**真跳掉**的次数（不在 `computes` 里） */
  computesSkipped: number;
  /** 那一段里至少一条没命中 ⇒ 这次 O(N) 跑不掉 */
  computesUnskippable: number;
  /** 那一段一条新记录都没来（含 `setRecordsAndRebuild` 那一支） */
  computesNoArrivals: number;

  /** verify 档开没开（由 `__ccmPerf.branchLedgerVerify === true` 决定，生产默认关） */
  verify: boolean;
  /** verify 档下：全命中那些次里，影子算出来的主线与真算**相等**的次数 */
  fastPathVerified: number;
  /** 同上，**不相等**的次数。非 0 = 档 1 的谓词本身是错的 */
  fastPathWrong: number;
}

/** `window.__ccmPerf` 的局部视图（`main.ts` 那份 `declare global` 不在这里扩）。 */
interface PerfBagWithBranchLedger {
  branchLedger?: BranchFrameLedger;
  branchLedgerVerify?: boolean;
}

function makeBranchLedger(verify: boolean): BranchFrameLedger {
  return {
    computes: 0,
    computeMs: 0,
    computeSamples: [],
    computeSamplesDropped: 0,
    rebuilds: 0,
    rebuildNodesMoved: 0,
    rebuildSamples: [],
    rebuildSamplesDropped: 0,
    standaloneUnwraps: 0,
    standaloneUnwrapNodesMoved: 0,
    frames: 0,
    recordsAdded: 0,
    recordsBulkSet: 0,
    fastPathHit: 0,
    fastPathMiss: 0,
    fastPathMissBy: { noParent: 0, parentUnknown: 0, parentHasChild: 0, parentOffMain: 0 },
    fastPathSamples: [],
    fastPathSamplesDropped: 0,
    computesSkippable: 0,
    computesSkipped: 0,
    computesUnskippable: 0,
    computesNoArrivals: 0,
    verify,
    fastPathVerified: 0,
    fastPathWrong: 0,
  };
}

/**
 * 拿账本；`window.__ccmPerf` 不存在就返回 null（仪表整套静默不启）。
 * 账本本身是**首次用到时懒建**的，`__ccmPerf` 被整份换掉即等于清零 —— 判据就靠这个复位。
 */
function branchLedger(): BranchFrameLedger | null {
  const bag = (globalThis as unknown as { __ccmPerf?: PerfBagWithBranchLedger }).__ccmPerf;
  if (!bag) return null;
  const existing = bag.branchLedger;
  if (existing) return existing;
  const fresh = makeBranchLedger(bag.branchLedgerVerify === true);
  bag.branchLedger = fresh;
  return fresh;
}

function bumpLedgerFrame(): void {
  const led = branchLedger();
  if (led) led.frames++;
}

function nowMs(): number {
  return typeof performance !== "undefined" && typeof performance.now === "function"
    ? performance.now()
    : Date.now();
}
