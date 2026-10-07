/**
 * **独立查看窗**的入口（`viewer.html?viewer=<sid>&origin=<机器>` 加载它）：按会话 ID 开任意一个会话（在跑的跟着长，已结束的也开得了）。
 *
 * 外壳：细顶栏（收起 / 展开「你说过的话」· `{项目} › {标题}` · 在跑徽标 ｜［恢复 ▾］或［切过去］· 打开目录 · Windows 上在跑的多「切到终端」）·
 * 左侧「你说过的话」一栏 ＋ 消息流（与历史页右边同一个只读查看器、同一套卡）· 细状态栏 `只读 · {n} 条 · 实时` / `只读 · {n} 条 · 已结束`。
 * 系统标题 `{会话标题} · {状态} · {项目}`（远端再加 `· {机器}`），状态变了跟着改。`Esc` 不关窗。
 *
 * 这一行的事实（标题 · 项目 · 记录在哪 · 在不在跑 · 能做什么）问那台的 `history-list`（`sid` 那一形：只要这一个会话）；
 * 内容按记录读、在跑的订 `session-lines/<sid>` 跟着长（`SessionViewer` 的 `follow`）。
 *
 * 带 `run=<运行>` ⇒ 这扇窗是一个子运行自己的窗口（agent 窗口，`views/agent-window.ts`）。
 *
 * 🔴 本窗的模块图里**没有**设置面板 / 历史页 / 命令栏 —— 判据是 `tests/frontend/ui/entry-graphs.vitest.ts`。
 */
import "./entry-common"; // 全局错误捕获（模块副作用）
import { installGlobalClickDelegation } from "./entry-render-common";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { loadTheme } from "./theme";
import { bindErrorToast } from "./backend-errors";
import { dispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { copyText } from "./copy-table";
import { fetchList, type HistoryRow } from "./history-list-reads";
import { SessionViewer } from "./views/session-viewer";
import { button, setBusy, setDisabled } from "./kit/button";
import { tag } from "./kit/badge";
import { banner } from "./kit/banner";
import { icon } from "./kit/icon";
import { splitButton } from "./kit/split-button";
import { defaultPick, resumeAccounts, resumeHint, resumeMenuItems, type ResumeAccounts, type ResumePick } from "./resume-menu";
import { resumeHistoryRow } from "./history-resume";
import { resumeInTmuxFor, setResumeInTmux } from "./resume-defaults";
import { getBehavior } from "./behavior";
import { askOf, FOLLOW, type AccountAsk } from "./launch-account";
import { openNewSession } from "./new-session";
import { revealInFolder } from "./reveal-in-folder";
import { terminalFrontAvailable } from "./terminal-front";
import { bringRemoteTerminalToFront, bringTerminalToFront } from "./tab-session-actions";
import { frontView } from "./front-result";
import { copyFrontDetail, flashFrontDone, showFrontResult } from "./front-pop";
import { machineName } from "./control-said";
import { openSettingsWindow } from "./settings/open-settings";
import { connectTerminalOf } from "./settings-dest";
import { startInTmuxThenAttach } from "./tmux-resume";
import { SWITCH_TO_SESSION_EVENT } from "./window-events";
import { windowStatus, windowTitle } from "./viewer-window-text";
import { bootstrapAgentWindow } from "./views/agent-window";

// Vite HMR：任何热更新一律整页重载（理由见 `main.ts` 同名那段）。
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload();
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  await loadTheme();
  const q = new URLSearchParams(location.search);
  const sid = q.get("viewer");
  const status = document.getElementById("status-bar");
  if (!sid) {
    if (status) status.textContent = copyText("entryViewer.module.missingParam");
    return;
  }
  // 这个会话在哪台机器上（`history-list` 与会话流的寻址键）。缺 ⇒ 本机。
  const rawOrigin = q.get("origin");
  const origin: Origin = rawOrigin === null || rawOrigin === "" || isLocalOrigin(rawOrigin) ? LOCAL_ORIGIN : rawOrigin;
  // 带 `run` ⇒ 这扇窗是这个会话里一个子运行自己的窗口（agent 窗口）。
  const run = q.get("run");
  if (run) {
    await loadKeys();
    await bootstrapAgentWindow(sid, run, origin);
    return;
  }
  await bootstrapViewer(sid, origin);
});

/** 这扇窗也认的那几条快捷键（最小化 · 全屏）＋ 出错提示。 */
async function loadKeys(): Promise<void> {
  installGlobalClickDelegation();
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.bind("app.toggle-fullscreen", () => {
    const w = getCurrentWindow();
    void w
      .isFullscreen()
      .then((f) => w.setFullscreen(!f))
      .catch((e) => console.warn("toggle-fullscreen failed:", e));
  });
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();
  bindErrorToast();
}

