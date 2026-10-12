/**
 * 「会打断什么」那一问（`session-interrupts`）：经通道问会话所在那台的后端（不猜、不补）。
 * 判定只住后端；界面那一侧的 2 秒等待在 `kit/interrupts.ts`（通道这一跳的期限同值，到点就当没答）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { INTERRUPTS_WITHIN_MS, type Interrupts } from "./kit/interrupts";

/** 问 `origin` 那台：动 `sid` 这个会话会打断什么（typed 帧命令：回包类型从 Rust 生成，形状由握手指纹核过）。 */
export async function askSessionInterrupts(origin: Origin, sid: string): Promise<Interrupts> {
  const r = await callTyped(origin, "session-interrupts", { sid }, budgetWithin(INTERRUPTS_WITHIN_MS));
  return { families: r.families.map((f) => ({ family: f.family, names: [...f.names] })) };
}
