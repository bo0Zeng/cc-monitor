// Tab 生命周期状态机的 DOM 单元测试（vitest + jsdom）。
//
// 为什么需要它：本轮迭代两个真 bug 都在 tabs.ts 的生命周期里——
//   1) resume 后已归档**本地** Tab 不自动复活（reviveTab + origin 门控）；
//   2) 关 Tab 只允许 archived（closeTab 守卫）。
// tabs.ts 重度依赖 DOM + 一堆协作模块（MessageStream / RecordTimeline / BranchFolder /
// 渲染 / Tauri IPC），无法像现有 *.test.ts 那样在裸 node 里测。这里用 jsdom 提供真 DOM、
// 把重协作者 mock 成空壳，于是能在真 TabManager 实例上断言状态翻转。

import { fullTitle } from "../../../src/frontend/ui/session-face";
import { type Mock, describe, it, expect, vi, beforeEach, afterEach, onTestFinished } from "vitest";

// ★ audit-0805 F15 第 1 步：**先让「每行调了几次」变得可测**。
//
// 这个文件此前把 `BranchFolder` 整个 stub 成空壳（每个方法 no-op），于是全仓**没有任何东西**
// 能证明 live 模式下每来一行调了几次 —— 而复核已经指出这条性质「行为上与不改完全等价
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
vi.mock("../../../src/frontend/ui/stream", () => ({
  MessageStream: class {
    contentElement = document.createElement("div");
    park(): void {}
    constructor(_root: HTMLElement) {}
    insertNode(): void {}
    batchInsert(fn: () => void): void {
      fn();
    }
    scrollToBottom(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../../src/frontend/ui/record-timeline", () => ({
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
vi.mock("../../../src/frontend/ui/branch-fold", () => ({
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
vi.mock("../../../src/frontend/ui/render-stream-record", () => ({
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
vi.mock("../../../src/frontend/ui/cards", () => ({
  reconcilePendingToolResults: vi.fn(() => []),
}));
vi.mock("../../../src/frontend/ui/cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), undoToast: vi.fn(), failToast: vi.fn() }));
// Batch14-F41：resumeTab 远端分支改走一键拉起 runner；behavior 提供 launcher 配置。
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
// 单个菜单的「杀」与「在 tmux 里 Resume」交那台（`sessions-stop` / `sessions-start`，与批量同一条）：这里换成 spy，
//   判的是单个那一侧交了什么、拿到回答之后做什么；那台怎么判住后端判据（`session_batch_tests.rs`），交法住 `tab-batch-run.vitest.ts`。
vi.mock("../../../src/frontend/ui/tab-batch-run", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/tab-batch-run")>();
  return { ...real, callStart: vi.fn(), callStop: vi.fn() };
});
// Batch14-F42：turn-end 通知与渲染独立,tabs 测试里 mock 成空壳(单独在 turn-notify.vitest 测)。
vi.mock("../../../src/frontend/ui/turn-notify", () => ({
  turnEndNotifier: { observe: vi.fn() },
}));
// ↗ 浮层［更新］就地部署：部署那一处住共用模块（`backend-deploy.ts`，它自己的判据住 `backend-deploy.vitest.ts`），这里换成 spy，判的是浮层接上它、就地说过程与结局。
vi.mock("../../../src/frontend/ui/backend-deploy", () => ({ updateBackendOf: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({
    resumeCommand: "cct",
  }),
}));
import { invoke } from "@tauri-apps/api/core";
import { updateBackendOf } from "../../../src/frontend/ui/backend-deploy";
import {
  chanArgsJson,
  chanReply,
  historyCalls,
  isChanCall,
  launchRenderShim,
  localLaunchCalls,
  sessionReadCalls,
  tmuxMintCalls,
  UNSUPPORTED,
  withAccountReads,
  withHistoryReads,
  withSessionReads,
  recordReadCalls,
} from "../../test-support/chan-fake";
import type { SessionFacts } from "../../../src/frontend/ui/session-reads";
import { invalidateAccountsCache } from "../../../src/frontend/ui/account-reads";
import { failToast, toast as showActionFailureToast } from "../../../src/frontend/ui/kit/toast";
import { __setHostOsForTests, type HostOs } from "../../../src/frontend/ui/settings/host-os";
import {
  runRemoteResume,
  runRemoteAttach,
} from "../../../src/frontend/ui/remote-launch-run";
import { callStart, callStop } from "../../../src/frontend/ui/tab-batch-run";
import {
  TabManager,
  moveTab,
  groupMoveForDrop,
  commonDirName,
  defaultGroupName,
  pickDropTarget,
  tabUnderY,
  DWELL_MS,
  DWELL_MOVE_PX,
  type Tab,
  type TabRect,
} from "../../../src/frontend/ui/tabs";
import { COLLECTION_CAP, type TabCollection } from "../../../src/frontend/ui/tab-collections";

/**
 * 钉住钟面（`performance.now` 恒定，这一条结束还原）：要逐字比「这一问交了多少期限」（`leftMs`）的判据用。
 * 期限是造的那一刻起算、过线那一刻现算剩下的；真钟下两刻之间被 GC / 调度停几毫秒，交出去的数就少几毫秒
 * （10-07 负载下红过：同一段问两次，两次的 `leftMs` 差了一档）。
 */
function pinClock(): void {
  const clock = vi.spyOn(performance, "now").mockReturnValue(1_000_000);
  onTestFinished(() => clock.mockRestore());
}

// 这一份里的会话都是 claude 会话：建好 tab 就当它的会话事实（`agent`）已经到了 —— 恢复 / 接回 / 选号要知道是哪一家，
//   事实没到之前那几项灰着的那一形另有判据（下面「还不知道是哪一家」那一组）。
const realEnsureTab = TabManager.prototype.ensureTab;
TabManager.prototype.ensureTab = function (this: TabManager, ...args: Parameters<TabManager["ensureTab"]>) {
  const t = realEnsureTab.apply(this, args);
  if (t.agent === null && !(globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown) t.agent = "claude";
  return t;
};
import { ENDED, GONE, LIVE, LIVE_ATTACHABLE, LIVE_RESUMABLE, LIVE_UNKNOWN_HOST, RECONNECTABLE, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import { readFileSync } from "node:fs";
import { buildStepLine, settleStepLine } from "../../../src/frontend/ui/cards/step-line";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { closeMenu } from "../../../src/frontend/ui/kit/menu";
import { answerAskDialog, answerAskText, askDialogText, noAskDialog } from "../../test-support/ask-dialog-driver.ts";
import type { TabStore } from "../../../src/frontend/ui/tab-store";
import type { TabBarView } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarDrag } from "../../../src/frontend/ui/tab-bar-drag";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { TabStreamView } from "../../../src/frontend/ui/tab-stream-view";
import type { TabSessionActions } from "../../../src/frontend/ui/tab-session-actions";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { appStore } from "../../../src/frontend/ui/app-store";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { peerVersionSaid } from "../../../src/frontend/ui/ipc/chan-caller";
import { recordFileWiring } from "../../../src/frontend/ui/record-file-notice";

/** 会话事实里的一格 usage（上限由后端定：这里按「判不出 ⇒ 1M」那一形造）。 */
const usageOf = (promptTokens: number, model: string | null) => ({
  promptTokens,
  model,
  peakPromptTokens: promptTokens,
  limit: 1_000_000,
  limitFrom: "assumed" as const,
});
import { applyConfigEdits, type Edit } from "./config-patch-fake";
import { dispatcher } from "../../../src/frontend/ui/keybindings/registry";
import { copyPattern } from "../../test-support/copy-pattern";

// `TabManager` 拆开之后各样东西住各自的家（store · tab 栏视图 · 拖拽 · 落盘偏好 · 流视图 · 会话动作）。
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
/** 本机 resume 那几发（`launch-local` 通道请求译回旧形参，`chan-fake.ts::localLaunchCalls`）。 */
const resumed = (): Record<string, unknown>[] => localLaunchCalls(vi.mocked(invoke).mock.calls, "resume_history_session");

/**
 * 摆一份分组：组表只有 `{id, name}`，组员 = `Tab.group`（tab 自己的属性）。
 * `tabs` 里的 sid 必须已经在栏里；没列到的 tab 一律散着。
 */
function setGroups(tm: TabManager, groups: { id: string; name: string; tabs?: string[] }[]): void {
  home(tm).prefs.collections = groups.map(({ id, name }) => ({ id, name }));
  for (const t of home(tm).store.tabs.values()) t.group = null;
  for (const g of groups) for (const sid of g.tabs ?? []) home(tm).store.tabs.get(sid)!.group = g.id;
}
/** 组 `gid` 里此刻有谁（按栏里的顺序）。 */
function membersOf(tm: TabManager, gid: string): string[] {
  return home(tm).store.orderedIds.filter((sid) => home(tm).store.tabs.get(sid)?.group === gid);
}

function makeTM(): TabManager {
  document.body.innerHTML = "";
  const barEl = document.createElement("div");
  const streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  return new TabManager(barEl, streamRootEl);
}

/** 这个 tab 的悬停卡（标签页栏的 `hoverCard`；取代原先挂在按钮上的原生 title）。 */
function cardOf(tm: TabManager, sid: string): HTMLElement | null {
  return (tm as unknown as { bar: { hoverCard(sid: string): HTMLElement | null } }).bar.hoverCard(sid);
}

/** 悬停卡逐行的字（第一行是标题全名）。 */
function cardText(tm: TabManager, sid: string): string {
  const c = cardOf(tm, sid);
  return c ? [...c.children].map((x) => x.textContent ?? "").join("\n") : "";
}

describe("TabManager 生命周期", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  it("ensureTab 默认建 live 本地 Tab（origin = `LOCAL_ORIGIN`）", () => {
    const tab = tm.ensureTab("s1", "/home/u", "p", LOCAL_ORIGIN);
    expect(tab.state).toEqual(LIVE);
    expect(tab.origin).toBe(LOCAL_ORIGIN);
    expect(home(tm).store.tabs.has("s1")).toBe(true);
  });

  it("ensureTab 远端 Tab（origin!==null）也建成 live", () => {
    const tab = tm.ensureTab("s2", "/home", "p", "pi");
    expect(tab.state).toEqual(LIVE);
    expect(tab.origin).toBe("pi");
  });

  // === Batch5-F18：骨架 Tab ===

  it("createSkeletonTab 建骨架；首行到达不重建、parentPath 回填", () => {
    tm.createSkeletonTab("sk1", "/root/proj", LOCAL_ORIGIN);
    const skeleton = home(tm).store.tabs.get("sk1")!;
    expect(skeleton.state).toEqual(LIVE);
    expect(skeleton.parentPath).toBe("");
    expect(skeleton.projectDir).toBe("/root/proj");

    // 首条真实行：同一 Tab 实例（不重建），parentPath 回填；行不带项目目录 ⇒ 不动它
    const after = tm.ensureTab("sk1", null, "/fake/sk1.jsonl", LOCAL_ORIGIN);
    expect(after).toBe(skeleton);
    expect(after.parentPath).toBe("/fake/sk1.jsonl");
    expect(after.projectDir).toBe("/root/proj");
  });

  // 会话里 shell 进了子目录：之后的记录 cwd 都是子目录，而界面只读尾巴 ⇒ 流里「见过的最早那条」是近期的一条。
  // 标题只认后端宣告的项目目录；后端不给（老后端）⇒ 退到 sid，不从行里猜。
  it("★ 项目目录只认宣告那一格：只读尾巴的流接上之后标题仍是项目名，不漂成子目录", () => {
    const tail = (sid: string, seq: number) =>
      tm.onLine({
        session_id: sid,
        cwd: "/a/proj/sub",
        path: `/p/${sid}.jsonl`,
        seq,
        message: { type: "assistant", uuid: `${sid}-${seq}` } as never,
      } as never);
    tm.createSkeletonTab("proj-sid", "/a/proj", LOCAL_ORIGIN);
    for (let seq = 40; seq < 45; seq++) tail("proj-sid", seq);
    const t = home(tm).store.tabs.get("proj-sid")!;
    expect(t.title).toBe("proj");
    expect(t.projectDir).toBe("/a/proj");

    tm.createSkeletonTab("0ldbacke-nd", null, LOCAL_ORIGIN);
    for (let seq = 40; seq < 45; seq++) tail("0ldbacke-nd", seq);
    expect(home(tm).store.tabs.get("0ldbacke-nd")!.title).toBe("0ldbacke");

    // 后端重宣告给了不同的项目目录（固定 tab 存的那份旧了）⇒ 对齐它。
    tm.createSkeletonTab("proj-sid", "/b/other", LOCAL_ORIGIN);
    expect(t.title).toBe("other");
  });

  it("远端骨架（无项目目录）标题用 sid 前缀，重复宣告幂等", () => {
    tm.createSkeletonTab("deadbeef-1234", null, "pi");
    const t = home(tm).store.tabs.get("deadbeef-1234")!;
    expect(t.origin).toBe("pi");
    expect(t.projectDir).toBeNull();
    expect(t.title).toBe("[pi] deadbeef"); // 无项目目录 → [host] + sid 前 8
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

  // === Batch7-F24：bg 会话 ⚙ 标题（〔「删掉树」〕不再挂宿主排成树：落位 / 拖拽 / 集合
  //     与普通 tab 相同那一组在本文件末尾「〔BG1〕」describe.each 里，两种 tab 各跑一遍） ===

  it("bg tab 平铺：顺序 == 到达序（不挂到同 cwd 交互 tab 之后），标题 ⚙ ＋ 任务名", () => {
    tm.createSkeletonTab("host-a", "/proj/a", LOCAL_ORIGIN, false, null);
    tm.createSkeletonTab("other", "/proj/b", LOCAL_ORIGIN, false, null);
    tm.createSkeletonTab("bg-a1", "/proj/a", LOCAL_ORIGIN, true, "评估任务");
    const order = home(tm).store.orderedIds;
    expect(order).toEqual(["host-a", "other", "bg-a1"]);
    const bg = home(tm).store.tabs.get("bg-a1")!;
    expect(bg.title).toBe("⚙ 评估任务");
  });

  it("远端 bg 标题带 origin 前缀", () => {
    tm.createSkeletonTab("h-local", "/p", LOCAL_ORIGIN, false, null);
    tm.createSkeletonTab("bg-remote", "/p", "pi", true, "远端任务");
    expect(home(tm).store.orderedIds).toEqual(["h-local", "bg-remote"]);
    expect(home(tm).store.tabs.get("bg-remote")!.title).toBe("[pi] ⚙ 远端任务");
  });

  // === Batch8-F26：(sid,seq) 去重（快照/tail 重叠区缝合的前端锚点） ===

  it("同 (tab, seq) 的行第二次到达被 seenSeqs 吞掉（快照与 tail 重叠区）", async () => {
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
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
  // 分叉血缘改由后端出成品（`history-facts`），这一组搬进文件末尾「会话事实」那组（后端给了什么 ⇒ tab 上是什么）；
  //   「首条锁定 · 只认 user/assistant」那两条口径住后端 `tests/backend/observe/facts_query_tests.rs`。

  // === Batch5-F19：last-active 写回 ===

  it("switchTo 写回 last-active；persistLastActive=false（viewer）不写", () => {
    localStorage.removeItem("cc-monitor.last-active-sid");
    tm.ensureTab("s-a", "/a", "p", LOCAL_ORIGIN);
    tm.ensureTab("s-b", "/b", "p", LOCAL_ORIGIN);
    tm.switchTo("s-b");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");

    // viewer 模式：禁写（防独立窗口污染主窗口记忆，审计 R1）
    tm.persistLastActive = false;
    tm.switchTo("s-a");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");
    localStorage.removeItem("cc-monitor.last-active-sid"); // 防同文件后续测试顺序耦合
  });

  it("手动 switchTo 触发 onManualSwitch（迟到宣告不抢焦点的清 pending 钩子）", () => {
    tm.ensureTab("m-a", "/a", "p", LOCAL_ORIGIN);
    tm.ensureTab("m-b", "/b", "p", LOCAL_ORIGIN);
    let fired = 0;
    tm.onManualSwitch = () => fired++;
    tm.switchTo("m-b"); // 默认 manual
    expect(fired).toBe(1);
    tm.ensureTab("m-c", "/c", "p", LOCAL_ORIGIN); // 非首个 tab，不切换
    tm.switchTo("m-a", "auto"); // auto 不触发
    expect(fired).toBe(1);
    localStorage.removeItem("cc-monitor.last-active-sid");
  });

  it("archiveTab：live → archived，且清空 activity（灯灭）", () => {
    const tab = tm.ensureTab("s3", "/home", "p", LOCAL_ORIGIN);
    tab.activity = { doing: "working", waitingFor: null };
    tm.archiveTab("s3");
    expect(tab.state).toEqual(ENDED);
    expect(tab.activity).toBeNull();
  });

  it("F91 红绿灯状态转移清陈旧类（activityLightClass 重构守护：两 toggle 每次都跑）", () => {
    // 守 F91 把 tab-bar 红绿灯抽到 session-status.ts 后仍逐字节等价：状态转移必须清掉旧的
    // 对立类（若哪天把两个 classList.toggle 之一改成条件执行，本测会红）。
    tm.ensureTab("lt", "/x", "p", LOCAL_ORIGIN);
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    tm.updateActivity("lt", "needs_you", "permission prompt");
    expect(btn().classList.contains("act-waiting")).toBe(true);
    expect(btn().classList.contains("act-idle")).toBe(false);
    // waiting → idle：陈旧 act-waiting 必须清、换 act-idle
    tm.updateActivity("lt", "idle", null);
    expect(btn().classList.contains("act-waiting")).toBe(false);
    expect(btn().classList.contains("act-idle")).toBe(true);
    // idle → busy：两类都清（默认绿点）
    tm.updateActivity("lt", "working", null);
    expect(btn().classList.contains("act-idle")).toBe(false);
    expect(btn().classList.contains("act-waiting")).toBe(false);
  });

  it("★ 活动信号早于 Tab 建立：远端骨架 Tab 建出来时灯必须已经是对的", () => {
    // ⚠ **这条以前一个用例都没有**，而它是远端会话的**真实时序**：
    // 远端 `live` 格 与 `activity` 格 是两个**独立**的 Tauri 事件，
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
    tm.createSkeletonTab("early-light", "/proj", "devbox", null, null);
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
    const tab = tm.ensureTab("early", null, "p", LOCAL_ORIGIN);
    expect(tab.state).toEqual(ENDED);
    expect(home(tm).store.pendingArchive.has("early")).toBe(false);
  });

  it("reviveTab（本地）：archived → live，并清 pendingArchive", () => {
    const tab = tm.ensureTab("s4", "/x", "p", LOCAL_ORIGIN);
    tm.archiveTab("s4");
    expect(tab.state).toEqual(ENDED);
    tm.reviveTab("s4");
    expect(tab.state).toEqual(LIVE);
  });

  it("reviveTab 不碰远端 Tab（origin!==null 门控）→ 仍 archived", () => {
    const tab = tm.ensureTab("s5", "/x", "p", "pi");
    tm.archiveTab("s5");
    tm.reviveTab("s5");
    expect(tab.state).toEqual(ENDED);
  });

  // ── audit-fixes F03.2：可重连（claude 退、tmux 在）生命周期 ──
  // 原先这一组断言 `tmuxIdle` ＋ `status`（可重连时 status 仍 live）；两轴之后直接断 `state`。
  it("F03.2 markTmuxIdle：进可重连 —— 死 ＋ 容器还在（不是已结束）", () => {
    const tab = tm.ensureTab("gi1", "/x", "p", "pi");
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    tm.markTmuxIdle("gi1");
    expect(tab.state).toEqual(RECONNECTABLE);
    expect(btn().classList.contains("reconnectable")).toBe(true);
    expect(btn().classList.contains("ended")).toBe(false);
  });

  it("〔U4〕tab 的 tooltip 标题之后那一行说状态（只从两轴派生）；死了的会话不再挂陈旧的「等待操作」", () => {
    const t = tm.ensureTab("tt1", "/x", "p", "pi");
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    // 第一行恒是标题；下面几行说状态
    // 悬停卡：第一行恒是标题全名；状态句那一行只从两轴 ＋ 活动信号派生（`session-face.ts::stateLine`）。
    const said = () => {
      const c = cardOf(tm, "tt1")!;
      expect(c.firstElementChild?.textContent).toBe(fullTitle(t));
      return c.querySelector(".tab-hover-state")?.textContent;
    };
    tm.updateActivity("tt1", "needs_you", "permission prompt");
    expect(said(), "活着、在等：种类还没到 ⇒ 只说需要你（不猜，也不印英文原样）").toBe(copyText("tabBar.needsKind.unknown"));
    tm.markTmuxIdle("tt1"); // 活动信号还留着（可重连不清它），但 claude 已经没了
    expect(said()).toBe(copyText("sessionState.reconnectable.tooltip"));
    tm.archiveTab("tt1");
    expect(said()).toBe(copyText("sessionState.ended.tooltip"));
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
      tm.ensureTab(sid, "/x", "p", "pi"); // 活（建 tab 不打探针）
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
    const tab = tm.ensureTab("gi2", "/x", "p", "pi");
    tm.updateActivity("gi2", "working", null); // 先有一次 busy
    tm.markTmuxIdle("gi2");
    expect(tab.state).toEqual(RECONNECTABLE);
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    // 同值 busy 再来一次（activity 无变化）——仍须回到活、类须去掉（早退前转移的守护）
    tm.updateActivity("gi2", "working", null);
    expect(tab.state).toEqual(LIVE);
    expect(btn().classList.contains("reconnectable")).toBe(false);
  });

  it("F03.2 远端复活（主信号）：可重连的 tab 又收后端重宣告/行 → ensureTab 回到活", () => {
    // D 审计修：不能只靠 session-activity（非 queue、null-activity backend 下永远不来 →
    // 活跃流式会话永久卡在可重连）。ensureTab（远端重宣告/行 = claude 复活，queue 内保序）是主信号。
    const tab = tm.ensureTab("gr1", "/x", "p", "pi");
    tm.markTmuxIdle("gr1");
    expect(tab.state).toEqual(RECONNECTABLE);
    // 复活：backend 重放该会话的行（或重宣告）→ 同 sid ensureTab
    tm.ensureTab("gr1", "/x", "p", "pi");
    expect(tab.state).toEqual(LIVE); // 删 ensureTab 里那条「远端见行」转移则此断言红
  });

  it("F03.2 已结束优先：archiveTab 把可重连改成已结束（tmux 真没了）", () => {
    const tab = tm.ensureTab("gi3", "/x", "p", "pi");
    tm.markTmuxIdle("gi3");
    expect(tab.state).toEqual(RECONNECTABLE);
    tm.archiveTab("gi3");
    expect(tab.state).toEqual(ENDED);
  });

  it("F03.2 已结束的 Tab 不被 markTmuxIdle 改回可重连", () => {
    const tab = tm.ensureTab("gi4", "/x", "p", "pi");
    tm.archiveTab("gi4");
    tm.markTmuxIdle("gi4"); // 已结束后迟到的 idle 信号——忽略
    expect(tab.state).toEqual(ENDED);
  });

  it("F03.2 可重连信号早于 Tab：进 pendingTmuxIdle，ensureTab 落实为可重连", () => {
    tm.markTmuxIdle("gi5"); // Tab 尚未建
    expect(home(tm).store.pendingTmuxIdle.has("gi5")).toBe(true);
    const tab = tm.ensureTab("gi5", "/x", "p", "pi");
    expect(tab.state).toEqual(RECONNECTABLE);
    expect(home(tm).store.pendingTmuxIdle.has("gi5")).toBe(false);
  });

  it("F03.2 已结束优先于暂存的可重连：pendingArchive + pendingTmuxIdle 同在时建成已结束", () => {
    tm.markTmuxIdle("gi6");
    tm.archiveTab("gi6"); // 二者都在暂存
    expect(home(tm).store.pendingTmuxIdle.has("gi6")).toBe(false); // archive 清掉暂存
    const tab = tm.ensureTab("gi6", "/x", "p", "pi");
    expect(tab.state).toEqual(ENDED);
  });

  it("远端 Tab 掉线归档后再收到行（ensureTab）→ 见行复活成 live", () => {
    const tab = tm.ensureTab("s6", "/x", "p", "pi");
    tm.archiveTab("s6");
    expect(tab.state).toEqual(ENDED);
    tm.ensureTab("s6", "/x", "p", "pi"); // backend 重连重放
    expect(tab.state).toEqual(LIVE);
  });

  it("closeTab 拒关 live Tab（守卫：仅 archived 可关）", () => {
    tm.ensureTab("s7", "/x", "p", LOCAL_ORIGIN);
    tm.closeTab("s7");
    expect(home(tm).store.tabs.has("s7")).toBe(true);
  });

  it("closeTab 关 archived Tab：移出 map + 摘 DOM", () => {
    const tab = tm.ensureTab("s8", "/x", "p", LOCAL_ORIGIN);
    const streamEl = tab.streamEl;
    expect(streamEl.parentElement).not.toBeNull();
    tm.archiveTab("s8");
    tm.closeTab("s8");
    expect(home(tm).store.tabs.has("s8")).toBe(false);
    expect(streamEl.parentElement).toBeNull();
  });

  it("closeTab 通知后端 forget_session（archived 才关）", () => {
    tm.ensureTab("s9", "/x", "p", LOCAL_ORIGIN);
    tm.archiveTab("s9");
    tm.closeTab("s9");
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("forget_session", {
      sessionId: "s9",
    });
  });

  it("switchTo：切 active + 清 unread + 加 .active 类", () => {
    tm.ensureTab("s10", "/x", "p", LOCAL_ORIGIN); // 首个 → 自动 active
    const tab2 = tm.ensureTab("s11", "/y", "p", LOCAL_ORIGIN);
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
    const tab = tm.ensureTab("s12", "/x", "p", LOCAL_ORIGIN);
    expect(tab.state).toEqual(LIVE); // 未被 pendingArchive 落实归档
  });

  // === Batch13-F40a：尾部优先门控 / 物化（D 审计 C-3 补测） ===
  //
  // 🔴 **它服务哪条业务要求：`INVARIANTS.md` 条 21 第 3 项**〔`P21` 2026-09-22 现打补写〕。
  //    条 21（启动重放滚动稳定性·贴底不抖）第 3 项逐字：「重放期「视口上方」旧内容不建 DOM
  //    —— `TabManager.onLine` 按 seq 门控，`seq < tab.window.floorSeq` 的旧记录只进
  //    `TailWindow` 账本，**根本不建卡不挂 DOM**；后台 virgin tab 在 `onBatchEnd` 空闲物化
  //    尾段 / `switchTo` 时同步物化」——**下面这一组（F40a 门控矩阵 → F40b fill）就是它。**
  // 把条 21 记成「全树零命中」并推出「所以它没人守」：读数对
  //    （没人在散文里点出条号），推论错（第 3 项这一组等值断言一直在守）。
  //    ⚠ 条 21 的第 1/2 项（守卫式 `snap()` · 不手动补偿 scrollTop）**不在这里** ——
  //    那是 `MessageStream` 的性质，本文件把它整个 mock 掉了（见文件抬头）。
  //    住址：`tests/frontend/ui/invariants-frontend-guard.vitest.ts` 的 ④⑤ 两格。

  const mkContent = (sid: string, seq: number, uuid: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      message: { type: "assistant", uuid } as never,
    }) as never;

  async function spyRender() {
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
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
    const { reconcilePendingToolResults } = await import("../../../src/frontend/ui/cards");
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

  /**
   * 切 tab 的同步段（点下去到处理函数返回）不读排版几何：热 tab 账本有余时「够不够一屏」那一问挪到下一帧。
   * 同步段里一读 `scrollHeight` / `getBoundingClientRect`，浏览器就得当场把刚翻出来的那个 tab 整棵样式与布局算完
   * —— 点击处理被拖长、这一帧画出来更晚（性能台架 WebKitGTK 热切同步段的大头就是这一读）。挪到下一帧：先画出来，再补。
   */
  it("热切：同步段一次排版读都没有；账本有余的那一脚补批在下一帧", async () => {
    await spyRender();
    tm.onLine(mkContent("geoA", 1, "geo-1")); // active
    tm.onBatchStart();
    for (let s = 1000; s < 1300; s++) tm.onLine(mkContent("geoB", s, `geo-${s}`));
    tm.onBatchEnd();
    const t = home(tm).store.tabs.get("geoB")!;
    tm.switchTo("geoB"); // virgin 全物化，floor 钉住
    tm.switchTo("geoA");
    tm.onBatchStart();
    for (let s = 100; s < 400; s++) tm.onLine(mkContent("geoB", s, `geo-old-${s}`)); // < floor ⇒ 收纳 300
    tm.onBatchEnd();
    expect(t.window.pendingCount).toBe(300);

    const rect = vi.spyOn(Element.prototype, "getBoundingClientRect");
    const sh = vi.spyOn(Element.prototype, "scrollHeight", "get");
    const ch = vi.spyOn(Element.prototype, "clientHeight", "get");
    tm.switchTo("geoB"); // 热切
    const reads = rect.mock.calls.length + sh.mock.calls.length + ch.mock.calls.length;
    rect.mockRestore();
    sh.mockRestore();
    ch.mockRestore();
    expect(reads, "同步段读几何 ⇒ 浏览器当场排刚翻出来的整个 tab").toBe(0);
    expect(t.window.pendingCount, "同步段里不补（补要量够不够一屏）").toBe(300);
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    expect(t.window.pendingCount, "下一帧那一脚照补（jsdom 无布局 ⇒ 判成没满一屏）").toBeLessThan(300);
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
    pinClock();
    await spyRender();
    // 按行号取回：答一个空页（from 原样、next = from）。
    // 原先答的是「[from, until) 一条可显示的都没有」—— 答了之后那一段整段记成见过（`seenSeqs.addRange`），
    //   下面「再造残账」那几条 seq 10–14 就成了自相矛盾的夹具（同一段号既说没有、又来了可显示的行）⇒ 改成空页。
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: Record<string, unknown>) =>
      Promise.resolve(
        cmd === "read_session_lines"
          ? { from: args!.from, next: args!.from, eof: false, payloads: [] }
          : undefined,
      )) as never);
    tm.onLine(mkContent("sentA", 1, "sa-1")); // active
    tm.onBatchStart();
    for (let s = 100; s < 300; s++) tm.onLine(mkContent("sentB", s, `sb-${s}`));
    tm.onBatchEnd();
    tm.switchTo("sentB"); // 物化(4 轮×150 上限 → 200 全弹尽)
    const t = home(tm).store.tabs.get("sentB")!;
    expect(t.window.pendingCount).toBe(0);
    await new Promise((r) => requestAnimationFrame(() => r(null))); // 切进来那一脚补批在下一帧（同步段不读几何）
    // 渲染窗口最老那一条是第 100 行（> 0）⇒ 下面可能还有：jsdom 恒不可滚 ⇒ 切入的 R-2 踢链（下一帧）问 [0, 100)
    // 只数这个 tab 的（等那一帧时，前面几格留下的切入那一脚也会在同一帧里跑，问的是它们自己的 tab）
    expect(recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_lines").filter((a) => (a as { jsonlPath: string }).jsonlPath === "/p/sentB.jsonl")).toEqual([
      // `leftMs`：往上翻是一件一问，交这一问的整份期限（`TabStreamView.BELOW_BUDGET_MS`）
      { origin: "<local>", jsonlPath: "/p/sentB.jsonl", from: 0, until: 100, leftMs: 60_000 },
    ]);
    expect(t.stream.contentElement.querySelector(".stream-more-above")?.textContent).toContain(copyText("tabStreamView.sentinel.loading"));
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
    expect(sentinel?.textContent).toBe(copyText("tabStreamView.sentinel.moreCount", { n: 5 }));
    // 切入:R-2 踢链(不可滚+账本有余)→ 补批到账尽 → 哨兵消失
    tm.switchTo("sentB");
    await new Promise((r) => requestAnimationFrame(() => r(null))); // 切进来那一脚补批在下一帧（同步段不读几何）
    expect(t.window.pendingCount).toBe(0);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).toBeNull();
  });

  /**
   * ★ 步 3：**「够不够一屏」读的是真实布局，不是 `scrollHeight`。**
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

  it("kind 升格:bg 骨架先到,interactive 宣告后到 → 升格(标题去 ⚙),位置不动", () => {
    // 场景还原(用户截图):bg-spare 的宣告先到,父会话被建成 ⚙;interactive 宣告后到必须升格纠正。
    // 原先升格还会把它摘下来按宿主重新挂树;树删了 ⇒ 位置与 kind 无关、不动。
    tm.createSkeletonTab("excel", "/proj/research", LOCAL_ORIGIN, false, null);
    tm.createSkeletonTab("parent", "/proj/research", LOCAL_ORIGIN, true, "迁移服务"); // 谎报形态先到
    tm.createSkeletonTab("fork-empty", "/proj/research", LOCAL_ORIGIN, true, "迁移服务"); // 空克隆
    expect(home(tm).store.orderedIds).toEqual(["excel", "parent", "fork-empty"]);
    expect(home(tm).store.tabs.get("parent")!.title).toContain("⚙");

    tm.createSkeletonTab("parent", "/proj/research", LOCAL_ORIGIN, false, null); // 真身宣告后到
    const p = home(tm).store.tabs.get("parent")!;
    expect(p.background).toBe(false);
    expect(p.title).not.toContain("⚙");
    expect(home(tm).store.orderedIds).toEqual(["excel", "parent", "fork-empty"]);
  });

  it("kind 不降格:interactive tab 后到 bg 宣告(spare 谎报)保持交互形态", () => {
    tm.createSkeletonTab("host2", "/proj/x", LOCAL_ORIGIN, false, null);
    tm.createSkeletonTab("host2", "/proj/x", LOCAL_ORIGIN, true, "spare 噪声");
    const t = home(tm).store.tabs.get("host2")!;
    expect(t.background).toBe(false);
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
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", "devbox");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTab("r1");
    // 默认 resume（没点号）⇒ 跟随，交给那台判。
    expect(runRemoteResume).toHaveBeenCalledWith("devbox", "claude", "r1", "/home/pi/proj", "cct", { account: { kind: "follow" }, preflight: expect.any(Function) });
    expect(resumed()).toEqual([]);
  });

  it("★ 会话事实到了才知道是哪一家：codex 会话按 codex 起；还不知道是哪一家 ⇒ 不起、说一句（不落 claude）", async () => {
    tm.ensureTab("cx", "/home/pi/proj", "/p/cx.jsonl", "devbox");
    home(tm).store.tabs.get("cx")!.agent = "codex";
    tm.archiveTab("cx");
    await home(tm).actions.resumeTab("cx");
    expect(vi.mocked(runRemoteResume).mock.calls.at(-1)?.[1]).toBe("codex");
    vi.mocked(runRemoteResume).mockClear();
    (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = true;
    try {
      tm.ensureTab("u1", "/home/pi/proj", "/p/u1.jsonl", "devbox");
    } finally {
      (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = false;
    }
    tm.archiveTab("u1");
    await home(tm).actions.resumeTab("u1");
    expect(runRemoteResume, "还不知道是哪一家就起了").not.toHaveBeenCalled();
    expect(String(vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[0])).toContain(copyText("tabSessionActions.agent.unknown"));
  });

  // 点了号 ⇒ 点名那个号原样交给那台（它判选不选得了；选不了时的说清 ＋ 显式选择由 `remote-launch-run.vitest.ts` 钉）。
  it("resumeTab 带账号名 → 点名那个号交给那台，界面不先查账号库", async () => {
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", "devbox");
    tm.archiveTab("r1");
    await home(tm).actions.resumeTab("r1", "z");
    expect(runRemoteResume).toHaveBeenCalledWith("devbox", "claude", "r1", "/home/pi/proj", "cct", {
      account: { kind: "named", name: "z" },
      preflight: expect.any(Function),
    });
    expect(historyCalls(vi.mocked(invoke).mock.calls, "update_history_metadata")).toHaveLength(0);
  });

  it("本地归档 tab → 仍走 resume_history_session，不碰远端 runner", async () => {
    tm.ensureTab("l1", "/home/u/p", "/p/l1.jsonl", LOCAL_ORIGIN);
    tm.archiveTab("l1");
    vi.mocked(invoke).mockImplementation(withHistoryReads(launchRenderShim(() => Promise.resolve(undefined))));
    await home(tm).actions.resumeTab("l1");
    expect(runRemoteResume).not.toHaveBeenCalled();
    expect(resumed()).toContainEqual({
      agent: "claude",
      sessionId: "l1",
      cwd: "/home/u/p",
      launcher: "cct", // 通用页那一格是各台的默认（本机没单独设 ⇒ 也用它）
      // P3t-Y2b：读不到本机 tmux 名单（这里 `invoke` 的缺省 mock 回 undefined）⇒ **不铸名**。
      // `null` 在这里是「不知道」，不是「没有名字被占」—— 硬铸就是不避让（issue #76）。
      tmuxName: null,
      account: { kind: "follow" },
    });
  });

  // ★★ P3t-Y2b：本机 resume 真的把铸好的 tmux 名传下去。
  //
  // 上面那条只证「不知道的时候不铸」。**光有它，整个 Y2b 被回退掉也不会红**
  //（回退之后恒 `tmuxName: null`，那条照样绿）⇒ 必须再钉正面：知道的时候要铸、且要避让。
  it("P3t-Y2b 本地 resume：本机后端铸了名字 → 原样传给起会话那一问", async () => {
    (invoke as unknown as Mock).mockImplementation(withHistoryReads(launchRenderShim(async (cmd: string) => {
      // `K-R96`：基名从 cwd 派生 ⇒ `/home/u/p` ⇒ `p-cc`；它已被占 ⇒ 让到 `-2`。派生 ＋ 避让在本机后端（`terminal-name-mint`），
      //   替身写死它铸了 `p-cc-2`；这里钉的是「问了、用的就是它铸的」（下面 `tmuxMintCalls` 那一行）。
      if (cmd === "tmux_name_mint") return "p-cc-2";
      return undefined;
    })));
    tm.ensureTab("l1abcdef", "/home/u/p", "/p/l1abcdef.jsonl", LOCAL_ORIGIN);
    tm.archiveTab("l1abcdef");
    await home(tm).actions.resumeTab("l1abcdef");
    expect(resumed()).toContainEqual({
      agent: "claude",
      sessionId: "l1abcdef",
      cwd: "/home/u/p",
      launcher: "cct",
      // 让到 `-2` 而不是撞上 `p-cc` —— 撞上去就是「静默接进第一个会话，
      // 而用户以为开了新的」（issue #76 那一族，F13 记着同一个坑）。
      // `K-R96`：名字里**没有 sid**（`l1abcdef` 一个字都不出现）—— 它骑在 `@ccm_sid` 上。
      tmuxName: "p-cc-2",
      account: { kind: "follow" },
    });
    expect(tmuxMintCalls(vi.mocked(invoke).mock.calls)).toEqual([[LOCAL_ORIGIN, { cwd: "/home/u/p" }]]);
  });
});

// 跟随 / 账号 0：界面不读 pin、不查账号库，交的就是那一格（那台判）。
describe("跟随 / 用账号 0：界面不读 pin，交的就是那一格", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", "devbox");
    tm.archiveTab("r1");
  });

  it("直连跟随 · 用账号 0：一次 pin 都不读；交 follow / base", async () => {
    await home(tm).actions.resumeTab("r1");
    await home(tm).actions.resumeTab("r1", undefined, true);
    expect(historyCalls(vi.mocked(invoke).mock.calls, "list_last_accounts")).toHaveLength(0);
    expect(vi.mocked(runRemoteResume).mock.calls.map((c) => (c[5] as { account: unknown }).account)).toEqual([
      { kind: "follow" },
      { kind: "base" },
    ]);
  });
});

// audit-fixes F03 步骤1（idle-tmux 就地复用，治 #76 根因 + #75 一条）：
// 单个「在 tmux 里 Resume」与批量「在 tmux 里后台起」同一条：交那台一个 sid 的一批（`sessions-start`，那台判在不在跑 · 空 tmux 就地键入 ·
//   铸名交一行 ccm），单个只多一步：起好之后开一个终端接进去。那台怎么判住后端判据；这里判单个这一侧交了什么、拿到回答之后做什么。
describe("单个「在 tmux 里 Resume」交那台（与批量同一条，只差 sid 的个数）", () => {
  let tm: TabManager;
  const reply = (outcome: string, why: string | null, session: string | null, detail = "") => ({
    sid: "r1", outcome, why, detail, said: null, copyDetail: "", session, bus: null, cmd: null, account: null, unavailable: null,
  });
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
    tm.ensureTab("r1", "/home/pi/proj", "/p/r1.jsonl", "devbox");
    tm.archiveTab("r1");
  });

  it("基座（useBase）：交的是这一个（账号 0）；起好了 ⇒ 接进那台答的那个会话", async () => {
    vi.mocked(callStart).mockResolvedValue([reply("done", null, "proj-cc")] as never);
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    // 第 4 格是起哪一家（标签页里的会话 ＝ 流跟的那一家；起法住 `tmux-resume.ts`，历史页按那一行自己的一家交）。
    expect(vi.mocked(callStart).mock.calls).toEqual([["devbox", "tmux", [{ sid: "r1", cwd: "/home/pi/proj", account: { kind: "base" } }], "claude"]]);
    expect(runRemoteAttach).toHaveBeenCalledWith("devbox", "claude", "proj-cc");
  });

  it("那台答「已经在跑」⇒ 不另起，接进去；在跑的不止一个 ⇒ 接第一个 ＋ 说出来", async () => {
    vi.mocked(callStart).mockResolvedValue([reply("skipped", "running", "cc-r1abcd", "cc-r1abcd")] as never);
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    expect(runRemoteAttach).toHaveBeenLastCalledWith("devbox", "claude", "cc-r1abcd");
    vi.mocked(callStart).mockResolvedValue([reply("skipped", "ambiguous", "cc-r1abcd", "cc-r1abcd, cc-r1efgh")] as never);
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    expect(runRemoteAttach).toHaveBeenLastCalledWith("devbox", "claude", "cc-r1abcd");
    expect(showActionFailureToast).toHaveBeenCalledWith(copyText("tabSessionActions.dupes.title"), expect.stringContaining("2"), expect.objectContaining({ level: "info" }));
  });

  it("记录已不在 / 那台没起成 / 问不到那台 ⇒ 不开终端、出声", async () => {
    vi.mocked(callStart).mockResolvedValue([reply("skipped", "record_gone", null, "/home/pi/.claude/projects")] as never);
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    expect(vi.mocked(showActionFailureToast).mock.calls.map((c) => c[0])).toContain(copyText("sessionState.recordGone.title"));
    vi.mocked(callStart).mockResolvedValue([reply("failed", "start_failed", "proj-cc", "ccm: 起不来")] as never);
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    vi.mocked(callStart).mockRejectedValue(new Error("通道不在"));
    vi.mocked(failToast).mockClear();
    await home(tm).actions.resumeTabTmux("r1", undefined, true);
    expect(runRemoteAttach).not.toHaveBeenCalled();
    expect(vi.mocked(showActionFailureToast).mock.calls.length).toBe(2);
    // 问不到那台：那次失败交给失败 toast（带详情 ⇒ 换成那一句；不带 ⇒ 标题留「没起」那句）。
    expect(vi.mocked(failToast).mock.calls.map((c) => c[0])).toEqual([copyText("remoteLaunchRun.inPlace.notRun")]);
  });

  it("跟随：交 follow（号与模型那台判，单个与批量同一条）；那台说选不了 ⇒ 不接、给显式选择，点了点名再交", async () => {
    vi.mocked(callStart).mockResolvedValueOnce([
      { ...reply("skipped", "account_unavailable", null, "z"), unavailable: { requested: "z", pinned: true, listKnown: true, alternative: "b" } },
    ] as never);
    await home(tm).actions.resumeTabTmux("r1");
    expect(vi.mocked(callStart).mock.calls[0][2]).toEqual([{ sid: "r1", cwd: "/home/pi/proj", account: { kind: "follow" } }]);
    expect(runRemoteAttach).not.toHaveBeenCalled();
    const hit = vi.mocked(showActionFailureToast).mock.calls.find((c) => c[0] === copyText("accountPick.refused.title"));
    expect(hit, "没有说清为什么没起").toBeTruthy();
    vi.mocked(callStart).mockResolvedValueOnce([reply("done", null, "proj-cc")] as never);
    hit![2]!.onClick!();
    await vi.waitFor(() => expect(runRemoteAttach).toHaveBeenCalledWith("devbox", "claude", "proj-cc"));
    expect(vi.mocked(callStart).mock.calls[1][2]).toEqual([{ sid: "r1", cwd: "/home/pi/proj", account: { kind: "named", name: "b" } }]);
  });
});

describe("F51 tab 右键 attach 反查（异步就绪 + 跨 tab 竞态守卫 R-1）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    tm = makeTM();
  });

  const btnOf = (sid: string): HTMLElement =>
    home(tm).bar.tabButtons.get(sid)!.root;
  const rightClick = (sid: string): void => {
    btnOf(sid).dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }),
    );
  };
  /** 菜单里字是 `label` 的那一项（只认字那一格，不连右侧键位 / 第二行）。 */
  const itemLabeled = (label: string): HTMLButtonElement | null => {
    const menu = document.body.querySelector("[role=menu]");
    const items = [...(menu?.querySelectorAll<HTMLButtonElement>("[role^=menuitem]") ?? [])];
    return items.find((b) => b.querySelector("[data-part=label]")?.textContent === label) ?? null;
  };
  const whyOf = (b: HTMLElement | null): string | undefined => b?.querySelector("[data-part=why]")?.textContent ?? undefined;
  const attachBtn = (): HTMLButtonElement | null => itemLabeled(copyText("tabMenu.attach.label"));
  const killBtn = (): HTMLButtonElement | null => itemLabeled(copyText("tabMenu.kill.label"));
  /** 点「在终端里打开」⇒ 接的是哪个 tmux 会话（交给那台的名字）。 */
  const attachTarget = (): string | undefined => {
    vi.mocked(runRemoteAttach).mockClear();
    attachBtn()?.click();
    return vi.mocked(runRemoteAttach).mock.calls.at(-1)?.[2] as string | undefined;
  };
  /** 点「结束会话…」⇒ 确认框里写的字（读完按取消关掉）。 */
  const killConfirmText = async (): Promise<string> => {
    killBtn()?.click();
    await flush();
    const dlg = document.body.querySelector<HTMLElement>("[role=alertdialog]");
    const text = dlg?.textContent ?? "";
    [...(dlg?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find((b) => b.textContent === copyText("kit.dialog.cancel"))?.click();
    return text;
  };
  const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

  // ★★ P3 刀 2 的 UI 半：**本机 tab 也有「杀死会话」**。
  //
  // 三格各钉一条，因为它们**失败方式完全不同**：
  // ① 通道不在（`null`）⇒ 不许留一个能点的破坏性菜单项 —— `null` 是「不知道」，
  //    留着它等于让用户点一个必失败的 kill。
  // ② 命中恰好一个 ⇒ 按 `@ccm_sid` 认，**不按名字前缀猜**（下面那条埋了名字诱饵）。
  // ③ 命中 ≥2 个 ⇒ 拒绝，不折叠成第一个（F04 R10 同款分级：破坏性动作代价不可逆）。
  // 〔「改. 重新读取对所有tab生效」·「右键菜单那一项删（一个入口）」〕右键菜单里零处「重新读取」；
  // 正控：同一张菜单开出来了（有「在新窗口打开」），栏顶那颗在。
  it("〔REREAD〕tab 右键菜单里没有「重新读取」，它在栏顶", () => {
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    rightClick("k1abcdef");
    const labels = [...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem]") ?? [])].map((b) => b.querySelector("[data-part=label]")?.textContent);
    expect(labels).toContain(copyText("tabMenu.open.openInWindow"));
    expect(labels.filter((l) => l?.includes(copyText("rsFilewinEditPage.more.reload")) || l === copyText("tabBar.head.refresh"))).toEqual([]);
    expect(document.body.querySelector(".tab-bar-reread")?.getAttribute("aria-label")).toBe(copyText("tabBar.head.refresh"));
  });

  // 栏顶一按 ⇒ 有打开 tab 的每台**恰好一次**整机 `resync`（不带 sid），台数 == 机器数（两向：多一台少一台都红）；
  // 各台并行（全部发出去之后才有一台回来）；在飞时再按不重入；做完按台说一句，没问到的说原因。
  it("〔REREAD〕栏顶「重新读取」⇒ 每台恰好一次整机 resync，并行、在飞不重入、按台汇总", async () => {
    const asked: [string, unknown][] = [];
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    vi.mocked(invoke).mockImplementation(withHistoryReads(async (cmd: string, args?: unknown) => {
      const a = args as { origin?: string; op?: string; payload?: number[] } | undefined;
      if (cmd === "chan_call" && a?.op === "resync") {
        asked.push([a.origin ?? "", JSON.parse(new TextDecoder().decode(Uint8Array.from(a.payload ?? [])))]);
        await gate;
        if (a.origin === "box") throw new Error(copyText("beServer.words.cantConnect"));
        const u = new TextEncoder().encode(
          JSON.stringify({ added: 0, removed: 0, retagged: 0, caught_up: a.origin === "laptop" ? 2 : 0, watchers: 1, unavailable: [], uncancellable: [] }),
        );
        return u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength);
      }
      return undefined;
    }));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    tm.ensureTab("k2abcdef", "/home/u/q", "/p/k2.jsonl", LOCAL_ORIGIN);
    tm.ensureTab("g1abcdef", "/home/g", "/p/g1.jsonl", "laptop");
    tm.ensureTab("b1abcdef", "/home/b", "/p/b1.jsonl", "box");
    tm.archiveTab("b1abcdef"); // 已结束的也在栏上 ⇒ 那台也算
    const btn = document.body.querySelector<HTMLButtonElement>(".tab-bar-reread")!;
    expect(btn.closest(".tab-bar-head"), "它在栏顶那一排（不在列表里）").not.toBeNull();
    btn.click();
    await flush();
    btn.click(); // 在飞：不重入
    await flush();
    expect(btn.disabled).toBe(true);
    expect(asked.map(([o]) => o).sort()).toEqual([LOCAL_ORIGIN, "box", "laptop"].sort());
    expect(asked.every(([, p]) => JSON.stringify(p) === "{}")).toBe(true);
    release();
    for (let i = 0; i < 5; i++) await flush();
    expect(btn.disabled).toBe(false);
    const last = vi.mocked(showActionFailureToast).mock.calls.at(-1)!;
    expect(last[0]).toBe(copyText("resync.machines.titlePartial"));
    const lines = last[1].split("\n");
    expect(lines).toHaveLength(3);
    expect(lines).toEqual(
      expect.arrayContaining([
        copyText("resync.machines.ok", { machine: copyText("resync.machines.local"), said: copyText("resync.done.caughtNone") + copyText("resync.done.same") }),
        copyText("resync.machines.ok", { machine: "laptop", said: copyText("resync.done.caughtUp", { n: 2 }) + copyText("resync.done.same") }),
        expect.stringMatching(copyPattern("resync.machines.failed", { machine: "box" }, { whole: true })),
      ]),
    );
  });

  it("P3 刀2-UI 本机 tab 右键：问不到那台（null）→ 结束项灰着、第二行写原因，不留能点的破坏性动作", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_local_tmux" ? Promise.resolve(null) : Promise.resolve(undefined),
    ));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    rightClick("k1abcdef");
    expect(killBtn()?.disabled, "还在问：先画出项、不可点").toBe(true);
    expect(killBtn()?.querySelector("[data-part=label]")?.textContent, "问的时候不换字").toBe(copyText("tabMenu.kill.label"));
    await flush();
    expect(killBtn()?.disabled).toBe(true);
    expect(whyOf(killBtn())).toBe(copyText("tabMenu.probe.failed"));
  });

  it("P3 刀2-UI 本机 tab 右键：按 @ccm_sid 认出唯一那个（名字前缀是诱饵）", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            // ★ 名字长得就是本 tab 的 `<sid8>-cc`，但 `@ccm_sid` 是别人的 —— **诱饵**。
            //   按名字前缀猜就会中它，那是「拿命名巧合当身份」（§30 禁的那一类）。
            { name: "k1abcdef-cc", path: "/home/u/p", command: "claude", attached: false, windows: 1, sid: "someone-else", agent: true },
            { name: "unrelated-cc", path: "/home/u/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef", agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    const said = await killConfirmText();
    expect(said, "tmux 会话名在确认框正文里").toContain("unrelated-cc");
    expect(said).not.toContain("k1abcdef-cc");
  });

  // ★★ P3 刀 3：本机**空 tmux**（claude 已退、只剩交互 shell）→ 就地 resume。
  //
  // 两条：正面（空壳 ⇒ 两格都在）与反面（有活 claude ⇒ 就地那格必须**不在**）。
  // 光有正面不够：把「空壳判定」删掉、无条件给这一格，正面照样绿 ——
  // 而那样会往一个**正在跑 claude** 的会话里再送一遍载荷（F14 逐字记着这个后果）。
  const resumeIntoBtn = (): HTMLButtonElement | null => itemLabeled(copyText("tabMenu.inPlace.label"));

  it("P3 刀3 本机空 tmux（Claude 已退出）→ 给「在 tmux 会话里恢复」，结束的确认框点名那个 tmux 会话", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            // command 不是 claude ⇒ 空壳（claude 已退、只剩交互 shell）。
            { name: "i1-cc", path: "/p", command: "bash", attached: false, windows: 1, sid: "k1abcdef", agent: false },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(resumeIntoBtn()?.disabled).toBe(false);
    expect(await killConfirmText()).toContain(copyText("sessionState.killIdle.title", { name: "i1-cc" }));
  });

  it("P3 刀3 反面：会话里还跑着 claude → **不给**就地 resume（别往活会话再送一遍载荷）", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            { name: "i1-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef", agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(resumeIntoBtn()).toBeNull();
    expect(await killConfirmText()).toMatch(copyPattern("tabSessionActions.kill.cuts", { name: "i1-cc" }));
  });

  it("P3 刀2-UI 本机 tab 右键：同身份命中 2 个 → 拒绝，不折叠成第一个", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            { name: "a-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef", agent: true },
            { name: "b-cc", path: "/p", command: "claude", attached: false, windows: 1, sid: "k1abcdef", agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL_ORIGIN);
    rightClick("k1abcdef");
    await flush();
    expect(whyOf(killBtn())).toBe(copyText("tabMenu.kill.dupesWhy", { n: 2 }));
    expect(killBtn()?.disabled).toBe(true);
  });

  it("远端 tab 右键 → 反查命中 claude 会话 → attach 项由禁用占位就绪为可点", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-abc", path: "/a", command: "claude", attached: false, windows: 1, sid: "A", agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("A", "/a", "p", "hostA");
    rightClick("A");
    expect(attachBtn()?.disabled).toBe(true); // 还在问：先画出项、不可点
    await flush();
    expect(attachBtn()?.disabled).toBe(false); // 就绪
    expect(attachTarget()).toBe("cc-abc");
  });

  it("前台命令报 node（claude 是 Node CLI）也认(D-Sug2)", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "sess", path: "/a", command: "node", attached: true, windows: 2, sid: "A", agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("A", "/a", "p", "hostA");
    rightClick("A");
    await flush();
    expect(attachTarget()).toBe("sess");
  });

  it("无匹配（cwd 不符）→ 占位移除,不显示 attach", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "other", path: "/elsewhere", command: "claude", attached: false, windows: 1, agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("A", "/a", "p", "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()).toBeNull();
  });

  // audit-fixes F03.3（attach-into-idle）：无活 claude 但目标 sid 的空 tmux（@ccm_sid 命中、
  // command=bash）还在 → attach 项就绪为「Attach（空 tmux …）」，让用户 attach 进空 shell。
  // 变异锚点：删 resolveAttachMenuItem else 分支的 idle 处理 → 占位被移除、attachBtn=null → 红。
  it("无活 claude 但有目标 sid 的空 tmux（idle）→「在终端里打开」就绪、接的就是它", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-A1", path: "/a", command: "bash", attached: false, windows: 1, sid: "A", agent: false },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("A", "/a", "p", "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()?.disabled).toBe(false);
    expect(attachTarget()).toBe("cc-A1");
  });

  // F04（R10）：命中 ≥2 个精确同 sid 的活会话——attach 仍就绪（非破坏性，警告即可），
  // kill 项禁用 + 诊断文案（破坏性，选错代价不可逆，须到终端手动处理）。preview 不受影响。
  it("目标 sid 同时活在 2 个 tmux（命中 ≥2 个）→ attach 仍就绪，kill 项禁用+诊断文案", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([
            { name: "cc-A1", path: "/a", command: "claude", attached: false, windows: 1, sid: "A", agent: true },
            { name: "cc-A2", path: "/other", command: "claude", attached: false, windows: 1, sid: "A", agent: true },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("A", "/a", "p", "hostA");
    rightClick("A");
    await flush();
    expect(attachBtn()?.disabled).toBe(false);
    expect(whyOf(attachBtn())).toBe(copyText("tabMenu.attach.dupesWhy", { n: 2, name: "cc-A1" }));
    expect(killBtn()?.disabled).toBe(true);
    expect(whyOf(killBtn())).toBe(copyText("tabMenu.kill.dupesWhy", { n: 2 }));
  });

  // 〔终端 L1〕看画面只有一个入口：抽屉终端页（「看它的终端」）；盖住主窗口的旧浮层那一项不再出（远端本机都一样）。
  it("〔终端 L1〕远端 tab 右键：有「看它的终端」，没有开旧浮层的那一项", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux"
        ? Promise.resolve([{ name: "cc-A1", path: "/a", command: "claude", attached: false, windows: 1, sid: "A", agent: true }])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("A", "/a", "p", "hostA");
    rightClick("A");
    await flush();
    const items = [...(document.body.querySelector("[role=menu]")?.querySelectorAll<HTMLElement>("[role^=menuitem]") ?? [])];
    const labels = items.map((b) => b.querySelector("[data-part=label]")?.textContent);
    expect(labels).toContain(copyText("tabMenu.open.drawer"));
    expect(items.filter((b) => b.dataset.id === "preview" || b.querySelector("[data-part=label]")?.textContent?.includes("画面")), "旧浮层那一项还在").toEqual([]);
    itemLabeled(copyText("tabMenu.open.drawer"))?.click();
    expect(document.querySelector(".pane-preview-overlay"), "点了不许开旧浮层").toBeNull();
  });

  it("R-1 守卫:tab A 查询在飞时右键 tab B → A 迟到结果不污染 B 的菜单", async () => {
    let resolveA!: (v: unknown) => void;
    const aPending = new Promise((r) => (resolveA = r));
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string, args?: unknown) => {
      if (cmd !== "list_remote_tmux") return Promise.resolve(undefined);
      const origin = (args as { origin: string }).origin;
      if (origin === "hostA") return aPending; // 在飞
      return Promise.resolve([
        { name: "B-sess", path: "/b", command: "claude", attached: false, windows: 1, sid: "B", agent: true },
      ]);
    }));
    tm.ensureTab("A", "/a", "p", "hostA");
    tm.ensureTab("B", "/b", "p", "hostB");

    rightClick("A"); // 菜单 A + A 查询在飞
    rightClick("B"); // 关 A、开菜单 B（新代次）+ B 查询即刻 resolve
    await flush();
    expect(attachBtn()?.disabled).toBe(false); // B 自身反查就绪

    resolveA([
      { name: "A-sess", path: "/a", command: "claude", attached: false, windows: 1, sid: "A", agent: true },
    ]);
    await flush(); // A 迟到:代次不符 → 整体 no-op,不动 B 菜单
    expect(attachTarget()).toBe("B-sess");
  });
});

