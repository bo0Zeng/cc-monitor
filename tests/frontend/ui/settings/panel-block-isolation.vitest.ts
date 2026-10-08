// F82b（#56+#47）+ A3：设置面板 4 组终态结构测试。把各重子分区 stub 成占位 div，
// 只钉「buildBody 产出 连接/外观/账号/集成 四组（顺序）+ 账号组含 AccountsSection」。
// 构造 SettingsPanel 不调 open()（配置读取在 open 里；本测只验 buildBody 的静态分组结构）。
import { describe, it, expect, vi } from "vitest";

// refresh spy 守 F82b 段移动没丢 this.remoteSection/this.dataSection 字段（丢了 open() 的
// `?.refresh()` 会静默 no-op）。vi.hoisted 让 spy 在被提升的 vi.mock 工厂里可见。
const { remoteRefresh, dataRefresh, boom, behaviorStub } = vi.hoisted(() => ({
  // P6c：让单条测试能改预设（`vi.mock` 的工厂被提升，引不到普通顶层变量）。
  behaviorStub: { presets: [] as string[] },
  remoteRefresh: vi.fn(),
  dataRefresh: vi.fn(),
  boom: { remote: false, ext: false, kb: false } as {
    remote: boolean;
    ext: boolean;
    kb: boolean;
  },
}));

