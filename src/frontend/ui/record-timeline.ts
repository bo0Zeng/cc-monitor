/**
 * RecordTimeline：单个 Tab / SessionViewer 持有的按 seq 排序的渲染条目。
 *
 * 每来一条记录 `insert(entry)` 二分找位置、insertBefore 相邻元素 ⇒ 后端先发哪条都不影响画面，永远按 seq 排好
 * （重放历史与实时增量走同一条路，不再分相位协调）。seq 只在同一会话内比（后端每份文件一个单调计数器）。
 *
 * insert 走 `stream.insertNode`：同步触发贴底，不靠 ResizeObserver 的异步回调 —— 重放期高频插入时滚动条才跟得上。
 * 工具组的邻居合并在 `render-stream-record.ts`（与 renderMessage 的产物耦合），这里只给 insert 与邻居查询。
 */

import type { ToolGroup } from "./cards";
import type { MessageStream } from "./stream";

export interface TimelineEntry {
  /** 后端 watcher 给的 per-file 单调 seq */
  seq: number;
  /** 卡片 / tool-group root 的 DOM 元素 */
  element: HTMLElement;
  /**
   * 渲染语义类别——给 tool-group 后处理判邻居用。
   * - `card`：普通卡（user / assistant 含 text / slash / compact / agent-tool 等）
   * - `tool-group`：tool-only assistant 渲染产出的工具组卡（可后处理合并）
   * - `aside`：旁注（系统注入的细条，开关关着时不显示）——**不算任何人的邻居**：左右邻居查询跳过它，
   *   相邻合并（工具组 · 重试 · 后台通知）照它不在时一样合
   */
  kind: "card" | "tool-group" | "aside";
  /**
   * tool-group entry 持有 ToolGroup 实例，供后续 tool-only 邻居 addToToolGroup 用。
   * card entry 此字段为 null。
   */
  toolGroup?: ToolGroup | null;
}

export class RecordTimeline {
  /** 按 seq 升序排列的 entries */
  private entries: TimelineEntry[] = [];

  constructor(private stream: MessageStream) {}

  /**
   * 按 seq 插入新 entry 并立即挂 DOM。返回插入位置 index（调用方合并时要看左右邻居）。
   * 重放期的旧记录不进 timeline（尾部优先收纳在 TailWindow 账本，不建卡；INVARIANTS §21.3）⇒ 这里没有挂载状态机。
   */
  insert(entry: TimelineEntry): number {
    const idx = this.binarySearchInsertIdx(entry.seq);
    this.entries.splice(idx, 0, entry);
    // 锚点 = 第一个还在这条流里的后继。已离场的（元素摘出了 DOM 却没出账）当场出账并出声：拿它当锚只能末尾追加，DOM 就此错序（rebuild 不重排卡）。
    // 所有摘卡的路都同步出账（reconcile · 骨架占位），走到这里说明多了一条没出账的路。
    const content = this.stream.contentElement;
    const j = idx + 1;
    while (j < this.entries.length && !content.contains(this.entries[j].element)) {
      console.warn(`[timeline] seq ${this.entries[j].seq} 的元素已不在流里却没出账 —— 当场出账（D4）`);
      this.entries.splice(j, 1);
    }
    this.stream.insertNode(entry.element, this.entries[j]?.element ?? null);
    return idx;
  }

  /** 从 `i` 起朝 `dir` 找第一个不是旁注的条目。 */
  private near(i: number, dir: -1 | 1): TimelineEntry | null {
    while (i >= 0 && i < this.entries.length && this.entries[i].kind === "aside") i += dir;
    return this.entries[i] ?? null;
  }

  /** 直接查某个 seq 是否已存在（dedup 用，理论上不该有同 seq 但防御） */
  has(seq: number): boolean {
    const idx = this.binarySearchInsertIdx(seq);
    return idx < this.entries.length && this.entries[idx].seq === seq;
  }

  /**
   * 查"假如 seq 此刻插入，它的左邻居是谁"——不真 insert。
   * tool-group 后处理用：判断新 tool-only 是否能合到左侧已有 ToolGroup。
   */
  peekPrev(seq: number): TimelineEntry | null {
    return this.near(this.binarySearchInsertIdx(seq) - 1, -1);
  }

  /** 当前 entries 数量 */
  get size(): number {
    return this.entries.length;
  }

  /** 最高已渲染 seq（空 timeline = -Infinity）。中部插入判定用。 */
  get maxSeq(): number {
    return this.entries.length > 0
      ? this.entries[this.entries.length - 1].seq
      : Number.NEGATIVE_INFINITY;
  }

  /**
   * 按 element 删 entry：reconcilePendingToolResults 把孤儿 fallback 卡从 DOM 摘掉后同步删账，免得它之后被选作锚点。
   * 线性扫描（单次 reconcile 移除数 ≤ pending 数）。
   */
  removeByElement(el: HTMLElement): void {
    const idx = this.entries.findIndex((e) => e.element === el);
    if (idx >= 0) this.entries.splice(idx, 1);
  }

  /** Tab 关闭 / SessionViewer dispose 时调，断 GC 引用 */
  dispose(): void {
    this.entries = [];
  }

  /**
   * 二分查找：返回 entry 应该插入的位置 idx，使 entries[idx-1].seq < seq <= entries[idx].seq。
   *
   * O(log N)。
   * 出错时（同 seq 已存在）也返回正确插入位置 —— caller 用 `has(seq)` 自行判重。
   */
  private binarySearchInsertIdx(seq: number): number {
    let lo = 0;
    let hi = this.entries.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (this.entries[mid].seq < seq) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    return lo;
  }
}
