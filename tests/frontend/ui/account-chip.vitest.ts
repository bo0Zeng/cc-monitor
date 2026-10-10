// A3 account-chip 纯函数测试（选主远端 / chip 文本）。
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

import {
  pickPrimaryOrigin,
  chipLabel,
  AccountChip,
} from "../../../src/frontend/ui/account-chip";
import type { RemoteHostConfig } from "../../../src/frontend/ui/remote-config";
import type { AccountsState, Account } from "../../../src/frontend/ui/accounts";
// 读面与偏好从 `accounts.ts` 拆出去了（`account-reads.ts` / `account-prefs.ts`），桩打在它们真住的模块上。
import * as readsMod from "../../../src/frontend/ui/account-reads";
import * as opsMod from "../../../src/frontend/ui/account-ops";
// `D4 阻-4`：命令面板那一侧的**生产段**（chip 的快照就是喂给它的）。
import { buildAccountCommands } from "../../../src/frontend/ui/account-commands";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { putAccounts } from "../../../src/frontend/ui/app-store";
import { dispatcher, type OverlayHandle } from "../../../src/frontend/ui/keybindings/registry";
import { copyText } from "../../../src/frontend/ui/copy-table";

beforeEach(() => {
  vi.restoreAllMocks();
  readRemoteConfigMock.mockReset();
  fetchAccountsMock.mockReset();
  invokeMock.mockReset().mockResolvedValue(undefined);
  vi.spyOn(readsMod, "fetchAccounts").mockImplementation(() => fetchAccountsMock());
  // F10 Phase D 审计排障发现：多条既有测试打开 chip 菜单后从不显式关闭（`toggleMenu` 把菜单
  // append 到 `document.body`，不像 tabs.ts 的上下文菜单那样每次开新的前先关旧的）——留下的
  // 陈旧 `.account-picker` 菜单会一直挂在全局 DOM 里，后面用 `document.querySelector(...)`
  // 全局查询的测试可能命中的是上一条测试遗留的菜单而不是本次刚开的（同 `tabs.vitest.ts` 的
  // `.tab-context-menu` 清理惯例，这里补一份）。
  document.querySelectorAll('[role="menu"]').forEach((n) => n.remove());
});

function host(p: Partial<RemoteHostConfig>): RemoteHostConfig {
  return {
    label: "",
    host: "h",
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
    connect: true,
    ...p,
  };
}
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
    origin: "devbox",
    available: true,
    oldBackend: false,
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
      effectiveDefault: def,
    },
    accounts: [],
    ...p,
  };
}

describe("pickPrimaryOrigin", () => {
  // 🔴 `K-R59`：这一组此前有两条断「跳过 daemonless 的主机」/「全 daemonless → null」。
  //    定框 `K35` 把那一档删了 ⇒ **今天一台都不跳**，两条一起下岗。
  it("取第一台", () => {
    expect(pickPrimaryOrigin([host({ label: "a" }), host({ label: "b" })])).toBe("a");
  });
  it("label 空 → 用 host", () => {
    expect(pickPrimaryOrigin([host({ label: "", host: "devbox.local" })])).toBe("devbox.local");
  });
  // 「没有一台有身份的远端」就是本机（上一版回 `null`、调用方再把 `null` 读成本机）。
  it("label 与 host 都空 → 本机（那台远端没有身份）", () => {
    expect(pickPrimaryOrigin([host({ label: "", host: "" })])).toBe(LOCAL_ORIGIN);
  });
  it("空列表 → 本机", () => {
    expect(pickPrimaryOrigin([])).toBe(LOCAL_ORIGIN);
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
      chipLabel(state({ meta: { enabled: false, acctsDir: "/a", manifestPath: "/a/x", updatedAt: null, sharedStore: null, count: 0, error: null, effectiveDefault: null } })),
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
    const chip = new AccountChip({ openSettings: () => {} });
    expect(chip.element.querySelector(".status-account-mismatch")).toBeNull();
  });
  it("AccountChip 不再暴露 updateMismatchBadge / alignAll", () => {
    const chip = new AccountChip({ openSettings: () => {} });
    expect((chip as unknown as Record<string, unknown>).updateMismatchBadge).toBeUndefined();
  });

  it("下拉列出账号 + 点非当前项 → 问那台 `accounts-set-default` 切这台的默认号（DoD 正路）", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "devbox" })] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy" })] }, "wei"),
    );
    const setDef = vi.spyOn(opsMod, "accountsSetDefault").mockResolvedValue({} as never);
    vi.spyOn(readsMod, "invalidateAccountsCache").mockImplementation(() => {});
    let changed = 0;
    const chip = new AccountChip({ openSettings: () => {}, onDefaultChanged: () => (changed += 1) });
    await chip.refresh();
    await chip.openMenu();
    const items = document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]');
    expect(items.length).toBe(2); // 下拉列出两个账号（全局切换器）
    const amy = [...items].find((b) => b.textContent?.includes("amy"))!;
    amy.click();
    // selectDefault 链：accountsSetDefault → invalidateCache → refresh(含两次 async 数据源) → onDefaultChanged。
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(setDef).toHaveBeenCalledWith("devbox", "amy"); // 点非当前项 → 这台（devbox）切到 amy
    expect(changed).toBe(1); // 切完回调 onDefaultChanged（让 main.ts 重算会话归属）
  });
});