describe("已结束的 tab 右键「恢复 ▸」：与历史页「恢复 ▾」同一个组件（恢复 · 账号 · 运行于 · 在此目录新建会话）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    tm = makeTM();
  });

  const rightClick = (sid: string): void => {
    home(tm).bar.tabButtons
      .get(sid)!
      .root.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }),
      );
  };
  // querySelectorAll 连带找到嵌套 flyout 里的项（jsdom 不管 CSS 的显隐）：不用先模拟悬停展开就能点到同一个按钮。
  const labelOf = (b: Element): string => b.querySelector('[data-part="label"]')?.textContent ?? b.textContent ?? "";
  const menuLabels = (): string[] =>
    [...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem]") ?? [])].map(labelOf);
  // 同名时取最里层那一个（一级项「恢复 ▸」与 flyout 里的「恢复」可以同字）。
  const item = (label: string): HTMLButtonElement | undefined =>
    [...(document.body.querySelector("[role=menu]")?.querySelectorAll<HTMLButtonElement>("[role^=menuitem]") ?? [])].filter((b) => labelOf(b) === label).pop();
  const flyout = (): HTMLElement => {
    const wrap = [...document.body.querySelectorAll("[role=none]")].find((w) => labelOf(w.children[0]) === copyText("tabMenu.item.resume"));
    expect(wrap, "找不到「恢复 ▸」那一项").toBeTruthy();
    return wrap!.querySelector<HTMLElement>(":scope > [role=menu]")!;
  };
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 12; i++) await flushMicro();
  };
  const RESUME = copyText("history.row.resume");
  const TMUX = copyText("resumeMenu.run.tmux");
  const DIRECT = copyText("resumeMenu.run.direct");
  const BASE = copyText("resumeMenu.account.base");

  const accounts = (...names: string[]) =>
    vi.mocked(invoke).mockImplementation(withHistoryReads(withAccountReads((cmd: string) =>
      cmd === "list_remote_accounts"
        ? Promise.resolve({
            available: true,
            error: null,
            meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: names.length, error: null },
            accounts: names.map((name, i) => ({ name, email: `${name}@x`, configDir: `/h/${name}`, isDefault: i === 0, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true })),
          })
        : cmd === "list_remote_tmux"
          ? Promise.resolve([])
          : cmd === "tmux_name_mint"
            ? Promise.resolve("proj-cc")
            : Promise.resolve(undefined),
    )));

  const openArchived = async (sid = "r1", origin: string = "devbox"): Promise<void> => {
    tm.ensureTab(sid, "/home/pi/proj", "p", origin);
    tm.archiveTab(sid);
    rightClick(sid);
    await settle();
  };


  it("活会话右键「账号…」：面板接上了才给，点了开这个会话的面板", async () => {
    tm.ensureTab("r2", "/home/pi/proj", "p", "devbox");
    rightClick("r2");
    await settle();
    expect(menuLabels(), "面板没接上不给").not.toContain(copyText("acct.menu.open"));
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    const open = vi.fn();
    tm.onOpenAccountPanel = open;
    rightClick("r2");
    await settle();
    item(copyText("acct.menu.open"))!.click();
    expect(open).toHaveBeenCalledWith("r2", "devbox");
  });
  it("远端：一级项 ＋ flyout（最上面「恢复」· 运行于两项单选 · 在此目录新建会话）；点单选只挪勾、菜单不关；「恢复」按勾着的那一组起", async () => {
    invalidateAccountsCache();
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_remote_tmux" ? Promise.resolve([]) : cmd === "tmux_name_mint" ? Promise.resolve("proj-cc") : Promise.resolve(undefined),
    ));
    await openArchived();
    const fly = flyout();
    expect([...fly.children].map((c) => (c.getAttribute("role") === "presentation" ? `[${c.textContent}]` : c.getAttribute("role") === "separator" ? "—" : labelOf(c)))).toEqual([
      RESUME, "—", `[${copyText("resumeMenu.group.run")}]`, TMUX, DIRECT, "—", copyText("history.menu.newInDir"),
    ]);
    expect(item(RESUME)!.textContent, "「恢复」右侧灰字是勾着的方式").toContain(DIRECT);
    // 点「在 tmux 里」：只挪勾，什么都不起
    item(TMUX)!.click();
    await settle();
    expect(document.body.querySelector("[role=menu]"), "点单选菜单不关").not.toBeNull();
    expect(item(TMUX)!.getAttribute("aria-checked")).toBe("true");
    expect(callStart).not.toHaveBeenCalled();
    expect(item(RESUME)!.textContent).toContain(copyText("resumeMenu.hint.tmux"));
    // 「恢复」⇒ 交那台在 tmux 里起这一个（在不在跑 · 铸名 · 判号都在那台）；账号跟随
    vi.mocked(callStart).mockResolvedValue([{ sid: "r1", outcome: "done", why: null, detail: "", session: "proj-cc", bus: null, cmd: null }] as never);
    item(RESUME)!.click();
    await settle();
    expect(vi.mocked(callStart).mock.calls[0].slice(0, 2)).toEqual(["devbox", "tmux"]);
    expect(vi.mocked(callStart).mock.calls[0][2]).toEqual([{ sid: "r1", cwd: "/home/pi/proj", account: { kind: "follow" } }]);
    expect(runRemoteAttach).toHaveBeenCalledWith("devbox", "claude", "proj-cc");
    // 勾在这个 tab 上记着：再开还是 tmux；挪回不用 tmux ⇒ 直接恢复
    rightClick("r1");
    await settle();
    expect(item(TMUX)!.getAttribute("aria-checked")).toBe("true");
    item(DIRECT)!.click();
    item(RESUME)!.click();
    await settle();
    expect(runRemoteResume).toHaveBeenCalledWith("devbox", "claude", "r1", "/home/pi/proj", "cct", { account: { kind: "follow" }, preflight: expect.any(Function) });
  });

  it("本机：同一个组件（不再是单独一项）；在 tmux 里 ⇒ 本机那一条（交本机后端在 tmux 里起再接进去）", async () => {
    invalidateAccountsCache();
    vi.mocked(callStart).mockResolvedValue([{ sid: "l1", outcome: "done", why: null, detail: "", session: "p-cc", bus: null, cmd: null }] as never);
    await openArchived("l1", LOCAL_ORIGIN);
    expect(menuLabels()).toEqual(expect.arrayContaining([RESUME, TMUX, DIRECT]));
    item(TMUX)!.click();
    item(RESUME)!.click();
    await settle();
    expect(vi.mocked(callStart).mock.calls[0].slice(0, 2)).toEqual([LOCAL_ORIGIN, "tmux"]);
  });

  it("那台只有一个可选号：账号组照出（那个号 ＋ 基座）；勾基座再「恢复」⇒ useBase（#75 逃生口不退化）", async () => {
    invalidateAccountsCache();
    accounts("z");
    await openArchived();
    expect(menuLabels()).toEqual(expect.arrayContaining(["z", BASE]));
    expect(item("z")!.getAttribute("role")).toBe("menuitemradio");
    item(BASE)!.click();
    expect(callStart).not.toHaveBeenCalled();
    expect(runRemoteResume).not.toHaveBeenCalled();
    item(RESUME)!.click();
    await settle();
    expect(runRemoteResume).toHaveBeenCalledWith("devbox", "claude", "r1", "/home/pi/proj", "cct", { account: { kind: "base" }, preflight: expect.any(Function) });
    invalidateAccountsCache();
  });

  it("两个号：勾 b ⇒ 真的用 b 起（不是跟随）；勾 z ＋ 在 tmux 里 ⇒ tmux 那条且带 z（账号 × 方式两组正交）", async () => {
    invalidateAccountsCache();
    accounts("z", "b");
    await openArchived();
    item("b")!.click();
    item(RESUME)!.click();
    await settle();
    expect(runRemoteResume).toHaveBeenCalledWith("devbox", "claude", "r1", "/home/pi/proj", "cct", expect.objectContaining({ account: { kind: "named", name: "b" } }));
    rightClick("r1");
    await settle();
    item("z")!.click();
    item(TMUX)!.click();
    item(RESUME)!.click();
    await settle();
    expect(vi.mocked(callStart).mock.calls[0][2]).toEqual([expect.objectContaining({ sid: "r1", account: { kind: "named", name: "z" } })]);
    invalidateAccountsCache();
  });

  // 账号名允许 `__base__`（账号也能在 app 之外写进账号库）：它是一个号，不是基座 —— 基座在类型上是另一支（`name: null`）。
  it("真实账号恰好叫 __base__ ⇒ 当成账号而非基座", async () => {
    invalidateAccountsCache();
    accounts("z", "__base__");
    await openArchived();
    item("__base__")!.click();
    item(RESUME)!.click();
    await settle();
    expect(runRemoteResume).toHaveBeenCalledWith("devbox", "claude", "r1", "/home/pi/proj", "cct", expect.objectContaining({ account: { kind: "named", name: "__base__" } }));
    invalidateAccountsCache();
  });

  it("那台没开多账号 ⇒ 账号组不出", async () => {
    invalidateAccountsCache();
    vi.mocked(invoke).mockImplementation(withHistoryReads(withAccountReads((cmd: string) =>
      cmd === "list_remote_accounts" ? Promise.resolve({ available: false, error: null, meta: null, accounts: [] }) : Promise.resolve(undefined),
    )));
    await openArchived();
    const heads = [...flyout().querySelectorAll('[role="presentation"]')].map((e) => e.textContent);
    expect(heads).toEqual([copyText("resumeMenu.group.run")]);
    invalidateAccountsCache();
  });

  // 用户手快：号还没到就已悬停展开了「恢复 ▸」；号一到整项换掉，flyout 不许无故收起。
  it("号到达前已悬停展开 flyout ⇒ 号到了之后 flyout 仍展开", async () => {
    invalidateAccountsCache();
    vi.useFakeTimers();
    try {
      let resolveAccounts!: (v: unknown) => void;
      const pending = new Promise((r) => (resolveAccounts = r));
      vi.mocked(invoke).mockImplementation(withHistoryReads(withAccountReads((cmd: string) =>
        cmd === "list_remote_accounts" ? pending : Promise.resolve(undefined),
      )));
      tm.ensureTab("r1", "/home/pi/proj", "p", "devbox");
      tm.archiveTab("r1");
      rightClick("r1");
      const resumeWrap = document.body.querySelector<HTMLElement>("[role=menu] > [role=none]");
      expect(resumeWrap).not.toBeNull();
      resumeWrap!.dispatchEvent(new MouseEvent("mouseenter", { bubbles: false }));
      await vi.advanceTimersByTimeAsync(150); // 展开延迟
      expect(resumeWrap!.dataset.subOpen).toBe("true");
      resolveAccounts({
        available: true,
        error: null,
        meta: { enabled: true, acctsDir: "/h", manifestPath: "/h/accounts.json", updatedAt: null, sharedStore: null, count: 1, error: null },
        accounts: [{ name: "z", email: "z@x", configDir: "/h/z", isDefault: true, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true }],
      });
      for (let i = 0; i < 12; i++) await vi.advanceTimersByTimeAsync(0);
      const newWrap = document.body.querySelector<HTMLElement>("[role=menu] > [role=none]");
      expect(newWrap).not.toBe(resumeWrap);
      expect(newWrap!.dataset.subOpen).toBe("true"); // 没有无故收起
    } finally {
      vi.useRealTimers();
      invalidateAccountsCache();
    }
  });

  // tab 栏可拖到 340px 宽，窄窗下 flyout 会溢出右边界：右侧放不下 ⇒ 往左翻。
  it("右侧空间不够时 flyout 往左翻，够用时不翻", async () => {
    invalidateAccountsCache();
    const origInnerWidth = window.innerWidth;
    const origGBCR = HTMLElement.prototype.getBoundingClientRect;
    try {
      Object.defineProperty(window, "innerWidth", { value: 400, configurable: true });
      tm.ensureTab("r1", "/home/pi/proj", "p", "devbox");
      tm.archiveTab("r1");
      rightClick("r1");
      const resumeWrap = document.body.querySelector<HTMLElement>("[role=menu] > [role=none]");
      const resumeBtn = resumeWrap!.querySelector<HTMLButtonElement>(":scope > button")!;
      const fly = resumeWrap!.querySelector<HTMLElement>("[role=menu][data-sub]")!;
      HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
        if (this === resumeWrap) return { left: 200, right: 380, top: 100, bottom: 128 } as DOMRect;
        if (this === fly) return { width: 0 } as DOMRect; // 未展开时宽度未知，兜底成 160
        return origGBCR.call(this);
      };
      resumeBtn.click();
      // 右边 380 + 160 > 400 − 8 ⇒ 翻到那一项左侧：相对那一项的偏移 = −160。
      expect(fly.style.left).toBe("-160px");
      resumeBtn.click();
      HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
        if (this === resumeWrap) return { left: 10, right: 50, top: 100, bottom: 128 } as DOMRect;
        if (this === fly) return { width: 0 } as DOMRect;
        return origGBCR.call(this);
      };
      resumeBtn.click();
      expect(fly.style.left).toBe("40px");
    } finally {
      HTMLElement.prototype.getBoundingClientRect = origGBCR;
      Object.defineProperty(window, "innerWidth", { value: origInnerWidth, configurable: true });
      invalidateAccountsCache();
    }
  });
});

