// 机器单页卡头的问题行：那几颗修法（比对指纹… · 重试 · 更新 · 推送公钥 · 连接这台）点了要真做事。
// WIN5 回归：「主机指纹变更 · 未连接 ［比对指纹…］」在机器单页上点了什么都不出 —— 页是建了，修法那一格没接上（`onFix` 空着）。
// 本文件从 `SettingsPanel` 这一头进：推一帧那台的状态成品 ⇒ 机器页问题行出那颗修法 ⇒ 点它 ⇒ 交到机器列表那一侧同一个口子。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const LOCAL_PAGE = "machine:<local>";
const REMOTE_PAGE = "machine:devbox";

const captured = vi.hoisted(() => ({
  machine: null as null | ((origin: string, m: unknown) => void),
  runFix: [] as [string, string][],
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
    pageIdOfMachine = (origin: string): string | null => (origin === "devbox" ? REMOTE_PAGE : null);
    menuFor = (): unknown[] => [];
    metaOfPage = (): string | null => null;
    setConnected = (): void => {};
    setMachine = (): void => {};
    setFacts = (): void => {};
    originOfPage = (id: string): string | null => (id === REMOTE_PAGE ? "devbox" : null);
    isUnconfiguredPage = (): boolean => false;
    isDisabledPage = (): boolean => false;
    runFix = (id: string, fix: string): void => void captured.runFix.push([id, fix]);
    element = document.createElement("div");
    refresh = vi.fn();
    constructor(opts?: { pages?: { addMachinePage: (id: string, title: string, el: HTMLElement, parts?: unknown) => void } }) {
      setTimeout(() => {
        opts?.pages?.addMachinePage(LOCAL_PAGE, "local", document.createElement("div"));
        opts?.pages?.addMachinePage(REMOTE_PAGE, "devbox", document.createElement("div"), {
          connection: document.createElement("div"),
          components: document.createElement("div"),
          terminal: document.createElement("div"),
        });
      }, 0);
    }
  },
}));

// —— 其余重子分区一律 stub 成 `{ element }`，本文件只关心「哪一块在哪一页上可见」 ——
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
    element = (() => {
      const d = document.createElement("div");
      d.className = "accounts-section-stub";
      return d;
    })();
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
import { buildMachinePage } from "../../../../src/frontend/ui/settings/machine-page";
import type { MachineFace } from "../../../../src/frontend/ui/settings/machine-state";
import { __setHostFactsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { factsOn } from "../../../test-support/host-facts";

import { copyText } from "../../../../src/frontend/ui/copy-table";

beforeEach(() => {
  __setHostFactsForTests(factsOn("linux"));
  captured.machine = null;
  captured.runFix = [];
  document.body.textContent = "";
});
afterEach(() => __setHostFactsForTests(null));

describe("机器单页问题行的修法接到真流程", () => {
  it("★ 指纹变了那一帧 ⇒ 机器页上出［比对指纹…］，点了交给机器那一侧（与列表那一行同一个口子）", async () => {
    new SettingsPanel({ windowMode: true });
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(captured.machine, "面板没订机器状态推送").not.toBeNull();
    captured.machine!("devbox", {
      state: "host_key_changed",
      reason: "host_key",
      stage: null,
      version: null,
      versionRelation: null,
      os: null,
      word: copyText("machineState.word.hostKey"),
      said: copyText("machineState.problem.hostKey"),
      fixes: [{ fix: "compare_fingerprint", label: copyText("machineState.fix.compareFingerprint") }],
      seenHostKey: "SHA256:new",
      expectedHostKey: "SHA256:old",
      detail: null,
      next: null,
    });
    const label = copyText("machineState.fix.compareFingerprint");
    const btn = [...document.querySelectorAll<HTMLButtonElement>(".machine-problem button")].find((b) => b.textContent === label);
    expect(btn, `机器页问题行里没有［${label}］`).toBeDefined();
    btn!.click();
    expect(captured.runFix).toEqual([[REMOTE_PAGE, "compare_fingerprint"]]);
  });
});

// 卡头的词与问题行只由那台状态写（`setMachine`）；通道那一路（`setConnected`）只换点。与机器列表那一行同一个做法。
describe("机器单页卡头：词与问题行只由那台状态写，通道只换点", () => {
  const face = (over: Partial<MachineFace> = {}): MachineFace => ({
    dot: "needs-you",
    word: copyText("machineState.word.installing"),
    problem: "那台的一句",
    tone: "busy",
    bar: true,
    fixes: [],
    detail: "",
    next: "",
    route: "",
    ...over,
  });
  const page = () => {
    const p = buildMachinePage({
      pageId: REMOTE_PAGE,
      name: "devbox",
      meta: "",
      connection: document.createElement("div"),
      ccMonitor: [],
      menu: () => [],
    });
    document.body.appendChild(p.element);
    const word = () => p.element.querySelector<HTMLElement>(".machine-head-state")!.textContent;
    const problem = () => p.element.querySelector<HTMLElement>(".machine-problem")!;
    const dot = () => p.element.querySelector<HTMLElement>(".machine-head-top [data-state]")?.dataset.state;
    return { p, word, problem, dot };
  };

  it("★ 那台推「安装中」、随后通道报连着 ⇒ 词仍是「安装中」、问题行仍是那台那一句，只换点", () => {
    const { p, word, problem, dot } = page();
    p.setMachine(face());
    p.setConnected(true);
    expect(word()).toBe(copyText("machineState.word.installing"));
    expect(problem().hidden).toBe(false);
    expect(problem().textContent).toContain("那台的一句");
    expect(dot()).toBe("up");
  });

  it("通道报断着 ⇒ 不自己写「离线」与离线那一句：词与问题行照那台状态", () => {
    const { p, word, problem, dot } = page();
    p.setMachine(face({ dot: "up", word: "", problem: "", tone: "warn", bar: false }));
    p.setConnected(false, "那台的离线那一句");
    expect(word()).toBe("");
    expect(problem().hidden).toBe(true);
    expect(dot()).toBe("failed");
  });

  it("停用那一笔：开回来 ⇒「已停用」与那一句撤掉，等那台的下一份", () => {
    const { p, word, problem } = page();
    p.setDisabled(true);
    expect(word()).toBe(copyText("machinePage.state.disabled"));
    expect(problem().textContent).toContain(copyText("machinePage.problem.disabled"));
    p.setDisabled(false);
    p.setConnected(true);
    expect(word()).toBe("");
    expect(problem().hidden).toBe(true);
    p.setMachine(face());
    expect(word()).toBe(copyText("machineState.word.installing"));
  });
});
