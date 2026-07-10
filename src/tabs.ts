import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { MessageStream } from "./stream";
import {
  reconcilePendingToolResults,
  type RenderContext,
} from "./cards";
import { BranchFolder } from "./branch-fold";
import { fetchSessionTasks, type TaskEntry, type TasksPanel } from "./tasks-panel";
import type { JsonlLinePayload } from "./events";
import type { BehaviorConfig } from "./behavior";
import { showActionFailureToast } from "./error-toast";
import { RecordTimeline } from "./record-timeline";
import { TailWindow } from "./live-window";
import {
  renderContentRecord,
  routeMetaAndBranch,
  type StreamSink,
} from "./render-stream-record";
import type { BranchRecord } from "./branching";
import { isAgentTool } from "./cards/subagent";
import type { AgentsPanel, AgentEntry } from "./agents-panel";
import { LS_KEYS, safeSet } from "./local-storage";
import { runRemoteResume, runRemoteResumeTmux, runRemoteAttach } from "./remote-launch-run";
import { turnEndNotifier } from "./turn-notify";
import { getBehavior } from "./behavior";

/**
 * Tab 生命周期：
 * - `live`：session 进程还在跑（`~/.claude/sessions/<PID>.json` 存在且 PID 探活通过）
 * - `archived`：session 进程退出，Tab 灰显但保留内容；用户可主动关
 *
 * 历史：设计文档原本规划过 `idle`（5min 无消息变灰），但实际未落地，
 * 移除以免误用。
 */
export type TabStatus = "live" | "archived";

export interface Tab {
  sessionId: string;
  /** Batch7-F24：会话类型（"interactive"/"bg"/null=未知视为交互）。bg → ⚙ 标题 + 树状挂宿主后。 */
  kind: string | null;
  /** Batch7-F24：bg 任务名（pidfile name 字段）；bg 标题优先用它。 */
  bgName: string | null;
  /**
   * Tab 标题。优先级：[项目] aiTitle > 项目名 > session_id 前 8 位。
   * aiTitle 一旦出现就锁住，后续 cwd 不再回退。
   */
  title: string;
  cwd: string | null;
  /**
   * `cwd` 来源记录的 seq。取**最小 seq（最早记录）**的 cwd = 项目根 / 启动目录。
   * 会话的 cwd 可能中途漂移到子目录；用最早记录的 cwd 才稳定指向项目根（与历史
   * 浏览器 quick_extract_cwd 口径一致）。Infinity = 尚未拿到任何带 cwd 的记录。
   */
  cwdSeq: number;
  /** Claude 给出的语义标题（JSONL 里 `ai-title` 记录的 aiTitle 字段），出现一次就锁定 */
  aiTitle: string | null;
  /**
   * issue #15：数据来源主机标签。null = 本地（标题无前缀）；非空（如 "raspberrypi.local"）
   * = 远端 SSH 主机名，标题加 `[origin]` 前缀以区分本地/远端。首条 line 帧的 origin
   * 决定，之后不变（同一 sid 只来自一个来源）。
   */
  origin: string | null;
  status: TabStatus;
  /**
   * issue #23：红绿灯（与 TabStatus 正交，不碰 archived 门控）。null=未知（旧版 CC
   * 无 status 字段 / 远端 v1 暂无透传）→ 维持现状绿点。
   */
  activity: { status: string; waitingFor: string | null } | null;
  /**
   * issue #23（第二增量）：本会话的 subagent 列表（tool_use id → entry，插入序）。
   * jsonl 流里配对 Task/Agent 的 tool_use（running）与 tool_result（done）；
   * 变 idle/归档时把仍 running 的标 aborted。上限 30，超出删最老的非 running。
   */
  agents: Map<string, AgentEntry>;
  streamEl: HTMLElement;
  stream: MessageStream;
  /** 父 JSONL 路径（subagent 加载需要） */
  parentPath: string;
  unread: number;
  /**
   * P5.2 B 重构：按 seq 排序的 timeline。renderStreamRecord 调
   * `timeline.insert / peekPrev` 决定 DOM 挂载位置 + tool-group 合并。
   * **取代**：原 pendingToolGroup（tool-group 合并改后处理，看 timeline 左邻居）
   * 和 pendingPrependFragment（无 source/inPrependMode 概念，永远按 seq 插入）。
   */
  timeline: RecordTimeline;
  /**
   * tool_use_id → tool_name 缓存。tool_use 在 assistant 消息出现时记下，
   * 下一条 user 消息的 tool_result 反查显示工具名。
   */
  toolUseNames: Map<string, string>;
  /**
   * tool_use_id → tool_use 折叠条 DOM。tool_result 直接注入对应 tool_use
   * 内部，不再产生独立折叠条。详见 cards/index.ts 的 injectOrBuildToolResult。
   */
  toolUseElements: Map<string, HTMLElement>;
  /**
   * issue #8: ESC 回退分支折叠管理器。
   * 跟踪本 Tab 内所有有 uuid 的卡片，监听 parentUuid 分叉，把"被回退"的连续段
   * 包到可折叠容器里。工具组（tool-group）不参与折叠（无单一 uuid）。
   */
  branchFolder: BranchFolder;
  /**
   * v2.3.1 (issue #1)：tool_result 可能在 tool_use 之前到达（jsonl 行序错位 /
   * 不同 session 文件混杂）。先走 fallback 渲染独立卡，batch 结束后
   * reconcilePendingToolResults 重新匹配 + 注入。
   */
  pendingToolResults: Map<
    string,
    {
      block: { type: "tool_result"; tool_use_id: string; content: unknown; is_error?: boolean };
      element: HTMLElement;
    }
  >;
  /**
   * 按 seq 去重集合。一个 Tab == 一个 jsonl path == 一个 seq 空间（本地 watcher 的
   * per-path seqs / 远端 daemon 的 per-process SeqCounter）。SSH 重连后新 daemon 会从
   * seq 0 重发整个会话 → 命中即丢，避免 Tab 内容翻倍。本地 seq 全程唯一 → 永不命中（no-op）。
   *
   * 注意：本集合**只防同 seq 重投**。本地 watcher 截断重读是**换新 seq** 重投整个
   * 文件、此处放行（at-least-once 投递，INVARIANTS § 25）——uuid 级幂等由下面的
   * processedUuids（#26）+ computeMainBranch 入口去重 + BranchFolder.seenUuids
   * （#25）分层兜住。closeTab 时 clear。
   */
  seenSeqs: Set<number>;
  /**
   * Batch13-F40a:尾部优先窗口账本(单洞后缀不变量,详 live-window.ts)。
   * 启动重放的旧记录不建卡、收纳于此(meta/branch 数据已喂);floor=null(virgin)
   * 的后台 tab 在 onBatchEnd 空闲物化尾段 / switchTo 命中时同步物化。
   */
  window: TailWindow;
  /**
   * F40b R-1:批期「窗口内中部插入」缓冲——大增量批(>600 行切块,末块先发)落在
   * 已渲染 tab 上时,老块 seq≥floor 但 <timeline.maxSeq,逐条挂 DOM 会跨帧上方
   * 插入(§21 病根)。缓冲到 onBatchEnd 一次 batchInsert 挂载。
   */
  midBatchBuffer: JsonlLinePayload[];
  /** F40b:上翻补批 scroll listener 引线(closeTab 摘) */
  fillHandler: (() => void) | null;
  /**
   * issue #26：已处理记录的 uuid 集——onLine 入口的 at-least-once 幂等
   * （违反此约束见 doc/INVARIANTS.md § 25）。截断重读换新 seq 重投时 seenSeqs 放行，
   * 若不按 uuid 拒掉，每条记录会以更大的 seq 在 timeline 末尾再渲染一遍（整段内容
   * 翻倍），且 trackAgents/unread 等副作用也会被重投误触发——故在入口整体拒掉。
   * 无 uuid 的记录（ai-title/mode 等元信息）不占集合、照常处理（它们本身幂等；
   * 已知微小残留：无 uuid 的 system 细条理论上可翻倍，影响面可忽略）。closeTab 时 clear。
   */
  processedUuids: Set<string>;
}

/** Tab 数量摘要，发给宿主用于状态栏 / empty-state 等外部 UI */
export interface TabsSummary {
  total: number;
  live: number;
  archived: number;
}

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
interface TabButtonRefs {
  root: HTMLButtonElement;
  label: HTMLSpanElement;
  badge: HTMLSpanElement;
  cwdBtn: HTMLSpanElement;
}

/** F51：远端 tmux 会话(`list_remote_tmux` 后端返回,反查 attach 用)。 */
interface TmuxSession {
  name: string;
  path: string;
  command: string;
  attached: boolean;
  windows: number;
}
/** tmux 反查缓存 TTL:菜单打开按需查,短缓存避免重复右键狂拉 ssh。 */
const TMUX_CACHE_TTL_MS = 8000;
/** F51：tmux 前台命令是否算 claude 会话。真机 tmux 多报 `claude`(调研 03 §2c 实测),
 * 但视启动路径也可能报解释器 `node`(claude 是 Node CLI)——两者都认,叠加 cwd 精确匹配
 * 收窄误配(D-正确性 Sug2:只认 claude 会在报 node 的环境静默失效)。 */
function isClaudeTmuxCommand(cmd: string): boolean {
  return cmd === "claude" || cmd === "node";
}

