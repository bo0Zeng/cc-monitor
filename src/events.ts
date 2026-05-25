import { listen } from "@tauri-apps/api/event";
import type { JsonlRecord } from "./cards";

export interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  message: JsonlRecord;
}

export interface SessionEndedPayload {
  session_id: string;
}

export interface EventHandlers {
  onLine: (e: JsonlLinePayload) => void;
  onSessionEnded: (sessionId: string) => void;
  /**
   * v2.2 (issue #12 性能): 启动重放（jsonl-batch 事件）开始时调一次。
   * TabManager 在此把所有 tab 的 BranchFolder 切到 batch 模式（recordAdded 只 push 不算）。
   */
  onBatchStart?: () => void;
  /**
   * v2.2: 同一批次的所有 record 都处理完后调一次。
   * TabManager 在此对每个 tab 的 BranchFolder 调 flushPending() 一次性算主线 + rebuild，
   * 然后切回 live 模式。
   */
  onBatchEnd?: () => void;
}

/** queue 中的不同事件类型，drain 按 kind 派发 */
type QueueItem =
  | { kind: "payload"; payload: JsonlLinePayload }
  | { kind: "batch-start" }
  | { kind: "batch-end" };

/**
 * 批量调度参数：每个事件循环 tick 处理至多 BATCH_SIZE 条或耗时 BATCH_MS 毫秒，
 * 之后用 setTimeout(0) 让出主线程。replay 会一次性 emit 数千条 jsonl-line，
 * 同步处理会阻塞 click 派发数秒（鼠标光标卡死、滚动可用——native 滚动绕过主线程）。
 * 批量 + 让出后，单批 ≤ BATCH_MS 仍能在 1 帧内完成，UI 响应不再被压垮。
 */
const BATCH_SIZE = 40;
const BATCH_MS = 8;

export function bindEvents(handlers: EventHandlers): void {
  const queue: QueueItem[] = [];
  let scheduled = false;

  const dispatchItem = (item: QueueItem): void => {
    try {
      if (item.kind === "payload") {
        handlers.onLine(item.payload);
      } else if (item.kind === "batch-start") {
        handlers.onBatchStart?.();
      } else if (item.kind === "batch-end") {
        handlers.onBatchEnd?.();
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
  void listen<JsonlLinePayload[]>("jsonl-batch", (e) => {
    queue.push({ kind: "batch-start" });
    for (const p of e.payload) {
      queue.push({ kind: "payload", payload: p });
    }
    queue.push({ kind: "batch-end" });
    ensureScheduled();
  });

  // session-ended 事件稀疏，直接同步派发
  void listen<SessionEndedPayload>("session-ended", (e) =>
    handlers.onSessionEnded(e.payload.session_id),
  );
}
