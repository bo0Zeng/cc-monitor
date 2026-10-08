/**
 * 〔「一个 store，一个 router」〕**拆分地图**。
 *
 * 拆之前（基线 `bf3cdc83`，4991 行）本文件是**一个类干五件事**。现打逐件（谁在调 → 拆到哪）：
 *
 * | # | 这件事 | 谁在调（生产） | 拆到 |
 * |---|---|---|---|
 * | ① | **会话状态账**：tab 集合 · 顺序 · 当前 tab · 早于 tab 到达的信号暂存（已结束/可重连/红绿灯）· 账号快照 · 任务快照 · 「变了」那一份订阅 | `main.ts` / `entry-viewer.ts` 把 `events.ts` 的事件喂进来（`onLine` · `archiveTab` · `reviveTab` · `markTmuxIdle` · `updateActivity` · `updateTasks` · `createSkeletonTab` · `setSessionAccounts`）；`main.ts` 读投影（`snapshotSessions` · `peekSession` · `hasTab` · `activeRepoInfo` · `touchedFilesFor` · `activeSessionId`） | `tab-store.ts`（store）· `tab-model.ts`（`Tab` 形状与标题）· `tab-session-facts.ts`（把后端给的会话事实落到 tab 上；数据源 `views/facts-source.ts`）· `tab-session-state.ts`（会话状态的两个轴：活性 × 可恢复性，转移与谓词） |
 * | ② | **路由**：切到哪个 tab、谁有权切（手动 5s 保护 · 自动跟随）、记住上次的 tab | `main.ts` 快捷键 / 命令面板 / 启动选 active（`switchTo` · `cycleActive` · `jumpToIndex` · `applyBehavior` · `persistLastActive` · `onManualSwitch`）；`onLine` 里真用户输入（`userActive`） | `tab-router.ts` |
 * | ③ | **实时流视图**：每个 tab 的流 DOM、按 seq 门控建卡、尾部窗口 / 骨架 / 上翻补批 / 哨兵 / 大纲、重放批 | `events.ts` → `onBatchStart` · `onLine` · `onBatchEnd`；`main.ts` DEV 探针 `debugSnapshot` | `tab-stream-view.ts` |
 * | ④ | **tab 栏视图**：按钮 · 徽章 · 分组 · 拖动排序与成组 · 固定 · 顺序落盘 | 用户手势；`main.ts` 启动 `loadCollections` · `loadPinned` · `loadOrder` | `tab-bar-view.ts` · `tab-bar-drag.ts` · `tab-drop.ts`（纯落点算术）· `tab-bar-prefs.ts`（集合 / 固定 / 顺序三份落盘） |
 * | ⑤ | **会话动作**：右键菜单（resume · 换号重启 · attach · 预览 · 杀会话 · 集合 · 固定）与它背后的 IPC（开目录 · 新窗口 · 切到终端窗口） | 用户右键；`main.ts` 快捷键 / 命令面板（`bringActiveTerminalToFront` · `openActiveTabCwd` · `openActiveInNewWindow` · `closeActiveIfArchived`） | `tab-menu.ts`（菜单项怎么组）· `kit/menu.ts`（菜单这个控件，全产品一份）· `tab-session-actions.ts`（动作本身；IPC 经包装层 `ipc/commands.ts`） |
 *
 * 本文件拆完只剩 `TabManager` 这个**组装根**：对外 API（`main.ts` / `entry-viewer.ts` 调的那些）
 * 逐字不变，事件怎么在上面几份之间流转写在这里。拆分逐子步提交，每一步 `tabs.vitest` 全绿、断言不动。
 */
import { speakerNameOf } from "./agent-profile";
import { SPEAKER_SELECTOR } from "./cards/speaker";
import { markRunCard, markRunWindow } from "./cards/subagent";
import { openAgentWindow } from "./agent-window-open";
import { runLabel } from "./runs";

import type { SessionRunsPayload } from "./generated/SessionRunsPayload";
import { openNewSession } from "./new-session";
import { fetchSessionTasks, type TaskEntry, type TasksPanel } from "./tasks-panel";
import type { JsonlLinePayload } from "./events";
import { detectAccountMismatch, type SessionAccount } from "./accounts";
import type { BehaviorConfig } from "./behavior";
import { toast, undoToast } from "./kit/toast";
import { copyText } from "./copy-table";
import { fullTitle, needsOf, needsWord } from "./session-face";
import { SeqSet, TailWindow } from "./live-window";
import type { AgentsPanel } from "./agents-panel";
import { turnEndNotifier } from "./turn-notify";
import type { GridSessionSnapshot, SessionPeek } from "./session-status";
import { contextPercentOf, type ContextLimitOverrides } from "./views/context-limit";
import {
  terminalFrontAvailable,
  TERMINAL_FRONT_UNAVAILABLE_TITLE,
  TERMINAL_FRONT_UNAVAILABLE_DETAIL,
} from "./terminal-front";
import { computeTitleFor, type Tab, type TabsSummary } from "./tab-model";
import type { SessionActivity } from "./generated/SessionActivity";
import { ENDED, LIVE, RECONNECTABLE, closesWithoutMenu, containerEvent, isLive, isResumeOnly, hasTerminal, inTmux, nextState, type StateEvent } from "./tab-session-state";
import type { SessionContainer } from "./generated/SessionContainer";
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, originFromWire, type Origin } from "./ipc/origin";
// `Tab` 的形状与标题函数搬去了 `tab-model.ts`；这里原样 re-export，既有 import 面零改动。
export type { Tab, TabsSummary } from "./tab-model";
// 落点算术搬去了 `tab-drop.ts`；原样 re-export，`tabs.vitest.ts` 的 import 面零改动。
export {
  moveTab,
  pickDropTarget,
  tabUnderY,
  commonDirName,
  defaultGroupName,
  groupMoveForDrop,
  DWELL_MS,
  DWELL_MOVE_PX,
} from "./tab-drop";
export type { DropTarget, GroupMove, TabRect } from "./tab-drop";
import { TabMenu } from "./tab-menu";
import { TabSelection } from "./tab-selection";
import { openBatchMenu, type TabBatchHost } from "./tab-batch-menu";
import { TabStore, type ActiveView } from "./tab-store";
import { appStore, type Slice } from "./app-store";
import { TabStreamView } from "./tab-stream-view";
import { LiveCards, type LivePainter, type TapPayload } from "./live-card";
import { TabBarPrefs } from "./tab-bar-prefs";
import { TabBarDrag } from "./tab-bar-drag";
import { TabBarView } from "./tab-bar-view";
import { TabRouter } from "./tab-router";
import { applyFacts } from "./tab-session-facts";
import { FactsSource } from "./views/facts-source";
import type { SessionFacts } from "./session-reads";
import {
  TabSessionActions,
  bringMonitorToFront,
  bringRemoteTerminalToFront,
  bringTerminalToFront,
  e2eLog,
  frontOnce,
  forgetSession,
} from "./tab-session-actions";
import { frontView, type FrontAct, type FrontResult, type FrontView } from "./front-result";
import { updateBackendOf } from "./backend-deploy";
import { awaitedFor, paintWaiting } from "./cards/step-line";
import { machineName } from "./control-said";
import { closeFrontResult, flashFrontDone, setFrontBusy, showFrontResult } from "./front-pop";
import { CHANNEL_ACTS, LaunchSlots, type SlotSpec } from "./launch-slot";
import { applyHandedBack } from "./cards/speaker-bar";
import { applyRetries } from "./cards/api-error";
import type { ActionId } from "./keybindings/actions";


export class TabManager {
  /**
   * **会话状态账住 `tab-store.ts`** —— tab 集合 · 顺序 · 当前 tab · 早到信号暂存 · 账号快照 ·
   * 任务快照，外加「变了」那唯一一份订阅。本类与拆出去的几份都读写同一个实例。
   */
  private readonly store = new TabStore();
  /**
   * **路由住 `tab-router.ts`**：下一个 / 第 N 个是谁、自动跟随放不放行、
   * 切完之后记住上次的 tab 与 5s 手动保护。切换本身的编排（可见性 · 物化 · 面板 · 贴底）仍在 `switchTo`。
   */
  private readonly router = new TabRouter(this.store, () => this.bar.visibleOrder());

  /**
   * 实时流视图住 `tab-stream-view.ts`。**在构造体里建，不写成字段初始化**：
   * 它要 `streamRootEl`，而字段初始化在参数属性赋值之前跑（esbuild 出原生 class field 时就是这个序），
   * 写成初始化器会拿到 `undefined`。
   */
  private readonly view: TabStreamView;

  constructor(
    /** tab 栏容器：本类只把它交给 tab 栏视图与拖拽（判据要它时读 `bar` / `dragger` 那两份，或用自己传进来的那个元素）。 */
    barEl: HTMLElement,
    streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI。它是 store 那一份订阅的第一个订阅者。 */
    onTabsChanged?: (summary: TabsSummary) => void,
    /** issue #11: 全局 TasksPanel，切 Tab / 收事件时由 TabManager 喂数据 */
    private tasksPanel?: TasksPanel,
    /** issue #23: 全局 AgentsPanel（子运行列表：运行表的成品），喂数方式同 tasksPanel */
    private agentsPanel?: AgentsPanel,
  ) {
    // 子运行的流有动静 ⇒ 面板那一行的「最近：…」跟着变。
    this.live.onRunLive = (sid) => {
      if (sid === this.store.activeId) this.agentsPanel?.refresh();
    };
    if (agentsPanel) {
      agentsPanel.host = {
        open: (sid, run) => void this.openRunWindow(sid, { run }),
        isOpen: (sid, run) => this.runWindows.has(`${sid}\u0000${run}`),
        liveOf: (sid, run) => this.live.core.liveBlockOf(sid, run),
      };
    }
    if (onTabsChanged) this.store.subscribe(onTabsChanged);
    // 「账号快照变了」改订阅 store：宿主整份换快照，这里同一拍应用（原先宿主直调 `setSessionAccounts`）。
    appStore.sessionAccounts.subscribe((snap) => {
      if (snap) this.setSessionAccounts(snap.rows, snap.emailByName, snap.lastByS, snap.readyOrigins, snap.currentByOrigin);
    });
    this.bar = new TabBarView(this.store, this.prefs, barEl, {
      refreshTabBar: () => this.refreshTabBar(),
      openTabCwd: (sid) => this.openTabCwd(sid),
      bringTerminalToFront: (sid) => this.frontFrom(sid, "row"),
      bringRemoteTerminalToFront: (sid) => this.frontFrom(sid, "row"),
      closeTab: (sid) => this.closeTab(sid),
      switchTo: (sid) => this.switchTo(sid),
      pick: (sid, how) => this.pick(sid, how),
      isSelected: (sid) => this.selection.has(sid),
      beginDrag: (e, sid, root) => this.dragger.begin(e, sid, root),
      takeSuppressedClick: (sid) => this.dragger.takeSuppressedClick(sid),
      openMenu: (e, sid) => this.openMenu(e, sid),
      openMenuAt: (el, sid) => this.openMenu(el, sid),
      rereadAll: () => this.rereadAll(),
      reconnect: (origin) => this.reconnect(origin),
    });
    this.slots = new LaunchSlots(
      {
        tail: this.bar.slotTail,
        root: streamRootEl,
        hasTab: (sid) => this.store.tabs.has(sid),
        switchTo: (sid) => this.switchTo(sid),
        shown: (on) => {
          this.slotOver = on;
          this.onSlotShown?.(on);
        },
      },
      CHANNEL_ACTS,
    );
    this.dragger = new TabBarDrag(this.store, this.prefs, barEl, this.bar.tabButtons, {
      refreshTabBar: () => this.refreshTabBar(),
      openInNewWindow: (sid, screenX, screenY) => this.openInNewWindow(sid, screenX, screenY),
    });
    this.view = new TabStreamView(this.store, streamRootEl, {
      onLine: (payload) => this.onLine(payload),
      refreshTabBar: () => this.refreshTabBar(),
      scheduleTabBarRefresh: () => this.bar.scheduleRefresh(),
      applyAiTitle: (tab, aiTitle) => this.applyAiTitle(tab, aiTitle),
      userActive: (sid) => this.userActive(sid),
      forkFrom: (tab, uuid) => void openNewSession({ origin: tab.origin, fork: { sid: tab.sessionId, uuid, title: tab.aiTitle ?? tab.title } }),
      runLabelOf: (sid, run) => {
        const r = this.live.board.of(sid).find((x) => x.run === run);
        return r ? runLabel(r) : undefined;
      },
    });
  }

