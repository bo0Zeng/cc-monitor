// F82b（#56+#47）+ A3：设置面板 4 组终态结构测试。把各重子分区 stub 成占位 div、保留真
// CollapsibleGroup，只钉「buildBody 产出 连接/外观/账号/集成 四组（顺序）+ 账号组含 AccountsSection」。
// 构造 SettingsPanel 不调 open()（配置读取在 open 里；本测只验 buildBody 的静态分组结构）。
import { describe, it, expect, vi } from "vitest";

// refresh spy 守 F82b 段移动没丢 this.remoteSection/this.dataSection 字段（丢了 open() 的
// `?.refresh()` 会静默 no-op）。vi.hoisted 让 spy 在被提升的 vi.mock 工厂里可见。
const { remoteRefresh, dataRefresh, boom, behaviorStub } = vi.hoisted(() => ({
  // P6c：让单条测试能改预设（`vi.mock` 的工厂被提升，引不到普通顶层变量）。
  behaviorStub: { localPresets: [] as string[], remotePresets: [] as string[] },
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
        addMachinePage: (
          id: string,
          title: string,
          el: HTMLElement,
          parts?: { connection: HTMLElement; components: HTMLElement },
        ) => void;
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
  // ⚠ **必须是 `mockImplementation` 不是 `mockResolvedValue`**：后者的对象字面量
  // 只在建 mock 时求值**一次** ⇒ 单条测试后来改 `behaviorStub` 它看不见（实测栽过一次，
  // chips 恒为空）。现取才让「每条测试自带一份预设」这件事成立。
  getBehavior: vi.fn(async () => ({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    showBgSessions: false,
    notifyTurnEnd: false,
    resumeCommandLocal: "",
    resumeCommandRemote: "",
    resumeCommandLocalPresets: behaviorStub.localPresets,
    resumeCommandRemotePresets: behaviorStub.remotePresets,
  })),
  setBehavior: vi.fn().mockResolvedValue(undefined),
  withResumePreset: (list: readonly string[], cmd: string) => {
    const v = cmd.trim();
    if (!v) return [...list];
    return [v, ...list.filter((x) => x !== v)].slice(0, 12);
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));

import { SettingsPanel } from "./panel";
import { __setHostOsForTests } from "./host-os";
import { setBehavior } from "../behavior";
import { beforeEach, afterEach } from "vitest";

// S9：jsdom 的 UA 含 `linux`，非 Windows 上「终端集成」那块**根本不构造**
// ⇒ 不置覆盖值的话，本套隔离测试就少守一块。钉成 windows，守的是完整那组。
beforeEach(() => __setHostOsForTests("windows"));
afterEach(() => __setHostOsForTests(null));

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
    // S6 后顶层是 3 页（cc-bus 已移出设置）。
    expect(pages.length, "三页都该在").toBe(3);
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

// ===== P6c：resume 命令预设（#69 b/c）=====
describe("P6c resume 命令预设", () => {
  beforeEach(() => {
    behaviorStub.localPresets = [];
    behaviorStub.remotePresets = [];
    document.body.textContent = "";
  });
  // 面板自己把 DOM 挂到 document（windowMode）—— 与本文件其余判据同一种查法。
  const chips = (cls: string) =>
    [...document.querySelectorAll<HTMLButtonElement>(`.${cls} .settings-preset`)];

  it("★ P6c-Y1：点一条预设 ⇒ 输入框变成它，并**真的走保存路径**", async () => {
    behaviorStub.localPresets = ["cct", "claude"];
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    void p;
    const list = chips("resume-presets-local");
    expect(list.map((b) => b.textContent)).toEqual(["cct", "claude"]);

    const input = [...document.querySelectorAll<HTMLInputElement>("input")].find(
      (i) => i.placeholder === "默认：检测 cc，回退 claude",
    )!;
    expect(input).toBeTruthy();
    vi.mocked(setBehavior).mockClear();
    list[1].click();
    await new Promise((r) => setTimeout(r, 0));

    expect(input.value).toBe("claude");
    // 只填输入框不保存 ⇒ 用户以为选了、其实没存。**必须钉保存真的发生了。**
    expect(vi.mocked(setBehavior)).toHaveBeenCalled();
    const saved = vi.mocked(setBehavior).mock.calls.at(-1)![0];
    expect(saved.resumeCommandLocal).toBe("claude");
  });

  it("★ P6c-Y3：点**远端**预设要走同一条越层诊断（预设是放大器，不是绕过口）", async () => {
    // 远端输入框的 tooltip 逐字：「别填 cct 这类自己建 tmux 的命令」——
    // 手打错一次是一次，存成预设是把错误**固化成一键**。
    behaviorStub.remotePresets = ["cct"];
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    const warn = document.querySelector<HTMLElement>(".settings-launcher-warning")!;
    expect(warn.style.display).toBe("none"); // 起手没有警告

    chips("resume-presets-remote")[0].click();
    await new Promise((r) => setTimeout(r, 0));
    expect(warn.style.display).toBe("block");
    expect(warn.textContent ?? "").not.toBe("");
  });

  it("★ P6c-D：预设能移除，但**正在生效的那条不给移除**", async () => {
    // 只进不出是个缺陷：打错一个 `ccmm`，它会一直占着位子直到被后来的挤出去。
    // 而移除一条**此刻正在用**的命令是自相矛盾的 —— 保存时它必然又被记回来
    // （`withResumePreset` 会把输入框里的值并进去），用户会看见「点了没反应」。
    behaviorStub.localPresets = ["ccm", "claude"];
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    void p;
    const dels = [
      ...document.querySelectorAll<HTMLButtonElement>(
        ".resume-presets-local .settings-preset-del",
      ),
    ];
    expect(dels).toHaveLength(2);
    expect(dels.every((d) => d.textContent === "×")).toBe(true);

    // 先选中 ccm 让它「正在生效」。
    vi.mocked(setBehavior).mockClear();
    document
      .querySelectorAll<HTMLButtonElement>(".resume-presets-local .settings-preset")[0]
      .click();
    await new Promise((r) => setTimeout(r, 0));
    const after = [
      ...document.querySelectorAll<HTMLButtonElement>(
        ".resume-presets-local .settings-preset-del",
      ),
    ];
    const activeDel = after[0];
    expect(activeDel.disabled, "正在生效的那条不给移除").toBe(true);
    expect(activeDel.title).toContain("正在生效");
    expect(after[1].disabled, "别的那条照常可移除").toBe(false);

    // 移除**不在生效**的那条 ⇒ 真的从落盘的列表里没了。
    vi.mocked(setBehavior).mockClear();
    after[1].click();
    await new Promise((r) => setTimeout(r, 0));
    const saved = vi.mocked(setBehavior).mock.calls.at(-1)![0];
    expect(saved.resumeCommandLocalPresets).not.toContain("claude");
    expect(saved.resumeCommandLocalPresets).toContain("ccm");
  });

  it("★ P6c-Y1b：没有预设时不留空盒子（一排看不见的元素只会挡布局）", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    expect(chips("resume-presets-local")).toHaveLength(0);
    expect(chips("resume-presets-remote")).toHaveLength(0);
  });
});
