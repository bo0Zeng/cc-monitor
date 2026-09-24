/**
 * 🔴 `设计/70 §8` 判据 **#3「非落地页零 I/O」**（第一刀 · 步 2 的主锚）。
 *
 * # 它钉的是什么
 *
 * `70 §1.2` 的成因链第 ③ 层 ＋ `§10.4` 那一行逐字：判据 #3 今天正是被
 * **足迹 · 数据位置 · 日志** 这三块、**外加 `drift-ledger`** 打破的 ——
 * 合计 **4 发** IPC（足迹 1 · 数据位置 1 · **日志 2**）＋ 漂移记账 1 发，
 * 全部在 `buildBody` 时就打出去，而落地页是 `machines`。
 *
 * ⚠ 折叠**不省**这一发：`defaultCollapsed: true` 的折叠是纯 CSS（`0fr ↔ 1fr`），
 * 孩子照常构造 —— 所以这条判据不能靠「它在折叠组里」蒙混。
 *
 * # 为什么绿来自**相等**，不是「计数 <= N」
 *
 * 本仓纪律：**地板在「变少」方向是瞎的，不许当主锚**。
 * ⇒ 这里两边都是**集合相等**：
 *   ① 落地页那一趟打出去的命令集合 == 登记的「落地页该打的那几条」（非空）；
 *   ② 点进「应用」之后**新增**的那一批 == 登记的「应用页那三发」（逐字、非空）。
 * ①②任一侧被清空、或有人把某一块偷偷搬回构造期，两个集合当场分叉。
 *
 * # 〔射程〕它盖得到什么、盖不到什么
 *
 * - **盖得到**：走 `src/ipc/commands` 那个包装层的每一次 IPC（全仓唯一的 `invoke` 包装层）。
 * - **盖不到**：① 绕过包装层直呼 `invoke("...")` 的调用点（本仓有过这种形态，
 *   `installface` 那格的裁词里也写着同一条边界）；② 那几块 per-machine 分节
 *   （账号 / 终端集成 / MCP / 插件 / cc-bus 钩子）—— 本文件把它们 stub 掉了。
 *   〔ST1 · 09-24〕那笔「它们构造期也发 I/O」的债已还：它们改成机器子页第一次可见时才放，
 *   判据住 `panel-per-machine-deferred-io.vitest.ts`（不 stub、两层录音机）；③ 真实排版（jsdom 没有排版引擎）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { ipc } = vi.hoisted(() => ({ ipc: { calls: [] as string[] } }));

// 把**唯一的 IPC 包装层**换成录音机。每一条命令名原样记下来，一律 reject ——
// 各块内部都有自己的 catch（真实失败路径），这样还能顺带证明「一块读不到不拖垮别人」。
vi.mock("../../src/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => () => {
        ipc.calls.push(name);
        return Promise.reject(new Error(`[录音机] ${name} 没有真后端`));
      },
    },
  ),
}));

// —— 与本条无关的重块 stub 掉（理由见头注〔射程〕②）——
vi.mock("../../src/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
    constructor(opts?: {
      pages?: { addMachinePage: (id: string, t: string, el: HTMLElement) => void };
    }) {
      setTimeout(() => {
        opts?.pages?.addMachinePage(
          "machine:（本机）",
          "本机",
          document.createElement("div"),
        );
      }, 0);
    }
  },
}));
const stubEl = () => ({ element: document.createElement("div") });
vi.mock("../../src/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/cc-bus-hooks-section", () => ({
  CcBusHooksSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/settings/cc_integration", () => ({
  CcIntegrationSection: class { element = document.createElement("div"); },
}));
vi.mock("../../src/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: vi.fn(),
    popOverlay: vi.fn(),
    startRecording: vi.fn(),
    cancelRecording: vi.fn(),
    exportOverrides: vi.fn().mockReturnValue({}),
    applyOverrides: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));
void stubEl;

import { SettingsPanel } from "../../src/settings/panel";
import { __setHostOsForTests } from "../../src/settings/host-os";
import { __resetMachineContextForTests } from "../../src/settings/machine-context";

/**
 * 登记表 ①：**落地页（机器）**这一趟该打的命令。
 * 只有 `BackendSection` 那一块 —— 它住落地页，打开就该是新读数（状态是运行期的东西）。
 */
const LANDING_IPC = ["load_config", "backend_machines", "backend_status"] as const;

/**
 * 登记表 ②：点进「应用」才该出现的那几发。
 *
 * 前三条是 `70 §10.4` 逐字点名的那三块（日志 2 · 数据位置 1）；
 * 后两条是同一页上「按账号生成命令」那一块的（`local_ccm_entry_status` ＋
 * 一次 `write_account_aliases` 的 `dryRun` 预览）—— 它们原先也在构造期就打出去，
 * 同属判据 #3 要治的那一族，所以一起推后。
 */
const APP_PAGE_IPC = [
  "get_diagnostics_config",
  "get_log_file_info",
  "get_data_paths",
  "local_ccm_entry_status",
  "write_account_aliases",
] as const;

