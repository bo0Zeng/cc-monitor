/**
 * 〔U2 · 第三波 · `设计/01 §1.5`「一个 store，一个 router」· `设计/99 §4.22.0`〕**拆分地图**。
 *
 * 拆之前（基线 `aede6f5d`，4991 行）本文件是**一个类干五件事**。现打逐件（谁在调 → 拆到哪）：
 *
 * | # | 这件事 | 谁在调（生产） | 拆到 |
 * |---|---|---|---|
 * | ① | **会话状态账**：tab 集合 · 顺序 · 当前 tab · 早于 tab 到达的信号暂存（已结束/可重连/红绿灯）· 账号快照 · 任务快照 · 「变了」那一份订阅 | `main.ts` / `entry-viewer.ts` 把 `events.ts` 的事件喂进来（`onLine` · `archiveTab` · `reviveTab` · `markTmuxIdle` · `updateActivity` · `updateTasks` · `createSkeletonTab` · `setSessionAccounts`）；`main.ts` 读投影（`snapshotSessions` · `peekSession` · `hasTab` · `activeRepoInfo` · `touchedFilesFor` · `getActiveSubagentContext` · `activeSessionId`） | `tab-store.ts`（store）· `tab-model.ts`（`Tab` 形状与标题）· `tab-session-facts.ts`（从记录里抽事实）· `tab-session-state.ts`（〔U4〕会话状态的两个轴：活性 × 可恢复性，转移与谓词） |
 * | ② | **路由**：切到哪个 tab、谁有权切（手动 5s 保护 · 自动跟随）、记住上次的 tab | `main.ts` 快捷键 / 命令面板 / 启动选 active（`switchTo` · `cycleActive` · `jumpToIndex` · `applyBehavior` · `persistLastActive` · `onManualSwitch`）；`onLine` 里真用户输入（`userActive`） | `tab-router.ts` |
 * | ③ | **实时流视图**：每个 tab 的流 DOM、按 seq 门控建卡、尾部窗口 / 骨架 / 上翻补批 / 哨兵 / 大纲、重放批 | `events.ts` → `onBatchStart` · `onLine` · `onBatchEnd`；`main.ts` DEV 探针 `debugSnapshot` | `tab-stream-view.ts` |
 * | ④ | **tab 栏视图**：按钮 · 徽章 · 分组 · 拖动排序与成组 · 固定 · 顺序落盘 | 用户手势；`main.ts` 启动 `loadCollections` · `loadPinned` · `loadOrder` | `tab-bar-view.ts` · `tab-bar-drag.ts` · `tab-drop.ts`（纯落点算术）· `tab-bar-prefs.ts`（集合 / 固定 / 顺序三份落盘） |
 * | ⑤ | **会话动作**：右键菜单（resume · 换号重启 · attach · 预览 · 杀会话 · 集合 · 固定）与它背后的 IPC（开目录 · 新窗口 · 切到终端窗口） | 用户右键；`main.ts` 快捷键 / 命令面板（`bringActiveTerminalToFront` · `openActiveTabCwd` · `openActiveInNewWindow` · `closeActiveIfArchived`） | `tab-menu.ts`（菜单项怎么组）· `tab-context-menu.ts`（菜单这个控件）· `tab-session-actions.ts`（动作本身；〔C4a〕IPC 经包装层 `ipc/commands.ts`） |
 *
 * 本文件拆完只剩 `TabManager` 这个**组装根**：对外 API（`main.ts` / `entry-viewer.ts` 调的那些）
 * 逐字不变，事件怎么在上面几份之间流转写在这里。拆分逐子步提交，每一步 `tabs.vitest` 全绿、断言不动。
 */
