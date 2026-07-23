// F76（#46）历史「来源列表」TTL 缓存的 jsdom 回归测试。
//
// 为什么需要它：history.ts 1613 行零测试，改到 open/refresh 缓存链极易静默回归
// （某入口绕过缓存 → 又变「每次重连所有远端」，或过期不重连 → 陈旧）。这里在真
// HistoryView 实例 + 真 history-cache 判定上，锁死 #46 的核心保证与 Phase D 审计修复：
//   - 打开 → 关闭 → TTL 内再打开：list_remote_history_projects **不被二次调用**（复用）；
//   - 本地 list_history_projects **每次都重扫**；TTL 过期/强刷 → 远端重新 fan-out；
//   - 全部台失败（Err）→ 降级**复用旧缓存**（不覆盖）、仅 toast；
//   - 部分台失败（Ok + failedHosts）→ **不冻结缓存**，下次 open 重试失败台。
//
// history.ts 重度依赖 Tauri IPC + 一堆协作模块，照 tabs.vitest.ts 先例把重协作者 mock 成空壳。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

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
import { LS_KEYS } from "../local-storage";
import { showActionFailureToast } from "../error-toast";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function countCalls(cmd: string): number {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd).length;
}

// 后端 list_remote_history_projects 返回 { projects, failedHosts }（F76）。
function setupInvoke(
  projects: unknown[] = [],
  failedHosts: string[] = [],
): void {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_history_projects") return Promise.resolve([]);
    if (cmd === "list_remote_history_projects")
      return Promise.resolve({ projects, failedHosts });
    return Promise.resolve(undefined);
  });
}

// 一个远端项目样本（origin=host 让它归远端段、进 remoteCache）。
function remoteProj(dir: string, origin = "hostA"): Record<string, unknown> {
  return {
    projectPath: `/r/${dir}`,
    projectName: dir,
    projectDir: dir,
    sessionCount: 3,
    starredCount: 0,
    hiddenCount: 0,
    lastActivity: 1,
    hasLive: false,
    origin,
  };
}

type ViewInternals = {
  projects: Array<{ origin?: string }>;
  remoteCache: { projects: unknown[] } | null;
  refresh(force: boolean): Promise<void>;
};

