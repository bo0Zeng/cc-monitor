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
import { SettingsPanel, SETTINGS_APPLIED_EVENT } from "./settings";
import { listen } from "@tauri-apps/api/event";
import { HistoryView } from "./views/history";
import { PanoramaView } from "./views/panorama";
import { UsageView } from "./views/usage-view";
import { UsageHud } from "./usage-hud";
import { bindErrorToast, showActionFailureToast } from "./error-toast";
import { bindRemoteHealthToast } from "./remote-health";
// F83（#39）：顶栏 SFTP 入口——按远端主机数 0/1/N 分支打开现有 SFTP 模态。
import { openSftpPanel } from "./sftp/panel";
import { readRemoteConfig, sftpEligibleHosts } from "./settings/remote-section";
import { TasksPanel } from "./tasks-panel";
import { AgentsPanel } from "./agents-panel";
import { getBehavior, setBehavior } from "./behavior";
import { dispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { turnEndNotifier } from "./turn-notify";

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

  // F82a（#56+#47）：独立设置窗口 —— URL 带 ?settings=1 时走精简 bootstrap 只挂设置面板，
  // 不建 tab / 历史 / 全景等主窗口 chrome。照 viewer §22 范式（async 建窗已在后端规避死锁）。
  if (new URLSearchParams(location.search).get("settings")) {
    await bootstrapSettings();
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

  // F88b（#52）：context% HUD chip——活跃会话「最新一轮 prompt token ÷ 模型上限」实时占用。
  // 挂 agents chip 旁；TabManager.onActiveUsageChanged 喂数据；点击打开用量视图（下方注入）。
  const usageHud = new UsageHud();
  status.appendChild(usageHud.summaryElement);

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
  // F88b：活跃会话 usage 变化 → 刷新 HUD context% chip（onLine 新 assistant 记录 / switchTo 切会话）
  tabs.onActiveUsageChanged = (model, promptTokens) => {
    usageHud.setActive(model, promptTokens);
  };

  // v2.4 issue #2：拉一次 behavior toggle 初值喂给 TabManager。
  // 设置面板改了之后会再调 applyBehavior 同步。
  void getBehavior().then((b) => tabs.applyBehavior(b));

  // F82a（#56+#47）：设置改由**独立窗口**承载（SS-3 终态），主窗口不再内嵌设置浮层。
  // 设置窗口保存 / 行为 toggle 后广播 SETTINGS_APPLIED_EVENT → 主窗口重读并应用主题 + 行为
  // （跨 OS 窗口回调够不到，用广播代替原 onBehaviorChange 直连）。loadTheme 内部 applyTheme。
  void listen(SETTINGS_APPLIED_EVENT, () => {
    void loadTheme(); // 主题：loadTheme 内部 applyTheme
    void getBehavior().then((b) => tabs.applyBehavior(b)); // 行为
    void getKeybindings().then((kb) => dispatcher.applyOverrides(kb)); // 键位：热应用主窗口 dispatcher
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
    void invoke("open_settings_window"); // F82a：开独立设置窗口（非浮层）
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

  // Batch15-P2：代码全景入口 —— 顶栏右侧，紧邻历史按钮左边。自挂 body 作 fixed overlay
  // （照 HistoryView），对活跃**本地**会话的 cwd 建 code-picture 索引画代码库地图。
  const panoramaView = new PanoramaView(() => tabs.activeRepoInfo());
  // F70（护城河）：右键 tab「在全景高亮本会话改动」→ 切到该会话 → 打开全景 → 高亮它改过的
  // 节点。TabManager 不直接持有 PanoramaView，走注入回调（同 onManualSwitch 范式）。
  tabs.requestPanoramaHighlight = (sid) => {
    const info = tabs.touchedFilesFor(sid); // 远端/无 cwd 已被 getter 挡掉（返 null）
    if (!info) return;
    tabs.switchTo(sid); // 置活跃 → activeRepoInfo=该仓 → 全景加载该仓
    void (async () => {
      await panoramaView.open(); // 同仓复用；异仓重索引/加载
      await panoramaView.highlightSession(info.files);
    })();
  };
  const panoramaTrigger = document.createElement("button");
  panoramaTrigger.type = "button";
  panoramaTrigger.className = "panorama-trigger";
  panoramaTrigger.title = "代码全景 (G)";
  panoramaTrigger.setAttribute("aria-label", "打开代码全景");
  panoramaTrigger.textContent = "🗺";
  panoramaTrigger.addEventListener("click", () => {
    if (panoramaView.isVisible()) panoramaView.close();
    else void panoramaView.open();
  });
  document.getElementById("app")?.appendChild(panoramaTrigger);

  // F88a：用量视图入口 —— 顶栏右侧，紧邻全景按钮左边。自挂 body 作 fixed overlay。
  // 只 token 不 $（已花费≠配额，本地推不出配额）。
  const usageView = new UsageView();
  const usageTrigger = document.createElement("button");
  usageTrigger.type = "button";
  usageTrigger.className = "usage-trigger";
  usageTrigger.title = "用量（token 已花费）";
  usageTrigger.setAttribute("aria-label", "打开用量视图");
  usageTrigger.textContent = "∑";
  usageTrigger.addEventListener("click", () => {
    if (usageView.isVisible()) usageView.close();
    else void usageView.open();
  });
  document.getElementById("app")?.appendChild(usageTrigger);
  // F88b：点 context% HUD chip → 也开用量视图（chip 是活跃会话实时占用，视图是跨会话汇总，互补）
  usageHud.onClick(() => {
    if (!usageView.isVisible()) void usageView.open();
  });

  // F83（#39）：顶栏 SFTP 入口 —— 设置搬独立窗后腾出的入口位给 SFTP。点击按远端主机数分支：
  // 0 台提示 / 1 台直开 / 多台选单（选单见 openSftpFromTopbar）。SFTP 仍用现有模态（抽屉化延后）。
  const sftpTrigger = document.createElement("button");
  sftpTrigger.type = "button";
  sftpTrigger.className = "sftp-trigger";
  sftpTrigger.title = "SFTP 文件（浏览 / 上传 / 下载远端文件）";
  sftpTrigger.setAttribute("aria-label", "打开 SFTP 文件面板");
  sftpTrigger.textContent = "🗂";
  sftpTrigger.addEventListener("click", () => void openSftpFromTopbar(sftpTrigger));
  document.getElementById("app")?.appendChild(sftpTrigger);

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
  dispatcher.bind("app.open-settings", () => void invoke("open_settings_window")); // F82a：开独立设置窗口
  dispatcher.bind("app.toggle-history", () => {
    if (historyView.isVisible()) historyView.close();
    else void historyView.open();
  });
  dispatcher.bind("app.toggle-panorama", () => {
    if (panoramaView.isVisible()) panoramaView.close();
    else void panoramaView.open();
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

  // F40c:DEV-only E2E 探针(抖动采样 + Ctrl+Alt+F9 状态快照 → fe_perf 日志)。
  // 生产构建 DEV 恒 false,整支被 vite 消除;热键不进 keybindings registry,
  // 不占用户配置面。须在 bindEvents 之前就绪(startup batch 紧随 frontend-ready)。
  if (import.meta.env.DEV) {
    try {
      e2eProbe = await import("./e2e-probe");
      e2eProbe.registerSnapshotHotkey(() => tabs.debugSnapshot());
    } catch (e) {
      console.warn("[e2e-probe] 加载失败(不影响功能):", e);
    }
  }

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
      "无可用远端主机",
      "先在设置 → 连接 配好 host / user，再打开 SFTP 文件面板。",
      { level: "info" },
    );
    return;
  }
  if (hosts.length === 1) {
    void openSftpPanel(hosts[0]);
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
      void openSftpPanel(h);
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

/**
 * F82a（#56+#47）：独立**设置窗口**的精简 bootstrap —— 只挂 SettingsPanel（windowMode），
 * 无 tab / 历史 / 全景 chrome。照 viewer §22 范式。设置项经既有 config 命令读写（窗口无关）；
 * 保存 / 行为 toggle 后 panel 广播 `settings-applied`，主窗口 listen 并重读应用主题+行为（跨窗同步）。
 * 主题已在 `bootstrap()` 开头 `loadTheme()` 应用（本 fn 在其后），此处不重复。
 * **窗体渲染/布局本环境无 GUI 不可自测 → 真机验证累积**（照 viewer 已验证脚手架把盲实现风险压最低）。
 */
async function bootstrapSettings(): Promise<void> {
  document.body.classList.add("settings-window-mode");
  // 同 viewer：外链/代码块复制走全局 click 代理（防未来设置里的外链在本 WebView 打开顶掉 UI）。
  installGlobalClickDelegation();
  const panel = new SettingsPanel({ windowMode: true });
  await panel.open(); // 面板压 overlay 栈底
  // ★ 设置窗有自己的 dispatcher 实例，必须 applyOverrides + start()（同 viewer bootstrap）——否则
  //   ① 快捷键编辑器录制收不到键（onKeyDown 未挂）；② Esc 无法经 overlay LIFO 逐层关（编辑器 / SFTP
  //   面板压栈其上时先关它们，栈底面板 handleEsc→cancel→关窗）。不 bind app 动作（本窗无 tab 等），
  //   overlay.close(Esc) 与录制都不依赖 bind。（原手搓的 window Esc 监听会双关窗，已删。）
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();
}

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
  // Batch14-F42：viewer 是独立 webview（自带一份 notifier 单例）且广播行照收——
  // 只让主窗口发通知，否则重复通知 + "用户正聚焦 viewer"时主窗口误发。
  turnEndNotifier.disable();
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
