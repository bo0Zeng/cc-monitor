/**
 * issue #8：ESC 回退分支的折叠 UI。
 *
 * 输入：一个 message stream 容器（已按 jsonl 顺序追加好卡片）+ 主线 uuid 集合
 * 输出：把连续的 off-main 卡片包到 `.branch-fold-wrap` 折叠容器
 *
 * **重建策略**（unwrap-then-rewrap 全量重建）：
 *  1. 解开所有现存 `.branch-fold-wrap`，把里面的卡 move 回容器顶层
 *  2. 顺序扫容器子元素：
 *     - 带 data-uuid 且在 mainBranch → on-main，断 run
 *     - 带 data-uuid 但不在 mainBranch → 加入当前 off-main run
 *     - 无 data-uuid 元素：断 run
 *  3. 每个 run 包到一个 wrap 里
 *
 * **为什么全量重建而不是增量**：分支变化非局部 —— 一条新 user record 可能让一
 * 大段原本 on-main 的卡转 off-main（指向更早 parent，把它们甩到旧分支）。增量
 * diff 复杂度高 bug 多，全量重建是 O(N) DOM 操作，N=几百到几千都在一帧预算内。
 *
 * **折叠状态保留**：fold-wrap 在 unwrap 前把 expanded 写到 `foldExpanded` Map
 * （key = run 的首条 uuid，稳定）；wrap 时按 key 查表恢复。
 */

