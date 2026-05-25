import { listen } from "@tauri-apps/api/event";
import type { JsonlRecord } from "./cards";
import type { TaskEntry } from "./tasks-panel";

export interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  message: JsonlRecord;
}

export interface SessionEndedPayload {
  session_id: string;
}

/** v2.3.0 issue #11: 后端 emit task-update 时的 payload */
export interface TasksUpdatePayload {
  sessionId: string;
  tasks: TaskEntry[];
}

/**
 * v2.3.1 (issue #1): chunk metadata 用来路由 prepend vs append。
 * - `chunkIndex === 0`：head 块 → onLine 走 append（首块定义 anchor）
 * - `chunkIndex > 0`：older 块 → onLine 走 prepend (insertBefore firstChunkAnchor)
 */
export interface BatchChunkMeta {
  chunkIndex: number;
  chunkTotal: number;
}

export interface EventHandlers {
  onLine: (e: JsonlLinePayload) => void;
  onSessionEnded: (sessionId: string) => void;
  /**
   * v2.2 (issue #12 性能): 启动重放（jsonl-batch 事件）开始时调一次。
   * TabManager 在此把所有 tab 的 BranchFolder 切到 batch 模式（recordAdded 只 push 不算）。
   *
   * v2.3.1 (issue #1)：传 chunk 元数据。chunkIndex === 0 = head 块，>0 = older 块。
   * TabManager 据此切换 append vs prepend 渲染模式。**整个 replay 期间只调一次**
   * （多块在 300ms grace 续期下被视作连续 batch）。
   */
  onBatchStart?: (meta?: BatchChunkMeta) => void;
  /**
   * v2.2: 同一批次的所有 record 都处理完后调一次。
   * TabManager 在此对每个 tab 的 BranchFolder 调 flushPending() 一次性算主线 + rebuild，
   * 然后切回 live 模式。
   *
   * v2.3.1：在多块切片场景下，**只在 300ms grace 真正触发时调一次**（即所有块都
   * 到齐 + 300ms 无新 payload），保证 BranchFolder 全 records 都到了再 fold。
   */
  onBatchEnd?: () => void;
  /**
   * v2.3.1 (issue #1)：每收到一个新 chunk 时调（除 chunk 0 外）。
   * TabManager 据此知道"现在开始处理 older 块，要 prepend"。
   */
  onChunk?: (meta: BatchChunkMeta) => void;
  /**
   * v2.3.0 issue #11: 后端 task watcher 监听到 <claude_dir>/tasks/<sid>/ 变更，
   * 重读整目录后 emit。前端按 sid 路由到对应 Tab 的 tasksPanel。
   * 不进 queue —— task 事件稀疏（人类敲命令级），直接同步派发。
   */
  onTasksUpdate?: (payload: TasksUpdatePayload) => void;
}

/** queue 中的不同事件类型，drain 按 kind 派发 */
type QueueItem =
  | { kind: "payload"; payload: JsonlLinePayload }
  | { kind: "batch-start"; meta: BatchChunkMeta }
  | { kind: "chunk"; meta: BatchChunkMeta }
  | { kind: "batch-end" };

/**
 * 批量调度参数：每个事件循环 tick 处理至多 BATCH_SIZE 条或耗时 BATCH_MS 毫秒，
 * 之后用 setTimeout(0) 让出主线程。replay 会一次性 emit 数千条 jsonl-line，
 * 同步处理会阻塞 click 派发数秒（鼠标光标卡死、滚动可用——native 滚动绕过主线程）。
 * 批量 + 让出后，单批 ≤ BATCH_MS 仍能在 1 帧内完成，UI 响应不再被压垮。
 */
const BATCH_SIZE = 40;
const BATCH_MS = 8;

/**
 * v2.3 (issue #1 性能修): 启动重放的 jsonl-batch 后，后端 EventReplay 释放锁，
 * 之前积压在 watcher → event_replay record() 路径上的 record 会**逐条** live emit
 * `jsonl-line`。这些事件**没被 batch-start/end 哨兵包裹**，于是按原实现立即切回
 * live 模式 → 每条 record 都走 per-record O(N) computeMainBranch → 启动后明显
 * 第二次卡顿（用户报告"先快一会儿然后变慢"）。
 *
 * 修法：把 onBatchEnd 调用**延迟** {@link BATCH_END_GRACE_MS}。延迟期间任何新
 * payload 都续期 timer（继续保留 batch 模式 + 只 push 不算）。真正稳定后 timer
 * 触发 → onBatchEnd 一次 flushPending + 切 live。
 *
 * 这样把"replay 释放锁后短期内积压的 live emit"自然吸收进同一个 batch 区间。
 *
 * 阈值选择：300ms 远大于积压解锁后涌出来一波的时间窗（实测几十 ms），又小到不影响
 * 真实时新消息（用户发一条消息的间隔 ≥ 几秒，CLI 自己也得 stream response）。
 */
