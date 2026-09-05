// `N-F1b` `NF1bD5`：**per-machine 那几块分节，在哪一页上真的看得见。**
//
// # 这份文件为什么必须存在（它补的正是一个结构性的洞）
//
// `N-F1b` 前四条 DoD 全兑现之后，用户**仍然看不见**账号那一节 —— 因为面板那一层还有
// 第二道锁：`panel.ts` 把每一块 per-machine 分节登记成 `appliesTo: "local" | "remote" | "both"`，
// 而 `movePerMachineTo` 按当前是哪一页把不匹配的整块 `hidden` 掉。
// 账号那一块当时登记成 `"remote"` ⇒ **本机页上它是 `hidden`**，
// 而一台没有配任何远端的机器**只有本机页**。
//
// ⚠⚠ 而 `accounts-section.vitest.ts` 那一族**一格都逮不到这件事**：它们在 jsdom 里
// 直接 `new AccountsSection()`，**结构性地绕过了 `panel.ts` 那一层**。
// 「那一节渲染得对不对」与「那一节在这一页上出不出现」是两件事，
// 前者那 56 格全绿的同时，后者可以整块是黑的。
// ⇒ 本文件**从 `SettingsPanel` 这一头进**，断的是**渲染之后的 `el.hidden`**，
// **不是** `appliesTo` 那个字面值 —— 断字面值等于把实现抄一遍当判据，
// `movePerMachineTo` 那行判断改坏了它照样绿。
//
// # 尺子的非空对照（本文件的第三条）
//
// 「`hidden === false`」这种断言最容易变成空真：抓错元素、或整块压根没进文档，
// 读出来都是 `false`。⇒ 第三条拿一块**登记成 `"local"`** 的分节（终端集成）
// 在**远端页**上读，必须读出 `hidden === true`。同一把尺子读得出两种结局，
// 上面两条才算数。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const LOCAL_PAGE = "machine:（本机）";
const REMOTE_PAGE = "machine:aya";

/**
 * `RemoteSection` 的替身抓住 panel 递进来的那套 `pages` 回调。
 *
 * ⚠ 替身**必须履行它替身的那份契约**（同 `panel-block-isolation.vitest.ts` 的头注）：
 * 真 `RemoteSection` 是在异步 `refresh()` 里调 `addMachinePage` 注册机器页的，
 * panel 正是在那一刻把 per-machine 那几块安顿下来。替身不注册 ⇒ 那几块永远游离在
 * 文档之外，本文件三条会在「找不到元素」上一起假红/假绿。
 */
const captured = vi.hoisted(() => ({
  pages: null as null | {
    addMachinePage: (
      id: string,
      title: string,
      el: HTMLElement,
      parts?: { connection: HTMLElement; components: HTMLElement },
    ) => void;
    removeMachinePage: (id: string) => void;
    navigateToMachinePage: (id: string) => void;
  },
}));

vi.mock("./remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = vi.fn();
    constructor(opts?: { pages?: typeof captured.pages }) {
      captured.pages = opts?.pages ?? null;
      // 延后注册：真的那份也是异步注册的，同步注册会让机器页抢在主路由前面。
      setTimeout(() => {
        const local = document.createElement("div");
        opts?.pages?.addMachinePage(LOCAL_PAGE, "本机", local);
        // 远端那一页按**真实形状**注册（带四栏 `parts`）——
        // `movePerMachineTo` 对带栏 / 不带栏走两条不同的路，两条都要真的跑到。
        const remote = document.createElement("div");
        opts?.pages?.addMachinePage(REMOTE_PAGE, "aya", remote, {
          connection: document.createElement("div"),
          components: document.createElement("div"),
        });
      }, 0);
    }
  },
}));

