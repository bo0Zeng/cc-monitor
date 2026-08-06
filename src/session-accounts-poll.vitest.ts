import { describe, it, expect, vi, afterEach } from "vitest";
import {
  HOST_FANOUT_LIMIT,
  mapWithLimit,
  collectAccountRows,
  createGatedPoller,
  type HostFetchers,
} from "./session-accounts-poll";
import type { RemoteHostConfig } from "./remote-config";
import type { AccountsState, SessionAccount } from "./accounts";

/** 一台远端的最小配置（只有被测代码读到的字段是真的）。 */
function host(label: string, daemonless = false): RemoteHostConfig {
  return { label, host: `${label}.example`, daemonless } as unknown as RemoteHostConfig;
}

function state(origin: string, available = true): AccountsState {
  return {
    origin,
    available,
    error: null,
    meta: null,
    accounts: [{ name: `acct-${origin}`, email: `${origin}@x` }],
    defaultName: null,
  } as unknown as AccountsState;
}

function session(origin: string): SessionAccount {
  return { pid: 1, sessionId: `sid-${origin}`, account: origin } as unknown as SessionAccount;
}

describe("mapWithLimit", () => {
  it("保序 —— 结果按输入顺序，与完成顺序无关", async () => {
    const delays = [30, 5, 20, 1, 10];
    const out = await mapWithLimit(delays, 2, async (d, i) => {
      await new Promise((r) => setTimeout(r, d));
      return i;
    });
    expect(out).toEqual([0, 1, 2, 3, 4]);
  });

  it("同时在飞的数量不超过上限", async () => {
    let live = 0;
    let peak = 0;
    await mapWithLimit(Array.from({ length: 12 }, (_, i) => i), 3, async () => {
      live += 1;
      peak = Math.max(peak, live);
      await new Promise((r) => setTimeout(r, 1));
      live -= 1;
    });
    expect(peak).toBe(3);
  });
});

/** 造一组会记录「同时在飞峰值」的 fetcher。 */
function fanoutProbe(): {
  f: HostFetchers;
  peak: () => number;
  sessionForced: boolean[];
  accountsForced: (boolean | undefined)[];
} {
  let live = 0;
  let peak = 0;
  const sessionForced: boolean[] = [];
  const accountsForced: (boolean | undefined)[] = [];
  const f: HostFetchers = {
    async fetchSessionAccounts(origin, force) {
      sessionForced.push(force === true);
      live += 1;
      peak = Math.max(peak, live);
      await new Promise((r) => setTimeout(r, 2));
      live -= 1;
      return [session(origin)];
    },
    async fetchAccounts(origin, force) {
      accountsForced.push(force);
      return state(origin);
    },
    currentAccountForBadge: (s) => s.accounts[0] ?? null,
  };
  return { f, peak: () => peak, sessionForced, accountsForced };
}

describe("collectAccountRows —— 扇出", () => {
  it("★ 十台远端的扇出有上限，而不是一次全放出去", async () => {
    const { f, peak } = fanoutProbe();
    await collectAccountRows(
      Array.from({ length: 10 }, (_, i) => host(`h${i}`)),
      f,
    );
    expect(
      peak(),
      `同时在飞 ${peak()} 台 —— 上限 ${HOST_FANOUT_LIMIT} 没生效。` +
        "无上限扇出正是报告 I-5 那一条：一次按键最终对每台各发一轮 SSH。",
    ).toBeLessThanOrEqual(HOST_FANOUT_LIMIT);
  });

  it("★ 也不是退回串行 —— 峰值必须真的用满上限（反向判据）", async () => {
    const { f, peak } = fanoutProbe();
    await collectAccountRows(Array.from({ length: 10 }, (_, i) => host(`h${i}`)), f);
    expect(
      peak(),
      "峰值 1 = 还是一台一台来。写「该限流」的判据时必须同时写一条「不许限成串行」的，" +
        "否则把上限设成 1 也能过。",
    ).toBe(HOST_FANOUT_LIMIT);
  });

  it("rows 按 host 顺序拼，不按完成顺序", async () => {
    const f: HostFetchers = {
      // 第一台最慢 —— 若按完成顺序拼，它会排到最后
      async fetchSessionAccounts(origin) {
        await new Promise((r) => setTimeout(r, origin === "h0" ? 20 : 1));
        return [session(origin)];
      },
      async fetchAccounts(origin) {
        return state(origin);
      },
      currentAccountForBadge: (s) => s.accounts[0] ?? null,
    };
    const out = await collectAccountRows([host("h0"), host("h1"), host("h2")], f, 3);
    expect(out.rows.map((r) => r.account)).toEqual(["h0", "h1", "h2"]);
  });

  it("daemonless 的机器一条查询都不发", async () => {
    const seen: string[] = [];
    const f: HostFetchers = {
      async fetchSessionAccounts(origin) {
        seen.push(origin);
        return [];
      },
      async fetchAccounts(origin) {
        return state(origin);
      },
      currentAccountForBadge: () => null,
    };
    await collectAccountRows([host("a"), host("b", true), host("c")], f);
    expect(seen).toEqual(["a", "c"]);
  });

  it("available:false 的 origin 不进 readyOrigins", async () => {
    const f: HostFetchers = {
      async fetchSessionAccounts() {
        return [];
      },
      async fetchAccounts(origin) {
        return state(origin, origin !== "bad");
      },
      currentAccountForBadge: (s) => (s.available ? (s.accounts[0] ?? null) : null),
    };
    const out = await collectAccountRows([host("good"), host("bad")], f);
    expect([...out.readyOrigins]).toEqual(["good"]);
  });

  it("★ 意图判据：session-accounts 强制刷新，accounts 走 TTL", async () => {
    const { f, sessionForced, accountsForced } = fanoutProbe();
    await collectAccountRows([host("a"), host("b")], f);
    expect(
      sessionForced,
      "轮询是 session-accounts 缓存的**写者**，必须显式 force —— " +
        "今天它靠「TTL 8s < 周期 10s」这个巧合恒 miss，谁把 TTL 调到 12s 就会把这条轮询变成空转",
    ).toEqual([true, true]);
    expect(
      accountsForced.every((x) => x !== true),
      "accounts 列表 30s TTL 是**刻意**的（只在迁移/登录时变），不该跟着一起 force",
    ).toBe(true);
  });
});

