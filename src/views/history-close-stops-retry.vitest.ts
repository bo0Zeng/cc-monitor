// audit-0805 F14：**关掉历史视图必须掐断那条 1 秒重试链**。
//
// 全文搜索在 `status === "indexing"` 时挂一个 `setTimeout(…, 1000)` 重跑 `runFullTextSearch()`，
// 而它内含 `search_remote_all` ⇒ 对每台远端各一条 SSH。那个回调的存活判据是 `seq === this.ftSeq`，
// 而 `close()` 此前**不动 ftSeq**（复位在 `open()`）⇒ 视图关掉、root 已 remove() 之后那条链**照跑**，
// 每秒继续对所有远端扇出，并把结果写进已 detach 的 DOM。
//
// ⚠ 这条是 V2 只读核实时**顺带查出来的**，报告里没有 —— 而它比报告点名的好几条都更直接：
// 用户关掉一个面板之后，后台还在替他每秒问所有远端。

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

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** 让 `search_history` 一直回 indexing ⇒ 每次都会挂上那个 1 秒重试。 */
function setupIndexing(): void {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "search_history")
      return Promise.resolve({ status: "indexing", indexedSessions: 3, results: [] });
    if (cmd === "get_search_index_status")
      return Promise.resolve({
        ready: false,
        indexedSessions: 3,
        indexedMessages: 0,
        builtAtMs: 0,
      });
    if (cmd === "list_history_projects") return Promise.resolve([]);
    if (cmd === "list_remote_history_projects")
      return Promise.resolve({ projects: [], failedHosts: [] });
    return Promise.resolve(undefined);
  });
}

type Internals = {
  searchMode: string;
  searchInput: HTMLInputElement;
  runFullTextSearch(): Promise<void>;
};

describe("关掉历史视图之后，那条 1 秒远端重试链必须停（audit-0805 F14）", () => {
  beforeEach(() => {
    setupIndexing();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("close() 之后推进 1 秒，不许再发起 search_history", async () => {
    const view = new HistoryView();
    await view.open();
    const inner = view as unknown as Internals;
    inner.searchMode = "fulltext";
    inner.searchInput.value = "kw"; // 空查询会早退，不会发出搜索

    await inner.runFullTextSearch();
    const afterFirst = invokeMock.mock.calls.filter((c) => c[0] === "search_history").length;
    // 抽取器自检：第一次都没打出去的话，下面的判定毫无意义。
    expect(
      afterFirst,
      "夹具没能触发一次全文搜索 —— 本条会零命中地绿（检查 search_history 的 mock 形状）",
    ).toBeGreaterThan(0);

    view.close();
    await vi.advanceTimersByTimeAsync(1500);

    const afterClose = invokeMock.mock.calls.filter((c) => c[0] === "search_history").length;
    expect(
      afterClose,
      "★ 视图关掉之后那条 1 秒重试链还在跑 —— 它内含 search_remote_all，" +
        "也就是**对每台远端各一条 SSH**。用户关掉了面板，后台还在替他每秒问所有远端，" +
        "而结果会被写进已经 detach 的 DOM。close() 里递增一次 ftSeq 就能让挂着的回调自行终止。",
    ).toBe(afterFirst);
  });

  it("没关的时候它照常等（防把上面写成「永远不等」）", async () => {
    const view = new HistoryView();
    await view.open();
    const inner = view as unknown as Internals;
    inner.searchMode = "fulltext";
    inner.searchInput.value = "kw";

    await inner.runFullTextSearch();
    const searchAfterFirst = invokeMock.mock.calls.filter(
      (c) => c[0] === "search_history",
    ).length;
    expect(searchAfterFirst, "夹具没触发搜索 —— 本条会零命中地绿").toBeGreaterThan(0);
    const statusBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "get_search_index_status",
    ).length;

    await vi.advanceTimersByTimeAsync(1500);

    // ⚠ F14 第四刀改了**等待的机制**：不再每秒重跑整条搜索，改成每秒问一次本地索引状态。
    // 这条判据的**意图没变**（不许写成「永远不等」），判的东西跟着机制换。
    const statusAfter = invokeMock.mock.calls.filter(
      (c) => c[0] === "get_search_index_status",
    ).length;
    expect(
      statusAfter,
      "视图还开着却连状态都不问了 —— 索引建好之后用户会一直看着「索引构建中」不动",
    ).toBeGreaterThan(statusBefore);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "search_history").length,
      "等待期间又去重跑整条搜索了 —— 那条路含 search_remote_all，等于每秒对每台远端一条 SSH",
    ).toBe(searchAfterFirst);
    view.close();
  });
});
