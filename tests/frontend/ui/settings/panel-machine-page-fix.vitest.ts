// 机器单页卡头的问题行：那几颗修法（比对指纹… · 重试 · 更新 · 推送公钥 · 连接这台）点了要真做事。
// WIN5 回归：「主机指纹变更 · 未连接 ［比对指纹…］」在机器单页上点了什么都不出 —— 页是建了，修法那一格没接上（`onFix` 空着）。
// 本文件从 `SettingsPanel` 这一头进：推一帧那台的状态成品 ⇒ 机器页问题行出那颗修法 ⇒ 点它 ⇒ 交到机器列表那一侧同一个口子。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const LOCAL_PAGE = "machine:（本机）";
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
}));

vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
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
  claudeDirIn: () => null,
  setClaudeDirOverride: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  behaviorIn: vi.fn(() => ({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    showBgSessions: false,
    notifyTurnEnd: false,
    resumeCommand: "",
    resumeCommandPresets: [],
    resumeInTmux: false,
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
import { fixLabel } from "../../../../src/frontend/ui/settings/machine-state";

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
      reason: null,
      stage: null,
      version: null,
      versionRelation: null,
      os: null,
      fixes: ["compare_fingerprint"],
      seenHostKey: "SHA256:new",
      detail: null,
    });
    const label = fixLabel("compare_fingerprint");
    const btn = [...document.querySelectorAll<HTMLButtonElement>(".machine-problem button")].find((b) => b.textContent === label);
    expect(btn, `机器页问题行里没有［${label}］`).toBeDefined();
    btn!.click();
    expect(captured.runFix).toEqual([[REMOTE_PAGE, "compare_fingerprint"]]);
    expect(copyText("machineState.fix.compareFingerprint")).toBe(label);
  });
});
