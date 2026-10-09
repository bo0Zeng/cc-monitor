/**
 * **主窗口**的 bootstrap（入口模块是 `entry-main.ts`，由 `index.html` 加载）。
 * 〔三入口拆分〕独立 viewer 窗与设置窗各有自己的入口，不加载本文件。
 * DOMContentLoaded 后按序：
 * 1. `loadTheme()` 从 config.json 应用 CSS 变量
 * 2. 实例化 TabManager / SettingsPanel / HistoryView / TasksPanel
 * 3. `bindEvents()` 订阅通道那几条流：会话行（含起停那几格）· tap · 账号 · 任务
 * 4. 装全局快捷键 dispatcher（keybindings/）+ 外链 click 代理（openUrl）+ ERROR toast
 * 5. `emit("frontend-ready")` 通知后端 replay 历史
 *
 * 另持有启动 perf 测量（`window.__ccmPerf`，与后端 lib.rs 的 t0 互补看完整启动管线）。
 * HMR 走 full reload（不引框架，原生 DOM，强制整页重载简化心智模型）。
 */
// `LOCAL_ORIGIN`：本机那个 origin 的**唯一住址**（Rust 侧是
// `origin::LOCAL`，三处由 `origin_tests.rs::the_sentinel_agrees_with_the_two_existing_homes` 钉着）。
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { emit } from "@tauri-apps/api/event";
import { commands } from "./ipc/commands";
import { icon } from "./kit/icon";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { StartupActive } from "./startup-active";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { bindEvents } from "./events";
import { TabManager } from "./tabs";
import { livePainter } from "./live-card-view";
import { mountTabBarResizer } from "./tab-bar-width";
import { terminalFrontCommand } from "./terminal-front-command";
import { loadTheme } from "./theme";
import { SETTINGS_APPLIED_EVENT } from "./settings";
import { setResumeInTmux } from "./resume-defaults";
import { RESYNC_DONE_EVENT } from "./settings/events";
import {
  AGENT_WINDOW_EVENT,
  AGENT_WINDOWS_ASK_EVENT,
  SHOW_RUN_CARD_EVENT,
  SWITCH_TO_SESSION_EVENT,
  type AgentWindowSaid,
  type ShowRunCard,
  type SwitchToSession,
} from "./window-events";
import { listen } from "@tauri-apps/api/event";
import { HistoryView } from "./views/history";
import { CcBusView } from "./views/cc-bus-view";
import { GridMonitorView } from "./views/grid-monitor";
import { CommandBarView, type Command } from "./views/command-bar";
import { readContextLimits } from "./views/context-limit";
import { loadConfig } from "./config";
import { UsageHud } from "./usage-hud";
import { recordFileWiring } from "./record-file-notice";
import { toast, undoLatest } from "./kit/toast";
import { KeysOverview } from "./views/keys-overview";
import { StatusMessages } from "./status-messages";
import { StatusChores } from "./status-chores";
import { choresOf } from "./settings/data-reads";
import { readReadiness } from "./settings/readiness-reads";
import { StatusStart } from "./status-start";
import { restoreZoom, stepZoom } from "./zoom";
import { mountTabBarFold, tabBarManuallyFolded, toggleTabBarFold } from "./tab-bar-fold";
import { attachTooltip } from "./kit/tooltip";
import { bindErrorToast } from "./backend-errors";
import { bindRemoteHealthToast } from "./remote-health";
// 顶栏远端文件入口：按远端主机数 0 / 1 / N 分支（`sftp-host-picker.ts`）。
import { openSftpFromTopbar, toggleSftpFromTopbar } from "./sftp-host-picker";
import { readRemoteConfig } from "./remote-config";
import { createUnknownKeysBar } from "./settings/unknown-keys-notice";
import { openSettingsWindow } from "./settings/open-settings"; // 点「设置」有反馈（不 import 设置面板）
import * as dest from "./settings-dest";
import { collectAccountRows, createEventRefresher } from "./session-accounts-poll";
import { lastAccounts } from "./history-reads";
import { TasksPanel } from "./tasks-panel";
import { AgentsPanel } from "./agents-panel";
import { MainDrawer } from "./main-drawer";
import { TerminalPage } from "./terminal-page";
import { onMachineState } from "./machine-feed";
import type { Tab } from "./tab-model";
import { REVEAL_RUN_EVENT } from "./cards/speaker-bar";
import { getBehavior } from "./behavior";
import { flipBehavior } from "./behavior-toggle";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { installGlobalClickDelegation } from "./entry-render-common";
import { AccountChip } from "./account-chip";
import { onQuotaChanged, refreshRules, syncSessions } from "./acct-center";
import { followActive, openSourcePicker, toggleAccountPanel, type AcctPanelHost } from "./acct-panel";
import { acctSessionWiring } from "./acct-session";
import { buildAccountCommands } from "./account-commands";
import { machineName } from "./control-said";
import { sessionCommands } from "./session-commands";
import type { FrontendReadyPayload } from "./generated/FrontendReadyPayload";
import { currentAccountForBadge } from "./accounts";
import { fetchSessionAccounts, fetchAccounts } from "./account-reads";
import { bindLaunchArrivals, noteLive, setArrivalSlots } from "./launch-arrival";
import { FOCUS_SESSION_EVENT, openNewSession, setNewSessionPlaceholder } from "./new-session";
import { copyText } from "./copy-table";
import { appStore } from "./app-store";
import { OverlayRouter } from "./overlay-router";
import { SessionHead, terminalActsOf } from "./session-head";
import { NeedsBar, NeedsWatch } from "./needs-bar";
import { notifySend } from "./turn-notify";

// === 启动 perf 测量 ===
// performance.now() 自页面 navigation start 起；前端各阶段时间点。
// 跟后端 lib.rs 的 t0 (进程启动 Instant) 互补 —— 前后端协同看完整管线。
declare global {
  interface Window {
    __ccmPerf: {
      domContentLoaded: number;
      themeLoaded?: number;
      frontendReadyEmit?: number;
      firstJsonlBatch?: number;
      firstPayloadDrained?: number;
      batchDrainEnd?: number;
      onBatchEndFired?: number;
      /** content 记录真实建卡数（renderMessage 走到的次数） */
      recordsRendered?: number;
      /** 被尾部优先门控收纳（不建卡）的 content 记录数 */
      recordsDeferred?: number;
    };
  }
}
window.__ccmPerf = {
  domContentLoaded: 0,
};

// 只在 DEV 的 E2E 探针句柄（动态 import；生产恒 null，调用点全用可选链）
let e2eProbe: typeof import("./e2e-probe") | null = null;

// Vite HMR 默认在没显式 `hot.accept` 时对 TS 文件部分热替换，可能让旧模块的
// 已注册 listener / 已渲染 DOM 与新代码并存。对监控这种长跑+事件密集的应用，
// 部分热替换会造成视觉错乱、消息重复 listener、event_replay 状态不一致。
// 强制：任何 hot update 一律 full reload，保证 monitor 视图永远是单一一致版本。
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload();
  });
}

// 全局错误捕获（渲染到 status-bar）在 `entry-common.ts`，三个入口共用。