// —— 重子分区 stub 成 { element }，聚焦分组结构本身 —— //
// 注：vi.mock 工厂被提升到文件顶部、不能引用顶层变量，故每个工厂内联一个占位类。
// **可控抛**：`shouldThrow` 打开时构造抛，用来验"一块坏、别的块还在"。
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  // S4b-2：stub 必须**履行它替身的那份契约** —— 真 RemoteSection 在 refresh 时会调
  // `pages.addMachinePage` 注册本机页，panel 靠那一刻把 per-machine 分节安顿下来。
  // stub 不调的话，那几块就永远游离在文档之外，测试会以为它们「消失了」。
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    headActions = (): HTMLElement[] => [];
    pageIdOfMachine = (): string | null => null;
    menuFor = (): unknown[] => [];
    metaOfPage = (): string | null => null;
    setConnected = (): void => {};
    originOfPage = (): string | null => null;
    isUnconfiguredPage = (): boolean => false;
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
vi.mock("../../../../src/frontend/ui/settings/data-section", () => ({
  DataSection: class {
    element = document.createElement("div");
    refresh = dataRefresh;
  },
}));
vi.mock("../../../../src/frontend/ui/settings/diagnostics-section", () => ({
  DiagnosticsSection: class {
    element = document.createElement("div");
    headButton = () => document.createElement("button");
  },
}));
// `cc_integration.ts` 并进了 `machine-aliases.ts`（终端集成成了「别名」那一块 PowerShell 那一侧），它的替身随之删掉。
vi.mock("../../../../src/frontend/ui/settings/ext-section", () => ({
  ExtSection: class {
    element = document.createElement("div");
    constructor() {
      if (boom.ext) throw new Error("EXT_BOOM");
    }
    loadNow(): void {}
  },
}));
// A3：账号组子分区 stub，给个可识别的 class 供断言（原「远端」空占位已被本组取代）。
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
    constructor() {
      if (boom.kb) throw new Error("KB_BOOM");
    }
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
  // ⚠ **必须是 `mockImplementation` 不是 `mockResolvedValue`**：后者的对象字面量
  // 只在建 mock 时求值**一次** ⇒ 单条测试后来改 `behaviorStub` 它看不见（实测栽过一次，
  // chips 恒为空）。现取才让「每条测试自带一份预设」这件事成立。
  behaviorIn: vi.fn(() => ({
    autoFollowUserActive: false,
    bringMonitorToFrontOnUserActive: false,
    showBgSessions: false,
    notifyTurnEnd: false,
    notifyNeeds: false,
    resumeCommand: "",
    resumeCommandPresets: behaviorStub.presets,
    resumeInTmux: false,
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

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { setBehavior } from "../../../../src/frontend/ui/behavior";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { beforeEach, afterEach } from "vitest";
import { copyPattern } from "../../../test-support/copy-pattern";

// S9：jsdom 的 UA 含 `linux`。从前非 Windows 上「终端集成」那块**根本不构造**，钉成 windows 为的是守完整那组；
// 那块并进了「别名」、两个平台都构造 ⇒ 这一钉只是沿用本文件一直以来的 Windows 形态。
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
    boom.ext = false;
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
    expect(failed!.textContent).toMatch(copyPattern("settingsPanel.safeBlock.failed"));
    expect(failed!.textContent).toContain("REMOTE_BOOM");
    expect(failed!.dataset.failedBlock).toBe(copyText("settingsPanel.group.remote"));
    // 其余块照常出——「每块一个 catch」的真正含义。
    // S2 后判据从「四个折叠组」换成「四页都在」：折叠组只剩两个（外观 / 日志与数据），
    // 而「一块坏不影响其余」这条性质现在体现在**页面结构完整**上。
    const pages = [...document.querySelectorAll(".settings-page")];
    // S6 后顶层是 3 页（cc-bus 已移出设置）；「改动足迹」并进机器页、「应用」下挂三个子页
    // ⇒ 应用 ＋ 外观 / 日志 / 数据位置 ＋ 机器 = 5 页；顶层多一页「扩展」⇒ 6 页。
    expect(pages.length, "六页都该在").toBe(6);
    expect(
      document.querySelector(".accounts-section-stub"),
      "账号块不受影响",
    ).not.toBeNull();
  });

  it("🔴 步 3：RemoteSection 真挂了 ⇒ 兜底态**亮出来**，那几块都还能用", async () => {
    // 兜底态只在**真失败**时出现。这条钉的是「真失败」那一半 ——
    // 加载中那一半（不许提前露脸）由 `panel-groups.vitest.ts` 两条钉。
    // 两条合起来才是一个判别式；只有其中一条时，「永远藏着」和「永远露着」各能蒙混一条。
    boom.remote = true;
    const p = new SettingsPanel({ windowMode: true });
    void p;
    await new Promise((r) => setTimeout(r, 0));
    const page = document.querySelector<HTMLElement>(
      '.settings-page[data-route-id="machines"]',
    )!;
    const slot = page.querySelector<HTMLElement>(".machine-page-sections")!;
    expect(slot.hidden, "机器页一个都注册不出来 ⇒ 这几块只能留在列表页上").toBe(false);
    const hint = page.querySelector<HTMLElement>('[data-fallback="per-machine"]');
    expect(hint, "得说一句「为什么它们在这儿」，不能默默换个位置").not.toBeNull();
    expect(hint!.hasAttribute("aria-busy"), "它不是加载态了，别再说自己在忙").toBe(false);
    expect(hint!.textContent).toBe(copyText("settingsPanel.fallback.body", { why: copyText("settingsPanel.machines.buildFailed") }));
    // 「都还能用」——账号那块的真身还在 DOM 里，不是被兜底提示替掉了。
    expect(document.querySelector(".accounts-section-stub")).not.toBeNull();
    // 后端那几行本该挂在机器列表的行上 —— 列表没建起来 ⇒ 它们也退回列表页，不许无处安放。
    for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
    const backendRows = page.querySelector<HTMLElement>(".backend-section");
    expect(backendRows, "机器列表挂了，后端那几行跟着从界面上消失了").not.toBeNull();
    expect(backendRows!.querySelector('.backend-row[data-origin="<local>"]')).not.toBeNull();
  });

  it("换一块抛（ExtSection）→ 同样只坏那一块", async () => {
    boom.ext = true;
    const p = new SettingsPanel({ windowMode: true });
    void p;
    await new Promise((r) => setTimeout(r, 0));
    expect(document.querySelector(".settings-panel")).not.toBeNull();
    const failed = document.querySelectorAll<HTMLElement>(
      ".settings-block-failed",
    );
    expect(failed.length, "只该坏一块").toBe(1);
    expect(failed[0].textContent).toContain("EXT_BOOM");
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

// ===== P6c：恢复命令（通用页一格，各台的默认）—— 下拉：默认那一家的启动器 · 用过的 · 自定义… =====
describe("P6c 恢复命令下拉", () => {
  beforeEach(() => {
    behaviorStub.presets = [];
    document.body.textContent = "";
  });
  const sel = () => document.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
  const labels = (): string[] => {
    sel().click();
    const got = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].map((i) => i.textContent ?? "");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    sel().click();
    return got;
  };
  const pick = (label: string): void => {
    sel().click();
    [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => i.textContent === label)!.click();
  };

  it("★ P6c-Y1：候选 ＝ 默认（适配层画像的启动器）· 用过的几条 · 自定义…；选一条 ⇒ **真的走保存路径**并记进用过的", async () => {
    behaviorStub.presets = ["cct", "ccm --tmux"];
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    const got = labels();
    expect(got[0], "默认那一项 ＝ 适配层画像里默认那一家的启动器 ＋ 灰字「默认」").toContain(copyText("resumeSelect.option.defaultBare"));
    expect(got.slice(1)).toEqual(["cct", "ccm --tmux", copyText("resumeSelect.option.custom")]);
    vi.mocked(setBehavior).mockClear();
    pick("ccm --tmux");
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(setBehavior)).toHaveBeenCalled();
    const saved = vi.mocked(setBehavior).mock.calls.at(-1)![0];
    expect(saved.resumeCommand).toBe("ccm --tmux");
    expect(saved.resumeCommandPresets[0]).toBe("ccm --tmux");
  });

  it("★ P6c-Y3：选的那条要走同一条越层诊断（这一格也是远端几台的默认；候选是放大器，不是绕过口）", async () => {
    behaviorStub.presets = ["cct"];
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    const warn = document.querySelector<HTMLElement>(".settings-launcher-warning")!;
    expect(warn.style.display).toBe("none");
    pick("cct");
    await new Promise((r) => setTimeout(r, 0));
    expect(warn.style.display).toBe("block");
    expect(warn.textContent ?? "").not.toBe("");
  });

  it("★ P6c-C：自定义… 展开一格，失焦存成那一条", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    const custom = document.querySelector<HTMLInputElement>("[data-role=resume-custom]")!;
    expect(custom.hidden).toBe(true);
    pick(copyText("resumeSelect.option.custom"));
    expect(custom.hidden).toBe(false);
    vi.mocked(setBehavior).mockClear();
    custom.value = "my-claude";
    custom.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(setBehavior).mock.calls.at(-1)![0].resumeCommand).toBe("my-claude");
    expect(sel().dataset.value).toBe("my-claude");
  });
});

