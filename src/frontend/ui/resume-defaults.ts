/**
 * 「恢复」默认的「运行于」：通用页「恢复到 tmux 里」那一格 × 那台有没有 tmux（那台答过才知道）。
 * 历史页［恢复 ▾］与标签页「恢复 ▸」的默认那一组都读这里（`resume-menu.ts::defaultPick`）。
 */
import type { Origin } from "./ipc/origin";

let inTmux = false;
/** 那台有没有 tmux（问到过的才有；`false` ＝ 那台说了没有）。 */
const tmuxOn = new Map<Origin, boolean>();

/** 通用页那一格（宿主读到行为时交进来：启动时与设置改了之后）。 */
export function setResumeInTmux(on: boolean): void {
  inTmux = on;
}

/** 记下那台有没有 tmux（那台答过才记）。 */
export function noteMachineTmux(origin: Origin, has: boolean): void {
  tmuxOn.set(origin, has);
}

/** 那台有没有 tmux：答过 ⇒ 那一答；没问到 ⇒ `null`（不知道，不当没有）。 */
export function machineHasTmux(origin: Origin): boolean | null {
  return tmuxOn.get(origin) ?? null;
}

/** 默认在不在 tmux 里：照那一格；那台说了没有 tmux ⇒ 不用 tmux（没问到 ⇒ 照那一格）。 */
export function resumeInTmuxFor(origin: Origin | undefined): boolean {
  return inTmux && (origin === undefined || tmuxOn.get(origin) !== false);
}
