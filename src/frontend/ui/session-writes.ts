/**
 * **删一个会话 · 换它的模型与权限档** —— 界面经通道直说那台机器的后端。
 *
 * 本机远端同一条 `chan.call(origin, …)`：`files-delete-session {sid} → {path}`（只收 sid，落点由后端按 sid 在它自己的记录树里找、必须恰是 `<sid>.jsonl`）。
 * 入参 · 回包的类型从后端生成（`generated/TypedCommands.ts`），两端形状由握手时的形状指纹核过。
 * 分叉本身随起新会话那一个请求写（`session-new` 带 `forkFrom`）；分叉源会话那三格是生成类型 `ForkLaunch`（`session-new-facts` 回包里带着）。
 */
import { ChanError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, refusalOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { saidOfTransport } from "./control-said";
import type { Switched } from "./session-switch";

/** 删一份文件 ＋ 回程。 */
const DELETE_BUDGET_MS = 30_000;

/** 删 `origin` 那台机器上的会话 `sid`（回包里那一格 `path` 是删掉的那份，界面不用它）。 */
export async function deleteSession(origin: Origin, sid: string): Promise<void> {
  await callTyped(origin, "files-delete-session", { sid }, budgetWithin(DELETE_BUDGET_MS));
}

/** 换模型 · 切权限档：送那一家的那一行 / 按切档键、从记录里读回（那台自己的期限之外留回程）。 */
const SWITCH_BUDGET_MS = 30_000;

/** 换这条会话的模型（`session-model`）：结局照回包读；没回来 ⇒ 那一句（发出去了没等到回话 ⇒ 不说，界面写「结果未知」）。不抛。 */
export async function switchModel(origin: Origin, sid: string, model: string): Promise<Switched> {
  try {
    const r = await callTyped(origin, "session-model", { sid, model }, budgetWithin(SWITCH_BUDGET_MS));
    return { done: r.result === "done", text: r.model, said: r.said ?? null };
  } catch (e) {
    return { done: false, text: null, said: failedSaid(origin, e) };
  }
}

/** 切这条会话的权限档（`session-permission`）：同 [`switchModel`]，读回的字是那一档写给人看的字。 */
export async function switchPermission(origin: Origin, sid: string, mode: string): Promise<Switched> {
  try {
    const r = await callTyped(origin, "session-permission", { sid, mode }, budgetWithin(SWITCH_BUDGET_MS));
    return { done: r.result === "done", text: r.modeText, said: r.said ?? null };
  } catch (e) {
    return { done: false, text: null, said: failedSaid(origin, e) };
  }
}

/** 没回来的那一句：被拒 ⇒ 那台写的；发出去了没等到回话 ⇒ `null`（结果未知）；别的通道那几层 ⇒ 共用的那一句。 */
function failedSaid(origin: Origin, e: unknown): string | null {
  if (!(e instanceof ChanError)) return e instanceof Error ? e.message : String(e);
  const err = e.error;
  if (err.layer === "peer" && err.why === "refused") return refusalOf(err.body)?.message ?? null;
  if (err.layer === "hop" && err.reach !== "NotSent") return null;
  return saidOfTransport(origin, err);
}
