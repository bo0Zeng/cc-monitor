import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import { commands } from "./ipc/commands";
import { chan, type Item, type Sub } from "./ipc/chan";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { showActionFailureToast } from "./error-toast";
import { ACCOUNTS_CHANGED_KIND, ACCOUNTS_CHANGED_WINDOW, accountsChangedItems } from "./session-accounts-poll";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
// C02（rust-ts-boundary）：这 5 个 payload 类型**改成从生成物 re-export**，不再手写。
// 源是 `src/bridge/src/bridge.rs` 的 `#[cfg_attr(test, derive(ts_rs::TS))]`。
// **仍然 `export`**，但理由要写准（C02 Phase D 审计 S4）：初版写的是「别的模块从这里
// import 这些类型」——**那句可被 grep 否证**：这 5 个名字除本文件与守卫测试外，
// 全仓没有任何 import 点。真正的理由是**不缩小已经导出的表面**（保守、零代价，
// 且若将来有人要用，从枢纽拿是对的地方）。
// 对照：`tasks-panel.ts::TaskEntry` 那处同款注释是**准的**——它真有外部消费者（`tabs.ts`）。
//
// **C04c 起 `JsonlLinePayload` / `JsonlBatchPayload` 也是生成的。**
// C02 时这里写着「用生成物替换它是一次渲染层重构，不是一次类型迁移 ⇒ 延后」
// ——**实测把这个判断推翻了**：直接接生成物一共只有 6 个 `tsc` 错，其中 4 个是机械的
// （`export type {…} from` 不带名字进本地作用域），真错只有 2 处 `string | null` vs
// `string | undefined`，且都能在**消费侧**修（`branching.ts` / `turn-notify.ts`），
// 一行 `tabs.ts` 都不用碰。
//
// 那段理由本身也有一处方向反了（C02 audit I4 已订正）：这个边界上
// **Rust 的 `JsonlRecord` 就是线定义**（wire == `serde_json::to_string(它)`），
// 而 TS 侧那份手抄反而**更窄**（8 vs 12 个 variant），所以方向是让 TS 对齐 Rust。
import type { JsonlLinePayload } from "./generated/JsonlLinePayload";
import type { SessionStreamFrame } from "./generated/SessionStreamFrame";
import type { SessionEndedPayload } from "./generated/SessionEndedPayload";
import type { SessionIdlePayload } from "./generated/SessionIdlePayload";
import type { SessionStartedPayload } from "./generated/SessionStartedPayload";
import type { TasksUpdatePayload } from "./generated/TasksUpdatePayload";
import type { SessionActivityPayload } from "./generated/SessionActivityPayload";
import type { RemoteSessionAddedPayload } from "./generated/RemoteSessionAddedPayload";
import type { SessionContainerPayload } from "./generated/SessionContainerPayload";
import type { SessionTapPayload } from "./generated/SessionTapPayload";
import type { OriginSessionsListedPayload } from "./generated/OriginSessionsListedPayload";
import type { SessionUnseenPayload } from "./generated/SessionUnseenPayload";
// 本文件内部也用这些名字（8 处），所以 import + re-export 都要有：
// 只写 `export type { … } from` 不会把名字带进本地作用域。
export type {
  JsonlLinePayload,
  SessionEndedPayload,
  SessionIdlePayload,
  SessionStartedPayload,
  TasksUpdatePayload,
  SessionActivityPayload,
};


