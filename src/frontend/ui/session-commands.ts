/**
 * 命令面板里「切到会话」那几条（来自 tab 栏的只读投影 `TabManager.snapshotSessions`）。纯函数。
 * 标题就用 tab 的标题：远端会话的机器名它已经带着（`tab-model.ts::computeTitleFor`），这里不再加一遍。
 */
import type { Command } from "./views/command-bar";
import type { GridSessionSnapshot } from "./session-status";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";

export function sessionCommands(
  sessions: readonly Pick<GridSessionSnapshot, "sessionId" | "title" | "origin" | "cwd">[],
  switchTo: (sid: string) => void,
): Command[] {
  return sessions.map((s) => ({
    id: `switch-${s.sessionId}`,
    title: copyText("main.cmd.switchSession", { title: s.title }),
    keywords: copyText("main.cmd.switchSessionKeywords", { cwd: s.cwd ?? "", machine: isRemoteOrigin(s.origin) ? s.origin : "" }),
    run: () => switchTo(s.sessionId),
  }));
}
