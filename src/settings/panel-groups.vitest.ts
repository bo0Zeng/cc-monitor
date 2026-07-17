// F82b（#56+#47）：设置面板 4 组终态结构测试。把各重子分区 stub 成占位 div、保留真
// CollapsibleGroup，只钉「buildBody 产出 连接/外观/远端/集成 四组（顺序）+ 远端留空占位」。
// 构造 SettingsPanel 不调 open()（配置读取在 open 里；本测只验 buildBody 的静态分组结构）。
import { describe, it, expect, vi } from "vitest";

// refresh spy 守 F82b 段移动没丢 this.remoteSection/this.dataSection 字段（丢了 open() 的
// `?.refresh()` 会静默 no-op）。vi.hoisted 让 spy 在被提升的 vi.mock 工厂里可见。
const { remoteRefresh, dataRefresh } = vi.hoisted(() => ({
  remoteRefresh: vi.fn(),
  dataRefresh: vi.fn(),
}));

// —— 重子分区 stub 成 { element }，聚焦分组结构本身 —— //
// 注：vi.mock 工厂被提升到文件顶部、不能引用顶层变量，故每个工厂内联一个占位类。
vi.mock("./remote-section", () => ({
  RemoteSection: class {
    element = document.createElement("div");
    refresh = remoteRefresh;
  },
}));
vi.mock("./data-section", () => ({
  DataSection: class {
    element = document.createElement("div");
    refresh = dataRefresh;
  },
}));
vi.mock("./diagnostics-section", () => ({
  DiagnosticsSection: class {
    element = document.createElement("div");
  },
}));
vi.mock("./cc_integration", () => ({
  CcIntegrationSection: class {
    element = document.createElement("div");
  },
}));
vi.mock("./mcp-section", () => ({
  McpSection: class {
    element = document.createElement("div");
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
  getBehavior: vi.fn().mockResolvedValue({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    showBgSessions: false,
    notifyTurnEnd: false,
    resumeCommandLocal: "",
    resumeCommandRemote: "",
  }),
  setBehavior: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "./panel";

function groupTitles(): string[] {
  return [...document.querySelectorAll(".settings-collapsible-title")].map(
    (e) => e.textContent ?? "",
  );
}

/** 取指定标题的 CollapsibleGroup 的 body（`.settings-collapsible-body-inner`），用于把子分节断言
 *  收窄到该组内部（防「行为」被移到别的组仍误判通过）。 */
function groupBody(title: string): HTMLElement {
  const root = [...document.querySelectorAll<HTMLElement>(".settings-collapsible")].find(
    (g) => g.querySelector(".settings-collapsible-title")?.textContent === title,
  );
  if (!root) throw new Error(`group not found: ${title}`);
  return root.querySelector<HTMLElement>(".settings-collapsible-body-inner") ?? root;
}

describe("F82b 设置 4 组终态", () => {
  it("buildBody 产出 连接 / 外观 / 远端 / 集成 四组（按序）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    expect(groupTitles()).toEqual(["连接", "外观", "远端", "集成"]);
  });

  it("远端组是留空占位（含占位文案）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    const empty = document.querySelector(".settings-group-empty");
    expect(empty).toBeTruthy();
    expect(empty?.textContent).toContain("暂无设置");
  });

  it("外观组**内部**含 行为 / 快捷键 / 字体 / 颜色 子分节小标题（收窄到该组）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    const subTitles = [...groupBody("外观").querySelectorAll(".settings-group-title")].map(
      (e) => e.textContent ?? "",
    );
    for (const t of ["行为", "快捷键", "字体", "颜色"]) {
      expect(subTitles).toContain(t);
    }
  });

  it("集成组内含 MCP 子分节（SS-3「MCP 进集成」，F87）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    const subTitles = [...groupBody("集成").querySelectorAll(".settings-group-title")].map(
      (e) => e.textContent ?? "",
    );
    expect(subTitles).toContain("MCP");
  });

  it("open() 刷新到 RemoteSection / DataSection（守 this.remoteSection/dataSection 字段未丢）", async () => {
    document.body.replaceChildren();
    remoteRefresh.mockClear();
    dataRefresh.mockClear();
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    // open() 里 this.remoteSection?.refresh() / this.dataSection?.refresh()——字段若被漏赋值则静默 no-op
    expect(remoteRefresh).toHaveBeenCalled();
    expect(dataRefresh).toHaveBeenCalled();
  });
});
