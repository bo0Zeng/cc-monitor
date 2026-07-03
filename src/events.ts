import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { JsonlRecord } from "./cards";
import type { TaskEntry } from "./tasks-panel";

export interface JsonlLinePayload {
  session_id: string;
  cwd: string | null;
  path: string;
  /**
   * P5.1：per-file 单调递增的行号（后端 watcher 给）。
   * 前端 RecordTimeline 按 seq 排序到 DOM —— 后端 emit 顺序不影响视觉。
   * 同 session 内单调；跨 session 不可比；不跨 monitor 进程持久。
   */
  seq: number;
  /**
   * issue #15：数据来源标签。缺省（undefined）= 本地，Tab 标题无前缀（与历史一致）；
   * 有值（如 "raspberrypi.local"）= 远端 SSH 数据源主机名，Tab 标题加 `[origin]` 前缀
   * 以区分本地/远端会话。后端仅在远端行序列化此字段（本地行 skip）。
   */
  origin?: string;
  message: JsonlRecord;
}

export interface SessionEndedPayload {
  session_id: string;
}

/** 会话（重新）变活事件 payload（镜像 bridge.rs::SessionStartedPayload）。
 *  Batch7-F24：附 pidfile 元信息——前端无 Tab 时建骨架（中途出现的本地 bg 会话）。 */
export interface SessionStartedPayload {
  session_id: string;
  cwd: string | null;
  kind: string | null;
  name: string | null;
}

/** v2.3.0 issue #11: 后端 emit task-update 时的 payload */
export interface TasksUpdatePayload {
  sessionId: string;
  tasks: TaskEntry[];
}

export interface EventHandlers {
  /**
   * P5.2 B 重构：onLine 不再带 source 参数。
   * payload 携带 seq，前端 RecordTimeline 按 seq 排到 DOM，emit 顺序不影响视觉。
   */
  onLine: (e: JsonlLinePayload) => void;
  onSessionEnded: (sessionId: string) => void;
  /**
   * 会话（重新）变活（SESSION_STARTED）。后端在 sessions/<PID>.json 新增**且 PID
   * 探活通过**时 emit —— resume 场景：崩溃→Tab 灰显→`/resume` 后回 live，无需 F5。
   * 与 session-ended 同进 queue（保持「结束/复活」相对后端 emit 顺序，见下方 sub 注释）。
   * 前端复活已归档的本地 Tab（tabs.reviveTab）。
   */
  onSessionStarted?: (
    sessionId: string,
    meta: { cwd: string | null; kind: string | null; name: string | null },
  ) => void;
  /** Batch5-F18：远端会话宣告 → 建骨架 Tab（不等首行）。Batch7-F24：附 pidfile
   *  元信息（p1e daemon 起有值；旧 daemon → null）。 */
  onRemoteSessionAdded?: (
    sessionId: string,
    origin: string,
    meta: { kind: string | null; cwd: string | null; name: string | null },
  ) => void;
  /**
   * v2.2 (issue #12 性能): 启动重放（jsonl-batch 第一块）到达时调一次。
   * TabManager 在此把所有 tab 的 BranchFolder 切到 batch 模式 + lazy hljs 开关。
   * **整个 replay 期间只调一次**（多块在 300ms grace 续期下被视作连续 batch）。
   *
   * P5.2 B 重构：删了 meta 参数 — 前端不再依赖 head/older 区分（timeline 按 seq 排）。
   */
  onBatchStart?: () => void;
  /**
   * v2.2: 同一批次的所有 record 都处理完后调一次。
   * TabManager 调 flushPending 算主线 + rebuild，切回 live 模式。
   *
   * v2.3.1：在多块切片场景下，**只在 300ms grace 真正触发时调一次**。
   */
  onBatchEnd?: () => void;
  /**
   * v2.3.0 issue #11: 后端 task watcher 监听到 <claude_dir>/tasks/<sid>/ 变更，
   * 重读整目录后 emit。前端按 sid 路由到对应 Tab 的 tasksPanel。
   * 不进 queue —— task 事件稀疏（人类敲命令级），直接同步派发。
   */
  onTasksUpdate?: (payload: TasksUpdatePayload) => void;
  /**
   * issue #23：会话红绿灯。后端仅在 sessions/<PID>.json 的官方 status 变化时
   * emit（天然稀疏，同 session-ended 直接同步派发）。status: "busy"=运行中 /
   * "idle"/"shell"=等输入 / "waiting"=等弹窗决定（waiting_for 细分原因）。
   */
  onSessionActivity?: (payload: SessionActivityPayload) => void;
}

