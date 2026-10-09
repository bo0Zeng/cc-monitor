import { makeYieldToMain } from "./yield-to-main";
import { commands } from "./ipc/commands";
import { chan, type HopFault, type HopTag, type Item, type Sub } from "../../comms/inward/chan";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { toast } from "./kit/toast";
import { ACCOUNTS_CHANGED_KIND, ACCOUNTS_CHANGED_WINDOW, accountsChangedItems } from "./session-accounts-poll";
import { SESSION_TASKS_KIND, SESSION_TASKS_WINDOW, tasksChangedItems } from "./tasks-stream";
import { QUOTA_CHANGED_KIND, QUOTA_CHANGED_WINDOW, quotaChangedItems, type PlanMoved } from "./quota-stream";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
// 这几个 payload 类型从生成物 re-export（源：`src/frontend/shell/src/ui_contract.rs` 的 ts-rs 派生）。
// 仍然 `export`：不缩小已经导出的表面（要用的话从这个枢纽拿）。
import type { JsonlLinePayload } from "./generated/JsonlLinePayload";
import type { SessionStreamFrame } from "./generated/SessionStreamFrame";
import type { SessionEndedPayload } from "./generated/SessionEndedPayload";
import type { SessionIdlePayload } from "./generated/SessionIdlePayload";
import type { SessionActivityPayload } from "./generated/SessionActivityPayload";
import type { SessionTapPayload } from "./generated/SessionTapPayload";
import type { SessionRunsPayload } from "./generated/SessionRunsPayload";
import { decodeRunsPayload } from "./runs";
import type { SessionContainer } from "./generated/SessionContainer";
// 本文件内部也用这些名字（8 处），所以 import + re-export 都要有：
// 只写 `export type { … } from` 不会把名字带进本地作用域。
export type {
  JsonlLinePayload,
  SessionEndedPayload,
  SessionIdlePayload,
  SessionActivityPayload,
};


