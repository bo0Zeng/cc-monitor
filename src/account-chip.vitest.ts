// A3 account-chip 纯函数测试（选主远端 / chip 文本）。
import { readFileSync } from "node:fs";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

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
// `D4 阻-4`：命令面板那一侧的**生产段**（chip 的快照就是喂给它的）。
import { buildAccountCommands } from "./account-commands";
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
    resumeCommand: "",
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
    notice: null,
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

// ---------------------------------------------------------------------------
// K-A1（第二轮）`Y2`/`Y3`：chip 菜单那一列状态的取值**只**来自
// `accounts.ts::accountStatusBadge` —— 而且断言落在**真渲染出来的 DOM 节点**上。
//
// 为什么必须有 DOM 这一层（第一轮的病理，逐字）：第一轮把三态收进 `accountStatusBadge`
// 并只把**设置面板**那侧接了过去，纯函数那一族当场全绿 —— 而 `account-chip.ts` 这一处
// 根本没接上，用户从状态栏 chip 的账号菜单看到的仍是「已登录」。
// ⇒ 「纯函数全绿而 DOM 没接上」是本仓真发生过的失效模式，所以这一族**不**断言
// `accountStatusBadge(a)` 的返回值（那是上一轮已经绿了的东西，在这一侧等于没测），
// 只断言 `.account-picker-status` 的 `textContent` 与那一行 `<button>` 的 `title`。
//
// ★ 第三轮（PM 裁定「警示走 CSS，不进文本」）在同一族里多钉一维：**`warn` 那个类**。
// 约定与设置那侧逐字同一套 —— 语义住布尔（`accountStatusBadge` 的 `warn`）、呈现住 CSS
// （`.accounts-row-badge.warn` / 本轮新加的 `.account-picker-status.warn`）。
// ⇒ 每一格都断 `classList.contains("warn")`，含一格**阴性对照**（已登录不许上警示色）。
// ---------------------------------------------------------------------------
describe("K-A1（第二轮）chip 菜单的账号状态（DOM 层）", () => {
  async function menuRows(accounts: Account[], defaultName: string): Promise<HTMLButtonElement[]> {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "aya" })] });
    fetchAccountsMock.mockResolvedValue(state({ accounts, defaultName }));
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    await chip.openMenu();
    const items = [...document.querySelectorAll<HTMLButtonElement>(".account-picker-item")];
    // 地板：菜单没渲染出来 ⇒ 下面每一条都会在空集合上「找不到行」而崩，不会静静地绿。
    expect(items.length, "菜单没把账号渲染出来 —— 下面的断言测不到任何东西").toBe(accounts.length);
    return items;
  }
  const rowOf = (items: HTMLButtonElement[], name: string): HTMLButtonElement =>
    items.find((el) => el.querySelector(".account-picker-name")?.textContent === name)!;
  const statusElOf = (row: HTMLButtonElement): HTMLElement =>
    row.querySelector<HTMLElement>(".account-picker-status")!;
  const statusOf = (row: HTMLButtonElement): string => statusElOf(row).textContent ?? "";
  /** 「这一格有没有拿到警示呈现」—— 第三轮裁定之后，⚠ 住这儿，不住 `textContent`。 */
  const warnOf = (row: HTMLButtonElement): boolean => statusElOf(row).classList.contains("warn");

  it("★ Y2：api-key 号（缺订阅凭据）在菜单里写「api-key（未配置端点）」——不是「已登录」，也不是「未登录 ⚠」", async () => {
    // `authReady: true` 不是我编的：`acct_core::auth_ready` 对 api-key 那一支逐字 `=> true`
    //（`src-tauri/crates/acct-core/src/lib.rs:95-100`），线上 daemon 出的就是这个形。
    const kk = acct({ name: "kk", authKind: "api-key", loggedIn: false, authReady: true });
    const items = await menuRows([acct({ name: "wei" }), kk], "wei");
    const row = rowOf(items, "kk");
    expect(statusOf(row)).toBe("api-key（未配置端点）");
    // 三条阴性：替换**前**这一行会写「未登录 ⚠」（因为 `loggedIn: false`），而 KA6a 点名的
    // 坏体验是「已登录」那一档。两句都不许再出现在这个格子里。
    expect(statusOf(row)).not.toBe("已登录");
    expect(statusOf(row)).not.toBe("未登录 ⚠");
    expect(statusOf(row)).not.toBe("未登录");
    // title 是 `accountStatusBadge` 给的那一句：等号防漂 + 一句字面量防「两边一起坏」的循环自证。
    // ⚠〔`K-H2b` `D1 阻-5` 08-28〕chip 现在**明说这一行属于哪一半**：`menuRows` 造的是
    //    远端那一档（`readRemoteConfig` 给了一台 host）⇒ 这里要拿同样的 scope 去比，
    //    否则「等号防漂」比的是两句不同的话。
    expect(row.title).toBe(accountsMod.accountStatusBadge(kk, { scope: "remote" }).title);
    expect(row.title).toContain("请求会在 claude 那边报鉴权失败");
    // ★ 第三轮：这一格**本来就该是警示态**（选得中却连不上），警示由 `.warn` 类呈现 ——
    // 文本里一个字形都不拼，所以上面那三条 `not.toBe` 与这一条并不打架。
    expect(warnOf(row), "api-key 那格没拿到 warn 类 ⇒ 用户看到的是一句普通灰字").toBe(true);
  });

  it("Y3 阴性对照①：in-place ⇒「逃生口」", async () => {
    const esc = acct({ name: "esc", mode: "in-place" });
    const items = await menuRows([esc, acct({ name: "wei" })], "wei");
    const row = rowOf(items, "esc");
    expect(statusOf(row)).toBe("逃生口");
    expect(row.title).toBe(accountsMod.accountStatusBadge(esc, { scope: "remote" }).title);
    expect(row.title).toContain("in-place 模式");
    // 这一格断的是 `accountStatusBadge` **实际给的** `warn` 值 —— 实读 `src/accounts.ts:185-191`：
    // in-place 那一支逐字 `warn: true`（它「选得中但不支持按会话切号」，同样是警示态）。
    expect(accountsMod.accountStatusBadge(esc).warn, "取值源变了就该在这儿先红").toBe(true);
    expect(warnOf(row)).toBe(true);
  });

  it("Y3 阴性对照②：订阅号缺凭据 ⇒ 仍是「未登录」那一档（没被这次改动一起放宽）", async () => {
    const old = acct({ name: "old", loggedIn: false });
    const items = await menuRows([old, acct({ name: "wei" })], "wei");
    const row = rowOf(items, "old");
    expect(statusOf(row)).toBe("未登录");
    expect(statusOf(row)).not.toBe("已登录");
    // 这一句 title 与替换前**逐字相同**（见下面那条「真发现」里贴的替换前三句）。
    expect(row.title).toBe("该账号尚未登录——请在终端里用它 /login");
    expect(row.title).toBe(accountsMod.accountStatusBadge(old, { scope: "remote" }).title);
    // ★ 第三轮：第二轮丢掉的那个 ⚠ 就补在这儿 —— 不是拼回文本，是拿到 `.warn` 类。
    expect(warnOf(row), "订阅号缺凭据那格没拿到 warn 类 ⇒ 「未登录」丢了警示呈现").toBe(true);
  });

  it("Y3 阴性对照③：订阅号有凭据 ⇒「已登录」，且这一行 title 仍是空串（与替换前逐字相同）", async () => {
    const wei = acct({ name: "wei", loggedIn: true });
    const items = await menuRows([wei], "wei");
    const row = rowOf(items, "wei");
    expect(statusOf(row)).toBe("已登录");
    // 替换前这一支**不设** `row.title`（读作空串）；`accountStatusBadge` 给的 title 是 ""
    // ⇒ 读数逐字相同（差别只在 DOM 上多了个空的 `title` 属性，用户看不见）。
    expect(row.title).toBe("");
    expect(row.title).toBe(accountsMod.accountStatusBadge(wei, { scope: "remote" }).title);
    // ★ 第三轮的**阴性对照**：健康态一格不上色（`.accounts-row-badge` 那条注释逐字的道理 ——
    // 恒真的信息不携带信息，涂它只会稀释真正要跳出来的那几档）。无条件加类会在这儿红。
    expect(warnOf(row), "已登录不该上警示色 —— 那说明 warn 被无条件加上了").toBe(false);
  });

  it("★ Y3 真发现：三格里**两格文案与替换前不逐字相同** —— 记成机检，交 PM 裁，不自批", async () => {
    // 派工单 Y3 要求「`row.title` 与替换前逐字相同」。替换前那三句逐字取自
    // `git show HEAD:src/account-chip.ts`（本轮之前那一版 `accountRow`，即第二个 commit）：
    //   in-place     : text「逃生口」   · title「in-place 模式：不支持按会话切号」
    //   订阅缺凭据   : text「未登录 ⚠」 · title「该账号尚未登录——请在终端里用它 /login」
    //   订阅有凭据   : text「已登录」   · title 不设（读作空串）
    // 实得：title 三句里**一句不同**（in-place），text 三句里**一句不同**（缺凭据那句少了 ⚠）。
    // 两处都不是笔误，是「收敛到同一取值源」的必然结果，逐条给因：
    //   ① ⚠ 拼不回去：`accountStatusBadge` 把「要不要警示」表达成 `warn: true` 这个布尔而
    //      不是字形，设置那侧靠 `.accounts-row-badge.warn` 的 CSS 上色；本菜单的
    //      `.account-picker-status` 没有 `.warn` 规则（`src/styles.css:6106-6110`），
    //      而 `styles.css` 不在本轮写区。★ 更硬的一条：把 ⚠ 拼进 `text` 会让 api-key 那一支
    //      变成「api-key（未配置端点） ⚠」—— 与本轮 Y2 逐字冲突。**Y2 与 Y3 在这一格上互斥**，
    //      单一取值源下无解，只能由 PM 裁（要 ⚠ 就得改 Y2 或给 chip 补一条 `.warn` CSS）。
    //   ② in-place 的 title 换成了同义但更明确的一句（多了「cc-monitor」与「对它」）。
    // 本条把这两处钉成机检：PM 若裁定恢复旧文案，这条会红 —— 改它是一个显式动作，不是漂移。
    //
    // ★★ **第三轮：这条判据的含义转义了，判据本身一字不动。**
    // 从「记录一处 delta、待 PM 裁」变成「**钉住『警示不回文本里去』这条裁定**」——
    // PM 已裁走 CSS 那条路（`.account-picker-status.warn`，本轮新加），⇒ 谁再把 ⚠ 拼进
    // `text`（比如为了让 chip「看起来像旧版」），下面 `not.toBe("未登录 ⚠")` 当场红。
    // 那不再是 delta 登记，是一条**反悔要显式**的护栏。同理第二句钉住 in-place 那句 title
    // 不许悄悄退回旧文案（要退就得改这一行，那是显式动作）。
    const esc = acct({ name: "esc", mode: "in-place" });
    const old = acct({ name: "old", loggedIn: false });
    const items = await menuRows([esc, old], "old");
    // Δ① text：⚠ 没了
    expect(statusOf(rowOf(items, "old"))).not.toBe("未登录 ⚠");
    expect(statusOf(rowOf(items, "old"))).toBe("未登录");
    // Δ② title：in-place 那句换了
    expect(rowOf(items, "esc").title).not.toBe("in-place 模式：不支持按会话切号");
    expect(rowOf(items, "esc").title).toBe("in-place 模式：cc-monitor 不支持对它按会话切号");
  });

  // ★ 第三轮补的地板：上面那 4 条 warn 断言测的是**类加没加**，jsdom 不加载 `styles.css`
  // ⇒ 把 `.account-picker-status.warn` 那条规则删掉，它们**一条都不会红**，而用户看到的
  // 又是一句普通灰字（= 第二轮那个 bug 原样回来）。这正是本仓已有范式治的那个形：
  // `topbar-icons.vitest.ts` 逐字写着「改名让 styles.css 那条规则失去了宿主」。
  it("★ 第三轮：`.warn` 那个类在 styles.css 里真有宿主（否则语义在、呈现没了）", () => {
    // ⚠ 匹配单位是**一整行选择器**，不是子串 —— `scanning-guard-registry.vitest.ts` 那条
    // 递减棘轮逐字说了为什么（子串比事实小 ⇒ 把事实撑大的改动从缝里溜过去而判据照样绿：
    // 比如有人写 `.account-picker-status.warn .x { … }`，子串照样命中而那格根本没上色）。
    const cssLines = readFileSync(`${REPO_ROOT}/src/styles.css`, "utf8")
      .split("\n")
      .map((l) => l.trim());
    // 抽取器自检：先确认这把尺子够得着这个文件（不然下面两条是空真）。
    expect(cssLines.length, "读到的 styles.css 只有几行 —— 尺子坏了").toBeGreaterThan(1000);
    expect(cssLines, "读到的不是 styles.css —— 连基准那条规则都没有").toContain(
      ".account-picker-status {",
    );
    expect(cssLines, "`.account-picker-status.warn` 没有 CSS 宿主 ⇒ chip 那格的警示态与正常态长得一模一样").toContain(
      ".account-picker-status.warn {",
    );
    // 同职第二处也一起钉住（设置那张表），免得「治了这一处、没治所有同职的地方」。
    expect(cssLines, "设置那张账号表的 `.accounts-row-badge.warn` 宿主没了 —— 同一套约定的另一半").toContain(
      ".accounts-row-badge.warn {",
    );
  });
});