  /**
   * 活卡：中转抄出来的 SSE 先上屏，jsonl 那一轮到了整轮覆盖（`live-card.ts`）。
   * 路由只认「`stream` 就是这个 tab 的 sid、机器也对得上」；对不上 ⇒ 匿名流，不显示。
   */
  private readonly live = new LiveCards(
    (origin, stream) => {
      const t = this.store.tabs.get(stream);
      return t && t.origin === origin ? t.sessionId : null;
    },
    (sid) => this.store.tabs.get(sid)?.stream.trailerElement ?? null,
  );


  /**
   * tab 栏的三份落盘偏好（集合 · 固定 · 顺序）住 `tab-bar-prefs.ts`。
   */
  private readonly prefs = new TabBarPrefs(this.store, {
    refreshTabBar: () => this.refreshTabBar(),
    createSkeletonTab: (sid, projectDir, origin, background, name) =>
      this.createSkeletonTab(sid, projectDir, origin, background, name),
    resumeTab: (sid) => this.actions.resumeTab(sid),
  });

  /** P7a-3：从 `config.json` 拉一次集合并重画。宿主启动时调一次。 */
  loadCollections(): Promise<void> {
    return this.prefs.loadCollections().then(() => this.settleUnarrivedOnce());
  }
  /** 启动时把固定的 tab 复活出来（流程见 `tab-bar-prefs.ts` 那一份的头注）。 */
  loadPinned(): Promise<void> {
    return this.prefs.loadPinned();
  }
  /** 启动时把落盘的顺序拉回来（为什么它曾是结构性 no-op 见 `tab-bar-prefs.ts` 那一份的头注）。 */
  loadOrder(): Promise<void> {
    return this.prefs.loadOrder();
  }
  /** 右键菜单那一项：翻转固定。 */
  /** 固定 / 取消固定。取消固定撤得回 ⇒ 不确认：直接做 ＋ 8 秒撤销。 */
  togglePin(sid: string): void {
    const t = this.store.tabs.get(sid);
    const wasPinned = t?.pinned === true;
    this.prefs.togglePin(sid);
    if (t && wasPinned) undoToast(copyText("tabBar.unpinned.toast", { title: fullTitle(t) }), () => this.prefs.togglePin(sid), () => {});
  }

  /** 移出分组：撤得回 ⇒ 不确认：直接做 ＋ 8 秒撤销（组随最后一个人走没了也建得回来）。 */
  private leaveGroupUndoable(sid: string): void {
    const col = this.prefs.groupOf(sid);
    void this.prefs.leaveGroup(sid);
    this.refreshTabBar();
    if (!col) return;
    const before = { ...col };
    undoToast(copyText("tabBar.group.left", { name: col.name }), () => {
      void this.prefs.restoreGroup(before, [sid]);
      this.refreshTabBar();
    }, () => {});
  }

  /**
   * 拖动排序 / 拖动成组 / 拖出去撕窗口的状态机住 `tab-bar-drag.ts`（落点算术住 `tab-drop.ts`）。
   * 在构造体里建：它要 `barEl`（参数属性，字段初始化时还没赋上）。
   */
  private readonly dragger: TabBarDrag;
  /**
   * tab 栏视图（按钮 · 徽章 · 分组容器 · 整刷与帧末合批）住 `tab-bar-view.ts`。
   * 在构造体里建（要 `barEl`）。**整刷的入口仍是本类的 `refreshTabBar`**：拖拽守卫在这一层，
   * 而且判据会把实例上的 `refreshTabBar` 换成计数替身 —— 帧末合批那一刷必须经它。
   */
  private readonly bar: TabBarView;
  /** 起新会话之后、报到之前的占位标签页（`launch-slot.ts`）：栏里跟在列表末尾，那一页盖在消息流上。 */
  private readonly slots: LaunchSlots;
  /** 占位标签页那一页显 / 收（宿主让会话头跟着让开）。 */
  onSlotShown: ((on: boolean) => void) | null = null;
  /** 占位标签页那一页此刻显没显（显着 ⇒ 作用于当前会话的快捷键不落到底下那个真标签页，[`shadowedBySlot`]）。 */
  private slotOver = false;
  /** 作用于当前会话的那几键（占位标签页显着时让开）。 */
  private static readonly ACTS_ON_ACTIVE_SESSION: ReadonlySet<ActionId> = new Set<ActionId>([
    "session.find",
    "session.toggle-process",
    "session.prev-turn",
    "session.next-turn",
    "session.to-bottom",
    "tab.close-archived",
    "tab.open-cwd",
    "tab.pop-out",
    "tab.context-menu",
    "terminal.bring-front",
  ]);

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON（形状、口径与秤 6 的三个账本见 `tab-stream-view.ts` 那一份）。
   * 生产不接线,方法本身无副作用。
   */
  debugSnapshot(): string {
    return this.view.debugSnapshot();
  }

  /**
   * 会话动作住 `tab-session-actions.ts`（它的 IPC 也经包装层 `ipc/commands.ts`）。它只要宿主给四样读数 / 回调。
   */
  private readonly actions = new TabSessionActions({
    tab: (sid) => this.store.tabs.get(sid),
    isAttachable: (sid) => this.isAttachable(sid),
    markRecord: (sid, present) => this.markRecord(sid, present),
  });

  /** 右键菜单里放哪几项住 `tab-menu.ts`；点下去做事直接交给上面那份 `actions`。 */
  private readonly menu = new TabMenu(
    {
      tab: (sid) => this.store.tabs.get(sid),
      isAttachable: (sid) => this.isAttachable(sid),
      openAccountPanel: (sid) => {
        const t = this.store.tabs.get(sid);
        if (t && this.onOpenAccountPanel) this.onOpenAccountPanel(sid, t.origin);
      },
      accountPanelWired: () => this.onOpenAccountPanel !== null,
      collectionsLoaded: () => this.prefs.collectionsLoaded,
      collections: () => this.prefs.collections,
      // 组员关系是 tab 自己的属性：菜单那三个动作改完内存（落盘偏好那一份同时写盘）就重画。
      groupOf: (sid) => this.prefs.groupOf(sid),
      joinGroup: (sid, gid) => {
        void this.prefs.joinGroup(sid, gid);
        this.refreshTabBar();
      },
      foundGroup: (sids, name, id) => {
        const why = this.prefs.foundGroup(sids, name, id);
        this.refreshTabBar();
        return why;
      },
      leaveGroup: (sid) => this.leaveGroupUndoable(sid),
      pinnedLoaded: () => this.prefs.pinnedLoaded,
      togglePin: (sid) => this.togglePin(sid),
      close: (sid) => this.closeTab(sid),
      openCwd: (sid) => void this.openTabCwd(sid),
      front: (sid) => this.frontFor(sid),
      viewTerminal: (sid) => {
        this.switchTo(sid);
        this.onViewTerminal?.();
      },
    },
    this.actions,
  );

  /** tab 栏的多选（纯界面状态，`tab-selection.ts`）。选中集合一变就重画（`.selected`）。 */
  private readonly selection = new TabSelection(() => this.refreshTabBar());

  /** 点 tab 按钮时的多选：单击清多选 · Ctrl 加减 · Shift 按条上看得到的顺序连选 · 点空白清掉。 */
  private pick(sid: string | null, how: "plain" | "toggle" | "range" | "clear"): void {
    const had = this.selection.size;
    if (how === "clear" || sid === null) this.selection.clear();
    else if (how === "plain") this.selection.plain(sid);
    else if (how === "toggle") this.selection.toggle(sid, this.store.activeId);
    else this.selection.range(sid, this.bar.visibleOrder(), this.store.activeId);
    // 单击且本来就没有多选 ⇒ 什么都没变，不白刷一遍（切 tab 那一刷随后自己来）。
    if (had > 0 || this.selection.size > 0) this.refreshTabBar();
  }

  /** 右键一个选中的 tab（多选 ≥ 2）⇒ 批量菜单；右键一个没选中的 ⇒ 清掉多选，照旧单个菜单。 */
  private openMenu(e: MouseEvent | HTMLElement, sid: string): void {
    if (e instanceof MouseEvent && this.selection.size >= 2 && this.selection.has(sid)) {
      openBatchMenu(e, this.selection.inOrder(this.bar.visibleOrder()), this.batchHost);
      return;
    }
    if (!this.selection.has(sid) && this.selection.size > 0) {
      this.selection.clear();
      this.refreshTabBar();
    }
    this.menu.open(e, sid);
  }

  /** 批量菜单里只经 monitor 的那几项：每一个同单个那一项（固定 · 集合 · ×），落盘各一次。 */
  private readonly batchHost: TabBatchHost = {
    tab: (sid) => this.store.tabs.get(sid),
    collectionsLoaded: () => this.prefs.collectionsLoaded,
    collections: () => this.prefs.collections,
    pinnedLoaded: () => this.prefs.pinnedLoaded,
    setPinned: (sids, on) => this.prefs.setPinnedMany(sids, on),
    joinGroup: (sids, gid) => {
      void this.prefs.joinGroupMany(sids, gid);
      this.refreshTabBar();
    },
    foundGroup: (sids, name, id) => {
      const why = this.prefs.foundGroup(sids, name, id);
      this.refreshTabBar();
      return why;
    },
    leaveGroup: (sids) => {
      void this.prefs.leaveGroupMany(sids);
      this.refreshTabBar();
    },
    closeTabs: (sids) => {
      for (const sid of sids) this.closeTab(sid);
    },
  };

  private openTabCwd(sid: string): Promise<void> {
    return this.actions.openTabCwd(sid);
  }
  private openInNewWindow(sid: string, screenX?: number, screenY?: number): Promise<void> {
    return this.actions.openInNewWindow(sid, screenX, screenY);
  }

