/**
 * 轮换（第五版）界面那几块的读与写：都是 typed 命令（入参 · 回包从 Rust 类型生成），问会话或策略所在的那一台。
 * 判定全在后端：这里只把手势翻成命令、把回包交给画的那一处；收不下 ⇒ 抛 `ChanError`（调用方照 `saidFrom` 说一句）。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, refusalOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotationViewReply } from "./generated/RotationViewReply";
import type { RotTarget } from "./generated/RotTarget";

/** 读盘 ＋ 跑策略 ＋ 推演 ＋ 回程。 */
const BUDGET_MS = 15_000;

/** 一条轮换命令的期限（读盘 ＋ 跑策略 ＋ 推演 ＋ 回程）。 */
export function rotBudget(): ReturnType<typeof budgetWithin> {
  return budgetWithin(BUDGET_MS);
}

/** 那台的策略目录。 */
export function readStrategies(
  origin: Origin,
): Promise<RotationStrategiesReply> {
  return callTyped(origin, "rotation-strategies", {}, rotBudget());
}

/** 一个会话（或一份策略）的四块要画的全部；推 `hours` 小时。 */
export function readView(
  origin: Origin,
  target: RotTarget,
  hours: number,
): Promise<RotationViewReply> {
  return callTyped(origin, "rotation-view", { target, hours }, rotBudget());
}

/** 后端回的拒绝码（不是拒绝 ⇒ `null`）与它带的数据。 */
export function refusedCode(
  e: unknown,
): { code: string; data?: unknown } | null {
  const err = e instanceof ChanError ? e.error : undefined;
  if (!err || err.layer !== "peer" || err.why !== "refused") return null;
  const r = refusalOf(err.body);
  return r ? { code: r.code, data: r.data } : null;
}

