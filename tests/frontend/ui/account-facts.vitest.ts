import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect, vi } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import {
  askHost,
  createAccountFacts,
  createEventRefresher,
  mergeFacts,
  type HostFetchers,
} from "../../../src/frontend/ui/account-facts";
import type { AccountFacts } from "../../../src/frontend/ui/app-store";
import type { Account, AccountsState } from "../../../src/frontend/ui/accounts";

/** 每一问记一笔「哪台 · 哪一问」；`bad` 那台的上次用的号问不到。 */
function probe(): { f: HostFetchers; asked: string[] } {
  const asked: string[] = [];
  const f: HostFetchers = {
    fetchAccounts: async (origin) => {
      asked.push(`${origin} accounts-list`);
      return {
        origin,
        available: origin !== "down",
        accounts: [{ name: `acct-${origin}`, email: `${origin}@x` }],
      } as unknown as AccountsState;
    },
    currentAccountForBadge: (st) => ({ name: `acct-${st.origin}` }) as Account,
    lastAccounts: async (origin) => {
      asked.push(`${origin} history-last-accounts`);
      if (origin === "bad") throw new Error("没有控制通道");
      return { [`s-${origin}`]: `acct-${origin}` };
    },
  };
  return { f, asked };
}

const settle = async (): Promise<void> => {
  for (let i = 0; i < 6; i += 1) await new Promise((r) => setTimeout(r, 0));
};

describe("账号清单那一侧：只在那台清单可能变了时问那一台", () => {
  it("★ 不给台 ⇒ 各台各两问（清单 ＋ 上次用的号）；给一台 ⇒ 只问那一台；不是要问的远端（本机）⇒ 一问不发", async () => {
    const { f, asked } = probe();
    const got: AccountFacts[] = [];
    const feed = createAccountFacts({ fetchers: f, hosts: async () => ["devbox", "nano", "pi"], publish: (x) => got.push(x) });
    feed.refresh();
    await settle();
    expect(asked.sort()).toEqual(
      ["devbox", "nano", "pi"].flatMap((o) => [`${o} accounts-list`, `${o} history-last-accounts`]).sort(),
    );
    asked.length = 0;
    feed.refresh("nano");
    await settle();
    expect(asked.sort()).toEqual(["nano accounts-list", "nano history-last-accounts"]);
    asked.length = 0;
    feed.refresh("<local>");
    await settle();
    expect(asked).toEqual([]);
    // 只重问了一台，另两台的那一份照留着
    expect([...got.at(-1)!.currentByOrigin.keys()]).toEqual(["devbox", "nano", "pi"]);
  });

  it("没有要问的远端 ⇒ 交一份空的（徽章全收）", async () => {
    const { f, asked } = probe();
    const got: AccountFacts[] = [];
    createAccountFacts({ fetchers: f, hosts: async () => [], publish: (x) => got.push(x) }).refresh();
    await settle();
    expect(asked).toEqual([]);
    expect(got.at(-1)?.readyOrigins.size).toBe(0);
  });

  it("那一台从配置里拿掉了 ⇒ 下一份里没有它", async () => {
    const { f } = probe();
    let hosts = ["devbox", "nano"];
    const got: AccountFacts[] = [];
    const feed = createAccountFacts({ fetchers: f, hosts: async () => hosts, publish: (x) => got.push(x) });
    feed.refresh();
    await settle();
    hosts = ["devbox"];
    feed.refresh("devbox");
    await settle();
    expect([...got.at(-1)!.currentByOrigin.keys()]).toEqual(["devbox"]);
  });
});

describe("主界面接线：谁会重问账号清单", () => {
  // 会话起停不在里面：会话跑在哪个号下由那台在宣告里说（`tabs.vitest.ts` 那条「3 台、起停一条会话」管徽章那一半）。
  it("★ 重问只有三处：本界面切号（那一台）· 那台 `changed/accounts`（那一台）· 订阅登记好之后补刷一次（各台）", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src", "frontend", "ui", "main.ts"), "utf8");
    const lines = (needle: RegExp): string[] => src.split("\n").filter((l) => needle.test(l)).map((l) => l.trim());
    expect(lines(/accountFacts\.refresh\(/)).toEqual([
      "onDefaultChanged: (origin: Origin) => accountFacts.refresh(origin),",
      "accountFacts.refresh(origin);",
    ]);
    expect(lines(/onAccountsChanged\(/).filter((l) => !l.startsWith("const "))).toEqual([
      "onAccountsChanged(origin);",
      "onAccountsChanged();",
    ]);
    // 带台的那一处在 `changed` 的 accounts 分支里
    expect(/case "accounts":\s*onAccountsChanged\(origin\);/.test(src)).toBe(true);
  });
});

describe("「上次用哪个号起的」每台各问一次那台，并起来", () => {
  it("★ 两台各自记着的那几条都在；问不到的那一台不拖垮别台（它那几条当没有）", async () => {
    const { f } = probe();
    const per = await Promise.all(["devbox", "bad", "nano"].map(async (o) => [o, await askHost(o, f)] as const));
    const got = mergeFacts(per);
    expect([...got.lastByS]).toEqual([
      ["s-devbox", "acct-devbox"],
      ["s-nano", "acct-nano"],
    ]);
  });
  it("清单不可用的那一台不算可查（徽章不在那台显）", async () => {
    const { f } = probe();
    const got = mergeFacts([["down", await askHost("down", f)] as const, ["pi", await askHost("pi", f)] as const]);
    expect([...got.readyOrigins]).toEqual(["pi"]);
    expect(got.emailByName.get("acct-pi")).toBe("pi@x");
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