// ---------------------------------------------------------------------------
// `K-H2b` `D1 阻-4` / `阻-5`：**没有远端时，chip 渲染的是本机账号，而且带中转三态**
// ---------------------------------------------------------------------------
//
// ★★ 它治的是两条**同源**的病：
// ① `阻-4`：本文件此前两处注释写着「远端全关掉时这里渲染的就是本机账号」——**是假的**。
//    `fetchAccounts` 只问 `list_remote_accounts`，`origin` 为 `null` 时 `refresh` 整个隐藏。
// ② `阻-5`：于是 `KH2B7` 那三态**在用户看得见的地方一处都没落地**
//    （`fetchLocalRelayRouting` / `localRelayStateFor` 生产调用方各 0）。
// ⇒ 本组既钉「本机那几行真的渲染出来了」，也钉「三态真的分得开」。
describe("K-H2b D1 阻-5：没有远端时 chip 渲染本机账号，徽章带中转三态", () => {
  /** 起一个「没有远端」的 chip，本机账号由 `fetchLocalAccounts` 给、routing 由那条命令给。 */
  async function localMenuRows(
    accounts: Account[],
    routing: { routed: string[]; running: boolean } | "fail",
  ): Promise<HTMLButtonElement[]> {
    // ⚠ 本组同一条测试里会开三次菜单，而 `toggleMenu` 把菜单 append 到 `document.body`、
    //    **不会先关旧的**（既有测试每条只开一次，所以从没撞上）⇒ 每次先扫干净，
    //    否则数出来的是三次的**累加**（本轮实测：应为 2，实得 4）。
    document.querySelectorAll(".account-picker").forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    vi.spyOn(accountsMod, "fetchLocalAccounts").mockResolvedValue(
      state({ accounts, defaultName: accounts[0]?.name ?? "" }),
    );
    const spy = vi.spyOn(accountsMod, "fetchLocalRelayRouting");
    if (routing === "fail") spy.mockRejectedValue(new Error("问不到"));
    else spy.mockResolvedValue(routing);
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    await chip.openMenu();
    const items = [...document.querySelectorAll<HTMLButtonElement>(".account-picker-item")];
    expect(items.length, "没有远端时 chip 一行都没渲染 —— 那正是 `阻-4` 那句假话的真实形状").toBe(
      accounts.length,
    );
    return items;
  }
  const rowOf = (items: HTMLButtonElement[], name: string): HTMLButtonElement =>
    items.find((el) => el.querySelector(".account-picker-name")?.textContent === name)!;
  const statusOf = (row: HTMLButtonElement): string =>
    row.querySelector<HTMLElement>(".account-picker-status")!.textContent ?? "";

  const apiKey = (name: string, dir: string) =>
    acct({ name, configDir: dir, authKind: "api-key", loggedIn: false, authReady: true });

  it("★★ 三态在 DOM 上真的分得开（表里有行 + 中转在跑 / 表里有行 + 没跑 / 表里没行）", async () => {
    const A = apiKey("acct-a", "/h/.claude-accts/acct-a");
    const B = apiKey("acct-b", "/h/.claude-accts/acct-b");
    // ① 有行 + 在跑 ⇒ 「经本机中转」；同一趟里 B 没行 ⇒ 「未配置端点」（非空对照就在同一趟）。
    let items = await localMenuRows([A, B], { routed: ["/h/.claude-accts/acct-a"], running: true });
    expect(statusOf(rowOf(items, "acct-a"))).toBe("api-key（经本机中转）");
    expect(statusOf(rowOf(items, "acct-b"))).toBe("api-key（未配置端点）");
    // ② 只把「中转在不在跑」翻过来 ⇒ 第三档。
    items = await localMenuRows([A, B], { routed: ["/h/.claude-accts/acct-a"], running: false });
    expect(statusOf(rowOf(items, "acct-a"))).toBe("api-key（中转未运行）");
    // ③ 问不到 routing ⇒ **不表态**，回落到缺席那一档（只说条件、不下判断）。
    items = await localMenuRows([A, B], "fail");
    expect(statusOf(rowOf(items, "acct-a"))).toBe("api-key（未配置端点）");
    expect(rowOf(items, "acct-a").title).toContain("不替它下判断");
  });

  it("★ 本机那一趟问的是本机那条路（不是 `list_remote_accounts`）", async () => {
    const A = apiKey("acct-a", "/h/.claude-accts/acct-a");
    const localSpy = vi.spyOn(accountsMod, "fetchLocalAccounts");
    await localMenuRows([A], { routed: [], running: false });
    expect(localSpy).toHaveBeenCalled();
    // 阴性对照：远端那条一次都没被问 —— 否则「渲染的是本机账号」这句话又成了假的。
    expect(fetchAccountsMock).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// `K-H2b` `D2 阻-7`：**本件只该「加」看得见，不该「改」看不见**
// ---------------------------------------------------------------------------
describe("K-H2b D2 阻-7：本机那一档 not-ready 仍然整个隐藏", () => {
  it("★ 没有远端 + 本机也没有 manifest ⇒ chip 隐藏（不许冒出一句远端口吻的假话）", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    // 本机没启用多账号：`available:true` 但 `meta.enabled:false` ⇒ `deriveUi` 判 not-enabled。
    vi.spyOn(accountsMod, "fetchLocalAccounts").mockResolvedValue({
      origin: "<local>",
      available: true,
      error: null,
      meta: {
        enabled: false,
        acctsDir: "",
        manifestPath: "",
        updatedAt: null,
        sharedStore: null,
        count: 0,
        error: null,
        accountZeroAware: true,
      },
      accounts: [],
      defaultName: null,
      notice: null,
    } as unknown as AccountsState);
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    const el = (chip as unknown as { element: HTMLElement }).element;
    expect(
      el.style.display,
      "本件之前这一形是**整个隐藏**；放宽 origin 门之后它会显出来，\n" +
        "菜单里还写一句「该远端尚未启用多账号」—— 而那台『远端』根本不存在。",
    ).toBe("none");
  });

  it("★ 本机那一档不渲染「刷新用量」那个静默死按钮", async () => {
    document.querySelectorAll(".account-picker").forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    vi.spyOn(accountsMod, "fetchLocalAccounts").mockResolvedValue(
      state({ accounts: [acct({ name: "acct-a", configDir: "/h/.claude-accts/acct-a" })], defaultName: "acct-a" }),
    );
    vi.spyOn(accountsMod, "fetchLocalRelayRouting").mockResolvedValue({ routed: [], running: false });
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    await chip.openMenu();
    const labels = [...document.querySelectorAll(".account-picker *")].map((e) => e.textContent);
    // 非空对照：菜单确实渲染出来了（否则下面那条 not.toContain 是空真）。
    expect(labels).toContain("管理账号…");
    // 正题：那个按钮在本机那一档是死的（`loadCurrentAccountUsage` 首行就 return）。
    expect(labels).not.toContain("刷新用量");
  });
});

// ---------------------------------------------------------------------------
// `K-H2b` `D4 阻-4`：**chip 菜单与 Ctrl+K 命令面板必须同源**
// ---------------------------------------------------------------------------
//
// ★★ 它治的是什么（`D4` 现打，PM 裁四认账：这一格是 PM 合并两路审计时漏抄的）：
// `D2 阻-6` 的事实段里列了三条副作用，`§0m` 只抄了两条。漏掉的那条是 ——
// `toggleMenu` 那道门 `D1 阻-5` 放宽成「有远端**或**本机那一档」，而 `snapshotReady()`
// 留在 `!this.origin` 那道旧门后面 ⇒ **本机 ready 那一档：chip 显示、菜单里能切号，
// 而 Ctrl+K 命令面板拿到 `null`。** 两处对同一件事给两个答案，**而且静默**。
// ⇒ 正是 `KA6a` 第一轮栽过的「同职两处不同源」。
//
// ⚠ 本组量的是**行为**：走真的 `AccountChip.refresh()` + `openMenu()` 拿 DOM，
//    走真的 `snapshotReady()` 喂真的 `buildAccountCommands`（命令面板那一侧的生产段），
//    然后把两边**能点选的那几个号**对拍。不读源码文本。
//
// ⚠ 本组**买不到**：`main.ts` 那一行是不是真的把 `snapshotReady()` 喂给了
//    `buildAccountCommands`（`main.ts` 不在本件写区，也没有 DOM 判据够得着它）。
//    今天靠的是一个**读数**（08-28 现打，分母 = `git ls-files -z | xargs -0 grep -n snapshotReady`）：
//    `account-chip.ts` 之外的生产消费方**恰好 1 处**，就是 `main.ts:558`
//    （chip 内部还有一处 `applyDefaultByName`，那是它自己的事）。**读数不是判据。**
describe("K-H2b D4 阻-4：chip 能列出来的号，命令面板也能列出来", () => {
  /** 起一个「没有远端」的 chip，本机账号由 `fetchLocalAccounts` 给。 */
  async function localChip(accounts: Account[], defaultName: string | null): Promise<AccountChip> {
    document.querySelectorAll(".account-picker").forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    vi.spyOn(accountsMod, "fetchLocalAccounts").mockResolvedValue(state({ accounts, defaultName }));
    vi.spyOn(accountsMod, "fetchLocalRelayRouting").mockResolvedValue({ routed: [], running: false });
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    return chip;
  }
  /** chip 菜单里**能点选**的那几个号（`disabled` 的那几行不算 —— 它们点了也不切）。 */
  async function pickableInMenu(chip: AccountChip): Promise<string[]> {
    await chip.openMenu();
    return [...document.querySelectorAll<HTMLButtonElement>(".account-picker-item")]
      .filter((el) => !el.classList.contains("disabled"))
      .map((el) => el.querySelector(".account-picker-name")?.textContent ?? "");
  }
  /** 命令面板那一侧**真的**产出的那几个号（走生产段 `buildAccountCommands`）。 */
  function pickableInCommandBar(chip: AccountChip): string[] {
    return buildAccountCommands({
      snapshot: chip.snapshotReady(),
      chordHint: () => undefined,
      setCurrent: () => {},
      openSettings: () => {},
    })
      .filter((c) => c.id.startsWith("acct-default-"))
      .map((c) => c.id.slice("acct-default-".length));
  }

  it("★★ 本机那一档：两边列出**同一组**号（先前 chip 有两行、命令面板一条都没有）", async () => {
    const A = acct({ name: "acct-a", configDir: "/h/.claude-accts/acct-a" });
    const B = acct({ name: "acct-b", configDir: "/h/.claude-accts/acct-b" });
    // 阴性侧就在同一趟里：`exists:false` 那个号两边都不该出现。
    const gone = acct({ name: "acct-gone", configDir: "/h/.claude-accts/acct-gone", exists: false });
    const chip = await localChip([A, B, gone], "acct-b");
    const menu = await pickableInMenu(chip);
    const bar = pickableInCommandBar(chip);
    // 反空真：两边都真的列出了东西（否则下面那条相等是 `[] == []` 的空真）。
    expect(menu, "chip 菜单里一个能点的号都没有 —— 下面那条相等会是空真").toEqual(["acct-a", "acct-b"]);
    expect(
      bar,
      "命令面板那一侧一条账号命令都没有 —— 这正是 `D4 阻-4` 那个洞的形状：\n" +
        "chip 显示、菜单里能切号，而 Ctrl+K 拿到 `null`（两处对同一件事给两个答案，且静默）",
    ).toEqual(menu);
    // 阴性对照：不可选的那个号两边都没有（证明这把尺子不是「把所有号原样倒出来」）。
    expect(menu).not.toContain("acct-gone");
    expect(bar).not.toContain("acct-gone");
  });

  it("★ 远端那一档照旧（放宽只加了本机那一半，没有改远端那一半）", async () => {
    document.querySelectorAll(".account-picker").forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ host: "hostA" })] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z", configDir: "/h/.claude-accts/z" })], defaultName: "z" }),
    );
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    expect(pickableInCommandBar(chip)).toEqual(["z"]);
    expect(await pickableInMenu(chip)).toEqual(["z"]);
  });

  it("★ `D2 阻-7` 不许被这一格顺手放宽：本机也没有 manifest ⇒ 两边都空", async () => {
    document.querySelectorAll(".account-picker").forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    // `meta.enabled:false` ⇒ `deriveUi` 判 not-enabled ⇒ `refresh` 把 state 清回 null 并隐藏。
    vi.spyOn(accountsMod, "fetchLocalAccounts").mockResolvedValue({
      ...state({ accounts: [], defaultName: null }),
      meta: { enabled: false, acctsDir: "", manifestPath: "", updatedAt: null, sharedStore: null, count: 0, error: null },
    } as unknown as AccountsState);
    vi.spyOn(accountsMod, "fetchLocalRelayRouting").mockResolvedValue({ routed: [], running: false });
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    expect(
      chip.snapshotReady(),
      "什么都没有的时候命令面板冒出了一份快照 —— 放宽那道门时把 `D2 阻-7` 一起放宽了",
    ).toBeNull();
    expect(pickableInCommandBar(chip)).toEqual([]);
  });
});
