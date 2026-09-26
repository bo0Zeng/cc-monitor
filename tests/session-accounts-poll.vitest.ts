import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import {
  HOST_FANOUT_LIMIT,
  mapWithLimit,
  collectAccountRows,
  createEventRefresher,
  watchAccountsChanged,
  ACCOUNTS_CHANGED_KIND,
  ACCOUNTS_CHANGED_WINDOW,
  type AccountsChangedChannel,
  type HostFetchers,
} from "../src/session-accounts-poll";
import type { Item, Sub } from "../src/ipc/chan";
import type { RemoteHostConfig } from "../src/remote-config";
import type { AccountsState, SessionAccount } from "../src/accounts";

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

/**
 * 〔DL1〕**账号那一格经通道订**（`watchAccountsChanged`）—— 替掉裸事件 `remote-backend-ready`。
 *
 * 守的要求：`设计/01 §2.2`「前端只有两个动作：`call` · `subscribe`」；`设计/05 §3.3.5`（订阅不失败，看不见 ⇒ `unseen`）·
 * `§3.3.4`（丢必须说：`gap`）。设计与读数：`调研/第四波记录/DL1.md §3`。
 * 异源：替身 `chan` 只记「订了什么、还了多少 credit」、按用例递格，不经被测代码的任何一行。
 */
describe("watchAccountsChanged（DL1：remote-backend-ready 迁 subscribe）", () => {
  interface Fake {
    ch: AccountsChangedChannel;
    subs: { origin: string; kind: string; from: Uint8Array | null; want: number }[];
    sinks: Map<string, (items: Item[]) => void>;
    wants: Map<string, number[]>;
  }
  function fakeChan(early?: { origin: string; items: Item[] }): Fake {
    const f: Fake = { ch: null as unknown as AccountsChangedChannel, subs: [], sinks: new Map(), wants: new Map() };
    f.ch = {
      subscribe: async (origin, kind, from, want, sink) => {
        f.subs.push({ origin, kind, from, want });
        f.sinks.set(origin, sink);
        f.wants.set(origin, []);
        // 格可能先于 `subscribe` 的返回到达（`chan.ts` 头注）
        if (early && early.origin === origin) sink(early.items);
        const sub: Sub = { want: (n) => f.wants.get(origin)!.push(n), stop: () => {} };
        return sub;
      },
    };
    return f;
  }
  const frame: Item = { t: "frame", seq: 0, body: '{"accounts_changed":true}' };
  const seen: Item = { t: "seen", from: null };
  const gap: Item = { t: "gap", fromSeq: 0, toSeq: 2 };
  const unseen: Item = { t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" };
  const closed: Item = { t: "closed", by: { ours: "Broken" } };

  it("★ 每台恰好订一条，kind 是约定串、不带续传、一开始给约定的 credit", async () => {
    const f = fakeChan();
    await watchAccountsChanged(f.ch, ["<local>", "box-a", "box-b"], () => {});
    expect(f.subs).toEqual(
      ["<local>", "box-a", "box-b"].map((origin) => ({ origin, kind: "accounts-changed", from: null, want: ACCOUNTS_CHANGED_WINDOW })),
    );
  });

  it("★ seen / frame / gap 各叫一次；unseen / closed 不叫（两向）", async () => {
    const f = fakeChan();
    const calls: number[] = [];
    let n = 0;
    await watchAccountsChanged(f.ch, ["box-a"], () => calls.push(++n));
    const sink = f.sinks.get("box-a")!;
    sink([unseen]);
    sink([closed]);
    expect(calls, "unseen / closed 不该刷").toEqual([]);
    sink([seen]);
    sink([frame]);
    sink([gap]);
    expect(calls, "seen / frame / gap 各该刷一次").toEqual([1, 2, 3]);
  });

  it("★ 一批里有几格都只叫一次；frame 用掉的 credit 当场还、别的格不还", async () => {
    const f = fakeChan();
    let calls = 0;
    await watchAccountsChanged(f.ch, ["box-a"], () => calls++);
    f.sinks.get("box-a")!([seen, frame, frame, gap, unseen]);
    expect(calls).toBe(1);
    expect(f.wants.get("box-a")).toEqual([2]);
    f.sinks.get("box-a")!([seen, gap]);
    expect(f.wants.get("box-a"), "没有 frame 的一批不该还 credit").toEqual([2]);
  });

  it("★ 先于 subscribe 返回到达的 frame：credit 记着、拿到 Sub 之后还", async () => {
    const f = fakeChan({ origin: "box-a", items: [frame] });
    let calls = 0;
    await watchAccountsChanged(f.ch, ["box-a"], () => calls++);
    expect(calls).toBe(1);
    expect(f.wants.get("box-a")).toEqual([1]);
  });

  it("★ kind 串两侧相等：TS 常量 == Rust `event_replay.rs::ACCOUNTS_CHANGED_KIND`（从 Rust 源码抠，异源）", () => {
    const rs = readFileSync("src/bridge/src/event_replay.rs", "utf8");
    const m = rs.match(/pub const ACCOUNTS_CHANGED_KIND: &str = "([^"]+)";/);
    expect(m, "Rust 那一侧的常量抠不出来").not.toBeNull();
    expect(ACCOUNTS_CHANGED_KIND).toBe(m![1]);
  });

  it("★ 生产段零处再听裸事件 `remote-backend-ready`；main.ts 恰好一处经通道订它（零命中带正控）", () => {
    const strip = (t: string): string => t.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
    const main = strip(readFileSync("src/main.ts", "utf8"));
    const dead = ["remote", "backend", "ready"].join("-");
    expect(main.includes(`"${dead}"`), "main.ts 又在听那个裸事件").toBe(false);
    expect(main.match(/watchAccountsChanged\(chan, machines,/g)?.length ?? 0, "main.ts 不是恰好一处订 accounts-changed").toBe(1);
    // 正控：同一个剥法与找法认得出一处真在的裸 listen。
    expect(main.includes('listen("remote-session-added"'), "正控失败：识别器认不出一处真在的 listen").toBe(true);
    expect(strip(`listen("${dead}", f);`).includes(`"${dead}"`), "正控失败：剥法把代码剥掉了").toBe(true);
  });
});
