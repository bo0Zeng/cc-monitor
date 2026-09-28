/**
 * **主窗口**的 bootstrap（入口模块是 `entry-main.ts`，由 `index.html` 加载）。
 * 〔三入口拆分 · `设计/01 §1.2`〕独立 viewer 窗与设置窗各有自己的入口，不加载本文件。
 * DOMContentLoaded 后按序：
 * 1. `loadTheme()` 从 config.json 应用 CSS 变量
 * 2. 实例化 TabManager / SettingsPanel / HistoryView / TasksPanel
 * 3. `bindEvents()` 订阅后端事件（jsonl-line / jsonl-batch / session-ended / task-update）
 * 4. 装全局快捷键 dispatcher（keybindings/）+ 外链 click 代理（openUrl）+ ERROR toast
 * 5. `emit("frontend-ready")` 通知后端 replay 历史
 *
 * 另持有启动 perf 测量（`window.__ccmPerf`，与后端 lib.rs 的 t0 互补看完整启动管线）。
 * HMR 走 full reload（不引框架，原生 DOM，强制整页重载简化心智模型）。
 */
// `LOCAL_ORIGIN`〔`设计/05 §8` 步 2〕：本机那个 origin 的**唯一住址**（Rust 侧是
// `origin::LOCAL`，三处由 `origin_tests.rs::the_sentinel_agrees_with_the_two_existing_homes` 钉着）。
import { isRemoteOrigin, LOCAL_ORIGIN } from "./ipc/origin";
import { emit } from "@tauri-apps/api/event";
import { commands } from "./ipc/commands";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { bindEvents } from "./events";
import { TabManager } from "./tabs";
import { paintLiveCards } from "./live-card-view";
import { mountTabBarResizer } from "./tab-bar-width";
import { terminalFrontCommand } from "./terminal-front-command";
import { loadTheme } from "./theme";
import { SETTINGS_APPLIED_EVENT } from "./settings";
import { RESYNC_DONE_EVENT } from "./settings/events";
import { listen } from "@tauri-apps/api/event";
import { HistoryView } from "./views/history";
import { SessionViewer } from "./views/session-viewer"; // F77：点 agent 看记录复用只读会话查看器
import { PanoramaView } from "./views/panorama";
import { InboxView } from "./views/inbox-view";
import { CcBusView } from "./views/cc-bus-view";
import { GridMonitorView } from "./views/grid-monitor";
import { CommandBarView, type Command } from "./views/command-bar";
import { UsageHud } from "./usage-hud";
import { recordFileWiring } from "./record-file-notice";
import { bindErrorToast, showActionFailureToast } from "./error-toast";
import { bindRemoteHealthToast } from "./remote-health";
// F83（#39）：顶栏 SFTP 入口——按远端主机数 0/1/N 分支打开现有 SFTP 模态。
import { openFileWindow } from "./file-window";
import { readRemoteConfig, sftpEligibleHosts, hostKey } from "./remote-config";
// N-F3：主窗口那一条「还差什么」指路 —— 那张清单此前只在设置面板 → 远端那一节渲染，
// 刚装完没打开过设置的人一个字都看不到。两个维度两个值，见 first-run-hint.ts 头注。
import { FirstRunHint } from "./first-run-hint";
import { LOCAL_MACHINE_KEY, readStatus } from "./settings/machine-status";
import { hostOs } from "./settings/host-os";
import { createUnknownKeysBar } from "./settings/unknown-keys-notice";
import { openSettingsWindow } from "./settings/open-settings"; // ST1：点「设置」有反馈（不 import 设置面板）
import { collectAccountRows, createEventRefresher } from "./session-accounts-poll";
import { lastAccounts } from "./history-reads";
import { TasksPanel } from "./tasks-panel";
import { AgentsPanel } from "./agents-panel";
import { getBehavior, setBehavior } from "./behavior";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { installGlobalClickDelegation } from "./entry-render-common";
import { AccountChip } from "./account-chip";
import { buildAccountCommands } from "./account-commands";
import type { FrontendReadyPayload } from "./generated/FrontendReadyPayload";
import { currentAccountForBadge } from "./accounts";
import { fetchSessionAccounts, fetchAccounts } from "./account-reads";
import { resolvePendingLocalLaunches } from "./local-launch-backfill";
import { copyText } from "./copy-table";
import { appStore } from "./app-store";
import { OverlayRouter } from "./overlay-router";

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
// 原先靠「三个窗口都加载 main.ts」顺带拿到（`设计/01 §1.2` 三入口拆分）。

