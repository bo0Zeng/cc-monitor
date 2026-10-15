/**
 * 额度与轮换的**取数**：什么时候问哪台、问回来放进 `appStore` 哪一格。画的那几处（状态栏按钮 · 悬停卡 · 标签页）
 * 只订 `appStore.quota` / `sessionRotation`，不自己问。
 *
 * - 问的时机全是事件：tab 来了（新 sid）· 那台推 `changed {quota | rotation | rotation_strategies}`（额度账 ⇒ 重读 `quota-read`；
 *   某个 sid ⇒ 重问那一个的 `rotation-view`；策略目录 · 哪个会话用哪份变了 ⇒ 那台有 tab 的会话全部重问）·
 *   又接上了 / 丢了格 ⇒ 那台全部重问。零定时器。
 * - 判定全在后端；这里不判「能不能换 / 该不该换」。
 */
import { appStore, putIn } from "./app-store";
import type { Origin } from "./ipc/origin";
import { readQuota, tookQuota } from "./quota-reads";
import type { QuotaRead } from "./acct-words";
import { readSessionView } from "./rotation-view-reads";
import { pushedProducts, type Changed } from "./changed-stream";
import { invalidateAccountsCache } from "./account-reads";

/** 每台此刻有 tab 的会话。 */
const known = new Map<Origin, Set<string>>();

export async function refreshQuota(origin: Origin): Promise<void> {
  try {
    putIn(appStore.quota, origin, await readQuota(origin));
  } catch (e) {
    console.warn(`[acct] quota-read [${origin}] 失败：`, e);
    putIn(appStore.quota, origin, null);
  }
}

/** 一批会话各问一次 `rotation-view`（逐个；一个失败不挡别的，那一格照旧）。 */
export async function refreshSessions(origin: Origin, sids: readonly string[]): Promise<void> {
  await Promise.all(
    sids.map(async (sid) => {
      try {
        putIn(appStore.sessionRotation, sid, { origin, read: await readSessionView(origin, sid) });
      } catch (e) {
        console.warn(`[acct] rotation-view [${origin}] ${sid} 失败：`, e);
      }
    }),
  );
}

/**
 * tab 集合变了 ⇒ 新出现的会话问一次；那台第一次出现 ⇒ 额度账也问一次。已经问过的不重问（之后靠推送）。
 */
export function syncSessions(tabs: readonly { sessionId: string; origin: Origin }[]): void {
  const fresh = new Map<Origin, string[]>();
  for (const t of tabs) {
    let set = known.get(t.origin);
    if (!set) {
      set = new Set();
      known.set(t.origin, set);
      void refreshQuota(t.origin);
    }
    if (set.has(t.sessionId)) continue;
    set.add(t.sessionId);
    fresh.set(t.origin, [...(fresh.get(t.origin) ?? []), t.sessionId]);
  }
  for (const [origin, sids] of fresh) void refreshSessions(origin, sids);
}

/**
 * 那台推来额度 / 会话轮换 / 策略目录变了（`events.ts` 的 `onChanged`，主题 `quota` · `rotation` · `rotation_strategies`）。
 * 额度帧里带了成品 ⇒ 同一处收下（同一条命令的回包，形状由指纹核过）、直接放进 store；轮换那两种不带成品 ⇒ 重问。`all`（期间可能漏了）⇒ 那台全部重问。
 */
export function onQuotaChanged(origin: Origin, topic: "quota" | "rotation" | "rotation_strategies", change: Changed): void {
  if (topic === "quota") {
    // 账号清单每号带「5h 那一格」：额度变了，下一次问清单不拿缓存。
    invalidateAccountsCache(origin);
    const p = pushedProducts(change, (v) => tookQuota(v as QuotaRead));
    const q = p.got.get(null);
    if (q !== undefined) putIn(appStore.quota, origin, q);
    if (p.all || (q === undefined && p.ask.length > 0)) void refreshQuota(origin);
    return;
  }
  const keys = change.cells.map((c) => c.key);
  const mine = known.get(origin);
  const every = topic === "rotation_strategies" || change.all || keys.includes(null);
  const sids = every ? [...(mine ?? [])] : keys.filter((s): s is string => s !== null && (mine?.has(s) ?? false));
  void refreshSessions(origin, sids);
}