async function bootstrapViewer(sid: string, origin: Origin): Promise<void> {
  document.body.classList.add("viewer-mode");
  const app = document.getElementById("app");
  const streamRoot = document.getElementById("message-stream");
  const statusEl = document.getElementById("status-bar");
  if (!app || !streamRoot || !statusEl) {
    console.error("viewer: layout containers missing");
    return;
  }
  installGlobalClickDelegation();

  // 细顶栏（`#app` 网格的 top 那一格）。
  const topbar = document.createElement("div");
  topbar.className = "viewer-topbar";
  const sideBtn = button({ label: copyText("sessionViewer.tools.saidLabel"), kind: "icon", icon: "list", hint: copyText("sessionViewer.tools.saidLabel") });
  sideBtn.dataset.role = "side";
  const crumbs = document.createElement("div");
  crumbs.className = "viewer-topbar-title";
  const acts = document.createElement("div");
  acts.className = "viewer-topbar-acts";
  topbar.append(sideBtn, crumbs, acts);
  app.insertBefore(topbar, app.firstChild);

  const viewer = new SessionViewer({ window: { foot: statusEl } });
  streamRoot.replaceChildren(viewer.element);
  sideBtn.addEventListener("click", () => viewer.toggleSide());

  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.bind("app.toggle-fullscreen", () => {
    const w = getCurrentWindow();
    void w
      .isFullscreen()
      .then((f) => w.setFullscreen(!f))
      .catch((e) => console.warn("toggle-fullscreen failed:", e));
  });
  dispatcher.bind("session.find", () => viewer.openFind());
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();
  bindErrorToast();

  let row: HistoryRow | null;
  try {
    const list = await fetchList(isLocalOrigin(origin) ? undefined : origin, { sid });
    row = list.rows.find((r) => r.sessionId === sid) ?? null;
  } catch (e) {
    viewer.showBanner(banner("error", copyText("sessionViewer.load.failed", { why: String(e) }), []));
    return;
  }
  if (!row) {
    // 记录不在了（被删了 / 那台没有这个会话）：顶上一条，窗口照留。
    viewer.showBanner(banner("warn", copyText("viewerWindow.record.gone"), []));
    return;
  }
  const r = row;
  let live: boolean | null = r.status === "live" ? true : r.status === "ended" ? false : null;
  const win = getCurrentWindow();
  const paint = (): void => {
    void win.setTitle(windowTitle(r, live)).catch(() => {});
    crumbs.replaceChildren(...crumbsOf(r, live === true));
    acts.replaceChildren(...actionsOf(r, live === true, viewer));
  };
  paint();
  await viewer.load({
    jsonlPath: r.jsonlPath,
    displayTitle: r.untitled ? copyText("history.row.untitled") : r.label,
    origin,
    agent: r.agent,
    cwd: r.projectPath,
    suppressBranch: !r.can.fork,
    statusOf: windowStatus,
    follow: {
      sid,
      live: live === true,
      onLive: (l) => {
        live = l;
        paint();
      },
    },
  });
}

/** `{项目} › {标题}` ＋ 在跑的徽标。 */
function crumbsOf(r: HistoryRow, live: boolean): Node[] {
  const project = document.createElement("span");
  project.className = "viewer-topbar-project";
  project.textContent = r.projectName;
  const sep = icon("caretRight", "compact");
  const title = document.createElement("span");
  title.className = "viewer-topbar-label";
  title.textContent = r.untitled ? copyText("history.row.untitled") : r.label;
  title.title = title.textContent;
  const out: Node[] = [project, sep, title];
  if (live) {
    const b = tag(copyText("history.row.live"));
    b.dataset.emph = "live";
    out.push(b);
  }
  if (r.agentTag) out.push(tag(r.agentTag));
  return out;
}

/** 顶栏 ↗：切过去了换对勾；其余结局一个浮层锚在它下面（这扇窗做得了的那几颗按钮；更新 · 重新连接在主窗口做）。 */
async function viewerFront(b: HTMLElement, r: HistoryRow, origin: Origin): Promise<void> {
  const res = r.origin ? await bringRemoteTerminalToFront(origin, r.sessionId) : await bringTerminalToFront(r.sessionId);
  const view = frontView(res, machineName(origin), false);
  if (view === null) {
    flashFrontDone(b);
    return;
  }
  const acts = view.acts.filter((a) => a.kind !== "update" && a.kind !== "reconnect");
  showFrontResult(b, r.sessionId, { ...view, acts }, async (a) => {
    if (a.kind === "copy") await copyFrontDetail(a.detail);
    else if (a.kind === "retry") await viewerFront(b, r, origin);
    else if (a.kind === "connect") await openSettingsWindow(undefined, connectTerminalOf(LOCAL_ORIGIN));
    else if (a.kind === "open-in-terminal") await startInTmuxThenAttach({ origin, agent: r.agent, sid: r.sessionId, cwd: r.projectPath }, FOLLOW, { again: async () => {} });
  });
}