const BATCH_END_GRACE_MS = 300;

export function bindEvents(handlers: EventHandlers): void {
  const queue: QueueItem[] = [];
  let scheduled = false;

  // batch-end 延迟状态机
  let inBatchMode = false;
  let endTimer: number | null = null;

  // === perf 测量 ===
  let payloadCount = 0;
  const perf = window.__ccmPerf ?? {};

  const scheduleBatchEnd = (): void => {
    if (endTimer !== null) {
      clearTimeout(endTimer);
    }
    endTimer = window.setTimeout(() => {
      endTimer = null;
      if (inBatchMode) {
        inBatchMode = false;
        try {
          handlers.onBatchEnd?.();
        } catch (e) {
          console.error("[events] onBatchEnd threw:", e);
        }
        // perf：真正稳定（无更多积压）→ 输出完整 timeline
        perf.onBatchEndFired = performance.now();
        emitPerfSummary(perf, payloadCount);
      }
    }, BATCH_END_GRACE_MS);
  };

  const enterBatchMode = (meta: BatchChunkMeta): void => {
    if (endTimer !== null) {
      clearTimeout(endTimer);
      endTimer = null;
    }
    if (inBatchMode) {
      // 已在 batch 模式收到新 chunk → 不重入 onBatchStart，但 chunk 路由给 TabManager
      try {
        handlers.onChunk?.(meta);
      } catch (e) {
        console.error("[events] onChunk threw:", e);
      }
      return;
    }
    inBatchMode = true;
    try {
      handlers.onBatchStart?.(meta);
    } catch (e) {
      console.error("[events] onBatchStart threw:", e);
    }
  };

  const dispatchItem = (item: QueueItem): void => {
    try {
      if (item.kind === "payload") {
        // 在 batch-end 延迟窗口内来 payload → 续期 timer 保持 batch 模式
        if (inBatchMode && endTimer !== null) {
          scheduleBatchEnd();
        }
        if (perf.firstPayloadDrained === undefined) {
          perf.firstPayloadDrained = performance.now();
        }
        payloadCount += 1;
        handlers.onLine(item.payload);
      } else if (item.kind === "batch-start") {
        enterBatchMode(item.meta);
      } else if (item.kind === "chunk") {
        // 切到新 chunk（仅在已 batch 模式时触发，让 TabManager 改 prepend 模式）
        try {
          handlers.onChunk?.(item.meta);
        } catch (e) {
          console.error("[events] onChunk threw:", e);
        }
      } else if (item.kind === "batch-end") {
        // 不直接切回 live，延迟 300ms；期间积压 payload / 下一个 chunk 会续期 timer
        // perf：记录 batch payload 全部 drain 完毕的时刻（不是 onBatchEnd fire）
        perf.batchDrainEnd = performance.now();
        scheduleBatchEnd();
      }
    } catch (e) {
      // v2.1.1: try/catch 防御 —— 单条 record 处理出错不能冻死整个 replay
      // queue（v2.1.0 踩过：computeMainBranch stack overflow → drain 异常逃逸
      // → 后续上千条 record 永远不渲染）。错单条记日志继续。
      console.error("[events] handler threw, skipping item:", item, e);
    }
  };

  const drain = (): void => {
    const start = performance.now();
    let processed = 0;
    while (
      queue.length > 0 &&
      processed < BATCH_SIZE &&
      performance.now() - start < BATCH_MS
    ) {
      const item = queue.shift();
      if (item) dispatchItem(item);
      processed += 1;
    }
    if (queue.length > 0) {
      // 让出主线程一帧再处理下一批
      setTimeout(drain, 0);
    } else {
      scheduled = false;
    }
  };

  const ensureScheduled = (): void => {
    if (scheduled || queue.length === 0) return;
    scheduled = true;
    // 用 setTimeout(0) 而非 queueMicrotask，确保批与批之间真正让出
    // （microtask 同一 tick 内连续清空，无让出效果）
    setTimeout(drain, 0);
  };

  void listen<JsonlLinePayload>("jsonl-line", (e) => {
    queue.push({ kind: "payload", payload: e.payload });
    ensureScheduled();
  });

  // v1.7.13: 启动时 replay 用 jsonl-batch 一次性发整个 history（替代之前的
  // N 次单条 jsonl-line emit，省 200-400ms 启动 IPC overhead）。
  // v2.2 (issue #12): 包裹 batch-start / batch-end 哨兵，让 TabManager 在
  // 重放期把 BranchFolder 切 batch 模式（每条 push 不算），结束后 flush 一次。
  // 避免重放 2000 条时每条都 O(N) computeMainBranch 累计 O(N²) 卡 UI。
  void listen<{
    chunkIndex: number;
    chunkTotal: number;
    payloads: JsonlLinePayload[];
  }>("jsonl-batch", (e) => {
    const meta: BatchChunkMeta = {
      chunkIndex: e.payload.chunkIndex,
      chunkTotal: e.payload.chunkTotal,
    };
    if (perf.firstJsonlBatch === undefined) {
      perf.firstJsonlBatch = performance.now();
      console.info(
        `[perf] first jsonl-batch received @ ${perf.firstJsonlBatch.toFixed(0)}ms · chunk ${meta.chunkIndex + 1}/${meta.chunkTotal} payload=${e.payload.payloads.length}`,
      );
    }
    // chunk 0 → batch-start (TabManager 进 batch 模式 + 记 anchor)
    // chunk >0 → chunk 事件（TabManager 切到 prepend 模式）
    // 不管 chunk index，所有 payload 入 queue
    if (meta.chunkIndex === 0) {
      queue.push({ kind: "batch-start", meta });
    } else {
      queue.push({ kind: "chunk", meta });
    }
    for (const p of e.payload.payloads) {
      queue.push({ kind: "payload", payload: p });
    }
    // 每块都发 batch-end —— 300ms grace 会续期等下一块或真新消息
    queue.push({ kind: "batch-end" });
    ensureScheduled();
  });

  // session-ended 事件稀疏，直接同步派发
  void listen<SessionEndedPayload>("session-ended", (e) =>
    handlers.onSessionEnded(e.payload.session_id),
  );

  // v2.3.0 issue #11: task-update 同样稀疏，绕过 queue 直接派发
  void listen<TasksUpdatePayload>("task-update", (e) => {
    handlers.onTasksUpdate?.(e.payload);
  });
}