// ===== 通用页：开关拨了马上存；存失败拇指退回 ＋ 行下一句（C4）· 恢复到 tmux 里 · 上下文上限锚点 =====
describe("通用页 · 行为与恢复", () => {
  beforeEach(() => {
    behaviorStub.presets = [];
    document.body.textContent = "";
    vi.mocked(setBehavior).mockReset();
    vi.mocked(setBehavior).mockResolvedValue(undefined);
  });
  /** 名字是 `label` 的那个开关（拇指）与它那一行。 */
  const sw = (label: string): { thumb: HTMLButtonElement; row: HTMLElement } => {
    const l = [...document.querySelectorAll<HTMLLabelElement>(".settings-switch-row label")].find(
      (x) => x.querySelector("span")?.firstChild?.textContent === label,
    );
    expect(l, `找不到开关「${label}」—— 下面是空真`).toBeTruthy();
    return { thumb: l!.querySelector<HTMLButtonElement>("[role=switch]")!, row: l!.closest<HTMLElement>(".settings-switch-row")! };
  };

  it("★ 拨「恢复到 tmux 里」⇒ 存的那一份 resumeInTmux 翻过来，别的格原样", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    const { thumb } = sw(copyText("settingsPanel.behavior.resumeInTmux"));
    expect(thumb.getAttribute("aria-checked")).toBe("false");
    thumb.click();
    await new Promise((r) => setTimeout(r, 0));
    const saved = vi.mocked(setBehavior).mock.calls.at(-1)![0];
    expect(saved.resumeInTmux).toBe(true);
    expect(saved.notifyNeeds).toBe(false);
  });

  it("★ 存失败 ⇒ 拇指退回原位 ＋ 那一行下面说为什么（不弹会消失的 toast 了事）", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    vi.mocked(setBehavior).mockRejectedValueOnce(new Error("盘满了-xyz"));
    const { thumb, row } = sw(copyText("settingsPanel.behavior.notifyNeeds"));
    thumb.click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(thumb.getAttribute("aria-checked"), "存失败了拇指还停在新位置").toBe("false");
    const err = row.querySelector<HTMLElement>(".settings-row-error")!;
    expect(err.hidden).toBe(false);
    expect(err.textContent).toContain("盘满了-xyz");
  });

  it("「同时提到前台」挂在上一项下：上一项关着时禁用", async () => {
    const p = new SettingsPanel({ windowMode: true });
    await p.open();
    // 这一份桩里 autoFollowUserActive 是 false。
    expect(sw(copyText("settingsPanel.behavior.autoFront")).thumb.getAttribute("aria-disabled")).toBe("true");
    sw(copyText("settingsPanel.behavior.autoFollow")).thumb.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(sw(copyText("settingsPanel.behavior.autoFront")).thumb.getAttribute("aria-disabled")).toBe("false");
  });

  it("上下文上限那一节挂锚点 context-limits（主窗口［设上限］跳过来）", () => {
    new SettingsPanel({ windowMode: true });
    const spot = document.querySelector<HTMLElement>('[data-anchor="context-limits"]')!;
    expect(spot).toBeTruthy();
    // 跳过来那一下展开折着的那一节。
    expect(spot.querySelector("[aria-expanded]")!.getAttribute("aria-expanded")).toBe("false");
    spot.dispatchEvent(new Event("settings-reveal"));
    expect(spot.querySelector("[aria-expanded]")!.getAttribute("aria-expanded")).toBe("true");
  });
});