window.addEventListener("DOMContentLoaded", async () => {
  window.__ccmPerf.domContentLoaded = performance.now();
  mountTabBarFold(); // 栏收不收（窗宽 / 手动）先定，免得窄窗启动先闪一下宽栏
  restoreZoom();
  console.info(
    `[perf] DOMContentLoaded @ ${window.__ccmPerf.domContentLoaded.toFixed(0)}ms (since navigation start)`,
  );
  // 主题尽早应用，避免渲染抖动
  await loadTheme();
  window.__ccmPerf.themeLoaded = performance.now();

  // 只读窗口与设置窗口不经过这里：各有自己的 html 与入口（`viewer.html → entry-viewer.ts`、`settings.html → entry-settings.ts`）。

  const tabBar = document.getElementById("tab-bar");
  const streamRoot = document.getElementById("message-stream");
  const status = document.getElementById("status-bar");

  if (!tabBar || !streamRoot || !status) {
    console.error("layout containers missing");
    return;
  }

  // 状态栏：消息 · 空 · 这个会话的几枚（任务 · agent · 上下文 · 账号）· 竖线 · 命令。没内容的那枚不渲染。
  status.innerHTML = "";
  // 最左「消息」：本次运行里最近 20 条提示（toast 收进来的那几条也在这里找得回）。
  status.appendChild(new StatusMessages().el);
  // 「要你动手 N」（有才出）：各台「文件与数据」进角标的件数相加，与设置窗左栏同一个数。
  const chores = new StatusChores({
    machines: async () => {
      const got: unknown = await commands.backend_machines();
      return Array.isArray(got) ? got.filter((o): o is string => typeof o === "string") : [LOCAL_ORIGIN];
    },
    chores: choresOf,
    open: () => void openSettingsWindow(undefined, dest.CHORES),
  });
  status.appendChild(chores.el);
  void chores.refreshAll();
  // 「开始用 · 剩 N 步」（有才出，紧跟「要你动手」）：本机后端 `first-run` 的 `left`，与设置窗机器页「开始用」同一个数。
  const start = new StatusStart({ read: readReadiness, open: () => void openSettingsWindow(undefined, dest.START) });
  status.appendChild(start.el);
  void start.refresh();
  void getCurrentWindow()
    .onFocusChanged(({ payload: focused }) => {
      if (focused) {
        void chores.refreshAll();
        void start.refresh();
      }
    })
    .catch((e: unknown) => console.warn("[status-chores] 挂焦点监听失败：", e));
  const statusSpacer = document.createElement("span");
  statusSpacer.className = "status-sp";
  status.appendChild(statusSpacer);
  const statusDivider = document.createElement("span");
  statusDivider.className = "status-divider";
  statusDivider.setAttribute("aria-hidden", "true");

  // 这个会话的任务 · 子 agent：状态栏一枚 chip ＋ 底部抽屉里一页（抽屉在状态栏之上、主区底部，把消息流往上推）。
  const tasksPanel = new TasksPanel();
  status.appendChild(tasksPanel.summaryElement);
  const agentsPanel = new AgentsPanel();
  status.appendChild(agentsPanel.summaryElement);
  // 终端页跟着当前标签页走；`tabs` 在下面才建 ⇒ 先经一个口子取当前标签页，建好了再接上。
  let activeTab: () => Tab | null = () => null;
  const terminalPage = new TerminalPage({
    active: () => activeTab(),
    front: (sid) => tabs.frontFor(sid),
    focusStream: () => {
      const el = activeTab()?.streamEl;
      if (!el) return;
      if (el.tabIndex < 0) el.tabIndex = -1;
      el.focus({ preventScroll: true });
    },
  });
  // 那台重连回来（壳推「已连上」）⇒ 终端页停在「那台断开」的实时自己重新订上。
  onMachineState((origin, m) => {
    if (m?.state === "up") terminalPage.machineUp(origin);
  });
  const mainDrawer = new MainDrawer(tasksPanel, agentsPanel, terminalPage, () => (document.getElementById("app")?.clientHeight ?? window.innerHeight) - status.getBoundingClientRect().height);
  mainDrawer.dock.el.id = "bottom-drawer";
  document.getElementById("app")?.insertBefore(mainDrawer.dock.el, status);
  // 消息流里 agent 事件条的「打开窗口 ›」· 派出卡的卡头：开那个子运行自己的窗口（已开着 ⇒ 拉到前面）。
  document.addEventListener(REVEAL_RUN_EVENT, (e) => {
    const d = (e as CustomEvent<{ run?: unknown; tool?: unknown }>).detail;
    const sid = tabs.activeSessionId();
    if (sid === null) return;
    if (typeof d?.run === "string") void tabs.openRunWindow(sid, { run: d.run });
    else if (typeof d?.tool === "string") void tabs.openRunWindow(sid, { tool: d.tool });
  });

  // 「上下文」chip：当前会话最新一轮占上限多少（后端定上限）；点开看用量与上限来源。
  const usageHud = new UsageHud();
  status.appendChild(usageHud.summaryElement);

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.innerHTML = copyText("main.empty.noSessions");
  streamRoot.appendChild(empty);

  // 启动时记住的那一格（上次所在的 tab）：只在那个会话出现时恢复、就绪前不许被覆盖、没等到就明说（`startup-active.ts`）。
  let startup: StartupActive | null = null;

  // 会话头（主区顶上 40px）与「需要你」钉条（消息流底部）：只读当前 tab，做事经 `tabs` 那几条。
  const sessionHead = new SessionHead({
      active: () => tabs.activeTab(),
      viewTerminal: () => mainDrawer.dock.show("terminal"),
      openCwd: (sid) => tabs.openCwdOf(sid),
      front: (sid) => tabs.frontFor(sid),
      find: () => tabs.openFind(),
      more: (anchor, sid) => tabs.openMenuFor(anchor, sid),
      resume: (anchor, sid) => tabs.openResumeFor(anchor, sid),
      attach: (sid) => tabs.attachInTerminal(sid),
      reconnect: (origin) => tabs.reconnect(origin),
  });
  document.getElementById("session-head")?.replaceWith(sessionHead.el);
  const needsBar = new NeedsBar({
    active: () => tabs.activeTab(),
    front: (sid) => tabs.frontFor(sid),
    attach: (sid) => tabs.attachInTerminal(sid),
  });
  streamRoot.appendChild(needsBar.el);
  const needsWatch = new NeedsWatch({
    setTitle: (title) => {
      document.title = title;
      void getCurrentWindow().setTitle(title).catch((e: unknown) => console.warn("set title failed:", e));
    },
    isFocused: () => document.hasFocus(),
    enabled: async () => (await getBehavior()).notifyNeeds,
    send: notifySend,
    now: () => Date.now(),
  });

  const tabs = new TabManager(
    tabBar,
    streamRoot,
    ({ total }) => {
      // tab 集合变了 ⇒ 新出现的会话问一次它那台的轮换格（构造途中也会叫到这里 ⇒ 排到下一拍，`tabs` 已赋值）。
      queueMicrotask(() => syncSessions(tabs.snapshotSessions()));
      empty.style.display = total > 0 ? "none" : "";
      // 会话头 · 「需要你」钉条 · 窗口标题与系统通知：跟着标签页栏一起刷（构造途中也会叫到 ⇒ 排到下一拍）。
      queueMicrotask(() => {
        sessionHead.render();
        terminalPage.sessionChanged();
        paintTerminalActs();
        needsBar.render();
        needsWatch.observe(tabs.tabsInOrder());
      });
    },
    tasksPanel,
    agentsPanel,
  );
  usageHud.host = { openSettings: () => void openSettingsWindow(undefined, dest.CONTEXT_LIMITS), retry: () => tabs.retryActiveFacts() };
  // 活卡的画法（带 `.module.css`）只由主窗口入口装进来 —— 理由见 `live-card-view.ts` 头注。
  tabs.setLivePainter(livePainter);
  // 启动时拉一次标签页集合（住 `config.json`，不是 localStorage —— 见
  // `tab-collections.ts` 头注：集合名是用户手写的真相，必须活过一次清缓存）。
  void tabs.loadCollections();
  // 顺序也要拉回来。排在集合之后：两者都只改内存 + 重画，互不依赖，
  // 但集合决定分组容器、顺序决定容器内次序 —— 先有容器再排，少一次无谓重画。
  //
  // 固定表排在顺序之前、串着跑：次序不承重（顺序是一份留着的意图，tab 陆续到达时反复应用），只省一次重画；
  //   顺序能不能活过重启由 `tabs.vitest.ts`「顺序落盘：读回来那一半」按真实启动时序盯着。
  // `finally` 不是 `then`：`loadPinned` 失败也得让顺序照常回来。
  void tabs.loadPinned().finally(() => void tabs.loadOrder());

  // 状态栏「当前账号」chip：绑第一台可用远端的默认账号；未连远端 / 未启用多账号安静降级。点击弹选单切默认账号。
  // 构造在 `refreshSessionAccounts`（下方 `const`）之前：`onDefaultChanged` 回调间接调它，只在用户切号时才执行，
  //   那时它早已初始化 —— 别在这中间插一个会触发回调的 await（会踩 TDZ）。
  // 「账号」面板（右侧抽屉）：状态栏按钮 · 右键「账号…」· 会话头 ⋯ 开同一个。
  const panelHost: AcctPanelHost = {
    sessionTitle: (sid) => tabs.snapshotSessions().find((x) => x.sessionId === sid)?.title ?? sid.slice(0, 8),
    cwdOf: (sid) => tabs.snapshotSessions().find((x) => x.sessionId === sid)?.cwd ?? "",
    agentOf: (sid) => tabs.agentOf(sid),
    openSettings: (origin) => void openSettingsWindow(undefined, dest.accountsOf(origin)),
    openRules: (origin, rule) => void openSettingsWindow(undefined, dest.rulesOf(origin, rule)),
    openDefaultMenu: (anchor, origin) => void accountChip.openDefaultMenu(anchor, origin),
    defaultOf: (origin) => accountChip.defaultOf(origin),
    // 恢复菜单挂在状态栏上（抽屉的底边）：toast 的按钮点了就收起，没有自己的锚。
    openResume: (sid) => {
      const anchor = document.getElementById("status-bar");
      if (anchor) tabs.openResumeFor(anchor, sid);
    },
  };
  const openAcctPanel = (sid: string, origin: string): void => toggleAccountPanel(sid, origin, panelHost);
  tabs.onOpenAccountPanel = openAcctPanel;
  // 多选右键「轮换规则 ▸」：那台的规则表（还没读过 ⇒ 去读，这一回先转圈）· 管理 ⇒ 设置窗那台的「轮换」栏。
  tabs.onRotationRules = {
    rulesOf: (origin) => {
      const got = appStore.rotationRules.get().get(origin);
      if (got === undefined) void refreshRules(origin);
      return got ?? null;
    },
    openRules: (origin) => void openSettingsWindow(undefined, dest.rulesOf(origin)),
  };
  // ↗ 浮层的［接上终端］直达设置那一节（［更新］就地做，住 `tabs.ts`）。
  tabs.onConnectTerminal = () => void openSettingsWindow(undefined, dest.connectTerminalOf(LOCAL_ORIGIN));
  tabs.onOpenHere = (sid) => {
    tabs.switchTo(sid);
    mainDrawer.open("terminal");
  };
  // 起新会话：在主窗口里起的先长出占位标签页、报到了换成真的；在设置 / 查看窗里起的点了［切过去］再切，先把主窗口拉到前面。
  setNewSessionPlaceholder((spec) => tabs.addLaunchSlot(spec));
  // 别的起会话路（恢复 · cc-bus 派生 · 开窗 resume）在主窗口里发起的也走同一件占位标签页。
  setArrivalSlots((spec) => tabs.addLaunchSlot(spec));
  // 占位标签页那一页显着：会话头与真标签页的选中样子让开。
  tabs.onSlotShown = (on) => {
    sessionHead.el.toggleAttribute("data-slot-over", on);
    tabBar.toggleAttribute("data-slot-over", on);
  };
  void listen<{ origin: string; sid: string }>(FOCUS_SESSION_EVENT, (e) => {
    const w = getCurrentWindow();
    void w.unminimize().then(() => w.setFocus()).catch(() => {});
    tabs.switchTo(e.payload.sid);
  });
  tabs.onViewTerminal = () => mainDrawer.dock.show("terminal");
  const activeOrigin = (): Origin | null => {
    const sid = tabs.activeSessionId();
    return sid === null ? null : tabs.originOf(sid);
  };
  const accountChipDeps = {
    openSettings: () => void openSettingsWindow(undefined, dest.accountsOf(activeOrigin() ?? LOCAL_ORIGIN)),
    togglePanel: openAcctPanel,
    // 切号后立刻重算一次：currentByOrigin 只由下面那个事件驱动的刷新器喂，不主动刷的话
    // chip 已显示新账号，而对齐动作会把会话打回**刚被切走**的旧账号（D 审计重-5）。
    onDefaultChanged: () => accountsRefresher.request(true),
  };
  const accountChip = new AccountChip(accountChipDeps);
  status.append(statusDivider, accountChip.element); // 还没有当前会话：账号排在竖线后（有会话时挪到前面那一组）
  void accountChip.refresh();

  // 账号徽章数据管道：对每台远端拉 session-accounts（哪条会话属于哪个号）＋ 账号邮箱，聚合喂 tabs。走 accounts store 的 8s TTL 缓存 ＋ available:false 降级。
  // 序号门：每次进入 ++refreshSeq，写快照前若已被更晚一次推大 ⇒ 丢弃本次（慢的旧快照不覆盖新快照）。
  let refreshSeq = 0;
  const refreshSessionAccounts = async (forceAccounts: boolean): Promise<void> => {
    const mySeq = ++refreshSeq;
    try {
      const cfg = await readRemoteConfig();
      if (mySeq !== refreshSeq) return; // 有更晚的刷新已开始 → 本次作废，别覆盖它
      if (!cfg.hosts.some((h) => h.connect)) {
        // 账号快照整份换进 store，tab 栏徽章订阅它（同一拍应用）。
        appStore.sessionAccounts.set({
          rows: [],
          emailByName: new Map(),
          lastByS: new Map(),
          readyOrigins: new Set(),
          currentByOrigin: new Map(),
        });
        return;
      }
      // 各台并行问（`collectAccountRows`：并发上限 4、保序；判据在 `session-accounts-poll.vitest.ts`）。
      const { rows, emailByName, readyOrigins, currentByOrigin, lastByS } = await collectAccountRows(
        cfg.hosts,
        { fetchSessionAccounts, fetchAccounts, currentAccountForBadge, lastAccounts },
        undefined,
        forceAccounts,
      );
      if (mySeq !== refreshSeq) return; // 晚到的旧快照不覆盖新快照
      appStore.sessionAccounts.set({ rows, emailByName, lastByS, readyOrigins, currentByOrigin });
    } catch (e) {
      console.warn("refreshSessionAccounts failed:", e);
    }
  };
  // 刷新由事件驱动、零定时器（「会话 ↔ 账号」只在会话起停时变；理由在 `session-accounts-poll.ts` 头注）：
  //   · 某台的长连接握手完成（启动 / 重连）或那台账号清单变了 ⇒ 强制刷账号清单，
  //     账号 chip 也在这一刻重取（在那之前问只会拿到「没有控制通道」）—— 经通道订的 `accounts-changed`；
  //   · 远端 `live` 格 / `ended` 格：会话起停；
  //   · 本 UI 切号：上面 `onDefaultChanged`。
  const accountsRefresher = createEventRefresher(refreshSessionAccounts);
  accountsRefresher.request();
  // 「某台长连接握手完成 / 那台账号清单变了」⇒ 强制刷账号清单 ＋ chip：经通道订每台的 `accounts-changed`（下面 `bindEvents` 的 `accounts` ＋ `onAccountsChanged`）。
  const onAccountsChanged = (): void => {
    accountsRefresher.request(true);
    void accountChip.refresh(true);
  };
  // 会话起停：下面 `bindEvents` 的 `onRemoteSessionAdded` / `onSessionEnded` 里各请一次。

  // 用户手动切过 tab 后，迟到的宣告不补切抢焦点（用户的选择优先，记下此刻所在那一格）
  tabs.onManualSwitch = () => {
    startup?.onManualSwitch();
  };
  // 当前 tab 那一格变了 → 刷新 HUD context% chip（会话事实到了 / 切会话 / 要不到 ⇒ chip 说原因）。订阅 store。
  // 设置里的上下文上限表：随会话事实交给各台后端（上限在那里定，状态栏与监控板读同一个数）。读不到 ⇒ 空表。
  const loadContextLimits = async (): Promise<void> => {
    try {
      tabs.setContextLimits(readContextLimits((await loadConfig())["contextLimits"]));
    } catch {
      /* 读不到设置 ⇒ 后端只按它自己看得到的定 */
    }
  };
  void loadContextLimits();
  activeTab = () => tabs.activeTab();
  // 报错卡上去它的终端那两颗：当前会话的消息流根上标「能不能」（与会话头同一道），点了交 `tabs`。
  function paintTerminalActs(): void {
    const t = tabs.activeTab();
    if (!t) return;
    const acts = terminalActsOf(t);
    t.streamEl.dataset.canFront = acts.front ? "1" : "0";
    t.streamEl.dataset.canAttach = acts.attach ? "1" : "0";
  }
  streamRoot.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement | null)?.closest?.<HTMLElement>(".api-error-acts [data-act]");
    const sid = tabs.activeSessionId();
    if (!b || sid === null) return;
    if (b.dataset.act === "front") tabs.frontFor(sid);
    else if (b.dataset.act === "attach") tabs.attachInTerminal(sid);
  });
  tabs.active.subscribe((a) => {
    // 切了 tab ⇒ 会话头 · 「需要你」钉条 · 抽屉的终端页换成这一个。
    sessionHead.render();
    terminalPage.sessionChanged();
    paintTerminalActs();
    needsBar.render();
    usageHud.setActive(a.model, a.promptTokens, a.contextLimit, a.limitFrom);
    usageHud.setUnavailable(a.unavailable);
    // 状态栏账号按钮 = 本会话（排在竖线前那一组）；没有会话 ⇒ `默认 work`，排到竖线后。面板开着就跟着换成那个会话（不关）。
    const origin = a.sid === null ? null : tabs.originOf(a.sid);
    const cur = a.sid !== null && origin !== null ? { sid: a.sid, origin } : null;
    accountChip.setActive(cur);
    if (cur !== null) statusDivider.before(accountChip.element);
    else statusDivider.after(accountChip.element);
    followActive(cur, panelHost);
    if (a.sid !== null) acctSession.repaintBanner(a.sid); // 「还有多久」只在画的那一刻算：切到这个 tab 那一刻重算
  });
  // 会话里的换号条 · 提示条；tab 栏被卡住的会话 `✕ 5h`。
  const acctSession = acctSessionWiring({
    streamContentOf: (sid) => tabs.streamContentOf(sid),
    openPanel: openAcctPanel,
  });
  appStore.sessionRotation.subscribe(() => tabs.repaintTabBar());

  // 拉一次 behavior toggle 初值喂给 TabManager。
  // 设置面板改了之后会再调 applyBehavior 同步。
  void getBehavior().then((b) => {
    tabs.applyBehavior(b);
    setResumeInTmux(b.resumeInTmux);
  });

  // 设置在独立窗口里：那边保存 / 改行为后广播 SETTINGS_APPLIED_EVENT → 这里重读并应用主题 ＋ 行为（跨窗口回调够不到）。
  // 状态栏命令 chip 的键位刷新钩子：改键热应用后刷新 chip 上的键位（chip 创建时把重建函数赋进来）。
  let refreshCmdkChord: () => void = () => {};
  // 设置窗「重新对齐」做完 ⇒ 标出那台上记录没了的固定条（不自动摘）。
  void listen<{ origin: string }>(RESYNC_DONE_EVENT, (e) => void tabs.flagPinsWithoutRecord(e.payload.origin));
  // 任何窗口起会话之后交过来的「等它」都在这里收（主窗口订着每台的会话流）。
  bindLaunchArrivals();
  // 独立查看窗里点［切过去］· agent 窗口路径上点会话那一段 ⇒ 主窗口拉到前面、切到那个会话的标签页。
  void listen<SwitchToSession>(SWITCH_TO_SESSION_EVENT, (e) => {
    tabs.switchTo(e.payload.sessionId);
    const w = getCurrentWindow();
    void w.unminimize().then(() => w.setFocus()).catch(() => {});
  });
  // agent 窗口开了 / 关了 ⇒ 面板那一行与派出卡标「窗口已开」；主窗口（重新载入之后）先问一次哪些开着。
  void listen<AgentWindowSaid>(AGENT_WINDOW_EVENT, (e) => tabs.setRunWindow(e.payload.sessionId, e.payload.run, e.payload.open)).then(() =>
    emit(AGENT_WINDOWS_ASK_EVENT, {}).catch(() => {}),
  );
  // agent 窗口里「回到派出它的地方」（主会话派的）⇒ 主窗口拉到前面、切到那个会话、滚到那张派出卡。
  void listen<ShowRunCard>(SHOW_RUN_CARD_EVENT, (e) => {
    if (e.payload.in !== null) return;
    tabs.showRunCard(e.payload.sessionId, e.payload.tool);
    const w = getCurrentWindow();
    void w.unminimize().then(() => w.setFocus()).catch(() => {});
  });
  void listen(SETTINGS_APPLIED_EVENT, () => {
    void loadTheme(); // 主题：loadTheme 内部 applyTheme
    void getBehavior().then((b) => {
      tabs.applyBehavior(b); // 行为
      setResumeInTmux(b.resumeInTmux); // 恢复默认的「运行于」
    });
    void getKeybindings().then((kb) => {
      dispatcher.applyOverrides(kb); // 键位：热应用主窗口 dispatcher
      refreshCmdkChord(); // 键位变 → 同步刷新命令 chip 的 kbd（兑现「改键即变」）
    });
    void accountChip.refresh(true); // 远端配置 / 默认账号可能变了，刷新账号 chip
    void loadContextLimits(); // 上下文上限表可能在设置页改过
  });
  // 竖直 tab 栏右缘拖拽调宽（`tab-bar-width.ts`，宽度经 `LS_KEYS` ＋ `safeGet/safeSet` 记忆）。
  mountTabBarResizer();

  // 栏顶一排全局入口（会话总览 · 文件 · 历史 · 设置）：图标按钮 ＋ 悬停「名字 · 当前键位」，摆在标签页栏顶。
  const headButton = (cls: string, name: Parameters<typeof icon>[0], label: string, hint: () => string, onClick: () => void): HTMLButtonElement => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `tab-bar-icon ${cls}`;
    b.setAttribute("aria-label", label);
    b.appendChild(icon(name));
    attachTooltip(b, hint);
    b.addEventListener("click", onClick);
    return b;
  };
  const hintWithKey = (name: string, action: Parameters<typeof dispatcher.effectiveChord>[0] | null): (() => string) => () => {
    const chord = action === null ? null : dispatcher.effectiveChord(action);
    return chord ? `${name} · ${KeybindingDispatcher.prettyChord(chord)}` : name;
  };
  const settingsTrigger = headButton("settings-trigger", "settings", copyText("main.cmd.openSettings"), hintWithKey(copyText("main.topbar.settingsHint"), "app.open-settings"), () => {
    void openSettingsWindow(settingsTrigger); // 开独立设置窗口；点了有反馈
  });

  // 历史浏览器入口（HistoryView 自挂 body，fixed overlay）
  // 记录文件不见了 / 被改过 ⇒ 那个 tab 顶上说一句（只在主窗口接，理由见 `record-file-notice.ts`）。
  const recordFile = recordFileWiring((sid) => tabs.streamElOf(sid));
  const historyView = new HistoryView();
  // overlay 开关只经路由（顶栏按钮 · 快捷键 · 命令面板同一口）。
  const overlays = new OverlayRouter();
  overlays.register("history", historyView);
  // 删会话前看活不活：条目自己那一格可能是列表拉下来那一刻的，tab 栏是此刻的。
  historyView.liveInTabs = (sid) => tabs.isSessionLive(sid);
  // 行上的「等批准」徽标看主窗口此刻的状态；「切过去」切到主窗口那个标签页。
  historyView.needsOf = (sid) => tabs.needsWordOf(sid);
  historyView.switchTo = (sid) => tabs.switchTo(sid);
  const historyTrigger = headButton("history-trigger", "history", copyText("main.cmd.openHistory"), hintWithKey(copyText("main.topbar.historyHint"), "app.toggle-history"), () => overlays.toggle("history"));

  // 多 agent 并排监控入口：跨机器只读状态板（一屏看所有会话实时状态，点卡片跳会话；不派发、不驱动 agent）。
  const gridMonitorView = new GridMonitorView(tabs);
  overlays.register("grid", gridMonitorView);
  const gridTrigger = headButton("grid-monitor-trigger", "grid", copyText("main.cmd.openGrid"), hintWithKey(copyText("main.topbar.gridHint"), null), () => overlays.toggle("grid"));

  // 远端文件入口：按远端主机数分支 —— 0 台提示 / 1 台直开 / 多台选单（openSftpFromTopbar）；终点是原生文件窗口（`file-window.ts`）。
  const sftpTrigger = headButton("sftp-trigger", "files", copyText("main.cmd.openFiles"), hintWithKey(copyText("main.topbar.filesHint"), null), () => void toggleSftpFromTopbar(sftpTrigger));
  tabs.mountHeadActions([gridTrigger, sftpTrigger, historyTrigger, settingsTrigger]);

  // cc-bus 驾驶舱是顶层运营视图，入口在命令面板里、不另占顶栏图标（低频视图；理由见 `views/cc-bus-view.ts` 头注）。
  const ccBusView = new CcBusView();
  overlays.register("cc-bus", ccBusView);

  // 快捷键一览（`?` · 命令面板）：「改快捷键…」直达设置里快捷键那一节。
  const keysOverview = new KeysOverview({ editKeys: () => void openSettingsWindow(undefined, dest.KEYBINDINGS) });
  const toggleFullscreen = (): void => {
    const w = getCurrentWindow();
    void w
      .isFullscreen()
      .then((f) => w.setFullscreen(!f))
      .catch((e) => console.warn("toggle-fullscreen failed:", e));
  };

  // 命令面板里分组那三条（作用于选中的 / 当前标签页）：新建分组 · 加入分组…（有组才列）· 移出分组（在组里才列）。
  const groupCommands = (): Command[] => {
    const g = tabs.groupCommandState();
    if (g === null) return [];
    const kw = copyText("main.cmd.groupKeywords");
    const out: Command[] = [{ id: "group-new", group: "current", icon: "list", title: copyText("main.cmd.groupNew"), keywords: kw, run: () => tabs.foundGroupFromCommand() }];
    if (g.groups > 0) out.push({ id: "group-join", group: "current", icon: "list", title: copyText("main.cmd.groupJoin"), keywords: kw, run: () => tabs.openGroupMenuFromCommand() });
    if (g.grouped > 0) out.push({ id: "group-leave", group: "current", icon: "list", title: copyText("main.cmd.groupLeave"), keywords: kw, run: () => tabs.leaveGroupFromCommand() });
    return out;
  };

  // 命令面板（Ctrl+K）：打开那一刻现拼一份（分组 · 键位 · 可不可用由这里给，面板只排版）。
  const buildCommands = (): Command[] => {
    const chordHint = (id: Parameters<typeof dispatcher.effectiveChord>[0]): string | undefined => {
      const raw = dispatcher.effectiveChord(id);
      return raw ? KeybindingDispatcher.prettyChord(raw) : undefined;
    };
    const cur = tabs.activeSessionId();
    const curOrigin = cur === null ? null : tabs.originOf(cur);
    const cmds: Command[] = [];
    // 会话（空输入时只列在等你的那几行，在「需要你」一组）。
    cmds.push(...sessionCommands(tabs.tabsInOrder(), (sid) => tabs.switchTo(sid), (n) => chordHint(`tab.jump-${n}` as Parameters<typeof chordHint>[0])));
    if (cur !== null && curOrigin !== null) {
      cmds.push(
        { id: "find", group: "current", icon: "search", title: copyText("main.cmd.find"), keywords: copyText("main.cmd.findKeywords"), hint: chordHint("session.find"), run: () => tabs.openFind() },
        { id: "toggle-tasks", group: "current", icon: "tasks", title: copyText("main.cmd.tasks"), keywords: copyText("main.cmd.tasksKeywords"), hint: chordHint("panel.toggle-tasks"), run: () => mainDrawer.toggle("tasks") },
        { id: "toggle-agents", group: "current", icon: "agent", title: copyText("main.cmd.agents"), keywords: copyText("main.cmd.agentsKeywords"), hint: chordHint("panel.toggle-agents"), run: () => mainDrawer.toggle("agents") },
        { id: "toggle-terminal", group: "current", icon: "terminal", title: copyText("main.cmd.terminal"), keywords: copyText("main.cmd.terminalKeywords"), hint: chordHint("panel.toggle-terminal"), run: () => mainDrawer.toggle("terminal") },
        { id: "pop-out", group: "current", icon: "popOut", title: copyText("main.cmd.popOut"), keywords: copyText("main.cmd.popOutKeywords"), hint: chordHint("tab.pop-out"), run: () => tabs.openActiveInNewWindow() },
        { id: "open-cwd", group: "current", icon: "folder", title: copyText("main.cmd.openCwd"), keywords: copyText("main.cmd.openCwdKeywords"), hint: chordHint("tab.open-cwd"), run: () => tabs.openActiveTabCwd() },
        // ↗：只有 cc-monitor 跑在 Windows 上才能用；别的系统上灰着（门与 tab 上那颗按钮是同一道，见 `terminal-front-command.ts`）。
        ...terminalFrontCommand({ id: "term-front", group: "current" as const, icon: "front" as const, title: copyText("main.cmd.terminalFront"), keywords: copyText("main.cmd.terminalFrontKeywords"), hint: chordHint("terminal.bring-front"), run: () => tabs.bringActiveTerminalToFront() }),
        tabs.isPinned(cur)
          ? { id: "unpin", group: "current", icon: "pin", title: copyText("main.cmd.unpin"), keywords: copyText("main.cmd.pinKeywords"), run: () => tabs.togglePin(cur) }
          : { id: "pin", group: "current", icon: "pin", title: copyText("main.cmd.pin"), keywords: copyText("main.cmd.pinKeywords"), run: () => tabs.togglePin(cur) },
        ...groupCommands(),
        { id: "proc-expand", group: "current", icon: "expand", title: copyText("main.cmd.procExpand"), keywords: copyText("main.cmd.procExpandKeywords"), hint: chordHint("session.toggle-process"), run: () => tabs.toggleProcessDefault() },
        { id: "acct-panel", group: "current", icon: "account", title: copyText("acct.menu.open"), keywords: copyText("acct.menu.open"), run: () => openAcctPanel(cur, curOrigin) },
      );
    }
    cmds.push(
      {
        id: "new-session",
        group: "open",
        icon: "plus",
        title: copyText("main.cmd.newSession"),
        keywords: copyText("main.cmd.newSessionKeywords"),
        run: () => {
          const t = tabs.activeTab();
          void openNewSession(t ? { origin: t.origin, cwd: t.projectDir ?? undefined } : {});
        },
      },
      { id: "open-history", group: "open", icon: "history", title: copyText("main.cmd.openHistory"), keywords: copyText("main.cmd.historyKeywords"), hint: chordHint("app.toggle-history"), run: () => overlays.open("history") },
      { id: "open-grid", group: "open", icon: "grid", title: copyText("main.cmd.openGrid"), keywords: copyText("main.cmd.gridKeywords"), run: () => overlays.open("grid") },
      { id: "open-cc-bus", group: "open", icon: "bus", title: copyText("main.cmd.openCcBus"), keywords: copyText("main.cmd.ccBusKeywords"), hint: chordHint("app.open-cc-bus"), run: () => overlays.open("cc-bus") },
      { id: "open-settings", group: "open", icon: "settings", title: copyText("main.cmd.openSettings"), keywords: copyText("main.cmd.settingsKeywords"), hint: chordHint("app.open-settings"), run: () => void openSettingsWindow() },
      { id: "open-sftp", group: "open", icon: "files", title: copyText("main.cmd.openFiles"), keywords: copyText("main.cmd.filesKeywords"), run: () => void openSftpFromTopbar(sftpTrigger) },
      { id: "keys", group: "open", icon: "keyboard", title: copyText("main.cmd.keys"), keywords: copyText("main.cmd.keysKeywords"), hint: chordHint("app.keys"), run: () => keysOverview.open() },
      { id: "refresh", group: "open", icon: "refresh", title: copyText("main.cmd.refresh"), keywords: copyText("main.cmd.refreshKeywords"), run: () => tabs.refreshAll() },
      { id: "win-fullscreen", group: "window", icon: "fullscreen", title: copyText("main.cmd.fullscreen"), keywords: copyText("main.cmd.fullscreenKeywords"), hint: chordHint("app.toggle-fullscreen"), run: toggleFullscreen },
      { id: "win-minimize", group: "window", icon: "minimize", title: copyText("main.cmd.minimize"), keywords: copyText("main.cmd.minimizeKeywords"), hint: chordHint("app.minimize"), run: () => void getCurrentWindow().minimize() },
      tabBarManuallyFolded()
        ? { id: "tab-bar-fold", group: "window", icon: "sidebar", title: copyText("main.cmd.expandTabBar"), keywords: copyText("main.cmd.tabBarKeywords"), hint: chordHint("app.toggle-tab-bar"), run: toggleTabBarFold }
        : { id: "tab-bar-fold", group: "window", icon: "sidebar", title: copyText("main.cmd.collapseTabBar"), keywords: copyText("main.cmd.tabBarKeywords"), hint: chordHint("app.toggle-tab-bar"), run: toggleTabBarFold },
      { id: "zoom-in", group: "window", icon: "zoomIn", title: copyText("main.cmd.zoomIn"), keywords: copyText("main.cmd.zoomKeywords"), hint: chordHint("app.zoom-in"), run: () => stepZoom(1) },
      { id: "zoom-out", group: "window", icon: "zoomOut", title: copyText("main.cmd.zoomOut"), keywords: copyText("main.cmd.zoomKeywords"), hint: chordHint("app.zoom-out"), run: () => stepZoom(-1) },
      { id: "zoom-reset", group: "window", icon: "reset", title: copyText("main.cmd.zoomReset"), keywords: copyText("main.cmd.zoomKeywords"), hint: chordHint("app.zoom-reset"), run: () => stepZoom(0) },
    );
    // 账号：每个号一条「设 X 为默认账号」· 管理账号…（构造在 account-commands.ts，纯函数）· 新会话默认…（同状态栏无会话时那个下拉）。
    cmds.push(
      ...buildAccountCommands({
        snapshot: accountChip.snapshotReady(),
        chordHint: (id) => chordHint(id as Parameters<typeof chordHint>[0]),
        setCurrent: (name) => void accountChip.applyDefaultByName(name),
        openSettings: () => void openSettingsWindow(undefined, dest.accountsOf(curOrigin ?? LOCAL_ORIGIN)),
        rotation:
          cur !== null && curOrigin !== null
            ? {
                machine: machineName(curOrigin),
                apply: () => openSourcePicker(cur, curOrigin, panelHost),
                openRules: () => void openSettingsWindow(undefined, dest.rulesOf(curOrigin)),
              }
            : null,
      }).map((c) => ({ ...c, group: "account" as const, icon: "account" as const })),
      {
        id: "acct-default-menu",
        group: "account",
        icon: "account",
        title: copyText("acct.cmd.default"),
        keywords: copyText("acct.cmd.default"),
        run: () => void accountChip.openDefaultMenu(accountChip.element, curOrigin ?? LOCAL_ORIGIN),
      },
    );
    return cmds;
  };
  const commandBar = new CommandBarView(buildCommands);

  // 命令栏的可见入口：状态栏最右一个常驻 chip，图标 ＋「命令」＋ 真实键位（跟随 effectiveChord，改键即变 —— 见 refreshCmdkChord）；
  //   点它 = commandBar.toggle。
  {
    const cmdkHint = document.createElement("button");
    cmdkHint.type = "button";
    cmdkHint.className = "status-cmdk";
    cmdkHint.appendChild(icon("command", "compact"));
    const label = document.createElement("span");
    label.textContent = copyText("main.cmdk.label");
    cmdkHint.appendChild(label);
    // kbd 可重建：改键热应用后经 refreshCmdkChord 刷新。
    refreshCmdkChord = (): void => {
      cmdkHint.querySelector("kbd")?.remove();
      const chord = dispatcher.effectiveChord("app.open-command-bar");
      if (chord) {
        const kbd = document.createElement("kbd");
        kbd.textContent = KeybindingDispatcher.prettyChord(chord);
        cmdkHint.appendChild(kbd); // 未绑键（chord=null）则不显 kbd，chip 仍作点击入口
      }
    };
    refreshCmdkChord();
    cmdkHint.title = copyText("main.cmdk.hint");
    cmdkHint.addEventListener("click", () => {
      cmdkHint.classList.remove("first-run"); // 点过即消掉首运行高亮
      commandBar.toggle();
    });
    status.appendChild(cmdkHint); // 竖线之后：全局那几枚
    // 首次运行一次性微高亮（非模态），让新用户注意到这个角落入口；见过就不再亮。
    if (!safeGet(LS_KEYS.cmdkHintSeen)) {
      cmdkHint.classList.add("first-run");
      safeSet(LS_KEYS.cmdkHintSeen, "1");
    }
  }

  // ⚠ 只在主窗口挂：独立 viewer 窗与设置窗各走自己的入口（`entry-viewer.ts` / `entry-settings.ts`），不经过本文件。
  // 「配置里有个键没生效」在主窗口也要说得出来：没打开过设置的人看不到设置面板顶上那一条 ⇒ 状态栏一条非模态的指路。
  //   同一个组件两个挂点，那句话只住 `unknown-keys-notice.ts`；`createUnknownKeysBar()` 先用快照渲、再拿盘上那份重渲。
  //   这一挂点只由源码扫描盯着（main.ts 没有 DOM 判据台架），「挂没挂上」判不了。
  status.appendChild(createUnknownKeysBar());


  // 外链 + 代码块复制的全局 click 代理（主窗口 / 独立 viewer 窗口共用）
  installGlobalClickDelegation();

  // 快捷键走 KeybindingDispatcher 统一派发。
  // 各 action 默认 chord 见 keybindings/actions.ts；用户覆盖存 config.json
  // `keybindings` 字段。Esc 关弹层由 dispatcher 的 overlay stack 管理（settings /
  // history / tasks-panel 各自 push/pop）。
  dispatcher.bind("tab.next", () => tabs.cycleActive(1));
  dispatcher.bind("tab.prev", () => tabs.cycleActive(-1));
  for (let i = 1; i <= 9; i++) {
    dispatcher.bind(`tab.jump-${i}` as const, () => tabs.jumpToIndex(i));
  }
  // 作用于当前会话的那几键：占位标签页那一页显着时让开（不落到底下那个真标签页）。
  const onSession = (id: Parameters<typeof tabs.shadowedBySlot>[0], fn: () => void): void =>
    dispatcher.bind(id, () => {
      if (!tabs.shadowedBySlot(id)) fn();
    });
  onSession("tab.close-archived", () => tabs.closeActiveIfArchived());
  onSession("tab.open-cwd", () => tabs.openActiveTabCwd());
  dispatcher.bind("needs.next", () => tabs.jumpToNextNeeds());
  onSession("tab.pop-out", () => tabs.openActiveInNewWindow());
  // 会话内查找（大纲同一块面板）；历史查看器开着时落在它上面（它盖在 tab 上）。
  dispatcher.bind("session.find", () => {
    if (!historyView.openFind() && !tabs.shadowedBySlot("session.find")) tabs.openFind();
  });
  onSession("session.toggle-process", () => tabs.toggleProcessDefault());
  onSession("session.prev-turn", () => tabs.stepTurn(-1));
  onSession("session.next-turn", () => tabs.stepTurn(1));
  onSession("terminal.bring-front", () => tabs.bringActiveTerminalToFront());
  dispatcher.bind("app.open-settings", () => void openSettingsWindow()); // 开独立设置窗口
  dispatcher.bind("app.toggle-history", () => overlays.toggle("history"));
  dispatcher.bind("app.open-command-bar", () => commandBar.toggle()); // Ctrl+K 命令栏
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.bind("app.toggle-fullscreen", toggleFullscreen);
  dispatcher.bind("app.keys", () => keysOverview.toggle());
  dispatcher.bind("app.zoom-in", () => stepZoom(1));
  dispatcher.bind("app.zoom-out", () => stepZoom(-1));
  dispatcher.bind("app.zoom-reset", () => stepZoom(0));
  dispatcher.bind("app.toggle-tab-bar", toggleTabBarFold);
  dispatcher.bind("app.open-cc-bus", () => overlays.toggle("cc-bus"));
  dispatcher.bind("app.undo", () => void undoLatest());
  onSession("tab.context-menu", () => tabs.openActiveMenu());
  // 标签页栏里（焦点在栏里）：挪一格 · 出组 · 与上一个成组 · 改分组名；F6 主区 ↔ 栏。
  dispatcher.bind("tabBar.move-up", () => tabs.moveFocused(-1));
  dispatcher.bind("tabBar.move-down", () => tabs.moveFocused(1));
  dispatcher.bind("tabBar.leave-group", () => tabs.leaveFocused());
  dispatcher.bind("tabBar.join-prev", () => tabs.joinPrevFocused());
  dispatcher.bind("tabBar.rename-group", () => tabs.renameFocusedGroup());
  dispatcher.bind("tabBar.focus-cycle", () => tabs.cycleFocus());
  onSession("session.to-bottom", () => tabs.toBottom());
  dispatcher.bind("panel.toggle-tasks", () => mainDrawer.toggle("tasks"));
  dispatcher.bind("panel.toggle-agents", () => mainDrawer.toggle("agents"));
  dispatcher.bind("panel.toggle-terminal", () => mainDrawer.toggle("terminal"));
  // 翻完说一句、并告诉设置窗（`behavior-toggle.ts`）。
  dispatcher.bind("behavior.toggle-auto-follow", () => void flipBehavior("autoFollowUserActive", (b) => tabs.applyBehavior(b)));
  dispatcher.bind("behavior.toggle-bring-monitor", () => void flipBehavior("bringMonitorToFrontOnUserActive", (b) => tabs.applyBehavior(b)));

  // 账号相关快捷键（ACTIONS 里 default:null —— 默认不绑，用户想要自己去绑）。
  dispatcher.bind("account.switch-default", () => {
    void accountChip.openMenu(); // 显式入口（chip 隐藏时不开菜单），别用合成 click
  });

  // 先加载用户覆盖，再 start —— 避免 start 后 1-2ms 内按键走 default 而非用户值
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();

  // 只在 DEV 的 E2E 探针（抖动采样 ＋ Ctrl+Alt+F9 状态快照 → fe_perf 日志）：生产构建整支被 vite 消掉；热键不进 keybindings 登记。
  // 须在 bindEvents 之前就绪（启动那一批紧随 frontend-ready）。
  if (import.meta.env.DEV) {
    try {
      e2eProbe = await import("./e2e-probe");
      e2eProbe.registerSnapshotHotkey(
        () => tabs.debugSnapshot(),
        () => tabs.debugSessionsSnapshot(), // 全会话状态出口（Ctrl+Alt+F10 / 中键账号 chip）
      );
    } catch (e) {
      console.warn("[e2e-probe] 加载失败(不影响功能):", e);
    }
  }

  // 会话内容经通道 `subscribe` 来：每台机器一条 `session-lines`。哪几台由后端注册表说了算
  //   （`backend_machines`：本机恒第一，远端是起步时各起了一条流的那几台）。问不到 ⇒ 至少订本机。
  let machines: string[] = [LOCAL_ORIGIN];
  try {
    machines = await commands.backend_machines();
  } catch (e) {
    console.warn("[events] backend_machines 失败，只订本机的会话流：", e);
  }
  // await：保证 listener 注册完成、会话流订阅在 monitor 那一侧登记好，再 emit frontend-ready
  // （它就是这些订阅的就绪点：后端先重发宣告、再按 credit 交留存、再对账 —— 顺序见 `event_replay·rs::ready_point`）。
  await bindEvents({
    // 前端按 seq 排进 timeline，不分重放 / 实时
    onLine: (e) => {
      tabs.onLine(e);
      recordFile.afterLine(e.session_id); // 又来了一行 ⇒ 「记录文件不见了」那一句收掉
    },
    onSessionEnded: (sessionId) => {
      tabs.archiveTab(sessionId);
      accountsRefresher.request(); // 会话结束了
    },
    // 远端 claude 退了但 tmux 会话仍在 → 灰灯（idle-tmux，不归档）。
    onSessionIdle: (sessionId) => tabs.markTmuxIdle(sessionId),
    // 活会话的容器 · 某台机器的活会话清单报完了（说不清 → 已结束）。
    onSessionContainer: (sessionId, container) => tabs.noteContainer(sessionId, container),
    onOriginSessionsListed: (origin, all) => {
      tabs.markOriginSeen(origin);
      if (!all) return;
      // 各台都报完了（壳那一拍）⇒ 没回来的组员收掉、空组头不留；记住的那一格还没出现 ⇒ 明说。
      tabs.markAllListed();
      startup?.onAllListed();
    },
    // 中转抄出来的 SSE 事件（会话流 `session-tap`）→ 活卡（jsonl 到了整轮覆盖）；那台看不见了 ⇒ 活卡全撤。
    onSessionTap: (e) => tabs.onSessionTap(e),
    onSessionRuns: (p) => tabs.onSessionRuns(p),
    onSessionTapLost: (origin) => tabs.dropLiveCards(origin),
    // 那台的长连接又通了 / 那台账号清单变了 ⇒ 强制刷账号清单 ＋ chip（`accounts-changed` 流）。
    onAccountsChanged,
    // 记录文件不见了 / 被改过已从头重读 ⇒ 那个 tab 顶上说一句。
    onSessionFileNotice: (sessionId, change) => {
      tabs.onRecordFileReread(sessionId, change); // 从头重读 ⇒ tab 整份重来（先重来、再在新的流容器上说那一句）
      recordFile.onSessionFileNotice(sessionId, change);
    },
    // 那台机器看不见了 ⇒ 那台上活的 · 可重连的说不清（不是已结束）。机器级一格。
    onOriginUnseen: (origin) => tabs.markOriginUnseen(origin),
    // 一台机器一直看不见 ⇒ 提示里「打开设置」（那台连不连得上在机器页里看得到）。
    openMachineSettings: (origin) => void openSettingsWindow(undefined, dest.machineOf(origin)),
    // 会话复活（resume）：后端 liveness 门控后才发，复活已归档的本地 Tab，免 F5。
    // 没有 Tab（运行中途新出现的本地会话）→ 建骨架：bg 会话的 kind / name 只从这里来（首行建的 tab 不带 kind）。
    onSessionStarted: (sessionId, meta) => {
      const had = tabs.hasTab(sessionId);
      // 没有就建骨架；已有（固定的 / 归档的）就对齐后端给的项目目录，再复活。
      tabs.createSkeletonTab(sessionId, meta.projectDir || null, LOCAL_ORIGIN, meta.background, meta.name);
      if (had) tabs.reviveTab(sessionId);
      // 本机骨架也在就绪点才到（与远端同一条路）⇒ 上次所在 tab 是本机会话时同样在这里补切。
      startup?.onAppeared(sessionId);
      noteLive(LOCAL_ORIGIN, sessionId, { cwd: meta.cwd }); // 起会话的真成功正信号
    },
    // 启动重放期间走批模式（惰性高亮 ＋ BranchFolder.batchMode），结束时 flush。
    // DEV 抖动探针跨在批窗口上（生产 probe 恒 null）。
    onBatchStart: () => {
      e2eProbe?.startReplayJitterProbe();
      tabs.onBatchStart();
    },
    onBatchEnd: () => {
      e2eProbe?.stopReplayJitterProbe();
      tabs.onBatchEnd();
    },
    // 那台后端说这几个会话的任务变了（或期间可能漏了）⇒ 重问 `tasks-list`。
    onTasksChanged: (origin, sids, all) => tabs.refreshTasks(origin, sids, all),
    // 那台的额度账 / 某个会话的轮换变了 ⇒ 重问（`acct-center.ts`；画的那几处订 store）。
    onQuotaChanged,
    // 会话红绿灯（后端翻好的活动态）
    onSessionActivity: (e) =>
      tabs.updateActivity(e.session_id, e.activity, e.waiting_for),
    // 远端会话宣告 → 骨架 Tab，附项目目录 / kind / name：骨架标题当场完整（bg → ⚙ ＋ 任务名）；没给项目目录 ⇒ 标题退到 aiTitle / sid。
    onRemoteSessionAdded: (sessionId, origin, meta) => {
      noteLive(origin, sessionId, { cwd: meta.cwd }); // 起会话的真成功正信号
      tabs.createSkeletonTab(
        sessionId,
        meta.projectDir || null,
        origin,
        meta.background,
        meta.name,
        meta.attachable,
      );
      // 上次所在 tab 是远端会话时在此补切（应用一次即清）。
      startup?.onAppeared(sessionId);
      accountsRefresher.request(); // 会话起了

    },
    // 那台机器的会话流丢了几格 ⇒ 那台的每个 tab 按行号补（`TabManager.onStreamGap`）。
    onStreamGap: (origin) => tabs.onStreamGap(origin),
  }, {
    streams: machines.map((origin) => ({ origin, kind: "session-lines" })),
    // 每台的中转都住那台的常驻后端 ⇒ 每台都订 tap（与会话行同一份机器清单）。
    taps: machines,
    // 每台一条 `accounts-changed`（替掉裸事件 `remote-backend-ready`）。
    accounts: machines,
    // 每台一条 `session-tasks`（替掉本机那个裸事件 `task-update`；远端第一次有推送）。
    tasks: machines,
    // 每台一条 `quota-changed`（额度账 · 会话轮换变了）。
    quota: machines,
  });
  // 账号那一格经通道订（每台一条 `accounts-changed`，上面 `bindEvents` 的 `accounts`）。
  //   订阅登记之前那一窗里连上的不会有 `seen`（句柄只在状态变时说）⇒ `bindEvents` 返回（订阅都登记好了）之后补刷一次 ——
  //   与「独立窗口的订阅本身就是它的就绪点」同一个道理。
  onAccountsChanged();

  // 后端 ERROR 级别 tracing → 右下角红色 toast
  bindErrorToast();
  // 远端健康事件（拥塞丢行 / 版本不符）→ 右下角 info toast
  bindRemoteHealthToast();

  // 先读记忆再建骨架：骨架（会话流里的 `live` 成品，在下面 `frontend-ready` 的就绪点才到）一建出来就可能经 switchTo 写回 localStorage，读晚了就把记忆覆写掉。
  // 启动 active = 上次所在 tab：此刻还没有骨架 ⇒ 挂 pending，等它的 `live` 到达时补切（本机 `onSessionStarted` ·
  //   远端 `onRemoteSessionAdded` 两处同一段；应用一次即清，30 s 之后不再抢焦点）。
  const lastActive = safeGet(LS_KEYS.lastActiveSid);
  startup = new StartupActive(lastActive, !!lastActive && tabs.hasTab(lastActive), {
    switchTo: (sid) => tabs.switchTo(sid, "auto"),
    holdMemory: (hold) => {
      tabs.persistLastActive = !hold;
    },
    rememberCurrent: () => {
      const cur = tabs.activeSessionId();
      if (cur) safeSet(LS_KEYS.lastActiveSid, cur);
    },
    sayGone: (sid) =>
      toast(copyText("startupActive.gone.title"), copyText("startupActive.gone.body", { sid: sid.slice(0, 8) }), {
        level: "info",
      }),
  });
  // 那一拍要是在它之前就到了（就绪点之前的重放）⇒ 当场补上。
  if (tabs.everAllListed) startup.onAllListed();

  // 通知后端可以发了 —— 缓冲的行会被 flush 过来。payload 带上次所在 tab：后端重放按会话分组、那个 tab 的先发。
  window.__ccmPerf.frontendReadyEmit = performance.now();
  console.info(
    `[perf] emit frontend-ready @ ${window.__ccmPerf.frontendReadyEmit.toFixed(0)}ms`,
  );
  // `frontend-ready` 是方向相反的那个 payload（前端发、Rust 收）：类型生成自 `ui_contract.rs::FrontendReadyPayload`
  //   （字段级 `#[serde(rename = "prioritySid")]`，不是容器级 `rename_all`）。
  const frontendReady: FrontendReadyPayload = { prioritySid: lastActive };
  void emit("frontend-ready", frontendReady);

  // 红绿灯的初始值随 `live` 成品一起在就绪点交（`activity` 那一格）。

  // 最大化 / 全屏后内容错位由 Rust 侧修（`src/frontend/shell/src/lib.rs` on_window_event：去抖后微调 webview 尺寸，
  //   强制 wry 重新 put_Bounds，把 WebView2 合成层钉回左上角）—— DOM 这一层够不着合成层偏移。
});
