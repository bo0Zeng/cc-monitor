import "./styles.css";
import { emit } from "@tauri-apps/api/event";
import { bindEvents } from "./events";
import { TabManager } from "./tabs";
import { loadTheme } from "./theme";
import { SettingsPanel } from "./settings";

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
  // 主题尽早应用，避免渲染抖动
  await loadTheme();

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

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.innerHTML = `暂无活跃会话<br><small>打开终端跑 <code>claude</code> 后将自动出现</small>`;
  streamRoot.appendChild(empty);

  const tabs = new TabManager(tabBar, streamRoot, ({ total, live }) => {
    statusCount.textContent = `活跃 ${live}`;
    statusMsg.textContent =
      live > 0 ? "M2 · 监听中" : "M2 · 等待活跃 Claude Code 会话…";
    empty.style.display = total > 0 ? "none" : "";
  });

  // 外观设置入口 —— 注入到 #app 上（绝对定位到 Tab Bar 右上）
  const settingsPanel = new SettingsPanel();
  const settingsTrigger = document.createElement("button");
  settingsTrigger.type = "button";
  settingsTrigger.className = "settings-trigger";
  settingsTrigger.title = "外观设置 (Ctrl+,)";
  settingsTrigger.setAttribute("aria-label", "打开外观设置");
  settingsTrigger.textContent = "⚙";
  settingsTrigger.addEventListener("click", () => {
    void settingsPanel.open();
  });
  document.getElementById("app")?.appendChild(settingsTrigger);

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

  // 快捷键：Ctrl+Tab 切下一个 / Ctrl+Shift+Tab 切上一个 / Ctrl+W 关 archived /
  // Ctrl+, 开设置；Esc 关设置在 SettingsPanel 内部处理。
  window.addEventListener("keydown", (e) => {
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === "Tab") {
      e.preventDefault();
      tabs.cycleActive(e.shiftKey ? -1 : 1);
    } else if (e.key === "w" || e.key === "W") {
      e.preventDefault();
      tabs.closeActiveIfArchived();
    } else if (e.key === ",") {
      e.preventDefault();
      void settingsPanel.open();
    }
  });

  bindEvents({
    onLine: (e) => tabs.onLine(e),
    onSessionEnded: (sessionId) => tabs.archiveTab(sessionId),
  });

  // 通知后端可以发了 —— 缓冲的 line 会被 flush 过来
  void emit("frontend-ready");
});
