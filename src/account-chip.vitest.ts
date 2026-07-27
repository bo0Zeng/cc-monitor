// A3 account-chip 纯函数测试（选主远端 / chip 文本）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const readRemoteConfigMock = vi.fn();
const fetchAccountsMock = vi.fn();
vi.mock("./remote-config", () => ({ readRemoteConfig: () => readRemoteConfigMock() }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { pickPrimaryOrigin, chipLabel, AccountChip } from "./account-chip";
import type { RemoteHostConfig } from "./remote-config";
import type { AccountsState, Account } from "./accounts";
import * as accountsMod from "./accounts";

beforeEach(() => {
  vi.restoreAllMocks();
  readRemoteConfigMock.mockReset();
  fetchAccountsMock.mockReset();
  vi.spyOn(accountsMod, "fetchAccounts").mockImplementation(() => fetchAccountsMock());
});

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

// ------------------------------------------------------------ F1：chip = 纯全局切换器（去 ⚠k）
// F1 把 chip 收敛成 CCSwitcher 式纯全局切换器：⚠k 不一致计数 + 批量对齐入口都移出（批量对齐走
// 命令面板）。这里锁住「chip 不再长出 ⚠k 徽章」——防有人把它加回来。
describe("F1 chip 纯全局切换器（无 ⚠k）", () => {
  it("chip 结构里不含 ⚠k 计数 span（已移出，批量对齐走命令面板）", () => {
    const chip = new AccountChip({ openSettings: () => {} });
    expect(chip.element.querySelector(".status-account-mismatch")).toBeNull();
  });
  it("AccountChip 不再暴露 updateMismatchBadge / alignAll", () => {
    const chip = new AccountChip({ openSettings: () => {} });
    expect((chip as unknown as Record<string, unknown>).updateMismatchBadge).toBeUndefined();
  });

  it("下拉列出账号 + 点非当前项 → 走 setDefaultName 全局切号（DoD 正路）", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "aya" })] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "wei" }), acct({ name: "amy" })], defaultName: "wei" }),
    );
    const setDef = vi.spyOn(accountsMod, "setDefaultName").mockResolvedValue(undefined);
    vi.spyOn(accountsMod, "invalidateAccountsCache").mockImplementation(() => {});
    let changed = 0;
    const chip = new AccountChip({ openSettings: () => {}, onDefaultChanged: () => (changed += 1) });
    await chip.refresh();
    await chip.openMenu();
    const items = document.querySelectorAll<HTMLButtonElement>(".account-picker-item");
    expect(items.length).toBe(2); // 下拉列出两个账号（全局切换器）
    const amy = [...items].find((b) => b.textContent?.includes("amy"))!;
    amy.click();
    // selectDefault 链：setDefaultName → invalidateCache → refresh(含两次 async 数据源) → onDefaultChanged。
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(setDef).toHaveBeenCalledWith("amy"); // 点非当前项 → 全局切到 amy
    expect(changed).toBe(1); // 切完回调 onDefaultChanged（让 main.ts 重算会话归属）
  });
});

// ------------------------------------------------------ U8：chip 头像的休眠（此前零覆盖）
// D 审计指出：U4 引入头像时没测，U8 加门控也没补 —— 删掉门控不会红。休眠现在**只**作用于
// 这一处（tab 徽章是「信息才显」，不是颜色噪音，不该睡），所以这里更得锁住。
// 注意：这里**走真实的 refresh() 路径**（mock 掉两个数据源），不在测试里重抄一遍渲染分支——
// 抄一遍就等于测自己，删掉实现里的门控也不会红。
describe("account-ux U8 chip 头像休眠", () => {
  const icon = (chip: AccountChip): HTMLElement =>
    chip.element.querySelector<HTMLElement>(".status-account-icon")!;

  async function mountWith(st: AccountsState): Promise<AccountChip> {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "aya" })] });
    fetchAccountsMock.mockResolvedValue(st);
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    return chip;
  }

  it("≥2 可选账号 → 显彩色头像", async () => {
    const chip = await mountWith(
      state({ accounts: [acct({ name: "wei" }), acct({ name: "amy" })], defaultName: "wei" }),
    );
    expect(icon(chip).querySelector(".acct-avatar")).not.toBeNull();
  });

  it("只有 1 个可选账号 → 退回 👤（颜色此时区分不了任何东西）", async () => {
    const chip = await mountWith(state({ accounts: [acct({ name: "wei" })], defaultName: "wei" }));
    expect(icon(chip).querySelector(".acct-avatar")).toBeNull();
    expect(icon(chip).textContent).toBe("👤");
  });

  it("2 个账号但只有 1 个可选 → 仍休眠（数可选数，不是总数）", async () => {
    const chip = await mountWith(
      state({
        accounts: [acct({ name: "wei" }), acct({ name: "amy", loggedIn: false })],
        defaultName: "wei",
      }),
    );
    expect(icon(chip).querySelector(".acct-avatar")).toBeNull();
  });
});