const flushMicro = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

// 〔散文墓碑〕tmux↔sid 那几个前端过滤（findClaudeTmux · findClaudeTmuxMatches · findIdleTmux · isCwdFallbackMatch）与 tmux 名单短缓存的判据删了：
//   「这个 sid 由哪个 tmux 会话在跑」只在后端判（`control/session_batch.rs::standing`，判据住 `session_batch_tests.rs`），按目录猜那一支删了。

describe("auto-e2e F-E4 可注入 confirm seam（killInTmux：交那台 sessions-stop，与批量同一条）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  const microFlush = async (): Promise<void> => {
    await Promise.resolve();
    await Promise.resolve();
  };
  // 单个杀交那台一个 sid 的一批（`sessions-stop`；那台按 sid 认出是哪个、过三道门）⇒ 这里数交了几发、交的是哪个 sid。
  const killCalls = (): unknown[] => vi.mocked(callStop).mock.calls;

  it("killInTmux 默认（不传 opts）→ 弹应用内对话框；答之前不杀，答「取消」⇒ 不杀", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.killInTmux("hostA", "s1", "cc-abc");
    await microFlush();
    expect(askDialogText(), "没弹应用内对话框").toContain(copyText("tabSessionActions.kill.title", { title: "cc-abc" }));
    expect(killCalls(), "还没答就杀了").toHaveLength(0);
    await answerAskDialog(false);
    expect(killCalls()).toHaveLength(0);
    expect(confirmSpy, "还在用原生 window.confirm").not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("killInTmux 注入 confirm=()=>true → 不碰 window.confirm、交那台这一个 sid", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.killInTmux("hostA", "s1", "cc-abc", { confirm: () => true });
    await microFlush();
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(killCalls()).toHaveLength(1);
    expect(killCalls()[0]).toEqual(["hostA", ["s1"]]);
    confirmSpy.mockRestore();
  });

  it("killInTmux 注入 confirm=()=>false → no-op，不 invoke", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.killInTmux("hostA", "s1", "cc-abc", { confirm: () => false });
    await microFlush();
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(killCalls()).toHaveLength(0);
    confirmSpy.mockRestore();
  });

  // UX 审计 #1：灰态(idle-tmux) tab 也能 kill——opts.idle 走"Claude 已退出"文案（非"正在运行"），照常 kill。
  it("killInTmux { idle:true } → 文案说 Claude 已退出、非'正在运行'，仍 kill 空 tmux", async () => {
    const msgs: string[] = [];
    home(tm).actions.killInTmux("hostA", "s1", "cc-idle1234", {
      idle: true,
      confirm: (m) => {
        msgs.push(m.body ?? "");
        return true;
      },
    });
    await microFlush();
    expect(msgs).toHaveLength(1);
    expect(msgs[0]).toContain(copyText("sessionState.reconnectable.name"));
    expect(msgs[0]).not.toContain("正在运行的 Claude");
    expect(killCalls()).toHaveLength(1);
    expect(killCalls()[0]).toEqual(["hostA", ["s1"]]);
  });

  // 护栏：live（非 idle）的确认框逐项写会中断什么（那个 tmux 会话里的 Claude · 当前轮次）与保留什么。
  it("killInTmux 非 idle → 中断那一项点名 tmux 会话里的 Claude 与当前轮次；保留会话记录（live 路径护栏）", async () => {
    const rows: { label: string; items: string[] }[][] = [];
    home(tm).actions.killInTmux("hostA", "s1", "cc-live1234", {
      title: "表格分页",
      confirm: (m) => {
        rows.push(m.rows ?? []);
        return false;
      },
    });
    await microFlush();
    expect(rows).toEqual([
      [
        { label: copyText("kit.interrupts.cut"), items: [copyText("tabSessionActions.kill.cuts", { machine: "hostA", name: "cc-live1234" })] },
        { label: copyText("kit.interrupts.keep"), items: [copyText("tabSessionActions.kill.keeps")] },
      ],
    ]);
  });
});

describe("F79 杀死会话（二次确认 ＋ 交那台 sessions-stop）", () => {
  beforeEach(() => vi.clearAllMocks());
  it("二次确认通过 → 交那台这一个 sid（变已结束由会话流兜、不主动 archive）；杀成了说杀的是哪个", async () => {
    vi.mocked(callStop).mockResolvedValue([{ sid: "s1", outcome: "done", why: null, detail: "", session: "cc-abc", bus: { removed: [], failed: [], unread: null }, cmd: null }] as never);
    const tm = home(makeTM()).actions;
    tm.killInTmux("hostA", "s1", "cc-abc");
    await answerAskDialog(true);
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(callStop).mock.calls).toEqual([["hostA", ["s1"]]]);
    expect(vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[0]).toBe(copyText("sessionState.kill.done", { title: "cc-abc" }));
  });
  it("二次确认取消 → 不交", async () => {
    const tm = home(makeTM()).actions;
    tm.killInTmux("hostA", "s1", "cc-abc");
    await answerAskDialog(false);
    expect(callStop).not.toHaveBeenCalled();
  });
  it("那台没杀（关卡拒 / 不在 tmux 里）⇒ 说那台的原因；关卡 2 拒的 ⇒ 提示带「对齐后重试」", async () => {
    vi.mocked(callStop).mockResolvedValue([{ sid: "s1", outcome: "failed", why: "wrong_owner", detail: "x", session: "cc-abc", bus: null, cmd: null }] as never);
    const tm = home(makeTM()).actions;
    tm.killInTmux("hostA", "s1", "cc-abc", { confirm: () => true });
    await new Promise((r) => setTimeout(r, 0));
    const [title, body] = vi.mocked(showActionFailureToast).mock.calls.at(-1)!;
    expect(title).toBe(copyText("tabSessionActions.kill.failed", { title: "cc-abc" }));
    expect(body).toContain(copyText("resync.retry.hint"));
  });
});

// 「F70 会话改动集聚合」那一组搬进文件末尾「会话事实」那组：改动文件集由后端出成品（口径 · 去重 · 近因序
//   住 `tests/backend/observe/facts_query_tests.rs`），前端这边只剩「成品 ⇒ 监控板 peek 的透传」。

describe("F91b TabManager.peekSession（监控板内容 peek 纯读派生）", () => {

  it("unknown sid → null", () => {
    const tm = makeTM();
    expect(tm.peekSession("nope")).toBeNull();
  });

  it("运行中 subagent 排在前，同档保插入序；model / 改过的文件透传", () => {
    const tm = makeTM();
    const tab = tm.ensureTab("s1", "/proj", "/p/s1.jsonl", LOCAL_ORIGIN);
    tab.latestModel = "claude-opus-4-8";
    tab.touchedFiles.add("/proj/a.ts");
    tab.touchedFiles.add("/proj/b.ts");
    // 运行表（后端的成品）序：done, running, stopped, running —— 期望 running 提前、组内保表序
    tm.onSessionRuns({
      session_id: "s1",
      runs: [
        { run: "1", label: "done1", state: "done" },
        { run: "2", label: "run1", state: "running" },
        { run: "3", label: "stop1", state: "stopped" },
        { run: "4", label: "run2", state: "running" },
      ],
      ended: [],
    });

    const p = tm.peekSession("s1");
    expect(p).not.toBeNull();
    expect(p!.model).toBe("claude-opus-4-8");
    expect(p!.recentFiles).toEqual(["/proj/a.ts", "/proj/b.ts"]);
    // running 全部提前且组内保序；非 running 组内也保插入序
    expect(p!.agents.map((a) => a.label)).toEqual(["run1", "run2", "done1", "stop1"]);
    expect(p!.agents.map((a) => a.status)).toEqual(["running", "running", "done", "stopped"]);
  });

  it("无 usage / 无 agent / 无改文件 → 字段空但不报错", () => {
    const tm = makeTM();
    tm.ensureTab("s2", "/x", "/p/s2.jsonl", LOCAL_ORIGIN);
    const p = tm.peekSession("s2");
    expect(p).toEqual({ model: null, recentFiles: [], agents: [] });
  });

  // F91b-fix(batch18)：touchedFiles 近因序（peek `recentFiles` 尾部 = 最近改的）。近因序今天由后端排
  //   （`facts_query_tests.rs::the_three_facts_follow_the_moved_rules`），前端只保序透传 —— 那一条在文件末尾「〔STC〕会话事实」那组。
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
    tm.setSessionAccounts(rows, new Map(), last, new Set(["devbox"]), current);
  }

  // 「账号快照变了」改订阅 store：宿主整份换进来，徽章**同一拍**就换（与原先宿主直调 `setSessionAccounts` 同时机）。
  it("★ 〔GAP1〕账号快照经 store 换进来 ⇒ 同一拍徽章就挂上（不等任何一拍）", () => {
    appStore.sessionAccounts.__resetForTests(null); // 别的判据留下的 TabManager 不再订阅
    tm = makeTM();
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", "devbox");
    expect(badge()?.querySelector(".acct-avatar") ?? null).toBeNull();
    appStore.sessionAccounts.set({
      rows: [liveRow("r1", "b")],
      emailByName: new Map(),
      lastByS: new Map(),
      readyOrigins: new Set(["devbox"]),
      currentByOrigin: new Map([["devbox", "z"]]),
    });
    expect(badge()?.querySelector(".acct-avatar"), "store 换了快照，徽章没跟上").not.toBeNull();
  });
  it("会话账号 != 当前账号(live) → 挂实心头像", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", "devbox");
    feed([liveRow("r1", "b")], new Map(), new Map([["devbox", "z"]]));
    const el = badge();
    expect(el?.style.display).not.toBe("none");
    expect(el?.querySelector(".acct-avatar")).not.toBeNull();
    expect(el?.querySelector(".acct-avatar.ghost")).toBeNull(); // live = 实心
  });
  // 头像**只在这个会话的号 ≠ 那台默认号时出**——全都出 ⇒ 每行一块、信息为零。
  // 悬停卡里永远写账号（下一条）；头像悬停说「账号 {name}」。
  it("会话账号 == 默认账号 → 不挂头像；悬停卡里照样写账号", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", "devbox");
    feed([liveRow("r1", "z")], new Map(), new Map([["devbox", "z"]]));
    expect(badge()?.style.display).toBe("none");
    expect(cardText(tm, "r1")).toContain(copyText("tabBar.hover.account", { name: "z" }));
  });
  it("lastAccount 软来源且 != 当前 → 幽灵头像", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", "devbox");
    feed([], new Map([["r1", "b"]]), new Map([["devbox", "z"]]));
    const av = badge()?.querySelector(".acct-avatar");
    expect(av).not.toBeNull();
    expect(av?.classList.contains("ghost")).toBe(true);
  });
  it("未知账号(无 live 无 last) → 不挂徽章", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", "devbox");
    feed([], new Map(), new Map([["devbox", "z"]]));
    expect(badge()?.style.display).toBe("none");
  });
  it("默认账号未就绪(currentByOrigin 无该 origin) → 不挂头像（说不出「不同」，不猜）；悬停卡里写账号", () => {
    tm.ensureTab("r1", "/w", "/p/r1.jsonl", "devbox");
    feed([liveRow("r1", "b")], new Map(), new Map()); // 无 current
    expect(badge()?.style.display).toBe("none");
    expect(cardText(tm, "r1")).toContain(copyText("tabBar.hover.account", { name: "b" }));
  });

});

/**
 * ★★ ↗ 远端那一格：点那一刻按顺序问三方，**前端不解析、不猜，tmux 不在前提链上**。
 * ① 问会话所在那台 `session-terminals`；② 那台回话里的 `terminals` **原样**交本机后端 `terminal-processes`；
 * ③ 本机后端的 `chain` **原样**交壳拉前。每一方的结局都落成一个结局族，画成锚在 ↗ 下面的浮层（不出 toast）；
 * 切过去了什么都不出、↗ 换成对勾。
 */
