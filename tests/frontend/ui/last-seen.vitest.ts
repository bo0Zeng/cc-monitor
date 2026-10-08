/**
 * 离线那台的上次值：读成了交本机后端记（`last-seen-write`，同一份这次运行里只交一次、本机那台不记）；
 * 账号那一问这次运行里还没答成过而那台问不到 ⇒ 问本机后端记着的那一份，照它画「上次的」（跨重启还在）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { calls, store, reachable } = vi.hoisted(() => ({
  calls: [] as Array<{ origin: string; op: string; args: Record<string, unknown> }>,
  store: new Map<string, Record<string, { atMs: number; value: unknown }>>(),
  reachable: new Set<string>(),
}));
const accountsReply = { meta: { enabled: true, acctsDir: "/h/.cc-monitor/accounts", manifestPath: "/h/.cc-monitor/accounts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null, unsupported: null, nextDefault: null, home: "/h" }, accounts: [], notice: null };
vi.mock("../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      const args = JSON.parse(new TextDecoder().decode(body)) as Record<string, unknown>;
      calls.push({ origin, op, args });
      const enc = (v: unknown) => Promise.resolve(new TextEncoder().encode(JSON.stringify(v)));
      if (op === "last-seen-write" && args.forget === true) {
        store.delete(args.origin as string);
        return enc({ atMs: 43 });
      }
      if (op === "last-seen-write") {
        store.set(args.origin as string, { ...(store.get(args.origin as string) ?? {}), [args.kind as string]: { atMs: 42, value: args.value } });
        return enc({ atMs: 42 });
      }
      if (op === "last-seen-read") {
        const m = store.get(args.origin as string) ?? {};
        return enc({ accounts: m.accounts ?? null, data: m.data ?? null });
      }
      if (op === "accounts-list" && reachable.has(origin)) return enc(accountsReply);
      return Promise.reject(new Error(`${origin} 连不上`));
    },
  },
}));

import { forgetSeen, recallSeen, rememberSeen, __resetLastSeenForTests } from "../../../src/frontend/ui/last-seen";
import { fetchAccounts, __resetAccountsCacheForTest } from "../../../src/frontend/ui/account-reads";

const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  calls.length = 0;
  store.clear();
  reachable.clear();
  __resetLastSeenForTests();
  __resetAccountsCacheForTest();
});

describe("上次值交本机后端记", () => {
  it("★ 只交本机后端；同一份只交一次；本机那台不记；值不是对象不交", async () => {
    await rememberSeen("devbox", "data", { a: 1 });
    await rememberSeen("devbox", "data", { a: 1 });
    await rememberSeen("<local>", "data", { a: 1 });
    await rememberSeen("devbox", "accounts", [1, 2]);
    expect(calls.map((c) => `${c.origin}:${c.op}:${c.args.origin}:${c.args.kind}`)).toEqual(["<local>:last-seen-write:devbox:data"]);
    await rememberSeen("devbox", "data", { a: 2 });
    expect(calls).toHaveLength(2);
    expect(await recallSeen("devbox", "data")).toEqual({ atMs: 42, value: { a: 2 } });
    expect(await recallSeen("devbox", "accounts")).toBeNull();
  });

  it("★ 账号：读成了 ⇒ 记下；「重启」后那台问不到 ⇒ 照本机后端记着的那一份画上次的（带记下的时刻）", async () => {
    reachable.add("devbox");
    const ok = await fetchAccounts("devbox", true);
    expect(ok.available).toBe(true);
    await settle();
    expect(store.get("devbox")?.accounts?.value).toEqual(accountsReply);
    // 「重启」：界面这一侧的记忆都没了，那台这回连不上。
    __resetAccountsCacheForTest();
    __resetLastSeenForTests();
    reachable.clear();
    const off = await fetchAccounts("devbox", true);
    expect(off.available).toBe(false);
    expect(off.last?.atMs).toBe(42);
    expect(off.last?.meta.enabled).toBe(true);
  });

  it("★ 删机器：交本机后端清掉那台（只带 origin ＋ forget）；之后问不到它；同名再加回来、同一份照样再交", async () => {
    await rememberSeen("devbox", "data", { a: 1 });
    await rememberSeen("gpu-01", "data", { b: 1 });
    calls.length = 0;
    await forgetSeen("devbox");
    expect(calls.map((c) => [c.origin, c.op, c.args])).toEqual([["<local>", "last-seen-write", { origin: "devbox", forget: true }]]);
    expect(await recallSeen("devbox", "data")).toBeNull();
    expect(await recallSeen("gpu-01", "data")).toEqual({ atMs: 42, value: { b: 1 } });
    calls.length = 0;
    await rememberSeen("devbox", "data", { a: 1 });
    expect(calls.map((c) => c.op), "同名那台再加回来：这次运行里交过的记忆跟着清").toEqual(["last-seen-write"]);
    calls.length = 0;
    await forgetSeen("<local>");
    expect(calls, "本机那台不记，也不清").toEqual([]);
  });
});