export interface EventHandlers {
  /** 一行记录。payload 带 seq，RecordTimeline 按 seq 排进 DOM，先到后到不影响画面。 */
  onLine: (e: JsonlLinePayload) => void;
  onSessionEnded: (sessionId: string) => void;
  /**
   * 远端 claude 退出但 tmux 会话仍在 → 灰灯（idle-tmux），不归档。后端在会话没了而 `@ccm_sid` 还在时发 `idle` 格（与 ended 格对同一 sid 二选一）。
   * 与 ended 格同进 queue：要相对该会话的行与之后的 remote-added 保序。前端 tabs.markTmuxIdle 置灰点。
   */
  onSessionIdle?: (sessionId: string) => void;
  /**
   * 会话（重新）变活。后端在 sessions/<PID>.json 新增且 PID 探活通过时发 —— 崩溃 → Tab 灰显 → `/resume` 后回 live，无需 F5。
   * 与 ended 格同进 queue（保持结束 / 复活的先后）。
   * 前端复活已归档的本地 Tab（tabs.reviveTab）。
   */
  onSessionStarted?: (
    sessionId: string,
    meta: {
      /** pidfile 记的起会话目录（认「我刚起的那条」用）。 */
      cwd: string | null;
      /** 会话的项目目录（后端给的那一格；标题只认它）。 */
      projectDir: string | null;
      background: boolean;
      name: string | null;
    },
  ) => void;
  /** 远端会话宣告 → 建骨架 Tab（不等首行），附 pidfile 元信息。 */
  onRemoteSessionAdded?: (
    sessionId: string,
    origin: string,
    meta: {
      background: boolean;
      /** `null` = 没说 = 视为可以 attach。 */
      attachable: boolean | null;
      /** pidfile 记的起会话目录（认「我刚起的那条」用）。 */
      cwd: string | null;
      /** 会话的项目目录（后端给的那一格；标题只认它）。 */
      projectDir: string | null;
      name: string | null;
    },
  ) => void;
  /**
   * 活会话住在什么容器里（`container` 格，本机与远端同一个事件）。
   * 进 queue：与 `remote-added` / 行保序（先建 tab、再落容器；早到的由 TabManager 暂存）。
   */
  onSessionContainer?: (sessionId: string, container: SessionContainer) => void;
  /**
   * 一个会话的运行表（会话流里的 `runs` 格，那台后端的成品）：列在 agent 面板里，状态标到派出它的那张工具卡上（不进主 tab 的消息流）。
   * 进 queue：排在那个会话的宣告之后（处理它时 tab 已在）。
   */
  onSessionRuns?: (p: SessionRunsPayload) => void;
  /**
   * 某台机器的活会话清单报完了（`listed` 格）。进 queue：排在那台的
   * `remote-added` 之后 ⇒ 处理它时，那台此刻全部的活会话都已宣告过。
   * `all` = 壳说这一刻机器表里每一台都报完了（「各台都报完」那一拍，壳那一侧 `session_book` 按机器表算）。
   */
  onOriginSessionsListed?: (origin: string, all: boolean) => void;
  /**
   * 中转抄出来的一个 SSE 事件：会话流 `session-tap`（通道 `subscribe`）里的一格。
   * **不进 queue**：活卡是临时态，与行 / 起停事件之间不需要顺序（jsonl 那一轮到了整轮覆盖、墓碑挡迟到的 tap）；
   * 进 queue 反倒会让 token 级的洪峰排在行前面。credit 在本格处理完当场还。
   */
  onSessionTap?: (e: SessionTapPayload) => void;
  /**
   * 那台机器的 tap 流看不见了（订阅里的 `Unseen`：本机后端那条流断了）⇒ 那台上还开着的响应不会再有下文，活卡全撤。
   */
  onSessionTapLost?: (origin: Origin) => void;
  /**
   * 某台机器的账号清单**可能**变了（`accounts-changed` 流里的 `seen` / `frame` / `gap`，
   * 一批只叫一次）⇒ 强制刷账号清单与 chip。替掉裸 Tauri 事件 `remote-backend-ready`。
   */
  onAccountsChanged?: () => void;
  /**
   * 活会话的记录文件不见了（`change` = `"gone"`）/ 被改过已从头重读（`"truncated"` / `"rewritten"`）。
   * 会话流里的一格（`{"file_notice": …}`），与行同序：重读出来的行排在它后面。
   */
  onSessionFileNotice?: (sessionId: string, change: string) => void;
  /**
   * 那台机器看不见了（会话流 `unseen` 格：连接断了 / F5 时那台还没报完清单）⇒ 那台上活的 · 可重连的落说不清。
   * 机器级：一格说一台。后端紧跟着把那台还活着的再宣告一次 ⇒ 真活着的翻回活。
   * 进 queue：与行 / `live` / `listed` 保序（断连那一刻之前的行先落，重连之后的重宣告与清单后到）。
   */
  onOriginUnseen?: (origin: string) => void;
  /** 一台机器一直看不见时，那条提示里「打开设置」按它开（没给 ⇒ 提示里不带按钮）。 */
  openMachineSettings?: (origin: Origin) => void;
  /**
   * 启动重放的第一块到达时调一次：TabManager 把所有 tab 的 BranchFolder 切到 batch 模式、开惰性高亮。
   * 整个重放期间只调一次（多块在 300ms grace 续期下视作连续的一批）。
   */
  onBatchStart?: () => void;
  /** 同一批的记录都处理完后调一次（多块时只在 300ms grace 真正到点时调）：TabManager 算主线 ＋ rebuild，切回 live。 */
  onBatchEnd?: () => void;
  /**
   * 那台机器上这几个会话的任务清单变了（`sids`），或者期间可能漏了（`all`：那台又接上 / 丢了几格）⇒
   * 调用方重问 `tasks-list`。来自通道 `subscribe(origin, "session-tasks")`（`tasks-panel.ts::tasksChangedItems` 读格）。
   */
  onTasksChanged?: (origin: Origin, sids: readonly string[], all: boolean) => void;
  /**
   * 某台的额度账变了（`quota`）/ 某几个会话的轮换 / 账号格变了（`sids`）/ 期间可能漏了（`all`：那台整台重问）。
   * 来自通道 `subscribe(origin, "quota-changed")`（`quota-stream.ts::quotaChangedItems` 读格）。
   */
  onQuotaChanged?: (origin: Origin, change: { quota: boolean; sids: readonly string[]; all: boolean; rules: boolean }) => void;
  /**
   * 某台的这几个 pb 工作区的计划变了（`moved`：工作区 · 新摘要 · 要你看几条）/ 期间可能漏了（`all`：那台整台重问）。
   * 与额度同一条流 `subscribe(origin, "quota-changed")`（`quota-stream.ts::quotaChangedItems` 读格）。
   */
  onPlanChanged?: (origin: Origin, change: { moved: readonly PlanMoved[]; all: boolean }) => void;
  /**
   * 会话红绿灯：后端只在 sessions/<PID>.json 的官方 status 变化时发（天然稀疏，当场派）。
   * "busy" = 运行中 / "idle"、"shell" = 等输入 / "waiting" = 等弹窗决定（waiting_for 细分原因）。
   */
  onSessionActivity?: (payload: SessionActivityPayload) => void;
  /**
   * 那台机器的会话流里**丢了几格**（没 credit 时句柄丢了、原位报的 `gap`）。
   * 丢的是哪几个会话的哪几行，流里说不出来（一条订阅里混着多个会话）⇒ 宿主对那台机器的每个 tab 按行号补
   * （`TabManager.onStreamGap`）。进 queue：与行保序（丢在哪两格之间，补就从那里起）。
   */
  onStreamGap?: (origin: Origin) => void;
}

