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
    /** 测试可写:R-1 中部插入判定读它(真实现=最高已渲染 seq) */
    _maxSeq = Number.NEGATIVE_INFINITY;
    insert(e: { seq: number }): void {
      // 闭合「直渲推高 maxSeq → 老块落缓冲」反馈链(D 审计 S-4)
      this._maxSeq = Math.max(this._maxSeq, e.seq);
    }
    removeByElement(): void {}
    dispose(): void {}
    get size(): number {
      return 0;
    }
    get maxSeq(): number {
      return this._maxSeq;
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
vi.mock("./cards", () => ({
  reconcilePendingToolResults: vi.fn(() => []),
  // A5：镜像真 isCompactRecord（真身在 cards/compact.vitest.ts 单测）——role:user + compact 前缀。
  isCompactRecord: (m: unknown) => {
    const inner = (m as { message?: { role?: unknown; content?: unknown } } | null)?.message;
    if (!inner || inner.role !== "user") return false;
    const c = inner.content;
    const text = typeof c === "string" ? c : "";
    return text
      .trimStart()
      .startsWith("This session is being continued from a previous conversation");
  },
}));
vi.mock("./cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("./tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("./error-toast", () => ({ showActionFailureToast: vi.fn() }));
// Batch14-F41：resumeTab 远端分支改走一键拉起 runner；behavior 提供 launcher 配置。
vi.mock("./remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
// Batch14-F42：turn-end 通知与渲染独立,tabs 测试里 mock 成空壳(单独在 turn-notify.vitest 测)。
vi.mock("./turn-notify", () => ({
  turnEndNotifier: { observe: vi.fn() },
}));
vi.mock("./behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({
    resumeCommandLocal: "",
    resumeCommandRemote: "cct",
  }),
}));
// A5：换号重启编排（单测在 account-restart.vitest）——这里 mock 成 spy，只验 tabs 侧守卫是否放行。
vi.mock("./account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000, // tabs.ts awaitExitFor 默认参用；mock 需导出，否则 undefined
}));

