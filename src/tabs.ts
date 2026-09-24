/**
 * 〔U2 · 第三波 · `设计/01 §1.5`「一个 store，一个 router」· `设计/99 §4.22.0`〕**拆分地图**。
 *
 * 拆之前（基线 `aede6f5d`，4991 行）本文件是**一个类干五件事**。现打逐件（谁在调 → 拆到哪）：
 *
 * | # | 这件事 | 谁在调（生产） | 拆到 |
 * |---|---|---|---|
 * | ① | **会话状态账**：tab 集合 · 顺序 · 当前 tab · 早于 tab 到达的信号暂存（归档/灰灯/红绿灯）· 账号快照 · 任务快照 · 「变了」那一份订阅 | `main.ts` / `entry-viewer.ts` 把 `events.ts` 的事件喂进来（`onLine` · `archiveTab` · `reviveTab` · `markTmuxIdle` · `updateActivity` · `updateTasks` · `createSkeletonTab` · `setSessionAccounts`）；`main.ts` 读投影（`snapshotSessions` · `peekSession` · `hasTab` · `activeRepoInfo` · `touchedFilesFor` · `getActiveSubagentContext` · `activeSessionId`） | `tab-store.ts`（store）· `tab-model.ts`（`Tab` 形状与标题）· `tab-session-facts.ts`（从记录里抽事实） |
 * | ② | **路由**：切到哪个 tab、谁有权切（手动 5s 保护 · 自动跟随）、记住上次的 tab | `main.ts` 快捷键 / 命令面板 / 启动选 active（`switchTo` · `cycleActive` · `jumpToIndex` · `applyBehavior` · `persistLastActive` · `onManualSwitch`）；`onLine` 里真用户输入（`userActive`） | `tab-router.ts` |
 * | ③ | **实时流视图**：每个 tab 的流 DOM、按 seq 门控建卡、尾部窗口 / 骨架 / 上翻补批 / 哨兵 / 大纲、重放批 | `events.ts` → `onBatchStart` · `onLine` · `onBatchEnd`；`main.ts` DEV 探针 `debugSnapshot` | `tab-stream-view.ts` |
 * | ④ | **tab 栏视图**：按钮 · 徽章 · 分组 · 拖动排序与成组 · 固定 · 顺序落盘 | 用户手势；`main.ts` 启动 `loadCollections` · `loadPinned` · `loadOrder` | `tab-bar-view.ts` · `tab-bar-drag.ts` · `tab-drop.ts`（纯落点算术）· `tab-bar-prefs.ts`（集合 / 固定 / 顺序三份落盘） |
 * | ⑤ | **会话动作**：右键菜单（resume · 换号重启 · attach · 预览 · 杀会话 · 集合 · 固定）与它背后的 IPC（开目录 · 新窗口 · 切到终端窗口） | 用户右键；`main.ts` 快捷键 / 命令面板（`bringActiveTerminalToFront` · `openActiveTabCwd` · `openActiveInNewWindow` · `closeActiveIfArchived`） | `tab-menu.ts`（菜单项怎么组）· `tab-context-menu.ts`（菜单这个控件）· `tab-session-actions.ts`（动作本身 ＋ 本层唯一直呼 `invoke` 的一份） |
 *
 * 本文件拆完只剩 `TabManager` 这个**组装根**：对外 API（`main.ts` / `entry-viewer.ts` 调的那些）
 * 逐字不变，事件怎么在上面几份之间流转写在这里。拆分逐子步提交，每一步 `tabs.vitest` 全绿、断言不动。
 */
import { isCompactRecord } from "./cards";
import { runForkFlow } from "./fork-flow"; // G6：分叉完把新会话起起来（E78 起连反馈也在里面）
import type { BranchResult } from "./generated/BranchResult";
import { fetchSessionTasks, type TaskEntry, type TasksPanel } from "./tasks-panel";
import type { JsonlLinePayload } from "./events";
import {
  sessionBadge,
  shouldShowAccountBadge,
  detectAccountMismatch,
  type SessionAccount,
} from "./accounts";
import { accountAvatarEl } from "./account-color";
import type { BehaviorConfig } from "./behavior";
import { showActionFailureToast } from "./error-toast";
import { TailWindow } from "./live-window";
import type { AgentsPanel } from "./agents-panel";
import { LS_KEYS, safeSet } from "./local-storage";
import {
  collectionOf,
  deleteCollection,
  getCollections,
  newCollectionId,
  renameCollection,
  setCollections,
  type TabCollection,
} from "./tab-collections";
import {
  getPinned,
  getTabOrder,
  isDegradedPin,
  setPinned,
  setTabOrder,
  type PinnedTab,
} from "./tab-bar-state";
import { turnEndNotifier } from "./turn-notify";
import { activityLightClass, type GridSessionSnapshot, type SessionPeek } from "./session-status";
import { contextPercent } from "./views/context-limit";
import {
  terminalFrontAvailable,
  TERMINAL_FRONT_UNAVAILABLE_TITLE,
  TERMINAL_FRONT_UNAVAILABLE_DETAIL,
} from "./terminal-front";
import { computeTitleFor, type Tab, type TabsSummary } from "./tab-model";
// 〔U2〕`Tab` 的形状与标题函数搬去了 `tab-model.ts`；这里原样 re-export，既有 import 面零改动。
export type { Tab, TabStatus, TabsSummary } from "./tab-model";
import {
  applyDropToCollections,
  collectionsEqual,
  defaultGroupName,
  moveTabBlock,
  pickDropTarget,
  tabUnderY,
  DWELL_MS,
  DWELL_MOVE_PX,
  type DropTarget,
  type TabRect,
} from "./tab-drop";
// 〔U2〕落点算术搬去了 `tab-drop.ts`；原样 re-export，`tabs.vitest.ts` 的 import 面零改动。
export {
  moveTabBlock,
  pickDropTarget,
  tabUnderY,
  commonDirName,
  defaultGroupName,
  applyDropToCollections,
  collectionsEqual,
  DWELL_MS,
  DWELL_MOVE_PX,
} from "./tab-drop";
export type { DropTarget, TabRect } from "./tab-drop";
import { TabMenu } from "./tab-menu";
import { TabStore } from "./tab-store";
import { TabStreamView } from "./tab-stream-view";
import {
  abortRunningAgents,
  noteAgents,
  noteForkedFrom,
  noteTouchedFiles,
  noteUsage,
} from "./tab-session-facts";
import {
  TabSessionActions,
  bringMonitorToFront,
  bringRemoteTerminalToFront,
  bringTerminalToFront,
  e2eLog,
  forgetSession,
  listSessionActivity,
} from "./tab-session-actions";

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
interface TabButtonRefs {
  root: HTMLButtonElement;
  label: HTMLSpanElement;
  badge: HTMLSpanElement;
  /** A3：账号徽章（该会话属于哪个账号；本地会话不显示，未知显 —）。 */
  acctBadge: HTMLSpanElement;
  cwdBtn: HTMLSpanElement;
  /**
   * 〔步 17·B〕📌 角标。**纯展示，不可点** —— 与 `.tab-badge`（未读数）同族。
   * 固定是右键菜单那一项的事；这里再放一个能点的东西就是同一个动作两个入口。
   */
  pinBadge: HTMLSpanElement;
}

import {
  findClaudeTmuxMatches,
  findClaudeTmux,
  findIdleTmux,
  isCwdFallbackMatch,
  claudeExited,
  type TmuxSession,
} from "./tmux-sessions";
// G6：tmux↔sid 判据搬进叶子模块 `tmux-sessions.ts`（`fork-flow.ts` 也要用，而它被本文件
// import ⇒ 留在这里会成环）。**原样 re-export**，既有 import 面零改动。
export {
  findClaudeTmuxMatches,
  findClaudeTmux,
  findIdleTmux,
  isCwdFallbackMatch,
  claudeExited,
};
export type { TmuxSession };

export class TabManager {
  /**
   * 〔U2 · ①〕**会话状态账住 `tab-store.ts`** —— tab 集合 · 顺序 · 当前 tab · 早到信号暂存 · 账号快照 ·
   * 任务快照，外加「变了」那唯一一份订阅。本类与拆出去的几份都读写同一个实例。
   */
  private readonly store = new TabStore();

  // ── 〔U2〕判据探针：这几样拆之前是本类的私有字段，`tabs.vitest.ts` 按名字直读（`activeId` 还直写）。
  //    值住 store，这里只是同名别名；`protected` 只为不让 `noUnusedLocals` 把「只被判据读」的访问器当死代码。
  protected get tabs(): Map<string, Tab> {
    return this.store.tabs;
  }
  protected get activeId(): string | null {
    return this.store.activeId;
  }
  protected set activeId(v: string | null) {
    this.store.activeId = v;
  }
  protected get orderedIds(): string[] {
    return this.store.orderedIds;
  }
  protected get pendingArchive(): Set<string> {
    return this.store.pendingArchive;
  }
  protected get pendingTmuxIdle(): Set<string> {
    return this.store.pendingTmuxIdle;
  }

  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  private tabButtons = new Map<string, TabButtonRefs>();
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
    /**
     * 〔步 17·D〕松手时的落点。**三种语义**（`§D.3`）——在这之前这里是
     * `dropBefore?: string | null`（只有「插到谁之前」与「末尾」两种）。只在**未 armed** 时有意义。
     */
    dropTarget: DropTarget;
    /** 〔步 17·D〕指针此刻压着谁（停留计时的对象）。`null` = 没压在任何 tab 上。 */
    dwellSid: string | null;
    /** 〔步 17·D〕停留已经攒满的那个 sid。计时器到点才写，抖动 / 换目标即清回 `null`。 */
    dwellArmed: string | null;
    /** 〔步 17·D〕本轮停留的锚点。指针离它超过 `DWELL_MOVE_PX` 就重新计时。 */
    dwellX: number;
    dwellY: number;
    /** 〔步 17·D〕停留计时器句柄。`teardownDrag` 必须清 —— 否则它会在拖拽结束后才到点。 */
    dwellTimer: number | null;
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
   * ★ 6d：拖拽进行中有人要求刷 tab 栏 —— 记一笔，`teardownDrag` 收尾时补一次。
   * 只是一个「有没有」，不记是谁要求的：`refreshTabBar` 本来就是整栏重刷，补一次就够。
   */
  private tabBarDirtyDuringDrag = false;
  /**
   * 拖拽撕离阈值：指针移动超过此像素才判定为"拖"，否则视为普通点击。
   */
  private static readonly DRAG_THRESHOLD_PX = 6;
  /**
   * 拖拽结束后需抑制掉紧随 mouseup 的那次 click 的 sid（避免拖完又误切 Tab）。
   * null = 不抑制。click handler 命中后清零（一次性）。
   */
  private suppressClickSid: string | null = null;

