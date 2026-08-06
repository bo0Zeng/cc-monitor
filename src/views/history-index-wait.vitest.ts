// audit-0805 F14 第四刀：**索引没建好时的等待，不许每秒把整条搜索重跑一遍**。
//
// Rust 侧 `search.rs:848-860` 是无条件 `tokio::join!(本地索引, search_remote_all)` ——
// 没有「本地还在建索引就别问远端」这一说。而 `merge_search_results` 一旦拿到非空远端结果
// 就直接返回 `status: "ready"` ⇒ 那条 1 秒重试链的**实际形态**是：
// 「每秒问一遍所有远端，每秒得到『没有』，然后再问一遍」。
//
// 换成每秒只问 `get_search_index_status`（`search.rs:890`，一次 `RwLock::read`，零 SSH），
// 就绪后再跑**一次**完整搜索（那一次照常含远端）。

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

type Internals = {
  searchMode: string;
  searchInput: HTMLInputElement;
  statusEl: HTMLElement;
  runFullTextSearch(): Promise<void>;
};

/** 索引状态可控的夹具。`indexReady` 一置 true，下一次问状态就返回就绪。 */
function setup(opts: { statusThrows?: boolean } = {}): { setReady(): void } {
  let ready = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "search_history")
      return Promise.resolve(
        ready
          ? { status: "ready", indexedSessions: 9, totalHits: 0, sessionCount: 0, sessions: [] }
          : { status: "indexing", indexedSessions: 3, sessions: [] },
      );
    if (cmd === "get_search_index_status") {
      if (opts.statusThrows) return Promise.reject(new Error("索引进程没了"));
      return Promise.resolve({
        ready,
        indexedSessions: ready ? 9 : 3,
        indexedMessages: 0,
        builtAtMs: 0,
      });
    }
    if (cmd === "list_history_projects") return Promise.resolve([]);
    if (cmd === "list_remote_history_projects")
      return Promise.resolve({ projects: [], failedHosts: [] });
    return Promise.resolve(undefined);
  });
  return {
    setReady() {
      ready = true;
    },
  };
}

function count(cmd: string): number {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd).length;
}

async function openSearching(): Promise<{ view: HistoryView; inner: Internals }> {
  const view = new HistoryView();
  await view.open();
  const inner = view as unknown as Internals;
  inner.searchMode = "fulltext";
  inner.searchInput.value = "kw"; // 空查询会早退
  await inner.runFullTextSearch();
  // 抽取器自检：第一发都没打出去的话，下面全部零命中地绿。
  expect(count("search_history"), "夹具没能触发一次全文搜索 —— 本条会零命中地绿").toBe(1);
  return { view, inner };
}

describe("索引等待期间不许再扇出远端（audit-0805 F14 第四刀）", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("★ 等十秒：search_history 一次都不许再发，问的必须是本地索引状态", async () => {
    setup();
    const { view } = await openSearching();

    await vi.advanceTimersByTimeAsync(10_000);

    expect(
      count("search_history"),
      "★ 等待期间又重跑了整条搜索 —— 它在 Rust 侧无条件 join 了 search_remote_all，" +
        "也就是**每秒对每台远端各一条 SSH**，而且这条链只在远端一条都没命中时才会持续转：" +
        "每秒问一遍所有远端，每秒得到「没有」，然后再问一遍。",
    ).toBe(1);
    expect(
      count("get_search_index_status"),
      "十秒里一次本地状态都没问 —— 那就不是「换数据源」，是「不等了」",
    ).toBe(10);
    view.close();
  });

  it("★ 索引就绪之后，必须真的把完整搜索（含远端）再跑一次", async () => {
    const h = setup();
    const { view } = await openSearching();

    await vi.advanceTimersByTimeAsync(3000);
    expect(count("search_history"), "还没就绪就抢跑").toBe(1);

    h.setReady();
    await vi.advanceTimersByTimeAsync(1000);

    expect(
      count("search_history"),
      "★ 索引建好了却不再搜 —— 用户会一直停在「索引构建中」。" +
        "把远端那条路省掉的前提是「就绪后补一次完整的」，少了这一步就是把功能删了。",
    ).toBe(2);
    view.close();
  });

  it("★ 等待有上限，超了要说清为什么停、用户能做什么（E4/E5）", async () => {
    setup();
    const { view, inner } = await openSearching();

    // 120 拍上限 → 推进 121 秒
    await vi.advanceTimersByTimeAsync(121_000);

    expect(
      count("get_search_index_status"),
      "★ 超过上限还在问 —— 此前这条链**一个上限都没有**：索引若永远建不好它会一直转",
    ).toBeLessThanOrEqual(120);
    const text = inner.statusEl.textContent ?? "";
    expect(text, `停下来了但没说为什么。实际文案：「${text}」`).toContain("已停止自动重试");
    expect(
      text,
      `只说了停，没告诉用户能做什么（E4：静默失败给身份）。实际文案：「${text}」`,
    ).toContain("按项目");
    view.close();
  });

  it("★ 连状态都查不到时，说清楚再停 —— 不许装作还在等（E4）", async () => {
    setup({ statusThrows: true });
    const { view, inner } = await openSearching();

    await vi.advanceTimersByTimeAsync(3000);

    const text = inner.statusEl.textContent ?? "";
    expect(text, `查状态失败却没吭声。实际文案：「${text}」`).toContain("索引状态查询失败");
    expect(
      count("get_search_index_status"),
      "查状态失败之后还在一秒一次地重试 —— 那是把一个持续失败拖成后台噪声",
    ).toBe(1);
    view.close();
  });
});
