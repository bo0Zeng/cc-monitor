// A3 account-chip 纯函数测试（选主远端 / chip 文本）。
import { describe, it, expect } from "vitest";
import { pickPrimaryOrigin, chipLabel } from "./account-chip";
import type { RemoteHostConfig } from "./settings/remote-section";
import type { AccountsState, Account } from "./accounts";

function host(p: Partial<RemoteHostConfig>): RemoteHostConfig {
  return {
    label: "",
    host: "h",
    port: 22,
    user: "u",
    keyPath: "",
    daemonPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless: false,
    ...p,
  };
}
function acct(p: Partial<Account>): Account {
  return {
    name: "z",
    email: "z@x.edu",
    configDir: "/h/.claude-accts/z",
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    ...p,
  };
}
function state(p: Partial<AccountsState>): AccountsState {
  return {
    origin: "aya",
    available: true,
    error: null,
    meta: {
      enabled: true,
      acctsDir: "/a",
      manifestPath: "/a/accounts.json",
      updatedAt: null,
      sharedStore: null,
      count: 0,
      error: null,
    },
    accounts: [],
    defaultName: null,
    ...p,
  };
}

describe("pickPrimaryOrigin", () => {
  it("取第一台非 daemonless", () => {
    expect(pickPrimaryOrigin([host({ label: "a" }), host({ label: "b" })])).toBe("a");
  });
  it("跳过 daemonless", () => {
    expect(pickPrimaryOrigin([host({ label: "a", daemonless: true }), host({ label: "b" })])).toBe("b");
  });
  it("label 空 → 用 host", () => {
    expect(pickPrimaryOrigin([host({ label: "", host: "aya.local" })])).toBe("aya.local");
  });
  it("全 daemonless → null", () => {
    expect(pickPrimaryOrigin([host({ daemonless: true })])).toBeNull();
  });
  it("空列表 → null", () => {
    expect(pickPrimaryOrigin([])).toBeNull();
  });
});

describe("chipLabel", () => {
  it("无 state → 未连远端", () => {
    expect(chipLabel(null)).toBe("未连远端");
  });
  it("daemonless（hidden）→ 空串（调用方隐藏）", () => {
    expect(chipLabel(state({ available: false, error: "该主机 daemonless" }))).toBe("");
  });
  it("旧 daemon → daemon 需更新", () => {
    expect(chipLabel(state({ available: false, error: "版本过旧" }))).toBe("daemon 需更新");
  });
  it("未启用 → 未启用", () => {
    expect(
      chipLabel(state({ meta: { enabled: false, acctsDir: "/a", manifestPath: "/a/x", updatedAt: null, sharedStore: null, count: 0, error: null } })),
    ).toBe("未启用");
  });
  it("ready → 显示当前默认账号名", () => {
    const s = state({ accounts: [acct({ name: "z", isDefault: true }), acct({ name: "b" })], defaultName: "b" });
    expect(chipLabel(s)).toBe("b");
  });
  it("ready 无 defaultName → 跟随 manifest isDefault", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })] });
    expect(chipLabel(s)).toBe("b");
  });
});