import { invoke } from "@tauri-apps/api/core";
import { restartWithAccount } from "./account-restart";
import { runRemoteResume, runRemoteResumeTmux, runRemoteAttach } from "./remote-launch-run";
import { TabManager, findClaudeTmux, isCwdFallbackMatch, claudeExited, type Tab } from "./tabs";

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

  // === issue #63①：fork 血缘徽标 ===

  it("#63① fork 会话:首条带 forkedFrom → 标题加 ↳ 徽标 + tab 记录来源 sid", () => {
    tm.onLine({
      session_id: "fork-sid",
      cwd: "/home/u/proj",
      path: "/home/u/proj/fork-sid.jsonl",
      seq: 0,
      message: { type: "user", uuid: "u1", forkedFrom: { sessionId: "parent-abcd1234", messageUuid: "m1" } },
    } as never);
    const tab = peek(tm).tabs.get("fork-sid")!;
    expect(tab.forkedFromSessionId).toBe("parent-abcd1234");
    expect(tab.title.startsWith("↳ ")).toBe(true); // ★区分:未修则不加徽标
  });

  it("#63① 非 fork 会话不加徽标(区分性)", () => {
    tm.onLine({
      session_id: "plain",
      cwd: "/home/u/proj",
      path: "/home/u/proj/plain.jsonl",
      seq: 0,
      message: { type: "user", uuid: "u2" },
    } as never);
    const tab = peek(tm).tabs.get("plain")!;
    expect(tab.forkedFromSessionId).toBeNull();
    expect(tab.title.startsWith("↳")).toBe(false);
  });

  it("#63① fork + 后到的 aiTitle → 单个 ↳(不重复叠加,pin 掉 doubling)", () => {
    tm.onLine({
      session_id: "f3",
      cwd: "/home/u/proj",
      path: "/home/u/proj/f3.jsonl",
      seq: 0,
      message: { type: "user", uuid: "a", forkedFrom: { sessionId: "parent-x" } },
    } as never);
    const tab = peek(tm).tabs.get("f3")!;
    // 直接驱动私有 applyAiTitle(routeMetaAndBranch 被 mock、不会触发 sink);模拟 ai-title 后到。
    (tm as unknown as { applyAiTitle(t: Tab, s: string): void }).applyAiTitle(tab, "我的功能");
    expect(tab.title).toBe("↳ [proj] 我的功能"); // 恰一个 ↳、且 aiTitle 合成正确
    expect((tab.title.match(/↳/g) ?? []).length).toBe(1);
  });

  it("#63① 远端 fork:↳ 在 [origin] 之外(↳ [pi] …)", () => {
    tm.onLine({
      session_id: "fr",
      cwd: "/home/u/proj",
      path: "/home/u/proj/fr.jsonl",
      seq: 0,
      origin: "pi",
      message: { type: "user", uuid: "a", forkedFrom: { sessionId: "parent-remote" } },
    } as never);
    expect(peek(tm).tabs.get("fr")!.title.startsWith("↳ [pi] ")).toBe(true);
  });

  it("#63① tooltip 标出来源 sid(唯一暴露 parent sid 的地方)", () => {
    tm.onLine({
      session_id: "ft",
      cwd: "/home/u/proj",
      path: "/home/u/proj/ft.jsonl",
      seq: 0,
      message: { type: "user", uuid: "a", forkedFrom: { sessionId: "abcd1234-parent" } },
    } as never);
    // tab 按钮渲染进 barEl → 其 title 属性含血缘行(前 8 位 sid)
    const el = document.body.querySelector<HTMLElement>('[title*="从 abcd1234 fork 而来"]');
    expect(el).not.toBeNull();
  });

  it("#63① forkedFrom 出现一次即锁定,后续记录不覆盖(同 aiTitle)", () => {
    tm.onLine({
      session_id: "f2",
      cwd: "/p",
      path: "/p/f2.jsonl",
      seq: 0,
      message: { type: "user", uuid: "a", forkedFrom: { sessionId: "first-parent" } },
    } as never);
    tm.onLine({
      session_id: "f2",
      cwd: "/p",
      path: "/p/f2.jsonl",
      seq: 1,
      message: { type: "user", uuid: "b", forkedFrom: { sessionId: "SHOULD-NOT-WIN" } },
    } as never);
    expect(peek(tm).tabs.get("f2")!.forkedFromSessionId).toBe("first-parent");
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

  it("F91 红绿灯状态转移清陈旧类（activityLightClass 重构守护：两 toggle 每次都跑）", () => {
    // 守 F91 把 tab-bar 红绿灯抽到 session-status.ts 后仍逐字节等价：状态转移必须清掉旧的
    // 对立类（若哪天把两个 classList.toggle 之一改成条件执行，本测会红）。
    tm.ensureTab("lt", "/x", "p", 0, null);
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    tm.updateActivity("lt", "waiting", "permission prompt");
    expect(btn().classList.contains("act-waiting")).toBe(true);
    expect(btn().classList.contains("act-idle")).toBe(false);
    // waiting → idle：陈旧 act-waiting 必须清、换 act-idle
    tm.updateActivity("lt", "idle", null);
    expect(btn().classList.contains("act-waiting")).toBe(false);
    expect(btn().classList.contains("act-idle")).toBe(true);
    // idle → busy：两类都清（默认绿点）
    tm.updateActivity("lt", "busy", null);
    expect(btn().classList.contains("act-idle")).toBe(false);
    expect(btn().classList.contains("act-waiting")).toBe(false);
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
      return [];
    });
    tm.onBatchEnd();
    tm.switchTo("mat"); // virgin → 同步物化
    expect(order.join(",")).toContain("unwrap,render,render,reconcile,rebuild");
    spy.mockImplementation(() => {});
    vi.mocked(reconcilePendingToolResults).mockImplementation(() => []);
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

  // === Batch13-F40b：R-1 缓冲 / 上翻补批 / 哨兵 ===

  it("F40b R-1：批期窗口内中部插入缓冲不渲,onBatchEnd 排序一次挂载;后台照计 unread", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("r1a", 1, "ra-1")); // active,floor=1
    tm.onLine(mkContent("r1b", 100, "rb-1")); // 后台?否——第二个 tab 不自动切,live 直渲钉 floor=100
    const bg = peek(tm).tabs.get("r1b")!;
    expect(peek(tm).activeId).toBe("r1a");
    (bg.timeline as unknown as { _maxSeq: number })._maxSeq = 500; // 已渲染到 seq 500
    tm.onBatchStart();
    spy.mockClear();
    tm.onLine(mkContent("r1b", 300, "rb-mid1")); // ≥floor 且 <maxSeq → 缓冲
    tm.onLine(mkContent("r1b", 200, "rb-mid2"));
    expect(spy.mock.calls.length).toBe(0);
    expect(bg.midBatchBuffer.map((p) => p.seq)).toEqual([300, 200]);
    expect(bg.unread).toBe(2); // 离线期真新消息照计
    tm.onLine(mkContent("r1b", 600, "rb-tail")); // >maxSeq…但 mock maxSeq 恒 500 → 600≥500?
    // 600 > maxSeq(500) → 不缓冲,直渲
    expect(spy.mock.calls.length).toBe(1);
    tm.onBatchEnd(); // flush:排序后一次挂载
    const seqs = spy.mock.calls.slice(1).map((c) => (c[0] as { seq: number }).seq);
    expect(seqs).toEqual([200, 300]);
    expect(bg.midBatchBuffer.length).toBe(0);
  });

  it("F40b fill：R-2 切入踢链 + 选区守卫 + 非 active 不触发", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("fillA", 1, "fa-1")); // active
    tm.onBatchStart();
    for (let s = 100; s < 800; s++) tm.onLine(mkContent("fills", s, `fs-${s}`)); // 后台收纳 700 条
    tm.onBatchEnd();
    const t = peek(tm).tabs.get("fills")!;
    expect(t.window.pendingCount).toBe(700);

    // 选区进行中切入:virgin 物化照走(4 轮×150=600,物化不看选区);
    // R-2 踢链(不可滚+账余 100)被选区守卫挡 → 账保 100
    const selSpy = vi
      .spyOn(document, "getSelection")
      .mockReturnValue({ isCollapsed: false } as unknown as Selection);
    tm.switchTo("fills");
    expect(t.window.pendingCount).toBe(100);
    // 选区仍在:scroll 也被挡
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(t.window.pendingCount).toBe(100);
    // 选区收起:scroll → 补批弹尽
    selSpy.mockReturnValue({ isCollapsed: true } as unknown as Selection);
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(t.window.pendingCount).toBe(0);
    selSpy.mockRestore();

    // 非 active:先切走,再给 fills(后台,floor 已钉)造残账——onBatchEnd 的
    // 「active 不足一屏补物化」只作用于 active(fillA,无账),后台账保留
    tm.switchTo("fillA");
    tm.onBatchStart();
    tm.onLine(mkContent("fills", 50, "fs-old")); // seq<floor → 收纳
    tm.onBatchEnd();
    expect(t.window.pendingCount).toBe(1);
    spy.mockClear();
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(t.window.pendingCount).toBe(1); // 非 active,未补
    expect(spy.mock.calls.length).toBe(0);
  });

  it("F40b fill：renderingFill 防重入——补批渲染中同步再触发 scroll 不嵌套", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("reA", 1, "re-1")); // active
    tm.onBatchStart();
    for (let s = 1000; s < 1300; s++) tm.onLine(mkContent("reB", s, `re-${s}`)); // 后台 300
    tm.onBatchEnd();
    const t = peek(tm).tabs.get("reB")!;
    tm.switchTo("reB"); // virgin 全物化(300≤600),floor=1000
    expect(t.window.pendingCount).toBe(0);

    tm.switchTo("reA");
    tm.onBatchStart();
    for (let s = 100; s < 400; s++) tm.onLine(mkContent("reB", s, `re-old-${s}`)); // <floor 收纳 300
    tm.onBatchEnd();
    expect(t.window.pendingCount).toBe(300);

    // 选区挡住切入时的 R-2 踢链,保住账本
    const selSpy = vi
      .spyOn(document, "getSelection")
      .mockReturnValue({ isCollapsed: false } as unknown as Selection);
    tm.switchTo("reB");
    expect(t.window.pendingCount).toBe(300);
    selSpy.mockReturnValue({ isCollapsed: true } as unknown as Selection);
    spy.mockImplementation(() => {
      t.streamEl.dispatchEvent(new Event("scroll")); // 渲染中同步重入
    });
    t.streamEl.dispatchEvent(new Event("scroll"));
    // 只弹一批 200(嵌套触发被 renderingFill 挡;若守卫失效会连弹到 0)
    expect(t.window.pendingCount).toBe(100);
    spy.mockImplementation(() => {});
    selSpy.mockRestore();
  });

  it("F40b：物化/补批 sink 不接 onRealUserInput(历史 user 卡不自动切 tab)", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("uaA", 1, "ua-1")); // active
    tm.onBatchStart();
    for (let s = 10; s < 13; s++) tm.onLine(mkContent("uaB", s, `ub-${s}`));
    tm.onBatchEnd();
    spy.mockClear();
    tm.switchTo("uaB"); // 物化 3 条
    const materializeCalls = spy.mock.calls.filter((c) => (c[0] as { seq: number }).seq >= 10);
    expect(materializeCalls.length).toBe(3);
    for (const c of materializeCalls) {
      expect((c[2] as { onRealUserInput?: unknown }).onRealUserInput).toBeUndefined();
    }
    // 对照:live 路径的 sink 带 onRealUserInput
    tm.onLine(mkContent("uaB", 99, "ub-live"));
    const liveCall = spy.mock.calls[spy.mock.calls.length - 1];
    expect((liveCall[2] as { onRealUserInput?: unknown }).onRealUserInput).toBeTypeOf("function");
  });

  it("F40b 哨兵：账本非空显示剩余条数,补尽消失", async () => {
    await spyRender();
    tm.onLine(mkContent("sentA", 1, "sa-1")); // active
    tm.onBatchStart();
    for (let s = 100; s < 300; s++) tm.onLine(mkContent("sentB", s, `sb-${s}`));
    tm.onBatchEnd();
    tm.switchTo("sentB"); // 物化(4 轮×150 上限 → 200 全弹尽)
    const t = peek(tm).tabs.get("sentB")!;
    expect(t.window.pendingCount).toBe(0);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();

    // 再造残账(先切走,防 onBatchEnd 的 active 补物化清账):哨兵文本准确
    tm.switchTo("sentA");
    tm.onBatchStart();
    for (let s = 10; s < 15; s++) tm.onLine(mkContent("sentB", s, `sb-old-${s}`)); // <floor 收纳
    tm.onBatchEnd();
    // 直接刷哨兵验证文本(不切入——jsdom 恒不可滚,切入会走 R-2 踢链清账)
    (tm as unknown as { updateSentinel(x: Tab): void }).updateSentinel(t);
    const sentinel = t.stream.contentElement.querySelector(".stream-more-above");
    expect(sentinel?.textContent).toContain("5 条更早消息");
    // 切入:R-2 踢链(不可滚+账本有余)→ 补批到账尽 → 哨兵消失
    tm.switchTo("sentB");
    expect(t.window.pendingCount).toBe(0);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();
  });

  // === v2.22.2:同 sid kind 冲突消解(bg-spare 谎报父 sid) ===

  it("kind 升格:bg 骨架先到,interactive 宣告后到 → 升格为宿主并重锚孤儿 bg", () => {
    // 场景还原(用户截图):bg-spare 的宣告先到,父会话被建成 ⚙ 挂到同 cwd 的
    // 别的交互会话(Excel)之下;interactive 宣告后到必须升格纠正。
    tm.createSkeletonTab("excel", "/proj/shengwu", null, "interactive", null);
    tm.createSkeletonTab("parent", "/proj/shengwu", null, "bg", "迁移服务"); // 谎报形态先到
    tm.createSkeletonTab("fork-empty", "/proj/shengwu", null, "bg", "迁移服务"); // 空克隆
    expect(peek(tm).orderedIds).toEqual(["excel", "parent", "fork-empty"]);
    expect(peek(tm).tabs.get("parent")!.title).toContain("⚙");

    tm.createSkeletonTab("parent", "/proj/shengwu", null, "interactive", null); // 真身宣告后到
    const p = peek(tm).tabs.get("parent")!;
    expect(p.kind).toBe("interactive");
    expect(p.title).not.toContain("⚙");
    // parent 升格为宿主:提出子树位、追加为交互 tab,孤儿 bg(fork-empty 原挂
    // excel 子串)不被搬走——「多宿主取第一个」契约保持(excel 仍是先到宿主)
    expect(peek(tm).orderedIds).toEqual(["excel", "fork-empty", "parent"]);
  });

  it("kind 不降格:interactive tab 后到 bg 宣告(spare 谎报)保持交互形态", () => {
    tm.createSkeletonTab("host2", "/proj/x", null, "interactive", null);
    tm.createSkeletonTab("host2", "/proj/x", null, "bg", "spare 噪声");
    const t = peek(tm).tabs.get("host2")!;
    expect(t.kind).toBe("interactive");
    expect(t.title).not.toContain("⚙");
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

describe("F41 resumeTab：远端一键拉起 / 本地不变", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  it("远端归档 tab → runRemoteResume(origin, sid, cwd, launcher)", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await (tm as unknown as { resumeTab(sid: string): Promise<void> }).resumeTab("r1");
    // A4：默认 resume（无账号）→ 第 5 参 configDir=undefined（不注入，行为与旧版等价）。
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", undefined);
    expect(invoke).not.toHaveBeenCalledWith("resume_history_session", expect.anything());
  });

  it("A4：resumeTab 带账号名但账号库不可用 → withAccount 退化默认 resume（不注入、不记账）", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    // tabs.vitest 的 invoke 默认返 undefined → fetchAccounts 视作不可用 → withAccount 退化默认。
    // （带账号注入 + 记 lastAccount 的正路在 accounts.vitest 的 withAccount 套件覆盖。）
    await (
      tm as unknown as { resumeTab(sid: string, accountName?: string): Promise<void> }
    ).resumeTab("r1", "z");
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", undefined);
    expect(invoke).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
  });

  it("本地归档 tab → 仍走 resume_history_session，不碰远端 runner", async () => {
    tm.ensureTab("l1", "/home/u/p", "/p/l1.jsonl", 0, null);
    tm.archiveTab("l1");
    await (tm as unknown as { resumeTab(sid: string): Promise<void> }).resumeTab("l1");
    expect(runRemoteResume).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("resume_history_session", {
      sessionId: "l1",
      cwd: "/home/u/p",
      launcher: null,
    });
  });
});