  /**
   * v2.2 (issue #12): 启动重放（jsonl-batch）开始时调一次。所有现有 Tab 的
   * BranchFolder 切到 batch 模式 —— 后续 recordAdded 只 push 不算 mainBranch。
   * 重放期 ensureTab 新创建的 Tab 也会自动进 batch（看 this.store.inBatch）。
   *
   * P5.2 B 重构：删了 inPrependMode / pendingToolGroup 清零 —— 前端用 timeline
   * 按 seq 排序，tool-group 合并改后处理（看左邻居），不再需要 chunk 边界协调。
   */
  onBatchStart(): void {
    this.store.inBatch = true;
    // P5.5 B 重构：lazy 通过 ctx.lazy 传到 renderMarkdown —— onLine 构造 ctx 时
    // 用 this.store.inBatch 设置。不再依赖 setRenderLazyMode 全局开关。
    for (const t of this.store.tabs.values()) {
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
    this.store.inBatch = false;
    // 会话事实（含 HUD 那一格 usage）在这里统一问后端（`batchEnd` 里），到了再推给 HUD（`onSessionFacts`）——
    //   批内不再逐条攒、也就没有「批末 flush 一次」那一步了。
    this.view.batchEnd();
  }

  /**
   * 那台机器的会话流**丢了几格**（`gap`：前端落后超过一整个 credit 窗口，句柄丢了、原位报）。
   * 丢的是哪几个会话的哪几行流里说不出来 ⇒ 那台机器上的**每个** tab 都按行号补（`TabStreamView.recoverFromGap`）。
   */
  onStreamGap(origin: Origin): void {
    for (const t of this.store.tabs.values()) {
      if (t.origin === origin) this.view.recoverFromGap(t);
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
    // 项目目录不从行里取（行上的 cwd 是那一刻的工作目录，会漂进子目录）：只认后端那一格（会话宣告 / 会话事实）。
    const tab = this.ensureTab(payload.session_id, null, payload.path, originFromWire(payload.origin));

    // 按 seq 去重：旁路快照与实时行的重叠区是精确重复的 (sid, seq)（后端 seq = 行号，§25a），
    // 老后端重连还会从 seq 0 重发整段。必须在 renderStreamRecord 之前、且覆盖 skip 记录
    // （attachment/isMeta/空 user 有 seq 但不入 timeline，timeline.has 漏判）。
    // 本机会话的行也走后端的帧与旁路快照之后，这一道本机同样会命中（原先写「本地永不命中」）。
    // 这是**唯一**一道：seq ＝ 当前文件里的行号，从头重读先出声、tab 整份重来
    //   （`onRecordFileReread`）⇒「换新 seq 重投同一条记录」那条路没了，原先按 uuid 再挡的那一道随之删了。
    if (tab.seenSeqs.has(payload.seq)) return;
    // monitor 连着见过、都不可显示的那一段一起记（去重集合成区间，段数有上界）。
    if (payload.skipped_from !== undefined) tab.seenSeqs.addRange(payload.skipped_from, payload.seq);
    tab.seenSeqs.add(payload.seq);

    // jsonl 那一轮到了 ⇒ 同 `message.id` 的活卡整轮覆盖（撤掉）；挂在去重**之后**：
    // 「前端现有的去重层就是吸收层」—— 重投 / 快照重叠区的重复记录不会重复触发。
    this.live.onRecord(tab.sessionId, payload.rid);

    // 大纲：只记一笔「这份会话又长了」（清单问后端要，这里不判、不攒）。
    this.view.noteGrew(tab); // 会话事实同一笔（分叉 · agent · 改动文件 · usage 问后端要）

    // Batch14-F42：turn-end 系统通知。放在双重去重之后（重投行不重报）、
    // 渲染管线之前（通知与渲染/收纳互相独立）。批量重放由 inBatch 短路。
    turnEndNotifier.observe(payload.session_id, tab.title, payload, this.store.inBatch);

    // 这里原先还挂着四个旁路记账员（分叉血缘 · agent 配对 · 最新 usage · 改动文件集），
    //   它们改成问后端要（`history-facts`）；`onLine` 上只剩「真事件」那两个（compact 完成 · 轮次结束）。
    //   判据 `tests/frontend/ui/online-bypass-ledger.vitest.ts`。

    this.view.ingest(tab, payload);
  }

  /**
   * Batch5-F18：骨架 Tab——活跃清单（本地 IPC / 远端 session_added 事件）一到
   * 即建，不等首条内容行。复用 ensureTab 全部语义：项目目录取后端那一格（给了就对齐）；parentPath 空由
   * 首条行回填；pendingArchive/pendingActivity 落实、batch 模式继承均沿用。
   * 已存在同 sid Tab 时不重建（幂等，重连重发 session_added 无害），项目目录按宣告对齐。
   */
  createSkeletonTab(
    sessionId: string,
    projectDir: string | null,
    origin: Origin,
    background: boolean | null = null,
    name: string | null = null,
    // E73：`null` = 没说（旧 backend / 存量会话）= 视为可以。只有显式 `false` 才记账。
    attachable: boolean | null = null,
  ): void {
    if (attachable === false) this.store.notAttachableSids.add(sessionId);
    else this.store.notAttachableSids.delete(sessionId);
    this.ensureTab(sessionId, projectDir, "", origin, background, name);
  }

  /**
   * E73：attach / 「杀死空 tmux」这几个动作对这个会话有没有意义。
   * `↗` 不再看它：点的那一刻现查 —— 那台后端答「此刻连着这个会话的终端」、本机后端答进程链
   *（`remote-terminal-front.ts`），沿链找窗口拉前的那一跳归 monitor（`lib.rs::bring_remote_terminal_to_front`）。
   *
   * **默认 true**：没说就是可以。判据只认后端明说的 `attachable:false`
   *（源头是 pidfile 的同名布尔，契约见 `src/doc/IPC-PROTOCOL.md` §9.3）。
   */
  isAttachable(sid: string): boolean {
    return !this.store.notAttachableSids.has(sid);
  }

  /** Batch5-F19：启动 active 选择用（last-active 是否已有 tab）。 */
  hasTab(sessionId: string): boolean {
    return this.store.tabs.has(sessionId);
  }

  /**
   * F91（#27）：跨会话监控快照——`GridMonitorView` 消费的**只读派生 DTO 列表**（本地 + 所有远端会话）。
   * 纯派生：不外泄任何内部 DOM / Map 引用（防外部改到 TabManager 内部状态）。插入序（同 tab-bar）。
   * context% 的上限与状态栏同一个数（后端定的 `latestContextLimit`）。
   */
  /**
   * A3：喂入远端 live 探测的会话账号归属（来自 backend `--session-accounts`）+ 账号邮箱表。喂完刷新所有 tab 的账号徽章。
   * 生产上只由 `appStore.sessionAccounts` 的订阅调（构造体里那一行）；判据可直接喂。
   */
  setSessionAccounts(
    rows: SessionAccount[],
    emailByName: Map<string, string>,
    lastAccountByS: Map<string, string> = new Map(),
    readyOrigins: Set<string> = new Set(),
    currentByOrigin: Map<string, string> = new Map(),
  ): void {
    this.store.sessionAccountsByS = new Map();
    for (const r of rows) {
      if (r.sessionId) this.store.sessionAccountsByS.set(r.sessionId, r);
    }
    this.store.accountEmailByName = emailByName;
    this.store.accountLastByS = lastAccountByS;
    this.store.accountReadyOrigins = readyOrigins;
    this.store.currentByOrigin = currentByOrigin;
    this.bar.refreshAccountBadges();
  }

  snapshotSessions(): GridSessionSnapshot[] {
    const out: GridSessionSnapshot[] = [];
    for (const tab of this.store.tabs.values()) {
      const runs = this.live.board.of(tab.sessionId);
      out.push({
        sessionId: tab.sessionId,
        title: tab.title,
        origin: tab.origin,
        cwd: tab.projectDir,
        state: tab.state, // 两轴原样交出去：cell 与 tab-bar 读同一份、经同一组谓词
        activity: tab.activity?.doing ?? null,
        waitingFor: tab.activity?.waitingFor ?? null,
        runningAgents: runs.filter((r) => r.state === "running").length,
        totalAgents: runs.length,
        contextPct:
          tab.latestPromptTokens != null
            ? contextPercentOf(tab.latestPromptTokens, tab.latestContextLimit)
            : null,
        contextTokens: tab.latestPromptTokens,
        unread: tab.unread,
        background: tab.background,
        account: this.store.sessionAccountsByS.get(tab.sessionId)?.account ?? null,
      });
    }
    return out;
  }

  /**
   * auto-e2e F-E0:全会话状态一行 JSON——Tier1/Tier2 断言出口(经 e2e-probe Ctrl+Alt+F10 触发 →
   * fe_perf 日志)。复用 `snapshotSessions`(已含两轴状态/origin/account),派生 `mismatch`
   * (detectAccountMismatch:活会话账号与该 origin 当前账号确知且不一致)。**不动 `debugSnapshot`
   * 形状**(f40-suite 依赖它),这是并列的第二个探针出口。生产不接线,方法本身无副作用/无落盘。
   */
  debugSessionsSnapshot(): string {
    const sessions = this.snapshotSessions().map((s) => ({
      sid: s.sessionId.slice(0, 8),
      liveness: s.state.liveness,
      recoverability: s.state.recoverability,
      origin: s.origin,
      account: s.account,
      mismatch: detectAccountMismatch(
        s.account,
        isRemoteOrigin(s.origin) ? this.store.currentByOrigin.get(s.origin) ?? null : null,
      ),
    }));
    return JSON.stringify(sessions);
  }

  /**
   * F91b（batch17）：监控板选中 cell 的「内容 peek」补充数据（纯读派生，无写/无落盘）。
   * 只给 `snapshotSessions` 之外的细节：model / 改过的文件 / subagent 名单（运行中优先）。
   * 未知 sid → null（选中会话恰好消失时调用方据此清选中）。
   */
  peekSession(sessionId: string): SessionPeek | null {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return null;
    const agents = this.live.board
      .of(sessionId)
      .map((r) => ({ label: runLabel(r), status: r.state }))
      .sort((x, y) => (x.status === "running" ? 0 : 1) - (y.status === "running" ? 0 : 1));
    return {
      model: tab.latestModel,
      recentFiles: [...tab.touchedFiles],
      agents,
    };
  }

  /** Batch5-F19：switchTo 是否写回 last-active（viewer/tear-off 窗口置 false）。值住路由（`tab-router.ts`）。 */
  get persistLastActive(): boolean {
    return this.router.persistLastActive;
  }
  set persistLastActive(v: boolean) {
    this.router.persistLastActive = v;
  }

  /** Batch5-F19（G 验收）：用户手动切 tab 时回调——main.ts 用它清 pendingStartupActive，
   *  防迟到的远端宣告补切抢走用户已选的焦点。值住路由（`tab-router.ts`）。 */
  get onManualSwitch(): (() => void) | null {
    return this.router.onManualSwitch;
  }
  set onManualSwitch(fn: (() => void) | null) {
    this.router.onManualSwitch = fn;
  }

  /**
   * 「当前 tab 变了」改订阅 store（原先是 `onActiveUsageChanged` / `onActiveFactsAvailability`
   * 两个点对点回调）。写它的几处：切 tab（`switchTo`）· 当前 tab 的 usage / 事实可用性 / 项目目录变了 · 关掉最后一个 tab。同值不通知。
   */
  get active(): Slice<ActiveView> {
    return this.store.active;
  }

  /** 当前 tab 那一格按此刻的 tab 重写一遍（没有当前 tab ⇒ 全空）。 */
  private publishActive(): void {
    const sid = this.store.activeId;
    const t = sid === null ? undefined : this.store.tabs.get(sid);
    this.store.active.set({
      sid: t ? sid : null,
      model: t?.latestModel ?? null,
      promptTokens: t?.latestPromptTokens ?? null,
      contextLimit: t?.latestContextLimit ?? null,
      limitFrom: t?.latestLimitFrom ?? "assumed",
      unavailable: t?.facts.unavailableReason ?? null,
      projectDir: t?.projectDir ?? null,
    });
  }

  /**
   * 有就取、没有就建。`projectDir` 只认后端给的那一格（会话宣告 / 固定 tab 存下来的；会话事实那一路在 `onSessionFacts`）；行进来时传 `null` —— 不从行里猜。
   * 给了、且与 tab 上的不同 ⇒ 改过来、重算标题（后端重宣告时对齐它）；机器同理（已有 tab 只有宣告与行会再走到这里）。
   */
  ensureTab(
    sessionId: string,
    projectDir: string | null,
    sourcePath: string,
    origin: Origin = LOCAL_ORIGIN,
    background: boolean | null = null,
    bgName: string | null = null,
  ): Tab {
    this.settleClose(sessionId); // 撤销期里关掉的那个又来了 ⇒ 先把旧的收干净，再按新的建
    let tab = this.store.tabs.get(sessionId);
    if (tab) {
      // 机器以后端宣告 / 行为准：tab 可能是按盘上存的旧标签建的（固定的 tab 复活），不改过来活卡永远对不上。
      if (origin !== tab.origin) {
        tab.origin = origin;
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
        if (sessionId === this.store.activeId) this.publishActive();
        if (tab.pinned) void this.prefs.persistPinned(); // 盘上那份同拍改写：等不到宣告的已结束会话下次起来也是对的
      }
      // SSH 重连：远端会话掉线时被 flush 归档过，现在又收到它的行 = backend 在重放 = 会话仍
      // 活着 → 复活成 live。必须放在 ensureTab 里（在 onLine 的 seq 去重 return 之前），否则整段
      // 重放全被去重时连第一条行都走不到翻转。**仅远端**：本地归档由 PID 判活驱动，不靠「收到行」
      // 翻转，避免会话退出时尾写把已归档的本地 Tab 误复活（远端掉线归档是连接驱动，无此风险）。
      //
      // audit-fixes F03.2（D 审计修）：远端**可重连**的 tab 又收到后端重宣告 / jsonl 行 = claude
      // 复活（backend 只对活 pidfile 重宣告并推行；真 idle 会话已从 remote_active 移出、不重宣告也不
      // 推行）。这是「可重连 → 活」的**主**信号（queue 内、与行保序，SESSION_IDLE 恒排在会话末行之后，
      // 故复活行/重宣告严格晚于 idle）。不能只靠 activity 格：那是非 queue 同步派发、且
      // null-activity 的后端（远端 v1 无 status 字段）下永远不来 → 活跃流式会话永久卡在可重连。
      //
      // 上面两件事原先是两段（`status` 翻 live · `tmuxIdle` 清 false），因为两个轴挤在两个字段里；
      //   两轴之后它们是同一条转移：「远端见行」把**死了的**（已结束 / 可重连）翻回活（`nextState`）。
      // 取回来的历史行不算「远端见行」（`TabStore.historyFeed` 头注）。
      if (isRemoteOrigin(tab.origin) && !this.store.historyFeed && this.applyState(tab, "remote-line")) {
        this.prefs.clearPinHint(sessionId); // 远端复活：空态提示的对象没了
        this.refreshTabBar();
        this.emitTabStateProbe(tab); // F-E1:远端复活(死 → 活)
      }
      // v2.22.2 kind 冲突消解:同一 sid 可能有多份 pidfile(实证:cc-backend 的
      // bg-spare 备用进程复用**父会话的 sid**写 kind=bg)——宣告到达顺序不定,
      // bg 先到会把真交互会话降格成 ⚙(用户截图实锤)。
      // 规则:**interactive 恒压过 bg**——后到的 interactive 宣告在此升格纠正标题;
      // 反向(bg 后到)绝不降格。
      // 〔「删掉树」〕升格**不动位置**:原先这里还把它摘下来按宿主重新挂树,
      // 树删了之后位置与 kind 无关。
      if (background === false && tab.background) {
        tab.background = false;
        tab.bgName = null;
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
      }
      // Batch5-F18：骨架 Tab（无行创建）的 parentPath 为空——首条带路径的行回填，
      // 保住「在新窗口打开」等依赖 jsonl 路径的功能。
      if (!tab.parentPath && sourcePath) {
        tab.parentPath = sourcePath;
      }
      if (projectDir && projectDir !== tab.projectDir) {
        tab.projectDir = projectDir;
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
        if (sessionId === this.store.activeId) this.publishActive();
      }
      return tab;
    }

    const title = computeTitleFor(
      sessionId,
      projectDir,
      null,
      isRemoteOrigin(origin) ? origin : null,
      background ?? false,
      bgName,
    );

    const { streamEl, stream, branchFolder, timeline, inputsEl, inputsPanel, outline, turnFold, turnRail } =
      this.view.mountTabDom(sessionId);

    // v2.3.0 issue #11: 异步 fetch 初始 task 快照。`session-tasks` 流那一路（`refreshTasks`）并行更新
    // tasksBySid，两路收敛到同一份数据；若 sid 是 active 同步推给全局 panel。
    void fetchSessionTasks(sessionId, origin).then((tasks) => {
      this.store.tasksBySid.set(sessionId, tasks);
      if (this.store.activeId === sessionId) {
        this.tasksPanel?.setSession(sessionId, tasks);
      }
    });

    tab = {
      sessionId,
      background: background ?? false,
      bgName,
      title,
      projectDir,
      aiTitle: null,
      forkedFromSessionId: null, // issue #63①：后端的会话事实到了才有（`onSessionFacts`）
      agent: null, // 是哪一家：会话事实到了才有（`onSessionFacts`），之前要分家的那几项灰着
      writers: [], // 同上
      origin,
      state: LIVE, // 见了行 / 宣告了才建 ⇒ 活着；早到的死亡信号在下面落实
      // **不做自动固定**（照 `tab-collections.ts` 那条「手动建，不要自动」的先例，
      // `§B.7` 逐字）。盘上固定过的那些由 `loadPinned` 在复活时置回 true。
      pinned: false,
      // 组员关系是 tab 自己的属性；盘上有它的组 id ⇒ 下面 `adoptGroup` 归位。
      group: null,
      streamEl,
      stream,
      parentPath: sourcePath,
      unread: 0,
      timeline,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      runCards: new Map(),
      branchFolder,
      pendingToolResults: new Map(),
      seenSeqs: new SeqSet(),
      window: new TailWindow(),
      skeleton: null,
      skeletonFetch: "idle",
      midBatchBuffer: [],
      fillHandler: null,
      outline,
      turnFold,
      turnRail,
      inputsPanel,
      inputsEl,
      // issue #23：红绿灯信号若先于建 Tab 到达，从暂存取（否则 null=未知→绿）
      activity: this.store.pendingActivity.get(sessionId) ?? null,
      // 下面三样只经 `facts` 落下来（后端 `history-facts` 出成品，`onSessionFacts`）。
      touchedFiles: new Set(), // 会话改动集
      latestPromptTokens: null, // F88b：HUD context% 数据
      latestModel: null,
      latestContextLimit: null,
      latestLimitFrom: "assumed",
      needs: null,
      pending: [],
      lastSay: null,
      retries: new Map(),
      facts: new FactsSource(
        () => {
          // 问的是**当前**那一份 tab 的路径与机器（`parentPath` 由首条行回填；没有 ⇒ 这一趟不要）。
          const t = this.store.tabs.get(sessionId);
          return t && t.parentPath ? { origin: t.origin, jsonlPath: t.parentPath, limits: this.contextLimits } : null;
        },
        {
          facts: (f) => this.onSessionFacts(sessionId, f),
          availability: () => this.onFactsAvailability(sessionId),
        },
      ),
    };
    this.store.pendingActivity.delete(sessionId);
    this.view.wireTab(tab);
    // issue #19：若该 sid 的归档信号先于本次建 Tab 到达（见 archiveTab），落实归档，
    // 避免重载后已结束会话复活成关不掉的 live Tab。本地 un-archive（上方 origin!==null
    // 那条）不适用，故归档后续 replay 行也不会把它复活。
    if (this.store.pendingArchive.delete(sessionId)) {
      tab.state = ENDED;
      tab.activity = null; // 同 archiveTab：死会话不留陈旧灯/tooltip
      this.store.pendingTmuxIdle.delete(sessionId); // 已结束优先：真 tmux 没了，暂存的可重连作废
      this.store.pendingContainer.delete(sessionId); // 死了 ⇒ 容器一格由死的那一刻说了算
    } else if (this.store.pendingTmuxIdle.delete(sessionId)) {
      // audit-fixes F03.2：可重连信号早于建 Tab（F5 重放乱序）→ 落实。
      tab.state = RECONNECTABLE;
      this.store.pendingContainer.delete(sessionId);
    } else {
      // 容器事实早于建 Tab ⇒ 落实（建出来就是活的，转移只经 `nextState`）。
      const c = this.store.pendingContainer.get(sessionId);
      this.store.pendingContainer.delete(sessionId);
      if (c) tab.state = nextState(tab.state, c);
    }
    this.store.tabs.set(sessionId, tab);
    this.store.placeInOrder(tab);
    // tab 到了 ⇒ 盘上那份组 id 意图若有它，挪到 `tab.group`（形同上一行对顺序意图的再应用）。
    this.prefs.adoptGroup(tab);

    if (this.store.activeId === null) {
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

  /** 根据 tab.projectDir + tab.aiTitle + sessionId 算出展示标题（远端 Tab 加 `[origin]` 前缀） */
  private computeTitle(tab: Tab): string {
    return computeTitleFor(
      tab.sessionId,
      tab.projectDir,
      tab.aiTitle,
      isRemoteOrigin(tab.origin) ? tab.origin : null,
      tab.background,
      tab.bgName,
      tab.forkedFromSessionId,
    );
  }

  /**
   * auto-e2e F-E1:tab 生命周期状态转移探针。在**真值点**(markTmuxIdle 进可重连 / archiveTab 进已结束 /
   * reviveTab 复活 / ensureTab 远端复活)emit 可 grep 的 `[e2e] tab-state` 行,gray-light 全链
   * 套件按它断言 活→可重连→已结束 序列(跨进程整链,单测碰不到)。self-gate
   * `import.meta.env.DEV`:生产构建整支(含模板串)被 vite 消除。
   * 行里两个键就是两个轴（原先是 `status=… tmuxIdle=…`）；`tests/e2e/graylight-suite.sh` 的两条 grep 同拍改，
   *   两边对得上由 `tests/frontend/ui/tab-session-state.vitest.ts` 对拍。
   */
  private emitTabStateProbe(tab: Tab): void {
    if (!import.meta.env.DEV) return;
    e2eLog(
      `[e2e] tab-state sid=${tab.sessionId.slice(0, 8)} liveness=${tab.state.liveness} recoverability=${
        tab.state.recoverability ?? "-"
      } origin=${isLocalOrigin(tab.origin) ? "local" : tab.origin}`,
    );
  }

  /**
   * 会话状态**只经这一处改**：转移表住 `tab-session-state.ts::nextState`，这里只落值。
   * 返回「变没变」—— 调用方据此决定要不要重画、打探针、做各自的副作用。
   */
  private applyState(tab: Tab, ev: StateEvent): boolean {
    const next = nextState(tab.state, ev);
    if (next === tab.state) return false;
    tab.state = next;
    return true;
  }

  /** session 退出（~/.claude/sessions/<PID>.json 被删）且容器也没了 —— 已结束，内容保留 */
  archiveTab(sessionId: string): void {
    this.live.dropTab(sessionId); // 结束了 ⇒ 它的活卡全撤
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      // issue #19：Tab 还没被 ensureTab 建出来（归档信号早于 replay 行到达）——
      // 记下待归档，建 Tab 时落实。否则这里直接 return 会静默丢弃归档 → 僵尸 live Tab。
      this.store.pendingArchive.add(sessionId);
      this.store.pendingActivity.delete(sessionId); // issue #23：死会话的暂存灯一并清
      this.store.pendingTmuxIdle.delete(sessionId); // audit-fixes F03.2：已结束优先，清暂存的可重连
      this.store.pendingContainer.delete(sessionId); // 死了 ⇒ 暂存的容器事实作废
      return;
    }
    // 活 / 可重连 ⇒ 已结束；已经是已结束 ⇒ 不变（`nextState`）。
    if (!this.applyState(tab, "ended")) return;
    // issue #23：会话结束 → 灯灭（CSS 上 `.ended` 本就隐藏 .live-dot，这里保持状态干净）
    tab.activity = null;
    // P5.2 B 重构后无 pendingToolGroup —— archive 不需要打断 tool-group 累积
    // （tool-group 合并改后处理，看 timeline 邻居；archive 后无新 record 入 timeline）。
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:已结束(tmux 也没了)
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
    this.store.pendingArchive.delete(sessionId);
    this.store.pendingTmuxIdle.delete(sessionId); // audit-fixes F03.2：复活即清暂存灰灯
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return;
    if (isRemoteOrigin(tab.origin)) return; // 仅本地；远端复活走 ensureTab 见行路径
    if (!this.applyState(tab, "started")) return; // 死 ⇒ 活；已经活着 ⇒ 不变
    this.prefs.clearPinHint(sessionId); // 真接上了 ⇒ 那块「只能 resume」的空态该走了
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:本地复活(死 → 活)
  }

  /**
   * audit-fixes F03.2：远端 claude 退出但 tmux 会话仍在 → **可重连**（死 ＋ 容器还在）。
   * 后端 emitter 收 backend removed 且 `@ccm_sid` present 时 emit `idle` 格 驱动（**不**
   * 归档、不 forget）。Tab 未建（F5 重放乱序）则暂存待 ensureTab 落实。已结束的 Tab 不回到可重连
   * （真 tmux 没了才裁已结束，已结束优先）。无变化不重绘。离开可重连四处：
   * ensureTab（**主**：远端 tab 又收后端重宣告/行 = 复活，queue 内保序）/ updateActivity
   * （claude 再产活动，非 queue 的次要信号）/ reviveTab（本地）/ archiveTab（tmux 真没了）。
   * 改之前这里只置 `tmuxIdle = true`、`status` 留在 live —— 活性一轴说了假话。
   */
  markTmuxIdle(sessionId: string): void {
    this.live.dropTab(sessionId); // claude 退了 ⇒ 它的活卡全撤
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      this.store.pendingTmuxIdle.add(sessionId);
      return;
    }
    // 活 ⇒ 可重连；已经死了（可重连 / 已结束）⇒ 不变（`nextState`）。
    if (!this.applyState(tab, "idle")) return;
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:可重连(claude 退但 tmux 在)
  }

  /** `session-tap`：中转抄出来的一件归一事件（`events.ts` 直派）。 */
  onSessionTap(p: TapPayload): void {
    this.live.onTap(p);
  }

  /** 开着窗口的那几个子运行（`sid\0run`；agent 窗口开 / 关时自己报，`window-events.ts`）。 */
  private readonly runWindows = new Set<string>();

  /**
   * 一个会话的运行表到了（会话流里的 `runs` 格）：派出它们的那几张工具卡记上是哪个子运行、什么状态（被挤出表的已收场那几个
   * 在 `ended` 里，卡照样标）；当前 tab 的就交 agent 面板。主 tab 的消息流里不画子运行。
   */
  onSessionRuns(p: SessionRunsPayload): void {
    const tab = this.store.tabs.get(p.session_id);
    if (!tab) return;
    this.live.onRuns(tab.sessionId, p.runs);
    for (const r of p.runs) {
      if (r.tool === undefined) continue;
      const card = tab.runCards.get(r.tool);
      if (!card) continue;
      markRunCard(card, r.run, r.state, r);
      markRunWindow(card, this.runWindows.has(`${tab.sessionId}\u0000${r.run}`));
    }
    for (const e of p.ended) {
      const card = tab.runCards.get(e.tool);
      if (!card) continue;
      markRunCard(card, e.run, e.state);
      markRunWindow(card, this.runWindows.has(`${tab.sessionId}\u0000${e.run}`));
    }
    if (tab.sessionId === this.store.activeId) this.agentsPanel?.setSession(tab.sessionId, p.runs);
  }

  /** 一个子运行的窗口开了 / 关了：面板那一行与派出它的那张卡标「窗口已开」。 */
  setRunWindow(sid: string, run: string, open: boolean): void {
    const k = `${sid}\u0000${run}`;
    if (open === this.runWindows.has(k)) return;
    if (open) this.runWindows.add(k);
    else this.runWindows.delete(k);
    const tab = this.store.tabs.get(sid);
    const tool = this.live.board.of(sid).find((r) => r.run === run)?.tool;
    const card = tab && tool !== undefined ? tab.runCards.get(tool) : undefined;
    if (card) markRunWindow(card, open);
    if (sid === this.store.activeId) this.agentsPanel?.refresh();
  }

  /** 开一个子运行自己的窗口（按运行 · 或按派出它的那次工具调用找；运行表里还没有 ⇒ 不开）。已开着 ⇒ 壳把它拉到前面。 */
  async openRunWindow(sid: string, which: { run?: string; tool?: string }): Promise<void> {
    const tab = this.store.tabs.get(sid);
    const r = this.live.board.of(sid).find((x) => (which.run !== undefined ? x.run === which.run : x.tool === which.tool));
    if (!tab || !r) return;
    await openAgentWindow({ origin: tab.origin, sid, run: r, session: tab.aiTitle ?? tab.title, machine: isRemoteOrigin(tab.origin) ? tab.origin : null });
  }

  /** 回到派出它的地方：切到那个会话、滚到派出它的那张卡、闪一下。没有这张卡 ⇒ 只切过去。 */
  showRunCard(sid: string, tool: string): void {
    this.switchTo(sid);
    const card = this.store.tabs.get(sid)?.runCards.get(tool);
    if (!card) return;
    card.scrollIntoView({ block: "center" });
    card.classList.remove("search-hit-flash");
    void card.offsetWidth;
    card.classList.add("search-hit-flash");
  }

  /** 装活卡的画法（主窗口入口装；独立查看器不装 ⇒ 只记账不画，见 `live-card.ts::LivePainter`）。 */
  setLivePainter(p: LivePainter): void {
    this.live.setPainter(p);
  }

  /** 那台机器的 tap 流看不见了 ⇒ 那台的活卡全撤。 */
  dropLiveCards(origin: string): void {
    this.live.dropOrigin(origin);
  }

  /** 这个会话此刻在 tab 栏里是活的吗（历史浏览器删会话前问一句用）。没有这个 tab ⇒ `false`。 */
  isSessionLive(sessionId: string): boolean {
    const tab = this.store.tabs.get(sessionId);
    return tab !== undefined && isLive(tab.state);
  }

  /** 这个会话此刻「需要你」的那个词（等批准 …）；没有这个 tab / 不需要 ⇒ `null`（历史页那一行的徽标与状态点）。 */
  needsWordOf(sessionId: string): string | null {
    const tab = this.store.tabs.get(sessionId);
    const n = tab ? needsOf(tab) : null;
    return n ? needsWord(n.kind) : null;
  }

  /**
   * 这个会话的流容器（没有这个 tab ⇒ `null`）—— 记录文件那一句话挂在它顶上
   * （`record-file-notice.ts`；那个模块只由主窗口 `main.ts` 接线，样式随主窗口的产物走，不进与独立查看窗共用的块）。
   * **不碰会话状态**（判活不看 jsonl：不误判结束）。
   */
  streamElOf(sessionId: string): HTMLElement | null {
    return this.store.tabs.get(sessionId)?.streamEl ?? null;
  }

  /** 这个会话的消息流内容那一层（换号条按发生那一刻追加在这里，之后来的记录排在它后面）。 */
  streamContentOf(sessionId: string): HTMLElement | null {
    return this.store.tabs.get(sessionId)?.stream.contentElement ?? null;
  }

  /** 右键「账号…」开哪个面板（主窗口接；独立窗口不接 ⇒ 那一项照样在，点了不动）。 */
  onOpenAccountPanel: ((sid: string, origin: Origin) => void) | null = null;
  /** 菜单「看它的终端」：切到那个标签页之后开底部抽屉的终端页（抽屉归主窗口入口）。 */
  onViewTerminal: (() => void) | null = null;

  /** 这个会话在哪台（没有这个 tab ⇒ `null`）。 */
  /** 这个会话是哪一家（会话事实给的 `agent`）；还不知道 ⇒ `null`。 */
  agentOf(sessionId: string): string | null {
    return this.store.tabs.get(sessionId)?.agent ?? null;
  }

  originOf(sessionId: string): Origin | null {
    return this.store.tabs.get(sessionId)?.origin ?? null;
  }

  /** tab 栏上别处的数据变了（额度：被卡的会话 `✕ 5h`）⇒ 重画一遍（按钮只在样子变了才写 DOM）。 */
  repaintTabBar(): void {
    this.refreshTabBar();
  }

  /**
   * 记录文件从头重读了（截短 / 改写）：后端行号从 0 重数、旧的一代作废
   * ⇒ 这个 tab 的内容整份重来（`TabStreamView.restartContent`），之后到的行按新的一代建。「不见了」不重来（号没换代）。
   * 要在「顶上说一句」之前调：重来换了流容器，那句话画在新的上面。
   */
  onRecordFileReread(sessionId: string, change: string): void {
    if (change !== "truncated" && change !== "rewritten") return;
    const tab = this.store.tabs.get(sessionId);
    if (tab) this.view.restartContent(tab);
  }

  /**
   * 后端报来这条活会话的容器（`container` 格：在认得的宿主里 · 不在任何宿主里 · 这边不认识的宿主）。
   * Tab 还没建 ⇒ 暂存（同 `pendingActivity`），建 Tab 时落实；不认识的宿主记成「终端形式未知」、日志一条（不吞）。
   * 只落在活着的会话上（`nextState`）：死了的那一格由死的那一刻的裁决说了算。
   */
  noteContainer(sessionId: string, container: SessionContainer): void {
    if (container.form === "other") {
      console.warn(`[tabs] 会话 ${sessionId.slice(0, 8)} 的终端形式这边不认识：host=${container.host}`);
    }
    const ev = containerEvent(container);
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      this.store.pendingContainer.set(sessionId, ev);
      return;
    }
    if (!this.applyState(tab, ev)) return;
    this.refreshTabBar();
  }

  /**
   * 〔说不清〕这台机器的活会话清单报完了（远端 `origin-sessions-listed`；本机 `list_active_sessions`〔散文墓碑〕）。
   *
   * 这台的「说不清」（固定复活、还没被报过）逐条落地：`liveSids` 里有 ⇒ 活（本机那条路给清单；远端的清单
   * 早已经由 远端 `live` 格 把 tab 建成活的了，不传）；没有 ⇒ 已结束（
   * 「A 看得见却没报这条」）。记下这台已报完 ⇒ 之后才复活出来的固定 tab 直接落已结束。
   */
  markOriginSeen(origin: Origin, liveSids?: ReadonlySet<string>): void {
    this.store.seenOrigins.add(origin);
    this.bar.markOriginDown(origin, false);
    let changed = false;
    for (const tab of this.store.tabs.values()) {
      if (tab.origin !== origin || tab.state.liveness !== "unseen") continue;
      const ev: StateEvent = liveSids?.has(tab.sessionId) ? "started" : "seen-absent";
      if (this.applyState(tab, ev)) {
        changed = true;
        if (ev === "started") this.prefs.clearPinHint(tab.sessionId);
        this.emitTabStateProbe(tab);
      }
    }
    if (changed) this.refreshTabBar();
  }

  /** 当前会话的会话事实再问一次（状态栏「上下文」浮层里的［重试］）。 */
  retryActiveFacts(): void {
    const t = this.store.activeId === null ? undefined : this.store.tabs.get(this.store.activeId);
    if (!t) return;
    t.facts.markStale();
    void t.facts.refresh();
  }

  /** 设置里的上下文上限表（随会话事实交给那台后端，上限在那里定）。 */
  private contextLimits: ContextLimitOverrides = {};

  /** 设置里的上限表换了 ⇒ 每个 tab 带着上一份成品再问一次（只有上限会变，文件不重扫）。 */
  setContextLimits(limits: ContextLimitOverrides): void {
    if (JSON.stringify(limits) === JSON.stringify(this.contextLimits)) return;
    this.contextLimits = limits;
    for (const t of this.store.tabs.values()) {
      if (!t.facts.everArrived) continue;
      t.facts.markStale();
      void t.facts.refresh();
    }
  }

  /** 壳说过「各台都报完了」（`listed` 格带 `all`）。 */
  private allListed = false;
  private unarrivedSettled = false;

  /** 「各台都报完了」那一拍（壳给的，`listed` 格的 `all`）。 */
  markAllListed(): void {
    this.allListed = true;
    this.settleUnarrivedOnce();
  }

  /** 这一趟里壳说过「各台都报完了」没有。 */
  get everAllListed(): boolean {
    return this.allListed;
  }

  /** 各台都报完了活会话清单 ⇒ 盘上记着、却一个都没回来的组员不会再来了：收掉，空了的组随之消失（只做一次）。 */
  private settleUnarrivedOnce(): void {
    if (this.unarrivedSettled || !this.allListed || !this.prefs.collectionsLoaded) return;
    this.unarrivedSettled = true;
    void this.prefs.forgetUnarrived();
    this.refreshTabBar();
  }

  /**
   * 这台机器**看不见了**（会话流 `unseen` 格：到它的连接断了 / F5 时它还没报完清单）。
   * **机器级**：一格说一台，这台上的 tab 逐个过 `nextState` 的 `unseen`。
   *
   * 活的 / 可重连的 ⇒ 说不清；已结束 / 记录没了不动。那台机器从「报完了清单」里摘掉 ——
   * 之后才复活出来的固定 tab 不再直接落已结束，要等那台重连、再报一次（`markOriginSeen`）。
   * 还活着的由紧跟着的重宣告（`live` 格 → `started` / `remote-line`）翻回活，可重连的由 `idle` 格落回，其余由 `listed` 落已结束。
   * Tab 还没建 ⇒ 什么都不做（它建出来时由行 / 宣告定）。
   */
  markOriginUnseen(origin: Origin): void {
    this.store.seenOrigins.delete(origin);
    this.bar.markOriginDown(origin, true);
    let changed = false;
    for (const tab of this.store.tabs.values()) {
      if (tab.origin !== origin || !this.applyState(tab, "unseen")) continue;
      changed = true;
      this.emitTabStateProbe(tab);
    }
    if (changed) this.refreshTabBar();
  }

  /**
   * resume 一跳问过那台后端：这条会话的记录在不在（`history-record`）。
   * 不在 ⇒ 已结束落到「记录没了」；在 ⇒ 「记录没了」翻回已结束（记录回来了，例：同步盘补齐）。
   * 别的态不动（`nextState`：可重连的终端还在，接得回去；活的不走 resume）。
   */
  /**
   * 栏顶「重新读取」：栏上每个 tab 所在的机器各对齐 ＋ 补读一次（`resyncMachines` 去重、并行）；
   * 成功的那几台照「对齐做完」标出记录没了的固定条（与设置页「重新对齐」同一步）。
   */
  /** 栏顶左边那几颗全局入口（宿主建好交进来）。 */
  mountHeadActions(buttons: HTMLElement[]): void {
    this.bar.mountHeadActions(buttons);
  }

  /** `Ctrl+J` · 点「需要你」：跳到下一个需要你的会话（等得最久的在前；当前就是 ⇒ 下一个）。 */
  jumpToNextNeeds(): void {
    const sid = this.bar.nextNeedsSid();
    if (sid !== null) this.switchTo(sid);
  }

  /** 此刻需要你的会话数（窗口标题 · 系统通知用）。 */
  needsCount(): number {
    return this.bar.needsCountNow();
  }

  /** 全部 tab，按条上看到的顺序（窗口标题 · 系统通知数「需要你」用）。 */
  tabsInOrder(): Tab[] {
    return this.bar.visibleOrder().map((sid) => this.store.tabs.get(sid)).filter((t): t is Tab => t !== undefined);
  }

  /** 会话头 · 「需要你」钉条读的那一份：当前 tab（没有 ⇒ `null`）。 */
  activeTab(): Tab | null {
    const sid = this.store.activeId;
    return sid === null ? null : (this.store.tabs.get(sid) ?? null);
  }

  /** 会话头 / 钉条的［恢复 ▾］。 */
  openResumeFor(anchor: HTMLElement, sid: string): void {
    this.menu.openResumeMenu(anchor, sid);
  }

  /** 会话头 / 钉条的［在终端里打开］（远端、在 tmux 里）。 */
  attachInTerminal(sid: string): void {
    void this.menu.attachRemote(sid);
  }

  /** 会话头 · 钉条 · 命令面板的 ↗ 与行尾那颗同一条路（结果锚在会话头的 ↗）。 */
  frontFor(sid: string): void {
    const tab = this.store.tabs.get(sid);
    if (!tab || !hasTerminal(tab.state) || !terminalFrontAvailable()) return;
    void this.frontFrom(sid, "head");
  }

  /** 会话头的「打开工作目录」。 */
  openCwdOf(sid: string): void {
    void this.openTabCwd(sid);
  }

  /** 这个 tab 的菜单锚在一颗按钮上（会话头「更多」）。 */
  /** 会话头「⋯」：这个标签页的右键菜单 ＋ 流的开关（过程默认展开）。 */
  openMenuFor(anchor: HTMLElement, sid: string): void {
    if (!this.selection.has(sid) && this.selection.size > 0) {
      this.selection.clear();
      this.refreshTabBar();
    }
    this.menu.open(anchor, sid, this.view.streamToggles());
  }

  /** 菜单键 / `Shift+F10`：在当前标签页上开右键菜单（锚在它那一行；那一行看不见 ⇒ 锚在会话头）。 */
  openActiveMenu(): void {
    const sid = this.store.activeId;
    if (sid === null) return;
    const row = document.querySelector<HTMLElement>("#tab-bar .tab.active");
    const anchor = row && row.getClientRects().length > 0 ? row : document.getElementById("session-head");
    if (anchor) this.openMenuFor(anchor, sid);
  }

  /** `End`：当前会话回到底部。 */
  toBottom(): void {
    this.activeTab()?.stream.scrollToBottom();
  }

  /** 命令面板「刷新各台机器上的会话」：同标签页栏顶那颗刷新。 */
  refreshAll(): void {
    void this.rereadAll();
  }

  /** 这个会话固定了没有（命令面板「固定 / 取消固定」那一条的字）。 */
  isPinned(sid: string): boolean {
    return this.store.tabs.get(sid)?.pinned === true;
  }

  /** 离线条的［重新连接］：那台断着在退避里等 ⇒ 立刻重拨一次（壳那一侧 `backend_start`：在跑就是「别等了」）。 */
  reconnect(origin: string): void {
    this.actions.reconnect(origin);
  }

  private async rereadAll(): Promise<void> {
    const ok = await this.actions.rereadMachines([...this.store.tabs.values()].map((t) => t.origin));
    for (const o of ok) void this.flagPinsWithoutRecord(o);
  }

  /** 那台「重新对齐」过 ⇒ 标出记录没了的固定条、说一句，点了才摘（本体在 `actions`）。 */
  flagPinsWithoutRecord(origin: string): Promise<void> {
    const pinned = [...this.store.tabs.values()].filter((t) => t.pinned && t.origin === origin);
    return this.actions.flagPinsWithoutRecord(origin, pinned, (sid) => {
      if (this.store.tabs.get(sid)?.pinned) this.togglePin(sid);
    });
  }

  markRecord(sessionId: string, present: boolean): void {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return;
    if (!this.applyState(tab, present ? "record-present" : "record-gone")) return;
    this.refreshTabBar();
    this.emitTabStateProbe(tab);
  }

  /**
   * 红绿灯状态更新（会话流的 activity 格）。`doing = null`（说不清）→ 清空回默认绿点。Tab 还没建则暂存
   * （pendingActivity，ensureTab 落实）。无变化不重绘。
   */
  updateActivity(
    sessionId: string,
    doing: SessionActivity | null,
    waitingFor: string | null,
  ): void {
    const act = doing === null ? null : { doing, waitingFor };
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      if (act) this.store.pendingActivity.set(sessionId, act);
      else this.store.pendingActivity.delete(sessionId);
      return;
    }
    // 已结束不更新（审计：心跳清死会话后磁盘残留 PID.json 被重扫会推陈旧
    // activity，已结束的 tab 会挂上过期的 waiting tooltip——灯本身被 CSS 隐藏）。
    if (isResumeOnly(tab.state)) return;
    // audit-fixes F03.2：收到活动信号 = claude 活着（远端 activity 仅在 claude 存活时由
    // backend 推）→ 可重连回到活。必须放在下方「无变化早退」之前：复活后首个 activity 未必与
    // 之前的陈旧 activity 值不同，否则被早退跳过、回不到活。回到活即使 activity 没变也要重绘。
    const clearedIdle = act !== null && this.applyState(tab, "activity");
    if (
      tab.activity?.doing === act?.doing &&
      tab.activity?.waitingFor === act?.waitingFor
    ) {
      if (clearedIdle) this.refreshTabBar();
      return;
    }
    // 需要你的种类与那一句在会话事实里（后端配着记录判）：状态一变就再要一份；不在等了 ⇒ 手上那份当场作废（不等回包）。
    if (act?.doing !== "needs_you") tab.needs = null;
    tab.facts.markStale();
    void tab.facts.refresh();
    tab.activity = act;
    this.refreshTabBar();
  }

