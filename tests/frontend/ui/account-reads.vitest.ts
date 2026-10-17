/**
 * 账号读面里另外几口：信任预检（`accounts-trust`）答成 / 没答成两形 · 账号页那一家的画像与 `launchAgentId` 同一个出处 ·
 * 本机后端记着的上次那份不是这一版 ⇒ 不画上次的。账号清单那一问本身见 `accounts.vitest.ts` · `last-seen.vitest.ts`。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { calls, replies } = vi.hoisted(() => ({
  calls: [] as Array<{ origin: string; op: string; args: Record<string, unknown> }>,
  replies: new Map<string, unknown>(),
}));
vi.mock("../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      const args = JSON.parse(new TextDecoder().decode(body)) as Record<string, unknown>;
      calls.push({ origin, op, args });
      const key = `${origin}:${op}`;
      if (!replies.has(key)) return Promise.reject(new Error(`${origin} 断开`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(replies.get(key))));
    },
  },
}));

import { accountsAgentProfile, checkTrust, fetchAccounts, launchAgentId } from "../../../src/frontend/ui/account-reads";

beforeEach(() => {
  calls.length = 0;
  replies.clear();
});

describe("信任预检 `checkTrust`", () => {
  it("★ 答成 ⇒ 原样交回那台判的两格；入参照收（configDir 为 null 问账号 0）", async () => {
    replies.set("devbox:accounts-trust", { trusted: true, known: false });
    expect(await checkTrust("devbox", null, "/w")).toEqual({ available: true, error: null, trusted: true, known: false });
    expect(calls).toEqual([{ origin: "devbox", op: "accounts-trust", args: { configDir: null, cwd: "/w" } }]);
  });
  it("★ 没答成 ⇒ available:false ＋ 一句人话，信任两格都说不知道（调用方只警告不拦）", async () => {
    const r = await checkTrust("devbox", "/h/.claude-alt/z", "/w");
    expect(r.available).toBe(false);
    expect([r.trusted, r.known]).toEqual([false, false]);
    expect(typeof r.error === "string" && r.error.length > 0, String(r.error)).toBe(true);
  });
});

describe("账号页那一家的画像", () => {
  it("★ 与 `launchAgentId` 同一个出处：画像那一行的 agent 就是它", () => {
    const p = accountsAgentProfile();
    expect(p).not.toBeNull();
    expect(p?.agent).toBe(launchAgentId());
  });
});

describe("默认那一家不在画像表里", () => {
  it("★ 抛表里那一句，不回落到任何一家", async () => {
    vi.resetModules();
    vi.doMock("../../../src/frontend/ui/agent-profile", async (orig) => ({ ...(await orig<object>()), DEFAULT_AGENT: "no-such-agent" }));
    try {
      const m = await import("../../../src/frontend/ui/account-reads");
      expect(() => m.launchAgentId()).toThrow(/no-such-agent/);
      expect(() => m.accountsAgentProfile()).toThrow(/no-such-agent/);
    } finally {
      vi.doUnmock("../../../src/frontend/ui/agent-profile");
      vi.resetModules();
    }
  });
});

describe("上次那份不是这一版", () => {
  it("★ 那台问不到、本机后端说记着的那份读不懂 ⇒ 不画上次的（last 为 null），此刻照旧不可用", async () => {
    replies.set("<local>:last-seen-read", { accounts: { state: "unreadable", atMs: 7 }, data: null });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const off = await fetchAccounts("devbox");
    expect(off.available).toBe(false);
    expect(off.last).toBeNull();
    expect(warn.mock.calls.some((c) => String(c[0]).includes("读不懂")), "读不懂要进日志").toBe(true);
    warn.mockRestore();
  });
});