describe("F51 tab 右键 attach 反查（异步就绪 + 跨 tab 竞态守卫 R-1）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    tm = makeTM();
  });

  interface TMButtons {
    tabButtons: Map<string, { root: HTMLElement }>;
  }
  const btnOf = (sid: string): HTMLElement =>
    (tm as unknown as TMButtons).tabButtons.get(sid)!.root;
  const rightClick = (sid: string): void => {
    btnOf(sid).dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }),
    );
  };
  const attachBtn = (): HTMLButtonElement | null => {
    const menu = document.body.querySelector(".tab-context-menu");
    const items = [...(menu?.querySelectorAll(".tab-context-menu-item") ?? [])];
    return (
      (items as HTMLButtonElement[]).find((b) => b.textContent?.startsWith("Attach")) ?? null
    );
  };
  const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

  it("远端 tab 右键 → 反查命中 claude 会话 → attach 项由禁用占位就绪为可点", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-abc", path: "/a", command: "claude", attached: false, windows: 1 },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("A", "/a", "p", 0, "hostA");
    rightClick("A");
    expect(attachBtn()?.disabled).toBe(true); // 占位「检测中」
    await flush();
    expect(attachBtn()?.textContent).toContain("cc-abc"); // 就绪
    expect(attachBtn()?.disabled).toBe(false);
  });

  it("前台命令报 node（claude 是 Node CLI）也认(D-Sug2)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "sess", path: "/a", command: "node", attached: true, windows: 2 },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("A", "/a", "p", 0, "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()?.textContent).toContain("sess");
  });

  it("无匹配（cwd 不符）→ 占位移除,不显示 attach", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "other", path: "/elsewhere", command: "claude", attached: false, windows: 1 },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("A", "/a", "p", 0, "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()).toBeNull();
  });

  it("R-1 守卫:tab A 查询在飞时右键 tab B → A 迟到结果不污染 B 的菜单", async () => {
    let resolveA!: (v: unknown) => void;
    const aPending = new Promise((r) => (resolveA = r));
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd !== "list_remote_tmux") return Promise.resolve(undefined);
      const origin = (args as { origin: string }).origin;
      if (origin === "hostA") return aPending; // 在飞
      return Promise.resolve([
        { name: "B-sess", path: "/b", command: "claude", attached: false, windows: 1 },
      ]);
    });
    tm.ensureTab("A", "/a", "p", 0, "hostA");
    tm.ensureTab("B", "/b", "p", 0, "hostB");

    rightClick("A"); // 菜单 A + A 查询在飞
    rightClick("B"); // 关 A、开菜单 B（新代次）+ B 查询即刻 resolve
    await flush();
    expect(attachBtn()?.textContent).toContain("B-sess"); // B 自身反查就绪

    resolveA([
      { name: "A-sess", path: "/a", command: "claude", attached: false, windows: 1 },
    ]);
    await flush(); // A 迟到:代次不符 → 整体 no-op,不动 B 菜单
    expect(attachBtn()?.textContent).toContain("B-sess");
    expect(attachBtn()?.textContent).not.toContain("A-sess");
  });
});

