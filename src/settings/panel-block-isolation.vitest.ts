// F82b（#56+#47）+ A3：设置面板 4 组终态结构测试。把各重子分区 stub 成占位 div、保留真
// CollapsibleGroup，只钉「buildBody 产出 连接/外观/账号/集成 四组（顺序）+ 账号组含 AccountsSection」。
// 构造 SettingsPanel 不调 open()（配置读取在 open 里；本测只验 buildBody 的静态分组结构）。
import { describe, it, expect, vi } from "vitest";

// refresh spy 守 F82b 段移动没丢 this.remoteSection/this.dataSection 字段（丢了 open() 的
// `?.refresh()` 会静默 no-op）。vi.hoisted 让 spy 在被提升的 vi.mock 工厂里可见。
const { remoteRefresh, dataRefresh, boom } = vi.hoisted(() => ({
  remoteRefresh: vi.fn(),
  dataRefresh: vi.fn(),
  boom: { remote: false, mcp: false, kb: false } as {
    remote: boolean;
    mcp: boolean;
    kb: boolean;
  },
}));

// —— 重子分区 stub 成 { element }，聚焦分组结构本身 —— //
// 注：vi.mock 工厂被提升到文件顶部、不能引用顶层变量，故每个工厂内联一个占位类。
// **可控抛**：`shouldThrow` 打开时构造抛，用来验"一块坏、别的块还在"。
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
        addMachinePage: (id: string, title: string, el: HTMLElement) => void;
      };
    }) {
      if (boom.remote) throw new Error("REMOTE_BOOM");
      // **延后注册**：真 RemoteSection 是在异步 `refresh()` 里注册页的，
      // 所以本机页排在 buildBody 注册的那几个主路由**之后**。同步注册会让它抢在
      // 「应用/机器/…」前面成为第一页，落地页与导航顺序就都错了。
      setTimeout(() => {
        const page = document.createElement("div");
        opts?.pages?.addMachinePage("machine:（本机）", "本机", page);
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
    constructor() {
      if (boom.mcp) throw new Error("MCP_BOOM");
    }
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
    constructor() {
      if (boom.kb) throw new Error("KB_BOOM");
    }
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
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));

import { SettingsPanel } from "./panel";
import { beforeEach } from "vitest";

// **T07 审计阻塞 2：这些是行为测试，替掉原先那 4 条源码文本扫描。**
//
// 原先那 4 条是安慰剂——审计造了个编译得过的变异（把 `build()` 求值移出 try，
// 围栏彻底失效）：`tsc` 0、`vitest` 全量 **813 全绿**。而计划 §2 的 DoD 原文要求
// 「有测试证明：让某个 section 的构造抛 → 面板**仍然渲染**…（反向自检：去掉 try/catch
// → 该测试必须红）」。那条 DoD 当时**未达成**，而我把它算进了"已做"。
//
// 反验证就用审计那个变异：把 `build()` 求值移出 try → 下面这些必须红。
describe("T07 分区块隔离（真行为）", () => {
  beforeEach(() => {
    boom.remote = false;
    boom.mcp = false;
    boom.kb = false;
    document.body.textContent = "";
  });

  it("RemoteSection 构造抛 → 面板仍然渲染，其余块都在", async () => {
    boom.remote = true;
    const p = new SettingsPanel({ windowMode: true });
    void p;
    await new Promise((r) => setTimeout(r, 0));
    // **这一条就是阻塞①的证据**：修之前 `new SettingsPanel` 直接炸穿、什么都没上屏
    expect(
      document.querySelector(".settings-panel"),
      "面板必须仍然渲染",
    ).not.toBeNull();
    // 失败块就地显示错误 + 可复制原文
    const failed = document.querySelector<HTMLElement>(
      ".settings-block-failed",
    );
    expect(failed, "失败块必须在").not.toBeNull();
    expect(failed!.textContent).toContain("此区块加载失败");
    expect(failed!.textContent).toContain("REMOTE_BOOM");
    expect(failed!.dataset.failedBlock).toBe("连接（远端）");
    // 其余块照常出——「每块一个 catch」的真正含义。
    // S2 后判据从「四个折叠组」换成「四页都在」：折叠组只剩两个（外观 / 日志与数据），
    // 而「一块坏不影响其余」这条性质现在体现在**页面结构完整**上。
    const pages = [...document.querySelectorAll(".settings-page")];
    expect(pages.length, "四页都该在").toBe(4);
    expect(
      document.querySelector(".accounts-section-stub"),
      "账号块不受影响",
    ).not.toBeNull();
  });

  it("换一块抛（McpSection）→ 同样只坏那一块", async () => {
    boom.mcp = true;
    const p = new SettingsPanel({ windowMode: true });
    void p;
    await new Promise((r) => setTimeout(r, 0));
    expect(document.querySelector(".settings-panel")).not.toBeNull();
    const failed = document.querySelectorAll<HTMLElement>(
      ".settings-block-failed",
    );
    expect(failed.length, "只该坏一块").toBe(1);
    expect(failed[0].textContent).toContain("MCP_BOOM");
    expect(document.querySelector(".accounts-section-stub")).not.toBeNull();
  });

  it("没有块抛 → 一个失败块都不该出现（反向自检：别恒显示错误框）", () => {
    const p = new SettingsPanel({ windowMode: true });
    void p;
    expect(document.querySelectorAll(".settings-block-failed").length).toBe(0);
  });

  it("快捷键块失败后 open() 仍要把面板打开（T07 审计⑤：隔离要覆盖整个生命周期）", async () => {
    // 审计实测：修之前这条会 reject、`.settings-panel` 拿不到 `.open`
    boom.kb = true;
    const p = new SettingsPanel({ windowMode: true });
    await expect(p.open()).resolves.toBeUndefined();
    expect(
      document.querySelector(".settings-panel")?.classList.contains("open"),
    ).toBe(true);
  });

  it("RemoteSection 抛之后 open() 不许因为 remoteSection 是 undefined 而炸", async () => {
    boom.remote = true;
    const p = new SettingsPanel({ windowMode: true });
    void p;
    // `open()` 里是 `this.remoteSection?.refresh()`，天然容错——这条钉住它
    await expect(p.open()).resolves.toBeUndefined();
  });
});