describe("：↗ 远端那一格按顺序问三方", () => {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
  const TERMINALS = [
    { ssh: { clientAddr: "192.0.2.5", clientPort: 62414, serverAddr: "192.0.2.9", serverPort: 22 }, activity: 9 },
  ];
  const CHAIN = [{ pid: 700, name: "ssh.exe", start: 4000 }];
  /** 那台 / 本机各回什么；壳那一跳回什么结局。 */
  /** `front` 答交进程链那一问；`label` 答交 `terminals` 那一问（按窗口标签找，缺 ⇒ 没对上 `null`）。 */
  function answer(shown: unknown, found: unknown, front: () => Promise<unknown>, label: () => Promise<unknown> = () => Promise.resolve(null)): void {
    mockInvoke.mockImplementation((cmd: string, args: unknown) => {
      if (isChanCall(cmd, args, "session-terminals")) return Promise.resolve(chanReply(shown));
      if (isChanCall(cmd, args, "terminal-processes")) return Promise.resolve(chanReply(found));
      if (cmd === "bring_remote_terminal_to_front") return "terminals" in (args as object) ? label() : front();
      return Promise.resolve([]);
    });
  }
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    __setHostOsForTests("windows");
    document.querySelectorAll("[data-role=front-result]").forEach((e) => e.closest("[role=dialog]")?.remove());
  });
  afterEach(() => __setHostOsForTests(null));

  const frontBtn = (tm: TabManager, sid: string): HTMLElement | null =>
    (home(tm).bar.tabButtons.get(sid)?.root.querySelector(".tab-focus") as HTMLElement | null) ?? null;
  async function clickFront(tm: TabManager, sid: string): Promise<void> {
    const btn = frontBtn(tm, sid);
    expect(btn, "量具自检：Windows 上应当渲出 ↗").not.toBeNull();
    btn!.click();
    for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
  }
  const pop = (): HTMLElement | null => document.querySelector<HTMLElement>("[data-role=front-result]");
  const popText = (): string => pop()?.textContent ?? "";
  const popButtons = (): string[] => [...(pop()?.querySelectorAll("button") ?? [])].map((b) => b.textContent ?? "").filter((t) => t !== "");

  it("★★ 两跳都是原样交：那台的 `terminals` ⇒ 本机后端，本机后端的 `chain` ⇒ 壳；分不清 ⇒ 浮层照实说拉不了、带候选个数（不切不闪），一次 tmux 都不问", async () => {
    answer({ terminals: TERMINALS }, { chain: CHAIN }, () => Promise.resolve({ kind: "several", program: "WindowsTerminal.exe", count: 2 }));
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "devbox", false, null);
    await clickFront(tm, "r1");
    const calls = mockInvoke.mock.calls as [string, unknown][];
    const asked = calls.filter(([c, a]) => isChanCall(c, a, "session-terminals"));
    expect(asked.map(([, a]) => [(a as { origin: string }).origin, chanArgsJson(a as never)])).toEqual([
      ["devbox", { sid: "r1" }],
    ]);
    const local = calls.filter(([c, a]) => isChanCall(c, a, "terminal-processes"));
    expect(local.map(([, a]) => chanArgsJson(a as never))).toEqual([{ terminals: TERMINALS }]);
    // 先把那台的 `terminals` 原样交 monitor 按窗口标签找（没对上），再交本机后端的 `chain`。
    expect(calls.filter(([c]) => c === "bring_remote_terminal_to_front").map(([, a]) => a)).toEqual([
      { terminals: TERMINALS },
      { chain: CHAIN },
    ]);
    expect(
      calls.filter(([c, a]) => c === "list_remote_tmux" || c === "list_local_tmux" || isChanCall(c, a, "terminals-list")),
      "↗ 又去查了 tmux —— tmux 回到了 ↗ 的前提链上",
    ).toEqual([]);
    expect(showActionFailureToast, "结局画在浮层上，不出 toast").not.toHaveBeenCalled();
    expect(pop()?.dataset.shade).toBe("amber");
    expect(popText()).toContain(copyText("front.title.unsure"));
    expect(popText()).toContain(copyText("front.body.several", { program: "WindowsTerminal.exe", n: 2 }));
    expect(popText()).toContain(copyText("front.hint.several"));
    expect(pop()!.closest("[role=dialog]")?.getAttribute("aria-label")).toBe(copyText("front.pop.aria"));
    expect(popButtons(), "分不清：不给闪窗口那一颗").toEqual([]);
  });

  it("★ 每一方的原因各落成一族（标题 · 正文 · 按钮 · 色）；本机后端说了原因就不去找窗口；同一个会话再来一次原地换内容", async () => {
    const cases: [unknown, unknown, string, string, string[], string][] = [
      [{ terminals: [], why: "detached" }, null, copyText("front.title.noTerminal"), copyText("front.body.detached"), [copyText("front.act.openInTerminal")], "grey"],
      [{ terminals: [], why: "no-terminal" }, null, copyText("front.title.noTerminal"), copyText("front.body.background"), [], "grey"],
      [{ terminals: [], why: "unreadable" }, null, copyText("front.title.unsure"), copyText("front.body.unreadable", { machine: "devbox" }), [copyText("detail.act.copy")], "grey"],
      [{ terminals: TERMINALS }, { chain: [], why: "not-ssh" }, copyText("front.title.elsewhere"), copyText("front.body.notSsh", { machine: "devbox" }), [], "grey"],
      [{ terminals: TERMINALS }, { chain: [], why: "elsewhere", addr: "203.0.113.8" }, copyText("front.title.elsewhere"), copyText("front.body.elsewhere", { addr: "203.0.113.8" }), [], "grey"],
      [{ terminals: TERMINALS }, { chain: [], why: "mismatch" }, copyText("front.title.unsure"), copyText("front.body.mismatch"), [], "grey"],
      [{ terminals: TERMINALS }, { chain: [], why: "query-failed" }, copyText("front.title.unknown"), "", [copyText("front.act.retry"), copyText("detail.act.copy")], "red"],
      [{ terminals: [], why: "<新原因>" }, null, copyText("front.title.badShape"), "", [copyText("detail.act.copy")], "red"],
    ];
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "devbox", false, null);
    let first: Element | null = null;
    for (const [shown, found, title, body, buttons, tone] of cases) {
      mockInvoke.mockReset();
      answer(shown, found, () => Promise.resolve({ kind: "switched" }));
      await clickFront(tm, "r1");
      const calls = mockInvoke.mock.calls as [string, unknown][];
      expect(calls.some(([c, a]) => c === "bring_remote_terminal_to_front" && "chain" in (a as object)), `${title}：说了原因还去找了窗口`).toBe(false);
      expect(calls.some(([c, a]) => isChanCall(c, a, "terminal-processes")), `${title}：本机后端问没问`).toBe(found !== null);
      expect(document.querySelectorAll("[data-role=front-result]").length, `${title}：同一个会话不另弹`).toBe(1);
      first ??= pop()!.closest("[role=dialog]");
      expect(pop()!.closest("[role=dialog]"), `${title}：原地换内容（同一个浮层，不关了重开）`).toBe(first);
      expect(pop()!.firstElementChild!.textContent, title).toBe(title);
      if (body) expect(popText(), title).toContain(body);
      expect(popButtons(), title).toEqual(buttons);
      expect(pop()!.dataset.shade, title).toBe(tone);
    }
    expect(showActionFailureToast).not.toHaveBeenCalled();
  });

  it("★ 按窗口标签对上了（monitor 回那一次的结局）⇒ 不再问本机后端、不另交进程链；切过去了 ↗ 换对勾", async () => {
    const LABELED = [{ ...TERMINALS[0], window: "600-639150434950992340" }];
    answer({ terminals: LABELED }, { chain: CHAIN }, () => Promise.resolve({ kind: "switched" }), () => Promise.resolve({ kind: "switched" }));
    const tm = makeTM();
    tm.createSkeletonTab("r3", "/p", "devbox", false, null);
    await clickFront(tm, "r3");
    const calls = mockInvoke.mock.calls as [string, unknown][];
    expect(calls.filter(([c]) => c === "bring_remote_terminal_to_front").map(([, a]) => a)).toEqual([{ terminals: LABELED }]);
    expect(calls.some(([c, a]) => isChanCall(c, a, "terminal-processes"))).toBe(false);
    expect(pop(), "切过去了：不出浮层").toBeNull();
  });

  it("★ 按窗口标签没对上（monitor 回 null）⇒ 接着问本机后端、交进程链", async () => {
    answer({ terminals: TERMINALS }, { chain: CHAIN }, () => Promise.resolve({ kind: "switched" }));
    const tm = makeTM();
    tm.createSkeletonTab("r4", "/p", "devbox", false, null);
    await clickFront(tm, "r4");
    const calls = mockInvoke.mock.calls as [string, unknown][];
    expect(calls.filter(([c]) => c === "bring_remote_terminal_to_front").map(([, a]) => a)).toEqual([{ terminals: TERMINALS }, { chain: CHAIN }]);
  });

  it("★ 切过去了：什么都不出、↗ 换成对勾 1 秒；开着的结局浮层一起收起", async () => {
    answer({ terminals: [], why: "no-terminal" }, null, () => Promise.resolve({ kind: "switched" }));
    const tm = makeTM();
    tm.createSkeletonTab("r2", "/p", "devbox", false, null, false);
    expect(tm.isAttachable("r2"), "量具自检：这个会话确实被宣告成不可 attach").toBe(false);
    await clickFront(tm, "r2");
    expect(pop(), "量具自检：先有一个结局浮层").not.toBeNull();
    mockInvoke.mockReset();
    answer({ terminals: TERMINALS }, { chain: CHAIN }, () => Promise.resolve({ kind: "switched" }));
    await clickFront(tm, "r2");
    expect(mockInvoke.mock.calls.map((c) => c[0]), "`attachable:false` 不在前端短路 ↗").toContain("bring_remote_terminal_to_front");
    expect(pop(), "切过去了 ⇒ 浮层收起").toBeNull();
    expect(showActionFailureToast, "成功了还弹了 toast").not.toHaveBeenCalled();
    expect(frontBtn(tm, "r2")!.querySelector("svg")?.getAttribute("data-icon"), "↗ 换成对勾").toBe("check");
  });

  it("★ 在飞超过 300ms：↗ 旁出「查找终端…」，同一个会话再点不重发；回来了就收起", async () => {
    let done: (v: unknown) => void = () => {};
    mockInvoke.mockImplementation((cmd: string) => (cmd === "bring_terminal_to_front" ? new Promise((r) => (done = r)) : Promise.resolve([])));
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL_ORIGIN);
    const btn = frontBtn(tm, "l1")!;
    btn.click();
    btn.click();
    await new Promise((r) => setTimeout(r, 120));
    expect(document.querySelector("[data-role=front-busy]"), "300ms 之内不出（防闪）").toBeNull();
    await new Promise((r) => setTimeout(r, 300));
    expect(document.querySelector("[data-role=front-busy]")?.textContent).toBe(copyText("front.pending.label"));
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "bring_terminal_to_front").length, "在飞时再点不重发").toBe(1);
    done({ kind: "switched" });
    await new Promise((r) => setTimeout(r, 500));
    expect(document.querySelector("[data-role=front-busy]"), "回来了就收起").toBeNull();
  });

  it("★ 通道那一跳：那台旧 ⇒「要更新」＋［更新］；期限到 ⇒「无应答」＋［重试］；连不上 ⇒「离线」＋［重新连接］", async () => {
    const tm = makeTM();
    tm.createSkeletonTab("r3", "/p", "devbox", false, null);
    // 壳交回来的失败形状（`wire::WireErr` 的线上形状，`chan.ts::decodeFail` 收）。
    const hop = (why: "Overrun" | "Unreachable") => ({ err: { Hop: { idx: 0, tag: "wait", reach: "Sent", why } }, body: [] });
    const cases: [unknown, string, string, string[]][] = [
      [UNSUPPORTED, copyText("front.title.update", { machine: "devbox" }), copyText("front.body.update"), [copyText("front.act.update")]],
      [hop("Overrun"), copyText("front.title.failed"), copyText("front.body.timeout", { machine: "devbox" }), [copyText("front.act.retry")]],
      [hop("Unreachable"), copyText("front.title.failed"), copyText("front.body.offline", { machine: "devbox" }), [copyText("front.act.reconnect")]],
    ];
    for (const [err, title, body, buttons] of cases) {
      mockInvoke.mockReset();
      mockInvoke.mockImplementation((cmd: string, args: unknown) => (isChanCall(cmd, args, "session-terminals") ? Promise.reject(err) : Promise.resolve([])));
      await clickFront(tm, "r3");
      expect(pop()!.firstElementChild!.textContent, title).toBe(title);
      expect(popText(), title).toContain(body);
      expect(popButtons(), title).toEqual(buttons);
    }
  });

  it("★ ［更新］就地做：不开设置，经共用部署模块换上这一版；过程与结局在同一个浮层里说", async () => {
    const tm = makeTM();
    tm.createSkeletonTab("r5", "/p", "devbox", false, null);
    let finish: (v: string | null) => void = () => {};
    vi.mocked(updateBackendOf).mockImplementation((_o, _m, onStart) => {
      onStart?.();
      return new Promise((r) => (finish = r));
    });
    const clickUpdate = async (): Promise<void> => {
      mockInvoke.mockReset();
      mockInvoke.mockImplementation((cmd: string, args: unknown) => (isChanCall(cmd, args, "session-terminals") ? Promise.reject(UNSUPPORTED) : Promise.resolve([])));
      await clickFront(tm, "r5");
      const btn = [...pop()!.querySelectorAll("button")].find((b) => b.textContent === copyText("front.act.update"));
      expect(btn, "量具自检：浮层上有［更新］").toBeDefined();
      btn!.click();
      for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    };
    await clickUpdate();
    expect(vi.mocked(updateBackendOf).mock.calls.map((c) => [c[0], c[1]])).toEqual([["devbox", "devbox"]]);
    expect("onUpdateMachine" in tm, "［更新］还留着交给主窗口开设置窗的钩子").toBe(false);
    expect(pop()!.firstElementChild!.textContent, "在飞：浮层说正在更新").toBe(copyText("front.title.updating", { machine: "devbox" }));
    expect(popButtons()).toEqual([]);
    finish("已部署 X");
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(pop()!.firstElementChild!.textContent).toBe(copyText("front.title.updated", { machine: "devbox" }));
    expect(popText()).toContain("已部署 X");
    expect(popButtons(), "换上了：只留关闭，不挂［重试］").toEqual([]);
    expect(popText(), "重拨成了：不叫人去重连").not.toContain(copyText("front.body.redialFailed"));
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "backend_start").map((c) => c[1]), "换上了就重拨那台").toEqual([{ origin: "devbox" }]);

    // 换上了、重拨没成：才说要重连，给［重新连接］。
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockResolvedValue("已部署 Y");
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string, args: unknown) =>
      isChanCall(cmd, args, "session-terminals") ? Promise.reject(UNSUPPORTED) : cmd === "backend_start" ? Promise.reject(new Error("拨不通")) : Promise.resolve([]),
    );
    await clickFront(tm, "r5");
    [...pop()!.querySelectorAll("button")].find((b) => b.textContent === copyText("front.act.update"))!.click();
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
    expect(pop()!.firstElementChild!.textContent).toBe(copyText("front.title.updated", { machine: "devbox" }));
    expect(popText()).toContain(copyText("front.body.redialFailed"));
    expect(popButtons()).toEqual([copyText("front.act.reconnect")]);

    // 部署失败：同一个浮层里红着说；壳带回的复制详情挂在［复制详情］上（复制的是那份详情，不是那一句）；［重试］再更新一次。
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockRejectedValue(Object.assign(new Error("上传失败"), { detail: "码：refused" }));
    await clickUpdate();
    expect(pop()!.firstElementChild!.textContent).toBe(copyText("front.title.updateFailed", { machine: "devbox" }));
    expect(pop()!.dataset.shade).toBe("red");
    expect(popText()).toContain("上传失败");
    expect(popButtons()).toEqual([copyText("front.act.retry"), copyText("detail.act.copy")]);
    const copied = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText: copied }, configurable: true });
    [...pop()!.querySelectorAll("button")].find((b) => b.textContent === copyText("detail.act.copy"))!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(copied.mock.calls.map((c) => (c as unknown[])[0])).toEqual([`${copyText("front.title.updateFailed", { machine: "devbox" })}\n码：refused`]);
    [...pop()!.querySelectorAll("button")].find((b) => b.textContent === copyText("front.act.retry"))!.click();
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(updateBackendOf), "失败态的［重试］是再更新一次").toHaveBeenCalledTimes(2);

    // 会打断什么的确认框里点了取消：浮层收起，什么都不做。
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockResolvedValue(null);
    await clickUpdate();
    expect(pop(), "取消了：浮层不再出").toBeNull();
  });
});

/**
 * ★★ ↗ 在非 Windows 上**别装得能用**。
 *
 * 非 Windows 上 ↗ 的最后一跳（`EnumWindows` / `SetForegroundWindow`）在 Rust 侧是恒失败的桩
 * ⇒ 那颗按钮每点必败。门住 `terminal-front.ts`；`unknown` 照常显示（与 `hostOsAllows` 同一条理由）。
 */
describe("过程里还没结果的那几步照会话事实画（后端 pending[].state · needs.call）", () => {
  it("★ 事实说 b1 在等批准 ⇒ 等你批准；b2 在跑 ⇒ 在跑；换过来照换；事实里已经没有的那一步 ⇒ 状态不明；结果到了的那一步不动", () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL_ORIGIN);
    const tab = home(tm).store.tabs.get("l1")!;
    const step = (id: string): HTMLElement => {
      const line = buildStepLine("Bash", { tool: "Bash", arg: id, known: true } as never, "", id);
      tab.streamEl.appendChild(line);
      return line;
    };
    const [b1, b2] = [step("b1"), step("b2")];
    expect([b1.dataset.state, b2.dataset.state], "事实到之前不画").toEqual(["pending", "pending"]);
    const call = (id: string, state: "running" | "awaiting" | "unclear", why: "noWriter" | "untracked" | null = null) => ({ id, name: "Bash", what: id, at: null, state, why });
    const paint = (pending: Tab["pending"], needs: Tab["needs"]): void => {
      tab.pending = pending;
      tab.needs = needs;
      (tm as unknown as { paintStepWaits(t: Tab): void }).paintStepWaits(tab);
    };
    const approve = (c: string) => ({ kind: "approve" as const, tool: "Bash", call: c, what: c, sinceMs: Date.now() - 125_000 });
    paint([call("b1", "awaiting"), call("b2", "running")], approve("b1"));
    expect([b1.dataset.state, b2.dataset.state]).toEqual(["awaiting", "running"]);
    expect(b1.querySelector(".step-right")?.textContent, "右侧已等多久").toBe("2m05s");
    paint([call("b1", "running"), call("b2", "awaiting")], approve("b2"));
    expect([b1.dataset.state, b2.dataset.state]).toEqual(["running", "awaiting"]);
    paint([call("b2", "unclear", "noWriter")], null);
    expect([b1.dataset.state, b2.dataset.state]).toEqual(["unclear", "unclear"]);
    expect(b2.querySelector<HTMLElement>(".step-right")?.title, "原因是后端给的码").toBe(copyText("stream.step.unclearNoWriter"));
    settleStepLine(b1, undefined, { ok: true } as never, false, 10);
    paint([call("b1", "running")], null);
    expect(b1.dataset.state, "结果到了的不动").toBe("ok");
  });
});

describe("agent 来话的事件条：标签按运行表查（流视图把运行表交给渲染）", () => {
  it("★ 运行表里有那个运行 ⇒ 事件条用表里的标签，不用来话自带的名字", () => {
    const tm = makeTM();
    tm.ensureTab("l3", "/w", "p", LOCAL_ORIGIN);
    tm.switchTo("l3");
    tm.onSessionRuns({ session_id: "l3", runs: [{ run: "a7", label: "审面板交互" }], ended: [] } as never);
    const label = (tm as unknown as { view: { host: { runLabelOf(sid: string, run: string): string | undefined } } }).view.host.runLabelOf;
    expect(label("l3", "a7")).toBe("审面板交互");
    expect(label("l3", "zz"), "表里没有 ⇒ 不给，用来话自带的").toBeUndefined();
  });
});

describe("新出现的一行淡入（起步那一批不算）", () => {
  it("第一批画出去之前建的行不带淡入；之后再长出来的带；动画放完就摘", () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL_ORIGIN);
    const row = (sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root;
    expect(row("l1").classList.contains("tab-enter"), "起步那一行不淡入").toBe(false);
    tm.ensureTab("l2", "/w2", "q", LOCAL_ORIGIN);
    expect(row("l2").classList.contains("tab-enter"), "之后新出现的一行淡入").toBe(true);
    row("l2").dispatchEvent(new Event("animationend"));
    expect(row("l2").classList.contains("tab-enter")).toBe(false);
  });
});

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
      tm.ensureTab("l1", "/w", "p", LOCAL_ORIGIN);
      tm.createSkeletonTab("r1", "/p", "devbox", false, null);
      expect(home(tm).bar.tabButtons.get("l1"), `${os}：量具自检，tab 按钮本身得在`).toBeDefined();
      expect(focusBtnOf(tm, "l1") !== null, `${os} 本地 tab 的 ↗`).toBe(shown);
      expect(focusBtnOf(tm, "r1") !== null, `${os} 远端 tab 的 ↗`).toBe(shown);
    }
  });

  it("★ 快捷键 / 命令面板在 linux 上走到 ↗ ⇒ 说实话、**不发 IPC**", async () => {
    __setHostOsForTests("linux");
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "devbox", false, null);
    tm.switchTo("r1");
    tm.bringActiveTerminalToFront();
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    const sent = (mockInvoke.mock.calls as [string, unknown][])
      .filter(
        ([c, a]) =>
          c === "bring_remote_terminal_to_front" || c === "bring_terminal_to_front" || isChanCall(c, a, "session-terminals"),
      )
      .map(([c]) => c);
    expect(sent, "linux 上照样发了 ↗ 的 IPC —— 那是装作试过").toEqual([]);
    expect(showActionFailureToast).toHaveBeenCalledWith(
      copyText("terminalFront.unavailable.title"),
      expect.stringContaining("Windows"),
      expect.objectContaining({ level: "info" }),
    );
  });

  it("★ 对照：windows 上快捷键照常发 IPC（上一条不是因为别的原因没发）", async () => {
    __setHostOsForTests("windows");
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "devbox", false, null);
    tm.switchTo("r1");
    tm.bringActiveTerminalToFront();
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect((mockInvoke.mock.calls as [string, unknown][]).some(([c, a]) => isChanCall(c, a, "session-terminals"))).toBe(true);
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
    tm.createSkeletonTab("s1", "/p", "devbox", false, null);
    expect(tm.isAttachable("s1")).toBe(true);
    // 连 tab 都还没有的 sid 也按可以算（不知道 ≠ 不可以）
    expect(tm.isAttachable("从没见过")).toBe(true);
  });

  it("★★ 显式 false → 记账；再宣告成 true / 缺席 → 撤销（不许粘住）", () => {
    const tm = makeTM();
    tm.createSkeletonTab("s1", "/p", "devbox", false, null, false);
    expect(tm.isAttachable("s1")).toBe(false);
    // 同一个 sid 后来被宣告成可以（比如 bridge 退出、真人接管）——不能一直挂着
    tm.createSkeletonTab("s1", "/p", "devbox", false, null, null);
    expect(tm.isAttachable("s1")).toBe(true);
  });

  it("★ 只认布尔 false，不认别的假值形态", () => {
    const tm = makeTM();
    tm.createSkeletonTab("s1", "/p", "devbox", false, null, true);
    expect(tm.isAttachable("s1")).toBe(true);
  });
});

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
    tm.ensureTab("s1", "/w", "/w/s1.jsonl", LOCAL_ORIGIN);
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
    tm.ensureTab("front", "/w", "/w/front.jsonl", LOCAL_ORIGIN);
    tm.ensureTab("bg", "/w", "/w/bg.jsonl", LOCAL_ORIGIN);
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

// ===== 已结束的 tab 留在原位（原「P7a-1（#61）独立归档区」，抽屉已删）=====
//
// `#61` 正文自陈「状态机已经有了，缺的是那个「口」」。判据钉的是**分流**本身：
// 主栏里没有它 **且** 抽屉里有它 —— 两面都钉，否则「两边各渲一份」也能过。

describe("已结束的 tab 留在原位灰着（原「P7a-1 独立归档区」）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });




  it("★ 已结束的 tab 留在主栏，一个都不许消失（连 barEl 没有父节点时也一样）", () => {
    // 🔴 **本条留着，但它买的东西变了 —— 从「边界情形」变成「正题」。**
    //
    // 旧理由：抽屉是 `barEl` 的兄弟，拿不到父节点时若仍把 tab 挪进那个**孤儿**容器，
    // 它就从文档里整个不见了。那时这是一条**边界**判据（只在拿不到父节点时有意义）。
    //
    // 今天抽屉整个删了（用户逐字「没有归档这个东西……就是灰 tab」）
    // ⇒ 「已结束的 tab 留在主栏灰着」**成了正常行为本身**，这一条于是是它的正面断言。
    // ⚠ 孤儿 `barEl` 那个布景**刻意留着**：它仍是一个真的边界情形，而且这条断言
    //   在那个布景下也该成立 —— 顺手把两件事一起买了。
    // ⚠ 「灰着但还在」这句话来自 `tabs.ts` 自己的旧头注，而用户 2026-09-19 逐字说了同一句。
    document.body.innerHTML = "";
    const orphanBar = document.createElement("div"); // 刻意不 append 到 body
    const streamRootEl = document.createElement("div");
    document.body.append(streamRootEl);
    const tm2 = new TabManager(orphanBar, streamRootEl);
    tm2.ensureTab("a", "/c", "p", LOCAL_ORIGIN);
    tm2.ensureTab("b", "/c", "p", LOCAL_ORIGIN);
    tm2.switchTo("a");
    tm2.archiveTab("b");
    (tm2 as unknown as { refreshTabBar: () => void }).refreshTabBar();
    const inBar = [...(orphanBar.querySelector(".tab-list") ?? orphanBar).children].filter((e) => e.classList.contains("tab"));
    expect(inBar, "两个都该还在主栏 —— 一个都不许被挪进孤儿容器").toHaveLength(2);
  });

});

// ===== P7a-2（#61）：栏内拖动排序 =====
//
// ★ 摸底订正过措辞：tear-off 的 arm 条件是 `e.clientX > barRight + 16`，
// 而那是一条**竖栏** ⇒ 空闲的不是「横向」，是 **`clientY` 从没被用过**。
// 重排走纵向，与撕离天然不争同一根轴。

