// account-chip：chip 文本 · 本会话那一形照会话那台写好的那一份 · 没有会话看本机的默认号。
import { describe, it, expect, vi, beforeEach } from "vitest";

const readRemoteConfigMock = vi.fn();
const fetchAccountsMock = vi.fn();
const invokeMock = vi.fn();
vi.mock("../../../src/frontend/ui/remote-config", () => ({ readRemoteConfig: () => readRemoteConfigMock() }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
// chip 自己不直接调 invoke（只经 `fetchAccounts`）——这个 mock 原先是为
// 用量懒加载那条路装的，那条路整轴退役了；mock 留着是因为下面几条仍要拦住一切 invoke，
// 好断言「chip 不该再发任何用量请求」。
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { chipLabel, AccountChip } from "../../../src/frontend/ui/account-chip";
import type { SessionAccountReply } from "../../../src/frontend/ui/generated/SessionAccountReply";
import type { AccountsState, Account } from "../../../src/frontend/ui/accounts";
// 读面与偏好从 `accounts.ts` 拆出去了（`account-reads.ts` / `account-prefs.ts`），桩打在它们真住的模块上。
import * as readsMod from "../../../src/frontend/ui/account-reads";
// `D4 阻-4`：命令面板那一侧的**生产段**（chip 的快照就是喂给它的）。
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { appStore, putIn } from "../../../src/frontend/ui/app-store";
import { copyText } from "../../../src/frontend/ui/copy-table";

beforeEach(() => {
  vi.restoreAllMocks();
  readRemoteConfigMock.mockReset();
  fetchAccountsMock.mockReset();
  invokeMock.mockReset().mockResolvedValue(undefined);
  // 替身守真身的约定：问回来那一份放进 store（画的那几处只读 store）。
  vi.spyOn(readsMod, "fetchAccounts").mockImplementation(async (origin) => {
    const st = (await fetchAccountsMock(origin)) as AccountsState;
    putIn(appStore.accounts, origin, st);
    return st;
  });
});

function acct(p: Partial<Account>): Account {
  return {
    name: "z",
    email: "z@example.test",
    configDir: "/h/.claude-alt/z",
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: true,
    authKind: "subscription",
    authReady: true,
    selectable: true,
    badge: { text: copyText("accounts.badge.signedIn"), warn: false, title: "" },
    ...p,
  };
}
/** `def` ＝ 那台写好的默认号（`meta.effectiveDefault`，夹具照成品给）。 */
function state(p: Partial<AccountsState>, def: string | null = null): AccountsState {
  return {
    origin: LOCAL,
    available: true,
    oldBackend: false,
    error: null,
    notice: null,
    meta: {
      enabled: true,
      manifestPath: "/a/accounts.json",
      updatedAt: null,
      count: 0,
      error: null,
      effectiveDefault: def,
    },
    accounts: [],
    ...p,
  };
}

/** 会话那台写好的那一份（`session-account`）。 */
function reply(p: Partial<SessionAccountReply>): SessionAccountReply {
  return {
    account: "bob",
    name: "Bob",
    when: "now",
    from: null,
    accountLine: "账号 bob",
    fromLine: null,
    usage: null,
    stale: false,
    hover: [["Bob"]],
    sampled: null,
    differs: false,
    badge: null,
    ...p,
  };
}

describe("本会话那一形照会话那台写好的那一份（`session-account`）", () => {
  beforeEach(() => {
    appStore.sessionAccount.set(new Map());
    appStore.accounts.set(new Map());
  });

  it("★ 当前 tab 是远端 b 的会话 ⇒ chip 画 b 那台说的号 · 换来的那个 ⇄ · 用量那几格；不问第一台远端", () => {
    appStore.sessionAccount.set(
      new Map([
        [
          "s1",
          {
            origin: "b",
            read: reply({
              from: "amy",
              usage: { slot: "5h", window: "5h", value: "63%", reset: "↻19:00", text: "5h 63% ↻19:00", tone: "warn" },
              stale: true,
            }),
          },
        ],
      ]),
    );
    const chip = new AccountChip({});
    chip.setActive({ sid: "s1", origin: "b" });
    const el = chip.element;
    expect(el.dataset.mode).toBe("session");
    expect(el.style.display).toBe("");
    expect(el.textContent).toContain("Bob");
    expect(el.textContent).toContain("5h");
    expect(el.textContent).toContain("63%");
    expect(el.textContent).toContain("↻19:00");
    expect(el.dataset.shade).toBe("warn");
    expect(el.dataset.stale).toBe("true");
    expect(el.querySelector("[class*=acctChipSwap]"), "换过号 ⇒ 名字前一个 ⇄").not.toBeNull();
    expect(readRemoteConfigMock, "chip 不再替人挑「第一台远端」").not.toHaveBeenCalled();
  });

  it("★ b 那台的那一份换了 ⇒ 同一拍重画；别的会话的那一份不动它", () => {
    const chip = new AccountChip({});
    chip.setActive({ sid: "s1", origin: "b" });
    putIn(appStore.sessionAccount, "s2", { origin: "c", read: reply({ name: "Carol" }) });
    expect(chip.element.textContent).not.toContain("Carol");
    putIn(appStore.sessionAccount, "s1", { origin: "b", read: reply({ name: "Dave", from: null }) });
    expect(chip.element.textContent).toContain("Dave");
    expect(chip.element.querySelector("[class*=acctChipSwap]")).toBeNull();
  });

  it("★ 点账号按钮永远开账号面板：没有当前会话 ⇒ 本机那一形（sid null）；有会话但号还没问到 ⇒ 那个会话；都不开下拉", async () => {
    fetchAccountsMock.mockResolvedValue(state({ origin: LOCAL, accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy" })] }, "wei"));
    const togglePanel = vi.fn();
    const chip = new AccountChip({ togglePanel });
    await chip.refresh();
    chip.element.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(togglePanel).toHaveBeenLastCalledWith(null, LOCAL);
    chip.setActive({ sid: "s9", origin: "b" });
    chip.element.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(togglePanel).toHaveBeenLastCalledWith("s9", "b");
    expect(document.querySelectorAll('[role="menu"]').length).toBe(0);
  });

  it("★ 没有会话 ⇒ 看本机那台的默认号", async () => {
    fetchAccountsMock.mockResolvedValue(state({ origin: LOCAL, accounts: [acct({ name: "wei", isDefault: true })] }, "wei"));
    const chip = new AccountChip({});
    await chip.refresh();
    expect(fetchAccountsMock).toHaveBeenCalledWith(LOCAL);
    expect(chip.element.dataset.mode).toBe("default");
    expect(chip.element.textContent).toContain("wei");
  });
});

describe("chipLabel", () => {
  it("无 state → 未连远端", () => {
    expect(chipLabel(null)).toBe(copyText("accountChip.label.noRemote"));
  });
  it("旧 backend → 后端需更新", () => {
    expect(chipLabel(state({ available: false, oldBackend: true, error: "版本过旧" }))).toBe(copyText("accountChip.label.backendOld"));
  });
  // 没问出来 ≠ 要更新。
  it("查询失败 → 账号没查到（不说需更新）", () => {
    expect(chipLabel(state({ available: false, error: "现在够不着那台机器的后端" }))).toBe(copyText("accountChip.label.queryFailed"));
  });
  it("未启用 → 未启用", () => {
    expect(
      chipLabel(state({ meta: { enabled: false, manifestPath: "/a/x", updatedAt: null, count: 0, error: null, effectiveDefault: null } })),
    ).toBe(copyText("accountChip.label.disabled"));
  });
  it("ready → 显示那台清单里的默认号（isDefault）", () => {
    const s = state({ accounts: [acct({ name: "z" }), acct({ name: "b", isDefault: true })] }, "b");
    expect(chipLabel(s)).toBe("b");
  });
});

// ------------------------------------------------------------ F1：chip = 纯全局切换器（去 ⚠k）
// F1 把 chip 收敛成 CCSwitcher 式纯全局切换器：⚠k 不一致计数入口移出 chip（当时走命令面板；
// 批量对齐/命令面板对齐命令随 F09 一并删除，现在两者都不在了）。这里锁住「chip 不再长出
// ⚠k 徽章」——防有人把它加回来。
describe("F1 chip 纯全局切换器（无 ⚠k）", () => {
  it("chip 结构里不含 ⚠k 计数 span（已移出，F09 后批量对齐整体删除）", () => {
    const chip = new AccountChip({});
    expect(chip.element.querySelector(".status-account-mismatch")).toBeNull();
  });
  it("AccountChip 不再暴露 updateMismatchBadge / alignAll", () => {
    const chip = new AccountChip({});
    expect((chip as unknown as Record<string, unknown>).updateMismatchBadge).toBeUndefined();
  });
  it("★ 按钮上没有下拉的口子：点了只开账号面板（「新会话默认」只在那台的账号设置里）", () => {
    const chip = new AccountChip({});
    expect((chip as unknown as Record<string, unknown>).openMenu).toBeUndefined();
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
    fetchAccountsMock.mockResolvedValue(st);
    const chip = new AccountChip({});
    await chip.refresh();
    return chip;
  }

  it("≥2 可选账号 → 显彩色头像", async () => {
    const chip = await mountWith(
      state({ accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy" })] }, "wei"),
    );
    expect(icon(chip).querySelector(".acct-avatar")).not.toBeNull();
  });

  // chip 不再自存一份账号清单：store 里它那台换了一份（本窗口任何一次取回），它同一拍重画，不等自己 refresh。
  it("★ 〔GAP1〕store 里 chip 那台的账号清单换了 ⇒ 同一拍重画（不调 refresh、不再取）", async () => {
    const chip = await mountWith(state({ accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy" })] }, "wei"));
    expect(chip.element.textContent).toContain("wei");
    const fetches = fetchAccountsMock.mock.calls.length;
    putIn(appStore.accounts, LOCAL, state({ accounts: [acct({ name: "wei" }), acct({ name: "amy", isDefault: true })] }, "amy"));
    expect(chip.element.textContent).toContain("amy");
    expect(fetchAccountsMock.mock.calls.length, "重画不该再取一次").toBe(fetches);
  });

  it("只有 1 个可选账号 → 退回账号图标（颜色此时区分不了任何东西）", async () => {
    const chip = await mountWith(state({ accounts: [acct({ name: "wei", isDefault: true })] }, "wei"));
    expect(icon(chip).querySelector(".acct-avatar")).toBeNull();
    expect(icon(chip).textContent, "代码画的 Phosphor，不是字符").toBe("");
    expect(icon(chip).querySelector<SVGElement>("svg")?.dataset.icon).toBe("account");
  });

  it("2 个账号但只有 1 个可选 → 仍休眠（数可选数，不是总数）", async () => {
    const chip = await mountWith(
      state({
        accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy", loggedIn: false, authReady: false, selectable: false })],
      }, "wei"),
    );
    expect(icon(chip).querySelector(".acct-avatar")).toBeNull();
  });
});
// ---------------------------------------------------------------------------
// **这里只该「加」看得见，不该「改」看不见**
// ---------------------------------------------------------------------------
describe("K-H2b D2 阻-7：本机那一档 not-ready 仍然整个隐藏", () => {
  it("★ 没有远端 + 本机也没有 manifest ⇒ chip 隐藏（不许冒出一句远端口吻的假话）", async () => {
    // 本机没启用多账号：`available:true` 但 `meta.enabled:false` ⇒ `deriveUi` 判 not-enabled。
    fetchAccountsMock.mockResolvedValue({
      origin: "<local>",
      available: true,
      error: null,
      meta: {
        enabled: false,
        manifestPath: "",
        updatedAt: null,
        count: 0,
        error: null,
        accountZeroAware: true,
      },
      accounts: [],
      notice: null,
    } as unknown as AccountsState);
    const chip = new AccountChip({});
    await chip.refresh();
    const el = (chip as unknown as { element: HTMLElement }).element;
    expect(
      el.style.display,
      "这一形该**整个隐藏**；放宽 origin 门之后它会显出来，\n" +
        "菜单里还写一句「该远端尚未启用多账号」—— 而那台『远端』根本不存在。",
    ).toBe("none");
  });
});

