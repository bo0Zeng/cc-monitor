// S2（settings-ia）：设置面板**分页**结构测试。把各重子分区 stub 成占位 div、保留真
// `CollapsibleGroup` 与真 `SettingsRouter`，钉住「哪些块在哪一页」。
// 构造 SettingsPanel 不调 open()（配置读取在 open 里；本测只验 buildBody 的静态结构）。
//
// **本文件此前钉的是 F82b 的「连接/外观/账号/集成」四组**。S2 按主计划 §2.1 的判据
// （每个顶层 = 一类被设置的对象）重排成 应用/机器/改动足迹 + 临时的 cc-bus，
// 那四个组名随之消失 —— 所以这里是**跟着功能改**，不是把碍事的断言删掉。
// 新断言比旧的更强：旧的只点名 4 个组 + 抽查几个子分节；新的是**逐页的完整清单**
// ——少一块、多一块、搬错页，三种都红。
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
// A3：账号组子分区 stub，给个可识别的 class 供断言（原「远端」空占位已被本组取代）。
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

/** 某一页里的分节标题清单（含页内折叠组的标题）。 */
function pageTitles(routeId: string): string[] {
  const page = document.querySelector<HTMLElement>(
    `.settings-page[data-route-id="${routeId}"]`,
  );
  if (!page) throw new Error(`page not found: ${routeId}`);
  return [
    ...page.querySelectorAll(
      ".settings-group-title, .settings-collapsible-title",
    ),
  ].map((e) => e.textContent ?? "");
}

function navTitles(): string[] {
  return [...document.querySelectorAll(".settings-nav-item")].map(
    (e) => e.textContent ?? "",
  );
}

describe("S2 设置面板分页结构", () => {
  it("导航 = 应用 / 机器 / 改动足迹 / cc-bus（按序）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    // cc-bus 是**临时**一项：它是运营视图不是设置（主计划 §1-1），S6 会把它移出设置窗。
    // 单列在这里而不是塞进上面任何一页，是为了不误归档；S6 那天删掉它就是删一行注册。
    expect(navTitles()).toEqual(["应用", "机器", "改动足迹", "cc-bus"]);
  });

  it("★ 逐页完整清单 —— 14 个叶子块一个不少、一个不错位", () => {
    // 这是本轮最重要的一条：S2 只搬不改，**搬丢一块 = 一个功能凭空消失**，
    // 而它在 UI 上的表现只是「某个设置项找不到了」，不会报错。
    // 用**完整相等**而不是 `toContain`：后者对「多出一块」和「顺序乱了」都是瞎的。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    expect(pageTitles("app")).toEqual([
      "行为",
      "快捷键",
      "外观", // 折叠组
      "字体",
      "颜色",
      "日志与数据", // 折叠组
      "Claude 数据目录",
      "诊断",
      "数据存储",
    ]);
    // 这四块归「机器」的判据：它们**各自维护着一份 origin 选择器**
    //（主计划 §5-4 点名 accounts/mcp/cc-bus/cc-bus-hooks 四份互不同步）
    // ⇒ 它们改的是某台机器的状态。S4 会把这四份换成「当前在哪台机器页」这个上下文。
    expect(pageTitles("machines")).toEqual([
      "连接（远端）",
      "账号",
      "终端集成",
      "MCP",
      "cc-bus 钩子",
    ]);
    expect(pageTitles("footprint")).toEqual(["配置面审计"]);
    expect(pageTitles("cc-bus")).toEqual(["cc-bus"]);
  });

  it("「账号」块真的挂着 AccountsSection（不是只有个标题）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machines"]',
    );
    expect(page!.querySelector(".accounts-section-stub")).toBeTruthy();
    // F82b 那个「留空占位」组已随 S2 一并消失（它的说明文案也删了，见 panel.ts 注释）。
    expect(document.querySelector(".settings-group-empty")).toBeNull();
  });

  it("★ 落地页是「机器」，且同一时刻只有它可见", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    const visible = [
      ...document.querySelectorAll<HTMLElement>(".settings-page"),
    ].filter((el) => !el.hidden);
    expect(visible.map((el) => el.dataset.routeId)).toEqual(["machines"]);
  });

  it("open() 回落地页（刻意不记忆上次停在哪一页）", async () => {
    document.body.replaceChildren();
    const p = new SettingsPanel({ windowMode: true });
    // 先切走
    [...document.querySelectorAll<HTMLButtonElement>(".settings-nav-item")][0]!.click();
    expect(
      [...document.querySelectorAll<HTMLElement>(".settings-page")]
        .filter((el) => !el.hidden)
        .map((el) => el.dataset.routeId),
    ).toEqual(["app"]);
    await p.open();
    expect(
      [...document.querySelectorAll<HTMLElement>(".settings-page")]
        .filter((el) => !el.hidden)
        .map((el) => el.dataset.routeId),
    ).toEqual(["machines"]);
  });

  it("open() 刷新到 RemoteSection / DataSection（守 this.remoteSection/dataSection 字段未丢）", async () => {
    // S2 把这两个字段的赋值点从「集成组」搬到了别的页，赋值**时机**没变（仍在 build 里）。
    // 这条是那次搬运的护栏：字段若被漏赋值，open() 里的 `?.refresh()` 会静默 no-op。
    document.body.replaceChildren();
    remoteRefresh.mockClear();
    dataRefresh.mockClear();
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    expect(remoteRefresh).toHaveBeenCalled();
    expect(dataRefresh).toHaveBeenCalled();
  });
});
