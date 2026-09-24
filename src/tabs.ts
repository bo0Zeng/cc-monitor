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
 * | ⑤ | **会话动作**：右键菜单（resume · 换号重启 · attach · 预览 · 杀会话 · 集合 · 固定）与它背后的 IPC（开目录 · 新窗口 · 切到终端窗口） | 用户右键；`main.ts` 快捷键 / 命令面板（`bringActiveTerminalToFront` · `openActiveTabCwd` · `openActiveInNewWindow` · `closeActiveIfArchived`） | `tab-menu.ts`（菜单项怎么组）· `tab-context-menu.ts`（菜单这个控件）· `tab-session-actions.ts`（动作本身 ＋ 本层唯一的 IPC 出口） |
 *
 * 本文件拆完只剩 `TabManager` 这个**组装根**：对外 API（`main.ts` / `entry-viewer.ts` 调的那些）
 * 逐字不变，事件怎么在上面几份之间流转写在这里。拆分逐子步提交，每一步 `tabs.vitest` 全绿、断言不动。
 */
import { MessageStream } from "./stream";
import {
  reconcilePendingToolResults,
  isCompactRecord,
  type RenderContext,
} from "./cards";
import { BranchFolder } from "./branch-fold";
import { attachBranchButton } from "./branch-button"; // G4：实时会话的分叉入口
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
import { RecordTimeline } from "./record-timeline";
import { TailWindow, type SkeletonLedger } from "./live-window";
// 〔`设计/10` 骨架 · 子步 4〕骨架层（占位 ＋ 只物化可见区）。接入点全部带「骨架」字样，搜得到。
import { SkeletonView, ledgerFromIndex } from "./skeleton-view";
import { skeletonKind } from "./height-estimate";
// K-R45 乙（`KR45D2`）：「大纲」。界面 / 跳 与历史查看器共用同一份；〔SE1〕清单问后端要（`OutlineSource`）。
import { UserInputPanel } from "./views/user-input-panel";
import { OutlineSource } from "./views/outline-source";
// ⚠ **实时窗口 import 历史查看器，方向是别扭的 —— 这是写区逼出来的将就，不是惯例。**
// 共用的只有 `revealCard`（找卡→展开→滚，两条路的卡由同一份渲染器建）。把它搬进中立文件
// 要同时改 `src/bridge/src/polling_registry.rs` 的调度点分类账（rAF/setTimeout 按文件精确对账），
// 而 `src/bridge/` 不在本轮写区 —— 实测搬了就红。理由与读数在 `revealCard` 的头注 + 件 `§5.6`。
import { revealCard } from "./views/session-viewer";
import {
  renderContentRecord,
  routeMetaAndBranch,
  type MetaSink,
  type StreamSink,
} from "./render-stream-record";

/** 〔U3b〕只问「这条是不是 meta」、不喂任何账的空 sink（骨架按偏移取回**见过**的行时用）。 */
const NOOP_META: MetaSink = { onBranchRecord: () => {}, onQueueOperation: () => {} };
import type { BranchRecord } from "./branching";
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
// ⚠ **两个同名常量**：本文件要的是 `backend-policy` 那个（`"<local>"`，与 Rust
// `inbound_client::LOCAL_ORIGIN` 逐字节相同、有跨语言判据钉着）；`accounts.ts` 里那个是
// `"__local__"`，是账号面自己的标记，**不是 backend origin**。导错一个不会红，只会静默查不到。
import { LOCAL_ORIGIN } from "./backend-policy";
import { commands } from "./ipc/commands";
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

/**
 * 秤 6(`设计/17 §6` 表第 6 行):读 `BranchFolder.records` 的**条数**,给 `debugSnapshot`。
 *
 * # 为什么是按结构读,不是加一个 getter
 *
 * `records` 是 `BranchFolder` 的 private 字段,而本轮的写区**不含** `branch-fold.ts`
 * (同一棵树上还有别路 agent 在写)。TS 的 `private` 只活在编译期,运行时它就是个普通
 * 字段 ⇒ 这里按名字读一次。**只给 DEV 探针用,无副作用、不改任何行为。**
 * 哪天 `branch-fold.ts` 可写了,把这里换成一个 `get recordCount()` 是纯收窄。
 *
 * # 读不到时返 -1,不返 0
 *
 * 字段一旦改名,返 0 会被读成「**账本是空的**」—— 那是一句假话,而且是**朝着"看起来
 * 一切正常"的方向**假(同篇 `§6` 反复点名的那一族:坏掉的尺子把真缺陷一起藏起来)。
 * 返 -1 在读数里一眼就是「这根尺子断了」。`tests/scale6-memory-ledger.vitest.ts`
 * 有一格专钉「它不许是 -1」。
 *
 * # 它量的是条数,不是字节
 *
 * 一条 `BranchRecord` 是 `{uuid, parentUuid, timestamp}` 三个短字符串,**不含正文** ——
 * 这正是下面那句判词的一半依据。想要字节得另外称,本秤不称。
 */
function branchRecordCount(folder: BranchFolder): number {
  const inner = folder as unknown as { records?: unknown };
  return Array.isArray(inner.records) ? inner.records.length : -1;
}

