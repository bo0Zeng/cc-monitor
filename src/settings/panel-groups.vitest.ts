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
  // S4b-2：stub 必须**履行它替身的那份契约** —— 真 RemoteSection 在 refresh 时会调
  // `pages.addMachinePage` 注册本机页，panel 靠那一刻把 per-machine 分节安顿下来。
  // stub 不调的话，那几块就永远游离在文档之外，测试会以为它们「消失了」。
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = remoteRefresh;
    constructor(opts?: {
      pages?: {
        addMachinePage: (
          id: string,
          title: string,
          el: HTMLElement,
          parts?: { connection: HTMLElement; components: HTMLElement },
        ) => void;
      };
    }) {
      
      // **延后注册**：真 RemoteSection 是在异步 `refresh()` 里注册页的，
      // 所以本机页排在 buildBody 注册的那几个主路由**之后**。同步注册会让它抢在
      // 「应用/机器/…」前面成为第一页，落地页与导航顺序就都错了。
      setTimeout(() => {
        const page = document.createElement("div");
        opts?.pages?.addMachinePage("machine:（本机）", "本机", page);
        // 再注册一台**远端**机器页，带 parts —— 只有带 parts 的页才会被拆成四栏。
        const conn = document.createElement("div");
        conn.textContent = "CONN";
        const comp = document.createElement("div");
        comp.textContent = "COMP";
        opts?.pages?.addMachinePage("machine:aya", "aya", document.createElement("div"), {
          connection: conn,
          components: comp,
        });
      }, 0);
    }
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

  /** 等 RemoteSection 那边异步注册完本机页（真实实现是在 `refresh()` 里注册的）。 */
  const tick = () => new Promise((r) => setTimeout(r, 0));

  it("★ 逐页完整清单 —— 14 个叶子块一个不少、一个不错位", async () => {
    // 这是本轮最重要的一条：S2 只搬不改，**搬丢一块 = 一个功能凭空消失**，
    // 而它在 UI 上的表现只是「某个设置项找不到了」，不会报错。
    // 用**完整相等**而不是 `toContain`：后者对「多出一块」和「顺序乱了」都是瞎的。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
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
    // ★ S4b-2：这四块**已从列表页搬到机器详情页**。「机器」这一页现在只剩机器列表本身。
    expect(pageTitles("machines")).toEqual(["连接（远端）"]);
    // 它们跟着「当前在看哪台机器」走；初始落在本机页上（与 machine-context 的初始值对齐）。
    expect(pageTitles("machine:（本机）")).toEqual([
      "账号",
      "终端集成",
      "MCP",
      "cc-bus 钩子",
    ]);
    expect(pageTitles("footprint")).toEqual(["配置面审计"]);
    expect(pageTitles("cc-bus")).toEqual(["cc-bus"]);
  });

  it("「账号」块真的挂着 AccountsSection（不是只有个标题）", async () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:（本机）"]',
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

  it("★ 本机页上不出现只对远端有意义的块（S4a 那个半截状态的解药）", async () => {
    // S4a 时三块的下拉只列远端、表示不了本机，收到 null 只能原地不动。
    // 现在本机页上它们压根不显示，「表示不了」这件事也就不存在了。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    const local = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:（本机）"]',
    )!;
    const visibleTitles = [...local.querySelectorAll<HTMLElement>(".settings-group")]
      .filter((g) => !g.hidden)
      .map((g) => g.querySelector(".settings-group-title")?.textContent ?? "");
    // 「账号」是 per-origin 的远端概念 ⇒ 本机页上隐藏；
    // 「终端集成」（PowerShell $PROFILE）只对本机有意义 ⇒ 显示。
    expect(visibleTitles).toContain("终端集成");
    expect(visibleTitles).not.toContain("账号");
    // MCP / cc-bus 钩子两边都有意义
    expect(visibleTitles).toContain("MCP");
    expect(visibleTitles).toContain("cc-bus 钩子");
  });

  it("★ RemoteSection 挂掉时那四块仍在（隔离不能因为它们依赖机器页而被打破）", async () => {
    // 审计时真造它抛才发现的：没有机器页 ⇒ slot 无处安放 ⇒ 四块一起消失，
    // 那就是「一块坏，五块没」。兜底落点让最坏情况只是它们留在列表页上。
    // 这里用「本机页没注册」来代表那个场景（stub 不注册 = RemoteSection 没跑起来）。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    // **不等** tick：此刻本机页还没注册，等价于 RemoteSection 挂掉的处境。
    expect(pageTitles("machines")).toEqual([
      "连接（远端）",
      "账号",
      "终端集成",
      "MCP",
      "cc-bus 钩子",
    ]);
  });

  it("★ 远端机器页拆成横向四栏（连接/组件/账号/工具），本机页不拆", async () => {
    // 分栏复用 SettingsRouter（横向 + 无页头），不另造 tab 原语。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    // stub 只注册本机页（没有卡片、不带 parts）⇒ 它**不该**被拆栏。
    const local = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:（本机）"]',
    )!;
    expect(local.querySelector(".settings-shell-h")).toBeNull();

    // 远端机器页**必须**拆成四栏，且顺序是 连接/组件/账号/工具。
    const remote = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:aya"]',
    )!;
    const strip = remote.querySelector<HTMLElement>(".settings-shell-h");
    expect(strip, "远端机器页必须分栏").not.toBeNull();
    expect(
      [...strip!.querySelectorAll(".settings-nav-item")].map((b) => b.textContent),
    ).toEqual(["连接", "组件", "账号", "工具"]);
    // 「连接」是落地栏，同一时刻只有它可见
    const visible = [...strip!.querySelectorAll<HTMLElement>(".settings-page")].filter(
      (e) => !e.hidden,
    );
    expect(visible).toHaveLength(1);
    expect(visible[0]!.textContent).toContain("CONN");
  });

  it("★ 切到远端机器页 → 那几块分节各自落进「账号 / 工具」栏", async () => {
    // 分栏若不接线，它们会退回「整块搬到页面底部」，四栏就成了空壳。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    // 点导航里的 aya 进那一页
    const ayaNav = [...document.querySelectorAll<HTMLButtonElement>(".settings-nav-item")]
      .find((b) => b.textContent === "aya")!;
    ayaNav.click();

    const strip = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:aya"] .settings-shell-h',
    )!;
    const tabPage = (id: string) =>
      strip.querySelector<HTMLElement>(`.settings-page[data-route-id="machine:aya#${id}"]`)!;
    // 账号块进「账号」栏
    expect(tabPage("acct").querySelector(".accounts-section-stub")).toBeTruthy();
    // MCP / cc-bus 钩子 / 终端集成 进「工具」栏
    const toolTitles = [...tabPage("tools").querySelectorAll(".settings-group-title")].map(
      (e) => e.textContent,
    );
    expect(toolTitles).toContain("MCP");
    expect(toolTitles).toContain("cc-bus 钩子");
    // 反向：账号**不该**也出现在工具栏里（搬 DOM 一处一份，不能有两份）
    expect(toolTitles).not.toContain("账号");
  });
});