describe("P7a-2 moveTab（纯）", () => {
  it("★ P7a2-Y1：搬到某个 sid 之前 / 末尾，落位逐项对得上", () => {
    const o = ["a", "b", "c", "d"];
    expect(moveTab(o, "c", "a")).toEqual(["c", "a", "b", "d"]);
    expect(moveTab(o, "a", "d")).toEqual(["b", "c", "a", "d"]);
    expect(moveTab(o, "a", null)).toEqual(["b", "c", "d", "a"]);
    // 原先这里还有一格「多元素的块保持内部相对序」—— 块随 bg 树一起删了。
    // 不改原数组。
    expect(o).toEqual(["a", "b", "c", "d"]);
  });

  it("★ P7a2-Y1b：落点就是它自己 ⇒ **原样返回**（拖到自己身上不是一次重排）", () => {
    const o = ["a", "b", "c"];
    expect(moveTab(o, "b", "b")).toEqual(o);
    // 把它算成「挪到末尾」是错的 —— 那会让一次误触把 tab 甩到最后。
    expect(moveTab(o, "b", "b")).not.toEqual(["a", "c", "b"]);
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
    const kids = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    kids.forEach((el, i) => {
      el.getBoundingClientRect = () =>
        ({ top: i * 40, height: 40, bottom: i * 40 + 40, left: 0, right: 100 }) as DOMRect;
    });
  };
  const dragTo = (sid: string, clientY: number, clientX = 10): void => {
    // ⚠ tab 根元素上**没有** sid 属性（实测：`createTabButton` 只设 class）。
    // 主栏里的 DOM 顺序 == `orderedIds` 里主栏那部分的顺序（`refreshTabBar` 保证），
    // ⇒ 按下标取，别按文本猜。第一版按 `title.includes(sid)` 找，恒取到第一个 ⇒ 两条判据假红。
    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
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
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", LOCAL_ORIGIN);
    flushBar();
    stubRects();
    expect(order()).toEqual(["a", "b", "c"]);
    // 拖第三个（c）到最上面：clientY=10 落在第一条（0..40）的上半 ⇒ 插到 a 之前。
    dragTo("c", 10);
    expect(order()).toEqual(["c", "a", "b"]);
  });

  // 〔「删掉树」〕原「★ P7a2-Y3：拖交互 tab 时它的 bg 子串跟着走」一格删了 ——
  //   现在反过来钉：拖同 cwd 的宿主**只有它自己动**（本文件末尾「〔BG1〕」那组的 ②，两种 tab 各跑）。

  it("★ P7a2-D：拖动时**落点看得见**，撕离那一路不指示，拖完必须清掉", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", LOCAL_ORIGIN);
    flushBar();
    stubRects();
    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
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
    tm.ensureTab("a", "/c", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    flushBar();
    stubRects();
    const before = [...order()];
    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
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
   * 「拖拽进行中注入一次活动事件 ⇒ 松手之前 `#tab-bar` 子节点顺序一次都不变；
   * 松手后那一次重画必须补上（守卫是推迟不是丢弃）。住 `tests/frontend/ui/tabs.vitest.ts`「6d」。」
   *
   * # 机制（已核实，不是推测）
   *
   * `refreshTabBar` 挂在活动路上：`updateActivity` / `archiveTab` / `ensureTab` 末尾都无条件调它。
   * 拖到一半来一条活动事件，第 4 段那个「排序」循环就照 `orderedIds` 重排一次 DOM，
   * **指针底下的那个 tab 当场被换掉** —— 用户松手落到的不是他瞄的那一格。
   *
   * # 🔴 为什么注入的是「一个按盘上顺序该落在中间的新 tab 到了」
   *
   * 反空真自检（「扫到空集时要红，不是绿」）：注入的事件必须**真的改 `orderedIds` 的中间位置**，
   * 否则拿掉守卫 DOM 也不动，判据等于没买。
   * 〔AR1 重锚〕上一版注入的是「会话结束」，理由是「归档抽屉是 `barEl` 的兄弟，那个 tab 会整个离开 `#tab-bar`」。
   *   抽屉删了⇒ 会话结束今天**只改那颗按钮的 class、不改顺序**，那一版咬住的只剩
   *   「拖拽中零 DOM 写」（`barSnap` 连 `className` 一起比），不再是它自称的「跳位置」。
   * ⇒ 换成今天真会从中间插进去的那一路：盘上那份顺序（`savedOrder`）给新到的 `d` 留了 `a` 与 `b` 之间那一格，
   *   `ensureTab("d")` → `TabStore.placeInOrder` → `applySavedOrder` 把它放回那一格。这一路与 bg 树无关。
   *   下面第一条断言就是反空真前置：拖拽中**模型已经变了**（`d` 在第二位），而 DOM 一次都没动。
   *
   * # 判据钉的是「顺序一次都不变」，不是「最终顺序对不对」
   *
   * 最终顺序在 `mouseup` 之后本来就会对（那时重画照样发生）。
   * 坏的是**拖拽窗口之内**那一次重排 —— 所以快照要在 `mouseup` 之前比。
   */
  it("★ 6d：拖拽进行中来一个该落在中间的新 tab ⇒ mouseup 之前 #tab-bar 子节点顺序一次都不变", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN); // 首个 tab ⇒ 它是 active（`switchTo(_, "auto")`）
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", LOCAL_ORIGIN);
    // 盘上那份顺序：`d` 还没到，它的那一格在 `a` 与 `b` 之间（`applySavedOrder` 头注：没到的留在 `savedOrder` 里等它）。
    home(tm).store.savedOrder = ["a", "d", "b", "c"];
    flushBar();
    stubRects();
    expect(order(), "前置：d 没到之前顺序照旧").toEqual(["a", "b", "c"]);
    // 主栏子节点的「长相顺序」。用 textContent 而不是下标 —— 少一个、换一个、多一个都要能看出来。
    const barSnap = (): string =>
      [...(bar.querySelector(".tab-list") ?? bar).children].map((e) => `${e.className}#${e.textContent ?? ""}`).join(" | ");

    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    // 拖最后那个（c），指针停在第一条（a，0..40）的上半 ⇒ 落点 = a 之前。
    roots[2].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 10, bubbles: true }),
    );
    const during = barSnap();
    expect(during, "前置：三个 tab 都还在主栏里").toContain("tab ");

    // ⬇ 拖拽进行中注入一次活动事件：新会话 d 宣告到了，盘上顺序把它放在 a 与 b 之间。
    tm.ensureTab("d", "/c4", "p", LOCAL_ORIGIN);
    expect(order(), "反空真前置：模型真的从中间插进去了（拿掉守卫时 DOM 会跟着动）").toEqual([
      "a",
      "d",
      "b",
      "c",
    ]);

    expect(barSnap(), "拖拽中 tab 不许自己跳位置：松手前 #tab-bar 的子节点顺序一次都不许变").toBe(
      during,
    );

    // ⬇ 松手之后**必须补上**那一次重画 —— 守卫不是「把刷新永久吞掉」。
    document.dispatchEvent(
      new MouseEvent("mouseup", { clientX: 10, clientY: 10, bubbles: true }),
    );
    expect(barSnap(), "松手后 d 那一格要落实，否则守卫就成了静默丢刷新").not.toBe(during);
    expect(order(), "拖动本身的结果照常落实（c 到 a 之前），d 留在盘上给它的那一格").toEqual([
      "c",
      "a",
      "d",
      "b",
    ]);
    const barTabs = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab"));
    expect(barTabs.length, "四个 tab 都在主栏里").toBe(4);
  });

  /**
   * ★ 6d 下半（「松手后那一次重画必须补上（守卫是推迟不是丢弃）」）。
   *
   * 上一格松手落在别的 tab 上 ⇒ `applyDrop` 自己就会整刷一次，**盖住了**「推迟的那一次补没补」
   *   （死值验现打：删掉 `teardownDrag` 里补刷那一句，上一格照样绿）。
   * ⇒ 这一格走撕窗口那一路（`armed`）：它**不碰顺序、不刷栏**（`P7a2-Y2`），松手后唯一的刷新
   *   只能来自 `teardownDrag` 补的那一次。补丢了 ⇒ 拖拽中到的 d 永远不出现在栏里。
   */
  it("★ 6d 下半：拖出右缘撕窗口 ⇒ 拖拽中被挡下的那次重画在松手时补上（d 出现在它那一格）", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", LOCAL_ORIGIN);
    home(tm).store.savedOrder = ["a", "d", "b", "c"];
    flushBar();
    stubRects();
    const barTabText = (): string[] =>
      [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")).map((e) => e.textContent ?? "");
    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    roots[2].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    // clientX 远超 barRight+16 ⇒ armed
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 9999, clientY: 100, bubbles: true }),
    );
    const during = barTabText();
    tm.ensureTab("d", "/c4", "p", LOCAL_ORIGIN);
    expect(barTabText(), "拖拽中被挡下：d 还没进栏").toEqual(during);
    expect(during.some((t) => t.includes("c4")), "前置：d 此刻不在栏里（否则下面那条恒真）").toBe(false);
    document.dispatchEvent(
      new MouseEvent("mouseup", { clientX: 9999, clientY: 100, bubbles: true }),
    );
    expect(order(), "撕窗口那一路不改顺序；d 在盘上给它的那一格").toEqual(["a", "d", "b", "c"]);
    expect(
      barTabText().map((t) => (t.match(/c\d/) ?? [""])[0]),
      "推迟不是丢弃：松手后栏里按 orderedIds 摆出 d",
    ).toEqual(["c1", "c4", "c2", "c3"]);
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
  const setCols = (cols: { id: string; name: string; tabs?: string[] }[]): void => setGroups(tm, cols);
  const order = (): string[] => home(tm).store.orderedIds;

  // ── 🔴 墓碑：五条测「归档抽屉」的判据整块删除 ──────
  //
  // 删的是：`P7a1-Y1`（归档 tab 离开主栏进抽屉）· `P7a1-Y2`（当前 tab 绝不进抽屉）
  // · `P7a1-Y2b`（切走之后才落进抽屉）· `P7a1-Y3`（抽屉里的 tab 不挂 tear-off）
  // · `P7a3-Y2b`（归档优先于集合：灰 tab 进抽屉不进组）。
  //
  // **它们没有失效，是被测的那个功能整个不存在了。** 用户 2026-09-19 逐字：
  // 「没有归档这个东西，不要归档，就是灰 tab。现在的归档是错误的，甚至是 bug 的来源，
  //   全部删掉。」而抬头本来就写着「**已定**：删归档抽屉 · 固定灰 tab」。
  //
  // ⚠ **`P7a3-Y2b` 那条的判词今天正好反过来**：它断言「灰 tab 进抽屉**不进组**」，
  //   而 `§A.3` 逐字「**灰 tab 也能在组里**，更符合直觉」⇒ 留着它就是把设计判红。
  //   下面 `P7a3` 那一族里已经有「按集合分组」的正向判据盖住新行为，不另立。
  //
  // ⚠ 留下这块墓碑而不是静悄悄删：那条 —— 一条判据消失时，
  //   「它被删了」与「它从来没有过」在盘上长得一模一样。

  it("★ P7a3-Y2：成员进它的组，非成员照常直接挂主栏", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    tm.ensureTab("c", "/c3", "p", LOCAL_ORIGIN);
    setCols([{ id: "g1", name: "白天", tabs: ["a", "c"] }]);
    flushBar();

    const group = bar.querySelector<HTMLElement>(".tab-group")!;
    expect(group, "组容器要在").toBeTruthy();
    expect(group.querySelector(".tab-group-name")!.textContent).toBe("白天");
    // 成员真的在组里（两个），非成员一个都不在。
    expect(group.querySelectorAll(".tab-group-list > .tab")).toHaveLength(2);
    // 非成员直挂主栏（不是任何组的子孙）。
    const loose = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab"));
    expect(loose).toHaveLength(1);
  });

  // ── P-extra 组头就地改名（「组头就地 `<input>`：Enter 提交 / Esc 取消 / blur 提交」）──
  describe("P-extra 组头就地改名", () => {
    // 改名只经 `TabBarPrefs.renameGroup`（组表那一条补丁）⇒ 数它收到的 (id, 新名)。
    const renameWrites = (): { id: string; name: string }[] =>
      (home(tm).prefs.renameGroup as unknown as Mock).mock.calls.map((c) => ({ id: c[0] as string, name: c[1] as string }));
    let promptSpy: ReturnType<typeof vi.spyOn>;
    const open = (): HTMLInputElement => {
      tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
      setCols([{ id: "g1", name: "白天", tabs: ["a"] }]);
      flushBar();
      home(tm).prefs.renameGroup = vi.fn().mockResolvedValue(undefined) as never;
      bar.querySelector<HTMLElement>(".tab-group-name")!.click();
      const input = bar.querySelector<HTMLInputElement>(".tab-group-head input")!;
      expect(input, "组头没换成输入框").toBeTruthy();
      return input;
    };
    const key = (el: HTMLElement, k: string): void => {
      el.dispatchEvent(new KeyboardEvent("keydown", { key: k, code: k, bubbles: true }));
    };
    beforeEach(() => {
      promptSpy = vi.spyOn(window, "prompt");
    });
    afterEach(() => {
      expect(promptSpy, "还在弹原生 window.prompt").not.toHaveBeenCalled();
      promptSpy.mockRestore();
    });

    it("点组头 ⇒ 就地输入框（初值 = 现名、聚焦、全选），名字按钮让位", () => {
      const input = open();
      expect(input.value).toBe("白天");
      expect(document.activeElement).toBe(input);
      expect([input.selectionStart, input.selectionEnd]).toEqual([0, 2]);
      expect(bar.querySelector<HTMLElement>(".tab-group-name")!.hidden).toBe(true);
      key(input, "Escape");
    });

    it("Enter 提交：恰写一次、写的是新名；输入框收起、名字按钮回来", () => {
      const input = open();
      input.value = "  夜里 ";
      key(input, "Enter");
      expect(renameWrites()).toEqual([{ id: "g1", name: "夜里" }]);
      expect(bar.querySelector(".tab-group-head input")).toBeNull();
      expect(bar.querySelector<HTMLElement>(".tab-group-name")!.hidden).toBe(false);
    });

    it("Esc 取消：零写、名字不变", () => {
      const input = open();
      input.value = "夜里";
      key(input, "Escape");
      expect(renameWrites()).toHaveLength(0);
      expect(bar.querySelector(".tab-group-head input")).toBeNull();
      expect(bar.querySelector(".tab-group-name")!.textContent).toBe("白天");
    });

    it("blur 提交", () => {
      const input = open();
      input.value = "傍晚";
      input.blur();
      expect(renameWrites()).toEqual([{ id: "g1", name: "傍晚" }]);
    });

    it("空名 / 与现名相同 ⇒ 不写（只认一次：Enter 之后的 blur 不再写）", () => {
      let input = open();
      input.value = "   ";
      key(input, "Enter");
      input = (bar.querySelector<HTMLElement>(".tab-group-name")!.click(),
      bar.querySelector<HTMLInputElement>(".tab-group-head input")!);
      input.value = "白天";
      key(input, "Enter");
      input.blur();
      expect(renameWrites()).toHaveLength(0);
    });
  });

  it("★★ P7a3-E：**没拉过集合的实例不许写集合** —— viewer 窗口会把用户已有的全冲掉", () => {
    // 撕离出来的 viewer 窗口也用 TabManager（`main.ts:938`，tab 栏由 .viewer-mode 隐藏），
    // 但它**从不 loadCollections** ⇒ `collections` 恒空。右键菜单里若还留着「新建集合…」，
    // 点一下就把「只含这一个」的列表写回 config.json —— 用户已有的集合全没了。
    // 同族先例就在旁边一行：「viewer 窗口共享 localStorage，禁写 last-active（防污染主窗口记忆）」。
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    flushBar();
    const root = [...(bar.querySelector(".tab-list") ?? bar).children].find((e) => e.classList.contains("tab")) as HTMLElement;
    root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const labels = [...document.querySelectorAll("[role=menu] button")].map(
      (e) => e.querySelector("[data-part=label]")?.textContent ?? "",
    );
    expect(labels.length, "菜单要真的开出来（否则本判据在空转）").toBeGreaterThan(0);
    expect(labels.join("|"), "没拉过集合就不给集合入口").not.toContain(copyText("tabMenu.collection.add"));
  });

  it("★ P7a3-E 反面：**拉过了就必须给入口**（否则「永远不给」也能过）", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    await tm.loadCollections(); // mock 的 config 是空的 ⇒ 集合为空，但「拉过」为真
    flushBar();
    const root = [...(bar.querySelector(".tab-list") ?? bar).children].find((e) => e.classList.contains("tab")) as HTMLElement;
    root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const labels = [...document.querySelectorAll("[role=menu] button")].map(
      (e) => e.querySelector("[data-part=label]")?.textContent ?? "",
    );
    expect(labels.join("|")).toContain(copyText("tabMenu.collection.add"));
  });

  // 要求：「一条都不许静默忽略」。右键菜单那两条入口到上界要出声（正反各一格）。
  it("〔TL2 · E13〕右键「新建集合…」集合数到上界 ⇒ 不弹输入框、说一句；差一个 ⇒ 照常弹", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    await tm.loadCollections();
    const clickNew = async (): Promise<void> => {
      flushBar();
      const root = [...(bar.querySelector(".tab-list") ?? bar).children].find((e) => e.classList.contains("tab")) as HTMLElement;
      root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      const btn = [...document.querySelectorAll("[role=menu] button")].find(
        (e) => e.querySelector("[data-part=label]")?.textContent === copyText("tabMenu.collection.new"),
      ) as HTMLButtonElement | undefined;
      expect(btn, "菜单里要有「新建集合…」（否则本判据在空转）").toBeTruthy();
      btn!.click();
      document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
      for (let i = 0; i < 5; i++) await Promise.resolve();
    };
    const many = (n: number): { id: string; name: string }[] =>
      Array.from({ length: n }, (_, i) => ({ id: `c${i}`, name: `组${i}` }));
    setCols(many(COLLECTION_CAP));
    await clickNew();
    expect(noAskDialog(), "满了还让用户白填一次名字").toBe(true);
    expect(vi.mocked(showActionFailureToast).mock.calls.map((c) => String(c[0]))).toEqual([copyText("tabCollections.full.collectionsTitle")]);

    vi.mocked(showActionFailureToast).mockClear();
    setCols(many(COLLECTION_CAP - 1));
    await clickNew();
    expect(noAskDialog(), "没满就该照常问名字（正控）").toBe(false);
    await answerAskText(null);
    expect(showActionFailureToast).not.toHaveBeenCalled();
  });

  // 成员上界随成员名单一起作废 ⇒ 原「那一组满了 ⇒ 说一句」一格删了（被测的东西不在了）；
  //   换成「加入 / 移出就是改这个 tab 自己的组 id」正反各一格。
  it("〔GRP1〕右键「加入集合 › 某组」⇒ 这个 tab 的 `group` 就是那一组、一句话都不说；「移出」⇒ 回到散 tab", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    await tm.loadCollections();
    const click = (sid: string, label: string): void => {
      flushBar();
      const root = home(tm).bar.tabButtons.get(sid)!.root;
      root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      const btn = [...document.querySelectorAll("[role=menu] button")].find(
        (e) => e.querySelector("[data-part=label]")?.textContent === label,
      ) as HTMLButtonElement | undefined;
      expect(btn, `菜单里要有「${label}」（否则本判据在空转）`).toBeTruthy();
      btn!.click();
      document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    };
    setCols([{ id: "g", name: "白天", tabs: ["b"] }]);
    click("a", "白天");
    expect(showActionFailureToast).not.toHaveBeenCalled();
    expect(membersOf(tm, "g"), "a 该进组（b 原本就在）").toEqual(["a", "b"]);
    click("a", copyText("tabMenu.collection.remove", { name: "白天" }));
    expect(membersOf(tm, "g"), "移出只动 a").toEqual(["b"]);
    expect(home(tm).store.tabs.get("a")!.group).toBeNull();
  });

  it("★ P7a3-D：组在前、未归组的在后（DoD 逐字如此，实现不许自己反过来）", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN); // 归组
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN); // 散的
    setCols([{ id: "g1", name: "白天", tabs: ["a"] }]);
    flushBar();
    const kids = [...(bar.querySelector(".tab-list") ?? bar).children];
    const gi = kids.findIndex((e) => e.classList.contains("tab-group"));
    const ti = kids.findIndex((e) => e.classList.contains("tab"));
    expect(gi, "组容器要在").toBeGreaterThanOrEqual(0);
    expect(ti, "散 tab 要在").toBeGreaterThanOrEqual(0);
    expect(gi, "组排在未归组的之前").toBeLessThan(ti);
  });


  it("★ P7a3-Y3：解散集合**一个会话都不许少**", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    setCols([{ id: "g1", name: "白天", tabs: ["a"] }]);
    flushBar();
    const before = [...order()];
    expect(bar.querySelector(".tab-group")).toBeTruthy();

    (bar.querySelector(".tab-group-del") as HTMLButtonElement).click();
    flushBar();
    // 「没删掉会话」是**没有发生的事** ⇒ 钉逐项相等，不是钉「没崩」。
    expect(order(), "集合是个视图，不是容器").toEqual(before);
    expect(bar.querySelector(".tab-group"), "组容器该没了").toBeNull();
    expect([...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab"))).toHaveLength(2);
    expect(home(tm).store.tabs.get("a")!.group, "解散 ⇒ 组员回到散 tab").toBeNull();
  });

  // 「空」今天只剩一种来路：重启后组员还没到（意图）—— 组表里有它就画。
  it("空集合也留着 —— 刚建的集合不该看不见", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    setCols([{ id: "g1", name: "空的" }]);
    flushBar();
    expect(bar.querySelector(".tab-group")).toBeTruthy();
    expect(bar.querySelectorAll(".tab-group-list > .tab")).toHaveLength(0);
  });
});

// ==========================================================================
// 固定（pinned）—— 「关了 app 再打开它还在」
//
// `§B.1` 现打：在这之前全仓 `pinned`/`isPinned`/`pinTab` **零命中**。
// `§B.3` 的承重：它**必须是正交的一维**，不能做成 `TabStatus` 的第三态 ——
//   因为 `archived + pinned` 才是用户的主用例（固定住一个已经跑完的会话）。
//
// 🔴 反空真：这一组的 config 是**一份真的在内存里的盘**（走那个已经被 mock 的
//   `invoke`，`load_config`/`patch_config` 两条命令），所以「落盘了没有」是
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
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    disk = {};
    // 🔴 config 走 `invoke` 这一层（与仓里 `src/frontend/ui/config.ts` 的真实链路一致）：
    //   上面的 `tab-bar-state` / `tab-collections` 因此是**真跑**的，
    //   判据买到的是那一段的形状（只动自己那个键 · 清洗 · 上界），不是一个 spy。
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "patch_config") {
        // 写只交补丁；按与 Rust 写口同一份金样的语义应用（`tests/frontend/ui/config-patch-fake.ts`）。
        disk = JSON.parse(applyConfigEdits(JSON.stringify(disk), (args as { edits: Edit[] }).edits));
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    }));
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
            background: false,
            name: null,
            title: "存下来的标题",
          },
        ],
      },
    };
    await tm.loadPinned();
    const t = tabOf("s1");
    // 〔说不清〕本机还没报过清单 ⇒ **说不清**（`Unseen` 不许显示成已结束）；
    //   报完了、清单里没有它 ⇒ 已结束（`§B.5` 那句「没有活进程」这才成立）。
    expect(t.state, "那台还没报完清单 ⇒ 说不清，不许说成已结束").toEqual(UNSEEN);
    tm.markOriginSeen(LOCAL_ORIGIN, new Set());
    expect(t.state, "清单报完了、里面没有它 ⇒ 已结束").toEqual(ENDED);
    expect(t.pinned, "复活出来的当然是固定的").toBe(true);
    expect(t.title, "🔴 标题要用存下来的那份 —— 不等读文件（`§B.5` 逐字）").toBe("存下来的标题");
    expect(t.parentPath, "jsonlPath 是复活的必需品，没落到 tab 上等于白存").toBe("/p/s1.jsonl");
    expect(t.projectDir).toBe("/home/u/proj");
  });

  it("🔴 盘上那条**已经活着**时：只补 `pinned`，`status` 一个字不碰", async () => {
    // 后端 `event_replay` 可能已经先宣告了它。把一条真活着的会话按回 archived 是一句假话。
    tm.ensureTab("s1", "/c", "/real.jsonl", LOCAL_ORIGIN);
    disk = { tabBar: { pinned: [{ sid: "s1", origin: LOCAL_ORIGIN, title: "旧标题", jsonlPath: "/old.jsonl" }] } };
    await tm.loadPinned();
    expect(tabOf("s1").state, "🔴 把活着的会话按成已结束了").toEqual(LIVE);
    expect(tabOf("s1").pinned).toBe(true);
    expect(tabOf("s1").parentPath, "活着那条的真路径不许被盘上的旧值覆盖").toBe("/real.jsonl");
  });

  it("盘上存的机器标签旧了：后端宣告在另一台 ⇒ tab 改过来，盘上那条同拍改写（下次起来就是对的标签）", async () => {
    disk = { tabBar: { pinned: [{ sid: "s1", origin: "old-host", title: "T", jsonlPath: "/p/s1.jsonl" }] } };
    await tm.loadPinned();
    tm.ensureTab("s1", "/home/u/proj", "", "new-host");
    await flushDisk();
    expect(tabOf("s1").origin).toBe("new-host");
    expect((pinnedOnDisk() as { sid: string; origin: string }[]).map((p) => [p.sid, p.origin])).toEqual([["s1", "new-host"]]);
  });

  it("`togglePin` ⇒ 内存翻转 ＋ **盘上真的出现/消失那一条**", async () => {
    await tm.loadPinned(); // 先取得资格
    tm.ensureTab("s1", "/home/u/proj", "/p/s1.jsonl", LOCAL_ORIGIN);
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
    tm.ensureTab("s1", "/c", "p", LOCAL_ORIGIN);
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
    tm.ensureTab("s1", "/c", "/p/s1.jsonl", LOCAL_ORIGIN);
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
    tm.ensureTab("s1", "/c", "p", LOCAL_ORIGIN);
    tm.togglePin("s1"); // live 也能 pin（`§B.3b`：效果等它变灰才显现）
    expect(tabOf("s1").state).toEqual(LIVE);
    expect(tabOf("s1").pinned).toBe(true);
    tm.archiveTab("s1");
    expect(tabOf("s1").state, "四种组合的第四格 —— pin 唯一真正生效的那一格").toEqual(ENDED);
    expect(tabOf("s1").pinned, "🔴 归档把 pin 冲掉了 ⇒ 正交性破了，主用例没了").toBe(true);
  });

  it("🔴 `§B.3b`：固定**不影响位置** —— `orderedIds` 一个字不许动", async () => {
    await tm.loadPinned();
    for (const s of ["a", "b", "c"]) tm.ensureTab(s, "/c", "p", LOCAL_ORIGIN);
    const before = [...home(tm).store.orderedIds];
    tm.togglePin("b"); // 固定中间那个
    expect(home(tm).store.orderedIds, "有人把固定的排到前面去了 —— 那是被 `§B.3b` 删掉的「固定区」").toEqual(
      before,
    );
    expect(before).toEqual(["a", "b", "c"]);
  });

  it("📌 角标：`.tab.pinned` 跟着 `Tab.pinned` 走，且角标元素真的在（正反两控）", async () => {
    await tm.loadPinned();
    tm.ensureTab("s1", "/c", "p", LOCAL_ORIGIN);
    const root = home(tm).bar.tabButtons.get("s1")!.root;
    expect(root.classList.contains("pinned"), "还没固定就显角标 ⇒ 这个类没在跟着值走").toBe(false);
    tm.togglePin("s1");
    expect(root.classList.contains("pinned"), "固定了却不显 —— 用户看不出哪些是固定的").toBe(true);
    expect(root.querySelector(".tab-pin svg"), "角标元素不在 / 不是图标件（Phosphor 图钉）").not.toBeNull();
  });

  it("右键菜单：那一项的文案跟着状态翻转；没资格时整项不出现", async () => {
    const rightClick = (sid: string): void => {
      home(tm)
        .bar.tabButtons.get(sid)!
        .root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    };
    const labels = (): string[] =>
      [
        ...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem]") ??
          []),
      ].map((b) => b.querySelector("[data-part=label]")?.textContent ?? "");

    tm.ensureTab("s1", "/c", "p", LOCAL_ORIGIN);
    rightClick("s1");
    expect(labels(), "🔴 没 `loadPinned` 过就给入口 ⇒ 点一下清空用户的固定表").not.toContain(
      copyText("tabMenu.pin.pin"),
    );

    await tm.loadPinned();
    rightClick("s1");
    expect(labels()).toContain(copyText("tabMenu.pin.pin"));
    (
      [...document.body.querySelectorAll("[role^=menuitem]")].find(
        (b) => b.querySelector("[data-part=label]")?.textContent === copyText("tabMenu.pin.pin"),
      ) as HTMLButtonElement
    ).click();
    expect(tabOf("s1").pinned, "菜单项点了没反应 —— 这个仓刚因为这个撤掉过两个取色器").toBe(true);
    rightClick("s1");
    expect(labels(), "已经固定了还显「固定此标签」⇒ 用户没法取消").toContain(copyText("tabMenu.pin.unpin"));
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
    expect(hintOf("ok").querySelector(".pin-revived-hint-btn")?.textContent).toBe(copyText("tabBarPrefs.pin.resume"));

    expect(
      hintOf("bad").querySelector("strong")?.textContent,
      "`§B.6` 逐字：`jsonlPath` 为空的要提示「这个会话没有留下记录」",
    ).toBe(copyText("sessionState.pinnedEmpty.head"));
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
// Edge 式拖动合并成组 —— **纯判定那一半**
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
  /** 被拖的那一个不在这组矩形里（原先这里是「被拖的那一块」空集）。 */
  const none = `（${copyText("extPage.state.missing")}）`;

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

  it("被拖的那一个不参与落点（落到自己身上不是一次重排）", () => {
    expect(pickDropTarget(rects, 5, "a", null), "a 被拖着，落点该轮到 b").toEqual({
      kind: "before",
      sid: "b",
    });
    expect(
      pickDropTarget(rects, 40, "b", "b"),
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
    expect(tabUnderY(rects, 28, none), "28 是 b 的上沿 ⇒ 算 b").toBe("b");
    expect(tabUnderY(rects, 27.9, none), "27.9 还在 a 里").toBe("a");
    expect(tabUnderY(rects, 999, none), "谁都没压着要能说「没有」").toBeNull();
    expect(tabUnderY(rects, 5, "a"), "被拖的那个要排除").toBeNull();
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
    expect(defaultGroupName(null, null, []), "一个组都没有 ⇒ 从 1 起").toBe(copyText("tabDrop.group.defaultName", { n: "1" }));
    expect(defaultGroupName(null, null, [copyText("tabDrop.group.defaultName", { n: "1" }), "白天", copyText("tabDrop.group.defaultName", { n: "3" })]), "最大编号是 3 ⇒ 下一个是 4").toBe(
      copyText("tabDrop.group.defaultName", { n: "4" }),
    );
    expect(defaultGroupName("/a/x", "/b/y", [copyText("tabDrop.group.defaultName", { n: "2" })]), "共同前缀算不出来才轮到编号").toBe(copyText("tabDrop.group.defaultName", { n: "3" }));
  });

  it("有共同目录名时**优先用它**（反面控：别退回编号）", () => {
    expect(defaultGroupName("/home/u/proj/a", "/home/u/proj/b", [copyText("tabDrop.group.defaultName", { n: "1" })])).toBe("proj");
  });
});

describe("步 17·D ② 归属跟着落点宿主走（`§D.7` 的「拖出组」与「拖进组」是同一条规则）", () => {
  // 组员关系是 tab 自己的属性 ⇒ 这里不再重算整张组表（原 `applyDropToCollections` /
  //   `collectionsEqual` 随成员名单一起删），纯函数 `groupMoveForDrop` 只回「被拖那个 tab 怎么动」。
  //   每一格对应原来的一格；「建组到上界」那一格挪到了 ④（上界现在由 `TabBarPrefs.foundGroup` 判）。
  const groupOf =
    (m: Record<string, string>) =>
    (sid: string): string | null =>
      m[sid] ?? null;

  it("🆕 `onto` 到一个**没有组**的 tab ⇒ 与它现建一个组", () => {
    expect(groupMoveForDrop(groupOf({}), "a", { kind: "onto", sid: "b" })).toEqual({ kind: "found", with: "b" });
  });

  it("`onto` 到一个**已经在组里**的 tab ⇒ 进那个组，不新建", () => {
    expect(groupMoveForDrop(groupOf({ b: "g1" }), "a", { kind: "onto", sid: "b" })).toEqual({ kind: "join", gid: "g1" });
  });

  // 〔「删掉树」〕原「整块一起走（交互 tab 连同它的 bg 子串）」一格删了 —— 归属只跟着被拖的
  //   那一个走（本文件末尾「〔BG1〕」那组的 ③ ④，两种 tab 各跑）。

  it("🔴 `before` 一个**散 tab** ⇒ 从原来的组里**移出**（`§D.7` 的拖出组）", () => {
    expect(
      groupMoveForDrop(groupOf({ a: "g1", b: "g1" }), "a", { kind: "before", sid: "z" }),
      "落点宿主是散 tab 区 ⇒ a 不再属于 g1",
    ).toEqual({ kind: "leave" });
  });

  it("`before` 一个**组里的 tab** ⇒ 进那个组（同一条规则的另一侧）", () => {
    expect(groupMoveForDrop(groupOf({ b: "g1" }), "a", { kind: "before", sid: "b" })).toEqual({ kind: "join", gid: "g1" });
  });

  it("`end` ⇒ 移出（末尾就是散 tab 区）", () => {
    expect(groupMoveForDrop(groupOf({ a: "g1" }), "a", { kind: "end" })).toEqual({ kind: "leave" });
  });

  it("🔴 反面控：什么都不该变的两种情形 ⇒ `stay`（零写盘：拖动是高频动作）", () => {
    expect(
      groupMoveForDrop(groupOf({ a: "g1" }), "a", { kind: "onto", sid: "a" }),
      "压在自己身上不是一次合并",
    ).toEqual({ kind: "stay" });
    expect(
      groupMoveForDrop(groupOf({ z: "g1" }), "a", { kind: "before", sid: "b" }),
      "两个都是散 tab ⇒ 归属这一维没有任何事发生",
    ).toEqual({ kind: "stay" });
  });
});

