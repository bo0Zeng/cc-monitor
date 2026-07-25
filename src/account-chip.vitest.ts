// A3 account-chip 纯函数测试（选主远端 / chip 文本）。
import { describe, it, expect } from "vitest";
import { pickPrimaryOrigin, chipLabel, AccountChip, type AccountChipDeps } from "./account-chip";
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

// ------------------------------------------------------------ account-ux U6：⚠k 计数（DOM 行为）
// D 审计指出这块此前零覆盖：updateMismatchBadge 的显/隐/ready 门控、菜单对齐入口的 gating，
// 都是 U6 新增的 DOM 行为，却一条测都没有。
describe("account-ux U6 chip ⚠k 不一致计数", () => {
  const mk = (deps: Partial<AccountChipDeps> = {}): AccountChip =>
    new AccountChip({ openSettings: () => {}, ...deps });
  /** 直接注入 chip 内部状态（refresh 要打 IPC，这里只测纯 DOM 判定）。 */
  const seed = (chip: AccountChip, st: AccountsState | null, visible = true): void => {
    (chip as unknown as { state: AccountsState | null }).state = st;
    chip.element.style.display = visible ? "" : "none";
  };
  const badge = (chip: AccountChip): HTMLElement =>
    chip.element.querySelector<HTMLElement>(".status-account-mismatch")!;
  const ready = state({ accounts: [acct({ name: "z", isDefault: true })], defaultName: "z" });

  it("ready + count>0 → 显 ⚠k，且带可读的 aria-label", () => {
    const chip = mk();
    seed(chip, ready);
    chip.updateMismatchBadge(3);
    expect(badge(chip).style.display).not.toBe("none");
    expect(badge(chip).textContent).toBe("⚠3");
    expect(badge(chip).getAttribute("aria-label")).toContain("3");
  });

  it("count=0 → 不显", () => {
    const chip = mk();
    seed(chip, ready);
    chip.updateMismatchBadge(0);
    expect(badge(chip).style.display).toBe("none");
  });

  it("chip 本身隐藏（未连远端）→ 不显", () => {
    const chip = mk();
    seed(chip, ready, false);
    chip.updateMismatchBadge(2);
    expect(badge(chip).style.display).toBe("none");
  });

  it("非 ready（未启用/需更新）→ 不显：菜单里根本没有对齐入口，显了是死胡同", () => {
    const chip = mk();
    seed(chip, state({ accounts: [] })); // 零账号 → deriveUi 判 not-enabled
    chip.updateMismatchBadge(2);
    expect(badge(chip).style.display).toBe("none");
  });

  it("state 未知（还没拉到账号）→ 不显", () => {
    const chip = mk();
    seed(chip, null);
    chip.updateMismatchBadge(2);
    expect(badge(chip).style.display).toBe("none");
  });
});