  /**
   * 后端的一份会话事实到了（`views/facts-source.ts`）⇒ 落到 tab 上（`applyFacts`，纯投影），
   * 只刷变了的那几块：分叉 ⇒ 标题 `↳`（issue #63①）· 在写它的进程 ⇒ tab 的悬停提示 · usage ⇒ HUD（F88b，只 active）。
   * 改动文件集没有推的去处（监控板 peek 是现取）。
   */
  private onSessionFacts(sid: string, f: SessionFacts): void {
    const tab = this.store.tabs.get(sid);
    if (!tab) return;
    const ch = applyFacts(tab, f);
    // 交回了的子运行：它的收场通知以交回为准（同一个子运行只报一次）。
    applyHandedBack(tab.streamEl, new Set(f.handedBack));
    // 一串一串重试的结局（按首条的 id）。
    applyRetries(tab.streamEl, tab.retries);
    if (ch.forkedFrom || ch.projectDir) {
      tab.title = this.computeTitle(tab);
      this.refreshTabBar();
    } else if (ch.writers || ch.needs || ch.peek) this.refreshTabBar();
    if (ch.needs) tab.turnRail.render(); // 在等你的那一轮琥珀
    this.paintStepWaits(tab);
    if ((ch.usage || ch.projectDir) && sid === this.store.activeId) this.publishActive();
    if (ch.agent) {
      // 是哪一家到了：先画出来的卡头那一格补上那一家的名字（之后建的卡按它直接画）。
      const who = speakerNameOf(tab.agent) ?? "";
      for (const el of tab.streamEl.querySelectorAll<HTMLElement>(SPEAKER_SELECTOR)) el.textContent = who;
    }
  }

