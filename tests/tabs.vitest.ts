// Tab 生命周期状态机的 DOM 单元测试（vitest + jsdom）。
//
// 为什么需要它：本轮迭代两个真 bug 都在 tabs.ts 的生命周期里——
//   1) resume 后已归档**本地** Tab 不自动复活（reviveTab + origin 门控）；
//   2) 关 Tab 只允许 archived（closeTab 守卫）。
// tabs.ts 重度依赖 DOM + 一堆协作模块（MessageStream / RecordTimeline / BranchFolder /
// 渲染 / Tauri IPC），无法像现有 *.test.ts 那样在裸 node 里测。这里用 jsdom 提供真 DOM、
// 把重协作者 mock 成空壳，于是能在真 TabManager 实例上断言状态翻转。

import { type Mock, describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ★ audit-0805 F15 第 1 步：**先让「每行调了几次」变得可测**。
//
// 这个文件此前把 `BranchFolder` 整个 stub 成空壳（每个方法 no-op），于是全仓**没有任何东西**
// 能证明 live 模式下每来一行调了几次 —— 而 V5 已经指出这条性质「行为上与不改完全等价
// （同样的行、同样的结果）」，**慢不会让任何测试变红**。
// ⇒ 不先建量具就动性能，改完无从证明改对了。计数器挂在既有 stub 上，成本近零。
//
// ⚠⚠ **但本轮实测：光有计数器还建不起判据** —— 见 `audit-0805/features/F15-*.md §2`。
//   `routeMetaAndBranch` 在本文件里被 mock 成**不调 `sink.onBranchRecord`**（`:77-84`），
//   于是 `recordAdded` 在这套 mock 下**永远是 0**。
//   ★ **让 tabs.ts 可测的那批 mock，恰好把「调了几次」这件事也 mock 没了。**
//   计数器先留着（零成本、零行为影响），判据要等那批 mock 被改成「保真到调用次数」那一层。
const f15 = vi.hoisted(() => ({
  recordAdded: 0,
  rebuildNow: 0,
  /**
   * ★ `renderContentRecord` 的**保真默认实现**（往 timeline 里塞一条）。
   *
   * 提到这里是因为文件里有两处测试会临时改这个 mock 的实现，改完再「恢复」——
   * 而它们此前恢复成的是空 `() => {}`，**把保真实现永久打回空壳**，
   * 于是后面所有测试里 `tabs.ts:891` 的 `inserted` 都恒假。
   * 这是 F15 §2 那个族的第三次：**量具被别处悄悄改没了，而且看起来一切正常**。
   * ⇒ 恢复一律用本函数，别再手写 `() => {}`（那不是「恢复」，是「换成另一种坏」）。
   */
  insertingRender: (
    payload: { seq: number },
    _ctx: unknown,
    sink?: { timeline?: { insert?: (e: unknown) => void } },
  ) => {
    sink?.timeline?.insert?.({
      seq: payload.seq,
      element: document.createElement("div"),
      kind: "card",
      toolGroup: null,
    });
  },
}));

// --- 把重/IPC 协作者 mock 掉，让 TabManager 能在 jsdom 下实例化（避免拉 marked/katex/IPC）---
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../src/stream", () => ({
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
vi.mock("../src/record-timeline", () => ({
  RecordTimeline: class {
    constructor(_s: unknown) {}
    /** 测试可写:R-1 中部插入判定读它(真实现=最高已渲染 seq) */
    _maxSeq = Number.NEGATIVE_INFINITY;
    /**
     * ★ F15 第 2 步：**`size` 必须真会涨**。
     *
     * 此前 `size` 恒返回 0 ⇒ `tabs.ts:891` 的 `inserted`（`timeline.size > beforeSize`）
     * **恒假** ⇒ 后台 tab 那条 `refreshTabBar()`（`:896`）在本文件的 mock 下**永远走不到**。
     * 于是「后台 tab 每来一行刷一次 tab bar」这条性质**无法被任何判据看见** ——
     * 那正是 F15 §2 记的第二种成因：**量具把它测没了**（不是没人写判据）。
     * 这里只补「进来几条」这一个维度，不复刻真实现的去重/排序（那是 record-timeline 自己的单测）。
     */
    _seqs = new Set<number>();
    insert(e: { seq: number }): void {
      // 闭合「直渲推高 maxSeq → 老块落缓冲」反馈链(D 审计 S-4)
      this._maxSeq = Math.max(this._maxSeq, e.seq);
      this._seqs.add(e.seq);
    }
    removeByElement(): void {}
    dispose(): void {}
    get size(): number {
      return this._seqs.size;
    }
    get maxSeq(): number {
      return this._maxSeq;
    }
  },
}));
vi.mock("../src/branch-fold", () => ({
  BranchFolder: class {
    constructor(_el: unknown) {}
    setBatchMode(): void {}
    flushPending(): void {}
    // F15：计数，不做事 —— 量的是「被调了几次」，不是它做了什么。
    recordAdded(): void {
      f15.recordAdded++;
    }
    unwrapAll(): void {}
    rebuildNow(): void {
      f15.rebuildNow++;
    }
    dispose(): void {}
  },
}));
// F40a:tabs.ts 消费两段式入口(routeMetaAndBranch 判 meta / renderContentRecord 建卡)。
// mock 按 message.type 粗判 consumed/content,与真实现语义对齐(防未来 meta 用例静默走错路)
vi.mock("../src/render-stream-record", () => ({
  // ★ F15 第 1 步：mock 必须**真调 `sink.onBranchRecord`**。
  //
  // 此前它只返回 `"consumed"`/`"content"`、一次都不碰 sink ⇒ `tab.branchFolder.recordAdded`
  // （`tabs.ts:810` 挂在 sink 上）在本文件里**永远是 0 次** ⇒ 「live 每来一行喂一次
  // BranchFolder」这条性质**无法被任何判据看见**。
  // 这里镜像真实现的**分支判定**（`branching.ts:282-291`：类型在集合内 + 有 uuid + 有 timestamp），
  // 不复刻它构造出的 `BranchRecord` 内容 —— 本文件量的是**次数**，不是那条记录长什么样。
  routeMetaAndBranch: vi.fn(
    (
      payload: {
        message?: { type?: string; uuid?: string | null; timestamp?: string | null };
      },
      sink?: { onBranchRecord?: (rec: unknown) => void },
    ) => {
      const m = payload.message;
      const t = m?.type ?? "";
      if (["ai-title", "custom-title", "queue-operation"].includes(t)) return "consumed";
      const isBranchKind = [
        "user",
        "assistant",
        "attachment",
        "system",
        "cc-monitor-unrecognized",
      ].includes(t);
      if (isBranchKind && m?.uuid && m?.timestamp) {
        sink?.onBranchRecord?.({ uuid: m.uuid, parentUuid: null, timestamp: m.timestamp });
      }
      return "content";
    },
  ),
  // ★ F15 第 3 步：mock 必须**真往 timeline 里塞**。
  //
  // 上面把 `RecordTimeline.size` 改成真会涨之后还不够 —— 真正调 `timeline.insert` 的是
  // 这个函数（`render-stream-record.ts:173-179` 的 `card` 分支），而它此前是个空 `vi.fn()`。
  // 三处 mock 里少任何一处，`tabs.ts:891` 的 `inserted` 都恒假。
  // ⇒ 「量具把它测没了」这件事**是三处叠出来的**，只修一处会让人以为修完了却仍然零命中。
  // 这里只塞「一条记录 = 一个 seq」，不复刻真实现的 tool-group 合并（那有自己的单测）。
  renderContentRecord: vi.fn(f15.insertingRender),
}));
vi.mock("../src/cards", () => ({
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
vi.mock("../src/cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("../src/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
// Batch14-F41：resumeTab 远端分支改走一键拉起 runner；behavior 提供 launcher 配置。
vi.mock("../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
// Batch14-F42：turn-end 通知与渲染独立,tabs 测试里 mock 成空壳(单独在 turn-notify.vitest 测)。
vi.mock("../src/turn-notify", () => ({
  turnEndNotifier: { observe: vi.fn() },
}));
vi.mock("../src/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({
    resumeCommandLocal: "",
    resumeCommandRemote: "cct",
  }),
}));
// A5：换号重启编排（单测在 account-restart.vitest）——这里 mock 成 spy，只验 tabs 侧守卫是否放行。
vi.mock("../src/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000, // tabs.ts awaitExitFor 默认参用；mock 需导出，否则 undefined
}));

import { invoke } from "@tauri-apps/api/core";
import { sessionReadCalls, withSessionReads } from "./test-support/chan-fake";
import { restartWithAccount } from "../src/account-restart";
import { invalidateAccountsCache } from "../src/accounts";
import { showActionFailureToast } from "../src/error-toast";
import { __setHostOsForTests, type HostOs } from "../src/settings/host-os";
import {
  runRemoteResume,
  runRemoteResumeTmux,
  runRemoteResumeIntoExistingTmux,
  runRemoteAttach,
} from "../src/remote-launch-run";
import {
  TabManager,
  findClaudeTmux,
  findClaudeTmuxMatches,
  findIdleTmux,
  isCwdFallbackMatch,
  claudeExited,
  moveTabBlock,
  applyDropToCollections,
  collectionsEqual,
  commonDirName,
  defaultGroupName,
  pickDropTarget,
  tabUnderY,
  DWELL_MS,
  DWELL_MOVE_PX,
  type Tab,
  type TabRect,
} from "../src/tabs";
import type { TabCollection } from "../src/tab-collections";
import { ENDED, GONE, LIVE, LIVE_ATTACHABLE, LIVE_RESUMABLE, RECONNECTABLE, UNSEEN } from "../src/tab-session-state";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import type { TabStore } from "../src/tab-store";
import type { TabBarView } from "../src/tab-bar-view";
import type { TabBarDrag } from "../src/tab-bar-drag";
import type { TabBarPrefs } from "../src/tab-bar-prefs";
import type { TabStreamView } from "../src/tab-stream-view";
import type { TabSessionActions } from "../src/tab-session-actions";
import { LOCAL_ORIGIN } from "../src/ipc/origin";

