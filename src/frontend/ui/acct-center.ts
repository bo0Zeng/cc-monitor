/**
 * 额度 · 轮换 · 会话的号的**取数**：什么时候问哪台、问回来放进 `appStore` 哪一格。画的那几处（状态栏按钮 · 悬停卡 · 标签页 · 会话面板）
 * 只订 `appStore.quota` / `sessionRotation` / `sessionAccount`，不自己问。
 *
 * - 问的时机全是事件：tab 来了（新 sid ⇒ 它那台的 `rotation-view` 与 `session-account`）· 那台推 `changed {quota | rotation | rotation_strategies}`
 *   （额度账 ⇒ 重读 `quota-read`，那台有 tab 的会话的号重问（用量在那一份里）；某个 sid ⇒ 重问那一个的 `rotation-view` 与 `session-account`；
 *   策略目录 · 哪个会话用哪份变了 ⇒ 那台有 tab 的会话 `rotation-view` 全部重问）· 那台账号清单变了 ⇒ 那台有 tab 的会话的号重问（与默认号比在那一份里）·
 *   又接上了 / 丢了格 ⇒ 那台全部重问。零定时器。
 * - 判定全在后端；这里不判「能不能换 / 该不该换 / 是哪个号 / 跟默认号一不一样」。
 */
import { appStore, putIn } from "./app-store";
import type { Origin } from "./ipc/origin";
import { detailOf, saidOf } from "./kit/detail";
import { readQuota, tookQuota } from "./quota-reads";
import { readSessionView } from "./rotation-view-reads";
import { readSessionAccount } from "./session-account-reads";
import { pushedProducts, type Changed } from "./changed-stream";

/** 每台此刻有 tab 的会话。 */
const known = new Map<Origin, Set<string>>();

/** 问一次那台的额度账。整问失败 ⇒ 那一格记下出错那一层写好的那一句与复制详情（界面照它说，不自己补一句）。 */
export async function refreshQuota(origin: Origin): Promise<void> {
  try {
    putIn(appStore.quota, origin, await readQuota(origin));
  } catch (e) {
    console.warn(`[acct] quota-read [${origin}] 失败：`, e);
    putIn(appStore.quota, origin, { state: "failed", said: saidOf(e), detail: detailOf(e) });
  }
}

/** 一批会话各问一次 `rotation-view`（逐个；一个失败不挡别的，那一格照旧）。 */
async function refreshRotations(origin: Origin, sids: readonly string[]): Promise<void> {
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

/** 一批会话各问一次 `session-account`（逐个；一个失败不挡别的，那一格照旧）。 */
async function refreshAccounts(origin: Origin, sids: readonly string[]): Promise<void> {
  await Promise.all(
    sids.map(async (sid) => {
      try {
        putIn(appStore.sessionAccount, sid, { origin, read: await readSessionAccount(origin, sid) });
      } catch (e) {
        console.warn(`[acct] session-account [${origin}] ${sid} 失败：`, e);
      }
    }),
  );
}

/** 一批会话的轮换与号各问一次。 */
export async function refreshSessions(origin: Origin, sids: readonly string[]): Promise<void> {
  await Promise.all([refreshRotations(origin, sids), refreshAccounts(origin, sids)]);
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

/** 那台有 tab 的会话的号全部重问；不给 ⇒ 各台。 */
function reaskAccounts(origin?: Origin): void {
  for (const [o, sids] of known) if (origin === undefined || o === origin) void refreshAccounts(o, [...sids]);
}

/**
 * 那台推来额度 / 会话轮换 / 策略目录变了（`events.ts` 的 `onChanged`，主题 `quota` · `rotation` · `rotation_strategies`）。
 * 额度帧里带了成品（同一条命令的回包，形状由指纹核过）⇒ 直接放进 store；轮换那两种不带成品 ⇒ 重问。`all`（期间可能漏了）⇒ 那台全部重问。
 */
export function onQuotaChanged(origin: Origin, topic: "quota" | "rotation" | "rotation_strategies", change: Changed): void {
  if (topic === "quota") {
    const p = pushedProducts(change, "quota");
    const q = p.got.get(null);
    if (q !== undefined) putIn(appStore.quota, origin, tookQuota(q));
    if (p.all || (q === undefined && p.ask.length > 0)) void refreshQuota(origin);
    reaskAccounts(origin);
    return;
  }
  const keys = change.cells.map((c) => c.key);
  const mine = known.get(origin);
  const every = topic === "rotation_strategies" || change.all || keys.includes(null);
  const sids = every ? [...(mine ?? [])] : keys.filter((s): s is string => s !== null && (mine?.has(s) ?? false));
  if (topic === "rotation_strategies") void refreshRotations(origin, sids);
  else void refreshSessions(origin, sids);
}

/** 那台账号清单变了（连上 · 默认号换了 · 清单动了；`changed/accounts`）⇒ 那台有 tab 的会话的号重问；不给 ⇒ 各台。 */
export function onAccountsChanged(origin?: Origin): void {
  reaskAccounts(origin);
}