  /**
   * 过程里还没结果的那几步照会话事实画：`pending[].state`（在跑 · 在等你 · 状态不明）；事实里已经没有它（那一轮过去了、
   * 被截在上界外）⇒ 状态不明。结果已经到了的不动（`paintWaiting` 自己守）。
   */
  private paintStepWaits(tab: Tab): void {
    const WAITING_ROWS = '.step-line[data-call]:is([data-state="pending"], [data-state="running"], [data-state="awaiting"], [data-state="unclear"])';
    const by = new Map(tab.pending.map((p) => [p.id, p] as const));
    const n = tab.needs;
    for (const row of tab.streamEl.querySelectorAll<HTMLElement>(WAITING_ROWS)) {
      const call = row.dataset.call ?? "";
      const waited = n?.call === call ? awaitedFor(n.sinceMs, Date.now()) : null;
      const p = by.get(call);
      paintWaiting(row, p?.state ?? "unclear", waited, n?.kind === "approve", p?.why ?? null);
    }
  }

  /** 这个 tab 的会话事实可不可用变了 ⇒ 是 active 就告诉 HUD（要不到 ⇒ 出声，「不可用，不是空表」）。 */
  private onFactsAvailability(sid: string): void {
    if (sid === this.store.activeId) this.publishActive(); // 原因已落在 tab.facts 上
  }