// 〔S4 · 第四波〕`TabManager` 拆开之后各样东西住各自的家（store · tab 栏视图 · 拖拽 · 落盘偏好 · 流视图 · 会话动作）。
// 判据**直接指向新家**；`TabManager` 上不再为旧判据留同名转交。TS 的 `private` 只在编译期，运行时这几个字段就在实例上。仅测试用。
interface TMHomes {
  store: TabStore;
  bar: TabBarView;
  dragger: TabBarDrag;
  prefs: TabBarPrefs;
  view: TabStreamView;
  actions: TabSessionActions;
}
const home = (tm: TabManager): TMHomes => tm as unknown as TMHomes;

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

  it("ensureTab 默认建 live 本地 Tab（origin = `LOCAL_ORIGIN`）", () => {
    const tab = tm.ensureTab("s1", "/home/u", "p", 0, LOCAL_ORIGIN);
    expect(tab.state).toEqual(LIVE);
    expect(tab.origin).toBe(LOCAL_ORIGIN);
    expect(home(tm).store.tabs.has("s1")).toBe(true);
  });

  it("ensureTab 远端 Tab（origin!==null）也建成 live", () => {
    const tab = tm.ensureTab("s2", "/home", "p", 0, "pi");
    expect(tab.state).toEqual(LIVE);
    expect(tab.origin).toBe("pi");
  });

  // === Batch5-F18：骨架 Tab ===

  it("createSkeletonTab 建骨架；首行到达不重建、cwd/parentPath 回填", () => {
    tm.createSkeletonTab("sk1", "/root/proj", LOCAL_ORIGIN);
    const skeleton = home(tm).store.tabs.get("sk1")!;
    expect(skeleton.state).toEqual(LIVE);
    expect(skeleton.parentPath).toBe("");
    expect(skeleton.cwd).toBe("/root/proj");

    // 首条真实行：同一 Tab 实例（不重建），parentPath 回填、更小 seq 的 cwd 覆盖
    const after = tm.ensureTab("sk1", "/root/proj/sub", "/fake/sk1.jsonl", 3, LOCAL_ORIGIN);
    expect(after).toBe(skeleton);
    expect(after.parentPath).toBe("/fake/sk1.jsonl");
    expect(after.cwd).toBe("/root/proj/sub"); // seq 3 < MAX_SAFE_INTEGER → 覆盖为行内 cwd
  });

  it("远端骨架（无 cwd）标题用 sid 前缀，重复宣告幂等", () => {
    tm.createSkeletonTab("deadbeef-1234", null, "pi");
    const t = home(tm).store.tabs.get("deadbeef-1234")!;
    expect(t.origin).toBe("pi");
    expect(t.cwd).toBeNull();
    expect(t.title).toBe("[pi] deadbeef"); // 无 cwd → [host] + sid 前 8
    const before = home(tm).store.tabs.size;
    tm.createSkeletonTab("deadbeef-1234", null, "pi"); // 重连重发 session_added
    expect(home(tm).store.tabs.size).toBe(before);
    expect(home(tm).store.tabs.get("deadbeef-1234")).toBe(t);
  });

  it("归档信号早于骨架建立：骨架落实 pendingArchive 为 archived", () => {
    tm.archiveTab("sk-late");
    tm.createSkeletonTab("sk-late", null, "pi");
    expect(home(tm).store.tabs.get("sk-late")!.state).toEqual(ENDED);
  });

  // === Batch7-F24：bg 会话树状 ===

  it("bg tab 挂到同 cwd 交互宿主之后（先宿主后 bg）", () => {
    tm.createSkeletonTab("host-a", "/proj/a", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("other", "/proj/b", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("bg-a1", "/proj/a", LOCAL_ORIGIN, "bg", "评估任务");
    const order = home(tm).store.orderedIds;
    expect(order).toEqual(["host-a", "bg-a1", "other"]);
    const bg = home(tm).store.tabs.get("bg-a1")!;
    expect(bg.title).toBe("⚙ 评估任务");
  });

  it("孤儿 bg 先到、宿主后到 → 重锚到宿主之后", () => {
    tm.createSkeletonTab("bg-x1", "/proj/x", LOCAL_ORIGIN, "bg", "t1");
    tm.createSkeletonTab("noise", "/proj/n", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("host-x", "/proj/x", LOCAL_ORIGIN, "interactive", null);
    expect(home(tm).store.orderedIds).toEqual(["noise", "host-x", "bg-x1"]);
  });

  it("同 cwd 第二个交互宿主不搬走第一个宿主已挂的 bg 子串（多宿主取第一个）", () => {
    tm.createSkeletonTab("host-a1", "/proj/a", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("bg-a1", "/proj/a", LOCAL_ORIGIN, "bg", "t1");
    tm.createSkeletonTab("bg-a2", "/proj/a", LOCAL_ORIGIN, "bg", "t2");
    tm.createSkeletonTab("host-a2", "/proj/a", LOCAL_ORIGIN, "interactive", null);
    expect(home(tm).store.orderedIds).toEqual(["host-a1", "bg-a1", "bg-a2", "host-a2"]);
  });

  it("远端 bg 带 origin 前缀且不跨 origin 认宿主", () => {
    tm.createSkeletonTab("h-local", "/p", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("bg-remote", "/p", "pi", "bg", "远端任务");
    // origin 不同 → 不挂本地宿主，顶层追加
    expect(home(tm).store.orderedIds).toEqual(["h-local", "bg-remote"]);
    expect(home(tm).store.tabs.get("bg-remote")!.title).toBe("[pi] ⚙ 远端任务");
  });

  // === Batch8-F26：(sid,seq) 去重（快照/tail 重叠区缝合的前端锚点） ===

  it("同 (tab, seq) 的行第二次到达被 seenSeqs 吞掉（快照与 tail 重叠区）", async () => {
    const { renderContentRecord } = await import("../src/render-stream-record");
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
    const tab = home(tm).store.tabs.get("fork-sid")!;
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
    const tab = home(tm).store.tabs.get("plain")!;
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
    const tab = home(tm).store.tabs.get("f3")!;
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
    expect(home(tm).store.tabs.get("fr")!.title.startsWith("↳ [pi] ")).toBe(true);
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
    expect(home(tm).store.tabs.get("f2")!.forkedFromSessionId).toBe("first-parent");
  });

  // === Batch5-F19：last-active 写回 ===

  it("switchTo 写回 last-active；persistLastActive=false（viewer）不写", () => {
    localStorage.removeItem("cc-monitor.last-active-sid");
    tm.ensureTab("s-a", "/a", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("s-b", "/b", "p", 0, LOCAL_ORIGIN);
    tm.switchTo("s-b");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");

    // viewer 模式：禁写（防独立窗口污染主窗口记忆，审计 R1）
    tm.persistLastActive = false;
    tm.switchTo("s-a");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");
    localStorage.removeItem("cc-monitor.last-active-sid"); // 防同文件后续测试顺序耦合
  });

  it("手动 switchTo 触发 onManualSwitch（迟到宣告不抢焦点的清 pending 钩子）", () => {
    tm.ensureTab("m-a", "/a", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("m-b", "/b", "p", 0, LOCAL_ORIGIN);
    let fired = 0;
    tm.onManualSwitch = () => fired++;
    tm.switchTo("m-b"); // 默认 manual
    expect(fired).toBe(1);
    tm.ensureTab("m-c", "/c", "p", 0, LOCAL_ORIGIN); // 非首个 tab，不切换
    tm.switchTo("m-a", "auto"); // auto 不触发
    expect(fired).toBe(1);
    localStorage.removeItem("cc-monitor.last-active-sid");
  });

  it("archiveTab：live → archived，且清空 activity（灯灭）", () => {
    const tab = tm.ensureTab("s3", "/home", "p", 0, LOCAL_ORIGIN);
    tab.activity = { status: "busy", waitingFor: null } as unknown as Tab["activity"];
    tm.archiveTab("s3");
    expect(tab.state).toEqual(ENDED);
    expect(tab.activity).toBeNull();
  });

  it("F91 红绿灯状态转移清陈旧类（activityLightClass 重构守护：两 toggle 每次都跑）", () => {
    // 守 F91 把 tab-bar 红绿灯抽到 session-status.ts 后仍逐字节等价：状态转移必须清掉旧的
    // 对立类（若哪天把两个 classList.toggle 之一改成条件执行，本测会红）。
    tm.ensureTab("lt", "/x", "p", 0, LOCAL_ORIGIN);
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

  it("★ 活动信号早于 Tab 建立：远端骨架 Tab 建出来时灯必须已经是对的", () => {
    // ⚠ **这条以前一个用例都没有**，而它是远端会话的**真实时序**：
    // `remote-session-added` 与 `session-activity` 是两个**独立**的 Tauri 事件，
    // 谁先到没有保证。灯先到时它进 `pendingActivity`，只有 `ensureTab` 会落实它。
    //
    // 不测这条的后果**恰好是最坏的那种**：`activityLightClass(null)` 返回 `""`
    //（默认绿点，见 `session-status.ts` 头注「`busy` / `null` → 默认绿点」）——
    // ⇒ 这一跳一旦断掉，UI 显示的是**最让人安心的颜色**，而不是错误状态。
    // 一个"全绿"的界面既可能是"都在忙"，也可能是"灯一条都没到"，肉眼分不出。
    //
    // 08-14 排一个「Windows 前端会话全是绿灯」的实机现象时，这条链被逐跳读过一遍
    // （backend 发初始 status → monitor `status_changed` → `session-activity` →
    // `updateActivity` → `pendingActivity` → `ensureTab` → `activityLightClass`），
    // **每一跳都是通的**，但**没有任何一条判据在守整条链**。这一格补的就是最险的那一跳。
    // ⚠ 只断言**可观察行为**，不戳内部 `pendingActivity`：那个字段当年不在探针接口（今天的 `TMHomes`）上，
    // 戳它会让本文件 `tsc --noEmit` 红（第一版就是这么写的，当场被基线 tsc 逮住）。
    // 而且断行为本来就更强 —— 它不关心暂存用什么数据结构实现。
    tm.updateActivity("early-light", "idle", null);
    // 远端骨架 Tab 走 createSkeletonTab → ensureTab（顺序不能反：它必须落实暂存的灯）
    tm.createSkeletonTab("early-light", "/proj", "aya", null, null);
    const btn = document.querySelector<HTMLElement>(".tab")!;
    expect(
      btn.classList.contains("act-idle"),
      "灯先到、Tab 后建 ⇒ 建出来就该是红的。现在是默认绿 —— " +
        "`pendingActivity` 没被 `ensureTab` 落实，而默认值是绿，所以这个洞不会自己暴露。",
    ).toBe(true);
    // 落实之后再来一次同值信号不该出问题（幂等）。
    tm.updateActivity("early-light", "idle", null);
    expect(btn.classList.contains("act-idle")).toBe(true);
  });

  it("归档信号早于 Tab 建立：进 pendingArchive，ensureTab 时落实归档", () => {
    tm.archiveTab("early");
    expect(home(tm).store.pendingArchive.has("early")).toBe(true);
    const tab = tm.ensureTab("early", null, "p", 0, LOCAL_ORIGIN);
    expect(tab.state).toEqual(ENDED);
    expect(home(tm).store.pendingArchive.has("early")).toBe(false);
  });

  it("reviveTab（本地）：archived → live，并清 pendingArchive", () => {
    const tab = tm.ensureTab("s4", "/x", "p", 0, LOCAL_ORIGIN);
    tm.archiveTab("s4");
    expect(tab.state).toEqual(ENDED);
    tm.reviveTab("s4");
    expect(tab.state).toEqual(LIVE);
  });

  it("reviveTab 不碰远端 Tab（origin!==null 门控）→ 仍 archived", () => {
    const tab = tm.ensureTab("s5", "/x", "p", 0, "pi");
    tm.archiveTab("s5");
    tm.reviveTab("s5");
    expect(tab.state).toEqual(ENDED);
  });

  // ── audit-fixes F03.2：可重连（claude 退、tmux 在）生命周期 ──
  // 〔U4〕原先这一组断言 `tmuxIdle` ＋ `status`（可重连时 status 仍 live）；两轴之后直接断 `state`。
  it("F03.2 markTmuxIdle：进可重连 —— 死 ＋ 容器还在（不是已结束）", () => {
    const tab = tm.ensureTab("gi1", "/x", "p", 0, "pi");
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    tm.markTmuxIdle("gi1");
    expect(tab.state).toEqual(RECONNECTABLE);
    expect(btn().classList.contains("reconnectable")).toBe(true);
    expect(btn().classList.contains("ended")).toBe(false);
  });

  it("〔U4〕tab 的 tooltip 第一行说状态（只从两轴派生）；死了的会话不再挂陈旧的「等待操作」", () => {
    tm.ensureTab("tt1", "/x", "p", 0, "pi");
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    tm.updateActivity("tt1", "waiting", "permission prompt");
    expect(btn().title, "活着：不说状态，只说在等什么").toBe("等待操作：permission prompt");
    tm.markTmuxIdle("tt1"); // 活动信号还留着（可重连不清它），但 claude 已经没了
    expect(btn().title).toBe("程序退了，终端还在 —— 可以接回去");
    tm.archiveTab("tt1");
    expect(btn().title).toBe("这个会话已结束");
    expect([btn().classList.contains("ended"), btn().classList.contains("reconnectable")]).toEqual([true, false]);
  });

  it("〔U4 · S6〕e2e 探针真吐的行 ↔ graylight-suite.sh 的两条 grep（shell 语料 vs 运行输出，两向）", () => {
    // 套件里那两条模式（ERE）原样取出来：`GRAY="$(wait_log "$MARK" "<模式>" …` / `ARCH=…`。
    const suite = readFileSync(resolve(REPO_ROOT, "tests/e2e/graylight-suite.sh"), "utf8");
    const pat = (name: string): string => {
      const m = new RegExp(`^${name}="\\$\\(wait_log "\\$MARK" "([^"]+)"`, "m").exec(suite);
      expect(m, `套件里找不到 ${name} 那条 wait_log —— 抽取器或套件变了`).not.toBeNull();
      return m![1];
    };
    const sid = "0123abcd-e2e0-4000-8000-000000000000";
    // shell 双引号里 `\[` 原样留着反斜杠、`$SID8` 展开成前 8 位 ⇒ 这就是 grep -E 真拿到的模式。
    const ere = (name: string): RegExp => new RegExp(pat(name).replace("$SID8", sid.slice(0, 8)));
    const lines: string[] = [];
    const spy = vi.spyOn(console, "info").mockImplementation((l: unknown) => {
      if (typeof l === "string" && l.startsWith("[e2e] tab-state")) lines.push(l);
    });
    try {
      tm.ensureTab(sid, "/x", "p", 0, "pi"); // 活（建 tab 不打探针）
      tm.markTmuxIdle(sid); // → 可重连
      tm.archiveTab(sid); // → 已结束
    } finally {
      spy.mockRestore();
    }
    expect(lines, "两次转移各打一行").toHaveLength(2);
    const [toIdle, toEnded] = lines;
    const gray = ere("GRAY");
    const arch = ere("ARCH");
    // 两向：各自只认自己那一行。
    expect([gray.test(toIdle), gray.test(toEnded)]).toEqual([true, false]);
    expect([arch.test(toIdle), arch.test(toEnded)]).toEqual([false, true]);
  });

  it("F03.2 收到活动信号回到活（claude 复活）——activity 值不变也回且重绘", () => {
    const tab = tm.ensureTab("gi2", "/x", "p", 0, "pi");
    tm.updateActivity("gi2", "busy", null); // 先有一次 busy
    tm.markTmuxIdle("gi2");
    expect(tab.state).toEqual(RECONNECTABLE);
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    // 同值 busy 再来一次（activity 无变化）——仍须回到活、类须去掉（早退前转移的守护）
    tm.updateActivity("gi2", "busy", null);
    expect(tab.state).toEqual(LIVE);
    expect(btn().classList.contains("reconnectable")).toBe(false);
  });

  it("F03.2 远端复活（主信号）：可重连的 tab 又收后端重宣告/行 → ensureTab 回到活", () => {
    // D 审计修：不能只靠 session-activity（非 queue、null-activity backend 下永远不来 →
    // 活跃流式会话永久卡在可重连）。ensureTab（远端重宣告/行 = claude 复活，queue 内保序）是主信号。
    const tab = tm.ensureTab("gr1", "/x", "p", 0, "pi");
    tm.markTmuxIdle("gr1");
    expect(tab.state).toEqual(RECONNECTABLE);
    // 复活：backend 重放该会话的行（或重宣告）→ 同 sid ensureTab
    tm.ensureTab("gr1", "/x", "p", 1, "pi");
    expect(tab.state).toEqual(LIVE); // 删 ensureTab 里那条「远端见行」转移则此断言红
  });

  it("F03.2 已结束优先：archiveTab 把可重连改成已结束（tmux 真没了）", () => {
    const tab = tm.ensureTab("gi3", "/x", "p", 0, "pi");
    tm.markTmuxIdle("gi3");
    expect(tab.state).toEqual(RECONNECTABLE);
    tm.archiveTab("gi3");
    expect(tab.state).toEqual(ENDED);
  });

  it("F03.2 已结束的 Tab 不被 markTmuxIdle 改回可重连", () => {
    const tab = tm.ensureTab("gi4", "/x", "p", 0, "pi");
    tm.archiveTab("gi4");
    tm.markTmuxIdle("gi4"); // 已结束后迟到的 idle 信号——忽略
    expect(tab.state).toEqual(ENDED);
  });

  it("F03.2 可重连信号早于 Tab：进 pendingTmuxIdle，ensureTab 落实为可重连", () => {
    tm.markTmuxIdle("gi5"); // Tab 尚未建
    expect(home(tm).store.pendingTmuxIdle.has("gi5")).toBe(true);
    const tab = tm.ensureTab("gi5", "/x", "p", 0, "pi");
    expect(tab.state).toEqual(RECONNECTABLE);
    expect(home(tm).store.pendingTmuxIdle.has("gi5")).toBe(false);
  });

  it("F03.2 已结束优先于暂存的可重连：pendingArchive + pendingTmuxIdle 同在时建成已结束", () => {
    tm.markTmuxIdle("gi6");
    tm.archiveTab("gi6"); // 二者都在暂存
    expect(home(tm).store.pendingTmuxIdle.has("gi6")).toBe(false); // archive 清掉暂存
    const tab = tm.ensureTab("gi6", "/x", "p", 0, "pi");
    expect(tab.state).toEqual(ENDED);
  });

  it("远端 Tab 掉线归档后再收到行（ensureTab）→ 见行复活成 live", () => {
    const tab = tm.ensureTab("s6", "/x", "p", 0, "pi");
    tm.archiveTab("s6");
    expect(tab.state).toEqual(ENDED);
    tm.ensureTab("s6", "/x", "p", 1, "pi"); // backend 重连重放
    expect(tab.state).toEqual(LIVE);
  });

  it("closeTab 拒关 live Tab（守卫：仅 archived 可关）", () => {
    tm.ensureTab("s7", "/x", "p", 0, LOCAL_ORIGIN);
    tm.closeTab("s7");
    expect(home(tm).store.tabs.has("s7")).toBe(true);
  });

  it("closeTab 关 archived Tab：移出 map + 摘 DOM", () => {
    const tab = tm.ensureTab("s8", "/x", "p", 0, LOCAL_ORIGIN);
    const streamEl = tab.streamEl;
    expect(streamEl.parentElement).not.toBeNull();
    tm.archiveTab("s8");
    tm.closeTab("s8");
    expect(home(tm).store.tabs.has("s8")).toBe(false);
    expect(streamEl.parentElement).toBeNull();
  });

  it("closeTab 通知后端 forget_session（archived 才关）", () => {
    tm.ensureTab("s9", "/x", "p", 0, LOCAL_ORIGIN);
    tm.archiveTab("s9");
    tm.closeTab("s9");
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("forget_session", {
      sessionId: "s9",
    });
  });

  it("switchTo：切 active + 清 unread + 加 .active 类", () => {
    tm.ensureTab("s10", "/x", "p", 0, LOCAL_ORIGIN); // 首个 → 自动 active
    const tab2 = tm.ensureTab("s11", "/y", "p", 0, LOCAL_ORIGIN);
    tab2.unread = 5;
    expect(home(tm).store.activeId).toBe("s10");
    tm.switchTo("s11");
    expect(home(tm).store.activeId).toBe("s11");
    expect(tab2.unread).toBe(0);
    expect(tab2.streamEl.classList.contains("active")).toBe(true);
  });

  it("reviveTab 在 Tab 尚不存在时也清 pendingArchive（不误归档后续新 Tab）", () => {
    home(tm).store.pendingArchive.add("s12");
    tm.reviveTab("s12"); // Tab 还没建
    expect(home(tm).store.pendingArchive.has("s12")).toBe(false);
    const tab = tm.ensureTab("s12", "/x", "p", 0, LOCAL_ORIGIN);
    expect(tab.state).toEqual(LIVE); // 未被 pendingArchive 落实归档
  });

  // === Batch13-F40a：尾部优先门控 / 物化（D 审计 C-3 补测） ===
  //
  // 🔴 **它服务哪条业务要求：`INVARIANTS.md` 条 21 第 3 项**〔`P21` 2026-09-22 现打补写〕。
  //    条 21（启动重放滚动稳定性·贴底不抖）第 3 项逐字：「重放期「视口上方」旧内容不建 DOM
  //    —— `TabManager.onLine` 按 seq 门控，`seq < tab.window.floorSeq` 的旧记录只进
  //    `TailWindow` 账本，**根本不建卡不挂 DOM**；后台 virgin tab 在 `onBatchEnd` 空闲物化
  //    尾段 / `switchTo` 时同步物化」——**下面这一组（F40a 门控矩阵 → F40b fill）就是它。**
  //    `真相源/105 §1.4` 把条 21 记成「全树零命中」并推出「所以它没人守」：读数对
  //    （没人在散文里点出条号），推论错（第 3 项这一组等值断言一直在守）。
  //    ⚠ 条 21 的第 1/2 项（守卫式 `snap()` · 不手动补偿 scrollTop）**不在这里** ——
  //    那是 `MessageStream` 的性质，本文件把它整个 mock 掉了（见文件抬头）。
  //    住址：`tests/invariants-frontend-guard.vitest.ts` 的 ④⑤ 两格。

  const mkContent = (sid: string, seq: number, uuid: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      message: { type: "assistant", uuid } as never,
    }) as never;

  async function spyRender() {
    const { renderContentRecord } = await import("../src/render-stream-record");
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
    expect(home(tm).store.tabs.get("act")!.window.floorSeq).toBe(100);

    tm.onLine(mkContent("bg", 50, "b-1")); // 后台 virgin：收纳
    tm.onLine(mkContent("bg", 51, "b-2"));
    const bg = home(tm).store.tabs.get("bg")!;
    expect(spy.mock.calls.length).toBe(actCalls); // 没为后台建卡
    expect(bg.window.floorSeq).toBeNull();
    expect(bg.window.pendingCount).toBe(2);
    expect(bg.unread).toBe(0); // 收纳不计 unread（修 S-2）
  });

  it("F40a 门控：批后 seq<floor 收纳、seq≥floor 渲染", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("gate", 100, "g-1")); // !inBatch → 直渲并钉 floor=100
    const t = home(tm).store.tabs.get("gate")!;
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
    const vg = home(tm).store.tabs.get("vg")!;
    expect(vg.window.pendingCount).toBe(5); // rIC 还没轮到（异步）
    spy.mockClear();
    tm.onLine(mkContent("vg", 99, "v-live")); // 真 live 行先到
    expect(vg.window.pendingCount).toBe(0); // 先物化（takeTail 钉 floor=10）
    expect(vg.window.floorSeq).toBe(10);
    expect(spy.mock.calls.length).toBe(6); // 5 条物化 + 1 条 live
  });

  it("F40a 物化顺序：unwrapAll → 渲染 → reconcile → rebuildNow", async () => {
    const spy = await spyRender();
    const { reconcilePendingToolResults } = await import("../src/cards");
    tm.onLine(mkContent("act3", 1, "y-1")); // active
    tm.onBatchStart();
    tm.onLine(mkContent("mat", 20, "m-1"));
    tm.onLine(mkContent("mat", 21, "m-2"));
    const mat = home(tm).store.tabs.get("mat")!;
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
    spy.mockImplementation(f15.insertingRender); // ★ 不是 `() => {}`：见 `f15.insertingRender` 头注
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
    expect(home(tm).view.materializeQueue).toEqual(["idle2"]);
    expect(home(tm).store.tabs.get("dead")!.window.pendingCount).toBe(1); // 未被物化
  });

  // === Batch13-F40b：R-1 缓冲 / 上翻补批 / 哨兵 ===

  it("F40b R-1：批期窗口内中部插入缓冲不渲,onBatchEnd 排序一次挂载;后台照计 unread", async () => {
    const spy = await spyRender();
    tm.onLine(mkContent("r1a", 1, "ra-1")); // active,floor=1
    tm.onLine(mkContent("r1b", 100, "rb-1")); // 后台?否——第二个 tab 不自动切,live 直渲钉 floor=100
    const bg = home(tm).store.tabs.get("r1b")!;
    expect(home(tm).store.activeId).toBe("r1a");
    (bg.timeline as unknown as { _maxSeq: number })._maxSeq = 500; // 已渲染到 seq 500
    tm.onBatchStart();
    spy.mockClear();
    tm.onLine(mkContent("r1b", 300, "rb-mid1")); // ≥floor 且 <maxSeq → 缓冲
    tm.onLine(mkContent("r1b", 200, "rb-mid2"));
    expect(spy.mock.calls.length).toBe(0);
    expect(bg.midBatchBuffer.map((p) => p.seq)).toEqual([300, 200]);
    // ★ F15 订正：原写 2，那个 2 是**量具缺陷的产物**。
    // `:742` 那行（seq 100）直渲进后台 tab r1b，生产里 `timeline.size` 会涨 ⇒
    // `tabs.ts:891` 的 `inserted` 为真 ⇒ unread 该 +1；而旧 stub 的 `size` **恒返回 0**，
    // 那一次自增在测试里根本不存在。三条真新消息（100 / 300 / 200）⇒ **3**。
    // ⚠ 这条值得记：量具把一条性质测没了之后，**依赖它的既有断言会跟着长出一个假数**，
    // 而那个假数看起来完全正常（谁会怀疑一个跑了几百次的绿断言？）。
    expect(bg.unread).toBe(3); // 离线期真新消息照计（3 条：seq 100 / 300 / 200）
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
    const t = home(tm).store.tabs.get("fills")!;
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
    const t = home(tm).store.tabs.get("reB")!;
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
    spy.mockImplementation(f15.insertingRender); // ★ 不是 `() => {}`：见 `f15.insertingRender` 头注
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

  it("F40b 哨兵：账本非空显示剩余条数,补尽之后〔CF2〕按行号问一次更早的、问到顶才消失", async () => {
    await spyRender();
    // 〔CF2〕按行号取回：答「那一段一条可显示的都没有」（from 原样、next = until）
    vi.mocked(invoke).mockImplementation(((cmd: string, args?: { from: number; until: number }) =>
      Promise.resolve(
        cmd === "read_session_lines"
          ? { from: args!.from, next: args!.until, eof: false, payloads: [] }
          : undefined,
      )) as never);
    tm.onLine(mkContent("sentA", 1, "sa-1")); // active
    tm.onBatchStart();
    for (let s = 100; s < 300; s++) tm.onLine(mkContent("sentB", s, `sb-${s}`));
    tm.onBatchEnd();
    tm.switchTo("sentB"); // 物化(4 轮×150 上限 → 200 全弹尽)
    const t = home(tm).store.tabs.get("sentB")!;
    expect(t.window.pendingCount).toBe(0);
    // 〔CF2〕渲染窗口最老那一条是第 100 行（> 0）⇒ 下面可能还有：jsdom 恒不可滚 ⇒ 切入的 R-2 踢链当场问 [0, 100)
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === "read_session_lines")).toEqual([
      ["read_session_lines", { origin: "<local>", jsonlPath: "/p/sentB.jsonl", from: 0, until: 100 }],
    ]);
    expect(t.stream.contentElement.querySelector(".stream-more-above")?.textContent).toContain("正在取");
    await new Promise((r) => setTimeout(r, 0));
    // 问的是从第 0 行起 ⇒ 到顶了 ⇒ 哨兵退场，之后不再问
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();
    vi.mocked(invoke).mockResolvedValue(undefined as never);

    // 再造残账(先切走,防 onBatchEnd 的 active 补物化清账):哨兵文本准确
    tm.switchTo("sentA");
    tm.onBatchStart();
    for (let s = 10; s < 15; s++) tm.onLine(mkContent("sentB", s, `sb-old-${s}`)); // <floor 收纳
    tm.onBatchEnd();
    // 直接刷哨兵验证文本(不切入——jsdom 恒不可滚,切入会走 R-2 踢链清账)
    home(tm).view.updateSentinel(t);
    const sentinel = t.stream.contentElement.querySelector(".stream-more-above");
    expect(sentinel?.textContent).toContain("5 条更早消息");
    // 切入:R-2 踢链(不可滚+账本有余)→ 补批到账尽 → 哨兵消失
    tm.switchTo("sentB");
    expect(t.window.pendingCount).toBe(0);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();
  });

  /**
   * ★ 步 3（`设计/10 §6`）：**「够不够一屏」读的是真实布局，不是 `scrollHeight`。**
   *
   * # 这一格为什么能红
   *
   * 造的正是用户报的那一屏：**`scrollHeight` 说"滚得动"，而屏幕上是半屏空的**。
   * 成因不是假设 —— 每张顶层卡都带 `contain-intrinsic-size: auto <估值>`
   * （`height-estimate.ts`），没渲染过的卡按**估值**计入 `scrollHeight`。
   * 旧判据 `scrollHeight - clientHeight > 1` 在第 1 轮就成立 ⇒ 补批当场停手（只补 150 条）；
   * 新判据问的是「最后一张卡的底边到没到容器下沿」⇒ 该补的 4 轮补满（600 条）。
   *
   * # 量的是条数，不是像素
   *
   * jsdom 没有布局引擎 ⇒ 这里的 rect 与 `scrollHeight` 全是**显式桩**。
   * 所以这一格断的是「判据读了哪一个数」，**不是**「真机上到底满没满屏」——
   * 后者要真渲染，登记在报告里那条诚实边界上。
   */
  it("★ 步 3：scrollHeight 说滚得动、但最后一张卡没够到下沿 ⇒ 照样补批", async () => {
    await spyRender();
    tm.onLine(mkContent("fillHead", 1, "fh-1")); // 首个 tab → active
    tm.onBatchStart();
    for (let s = 100; s < 800; s++) tm.onLine(mkContent("fillBody", s, `fb-${s}`)); // 后台收纳 700
    tm.onBatchEnd();
    const t = home(tm).store.tabs.get("fillBody")!;
    expect(t.window.pendingCount).toBe(700);

    // ① 视口：高 600，下沿在 600。
    t.streamEl.getBoundingClientRect = () =>
      ({ top: 0, bottom: 600, height: 600, left: 0, right: 900, width: 900 }) as DOMRect;
    // ② `scrollHeight` 被估值撑到 2000 ⇒ **旧判据在这里恒说"滚得动"**。
    Object.defineProperty(t.streamEl, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(t.streamEl, "clientHeight", { value: 600, configurable: true });
    // ③ 真实布局：最后一张卡的底边只到 120 —— 屏幕下面 480px 是空的。
    const card = document.createElement("div");
    card.getBoundingClientRect = () =>
      ({ top: 80, bottom: 120, height: 40, left: 0, right: 780, width: 780 }) as DOMRect;
    t.stream.contentElement.appendChild(card);

    tm.switchTo("fillBody"); // virgin ⇒ 同步物化，停手条件就是本格的被测对象
    expect(
      t.window.pendingCount,
      "读 scrollHeight 的话第 1 轮就停手（余 550）；读真实布局才会补满 4 轮（余 100）",
    ).toBe(100);
  });

  /**
   * ★ 步 3 下半：**视口自己变大 ⇒ 重新补批。**
   *
   * `fillAbove` 挂在 scroll 事件上，而不可滚的元素**不产生 scroll 事件**
   * ⇒ 把窗口从半屏拉到全屏时，多出来那块空白之前没有任何入口去补。
   * 这一格量的是消费端那三道门（active / 账本非空 / 还没满屏）与「真会补」。
   * 观察端（RO 到底有没有观察 `scrollEl`）在 `stream-viewport-resize.vitest.ts`
   * —— 本文件把 `MessageStream` 整个 mock 掉了，**盖不住那一形**。
   */
  it("★ 步 3：视口变大 ⇒ 重新补批；非 active / 已满屏都不补", async () => {
    await spyRender();
    tm.onLine(mkContent("rzHead", 1, "rh-1")); // active
    tm.onBatchStart();
    for (let s = 100; s < 500; s++) tm.onLine(mkContent("rzBody", s, `rb-${s}`)); // 后台 400
    tm.onBatchEnd();
    const t = home(tm).store.tabs.get("rzBody")!;
    const fire = (): void => t.stream.onViewportResize?.();
    expect(t.stream.onViewportResize, "宿主必须挂上这个消费端，否则观察了也没人管").toBeTypeOf(
      "function",
    );

    // ① 非 active ⇒ 一条都不补（后台 tab 的 0×0 → 真实尺寸那一跳不是"用户拉窗口"）。
    const before = t.window.pendingCount;
    expect(before, "前置：要有账才谈得上补").toBe(400);
    fire();
    expect(t.window.pendingCount, "非 active 不许补").toBe(before);

    // ⚠ 直接置活跃，**不走 `switchTo`** —— 那条路自带 virgin 物化 + R-2 踢链，
    //   会把账先清空，剩下的两格就都成了空真。
    home(tm).store.activeId = "rzBody";
    const pending = t.window.pendingCount;

    // ② 已经满屏（最后一张卡的底边到了下沿）⇒ 不补。
    t.streamEl.getBoundingClientRect = () =>
      ({ top: 0, bottom: 600, height: 600, left: 0, right: 900, width: 900 }) as DOMRect;
    const card = document.createElement("div");
    let cardBottom = 600;
    card.getBoundingClientRect = () =>
      ({ top: 0, bottom: cardBottom, height: cardBottom, left: 0, right: 780, width: 780 }) as DOMRect;
    t.stream.contentElement.appendChild(card);
    fire();
    expect(t.window.pendingCount, "已满屏还补 = 每次 RO 都白干一轮").toBe(pending);

    // ③ 窗口拉高（卡还是那么高，下面空出来一块）⇒ 补。
    cardBottom = 120;
    fire();
    expect(t.window.pendingCount, "视口变大之后没有任何东西去补那块空白").toBeLessThan(pending);
  });

  // === v2.22.2:同 sid kind 冲突消解(bg-spare 谎报父 sid) ===

  it("kind 升格:bg 骨架先到,interactive 宣告后到 → 升格为宿主并重锚孤儿 bg", () => {
    // 场景还原(用户截图):bg-spare 的宣告先到,父会话被建成 ⚙ 挂到同 cwd 的
    // 别的交互会话(Excel)之下;interactive 宣告后到必须升格纠正。
    tm.createSkeletonTab("excel", "/proj/shengwu", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("parent", "/proj/shengwu", LOCAL_ORIGIN, "bg", "迁移服务"); // 谎报形态先到
    tm.createSkeletonTab("fork-empty", "/proj/shengwu", LOCAL_ORIGIN, "bg", "迁移服务"); // 空克隆
    expect(home(tm).store.orderedIds).toEqual(["excel", "parent", "fork-empty"]);
    expect(home(tm).store.tabs.get("parent")!.title).toContain("⚙");

    tm.createSkeletonTab("parent", "/proj/shengwu", LOCAL_ORIGIN, "interactive", null); // 真身宣告后到
    const p = home(tm).store.tabs.get("parent")!;
    expect(p.kind).toBe("interactive");
    expect(p.title).not.toContain("⚙");
    // parent 升格为宿主:提出子树位、追加为交互 tab,孤儿 bg(fork-empty 原挂
    // excel 子串)不被搬走——「多宿主取第一个」契约保持(excel 仍是先到宿主)
    expect(home(tm).store.orderedIds).toEqual(["excel", "fork-empty", "parent"]);
  });

  it("kind 不降格:interactive tab 后到 bg 宣告(spare 谎报)保持交互形态", () => {
    tm.createSkeletonTab("host2", "/proj/x", LOCAL_ORIGIN, "interactive", null);
    tm.createSkeletonTab("host2", "/proj/x", LOCAL_ORIGIN, "bg", "spare 噪声");
    const t = home(tm).store.tabs.get("host2")!;
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
    const t = home(tm).store.tabs.get("meta-bg")!;
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
    await home(tm).actions.resumeTab("r1");
    // A4：默认 resume（无账号）→ 第 5 参 configDir=undefined（不注入，行为与旧版等价）。
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", { configDir: undefined, accountName: undefined, modelOverride: undefined });
    expect(invoke).not.toHaveBeenCalledWith("resume_history_session", expect.anything());
  });

  it("A4/F07：resumeTab 带账号名但账号库不可用 → 退化默认 + **onUnselectable toast（不静默吞）**", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    // tabs.vitest 的 invoke 默认返 undefined → fetchAccounts 视作不可用 → withAccount 退化默认。
    // （accounts.vitest 的 withAccount 套件覆盖了"resolveAccount 自己的决策逻辑"，
    // 但不覆盖"tabs.ts 的 run 回调是否真把 accountName 转传给了 runRemoteResume"这条
    // 集成层接线——F05 Phase D 审计发现的真实覆盖缺口，下面新增一条测试补上。）
    await home(tm).actions.resumeTab("r1", "z");
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", { configDir: undefined, accountName: undefined, modelOverride: undefined });
    expect(invoke).not.toHaveBeenCalledWith("update_history_metadata", expect.anything());
    // F07：显式选号解析不到 → 提示，别静默落基座（对齐 history.ts）。变异锚点：删 onUnselectable 回调 → 此测红。
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "账号不可用",
      expect.stringContaining("账号「z」当前不可选"),
      expect.anything(),
    );
  });

  // F05 Phase D 审计：补齐"账号真的可选时，accountName 是否真的转传到了 runRemoteResume"这条
  // 集成层断言——此前所有 tabs.vitest 用例都只覆盖"账号库不可用/未选账号"这两种 accountName
  // 恒为 undefined 的场景，`resumeTab` 里 `(cd, an) => runRemoteResume(..., cd, an)` 这一行接线
  // 本身从未被验证过（哪怕写反成 `(cd) => runRemoteResume(..., cd)` 也会全绿）。
  it("A4：resumeTab 带账号名且账号可选 → runRemoteResume 收到真实 configDir + accountName", async () => {
    // fetchAccounts 有模块级缓存(30s TTL)——早前用例可能已用默认 mock 给 "aya" 缓存过一次，
    // 不清掉可能命中陈旧缓存、走不到下面的自定义 mock。
    invalidateAccountsCache();
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({
            available: true,
            error: null,
            meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
            accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
          })
        : Promise.resolve(undefined),
    );
    await home(tm).actions.resumeTab("r1", "z");
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", { configDir: "/h/.claude-accts/z", accountName: "z", modelOverride: undefined });
    expect(showActionFailureToast).not.toHaveBeenCalledWith("账号不可用", expect.anything(), expect.anything());
    // fetchAccounts 有 30s TTL 模块级缓存——本测试是文件里第一个真填充"可选账号"数据的用例，
    // 不清掉会让缓存值泄漏进后续测试（它们期望账号库不可用/未选账号）。
    invalidateAccountsCache();
  });

  it("本地归档 tab → 仍走 resume_history_session，不碰远端 runner", async () => {
    tm.ensureTab("l1", "/home/u/p", "/p/l1.jsonl", 0, LOCAL_ORIGIN);
    tm.archiveTab("l1");
    await home(tm).actions.resumeTab("l1");
    expect(runRemoteResume).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("resume_history_session", {
      sessionId: "l1",
      cwd: "/home/u/p",
      launcher: null,
      // P3t-Y2b：读不到本机 tmux 名单（这里 `invoke` 的缺省 mock 回 undefined）⇒ **不铸名**。
      // `null` 在这里是「不知道」，不是「没有名字被占」—— 硬铸就是不避让（issue #76）。
      tmuxName: null,
    });
  });

  // ★★ P3t-Y2b：本机 resume 真的把铸好的 tmux 名传下去。
  //
  // 上面那条只证「不知道的时候不铸」。**光有它，整个 Y2b 被回退掉也不会红**
  //（回退之后恒 `tmuxName: null`，那条照样绿）⇒ 必须再钉正面：知道的时候要铸、且要避让。
  it("P3t-Y2b 本地 resume：拿到本机 tmux 名单 → 铸一个不撞的名字传给后端", async () => {
    (invoke as unknown as Mock).mockImplementation(async (cmd: string) => {
      // `K-R96`：基名从 cwd 派生 ⇒ `/home/u/p` ⇒ `p-cc`。它已被占 ⇒ `mintTmuxName` 必须让到 `-2`。
      if (cmd === "list_local_tmux")
        return [
          { name: "p-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: null },
          { name: "unrelated", path: "/p", command: "bash", attached: false, windows: 1, sid: null },
        ];
      return undefined;
    });
    tm.ensureTab("l1abcdef", "/home/u/p", "/p/l1abcdef.jsonl", 0, LOCAL_ORIGIN);
    tm.archiveTab("l1abcdef");
    await home(tm).actions.resumeTab("l1abcdef");
    expect(invoke).toHaveBeenCalledWith("resume_history_session", {
      sessionId: "l1abcdef",
      cwd: "/home/u/p",
      launcher: null,
      // 让到 `-2` 而不是撞上 `p-cc` —— 撞上去就是「静默接进第一个会话，
      // 而用户以为开了新的」（issue #76 那一族，F13 记着同一个坑）。
      // `K-R96`：名字里**没有 sid**（`l1abcdef` 一个字都不出现）—— 它骑在 `@ccm_sid` 上。
      tmuxName: "p-cc-2",
    });
  });
});

// audit-fixes F01（修 B1，full-audit 阻塞）：跟随 resume 的 pin 必须**现读磁盘**
// (`list_last_accounts`)，不读内存镜像 `accountLastByS`——后者在启动窗口 / 查询失败 /
// 刚显式钉后 10s 内是陈旧或空的，读它会让 withAccount 的不-clobber 守卫拿到假 priorPin=null，
// 把磁盘真实 pin 静默覆盖成全局当前账号。变异锚点：把 readSessionPin 换回
// this.accountLastByS.get(sid)，这两条即红（list_last_accounts 不再被 invoke）。
describe("audit-fixes F01 follow-resume pin 现读磁盘（修 B1 内存脏读覆盖）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
    // 内存镜像里种一个**陈旧**值,证明 resume 不依赖它;磁盘(list_last_accounts)才是真相源。
    tm.setSessionAccounts([], new Map(), new Map([["r1", "STALE"]]));
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_last_accounts" ? Promise.resolve({ r1: "z" }) : Promise.resolve(undefined),
    );
  });

  it("resumeTab（直连，跟随）→ 现读 list_last_accounts，不读内存镜像", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTab("r1");
    expect(invoke).toHaveBeenCalledWith("list_last_accounts");
  });

  it("resumeTabTmux（tmux，跟随）→ 现读 list_last_accounts，不读内存镜像", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    expect(invoke).toHaveBeenCalledWith("list_last_accounts");
  });

  it("显式选号（带账号名）→ 不进跟随分支、不读 pin（list_last_accounts 不被 invoke）", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTab("r1", "z");
    expect(invoke).not.toHaveBeenCalledWith("list_last_accounts");
  });

  // F01 步骤2:显式「用基座 resume」——不跟随、不读 pin、不注入(configDir undefined),
  // 让装账号前住基座的老会话不被 follow 注入全局当前账号(修 #75 逃生口)。
  // 变异锚点:follow 条件去掉 `|| useBase` → 基座又去读 pin → list_last_accounts 被 invoke → 红。
  it("用基座 resume（useBase）→ 不读 pin、不注入（configDir undefined）", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTab("r1", undefined, true);
    expect(invoke).not.toHaveBeenCalledWith("list_last_accounts");
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", { configDir: undefined, accountName: undefined, modelOverride: undefined });
  });

  // F04：tmux 后端的基座逃生口，与直连对称（两后端一致）。useBase → 不跟随、不读 pin、不注入。
  // 变异锚点：resumeTabTmux 的 follow 去掉 `useBase ?` → 又读 pin → list_last_accounts 被 invoke → 红。
  it("用基座 resume（tmux，useBase）→ 不读 pin、不注入（起全新 tmux resume，cd undefined）", async () => {
    // 默认 invoke 返 undefined → list_remote_tmux 无活会话/无 idle → 走 ② 全新 resume。
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    expect(invoke).not.toHaveBeenCalledWith("list_last_accounts");
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "proj-cc", {
      configDir: undefined,
      accountName: undefined,
      modelOverride: undefined,
    });
  });
});

