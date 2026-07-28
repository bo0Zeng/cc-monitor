// A3 account-chip 纯函数测试（选主远端 / chip 文本）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const readRemoteConfigMock = vi.fn();
const fetchAccountsMock = vi.fn();
const invokeMock = vi.fn();
vi.mock("./remote-config", () => ({ readRemoteConfig: () => readRemoteConfigMock() }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));
// F10：account-usage.ts 走 invoke("account_usage",...)——这个文件之前不需要 mock
// @tauri-apps/api/core（chip 自己不直接调 invoke，只经 fetchAccounts），现在新增用量
// 懒加载路径需要它。
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import {
  pickPrimaryOrigin,
  chipLabel,
  formatUsageSummaryCompact,
  formatUsageSummaryForMenu,
  AccountChip,
} from "./account-chip";
import type { RemoteHostConfig } from "./remote-config";
import type { AccountsState, Account } from "./accounts";
import * as accountsMod from "./accounts";
import { invalidateAccountUsageCache } from "./account-usage";
import { showActionFailureToast } from "./error-toast";

beforeEach(() => {
  vi.restoreAllMocks();
  readRemoteConfigMock.mockReset();
  fetchAccountsMock.mockReset();
  invokeMock.mockReset().mockResolvedValue(undefined);
  vi.spyOn(accountsMod, "fetchAccounts").mockImplementation(() => fetchAccountsMock());
  // F10：account-usage.ts 的去抖缓存是模块级单例，跨测试文件全程存活——每个测试用例开始前
  // 清空，否则某条测试（如触发 chip.openMenu() 却没显式 mock account_usage）留下的
  // probe-failed 缓存条目会让后面用同一 origin/账号名的测试误判"缓存命中,不该重新 invoke"。
  invalidateAccountUsageCache();
  // F10 Phase D 审计排障发现：多条既有测试打开 chip 菜单后从不显式关闭（`toggleMenu` 把菜单
  // append 到 `document.body`，不像 tabs.ts 的上下文菜单那样每次开新的前先关旧的）——留下的
  // 陈旧 `.account-picker` 菜单会一直挂在全局 DOM 里，后面用 `document.querySelector(...)`
  // 全局查询的测试可能命中的是上一条测试遗留的菜单而不是本次刚开的（同 `tabs.vitest.ts` 的
  // `.tab-context-menu` 清理惯例，这里补一份）。
  document.querySelectorAll(".account-picker").forEach((n) => n.remove());
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
// F1 把 chip 收敛成 CCSwitcher 式纯全局切换器：⚠k 不一致计数入口移出 chip（当时走命令面板；
// 批量对齐/命令面板对齐命令随 F09 一并删除，现在两者都不在了）。这里锁住「chip 不再长出
// ⚠k 徽章」——防有人把它加回来。
describe("F1 chip 纯全局切换器（无 ⚠k）", () => {
  it("chip 结构里不含 ⚠k 计数 span（已移出，F09 后批量对齐整体删除）", () => {
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

describe("formatUsageSummaryCompact", () => {
  it("单窗口 → 纯百分比", () => {
    expect(formatUsageSummaryCompact({ status: "ok", buckets: [{ label: "会话", usedPercent: 38 }] })).toBe("38%");
  });
  it("多窗口 → 斜杠分隔，只带一个尾随 %", () => {
    expect(
      formatUsageSummaryCompact({
        status: "ok",
        buckets: [
          { label: "会话", usedPercent: 38 },
          { label: "每周", usedPercent: 71 },
        ],
      }),
    ).toBe("38/71%");
  });
  it("非 ok 态一律空串（不占地方，不是每次都要展示失败原因）", () => {
    expect(formatUsageSummaryCompact({ status: "not-logged-in" })).toBe("");
    expect(formatUsageSummaryCompact({ status: "cli-missing" })).toBe("");
    expect(formatUsageSummaryCompact({ status: "unrecognized", reason: "x" })).toBe("");
    expect(formatUsageSummaryCompact({ status: "probe-failed", error: "x" })).toBe("");
  });
  it("ok 但零桶（理论不可达，纵深防御）→ 空串", () => {
    expect(formatUsageSummaryCompact({ status: "ok", buckets: [] })).toBe("");
  });
});

// F10 Phase D 审计（UX，重要）：菜单里当前账号那一行富余空间放得下失败短句，不该跟"没查过"
// 一样空白——`formatUsageSummaryForMenu` 是与折叠态 `formatUsageSummaryCompact` 分开的格式化，
// 只有 ok 态两者一致，其余四态菜单版本给出可读短句。
describe("formatUsageSummaryForMenu", () => {
  it("ok 态与 formatUsageSummaryCompact 一致", () => {
    const outcome = { status: "ok" as const, buckets: [{ label: "会话", usedPercent: 38 }] };
    expect(formatUsageSummaryForMenu(outcome)).toBe(formatUsageSummaryCompact(outcome));
  });
  it("四种失败态各给可读短句（不是空串）", () => {
    expect(formatUsageSummaryForMenu({ status: "not-logged-in" })).toBe("未登录");
    expect(formatUsageSummaryForMenu({ status: "cli-missing" })).toBe("无 claude");
    expect(formatUsageSummaryForMenu({ status: "unrecognized", reason: "x" })).toBe("读不到");
    expect(formatUsageSummaryForMenu({ status: "probe-failed", error: "x" })).toBe("探测失败");
  });
});

// F10：菜单展开懒加载当前账号用量——只在展开时探测（不是 refresh()/app 启动时），回填折叠态
// chip 摘要 + 菜单当前账号行；"刷新用量"与"刷新"（账号列表）语义分开。
describe("F10 chip 用量摘要：菜单展开懒加载", () => {
  function mockUsageInvoke(resp: { captured: boolean; raw?: string | null; error?: string | null }): void {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "account_usage"
        ? Promise.resolve({ captured: resp.captured, raw: resp.raw ?? null, error: resp.error ?? null })
        : Promise.resolve(undefined),
    );
  }
  const flush = async (): Promise<void> => {
    for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
  };

  async function mountReady(): Promise<AccountChip> {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "aya" })] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "wei" }), acct({ name: "amy" })], defaultName: "wei" }),
    );
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    return chip;
  }

  it("刷新态（refresh）不触发用量探测——较重操作不该背在轻量调用上", async () => {
    mockUsageInvoke({ captured: true, raw: "50%" });
    await mountReady();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "account_usage")).toBe(false);
  });

  it("展开菜单 → 懒加载当前账号用量，回填折叠态 chip 与菜单当前账号行", async () => {
    mockUsageInvoke({ captured: true, raw: "Current session\n  38%\nResets in 2h" });
    const chip = await mountReady();
    await chip.openMenu();
    await flush();
    expect(chip.element.querySelector(".status-account-usage")?.textContent).toBe("38%");
    const currentRow = document.querySelector(".account-picker-item.current");
    expect(currentRow?.querySelector(".account-picker-usage")?.textContent).toBe("38%");
  });

  it("非当前账号行不懒加载用量（只有当前账号那行探测）", async () => {
    mockUsageInvoke({ captured: true, raw: "38%" });
    const chip = await mountReady();
    await chip.openMenu();
    await flush();
    const items = document.querySelectorAll(".account-picker-item");
    const nonCurrent = [...items].find((el) => !el.classList.contains("current"))!;
    expect(nonCurrent.querySelector(".account-picker-usage")).toBeNull();
    // 只对当前账号（wei）探测了一次，不是每个账号各探一次。
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "account_usage")).toHaveLength(1);
  });

  it("失败态（如未安装 tmux）→ 折叠态摘要保持空（不占地方，不强行显示错误文案）", async () => {
    mockUsageInvoke({ captured: false, error: "远端未安装 tmux" });
    const chip = await mountReady();
    await chip.openMenu();
    await flush();
    expect(chip.element.querySelector(".status-account-usage")?.textContent).toBe("");
  });

  it("「刷新用量」动作存在且与「刷新」（账号列表）分开", async () => {
    mockUsageInvoke({ captured: true, raw: "50%" });
    const chip = await mountReady();
    await chip.openMenu();
    await flush();
    const actions = [...document.querySelectorAll(".account-picker-action")].map((b) => b.textContent);
    expect(actions).toContain("刷新");
    expect(actions).toContain("刷新用量");
  });

  it("点击「刷新用量」忽略去抖缓存，重新 invoke（force）", async () => {
    mockUsageInvoke({ captured: true, raw: "50%" });
    const chip = await mountReady();
    await chip.openMenu();
    await flush();
    const before = invokeMock.mock.calls.filter(([cmd]) => cmd === "account_usage").length;
    const refreshUsageBtn = [...document.querySelectorAll<HTMLButtonElement>(".account-picker-action")].find(
      (b) => b.textContent === "刷新用量",
    )!;
    refreshUsageBtn.click();
    await flush();
    const after = invokeMock.mock.calls.filter(([cmd]) => cmd === "account_usage").length;
    expect(after).toBe(before + 1);
  });

  it("点击「刷新用量」完成后给出 toast（menuAction 会立刻关闭菜单，用户看不到过程，靠 toast 反馈）", async () => {
    mockUsageInvoke({ captured: true, raw: "Current session\n  50%\nResets in 1h" });
    const chip = await mountReady();
    await chip.openMenu();
    await flush();
    const refreshUsageBtn = [...document.querySelectorAll<HTMLButtonElement>(".account-picker-action")].find(
      (b) => b.textContent === "刷新用量",
    )!;
    refreshUsageBtn.click();
    await flush();
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "用量已刷新",
      expect.stringContaining("50%"),
      expect.objectContaining({ level: "info" }),
    );
  });

  it("菜单展开后当前账号行先显示占位「…」，resolve 后才换成真实结果（此前完全空白,跟探测失败/没查过无法区分）", async () => {
    let resolveInvoke!: (v: unknown) => void;
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "account_usage" ? new Promise((r) => (resolveInvoke = r)) : Promise.resolve(undefined),
    );
    const chip = await mountReady();
    await chip.openMenu();
    const currentRow = document.querySelector(".account-picker-item.current");
    expect(currentRow?.querySelector(".account-picker-usage")?.textContent).toBe("…");
    resolveInvoke({ captured: true, raw: "50%" });
    await flush();
    expect(currentRow?.querySelector(".account-picker-usage")?.textContent).not.toBe("…");
  });

  it("F10 Phase D 审计（UX，阻塞）：探测期间切换当前账号 → 姗姗来迟的结果不会误标到新账号的折叠态 chip 上", async () => {
    let resolveWeiProbe!: (v: unknown) => void;
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "account_usage" ? new Promise((r) => (resolveWeiProbe = r)) : Promise.resolve(undefined),
    );
    const setDef = vi.spyOn(accountsMod, "setDefaultName").mockResolvedValue(undefined);
    vi.spyOn(accountsMod, "invalidateAccountsCache").mockImplementation(() => {});
    const chip = await mountReady(); // 当前账号 = wei
    await chip.openMenu(); // 对 wei 发起探测（挂起，尚未 resolve）

    // 切到 amy——selectDefault 内部会 closeMenu + refresh(true)，refresh 会先清空 usageSpan。
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "wei" }), acct({ name: "amy" })], defaultName: "amy" }),
    );
    const items = document.querySelectorAll<HTMLButtonElement>(".account-picker-item");
    const amyRow = [...items].find((b) => b.textContent?.includes("amy"))!;
    amyRow.click();
    await flush();
    expect(setDef).toHaveBeenCalledWith("amy");
    expect(chip.element.querySelector(".status-account-usage")?.textContent).toBe(""); // 切号后先清空

    // wei 那次挂起的探测这时才姗姗来迟地 resolve——不该覆盖折叠态（当前账号已经是 amy）。
    resolveWeiProbe({ captured: true, raw: "Current session\n  99%\nResets in 1h" });
    await flush();
    expect(chip.element.querySelector(".status-account-usage")?.textContent).not.toBe("99%");
  });
});