window.addEventListener("DOMContentLoaded", async () => {
  window.__ccmPerf.domContentLoaded = performance.now();
  console.info(
    `[perf] DOMContentLoaded @ ${window.__ccmPerf.domContentLoaded.toFixed(0)}ms (since navigation start)`,
  );
  // 主题尽早应用，避免渲染抖动
  await loadTheme();
  window.__ccmPerf.themeLoaded = performance.now();

  // 〔三入口拆分 · `设计/01 §1.2`〕独立只读窗口与独立设置窗口**不再经过这里**：
  // 它们各有自己的 html 与入口模块（`viewer.html → src/entry-viewer.ts`、
  // `settings.html → src/entry-settings.ts`），模块图里根本没有本文件。
  // 原先这里按 `?viewer=` / `?settings=1` 分叉，代价是两个精简窗也要加载主窗口的整份包。

  const tabBar = document.getElementById("tab-bar");
  const streamRoot = document.getElementById("message-stream");
  const status = document.getElementById("status-bar");

  if (!tabBar || !streamRoot || !status) {
    console.error("layout containers missing");
    return;
  }

  // 〔B2 · 条 66〕这里原来在启动时把 config.json 里的后端策略**推**给 Rust。
  //   那个值搬到了后端所在那台机器上（`设计/01 §3.3b`），由后端在决定那一刻现读 ⇒
  //   **没有东西要推了**，这一步整条删掉（留着就是第二个真相源）。

  status.innerHTML = "";
  const statusMsg = document.createElement("span");
  statusMsg.className = "status-msg";
  statusMsg.textContent = copyText("main.status.waiting");
  status.appendChild(statusMsg);
  const statusCount = document.createElement("span");
  statusCount.className = "status-count";
  statusCount.textContent = copyText("main.status.count", { live: 0 });
  status.appendChild(statusCount);

  // issue #11: Task 面板的 summary chip 嵌入 status bar 右侧（活跃数右边）；
  // popover 浮层挂到 #app（fixed bottom 浮出）。两个元素由同一 TasksPanel 单例管。
  const tasksPanel = new TasksPanel();
  status.appendChild(tasksPanel.summaryElement);
  document.getElementById("app")?.appendChild(tasksPanel.popoverElement);

  // issue #23: Agents 面板（subagent 列表 + 各自状态灯），同形态挂在 tasks chip 旁
  const agentsPanel = new AgentsPanel();
  status.appendChild(agentsPanel.summaryElement);
  document.getElementById("app")?.appendChild(agentsPanel.popoverElement);

  // F88b（#52）：context% HUD chip——活跃会话「最新一轮 prompt token ÷ 模型上限」实时占用。
  // 挂 agents chip 旁；TabManager.onActiveUsageChanged 喂数据。**只读 chip，不可点** ——
  // 它当初点开的那个跨会话聚合视图（`views/usage-view.ts`）已随 `设计/50` 退役。
  const usageHud = new UsageHud();
  status.appendChild(usageHud.summaryElement);

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.innerHTML = copyText("main.empty.noSessions");
  streamRoot.appendChild(empty);

  // Batch5-F19：上次所在 tab 是远端会话时，等它的 remote-session-added 到达再切
  // （应用一次即清）。带 30s 启动窗口期限（审计 R2）：SSH 慢连/重连可达分钟级，
  // 用户此时早已在工作，迟到的宣告不该抢焦点。
  let pendingStartupActive: string | null = null;
  const startupActiveDeadline = Date.now() + 30_000;

  const tabs = new TabManager(
    tabBar,
    streamRoot,
    ({ total, live }) => {
      statusCount.textContent = copyText("main.status.count", { live });
      statusMsg.textContent =
        live > 0 ? copyText("main.status.watching") : copyText("main.status.waiting");
      empty.style.display = total > 0 ? "none" : "";
    },
    tasksPanel,
    agentsPanel,
  );
  // 〔TAP · V124〕活卡的画法（带 `.module.css`）只由主窗口入口装进来 —— 理由见 `live-card-view.ts` 头注。
  tabs.setLivePainter(paintLiveCards);
  // P7a-3（#61）：启动时拉一次标签页集合（住 `config.json`，不是 localStorage —— 见
  // `tab-collections.ts` 头注：集合名是用户手写的真相，必须活过一次清缓存）。
  void tabs.loadCollections();
  // 〔步 17·C〕顺序也要拉回来。排在集合之后：两者都只改内存 + 重画，互不依赖，
  // 但集合决定分组容器、顺序决定容器内次序 —— 先有容器再排，少一次无谓重画。
  //
  // 〔步 17·B〕固定表排在顺序之前，串着跑（不是并排 `void`）。
  // ⚠ **2026-09-21 订正：这个次序不再是承重的。** 原先这儿写着「**必须**排在顺序之前」，
  //   理由是「`loadOrder` 按今天真的存在的 sid 过滤，固定的 tab 得先造出来，
  //   否则位置会被当成已删会话整条摘掉」——**那道读时过滤本身就是步 17·C 那个 bug**
  //   （`tabs.ts` 的 `loadOrder` 头注记着成因与现打）。顺序现在是一份**留着的意图**，
  //   tab 陆续到达时会被反复应用 ⇒ 谁先谁后都排得回来。次序留着是因为它无害且省一次重画。
  // 🔴 **也正因为如此，这一行不再是「顺序能不能活过重启」的承重点** ——
  //   那条性质今天由 `tests/tabs.vitest.ts`「步 17·C 顺序落盘：读回来那一半」按
  //   **真实启动时序**（loadPinned → loadOrder → 会话陆续到）盯着，9 格全是相等断言。
  // ⚠ `finally` 不是 `then`：`loadPinned` 失败也得让顺序照常回来（互不为前提）。
  void tabs.loadPinned().finally(() => void tabs.loadOrder());

  // A3：状态栏「当前账号」chip（多账号 cc-acct-iso）。绑第一台可用远端的默认账号；
  // 未连远端 / 未启用多账号 各自安静降级（不报错）。点击弹选单切默认账号。
  // **构造在 `refreshSessionAccounts` 定义之前**（D 审计）：`onDefaultChanged` 回调间接调用
  // `refreshSessionAccounts`（下方 `const` 声明，函数体里会写 `appStore.sessionAccounts`）——
  // 回调本身只在用户切号时才真正执行，届时 `refreshSessionAccounts` 早已初始化完毕，不会踩
  // TDZ；但这条"届时早已初始化"的保证依赖"这中间没有 await 会提前执行到回调"这条隐式不变量，
  // 谁在中间插一个真会被调用的 await 就有踩 TDZ 的风险，需要留意。
  const accountChip = new AccountChip({
    openSettings: () => void openSettingsWindow(),
    // 切号后立刻重算一次：currentByOrigin 只由下面那个事件驱动的刷新器喂，不主动刷的话
    // chip 已显示新账号，而对齐动作会把会话打回**刚被切走**的旧账号（D 审计重-5）。
    onDefaultChanged: () => accountsRefresher.request(true),
  });
  status.appendChild(accountChip.element);
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
      if (!cfg.enabled) {
        // 〔GAP1 · `设计/01 §1.5`〕账号快照整份换进 store，tab 栏徽章订阅它（同一拍应用）。
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
      const { rows, emailByName, readyOrigins, currentByOrigin } = await collectAccountRows(
        cfg.hosts,
        { fetchSessionAccounts, fetchAccounts, currentAccountForBadge },
        undefined,
        forceAccounts,
      );
      // A4：sid → lastAccount（源②）。本机 history-metadata 读一次（远端会话的 lastAccount 也
      // 由 cc-monitor 记在本机），live 探测不到时徽章兜底显「上次用本工具起」。失败 → 空表降级。
      let lastByS = new Map<string, string>();
      try {
        // 〔C4d〕问本机常驻后端（注解的读写者）。
        const raw = await lastAccounts();
        lastByS = new Map(Object.entries(raw));
      } catch (e) {
        console.warn("history-last-accounts failed:", e);
      }
      if (mySeq !== refreshSeq) return; // I4：晚到的旧快照不覆盖新快照
      appStore.sessionAccounts.set({ rows, emailByName, lastByS, readyOrigins, currentByOrigin });
    } catch (e) {
      console.warn("refreshSessionAccounts failed:", e);
    }
  };
  // 🔴 〔`C1` · 2026-09-24〕那个 10 秒轮询**删了**（理由整段在 `session-accounts-poll.ts` 头注）：
  // 两条查询搬上了已有的长连接，而「会话 ↔ 账号」只在会话起停时变 —— 那本来就有事件。
  // ⇒ 刷新改由事件驱动，零定时器：
  //   · 某台的长连接握手完成（启动 / 重连）或那台账号清单变了 ⇒ 强制刷账号清单，
  //     账号 chip 也在这一刻重取（在那之前问只会拿到「没有控制通道」）—— 〔DL1〕经通道订的 `accounts-changed`；
  //   · `remote-session-added` / `session-ended`：会话起停；
  //   · 本 UI 切号：上面 `onDefaultChanged`。
  const accountsRefresher = createEventRefresher(refreshSessionAccounts);
  accountsRefresher.request();
  // 〔DL1〕「某台长连接握手完成 / 那台账号清单变了」⇒ 强制刷账号清单 ＋ chip。原先听裸事件 `remote-backend-ready`；
  //   今天经通道订每台的 `accounts-changed`（下面 `bindEvents` 的 `accounts` ＋ `onAccountsChanged`）。
  const onAccountsChanged = (): void => {
    accountsRefresher.request(true);
    void accountChip.refresh(true);
  };
  void listen("remote-session-added", () => accountsRefresher.request());
  void listen("session-ended", () => accountsRefresher.request());

  // Batch5-F19（G 验收）：用户手动切过 tab 后，迟到的远端宣告不再补切抢焦点
  tabs.onManualSwitch = () => {
    pendingStartupActive = null;
  };
  // F88b：当前 tab 那一格变了 → 刷新 HUD context% chip（〔STC〕后端的会话事实到了 / switchTo 切会话 / 要不到 ⇒ chip 说原因）。
  // 〔GAP1 · `设计/01 §1.5`〕订阅 store（原先是两个点对点回调）。
  tabs.active.subscribe((a) => {
    usageHud.setActive(a.model, a.promptTokens);
    usageHud.setUnavailable(a.unavailable);
  });

  // F77（#53）：点 agents 面板某行 → load_subagent 拿子 agent jsonl 路径 → SessionViewer 只读展示该
  // agent 的记录。★ P7c-1（08-12）起**远端会话也支持**（同 subagent 卡片：origin 传下去，
  // backend 的 `--list-subagents` 只列候选，挑选留后端本侧）。
  let agentViewer: SessionViewer | null = null;
  let agentViewerMount: HTMLElement | null = null;
  const closeAgentViewer = (): void => {
    if (agentViewer) {
      agentViewer.dispose();
      agentViewer = null;
    }
    if (agentViewerMount) {
      agentViewerMount.remove();
      agentViewerMount = null;
    }
  };
  agentsPanel.onAgentOpen = (entry) => {
    const actx = tabs.getActiveSubagentContext();
    if (!actx) return;
    void (async () => {
      try {
        // C04d 批 5b：这里原来写 `invoke<{ path: string }>` —— **同一个命令在全仓有两种 TS 类型**
        // （`cards/subagent.ts` 用完整的 `SubagentLoadResult`，这里只声明 `path`）。
        // 包装层收敛成一处后这类分叉结构性消失：本处只读 `.path`，用完整类型完全够。
        const result = await commands.load_subagent({
          parentJsonlPath: actx.parentPath,
          description: entry.desc, // ★ 用 trim 后的原始 desc（非展示 label）——load_subagent 精确匹配
          toolUseTimestamp: entry.timestamp,
          // P7c-1：远端会话也能展开了（backend `--list-subagents` 只列候选，挑选留后端本侧）。
          // 〔C4a〕`actx.origin` 本机就是 `LOCAL_ORIGIN`（前端与线上同一个表示，不再换）。
          origin: actx.origin,
        });
        closeAgentViewer(); // 关掉上一个（单例语义）
        agentViewerMount = document.createElement("div");
        agentViewerMount.className = "agent-records-viewer-mount"; // fixed 全屏 + 高 z-index
        agentViewerMount.tabIndex = -1; // 可聚焦，让 Esc keydown 有落点（子元素 keydown 也冒泡到这）
        // Esc 关闭（SessionViewer 无自带 Esc；挂载壳上接一个，同返回按钮）。
        agentViewerMount.addEventListener("keydown", (e) => {
          if (e.key === "Escape") closeAgentViewer();
        });
        agentViewer = new SessionViewer(() => closeAgentViewer());
        agentViewerMount.appendChild(agentViewer.element);
        document.body.appendChild(agentViewerMount);
        // suppressBranch：子 agent 记录不是可分支会话，关掉「建分支」按钮。
        void agentViewer.load({
          jsonlPath: result.path,
          displayTitle: entry.label,
          // 〔C4a〕子 agent 的文件在父会话那台机器上。上一版这里**没传** origin，
          // 而查看器把「没说」当本机 ⇒ 远端子 agent 的记录被拿去本机读（读不到）。
          origin: actx.origin,
          suppressBranch: true,
        });
        agentViewerMount.focus?.(); // 让 Esc keydown 能落到挂载壳
      } catch (e) {
        showActionFailureToast(copyText("main.subagent.loadFailed"), String(e));
      }
    })();
  };

  // v2.4 issue #2：拉一次 behavior toggle 初值喂给 TabManager。
  // 设置面板改了之后会再调 applyBehavior 同步。
  void getBehavior().then((b) => tabs.applyBehavior(b));

  // F82a（#56+#47）：设置改由**独立窗口**承载（SS-3 终态），主窗口不再内嵌设置浮层。
  // 设置窗口保存 / 行为 toggle 后广播 SETTINGS_APPLIED_EVENT → 主窗口重读并应用主题 + 行为
  // （跨 OS 窗口回调够不到，用广播代替原 onBehaviorChange 直连）。loadTheme 内部 applyTheme。
  // F84b-fix(batch18)：状态栏命令 chip 的键位刷新钩子——改键热应用后同步刷新 chip 显示的 kbd
  //（原实现只在 bootstrap 算一次，改键后 chip 教死键；chip 创建时把重建函数赋进来）。
  let refreshCmdkChord: () => void = () => {};
  // N-F3：同一个钩子形状给「还差什么」那条指路用。**它必须在这里挂**——
  // 用户去设置里补齐了一格，广播过来这一拍就是「状态维」该重算的那一刻；
  // 不挂的话「补齐了 ⇒ 指路消失」要等下次启动才兑现（那就成了一句半真的话）。
  let refreshFirstRunHint: () => void = () => {};
  // 〔RESYNC · `99 §2.1` ㉟①〕设置窗「重新对齐」做完 ⇒ 标出那台上记录没了的固定条（不自动摘）。
  void listen<{ origin: string }>(RESYNC_DONE_EVENT, (e) => void tabs.flagPinsWithoutRecord(e.payload.origin));
  void listen(SETTINGS_APPLIED_EVENT, () => {
    void loadTheme(); // 主题：loadTheme 内部 applyTheme
    void getBehavior().then((b) => tabs.applyBehavior(b)); // 行为
    void getKeybindings().then((kb) => {
      dispatcher.applyOverrides(kb); // 键位：热应用主窗口 dispatcher
      refreshCmdkChord(); // 键位变 → 同步刷新命令 chip 的 kbd（兑现「改键即变」）
    });
    void accountChip.refresh(true); // A3：远端配置/默认账号可能变了，刷新账号 chip
    refreshFirstRunHint(); // N-F3：账本/远端列表可能变了 → 「还差什么」现算一遍
  });
  // Batch11-F33：竖直 tab 栏——右缘拖拽调宽。〔CFG1〕整段搬进 tab 栏自己的模块（`tab-bar-width.ts`），
  // 宽度经 `LS_KEYS` ＋ `safeGet/safeSet` 记忆（D §D5：原先住这里、直调 localStorage、键不在登记里）。
  mountTabBarResizer();

  const settingsTrigger = document.createElement("button");
  settingsTrigger.type = "button";
  settingsTrigger.className = "settings-trigger";
  settingsTrigger.title = copyText("main.topbar.settingsHint");
  settingsTrigger.setAttribute("aria-label", copyText("main.cmd.openSettings"));
  settingsTrigger.addEventListener("click", () => {
    void openSettingsWindow(settingsTrigger); // F82a：开独立设置窗口（非浮层）· ST1：点了有反馈
  });
  document.getElementById("app")?.appendChild(settingsTrigger);

  // 历史浏览器入口 —— 顶栏右侧，紧邻设置按钮左边
  // v2.5+: HistoryView 不再接管 streamRoot，自挂 body 作 fixed overlay
  // 〔FW1 · 第四波 4D · D-d〕记录文件不见了 / 被改过 ⇒ 那个 tab 顶上说一句（只在主窗口接，理由见 `record-file-notice.ts`）。
  const recordFile = recordFileWiring((sid) => tabs.streamElOf(sid));
  const historyView = new HistoryView();
  // 〔GAP1 · `设计/01 §1.5`〕overlay 开关只经路由（顶栏按钮 · 快捷键 · 命令面板同一口）。
  const overlays = new OverlayRouter();
  overlays.register("history", historyView);
  // 〔FW1 · 第四波 4D · D-e〕删会话前看活不活：条目自己那一格可能是列表拉下来那一刻的，tab 栏是此刻的。
  historyView.liveInTabs = (sid) => tabs.isSessionLive(sid);
  const historyTrigger = document.createElement("button");
  historyTrigger.type = "button";
  historyTrigger.className = "history-trigger";
  historyTrigger.title = copyText("main.topbar.historyHint");
  historyTrigger.setAttribute("aria-label", copyText("main.topbar.openHistory"));
  // 纯字符的时钟符号（U+25F7），避免 emoji 跨平台/字体差异
  historyTrigger.addEventListener("click", () => overlays.toggle("history"));
  document.getElementById("app")?.appendChild(historyTrigger);

  // Batch15-P2：代码全景入口 —— 顶栏右侧，紧邻历史按钮左边。自挂 body 作 fixed overlay
  // （照 HistoryView），对活跃**本地**会话的 cwd 建 code-picture 索引画代码库地图。
  const panoramaView = new PanoramaView(() => tabs.activeRepoInfo());
  // devbench F03b：收件箱 overlay —— 与 panorama 同一个 cwd 取法（活跃 tab）。
  const inboxView = new InboxView(() => tabs.activeRepoInfo());
  overlays.register("panorama", panoramaView);
  overlays.register("inbox", inboxView);
  // F70（护城河）：右键 tab「在全景高亮本会话改动」→ 切到该会话 → 打开全景 → 高亮它改过的
  // 节点。TabManager 不直接持有 PanoramaView，走注入回调（同 onManualSwitch 范式）。
  tabs.requestPanoramaHighlight = (sid) => {
    const info = tabs.touchedFilesFor(sid); // 远端/无 cwd 已被 getter 挡掉（返 null）
    if (!info) return;
    tabs.switchTo(sid); // 置活跃 → activeRepoInfo=该仓 → 全景加载该仓
    void (async () => {
      await overlays.show("panorama"); // 同仓复用；异仓重索引/加载
      await panoramaView.highlightSession(info.files);
    })();
  };
  const panoramaTrigger = document.createElement("button");
  panoramaTrigger.type = "button";
  panoramaTrigger.className = "panorama-trigger";
  panoramaTrigger.title = copyText("main.topbar.panoramaHint");
  panoramaTrigger.setAttribute("aria-label", copyText("main.cmd.openPanorama"));
  panoramaTrigger.addEventListener("click", () => overlays.toggle("panorama"));
  document.getElementById("app")?.appendChild(panoramaTrigger);

  // F91（#27）：多 agent 并排监控入口 —— 顶栏右侧一排（🗂 左边，right:168px）。跨机器只读
  // mission-control 状态板（一屏看所有会话实时状态，点卡片跳会话；只读——不派发/不驱动 agent）。
  const gridMonitorView = new GridMonitorView(tabs);
  overlays.register("grid", gridMonitorView);
  const gridTrigger = document.createElement("button");
  gridTrigger.type = "button";
  gridTrigger.className = "grid-monitor-trigger";
  gridTrigger.title = copyText("main.topbar.gridHint");
  gridTrigger.setAttribute("aria-label", copyText("main.cmd.openGrid"));
  gridTrigger.addEventListener("click", () => overlays.toggle("grid"));
  document.getElementById("app")?.appendChild(gridTrigger);

  // F83（#39）：顶栏远端文件入口 —— 设置搬独立窗后腾出的入口位。点击按远端主机数分支：
  // 0 台提示 / 1 台直开 / 多台选单（选单见 openSftpFromTopbar）。〔F7b〕终点是原生文件窗口（`file-window.ts`）。
  const sftpTrigger = document.createElement("button");
  sftpTrigger.type = "button";
  sftpTrigger.className = "sftp-trigger";
  sftpTrigger.title = copyText("main.topbar.filesHint");
  sftpTrigger.setAttribute("aria-label", copyText("main.cmd.openFiles"));
  sftpTrigger.addEventListener("click", () => void openSftpFromTopbar(sftpTrigger));
  document.getElementById("app")?.appendChild(sftpTrigger);

  // S6（settings-ia）：cc-bus 驾驶舱从设置里搬出来，成为顶层运营视图。
  // **刻意不加第 7 个顶栏图标** —— F84 加命令面板时就为「顶栏已拥挤」立过先例
  //（那次逐字写着「键位唯一入口…按钮延后」）。驾驶舱是低频运营视图，正是面板服务的对象。
  // 理由完整版见 `views/cc-bus-view.ts` 头注。
  const ccBusView = new CcBusView();
  overlays.register("cc-bus", ccBusView);

  // F84（#57）：键盘命令栏（Ctrl+K）。只读命令面板——组装既有 view/dispatcher 目标 + F91
  // snapshotSessions() 的「切到会话…」。写/驱动动作（resume/kill/delete）首刀排除（守北极星）。
  // 键位唯一入口（palette 惯例，顶栏已拥挤，按钮延后）。commandBar 放 sftpTrigger 之后建，
  // 使 buildCommands 引用的 sftpTrigger 已声明。
  const buildCommands = (): Command[] => {
    // 命令项右侧显示对应快捷键（教学式发现，业务二审 gap#3）——取该 action 当前生效 chord 的友好名。
    const chordHint = (
      id: Parameters<typeof dispatcher.effectiveChord>[0],
    ): string | undefined => {
      const raw = dispatcher.effectiveChord(id);
      return raw ? KeybindingDispatcher.prettyChord(raw) : undefined;
    };
    const cmds: Command[] = [
      { id: "open-history", title: copyText("main.cmd.openHistory"), keywords: copyText("main.cmd.historyKeywords"), hint: chordHint("app.toggle-history"), run: () => overlays.open("history") },
      { id: "open-panorama", title: copyText("main.cmd.openPanorama"), keywords: copyText("main.cmd.panoramaKeywords"), hint: chordHint("app.toggle-panorama"), run: () => overlays.open("panorama") },
      // devbench F03b：**开 overlay 属命令面板首刀允许的只读动作**（写发生在 overlay 内的保存上）。
      { id: "open-inbox", title: copyText("main.cmd.openInbox"), keywords: copyText("main.cmd.inboxKeywords"), run: () => overlays.open("inbox") },
      { id: "open-cc-bus", title: copyText("main.cmd.openCcBus"), keywords: copyText("main.cmd.ccBusKeywords"), run: () => overlays.open("cc-bus") },
      { id: "open-grid", title: copyText("main.cmd.openGrid"), keywords: copyText("main.cmd.gridKeywords"), run: () => overlays.open("grid") },
      { id: "open-settings", title: copyText("main.cmd.openSettings"), keywords: copyText("main.cmd.settingsKeywords"), hint: chordHint("app.open-settings"), run: () => void openSettingsWindow() },
      { id: "open-sftp", title: copyText("main.cmd.openFiles"), keywords: copyText("main.cmd.filesKeywords"), run: () => void openSftpFromTopbar(sftpTrigger) },
      { id: "win-minimize", title: copyText("main.cmd.minimize"), keywords: copyText("main.cmd.minimizeKeywords"), hint: chordHint("app.minimize"), run: () => void getCurrentWindow().minimize() },
      { id: "win-fullscreen", title: copyText("main.cmd.fullscreen"), keywords: copyText("main.cmd.fullscreenKeywords"), hint: chordHint("app.toggle-fullscreen"), run: () => { const w = getCurrentWindow(); void w.isFullscreen().then((f) => w.setFullscreen(!f)).catch((e) => console.warn("toggle-fullscreen failed:", e)); } },
      // 〔U2〕↗ 那一项只在 ↗ 真能用的机器上列出来（非 Windows 不列；门与 tab 上那颗按钮是同一道，见 `terminal-front-command.ts`）。
      ...terminalFrontCommand({ id: "term-front", title: copyText("main.cmd.terminalFront"), keywords: copyText("main.cmd.terminalFrontKeywords"), hint: chordHint("terminal.bring-front"), run: () => tabs.bringActiveTerminalToFront() }),
      { id: "toggle-tasks", title: copyText("main.cmd.tasks"), keywords: copyText("main.cmd.tasksKeywords"), hint: chordHint("panel.toggle-tasks"), run: () => tasksPanel.toggle() },
      { id: "tab-next", title: copyText("main.cmd.tabNext"), keywords: copyText("main.cmd.tabNextKeywords"), hint: chordHint("tab.next"), run: () => tabs.cycleActive(1) },
      { id: "tab-prev", title: copyText("main.cmd.tabPrev"), keywords: copyText("main.cmd.tabPrevKeywords"), hint: chordHint("tab.prev"), run: () => tabs.cycleActive(-1) },
    ];
    // A3/U8：账号命令。构造逻辑在 account-commands.ts（纯函数，可测——原先长在这个闭包里，
    // "命令何时出现"完全测不到，把判定改成恒 true 也不会红）。这里只喂快照与动作。
    cmds.push(
      ...buildAccountCommands({
        snapshot: accountChip.snapshotReady(),
        chordHint: (id) => chordHint(id as Parameters<typeof chordHint>[0]),
        setCurrent: (name) => void accountChip.applyDefaultByName(name),
        openSettings: () => void openSettingsWindow(),
      }),
    );
    // 切到会话（来自 F91 只读投影 snapshotSessions）
    for (const s of tabs.snapshotSessions()) {
      const originTag = isRemoteOrigin(s.origin) ? `[${s.origin}] ` : "";
      cmds.push({
        id: `switch-${s.sessionId}`,
        title: copyText("main.cmd.switchSession", { originTag, title: s.title }),
        keywords: copyText("main.cmd.switchSessionKeywords", { cwd: s.cwd ?? "", machine: isRemoteOrigin(s.origin) ? s.origin : "" }),
        run: () => tabs.switchTo(s.sessionId),
      });
    }
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
    const icon = document.createElement("span");
    icon.className = "status-cmdk-icon";
    icon.setAttribute("aria-hidden", "true"); // F84b-fix：图标纯装饰，屏读器别念 U+2328 字形名
    cmdkHint.appendChild(icon);
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
    status.appendChild(cmdkHint); // status-msg flex:1 把它顶到最右
    // F84b-fix：首运行一次性微高亮（非模态），帮新用户注意到这个克制的角落入口；见过即不再。
    if (!safeGet(LS_KEYS.cmdkHintSeen)) {
      cmdkHint.classList.add("first-run");
      safeSet(LS_KEYS.cmdkHintSeen, "1");
    }
  }

  // N-F3：「还差什么」那条指路 —— 放在命令 chip 旁边（同一条状态栏、同一种克制），
  // 但**语义与它相反**：命令 chip 是一次性知识（见过即不再），这一条是**状态**，
  // 补齐了自己就消失、又缺了自己就回来。两者的值一个都不共用，见 first-run-hint.ts 头注。
  //
  // ⚠ 只在主窗口挂：独立 viewer 窗与设置窗各走自己的入口（`entry-viewer.ts` / `entry-settings.ts`），不经过本文件。
  // 🔴 〔`P12` 收尾 2026-09-21〕**「配置里有个键没生效」这句话要在主窗口也说得出来。**
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
  //   （`tests/settings/unknown-keys-notice.vitest.ts`）；主窗口这一侧**没有同等的**
  //   ——`main.ts` 今天没有 DOM 判据台架，本处只由源码扫描盯着。
  //   而 `P12` 的死值验刀 1 现打证明过：**源码扫描看不见「挂没挂上」**
  //   （组件在、函数在、只是不 appendChild ⇒ 扫描照样绿）。
  //   ⇒ 这一挂点的失效形状今天**判不了**，缺的是 `main.ts` 的 DOM 台架。**登记，不假装。**
  status.appendChild(createUnknownKeysBar());

  {
    // `origins` 要读远端配置（异步），所以先拿一份快照，
    // 由 `reload()` 刷新；`FirstRunHint` 自己不碰 IO（照 readiness.ts 的注入范式）。
    let origins: string[] = [LOCAL_MACHINE_KEY];
    const firstRunHint = new FirstRunHint(status, {
      origins: () => origins,
      statusOf: readStatus,
      hostOs,
      // 「点得进那张清单」= 打开设置窗口（清单住在它的「远端」那一节）。
      // ⚠ 今天**只能到窗口这一格**：`open_settings_window` 不收参数，
      //   直达那一节要动 `src/bridge` 与 `settings/panel.ts`，都在本件写区外。
      openList: () => void openSettingsWindow(),
    });
    const reload = async (): Promise<void> => {
      try {
        const cfg = await readRemoteConfig();
        origins = [LOCAL_MACHINE_KEY, ...cfg.hosts.map(hostKey)];
      } catch (e) {
        // 读不到远端配置不该让这条提示消失 —— 本机那几格照样算得出来。
        console.warn(`[N-F3] 远端配置读失败，只按本机算「还差什么」：${String(e)}`);
      }
      firstRunHint.refresh();
    };
    refreshFirstRunHint = () => void reload();
    void reload();
  }

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
  dispatcher.bind("tab.pop-out", () => tabs.openActiveInNewWindow());
  dispatcher.bind("session.find", () => tabs.openFind()); // 〔SE2〕会话内查找（大纲同一块面板）
  dispatcher.bind("terminal.bring-front", () => tabs.bringActiveTerminalToFront());
  dispatcher.bind("app.open-settings", () => void openSettingsWindow()); // F82a：开独立设置窗口
  dispatcher.bind("app.toggle-history", () => overlays.toggle("history"));
  dispatcher.bind("app.toggle-panorama", () => overlays.toggle("panorama"));
  dispatcher.bind("app.open-command-bar", () => commandBar.toggle()); // F84（#57）Ctrl+K 命令栏
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.bind("app.toggle-fullscreen", () => {
    const w = getCurrentWindow();
    void w
      .isFullscreen()
      .then((f) => w.setFullscreen(!f))
      .catch((e) => console.warn("toggle-fullscreen failed:", e));
  });
  dispatcher.bind("panel.toggle-tasks", () => tasksPanel.toggle());
  dispatcher.bind("behavior.toggle-auto-follow", () => {
    void (async () => {
      const cur = await getBehavior();
      const next = { ...cur, autoFollowUserActive: !cur.autoFollowUserActive };
      await setBehavior(next);
      tabs.applyBehavior(next);
    })();
  });
  dispatcher.bind("behavior.toggle-bring-monitor", () => {
    void (async () => {
      const cur = await getBehavior();
      const next = {
        ...cur,
        bringMonitorToFrontOnUserActive: !cur.bringMonitorToFrontOnUserActive,
      };
      await setBehavior(next);
      tabs.applyBehavior(next);
    })();
  });

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

  // 〔CF2 · 第四波 4B〕会话内容经通道 `subscribe` 来：每台机器一条 `session-lines`。哪几台由后端注册表说了算
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
      recordFile.afterLine(e.session_id); // 〔FW1 · D-d〕又来了一行 ⇒ 「记录文件不见了」那一句收掉
    },
    onSessionEnded: (sessionId) => tabs.archiveTab(sessionId),
    // audit-fixes F03.2：远端 claude 退但 tmux 会话仍在 → 灰灯（idle-tmux 第三态，非归档）。
    onSessionIdle: (sessionId) => tabs.markTmuxIdle(sessionId),
    // 〔U4b · 第四波〕活会话的容器（G3）· 某台机器的活会话清单报完了（说不清 → 已结束）。
    onSessionContainer: (sessionId, container) => tabs.noteContainer(sessionId, container),
    onOriginSessionsListed: (origin) => tabs.markOriginSeen(origin),
    // 〔TAP · V124〕中转抄出来的 SSE 事件（会话流 `session-tap`）→ 活卡（jsonl 到了整轮覆盖）；那台看不见了 ⇒ 活卡全撤。
    onSessionTap: (e) => tabs.onSessionTap(e),
    onSessionTapLost: (origin) => tabs.dropLiveCards(origin),
    // 〔DL1〕那台的长连接又通了 / 那台账号清单变了 ⇒ 强制刷账号清单 ＋ chip（`accounts-changed` 流）。
    onAccountsChanged,
    // 〔FW1 · 第四波 4D · D-d〕记录文件不见了 / 被改过已从头重读 ⇒ 那个 tab 顶上说一句。
    onSessionFileNotice: (sessionId, change) => {
      tabs.onRecordFileReread(sessionId, change); // 〔RENDER2〕从头重读 ⇒ tab 整份重来（先重来、再在新的流容器上说那一句）
      recordFile.onSessionFileNotice(sessionId, change);
    },
    // 〔GP1 · 第四波〕那台机器看不见了 ⇒ 说不清（不是已结束）。
    onSessionUnseen: (sessionId) => tabs.markUnseen(sessionId),
    // 会话复活（resume）：后端 liveness 门控后才发，复活已归档的本地 Tab，免 F5。
    // Batch7-F24：无 Tab（= 运行中途**新出现**的本地会话）→ 建骨架——bg 会话必须
    // 从这条通道拿 kind/name（首行 onLine→ensureTab 不带 kind，会建成无 ⚙ 普通 tab）。
    onSessionStarted: (sessionId, meta) => {
      if (tabs.hasTab(sessionId)) {
        tabs.reviveTab(sessionId);
      } else {
        tabs.createSkeletonTab(sessionId, meta.cwd || null, LOCAL_ORIGIN, meta.kind, meta.name);
      }
      // ★★ `K-P5h` `KP5HD3`：**「过一会儿再问」搭的是这条已有的事件，不是一个新定时器。**
      //    身份 token 在会话起来**之前**就铸好了，而 `--session-accounts` 要进程已经在跑
      //    才读得到 ⇒ 回填必然要等。等的办法有两种，这里选的是「内核一有事就通知」那种：
      //    `sessions/<PID>.json` 一变，`lib.rs` 的 `session-changes-emitter` 就发这条事件 ——
      //    **一条新会话出生正是它响的时刻**，也正是回填该问的时刻。
      //    🔴 不排定时器：本项目有一条已交付的性质是「判活不靠定时轮询」，
      //       在这里起一个新的周期唤醒就是开倒车（`polling_registry` 那两张表在管这件事）。
      //    ⚠ 不 `await`：回填是补记账，失败也不该影响建 tab 这条主路（它自己吞异常）。
      void resolvePendingLocalLaunches();
      // 〔MIG-1〕本机骨架也在就绪点才到（与远端同一条路）⇒ 上次所在 tab 是本机会话时同样在这里补切。
      if (pendingStartupActive === sessionId) {
        pendingStartupActive = null;
        if (Date.now() < startupActiveDeadline) {
          tabs.switchTo(sessionId, "auto");
        }
      }
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
    onTasksUpdate: (e) => tabs.updateTasks(e.sessionId, e.tasks),
    // issue #23: 会话红绿灯（busy=绿 / idle·shell=红 / waiting=黄）
    onSessionActivity: (e) =>
      tabs.updateActivity(e.session_id, e.status, e.waiting_for),
    // Batch5-F18：远端会话宣告 → 骨架 Tab。Batch7-F24：p1e backend 附 cwd/kind/name
    // ——骨架标题即时完整（bg → ⚙ ＋ 任务名；〔BG1〕不再挂宿主排成树）；旧后端缺省照旧 sid 前缀。
    onRemoteSessionAdded: (sessionId, origin, meta) => {
      tabs.createSkeletonTab(
        sessionId,
        meta.cwd || null,
        origin,
        meta.kind,
        meta.name,
        meta.attachable, // E73
      );
      // Batch5-F19：上次所在 tab 是远端会话时在此补切（应用一次即清；超过
      // 30s 启动窗口则放弃——迟到宣告不抢焦点，replay 优先级不受影响）
      if (pendingStartupActive === sessionId) {
        pendingStartupActive = null;
        if (Date.now() < startupActiveDeadline) {
          tabs.switchTo(sessionId, "auto");
        }
      }
    },
    // 〔CF2〕那台机器的会话流丢了几格 ⇒ 那台的每个 tab 按行号补（`TabManager.onStreamGap`）。
    onStreamGap: (origin) => tabs.onStreamGap(origin),
  }, {
    streams: machines.map((origin) => ({ origin, kind: "session-lines" })),
    // 〔HOST · V139〕每台的中转都住那台的常驻后端 ⇒ 每台都订 tap（与会话行同一份机器清单）。
    taps: machines,
    // 〔DL1〕每台一条 `accounts-changed`（替掉裸事件 `remote-backend-ready`）。
    accounts: machines,
  });
  // 〔DL1 · `设计/01 §2.2`〕账号那一格经通道订（每台一条 `accounts-changed`，上面 `bindEvents` 的 `accounts`）。
  //   订阅登记之前那一窗里连上的不会有 `seen`（句柄只在状态变时说）⇒ `bindEvents` 返回（订阅都登记好了）之后补刷一次 ——
  //   与「独立窗口的订阅本身就是它的就绪点」同一个道理。
  onAccountsChanged();

  // v2.0.0 (issue #4)：后端 ERROR 级别 tracing → 右下角红色 toast
  bindErrorToast();
  // issue #32 (SS-F)：远端健康事件（拥塞丢行 / 版本不符）→ 右下角 info toast
  bindRemoteHealthToast();

  // Batch5-F19（G 验收 B-1）：**先读记忆再建骨架**——骨架（〔MIG-1〕本机远端一样，都是会话流里的 `live` 成品，
  //   在下面 `frontend-ready` 的就绪点才到）一建出来就可能经 switchTo 写回 localStorage，读晚了就把记忆覆写掉。
  // 启动 active = 上次所在 tab：此刻还没有骨架 ⇒ 挂 pending，等它的 `live` 到达时补切（本机 `onSessionStarted` ·
  //   远端 `onRemoteSessionAdded` 两处同一段；应用一次即清，30 s 之后不再抢焦点）。
  //   〔从前本机骨架先经 `list_active_sessions`〔散文墓碑〕在这里建好、建时抑制写回（`persistLastActive = false … true`）；
  //    骨架挪到就绪点之后那一对抑制没有要罩住的东西了，删了。〕
  const lastActive = safeGet(LS_KEYS.lastActiveSid);
  pendingStartupActive = lastActive && tabs.hasTab(lastActive) ? null : lastActive;
  if (lastActive && tabs.hasTab(lastActive)) tabs.switchTo(lastActive, "auto");

  // 通知后端可以发了 —— 缓冲的 line 会被 flush 过来。payload 带上次所在 tab
  // （Batch5-F19）：后端 replay 按 session 分组、该 tab 的内容块先发。
  window.__ccmPerf.frontendReadyEmit = performance.now();
  console.info(
    `[perf] emit frontend-ready @ ${window.__ccmPerf.frontendReadyEmit.toFixed(0)}ms`,
  );
  // C02：`frontend-ready` 是**唯一方向相反**的 payload（前端 emit、Rust 收），
  // 而 TS 侧**从来没有过这个类型**——这是净新增能力，不是替换。
  // 生成物来自 `bridge.rs::FrontendReadyPayload`（只有 `Deserialize`；实测 `ts-rs` 照样生成）。
  // 它用的是**字段级** `#[serde(rename = "prioritySid")]`，不是容器级 `rename_all`。
  const frontendReady: FrontendReadyPayload = { prioritySid: lastActive };
  void emit("frontend-ready", frontendReady);

  // 〔MIG-1〕红绿灯的初始值随 `live` 成品一起在就绪点交（`activity` 那一格），不再另拉快照（`list_session_activity`〔散文墓碑〕）。

  // 注：maximize / 全屏后内容错位的修复在 Rust 侧（src/bridge/src/lib.rs on_window_event：
  // 去抖后微调 webview 尺寸强制 wry 重新 put_Bounds，把 WebView2 合成层钉回左上角）。
  // 旧版（v2.13.0）在这里做的 onResized + scrollTop 微滚动够不着 DOM 之下的合成层偏移，已删。
});