describe("F52 归档远端 tab 右键：Resume 直连 + tmux 并列", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    tm = makeTM();
  });

  interface TMButtons {
    tabButtons: Map<string, { root: HTMLElement }>;
  }
  const rightClick = (sid: string): void => {
    (tm as unknown as TMButtons).tabButtons
      .get(sid)!
      .root.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }),
      );
  };
  const menuLabels = (): string[] =>
    [...(document.body.querySelector(".tab-context-menu")?.querySelectorAll(".tab-context-menu-item") ?? [])].map(
      (b) => b.textContent ?? "",
    );
  const clickItem = (label: string): void => {
    const btn = [
      ...(document.body.querySelector(".tab-context-menu")?.querySelectorAll(".tab-context-menu-item") ?? []),
    ].find((b) => b.textContent === label) as HTMLButtonElement | undefined;
    btn?.click();
  };

  it("归档远端 tab → 「Resume（直连）」+「Resume（tmux）」并列", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    const labels = menuLabels();
    expect(labels).toContain("Resume（直连）");
    expect(labels).toContain("Resume（tmux）");
    // tmux 项 → 先查 list_remote_tmux(默认 mock 返 undefined = 无活会话)→ 起全新 resume,
    // 带第 5 个不撞名 name="cc-r1"(F74:灰会话 resume 不复用可能漂移的名)。
    clickItem("Resume（tmux）");
    await flushMicro();
    await flushMicro();
    // account-ux U3：归档 tmux resume 也走 withAccount follow → 第 6 参 configDir（空 mock → undefined）。
    expect(runRemoteResumeTmux).toHaveBeenCalledWith(
      "aya",
      "r1",
      "/home/pi/proj",
      "cct",
      "cc-r1",
      undefined,
    );
    // 直连项 → runRemoteResume
    rightClick("r1");
    clickItem("Resume（直连）");
    await flushMicro();
    // A4：默认 resume（无账号）→ 第 5 参 configDir=undefined（不注入，行为与旧版等价）。
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", undefined);
  });

  it("F74 Resume（tmux）:@ccm_sid 命中活会话 → 精确 attach 它(不撞同目录漂移分支),不重开", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            // 同目录两个 claude:漂移分支(sid 不符,且列在前)+ 目标原会话(sid 命中)。
            { name: "proj_cc-2", path: "/home/pi/proj", command: "claude", attached: false, windows: 1, sid: "branch99" },
            { name: "proj_cc", path: "/home/pi/proj", command: "claude", attached: true, windows: 1, sid: "r1" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    clickItem("Resume（tmux）");
    await flushMicro();
    await flushMicro();
    // 精确 attach 到 sid 命中的 proj_cc——不是列在前面的漂移分支 proj_cc-2;且不走 resume。
    expect(runRemoteAttach).toHaveBeenCalledWith("aya", "proj_cc");
    expect(runRemoteResumeTmux).not.toHaveBeenCalled();
  });

  it("F74 Resume（tmux）:@ccm_sid 已知但无一命中(原名被漂移会话占着)→ 起全新 resume 挑不撞名", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-r1", path: "/home/pi/proj", command: "claude", attached: true, windows: 1, sid: "drift77" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    clickItem("Resume（tmux）");
    await flushMicro();
    await flushMicro();
    expect(runRemoteAttach).not.toHaveBeenCalled();
    // cc-r1 被漂移会话占着 → 挑 cc-r1-2 新建,保证 --resume r1 落进原会话。
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "cc-r1-2", undefined);
  });

  it("F74 Resume（tmux）:老 wrapper(整表无 @ccm_sid)→ 起全新 fresh resume,不 attach 不确定会话", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            // 老 wrapper:同 cwd 有 claude 但无 sid 信息(sid:null)。
            { name: "proj_cc", path: "/home/pi/proj", command: "claude", attached: true, windows: 1, sid: null },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    clickItem("Resume（tmux）");
    await flushMicro();
    await flushMicro();
    // findClaudeTmux 按 cwd 兜底命中 proj_cc,但 live.sid(null)!==sid → **不 attach 不确定的会话**,
    // 起 fresh resume(cc-r1 未被占 → 基名);--resume r1 恒落对会话(§30「找不到就别静默换」)。
    expect(runRemoteAttach).not.toHaveBeenCalled();
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "cc-r1", undefined);
  });

  it("归档本地 tab → 仍单「Resume」(无 tmux 项)", () => {
    tm.ensureTab("l1", "/home/u/p", "p", 0, null);
    tm.archiveTab("l1");
    rightClick("l1");
    const labels = menuLabels();
    expect(labels).toContain("Resume");
    expect(labels).not.toContain("Resume（tmux）");
    expect(labels).not.toContain("Resume（直连）");
  });
});

