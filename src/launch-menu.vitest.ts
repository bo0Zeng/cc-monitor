import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Account, AccountsState } from "./accounts.ts";

const fetchAccountsMock = vi.fn<(origin: string) => Promise<AccountsState>>();

vi.mock("./accounts.ts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./accounts.ts")>();
  return { ...actual, fetchAccounts: (origin: string) => fetchAccountsMock(origin) };
});

import { enumerateAccountModifiers } from "./launch-menu.ts";

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

describe("enumerateAccountModifiers", () => {
  beforeEach(() => {
    fetchAccountsMock.mockReset();
  });

  // R05：原先有两条纯测 container 组的用例 + 一条测"account 组恒排在 container 组之前"的用例
  // ——container 组已作为死代码删除（全仓唯一生产调用点从不读它），那三条随之删除：
  // 它们测的是从未被渲染过的东西。另有三条原先用"groups 只剩 container"来表达"account 组不出现"，
  // 现改为直接断言空数组——**断言的行为没变，只是不再借一个死组来表达**。
  it("恰好 1 个可选账号 → 只含基座逃生口（无需切到唯一账号）", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z")]));
    expect(await enumerateAccountModifiers("host")).toEqual([{ kind: "base", label: "基座（不隔离）" }]);
  });

  it("0 可选账号（无账号/全不可选）→ 空数组", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z", { loggedIn: false })]));
    expect(await enumerateAccountModifiers("host")).toEqual([]);
  });

  it("≥2 可选账号 → 基座 + 每个可选账号，过滤掉不可选的", async () => {
    fetchAccountsMock.mockResolvedValue(
      state([acct("z"), acct("b"), acct("dead", { loggedIn: false })]),
    );
    expect(await enumerateAccountModifiers("host")).toEqual([
      { kind: "base", label: "基座（不隔离）" },
      { kind: "account", name: "z", label: "z" },
      { kind: "account", name: "b", label: "b" },
    ]);
  });

  // R05：判别联合取代裸魔法串 `"__base__"` 之后，"哪一项是基座"由 `kind` 回答。
  // 这条钉住的是**类型化本身的收益**：基座项在类型上没有 `name` 字段，
  // 所以"把基座当成一个名叫 __base__ 的账号"这类错误不再可表达。
  it("基座项用 kind:\"base\" 标识，且不带账号名（不再是裸字符串 __base__）", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z"), acct("b")]));
    const opts = await enumerateAccountModifiers("host");
    expect(opts[0].kind).toBe("base");
    expect("name" in opts[0]).toBe(false);
    expect(opts.filter((o) => o.kind === "account").map((o) => o.label)).toEqual(["z", "b"]);
    // 全仓不该再出现这个魔法串（本断言只覆盖返回值形状，跨文件由类型系统保证）
    expect(JSON.stringify(opts)).not.toContain("__base__");
  });

  // F09 Phase D 审计（后端架构，重要）：`isSelectable` 通过但 `configDir` 落空的账号——旧版
  // `appendAccountMenuItems` 会 `continue`（静默隐藏），本函数**有意不**复刻这条过滤，锁定
  // 当前行为（显示、留给点击后的 `onUnselectable` 反馈），防止以后有人"顺手"补回那条 filter
  // 当成 bug 修。
  it("isSelectable 但 configDir 落空的账号仍出现在选项里（不静默隐藏，交给点击后的反馈）", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z"), acct("weird", { configDir: "" })]));
    const opts = await enumerateAccountModifiers("host");
    expect(opts.map((o) => (o.kind === "base" ? "__BASE__" : o.name))).toEqual([
      "__BASE__",
      "z",
      "weird",
    ]);
  });

  it("账号功能不可用（available:false）→ 空数组，不抛异常", async () => {
    fetchAccountsMock.mockResolvedValue(state([acct("z"), acct("b")], false));
    expect(await enumerateAccountModifiers("host")).toEqual([]);
  });

  it("fetchAccounts 抛异常 → 空数组，不向上抛（容错降级同 appendAccountMenuItems 既有模式）", async () => {
    fetchAccountsMock.mockRejectedValue(new Error("network"));
    expect(await enumerateAccountModifiers("host")).toEqual([]);
  });
});