// ==========================================================================
// 手势接到真 DOM 上那一半：`§D.2` 的两个缺口是不是真的堵上了
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
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c1", "p", LOCAL_ORIGIN);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["b"] }]);
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
    tm.ensureTab("a", "/home/u/proj/x", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/home/u/proj/y", "p", LOCAL_ORIGIN);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf().length, "没建组").toBe(1);
    expect(membersOf(tm, colsOf()[0].id), "成员不对（按栏里的顺序：a 已插到 b 之前）").toEqual(["a", "b"]);
    expect(colsOf()[0].name, "默认名该走「共同前缀目录名」那一条（`§D.6` ①）").toBe("proj");
    expect(order(), "顺序也要落实：a 插到 b 之前").toEqual(["a", "b"]);
  });

  it("🔴 顺序没变、只有归属变了的那一拍**不许被吞掉**", () => {
    // 这是把 `applyReorder` 改成 `applyDrop` 的正题：旧实现 `if (顺序没变) return`，
    // 而「把组里的 tab 原地拖出来」恰好就是顺序不变、归属变。
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["a"] }]);
    home(tm).dragger.applyDrop("a", { kind: "before", sid: "b" }); // a 本来就在 b 前面 ⇒ 顺序不变
    expect(order(), "顺序确实没变（前提成立，这一格才有意义）").toEqual(["a", "b"]);
    expect(membersOf(tm, "g1"), "🔴 归属那一半被「没变化就 return」吞了").toEqual([]);
  });

  it("`end` ⇒ 拖出组并落到末尾", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["a"] }]);
    home(tm).dragger.applyDrop("a", { kind: "end" });
    expect(order()).toEqual(["b", "a"]);
    expect(membersOf(tm, "g1")).toEqual([]);
    expect(colsOf(), "拖出的是在栏里的最后一个 ⇒ 组消失").toEqual([]);
  });

  // 要求：「一条都不许静默忽略」—— 到上界时这一下没做成，要说出来（正反各一格）。
  // 原「拖进一个满了的组 ⇒ 说一句『没有加进』」一格删了：成员上界随成员名单一起作废（被测的东西不在了）。
  it("〔TL2 · E13〕集合数到上界时拖放建组 ⇒ 说一句「没有建新集合」；差一个 ⇒ 建出来、不出声", () => {
    const many = (n: number): TabCollection[] =>
      Array.from({ length: n }, (_, i) => ({ id: `c${i}`, name: `组${i}` }));
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    home(tm).prefs.collections = many(COLLECTION_CAP);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf().length).toBe(COLLECTION_CAP);
    expect(vi.mocked(showActionFailureToast).mock.calls.map((c) => String(c[0]))).toEqual([copyText("tabCollections.full.collectionsTitle")]);
    expect(
      [home(tm).store.tabs.get("a")!.group, home(tm).store.tabs.get("b")!.group],
      "没建出组 ⇒ 两个都不许挂在一个不存在的组 id 上",
    ).toEqual([null, null]);

    vi.clearAllMocks();
    home(tm).prefs.collections = many(COLLECTION_CAP - 1);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf().length, "差一个没满 ⇒ 该建出来").toBe(COLLECTION_CAP);
    expect(membersOf(tm, colsOf()[COLLECTION_CAP - 1].id), "建出来的组里恰是这两个").toEqual(["a", "b"]);
    expect(showActionFailureToast).not.toHaveBeenCalled();
  });

  it("没 `loadCollections` 过的实例：归属一个字不动（顺序照常）", () => {
    // 与右键菜单那道门同一条理由：没读过盘就写，等于把用户已有的集合清空。
    home(tm).prefs.collectionsLoaded = false;
    tm.ensureTab("a", "/c1", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/c2", "p", LOCAL_ORIGIN);
    home(tm).dragger.applyDrop("b", { kind: "onto", sid: "a" });
    expect(colsOf(), "没读过盘还敢建组 ⇒ 下一次落盘会把用户的集合全冲掉").toEqual([]);
    expect(home(tm).store.tabs.get("b")!.group, "组 id 也一个字不动").toBeNull();
    expect(order(), "顺序这一半照常").toEqual(["b", "a"]);
  });
});

// ==========================================================================
// 停留计时器**真的接在拖拽上** —— 一次完整的假手势
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
    tm.ensureTab("a", "/home/u/proj/x", "p", LOCAL_ORIGIN);
    tm.ensureTab("b", "/home/u/proj/y", "p", LOCAL_ORIGIN);
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
    expect(membersOf(tm, cols[0].id), "组员 = 带这个组 id 的 tab（a 已插到 b 之前）").toEqual(["a", "b"]);
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
// 步 17·C 顺序落盘 —— **「读回来」那一半**（那行的 🟡）
//
// 落盘那一侧（`tab-bar-state.ts` 的 `sanitizeOrder` / `getTabOrder` / `setTabOrder`）
// 已经有 `tests/frontend/ui/tab-bar-state.vitest.ts` 10 格盯着，**那一段是好的**。
// 这一组只量一件事：**`TabManager.loadOrder` 有没有真的把盘上那份顺序装回栏里。**
//
// 🔴 **为什么必须照「真实启动时序」摆**（这一组全部判据的承重）：
//   `src/frontend/ui/main.ts` 里那一行是 `loadPinned().finally(() => loadOrder())` ——
//   `loadOrder` 跑在**会话还没到**的那一刻，tab 是随后由 `session_added` / 首行**陆续**
//   建出来的。⇒ 把 tab 先造好再调 `loadOrder` 的判据**买不到这条性质**：
//   它在那种摆法下是绿的，而在真实时序下整张顺序会被摘掉。
//   ⚠ 这也正是 `tab-bar-state.vitest.ts` 那条「往返」格量不到的面 ——
//     它 `setTabOrder(["x","y"])` 之后拿 `getTabOrder(new Set(["x","y"]))` 读，
//     **`alive` 两侧同源 ⇒ 恒真**（点名的那一形）。
//
// 🔴 反空真：这一组的 config 是**一份真的在内存里的盘**（走已被 mock 的 `invoke`,
//   `load_config`/`patch_config`），所以「顺序落没落上」是**读盘对拍**，不是数调用次数。
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
   * **真实启动时序**（逐行对着 `src/frontend/ui/main.ts` 那三句摆）：
   *   `new TabManager` → `loadPinned()` → `loadOrder()` → 会话**随后**陆续到。
   * @param arriving 会话到达的次序（＝后端清单/事件流给的次序，通常不等于用户拖出来的次序）
   */
  const startupThenSessionsArrive = async (arriving: string[]): Promise<void> => {
    await tm.loadPinned();
    await tm.loadOrder();
    for (const sid of arriving) {
      // 每个 sid 一个自己的 cwd（原先是为了避开 bg 树状锚定；树删了之后留着无害）。
      tm.ensureTab(sid, `/proj/${sid}`, `/p/${sid}.jsonl`, LOCAL_ORIGIN, false, null);
    }
  };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "patch_config") {
        // 写只交补丁；按与 Rust 写口同一份金样的语义应用（`tests/frontend/ui/config-patch-fake.ts`）。
        disk = JSON.parse(applyConfigEdits(JSON.stringify(disk), (args as { edits: Edit[] }).edits));
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    }));
    tm = makeTM();
  });

  // --- ① 量具自检：先证明这一组的「盘」和 `loadOrder` 这条路真的在动 ---------
  it("🔴 量具自检：tab **先在**、再调 `loadOrder` ⇒ 顺序真的会被重排（不过这格，下面全是空转）", async () => {
    // 这一格**刻意**用「先造 tab 再读盘」那种摆法 —— 它证明的是
    // 「读盘 + 重排 + 重画」这套机件本身是通的，从而把下面那格的红**钉在时序上**，
    // 而不是钉在「读不出来」或「键不对」上。
    tm.ensureTab("a", "/proj/a", "/p/a.jsonl", LOCAL_ORIGIN, false, null);
    tm.ensureTab("b", "/proj/b", "/p/b.jsonl", LOCAL_ORIGIN, false, null);
    tm.ensureTab("c", "/proj/c", "/p/c.jsonl", LOCAL_ORIGIN, false, null);
    expect(home(tm).store.orderedIds, "到达序就是建出来的序").toEqual(["a", "b", "c"]);

    putOrderOnDisk(["c", "b", "a"]);
    await tm.loadOrder();
    expect(
      home(tm).store.orderedIds,
      "🔴 连「tab 已经在了」这种最宽松的摆法都排不回来 ⇒ 成因不是时序，是读/用那一段本身坏了",
    ).toEqual(["c", "b", "a"]);
  });

  // --- ② 正题：真实启动时序下的「存 → 读回来」--------------------------------
  it("🔴 顺序跨重启（验证钩子 9）：盘上一份非默认顺序 ⇒ 会话到齐后**逐位**对上", async () => {
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
    tm.ensureTab("d", "/proj/d", "/p/d.jsonl", LOCAL_ORIGIN, false, null);
    expect(
      home(tm).store.orderedIds,
      "🔴 后到的 tab 把用户刚拖的顺序撤销了（拿一份过期的盘去排）",
    ).toEqual(["a", "c", "b", "d"]);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 〔骨架〕接入判据：索引 → 占位 → 门控 → 跳转。
// 本文件把 `MessageStream` / `RecordTimeline` mock 掉了 ⇒ 这里只判**接线**（要没要索引、
// 接没接上、哪些行收纳哪些建卡）；「只物化可见区」本身的几何判据住 `tests/frontend/ui/skeleton-view.vitest.ts`
// （那边是真 stream ＋ 真 timeline）。
// ═══════════════════════════════════════════════════════════════════════
describe("骨架接入：索引 → 占位 → 门控 → 跳转", () => {
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
  // 骨架索引改走通道：一发 `chan_call` 译回「哪一问 ＋ 旧形参」（`chan-fake.ts::sessionReadCalls`）。
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
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
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

  // 切换可打断：索引是切进来那一下要的，回来时人已经切走了 ⇒ 不在后台接（接骨架要插占位、量几何、补可见区 ——
  // 收起的 tab 里做这些是白干还逼排版，快速连切时一串旧切换的活全堆在后面）。停着，切回来的下一帧再接。
  it("★ 索引回来时 tab 已经切走 ⇒ 不在后台接；切回来的下一帧接上，不重问", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
    const t = replay("pk"); // pk 是当前的，批结束要了索引（还没回来）
    tm.onLine(mk("pkOther", 5, "po5"));
    tm.switchTo("pkOther"); // 索引回来之前切走
    await settle();
    expect(t.skeleton, "切走了还在后台接骨架 ⇒ 旧切换的活还在干").toBeNull();
    tm.switchTo("pk");
    expect(t.skeleton, "同步段里不接（接要量几何）").toBeNull();
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    expect(t.skeleton, "切回来那一帧该接上").not.toBeNull();
    expect(t.skeleton!.pendingRows).toBe(200);
    expect(indexCalls().filter(([, a]) => (a as { jsonlPath: string }).jsonlPath === "/p/pk.jsonl").length, "停着的那一份直接用，不重问").toBe(1);
  });

  // 「列宽变了」的入口：消息流尺寸变了 ⇒ 现量 `.stream-content` 宽交骨架重估；量不到宽不动。
  it("〔P3〕消息流尺寸变了 ⇒ 现量的列宽交骨架 relayout（后台 tab 也跟上）；量不到宽 ⇒ 不动", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
    const t = replay("cw");
    await settle();
    expect(t.skeleton, "前置：骨架要接上").not.toBeNull();
    const spy = vi.spyOn(t.skeleton!, "relayout");
    let width = 0;
    t.stream.contentElement.getBoundingClientRect = () => ({ width }) as DOMRect;
    home(tm).store.activeId = "other";
    t.stream.onViewportResize?.();
    expect(spy, "量不到宽也重估").not.toHaveBeenCalled();
    width = 640;
    t.stream.onViewportResize?.();
    expect(spy.mock.calls).toEqual([[640]]);
  });

  it("老后端（available:false · oldBackend）⇒ 不接、不再问，尾部窗口照旧（账本还在、哨兵还在）", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(
        cmd === "read_session_index"
          ? { available: false, reason: "老后端", failure: "oldBackend", from: 0, end: 0, rows: [] }
          : undefined,
      ),
    ) as never));
    const t = replay("old");
    await settle();
    expect(t.skeleton).toBeNull();
    expect(t.skeletonFetch).toBe("done");
    expect(t.window.pendingCount).toBe(200);
    expect(t.stream.contentElement.querySelector(".stream-more-above")).not.toBeNull();
    tm.onBatchStart();
    tm.onBatchEnd();
    expect(indexCalls().length, "结构性失败不该再问").toBe(1);
  });

  // 「索引或清单一次瞬时失败（ssh 抖一下）⇒ 这个 tab 灰到关掉重开」⇒ 下一次触发点再问一次。
  it("★ 〔GAP1〕瞬时失败 ⇒ 下一次触发点（批结束）再问**一次**；再失败 ⇒ 定死不再问", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(
        cmd === "read_session_index"
          ? { available: false, reason: "抖了一下", failure: "transport", from: 0, end: 0, rows: [] }
          : undefined,
      ),
    ) as never));
    const t = replay("flap");
    await settle();
    expect(t.skeletonFetch).toBe("again");
    tm.onBatchStart();
    tm.onBatchEnd();
    await settle();
    expect(indexCalls().length, "瞬时失败之后的下一次触发点该再问一次").toBe(2);
    expect(t.skeletonFetch).toBe("done");
    tm.onBatchStart();
    tm.onBatchEnd();
    await settle();
    expect(indexCalls().length, "重问过一次还失败 ⇒ 不再问（有界）").toBe(2);
  });

  it("🔴 seq 空间对不上（uuid 在索引里落在别的 seq）⇒ 不接 —— 不许硬对", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300, 7) : undefined),
    ) as never));
    const t = replay("shift");
    await settle();
    expect(t.skeleton).toBeNull();
    expect(t.window.pendingCount).toBe(200);
  });

  it("门控：占位里的行收纳；岛（ensure 物化过的一段）里迟到的行就地建卡，不收纳", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
    const t = replay("gate", new Set([60, 150]));
    await settle();
    const sk = t.skeleton!;
    sk.ensure(60, 5); // [55, 66) 物化（60 还没到）
    expect(sk.isPending(60)).toBe(false);
    const before = t.window.pendingCount;
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    tm.onLine(mk("gate", 60, "u60")); // 迟到、落在岛里 ⇒ 建卡
    expect(spy.mock.calls.map((c) => (c[0] as { seq: number }).seq)).toEqual([60]);
    tm.onLine(mk("gate", 150, "u150")); // 迟到、落在占位里 ⇒ 收纳
    expect(spy.mock.calls.length).toBe(1);
    expect(t.window.pendingCount).toBe(before + 1);
  });

  it("子步 5：物化到一段**还没到过**的行 ⇒ 按索引的字节边界要回来（从偏移读），走 onLine 全套建卡", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string, args?: unknown) => {
      if (cmd === "read_session_index") return Promise.resolve(idx(300));
      if (cmd === "read_session_range") {
        // 请求里不再带 `lineCount`（后端自己数）：这一段几行按夹具的行边界算（o = seq × 10、n = 10）。
        const a = args as { seqBase: number; offset: number; until: number };
        return Promise.resolve(
          Array.from({ length: (a.until - a.offset) / 10 }, (_, k) => mk("miss", a.seqBase + k, `u${a.seqBase + k}`)),
        );
      }
      return Promise.resolve(undefined);
    }) as never));
    const t = replay("miss", new Set([100, 101, 102, 150]));
    await settle();
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    t.skeleton!.ensure(101, 3); // [98, 105)：98/99/103/104 在账本里，100–102 没到过
    const ranges = recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_range");
    // 连续缺的三行并成**一段**、一次 IPC；字节边界取自索引（o = seq×10，n = 10）
    expect(ranges).toEqual([
      { origin: "<local>", jsonlPath: "/p/miss.jsonl", offset: 1000, until: 1030, seqBase: 100 },
    ]);
    await settle();
    const rendered = spy.mock.calls.map((c) => (c[0] as { seq: number }).seq).sort((x, y) => x - y);
    expect(rendered).toEqual([98, 99, 100, 101, 102, 103, 104]);
    // 🔴 取回的是**历史**：按重放语义建卡 —— sink 不接 onRealUserInput（历史 user 卡不许自动切 tab），
    //    轮次结束检测按批期短路（不许为历史弹系统通知）
    for (const c of spy.mock.calls.filter((c) => (c[0] as { seq: number }).seq >= 100 && (c[0] as { seq: number }).seq <= 102)) {
      expect((c[2] as { onRealUserInput?: unknown }).onRealUserInput).toBeUndefined();
    }
    const { turnEndNotifier } = await import("../../../src/frontend/ui/turn-notify");
    const obs = vi.mocked(turnEndNotifier.observe).mock.calls.filter(
      (c) => (c[2] as { seq: number }).seq >= 100 && (c[2] as { seq: number }).seq <= 102,
    );
    expect(obs.length).toBe(3);
    expect(obs.every((c) => c[3] === true), "历史行按 live 喂了 ⇒ 会为旧轮次弹通知").toBe(true);
    // 150 不在这一段里 ⇒ 没被要
    expect(ranges.some((c) => c.seqBase === 150)).toBe(false);
  });

  it("滚动：接上骨架后**每次**滚动都交给 fillVisible —— 不再只在离顶 800px 内才补（占位可以在中部）", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
    const t = replay("scroll");
    await settle();
    const spy = vi.spyOn(t.skeleton!, "fillVisible");
    Object.defineProperty(t.streamEl, "scrollTop", { value: 5000, configurable: true });
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("〔U3b · 步 8〕接上骨架 ⇒ 前端账本只留离尾巴最近的 200 条（〔CF2〕monitor 那一半不再登记）；丢掉的滚到时按偏移要回来、只建卡不重记账", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string, args?: unknown) => {
      if (cmd === "read_session_index") return Promise.resolve(idx(1100));
      if (cmd === "read_session_range") {
        // 同上：这一段几行按夹具的行边界算。
        const a = args as { seqBase: number; offset: number; until: number };
        return Promise.resolve(
          Array.from({ length: (a.until - a.offset) / 10 }, (_, k) => mk("big", a.seqBase + k, `u${a.seqBase + k}`)),
        );
      }
      return Promise.resolve(undefined);
    }) as never));
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
    // ② monitor 重放缓冲不再要前端登记（每个会话都只留尾巴）⇒ 零次
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === "replay_keep_tail_only")).toEqual([]);
    // ③ 滚到被丢掉的那段：按偏移要回来；这些行**见过**（旁路账早记过）⇒ 只建卡，不再走 onLine
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    const { turnEndNotifier } = await import("../../../src/frontend/ui/turn-notify");
    spy.mockClear();
    vi.mocked(turnEndNotifier.observe).mockClear();
    t.skeleton!.ensure(300, 2); // [298, 303)
    await settle();
    expect(recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_range")).toEqual([
      { origin: "<local>", jsonlPath: "/p/big.jsonl", offset: 2980, until: 3030, seqBase: 298 },
    ]);
    expect(spy.mock.calls.map((c) => (c[0] as { seq: number }).seq)).toEqual([298, 299, 300, 301, 302]);
    expect(vi.mocked(turnEndNotifier.observe), "见过的行又走了一遍 onLine 旁路").not.toHaveBeenCalled();
  });

  it("大纲跳转：点到还在占位里的一条 ⇒ 先按 uuid→seq 物化那一段再跳", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
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

  // 数据源换成后端之后，这条路必须照旧通：行是后端清单给的（不是流上攒的），
  //   点**那一行**（不是直接调宿主）⇒ 先按 uuid→seq 物化再跳。
  it("🔴 SE1：大纲的行来自后端清单，点到还在占位里的那一行 ⇒ 先物化那一段再跳", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
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
    ) as never));
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
// 〔T3 / T4〕三格后端事实在 `TabManager` 上真走一遍。
// ==========================================================================
describe("〔U4b〕容器 · 说不清 · 记录没了 —— TabManager 真走", () => {
  let tm: TabManager;
  let disk: Record<string, unknown>;
  /** 记录那一问的桩答案：`undefined` = 抛错（问不到）。它经通道问 `history-record`，`withSessionReads` 译回旧叫法。 */
  let probe: { present: boolean; root: string } | undefined;
  const tabOf = (sid: string): Tab => home(tm).store.tabs.get(sid)!;
  const btn = (): HTMLElement => document.querySelector<HTMLElement>(".tab")!;

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    probe = { present: true, root: "/h/.claude/projects" };
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads(launchRenderShim((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "patch_config") {
        // 写只交补丁；按与 Rust 写口同一份金样的语义应用（`tests/frontend/ui/config-patch-fake.ts`）。
        disk = JSON.parse(applyConfigEdits(JSON.stringify(disk), (args as { edits: Edit[] }).edits));
        return Promise.resolve(undefined);
      }
      if (cmd === "probe_session_record")
        return probe ? Promise.resolve(probe) : Promise.reject(new Error("没有控制通道"));
      return Promise.resolve(undefined);
    }))));
    tm = makeTM();
  });

  it("★ G3：容器事实落在活会话上；早到的暂存、建 tab 时落实；死了之后来的不改死的那一格", () => {
    // 悬停卡不再单列容器那一句：活会话的状态句只说在干什么；容器决定的是右键 / 会话头给哪几样。
    tm.noteContainer("c1", { form: "hosted", host: "tmux", terminal: "tmux-3-7" }); // 早于建 tab
    const t = tm.ensureTab("c1", "/x", "p", "pi");
    expect(t.state).toEqual(LIVE_ATTACHABLE);
    tm.noteContainer("c1", { form: "none" });
    expect(t.state).toEqual(LIVE_RESUMABLE);
    tm.archiveTab("c1");
    tm.noteContainer("c1", { form: "hosted", host: "tmux", terminal: null }); // 晚到：死了的那一格由死的那一刻说了算
    expect(t.state).toEqual(ENDED);
  });

  it("★ 不认识的宿主不被吞：那一格进「终端形式未知」（不当可接回）、日志一条", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const t = tm.ensureTab("c3", "/x", "p", "pi");
    tm.noteContainer("c3", { form: "other", host: "hosted" });
    expect(t.state).toEqual(LIVE_UNKNOWN_HOST);
    expect(warn.mock.calls.filter((c) => String(c[0]).includes("host=hosted")).length).toBe(1);
    warn.mockRestore();
  });

  it("★ G3 行为不回退：活着、只是不在 tmux 里的会话 ≠ 已结束（× 不露、↗ 还在）", () => {
    tm.ensureTab("c2", "/x", "p", "pi");
    tm.noteContainer("c2", { form: "none" });
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
      ["p1", "p2", "p3"].map((sid) => cardText(tm, sid)).join("\n") +
      [...document.querySelectorAll<HTMLElement>(".pin-revived-hint")].map((n) => n.textContent).join("\n");
    expect(said()).toContain(copyText("sessionState.unseen.name"));
    expect(said()).not.toContain(copyText("sessionState.ended.name"));
    // 本机清单报完：p3 在清单里 ⇒ 活；p2 不在 ⇒ 已结束；远端那条不受本机清单影响。
    tm.markOriginSeen(LOCAL_ORIGIN, new Set(["p3"]));
    expect([tabOf("p1").state, tabOf("p2").state, tabOf("p3").state]).toEqual([UNSEEN, ENDED, LIVE]);
    // 远端报完（`listed` 格）：没被宣告过 ⇒ 已结束。正控：这时才出现「已结束」。
    tm.markOriginSeen("pi");
    expect(tabOf("p1").state).toEqual(ENDED);
    expect(said()).toContain(copyText("sessionState.ended.name"));
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
    tm.ensureTab("g1", "/home/u/p", "/p/g1.jsonl", LOCAL_ORIGIN);
    tm.archiveTab("g1");
    probe = { present: false, root: "/h/.claude/projects" };
    const opened = (): number => vi.mocked(invoke).mock.calls.filter(([c]) => c === "open_local_terminal").length;
    await home(tm).actions.resumeTab("g1");
    // 本机后端先判号、渲那一行，开窗之前问记录：不在 ⇒ 不开窗。
    expect(opened()).toBe(0);
    expect(showActionFailureToast).toHaveBeenCalledWith(
      copyText("sessionState.recordGone.title"),
      copyText("sessionState.recordGone.body", { who: copyText("tabSessionActions.who.local"), root: "/h/.claude/projects", sid: "g1" }),
    );
    expect(tabOf("g1").state).toEqual(GONE);
    expect(cardOf(tm, "g1")?.querySelector(".tab-hover-state")?.textContent).toBe(copyText("sessionState.gone.tooltip"));
    probe = { present: true, root: "/h/.claude/projects" };
    await home(tm).actions.resumeTab("g1");
    expect(tabOf("g1").state).toEqual(ENDED);
    expect(opened()).toBe(1);
  });

  // 在 tmux 里那一条的「记录还在不在」由那台在起之前自己问（`sessions-start` 的 `record_gone`，见「单个在 tmux 里 Resume」那组）。
  /** 执行器替身：照生产那样在开窗之前问 `preflight`（那台判出来的号目录，这里不关心），回开没开。 */
  const openedRemote: string[] = [];
  const remoteRunnerAsksPreflight = (): void => {
    openedRemote.length = 0;
    vi.mocked(runRemoteResume).mockImplementation(async (_o, _a, sid, _c, _l, mods) => {
      const m = mods as { preflight?: (d: string | undefined) => Promise<boolean> } | undefined;
      if (m?.preflight && !(await m.preflight(undefined))) return false;
      openedRemote.push(sid);
      return true;
    });
  };

  it("★ G1：远端直连同样先问；问不到 ⇒ 当不知道、照今天的路走（不当「不在」）", async () => {
    remoteRunnerAsksPreflight();
    tm.ensureTab("g2", "/home/pi/proj", "/p/g2.jsonl", "devbox");
    tm.archiveTab("g2");
    probe = { present: false, root: "/home/pi/.claude/projects" };
    await home(tm).actions.resumeTab("g2");
    expect(openedRemote).toEqual([]);
    expect(tabOf("g2").state).toEqual(GONE);
    probe = undefined; // 问不到
    await home(tm).actions.resumeTab("g2");
    expect(openedRemote).toEqual(["g2"]);
    expect(tabOf("g2").state, "问不到不改状态").toEqual(GONE);
  });

  // 「照起但说一句『查不到记录还在不在』；形状不对报两端契约对不上（出声不静默）」。
  it("★ 〔FIX2〕问不到 ⇒ 照起，但说一句查不到；形状不对 ⇒ 那一句说回的认不出（不猜版本）", async () => {
    remoteRunnerAsksPreflight();
    tm.ensureTab("g4", "/home/pi/proj", "/p/g4.jsonl", "devbox");
    tm.archiveTab("g4");
    const title = copyText("tabSessionActions.recordUnknown.title");
    probe = undefined; // 问不到
    await home(tm).actions.resumeTab("g4");
    expect(openedRemote).toEqual(["g4"]);
    expect(vi.mocked(showActionFailureToast).mock.calls.map((c) => c[0])).toEqual([title]);
    vi.mocked(showActionFailureToast).mockClear();
    probe = { present: true } as unknown as { present: boolean; root: string }; // 少一格 ⇒ 形状不对
    await home(tm).actions.resumeTab("g4");
    expect(openedRemote).toEqual(["g4", "g4"]);
    expect(showActionFailureToast).toHaveBeenCalledWith(title, peerVersionSaid("reply_unreadable", "devbox"));
  });

  it("★ G1：可重连的会话记录没了也不落「记录已不在」（终端还在，接得回去）", () => {
    tm.ensureTab("g3", "/x", "p", "pi");
    tm.markTmuxIdle("g3");
    tm.markRecord("g3", false);
    expect(tabOf("g3").state).toEqual(RECONNECTABLE);
  });
});

/**
 * **远端断连 ⇒ 说不清，不是已结束**（「`Unseen` 不许被显示成已结束」）。
 * 会话流 `unseen` 格（机器级）→ `TabManager.markOriginUnseen`，TabManager 真走。
 */
describe("〔GP1〕那台机器看不见了 —— TabManager 真走", () => {
  let tm: TabManager;
  const tabOf = (sid: string): Tab => home(tm).store.tabs.get(sid)!;
  const titles = (): string => [...home(tm).store.tabs.keys()].map((sid) => cardText(tm, sid)).join("\n");

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads(() => Promise.resolve(undefined))));
    tm = makeTM();
  });

  it("★ 活 / 可重连 ⇒ 说不清（字里零处「已结束」）；已结束 / 记录没了不动", () => {
    tm.ensureTab("u1", "/x", "p", "pi"); // 活
    tm.ensureTab("u2", "/x", "p", "pi");
    tm.markTmuxIdle("u2"); // 可重连
    tm.ensureTab("o1", "/x", "p", "mu"); // 别的机器上的活会话
    tm.markOriginUnseen("pi"); // 机器级一格
    expect([tabOf("u1").state, tabOf("u2").state]).toEqual([UNSEEN, UNSEEN]);
    expect(tabOf("o1").state, "别的机器不受牵连").toEqual(LIVE);
    // 两颗的提示句都说「说不清」、零处「已结束」（改之前断连那一刻这里是两句「这个会话已结束」）。
    expect(titles().split(copyText("sessionState.unseen.name")).length - 1).toBe(2);
    expect(titles()).not.toContain(copyText("sessionState.ended.name"));
    // 死透了的不动：已结束 / 记录没了收到 unseen 照旧（正控：这时才出现「已结束」）。
    tm.ensureTab("u3", "/x", "p", "pi");
    tm.archiveTab("u3");
    tm.ensureTab("u4", "/x", "p", "pi");
    tm.archiveTab("u4");
    tm.markRecord("u4", false);
    tm.markOriginUnseen("pi");
    expect([tabOf("u3").state, tabOf("u4").state]).toEqual([ENDED, GONE]);
    expect(titles()).toContain(copyText("sessionState.ended.name"));
  });

  it("★ 重连之后：重宣告的翻回活；那台报完清单、没有它的 ⇒ 已结束", () => {
    tm.ensureTab("r1", "/x", "p", "pi");
    tm.ensureTab("r2", "/x", "p", "pi");
    tm.markOriginUnseen("pi");
    expect([tabOf("r1").state, tabOf("r2").state]).toEqual([UNSEEN, UNSEEN]);
    tm.createSkeletonTab("r1", "/x", "pi"); // 重连后后端初扫重宣告 r1
    tm.markOriginSeen("pi"); // `sessions_replayed` ⇒ 报完了，没有 r2
    expect([tabOf("r1").state, tabOf("r2").state]).toEqual([LIVE, ENDED]);
  });

  it("★ 那台从「报完了」里摘掉：之后才复活的固定 tab 落说不清，不再直接落已结束", async () => {
    tm.markOriginSeen("pi");
    tm.ensureTab("s1", "/x", "p", "pi");
    tm.markOriginUnseen("pi");
    let disk: Record<string, unknown> = {
      tabBar: { pinned: [{ sid: "s2", origin: "pi", title: "S", jsonlPath: "/p/s2.jsonl" }] },
    };
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "patch_config") {
        // 写只交补丁；按与 Rust 写口同一份金样的语义应用（`tests/frontend/ui/config-patch-fake.ts`）。
        disk = JSON.parse(applyConfigEdits(JSON.stringify(disk), (args as { edits: Edit[] }).edits));
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    })));
    await tm.loadPinned();
    expect(tabOf("s2").state).toEqual(UNSEEN);
  });

  // 〔GP1 问 3〕要求：「可重连 → 断连 → 重连后，tmux 里还在的那几条重新宣告为可重连（不是落已结束）」·
  // 转移表（说不清 + idle ⇒ 可重连；已结束 + idle 不动 ⇒ 次序承重）。
  it("〔TL2 · GP1 问 3〕可重连 → 断连（说不清）→ 重连：先重宣告 idle、再报完清单 ⇒ 可重连；tmux 不在的那条 ⇒ 已结束", () => {
    tm.ensureTab("k1", "/x", "p", "pi");
    tm.ensureTab("k2", "/x", "p", "pi");
    tm.markTmuxIdle("k1");
    tm.markTmuxIdle("k2");
    tm.markOriginUnseen("pi");
    expect([tabOf("k1").state, tabOf("k2").state]).toEqual([UNSEEN, UNSEEN]);
    // emitter 那一笔：k1 的 tmux 还在 ⇒ idle 格；k2 不在 ⇒ ended 格；**然后**才是 listed 格。
    tm.markTmuxIdle("k1");
    tm.archiveTab("k2");
    tm.markOriginSeen("pi");
    expect([tabOf("k1").state, tabOf("k2").state]).toEqual([RECONNECTABLE, ENDED]);
    const k1Title = home(tm).bar.tabButtons.get("k1")!.root.title;
    expect(k1Title, "重连后 tmux 还在的那条被说成已结束了").not.toContain(copyText("sessionState.ended.name"));
    // 次序承重的反面（正控）：若先报完清单再宣告 idle ⇒ 已结束收 idle 不动 —— 这正是 monitor 那一侧要保序的理由。
    tm.ensureTab("k3", "/x", "p", "pi2");
    tm.markTmuxIdle("k3");
    tm.markOriginUnseen("pi2");
    tm.markOriginSeen("pi2");
    tm.markTmuxIdle("k3");
    expect(tabOf("k3").state, "次序反了就回不来 —— 若这格变了，monitor 侧的保序就不再承重，回来重看").toEqual(ENDED);
  });

  it("★ 那台一个 tab 都没有时收到 unseen ⇒ 什么都不建", () => {
    tm.markOriginUnseen("nowhere");
    expect(home(tm).store.tabs.size).toBe(0);
  });
});

