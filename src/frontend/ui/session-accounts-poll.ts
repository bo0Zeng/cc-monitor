/**
 * 账号轮询那一圈的**可验证部分**〔audit-0805 F14 第三刀，报告 I-5〕。
 *
 * # 为什么这些东西住在这里，而不是继续留在 `main.ts` 里
 *
 * `main.ts` **一个 export 都没有**（它是入口模块）⇒ 那圈轮询的三条性质
 * 「扇出有没有上限 / 上一轮没跑完会不会叠加 / 窗口看不见时停不停」
 * **一条都没法写判据**。I-5 说的三件事恰好全在这三条上：
 *
 * | 报告 I-5 说的 | 08-05 复核实测 |
 * |---|---|
 * | host 循环串行 | 成立 —— `for (const h of cfg.hosts) { await Promise.all([…]) }`，`Promise.all` 只并行**同一台**的两条 |
 * | 无重入锁 | 成立 —— `refreshSeq` 只**丢弃晚到的快照**，新一轮照样进；单轮 >10s 时轮次叠加 |
 * | 无可见性门控 | 成立 —— `document.hidden` / `visibilitychange` 在 `src/` 生产代码**零命中** |
 * | 句柄丢弃 | 成立 —— 全仓 `clearInterval` 只有 `views/grid-monitor.ts:200` 一处 |
 *
 * ⇒ 按「**按可验证性拆**」下刀：把这三条抽成两个自己就能测的单元，
 * `main.ts` 只剩接线。
 *
 * # 那个「缓存恒不命中」的 TTL 问题，这里是怎么定的
 *
 * `SESSION_ACCOUNTS_TTL_MS`（8s）< 轮询周期（10s）⇒ 轮询**恒 miss**。
 * F14 §4 当时判断「把 TTL 抬过周期是在回避问题」——本轮把它定死：
 *
 * **轮询是这个缓存的写者，不是读者。** 会话账号归属随起停变，轮询就是那个负责刷新的人，
 * 所以它**显式 `force`**；缓存是给**别的读者**（chip 菜单、tab 徽章按需读）用的。
 * ⇒ 行为与今天一样（今天靠 `8 < 10` 这个巧合达到同样效果），但**意图从巧合变成明写**，
 * 而且以后有人把 TTL 调到 12s 时不会悄悄把这条轮询变成空转。
 *
 * ⚠ 与之相对，`fetchAccounts`（账号列表，30s TTL）**默认不 force**：
 * 账号列表只在迁移/登录时变。**两者的区别是数据变化率，不是疏忽** ——
 * 写在这里免得下一个人「顺手统一一下」。
 *
 * # 🔴 〔`C1` · 2026-09-24〕那个 10 秒轮询**删了**
 *
 * 它补的是「别人改了账号没有事件源」（`polling_registry` 那条 F02 订正逐字）。两件事改了它的前提：
 *
 * 1. 两条查询搬上了**已有的长连接**（后端帧面 `accounts-sessions` / `accounts-list`）——
 *    问一次不再是「一次完整的 TCP+SSH+鉴权」（此前每台每小时 480 次握手）。
 * 2. 「会话 ↔ 账号」只在**会话起停**时变（后端读的是 `sessions/<PID>.json` ＋ 那个进程的环境，
 *    进程活着时环境不变）—— 而会话起停**本来就有帧**（`session_added` / `session_removed`），
 *    monitor 早就把它们转成 远端 `live` 格 / `ended` 格 两个事件。
 *
 * ⇒ 刷新改由事件驱动（{@link createEventRefresher}）：长连接握手完成（〔DL1〕经通道订的 `accounts-changed` 流里那一格 `seen`，
 * 强制刷账号清单；原先是裸事件 `remote-backend-ready`，见 {@link accountsChangedItems}）· 会话起停 · 本 UI 切号。**零定时器**。
 * 买不到的一格如实写：**另一个 monitor 改了默认账号、而这台上没有任何会话起停** ——
 * 这边的账号清单要等下一次握手 / 会话起停 / 本 UI 操作才刷新（此前最多 30 秒）。
 */

import type { RemoteHostConfig } from "./remote-config";
import type { Account, AccountsState, SessionAccount } from "./accounts";
import type { Item } from "../../comms/inward/chan";

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
}

export interface AccountRows {
  rows: SessionAccount[];
  emailByName: Map<string, string>;
  /** 账号确实可查询（`available`）的 origin —— 徽章只在这些 origin 上显。 */
  readyOrigins: Set<string>;
  /** origin → 当前账号名，供 tab 徽章「信息才显」比对。 */
  currentByOrigin: Map<string, string>;
}

