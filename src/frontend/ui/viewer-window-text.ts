/**
 * 独立查看窗的两句字：状态栏那一行与系统标题（只排版，在不在跑 · 订没订着流由查看器给）。
 */
import { copyText } from "./copy-table";
import type { HistoryRow } from "./history-list-reads";
import type { ViewerStatus } from "./views/session-viewer";

/** 窗口状态栏那一行：`只读 · {n} 条 · 实时`（真订着流、在跑）/ `只读 · {n} 条 · 已结束` / `只读 · {n} 条`。 */
export function windowStatus(s: ViewerStatus): string {
  if (s.live && s.following) return copyText("viewerWindow.status.live", { n: s.n });
  if (!s.live) return copyText("viewerWindow.status.ended", { n: s.n, state: copyText("sessionState.ended.name") });
  return copyText("viewerWindow.status.plain", { n: s.n });
}

/** 系统标题：`{会话标题} · {状态} · {项目}`，远端再加 `· {机器}`。 */
export function windowTitle(r: HistoryRow, live: boolean | null): string {
  const title = r.untitled ? copyText("history.row.untitled") : r.label;
  const state =
    live === null ? copyText("sessionState.unseen.name") : live ? copyText("history.row.live") : copyText("sessionState.ended.name");
  return r.origin
    ? copyText("viewerWindow.title.remote", { title, state, project: r.projectName, machine: r.origin })
    : copyText("viewerWindow.title.local", { title, state, project: r.projectName });
}

