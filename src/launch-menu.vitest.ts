import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Account, AccountsState } from "./accounts.ts";

const fetchAccountsMock = vi.fn<(origin: string) => Promise<AccountsState>>();

vi.mock("./accounts.ts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./accounts.ts")>();
  return { ...actual, fetchAccounts: (origin: string) => fetchAccountsMock(origin) };
});

import { enumerateModifierGroups } from "./launch-menu.ts";

function acct(name: string, opts: Partial<Account> = {}): Account {
  return {
    name,
    email: `${name}@example.com`,
    configDir: `/home/u/.claude-accts/${name}`,
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    ...opts,
  };
}

function state(accounts: Account[], available = true): AccountsState {
  return { origin: "host", available, error: null, meta: null, accounts, defaultName: null };
}

describe("enumerateModifierGroups", () => {
  beforeEach(() => {
    fetchAccountsMock.mockReset();
  });

  it("container 组恒 2 项，selected 标记对应当前 kind", async () => {
    fetchAccountsMock.mockResolvedValue(state([]));
    const groups = await enumerateModifierGroups("host", "tmux");
    const container = groups.find((g) => g.id === "container");
    expect(container?.options).toEqual([
      { id: "tmux", label: "tmux", selected: true },
      { id: "none", label: "直连（不建 tmux）", selected: false },
    ]);
  });

  it("container 组 selected 跟随 currentContainerKind=none", async () => {
    fetchAccountsMock.mockResolvedValue(state([]));
    const groups = await enumerateModifierGroups("host", "none");
    const container = groups.find((g) => g.id === "container");
    expect(container?.options.find((o) => o.id === "none")?.selected).toBe(true);
    expect(container?.options.find((o) => o.id === "tmux")?.selected).toBe(false);
  });

  it("恰好 1 个可选账号 → account 组出现但只含基座逃生口（无需切到唯一账号）", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z")]));
    const groups = await enumerateModifierGroups("host", "tmux");
    const account = groups.find((g) => g.id === "account");
    expect(account?.options.map((o) => o.id)).toEqual(["__base__"]);
  });

  it("0 可选账号（无账号/全不可选）→ account 组不出现", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z", { loggedIn: false })]));
    const groups = await enumerateModifierGroups("host", "tmux");
    expect(groups.map((g) => g.id)).toEqual(["container"]);
  });

  it("≥2 可选账号 → account 组出现，含基座 + 每个可选账号，过滤掉不可选的", async () => {
    fetchAccountsMock.mockResolvedValue(
      state([acct("z"), acct("b"), acct("dead", { loggedIn: false })]),
    );
    const groups = await enumerateModifierGroups("host", "tmux");
    const account = groups.find((g) => g.id === "account");
    expect(account?.options.map((o) => o.id)).toEqual(["__base__", "z", "b"]);
  });

  // F09 Phase D 审计（后端架构，重要）：`isSelectable` 通过但 `configDir` 落空的账号——旧版
  // `appendAccountMenuItems` 会 `continue`（静默隐藏），本函数**有意不**复刻这条过滤，锁定
  // 当前行为（显示、留给点击后的 `onUnselectable` 反馈），防止以后有人"顺手"补回那条 filter
  // 当成 bug 修。
  it("isSelectable 但 configDir 落空的账号仍出现在选项里（不静默隐藏，交给点击后的反馈）", async () => {
    fetchAccountsMock.mockResolvedValue(
      state([acct("z"), acct("weird", { configDir: "" })]),
    );
    const groups = await enumerateModifierGroups("host", "tmux");
    const account = groups.find((g) => g.id === "account");
    expect(account?.options.map((o) => o.id)).toEqual(["__base__", "z", "weird"]);
  });

  it("account 组恒排在 container 组之前", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z"), acct("b")]));
    const groups = await enumerateModifierGroups("host", "tmux");
    expect(groups.map((g) => g.id)).toEqual(["account", "container"]);
  });

  it("账号功能不可用（available:false）→ account 组不出现,不抛异常", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z"), acct("b")], false));
    const groups = await enumerateModifierGroups("host", "tmux");
    expect(groups.map((g) => g.id)).toEqual(["container"]);
  });

  it("fetchAccounts 抛异常 → account 组不出现,不向上抛（容错降级同 appendAccountMenuItems 既有模式）", async () => {
    fetchAccountsMock.mockRejectedValue(new Error("network"));
    const groups = await enumerateModifierGroups("host", "tmux");
    expect(groups.map((g) => g.id)).toEqual(["container"]);
  });
});
