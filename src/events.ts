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
}

/**
 * 批量调度参数：每个事件循环 tick 处理至多 BATCH_SIZE 条或耗时 BATCH_MS 毫秒，
 * 之后用 setTimeout(0) 让出主线程。replay 会一次性 emit 数千条 jsonl-line，
 * 同步处理会阻塞 click 派发数秒（鼠标光标卡死、滚动可用——native 滚动绕过主线程）。
 * 批量 + 让出后，单批 ≤ BATCH_MS 仍能在 1 帧内完成，UI 响应不再被压垮。
 */
const BATCH_SIZE = 40;
const BATCH_MS = 8;

export function bindEvents(handlers: EventHandlers): void {
  const queue: JsonlLinePayload[] = [];
  let scheduled = false;

  const drain = (): void => {
    const start = performance.now();
    let processed = 0;
    while (
      queue.length > 0 &&
      processed < BATCH_SIZE &&
      performance.now() - start < BATCH_MS
    ) {
      const p = queue.shift();
      if (p) handlers.onLine(p);
      processed += 1;
    }
    if (queue.length > 0) {
      // 让出主线程一帧再处理下一批
      setTimeout(drain, 0);
    } else {
      scheduled = false;
    }
  };

  void listen<JsonlLinePayload>("jsonl-line", (e) => {
    queue.push(e.payload);
    if (!scheduled) {
      scheduled = true;
      // 用 setTimeout(0) 而非 queueMicrotask，确保批与批之间真正让出
      // （microtask 同一 tick 内连续清空，无让出效果）
      setTimeout(drain, 0);
    }
  });

  // session-ended 事件稀疏，直接同步派发
  void listen<SessionEndedPayload>("session-ended", (e) =>
    handlers.onSessionEnded(e.payload.session_id),
  );
}
