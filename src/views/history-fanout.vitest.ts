/**
 * audit-0805 F07 下半（报告 B-6 前四环）：**一次按键的代价是三个放大器相乘，不是相加**。
 *
 * | # | 放大器 | 原状 |
 * |---|---|---|
 * | ① | 搜索框 `input` **无防抖** | 每敲一个字符都同步走一遍下面两条 |
 * | ② | `renderList` **自我扇出** | `appendProjectGroup` 里 `.then(() => this.renderList())` 写在 per-project 循环里；搜索激活时 `expanded` 恒 true ⇒ **P 个未缓存项目各触发一次完整重建**，而每次重建又会再走一遍这个循环 |
 * | ③ | 那 P 条 `loadProjectSessions` **无并发上限** | 一次按键 P 条 IPC 齐发（远端项目每条还含 SSH） |
 *
 * ★ 另有**第四个、核实台账没点到的**：`loadProjectSessions` 的 `channel.onmessage` 里那个
 * `rafPending` 布尔是**每个项目各一个** —— 去重只在单个项目内部生效，
 * P 个项目同时流式回来时一帧里仍可能重画 P 次。
 *
 * # 判据钉什么
 *
 * 钉的是**次数**，不是「快不快」：`renderList` 跑了几次、`loadProjectSessions` 起了几条、
 * 同时在飞几条。这三个数是可判定的；「卡不卡」不是。
 * ⚠ 仓里已确诊过这条链的用户后果（`events.ts:150-152` 逐字「用户报告先快一会儿然后变慢」），
 * 但那是**另一条**链（渲染），本件只管历史视图这条。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";

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

interface Internals {
  isOpen: boolean;
  searchMode: string;
  searchInput: HTMLInputElement;
  projects: unknown[];
  sessionCache: Map<string, unknown>;
  fanoutStats: { renders: number; loads: number; peakConcurrent: number };
  renderList(): void;
  toggleAll(): Promise<void>;
}
const peek = (v: HistoryView): Internals => v as unknown as Internals;

/** P 个项目，每个项目的 session 流式加载**永不自己完成**（由测试放行）。 */
function setup(projectCount: number): { release: () => void; inFlight: () => number } {
  const pending: (() => void)[] = [];
  let live = 0;
  const projects = Array.from({ length: projectCount }, (_, i) => ({
    name: `p${i}`,
    path: `/p${i}`,
    projectDir: `/p${i}`,
    sessionCount: 3,
    updatedAt: 0,
  }));
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_history_projects") return Promise.resolve(projects);
    if (cmd === "list_remote_history_projects")
      return Promise.resolve({ projects: [], failedHosts: [] });
    if (cmd === "stream_history_sessions_in_project") {
      live += 1;
      return new Promise<void>((r) =>
        pending.push(() => {
          live -= 1;
          r();
        }),
      );
    }
    return Promise.resolve(undefined);
  });
  return {
    release: () => {
      while (pending.length) pending.shift()?.();
    },
    inFlight: () => live,
  };
}

async function openSearching(projectCount: number): Promise<{
  view: HistoryView;
  inner: Internals;
  h: ReturnType<typeof setup>;
}> {
  const h = setup(projectCount);
  const view = new HistoryView();
  await view.open();
  const inner = peek(view);
  // 抽取器自检：项目没进来的话，下面几条全是零命中地绿。
  expect(inner.projects.length, "夹具没把项目喂进去 —— 本文件会零命中地绿").toBe(projectCount);
  inner.fanoutStats.renders = 0;
  inner.fanoutStats.loads = 0;
  inner.fanoutStats.peakConcurrent = 0;
  return { view, inner, h };
}