/** 一台远端上的两条查询。 */
async function oneHost(
  h: RemoteHostConfig,
  f: HostFetchers,
  forceAccounts: boolean,
): Promise<{ origin: string; sessions: SessionAccount[]; state: AccountsState }> {
  const origin = h.label || h.host;
  const [sessions, state] = await Promise.all([
    // ★ 显式 force：刷新者是这个缓存的**写者**，见模块头注。
    f.fetchSessionAccounts(origin, true),
    // ★ 默认不 force（账号列表 30s TTL，变化率低）；长连接刚握手完时才 force ——
    //   那之前缓存里可能是一份「没有控制通道」的不可用结果，不 force 会把它再端 30 秒。
    forceAccounts ? f.fetchAccounts(origin, true) : f.fetchAccounts(origin),
  ]);
  return { origin, sessions, state };
}

/**
 * 对所有已配置远端扇出，**有上限、保序**地聚合。
 *
 * ⚠ `K-R59` 之前这里先 `filter(h => !h.daemonless)` —— 那一档没了，今天一台都不排。
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
  };
  for (const { origin, sessions, state } of per) {
    out.rows.push(...sessions);
    for (const a of state.accounts) if (a.email) out.emailByName.set(a.name, a.email);
    if (state.available) out.readyOrigins.add(origin);
    const cur = f.currentAccountForBadge(state);
    if (cur) out.currentByOrigin.set(origin, cur.name);
  }
  return out;
}

/**
 * 事件驱动的刷新器〔`C1` · 2026-09-24〕—— 替掉那个 10 秒 `setInterval`。
 *
 * - **零定时器**：只在 {@link EventRefresher.request} 时跑。
 * - **不叠加**：在飞时再来的请求合并成**一次**补跑（跑完立刻再跑一轮，拿到最新）；
 *   启动时一批 远端 `live` 格 涌进来只会多跑一轮，不会摞 N 轮。
 * - `force` 在合并时取「或」：任何一次要求 force，补跑那一轮就 force。
 * - `run` 抛错由这里接住（E4：有身份地失败，计进 `failures`），不留 unhandled rejection，
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

/**
 * 〔DL1 · `设计/01 §2.2`「前端只有两个动作」〕流标签：那台机器上「账号清单可能变了」。
 * 与 Rust `event_replay.rs::ACCOUNTS_CHANGED_KIND` 同一个串（两侧对拍在 `session-accounts-poll.vitest.ts`）。
 */
export const ACCOUNTS_CHANGED_KIND = "accounts-changed";

/**
 * 每条 `accounts-changed` 订阅一开始给多少格 credit。那一格很稀（一台一次连上 / 一次清单变），
 * 收到的 `frame` 当场还 ⇒ 正常用法打不满；打满了句柄就丢、下一次原位给 `gap`（`05 §3.3.4` 级 2），这里当成「变过」照刷。
 */
export const ACCOUNTS_CHANGED_WINDOW = 8;

/**
 * 〔DL1 · 合并 TAP 时收成这一形〕**`accounts-changed` 流里的一批格 ⇒ 要不要刷、还多少 credit**（纯函数）。
 *
 * 订阅本身与会话行 · tap 走**同一处** `chan.subscribe`（`events.ts::bindEvents` 的 `plan`，`X6` 调用点恰好一处）；
 * 这里只答「这一批格是什么意思」：
 *
 * | 格 | 意思 | 这里 |
 * |---|---|---|
 * | `seen` | 那台的长连接（又）通了、能问了 | 要刷 |
 * | `frame` | 那台后端说账号清单变了（`05 §13.6 ③`） | 要刷，占一格 credit（当场还） |
 * | `gap` | 没 credit 时丢过几格 | 当成变过：要刷 |
 * | `unseen` / `closed` | 断了 / 这条订阅没了 | 不刷（断着问不到；连上时会有 `seen`） |
 *
 * 一批里有几格都只刷一次（`changed` 是一个布尔）。
 */
export function accountsChangedItems(items: readonly Item[]): { changed: boolean; frames: number } {
  let changed = false;
  let frames = 0;
  for (const it of items) {
    if (it.t === "frame") {
      frames += 1;
      changed = true;
    } else if (it.t === "seen" || it.t === "gap") {
      changed = true;
    }
  }
  return { changed, frames };
}
