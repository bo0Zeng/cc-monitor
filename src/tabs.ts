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
import { renderStreamRecord, type StreamSink } from "./render-stream-record";
import type { BranchRecord } from "./branching";

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
  status: TabStatus;
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

export class TabManager {
  private tabs = new Map<string, Tab>();
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

  constructor(
    private barEl: HTMLElement,
    private streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI */
    private onTabsChanged?: (summary: TabsSummary) => void,
    /** issue #11: 全局 TasksPanel，切 Tab / 收事件时由 TabManager 喂数据 */
    private tasksPanel?: TasksPanel,
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
      // 重放期"视口上方"的旧消息延后批量挂载，消除逐帧上方插入造成的微抖。
      t.timeline.setDeferMode(true);
    }
  }

  /**
   * v2.2 (issue #12): 重放批次完结。各 Tab 调 flushPending 一次性算 + rebuild，
   * 然后切回 live 模式。后续真实时新消息按 timeline 路径走。
   */
  onBatchEnd(): void {
    this.inBatch = false;
    for (const t of this.tabs.values()) {
      // 先把延后的"视口上方"旧消息批量挂回 DOM —— 必须在 branchFolder.flushPending
      // 之前（后者要扫完整 DOM 算主线/折叠）。
      t.timeline.flushDeferred();
      t.branchFolder.flushPending();
      t.branchFolder.setBatchMode(false);
      // 切块场景下，老块的 tool_use 现在已渲染 → 重试匹配早到的 fallback result
      const ctx: RenderContext = {
        parentPath: t.parentPath,
        toolUseNames: t.toolUseNames,
        toolUseElements: t.toolUseElements,
        pendingToolResults: t.pendingToolResults,
      };
      reconcilePendingToolResults(ctx);
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
    const tab = this.ensureTab(payload.session_id, payload.cwd, payload.path, payload.seq);

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      pendingToolResults: tab.pendingToolResults,
      // P5.5：batch 期间走 lazy hljs（代码块占位 + IntersectionObserver 触发再补跑）
      lazy: this.inBatch,
    };
    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: (rec: BranchRecord) => tab.branchFolder.recordAdded(rec),
      onTitleUpdate: (title: string) => this.applyAiTitle(tab, title),
      onRealUserInput: (sid: string) => this.userActive(sid),
      observeForLazyEnhance: this.inBatch,
    };

    const beforeSize = tab.timeline.size;
    renderStreamRecord(payload, ctx, sink);
    const inserted = tab.timeline.size > beforeSize;

    // unread 计数：只有真新 entry 入 timeline 才算（tool-group 合并到旧 group 不算）
    if (inserted && this.activeId !== tab.sessionId) {
      tab.unread += 1;
      this.refreshTabBar();
    }
  }

  ensureTab(sessionId: string, cwd: string | null, sourcePath: string, seq: number): Tab {
    let tab = this.tabs.get(sessionId);
    if (tab) {
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

    const title = computeTitleFor(sessionId, cwd, null);

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
      timeline.setDeferMode(true);
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
      title,
      cwd,
      // 记下当前 cwd 来源的 seq；后续更早（更小 seq）的记录可覆盖（取项目根）。
      cwdSeq: cwd ? seq : Number.POSITIVE_INFINITY,
      aiTitle: null,
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
    };
    this.tabs.set(sessionId, tab);
    this.orderedIds.push(sessionId);

    if (this.activeId === null) {
      this.switchTo(sessionId);
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

  /** 根据 tab.cwd + tab.aiTitle + sessionId 算出展示标题 */
  private computeTitle(tab: Tab): string {
    return computeTitleFor(tab.sessionId, tab.cwd, tab.aiTitle);
  }

  /** session 退出（~/.claude/sessions/<PID>.json 被删）—— 灰显归档，内容保留 */
  archiveTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status === "archived") return;
    tab.status = "archived";
    // P5.2 B 重构后无 pendingToolGroup —— archive 不需要打断 tool-group 累积
    // （tool-group 合并改后处理，看 timeline 邻居；archive 后无新 record 入 timeline）。
    this.refreshTabBar();
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

  /** 快捷键 Ctrl+` ：把当前活跃 Tab 对应的终端窗口拉到前台（仅 live） */
  bringActiveTerminalToFront(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (tab && tab.status !== "archived") {
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

  /** issue #10：在独立只读窗口打开指定 session（Tab 右键 / 快捷键）。 */
  private async openInNewWindow(sid: string): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab) return;
    try {
      await invoke("open_session_in_new_window", {
        sessionId: sid,
        title: tab.title,
      });
    } catch (e) {
      showActionFailureToast("打开新窗口失败", String(e));
    }
  }

  /** 打开指定 Tab 的 cwd 到系统文件管理器。无 cwd 静默忽略。 */
  private async openTabCwd(sid: string): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab?.cwd) return;
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
    if (next) {
      next.unread = 0;
      next.stream.scrollToBottom();
    }
    this.activeId = sessionId;
    if (source === "manual") {
      this.manualOverrideUntil = Date.now() + TabManager.MANUAL_OVERRIDE_MS;
    }
    // issue #11: 切换 task panel 数据源到新 active Tab 的 sid
    this.tasksPanel?.setSession(sessionId, this.tasksBySid.get(sessionId) ?? []);
    this.refreshTabBar();
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
    cwdBtn.title = "打开工作目录 (Ctrl+Shift+E)";
    cwdBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void this.openTabCwd(sid);
    });
    root.appendChild(cwdBtn);

    // ↗ 拉对应终端窗口（v1.7 用 sid_hwnd_cache）
    const focusBtn = document.createElement("span");
    focusBtn.className = "tab-focus";
    focusBtn.textContent = "↗";
    focusBtn.title = "调出对应终端 (Ctrl+`)";
    focusBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void bringTerminalToFront(sid);
    });
    root.appendChild(focusBtn);

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭 Tab";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      this.closeTab(sid);
    });
    root.appendChild(closeBtn);

    root.addEventListener("click", () => this.switchTo(sid));
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
      showTabContextMenu(e.clientX, e.clientY, [
        { label: "在新窗口打开", onClick: () => void this.openInNewWindow(sid) },
      ]);
    });

    return { root, label, badge, cwdBtn };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    refs.root.classList.toggle("active", sid === this.activeId);
    refs.root.classList.toggle("archived", tab.status === "archived");
    refs.root.classList.toggle("has-cwd", !!tab.cwd);
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
 */