describe("createGatedPoller", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("★ 上一轮没跑完，这一拍跳过而不是叠加", async () => {
    vi.useFakeTimers();
    let entered = 0;
    let release!: () => void;
    const blocked = new Promise<void>((r) => (release = r));
    const p = createGatedPoller({
      intervalMs: 1000,
      tick: async () => {
        entered += 1;
        await blocked;
      },
      isHidden: () => false,
      subscribeVisibility: () => () => {},
    });
    p.start(); // 立刻跑一轮，卡住不返回
    expect(entered).toBe(1);
    await vi.advanceTimersByTimeAsync(5000); // 五拍
    expect(
      entered,
      `五拍之内进了 ${entered} 次 —— 没有重入锁。报告 I-5：单轮 >10s 时轮次会摞起来，` +
        "每摞一层就是对所有远端多一轮 SSH",
    ).toBe(1);
    expect(p.skippedInFlight).toBe(5);
    release();
  });

  it("★ 窗口不可见时跳过；重新可见时立刻补一轮", async () => {
    vi.useFakeTimers();
    let hidden = false;
    let ticks = 0;
    let onVis!: () => void;
    const p = createGatedPoller({
      intervalMs: 1000,
      tick: async () => {
        ticks += 1;
      },
      isHidden: () => hidden,
      subscribeVisibility: (cb) => {
        onVis = cb;
        return () => {};
      },
    });
    p.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(ticks).toBe(1); // start 那一轮

    hidden = true;
    await vi.advanceTimersByTimeAsync(3000);
    expect(ticks, "窗口看不见时还在对所有远端发 SSH").toBe(1);
    expect(p.skippedHidden).toBe(3);

    hidden = false;
    onVis();
    await vi.advanceTimersByTimeAsync(0);
    expect(ticks, "切回来时数据是陈旧的，必须补一轮").toBe(2);
  });

  it("可见时照常轮询（反向判据：不许门控成永不刷新）", async () => {
    vi.useFakeTimers();
    let ticks = 0;
    const p = createGatedPoller({
      intervalMs: 1000,
      tick: async () => {
        ticks += 1;
      },
      isHidden: () => false,
      subscribeVisibility: () => () => {},
    });
    p.start();
    await vi.advanceTimersByTimeAsync(3000);
    expect(ticks, "沿用 F13/F14 立的形态：写一条「该停」就要写一条「不许全停」").toBe(4);
    p.stop();
  });

  it("★ stop() 之后真的不再跑 —— 今天那个 interval 句柄是丢掉的", async () => {
    vi.useFakeTimers();
    let ticks = 0;
    let unsubscribed = false;
    const p = createGatedPoller({
      intervalMs: 1000,
      tick: async () => {
        ticks += 1;
      },
      isHidden: () => false,
      subscribeVisibility: () => () => {
        unsubscribed = true;
      },
    });
    p.start();
    await vi.advanceTimersByTimeAsync(2000);
    const before = ticks;
    p.stop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(ticks, "stop() 之后还在跑 —— 停不下来的轮询和没有 stop 是一回事").toBe(before);
    expect(unsubscribed, "stop() 必须退订 visibilitychange，否则监听器泄漏").toBe(true);
  });

  it("★ tick 抛错不会把重入锁焊死", async () => {
    vi.useFakeTimers();
    let ticks = 0;
    const seen: string[] = [];
    const p = createGatedPoller({
      intervalMs: 1000,
      tick: async () => {
        ticks += 1;
        throw new Error("boom");
      },
      isHidden: () => false,
      subscribeVisibility: () => () => {},
      onError: (e) => seen.push(String(e)),
    });
    p.start();
    await vi.advanceTimersByTimeAsync(3000);
    expect(
      ticks,
      "一次异常就把轮询永久锁死 —— 这正是 `finally` 而不是 `then` 的理由",
    ).toBeGreaterThan(1);
    // ★ 接住不等于吞掉：每次失败都要有身份（E4）
    expect(p.failures, "失败被吞成「看起来一切正常」").toBe(ticks);
    expect(seen.every((m) => m.includes("boom"))).toBe(true);
    p.stop();
  });
});