const flushMicro = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

describe("F74 findClaudeTmux（精确 tmux↔sid 映射）", () => {
  const S = (name: string, path: string, command: string, sid: string | null) => ({
    name,
    path,
    command,
    attached: false,
    windows: 1,
    sid,
  });
  it("优先 @ccm_sid 精确匹配（同目录多 claude，命中 sid 的那个，无关列出顺序）", () => {
    const list = [S("a", "/p", "claude", "branch9"), S("b", "/p", "claude", "target")];
    expect(findClaudeTmux(list, "target", "/p")?.name).toBe("b");
  });
  it("sid 已知但无一命中 → undefined（绝不按 cwd 抓同目录别的 claude，SS-5/SS-9）", () => {
    const list = [S("a", "/p", "claude", "other")];
    expect(findClaudeTmux(list, "target", "/p")).toBeUndefined();
  });
  it("整张列表无 @ccm_sid（老 wrapper / 未装）→ 回退 path===cwd 匹配（向后兼容）", () => {
    const list = [S("a", "/p", "claude", null)];
    expect(findClaudeTmux(list, "target", "/p")?.name).toBe("a");
  });
  it("回退分支仍要 claude 命令 + cwd 非空", () => {
    expect(findClaudeTmux([S("a", "/p", "zsh", null)], "t", "/p")).toBeUndefined();
    expect(findClaudeTmux([S("a", "/p", "claude", null)], "t", "")).toBeUndefined();
  });
  it("null / 空列表 → undefined", () => {
    expect(findClaudeTmux(null, "t", "/p")).toBeUndefined();
    expect(findClaudeTmux([], "t", "/p")).toBeUndefined();
  });
});

