/**
 * **历史注解与上次账号表，经通道问常驻后端**（注解的读写者是本机常驻后端，文件原地不动）。
 *
 * 清单那一族（平铺会话清单）住 `history-list-reads.ts`。这里只剩：改注解（星标 / 改标题 / 隐藏）· 删会话后连带删注解 ·
 * 「这台上每条会话上次用哪个号起」（三条都按类型问：入参 · 回包从后端类型生成）；外加两处共用的东西：形状不对的那一种错（[`HistoryShapeError`]）与给人看的那句话（[`historyReasonOf`]）。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 注解几问 10 秒（读写一份小文件）。
 */
import { budgetWithin, callTyped, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { Entry } from "./generated/Entry";
import type { Patch } from "./generated/Patch";

// ─── 成品的形状 ───

/** 一条注解（`history-annotate` 回的那一条）：形状从后端类型生成。 */
export type EntryMetadata = Entry;

const ANNOTATION_BUDGET_MS = 10_000;

/** 形状不对（本机后端回的东西认不出）。`message` 只是细目（英文，进日志）；给人看的那句由 [`historyReasonOf`] 按码取。 */
export class HistoryShapeError extends ReplyUnreadable {
  constructor(what: string) {
    super(what);
    this.name = "HistoryShapeError";
  }
}

/** 一次失败 ⇒ 给人看的那句话（`chan-caller.ts::saidFrom`；清单与搜索都是本机后端答的，远端那台的原因由它说成人话带回来）。 */
export function historyReasonOf(e: unknown): string {
  return saidFrom(e, LOCAL_ORIGIN);
}

// ─── 问 ───

/** 改一条注解（星标 / 改名 / 隐藏）。缺格或 `null` = 不改；标题给空白串 = 清空。注解跟着会话住在那台：问的是会话所在那台（`origin`）。 */
export async function annotate(origin: Origin, sessionId: string, patch: Patch): Promise<EntryMetadata> {
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  const { entry } = await callTyped(origin, "history-annotate", { sid: sessionId, patch }, budget);
  return entry;
}

/** 删会话之后连带删那一条注解（会话所在那台的；删不掉只记一行：孤儿注解不害人）。 */
export async function forgetAnnotation(origin: Origin, sessionId: string): Promise<void> {
  try {
    const budget = budgetWithin(ANNOTATION_BUDGET_MS);
    await callTyped(origin, "history-forget", { sid: sessionId }, budget);
  } catch (e) {
    console.warn(`会话 ${sessionId} 删了，它那条注解没删掉（留着不害人）:`, e);
  }
}

/** `origin` 那台记着的 sid → 上次用哪个号起（只含真记过的那几条；会话跑在哪台就问哪台）。 */
export async function lastAccounts(origin: Origin): Promise<Record<string, string>> {
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  const { accounts } = await callTyped(origin, "history-last-accounts", {}, budget);
  return accounts;
}