export interface EventHandlers {
  /**
   * P5.2 B 重构：onLine 不再带 source 参数。
   * payload 携带 seq，前端 RecordTimeline 按 seq 排到 DOM，emit 顺序不影响视觉。
   */
  onLine: (e: JsonlLinePayload) => void;
  onSessionEnded: (sessionId: string) => void;
  /**
   * audit-fixes F03.2：远端 claude 退出但 tmux 会话仍在 → 灰灯（idle-tmux）。后端 emitter
   * 收 backend removed 且 `@ccm_sid` present 时 emit `session-idle`（**不** emit session-ended，
   * 故不归档）。与 session-ended 同进 queue：二者对同一 sid 互斥（emitter removed 臂择一），
   * 但需相对该会话的行/后续 remote-added 保序（idle 在行之后、复活 remote-added 之前）。
   * 前端 tabs.markTmuxIdle 置灰点。
   */
  onSessionIdle?: (sessionId: string) => void;
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
   *  元信息（p1e backend 起有值；旧 backend → null）。 */
  onRemoteSessionAdded?: (
    sessionId: string,
    origin: string,
    meta: {
      kind: string | null;
      /** E73：`null` = 没说（旧 backend / 存量会话）= 视为可以 attach。 */
      attachable: boolean | null;
      cwd: string | null;
      name: string | null;
    },
  ) => void;
  /**
   * 〔U4b · 第四波〕活会话住在什么容器里（`session-container`，本机与远端同一个事件）。
   * 进 queue：与 `remote-added` / 行保序（先建 tab、再落容器；早到的由 TabManager 暂存）。
   */
  onSessionContainer?: (sessionId: string, container: string) => void;
  /**
   * 〔U4b · 第四波〕某台机器的活会话清单报完了（`origin-sessions-listed`）。进 queue：排在那台的
   * `remote-added` 之后 ⇒ 处理它时，那台此刻全部的活会话都已宣告过（`设计/30 §3.5.7a`）。
   */
  onOriginSessionsListed?: (origin: string) => void;
  /**
   * 〔TAP · V124 · `设计/20 §8`〕中转抄出来的一个 SSE 事件：会话流 `session-tap`（通道 `subscribe`，`设计/05 §15`）里的一格。
   * **不进 queue**：活卡是临时态，与行 / 起停事件之间不需要顺序（jsonl 那一轮到了整轮覆盖、墓碑挡迟到的 tap）；
   * 进 queue 反倒会让 token 级的洪峰排在行前面。credit 在本格处理完当场还。
   */
  onSessionTap?: (e: SessionTapPayload) => void;
  /**
   * 〔TAP〕那台机器的 tap 流看不见了（订阅里的 `Unseen`：本机后端那条流断了）⇒ 那台上还开着的响应不会再有下文，活卡全撤。
   */
  onSessionTapLost?: (origin: Origin) => void;
  /**
   * 〔DL1 · `设计/01 §2.2`〕某台机器的账号清单**可能**变了（`accounts-changed` 流里的 `seen` / `frame` / `gap`，
   * 一批只叫一次）⇒ 强制刷账号清单与 chip。替掉裸 Tauri 事件 `remote-backend-ready`。
   */
  onAccountsChanged?: () => void;
  /**
   * 〔FW1 · 第四波 4D · D-d〕活会话的记录文件不见了（`change` = `"gone"`）/ 被改过已从头重读（`"truncated"` / `"rewritten"`）。
   * 会话流里的一格（`{"file_notice": …}`），与行同序：重读出来的行排在它后面。
   */
  onSessionFileNotice?: (sessionId: string, change: string) => void;
  /**
   * 〔GP1 · 第四波〕这条会话所在的那台机器看不见了（`session-unseen`：连接断了 / F5 时那台还没报完清单）⇒ 说不清。
   * 进 queue：与行 / `remote-added` / `listed` 保序（断连那一刻之前的行先落，重连之后的重宣告与清单后到）。
   */
  onSessionUnseen?: (sessionId: string) => void;
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
  /**
   * 〔CF2 · 第四波 4B〕那台机器的会话流里**丢了几格**（没 credit 时句柄丢了、原位报的 `gap`，`设计/05 §3.3.4`）。
   * 丢的是哪几个会话的哪几行，流里说不出来（一条订阅里混着多个会话）⇒ 宿主对那台机器的每个 tab 按行号补
   * （`TabManager.onStreamGap`）。进 queue：与行保序（丢在哪两格之间，补就从那里起）。
   */
  onStreamGap?: (origin: Origin) => void;
}

/**
 * 〔CF2 · 第四波 4B〕**会话流一开始给多少 credit**（格数；一格 = 一行或一个批边界）。
 *
 * 它就是这一侧队列的上界（`设计/05 §3.3.4` 级 1 在 webview 这一跳第一次成立）：句柄交出去的格
 * 不会多于我们给的 credit，而每处理掉一格还一格（`want`，按 drain 一片批量还）。
 * 取 20 000：与原来一次 F5 重放的量级相当（每个会话 ≤ 750 条 × 十几二十个会话）——
 * 小了，F5 重放会一次次停下来等 credit（慢，但不丢）；大了，前端落后时的队列跟着长。
 * 落后超过一整个窗口的实时行被句柄丢掉、原位报 `gap`（`onStreamGap` 按行号补）。
 */
export const STREAM_WINDOW = 20_000;

/**
 * 〔TAP · V124〕`session-tap` 订阅的 credit 窗口（格）：webview 这一跳在途的 tap 最多这么多格，超了 monitor 那一侧丢、
 * 位置照占、原位 `Gap`（`05 §3.3.4` 级 2）。每格处理完当场还 ⇒ 正常节奏下窗口永远不会见底；只有 webview 卡住（最小化、
 * 长任务）时才丢 —— 丢了由活卡的位置号 `n` 看出缺口、撤卡，jsonl 定稿。值与后端 tap 通道同一个量级（256）。
 */