  /**
   * P7a-1（#61）：**归档区**。`#61` 正文自陈「状态机已经有了，缺的是那个「口」」——
   * 那个口就在 [`refreshTabBar`]，它是全仓**唯一**把 tab 按钮塞进 `barEl` 的地方。
   *
   * 三个元素由本类自己建（不改构造签名：那有两个生产调用点 + 一批夹具），
   * 挂在 `barEl` 之后，作为它的兄弟。
   */
  /** P7a-3（#61）：标签页集合。**零自动归组**〔用 08-11「纯手动」〕。 */
  private collections: TabCollection[] = [];
  /**
   * P7a-3 E 阶段补审：**这个实例拉过集合没有。**
   *
   * 撕离出来的 viewer 窗口也用 `TabManager`（`main.ts:938`，tab 栏由 `.viewer-mode` 隐藏），
   * 但它**从不 `loadCollections`** ⇒ `collections` 恒空。右键菜单里若还留着「新建集合…」，
   * 点一下就把「只含这一个」的列表写回 `config.json` —— **用户已有的集合全没了**。
   *
   * 同族先例就在旁边一行：「viewer 窗口共享 localStorage，**禁写 last-active**（防污染主窗口记忆）」。
   * ⇒ 没拉过就不给入口。这不是把功能藏起来，是**没有那份真相就没有资格改它**。
   */
  private collectionsLoaded = false;
  /** 每个集合在主栏里的容器（组头 + 成员列表）。 */
  private groupEls = new Map<string, { wrap: HTMLElement; head: HTMLElement; list: HTMLElement }>();
  /**
   * 〔步 17·B〕**这个实例拉过固定表没有** —— 与 `collectionsLoaded` 同一条理由，
   * 而且这里更要命：`persistPinned` 是**按当前 tab 重算整张表**写回去的，
   * 没拉过就写 ⇒ 用户上次固定的全没了。撕离出来的 viewer 窗口正是这种实例。
   * ⇒ 没拉过就不给入口（右键菜单里那两项不出现），也不落盘。
   */
  private pinnedLoaded = false;
  /**
   * 〔步 17·B〕`loadPinned` 那一趟从盘上读到的记录（sid → 条目）。
   *
   * 🔴 **它不是「谁被固定了」的真相** —— 那件事的唯一住址是 `Tab.pinned`
   * （一个事实一个住址）。这里存的是**盘上那份记录的内容**，只有两个用途：
   * ① `§B.6` 第一格的降级判定（`jsonlPath` 为空的那些，点进去要说人话）；
   * ② `lastActiveAt` 的沿用 —— 已经灰了的 tab 什么时候最后活动过，前端没有这个数，
   *    不许在每次落盘时把它刷成 `Date.now()`（那是把「说不清」写成一句假话）。
   */
  private pinnedRecords = new Map<string, PinnedTab>();
  /**
   * 〔步 17·B〕复活出来的固定 tab 的**空态提示**（sid → 元素）。
   *
   * 🔴 它是「**不许留一个点了没反应的 tab**」这条纪律的落点：复活的 tab 里
   * 一条内容都没有（`99 §2.5 P3` 裁定「已结束的会话点进去不能看内容，只能 resume」），
   * 不放点东西进去，用户点它就是一片空白 —— 那与坏了没有区别。
   * 复活成 live（真 resume 上了）时摘掉。
   */
  private pinHintEls = new Map<string, HTMLElement>();


  /**
   * 〔U2 · ③〕实时流视图住 `tab-stream-view.ts`。**在构造体里建，不写成字段初始化**：
   * 它要 `streamRootEl`，而字段初始化在参数属性赋值之前跑（esbuild 出原生 class field 时就是这个序），
   * 写成初始化器会拿到 `undefined`。
   */
  private readonly view: TabStreamView;

  constructor(
    private barEl: HTMLElement,
    streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI。〔U2〕它是 store 那一份订阅的第一个订阅者。 */
    onTabsChanged?: (summary: TabsSummary) => void,
    /** issue #11: 全局 TasksPanel，切 Tab / 收事件时由 TabManager 喂数据 */
    private tasksPanel?: TasksPanel,
    /** issue #23: 全局 AgentsPanel（subagent 列表 + 各自状态灯），喂数方式同 tasksPanel */
    private agentsPanel?: AgentsPanel,
  ) {
    if (onTabsChanged) this.store.subscribe(onTabsChanged);
    this.view = new TabStreamView(this.store, streamRootEl, {
      onLine: (payload) => this.onLine(payload),
      refreshTabBar: () => this.refreshTabBar(),
      scheduleTabBarRefresh: () => this.scheduleTabBarRefresh(),
      applyAiTitle: (tab, aiTitle) => this.applyAiTitle(tab, aiTitle),
      userActive: (sid) => this.userActive(sid),
      startForkedSession: (tab, res) => this.startForkedSession(tab, res),
    });
  }

  // ── 〔U2〕判据探针（流视图那一份）：`tabs.vitest.ts` 按名字直调 `updateSentinel`、直读 `materializeQueue`。
  protected updateSentinel(tab: Tab): void {
    this.view.updateSentinel(tab);
  }
  protected get materializeQueue(): string[] {
    return this.view.materializeQueue;
  }

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON（形状、口径与秤 6 的三个账本见 `tab-stream-view.ts` 那一份）。
   * 生产不接线,方法本身无副作用。
   */
  debugSnapshot(): string {
    return this.view.debugSnapshot();
  }

  /**
   * 〔U2 · ⑤〕会话动作住 `tab-session-actions.ts`（tab 层唯一直呼 `invoke` 的一份）。它只要宿主给四样读数 / 回调。
   */
  private readonly actions = new TabSessionActions({
    tab: (sid) => this.store.tabs.get(sid),
    isAttachable: (sid) => this.isAttachable(sid),
    sessionAccount: (sid) => this.store.sessionAccountsByS.get(sid),
    refreshAccountBadgeFor: (sid) => this.refreshAccountBadgeFor(sid),
  });

  /** 〔U2 · ⑤〕右键菜单里放哪几项住 `tab-menu.ts`；点下去做事直接交给上面那份 `actions`。 */
  private readonly menu = new TabMenu(
    {
      tab: (sid) => this.store.tabs.get(sid),
      isAttachable: (sid) => this.isAttachable(sid),
      collectionsLoaded: () => this.collectionsLoaded,
      collections: () => this.collections,
      commitCollections: (next) => this.commitCollections(next),
      pinnedLoaded: () => this.pinnedLoaded,
      togglePin: (sid) => this.togglePin(sid),
      requestPanoramaHighlight: (sid) => this.requestPanoramaHighlight?.(sid),
    },
    this.actions,
  );