/**
 * 登记表 ②b：**重开一次设置**之后再点进「应用」，该重来的是哪几发。
 *
 * 只有那三块 —— 「按账号生成命令」那两发**不重来**，因为那一块是「建一次 DOM」，
 * 重开时重建等于在页面上多出第二份。⚠ 代价如实写：它显示的是**第一次点进来时**
 * 的读数，重开设置不会刷新它。那是本件**没有**动的一格（`70 §6 #2`「保存模型
 * 统一成全即时」那条也还没做），不是这条判据放水。
 */
const APP_PAGE_IPC_ON_REOPEN = [
  "get_diagnostics_config",
  "get_log_file_info",
  "get_data_paths",
] as const;

/** 登记表 ③：点进「改动足迹」才该出现的那一发。 */
const FOOTPRINT_IPC = ["drift_ledger_report"] as const;

/** 登记表 ④：点进某台机器的子页才该出现的那一发（步 14a 之后「足迹」住那儿）。 */
const MACHINE_PAGE_IPC = ["config_surface_report"] as const;

const tick = () => new Promise((r) => setTimeout(r, 0));
const uniq = (xs: string[]) => [...new Set(xs)].sort();

/** 取一段时间内新增的命令名（去重 + 排序），并把游标推到末尾。 */
function since(mark: number): string[] {
  return uniq(ipc.calls.slice(mark));
}

describe("`70 §8` 判据 #3：非落地页零 I/O（第一刀 · 步 2）", () => {
  beforeEach(() => {
    ipc.calls = [];
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
  });

  it("🔴 量具自检：录音机真的在录（落地页那一趟必须非空且逐字相等）", async () => {
    // ⚠ 这一格是**反空真锚**。没有它，下面每一条「不许出现 X」都可以靠
    //「录音机根本没接上、一条都没录到」零命中地绿。
    new SettingsPanel({ windowMode: true });
    await tick();
    expect(uniq(ipc.calls)).toEqual(uniq([...LANDING_IPC]));
    expect(ipc.calls.length, "一条都没录到 ⇒ 本文件全部断言都是空转").toBeGreaterThan(0);
  });

  it("构造面板只发落地页那几条 —— 「应用」「改动足迹」「机器子页」的一条都不许有", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    const got = uniq(ipc.calls);
    // 相等，不是「不包含」——「不包含」对「多打了一条别的」是瞎的。
    expect(got).toEqual(uniq([...LANDING_IPC]));
    for (const name of [...APP_PAGE_IPC, ...FOOTPRINT_IPC, ...MACHINE_PAGE_IPC]) {
      expect(got, `${name} 不该在用户点进那一页之前就打出去`).not.toContain(name);
    }
  });

  it("点进「应用」⇒ **恰好**多出那三发（日志 2 · 数据位置 1）", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    await tick();
    expect(since(mark)).toEqual(uniq([...APP_PAGE_IPC]));
  });

  it("再点一次「应用」⇒ 一发都不许多（幂等：切页不是轮询）", async () => {
    const p = new SettingsPanel({ windowMode: true });
    void p;
    await tick();
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-machines")!.click();
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    await tick();
    expect(since(mark)).toEqual([]);
  });

  it("点进「改动足迹」⇒ 恰好多出漂移记账那一发", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-footprint")!.click();
    await tick();
    expect(since(mark)).toEqual(uniq([...FOOTPRINT_IPC]));
  });

  it("🔴 步 14a：「足迹」那一发跟着**机器子页**走，不跟顶层「改动足迹」页走", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    // 顶层「改动足迹」页上**不再**有它（它搬到机器子页去了）。
    let mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-footprint")!.click();
    await tick();
    expect(since(mark)).not.toContain("config_surface_report");
    // 点进本机页才发。
    mark = ipc.calls.length;
    document
      .querySelector<HTMLButtonElement>('#settings-tab-machine\\:（本机）')!
      .click();
    await tick();
    expect(since(mark)).toEqual(uniq([...MACHINE_PAGE_IPC]));
  });

  it("`open()` 之后仍然只碰落地页（这就是用户说的「打开设置」那一下）", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await tick();
    ipc.calls = [];
    await p.open();
    await tick();
    const got = uniq(ipc.calls);
    for (const name of [...APP_PAGE_IPC, ...FOOTPRINT_IPC, ...MACHINE_PAGE_IPC]) {
      expect(got, `打开设置不该碰 ${name}`).not.toContain(name);
    }
    // 反向锚：`open()` 真的做事了（它至少要重读一次 config）—— 否则上面那一串
    // 「不该有」可以靠「`open()` 什么都没做」零命中地绿。
    expect(got, "open() 一条 IPC 都没发 ⇒ 上面那几条是空转").toContain("load_config");
  });

  it("🔴 重开一次设置 ⇒ 那几页的第一发**重新算**（不是永远只发一次）", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await tick();
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    await tick();
    await p.open();
    await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    await tick();
    // 重开之后再点进「应用」要拿到**新读数** —— 否则用户改了外部状态、重开设置
    // 看到的还是上一次那份，而界面上看不出来。
    expect(since(mark)).toEqual(uniq([...APP_PAGE_IPC_ON_REOPEN]));
  });
});