  /**
   * 关闭 Tab：销毁 stream DOM、从 Map 中移除、通知后端 forget 历史、必要时切到相邻 Tab。
   * 仅允许关闭**已结束**（只能 resume）的 Tab，避免误关运行中的会话（`tab-session-state.ts::isResumeOnly`；
   * 可重连的不在其中 —— 与改两轴之前逐条相同）。
   * forget 后该 session 不会在下次 F5 刷新时被 event_replay 重放复活。
   */
  closeTab(sessionId: string): void {
    const tab = this.detachTab(sessionId);
    if (tab) this.finishClose(tab);
  }

  /** 关掉还在撤销期里的那几个（sid ⇒ 摘下来的 tab 与它原来在顺序里的位置）。 */
  private readonly pendingCloses = new Map<string, { tab: Tab; index: number }>();

  /**
   * 快捷键 `W`：当前 tab 已结束才关（说不清的只在右键菜单里关）；先只从栏上摘下来，给一条 8 秒「撤销」——
   * 撤销 ⇒ 原位、原分组、原固定放回（内容也还在）；到点 ⇒ 做完关闭剩下的事（取消固定 · 出组 · 让后端忘掉）。
   */
  closeActiveIfArchived(): void {
    const sid = this.store.activeId;
    if (!sid) return;
    const current = this.store.tabs.get(sid);
    if (!current || !closesWithoutMenu(current.state)) return;
    const index = this.store.orderedIds.indexOf(sid);
    const tab = this.detachTab(sid);
    if (!tab) return;
    tab.streamEl.classList.remove("active");
    tab.inputsEl.classList.remove("active");
    this.pendingCloses.set(sid, { tab, index });
    const headline = tab.pinned
      ? copyText("tabBar.close.doneUnpinned", { title: fullTitle(tab) })
      : copyText("tabBar.close.done", { title: fullTitle(tab) });
    undoToast(headline, () => this.undoClose(sid), () => this.settleClose(sid));
  }

