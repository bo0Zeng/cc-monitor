/**
 * 账号刷新那一圈里能单测的部分（`main.ts` 是入口、没有 export，只剩接线）：扇出有上限、请求不叠加。
 *
 * - 会话账号归属的刷新显式 `force`：这里是那份缓存的写者，缓存给别的读者（chip 菜单、tab 徽章）用；
 *   账号列表（`fetchAccounts`，30s TTL）默认不 force —— 它只在迁移 / 登录时变，区别是数据变化率。
 * - 事件驱动、零定时器（{@link createEventRefresher}）：「会话 ↔ 账号」只在会话起停时变，起停本来就有事件；
 *   刷新的时机 = 长连接握手完成 / 那台账号清单变了（`changed/accounts` 流里的 `seen` / 帧，见 `changed-stream.ts`）· 会话起停 · 本界面切号。
 *   代价：另一个 monitor 改了默认账号、这台又没有会话起停时，这边要等下一次握手 / 起停 / 操作才刷新。
 */

import type { RemoteHostConfig } from "./remote-config";
import type { Account, AccountsState, SessionAccount } from "./accounts";

/**
 * 同时在飞的远端数上限。
 *
 * 4 是照 `views/history.ts:142 LOAD_ALL_CONCURRENCY` 抄的 —— 同一个仓里同一类问题
 * （一次性向后端 fire N 个 IPC）**不另发明一个数**。
 * ⚠ 这两个常量**刻意不共用**：它们节流的是两条不同的路（历史项目加载 vs 账号轮询），
 * 将来谁要单独调都不该被对方绊住。共用的是**范式**，不是值。
 */
export const HOST_FANOUT_LIMIT = 4;

/**
 * 有并发上限的 map。保序：结果按 `items` 的顺序返回，与完成顺序无关。
 *
 * ⚠ 保序不是装饰 —— 今天串行循环产出的 rows 是按 host 顺序拼的，
 * 并发化之后如果按完成顺序拼，UI 里的行序会随网络抖动变。
 */
export async function mapWithLimit<T, R>(
  items: readonly T[],
  limit: number,
  fn: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  const out = new Array<R>(items.length);
  let next = 0;
  const worker = async (): Promise<void> => {
    while (next < items.length) {
      const i = next++;
      out[i] = await fn(items[i], i);
    }
  };
  const n = Math.max(1, Math.min(limit, items.length));
  await Promise.all(Array.from({ length: n }, () => worker()));
  return out;
}

export interface HostFetchers {
  fetchSessionAccounts(origin: string, force?: boolean): Promise<SessionAccount[]>;
  fetchAccounts(origin: string, force?: boolean): Promise<AccountsState>;
  currentAccountForBadge(state: AccountsState): Account | null;
  /** 那台记着的「每条会话上次用哪个号起的」（`history-last-accounts`，会话跑在哪台就问哪台）。 */
  lastAccounts(origin: string): Promise<Record<string, string>>;
}

export interface AccountRows {
  rows: SessionAccount[];
  emailByName: Map<string, string>;
  /** 账号确实可查询（`available`）的 origin —— 徽章只在这些 origin 上显。 */
  readyOrigins: Set<string>;
  /** origin → 当前账号名，供 tab 徽章「信息才显」比对。 */
  currentByOrigin: Map<string, string>;
  /** sid → 上次用哪个号起的（各台那份并起来；不活的会话徽章用它）。 */
  lastByS: Map<string, string>;
}

/** 一台远端上的三条查询。 */
async function oneHost(
  h: RemoteHostConfig,
  f: HostFetchers,
  forceAccounts: boolean,
): Promise<{ origin: string; sessions: SessionAccount[]; state: AccountsState; last: Record<string, string> }> {
  const origin = h.label || h.host;
  const [sessions, state, last] = await Promise.all([
    // ★ 显式 force：刷新者是这个缓存的**写者**，见模块头注。
    f.fetchSessionAccounts(origin, true),
    // ★ 默认不 force（账号列表 30s TTL，变化率低）；长连接刚握手完时才 force ——
    //   那之前缓存里可能是一份「没有控制通道」的不可用结果，不 force 会把它再端 30 秒。
    forceAccounts ? f.fetchAccounts(origin, true) : f.fetchAccounts(origin),
    // 问不到（通道没起 / 那台读不懂那份记录）⇒ 这台一条都不算（徽章退到「—」，不猜）。
    f.lastAccounts(origin).catch((e: unknown) => {
      console.warn(`history-last-accounts @${origin} failed:`, e);
      return {} as Record<string, string>;
    }),
  ]);
  return { origin, sessions, state, last };
}

/**
 * 对所有已配置远端扇出，**有上限、保序**地聚合。
 *
 * 返回聚合结果而不是直接写 UI —— 那是接线层的事，留在 `main.ts`。
 */
export async function collectAccountRows(
  hosts: readonly RemoteHostConfig[],
  f: HostFetchers,
  limit: number = HOST_FANOUT_LIMIT,
  forceAccounts = false,
): Promise<AccountRows> {
  const per = await mapWithLimit(hosts, limit, (h) => oneHost(h, f, forceAccounts));

  const out: AccountRows = {
    rows: [],
    emailByName: new Map(),
    readyOrigins: new Set(),
    currentByOrigin: new Map(),
    lastByS: new Map(),
  };
  for (const { origin, sessions, state, last } of per) {
    for (const [sid, name] of Object.entries(last)) out.lastByS.set(sid, name);
    out.rows.push(...sessions);
    for (const a of state.accounts) if (a.email) out.emailByName.set(a.name, a.email);
    if (state.available) out.readyOrigins.add(origin);
    const cur = f.currentAccountForBadge(state);
    if (cur) out.currentByOrigin.set(origin, cur.name);
  }
  return out;
}

/**
 * 事件驱动的刷新器。
 *
 * - **零定时器**：只在 {@link EventRefresher.request} 时跑。
 * - **不叠加**：在飞时再来的请求合并成**一次**补跑（跑完立刻再跑一轮，拿到最新）；
 *   启动时一批 远端 `live` 格 涌进来只会多跑一轮，不会摞 N 轮。
 * - `force` 在合并时取「或」：任何一次要求 force，补跑那一轮就 force。
 * - `run` 抛错由这里接住（有身份地失败，计进 `failures`），不留 unhandled rejection，
 *   也不把「在飞」焊死。
 */
export interface EventRefresher {
  request(force?: boolean): void;
  /** 在飞时进来、被合并掉的请求数（判据用）。 */
  readonly coalesced: number;
  /** `run` 抛错的次数。 */
  readonly failures: number;
}

export function createEventRefresher(
  run: (force: boolean) => Promise<void>,
  onError: (e: unknown) => void = (e) => console.warn("session-accounts refresh failed:", e),
): EventRefresher {
  let inFlight = false;
  let pending = false;
  let pendingForce = false;
  let coalesced = 0;
  let failures = 0;
  const loop = async (force: boolean): Promise<void> => {
    inFlight = true;
    try {
      await run(force);
    } catch (e) {
      failures += 1;
      onError(e);
    } finally {
      inFlight = false;
    }
    if (pending) {
      const f = pendingForce;
      pending = false;
      pendingForce = false;
      await loop(f);
    }
  };
  return {
    request(force = false): void {
      if (inFlight) {
        pending = true;
        pendingForce = pendingForce || force;
        coalesced += 1;
        return;
      }
      void loop(force);
    },
    get coalesced(): number {
      return coalesced;
    },
    get failures(): number {
      return failures;
    },
  };
}
