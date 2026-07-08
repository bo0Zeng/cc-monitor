/**
 * 前端入口。DOMContentLoaded 后按序：
 * 1. `loadTheme()` 从 config.json 应用 CSS 变量
 * 2. 实例化 TabManager / SettingsPanel / HistoryView / TasksPanel
 * 3. `bindEvents()` 订阅后端事件（jsonl-line / jsonl-batch / session-ended / task-update）
 * 4. 装全局快捷键 dispatcher（keybindings/）+ 外链 click 代理（openUrl）+ ERROR toast
 * 5. `emit("frontend-ready")` 通知后端 replay 历史
 *
 * 另持有启动 perf 测量（`window.__ccmPerf`，与后端 lib.rs 的 t0 互补看完整启动管线）。
 * HMR 走 full reload（不引框架，原生 DOM，强制整页重载简化心智模型）。
 */
import "./styles.css";
import { emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { LS_KEYS, safeGet } from "./local-storage";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { bindEvents } from "./events";
import { TabManager } from "./tabs";
import { loadTheme } from "./theme";
import { SettingsPanel } from "./settings";
import { HistoryView } from "./views/history";
import { bindErrorToast } from "./error-toast";
import { bindRemoteHealthToast } from "./remote-health";
import { TasksPanel } from "./tasks-panel";
import { AgentsPanel } from "./agents-panel";
import { getBehavior, setBehavior } from "./behavior";
import { dispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";

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

// Vite HMR 默认在没显式 `hot.accept` 时对 TS 文件部分热替换，可能让旧模块的
// 已注册 listener / 已渲染 DOM 与新代码并存。对监控这种长跑+事件密集的应用，
// 部分热替换会造成视觉错乱、消息重复 listener、event_replay 状态不一致。
// 强制：任何 hot update 一律 full reload，保证 monitor 视图永远是单一一致版本。
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload();
  });
}

// 全局错误捕获 —— 渲染到 status-bar 便于无 devtools 也能诊断
window.addEventListener("error", (e) => {
  // Batch13-F38:c-v 材料化→RO→snap 链路使这条**良性**浏览器提示变频繁
  // (RO 回调引发布局变化,浏览器推迟到下帧,无功能影响)——不上状态栏不进 console
  if (e.message.includes("ResizeObserver loop")) return;
  const bar = document.getElementById("status-bar");
  if (bar) bar.textContent = `ERR: ${e.message} @ ${e.filename}:${e.lineno}`;
  console.error("global error:", e.error ?? e.message);
});
window.addEventListener("unhandledrejection", (e) => {
  const bar = document.getElementById("status-bar");
  if (bar) bar.textContent = `REJ: ${e.reason}`;
  console.error("unhandled rejection:", e.reason);
});

