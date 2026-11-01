/**
 * **结束会话只有这一个确认框，结局只有这一种说法**：右键「结束会话…」· 多选「结束会话（n）」· 终端名单「结束所选」·
 * 起会话占位上的［结束］都用它（A2-17-12-3）。框里只列要结束的那几条的名字、会中断什么 · 会保留什么；
 * 能不能结束由那台判（脸上的 `can.end`，终端名单那一行的句柄），结束不成的那一句也由那台写，这里只照摆。
 */
import { confirmDialog, type ConfirmFn, type ConfirmSpec } from "./kit/dialog";
import { copyText } from "./copy-table";
import { recentToasts, toast, undoToast, type ToastRecord } from "./kit/toast";
import { showMessage } from "./status-messages";
import type { SessionsBatchReply } from "./generated/SessionsBatchReply";

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

/**
 * 一批会话 · 一批终端的回包（`sessions-batch` · `terminals-end`）：那一句 · 逐条 · 复制详情由本机后端合好，这里只照摆（逐条进「消息」那一条，［查看］展开它）。
 * `undo` 给了（撤得回的批量：隐藏）⇒ 同一条上先［撤销］（`Ctrl Z` 同样），再［查看］。
 */
export function sayReply(reply: Pick<SessionsBatchReply, "said" | "failed" | "lines" | "copyDetail">, undo?: { run: () => void; settle: () => void }): void {
  const made: { rec?: ToastRecord } = {};
  const view = reply.lines.length > 0 ? { label: copyText("tabBatch.result.view"), run: () => made.rec && showMessage(made.rec), toastOnly: true } : undefined;
  const level = reply.failed ? "error" : "success";
  if (undo) undoToast(reply.said, undo.run, undo.settle, { level, more: reply.lines, detail: reply.copyDetail, ...(view ? { action: view } : {}) });
  else toast(reply.said, "", { level, more: reply.lines, action: view, detail: reply.copyDetail });
  made.rec = recentToasts()[0];
}