// —— 其余重子分区一律 stub 成 `{ element }`，本文件只关心「哪一块在哪一页上可见」 ——
vi.mock("./data-section", () => ({
  DataSection: class {
    element = document.createElement("div");
    refresh = vi.fn();
  },
}));
vi.mock("./diagnostics-section", () => ({
  DiagnosticsSection: class {
    element = document.createElement("div");
  },
}));
vi.mock("./cc_integration", () => ({
  CcIntegrationSection: class {
    element = (() => {
      const d = document.createElement("div");
      d.className = "cc-integration-stub";
      return d;
    })();
  },
}));
vi.mock("./mcp-section", () => ({
  McpSection: class {
    element = document.createElement("div");
  },
}));
vi.mock("./accounts-section", () => ({
  AccountsSection: class {
    element = (() => {
      const d = document.createElement("div");
      d.className = "accounts-section-stub";
      return d;
    })();
  },
}));
vi.mock("../keybindings/editor", () => ({
  KeybindingsEditor: class {
    element = document.createElement("div");
  },
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: vi.fn(),
    popOverlay: vi.fn(),
    startRecording: vi.fn(),
    cancelRecording: vi.fn(),
    exportOverrides: vi.fn().mockReturnValue({}),
    applyOverrides: vi.fn(),
  },
}));
vi.mock("../theme", () => ({
  applyTheme: vi.fn(),
  applyThemeToken: vi.fn(),
  loadTheme: vi.fn().mockResolvedValue({}),
  saveTheme: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../paths", () => ({
  getClaudeDirOverride: vi.fn().mockResolvedValue(""),
  setClaudeDirOverride: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../behavior", () => ({
  getBehavior: vi.fn(async () => ({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    showBgSessions: false,
    notifyTurnEnd: false,
    resumeCommandLocal: "",
    resumeCommandRemote: "",
    resumeCommandLocalPresets: [],
    resumeCommandRemotePresets: [],
  })),
  setBehavior: vi.fn().mockResolvedValue(undefined),
  withResumePreset: (list: readonly string[]) => [...list],
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));

import { SettingsPanel } from "./panel";
import { __setHostOsForTests } from "./host-os";

// jsdom 的 UA 含 `linux` ⇒ 不置覆盖值的话「终端集成」那块根本不构造，
// 而本文件第三条（尺子的非空对照）正要拿它当那块 `appliesTo: "local"` 的分节。
beforeEach(() => {
  __setHostOsForTests("windows");
  captured.pages = null;
  document.body.textContent = "";
});
afterEach(() => __setHostOsForTests(null));

/** 建面板并等机器页注册落定（替身在 `setTimeout(0)` 里注册）。 */
async function mountPanel(): Promise<void> {
  const p = new SettingsPanel({ windowMode: true });
  void p;
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

/**
 * 一块 per-machine 分节的**外壳**（`hidden` 就挂在它身上）。
 *
 * 用「从块内的 stub 往上找 `.settings-group`」而不是按标题文字找：
 * 标题是**文案**，改一次文案判据就静默失效；而 stub 那个 class 是本文件自己放的。
 */
function blockOf(innerSelector: string): HTMLElement {
  const inner = document.querySelector(innerSelector);
  expect(inner, `找不到 ${innerSelector} —— 那一块压根没构造，下面全是在 null 上判`).not.toBeNull();
  const block = inner!.closest<HTMLElement>(".settings-group");
  expect(block, `${innerSelector} 不在任何 .settings-group 里 —— 外壳形状变了`).not.toBeNull();
  return block!;
}

describe("N-F1b NF1bD5：账号那一节在本机页上真的显示出来（穿过 panel.ts 那一层）", () => {
  it("★ 本机页：账号那一块 hidden === false —— 一台没有远端的机器只有这一页", async () => {
    await mountPanel();
    const block = blockOf(".accounts-section-stub");
    // 非空对照①：它真的落在文档里，不是游离着（游离的元素 hidden 也读作 false）。
    expect(
      document.body.contains(block),
      "账号那一块不在文档里 —— 下面那条会在一个游离节点上恒真",
    ).toBe(true);
    // 正题：本机页上它不许被藏起来。
    // 🔴 先证会红：把 `panel.ts` 那一节的 `appliesTo` 改回 `"remote"` ⇒ 这一条当场红。
    expect(
      block.hidden,
      "本机页上账号那一节被 hidden 掉了 —— 那一节渲染得再对，用户也看不见它",
    ).toBe(false);
  });

  it("★ 另一侧：远端页上账号那一节照旧可见（两侧都断，同 NF1bD1 那条的形）", async () => {
    await mountPanel();
    expect(captured.pages, "没抓到 pages 回调 —— 下面导不了航").not.toBeNull();
    captured.pages!.navigateToMachinePage(REMOTE_PAGE);
    const block = blockOf(".accounts-section-stub");
    expect(document.body.contains(block)).toBe(true);
    expect(block.hidden, "远端页上账号那一节不见了 —— 本件把远端那一侧弄坏了").toBe(false);
  });

  it("★ 尺子的非空对照：同一把尺子在远端页上读一块 `appliesTo: \"local\"` 的分节，读出 hidden === true", async () => {
    await mountPanel();
    // 本机页上「终端集成」该是可见的（它就是本机专属的那一块）。
    const local = blockOf(".cc-integration-stub");
    expect(local.hidden, "本机页上连本机专属那一块都被藏了 —— 尺子或夹具坏了").toBe(false);
    // ★ 切到远端页：同一块必须翻面。翻不了 ⇒ 这把尺子读不出 `true`，
    //   上面两条「=== false」就都是空真。
    captured.pages!.navigateToMachinePage(REMOTE_PAGE);
    expect(
      local.hidden,
      "远端页上本机专属那一块仍然可见 —— 这把尺子读不出 hidden===true，上面两条是空真",
    ).toBe(true);
  });
});
