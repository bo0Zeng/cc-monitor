/**
 * 额度账的界面口：`quota-read`（会话所在那台；本机远端同一条 `chan.call(origin, …)`）。
 * 轮换那一组在 `rotation-view-reads.ts`。判定全在后端：回包类型从 Rust 生成、形状由握手时的指纹核过，这里只收下号名表、交给画的那几处。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { takeNames, type QuotaRead } from "./acct-words";

/** 读一份额度账（读盘 ＋ 回程）。 */
const READ_BUDGET_MS = 15_000;

/** 收下一份成品（问回来的 · 推来的同一处）：号名 / 位名表换成它带的那一张。 */
export function tookQuota(r: QuotaRead): QuotaRead {
  takeNames(r.names);
  return r;
}

export async function readQuota(origin: Origin): Promise<QuotaRead> {
  return tookQuota(await callTyped(origin, "quota-read", {}, budgetWithin(READ_BUDGET_MS)));
}
