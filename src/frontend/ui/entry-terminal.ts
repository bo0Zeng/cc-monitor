/**
 * **单独终端窗口**的入口（`terminal.html?origin=<机器>&terminal=<句柄>&name=<tmux 会话名>&title=<标题>` 加载它）：
 * 整扇窗只装终端面部件（`TerminalPage`，与抽屉 · 「终端」视图同一个），看 URL 里那一台的那个终端。
 * 规矩同 agent 窗口：一个终端一扇（壳按「那台 ＋ 句柄」定窗口名，再点拉前）；终端结束了窗不关（面上照「已结束」画）；退出 app 一起关。
 */
import "./entry-common"; // 全局错误捕获（模块副作用）
import { installGlobalClickDelegation } from "./entry-render-common";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { loadTheme } from "./theme";
import { bindErrorToast } from "./backend-errors";
import { dispatcher } from "./keybindings/registry";
import { getKeybindings } from "./keybindings/store";
import { copyText } from "./copy-table";
import { TerminalPage, type Watched } from "./terminal-page";
import { onMachineUp } from "./machine-feed";

// Vite HMR：任何热更新一律整页重载（理由见 `main.ts` 同名那段）。
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    window.location.reload();
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  await loadTheme();
  const q = new URLSearchParams(location.search);
  const terminal = q.get("terminal");
  const root = document.getElementById("terminal-window");
  if (!terminal || !root) {
    if (root) root.textContent = copyText("entryViewer.module.missingParam");
    return;
  }
  const raw = q.get("origin");
  const origin: Origin = raw === null || raw === "" || isLocalOrigin(raw) ? LOCAL_ORIGIN : raw;
  const watched: Watched = { kind: "slot", origin, terminal, name: q.get("name") ?? "", title: q.get("title") ?? "" };
  installGlobalClickDelegation();
  dispatcher.bind("app.minimize", () => void getCurrentWindow().minimize());
  dispatcher.applyOverrides(await getKeybindings());
  dispatcher.start();
  bindErrorToast();
  const page = new TerminalPage({ active: () => watched, front: () => {}, focusStream: () => page.el.focus() });
  page.el.tabIndex = -1;
  root.replaceChildren(page.el);
  // 那台断开又连上 ⇒ 实时自己重新订上。
  onMachineUp((o) => page.machineUp(o));
  page.sessionChanged();
  page.setVisible(true);
});
