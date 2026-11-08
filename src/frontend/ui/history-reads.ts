/**
 * **历史注解，经通道问常驻后端**（注解的读写者是本机常驻后端，文件原地不动）。
 *
 * 清单那一族（平铺会话清单）住 `history-list-reads.ts`。这里只剩：改注解（星标 / 改标题 / 隐藏）。删会话连带删注解在那台后端的 `files-delete-session` 里一并做完。
 * 按类型问：入参 · 回包从后端类型生成。会话上次用哪个号起的在那一条会话的号那一份成品里（`session-account-reads.ts`）。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 注解那一问 10 秒（读写一份小文件）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";
import type { Annotated } from "./generated/Annotated";
import type { Patch } from "./generated/Patch";

// ─── 成品的形状 ───


const ANNOTATION_BUDGET_MS = 10_000;

// ─── 问 ───

/**
 * 改一条注解（星标 / 改名 / 隐藏）。缺格或 `null` = 不改；标题给空白串 = 清空。注解跟着会话住在那台：问的是会话所在那台（`origin`）。
 * 回改完的那一条 ＋ 改之前单个隐藏那一格（`prev`，撤销隐藏照它交回去）。
 */
export async function annotate(origin: Machine, sessionId: string, patch: Patch): Promise<Annotated> {
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  return callTyped(origin, "history-annotate", { sid: sessionId, patch }, budget);
}