// ============ F83（#39）：顶栏 SFTP 入口 ============

/** 多台远端时的选主机浮层（body-level，单例）。照 history F96 菜单关闭范式。 */
let sftpHostPicker: HTMLElement | null = null;
let sftpHostPickerClose: ((ev: Event) => void) | null = null;

function closeSftpHostPicker(): void {
  if (sftpHostPickerClose) {
    document.removeEventListener("pointerdown", sftpHostPickerClose);
    document.removeEventListener("keydown", sftpHostPickerClose);
    sftpHostPickerClose = null;
  }
  if (sftpHostPicker) {
    sftpHostPicker.remove();
    sftpHostPicker = null;
  }
}

/** 顶栏 SFTP 入口点击：0 台提示 / 1 台直开 / 多台选单。 */
async function openSftpFromTopbar(anchor: HTMLElement): Promise<void> {
  const cfg = await readRemoteConfig();
  const hosts = sftpEligibleHosts(cfg);
  if (hosts.length === 0) {
    // 这是引导提示不是失败 → info 级（非红色错误）。
    showActionFailureToast(
      copyText("main.sftp.noMachine"),
      copyText("main.sftp.noMachineHint"),
      { level: "info" },
    );
    return;
  }
  if (hosts.length === 1) {
    void openFileWindow(hosts[0]);
    return;
  }
  // ≥2 台：选主机浮层（照 history F96：body-level fixed，Esc / 外部 pointerdown 关，下一拍挂监听防自关）。
  closeSftpHostPicker();
  const menu = document.createElement("div");
  menu.className = "sftp-host-picker";
  for (const h of hosts) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "sftp-host-picker-item";
    item.textContent = h.label || h.host;
    item.addEventListener("click", () => {
      closeSftpHostPicker();
      void openFileWindow(h);
    });
    menu.appendChild(item);
  }
  const r = anchor.getBoundingClientRect();
  menu.style.top = `${r.bottom + 4}px`;
  menu.style.right = `${Math.max(4, window.innerWidth - r.right)}px`;
  document.body.appendChild(menu);
  sftpHostPicker = menu;
  const close = (ev: Event): void => {
    if (ev instanceof KeyboardEvent && ev.key !== "Escape") return;
    if (ev.type === "pointerdown" && menu.contains(ev.target as Node)) return;
    closeSftpHostPicker();
  };
  sftpHostPickerClose = close;
  setTimeout(() => {
    if (sftpHostPicker !== menu) return; // 期间被新菜单/关闭取代 → 别挂陈旧监听
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", close);
  }, 0);
}