export class TabManager {
  private tabs = new Map<string, Tab>();
  /** F51：per-origin tmux 会话短缓存(反查 attach)。null=该 origin 无 tmux。 */
  private tmuxCache = new Map<string, { ts: number; sessions: TmuxSession[] | null }>();
  /** 按插入顺序的 sessionId 数组，与 this.tabs.keys() 顺序一致但避免每次 Array.from */
  private orderedIds: string[] = [];
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  private tabButtons = new Map<string, TabButtonRefs>();
  private activeId: string | null = null;
  /**
   * v2.2 (issue #12): 当前是否在 batch 模式（启动重放 jsonl-batch 期间）。
   * batch 模式中 ensureTab 创建的新 Tab 也要把 BranchFolder 设成 batch。
   *
   * P5.2 B 重构：inPrependMode / pendingPrependFragment / source flag 全删 —— 前端
   * 改用 RecordTimeline 按 seq binary-insert，DOM 位置由 seq 决定不受 emit 顺序影响。
   * 仍保留 inBatch 是因为它控两件事：(1) lazy hljs 注册 (2) BranchFolder.batchMode。
   */
  private inBatch = false;
  /**
   * issue #11: 每个 sid 当前 task 列表（由 ensureTab 拉初次快照 + task-update 事件
   * 更新）。切 Tab 时把对应 sid 的快照喂给全局 TasksPanel。
   */
  private tasksBySid = new Map<string, TaskEntry[]>();
  /**
   * issue #19：归档信号（session-ended）可能早于 replay 把该 sid 的 Tab 建出来。
   * archiveTab 时若 Tab 还不存在，记进这里；ensureTab 建 Tab 时回查、落实归档。
   *
   * issue #20 后 session-ended 已改进 events.ts 的 queue 与行同序处理（否则补发
   * 归档会被后续 drain 的远端行 un-archive 吃掉），正常路径下 ended 不会再早于
   * 行到达——本集合降级为防御层（§ 17a 双层防御），保留兜“ended 先于该 sid 任何
   * 行”的异常序。
   */
  private pendingArchive = new Set<string>();
  /**
   * issue #23：红绿灯信号早于 Tab 建出来时暂存（同 pendingArchive 的时序竞争模式：
   * session-activity 同步派发，而建 Tab 的行走异步 queue/drain）。ensureTab 时落实。
   */
  private pendingActivity = new Map<
    string,
    { status: string; waitingFor: string | null }
  >();
  /**
   * v2.4 issue #2：用户在终端真敲键 → 自动切到对应 Tab 的开关。
   * 默认 true，从 config.json (autoFollowUserActive) 加载。
   */
  private autoFollowUserActive: boolean = true;
  /**
   * v2.4 issue #2：自动切 tab 时是否同时把 monitor 窗口拉前台。默认 false。
   */
  private bringMonitorToFront: boolean = false;
  /**
   * v2.4 issue #2：用户**手动**点 Tab Bar / Ctrl+Tab 后 5s 内拒绝任何 user-active
   * 自动切。表示"我现在主动在看另一个 tab，请别抢回去"。
   *
   * 跟 v1 早期 user-lock 的区别：v1 阻塞 OS focus 检测（已废），v2.4 阻塞
   * watcher 反推的 type=user 信号；信号语义不同，5s 经验值复用合理。
   *
   * 0 = 没有 override 中。每次 manual switchTo 时更新为 now+5000。
   */
  private manualOverrideUntil: number = 0;
  /** Manual override 窗口长度（ms）。issue #2 钦定 5s。 */
  private static readonly MANUAL_OVERRIDE_MS = 5000;
  /** Batch13-F40a:物化/后台 tab 尾段条数(与 F39 viewer TAIL_INITIAL 同语义) */
  private static readonly MATERIALIZE_TAIL_K = 150;
  /** F40b:上翻补批批量/触发距离(沿用 F39 实测值) */
  private static readonly FILL_BATCH = 200;
  private static readonly TOP_TRIGGER_PX = 800;
  /** F40b:补批防重入(补偿测量期间嵌套触发会算错差值) */
  private renderingFill = false;

  /**
   * Tab 撕离（tear-off）拖拽状态机。同一时刻只允许一个拖拽，整段存这里。
   * - mousedown（左键，非子动作按钮）记录起点 → 候选拖拽（dragging=false）
   * - document mousemove 越过 6px 阈值 → dragging=true，建 ghost、源 Tab 变暗
   * - 指针拖离 tab 栏右缘（clientX > barRight + 16，F33 竖栏后为横向判定）→ armed=true（松手即弹窗）
   * - document mouseup：armed → openInNewWindow(落点)；否则取消。两种情况都抑制后续 click
   * null = 当前无拖拽。
   */
  private drag: {
    sid: string;
    startX: number;
    startY: number;
    barRight: number;
    root: HTMLElement;
    dragging: boolean;
    armed: boolean;
    ghost: HTMLElement | null;
    onMove: (e: MouseEvent) => void;
    onUp: (e: MouseEvent) => void;
  } | null = null;
  /**
   * 拖拽撕离阈值：指针移动超过此像素才判定为"拖"，否则视为普通点击。
   */
  private static readonly DRAG_THRESHOLD_PX = 6;
  /**
   * 拖拽结束后需抑制掉紧随 mouseup 的那次 click 的 sid（避免拖完又误切 Tab）。
   * null = 不抑制。click handler 命中后清零（一次性）。
   */
  private suppressClickSid: string | null = null;

  constructor(
    private barEl: HTMLElement,
    private streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI */
    private onTabsChanged?: (summary: TabsSummary) => void,
    /** issue #11: 全局 TasksPanel，切 Tab / 收事件时由 TabManager 喂数据 */
    private tasksPanel?: TasksPanel,
    /** issue #23: 全局 AgentsPanel（subagent 列表 + 各自状态灯），喂数方式同 tasksPanel */
    private agentsPanel?: AgentsPanel,
  ) {}

  private notifyChanged(): void {
    if (!this.onTabsChanged) return;
    let live = 0;
    let archived = 0;
    for (const t of this.tabs.values()) {
      if (t.status === "archived") archived += 1;
      else live += 1;
    }
    this.onTabsChanged({ total: this.tabs.size, live, archived });
  }

  /**
   * v2.2 (issue #12): 启动重放（jsonl-batch）开始时调一次。所有现有 Tab 的
   * BranchFolder 切到 batch 模式 —— 后续 recordAdded 只 push 不算 mainBranch。
   * 重放期 ensureTab 新创建的 Tab 也会自动进 batch（看 this.inBatch）。
   *
   * P5.2 B 重构：删了 inPrependMode / pendingToolGroup 清零 —— 前端用 timeline
   * 按 seq 排序，tool-group 合并改后处理（看左邻居），不再需要 chunk 边界协调。
   */
  onBatchStart(): void {
    this.inBatch = true;
    // P5.5 B 重构：lazy 通过 ctx.lazy 传到 renderMarkdown —— onLine 构造 ctx 时
    // 用 this.inBatch 设置。不再依赖 setRenderLazyMode 全局开关。
    for (const t of this.tabs.values()) {
      t.branchFolder.setBatchMode(true);
      // Batch13-F40a:deferMode 已退役——重放期旧记录根本不建卡(收纳进 tab.window),
      // "视口上方插入"次数为 0,比"延后到一帧"更强(INVARIANTS §21.3)。
    }
  }

  /**
   * v2.2 (issue #12): 重放批次完结。各 Tab 调 flushPending 一次性算 + rebuild，
   * 然后切回 live 模式。后续真实时新消息按 timeline 路径走。
   */
  onBatchEnd(): void {
    this.inBatch = false;
    for (const t of this.tabs.values()) {
      // F40b R-1:先把批期缓冲的中部插入一次挂载(内含 unwrapAll/rebuildNow),
      // 再走既有 flushPending/reconcile
      this.flushMidBatchBuffer(t);
      t.branchFolder.flushPending();
      t.branchFolder.setBatchMode(false);
      // 切块场景下，老块的 tool_use 现在已渲染 → 重试匹配早到的 fallback result
      const ctx: RenderContext = {
        parentPath: t.parentPath,
        origin: t.origin,
        toolUseNames: t.toolUseNames,
        toolUseElements: t.toolUseElements,
        pendingToolResults: t.pendingToolResults,
      };
      // S-6:孤儿卡出 DOM 的同时出账,防悬空 anchor
      for (const el of reconcilePendingToolResults(ctx)) t.timeline.removeByElement(el);
    }
    // Batch13-F40a:active tab 不足一屏(或还是 virgin)→ 立即补物化到可见;
    // 其余 virgin 后台 tab 进空闲物化队列(逐个串行,避免并发建卡风暴)。
    const active = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (active && active.window.pendingCount > 0) {
      const el = active.streamEl;
      const notScrollable = el.scrollHeight - el.clientHeight <= 1;
      if (active.window.floorSeq === null || notScrollable) {
        this.materializeUntilFilled(active);
        active.stream.scrollToBottom();
      }
    }
    // D 审计 S-5:archived 死会话不进后台物化队列(纯浪费;switchTo 命中 virgin
    // 已有同步物化兜底)。
    this.materializeQueue = [...this.tabs.entries()]
      .filter(
        ([sid, t]) =>
          sid !== this.activeId &&
          t.status !== "archived" &&
          t.window.floorSeq === null &&
          t.window.pendingCount > 0,
      )
      .map(([sid]) => sid);
    this.scheduleIdleMaterialize();
    // F40b:active tab 未走物化分支(尾块已可滚)时也要挂哨兵
    if (active) this.updateSentinel(active);
  }

  /**
   * D 审计 R-3:一次 150 条 payload 可能只产出几张卡(tool-group 合并成单卡 34px、
   * skip 记录占配额不产卡)——工具密集会话一轮物化后屏幕仍近空,而 F40a 没有上翻
   * 补批兜底。有界循环补到可滚动或账本弹尽(≤4 轮防病态会话空转)。
   */
  private materializeUntilFilled(tab: Tab): void {
    for (let round = 0; round < 4; round++) {
      if (tab.window.pendingCount === 0) return;
      const el = tab.streamEl;
      if (round > 0 && el.scrollHeight - el.clientHeight > 1) return;
      this.materializeTail(tab);
    }
  }