/** 右端：［恢复 ▾］或［切过去］· 打开目录（本机）· Windows 上在跑的「切到终端」。 */
function actionsOf(r: HistoryRow, live: boolean, viewer: SessionViewer): HTMLElement[] {
  const out: HTMLElement[] = [];
  if (live) {
    // 切过去：主窗口拉前、切到那个会话的标签页（窗口留着）。
    out.push(
      button({
        label: copyText("history.row.switch"),
        kind: "primary",
        size: "compact",
        hint: copyText("history.row.switchHint"),
        onClick: () => void emit(SWITCH_TO_SESSION_EVENT, { sessionId: r.sessionId }).catch((e: unknown) => console.warn("[viewer] 交不给主窗口：", e)),
      }),
    );
  } else {
    out.push(resumeControl(r, viewer));
  }
  if (!r.origin && r.projectPath) {
    out.push(button({ label: copyText("history.menu.openDir"), kind: "icon", icon: "folder", hint: copyText("history.menu.openDir"), onClick: () => void revealInFolder(r.projectPath) }));
  }
  if (live && terminalFrontAvailable()) {
    const origin = r.origin ?? LOCAL_ORIGIN;
    out.push(
      button({
        label: copyText("tabBarView.tab.terminalHint"),
        kind: "icon",
        icon: "front",
        hint: copyText("tabBarView.tab.terminalHint"),
        onClick: (ev) => void viewerFront(ev.currentTarget as HTMLElement, r, origin),
      }),
    );
  }
  return out;
}

/** ［恢复 ▾］：与历史页同一个组件；起了 ⇒ 主窗口那边等它出现（窗口留着）；没起 ⇒ 消息流上面一条错误条 ＋［重试］。 */
function resumeControl(r: HistoryRow, viewer: SessionViewer): HTMLElement {
  const pick: ResumePick = defaultPick(r.origin ?? LOCAL_ORIGIN);
  let accounts: ResumeAccounts = !r.can.accounts
    ? { kind: "none", agentName: r.agentTag ?? r.agent }
    : r.lastAccount
      ? { kind: "list", items: [{ name: r.lastAccount, label: r.lastAccount, quota: null, last: true }] }
      : { kind: "off" };
  // 选不了的号：那台不起、给替代 ⇒ 点了替代 ⇒ 带那个号再起一次（`again`）。
  const go = async (account: AccountAsk = askOf(pick.account, pick.useBase)): Promise<void> => {
    viewer.showBanner(null);
    setBusy(sb.main, copyText("history.resume.busy"));
    try {
      await resumeHistoryRow(r, { tmux: pick.tmux, account }, (a) => go(a ?? account));
    } catch (e) {
      const said = copyText("history.resume.failed", { machine: r.origin ?? copyText("history.filter.local"), why: String(e) });
      const retry = button({ label: copyText("history.group.retry"), size: "compact", onClick: () => void go(account) });
      viewer.showBanner(banner("error", said, [retry]));
    } finally {
      setBusy(sb.main, null);
    }
  };
  const sb = splitButton({
    label: copyText("history.row.resume"),
    hint: resumeHint(accounts, pick),
    onClick: () => void go(),
    moreLabel: copyText("history.resume.more"),
    items: async () => {
      accounts = await resumeAccounts(r.origin ?? LOCAL_ORIGIN, r.agent, { hasAccounts: r.can.accounts, agentName: r.agentTag ?? r.agent, last: r.lastAccount });
      sb.main.title = resumeHint(accounts, pick);
      return resumeMenuItems({
        accounts,
        pick,
        onChange: (p) => (sb.main.title = resumeHint(accounts, p)),
        newInDir: () => void openNewSession({ origin: r.origin ?? LOCAL_ORIGIN, cwd: r.projectPath, agent: r.agent }),
      });
    },
  });
  sb.root.dataset.role = "resume";
  // 默认的「运行于」照通用页「恢复到 tmux 里」（这扇窗自己读一次；菜单没动过才换）。
  let touched = false;
  const before = pick.tmux;
  void getBehavior().then((b) => {
    setResumeInTmux(b.resumeInTmux);
    if (touched || pick.tmux !== before) return;
    pick.tmux = resumeInTmuxFor(r.origin ?? LOCAL_ORIGIN);
    sb.main.title = resumeHint(accounts, pick);
  });
  sb.more.addEventListener("click", () => (touched = true), { once: true });
  if (r.can.resume === "bg") {
    setDisabled(sb.main, copyText("history.row.bgHint"));
    setDisabled(sb.more, copyText("history.row.bgHint"));
  }
  return sb.root;
}