// audit-fixes F03 步骤1（idle-tmux 就地复用，治 #76 根因 + #75 一条）：
// 目标 sid 的 tmux 还在（@ccm_sid 命中）但 command≠claude（空 shell）→ resumeTabTmux 应**复用原会话名**
// 就地 resume（runRemoteResumeIntoExistingTmux），而不是铸名口起 `<项目名>-cc-N` 新会话。
// 变异锚点：删掉 ①.5 idle 分支 → 回落 ② 起新会话 → runRemoteResumeTmux 被调、reuse 没被调 → 红。
describe("audit-fixes F03 resumeTabTmux idle-tmux 就地复用", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  it("sid 的空 tmux（@ccm_sid 命中、command=bash）→ 就地复用原名 resume，不起新会话", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            // claude 已退,只剩交互 shell 的 cc-<sid8>:sid 命中但 command=bash。
            { name: "cc-r1abcd", path: "/home/pi/proj", command: "bash", attached: false, windows: 1, sid: "r1" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    // 就地复用原名(cc-r1abcd),不 attach(不是 live)、不起新会话。
    expect(runRemoteResumeIntoExistingTmux).toHaveBeenCalledWith("aya", "r1", "cc-r1abcd", "cct", {
      configDir: undefined,
      accountName: undefined,
      modelOverride: undefined,
    });
    expect(runRemoteResumeTmux).not.toHaveBeenCalled();
    expect(runRemoteAttach).not.toHaveBeenCalled();
  });

  it("sid 的 tmux 里 command=claude（活）→ 走 attach，不走就地复用", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-r1abcd", path: "/home/pi/proj", command: "claude", attached: true, windows: 1, sid: "r1" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    expect(runRemoteAttach).toHaveBeenCalledWith("aya", "cc-r1abcd");
    expect(runRemoteResumeIntoExistingTmux).not.toHaveBeenCalled();
  });

  it("sid 无对应 tmux（全新/漂移占名）→ 起全新 resume，不就地复用", async () => {
    // 列表里只有别的 sid 的会话 → 目标 sid 既非 live 也无 idle → 落 mintSessionTmuxName 新起。
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-other12", path: "/home/pi/proj", command: "bash", attached: false, windows: 1, sid: "other" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    expect(runRemoteResumeTmux).toHaveBeenCalled();
    expect(runRemoteResumeIntoExistingTmux).not.toHaveBeenCalled();
  });

  // F05 Phase D 审计：resumeTabTmux 走的是 follow 解析（不是显式 accountName 参数），接线是
  // `(cd, an) => runRemoteResumeTmux(..., cd, an)`——补一条"跟随解析真命中账号"的集成测试，
  // 证明 an 真的被转传，不只是 accounts.vitest 单独测过 resolveAccount 自己的决策逻辑。
  it("跟随解析命中当前账号 → runRemoteResumeTmux 收到真实 configDir + accountName", async () => {
    invalidateAccountsCache(); // 同上：防陈旧缓存命中挡住下面的自定义 mock
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "list_remote_tmux") {
        return Promise.resolve([
          { name: "cc-other12", path: "/home/pi/proj", command: "bash", attached: false, windows: 1, sid: "other" },
        ]);
      }
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
          accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
        });
      }
      if (cmd === "list_last_accounts") return Promise.resolve({}); // 无既有 pin → 落 current
      return Promise.resolve(undefined);
    });
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "proj-cc", {
      configDir: "/h/.claude-accts/z",
      accountName: "z",
      modelOverride: undefined,
    });
    invalidateAccountsCache(); // 同上：清掉本测试填充的账号缓存，别泄漏进后续测试
  });

  // F07 Phase D 审计：此前所有涉及 executor 的断言里 modelOverride 尾参恒为 undefined
  // （测试用的 invoke mock 从未给 "load_config" 配过 modelByAccount 数据）——接线代码本身
  // （withAccount 内部 getModelForAccount 查询 → run(cd, an, mo) → runRemoteResumeTmux(...,mo)）
  // 从未被真实模型字符串验证过。补一条同上但账号 z 配了模型偏好的集成测试。
  it("F07：跟随解析命中当前账号且该账号配了模型偏好 → runRemoteResumeTmux 收到真实 modelOverride", async () => {
    invalidateAccountsCache();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "list_remote_tmux") {
        return Promise.resolve([
          { name: "cc-other12", path: "/home/pi/proj", command: "bash", attached: false, windows: 1, sid: "other" },
        ]);
      }
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
          accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
        });
      }
      if (cmd === "list_last_accounts") return Promise.resolve({}); // 无既有 pin → 落 current
      if (cmd === "load_config") return Promise.resolve({ accounts: { modelByAccount: { z: "opus" } } });
      return Promise.resolve(undefined);
    });
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "proj-cc", {
      configDir: "/h/.claude-accts/z",
      accountName: "z",
      modelOverride: "opus",
    });
    invalidateAccountsCache();
  });

  // F04（R10）：命中 ≥2 个精确同 sid 的活会话——attach 非破坏性、可撤销，故"警告+继续"而非拒绝
  // （与破坏性的 restartTabWithAccount 分级不同，见 F04 计划 §2 取舍④）。
  it("sid 同时活在 2 个 tmux（命中 ≥2 个）→ 仍 attach 到第一个 + 警告 toast，不静默假装只有一个", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-r1abcd", path: "/home/pi/proj", command: "claude", attached: true, windows: 1, sid: "r1" },
            { name: "cc-r1efgh", path: "/other", command: "claude", attached: false, windows: 1, sid: "r1" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", 0, "aya");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTabTmux("r1");
    expect(runRemoteAttach).toHaveBeenCalledWith("aya", "cc-r1abcd"); // 仍接入第一个，不拒绝
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "检测到多个同身份会话",
      expect.stringContaining("2"),
      expect.objectContaining({ level: "info" }),
    );
  });
});

describe("F51 tab 右键 attach 反查（异步就绪 + 跨 tab 竞态守卫 R-1）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    tm = makeTM();
  });

  const btnOf = (sid: string): HTMLElement =>
    home(tm).bar.tabButtons.get(sid)!.root;
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
  const killBtn = (): HTMLButtonElement | null => {
    const menu = document.body.querySelector(".tab-context-menu");
    const items = [...(menu?.querySelectorAll(".tab-context-menu-item") ?? [])];
    return (
      (items as HTMLButtonElement[]).find((b) => b.textContent?.includes("杀死会话")) ?? null
    );
  };
  const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

  // ★★ P3 刀 2 的 UI 半：**本机 tab 也有「杀死会话」**。
  //
  // 三格各钉一条，因为它们**失败方式完全不同**：
  // ① 通道不在（`null`）⇒ 不许留一个能点的破坏性菜单项 —— `null` 是「不知道」，
  //    留着它等于让用户点一个必失败的 kill。
  // ② 命中恰好一个 ⇒ 按 `@ccm_sid` 认，**不按名字前缀猜**（下面那条埋了名字诱饵）。
  // ③ 命中 ≥2 个 ⇒ 拒绝，不折叠成第一个（F04 R10 同款分级：破坏性动作代价不可逆）。
  it("P3 刀2-UI 本机 tab 右键：backend 通道不在（null）→ kill 项消失，不留必失败的破坏性动作", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_local_tmux" ? Promise.resolve(null) : Promise.resolve(undefined),
    );
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", 0, LOCAL_ORIGIN);
    rightClick("k1abcdef");
    expect(killBtn()?.textContent).toContain("检测 tmux");
    await flush();
    expect(killBtn()).toBeNull();
  });

  it("P3 刀2-UI 本机 tab 右键：按 @ccm_sid 认出唯一那个（名字前缀是诱饵）", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            // ★ 名字长得就是本 tab 的 `<sid8>-cc`，但 `@ccm_sid` 是别人的 —— **诱饵**。
            //   按名字前缀猜就会中它，那是「拿命名巧合当身份」（§30 禁的那一类）。
            { name: "k1abcdef-cc", path: "/home/u/p", command: "claude", attached: false, windows: 1, sid: "someone-else" },
            { name: "unrelated-cc", path: "/home/u/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", 0, LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(killBtn()?.textContent).toContain("unrelated-cc");
    expect(killBtn()?.textContent).not.toContain("k1abcdef-cc");
  });

  // ★★ P3 刀 3：本机**空 tmux**（claude 已退、只剩交互 shell）→ 就地 resume。
  //
  // 两条：正面（空壳 ⇒ 两格都在）与反面（有活 claude ⇒ 就地那格必须**不在**）。
  // 光有正面不够：把「空壳判定」删掉、无条件给这一格，正面照样绿 ——
  // 而那样会往一个**正在跑 claude** 的会话里再送一遍载荷（F14 逐字记着这个后果）。
  const resumeIntoBtn = (): HTMLButtonElement | null => {
    const menu = document.body.querySelector(".tab-context-menu");
    const items = [...(menu?.querySelectorAll(".tab-context-menu-item") ?? [])];
    return (
      (items as HTMLButtonElement[]).find((b) => b.textContent?.includes("就地 resume")) ?? null
    );
  };

  it("P3 刀3 本机空 tmux → 给「就地 resume」，且 kill 文案改成「kill 空 tmux」", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            // command 不是 claude ⇒ 空壳（claude 已退、只剩交互 shell）。
            { name: "i1-cc", path: "/p", command: "bash", attached: false, windows: 1, sid: "k1abcdef" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", 0, LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(resumeIntoBtn()?.textContent).toContain("i1-cc");
    expect(killBtn()?.textContent).toContain("空 tmux");
  });

  it("P3 刀3 反面：会话里还跑着 claude → **不给**就地 resume（别往活会话再送一遍载荷）", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            { name: "i1-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", 0, LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(resumeIntoBtn()).toBeNull();
    expect(killBtn()?.textContent).toContain("kill tmux i1-cc");
  });

  it("P3 刀2-UI 本机 tab 右键：同身份命中 2 个 → 拒绝，不折叠成第一个", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            { name: "a-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef" },
            { name: "b-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", 0, LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(killBtn()?.textContent).toContain("拒绝");
    expect(killBtn()?.disabled).toBe(true);
  });

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

  // audit-fixes F03.3（attach-into-idle）：无活 claude 但目标 sid 的空 tmux（@ccm_sid 命中、
  // command=bash）还在 → attach 项就绪为「Attach（空 tmux …）」，让用户 attach 进空 shell。
  // 变异锚点：删 resolveAttachMenuItem else 分支的 idle 处理 → 占位被移除、attachBtn=null → 红。
  it("无活 claude 但有目标 sid 的空 tmux（idle）→ attach 项就绪为「空 tmux」", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-A1", path: "/a", command: "bash", attached: false, windows: 1, sid: "A" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("A", "/a", "p", 0, "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()?.textContent).toContain("空 tmux cc-A1");
    expect(attachBtn()?.disabled).toBe(false);
  });

  // F04（R10）：命中 ≥2 个精确同 sid 的活会话——attach 仍就绪（非破坏性，警告即可），
  // kill 项禁用 + 诊断文案（破坏性，选错代价不可逆，须到终端手动处理）。preview 不受影响。
  it("目标 sid 同时活在 2 个 tmux（命中 ≥2 个）→ attach 仍就绪，kill 项禁用+诊断文案", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-A1", path: "/a", command: "claude", attached: false, windows: 1, sid: "A" },
            { name: "cc-A2", path: "/other", command: "claude", attached: false, windows: 1, sid: "A" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("A", "/a", "p", 0, "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()?.disabled).toBe(false);
    expect(attachBtn()?.textContent).toContain("cc-A1");
    expect(killBtn()?.disabled).toBe(true);
    expect(killBtn()?.textContent).toContain("2 个同身份会话");
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

describe("F09/F52 归档远端 tab 右键：Resume 一级项 + 二级 flyout（tmux/直连）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    tm = makeTM();
  });

  const rightClick = (sid: string): void => {
    home(tm).bar.tabButtons
      .get(sid)!
      .root.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }),
      );
  };
  // F09：querySelectorAll 会连带找到嵌套 flyout 里的叶子项（jsdom 不管 CSS display:none，
  // 结构上它们本来就在 DOM 里）——这正是测试想要的：不用先模拟 hover/click 展开就能直接
  // 断言/点击叶子，同今天真实用户"点开 Resume 再点 tmux"最终触达的是同一个按钮。
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

  it("归档远端 tab → 收敛成 1 个「Resume」一级项 + flyout（tmux/直连），旧扁平字符串消失", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    const labels = menuLabels();
    expect(labels).toContain("Resume");
    expect(labels).toContain("tmux");
    expect(labels).toContain("直连（不建 tmux）");
    expect(labels).not.toContain("Resume（直连）");
    expect(labels).not.toContain("Resume（tmux）");
    // tmux 叶子 → 先查 list_remote_tmux(默认 mock 返 undefined = 无活会话)→ 起全新 resume,
    // 带第 5 个不撞名 name="cc-r1"(F74:灰会话 resume 不复用可能漂移的名)。
    clickItem("tmux");
    await flushMicro();
    await flushMicro();
    // account-ux U3：归档 tmux resume 也走 withAccount follow → 第 6 参 configDir（空 mock → undefined）。
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "proj-cc", {
      configDir: undefined,
      accountName: undefined,
      modelOverride: undefined,
    });
    // 直连叶子 → runRemoteResume
    rightClick("r1");
    clickItem("直连（不建 tmux）");
    await flushMicro();
    // A4：默认 resume（无账号）→ 第 5 参 configDir=undefined（不注入，行为与旧版等价）。
    expect(runRemoteResume).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", { configDir: undefined, accountName: undefined, modelOverride: undefined });
  });

  it("F74 tmux 叶子:@ccm_sid 命中活会话 → 精确 attach 它(不撞同目录漂移分支),不重开", async () => {
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
    clickItem("tmux");
    await flushMicro();
    await flushMicro();
    // 精确 attach 到 sid 命中的 proj_cc——不是列在前面的漂移分支 proj_cc-2;且不走 resume。
    expect(runRemoteAttach).toHaveBeenCalledWith("aya", "proj_cc");
    expect(runRemoteResumeTmux).not.toHaveBeenCalled();
  });

  it("F74 tmux 叶子:@ccm_sid 已知但无一命中(原名被漂移会话占着)→ 起全新 resume 挑不撞名", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "proj-cc", path: "/home/pi/proj", command: "claude", attached: true, windows: 1, sid: "drift77" },
          ])
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    clickItem("tmux");
    await flushMicro();
    await flushMicro();
    expect(runRemoteAttach).not.toHaveBeenCalled();
    // proj-cc 被漂移会话占着 → 挑 proj-cc-2 新建,保证 --resume r1 落进原会话。
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "proj-cc-2", { configDir: undefined, accountName: undefined, modelOverride: undefined });
  });

  it("F74 tmux 叶子:老 wrapper(整表无 @ccm_sid)→ 起全新 fresh resume,不 attach 不确定会话", async () => {
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
    clickItem("tmux");
    await flushMicro();
    await flushMicro();
    // findClaudeTmux 按 cwd 兜底命中 proj_cc,但 live.sid(null)!==sid → **不 attach 不确定的会话**,
    // 起 fresh resume(cc-r1 未被占 → 基名);--resume r1 恒落对会话(§30「找不到就别静默换」)。
    expect(runRemoteAttach).not.toHaveBeenCalled();
    expect(runRemoteResumeTmux).toHaveBeenCalledWith("aya", "r1", "/home/pi/proj", "cct", "proj-cc", { configDir: undefined, accountName: undefined, modelOverride: undefined });
  });

  it("归档本地 tab → 仍单「Resume」(无 flyout，无 tmux/直连叶子)", () => {
    tm.ensureTab("l1", "/home/u/p", "p", 0, LOCAL_ORIGIN);
    tm.archiveTab("l1");
    rightClick("l1");
    const labels = menuLabels();
    expect(labels).toContain("Resume");
    expect(labels).not.toContain("tmux");
    expect(labels).not.toContain("直连（不建 tmux）");
  });

  it("F09：账号数据就绪后（恰好 1 个可选账号）→ Resume flyout 追加「不指定账号（用远端 ~/.claude 那套凭据，不跟随当前账号）」，不追加具名账号", async () => {
    invalidateAccountsCache(); // 防陈旧缓存命中挡住下面的自定义 mock
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({
            available: true,
            error: null,
            meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
            accounts: [{ name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
          })
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    await flushMicro();
    await flushMicro();
    const labels = menuLabels();
    expect(labels).toContain("不指定账号（用远端 ~/.claude 那套凭据，不跟随当前账号）");
    // 只有 1 个可选账号 → 不追加具名账号项（同旧版阈值，见 launch-menu.ts）。
    expect(labels).not.toContain("z");
    clickItem("不指定账号（用远端 ~/.claude 那套凭据，不跟随当前账号）");
    // 基座项本身也带 submenu（tmux/直连），点它只展开/切换，不直接执行——不该调用任何 resume。
    expect(runRemoteResumeTmux).not.toHaveBeenCalled();
    expect(runRemoteResume).not.toHaveBeenCalled();
    invalidateAccountsCache(); // 别泄漏进后续测试
  });

  it("F09：≥2 可选账号 → Resume flyout 含每个具名账号，账号×容器真正正交（此前实现缺口已补）", async () => {
    invalidateAccountsCache();
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({
            available: true,
            error: null,
            meta: { enabled: true, acctsDir: "/h/.claude-accts", manifestPath: "/h/.claude-accts/accounts.json", updatedAt: null, sharedStore: null, count: 2, error: null },
            accounts: [
              { name: "z", email: "z@x.edu", configDir: "/h/.claude-accts/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
              { name: "b", email: "b@x.edu", configDir: "/h/.claude-accts/b", isDefault: false, mode: "isolated", exists: true, loggedIn: true },
            ],
          })
        : Promise.resolve(undefined),
    );
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    await flushMicro();
    await flushMicro();
    const labels = menuLabels();
    expect(labels).toContain("z");
    expect(labels).toContain("b");
    // 具名账号项各自也带 tmux/直连子选择——用 querySelectorAll 能拿到的叶子总数量佐证（顶层
    // tmux/直连 2 个 + 基座下 2 个 + z 下 2 个 + b 下 2 个 = 8 个 container 叶子）。
    const containerLeafCount = labels.filter((l) => l === "tmux" || l === "直连（不建 tmux）").length;
    expect(containerLeafCount).toBe(8);
    invalidateAccountsCache();
  });

  // ---- R05 Phase D 审计（重要）：本功能唯一实质重写的那一行 `tabs.ts` 的三元表达式
  // （`opt.kind === "base" ? containerLeaves(undefined, true) : containerLeaves(opt.name, false)`）
  // **此前零测试覆盖**。审计做了三个变异全部存活：把基座分支的 useBase 改 false（= #75 逃生口
  // 退化成 follow 注入）、把具名账号分支的 opt.name 改 undefined（= 点「用 b resume」实际跟随
  // 默认号）、把具名账号分支的 useBase 改 true —— `npm test` 全绿。
  // 既有断言只查 label 集合与叶子**数量**，从不点开某个账号的叶子看真实调用参数；tsc 也帮不上忙
  // （两个分支都类型正确）。下面这组补的就是"点了哪个账号，就真的用哪个账号起"。
  /** 在某个父项的 flyout 里点某个叶子 label。 */
  const clickLeafUnder = (parent: string, leaf: string): void => {
    const wrap = [...document.body.querySelectorAll(".tab-context-menu-item-wrap")].find(
      (w) => (w.children[0] as HTMLElement)?.textContent === parent,
    );
    expect(wrap, `找不到父项 ${parent}`).toBeTruthy();
    const btn = [
      ...wrap!.querySelectorAll(":scope > .tab-context-submenu > .tab-context-menu-item"),
    ].find((b) => b.textContent === leaf) as HTMLButtonElement | undefined;
    expect(btn, `父项 ${parent} 下找不到叶子 ${leaf}`).toBeTruthy();
    btn!.click();
  };

  const twoAccounts = (extraName?: string) =>
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({
            available: true,
            error: null,
            meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 2, error: null },
            accounts: [
              { name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
              { name: extraName ?? "b", email: "b@x", configDir: `/h/${extraName ?? "b"}`, isDefault: false, mode: "isolated", exists: true, loggedIn: true },
            ],
          })
        : Promise.resolve(undefined),
    );

  const openArchivedMenu = async (): Promise<void> => {
    tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
    tm.archiveTab("r1");
    rightClick("r1");
    await flushMicro();
    await flushMicro();
  };

  it("R05：点「不指定账号（用远端 ~/.claude 那套凭据，不跟随当前账号）」下的直连 → useBase 生效（configDir 为空），#75 逃生口不退化", async () => {
    invalidateAccountsCache();
    twoAccounts();
    await openArchivedMenu();
    clickLeafUnder("不指定账号（用远端 ~/.claude 那套凭据，不跟随当前账号）", "直连（不建 tmux）");
    await flushMicro();
    expect(runRemoteResume).toHaveBeenCalledWith(
      "aya", "r1", "/home/pi/proj", "cct",
      { configDir: undefined, accountName: undefined, modelOverride: undefined },
    );
    invalidateAccountsCache();
  });

  it("R05：点具名账号「b」下的直连 → 真的用 b 起（不是跟随默认号）", async () => {
    invalidateAccountsCache();
    twoAccounts();
    await openArchivedMenu();
    clickLeafUnder("b", "直连（不建 tmux）");
    await flushMicro();
    expect(runRemoteResume).toHaveBeenCalledWith(
      "aya", "r1", "/home/pi/proj", "cct",
      expect.objectContaining({ configDir: "/h/b", accountName: "b" }),
    );
    invalidateAccountsCache();
  });

  it("R05：点具名账号「z」下的 tmux → 走 tmux 路径且带 z", async () => {
    invalidateAccountsCache();
    twoAccounts();
    await openArchivedMenu();
    clickLeafUnder("z", "tmux");
    await flushMicro();
    expect(runRemoteResumeTmux).toHaveBeenCalledWith(
      "aya", "r1", "/home/pi/proj", "cct", expect.any(String),
      expect.objectContaining({ configDir: "/h/z", accountName: "z" }),
    );
    invalidateAccountsCache();
  });

  // R05 Phase D 审计（第 4 题，**实测修了一个真 bug**）：账号名允许下划线
  // （`settings/acct-deploy.ts::validateAcctName` 放行 `[A-Za-z0-9._-]`、只禁首字符 `-`/`.`），
  // 故一个**真实账号**完全可以叫 `__base__`；何况账号也能由 cc-acct-iso 在 app 之外直接建、
  // 根本不过这道校验。改造前 `isBase = opt.id === "__base__"` 会把它判成基座：
  // 点它 → 静默落基座、用户选的号被吞掉（R11/R08 那族「看起来生效了，只是用了错的号」），
  // 且 `filter(o => o.id !== "__base__")` 连带把它从 realAccounts 里滤掉 → Restart 入口凭空消失。
  // 审计双向实测过：改动前这两条红、改动后绿。判别联合把"是基座"从**值域内的保留名**
  // 变成**类型上的另一支**，从根上消掉了碰撞。
  it("R05：真实账号恰好叫 __base__ → 当成账号而非基座（保留名碰撞已从类型上消除）", async () => {
    invalidateAccountsCache();
    twoAccounts("__base__");
    await openArchivedMenu();
    clickLeafUnder("__base__", "直连（不建 tmux）");
    await flushMicro();
    expect(runRemoteResume).toHaveBeenCalledWith(
      "aya", "r1", "/home/pi/proj", "cct",
      expect.objectContaining({ configDir: "/h/__base__", accountName: "__base__" }),
    );
    invalidateAccountsCache();
  });

  it("R05：账号名为 __base__ 时不吞掉 Restart 入口（改造前 realAccounts 会误过滤它）", async () => {
    invalidateAccountsCache();
    twoAccounts("__base__");
    tm.ensureTab("r2", "/home/pi/proj", "p", 0, "aya");
    rightClick("r2");
    await flushMicro();
    await flushMicro();
    expect(menuLabels()).toContain("Restart（换号重启）");
    invalidateAccountsCache();
  });

  it("R05：0 可选账号 → 不渲染分隔线（`length > 0` 那道闸；审计变异 M7 曾存活）", async () => {
    invalidateAccountsCache();
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({ available: false, error: null, meta: null, accounts: [] })
        : Promise.resolve(undefined),
    );
    await openArchivedMenu();
    expect(document.body.querySelectorAll(".tab-context-menu-divider").length).toBe(0);
    invalidateAccountsCache();
  });

  // F09 Phase D 审计（UX，阻塞）：用户手快，右键后账号数据（异步 fetchAccounts）还没回来就已经
  // hover 展开了 Resume 的顶层 flyout（此时只有 tmux/直连两项）；账号数据一到，
  // appendAccountMenuItems 用 updateTabContextMenuItem 整体替换 Resume 这个 DOM 节点，新节点
  // 默认无 is-open——flyout 会在鼠标没动的情况下无预警"啪"地收起。直接命中 R4"悬停+点击都可
  // 触发"这条契约。用假计时器复现"先 hover 展开、后台数据才到达"这个时序。
  it("F09：账号数据到达前已 hover 展开 Resume flyout → 数据到达后 flyout 仍保持展开（不无故收起）", async () => {
    invalidateAccountsCache();
    vi.useFakeTimers();
    try {
      let resolveAccounts!: (v: unknown) => void;
      const pending = new Promise((r) => (resolveAccounts = r));
      vi.mocked(invoke).mockImplementation((cmd: string) =>
        cmd === "list_remote_accounts" ? pending : Promise.resolve(undefined),
      );
      tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
      tm.archiveTab("r1");
      rightClick("r1");
      const resumeWrap = document.body.querySelector<HTMLElement>(
        ".tab-context-menu > .tab-context-menu-item-wrap",
      );
      expect(resumeWrap).not.toBeNull();
      resumeWrap!.dispatchEvent(new MouseEvent("mouseenter", { bubbles: false }));
      await vi.advanceTimersByTimeAsync(150); // 展开延迟
      expect(resumeWrap!.classList.contains("is-open")).toBe(true);
      // 账号数据这时才到达（fetchAccounts resolve）→ appendAccountMenuItems 换掉 Resume 节点。
      resolveAccounts({
        available: true,
        error: null,
        meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
        accounts: [{ name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
      });
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(0);
      const newWrap = document.body.querySelector<HTMLElement>(
        ".tab-context-menu > .tab-context-menu-item-wrap",
      );
      expect(newWrap).not.toBeNull();
      expect(newWrap!.classList.contains("is-open")).toBe(true); // 没有无故收起
    } finally {
      vi.useRealTimers();
      invalidateAccountsCache();
    }
  });

  // F09 Phase D 审计（UX，重要）：tab-bar 可拖到 340px 宽（main.ts::clampW 硬上限），三级级联
  // 从这个位置起算，窄窗口下最深一级 flyout 会溢出右边界变死菜单。用 stub 过的
  // getBoundingClientRect 模拟"右侧放不下"/"放得下"两种视口几何，锁定 flipSubmenuIfOverflowing
  // 的判断逻辑（不是测真实像素渲染，是测这个函数会不会在该加 flip-left 时加、不该加时不加）。
  it("F09：右侧空间不够时 Resume flyout 加 flip-left，够用时不加", async () => {
    invalidateAccountsCache();
    const origInnerWidth = window.innerWidth;
    const origGBCR = HTMLElement.prototype.getBoundingClientRect;
    try {
      Object.defineProperty(window, "innerWidth", { value: 400, configurable: true });
      tm.ensureTab("r1", "/home/pi/proj", "p", 0, "aya");
      tm.archiveTab("r1");
      rightClick("r1");
      const resumeWrap = document.body.querySelector<HTMLElement>(
        ".tab-context-menu > .tab-context-menu-item-wrap",
      );
      const resumeBtn = resumeWrap!.querySelector<HTMLButtonElement>(":scope > button")!;
      const flyout = resumeWrap!.querySelector<HTMLElement>(".tab-context-submenu")!;

      // 场景①：wrap 贴着右边界（right=380），flyout 估宽 150 → 380+150=530 > innerWidth(400) → 该 flip。
      HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
        if (this === resumeWrap) return { right: 380 } as DOMRect;
        if (this === flyout) return { width: 0 } as DOMRect; // 未展开时宽度未知，函数内部兜底成 150
        return origGBCR.call(this);
      };
      resumeBtn.click();
      expect(flyout.classList.contains("flip-left")).toBe(true);
      resumeBtn.click(); // 收起，复位状态

      // 场景②：wrap 靠左（right=50），同样估宽 150 → 50+150=200 < innerWidth(400) → 不该 flip。
      HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
        if (this === resumeWrap) return { right: 50 } as DOMRect;
        if (this === flyout) return { width: 0 } as DOMRect;
        return origGBCR.call(this);
      };
      resumeBtn.click();
      expect(flyout.classList.contains("flip-left")).toBe(false);
    } finally {
      HTMLElement.prototype.getBoundingClientRect = origGBCR;
      Object.defineProperty(window, "innerWidth", { value: origInnerWidth, configurable: true });
      invalidateAccountsCache();
    }
  });
});