  /**
   * Batch13-F40a/b:批量渲染内核(物化 / 上翻补批 / R-1 缓冲 flush 共用)。
   * - sink 不接 onRealUserInput(历史 user 卡不得触发自动切 tab)、branch/queue/title
   *   为 no-op(收纳/缓冲时 routeMetaAndBranch 已喂过,BranchFolder.seenUuids 双保险);
   * - 不计 unread(重放历史不是"未读新消息",修 backlog S-2;R-1 缓冲的 unread
   *   在缓冲时已计);
   * - lazy hljs + observe(批量渲染的是历史内容,滚入视口再高亮);
   * - 插卡前 unwrapAll 摊平(邻居可能在 fold wrap 内,F39 实证的不变量)、批内
   *   暂停逐卡 snap(S-7,防 150 次强制 reflow)、插完 reconcile 孤儿 tool_result
   *   (S-6 同步出账)、rebuildNow 无条件重折(flushPending 的 setsEqual 短路会把
   *   摊平永久化)。
   */
  private renderPayloadsBatch(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      pendingToolResults: tab.pendingToolResults,
      lazy: true,
    };
    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      observeForLazyEnhance: true,
    };
    tab.branchFolder.unwrapAll();
    tab.stream.batchInsert(() => {
      for (const p of payloads) {
        try {
          renderContentRecord(p, ctx, sink);
        } catch (e) {
          console.error("[tabs] 批量渲染单条失败,跳过:", p.seq, e);
        }
      }
    });
    for (const el of reconcilePendingToolResults(ctx)) tab.timeline.removeByElement(el);
    tab.branchFolder.rebuildNow();
  }

  /**
   * Batch13-F40a:物化 tab 的尾段——从窗口账本弹出 seq 最高的 ≤k 条建卡。
   * 物化目标是 virgin/近 virgin tab(无滚动位置可保),不需要滚动补偿——上翻补批
   * 的手动补偿在 fillAbove(F40b)。
   */
  private materializeTail(tab: Tab, k = TabManager.MATERIALIZE_TAIL_K): void {
    this.renderPayloadsBatch(tab, tab.window.takeTail(k));
    this.updateSentinel(tab);
  }

  /**
   * F40b R-1:批期缓冲的「窗口内中部插入」(大增量批老块)一次性挂载。
   * 排序后走渲染内核(含 unwrapAll/rebuildNow——不能依赖随后 flushPending,
   * 它的 setsEqual 短路会把摊平永久化)。在 onBatchEnd 的 flushPending/reconcile
   * 之前调。
   */
  private flushMidBatchBuffer(tab: Tab): void {
    if (tab.midBatchBuffer.length === 0) return;
    const payloads = tab.midBatchBuffer.sort((a, b) => a.seq - b.seq);
    tab.midBatchBuffer = [];
    // 已知取舍(D 审计):批末 flush 不做选区守卫——unwrap/rebuild 会杀进行中选区,
    // 但 flush 不可延迟(数据必须落),且触发面(选中文本时恰逢远端大增量批)极窄。
    this.renderPayloadsBatch(tab, payloads);
    this.refreshTabBar(); // 缓冲期攒下的 unread 徽标一次刷新
  }

  /**
   * F40b:上翻补批。守卫:防重入 / 账空 / 选区进行中(补批 unwrap/rebuild 会杀
   * 进行中的选区,等下次 scroll 再试)。补偿:临时关原生锚定(防 WebView2 与手动
   * 补偿 double-shift;WebKitGTK 本就无锚定),测量→渲染→scrollTop 回写在同一
   * 同步任务内(不许 await/rAF 打断)。rAF 自链:零高批/不足一屏无 scroll 事件
   * (F39-R1 场景),补完复检直到离开触发区或账尽。
   */
  private fillAbove(tab: Tab): void {
    if (this.renderingFill) return;
    if (tab.window.pendingCount === 0) return;
    const sel = document.getSelection();
    if (sel && !sel.isCollapsed) return;
    const el = tab.streamEl;
    this.renderingFill = true;
    try {
      el.style.overflowAnchor = "none";
      const beforeH = el.scrollHeight;
      const beforeTop = el.scrollTop;
      this.renderPayloadsBatch(tab, tab.window.takeTail(TabManager.FILL_BATCH));
      // 哨兵刷新必须在补偿回写**之前**:账尽移除的 ±30px 计入 Δ 一并吃掉——
      // 移除若在补偿后,dev(无锚定)会在"会话第一条"处一次性跳 30px(D 审计)。
      this.updateSentinel(tab);
      el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
    } finally {
      // 还原必须在 finally:渲染内核抛出时留下 overflow-anchor:none = 该 tab 永久
      // 失去原生锚定,违反 §21.2 且无自愈(D 审计,两家共识)
      el.style.overflowAnchor = "";
      this.renderingFill = false;
    }
    requestAnimationFrame(() => {
      if (this.activeId !== tab.sessionId) return;
      const t = this.tabs.get(tab.sessionId);
      if (!t || t.window.pendingCount === 0) return;
      const e = t.streamEl;
      if (e.scrollTop <= TabManager.TOP_TRIGGER_PX || e.scrollHeight - e.clientHeight <= 1) {
        this.fillAbove(t);
      }
    });
  }

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON——无 devtools 环境下 E2E 断言的
   * 唯一出口(经 e2e-probe 热键 → fe_perf 日志)。生产不接线,方法本身无副作用。
   */
  debugSnapshot(): string {
    const tab = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (!tab) return JSON.stringify({ active: null });
    const el = tab.streamEl;
    const statusText = document.getElementById("status-bar")?.textContent ?? "";
    return JSON.stringify({
      sid: tab.sessionId.slice(0, 8),
      scrollTop: Math.round(el.scrollTop),
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
      distBottom: Math.round(el.scrollHeight - el.scrollTop - el.clientHeight),
      pending: tab.window.pendingCount,
      midBuffer: tab.midBatchBuffer.length,
      timeline: tab.timeline.size,
      foldWraps: tab.stream.contentElement.querySelectorAll(":scope > .branch-fold-wrap").length,
      sentinel:
        tab.stream.contentElement.querySelector(":scope > .stream-more-above")?.textContent ??
        null,
      err: statusText.startsWith("ERR") || statusText.startsWith("REJ") ? statusText : null,
    });
  }

  /**
   * F40b:顶端哨兵——账本非空时置顶「还有 N 条更早消息」,账尽移除。
   * 非 timeline 实体、无 data-uuid(BranchFolder 视作断 run,天然免疫 fold);
   * 二分插入的 anchor 恒为 timeline 元素,最老卡 insertBefore(首卡) 自然落哨兵后。
   */
  private updateSentinel(tab: Tab): void {
    const content = tab.stream.contentElement;
    let el = content.querySelector(":scope > .stream-more-above") as HTMLElement | null;
    const n = tab.window.pendingCount;
    if (n === 0) {
      el?.remove();
      return;
    }
    if (!el) {
      el = document.createElement("div");
      el.className = "stream-more-above";
      content.prepend(el);
    }
    el.textContent = `↑ 还有 ${n} 条更早消息 · 上翻加载`;
  }

  /** F40a:virgin 后台 tab 的空闲物化队列(串行;rIC 缺失时 setTimeout 兜底) */
  private materializeQueue: string[] = [];
  private materializeScheduled = false;

  private scheduleIdleMaterialize(): void {
    if (this.materializeScheduled) return;
    const sid = this.materializeQueue.shift();
    if (sid === undefined) return;
    this.materializeScheduled = true;
    const run = (): void => {
      this.materializeScheduled = false;
      const tab = this.tabs.get(sid);
      // 只物化仍是 virgin 的(switchTo 可能已同步物化过);二次 batch 开始则原样跳过,
      // 账本继续收纳,批结束会重新排队。
      if (tab && !this.inBatch && tab.window.floorSeq === null) {
        this.materializeTail(tab);
      }
      this.scheduleIdleMaterialize();
    };
    if (typeof window.requestIdleCallback === "function") {
      window.requestIdleCallback(run, { timeout: 2000 });
    } else {
      window.setTimeout(run, 200);
    }
  }

  /**
   * 收到一行 JSONL 时调用。
   *
   * P5.2 B 重构：路由到 renderStreamRecord（三 caller 共享管线）。
   * - timeline.insert 按 seq 决定 DOM 位置（不再有 source/inPrependMode 区分）
   * - tool-group 后处理合并基于 timeline 左邻居
   * - ai-title 走 sink.onTitleUpdate
   * - 真用户输入走 sink.onRealUserInput → this.userActive
   */
  onLine(payload: JsonlLinePayload): void {
    const tab = this.ensureTab(
      payload.session_id,
      payload.cwd,
      payload.path,
      payload.seq,
      payload.origin ?? null,
    );

    // SSH 重连后远端 daemon 从 seq 0 重发该 session 整段 jsonl → 按 seq 去重。必须在
    // renderStreamRecord 之前、且覆盖 skip 记录（attachment/isMeta/空 user 有 seq 但不入
    // timeline，timeline.has 漏判）。本地 seq 全程唯一 → 此 set 永不命中（本地 no-op）。
    if (tab.seenSeqs.has(payload.seq)) return;
    tab.seenSeqs.add(payload.seq);

    // issue #26：按 uuid 去重——截断重读换新 seq 重投时上面的 seq 去重放行，这里把
    // "同一记录再来一遍"整体拒掉（不渲染、不 trackAgents），否则内容在 timeline 末尾
    // 翻倍（INVARIANTS § 25 的渲染层履约点）。必须放在 ensureTab 之后（远端
    // un-archive 靠"收到行"翻转，重投行也要触发它）、seq 去重之后。
    const uuid = (payload.message as { uuid?: unknown }).uuid;
    if (typeof uuid === "string" && uuid.length > 0) {
      if (tab.processedUuids.has(uuid)) return;
      tab.processedUuids.add(uuid);
    }

    // Batch14-F42：turn-end 系统通知。放在双重去重之后（重投行不重报）、
    // 渲染管线之前（通知与渲染/收纳互相独立）。批量重放由 inBatch 短路。
    turnEndNotifier.observe(payload.session_id, tab.title, payload, this.inBatch);

    // issue #23（第二增量）：配对 agent 工具调用，喂 AgentsPanel
    this.trackAgents(tab, payload.message);

    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: (rec: BranchRecord) => tab.branchFolder.recordAdded(rec),
      // issue #36：队列消息内容 → 折叠豁免集合
      onQueueOperation: (content: string) => tab.branchFolder.addQueuedContent(content),
      onTitleUpdate: (title: string) => this.applyAiTitle(tab, title),
      onRealUserInput: (sid: string) => this.userActive(sid),
      observeForLazyEnhance: this.inBatch,
    };

    // Batch13-F40a:meta/branch 收集与渲染解耦——收纳(不建卡)的记录也要喂
    // title/queue/branch 数据(routeMetaAndBranch 是两条路径的单一来源,账本 §3)。
    if (routeMetaAndBranch(payload, sink) === "consumed") return;

    // 门控(单洞后缀不变量,纯 seq 判定):
    // - virgin + batch:active tab 首条 content 钉 floor(尾块直渲,进步式首屏
    //   与 deferMode 时代一致);后台 tab 恒收纳(virgin,批后空闲物化)。
    // - virgin + live:新开 tab 直渲并钉 floor。
    // - seq >= floor:渲染(live 追加/尾块);seq < floor:收纳(旧块/F30 尾部
    //   优先回填/迟到块——无论 inBatch,与 batch 哨兵解耦)。
    // D 审计 C-1(近 virgin 竞态):批后 rIC 队列还没轮到该 tab 就来了真 live 行——
    // 若直接 pinFloor(新行 seq),账本里整段历史会被钉死滞留(F40a 无再物化入口)。
    // 先物化尾段(takeTail 顺带钉 floor),live 行(seq 恒更新)再照常 admit。
    if (tab.window.floorSeq === null && !this.inBatch && tab.window.pendingCount > 0) {
      this.materializeTail(tab);
    }
    const floor = tab.window.floorSeq;
    let render: boolean;
    if (floor === null) {
      render = !this.inBatch || this.activeId === tab.sessionId;
      if (render) tab.window.pinFloor(payload.seq);
    } else {
      render = tab.window.admit(payload.seq);
    }
    // F40b R-1:批期落在渲染窗口内的**中部**插入(seq≥floor 且 <已渲染最高 seq
    // ——大增量批的老块)→ 缓冲,onBatchEnd 一次挂载,消逐帧上方插入(§21)。
    // 离线期真新消息:unread 照计(粒度=记录,与逐条渲染的 inserted 判定在
    // tool-group 合并上略有偏差,99+ 封顶下可接受)。
    if (render && this.inBatch && payload.seq < tab.timeline.maxSeq) {
      tab.midBatchBuffer.push(payload);
      // 徽标刷新攒到批末 flush 一次(D 审计:600 条缓冲 = 600 次全 bar 巡检)
      if (this.activeId !== tab.sessionId) tab.unread += 1;
      return;
    }
    if (!render) {
      tab.window.defer(payload);
      // 仪表(jsdom 单测无 main.ts,须防 undefined)
      if (window.__ccmPerf) window.__ccmPerf.recordsDeferred = (window.__ccmPerf.recordsDeferred ?? 0) + 1;
      // 收纳不计 unread——重放历史不是"未读新消息"(修 backlog S-2)
      return;
    }

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      pendingToolResults: tab.pendingToolResults,
      // P5.5：batch 期间走 lazy hljs（代码块占位 + IntersectionObserver 触发再补跑）
      lazy: this.inBatch,
    };
    const beforeSize = tab.timeline.size;
    renderContentRecord(payload, ctx, sink);
    const inserted = tab.timeline.size > beforeSize;

    // unread 计数：只有真新 entry 入 timeline 才算（tool-group 合并到旧 group 不算）
    if (inserted && this.activeId !== tab.sessionId) {
      tab.unread += 1;
      this.refreshTabBar();
    }
  }

  /**
   * Batch5-F18：骨架 Tab——活跃清单（本地 IPC / 远端 session_added 事件）一到
   * 即建，不等首条内容行。复用 ensureTab 全部语义：cwd 以 MAX_SAFE_INTEGER 的
   * seq 记入 → 任何真实行的 cwd（更小 seq）照常覆盖为项目根；parentPath 空由
   * 首条行回填；pendingArchive/pendingActivity 落实、batch 模式继承均沿用。
   * 已存在同 sid Tab 时为 no-op（幂等，重连重发 session_added 无害）。
   */
  /**
   * Batch7-F24 树状排序：bg tab 插到同 (cwd, origin) 交互宿主（及其既有 bg 子项）
   * 之后；无宿主则追加末尾。交互 tab 创建时反向重锚——把已存在的同 (cwd, origin)
   * bg tab 拉到自己身后（骨架清单里 bg 可能先于宿主出现）。父子判定 v1 = cwd
   * 归属（pidfile 无 parentSessionId 字段，精确父子留 backlog）。
   */
  private placeInOrder(tab: Tab): void {
    const isBg = tab.kind !== null && tab.kind !== "interactive";
    const sameHost = (t: Tab | undefined): boolean =>
      !!t && t.cwd !== null && t.cwd === tab.cwd && t.origin === tab.origin;
    if (isBg && tab.cwd) {
      // 找宿主（交互 + 同 cwd/origin）——插到宿主连同其已有 bg 子串之后
      for (let i = 0; i < this.orderedIds.length; i++) {
        const t = this.tabs.get(this.orderedIds[i]);
        if (sameHost(t) && (t!.kind === null || t!.kind === "interactive")) {
          let j = i + 1;
          while (j < this.orderedIds.length) {
            const c = this.tabs.get(this.orderedIds[j]);
            if (sameHost(c) && c!.kind !== null && c!.kind !== "interactive") j++;
            else break;
          }
          this.orderedIds.splice(j, 0, tab.sessionId);
          return;
        }
      }
      this.orderedIds.push(tab.sessionId);
      return;
    }
    // 交互 tab：追加，再把**真孤儿** bg 子项拉到身后（保持原相对序）。
    // 已紧跟在先到宿主（同 cwd/origin 交互 tab）之后的 bg 子串不动——
    // 计划契约"多宿主取第一个"（审计 D-R3：第二个同 cwd 交互会话不许搬走
    // 第一个宿主已挂好的子树）。
    this.orderedIds.push(tab.sessionId);
    if (tab.cwd) {
      const orphans: string[] = [];
      let anchored = false; // 当前扫描位置是否处于"sameHost 宿主的 bg 子串"内
      for (const sid of this.orderedIds) {
        if (sid === tab.sessionId) continue;
        const t = this.tabs.get(sid);
        const isBg = !!t && t.kind !== null && t.kind !== "interactive";
        if (!isBg) {
          anchored = sameHost(t) && (t!.kind === null || t!.kind === "interactive");
          continue;
        }
        if (sameHost(t)) {
          if (!anchored) orphans.push(sid);
          // anchored 保持——宿主的 bg 子串延续
        } else {
          anchored = false; // 异族 bg 打断子串
        }
      }
      if (orphans.length) {
        this.orderedIds = this.orderedIds.filter((sid) => !orphans.includes(sid));
        const at = this.orderedIds.indexOf(tab.sessionId) + 1;
        this.orderedIds.splice(at, 0, ...orphans);
      }
    }
  }

  createSkeletonTab(
    sessionId: string,
    cwd: string | null,
    origin: string | null,
    kind: string | null = null,
    name: string | null = null,
  ): void {
    this.ensureTab(sessionId, cwd, "", Number.MAX_SAFE_INTEGER, origin, kind, name);
  }

  /** Batch5-F19：启动 active 选择用（last-active 是否已有 tab）。 */
  hasTab(sessionId: string): boolean {
    return this.tabs.has(sessionId);
  }

  /** Batch5-F19：switchTo 是否写回 last-active（viewer/tear-off 窗口置 false）。 */
  persistLastActive = true;

  /** Batch5-F19（G 验收）：用户手动切 tab 时回调——main.ts 用它清 pendingStartupActive，
   *  防迟到的远端宣告补切抢走用户已选的焦点。 */
  onManualSwitch: (() => void) | null = null;

  ensureTab(
    sessionId: string,
    cwd: string | null,
    sourcePath: string,
    seq: number,
    origin: string | null = null,
    kind: string | null = null,
    bgName: string | null = null,
  ): Tab {
    let tab = this.tabs.get(sessionId);
    if (tab) {
      // SSH 重连：远端会话掉线时被 flush 归档过，现在又收到它的行 = daemon 在重放 = 会话仍
      // 活着 → 复活成 live。必须放在 ensureTab 里（在 onLine 的 seq 去重 return 之前），否则整段
      // 重放全被去重时连第一条行都走不到翻转。**仅远端**：本地归档由 PID 判活驱动，不靠「收到行」
      // 翻转，避免会话退出时尾写把已归档的本地 Tab 误复活（远端掉线归档是连接驱动，无此风险）。
      if (tab.status === "archived" && tab.origin !== null) {
        tab.status = "live";
        this.refreshTabBar();
      }
      // v2.22.2 kind 冲突消解:同一 sid 可能有多份 pidfile(实证:cc-daemon 的
      // bg-spare 备用进程复用**父会话的 sid**写 kind=bg)——宣告到达顺序不定,
      // bg 先到会把真交互会话降格成 ⚙ 且树状挂到别的宿主下(用户截图实锤)。
      // 规则:**interactive 恒压过 bg**——后到的 interactive 宣告在此升格纠正
      // (重新按宿主定位 + 把同 cwd 孤儿 bg 拉回身后);反向(bg 后到)绝不降格。
      if (kind === "interactive" && tab.kind !== null && tab.kind !== "interactive") {
        tab.kind = "interactive";
        tab.bgName = null;
        tab.title = this.computeTitle(tab);
        const i = this.orderedIds.indexOf(sessionId);
        if (i >= 0) this.orderedIds.splice(i, 1);
        this.placeInOrder(tab);
        this.refreshTabBar();
      }
      // Batch5-F18：骨架 Tab（无行创建）的 parentPath 为空——首条带路径的行回填，
      // 保住「在新窗口打开」等依赖 jsonl 路径的功能。
      if (!tab.parentPath && sourcePath) {
        tab.parentPath = sourcePath;
      }
      // cwd 取**最早（最小 seq）**那条记录的 —— 即项目根 / 启动目录。
      // 不能用「第一个到达的」：启动重放末块先发，最先到的是最新记录，而会话的 cwd
      // 可能在过程中漂移（如工作目录切到子目录）→ 会抓到子目录而非项目根。与历史
      // 浏览器 quick_extract_cwd（读最早 cwd）口径一致。
      if (cwd && seq < tab.cwdSeq) {
        tab.cwd = cwd;
        tab.cwdSeq = seq;
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
      }
      return tab;
    }

    const title = computeTitleFor(sessionId, cwd, null, origin, kind, bgName);

    const streamEl = document.createElement("div");
    streamEl.className = "stream"; // 默认 .stream 已含 visibility:hidden（见 styles.css）
    this.streamRootEl.appendChild(streamEl);

    const stream = new MessageStream(streamEl);
    const branchFolder = new BranchFolder(stream.contentElement);
    const timeline = new RecordTimeline(stream);
    // v2.2 issue #12: 重放期创建的新 Tab 也进 batch 模式，避免每条 record 都
    // 触发 O(N) computeMainBranch。批结束时 onBatchEnd 会统一 flush。
    if (this.inBatch) {
      branchFolder.setBatchMode(true);
    }

    // v2.3.0 issue #11: 异步 fetch 初始 task 快照。task-update 事件路径并行更新
    // tasksBySid，两路收敛到同一份数据；若 sid 是 active 同步推给全局 panel。
    void fetchSessionTasks(sessionId).then((tasks) => {
      this.tasksBySid.set(sessionId, tasks);
      if (this.activeId === sessionId) {
        this.tasksPanel?.setSession(sessionId, tasks);
      }
    });

    tab = {
      sessionId,
      kind,
      bgName,
      title,
      cwd,
      // 记下当前 cwd 来源的 seq；后续更早（更小 seq）的记录可覆盖（取项目根）。
      cwdSeq: cwd ? seq : Number.POSITIVE_INFINITY,
      aiTitle: null,
      origin,
      status: "live",
      streamEl,
      stream,
      parentPath: sourcePath,
      unread: 0,
      timeline,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      branchFolder,
      pendingToolResults: new Map(),
      seenSeqs: new Set(),
      window: new TailWindow(),
      midBatchBuffer: [],
      fillHandler: null,
      processedUuids: new Set(),
      // issue #23：红绿灯信号若先于建 Tab 到达，从暂存取（否则 null=未知→绿）
      activity: this.pendingActivity.get(sessionId) ?? null,
      agents: new Map(),
    };
    this.pendingActivity.delete(sessionId);
    // F40b:上翻补批触发器(passive 只读滚动位置;handler 内判 active,后台 tab
    // 的程序化滚动/尺寸变化不触发补批)
    const fillHandler = (): void => {
      if (this.activeId !== sessionId) return;
      const t = this.tabs.get(sessionId);
      if (t && t.streamEl.scrollTop <= TabManager.TOP_TRIGGER_PX) this.fillAbove(t);
    };
    streamEl.addEventListener("scroll", fillHandler, { passive: true });
    tab.fillHandler = fillHandler;
    // issue #19：若该 sid 的归档信号先于本次建 Tab 到达（见 archiveTab），落实归档，
    // 避免重载后已结束会话复活成关不掉的 live Tab。本地 un-archive（上方 origin!==null
    // 那条）不适用，故归档后续 replay 行也不会把它复活。
    if (this.pendingArchive.delete(sessionId)) {
      tab.status = "archived";
      tab.activity = null; // 同 archiveTab：死会话不留陈旧灯/tooltip
    }
    this.tabs.set(sessionId, tab);
    this.placeInOrder(tab);

    if (this.activeId === null) {
      // "auto"：首个 Tab 的激活不是用户手势，不该占用 5s manualOverride 抑制
      // auto-follow（G5 验收 S-1）
      this.switchTo(sessionId, "auto");
    } else {
      this.refreshTabBar();
    }
    return tab;
  }

  /** 应用 ai-title：锁定语义标题，并按 [项目名] aiTitle 格式更新 Tab 标题 */
  private applyAiTitle(tab: Tab, aiTitle: string): void {
    const trimmed = aiTitle.trim();
    if (!trimmed) return;
    if (tab.aiTitle === trimmed) return;
    tab.aiTitle = trimmed;
    tab.title = this.computeTitle(tab);
    this.refreshTabBar();
  }

  /** 根据 tab.cwd + tab.aiTitle + sessionId 算出展示标题（远端 Tab 加 `[origin]` 前缀） */
  private computeTitle(tab: Tab): string {
    return computeTitleFor(tab.sessionId, tab.cwd, tab.aiTitle, tab.origin, tab.kind, tab.bgName);
  }

  /** session 退出（~/.claude/sessions/<PID>.json 被删）—— 灰显归档，内容保留 */
  archiveTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      // issue #19：Tab 还没被 ensureTab 建出来（归档信号早于 replay 行到达）——
      // 记下待归档，建 Tab 时落实。否则这里直接 return 会静默丢弃归档 → 僵尸 live Tab。
      this.pendingArchive.add(sessionId);
      this.pendingActivity.delete(sessionId); // issue #23：死会话的暂存灯一并清
      return;
    }
    if (tab.status === "archived") return;
    tab.status = "archived";
    // issue #23：会话结束 → 灯灭（CSS 上 archived 本就隐藏 .live-dot，这里保持状态干净）
    tab.activity = null;
    this.sweepRunningAgents(tab); // 会话死了，running agent 必然中止
    // P5.2 B 重构后无 pendingToolGroup —— archive 不需要打断 tool-group 累积
    // （tool-group 合并改后处理，看 timeline 邻居；archive 后无新 record 入 timeline）。
    this.refreshTabBar();
  }

  /**
   * 会话（重新）变活 → 复活已归档 Tab。archiveTab 的对称面，由后端 SESSION_STARTED
   * 事件驱动（见 events.ts；后端已用 is_session_active 门控，只在 PID 真活时发）。
   *
   * **仅本地**（origin===null）：本地归档/复活由 PID 探活驱动，不靠「收到行」翻转——
   * 避免会话退出尾写误复活（见 ensureTab 行 372 的远端-only 复活注释）。远端 Tab 复活
   * 仍走 ensureTab「掉线归档→重连重放见行复活」路径，与本方法正交、互不触发。
   *
   * 先撤 pendingArchive：归档信号若还停在那（Tab 尚未由 ensureTab 建出），不撤的话
   * 随后建 Tab 会按 pendingArchive 落实归档（行 442）→ 复活被吞。Tab 不存在（全新
   * 会话首启、jsonl 行尚未建 Tab）则 no-op：随后 jsonl-batch 建的新 Tab 默认即 live。
   */
  reviveTab(sessionId: string): void {
    this.pendingArchive.delete(sessionId);
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.origin !== null) return; // 仅本地；远端复活走 ensureTab 见行路径
    if (tab.status !== "archived") return;
    tab.status = "live";
    this.refreshTabBar();
  }

  /**
   * issue #23：红绿灯状态更新（session-activity 事件 / 启动快照两路汇入）。
   * status=null（旧版 CC 无字段）视为未知 → 清空回绿点现状。Tab 还没建则暂存
   * （pendingActivity，ensureTab 落实）。无变化不重绘。
   */
  updateActivity(
    sessionId: string,
    status: string | null,
    waitingFor: string | null,
  ): void {
    const act = status === null ? null : { status, waitingFor };
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      if (act) this.pendingActivity.set(sessionId, act);
      else this.pendingActivity.delete(sessionId);
      return;
    }
    // archived 不更新（审计：心跳清死会话后磁盘残留 PID.json 被重扫会推陈旧
    // activity，archived tab 会挂上过期的 waiting tooltip——灯本身被 CSS 隐藏）。
    if (tab.status === "archived") return;
    if (
      tab.activity?.status === act?.status &&
      tab.activity?.waitingFor === act?.waitingFor
    ) {
      return;
    }
    tab.activity = act;
    // issue #23（第二增量）：turn 结束（idle/shell）→ 没等到 tool_result 的 agent
    // 必然不会再回来（ESC 打断/异常），标 aborted。waiting 不清——其他 agent 可能
    // 还在并行跑（waitingFor "worker request" 正是 agent 在要权限）。
    if (act && (act.status === "idle" || act.status === "shell")) {
      this.sweepRunningAgents(tab);
    }
    this.refreshTabBar();
  }

  /**
   * issue #23（第二增量）：从 jsonl 流配对 agent 工具调用。
   * - assistant 的 Task/Agent tool_use → 注册 running（label 取 input.description，
   *   回退 prompt 首行 / 工具名）
   * - user 的 tool_result（按 tool_use_id 命中）→ done
   * 防 spam：只在真有变化时刷新面板。结构防御：message 形态全 unknown 窄化，
   * 任何不匹配静默跳过（§18 同源精神）。
   */
  private trackAgents(tab: Tab, message: unknown): void {
    const rec = message as {
      type?: string;
      message?: { content?: unknown };
    };
    const content = rec?.message?.content;
    if (!Array.isArray(content)) return;
    let changed = false;
    if (rec.type === "assistant") {
      for (const b of content) {
        const blk = b as {
          type?: string;
          id?: string;
          name?: string;
          input?: { description?: unknown; prompt?: unknown; subagent_type?: unknown };
        };
        if (
          blk?.type !== "tool_use" ||
          typeof blk.id !== "string" ||
          typeof blk.name !== "string" ||
          !isAgentTool(blk.name)
        ) {
          continue;
        }
        const desc =
          typeof blk.input?.description === "string" ? blk.input.description : "";
        const prompt =
          typeof blk.input?.prompt === "string" ? blk.input.prompt : "";
        const label =
          desc || prompt.split("\n")[0]?.slice(0, 80) || blk.name;
        const agentType =
          typeof blk.input?.subagent_type === "string"
            ? blk.input.subagent_type
            : null;
        tab.agents.set(blk.id, {
          id: blk.id,
          label,
          agentType,
          status: "running",
        });
        changed = true;
      }
      // 上限 30：超出删最老的非 running（Map 保持插入序）
      if (tab.agents.size > 30) {
        for (const [id, a] of tab.agents) {
          if (tab.agents.size <= 30) break;
          if (a.status !== "running") tab.agents.delete(id);
        }
      }
    } else if (rec.type === "user") {
      for (const b of content) {
        const blk = b as { type?: string; tool_use_id?: string };
        if (blk?.type !== "tool_result" || typeof blk.tool_use_id !== "string") {
          continue;
        }
        const a = tab.agents.get(blk.tool_use_id);
        if (a && a.status === "running") {
          a.status = "done";
          changed = true;
        }
      }
    }
    if (changed) this.agentsChanged(tab);
  }

  /** issue #23：会话不再 busy（idle/shell/归档）→ 仍 running 的 agent 标 aborted
   *（ESC 打断/崩溃不会有 tool_result，防僵尸"运行中"）。 */
  private sweepRunningAgents(tab: Tab): void {
    let changed = false;
    for (const a of tab.agents.values()) {
      if (a.status === "running") {
        a.status = "aborted";
        changed = true;
      }
    }
    if (changed) this.agentsChanged(tab);
  }

  /** agents 变化 → 若是 active Tab 同步给全局面板 */
  private agentsChanged(tab: Tab): void {
    if (this.activeId === tab.sessionId) {
      this.agentsPanel?.setSession(tab.sessionId, [...tab.agents.values()]);
    }
  }

  /**
   * issue #23：启动/F5 后拉一次红绿灯快照做初始收敛——session-activity 是稀疏
   * 事件、不进 replay buffer，重载会丢（同 fetchSessionTasks 的双路收敛模式）。
   * 失败静默（灯保持未知绿，不影响主功能）。
   */
  async syncActivitySnapshot(): Promise<void> {
    try {
      const list = await invoke<
        { session_id: string; status: string | null; waiting_for: string | null }[]
      >("list_session_activity");
      for (const a of list) {
        this.updateActivity(a.session_id, a.status, a.waiting_for);
      }
    } catch (e) {
      console.warn("list_session_activity failed:", e);
    }
  }

  /**
   * 关闭 Tab：销毁 stream DOM、从 Map 中移除、通知后端 forget 历史、必要时切到相邻 Tab。
   * 仅允许关闭 archived 状态的 Tab，避免误关运行中的会话。
   * forget 后该 session 不会在下次 F5 刷新时被 event_replay 重放复活。
   */
  closeTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status !== "archived") return;

    const wasActive = this.activeId === sessionId;
    const idx = this.orderedIds.indexOf(sessionId);
    // 优先切到后一个 Tab，否则前一个
    const fallbackId =
      this.orderedIds[idx + 1] ?? this.orderedIds[idx - 1] ?? null;

    tab.stream.dispose();
    tab.streamEl.remove();
    // 显式清 Map：释放对已卸载 DOM 节点的强引用，让 GC 可早回收
    // （Map 本身也会随 Tab 对象一起回收，但显式 clear 让 DOM 引用计数立即归零）
    tab.toolUseNames.clear();
    tab.toolUseElements.clear();
    tab.pendingToolResults.clear();
    tab.seenSeqs.clear();
    tab.processedUuids.clear();
    // F40a/b:窗口账本与缓冲持整段历史 payload(大会话数十 MB 级),断引用;摘 fill listener
    tab.window.dispose();
    tab.midBatchBuffer = [];
    if (tab.fillHandler) tab.streamEl.removeEventListener("scroll", tab.fillHandler);
    tab.timeline.dispose();
    tab.branchFolder.dispose();
    this.tasksBySid.delete(sessionId);
    this.tabs.delete(sessionId);
    if (idx >= 0) this.orderedIds.splice(idx, 1);

    // 让后端 event_replay 把这个 session 的历史也丢掉
    void invoke("forget_session", { sessionId }).catch((e) => {
      console.warn(`forget_session ${sessionId} failed:`, e);
    });

    if (wasActive) {
      if (fallbackId !== null) {
        this.switchTo(fallbackId);
      } else {
        this.activeId = null;
        // issue #11: 关掉最后一个 Tab → panel 进入 null session 状态
        this.tasksPanel?.setSession(null, []);
        this.agentsPanel?.setSession(null, []);
        this.refreshTabBar();
      }
    } else {
      this.refreshTabBar();
    }
  }

  /**
   * 切到上 / 下一个 Tab。delta=+1 下一个、-1 上一个。环回。
   * 快捷键 Ctrl+Tab / Ctrl+Shift+Tab 用。
   */
  cycleActive(delta: 1 | -1): void {
    const ids = this.orderedIds;
    if (ids.length === 0) return;
    const idx = this.activeId ? ids.indexOf(this.activeId) : -1;
    const nextIdx = ((idx + delta) % ids.length + ids.length) % ids.length;
    const targetId = ids[nextIdx];
    if (targetId && targetId !== this.activeId) {
      this.switchTo(targetId);
    }
  }

  /**
   * 跳到第 N 个 Tab（1-indexed，issue #5 快捷键 Ctrl+1..9 用）。
   * N 大于现有 Tab 数 → 静默忽略；N 对应 Tab 已经 active → 无操作。
   */
  jumpToIndex(oneBasedIdx: number): void {
    const ids = this.orderedIds;
    if (oneBasedIdx < 1 || oneBasedIdx > ids.length) return;
    const targetId = ids[oneBasedIdx - 1];
    if (targetId && targetId !== this.activeId) {
      this.switchTo(targetId);
    }
  }

  /**
   * issue #11: 后端 `task-update` 事件路由——总是更新内存 map（即使 Tab 还没建），
   * 只有 sid 是当前 active 时才推全局 panel 重渲染。
   *
   * 不需要 "Tab 不存在就丢弃"——task 文件先于 jsonl 出现是合法时序，
   * 之后 ensureTab 时会从 tasksBySid 拿数据；fetchSessionTasks 拿到的也是同样数据。
   */
  updateTasks(sessionId: string, tasks: TaskEntry[]): void {
    this.tasksBySid.set(sessionId, tasks);
    if (this.activeId === sessionId) {
      this.tasksPanel?.setSession(sessionId, tasks);
    }
  }

  /**
   * v2.4 issue #2：把 behavior config 应用到 TabManager。
   * 启动时由 main.ts 调一次拉初值；设置面板 toggle 改了也调一次同步。
   */
  applyBehavior(cfg: BehaviorConfig): void {
    this.autoFollowUserActive = cfg.autoFollowUserActive;
    this.bringMonitorToFront = cfg.bringMonitorToFrontOnUserActive;
  }

  /**
   * v2.4 issue #2：watcher 反推识别到"用户在终端真敲了一行回车"（type=user
   * 且不是 tool_result 回灌 / CLI noise，由 tabs.onLine 的 result.kind 判定）
   * 时调用本方法。
   *
   * **跳过条件**（任一命中就 silently no-op）：
   * 1. autoFollowUserActive=false（设置面板关了）
   * 2. manualOverrideUntil > now（用户 5s 内手动点过 tab，明确意图保护）
   * 3. sid 不存在 / 已 archive（防御）
   * 4. sid 已经是 active（无操作）
   *
   * 通过后调 switchTo(sid, "auto")，可选 invoke bring_monitor_to_front。
   */
  userActive(sessionId: string): void {
    // v2.6 修回归：B 重构后 render-stream-record 删了 source="live" 过滤参数，
    // chunked replay batch 期间的历史 user 消息会触发本方法 → 反复自动切 tab。
    // 在这里加 inBatch 守卫等价 v2.5 的 source==="live" 检查。
    if (this.inBatch) return;
    if (!this.autoFollowUserActive) return;
    if (Date.now() < this.manualOverrideUntil) return;
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status === "archived") return;
    if (this.activeId === sessionId) {
      // 已经在这个 tab 但用户开了"拉前 monitor"也照拉
      if (this.bringMonitorToFront) {
        void invoke("bring_monitor_to_front").catch((e) => {
          console.warn("bring_monitor_to_front failed:", e);
        });
      }
      return;
    }
    this.switchTo(sessionId, "auto");
    if (this.bringMonitorToFront) {
      void invoke("bring_monitor_to_front").catch((e) => {
        console.warn("bring_monitor_to_front failed:", e);
      });
    }
  }

  /** 快捷键 Ctrl+W：当前活跃 Tab 是 archived 才关，live 不动 */
  closeActiveIfArchived(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (tab && tab.status === "archived") {
      this.closeTab(this.activeId);
    }
  }

  /** 快捷键 Ctrl+` ：把当前活跃 Tab 对应的终端窗口拉到前台（live 本地 / 远端均可） */
  bringActiveTerminalToFront(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (!tab || tab.status === "archived") return;
    // Feature ②：远端 Tab → 后端按 ccm-rbind HWND 缓存拉前；本地 Tab → 原 sid_hwnd_cache 路径。
    if (tab.origin !== null) {
      void bringRemoteTerminalToFront(this.activeId);
    } else {
      void bringTerminalToFront(this.activeId);
    }
  }

  /** 快捷键 Ctrl+Shift+E：打开当前活跃 Tab 的工作目录到系统文件管理器 */
  openActiveTabCwd(): void {
    if (!this.activeId) return;
    void this.openTabCwd(this.activeId);
  }

  /** issue #10 快捷键 Ctrl+Shift+N：把当前活跃 Tab 在独立只读窗口打开 */
  openActiveInNewWindow(): void {
    if (this.activeId) void this.openInNewWindow(this.activeId);
  }

  /**
   * issue #10：在独立只读窗口打开指定 session（Tab 右键 / 快捷键 / 拖拽撕离）。
   *
   * `screenX` / `screenY`（可选）= 拖拽撕离的落点屏幕坐标（来自 mouseup 的
   * `e.screenX/screenY`）。两者都给出时透传给后端在该处摆放新窗口；右键 / 快捷键
   * 不传则后端走默认居中。
   */
  private async openInNewWindow(
    sid: string,
    screenX?: number,
    screenY?: number,
  ): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab) return;
    try {
      await invoke("open_session_in_new_window", {
        sessionId: sid,
        title: tab.title,
        ...(screenX !== undefined && screenY !== undefined
          ? { x: screenX, y: screenY }
          : {}),
      });
    } catch (e) {
      showActionFailureToast("打开新窗口失败", String(e));
    }
  }

  /**
   * Tab 撕离拖拽起点（左键 mousedown）。只是"候选"：记录起点 + 挂 document 级
   * mousemove/mouseup，等指针越过阈值才真正进入拖拽。子动作按钮（📂/↗/×）的
   * mousedown 已 stopPropagation，不会走到这里。
   */
  private beginTabDrag(e: MouseEvent, sid: string, root: HTMLElement): void {
    // 已有拖拽在进行（理论上不会，因 mouseup 会清）—— 防御性忽略。
    if (this.drag) return;
    // 新一轮交互开始：清掉可能残留的抑制标记，避免陈旧 flag 误吞下次 click。
    this.suppressClickSid = null;

    const barRight = this.barEl.getBoundingClientRect().right;
    const onMove = (ev: MouseEvent): void => this.onDragMove(ev);
    const onUp = (ev: MouseEvent): void => this.onDragUp(ev);
    this.drag = {
      sid,
      startX: e.clientX,
      startY: e.clientY,
      barRight,
      root,
      dragging: false,
      armed: false,
      ghost: null,
      onMove,
      onUp,
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  /** document mousemove：阈值判定 → 起拖（建 ghost / 变暗），随后跟随 + arm 检测。 */
  private onDragMove(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;

    // 容错：主键已松开（mouseup 在窗口外丢失，比如拖到别的 app 上释放）→ 收尾取消，
    // 不弹窗（落点不可信），避免 ghost 残留 + 拖拽状态卡死。下次按下会重新开始。
    if ((e.buttons & 1) === 0) {
      const wasDragging = d.dragging;
      const sid = d.sid;
      this.teardownDrag();
      if (wasDragging) this.suppressClickSid = sid;
      return;
    }

    if (!d.dragging) {
      const dx = e.clientX - d.startX;
      const dy = e.clientY - d.startY;
      if (Math.hypot(dx, dy) <= TabManager.DRAG_THRESHOLD_PX) return;
      // 越过阈值 → 正式起拖：阻止文本选区、建 ghost、源 Tab 变暗。
      e.preventDefault();
      d.dragging = true;
      d.root.classList.add("dragging");
      const ghost = document.createElement("div");
      ghost.className = "tab-drag-ghost";
      ghost.textContent = this.tabs.get(d.sid)?.title ?? "";
      document.body.appendChild(ghost);
      d.ghost = ghost;
    }

    // 跟随光标（偏右下避免压在指针正下方）。
    if (d.ghost) {
      d.ghost.style.left = `${e.clientX + 8}px`;
      d.ghost.style.top = `${e.clientY + 8}px`;
    }

    // arm：指针拖离竖栏右缘一段距离 = 松手即弹独立窗口（F33 前是下缘判定）。
    const armed = e.clientX > d.barRight + 16;
    if (armed !== d.armed) {
      d.armed = armed;
      if (d.ghost) {
        d.ghost.classList.toggle("armed", armed);
        d.ghost.textContent = armed
          ? "松开 → 独立窗口"
          : (this.tabs.get(d.sid)?.title ?? "");
      }
    }
  }

  /** 收尾：拆 document listener、清 ghost / 源 Tab 变暗、清空拖拽状态。 */
  private teardownDrag(): void {
    const d = this.drag;
    if (!d) return;
    document.removeEventListener("mousemove", d.onMove);
    document.removeEventListener("mouseup", d.onUp);
    d.ghost?.remove();
    d.root.classList.remove("dragging");
    this.drag = null;
  }

  /** document mouseup：收尾；armed 则在落点弹独立窗口。 */
  private onDragUp(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;
    const { dragging, armed, sid } = d;
    this.teardownDrag();

    if (!dragging) return; // 没越阈值 = 纯点击，交给 click handler 正常切 Tab。

    // 起过拖（无论 armed 与否）都抑制紧随的 click —— 拖完不该顺带切 Tab。
    this.suppressClickSid = sid;
    if (armed) {
      void this.openInNewWindow(sid, e.screenX, e.screenY);
    }
  }

  /**
   * F37：手动 resume 一个已结束（灰）的 Tab。与历史浏览器 ↺ 同一套语义：
   * 本地 → 新终端窗口跑 resume（尊重 F34 自定义命令，缺省 cc 检测→claude）；
   * 远端 → F41 一键拉起 wt.exe/PowerShell 跑 `ssh -t …`，失败回退复制命令。
   * resume 成功后 CC 续写同一 jsonl，既有「会话复活」路径会自动把灰 Tab 点亮。
   */
  private async resumeTab(sid: string): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab) return;
    const behavior = await getBehavior();
    if (tab.origin !== null) {
      await runRemoteResume(tab.origin, sid, tab.cwd ?? "", behavior.resumeCommandRemote);
      return;
    }
    try {
      await invoke("resume_history_session", {
        sessionId: sid,
        cwd: tab.cwd ?? "",
        launcher: behavior.resumeCommandLocal || null,
      });
    } catch (err) {
      showActionFailureToast("恢复失败", String(err));
    }
  }

  /**
   * F52：tmux 版 resume（远端专用）——在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude。
   * 与 resumeTab 的直连版并列;本地 tab（origin===null）无 tmux 用例,直接 return。
   */
  private async resumeTabTmux(sid: string): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab || tab.origin === null) return;
    const behavior = await getBehavior();
    await runRemoteResumeTmux(tab.origin, sid, tab.cwd ?? "", behavior.resumeCommandRemote);
  }

  /**
   * F51：菜单打开后异步反查 tmux——查该 origin 的会话列表(短缓存),按 `path===cwd &&
   * command==="claude"` 反查该 tab 的 Claude 所在 tmux 会话。命中 → 把禁用占位「检测中」
   * 换成可点的 Attach;无 tmux / 无匹配 / 查询失败 → 移除占位。菜单已关则 update/remove no-op。
   */
  private async resolveAttachMenuItem(origin: string, cwd: string): Promise<void> {
    const gen = tabMenuGeneration; // 捕获发起查询的那一代菜单(R-1 守卫)
    let sessions: TmuxSession[] | null;
    try {
      sessions = await invoke<TmuxSession[] | null>("list_remote_tmux", { origin });
      // 只缓存确定结果(成功列表 / NO_TMUX=null);瞬时 ssh 失败不缓存,免 8s 内抑制重试(D-Sug3)。
      this.tmuxCache.set(origin, { ts: Date.now(), sessions });
    } catch {
      // 查询失败(纯 ssh exec 抖动)→ 移除占位,不缓存。
      if (gen === tabMenuGeneration) removeTabContextMenuItem("attach");
      return;
    }
    // 菜单已换/已关(新代次)→ 别动别的菜单(R-1 跨 tab 串味)。
    if (gen !== tabMenuGeneration) return;
    const match = sessions?.find((s) => s.path === cwd && isClaudeTmuxCommand(s.command));
    if (match) {
      updateTabContextMenuItem("attach", {
        id: "attach",
        label: `Attach（tmux: ${match.name}）`,
        onClick: () => void runRemoteAttach(origin, match.name),
      });
    } else {
      removeTabContextMenuItem("attach");
    }
  }

  /** 打开指定 Tab 的 cwd 到系统文件管理器。无 cwd / 远端 Tab 静默忽略。 */
  private async openTabCwd(sid: string): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab?.cwd) return;
    // FIX 5：远端 Tab 的 cwd 是远端路径，本地 openPath 必然失败。Batch9-F29：
    // 从静默 no-op 改为提示（盘点 #9：用户按 E 没反应像坏了）。
    if (tab.origin !== null) {
      showActionFailureToast(
        "远端目录无法本地打开",
        `该会话在远端机器 [${tab.origin}]，工作目录 ${tab.cwd} 不在本机。`,
        { level: "info" },
      );
      return;
    }
    try {
      await openPath(tab.cwd);
    } catch (e) {
      console.warn(`[tabs] openPath ${tab.cwd} failed:`, e);
    }
  }

  /**
   * 切到目标 Tab。
   *
   * v2.4 issue #2：`source` 区分用户主动 vs 自动跟随。
   * - `"manual"`（默认）：Tab Bar 点击 / Ctrl+Tab / 中键关 / 内部 fallback 切。
   *   设置 manualOverrideUntil = now+5s，期间拒绝 userActive 自动切。
   * - `"auto"`：watcher 反推 user-active 触发的自动切。不更新 override，
   *   不互相锁死（不然 auto 调 switchTo 又设 override 自己就被锁了）。
   */
  switchTo(sessionId: string, source: "manual" | "auto" = "manual"): void {
    if (!this.tabs.has(sessionId)) return;
    if (this.activeId === sessionId) return;

    // 切 active 走 .active class（CSS visibility 控制），避免 display:none/block
    // 触发整棵子树重建 layout tree 卡顿。详 styles.css 的 .stream 注释。
    for (const [sid, t] of this.tabs) {
      t.streamEl.classList.toggle("active", sid === sessionId);
    }
    const next = this.tabs.get(sessionId);
    if (next) next.unread = 0;
    this.activeId = sessionId;
    // Batch13-F40a:命中 virgin tab(启动重放全收纳,还没建过卡)→ 同步物化尾段,
    // 避免切过去一片空白(R-3:有界循环补到可滚,防工具密集会话一轮近空屏)。
    // 非 virgin tab 不动(上翻补批属 F40b)。
    if (next && next.window.floorSeq === null && next.window.pendingCount > 0) {
      this.materializeUntilFilled(next);
    }
    // F40b:切入即刷新哨兵(非 virgin 但账本非空的 tab 也要见到「还有 N 条」)
    if (next) this.updateSentinel(next);
    // D 审计 R-2:非 virgin + 不可滚 + 账本有余的 tab 没有 fill 入口(不可滚元素
    // 不产生 scroll 事件,哨兵可见却"上翻物理不可达")——切入时踢一次,rAF 自链
    // 接管直到可滚或账尽。
    if (next && next.window.pendingCount > 0) {
      const el = next.streamEl;
      if (el.scrollHeight - el.clientHeight <= 1) this.fillAbove(next);
    }
    // Batch5-F19：记住所在 tab——下次启动 active 选择 + replay 优先该 session。
    // viewer/tear-off 窗口共享同 origin 的 localStorage（INVARIANT § 14），它们的
    // TabManager 置 persistLastActive=false，防独立窗口看会话 X 污染主窗口记忆。
    if (this.persistLastActive) {
      safeSet(LS_KEYS.lastActiveSid, sessionId);
    }
    if (source === "manual") {
      this.manualOverrideUntil = Date.now() + TabManager.MANUAL_OVERRIDE_MS;
      this.onManualSwitch?.();
    }
    this.refreshTabBar(); // active 高亮 + badge 立即更新（廉价，不阻塞）

    // 切 Tab 卡顿优化：把会**强制同步 reflow** 的 scrollToBottom（读 scrollHeight）+
    // 面板整表 re-render 推到下一帧——让 .active 的 visibility 切换先绘制出来（切 Tab 即时
    // 跟手），重活下一帧再做。期间又切走则跳过（不把面板/滚动落到已非 active 的会话上）。
    requestAnimationFrame(() => {
      if (this.activeId !== sessionId) return;
      next?.stream.scrollToBottom();
      // issue #11: 切换 task panel 数据源到新 active Tab 的 sid
      this.tasksPanel?.setSession(sessionId, this.tasksBySid.get(sessionId) ?? []);
      // issue #23: agents 面板同步切到新 active Tab
      this.agentsPanel?.setSession(
        sessionId,
        [...(this.tabs.get(sessionId)?.agents.values() ?? [])],
      );
    });
  }

  /**
   * 局部更新策略（避免每次 onLine 都 replaceChildren）：
   *   1. 删除：tabButtons 缓存里有但 orderedIds 已没的 sid → 摘 DOM + 清缓存
   *   2. 创建：orderedIds 里有但缓存没的 sid → createTabButton 一次（含所有 5 个子
   *      元素 + 事件 listener），visibility 全交 CSS 控制
   *   3. 更新：updateTabButton 同步 active / archived / has-unread class + label/badge 文本
   *   4. 排序：iterate orderedIds + insertBefore，确保 DOM 顺序 = orderedIds 顺序
   *
   * CSS 配合（styles.css）：
   *   .tab.archived .live-dot { display: none }
   *   .tab:not(.archived) .tab-close { display: none }
   *   .tab .tab-badge { display: none }
   *   .tab.has-unread:not(.active) .tab-badge { display: inline-block }
   */
  private refreshTabBar(): void {
    // 1. 删
    const wanted = new Set(this.orderedIds);
    for (const sid of Array.from(this.tabButtons.keys())) {
      if (!wanted.has(sid)) {
        const refs = this.tabButtons.get(sid)!;
        refs.root.remove();
        this.tabButtons.delete(sid);
      }
    }

    // 2 + 3 + 4. 创建 / 更新 / 排序
    let prev: ChildNode | null = null;
    for (const sid of this.orderedIds) {
      const tab = this.tabs.get(sid);
      if (!tab) continue;
      let refs = this.tabButtons.get(sid);
      if (!refs) {
        refs = this.createTabButton(sid);
        this.tabButtons.set(sid, refs);
      }
      this.updateTabButton(refs, sid, tab);
      // 排序：希望此 button 出现在 prev 之后
      const targetNext: ChildNode | null = prev
        ? prev.nextSibling
        : this.barEl.firstChild;
      if (refs.root !== targetNext) {
        this.barEl.insertBefore(refs.root, targetNext);
      }
      prev = refs.root;
    }

    this.notifyChanged();
  }

  private createTabButton(sid: string): TabButtonRefs {
    const root = document.createElement("button");
    root.className = "tab";

    const dot = document.createElement("span");
    dot.className = "live-dot";
    root.appendChild(dot);

    const label = document.createElement("span");
    label.className = "tab-title";
    root.appendChild(label);

    const badge = document.createElement("span");
    badge.className = "tab-badge";
    root.appendChild(badge);

    // 📂 打开工作目录（cwd）—— 系统默认文件管理器
    const cwdBtn = document.createElement("span");
    cwdBtn.className = "tab-cwd";
    cwdBtn.textContent = "📂";
    cwdBtn.title = "打开工作目录 (E)";
    cwdBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void this.openTabCwd(sid);
    });
    // 子动作按钮自己处理点击：吞掉 mousedown 避免在它们身上起 Tab 拖拽。
    cwdBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(cwdBtn);

    // ↗ 拉对应终端窗口（v1.7 用 sid_hwnd_cache）
    const focusBtn = document.createElement("span");
    focusBtn.className = "tab-focus";
    focusBtn.textContent = "↗";
    focusBtn.title = "调出对应终端 (`)";
    focusBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const t = this.tabs.get(sid);
      if (!t || t.status === "archived") return;
      // Feature ②：远端 Tab → 走后端按 ccm-rbind 标题缓存的 HWND 拉前（未绑定则 toast no-op）；
      // 本地 Tab → 走原 sid_hwnd_cache 路径。
      if (t.origin !== null) {
        void bringRemoteTerminalToFront(sid);
      } else {
        void bringTerminalToFront(sid);
      }
    });
    focusBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(focusBtn);

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭 Tab";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      this.closeTab(sid);
    });
    closeBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(closeBtn);

    root.addEventListener("click", () => {
      // 拖拽刚结束的那次 click 不切 Tab（drag-then-release ≠ 选中）。一次性消费。
      if (this.suppressClickSid === sid) {
        this.suppressClickSid = null;
        return;
      }
      this.switchTo(sid);
    });
    // 左键 mousedown：候选 Tab 撕离拖拽（越过阈值才真拖，否则仍是普通 click）。
    root.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      this.beginTabDrag(e, sid, root);
    });
    // 中键点击归档 Tab 也关闭（常见 UX）
    root.addEventListener("mousedown", (e) => {
      if (e.button !== 1) return;
      const t = this.tabs.get(sid);
      if (t?.status === "archived") {
        e.preventDefault();
        this.closeTab(sid);
      }
    });
    // issue #10：右键菜单「在新窗口打开」（双屏 / 并排）
    root.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      const t = this.tabs.get(sid);
      const items: TabMenuItem[] = [
        { label: "在新窗口打开", onClick: () => void this.openInNewWindow(sid) },
      ];
      // F37：灰 tab（会话已结束）右键手动 resume——不用绕去历史浏览器。
      // F41 起本地与远端都是一键拉起新终端（远端=wt.exe 跑 ssh -t，失败才回退复制命令）。
      // F52：远端归档 tab 再并列一个「Resume（tmux）」——在远端 tmux 会话里幂等 resume,
      // resume 完人在 tmux 里(断线可 F51 attach 回来);本地归档仍单「Resume」。
      if (t?.status === "archived") {
        if (t.origin !== null) {
          items.push({
            label: "Resume（直连）",
            onClick: () => void this.resumeTab(sid),
          });
          items.push({
            label: "Resume（tmux）",
            onClick: () => void this.resumeTabTmux(sid),
          });
        } else {
          items.push({
            label: "Resume",
            onClick: () => void this.resumeTab(sid),
          });
        }
      }
      // F51：远端 tab（有 cwd）——反查该 cwd 正跑 claude 的 tmux 会话 → Attach。
      // 缓存命中同步定夺(无占位闪烁);未命中先禁用占位「检测中」+ 异步查询就绪。
      const origin = t?.origin ?? null;
      const cwd = t?.cwd ?? null;
      let needAsyncAttach = false;
      if (origin !== null && cwd) {
        const cached = this.tmuxCache.get(origin);
        if (cached && Date.now() - cached.ts < TMUX_CACHE_TTL_MS) {
          const m = cached.sessions?.find(
            (s) => s.path === cwd && isClaudeTmuxCommand(s.command),
          );
          if (m) {
            items.push({
              id: "attach",
              label: `Attach（tmux: ${m.name}）`,
              onClick: () => void runRemoteAttach(origin, m.name),
            });
          }
        } else {
          items.push({
            id: "attach",
            label: "Attach（检测 tmux…）",
            enabled: false,
            onClick: () => {},
          });
          needAsyncAttach = true;
        }
      }
      showTabContextMenu(e.clientX, e.clientY, items);
      if (needAsyncAttach && origin !== null && cwd) {
        void this.resolveAttachMenuItem(origin, cwd);
      }
    });

    return { root, label, badge, cwdBtn };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    refs.root.classList.toggle("active", sid === this.activeId);
    refs.root.classList.toggle("archived", tab.status === "archived");
    refs.root.classList.toggle("has-cwd", !!tab.cwd);
    // FIX 5 / Feature ②（issue #15）：远端 Tab（origin 非 null）的 cwd 是 Pi 上的路径，
    // 本地不存在，故 .remote 类只隐藏「打开工作目录」📂（CSS）。「调出终端」↗ 现在保留
    // 给远端 —— 点击走 bringRemoteTerminalToFront（后端按 ccm-rbind 拉本地 ssh 窗口）。
    refs.root.classList.toggle("remote", tab.origin !== null);
    // Batch7-F24：bg 任务 tab——缩进 + ⌞ 前缀由 CSS 承担
    refs.root.classList.toggle("tab-bg", tab.kind !== null && tab.kind !== "interactive");
    // issue #23 红绿灯：busy=绿（.live-dot 默认色）/ idle·shell=红 / waiting=黄。
    // activity 为 null（旧版 CC / 远端 v1）不加类 → 维持现状绿点。
    const actStatus = tab.activity?.status ?? null;
    refs.root.classList.toggle(
      "act-idle",
      actStatus === "idle" || actStatus === "shell",
    );
    refs.root.classList.toggle("act-waiting", actStatus === "waiting");
    refs.root.title =
      actStatus === "waiting" && tab.activity?.waitingFor
        ? `等待操作：${tab.activity.waitingFor}`
        : "";
    const unread = tab.unread > 0 && sid !== this.activeId;
    refs.root.classList.toggle("has-unread", unread);

    if (refs.label.textContent !== tab.title) {
      refs.label.textContent = tab.title;
    }
    if (unread) {
      const text = tab.unread > 99 ? "99+" : String(tab.unread);
      if (refs.badge.textContent !== text) {
        refs.badge.textContent = text;
      }
    }
  }
}

