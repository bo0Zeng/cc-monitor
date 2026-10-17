/**
 * **历史注解与上次账号表，经通道问常驻后端**（注解的读写者是本机常驻后端，文件原地不动）。
 *
 * 清单那一族（平铺会话清单）住 `history-list-reads.ts`。这里只剩：改注解（星标 / 改标题 / 隐藏）· 删会话后连带删注解 ·
 * 「这台上每条会话上次用哪个号起」（三条都按类型问：入参 · 回包从后端类型生成）。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 注解几问 10 秒（读写一份小文件）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { Annotated } from "./generated/Annotated";
import type { Patch } from "./generated/Patch";

// ─── 成品的形状 ───


const ANNOTATION_BUDGET_MS = 10_000;

// ─── 问 ───

/**
 * 改一条注解（星标 / 改名 / 隐藏）。缺格或 `null` = 不改；标题给空白串 = 清空。注解跟着会话住在那台：问的是会话所在那台（`origin`）。
 * 回改完的那一条 ＋ 改之前单个隐藏那一格（`prev`，撤销隐藏照它交回去）。
 */
export async function annotate(origin: Origin, sessionId: string, patch: Patch): Promise<Annotated> {
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  return callTyped(origin, "history-annotate", { sid: sessionId, patch }, budget);
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