describe("F09 活会话右键：Restart 一级项 + flyout（换号重启，无容器轴）", () => {
  let tm: TabManager;
  const sess = (over: Record<string, unknown> = {}) => ({
    name: "cc-m1",
    path: "/w",
    command: "claude",
    attached: false,
    windows: 1,
    sid: "m1",
    ...over,
  });
  const rightClick = (sid: string): void => {
    home(tm).bar.tabButtons
      .get(sid)!
      .root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
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

  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    invalidateAccountsCache();
    tm = makeTM();
  });

  it("<2 可选账号 → 不出现「Restart」（同旧版阈值，不加噪）", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({
            available: true,
            error: null,
            meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
            accounts: [{ name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true }],
          })
        : Promise.resolve(undefined),
    );
    tm.ensureTab("m1", "/w", "/p/m1.jsonl", 0, "aya");
    rightClick("m1");
    await flushMicro();
    await flushMicro();
    expect(menuLabels()).not.toContain("Restart（换号重启）");
  });

  it("≥2 可选账号 → 「Restart」一级项 + 每账号 flyout（直接重启/先压缩再重启），无 tmux/直连子选择", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 2, error: null },
          accounts: [
            { name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
            { name: "b", email: "b@x", configDir: "/h/b", isDefault: false, mode: "isolated", exists: true, loggedIn: true },
          ],
        });
      }
      if (cmd === "list_remote_tmux") return Promise.resolve([sess()]);
      return Promise.resolve(undefined);
    });
    tm.ensureTab("m1", "/w", "/p/m1.jsonl", 0, "aya");
    rightClick("m1");
    await flushMicro();
    await flushMicro();
    const labels = menuLabels();
    expect(labels).toContain("Restart（换号重启）");
    expect(labels).toContain("z");
    expect(labels).toContain("b");
    expect(labels).not.toContain("tmux");
    expect(labels).not.toContain("直连（不建 tmux）");
    expect(labels).not.toContain("不指定账号（用远端 ~/.claude 那套凭据，不跟随当前账号）"); // restart 从不给基座逃生口（旧版行为）
    expect(labels).toContain("直接重启");
    expect(labels).toContain("先压缩上下文再重启");

    clickItem("直接重启");
    await flushMicro();
    expect(restartWithAccount).toHaveBeenCalledWith(
      expect.objectContaining({ origin: "aya", sessionId: "m1", accountName: "z", compactFirst: false }),
    );
  });

  // F09 Phase D 审计（UX，重要）：⇄ 按钮删除前，重启中的会话至少有"⇄ 立刻置灰"这个视觉信号；
  // 现在菜单是唯一入口，若不禁用，点了会静默命中 in-flight 守卫——菜单应提前呈现"当前不可点"。
  it("该会话正在重启中 → Restart 一级项禁用（不是点了才知道不可用）", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 2, error: null },
          accounts: [
            { name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
            { name: "b", email: "b@x", configDir: "/h/b", isDefault: false, mode: "isolated", exists: true, loggedIn: true },
          ],
        });
      }
      if (cmd === "list_remote_tmux") return Promise.resolve([sess()]);
      return Promise.resolve(undefined);
    });
    tm.ensureTab("m1", "/w", "/p/m1.jsonl", 0, "aya");
    home(tm).actions.restartingSids.add("m1");
    rightClick("m1");
    await flushMicro();
    await flushMicro();
    const restartBtn = [
      ...(document.body.querySelector(".tab-context-menu")?.querySelectorAll(".tab-context-menu-item") ?? []),
    ].find((b) => b.textContent === "Restart（换号重启）") as HTMLButtonElement | undefined;
    expect(restartBtn).not.toBeUndefined();
    expect(restartBtn?.disabled).toBe(true);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// `K-P5g` `KP5GD1` 的**接线那一半**
//
// `accounts.vitest.ts` 那一组钉的是「那个纯函数会不会分岔」；本组钉的是
// **它真的被接在了生产路径上、而且喂进去的正是 `--session-accounts` 那一格**。
// 两条都要：只有纯函数 ⇒ 它可以躺着没人调；只有接线 ⇒ 分岔可以是假的。
//
// ⚠ 这两条只在 `launchId` 这一格上不同（同一个 `ROW` 基座、同一套 mock、同一串点击），
// 所以两条断言的差别只可能由那一格造成。
// ═══════════════════════════════════════════════════════════════════════════
describe("K-P5g：tmux 定位不到时，那句提示真的由读回来的身份 token 决定", () => {
  let tm: TabManager;
  const rightClick = (sid: string): void => {
    home(tm).bar.tabButtons
      .get(sid)!
      .root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
  };
  const clickItem = (label: string): void => {
    const btn = [
      ...(document.body.querySelector(".tab-context-menu")?.querySelectorAll(".tab-context-menu-item") ?? []),
    ].find((b) => b.textContent === label) as HTMLButtonElement | undefined;
    btn?.click();
  };
  const ROW = (launchId: string | null) => ({
    pid: 4242,
    sessionId: "m1",
    cwd: "/w",
    configDir: null,
    account: null,
    bare: true,
    alive: true,
    launchId,
  });

  /** 走完「右键 → Restart → 直接重启」，回那次 toast 的 `[title, body]`。 */
  const restartAndCatchToast = async (launchId: string | null): Promise<[string, string]> => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "list_remote_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 2, error: null },
          accounts: [
            { name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
            { name: "b", email: "b@x", configDir: "/h/b", isDefault: false, mode: "isolated", exists: true, loggedIn: true },
          ],
        });
      }
      // ★ 关键前提：tmux 里**精确命中不到**这条 sid ⇒ 走 `!live` 那条拒绝分支。
      if (cmd === "list_remote_tmux") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    tm.ensureTab("m1", "/w", "/p/m1.jsonl", 0, "aya");
    // 这就是那一格的**唯一入口**：backend 的 `--session-accounts` 出参经 main.ts 喂进来。
    tm.setSessionAccounts([ROW(launchId)], new Map());
    rightClick("m1");
    await flushMicro();
    await flushMicro();
    clickItem("直接重启");
    for (let i = 0; i < 6; i++) await flushMicro();
    expect(restartWithAccount).not.toHaveBeenCalled(); // 拒绝分支：绝不能真去重启
    const calls = vi.mocked(showActionFailureToast).mock.calls;
    expect(calls.length).toBe(1);
    return [String(calls[0][0]), String(calls[0][1])];
  };

  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    invalidateAccountsCache();
    tm = makeTM();
  });

  it("没有身份 token ⇒ 老话（两条成因并排摆着）", async () => {
    const [title, body] = await restartAndCatchToast(null);
    expect(title).toBe("无法换号重启");
    expect(body).toContain("或无法精确定位");
  });

  it("★★ 带着身份 token ⇒ 提示换了一条，且 token 一个字节都没进提示", async () => {
    const TOKEN = "0198f0d2-1111-4222-8333-444455556666";
    const [title, body] = await restartAndCatchToast(TOKEN);
    expect(title).toBe("无法换号重启：tmux 标记丢了");
    expect(body).toContain("带着本工具铸的身份标记");
    expect(body).not.toContain("或无法精确定位");
    // 🔴 死值验的落点：把生产段那一行改回写死的老文案（不读 `sessionAccountsByS`），
    //    本条当场红；而 `K-P5f` 已经买到的「读到了」那一族一条都不会红。
    expect(body).not.toContain(TOKEN);
    expect(title).not.toContain(TOKEN);
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

// F04（R10 根治）：findClaudeTmuxMatches 不折叠成第一个——findClaudeTmux 用它重实现，
// 这组测试锁住"两者在单/零命中场景下逐字节同结果"这条 F04 步骤4 的核心不变量，并新增
// 之前完全没有覆盖过的"命中 ≥2 个"场景（R10 的字面定义）。
describe("F04 findClaudeTmuxMatches（不折叠成第一个，R10 根治的类型基础）", () => {
  const S = (name: string, path: string, command: string, sid: string | null) => ({
    name,
    path,
    command,
    attached: false,
    windows: 1,
    sid,
  });
  it("同一 sid 命中 2 个活 claude 会话 → 返回全部 2 个，不丢任何一个（R10 的字面场景）", () => {
    const list = [S("cc-a", "/p", "claude", "target"), S("cc-b", "/q", "claude", "target")];
    const matches = findClaudeTmuxMatches(list, "target");
    expect(matches.length).toBe(2);
    expect(matches.map((m) => m.name).sort()).toEqual(["cc-a", "cc-b"]);
  });
  it("findClaudeTmux 在命中 2 个时只返回 matches[0]（.find 与 .filter[0] 同一遍历顺序，逐字节同结果）", () => {
    const list = [S("cc-a", "/p", "claude", "target"), S("cc-b", "/q", "claude", "target")];
    expect(findClaudeTmux(list, "target", "/p")?.name).toBe(
      findClaudeTmuxMatches(list, "target")[0].name,
    );
  });
  it("恰好 1 个命中 → 数组长度 1（对齐 findClaudeTmux 的单值语义）", () => {
    const list = [S("cc-a", "/p", "claude", "target")];
    expect(findClaudeTmuxMatches(list, "target").length).toBe(1);
  });
  it("0 个命中 → 空数组（不是 undefined）", () => {
    expect(findClaudeTmuxMatches([S("cc-a", "/p", "claude", "other")], "target")).toEqual([]);
    expect(findClaudeTmuxMatches(null, "target")).toEqual([]);
  });
  it("command≠claude 的不算命中（与 findClaudeTmux 的 command 门槛一致）", () => {
    expect(findClaudeTmuxMatches([S("cc-a", "/p", "bash", "target")], "target")).toEqual([]);
  });
});

// audit-fixes F03（idle-tmux）：findIdleTmux 与 findClaudeTmux 互斥——前者要 @ccm_sid 命中且
// command≠claude（空 shell），后者要 command=claude。F03.1 就地复用 + F03.3 attach-idle 共用。
describe("audit-fixes F03 findIdleTmux（sid 命中但 command≠claude 的空 tmux）", () => {
  const S = (name: string, command: string, sid: string | null) => ({
    name,
    path: "/p",
    command,
    attached: false,
    windows: 1,
    sid,
  });
  it("@ccm_sid 命中 + command≠claude（bash）→ 命中该空 tmux", () => {
    expect(findIdleTmux([S("cc-t1", "bash", "target")], "target")?.name).toBe("cc-t1");
  });
  it("@ccm_sid 命中但 command=claude（活会话）→ 不算 idle（互斥 findClaudeTmux）", () => {
    expect(findIdleTmux([S("cc-t1", "claude", "target")], "target")).toBeUndefined();
  });
  it("command=node（claude 是 Node CLI）也算活、不算 idle", () => {
    expect(findIdleTmux([S("cc-t1", "node", "target")], "target")).toBeUndefined();
  });
  it("只按 @ccm_sid 精确命中，绝不按 cwd 猜（sid 不符 → 不命中）", () => {
    expect(findIdleTmux([S("cc-x", "bash", "other")], "target")).toBeUndefined();
    expect(findIdleTmux([S("cc-x", "bash", null)], "target")).toBeUndefined();
  });
  it("null / 空列表 → undefined", () => {
    expect(findIdleTmux(null, "t")).toBeUndefined();
    expect(findIdleTmux([], "t")).toBeUndefined();
  });
});

// auto-e2e F-E4：可注入 confirm seam（killRemoteTmux）——**行为等价**验证。默认（不传 opts）**必须**
// 仍调 window.confirm、消息串不变（默认交互零变化，这是 seam 非行为改动）；注入 confirm 才旁路
// （headless e2e / DEV）。DOM(jsdom) 层是该 TabManager 方法的诚实天花板。
describe("auto-e2e F-E4 可注入 confirm seam（killRemoteTmux 行为等价）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  const microFlush = async (): Promise<void> => {
    await Promise.resolve();
    await Promise.resolve();
  };
  const killCalls = (): unknown[] =>
    vi.mocked(invoke).mock.calls.filter((c) => c[0] === "kill_remote_tmux");

  it("killRemoteTmux 默认（不传 opts）→ 仍调 window.confirm（默认交互零变化）", () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    home(tm).actions.killRemoteTmux("hostA", "cc-abc", false);
    expect(confirmSpy).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("killRemoteTmux 注入 confirm=()=>true → 不碰 window.confirm、invoke kill_remote_tmux", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.killRemoteTmux("hostA", "cc-abc", false, { confirm: () => true });
    await microFlush();
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(killCalls()).toHaveLength(1);
    expect((killCalls()[0] as unknown[])[1]).toMatchObject({ origin: "hostA", target: "cc-abc" });
    confirmSpy.mockRestore();
  });

  it("killRemoteTmux 注入 confirm=()=>false → no-op，不 invoke", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.killRemoteTmux("hostA", "cc-abc", false, { confirm: () => false });
    await microFlush();
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(killCalls()).toHaveLength(0);
    confirmSpy.mockRestore();
  });

  // UX 审计 #1：灰态(idle-tmux) tab 也能 kill——opts.idle 走"Claude 已退出"文案（非"正在运行"），照常 kill。
  it("killRemoteTmux { idle:true } → 文案说 Claude 已退出、非'正在运行'，仍 kill 空 tmux", async () => {
    const msgs: string[] = [];
    home(tm).actions.killRemoteTmux("hostA", "cc-idle1234", false, {
      idle: true,
      confirm: (m: string) => {
        msgs.push(m);
        return true;
      },
    });
    await microFlush();
    expect(msgs).toHaveLength(1);
    expect(msgs[0]).toContain("Claude 已退出");
    expect(msgs[0]).not.toContain("正在运行的 Claude");
    expect(killCalls()).toHaveLength(1);
    expect((killCalls()[0] as unknown[])[1]).toMatchObject({ origin: "hostA", target: "cc-idle1234" });
  });

  // 护栏：live（非 idle）文案必须仍含"正在运行的 Claude"——防日后误改 live 文案不被测出。
  it("killRemoteTmux 非 idle → 文案含'正在运行的 Claude'（live 路径护栏）", () => {
    const msgs: string[] = [];
    home(tm).actions.killRemoteTmux("hostA", "cc-live1234", false, {
      confirm: (m: string) => {
        msgs.push(m);
        return false;
      },
    });
    expect(msgs).toHaveLength(1);
    expect(msgs[0]).toContain("正在运行的 Claude");
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

  // F04 Phase D 审计：`resolveAttachMenuItem`/缓存菜单构建的代码注释断言"matches.length>1"
  // （F04 的多命中告警）与 `viaCwd`（F74c 的 cwd 回退告警）互斥、不会同时触发——此前只靠人工
  // 读代码证明（`viaCwd` 要求"整张列表无任何 @ccm_sid"，`matches.length>1` 要求至少两条精确
  // sid 命中，两个前提不可能同时满足）。这条测试把该不变量钉死，未来重构悄悄破坏它会立刻转红。
  it("F04：matches.length>1（多命中告警）与 viaCwd（cwd 回退告警）不会同时为真", () => {
    const scenarios: Array<{ label: string; sessions: ReturnType<typeof S>[] }> = [
      { label: "整张列表无 sid（走 cwd 回退）", sessions: [S("a", "/p", "claude", null)] },
      {
        label: "恰好 2 个精确命中同 sid",
        sessions: [S("a", "/p", "claude", "target"), S("b", "/q", "claude", "target")],
      },
      { label: "混合：部分会话有 sid 但都不是目标", sessions: [S("a", "/p", "claude", "other")] },
    ];
    for (const { label, sessions } of scenarios) {
      const ambiguous = findClaudeTmuxMatches(sessions, "target").length > 1;
      const viaCwd = isCwdFallbackMatch(sessions, "target");
      expect(ambiguous && viaCwd, label).toBe(false);
    }
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
  it("二次确认通过 → invoke kill_remote_tmux（origin/target 正确，变灰由 #60-A 兜、不主动 archive）", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const tm = home(makeTM()).actions;
    tm.killRemoteTmux("hostA", "cc-abc", false);
    await Promise.resolve();
    const call = vi.mocked(invoke).mock.calls.find((c) => c[0] === "kill_remote_tmux");
    expect(call).toBeTruthy();
    expect(call![1]).toMatchObject({ origin: "hostA", target: "cc-abc" });
    confirmSpy.mockRestore();
  });
  it("二次确认取消 → 不 invoke", () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    const tm = home(makeTM()).actions;
    tm.killRemoteTmux("hostA", "cc-abc", false);
    expect(
      vi.mocked(invoke).mock.calls.some((c) => c[0] === "kill_remote_tmux"),
    ).toBe(false);
    confirmSpy.mockRestore();
  });
  it("F79 审计修复：cwd 回退命中（viaCwd）→ 二次确认加强 caveat（可能杀同目录别的会话）", () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    const tm = home(makeTM()).actions;
    tm.killRemoteTmux("hostA", "cc-abc", true);
    const msg = String(confirmSpy.mock.calls[0]?.[0] ?? "");
    // 〔U2〕按术语表改词：`@ccm_sid` 是禁词（say：不说标记，说后果「认不出是哪个会话」），
    //   这一格随改词同拍改 —— 钉的仍是同一件事（回退命中 ⇒ 确认框里有串味警告）。
    expect(msg).toContain("认不出这是哪个会话"); // 未检测到身份标记
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
    expect(info!.origin).toBe(LOCAL_ORIGIN);
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
  it("活跃本地 tab → { parentPath(=sourcePath), origin: LOCAL_ORIGIN }", () => {
    const tm = makeTM();
    tm.ensureTab("s1", "/home/u", "/p/s1.jsonl", 0, LOCAL_ORIGIN);
    tm.switchTo("s1");
    expect(tm.getActiveSubagentContext()).toEqual({
      parentPath: "/p/s1.jsonl",
      origin: LOCAL_ORIGIN,
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
    tm.createSkeletonTab("sk", "/root/proj", LOCAL_ORIGIN); // parentPath 空
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
    const tab = tm.ensureTab("s1", "/proj", "/p/s1.jsonl", 0, LOCAL_ORIGIN);
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
    tm.ensureTab("s2", "/x", "/p/s2.jsonl", 0, LOCAL_ORIGIN);
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
    const awaitC = home(tm).actions.awaitCompactFor("cs1", 60_000);
    const p = awaitC(); // 注册 waiter
    tm.onLine(compactLine("cs1") as never);
    await expect(p).resolves.toBe(true);
  });

  it("超时 → resolve(false)", async () => {
    vi.useFakeTimers();
    try {
      const awaitC = home(tm).actions.awaitCompactFor("cs2", 5000);
      const p = awaitC();
      vi.advanceTimersByTime(5000);
      await expect(p).resolves.toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("非 compact 行不 resolve（等待者仍挂着）", async () => {
    const awaitC = home(tm).actions.awaitCompactFor("cs3", 60_000);
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
    const awaitC = home(tm).actions.awaitCompactFor("cs4", 60_000);
    let resolved = false;
    void awaitC().then(() => {
      resolved = true;
    });
    tm.onLine(compactLine("other-sid") as never); // 不同 sid
    await Promise.resolve();
    expect(resolved).toBe(false);
  });
});

// 〔`A3` 第二波〕**本机换号重启**：菜单与编排入口都对本机 tab 开放，origin 取 backend 的 `<local>`。
// 死值验对照：把 `restartTabWithAccount` 开头那条改回 `tab.origin === null ⇒ return false`、
// 或把右键那一行改回 `origin !== null && t`，下面各红一条。
describe("A3 本机换号重启：菜单与入口都认本机 tab", () => {
  const restartSpy = restartWithAccount as unknown as ReturnType<typeof vi.fn>;
  let tm: TabManager;
  const TWO_LOCAL = {
    available: true,
    error: null,
    meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 2, error: null },
    accounts: [
      { name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true },
      { name: "b", email: "b@x", configDir: "/h/b", isDefault: false, mode: "isolated", exists: true, loggedIn: true },
    ],
  };
  const localSess = (over: Record<string, unknown> = {}) => ({
    name: "proj-cc",
    path: "/w",
    command: "claude",
    attached: false,
    windows: 1,
    sid: "l1",
    ...over,
  });
  const rightClick = (sid: string): void => {
    home(tm).bar.tabButtons
      .get(sid)!
      .root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
  };
  const menuItems = (): HTMLButtonElement[] =>
    [
      ...(document.body.querySelector(".tab-context-menu")?.querySelectorAll(".tab-context-menu-item") ?? []),
    ] as HTMLButtonElement[];
  const invokedCmds = (): string[] =>
    (invoke as unknown as ReturnType<typeof vi.fn>).mock.calls.map((c) => String(c[0]));

  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    invalidateAccountsCache();
    tm = makeTM();
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === "list_local_accounts") return Promise.resolve(TWO_LOCAL);
      if (cmd === "list_local_tmux") return Promise.resolve([localSess()]);
      return Promise.resolve(undefined);
    });
  });

  it("本机活会话 ≥2 可选账号 → 出现「Restart」，点了走 `<local>`；账号清单问的是本机后端", async () => {
    tm.ensureTab("l1", "/w", "/p/l1.jsonl", 0, LOCAL_ORIGIN);
    rightClick("l1");
    await flushMicro();
    await flushMicro();
    await flushMicro();
    expect(menuItems().map((b) => b.textContent)).toContain("Restart（换号重启）");
    // 账号清单那一跳问的是**本机**（`list_local_accounts`），不是拿 `<local>` 去问远端。
    expect(invokedCmds()).toContain("list_local_accounts");
    expect(invokedCmds()).not.toContain("list_remote_accounts");
    menuItems().find((b) => b.textContent === "直接重启")?.click();
    await flushMicro();
    expect(restartSpy).toHaveBeenCalledWith(
      expect.objectContaining({ origin: "<local>", sessionId: "l1", accountName: "z", compactFirst: false }),
    );
  });

  it("本机**归档** tab 不拉账号清单（本机 Resume 不带账号选择，只有活会话才有换号重启）", async () => {
    tm.ensureTab("l1", "/w", "/p/l1.jsonl", 0, LOCAL_ORIGIN);
    home(tm).store.tabs.get("l1")!.state = ENDED;
    rightClick("l1");
    await flushMicro();
    await flushMicro();
    expect(invokedCmds()).not.toContain("list_local_accounts");
    expect(menuItems().map((b) => b.textContent)).not.toContain("Restart（换号重启）");
  });

  it("本机会话精确 @ccm_sid 命中 → 编排器拿到 `<local>` ＋ 本机 resume 命令 ＋ 本机 tmux 名", async () => {
    tm.ensureTab("l1", "/w", "/p/l1.jsonl", 0, LOCAL_ORIGIN);
    await home(tm).actions.restartTabWithAccount("l1", "b", false);
    expect(restartSpy).toHaveBeenCalledTimes(1);
    const arg = restartSpy.mock.calls[0][0];
    expect(arg.origin).toBe("<local>");
    expect(arg.tmuxName).toBe("proj-cc");
    expect(arg.accountName).toBe("b");
    expect(arg.launcher).toBe(""); // getBehavior 的 mock：resumeCommandLocal = ""（远端那条是 "cct"）
    expect(invokedCmds()).toContain("list_local_tmux");
    expect(invokedCmds()).not.toContain("list_remote_tmux");
  });

  it("本机会话不在本工具 tmux 里 → 拒重启，提示**不指**本机不存在的那条补救路", async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_local_tmux" ? Promise.resolve([]) : Promise.resolve(undefined),
    );
    tm.ensureTab("l1", "/w", "/p/l1.jsonl", 0, LOCAL_ORIGIN);
    await home(tm).actions.restartTabWithAccount("l1", "b", false);
    expect(restartSpy).not.toHaveBeenCalled();
    const toast = vi.mocked(showActionFailureToast).mock.calls.at(-1);
    expect(toast?.[0]).toBe("无法换号重启");
    expect(String(toast?.[1])).not.toContain("把此会话切到账号 X");
  });
});

describe("A5 restartTabWithAccount 阻塞守卫（精确 @ccm_sid 命中才动手）", () => {
  const restartSpy = restartWithAccount as unknown as ReturnType<typeof vi.fn>;
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
    await home(tm).actions.restartTabWithAccount("target-sid", "z", false);
    expect(restartSpy).not.toHaveBeenCalled();
  });

  it("精确 @ccm_sid 命中 → 放行调编排器（带对的 tmuxName/account）", async () => {
    tm.ensureTab("target-sid", "/home/pi/proj", "/p/t.jsonl", 0, "aya");
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([sess({ name: "cc-target01", sid: "target-sid" })])
        : Promise.resolve(undefined),
    );
    await home(tm).actions.restartTabWithAccount("target-sid", "z", true);
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
    await home(tm).actions.restartTabWithAccount("target-sid", "z", false);
    expect(restartSpy).not.toHaveBeenCalled();
  });

  // F04（R10）：命中 ≥2 个精确同 sid 的活会话——重启是破坏性操作（kill+relaunch），选错的代价
  // 不可逆，故**拒绝**而非"警告+继续"（与非破坏性的 resumeTabTmux 分级不同，见 F04 计划 §2 取舍④）。
  it("目标 sid 同时活在 2 个 tmux（命中 ≥2 个）→ 拒重启、不调编排器（防杀错留活）", async () => {
    tm.ensureTab("target-sid", "/home/pi/proj", "/p/t.jsonl", 0, "aya");
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            sess({ name: "cc-target01", sid: "target-sid" }),
            sess({ name: "cc-target02", path: "/other", sid: "target-sid" }),
          ])
        : Promise.resolve(undefined),
    );
    await home(tm).actions.restartTabWithAccount("target-sid", "z", false);
    expect(restartSpy).not.toHaveBeenCalled();
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "换号重启拒绝",
      expect.stringContaining("2"),
      expect.objectContaining({ level: "info" }),
    );
  });

  // F09 Phase D 审计（UX，重要）：⇄ 按钮删除后，命中 in-flight 守卫曾经完全静默——右键菜单是
  // 唯一入口，点了却毫无反应（含最长 5 分钟 compact 等待窗口），用户大概率以为没点中、再点
  // 一次。已修：给个明确 toast。
  it("同一 sid 重启进行中时再次调用 → 拒绝且给出明确 toast（不再静默无反应）", async () => {
    let resolveTmux!: (v: unknown) => void;
    const pending = new Promise((r) => (resolveTmux = r));
    tm.ensureTab("target-sid", "/home/pi/proj", "/p/t.jsonl", 0, "aya");
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) =>
      cmd === "list_remote_tmux" ? pending : Promise.resolve(undefined),
    );
    const first = home(tm).actions.restartTabWithAccount("target-sid", "z", false);
    const second = await home(tm).actions.restartTabWithAccount("target-sid", "z", false);
    expect(second).toBe(false);
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "正在重启中",
      expect.stringContaining("还没完成"),
      expect.objectContaining({ level: "info" }),
    );
    expect(restartSpy).not.toHaveBeenCalled(); // 第二次调用不该触发编排器
    resolveTmux([]);
    await first;
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

  it("会话账号 != 当前账号(live) → 挂实心头像", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([liveRow("r1", "b")], new Map(), new Map([["aya", "z"]]));
    const el = badge();
    expect(el?.style.display).not.toBe("none");
    expect(el?.querySelector(".acct-avatar")).not.toBeNull();
    expect(el?.querySelector(".acct-avatar.ghost")).toBeNull(); // live = 实心
  });
  // F09（R7 语义反转）：徽章从"仅不一致才挂"变成"账号已知即恒显示身份"——一致态也挂头像，
  // 只是 tooltip 不带"不一致"后缀（视觉区分靠 live 实心/last 幽灵，不是挂/不挂本身）。
  it("会话账号 == 当前账号 → 仍挂徽章（恒显身份），tooltip 不含「不一致」", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([liveRow("r1", "z")], new Map(), new Map([["aya", "z"]]));
    const el = badge();
    expect(el?.style.display).not.toBe("none");
    expect(el?.querySelector(".acct-avatar")).not.toBeNull();
    expect(el?.title).not.toContain("不一致");
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
  it("当前账号未就绪(currentByOrigin 无该 origin) → 仍挂徽章（会话自己的账号已知，身份展示不需要先知道 current），但不判定为不一致（不猜）", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", 0, "aya");
    feed([liveRow("r1", "b")], new Map(), new Map()); // 无 current
    const el = badge();
    expect(el?.style.display).not.toBe("none");
    expect(el?.title).not.toContain("不一致");
  });

});

