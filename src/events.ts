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
      if (p) {
        // v2.1.1: try/catch 防御 —— 单条 record 处理出错不能冻死整个 replay
        // queue（v2.1.0 踩过：computeMainBranch stack overflow → drain 异常逃逸
        // → 后续上千条 record 永远不渲染）。错单条记日志继续。
        try {
          handlers.onLine(p);
        } catch (e) {
          console.error("[events] onLine threw, skipping record:", p, e);
        }
      }
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

  // v1.7.13: 启动时 replay 用 jsonl-batch 一次性发整个 history（替代之前的
  // N 次单条 jsonl-line emit，省 200-400ms 启动 IPC overhead）。
  // 拿到 Vec 后 push 进 queue 走原批量 drain 逻辑，UI 仍然分帧不卡。
  void listen<JsonlLinePayload[]>("jsonl-batch", (e) => {
    for (const p of e.payload) {
      queue.push(p);
    }
    if (!scheduled && queue.length > 0) {
      scheduled = true;
      setTimeout(drain, 0);
    }
  });

  // session-ended 事件稀疏，直接同步派发
  void listen<SessionEndedPayload>("session-ended", (e) =>
    handlers.onSessionEnded(e.payload.session_id),
  );
}
