/**
 * 额度账的界面口：`quota-read`（会话所在那台；本机远端同一条 `chan.call(origin, …)`）。
 * 轮换那一组在 `rotation-view-reads.ts`。判定全在后端：这里只按形状收、交给画的那几处。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { takeNames, type QuotaRead } from "./acct-words";

/** 读一份额度账（读盘 ＋ 回程）。 */
const READ_BUDGET_MS = 15_000;

function bad(what: string): never {
  throw new Error(`quota reply shape mismatch: ${what}`); // 程序员错误，刻意英文
}

function obj(v: unknown, what: string): Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : bad(`${what} is not an object`);
}

function arr(v: unknown, what: string): unknown[] {
  return Array.isArray(v) ? v : bad(`${what} is not an array`);
}

const STATES = new Set(["present", "absent", "unreadable"]);

/** `quota-read` 的回包（形状不对 ⇒ 抛）。 */
export function decodeQuotaRead(v: unknown): QuotaRead {
  const o = obj(v, "reply");
  if (!STATES.has(o.state as string)) bad(`state ${JSON.stringify(o.state)}`);
  if (typeof o.now !== "number") bad("now");
  for (const [i, a] of arr(o.accounts, "accounts").entries()) {
    const x = obj(a, `accounts[${i}]`);
    if (
      typeof x.account !== "string" ||
      typeof x.seenAt !== "number" ||
      typeof x.state !== "string" ||
      !Array.isArray(x.slots)
    )
      bad(`accounts[${i}]`);
  }
  for (const [i, u] of arr(o.unseen, "unseen").entries()) {
    const x = obj(u, `unseen[${i}]`);
    if (typeof x.account !== "string" || typeof x.kind !== "string")
      bad(`unseen[${i}]`);
  }
  arr(o.usableNow, "usableNow");
  for (const [i, c] of arr(o.candidates, "candidates").entries()) {
    const x = obj(c, `candidates[${i}]`);
    if (typeof x.agent !== "string" || typeof x.account !== "string" || typeof x.kind !== "string")
      bad(`candidates[${i}]`);
  }
  takeNames(o.names);
  return o as unknown as QuotaRead;
}

// 每一问各写一处 `chan.call`，操作名是字面量（通信层判据按字面量认是哪条帧命令）。

export async function readQuota(origin: Origin): Promise<QuotaRead> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeQuotaRead(
    readJson(await chan.call(origin, "quota-read", body, budget)),
  );
}
