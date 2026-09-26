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
const { remoteRefresh, dataRefresh, dataLoadNow } = vi.hoisted(() => ({
  // ⚠ 必须**真的回一个 Promise**：`RemoteSection.refresh()` 的签名是 `Promise<void>`，
  //   而 `panel.open()` 在它上面接了 `.catch()`（步 4·D：失败要落在那一块上，
  //   不许再多产一条走状态栏的未捕获 rejection）。回 `undefined` 的 stub 会让
  //   `open()` 当场 TypeError —— 那不是生产代码的 bug，是 stub 没履行它替身的契约。
  remoteRefresh: vi.fn().mockResolvedValue(undefined),
  dataRefresh: vi.fn(),
  dataLoadNow: vi.fn(),
  // 〔AL1c · 4B〕这里原来还有一个 `ccIntegrationBuilds` 计数（S9：数「终端集成」那块**构造**了几次）。
  // 那块并进了「别名」（`设计/71 §7` W5），「构造即发 Windows 专用 IPC」那条纪律跟着过去、
  // 改由 `machine-aliases.vitest.ts` 的「构造零 I/O」两平台各钉一遍 ⇒ 计数与替身一起走了。
}));

// —— 重子分区 stub 成 { element }，聚焦分组结构本身 —— //
// 注：vi.mock 工厂被提升到文件顶部、不能引用顶层变量，故每个工厂内联一个占位类。
vi.mock("../../src/settings/remote-section", () => ({
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
          parts?: { connection: HTMLElement; components: HTMLElement; tools: HTMLElement },
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
        // 〔ST2〕第三块：「工具」栏里那台机器自己的别名（与本机页同一个位置）。
        const tools = document.createElement("div");
        tools.textContent = "TOOLS";
        tools.id = "stub-remote-aliases";
        opts?.pages?.addMachinePage("machine:devbox", "devbox", document.createElement("div"), {
          connection: conn,
          components: comp,
          tools,
        });
      }, 0);
    }
  },
}));
vi.mock("../../src/settings/data-section", () => ({
  DataSection: class {
    element = Object.assign(document.createElement("div"), { id: "stub-data-section" });
    refresh = dataRefresh;
    // `设计/70 §1.3 B`（步 2）：真 DataSection 的第一发 I/O 由宿主在
    // 「这一页首次可见」时通过 `loadNow()` 放行 —— stub 必须履行同一份契约，
    // 否则这条护栏守的是一个盘上不存在的接口。
    loadNow = dataLoadNow;
  },
}));
vi.mock("../../src/settings/diagnostics-section", () => ({
  DiagnosticsSection: class {
    element = Object.assign(document.createElement("div"), { id: "stub-diagnostics-section" });
    loadNow = vi.fn();
  },
}));
vi.mock("../../src/settings/mcp-section", () => ({
  McpSection: class {
    element = document.createElement("div");
  },
}));
// A3：账号组子分区 stub，给个可识别的 class 供断言（原「远端」空占位已被本组取代）。
vi.mock("../../src/settings/accounts-section", () => ({
  AccountsSection: class {
    element = (() => {
      const d = document.createElement("div");
      d.className = "accounts-section-stub";
      return d;
    })();
  },
}));
vi.mock("../../src/keybindings/editor", () => ({
  KeybindingsEditor: class {
    element = document.createElement("div");
  },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: vi.fn(),
    popOverlay: vi.fn(),
    startRecording: vi.fn(),
    cancelRecording: vi.fn(),
    exportOverrides: vi.fn().mockReturnValue({}),
    applyOverrides: vi.fn(),
  },
}));
vi.mock("../../src/theme", () => ({
  applyTheme: vi.fn(),
  applyThemeToken: vi.fn(),
  loadTheme: vi.fn().mockResolvedValue({}),
  saveTheme: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../src/paths", () => ({
  getClaudeDirOverride: vi.fn().mockResolvedValue(""),
  setClaudeDirOverride: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../src/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    showBgSessions: false,
    notifyTurnEnd: false,
    resumeCommandLocal: "",
    resumeCommandRemote: "",
    resumeCommandLocalPresets: [],
    resumeCommandRemotePresets: [],
  }),
  setBehavior: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "../../src/settings/panel";
import { __setHostOsForTests } from "../../src/settings/host-os";
import { beforeEach, afterEach } from "vitest";

// S9：jsdom 的 UA 含 `linux` ⇒ 不置覆盖值，本文件整套跑的就是「非 Windows」那条分支，
// 而下面这些断言本来是照 Windows 形态写的。
// **显式钉成 windows**，Linux 那条形态由本文件末尾专门的一节覆盖。
beforeEach(() => {
  __setHostOsForTests("windows");
});
afterEach(() => __setHostOsForTests(null));

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
  it("导航 = 应用 / 机器（按序）", () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    // S6 已把 cc-bus 驾驶舱移出设置（它是运营视图不是设置，§1-1）。
    // 它现在的入口是命令面板（不加第 7 个顶栏图标，理由见 views/cc-bus-view.ts 头注）。
    // 🔴 〔ST2 · 用户 09-24 裁「并进机器页，删掉顶层页」〕「改动足迹」顶层页没了 ⇒ 顶层只剩两个。
    // 〔ST2 · `70 §6` #3 · 步 15〕「应用」下挂三个子页（替掉原来的两个折叠组）。
    expect(navTitles()).toEqual(["应用", "外观", "日志", "数据位置", "机器"]);
  });

  /** 等 RemoteSection 那边异步注册完本机页（真实实现是在 `refresh()` 里注册的）。 */
  const tick = () => new Promise((r) => setTimeout(r, 0));

  it("★ 逐页完整清单 —— 17 个叶子块一个不少、一个不错位", async () => { // P8a +1（插件（marketplace））；〔AL1〕+1（别名）；〔AL1c〕−1（终端集成并进别名）
    // 这是本轮最重要的一条：S2 只搬不改，**搬丢一块 = 一个功能凭空消失**，
    // 而它在 UI 上的表现只是「某个设置项找不到了」，不会报错。
    // 用**完整相等**而不是 `toContain`：后者对「多出一块」和「顺序乱了」都是瞎的。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    // 🔴 〔ST2 · `70 §6` #3 · 步 15〕两个折叠组（外观 · 日志与数据）换成「应用」下的三个子页。
    //   「日志」「数据位置」那两块各自独占一页 ⇒ 块不再自带标题（页头就是它的名字，§8 #11 不重名）。
    expect(pageTitles("app")).toEqual(["行为", "快捷键"]);
    expect(pageTitles("app-appearance")).toEqual(["字体", "颜色"]);
    // `70 §10.3`：「诊断」**让名**给 `§5.3` 那个改名（否则面板里会有两个「诊断」）——今天是页名。
    expect(pageTitles("app-logs")).toEqual([]);
    expect(pageTitles("app-data")).toEqual(["Claude 数据目录"]);
    // 那两块真的在它们各自那一页上（只是不带块标题）。
    for (const [id, cls] of [
      ["app-logs", "#stub-diagnostics-section"],
      ["app-data", "#stub-data-section"],
    ] as const) {
      const page = document.querySelector<HTMLElement>(`.settings-page[data-route-id="${id}"]`)!;
      expect(page.querySelector(cls), `${id} 页上没有它那一块`).not.toBeNull();
    }
    // ★ S4b-2：那四块**已从列表页搬到机器详情页**。
    // 🔴 〔第四波 ST2 · `70 §5.3` · 步 14〕「backend 开关」**不再单独占一块**：它并进了机器列表那一行
    //   （四格挂在「远端连接」那块的列表行上，钉在 `machine-list-backend-cells.vitest.ts`）。
    //   ⇒ 列表页的块只剩一块（列表 ＋ 添加 ＋ 全局开关 ＋ 诊断都在它里面，`§8` #10）。
    // 〔ST2〕顶层「改动足迹」删掉之后，漂移记账那一块并进**每台机器子页的「足迹」栏**（见下面本机页那张表）。
    expect(pageTitles("machines")).toEqual(["远端连接"]);
    // 它们跟着「当前在看哪台机器」走；初始落在本机页上（与 machine-context 的初始值对齐）。
    expect(pageTitles("machine:（本机）")).toEqual([
      "账号",
      // 〔AL1c · 4B〕「终端集成」并进了下面「别名」那一块（Windows 上它是 PowerShell 那一侧的别名块）。
      // 〔AL1 · 2026-09-24〕`设计/71 §13` ②：别名并进机器页（`70 §3.3`），从「应用 → 行为」搬来。
      "别名",
      "MCP",
      // 〔AS2 · 4B〕资产目录：别的机器有、这台没有的 skill / MCP，装要你点（本机与远端页都有）。
      "资产目录",
      // P8a：marketplace 只读枚举。**只在本机页**——远端今天没有这条口
      // （欠账记在 `parity_ledger::plugins.marketplaces`），挂到远端就是个恒失败的块。
      "插件（marketplace）",
      "cc-bus 钩子",
      // 🔴 `70 §10.1`（步 14a）：「足迹」从顶层「改动足迹」页搬进来，是**新增的第五块**。
      "足迹",
      // 〔ST2〕原顶层「改动足迹」页剩下的那一块，同栏。
      "未识别的数据",
    ]);
    // `70 §10.1`（步 14a）：「配置面审计」→ 改名「足迹」并搬进机器子页。
    // 🔴 〔ST2〕`§10.5` #1 用户裁了：顶层「改动足迹」页**删掉**，剩下那一块并进机器页。
    expect(document.querySelector('.settings-page[data-route-id="footprint"]')).toBeNull();
    // cc-bus 已不在设置里（S6）—— 连页都不该存在。
    expect(
      document.querySelector('.settings-page[data-route-id="cc-bus"]'),
    ).toBeNull();
  });

  /**
   * 🔴 〔AL1 · 2026-09-24〕**别名那一块挂在机器页「本机」上，「应用」页上不再有它。**
   *
   * ⚠ 它买的是**接线**，不是那一块的行为（后者归 `machine-aliases.vitest.ts`）——
   * 那一块的单测直接 `buildAliasManager()`，结构上绕过了「它有没有被挂上去」。
   * 两向：本机页上**有**（`.machine-aliases`），「应用」页上**没有**任何一块别名
   * （从前那两块的类名 `.ccm-acct-alias` / `.ccm-alias-gen` 一个都不许剩在那一页）。
   */
  it("★ AL1：本机页挂着「别名」，「应用」页上一块别名都没有", async () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    await tick();
    const app = document.querySelector<HTMLElement>('.settings-page[data-route-id="app"]');
    expect(app, "「应用」页不在 —— 这条断言量错了地方").toBeTruthy();
    expect(app!.querySelector(".ccm-acct-alias, .ccm-alias-gen, .machine-aliases")).toBeNull();
    const local = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:（本机）"]',
    );
    const block = local!.querySelector<HTMLElement>(".machine-aliases");
    expect(block, "本机页上找不到「别名」那一块").toBeTruthy();
    // ⚠ 「在 DOM 里」不等于「看得见」：per-machine 那几块是**单例 ＋ 按页 hidden**，
    //   `appliesTo` 写错的那一块照样躺在本机页的 DOM 里，只是被藏起来了（死值验现打）。
    for (let n: HTMLElement | null = block; n && n !== local; n = n.parentElement) {
      expect(n.hidden, "「别名」那一块在本机页上被藏起来了（appliesTo 写错？）").toBe(false);
    }
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
    dataLoadNow.mockClear();
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    expect(remoteRefresh).toHaveBeenCalled();
    // 🔴 步 2（`70 §1.3 B`）：`open()` **不再**无条件 `dataSection.refresh()` ——
    // 那一发在落地页是「机器」的时候是白发的（`§8` 判据 #3 今天正是被它这一族打破的）。
    // 字段还在、契约还在，只是放行的时机换成了「这一页首次可见」。
    expect(dataRefresh, "落地页是「机器」⇒ 打开设置不许碰「应用」页的 I/O").not.toHaveBeenCalled();
    expect(dataLoadNow, "还没点进「应用」⇒ 连第一发都不许放").not.toHaveBeenCalled();
    // 点进「数据位置」——这一刻才放行。**相等断言的反向锚**：上面那两条若因为
    // 字段被漏赋值（`this.dataSection` 是 undefined）而绿，这一条会红。
    // 〔ST2 · 步 15〕它今天是「应用」下的子页（原来在「应用」页的折叠组里）。
    document.querySelector<HTMLButtonElement>("#settings-tab-app")!.click();
    expect(dataLoadNow, "点「应用」本身不该放数据位置那一发").not.toHaveBeenCalled();
    document.querySelector<HTMLButtonElement>("#settings-tab-app-data")!.click();
    expect(dataLoadNow, "点进「数据位置」之后第一发必须真的放出去").toHaveBeenCalled();
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
    // 「别名」（〔AL1c〕含从前的「终端集成」，PowerShell $PROFILE 那一块）只对本机有意义 ⇒ 显示。
    expect(visibleTitles).toContain("别名");
    // 🔴 **`N-F1b`（09-05）改了这一格的事实，PM 落**。
    //
    // 旧断言逐字：`expect(visibleTitles).not.toContain("账号");`
    // 旧理由逐字：「「账号」是 per-origin 的远端概念 ⇒ 本机页上隐藏」。
    //
    // 那句话今天是假的：账号**在本机也有**（`local_accounts.rs` 的读口自 `a354c83` 就在，
    // 状态栏那个账号标签一直在渲染本机账号），`N-F1b` 把设置面板那一节也接上了本机那条路，
    // 并把它的 `appliesTo` 由 `"remote"` 改成 `"both"`。
    //
    // ⚠ **本条的题目没变**（「本机页上不出现只对远端有意义的块」仍然要守），
    // 变的只是**「账号」不再属于那个集合**；它守的另外三格一个字没动。
    //
    // ⚠⚠ 为什么这一行必须跟着改，而不是「两条判据打架、权衡一下」——
    // 它与 `panel-machine-page-visibility.vitest.ts` 那条断的是**同一个比特的两个相反符号**：
    // `appliesTo: "remote"` 时本条绿、那条红；改成 `"both"` 时反过来。
    // **任何时刻恰好一绿一红，两条不可能同时绿**（`N-F1b` 的 `NbM5` 实打过）。
    // ⇒ 同一件事实写在两处，本件改了那件事实 ⇒ 两处一起改，不是二选一。
    expect(visibleTitles).toContain("账号");
    // MCP / cc-bus 钩子两边都有意义
    expect(visibleTitles).toContain("MCP");
    expect(visibleTitles).toContain("cc-bus 钩子");
  });

  it("🔴 步 3：机器页还没注册上来时，列表页上是**骨架**，不是兜底态", async () => {
    // 🔴 这条**换了判据，不是放宽了判据**。
    //
    // 旧版逐字写着「不等 tick：此刻本机页还没注册，**等价于 RemoteSection 挂掉的处境**」
    // —— 那句「等价」正是 `设计/70 §1.1` 判掉的那个错：**加载中**与**真失败**在屏幕上
    // 本来就不该等价。而因为机器列表是异步加载的，那个「RemoteSection 抛异常时的最坏
    // 情况」变成了**每次打开的前 3 秒的默认视图**（用户截图 1 里那一屏就是它）。
    //
    // ⇒ 现在：没注册上来 = 加载中 ⇒ slot 藏着、屏上是骨架；
    //   真失败那一档由 `panel-block-isolation.vitest.ts` 那条（**真让 RemoteSection 抛**）钉。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machines"]',
    )!;
    const slot = page.querySelector<HTMLElement>(".machine-page-sections")!;
    expect(slot.hidden, "加载中不许把兜底态摆出来").toBe(true);
    const sk = page.querySelector<HTMLElement>("[data-skeleton]");
    expect(sk, "加载中要有骨架（不是空的，也不是兜底态）").not.toBeNull();
    expect(sk!.getAttribute("aria-busy")).toBe("true");
    // 隔离没有因此被打破：那几块**都还在 DOM 里**，只是先藏着、等机器页来了就搬走。
    expect(pageTitles("machines")).toEqual([
      "远端连接",
      "账号",
      "别名", // 〔AL1〕本机那一格的 ②，跟着 per-machine 那几块一起留在兜底落点（〔AL1c〕终端集成并进了它）
      "MCP",
      "资产目录", // 〔AS2〕
      "插件（marketplace）",
      "cc-bus 钩子",
      "足迹",
      "未识别的数据",
    ]);
  });

  it("🔴 步 3 的另一半：机器页注册上来了 ⇒ 骨架撤掉、slot 不再藏着", async () => {
    // 反向锚：上一条若因为「slot 永远藏着」而绿，这一条会红。
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    const slot = document.querySelector<HTMLElement>(".machine-page-sections")!;
    expect(slot.hidden).toBe(false);
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machines"]',
    )!;
    const sk = page.querySelector<HTMLElement>("[data-skeleton]");
    expect(sk?.hidden ?? true, "机器页来了，列表页上那块骨架就该收起来").toBe(true);
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
      '.settings-page[data-route-id="machine:devbox"]',
    )!;
    const strip = remote.querySelector<HTMLElement>(".settings-shell-h");
    expect(strip, "远端机器页必须分栏").not.toBeNull();
    expect(
      [...strip!.querySelectorAll(".settings-nav-item")].map((b) => b.textContent),
    ).toEqual(["连接", "组件", "账号", "工具", "足迹"]);
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
    // 点导航里的 devbox 进那一页
    const ayaNav = [...document.querySelectorAll<HTMLButtonElement>(".settings-nav-item")]
      .find((b) => b.textContent === "devbox")!;
    ayaNav.click();

    const strip = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:devbox"] .settings-shell-h',
    )!;
    const tabPage = (id: string) =>
      strip.querySelector<HTMLElement>(`.settings-page[data-route-id="machine:devbox#${id}"]`)!;
    // 账号块进「账号」栏
    expect(tabPage("acct").querySelector(".accounts-section-stub")).toBeTruthy();
    // MCP / cc-bus 钩子 进「工具」栏（〔AL1c〕终端集成并进了「别名」）
    const toolTitles = [...tabPage("tools").querySelectorAll(".settings-group-title")].map(
      (e) => e.textContent,
    );
    expect(toolTitles).toContain("MCP");
    expect(toolTitles).toContain("cc-bus 钩子");
    // 反向：账号**不该**也出现在工具栏里（搬 DOM 一处一份，不能有两份）
    expect(toolTitles).not.toContain("账号");
    // 🔴 〔ST2 · 协调方转主会话裁：别名统一放「工具」栏〕这台机器自己的别名在「工具」栏**最前面**，
    //   不在「组件」栏 —— 与本机页「工具 → 别名」同一个位置。
    const tools = tabPage("tools");
    expect(tools.querySelector("#stub-remote-aliases"), "远端的别名不在「工具」栏").not.toBeNull();
    expect(tabPage("comp").querySelector("#stub-remote-aliases")).toBeNull();
    const stub = tools.querySelector<HTMLElement>("#stub-remote-aliases")!;
    expect(stub.parentElement!.firstElementChild, "别名不在「工具」栏最前面").toBe(stub);
    // 本机那一格：「工具」里也有「别名」—— 两边同一个位置。
    document.querySelector<HTMLButtonElement>("#settings-tab-machine\\:（本机）")!.click();
    const localTitles = [
      ...document.querySelectorAll('.settings-page[data-route-id="machine:（本机）"] .settings-group-title'),
    ].map((e) => e.textContent);
    expect(localTitles).toContain("别名");
  });

  it("★ S7：没有待生效改动时，重启条不出现（恒显示的警告 = 背景噪音）", async () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    const bar = document.querySelector<HTMLElement>(".settings-restart-bar");
    expect(bar, "条本身要在 DOM 里（只是隐藏），否则状态变了没地方显示").not.toBeNull();
    expect(bar!.hidden).toBe(true);
  });

  it("★ S7：重启条挂在面板底部、不属于任何一页（改哪一页都可能点亮它）", async () => {
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    const bar = document.querySelector<HTMLElement>(".settings-restart-bar")!;
    // 不在任何 .settings-page 里
    expect(bar.closest(".settings-page")).toBeNull();
    // 也不在导航里
    expect(bar.closest(".settings-nav")).toBeNull();
  });
});