import { isCompactRecord } from "./cards";
import { runForkFlow } from "./fork-flow"; // G6：分叉完把新会话起起来（E78 起连反馈也在里面）
import type { BranchResult } from "./generated/BranchResult";
import { fetchSessionTasks, type TaskEntry, type TasksPanel } from "./tasks-panel";
import type { JsonlLinePayload } from "./events";
import { detectAccountMismatch, type SessionAccount } from "./accounts";
import type { BehaviorConfig } from "./behavior";
import { showActionFailureToast } from "./error-toast";
import { TailWindow } from "./live-window";
import type { AgentsPanel } from "./agents-panel";
import { turnEndNotifier } from "./turn-notify";
import type { GridSessionSnapshot, SessionPeek } from "./session-status";
import { contextPercent } from "./views/context-limit";
import {
  terminalFrontAvailable,
  TERMINAL_FRONT_UNAVAILABLE_TITLE,
  TERMINAL_FRONT_UNAVAILABLE_DETAIL,
} from "./terminal-front";
import { computeTitleFor, type Tab, type TabsSummary } from "./tab-model";
import { ENDED, LIVE, RECONNECTABLE, isResumeOnly, hasTerminal, nextState, type StateEvent } from "./tab-session-state";
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, originFromWire, type Origin } from "./ipc/origin";
// 〔U2〕`Tab` 的形状与标题函数搬去了 `tab-model.ts`；这里原样 re-export，既有 import 面零改动。
export type { Tab, TabsSummary } from "./tab-model";
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
import { TabBarPrefs } from "./tab-bar-prefs";
import { TabBarDrag } from "./tab-bar-drag";
import { TabBarView } from "./tab-bar-view";
import { TabRouter } from "./tab-router";
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
  /**
   * 〔U2 · ② · `设计/01 §1.5`〕**路由住 `tab-router.ts`**：下一个 / 第 N 个是谁、自动跟随放不放行、
   * 切完之后记住上次的 tab 与 5s 手动保护。切换本身的编排（可见性 · 物化 · 面板 · 贴底）仍在 `switchTo`。
   */
  private readonly router = new TabRouter(this.store);

  /**
   * 〔U2 · ③〕实时流视图住 `tab-stream-view.ts`。**在构造体里建，不写成字段初始化**：
   * 它要 `streamRootEl`，而字段初始化在参数属性赋值之前跑（esbuild 出原生 class field 时就是这个序），
   * 写成初始化器会拿到 `undefined`。
   */
  private readonly view: TabStreamView;

  constructor(
    /** tab 栏容器：本类只把它交给 tab 栏视图与拖拽（判据要它时读 `bar` / `dragger` 那两份，或用自己传进来的那个元素）。 */
    barEl: HTMLElement,
    streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI。〔U2〕它是 store 那一份订阅的第一个订阅者。 */
    onTabsChanged?: (summary: TabsSummary) => void,
    /** issue #11: 全局 TasksPanel，切 Tab / 收事件时由 TabManager 喂数据 */
    private tasksPanel?: TasksPanel,
    /** issue #23: 全局 AgentsPanel（subagent 列表 + 各自状态灯），喂数方式同 tasksPanel */
    private agentsPanel?: AgentsPanel,
  ) {
    if (onTabsChanged) this.store.subscribe(onTabsChanged);
    this.bar = new TabBarView(this.store, this.prefs, barEl, {
      refreshTabBar: () => this.refreshTabBar(),
      openTabCwd: (sid) => this.openTabCwd(sid),
      bringTerminalToFront: (sid) => bringTerminalToFront(sid),
      bringRemoteTerminalToFront: (sid) => bringRemoteTerminalToFront(sid),
      closeTab: (sid) => this.closeTab(sid),
      switchTo: (sid) => this.switchTo(sid),
      beginDrag: (e, sid, root) => this.dragger.begin(e, sid, root),
      takeSuppressedClick: (sid) => this.dragger.takeSuppressedClick(sid),
      openMenu: (e, sid) => this.menu.open(e, sid),
    });
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
      startForkedSession: (tab, res) => this.startForkedSession(tab, res),
    });
  }

  /**
   * 〔U2 · ④〕tab 栏的三份落盘偏好（集合 · 固定 · 顺序）住 `tab-bar-prefs.ts`。
   */
  private readonly prefs = new TabBarPrefs(this.store, {
    refreshTabBar: () => this.refreshTabBar(),
    createSkeletonTab: (sid, cwd, origin, kind, name) =>
      this.createSkeletonTab(sid, cwd, origin, kind, name),
    resumeTab: (sid) => this.actions.resumeTab(sid),
  });

  /** P7a-3：从 `config.json` 拉一次集合并重画。宿主启动时调一次。 */
  loadCollections(): Promise<void> {
    return this.prefs.loadCollections();
  }
  /** 〔步 17·B〕启动时把固定的 tab 复活出来（流程见 `tab-bar-prefs.ts` 那一份的头注）。 */
  loadPinned(): Promise<void> {
    return this.prefs.loadPinned();
  }
  /** 〔步 17·C〕启动时把落盘的顺序拉回来（为什么它曾是结构性 no-op 见 `tab-bar-prefs.ts` 那一份的头注）。 */
  loadOrder(): Promise<void> {
    return this.prefs.loadOrder();
  }
  /** 右键菜单那一项：翻转固定。 */
  togglePin(sid: string): void {
    this.prefs.togglePin(sid);
  }

  /**
   * 〔U2 · ④〕拖动排序 / 拖动成组 / 拖出去撕窗口的状态机住 `tab-bar-drag.ts`（落点算术住 `tab-drop.ts`）。
   * 在构造体里建：它要 `barEl`（参数属性，字段初始化时还没赋上）。
   */
  private readonly dragger: TabBarDrag;
  /**
   * 〔U2 · ④〕tab 栏视图（按钮 · 徽章 · 分组容器 · 整刷与帧末合批）住 `tab-bar-view.ts`。
   * 在构造体里建（要 `barEl`）。**整刷的入口仍是本类的 `refreshTabBar`**：拖拽守卫在这一层，
   * 而且判据会把实例上的 `refreshTabBar` 换成计数替身 —— 帧末合批那一刷必须经它。
   */
  private readonly bar: TabBarView;

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON（形状、口径与秤 6 的三个账本见 `tab-stream-view.ts` 那一份）。
   * 生产不接线,方法本身无副作用。
   */
  debugSnapshot(): string {
    return this.view.debugSnapshot();
  }

  /**
   * 〔U2 · ⑤〕会话动作住 `tab-session-actions.ts`（〔C4a〕它的 IPC 也经包装层 `ipc/commands.ts`）。它只要宿主给四样读数 / 回调。
   */
  private readonly actions = new TabSessionActions({
    tab: (sid) => this.store.tabs.get(sid),
    isAttachable: (sid) => this.isAttachable(sid),
    sessionAccount: (sid) => this.store.sessionAccountsByS.get(sid),
    refreshAccountBadgeFor: (sid) => this.bar.refreshAccountBadgeFor(sid),
    markRecord: (sid, present) => this.markRecord(sid, present),
  });

  /** 〔U2 · ⑤〕右键菜单里放哪几项住 `tab-menu.ts`；点下去做事直接交给上面那份 `actions`。 */
  private readonly menu = new TabMenu(
    {
      tab: (sid) => this.store.tabs.get(sid),
      isAttachable: (sid) => this.isAttachable(sid),
      collectionsLoaded: () => this.prefs.collectionsLoaded,
      collections: () => this.prefs.collections,
      commitCollections: (next) => this.prefs.commitCollections(next),
      pinnedLoaded: () => this.prefs.pinnedLoaded,
      togglePin: (sid) => this.togglePin(sid),
      requestPanoramaHighlight: (sid) => this.requestPanoramaHighlight?.(sid),
    },
    this.actions,
  );

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
      originFromWire(payload.origin),
    );

    // 按 seq 去重：旁路快照与实时行的重叠区是精确重复的 (sid, seq)（后端 seq = 行号，§25a），
    // 老后端重连还会从 seq 0 重发整段。必须在 renderStreamRecord 之前、且覆盖 skip 记录
    // （attachment/isMeta/空 user 有 seq 但不入 timeline，timeline.has 漏判）。
    // 〔CF1〕本机会话的行也走后端的帧与旁路快照之后，这一道本机同样会命中（原先写「本地永不命中」）。
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
    origin: Origin,
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
   * 无活跃 tab / 活跃 tab 无 cwd → 返 null。origin 是远端 = 远端会话（代码在远端机，
   * 本地 code-picture 索引不到，全景侧据此显式提示不索引）。**additive getter，只读，不改既有逻辑。**
   */
  activeRepoInfo(): { cwd: string; origin: Origin } | null {
    const tab = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (!tab || !tab.cwd) return null;
    return { cwd: tab.cwd, origin: tab.origin };
  }

  /**
   * F70：某会话在全景图上可高亮的「改动集」——cwd（定仓）+ 它写类工具碰过的文件。
   * **本地会话专属**：origin 是远端 / 无 cwd → 返 null（远端代码不在本机、code-picture
   * 索引不到，高亮不可用——门控就地做，呼应 activeRepoInfo）。**只读 getter，不落盘。**
   */
  touchedFilesFor(
    sid: string,
  ): { cwd: string; origin: Origin; files: string[] } | null {
    const tab = this.store.tabs.get(sid);
    if (!tab || !tab.cwd || isRemoteOrigin(tab.origin)) return null;
    return { cwd: tab.cwd, origin: LOCAL_ORIGIN, files: [...tab.touchedFiles] };
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
    this.bar.refreshAccountBadges();
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
        state: tab.state, // 〔U4〕两轴原样交出去：cell 与 tab-bar 读同一份、经同一组谓词
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
    const agents = [...tab.agents.values()]
      .map((a) => ({ label: a.label, status: a.status }))
      .sort((x, y) => (x.status === "running" ? 0 : 1) - (y.status === "running" ? 0 : 1));
    return {
      model: tab.latestModel,
      recentFiles: [...tab.touchedFiles],
      agents,
    };
  }

  /** Batch5-F19：switchTo 是否写回 last-active（viewer/tear-off 窗口置 false）。〔U2〕值住路由（`tab-router.ts`）。 */
  get persistLastActive(): boolean {
    return this.router.persistLastActive;
  }
  set persistLastActive(v: boolean) {
    this.router.persistLastActive = v;
  }

  /** Batch5-F19（G 验收）：用户手动切 tab 时回调——main.ts 用它清 pendingStartupActive，
   *  防迟到的远端宣告补切抢走用户已选的焦点。〔U2〕值住路由（`tab-router.ts`）。 */
  get onManualSwitch(): (() => void) | null {
    return this.router.onManualSwitch;
  }
  set onManualSwitch(fn: (() => void) | null) {
    this.router.onManualSwitch = fn;
  }

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
    origin: Origin = LOCAL_ORIGIN,
    kind: string | null = null,
    bgName: string | null = null,
  ): Tab {
    let tab = this.store.tabs.get(sessionId);
    if (tab) {
      // SSH 重连：远端会话掉线时被 flush 归档过，现在又收到它的行 = backend 在重放 = 会话仍
      // 活着 → 复活成 live。必须放在 ensureTab 里（在 onLine 的 seq 去重 return 之前），否则整段
      // 重放全被去重时连第一条行都走不到翻转。**仅远端**：本地归档由 PID 判活驱动，不靠「收到行」
      // 翻转，避免会话退出时尾写把已归档的本地 Tab 误复活（远端掉线归档是连接驱动，无此风险）。
      //
      // audit-fixes F03.2（D 审计修）：远端**可重连**的 tab 又收到后端重宣告 / jsonl 行 = claude
      // 复活（backend 只对活 pidfile 重宣告并推行；真 idle 会话已从 remote_active 移出、不重宣告也不
      // 推行）。这是「可重连 → 活」的**主**信号（queue 内、与行保序，SESSION_IDLE 恒排在会话末行之后，
      // 故复活行/重宣告严格晚于 idle）。不能只靠 session-activity：那是非 queue 同步派发、且
      // null-activity 的后端（远端 v1 无 status 字段）下永远不来 → 活跃流式会话永久卡在可重连。
      //
      // 〔U4〕上面两件事原先是两段（`status` 翻 live · `tmuxIdle` 清 false），因为两个轴挤在两个字段里；
      //   两轴之后它们是同一条转移：「远端见行」把**死了的**（已结束 / 可重连）翻回活（`nextState`）。
      // 〔CF2〕取回来的历史行不算「远端见行」（`TabStore.historyFeed` 头注）。
      if (isRemoteOrigin(tab.origin) && !this.store.historyFeed && this.applyState(tab, "remote-line")) {
        this.prefs.clearPinHint(sessionId); // 〔步 17·B〕远端复活：空态提示的对象没了
        this.refreshTabBar();
        this.emitTabStateProbe(tab); // F-E1:远端复活(死 → 活)
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

    const title = computeTitleFor(
      sessionId,
      cwd,
      null,
      isRemoteOrigin(origin) ? origin : null,
      kind,
      bgName,
    );

    const { streamEl, stream, branchFolder, timeline, inputsEl, inputsPanel, outline } =
      this.view.mountTabDom(sessionId);

    // v2.3.0 issue #11: 异步 fetch 初始 task 快照。task-update 事件路径并行更新
    // tasksBySid，两路收敛到同一份数据；若 sid 是 active 同步推给全局 panel。
    void fetchSessionTasks(sessionId, origin).then((tasks) => {
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
      state: LIVE, // 〔U4〕见了行 / 宣告了才建 ⇒ 活着；早到的死亡信号在下面落实
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
      tab.state = ENDED;
      tab.activity = null; // 同 archiveTab：死会话不留陈旧灯/tooltip
      this.store.pendingTmuxIdle.delete(sessionId); // 已结束优先：真 tmux 没了，暂存的可重连作废
      this.store.pendingContainer.delete(sessionId); // 〔U4b〕死了 ⇒ 容器一格由死的那一刻说了算
    } else if (this.store.pendingTmuxIdle.delete(sessionId)) {
      // audit-fixes F03.2：可重连信号早于建 Tab（F5 重放乱序）→ 落实。
      tab.state = RECONNECTABLE;
      this.store.pendingContainer.delete(sessionId);
    } else {
      // 〔U4b · G3〕容器事实早于建 Tab ⇒ 落实（建出来就是活的，转移只经 `nextState`）。
      const c = this.store.pendingContainer.get(sessionId);
      this.store.pendingContainer.delete(sessionId);
      if (c) tab.state = nextState(tab.state, c === "tmux" ? "container-tmux" : "container-none");
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
      isRemoteOrigin(tab.origin) ? tab.origin : null,
      tab.kind,
      tab.bgName,
      tab.forkedFromSessionId,
    );
  }

  /**
   * auto-e2e F-E1:tab 生命周期状态转移探针。在**真值点**(markTmuxIdle 进可重连 / archiveTab 进已结束 /
   * reviveTab 复活 / ensureTab 远端复活)emit 可 grep 的 `[e2e] tab-state` 行,gray-light 全链
   * 套件按它断言 活→可重连→已结束 序列(跨进程整链,单测碰不到)。self-gate
   * `import.meta.env.DEV`:生产构建整支(含模板串)被 vite 消除。
   * 〔U4〕行里两个键就是两个轴（原先是 `status=… tmuxIdle=…`）；`tests/e2e/graylight-suite.sh` 的两条 grep 同拍改，
   *   两边对得上由 `tests/tab-session-state.vitest.ts` 对拍。
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
   * 〔U4〕会话状态**只经这一处改**：转移表住 `tab-session-state.ts::nextState`，这里只落值。
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
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      // issue #19：Tab 还没被 ensureTab 建出来（归档信号早于 replay 行到达）——
      // 记下待归档，建 Tab 时落实。否则这里直接 return 会静默丢弃归档 → 僵尸 live Tab。
      this.store.pendingArchive.add(sessionId);
      this.store.pendingActivity.delete(sessionId); // issue #23：死会话的暂存灯一并清
      this.store.pendingTmuxIdle.delete(sessionId); // audit-fixes F03.2：已结束优先，清暂存的可重连
      this.store.pendingContainer.delete(sessionId); // 〔U4b〕死了 ⇒ 暂存的容器事实作废
      return;
    }
    // 活 / 可重连 ⇒ 已结束；已经是已结束 ⇒ 不变（`nextState`）。
    if (!this.applyState(tab, "ended")) return;
    // issue #23：会话结束 → 灯灭（CSS 上 `.ended` 本就隐藏 .live-dot，这里保持状态干净）
    tab.activity = null;
    this.sweepRunningAgents(tab); // 会话死了，running agent 必然中止
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
    this.prefs.clearPinHint(sessionId); // 〔步 17·B〕真接上了 ⇒ 那块「只能 resume」的空态该走了
    this.refreshTabBar();
    this.emitTabStateProbe(tab); // F-E1:本地复活(死 → 活)
  }

  /**
   * audit-fixes F03.2：远端 claude 退出但 tmux 会话仍在 → **可重连**（死 ＋ 容器还在）。
   * 后端 emitter 收 backend removed 且 `@ccm_sid` present 时 emit `session-idle` 驱动（**不**
   * 归档、不 forget）。Tab 未建（F5 重放乱序）则暂存待 ensureTab 落实。已结束的 Tab 不回到可重连
   * （真 tmux 没了才裁已结束，已结束优先）。无变化不重绘。离开可重连四处：
   * ensureTab（**主**：远端 tab 又收后端重宣告/行 = 复活，queue 内保序）/ updateActivity
   * （claude 再产活动，非 queue 的次要信号）/ reviveTab（本地）/ archiveTab（tmux 真没了）。
   * 〔U4〕改之前这里只置 `tmuxIdle = true`、`status` 留在 live —— 活性一轴说了假话。
   */
  markTmuxIdle(sessionId: string): void {
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

  /**
   * 〔U4b · 第四波 · G3〕后端报来这条活会话的容器（`session-container`：`"tmux"` / `"none"`）。
   * Tab 还没建 ⇒ 暂存（同 `pendingActivity`），建 Tab 时落实；不认识的取值当没报（丢掉）。
   * 只落在活着的会话上（`nextState`）：死了的那一格由死的那一刻的裁决说了算。
   */
  noteContainer(sessionId: string, container: string): void {
    if (container !== "tmux" && container !== "none") return;
    const tab = this.store.tabs.get(sessionId);
    if (!tab) {
      this.store.pendingContainer.set(sessionId, container);
      return;
    }
    if (!this.applyState(tab, container === "tmux" ? "container-tmux" : "container-none")) return;
    this.refreshTabBar();
  }

  /**
   * 〔U4b · 第四波 · 说不清〕这台机器的活会话清单报完了（远端 `origin-sessions-listed`；本机 `list_active_sessions`）。
   *
   * 这台的「说不清」（固定复活、还没被报过）逐条落地：`liveSids` 里有 ⇒ 活（本机那条路给清单；远端的清单
   * 早已经由 `remote-session-added` 把 tab 建成活的了，不传）；没有 ⇒ 已结束（`设计/30 §3.5.7a`
   * 「A 看得见却没报这条」）。记下这台已报完 ⇒ 之后才复活出来的固定 tab 直接落已结束。
   */
  markOriginSeen(origin: Origin, liveSids?: ReadonlySet<string>): void {
    this.store.seenOrigins.add(origin);
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

  /**
   * 〔U4b · 第四波 · G1〕resume 一跳问过那台后端：这条会话的记录在不在（`history-record`）。
   * 不在 ⇒ 已结束落到「记录没了」；在 ⇒ 「记录没了」翻回已结束（记录回来了，例：同步盘补齐）。
   * 别的态不动（`nextState`：可重连的终端还在，接得回去；活的不走 resume）。
   */
  markRecord(sessionId: string, present: boolean): void {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return;
    if (!this.applyState(tab, present ? "record-present" : "record-gone")) return;
    this.refreshTabBar();
    this.emitTabStateProbe(tab);
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
    // 已结束不更新（审计：心跳清死会话后磁盘残留 PID.json 被重扫会推陈旧
    // activity，已结束的 tab 会挂上过期的 waiting tooltip——灯本身被 CSS 隐藏）。
    if (isResumeOnly(tab.state)) return;
    // audit-fixes F03.2：收到活动信号 = claude 活着（远端 activity 仅在 claude 存活时由
    // backend 推）→ 可重连回到活。必须放在下方「无变化早退」之前：复活后首个 activity 未必与
    // 之前的陈旧 activity 值不同，否则被早退跳过、回不到活。回到活即使 activity 没变也要重绘。
    const clearedIdle = act !== null && this.applyState(tab, "activity");
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
   * 仅允许关闭**已结束**（只能 resume）的 Tab，避免误关运行中的会话（`tab-session-state.ts::isResumeOnly`；
   * 可重连的不在其中 —— 与改两轴之前逐条相同）。
   * forget 后该 session 不会在下次 F5 刷新时被 event_replay 重放复活。
   */
  closeTab(sessionId: string): void {
    const tab = this.store.tabs.get(sessionId);
    if (!tab) return;
    if (!isResumeOnly(tab.state)) return;

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
      this.prefs.pinnedRecords.delete(sessionId);
      void this.prefs.persistPinned();
    }
    this.prefs.clearPinHint(sessionId);

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
    // 〔U2〕五道跳过条件（批期 · 开关 · 5s 手动保护 · 没有 / 已归档 · 已经是它）住路由（`tab-router.ts::autoFollow`）。
    const decision = this.router.autoFollow(sessionId);
    if (decision === "ignore") return;
    // 已经在这个 tab（`front-only`）但用户开了"拉前 monitor"也照拉
    if (decision === "switch") this.switchTo(sessionId, "auto");
    if (this.router.bringMonitorToFront) bringMonitorToFront();
  }

  /** 快捷键 Ctrl+W：当前活跃 Tab 已结束才关，活着 / 可重连的不动（同 `closeTab`） */
  closeActiveIfArchived(): void {
    if (!this.store.activeId) return;
    const tab = this.store.tabs.get(this.store.activeId);
    if (tab && isResumeOnly(tab.state)) {
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
    // 〔U4〕还有终端可去（活着，或可重连：登录 shell 的 ssh 窗还在）才拉。
    if (!tab || !hasTerminal(tab.state)) return;
    // Feature ②：远端 Tab → 后端唯一分派点（先启动令牌、后 ccm-rbind 标题退路）；本地 Tab → 原 sid_hwnd_cache 路径。
    if (isRemoteOrigin(tab.origin)) {
      void bringRemoteTerminalToFront(this.store.activeId);
    } else {
      void bringTerminalToFront(this.store.activeId);
    }
  }

  /** 〔SE2〕快捷键 Ctrl+F（`session.find`）：当前 tab 的查找面板打开到「搜索」。实现在流视图。 */
  openFind(): void {
    this.view.openFind();
  }

  /** 快捷键 Ctrl+Shift+E：打开当前活跃 Tab 的工作目录到系统文件管理器 */
  openActiveTabCwd(): void {
    if (!this.store.activeId) return;
    void this.openTabCwd(this.store.activeId);
  }

  /** F77：活跃 tab 的子 agent 加载上下文（parentPath + origin）——main.ts 点 agent 行时用它
   *  调 `load_subagent`。无活跃 tab / 无 parentPath → null。 */
  getActiveSubagentContext(): { parentPath: string; origin: Origin } | null {
    const tab = this.store.activeId !== null ? this.store.tabs.get(this.store.activeId) : undefined;
    if (!tab || !tab.parentPath) return null;
    return { parentPath: tab.parentPath, origin: tab.origin };
  }

  /** issue #10 快捷键 Ctrl+Shift+N：把当前活跃 Tab 在独立只读窗口打开 */
  openActiveInNewWindow(): void {
    if (this.store.activeId) void this.openInNewWindow(this.store.activeId);
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
    // Batch5-F19 记住所在 tab ＋ 手动切的 5s 保护（〔U2〕住路由：`tab-router.ts::noteSwitched`）。
    this.router.noteSwitched(sessionId, source);
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
    if (this.dragger.deferRefresh()) return;
    this.bar.refresh();
  }

}

// P5.2 B 重构：markCardUuid + feedBranchFolder 已搬到 render-stream-record.ts
// （三 caller 共用 renderStreamRecord 函数内部调用）。tabs.ts 不再持有这两个 helper。