describe("F74c(#60-B) isCwdFallbackMatch（cwd 回退串味提示判定）", () => {
  const S = (name: string, path: string, command: string, sid: string | null) => ({
    name,
    path,
    command,
    attached: false,
    windows: 1,
    sid,
  });
  it("精确 @ccm_sid 命中 → false（非回退，不提示）", () => {
    expect(isCwdFallbackMatch([S("b", "/p", "claude", "target")], "target")).toBe(false);
  });
  it("有会话带 sid 但无一命中 → false（findClaudeTmux 返 undefined、不 attach、无串味）", () => {
    expect(isCwdFallbackMatch([S("a", "/p", "claude", "other")], "target")).toBe(false);
  });
  it("整张列表无 @ccm_sid（老 wrapper/未装）→ true（会走 cwd 回退，attach 前提示）", () => {
    expect(isCwdFallbackMatch([S("a", "/p", "claude", null)], "target")).toBe(true);
  });
  it("null / 空列表 → true（无 sid 可依，回退语义）", () => {
    expect(isCwdFallbackMatch(null, "t")).toBe(true);
    expect(isCwdFallbackMatch([], "t")).toBe(true);
  });
});

describe("A5+ claudeExited（优雅退出检测：目标 sid 前台是否不再是 claude）", () => {
  const S = (name: string, path: string, command: string, sid: string | null) => ({
    name,
    path,
    command,
    attached: false,
    windows: 1,
    sid,
  });
  it("目标 sid 仍精确命中 claude → 未退出(false)", () => {
    expect(claudeExited([S("b", "/p", "claude", "target")], "target", "/p")).toBe(false);
  });
  it("目标会话前台回到 shell（@ccm_sid 犹在但命令变 zsh）→ 已退出(true)", () => {
    expect(claudeExited([S("b", "/p", "zsh", "target")], "target", "/p")).toBe(true);
  });
  it("目标会话已消失（列表里只剩别的 sid）→ 已退出(true)", () => {
    expect(claudeExited([S("a", "/p", "claude", "other")], "target", "/p")).toBe(true);
  });
  it("空列表 → 已退出(true)", () => {
    expect(claudeExited([], "target", "/p")).toBe(true);
  });
  it("cwd 回退命中的是别的 claude（无任何 @ccm_sid）→ live.sid=null!==target → 已退出(true)", () => {
    // 与破坏性重启守卫一致：cwd 回退命中 sid=null → 不当成目标会话仍活。
    expect(claudeExited([S("a", "/p", "claude", null)], "target", "/p")).toBe(true);
  });
});

describe("F79 杀死远端 tmux 会话（二次确认 + kill_remote_tmux）", () => {
  beforeEach(() => vi.clearAllMocks());
  type KillTM = {
    killRemoteTmux(origin: string, tmuxName: string, viaCwd: boolean): void;
  };
  it("二次确认通过 → invoke kill_remote_tmux（origin/target 正确，变灰由 #60-A 兜、不主动 archive）", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const tm = makeTM() as unknown as KillTM;
    tm.killRemoteTmux("hostA", "cc-abc", false);
    await Promise.resolve();
    const call = vi.mocked(invoke).mock.calls.find((c) => c[0] === "kill_remote_tmux");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ origin: "hostA", target: "cc-abc" });
    confirmSpy.mockRestore();
  });
  it("二次确认取消 → 不 invoke", () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    const tm = makeTM() as unknown as KillTM;
    tm.killRemoteTmux("hostA", "cc-abc", false);
    expect(
      vi.mocked(invoke).mock.calls.some((c) => c[0] === "kill_remote_tmux"),
    ).toBe(false);
    confirmSpy.mockRestore();
  });
  it("F79 审计修复：cwd 回退命中（viaCwd）→ 二次确认加强 caveat（可能杀同目录别的会话）", () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    const tm = makeTM() as unknown as KillTM;
    tm.killRemoteTmux("hostA", "cc-abc", true);
    const msg = String(confirmSpy.mock.calls[0]?.[0] ?? "");
    expect(msg).toContain("@ccm_sid"); // 未检测到身份标记
    expect(msg).toContain("同目录"); // 可能杀同目录别的 Claude
    confirmSpy.mockRestore();
  });
});

describe("F70 会话改动集聚合（onLine → touchedFiles / touchedFilesFor 门控）", () => {
  const editLine = (
    sid: string,
    seq: number,
    uuid: string,
    filePath: string,
    origin: string | null,
    toolName = "Edit",
  ): unknown => ({
    session_id: sid,
    cwd: "/proj",
    path: `/proj/${sid}.jsonl`,
    seq,
    origin,
    message: {
      type: "assistant",
      uuid,
      // 真实 jsonl 记录：content 在 message.message.content（trackAgents/collectEditedFiles 同款读法）。
      message: {
        content: [{ type: "tool_use", name: toolName, input: { file_path: filePath } }],
      },
    },
  });

  it("本地会话：写类工具 file_path 累进 + 去重；touchedFilesFor 返 files", () => {
    const tm = makeTM();
    tm.onLine(editLine("s-local", 1, "u1", "/proj/a.ts", null) as never);
    tm.onLine(editLine("s-local", 2, "u2", "/proj/b.rs", null, "Write") as never);
    tm.onLine(editLine("s-local", 3, "u3", "/proj/a.ts", null) as never); // 重复文件 → 去重
    const info = tm.touchedFilesFor("s-local");
    expect(info).not.toBeNull();
    expect(info!.origin).toBeNull();
    expect([...info!.files].sort()).toEqual(["/proj/a.ts", "/proj/b.rs"]);
  });

  it("远端会话（origin!==null）→ touchedFilesFor 返 null（门控，代码不在本机）", () => {
    const tm = makeTM();
    tm.onLine(editLine("s-remote", 1, "r1", "/proj/x.ts", "aya") as never);
    expect(tm.touchedFilesFor("s-remote")).toBeNull();
  });

  it("非写类工具不计入；无此会话 → null", () => {
    const tm = makeTM();
    tm.onLine(editLine("s-read", 1, "k1", "/proj/r.ts", null, "Read") as never);
    // Read 不是写类 → 空集，但 tab 存在（本地有 cwd）→ 返 files:[]
    expect(tm.touchedFilesFor("s-read")?.files).toEqual([]);
    expect(tm.touchedFilesFor("does-not-exist")).toBeNull();
  });
});

