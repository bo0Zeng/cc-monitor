/**
 * 〔MIG-3b · `设计/05 §14.3` C 组 · `§9` 第 12 条〕**会话的两件「改世界」的事：分叉 · 删** —— 界面经通道直说那台机器的后端。
 *
 * 本机远端同一条 `chan.call(origin, …)`：
 * - 分叉：`session-fork {sid, uuid} → {sessionId, jsonlPath}`（后端就地读 → `branch-core` 变换 → `O_EXCL` 新建，
 *   `src/backend/control/fork_write.rs`）。sid / uuid 的放行判定在后端入口（`INVARIANTS §47` ①，本侧 = 真去用它的那一侧），界面零判。
 * - 删：`files-delete-session {sid} → {path}`（只收 sid，落点由后端按 sid 在它自己的记录树里找、必须恰是 `<sid>.jsonl`）。
 *
 * monitor 那两条转交的 Tauri 命令删了；成品按形状严格收
 * （多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不返回空壳：分叉已经落盘而这边读不出结果时，重试会多出一份孤儿分支）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 分叉的成品（线上形状由 `tests/__fixtures__/session-fork.golden.json` 钉住：后端测试产出它，{@link decodeFork} 读同一份）。 */
export type BranchResult = { sessionId: string; jsonlPath: string };

/** 读一份 jsonl ＋ 写一份新文件：正常毫秒级；30 s 给巨型会话与慢链路留余量（与它上一个住址 monitor `FORK_BUDGET` 同值）。 */
const FORK_BUDGET_MS = 30_000;
/** 删一份文件 ＋ 回程。 */
const DELETE_BUDGET_MS = 30_000;

function exactly(v: unknown, what: string, keys: string[]): Record<string, unknown> {
  if (v === null || typeof v !== "object" || Array.isArray(v)) throw new Error(`${what} reply shape mismatch: not an object`);
  const o = v as Record<string, unknown>;
  const got = Object.keys(o).sort().join(",");
  if (got !== [...keys].sort().join(",")) throw new Error(`${what} reply shape mismatch: keys ${got}`);
  return o;
}

/** `session-fork` 的成品 → {@link BranchResult}（恰好两格、都是串）。 */
export function decodeFork(v: unknown): BranchResult {
  const o = exactly(v, "session-fork", ["sessionId", "jsonlPath"]);
  if (typeof o.sessionId !== "string" || typeof o.jsonlPath !== "string") {
    throw new Error("session-fork reply shape mismatch: a field is not a string");
  }
  return { sessionId: o.sessionId, jsonlPath: o.jsonlPath };
}

/** 在 `origin` 那台机器上从 `uuid` 那条消息分叉 `sid`。 */
export async function forkSession(origin: Origin, sid: string, uuid: string): Promise<BranchResult> {
  const budget = budgetWithin(FORK_BUDGET_MS);
  const body = jsonBody({ sid, uuid });
  const reply = await chan.call(origin, "session-fork", body, budget);
  try {
    return decodeFork(readJson(reply));
  } catch (e) {
    // 分叉已经落盘、这边读不出结果 ⇒ 说清是契约问题、别重试（重试会多出一份孤儿分支）。
    throw new Error(copyText("sessionWrites.fork.badProduct", { e: e instanceof Error ? e.message : String(e) }));
  }
}

/** 删 `origin` 那台机器上的会话 `sid`。成品恰好一格 `path`（删掉的那份；非 UTF-8 路径是 `{b16}` 形，界面不用它）。 */
export async function deleteSession(origin: Origin, sid: string): Promise<void> {
  const budget = budgetWithin(DELETE_BUDGET_MS);
  const body = jsonBody({ sid });
  const reply = await chan.call(origin, "files-delete-session", body, budget);
  exactly(readJson(reply), "files-delete-session", ["path"]);
}
