/**
 * **问图**（计划页一格详情头的［问这一块…］）：开全产品那一个新建会话框的问图样子。
 *
 * 开之前问计划页那台一次 `plan-ask`：开口那句那台拼好（别名的那句 ＋「看 编号「标题」这一格：」），别名预选那台记着的问图用的那条；
 * 框里改选别名 ⇒ 带上那条再问一次（那台记新的、拼新的）。机器锁那台、目录锁工作区根；起会话照旧走 `session-open`。
 * 问图不补足：同一格连问两次 ＝ 起两条。
 */
import { copyText } from "./copy-table";
import { failToast } from "./kit/toast";
import { openNewSession, type NewSessionSpec } from "./new-session";
import { askPlan, type PlanAsk, type PlanCell, type PlanSlice } from "./plan-reads";
import type { Origin } from "./ipc/origin";

/** 问图那一下开的框：照那台回的那一份预填；改选别名再问那台。 */
export function askSpec(origin: Origin, workspace: string, slice: PlanSlice, cell: PlanCell, first: PlanAsk): NewSessionSpec {
  const at = { workspace, slice: slice.name, id: cell.id };
  return {
    origin,
    lockMachine: true,
    cwd: workspace,
    lockCwd: true,
    prompt: first.line,
    ask: {
      title: copyText("plan.ask.title", { title: cell.title ?? cell.id }),
      alias: first.alias,
      relined: async (alias: string) => (await askPlan(origin, { ...at, alias })).line,
    },
  };
}

/** ［问这一块…］：先问那台，再开框；问不到 ⇒ 那一句（不开框）。 */
export async function openPlanAsk(origin: Origin, workspace: string, slice: PlanSlice, cell: PlanCell): Promise<void> {
  let first: PlanAsk;
  try {
    first = await askPlan(origin, { workspace, slice: slice.name, id: cell.id });
  } catch (e) {
    failToast(copyText("plan.ask.failed"), e, { level: "error" });
    return;
  }
  await openNewSession(askSpec(origin, workspace, slice, cell, first));
}