describe("HistoryView 来源列表 TTL 缓存 (F76 #46)", () => {
  let nowSpy: ReturnType<typeof vi.spyOn>;
  let now = 1_000_000;

  beforeEach(() => {
    now = 1_000_000;
    nowSpy = vi.spyOn(Date, "now").mockImplementation(() => now);
    localStorage.clear();
    document.body.replaceChildren();
    setupInvoke([]);
  });
  afterEach(() => {
    nowSpy.mockRestore();
  });

  it("TTL 内 reopen：远端只 fan-out 一次，本地每次重扫", async () => {
    const view = new HistoryView();

    await view.open();
    expect(countCalls("list_history_projects")).toBe(1);
    expect(countCalls("list_remote_history_projects")).toBe(1);

    view.close();
    now += 5_000; // 5s < 30s TTL
    await view.open();

    // 本地每次重扫 → 2 次；远端 TTL 内复用 → 仍 1 次（#46 核心保证）
    expect(countCalls("list_history_projects")).toBe(2);
    expect(countCalls("list_remote_history_projects")).toBe(1);
  });

  it("TTL 过期后 reopen：远端重新 fan-out", async () => {
    const view = new HistoryView();
    await view.open();
    expect(countCalls("list_remote_history_projects")).toBe(1);

    view.close();
    now += 40_000; // > 30s TTL
    await view.open();
    expect(countCalls("list_remote_history_projects")).toBe(2);
  });

  it("强刷 refresh(true)：无视 TTL 重新 fan-out", async () => {
    const view = new HistoryView();
    await view.open();
    expect(countCalls("list_remote_history_projects")).toBe(1);

    now += 1_000; // 仍在 TTL 内
    await (view as unknown as ViewInternals).refresh(true);
    expect(countCalls("list_remote_history_projects")).toBe(2);
  });

  it("全部台失败（Err）：降级复用旧缓存（不覆盖）+ toast，仍显示旧远端项目", async () => {
    const view = new HistoryView();
    // open 1：成功缓存 2 个真实远端项目
    setupInvoke([remoteProj("p1"), remoteProj("p2")]);
    await view.open();
    const inner = view as unknown as ViewInternals;
    expect(inner.remoteCache?.projects.length).toBe(2);
    expect(inner.projects.filter((p) => p.origin === "hostA").length).toBe(2);

    // 过期后再抓时全部台失败（reject）
    now += 40_000;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects") return Promise.resolve([]);
      if (cmd === "list_remote_history_projects")
        return Promise.reject(new Error("all hosts down"));
      return Promise.resolve(undefined);
    });
    view.close();
    await view.open();

    // 失败被 toast、不抛；**旧缓存被保住不覆盖**；首帧仍从缓存渲染出 2 个远端项目
    expect(showActionFailureToast).toHaveBeenCalled();
    expect(inner.remoteCache?.projects.length).toBe(2);
    expect(inner.projects.filter((p) => p.origin === "hostA").length).toBe(2);
  });

  it("部分台失败（Ok + failedHosts）：不冻结缓存，下次 open 重试失败台", async () => {
    const view = new HistoryView();
    // open 1：hostA 成功给 1 个项目，hostB 失败
    setupInvoke([remoteProj("pA", "hostA")], ["hostB"]);
    await view.open();
    const inner = view as unknown as ViewInternals;

    // 部分失败 → 缓存**不冻结**（置 null）、toast 提示、但已拿到的 hostA 项目仍渲染
    expect(showActionFailureToast).toHaveBeenCalled();
    expect(inner.remoteCache).toBeNull();
    expect(inner.projects.filter((p) => p.origin === "hostA").length).toBe(1);

    // TTL 内 reopen：因缓存为 null（未冻结）→ 仍重新 fan-out 重试失败台（对齐 F76 前自愈）
    const before = countCalls("list_remote_history_projects");
    view.close();
    now += 5_000; // TTL 内
    await view.open();
    expect(countCalls("list_remote_history_projects")).toBe(before + 1);
  });

  // === F76b(#46) 跨启动持久化:首开也暖 ===

  it("F76b 全部台成功 → 快照持久化到 localStorage", async () => {
    const view = new HistoryView();
    setupInvoke([remoteProj("p1"), remoteProj("p2")]);
    await view.open();
    const raw = localStorage.getItem(LS_KEYS.historyRemoteSources);
    expect(raw).not.toBeNull();
    expect((JSON.parse(raw!) as { projects: unknown[] }).projects.length).toBe(2);
  });

  it("F76b 新实例从 localStorage hydrate:首帧暖(远端项目在场)+ loadedAt 归 0 强制首开仍 refetch", async () => {
    // 预置上次启动存的持久快照(loadedAt 给个不太久以前的值,验证 hydrate 归 0 而非沿用)
    localStorage.setItem(
      LS_KEYS.historyRemoteSources,
      JSON.stringify({ projects: [remoteProj("cached")], loadedAt: now - 5_000 }),
    );
    const view = new HistoryView();
    const inner = view as unknown as ViewInternals;
    // ★构造即 hydrate:remoteCache 有值(供首帧暖绘),但 loadedAt 归 0(仅作暖绘、不冒充新鲜)
    expect(inner.remoteCache?.projects.length).toBe(1);
    expect((inner.remoteCache as unknown as { loadedAt: number }).loadedAt).toBe(0);
    // ★首开:即便刚 hydrate,loadedAt=0 也强制 refetch 一次(不吃跨启动陈旧)
    setupInvoke([remoteProj("fresh")]);
    await view.open();
    expect(countCalls("list_remote_history_projects")).toBe(1);
  });

  it("F76b 部分台失败 → 清掉持久快照(免下次启动暖绘残缺列表)", async () => {
    const view = new HistoryView();
    setupInvoke([remoteProj("p1")]); // 先成功 → 持久化
    await view.open();
    expect(localStorage.getItem(LS_KEYS.historyRemoteSources)).not.toBeNull();
    now += 40_000; // 过期
    setupInvoke([remoteProj("pA", "hostA")], ["hostB"]); // 部分失败
    view.close();
    await view.open();
    expect(localStorage.getItem(LS_KEYS.historyRemoteSources)).toBeNull();
  });

  it("F76b 脏 localStorage(混 null/基元元素)→ 逐元素过滤、不崩、open 正常", async () => {
    localStorage.setItem(
      LS_KEYS.historyRemoteSources,
      JSON.stringify({ projects: [null, "garbage", 42, remoteProj("ok")], loadedAt: now - 1_000 }),
    );
    const view = new HistoryView();
    const inner = view as unknown as ViewInternals;
    // ★只留形状合法的 1 个(projectPath 为 string);null/基元被过滤(否则首帧 renderList deref 会崩)
    expect(inner.remoteCache?.projects.length).toBe(1);
    // ★open 不 reject(修前:首帧 renderList 对 null 元素 deref p.origin 抛 → open() 崩、历史打不开)
    setupInvoke([]);
    await expect(view.open()).resolves.toBeUndefined();
  });

  it("F76b 跨启动 all-fail:持久快照不清、首帧仍暖(Err 分支不动 localStorage)", async () => {
    localStorage.setItem(
      LS_KEYS.historyRemoteSources,
      JSON.stringify({ projects: [remoteProj("cached")], loadedAt: now - 1_000 }),
    );
    const view = new HistoryView(); // 新实例 hydrate 上次成功快照
    invokeMock.mockReset();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects") return Promise.resolve([]);
      if (cmd === "list_remote_history_projects") return Promise.reject(new Error("all down"));
      return Promise.resolve(undefined);
    });
    await view.open();
    const inner = view as unknown as ViewInternals;
    expect(showActionFailureToast).toHaveBeenCalled();
    expect(inner.projects.filter((p) => p.origin === "hostA").length).toBe(1); // 暖帧 cached 仍在
    expect(localStorage.getItem(LS_KEYS.historyRemoteSources)).not.toBeNull(); // Err 分支不清持久
  });
});
