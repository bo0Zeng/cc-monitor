/**
 * RecordTimeline（P5.2b）—— B 重构的核心数据结构。
 *
 * ## 是什么
 *
 * 单个 Tab / SessionViewer 持有的"按 seq 排序的渲染条目数组"。每次后端
 * emit 一条新 payload，调用 `insert(entry)` 用 binary search 找位置 →
 * DOM insertBefore 相邻 element。**消除了之前 inPrependMode / pendingPrependFragment
 * / source: batch | live / chunk_index 全部状态机**——后端 emit 顺序不影响
 * 视觉，timeline 永远按 seq 排好。
 *
 * ## 为什么这么干
 *
 * 之前一根 jsonl-batch / jsonl-line IPC 通道塞两种语义（replay 历史 vs live 增量），
 * 前端用 6 个 flag 互相协调（inPrependMode / pendingPrependFragment / source /
 * batchMode / replaying / endTimer）。每对相位差都是潜在 bug——5 个已知漏洞 +
 * tool-group 跨 source 错位 + 第二批 batch chunk 0 不重置 等。
 *
 * 改成 timeline + seq：插入位置由 seq 决定，状态机降到 1（inBatch 仅控 lazy hljs）。
 *
 * ## 单 tab 视角不变
 *
 * 跨 session 不需要全局 seq——每个 tab 独立 timeline，seq 只在 same-session 内
 * 比较。watcher 给每文件维护 next_seq 计数器，process_file 顺序读保证单调。
 *
 * ## DOM 模型
 *
 * timeline 持有 MessageStream 实例（不是裸 contentEl），insert 走 stream.insertNode
 * —— 同步触发 stickToBottom 的 snap 贴底逻辑，避免依赖 ResizeObserver 异步窗口。
 * 启动 chunked replay 期间 timeline.insert 高频调用，**必须同步贴底**，否则滚动条
 * 跟不上内容增长，视觉上停在中间 / 顶部（已知 bug，B 重构后回归一次）。
 *
 * ## tool-group 合并
 *
 * 本模块只暴露 insert + neighbor 查询；tool-group 后处理合并算法在
 * `render-stream-record.ts` 里（P5.3）实现，因为它跟 renderMessage 输出耦合。
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
   */
  kind: "card" | "tool-group";
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
   * 按 seq 插入新 entry 并立即挂 DOM。返回插入位置 index（caller 后处理合并要看
   * 左右邻居）。
   *
   * Batch13-F40a:deferMode（启动重放延后挂载）已退役——尾部优先门控让重放期的
   * 旧记录根本不进 timeline（收纳在 TailWindow 账本，不建卡），"视口上方逐帧插入
   * 导致 ±0.5px 抖动"的病根整个消失（INVARIANTS §21.3）。本类回归纯粹的
   * 「seq 有序 + 邻居查询」结构，无挂载状态机。
   */
  insert(entry: TimelineEntry): number {
    const idx = this.binarySearchInsertIdx(entry.seq);
    this.entries.splice(idx, 0, entry);
    const nextEntry = this.entries[idx + 1];
    this.stream.insertNode(entry.element, nextEntry?.element ?? null);
    return idx;
  }

  /** 查 idx 处 entry 的左右邻居（tool-group 后处理合并要用） */
  neighborsAt(idx: number): { prev: TimelineEntry | null; next: TimelineEntry | null } {
    return {
      prev: this.entries[idx - 1] ?? null,
      next: this.entries[idx + 1] ?? null,
    };
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
    const idx = this.binarySearchInsertIdx(seq);
    return this.entries[idx - 1] ?? null;
  }

  /** 当前 entries 数量 */
  get size(): number {
    return this.entries.length;
  }

  /** Tab 关闭 / SessionViewer dispose 时调，断 GC 引用 */
  dispose(): void {
    this.entries = [];
  }

  /**
   * 二分查找：返回 entry 应该插入的位置 idx，使 entries[idx-1].seq < seq <= entries[idx].seq。
   *
   * 平均 O(log N)；N=3000 实测 ~12 次比较，毫秒以下。
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
