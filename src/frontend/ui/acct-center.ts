/**
 * 额度与轮换的**取数**：什么时候问哪台、问回来放进 `appStore` 哪一格。画的那几处（状态栏按钮 · 悬停卡 · 账号面板 ·
 * 换号条 · 提示条 · 标签页）只订 `appStore.quota` / `sessionRotation` / `rotationRules`，不自己问。
 *
 * - 问的时机全是事件：tab 来了（新 sid）· 那台推 `changed {quota | rotation | rotation_rules}`（额度账 ⇒ 重读 `quota-read`；某个 sid ⇒ 重问那一个；
 *   又接上了 / 丢了格 ⇒ 那台全部重问）· 面板打开 · 写了之后。零定时器。
 * - 判定全在后端；这里不判「能不能换 / 该不该换」。
 */
import { appStore, putIn } from "./app-store";
import type { Origin } from "./ipc/origin";
import { readQuota, readRules, readSessionRotation } from "./quota-reads";
import { changedKeys, type Changed } from "./changed-stream";

/** 每台此刻有 tab 的会话（rotation-session-read 一批问这么多）。 */
const known = new Map<Origin, Set<string>>();

export async function refreshQuota(origin: Origin): Promise<void> {
  try {
    putIn(appStore.quota, origin, await readQuota(origin));
  } catch (e) {
    console.warn(`[acct] quota-read [${origin}] 失败：`, e);
    putIn(appStore.quota, origin, null);
  }
}

export async function refreshRules(origin: Origin): Promise<void> {
  try {
    putIn(appStore.rotationRules, origin, await readRules(origin));
  } catch (e) {
    console.warn(`[acct] rotation-rules-read [${origin}] 失败：`, e);
    putIn(appStore.rotationRules, origin, null);
  }
}

export async function refreshSessions(origin: Origin, sids: readonly string[]): Promise<void> {
  if (sids.length === 0) return;
  try {
    const got = await readSessionRotation(origin, [...sids]);
    const next = new Map(appStore.sessionRotation.get());
    for (const [sid, read] of Object.entries(got.sessions)) next.set(sid, { origin, now: got.now, read });
    appStore.sessionRotation.set(next);
  } catch (e) {
    console.warn(`[acct] rotation-session-read [${origin}] 失败：`, e);
  }
}

/**
 * tab 集合变了 ⇒ 新出现的会话问一次（每台一批）；那台第一次出现 ⇒ 额度账与默认轮换也问一次。
 * 已经问过的不重问（之后靠推送）。
 */
export function syncSessions(tabs: readonly { sessionId: string; origin: Origin }[]): void {
  const fresh = new Map<Origin, string[]>();
  for (const t of tabs) {
    let set = known.get(t.origin);
    if (!set) {
      set = new Set();
      known.set(t.origin, set);
      void refreshQuota(t.origin);
      void refreshRules(t.origin);
    }
    if (set.has(t.sessionId)) continue;
    set.add(t.sessionId);
    fresh.set(t.origin, [...(fresh.get(t.origin) ?? []), t.sessionId]);
  }
  for (const [origin, sids] of fresh) void refreshSessions(origin, sids);
}

/** 那台推来额度 / 会话轮换 / 规则表变了（`events.ts` 的 `onChanged`，主题 `quota` · `rotation` · `rotation_rules`）。`change.all` ⇒ 那一样整份重问。 */
export function onQuotaChanged(origin: Origin, topic: "quota" | "rotation" | "rotation_rules", change: Changed): void {
  if (topic === "quota") void refreshQuota(origin);
  else if (topic === "rotation_rules") void refreshRules(origin);
  else {
    const keys = changedKeys(change);
    const all = change.all || keys.length !== change.cells.length;
    const sids = all ? [...(known.get(origin) ?? [])] : keys.filter((s) => known.get(origin)?.has(s));
    void refreshSessions(origin, sids);
  }
}