/** 启动管线 perf timeline 输出（onBatchEnd 真正 fire 时调一次） */
function emitPerfSummary(
  p: typeof window.__ccmPerf,
  payloadCount: number,
): void {
  const dom = p.domContentLoaded ?? 0;
  const theme = p.themeLoaded ?? 0;
  const ready = p.frontendReadyEmit ?? 0;
  const firstBatch = p.firstJsonlBatch ?? 0;
  const firstDrain = p.firstPayloadDrained ?? 0;
  const drainEnd = p.batchDrainEnd ?? 0;
  const endFired = p.onBatchEndFired ?? performance.now();

  const lines = [
    `[perf] === 启动管线 timeline (前端 performance.now ms) ===`,
    `[perf]   DOMContentLoaded       T+${dom.toFixed(0)}`,
    `[perf]   loadTheme done         T+${theme.toFixed(0)} (+${(theme - dom).toFixed(0)})`,
    `[perf]   emit frontend-ready    T+${ready.toFixed(0)} (+${(ready - theme).toFixed(0)})`,
    `[perf]   first jsonl-batch in   T+${firstBatch.toFixed(0)} (+${(firstBatch - ready).toFixed(0)}) ← 后端 replay 完成`,
    `[perf]   first payload dispatch T+${firstDrain.toFixed(0)} (+${(firstDrain - firstBatch).toFixed(0)})`,
    `[perf]   batch payloads drained T+${drainEnd.toFixed(0)} (+${(drainEnd - firstDrain).toFixed(0)}) ← drain 全部 ${payloadCount} 条`,
    `[perf]   onBatchEnd fired       T+${endFired.toFixed(0)} (+${(endFired - drainEnd).toFixed(0)}) ← 300ms grace 后真正切 live`,
    `[perf]   ── 总耗时（DOM → 加载完成）：${(endFired - dom).toFixed(0)}ms ──`,
  ];
  console.info(lines.join("\n"));
}