  // ── 〔U2〕判据探针：拆之前这几个是 `TabManager` 的私有成员，`tabs.vitest.ts` 按名字直调 / 直读。
  //    搬家之后留一层同名转交（不加 `async`、不包一层 `await` —— 那会多一拍微任务，
  //    而有几条 DOM 判据只放行一个微任务）。`protected` 只是为了不让 `noUnusedLocals`
  //    把「只被判据读」的成员当死代码；它们不是对外 API。
  protected get tmuxCache(): Map<string, { ts: number; sessions: TmuxSession[] | null }> {
    return this.actions.tmuxCache;
  }
  protected get restartingSids(): Set<string> {
    return this.actions.restartingSids;
  }
  protected fetchTmuxFresh(origin: string): Promise<TmuxSession[] | null | undefined> {
    return this.actions.fetchTmuxFresh(origin);
  }
  protected resumeTab(sid: string, accountName?: string, useBase?: boolean): Promise<void> {
    return this.actions.resumeTab(sid, accountName, useBase);
  }
  protected resumeTabTmux(sid: string, accountName?: string, useBase?: boolean): Promise<void> {
    return this.actions.resumeTabTmux(sid, accountName, useBase);
  }
  protected restartTabWithAccount(
    sid: string,
    accountName: string,
    compactFirst: boolean,
    confirmFn?: (msg: string) => boolean,
  ): Promise<boolean> {
    return this.actions.restartTabWithAccount(sid, accountName, compactFirst, confirmFn);
  }
  protected awaitCompactFor(sid: string, timeoutMs?: number): () => Promise<boolean> {
    return this.actions.awaitCompactFor(sid, timeoutMs);
  }
  protected killRemoteTmux(
    origin: string,
    tmuxName: string,
    viaCwd: boolean,
    opts?: { confirm?: (message: string) => boolean; idle?: boolean },
  ): void {
    this.actions.killRemoteTmux(origin, tmuxName, viaCwd, opts);
  }
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
    const active = this.view.batchEnd();
    // F88b：批期 trackUsage 只更了 tab 字段没喂 chip，这里对活跃 tab 单次 flush 到 HUD
    // （批内多条 assistant 记录只刷一次，消视觉抖动）。
    this.onActiveUsageChanged?.(active?.latestModel ?? null, active?.latestPromptTokens ?? null);
  }

  /**
   * G6：分叉产出新会话文件之后 —— **起它**。
   *
   * 「不杀旧会话、起新会话」是用户对这个功能的原话，所以这里对源会话一个字都不碰：
   * 只查它的事实（活没活、哪个账号、在哪个 tmux），拿去给新会话配参数。
   * 两个调用点（批量渲染 / 逐行渲染）走同一条路，行为不许分裂。
   */
  private async startForkedSession(tab: Tab, res: BranchResult): Promise<void> {
    // E78：查事实、起会话、反馈**全在 `runForkFlow` 里** —— 这里只说「我是谁 + 分叉结果」。
    // 此前这三步在本文件与 `session-viewer.ts` 各写一遍（连 toast 文案都是逐字重复的双写点）。
    await runForkFlow({
      origin: tab.origin,
      newSessionId: res.sessionId,
      sourceSessionId: tab.sessionId,
      cwd: tab.cwd,
    });
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

    // SSH 重连后远端后端从 seq 0 重发该 session 整段 jsonl → 按 seq 去重。必须在
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

    // 〔SE1〕大纲：只记一笔「这份会话又长了」（清单问后端要，这里不判、不攒）。
    this.view.noteOutlineLine(tab);

    // A5：换号重启的 compact 完成检测。仅当有该 sid 的等待者才判（常态零开销）：见 compact 摘要
    // 行即 resolve 该等待者（换号重启编排随即从 compact 步进入 kill 步）。
    if (this.actions.hasCompactWaiters()) {
      this.actions.settleCompact(payload.session_id, () => isCompactRecord(payload.message));
    }

    // issue #63①：首条带 forkedFrom 的记录 → 锁定血缘、给 tab 标题加 `↳` 徽标(与原会话区分)。
    // 放在双重去重之后、turnEndNotifier 之前——这样首条即含徽标的标题也进 turn-end 通知(审计 建议)。
    this.applyForkedFrom(tab, payload.message);

    // Batch14-F42：turn-end 系统通知。放在双重去重之后（重投行不重报）、
    // 渲染管线之前（通知与渲染/收纳互相独立）。批量重放由 inBatch 短路。
    turnEndNotifier.observe(payload.session_id, tab.title, payload, this.store.inBatch);

    // issue #23（第二增量）：配对 agent 工具调用，喂 AgentsPanel
    this.trackAgents(tab, payload.message);

    // F88b：捕获带 usage 的 assistant 记录 → 更新本会话最新 prompt token+model（供 HUD context%）。
    // 与 trackAgents 同处（双重去重之后，重投不重复累）。活跃会话则即时推给 HUD。
    this.trackUsage(tab, payload.message, payload.seq);

    // F70：累进本会话改动集（写类工具 file_path）——放在双重去重之后（重投不重复累），
    // 渲染/收纳门控之前（连"收纳不建卡"的记录也计入）。纯增量、无 DOM。近因序那条理由见 `noteTouchedFiles` 头注。
    noteTouchedFiles(tab, payload.message);

    this.view.ingest(tab, payload);
  }

  /**
   * Batch5-F18：骨架 Tab——活跃清单（本地 IPC / 远端 session_added 事件）一到
   * 即建，不等首条内容行。复用 ensureTab 全部语义：cwd 以 MAX_SAFE_INTEGER 的
   * seq 记入 → 任何真实行的 cwd（更小 seq）照常覆盖为项目根；parentPath 空由
   * 首条行回填；pendingArchive/pendingActivity 落实、batch 模式继承均沿用。
   * 已存在同 sid Tab 时为 no-op（幂等，重连重发 session_added 无害）。
   */
  createSkeletonTab(
    sessionId: string,
    cwd: string | null,
    origin: string | null,
    kind: string | null = null,
    name: string | null = null,
    // E73：`null` = 没说（旧 backend / 存量会话）= 视为可以。只有显式 `false` 才记账。
    attachable: boolean | null = null,
  ): void {
    if (attachable === false) this.store.notAttachableSids.add(sessionId);
    else this.store.notAttachableSids.delete(sessionId);
    this.ensureTab(sessionId, cwd, "", Number.MAX_SAFE_INTEGER, origin, kind, name);
  }

  /**
   * E73：attach / 「杀死空 tmux」这几个动作对这个会话有没有意义。
   * 〔第二波 T4〕`↗` 不再看它：`设计/80 §8.7` 步 4 把「有没有终端」的四套判断收成一句
   * 「这个 sid 有没有启动令牌」，那一句住后端（`bind.rs::resolve_remote_front`）。
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
   * Batch15-P2：活跃 tab 的仓信息（cwd + origin），供全景视图判断索引哪个本地仓。
   * 无活跃 tab / 活跃 tab 无 cwd → 返 null。origin!==null = 远端会话（代码在远端机，
   * 本地 code-picture 索引不到，全景侧据此显式提示不索引）。**additive getter，只读，不改既有逻辑。**
   */
  activeRepoInfo(): { cwd: string; origin: string | null } | null {
    const tab = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (!tab || !tab.cwd) return null;
    return { cwd: tab.cwd, origin: tab.origin };
  }

  /**
   * F70：某会话在全景图上可高亮的「改动集」——cwd（定仓）+ 它写类工具碰过的文件。
   * **本地会话专属**：origin!==null（远端）/ 无 cwd → 返 null（远端代码不在本机、code-picture
   * 索引不到，高亮不可用——门控就地做，呼应 activeRepoInfo）。**只读 getter，不落盘。**
   */
  touchedFilesFor(
    sid: string,
  ): { cwd: string; origin: null; files: string[] } | null {
    const tab = this.store.tabs.get(sid);
    if (!tab || !tab.cwd || tab.origin !== null) return null;
    return { cwd: tab.cwd, origin: null, files: [...tab.touchedFiles] };
  }

  /**
   * F91（#27）：跨会话监控快照——`GridMonitorView` 消费的**只读派生 DTO 列表**（本地 + 所有远端会话）。
   * 纯派生：不外泄任何内部 DOM / Map 引用（防外部改到 TabManager 内部状态）。插入序（同 tab-bar）。
   * context% 复用 pricing.ts `contextPercent`（上限未知 / 无 usage → null）。
   */
  /**
   * A3：喂入远端 live 探测的会话账号归属（来自 backend `--session-accounts`）+ 账号邮箱表。
   * main.ts 定期聚合各远端调用。喂完刷新所有 tab 的账号徽章。
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
    for (const [sid, refs] of this.tabButtons) {
      const tab = this.store.tabs.get(sid);
      if (tab) this.updateAccountBadge(refs, sid, tab);
    }
  }

  /**
   * F09（R7 语义反转）：账号徽章从"仅不一致时才显示的警示信号"改为"账号已知即恒显示的身份
   * 标识"——门（`shouldShowAccountBadge`）通过 + 账号已知（源③已知除外）就显示，不再要求
   * `detectAccountMismatch` 为真。旧版"不一致才显示"这条信息没有消失，只是从"触发显示的唯一
   * 条件"降级为"视觉区分的一个维度"：一致态也显示头像（用户能一眼看出这个会话归属哪个账号），
   * 不一致态仍用 tooltip 追加"与当前账号不一致"提示，且沿用既有 live 实心/last 幽灵区分
   * （不新增视觉语言）。⇄ 一键对齐按钮随对齐全套一并删除（见 features/F09-ui-convergence.md
   * §1"不做什么"——批量/一键对齐是组合层便利,不做等价替代,用户改走 flyout 逐会话操作）。
   */
  private updateAccountBadge(refs: TabButtonRefs, sid: string, tab: Tab): void {
    const hide = (): void => {
      refs.acctBadge.textContent = "";
      refs.acctBadge.className = "tab-acct-badge";
      refs.acctBadge.style.display = "none";
    };
    if (!shouldShowAccountBadge(tab.origin, this.store.accountReadyOrigins)) return hide();
    const b = sessionBadge(
      sid,
      tab.origin,
      this.store.sessionAccountsByS,
      this.store.accountEmailByName,
      this.store.accountLastByS,
    );
    if (!b || !b.account) return hide(); // 未知账号（源③）→ 退 hover；顺带把 b.account 窄化为 string
    refs.acctBadge.textContent = "";
    refs.acctBadge.className = "tab-acct-badge";
    refs.acctBadge.appendChild(accountAvatarEl(b.account, { size: 14, ghost: b.source === "last" }));
    const current = tab.origin ? this.store.currentByOrigin.get(tab.origin) ?? null : null;
    const mismatch = detectAccountMismatch(b.account, current);
    refs.acctBadge.title = mismatch ? `${b.tooltip} · 与当前账号「${current}」不一致` : b.tooltip;
    refs.acctBadge.style.display = "";
  }

  snapshotSessions(): GridSessionSnapshot[] {
    const out: GridSessionSnapshot[] = [];
    for (const tab of this.store.tabs.values()) {
      let running = 0;
      for (const a of tab.agents.values()) {
        if (a.status === "running") running += 1;
      }
      out.push({
        sessionId: tab.sessionId,
        title: tab.title,
        origin: tab.origin,
        cwd: tab.cwd,
        status: tab.status,
        tmuxIdle: tab.tmuxIdle, // audit-fixes F03.2：cell 灰灯与 tab-bar 同源
        activityStatus: tab.activity?.status ?? null,
        waitingFor: tab.activity?.waitingFor ?? null,
        runningAgents: running,
        totalAgents: tab.agents.size,
        contextPct:
          tab.latestPromptTokens != null
            ? contextPercent(tab.latestModel, tab.latestPromptTokens)
            : null,
        unread: tab.unread,
        kind: tab.kind,
        account: this.store.sessionAccountsByS.get(tab.sessionId)?.account ?? null,
      });
    }
    return out;
  }

  /**
   * auto-e2e F-E0:全会话状态一行 JSON——Tier1/Tier2 断言出口(经 e2e-probe Ctrl+Alt+F10 触发 →
   * fe_perf 日志)。复用 `snapshotSessions`(已含 status/tmuxIdle/origin/account),派生 `mismatch`
   * (detectAccountMismatch:活会话账号与该 origin 当前账号确知且不一致)。**不动 `debugSnapshot`
   * 形状**(f40-suite 依赖它),这是并列的第二个探针出口。生产不接线,方法本身无副作用/无落盘。
   */
  debugSessionsSnapshot(): string {
    const sessions = this.snapshotSessions().map((s) => ({
      sid: s.sessionId.slice(0, 8),
      status: s.status,
      tmuxIdle: s.tmuxIdle,
      origin: s.origin,
      account: s.account,
      mismatch: detectAccountMismatch(
        s.account,
        s.origin ? this.store.currentByOrigin.get(s.origin) ?? null : null,
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
    const agents = [...tab.agents.values()]
      .map((a) => ({ label: a.label, status: a.status }))
      .sort((x, y) => (x.status === "running" ? 0 : 1) - (y.status === "running" ? 0 : 1));
    return {
      model: tab.latestModel,
      recentFiles: [...tab.touchedFiles],
      agents,
    };
  }

  /** Batch5-F19：switchTo 是否写回 last-active（viewer/tear-off 窗口置 false）。 */
  persistLastActive = true;

  /** Batch5-F19（G 验收）：用户手动切 tab 时回调——main.ts 用它清 pendingStartupActive，
   *  防迟到的远端宣告补切抢走用户已选的焦点。 */
  onManualSwitch: (() => void) | null = null;

  /** F70：右键「在全景高亮本会话改动」回调——main.ts 注入（TabManager 不直接持有
   *  PanoramaView，走注入回调，同 onManualSwitch 范式）。仅本地会话菜单出现该项。 */
  requestPanoramaHighlight: ((sid: string) => void) | null = null;

  /** F88b：活跃会话最新 usage 变化回调——main.ts 注入喂 UsageHud（context% chip）。
   *  两处触发：onLine 捕到活跃会话新 assistant 记录；switchTo 切到别的会话。
   *  (model=null 或 promptTokens=null → chip 显 `?` 或隐藏)。同 requestPanoramaHighlight 注入范式。 */
  onActiveUsageChanged: ((model: string | null, promptTokens: number | null) => void) | null =
    null;

  ensureTab(
    sessionId: string,
    cwd: string | null,
    sourcePath: string,
    seq: number,
    origin: string | null = null,
    kind: string | null = null,
    bgName: string | null = null,
  ): Tab {
    let tab = this.store.tabs.get(sessionId);
    if (tab) {
      // SSH 重连：远端会话掉线时被 flush 归档过，现在又收到它的行 = backend 在重放 = 会话仍
      // 活着 → 复活成 live。必须放在 ensureTab 里（在 onLine 的 seq 去重 return 之前），否则整段
      // 重放全被去重时连第一条行都走不到翻转。**仅远端**：本地归档由 PID 判活驱动，不靠「收到行」
      // 翻转，避免会话退出时尾写把已归档的本地 Tab 误复活（远端掉线归档是连接驱动，无此风险）。
      if (tab.status === "archived" && tab.origin !== null) {
        tab.status = "live";
        this.clearPinHint(sessionId); // 〔步 17·B〕远端复活：空态提示的对象没了
        this.refreshTabBar();
      }
      // audit-fixes F03.2（D 审计修）：远端 idle-tmux tab 又收到后端重宣告 / jsonl 行 = claude
      // 复活（backend 只对活 pidfile 重宣告并推行；真 idle 会话已从 remote_active 移出、不重宣告也不
      // 推行）→ 清灰。这是清灰的**主**信号（queue 内、与行保序，SESSION_IDLE 恒排在会话末行之后，
      // 故复活行/重宣告严格晚于 idle）。不能只靠 session-activity 清灰：那是非 queue 同步派发、且
      // null-activity 的后端（远端 v1 无 status 字段）下永不清 → 活跃流式会话永久卡灰。
      if (tab.tmuxIdle) {
        tab.tmuxIdle = false;
        this.refreshTabBar();
        this.emitTabStateProbe(tab); // F-E1:远端复活清灰(idle→live)
      }
      // v2.22.2 kind 冲突消解:同一 sid 可能有多份 pidfile(实证:cc-backend 的
      // bg-spare 备用进程复用**父会话的 sid**写 kind=bg)——宣告到达顺序不定,
      // bg 先到会把真交互会话降格成 ⚙ 且树状挂到别的宿主下(用户截图实锤)。
      // 规则:**interactive 恒压过 bg**——后到的 interactive 宣告在此升格纠正
      // (重新按宿主定位 + 把同 cwd 孤儿 bg 拉回身后);反向(bg 后到)绝不降格。
      if (kind === "interactive" && tab.kind !== null && tab.kind !== "interactive") {
        tab.kind = "interactive";
        tab.bgName = null;
        tab.title = this.computeTitle(tab);
        const i = this.store.orderedIds.indexOf(sessionId);
        if (i >= 0) this.store.orderedIds.splice(i, 1);
        this.store.placeInOrder(tab);
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

    const { streamEl, stream, branchFolder, timeline, inputsEl, inputsPanel, outline } =
      this.view.mountTabDom(sessionId);

    // v2.3.0 issue #11: 异步 fetch 初始 task 快照。task-update 事件路径并行更新
    // tasksBySid，两路收敛到同一份数据；若 sid 是 active 同步推给全局 panel。
    void fetchSessionTasks(sessionId).then((tasks) => {
      this.store.tasksBySid.set(sessionId, tasks);
      if (this.store.activeId === sessionId) {
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
      forkedFromSessionId: null, // issue #63①:onLine 见首条 forkedFrom 记录时锁定
      origin,
      status: "live",
      // 〔步 17·B〕**不做自动固定**（照 `tab-collections.ts` 那条「手动建，不要自动」的先例，
      // `§B.7` 逐字）。盘上固定过的那些由 `loadPinned` 在复活时置回 true。
      pinned: false,
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
      skeleton: null,
      skeletonFetch: "idle",
      midBatchBuffer: [],
      fillHandler: null,
      processedUuids: new Set(),
      outline,
      inputsPanel,
      inputsEl,
      // issue #23：红绿灯信号若先于建 Tab 到达，从暂存取（否则 null=未知→绿）
      activity: this.store.pendingActivity.get(sessionId) ?? null,
      tmuxIdle: false, // audit-fixes F03.2：默认非灰；pendingTmuxIdle 在下方落实
      agents: new Map(),
      touchedFiles: new Set(), // F70：会话改动集，onLine 增量累进
      latestPromptTokens: null, // F88b：HUD context% 数据；onLine 捕获带 usage 的 assistant 记录
      latestModel: null,
      latestUsageSeq: -1,
    };
    this.store.pendingActivity.delete(sessionId);
    this.view.wireTab(tab);
    // issue #19：若该 sid 的归档信号先于本次建 Tab 到达（见 archiveTab），落实归档，
    // 避免重载后已结束会话复活成关不掉的 live Tab。本地 un-archive（上方 origin!==null
    // 那条）不适用，故归档后续 replay 行也不会把它复活。
    if (this.store.pendingArchive.delete(sessionId)) {
      tab.status = "archived";
      tab.activity = null; // 同 archiveTab：死会话不留陈旧灯/tooltip
      this.store.pendingTmuxIdle.delete(sessionId); // 归档优先：真 tmux 没了，灰灯作废
    } else if (this.store.pendingTmuxIdle.delete(sessionId)) {
      // audit-fixes F03.2：灰灯信号早于建 Tab（F5 重放乱序）→ 落实为 idle-tmux 灰点。
      tab.tmuxIdle = true;
    }
    this.store.tabs.set(sessionId, tab);
    this.store.placeInOrder(tab);

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

  /** issue #63①：首条带 `forkedFrom` 的记录锁定血缘（`tab-session-facts.ts::noteForkedFrom`）⇒ 标题加 `↳` 徽标。 */
  private applyForkedFrom(tab: Tab, message: unknown): void {
    if (!noteForkedFrom(tab, message)) return;
    tab.title = this.computeTitle(tab);
    this.refreshTabBar();
  }

  /** 根据 tab.cwd + tab.aiTitle + sessionId 算出展示标题（远端 Tab 加 `[origin]` 前缀） */
  private computeTitle(tab: Tab): string {
    return computeTitleFor(
      tab.sessionId,
      tab.cwd,
      tab.aiTitle,
      tab.origin,
      tab.kind,
      tab.bgName,
      tab.forkedFromSessionId,
    );
  }

  /**
   * auto-e2e F-E1:tab 生命周期状态转移探针。在**真值点**(markTmuxIdle 置灰 / archiveTab 归档 /
   * reviveTab 复活 / ensureTab 远端复活清灰)emit 可 grep 的 `[e2e] tab-state` 行,gray-light 全链
   * 套件按它断言 live→tmuxIdle=1(灰)→archived 序列(跨进程整链,单测碰不到)。self-gate
   * `import.meta.env.DEV`:生产构建整支(含模板串)被 vite 消除。
   */
  private emitTabStateProbe(tab: Tab): void {
    if (!import.meta.env.DEV) return;
    e2eLog(
      `[e2e] tab-state sid=${tab.sessionId.slice(0, 8)} status=${tab.status} tmuxIdle=${
        tab.tmuxIdle ? 1 : 0
      } origin=${tab.origin ?? "local"}`,
    );
  }

  /** session 退出（~/.claude/sessions/<PID>.json 被删）—— 灰显归档，内容保留 */
  archiveTab(sessionId: string): void {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      // issue #19：Tab 还没被 ensureTab 建出来（归档信号早于 replay 行到达）——
      // 记下待归档，建 Tab 时落实。否则这里直接 return 会静默丢弃归档 → 僵尸 live Tab。
      this.store.pendingArchive.add(sessionId);
      this.store.pendingActivity.delete(sessionId); // issue #23：死会话的暂存灯一并清
      this.store.pendingTmuxIdle.delete(sessionId); // audit-fixes F03.2：归档优先，清暂存灰灯
      return;
    }
    if (tab.status === "archived") return;
    tab.status = "archived";
    // issue #23：会话结束 → 灯灭（CSS 上 archived 本就隐藏 .live-dot，这里保持状态干净）
    tab.activity = null;
    tab.tmuxIdle = false; // audit-fixes F03.2：归档优先——tmux 真没了，清灰点保持状态干净
    this.sweepRunningAgents(tab); // 会话死了，running agent 必然中止
    // P5.2 B 重构后无 pendingToolGroup —— archive 不需要打断 tool-group 累积
    // （tool-group 合并改后处理，看 timeline 邻居；archive 后无新 record 入 timeline）。
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:归档(tmux 也没了 → archived)
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
    if (tab.origin !== null) return; // 仅本地；远端复活走 ensureTab 见行路径
    if (tab.status !== "archived") return;
    tab.status = "live";
    this.clearPinHint(sessionId); // 〔步 17·B〕真接上了 ⇒ 那块「只能 resume」的空态该走了
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:本地复活(archived→live)
  }

  /**
   * audit-fixes F03.2：远端 claude 退出但 tmux 会话仍在 → 灰灯（idle-tmux 第三态）。
   * 后端 emitter 收 backend removed 且 `@ccm_sid` present 时 emit `session-idle` 驱动（**不**
   * 归档、不 forget，故 status 仍 live，仅灯变灰）。Tab 未建（F5 重放乱序）则暂存待 ensureTab
   * 落实。archived 的 Tab 不置灰（真 tmux 没了才归档，归档优先）。无变化不重绘。清灰四处：
   * ensureTab（**主**：远端 tab 又收后端重宣告/行 = 复活，queue 内保序）/ updateActivity
   * （claude 再产活动，非 queue 的次要信号）/ reviveTab（本地）/ archiveTab（tmux 真没了）。
   */
  markTmuxIdle(sessionId: string): void {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      this.store.pendingTmuxIdle.add(sessionId);
      return;
    }
    if (tab.status === "archived") return; // 归档优先，不回置灰
    if (tab.tmuxIdle) return; // 无变化不重绘
    tab.tmuxIdle = true;
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:灰灯(claude 退但 tmux 在,status 仍 live)
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
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      if (act) this.store.pendingActivity.set(sessionId, act);
      else this.store.pendingActivity.delete(sessionId);
      return;
    }
    // archived 不更新（审计：心跳清死会话后磁盘残留 PID.json 被重扫会推陈旧
    // activity，archived tab 会挂上过期的 waiting tooltip——灯本身被 CSS 隐藏）。
    if (tab.status === "archived") return;
    // audit-fixes F03.2：收到活动信号 = claude 活着（远端 activity 仅在 claude 存活时由
    // backend 推）→ 清灰灯。必须放在下方「无变化早退」之前：复活后首个 activity 未必与灰前
    // 的陈旧 activity 值不同，否则灰点被早退跳过、清不掉。清了灰即使 activity 没变也要重绘。
    const clearedIdle = tab.tmuxIdle && act !== null;
    if (clearedIdle) tab.tmuxIdle = false;
    if (
      tab.activity?.status === act?.status &&
      tab.activity?.waitingFor === act?.waitingFor
    ) {
      if (clearedIdle) this.refreshTabBar();
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

  /** F88b：带 usage 的 assistant 记录 ⇒ 更新本会话最新 prompt token ＋ model（`tab-session-facts.ts::noteUsage`）。
   *  批期不即时喂 chip（onBatchEnd 单次 flush）；实时流则即时刷活跃会话。 */
  private trackUsage(tab: Tab, message: unknown, seq: number): void {
    if (!noteUsage(tab, message, seq)) return;
    if (!this.store.inBatch && tab.sessionId === this.store.activeId) {
      this.onActiveUsageChanged?.(tab.latestModel, tab.latestPromptTokens);
    }
  }

  /** issue #23（第二增量）：配对 agent 工具调用（`tab-session-facts.ts::noteAgents`），真有变化才刷面板。 */
  private trackAgents(tab: Tab, message: unknown): void {
    if (noteAgents(tab, message)) this.agentsChanged(tab);
  }

  /** issue #23：会话不再 busy ⇒ 仍 running 的 agent 标 aborted（`tab-session-facts.ts::abortRunningAgents`）。 */
  private sweepRunningAgents(tab: Tab): void {
    if (abortRunningAgents(tab)) this.agentsChanged(tab);
  }

  /** agents 变化 → 若是 active Tab 同步给全局面板 */
  private agentsChanged(tab: Tab): void {
    if (this.store.activeId === tab.sessionId) {
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
      const list = await listSessionActivity();
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
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status !== "archived") return;

    const wasActive = this.store.activeId === sessionId;
    const idx = this.store.orderedIds.indexOf(sessionId);
    // 优先切到后一个 Tab，否则前一个
    const fallbackId =
      this.store.orderedIds[idx + 1] ?? this.store.orderedIds[idx - 1] ?? null;

    this.view.disposeTab(tab);
    this.store.tasksBySid.delete(sessionId);
    this.store.tabs.delete(sessionId);
    if (idx >= 0) this.store.orderedIds.splice(idx, 1);
    // 〔步 17·B〕**关掉 = 取消固定。**
    //
    // pin 的语义是「别丢」（`§B.3b`），而 `×` 是用户**明确说要丢**。两者撞上时以后者为准 ——
    // 不摘的话下次开 app 它又回来了，那正是「能操作但没反应」的一种（点了 ×，第二天还在）。
    // ⚠ 只有真被固定过才写盘：没固定的 tab 关一下不该顺手改 `config.json`。
    if (tab.pinned) {
      tab.pinned = false;
      this.pinnedRecords.delete(sessionId);
      void this.persistPinned();
    }
    this.clearPinHint(sessionId);

    // 让后端 event_replay 把这个 session 的历史也丢掉
    forgetSession(sessionId);

    if (wasActive) {
      if (fallbackId !== null) {
        this.switchTo(fallbackId);
      } else {
        this.store.activeId = null;
        // issue #11: 关掉最后一个 Tab → panel 进入 null session 状态
        this.tasksPanel?.setSession(null, []);
        this.agentsPanel?.setSession(null, []);
        // F88b（审计）：无 fallback 时 switchTo 不会跑 → 手动隐藏 HUD chip，否则残留死会话的 ctx%
        this.onActiveUsageChanged?.(null, null);
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
    const ids = this.store.orderedIds;
    if (ids.length === 0) return;
    const idx = this.store.activeId ? ids.indexOf(this.store.activeId) : -1;
    const nextIdx = ((idx + delta) % ids.length + ids.length) % ids.length;
    const targetId = ids[nextIdx];
    if (targetId && targetId !== this.store.activeId) {
      this.switchTo(targetId);
    }
  }

  /**
   * 跳到第 N 个 Tab（1-indexed，issue #5 快捷键 Ctrl+1..9 用）。
   * N 大于现有 Tab 数 → 静默忽略；N 对应 Tab 已经 active → 无操作。
   */
  jumpToIndex(oneBasedIdx: number): void {
    const ids = this.store.orderedIds;
    if (oneBasedIdx < 1 || oneBasedIdx > ids.length) return;
    const targetId = ids[oneBasedIdx - 1];
    if (targetId && targetId !== this.store.activeId) {
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
    if (this.store.inBatch) return;
    if (!this.autoFollowUserActive) return;
    if (Date.now() < this.manualOverrideUntil) return;
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status === "archived") return;
    if (this.store.activeId === sessionId) {
      // 已经在这个 tab 但用户开了"拉前 monitor"也照拉
      if (this.bringMonitorToFront) bringMonitorToFront();
      return;
    }
    this.switchTo(sessionId, "auto");
    if (this.bringMonitorToFront) bringMonitorToFront();
  }

  /** 快捷键 Ctrl+W：当前活跃 Tab 是 archived 才关，live 不动 */
  closeActiveIfArchived(): void {
    if (!this.store.activeId) return;
    const tab = this.store.tabs.get(this.store.activeId);
    if (tab && tab.status === "archived") {
      this.closeTab(this.store.activeId);
    }
  }

  /** 快捷键 Ctrl+` ：把当前活跃 Tab 对应的终端窗口拉到前台（live 本地 / 远端均可） */
  bringActiveTerminalToFront(): void {
    // 〔第二波 T4 · LF1〕非 Windows 上 ↗ 的最后一跳是桩（每点必败）⇒ 按钮不渲；
    //   快捷键 / 命令面板（住 `main.ts`）还够得到这里 ⇒ 说一句实话，不发 IPC。见 `terminal-front.ts`。
    if (!terminalFrontAvailable()) {
      showActionFailureToast(TERMINAL_FRONT_UNAVAILABLE_TITLE, TERMINAL_FRONT_UNAVAILABLE_DETAIL, {
        level: "info",
        durationMs: 6000,
      });
      return;
    }
    if (!this.store.activeId) return;
    const tab = this.store.tabs.get(this.store.activeId);
    if (!tab || tab.status === "archived") return;
    // Feature ②：远端 Tab → 后端唯一分派点（先启动令牌、后 ccm-rbind 标题退路）；本地 Tab → 原 sid_hwnd_cache 路径。
    if (tab.origin !== null) {
      void bringRemoteTerminalToFront(this.store.activeId);
    } else {
      void bringTerminalToFront(this.store.activeId);
    }
  }

  /** 快捷键 Ctrl+Shift+E：打开当前活跃 Tab 的工作目录到系统文件管理器 */
  openActiveTabCwd(): void {
    if (!this.store.activeId) return;
    void this.openTabCwd(this.store.activeId);
  }

  /** F77：活跃 tab 的子 agent 加载上下文（parentPath + origin）——main.ts 点 agent 行时用它
   *  调 `load_subagent`。无活跃 tab / 无 parentPath → null；远端会话 origin!==null（不支持，调用方提示）。 */
  getActiveSubagentContext(): { parentPath: string; origin: string | null } | null {
    const tab = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (!tab || !tab.parentPath) return null;
    return { parentPath: tab.parentPath, origin: tab.origin };
  }

  /** issue #10 快捷键 Ctrl+Shift+N：把当前活跃 Tab 在独立只读窗口打开 */
  openActiveInNewWindow(): void {
    if (this.store.activeId) void this.openInNewWindow(this.store.activeId);
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
      dropTarget: { kind: "end" },
      dwellSid: null,
      dwellArmed: null,
      dwellX: e.clientX,
      dwellY: e.clientY,
      dwellTimer: null,
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
      ghost.textContent = this.store.tabs.get(d.sid)?.title ?? "";
      document.body.appendChild(ghost);
      d.ghost = ghost;
    }

    // 跟随光标（偏右下避免压在指针正下方）。
    if (d.ghost) {
      d.ghost.style.left = `${e.clientX + 8}px`;
      d.ghost.style.top = `${e.clientY + 8}px`;
    }

    // P7a-2：**纵向那根轴今天一个消费者都没有** —— arm 只看 `clientX`（见下一行）。
    // 所以栏内重排走 `clientY`，与 tear-off 天然不争同一根轴。
    // 〔步 17·D〕这里先更新停留状态，再算落点 —— 落点的 `onto` 那一支要读停留的结论。
    this.updateDwell(e.clientX, e.clientY);
    d.dropTarget = this.computeDropTarget(e.clientY);
    // ★ **落点要看得见**〔D 阶段补审〕：只搬不指示的话，「拖动排序」是一次盲操作 ——
    // 用户松手前不知道会落在哪，只能松开看结果、错了再拖一次。
    // armed（拖出右缘）时不指示：那一路根本不重排，指一条不会发生的落点是在骗人。
    this.markDropTarget(e.clientX > d.barRight + 16 ? null : d.dropTarget);

    // arm：指针拖离竖栏右缘一段距离 = 松手即弹独立窗口（F33 前是下缘判定）。
    const armed = e.clientX > d.barRight + 16;
    if (armed !== d.armed) {
      d.armed = armed;
      if (d.ghost) {
        d.ghost.classList.toggle("armed", armed);
        d.ghost.textContent = armed
          ? "松开 → 独立窗口"
          : (this.store.tabs.get(d.sid)?.title ?? "");
      }
    }
  }

  /**
   * 〔步 17·D · `§D.2`〕量一遍栏里每个 tab 在纵轴上占的那一段。
   *
   * 🔴 **`parentElement !== this.barEl` 那道过滤没了。**
   *   `§D.2` 现打它是两个缺口之一：它把**组里的 tab 整体排除**在落点之外
   *   ⇒ 拖不进组、也拖不出组。换成 `barEl.contains(...)`：组容器是 `barEl` 的子树，
   *   组里的 tab 因此照常参与，而栏外的东西（撕出去的窗口等）仍然不参与。
   *
   * ⚠ 只量一次、返回纯数据 —— 判定逻辑在 `pickDropTarget`（纯函数，判据直接打它）。
   *   这也顺带守住 `§3 P3`「拖拽 layout thrash：量一次、只改变化的那一个」。
   */
  private tabRects(): TabRect[] {
    const out: TabRect[] = [];
    for (const [sid, refs] of this.tabButtons) {
      if (!this.barEl.contains(refs.root)) continue;
      const r = refs.root.getBoundingClientRect();
      out.push({ sid, top: r.top, height: r.height });
    }
    return out;
  }

  /** 〔步 17·D〕现在的落点。三种语义的判定住 `pickDropTarget`，这里只负责喂它读数。 */
  private computeDropTarget(clientY: number): DropTarget {
    const d = this.drag;
    if (!d) return { kind: "end" };
    return pickDropTarget(
      this.tabRects(),
      clientY,
      new Set(this.dragBlockOf(d.sid)),
      d.dwellArmed,
    );
  }

  /**
   * 〔步 17·D · `§D.4`〕停留（dwell）判定：**压住 ≥250ms 且抖动 <4px ⇒ 切进 `onto` 态**。
   *
   * 🔴 **必须用计时器，不能只在 `mousemove` 里数时间** —— 指针停住之后
   *   `mousemove` 就不再来了，靠事件驱动的话「停留」永远攒不满，这个手势等于没做。
   *
   * 三种情况清零重来（`§D.4` 逐字「一旦移出该 tab 的矩形 **或** 移动超过 4px ⇒ 退回」）：
   * ① 换了压着的 tab；② 没压在任何 tab 上；③ 还在同一个上但离锚点超过 `DWELL_MOVE_PX`。
   */
  private updateDwell(clientX: number, clientY: number): void {
    const d = this.drag;
    if (!d) return;
    const hovered = tabUnderY(this.tabRects(), clientY, new Set(this.dragBlockOf(d.sid)));
    const moved = Math.hypot(clientX - d.dwellX, clientY - d.dwellY);
    if (hovered === d.dwellSid && moved < DWELL_MOVE_PX) return; // 还在攒，别打断计时
    d.dwellSid = hovered;
    d.dwellArmed = null;
    d.dwellX = clientX;
    d.dwellY = clientY;
    if (d.dwellTimer !== null) window.clearTimeout(d.dwellTimer);
    d.dwellTimer = null;
    if (hovered === null) return;
    d.dwellTimer = window.setTimeout(() => {
      const cur = this.drag;
      // 计时器到点时拖拽可能已经结束 / 已经换了目标 —— 两者都不许再改状态。
      if (!cur || cur.dwellTimer === null || cur.dwellSid !== hovered) return;
      cur.dwellTimer = null;
      cur.dwellArmed = hovered;
      // **可逆可见**（`§D.4` 理由 3）：攒满的那一刻就给反馈，用户看到了再松手。
      // 指针停着不动 ⇒ 不会再有 `mousemove` 来重算落点，所以这里自己算一次。
      cur.dropTarget = { kind: "onto", sid: hovered };
      if (!cur.armed) this.markDropTarget(cur.dropTarget);
    }, DWELL_MS);
  }

  /**
   * P7a-2：被拖的那一块 —— 交互 tab 连同它**紧跟其后**的同 `(cwd, origin)` bg 子串。
   *
   * ★ 为什么必须带上子串：`placeInOrder` 维护的那棵树不是装饰，它表达
   * 「这些后台任务属于那个会话」（还带着 D-R3 的审计账）。
   * 一次拖动就把树拆散，比不能拖更坏。
   */
  private dragBlockOf(sid: string): string[] {
    const host = this.store.tabs.get(sid);
    if (!host) return [sid];
    const hostIsInteractive = host.kind === null || host.kind === "interactive";
    if (!hostIsInteractive) return [sid];
    const out = [sid];
    const at = this.store.orderedIds.indexOf(sid);
    for (let i = at + 1; i < this.store.orderedIds.length; i++) {
      const t = this.store.tabs.get(this.store.orderedIds[i]);
      if (!t) break;
      const isBg = t.kind !== null && t.kind !== "interactive";
      if (isBg && t.cwd !== null && t.cwd === host.cwd && t.origin === host.origin) {
        out.push(this.store.orderedIds[i]);
      } else break;
    }
    return out;
  }

  /**
   * P7a-2 D 补审：给落点那个 tab 打标（`null` = 不指示，armed 那一路用）。
   *
   * 〔步 17·D · `§D.5`〕两种态两种标：`before` 顶部一条线（原样）· `onto` 整块描边 + 轻微放大。
   * `end` 不标（原样 —— 末尾没有可以描的对象）。
   */
  private markDropTarget(target: DropTarget | null): void {
    const beforeSid = target?.kind === "before" ? target.sid : null;
    const ontoSid = target?.kind === "onto" ? target.sid : null;
    for (const [sid, refs] of this.tabButtons) {
      refs.root.classList.toggle("drop-before", sid === beforeSid);
      refs.root.classList.toggle("drop-onto", sid === ontoSid);
    }
  }

  /**
   * 〔步 17·D〕把一次落点的**全部后果**落实：顺序 ＋ 集合归属，一拍做完。
   *
   * 🔴 **两件事不许分两拍** —— 顺序没变（拖回原位）但归属变了（从组里拖出来）是
   *   真实情形；反过来也是。谁先 `return`，另一半就静默丢了。
   *   在这之前这里叫 `applyReorder`，只管顺序、`if (没变化) return` 直接结束。
   */
  private applyDrop(sid: string, target: DropTarget): void {
    const block = this.dragBlockOf(sid);
    // ① 集合归属跟着落点宿主走（`§D.7` 的「拖出组」与「拖进组」是同一条规则的两侧）。
    if (this.collectionsLoaded) {
      const other = target.kind === "end" ? null : this.store.tabs.get(target.sid);
      const nextCols = applyDropToCollections(
        this.collections,
        block,
        target,
        defaultGroupName(
          this.store.tabs.get(sid)?.cwd ?? null,
          other?.cwd ?? null,
          this.collections.map((c) => c.name),
        ),
        newCollectionId(),
      );
      // 没变就不写盘：拖动是高频动作，每拖一下都改一次 `config.json` 是白写。
      if (!collectionsEqual(this.collections, nextCols)) {
        this.collections = nextCols; // 先改内存（下面统一重画一次），再落盘
        void this.persistCollections(nextCols);
      }
    }
    // ② 顺序。`onto` 的落位 = 插到目标**之前**（组里成员的相对次序由 `orderedIds` 定，
    //    见 `refreshTabBar`）；`end` 是末尾。
    const beforeSid = target.kind === "end" ? null : target.sid;
    const next = moveTabBlock(this.store.orderedIds, block, beforeSid);
    if (next.length !== this.store.orderedIds.length) {
      this.refreshTabBar(); // 防御：块算错了就只重画（①可能已经改了归属）
      return;
    }
    if (next.every((x, i) => x === this.store.orderedIds[i])) {
      this.refreshTabBar();
      return;
    }
    this.store.orderedIds = next;
    this.refreshTabBar();
    // 🔴 〔步 17·C · 2026-09-19〕**拖动的结果要落盘** —— `设计/30 §C` 逐字「今天拖了白拖」。
    //   在这之前 `orderedIds` 的 8 个写入点零持久化，而集合（`tabCollections`）是落盘的
    //   ⇒ 同一个栏里两种寿命：**你建的分组活过重启，你拖的顺序活不过**。
    //   `tab-collections.ts` 立集合落盘的理由是「用户手写的真相，不是能重算的缓存」，
    //   而拖动排序**完全符合那条判据** ⇒ 不给它同样的待遇，那条理由就是选择性适用的。
    // ⚠ 形状照 `commitCollections`：**先改内存再落盘**（上面两行已做完），
    //   落盘失败只记日志 —— 顺序丢一次远好过拖动卡一下。
    void this.persistOrder();
  }

  /** 把当前顺序写进 `config.json` 的 `tabBar.order`。失败只记日志，不打断交互。 */
  private async persistOrder(): Promise<void> {
    // 🔴 **先把内存里那份意图同步掉，再去写盘** —— 用户刚拖出来的这张就是最新的意图。
    //   不同步的话，`savedOrder` 还是启动时读到的那份**旧**顺序，而它每来一个新 tab
    //   就会被再应用一次（`placeInOrder`）⇒ **后到的一个 tab 能把用户刚拖的一下整张撤销**。
    //   放在 `await` 之前：落盘失败也照样同步 —— 内存里那张已经是用户看见的事实了。
    this.store.savedOrder = [...this.store.orderedIds];
    try {
      await setTabOrder(this.store.orderedIds);
    } catch (e) {
      console.warn("[tab-bar] 顺序落盘失败:", e);
    }
  }

  /**
   * 启动时把落盘的顺序拉回来。**宿主在 `loadCollections` 之后调一次。**
   *
   * ⚠ 它**只重排已经存在的 tab，不凭空造 tab**（`§C.3` 逐字）——
   *   盘上的顺序里会有已经不存在的 sid（上次那个会话被删了）。
   * ⚠ **盘上没提到的 tab 排在后面**，保持它们此刻的相对次序 ——
   *   否则「启动后新建的 tab」会被一份旧顺序挤到看不见的地方。
   *
   * # 🔴 2026-09-21：修掉「结构性 no-op」（`99 §4` 步 17 那行的 🟡）
   *
   * 在这之前它是这么写的：
   * ```ts
   * const saved = await getTabOrder(new Set(this.store.orderedIds));  // ← alive = 此刻的 tab 集
   * if (saved.length === 0) return;
   * ```
   * **它在唯一那个调用点上恒等于 no-op。** `main.ts` 里那一行是
   * `loadPinned().finally(() => loadOrder())` ⇒ 跑到这儿的时候会话**还没到**
   * （tab 由随后的 `session_added` / 首行陆续建出来，启动窗口期 30s）⇒
   * `orderedIds` 是空的（顶多只有几个复活出来的 pinned）⇒ `alive` 是空集 ⇒
   * `sanitizeOrder` 把盘上那张**整张**当成死 sid 摘掉 ⇒ 空表 ⇒ 上面那句直接 `return`。
   * 现打：`sanitizeOrder(["c","b","a"], new Set())` == `[]`，而 `alive` 传 `null`
   * 时原样是 `["c","b","a"]` —— **数据一直读得出来，是被自己那道过滤删掉的。**
   *
   * 🔴 **所以这不是「调用点排早了」，挪一挪就好** —— 前端**没有任何一刻**知道
   *   「会话到齐了」（它们从几台远端陆续宣告，还能中途掉线重连）。
   *   「已删的会话」与「还没到的会话」在任何单一时刻都**不可区分**
   *   ⇒ 只要过滤发生在读的那一拍，这个 bug 就还在。
   *
   * ⇒ 改法：把盘上那份顺序**留着**（`savedOrder`，一份意图，不是一次性的动作），
   *   读的时候**不按存活过滤**，过滤改在**每次应用**时按「此刻真的在的 tab」做
   *   （`applySavedOrder`）；tab 陆续到达时由 `placeInOrder` 再应用一次。
   */
  async loadOrder(): Promise<void> {
    // `null` = 这一趟不按存活过滤（理由见 `getTabOrder` 头注与上面那段）。
    this.store.savedOrder = await getTabOrder(null);
    if (this.store.applySavedOrder()) this.refreshTabBar();
  }

  /** 收尾：拆 document listener、清 ghost / 源 Tab 变暗、清空拖拽状态。 */
  private teardownDrag(): void {
    const d = this.drag;
    if (!d) return;
    document.removeEventListener("mousemove", d.onMove);
    document.removeEventListener("mouseup", d.onUp);
    // 〔步 17·D〕停留计时器必须在这里清。不清的话它会在拖拽结束之后才到点，
    // 往一个已经收尾的状态上写 `onto` —— 而那时 `this.drag` 已是 null，
    // 回调里的守卫会吞掉它，但计时器本身是条悬空引线（同 `pendingMenuTimers` 那条教训）。
    if (d.dwellTimer !== null) window.clearTimeout(d.dwellTimer);
    d.dwellTimer = null;
    this.markDropTarget(null); // 拖拽结束必须清掉落点标记，否则它会挂在那儿
    d.ghost?.remove();
    d.root.classList.remove("dragging");
    this.drag = null;
    // ★ 6d：拖拽期间被守卫挡下的那些刷新，在这里**补一次**。
    // 少了这一句，守卫就从「推迟」变成「静默丢弃」：会话在拖拽那一秒里跑完了，
    // 它的 tab 会一直留在主栏假装还活着，直到下一件无关的事碰巧再刷一次栏。
    // ⚠ 必须在 `this.drag = null` **之后** —— 否则自己被自己的守卫挡回去。
    if (this.tabBarDirtyDuringDrag) {
      this.tabBarDirtyDuringDrag = false;
      this.refreshTabBar();
    }
  }

  /** document mouseup：收尾；armed 则在落点弹独立窗口。 */
  private onDragUp(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;
    // 〔步 17·D〕落点要在 `teardownDrag` 之前取出来 —— 它会把整个 `drag` 清成 null。
    const { dragging, armed, sid, dropTarget } = d;
    this.teardownDrag();

    if (!dragging) return; // 没越阈值 = 纯点击，交给 click handler 正常切 Tab。

    // 起过拖（无论 armed 与否）都抑制紧随的 click —— 拖完不该顺带切 Tab。
    this.suppressClickSid = sid;
    if (armed) {
      // ★ P7a-2-Y2：撕窗口这一路**顺序一个字不动**。两件事都做的话，
      // 用户撕出一个窗口的同时原栏的序被改了 —— 他没要求过那件事。
      void this.openInNewWindow(sid, e.screenX, e.screenY);
      return;
    }
    this.applyDrop(sid, dropTarget);
  }

  /** 单个 tab 的账号徽章就地重刷（in-flight 状态变化时用；tab 已没了就静默跳过）。 */
  private refreshAccountBadgeFor(sid: string): void {
    const refs = this.tabButtons.get(sid);
    const tab = this.store.tabs.get(sid);
    if (refs && tab) this.updateAccountBadge(refs, sid, tab);
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
    if (this.store.activeId === sessionId) return;

    // 切 active 走 .active class（CSS visibility 控制），避免 display:none/block
    // 触发整棵子树重建 layout tree 卡顿。详 styles.css 的 .stream 注释。
    this.view.showOnly(sessionId);
    const next = this.store.tabs.get(sessionId);
    if (next) next.unread = 0;
    this.store.activeId = sessionId;
    if (next) this.view.activate(next);
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
      if (this.store.activeId !== sessionId) return;
      next?.stream.scrollToBottom();
      // ★ 步 3：**第二帧再贴一次**（对齐 `session-viewer.ts:82` 已有的同一修法）。
      //
      // 第一帧贴底时，刚从 `visibility:hidden` 翻出来的那些卡还带着
      // `content-visibility: auto` 的**估值**几何 —— 按估值算出来的 `scrollHeight`
      // 不是真的，贴完仍可能差半屏。下一帧周边已材料化成真实尺寸，再发一次落点才准。
      // ⚠ 诚实边界：`scrollToBottom()` 会把 `stickToBottom` **重新置真** ——
      // 所以这不是一次「只读的校正」，它和第一次一样是强制贴底。
      // 可接受的理由只有一条：两次之间只隔**一帧（~16ms）**，人不可能在这中间滚出意图；
      // 切走了则上面那道 `activeId` 守卫已经挡住。真要更细，得让 `MessageStream` 出一个
      // 「只重贴、不改粘底态」的入口 —— 那是另一件事，别在这一步顺手扩。
      requestAnimationFrame(() => {
        if (this.store.activeId !== sessionId) return;
        this.store.tabs.get(sessionId)?.stream.scrollToBottom();
      });
      // issue #11: 切换 task panel 数据源到新 active Tab 的 sid
      this.tasksPanel?.setSession(sessionId, this.store.tasksBySid.get(sessionId) ?? []);
      // issue #23: agents 面板同步切到新 active Tab
      this.agentsPanel?.setSession(
        sessionId,
        [...(this.store.tabs.get(sessionId)?.agents.values() ?? [])],
      );
      // F88b：HUD context% chip 切到新 active 会话的最新 usage（无带 usage 记录 → null → 隐藏）
      const nt = this.store.tabs.get(sessionId);
      this.onActiveUsageChanged?.(nt?.latestModel ?? null, nt?.latestPromptTokens ?? null);
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
  /**
   * `refreshTabBar` 的**帧末合批**入口〔audit-0805 F15〕。
   *
   * 排一次位（`tabBarRefreshScheduled`）+ 无 rAF 时 `setTimeout` 兜底，
   * 范式与同文件的 `scheduleIdleMaterialize` 一致。
   * ⚠ 只给 live 路那一处用，别把用户动作触发的调用点也改过来（理由写在调用处）。
   */
  private tabBarRefreshScheduled = false;

  private scheduleTabBarRefresh(): void {
    if (this.tabBarRefreshScheduled) return;
    this.tabBarRefreshScheduled = true;
    const run = (): void => {
      this.tabBarRefreshScheduled = false;
      this.refreshTabBar();
    };
    if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(run);
    } else {
      window.setTimeout(run, 0);
    }
  }

  /** P7a-1：归档区 UI 建一次。默认折叠（缺省 `"1"`，与 agents/tasks 面板同形态）。 */
  // 🔴 〔步 17·A · 2026-09-19〕**`ensureArchiveUi()` 整个删掉。**
  //
  // `设计/30 §A` 抬头逐字「**已定**：删归档抽屉 · 固定灰 tab」，三条独立理由：
  //   ① 它永久吃 450px 屏宽（`.tab-archive` 是 `#app` 的 grid item 却没认领格子）
  //   ② 它是个撕窗口陷阱（tear-off 判定线对抽屉没有意义）
  //   ③ **它的存在理由本来就自相矛盾** —— 原 `belongsInArchive` 的注释自己写着：
  //      active tab 会「在你正看着它的时候」掉进折叠的抽屉里 ⇒ 已经为 active 开了例外。
  //      把例外推广到全部，抽屉就没了。
  //
  // ⚠ **删的是抽屉，不是状态。** `status === "archived"` 照旧存在，那种 tab
  //   **留在原位灰着**（`.tab.archived` 那条 CSS 本来就有，`§A.3` 逐字「不用新写」）。
  //   用户 2026-09-19 逐字：「没有归档这个东西，不要归档，就是灰 tab。」

  /** P7a-3：从 `config.json` 拉一次集合并重画。宿主启动时调一次。 */
  async loadCollections(): Promise<void> {
    this.collections = await getCollections();
    this.collectionsLoaded = true;
    this.refreshTabBar();
  }

  /** P7a-3：落盘 + 重画。**先改内存再落盘** —— 让 UI 立刻响应，落盘失败只记日志。 */
  private async commitCollections(next: TabCollection[]): Promise<void> {
    this.collections = next;
    this.refreshTabBar();
    await this.persistCollections(next);
  }

  /**
   * 只落盘、不重画。〔步 17·D〕`applyDrop` 要在同一拍里改**顺序 ＋ 归属**，
   * 由它统一重画一次 —— 这里再画一次就是白画（拖动结束那一拍本来就重。`§3 P3`）。
   * ⚠ 「落盘失败只记日志」这句话只能有一个住址，所以 `commitCollections` 也走这里。
   */
  private async persistCollections(next: readonly TabCollection[]): Promise<void> {
    try {
      await setCollections(next);
    } catch (e) {
      console.warn("[tab-collections] 落盘失败:", e);
    }
  }

  // ===== 〔步 17·B · `设计/30 §B`〕固定（pinned）=====

  /**
   * 启动时把固定的 tab **复活**出来。宿主在 `loadCollections` 之后调一次。
   *
   * ⚠ **2026-09-21 订正**：这儿原先写着「必须在 `loadOrder` **之前**」，理由是
   *   「`loadOrder` 用 `getTabOrder(new Set(this.store.orderedIds))` 按今天真的存在的 sid 过滤，
   *   复活的 tab 得先存在，位置才排得回来」。**那个理由连着那道读时过滤一起没了**
   *   （`loadOrder` 头注记着为什么它是个 no-op）：顺序现在是一份**留着的意图**
   *   （`savedOrder`），复活出来的 tab 走 `createSkeletonTab` → `placeInOrder` 时
   *   会**再应用一次** ⇒ 两者谁先谁后都排得回来。
   * ⚠ `main.ts` 今天仍然是「pinned 先、order 后」那个次序 —— 不改它，但那**不再是承重的**。
   *
   * # 复活流程（`§B.5` 逐字）
   * ```
   * 读 tabBar.pinned[] → 逐条 createSkeletonTab(sid, cwd, origin, kind, name)
   *   ├ 标 pinned = true
   *   ├ 标 status = "archived"（没有活进程；后端 replay 随后宣告它活着 ⇒ 事件流会改回 live）
   *   └ 标题直接用存下来的那份（不等读文件）
   * ```
   * 🔴 **不读内容** —— `99 §2.5 P3` 已裁定「已结束的会话点进去不能看内容，只能 resume」。
   *   `replay_session_to_window` 那条路对 archived 本来就走不通（它的头注逐字：
   *   「仅活跃 session 的历史在 buffer 里」）。
   *
   * ⚠ **已经存在的 sid 不重建**（后端 replay 可能已经先宣告了它）—— 只补一个 `pinned = true`，
   *   `status` 一个字不碰：那条会话真活着的时候，把它按回 archived 是一句假话。
   */
  async loadPinned(): Promise<void> {
    const list = await getPinned();
    this.pinnedRecords = new Map(list.map((p) => [p.sid, p]));
    for (const p of list) {
      const existed = this.store.tabs.get(p.sid);
      if (!existed) {
        this.createSkeletonTab(p.sid, p.cwd, p.origin, p.kind, p.name);
        const t = this.store.tabs.get(p.sid);
        if (!t) continue;
        // 没有活进程 ⇒ 灰着。`archiveTab` 那条路要求 tab 已在事件流里，这里是**凭空造**，
        // 所以直接置位；两者最终形态一致（`.tab.archived` 那条 CSS 本来就有）。
        t.status = "archived";
        t.activity = null;
        t.parentPath = p.jsonlPath; // `§B.5`：复活的必需品（resume 与「有没有记录」都靠它）
        t.title = p.title; // 骨架期就显示正确标题，不等读文件
        t.pinned = true;
        this.mountPinHint(t);
      } else {
        existed.pinned = true;
      }
    }
    this.pinnedLoaded = true;
    this.refreshTabBar();
  }

  /**
   * 复活出来的固定 tab 的空态：**说清它是什么 ＋ 给出那唯一的出口**。
   *
   * `§B.5` 复活流程最后一行逐字：「用户点进去那一刻，**出现 resume 入口**（🔴 不读内容）」。
   * `§B.6` 第一格：`jsonlPath` 为空的那种要提示「这个会话没有留下记录」——
   * **不要留一个点了没反应的 tab**。两种情形在这里分叉。
   */
  private mountPinHint(tab: Tab): void {
    const sid = tab.sessionId;
    const degraded = this.pinIsDegraded(sid);
    const box = document.createElement("div");
    box.className = "pin-revived-hint";
    const head = document.createElement("strong");
    head.textContent = degraded ? "这个会话没有留下记录" : "📌 固定下来的已结束会话";
    const body = document.createElement("p");
    body.textContent = degraded
      ? "固定它的时候它还没写下任何一行，前端没有它的 jsonl 路径 —— 没有可以接回去的东西。右键 × 可以把它去掉。"
      : "内容不在本地缓存里（已结束的会话只能 resume，不能回看）。resume 成功后 Claude 会续写同一份记录，这个 tab 会自己亮起来。";
    box.append(head, body);
    if (!degraded) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "pin-revived-hint-btn";
      btn.textContent = "Resume 这个会话";
      // 与右键菜单的「Resume」同一个动作、同一个住址 —— 这里只是把入口放在用户正看着的地方。
      btn.addEventListener("click", () => void this.resumeTab(sid));
      box.appendChild(btn);
    }
    tab.streamEl.appendChild(box);
    this.pinHintEls.set(sid, box);
  }

  /** 复活成 live（resume 真的接上了）或 tab 关掉时，摘掉那块空态提示。 */
  private clearPinHint(sid: string): void {
    this.pinHintEls.get(sid)?.remove();
    this.pinHintEls.delete(sid);
  }

  /**
   * 把一个 tab 压成一条落盘记录。字段表逐条照 `§B.5`。
   *
   * ⚠ `lastActiveAt`：**live ⇒ 此刻**（「它现在还活着」是个真读数）；
   *   **archived ⇒ 沿用盘上那份，没有就 `null`** —— 前端 `Tab` 上零时间戳字段（现打），
   *   把它刷成 `Date.now()` 会让「最后活动时刻」变成「最后一次落盘时刻」，那是假话。
   */
  private pinRecordFor(tab: Tab): PinnedTab {
    const prev = this.pinnedRecords.get(tab.sessionId);
    return {
      sid: tab.sessionId,
      jsonlPath: tab.parentPath,
      cwd: tab.cwd,
      origin: tab.origin,
      // `§3.5.7`：缺了 resume 会静默落到默认号。两个源都读不到 ⇒ `null`＝没记到，不是默认号。
      account:
        this.store.sessionAccountsByS.get(tab.sessionId)?.account ??
        this.store.accountLastByS.get(tab.sessionId) ??
        prev?.account ??
        null,
      lastActiveAt: tab.status === "live" ? Date.now() : prev?.lastActiveAt ?? null,
      kind: tab.kind,
      name: tab.bgName,
      title: tab.title,
    };
  }

  /**
   * 把「现在哪些 tab 被固定了」整张表写回 `config.json` 的 `tabBar.pinned`。
   *
   * **真相源是 `Tab.pinned`**，这里只是把它压平 ⇒ 不会出现「内存说固定了、盘上没有」。
   *
   * 🔴 **「没 `loadPinned` 过就不写」那道门不在这里，在 `togglePin`** —— 这是死值验逼出来的：
   *   我原本在这里也放了一条 `if (!this.pinnedLoaded) return;`，**刀 9 实测它恒不承重**
   *   （去掉之后一格都不红）。原因是它没有任何可区分的输入：本函数只有两个调用方，
   *   `togglePin` 自己那道门已经挡在前面，而 `closeTab` 只在 `tab.pinned` 为真时才调，
   *   `tab.pinned` 又只能由 `loadPinned`（与 `pinnedLoaded = true` 同一个微任务）
   *   或 `togglePin` 置起来。
   * ⇒ 照 `sanitizeCollections` 那条逐字先例删掉：「留一道任何输入都区分不出的守卫，
   *   就是一条假绿的防线」。真正在承重的那道由死值验刀 10 钉着。
   */
  private async persistPinned(): Promise<void> {
    const next: PinnedTab[] = [];
    for (const sid of this.store.orderedIds) {
      const tab = this.store.tabs.get(sid);
      if (tab?.pinned) next.push(this.pinRecordFor(tab));
    }
    this.pinnedRecords = new Map(next.map((p) => [p.sid, p]));
    try {
      await setPinned(next);
    } catch (e) {
      console.warn("[tab-bar] 固定落盘失败:", e);
    }
  }

  /**
   * 右键菜单那一项：翻转固定。**先改内存再落盘**（照 `commitCollections` 的形状）。
   *
   * ⚠ 不做自动固定（`§B.7` 逐字「照 `tab-collections.ts` 那条『手动建，不要自动』的先例」）——
   *   这是唯一的入口。
   */
  togglePin(sid: string): void {
    const tab = this.store.tabs.get(sid);
    if (!tab || !this.pinnedLoaded) return;
    tab.pinned = !tab.pinned;
    this.refreshTabBar();
    void this.persistPinned();
  }

  /**
   * `§B.6` 第一格：这条固定记录**点进去也没有东西可看**（`jsonlPath` 为空 ——
   * 骨架 tab 从没收到过带路径的行就被固定了）。
   *
   * 🔴 用途是**不许留一个点了没反应的 tab**：点它的时候要说人话（见点击处的提示）。
   * ⚠ 两个条件都要：盘上那条是降级的 **且** 到现在也没有行回填过 `parentPath`
   *   （真来了行就不再降级 —— 那条 tab 已经有记录可读了）。
   */
  private pinIsDegraded(sid: string): boolean {
    const rec = this.pinnedRecords.get(sid);
    if (!rec || !isDegradedPin(rec)) return false;
    return (this.store.tabs.get(sid)?.parentPath ?? "") === "";
  }



  /**
   * P7a-3：拿到某集合在主栏里的容器（没有就建）。
   *
   * 组头点一下改名、右侧 `×` 解散。**解散只去掉分组，一个 tab 都不动** ——
   * 集合是个视图，不是容器。
   */
  private groupElFor(col: TabCollection): HTMLElement {
    let g = this.groupEls.get(col.id);
    if (!g) {
      const wrap = document.createElement("div");
      wrap.className = "tab-group";
      const head = document.createElement("div");
      head.className = "tab-group-head";
      const name = document.createElement("button");
      name.type = "button";
      name.className = "tab-group-name";
      name.addEventListener("click", () => {
        const cur = this.collections.find((x) => x.id === col.id);
        const next = window.prompt("集合名:", cur?.name ?? "");
        if (next === null) return;
        void this.commitCollections(renameCollection(this.collections, col.id, next));
      });
      const del = document.createElement("button");
      del.type = "button";
      del.className = "tab-group-del";
      del.textContent = "×";
      del.title = "解散这个集合（只去掉分组，会话一个都不会关）";
      del.addEventListener("click", () => {
        void this.commitCollections(deleteCollection(this.collections, col.id));
      });
      head.append(name, del);
      const list = document.createElement("div");
      list.className = "tab-group-list";
      wrap.append(head, list);
      this.barEl.appendChild(wrap);
      g = { wrap, head, list };
      this.groupEls.set(col.id, g);
    }
    (g.head.firstElementChild as HTMLElement).textContent = col.name;
    return g.list;
  }

  private refreshTabBar(): void {
    // ★ 6d（条 54）：**拖拽进行中不重排 tab 栏。**
    //
    // `refreshTabBar` 挂在活动路上（`updateActivity` / `archiveTab` / `ensureTab` 末尾都
    // 无条件调它），而这些事件在拖拽那一两秒里照常来。下面第 4 段那个排序循环一跑，
    // **指针底下的 tab 就被换掉了** —— 用户松手落到的不是他瞄的那一格。
    // 已实证的两条路：① 会话跑完 ⇒ 归档 ⇒ 那个 tab 整个离开 `barEl`（抽屉是它的兄弟），
    // 下面的全部上移一格；② 新 bg 会话宣告 ⇒ `placeInOrder` 从**中间**插进去。
    //
    // ⚠ 守的是 `d.dragging`（真起拖了）而不是 `this.drag` 在不在 —— 后者在「按下还没动」
    // 那一段也为真，那段本来就该照常刷新（它与点击没有区别）。
    // ⚠ 不是丢掉这次刷新：记一笔脏，`teardownDrag` 收尾时补一次（见那里）。
    if (this.drag?.dragging) {
      this.tabBarDirtyDuringDrag = true;
      return;
    }
    // 1. 删
    const wanted = new Set(this.store.orderedIds);
    for (const sid of Array.from(this.tabButtons.keys())) {
      if (!wanted.has(sid)) {
        const refs = this.tabButtons.get(sid)!;
        refs.root.remove();
        this.tabButtons.delete(sid);
      }
    }

    // 2 + 3 + 4. 创建 / 更新 / 排序
    // 〔步 17·A〕抽屉没了 ⇒ 只剩主栏 ＋ 按集合分的若干组
    // ⇒ 推广成「**每容器一个游标**」。
    // 组容器按集合顺序先摆好（空集合也留着 —— 用户刚建的集合不该看不见）。
    for (const [id, g] of this.groupEls) {
      if (!this.collections.some((x) => x.id === id)) {
        g.wrap.remove();
        this.groupEls.delete(id);
      }
    }
    for (const col of this.collections) this.groupElFor(col);
    const cursors = new Map<HTMLElement, ChildNode | null>();
    // ★ **未归组的排在所有组之后**〔D 阶段补审〕。
    //
    // `barEl` 的游标若从 `firstChild` 起，散 tab 会插到**组容器之前** ——
    // 而 `P7a3-Y2` 逐字写的是「未归组的照常**在后面**」。
    // 实现与自己的 DoD 措辞不符，是那种「读起来都对、跑起来是另一回事」的差错。
    // ⇒ 把 `barEl` 的起点定在最后一个组容器上（没有组则回到 `firstChild` 语义）。
    const lastGroup = [...this.barEl.children]
      .filter((e) => e.classList.contains("tab-group"))
      .pop();
    if (lastGroup) cursors.set(this.barEl, lastGroup);
    for (const sid of this.store.orderedIds) {
      const tab = this.store.tabs.get(sid);
      if (!tab) continue;
      let refs = this.tabButtons.get(sid);
      if (!refs) {
        refs = this.createTabButton(sid);
        this.tabButtons.set(sid, refs);
      }
      this.updateTabButton(refs, sid, tab);
      // 〔步 17·A〕分流从三路（抽屉 / 组 / 主栏）降到**两路**（组 / 主栏）。
      // 「归档优先于集合」那条判定整条消失 ⇒ **灰 tab 也能在组里**（`§A.3` 逐字）。
      const col = collectionOf(this.collections, sid);
      const host = col ? this.groupElFor(col) : this.barEl;
      // 排序：希望此 button 出现在**同容器内**前一个之后。
      const prev = cursors.get(host) ?? null;
      const targetNext: ChildNode | null = prev ? prev.nextSibling : host.firstChild;
      if (refs.root !== targetNext || refs.root.parentElement !== host) {
        host.insertBefore(refs.root, targetNext);
      }
      cursors.set(host, refs.root);
    }

    this.store.notify();
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

    // A3：账号徽章（该会话属于哪个账号）。默认隐藏，updateTabButton 按 sessionBadge 填。
    const acctBadge = document.createElement("span");
    acctBadge.className = "tab-acct-badge";
    acctBadge.style.display = "none";
    root.appendChild(acctBadge);

    const badge = document.createElement("span");
    badge.className = "tab-badge";
    root.appendChild(badge);

    // 〔步 17·B〕📌 固定角标。默认不显（CSS `.tab:not(.pinned) .tab-pin { display:none }`），
    // `updateTabButton` 只翻 `.pinned` 这一个类 —— 与其它 5 个子元素同一套「一次性 append、
    // 可见性交给 class」的形状（见 `.tab .tab-badge` 那条注释）。
    const pinBadge = document.createElement("span");
    pinBadge.className = "tab-pin";
    pinBadge.textContent = "📌";
    pinBadge.title = "已固定：关掉 app 再打开它还在";
    root.appendChild(pinBadge);

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

    // ↗ 拉对应终端窗口（v1.7 用 sid_hwnd_cache）。〔第二波 T4 · LF1〕非 Windows 不渲（`terminal-front.ts`）。
    const focusBtn = document.createElement("span");
    focusBtn.className = "tab-focus";
    focusBtn.textContent = "↗";
    focusBtn.title = "调出对应终端 (`)";
    focusBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const t = this.store.tabs.get(sid);
      if (!t || t.status === "archived") return;
      // Feature ②：远端 Tab → 后端唯一分派点（先启动令牌、后 ccm-rbind 标题退路）；
      // 本地 Tab → 走原 sid_hwnd_cache 路径。
      if (t.origin !== null) {
        void bringRemoteTerminalToFront(sid);
      } else {
        void bringTerminalToFront(sid);
      }
    });
    focusBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    if (terminalFrontAvailable()) root.appendChild(focusBtn);

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
      // 〔步 17·A〕原先这里有一条「归档区里的 tab 不参与拖拽」的例外 ——
      // 抽屉没了，那条例外自动不需要（`§A.3` 逐字「净收益」）。
      this.beginTabDrag(e, sid, root);
    });
    // 中键点击归档 Tab 也关闭（常见 UX）
    root.addEventListener("mousedown", (e) => {
      if (e.button !== 1) return;
      const t = this.store.tabs.get(sid);
      if (t?.status === "archived") {
        e.preventDefault();
        this.closeTab(sid);
      }
    });
    // issue #10：右键菜单「在新窗口打开」（双屏 / 并排）。〔U2〕菜单里放哪几项住 `tab-menu.ts`。
    root.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      this.menu.open(e, sid);
    });

    return { root, label, badge, acctBadge, cwdBtn, pinBadge };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    refs.root.classList.toggle("active", sid === this.store.activeId);
    refs.root.classList.toggle("archived", tab.status === "archived");
    // 〔步 17·B〕固定：**只多一个 📌 角标，位置一个字不动**（`§B.3b`：没有「固定区」，
    // pin 管的是「别丢」不是「排前面」；位置由 `§C` 的顺序落盘管，两者不抢）。
    refs.root.classList.toggle("pinned", tab.pinned);
    refs.root.classList.toggle("has-cwd", !!tab.cwd);
    // FIX 5 / Feature ②（issue #15）：远端 Tab（origin 非 null）的 cwd 是 Pi 上的路径，
    // 本地不存在，故 .remote 类只隐藏「打开工作目录」📂（CSS）。「调出终端」↗ 现在保留
    // 给远端 —— 点击走 bringRemoteTerminalToFront（后端按 ccm-rbind 拉本地 ssh 窗口）。
    refs.root.classList.toggle("remote", tab.origin !== null);
    // Batch7-F24：bg 任务 tab——缩进 + ⌞ 前缀由 CSS 承担
    refs.root.classList.toggle("tab-bg", tab.kind !== null && tab.kind !== "interactive");
    // issue #23 红绿灯：busy=绿（.live-dot 默认色）/ idle·shell=红 / waiting=黄。
    // activity 为 null（旧版 CC / 远端 v1）不加类 → 维持现状绿点。
    // F91：语义抽到 session-status.ts 供 tab-bar 与 mission-control grid 共用（逐字节等价）。
    const actStatus = tab.activity?.status ?? null;
    const lightClass = activityLightClass(actStatus);
    refs.root.classList.toggle("act-idle", lightClass === "act-idle");
    refs.root.classList.toggle("act-waiting", lightClass === "act-waiting");
    // audit-fixes F03.2：idle-tmux 灰灯（claude 退但 tmux 会话仍在）。与 archived 正交——
    // status 仍 live（灯不被 archived 隐藏），tmux-idle 把 .live-dot 覆写为灰、压过红绿黄。
    refs.root.classList.toggle("tmux-idle", tab.tmuxIdle);
    const titleParts: string[] = [];
    if (actStatus === "waiting" && tab.activity?.waitingFor) {
      titleParts.push(`等待操作：${tab.activity.waitingFor}`);
    }
    // issue #63①：fork 会话在 tooltip 里标出血缘(徽标 `↳` 在标题上、来源 sid 在此)。
    if (tab.forkedFromSessionId) {
      titleParts.push(`↳ 从 ${tab.forkedFromSessionId.slice(0, 8)} fork 而来`);
    }
    refs.root.title = titleParts.join("\n");
    const unread = tab.unread > 0 && sid !== this.store.activeId;
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
    this.updateAccountBadge(refs, sid, tab); // A3：账号徽章随 tab 更新一并刷新
  }
}

// P5.2 B 重构：markCardUuid + feedBranchFolder 已搬到 render-stream-record.ts
// （三 caller 共用 renderStreamRecord 函数内部调用）。tabs.ts 不再持有这两个 helper。
