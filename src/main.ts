import "./styles.css";
import { emit } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { bindEvents } from "./events";
import { TabManager } from "./tabs";
import { loadTheme } from "./theme";
import { SettingsPanel } from "./settings";
import { HistoryView } from "./views/history";
import { bindErrorToast } from "./error-toast";
import { TasksPanel } from "./tasks-panel";
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

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.innerHTML = `暂无活跃会话<br><small>打开终端跑 <code>claude</code> 后将自动出现</small>`;
  streamRoot.appendChild(empty);

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
  );

  // v2.4 issue #2：拉一次 behavior toggle 初值喂给 TabManager。
  // 设置面板改了之后会再调 applyBehavior 同步。
  void getBehavior().then((b) => tabs.applyBehavior(b));

  // 设置入口 —— 注入到 #app 上（绝对定位到 Tab Bar 右上）
  // v2.4 issue #2: 行为 toggle 改了立即同步给 TabManager
  const settingsPanel = new SettingsPanel({
    onBehaviorChange: (cfg) => tabs.applyBehavior(cfg),
  });
  const settingsTrigger = document.createElement("button");
  settingsTrigger.type = "button";
  settingsTrigger.className = "settings-trigger";
  settingsTrigger.title = "设置 (Ctrl+,)";
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
  historyTrigger.title = "历史会话浏览器 (Ctrl+H)";
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

  // v2.4.3 issue #13: 外链全局事件代理。render.ts 给 http/https/mailto 链接
  // 打了 data-external 标记；这里 preventDefault + openUrl 走系统默认浏览器，
  // 避免 WebView2 把整个 monitor UI 替换成外站。
  // capture 阶段 + 顶层接管，避免被卡片内部 click handler 抢先 stopPropagation。
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

  // 代码块"复制"按钮全局事件代理：marked code renderer 输出 HTML 字符串无法
  // 在生成时挂 listener，统一在这里 delegate。click 命中 .code-copy 时把所在
  // .code-block 里 <pre> 的纯文本扔进剪贴板。
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
  dispatcher.bind("terminal.bring-front", () => tabs.bringActiveTerminalToFront());
  dispatcher.bind("app.open-settings", () => void settingsPanel.open());
  dispatcher.bind("app.toggle-history", () => {
    if (historyView.isVisible()) historyView.close();
    else void historyView.open();
  });
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
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

  bindEvents({
    // P5.2 B 重构：onLine 不再带 source 参数（前端按 seq timeline 排，不分 batch/live）
    onLine: (e) => tabs.onLine(e),
    onSessionEnded: (sessionId) => tabs.archiveTab(sessionId),
    // 启动重放（jsonl-batch）期间走 batch 模式（lazy hljs + BranchFolder.batchMode），
    // 结束时 flush。onChunk 已删 —— B 重构后 chunk 切边界对前端不可见。
    onBatchStart: () => tabs.onBatchStart(),
    onBatchEnd: () => tabs.onBatchEnd(),
    // v2.3.0 issue #11: task watcher 推送的 task 列表更新
    onTasksUpdate: (e) => tabs.updateTasks(e.sessionId, e.tasks),
  });

  // v2.0.0 (issue #4)：后端 ERROR 级别 tracing → 右下角红色 toast
  bindErrorToast();

  // 通知后端可以发了 —— 缓冲的 line 会被 flush 过来
  window.__ccmPerf.frontendReadyEmit = performance.now();
  console.info(
    `[perf] emit frontend-ready @ ${window.__ccmPerf.frontendReadyEmit.toFixed(0)}ms`,
  );
  void emit("frontend-ready");
});
