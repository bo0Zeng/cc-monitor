import { describe, it, expect, vi } from "vitest";
import {
  HOST_FANOUT_LIMIT,
  mapWithLimit,
  collectAccountRows,
  createEventRefresher,
  type HostFetchers,
} from "../../../src/frontend/ui/session-accounts-poll";
import type { RemoteHostConfig } from "../../../src/frontend/ui/remote-config";
import type { AccountsState, SessionAccount } from "../../../src/frontend/ui/accounts";

/** 一台远端的最小配置（只有被测代码读到的字段是真的）。 */
function host(label: string): RemoteHostConfig {
  return { label, host: `${label}.example` } as unknown as RemoteHostConfig;
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
    lastAccounts: async () => ({}),
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
      lastAccounts: async () => ({}),
    };
    const out = await collectAccountRows([host("h0"), host("h1"), host("h2")], f, 3);
    expect(out.rows.map((r) => r.account)).toEqual(["h0", "h1", "h2"]);
  });

  /**
   * 🔴 `K-R59`：这一条此前逐字叫「daemonless 的机器一条查询都不发」，
   * 断的是 `collectAccountRows` 先 `filter(h => !h.daemonless)`。
   * 定框 `K35`（「没有没有后端的情况」）把那一档删了 ⇒ **翻面**：一台都不排。
   */
  it("🔴 K-R59：**一台都不排** —— 每台已配置远端都发查询（那个降级开关没了）", () => {
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
      lastAccounts: async () => ({}),
    };
    return collectAccountRows([host("a"), host("b"), host("c")], f).then(() => {
      expect(seen).toEqual(["a", "b", "c"]);
    });
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
      lastAccounts: async () => ({}),
    };
    const out = await collectAccountRows([host("good"), host("bad")], f);
    expect([...out.readyOrigins]).toEqual(["good"]);
  });

  it("★ 〔C1〕长连接刚握手完（forceAccounts）⇒ 账号清单也 force；默认仍不 force", async () => {
    const forced = fanoutProbe();
    await collectAccountRows([host("a"), host("b")], forced.f, undefined, true);
    expect(forced.accountsForced).toEqual([true, true]);
    const plain = fanoutProbe();
    await collectAccountRows([host("a"), host("b")], plain.f);
    expect(plain.accountsForced.every((x) => x !== true)).toBe(true);
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

describe("createEventRefresher（C1：替掉 10 秒轮询）", () => {
  it("★ 零定时器：不 request 就一次都不跑", async () => {
    vi.useFakeTimers();
    let runs = 0;
    createEventRefresher(async () => {
      runs += 1;
    });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(runs, "没有事件却跑了 —— 那就是轮询换了个名字").toBe(0);
    expect(vi.getTimerCount(), "刷新器自己挂了定时器").toBe(0);
    vi.useRealTimers();
  });

  it("★ 在飞时再来的请求合并成**一次**补跑，而不是叠加", async () => {
    let runs = 0;
    let release!: () => void;
    let blocked = new Promise<void>((r) => (release = r));
    const r = createEventRefresher(async () => {
      runs += 1;
      await blocked;
    });
    r.request();
    for (let i = 0; i < 5; i += 1) r.request();
    expect(runs).toBe(1);
    expect(r.coalesced).toBe(5);
    const first = release;
    blocked = Promise.resolve();
    first();
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
    expect(runs, "五次请求应合并成恰好一次补跑（拿到最新），不是 0 次也不是 5 次").toBe(2);
  });

  it("★ force 在合并时取「或」", async () => {
    const seen: boolean[] = [];
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    let first = true;
    const r = createEventRefresher(async (force) => {
      seen.push(force);
      if (first) {
        first = false;
        await gate;
      }
    });
    r.request();
    r.request(false);
    r.request(true);
    r.request(false);
    release();
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
    expect(seen).toEqual([false, true]);
  });

  it("★ run 抛错不会把「在飞」焊死，且有身份地计数", async () => {
    const errs: string[] = [];
    let runs = 0;
    const r = createEventRefresher(
      async () => {
        runs += 1;
        throw new Error("boom");
      },
      (e) => errs.push(String(e)),
    );
    r.request();
    await new Promise((res) => setTimeout(res, 0));
    r.request();
    await new Promise((res) => setTimeout(res, 0));
    expect(runs, "一次异常就锁死了刷新").toBe(2);
    expect(r.failures).toBe(2);
    expect(errs.every((m) => m.includes("boom"))).toBe(true);
  });
});

describe("「上次用哪个号起的」每台各问一次那台，并起来", () => {
  it("★ 两台各自记着的那几条都在；问不到的那一台不拖垮别台（它那几条当没有）", async () => {
    const f: HostFetchers = {
      fetchSessionAccounts: async () => [],
      fetchAccounts: async (origin) => ({ origin, available: true, accounts: [] }) as unknown as AccountsState,
      currentAccountForBadge: () => null,
      lastAccounts: async (origin) => {
        if (origin === "bad") throw new Error("没有控制通道");
        return { [`s-${origin}`]: `acct-${origin}` };
      },
    };
    const got = await collectAccountRows([host("devbox"), host("bad"), host("nano")], f);
    expect([...got.lastByS]).toEqual([
      ["s-devbox", "acct-devbox"],
      ["s-nano", "acct-nano"],
    ]);
  });
});
