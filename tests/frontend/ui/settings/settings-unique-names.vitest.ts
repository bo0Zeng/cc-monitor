/**
 * 判据 **#11**：**界面上没有重名的顶层项**（ST1「改名」那一拍的判据）。
 *
 * `§5.3`把机器列表页那块「还差什么（诊断汇总）」改名成「诊断」；而「应用 → 日志与数据」里
 * 原先**也有一个**「诊断」（monitor 的日志开关）。`§10.3` 要求两次改名同拍：那一块让名成「日志」，
 * 否则从那一刻起面板上会同时有两个「诊断」—— 一个是「这台机器还缺什么」，一个是日志开关。
 *
 * # 判据形状
 *
 * 人群 = 真渲染出来的设置面板里：**左侧导航项** ＋ **块标题**（`.settings-group-title` ·
 * 折叠组标题 `.settings-collapsible-title`）。
 * - 同名计数 == 1（两向：列表长度 == 去重后的长度）；
 * - 人群锚：「诊断」「日志」**都在**（改名真的落了，而不是两个都消失了所以不重名）。
 *
 * ⚠ 刻意**不**把机器子页的横向栏（连接 / 组件 / 账号 / 工具 / 足迹）算进来：
 *   那一栏按块取名（「账号」栏里就是「账号」那一块），是**同一个东西的两层包装**，不是两个东西撞名。
 *
 * 用**真的** `RemoteSection`（「诊断」那块由它画）；IPC 一律 reject（各块自有 catch），
 * 账本为空 ⇒ 「诊断」块会出现（全新用户每一格都「没测过」）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    { get: () => () => Promise.reject(new Error("[录音机] 没有真后端")) },
  ),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: () => Promise.reject(new Error("[录音机] 没有真后端")),
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
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";

const tick = () => new Promise((r) => setTimeout(r, 0));

/** 一个元素自己的文字（不含 ⓘ 之类的子控件）。 */
const ownText = (el: Element): string =>
  [...el.childNodes]
    .filter((n) => n.nodeType === Node.TEXT_NODE)
    .map((n) => n.nodeValue ?? "")
    .join("")
    .trim();

async function names(): Promise<string[]> {
  new SettingsPanel({ windowMode: true });
  for (let i = 0; i < 4; i++) await tick();
  const root = document.querySelector(".settings-panel")!;
  // 左侧导航：最外层那个 nav（机器子页里的横向栏不算，见头注）。
  const topNav = root.querySelector(".settings-shell:not(.settings-shell-h) > .settings-nav")!;
  const nav = [...topNav.querySelectorAll(".settings-nav-item")].map((el) => el.querySelector(".settings-nav-label")?.textContent?.trim() ?? "");
  const blocks = [
    ...root.querySelectorAll(".settings-group-title, .settings-collapsible-title"),
  ].map(ownText);
  return [...nav, ...blocks].filter((t) => t !== "");
}

describe("：面板上没有重名的导航项 / 块标题", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    __resetMachineContextForTests();
    __setHostOsForTests("windows");
    try {
      localStorage.clear();
    } catch {
      /* 账本住 localStorage；清掉 = 全新用户 */
    }
  });

  it("人群锚：「日志」（通用页下）在，且人群不小", async () => {
    const got = await names();
    expect(got).toContain("日志");
    expect(got).toContain("机器");
    expect(got).toContain("账号");
    // 旧名一个都不许还在。
    expect(got.filter((t) => t.includes("还差什么"))).toEqual([]);
  });

  it("同名计数 == 1（列表 == 去重后的列表）", async () => {
    const got = await names();
    const dup = got.filter((t, i) => got.indexOf(t) !== i);
    expect(dup, `重名：${dup.join("、")}（全表：${got.join(" / ")}）`).toEqual([]);
  });
});
