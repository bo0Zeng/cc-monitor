/**
 * ST1「关窗改隐藏」＋「未保存关窗拦截」（`设计/01 §1.3` · `70 §1.3 F` · `§6` #1 · `§8` #7）。
 *
 * 窗口模式下：
 * - 系统 X（close-requested）一律 `preventDefault`，交给面板判；**放行的动作是 `hide()`**，
 *   面板从不调 `close()` / `destroy()`（主窗销毁时由后端把本窗一起收掉 —— `lib.rs` 那条纯函数有自己的测试）。
 * - 有没保存的改动（外观 / Claude 数据目录）⇒ 先亮一条「保存并关闭 / 丢弃改动 / 继续编辑」，不静默丢。
 * - 藏起来之后又拿到焦点（= 被 `open_settings_window` 重新 show）⇒ 重跑 `open()`（重读设置、回落地页）。
 *
 * 〔射程〕jsdom 里没有真窗口：量的是「面板对窗口 API 发了什么」，不是窗口真的藏没藏（那一维要真机）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { win, theme } = vi.hoisted(() => ({
  win: {
    closeRequested: null as null | ((e: { preventDefault: () => void }) => void),
    focus: null as null | ((e: { payload: boolean }) => void),
    hide: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    destroy: vi.fn(() => Promise.resolve()),
  },
  theme: {
    load: vi.fn(() => Promise.resolve({})),
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
vi.mock("../../src/theme", () => ({
  loadTheme: theme.load,
  saveTheme: theme.save,
  applyTheme: theme.apply,
  applyThemeToken: vi.fn(),
}));
vi.mock("../../src/paths", () => ({
  getClaudeDirOverride: () => Promise.resolve(null),
  setClaudeDirOverride: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../src/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => () => Promise.reject(new Error("[录音机]")) }),
}));
vi.mock("../../src/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
  },
}));
vi.mock("../../src/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../src/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../src/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../src/settings/cc-bus-hooks-section", () => ({
  CcBusHooksSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../src/settings/cc_integration", () => ({
  CcIntegrationSection: class { element = document.createElement("div"); loadNow() {} },
}));
vi.mock("../../src/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(), listen: vi.fn() }));

import { SettingsPanel } from "../../src/settings/panel";
import { __setHostOsForTests } from "../../src/settings/host-os";
import { __resetMachineContextForTests } from "../../src/settings/machine-context";

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
const guard = () => document.querySelector<HTMLElement>("[data-close-guard]")!;
const guardShown = () => guard().classList.contains("settings-banner-show");
const btn = (t: string) =>
  [...guard().querySelectorAll("button")].find((b) => b.textContent === t)!;
/** 改一个颜色（外观要点保存才落盘 —— 这就是「未保存」）。 */
function editColor(): void {
  const color = document.querySelector<HTMLInputElement>(".settings-panel input[type=color]")!;
  color.value = "#123456";
  color.dispatchEvent(new Event("input"));
}

describe("ST1：设置窗关窗 ＝ 隐藏；有未保存改动先拦", () => {
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
    expect(guardShown()).toBe(false);
  });

  it("改了颜色：系统 X ⇒ 不藏、亮出拦截条；「继续编辑」收起、窗口还在", async () => {
    const p = await mount();
    editColor();
    expect(p.isDirty(), "前提：改颜色要算未保存").toBe(true);
    expect(nativeX()).toBe(true);
    expect(guardShown()).toBe(true);
    expect(win.hide).not.toHaveBeenCalled();
    btn("继续编辑").click();
    expect(guardShown()).toBe(false);
    expect(win.hide).not.toHaveBeenCalled();
  });

  it("「丢弃改动」⇒ 回滚外观、藏窗，不落盘", async () => {
    await mount();
    editColor();
    nativeX();
    btn("丢弃改动").click();
    await tick();
    expect(theme.apply).toHaveBeenCalledWith({});
    expect(theme.save).not.toHaveBeenCalled();
    expect(win.hide).toHaveBeenCalledTimes(1);
  });

  it("「保存并关闭」⇒ 落盘那份改过的外观、再藏窗", async () => {
    await mount();
    editColor();
    nativeX();
    btn("保存并关闭").click();
    for (let i = 0; i < 3; i++) await tick();
    expect(theme.save).toHaveBeenCalledTimes(1);
    expect(theme.save.mock.calls[0]).toEqual([expect.objectContaining({ bg: "#123456" })]);
    expect(win.hide).toHaveBeenCalledTimes(1);
  });

  it("Claude 数据目录改了没存也算未保存", async () => {
    const p = await mount();
    const dir = document.querySelector<HTMLInputElement>(".settings-input-wide")!;
    dir.value = "/elsewhere/.claude";
    expect(p.isDirty()).toBe(true);
    nativeX();
    expect(guardShown()).toBe(true);
  });

  it("藏起来之后再拿到焦点（= 被重新 show）⇒ 重跑 open()：重读外观；没藏过的焦点不算", async () => {
    await mount();
    theme.load.mockClear();
    win.focus!({ payload: true }); // 没藏过：只是普通的切回来
    await tick();
    expect(theme.load, "没藏过也重读 —— 每次切回窗口都会白打一趟").not.toHaveBeenCalled();
    nativeX();
    await tick();
    expect(win.hide).toHaveBeenCalledTimes(1);
    win.focus!({ payload: true });
    await tick();
    expect(theme.load).toHaveBeenCalledTimes(1);
    expect(document.querySelector(".settings-panel")!.classList.contains("open")).toBe(true);
  });
});
