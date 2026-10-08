/**
 * 离线那台的上次值：每台最近一次读成的那一份（账号清单 · 「文件与数据」那一份）交本机后端记下（`last-seen-write`，
 * 写它自己的 `~/.cc-monitor/last-seen.json`），连不上时问回来（`last-seen-read`）画「上次的」——跨重启还在。
 * 界面不自己写文件；本机那台不记（问不到本机后端时也问不到这一份）。同一份内容这次运行里只交一次。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { isObj } from "./ipc/decode";

export type SeenKind = "accounts" | "data";

/** 一份上次值：原样那一份应答 ＋ 记下的时刻。 */
export interface Seen {
  atMs: number;
  value: unknown;
}

const BUDGET_MS = 10_000;
/** 这次运行里交过的那一份（键 ＝ 机器 ＋ 种类），同样的不再交。 */
const sent = new Map<string, string>();

/** 读成了一份 ⇒ 交本机后端记下（失败只进日志：它只是缓存）。 */
export async function rememberSeen(origin: Origin, kind: SeenKind, value: unknown): Promise<void> {
  if (isLocalOrigin(origin) || !isObj(value)) return;
  const key = `${origin}\u0000${kind}`;
  const text = JSON.stringify(value);
  if (sent.get(key) === text) return;
  sent.set(key, text);
  try {
    const body = jsonBody({ origin, kind, value });
    const budget = budgetWithin(BUDGET_MS);
    await chan.call(LOCAL_ORIGIN, "last-seen-write", body, budget);
  } catch (e) {
    sent.delete(key);
    console.warn(`[last-seen] ${origin} 的 ${kind} 没记下：`, e);
  }
}

/** 那台上次读成的那一份；没有 / 问不到 ⇒ `null`。 */
export async function recallSeen(origin: Origin, kind: SeenKind): Promise<Seen | null> {
  if (isLocalOrigin(origin)) return null;
  try {
    const body = jsonBody({ origin });
    const budget = budgetWithin(BUDGET_MS);
    const got = readJson(await chan.call(LOCAL_ORIGIN, "last-seen-read", body, budget));
    const e = isObj(got) ? got[kind] : null;
    if (!isObj(e) || typeof e.atMs !== "number" || !isObj(e.value)) return null;
    return { atMs: e.atMs, value: e.value };
  } catch (e) {
    console.warn(`[last-seen] ${origin} 的上次值问不到：`, e);
    return null;
  }
}

/** 仅供测试：清掉「交过的」那份记忆。 */
export function __resetLastSeenForTests(): void {
  sent.clear();
}
