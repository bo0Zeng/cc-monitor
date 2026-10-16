/**
 * 离线那台的上次值：每台最近一次读成的那一份（账号清单 · 「文件与数据」那一份）交本机后端记下（`last-seen-write`，
 * 写它自己的 `~/.cc-monitor/last-seen.json`），断开时问回来（`last-seen-read`）画「上次的」——跨重启还在。
 * 界面不自己写文件、不记交过什么：读成一次交一次，记下的时刻是最后一次读成的那一刻。本机那台不记（问不到本机后端时也问不到这一份）。
 * 机器从列表删掉（撤销条到点）⇒ 交本机后端清掉那台（`last-seen-write` 带 `forget`）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { isObj } from "./ipc/decode";
import type { SeenKind } from "./generated/SeenKind";
import type { LastSeen } from "./generated/LastSeen";

export type { SeenKind };

const BUDGET_MS = 10_000;

/** 读成了一份 ⇒ 交本机后端记下（失败只进日志：它只是缓存）。 */
export async function rememberSeen(origin: Origin, kind: SeenKind, value: unknown): Promise<void> {
  if (isLocalOrigin(origin) || !isObj(value)) return;
  try {
    await callTyped(LOCAL_ORIGIN, "last-seen-write", { origin, kind, value }, budgetWithin(BUDGET_MS));
  } catch (e) {
    console.warn(`[last-seen] ${origin} 的 ${kind} 没记下：`, e);
  }
}

/** 那台上次读成的那一份（`data` 那一样后端按回包类型核过，不是这一版就说读不懂）；没有 / 问不到 ⇒ `null`。 */
export async function recallSeen<K extends SeenKind>(origin: Origin, kind: K): Promise<LastSeen[K] | null> {
  if (isLocalOrigin(origin)) return null;
  try {
    const got = await callTyped(LOCAL_ORIGIN, "last-seen-read", { origin }, budgetWithin(BUDGET_MS));
    return got[kind];
  } catch (e) {
    console.warn(`[last-seen] ${origin} 的上次值问不到：`, e);
    return null;
  }
}

/** 那台从列表删掉了 ⇒ 交本机后端清掉它的上次值（失败只进日志：它只是缓存，最多 64 台、超了丢最旧）。 */
export async function forgetSeen(origin: Origin): Promise<void> {
  if (isLocalOrigin(origin)) return;
  try {
    await callTyped(LOCAL_ORIGIN, "last-seen-write", { origin, forget: true }, budgetWithin(BUDGET_MS));
  } catch (e) {
    console.warn(`[last-seen] ${origin} 的上次值没清掉：`, e);
  }
}
