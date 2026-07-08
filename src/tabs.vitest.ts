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
    batchInsert(fn: () => void): void {
      fn();
    }
    scrollToBottom(): void {}
    dispose(): void {}
  },
}));
vi.mock("./record-timeline", () => ({
  RecordTimeline: class {
    constructor(_s: unknown) {}
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
    flushPending(): void {}
    recordAdded(): void {}
    unwrapAll(): void {}
    rebuildNow(): void {}
    dispose(): void {}
  },
}));
// F40a:tabs.ts 消费两段式入口(routeMetaAndBranch 判 meta / renderContentRecord 建卡)。
// mock 按 message.type 粗判 consumed/content,与真实现语义对齐(防未来 meta 用例静默走错路)
vi.mock("./render-stream-record", () => ({
  routeMetaAndBranch: vi.fn((payload: { message?: { type?: string } }) =>
    ["ai-title", "custom-title", "queue-operation"].includes(payload.message?.type ?? "")
      ? "consumed"
      : "content",
  ),
  renderContentRecord: vi.fn(),
}));
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
  materializeQueue: string[];
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

  // === Batch7-F24：bg 会话树状 ===

  it("bg tab 挂到同 cwd 交互宿主之后（先宿主后 bg）", () => {
    tm.createSkeletonTab("host-a", "/proj/a", null, "interactive", null);
    tm.createSkeletonTab("other", "/proj/b", null, "interactive", null);
    tm.createSkeletonTab("bg-a1", "/proj/a", null, "bg", "评估任务");
    const order = peek(tm).orderedIds;
    expect(order).toEqual(["host-a", "bg-a1", "other"]);
    const bg = peek(tm).tabs.get("bg-a1")!;
    expect(bg.title).toBe("⚙ 评估任务");
  });

  it("孤儿 bg 先到、宿主后到 → 重锚到宿主之后", () => {
    tm.createSkeletonTab("bg-x1", "/proj/x", null, "bg", "t1");
    tm.createSkeletonTab("noise", "/proj/n", null, "interactive", null);
    tm.createSkeletonTab("host-x", "/proj/x", null, "interactive", null);
    expect(peek(tm).orderedIds).toEqual(["noise", "host-x", "bg-x1"]);
  });

  it("同 cwd 第二个交互宿主不搬走第一个宿主已挂的 bg 子串（多宿主取第一个）", () => {
    tm.createSkeletonTab("host-a1", "/proj/a", null, "interactive", null);
    tm.createSkeletonTab("bg-a1", "/proj/a", null, "bg", "t1");
    tm.createSkeletonTab("bg-a2", "/proj/a", null, "bg", "t2");
    tm.createSkeletonTab("host-a2", "/proj/a", null, "interactive", null);
    expect(peek(tm).orderedIds).toEqual(["host-a1", "bg-a1", "bg-a2", "host-a2"]);
  });

  it("远端 bg 带 origin 前缀且不跨 origin 认宿主", () => {
    tm.createSkeletonTab("h-local", "/p", null, "interactive", null);
    tm.createSkeletonTab("bg-remote", "/p", "pi", "bg", "远端任务");
    // origin 不同 → 不挂本地宿主，顶层追加
    expect(peek(tm).orderedIds).toEqual(["h-local", "bg-remote"]);
    expect(peek(tm).tabs.get("bg-remote")!.title).toBe("[pi] ⚙ 远端任务");
  });

  // === Batch8-F26：(sid,seq) 去重（快照/tail 重叠区缝合的前端锚点） ===

  it("同 (tab, seq) 的行第二次到达被 seenSeqs 吞掉（快照与 tail 重叠区）", async () => {
    const { renderContentRecord } = await import("./render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    const mkPayload = (seq: number, uuid: string) => ({
      session_id: "dup-sid",
      cwd: "/p",
      path: "/p/dup-sid.jsonl",
      seq,
      message: { type: "assistant", uuid } as never,
    });
    tm.onLine(mkPayload(7, "u-1") as never);
    const after1 = spy.mock.calls.length;
    tm.onLine(mkPayload(7, "u-1") as never); // 同 (sid,seq) 重复 → 去重
    expect(spy.mock.calls.length).toBe(after1);
    tm.onLine(mkPayload(8, "u-2") as never); // 新 seq → 放行
    expect(spy.mock.calls.length).toBeGreaterThan(after1);
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
    localStorage.removeItem("cc-monitor.last-active-sid"); // 防同文件后续测试顺序耦合
  });

  it("手动 switchTo 触发 onManualSwitch（迟到宣告不抢焦点的清 pending 钩子）", () => {
    tm.ensureTab("m-a", "/a", "p", 0, null);
    tm.ensureTab("m-b", "/b", "p", 0, null);
    let fired = 0;
    tm.onManualSwitch = () => fired++;
    tm.switchTo("m-b"); // 默认 manual
    expect(fired).toBe(1);
    tm.ensureTab("m-c", "/c", "p", 0, null); // 非首个 tab，不切换
    tm.switchTo("m-a", "auto"); // auto 不触发
    expect(fired).toBe(1);
    localStorage.removeItem("cc-monitor.last-active-sid");
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

  // === Batch13-F40a：尾部优先门控 / 物化（D 审计 C-3 补测） ===

  const mkContent = (sid: string, seq: number, uuid: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      message: { type: "assistant", uuid } as never,
    }) as never;

  async function spyRender() {
    const { renderContentRecord } = await import("./render-stream-record");
    return renderContentRecord as unknown as ReturnType<typeof vi.fn>;
  }

  it("F40a 门控矩阵：批期 active 首条钉 floor 直渲；后台恒收纳且不计 unread", async () => {
    const spy = await spyRender();
    spy.mockClear();
    tm.onLine(mkContent("act", 100, "a-1")); // 首个 tab → auto active
    tm.onBatchStart();
    tm.onLine(mkContent("act", 101, "a-2")); // active 批期直渲
    const actCalls = spy.mock.calls.length;
    expect(actCalls).toBeGreaterThanOrEqual(2);
    expect(peek(tm).tabs.get("act")!.window.floorSeq).toBe(100);

    tm.onLine(mkContent("bg", 50, "b-1")); // 后台 virgin：收纳
    tm.onLine(mkContent("bg", 51, "b-2"));
    const bg = peek(tm).tabs.get("bg")!;
    expect(spy.mock.calls.length).toBe(actCalls); // 没为后台建卡
    expect(bg.window.floorSeq).toBeNull();
    expect(bg.window.pendingCount).toBe(2);
    expect(bg.unread).toBe(0); // 收纳不计 unread（修 S-2）
  });

  it("F40a 门控：批后 seq<floor 收纳、seq≥floor 渲染", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("gate", 100, "g-1")); // !inBatch → 直渲并钉 floor=100
    const t = peek(tm).tabs.get("gate")!;
    expect(t.window.floorSeq).toBe(100);
    spy.mockClear();
    tm.onLine(mkContent("gate", 5, "g-old")); // F30 回填/迟到旧块 → 收纳
    expect(spy.mock.calls.length).toBe(0);
    expect(t.window.pendingCount).toBe(1);
    tm.onLine(mkContent("gate", 101, "g-2")); // live 追加 → 渲染
    expect(spy.mock.calls.length).toBe(1);
  });

  it("F40a C-1 竞态：近 virgin tab 的首条 live 行先物化账本再渲染（历史不滞留）", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("act2", 1, "x-1")); // active
    tm.onBatchStart();
    for (let s = 10; s < 15; s++) tm.onLine(mkContent("vg", s, `v-${s}`)); // 后台收纳 5 条
    tm.onBatchEnd();
    const vg = peek(tm).tabs.get("vg")!;
    expect(vg.window.pendingCount).toBe(5); // rIC 还没轮到（异步）
    spy.mockClear();
    tm.onLine(mkContent("vg", 99, "v-live")); // 真 live 行先到
    expect(vg.window.pendingCount).toBe(0); // 先物化（takeTail 钉 floor=10）
    expect(vg.window.floorSeq).toBe(10);
    expect(spy.mock.calls.length).toBe(6); // 5 条物化 + 1 条 live
  });

  it("F40a 物化顺序：unwrapAll → 渲染 → reconcile → rebuildNow", async () => {
    const spy = await spyRender();
    const { reconcilePendingToolResults } = await import("./cards");
    tm.onLine(mkContent("act3", 1, "y-1")); // active
    tm.onBatchStart();
    tm.onLine(mkContent("mat", 20, "m-1"));
    tm.onLine(mkContent("mat", 21, "m-2"));
    const mat = peek(tm).tabs.get("mat")!;
    const order: string[] = [];
    vi.spyOn(mat.branchFolder, "unwrapAll").mockImplementation(() => {
      order.push("unwrap");
    });
    vi.spyOn(mat.branchFolder, "rebuildNow").mockImplementation(() => {
      order.push("rebuild");
    });
    spy.mockImplementation(() => {
      order.push("render");
    });
    vi.mocked(reconcilePendingToolResults).mockImplementation(() => {
      order.push("reconcile");
    });
    tm.onBatchEnd();
    tm.switchTo("mat"); // virgin → 同步物化
    expect(order.join(",")).toContain("unwrap,render,render,reconcile,rebuild");
    spy.mockImplementation(() => {});
    vi.mocked(reconcilePendingToolResults).mockImplementation(() => {});
  });

  it("F40a S-5：archived tab 不进后台物化队列", () => {
    tm.onLine(mkContent("act4", 1, "z-1")); // active
    tm.onBatchStart();
    tm.onLine(mkContent("dead", 30, "d-1")); // 后台收纳
    tm.onLine(mkContent("idle1", 40, "i-1")); // 后台收纳 ×2
    tm.onLine(mkContent("idle2", 41, "i-2"));
    tm.archiveTab("dead");
    tm.onBatchEnd();
    // scheduleIdleMaterialize 同步 shift 队首进定时器闭包 → 队列只剩第二个非归档 tab;
    // 关键断言:dead 从未入队(队首 shift 的是 idle1)
    expect(peek(tm).materializeQueue).toEqual(["idle2"]);
    expect(peek(tm).tabs.get("dead")!.window.pendingCount).toBe(1); // 未被物化
  });

  it("F40a meta 记录批期被消费不进账本", () => {
    tm.onLine(mkContent("act5", 1, "w-1")); // active
    tm.onBatchStart();
    tm.onLine({
      session_id: "meta-bg",
      cwd: "/p",
      path: "/p/meta-bg.jsonl",
      seq: 60,
      message: { type: "ai-title", aiTitle: "标题" } as never,
    } as never);
    const t = peek(tm).tabs.get("meta-bg")!;
    expect(t.window.pendingCount).toBe(0); // consumed,不收纳
    expect(t.window.floorSeq).toBeNull();
  });
});
