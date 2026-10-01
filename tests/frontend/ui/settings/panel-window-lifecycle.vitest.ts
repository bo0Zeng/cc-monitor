/**
 * ST1「关窗改隐藏」＋「未保存关窗拦截」。
 *
 * 窗口模式下：
 * - 系统 X（close-requested）一律 `preventDefault`，交给面板判；**放行的动作是 `hide()`**，
 *   面板从不调 `close()` / `destroy()`（主窗销毁时由后端把本窗一起收掉 —— `lib.rs` 那条纯函数有自己的测试）。
 * - 保存模型统一成**全即时**：外观 / Claude 数据目录改了就落
 *   （`change`），页脚的「保存」「取消」与「未保存关窗拦截」那一条一起退场；关窗前还没触发 `change`
 *   的那一格由关窗那一下顺手落掉 —— **不静默丢**这件事换了兑现方式，没有丢。
 * - 藏起来之后又拿到焦点（= 被 `open_settings_window` 重新 show）⇒ 重跑 `open()`（重读设置、回落地页）。
 *
 * 〔射程〕jsdom 里没有真窗口：量的是「面板对窗口 API 发了什么」，不是窗口真的藏没藏（那一维要真机）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { win, theme, ipc } = vi.hoisted(() => ({
  ipc: { calls: [] as string[] },
  win: {
    closeRequested: null as null | ((e: { preventDefault: () => void }) => void),
    focus: null as null | ((e: { payload: boolean }) => void),
    hide: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    destroy: vi.fn(() => Promise.resolve()),
  },
  theme: {
    save: vi.fn(() => Promise.resolve()),
    apply: vi.fn(),
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    hide: win.hide,
    close: win.close,
    destroy: win.destroy,
    onCloseRequested: (cb: (e: { preventDefault: () => void }) => void) => {
      win.closeRequested = cb;
      return Promise.resolve(() => {});
    },
    onFocusChanged: (cb: (e: { payload: boolean }) => void) => {
      win.focus = cb;
      return Promise.resolve(() => {});
    },
  }),
}));
vi.mock("../../../../src/frontend/ui/theme", () => ({
  themeIn: () => ({}),
  saveTheme: theme.save,
  applyTheme: theme.apply,
  applyThemeToken: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/paths", () => ({
  claudeDirIn: () => null,
  setClaudeDirOverride: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => () => {
        ipc.calls.push(name);
        return Promise.reject(new Error("[录音机]"));
      },
    },
  ),
}));
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
  },
}));
vi.mock("../../../../src/frontend/ui/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../../../src/frontend/ui/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../../../src/frontend/ui/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); loadNow() {} },
}));
// `cc_integration.ts` 并进了 `machine-aliases.ts`（终端集成成了「别名」那一块 PowerShell 那一侧），它的替身随之删掉。
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
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(), listen: vi.fn() }));

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";

const tick = () => new Promise((r) => setTimeout(r, 0));

async function mount(): Promise<SettingsPanel> {
  const p = new SettingsPanel({ windowMode: true });
  await p.open();
  await tick();
  return p;
}
/** 系统 X：返回它被 preventDefault 了没有。 */
function nativeX(): boolean {
  let prevented = false;
  expect(win.closeRequested, "面板没挂 close-requested —— 系统 X 会直接销毁窗口").toBeTruthy();
  win.closeRequested!({ preventDefault: () => (prevented = true) });
  return prevented;
}
/** 拖一下取色器（`input`：只预览，还没落盘）。 */
function dragColor(): HTMLInputElement {
  const color = document.querySelector<HTMLInputElement>(".settings-panel input[type=color]")!;
  color.value = "#123456";
  color.dispatchEvent(new Event("input"));
  return color;
}
/** 松手（`change`）。 */
function releaseColor(color: HTMLInputElement): void {
  color.dispatchEvent(new Event("change"));
}

