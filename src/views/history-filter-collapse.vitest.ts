// F86（#45）历史「来源筛选保持」+「多主机默认折叠」的 jsdom 回归测试。
//
// 为什么需要它：history.ts 1613 行零 TS 单测，折叠/筛选持久化极易静默回归（默认折叠污染成偏好、
// 或用户展开后又被默认折叠盖掉、或隐藏偏好丢失）。这里在真 HistoryView 实例 + 真 history-prefs 上
// 锁死：隐藏跨重启保持、远端首见默认折叠、用户展开后跨重启保持展开、折回默认清键。
//
// 照 history-source-cache.vitest.ts 的 mock 骨架把重协作者 mock 成空壳；「重启」= close 旧实例后
// new 一个新 HistoryView（共享 localStorage，不清）。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("./session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_close: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-launch-run", () => ({ runRemoteResume: vi.fn() }));
vi.mock("../behavior", () => ({ getBehavior: () => ({}) }));
vi.mock("../format", () => ({ formatTimestampSmart: () => "时间" }));

import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "./history";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

// 一个本地项目（origin 缺省）+ 一个远端项目（origin=hostA）→ distinctOrigins==2 → 触发分组 + chip 行。
function localProj(): Record<string, unknown> {
  return { projectPath: "/l/p", projectName: "本地项目", projectDir: "lp", sessionCount: 1, starredCount: 0, hiddenCount: 0, lastActivity: 1, hasLive: false };
}
function remoteProj(): Record<string, unknown> {
  return { projectPath: "/r/p", projectName: "远端项目", projectDir: "rp", sessionCount: 1, starredCount: 0, hiddenCount: 0, lastActivity: 1, hasLive: false, origin: "hostA" };
}
function setupInvoke(): void {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_history_projects") return Promise.resolve([localProj()]);
    if (cmd === "list_remote_history_projects")
      return Promise.resolve({ projects: [remoteProj()], failedHosts: [] });
    return Promise.resolve(undefined);
  });
}

type ViewInternals = {
  hiddenOrigins: Set<string>;
  originOpenOverrides: Record<string, boolean>;
  projects: unknown[];
};

function group(name: string): HTMLDetailsElement | undefined {
  return [...document.querySelectorAll<HTMLDetailsElement>(".history-origin-group")].find(
    (g) => g.querySelector(".history-origin-name")?.textContent === name,
  );
}
function chip(text: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>(".history-origin-chip")].find(
    (c) => c.textContent === text,
  );
}

describe("HistoryView 来源筛选保持 + 多主机默认折叠 (F86 #45)", () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.replaceChildren();
    setupInvoke();
  });

  it("远端首见默认折叠、本地默认展开，且不污染偏好表", async () => {
    const view = new HistoryView();
    await view.open();
    expect(group("[hostA]")?.open).toBe(false); // 远端默认折叠
    expect(group("本地")?.open).toBe(true); // 本地默认展开
    // 首见默认折叠不写偏好（即便初始程序化 open=false 触发了 toggle，nextOverrides 也删键）
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({});
  });

  it("用户展开远端 → 偏好持久 → 重启后保持展开（不被默认折叠盖掉）", async () => {
    const view = new HistoryView();
    await view.open();
    const hostA = group("[hostA]")!;
    // 模拟用户展开
    hostA.open = true;
    hostA.dispatchEvent(new Event("toggle"));
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({ hostA: true });
    expect(localStorage.getItem("cc-monitor.history.origin-open")).toContain("hostA");

    // 重启：close + 新实例（共享 localStorage）
    view.close();
    const view2 = new HistoryView();
    await view2.open();
    expect(group("[hostA]")?.open).toBe(true); // 保持展开
    expect(group("本地")?.open).toBe(true); // 本地仍展开
  });

  it("展开后折回默认 → 偏好表清空该键（回落默认折叠）", async () => {
    const view = new HistoryView();
    await view.open();
    const hostA = group("[hostA]")!;
    hostA.open = true;
    hostA.dispatchEvent(new Event("toggle"));
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({ hostA: true });
    // 折回
    hostA.open = false;
    hostA.dispatchEvent(new Event("toggle"));
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({});
  });

  it("隐藏某来源 → 跨重启保持隐藏 + chip inactive", async () => {
    const view = new HistoryView();
    await view.open();
    // 点掉 hostA chip
    chip("[hostA]")!.click();
    const inner = view as unknown as ViewInternals;
    expect(inner.hiddenOrigins.has("hostA")).toBe(true);
    expect(localStorage.getItem("cc-monitor.history.hidden-origins")).toContain("hostA");

    // 重启
    view.close();
    const view2 = new HistoryView();
    await view2.open();
    const inner2 = view2 as unknown as ViewInternals;
    expect(inner2.hiddenOrigins.has("hostA")).toBe(true); // 仍隐藏
    expect(chip("[hostA]")?.classList.contains("active")).toBe(false); // chip inactive
    // hostA 项目不渲染（无 [hostA] 分区）
    expect(group("[hostA]")).toBeUndefined();
  });
});
