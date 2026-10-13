/**
 * **删一个会话** —— 界面经通道直说那台机器的后端。
 *
 * 本机远端同一条 `chan.call(origin, …)`：`files-delete-session {sid} → {path}`（只收 sid，落点由后端按 sid 在它自己的记录树里找、必须恰是 `<sid>.jsonl`）。
 * 入参 · 回包的类型从后端生成（`generated/TypedCommands.ts`），两端形状由握手时的形状指纹核过。
 * 分叉本身随起新会话那一个请求写（`session-new` 带 `forkFrom`）；分叉源会话那三格是生成类型 `ForkLaunch`（`session-new-facts` 回包里带着）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/** 删一份文件 ＋ 回程。 */
const DELETE_BUDGET_MS = 30_000;

/** 删 `origin` 那台机器上的会话 `sid`（回包里那一格 `path` 是删掉的那份，界面不用它）。 */
export async function deleteSession(origin: Origin, sid: string): Promise<void> {
  await callTyped(origin, "files-delete-session", { sid }, budgetWithin(DELETE_BUDGET_MS));
}