window.addEventListener("DOMContentLoaded", async () => {
  window.__ccmPerf.domContentLoaded = performance.now();
  console.info(
    `[perf] DOMContentLoaded @ ${window.__ccmPerf.domContentLoaded.toFixed(0)}ms (since navigation start)`,
  );
  // 主题尽早应用，避免渲染抖动
  await loadTheme();
  window.__ccmPerf.themeLoaded = performance.now();

  // issue #10：独立只读窗口 —— URL 带 ?viewer=<sid> 时走精简 bootstrap 后返回，
  // 不建设置 / 历史 / 多 tab chrome，只镜像渲染该 session。
  const viewerSid = new URLSearchParams(location.search).get("viewer");
  if (viewerSid) {
    await bootstrapViewer(viewerSid);
    return;
  }

  const tabBar = document.getElementById("tab-bar");
  const streamRoot = document.getElementById("message-stream");
  const status = document.getElementById("status-bar");

  if (!tabBar || !streamRoot || !status) {
    console.error("layout containers missing");
    return;
  }

  status.innerHTML = "";
  const statusMsg = document.createElement("span");
  statusMsg.className = "status-msg";
  statusMsg.textContent = "M2 · 等待活跃 Claude Code 会话…";
  status.appendChild(statusMsg);
  const statusCount = document.createElement("span");
  statusCount.className = "status-count";
  statusCount.textContent = "活跃 0";
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

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.innerHTML = `暂无活跃会话<br><small>打开终端跑 <code>claude</code> 后将自动出现</small>`;
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
      statusCount.textContent = `活跃 ${live}`;
      statusMsg.textContent =
        live > 0 ? "M2 · 监听中" : "M2 · 等待活跃 Claude Code 会话…";
      empty.style.display = total > 0 ? "none" : "";
    },
    tasksPanel,
    agentsPanel,
  );
  // Batch5-F19（G 验收）：用户手动切过 tab 后，迟到的远端宣告不再补切抢焦点
  tabs.onManualSwitch = () => {
    pendingStartupActive = null;
  };

  // v2.4 issue #2：拉一次 behavior toggle 初值喂给 TabManager。
  // 设置面板改了之后会再调 applyBehavior 同步。
  void getBehavior().then((b) => tabs.applyBehavior(b));

  // 设置入口 —— 注入到 #app 上（绝对定位到 Tab Bar 右上）
  // v2.4 issue #2: 行为 toggle 改了立即同步给 TabManager
  const settingsPanel = new SettingsPanel({
    onBehaviorChange: (cfg) => tabs.applyBehavior(cfg),
  });
  // Batch11-F33：竖直 tab 栏——右缘拖拽调宽（localStorage 记忆）+ 窄窗折叠图标条。
  {
    const appEl = document.getElementById("app");
    if (appEl) {
      const KEY = "cc-monitor.tab-bar-w";
      const clampW = (w: number): number => Math.min(340, Math.max(110, w));
      const saved = Number(localStorage.getItem(KEY));
      if (Number.isFinite(saved) && saved > 0) {
        appEl.style.setProperty("--tab-bar-w", `${clampW(saved)}px`);
      }
      const resizer = document.createElement("div");
      resizer.id = "tab-bar-resizer";
      resizer.title = "拖拽调整 tab 栏宽度";
      // 拖动期间**不能**实时改 --tab-bar-w：网格列宽一变，消息区整棵布局树重排，
      // 而切 tab 零卡顿方案让所有 tab 的 DOM 都 visibility 保活在布局树里——每次
      // mousemove 全量重排 = 拖动巨卡。改为拖动时只画 fixed 参考线（repaint-only），
      // 松手一次性提交宽度。
      resizer.addEventListener("mousedown", (e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        const barLeft = document.getElementById("tab-bar")?.getBoundingClientRect().left ?? 0;
        const guide = document.createElement("div");
        guide.className = "tab-bar-resize-guide";
        const applyGuide = (clientX: number): number => {
          const w = clampW(clientX - barLeft);
          guide.style.left = `${barLeft + w}px`;
          return w;
        };
        let lastW = applyGuide(e.clientX);
        document.body.appendChild(guide);
        resizer.classList.add("resizing");
        const finish = (commit: boolean): void => {
          document.removeEventListener("mousemove", onMove);
          document.removeEventListener("mouseup", onUp);
          window.removeEventListener("blur", onBlur);
          guide.remove();
          resizer.classList.remove("resizing");
          if (commit) {
            appEl.style.setProperty("--tab-bar-w", `${lastW}px`);
            localStorage.setItem(KEY, String(lastW));
          }
        };
        const onMove = (ev: MouseEvent): void => {
          // 主键已松开（窗外释放 / 切走时 mouseup 丢失）→ 取消收尾。否则 document
          // 级 mousemove 监听永久泄漏，之后选中文字都在触发它（同 tab 撕离的容错）。
          if ((ev.buttons & 1) === 0) {
            finish(false);
            return;
          }
          lastW = applyGuide(ev.clientX);
        };
        const onUp = (): void => finish(true);
        // alt-tab / 点别的窗口切走 → 落点不可信，直接取消（回来不会带着幽灵拖拽）。
        const onBlur = (): void => finish(false);
        document.addEventListener("mousemove", onMove);
        document.addEventListener("mouseup", onUp);
        window.addEventListener("blur", onBlur);
      });
      appEl.appendChild(resizer);
      // 窄窗折叠：内容列 780px + 栏 + 呼吸空间放不下 → 图标条（44px）
      const applyCollapse = (): void => {
        document.body.classList.toggle("tabbar-collapsed", window.innerWidth < 980);
      };
      window.addEventListener("resize", applyCollapse);
      applyCollapse();
    }
  }

  const settingsTrigger = document.createElement("button");
  settingsTrigger.type = "button";
  settingsTrigger.className = "settings-trigger";
  settingsTrigger.title = "设置 (,)";
  settingsTrigger.setAttribute("aria-label", "打开设置");
  settingsTrigger.textContent = "⚙";
  settingsTrigger.addEventListener("click", () => {
    void settingsPanel.open();
  });
  document.getElementById("app")?.appendChild(settingsTrigger);

  // 历史浏览器入口 —— 顶栏右侧，紧邻设置按钮左边
  // v2.5+: HistoryView 不再接管 streamRoot，自挂 body 作 fixed overlay
  const historyView = new HistoryView();
  const historyTrigger = document.createElement("button");
  historyTrigger.type = "button";
  historyTrigger.className = "history-trigger";
  historyTrigger.title = "历史会话浏览器 (H)";
  historyTrigger.setAttribute("aria-label", "打开历史会话浏览器");
  // 纯字符的时钟符号（U+25F7），避免 emoji 跨平台/字体差异
  historyTrigger.textContent = "◷";
  historyTrigger.addEventListener("click", () => {
    if (historyView.isVisible()) {
      historyView.close();
    } else {
      void historyView.open();
    }
  });
  document.getElementById("app")?.appendChild(historyTrigger);

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
  dispatcher.bind("terminal.bring-front", () => tabs.bringActiveTerminalToFront());
  dispatcher.bind("app.open-settings", () => void settingsPanel.open());
  dispatcher.bind("app.toggle-history", () => {
    if (historyView.isVisible()) historyView.close();
    else void historyView.open();
  });
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

  // 先加载用户覆盖，再 start —— 避免 start 后 1-2ms 内按键走 default 而非用户值
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();

  // await：保证 listener 注册完成再 emit frontend-ready（否则后端 replay 可能早于
  // 监听就绪而丢事件 —— 主窗口此前靠往返延迟侥幸不触发，显式 await 更稳，也跟
  // viewer 路径一致）。
  await bindEvents({
    // P5.2 B 重构：onLine 不再带 source 参数（前端按 seq timeline 排，不分 batch/live）
    onLine: (e) => tabs.onLine(e),
    onSessionEnded: (sessionId) => tabs.archiveTab(sessionId),
    // 会话复活（resume）：后端 liveness 门控后才发，复活已归档的本地 Tab，免 F5。
    // Batch7-F24：无 Tab（= 运行中途**新出现**的本地会话）→ 建骨架——bg 会话必须
    // 从这条通道拿 kind/name（首行 onLine→ensureTab 不带 kind，会建成无 ⚙ 普通 tab）。
    onSessionStarted: (sessionId, meta) => {
      if (tabs.hasTab(sessionId)) {
        tabs.reviveTab(sessionId);
      } else {
        tabs.createSkeletonTab(sessionId, meta.cwd || null, null, meta.kind, meta.name);
      }
    },
    // 启动重放（jsonl-batch）期间走 batch 模式（lazy hljs + BranchFolder.batchMode），
    // 结束时 flush。onChunk 已删 —— B 重构后 chunk 切边界对前端不可见。
    onBatchStart: () => tabs.onBatchStart(),
    onBatchEnd: () => tabs.onBatchEnd(),
    // v2.3.0 issue #11: task watcher 推送的 task 列表更新
    onTasksUpdate: (e) => tabs.updateTasks(e.sessionId, e.tasks),
    // issue #23: 会话红绿灯（busy=绿 / idle·shell=红 / waiting=黄）
    onSessionActivity: (e) =>
      tabs.updateActivity(e.session_id, e.status, e.waiting_for),
    // Batch5-F18：远端会话宣告 → 骨架 Tab。Batch7-F24：p1e daemon 附 cwd/kind/name
    // ——骨架标题即时完整（bg → ⚙ + 树状挂宿主后）；旧 daemon 缺省照旧 sid 前缀。
    onRemoteSessionAdded: (sessionId, origin, meta) => {
      tabs.createSkeletonTab(sessionId, meta.cwd || null, origin, meta.kind, meta.name);
      // Batch5-F19：上次所在 tab 是远端会话时在此补切（应用一次即清；超过
      // 30s 启动窗口则放弃——迟到宣告不抢焦点，replay 优先级不受影响）
      if (pendingStartupActive === sessionId) {
        pendingStartupActive = null;
        if (Date.now() < startupActiveDeadline) {
          tabs.switchTo(sessionId, "auto");
        }
      }
    },
  });

  // v2.0.0 (issue #4)：后端 ERROR 级别 tracing → 右下角红色 toast
  bindErrorToast();
  // issue #32 (SS-F)：远端健康事件（拥塞丢行 / 版本不符）→ 右下角 info toast
  bindRemoteHealthToast();

  // Batch5-F19（G 验收 B-1）：**先读记忆再建骨架**——第一个骨架的自动切换会
  // 经 switchTo 写回 localStorage，读晚了就把用户记忆覆写成清单首个 sid（F19
  // 主路径在"本地有会话"的常见场景下整体失效）。骨架期同时抑制写回双保险。
  const lastActive = safeGet(LS_KEYS.lastActiveSid);
  tabs.persistLastActive = false;

  // Batch5-F18：frontend-ready 之前先拉本地活跃清单建全部骨架 Tab——用户在
  // 内容重放开始前就看到完整 tab 栏。失败不阻启动（骨架只是体验优化，行
  // 到达照常 ensureTab 建）。远端骨架走 remote-session-added 事件，不在此列。
  try {
    const active = await invoke<
      { session_id: string; cwd: string; kind: string | null; name: string | null }[]
    >("list_active_sessions");
    for (const s of active) {
      tabs.createSkeletonTab(s.session_id, s.cwd || null, null, s.kind ?? null, s.name ?? null);
    }
  } catch (e) {
    console.warn("[skeleton] list_active_sessions failed:", e);
  }

  // Batch5-F19：启动 active = 上次所在 tab（localStorage 记忆）。本地骨架里有
  // 就立即切；是远端会话则挂 pending，等它的 remote-session-added 宣告到达时
  // 补切（应用一次即清，之后不再抢焦点）。选择完成后恢复写回。
  if (lastActive && tabs.hasTab(lastActive)) {
    tabs.switchTo(lastActive, "auto");
    pendingStartupActive = null;
  } else {
    pendingStartupActive = lastActive;
  }
  tabs.persistLastActive = true;

  // 通知后端可以发了 —— 缓冲的 line 会被 flush 过来。payload 带上次所在 tab
  // （Batch5-F19）：后端 replay 按 session 分组、该 tab 的内容块先发。
  window.__ccmPerf.frontendReadyEmit = performance.now();
  console.info(
    `[perf] emit frontend-ready @ ${window.__ccmPerf.frontendReadyEmit.toFixed(0)}ms`,
  );
  void emit("frontend-ready", { prioritySid: lastActive });

  // issue #23: 红绿灯初始快照（session-activity 事件不进 replay buffer，F5 会丢；
  // 快照 + 事件增量双路收敛，同 fetchSessionTasks 模式）。Tab 未建时进 pendingActivity 暂存。
  void tabs.syncActivitySnapshot();

  // 注：maximize / 全屏后内容错位的修复在 Rust 侧（src-tauri/src/lib.rs on_window_event：
  // 去抖后微调 webview 尺寸强制 wry 重新 put_Bounds，把 WebView2 合成层钉回左上角）。
  // 旧版（v2.13.0）在这里做的 onResized + scrollTop 微滚动够不着 DOM 之下的合成层偏移，已删。
});