/**
 * issue #10：极简一次性上下文菜单（Tab 右键用）。挂 document.body 作 fixed 浮层，
 * 点任意项 / 点外部 / Esc 即关。一次只允许一个（开新的前先关旧的）。
 *
 * B14-F51：升级为 action 注册表 lite——项带可选 `id`/`enabled`,并可对**已打开**菜单按 id
 * `update`/`remove`(承载异步就绪项,如 attach 的 tmux 反查回来才可点）。
 */
interface TabMenuItem {
  id?: string;
  label: string;
  enabled?: boolean; // 缺省 true;false = 禁用占位
  onClick: () => void;
}
let activeTabMenu: HTMLElement | null = null;
const activeTabMenuItems = new Map<string, HTMLButtonElement>();
/** F51：菜单代次令牌——每次开/关菜单自增。在飞的异步就绪(attach 反查)回来时比对代次,
 * 只作用于发起它的那一代菜单;换/关菜单后旧查询整体 no-op(防 R-1 跨 tab 串味错配)。 */
let tabMenuGeneration = 0;

function makeTabMenuButton(it: TabMenuItem): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "tab-context-menu-item";
  btn.textContent = it.label;
  const enabled = it.enabled !== false;
  btn.disabled = !enabled;
  if (enabled) {
    btn.addEventListener("click", () => {
      closeTabContextMenu();
      it.onClick();
    });
  }
  return btn;
}

