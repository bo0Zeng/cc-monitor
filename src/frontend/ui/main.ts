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
import { OPEN_ACCOUNT_PANEL_EVENT, RESYNC_DONE_EVENT, type OpenAccountPanel } from "./settings/events";
import { SWITCH_TO_SESSION_EVENT, type SwitchToSession } from "./window-events";
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
import { restoreZoom, stepZoom } from "./zoom";
import { mountTabBarFold, tabBarManuallyFolded, toggleTabBarFold } from "./tab-bar-fold";
import { attachTooltip } from "./kit/tooltip";
import { bindErrorToast } from "./backend-errors";
import { bindRemoteHealthToast } from "./remote-health";
// F83（#39）：顶栏远端文件入口——按远端主机数 0/1/N 分支（`sftp-host-picker.ts`）。
import { openSftpFromTopbar, toggleSftpFromTopbar } from "./sftp-host-picker";
import { readRemoteConfig } from "./remote-config";
import { createUnknownKeysBar } from "./settings/unknown-keys-notice";
import { openSettingsWindow } from "./settings/open-settings"; // ST1：点「设置」有反馈（不 import 设置面板）
import * as dest from "./settings-dest";
import { collectAccountRows, createEventRefresher } from "./session-accounts-poll";
import { lastAccounts } from "./history-reads";
import { TasksPanel } from "./tasks-panel";
import { AgentsPanel } from "./agents-panel";
import { MainDrawer } from "./main-drawer";
import { TerminalPage } from "./terminal-page";
import type { Tab } from "./tab-model";
import { REVEAL_RUN_EVENT } from "./cards/speaker-bar";
import { getBehavior } from "./behavior";
import { flipBehavior } from "./behavior-toggle";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { installGlobalClickDelegation } from "./entry-render-common";
import { AccountChip } from "./account-chip";
import { onQuotaChanged, syncSessions } from "./acct-center";
import { followActive, openAccountPanelAt, toggleAccountPanel, type AcctPanelHost } from "./acct-panel";
import { jumpToAccountPanel } from "./acct-jump";
import { fullTitle } from "./session-face";
import { acctSessionWiring } from "./acct-session";
import { buildAccountCommands } from "./account-commands";
import { sessionCommands } from "./session-commands";
import type { FrontendReadyPayload } from "./generated/FrontendReadyPayload";
import { currentAccountForBadge } from "./accounts";
import { fetchSessionAccounts, fetchAccounts } from "./account-reads";
import { bindLaunchArrivals, noteLive } from "./launch-arrival";
import { FOCUS_SESSION_EVENT, openNewSession, setNewSessionFocus } from "./new-session";
import { copyText } from "./copy-table";
import { appStore } from "./app-store";
import { OverlayRouter } from "./overlay-router";
import { SessionHead } from "./session-head";
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
      /** Batch13-F40 仪表:content 记录真实建卡数(renderMessage 走到的次数) */
      recordsRendered?: number;
      /** Batch13-F40 仪表:被尾部优先门控收纳(不建卡)的 content 记录数 */
      recordsDeferred?: number;
    };
  }
}
window.__ccmPerf = {
  domContentLoaded: 0,
};

// F40c:DEV-only E2E 探针句柄(动态 import;生产恒 null,调用点全部可选链)
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

