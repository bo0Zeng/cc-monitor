/**
 * issue #8：ESC 回退分支的折叠 UI。
 *
 * 输入：一个 message stream 容器，里面已经按 jsonl 顺序追加好了一堆卡片。
 *      被识别为"主线"的 uuid 集合。
 * 输出：让连续的"被回退" off-main 卡片包到一个折叠容器里，header 显示
 *      "▶ 已被 ESC 回退（含 N 条消息）"。
 *
 * **重建策略**（DOM 重排）：
 *  1. 先把所有现存 `.branch-fold-wrap` 解开 —— 把里面的卡 move 回容器顶层
 *  2. 顺序扫容器子元素：
 *     - 如果是带 data-uuid 的卡 + 在 mainBranch → 留在原位
 *     - 如果是带 data-uuid 的卡 + 不在 mainBranch → 加入"当前 run"
 *     - 工具组 / 其他无 uuid 元素：结束当前 run（如果有），自身保持原位
 *  3. 每个 run 结束时（≥1 张卡），用 `.branch-fold-wrap` 包起来，header 显示数量
 *
 * **为什么 unwrap-then-rewrap 而不是增量**：
 * - 分支变化是非局部的：一条新 user record 可能让一大段原本 on-main 的卡片
 *   转 off-main（如果它指向更早的 parent，把它们都甩到旧分支上）
 * - 增量 diff 复杂度高、bug 多；全量重建是 O(N) DOM 操作，N=几百到几千都在
 *   一帧渲染预算内
 *
 * **折叠状态保留**：每个 fold-wrap 在 unwrap 前把 expanded class 状态记到 outer
 * 容器的 `data-fold-expanded:<rangeKey>` ……不行，rangeKey 不稳定。
 * 改为：用 fold 的第一条 uuid 当 key 写到 容器级 Map。重建时按这个 key 查表。
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
  /** 上次重建用的 mainBranch；判等避免无 diff 时空重排 */
  private lastMainBranch: Set<string> = new Set();
  /** 折叠 ID（每个 fold 的第一条 uuid） → 用户是否手动展开了 */
  private foldExpanded = new Map<string, boolean>();

  constructor(container: HTMLElement) {
    this.container = container;
  }

  /**
   * 卡片刚 append 完之后调一次。caller 给出 uuid / parentUuid / timestamp（用于
   * 主线识别）。
   *
   * 返回 true 表示有 fold 结构变化（off-main 集合变了），调用方可据此做日志。
   */
  recordAdded(rec: BranchRecord): boolean {
    this.records.push(rec);
    const next = computeMainBranch(this.records);
    if (setsEqual(next, this.lastMainBranch)) return false;
    this.lastMainBranch = next;
    this.rebuild();
    return true;
  }

  /**
   * 批量场景（session-viewer 一次 load 全部历史）：先 push 所有 records，
   * 然后调一次 rebuildAll。比逐条 recordAdded 省一堆中间 rebuild。
   */
  setRecordsAndRebuild(records: ReadonlyArray<BranchRecord>): void {
    this.records = records.slice();
    this.lastMainBranch = computeMainBranch(this.records);
    this.rebuild();
  }

  /** Tab 销毁时调，断 GC 引用 */
  dispose(): void {
    this.records = [];
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