/**
 * **resume 之前问记录，问的是这次 resume 要用的那个账号根**。
 * 期望的目录 == 交给起会话那一格的目录（同一次解析，两处读同一个值）。
 */
describe("〔GP1〕记录那一问带上这次 resume 的账号根（那台判出来的那个号）", () => {
  let tm: TabManager;
  const probes = (): Record<string, unknown>[] =>
    sessionReadCalls(vi.mocked(invoke).mock.calls, "probe_session_record");

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
  });

  it("★ 远端直连：问的是那台判出来的号的目录（执行器开窗之前交回来的那一个）", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads(() => Promise.resolve(undefined))));
    vi.mocked(runRemoteResume).mockImplementation(async (_o, _a, _s, _c, _l, mods) => {
      const m = mods as { preflight?: (d: string | undefined) => Promise<boolean> } | undefined;
      return (await m?.preflight?.("/h/.claude-alt/z")) ?? true;
    });
    tm.ensureTab("k1", "/home/pi/proj", "/p/k1.jsonl", "devbox");
    tm.archiveTab("k1");
    await home(tm).actions.resumeTab("k1");
    expect(probes().map((p) => p.configDir)).toEqual(["/h/.claude-alt/z"]);
  });

  it("★ 本机：问的是本机后端判出来、随那一行交回来的号目录；账号 0 / 不指定 ⇒ 不带", async () => {
    let account: unknown = { name: "acct-b", configDir: "/h/.claude-alt/acct-b", model: null };
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads(launchRenderShim((cmd: string) => {
      if (cmd === "probe_session_record") return Promise.resolve({ present: true, root: "/h/.claude-alt/acct-b/projects" });
      if (cmd === "resume_history_session") return Promise.resolve({ account });
      return Promise.resolve(undefined);
    }))));
    tm.ensureTab("k2", "/home/u/p", "/p/k2.jsonl", LOCAL_ORIGIN);
    tm.archiveTab("k2");
    await home(tm).actions.resumeTab("k2");
    account = null;
    await home(tm).actions.resumeTab("k2");
    expect(probes().map((p) => p.configDir)).toEqual(["/h/.claude-alt/acct-b", undefined]);
  });
});

// **接线判据**：`main.ts` 起步那几行（`list_active_sessions`〔散文墓碑〕 之后标本机清单报完 ·
// 两个新事件交给 TabManager）没有 DOM 判据够得着（整个 `main.ts` 是入口脚本）⇒ 读源码数调用点，两向恰好一处。
describe("〔U4b〕main.ts 接线", () => {
  // 本机的「清单报完了」不再另走 `list_active_sessions`〔散文墓碑〕 那一格：与远端同一格（会话流里的 `listed`）⇒ 本机那条接线零处。
  it("★ 容器 · 清单报完（本机远端同一格）· 各台都报完那一拍各恰一处；本机不再另拉清单", () => {
    const main = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/main.ts"), "utf8");
    const n = (needle: string): number => main.split(needle).length - 1;
    expect([
      n("tabs.markOriginSeen(LOCAL_ORIGIN,"),
      n("onSessionContainer: (sessionId, container) => tabs.noteContainer(sessionId, container)"),
      n("      tabs.markOriginSeen(origin);\n      if (!all) return;"), // 同一格带壳那一拍「各台都报完」
      n("      tabs.markAllListed();\n      startup?.onAllListed();"), // 那一拍：收空组 · 交「启动时记住的那一格」
    ]).toEqual([0, 1, 1, 1]);
  });
  // 「那台机器看不见了」主窗接一处（入口脚本没有 DOM 判据够得着）；独立查看窗不建标签页，看不见走它那一条订阅（`followSession` 的 `sight`）。
  it("★〔GP1〕session-unseen 接线：main.ts 恰一处 · entry-viewer.ts 零处（不建标签页）", () => {
    const n = (file: string, needle: string): number =>
      readFileSync(resolve(REPO_ROOT, file), "utf8").split(needle).length - 1;
    expect([
      n("src/frontend/ui/main.ts", "onOriginUnseen: (origin) => tabs.markOriginUnseen(origin)"),
      n("src/frontend/ui/entry-viewer.ts", "TabManager"),
    ]).toEqual([1, 0]);
  });
});

/**
 * **没接骨架的 tab 按行号往下取**（`TabStreamView.fetchBelow` · `read_session_lines`）。
 *
 * 要求：「无索引会话的重放缓冲上界（要先有不依赖索引的取回路）」
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
  const asks = () => recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_lines");
  /** 后端答：`[from, until)` 里每一行都可显示（seq 就是行号）。 */
  const answerAll = (sid: string, origin?: string) =>
    withSessionReads((cmd: string, args?: Record<string, unknown>) => {
      const a = args as { from: number; until: number } | undefined;
      return Promise.resolve(
        cmd === "read_session_lines"
          ? {
              from: a!.from,
              next: a!.until,
              eof: false,
              payloads: Array.from({ length: a!.until - a!.from }, (_, k) => mk(sid, a!.from + k, origin)),
            }
          : undefined,
      );
    }) as never;

  it("★ L3：账尽 ＋ 最老那一条不是第 0 行 ⇒ 问 [floor − 200, floor)；回来的进账本、补上屏；问到第 0 行就不再问", async () => {
    pinClock();
    vi.mocked(invoke).mockImplementation(answerAll("lb"));
    tm.onLine(mk("head", 1)); // 首个 tab ⇒ active
    tm.onLine(mk("lb", 300)); // 后台 tab：非批期直渲、钉 floor = 300；账本空
    const t = home(tm).store.tabs.get("lb")!;
    expect(t.window.pendingCount).toBe(0);
    expect(t.window.wantsBelow).toBe(true);
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    tm.switchTo("lb"); // jsdom 恒不可滚 ⇒ R-2 踢一脚
    await new Promise((r) => requestAnimationFrame(() => r(null))); // 切进来那一脚补批在下一帧（同步段不读几何）
    expect(asks()).toEqual([{ origin: "<local>", jsonlPath: "/p/lb.jsonl", from: 100, until: 300, leftMs: 60_000 }]);
    await settle();
    // 回来的 200 条补上了屏（渲染窗口向下扩到 100）；之后接着问 [0, 100)，到第 0 行为止
    expect(t.window.floorSeq).toBe(0);
    expect(asks()).toEqual([
      // `leftMs`：往上翻一件一问，交整份（`TabStreamView.BELOW_BUDGET_MS`）
      { origin: "<local>", jsonlPath: "/p/lb.jsonl", from: 100, until: 300, leftMs: 60_000 },
      { origin: "<local>", jsonlPath: "/p/lb.jsonl", from: 0, until: 100, leftMs: 60_000 },
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

  it("★ L3：问回来的那一段全是不显示的记录（floor 不动）⇒ 下一问接着往下、不原地重问；问到第 0 行为止、之后再踢也不问", async () => {
    // 后端答：那一段一条可显示的都没有（seq 里本来就有不显示的记录占的号）
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: Record<string, unknown>) => {
      const a = args as { from: number; until: number } | undefined;
      return Promise.resolve(
        cmd === "read_session_lines" ? { from: a!.from, next: a!.until, eof: false, payloads: [] } : undefined,
      );
    }) as never);
    tm.onLine(mk("head", 1));
    tm.onLine(mk("nd", 450)); // floor = 450
    const t = home(tm).store.tabs.get("nd")!;
    tm.switchTo("nd");
    await settle();
    expect(
      asks().map((a) => [(a as { from: number }).from, (a as { until: number }).until]),
      "floor 不动时按 floor 原地重问 ⇒ 无限循环；这里要的是一问接一问往下，到 0 为止",
    ).toEqual([
      [250, 450],
      [50, 250],
      [0, 50],
    ]);
    expect(t.window.floorSeq, "一条可显示的都没取回来，floor 不该动").toBe(450);
    expect(t.window.belowState).toEqual({ kind: "none" });
    home(tm).view.activate(t);
    await settle();
    expect(asks().length, "到顶了还在问").toBe(3);
  });

  it("★ L3：问不动（老后端 / 断了）⇒ 〔GAP1〕下一次上翻再问一次；连续第二次才哨兵说原因、不再自动重问；切走再切回来才再问一次", async () => {
    pinClock();
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string) =>
      cmd === "read_session_lines"
        ? Promise.reject(new Error("那台后端还不认这条查询"))
        : Promise.resolve(undefined)) as never);
    tm.onLine(mk("head", 1));
    tm.onLine(mk("fb", 50));
    const t = home(tm).store.tabs.get("fb")!;
    tm.switchTo("fb");
    await settle();
    expect(asks().length).toBe(1);
    // 第一次失败：不定死，下一次触发点（上翻）再问同一段 —— 恰好一次。
    expect(t.window.belowState).toEqual({ kind: "maybe" });
    (home(tm).view as unknown as { fillAbove(t: unknown): void }).fillAbove(t);
    await settle();
    expect(asks().length, "第一次失败之后的下一次上翻该再问一次").toBe(2);
    expect(asks()[1]).toEqual(asks()[0]);
    // 失败经通道那一跳说（`chan-caller.ts::saidFrom` 带上对端的码），原因那一句原样在里面。
    expect(t.window.belowState).toEqual({ kind: "failed", reason: expect.stringContaining("那台后端还不认这条查询") });
    expect(t.stream.contentElement.querySelector(".stream-more-above")?.textContent).toContain(
      "那台后端还不认这条查询",
    );
    // 连续第二次失败之后：上翻（fillAbove 的每一个入口）不自动重问
    (home(tm).view as unknown as { fillAbove(t: unknown): void }).fillAbove(t);
    (home(tm).view as unknown as { fillAbove(t: unknown): void }).fillAbove(t);
    expect(asks().length, "连续两次失败之后的上翻不许自己重问（否则是一个无界的重试环）").toBe(2);
    home(tm).view.activate(t);
    await new Promise((r) => requestAnimationFrame(() => r(null))); // 切进来那一脚补批在下一帧（同步段不读几何）
    expect(asks().length, "activate 自己就是「切进来」—— 这一脚允许重问").toBe(3);
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
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const rendered = (renderContentRecord as unknown as ReturnType<typeof vi.fn>).mock.calls.map(
      (c) => (c[0] as { seq: number }).seq,
    );
    expect(rendered, "见过的那一条也要上屏（放回账本 ⇒ 补批建卡）").toContain(240);
  });

  it("★ S3′：会话流丢过格（`onStreamGap`）⇒ 那台机器的 tab：账本整份出账、从见过的最大行号 + 1 起往后取到 eof；别的机器的 tab 不动", async () => {
    const pages = new Map<string, number>();
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: Record<string, unknown>) => {
      const a = args as { jsonlPath: string; from: number; until?: number } | undefined;
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
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
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
    // `leftMs` 另判（下面那条「一件事一个总期限」）：这里只看问的是哪几段。
    const noLeft = (a: object): object => {
      const { leftMs: _left, ...rest } = a as { leftMs?: number };
      return rest;
    };
    expect(of("/p/gp.jsonl").map(noLeft)).toEqual([
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

  /**
   * 〔「一次调用一个绝对时刻……`Duration` 跨跳传递时每一跳都会重新开始计时 —— 那正是病 2 的机制」〕
   * **往后补是一件事、一个总期限**：每一问交的是「那一件还剩多少」（越往后越少，不重新计时）；
   * 总期限过了还没到末尾 ⇒ 不再问（停下、记一行）。正控：期限之内、到末尾就停（上面那条 S3′）。
   * 钟面用替身（`performance.now` 每被读一次走 25 秒），不等真时间。
   */
  it("★ DL1：丢格之后往后补 —— 每问交剩下的、越来越少；总期限一过就不再问", async () => {
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: Record<string, unknown>) =>
      Promise.resolve(
        cmd === "read_session_lines"
          ? // 第 30 行到头：期限一直在的话（每页重新计时那一形）会一路问到这里 —— 问 20 次、干净地红，而不是无限问下去把 worker 撑爆
            { from: Number(args!.from), next: Number(args!.from) + 1, eof: Number(args!.from) >= 30, payloads: [mk("dl", Number(args!.from))] }
          : undefined,
      )) as never);
    tm.onLine(mk("dl", 10));
    let clock = 1_000_000;
    const now = vi.spyOn(performance, "now").mockImplementation(() => (clock += 25_000));
    try {
      tm.onStreamGap("<local>");
      await settle();
    } finally {
      now.mockRestore();
    }
    const lefts = asks()
      .filter((a) => (a as { jsonlPath: string }).jsonlPath === "/p/dl.jsonl")
      .map((a) => (a as { leftMs: number }).leftMs);
    expect(lefts.length, "一问都没问 / 总期限没生效（一直在问）").toBeGreaterThan(0);
    expect(lefts.length, "总期限 120 秒、钟每读一次走 25 秒 —— 问不过 5 次").toBeLessThanOrEqual(5);
    expect(lefts[0], "第一问交的不是那一件的整份（减去起算到第一问之间走的那一格）").toBeLessThanOrEqual(120_000);
    for (let i = 1; i < lefts.length; i++) {
      expect(lefts[i], `第 ${i + 1} 问没比上一问少 —— 每页重新计时了`).toBeLessThan(lefts[i - 1]);
    }
    expect(lefts.every((l) => l > 0), "过了期限还在问").toBe(true);
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
 * **前端账本有上界**（`live-window.ts::PENDING_CAP` / `PENDING_KEEP`）。
 *
 * 要求：「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界，满了必须落级 1 或级 2，
 * **不许静默堆**」—— 出账的那些往上翻时按行号取回（上面那一组 L3），所以这是级 2 不是「丢了就没了」。
 */
describe("〔CF2〕前端账本的上界", () => {
  it("★ 超过 CAP 就只留 seq 最高的 KEEP 条（乱序到达也按 seq 留）；「到顶了」随之回到「下面可能还有」", async () => {
    const { TailWindow, PENDING_CAP, PENDING_KEEP } = await import("../../../src/frontend/ui/live-window");
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

// ==========================================================================
// bg 会话平铺：拖拽 / 集合 / 落位对 bg tab 与普通 tab **行为相同**
//
// 守的要求：用户原话 ——「后台（bg）会话不再自动挂到宿主下排成树：
//   删 `placeInTree` 树状挂载、`dragBlockOf` 拖拽例外、`.tab-bg` 那套；bg 会话就是普通 tab、平铺」；
// 「自动归组 / 自动固定 —— 手动建，不要自动，纯手动」。
//
// 形状：**同一组用例两种 tab 各跑**（`describe.each`）。「同 cwd 的那一颗」（`sub`）一跑是普通 tab、
//   一跑是 bg；期望是**同一份手写表**（不从实现生成）。宿主 `host` 与 `sub` 同 `(cwd, origin)` ——
//   正是旧树认「宿主 ＋ bg 子串」的那个布景，所以每一格在树还在时 bg 那一跑都会红（起步 `e4cc3c23` 现打）。
// ⚠ 标题不在本组里：`⚙ 任务名` 标题没要求删，两跑标题本来就不同。
// ==========================================================================
describe.each([
  ["普通 tab", false],
  ["bg tab", true],
])("同 cwd 的那一颗是%s ⇒ 落位 / 拖拽 / 集合与另一跑逐字相同", (_label, subKind) => {
  let tm: TabManager;
  const order = (): string[] => home(tm).store.orderedIds;
  const colsOf = (): TabCollection[] => home(tm).prefs.collections;
  /** `host` 与 `sub` 同 cwd；`other` 另一个项目。三颗按 `arriving` 的次序宣告。 */
  const arrive = (arriving: string[]): void => {
    for (const sid of arriving) {
      if (sid === "host") tm.createSkeletonTab("host", "/proj/a", LOCAL_ORIGIN, false, null);
      if (sid === "sub") tm.createSkeletonTab("sub", "/proj/a", LOCAL_ORIGIN, subKind, "任务");
      if (sid === "other") tm.createSkeletonTab("other", "/proj/z", LOCAL_ORIGIN, false, null);
    }
  };
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    home(tm).prefs.collectionsLoaded = true;
  });

  it("① 到达落位 == 到达序（不挂到同 cwd 那颗后面）", () => {
    arrive(["host", "other", "sub"]);
    expect(order()).toEqual(["host", "other", "sub"]);
    tm = makeTM();
    arrive(["sub", "other", "host"]);
    expect(order(), "先到的那颗不被后到的同 cwd 那颗拉走").toEqual(["sub", "other", "host"]);
  });

  it("② 真拖拽：拖 host 到最下面 ⇒ 只有 host 动", () => {
    arrive(["host", "sub", "other"]);
    (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
    const bar = document.body.firstElementChild as HTMLElement;
    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    roots.forEach((el, i) => {
      el.getBoundingClientRect = () =>
        ({ top: i * 40, height: 40, bottom: i * 40 + 40, left: 0, right: 100 }) as DOMRect;
    });
    expect(order(), "前置：三颗按到达序").toEqual(["host", "sub", "other"]);
    roots[0].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 500, bubbles: true }),
    );
    document.dispatchEvent(new MouseEvent("mouseup", { clientX: 10, clientY: 500, bubbles: true }));
    expect(order()).toEqual(["sub", "other", "host"]);
  });

  it("③ 拖 host `onto` other 成组 ⇒ 组里只有这两颗", () => {
    arrive(["host", "sub", "other"]);
    home(tm).dragger.applyDrop("host", { kind: "onto", sid: "other" });
    expect(colsOf().map((c) => membersOf(tm, c.id))).toEqual([["host", "other"]]);
    expect(order()).toEqual(["sub", "host", "other"]);
  });

  it("④ host 与 sub 同组，把 host 拖出组 ⇒ sub 留在组里", () => {
    arrive(["host", "sub", "other"]);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["host", "sub"] }]);
    home(tm).dragger.applyDrop("host", { kind: "end" });
    expect(colsOf()).toEqual([{ id: "g1", name: "白天" }]);
    expect(membersOf(tm, "g1")).toEqual(["sub"]);
    expect(order()).toEqual(["sub", "other", "host"]);
  });

  it("⑤ 同 sid 后到一份 interactive 宣告（升格）⇒ 位置不动", () => {
    arrive(["host", "sub", "other"]);
    tm.createSkeletonTab("sub", "/proj/a", LOCAL_ORIGIN, false, null);
    expect(home(tm).store.tabs.get("sub")!.background).toBe(false);
    expect(order()).toEqual(["host", "sub", "other"]);
  });
});

// ════════════════════════════════════════════════════════════════════════════
// 会话事实：**后端给了什么 ⇒ tab 上是什么**
// ════════════════════════════════════════════════════════════════════════════
//
// 分叉血缘 · 改动文件集 · 最新 usage 由后端出成品（帧命令 `history-facts`，`session-reads.ts` 第五问）。
// 口径（首条锁定 · 近因序 · 上界 · 取文件序最后一条）全在后端判据（`tests/backend/observe/facts_query_tests.rs`）；
// 这一组只钉前端那一侧：成品原样落到 tab 上、原样当续传令牌交回去、只刷变了的那几块、要不到就出声。
// 子 agent 的列表与状态不在会话事实里（运行表，`tests/frontend/ui/runs.vitest.ts`）。
// 夹具只造结构（sid / 路径 / 占位 id），不采会话正文。
describe("〔STC〕会话事实：后端给了什么 ⇒ tab 上是什么", () => {
  const facts = (p: Partial<SessionFacts> = {}): SessionFacts => ({
    agent: "claude",
    end: 100,
    forkedFrom: null,
    touchedFiles: [],
    usage: null,
    projectDir: null,
    writers: [],
    pending: [],
    lastSay: null,
    needs: null,
    handedBack: [],
    retries: [],
    ...p,
  });
  const line = (sid: string, seq: number, origin: string | null = null) =>
    ({
      session_id: sid,
      cwd: "/home/u/proj",
      path: `/home/u/proj/${sid}.jsonl`,
      seq,
      origin,
      message: { type: "user", uuid: `${sid}-${seq}` },
    }) as never;
  /** 等在途的那几趟通道往返落完（每趟是若干个微任务 ＋ 一次宏任务，多等几轮）。 */
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
  };
  /** 记下每一问的 `(origin, path, prior)`；答法由 `answer` 现算（抛 ⇒ 通道那一跳的失败形状原样抛）。 */
  let asked: { origin: string; path: string; prior: unknown }[] = [];
  const answerFacts = (answer: (path: string, prior: unknown) => unknown): void => {
    asked = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (!isChanCall(cmd, args, "history-facts")) return undefined;
      const body = chanArgsJson(args) as { path: string; prior?: unknown };
      asked.push({ origin: args.origin, path: body.path, prior: body.prior ?? null });
      return chanReply(answer(body.path, body.prior ?? null));
    });
  };
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  afterEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  it("#63① 成品说是分叉来的 ⇒ tab 记下来源 sid、标题加 ↳；tooltip 标出来源 sid 前 8 位", async () => {
    answerFacts(() => facts({ forkedFrom: "abcd1234-parent" }));
    tm.onLine(line("fork-sid", 0));
    await settle();
    const tab = home(tm).store.tabs.get("fork-sid")!;
    expect(tab.forkedFromSessionId).toBe("abcd1234-parent");
    expect(tab.title.startsWith("↳ ")).toBe(true);
    expect(cardText(tm, "fork-sid"), "悬停卡标出来源 sid 前 8 位").toContain(copyText("tabBar.hover.forked", { id: "abcd1234" }));
  });

  it("★ 成品说是哪一家 ⇒ tab 记下它（之前不知道 ⇒ `null`，不落 claude）；先画出来的卡头那一格补上那一家的短名", async () => {
    (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = true;
    try {
      answerFacts(() => facts({ agent: "codex" }));
      tm.onLine(line("cx-sid", 0));
      const tab = home(tm).store.tabs.get("cx-sid")!;
      expect(tab.agent, "事实到之前不知道是哪一家").toBeNull();
      const card = document.createElement("div");
      card.className = "card card-assistant";
      card.innerHTML = '<div class="card-header"><span class="role"></span></div>';
      tab.streamEl.appendChild(card);
      const head = card.querySelector<HTMLElement>(".role")!;
      await settle();
      expect(tab.agent).toBe("codex");
      expect(head.textContent, "卡头那一格补上那一家的短名（画像里的 speakerName）").toBe("Codex");
    } finally {
      (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = false;
    }
  });

  it("成品说不止一个进程在写这条会话 ⇒ tab 的悬停提示多一行；一个 ⇒ 不说；会话结束了 ⇒ 不说", async () => {
    let writers = [4242, 5252];
    answerFacts(() => facts({ writers }));
    tm.onLine(line("dup", 0));
    await settle();
    const tab = home(tm).store.tabs.get("dup")!;
    const tip = () => cardText(tm, "dup").split("\n");
    const said = copyText("tabBarView.tab.writers", { n: 2 });
    expect(tab.writers).toEqual([4242, 5252]);
    expect(tip()).toContain(said);
    writers = [4242];
    tm.onLine(line("dup", 1));
    await settle();
    expect(tip()).not.toContain(said);
    writers = [4242, 5252];
    tm.onLine(line("dup", 2));
    await settle();
    expect(tip()).toContain(said);
    tm.archiveTab("dup");
    expect(tip()).not.toContain(said);
  });

  // 刚起的会话宣告时还没有记录（或老后端宣告不带）⇒ 后端读到记录开头之后经会话事实给项目目录，标题跟上；行里的 cwd 不读。
  it("成品带项目目录 ⇒ tab 记下、标题跟上；没带（null）⇒ 不动 tab 上那一份", async () => {
    answerFacts(() => facts({ projectDir: "/a/proj" }));
    tm.onLine({ ...(line("pd", 0) as object), cwd: "/a/proj/sub" } as never);
    await settle();
    const tab = home(tm).store.tabs.get("pd")!;
    expect(tab.projectDir).toBe("/a/proj");
    expect(tab.title).toBe("proj");
    answerFacts(() => facts({ end: 200 }));
    tm.onLine({ ...(line("pd", 1) as object), cwd: "/a/proj/sub" } as never);
    await settle();
    expect(tab.title).toBe("proj");
  });

  it("#63① 成品说不是分叉 ⇒ 不加徽标（区分性）", async () => {
    answerFacts(() => facts());
    tm.onLine(line("plain", 0));
    await settle();
    const tab = home(tm).store.tabs.get("plain")!;
    expect(tab.forkedFromSessionId).toBeNull();
    expect(tab.title.startsWith("↳")).toBe(false);
  });

  it("#63① 分叉 ＋ 后到的 aiTitle ⇒ 恰一个 ↳；远端分叉 ↳ 在 [origin] 之外", async () => {
    answerFacts(() => facts({ forkedFrom: "parent-x" }));
    tm.createSkeletonTab("f3", "/home/u/proj", LOCAL_ORIGIN); // 项目目录由会话宣告给
    tm.onLine(line("f3", 0));
    tm.onLine(line("fr", 0, "pi"));
    await settle();
    const tab = home(tm).store.tabs.get("f3")!;
    (tm as unknown as { applyAiTitle(t: Tab, s: string): void }).applyAiTitle(tab, "我的功能");
    expect(tab.title).toBe("↳ [proj] 我的功能");
    expect((tab.title.match(/↳/g) ?? []).length).toBe(1);
    expect(home(tm).store.tabs.get("fr")!.title.startsWith("↳ [pi] ")).toBe(true);
  });

  it("续传：第二问把上一份成品**原样**当 prior 交回去；成品整份替换（后端说什么就是什么）", async () => {
    let n = 0;
    const first = facts({ end: 10, forkedFrom: "p1", touchedFiles: ["/a"] });
    const second = facts({ end: 20, forkedFrom: "p1", touchedFiles: ["/b", "/a"] });
    answerFacts(() => (n++ === 0 ? first : second));
    tm.onLine(line("s", 0));
    await settle();
    tm.onLine(line("s", 1));
    await settle();
    expect(asked.map((a) => a.prior)).toEqual([null, first]);
    const tab = home(tm).store.tabs.get("s")!;
    expect([...tab.touchedFiles]).toEqual(["/b", "/a"]);
  });

  it("带着 prior 要不到（续点越过文件尾 = 截断 / 重写）⇒ 不带 prior 从 0 重要一趟", async () => {
    let n = 0;
    answerFacts((_p, prior) => {
      n++;
      if (n === 2 && prior !== null) throw { err: "Refused", body: [...new TextEncoder().encode('{"code":"failed","message":"past EOF"}')] };
      return facts({ end: n * 10, touchedFiles: [`/f${n}`] });
    });
    tm.onLine(line("t", 0));
    await settle();
    tm.onLine(line("t", 1));
    await settle();
    expect(asked.map((a) => a.prior === null)).toEqual([true, false, true]);
    expect([...home(tm).store.tabs.get("t")!.touchedFiles]).toEqual(["/f3"]);
  });

  it("批期不问；批结束每个「没要过或又长了」的 tab 各问一次（不只 active）", async () => {
    answerFacts((path) => facts({ forkedFrom: path.includes("b1") ? "src" : null }));
    tm.onBatchStart();
    tm.onLine(line("b1", 0));
    tm.onLine(line("b1", 1));
    tm.onLine(line("b2", 0));
    await settle();
    expect(asked).toEqual([]);
    tm.onBatchEnd();
    await settle();
    expect(asked.map((a) => a.path).sort()).toEqual(["/home/u/proj/b1.jsonl", "/home/u/proj/b2.jsonl"]);
    expect(home(tm).store.tabs.get("b1")!.forkedFromSessionId).toBe("src"); // 后台 tab 也有 ↳
  });

  it("改动文件集：成品原样透传给监控板 peek（近因序由后端排）；远端问的是那台", async () => {
    answerFacts(() => facts({ touchedFiles: ["/proj/b.rs", "/proj/a.ts"] }));
    tm.onLine(line("s-local", 0));
    tm.onLine(line("s-remote", 0, "devbox"));
    await settle();
    expect(tm.peekSession("s-local")!.recentFiles).toEqual(["/proj/b.rs", "/proj/a.ts"]); // F91b：尾 = 最近改
    expect(asked.find((a) => a.path.includes("s-remote"))?.origin).toBe("devbox"); // 远端问的是那台
  });

  it("F88b usage：active 的成品一到就推给 HUD；后台 tab 不推；切过去时推那一格", async () => {
    const seen: [string | null, number | null][] = [];
    tm.active.subscribe((a) => seen.push([a.model, a.promptTokens])); // 订阅 store（原先是回调）
    answerFacts((path) =>
      facts({ usage: path.includes("u1") ? usageOf(42, "m-a") : usageOf(7, null) }),
    );
    tm.onLine(line("u1", 0)); // 第一个 tab 自动成为 active
    tm.onLine(line("u2", 0));
    await settle();
    // 切换那一格的推送在 rAF 里（`switchTo`），与事实到达谁先谁后不定 ⇒ 等「最后一次」落定，不钉次序。
    await vi.waitFor(() => expect(seen.at(-1)).toEqual(["m-a", 42]));
    expect(seen.some(([, t]) => t === 7)).toBe(false); // 后台 tab 的事实到了不推
    tm.switchTo("u2");
    await vi.waitFor(() => expect(seen.at(-1)).toEqual([null, 7]));
    expect(home(tm).store.tabs.get("u2")!.latestPromptTokens).toBe(7); // 监控板那一格读的也是它（`snapshotSessions`）
    expect(tm.peekSession("u1")!.model).toBe("m-a");
  });

  // 「当前 tab 变了」改订阅 store 之后的时机差：同值不通知（原先两个回调同值也照调）。
  it("★ 〔GAP1〕当前 tab 那一格同值不通知：可用性重报一次同样的值 ⇒ 零通知；切到别的 tab ⇒ 恰一次", async () => {
    answerFacts((path) => facts({ usage: path.includes("v1") ? usageOf(5, "m") : null }));
    tm.onLine(line("v1", 0));
    tm.onLine(line("v2", 0));
    await settle();
    await vi.waitFor(() => expect(tm.active.get().promptTokens).toBe(5));
    const seen: unknown[] = [];
    tm.active.subscribe((a) => seen.push(a));
    (tm as unknown as { onFactsAvailability(sid: string): void }).onFactsAvailability("v1");
    expect(seen, "值没变却通知了").toEqual([]);
    tm.switchTo("v2");
    await vi.waitFor(() => expect(seen.length).toBe(1));
    expect(seen).toEqual([{ sid: "v2", model: null, promptTokens: null, contextLimit: null, limitFrom: "assumed", unavailable: null, projectDir: null }]);
  });

  it("要不到（老后端不认这条命令）⇒ active 的 HUD 出声（原因非空）、此后不再问；可用 ⇒ 说 null", async () => {
    const said: (string | null)[] = [];
    tm.active.subscribe((a) => said.push(a.unavailable)); // 订阅 store（原先是回调）
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (isChanCall(cmd, args, "history-facts")) throw UNSUPPORTED;
      return undefined;
    });
    tm.onLine(line("old", 0));
    await settle();
    const calls = () => vi.mocked(invoke).mock.calls.filter((c) => isChanCall(c[0] as string, c[1], "history-facts")).length;
    expect(calls()).toBe(1);
    expect(said.at(-1)).toMatch(copyPattern("peerVersion.said.old"));
    tm.onLine(line("old", 1));
    await settle();
    expect(calls()).toBe(1); // 结构性失败 ⇒ 不再问
    answerFacts(() => facts());
    tm.onLine(line("fine", 0));
    await settle();
    tm.switchTo("fine");
    await vi.waitFor(() => expect(said.at(-1)).toBeNull()); // 切换那一格的推送在 rAF 里
  });
});