/** issue #23：session-activity 事件 payload（镜像 bridge.rs::SessionActivityPayload） */
export interface SessionActivityPayload {
  session_id: string;
  status: string | null;
  waiting_for: string | null;
}

/** queue 中的不同事件类型，drain 按 kind 派发 */
type QueueItem =
  | { kind: "payload"; payload: JsonlLinePayload }
  | { kind: "batch-start" }
  | { kind: "batch-end" }
  | { kind: "ended"; sessionId: string }
  | {
      kind: "started";
      sessionId: string;
      cwd: string | null;
      sessionKind: string | null;
      name: string | null;
    }
  // Batch5-F18：远端会话宣告（daemon session_added 透传）——骨架 Tab 入口。
  // 走同一 queue 与 ended/started/行保序（INVARIANT § 20 / issue #20 教训）。
  | {
      kind: "remote-added";
      sessionId: string;
      origin: string;
      sessionKind: string | null;
      cwd: string | null;
      name: string | null;
    };

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

/** bindEvents 选项。 */
export interface BindEventsOptions {
  /**
   * issue #10：独立 viewer 窗口用 `true` —— 改用 `getCurrentWebviewWindow().listen`
   * （注册成 `WebviewWindow{label}` 监听）而非模块级 `listen`（注册成 `Any` 监听）。
   *
   * 为什么必须：后端 `replay_session_to_window` 用 `emit_to(本窗口)` **定向**发历史
   * （不广播，否则污染主窗口 timeline）。Tauri 2 事件按 target-kind 匹配：定向发射
   * 命不中 `Any` 监听，所以模块 `listen` 收不到 → viewer 空白。带标签监听才接得到
   * 定向事件；而广播 `Any`（live jsonl-line）是通配，带标签监听照样收得到。
   * 主窗口只收广播（`Any`），保持模块 `listen` 即可，不传此项。
   */
  windowScoped?: boolean;
}

/**
 * 订阅后端事件。**返回 Promise，resolve 时所有 listener 已在 Rust 侧注册完成**。
 *
 * 为什么 async：`listen()` 本身是异步的（内部 invoke 注册），在它 resolve 前 emit 的
 * 事件会丢。主窗口靠 frontend-ready 往返 + watcher 扫描的天然延迟掩盖了这个竞态；但
 * issue #10 独立窗口在 bindEvents 后立刻调 replay_session_to_window 定向发历史 —— 不
 * await 注册就会把 1599 条历史全丢（实测白屏只剩状态栏）。caller 必须 `await bindEvents`
 * 再触发任何会导致后端 emit 的调用（frontend-ready / replay_session_to_window）。
 */