  /** 撤销期里的那一个放回去：原位（顺序里原来那一格）· 原分组 · 原固定，并切回它。 */
  private undoClose(sid: string): void {
    const p = this.pendingCloses.get(sid);
    if (!p) return;
    this.pendingCloses.delete(sid);
    const { tab, index } = p;
    this.store.tabs.set(sid, tab);
    this.store.orderedIds.splice(Math.min(index, this.store.orderedIds.length), 0, sid);
    if (tab.group !== null && !this.prefs.collections.some((c) => c.id === tab.group)) tab.group = null;
    if (tab.pinned) void this.prefs.persistPinned(); // 期间别处落过一次固定表就没有它了
    this.switchTo(sid);
    this.refreshTabBar();
  }

  /** 撤销期过了（或这个会话又来了，得先把旧的收干净）⇒ 做完关闭剩下的事。 */
  private settleClose(sid: string): void {
    const p = this.pendingCloses.get(sid);
    if (!p) return;
    this.pendingCloses.delete(sid);
    this.finishClose(p.tab);
  }

  /**
   * 关闭的前一半：从栏上摘下来（只认已结束的），是当前 tab 就落到条上看到的邻居。回摘下来的 tab；关不了 ⇒ `null`。
   */
  private detachTab(sessionId: string): Tab | null {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return null;
    if (!isResumeOnly(tab.state)) return null;

    const wasActive = this.store.activeId === sessionId;
    const idx = this.store.orderedIds.indexOf(sessionId);
    // 落到条上看到的顺序里的后一个，没有就前一个（落点在别的组里也照这个顺序）
    const seen = this.bar.visibleOrder();
    const at = seen.indexOf(sessionId);
    const fallbackId = seen[at + 1] ?? seen[at - 1] ?? null;

    this.store.tabs.delete(sessionId);
    if (idx >= 0) this.store.orderedIds.splice(idx, 1);

    if (wasActive) {
      if (fallbackId !== null) {
        this.switchTo(fallbackId);
      } else {
        this.store.activeId = null;
        // issue #11: 关掉最后一个 Tab → panel 进入 null session 状态
        this.tasksPanel?.setSession(null, []);
        this.agentsPanel?.setSession(null, []);
        // F88b（审计）：无 fallback 时 switchTo 不会跑 → 当前 tab 那一格清空（HUD 隐藏，不残留死会话的 ctx% / 原因）
        this.publishActive();
        this.refreshTabBar();
      }
    } else {
      this.refreshTabBar();
    }
    return tab;
  }

  /** 关闭的后一半：撤活卡 · 拆流 · 取消固定 · 出组 · 让后端忘掉。`tab` 已不在 `store.tabs` 里。 */
  private finishClose(tab: Tab): void {
    const sessionId = tab.sessionId;
    this.live.dropTab(sessionId); // 先撤活卡（它的 DOM 随流容器一起走）
    this.view.disposeTab(tab);
    this.store.tasksBySid.delete(sessionId);
    // **关掉 = 取消固定。**
    //
    // pin 的语义是「别丢」（`§B.3b`），而 `×` 是用户**明确说要丢**。两者撞上时以后者为准 ——
    // 不摘的话下次开 app 它又回来了，那正是「能操作但没反应」的一种（点了 ×，第二天还在）。
    // ⚠ 只有真被固定过才写盘：没固定的 tab 关一下不该顺手改 `config.json`。
    if (tab.pinned) {
      tab.pinned = false;
      this.prefs.pinnedRecords.delete(sessionId);
      void this.prefs.persistPinned();
    }
    this.prefs.clearPinHint(sessionId);
    // 🔴 〔「x就是没了, 不存在还要移出分组」〕**关掉 = 组关系随它一起没。**
    //   摘盘上它那一键 `tabBar.groupOf.<sid>`；它是组里最后一个在栏里的 ⇒ 组也没（`TabBarPrefs.forgetTab`）。
    //   必须在上面 `store.tabs.delete` 之后：「组里还剩谁」只数真在栏里的。没分组的 tab ⇒ 零写。
    void this.prefs.forgetTab(tab);

    // 盘上顺序里的那一格只在这里摘（还没到的那些在落盘时并回去，见 `TabStore.mergedOrder`）
    this.store.savedOrder = this.store.savedOrder.filter((s) => s !== sessionId);
    // 让后端 event_replay 把这个 session 的历史也丢掉
    forgetSession(sessionId);
    this.refreshTabBar(); // 出组可能让组没了
  }

  /**
   * 切到上 / 下一个 Tab。delta=+1 下一个、-1 上一个。环回。
   * 快捷键 Ctrl+Tab / Ctrl+Shift+Tab 用。
   */
  cycleActive(delta: 1 | -1): void {
    const targetId = this.router.cycleTarget(delta);
    if (targetId !== null) this.switchTo(targetId);
  }

  /**
   * 跳到第 N 个 Tab（1-indexed，issue #5 快捷键 Ctrl+1..9 用）。
   * N 大于现有 Tab 数 → 静默忽略；N 对应 Tab 已经 active → 无操作。
   */
  jumpToIndex(oneBasedIdx: number): void {
    const targetId = this.router.indexTarget(oneBasedIdx);
    if (targetId !== null) this.switchTo(targetId);
  }

  /**
   * issue #11: 任务快照落账（来源是 {@link refreshTasks} 重问回来的成品）——总是更新内存 map（即使 Tab 还没建），
   * 只有 sid 是当前 active 时才推全局 panel 重渲染。
   *
   * 不需要 "Tab 不存在就丢弃"——task 文件先于 jsonl 出现是合法时序，
   * 之后 ensureTab 时会从 tasksBySid 拿数据；fetchSessionTasks 拿到的也是同样数据。
   */
  /**
   * 那台机器说这几个会话的任务变了（`all` ⇒ 那台的每个 tab 都重问：期间可能漏了）⇒ 重问 `tasks-list`、交 {@link updateTasks}。
   * 只问手里有 tab 的会话（没有 tab 的变更，建 tab 那一刻本来就会问一次）。
   */
  refreshTasks(origin: Tab["origin"], sids: readonly string[], all: boolean): void {
    const want = new Set(sids);
    for (const t of this.store.tabs.values()) {
      if (t.origin !== origin || (!all && !want.has(t.sessionId))) continue;
      const sid = t.sessionId;
      void fetchSessionTasks(sid, origin).then((tasks) => this.updateTasks(sid, tasks));
    }
  }

  updateTasks(sessionId: string, tasks: TaskEntry[]): void {
    this.store.tasksBySid.set(sessionId, tasks);
    if (this.store.activeId === sessionId) {
      this.tasksPanel?.setSession(sessionId, tasks);
    }
  }

  /**
   * v2.4 issue #2：把 behavior config 应用到 TabManager。
   * 启动时由 main.ts 调一次拉初值；设置面板 toggle 改了也调一次同步。
   */
  applyBehavior(cfg: BehaviorConfig): void {
    this.router.applyBehavior(cfg);
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
    // 五道跳过条件（批期 · 开关 · 5s 手动保护 · 没有 / 已归档 · 已经是它）住路由（`tab-router.ts::autoFollow`）。
    const decision = this.router.autoFollow(sessionId);
    if (decision === "ignore") return;
    // 已经在这个 tab（`front-only`）但用户开了"拉前 monitor"也照拉
    if (decision === "switch") this.switchTo(sessionId, "auto");
    if (this.router.bringMonitorToFront) bringMonitorToFront();
  }

  /** 快捷键 Ctrl+` ：把当前活跃 Tab 对应的终端窗口拉到前台（live 本地 / 远端均可） */
  bringActiveTerminalToFront(): void {
    // 非 Windows 上 ↗ 的最后一跳是桩（每点必败）⇒ 按钮不渲；
    //   快捷键 / 命令面板（住 `main.ts`）还够得到这里 ⇒ 说一句实话，不发 IPC。见 `terminal-front.ts`。
    if (!terminalFrontAvailable()) {
      toast(TERMINAL_FRONT_UNAVAILABLE_TITLE, TERMINAL_FRONT_UNAVAILABLE_DETAIL, {
        level: "info",
      });
      return;
    }
    if (!this.store.activeId) return;
    const tab = this.store.tabs.get(this.store.activeId);
    // 还有终端可去（活着，或可重连：登录 shell 的 ssh 窗还在）才拉。
    if (!tab || !hasTerminal(tab.state)) return;
    void this.frontFrom(this.store.activeId, "head");
  }