// 全局错误捕获（渲染到 status-bar）搬去了 `entry-common.ts` —— 三个入口共用，
// 原先靠「三个窗口都加载 main.ts」顺带拿到（三入口拆分）。

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

  // 〔三入口拆分〕独立只读窗口与独立设置窗口**不再经过这里**：
  // 它们各有自己的 html 与入口模块（`viewer.html → src/frontend/ui/entry-viewer.ts`、
  // `settings.html → src/frontend/ui/entry-settings.ts`），模块图里根本没有本文件。
  // 原先这里按 `?viewer=` / `?settings=1` 分叉，代价是两个精简窗也要加载主窗口的整份包。

  const tabBar = document.getElementById("tab-bar");
  const streamRoot = document.getElementById("message-stream");
  const status = document.getElementById("status-bar");

  if (!tabBar || !streamRoot || !status) {
    console.error("layout containers missing");
    return;
  }

  // 〔条 66〕这里原来在启动时把 config.json 里的后端策略**推**给 Rust。
  //   那个值搬到了后端所在那台机器上，由后端在决定那一刻现读 ⇒
  //   **没有东西要推了**，这一步整条删掉（留着就是第二个源头）。

  // 状态栏：消息 · 空 · 这个会话的几枚（任务 · agent · 上下文 · 账号）· 竖线 · 命令。没内容的那枚不渲染。
  status.innerHTML = "";
  // 最左「消息」：本次运行里最近 20 条提示（toast 收进来的那几条也在这里找得回）。
  status.appendChild(new StatusMessages().el);
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
  const mainDrawer = new MainDrawer(tasksPanel, agentsPanel, terminalPage, () => (document.getElementById("app")?.clientHeight ?? window.innerHeight) - status.getBoundingClientRect().height);
  mainDrawer.dock.el.id = "bottom-drawer";
  document.getElementById("app")?.insertBefore(mainDrawer.dock.el, status);
  // 消息流里 agent 事件条的「打开窗口 ›」：把那个 agent 摆到眼前（抽屉 agent 页里它那一行的时间线）。
  document.addEventListener(REVEAL_RUN_EVENT, (e) => {
    const run = (e as CustomEvent<{ run?: unknown }>).detail?.run;
    if (typeof run === "string") agentsPanel.reveal(run);
  });

  // 「上下文」chip：当前会话最新一轮占上限多少（后端定上限）；点开看用量与上限来源。
  const usageHud = new UsageHud();
  status.appendChild(usageHud.summaryElement);

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.innerHTML = copyText("main.empty.noSessions");
  streamRoot.appendChild(empty);

  // Batch5-F19：启动时记住的那一格（上次所在的 tab）—— 只在那个会话出现时恢复、就绪前不许被覆盖、
  //   没等到就明说（`src/frontend/ui/startup-active.ts`）。原先的 30 s 启动窗口（迟到的宣告静默不切、窗口内记忆早被别的 tab 写掉）换成按事件判。
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
  // P7a-3（#61）：启动时拉一次标签页集合（住 `config.json`，不是 localStorage —— 见
  // `tab-collections.ts` 头注：集合名是用户手写的真相，必须活过一次清缓存）。
  void tabs.loadCollections();
  // 顺序也要拉回来。排在集合之后：两者都只改内存 + 重画，互不依赖，
  // 但集合决定分组容器、顺序决定容器内次序 —— 先有容器再排，少一次无谓重画。
  //
  // 固定表排在顺序之前，串着跑（不是并排 `void`）。
  // ⚠ **2026-09-21 订正：这个次序不再是承重的。** 原先这儿写着「**必须**排在顺序之前」，
  //   理由是「`loadOrder` 按今天真的存在的 sid 过滤，固定的 tab 得先造出来，
  //   否则位置会被当成已删会话整条摘掉」——**那道读时过滤本身就是步 17·C 那个 bug**
  //   （`tabs.ts` 的 `loadOrder` 头注记着成因与现打）。顺序现在是一份**留着的意图**，
  //   tab 陆续到达时会被反复应用 ⇒ 谁先谁后都排得回来。次序留着是因为它无害且省一次重画。
  // 🔴 **也正因为如此，这一行不再是「顺序能不能活过重启」的承重点** ——
  //   那条性质今天由 `tests/frontend/ui/tabs.vitest.ts`「步 17·C 顺序落盘：读回来那一半」按
  //   **真实启动时序**（loadPinned → loadOrder → 会话陆续到）盯着，9 格全是相等断言。
  // ⚠ `finally` 不是 `then`：`loadPinned` 失败也得让顺序照常回来（互不为前提）。
  void tabs.loadPinned().finally(() => void tabs.loadOrder());

  // A3：状态栏「当前账号」chip（多账号）。绑第一台可用远端的默认账号；
  // 未连远端 / 未启用多账号 各自安静降级（不报错）。点击弹选单切默认账号。
  // **构造在 `refreshSessionAccounts` 定义之前**（D 审计）：`onDefaultChanged` 回调间接调用
  // `refreshSessionAccounts`（下方 `const` 声明，函数体里会写 `appStore.sessionAccounts`）——
  // 回调本身只在用户切号时才真正执行，届时 `refreshSessionAccounts` 早已初始化完毕，不会踩
  // TDZ；但这条"届时早已初始化"的保证依赖"这中间没有 await 会提前执行到回调"这条隐式不变量，
  // 谁在中间插一个真会被调用的 await 就有踩 TDZ 的风险，需要留意。
  // 「账号」面板（右侧抽屉）：状态栏按钮 · 右键「账号…」· 会话头 ⋯ 开同一个。
  const panelHost: AcctPanelHost = {
    sessionTitle: (sid) => tabs.snapshotSessions().find((x) => x.sessionId === sid)?.title ?? sid.slice(0, 8),
    cwdOf: (sid) => tabs.snapshotSessions().find((x) => x.sessionId === sid)?.cwd ?? "",
    openSettings: (origin) => void openSettingsWindow(undefined, dest.accountsOf(origin)),
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
  // ↗ 浮层的两颗：［接上终端］直达设置那一节 ·［更新］开那台机器页（更新那一颗只住机器卡上）。
  tabs.onConnectTerminal = () => void openSettingsWindow(undefined, dest.connectTerminalOf(LOCAL_ORIGIN));
  tabs.onUpdateMachine = (origin) => void openSettingsWindow(undefined, dest.machineOf(origin));
  // 起新会话：那台报出新会话 ⇒ 切过去（在主窗口里起的直接切；在设置 / 查看窗里起的点了［切过去］再切，先把主窗口拉到前面）。
  setNewSessionFocus((_origin, sid) => tabs.switchTo(sid));
  void listen<{ origin: string; sid: string }>(FOCUS_SESSION_EVENT, (e) => {
    const w = getCurrentWindow();
    void w.unminimize().then(() => w.setFocus()).catch(() => {});
    tabs.switchTo(e.payload.sid);
  });
  // 设置窗账号页「时间轴 · 默认轮换」⇒ 主窗口拉到前面、开账号面板滚到那一节（只开不写）。
  void listen<OpenAccountPanel>(OPEN_ACCOUNT_PANEL_EVENT, (e) =>
    jumpToAccountPanel(e.payload, {
      raise: () => {
        const w = getCurrentWindow();
        void w.unminimize().then(() => w.setFocus()).catch(() => {});
      },
      active: () => {
        const sid = tabs.activeSessionId();
        const origin = sid === null ? null : tabs.originOf(sid);
        return sid !== null && origin !== null ? { sid, origin } : null;
      },
      firstOn: (origin) => {
        const t = tabs.tabsInOrder().find((x) => x.origin === origin);
        return t ? { sid: t.sessionId, title: fullTitle(t) } : null;
      },
      switchTo: (sid) => tabs.switchTo(sid),
      openAt: (sid, origin, anchor) => openAccountPanelAt(sid, origin, panelHost, anchor),
    }),
  );
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

  // A3：账号徽章数据管道——定期对每台远端拉 session-accounts（哪条会话属于哪个号）+ 账号邮箱，
  // 聚合喂 tabs（tabs 已在上方构造）。走 accounts store 的 8s TTL 缓存 + available:false 降级，
  // 未迁移 / 旧后端零副作用。
  // audit-fixes I4：refreshSessionAccounts 无重入/顺序保护 → 慢的旧快照可覆盖新快照（把切号后刚
  // 关上的"反向窗口"从并发侧重开）。加 in-flight 递增序号门：每次进入 ++refreshSeq 取本地 mySeq，
  // 写账号快照前若 refreshSeq 已被更晚一次进入推大（mySeq !== refreshSeq）→ 丢弃本次。
  // ⚠ 句柄留着：F14 第三刀之前这个 interval 的句柄是**丢掉的**，全仓没人停得了它。
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
      // audit-0805 F14 第三刀（报告 I-5）：这圈扇出原来是**串行**的（`for` 里直接 `await`，
      // `Promise.all` 只并行同一台的两条）⇒ N 台远端的一轮 = N 次往返串起来。
      // 现在走 `collectAccountRows`：**有上限（4）、保序**。判据在
      // `session-accounts-poll.vitest.ts`，`readyOrigins` / `currentByOrigin` 的语义原样搬过去。
      const { rows, emailByName, readyOrigins, currentByOrigin, lastByS } = await collectAccountRows(
        cfg.hosts,
        { fetchSessionAccounts, fetchAccounts, currentAccountForBadge, lastAccounts },
        undefined,
        forceAccounts,
      );
      if (mySeq !== refreshSeq) return; // I4：晚到的旧快照不覆盖新快照
      appStore.sessionAccounts.set({ rows, emailByName, lastByS, readyOrigins, currentByOrigin });
    } catch (e) {
      console.warn("refreshSessionAccounts failed:", e);
    }
  };
  // 🔴 那个 10 秒轮询**删了**（理由整段在 `session-accounts-poll.ts` 头注）：
  // 两条查询搬上了已有的长连接，而「会话 ↔ 账号」只在会话起停时变 —— 那本来就有事件。
  // ⇒ 刷新改由事件驱动，零定时器：
  //   · 某台的长连接握手完成（启动 / 重连）或那台账号清单变了 ⇒ 强制刷账号清单，
  //     账号 chip 也在这一刻重取（在那之前问只会拿到「没有控制通道」）—— 经通道订的 `accounts-changed`；
  //   · 远端 `live` 格 / `ended` 格：会话起停；
  //   · 本 UI 切号：上面 `onDefaultChanged`。
  const accountsRefresher = createEventRefresher(refreshSessionAccounts);
  accountsRefresher.request();
  // 「某台长连接握手完成 / 那台账号清单变了」⇒ 强制刷账号清单 ＋ chip。原先听裸事件 `remote-backend-ready`；
  //   今天经通道订每台的 `accounts-changed`（下面 `bindEvents` 的 `accounts` ＋ `onAccountsChanged`）。
  const onAccountsChanged = (): void => {
    accountsRefresher.request(true);
    void accountChip.refresh(true);
  };
  // 会话起停那一格（原先听裸事件 `remote-session-added`〔散文墓碑〕/ `session-ended`〔散文墓碑〕，那两个事件已并进会话流）：
  //   下面 `bindEvents` 的 `onRemoteSessionAdded` / `onSessionEnded` 里各请一次。

  // Batch5-F19（G 验收）：用户手动切过 tab 后，迟到的宣告不再补切抢焦点（用户的选择优先，记下此刻所在那一格）
  tabs.onManualSwitch = () => {
    startup?.onManualSwitch();
  };
  // F88b：当前 tab 那一格变了 → 刷新 HUD context% chip（后端的会话事实到了 / switchTo 切会话 / 要不到 ⇒ chip 说原因）。
  // 订阅 store（原先是两个点对点回调）。
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
  tabs.active.subscribe((a) => {
    // 切了 tab ⇒ 会话头 · 「需要你」钉条 · 抽屉的终端页换成这一个。
    sessionHead.render();
    terminalPage.sessionChanged();
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

  // v2.4 issue #2：拉一次 behavior toggle 初值喂给 TabManager。
  // 设置面板改了之后会再调 applyBehavior 同步。
  void getBehavior().then((b) => tabs.applyBehavior(b));

  // F82a（#56+#47）：设置改由**独立窗口**承载（SS-3 终态），主窗口不再内嵌设置浮层。
  // 设置窗口保存 / 行为 toggle 后广播 SETTINGS_APPLIED_EVENT → 主窗口重读并应用主题 + 行为
  // （跨 OS 窗口回调够不到，用广播代替原 onBehaviorChange 直连）。loadTheme 内部 applyTheme。
  // F84b-fix(batch18)：状态栏命令 chip 的键位刷新钩子——改键热应用后同步刷新 chip 显示的 kbd
  //（原实现只在 bootstrap 算一次，改键后 chip 教死键；chip 创建时把重建函数赋进来）。
  let refreshCmdkChord: () => void = () => {};
  // 设置窗「重新对齐」做完 ⇒ 标出那台上记录没了的固定条（不自动摘）。
  void listen<{ origin: string }>(RESYNC_DONE_EVENT, (e) => void tabs.flagPinsWithoutRecord(e.payload.origin));
  // 任何窗口起会话之后交过来的「等它」都在这里收（主窗口订着每台的会话流）。
  bindLaunchArrivals();
  // 独立查看窗里点［切过去］⇒ 主窗口拉到前面、切到那个会话的标签页。
  void listen<SwitchToSession>(SWITCH_TO_SESSION_EVENT, (e) => {
    tabs.switchTo(e.payload.sessionId);
    const w = getCurrentWindow();
    void w.unminimize().then(() => w.setFocus()).catch(() => {});
  });
  void listen(SETTINGS_APPLIED_EVENT, () => {
    void loadTheme(); // 主题：loadTheme 内部 applyTheme
    void getBehavior().then((b) => tabs.applyBehavior(b)); // 行为
    void getKeybindings().then((kb) => {
      dispatcher.applyOverrides(kb); // 键位：热应用主窗口 dispatcher
      refreshCmdkChord(); // 键位变 → 同步刷新命令 chip 的 kbd（兑现「改键即变」）
    });
    void accountChip.refresh(true); // A3：远端配置/默认账号可能变了，刷新账号 chip
    void loadContextLimits(); // 上下文上限表可能在设置页改过
  });
  // Batch11-F33：竖直 tab 栏——右缘拖拽调宽。整段搬进 tab 栏自己的模块（`tab-bar-width.ts`），
  // 宽度经 `LS_KEYS` ＋ `safeGet/safeSet` 记忆（D §D5：原先住这里、直调 localStorage、键不在登记里）。
  mountTabBarResizer();

  // 栏顶一排全局入口（会话总览 · 文件 · 历史 · 设置）：图标按钮 ＋ 悬停「名字 · 当前键位」，摆进标签页栏顶（不再浮在消息流上）。
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
    void openSettingsWindow(settingsTrigger); // F82a：开独立设置窗口（非浮层）· ST1：点了有反馈
  });

  // 历史浏览器入口 —— 顶栏右侧，紧邻设置按钮左边
  // v2.5+: HistoryView 不再接管 streamRoot，自挂 body 作 fixed overlay
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

  // F91（#27）：多 agent 并排监控入口 —— 顶栏右侧一排（SFTP 入口左边，right:104px）。跨机器只读
  // mission-control 状态板（一屏看所有会话实时状态，点卡片跳会话；只读——不派发/不驱动 agent）。
  const gridMonitorView = new GridMonitorView(tabs);
  overlays.register("grid", gridMonitorView);
  const gridTrigger = headButton("grid-monitor-trigger", "grid", copyText("main.cmd.openGrid"), hintWithKey(copyText("main.topbar.gridHint"), null), () => overlays.toggle("grid"));

  // F83（#39）：顶栏远端文件入口 —— 设置搬独立窗后腾出的入口位。点击按远端主机数分支：
  // 0 台提示 / 1 台直开 / 多台选单（选单见 openSftpFromTopbar）。终点是原生文件窗口（`file-window.ts`）。
  const sftpTrigger = headButton("sftp-trigger", "files", copyText("main.cmd.openFiles"), hintWithKey(copyText("main.topbar.filesHint"), null), () => void toggleSftpFromTopbar(sftpTrigger));
  tabs.mountHeadActions([gridTrigger, sftpTrigger, historyTrigger, settingsTrigger]);

  // S6（settings-ia）：cc-bus 驾驶舱从设置里搬出来，成为顶层运营视图。
  // **刻意不加第 7 个顶栏图标** —— F84 加命令面板时就为「顶栏已拥挤」立过先例
  //（那次逐字写着「键位唯一入口…按钮延后」）。驾驶舱是低频运营视图，正是面板服务的对象。
  // 理由完整版见 `views/cc-bus-view.ts` 头注。
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
      { id: "zoom-reset", group: "window", icon: "zoomReset", title: copyText("main.cmd.zoomReset"), keywords: copyText("main.cmd.zoomKeywords"), hint: chordHint("app.zoom-reset"), run: () => stepZoom(0) },
    );
    // 账号：每个号一条「设 X 为默认账号」· 管理账号…（构造在 account-commands.ts，纯函数）· 新会话默认…（同状态栏无会话时那个下拉）。
    cmds.push(
      ...buildAccountCommands({
        snapshot: accountChip.snapshotReady(),
        chordHint: (id) => chordHint(id as Parameters<typeof chordHint>[0]),
        setCurrent: (name) => void accountChip.applyDefaultByName(name),
        openSettings: () => void openSettingsWindow(undefined, dest.accountsOf(curOrigin ?? LOCAL_ORIGIN)),
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

  // F84b（batch17）：命令栏可发现性——命令栏原本是纯键位入口（Ctrl+K），无任何可见提示它存在。
  // 状态栏最右加一个克制的常驻 chip：图标+「命令」+真实键位（跟随 effectiveChord，改键即变——见 refreshCmdkChord）；
  // 本身即点击入口（复用 commandBar.toggle，非新路径）。placement 见 F84b 计划（顶栏已 6 图标拥挤，选状态栏）。
  {
    const cmdkHint = document.createElement("button");
    cmdkHint.type = "button";
    cmdkHint.className = "status-cmdk";
    cmdkHint.appendChild(icon("command", "compact"));
    const label = document.createElement("span");
    label.textContent = copyText("main.cmdk.label");
    cmdkHint.appendChild(label);
    // F84b-fix：kbd 抽成可重建——改键热应用后经 refreshCmdkChord 刷新，不再教死键。
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
    // F84b-fix：首运行一次性微高亮（非模态），帮新用户注意到这个克制的角落入口；见过即不再。
    if (!safeGet(LS_KEYS.cmdkHintSeen)) {
      cmdkHint.classList.add("first-run");
      safeSet(LS_KEYS.cmdkHintSeen, "1");
    }
  }

  // ⚠ 只在主窗口挂：独立 viewer 窗与设置窗各走自己的入口（`entry-viewer.ts` / `entry-settings.ts`），不经过本文件。
  // 🔴 **「配置里有个键没生效」这句话要在主窗口也说得出来。**
  //
  // `P12` 那一刀把它做在了设置面板顶上，而做那一刀的人自己报备了剩下的缺口，逐字：
  // 「**从没打开过设置的用户今天仍然看不到**」——`SettingsPanel` 只在设置窗构造。
  //
  // 这一形本仓治过一次，而且理由写在 `first-run-hint.ts` 头注里：
  // 「一个刚装完、还没打开过设置的人，一个字都看不到」⇒ `N-F3` 的解法是
  // **主窗口状态栏上一条非模态的指路**。这里照那条先例办，不另发明一个 UI。
  //
  // ⚠ **同一个组件、两个挂点** —— 那句话只有一个住址（`unknown-keys-notice.ts`），
  //   不在主窗口再写第二份文案。`createUnknownKeysBar()` 自己先用快照渲、
  //   再拿盘上那份重渲 ⇒ 不用额外接线。
  //
  // ⚠ **判据强度如实说**：设置面板那一侧有一条**真渲染面板、去 DOM 里找那个键名**的判据
  //   （`tests/frontend/ui/settings/unknown-keys-notice.vitest.ts`）；主窗口这一侧**没有同等的**
  //   ——`main.ts` 今天没有 DOM 判据台架，本处只由源码扫描盯着。
  //   而 `P12` 的死值验刀 1 现打证明过：**源码扫描看不见「挂没挂上」**
  //   （组件在、函数在、只是不 appendChild ⇒ 扫描照样绿）。
  //   ⇒ 这一挂点的失效形状今天**判不了**，缺的是 `main.ts` 的 DOM 台架。**登记，不假装。**
  status.appendChild(createUnknownKeysBar());


  // 外链 + 代码块复制的全局 click 代理（主窗口 / 独立 viewer 窗口共用）
  installGlobalClickDelegation();

  // 快捷键：issue #5 走 KeybindingDispatcher 统一派发。
  // 各 action 默认 chord 见 keybindings/actions.ts；用户覆盖存 config.json
  // `keybindings` 字段。Esc 关弹层由 dispatcher 的 overlay stack 管理（settings /
  // history / tasks-panel 各自 push/pop）。
  dispatcher.bind("tab.next", () => tabs.cycleActive(1));
  dispatcher.bind("tab.prev", () => tabs.cycleActive(-1));
  for (let i = 1; i <= 9; i++) {
    dispatcher.bind(`tab.jump-${i}` as const, () => tabs.jumpToIndex(i));
  }
  dispatcher.bind("tab.close-archived", () => tabs.closeActiveIfArchived());
  dispatcher.bind("tab.open-cwd", () => tabs.openActiveTabCwd());
  dispatcher.bind("needs.next", () => tabs.jumpToNextNeeds());
  dispatcher.bind("tab.pop-out", () => tabs.openActiveInNewWindow());
  // 会话内查找（大纲同一块面板）；历史查看器开着时落在它上面（它盖在 tab 上）。
  dispatcher.bind("session.find", () => {
    if (!historyView.openFind()) tabs.openFind();
  });
  dispatcher.bind("session.toggle-process", () => tabs.toggleProcessDefault());
  dispatcher.bind("session.prev-turn", () => tabs.stepTurn(-1));
  dispatcher.bind("session.next-turn", () => tabs.stepTurn(1));
  dispatcher.bind("terminal.bring-front", () => tabs.bringActiveTerminalToFront());
  dispatcher.bind("app.open-settings", () => void openSettingsWindow()); // F82a：开独立设置窗口
  dispatcher.bind("app.toggle-history", () => overlays.toggle("history"));
  dispatcher.bind("app.open-command-bar", () => commandBar.toggle()); // F84（#57）Ctrl+K 命令栏
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.bind("app.toggle-fullscreen", toggleFullscreen);
  dispatcher.bind("app.keys", () => keysOverview.toggle());
  dispatcher.bind("app.zoom-in", () => stepZoom(1));
  dispatcher.bind("app.zoom-out", () => stepZoom(-1));
  dispatcher.bind("app.zoom-reset", () => stepZoom(0));
  dispatcher.bind("app.toggle-tab-bar", toggleTabBarFold);
  dispatcher.bind("app.open-cc-bus", () => overlays.toggle("cc-bus"));
  dispatcher.bind("app.undo", () => void undoLatest());
  dispatcher.bind("tab.context-menu", () => tabs.openActiveMenu());
  dispatcher.bind("session.to-bottom", () => tabs.toBottom());
  dispatcher.bind("panel.toggle-tasks", () => mainDrawer.toggle("tasks"));
  dispatcher.bind("panel.toggle-agents", () => mainDrawer.toggle("agents"));
  dispatcher.bind("panel.toggle-terminal", () => mainDrawer.toggle("terminal"));
  // 翻完说一句、并告诉设置窗（`behavior-toggle.ts`）。
  dispatcher.bind("behavior.toggle-auto-follow", () => void flipBehavior("autoFollowUserActive", (b) => tabs.applyBehavior(b)));
  dispatcher.bind("behavior.toggle-bring-monitor", () => void flipBehavior("bringMonitorToFrontOnUserActive", (b) => tabs.applyBehavior(b)));

  // account-ux U8：账号相关快捷键（ACTIONS 里 default:null —— 默认不绑，用户想要自己去绑）。
  dispatcher.bind("account.switch-default", () => {
    void accountChip.openMenu(); // 显式入口（chip 隐藏时不开菜单），别用合成 click
  });

  // 先加载用户覆盖，再 start —— 避免 start 后 1-2ms 内按键走 default 而非用户值
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();

  // F40c:DEV-only E2E 探针(抖动采样 + Ctrl+Alt+F9 状态快照 → fe_perf 日志)。
  // 生产构建 DEV 恒 false,整支被 vite 消除;热键不进 keybindings registry,
  // 不占用户配置面。须在 bindEvents 之前就绪(startup batch 紧随 frontend-ready)。
  if (import.meta.env.DEV) {
    try {
      e2eProbe = await import("./e2e-probe");
      e2eProbe.registerSnapshotHotkey(
        () => tabs.debugSnapshot(),
        () => tabs.debugSessionsSnapshot(), // F-E0:全会话状态出口(Ctrl+Alt+F10 / 中键账号 chip)
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
    // P5.2 B 重构：onLine 不再带 source 参数（前端按 seq timeline 排，不分 batch/live）
    onLine: (e) => {
      tabs.onLine(e);
      recordFile.afterLine(e.session_id); // 又来了一行 ⇒ 「记录文件不见了」那一句收掉
    },
    onSessionEnded: (sessionId) => {
      tabs.archiveTab(sessionId);
      accountsRefresher.request(); // 会话结束了（原先听裸事件）
    },
    // audit-fixes F03.2：远端 claude 退但 tmux 会话仍在 → 灰灯（idle-tmux 第三态，非归档）。
    onSessionIdle: (sessionId) => tabs.markTmuxIdle(sessionId),
    // 活会话的容器（G3）· 某台机器的活会话清单报完了（说不清 → 已结束）。
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
    // Batch7-F24：无 Tab（= 运行中途**新出现**的本地会话）→ 建骨架——bg 会话必须
    // 从这条通道拿 kind/name（首行 onLine→ensureTab 不带 kind，会建成无 ⚙ 普通 tab）。
    onSessionStarted: (sessionId, meta) => {
      const had = tabs.hasTab(sessionId);
      // 没有就建骨架；已有（固定的 / 归档的）就对齐后端给的项目目录，再复活。
      tabs.createSkeletonTab(sessionId, meta.projectDir || null, LOCAL_ORIGIN, meta.kind, meta.name);
      if (had) tabs.reviveTab(sessionId);
      // 本机骨架也在就绪点才到（与远端同一条路）⇒ 上次所在 tab 是本机会话时同样在这里补切。
      startup?.onAppeared(sessionId);
      noteLive(LOCAL_ORIGIN, sessionId, { cwd: meta.cwd }); // 起会话的真成功正信号
    },
    // 启动重放（jsonl-batch）期间走 batch 模式（lazy hljs + BranchFolder.batchMode），
    // 结束时 flush。onChunk 已删 —— B 重构后 chunk 切边界对前端不可见。
    // F40c:DEV 抖动探针跨在 batch 窗口上(生产 probe 恒 null,零开销)。
    onBatchStart: () => {
      e2eProbe?.startReplayJitterProbe();
      tabs.onBatchStart();
    },
    onBatchEnd: () => {
      e2eProbe?.stopReplayJitterProbe();
      tabs.onBatchEnd();
    },
    // v2.3.0 issue #11: task watcher 推送的 task 列表更新
    // 那台后端说这几个会话的任务变了（或期间可能漏了）⇒ 重问 `tasks-list`。
    onTasksChanged: (origin, sids, all) => tabs.refreshTasks(origin, sids, all),
    // 那台的额度账 / 某个会话的轮换变了 ⇒ 重问（`acct-center.ts`；画的那几处订 store）。
    onQuotaChanged,
    // issue #23: 会话红绿灯（busy=绿 / idle·shell=红 / waiting=黄）
    onSessionActivity: (e) =>
      tabs.updateActivity(e.session_id, e.status, e.waiting_for),
    // Batch5-F18：远端会话宣告 → 骨架 Tab。附项目目录 / kind / name
    // ——骨架标题即时完整（bg → ⚙ ＋ 任务名；不再挂宿主排成树）；旧后端不给项目目录 ⇒ 标题退到 aiTitle / sid。
    onRemoteSessionAdded: (sessionId, origin, meta) => {
      noteLive(origin, sessionId, { cwd: meta.cwd }); // 起会话的真成功正信号
      tabs.createSkeletonTab(
        sessionId,
        meta.projectDir || null,
        origin,
        meta.kind,
        meta.name,
        meta.attachable, // E73
      );
      // Batch5-F19：上次所在 tab 是远端会话时在此补切（应用一次即清）。
      startup?.onAppeared(sessionId);
      accountsRefresher.request(); // 会话起了（原先听裸事件）

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

  // v2.0.0 (issue #4)：后端 ERROR 级别 tracing → 右下角红色 toast
  bindErrorToast();
  // issue #32 (SS-F)：远端健康事件（拥塞丢行 / 版本不符）→ 右下角 info toast
  bindRemoteHealthToast();

  // Batch5-F19（G 验收 B-1）：**先读记忆再建骨架**——骨架（本机远端一样，都是会话流里的 `live` 成品，
  //   在下面 `frontend-ready` 的就绪点才到）一建出来就可能经 switchTo 写回 localStorage，读晚了就把记忆覆写掉。
  // 启动 active = 上次所在 tab：此刻还没有骨架 ⇒ 挂 pending，等它的 `live` 到达时补切（本机 `onSessionStarted` ·
  //   远端 `onRemoteSessionAdded` 两处同一段；应用一次即清，30 s 之后不再抢焦点）。
  //   〔从前本机骨架先经 `list_active_sessions`〔散文墓碑〕在这里建好、建时抑制写回（`persistLastActive = false … true`）；
  //    骨架挪到就绪点之后那一对抑制没有要罩住的东西了，删了。〕
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

  // 通知后端可以发了 —— 缓冲的 line 会被 flush 过来。payload 带上次所在 tab
  // （Batch5-F19）：后端 replay 按 session 分组、该 tab 的内容块先发。
  window.__ccmPerf.frontendReadyEmit = performance.now();
  console.info(
    `[perf] emit frontend-ready @ ${window.__ccmPerf.frontendReadyEmit.toFixed(0)}ms`,
  );
  // C02：`frontend-ready` 是**唯一方向相反**的 payload（前端 emit、Rust 收），
  // 而 TS 侧**从来没有过这个类型**——这是净新增能力，不是替换。
  // 生成物来自 `ui_contract.rs::FrontendReadyPayload`（只有 `Deserialize`；实测 `ts-rs` 照样生成）。
  // 它用的是**字段级** `#[serde(rename = "prioritySid")]`，不是容器级 `rename_all`。
  const frontendReady: FrontendReadyPayload = { prioritySid: lastActive };
  void emit("frontend-ready", frontendReady);

  // 红绿灯的初始值随 `live` 成品一起在就绪点交（`activity` 那一格），不再另拉快照（`list_session_activity`〔散文墓碑〕）。

  // 注：maximize / 全屏后内容错位的修复在 Rust 侧（src/frontend/shell/src/lib.rs on_window_event：
  // 去抖后微调 webview 尺寸强制 wry 重新 put_Bounds，把 WebView2 合成层钉回左上角）。
  // 旧版（v2.13.0）在这里做的 onResized + scrollTop 微滚动够不着 DOM 之下的合成层偏移，已删。
});
