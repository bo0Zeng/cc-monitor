/**
 * 一条会话的号那一份成品的读口：问会话所在那台的 `session-account`（本机远端同一条 `chan.call(origin, …)`）。
 * typed 命令（入参 · 回包从 Rust 类型生成），形状由握手指纹管。用哪个号 · 比不比默认号 · 写成什么字全在那台；这里只问、交出去。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { SessionAccountReply } from "./generated/SessionAccountReply";

/** 读账本 · 活会话表 · 账号库 · 额度账 ＋ 回程。 */
const BUDGET_MS = 15_000;

/** 一条会话的号（那台写好的那一份）。失败照抛。 */
export async function readSessionAccount(origin: Origin, sid: string): Promise<SessionAccountReply> {
  return await callTyped(origin, "session-account", { sid }, budgetWithin(BUDGET_MS));
}