/**
 * **会话流一开始给多少 credit**（格数；一格 = 一行或一个批边界）。
 *
 * 它就是这一侧队列的上界：句柄交出去的格不会多于给的 credit，每处理掉一格还一格（`want`，按 drain 一片批量还）。
 * 取 20 000：与一次 F5 重放的量级相当（每个会话 ≤ 750 条 × 十几二十个会话）——
 * 小了，F5 重放会一次次停下来等 credit（慢，但不丢）；大了，前端落后时的队列跟着长。
 * 落后超过一整个窗口的实时行被句柄丢掉、原位报 `gap`（`onStreamGap` 按行号补）。
 */
export const STREAM_WINDOW = 20_000;

/**
 * 〔登记的例外〕会话流里**不吃 credit、不丢**的那几种格（会话起停 / 状态的成品）。
 * 收到它们不还 credit（monitor 那一侧交它们时本来就没扣）。Rust 那一侧同一张表是 `ui_contract.rs::SessionStreamFrame::takes_credit`，
 * 两侧对金样 `tests/__fixtures__/session-stream-credit.golden.json`。
 */
export const CREDIT_EXEMPT_FRAMES = [
  "live",
  "activity",
  "container",
  "idle",
  "ended",
  "unseen",
  "listed",
  "snapshot_inflight",
  "runs",
] as const;

/**
 * `session-tap` 订阅的 credit 窗口（格）：webview 这一跳在途的 tap 最多这么多格，超了 monitor 那一侧丢、
 * 位置照占、原位 `Gap`（级 2）。每格处理完当场还 ⇒ 正常节奏下窗口永远不会见底；只有 webview 卡住（最小化、
 * 长任务）时才丢 —— 丢了由活卡的位置号 `n` 看出缺口、撤卡，jsonl 定稿。值与后端 tap 通道同一个量级（256）。
 */
export const TAP_WINDOW = 256;

/** 一条会话流订阅在本文件里的账：还没还的 credit。`sub` 在登记那一跳回来之前是 `null`。 */
interface StreamHold {
  sub: Sub | null;
  owed: number;
}

/** queue 中的不同事件类型，drain 按 kind 派发 */
type QueueItem =
  // 流里来的三种带 `grant`：处理掉它就还那条订阅一格 credit。
  | { kind: "payload"; payload: JsonlLinePayload; grant?: StreamHold }
  | { kind: "batch-start"; grant?: StreamHold }
  | { kind: "batch-end"; grant?: StreamHold }
  // 那台机器的流里丢了几格（`gap`）。
  | { kind: "gap"; origin: Origin }
  | { kind: "ended"; sessionId: string }
  // 灰灯（idle-tmux）—— 与 ended 同 queue 保序，见 onSessionIdle。
  | { kind: "idle"; sessionId: string }
  | {
      kind: "started";
      sessionId: string;
      cwd: string | null;
      projectDir: string | null;
      background: boolean;
      name: string | null;
    }
  // 远端会话宣告 —— 骨架 Tab 入口。走同一 queue 与 ended / started / 行保序（INVARIANT § 20）。
  | {
      kind: "remote-added";
      sessionId: string;
      origin: string;
      background: boolean;
      /** attach 进去对人有没有意义。`null` = 没说 = 视为可以。 */
      attachable: boolean | null;
      cwd: string | null;
      projectDir: string | null;
      name: string | null;
    }
  // 容器事实 / 某台清单报完了 —— 同一 queue 保序（见 EventHandlers 里两条的注释）。
  | { kind: "container"; sessionId: string; container: SessionContainer }
  | { kind: "runs"; payload: SessionRunsPayload }
  | { kind: "listed"; origin: string; all: boolean }
  // 那台机器看不见了 —— 同一 queue 保序（见 EventHandlers.onOriginUnseen）。
  | { kind: "unseen"; origin: string }
  // 记录文件不见了 / 被改过已从头重读 —— 流里的一格，与行同序（见 EventHandlers.onSessionFileNotice）。
  | { kind: "file-notice"; sessionId: string; change: string; grant?: StreamHold };

/**
 * drain 用的 FIFO：**数组 ＋ 头下标**，出队 O(1)。
 *
 * 不用 `shift()`：V8 的 left-trim 快路不是无条件的，数组长过一万多之后每次 `shift` 退化成搬整份，
 * 而这条队列能长到一个 credit 窗口（`STREAM_WINDOW` = 20 000）。
 *
 * - 出队把那一格置空（不留对已派发条目的引用）；排空时整份复位；头下标过 {@link COMPACT_AT} 且过半时
 *   压缩一次（摊还 O(1)，队列不会只增不减）。
 * - 队首插入（突发检测的 `batch-start` 哨兵）：头下标前有空位就放进去，没有才退回 `unshift`
 *   （一次突发只插一次）。
 */
