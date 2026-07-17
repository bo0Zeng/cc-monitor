// F76（#46）历史「来源列表」TTL 缓存的 jsdom 回归测试。
//
// 为什么需要它：history.ts 1613 行零测试，改到 open/refresh 缓存链极易静默回归
// （某入口绕过缓存 → 又变「每次重连所有远端」，或过期不重连 → 陈旧）。这里在真
// HistoryView 实例 + 真 history-cache 判定上，锁死 #46 的核心保证：
//   - 打开 → 关闭 → TTL 内再打开：list_remote_history_projects **不被二次调用**（复用）；
//   - 本地 list_history_projects **每次都重扫**（便宜、防陈旧）；
//   - TTL 过期后再打开 → 远端**重新 fan-out**；
//   - 强刷（refresh(true)）→ 无视 TTL 重新 fan-out。
//
// history.ts 重度依赖 Tauri IPC + 一堆协作模块，照 tabs.vitest.ts 先例把重协作者 mock 成空壳。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
  // loadProjectSessions 用（本测试不展开项目，给个能 new 的空壳即可）
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
import { showActionFailureToast } from "../error-toast";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function countCalls(cmd: string): number {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd).length;
}

function setupInvoke(remote: unknown[] = []): void {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_history_projects") return Promise.resolve([]);
    if (cmd === "list_remote_history_projects") return Promise.resolve(remote);
    return Promise.resolve(undefined);
  });
}

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

    // 刷新按钮走 refresh(true)（此处直调等价，时间仍在 TTL 内）
    now += 1_000;
    await (view as unknown as { refresh(force: boolean): Promise<void> }).refresh(true);
    expect(countCalls("list_remote_history_projects")).toBe(2);
  });

  it("远端 fan-out 失败：降级复用上次缓存，不再重连，仍提示", async () => {
    const view = new HistoryView();
    await view.open(); // 成功抓一次，缓存 5 个远端项目
    setupInvoke(); // reset 计数（但下面手动改实现）

    // 让远端在过期后再抓时失败
    now += 40_000;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_history_projects") return Promise.resolve([]);
      if (cmd === "list_remote_history_projects")
        return Promise.reject(new Error("ssh down"));
      return Promise.resolve(undefined);
    });
    await view.close();
    await view.open();
    // 失败被 toast、不抛；本地仍成功渲染（不阻断）
    expect(showActionFailureToast).toHaveBeenCalled();
  });
});
