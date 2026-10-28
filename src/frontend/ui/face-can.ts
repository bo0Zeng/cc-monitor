/**
 * **一条会话此刻能做什么 —— 只读后端那份整份脸的 `can`。**
 *
 * 点 · 字 · 能不能关 / 恢复 / 拉前都是后端判好的（`SessionFace`，会话三帧带来；那台看不见时那一张由本机后端造、随「说不清」那一格交来）。
 * 这里只有读 `can` 的几个短名（各处不各写一遍 `can.term.can === "yes"`）。一张脸都还没收到（恢复的标签页、重放还没到）⇒ `null`：
 * 不画点，几个短名一律答「不能」。界面不自己拼脸。
 */
import type { SessionFace } from "./generated/SessionFace";
import type { Origin } from "./generated/Origin";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 这个会话在哪台：本机写「本机」，远端写机器名。 */
export function machineOf(t: { origin: Origin }): string {
  return isRemoteOrigin(t.origin) ? t.origin : copyText("sessionFace.machine.local");
}

/** 一张脸（还没收到 ⇒ `null`）。 */
type Face = SessionFace | null;

/** 进程在（「活跃 N」· 最后活动时刻 · 几个进程在写）。 */
export const isLive = (f: Face): boolean => f?.life.is === "live";
/** 还有终端可去（拉前 · 停 · 杀 · 终端页 · 自动跟随 · 后台物化）。 */
export const hasTerminal = (f: Face): boolean => f?.can.term.can === "yes";
/** 能恢复（Resume / 在 tmux 里起）。 */
export const canResume = (f: Face): boolean => f?.can.resume.can === "yes";
/** × · 中键 · W · 批量都关得掉。 */
export const closesQuietly = (f: Face): boolean => f?.can.close.can === "quiet";
/** 右键菜单里关得掉（含只在菜单里关的那一形）。还没收到脸 ⇒ 关得掉（只在菜单里）：一个标签页总能关。 */
export const closesFromMenu = (f: Face): boolean => f === null || f.can.close.can !== "no";
/** 已结束那一套外观（已结束 · 记录已不在）。 */
export const isEnded = (f: Face): boolean => f?.dot === "ended" || f?.dot === "gone";
