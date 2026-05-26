import { invoke } from "@tauri-apps/api/core";
import { MessageStream } from "./stream";
import {
  renderMessage,
  buildToolGroup,
  addToToolGroup,
  reconcilePendingToolResults,
  type JsonlRecord,
  type ToolGroup,
  type RenderContext,
} from "./cards";
import { observeForEnhance, setRenderLazyMode } from "./render";
import { BranchFolder } from "./branch-fold";
import { extractBranchRecord } from "./branching";
import { fetchSessionTasks, type TaskEntry, type TasksPanel } from "./tasks-panel";
import type { PayloadSource } from "./events";
import type { BehaviorConfig } from "./behavior";

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
  /** Claude 给出的语义标题（JSONL 里 `ai-title` 记录的 aiTitle 字段），出现一次就锁定 */
  aiTitle: string | null;
  status: TabStatus;
  streamEl: HTMLElement;
  stream: MessageStream;
  /** 父 JSONL 路径（subagent 加载需要） */
  parentPath: string;
  unread: number;
  /** 当前正在累积的工具组（连续 tool-only assistant 消息），出现普通卡时清空 */
  pendingToolGroup: ToolGroup | null;
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
   * v2.3.1 (issue #1)：older chunk 累积 fragment 一次性 prepend 到 stream 顶部，
   * 避免每条 prepend 触发多次 layout（3920 条 × layout 是几秒级开销）。
   *
   * 每次 chunk 切换 / batch-end 时 flushPendingPrepend 把它 prepend 到 stream
   * **顶部**（contentEl.firstChild 前）。多 chunk 调用后 DOM 顺序自然是
   * [最老, ..., 次新, head 最新]。
   */
  pendingPrependFragment: DocumentFragment | null;
  /**
   * v2.3.1 (issue #1)：切块场景下 tool_result 可能在 tool_use 之前到达
   * （head 块含 result，older 块才有 tool_use）。先走 fallback 渲染独立卡，
   * 全部 chunks 完成后 reconcilePendingToolResults 重新匹配 + 注入。
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
   */
  private inBatch = false;
  /**
   * v2.3.1 (issue #1) 切块加速：当前是否在 "older chunk prepend 模式"。
   * - chunk 0 (head) 到达：append 模式（默认），渲染完后第一个 child 当 anchor
   * - chunk > 0 到达：prepend 模式，onLine 不直接 append 而是 buffer 到 fragment
   * - 每个 chunk 边界（next chunk 来 or batch-end）flush fragment 一次 insertBefore
   */
  private inPrependMode = false;
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
   * v2.3.1 (issue #1)：切块场景下 chunk 0 (head) 走 append 模式正常路径。
   */
  onBatchStart(): void {
    this.inBatch = true;
    this.inPrependMode = false;
    // v2.3.1 Phase 2：batch 期间开 lazy hljs（代码块占位，IntersectionObserver 触发再 enhance）
    setRenderLazyMode(true);
    for (const t of this.tabs.values()) {
      t.branchFolder.setBatchMode(true);
      // v2.3.1: chunk 边界打断 tool-group 累积，避免相邻 chunk 的 tool-only
      // assistant 被错误合并到同一个 group（它们时间上可能跨度大）
      t.pendingToolGroup = null;
    }
  }

  /**
   * v2.3.1 (issue #1)：新 chunk 到达（chunkIndex > 0）。
   *
   * 切到 prepend 模式 —— 后续 onLine 渲染的卡 buffer 到 fragment。
   * 下个 chunk 边界 / batch-end 时一次性 prependFragmentAtTop，多次切换最终
   * 形成 DOM 顺序 [最老, ..., 次新, head 最新]。
   */
  onChunk(chunkIndex: number): void {
    // 切换前先 flush 上一块累积的 fragment（如果有）
    this.flushPendingPrepend();

    if (chunkIndex === 0) {
      // 不应该走这里（chunk 0 走 onBatchStart）；防御性
      return;
    }
    this.inPrependMode = true;
    // 给所有现有 Tab 创建 fragment + chunk 边界打断 tool-group
    for (const t of this.tabs.values()) {
      t.pendingToolGroup = null;
      if (t.pendingPrependFragment === null) {
        t.pendingPrependFragment = document.createDocumentFragment();
      }
    }
  }

  /**
   * v2.2 (issue #12): 重放批次完结。各 Tab 调 flushPending 一次性算 + rebuild，
   * 然后切回 live 模式。后续真实时新消息按现有 per-record 路径走。
   *
   * v2.3.1 (issue #1)：先 flush 累积的 prepend fragment，再退出 batch 模式。
   */
  onBatchEnd(): void {
    this.flushPendingPrepend();
    this.inBatch = false;
    this.inPrependMode = false;
    // v2.3.1 Phase 2：batch 结束切回 live 模式（后续真实时新消息走 full pipeline 同步 hljs）
    setRenderLazyMode(false);
    for (const t of this.tabs.values()) {
      t.branchFolder.flushPending();
      t.branchFolder.setBatchMode(false);
      // v2.3.1: 切块场景下，老块的 tool_use 现在已渲染 → 重试匹配早到的 fallback result
      const ctx: RenderContext = {
        parentPath: t.parentPath,
        toolUseNames: t.toolUseNames,
        toolUseElements: t.toolUseElements,
        pendingToolResults: t.pendingToolResults,
      };
      reconcilePendingToolResults(ctx);
      // 清 fragment 让下次启动时干净
      t.pendingPrependFragment = null;
    }
  }

  /**
   * v2.3.1 (issue #1)：把每个 Tab 累积的 prepend fragment 一次性 prepend 到 stream 顶部。
   *
   * **不用 anchor**：直接贴到 contentEl 顶部（contentEl.firstChild 前）。多次调用
   * 后 DOM 顺序自然是 [最新调用的内容, ..., 最早调用的内容]。
   * 因为 chunks 顺序是 head(最新) → mid 次新 → ... → tail 最老：
   *   - chunk 0 (head) → append 到 stream 底部
   *   - chunk 1 (次新)→ prepend 到顶部 → DOM: [chunk1, chunk0]
   *   - chunk 2 (再老) → prepend 到顶部 → DOM: [chunk2, chunk1, chunk0]
   *   - ...
   *   - chunk N (最老) → prepend 到顶部 → DOM: [chunkN, ..., chunk1, chunk0] ✓
   * 最终时间升序正确。
   */
  private flushPendingPrepend(): void {
    for (const t of this.tabs.values()) {
      if (!t.pendingPrependFragment) continue;
      if (t.pendingPrependFragment.childNodes.length === 0) continue;
      t.stream.prependFragmentAtTop(t.pendingPrependFragment);
      t.pendingPrependFragment = document.createDocumentFragment();
    }
  }

  /**
   * v2.3.1 (issue #1)：onLine 渲染出新卡片 element 时统一走这里。
   * - 默认（append 模式 / live 模式 / chunk 0 head）：tab.stream.append() 加到 stream 底部
   * - prepend 模式（chunk index > 0 期间）：buffer 到 tab.pendingPrependFragment，
   *   下个 chunk 边界 / batch-end 一次性 insertBefore(...firstChunkAnchor)
   *
   * 用 fragment buffer 而不是逐条 insertBefore 是关键性能点：N=3000 时省 ~3000 次 layout。
   *
   * v2.4 修首次启动乱序：**live source 永远 stream.append**（绕开 inPrependMode）。
   * 用户在 chunked replay 期间敲的真新行时间序最新，必须贴到 stream 底部；
   * 旧实现把 live 也丢进 prepend fragment 会被错误推到顶部。batch source 仍按
   * inPrependMode 走 buffer / append 切块逻辑不变。
   */
  private appendCardOrBuffer(
    tab: Tab,
    element: HTMLElement,
    source: PayloadSource,
  ): void {
    if (source === "batch" && this.inPrependMode && tab.pendingPrependFragment) {
      tab.pendingPrependFragment.appendChild(element);
    } else {
      tab.stream.append(element);
    }
    // v2.3.1 Phase 2: batch / 切块期间的卡片用 lazy hljs，注册 IntersectionObserver
    // 让进可视区时再补跑高亮。live 路径的卡片本来 hljs 已经同步跑完，这里 observe
    // 后 enhanceCard 是 fast path（没 .code-pending 即直接标 enhanced 返回）。
    // 不区分模式统一 observe 简化逻辑 + 多余开销可忽略。
    if (this.inBatch) {
      observeForEnhance(element);
    }
  }

  /**
   * 收到一行 JSONL 时调用。
   *
   * v2.4：`source` 区分 batch（chunked replay 历史）vs live（用户实时敲的真新行）。
   * 加载期间 live 必须贴到 stream 底部，不被 inPrependMode 错误捕获。
   */
  onLine(
    payload: {
      session_id: string;
      cwd: string | null;
      path: string;
      message: JsonlRecord;
    },
    source: PayloadSource = "live",
  ): void {
    const tab = this.ensureTab(payload.session_id, payload.cwd, payload.path);

    // ai-title / custom-title 不进入消息流，只更新 Tab 标题。
    // Claude Code v2.1.x 起 schema 从 ai-title/aiTitle 改为 custom-title/customTitle；
    // 两者语义一致（会话级语义标题），共用 applyAiTitle，旧 jsonl 兼容。
    if (payload.message.type === "ai-title") {
      this.applyAiTitle(tab, payload.message.aiTitle);
      return;
    }
    if (payload.message.type === "custom-title") {
      this.applyAiTitle(tab, payload.message.customTitle);
      return;
    }

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      pendingToolResults: tab.pendingToolResults,
    };
    const result = renderMessage(payload.message, ctx);

    switch (result.kind) {
      case "skip":
        break;
      case "card":
        // 普通卡（user / 含 text 的 assistant）出现就断开工具组累积
        tab.pendingToolGroup = null;
        // issue #8: 把 uuid/parentUuid 写到 DOM 上，让 BranchFolder 能扫
        markCardUuid(result.element, payload.message);
        this.appendCardOrBuffer(tab, result.element, source);
        // v2.4 issue #2: 真用户输入触发自动切 tab。
        // 三个判断缺一不可，每个都是"信号准"的关键：
        //   - result.kind === "card" 已经过滤了 tool_result 回灌（走 tool-group）
        //     + CLI noise（走 skip）+ <synthetic> 占位（走 skip）。详 cards/index.ts
        //   - message.type === "user" 排除 assistant 卡（claude 流式回复不抢焦）
        //   - source === "live" 排除启动 replay 阶段的历史 user 消息（chunked
        //     batch 进来会上千条连环触发 → 闪烁）
        if (source === "live" && payload.message.type === "user") {
          this.userActive(tab.sessionId);
        }
        break;
      case "tool-group": {
        if (tab.pendingToolGroup) {
          // 追加到当前组（不需要重新 append 到 stream / fragment——已经挂着）
          addToToolGroup(tab.pendingToolGroup, result.units);
        } else {
          // 新建组卡片并 append 到 stream / fragment
          const group = buildToolGroup(result.timestamp);
          addToToolGroup(group, result.units);
          tab.pendingToolGroup = group;
          // issue #8: 给 tool-group root 写 data-uuid，让 BranchFolder 把它当
          // 普通 card 一样判别主线。否则 tool-group 在 DOM 里既不算 on-main
          // 也不算 off-main → 会切断 off-main 连续段，导致被回退的消息每条单独成 fold。
          // 用**首个**贡献消息的 uuid 当代表（典型情况下整组同主支，混合极罕见）。
          markCardUuid(group.root, payload.message);
          this.appendCardOrBuffer(tab, group.root, source);
        }
        break;
      }
    }

    // issue #8: 主线检测必须 track **所有** user/assistant 记录，包括渲染成
    // tool-group 的（纯工具调用 assistant）和被 skip 的（空内容 user）。
    // 否则 parent 链断 → 几乎所有卡片判 off-main → 全部塞进折叠卡 = 已知 bug。
    // 这些 "被 track 但没 data-uuid 的记录" 在 BranchFolder 里只参与主线计算，
    // 不参与 DOM 扫描（rebuild 只看带 data-uuid 的元素）。
    feedBranchFolder(tab.branchFolder, payload.message);

    if (result.kind === "skip") return;

    if (this.activeId !== tab.sessionId) {
      tab.unread += 1;
      this.refreshTabBar();
    }
  }

  ensureTab(sessionId: string, cwd: string | null, sourcePath: string): Tab {
    let tab = this.tabs.get(sessionId);
    if (tab) {
      if (!tab.cwd && cwd) {
        tab.cwd = cwd;
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
      }
      return tab;
    }

    const title = computeTitleFor(sessionId, cwd, null);

    const streamEl = document.createElement("div");
    streamEl.className = "stream";
    streamEl.style.display = "none";
    this.streamRootEl.appendChild(streamEl);

    const stream = new MessageStream(streamEl);
    const branchFolder = new BranchFolder(stream.contentElement);
    // v2.2 issue #12: 重放期创建的新 Tab 也进 batch 模式，避免每条 record 都
    // 触发 O(N) computeMainBranch。批结束时 onBatchEnd 会统一 flush。
    if (this.inBatch) branchFolder.setBatchMode(true);

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
      aiTitle: null,
      status: "live",
      streamEl,
      stream,
      parentPath: sourcePath,
      unread: 0,
      pendingToolGroup: null,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      branchFolder,
      pendingPrependFragment: null,
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
    tab.pendingToolGroup = null;
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
    tab.pendingToolGroup = null;
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

    for (const [sid, t] of this.tabs) {
      t.streamEl.style.display = sid === sessionId ? "block" : "none";
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

    return { root, label, badge };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    refs.root.classList.toggle("active", sid === this.activeId);
    refs.root.classList.toggle("archived", tab.status === "archived");
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

/**
 * issue #8: 给 user/assistant 卡的 root element 写 data-uuid（+ data-parent-uuid）。
 * BranchFolder 用 data-uuid 扫定位 + 主线判定。
 *
 * 工具组（tool-group）的 root 元素**不**走这里 —— 它没有单一 uuid，不参与折叠。
 */
function markCardUuid(el: HTMLElement, rec: JsonlRecord): void {
  if (rec.type !== "user" && rec.type !== "assistant") return;
  el.setAttribute("data-uuid", rec.uuid);
  if (rec.parentUuid) {
    el.setAttribute("data-parent-uuid", rec.parentUuid);
  }
}

/**
 * issue #8: 把记录推到 BranchFolder 做主线计算。
 *
 * **必须 track 的范围**（链完整性，详 branching.ts::extractBranchRecord）：
 *  - user / assistant：被渲染的目标，参与 DOM 折叠
 *  - attachment：不渲染但占 5% 链节点
 *  - system：占 3% 链节点
 *
 * attachment/system 只参与算法（提供 parent 链节点），没 data-uuid 不参与
 * DOM rebuild 扫描。
 */
function feedBranchFolder(folder: BranchFolder, rec: JsonlRecord): void {
  const br = extractBranchRecord(rec);
  if (br) folder.recordAdded(br);
}

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
    showBringTerminalToast(String(e?.message ?? e));
  });
}

/**
 * 拉前失败时右下角弹 8s fixed toast。
 *
 * 为什么 fixed：v1.6.4 把错放进 status-bar 文字会触发 flex 重排挤压消息区，
 * 用户报告"消息往右移动"。toast 用 position:fixed 完全脱离正常文档流。
 */
function showBringTerminalToast(msg: string): void {
  document.querySelector("#bring-terminal-toast")?.remove();
  const toast = document.createElement("div");
  toast.id = "bring-terminal-toast";
  toast.textContent = `⚠ 拉前失败：${msg}`;
  toast.title = msg;
  document.body.appendChild(toast);
  window.setTimeout(() => toast.remove(), 8000);
}