/**
 * ★★ 〔`设计/80 §8.7` 步 4，第二波 T4〕↗ 远端那一格：**前端不再猜，tmux 不在前提链上**。
 *
 * 这里原先是 E73 那组「↗ 失败之后再打一次 `list_remote_tmux` 分四档归因」的判据。
 * 步 4 把「有没有终端」的四套判断（`attachable` 布尔 · `findClaudeTmuxMatches` · 后端 HWND 校验 ·
 * E73 那次远端 RPC）收成后端一句 ——「这个会话是不是 cc-monitor 启动的」（有没有启动令牌），
 * 归因的判据住 `bind_tests.rs` 那张 64 格真值表。前端这一侧只钉三件事：
 * ① 失败时**一次 tmux 查询都不发**（用户逐字「不能依赖 tmux」）；② 后端那句话**原样**给用户；
 * ③ `attachable:false` 不再在前端短路 ↗（那是被收掉的四套之一）。
 */
describe("设计/80 §8.7 步 4：↗ 远端那一格只问后端一次", () => {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
  const BACKEND_SAYS = "<后端归因原文>";
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    __setHostOsForTests("windows");
  });
  afterEach(() => __setHostOsForTests(null));

  async function clickFront(tm: TabManager, sid: string): Promise<void> {
    const btn = home(tm).bar.tabButtons.get(sid)?.root.querySelector(".tab-focus") as HTMLElement | null;
    expect(btn, "量具自检：Windows 上应当渲出 ↗").not.toBeNull();
    btn!.click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
  }

  it("★★ 失败 ⇒ 后端那句话原样给用户，而且**一次 tmux 查询都不发**", async () => {
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "bring_remote_terminal_to_front" ? Promise.reject(new Error(BACKEND_SAYS)) : Promise.resolve([]),
    );
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "aya", "interactive", null);
    await clickFront(tm, "r1");
    const cmds = mockInvoke.mock.calls.map((c) => c[0]);
    expect(cmds, "量具自检：↗ 真的发到了后端").toContain("bring_remote_terminal_to_front");
    expect(
      cmds.filter((c) => c === "list_remote_tmux" || c === "list_local_tmux"),
      "↗ 失败之后又去查了一次 tmux —— tmux 回到了 ↗ 的前提链上（E73 那次 RPC 是步 4 要收的四套之一）",
    ).toEqual([]);
    // 〔U2〕「拉前」是术语表的禁词（say：「切到终端窗口」），这一格随改词同拍改（行为变更是题面要的，不是迁就）。
    expect(showActionFailureToast).toHaveBeenCalledWith("切到终端窗口失败", BACKEND_SAYS);
  });

  it("★ `attachable:false` 不再在前端短路 ↗ —— 照样问后端（归因是后端那一个布尔的事）", async () => {
    mockInvoke.mockResolvedValue(undefined);
    const tm = makeTM();
    tm.createSkeletonTab("r2", "/p", "aya", "interactive", null, false);
    expect(tm.isAttachable("r2"), "量具自检：这个会话确实被宣告成不可 attach").toBe(false);
    await clickFront(tm, "r2");
    expect(mockInvoke.mock.calls.map((c) => c[0])).toContain("bring_remote_terminal_to_front");
    expect(showActionFailureToast, "成功了还弹了 toast / 前端又替后端解释了一句").not.toHaveBeenCalled();
  });
});

/**
 * ★★ 〔第二波 T4 · LF1〕↗ 在非 Windows 上**别装得能用**。
 *
 * 非 Windows 上 ↗ 的最后一跳（`EnumWindows` / `SetForegroundWindow`）在 Rust 侧是恒失败的桩
 * ⇒ 那颗按钮每点必败。门住 `terminal-front.ts`；`unknown` 照常显示（与 `hostOsAllows` 同一条理由）。
 */
describe("LF1：↗ 只在 Windows 上出现", () => {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });
  afterEach(() => __setHostOsForTests(null));

  const focusBtnOf = (tm: TabManager, sid: string) =>
    home(tm).bar.tabButtons.get(sid)?.root.querySelector(".tab-focus") ?? null;

  it("★★ 两向：linux / macos 不渲 ↗，windows / unknown 渲（本地 tab 与远端 tab 同一道门）", () => {
    const want: [HostOs, boolean][] = [
      ["windows", true],
      ["unknown", true],
      ["linux", false],
      ["macos", false],
    ];
    for (const [os, shown] of want) {
      __setHostOsForTests(os);
      const tm = makeTM();
      tm.ensureTab("l1", "/w", "p", 0, LOCAL_ORIGIN);
      tm.createSkeletonTab("r1", "/p", "aya", "interactive", null);
      expect(home(tm).bar.tabButtons.get("l1"), `${os}：量具自检，tab 按钮本身得在`).toBeDefined();
      expect(focusBtnOf(tm, "l1") !== null, `${os} 本地 tab 的 ↗`).toBe(shown);
      expect(focusBtnOf(tm, "r1") !== null, `${os} 远端 tab 的 ↗`).toBe(shown);
    }
  });

  it("★ 快捷键 / 命令面板在 linux 上走到 ↗ ⇒ 说实话、**不发 IPC**", () => {
    __setHostOsForTests("linux");
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "aya", "interactive", null);
    tm.switchTo("r1");
    tm.bringActiveTerminalToFront();
    const sent = mockInvoke.mock.calls
      .map((c) => c[0])
      .filter((c) => c === "bring_remote_terminal_to_front" || c === "bring_terminal_to_front");
    expect(sent, "linux 上照样发了 ↗ 的 IPC —— 那是装作试过").toEqual([]);
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "本机不能切到终端窗口",
      expect.stringContaining("Windows"),
      expect.objectContaining({ level: "info" }),
    );
  });

  it("★ 对照：windows 上快捷键照常发 IPC（上一条不是因为别的原因没发）", () => {
    __setHostOsForTests("windows");
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "aya", "interactive", null);
    tm.switchTo("r1");
    tm.bringActiveTerminalToFront();
    expect(mockInvoke.mock.calls.map((c) => c[0])).toContain("bring_remote_terminal_to_front");
  });
});

/**
 * ★★ E73 的字段那一半：**`attachable` 把「该不该出现」与「attach 有没有意义」拆成两个轴。**
 *
 * `kind` 此前把这两件事压在一个轴上，而 SDK / 脚本驱动的会话正好「①要②不要」。
 * 那些会话**有** tmux、`@ccm_sid` 也对，只是 `stdin=DEVNULL` —— 于是它精确落进
 * `findIdleTmux` 的判据里（`@ccm_sid` 命中 + 前台不是 claude），monitor 会把它当成
 * **空壳**、给出「杀死会话（kill 空 tmux）」。它以为那是空的，实际里面跑着东西。
 */
describe("E73：attachable 门控", () => {
  it("★ 缺席 = 可以（存量会话与旧后端零迁移）", () => {
    const tm = makeTM();
    tm.createSkeletonTab("s1", "/p", "aya", "interactive", null);
    expect(tm.isAttachable("s1")).toBe(true);
    // 连 tab 都还没有的 sid 也按可以算（不知道 ≠ 不可以）
    expect(tm.isAttachable("从没见过")).toBe(true);
  });

  it("★★ 显式 false → 记账；再宣告成 true / 缺席 → 撤销（不许粘住）", () => {
    const tm = makeTM();
    tm.createSkeletonTab("s1", "/p", "aya", "interactive", null, false);
    expect(tm.isAttachable("s1")).toBe(false);
    // 同一个 sid 后来被宣告成可以（比如 bridge 退出、真人接管）——不能一直挂着
    tm.createSkeletonTab("s1", "/p", "aya", "interactive", null, null);
    expect(tm.isAttachable("s1")).toBe(true);
  });

  it("★ 只认布尔 false，不认别的假值形态", () => {
    const tm = makeTM();
    tm.createSkeletonTab("s1", "/p", "aya", "interactive", null, true);
    expect(tm.isAttachable("s1")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// audit-0805 F14 第五刀（报告 I9′）：`fetchTmuxFresh` 是类内唯一取数点，
// 「取数」与「写缓存」在语法上是同一件事。这里钉它的**三态契约** ——
// 结构守卫（`tmux-cache-single-writer.vitest.ts`）只能证明「只剩一个取数点」，
// 证不了「那一个取数点做对了」。
// ---------------------------------------------------------------------------
describe("tmux 取数点的三态契约（audit-0805 F14 第五刀）", () => {

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("查到会话 → 返回列表，并把它写进缓存", async () => {
    const tm = makeTM();
    const list = [{ name: "cc-a", ccmSid: "s1" }];
    vi.mocked(invoke).mockResolvedValueOnce(list);
    const got = await home(tm).actions.fetchTmuxFresh("box1");
    expect(got).toEqual(list);
    expect(
      home(tm).actions.tmuxCache.get("box1")?.sessions,
      "取了数却没写缓存 —— 报告 I9′ 那条毛病就是这么来的",
    ).toEqual(list);
  });

  it("★ NO_TMUX（null）是**确定答案**，照样写缓存", async () => {
    const tm = makeTM();
    vi.mocked(invoke).mockResolvedValueOnce(null);
    const got = await home(tm).actions.fetchTmuxFresh("box1");
    expect(got).toBeNull();
    expect(
      home(tm).actions.tmuxCache.has("box1"),
      "「远端确实没有 tmux」是查到了的结果，不写缓存等于每次都要重问一遍",
    ).toBe(true);
  });

  it("★ 查询失败 → undefined 且**不写缓存**（三态不许压成两态）", async () => {
    const tm = makeTM();
    vi.mocked(invoke).mockRejectedValueOnce(new Error("ssh 抖了一下"));
    const got = await home(tm).actions.fetchTmuxFresh("box1");
    expect(
      got,
      "把失败压成 null 会让「远端确实没有会话」和「我没问到」变得无法区分",
    ).toBeUndefined();
    expect(
      home(tm).actions.tmuxCache.has("box1"),
      "★ 一次 ssh 抖动被写进缓存 ⇒ 之后 8s 内的重试全被抑制（D-Sug3 就是防这个）",
    ).toBe(false);
  });
});

// ═══ audit-0805 F15：每来一行，到底调了几次 ═══════════════════════════════
//
// 这一组是**现状基线**（characterization）：它先把「每行的代价」变成一个**可判定的数**，
// 合批本体改完之后再把这些数收紧。没有它，V5 那句话就成立 ——
// 「行为上与不改完全等价（同样的行、同样的结果），**慢不会让任何测试变红**」。
//
// ⚠ 上一轮建不起来，不是因为没人写，是因为**三处 mock 叠在一起把它测没了**：
//   ① `routeMetaAndBranch` 不调 `sink.onBranchRecord` ② `renderContentRecord` 不塞 timeline
//   ③ `RecordTimeline.size` 恒 0。三处少改一处，下面每条都会零命中地绿。
describe("F15 每行代价的现状基线", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.useFakeTimers();
    tm = makeTM();
    f15.recordAdded = 0;
    f15.rebuildNow = 0;
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const line = (sid: string, seq: number) => ({
    session_id: sid,
    cwd: "/w",
    path: `/w/${sid}.jsonl`,
    seq,
    message: { type: "assistant", uuid: `u-${seq}`, timestamp: "2026-08-06T00:00:00Z" },
  });

  it("★ live 每来一行，BranchFolder 就被喂一次", () => {
    tm.ensureTab("s1", "/w", "/w/s1.jsonl", 0, LOCAL_ORIGIN);
    for (let i = 1; i <= 5; i++) tm.onLine(line("s1", i) as never);
    // 抽取器自检：三处 mock 只要有一处退回空壳，这里就是 0 —— 那不是「合批做好了」。
    expect(
      f15.recordAdded,
      "`recordAdded` 一次都没被调到 —— 不是合批生效了，是 `routeMetaAndBranch` 的 mock " +
        "又不调 `sink.onBranchRecord` 了（F15 §2 那个坑）。先修量具再读这个数。",
    ).toBeGreaterThan(0);
    expect(
      f15.recordAdded,
      "喂 5 行、BranchFolder 被调的次数变了（该是 5）。⚠ 这个数**不因合批而降**：" +
        "F15 的合批做在 `BranchFolder` **内部**（帧末只算一次主线），而本文件把 " +
        "`BranchFolder` 整个 stub 掉了，量到的是「被喂了几次」。" +
        "真正省下的那次 O(N) 由 `branch-fold-batching.vitest.ts` 钉。",
    ).toBe(5);
  });

  it("★ 后台 tab 每来一行，tab bar 就整刷一次", () => {
    tm.ensureTab("front", "/w", "/w/front.jsonl", 0, LOCAL_ORIGIN);
    tm.ensureTab("bg", "/w", "/w/bg.jsonl", 0, LOCAL_ORIGIN);
    tm.switchTo("front");
    let refreshes = 0;
    const inner = tm as unknown as { refreshTabBar: () => void };
    const real = inner.refreshTabBar.bind(inner);
    inner.refreshTabBar = () => {
      refreshes++;
      real();
    };
    for (let i = 1; i <= 4; i++) tm.onLine(line("bg", i) as never);
    // ★ 合批之后：**帧内一次都不刷**。
    expect(
      refreshes,
      `后台 tab 连来 4 行，帧内就刷了 ${refreshes} 次 tab bar —— ` +
        "unread 那条路（`tabs.ts:893-897`）又变回逐行整刷了。",
    ).toBe(0);
    // 抽取器自检：unread 真的涨了，才说明这条链走到了（否则下面是零命中地绿）。
    expect(
      home(tm).store.tabs.get("bg")!.unread,
      "后台 tab 的 unread 一条都没涨 —— 多半是 `tabs.ts:891` 的 `inserted` 又恒假了" +
        "（`renderContentRecord` 不塞 timeline / `size` 恒 0）。那时这条判据什么也没量。",
    ).toBe(4);
    vi.advanceTimersByTime(50);
    expect(
      refreshes,
      `帧末刷了 ${refreshes} 次（该是 1）。0 = 合批变成了「永远不刷」，徽标永远不更新；` +
        ">1 = 排一次位没生效。",
    ).toBe(1);
  });
});

// ===== P7a-1（#61）：独立归档区 =====
//
// `#61` 正文自陈「状态机已经有了，缺的是那个「口」」。判据钉的是**分流**本身：
// 主栏里没有它 **且** 抽屉里有它 —— 两面都钉，否则「两边各渲一份」也能过。

describe("已结束的 tab 留在原位灰着（原「P7a-1 独立归档区」）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });




  it("★ 已结束的 tab 留在主栏，一个都不许消失（连 barEl 没有父节点时也一样）", () => {
    // 🔴 〔步 17·A · 2026-09-19〕**本条留着，但它买的东西变了 —— 从「边界情形」变成「正题」。**
    //
    // 旧理由：抽屉是 `barEl` 的兄弟，拿不到父节点时若仍把 tab 挪进那个**孤儿**容器，
    // 它就从文档里整个不见了。那时这是一条**边界**判据（只在拿不到父节点时有意义）。
    //
    // 今天抽屉整个删了（用户逐字「没有归档这个东西……就是灰 tab」，`设计/30 §A`）
    // ⇒ 「已结束的 tab 留在主栏灰着」**成了正常行为本身**，这一条于是是它的正面断言。
    // ⚠ 孤儿 `barEl` 那个布景**刻意留着**：它仍是一个真的边界情形，而且这条断言
    //   在那个布景下也该成立 —— 顺手把两件事一起买了。
    // ⚠ 「灰着但还在」这句话来自 `tabs.ts` 自己的旧头注，而用户 2026-09-19 逐字说了同一句。
    document.body.innerHTML = "";
    const orphanBar = document.createElement("div"); // 刻意不 append 到 body
    const streamRootEl = document.createElement("div");
    document.body.append(streamRootEl);
    const tm2 = new TabManager(orphanBar, streamRootEl);
    tm2.ensureTab("a", "/c", "p", 0, LOCAL_ORIGIN);
    tm2.ensureTab("b", "/c", "p", 0, LOCAL_ORIGIN);
    tm2.switchTo("a");
    tm2.archiveTab("b");
    (tm2 as unknown as { refreshTabBar: () => void }).refreshTabBar();
    const inBar = [...orphanBar.children].filter((e) => e.classList.contains("tab"));
    expect(inBar, "两个都该还在主栏 —— 一个都不许被挪进孤儿容器").toHaveLength(2);
  });

});

// ===== P7a-2（#61）：栏内拖动排序 =====
//
// ★ 摸底订正过 ROADMAP 的措辞：tear-off 的 arm 条件是 `e.clientX > barRight + 16`，
// 而那是一条**竖栏** ⇒ 空闲的不是「横向」，是 **`clientY` 从没被用过**。
// 重排走纵向，与撕离天然不争同一根轴。

describe("P7a-2 moveTabBlock（纯）", () => {
  it("★ P7a2-Y1：整块搬到某个 sid 之前 / 末尾，落位逐项对得上", () => {
    const o = ["a", "b", "c", "d"];
    expect(moveTabBlock(o, ["c"], "a")).toEqual(["c", "a", "b", "d"]);
    expect(moveTabBlock(o, ["a"], "d")).toEqual(["b", "c", "a", "d"]);
    expect(moveTabBlock(o, ["a"], null)).toEqual(["b", "c", "d", "a"]);
    // 多元素的块保持内部相对序。
    expect(moveTabBlock(o, ["b", "c"], "a")).toEqual(["b", "c", "a", "d"]);
    // 不改原数组。
    expect(o).toEqual(["a", "b", "c", "d"]);
  });

  it("★ P7a2-Y1b：落点在块内 ⇒ **原样返回**（拖到自己身上不是一次重排）", () => {
    const o = ["a", "b", "c"];
    expect(moveTabBlock(o, ["b"], "b")).toEqual(o);
    expect(moveTabBlock(o, ["a", "b"], "b")).toEqual(o);
    // 把它算成「挪到末尾」是错的 —— 那会让一次误触把 tab 甩到最后。
    expect(moveTabBlock(o, ["b"], "b")).not.toEqual(["a", "c", "b"]);
  });
});

describe("P7a-2 栏内拖动排序（真拖拽）", () => {
  let tm: TabManager;
  let bar: HTMLElement;
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    bar = document.body.firstElementChild as HTMLElement;
  });
  const flushBar = (): void =>
    (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
  const order = (): string[] => home(tm).store.orderedIds;
  /** jsdom 的 getBoundingClientRect 恒零 ⇒ 按主栏里的顺序给每个 tab 造一条 40px 的带。 */
  const stubRects = (): void => {
    const kids = [...bar.children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    kids.forEach((el, i) => {
      el.getBoundingClientRect = () =>
        ({ top: i * 40, height: 40, bottom: i * 40 + 40, left: 0, right: 100 }) as DOMRect;
    });
  };
  const dragTo = (sid: string, clientY: number, clientX = 10): void => {
    // ⚠ tab 根元素上**没有** sid 属性（实测：`createTabButton` 只设 class）。
    // 主栏里的 DOM 顺序 == `orderedIds` 里主栏那部分的顺序（`refreshTabBar` 保证），
    // ⇒ 按下标取，别按文本猜。第一版按 `title.includes(sid)` 找，恒取到第一个 ⇒ 两条判据假红。
    const roots = [...bar.children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    const idx = order().indexOf(sid);
    const root = roots[idx];
    expect(root, `主栏里找不到 ${sid}`).toBeTruthy();
    root.dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX, clientY, bubbles: true }),
    );
    document.dispatchEvent(new MouseEvent("mouseup", { clientX, clientY, bubbles: true }));
  };

  it("★ P7a2-Y1：纵向拖动重排主栏（落位逐项对得上）", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", 0, LOCAL_ORIGIN);
    flushBar();
    stubRects();
    expect(order()).toEqual(["a", "b", "c"]);
    // 拖第三个（c）到最上面：clientY=10 落在第一条（0..40）的上半 ⇒ 插到 a 之前。
    dragTo("c", 10);
    expect(order()).toEqual(["c", "a", "b"]);
  });

  it("★ P7a2-Y3：拖交互 tab 时，它的 bg 子串**跟着走**（不许把树拆散）", () => {
    // 造一棵真的树：宿主 + 两个同 cwd 的 bg 子项（`placeInOrder` 会把它们锚在宿主之后）。
    tm.ensureTab("host", "/proj/a", "p", 0, LOCAL_ORIGIN);
    tm.createSkeletonTab("bg1", "/proj/a", LOCAL_ORIGIN, "bg", "t1");
    tm.createSkeletonTab("bg2", "/proj/a", LOCAL_ORIGIN, "bg", "t2");
    tm.ensureTab("other", "/proj/z", "p", 0, LOCAL_ORIGIN);
    flushBar();
    stubRects();
    expect(order()).toEqual(["host", "bg1", "bg2", "other"]);
    // 把 other 拖到最上面 —— 它没有子项，只有它自己动。
    dragTo("other", 10);
    expect(order()).toEqual(["other", "host", "bg1", "bg2"]);
    stubRects();
    // 再把 host 拖到最上面：**整串跟着走**，顺序不许被打散。
    dragTo("host", 10);
    expect(order()).toEqual(["host", "bg1", "bg2", "other"]);
  });

  it("★ P7a2-D：拖动时**落点看得见**，撕离那一路不指示，拖完必须清掉", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", 0, LOCAL_ORIGIN);
    flushBar();
    stubRects();
    const roots = [...bar.children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    const marked = (): number => bar.querySelectorAll(".tab.drop-before").length;

    roots[2].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 10, bubbles: true }),
    );
    // 落点 = 第一条（a）之前。
    expect(marked()).toBe(1);
    expect(roots[0].classList.contains("drop-before")).toBe(true);

    // 拖出右缘 ⇒ armed，那一路根本不重排 ⇒ 不许再指一条不会发生的落点。
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 9999, clientY: 10, bubbles: true }),
    );
    expect(marked(), "撕离那一路不指示落点").toBe(0);

    // ⚠ **必须先拖回非 armed 让标记重新挂上**，否则松手时本来就没标记，
    // 「拖完清干净」那句是恒真的（实测：拿掉 teardown 里的清理，判据照样绿）。
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 10, bubbles: true }),
    );
    expect(marked(), "拖回栏内应重新指示").toBe(1);

    document.dispatchEvent(
      new MouseEvent("mouseup", { clientX: 10, clientY: 10, bubbles: true }),
    );
    expect(marked(), "拖完必须清干净，标记不许挂在那儿").toBe(0);
  });

  it("★ P7a2-Y2：armed（拖出右缘）时**顺序一个字不动**", () => {
    tm.ensureTab("a", "/c", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    flushBar();
    stubRects();
    const before = [...order()];
    const roots = [...bar.children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    roots[0].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    // clientX 远超 barRight+16 ⇒ armed
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 9999, clientY: 100, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mouseup", { clientX: 9999, clientY: 100, bubbles: true }),
    );
    expect(order(), "撕窗口那一路不许顺带重排").toEqual(before);
  });

  /**
   * ★ 6d（条 54）：**拖到一半，tab 不许自己跳位置。**
   *
   * # 机制（已核实，不是推测）
   *
   * `refreshTabBar` 全身**没有任何 `this.drag` 守卫**，而它挂在活动路上：
   * `updateActivity` / `archiveTab` / `ensureTab` 末尾都无条件调它。
   * ⇒ 拖到一半来一条活动事件，第 4 段那个「排序」循环就照 `orderedIds` 重排一次 DOM，
   * **指针底下的那个 tab 当场被换掉** —— 用户松手落到的不是他瞄的那一格。
   *
   * # 🔴 为什么注入的是「会话结束」而不是一条 `updateActivity`
   *
   * 反空真自检（`设计/01 §7.4`「扫到空集时要红，不是绿」）：
   * 一条**不改 `orderedIds`、不改归档归属**的 `updateActivity`，
   * 走完 `refreshTabBar` 之后 `refs.root === targetNext` 恒成立 ⇒ 一次 `insertBefore` 都不会发生
   * ⇒ 那样写出来的判据**拿掉守卫也是绿的**，等于没买。
   * 真会动 DOM 顺序的活动事件有两类，这里取第一类（第二类见下一格）：
   *   ① **会话跑完 ⇒ 归档** —— 归档抽屉是 `barEl` 的**兄弟**（`ensureArchiveUi`），
   *      那个 tab 会整个**离开** `#tab-bar`，它下面的全部上移一格；
   *   ② **新会话/新 bg 宣告** —— `placeInOrder` 把 bg 锚在宿主之后 ⇒ 从**中间**插进去。
   *
   * # 判据钉的是「顺序一次都不变」，不是「最终顺序对不对」
   *
   * 最终顺序在 `mouseup` 之后本来就会对（那时重画照样发生）。
   * 坏的是**拖拽窗口之内**那一次重排 —— 所以快照要在 `mouseup` 之前比。
   */
  it("★ 6d：拖拽进行中来一条活动事件 ⇒ mouseup 之前 #tab-bar 子节点顺序一次都不变", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN); // 首个 tab ⇒ 它是 active（`switchTo(_, "auto")`）
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", 0, LOCAL_ORIGIN);
    flushBar();
    stubRects();
    // 主栏子节点的「长相顺序」。用 textContent 而不是下标 —— 少一个、换一个都要能看出来。
    const barSnap = (): string =>
      [...bar.children].map((e) => `${e.className}#${e.textContent ?? ""}`).join(" | ");

    const roots = [...bar.children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    // 拖最后那个（c），指针停在第一条（a，0..40）的上半 ⇒ 落点 = a 之前。
    roots[2].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 10, bubbles: true }),
    );
    const during = barSnap();
    expect(during, "前置：三个 tab 都还在主栏里").toContain("tab ");

    // ⬇ 拖拽进行中注入一次活动事件：b（**不是 active、在被拖的 c 上面**）的会话跑完了。
    tm.archiveTab("b");

    expect(barSnap(), "拖拽中 tab 不许自己跳位置：松手前 #tab-bar 的子节点顺序一次都不许变").toBe(
      during,
    );

    // ⬇ 松手之后**必须补上**那一次重画 —— 守卫不是「把刷新永久吞掉」。
    document.dispatchEvent(
      new MouseEvent("mouseup", { clientX: 10, clientY: 10, bubbles: true }),
    );
    expect(barSnap(), "松手后归档那一格要落实，否则守卫就成了静默丢刷新").not.toBe(during);
    // 🔴 〔步 17·A · 2026-09-19〕后置断言换了，**本条的正题一个字没动**。
    //   原来这里断言「b 已搬进归档抽屉」。抽屉整个删了（用户逐字「没有归档这个东西」，
    //   `设计/30 §A`「已定：删归档抽屉」）⇒ 新行为是**留在原位灰着**。
    //   本条买的仍是条 54：**拖拽进行中 tab 不自己跳位置**；变的只是「松手后落实成什么」。
    const barTabs = [...bar.children].filter((e) => e.classList.contains("tab"));
    expect(barTabs.length, "三个 tab 必须都还在主栏里 —— 归档不再把谁搬走").toBe(3);
    expect(
      barTabs.filter((e) => e.classList.contains("ended")).length,
      "b 已结束 ⇒ 原位变淡（`.tab.ended`，〔U4〕原名 `.tab.archived`），而不是消失进另一个容器",
    ).toBe(1);
    expect(order(), "拖动本身的结果照常落实").toEqual(["c", "a", "b"]);
  });
});

