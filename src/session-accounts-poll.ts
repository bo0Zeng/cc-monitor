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
 * ⚠ 与之相对，`fetchAccounts`（账号列表，30s TTL）**刻意不 force**：
 * 账号列表只在迁移/登录时变，让它 3 轮才真发一次 SSH 是对的。
 * **两者的区别是数据变化率，不是疏忽** —— 写在这里免得下一个人「顺手统一一下」。
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
}

export interface AccountRows {
  rows: SessionAccount[];
  emailByName: Map<string, string>;
  /** 账号确实可查询（`available`）的 origin —— 徽章只在这些 origin 上显。 */
  readyOrigins: Set<string>;
  /** origin → 当前账号名，供 tab 徽章「信息才显」比对。 */
  currentByOrigin: Map<string, string>;
}

/** 一台远端上的两条查询。`daemonless` 的机器在调用方过滤掉，这里不再判。 */
async function oneHost(
  h: RemoteHostConfig,
  f: HostFetchers,
): Promise<{ origin: string; sessions: SessionAccount[]; state: AccountsState }> {
  const origin = h.label || h.host;
  const [sessions, state] = await Promise.all([
    // ★ 显式 force：轮询是这个缓存的**写者**，见模块头注。
    f.fetchSessionAccounts(origin, true),
    // ★ 刻意不 force：账号列表 30s TTL，变化率低，3 轮真发一次就够。
    f.fetchAccounts(origin),
  ]);
  return { origin, sessions, state };
}

/**
 * 对所有非 `daemonless` 的远端扇出，**有上限、保序**地聚合。
 *
 * 返回聚合结果而不是直接写 UI —— 那是接线层的事，留在 `main.ts`。
 */
export async function collectAccountRows(
  hosts: readonly RemoteHostConfig[],
  f: HostFetchers,
  limit: number = HOST_FANOUT_LIMIT,
): Promise<AccountRows> {
  const targets = hosts.filter((h) => !h.daemonless);
  const per = await mapWithLimit(targets, limit, (h) => oneHost(h, f));

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

export interface GatedPollerOptions {
  intervalMs: number;
  /**
   * 一轮要干的活。
   *
   * ⚠ **它抛错由本轮询器接住**，不是「调用方自己消化」：`start()` 里是 `void run()`，
   * 不接住就变成 unhandled rejection —— 本仓 `main.ts` 挂着全局 `unhandledrejection`
   * 钩子，那会把每一次失败刷进状态栏。接住之后按 **E4**（静默失败给身份）
   * 打一条带名字的 warn，并计进 `failures` 让判据看得见。
   */
  tick: () => Promise<void>;
  /** 出错时怎么报。默认 `console.warn`。 */
  onError?: (e: unknown) => void;
  /** 窗口当前看不见吗。默认读 `document.hidden`。 */
  isHidden?: () => boolean;
  /** 订阅可见性变化，返回退订函数。默认挂 `visibilitychange`。 */
  subscribeVisibility?: (cb: () => void) => () => void;
}

export interface GatedPoller {
  start(): void;
  /** 停表并退订。⚠ 今天 `main.ts` 那个 `setInterval` 的句柄是**丢掉的**，全仓没人停得了它。 */
  stop(): void;
  /** 因为上一轮还在飞而跳过的次数（判据用）。 */
  readonly skippedInFlight: number;
  /** 因为窗口不可见而跳过的次数（判据用）。 */
  readonly skippedHidden: number;
  /** `tick` 抛错的次数。⚠ 有身份地失败（E4）：不许把异常吞成「看起来一切正常」。 */
  readonly failures: number;
}

/**
 * 带**重入锁**与**可见性门控**的轮询器。
 *
 * - 上一轮没跑完 ⇒ 这一拍**跳过**（不叠加）。今天没有这条：单轮 >10s 时轮次会摞起来。
 * - 窗口不可见 ⇒ 跳过；**重新可见时立刻补一轮**（否则用户切回来看到的是陈旧数据）。
 * - `start()` 会**先跑一轮**再起表 —— 与今天 `void refreshSessionAccounts(); setInterval(…)` 一致。
 *
 * ⚠ 这不是新增周期唤醒（定框 **E6** 只许减不许增）：它**替换**了 `main.ts` 原来那个
 * `setInterval`，而且新增了「可见性跳过」与「能停」两条**减少**唤醒的性质。
 */
export function createGatedPoller(o: GatedPollerOptions): GatedPoller {
  const isHidden = o.isHidden ?? ((): boolean => document.hidden);
  const subscribe =
    o.subscribeVisibility ??
    ((cb: () => void): (() => void) => {
      document.addEventListener("visibilitychange", cb);
      return () => document.removeEventListener("visibilitychange", cb);
    });

  let timer: ReturnType<typeof setInterval> | null = null;
  let unsubscribe: (() => void) | null = null;
  let inFlight = false;
  let skippedInFlight = 0;
  let skippedHidden = 0;
  let failures = 0;

  const onError =
    o.onError ??
    ((e: unknown): void => {
      console.warn("session-accounts poller tick failed:", e);
    });

  const run = async (): Promise<void> => {
    if (inFlight) {
      skippedInFlight += 1;
      return;
    }
    inFlight = true;
    try {
      await o.tick();
    } catch (e) {
      // 必须接住：`start()` 里是 `void run()`，不接住就成 unhandled rejection，
      // 而 `main.ts` 的全局钩子会把它刷进状态栏（判据 `tick 抛错` 那条实测撞出来的）。
      failures += 1;
      onError(e);
    } finally {
      // `finally` 而不是 `then`：`tick` 抛了也必须解锁，否则一次异常就把轮询永久锁死。
      inFlight = false;
    }
  };

  return {
    start(): void {
      if (timer !== null) return;
      void run();
      timer = setInterval(() => {
        if (isHidden()) {
          skippedHidden += 1;
          return;
        }
        void run();
      }, o.intervalMs);
      unsubscribe = subscribe(() => {
        if (!isHidden()) void run();
      });
    },
    stop(): void {
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
      unsubscribe?.();
      unsubscribe = null;
    },
    get skippedInFlight(): number {
      return skippedInFlight;
    },
    get skippedHidden(): number {
      return skippedHidden;
    },
    get failures(): number {
      return failures;
    },
  };
}
