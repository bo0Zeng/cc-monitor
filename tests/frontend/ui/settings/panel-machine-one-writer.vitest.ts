// 设置窗里那台的几格（导航点 · 卡头的点与词 · 「这台上的 cc-monitor」折叠头）只一条路写：那台状态（后端写好的点 · 词 · 那一句）；
// 本机后端没起来时是壳那一格；都没有 ⇒「说不清」那一形。后端那一行问完状态、表单里没保存的开关都不另写一遍。
// 坏的样子：那台推「需手动」，紧跟着后端那一行问完、照「连着」把点盖成连着；本机后端没起来时一份迟到的本机成品把壳那一句盖掉；
// 卡上刚拨了「连接这台」还没存，整页就先写成「已停用」；折叠头与后端那一行状态格各拼一句、说法不一样。
// 从 `SettingsPanel` 这一头进：真 `BackendSection`（IPC 换成桩）· 真机器页 · 真导航；机器列表那一侧换成桩。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import HEALTH_GOLDEN from "../../../__fixtures__/backend-health.golden.json";

const LOCAL_PAGE = "machine:<local>";
const REMOTE_PAGE = "machine:devbox";

const captured = vi.hoisted(() => ({
  machine: null as null | ((origin: string, m: unknown) => void),
  local: null as null | ((l: unknown) => void),
  /** 卡上「连接这台」此刻的表单值（没保存）。 */
  formOff: false,
  /** 壳此刻说本机后端怎样（`backend_status` 的 `local` 照它答，与推来的那一帧同一个事实）。 */
  localBackend: { state: "up" } as Record<string, unknown>,
  /** 每一问问了几次（`backend_status` · 读退出策略那一问 · 读认不出的会话流那一问）。 */
  asked: { status: 0, exit: 0, drift: 0 },
  /** 宿主喂进机器列表那一侧的那台事实（卡头第二行照它拼）。 */
  facts: new Map<string, unknown>(),
}));

vi.mock("../../../../src/frontend/ui/machine-feed", () => ({
  onMachineState: (cb: (origin: string, m: unknown) => void) => {
    captured.machine = cb;
    return () => {};
  },
  onLocalBackend: (cb: (l: unknown) => void) => {
    captured.local = cb;
    return () => {};
  },
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (args: unknown) => {
        switch (name) {
          case "backend_machines":
            return Promise.resolve(["<local>", "devbox"]);
          case "backend_status":
            captured.asked.status++;
            return Promise.resolve({ channel: captured.localBackend.state === "up", pid: 7, health: HEALTH_GOLDEN.cases[0].face, local: captured.localBackend });
          case "chan_call": {
            const op = (args as { op?: unknown } | undefined)?.op;
            if (op === "exit-policy-read") captured.asked.exit++;
            else captured.asked.drift++;
            if (op !== "exit-policy-read") return Promise.reject(new Error("没有这一问"));
            const u = new TextEncoder().encode(JSON.stringify({ state: "absent", killOnExit: false, said: "退出时后台照常跑" }));
            return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
          }
          default:
            return Promise.reject(new Error(`桩里没有 ${name}`));
        }
      },
    },
  ),
}));

vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:<local>",
  RemoteSection: class {
    headActions = (): HTMLElement[] => [];
    pageIdOfMachine = (origin: string): string | null => (origin === "devbox" ? REMOTE_PAGE : null);
    menuFor = (): unknown[] => [];
    metaOfPage = (id: string): string | null => {
      const origin = id === REMOTE_PAGE ? "devbox" : null;
      const facts = origin === null ? undefined : captured.facts.get(origin);
      return facts === undefined ? null : machineMetaReal("head", "devbox", facts as never);
    };
    setMachine = (): void => {};
    setLocalBackend = (): void => {};
    setFacts = (origin: string, facts: unknown): void => {
      captured.facts.set(origin, facts);
    };
    originOfPage = (id: string): string | null => (id === REMOTE_PAGE ? "devbox" : null);
    isUnconfiguredPage = (): boolean => false;
    isDisabledPage = (id: string): boolean => id === REMOTE_PAGE && captured.formOff;
    runFix = (): void => {};
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
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: () => {}, failToast: () => {}, undoToast: () => {} }));

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import type { MachineState } from "../../../../src/frontend/ui/settings/machine-state";
import { __setHostFactsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { factsOn } from "../../../test-support/host-facts";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { machineMeta as machineMetaReal } from "../../../../src/frontend/ui/settings/machine-page";

/** 那台的状态（结构夹具：只填界面读的那几格；字是夹具自己的）。 */
const state = (over: Partial<MachineState> = {}): MachineState => ({
  name: "devbox",
  state: "needs_update",
  offline: false,
  reason: null,
  stage: null,
  version: "9.9.9",
  versionRelation: "older",
  os: null,
  word: "核心·要更新",
  said: "核心·那台要更新",
  offlineSaid: "核心·devbox 离线",
  fixes: [],
  seenHostKey: null,
  expectedHostKey: null,
  detail: null,
  ...over,
});

const settle = async (): Promise<void> => {
  for (let i = 0; i < 12; i++) await new Promise((r) => setTimeout(r, 0));
};

const pageOf = (id: string): HTMLElement =>
  [...document.querySelectorAll<HTMLElement>(".machine-page")].find((p) => p.querySelector(`[data-backend-cells]`)?.getAttribute("data-backend-cells") === (id === LOCAL_PAGE ? "<local>" : "devbox"))!;
const navDot = (id: string): string | undefined =>
  document.querySelector<HTMLElement>(`[data-route-id="${id}"] [data-state]`)?.dataset.state;
const headDot = (id: string): string | undefined => pageOf(id).querySelector<HTMLElement>(".machine-head-top [data-state]")?.dataset.state;
const headWord = (id: string): string => pageOf(id).querySelector<HTMLElement>(".machine-head-state")!.textContent ?? "";
const problem = (id: string): HTMLElement => pageOf(id).querySelector<HTMLElement>(".machine-problem")!;
const ccSummary = (id: string): string =>
  pageOf(id).querySelector<HTMLElement>(".machine-monitor-fold > [data-fold-head] > span:last-child")!.textContent ?? "";
const ccState = (id: string): string => pageOf(id).querySelector<HTMLElement>(".backend-row-state")!.textContent ?? "";

async function mount(): Promise<void> {
  new SettingsPanel({ windowMode: true });
  await settle();
  expect(captured.machine, "面板没订机器状态推送").not.toBeNull();
  expect(pageOf(REMOTE_PAGE), "那台的页没建起来").toBeDefined();
}

beforeEach(() => {
  __setHostFactsForTests(factsOn("linux"));
  captured.machine = null;
  captured.local = null;
  captured.formOff = false;
  captured.localBackend = { state: "up" };
  captured.asked = { status: 0, exit: 0, drift: 0 };
  captured.facts.clear();
  document.body.textContent = "";
});
afterEach(() => __setHostFactsForTests(null));

describe("设置窗那台的几格只由那台状态写", () => {
  it("★ 那台说「需手动」、随后后端那一行问完（连着）⇒ 导航点与卡头的点仍是「需手动」那一档", async () => {
    await mount();
    captured.machine!("devbox", state());
    await settle();
    expect(navDot(REMOTE_PAGE), "导航点被后端那一行问完的「连着」盖掉").toBe("needs-you");
    expect(headDot(REMOTE_PAGE), "卡头的点被盖掉").toBe("needs-you");
    expect(headWord(REMOTE_PAGE)).toBe("核心·要更新");
  });

  it("★ 本机后端没起来（壳那一格）之后又来一份本机成品 ⇒ 导航点与卡头仍照壳那一格", async () => {
    await mount();
    captured.localBackend = { state: "down", look: "down", tone: "fail", word: "壳·没起来", said: "壳·本机后端没起来", fixes: [], next: null };
    captured.local!(captured.localBackend);
    captured.machine!("<local>", state({ name: "local", state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    expect(headWord(LOCAL_PAGE), "迟到的本机成品把壳那一句盖了").toBe("壳·没起来");
    expect(problem(LOCAL_PAGE).textContent).toContain("壳·本机后端没起来");
    expect(navDot(LOCAL_PAGE)).toBe("failed");
    expect(headDot(LOCAL_PAGE)).toBe("failed");
  });

  it("★ 卡上「连接这台」表单里关了、还没存 ⇒ 卡头照那台状态，不先写「已停用」", async () => {
    await mount();
    captured.machine!("devbox", state({ state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    captured.formOff = true;
    captured.machine!("devbox", state({ state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    expect(headWord(REMOTE_PAGE), "表单里没保存的开关写成了「已停用」").toBe("");
    expect(headDot(REMOTE_PAGE)).toBe("up");
    expect(navDot(REMOTE_PAGE)).toBe("up");
    expect(problem(REMOTE_PAGE).hidden).toBe(true);
  });

  it("★「这台上的 cc-monitor」折叠头与后端那一行状态格是同一句", async () => {
    await mount();
    captured.machine!("devbox", state({ state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    expect(ccState(REMOTE_PAGE)).not.toBe(copyText("backend.buildCells.querying"));
    expect(ccSummary(REMOTE_PAGE)).toBe(ccState(REMOTE_PAGE));
  });

  it("★ 版本不可比的那台：后端写在词里的「开发版」卡头只出一次（第二行不另补）", async () => {
    await mount();
    const dev = copyText("machineState.word.incomparable");
    captured.machine!("devbox", state({ state: "up", word: dev, said: "", version: null, versionRelation: "incomparable" }));
    await settle();
    const head = pageOf(REMOTE_PAGE).querySelector<HTMLElement>(".machine-head")?.textContent ?? "";
    expect(headWord(REMOTE_PAGE)).toBe(dev);
    expect(head.split(dev).length - 1, `卡头「${head}」里「${dev}」出了不止一次`).toBe(1);
  });

  it("★ 那台的状态帧内容没变、再来一帧 ⇒ 不再重问状态 · 退出策略 · 认不出的会话流", async () => {
    await mount();
    captured.machine!("devbox", state({ state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    const before = { ...captured.asked };
    captured.machine!("devbox", state({ state: "up", word: "", said: "", versionRelation: "same" }));
    captured.machine!("<local>", state({ name: "local", state: "up", word: "", said: "", versionRelation: "same" }));
    captured.machine!("<local>", state({ name: "local", state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    expect(captured.asked, "同样的一帧又触发了重问").toEqual({
      status: before.status + 1,
      exit: before.exit + 1,
      drift: before.drift + 1,
    });
  });

  it("★ 那台从离线变成连着 ⇒ 后端那一行重问一次，状态格照新的写", async () => {
    await mount();
    captured.machine!("devbox", state({ state: "down", offline: true, word: "", said: "" }));
    await settle();
    expect(ccState(REMOTE_PAGE)).toContain(copyText("machinePage.state.down"));
    const before = captured.asked.status;
    captured.machine!("devbox", state({ state: "up", word: "", said: "", versionRelation: "same" }));
    await settle();
    expect(captured.asked.status).toBe(before + 1);
    expect(ccState(REMOTE_PAGE)).toContain(copyText("backend.status.connected"));
  });

  it("★ 壳推了本机后端没起来之后，后端那一行问回来的旧「起着」不盖它", async () => {
    await mount();
    captured.local!({ state: "down", look: "down", tone: "fail", word: "壳·没起来", said: "壳·本机后端没起来", fixes: [], next: null });
    // `backend_status` 的回包还是先前那一份（起着）：它不许写本机那一格。
    captured.localBackend = { state: "up" };
    captured.machine!("<local>", state({ name: "local", state: "up", word: "", said: "", versionRelation: "same", version: "1.2.3" }));
    await settle();
    expect(headWord(LOCAL_PAGE), "回包里那份旧的盖了壳推来的那一格").toBe("壳·没起来");
  });
});