// ===== P7a-3（#61）：标签页集合的**渲染**那半 =====
describe("P7a-3 集合分组渲染", () => {
  let tm: TabManager;
  let bar: HTMLElement;
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    bar = document.body.firstElementChild as HTMLElement;
  });
  const flushBar = (): void =>
    (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
  const setCols = (cols: unknown): void => {
    home(tm).prefs.collections = cols as TabCollection[];
  };
  const order = (): string[] => home(tm).store.orderedIds;

  // ── 🔴 〔步 17·A · 2026-09-19〕墓碑：五条测「归档抽屉」的判据整块删除 ──────
  //
  // 删的是：`P7a1-Y1`（归档 tab 离开主栏进抽屉）· `P7a1-Y2`（当前 tab 绝不进抽屉）
  // · `P7a1-Y2b`（切走之后才落进抽屉）· `P7a1-Y3`（抽屉里的 tab 不挂 tear-off）
  // · `P7a3-Y2b`（归档优先于集合：灰 tab 进抽屉不进组）。
  //
  // **它们没有失效，是被测的那个功能整个不存在了。** 用户 2026-09-19 逐字：
  // 「没有归档这个东西，不要归档，就是灰 tab。现在的归档是错误的，甚至是 bug 的来源，
  //   全部删掉。」而 `设计/30 §A` 抬头本来就写着「**已定**：删归档抽屉 · 固定灰 tab」。
  //
  // ⚠ **`P7a3-Y2b` 那条的判词今天正好反过来**：它断言「灰 tab 进抽屉**不进组**」，
  //   而 `§A.3` 逐字「**灰 tab 也能在组里**，更符合直觉」⇒ 留着它就是把设计判红。
  //   下面 `P7a3` 那一族里已经有「按集合分组」的正向判据盖住新行为，不另立。
  //
  // ⚠ 留下这块墓碑而不是静悄悄删：`设计/16 §5.3` 那条 —— 一条判据消失时，
  //   「它被删了」与「它从来没有过」在盘上长得一模一样。

  it("★ P7a3-Y2：成员进它的组，非成员照常直接挂主栏", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", 0, LOCAL_ORIGIN);
    setCols([{ id: "g1", name: "白天", members: ["a", "c"] }]);
    flushBar();

    const group = bar.querySelector<HTMLElement>(".tab-group")!;
    expect(group, "组容器要在").toBeTruthy();
    expect(group.querySelector(".tab-group-name")!.textContent).toBe("白天");
    // 成员真的在组里（两个），非成员一个都不在。
    expect(group.querySelectorAll(".tab-group-list > .tab")).toHaveLength(2);
    // 非成员直挂主栏（不是任何组的子孙）。
    const loose = [...bar.children].filter((e) => e.classList.contains("tab"));
    expect(loose).toHaveLength(1);
  });

  it("★★ P7a3-E：**没拉过集合的实例不许写集合** —— viewer 窗口会把用户已有的全冲掉", () => {
    // 撕离出来的 viewer 窗口也用 TabManager（`main.ts:938`，tab 栏由 .viewer-mode 隐藏），
    // 但它**从不 loadCollections** ⇒ `collections` 恒空。右键菜单里若还留着「新建集合…」，
    // 点一下就把「只含这一个」的列表写回 config.json —— 用户已有的集合全没了。
    // 同族先例就在旁边一行：「viewer 窗口共享 localStorage，禁写 last-active（防污染主窗口记忆）」。
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    flushBar();
    const root = [...bar.children].find((e) => e.classList.contains("tab")) as HTMLElement;
    root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const labels = [...document.querySelectorAll(".tab-context-menu button")].map(
      (e) => e.textContent ?? "",
    );
    expect(labels.length, "菜单要真的开出来（否则本判据在空转）").toBeGreaterThan(0);
    expect(labels.join("|"), "没拉过集合就不给集合入口").not.toContain("加入集合");
  });

  it("★ P7a3-E 反面：**拉过了就必须给入口**（否则「永远不给」也能过）", async () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    await tm.loadCollections(); // mock 的 config 是空的 ⇒ 集合为空，但「拉过」为真
    flushBar();
    const root = [...bar.children].find((e) => e.classList.contains("tab")) as HTMLElement;
    root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const labels = [...document.querySelectorAll(".tab-context-menu button")].map(
      (e) => e.textContent ?? "",
    );
    expect(labels.join("|")).toContain("加入集合");
  });

  it("★ P7a3-D：组在前、未归组的在后（DoD 逐字如此，实现不许自己反过来）", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN); // 归组
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN); // 散的
    setCols([{ id: "g1", name: "白天", members: ["a"] }]);
    flushBar();
    const kids = [...bar.children];
    const gi = kids.findIndex((e) => e.classList.contains("tab-group"));
    const ti = kids.findIndex((e) => e.classList.contains("tab"));
    expect(gi, "组容器要在").toBeGreaterThanOrEqual(0);
    expect(ti, "散 tab 要在").toBeGreaterThanOrEqual(0);
    expect(gi, "组排在未归组的之前").toBeLessThan(ti);
  });


  it("★ P7a3-Y3：解散集合**一个会话都不许少**", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    setCols([{ id: "g1", name: "白天", members: ["a"] }]);
    flushBar();
    const before = [...order()];
    expect(bar.querySelector(".tab-group")).toBeTruthy();

    (bar.querySelector(".tab-group-del") as HTMLButtonElement).click();
    flushBar();
    // 「没删掉会话」是**没有发生的事** ⇒ 钉逐项相等，不是钉「没崩」。
    expect(order(), "集合是个视图，不是容器").toEqual(before);
    expect(bar.querySelector(".tab-group"), "组容器该没了").toBeNull();
    expect([...bar.children].filter((e) => e.classList.contains("tab"))).toHaveLength(2);
  });

  it("空集合也留着 —— 刚建的集合不该看不见", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    setCols([{ id: "g1", name: "空的", members: [] }]);
    flushBar();
    expect(bar.querySelector(".tab-group")).toBeTruthy();
    expect(bar.querySelectorAll(".tab-group-list > .tab")).toHaveLength(0);
  });
});

// ==========================================================================
// 〔步 17·B · `设计/30 §B`〕固定（pinned）—— 「关了 app 再打开它还在」
//
// `§B.1` 现打：在这之前全仓 `pinned`/`isPinned`/`pinTab` **零命中**。
// `§B.3` 的承重：它**必须是正交的一维**，不能做成 `TabStatus` 的第三态 ——
//   因为 `archived + pinned` 才是用户的主用例（固定住一个已经跑完的会话）。
//
// 🔴 反空真：这一组的 config 是**一份真的在内存里的盘**（走那个已经被 mock 的
//   `invoke`，`load_config`/`save_config` 两条命令），所以「落盘了没有」是
//   **读盘对拍**，不是「有没有调过某个函数」。
// ==========================================================================
describe("步 17·B 固定：落盘 · 复活 · 正交", () => {
  let tm: TabManager;
  let disk: Record<string, unknown>;
  const tabOf = (sid: string): Tab => home(tm).store.tabs.get(sid)!;
  /** 落盘是 fire-and-forget（`先改内存再落盘`）⇒ 读盘前要把那条链子跑完。
   *  两次 `Promise.resolve()` 不够：`writeSegKey` 里 load→save 是两跳 `invoke`。 */
  const flushDisk = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
  const pinnedOnDisk = (): unknown[] => {
    const seg = disk.tabBar as Record<string, unknown> | undefined;
    return (seg?.pinned as unknown[]) ?? [];
  };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    document.body.querySelectorAll(".tab-context-menu").forEach((n) => n.remove());
    disk = {};
    // 🔴 config 走 `invoke` 这一层（与仓里 `src/config.ts` 的真实链路一致）：
    //   上面的 `tab-bar-state` / `tab-collections` 因此是**真跑**的，
    //   判据买到的是那一段的形状（只动自己那个键 · 清洗 · 上界），不是一个 spy。
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "save_config") {
        disk = JSON.parse(JSON.stringify((args as { value: unknown }).value));
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    tm = makeTM();
  });

  it("🔴 量具自检：这一组的「盘」真的在读写（不过这格，下面全是空转）", async () => {
    disk = { tabBar: { pinned: [{ sid: "写进去的", origin: LOCAL_ORIGIN, title: "T" }] } };
    await tm.loadPinned();
    expect(home(tm).store.tabs.has("写进去的"), "盘上的东西没被读进来 ⇒ 下面的复活判据全是空转").toBe(
      true,
    );
    await tm.loadPinned(); // 幂等：再读一次不该多出东西
    expect(home(tm).store.orderedIds).toEqual(["写进去的"]);
  });

  it("复活：盘上一条 ⇒ 造出一个**灰着的**骨架 tab，字段逐个对拍（`§B.5`）", async () => {
    disk = {
      tabBar: {
        pinned: [
          {
            sid: "s1",
            jsonlPath: "/p/s1.jsonl",
            cwd: "/home/u/proj",
            origin: LOCAL_ORIGIN,
            account: "work",
            lastActiveAt: 1700000000000,
            kind: "interactive",
            name: null,
            title: "存下来的标题",
          },
        ],
      },
    };
    await tm.loadPinned();
    const t = tabOf("s1");
    // 〔U4b · 说不清〕本机还没报过清单 ⇒ **说不清**（`设计/30 §3.5.7a`：`Unseen` 不许显示成已结束）；
    //   报完了、清单里没有它 ⇒ 已结束（`§B.5` 那句「没有活进程」这才成立）。
    expect(t.state, "那台还没报完清单 ⇒ 说不清，不许说成已结束").toEqual(UNSEEN);
    tm.markOriginSeen(LOCAL_ORIGIN, new Set());
    expect(t.state, "清单报完了、里面没有它 ⇒ 已结束").toEqual(ENDED);
    expect(t.pinned, "复活出来的当然是固定的").toBe(true);
    expect(t.title, "🔴 标题要用存下来的那份 —— 不等读文件（`§B.5` 逐字）").toBe("存下来的标题");
    expect(t.parentPath, "jsonlPath 是复活的必需品，没落到 tab 上等于白存").toBe("/p/s1.jsonl");
    expect(t.cwd).toBe("/home/u/proj");
  });

  it("🔴 盘上那条**已经活着**时：只补 `pinned`，`status` 一个字不碰", async () => {
    // 后端 `event_replay` 可能已经先宣告了它。把一条真活着的会话按回 archived 是一句假话。
    tm.ensureTab("s1", "/c", "/real.jsonl", 0, LOCAL_ORIGIN);
    disk = { tabBar: { pinned: [{ sid: "s1", origin: LOCAL_ORIGIN, title: "旧标题", jsonlPath: "/old.jsonl" }] } };
    await tm.loadPinned();
    expect(tabOf("s1").state, "🔴 把活着的会话按成已结束了").toEqual(LIVE);
    expect(tabOf("s1").pinned).toBe(true);
    expect(tabOf("s1").parentPath, "活着那条的真路径不许被盘上的旧值覆盖").toBe("/real.jsonl");
  });

  it("`togglePin` ⇒ 内存翻转 ＋ **盘上真的出现/消失那一条**", async () => {
    await tm.loadPinned(); // 先取得资格
    tm.ensureTab("s1", "/home/u/proj", "/p/s1.jsonl", 0, LOCAL_ORIGIN);
    tm.togglePin("s1");
    await flushDisk();
    expect(tabOf("s1").pinned).toBe(true);
    expect(
      (pinnedOnDisk() as { sid: string; jsonlPath: string }[]).map((p) => [p.sid, p.jsonlPath]),
      "落盘的记录不对（sid / jsonlPath 是复活的两个必需品）",
    ).toEqual([["s1", "/p/s1.jsonl"]]);

    tm.togglePin("s1");
    await flushDisk();
    expect(tabOf("s1").pinned).toBe(false);
    expect(pinnedOnDisk(), "取消固定之后盘上还留着 ⇒ 下次启动它又回来了").toEqual([]);
  });

  it("🔴 没 `loadPinned` 过的实例：`togglePin` 什么都不做（防静默清空）", async () => {
    // `persistPinned` 是**按当前 tab 重算整张表**写回去的 —— 没读过盘就写 = 清空。
    // 撕离出来的 viewer 窗口正是这种实例（与 `collectionsLoaded` 同一条理由）。
    disk = { tabBar: { pinned: [{ sid: "别人固定的", origin: LOCAL_ORIGIN, title: "T" }] } };
    tm.ensureTab("s1", "/c", "p", 0, LOCAL_ORIGIN);
    tm.togglePin("s1");
    await flushDisk();
    expect(tabOf("s1").pinned, "没资格就不许改内存").toBe(false);
    expect(
      (pinnedOnDisk() as { sid: string }[]).map((p) => p.sid),
      "🔴 用户上次固定的那条被冲掉了",
    ).toEqual(["别人固定的"]);
  });

  it("`closeTab` 一个固定的灰 tab ⇒ 盘上那条跟着摘掉（点了 × 就不该第二天还在）", async () => {
    await tm.loadPinned();
    tm.ensureTab("s1", "/c", "/p/s1.jsonl", 0, LOCAL_ORIGIN);
    tm.togglePin("s1");
    await flushDisk();
    expect((pinnedOnDisk() as { sid: string }[]).map((p) => p.sid)).toEqual(["s1"]);

    tm.archiveTab("s1");
    tm.closeTab("s1");
    await flushDisk();
    expect(pinnedOnDisk(), "关掉了还留在盘上 ⇒ 下次开 app 它又回来了").toEqual([]);
  });

  it("🔴 正交：`archived` 与 `pinned` **同时成立**（`§B.3` 的主用例）", async () => {
    await tm.loadPinned();
    tm.ensureTab("s1", "/c", "p", 0, LOCAL_ORIGIN);
    tm.togglePin("s1"); // live 也能 pin（`§B.3b`：效果等它变灰才显现）
    expect(tabOf("s1").state).toEqual(LIVE);
    expect(tabOf("s1").pinned).toBe(true);
    tm.archiveTab("s1");
    expect(tabOf("s1").state, "四种组合的第四格 —— pin 唯一真正生效的那一格").toEqual(ENDED);
    expect(tabOf("s1").pinned, "🔴 归档把 pin 冲掉了 ⇒ 正交性破了，主用例没了").toBe(true);
  });

  it("🔴 `§B.3b`：固定**不影响位置** —— `orderedIds` 一个字不许动", async () => {
    await tm.loadPinned();
    for (const s of ["a", "b", "c"]) tm.ensureTab(s, "/c", "p", 0, LOCAL_ORIGIN);
    const before = [...home(tm).store.orderedIds];
    tm.togglePin("b"); // 固定中间那个
    expect(home(tm).store.orderedIds, "有人把固定的排到前面去了 —— 那是被 `§B.3b` 删掉的「固定区」").toEqual(
      before,
    );
    expect(before).toEqual(["a", "b", "c"]);
  });

  it("📌 角标：`.tab.pinned` 跟着 `Tab.pinned` 走，且角标元素真的在（正反两控）", async () => {
    await tm.loadPinned();
    tm.ensureTab("s1", "/c", "p", 0, LOCAL_ORIGIN);
    const root = home(tm).bar.tabButtons.get("s1")!.root;
    expect(root.classList.contains("pinned"), "还没固定就显角标 ⇒ 这个类没在跟着值走").toBe(false);
    tm.togglePin("s1");
    expect(root.classList.contains("pinned"), "固定了却不显 —— 用户看不出哪些是固定的").toBe(true);
    expect(root.querySelector(".tab-pin")?.textContent, "角标元素不在 / 内容不对").toBe("📌");
  });

  it("右键菜单：那一项的文案跟着状态翻转；没资格时整项不出现", async () => {
    const rightClick = (sid: string): void => {
      home(tm)
        .bar.tabButtons.get(sid)!
        .root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    };
    const labels = (): string[] =>
      [
        ...(document.body.querySelector(".tab-context-menu")?.querySelectorAll(".tab-context-menu-item") ??
          []),
      ].map((b) => b.textContent ?? "");

    tm.ensureTab("s1", "/c", "p", 0, LOCAL_ORIGIN);
    rightClick("s1");
    expect(labels(), "🔴 没 `loadPinned` 过就给入口 ⇒ 点一下清空用户的固定表").not.toContain(
      "📌 固定此标签",
    );

    await tm.loadPinned();
    rightClick("s1");
    expect(labels()).toContain("📌 固定此标签");
    (
      [...document.body.querySelectorAll(".tab-context-menu-item")].find(
        (b) => b.textContent === "📌 固定此标签",
      ) as HTMLButtonElement
    ).click();
    expect(tabOf("s1").pinned, "菜单项点了没反应 —— 这个仓刚因为这个撤掉过两个取色器").toBe(true);
    rightClick("s1");
    expect(labels(), "已经固定了还显「固定此标签」⇒ 用户没法取消").toContain("取消固定");
  });

  it("🔴 `§B.6` 第一格：复活出来的空 tab 里**有话说**，两种情形两种话（正反两控）", async () => {
    disk = {
      tabBar: {
        pinned: [
          { sid: "ok", origin: LOCAL_ORIGIN, jsonlPath: "/p/ok.jsonl", title: "有记录的" },
          { sid: "bad", origin: LOCAL_ORIGIN, jsonlPath: "", title: "没记录的" },
        ],
      },
    };
    await tm.loadPinned();
    const hintOf = (sid: string): HTMLElement =>
      tabOf(sid).streamEl.querySelector(".pin-revived-hint") as HTMLElement;

    expect(hintOf("ok"), "复活的 tab 里一片空白 —— 那与坏了没有区别").toBeTruthy();
    expect(hintOf("ok").querySelector(".pin-revived-hint-btn")?.textContent).toBe("Resume 这个会话");

    expect(
      hintOf("bad").querySelector("strong")?.textContent,
      "`§B.6` 逐字：`jsonlPath` 为空的要提示「这个会话没有留下记录」",
    ).toBe("这个会话没有留下记录");
    expect(
      hintOf("bad").querySelector(".pin-revived-hint-btn"),
      "🔴 没有记录还给一个 Resume 按钮 ⇒ 那就是一个点了必失败的按钮",
    ).toBeNull();
  });

  it("resume 真接上了（`reviveTab`）⇒ 那块空态提示自己走掉", async () => {
    disk = { tabBar: { pinned: [{ sid: "s1", origin: LOCAL_ORIGIN, jsonlPath: "/p/s1.jsonl", title: "T" }] } };
    await tm.loadPinned();
    expect(tabOf("s1").streamEl.querySelector(".pin-revived-hint")).toBeTruthy();
    tm.reviveTab("s1");
    expect(
      tabOf("s1").streamEl.querySelector(".pin-revived-hint"),
      "会话已经活了，那块「只能 resume」的话还挂在内容上面",
    ).toBeNull();
  });
});

// ==========================================================================
// 〔步 17·D · `设计/30 §D`〕Edge 式拖动合并成组 —— **纯判定那一半**
//
// `§D.1` 现打：数据层全都在（`TabCollection` 与它那五个操作、落盘、上界、组容器渲染）
// ⇒ **缺的只有那个手势**。所以这一组打的是手势拆出来的那几个纯函数：
// 落点（3 种语义）· 停留（时间维）· 默认组名 · 归属跟着落点宿主走。
//
// 🔴 反空真：每一格都是**相等断言**。三种落点语义各有正例，而且每一条正例旁边
//    都有一条「它不该给出这个答案」的反例 —— 一个「永远回 `end`」的实现
//    必须在这一组里红掉，否则这组就是空转。
// ==========================================================================
describe("步 17·D ⓪ 量具自检：落点函数真的在分三种，不是恒回一种", () => {
  // 三个 tab，每个高 28（`§D.4` 里那个实测值），首个 top=0。
  const rects: TabRect[] = [
    { sid: "a", top: 0, height: 28 },
    { sid: "b", top: 28, height: 28 },
    { sid: "c", top: 56, height: 28 },
  ];
  const none = new Set<string>();

  it("🔴 三种语义**都出得来**，而且各自出在该出的地方", () => {
    // 一格里三条分开断言：合成一条的话，只要有一种出不来，另两种也能让它看起来对。
    expect(pickDropTarget(rects, 5, none, null), "指针在 a 的上半 ⇒ 插到 a 之前").toEqual({
      kind: "before",
      sid: "a",
    });
    expect(pickDropTarget(rects, 200, none, null), "指针在所有 tab 下面 ⇒ 末尾").toEqual({
      kind: "end",
    });
    expect(pickDropTarget(rects, 40, none, "b"), "停留攒满了 ⇒ 与 b 成组").toEqual({
      kind: "onto",
      sid: "b",
    });
  });

  it("中线是分界：过了中线就轮到下一个（差一错在这里露）", () => {
    expect(pickDropTarget(rects, 13, none, null)).toEqual({ kind: "before", sid: "a" });
    expect(pickDropTarget(rects, 14, none, null), "恰好在 a 的中线上 ⇒ 已经不是 a 了").toEqual({
      kind: "before",
      sid: "b",
    });
  });

  it("🔴 **按视觉序扫，不按 `orderedIds` 扫**（`§D.2` 拆掉组过滤之后的必要条件）", () => {
    // 组容器整块排在散 tab 前面 ⇒ 数组次序与屏幕次序不再一致。
    // 喂一份「数组里 c 在最前、屏幕上 c 在最后」的读数：按数组扫会答 c。
    const shuffled: TabRect[] = [
      { sid: "c", top: 56, height: 28 },
      { sid: "a", top: 0, height: 28 },
      { sid: "b", top: 28, height: 28 },
    ];
    expect(
      pickDropTarget(shuffled, 5, none, null),
      "按数组次序扫的话这里会答 `c` —— 而 c 在屏幕最下面，指针在最上面",
    ).toEqual({ kind: "before", sid: "a" });
  });

  it("被拖的那一块整体不参与落点（落到自己身上不是一次重排）", () => {
    expect(pickDropTarget(rects, 5, new Set(["a"]), null), "a 被拖着，落点该轮到 b").toEqual({
      kind: "before",
      sid: "b",
    });
    expect(
      pickDropTarget(rects, 40, new Set(["b"]), "b"),
      "🔴 停留攒在自己身上也不许成组（不然拖一下自己就多一个组）",
    ).toEqual({ kind: "before", sid: "c" });
  });

  it("🔴 停留攒满了、但指针已经划出那个矩形 ⇒ **不给 `onto`**（两个来源都得同意）", () => {
    // 只信计时器的话，指针早已划走还会合并成组 —— 那正是 `§D.4` 要避的误触。
    expect(pickDropTarget(rects, 5, none, "c"), "计时器说 c，可指针在 a 头上").toEqual({
      kind: "before",
      sid: "a",
    });
  });

  it("`tabUnderY` 正反两控：压在矩形里才算，边界按左闭右开", () => {
    expect(tabUnderY(rects, 28, new Set()), "28 是 b 的上沿 ⇒ 算 b").toBe("b");
    expect(tabUnderY(rects, 27.9, new Set()), "27.9 还在 a 里").toBe("a");
    expect(tabUnderY(rects, 999, new Set()), "谁都没压着要能说「没有」").toBeNull();
    expect(tabUnderY(rects, 5, new Set(["a"])), "被拖的那块要排除").toBeNull();
  });

  it("两个常数就是 `§D.4` 写的那两个数（改了要有人知道）", () => {
    expect(DWELL_MS, "停留门槛不是 250ms ⇒ 与 Edge 的手感、与 `§D.4` 的推理都脱钩").toBe(250);
    expect(DWELL_MOVE_PX).toBe(4);
  });
});

describe("步 17·D ① 默认组名（`§D.6`）—— 这条路上不能弹 `window.prompt`", () => {
  it("① 共同前缀的目录名（最常见：同项目的两个会话）", () => {
    expect(commonDirName("/home/u/proj/a", "/home/u/proj/b")).toBe("proj");
    expect(commonDirName("/home/u/proj", "/home/u/proj"), "两条同 cwd ⇒ 就是它自己").toBe("proj");
    expect(commonDirName("C:\\work\\proj\\a", "C:/work/proj/b"), "两种分隔符都要认").toBe("proj");
  });

  it("算不出来要**说算不出来**（反面控：不许随便回一个）", () => {
    expect(commonDirName("/home/a", "/var/b"), "没有共同前缀").toBeNull();
    expect(commonDirName(null, "/var/b"), "一边没有 cwd").toBeNull();
    expect(commonDirName("C:\\a", "C:\\b"), "只共到盘符 ⇒ 「C:」当组名是噪声").toBeNull();
  });

  it("② 没有共同 cwd ⇒ `组 N`，取当前最大编号 +1", () => {
    expect(defaultGroupName(null, null, []), "一个组都没有 ⇒ 从 1 起").toBe("组 1");
    expect(defaultGroupName(null, null, ["组 1", "白天", "组 3"]), "最大编号是 3 ⇒ 下一个是 4").toBe(
      "组 4",
    );
    expect(defaultGroupName("/a/x", "/b/y", ["组 2"]), "共同前缀算不出来才轮到编号").toBe("组 3");
  });

  it("有共同目录名时**优先用它**（反面控：别退回编号）", () => {
    expect(defaultGroupName("/home/u/proj/a", "/home/u/proj/b", ["组 1"])).toBe("proj");
  });
});