const COMPACT_AT = 4096;
class DrainQueue<T> {
  private items: Array<T | undefined> = [];
  private head = 0;

  /** 还没出队的条数 */
  get size(): number {
    return this.items.length - this.head;
  }

  push(x: T): void {
    this.items.push(x);
  }

  /** 插到**剩余**队列的最前面 */
  pushFront(x: T): void {
    if (this.head > 0) this.items[--this.head] = x;
    else this.items.unshift(x);
  }

  take(): T | undefined {
    if (this.head >= this.items.length) return undefined;
    const x = this.items[this.head];
    this.items[this.head++] = undefined;
    if (this.head === this.items.length) {
      this.items.length = 0;
      this.head = 0;
    } else if (this.head >= COMPACT_AT && this.head * 2 >= this.items.length) {
      this.items.splice(0, this.head);
      this.head = 0;
    }
    return x;
  }
}

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

// 让开的方式：`makeYieldToMain`（`yield-to-main.ts`，与长回复分片渲染共用）。

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
 * onBatchEnd 延迟这么久才调：重放那一批之后，后端会把积压的记录逐条当实时行发出来（没有批哨兵包着），
 * 立刻切回 live 就会逐条 O(N) 算主线（启动后第二次卡顿）。延迟期间任何新 payload 都续期，稳定后一次 flushPending ＋ 切 live。
 * 300ms 远大于积压涌出来的那一波（几十 ms），又小到不影响真实的新消息（间隔 ≥ 几秒）。
 */
const BATCH_END_GRACE_MS = 300;

/** 会话流看不见了多久才说（起步那几秒常是「那台还在连」，不算）。 */
const UNSEEN_SAY_MS = 20_000;

/** 一台机器一直看不见：哪台 · 为什么（从没看见过 ⇒ 只说还没连上）· 能做什么。 */
function sayUnseen(origin: Origin, ever: boolean, why: HopFault, openSettings: (() => void) | undefined): void {
  const machine = isLocalOrigin(origin) ? copyText("control.machine.local") : origin;
  const title = !ever
    ? copyText("events.unseen.neverTitle", { machine })
    : why === "Dropped"
      ? copyText("events.unseen.droppedTitle", { machine })
      : why === "Overrun"
        ? copyText("events.unseen.overrunTitle", { machine })
        : copyText("events.unseen.unreachableTitle", { machine });
  const body = ever ? copyText("events.unseen.lostBody", { machine }) : copyText("events.unseen.neverBody", { machine });
  toast(title, body, openSettings ? { action: { label: copyText("main.cmd.openSettings"), run: openSettings } } : {});
}

/** bindEvents 选项。 */
export interface BindEventsOptions {
  // 按窗口定向投递只剩会话流的交格，由 `src/comms/inward/chan.ts` 那一处按窗口作用域听（条 22.2）。
  /**
   * 要订的会话流：`(origin, kind)`（`kind` = `session-lines` 整台机器 · `session-lines/<sid>` 一个会话）。
   * 会话内容只从这里来。在其余监听都注册完之后订，
   * `bindEvents` 返回时 monitor 那一侧已经登记好（主界面接着发 `frontend-ready` 就是它们的就绪点）。
   */
  streams?: ReadonlyArray<{ origin: Origin; kind: string }>;
  /**
   * 要订 `session-tap` 的机器（每台的中转住那台的常驻后端 ⇒ 每台都订）。
   * 与会话行同一条帧路、同一套 credit；窗口是 {@link TAP_WINDOW}。
   */
  taps?: ReadonlyArray<Origin>;
  /**
   * 要订 `accounts-changed` 的机器（那台的长连接又通了 / 那台后端说账号清单变了 ⇒ {@link EventHandlers.onAccountsChanged}）。
   * 与会话行 · tap 同一条帧路、同一处 `chan.subscribe`；窗口是 `ACCOUNTS_CHANGED_WINDOW`。
   */
  accounts?: ReadonlyArray<Origin>;
  /**
   * 要订 `session-tasks` 的机器（那台后端说某个会话的任务清单变了 ⇒ {@link EventHandlers.onTasksChanged}）。
   * 与会话行 · tap · 账号同一处 `chan.subscribe`；窗口是 `SESSION_TASKS_WINDOW`。
   */
  tasks?: ReadonlyArray<Origin>;
  /**
   * 要订 `quota-changed` 的机器（那台的额度账 / 某个会话的轮换变了 ⇒ {@link EventHandlers.onQuotaChanged}）。
   * 同一处 `chan.subscribe`；窗口是 `QUOTA_CHANGED_WINDOW`。
   */
  quota?: ReadonlyArray<Origin>;
}