describe("历史视图的三个放大器（audit-0805 F07 下半，报告 B-6）", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("★ ① 连敲 8 个字符 → 只重画一次，不是八次", async () => {
    const { view, inner } = await openSearching(3);
    inner.searchMode = "tree";
    for (const ch of "abcdefgh") {
      inner.searchInput.value += ch;
      inner.searchInput.dispatchEvent(new Event("input"));
    }
    expect(
      inner.fanoutStats.renders,
      "去抖窗口还没到就已经重画了 —— 那说明 `input` 处理器仍在同步调 renderList",
    ).toBe(0);

    await vi.advanceTimersByTimeAsync(300);
    expect(
      inner.fanoutStats.renders,
      `连敲 8 下之后重画了 ${inner.fanoutStats.renders} 次 —— 搜索框没有去抖。` +
        "而每次重画又会扇出 P 条懒加载、每条回来再触发一次重画：三个放大器是**相乘**的。",
    ).toBe(1);
    view.close();
  });

  // ═══ Phase G 全局变异抽样的产物 ═══════════════════════════════════════
  //
  // 抽样把 `SEARCH_DEBOUNCE_MS` 从 250 改成 **0**，**全套 1258 条全绿** —— 变异存活。
  //
  // 原因是上面那条「连敲 8 个字符 → 只重画一次」把 8 次按键**同步**发出去：
  // 同一 tick 内，`setTimeout(fn, 0)` 与 `setTimeout(fn, 250)` 的合批效果**一模一样**
  // （都靠那个 schedule-once 的 timer 句柄）。⇒ 它钉住的是「同一 tick 内合批」，
  // **完全没有碰到那个窗口值**。而真实打字是每次相隔几十到几百毫秒的。
  //
  // ★ 一般化：**常量被判据「覆盖」不等于它被判据钉住** —— 得让那个常量真的参与判定。
  it("★ ① 真实打字节奏（每 100ms 一下）仍只重画一次 —— 钉的是那个窗口值本身", async () => {
    const { view, inner } = await openSearching(3);
    inner.searchMode = "tree";
    for (const ch of "abcd") {
      inner.searchInput.value += ch;
      inner.searchInput.dispatchEvent(new Event("input"));
      await vi.advanceTimersByTimeAsync(100); // < 250ms 窗口 ⇒ 每次都该续期
    }
    expect(
      inner.fanoutStats.renders,
      `按 100ms 的节奏敲了 4 下，窗口还没满就已经重画了 ${inner.fanoutStats.renders} 次 —— ` +
        "去抖窗口比击键间隔还短（把 `SEARCH_DEBOUNCE_MS` 调到 0 就是这个样子，" +
        "而那个变异在 Phase G 抽样里**存活过**）。",
    ).toBe(0);

    await vi.advanceTimersByTimeAsync(300);
    expect(inner.fanoutStats.renders, "停手之后该重画恰好一次").toBe(1);
    view.close();
  });

  it("★ ① 反向：去抖不许变成「永远不搜」", async () => {
    const { view, inner } = await openSearching(2);
    inner.searchMode = "tree";
    inner.searchInput.value = "kw";
    inner.searchInput.dispatchEvent(new Event("input"));
    await vi.advanceTimersByTimeAsync(300);
    expect(
      inner.fanoutStats.renders,
      "去抖窗口过了还是没重画 —— 那不是去抖，是把搜索关掉了",
    ).toBeGreaterThan(0);
    view.close();
  });

  it("★ ③ 12 个未缓存项目 → 同时在飞的加载不超过 4", async () => {
    const { view, inner, h } = await openSearching(12);
    inner.searchMode = "tree";
    inner.searchInput.value = "p";
    inner.searchInput.dispatchEvent(new Event("input"));
    await vi.advanceTimersByTimeAsync(300);

    expect(
      h.inFlight(),
      `同时在飞 ${h.inFlight()} 条 —— 无上限扇出。远端项目每条还含一次 SSH，` +
        "一次按键把它们全放出去正是报告 B-6 说的那件事。",
    ).toBeLessThanOrEqual(4);
    // 反向：也不许限成串行
    expect(h.inFlight(), "只放了 1 条 —— 限成串行了，那会让 12 个项目排成一条长队").toBe(4);
    h.release();
    view.close();
  });

  it("★ ② 12 个项目的加载全部回来 → 重画次数远小于 12（批末合并）", async () => {
    const { view, inner, h } = await openSearching(12);
    inner.searchMode = "tree";
    inner.searchInput.value = "p";
    inner.searchInput.dispatchEvent(new Event("input"));
    await vi.advanceTimersByTimeAsync(300);
    const afterSearch = inner.fanoutStats.renders;

    h.release();
    await vi.advanceTimersByTimeAsync(200);

    const caused = inner.fanoutStats.renders - afterSearch;
    expect(
      caused,
      `12 条加载回来触发了 ${caused} 次全树重建 —— 那就是 renderList 的自我扇出：` +
        "`.then(() => this.renderList())` 写在 per-project 循环里，P 个项目各触发一次，" +
        "而每次重建又会再走一遍那个循环。批末合并之后应该远小于 12。",
    ).toBeLessThan(6);
    view.close();
  });

  it("★ ③ 全展开：也不许无上限齐发（此前是裸 Promise.all(map)）", async () => {
    const { view, inner, h } = await openSearching(12);
    void peek(view).toggleAll();
    await vi.advanceTimersByTimeAsync(0);
    expect(
      h.inFlight(),
      `全展开一次放出 ${h.inFlight()} 条 —— 「加载全部」那条路有工作池，这条却是裸 ` +
        "`Promise.all(toLoad.map(…))`。同一个仓里同一件事两种写法，其中一种是对的。",
    ).toBeLessThanOrEqual(4);
    h.release();
    await vi.advanceTimersByTimeAsync(50);
    void inner;
    view.close();
  });
});

