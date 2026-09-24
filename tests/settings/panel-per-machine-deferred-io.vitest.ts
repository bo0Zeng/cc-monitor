/**
 * ST1「延后加载」（`设计/70 §5.3` 判据 2：**子页内容只在该子页可见时才发 I/O**）。
 *
 * `panel-deferred-io.vitest.ts` 头注〔射程〕② 登记过一笔债：per-machine 那几块
 * （账号 / 终端集成 / MCP / 插件 / cc-bus 钩子）在那边被 stub 掉了，而它们**构造期也发 I/O**
 * —— 落地页是「机器」列表，这几块住在机器子页上，打开设置时那几发是白发的。
 * 本文件**不 stub 它们**，用真分节 ＋ 两层录音机（`commands` 包装层 ＋ 直呼的 `invoke`）量：
 *
 * ① 构造面板 ⇒ 录到的命令集合 == 落地页登记表（per-machine 那几块一条都不许有）；
 * ② 点进本机子页 ⇒ **新增**的集合 == 登记的 per-machine 那一批 ∪ 足迹那一发（两向相等）；
 * ③ 回列表页再点回来 ⇒ 一发都不许多（切页不是轮询）。
 *
 * 〔射程〕盖不到：真实排版；远端机器页（本文件的 RemoteSection 只注册本机页）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { ipc } = vi.hoisted(() => ({ ipc: { calls: [] as string[] } }));

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
// 绕过包装层直呼 `invoke` 的那几处（`accounts.ts` 的本机读口等）也要录上。
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (name: string) => {
    ipc.calls.push(name);
    return Promise.reject(new Error(`[录音机] ${name} 没有真后端`));
  },
}));
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
        opts?.pages?.addMachinePage("machine:（本机）", "本机", document.createElement("div"));
        // 第二台（远端）：量「一次打开里 per-machine 那几块只放一次」要有两个机器页。
        opts?.pages?.addMachinePage("machine:aya", "aya", document.createElement("div"));
      }, 0);
    }
  },
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
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "../../src/settings/panel";
import { __setHostOsForTests } from "../../src/settings/host-os";
import { __resetMachineContextForTests } from "../../src/settings/machine-context";

/** 落地页（机器列表）这一趟该打的命令 —— 与 `panel-deferred-io.vitest.ts` 的 LANDING_IPC 同一件事。 */
const LANDING_IPC = ["load_config", "backend_machines", "backend_status"] as const;

/**
 * 本机子页**第一次可见**时新增的那一批：足迹 1 发 ＋ per-machine 那几块各自的第一发。
 * 逐块登记（改哪一块的读口，这张表跟着改一行，而不是整体换个数）：
 */
const LOCAL_PAGE_IPC = [
  "config_surface_report", // 足迹（步 14a）
  "load_config", // 账号：先读远端清单（落地页也读它 —— 这里量的是「新增」那一段）
  "list_local_accounts", // 账号（本机那一支）
  "cc_integration_status", // 终端集成（Windows）
  "cc_get_auto_launch", //   同上
  "read_mcp_servers", // MCP（本机）
  "list_mcp_project_dirs", // MCP 的项目候选
  "list_plugin_marketplaces", // 插件
  "list_remote_mcp_origins", // cc-bus 钩子：认得哪些远端
  "diagnose_local_cc_bus_hooks", // cc-bus 钩子：本机诊断
] as const;

/**
 * 已经放过一次之后切到 aya：只有**跟着机器走、切换即重读**的那一块（MCP）重读。
 * 账号那块也订阅了机器，但它只认「已加载的远端清单里有的那台」—— 录音机下清单是空的 ⇒ 不读；
 * cc-bus 钩子切机器**刻意不发**（它的既有语义：远端诊断只在点「检查远端」时发）。
 * 足迹在远端页上不发（`applyOriginGate`：今天的读口不收 origin）。
 */
const SWITCH_TO_AYA_IPC: readonly string[] = ["list_mcp_project_dirs", "read_remote_mcp_servers"];
/**
 * 第一次可见就是 aya：per-machine 那一批放一次，**每一发恰好一次**。
 * ⚠ 代价如实写：「终端集成」「插件」两块只对本机有意义（远端页上是藏着的），
 *   但它们与其余几块是同一批单例、同一个时刻放 ⇒ 这里也读了（都是本机读，不走 SSH）。
 */
const FIRST_VISIT_AYA_IPC: readonly string[] = [
  "load_config", // 账号：读远端清单
  "list_remote_accounts", // 账号：aya 那一台
  "cc_integration_status",
  "cc_get_auto_launch",
  "list_mcp_project_dirs", // MCP：aya 的项目候选
  "read_remote_mcp_servers", // MCP：aya 的 user scope
  "list_plugin_marketplaces",
  "list_remote_mcp_origins",
  "diagnose_local_cc_bus_hooks",
];

const tick = () => new Promise((r) => setTimeout(r, 0));
const uniq = (xs: readonly string[]) => [...new Set(xs)].sort();

describe("ST1 延后加载：per-machine 那几块只在机器子页可见时才发 I/O", () => {
  beforeEach(() => {
    ipc.calls = [];
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
  });

  it("① 构造面板：录到的 == 落地页那几条（per-machine 那几块一条都不许有）", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    await tick();
    expect(ipc.calls.length, "一条都没录到 ⇒ 本文件全部断言都是空转").toBeGreaterThan(0);
    expect(uniq(ipc.calls)).toEqual(uniq(LANDING_IPC));
  });

  it("② 点进本机子页：新增的 == 足迹 ∪ per-machine 那一批（两向相等）", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-machine\\:（本机）")!.click();
    for (let i = 0; i < 4; i++) await tick();
    expect(uniq(ipc.calls.slice(mark))).toEqual(uniq(LOCAL_PAGE_IPC));
  });

  it("③ 回列表页再点回来：一发都不许多（切页不是轮询）", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    await tick();
    const local = () =>
      document.querySelector<HTMLButtonElement>("#settings-tab-machine\\:（本机）")!;
    local().click();
    for (let i = 0; i < 4; i++) await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-machines")!.click();
    local().click();
    for (let i = 0; i < 4; i++) await tick();
    expect(ipc.calls.slice(mark)).toEqual([]);
  });

  it("④ 先进本机、再进 aya：per-machine 那一批**不再放第二遍**（它们是单例，切机器由各块自己的订阅重读）", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    await tick();
    document.querySelector<HTMLButtonElement>("#settings-tab-machine\\:（本机）")!.click();
    for (let i = 0; i < 4; i++) await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-machine\\:aya")!.click();
    for (let i = 0; i < 4; i++) await tick();
    // 逐条（带重数）相等：多放一遍的那几发会以「同名第二次」的形状出现，去重就看不见了。
    expect([...ipc.calls.slice(mark)].sort()).toEqual([...SWITCH_TO_AYA_IPC].sort());
  });

  it("⑤ 直接进 aya（第一次可见就是远端页）：每一发**恰好一次**（订阅那一路与首次加载那一路不许叠）", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    await tick();
    const mark = ipc.calls.length;
    document.querySelector<HTMLButtonElement>("#settings-tab-machine\\:aya")!.click();
    for (let i = 0; i < 4; i++) await tick();
    expect([...ipc.calls.slice(mark)].sort()).toEqual([...FIRST_VISIT_AYA_IPC].sort());
  });
});