let activeTabMenu: HTMLElement | null = null;
function showTabContextMenu(
  x: number,
  y: number,
  items: { label: string; onClick: () => void }[],
): void {
  closeTabContextMenu();
  const menu = document.createElement("div");
  menu.className = "tab-context-menu";
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  for (const it of items) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "tab-context-menu-item";
    btn.textContent = it.label;
    btn.addEventListener("click", () => {
      closeTabContextMenu();
      it.onClick();
    });
    menu.appendChild(btn);
  }
  document.body.appendChild(menu);
  activeTabMenu = menu;
  // 下一拍再挂关闭监听，避免本次右键触发的事件立刻把菜单关掉
  window.setTimeout(() => {
    window.addEventListener("pointerdown", onDocPointerForMenu, true);
    window.addEventListener("keydown", onKeyForMenu, true);
  }, 0);
}
function closeTabContextMenu(): void {
  if (!activeTabMenu) return;
  activeTabMenu.remove();
  activeTabMenu = null;
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
 * Subagent 不再独立 Tab（嵌入到父 session 的 Task 折叠卡），所以没有 `↳` 前缀分支。
 */
function computeTitleFor(
  sessionId: string,
  cwd: string | null,
  aiTitle: string | null,
): string {
  const project = cwd ? projectNameFromCwd(cwd) : null;
  if (aiTitle) {
    return project ? `[${project}] ${aiTitle}` : aiTitle;
  }
  if (project) return project;
  return sessionId.slice(0, 8);
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

