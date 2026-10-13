/**
 * **删一个会话** —— 界面经通道直说那台机器的后端。
 *
 * 本机远端同一条 `chan.call(origin, …)`：`files-delete-session {sid} → {path}`（只收 sid，落点由后端按 sid 在它自己的记录树里找、必须恰是 `<sid>.jsonl`）。
 * 分叉本身随起新会话那一个请求写（`session-new` 带 `forkFrom`）；分叉源会话那三格是生成类型 `ForkLaunch`（`session-new-facts` 回包里带着）。
 *
 * monitor 那条转交的 Tauri 命令删了；成品按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不返回空壳）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { exactKeys, isObj } from "./ipc/decode";

/** 删一份文件 ＋ 回程。 */
const DELETE_BUDGET_MS = 30_000;

function shaped(v: unknown, what: string, keys: string[]): Record<string, unknown> {
  if (!isObj(v)) throw new Error(`${what} reply shape mismatch: not an object`);
  if (!exactKeys(v, keys)) throw new Error(`${what} reply shape mismatch: keys ${Object.keys(v).join(",")}`);
  return v;
}

/** 删 `origin` 那台机器上的会话 `sid`。成品恰好一格 `path`（删掉的那份；非 UTF-8 路径是 `{b16}` 形，界面不用它）。 */
export async function deleteSession(origin: Origin, sid: string): Promise<void> {
  const budget = budgetWithin(DELETE_BUDGET_MS);
  const body = jsonBody({ sid });
  const reply = await chan.call(origin, "files-delete-session", body, budget);
  shaped(readJson(reply), "files-delete-session", ["path"]);
}