export const TAP_WINDOW = 256;

/** 〔CF2〕一条会话流订阅在本文件里的账：还没还的 credit。`sub` 在登记那一跳回来之前是 `null`。 */
interface StreamHold {
  sub: Sub | null;
  owed: number;
}

/** queue 中的不同事件类型，drain 按 kind 派发 */
type QueueItem =
  // 〔CF2〕流里来的三种带 `grant`：处理掉它就还那条订阅一格 credit。
  | { kind: "payload"; payload: JsonlLinePayload; grant?: StreamHold }
  | { kind: "batch-start"; grant?: StreamHold }
  | { kind: "batch-end"; grant?: StreamHold }
  // 〔CF2〕那台机器的流里丢了几格（`gap`）。
  | { kind: "gap"; origin: Origin }
  | { kind: "ended"; sessionId: string }
  // audit-fixes F03.2：灰灯（idle-tmux）——与 ended 同 queue 保序，见 onSessionIdle。
  | { kind: "idle"; sessionId: string }
  | {
      kind: "started";
      sessionId: string;
      cwd: string | null;
      sessionKind: string | null;
      name: string | null;
    }
  // Batch5-F18：远端会话宣告（backend session_added 透传）——骨架 Tab 入口。
  // 走同一 queue 与 ended/started/行保序（INVARIANT § 20 / issue #20 教训）。
  | {
      kind: "remote-added";
      sessionId: string;
      origin: string;
      sessionKind: string | null;
      /** E73：attach 进去对人有没有意义。`null` = 没说（旧 backend / 存量会话）= 视为可以。 */
      attachable: boolean | null;
      cwd: string | null;
      name: string | null;
    }
  // 〔U4b · 第四波〕容器事实 / 某台清单报完了 —— 同一 queue 保序（见 EventHandlers 里两条的注释）。
  | { kind: "container"; sessionId: string; container: string }
  | { kind: "listed"; origin: string }
  // 〔GP1 · 第四波〕那台机器看不见了 —— 同一 queue 保序（见 EventHandlers.onSessionUnseen）。
  | { kind: "unseen"; sessionId: string }
  // 〔FW1 · 第四波 4D · D-d〕记录文件不见了 / 被改过已从头重读 —— 流里的一格，与行同序（见 EventHandlers.onSessionFileNotice）。
  | { kind: "file-notice"; sessionId: string; change: string; grant?: StreamHold };

/**
 * 批量调度参数。replay 会一次性 emit 数千条 jsonl-line，同步处理会阻塞 click 派发数秒
 * （鼠标光标卡死、滚动可用——native 滚动绕过主线程）。分批 + 让出后 UI 响应不再被压垮。
 *
 * - `BATCH_SIZE` / `BATCH_MS`：**猜出来的固定预算**，只在问不到「有没有人在操作」时用
 *   （见 {@link makeInputPendingProbe}）。
 * - `BATCH_MS_MAX`：能问的时候的**硬上限**。`isInputPending()` 只报告**输入**事件，
 *   它不知道"该画一帧了" ⇒ 只听它的话，一个没人碰鼠标的长队列能把主线程占住几秒而它一路说"不急"。
 */
const BATCH_SIZE = 40;
const BATCH_MS = 8;
const BATCH_MS_MAX = 50;

/**
 * ★ 步 4（`设计/10 §2.4`）：**让开的方式换成不会被规范钳制的那一种。**
 *
 * 原来是 `setTimeout(drain, 0)` 重新排自己。HTML 规范对**嵌套超过 5 层**的 timer
 * 强制最小 4ms ⇒ 真实节奏是「干 8ms、被迫歇 4ms」，利用率只有 2/3；
 * 上万条记录光排队就要好几秒。`MessageChannel` 同为宏任务，规范**没有**给它这条钳制。
 *
 * ⚠ **必须带特性探测**（`§2.4` 逐字要求）。两个生产壳（WebView2 / WebKitGTK）都有它，
 * 但本模块也在 jsdom / node 里被跑，而且"两个壳都有"是**今天**的事实，不是一条不变量。
 * 探不到就退回 `setTimeout` —— 慢，但不会静默地一条都不排（那是重放整个停摆）。
 */
function makeYieldToMain(run: () => void): () => void {
  if (typeof MessageChannel === "function") {
    try {
      const ch = new MessageChannel();
      ch.port1.onmessage = (): void => run();
      return (): void => ch.port2.postMessage(null);
    } catch {
      // 建不出来（某些受限环境）⇒ 落到下面的降级，不抛。
    }
  }
  return (): void => {
    setTimeout(run, 0);
  };
}