/**
 * S9：本机页上的「终端集成」按 OS 显隐 —— 〔AL1c · 第四波 4B〕那块并进了「别名」（`设计/71 §7` W5），
 * 门从「建不建那一块」变成「别名那一块用哪个平台」：Windows ⇒ PowerShell（含终端集成）；其余 ⇒ POSIX。
 * 「构造即发 Windows 专用 IPC」那条纪律跟着搬过去了：`machine-aliases.vitest.ts` 两个平台各钉「构造零 I/O」。
 *
 * v3.4.0 已经发了 `.deb` ⇒ Linux 用户不该在本机页看到一个装 PowerShell profile 的安装器。
 */
describe("S9 本机 OS 门（〔AL1c〕别名那一块的平台）", () => {
  const tick = () => new Promise((r) => setTimeout(r, 0));

  /** 本机页「别名」那一块（`buildAliasManager` 的根）。 */
  function aliasBlock(): HTMLElement {
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:（本机）"]',
    );
    if (!page) throw new Error("本机页不在");
    const b = page.querySelector<HTMLElement>(".machine-aliases");
    if (!b) throw new Error("本机页上没有「别名」那一块");
    return b;
  }

  /** 本机页上任何一块叫「终端集成」的（并进去之后一块都不该有）。 */
  function terminalBlocks(): number {
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machine:（本机）"]',
    )!;
    return [...page.querySelectorAll(".settings-group-title")].filter(
      (t) => t.textContent === "终端集成",
    ).length;
  }

  for (const [os, shell] of [
    ["windows", "powershell"],
    ["linux", "posix"],
    ["macos", "posix"],
    // ⚠ 认不出 OS ⇒ POSIX。从前「终端集成」在这一格**照常构造**（藏错了 Windows 用户就找不到安装入口），
    //   而「别名」在这一格走 POSIX；两块并成一块只能取一边 ⇒ 取别名那一边（不在非 Windows 上写 `$PROFILE`）。
    //   这一格的取舍报给主会话（`调研/第四波记录/AL1c.md §5`）。
    ["unknown", "posix"],
  ] as const) {
    it(`★ ${os} ⇒ 别名那一块是 ${shell}；单独的「终端集成」一块不再有`, async () => {
      __setHostOsForTests(os);
      document.body.replaceChildren();
      new SettingsPanel({ windowMode: true });
      await tick();
      expect(aliasBlock().dataset.shell).toBe(shell);
      expect(terminalBlocks()).toBe(0);
    });
  }

  it("门只管这一块 —— 同栏的 MCP / cc-bus 钩子在 Linux 上照常在", async () => {
    __setHostOsForTests("linux");
    document.body.replaceChildren();
    new SettingsPanel({ windowMode: true });
    await tick();
    const titles = [
      ...document
        .querySelector<HTMLElement>(
          '.settings-page[data-route-id="machine:（本机）"]',
        )!
        .querySelectorAll(".settings-group-title"),
    ].map((e) => e.textContent);
    expect(titles).toContain("MCP");
    expect(titles).toContain("cc-bus 钩子");
  });
});
