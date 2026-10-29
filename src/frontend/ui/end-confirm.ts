/**
 * **结束会话只有这一个确认框，结局只有这一种说法**：右键「结束会话…」· 多选「结束会话（n）」· 终端名单「结束所选」·
 * 起会话占位上的［结束］都用它（A2-17-12-3）。框里只列要结束的那几条的名字、会中断什么 · 会保留什么；
 * 能不能结束由那台判（脸上的 `can.end`，终端名单那一行的句柄），结束不成的那一句也由那台写，这里只照摆。
 */
import { confirmDialog, type ConfirmFn, type ConfirmSpec } from "./kit/dialog";
import { copyText } from "./copy-table";
import { recentToasts, toast, type ToastRecord } from "./kit/toast";
import { showMessage } from "./status-messages";

/** 那个确认框：标题与按钮写个数；`note` ＝ 清单下那一行（多选里跳过了几个已结束的）。 */
export function endConfirm(titles: readonly string[], note?: string): ConfirmSpec {
  return {
    title: copyText("tabBatch.stop.title", { n: titles.length }),
    action: copyText("tabBatch.stop.action", { n: titles.length }),
    danger: true,
    rows: [
      { label: copyText("kit.interrupts.cut"), items: [copyText("tabBatch.stop.cuts")] },
      { label: copyText("kit.interrupts.keep"), items: [copyText("tabSessionActions.kill.keeps")] },
    ],
    list: [...titles],
    ...(note !== undefined ? { note } : {}),
  };
}

/** 问一句（同一个框）。 */
export async function askEnd(titles: readonly string[], ask: ConfirmFn = confirmDialog, note?: string): Promise<boolean> {
  return ask(endConfirm(titles, note));
}

/** 一条的结局：做成了 · 没成（那台那一句）· 跳过（那一格的 why）；`detail` ＝ 那台写的详情（复制详情用）。 */
export interface EndRow {
  title: string;
  outcome: "done" | "failed" | "skipped";
  why: string;
  detail?: string;
}

/**
 * 一批的结局 toast：一句汇总；没成的 · 跳过的逐条（各自那一句）进「消息」那一条，［查看］展开它。
 * `head` 给了 ⇒ 用它当汇总（多选里别的几项）；不给 ⇒ 结束的那一句（「已结束 k · 失败 ×m」）。
 */
export function sayRows(rows: readonly EndRow[], head?: (done: number, failed: number) => string): void {
  const n = (k: EndRow["outcome"]): number => rows.filter((r) => r.outcome === k).length;
  const done = n("done");
  const failed = n("failed");
  const said = head
    ? head(done, failed)
    : failed > 0
      ? copyText("sessionState.batch.someFailed", { done, failed })
      : copyText("sessionState.batch.done", { done });
  const lines = [
    ...rows.filter((r) => r.outcome === "failed").map((r) => copyText("tabBatch.result.failedLine", { title: r.title, why: r.why })),
    ...rows.filter((r) => r.outcome === "skipped").map((r) => copyText("tabBatch.result.skippedLine", { title: r.title, why: r.why })),
  ];
  const made: { rec?: ToastRecord } = {};
  const view = lines.length > 0 ? { label: copyText("tabBatch.result.view"), run: () => made.rec && showMessage(made.rec), toastOnly: true } : undefined;
  // 复制详情：没成的那几条各一段（那一句 ＋ 那台写的详情），段间空一行；成了的不进。
  const detail = rows
    .filter((r) => r.outcome === "failed" && r.detail)
    .map((r) => `${r.why}\n${r.detail}`)
    .join("\n\n");
  toast(said, "", { level: failed > 0 ? "error" : "success", more: lines, action: view, detail });
  made.rec = recentToasts()[0];
}

/** 结束的结局：只有一条 ⇒ 点名那一条（已结束「…」/ 结束失败 · …，那台那一句）；几条 ⇒ 一批那一种。 */
export function sayEnded(rows: readonly EndRow[]): void {
  const only = rows.length === 1 ? rows[0] : undefined;
  if (only?.outcome === "done") {
    toast(copyText("sessionState.kill.done", { title: only.title }), "", { level: "success" });
    return;
  }
  if (only?.outcome === "failed") {
    toast(copyText("tabSessionActions.kill.failed", { title: only.title }), only.why, { detail: only.detail });
    return;
  }
  sayRows(rows);
}
