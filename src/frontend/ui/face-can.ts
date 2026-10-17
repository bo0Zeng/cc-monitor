/**
 * **一条会话此刻能做什么 —— 只读后端那份整份脸的 `can`。**
 *
 * 点 · 字 · 能不能关 / 恢复 / 拉前都是那台后端判好的（`SessionFace`，会话三帧带来）；这里只有两样：
 * 读 `can` 的几个短名（各处不各写一遍 `can.term.can === "yes"`），和客户端唯一自己知道的事 ——
 * **那台此刻看不见**（或这条还没收到过脸）⇒ [`unseenFace`]。
 */
import type { SessionFace } from "./generated/SessionFace";
import type { Origin } from "./generated/Origin";
import { isRemoteOrigin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 这个会话在哪台：本机写「本机」，远端写机器名。 */
export function machineOf(t: { origin: Origin }): string {
  return isRemoteOrigin(t.origin) ? t.origin : copyText("sessionFace.machine.local");
}

/**
 * 那台看不见 / 还没收到过脸的那一张：点 `unknown`，只在右键菜单里关，不给恢复（也许还在跑，再起一份就是同一会话两个进程），
 * 别的都不给。
 */
export function unseenFace(machine: string): SessionFace {
  const tip = copyText("sessionState.unseen.tooltip");
  const no = { can: "no", why: tip } as const;
  return {
    dot: "unknown",
    live: { is: "no" },
    settled: false,
    name: copyText("sessionState.unseen.name"),
    tip,
    line: copyText("sessionFace.state.unseen", { machine }),
    tone: "plain",
    order: 3,
    can: { close: { can: "menu" }, resume: { can: "no", why: copyText("sessionState.unseen.noResume") }, term: no, reattach: no, model: no, permission: no },
  };
}

/** 此刻画的那张脸：收到过的那一份；没有（那台看不见 · 还没收到）⇒ [`unseenFace`]。 */
export function faceOf(t: { face: SessionFace | null; origin: Origin }): SessionFace {
  return t.face ?? unseenFace(machineOf(t));
}

/** 进程在（「活跃 N」· 最后活动时刻 · 几个进程在写）。 */
export const isLive = (f: SessionFace): boolean => f.live.is === "yes";
/** 还有终端可去（拉前 · 停 · 杀 · 终端页 · 自动跟随 · 后台物化）。 */
export const hasTerminal = (f: SessionFace): boolean => f.can.term.can === "yes";
/** 在 tmux 里：终端窗口关了也能「在终端里打开」。 */
export const inTmux = (f: SessionFace): boolean => f.can.term.can === "yes" && f.can.term.tmux;
/** 能恢复（Resume / 在 tmux 里起）。 */
export const canResume = (f: SessionFace): boolean => f.can.resume.can === "yes";
/** × · 中键 · W · 批量都关得掉。 */
export const closesQuietly = (f: SessionFace): boolean => f.can.close.can === "quiet";
/** 右键菜单里关得掉（含只在菜单里关的那一形）。 */
export const closesFromMenu = (f: SessionFace): boolean => f.can.close.can !== "no";
/** 已结束那一套外观（已结束 · 记录已不在）。 */
export const isEnded = (f: SessionFace): boolean => f.dot === "ended" || f.dot === "gone";