export class TabManager {
  private tabs = new Map<string, Tab>();
  /** 按插入顺序的 sessionId 数组，与 this.tabs.keys() 顺序一致但避免每次 Array.from */
  private orderedIds: string[] = [];
  /**
   * 〔步 17·C · 2026-09-21〕**盘上那份顺序（`tabBar.order`），启动读一次之后留着。**
   *
   * 🔴 **它是「一份意图」，不是一次性的动作** —— 这就是那个 no-op 的修法所在
   *   （成因与现打见 `loadOrder` 头注）。tab 是**陆续**到的，所以这份顺序必须活过
   *   整个启动窗口期，每来一个 tab 就再应用一次（`placeInOrder` → `applySavedOrder`）。
   * ⚠ 里面**允许有今天不存在的 sid**（被删的 / 还没宣告到的）——
   *   它们进不了 `orderedIds`（`applySavedOrder` 按 `present` 筛），所以不会造出假 tab；
   *   上界由 `ORDER_CAP` 在读的那一侧管。
   * ⚠ 用户一拖，盘上那份就**过期**了 ⇒ `persistOrder` 落盘的同时把这里同步成新的那张，
   *   否则后到的 tab 会拿一份旧顺序把用户刚拖的一下撤销。
   */
  private savedOrder: string[] = [];
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  private tabButtons = new Map<string, TabButtonRefs>();
  /** A3：远端 live 探测的会话账号归属（sid → 探测行）。main.ts 定期喂。 */
  /**
   * E73：sid → **attach 进去对人有没有意义**（来自 pidfile 的 `attachable`，经后端帧透传）。
   *
   * 只记**显式 false** 的那些。缺席 = 可以 —— 存量会话与旧后端一律照旧，零迁移。
   *
   * # 为什么单独一张表而不是 `Tab` 的字段
   *
   * 加字段要动 `ensureTab` 的位置参数列车（R03 刚把那种形状收拾过一轮），而这就是
   * 「某个 sid 的一条会话级元信息」—— 与 `sessionAccountsByS` 同形，放这儿更合身。
   */
  private notAttachableSids = new Set<string>();

  private sessionAccountsByS = new Map<string, SessionAccount>();
  /** A3：账号名 → 邮箱（徽章 tooltip 用）。 */
  private accountEmailByName = new Map<string, string>();
  /** A4：sid → lastAccount（history-metadata）。徽章源②：live 探测不到时兜底。main.ts 定期喂。 */
  private accountLastByS = new Map<string, string>();
  /** A4/§7：账号可查询的远端 origin 集（available）。只有这些 origin 的会话才显徽章。 */
  private accountReadyOrigins = new Set<string>();
  /** account-ux U5：origin → 当前账号名。徽章「信息才显」比对：会话账号==它 → 不挂徽章。main.ts 定期喂。
   *  **只放 isSelectable 的账号**（main.ts 侧过滤）：不可选的当前账号对齐必失败，指着它说"你不一致"
   *  是假信息，且与 U1 `resolveFollowAccount`「不可选就下沉」的语义保持一致。 */
  private currentByOrigin = new Map<string, string>();
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
   * audit-fixes F03.2：灰灯（idle-tmux）信号早于 Tab 建出时暂存（同 pendingArchive 模式）。
   * F5 frontend-ready 重放会重发 SESSION_IDLE，可能早于骨架 remote-added 建 Tab——不暂存则
   * markTmuxIdle no-op、灰灯丢。ensureTab 建 Tab 时落实（除非同时 pendingArchive→归档优先）。
   */
  private pendingTmuxIdle = new Set<string>();
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

  /**
   * 〔U2 · ⑤〕会话动作住 `tab-session-actions.ts`（tab 层唯一的 IPC 出口）。它只要宿主给四样读数 / 回调。
   */
  private readonly actions = new TabSessionActions({
    tab: (sid) => this.tabs.get(sid),
    isAttachable: (sid) => this.isAttachable(sid),
    sessionAccount: (sid) => this.sessionAccountsByS.get(sid),
    refreshAccountBadgeFor: (sid) => this.refreshAccountBadgeFor(sid),
  });

