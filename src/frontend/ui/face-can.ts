/**
 * **一条会话此刻能做什么 —— 只读后端那份整份脸的 `can`。**
 *
 * 点 · 字 · 能不能关 / 恢复 / 拉前都是后端判好的（`SessionFace`，会话三帧带来；那台看不见时那一张由本机后端造、随「说不清」那一格交来）。
 * 这里只有读 `can` 的几个短名（各处不各写一遍 `can.term.can === "yes"`）。一张脸都还没收到（恢复的标签页、重放还没到）⇒ `null`：
 * 不画点，几个短名一律答「不能」。界面不自己拼脸。
 */
import type { SessionFace } from "./generated/SessionFace";
import type { WireSessionActivity } from "./generated/WireSessionActivity";
import type { Origin } from "./generated/Origin";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { terminalFrontAvailable } from "./terminal-front";

/** 这个会话在哪台：本机写「本机」，远端写机器名。 */
export function machineOf(t: { origin: Origin }): string {
  return isRemoteOrigin(t.origin) ? t.origin : copyText("sessionFace.machine.local");
}

/** 一张脸（还没收到 ⇒ `null`）。 */
type Face = SessionFace | null;

/** 进程在（「活跃 N」· 最后活动时刻 · 几个进程在写）。界面里「在跑」只这一个读法：读那台写好的 `life`，不看点、不另判。 */
export const isLive = (f: Face): boolean => f?.life.is === "live";
/** 那台说不清活不活（判不了 · 那台看不见）。 */
export const isUnclear = (f: Face): boolean => f?.life.is === "unclear";
/** 进程在 ⇒ 那台判出的在干什么（判不出 ⇒ 没有）；进程不在 ⇒ 没有。 */
export const activityOf = (f: Face): WireSessionActivity | undefined => (f?.life.is === "live" ? f.life.activity : undefined);
/** 还有终端可去（拉前 · 停 · 终端页 · 自动跟随 · 后台物化）。 */
export const hasTerminal = (f: Face): boolean => f?.can.term.can === "yes";
/**
 * ↗ 切到终端此刻点不点得了（菜单 · 会话头 · 栏行尾 · 命令面板 · 快捷键同一道，A2-17-12-1）：点得了 ⇒ `null`；
 * 点不了 ⇒ 那一句 —— 壳说这台上 ↗ 不是真的（`terminal-front.ts`）⇒ 那一句；会话脸 `can.term` 说不能 ⇒ 那台写好的那一句；
 * 还没收到脸 ⇒ 空串（灰着、不说为什么）。各处一律画、灰着写这一句，不藏。
 */
export function frontWhy(f: Face): string | null {
  if (!terminalFrontAvailable()) return copyText("terminalFront.unavailable.short");
  if (!f) return "";
  return f.can.term.can === "yes" ? null : f.can.term.why;
}
/** 结束得了（右键 · 多选同一个确认框，交 `sessions-stop`）。 */
export const canEnd = (f: Face): boolean => f?.can.end.can === "yes";
/** 能恢复（Resume / 在 tmux 里起）。 */
export const canResume = (f: Face): boolean => f?.can.resume.can === "yes";
/** × · 中键 · W · 批量都关得掉。 */
export const closesQuietly = (f: Face): boolean => f?.can.close.can === "quiet";
/** 右键菜单里关得掉（含只在菜单里关的那一形）。还没收到脸 ⇒ 关得掉（只在菜单里）：一个标签页总能关。 */
export const closesFromMenu = (f: Face): boolean => f === null || f.can.close.can !== "no";
/** 已结束那一套外观（已结束 · 记录已不在）。 */
export const isEnded = (f: Face): boolean => f?.dot === "ended" || f?.dot === "gone";