describe("F77 getActiveSubagentContext", () => {
  it("活跃本地 tab → { parentPath(=sourcePath), origin:null }", () => {
    const tm = makeTM();
    tm.ensureTab("s1", "/home/u", "/p/s1.jsonl", 0, null);
    tm.switchTo("s1");
    expect(tm.getActiveSubagentContext()).toEqual({
      parentPath: "/p/s1.jsonl",
      origin: null,
    });
  });
  it("活跃远端 tab → origin 非空（调用方据此提示不支持）", () => {
    const tm = makeTM();
    tm.ensureTab("s2", "/home", "/p/s2.jsonl", 0, "pi");
    tm.switchTo("s2");
    expect(tm.getActiveSubagentContext()?.origin).toBe("pi");
  });
  it("无活跃 tab → null", () => {
    const tm = makeTM();
    expect(tm.getActiveSubagentContext()).toBeNull();
  });
  it("活跃 tab 无 parentPath（骨架未回填）→ null", () => {
    const tm = makeTM();
    tm.createSkeletonTab("sk", "/root/proj", null); // parentPath 空
    tm.switchTo("sk");
    expect(tm.getActiveSubagentContext()).toBeNull();
  });
});

describe("F91b TabManager.peekSession（监控板内容 peek 纯读派生）", () => {
  const agent = (label: string, status: "running" | "done" | "aborted") => ({
    id: `id-${label}`,
    label,
    agentType: null,
    status,
    timestamp: "2026-07-17T00:00:00Z",
    desc: label,
  });

  it("unknown sid → null", () => {
    const tm = makeTM();
    expect(tm.peekSession("nope")).toBeNull();
  });

  it("运行中 subagent 排在前，同档保插入序；model / 改过的文件透传", () => {
    const tm = makeTM();
    const tab = tm.ensureTab("s1", "/proj", "/p/s1.jsonl", 0, null);
    tab.latestModel = "claude-opus-4-8";
    tab.touchedFiles.add("/proj/a.ts");
    tab.touchedFiles.add("/proj/b.ts");
    // 插入序：done, running, aborted, running —— 期望 running 提前、组内保插入序
    tab.agents.set("1", agent("done1", "done"));
    tab.agents.set("2", agent("run1", "running"));
    tab.agents.set("3", agent("abort1", "aborted"));
    tab.agents.set("4", agent("run2", "running"));

    const p = tm.peekSession("s1");
    expect(p).not.toBeNull();
    expect(p!.model).toBe("claude-opus-4-8");
    expect(p!.recentFiles).toEqual(["/proj/a.ts", "/proj/b.ts"]);
    // running 全部提前且组内保序；非 running 组内也保插入序
    expect(p!.agents.map((a) => a.label)).toEqual(["run1", "run2", "done1", "abort1"]);
    expect(p!.agents.map((a) => a.status)).toEqual(["running", "running", "done", "aborted"]);
  });

  it("无 usage / 无 agent / 无改文件 → 字段空但不报错", () => {
    const tm = makeTM();
    tm.ensureTab("s2", "/x", "/p/s2.jsonl", 0, null);
    const p = tm.peekSession("s2");
    expect(p).toEqual({ model: null, recentFiles: [], agents: [] });
  });

  // F91b-fix(batch18)：touchedFiles 近因序——re-touch 的文件经 onLine delete+add 移到末尾，
  // 使 peek `recentFiles`（= [...touchedFiles]）尾部是「最近改的」。锁住此行为，防重构退回插入序静默显错文件。
  it("touchedFiles 近因序：onLine 重触文件移到末尾（peek recentFiles 尾=最近改）", () => {
    const tm = makeTM();
    // collectEditedFiles 读 payload.message.message.content（外层 type=记录类型，内层 message=API 消息体）
    const edit = (seq: number, uuid: string, files: string[]) =>
      ({
        session_id: "s",
        cwd: "/p",
        path: "/p/s.jsonl",
        seq,
        message: {
          type: "assistant",
          uuid,
          message: {
            content: files.map((f) => ({ type: "tool_use", name: "Edit", input: { file_path: f } })),
          },
        },
      }) as never;
    tm.onLine(edit(1, "e1", ["/a.ts", "/b.ts", "/c.ts"]));
    expect(tm.peekSession("s")!.recentFiles).toEqual(["/a.ts", "/b.ts", "/c.ts"]);
    tm.onLine(edit(2, "e2", ["/a.ts"])); // 重触 a → 移到末尾（近因序）
    expect(tm.peekSession("s")!.recentFiles).toEqual(["/b.ts", "/c.ts", "/a.ts"]);
    tm.onLine(edit(3, "e3", ["/d.ts", "/b.ts"])); // 新增 d、重触 b → b 也移末尾
    expect(tm.peekSession("s")!.recentFiles).toEqual(["/c.ts", "/a.ts", "/d.ts", "/b.ts"]);
  });
});

