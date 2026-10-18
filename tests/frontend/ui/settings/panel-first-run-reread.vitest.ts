/**
 * 机器页顶上「开始用」那一块跟着它的事实重读：机器表改了（加完一台）⇒ 当场重读、带上新的台数；
 * 回到机器页（照第一、二步去别处做完回来）⇒ 重读。原先只在设置窗打开那一下读一次，加完机器那三步不自己打勾。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { reads, pages, count } = vi.hoisted(() => ({
  reads: [] as Array<number | undefined>,
  pages: { opts: null as null | { machinesReconciled?: () => void } },
  count: { n: 0 },
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => () => Promise.reject(new Error("没有真后端")) }),
}));
vi.mock("../../../../src/frontend/ui/settings/first-run", () => ({
  FirstRun: class {
    element = document.createElement("section");
    loadNow(remotes?: number): Promise<void> {
      reads.push(remotes);
      return Promise.resolve();
    }
  },
}));
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:<local>",
  RemoteSection: class {
    headActions = (): HTMLElement[] => [];
    pageIdOfMachine = (): string | null => null;
    menuFor = (): unknown[] => [];
    metaOfPage = (): string | null => null;
    setConnected = (): void => {};
    originOfPage = (): string | null => null;
    isUnconfiguredPage = (): boolean => false;
    machineCount = (): number => count.n;
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
    constructor(opts?: { pages?: { machinesReconciled?: () => void } }) {
      pages.opts = opts?.pages ?? null;
    }
  },
}));
vi.mock("../../../../src/frontend/ui/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/assets-section", () => ({
  AssetsSection: class { element = document.createElement("div"); },
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
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostFactsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/machine-context";
import { factsOn } from "../../../test-support/host-facts";

const tick = () => new Promise((r) => setTimeout(r, 0));
const visit = (id: string): void => document.querySelector<HTMLButtonElement>(`[id="settings-tab-${id}"]`)!.click();

describe("「开始用」跟着它的事实重读", () => {
  beforeEach(() => {
    reads.length = 0;
    count.n = 0;
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostFactsForTests(factsOn("linux"));
  });

  it("加完一台机器 ⇒ 当场重读、带上新的台数", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    reads.length = 0;
    count.n = 1;
    pages.opts!.machinesReconciled!();
    await tick();
    expect(reads).toEqual([1]);
  });

  it("从别的页回到机器页 ⇒ 重读", async () => {
    new SettingsPanel({ windowMode: true });
    await tick();
    visit("general");
    await tick();
    reads.length = 0;
    visit("machines");
    await tick();
    expect(reads).toEqual([0]);
  });
});