/**
 * 订阅后端事件。返回的 Promise resolve 时订阅已登记完成。
 * 调用方必须 `await bindEvents` 之后再触发任何会让后端发东西的调用（frontend-ready 等）：之前发的会丢。
 */
export async function bindEvents(
  handlers: EventHandlers,
  opts: BindEventsOptions = {},
): Promise<void> {
  const queue = new DrainQueue<QueueItem>();
  let scheduled = false;

  // batch-end 延迟状态机
  let inBatchMode = false;
  // 远端快照 / 回填在途计数（会话流的 snapshot_inflight 格）。> 0 时批结束定时器只续期不触发：
  // 慢链路回填块间隔 > 300ms 时提前退出批模式，旧历史会以实时形态插进时间线中段。5 分钟上限防后端卡住永久压住 flush。
  let snapshotInflight = 0;
  let batchModeSince = 0;
  const BATCH_HOLD_MAX_MS = 5 * 60_000;
  // 突发检测：实时行积压超过该深度 → 主动进批模式（与后端 INCREMENTAL_BATCH_THRESHOLD=50 同量级）。burstArmed 防哨兵在生效前重复入队。
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
    // 调度：一次性 —— batch-end 哨兵：每次重排前清掉上一个，另有 5 分钟防呆上限
    endTimer = window.setTimeout(() => {
      endTimer = null;
      // 回填在途 → 续期（除非超 5min 上限）
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
      // 已在批模式又收到 batch-start：忽略（惰性高亮与 BranchFolder.batchMode 仍开着，无需重入）。
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

  // 处理掉的流格攒着还 credit（每片 drain 末尾还一次，不是逐格一次 IPC）。
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
          projectDir: item.projectDir,
          background: item.background,
          name: item.name,
        });
      } else if (item.kind === "remote-added") {
        handlers.onRemoteSessionAdded?.(item.sessionId, item.origin, {
          background: item.background,
          attachable: item.attachable,
          cwd: item.cwd,
          projectDir: item.projectDir,
          name: item.name,
        });
      } else if (item.kind === "container") {
        handlers.onSessionContainer?.(item.sessionId, item.container);
      } else if (item.kind === "runs") {
        handlers.onSessionRuns?.(item.payload);
      } else if (item.kind === "listed") {
        handlers.onOriginSessionsListed?.(item.origin, item.all);
      } else if (item.kind === "unseen") {
        handlers.onOriginUnseen?.(item.origin);
      } else if (item.kind === "file-notice") {
        handlers.onSessionFileNotice?.(item.sessionId, item.change);
      } else if (item.kind === "gap") {
        handlers.onStreamGap?.(item.origin);
      }
    } catch (e) {
      // 单条记录处理出错不能冻死整条队列（异常逃出 drain ⇒ 后面上千条永远不渲染）：记日志继续。
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
    while (queue.size > 0) {
      const item = queue.take();
      if (item) dispatchItem(item);
      processed += 1;
      const elapsed = performance.now() - start;
      if (askInputPending) {
        // ★ 步 4②：**能问就别猜。** 8ms 之前连问都不问（让开本身要花一次宏任务跳），
        // 之后一路干到「真有输入事件在排队」或撞上硬上限。
        if (elapsed >= BATCH_MS && (askInputPending() || elapsed >= BATCH_MS_MAX)) break;
      } else if (processed >= BATCH_SIZE || elapsed >= BATCH_MS) {
        // 问不了（WebKitGTK 没有 `isInputPending`）⇒ 用固定预算。
        break;
      }
    }
    flushGrants();
    if (queue.size > 0) {
      // 让出主线程一跳再处理下一批（`MessageChannel`，探不到才退回 `setTimeout`）
      yieldToDrain();
    } else {
      scheduled = false;
      // 突发检测进入的批模式没有 batch-end 哨兵：队列清空且没有排着的退出 timer 时补排一个（grace 期内新 payload 照常续期）。
      if (inBatchMode && endTimer === null) {
        scheduleBatchEnd();
      }
    }
  };


  const ensureScheduled = (): void => {
    if (scheduled || queue.size === 0) return;
    scheduled = true;
    // 用宏任务而非 queueMicrotask，确保批与批之间真正让出
    // （microtask 同一 tick 内连续清空，无让出效果）。
    //
    // 这一跳用 `setTimeout` 不用 `MessageChannel`：4ms 钳制只打嵌套层级 > 5 的 timer，从事件回调里排的这一跳不被钳
    // （被钳的 `drain → drain` 自链走 `yieldToDrain`）；而且 `setTimeout` 在假定时器下是决定性的，`MessagePort` 不归它管。
    // 调度：自链 —— 队列 drain：队列空即停，`scheduled` 防重入
    setTimeout(drain, 0);
  };



  // 会话内容走通道的 `subscribe`（本函数末尾按 `opts.streams` 订）。一格 = 一行（`{"line": …}`）或成批那一段的边界（`{"batch": …}`）；
  //   进 queue：行 ⇒ payload，批边界 ⇒ batch-start / batch-end 哨兵。
  // 一台机器的会话流看不见了：起步那几秒（那台还在连）不吵；过了 `UNSEEN_SAY_MS` 还没看见 ⇒ 说是哪台、能做什么。
  //   从没看见过 ⇒「还没连上」（不猜原因）；看见过之后断了 ⇒ 照流里那一跳的原因说。
  //   断在「读」那一跳 ＝ 订的时候它看得见（那时流里不先来一格「看不见」），也算看见过。
  const sight = new Map<Origin, { ever: boolean; timer: ReturnType<typeof setTimeout> | null }>();
  const sightOf = (origin: Origin): { ever: boolean; timer: ReturnType<typeof setTimeout> | null } => {
    let s = sight.get(origin);
    if (!s) sight.set(origin, (s = { ever: false, timer: null }));
    return s;
  };
  const onUnseen = (origin: Origin, at: HopTag, why: HopFault): void => {
    const s = sightOf(origin);
    if (at !== "open") s.ever = true;
    if (s.timer !== null) return;
    const ever = s.ever;
    // 调度：一次性 —— 一台机器看不见满 20s 才说一句；又看见了当场清
    s.timer = setTimeout(() => {
      s.timer = null;
      sayUnseen(origin, ever, why, handlers.openMachineSettings ? () => handlers.openMachineSettings?.(origin) : undefined);
    }, UNSEEN_SAY_MS);
  };
  const onSeenAgain = (origin: Origin): void => {
    const s = sightOf(origin);
    s.ever = true;
    if (s.timer !== null) clearTimeout(s.timer);
    s.timer = null;
  };

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
          // 突发检测：逐行来的实时格没有批哨兵包着，任何突发源积压到阈值就主动进批模式 —— 哨兵插到队首，剩下的积压走惰性路径。
          // 已在批模式或哨兵已入队则不重复；退出走 drain 清空后的 grace 补排。
          if (!inBatchMode && !burstArmed && queue.size > BURST_ENTER_THRESHOLD) {
            burstArmed = true;
            queue.pushFront({ kind: "batch-start" });
          }
        } else if (f !== null && typeof f === "object" && "file_notice" in f) {
          queue.push({
            kind: "file-notice",
            sessionId: f.file_notice.session_id,
            change: f.file_notice.change,
            grant: hold,
          });
        } else if (f !== null && typeof f === "object" && "live" in f) {
          // 会话起停 / 状态的成品：与行同一条流、同序；不吃 credit（不带 grant）。本机远端同一形，只差建 tab 那一跳。
          const p = f.live;
          if (isLocalOrigin(p.origin)) {
            queue.push({
              kind: "started",
              sessionId: p.session_id,
              cwd: p.cwd ?? null,
              projectDir: p.project_dir ?? null,
              background: p.background,
              name: p.name ?? null,
            });
          } else {
            queue.push({
              kind: "remote-added",
              sessionId: p.session_id,
              origin: p.origin,
              background: p.background,
              attachable: p.attachable ?? null,
              cwd: p.cwd ?? null,
              projectDir: p.project_dir ?? null,
              name: p.name ?? null,
            });
          }
        } else if (f !== null && typeof f === "object" && "activity" in f) {
          // 红绿灯稀疏，当场派（不进 queue）。
          handlers.onSessionActivity?.(f.activity);
        } else if (f !== null && typeof f === "object" && "container" in f) {
          queue.push({ kind: "container", sessionId: f.container.session_id, container: f.container.container });
        } else if (f !== null && typeof f === "object" && "runs" in f) {
          const runs = decodeRunsPayload(f.runs);
          if (runs) queue.push({ kind: "runs", payload: runs });
          else console.warn("[events] 运行表那一格形状不对，不收：", JSON.stringify(f.runs).slice(0, 200));
        } else if (f !== null && typeof f === "object" && "idle" in f) {
          queue.push({ kind: "idle", sessionId: f.idle.session_id });
        } else if (f !== null && typeof f === "object" && "ended" in f) {
          queue.push({ kind: "ended", sessionId: f.ended.session_id });
        } else if (f !== null && typeof f === "object" && "unseen" in f) {
          queue.push({ kind: "unseen", origin: f.unseen.origin });
        } else if (f !== null && typeof f === "object" && "listed" in f) {
          queue.push({ kind: "listed", origin: f.listed.origin, all: f.listed.all === true });
        } else if (f !== null && typeof f === "object" && "snapshot_inflight" in f) {
          // 快照在途电平 —— 纯批调度信号，不进 queue。
          snapshotInflight = f.snapshot_inflight.count;
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
        console.warn(
          it.toSeq === null
            ? `[events] 会话流 [${origin}] 在第 ${it.fromSeq} 格处丢了一行（超长、说不出是哪一行）—— 按行号补`
            : `[events] 会话流 [${origin}] 丢了第 ${it.fromSeq}..${it.toSeq} 格（前端落后了）—— 按行号补`,
        );
        queue.push({ kind: "gap", origin });
      } else if (it.t === "unseen") {
        console.info(`[events] 会话流 [${origin}]：那台机器现在看不见（${it.why}）`);
        onUnseen(origin, it.at.tag, it.why);
      } else if (it.t === "seen") {
        console.info(`[events] 会话流 [${origin}]：又看得见了`);
        onSeenAgain(origin);
      } else {
        console.warn(`[events] 会话流 [${origin}] 关了：`, it.by);
        onSeenAgain(origin); // 关了另有一句（下面），不说「看不见」
        // 这条流是这台机器会话更新的唯一来源：关了之后什么都不会再来 ⇒ 必须让人知道（光打 console 看起来只是「没动静」）。
        //   句柄只在拒绝 / 出错时关，正常收尾不走这里。
        toast(
          copyText("events.stream.closedTitle"),
          isLocalOrigin(origin)
            ? copyText("events.stream.closedLocal")
            : copyText("events.stream.closedRemote", { machine: origin }),
        );
      }
    }
    ensureScheduled();
  };

  // 会话起停 / 状态都是会话流里的格（上面 `onStreamItems`）：与行同一条流 ⇒「ended 必须与行同序」由构造保证。

  // 任务变更走通道 `session-tasks`（见下面 `plan` 里那一种流）。



  // `session-tap`：一格 = 一个 tap 事件（`SessionTapPayload`），当场交活卡、当场还 credit；
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

  // `accounts-changed`：一批格 ⇒ 要不要刷（`accountsChangedItems` 答）；`frame` 占的 credit 当场还
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

  // `session-tasks`：一批格 ⇒ 哪几个会话要重问（`tasksChangedItems` 答）；`frame` 占的 credit 当场还（订阅返回之前到的欠着）。
  const onTasksItems = (origin: Origin, hold: StreamHold, items: Item[]): void => {
    const { sids, all, frames } = tasksChangedItems(items);
    if (frames > 0) {
      if (hold.sub) {
        hold.sub.want(frames + hold.owed);
        hold.owed = 0;
      } else {
        hold.owed += frames;
      }
    }
    if (all || sids.length > 0) handlers.onTasksChanged?.(origin, sids, all);
  };

  // `quota-changed`：一批格 ⇒ 额度账 / 哪几个会话的轮换要重问（`quotaChangedItems` 答）；credit 同 `session-tasks`。
  const onQuotaItems = (origin: Origin, hold: StreamHold, items: Item[]): void => {
    const { quota, sids, all, rules, plans, frames } = quotaChangedItems(items);
    if (frames > 0) {
      if (hold.sub) {
        hold.sub.want(frames + hold.owed);
        hold.owed = 0;
      } else {
        hold.owed += frames;
      }
    }
    if (quota || all || rules || sids.length > 0) handlers.onQuotaChanged?.(origin, { quota, sids, all, rules });
    if (all || plans.length > 0) handlers.onPlanChanged?.(origin, { moved: plans, all });
  };

  // 会话流：起停那几个事件的监听都在了之后再订（订阅一登记，句柄就可能开始交格）。
  //   返回时 monitor 那一侧已经登记好 ⇒ 主界面接着发 `frontend-ready`（就绪点）不会落空。
  // `session-tap` 与会话行走同一处 `chan.subscribe`（前端对通信层入口的调用点各恰好一处）：
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
    ...(opts.tasks ?? []).map((origin) => ({
      origin,
      kind: SESSION_TASKS_KIND,
      window: SESSION_TASKS_WINDOW,
      feed: onTasksItems,
    })),
    ...(opts.quota ?? []).map((origin) => ({
      origin,
      kind: QUOTA_CHANGED_KIND,
      window: QUOTA_CHANGED_WINDOW,
      feed: onQuotaItems,
    })),
  ];
  await Promise.all(
    plan.map(async ({ origin, kind, window, feed }) => {
      const hold: StreamHold = { sub: null, owed: 0 };
      hold.sub = await openStream(origin, kind, window, (items) => feed(origin, hold, items));
    }),
  );
}