function showTabContextMenu(x: number, y: number, items: TabMenuItem[]): void {
  closeTabContextMenu();
  const menu = document.createElement("div");
  menu.className = "tab-context-menu";
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  for (const it of items) {
    const btn = makeTabMenuButton(it);
    if (it.id) activeTabMenuItems.set(it.id, btn);
    menu.appendChild(btn);
  }
  document.body.appendChild(menu);
  activeTabMenu = menu;
  tabMenuGeneration++; // 新一代菜单 → 让上一代在飞的异步就绪回调失效
  // 下一拍再挂关闭监听，避免本次右键触发的事件立刻把菜单关掉
  window.setTimeout(() => {
    window.addEventListener("pointerdown", onDocPointerForMenu, true);
    window.addEventListener("keydown", onKeyForMenu, true);
  }, 0);
}

/** F51：把已打开菜单里某 id 项替换为新项(异步就绪→可点);菜单已关或无此 id 则 no-op。 */
function updateTabContextMenuItem(id: string, item: TabMenuItem): void {
  const old = activeTabMenuItems.get(id);
  if (!old || !activeTabMenu) return;
  const btn = makeTabMenuButton(item);
  activeTabMenuItems.set(item.id ?? id, btn);
  old.replaceWith(btn);
}

/** F51：移除已打开菜单里某 id 项(异步查无匹配);无此 id 则 no-op。 */
function removeTabContextMenuItem(id: string): void {
  const old = activeTabMenuItems.get(id);
  if (!old) return;
  old.remove();
  activeTabMenuItems.delete(id);
}