  /** 〔U2 · ⑤〕右键菜单里放哪几项住 `tab-menu.ts`；点下去做事直接交给上面那份 `actions`。 */
  private readonly menu = new TabMenu(
    {
      tab: (sid) => this.tabs.get(sid),
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
      // 步 3：「够不够一屏」改读**真实布局**（见 `contentReachesBottom` 的头注）。
      const notFilled = !this.contentReachesBottom(active);
      if (active.window.floorSeq === null || notFilled) {
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
    // 〔`设计/10` 骨架〕active tab 此刻一定有渲染后缀了 ⇒ 要索引（在途/要过就不重复）
    if (active) this.requestSkeleton(active);
    if (active?.outline.needsFetch) this.refreshOutline(active); // 〔SE1〕大纲
    // F88b：批期 trackUsage 只更了 tab 字段没喂 chip，这里对活跃 tab 单次 flush 到 HUD
    // （批内多条 assistant 记录只刷一次，消视觉抖动）。
    this.onActiveUsageChanged?.(active?.latestModel ?? null, active?.latestPromptTokens ?? null);
  }

  /**
   * ★ 步 3（`设计/10 §6`）：**「够不够一屏」改读真实布局。**
   *
   * # 旧判据错在哪
   *
   * 原来两处都写的是 `el.scrollHeight - el.clientHeight > 1`（「滚得动吗」）。
   * 那**不是**「屏幕填满了吗」—— 两者在这个仓里经常不是同一件事：
   * - 每张顶层卡都带 `content-visibility: auto` + `contain-intrinsic-size: auto <估值>`
   *   （`height-estimate.ts`）。**没渲染过的卡贡献的是估值**，`scrollHeight` 里掺着一笔
   *   与屏幕上看到的东西无关的账 ⇒ 估高了就"看起来滚得动"，而屏幕仍是半屏。
   * - 工具密集会话一轮物化可能只产出几张 34px 的合并卡，`scrollHeight` 差一两像素就
   *   越过 `>1` 这条线 ⇒ 补批当场停手。这正是用户报的「上下半屏」。
   *
   * # 新判据
   *
   * **最后一张卡的 `getBoundingClientRect().bottom` 有没有够到滚动容器的下沿。**
   * 这两个数都来自真实布局，不吃估值。
   *
   * ⚠ **没有布局时必须退回旧判据**：jsdom 无布局引擎，所有 rect 恒为 0
   * （容器 rect 高度也是 0）。这时按新判据算恒等于「满了」⇒ 补批整条路在测试环境里
   * 被静默关掉。所以这里显式探一次「这台机器给不给布局」，给不了就走老的算术判据 ——
   * **降级要写出来，不能靠恰好**。
   */
  private contentReachesBottom(tab: Tab): boolean {
    const el = tab.streamEl;
    const view = el.getBoundingClientRect();
    if (view.height <= 0) {
      // 拿不到真实布局（jsdom / 还没插进文档 / tab 不可见）⇒ 退回旧的算术判据。
      return el.scrollHeight - el.clientHeight > 1;
    }
    const last = tab.stream.contentElement.lastElementChild;
    if (!last) return false; // 一张卡都没有 ⇒ 肯定没满
    // 1px 容差：HiDPI 分数像素下 rect 是小数，卡刚好贴到下沿时会差零点几像素。
    return last.getBoundingClientRect().bottom >= view.bottom - 1;
  }

  /**
   * D 审计 R-3:一次 150 条 payload 可能只产出几张卡(tool-group 合并成单卡 34px、
   * skip 记录占配额不产卡)——工具密集会话一轮物化后屏幕仍近空,而 F40a 没有上翻
   * 补批兜底。有界循环补到**一屏填满**或账本弹尽(≤4 轮防病态会话空转)。
   * 〔步 3〕停手条件从「滚得动」换成 `contentReachesBottom`——理由见它的头注。
   */
  private materializeUntilFilled(tab: Tab): void {
    if (tab.skeleton) {
      tab.skeleton.fillVisible(); // 〔`设计/10` 骨架〕同上
      return;
    }
    for (let round = 0; round < 4; round++) {
      if (tab.window.pendingCount === 0) return;
      if (round > 0 && this.contentReachesBottom(tab)) return;
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
      // G6：**远端也挂**。〔`K-R88` 09-13〕本机那条命令也收 sid 了 ⇒
      // **两条路都只要 sid**，「本机拿不到 jsonl 路径就不能分叉」这道门跟着没了
      // （原先那个随迭代更新的路径游标也一并去掉：没人再要那个值）。
      onCardRendered: (el, msg) => {
        if (msg.type !== "user" && msg.type !== "assistant") return;
        if (!msg.uuid) return;
        attachBranchButton(el, {
          uuid: msg.uuid,
          sourceSessionId: tab.sessionId,
          origin: tab.origin,
          cwd: tab.cwd ?? undefined,
          onForked: (res) => void this.startForkedSession(tab, res),
        });
      },
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
   * 〔`设计/10` 骨架 · 子步 4〕向后端要这个会话的**骨架索引**，到了就接骨架。
   *
   * 只对**已经有渲染后缀**的 tab 要（`floor !== null`：骨架顶的是 `[0, floor)`）；
   * 每个 tab 只要一次（`skeletonFetch`），成不成都不重拉 —— 免得每切一次 tab 起一次本机后端进程。
   * 调用点只有两处：批结束时的 active tab、`switchTo` 切进来的那个 tab ⇒ 后台 tab 不花这一次。
   */
  private requestSkeleton(tab: Tab): void {
    if (tab.skeletonFetch !== "idle" || !tab.parentPath) return;
    if (tab.window.floorSeq === null) return;
    tab.skeletonFetch = "pending";
    const jsonlPath = tab.parentPath;
    const origin = tab.origin ?? LOCAL_ORIGIN;
    void commands
      .read_session_index({ origin, jsonlPath, fromOffset: 0 })
      .then(async (res) => {
        if (this.tabs.get(tab.sessionId) !== tab) return; // 期间关掉了
        tab.skeletonFetch = "done";
        const got = ledgerFromIndex(res);
        if (!got.ok) {
          console.info(`[tabs] 骨架未接（${tab.sessionId.slice(0, 8)}）：${got.reason}`);
          return;
        }
        // 索引拉回来之前 tab 可能又长了：floor 之下还有索引没覆盖到的行 ⇒ **续传**（从上次的 end 接着要）
        const floor = tab.window.floorSeq ?? 0;
        if (floor > got.ledger.endSeq) {
          const more = await commands.read_session_index({ origin, jsonlPath, fromOffset: got.end });
          if (more.available) got.ledger.append(more.rows);
        }
        if (this.tabs.get(tab.sessionId) !== tab) return;
        this.attachSkeleton(tab, got.ledger);
      })
      .catch((e: unknown) => {
        tab.skeletonFetch = "done";
        console.warn(`[tabs] 骨架索引拉取失败（${tab.sessionId.slice(0, 8)}）：`, e);
      });
  }

  /**
   * 接骨架：`[0, floor)` 画成占位。**接之前先对拍 seq 空间** —— 抽几条还在 pending 的记录，
   * 它们的 uuid 在索引里必须落在同一个 seq 上；对不上（本地截断重读换过 seq，INVARIANTS §25）
   * 就**不接**，退回尾部窗口（不许硬对）。
   */
  private attachSkeleton(tab: Tab, ledger: SkeletonLedger): void {
    const floor = tab.window.floorSeq;
    if (floor === null || tab.skeleton) return;
    for (const p of tab.window.peek(8)) {
      const u = (p.message as { uuid?: unknown }).uuid;
      if (typeof u !== "string") continue;
      const at = ledger.uuidToSeq.get(u);
      if (at !== p.seq) {
        console.warn(
          `[tabs] 骨架未接（${tab.sessionId.slice(0, 8)}）：seq ${p.seq} 在索引里是 ${String(at)} —— seq 空间对不上`,
        );
        return;
      }
    }
    const view = new SkeletonView(ledger, tab.streamEl, tab.timeline, {
      materialize: (lo, hi) => {
        const taken = tab.window.takeRange(lo, hi);
        this.renderPayloadsBatch(tab, taken);
        this.fetchMissingRows(tab, ledger, lo, hi, new Set(taken.map((p) => p.seq)));
      },
    });
    // 在视口上方插一块高占位：同 `fillAbove` 的纪律 —— 关原生锚定、同一个同步任务里按 ΔscrollHeight 补偿
    const el = tab.streamEl;
    const beforeH = el.scrollHeight;
    const beforeTop = el.scrollTop;
    try {
      el.style.overflowAnchor = "none";
      view.attach(floor);
      el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
    } finally {
      el.style.overflowAnchor = "";
    }
    tab.skeleton = view;
    this.updateSentinel(tab);
    if (this.activeId === tab.sessionId) view.fillVisible();
    // 〔U3b · `设计/10` 步 8〕骨架接上 ⇒ 正文不必再驻留：丢掉的那些滚到时按偏移要回来。
    // ① 前端账本只留离已渲染尾巴最近的一批（第一次上翻不用等 IPC）；
    // ② monitor 的重放缓冲只留尾巴（F5 之后也只重放尾巴，其余同样按偏移要）。
    tab.window.keepHighest(TabManager.FILL_BATCH);
    void commands
      .replay_keep_tail_only({ sessionId: tab.sessionId })
      .catch((e: unknown) => console.warn(`[tabs] 重放缓冲留尾巴失败（${tab.sessionId.slice(0, 8)}）：`, e));
  }

  /**
   * 〔`设计/10` 骨架 · 子步 5〕**按偏移取正文**：物化 `[lo, hi)` 时，账本里没有、也还没到过的那些
   * 会建卡的行（`seenSeqs` 里没有、索引说它不是「不建卡」的那种）⇒ 按索引里的字节边界向后端要
   * （`read_session_range` = `--read-session-from-offset … --until`），回来的行**走 `onLine` 全套**
   * （去重、旁路记账、门控 —— 这段已经不在占位里了，门控会就地建卡），与重放来的行一视同仁。
   *
   * 今天它补的是「重放还没推到」的那一截（远端尾部优先快照的回填期、大会话启动重放的在途期）；
   * 它也是「骨架不带正文」那条路的另一半 —— 等 `EventReplay.history` 加上界（`设计/10 步 8`），
   * 没推过来的历史就全靠它取。连续缺的行并成一段、一段一次 IPC；同一段不会被要两次
   * （骨架把它标成已物化之后就不会再交给宿主）。
   */
  private fetchMissingRows(
    tab: Tab,
    ledger: SkeletonLedger,
    lo: number,
    hi: number,
    taken: ReadonlySet<number>,
  ): void {
    if (!tab.parentPath) return;
    const runs: Array<[number, number]> = [];
    for (let s = lo; s < hi; s++) {
      const f = ledger.factsOf(s);
      // 〔U3b〕「缺」= 会建卡、而这一次没从账本里取到 —— 两种来历：重放没推过来（没见过），
      // 或见过、但骨架接上之后被丢出账本（`keepHighest`）。两种回来之后喂法不同，见下。
      const missing = f !== undefined && skeletonKind(f) !== "none" && !taken.has(s);
      if (!missing) continue;
      const last = runs[runs.length - 1];
      if (last && last[1] === s) last[1] = s + 1;
      else runs.push([s, s + 1]);
    }
    const origin = tab.origin ?? LOCAL_ORIGIN;
    for (const [a, b] of runs) {
      const first = ledger.factsOf(a)!;
      const lastRow = ledger.factsOf(b - 1)!;
      void commands
        .read_session_range({
          origin,
          jsonlPath: tab.parentPath,
          offset: first.o,
          until: lastRow.o + lastRow.n,
          seqBase: a,
          lineCount: b - a,
        })
        .then((payloads) => {
          if (this.tabs.get(tab.sessionId) !== tab) return;
          // 没见过的 ⇒ 走 `onLine` 全套（旁路记账、去重、门控）；
          // 见过的 ⇒ 旁路账早记过了、去重会把它拒掉 ⇒ 只建卡（meta 那几类照旧不建）。
          const fresh = payloads.filter((p) => !tab.seenSeqs.has(p.seq));
          const again = payloads.filter(
            (p) => tab.seenSeqs.has(p.seq) && routeMetaAndBranch(p, NOOP_META) === "content",
          );
          this.feedHistoryRows(tab, fresh);
          if (again.length > 0) this.renderPayloadsBatch(tab, again);
        })
        .catch((e: unknown) => console.warn(`[tabs] 按偏移取正文失败 [${a},${b})：`, e));
    }
  }

  /**
   * 按偏移取回的**历史**行喂进 `onLine` —— 必须按**重放**的语义喂，不能按 live：
   * live 语义下历史 user 卡会触发 `userActive`（自动切 tab / 拉前 monitor）、
   * 历史的轮次结束会弹系统通知、每条 `recordAdded` 都重算一次主线。
   * ⇒ 对这一个 tab 走一遍批：`inBatch` 置位（`userActive` / `turnEndNotifier` 都认它）、
   * 折叠层进批模式；喂完把批期缓冲的中部插入一次挂载、折叠层 flush。
   * 若此刻本来就在一个真批里（启动重放未完），只喂不收 —— 真批的 `onBatchEnd` 会收。
   */
  private feedHistoryRows(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const wasBatch = this.inBatch;
    this.inBatch = true;
    tab.branchFolder.setBatchMode(true);
    try {
      for (const p of payloads) this.onLine(p);
    } finally {
      this.inBatch = wasBatch;
      if (!wasBatch) {
        this.flushMidBatchBuffer(tab);
        tab.branchFolder.flushPending();
        tab.branchFolder.setBatchMode(false);
      }
    }
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
    // 〔`设计/10` 骨架〕接上了 ⇒ 不再「从尾巴往上一批批补」，只物化与视口相交的那段占位
    if (tab.skeleton) {
      tab.skeleton.fillVisible();
      return;
    }
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
   *
   * 🔴 秤 6(`设计/17 §6` 表第 6 行)在这里加了**三个账本的条数**:
   * `branchRecords` / `userInputs` / `pending`。口径与「量不到什么」写在
   * `branchRecordCount` 的头注与 `tests/evidence/S6-memory-ledger.md` 里。
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
      // 秤 6 的三个账本(`pending` 本来就在,不重复开一个字段):
      // ① `TailWindow.pending` —— 还没上屏的整条 payload,**这一份是真的文本驻留**
      pending: tab.window.pendingCount,
      // ② `BranchFolder.records` —— 每条一个 {uuid,parentUuid,timestamp} 三元组,不含正文
      branchRecords: branchRecordCount(tab.branchFolder),
      // ③ 大纲的条数 —— 〔SE1〕清单问后端要，前端只留面板上那几行（每行一份 80 字摘要）
      userInputs: tab.outline.count,
      midBuffer: tab.midBatchBuffer.length,
      // 〔`设计/10` 骨架〕接上没有 · 索引总条数 · 还在占位里的行数 · 占位块数（`timeline` 里含占位条目）
      skeleton: tab.skeleton
        ? {
            rows: tab.skeleton.ledger.count,
            pendingRows: tab.skeleton.pendingRows,
            gaps: tab.skeleton.gapCount,
          }
        : tab.skeletonFetch,
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
    // 〔`设计/10` 骨架〕接上了 ⇒ 占位本身就是「上面还有」，哨兵退场
    if (n === 0 || tab.skeleton) {
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
    this.noteOutlineLine(tab);

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
    turnEndNotifier.observe(payload.session_id, tab.title, payload, this.inBatch);

    // issue #23（第二增量）：配对 agent 工具调用，喂 AgentsPanel
    this.trackAgents(tab, payload.message);

    // F88b：捕获带 usage 的 assistant 记录 → 更新本会话最新 prompt token+model（供 HUD context%）。
    // 与 trackAgents 同处（双重去重之后，重投不重复累）。活跃会话则即时推给 HUD。
    this.trackUsage(tab, payload.message, payload.seq);

    // F70：累进本会话改动集（写类工具 file_path）——放在双重去重之后（重投不重复累），
    // 渲染/收纳门控之前（连"收纳不建卡"的记录也计入）。纯增量、无 DOM。近因序那条理由见 `noteTouchedFiles` 头注。
    noteTouchedFiles(tab, payload.message);

    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: (rec: BranchRecord) => tab.branchFolder.recordAdded(rec),
      // issue #36：队列消息内容 → 折叠豁免集合
      onQueueOperation: (content: string) => tab.branchFolder.addQueuedContent(content),
      onTitleUpdate: (title: string) => this.applyAiTitle(tab, title),
      onRealUserInput: (sid: string) => {
        this.userActive(sid);
        this.refreshOutline(tab); // 〔SE1〕真用户输入上屏 ⇒ 大纲要新的一截
      },
      observeForLazyEnhance: this.inBatch,
      // G4（branch-anywhere）：实时会话也挂「从这一轮分叉」按钮。
      // 钩子本来就在共享的 `render-stream-record.ts` 里，此前**只有历史查看器传了它**
      // ⇒ 实时 tab 上没有入口。按钮本体是共享组件（off-main 的呈现区分也在那里）。
      // **G6 起远端也挂**；〔`K-R88` 09-13〕本机那条也收 sid 之后，
      // 「本机拿不到 jsonl 路径」这道门对两条路都不再是门槛。
      onCardRendered: (el, msg) => {
        if (msg.type !== "user" && msg.type !== "assistant") return;
        if (!msg.uuid) return;
        attachBranchButton(el, {
          uuid: msg.uuid,
          sourceSessionId: tab.sessionId,
          origin: tab.origin,
          cwd: tab.cwd ?? undefined,
          onForked: (res) => void this.startForkedSession(tab, res),
        });
      },
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
      // 〔`设计/10` 骨架〕floor 之下、但已被物化过的那段（岛）里迟到的行 ⇒ 就地建卡，不收纳
      if (!render && tab.skeleton && !tab.skeleton.isPending(payload.seq)) render = true;
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
      // ★ F15：**帧末合批**，不是逐行整刷。
      // 这里是 live 路上每来一行都会走到的地方，而 `refreshTabBar` 是整条 bar 的重刷；
      // 徽标上的数字攒到帧末一次性更新，用户看到的结果一模一样。
      // ⚠ 只合批**这一处** —— 其余十几个 `refreshTabBar()` 调用点是用户动作触发的
      // （切 tab / 关 tab / 改名…），一帧最多一次，合批对它们没有收益，
      // 反而会把「点完立刻看到」变成「下一帧才看到」。
      this.scheduleTabBarRefresh();
    }
  }

  /**
   * 〔SE1〕大纲：一条 live 记录到了 —— 记一笔「又长了」（O(1)，不判是不是用户输入）。
   * 本 tab 是 active、非批期、而且**还一次都没要过**（首个 tab 建出来时路径可能还没到）⇒ 要一次。
   */
  private noteOutlineLine(tab: Tab): void {
    tab.outline.markStale();
    if (!tab.outline.everFetched && !this.inBatch && this.activeId === tab.sessionId) {
      void tab.outline.refresh();
    }
  }

  /**
   * 〔SE1〕大纲：向后端要新的一截（从上次的 `end` 接着要；在途就合并成一趟）。
   * 批期不要 —— 批结束时 active tab 统一要一次，后台 tab 切进来再要（`needsFetch`）。
   */
  private refreshOutline(tab: Tab): void {
    if (this.inBatch) return;
    void tab.outline.refresh();
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
  /**
   * 〔步 17·C · 2026-09-21〕新 tab 落位 = **先按树摆（`placeInTree`），再按盘上那份顺序摆**。
   *
   * 🔴 **这一层是那个 no-op 的第二半修法**：tab 是陆续到的，而 `loadOrder` 只跑一次
   *   ⇒ 只在 `loadOrder` 里应用一次，**后到的每一个 tab 都会落到末尾**，
   *   盘上给它留的那一格永远用不上（现打：会话到齐后顺序 == 到达序）。
   * ⚠ 包成两层而不是往 `placeInTree` 里塞一句：它有 3 个 `return` 出口，
   *   逐个补一句就是下一次「补漏了一个出口」。
   * ⚠ 这里**只动 `orderedIds`、不碰 DOM** —— 拖拽期间的重画抑制（★ 6d）由
   *   `refreshTabBar` 那道守卫管，与本函数无关（今天 `placeInTree` 也是这个形状）。
   */
  private placeInOrder(tab: Tab): void {
    this.placeInTree(tab);
    this.applySavedOrder();
  }

  private placeInTree(tab: Tab): void {
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
    // E73：`null` = 没说（旧 backend / 存量会话）= 视为可以。只有显式 `false` 才记账。
    attachable: boolean | null = null,
  ): void {
    if (attachable === false) this.notAttachableSids.add(sessionId);
    else this.notAttachableSids.delete(sessionId);
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
    return !this.notAttachableSids.has(sid);
  }

  /** Batch5-F19：启动 active 选择用（last-active 是否已有 tab）。 */
  hasTab(sessionId: string): boolean {
    return this.tabs.has(sessionId);
  }

  /**
   * Batch15-P2：活跃 tab 的仓信息（cwd + origin），供全景视图判断索引哪个本地仓。
   * 无活跃 tab / 活跃 tab 无 cwd → 返 null。origin!==null = 远端会话（代码在远端机，
   * 本地 code-picture 索引不到，全景侧据此显式提示不索引）。**additive getter，只读，不改既有逻辑。**
   */
  activeRepoInfo(): { cwd: string; origin: string | null } | null {
    const tab = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
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
    const tab = this.tabs.get(sid);
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
    this.sessionAccountsByS = new Map();
    for (const r of rows) {
      if (r.sessionId) this.sessionAccountsByS.set(r.sessionId, r);
    }
    this.accountEmailByName = emailByName;
    this.accountLastByS = lastAccountByS;
    this.accountReadyOrigins = readyOrigins;
    this.currentByOrigin = currentByOrigin;
    for (const [sid, refs] of this.tabButtons) {
      const tab = this.tabs.get(sid);
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
    if (!shouldShowAccountBadge(tab.origin, this.accountReadyOrigins)) return hide();
    const b = sessionBadge(
      sid,
      tab.origin,
      this.sessionAccountsByS,
      this.accountEmailByName,
      this.accountLastByS,
    );
    if (!b || !b.account) return hide(); // 未知账号（源③）→ 退 hover；顺带把 b.account 窄化为 string
    refs.acctBadge.textContent = "";
    refs.acctBadge.className = "tab-acct-badge";
    refs.acctBadge.appendChild(accountAvatarEl(b.account, { size: 14, ghost: b.source === "last" }));
    const current = tab.origin ? this.currentByOrigin.get(tab.origin) ?? null : null;
    const mismatch = detectAccountMismatch(b.account, current);
    refs.acctBadge.title = mismatch ? `${b.tooltip} · 与当前账号「${current}」不一致` : b.tooltip;
    refs.acctBadge.style.display = "";
  }

  snapshotSessions(): GridSessionSnapshot[] {
    const out: GridSessionSnapshot[] = [];
    for (const tab of this.tabs.values()) {
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
        account: this.sessionAccountsByS.get(tab.sessionId)?.account ?? null,
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
        s.origin ? this.currentByOrigin.get(s.origin) ?? null : null,
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
    const tab = this.tabs.get(sessionId);
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
    let tab = this.tabs.get(sessionId);
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

    // K-R45 乙：本 tab 的「我说过的 N 句」。界面是共用那一份，这里只给它两件宿主自己的事：
    // ① 怎么跳 —— 实时窗口没有 `uuidToIdx` / `UnrenderedRanges`，够得着的只有**已经建了卡的**
    //    那些（`revealCard` 找不到就什么都不做：这条流本来就贴在底部，再滚一次是无意义的动作）；
    // ② 跳空了怎么解释 —— 实时这一侧的成因与查看器**不是同一件事**：那边是「渲染时被剥成空卡」
    //    （永久），这边是「还收纳在 `TailWindow` 里没建卡」（**上翻补一批就好了**）。
    const inputsEl = document.createElement("div");
    inputsEl.className = "live-user-inputs";
    const inputsPanel = new UserInputPanel({
      // 〔`设计/10` 骨架 · 子步 4〕骨架接上之后，「还没加载」的那条先按 uuid→seq 物化出来再跳。
      jumpTo: (uuid) => {
        const sk = this.tabs.get(sessionId)?.skeleton;
        const seq = sk?.ledger.uuidToSeq.get(uuid);
        if (sk && seq !== undefined && sk.isPending(seq)) sk.ensure(seq);
        return revealCard(streamEl, uuid);
      },
      unjumpableHint: "这一条还没加载出来 —— 往上翻到更早的消息之后再点",
    });
    inputsEl.append(inputsPanel.toggle, inputsPanel.panel);
    this.streamRootEl.appendChild(inputsEl);
    // 〔SE1〕大纲的数据源：路径可能要等首条行回填（骨架 tab），所以每次要的时候现取
    const outline = new OutlineSource(inputsPanel, () => {
      const t = this.tabs.get(sessionId);
      return t?.parentPath ? { origin: t.origin ?? LOCAL_ORIGIN, jsonlPath: t.parentPath } : null;
    });
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
      activity: this.pendingActivity.get(sessionId) ?? null,
      tmuxIdle: false, // audit-fixes F03.2：默认非灰；pendingTmuxIdle 在下方落实
      agents: new Map(),
      touchedFiles: new Set(), // F70：会话改动集，onLine 增量累进
      latestPromptTokens: null, // F88b：HUD context% 数据；onLine 捕获带 usage 的 assistant 记录
      latestModel: null,
      latestUsageSeq: -1,
    };
    this.pendingActivity.delete(sessionId);
    // F40b:上翻补批触发器(passive 只读滚动位置;handler 内判 active,后台 tab
    // 的程序化滚动/尺寸变化不触发补批)
    const fillHandler = (): void => {
      if (this.activeId !== sessionId) return;
      const t = this.tabs.get(sessionId);
      // 〔`设计/10` 骨架〕接上了 ⇒ 占位可能在任何位置（拖滚动条到中部），**每次滚动**都看一眼
      // 视口里有没有占位 —— 不能沿用「离顶 800px 内才补」那道门（那是尾部窗口单洞后缀的假设）
      if (t?.skeleton) {
        t.skeleton.fillVisible();
        return;
      }
      if (t && t.streamEl.scrollTop <= TabManager.TOP_TRIGGER_PX) this.fillAbove(t);
    };
    streamEl.addEventListener("scroll", fillHandler, { passive: true });
    tab.fillHandler = fillHandler;
    // ★ 步 3：**视口自己变大 ⇒ 重新补批。**
    //
    // `fillHandler` 挂在 scroll 上，而**不可滚的元素根本不产生 scroll 事件** ——
    // 把窗口从半屏拉到全屏时，多出来的那块空白之前没有任何入口去填。
    // `MessageStream` 那边现在也观察 `scrollEl`（见 stream.ts），这里是它的消费端。
    // ⚠ 三道门都不可少：① 只给 active tab 补（后台 tab 0×0 → 真实尺寸那一跳不是
    // 「用户拉窗口」）；② 账本空了不补；③ 已经满屏了不补（否则每次 RO 都白干一轮）。
    stream.onViewportResize = (): void => {
      if (this.activeId !== sessionId) return;
      const t = this.tabs.get(sessionId);
      if (!t || t.window.pendingCount === 0) return;
      // 〔`设计/10` 骨架〕接上了 ⇒ 视口变大露出的是占位，只物化露出来的那段
      if (t.skeleton) {
        t.skeleton.fillVisible();
        return;
      }
      if (this.contentReachesBottom(t)) return;
      this.materializeUntilFilled(t);
      this.updateSentinel(t);
    };
    // issue #19：若该 sid 的归档信号先于本次建 Tab 到达（见 archiveTab），落实归档，
    // 避免重载后已结束会话复活成关不掉的 live Tab。本地 un-archive（上方 origin!==null
    // 那条）不适用，故归档后续 replay 行也不会把它复活。
    if (this.pendingArchive.delete(sessionId)) {
      tab.status = "archived";
      tab.activity = null; // 同 archiveTab：死会话不留陈旧灯/tooltip
      this.pendingTmuxIdle.delete(sessionId); // 归档优先：真 tmux 没了，灰灯作废
    } else if (this.pendingTmuxIdle.delete(sessionId)) {
      // audit-fixes F03.2：灰灯信号早于建 Tab（F5 重放乱序）→ 落实为 idle-tmux 灰点。
      tab.tmuxIdle = true;
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
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      // issue #19：Tab 还没被 ensureTab 建出来（归档信号早于 replay 行到达）——
      // 记下待归档，建 Tab 时落实。否则这里直接 return 会静默丢弃归档 → 僵尸 live Tab。
      this.pendingArchive.add(sessionId);
      this.pendingActivity.delete(sessionId); // issue #23：死会话的暂存灯一并清
      this.pendingTmuxIdle.delete(sessionId); // audit-fixes F03.2：归档优先，清暂存灰灯
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
    this.pendingArchive.delete(sessionId);
    this.pendingTmuxIdle.delete(sessionId); // audit-fixes F03.2：复活即清暂存灰灯
    const tab = this.tabs.get(sessionId);
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
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      this.pendingTmuxIdle.add(sessionId);
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
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      if (act) this.pendingActivity.set(sessionId, act);
      else this.pendingActivity.delete(sessionId);
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
    if (!this.inBatch && tab.sessionId === this.activeId) {
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
    // K-R45 乙：大纲跟着走（〔SE1〕`reset` 也让在途那趟回来后不许回写）。
    // 悬浮层是 `streamRootEl` 的直接子节点，不随 `streamEl.remove()` 一起走。
    tab.outline.reset();
    tab.inputsEl.remove();
    // 显式清 Map：释放对已卸载 DOM 节点的强引用，让 GC 可早回收
    // （Map 本身也会随 Tab 对象一起回收，但显式 clear 让 DOM 引用计数立即归零）
    tab.toolUseNames.clear();
    tab.toolUseElements.clear();
    tab.pendingToolResults.clear();
    tab.seenSeqs.clear();
    tab.processedUuids.clear();
    // F40a/b:窗口账本与缓冲持整段历史 payload(大会话数十 MB 级),断引用;摘 fill listener
    tab.window.dispose();
    tab.skeleton?.dispose(); // 〔`设计/10` 骨架〕
    tab.skeleton = null;
    tab.midBatchBuffer = [];
    if (tab.fillHandler) tab.streamEl.removeEventListener("scroll", tab.fillHandler);
    tab.timeline.dispose();
    tab.branchFolder.dispose();
    this.tasksBySid.delete(sessionId);
    this.tabs.delete(sessionId);
    if (idx >= 0) this.orderedIds.splice(idx, 1);
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
        this.activeId = null;
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
      if (this.bringMonitorToFront) bringMonitorToFront();
      return;
    }
    this.switchTo(sessionId, "auto");
    if (this.bringMonitorToFront) bringMonitorToFront();
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
    // 〔第二波 T4 · LF1〕非 Windows 上 ↗ 的最后一跳是桩（每点必败）⇒ 按钮不渲；
    //   快捷键 / 命令面板（住 `main.ts`）还够得到这里 ⇒ 说一句实话，不发 IPC。见 `terminal-front.ts`。
    if (!terminalFrontAvailable()) {
      showActionFailureToast(TERMINAL_FRONT_UNAVAILABLE_TITLE, TERMINAL_FRONT_UNAVAILABLE_DETAIL, {
        level: "info",
        durationMs: 6000,
      });
      return;
    }
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (!tab || tab.status === "archived") return;
    // Feature ②：远端 Tab → 后端唯一分派点（先启动令牌、后 ccm-rbind 标题退路）；本地 Tab → 原 sid_hwnd_cache 路径。
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

  /** F77：活跃 tab 的子 agent 加载上下文（parentPath + origin）——main.ts 点 agent 行时用它
   *  调 `load_subagent`。无活跃 tab / 无 parentPath → null；远端会话 origin!==null（不支持，调用方提示）。 */
  getActiveSubagentContext(): { parentPath: string; origin: string | null } | null {
    const tab = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (!tab || !tab.parentPath) return null;
    return { parentPath: tab.parentPath, origin: tab.origin };
  }

  /** issue #10 快捷键 Ctrl+Shift+N：把当前活跃 Tab 在独立只读窗口打开 */
  openActiveInNewWindow(): void {
    if (this.activeId) void this.openInNewWindow(this.activeId);
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
      ghost.textContent = this.tabs.get(d.sid)?.title ?? "";
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
          : (this.tabs.get(d.sid)?.title ?? "");
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
    const host = this.tabs.get(sid);
    if (!host) return [sid];
    const hostIsInteractive = host.kind === null || host.kind === "interactive";
    if (!hostIsInteractive) return [sid];
    const out = [sid];
    const at = this.orderedIds.indexOf(sid);
    for (let i = at + 1; i < this.orderedIds.length; i++) {
      const t = this.tabs.get(this.orderedIds[i]);
      if (!t) break;
      const isBg = t.kind !== null && t.kind !== "interactive";
      if (isBg && t.cwd !== null && t.cwd === host.cwd && t.origin === host.origin) {
        out.push(this.orderedIds[i]);
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
      const other = target.kind === "end" ? null : this.tabs.get(target.sid);
      const nextCols = applyDropToCollections(
        this.collections,
        block,
        target,
        defaultGroupName(
          this.tabs.get(sid)?.cwd ?? null,
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
    const next = moveTabBlock(this.orderedIds, block, beforeSid);
    if (next.length !== this.orderedIds.length) {
      this.refreshTabBar(); // 防御：块算错了就只重画（①可能已经改了归属）
      return;
    }
    if (next.every((x, i) => x === this.orderedIds[i])) {
      this.refreshTabBar();
      return;
    }
    this.orderedIds = next;
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
    this.savedOrder = [...this.orderedIds];
    try {
      await setTabOrder(this.orderedIds);
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
   * const saved = await getTabOrder(new Set(this.orderedIds));  // ← alive = 此刻的 tab 集
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
    this.savedOrder = await getTabOrder(null);
    if (this.applySavedOrder()) this.refreshTabBar();
  }

  /**
   * 把 `savedOrder`（盘上那份意图）应用到**此刻真的存在**的 tab 上。
   * 返回「顺序有没有真的变」—— 没变就别让调用方白重画一次栏。
   *
   * 🔴 **`present` 这道过滤是「不凭空造 tab」那条的落点**：盘上提到而今天不在的 sid
   *   （被删的会话 / **还没宣告到的会话**）在这里被跳过 —— 它**留在 `savedOrder` 里**，
   *   等它真的到了，`placeInOrder` 再应用一次就会把它放回自己那一格。
   *   ⚠ 这正是它不能在读的那一刻被摘掉的原因：摘掉就再也等不到了。
   * ⚠ **盘上没提到的排在后面**、且保持它们此刻的相对次序（`loadOrder` 头注逐字）。
   */
  private applySavedOrder(): boolean {
    if (this.savedOrder.length === 0) return false;
    const present = new Set(this.orderedIds);
    const head = this.savedOrder.filter((sid) => present.has(sid));
    if (head.length === 0) return false;
    const inHead = new Set(head);
    const next = [...head, ...this.orderedIds.filter((sid) => !inHead.has(sid))];
    // 恒等就早退。`next` 与 `orderedIds` 必然同长（两边都是 `orderedIds` 的重排），
    // 所以逐位比一遍就够。
    if (next.every((x, i) => x === this.orderedIds[i])) return false;
    this.orderedIds = next;
    return true;
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
    const tab = this.tabs.get(sid);
    if (refs && tab) this.updateAccountBadge(refs, sid, tab);
  }

  /** account-ux U8：当前活跃会话 sid（只读投影，供 Ctrl+K / 快捷键判定"对当前会话做某事"）。 */
  activeSessionId(): string | null {
    return this.activeId;
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
      // K-R45 乙：清单悬浮层与它那条流**同进同出**。漏掉这一句 = 所有 tab 的清单
      // 一起挂在屏幕上，而且点下去找的是别人的流（`revealCard` 只在自己的 streamEl 里找）。
      t.inputsEl.classList.toggle("active", sid === sessionId);
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
    // 〔`设计/10` 骨架〕切进来的 tab 要索引（上面刚物化过尾段 ⇒ floor 已钉）
    if (next) this.requestSkeleton(next);
    if (next?.outline.needsFetch) this.refreshOutline(next); // 〔SE1〕大纲：有新行才要
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
        if (this.activeId !== sessionId) return;
        this.tabs.get(sessionId)?.stream.scrollToBottom();
      });
      // issue #11: 切换 task panel 数据源到新 active Tab 的 sid
      this.tasksPanel?.setSession(sessionId, this.tasksBySid.get(sessionId) ?? []);
      // issue #23: agents 面板同步切到新 active Tab
      this.agentsPanel?.setSession(
        sessionId,
        [...(this.tabs.get(sessionId)?.agents.values() ?? [])],
      );
      // F88b：HUD context% chip 切到新 active 会话的最新 usage（无带 usage 记录 → null → 隐藏）
      const nt = this.tabs.get(sessionId);
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
   *   「`loadOrder` 用 `getTabOrder(new Set(this.orderedIds))` 按今天真的存在的 sid 过滤，
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
      const existed = this.tabs.get(p.sid);
      if (!existed) {
        this.createSkeletonTab(p.sid, p.cwd, p.origin, p.kind, p.name);
        const t = this.tabs.get(p.sid);
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
        this.sessionAccountsByS.get(tab.sessionId)?.account ??
        this.accountLastByS.get(tab.sessionId) ??
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
    for (const sid of this.orderedIds) {
      const tab = this.tabs.get(sid);
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
    const tab = this.tabs.get(sid);
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
    return (this.tabs.get(sid)?.parentPath ?? "") === "";
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
    const wanted = new Set(this.orderedIds);
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
    for (const sid of this.orderedIds) {
      const tab = this.tabs.get(sid);
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
      const t = this.tabs.get(sid);
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
      const t = this.tabs.get(sid);
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
    refs.root.classList.toggle("active", sid === this.activeId);
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
    this.updateAccountBadge(refs, sid, tab); // A3：账号徽章随 tab 更新一并刷新
  }
}

// P5.2 B 重构：markCardUuid + feedBranchFolder 已搬到 render-stream-record.ts
// （三 caller 共用 renderStreamRecord 函数内部调用）。tabs.ts 不再持有这两个 helper。
