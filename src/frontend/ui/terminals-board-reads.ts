/**
 * 「终端」视图那张名单的读写口：本机后端 `terminals-board` 问到各台、合成一份再答（每台一组 · 行排好字写好 · 筛选四格带数）；
 * 结束几行：有会话的那几行每台一次 `sessions-stop`（同 `killInTmux` 那一条），没会话的按 tmux 会话名 `kill`。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { settleTyped } from "./control-said";
import { callStop } from "./tab-batch-run";
import { killSession } from "./tmux-control";
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import type { TerminalsBoard } from "./generated/TerminalsBoard";
import type { BoardRow } from "./generated/BoardRow";

/** 本机后端问到各台（各台起一两发 tmux ＋ 回程）再合：等这么久。 */
const BOARD_BUDGET_MS = 20_000;

/** 问这几台、合一份（同抽屉终端页那一问，不自报前端）。失败 ⇒ 抛 `ControlError`（那一句已经说好）。 */
export async function readBoard(machines: readonly Origin[]): Promise<TerminalsBoard> {
  const req = { machines: [...machines] };
  return settleTyped(LOCAL_ORIGIN, "terminals-board", callTyped(LOCAL_ORIGIN, "terminals-board", req, budgetWithin(BOARD_BUDGET_MS)));
}

/** 名单里的一行连它在哪台。 */
export interface BoardPick {
  origin: Origin;
  row: BoardRow;
}

/** 结束这几行：先确认（同一种确认框），再每台交命令；没结束的数一句。 */
export async function endRows(items: readonly BoardPick[], confirm: ConfirmFn = confirmDialog): Promise<void> {
  const n = String(items.length);
  const ok = await confirm({
    title: copyText("terminalsView.end.title", { n }),
    rows: [{ label: copyText("kit.interrupts.cut"), items: items.map((i) => i.row.title) }],
    action: copyText("terminalsView.select.end", { n }),
    danger: true,
  });
  if (!ok) return;
  const byOrigin = new Map<Origin, BoardPick[]>();
  for (const i of items) byOrigin.set(i.origin, [...(byOrigin.get(i.origin) ?? []), i]);
  const done = await Promise.all(
    [...byOrigin].map(async ([origin, list]): Promise<number> => {
      const sids = list.flatMap((i) => (i.row.sid ? [i.row.sid] : []));
      let ended = 0;
      if (sids.length > 0) {
        try {
          ended += (await callStop(origin, sids)).filter((r) => r.outcome === "done").length;
        } catch {
          // 没结束的照数进下面那一句。
        }
      }
      for (const i of list.filter((i) => !i.row.sid)) {
        try {
          await killSession(origin, i.row.tmux_name);
          ended++;
        } catch {
          // 同上。
        }
      }
      return ended;
    }),
  );
  const ended = done.reduce((a, b) => a + b, 0);
  const left = items.length - ended;
  if (left > 0) toast(copyText("terminalsView.end.failed", { n: String(left) }), "");
  else toast(copyText("terminalsView.end.done", { n }), "", { level: "success" });
}
