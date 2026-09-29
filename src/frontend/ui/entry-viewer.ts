/**
 * **独立只读窗**的入口（`viewer.html?viewer=<sid>&origin=<机器>` 加载它）。`设计/01 §1.2`：只含 tab 管理 ＋ 渲染栈。
 *
 * 🔴 本窗的模块图里**没有**设置面板 / 历史 / 全景 / SFTP / 命令栏 —— 判据是
 * `tests/frontend/ui/entry-graphs.vitest.ts`（真跑 `vite build`，对本入口 chunk 的传递闭包做零命中断言），
 * 同一条还反过来钉住「tab 管理与渲染栈**确实在**」，免得零命中是因为整张图空了。
 *
 * 原先它加载 `index.html?viewer=<sid>`，由 `main.ts` 在 DOMContentLoaded 里分叉。
 */
import "./entry-common"; // 全局错误捕获（模块副作用）
import { installGlobalClickDelegation } from "./entry-render-common";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { basename } from "./format"; // F09：复用已测纯函数（去 main.ts 内联 basename 盲区；〔F7b〕随老面板退役从 sftp/paths 搬来）
import { bindEvents } from "./events";
import { TabManager } from "./tabs";
import { terminalFrontAvailable } from "./terminal-front";
import { loadTheme } from "./theme";
import { bindErrorToast } from "./error-toast";
import { dispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { turnEndNotifier } from "./turn-notify";
import { copyText } from "./copy-table";

// Vite HMR：任何热更新一律整页重载（理由见 `main.ts` 同名那段）。
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload();
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  // 主题尽早应用，避免渲染抖动
  await loadTheme();
  const sid = new URLSearchParams(location.search).get("viewer");
  if (!sid) {
    // 没带 sid 就没有东西可镜像。原先这种 URL 会落回主窗口的整套 bootstrap；
    // 拆开之后本窗口没有那一套，如实说出来而不是白屏。
    const status = document.getElementById("status-bar");
    if (status) status.textContent = copyText("entryViewer.module.missingParam");
    return;
  }
  // 〔CF2〕这个会话在哪台机器上（会话流 `subscribe(origin, kind)` 的寻址键）。缺 ⇒ 本机（旧的开窗 URL 只带 sid）。
  const rawOrigin = new URLSearchParams(location.search).get("origin");
  const origin: Origin = rawOrigin === null || rawOrigin === "" || isLocalOrigin(rawOrigin) ? LOCAL_ORIGIN : rawOrigin;
  await bootstrapViewer(sid, origin);
});

/**
 * issue #10：独立只读窗口的精简 bootstrap。
 *
 * 复用 TabManager（按 confirmed 架构）但只喂该 sid 的事件、隐藏全部 chrome
 * （tab 栏 / 设置 / 历史，由 `body.viewer-mode` CSS 控制）→ 自动继承分支折叠 /
 * 启动滚动消抖 / tool-group 合并 等全部渲染能力。
 *
 * 数据：〔CF2 · 第四波 4B〕订一条会话流 `session-lines/<sid>`（通道 `subscribe`，只要这一个会话）——
 * 留存由这条订阅当场交、之后的实时行接着交（与主窗口同一个 seq 空间）。原来的定向重放命令
 * （`replay_session_to_window`〔散文墓碑〕）与实时广播事件一起退役。重叠由 `seen` set 按 seq 去重。
 * **不发 `frontend-ready`** —— 那是主窗口那几条整台机器的订阅的就绪点。
 */
async function bootstrapViewer(sid: string, origin: Origin): Promise<void> {
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
  statusMsg.textContent = copyText("entryViewer.bootstrapViewer.title");
  status.appendChild(statusMsg);

  const empty = document.createElement("div");
  empty.className = "empty-state";
  empty.textContent = copyText("entryViewer.bootstrapViewer.loading");
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
  termBtn.textContent = copyText("entryViewer.bootstrapViewer.terminal");
  termBtn.title = copyText("entryViewer.bootstrapViewer.terminalHint");
  termBtn.addEventListener("click", () => tabs.bringActiveTerminalToFront());
  // 〔S4 · 第四波〕↗ 与 tab 上那颗同一道门（`terminal-front.ts`）：非 Windows 上最后一跳是桩、每点必败 ⇒ 不渲。
  //   快捷键（下面的 `terminal.bring-front`）还够得到 `bringActiveTerminalToFront`，那里会说一句实话。
  if (terminalFrontAvailable()) topbar.appendChild(termBtn);
  const cwdBtn = document.createElement("button");
  cwdBtn.type = "button";
  cwdBtn.className = "viewer-topbar-btn";
  cwdBtn.textContent = copyText("entryViewer.bootstrapViewer.cwd");
  cwdBtn.title = copyText("entryViewer.bootstrapViewer.cwdHint");
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
  dispatcher.bind("session.find", () => tabs.openFind()); // 〔SE2〕独立窗口里的那一个 tab 也能 Ctrl+F
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();

  // 留存与实时行可能重叠 → 按 per-file seq 去重。
  const seen = new Set<number>();
  let titleCwdSeq = Number.POSITIVE_INFINITY; // 顶栏标题取最早 cwd（项目根），同 tab.cwd 口径
  // **必须 await**：会话流订阅登记好再往下走。
  // 会话流的格由通道按窗口定向交（`chan.ts`）。〔MIG-1 收尾 · V41〕原先那一项按窗口作用域监听的选项随 `bindEvents` 里最后的 Tauri 监听一起删了。
  await bindEvents(
    {
      onLine: (e) => {
        if (e.session_id !== sid) return;
        if (seen.has(e.seq)) return;
        seen.add(e.seq);
        // 顶栏标题：用**最早**记录的 cwd 末段（项目根），跟 tab.cwd 口径一致。
        if (e.cwd && e.seq < titleCwdSeq) {
          titleCwdSeq = e.seq;
          const base = basename(e.cwd); // F09：已测 helper（行为等价：`if (base)` 守卫下 ""/undefined 同效）
          if (base) titleEl.textContent = base;
        }
        tabs.onLine(e);
      },
      onSessionEnded: (s) => {
        if (s === sid) tabs.archiveTab(s);
      },
      // audit-fixes F03.2：本 sid 的 idle-tmux 灰灯，与主窗一致（免视图窗停留陈旧绿灯）。
      onSessionIdle: (s) => {
        if (s === sid) tabs.markTmuxIdle(s);
      },
      onSessionStarted: (s) => {
        if (s === sid) tabs.reviveTab(s);
      },
      // 〔GP1 · 第四波〕那台机器看不见了 ⇒ 说不清（与主窗一致，免视图窗停在陈旧的「活」）。〔MIG-1 续〕机器级：落这台上的那一条。
      onOriginUnseen: (o) => tabs.markOriginUnseen(o),
      onBatchStart: () => tabs.onBatchStart(),
      onBatchEnd: () => tabs.onBatchEnd(),
      onStreamGap: (o) => tabs.onStreamGap(o),
    },
    { streams: [{ origin, kind: `session-lines/${sid}` }] },
  );

  bindErrorToast();
}
