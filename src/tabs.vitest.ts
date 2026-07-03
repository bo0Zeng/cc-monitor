// Tab 生命周期状态机的 DOM 单元测试（vitest + jsdom）。
//
// 为什么需要它：本轮迭代两个真 bug 都在 tabs.ts 的生命周期里——
//   1) resume 后已归档**本地** Tab 不自动复活（reviveTab + origin 门控）；
//   2) 关 Tab 只允许 archived（closeTab 守卫）。
// tabs.ts 重度依赖 DOM + 一堆协作模块（MessageStream / RecordTimeline / BranchFolder /
// 渲染 / Tauri IPC），无法像现有 *.test.ts 那样在裸 node 里测。这里用 jsdom 提供真 DOM、
// 把重协作者 mock 成空壳，于是能在真 TabManager 实例上断言状态翻转。

import { describe, it, expect, vi, beforeEach } from "vitest";

// --- 把重/IPC 协作者 mock 掉，让 TabManager 能在 jsdom 下实例化（避免拉 marked/katex/IPC）---
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("./stream", () => ({
  MessageStream: class {
    contentElement = document.createElement("div");
    constructor(_root: HTMLElement) {}
    insertNode(): void {}
    scrollToBottom(): void {}
    dispose(): void {}
  },
}));
vi.mock("./record-timeline", () => ({
  RecordTimeline: class {
    constructor(_s: unknown) {}
    setDeferMode(): void {}
    insert(): void {}
    dispose(): void {}
    get size(): number {
      return 0;
    }
  },
}));
vi.mock("./branch-fold", () => ({
  BranchFolder: class {
    constructor(_el: unknown) {}
    setBatchMode(): void {}
    setDeferMode(): void {}
    flushPending(): void {}
    recordAdded(): void {}
    dispose(): void {}
  },
}));
vi.mock("./render-stream-record", () => ({ renderStreamRecord: vi.fn() }));
vi.mock("./cards", () => ({ reconcilePendingToolResults: vi.fn() }));
vi.mock("./cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("./tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { TabManager, type Tab } from "./tabs";

// 私有字段的只读探针（TS private 仅编译期；运行时可读）。仅测试用。
interface TMInternals {
  tabs: Map<string, Tab>;
  activeId: string | null;
  orderedIds: string[];
  pendingArchive: Set<string>;
}
const peek = (tm: TabManager): TMInternals => tm as unknown as TMInternals;

function makeTM(): TabManager {
  document.body.innerHTML = "";
  const barEl = document.createElement("div");
  const streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  return new TabManager(barEl, streamRootEl);
}

describe("TabManager 生命周期", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  it("ensureTab 默认建 live 本地 Tab（origin=null）", () => {
    const tab = tm.ensureTab("s1", "/home/u", "p", 0, null);
    expect(tab.status).toBe("live");
    expect(tab.origin).toBeNull();
    expect(peek(tm).tabs.has("s1")).toBe(true);
  });

  it("ensureTab 远端 Tab（origin!==null）也建成 live", () => {
    const tab = tm.ensureTab("s2", "/home", "p", 0, "pi");
    expect(tab.status).toBe("live");
    expect(tab.origin).toBe("pi");
  });

  // === Batch5-F18：骨架 Tab ===

  it("createSkeletonTab 建骨架；首行到达不重建、cwd/parentPath 回填", () => {
    tm.createSkeletonTab("sk1", "/root/proj", null);
    const skeleton = peek(tm).tabs.get("sk1")!;
    expect(skeleton.status).toBe("live");
    expect(skeleton.parentPath).toBe("");
    expect(skeleton.cwd).toBe("/root/proj");

    // 首条真实行：同一 Tab 实例（不重建），parentPath 回填、更小 seq 的 cwd 覆盖
    const after = tm.ensureTab("sk1", "/root/proj/sub", "/fake/sk1.jsonl", 3, null);
    expect(after).toBe(skeleton);
    expect(after.parentPath).toBe("/fake/sk1.jsonl");
    expect(after.cwd).toBe("/root/proj/sub"); // seq 3 < MAX_SAFE_INTEGER → 覆盖为行内 cwd
  });

  it("远端骨架（无 cwd）标题用 sid 前缀，重复宣告幂等", () => {
    tm.createSkeletonTab("deadbeef-1234", null, "pi");
    const t = peek(tm).tabs.get("deadbeef-1234")!;
    expect(t.origin).toBe("pi");
    expect(t.cwd).toBeNull();
    expect(t.title).toBe("[pi] deadbeef"); // 无 cwd → [host] + sid 前 8
    const before = peek(tm).tabs.size;
    tm.createSkeletonTab("deadbeef-1234", null, "pi"); // 重连重发 session_added
    expect(peek(tm).tabs.size).toBe(before);
    expect(peek(tm).tabs.get("deadbeef-1234")).toBe(t);
  });

  it("归档信号早于骨架建立：骨架落实 pendingArchive 为 archived", () => {
    tm.archiveTab("sk-late");
    tm.createSkeletonTab("sk-late", null, "pi");
    expect(peek(tm).tabs.get("sk-late")!.status).toBe("archived");
  });

  // === Batch5-F19：last-active 写回 ===

  it("switchTo 写回 last-active；persistLastActive=false（viewer）不写", () => {
    localStorage.removeItem("cc-monitor.last-active-sid");
    tm.ensureTab("s-a", "/a", "p", 0, null);
    tm.ensureTab("s-b", "/b", "p", 0, null);
    tm.switchTo("s-b");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");

    // viewer 模式：禁写（防独立窗口污染主窗口记忆，审计 R1）
    tm.persistLastActive = false;
    tm.switchTo("s-a");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");
  });

  it("archiveTab：live → archived，且清空 activity（灯灭）", () => {
    const tab = tm.ensureTab("s3", "/home", "p", 0, null);
    tab.activity = { status: "busy", waitingFor: null } as unknown as Tab["activity"];
    tm.archiveTab("s3");
    expect(tab.status).toBe("archived");
    expect(tab.activity).toBeNull();
  });

  it("归档信号早于 Tab 建立：进 pendingArchive，ensureTab 时落实归档", () => {
    tm.archiveTab("early");
    expect(peek(tm).pendingArchive.has("early")).toBe(true);
    const tab = tm.ensureTab("early", null, "p", 0, null);
    expect(tab.status).toBe("archived");
    expect(peek(tm).pendingArchive.has("early")).toBe(false);
  });

  it("reviveTab（本地）：archived → live，并清 pendingArchive", () => {
    const tab = tm.ensureTab("s4", "/x", "p", 0, null);
    tm.archiveTab("s4");
    expect(tab.status).toBe("archived");
    tm.reviveTab("s4");
    expect(tab.status).toBe("live");
  });

  it("reviveTab 不碰远端 Tab（origin!==null 门控）→ 仍 archived", () => {
    const tab = tm.ensureTab("s5", "/x", "p", 0, "pi");
    tm.archiveTab("s5");
    tm.reviveTab("s5");
    expect(tab.status).toBe("archived");
  });

  it("远端 Tab 掉线归档后再收到行（ensureTab）→ 见行复活成 live", () => {
    const tab = tm.ensureTab("s6", "/x", "p", 0, "pi");
    tm.archiveTab("s6");
    expect(tab.status).toBe("archived");
    tm.ensureTab("s6", "/x", "p", 1, "pi"); // daemon 重连重放
    expect(tab.status).toBe("live");
  });

  it("closeTab 拒关 live Tab（守卫：仅 archived 可关）", () => {
    tm.ensureTab("s7", "/x", "p", 0, null);
    tm.closeTab("s7");
    expect(peek(tm).tabs.has("s7")).toBe(true);
  });

  it("closeTab 关 archived Tab：移出 map + 摘 DOM", () => {
    const tab = tm.ensureTab("s8", "/x", "p", 0, null);
    const streamEl = tab.streamEl;
    expect(streamEl.parentElement).not.toBeNull();
    tm.archiveTab("s8");
    tm.closeTab("s8");
    expect(peek(tm).tabs.has("s8")).toBe(false);
    expect(streamEl.parentElement).toBeNull();
  });

  it("closeTab 通知后端 forget_session（archived 才关）", () => {
    tm.ensureTab("s9", "/x", "p", 0, null);
    tm.archiveTab("s9");
    tm.closeTab("s9");
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("forget_session", {
      sessionId: "s9",
    });
  });

  it("switchTo：切 active + 清 unread + 加 .active 类", () => {
    tm.ensureTab("s10", "/x", "p", 0, null); // 首个 → 自动 active
    const tab2 = tm.ensureTab("s11", "/y", "p", 0, null);
    tab2.unread = 5;
    expect(peek(tm).activeId).toBe("s10");
    tm.switchTo("s11");
    expect(peek(tm).activeId).toBe("s11");
    expect(tab2.unread).toBe(0);
    expect(tab2.streamEl.classList.contains("active")).toBe(true);
  });

  it("reviveTab 在 Tab 尚不存在时也清 pendingArchive（不误归档后续新 Tab）", () => {
    peek(tm).pendingArchive.add("s12");
    tm.reviveTab("s12"); // Tab 还没建
    expect(peek(tm).pendingArchive.has("s12")).toBe(false);
    const tab = tm.ensureTab("s12", "/x", "p", 0, null);
    expect(tab.status).toBe("live"); // 未被 pendingArchive 落实归档
  });
});
