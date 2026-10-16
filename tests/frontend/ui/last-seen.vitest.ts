/**
 * 离线那台的上次值：读成了交本机后端记（`last-seen-write`，读成一次交一次、时刻是最后一次读成的那一刻；本机那台不记）；
 * 账号那一问这次运行里还没答成过而那台问不到 ⇒ 问本机后端记着的那一份，照它画「上次的」（跨重启还在）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { calls, store, reachable } = vi.hoisted(() => ({
  calls: [] as Array<{ origin: string; op: string; args: Record<string, unknown> }>,
  store: new Map<string, Record<string, { atMs: number; value: unknown }>>(),
  reachable: new Set<string>(),
}));
const accountsReply = { meta: { enabled: true, acctsDir: "/h/.cc-monitor/accounts", manifestPath: "/h/.cc-monitor/accounts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null, unsupported: null, nextDefault: null, effectiveDefault: null, home: "/h" }, accounts: [], notice: null };
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
      return Promise.reject(new Error(`${origin} 断开`));
    },
  },
}));

import { forgetSeen, recallSeen, rememberSeen } from "../../../src/frontend/ui/last-seen";
import { fetchAccounts } from "../../../src/frontend/ui/account-reads";

const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  calls.length = 0;
  store.clear();
  reachable.clear();
});

describe("上次值交本机后端记", () => {
  it("★ 只交本机后端；读成一次交一次（界面不记交过什么）；本机那台不记；值不是对象不交", async () => {
    await rememberSeen("devbox", "data", { a: 1 });
    await rememberSeen("devbox", "data", { a: 1 });
    await rememberSeen("<local>", "data", { a: 1 });
    await rememberSeen("devbox", "accounts", [1, 2]);
    expect(calls.map((c) => `${c.origin}:${c.op}:${c.args.origin}:${c.args.kind}`)).toEqual(["<local>:last-seen-write:devbox:data", "<local>:last-seen-write:devbox:data"]);
    await rememberSeen("devbox", "data", { a: 2 });
    expect(calls).toHaveLength(3);
    expect(await recallSeen("devbox", "data")).toEqual({ atMs: 42, value: { a: 2 } });
    expect(await recallSeen("devbox", "accounts")).toBeNull();
  });

  it("★ 账号：读成了 ⇒ 记下；那台问不到 ⇒ 照本机后端记着的那一份画上次的（带记下的时刻）—— 同一次运行里也问本机后端，界面不另记一份", async () => {
    reachable.add("devbox");
    const ok = await fetchAccounts("devbox");
    expect(ok.available).toBe(true);
    await settle();
    expect(store.get("devbox")?.accounts?.value).toEqual(accountsReply);
    // 同一次运行里那台断开：上次那份从本机后端来（时刻是它记下的那一刻），不是界面手里那份。
    reachable.clear();
    const off = await fetchAccounts("devbox");
    expect(off.available).toBe(false);
    expect(off.last?.atMs).toBe(42);
    expect(off.last?.meta.enabled).toBe(true);
    // 界面这一侧不记任何东西：画上次的只凭本机后端那一份（重启也一样）。
    expect((await fetchAccounts("devbox")).last?.atMs).toBe(42);
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
    expect(calls.map((c) => c.op), "同名那台再加回来：照样交").toEqual(["last-seen-write"]);
    calls.length = 0;
    await forgetSeen("<local>");
    expect(calls, "本机那台不记，也不清").toEqual([]);
  });
});