/**
 * issue #10：独立只读窗口的精简 bootstrap。
 *
 * 复用 TabManager（按 confirmed 架构）但只喂该 sid 的事件、隐藏全部 chrome
 * （tab 栏 / 设置 / 历史，由 `body.viewer-mode` CSS 控制）→ 自动继承分支折叠 /
 * 启动滚动消抖 / tool-group 合并 等全部渲染能力。
 *
 * 数据：实时 `jsonl-line` 广播本就到所有窗口（按 sid 过滤）；历史走定向
 * `replay_session_to_window`（与实时同 seq 空间）。两者重叠由 `seen` set 按 seq 去重。
 * **不发 `frontend-ready`** —— 那会触发后端对所有窗口的全量 replay。
 */
async function bootstrapViewer(sid: string): Promise<void> {
  document.body.classList.add("viewer-mode");
  const tabBar = document.getElementById("tab-bar");
  const streamRoot = document.getElementById("message-stream");
  const status = document.getElementById("status-bar");
  if (!tabBar || !streamRoot || !status) {
    console.error("viewer: layout containers missing");
    return;
  }

  status.innerHTML = "";
  const statusMsg = document.createElement("span");
  statusMsg.className = "status-msg";
  statusMsg.textContent = "独立只读视图";
  status.appendChild(statusMsg);

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.textContent = "加载中…";
  streamRoot.appendChild(empty);

  // 复用 TabManager，过滤到本 sid；tab 栏由 .viewer-mode 隐藏。无 tasksPanel。
  const tabs = new TabManager(tabBar, streamRoot, ({ total }) => {
    empty.style.display = total > 0 ? "none" : "";
  });
  // Batch5-F19 R1：viewer 窗口共享 localStorage，禁写 last-active（防污染主窗口记忆）
  tabs.persistLastActive = false;

  // issue #10：slim 顶栏 —— 标题 + 调出终端 + 打开工作目录。按钮复用 TabManager 的
  // bringActiveTerminalToFront / openActiveTabCwd（作用于其唯一的 active tab）。
  const topbar = document.createElement("div");
  topbar.className = "viewer-topbar";
  const titleEl = document.createElement("span");
  titleEl.className = "viewer-topbar-title";
  titleEl.textContent = sid.slice(0, 8);
  topbar.appendChild(titleEl);
  const termBtn = document.createElement("button");
  termBtn.type = "button";
  termBtn.className = "viewer-topbar-btn";
  termBtn.textContent = "↗ 终端";
  termBtn.title = "调出对应终端窗口 (`)";
  termBtn.addEventListener("click", () => tabs.bringActiveTerminalToFront());
  topbar.appendChild(termBtn);
  const cwdBtn = document.createElement("button");
  cwdBtn.type = "button";
  cwdBtn.className = "viewer-topbar-btn";
  cwdBtn.textContent = "📂 目录";
  cwdBtn.title = "打开工作目录 (E)";
  cwdBtn.addEventListener("click", () => tabs.openActiveTabCwd());
  topbar.appendChild(cwdBtn);
  const appEl = document.getElementById("app");
  appEl?.insertBefore(topbar, appEl.firstChild);

  installGlobalClickDelegation();

  // 快捷键：最小化 + 真全屏 + issue #10 调出终端 / 打开 cwd（复用 tabs 的 active-tab 动作）。
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.bind("app.toggle-fullscreen", () => {
    const w = getCurrentWindow();
    void w
      .isFullscreen()
      .then((f) => w.setFullscreen(!f))
      .catch((e) => console.warn("toggle-fullscreen failed:", e));
  });
  dispatcher.bind("terminal.bring-front", () => tabs.bringActiveTerminalToFront());
  dispatcher.bind("tab.open-cwd", () => tabs.openActiveTabCwd());
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();

  // 定向 replay 与实时广播可能重叠 → 按 per-file seq 去重。
  const seen = new Set<number>();
  let titleCwdSeq = Number.POSITIVE_INFINITY; // 顶栏标题取最早 cwd（项目根），同 tab.cwd 口径
  // **必须 await**：listener 注册完成前调 replay 会丢事件（实测白屏只剩状态栏）。
  // **windowScoped:true**：定向 replay 用 emit_to(本窗口)，须用窗口作用域监听才收得到
  // （模块级 listen 是 Any 监听，命不中定向发射）。详 BindEventsOptions.windowScoped。
  await bindEvents(
    {
      onLine: (e) => {
        if (e.session_id !== sid) return;
        if (seen.has(e.seq)) return;
        seen.add(e.seq);
        // 顶栏标题：用**最早**记录的 cwd 末段（项目根），跟 tab.cwd 口径一致。
        if (e.cwd && e.seq < titleCwdSeq) {
          titleCwdSeq = e.seq;
          const base = e.cwd.replace(/[\\/]+$/, "").split(/[\\/]/).pop();
          if (base) titleEl.textContent = base;
        }
        tabs.onLine(e);
      },
      onSessionEnded: (s) => {
        if (s === sid) tabs.archiveTab(s);
      },
      onSessionStarted: (s) => {
        if (s === sid) tabs.reviveTab(s);
      },
      onBatchStart: () => tabs.onBatchStart(),
      onBatchEnd: () => tabs.onBatchEnd(),
    },
    { windowScoped: true },
  );

  bindErrorToast();

  // 拉本 sid 的历史（定向 emit 到本窗口）。不发 frontend-ready。
  try {
    await invoke("replay_session_to_window", { sessionId: sid });
  } catch (e) {
    console.error("viewer: replay_session_to_window failed:", e);
    statusMsg.textContent = `加载失败：${String(e)}`;
  }
}