  /** ↗ 浮层的［接上终端］（主窗口接到设置窗那一节）。 */
  onConnectTerminal: (() => void) | null = null;

  /** ↗ 的锚：行尾那颗（从行上点的）或会话头那颗（别的入口；会话头没在画它 ⇒ 退到行尾那颗）。 */
  private frontAnchor(sid: string, from: "row" | "head"): HTMLElement | null {
    const row = this.bar.frontButton(sid);
    if (from === "row") return row;
    const head = [...document.querySelectorAll<HTMLElement>("[data-role=head-front]")].find((b) => b.dataset.sid === sid);
    return head ?? row;
  }

  /**
   * ↗ 的每个入口都经这里：在飞时不重发；慢了按钮进「进行中」、旁边出「查找终端…」；
   * 切过去了 ↗ 换对勾 1 秒；其余结局一个浮层锚在 ↗ 下面（`front-result.ts` 排版）。
   */
  private async frontFrom(sid: string, from: "row" | "head"): Promise<void> {
    const tab = this.store.tabs.get(sid);
    if (!tab) return;
    const origin = tab.origin;
    const run = (): Promise<FrontResult> => (isRemoteOrigin(origin) ? bringRemoteTerminalToFront(origin, sid) : bringTerminalToFront(sid));
    const r = await frontOnce(sid, run, (on) => {
      this.bar.setFrontPending(sid, on);
      setFrontBusy(this.frontAnchor(sid, from), on);
    });
    if (r === undefined) return;
    const anchor = this.frontAnchor(sid, from);
    const now = this.store.tabs.get(sid);
    const view = frontView(r, machineName(origin), now ? inTmux(now.state) : false);
    if (view === null) {
      closeFrontResult(sid);
      flashFrontDone(anchor);
      return;
    }
    if (!anchor) return;
    showFrontResult(anchor, sid, view, (a) => this.frontAct(sid, from, origin, a));
  }

  /** ↗ 浮层上的按钮。 */
  private async frontAct(sid: string, from: "row" | "head", origin: Origin, a: FrontAct): Promise<void> {
    switch (a.kind) {
      case "connect":
        this.onConnectTerminal?.();
        return;
      case "open-in-terminal":
        await this.menu.attachRemote(sid);
        return;
      case "copy":
        // 浮层里那颗自己复制（`kit/detail.ts`），不经这里。
        return;
      case "update":
      case "retry-update":
        await this.updateInPlace(sid, from, origin);
        return;
      case "retry":
        await this.frontFrom(sid, from);
        return;
      case "reconnect":
        this.reconnect(origin);
        return;
    }
  }

  /**
   * ↗ 浮层的［更新］：就地把这一版换到那台（部署住 `backend-deploy.ts`，与机器卡同一处），不开设置；
   * 正在更新 · 换上了 · 没换上都在同一个浮层里说。换上了就重拨那条流：拨成了只留关闭；没拨成才说要重连、给［重新连接］。
   * 没换上 ⇒ ［重试］再更新一次 ·［复制详情］。确认框里取消 ⇒ 什么都不出。
   */
  private async updateInPlace(sid: string, from: "row" | "head", origin: Origin): Promise<void> {
    const machine = machineName(origin);
    const show = (view: FrontView): void => {
      const anchor = this.frontAnchor(sid, from);
      if (anchor) showFrontResult(anchor, sid, view, (a) => this.frontAct(sid, from, origin, a));
    };
    const view = (title: string, body: string, tone: FrontView["tone"], acts: FrontAct[]): FrontView => ({ title, body, hint: null, tone, acts });
    let done: string | null;
    try {
      done = await updateBackendOf(origin, machine, () => show(view(copyText("front.title.updating", { machine }), "", "grey", [])));
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e);
      show(view(copyText("front.title.updateFailed", { machine }), detail, "red", [{ kind: "retry-update" }, { kind: "copy", detail }]));
      return;
    }
    if (done === null) return;
    const title = copyText("front.title.updated", { machine });
    try {
      await this.actions.redial(origin);
      show(view(title, done, "grey", []));
    } catch (e) {
      console.warn(`[front] ${origin} 换上之后重拨没成：`, e);
      show(view(title, `${done} ${copyText("front.body.redialFailed")}`, "amber", [{ kind: "reconnect" }]));
    }
  }

  /** 快捷键 Ctrl+F（`session.find`）：当前 tab 的查找面板打开到「搜索」。实现在流视图。 */
  openFind(): void {
    this.view.openFind();
  }

  /** `Alt+↑` / `Alt+↓`：上 / 下一轮（实现在流视图）。 */
  stepTurn(dir: -1 | 1): void {
    this.view.stepTurn(dir);
  }

  /** `Ctrl+O` · 会话头「⋯」：过程默认展开 / 收起（每扇窗一份，所有 tab 一起换）。 */
  toggleProcessDefault(): void {
    this.view.toggleProcessExpanded();
  }

  /** 快捷键 Ctrl+Shift+E：打开当前活跃 Tab 的工作目录到系统文件管理器 */
  openActiveTabCwd(): void {
    if (!this.store.activeId) return;
    void this.openTabCwd(this.store.activeId);
  }

  /** issue #10 快捷键 Ctrl+Shift+N：把当前活跃 Tab 在独立只读窗口打开 */
  openActiveInNewWindow(): void {
    if (this.store.activeId) void this.openInNewWindow(this.store.activeId);
  }

  /** 起新会话：那台回了「起好了 / 开窗」⇒ 长出一个占位标签页、主区换成它（报到了换成真的）。 */
  /**
   * 长一行占位标签页；认的是 sid、而那个会话已有标签页（恢复一条已结束的）⇒ 不长，直接切过去，回 `false`（没接）。
   */
  addLaunchSlot(spec: SlotSpec): boolean {
    if ("sid" in spec.match && this.store.tabs.has(spec.match.sid)) {
      this.switchTo(spec.match.sid);
      return false;
    }
    this.slots.add(spec);
    return true;
  }

  /** 这一键此刻要不要让开：占位标签页那一页显着、而它作用于当前会话（底下那个真标签页不是你看着的那个）。 */
  shadowedBySlot(id: ActionId): boolean {
    return this.slotOver && TabManager.ACTS_ON_ACTIVE_SESSION.has(id);
  }

  /** 只给判据用：占位标签页此刻几个 · 正看着哪一个。 */
  debugSlots(): { ids: number[]; showing: number | null } {
    return this.slots.debug();
  }

  /** account-ux U8：当前活跃会话 sid（只读投影，供 Ctrl+K / 快捷键判定"对当前会话做某事"）。 */
  activeSessionId(): string | null {
    return this.store.activeId;
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
    if (!this.store.tabs.has(sessionId)) return;
    // 正看着占位标签页：自动跟随不把人拽走；手动切 ⇒ 那一页收起（切回原来那个标签页也算）。
    if (this.slots.isShowing()) {
      if (source === "auto") return;
      this.slots.hide();
    }
    if (this.store.activeId === sessionId) return;

    // 切 active 走 .active class（CSS visibility 控制），避免 display:none/block
    // 触发整棵子树重建 layout tree 卡顿。详 styles.css 的 .stream 注释。
    this.view.showOnly(sessionId);
    const next = this.store.tabs.get(sessionId);
    if (next) next.unread = 0;
    this.store.activeId = sessionId;
    if (next) this.view.activate(next);
    // Batch5-F19 记住所在 tab ＋ 手动切的 5s 保护（住路由：`tab-router.ts::noteSwitched`）。
    this.router.noteSwitched(sessionId, source);
    this.refreshTabBar(); // active 高亮 + badge 立即更新（廉价，不阻塞）

    // 回到离开时的位置：切走时贴着底的才贴底（往上翻着看的不拽到底 —— 后台 tab 只是 `visibility:hidden`，滚动位置一直在）。
    // 贴底（读 scrollHeight，强制同步 reflow）与面板整表 re-render 推到下一帧：让 `.active` 的切换先画出来；期间又切走则跳过。
    const stuck = next?.stream.stuckToBottom ?? true;
    // 调度：合批 —— 切 Tab 后贴底与面板整表重画推到下一帧，期间切走则跳过
    requestAnimationFrame(() => {
      if (this.store.activeId !== sessionId) return;
      if (stuck) next?.stream.scrollToBottom();
      // 第二帧再贴一次（只对贴底的）：刚从 `visibility:hidden` 翻出来的卡还带着 `content-visibility: auto`
      // 的估值几何，第一帧算出来的 `scrollHeight` 不准；下一帧材料化成真实尺寸再落一次才准。
      if (stuck) {
        // 调度：一次性 —— 贴底的第二帧（卡材料化成真实尺寸后再落一次），回调里不再排
        requestAnimationFrame(() => {
          if (this.store.activeId !== sessionId) return;
          this.store.tabs.get(sessionId)?.stream.scrollToBottom();
        });
      }
      // issue #11: 切换 task panel 数据源到新 active Tab 的 sid
      this.tasksPanel?.setSession(sessionId, this.store.tasksBySid.get(sessionId) ?? []);
      // issue #23: agents 面板同步切到新 active Tab
      this.agentsPanel?.setSession(sessionId, this.live.board.of(sessionId));
      // F88b：HUD context% chip 切到新 active 会话的最新 usage（无带 usage 记录 → null → 隐藏）
      this.publishActive(); // 当前 tab 那一格（usage · 要不到的原因）
    });
  }




  private refreshTabBar(): void {
    // ★ 6d（条 54）：**拖拽进行中不重排 tab 栏。**
    //
    // `refreshTabBar` 挂在活动路上（`updateActivity` / `archiveTab` / `ensureTab` 末尾都
    // 无条件调它），而这些事件在拖拽那一两秒里照常来。下面第 4 段那个排序循环一跑，
    // **指针底下的 tab 就被换掉了** —— 用户松手落到的不是他瞄的那一格。
    // 会改 DOM 顺序的活动事件：新 tab 到达 ⇒ `placeInOrder` 按盘上那份顺序把它从**中间**插进去
    // （6d 注入的就是这一形）。原先这里列的第一条「会话跑完 ⇒ 归档 ⇒ tab 离开 `barEl`」
    // 随归档抽屉删了：会话结束今天只改那颗按钮的 class，tab 留在原位。
    // 原先还有「bg 挂到宿主之后」那一路，树删了。
    //
    // ⚠ 守的是 `d.dragging`（真起拖了）而不是 `this.drag` 在不在 —— 后者在「按下还没动」
    // 那一段也为真，那段本来就该照常刷新（它与点击没有区别）。
    // ⚠ 不是丢掉这次刷新：记一笔脏，`teardownDrag` 收尾时补一次（见那里）。
    if (this.dragger.deferRefresh()) return;
    // tab 没了就从多选里掉出去（与整刷删按钮同一拍）。
    this.selection.retain((sid) => this.store.tabs.has(sid));
    this.bar.refresh();
  }

}

// P5.2 B 重构：markCardUuid + feedBranchFolder 已搬到 render-stream-record.ts
// （三 caller 共用 renderStreamRecord 函数内部调用）。tabs.ts 不再持有这两个 helper。