// 选单的关法：点外面 · 再点 chip · Esc（Esc 走快捷键的弹层栈，一下只关最上面那一层）。
describe("账号选单：再点 chip 收起；Esc 只关选单", () => {
  const menus = (): number => document.querySelectorAll('[role="menu"]').length;
  const press = (el: Element): void => void el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true }));
  const tap = async (el: HTMLElement): Promise<void> => {
    press(el);
    el.click();
    await new Promise((r) => setTimeout(r, 0));
  };
  async function mounted(): Promise<AccountChip> {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "devbox" })] });
    fetchAccountsMock.mockResolvedValue(state({ accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy" })] }, "wei"));
    const chip = new AccountChip({ openSettings: () => {} });
    document.body.appendChild(chip.element);
    await chip.refresh();
    dispatcher.applyOverrides({});
    dispatcher.start();
    return chip;
  }

  it("开着时再点 chip ⇒ 收起（不是先关又开）；点外面也收起", async () => {
    const chip = await mounted();
    await tap(chip.element);
    expect(menus()).toBe(1);
    await tap(chip.element);
    expect(menus()).toBe(0);
    await tap(chip.element);
    press(document.body);
    expect(menus()).toBe(0);
    chip.element.remove();
  });

  it("Esc 只关选单：下面那层（多选 / 查找）这一下收不到", async () => {
    const chip = await mounted();
    let below = 0;
    const floor: OverlayHandle = { handleEsc: () => void below++ };
    dispatcher.pushOverlay(floor);
    await tap(chip.element);
    const esc = (): boolean => document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }));
    esc();
    expect([menus(), below]).toEqual([0, 0]);
    esc();
    expect(below).toBe(1);
    dispatcher.popOverlay(floor);
    chip.element.remove();
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
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "devbox" })] });
    fetchAccountsMock.mockResolvedValue(st);
    const chip = new AccountChip({ openSettings: () => {} });
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
    putAccounts("devbox", state({ accounts: [acct({ name: "wei" }), acct({ name: "amy", isDefault: true })] }, "amy"));
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

// 〔删用量〕原先这里是「F10 chip 用量摘要：菜单展开懒加载」整组
// （懒加载 · 去抖缓存 · 「刷新用量」按钮 · 原文一屏渲染 · 占位符 …）。
// 用量 ③ 轴（探针）整轴退役 ⇒ **被测对象没了**，不是断言变少了。
// 这里换成一条**翻面**的判据：chip 今天不许再发任何用量请求、也不许再长出那个按钮。
describe("：chip 上的用量面已退役（翻面判据）", () => {
  it("展开菜单不发任何 invoke，且没有「刷新用量」这个动作", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "devbox" })] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "wei", isDefault: true }), acct({ name: "amy" })] }, "wei"),
    );
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    await chip.openMenu();
    // 唯一许发的一问是那台的 API key 两格事实（`apikey-routing`，徽章用）；用量那一族一条都不许有。
    const others = invokeMock.mock.calls.filter(
      (c) => !(c[0] === "chan_call" && (c[1] as { op?: string })?.op === "apikey-routing"),
    );
    expect(others).toEqual([]);
    const actions = [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].map(
      (b) => b.textContent,
    );
    expect(actions).not.toContain(`${copyText("accountChip.menu.refresh")}${copyText("acct.hover.usage")}`);
    // 折叠态那个用量 span 也不该再存在（`.status-account-usage` 整条删了）。
    expect(chip.element.querySelector(".status-account-usage")).toBeNull();
  });
});
// 菜单每一项右边那一格只出自账号清单那一件成品（那台后端写的 `badge`：原地模式 · API key 号经不经中转 · 订阅号登录了没有，
// `observe/accounts_query.rs::badge_of`），界面不按号的类型去挑来源、不再单问 `apikey-routing`。断言落在真渲染出来的 DOM 上：
// 字照 `badge.text`、悬停照 `badge.title`、警示照 `badge.warn`（呈现走 `data-intent`，不拼进字）。
describe("chip 菜单的账号徽章：照抄核心写好的那一枚（DOM 层）", () => {
  async function menuRows(accounts: Account[], defaultName: string): Promise<HTMLButtonElement[]> {
    document.querySelectorAll('[role="menu"]').forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ label: "devbox" })] });
    fetchAccountsMock.mockResolvedValue(state({ accounts: accounts.map((a) => ({ ...a, isDefault: a.name === defaultName })) }));
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    await chip.openMenu();
    const items = [...document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')];
    expect(items.length, "菜单没把账号渲染出来 —— 下面的断言测不到任何东西").toBe(accounts.length);
    return items;
  }
  const rowOf = (items: HTMLButtonElement[], name: string): HTMLButtonElement =>
    items.find((el) => el.querySelector('[data-part="label"]')?.textContent === name)!;
  const detailOf = (row: HTMLButtonElement): HTMLElement => row.querySelector<HTMLElement>('[data-part="detail"]')!;

  it("★ 字 · 悬停 · 警示都照 `badge`：一枚警示的（API key 号没配上）· 一枚不警示的（订阅号已登录）", async () => {
    const kk = acct({ name: "kk", authKind: "api-key", loggedIn: false, authReady: true, badge: { text: "B-kk", warn: true, title: "T-kk" } });
    const wei = acct({ name: "wei", badge: { text: "B-wei", warn: false, title: "" } });
    const items = await menuRows([wei, kk], "wei");
    expect([detailOf(rowOf(items, "kk")).textContent, rowOf(items, "kk").title, detailOf(rowOf(items, "kk")).dataset.intent]).toEqual(["B-kk", "T-kk", "warn"]);
    expect([detailOf(rowOf(items, "wei")).textContent, rowOf(items, "wei").title, detailOf(rowOf(items, "wei")).dataset.intent]).toEqual(["B-wei", "", undefined]);
  });
});

