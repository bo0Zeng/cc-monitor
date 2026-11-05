// 那台的状态成品先到、机器页后注册（打开设置窗时机器表是异步读的）：注册上来那一刻照已到的成品画，不丢那一帧。
// 坏的样子：主窗栏底写「主机指纹变更 · 比对指纹…」，设置窗机器页只写「离线」，［比对指纹…］拿不到两枚指纹、比对框不弹。
// 本文件从 `SettingsPanel` 这一头进：先推成品 ⇒ 再注册页 ⇒ 页头问题行出那颗修法，机器那一侧收到那一份成品（比对要的两枚在里面）。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const LOCAL_PAGE = "machine:<local>";
const REMOTE_PAGE = "machine:devbox";

const captured = vi.hoisted(() => ({
  machine: null as null | ((origin: string, m: unknown) => void),
  /** 机器那一侧收到的成品（页 id · 成品）。 */
  painted: [] as [string, unknown][],
  /** 叫它注册那几页（测试里手动按）。 */
  register: null as null | (() => void),
  /** 注册完、列表都读完（同 `refresh` 收尾那一声）。 */
  settle: null as null | (() => void),
  /** 机器表改完那一声。 */
  reconciled: null as null | (() => void),
  registered: false,
}));

vi.mock("../../../../src/frontend/ui/machine-feed", () => ({
  onMachineState: (cb: (origin: string, m: unknown) => void) => {
    captured.machine = cb;
    return () => {};
  },
  onLocalBackend: () => () => {},
}));

vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:<local>",
  RemoteSection: class {
    headActions = (): HTMLElement[] => [];
    // 页还没注册 ⇒ 列表里没有这台（同真分节：页 id 建卡时才定）。
    pageIdOfMachine = (origin: string): string | null => (origin === "devbox" && captured.registered ? REMOTE_PAGE : null);
    menuFor = (): unknown[] => [];
    metaOfPage = (): string | null => null;
    setMachine = (id: string, m: unknown): void => void captured.painted.push([id, m]);
    setFacts = (): void => {};
    originOfPage = (id: string): string | null => (id === REMOTE_PAGE ? "devbox" : null);
    isUnconfiguredPage = (): boolean => false;
    runFix = (): void => {};
    machineCount = (): number => 1;
    element = document.createElement("div");
    refresh = vi.fn();
    constructor(opts?: {
      pages?: {
        addMachinePage: (id: string, title: string, el: HTMLElement, parts?: unknown) => void;
        machinePagesSettled?: () => void;
        machinesReconciled?: () => void;
      };
    }) {
      captured.register = () => {
        captured.registered = true;
        opts?.pages?.addMachinePage(LOCAL_PAGE, "local", document.createElement("div"));
        opts?.pages?.addMachinePage(REMOTE_PAGE, "devbox", document.createElement("div"), {
          connection: document.createElement("div"),
          components: document.createElement("div"),
          terminal: document.createElement("div"),
        });
      };
      captured.settle = () => opts?.pages?.machinePagesSettled?.();
      captured.reconciled = () => opts?.pages?.machinesReconciled?.();
    }
  },
}));

vi.mock("../../../../src/frontend/ui/settings/data-section", () => ({
  DataSection: class {
    element = document.createElement("div");
    refresh = vi.fn();
  },
}));
vi.mock("../../../../src/frontend/ui/settings/diagnostics-section", () => ({
  DiagnosticsSection: class {
    element = document.createElement("div");
    headButton = () => document.createElement("button");
  },
}));
vi.mock("../../../../src/frontend/ui/settings/mcp-section", () => ({
  McpSection: class {
    element = document.createElement("div");
  },
}));
vi.mock("../../../../src/frontend/ui/settings/accounts-section", () => ({
  AccountsSection: class {
    element = document.createElement("div");
  },
}));
vi.mock("../../../../src/frontend/ui/keybindings/editor", () => ({
  KeybindingsEditor: class {
    element = document.createElement("div");
  },
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
vi.mock("../../../../src/frontend/ui/theme", () => ({
  applyTheme: vi.fn(),
  applyThemeToken: vi.fn(),
  themeIn: () => ({}),
  saveTheme: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../../src/frontend/ui/paths", () => ({
  readClaudeDir: () => Promise.resolve(null),
  setClaudeDirOverride: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  behaviorIn: vi.fn(() => ({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    resumeCommand: "",
    resumeCommandPresets: [],
    resumeWhere: "managed",
  })),
  setBehavior: vi.fn().mockResolvedValue(undefined),
  withResumePreset: (list: readonly string[]) => [...list],
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostFactsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { factsOn } from "../../../test-support/host-facts";

import { copyText } from "../../../../src/frontend/ui/copy-table";

/** 指纹变了那一份成品（结构夹具：只填界面读的那几格）。 */
const HOST_KEY_CHANGED = {
  name: "devbox",
  state: "host_key_changed",
  offline: true,
  reason: "host_key",
  stage: null,
  version: null,
  versionRelation: null,
  os: null,
  word: copyText("machineState.word.hostKey"),
  said: copyText("machineState.problem.hostKey"),
  offlineSaid: "devbox 离线",
  fixes: [{ fix: "compare_fingerprint", label: copyText("machineState.fix.compareFingerprint") }],
  seenHostKey: "SHA256:new",
  expectedHostKey: "SHA256:old",
  detail: null,
  next: null,
};

const tick = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  __setHostFactsForTests(factsOn("linux"));
  captured.machine = null;
  captured.painted = [];
  captured.register = null;
  captured.settle = null;
  captured.reconciled = null;
  captured.registered = false;
  document.body.textContent = "";
});
afterEach(() => __setHostFactsForTests(null));

describe("机器页晚于那台的成品注册", () => {
  it("★ 成品先到、页后注册 ⇒ 注册那一刻照那份成品画：页头出［比对指纹…］，机器那一侧拿到带两枚指纹的那一份", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    expect(captured.machine, "面板没订机器状态推送").not.toBeNull();
    captured.machine!("devbox", HOST_KEY_CHANGED);
    expect(captured.painted, "页还没注册：那一侧此刻无处可画").toEqual([]);
    captured.register!();
    expect(captured.painted, "页注册上来没照已到的成品画").toContainEqual([REMOTE_PAGE, HOST_KEY_CHANGED]);
    const label = copyText("machineState.fix.compareFingerprint");
    const btn = [...document.querySelectorAll<HTMLButtonElement>(".machine-problem button")].find((b) => b.textContent === label);
    expect(btn, `机器页问题行里没有［${label}］`).toBeDefined();
  });

  it("★ 列表都读完（行已建好）⇒ 已到的每一份成品再画一遍：最后落下的是成品，不是通道那一句", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    captured.machine!("devbox", HOST_KEY_CHANGED);
    captured.register!();
    captured.painted = [];
    captured.settle!();
    expect(captured.painted).toEqual([[REMOTE_PAGE, HOST_KEY_CHANGED]]);
  });

  it("★ 机器表改完（卡上刚关了「连接该机」）⇒ 重画照的仍是那台最近一份成品：「已停用」等那台的下一份来写，界面不照表单先判", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    captured.machine!("devbox", HOST_KEY_CHANGED);
    captured.register!();
    captured.settle!();
    captured.painted = [];
    captured.reconciled!();
    expect(captured.painted).toEqual([[REMOTE_PAGE, HOST_KEY_CHANGED]]);
  });
});
