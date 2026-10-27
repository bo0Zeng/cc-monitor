/**
 * 一组会话的两问**走通道，那台后端出成品**（后端 `control/session_set.rs`）：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 补足的样子（存好的一组按名字，或框里还没存的那几行）：每行已有 · 再起 · 组名 · 起不了的原因 | `set-plan` | [`SetPlan`] |
 * | 照补足起（只起差的几个，起之前组注解就写好） | `set-launch` | [`SetLaunched`] |
 *
 * 存组 · 改名 · 删组走配置文件那一口（`profiles-reads.ts` 的 `writeProfiles`）。判定全在那台后端，这里只发、只收。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { RowEdit } from "./generated/RowEdit";
import type { SetPlan } from "./generated/SetPlan";
import type { SetLaunched } from "./generated/SetLaunched";

export type { RowEdit, SetPlan, SetLaunched };

/** 预览一问：读配置文件 ＋ 数活会话（秒级）。 */
const PLAN_BUDGET_MS = 15_000;
/** 起一组：逐条走 `session-open`（每条那台自己 15 s），一组至多 12 条。 */
const LAUNCH_BUDGET_MS = 120_000;

/** 存好的那一组（`name`）或框里那几行（`rows`）补足的样子。只读，那台一个字节不写。 */
export function planSet(origin: Origin, which: { name: string } | { rows: RowEdit[] }): Promise<SetPlan> {
  return callTyped(origin, "set-plan", which, budgetWithin(PLAN_BUDGET_MS));
}

/** 照存好的那一组补足、立刻起（补足是幂等的：连点两下不多起）。 */
export function launchSet(origin: Origin, name: string): Promise<SetLaunched> {
  return callTyped(origin, "set-launch", { name }, budgetWithin(LAUNCH_BUDGET_MS));
}
