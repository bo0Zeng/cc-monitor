/**
 * 起新会话框的两问读数（那台后端判，界面只读）：`session-open-facts`（框打开时：最近用过的目录 · 有没有 tmux · 能起哪几家 · 分叉源会话的三格）·
 * `session-open-dir`（目录失焦：在不在 · 那台会铸的终端名）。都是 typed 帧命令（入参 · 回包类型从 Rust 生成，形状由握手指纹核过）；起的那一问在 `session-open.ts`。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { DirChecked } from "./generated/DirChecked";
import type { OpenFacts } from "./generated/OpenFacts";

/** 读两问：扫记录目录 ＋ 列一次 tmux；15 s 盖住远端握手。 */
const READ_BUDGET_MS = 15_000;

/** 框打开时那一问的回包（分叉源会话的三格同 `session-fork` 的 `launch`）。 */
export type { OpenFacts };

/** 框打开时问一次：typed 帧命令（回包类型从 Rust 生成，形状由握手指纹核过）。 */
export async function askFacts(origin: Origin, fork: { sid: string; uuid: string } | null): Promise<OpenFacts> {
  return callTyped(origin, "session-open-facts", fork ? { forkOf: fork.sid, at: fork.uuid } : {}, budgetWithin(READ_BUDGET_MS));
}

/** 目录失焦：typed 帧命令（回包类型从 Rust 生成，形状由握手指纹核过）。 */
export async function askDir(origin: Origin, cwd: string, forkOf: string | null): Promise<DirChecked> {
  return callTyped(origin, "session-open-dir", forkOf ? { cwd, forkOf } : { cwd }, budgetWithin(READ_BUDGET_MS));
}