describe("A5 compact waiter（awaitCompactFor + onLine 检测）", () => {
  const PREFIX = "This session is being continued from a previous conversation";
  type Priv = { awaitCompactFor(sid: string, ms?: number): () => Promise<boolean> };
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  const compactLine = (sid: string) => ({
    session_id: sid,
    cwd: "/w",
    path: `/p/${sid}.jsonl`,
    seq: 1,
    message: { type: "user", uuid: `${sid}-u1`, message: { role: "user", content: `${PREFIX}…` } },
  });

  it("注册后 onLine 见该 sid 的 compact 摘要行 → resolve(true)", async () => {
    const awaitC = (tm as unknown as Priv).awaitCompactFor("cs1", 60_000);
    const p = awaitC(); // 注册 waiter
    tm.onLine(compactLine("cs1") as never);
    await expect(p).resolves.toBe(true);
  });

  it("超时 → resolve(false)", async () => {
    vi.useFakeTimers();
    try {
      const awaitC = (tm as unknown as Priv).awaitCompactFor("cs2", 5000);
      const p = awaitC();
      vi.advanceTimersByTime(5000);
      await expect(p).resolves.toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("非 compact 行不 resolve（等待者仍挂着）", async () => {
    const awaitC = (tm as unknown as Priv).awaitCompactFor("cs3", 60_000);
    let resolved = false;
    void awaitC().then(() => {
      resolved = true;
    });
    tm.onLine({
      session_id: "cs3",
      cwd: "/w",
      path: "/p/cs3.jsonl",
      seq: 1,
      message: { type: "user", uuid: "cs3-u", message: { role: "user", content: "普通消息" } },
    } as never);
    await Promise.resolve();
    expect(resolved).toBe(false);
  });

  it("别的 sid 的 compact 行不误 resolve 本 waiter", async () => {
    const awaitC = (tm as unknown as Priv).awaitCompactFor("cs4", 60_000);
    let resolved = false;
    void awaitC().then(() => {
      resolved = true;
    });
    tm.onLine(compactLine("other-sid") as never); // 不同 sid
    await Promise.resolve();
    expect(resolved).toBe(false);
  });
});

describe("A5 restartTabWithAccount 阻塞守卫（精确 @ccm_sid 命中才动手）", () => {
  const restartSpy = restartWithAccount as unknown as ReturnType<typeof vi.fn>;
  type Priv = { restartTabWithAccount(sid: string, name: string, c: boolean): Promise<void> };
  let tm: TabManager;
  const sess = (over: Record<string, unknown>) => ({
    name: "cc-abc12345",
    path: "/home/pi/proj",
    command: "claude",
    attached: false,
    windows: 1,
    sid: null,
    ...over,
  });
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  it("cwd 回退命中（live.sid !== sid）→ 拒重启、不调编排器（防杀错会话/双进程）", async () => {
    tm.ensureTab("target-sid", "/home/pi/proj", "/p/t.jsonl", 0, "aya");
    // 同 cwd 但无 @ccm_sid（sid:null）→ findClaudeTmux 走 cwd 回退 → live.sid=null !== target-sid
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux" ? Promise.resolve([sess({ sid: null })]) : Promise.resolve(undefined),
    );
    await (tm as unknown as Priv).restartTabWithAccount("target-sid", "z", false);
    expect(restartSpy).not.toHaveBeenCalled();
  });

  it("精确 @ccm_sid 命中 → 放行调编排器（带对的 tmuxName/account）", async () => {
    tm.ensureTab("target-sid", "/home/pi/proj", "/p/t.jsonl", 0, "aya");
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([sess({ name: "cc-target01", sid: "target-sid" })])
        : Promise.resolve(undefined),
    );
    await (tm as unknown as Priv).restartTabWithAccount("target-sid", "z", true);
    expect(restartSpy).toHaveBeenCalledTimes(1);
    const arg = restartSpy.mock.calls[0][0];
    expect(arg.tmuxName).toBe("cc-target01");
    expect(arg.accountName).toBe("z");
    expect(arg.sessionId).toBe("target-sid");
    expect(arg.compactFirst).toBe(true);
  });

  it("会话不在任何 tmux → 拒重启、不调编排器", async () => {
    tm.ensureTab("target-sid", "/home/pi/proj", "/p/t.jsonl", 0, "aya");
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux" ? Promise.resolve([]) : Promise.resolve(undefined),
    );
    await (tm as unknown as Priv).restartTabWithAccount("target-sid", "z", false);
    expect(restartSpy).not.toHaveBeenCalled();
  });
});

describe("account-ux U5 tab 徽章「信息才显」", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  const badge = (): HTMLElement | null =>
    document.body.querySelector<HTMLElement>(".tab-acct-badge");
  const liveRow = (sid: string, account: string) => ({
    pid: 1,
    sessionId: sid,
    cwd: "/w",
    configDir: `/h/${account}`,
    account,
    bare: false,
    alive: true,
  });
  // setSessionAccounts(rows, emailByName, lastAccountByS, readyOrigins, currentByOrigin)
  function feed(
    rows: ReturnType<typeof liveRow>[],
    last: Map<string, string>,
    current: Map<string, string>,
  ): void {
    tm.setSessionAccounts(rows, new Map(), last, new Set(["aya"]), current);
  }

  it("会话账号 != 当前工作账号(live) → 挂实心头像", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([liveRow("r1", "b")], new Map(), new Map([["aya", "z"]]));
    const el = badge();
    expect(el?.style.display).not.toBe("none");
    expect(el?.querySelector(".acct-avatar")).not.toBeNull();
    expect(el?.querySelector(".acct-avatar.ghost")).toBeNull(); // live = 实心
  });
  it("会话账号 == 当前工作账号 → 不挂徽章", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([liveRow("r1", "z")], new Map(), new Map([["aya", "z"]]));
    expect(badge()?.style.display).toBe("none");
  });
  it("lastAccount 软来源且 != 当前 → 幽灵头像", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([], new Map([["r1", "b"]]), new Map([["aya", "z"]]));
    const av = badge()?.querySelector(".acct-avatar");
    expect(av).not.toBeNull();
    expect(av?.classList.contains("ghost")).toBe(true);
  });
  it("未知账号(无 live 无 last) → 不挂徽章", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([], new Map(), new Map([["aya", "z"]]));
    expect(badge()?.style.display).toBe("none");
  });
  it("当前工作账号未就绪(currentByOrigin 无该 origin) → 不猜、不挂", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([liveRow("r1", "b")], new Map(), new Map()); // 无 current
    expect(badge()?.style.display).toBe("none");
  });
});
