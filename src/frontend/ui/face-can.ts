/**
 * **一条会话此刻能做什么 —— 只读后端那份整份脸的 `can`。**
 *
 * 点 · 字 · 能不能关 / 恢复 / 拉前都是后端判好的（`SessionFace`，会话三帧带来；那台看不见时那一张由本机后端造、随「说不清」那一格交来）。
 * 这里只有读 `can` 的几个短名（各处不各写一遍 `can.end.can === "yes"`）。一张脸都还没收到（恢复的标签页、重放还没到）⇒ `null`：
 * 不画点，几个短名一律答「不能」。界面不自己拼脸。
 */
import type { SessionFace } from "./generated/SessionFace";
import type { SessionActivity } from "./generated/SessionActivity";
import type { TermWay } from "./generated/TermWay";

/** 一张脸（还没收到 ⇒ `null`）。 */
type Face = SessionFace | null;

/** 进程在（「活跃 N」· 最后活动时刻 · 几个进程在写）。界面里「在跑」只这一个读法：读那台写好的 `life`，不看点、不另判。 */
export const isLive = (f: Face): boolean => f?.life.is === "live";
/** 那台说不清活不活（判不了 · 那台看不见）。 */
export const isUnclear = (f: Face): boolean => f?.life.is === "unclear";
/** 进程在 ⇒ 那台判出的在干什么（判不出 ⇒ 没有）；进程不在 ⇒ 没有。 */
export const activityOf = (f: Face): SessionActivity | undefined => (f?.life.is === "live" ? f.life.activity : undefined);
/** 进程或它住的那个终端还在（自动跟随 · 「已隐藏」那一行留不留）：读那台写好的 `life`，不另判。 */
export const terminalAlive = (f: Face): boolean => f?.life.is === "live" || f?.life.is === "reconnectable";
/**
 * 「终端」那一颗此刻做什么（会话头 · 右键 · 命令面板 · 快捷键同一道）：那台把两种看法都写好了，这里只挑 ——
 * 看的这一端就在会话住的那台上（本机标签页，`remote` 为假）⇒ `near`，别处 ⇒ `far`。还没收到脸 ⇒ `null`（灰着、不说为什么）。
 */
export const terminalWay = (f: Face, remote: boolean): TermWay | null => (f ? (remote ? f.can.terminal.far : f.can.terminal.near) : null);
/** 「终端」灰着的那一句：做得了 ⇒ `null`；做不了 ⇒ 那台写好的那一句；还没收到脸 ⇒ 空串。 */
export function terminalWhy(f: Face, remote: boolean): string | null {
  const w = terminalWay(f, remote);
  if (w === null) return "";
  return w.go === "no" ? w.why : null;
}
/** 结束得了（右键 · 多选同一个确认框，交 `sessions-stop`）。 */
export const canEnd = (f: Face): boolean => f?.can.end.can === "yes";
/** 停得下这一轮（输入框里的 ■，交 `session-interrupt`）：那台判一轮在跑才给（判不出 · 读记录中 · 需手动 ⇒ 不给）。 */
export const canStop = (f: Face): boolean => f?.can.stop.can === "yes";
/** 能恢复（Resume / 在 tmux 里起）。 */
export const canResume = (f: Face): boolean => f?.can.resume.can === "yes";
/** 程序退了、tmux 会话还在（标签页的可重连钩子）。 */
export const canReattach = (f: Face): boolean => f?.can.reattach.can === "yes";
/** × · 中键 · W · 批量都关得掉。 */
export const closesQuietly = (f: Face): boolean => f?.can.close.can === "quiet";
/** 右键菜单里关得掉（含只在菜单里关的那一形）。还没收到脸 ⇒ 关得掉（只在菜单里）：一个标签页总能关。 */
export const closesFromMenu = (f: Face): boolean => f === null || f.can.close.can !== "no";
/** 已结束那一套外观（已结束 · 记录已不在）。 */
export const isEnded = (f: Face): boolean => f?.dot === "ended" || f?.dot === "gone";
