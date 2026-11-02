/**
 * **问图**（计划页一格详情头的［问图…］）：开全产品那一个新建会话框的问图样子。
 *
 * 开之前问计划页那台一次 `plan-ask`：框顶那一句 · 开口那句 · 起在哪那一句（`runs`）那台写好，别名预选那台记着的问图用的那条；
 * 那台没有 tmux（开口那句只跟 tmux）⇒ 那台拒，这里照它那一句报、不开框；那台离线 ⇒ 详情头那颗本来就灰着（`plan.ts`）。
 * 框里只别名 · 目录（锁在工作区根）· 开口那句三格，机器锁那台、不出那一格；改选别名 ⇒ 带上那条再问一次（那台记新的、拼新的两句）。
 * 起会话照旧走 `session-open`。问图不补足：同一格连问两次 ＝ 起两条。
 */
import { copyText } from "./copy-table";
import { failToast } from "./kit/toast";
import { openNewSession, type NewSessionSpec } from "./new-session";
import { askPlan, type PlanAsk, type PlanCell, type PlanSlice } from "./plan-reads";
import type { Machine } from "./ipc/origin";

/** 问图那一下开的框：照那台回的那一份预填；改选别名再问那台。 */
export function askSpec(origin: Machine, workspace: string, slice: PlanSlice, cell: PlanCell, first: PlanAsk): NewSessionSpec {
  const at = { workspace, slice: slice.name, id: cell.id };
  return {
    origin,
    lockMachine: true,
    cwd: workspace,
    lockCwd: true,
    prompt: first.line,
    ask: {
      title: first.title,
      alias: first.alias,
      runs: first.runs,
      relined: async (alias: string) => {
        const r = await askPlan(origin, { ...at, alias });
        return { line: r.line, runs: r.runs };
      },
    },
  };
}

/** ［问图…］：先问那台，再开框；问不到 ⇒ 那一句（不开框）。 */
export async function openPlanAsk(origin: Machine, workspace: string, slice: PlanSlice, cell: PlanCell): Promise<void> {
  let first: PlanAsk;
  try {
    first = await askPlan(origin, { workspace, slice: slice.name, id: cell.id });
  } catch (e) {
    failToast(copyText("plan.ask.failed"), e, { level: "error" });
    return;
  }
  await openNewSession(askSpec(origin, workspace, slice, cell, first));
}