function closeTabContextMenu(): void {
  if (!activeTabMenu) return;
  activeTabMenu.remove();
  activeTabMenu = null;
  activeTabMenuItems.clear();
  tabMenuGeneration++; // 关菜单也让在飞的异步就绪回调失效(不改别的菜单)
  window.removeEventListener("pointerdown", onDocPointerForMenu, true);
  window.removeEventListener("keydown", onKeyForMenu, true);
}
function onDocPointerForMenu(e: PointerEvent): void {
  if (activeTabMenu && !activeTabMenu.contains(e.target as Node)) {
    closeTabContextMenu();
  }
}
function onKeyForMenu(e: KeyboardEvent): void {
  if (e.key === "Escape") closeTabContextMenu();
}

function projectNameFromCwd(cwd: string): string | null {
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}

/**
 * 标题格式（决策见 project_monitor_decisions.md）：
 *   aiTitle 有 + cwd 有 → `[项目] aiTitle`
 *   aiTitle 有 + cwd 无 → `aiTitle`
 *   aiTitle 无 + cwd 有 → `项目`
 *   都没有 → `<sid 前 8 位>`
 *
 * issue #15：`origin`（远端 SSH 主机名）非空时，在以上结果前再加 `[origin] ` 前缀，
 * 让用户一眼区分本地 / 远端 Tab（如 `[raspberrypi.local] [proj] aiTitle`）。本地
 * （origin=null）行为与历史完全一致，不加任何前缀。
 *
 * Subagent 不再独立 Tab（嵌入到父 session 的 Task 折叠卡），所以没有 `↳` 前缀分支。
 */