// ---------------------------------------------------------------------------
// `K-H2b` `D2 阻-7`：**本件只该「加」看得见，不该「改」看不见**
// ---------------------------------------------------------------------------
describe("K-H2b D2 阻-7：本机那一档 not-ready 仍然整个隐藏", () => {
  it("★ 没有远端 + 本机也没有 manifest ⇒ chip 隐藏（不许冒出一句远端口吻的假话）", async () => {
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    // 本机没启用多账号：`available:true` 但 `meta.enabled:false` ⇒ `deriveUi` 判 not-enabled。
    vi.spyOn(readsMod, "fetchLocalAccounts").mockResolvedValue({
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
    document.querySelectorAll('[role="menu"]').forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    vi.spyOn(readsMod, "fetchLocalAccounts").mockResolvedValue(
      state({ accounts: [acct({ name: "acct-a", isDefault: true, configDir: "/h/.claude-alt/acct-a" })] }, "acct-a"),
    );
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    await chip.openMenu();
    const labels = [...document.querySelectorAll('[role="menu"] *')].map((e) => e.textContent);
    // 非空对照：菜单确实渲染出来了（否则下面那条 not.toContain 是空真）。
    expect(labels).toContain(copyText("accountChip.menu.manage"));
    // 正题：那个按钮在本机那一档是死的（`loadCurrentAccountUsage` 首行就 return）。
    expect(labels).not.toContain(`${copyText("accountChip.menu.refresh")}${copyText("acct.hover.usage")}`);
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
    document.querySelectorAll('[role="menu"]').forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    vi.spyOn(readsMod, "fetchLocalAccounts").mockResolvedValue(
      state({ accounts: accounts.map((a) => ({ ...a, isDefault: a.name === defaultName })) }),
    );
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    return chip;
  }
  /** chip 菜单里**能点选**的那几个号（`disabled` 的那几行不算 —— 它们点了也不切）。 */
  async function pickableInMenu(chip: AccountChip): Promise<string[]> {
    await chip.openMenu();
    return [...document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')]
      .filter((el) => !el.disabled)
      .map((el) => el.querySelector('[data-part="label"]')?.textContent ?? "");
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
    const A = acct({ name: "acct-a", configDir: "/h/.claude-alt/acct-a" });
    const B = acct({ name: "acct-b", configDir: "/h/.claude-alt/acct-b" });
    // 阴性侧就在同一趟里：`exists:false` 那个号两边都不该出现。
    const gone = acct({ name: "acct-gone", configDir: "/h/.claude-alt/acct-gone", exists: false, selectable: false });
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
    document.querySelectorAll('[role="menu"]').forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: true, hosts: [host({ host: "hostA" })] });
    fetchAccountsMock.mockResolvedValue(
      state({ accounts: [acct({ name: "z", isDefault: true, configDir: "/h/.claude-alt/z" })] }, "z"),
    );
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    expect(pickableInCommandBar(chip)).toEqual(["z"]);
    expect(await pickableInMenu(chip)).toEqual(["z"]);
  });

  it("★ `D2 阻-7` 不许被这一格顺手放宽：本机也没有 manifest ⇒ 两边都空", async () => {
    document.querySelectorAll('[role="menu"]').forEach((el) => el.remove());
    readRemoteConfigMock.mockResolvedValue({ enabled: false, hosts: [] });
    // `meta.enabled:false` ⇒ `deriveUi` 判 not-enabled ⇒ `refresh` 把 state 清回 null 并隐藏。
    vi.spyOn(readsMod, "fetchLocalAccounts").mockResolvedValue({
      ...state({ accounts: []}),
      meta: { enabled: false, acctsDir: "", manifestPath: "", updatedAt: null, sharedStore: null, count: 0, error: null },
    } as unknown as AccountsState);
    const chip = new AccountChip({ openSettings: () => {} });
    await chip.refresh();
    expect(
      chip.snapshotReady(),
      "什么都没有的时候命令面板冒出了一份快照 —— 放宽那道门时把 `D2 阻-7` 一起放宽了",
    ).toBeNull();
    expect(pickableInCommandBar(chip)).toEqual([]);
  });
});