/**
 * 外链 + 代码块复制的全局 click 代理。主窗口与独立 viewer 窗口共用。
 * - 外链（render.ts 标 data-external）：preventDefault + openUrl 走系统浏览器，
 *   避免 WebView2 把 UI 替换成外站。capture 阶段顶层接管防被卡片 handler 抢先。
 * - 代码块"复制"按钮：marked 输出的 HTML 无法挂 listener，这里 delegate。
 */
function installGlobalClickDelegation(): void {
  document.addEventListener(
    "click",
    (e) => {
      const a = (e.target as HTMLElement | null)?.closest?.(
        "a[data-external]",
      ) as HTMLAnchorElement | null;
      if (!a) return;
      const href = a.getAttribute("href") ?? "";
      if (!href) return;
      e.preventDefault();
      void openUrl(href).catch((err) => {
        console.warn("[external link] openUrl failed:", href, err);
      });
    },
    true,
  );

  document.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement | null)?.closest?.(".code-copy");
    if (!btn) return;
    const block = btn.closest(".code-block");
    const pre = block?.querySelector("pre");
    const text = pre?.textContent ?? "";
    if (!text) return;
    void navigator.clipboard.writeText(text).then(
      () => {
        btn.classList.add("copied");
        btn.textContent = "已复制";
        window.setTimeout(() => {
          btn.classList.remove("copied");
          btn.textContent = "复制";
        }, 1200);
      },
      () => {
        btn.textContent = "失败";
        window.setTimeout(() => (btn.textContent = "复制"), 1200);
      },
    );
  });
}