function computeTitleFor(
  sessionId: string,
  cwd: string | null,
  aiTitle: string | null,
  origin: string | null = null,
  kind: string | null = null,
  bgName: string | null = null,
): string {
  const project = cwd ? projectNameFromCwd(cwd) : null;
  // Batch7-F24：bg 任务 → ⚙ + 任务名（缩进/⌞ 由 .tab-bg 样式承担）
  if (kind !== null && kind !== "interactive") {
    const base = `⚙ ${bgName ?? aiTitle ?? project ?? sessionId.slice(0, 8)}`;
    return origin ? `[${origin}] ${base}` : base;
  }
  let base: string;
  if (aiTitle) {
    base = project ? `[${project}] ${aiTitle}` : aiTitle;
  } else if (project) {
    base = project;
  } else {
    base = sessionId.slice(0, 8);
  }
  return origin ? `[${origin}] ${base}` : base;
}

// P5.2 B 重构：markCardUuid + feedBranchFolder 已搬到 render-stream-record.ts
// （三 caller 共用 renderStreamRecord 函数内部调用）。tabs.ts 不再持有这两个 helper。

/**
 * 拉对应终端到前台。v1.7 实现：后端查 sid_hwnd_cache + 复合指纹校验 + SetForegroundWindow。
 *
 * 失败模式（任一都会显示在 toast 上）：
 *   - "未绑定窗口"：该 session 启动时没经过 cc function 握手（直接跑 claude 而非 cc）
 *   - "窗口已不存在"：用户关掉了对应 PS/WT 窗口
 *   - "HWND 复用"：原窗口关闭后 HWND 被另一个无关窗口拿到
 *   - "invoke 超时"：极端情况下 Win32 调用卡住
 */