// ===== 活会话的记录文件不见了 / 被改过 ⇒ 那个 tab 顶上说一句，不碰会话状态 =====
// 要求：「删了 / 改名 ⇒ 出声（该 tab 说一句『记录文件不见了』），不崩、不误判结束」。
// 接线（`record-file-notice.ts::recordFileWiring`）喂的是**真的** `TabManager.streamElOf` 与 `onLine`，与 `main.ts` 同一形。
describe("〔FW1〕记录文件的出声", () => {
  const mk = (seq: number) => ({
    session_id: "rf-sid",
    cwd: "/p",
    path: "/p/rf-sid.jsonl",
    seq,
    message: { type: "assistant", uuid: `rf-${seq}` } as never,
  });
  const noticeText = (tm: TabManager): string | null => {
    const first = tm.streamElOf("rf-sid")?.firstElementChild as HTMLElement | null | undefined;
    return first && first.dataset.recordFileNotice !== undefined ? first.textContent : null;
  };

  it("「不见了」画在那个 tab 顶上、会话状态不动；又来一行 ⇒ 收掉；「已从头重读」那句留着", () => {
    const tm = makeTM();
    const rf = recordFileWiring((sid) => tm.streamElOf(sid));
    const line = (seq: number): void => {
      tm.onLine(mk(seq) as never);
      rf.afterLine("rf-sid");
    };
    line(0);
    const before = home(tm).store.tabs.get("rf-sid")!.state;
    rf.onSessionFileNotice("rf-sid", "gone");
    expect(noticeText(tm)).toBe(copyText("sessionState.recordFile.gone"));
    expect(home(tm).store.tabs.get("rf-sid")!.state, "记录文件不见了就改了会话状态（误判结束）").toEqual(before);
    line(1);
    expect(noticeText(tm), "又来了一行，「不见了」那句还挂着").toBeNull();
    rf.onSessionFileNotice("rf-sid", "rewritten");
    line(2);
    expect(noticeText(tm), "重读那句被下一行收掉了").toBe(copyText("sessionState.recordFile.rewritten"));
    rf.onSessionFileNotice("rf-sid", "truncated");
    expect(noticeText(tm), "新的一句没盖掉旧的").toBe(copyText("sessionState.recordFile.truncated"));
    // 认不出的取值 / 没有这个 tab ⇒ 不画、不抛。
    rf.onSessionFileNotice("rf-sid", "moved");
    expect(noticeText(tm)).toBe(copyText("sessionState.recordFile.truncated"));
    expect(() => rf.onSessionFileNotice("no-such-sid", "gone")).not.toThrow();
  });
});

// ==========================================================================
// 守原话「分组不应该单独存会话记录. x就是没了, 不存在还要移出分组」。
// 四条判据：J1 盘上零处会话 id 名单形状的组员表（带正控）· J2 × 之后盘与内存零残留 ·
// J3 最后一个成员关掉组消失 · J4 重启后只有还在栏里的 tab 带组。真 TabManager ＋ 内存假盘，读盘对拍。
// ==========================================================================
/** 盘上所有「装着 `sids` 里某个 sid 的数组」的路径（点分）—— J1 的扫描。 */
function sidArrays(v: unknown, sids: ReadonlySet<string>, path = ""): string[] {
  if (Array.isArray(v)) {
    const here = v.some((x) => typeof x === "string" && sids.has(x)) ? [path] : [];
    return [...here, ...v.flatMap((x, i) => sidArrays(x, sids, `${path}.${i}`))];
  }
  if (v && typeof v === "object") {
    return Object.entries(v).flatMap(([k, x]) => sidArrays(x, sids, path ? `${path}.${k}` : k));
  }
  return [];
}

describe("组员关系是 tab 自己的属性", () => {
  let tm: TabManager;
  let disk: Record<string, unknown>;
  const flushDisk = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
  const groupOfOnDisk = (): Record<string, string> =>
    ((disk.tabBar as Record<string, unknown> | undefined)?.groupOf ?? {}) as Record<string, string>;
  const add = (...sids: string[]): void => {
    for (const sid of sids) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL_ORIGIN);
  };
  const close = (sid: string): void => {
    tm.archiveTab(sid); // × 只关已结束的
    tm.closeTab(sid);
  };
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "patch_config") {
        disk = JSON.parse(applyConfigEdits(JSON.stringify(disk), (args as { edits: Edit[] }).edits));
      }
      return Promise.resolve(undefined);
    }));
    tm = makeTM();
  });

  it("J1 · 走遍写组的入口，盘上零处 sid 名单形状的组员表（组表每项恰 {id,name}；装 sid 的数组只有顺序表）", async () => {
    const sids = new Set(["a", "b", "c", "d"]);
    disk = { tabCollections: [{ id: "old", name: "旧", members: ["a", "b"] }] };
    expect(sidArrays(disk, sids), "正控：同一把扫描认得出旧形状").toEqual(["tabCollections.0.members"]);
    await tm.loadCollections();
    add("a", "b", "c", "d");
    const check = async (step: string): Promise<void> => {
      await flushDisk();
      for (const g of disk.tabCollections as object[]) expect(Object.keys(g).sort(), step).toEqual(["id", "name"]);
      expect(sidArrays(disk, sids).filter((p) => p !== "tabBar.order"), step).toEqual([]);
    };
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" }); // 拖放现建
    await check("拖放现建");
    const g = home(tm).store.tabs.get("a")!.group!;
    home(tm).dragger.applyDrop("c", { kind: "before", sid: "a" }); // 拖放进组
    await check("拖放进组");
    home(tm).dragger.applyDrop("c", { kind: "end" }); // 拖出
    await check("拖出");
    void home(tm).prefs.joinGroup("d", g);
    void home(tm).prefs.renameGroup(g, "改");
    await check("右键加入 · 改名");
    close("d");
    await check("×");
    void home(tm).prefs.leaveGroup("a");
    void home(tm).prefs.dissolveGroup("old");
    await check("右键移出 · 解散");
    expect(sidArrays(disk, sids), "两向：顺序表确实被扫到了").toEqual(["tabBar.order"]);
  });

  it("J2 · × 之后盘与内存零残留；同 sid resume 回来是散 tab（正控：意图里的 c 到了照样进组）", async () => {
    disk = { tabCollections: [{ id: "g", name: "白天" }], tabBar: { groupOf: { a: "g", b: "g", c: "g" } } };
    await tm.loadCollections();
    add("a", "b");
    const hits = (): number =>
      JSON.stringify({ t: disk.tabCollections, g: groupOfOnDisk() }).split('"a"').length - 1;
    expect(hits(), "正控：× 之前恰一处 groupOf.a").toBe(1);
    close("a");
    await flushDisk();
    expect(hits()).toBe(0);
    expect(groupOfOnDisk()).toEqual({ b: "g", c: "g" });
    expect(home(tm).store.tabs.has("a")).toBe(false);
    expect(home(tm).prefs.savedGroupOf.has("a")).toBe(false);
    expect(membersOf(tm, "g")).toEqual(["b"]);
    add("a", "c");
    expect(home(tm).store.tabs.get("a")!.group, "resume 回来不许回组").toBeNull();
    expect(home(tm).store.tabs.get("c")!.group).toBe("g");
  });

  it("J3 · 组里最后一个在栏里的关掉 ⇒ 组消失（盘 · 内存 · 栏），指向它的意图一并摘；还有成员时不许消失", async () => {
    disk = { tabCollections: [{ id: "g", name: "白天" }], tabBar: { groupOf: { a: "g", b: "g", x: "g" } } };
    await tm.loadCollections();
    add("a", "b");
    const bar = document.body.firstElementChild as HTMLElement;
    close("a");
    await flushDisk();
    expect(disk.tabCollections, "还有 b ⇒ 组在").toEqual([{ id: "g", name: "白天" }]);
    expect(bar.querySelectorAll(".tab-group")).toHaveLength(1);
    close("b");
    await flushDisk();
    expect(disk.tabCollections).toEqual([]);
    expect(groupOfOnDisk(), "没到的 x 指向它的那条也摘").toEqual({});
    expect(home(tm).prefs.collections).toEqual([]);
    expect([...home(tm).prefs.savedGroupOf]).toEqual([]);
    expect(bar.querySelectorAll(".tab-group")).toHaveLength(0);
  });

  it("J3b · 拖出 / 右键移出 / 挪去别组：组里在栏里的都走了 ⇒ 组消失（盘 · 内存）；还有一个 ⇒ 在", async () => {
    disk = {
      tabCollections: [
        { id: "g", name: "白天" },
        { id: "h", name: "夜里" },
      ],
      tabBar: { groupOf: { a: "g", b: "g", c: "h", x: "g" } },
    };
    await tm.loadCollections();
    add("a", "b", "c");
    const ids = (): string[] => (disk.tabCollections as { id: string }[]).map((g) => g.id);
    home(tm).dragger.applyDrop("a", { kind: "end" }); // 拖出，b 还在
    await flushDisk();
    expect(ids(), "还有 b ⇒ g 在").toEqual(["g", "h"]);
    void home(tm).prefs.leaveGroup("b"); // 右键移出最后一个
    await flushDisk();
    expect(ids()).toEqual(["h"]);
    expect(groupOfOnDisk(), "没到的 x 指向 g 的那条也摘").toEqual({ c: "h" });
    home(tm).dragger.applyDrop("c", { kind: "onto", sid: "a" }); // 挪去与 a 现建的新组，h 空了
    await flushDisk();
    expect(ids().includes("h"), "挪走最后一个 ⇒ h 消失").toBe(false);
    expect(home(tm).prefs.collections.map((g) => g.id)).toEqual(ids());
  });

  it.each([
    ["固定复活先于读组表", true],
    ["读组表先于固定复活", false],
  ])("J4 · 重启后只有还在栏里的 tab 带组（%s）", async (_label, pinnedFirst) => {
    disk = {
      tabCollections: [
        { id: "G", name: "甲" },
        { id: "H", name: "乙" },
      ],
      tabBar: {
        groupOf: { a: "G", b: "G", c: "H", p: "H" },
        pinned: [{ sid: "p", jsonlPath: "/j/p", origin: LOCAL_ORIGIN, title: "P" }],
      },
    };
    if (pinnedFirst) {
      await tm.loadPinned();
      await tm.loadCollections();
    } else {
      await tm.loadCollections();
      await tm.loadPinned();
    }
    add("a", "c"); // b 不来
    const carried = Object.fromEntries(
      [...home(tm).store.tabs.values()].filter((t) => t.group !== null).map((t) => [t.sessionId, t.group]),
    );
    expect(carried).toEqual({ a: "G", c: "H", p: "H" });
    expect(home(tm).store.tabs.has("b")).toBe(false);
    expect(membersOf(tm, "G")).toEqual(["a"]);
    expect(membersOf(tm, "H")).toEqual(["p", "c"]);
  });
});

// ===== 记录文件从头重读 ⇒ 后端行号从 0 重数 ⇒ tab 整份重来 =====
// 要求：「seq ＝ 当前文件里的行号，截断即换代，先发一帧『这份文件重写了』再从 0 重投」。
// 入口只剩按 seq 那一道去重 ⇒ 不重来的话，新的一代的第 0 行会被旧的一代的第 0 行挡掉（渲染内核在本文件里是替身，数它收到了谁）。
describe("〔RENDER2〕从头重读 ⇒ tab 整份重来", () => {
  const mk = (seq: number, uuid: string) =>
    ({ session_id: "rr-sid", cwd: "/p", path: "/p/rr-sid.jsonl", seq, message: { type: "assistant", uuid } }) as never;
  const rendered = async (): Promise<string[]> => {
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    return (renderContentRecord as unknown as ReturnType<typeof vi.fn>).mock.calls.map(
      (c) => (c[0] as { message: { uuid: string } }).message.uuid,
    );
  };

  it("截短 / 改写 ⇒ 新的一代的第 0 行建卡、换了新的 Tab 对象；「不见了」⇒ 不重来", async () => {
    for (const change of ["truncated", "rewritten", "gone"]) {
      vi.clearAllMocks();
      const tm = makeTM();
      for (const [s, u] of [[0, "a"], [1, "b"]] as const) tm.onLine(mk(s, u));
      const old = home(tm).store.tabs.get("rr-sid")!;
      tm.onRecordFileReread("rr-sid", change);
      tm.onLine(mk(0, "z"));
      const again = change !== "gone";
      expect(await rendered(), change).toEqual(again ? ["a", "b", "z"] : ["a", "b"]);
      expect(home(tm).store.tabs.get("rr-sid") !== old, `${change}：在途那几趟认的是「表里还是不是它」`).toBe(again);
    }
  });
});

// ===== 每个 tab 的去重集有上界 =====
// 要求：「`seenSeqs` 换成区间集，上界就变成『段数』…… 不可显示的行照占 seq 却不发 payload，
// 每一处都留一个洞」⇒ monitor 在 payload 上说出前面那一段（`skipped_from`），前端记成区间。
describe("〔RENDER2〕去重集是区间、收全了的会话收成一段", () => {
  it("SeqSet 与朴素 Set 逐号相等（随机加点 / 加段）", async () => {
    const { SeqSet } = await import("../../../src/frontend/ui/live-window");
    let x = 12345;
    const rnd = (n: number): number => ((x = (x * 1103515245 + 12345) % 2147483648) % n);
    for (let round = 0; round < 50; round++) {
      const set = new SeqSet();
      const naive = new Set<number>();
      for (let k = 0; k < 40; k++) {
        const lo = rnd(200);
        const hi = rnd(3) === 0 ? lo + 1 + rnd(20) : lo + 1;
        if (hi === lo + 1) set.add(lo);
        else set.addRange(lo, hi);
        for (let s = lo; s < hi; s++) naive.add(s);
      }
      for (let s = -2; s < 230; s++) expect(set.has(s), `round ${round} seq ${s}`).toBe(naive.has(s));
      expect(set.max).toBe(Math.max(-1, ...naive));
    }
  });

  it("按启动重放的到达序（末块先发、块内升序）喂一份每 7 行一条不可显示的会话 ⇒ 1 段；不带 skipped_from ⇒ 段数 == 洞数 + 1", () => {
    const N = 700;
    const hidden = (s: number): boolean => s % 7 === 3;
    const blocks = [[600, 700], [300, 600], [0, 300]];
    for (const say of [true, false]) {
      const tm = makeTM();
      for (const [lo, hi] of blocks) {
        let run: number | undefined; // 块内连着的不可显示那一段（monitor `SkipRuns` 同形；块首不认）
        for (let s = lo; s < hi; s++) {
          if (hidden(s)) {
            run ??= s;
            continue;
          }
          tm.onLine({
            session_id: "sq",
            cwd: "/p",
            path: "/p/sq.jsonl",
            seq: s,
            message: { type: "assistant", uuid: `sq-${s}` },
            ...(say && run !== undefined ? { skipped_from: run } : {}),
          } as never);
          run = undefined;
        }
      }
      const seen = home(tm).store.tabs.get("sq")!.seenSeqs;
      const holes = [...Array(N).keys()].filter(hidden).length;
      expect(seen.segments, say ? "说了" : "没说").toBe(say ? 1 : holes + 1);
    }
  });
});

// ===== 批的第二道闸：急路要当场物化的正文字符 =====
// 要求：「批的闸门应是字节预算（或『字节 ＋ 条数』双闸）」＋ 同节「要做的话闸的单位该换成『急路要物化的正文字符』」。
describe("〔RENDER2〕物化一批按正文字符截", () => {
  it("两条长正文合起来超预算 ⇒ 第一批只有最高那条、第二批是另一条连同其余短的 ⇒ 建卡顺序 == 手算的 [149, 0‥148]", async () => {
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    const tm = makeTM();
    tm.onLine({ session_id: "bgA", cwd: "/p", path: "/p/bgA.jsonl", seq: 0, message: { type: "assistant", uuid: "a0" } } as never);
    tm.onBatchStart();
    const big = 40 * 1024; // 两条合起来超过 64 Ki，各自一条不超
    for (let s = 0; s < 150; s++) {
      const text = s >= 148 ? "x".repeat(big) : "y";
      tm.onLine({
        session_id: "bgB",
        cwd: "/p",
        path: "/p/bgB.jsonl",
        seq: s,
        message: { type: "assistant", uuid: `b${s}`, message: { role: "assistant", content: [{ type: "text", text }] } },
      } as never);
    }
    tm.onBatchEnd();
    spy.mockClear();
    tm.switchTo("bgB");
    const order = spy.mock.calls.map((c) => (c[0] as { seq: number }).seq);
    expect(order).toEqual([149, ...Array.from({ length: 149 }, (_, i) => i)]);
  });
});

describe("tab 多选与批量菜单", () => {
  let tm: TabManager;
  const btn = (sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root;
  const click = (sid: string, mods: MouseEventInit = {}): void => {
    btn(sid).dispatchEvent(new MouseEvent("click", { bubbles: true, ...mods }));
  };
  /** 画出来带 `.selected` 的那几个（读 DOM，不读选中集合本身）。 */
  const drawnSelected = (): string[] =>
    home(tm).store.orderedIds.filter((sid) => btn(sid).classList.contains("selected")).sort();
  const rightClick = (sid: string): void => {
    btn(sid).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
  };
  const menuLabels = (): string[] =>
    [...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem]") ?? [])].map(
      (b) => b.textContent ?? "",
    );
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    tm = makeTM();
    for (const sid of ["a", "b", "c", "d", "e"]) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL_ORIGIN);
    // c、d 在组里 ⇒ 条上看得到的顺序是 c d（组）· a b e（散），不是到达的顺序。
    setGroups(tm, [{ id: "g", name: "组", tabs: ["c", "d"] }]);
    tm.switchTo("a");
    home(tm).bar.refresh();
  });

  it("S1 · Ctrl 加减（第一次把当前 tab 带上、不切 tab）· Shift 按条上的顺序连选 · 单击 / 空白 / Esc 清掉", () => {
    expect(home(tm).bar.visibleOrder(), "正控：组在前、散 tab 在后").toEqual(["c", "d", "a", "b", "e"]);
    click("b", { ctrlKey: true });
    expect(drawnSelected()).toEqual(["a", "b"]);
    expect(home(tm).store.activeId, "Ctrl 单击不切 tab").toBe("a");
    click("a", { ctrlKey: true });
    expect(drawnSelected()).toEqual(["b"]);
    click("a");
    expect(drawnSelected(), "单击清掉多选").toEqual([]);
    click("d", { shiftKey: true });
    // 锚点 a（上一次单击）到 d：条上是 d · a 相邻 —— 按到达顺序会是 a b c d。
    expect(drawnSelected()).toEqual(["a", "d"]);
    click("e", { shiftKey: true });
    expect(drawnSelected(), "锚点不动，换一头").toEqual(["a", "b", "e"]);
    const bar = document.body.firstElementChild as HTMLElement;
    // 空白 ＝ 列表里没有 tab 的地方（列表撑满栏顶那几条之下的整格）。
    (bar.querySelector(".tab-list") as HTMLElement).dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(drawnSelected(), "点空白清掉").toEqual([]);
    click("b", { ctrlKey: true });
    closeMenu(); // 前面几条用例右键开过的菜单还压在弹层栈上（换了 DOM 不会自己出栈）
    dispatcher.applyOverrides({}); // 主窗口启动时那两下（键位表 ＋ 挂监听；Esc → 栈顶 overlay）
    dispatcher.start();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    expect(drawnSelected(), "Esc 清掉").toEqual([]);
  });

  it("S2 · tab 没了自动掉出多选；右键没选中的 ⇒ 清掉多选、开单个菜单；右键选中的（≥2）⇒ 批量菜单带个数", () => {
    const head = (n: number): string => copyText("tabBatch.menu.head", { n });
    click("e", { ctrlKey: true });
    tm.archiveTab("e");
    tm.closeTab("e");
    expect(drawnSelected()).toEqual(["a"]);
    rightClick("a");
    const batchHead = (): string | null | undefined => document.body.querySelector("[role=menu] [role=presentation]")?.textContent;
    expect(batchHead(), "e 没了 ⇒ 多选只剩 a ⇒ 单个菜单").not.toBe(head(1));
    expect(batchHead()).not.toBe(head(2));
    click("b", { ctrlKey: true });
    rightClick("c");
    expect(drawnSelected(), "右键没选中的 ⇒ 清掉多选").toEqual([]);
    expect(batchHead(), "单个菜单").not.toBe(head(2));
    click("b", { ctrlKey: true });
    tm.archiveTab("b");
    rightClick("b");
    const labels = menuLabels();
    expect(document.body.querySelector("[role=menu] [role=presentation]")?.textContent, "头一行「已选 N 个」").toBe(head(2));
    expect(labels).toContain(copyText("tabBatch.menu.stop", { n: 1 }));
    expect(labels).toContain(copyText("tabBatch.menu.startTmux", { n: 1 }));
    expect(labels).toContain(copyText("tabBatch.menu.close", { n: 1 }));
    expect(drawnSelected(), "开批量菜单不动多选").toEqual(["a", "b"]);
  });
});

describe("起新会话的占位标签页接在标签页栏里", () => {
  it("★ 恒在列表最末（后来的标签页 · 新建的组都排在它前面）；手动切到哪个标签页（连切回原来那个）⇒ 那一页收起；自动跟随不把人拽走", () => {
    const tm = makeTM();
    tm.createSkeletonTab("a", "/p/a", LOCAL_ORIGIN, null, null);
    tm.createSkeletonTab("b", "/p/b", LOCAL_ORIGIN, null, null);
    tm.switchTo("a");
    const shown: boolean[] = [];
    tm.onSlotShown = (on) => shown.push(on);
    tm.addLaunchSlot({ origin: "devbox", cwd: "/home/u/srv/billing", tmuxName: "billing-2", agent: "claude", match: { cwd: "/home/u/srv/billing" } });
    tm.createSkeletonTab("c", "/p/c", LOCAL_ORIGIN, null, null);
    setGroups(tm, [{ id: "g1", name: "组一", tabs: ["b"] }]);
    tm.repaintTabBar();
    const list = home(tm).bar.listEl;
    const last = list.lastElementChild as HTMLElement;
    expect(last.querySelector("[data-slot]")?.textContent).toBe(`devboxbilling${copyText("launch.placeholder.title")}`);
    expect(tm.debugSlots().showing).toBe(1);
    expect(shown).toEqual([true]);

    tm.switchTo("b", "auto");
    expect(tm.debugSlots().showing, "自动跟随不切").toBe(1);
    expect(tm.activeSessionId()).toBe("a");

    tm.switchTo("a");
    expect(tm.debugSlots().showing, "切回原来那个也收起").toBeNull();
    expect(tm.debugSlots().ids, "那一行留着、还在等").toEqual([1]);
    expect(shown).toEqual([true, false]);
  });

  it("★ 恢复一条已有标签页的会话（按 sid 认）⇒ 不长占位页，直接切到那个标签页、回「没接」；按目录认的新起照旧长", () => {
    const tm = makeTM();
    tm.createSkeletonTab("a", "/p/a", LOCAL_ORIGIN, null, null);
    tm.createSkeletonTab("s1", "/p/s1", LOCAL_ORIGIN, null, null);
    tm.switchTo("a");
    const taken = tm.addLaunchSlot({ origin: LOCAL_ORIGIN, cwd: "/p", tmuxName: null, agent: "claude", match: { sid: "s1" } });
    expect(taken, "已有标签页 ⇒ 不接").toBe(false);
    expect(tm.debugSlots().ids, "没长占位页").toEqual([]);
    expect(tm.activeSessionId(), "切到了那个标签页").toBe("s1");
    expect(tm.addLaunchSlot({ origin: LOCAL_ORIGIN, cwd: "/p", tmuxName: null, agent: "claude", match: { sid: "s9" } }), "没有那个标签页 ⇒ 照旧长").toBe(true);
    expect(tm.addLaunchSlot({ origin: LOCAL_ORIGIN, cwd: "/p", tmuxName: null, agent: "claude", match: { cwd: "/p" } })).toBe(true);
    expect(tm.debugSlots().ids).toEqual([1, 2]);
  });

  it("★ 占位那一页显着 ⇒ 作用于当前会话的快捷键（查找 · 折叠 · 翻轮 · 到底 · 关 · 开目录 · 开窗 · 菜单 · 切到终端）不落到底下那个真标签页；全局的照常", () => {
    const tm = makeTM();
    tm.createSkeletonTab("a", "/p/a", LOCAL_ORIGIN, null, null);
    tm.switchTo("a");
    const sessionKeys = ["session.find", "session.toggle-process", "session.prev-turn", "session.next-turn", "session.to-bottom", "tab.close-archived", "tab.open-cwd", "tab.pop-out", "tab.context-menu", "terminal.bring-front"] as const;
    expect(sessionKeys.filter((k) => tm.shadowedBySlot(k)), "没有占位页时都照常").toEqual([]);
    tm.addLaunchSlot({ origin: "devbox", cwd: "/w/x", tmuxName: null, agent: "claude", match: { cwd: "/w/x" } });
    expect(sessionKeys.filter((k) => !tm.shadowedBySlot(k))).toEqual([]);
    expect(tm.shadowedBySlot("app.open-command-bar"), "全局的照常").toBe(false);
    expect(tm.shadowedBySlot("needs.next")).toBe(false);
    tm.switchTo("a");
    expect(tm.shadowedBySlot("session.find"), "切回真标签页 ⇒ 照常").toBe(false);
  });
});