/** 订一条流（前端对通信层 `subscribe` 的唯一调用点：整台机器的那几条与单个会话那一条都经这里）。 */
function openStream(origin: Origin, kind: string, window: number, sink: (items: Item[]) => void): Promise<Sub> {
  return chan.subscribe(origin, kind, null, window, sink);
}

/** 跟着一个会话（查看器 · 独立查看窗）：流里那一个会话的事。 */
export type FollowEvent =
  /** 新的记录行（含订阅当场交的留存；已经有的由调用方按 `seq` 去重）。 */
  | { t: "lines"; lines: JsonlLinePayload[] }
  /** 流里丢了几行（前端落后了）：调用方按行号补。 */
  | { t: "gap" }
  /** 会话起了 / 结束了。 */
  | { t: "live"; live: boolean }
  /** 那台看不看得见（看不见 ⇒ 这条流此刻不在交东西）。 */
  | { t: "sight"; seen: boolean }
  /** 这个会话的运行表（每次变都是整份）。 */
  | { t: "runs"; payload: SessionRunsPayload };

/**
 * **跟着一个会话**：订 `session-lines/<sid>`（与独立查看窗同一条订阅；留存订阅当场交、之后的实时行接着交），
 * 每一批格翻成 [`FollowEvent`] 交给 `sink`；吃 credit 的格交完当场还。返回时 monitor 那一侧已登记好。
 */