import { computeMainBranch, setsEqual, type BranchRecord } from "./branching";

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
  /**
   * issue #25：已见 uuid 集，recordAdded 拒重。投递层是 at-least-once（watcher
   * 截断重读会换 seq 重投整个文件，seenSeqs 防不住，违反此约束见
   * doc/INVARIANTS.md § 25），重复记录会毒化 computeMainBranch 的 Kahn 拓扑 →
   * 大段误折叠。computeMainBranch 入口也有去重（双层防御）；这里挡住还能避免
   * records 数组被重投无界增长。
   */
  private seenUuids = new Set<string>();
  /** 上次重建用的 mainBranch；判等避免无 diff 时空重排 */
  private lastMainBranch: Set<string> = new Set();
  /** 折叠 ID（每个 fold 的第一条 uuid） → 用户是否手动展开了 */
  private foldExpanded = new Map<string, boolean>();
  /**
   * v2.2 (issue #12 性能优化)：batch 模式。
   *
   * - **batch = true**（重放期）：recordAdded 只 push 不算，不 rebuild。直到
   *   调用方调 flushPending() 才一次性 computeMainBranch + rebuild。
   *   适合启动时 event_replay 批量灌 2000+ 条 jsonl 的场景，省 O(N²)。
   * - **batch = false**（live 模式，默认）：recordAdded 立刻算 + 可能 rebuild。
   *   适合真实时新消息到达，每条 1 帧内反映 fold 状态。
   *
   * caller（TabManager）通过 setBatchMode(true) → 灌 records → flushPending() →
   * setBatchMode(false) 控制切换。
   */
  private batchMode = false;

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
    if (this.seenUuids.has(rec.uuid)) return; // issue #25：重投拒收（见字段注释）
    this.seenUuids.add(rec.uuid);
    this.records.push(rec);
    if (this.batchMode) return; // batch 模式：延后到 flush
    const next = computeMainBranch(this.records);
    if (setsEqual(next, this.lastMainBranch)) return;
    this.lastMainBranch = next;
    this.rebuild();
  }

  /**
   * 批量场景（session-viewer 一次 load 全部历史）：先 push 所有 records，
   * 然后调一次 rebuildAll。比逐条 recordAdded 省一堆中间 rebuild。
   */
  setRecordsAndRebuild(records: ReadonlyArray<BranchRecord>): void {
    // issue #25：与 recordAdded 同等拒重（去重后存，保首见）
    this.seenUuids = new Set();
    this.records = [];
    for (const r of records) {
      if (!this.seenUuids.has(r.uuid)) {
        this.seenUuids.add(r.uuid);
        this.records.push(r);
      }
    }
    this.lastMainBranch = computeMainBranch(this.records);
    this.rebuild();
  }

  /**
   * v2.2: 切换 batch 模式。切到 batch 后到 flushPending 之间的 recordAdded
   * 都不会触发计算 / rebuild。切回 live 不会自动 flush，需 caller 显式调 flushPending。
   */
  setBatchMode(enabled: boolean): void {
    this.batchMode = enabled;
  }

  /**
   * v2.2: 在 batch 模式累计完后调一次，计算最新主线并 rebuild。
   * 也可在 live 模式手动调（等价于 setRecordsAndRebuild 但保持现有 records）。
   */
  flushPending(): void {
    const next = computeMainBranch(this.records);
    if (setsEqual(next, this.lastMainBranch)) return;
    this.lastMainBranch = next;
    this.rebuild();
  }

  /** Tab 销毁时调，断 GC 引用 */
  dispose(): void {
    this.records = [];
    this.seenUuids.clear();
    this.lastMainBranch = new Set();
    this.foldExpanded.clear();
  }

  // === 内部 DOM 操作 ===

  /** 全量重建 fold 结构 */
  private rebuild(): void {
    // 第 1 步：把所有 fold-wrap 解开 —— 把 inner 卡片 move 回 container 顶层
    this.unwrapAllFolds();

    // 第 2 步：扫 container.children，找连续的 off-main run
    const mainSet = this.lastMainBranch;
    const children = Array.from(this.container.children);
    let runStart: HTMLElement | null = null;
    const runs: Array<{ start: HTMLElement; end: HTMLElement; uuids: string[] }> = [];

    for (const child of children) {
      const el = child as HTMLElement;
      const uuid = el.getAttribute("data-uuid");
      const isOffMain = !!uuid && !mainSet.has(uuid);
      if (isOffMain) {
        if (!runStart) {
          runStart = el;
          runs.push({ start: el, end: el, uuids: [uuid!] });
        } else {
          // 延伸当前 run
          const last = runs[runs.length - 1];
          last.end = el;
          last.uuids.push(uuid!);
        }
      } else {
        // 非 off-main：断开 run
        runStart = null;
      }
    }

    // 第 3 步：对每个 run 用 fold-wrap 包起来
    for (const run of runs) {
      this.wrapRun(run.start, run.end, run.uuids);
    }
  }

  /** 把所有现存 fold-wrap 解开 */
  private unwrapAllFolds(): void {
    const wraps = Array.from(this.container.querySelectorAll(`:scope > .${FOLD_WRAP_CLASS}`));
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
        for (const card of cards) {
          this.container.insertBefore(card, wrap);
        }
      }
      wrap.remove();
    }
  }

  /** 把 [start, end] 这一段连续元素包到 fold-wrap 里 */
  private wrapRun(start: HTMLElement, end: HTMLElement, uuids: string[]): void {
    const foldKey = uuids[0]; // 用第一条 uuid 当稳定 key
    const expanded = this.foldExpanded.get(foldKey) ?? false; // 默认折叠

    const wrap = document.createElement("div");
    wrap.className = FOLD_WRAP_CLASS;
    wrap.setAttribute("data-fold-key", foldKey);
    if (expanded) wrap.classList.add("expanded");

    const header = document.createElement("div");
    header.className = FOLD_HEADER_CLASS;
    header.setAttribute("role", "button");
    header.setAttribute("tabindex", "0");
    header.setAttribute("aria-expanded", expanded ? "true" : "false");

    const arrow = document.createElement("span");
    arrow.className = FOLD_ARROW_CLASS;
    arrow.textContent = "▶";
    header.appendChild(arrow);

    const title = document.createElement("span");
    title.className = "branch-fold-title";
    title.textContent = `已被 ESC 回退（含 ${uuids.length} 条消息）`;
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
  }
}
