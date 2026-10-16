/**
 * 账号徽章要的「清单那一侧」：每台的账号清单（邮箱 · 默认号 · 可不可查）＋ 那台记着的「每条会话上次用哪个号起的」。
 *
 * 会话此刻跑在哪个号下不在这里：那台后端在会话宣告里说（`session_added.account`），起停不回头问。
 * 这里只在那台的账号清单可能变了时重问那一台（`changed/accounts`：连上 · 清单动了）·本界面切号时重问各台；零定时器。
 * 代价：另一个 monitor 改了默认账号、这台的清单文件又没动时，这边要等下一次连上 / 操作才刷新。
 */

import type { Account, AccountsState } from "./accounts";
import type { AccountFacts } from "./app-store";

export interface HostFetchers {
  fetchAccounts(origin: string): Promise<AccountsState>;
  currentAccountForBadge(state: AccountsState): Account | null;
  /** 那台记着的「每条会话上次用哪个号起的」（`history-last-accounts`，会话跑在哪台就问哪台）。 */
  lastAccounts(origin: string): Promise<Record<string, string>>;
}

/** 一台问回来的那一份。 */
export interface HostFacts {
  emailByName: Map<string, string>;
  /** 账号确实可查询（`available`）—— 徽章只在这些台上显。 */
  ready: boolean;
  /** 那台的当前账号名（选得了的才算），供徽章「信息才显」比对。 */
  current: string | null;
  lastByS: Map<string, string>;
}

/** 问一台：账号清单 ＋ 上次用的号，两问。 */
export async function askHost(origin: string, f: HostFetchers): Promise<HostFacts> {
  const [state, last] = await Promise.all([
    f.fetchAccounts(origin),
    // 问不到（通道没起 / 那台读不懂那份记录）⇒ 这台一条都不算（徽章退到「—」，不猜）。
    f.lastAccounts(origin).catch((e: unknown) => {
      console.warn(`history-last-accounts @${origin} failed:`, e);
      return {} as Record<string, string>;
    }),
  ]);
  const emailByName = new Map<string, string>();
  for (const a of state.accounts) if (a.email) emailByName.set(a.name, a.email);
  return {
    emailByName,
    ready: state.available,
    current: f.currentAccountForBadge(state)?.name ?? null,
    lastByS: new Map(Object.entries(last)),
  };
}

/** 各台那一份按台的顺序并成一份（同名账号 · 同一个 sid 后一台的为准）。 */
export function mergeFacts(per: readonly (readonly [string, HostFacts])[]): AccountFacts {
  const out: AccountFacts = { emailByName: new Map(), lastByS: new Map(), readyOrigins: new Set(), currentByOrigin: new Map() };
  for (const [origin, h] of per) {
    for (const [k, v] of h.emailByName) out.emailByName.set(k, v);
    for (const [k, v] of h.lastByS) out.lastByS.set(k, v);
    if (h.ready) out.readyOrigins.add(origin);
    if (h.current !== null) out.currentByOrigin.set(origin, h.current);
  }
  return out;
}

export interface AccountFactsDeps {
  fetchers: HostFetchers;
  /** 此刻要问的那几台远端（按配置的顺序）。 */
  hosts(): Promise<string[]>;
  /** 并好的那一份交出去（接线层放进 store）。 */
  publish(f: AccountFacts): void;
}

export interface AccountFactsFeed {
  /** 重问那一台（不是要问的远端 ⇒ 不问）；不给 ⇒ 各台都问。 */
  refresh(origin?: string): void;
}

export function createAccountFacts(d: AccountFactsDeps): AccountFactsFeed {
  const per = new Map<string, HostFacts>();
  const runs = new Map<string, EventRefresher>();
  const publish = (hosts: readonly string[]): void => {
    for (const o of [...per.keys()]) if (!hosts.includes(o)) per.delete(o);
    d.publish(mergeFacts(hosts.flatMap((o) => (per.has(o) ? [[o, per.get(o)!] as const] : []))));
  };
  // 每台一个刷新器：同一台在飞时再来的合并成一次补跑，慢的旧那一份不会盖过新的。
  const of = (origin: string): EventRefresher => {
    let r = runs.get(origin);
    if (r === undefined) {
      r = createEventRefresher(async () => {
        const got = await askHost(origin, d.fetchers);
        per.set(origin, got);
        publish(await d.hosts());
      });
      runs.set(origin, r);
    }
    return r;
  };
  return {
    refresh(origin?: string): void {
      void d
        .hosts()
        .then((hosts) => {
          if (origin === undefined && hosts.length === 0) return publish(hosts);
          for (const o of hosts) if (origin === undefined || o === origin) of(o).request();
        })
        .catch((e: unknown) => console.warn("account facts refresh failed:", e));
    },
  };
}

/**
 * 事件驱动的刷新器。
 *
 * - **零定时器**：只在 {@link EventRefresher.request} 时跑。
 * - **不叠加**：在飞时再来的请求合并成**一次**补跑（跑完立刻再跑一轮，拿到最新）。
 * - `run` 抛错由这里接住（有身份地失败，计进 `failures`），不留 unhandled rejection，
 *   也不把「在飞」焊死。
 */
export interface EventRefresher {
  request(): void;
  /** 在飞时进来、被合并掉的请求数（判据用）。 */
  readonly coalesced: number;
  /** `run` 抛错的次数。 */
  readonly failures: number;
}

export function createEventRefresher(
  run: () => Promise<void>,
  onError: (e: unknown) => void = (e) => console.warn("account facts refresh failed:", e),
): EventRefresher {
  let inFlight = false;
  let pending = false;
  let coalesced = 0;
  let failures = 0;
  const loop = async (): Promise<void> => {
    inFlight = true;
    try {
      await run();
    } catch (e) {
      failures += 1;
      onError(e);
    } finally {
      inFlight = false;
    }
    if (pending) {
      pending = false;
      await loop();
    }
  };
  return {
    request(): void {
      if (inFlight) {
        pending = true;
        coalesced += 1;
        return;
      }
      void loop();
    },
    get coalesced(): number {
      return coalesced;
    },
    get failures(): number {
      return failures;
    },
  };
}