export async function followSession(origin: Origin, sid: string, sink: (e: FollowEvent) => void): Promise<{ stop(): void }> {
  const hold: StreamHold = { sub: null, owed: 0 };
  const exempt: readonly string[] = CREDIT_EXEMPT_FRAMES;
  const feed = (items: Item[]): void => {
    const lines: JsonlLinePayload[] = [];
    let used = 0;
    const out: FollowEvent[] = [];
    for (const it of items) {
      if (it.t === "frame") {
        let f: Record<string, unknown> | null = null;
        try {
          const v: unknown = JSON.parse(it.body);
          f = v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
        } catch {
          f = null;
        }
        const kind = f ? Object.keys(f)[0] : undefined;
        if (kind === undefined || !exempt.includes(kind)) used += 1;
        if (!f) continue;
        const line = f.line as JsonlLinePayload | undefined;
        if (line && line.session_id === sid) lines.push(line);
        else if ("ended" in f && (f.ended as { session_id?: string }).session_id === sid) out.push({ t: "live", live: false });
        else if ("live" in f && (f.live as { session_id?: string }).session_id === sid) out.push({ t: "live", live: true });
        else if ("unseen" in f) out.push({ t: "sight", seen: false });
        else if ("runs" in f) {
          const p = decodeRunsPayload(f.runs);
          if (p === null) console.warn("[events] 运行表那一格形状不对，不收：", JSON.stringify(f.runs).slice(0, 200));
          else if (p.session_id === sid) out.push({ t: "runs", payload: p });
        }
      } else if (it.t === "gap") {
        out.push({ t: "gap" });
      } else if (it.t === "unseen" || it.t === "closed") {
        out.push({ t: "sight", seen: false });
      } else if (it.t === "seen") {
        out.push({ t: "sight", seen: true });
      }
    }
    // 吃 credit 的格当场还（订阅返回之前到的欠着，返回时一起还）。
    if (used > 0) {
      if (hold.sub) {
        hold.sub.want(used + hold.owed);
        hold.owed = 0;
      } else {
        hold.owed += used;
      }
    }
    if (lines.length > 0) sink({ t: "lines", lines });
    for (const e of out) sink(e);
  };
  hold.sub = await openStream(origin, `session-lines/${sid}`, STREAM_WINDOW, feed);
  if (hold.owed > 0) {
    hold.sub.want(hold.owed);
    hold.owed = 0;
  }
  return { stop: () => hold.sub?.stop() };
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

  // 多窗口（查看器 / 拆出的窗口）各自落盘，带 label 免得读数混在一起
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
    // 建卡 vs 收纳对照；计数为页面生命周期累计（二次批不归零；批后 rIC 物化不在本行内）
    `[perf]   建卡 rendered=${p.recordsRendered ?? 0} · 收纳 deferred=${p.recordsDeferred ?? 0} · drained=${payloadCount}(累计)`,
    `[perf]   ── 总耗时（DOM → 加载完成）：${(endFired - dom).toFixed(0)}ms ──`,
  ];
  console.info(lines.join("\n"));
  // 没有 devtools 的环境里的读数通道：经后端写进 monitor 日志（grep fe_perf）。
  // 返回的不是 Promise（桩返回 undefined）⇒ 出声不炸：在 grace 那个 `setTimeout` 回调里抛，没有调用栈接得住。
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
