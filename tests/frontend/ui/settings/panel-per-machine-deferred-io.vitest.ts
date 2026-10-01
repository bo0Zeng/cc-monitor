/**
 * ST1「延后加载」（`设计/70 §5.3` 判据 2：**子页内容只在该子页可见时才发 I/O**）。
 *
 * `panel-deferred-io.vitest.ts` 头注〔射程〕② 登记过一笔债：per-machine 那几块
 * （账号 / 终端集成（〔AL1c〕已并进「别名」）/ 当时的 cc-bus 钩子）在那边被 stub 掉了，而它们**构造期也发 I/O**
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

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
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
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
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
vi.mock("../../../../src/frontend/ui/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
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

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";

/** 落地页（机器列表）这一趟该打的命令 —— 与 `panel-deferred-io.vitest.ts` 的 LANDING_IPC 同一件事。 */
const LANDING_IPC = ["load_config", "backend_machines", "backend_status"] as const;

/**
 * 本机子页**第一次可见**时新增的那一批：足迹 1 发 ＋ per-machine 那几块各自的第一发。
 * 逐块登记（改哪一块的读口，这张表跟着改一行，而不是整体换个数）：
 */
const LOCAL_PAGE_IPC = [
  "footprint_client_facts", // 足迹（步 14a）：〔MIG-3b 续〕本机那一栏第一拍问 monitor 自己那几行的环境（录音机下它失败就停在这一拍）
  "drift_ledger_report", // 〔ST2〕未识别的数据（原顶层「改动足迹」那一块）；〔ST3〕按这台去问
  "load_config", // 账号：先读远端清单（落地页也读它 —— 这里量的是「新增」那一段）
  // 〔C4c · 第四波 4B〕账号（本机那一支）改走通道：`chan_call`（`accounts-list` 发给 `<local>`）—— 本表按集合比，
  //   与下面插件那一条同名，不另起一行。
  // 〔AL1c · 4B〕这里原来还有终端集成那两发（终端集成的状态那一发〔AL1d：今天并进 `aliases_read`〕/ `cc_get_auto_launch`）：那块并进了「别名」
  // （一个 `<details>`，**第一次展开**才建它、才发那两发）⇒ 子页可见时不再发，往后又延了一层。
  // MCP · 资产目录 · 插件三块搬去了顶层「扩展」页（它自己那一页可见时才问），不在这一批里。
  "chan_call", // 账号本机那一支（`accounts-list` 发给 `<local>`）
  // cc-bus 钩子那一块拿掉了（钩子状态在扩展页 cc-bus 那一行，抽屉打开才问），它那两发随之没了。
] as const;

/**
 * 已经放过一次之后切到 aya：只有**跟着机器走、切换即重读**的那几块（足迹 · 未识别的数据）重读。
 * 账号那块也订阅了机器，但它只认「已加载的远端清单里有的那台」—— 录音机下清单是空的 ⇒ 不读；
 * 〔ST2 · 用户 09-24 裁「远端也有真栏」〕足迹在远端页上**也去问**（按 aya 那台，回声不对就说答不了）——
 *   它并进了 per-machine 那一批单例，切机器由它自己的订阅重读，恰好一发。
 */
const SWITCH_TO_AYA_IPC: readonly string[] = [
  "chan_call", // 足迹：〔MIG-3b 续〕按 aya 经通道问 `footprint-report`
  // 〔ST3〕「未识别的数据」按机器分：切到 aya 由它自己的订阅重读，按 aya 去问，恰好一发。
  "drift_ledger_report",
  "chan_call", // 〔MOD〕未识别的数据里记录那两面：经通道问 aya 那台后端 `drift-report`
];
/**
 * 第一次可见就是 aya：per-machine 那一批放一次，**每一发恰好一次**。
 * 〔AL1c〕从前这里如实记着一笔代价：「终端集成」只对本机有意义，却在远端页上也跟着这一批读了两发。
 *   它并进「别名」之后要展开才读 ⇒ 这笔代价没了，下面那两行跟着删。
 *   〔RM1b · 第四波〕「插件」那块不再是本机专属：它问的是**当前那台**（这里就是 aya），同样恰好一次。
 */
const FIRST_VISIT_AYA_IPC: readonly string[] = [
  "chan_call", // 足迹：〔ST2 · MIG-3b 续〕按 aya 经通道问 `footprint-report`
  "drift_ledger_report", // 未识别的数据：〔ST3〕按 aya 去问
  "chan_call", // 〔MOD〕未识别的数据里记录那两面：经通道问 aya 那台后端 `drift-report`
  "load_config", // 账号：读远端清单
  "chan_call", // 账号：aya 那一台（〔C4c〕经通道说 `accounts-list`）
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