/**
 * ★ 步 4：**「干多久」从猜改成问。**
 *
 * `navigator.scheduling.isInputPending()` **只有 Chromium 有**
 * ⇒ Windows 的 WebView2 有、Linux 的 WebKitGTK **没有** ⇒ 探不到就退回固定预算。
 * 返回 `null` = 这台机器问不了。
 */
function makeInputPendingProbe(): (() => boolean) | null {
  if (typeof navigator === "undefined") return null;
  const sched = (
    navigator as Navigator & { scheduling?: { isInputPending?: () => boolean } }
  ).scheduling;
  if (!sched || typeof sched.isInputPending !== "function") return null;
  return (): boolean => {
    try {
      return sched.isInputPending!();
    } catch {
      // 问不动了就当"有人在操作"——宁可多让一次，也不要把一次异常变成一直干下去。
      return true;
    }
  };
}

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
  /**
   * 〔CF2 · 第四波 4B〕要订的会话流：`(origin, kind)`（`kind` = `session-lines` 整台机器 · `session-lines/<sid>` 一个会话）。
   * 会话内容**只**从这里来（原来的 `jsonl-line` / `jsonl-batch` 两个事件已退役）。在其余监听都注册完之后订，
   * `bindEvents` 返回时 monitor 那一侧已经登记好（主界面接着发 `frontend-ready` 就是它们的就绪点）。
   */
  streams?: ReadonlyArray<{ origin: Origin; kind: string }>;
  /**
   * 〔TAP · V124〕要订 `session-tap` 的机器（中转住本机常驻后端 ⇒ 今天只有本机那一台有来源）。
   * 与会话行同一条帧路、同一套 credit（`设计/05 §15`）；窗口是 {@link TAP_WINDOW}。
   */
  taps?: ReadonlyArray<Origin>;
  /**
   * 〔DL1〕要订 `accounts-changed` 的机器（那台的长连接又通了 / 那台后端说账号清单变了 ⇒ {@link EventHandlers.onAccountsChanged}）。
   * 与会话行 · tap 同一条帧路、同一处 `chan.subscribe`；窗口是 `ACCOUNTS_CHANGED_WINDOW`。
   */
  accounts?: ReadonlyArray<Origin>;
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
  // Batch9-F30：远端快照/回填在途计数（后端 snapshot-inflight 事件驱动）。
  // >0 时 batch 结束定时器只续期不触发——慢链路回填 chunk 间隔 >300ms 不再
  // 提前退出 batch 模式（退出后旧历史以 live 形态插时间线中段，增量分支
  // 计算路径没被锤过——审计推演的唯一乱序风险点）。5min 上限防呆（后端
  // inflight 卡死不至于永久压住 flush）。
  let snapshotInflight = 0;
  let batchModeSince = 0;
  const BATCH_HOLD_MAX_MS = 5 * 60_000;
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
      // Batch9-F30：回填在途 → 续期（除非超 5min 防呆上限）
      if (
        snapshotInflight > 0 &&
        performance.now() - batchModeSince < BATCH_HOLD_MAX_MS
      ) {
        scheduleBatchEnd();
        return;
      }
      // 防呆上限触发：连带把计数清零（审计 D：若后端计数事件曾乱序粘在非零，
      // 别让后续每个 batch 都再被拖满 5min——下一次真实事件会重新校准）
      if (snapshotInflight > 0) {
        console.warn("[events] snapshot-inflight 卡在非零达上限，强制清零");
        snapshotInflight = 0;
      }
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
    batchModeSince = performance.now();
    try {
      handlers.onBatchStart?.();
    } catch (e) {
      console.error("[events] onBatchStart threw:", e);
    }
  };

  // 〔CF2〕处理掉的流格攒着还 credit（每片 drain 末尾还一次，不是逐格一次 IPC）。
  const owedHolds = new Set<StreamHold>();
  const flushGrants = (): void => {
    for (const h of owedHolds) {
      if (h.sub && h.owed > 0) {
        h.sub.want(h.owed);
        h.owed = 0;
      }
    }
    owedHolds.clear();
  };

  const dispatchItem = (item: QueueItem): void => {
    if ("grant" in item && item.grant) {
      item.grant.owed += 1;
      owedHolds.add(item.grant);
    }
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
      } else if (item.kind === "idle") {
        handlers.onSessionIdle?.(item.sessionId);
      } else if (item.kind === "started") {
        handlers.onSessionStarted?.(item.sessionId, {
          cwd: item.cwd,
          kind: item.sessionKind,
          name: item.name,
        });
      } else if (item.kind === "remote-added") {
        handlers.onRemoteSessionAdded?.(item.sessionId, item.origin, {
          kind: item.sessionKind,
          attachable: item.attachable,
          cwd: item.cwd,
          name: item.name,
        });
      } else if (item.kind === "container") {
        handlers.onSessionContainer?.(item.sessionId, item.container);
      } else if (item.kind === "listed") {
        handlers.onOriginSessionsListed?.(item.origin);
      } else if (item.kind === "unseen") {
        handlers.onSessionUnseen?.(item.sessionId);
      } else if (item.kind === "file-notice") {
        handlers.onSessionFileNotice?.(item.sessionId, item.change);
      } else if (item.kind === "gap") {
        handlers.onStreamGap?.(item.origin);
      }
    } catch (e) {
      // v2.1.1: try/catch 防御 —— 单条 record 处理出错不能冻死整个 replay
      // queue（v2.1.0 踩过：computeMainBranch stack overflow → drain 异常逃逸
      // → 后续上千条 record 永远不渲染）。错单条记日志继续。
      console.error("[events] handler threw, skipping item:", item, e);
    }
  };

  // 步 4：这台机器能不能问「有没有输入事件在排队」。探一次，之后每条记录问一下
  //（`isInputPending` 本身很便宜，贵的是猜错）。`null` = 问不了 ⇒ 走固定预算。
  const askInputPending = makeInputPendingProbe();
  // 步 4：`drain → drain` 那条自链的让开方式。**只建一次**——
  // 每批新建一个 `MessageChannel` 是两个 port 的垃圾，且端口不关就是泄漏。
  const yieldToDrain = makeYieldToMain((): void => drain());

  const drain = (): void => {
    const start = performance.now();
    let processed = 0;
    while (queue.length > 0) {
      const item = queue.shift();
      if (item) dispatchItem(item);
      processed += 1;
      const elapsed = performance.now() - start;
      if (askInputPending) {
        // ★ 步 4②：**能问就别猜。** 8ms 之前连问都不问（让开本身要花一次宏任务跳），
        // 之后一路干到「真有输入事件在排队」或撞上硬上限。
        if (elapsed >= BATCH_MS && (askInputPending() || elapsed >= BATCH_MS_MAX)) break;
      } else if (processed >= BATCH_SIZE || elapsed >= BATCH_MS) {
        // 问不了（WebKitGTK 没有 `isInputPending`）⇒ 退回原来那套固定预算，一字不动。
        break;
      }
    }
    flushGrants();
    if (queue.length > 0) {
      // 让出主线程一跳再处理下一批（`MessageChannel`，探不到才退回 `setTimeout`）
      yieldToDrain();
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
    // 用宏任务而非 queueMicrotask，确保批与批之间真正让出
    // （microtask 同一 tick 内连续清空，无让出效果）。
    //
    // 🔴 **这一跳仍然是 `setTimeout`，不是 `MessageChannel`** —— 不是漏改：
    // 规范的 4ms 钳制只打**嵌套层级 > 5** 的 timer，而层级是从「当前正在跑的任务」继承的。
    // 这一跳是从事件回调里排的（层级 1）⇒ **它本来就不被钳**。
    // 被钳的只有 `drain → drain` 那条自链，换掉的也正是那一条（见 `yieldToDrain`）。
    // ⇒ 入口保持 `setTimeout` 还白得一样东西：它在假定时器下是**决定性**的，
    //    而 `MessagePort` 的投递时机不归假定时器管（实测：`advanceTimersByTime` 推不动它）。
    setTimeout(drain, 0);
  };

  // 收集所有 listen() 注册 promise，函数末尾 await —— 保证返回时监听已就绪。
  const registrations: Promise<unknown>[] = [];

  // Batch9-F30：快照 inflight（不进 queue——纯 batch 调度信号，无顺序语义）。
  // 归零时若 batch 模式在续期等待，下一次定时器触发即正常收尾。
  registrations.push(
    sub<{ count: number }>("snapshot-inflight", (ev) => {
      snapshotInflight = ev.payload.count;
    }),
  );

  // 〔CF2 · 第四波 4B〕会话内容**不再是** `jsonl-line` / `jsonl-batch` 两个事件：走通道的 `subscribe`
  //   （本函数末尾按 `opts.streams` 订）。一格 = 一行（`{"line": …}`）或成批那一段的边界（`{"batch": …}`）。
  //   进 queue 的样子与原来逐字相同：行 ⇒ payload；批边界 ⇒ batch-start / batch-end 哨兵。
  const onStreamItems = (origin: Origin, hold: StreamHold, items: Item[]): void => {
    for (const it of items) {
      if (it.t === "frame") {
        let f: SessionStreamFrame | null = null;
        try {
          f = JSON.parse(it.body) as SessionStreamFrame;
        } catch {
          f = null;
        }
        if (f !== null && typeof f === "object" && "line" in f) {
          queue.push({ kind: "payload", payload: f.line, grant: hold });
          // Batch5-F17 突发检测兜底：逐行来的实时格没有 batch 哨兵包裹，任何突发源
          // （历史上是远端 snapshot 逐帧，未来任何新源）积压到阈值就主动进 batch
          // 模式——哨兵插队到队首，让剩余积压走 defer/lazy 路径而不是逐条全量渲染。
          // 已在 batch 模式或哨兵已入队则不重复；退出走 drain 清空后的 grace 补排。
          if (!inBatchMode && !burstArmed && queue.length > BURST_ENTER_THRESHOLD) {
            burstArmed = true;
            queue.unshift({ kind: "batch-start" });
          }
        } else if (f !== null && typeof f === "object" && "file_notice" in f) {
          queue.push({
            kind: "file-notice",
            sessionId: f.file_notice.session_id,
            change: f.file_notice.change,
            grant: hold,
          });
        } else if (f !== null && typeof f === "object" && "batch" in f) {
          if (f.batch === "start") {
            if (perf.firstJsonlBatch === undefined) {
              perf.firstJsonlBatch = performance.now();
              console.info(`[perf] first stream batch received @ ${perf.firstJsonlBatch.toFixed(0)}ms`);
            }
            queue.push({ kind: "batch-start", grant: hold });
          } else {
            // 每块末尾一个 —— 300ms grace 内有新 payload / 新块都续期
            queue.push({ kind: "batch-end", grant: hold });
          }
        } else {
          // 两端契约对不上的一格：不猜，记一笔；它占过 credit，照还。
          console.warn("[events] 会话流里一格读不懂，跳过：", it.body.slice(0, 200));
          hold.owed += 1;
          owedHolds.add(hold);
        }
      } else if (it.t === "gap") {
        console.warn(`[events] 会话流 [${origin}] 丢了第 ${it.fromSeq}..${it.toSeq} 格（前端落后了）—— 按行号补`);
        queue.push({ kind: "gap", origin });
      } else if (it.t === "unseen") {
        console.info(`[events] 会话流 [${origin}]：那台机器现在看不见（${it.why}）`);
      } else if (it.t === "seen") {
        console.info(`[events] 会话流 [${origin}]：又看得见了`);
      } else {
        console.warn(`[events] 会话流 [${origin}] 关了：`, it.by);
        // 〔W5-UI · E §3.3〕这条流是这台机器会话更新的唯一来源；关了之后什么都不会再来 ⇒ 必须让人知道
        //   （原先只打 console：界面照旧，看起来只是「没动静」）。句柄只在拒绝 / 出错时关，正常收尾不走这里。
        showActionFailureToast(
          copyText("events.stream.closedTitle"),
          isLocalOrigin(origin)
            ? copyText("events.stream.closedLocal")
            : copyText("events.stream.closedRemote", { machine: origin }),
        );
      }
    }
    ensureScheduled();
  };

  // session-ended 必须进 queue 与行事件同序处理（issue #20）：之前同步派发，会
  // 抢在积压的 replay 行之前执行 —— 归档刚落实，后续 drain 的远端行就命中
  // tabs.ts ensureTab 的远端 un-archive（archived + origin!==null 见行即复活），
  // 重载对账补发的归档被原样吃掉 → 僵尸 live Tab。入队后前端处理顺序 = 后端
  // emit 顺序（重放块全部在前、补发 ended 在后；实时 ended 也天然晚于该会话的行：
  // backend 协议 removed 帧在行帧之后）。tabs.ts 的 pendingArchive 保留为防御层
  //（§ 17a 双层防御：万一 ended 仍早于建 Tab 的行，建 Tab 时落实归档）。
  registrations.push(
    sub<SessionEndedPayload>("session-ended", (e) => {
      queue.push({ kind: "ended", sessionId: e.payload.session_id });
      ensureScheduled();
    }),
  );

  // audit-fixes F03.2：session-idle 同进 queue，与 ended/行保序（灰灯落在会话末行之后、
  // 复活 remote-added 之前）。emitter 对同一 sid 只发 idle 或 ended 之一，故二者不冲突。
  registrations.push(
    sub<SessionIdlePayload>("session-idle", (e) => {
      queue.push({ kind: "idle", sessionId: e.payload.session_id });
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
    // C02：这里原先是**内联字面量类型**——最危险的一种手写形态（没有名字，
    // 漂移时没有任何东西会红，人在 review 里也很难看见）。换成生成物。
    sub<RemoteSessionAddedPayload>("remote-session-added", (e) => {
      queue.push({
        kind: "remote-added",
        sessionId: e.payload.session_id,
        origin: e.payload.origin,
        sessionKind: e.payload.kind ?? null,
        attachable: e.payload.attachable ?? null,
        cwd: e.payload.cwd ?? null,
        name: e.payload.name ?? null,
      });
      ensureScheduled();
    }),
  );

  // 〔U4b · 第四波〕活会话的容器 ＋ 某台清单报完了：同进 queue（与宣告 / 行保序）。
  registrations.push(
    sub<SessionContainerPayload>("session-container", (e) => {
      queue.push({
        kind: "container",
        sessionId: e.payload.session_id,
        container: e.payload.container,
      });
      ensureScheduled();
    }),
  );
  registrations.push(
    sub<OriginSessionsListedPayload>("origin-sessions-listed", (e) => {
      queue.push({ kind: "listed", origin: e.payload.origin });
      ensureScheduled();
    }),
  );
  // 〔GP1 · 第四波〕那台机器看不见了 ⇒ 说不清。同进 queue（与行 / 宣告 / 清单保序）。
  registrations.push(
    sub<SessionUnseenPayload>("session-unseen", (e) => {
      queue.push({ kind: "unseen", sessionId: e.payload.session_id });
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

  // 〔TAP · V124〕`session-tap`：一格 = 一个 tap 事件（`SessionTapPayload`），当场交活卡、当场还 credit；
  //   `Gap` 不补（位置号 `n` 在活卡那一侧看得出缺口）；`Unseen` / `Closed` ⇒ 那台的活卡全撤（开着的响应不会再有下文）。
  const onTapItems = (origin: Origin, hold: StreamHold, items: Item[]): void => {
    let used = 0;
    for (const it of items) {
      if (it.t === "frame") {
        used += 1;
        let p: SessionTapPayload | null = null;
        try {
          p = JSON.parse(it.body) as SessionTapPayload;
        } catch {
          p = null;
        }
        if (p !== null && typeof p === "object" && typeof p.stream === "string") {
          handlers.onSessionTap?.(p);
        } else {
          console.warn("[events] tap 流里一格读不懂，跳过：", it.body.slice(0, 200));
        }
      } else if (it.t === "gap") {
        console.info(`[events] tap 流 [${origin}] 丢了第 ${it.fromSeq}..${it.toSeq} 格（前端落后了）—— 活卡按位置号自己撤`);
      } else if (it.t === "unseen") {
        handlers.onSessionTapLost?.(origin);
      } else if (it.t === "closed") {
        console.warn(`[events] tap 流 [${origin}] 关了：`, it.by);
        handlers.onSessionTapLost?.(origin);
      }
    }
    if (used > 0) hold.sub?.want(used);
  };

  // 〔DL1〕`accounts-changed`：一批格 ⇒ 要不要刷（`accountsChangedItems` 答）；`frame` 占的 credit 当场还
  //   （格可能先于 `subscribe` 的返回到达 ⇒ 那时欠着，下一批一起还）。
  const onAccountsItems = (_origin: Origin, hold: StreamHold, items: Item[]): void => {
    const { changed, frames } = accountsChangedItems(items);
    if (frames > 0) {
      if (hold.sub) {
        hold.sub.want(frames + hold.owed);
        hold.owed = 0;
      } else {
        hold.owed += frames;
      }
    }
    if (changed) handlers.onAccountsChanged?.();
  };

  // 〔CF2 · 第四波 4B〕会话流：起停那几个事件的监听都在了之后再订（订阅一登记，句柄就可能开始交格）。
  //   返回时 monitor 那一侧已经登记好 ⇒ 主界面接着发 `frontend-ready`（就绪点）不会落空。
  // 〔TAP〕`session-tap` 与会话行走**同一处** `chan.subscribe`（前端对通信层入口的调用点各恰好一处，`X6`）：
  //   两种流只差窗口与这一格怎么交。
  const plan: { origin: Origin; kind: string; window: number; feed: typeof onStreamItems }[] = [
    ...(opts.streams ?? []).map(({ origin, kind }) => ({ origin, kind, window: STREAM_WINDOW, feed: onStreamItems })),
    ...(opts.taps ?? []).map((origin) => ({ origin, kind: "session-tap", window: TAP_WINDOW, feed: onTapItems })),
    ...(opts.accounts ?? []).map((origin) => ({
      origin,
      kind: ACCOUNTS_CHANGED_KIND,
      window: ACCOUNTS_CHANGED_WINDOW,
      feed: onAccountsItems,
    })),
  ];
  await Promise.all(
    plan.map(async ({ origin, kind, window, feed }) => {
      const hold: StreamHold = { sub: null, owed: 0 };
      hold.sub = await chan.subscribe(origin, kind, null, window, (items) => feed(origin, hold, items));
    }),
  );
}

/**
 * 「这个东西可以 `.catch`」—— 下面那一跳唯一依赖的那条前提，抽出来给它一个名字。
 *
 * ⚠ 判的是 **`.catch`**，不是 `.then`。两者在「thenable」这个词下常被当成一件事，
 * 而调用点真正要的是前者 —— 一个只有 `.then` 的 thenable 照样会在 `.catch(` 那一行炸。
 * **判据要贴着调用点写，不贴着行话写。**
 */
function isCatchable(v: unknown): v is Promise<unknown> {
  return typeof (v as { catch?: unknown } | null | undefined)?.catch === "function";
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

  // D 审计 S-2:多窗口(viewer/tear-off)各自触发落盘,带 label 防取证混淆
  const winLabel = getCurrentWebviewWindow().label;
  const lines = [
    `[perf] === 启动管线 timeline (前端 performance.now ms · window=${winLabel}) ===`,
    `[perf]   DOMContentLoaded       T+${dom.toFixed(0)}`,
    `[perf]   loadTheme done         T+${theme.toFixed(0)} (+${(theme - dom).toFixed(0)})`,
    `[perf]   emit frontend-ready    T+${ready.toFixed(0)} (+${(ready - theme).toFixed(0)})`,
    `[perf]   first jsonl-batch in   T+${firstBatch.toFixed(0)} (+${(firstBatch - ready).toFixed(0)}) ← 后端 replay 完成`,
    `[perf]   first payload dispatch T+${firstDrain.toFixed(0)} (+${(firstDrain - firstBatch).toFixed(0)})`,
    `[perf]   batch payloads drained T+${drainEnd.toFixed(0)} (+${(drainEnd - firstDrain).toFixed(0)}) ← drain 全部 ${payloadCount} 条`,
    `[perf]   onBatchEnd fired       T+${endFired.toFixed(0)} (+${(endFired - drainEnd).toFixed(0)}) ← 300ms grace 后真正切 live`,
    // Batch13-F40 仪表:建卡 vs 收纳对照(F40a 前 deferred 恒 0 = 基线口径)。
    // 计数为**页面生命周期累计**(二次 batch 不归零;批后 rIC 物化不在本行内)
    `[perf]   建卡 rendered=${p.recordsRendered ?? 0} · 收纳 deferred=${p.recordsDeferred ?? 0} · drained=${payloadCount}(累计)`,
    `[perf]   ── 总耗时（DOM → 加载完成）：${(endFired - dom).toFixed(0)}ms ──`,
  ];
  console.info(lines.join("\n"));
  // Batch13-F40:无 devtools 环境的唯一取证通道——经后端写进 monitor 日志(grep fe_perf)
  //
  // 🔴 **这一行曾把一条没人建立、也没人检查的前提当成事实**〔K-W1C 09-04〕：
  // 它直接对返回值调 `.catch`，而那句话只在「返回的东西有 `.catch`」时成立。
  // 前提破了的时候（桩返回 `undefined`）抛的是
  // `TypeError: Cannot read properties of undefined (reading 'catch')`，
  // 而它发生在 300ms grace 那个 `setTimeout` 的回调里 ⇒ 没有任何调用栈接得住
  // ⇒ vitest 记成 **unhandled error：整格红，而报文一个判据名都点不出来**。
  //
  // 这里补的是**检查**那一半（建立那一半在 `events-burst.vitest.ts` 的收尾里）：
  // 前提不成立就**出声**，不炸。
  // ⚠ **生产行为一个字节没变**：真 tauri 命令返回 Promise ⇒ 走的仍是原来那一支，
  // 逐字同样是 `void …catch(() => {})`。新增的只有「前提破了」那条岔路。
  const sent: unknown = commands.frontend_perf_log({ lines: lines.join("\n") });
  if (isCatchable(sent)) {
    void sent.catch(() => {});
  } else {
    console.warn(
      `[perf] frontend_perf_log 的返回值没有 .catch（拿到 ${typeof sent}）——` +
        "本次 perf timeline 没能经后端落盘。\n" +
        "★ 在测试里看到这一行 = 那个桩返回的不是 Promise；" +
        "它此前的表现是一条点不出判据名的 unhandled error。",
    );
  }
}
