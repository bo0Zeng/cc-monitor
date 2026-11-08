/**
 * 额度账的界面口：`quota-read`（会话所在那台；本机远端同一条 `chan.call(origin, …)`）。
 * 轮换那一组在 `rotation-view-reads.ts`。判定全在后端：回包类型从 Rust 生成、形状由握手时的指纹核过，这里只核一下位名表、交给画的那几处。
 * 号名与账号页 · 恢复菜单 · 新会话框那几格额度不在这里：都在那台 `accounts-list` 那一行（`account-line.ts`）。
 */
import { budgetWithin, callTyped, ReplyUnreadable } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";
import type { QuotaRead } from "./generated/QuotaRead";

export type { QuotaRead } from "./generated/QuotaRead";

/** 读一份额度账（读盘 ＋ 回程）。 */
const READ_BUDGET_MS = 15_000;

/**
 * 收下一份成品（问回来的 · 推来的同一处）。
 * 有一格的语义位不在同一份的 `names.slots` 里 ⇒ 两端契约对不上：整份照「回包读不出」抛（不拿码当名字；同手机 `usageView`）。
 */
export function tookQuota(origin: Machine, r: QuotaRead): QuotaRead {
  const lacking = r.accounts.flatMap((a) => a.slots).find((s) => !(s.slot in r.names.slots));
  if (lacking) throw new ReplyUnreadable(origin, `quota-read reply names.slots lacks ${lacking.slot}`);
  return r;
}

export async function readQuota(origin: Machine): Promise<QuotaRead> {
  return tookQuota(origin, await callTyped(origin, "quota-read", {}, budgetWithin(READ_BUDGET_MS)));
}