describe("步 17·D ② 归属跟着落点宿主走（`§D.7` 的「拖出组」与「拖进组」是同一条规则）", () => {
  const cols = (list: TabCollection[]): TabCollection[] => JSON.parse(JSON.stringify(list));

  it("🆕 `onto` 到一个**没有组**的 tab ⇒ 现建一个组，两个都进去", () => {
    const next = applyDropToCollections([], ["a"], { kind: "onto", sid: "b" }, "proj", "gX");
    expect(next, "建组 + 两个成员，顺序 = 目标在前、被拖的在后").toEqual([
      { id: "gX", name: "proj", members: ["b", "a"] },
    ]);
  });

  it("`onto` 到一个**已经在组里**的 tab ⇒ 进那个组，不新建", () => {
    const base = cols([{ id: "g1", name: "白天", members: ["b"] }]);
    const next = applyDropToCollections(base, ["a"], { kind: "onto", sid: "b" }, "proj", "gX");
    expect(next).toEqual([{ id: "g1", name: "白天", members: ["b", "a"] }]);
  });

  it("整块一起走（交互 tab 连同它的 bg 子串）—— 不许把子树劈成两半", () => {
    const next = applyDropToCollections([], ["a", "a-bg"], { kind: "onto", sid: "b" }, "n", "gX");
    expect(next[0].members).toEqual(["b", "a", "a-bg"]);
  });

  it("🔴 `before` 一个**散 tab** ⇒ 从原来的组里**移出**（`§D.7` 的拖出组）", () => {
    const base = cols([{ id: "g1", name: "白天", members: ["a", "b"] }]);
    const next = applyDropToCollections(base, ["a"], { kind: "before", sid: "z" }, "n", "gX");
    expect(next, "落点宿主是散 tab 区 ⇒ a 不再属于 g1").toEqual([
      { id: "g1", name: "白天", members: ["b"] },
    ]);
  });

  it("`before` 一个**组里的 tab** ⇒ 进那个组（同一条规则的另一侧）", () => {
    const base = cols([{ id: "g1", name: "白天", members: ["b"] }]);
    const next = applyDropToCollections(base, ["a"], { kind: "before", sid: "b" }, "n", "gX");
    expect(next).toEqual([{ id: "g1", name: "白天", members: ["b", "a"] }]);
  });

  it("`end` ⇒ 移出（末尾就是散 tab 区）", () => {
    const base = cols([{ id: "g1", name: "白天", members: ["a"] }]);
    expect(applyDropToCollections(base, ["a"], { kind: "end" }, "n", "gX")).toEqual([
      { id: "g1", name: "白天", members: [] },
    ]);
  });

  it("🔴 反面控：什么都不该变的两种情形，**一个字节都不许变**", () => {
    const base = cols([{ id: "g1", name: "白天", members: ["a"] }]);
    expect(
      applyDropToCollections(base, ["a"], { kind: "onto", sid: "a" }, "n", "gX"),
      "压在自己身上不是一次合并",
    ).toEqual(base);
    const flat = cols([{ id: "g1", name: "白天", members: ["z"] }]);
    expect(
      applyDropToCollections(flat, ["a"], { kind: "before", sid: "b" }, "n", "gX"),
      "两个都是散 tab ⇒ 归属这一维没有任何事发生",
    ).toEqual(flat);
  });

  it("🔴 建组到上界（32 个）⇒ **原样返回**，不许把 tab 塞进一个不存在的集合", () => {
    const full = Array.from({ length: 32 }, (_, i) => ({
      id: `g${i}`,
      name: `组 ${i + 1}`,
      members: [] as string[],
    }));
    const next = applyDropToCollections(full, ["a"], { kind: "onto", sid: "b" }, "新", "gX");
    expect(next, "满了还建 ⇒ a 会挂在一个 `createCollection` 根本没造出来的 id 上").toEqual(full);
  });

  it("`collectionsEqual` 正反两控（它是「没变就不写盘」那道门）", () => {
    const a = cols([{ id: "g1", name: "n", members: ["x", "y"] }]);
    expect(collectionsEqual(a, cols(a)), "同一份判成不同 ⇒ 每拖一下都白写一次盘").toBe(true);
    expect(
      collectionsEqual(a, [{ id: "g1", name: "n", members: ["y", "x"] }]),
      "成员次序变了也是变了",
    ).toBe(false);
    expect(collectionsEqual(a, [{ id: "g1", name: "改了", members: ["x", "y"] }])).toBe(false);
    expect(collectionsEqual(a, []), "长度不同").toBe(false);
  });
});

// ==========================================================================
// 〔步 17·D〕手势接到真 DOM 上那一半：`§D.2` 的两个缺口是不是真的堵上了
// ==========================================================================
describe("步 17·D ③ 组里的 tab 真的参与落点（`§D.2` 缺口一）", () => {
  let tm: TabManager;
  let bar: HTMLElement;
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    bar = document.body.firstElementChild as HTMLElement;
  });

  it("🔴 量具自检 ＋ 正题：`tabRects()` 量到的集合 == 栏里所有 tab（**含组里的**）", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c1", "p", 0, LOCAL_ORIGIN);
    home(tm).prefs.collections = [
      { id: "g1", name: "白天", members: ["b"] },
    ];
    home(tm).prefs.collectionsLoaded = true;
    (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();

    // jsdom 的 `getBoundingClientRect` 恒回全 0 ⇒ 先给每个按钮钉一个真读数，
    // 否则下面量到的「高度」全是 0，落点判定看起来正常其实没在量任何东西。
    const stub = (sid: string, top: number): void => {
      const el = home(tm).bar.tabButtons.get(sid)!.root;
      el.getBoundingClientRect = (): DOMRect => ({ top, height: 28, bottom: top + 28 }) as DOMRect;
    };
    stub("b", 0); // 组容器排在前面
    stub("a", 28);

    const rects = home(tm).dragger.tabRects();
    expect(
      rects.map((r) => r.sid).sort(),
      "🔴 `parentElement !== barEl` 那道过滤还在 ⇒ 组里的 b 量不到 ⇒ 拖不进也拖不出组",
    ).toEqual(["a", "b"]);
    expect(rects.every((r) => r.height === 28), "量具自检：高度得是真读数，不是 jsdom 的 0").toBe(
      true,
    );
    // 组容器是 barEl 的子树 —— 这一句钉住「参与」的原因是 contains，不是别的巧合。
    expect(bar.contains(home(tm).bar.tabButtons.get("b")!.root)).toBe(true);
    expect(home(tm).bar.tabButtons.get("b")!.root.parentElement).not.toBe(bar);
  });
});

describe("步 17·D ④ 一次落点把顺序与归属**一起**落实（`applyDrop`）", () => {
  let tm: TabManager;
  const order = (): string[] => home(tm).store.orderedIds;
  const colsOf = (): TabCollection[] => home(tm).prefs.collections;
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    home(tm).prefs.collectionsLoaded = true;
  });

  it("`onto` ⇒ 建组 + 入组 + 落到目标之前，三件事一次做完", () => {
    tm.ensureTab("a", "/home/u/proj/x", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/home/u/proj/y", "p", 0, LOCAL_ORIGIN);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf().length, "没建组").toBe(1);
    expect(colsOf()[0].members, "成员不对").toEqual(["b", "a"]);
    expect(colsOf()[0].name, "默认名该走「共同前缀目录名」那一条（`§D.6` ①）").toBe("proj");
    expect(order(), "顺序也要落实：a 插到 b 之前").toEqual(["a", "b"]);
  });

  it("🔴 顺序没变、只有归属变了的那一拍**不许被吞掉**", () => {
    // 这是把 `applyReorder` 改成 `applyDrop` 的正题：旧实现 `if (顺序没变) return`，
    // 而「把组里的 tab 原地拖出来」恰好就是顺序不变、归属变。
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    home(tm).prefs.collections = [
      { id: "g1", name: "白天", members: ["a"] },
    ];
    home(tm).dragger.applyDrop("a", { kind: "before", sid: "b" }); // a 本来就在 b 前面 ⇒ 顺序不变
    expect(order(), "顺序确实没变（前提成立，这一格才有意义）").toEqual(["a", "b"]);
    expect(colsOf()[0].members, "🔴 归属那一半被「没变化就 return」吞了").toEqual([]);
  });

  it("`end` ⇒ 拖出组并落到末尾", () => {
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    home(tm).prefs.collections = [
      { id: "g1", name: "白天", members: ["a"] },
    ];
    home(tm).dragger.applyDrop("a", { kind: "end" });
    expect(order()).toEqual(["b", "a"]);
    expect(colsOf()[0].members).toEqual([]);
  });

  it("没 `loadCollections` 过的实例：归属一个字不动（顺序照常）", () => {
    // 与右键菜单那道门同一条理由：没读过盘就写，等于把用户已有的集合清空。
    home(tm).prefs.collectionsLoaded = false;
    tm.ensureTab("a", "/c1", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", 0, LOCAL_ORIGIN);
    home(tm).dragger.applyDrop("b", { kind: "onto", sid: "a" });
    expect(colsOf(), "没读过盘还敢建组 ⇒ 下一次落盘会把用户的集合全冲掉").toEqual([]);
    expect(order(), "顺序这一半照常").toEqual(["b", "a"]);
  });
});

// ==========================================================================
// 〔步 17·D ⑤〕停留计时器**真的接在拖拽上** —— 一次完整的假手势
//
// 🔴 这一格买的是上面那些纯函数买不到的东西：`§D.4` 的整个推理建立在
//    「**指针停住之后 `mousemove` 就不再来了**」这件事上 ⇒ 必须有计时器。
//    一个只在 `mousemove` 里数时间的实现，纯函数那几格**全绿**，而手势根本不存在。
// ==========================================================================
describe("步 17·D ⑤ 停留 250ms 才成组（假手势打真事件链）", () => {
  let tm: TabManager;
  const rootOf = (sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root;
  /** jsdom 的 `getBoundingClientRect` 恒回全 0 ⇒ 不钉读数的话「压在谁身上」永远答第一个。 */
  const stubRect = (sid: string, top: number): void => {
    rootOf(sid).getBoundingClientRect = (): DOMRect =>
      ({ top, height: 28, bottom: top + 28 }) as DOMRect;
  };
  const move = (y: number): void => {
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 0, clientY: y, bubbles: true }),
    );
  };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.useFakeTimers();
    tm = makeTM();
    home(tm).prefs.collectionsLoaded = true;
    tm.ensureTab("a", "/home/u/proj/x", "p", 0, LOCAL_ORIGIN);
    tm.ensureTab("b", "/home/u/proj/y", "p", 0, LOCAL_ORIGIN);
    (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
    stubRect("a", 0);
    stubRect("b", 28);
    // 起拖：先按下，再越过 6px 阈值。
    rootOf("a").dispatchEvent(
      new MouseEvent("mousedown", { button: 0, buttons: 1, clientX: 0, clientY: 5, bubbles: true }),
    );
  });
  afterEach(() => {
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    vi.useRealTimers();
    document.body.querySelectorAll(".tab-drag-ghost").forEach((n) => n.remove());
  });

  it("🔴 不到 250ms 是 `before`，满了才翻成 `onto`（时间这一维真的在承重）", () => {
    move(40); // 落在 b 的上半 ⇒ before b
    expect(rootOf("b").classList.contains("drop-before"), "落点指示没出来 ⇒ 这一格在空转").toBe(
      true,
    );
    expect(rootOf("b").classList.contains("drop-onto"), "还没压够就成组 = `§D.4` 要避的误触").toBe(
      false,
    );

    vi.advanceTimersByTime(DWELL_MS - 1);
    expect(rootOf("b").classList.contains("drop-onto"), "差 1ms 就翻 ⇒ 门槛是假的").toBe(false);

    vi.advanceTimersByTime(1);
    expect(
      rootOf("b").classList.contains("drop-onto"),
      "🔴 压满 250ms 还不成组 —— 指针停住之后没有 `mousemove` 了，没有计时器这件事就永远不发生",
    ).toBe(true);
    expect(rootOf("b").classList.contains("drop-before"), "两个标不许同时挂着").toBe(false);
  });

  it("攒满之后抖一下（>4px）⇒ 退回 `before`（可逆）", () => {
    move(40);
    vi.advanceTimersByTime(DWELL_MS);
    expect(rootOf("b").classList.contains("drop-onto")).toBe(true);
    move(40 + DWELL_MOVE_PX + 1);
    expect(rootOf("b").classList.contains("drop-onto"), "抖过 4px 还锁在 `onto` 上 ⇒ 不可逆").toBe(
      false,
    );
  });

  it("松手在 `onto` 态 ⇒ 真的建出组来（手势整条链走通，不是只改了个 class）", () => {
    move(40);
    vi.advanceTimersByTime(DWELL_MS);
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    const cols = home(tm).prefs.collections;
    expect(cols.length, "松手了却没建组 ⇒ 视觉反馈与真实后果脱钩（比没做更坏）").toBe(1);
    expect(cols[0].members).toEqual(["b", "a"]);
    expect(cols[0].name, "默认名走 `§D.6` ①：两个 cwd 的共同前缀目录名").toBe("proj");
  });

  it("🔴 松手早于 250ms ⇒ 计时器必须**已经被清掉**（数在飞的定时器，不是只看后果）", () => {
    // ⚠ 这一格最初写成「松手后再快进，`.drop-onto` 不许出现」—— 死值验刀 21 实测**杀不掉**：
    //   计时器回调里那道 `if (!cur) return` 已经把后果挡住了，于是「清没清」看不出来。
    //   ⇒ 改成直接数在飞的定时器：那才是「悬空引线」这条性质本身。
    const base = vi.getTimerCount();
    move(40);
    expect(vi.getTimerCount(), "停留计时器根本没排上 ⇒ 这一格在空转").toBe(base + 1);

    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    expect(
      vi.getTimerCount(),
      "🔴 拖拽收尾了，停留计时器还挂在那儿 —— 一条悬空引线（同 `pendingMenuTimers` 那条教训）",
    ).toBe(base);

    vi.advanceTimersByTime(DWELL_MS * 4);
    expect(rootOf("b").classList.contains("drop-onto"), "更不该事后翻成组").toBe(false);
    expect(home(tm).prefs.collections, "更不该无中生有地建组").toEqual([]);
  });
});

// ==========================================================================
// 步 17·C 顺序落盘 —— **「读回来」那一半**（`设计/30 §C` · `99 §4` 步 17 那行的 🟡）
//
// 落盘那一侧（`tab-bar-state.ts` 的 `sanitizeOrder` / `getTabOrder` / `setTabOrder`）
// 已经有 `tests/tab-bar-state.vitest.ts` 10 格盯着，**那一段是好的**。
// 这一组只量一件事：**`TabManager.loadOrder` 有没有真的把盘上那份顺序装回栏里。**
//
// 🔴 **为什么必须照「真实启动时序」摆**（这一组全部判据的承重）：
//   `src/main.ts` 里那一行是 `loadPinned().finally(() => loadOrder())` ——
//   `loadOrder` 跑在**会话还没到**的那一刻，tab 是随后由 `session_added` / 首行**陆续**
//   建出来的。⇒ 把 tab 先造好再调 `loadOrder` 的判据**买不到这条性质**：
//   它在那种摆法下是绿的，而在真实时序下整张顺序会被摘掉。
//   ⚠ 这也正是 `tab-bar-state.vitest.ts` 那条「往返」格量不到的面 ——
//     它 `setTabOrder(["x","y"])` 之后拿 `getTabOrder(new Set(["x","y"]))` 读，
//     **`alive` 两侧同源 ⇒ 恒真**（`01 §7.4` 点名的那一形）。
//
// 🔴 反空真：这一组的 config 是**一份真的在内存里的盘**（走已被 mock 的 `invoke`,
//   `load_config`/`save_config`），所以「顺序落没落上」是**读盘对拍**，不是数调用次数。
//   全部断言是**逐位相等**（`toEqual` 整张数组），没有「至少有几个 tab」那种地板。
// ==========================================================================
describe("步 17·C 顺序落盘：读回来那一半", () => {
  let tm: TabManager;
  let disk: Record<string, unknown>;
  /** 落盘是 fire-and-forget（`先改内存再落盘`）⇒ 读盘前把那条链子跑完。 */
  const flushDisk = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
  const orderOnDisk = (): unknown => (disk.tabBar as Record<string, unknown> | undefined)?.order;

  /** 往「盘」上摆一份顺序（模拟上次关 app 前落下的那份）。 */
  const putOrderOnDisk = (order: string[]): void => {
    disk = { tabBar: { order } };
  };

  /**
   * **真实启动时序**（逐行对着 `src/main.ts` 那三句摆）：
   *   `new TabManager` → `loadPinned()` → `loadOrder()` → 会话**随后**陆续到。
   * @param arriving 会话到达的次序（＝后端清单/事件流给的次序，通常不等于用户拖出来的次序）
   */
  const startupThenSessionsArrive = async (arriving: string[]): Promise<void> => {
    await tm.loadPinned();
    await tm.loadOrder();
    for (const sid of arriving) {
      // 每个 sid 一个自己的 cwd：避开 `placeInOrder` 的树状锚定（bg 挂宿主后面），
      // 这一组量的是「盘上那份顺序」，不是那棵树。
      tm.ensureTab(sid, `/proj/${sid}`, `/p/${sid}.jsonl`, 0, LOCAL_ORIGIN, "interactive", null);
    }
  };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "save_config") {
        disk = JSON.parse(JSON.stringify((args as { value: unknown }).value));
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
    tm = makeTM();
  });

  // --- ① 量具自检：先证明这一组的「盘」和 `loadOrder` 这条路真的在动 ---------
  it("🔴 量具自检：tab **先在**、再调 `loadOrder` ⇒ 顺序真的会被重排（不过这格，下面全是空转）", async () => {
    // 这一格**刻意**用「先造 tab 再读盘」那种摆法 —— 它证明的是
    // 「读盘 + 重排 + 重画」这套机件本身是通的，从而把下面那格的红**钉在时序上**，
    // 而不是钉在「读不出来」或「键不对」上。
    tm.ensureTab("a", "/proj/a", "/p/a.jsonl", 0, LOCAL_ORIGIN, "interactive", null);
    tm.ensureTab("b", "/proj/b", "/p/b.jsonl", 0, LOCAL_ORIGIN, "interactive", null);
    tm.ensureTab("c", "/proj/c", "/p/c.jsonl", 0, LOCAL_ORIGIN, "interactive", null);
    expect(home(tm).store.orderedIds, "到达序就是建出来的序").toEqual(["a", "b", "c"]);

    putOrderOnDisk(["c", "b", "a"]);
    await tm.loadOrder();
    expect(
      home(tm).store.orderedIds,
      "🔴 连「tab 已经在了」这种最宽松的摆法都排不回来 ⇒ 成因不是时序，是读/用那一段本身坏了",
    ).toEqual(["c", "b", "a"]);
  });

  // --- ② 正题：真实启动时序下的「存 → 读回来」--------------------------------
  it("🔴 顺序跨重启（`设计/30 §6` 验证钩子 9）：盘上一份非默认顺序 ⇒ 会话到齐后**逐位**对上", async () => {
    // 用户上次把顺序拖成了 c,b,a（非默认 —— 后端清单给的是 a,b,c）。
    putOrderOnDisk(["c", "b", "a"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(
      home(tm).store.orderedIds,
      "🔴 盘上的顺序没装回来 —— 用户拖的顺序活不过一次重启（`§C` 逐字「今天拖了白拖」）",
    ).toEqual(["c", "b", "a"]);
  });

  // --- ③ 阴性对照：证明这条判据**不是**在「两侧同源」上恒真 --------------------
  it("🔴 阴性对照 a：读回来的那张**不许**等于到达序（否则上一格只是在量「会话是怎么到的」）", async () => {
    putOrderOnDisk(["c", "b", "a"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(
      home(tm).store.orderedIds,
      "🔴 读回来的顺序 == 到达序 ⇒ 上一格是恒真的（盘根本没参与）",
    ).not.toEqual(["a", "b", "c"]);
  });

  it("🔴 阴性对照 b：**只**换盘上那份、到达序一个字不动 ⇒ 结果必须跟着盘变（盘是唯一自变量）", async () => {
    putOrderOnDisk(["b", "a", "c"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(home(tm).store.orderedIds, "第一份盘没生效").toEqual(["b", "a", "c"]);

    // 同一套到达序，换一份盘 ⇒ 必须得到**另一张**顺序。
    tm = makeTM();
    putOrderOnDisk(["c", "a", "b"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(
      home(tm).store.orderedIds,
      "🔴 换了盘上那份，结果没跟着变 ⇒ 这条判据量的不是盘，是别的东西",
    ).toEqual(["c", "a", "b"]);
  });

  it("🔴 阴性对照 c：盘上**故意存错**（存成一份用户从没拖过的序）⇒ 正题那张断言必须红", async () => {
    // 这一格把「存盘那一侧存错」显式演一遍：盘上是 a,c,b，而正题期望的是 c,b,a。
    // 它绿 == 读回来的东西**真的跟着盘走**；它一旦连这个都绿不了，正题那格就是空转。
    putOrderOnDisk(["a", "c", "b"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    const got = home(tm).store.orderedIds;
    expect(got, "存错的那份也要如实装回来（读回来那一半不许自己纠正盘上的内容）").toEqual([
      "a",
      "c",
      "b",
    ]);
    expect(got, "🔴 存错的盘却读出了正题那张 ⇒ 结果与盘无关，判据恒真").not.toEqual([
      "c",
      "b",
      "a",
    ]);
  });

  // --- ④ 边界：盘上有一个今天已经不存在的 sid --------------------------------
  it("🔴 边界：盘上的顺序里有一个今天**不存在**的 sid ⇒ 不凭空造 tab，其余**照原样**排好", async () => {
    // `§C.3` 逐字：「顺序里会有已经不存在的 sid（上次固定的会话被删了）⇒ 读的时候要过滤」。
    // 明确行为 = **丢掉它**（不进 `orderedIds`、不造 tab），并且它的存在
    // **不许**打乱其余几个的相对次序。
    putOrderOnDisk(["c", "上次那条被删了", "b", "a"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(
      home(tm).store.orderedIds,
      "🔴 死 sid 把整张顺序带坏了（或者它自己混进了 orderedIds）",
    ).toEqual(["c", "b", "a"]);
    expect(
      home(tm).store.tabs.has("上次那条被删了"),
      "🔴 凭空造出了一个 tab —— `loadOrder` 逐字「只重排已存在的 tab，不凭空造 tab」",
    ).toBe(false);
  });

  it("盘上**没提到**的 tab 排在后面（`loadOrder` 头注逐字），不许被旧顺序挤没", async () => {
    putOrderOnDisk(["c", "a"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(home(tm).store.orderedIds, "🔴 新 tab 没排到后面 / 被旧顺序挤掉了").toEqual(["c", "a", "b"]);
  });

  it("盘上**没有** `tabBar.order`（第一次用）⇒ 到达序原样留着，不许被清空", async () => {
    disk = {};
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(home(tm).store.orderedIds, "🔴 空盘把到达序弄坏了").toEqual(["a", "b", "c"]);
  });

  // --- ⑤ 别把用户刚拖的那一下又按回旧顺序 ------------------------------------
  it("🔴 启动后用户又拖了一下 ⇒ 随后到的 tab **不许**把盘上那份旧顺序重新按上来", async () => {
    // 这一格盯的是「记住盘上那份」这个改法自带的风险：顺序一旦被用户改过，
    // 那份旧的就**过期**了，再拿它去排后到的 tab 等于把用户刚拖的一下撤销。
    putOrderOnDisk(["c", "b", "a"]);
    await startupThenSessionsArrive(["a", "b", "c"]);
    expect(home(tm).store.orderedIds).toEqual(["c", "b", "a"]);

    // 用户把 a 拖到最前面。
    home(tm).dragger.applyDrop("a", { kind: "before", sid: "c" });
    expect(home(tm).store.orderedIds, "拖动本身没生效，这一格后面就没意义了").toEqual(["a", "c", "b"]);
    await flushDisk();
    expect(orderOnDisk(), "拖完要落盘（`§C.3`：applyReorder 之后提交）").toEqual(["a", "c", "b"]);

    // 又来一条新会话。
    tm.ensureTab("d", "/proj/d", "/p/d.jsonl", 0, LOCAL_ORIGIN, "interactive", null);
    expect(
      home(tm).store.orderedIds,
      "🔴 后到的 tab 把用户刚拖的顺序撤销了（拿一份过期的盘去排）",
    ).toEqual(["a", "c", "b", "d"]);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 〔`设计/10` 骨架 · 子步 4〕接入判据：索引 → 占位 → 门控 → 跳转。
// 本文件把 `MessageStream` / `RecordTimeline` mock 掉了 ⇒ 这里只判**接线**（要没要索引、
// 接没接上、哪些行收纳哪些建卡）；「只物化可见区」本身的几何判据住 `tests/skeleton-view.vitest.ts`
// （那边是真 stream ＋ 真 timeline）。
// ═══════════════════════════════════════════════════════════════════════
describe("〔设计/10〕骨架接入：索引 → 占位 → 门控 → 跳转", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  const mk = (sid: string, seq: number, uuid: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      message: { type: "assistant", uuid } as never,
    }) as never;
  /** 索引：第 i 行 uuid = `u{i + shift}`（shift ≠ 0 模拟「seq 空间对不上」） */
  const idx = (n: number, shift = 0) => ({
    available: true,
    from: 0,
    end: n * 10,
    rows: Array.from({ length: n }, (_, i) => ({
      o: i * 10,
      n: 10,
      t: "assistant",
      u: `u${i + shift}`,
      ch: 100,
      pl: 1,
    })),
  });
  // 〔C4b〕骨架索引改走通道：一发 `chan_call` 译回「哪一问 ＋ 旧形参」（`chan-fake.ts::sessionReadCalls`）。
  const indexCalls = () =>
    sessionReadCalls(vi.mocked(invoke).mock.calls, "read_session_index").map((a) => ["read_session_index", a]);
  const settle = () => new Promise((r) => setTimeout(r, 0));

  /** 重放形状：尾巴先到（钉 floor=200），老的 [0,200) 后到 ⇒ 全收纳。`skip` 里的 seq 不发（模拟迟到）。 */
  function replay(sid: string, skip: Set<number> = new Set()): Tab {
    tm.onLine(mk(sid, 200, "u200")); // 首个 tab ⇒ active，非批期直渲钉 floor
    // jsdom 没布局 ⇒ `contentReachesBottom` 退回算术判据；给它一个「已满屏」的几何，
    // 否则批结束会把 [0,200) 全物化掉（那是没有骨架时的正确行为，不是本组要测的）
    const el = home(tm).store.tabs.get(sid)!.streamEl;
    Object.defineProperty(el, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(el, "clientHeight", { value: 800, configurable: true });
    tm.onBatchStart();
    for (let s = 201; s < 300; s++) tm.onLine(mk(sid, s, `u${s}`));
    for (let s = 0; s < 200; s++) if (!skip.has(s)) tm.onLine(mk(sid, s, `u${s}`));
    tm.onBatchEnd();
    return home(tm).store.tabs.get(sid)!;
  }

  it("批结束：active tab 要一次索引（本机 origin 逐字 `<local>`、从 0 起），到了就接骨架、哨兵退场", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never);
    const t = replay("sk");
    expect(indexCalls()).toEqual([
      ["read_session_index", { origin: "<local>", jsonlPath: "/p/sk.jsonl", fromOffset: 0 }],
    ]);
    await settle();
    expect(t.skeleton, "索引到了却没接上").not.toBeNull();
    expect(t.skeleton!.pendingRows).toBe(200);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();
    const snap = JSON.parse(tm.debugSnapshot()) as { skeleton: { rows: number; pendingRows: number } };
    expect(snap.skeleton).toMatchObject({ rows: 300, pendingRows: 200 });
    // 只要一次：再结束一批、切走再切回都不重拉
    tm.onBatchStart();
    tm.onBatchEnd();
    expect(indexCalls().length).toBe(1);
  });

  it("老后端 / 本机后端不在（available:false）⇒ 不接，尾部窗口照旧（账本还在、哨兵还在）", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(
        cmd === "read_session_index"
          ? { available: false, reason: "老后端", from: 0, end: 0, rows: [] }
          : undefined,
      ),
    ) as never);
    const t = replay("old");
    await settle();
    expect(t.skeleton).toBeNull();
    expect(t.skeletonFetch).toBe("done");
    expect(t.window.pendingCount).toBe(200);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).not.toBeNull();
  });

  it("🔴 seq 空间对不上（uuid 在索引里落在别的 seq）⇒ 不接 —— 不许硬对", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300, 7) : undefined),
    ) as never);
    const t = replay("shift");
    await settle();
    expect(t.skeleton).toBeNull();
    expect(t.window.pendingCount).toBe(200);
  });

  it("门控：占位里的行收纳；岛（ensure 物化过的一段）里迟到的行就地建卡，不收纳", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never);
    const t = replay("gate", new Set([60, 150]));
    await settle();
    const sk = t.skeleton!;
    sk.ensure(60, 5); // [55, 66) 物化（60 还没到）
    expect(sk.isPending(60)).toBe(false);
    const before = t.window.pendingCount;
    const { renderContentRecord } = await import("../src/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    tm.onLine(mk("gate", 60, "u60")); // 迟到、落在岛里 ⇒ 建卡
    expect(spy.mock.calls.map((c) => (c[0] as { seq: number }).seq)).toEqual([60]);
    tm.onLine(mk("gate", 150, "u150")); // 迟到、落在占位里 ⇒ 收纳
    expect(spy.mock.calls.length).toBe(1);
    expect(t.window.pendingCount).toBe(before + 1);
  });

  it("子步 5：物化到一段**还没到过**的行 ⇒ 按索引的字节边界要回来（从偏移读），走 onLine 全套建卡", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: unknown) => {
      if (cmd === "read_session_index") return Promise.resolve(idx(300));
      if (cmd === "read_session_range") {
        const a = args as { seqBase: number; lineCount: number };
        return Promise.resolve(
          Array.from({ length: a.lineCount }, (_, k) => mk("miss", a.seqBase + k, `u${a.seqBase + k}`)),
        );
      }
      return Promise.resolve(undefined);
    }) as never);
    const t = replay("miss", new Set([100, 101, 102, 150]));
    await settle();
    const { renderContentRecord } = await import("../src/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    t.skeleton!.ensure(101, 3); // [98, 105)：98/99/103/104 在账本里，100–102 没到过
    const ranges = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "read_session_range");
    // 连续缺的三行并成**一段**、一次 IPC；字节边界取自索引（o = seq×10，n = 10）
    expect(ranges).toEqual([
      [
        "read_session_range",
        { origin: "<local>", jsonlPath: "/p/miss.jsonl", offset: 1000, until: 1030, seqBase: 100, lineCount: 3 },
      ],
    ]);
    await settle();
    const rendered = spy.mock.calls.map((c) => (c[0] as { seq: number }).seq).sort((x, y) => x - y);
    expect(rendered).toEqual([98, 99, 100, 101, 102, 103, 104]);
    // 🔴 取回的是**历史**：按重放语义建卡 —— sink 不接 onRealUserInput（历史 user 卡不许自动切 tab），
    //    轮次结束检测按批期短路（不许为历史弹系统通知）
    for (const c of spy.mock.calls.filter((c) => (c[0] as { seq: number }).seq >= 100 && (c[0] as { seq: number }).seq <= 102)) {
      expect((c[2] as { onRealUserInput?: unknown }).onRealUserInput).toBeUndefined();
    }
    const { turnEndNotifier } = await import("../src/turn-notify");
    const obs = vi.mocked(turnEndNotifier.observe).mock.calls.filter(
      (c) => (c[2] as { seq: number }).seq >= 100 && (c[2] as { seq: number }).seq <= 102,
    );
    expect(obs.length).toBe(3);
    expect(obs.every((c) => c[3] === true), "历史行按 live 喂了 ⇒ 会为旧轮次弹通知").toBe(true);
    // 150 不在这一段里 ⇒ 没被要
    expect(ranges.some((c) => (c[1] as { seqBase: number }).seqBase === 150)).toBe(false);
  });

  it("滚动：接上骨架后**每次**滚动都交给 fillVisible —— 不再只在离顶 800px 内才补（占位可以在中部）", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never);
    const t = replay("scroll");
    await settle();
    const spy = vi.spyOn(t.skeleton!, "fillVisible");
    Object.defineProperty(t.streamEl, "scrollTop", { value: 5000, configurable: true });
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("〔U3b · 步 8〕接上骨架 ⇒ 前端账本只留离尾巴最近的 200 条（〔CF2〕monitor 那一半不再登记）；丢掉的滚到时按偏移要回来、只建卡不重记账", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: unknown) => {
      if (cmd === "read_session_index") return Promise.resolve(idx(1100));
      if (cmd === "read_session_range") {
        const a = args as { seqBase: number; lineCount: number };
        return Promise.resolve(
          Array.from({ length: a.lineCount }, (_, k) => mk("big", a.seqBase + k, `u${a.seqBase + k}`)),
        );
      }
      return Promise.resolve(undefined);
    }) as never);
    // 尾巴 [1000,1100) 先到（钉 floor=1000），[0,1000) 后到 ⇒ 全收纳
    tm.onLine(mk("big", 1000, "u1000"));
    const el = home(tm).store.tabs.get("big")!.streamEl;
    Object.defineProperty(el, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(el, "clientHeight", { value: 800, configurable: true });
    tm.onBatchStart();
    for (let s = 1001; s < 1100; s++) tm.onLine(mk("big", s, `u${s}`));
    for (let s = 0; s < 1000; s++) tm.onLine(mk("big", s, `u${s}`));
    tm.onBatchEnd();
    const t = home(tm).store.tabs.get("big")!;
    expect(t.window.pendingCount).toBe(1000);
    await settle();
    expect(t.skeleton).not.toBeNull();
    // ① 前端账本：只留 seq 最高的 200 条（[800,1000)）
    expect(t.window.pendingCount).toBe(200);
    expect(t.window.peek(1)[0].seq).toBe(800);
    // ② 〔CF2〕monitor 重放缓冲不再要前端登记（每个会话都只留尾巴）⇒ 零次
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === "replay_keep_tail_only")).toEqual([]);
    // ③ 滚到被丢掉的那段：按偏移要回来；这些行**见过**（旁路账早记过）⇒ 只建卡，不再走 onLine
    const { renderContentRecord } = await import("../src/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    const { turnEndNotifier } = await import("../src/turn-notify");
    spy.mockClear();
    vi.mocked(turnEndNotifier.observe).mockClear();
    t.skeleton!.ensure(300, 2); // [298, 303)
    await settle();
    expect(
      vi.mocked(invoke).mock.calls.filter((c) => c[0] === "read_session_range").map((c) => c[1]),
    ).toEqual([
      { origin: "<local>", jsonlPath: "/p/big.jsonl", offset: 2980, until: 3030, seqBase: 298, lineCount: 5 },
    ]);
    expect(spy.mock.calls.map((c) => (c[0] as { seq: number }).seq)).toEqual([298, 299, 300, 301, 302]);
    expect(vi.mocked(turnEndNotifier.observe), "见过的行又走了一遍 onLine 旁路").not.toHaveBeenCalled();
  });

  it("大纲跳转：点到还在占位里的一条 ⇒ 先按 uuid→seq 物化那一段再跳", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never);
    const t = replay("jump");
    await settle();
    expect(t.skeleton!.isPending(42)).toBe(true);
    const host = (t.inputsPanel as unknown as { host: { jumpTo(u: string): unknown } }).host;
    // jsdom 没有 `CSS.escape`（`revealCard` 要它）；本格判的是「跳之前先物化」，找不找得到卡不在本格
    const g = globalThis as unknown as { CSS?: { escape(s: string): string } };
    g.CSS ??= { escape: (x: string) => x };
    host.jumpTo("u42");
    expect(t.skeleton!.isPending(42)).toBe(false);
  });

  // 〔SE1〕数据源换成后端之后，这条路必须照旧通：行是后端清单给的（不是流上攒的），
  //   点**那一行**（不是直接调宿主）⇒ 先按 uuid→seq 物化再跳。
  it("🔴 SE1：大纲的行来自后端清单，点到还在占位里的那一行 ⇒ 先物化那一段再跳", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      Promise.resolve(
        cmd === "read_session_index"
          ? idx(300)
          : cmd === "list_user_inputs"
            ? {
                available: true,
                from: 0,
                end: 3,
                entries: [
                  { uuid: "u7", excerpt: "七", timestamp: "" },
                  { uuid: "u42", excerpt: "四十二", timestamp: "" },
                  { uuid: "u250", excerpt: "二百五十", timestamp: "" },
                ],
              }
            : undefined,
      ),
    ) as never);
    const t = replay("jump2");
    await settle();
    await settle();
    const rows = [...t.inputsEl.querySelectorAll<HTMLButtonElement>(".user-input-row")];
    // 行 == 后端给的，顺序不动（流上喂的全是 assistant 行 ⇒ 前端若还在自己攒，这里一条都不会有）
    expect(rows.map((r) => r.dataset.inputUuid)).toEqual(["u7", "u42", "u250"]);
    expect(t.skeleton!.isPending(42), "夹具没落在占位里 ⇒ 下面那半是空真").toBe(true);
    const g = globalThis as unknown as { CSS?: { escape(s: string): string } };
    g.CSS ??= { escape: (x: string) => x };
    rows[1].click();
    expect(t.skeleton!.isPending(42)).toBe(false);
    expect(t.skeleton!.isPending(7), "只物化点到的那一段，不是全建").toBe(true);
  });
});