export async function bindEvents(
  handlers: EventHandlers,
  opts: BindEventsOptions = {},
): Promise<void> {
  // windowScoped 时用窗口作用域监听（详 BindEventsOptions.windowScoped）。
  // 用泛型 wrapper 而非 .bind —— .bind 会丢失 listen 的泛型，破坏 sub<T> 调用点类型。
  const wv = opts.windowScoped ? getCurrentWebviewWindow() : null;
  const sub = <T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> =>
    wv ? wv.listen<T>(event, handler) : listen<T>(event, handler);

  const queue: QueueItem[] = [];
  let scheduled = false;

  // batch-end 延迟状态机
  let inBatchMode = false;
  // Batch5-F17 突发检测：jsonl-line 积压超过该深度 → 主动进 batch 模式（与后端
  // INCREMENTAL_BATCH_THRESHOLD=50 同量级）。burstArmed 防哨兵在生效前重复入队。
  const BURST_ENTER_THRESHOLD = 50;
  let burstArmed = false;
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

  const enterBatchMode = (): void => {
    burstArmed = false; // 突发哨兵已生效（或被 jsonl-batch 的哨兵抢先），解除武装
    if (endTimer !== null) {
      clearTimeout(endTimer);
      endTimer = null;
    }
    if (inBatchMode) {
      // 已在 batch 模式收到下一块 batch-start —— P5.2 B 重构后无 onChunk 概念，
      // 直接 ignore（lazy hljs / BranchFolder.batchMode 仍开着，无需重入）。
      return;
    }
    inBatchMode = true;
    try {
      handlers.onBatchStart?.();
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
        enterBatchMode();
      } else if (item.kind === "batch-end") {
        // 不直接切回 live，延迟 300ms；期间积压 payload / 下一个 chunk 会续期 timer
        // perf：记录 batch payload 全部 drain 完毕的时刻（不是 onBatchEnd fire）
        perf.batchDrainEnd = performance.now();
        scheduleBatchEnd();
      } else if (item.kind === "ended") {
        handlers.onSessionEnded(item.sessionId);
      } else if (item.kind === "started") {
        handlers.onSessionStarted?.(item.sessionId, {
          cwd: item.cwd,
          kind: item.sessionKind,
          name: item.name,
        });
      } else if (item.kind === "remote-added") {
        handlers.onRemoteSessionAdded?.(item.sessionId, item.origin, {
          kind: item.sessionKind,
          cwd: item.cwd,
          name: item.name,
        });
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
      // Batch5-F17：突发检测进入的 batch 模式没有 batch-end 哨兵可依赖——
      // 队列清空且没有已排程的退出 timer 时补排一个（grace 期内新 payload
      // 照常续期）。对哨兵路径无影响（batch-end 已排 timer → 条件不成立）。
      if (inBatchMode && endTimer === null) {
        scheduleBatchEnd();
      }
    }
  };

  const ensureScheduled = (): void => {
    if (scheduled || queue.length === 0) return;
    scheduled = true;
    // 用 setTimeout(0) 而非 queueMicrotask，确保批与批之间真正让出
    // （microtask 同一 tick 内连续清空，无让出效果）
    setTimeout(drain, 0);
  };

  // 收集所有 listen() 注册 promise，函数末尾 await —— 保证返回时监听已就绪。
  const registrations: Promise<unknown>[] = [];

  registrations.push(
    sub<JsonlLinePayload>("jsonl-line", (e) => {
      queue.push({ kind: "payload", payload: e.payload });
      // Batch5-F17 突发检测兜底：jsonl-line 没有 batch 哨兵包裹，任何突发源
      // （历史上是远端 snapshot 逐帧，未来任何新源）积压到阈值就主动进 batch
      // 模式——哨兵插队到队首，让剩余积压走 defer/lazy 路径而不是逐条全量渲染。
      // 已在 batch 模式或哨兵已入队则不重复；退出走 drain 清空后的 grace 补排。
      if (!inBatchMode && !burstArmed && queue.length > BURST_ENTER_THRESHOLD) {
        burstArmed = true;
        queue.unshift({ kind: "batch-start" });
      }
      ensureScheduled();
    }),
  );

  // v1.7.13: 启动时 replay 用 jsonl-batch 一次性发整个 history（替代之前的
  // N 次单条 jsonl-line emit，省 200-400ms 启动 IPC overhead）。
  // v2.2 (issue #12): 包裹 batch-start / batch-end 哨兵，让 TabManager 在
  // 重放期把 BranchFolder 切 batch 模式（每条 push 不算），结束后 flush 一次。
  //
  // P5.2 B 重构：chunkIndex / chunkTotal 元数据仍在后端 payload 里（兼容），
  // 但前端 dispatcher 不再用 —— 所有 payload 走单一路径，前端 timeline 按 seq 排序。
  registrations.push(
    sub<{
      chunkIndex: number;
      chunkTotal: number;
      payloads: JsonlLinePayload[];
    }>("jsonl-batch", (e) => {
      if (perf.firstJsonlBatch === undefined) {
        perf.firstJsonlBatch = performance.now();
        console.info(
          `[perf] first jsonl-batch received @ ${perf.firstJsonlBatch.toFixed(0)}ms · chunk ${e.payload.chunkIndex + 1}/${e.payload.chunkTotal} payload=${e.payload.payloads.length}`,
        );
      }
      // 第一块触发 batch-start（后续块在 grace 续期内被视作同一 batch）。
      if (e.payload.chunkIndex === 0) {
        queue.push({ kind: "batch-start" });
      }
      for (const p of e.payload.payloads) {
        queue.push({ kind: "payload", payload: p });
      }
      // 每块末尾发 batch-end —— 300ms grace 内有新 payload / 新块都续期
      queue.push({ kind: "batch-end" });
      ensureScheduled();
    }),
  );

  // session-ended 必须进 queue 与行事件同序处理（issue #20）：之前同步派发，会
  // 抢在积压的 replay 行之前执行 —— 归档刚落实，后续 drain 的远端行就命中
  // tabs.ts ensureTab 的远端 un-archive（archived + origin!==null 见行即复活），
  // 重载对账补发的归档被原样吃掉 → 僵尸 live Tab。入队后前端处理顺序 = 后端
  // emit 顺序（重放块全部在前、补发 ended 在后；实时 ended 也天然晚于该会话的行：
  // daemon 协议 removed 帧在行帧之后）。tabs.ts 的 pendingArchive 保留为防御层
  //（§ 17a 双层防御：万一 ended 仍早于建 Tab 的行，建 Tab 时落实归档）。
  registrations.push(
    sub<SessionEndedPayload>("session-ended", (e) => {
      queue.push({ kind: "ended", sessionId: e.payload.session_id });
      ensureScheduled();
    }),
  );

  // session-started 与 session-ended 同进 queue：保持「结束/复活」相对后端 emit 顺序，
  // 避免 started 抢在仍排队的 ended 之前同步执行而错误复活（issue #20 同序原则的对称面）。
  // 后端已用 is_session_active 门控，只在 PID 真活时发本事件 → 复活安全。
  registrations.push(
    sub<SessionStartedPayload>("session-started", (e) => {
      queue.push({
        kind: "started",
        sessionId: e.payload.session_id,
        cwd: e.payload.cwd ?? null,
        sessionKind: e.payload.kind ?? null,
        name: e.payload.name ?? null,
      });
      ensureScheduled();
    }),
  );

  // Batch5-F18：远端会话宣告同进 queue——骨架建 Tab 与该会话的行/ended 保序。
  registrations.push(
    sub<{
      session_id: string;
      origin: string;
      kind: string | null;
      cwd: string | null;
      name: string | null;
    }>("remote-session-added", (e) => {
      queue.push({
        kind: "remote-added",
        sessionId: e.payload.session_id,
        origin: e.payload.origin,
        sessionKind: e.payload.kind ?? null,
        cwd: e.payload.cwd ?? null,
        name: e.payload.name ?? null,
      });
      ensureScheduled();
    }),
  );

  // v2.3.0 issue #11: task-update 同样稀疏，绕过 queue 直接派发
  registrations.push(
    sub<TasksUpdatePayload>("task-update", (e) => {
      handlers.onTasksUpdate?.(e.payload);
    }),
  );

  // issue #23: session-activity 稀疏（CLI 仅在状态转换时写），同步派发
  registrations.push(
    sub<SessionActivityPayload>("session-activity", (e) => {
      handlers.onSessionActivity?.(e.payload);
    }),
  );

  // 等所有 listener 在 Rust 侧注册完成再返回（防 emit-before-listen 丢事件）。
  await Promise.all(registrations);
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