// ═══ audit-0805 §5 1u 结案（08-06）：单次重算的代价**量过了，不该 memo** ═══
//
// 1u 挂了一条待办「`buildSessionTree`/`sortTree` 单次重算仍是全量」。按第四问去量：
// 单个项目 200 / 1000 / 5000 / 20000 个会话 ⇒ 0.18 / 0.35 / 1.39 / 5.85 ms。
// 现实量级（几十到几百）是**亚毫秒**，而 F15 之后每帧只重画一次 ⇒ 加 memo 换来亚毫秒、
// 代价是一块新状态和它的失效 bug。**出口③：本来就不该做。**
//
// ★ 但那个结论**有前提**：`buildSessionTree` 是**按项目**调的，所以 N 是单项目会话数。
// 改成对全部项目建一棵大树，N 就变成总会话数，上面那张表要重量。
// 本条钉住调用点**恰好一处** —— 挪动它的人会被迫回到 `history.ts` 那段头注。
describe("§5 1u 结案的前提：树是按项目建的", () => {
  it("★ `buildSessionTree` 的调用点恰好一处", () => {
    const src = readFileSync("src/views/history.ts", "utf8");
    // 抽取器自检：读到的得是那个文件（它是几千行的大文件，不是空串）。
    expect(src.length, "history.ts 只读到几个字节 —— 路径变了，本条会零命中地绿").toBeGreaterThan(50_000);
    const calls = src.match(/buildSessionTree\(/g)?.length ?? 0;
    const defs = src.match(/function buildSessionTree\(/g)?.length ?? 0;
    expect(
      calls - defs,
      `\`buildSessionTree\` 有 ${calls - defs} 个调用点（该是 1）。\n` +
        "★ §5 1u 的结案结论（单次代价亚毫秒、不值得 memo）**建立在「按项目调」之上** ——\n" +
        "N 是单个项目的会话数。多一个调用点、或改成对全部项目建一棵树，那个结论就要重量。\n" +
        "请回 `history.ts::buildSessionTree` 头注看那张实测表，量完再改。",
    ).toBe(1);
  });
});