// ==========================================================================
// 〔U4b · 第四波 · T3 / T4〕三格后端事实在 `TabManager` 上真走一遍（`调研/第四波记录/U4b.md §2`）。
// ==========================================================================
describe("〔U4b〕容器 · 说不清 · 记录没了 —— TabManager 真走", () => {
  let tm: TabManager;
  let disk: Record<string, unknown>;
  /** `probe_session_record` 的桩答案：`undefined` = 抛错（问不到）。 */
  let probe: { present: boolean; root: string } | undefined;
  const tabOf = (sid: string): Tab => home(tm).store.tabs.get(sid)!;
  const btn = (): HTMLElement => document.querySelector<HTMLElement>(".tab")!;

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    probe = { present: true, root: "/h/.claude/projects" };
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "save_config") {
        disk = JSON.parse(JSON.stringify((args as { value: unknown }).value));
        return Promise.resolve(undefined);
      }
      if (cmd === "probe_session_record")
        return probe ? Promise.resolve(probe) : Promise.reject(new Error("没有控制通道"));
      return Promise.resolve(undefined);
    });
    tm = makeTM();
  });

  it("★ G3：容器事实落在活会话上（tooltip 第一行说它）；早到的暂存、建 tab 时落实；死了之后来的不改死的那一格", () => {
    tm.noteContainer("c1", "tmux"); // 早于建 tab
    const t = tm.ensureTab("c1", "/x", "p", 0, "pi");
    expect(t.state).toEqual(LIVE_ATTACHABLE);
    expect(btn().title).toBe("在 tmux 会话里运行：程序退了也能接回去");
    tm.noteContainer("c1", "none");
    expect(t.state).toEqual(LIVE_RESUMABLE);
    expect(btn().title).toBe("不在 tmux 会话里：程序退了只能 resume");
    tm.noteContainer("c1", "screen"); // 不认识的取值 ⇒ 当没报，不动
    expect(t.state).toEqual(LIVE_RESUMABLE);
    tm.archiveTab("c1");
    tm.noteContainer("c1", "tmux"); // 晚到：死了的那一格由死的那一刻说了算
    expect(t.state).toEqual(ENDED);
  });

  it("★ G3 行为不回退：活着、只是不在 tmux 里的会话 ≠ 已结束（× 不露、↗ 还在）", () => {
    tm.ensureTab("c2", "/x", "p", 0, "pi");
    tm.noteContainer("c2", "none");
    expect(btn().classList.contains("ended")).toBe(false);
    expect(btn().classList.contains("reconnectable")).toBe(false);
  });

  it("★ 说不清：固定复活、那台还没报完 ⇒ 说不清（字里零处「已结束」）；报完了没有它 ⇒ 已结束；报完了有它 ⇒ 活", async () => {
    disk = {
      tabBar: {
        pinned: [
          { sid: "p1", origin: "pi", title: "T1", jsonlPath: "/p/p1.jsonl" },
          { sid: "p2", origin: LOCAL_ORIGIN, title: "T2", jsonlPath: "/p/p2.jsonl" },
          { sid: "p3", origin: LOCAL_ORIGIN, title: "T3", jsonlPath: "/p/p3.jsonl" },
        ],
      },
    };
    await tm.loadPinned();
    for (const sid of ["p1", "p2", "p3"]) expect(tabOf(sid).state).toEqual(UNSEEN);
    // T4：说不清那一刻，tab 的提示句 · 状态名 · 固定空态，一处都不许说「已结束」。
    const said = (): string =>
      [...document.querySelectorAll<HTMLElement>(".tab")].map((b) => b.title).join("\n") +
      [...document.querySelectorAll<HTMLElement>(".pin-revived-hint")].map((n) => n.textContent).join("\n");
    expect(said()).toContain("说不清");
    expect(said()).not.toContain("已结束");
    // 本机清单报完：p3 在清单里 ⇒ 活；p2 不在 ⇒ 已结束；远端那条不受本机清单影响。
    tm.markOriginSeen(LOCAL_ORIGIN, new Set(["p3"]));
    expect([tabOf("p1").state, tabOf("p2").state, tabOf("p3").state]).toEqual([UNSEEN, ENDED, LIVE]);
    // 远端报完（`origin-sessions-listed`）：没被宣告过 ⇒ 已结束。正控：这时才出现「已结束」。
    tm.markOriginSeen("pi");
    expect(tabOf("p1").state).toEqual(ENDED);
    expect(said()).toContain("已结束");
  });

  it("★ 说不清：那台先报完、固定后复活 ⇒ 直接已结束（`seenOrigins` 记住了）；远端又宣告它 ⇒ 活", async () => {
    tm.markOriginSeen("pi");
    disk = { tabBar: { pinned: [{ sid: "q1", origin: "pi", title: "Q", jsonlPath: "/p/q1.jsonl" }] } };
    await tm.loadPinned();
    expect(tabOf("q1").state).toEqual(ENDED);
    tm.createSkeletonTab("q1", "/x", "pi");
    expect(tabOf("q1").state).toEqual(LIVE);
  });

  it("★ G1：resume 查到记录不在 ⇒ 不开终端 ＋ 诚实报错（说清哪台、哪棵树）＋ 落「记录已不在」；再查到在 ⇒ 翻回已结束、照常 resume", async () => {
    tm.ensureTab("g1", "/home/u/p", "/p/g1.jsonl", 0, LOCAL_ORIGIN);
    tm.archiveTab("g1");
    probe = { present: false, root: "/h/.claude/projects" };
    await home(tm).actions.resumeTab("g1");
    expect(invoke).not.toHaveBeenCalledWith("resume_history_session", expect.anything());
    expect(showActionFailureToast).toHaveBeenCalledWith(
      "没法 resume：记录已不在",
      "本机 的 /h/.claude/projects 里找不到会话 g1 的记录，resume 接不上它，所以没有打开终端。",
    );
    expect(tabOf("g1").state).toEqual(GONE);
    expect(btn().title).toBe("这个会话已结束，它的记录也不在了，没法 resume");
    probe = { present: true, root: "/h/.claude/projects" };
    await home(tm).actions.resumeTab("g1");
    expect(tabOf("g1").state).toEqual(ENDED);
    expect(invoke).toHaveBeenCalledWith("resume_history_session", expect.objectContaining({ sessionId: "g1" }));
  });

  it("★ G1：远端两条路（直连 · tmux 全新）同样先问；问不到 ⇒ 当不知道、照今天的路走（不当「不在」）", async () => {
    tm.ensureTab("g2", "/home/pi/proj", "/p/g2.jsonl", 0, "aya");
    tm.archiveTab("g2");
    probe = { present: false, root: "/home/pi/.claude/projects" };
    await home(tm).actions.resumeTab("g2");
    await home(tm).actions.resumeTabTmux("g2");
    expect(runRemoteResume).not.toHaveBeenCalled();
    expect(runRemoteResumeTmux).not.toHaveBeenCalled();
    expect(tabOf("g2").state).toEqual(GONE);
    probe = undefined; // 问不到
    await home(tm).actions.resumeTab("g2");
    expect(runRemoteResume).toHaveBeenCalledTimes(1);
    expect(tabOf("g2").state, "问不到不改状态").toEqual(GONE);
  });

  it("★ G1：可重连的会话记录没了也不落「记录已不在」（终端还在，接得回去）", () => {
    tm.ensureTab("g3", "/x", "p", 0, "pi");
    tm.markTmuxIdle("g3");
    tm.markRecord("g3", false);
    expect(tabOf("g3").state).toEqual(RECONNECTABLE);
  });
});

// 〔U4b · 第四波〕**接线判据**：`main.ts` 起步那几行（`list_active_sessions` 之后标本机清单报完 ·
// 两个新事件交给 TabManager）没有 DOM 判据够得着（整个 `main.ts` 是入口脚本）⇒ 读源码数调用点，两向恰好一处。
describe("〔U4b〕main.ts 接线", () => {
  it("★ 本机清单报完 · 容器事件 · 远端清单报完，三处接线各恰一处", () => {
    const main = readFileSync(resolve(REPO_ROOT, "src/main.ts"), "utf8");
    const n = (needle: string): number => main.split(needle).length - 1;
    expect([
      n("tabs.markOriginSeen(LOCAL_ORIGIN, new Set(active.map((s) => s.session_id)))"),
      n("onSessionContainer: (sessionId, container) => tabs.noteContainer(sessionId, container)"),
      n("onOriginSessionsListed: (origin) => tabs.markOriginSeen(origin)"),
    ]).toEqual([1, 1, 1]);
  });
});

/**
 * 〔CF2 · 第四波 4B〕**没接骨架的 tab 按行号往下取**（`TabStreamView.fetchBelow` · `read_session_lines`）。
 *
 * 要求住址：`设计/99 §4.4`「无索引会话的重放缓冲上界（要先有不依赖索引的取回路）」· `设计/05 §3.3.4`
 * 「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界」—— monitor 的重放缓冲从此每个会话只留尾巴，
 * F5 之后更早的正文就只剩这一条路回来；它不成立，上界就是「丢了就没了」。
 */
describe("〔CF2〕没接骨架的 tab：按行号往下取", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  afterEach(() => {
    vi.mocked(invoke).mockResolvedValue(undefined as never);
  });

  const mk = (sid: string, seq: number, origin?: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      ...(origin ? { origin } : {}),
      message: { type: "assistant", uuid: `${sid}-${seq}` } as never,
    }) as never;
  // ⚠ 等的是 rAF（`fillAbove` 补完一批靠 `requestAnimationFrame` 自链复检，jsdom 里约 16ms 一拍）——
  //   只让几个 0ms 宏任务过去等不到它：上一条的自链会漏进下一条的计数（首跑现打过一次）。
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 20));
  };
  const asks = () =>
    vi.mocked(invoke).mock.calls.filter((c) => c[0] === "read_session_lines").map((c) => c[1]);
  /** 后端答：`[from, until)` 里每一行都可显示（seq 就是行号）。 */
  const answerAll = (sid: string, origin?: string) =>
    ((cmd: string, a?: { from: number; until: number }) =>
      Promise.resolve(
        cmd === "read_session_lines"
          ? {
              from: a!.from,
              next: a!.until,
              eof: false,
              payloads: Array.from({ length: a!.until - a!.from }, (_, k) => mk(sid, a!.from + k, origin)),
            }
          : undefined,
      )) as never;

  it("★ L3：账尽 ＋ 最老那一条不是第 0 行 ⇒ 问 [floor − 200, floor)；回来的进账本、补上屏；问到第 0 行就不再问", async () => {
    vi.mocked(invoke).mockImplementation(answerAll("lb"));
    tm.onLine(mk("head", 1)); // 首个 tab ⇒ active
    tm.onLine(mk("lb", 300)); // 后台 tab：非批期直渲、钉 floor = 300；账本空
    const t = home(tm).store.tabs.get("lb")!;
    expect(t.window.pendingCount).toBe(0);
    expect(t.window.wantsBelow).toBe(true);
    const { renderContentRecord } = await import("../src/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    tm.switchTo("lb"); // jsdom 恒不可滚 ⇒ R-2 踢一脚
    expect(asks()).toEqual([{ origin: "<local>", jsonlPath: "/p/lb.jsonl", from: 100, until: 300 }]);
    await settle();
    // 回来的 200 条补上了屏（渲染窗口向下扩到 100）；之后接着问 [0, 100)，到第 0 行为止
    expect(t.window.floorSeq).toBe(0);
    expect(asks()).toEqual([
      { origin: "<local>", jsonlPath: "/p/lb.jsonl", from: 100, until: 300 },
      { origin: "<local>", jsonlPath: "/p/lb.jsonl", from: 0, until: 100 },
    ]);
    const rendered = new Set(spy.mock.calls.map((c) => (c[0] as { seq: number }).seq));
    for (let s = 0; s < 300; s++) expect(rendered.has(s), `第 ${s} 行没上屏`).toBe(true);
    // 🔴 取回的是历史：建卡的 sink 不接 onRealUserInput（不自动切 tab）
    for (const c of spy.mock.calls) {
      expect((c[2] as { onRealUserInput?: unknown }).onRealUserInput).toBeUndefined();
    }
    expect(t.window.belowState).toEqual({ kind: "none" });
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();
    // 到顶了：再怎么踢也不再问
    home(tm).view.activate(t);
    await settle();
    expect(asks().length).toBe(2);
  });

  it("★ L3：问不动（老后端 / 断了）⇒ 哨兵说原因、不自动重问；切走再切回来才再问一次", async () => {
    vi.mocked(invoke).mockImplementation(((cmd: string) =>
      cmd === "read_session_lines"
        ? Promise.reject(new Error("那台后端还不认这条查询"))
        : Promise.resolve(undefined)) as never);
    tm.onLine(mk("head", 1));
    tm.onLine(mk("fb", 50));
    const t = home(tm).store.tabs.get("fb")!;
    tm.switchTo("fb");
    await settle();
    expect(asks().length).toBe(1);
    expect(t.window.belowState).toEqual({ kind: "failed", reason: "那台后端还不认这条查询" });
    expect(t.stream.contentElement.querySelector(".stream-more-above")?.textContent).toContain(
      "那台后端还不认这条查询",
    );
    // 上翻（fillAbove 的每一个入口）不自动重问
    home(tm).view.activate(t);
    expect(asks().length, "activate 自己就是「切进来」—— 这一脚允许重问").toBe(2);
    await settle();
    (home(tm).view as unknown as { fillAbove(t: unknown): void }).fillAbove(t);
    (home(tm).view as unknown as { fillAbove(t: unknown): void }).fillAbove(t);
    expect(asks().length, "失败之后的上翻不许自己重问（否则是一个无界的重试环）").toBe(2);
  });

  it("★ L3：回来的行里**见过**的（被修剪出账本的）直接放回账本、不再过 onLine；没见过的走 onLine", async () => {
    vi.mocked(invoke).mockImplementation(answerAll("seen"));
    tm.onLine(mk("head", 1));
    tm.onLine(mk("seen", 250));
    const t = home(tm).store.tabs.get("seen")!;
    t.seenSeqs.add(240); // 模拟：240 见过，但已被修剪出账本
    const onLine = vi.spyOn(tm, "onLine");
    tm.switchTo("seen");
    await settle();
    const fed = onLine.mock.calls.map((c) => (c[0] as { seq: number }).seq);
    expect(fed).not.toContain(240);
    expect(fed).toContain(239);
    const { renderContentRecord } = await import("../src/render-stream-record");
    const rendered = (renderContentRecord as unknown as ReturnType<typeof vi.fn>).mock.calls.map(
      (c) => (c[0] as { seq: number }).seq,
    );
    expect(rendered, "见过的那一条也要上屏（放回账本 ⇒ 补批建卡）").toContain(240);
  });

  it("★ S3′：会话流丢过格（`onStreamGap`）⇒ 那台机器的 tab：账本整份出账、从见过的最大行号 + 1 起往后取到 eof；别的机器的 tab 不动", async () => {
    const pages = new Map<string, number>();
    vi.mocked(invoke).mockImplementation(((
      cmd: string,
      a?: { jsonlPath: string; from: number; until?: number },
    ) => {
      if (cmd !== "read_session_lines") return Promise.resolve(undefined);
      // 往后那一问（不给 until）：每份会话两段，第二段到 eof
      const n = (pages.get(a!.jsonlPath) ?? 0) + 1;
      pages.set(a!.jsonlPath, n);
      const sid = a!.jsonlPath.slice(3, -6); // "/p/<sid>.jsonl"
      const from = a!.from;
      return Promise.resolve({
        from,
        next: from + 2,
        eof: n >= 2,
        payloads: [mk(sid, from), mk(sid, from + 1)],
      });
    }) as never);
    tm.onLine(mk("head", 1));
    tm.onLine(mk("gp", 100)); // 钉 floor = 100
    tm.onBatchStart();
    for (let s = 50; s < 55; s++) tm.onLine(mk("gp", s)); // < floor ⇒ 收纳进账本
    tm.onBatchEnd();
    tm.onLine(mk("far", 7, "box"));
    const t = home(tm).store.tabs.get("gp")!;
    const far = home(tm).store.tabs.get("far")!;
    expect(t.window.pendingCount).toBeGreaterThan(0);
    const { renderContentRecord } = await import("../src/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    tm.onStreamGap("<local>");
    expect(t.window.pendingCount, "账本没出账 —— 里面可能夹着洞").toBe(0);
    await settle();
    // 本机那台上的两个 tab 都补（丢的是哪几个会话流里说不出来）；各自从自己见过的最大行号 + 1 起、到 eof 为止
    const of = (path: string) =>
      asks()
        .filter((a) => (a as { jsonlPath: string }).jsonlPath === path)
        .map((a) => (a as { from: number; until?: number }));
    expect(of("/p/gp.jsonl")).toEqual([
      { origin: "<local>", jsonlPath: "/p/gp.jsonl", from: 101 },
      { origin: "<local>", jsonlPath: "/p/gp.jsonl", from: 103 },
    ]);
    expect(of("/p/head.jsonl").map((a) => a.from)).toEqual([2, 4]);
    const rendered = spy.mock.calls
      .map((c) => c[0] as { seq: number; session_id: string })
      .filter((p) => p.session_id === "gp")
      .map((p) => p.seq);
    expect(rendered).toEqual([101, 102, 103, 104]);
    expect(far.seenSeqs.has(7), "别的机器的 tab 被动了").toBe(true);
    expect(asks().every((a) => (a as { jsonlPath: string }).jsonlPath !== "/p/far.jsonl")).toBe(true);
  });

  it("★ L4：取回的历史行不把已结束的远端 tab 翻活；实时远端行照旧翻活（正控）", async () => {
    vi.mocked(invoke).mockImplementation(answerAll("rm", "box"));
    tm.onLine(mk("head", 1));
    tm.onLine(mk("rm", 80, "box"));
    const t = home(tm).store.tabs.get("rm")!;
    tm.archiveTab("rm");
    expect(t.state).toBe(ENDED);
    tm.switchTo("rm"); // 往下取 [0, 80)：80 条远端历史行
    await settle();
    expect(asks().length).toBeGreaterThan(0);
    expect(t.state, "取回来的历史行把已结束的远端 tab 翻活了").toBe(ENDED);
    tm.onLine(mk("rm", 81, "box")); // 实时行
    expect(t.state, "正控：实时远端行照旧翻活").not.toBe(ENDED);
  });
});

/**
 * 〔CF2 · 第四波 4B〕**前端账本有上界**（`live-window.ts::PENDING_CAP` / `PENDING_KEEP`）。
 *
 * 要求住址：`设计/05 §3.3.4`「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界，满了必须落级 1 或级 2，
 * **不许静默堆**」—— 出账的那些往上翻时按行号取回（上面那一组 L3），所以这是级 2 不是「丢了就没了」。
 */
describe("〔CF2〕前端账本的上界", () => {
  it("★ 超过 CAP 就只留 seq 最高的 KEEP 条（乱序到达也按 seq 留）；「到顶了」随之回到「下面可能还有」", async () => {
    const { TailWindow, PENDING_CAP, PENDING_KEEP } = await import("../src/live-window");
    expect(PENDING_KEEP).toBeLessThan(PENDING_CAP);
    const w = new TailWindow();
    w.pinFloor(100_000);
    w.markFetchedBelow(0); // 先当作「到顶了」
    expect(w.belowState).toEqual({ kind: "none" });
    const mk = (seq: number) => ({ session_id: "s", cwd: null, path: "/p/s.jsonl", seq, message: {} }) as never;
    // 尾部优先那种到达序：高的一段先到，低的一段后到
    for (let s = PENDING_CAP; s < PENDING_CAP * 2; s++) w.defer(mk(s));
    expect(w.pendingCount, "刚好 CAP 条不修").toBe(PENDING_CAP);
    w.defer(mk(0)); // 第 CAP + 1 条（最低的）⇒ 修
    expect(w.pendingCount).toBe(PENDING_KEEP);
    expect(w.peek(1)[0].seq, "留下的不是 seq 最高的那些").toBe(PENDING_CAP * 2 - PENDING_KEEP);
    expect(w.belowState, "出过账了却还说「到顶了」—— 往上翻不会去取").toEqual({ kind: "maybe" });
  });
});