function bringTerminalToFront(sessionId: string): Promise<void> {
  const timeoutMs = 5000;
  return Promise.race([
    invoke<void>("bring_terminal_to_front", { sessionId }),
    new Promise<never>((_, reject) =>
      window.setTimeout(
        () => reject(new Error(`invoke 超时 ${timeoutMs}ms（后端 Win32 调用可能卡住）`)),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_terminal_to_front ${sessionId} failed:`, e);
    // P4.5: 改走统一 toast stack（去掉单例 #bring-terminal-toast 的"先到先被覆盖"问题）。
    showActionFailureToast("拉前失败", String(e?.message ?? e));
  });
}

/**
 * Feature ②：拉远端 Tab 对应的本地 ssh 窗口到前台。后端按 `ccm-rbind-<sid>` 窗口标题
 * 标记缓存的 HWND + SetForegroundWindow。
 *
 * 失败模式（任一都会显示在 toast 上）：
 *   - "未绑定窗口…"：远端 session 没经过 ccm wrapper 握手（直接跑 claude 而非 ccm），
 *     后端找不到 ccm-rbind 标记的窗口。设置面板「远端 ↗ 拉前」里有 ccm 函数贴远端。
 *   - "窗口已不存在"：用户关掉了对应 ssh 窗口
 *   - "invoke 超时"：极端情况下 Win32 调用卡住
 */
function bringRemoteTerminalToFront(sessionId: string): Promise<void> {
  const timeoutMs = 5000;
  return Promise.race([
    invoke<void>("bring_remote_terminal_to_front", { sessionId }),
    new Promise<never>((_, reject) =>
      window.setTimeout(
        () => reject(new Error(`invoke 超时 ${timeoutMs}ms（后端 Win32 调用可能卡住）`)),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_remote_terminal_to_front ${sessionId} failed:`, e);
    showActionFailureToast("拉前失败", String(e?.message ?? e));
  });
}

