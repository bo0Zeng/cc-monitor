/**
 * 「恢复」默认的「运行于」：**那台**的「恢复到 tmux 里」（那台后端 `prefs.json` 的 `resumeInTmux`，经 `prefs-read` 问）× 那台有没有 tmux（那台答过才知道）。
 * 历史页［恢复 ▾］与标签页「恢复 ▸」的默认那一组都读这里（`resume-menu.ts::defaultPick`）。
 *
 * 这里只记每台最近一答（进程内，不落盘）：没问过那台 ⇒ 这一次先按不进 tmux、后台问一次；设置改过了（宿主收到广播）⇒
 * [`forgetResumeDefaults`]、下次重问。问不到 ⇒ 记成不进 tmux（改过设置才再问，不一直重问）。
 */
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { readPrefs } from "./machine-prefs";

/** 那台的「恢复到 tmux 里」（问回来的；问着的那几台不在里面）。 */
const inTmux = new Map<Origin, boolean>();
/** 正在问的那几台（一台只问一次）。 */
const asking = new Set<Origin>();
/** 那台有没有 tmux（问到过的才有；`false` ＝ 那台说了没有）。 */
const tmuxOn = new Map<Origin, boolean>();
/** 问回来的次代：忘掉以后，之前发出去的那一问回来不记。 */
let generation = 0;

/** 设置改过了：忘掉记着的每台那一格，下次重问。 */
export function forgetResumeDefaults(): void {
  generation++;
  inTmux.clear();
  asking.clear();
}

/** 问那台一次（一台只问一次）；回来记下，问不到记成不进。 */
function ask(origin: Origin): Promise<void> {
  asking.add(origin);
  const gen = generation;
  return readPrefs(origin).then(
    (r) => {
      if (gen === generation) inTmux.set(origin, r.prefs.resumeInTmux);
    },
    () => {
      if (gen === generation) inTmux.set(origin, false);
    },
  );
}

/** 记下那台有没有 tmux（那台答过才记）。 */
export function noteMachineTmux(origin: Origin, has: boolean): void {
  tmuxOn.set(origin, has);
}

/** 那台有没有 tmux：答过 ⇒ 那一答；没问到 ⇒ `null`（不知道，不当没有）。 */
export function machineHasTmux(origin: Origin): boolean | null {
  return tmuxOn.get(origin) ?? null;
}

/** 默认在不在 tmux 里：照那台那一格（没问到 ⇒ 先按不进、去问）；那台说了没有 tmux ⇒ 不用 tmux。缺 origin ⇒ 本机。 */
export function resumeInTmuxFor(origin: Origin | undefined): boolean {
  const o = origin ?? LOCAL_ORIGIN;
  const pref = inTmux.get(o);
  if (pref === undefined && !asking.has(o)) void ask(o);
  return (pref ?? false) && tmuxOn.get(o) !== false;
}

/** 问到那台再答（独立查看窗：菜单没动过时照问回来的那一答换默认）。 */
export async function resumeInTmuxAt(origin: Origin): Promise<boolean> {
  if (!inTmux.has(origin)) await ask(origin);
  return resumeInTmuxFor(origin);
}