describe("ST1：设置窗关窗 ＝ 隐藏；〔ST2〕全即时：改了就落，关窗不拦", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
    win.closeRequested = null;
    win.focus = null;
    for (const f of [win.hide, win.close, win.destroy, theme.save, theme.apply]) f.mockClear();
  });

  it("没改动：系统 X ⇒ preventDefault ＋ hide()；从不 close / destroy", async () => {
    await mount();
    expect(nativeX(), "没拦 ⇒ Tauri 会把窗口销毁（关窗 ≠ 隐藏）").toBe(true);
    await tick();
    expect(win.hide).toHaveBeenCalledTimes(1);
    expect(win.close).not.toHaveBeenCalled();
    expect(win.destroy).not.toHaveBeenCalled();
  });

  it("★★ 松手就落：取色器 `change` ⇒ 当场落盘那份改过的外观（不等「保存」）；拖的时候不写", async () => {
    const p = await mount();
    const color = dragColor();
    expect(theme.save, "拖的时候（input ~60Hz）就在写盘").not.toHaveBeenCalled();
    releaseColor(color);
    for (let i = 0; i < 3; i++) await tick();
    expect(theme.save).toHaveBeenCalledTimes(1);
    expect(theme.save.mock.calls[0]).toEqual([expect.objectContaining({ bg: "#123456" })]);
    expect(p.isDirty(), "落完之后还算「没落」").toBe(false);
    // 没变就不写：再触发一次 change ⇒ 一发不多。
    releaseColor(color);
    for (let i = 0; i < 3; i++) await tick();
    expect(theme.save).toHaveBeenCalledTimes(1);
  });

  it("★★ 拖了没松手就关窗 ⇒ 那一格顺手落掉再藏窗（不拦、不丢）", async () => {
    const p = await mount();
    dragColor();
    expect(p.isDirty(), "前提：拖了没松手那一格还没落").toBe(true);
    expect(nativeX()).toBe(true);
    for (let i = 0; i < 3; i++) await tick();
    expect(theme.save).toHaveBeenCalledTimes(1);
    expect(theme.save.mock.calls[0]).toEqual([expect.objectContaining({ bg: "#123456" })]);
    expect(win.hide).toHaveBeenCalledTimes(1);
    expect(theme.apply, "关窗把改动回滚了 —— 全即时之后没有「丢弃」这回事").not.toHaveBeenCalledWith({});
  });

  it("★ Claude 数据目录：`change` 就落 ＋ 给重启条供货；Esc 与 X 同一条路（不再是「取消 = 回滚」）", async () => {
    const paths = await import("../../../../src/frontend/ui/paths");
    const setDir = vi.mocked(paths.setClaudeDirOverride);
    setDir.mockClear();
    const p = await mount();
    const dir = document.querySelector<HTMLInputElement>(".settings-input-wide")!;
    dir.value = "/elsewhere/.claude";
    dir.dispatchEvent(new Event("change"));
    for (let i = 0; i < 3; i++) await tick();
    expect(setDir).toHaveBeenCalledWith("/elsewhere/.claude");
    expect(document.querySelector(".settings-restart-bar")!.textContent).toContain("Claude 数据目录");
    expect(p.isDirty()).toBe(false);
    p.handleEsc();
    await tick();
    expect(win.hide).toHaveBeenCalledTimes(1);
  });

  it("★ 页脚「保存」「取消」与拦截条都没了；「恢复默认」在「外观」那一页上", async () => {
    await mount();
    const labels = [...document.querySelectorAll<HTMLButtonElement>(".settings-panel button")].map(
      (b) => b.textContent,
    );
    expect(labels.length, "一颗按钮都没扫到 —— 下面的「没有」是空真").toBeGreaterThan(5);
    for (const gone of ["保存", "取消", "保存并关闭", "丢弃改动", "继续编辑"]) {
      expect(labels, `「${gone}」还在 —— 全即时之后它没有可做的事`).not.toContain(gone);
    }
    expect(document.querySelector("[data-close-guard]")).toBeNull();
    const reset = [
      ...document.querySelectorAll<HTMLButtonElement>('.settings-page[data-route-id="app-appearance"] button'),
    ].find((b) => b.textContent === "恢复外观默认");
    expect(reset, "「恢复默认」没跟到外观页").toBeDefined();
  });

  it("藏起来之后再拿到焦点（= 被重新 show）⇒ 重跑 open()：重读外观；没藏过的焦点不算", async () => {
    await mount();
    // 「重读」＝ 那一发 load_config（三格从同一份配置派生）。
    const reads = () => ipc.calls.filter((c) => c === "load_config").length;
    ipc.calls = [];
    win.focus!({ payload: true }); // 没藏过：只是普通的切回来
    await tick();
    expect(reads(), "没藏过也重读 —— 每次切回窗口都会白打一趟").toBe(0);
    nativeX();
    await tick();
    expect(win.hide).toHaveBeenCalledTimes(1);
    win.focus!({ payload: true });
    await tick();
    expect(reads()).toBe(1);
    expect(document.querySelector(".settings-panel")!.classList.contains("open")).toBe(true);
  });
});

// 「浏览…」那一下：选目录的窗口打不开（插件抛）原先只打 console —— 点了什么都没发生。
describe("〔W5-UI〕选 Claude 数据目录的窗口打不开 ⇒ 说出来", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
  });

  it("插件抛 ⇒ 设置窗 banner 上说一句（原因原样）；插件正常返回取消 ⇒ 不说（正控）", async () => {
    const p = await mount();
    const pick = (): Promise<void> => (p as unknown as { pickClaudeDir(): Promise<void> }).pickClaudeDir();
    vi.mocked(openDialog).mockResolvedValueOnce(null);
    await pick();
    expect(document.body.textContent ?? "").not.toContain("选目录的窗口打不开");
    vi.mocked(openDialog).mockRejectedValueOnce(new Error("dialog-refused-xyz"));
    await pick();
    const text = document.body.textContent ?? "";
    expect(text, "打不开也不说").toContain("选目录的窗口打不开");
    expect(text).toContain("dialog-refused-xyz");
  });
});
