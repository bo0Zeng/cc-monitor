// Tab 生命周期状态机的 DOM 单元测试（vitest + jsdom）。
//
// 为什么需要它：那次迭代两个真 bug 都在 tabs.ts 的生命周期里——
//   1) resume 后已结束的 Tab 不回到活（今天活不活只由那台的脸说，`onFace`）；
//   2) 关 Tab 只允许已结束的（closeTab 守卫，照脸上的 `can.close`）。
// tabs.ts 重度依赖 DOM + 一堆协作模块（MessageStream / RecordTimeline / BranchFolder /
// 渲染 / Tauri IPC），无法像现有 *.test.ts 那样在裸 node 里测。这里用 jsdom 提供真 DOM、
// 把重协作者 mock 成空壳，于是能在真 TabManager 实例上断言状态翻转。

import { fmtDur } from "../../../src/frontend/ui/duration-format";
import { owned } from "../../test-support/owned-tab-manager.ts";
import { stateLine } from "../../../src/frontend/ui/session-face";
import { type Mock, describe, it, expect, vi, beforeEach, afterEach, onTestFinished } from "vitest";

// ★ audit-0805 F15：**「每行调了几次」要可测** —— 折叠层的 stub 带计数器（`cardsAdded` · `rebuildNow` · 收到的主线外清单），
// `renderContentRecord` 的 mock 真往 timeline 里塞（否则 `inserted` 恒假，计数恒零）。
const f15 = vi.hoisted(() => ({
  cardsAdded: 0,
  rebuildNow: 0,
  setList: [] as string[][],
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
    });
  },
}));

// --- 把重/IPC 协作者 mock 掉，让 TabManager 能在 jsdom 下实例化（避免拉渲染栈/IPC）---
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
    offCount = 0;
    setList(list: { off: readonly string[] }): void {
      f15.setList.push([...list.off]);
      this.offCount = list.off.length;
    }
    // F15：计数，不做事 —— 量的是「被调了几次」，不是它做了什么。
    cardsAdded(): void {
      f15.cardsAdded++;
    }
    unwrapAll(): void {}
    rebuildNow(): void {
      f15.rebuildNow++;
    }
    dispose(): void {}
  },
}));
// F40a:tabs.ts 消费两段式入口(routeMeta 判 meta / renderContentRecord 建卡)。
// mock 按 record.t 粗判 consumed/content,与真实现语义对齐(防未来 meta 用例静默走错路)
vi.mock("../../../src/frontend/ui/render-stream-record", () => ({
  routeMeta: vi.fn((payload: { record?: { t?: string } }) => (payload.record?.t === "title" ? "consumed" : "content")),
  // ★ F15 第 3 步：mock 必须**真往 timeline 里塞**。
  //
  // 上面把 `RecordTimeline.size` 改成真会涨之后还不够 —— 真正调 `timeline.insert` 的是
  // 这个函数（`render-stream-record.ts:173-179` 的 `card` 分支），而它此前是个空 `vi.fn()`。
  // 三处 mock 里少任何一处，`tabs.ts:891` 的 `inserted` 都恒假。
  // ⇒ 「量具把它测没了」这件事**是三处叠出来的**，只修一处会让人以为修完了却仍然零命中。
  // 这里只塞「一条记录 = 一个 seq」（与真实现同：一条记录至多一张卡）。
  renderContentRecord: vi.fn(f15.insertingRender),
}));
// 启动管线读数（`startup-perf.ts`）换成 spy：判的是冷起补读那一趟调到了哪几处（读数本身住 `startup-perf.vitest.ts`）。
vi.mock("../../../src/frontend/ui/startup-perf", () => ({
  noteFirstBatch: vi.fn(),
  noteHistoryArrived: vi.fn(),
  noteHistoryFed: vi.fn(),
  noteColdFilled: vi.fn(),
  emitPerfSummary: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/cards", () => ({
  reconcilePendingToolResults: vi.fn(() => []),
}));
vi.mock("../../../src/frontend/ui/cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), undoToast: vi.fn(), failToast: vi.fn(), silentUndo: vi.fn(), noteMessage: vi.fn(), recentToasts: vi.fn(() => []) }));
// 恢复交那台（`session-open`）：这里换成 spy，判的是交了什么；那台怎么判住后端判据，发 · 说 · 开窗住 `session-open.vitest.ts`。
vi.mock("../../../src/frontend/ui/session-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/session-open")>()),
  openSession: vi.fn().mockResolvedValue(true),
}));
// 单个菜单的「杀」交本机后端一项的那一批（`sessions-batch`，与批量同一条）：这里换成 spy，
//   判的是单个那一侧交了什么、拿到回包之后做什么；合那一句住后端判据（`sessions_batch_face_tests.rs`），交法住 `tab-batch-run.vitest.ts`。
const ONE_DONE = vi.hoisted(() => ({ results: [], said: "S-已结束", failed: false, lines: [] as string[], copyDetail: "", windows: [] }));
vi.mock("../../../src/frontend/ui/tab-batch-run", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/tab-batch-run")>();
  return { ...real, askSessions: vi.fn(async () => ONE_DONE) };
});
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
  sessionReadCalls,
  UNSUPPORTED,
  withAccountReads,
  withHistoryReads,
  withSessionReads,
  recordReadCalls,
  recordReadOf,
  type ChanCallArgs,
} from "../../test-support/chan-fake";
import type { SessionFacts, UsageFact } from "../../../src/frontend/ui/session-reads";
import { failToast, noteMessage, silentUndo, toast as showActionFailureToast, undoToast, type ToastOptions } from "../../../src/frontend/ui/kit/toast";
import { __setHostFactsForTests } from "../../../src/frontend/ui/settings/host-os";
import { openSession } from "../../../src/frontend/ui/session-open";
import { pickAsk } from "../../../src/frontend/ui/resume-menu";
import { askSessions } from "../../../src/frontend/ui/tab-batch-run";
import { TabManager, type Tab } from "../../../src/frontend/ui/tabs";
import * as startupPerf from "../../../src/frontend/ui/startup-perf";
import { DWELL_MS, DWELL_MOVE_PX, type DropTarget } from "../../../src/frontend/ui/tab-drop";
import tg from "../../../src/frontend/ui/tab-group.module.css";

/** 落点：插到 `sid` 之前（落下后属于 `gid`）· 末尾（组外）。 */
const before = (sid: string, gid: string | null = null): DropTarget => ({ kind: "insert", at: { sid, side: "before" }, gid });
const END: DropTarget = { kind: "insert", at: null, gid: null };
import type { TabGroup } from "../../../src/frontend/ui/tab-bar-prefs";
import { GroupBook, type GroupCall } from "../../../src/frontend/ui/group-book";
import type { GroupsRead } from "../../../src/frontend/ui/generated/GroupsRead";
import type { GroupsWriteReq } from "../../../src/frontend/ui/generated/GroupsWriteReq";
import type { MemberFacts } from "../../../src/frontend/ui/generated/MemberFacts";
import type { Patch } from "../../../src/frontend/ui/generated/Patch";

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
import { announced, goldenFace, liveFace } from "../../test-support/session-faces";
import { closesFromMenu } from "../../../src/frontend/ui/face-can";
import type { SessionFace } from "../../../src/frontend/ui/generated/SessionFace";
import { readFileSync } from "node:fs";
import { SkeletonLedger } from "../../../src/frontend/ui/live-window";
import { SkeletonView } from "../../../src/frontend/ui/skeleton-view";
import { buildStepLine, settleStepLine } from "../../../src/frontend/ui/cards/step-line";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { closeMenu } from "../../../src/frontend/ui/kit/menu";
import { answerAskDialog, askDialogText, noAskDialog } from "../../test-support/ask-dialog-driver.ts";
import type { TabStore } from "../../../src/frontend/ui/tab-store";
import type { TabBarView } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarDrag } from "../../../src/frontend/ui/tab-bar-drag";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { TabStreamView } from "../../../src/frontend/ui/tab-stream-view";
import type { TabSessionActions } from "../../../src/frontend/ui/tab-session-actions";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { appStore, putIn } from "../../../src/frontend/ui/app-store";
import type { SessionAccountReply } from "../../../src/frontend/ui/generated/SessionAccountReply";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { endConfirm } from "../../../src/frontend/ui/end-confirm";
import { recordFileWiring } from "../../../src/frontend/ui/record-file-notice";

/** 会话事实里的一格 usage（上限由后端定：这里按「判不出 ⇒ 1M」那一形造）。 */
const usageOf = (promptTokens: number, model: string | null): UsageFact => ({
  model,
  limit: 1_000_000,
  percent: null,
  contextText: `核心·${promptTokens}`,
  contextTone: "plain",
  promptTokensText: `核心·${promptTokens}`,
  limitText: "核心·1M",
  limitFromText: null,
});
import type { Edit } from "./config-patch-fake";
import { dispatcher } from "../../../src/frontend/ui/keybindings/registry";
import { copyPattern } from "../../test-support/copy-pattern";
import { factsOn } from "../../test-support/host-facts";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

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
/** 交给那台的每一问 `session-open`：`[哪台, 请求]`。 */
const opens = (): unknown[][] => vi.mocked(openSession).mock.calls.map((c) => [c[0], c[1]]);

/**
 * 摆一份分组：组表只有 `{id, name}`，组员 = `Tab.group`（tab 自己的属性）。
 * `tabs` 里的 sid 必须已经在栏里；没列到的 tab 一律散着。
 */
function setGroups(tm: TabManager, groups: { id: string; name: string; tabs?: string[] }[]): void {
  home(tm).prefs.collections = groups.map(({ id, name }) => ({ id, name, counts: [] }));
  for (const t of home(tm).store.tabs.values()) t.group = null;
  for (const g of groups) for (const sid of g.tabs ?? []) home(tm).store.tabs.get(sid)!.group = g.id;
}
/** 开着的菜单里写着 `label` 的那一项。 */
function groupMenuItem(label: string): HTMLElement {
  const it = [...document.body.querySelectorAll<HTMLElement>("[role=menu] [role^=menuitem]")].find((b) => b.textContent?.startsWith(label));
  if (!it) throw new Error(`菜单里没有「${label}」`);
  return it;
}
/** 组 `gid` 里此刻有谁（按栏里的顺序）。 */
function membersOf(tm: TabManager, gid: string): string[] {
  return home(tm).store.orderedIds.filter((sid) => home(tm).store.tabs.get(sid)?.group === gid);
}

/**
 * 组的后端替身（照 `history/groups.rs` 那几条的形状、只做判据要的）：组跟着组员走——有组员才读得出那一组（名字取最新那份），
 * 组员与固定是会话注解（住会话那台）；建组只起 id，进组带记录，改名 · 解散发给 `machines` 那几台。
 * 读回来的各组照建的先后、组员带那台给的复活那几格。`calls` 记下每一问 `[哪台, 命令, 入参]`。
 */
function groupBackend(): {
  book: GroupBook;
  calls: Array<[string, string, unknown]>;
  note: (origin: string, sid: string) => { groups: string[]; pinned: boolean; facts: MemberFacts };
  group: (id: string, name: string) => void;
  connect: (tm: TabManager) => Promise<void>;
  /** 那台拒固定 / 取消固定那一问（`history-annotate {pinned}`）· 拒写组那一问（`groups-write`）。 */
  refuse: { pin: boolean; group: boolean };
} {
  const refuse = { pin: false, group: false };
  const groups: Array<{ id: string; name: string; named: number }> = [];
  const notes = new Map<string, { origin: string; sid: string; groups: string[]; pinned: boolean; facts: MemberFacts }>();
  const calls: Array<[string, string, unknown]> = [];
  let made = 0;
  const note = (origin: string, sid: string): { origin: string; sid: string; groups: string[]; pinned: boolean; facts: MemberFacts } => {
    const k = `${origin}\n${sid}`;
    let m = notes.get(k);
    if (!m) {
      m = { origin, sid, groups: [], pinned: false, facts: { title: `标题-${sid}`, cwd: `/w/${sid}`, record: `/p/${sid}.jsonl`, account: null, bg: false, at: null } };
      notes.set(k, m);
    }
    return m;
  };
  const row = (m: { origin: string; sid: string; pinned: boolean; facts: MemberFacts }): GroupsRead["pinned"][number] => ({
    origin: m.origin,
    sid: m.sid,
    pinned: m.pinned,
    face: goldenFace("ended"),
    facts: m.facts,
  });
  const membersOfGroup = (id: string): ReturnType<typeof note>[] => [...notes.values()].filter((m) => m.groups.includes(id));
  const read = (): GroupsRead => ({
    groups: groups
      .filter((g) => membersOfGroup(g.id).length > 0)
      .map((g) => ({
        id: g.id,
        name: g.name,
        title: g.name,
        made: 0,
        named: g.named,
        known: { is: "now" },
        members: membersOfGroup(g.id).map(row),
        counts: [],
      })),
    pinned: [...notes.values()].filter((m) => m.pinned).map(row),
    machines: [],
  });
  const call: GroupCall = async (origin, cmd, req) => {
    calls.push([origin, cmd, req]);
    if (cmd === "groups-read") return read() as never;
    if (cmd === "groups-write") {
      const r = req as GroupsWriteReq;
      if (refuse.group) throw new Error("那台拒了写组-乙");
      if (r.op === "make") {
        made += 1;
        return { is: "made", id: `g${made}`, name: r.name, made, named: made } as never;
      }
      if (r.op === "move") {
        const m = note(origin, r.sid);
        if (r.leave != null) m.groups = m.groups.filter((g) => g !== r.leave);
        if (r.join != null) {
          const j = r.join;
          if (!m.groups.includes(j.id)) m.groups.push(j.id);
          const had = groups.find((g) => g.id === j.id);
          if (!had) groups.push({ id: j.id, name: j.name, named: j.named });
          else if (j.named > had.named) Object.assign(had, { name: j.name, named: j.named });
        }
        return { is: "moved", sid: r.sid, groups: m.groups } as never;
      }
      const g = groups.find((x) => x.id === r.id);
      if (r.op === "rename" && g) Object.assign(g, { name: r.name, named: g.named + 1 });
      if (r.op === "dissolve") for (const m of notes.values()) if (r.machines.includes(m.origin)) m.groups = m.groups.filter((x) => x !== r.id);
      return { is: "sent", id: r.id, missed: [] } as never;
    }
    const { sid, patch } = req as { sid: string; patch: Patch };
    const m = note(origin, sid);
    if (patch.pinned != null && refuse.pin) throw new Error("那台拒了固定-甲");
    if (patch.pinned != null) m.pinned = patch.pinned;
    return { entry: {}, prev: null } as never;
  };
  const book = new GroupBook(call, 0);
  return {
    book,
    calls,
    note,
    group: (id, name) => void groups.push({ id, name, named: 0 }),
    refuse,
    connect: async (tm) => {
      tm.attachGroups(book);
      await book.again();
    },
  };
}
/** 写都是交出去不等的（先改内存再交那台）⇒ 看替身之前把那几跳跑完（推送重问也在里面）。 */
const settleGroups = async (): Promise<void> => {
  for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
};

function makeTM(): TabManager {
  document.body.innerHTML = "";
  const barEl = document.createElement("div");
  const streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  return owned(announced(new TabManager(barEl, streamRootEl)));
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

  it("ensureTab 默认建 live 本地 Tab（origin = `LOCAL`）", () => {
    const tab = tm.ensureTab("s1", "/home/u", "p", LOCAL);
    expect(tab.face?.dot).toBe("running");
    expect(tab.origin).toBe(LOCAL);
    expect(home(tm).store.tabs.has("s1")).toBe(true);
  });

  it("ensureTab 远端 Tab（origin!==null）也建成 live", () => {
    const tab = tm.ensureTab("s2", "/home", "p", "pi");
    expect(tab.face?.dot).toBe("running");
    expect(tab.origin).toBe("pi");
  });

  // === Batch5-F18：骨架 Tab ===

  it("createSkeletonTab 建骨架；首行到达不重建、parentPath 回填", () => {
    tm.createSkeletonTab("sk1", "/root/proj", LOCAL);
    const skeleton = home(tm).store.tabs.get("sk1")!;
    expect(skeleton.face?.dot).toBe("running");
    expect(skeleton.parentPath).toBe("");
    expect(skeleton.projectDir).toBe("/root/proj");

    // 首条真实行：同一 Tab 实例（不重建），parentPath 回填；行不带项目目录 ⇒ 不动它
    const after = tm.ensureTab("sk1", null, "/fake/sk1.jsonl", LOCAL);
    expect(after).toBe(skeleton);
    expect(after.parentPath).toBe("/fake/sk1.jsonl");
    expect(after.projectDir).toBe("/root/proj");
  });

  it("骨架 tab 先建、行后到：展开取回全文按回填的那份记录路径问（不是建骨架时那个空串）", async () => {
    const asked: string[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { op?: string; payload?: number[] } | undefined;
      if (cmd === "chan_call" && a?.op === "history-page") asked.push((JSON.parse(new TextDecoder().decode(Uint8Array.from(a.payload ?? []))) as { path: string }).path);
      return undefined;
    });
    tm.createSkeletonTab("sk2", "/root/proj", LOCAL);
    const tab = tm.ensureTab("sk2", null, "/fake/sk2.jsonl", LOCAL);
    tab.skeleton = { ledger: { factsOf: () => ({ o: 0, n: 10 }) }, dispose: () => {} } as unknown as typeof tab.skeleton;
    await tab.fullRecord({ id: "r1", seq: 0 }).catch(() => null);
    expect(asked).toEqual(["/fake/sk2.jsonl"]);
  });

  // 会话里 shell 进了子目录：之后的记录 cwd 都是子目录，而界面只读尾巴 ⇒ 流里「见过的最早那条」是近期的一条。
  // 标题只认后端宣告的项目目录；后端不给（老后端）⇒ 退到 sid，不从行里猜。
  it("★ 项目目录只认宣告那一格：只读尾巴的流接上之后仍是宣告的那一个，不漂成子目录（标题不读它：名字是核心给的）", () => {
    const tail = (sid: string, seq: number) =>
      tm.onLine({
        session_id: sid,
        cwd: "/a/proj/sub",
        path: `/p/${sid}.jsonl`,
        seq,
        record: { t: "reply", id: `${sid}-${seq}` } as never,
      } as never);
    tm.createSkeletonTab("proj-sid", "/a/proj", LOCAL);
    for (let seq = 40; seq < 45; seq++) tail("proj-sid", seq);
    const t = home(tm).store.tabs.get("proj-sid")!;
    expect(t.projectDir).toBe("/a/proj");
    expect(t.title, "核心还没报名字 ⇒ 空着等它（界面不拿会话号顶名字）").toBe("");

    // 后端重宣告给了不同的项目目录（固定 tab 存的那份旧了）⇒ 对齐它。
    tm.createSkeletonTab("proj-sid", "/b/other", LOCAL);
    expect(t.projectDir).toBe("/b/other");
  });

  it("远端骨架（无项目目录）标题照抄核心那一行，重复宣告幂等", () => {
    tm.createSkeletonTab("deadbeef-1234", null, "pi");
    const t = home(tm).store.tabs.get("deadbeef-1234")!;
    expect(t.origin).toBe("pi");
    expect(t.projectDir).toBeNull();
    tm.updateName("deadbeef-1234", "deadbeef", "deadbeef"); // 记录没读到 ⇒ 核心给会话号头 8 位
    expect(t.title).toBe("deadbeef");
    const before = home(tm).store.tabs.size;
    tm.createSkeletonTab("deadbeef-1234", null, "pi"); // 重连重发 session_added
    expect(home(tm).store.tabs.size).toBe(before);
    expect(home(tm).store.tabs.get("deadbeef-1234")).toBe(t);
  });

  it("已结束的脸早于骨架到：骨架建出来就是已结束", () => {
    tm.onFace("sk-late", goldenFace("ended"));
    tm.createSkeletonTab("sk-late", null, "pi");
    expect(home(tm).store.tabs.get("sk-late")!.face?.dot).toBe("ended");
  });

  // === `INVARIANTS §26`：bg 会话 ⚙ 标题（〔「删掉树」〕不再挂宿主排成树：落位 / 拖拽 / 集合
  //     与普通 tab 相同那一组在本文件末尾「〔BG1〕」describe.each 里，两种 tab 各跑一遍） ===

  it("bg tab 平铺：顺序 == 到达序（不挂到同 cwd 交互 tab 之后），标题照抄核心写好的整行（⚙ 也是核心写的）", () => {
    tm.createSkeletonTab("host-a", "/proj/a", LOCAL, false);
    tm.createSkeletonTab("other", "/proj/b", LOCAL, false);
    tm.createSkeletonTab("bg-a1", "/proj/a", LOCAL, true);
    const order = home(tm).store.orderedIds;
    expect(order).toEqual(["host-a", "other", "bg-a1"]);
    tm.updateName("bg-a1", "评估任务", "a ⚙ 评估任务");
    const bg = home(tm).store.tabs.get("bg-a1")!;
    expect(bg.title).toBe("a ⚙ 评估任务");
  });

  it("远端 bg 标题也照抄：那台机器不进标题（栏上另画那台的标签）", () => {
    tm.createSkeletonTab("h-local", "/p", LOCAL, false);
    tm.createSkeletonTab("bg-remote", "/p", "pi", true);
    expect(home(tm).store.orderedIds).toEqual(["h-local", "bg-remote"]);
    tm.updateName("bg-remote", "远端任务", "p ⚙ 远端任务");
    expect(home(tm).store.tabs.get("bg-remote")!.title).toBe("p ⚙ 远端任务");
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
      record: { t: "reply", id: uuid } as never,
    });
    tm.onLine(mkPayload(7, "u-1") as never);
    const after1 = spy.mock.calls.length;
    tm.onLine(mkPayload(7, "u-1") as never); // 同 (sid,seq) 重复 → 去重
    expect(spy.mock.calls.length).toBe(after1);
    tm.onLine(mkPayload(8, "u-2") as never); // 新 seq → 放行
    expect(spy.mock.calls.length).toBe(after1 + 1);
  });

  // === issue #63①：fork 血缘徽标 ===
  // 分叉血缘改由后端出成品（`history-facts`），这一组搬进文件末尾「会话事实」那组（后端给了什么 ⇒ tab 上是什么）；
  //   「首条锁定 · 只认 user/assistant」那两条口径住后端 `tests/backend/observe/recognize/facts_tests.rs`。

  // === Batch5-F19：last-active 写回 ===

  it("switchTo 写回 last-active；persistLastActive=false（viewer）不写", () => {
    localStorage.removeItem("cc-monitor.last-active-sid");
    tm.ensureTab("s-a", "/a", "p", LOCAL);
    tm.ensureTab("s-b", "/b", "p", LOCAL);
    tm.switchTo("s-b");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");

    // viewer 模式：禁写（防独立窗口污染主窗口记忆，审计 R1）
    tm.persistLastActive = false;
    tm.switchTo("s-a");
    expect(localStorage.getItem("cc-monitor.last-active-sid")).toBe("s-b");
    localStorage.removeItem("cc-monitor.last-active-sid"); // 防同文件后续测试顺序耦合
  });

  it("手动 switchTo 触发 onManualSwitch（迟到宣告不抢焦点的清 pending 钩子）", () => {
    tm.ensureTab("m-a", "/a", "p", LOCAL);
    tm.ensureTab("m-b", "/b", "p", LOCAL);
    let fired = 0;
    tm.onManualSwitch = () => fired++;
    tm.switchTo("m-b"); // 默认 manual
    expect(fired).toBe(1);
    tm.ensureTab("m-c", "/c", "p", LOCAL); // 非首个 tab，不切换
    tm.switchTo("m-a", "auto"); // auto 不触发
    expect(fired).toBe(1);
    localStorage.removeItem("cc-monitor.last-active-sid");
  });

  it("★ 那台的脸早于 Tab 到 ⇒ 建出来就是那张脸；同一 sid 后到的覆盖先到的（建之前 · 建之后都是）", () => {
    tm.onFace("early", liveFace("idle"));
    tm.onFace("early", goldenFace("ended"));
    const tab = tm.ensureTab("early", null, "p", LOCAL);
    expect(tab.face).toEqual(goldenFace("ended"));
    expect(home(tm).store.pendingFace.has("early"), "落实了就不留").toBe(false);
    tm.onFace("early", goldenFace("reconnectable"));
    expect(tab.face).toEqual(goldenFace("reconnectable"));
    tm.onFace("early", liveFace("needs_you"));
    expect(tab.face?.dot).toBe("needs-you");
  });

  it("★ 没收到过脸（固定条复活 · 行先到）⇒ 不画点（界面不替那台拼一张）、只在菜单里关得掉；又见行 · 重宣告不替那台改脸", () => {
    document.body.innerHTML = "";
    const bar = document.createElement("div");
    const root = document.createElement("div");
    document.body.append(bar, root);
    const t = owned(new TabManager(bar, root));
    const tab = t.ensureTab("nf", "/x", "p", "pi");
    expect(tab.face, "一张脸都还没收到").toBeNull();
    expect(bar.querySelector<HTMLElement>(".tab [data-state]")?.dataset.state, "不画点").toBe("none");
    expect(closesFromMenu(tab.face), "一个标签页总能关").toBe(true);
    t.onFace("nf", goldenFace("ended"));
    t.ensureTab("nf", "/x", "p", "pi");
    expect(tab.face?.dot, "见行不翻活：活不活由那台的帧说").toBe("ended");
  });

  it("已结束的脸：点 · 类照它，监控板的序照脸上那一格", () => {
    const tab = tm.ensureTab("s3", "/home", "p", LOCAL);
    const btn = () => document.querySelector<HTMLElement>(".tab")!;
    const order = (sid: string) => tm.snapshotSessions().find((x) => x.sessionId === sid)?.face.order;
    tm.onFace("s3", liveFace("needs_you"));
    expect(order("s3")).toBe(goldenFace("live-needs-you-tmux").order);
    tm.onFace("s3", goldenFace("reconnectable"));
    expect([tab.face?.dot, btn().hasAttribute("data-reconnectable"), btn().classList.contains("ended"), order("s3")]).toEqual(["exited", true, false, goldenFace("reconnectable").order]);
    tm.onFace("s3", goldenFace("ended"));
    expect([tab.face?.dot, btn().hasAttribute("data-reconnectable"), btn().classList.contains("ended"), order("s3")]).toEqual(["ended", false, true, goldenFace("ended").order]);
  });

  it("灯那一档按点：在等 ⇒ waiting · 空闲 ⇒ idle · 在跑 ⇒ 不挂（换了就清旧的）", () => {
    tm.ensureTab("lt", "/x", "p", LOCAL);
    const light = () => document.querySelector<HTMLElement>(".tab")!.dataset.light;
    tm.onFace("lt", liveFace("needs_you"));
    expect(light()).toBe("waiting");
    tm.onFace("lt", liveFace("idle"));
    expect(light()).toBe("idle");
    tm.onFace("lt", liveFace("working"));
    expect(light()).toBeUndefined();
  });

  it("★ 离开在等 ⇒ 手上那份需手动当场清掉（流里在等的那一步直接读它）；进程退了也清", () => {
    const tab = tm.ensureTab("nw", "/x", "p", LOCAL);
    const approve = { kind: "approve" } as unknown as NonNullable<typeof tab.needs>;
    tm.onFace("nw", liveFace("needs_you"));
    tab.needs = approve;
    tm.onFace("nw", liveFace("working"));
    expect(tab.needs, "答完了").toBeNull();
    tm.onFace("nw", liveFace("needs_you"));
    tab.needs = approve;
    tm.onFace("nw", goldenFace("ended"));
    expect(tab.needs, "在等时进程退了").toBeNull();
  });

  it("★ 能不能换模型 · 权限档照脸上那两格", () => {
    const tab = tm.ensureTab("sw", "/x", "p", LOCAL);
    tm.onFace("sw", liveFace("working"));
    expect(tab.face?.can.model).toEqual(goldenFace("live-working-plain").can.model);
    tm.onFace("sw", goldenFace("ended"));
    expect(tab.face?.can.model).toEqual(goldenFace("ended").can.model);
  });

  it("〔U4〕悬停卡标题之后那一行照脸上那一句（会话事实没到 ⇒ 不猜种类）", () => {
    const t = tm.ensureTab("tt1", "/x", "p", "pi");
    const said = () => {
      const c = cardOf(tm, "tt1")!;
      expect(c.firstElementChild?.textContent).toBe(t.title);
      return c.querySelector(".tab-hover-state")?.textContent;
    };
    tm.onFace("tt1", liveFace("needs_you"));
    expect(said()).toBe(goldenFace("live-needs-you-tmux").line);
    tm.onFace("tt1", goldenFace("reconnectable"));
    expect(said()).toBe(goldenFace("reconnectable").line);
    tm.onFace("tt1", goldenFace("ended"));
    expect(said()).toBe(goldenFace("ended").line);
  });

  it("★ 那台看不见 ⇒ 本机后端逐条说的说不清照画（它只说活的 · 可重连的）、已结束 · 记录已不在照旧；那台的帧回来 ⇒ 各回各的脸", () => {
    for (const sid of ["u1", "u2", "u3", "u4"]) tm.ensureTab(sid, "/x", "p", "gpu-01");
    tm.ensureTab("o1", "/x", "p", "other");
    tm.onFace("u2", goldenFace("reconnectable"));
    tm.onFace("u3", goldenFace("ended"));
    tm.onFace("u4", goldenFace("gone"));
    const tab = (sid: string) => home(tm).store.tabs.get(sid)!;
    for (const sid of ["u1", "u2"]) tm.onFace(sid, goldenFace("unseen")); // 本机后端断线那一刻逐条发的 `left`
    expect(["u1", "u2", "u3", "u4", "o1"].map((sid) => tab(sid).face?.dot)).toEqual(["unknown", "unknown", "ended", "gone", "running"]);
    expect(stateLine(tab("u1"), Date.now()).text, "那一句照后端交来的那张脸").toBe(goldenFace("unseen").line);
    tm.onFace("u1", liveFace("idle"));
    tm.onFace("u2", goldenFace("reconnectable"));
    expect(["u1", "u2"].map((sid) => tab(sid).face?.dot)).toEqual(["idle", "exited"]);
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
      tm.onFace(sid, goldenFace("reconnectable")); // → 可重连
      tm.onFace(sid, goldenFace("ended")); // → 已结束
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

  it("closeTab 拒关 live Tab（守卫：仅 archived 可关）", () => {
    tm.ensureTab("s7", "/x", "p", LOCAL);
    tm.closeTab("s7");
    expect(home(tm).store.tabs.has("s7")).toBe(true);
  });

  it("closeTab 关 archived Tab：移出 map + 摘 DOM", () => {
    const tab = tm.ensureTab("s8", "/x", "p", LOCAL);
    const streamEl = tab.streamEl;
    expect(streamEl.parentElement).not.toBeNull();
    tm.onFace("s8", goldenFace("ended"));
    tm.closeTab("s8");
    expect(home(tm).store.tabs.has("s8")).toBe(false);
    expect(streamEl.parentElement).toBeNull();
  });

  it("closeTab 通知后端 forget_session（archived 才关）", () => {
    tm.ensureTab("s9", "/x", "p", LOCAL);
    tm.onFace("s9", goldenFace("ended"));
    tm.closeTab("s9");
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("forget_session", {
      sessionId: "s9",
    });
  });

  it("switchTo：切 active + 清 unread + 加 .active 类", () => {
    tm.ensureTab("s10", "/x", "p", LOCAL); // 首个 → 自动 active
    const tab2 = tm.ensureTab("s11", "/y", "p", LOCAL);
    tab2.unread = 5;
    expect(home(tm).store.activeId).toBe("s10");
    tm.switchTo("s11");
    expect(home(tm).store.activeId).toBe("s11");
    expect(tab2.unread).toBe(0);
    expect(tab2.streamEl.classList.contains("active")).toBe(true);
  });

  // === Batch13-F40a：尾部优先门控 / 物化（D 审计 C-3 补测） ===
  //
  // 🔴 **它服务哪条业务要求：`INVARIANTS.md` §21 第 3 项**〔`P21` 2026-09-22 现打补写〕。
  //    §21（启动重放滚动稳定性·贴底不抖）第 3 项逐字：「重放期「视口上方」旧内容不建 DOM
  //    —— `TabManager.onLine` 按 seq 门控，`seq < tab.window.floorSeq` 的旧记录只进
  //    `TailWindow` 账本，**根本不建卡不挂 DOM**；后台 virgin tab 只进账本，`switchTo` 时同步物化视口那一截」
  //    〔A17-45-3-4 删了后台空闲物化〕——**下面这一组（F40a 门控矩阵 → F40b fill）就是它。**
  // 把 §21 记成「全树零命中」并推出「所以它没人守」：读数对
  //    （没人在散文里点出条号），推论错（第 3 项这一组等值断言一直在守）。
  //    ⚠ §21 的第 1/2 项（守卫式 `snap()` · 不手动补偿 scrollTop）**不在这里** ——
  //    那是 `MessageStream` 的性质，本文件把它整个 mock 掉了（见文件抬头）。
  //    住址：`tests/frontend/ui/invariants-frontend-guard.vitest.ts` 的 ④⑤ 两格。

  const mkContent = (sid: string, seq: number, uuid: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      record: { t: "reply", id: uuid } as never,
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
    expect(actCalls).toBe(2);
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
    expect(vg.window.pendingCount).toBe(5); // 后台 tab 只进账本
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

    // 选区进行中切入:virgin 物化照走(jsdom 恒不满一屏 ⇒ 一次同步调用建满 8 截 × 20 = 160,物化不看选区);
    // R-2 踢链(不可滚+账余 540)被选区守卫挡 → 账保 540
    const selSpy = vi
      .spyOn(document, "getSelection")
      .mockReturnValue({ isCollapsed: false } as unknown as Selection);
    tm.switchTo("fills");
    expect(t.window.pendingCount).toBe(540);
    // 选区仍在:scroll 也被挡
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(t.window.pendingCount).toBe(540);
    // 选区收起:scroll → 补一批 200,之后 rAF 自链接着补到弹尽
    selSpy.mockReturnValue({ isCollapsed: true } as unknown as Selection);
    t.streamEl.dispatchEvent(new Event("scroll"));
    expect(t.window.pendingCount).toBe(340);
    await vi.waitFor(() => expect(t.window.pendingCount).toBe(0), UNTIL_CEILING);
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
    tm.switchTo("reB"); // virgin:同步建一截截(jsdom 恒不满一屏),余下的 rAF 自链补完,floor=1000
    await vi.waitFor(() => expect(t.window.pendingCount).toBe(0), UNTIL_CEILING);

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
    tm.switchTo("geoB"); // virgin：同步建一截截、余下的 rAF 自链补完，floor 钉住
    await vi.waitFor(() => expect(t.window.pendingCount).toBe(0), UNTIL_CEILING);
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

  /**
   * 贴着底看的长会话：最后一张卡的下沿离容器下沿还差底部内边距那一截（48px ＋ 卡的外边距，真浏览器量到 76px）——
   * 那不是「没满一屏」。以前照「最后一张卡够没够到容器下沿」判 ⇒ 贴底的 tab 每切进来一次都判成没满、补一批 200 条
   * （按住「下一个 tab」时每一下都在建卡，台架轨迹里补批占了一大半）。「够到」要把卡的下外边距与容器的下内边距算进去。
   */
  it("贴底看的 tab：最后一张卡下面只剩底部留白 ⇒ 算满一屏，切进来不补批", async () => {
    await spyRender();
    tm.onLine(mkContent("padA", 1, "pad-1")); // active
    tm.onBatchStart();
    for (let s = 1000; s < 1300; s++) tm.onLine(mkContent("padB", s, `pad-${s}`));
    tm.onBatchEnd();
    const t = home(tm).store.tabs.get("padB")!;
    tm.switchTo("padB");
    await vi.waitFor(() => expect(t.window.pendingCount).toBe(0), UNTIL_CEILING); // 冷开那一截之后 rAF 自链补完
    tm.switchTo("padA");
    tm.onBatchStart();
    for (let s = 100; s < 400; s++) tm.onLine(mkContent("padB", s, `pad-old-${s}`)); // 账本有余 300
    tm.onBatchEnd();
    // 真浏览器里量到的那一形：贴底（scrollTop 4284 = scrollHeight 5018 − clientHeight 734），最后一张卡下沿 698、容器下沿 774
    Object.defineProperty(t.streamEl, "scrollHeight", { value: 5018, configurable: true });
    Object.defineProperty(t.streamEl, "clientHeight", { value: 734, configurable: true });
    Object.defineProperty(t.streamEl, "scrollTop", { value: 4284, configurable: true, writable: true });
    t.streamEl.getBoundingClientRect = () => ({ top: 40, bottom: 774, height: 734, left: 0, right: 900, width: 900 }) as DOMRect;
    t.streamEl.style.paddingBottom = "48px"; // `.stream` 的下内边距
    const last = document.createElement("div");
    last.style.marginBottom = "28px"; // 卡的下外边距
    last.getBoundingClientRect = () => ({ top: 245, bottom: 698, height: 453, left: 0, right: 780, width: 780 }) as DOMRect;
    t.stream.contentElement.appendChild(last);
    tm.switchTo("padB");
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    expect(t.window.pendingCount, "贴底的 tab 被判成没满一屏 ⇒ 每切进来一次补一批").toBe(300);
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
    tm.switchTo("sentB"); // 冷开：同步建 8 截 × 20 = 160（jsdom 恒不满一屏），余 40 下一帧接着补
    const t = home(tm).store.tabs.get("sentB")!;
    expect(t.window.pendingCount).toBe(40);
    await new Promise((r) => requestAnimationFrame(() => r(null))); // 下一帧补尽
    expect(t.window.pendingCount).toBe(0);
    await new Promise((r) => requestAnimationFrame(() => r(null))); // 再下一帧：账尽 ⇒ 按行号往下问
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
   * 成因：没建卡的行是骨架占位，占位高是估值（`height-estimate.ts`），按**估值**计入 `scrollHeight`。
   * 旧判据 `scrollHeight - clientHeight > 1` 在第 1 截就成立 ⇒ 补批当场停手（只补 20 条）；
   * 新判据问的是「最后一张卡的底边到没到容器下沿」⇒ 一次同步调用的 8 截补满（160 条）。
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
      "读 scrollHeight 的话第 1 截就停手（余 680）；读真实布局才会补满一次同步调用的 8 截 × 20（余 540）",
    ).toBe(540);
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

  it("kind 升格:bg 骨架先到,interactive 宣告后到 → 升格,位置不动(标题里的 ⚙ 由那台随名字一起改)", () => {
    // 场景还原(用户截图):bg-spare 的宣告先到,父会话被建成后台;interactive 宣告后到必须升格纠正。
    // 原先升格还会把它摘下来按宿主重新挂树;树删了 ⇒ 位置与 kind 无关、不动。
    tm.createSkeletonTab("excel", "/proj/research", LOCAL, false);
    tm.createSkeletonTab("parent", "/proj/research", LOCAL, true); // 谎报形态先到
    tm.createSkeletonTab("fork-empty", "/proj/research", LOCAL, true); // 空克隆
    expect(home(tm).store.orderedIds).toEqual(["excel", "parent", "fork-empty"]);
    expect(home(tm).store.tabs.get("parent")!.background).toBe(true);

    tm.createSkeletonTab("parent", "/proj/research", LOCAL, false); // 真身宣告后到
    const p = home(tm).store.tabs.get("parent")!;
    expect(p.background).toBe(false);
    expect(home(tm).store.orderedIds).toEqual(["excel", "parent", "fork-empty"]);
  });

  it("kind 不降格:interactive tab 后到 bg 宣告(spare 谎报)保持交互形态", () => {
    tm.createSkeletonTab("host2", "/proj/x", LOCAL, false);
    tm.createSkeletonTab("host2", "/proj/x", LOCAL, true);
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
      record: { t: "title", text: "标题" } as never,
    } as never);
    const t = home(tm).store.tabs.get("meta-bg")!;
    expect(t.window.pendingCount).toBe(0); // consumed,不收纳
    expect(t.window.floorSeq).toBeNull();
  });
});

describe("「tab 集合变了」那一格也跟着状态变（会话头 · 终端页 · 需手动订它）", () => {
  it("★ 当前会话 运行中 → 空闲（数量没变、用量没变）⇒ 订阅者照样收到一次；同一状态再来不收", () => {
    document.body.innerHTML = "";
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const seen: number[] = [];
    const tm = owned(announced(new TabManager(barEl, streamRootEl, (s) => seen.push(s.total))));
    tm.ensureTab("st1", "/home/u", "p", LOCAL);
    tm.ensureTab("st2", "/home/u", "p", LOCAL);
    tm.onFace("st1", liveFace("working"));
    const before = seen.length;
    tm.onFace("st1", liveFace("idle"));
    expect(seen.length, "运行中 → 空闲，订阅者没收到（会话头停在「运行中」）").toBe(before + 1);
    tm.onFace("st1", liveFace("idle"));
    expect(seen.length, "同一状态再来不该通知").toBe(before + 1);
  });
});

describe("恢复交那台：恰好一问 session-open（本机 / 远端 × 不用 tmux / tmux 里 / 照这台）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });

  it("★ 六格各恢复一次 ⇒ 每格恰好一问、恢复那一形；放在哪没动过 ⇒ 不带 run；界面不问记录、不铸名、不记 pin", async () => {
    for (const origin of [LOCAL, "devbox"]) {
      for (const run of ["direct", "tmux", null] as const) {
        vi.mocked(openSession).mockClear();
        const sid = `${origin === LOCAL ? "l" : "r"}-${run ?? "m"}`;
        tm.ensureTab(sid, "/home/pi/proj", `/p/${sid}.jsonl`, origin);
        tm.onFace(sid, goldenFace("ended"));
        await home(tm).actions.resume(sid, pickAsk({ run, account: undefined, useBase: false }));
        expect(opens(), `${origin} · ${run}`).toEqual([[origin, { what: { kind: "resume", sid }, ...(run ? { run } : {}), local: origin === LOCAL }]]);
      }
    }
    expect(sessionReadCalls(vi.mocked(invoke).mock.calls, "probe_session_record")).toEqual([]);
    expect(historyCalls(vi.mocked(invoke).mock.calls, "update_history_metadata")).toHaveLength(0);
  });

  it("还不知道是哪一家也照交（哪一家那台从记录里读）；说不清那台在不在跑 ⇒ 不交", async () => {
    (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = true;
    try {
      tm.ensureTab("u1", "/home/pi/proj", "/p/u1.jsonl", "devbox");
    } finally {
      (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = false;
    }
    tm.onFace("u1", goldenFace("ended"));
    await home(tm).actions.resume("u1");
    expect(opens()).toEqual([["devbox", { what: { kind: "resume", sid: "u1" }, local: false }]]);
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
  const killBtn = (): HTMLButtonElement | null => itemLabeled(copyText("tabMenu.kill.label"));
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
  // ③ 命中 ≥2 个 ⇒ 拒绝，不折叠成第一个（`INVARIANTS §30` 同款分级：破坏性动作代价不可逆）。
  // 〔「改. 重新读取对所有tab生效」·「右键菜单那一项删（一个入口）」〕右键菜单里零处「重新读取」；
  // 正控：同一张菜单开出来了（有「在新窗口打开」）。栏顶也不再有那颗（推送断了由栏底机器条说），只在命令面板里。
  it("〔REREAD〕tab 右键菜单里没有「重新读取」，栏顶也没有", () => {
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL);
    rightClick("k1abcdef");
    const labels = [...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem]") ?? [])].map((b) => b.querySelector("[data-part=label]")?.textContent);
    expect(labels).toContain(copyText("history.menu.openWindow"));
    expect(labels.filter((l) => l?.includes(copyText("accountChip.menu.refresh")) || l === copyText("accountChip.menu.refresh"))).toEqual([]);
    expect(document.body.querySelector(".tab-bar-reread"), "栏顶不再有刷新").toBeNull();
  });

  // 栏顶一按 ⇒ 有打开 tab 的每台**恰好一次**整机 `resync`（不带 sid），台数 == 机器数（两向：多一台少一台都红）；
  // 各台并行（全部发出去之后才有一台回来）；在飞时再按不重入；做完按台说一句，没问到的说原因。
  it("〔REREAD〕命令面板「刷新各台机器上的会话」⇒ 每台恰好一次整机 resync，并行、按台汇总", async () => {
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
          JSON.stringify({ added: 0, removed: 0, retagged: 0, caught_up: a.origin === "laptop" ? 2 : 0, watchers: 1, unavailable: [] }),
        );
        return u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength);
      }
      return undefined;
    }));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL);
    tm.ensureTab("k2abcdef", "/home/u/q", "/p/k2.jsonl", LOCAL);
    tm.ensureTab("g1abcdef", "/home/g", "/p/g1.jsonl", "laptop");
    tm.ensureTab("b1abcdef", "/home/b", "/p/b1.jsonl", "box");
    tm.onFace("b1abcdef", goldenFace("ended")); // 已结束的也在栏上 ⇒ 那台也算
    tm.refreshAll();
    await flush();
    expect(asked.map(([o]) => o).sort()).toEqual([LOCAL, "box", "laptop"].sort());
    expect(asked.every(([, p]) => JSON.stringify(p) === "{}")).toBe(true);
    release();
    for (let i = 0; i < 5; i++) await flush();
    const last = vi.mocked(showActionFailureToast).mock.calls.at(-1)!;
    expect(last[0]).toBe(copyText("resync.machines.titlePartial"));
    const lines = last[1].split("\n");
    expect(lines).toHaveLength(3);
    expect(lines).toEqual(
      expect.arrayContaining([
        copyText("kit.text.machineSaid", { machine: copyText("detail.label.local"), said: copyText("resync.done.caughtNone") + copyText("resync.done.same") }),
        copyText("kit.text.machineSaid", { machine: "laptop", said: copyText("resync.done.caughtUp", { n: 2 }) + copyText("resync.done.same") }),
        expect.stringMatching(copyPattern("resync.machines.failed", { machine: "box" }, { whole: true })),
      ]),
    );
  });

  // 「结束会话…」能不能只照脸上的 `can.end`（A2-17-12-3）：不等问那台在哪个终端里（命中几个 · 不在终端里交了由那台判、那台说）。
  it("★ 结束项照 can.end：能 ⇒ 菜单一开就可点（问那台还没回）；不能 ⇒ 灰着、第二行是那一格的 why", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(() => new Promise(() => {})));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL);
    tm.onFace("k1abcdef", goldenFace("live-needs-you-tmux"));
    rightClick("k1abcdef");
    expect(killBtn()?.disabled).toBe(false);
    expect(await killConfirmText()).toContain(copyText("tabBatch.menu.stop", { n: 1 }));
    tm.ensureTab("k2abcdef", "/home/u/p", "/p/k2.jsonl", "hostA");
    tm.onFace("k2abcdef", goldenFace("live-working-plain"));
    rightClick("k2abcdef");
    expect(killBtn()?.disabled).toBe(true);
    expect(whyOf(killBtn())).toBe(copyText("terminal.empty.notOurs"));
  });

  // 本机**空 tmux**（claude 已退、只剩交互 shell）：恢复只在「恢复 ▸」里（运行于 tmux 那一项），
  // 菜单顶层不另出一项「在 tmux 会话里恢复」—— 那是同一个动作的重复（A2-17-7-1，本机远端同一份菜单）。
  const menuLabels = (): string[] =>
    [...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem] [data-part=label]") ?? [])].map((n) => n.textContent ?? "");

  it("本机空 tmux（Claude 已退出）→ 菜单顶层没有「在 tmux 会话里恢复」", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([
            // command 不是 claude ⇒ 空壳（claude 已退、只剩交互 shell）。
            { name: "i1-cc", path: "/p", command: "bash", attached: false, windows: 1, sid: "k1abcdef", agent: false },
          ])
        : Promise.resolve(undefined),
    ));
    tm.ensureTab("k1abcdef", "/home/u/p", "/p/k1.jsonl", LOCAL);
    rightClick("k1abcdef");
    await flush();
    expect(menuLabels()).not.toContain("在 tmux 会话里恢复");
  });

  // 右键 /「⋯」里终端那三样各自一项（拉前 · 在终端里打开 · 看它的终端），各照脸上各自那一格；本机远端同一道。
  const frontItem = (): HTMLButtonElement | null => itemLabeled(copyText("terminal.head.front"));
  const attachItem = (): HTMLButtonElement | null => itemLabeled(copyText("sessionHead.act.openTerm"));
  const viewItem = (): HTMLButtonElement | null => itemLabeled(copyText("sessionHead.act.terminal"));

  it("★ 右键菜单里终端三样各一项：拉前 · 在终端里打开 · 看它的终端，不再并成一颗「终端」", async () => {
    tm.ensureTab("A", "/a", "p", "hostA");
    tm.onFace("A", goldenFace("reconnectable"));
    rightClick("A");
    await flush();
    const labels = menuLabels();
    for (const k of ["terminal.head.front", "sessionHead.act.openTerm", "sessionHead.act.terminal"] as const) expect(labels.filter((l) => l === copyText(k)), k).toHaveLength(1);
    expect(labels).not.toContain(copyText("mobDrawer.item.terminal"));
  });

  it("★ 远端在 tmux 里 ⇒「在终端里打开」交那台另开外部终端窗口接进去；「看它的终端」开底部抽屉的终端页", async () => {
    const opened: string[] = [];
    tm.onOpenHere = (sid) => opened.push(sid);
    tm.ensureTab("A", "/a", "p", "hostA");
    tm.onFace("A", liveFace("working", true));
    rightClick("A");
    await flush();
    vi.mocked(openSession).mockClear();
    expect(attachItem()?.disabled).toBe(false);
    attachItem()!.click();
    await flush();
    expect(opens(), "交那台接回、这边开终端窗口").toEqual([["hostA", { what: { kind: "look", sid: "A" }, local: false }]]);
    expect(opened).toEqual([]);
    rightClick("A");
    await flush();
    viewItem()!.click();
    await flush();
    expect(opened).toEqual(["A"]);
  });

  it("★ 远端在 tmux 里 ⇒「拉前」点得了，点了问本机后端它在这台的哪个窗口（本机远端同一条）", async () => {
    tm.ensureTab("A", "/a", "p", "hostA");
    tm.onFace("A", liveFace("working", true));
    rightClick("A");
    await flush();
    expect(frontItem()?.disabled).toBe(false);
  });

  it("★ 远端不在任何宿主里 ⇒「在终端里打开」「看它的终端」灰着、第二行是那台写好的那一句；拉前照旧", async () => {
    const opened: string[] = [];
    tm.onOpenHere = (sid) => opened.push(sid);
    tm.ensureTab("A", "/a", "p", "hostA");
    const bare = liveFace("working", false);
    tm.onFace("A", bare);
    rightClick("A");
    await flush();
    expect(frontItem()?.disabled).toBe(false);
    expect(attachItem()?.disabled).toBe(true);
    expect(whyOf(attachItem())).toBe(bare.can.attach.can === "no" ? bare.can.attach.why : "量具自检：夹具该是不能");
    expect(viewItem()?.disabled).toBe(true);
    vi.mocked(openSession).mockClear();
    attachItem()!.click();
    viewItem()!.click();
    expect(opened).toEqual([]);
    expect(opens()).toEqual([]);
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
    const wrap = [...document.body.querySelectorAll("[role=none]")].find((w) => labelOf(w.children[0]) === copyText("history.row.resume"));
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
    tm.onFace(sid, goldenFace("ended"));
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
  it("★ 远端：一级项 ＋ flyout（最上面「恢复」· 运行于三项单选 · 在此目录新建会话）；勾画在那台答的默认上；没动过 ⇒ 不带 run，挪过 ⇒ 带", async () => {
    // 替身那台答的默认放在哪是「不用 tmux」（`chan-fake.ts::standingProduct`）。
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string) => (cmd === "list_remote_tmux" ? Promise.resolve([]) : Promise.resolve(undefined))));
    await openArchived();
    const fly = flyout();
    expect([...fly.children].map((c) => (c.getAttribute("role") === "presentation" ? `[${c.textContent}]` : c.getAttribute("role") === "separator" ? "—" : labelOf(c)))).toEqual([
      RESUME, "—", `[${copyText("resumeMenu.group.run")}]`, copyText("resumeMenu.run.managed"), TMUX, DIRECT, "—", copyText("history.menu.newInDir"),
    ]);
    expect(item(DIRECT)!.getAttribute("aria-checked"), "勾画在那台答的默认上").toBe("true");
    expect(item(RESUME)!.textContent, "「恢复」右侧灰字是勾着的方式").toContain(DIRECT);
    item(RESUME)!.click();
    await settle();
    expect(opens(), "人没动过 ⇒ 不带 run（那台照自己的设置）").toEqual([["devbox", { what: { kind: "resume", sid: "r1" }, local: false }]]);
    // 点「在 tmux 里」：只挪勾、菜单不关、什么都不起；勾在这个 tab 上记着
    rightClick("r1");
    await settle();
    vi.mocked(openSession).mockClear();
    item(TMUX)!.click();
    await settle();
    expect(document.body.querySelector("[role=menu]"), "点单选菜单不关").not.toBeNull();
    expect(item(TMUX)!.getAttribute("aria-checked")).toBe("true");
    expect(openSession).not.toHaveBeenCalled();
    item(RESUME)!.click();
    await settle();
    rightClick("r1");
    await settle();
    expect(item(TMUX)!.getAttribute("aria-checked"), "勾在这个 tab 上记着").toBe("true");
    expect(opens()).toEqual([["devbox", { what: { kind: "resume", sid: "r1" }, run: "tmux", local: false }]]);
  });

  it("本机：同一个组件（不再是单独一项）；在 tmux 里 ⇒ 同一问、带 run", async () => {
    await openArchived("l1", LOCAL);
    expect(menuLabels()).toEqual(expect.arrayContaining([RESUME, TMUX, DIRECT]));
    item(TMUX)!.click();
    item(RESUME)!.click();
    await settle();
    expect(opens()).toEqual([[LOCAL, { what: { kind: "resume", sid: "l1" }, run: "tmux", local: true }]]);
  });

  it("那台只有一个可选号：账号组照出（那个号 ＋ 基座）；勾基座再「恢复」⇒ 账号 0（#75 逃生口不退化）", async () => {
    accounts("z");
    await openArchived();
    expect(menuLabels()).toEqual(expect.arrayContaining(["z", BASE]));
    expect(item("z")!.getAttribute("role")).toBe("menuitemradio");
    item(BASE)!.click();
    expect(openSession).not.toHaveBeenCalled();
    item(RESUME)!.click();
    await settle();
    expect(opens()).toEqual([["devbox", { what: { kind: "resume", sid: "r1" }, account: { kind: "base" }, local: false }]]);
  });

  it("两个号：勾 b ⇒ 真的用 b 起（不是跟随）；勾 z ＋ 在 tmux 里 ⇒ 带 z 与 tmux（账号 × 方式两组正交）", async () => {
    accounts("z", "b");
    await openArchived();
    item("b")!.click();
    item(RESUME)!.click();
    await settle();
    rightClick("r1");
    await settle();
    item("z")!.click();
    item(TMUX)!.click();
    item(RESUME)!.click();
    await settle();
    expect(opens().map((c) => c[1])).toEqual([
      { what: { kind: "resume", sid: "r1" }, account: { kind: "named", name: "b" }, local: false },
      { what: { kind: "resume", sid: "r1" }, account: { kind: "named", name: "z" }, run: "tmux", local: false },
    ]);
  });

  // 账号名允许 `__base__`（账号也能在 app 之外写进账号库）：它是一个号，不是基座 —— 基座在类型上是另一支（`name: null`）。
  it("真实账号恰好叫 __base__ ⇒ 当成账号而非基座", async () => {
    accounts("z", "__base__");
    await openArchived();
    item("__base__")!.click();
    item(RESUME)!.click();
    await settle();
    expect(opens().map((c) => (c[1] as { account?: unknown }).account)).toEqual([{ kind: "named", name: "__base__" }]);
  });

  it("那台没开多账号 ⇒ 账号组不出", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withAccountReads((cmd: string) =>
      cmd === "list_remote_accounts" ? Promise.resolve({ available: false, error: null, meta: null, accounts: [] }) : Promise.resolve(undefined),
    )));
    await openArchived();
    const heads = [...flyout().querySelectorAll('[role="presentation"]')].map((e) => e.textContent);
    expect(heads).toEqual([copyText("resumeMenu.group.run")]);
  });

  // 用户手快：号还没到就已悬停展开了「恢复 ▸」；号一到整项换掉，flyout 不许无故收起。
  it("号到达前已悬停展开 flyout ⇒ 号到了之后 flyout 仍展开", async () => {
    vi.useFakeTimers();
    try {
      let resolveAccounts!: (v: unknown) => void;
      const pending = new Promise((r) => (resolveAccounts = r));
      vi.mocked(invoke).mockImplementation(withHistoryReads(withAccountReads((cmd: string) =>
        cmd === "list_remote_accounts" ? pending : Promise.resolve(undefined),
      )));
      tm.ensureTab("r1", "/home/pi/proj", "p", "devbox");
      tm.onFace("r1", goldenFace("ended"));
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
    }
  });

  // tab 栏可拖到 340px 宽，窄窗下 flyout 会溢出右边界：右侧放不下 ⇒ 往左翻。
  it("右侧空间不够时 flyout 往左翻，够用时不翻", async () => {
    const origInnerWidth = window.innerWidth;
    const origGBCR = HTMLElement.prototype.getBoundingClientRect;
    try {
      Object.defineProperty(window, "innerWidth", { value: 400, configurable: true });
      tm.ensureTab("r1", "/home/pi/proj", "p", "devbox");
      tm.onFace("r1", goldenFace("ended"));
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
      // 右边 380 + 160 > 400 − 8 ⇒ 翻到那一项左侧：视口里 200 − 160 = 40（子菜单是 fixed，写视口坐标）。
      expect(fly.style.left).toBe("40px");
      resumeBtn.click();
      HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
        if (this === resumeWrap) return { left: 10, right: 50, top: 100, bottom: 128 } as DOMRect;
        if (this === fly) return { width: 0 } as DOMRect;
        return origGBCR.call(this);
      };
      resumeBtn.click();
      expect(fly.style.left).toBe("50px");
    } finally {
      HTMLElement.prototype.getBoundingClientRect = origGBCR;
      Object.defineProperty(window, "innerWidth", { value: origInnerWidth, configurable: true });
    }
  });
});

const flushMicro = (): Promise<void> => new Promise((r) => setTimeout(r, 0));


describe("auto-e2e F-E4 可注入 confirm seam（endSession：交本机一项的 sessions-batch，与批量同一条；那台那一项走 terminal-end）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
  });
  const microFlush = async (): Promise<void> => {
    await Promise.resolve();
    await Promise.resolve();
  };
  // 单个杀交本机一项的那一批（`sessions-batch`；那台按 sid 认出是哪个、过三道门）⇒ 这里数交了几发、交的是哪一项。
  const killCalls = (): unknown[] => vi.mocked(askSessions).mock.calls;

  it("endSession 默认（不传 opts）→ 弹应用内对话框；答之前不杀，答「取消」⇒ 不杀", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.endSession("hostA", "s1", "cc-abc");
    await microFlush();
    expect(askDialogText(), "没弹应用内对话框").toContain(copyText("tabBatch.menu.stop", { n: 1 }));
    expect(killCalls(), "还没答就杀了").toHaveLength(0);
    await answerAskDialog(false);
    expect(killCalls()).toHaveLength(0);
    expect(confirmSpy, "还在用原生 window.confirm").not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("endSession 注入 confirm=()=>true → 不碰 window.confirm、交那台这一个 sid", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.endSession("hostA", "s1", "cc-abc", { confirm: () => true });
    await microFlush();
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(killCalls()).toHaveLength(1);
    expect(killCalls()[0]).toEqual(["stop", [{ machine: "hostA", sid: "s1" }]]);
    confirmSpy.mockRestore();
  });

  it("endSession 注入 confirm=()=>false → no-op，不 invoke", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    home(tm).actions.endSession("hostA", "s1", "cc-abc", { confirm: () => false });
    await microFlush();
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(killCalls()).toHaveLength(0);
    confirmSpy.mockRestore();
  });

  // 结束会话只一个确认框（A2-17-12-3）：单个交出去的就是多选 · 终端名单那一份，只列这一条的名字。
  it("★ endSession 的确认框 ＝ 结束会话那一个（endConfirm），列的是这一条的名字", async () => {
    const asked: unknown[] = [];
    home(tm).actions.endSession("hostA", "s1", "表格分页", {
      confirm: (m) => {
        asked.push(m);
        return false;
      },
    });
    await microFlush();
    expect(asked).toEqual([endConfirm(["表格分页"])]);
    expect(killCalls()).toHaveLength(0);
  });
});

describe("F79 杀死会话（二次确认 ＋ 交本机一项的 sessions-batch）", () => {
  beforeEach(() => vi.clearAllMocks());
  it("★ 二次确认通过 → 只发一发、发给本机后端那一批（一项带那台的机器名）；toast 照回包那一句（变已结束由会话流兜）", async () => {
    vi.mocked(askSessions).mockResolvedValueOnce(ONE_DONE as never);
    const tm = home(makeTM()).actions;
    tm.endSession("hostA", "s1", "cc-abc");
    await answerAskDialog(true);
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(askSessions).mock.calls).toEqual([["stop", [{ machine: "hostA", sid: "s1" }]]]);
    expect(vi.mocked(showActionFailureToast).mock.calls.at(-1)?.[0]).toBe("S-已结束");
  });
  it("二次确认取消 → 不交", async () => {
    const tm = home(makeTM()).actions;
    tm.endSession("hostA", "s1", "cc-abc");
    await answerAskDialog(false);
    expect(askSessions).not.toHaveBeenCalled();
  });
  it("那台没杀、码是关卡 2 拒的（wrong_owner）⇒ 提示带「对齐后重试」，那一句照回包那一项", async () => {
    const one = { machine: "hostA", sid: "s1", outcome: "failed", why: "wrong_owner", said: "不是这一端起的", copyDetail: "x", title: "cc-abc", session: null };
    vi.mocked(askSessions).mockResolvedValueOnce({ ...ONE_DONE, results: [one], said: "S-没成", failed: true } as never);
    const tm = home(makeTM()).actions;
    tm.endSession("hostA", "s1", "cc-abc", { confirm: () => true });
    await new Promise((r) => setTimeout(r, 0));
    const [title, body] = vi.mocked(showActionFailureToast).mock.calls.at(-1)!;
    expect(title).toBe("S-没成");
    expect(body).toContain(copyText("resync.retry.hint"));
  });
  it("别的没成 ⇒ 照回包摆（那一句 · 逐条 · 复制详情）", async () => {
    vi.mocked(askSessions).mockResolvedValueOnce({ ...ONE_DONE, said: "S-没成", failed: true, lines: ["L"], copyDetail: "D" } as never);
    const tm = home(makeTM()).actions;
    tm.endSession("hostA", "s1", "cc-abc", { confirm: () => true });
    await new Promise((r) => setTimeout(r, 0));
    const [title, , opts] = vi.mocked(showActionFailureToast).mock.calls.at(-1)!;
    expect(title).toBe("S-没成");
    expect((opts as { detail?: string }).detail).toBe("D");
  });
  it("本机后端问不到 ⇒ 一条失败提示（点名那一条）", async () => {
    vi.mocked(askSessions).mockRejectedValueOnce(new Error("gone"));
    const tm = home(makeTM()).actions;
    tm.endSession("hostA", "s1", "cc-abc", { confirm: () => true });
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(failToast).mock.calls.at(-1)?.[0]).toBe(copyText("tabSessionActions.kill.failed", { title: "cc-abc" }));
  });
});

// 「F70 会话改动集聚合」那一组搬进文件末尾「会话事实」那组：改动文件集由后端出成品（口径 · 去重 · 近因序
//   住 `tests/backend/observe/recognize/facts_tests.rs`），前端这边只剩「成品 ⇒ 监控板 peek 的透传」。

describe("F91b TabManager.peekSession（监控板内容 peek 纯读派生）", () => {

  it("unknown sid → null", () => {
    const tm = makeTM();
    expect(tm.peekSession("nope")).toBeNull();
  });

  it("子运行照那台写好的三组摆（在跑 ++ 最近 ++ 更早）、头那一句照抄 summary；model / 改过的文件透传", () => {
    const tm = makeTM();
    const tab = tm.ensureTab("s1", "/proj", "/p/s1.jsonl", LOCAL);
    tab.usage = usageOf(1, "claude-opus-4-8");
    tab.touchedFiles.add("/proj/a.ts");
    tab.touchedFiles.add("/proj/b.ts");
    // 表序与三组序故意对不上：界面只照三组摆，不按状态另排。
    tm.onSessionRuns({
      sid: "s1",
      runs: [
        { run: "1", label: "done1", label_text: "done1", state: "done", status_text: "核心·完成" },
        { run: "2", label: "run1", label_text: "run1", state: "running", status_text: "核心·在跑" },
        { run: "3", label: "stop1", label_text: "stop1", state: "stopped", status_text: "核心·已中止" },
        { run: "4", label: "run2", label_text: "run2", state: "running", status_text: "核心·在跑" },
      ],
      ended: [],
      groups: { running: ["4", "2"], recent: ["3"], older: ["1"], peek: ["4", "2", "3", "1"] },
      summary: "核心·agent 9 · 2 在跑",
    });

    const p = tm.peekSession("s1");
    expect(p).not.toBeNull();
    expect(p!.model).toBe("claude-opus-4-8");
    expect(p!.recentFiles).toEqual(["/proj/a.ts", "/proj/b.ts"]);
    expect(p!.agents.map((a) => a.label)).toEqual(["run2", "run1", "stop1", "done1"]);
    expect(p!.agents.map((a) => a.status)).toEqual(["running", "running", "stopped", "done"]);
    expect(p!.summary).toBe("核心·agent 9 · 2 在跑");
    expect(p!.more).toBeNull();
  });

  it("peek 只装那台挑好的那几个、「+N」照抄那台写的；格子徽标照抄那台写的「在跑」那一句（界面不数）", () => {
    const tm = makeTM();
    tm.ensureTab("s3", "/proj", "/p/s3.jsonl", LOCAL);
    tm.onFace("s3", liveFace("working"));
    const ids = Array.from({ length: 10 }, (_, i) => String(i));
    tm.onSessionRuns({
      sid: "s3",
      runs: ids.map((id) => ({ run: id, label: `r${id}`, label_text: `r${id}`, state: Number(id) < 3 ? "running" : "done", status_text: "核心·态" })),
      ended: [],
      groups: {
        running: ["0", "1", "2"],
        recent: ["3", "4", "5", "6", "7"],
        older: ["8", "9"],
        running_text: "核心·在跑 · agent 5",
        peek: ["0", "1", "2", "3", "4", "5"],
        peek_more_text: "核心·另 4 个",
      },
      summary: "核心·agent 10 · 3 在跑",
    } as never);
    const p = tm.peekSession("s3")!;
    expect(p.agents.map((a) => a.label)).toEqual(["r0", "r1", "r2", "r3", "r4", "r5"]);
    expect(p.more).toBe("核心·另 4 个");
    expect(tm.snapshotSessions().find((x) => x.sessionId === "s3")?.running).toBe("核心·在跑 · agent 5");
  });

  it("无 usage / 无 agent / 无改文件 → 字段空但不报错", () => {
    const tm = makeTM();
    tm.ensureTab("s2", "/x", "/p/s2.jsonl", LOCAL);
    const p = tm.peekSession("s2");
    expect(p).toEqual({ model: null, recentFiles: [], agents: [], summary: null, more: null });
  });

  // F91b-fix(batch18)：touchedFiles 近因序（peek `recentFiles` 尾部 = 最近改的）。近因序今天由后端排
  //   （`recognize/facts_tests.rs::the_three_facts_follow_the_moved_rules`），前端只保序透传 —— 那一条在文件末尾「〔STC〕会话事实」那组。
});

describe("account-ux U5 tab 徽章：照会话那台写好的那一份（`session-account`）", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    appStore.sessionAccount.__resetForTests(new Map());
    tm = makeTM();
  });
  const badge = (): HTMLElement | null =>
    document.body.querySelector<HTMLElement>(".tab-acct-badge");
  /** 那台写好的这条会话的号（夹具照线上形手写）。 */
  function said(sid: string, origin: string, p: Partial<SessionAccountReply>): void {
    const read: SessionAccountReply = {
      account: "b",
      name: "b",
      when: "now",
      from: null,
      accountLine: copyText("tabBar.hover.account", { name: "b" }),
      fromLine: null,
      usage: null,
      stale: false,
      hover: [["b"]],
      sampled: null,
      differs: false,
      badge: null,
      ...p,
    };
    putIn(appStore.sessionAccount, sid, { origin, read });
  }
  const announce = (sid: string): void => {
    tm.createSkeletonTab(sid, "/w", "devbox", false, null);
    tm.onFace(sid, liveFace("working"));
  };

  it("★ 那台说与默认号不同 ⇒ 挂头像；此刻的实心、上次的幽灵；悬停那一句照那台写的", () => {
    announce("r1");
    expect(badge()?.style.display, "还没问回来 ⇒ 不挂").toBe("none");
    said("r1", "devbox", { differs: true, badge: { ghost: false, title: "账号 b（那台写的）" } });
    const el = badge();
    expect(el?.style.display).not.toBe("none");
    expect(el?.querySelector(".acct-avatar")).not.toBeNull();
    expect(el?.querySelector(".acct-avatar.ghost")).toBeNull();
    expect(el?.title).toBe("账号 b（那台写的）");
    said("r1", "devbox", { when: "last", differs: true, badge: { ghost: true, title: "账号 b（那台写的）" } });
    expect(badge()?.querySelector(".acct-avatar.ghost"), "上次的号 ⇒ 幽灵头像").not.toBeNull();
  });
  it("那台说一致（没有徽章）⇒ 不挂头像；悬停卡里照那台写的那一行、换来那一行", () => {
    announce("r1");
    said("r1", "devbox", { fromLine: "从 a 换过来（那台写的）" });
    expect(badge()?.style.display).toBe("none");
    expect(cardText(tm, "r1")).toContain(copyText("tabBar.hover.account", { name: "b" }));
    expect(cardText(tm, "r1")).toContain("从 a 换过来（那台写的）");
  });
  it("那一份是别的台的（sid 撞了）⇒ 不认", () => {
    announce("r1");
    said("r1", "nano", { differs: true, badge: { ghost: false, title: "x" } });
    expect(badge()?.style.display).toBe("none");
  });
});

/**
 * ★★ ↗：点那一下只发壳一条命令（`bring_terminal_to_front {origin, sid}`）：问本机后端会话在这台电脑上的哪儿、找窗口、拉前都在壳里，
 * 回切过去了或本机后端写好的那一张（壳照结局码挑）；本机会话与远端会话同一条路、同一个形。
 * 前端不问后端、不排先后、不按结局挑字：交来的那一张照摆（句子逐种判在 `tests/common/face-core/front_tests.rs`）。
 * 切过去了什么都不出；壳拒了（问本机后端没问成）⇒ 照壳交来的那一句进失败提示。
 */
describe("：↗ 只发壳一条命令", () => {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
  /** 一张写好的浮层（字是夹具里的占位，判的是照摆）。 */
  const face = (title: string, body: string, tone: "plain" | "warn" | "fail", acts: unknown[] = [], hint?: string) => ({ title, body, tone, acts, ...(hint ? { hint } : {}) });
  const said = (f: ReturnType<typeof face>) => ({ kind: "said", ...f });
  /** 壳那一条命令回什么。 */
  function answer(front: () => Promise<unknown>): void {
    mockInvoke.mockImplementation((cmd: string) => (cmd === "bring_terminal_to_front" ? front() : Promise.resolve([])));
  }
  const shellCalls = (): unknown[] => (mockInvoke.mock.calls as [string, unknown][]).filter(([c]) => c === "bring_terminal_to_front").map(([, a]) => a);
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    __setHostFactsForTests(factsOn("windows"));
    document.querySelectorAll("[data-role=front-result]").forEach((e) => e.closest("[role=dialog]")?.remove());
  });
  afterEach(() => __setHostFactsForTests(null));

  /** 拉窗那一下（会话头 · 命令面板 · 快捷键同一条路，`frontFor`）。每一下之前清掉底部条的记账：量的只是这一下出了什么。 */
  async function clickFront(tm: TabManager, sid: string): Promise<void> {
    vi.mocked(showActionFailureToast).mockClear();
    tm.frontFor(sid);
    for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
  }
  /** 这一下出的那一条底部条（结局只有这一种样子）；没出 ⇒ `null`。 */
  const pop = (): { title: string; detail: string; opts: ToastOptions } | null => {
    const c = vi.mocked(showActionFailureToast).mock.calls.at(-1);
    return c ? { title: c[0], detail: c[1], opts: c[2] ?? {} } : null;
  };
  const popText = (): string => (pop() ? `${pop()!.title}\n${pop()!.detail}` : "");
  /** 那一条上的按钮：修法在前，带复制详情的末尾一颗［复制详情］（底部条自己那一颗）。 */
  const popButtons = (): string[] => {
    const p = pop();
    if (!p) return [];
    const acts = p.opts.action === undefined ? [] : Array.isArray(p.opts.action) ? p.opts.action : [p.opts.action];
    return [...acts.map((a) => a.label), ...(p.opts.detail ? [copyText("detail.act.copy")] : [])];
  };
  const press = async (label: string): Promise<void> => {
    const p = pop()!;
    const acts = Array.isArray(p.opts.action) ? p.opts.action : p.opts.action ? [p.opts.action] : [];
    acts.find((a) => a.label === label)!.run();
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
  };

  it("★★ 本机会话与远端会话：各发壳一条 `bring_terminal_to_front {origin, sid}`；不问 session-window、别的一概不发", async () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL);
    tm.createSkeletonTab("r1", "/p", "devbox", false);
    for (const [sid, origin] of [
      ["l1", LOCAL],
      ["r1", "devbox"],
    ] as const) {
      // 建标签页那几下各自起的读（骨架 · 会话事实 …）先落完，量的只是拉窗那一下发了什么（慢机器上它们会晚到、混进下面那一段）。
      answer(() => Promise.resolve({ kind: "switched" }));
      // 等到连着 20 拍都没有新的一问（上限 500 拍）：机器慢时那几下读会晚到。
      for (let i = 0, still = 0, seen = -1; i < 500 && still < 20; i++) {
        await new Promise((r) => setTimeout(r, 0));
        const n = mockInvoke.mock.calls.length;
        still = n === seen ? still + 1 : 0;
        seen = n;
      }
      mockInvoke.mockReset();
      answer(() => Promise.resolve({ kind: "switched" }));
      await clickFront(tm, sid);
      expect(shellCalls(), sid).toEqual([{ origin, sid }]);
      const others = (mockInvoke.mock.calls as [string, unknown][]).filter(([c]) => c !== "bring_terminal_to_front");
      expect(others.map(([c, a]) => (c === "chan_call" ? `chan_call:${(a as ChanCallArgs).op}` : c)), `${sid}：拉窗只该有那一条壳命令`).toEqual([]);
      expect(pop(), `${sid}：切过去了，什么都不出`).toBeNull();
    }
  });

  it("★★ 壳交来的那一张照摆成底部一条（分不清：照实说拉不了、带灰字办法）；没有锚在按钮上的浮层", async () => {
    answer(() => Promise.resolve(said(face("后端写的标题", "后端写的正文", "warn", [], "后端写的灰字"))));
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "devbox", false);
    await clickFront(tm, "r1");
    expect(pop()?.title).toBe("后端写的标题");
    expect(pop()?.opts.level).toBe("warn");
    expect(popText()).toContain("后端写的正文");
    expect(popText()).toContain("后端写的灰字");
    expect(popButtons(), "那一张没带按钮 ⇒ 不出").toEqual([]);
    expect(document.querySelector("[data-role=front-result]"), "不另起浮层").toBeNull();
  });

  it("★ 那一张的语气与按钮照摆", async () => {
    const cases: [unknown, string, string, string[], string][] = [
      [face("甲", "甲正文", "plain", [{ kind: "open-in-terminal" }]), "甲", "甲正文", [copyText("mobDrawer.item.terminal")], "info"],
      [face("乙", "乙正文", "plain"), "乙", "乙正文", [], "info"],
      [face("丙", "", "fail", [{ kind: "retry" }, { kind: "copy", detail: "d" }]), "丙", "", [copyText("front.act.retry"), copyText("detail.act.copy")], "error"],
      [face("丁", "丁正文", "warn", [{ kind: "update" }]), "丁", "丁正文", [copyText("front.act.update")], "warn"],
    ];
    const tm = makeTM();
    tm.createSkeletonTab("r1", "/p", "devbox", false);
    for (const [f, title, body, buttons, level] of cases) {
      mockInvoke.mockReset();
      answer(() => Promise.resolve(said(f as ReturnType<typeof face>)));
      await clickFront(tm, "r1");
      expect(pop()?.title, title).toBe(title);
      if (body) expect(popText(), title).toContain(body);
      expect(popButtons(), title).toEqual(buttons);
      expect(pop()?.opts.level, title).toBe(level);
    }
  });

  it("★ 壳拒了（问本机后端没问成）⇒ 不出空的一条、接住：照壳交来的那一句进失败提示（详情可复制）", async () => {
    const tm = makeTM();
    tm.createSkeletonTab("r8", "/p", "devbox", false);
    const refused = Object.assign(new Error("壳拒的那一句"), { detail: "壳的详情" });
    answer(() => Promise.reject(refused));
    vi.mocked(failToast).mockClear();
    await clickFront(tm, "r8");
    expect(vi.mocked(failToast).mock.calls, "失败提示的标题是那一下的说法「切到终端」").toEqual([[copyText("terminal.head.front"), refused]]);
    expect(pop(), "拿不到那一张就不出空的一条").toBeNull();
  });

  it("★ 壳那一张带［在 cc-monitor 里打开］（Wayland 桌面上切不了）⇒ 点了交 main 开这个会话的终端页", async () => {
    answer(() => Promise.resolve(said(face("切不了", "Wayland", "plain", [{ kind: "open-here" }]))));
    const tm = makeTM();
    const opened: string[] = [];
    tm.onOpenHere = (sid) => opened.push(sid);
    tm.createSkeletonTab("r5", "/p", "devbox", false);
    await clickFront(tm, "r5");
    expect(pop()?.title).toBe("切不了");
    expect(popButtons()).toEqual([copyText("front.act.openHere")]);
    await press(copyText("front.act.openHere"));
    expect(opened).toEqual(["r5"]);
  });

  it("★ 切过去了：什么都不出（不打勾、不弹）", async () => {
    answer(() => Promise.resolve({ kind: "switched" }));
    const tm = makeTM();
    tm.createSkeletonTab("r2", "/p", "devbox", false);
    await clickFront(tm, "r2");
    expect(mockInvoke.mock.calls.map((c) => c[0]), "`attachable:false` 不在前端短路拉窗").toContain("bring_terminal_to_front");
    expect(pop(), "成功了还出了一条").toBeNull();
  });

  it("★ 在飞时同一个会话再点不重发", async () => {
    let done: (v: unknown) => void = () => {};
    answer(() => new Promise((r) => (done = r)));
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL);
    tm.frontFor("l1");
    tm.frontFor("l1");
    for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
    expect(shellCalls().length, "在飞时再点不重发").toBe(1);
    done({ kind: "switched" });
  });

  it("★ ［更新］就地做：不开设置，经共用部署模块换上这一版；过程与结局各是一条底部条", async () => {
    const tm = makeTM();
    tm.createSkeletonTab("r5", "/p", "devbox", false);
    let finish: (v: string | null) => void = () => {};
    vi.mocked(updateBackendOf).mockImplementation((_o, _m, onStart) => {
      onStart?.();
      return new Promise((r) => (finish = r));
    });
    // 那台后端比这一版旧：本机后端写好的那一张带［更新］（壳原样交来）。
    const OLD = said(face("那台要更新", "", "warn", [{ kind: "update" }]));
    const clickUpdate = async (): Promise<void> => {
      mockInvoke.mockReset();
      mockInvoke.mockImplementation((cmd: string) => (cmd === "bring_terminal_to_front" ? Promise.resolve(OLD) : Promise.resolve([])));
      await clickFront(tm, "r5");
      expect(popButtons(), "量具自检：那一条上有［更新］").toContain(copyText("front.act.update"));
      await press(copyText("front.act.update"));
    };
    await clickUpdate();
    expect(vi.mocked(updateBackendOf).mock.calls.map((c) => [c[0], c[1]])).toEqual([["devbox", "devbox"]]);
    expect("onUpdateMachine" in tm, "［更新］还留着交给主窗口开设置窗的钩子").toBe(false);
    expect(pop()?.title, "在飞：说正在更新").toBe(copyText("front.title.updating", { machine: "devbox" }));
    expect(popButtons()).toEqual([]);
    finish("已部署 X");
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(pop()?.title).toBe(copyText("front.title.updated", { machine: "devbox" }));
    expect(popText()).toContain("已部署 X");
    expect(popButtons(), "换上了：不挂［重试］").toEqual([]);
    expect(popText(), "重拨成了：不叫人去重连").not.toContain(copyText("front.body.redialFailed"));
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "backend_start").map((c) => c[1]), "换上了就重拨那台").toEqual([{ origin: "devbox" }]);

    // 换上了、重拨没成：才说要重连，给［重新连接］。
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockResolvedValue("已部署 Y");
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "bring_terminal_to_front" ? Promise.resolve(OLD) : cmd === "backend_start" ? Promise.reject(new Error("拨不通")) : Promise.resolve([]),
    );
    await clickFront(tm, "r5");
    await press(copyText("front.act.update"));
    expect(pop()?.title).toBe(copyText("front.title.updated", { machine: "devbox" }));
    expect(popText()).toContain(copyText("front.body.redialFailed"));
    expect(popButtons()).toEqual([copyText("history.list.reconnect")]);

    // 部署失败：红着说；壳带回的复制详情挂在［复制详情］上（复制的是那份详情，不是那一句）；［重试］再更新一次。
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockRejectedValue(Object.assign(new Error("上传失败"), { detail: "码：refused" }));
    await clickUpdate();
    expect(pop()?.title).toBe(copyText("front.title.updateFailed", { machine: "devbox" }));
    expect(pop()?.opts.level).toBe("error");
    expect(popText()).toContain("上传失败");
    expect(popButtons()).toEqual([copyText("front.act.retry"), copyText("detail.act.copy")]);
    expect(pop()?.opts.detail).toBe("码：refused");
    await press(copyText("front.act.retry"));
    expect(vi.mocked(updateBackendOf), "失败态的［重试］是再更新一次").toHaveBeenCalledTimes(2);

    // 会打断什么的确认框里点了取消：什么都不做、不再出一条。
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockResolvedValue(null);
    await clickUpdate();
    expect(popButtons(), "取消了：没有新的一条").toContain(copyText("front.act.update"));
  });

  it("★ 机器条那一行的［更新］与 ↗ 浮层 · 机器卡同一个动作（共用部署模块，先问会打断什么），不开设置窗", async () => {
    const tm = makeTM();
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockResolvedValue("已部署 Z");
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue([]);
    tm.fixMachine("devbox", "update");
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(updateBackendOf).mock.calls.map((c) => [c[0], c[1]])).toEqual([["devbox", "devbox"]]);
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "open_settings_window"), "不开设置窗").toEqual([]);
  });

  it("★ 机器条［更新］没换上 ⇒ 不弹红 toast（那一句由那台的状态成品写在机器条那一行，A2-17-14-2）", async () => {
    const tm = makeTM();
    vi.mocked(updateBackendOf).mockReset();
    vi.mocked(updateBackendOf).mockRejectedValue(new Error("推不上去-甲"));
    vi.mocked(failToast).mockClear();
    tm.fixMachine("devbox", "update");
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(failToast)).not.toHaveBeenCalled();
  });

  it("★ ［重新连接］那台拒了（停用着 · 正在连 · 机器表里没有）⇒ 那一句原样进状态栏消息，不只进日志", async () => {
    const tm = makeTM();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string) => (cmd === "backend_start" ? Promise.reject(new Error("那台停用着-甲")) : Promise.resolve([])));
    vi.mocked(noteMessage).mockClear();
    tm.fixMachine("devbox", "retry");
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "backend_start").map((c) => c[1])).toEqual([{ origin: "devbox" }]);
    expect(vi.mocked(noteMessage).mock.calls.map((c) => c[0])).toEqual(["那台停用着-甲"]);
  });
});

/**
 * ★★ ↗ 在 Windows · Linux 以外**别装得能用**。
 *
 * macOS 上 ↗ 的最后一跳在 Rust 侧是恒失败的桩 ⇒ 那颗按钮每点必败。门住 `terminal-front.ts`；`unknown` 照常显示（读不到壳那一份时的样子 `UNKNOWN_HOST`）。
 */
describe("过程里还没结果的那几步照会话事实画（后端 pending[].state · needs.call）", () => {
  it("★ 事实说 b1 在等批准 ⇒ 等你批准；b2 在跑 ⇒ 在跑；换过来照换；事实里已经没有的那一步 ⇒ 状态不明；结果到了的那一步不动", () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL);
    const tab = home(tm).store.tabs.get("l1")!;
    const step = (id: string): HTMLElement => {
      const line = buildStepLine("Bash", { tool: "Bash", arg: id, known: true } as never, id);
      tab.streamEl.appendChild(line);
      return line;
    };
    const [b1, b2] = [step("b1"), step("b2")];
    expect([b1.dataset.state, b2.dataset.state], "事实到之前不画").toEqual(["pending", "pending"]);
    const call = (id: string, state: "running" | "awaiting" | "unclear", why: "noWriter" | "untracked" | null = null) => ({ id, name: "Bash", what: id, at: null, atMs: null, state, why, text: state === "unclear" ? "U" : null, whyText: why === null ? null : `W:${why}` });
    const paint = (pending: Tab["pending"], needs: Tab["needs"]): void => {
      tab.pending = pending;
      tab.needs = needs;
      (home(tm).view as unknown as { paintStepWaits(t: Tab): void }).paintStepWaits(tab); // 流里补画住流视图（8a ④）
    };
    const approve = (c: string) => ({ kind: "approve" as const, tool: "Bash", call: c, what: c, text: copyText("beSession.needs.approve"), head: "h", headCode: c, tone: "need" as const, rank: 1, waitedMs: 125_000, ask: null, manualText: null, receivedAt: Date.now() });
    paint([call("b1", "awaiting"), call("b2", "running")], approve("b1"));
    expect([b1.dataset.state, b2.dataset.state]).toEqual(["awaiting", "running"]);
    expect(b1.querySelector(".step-right")?.textContent, "右侧已等多久").toBe(fmtDur(125));
    paint([call("b1", "running"), call("b2", "awaiting")], approve("b2"));
    expect([b1.dataset.state, b2.dataset.state]).toEqual(["running", "awaiting"]);
    paint([call("b2", "unclear", "noWriter")], null);
    expect([b1.dataset.state, b2.dataset.state]).toEqual(["unclear", "unclear"]);
    expect(b2.querySelector<HTMLElement>(".step-right")?.title, "悬停照后端写好的那一句").toBe("W:noWriter");
    settleStepLine(b1, undefined, { ok: true } as never, false);
    paint([call("b1", "running")], null);
    expect(b1.dataset.state, "结果到了的不动").toBe("ok");
  });
});

describe("agent 来话的事件条：标签按运行表查（流视图把运行表交给渲染）", () => {
  it("★ 运行表里有那个运行 ⇒ 事件条用表里的标签，不用来话自带的名字", () => {
    const tm = makeTM();
    tm.ensureTab("l3", "/w", "p", LOCAL);
    tm.switchTo("l3");
    tm.onSessionRuns({ sid: "l3", runs: [{ run: "a7", label: "审面板交互", label_text: "核心·审面板交互" }], ended: [] } as never);
    const label = (tm as unknown as { view: { host: { runLabelOf(sid: string, run: string): string | undefined } } }).view.host.runLabelOf;
    expect(label("l3", "a7")).toBe("核心·审面板交互");
    expect(label("l3", "zz"), "表里没有 ⇒ 不给，用来话自带的").toBeUndefined();
  });
});

describe("新出现的一行淡入（起步那一批不算）", () => {
  it("第一批画出去之前建的行不带淡入；之后再长出来的带；动画放完就摘", () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL);
    const row = (sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root;
    expect(row("l1").classList.contains("tab-enter"), "起步那一行不淡入").toBe(false);
    tm.ensureTab("l2", "/w2", "q", LOCAL);
    expect(row("l2").classList.contains("tab-enter"), "之后新出现的一行淡入").toBe(true);
    row("l2").dispatchEvent(new Event("animationend"));
    expect(row("l2").classList.contains("tab-enter")).toBe(false);
  });
});

describe("拉前 · 看它的终端：快捷键 · 命令面板各走各的那一格（本机远端同一道）", () => {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });
  const frontSent = (): string[] =>
    (mockInvoke.mock.calls as [string, unknown][]).filter(([c]) => c === "bring_terminal_to_front").map(([c]) => c);
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
  };

  it("★ 拉前做不了（那台写好的那一句）⇒ 那一句进状态栏消息、不弹 toast、**不发 IPC**", async () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL);
    tm.onFace("l1", goldenFace("gone"));
    tm.switchTo("l1");
    tm.frontActive();
    await settle();
    expect(frontSent(), "做不了还发了拉窗的 IPC —— 那是装作试过").toEqual([]);
    expect(showActionFailureToast, "做不了不弹 toast").not.toHaveBeenCalled();
    const gone = goldenFace("gone").can.front;
    expect(noteMessage).toHaveBeenCalledWith(copyText("terminal.head.front"), gone.can === "no" ? gone.why : "量具自检：夹具该是不能");
  });

  it("★ 拉前：本机 · 远端 tmux 会话都走拉窗那一条（壳 `bring_terminal_to_front`），不开终端页", async () => {
    mockInvoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "bring_terminal_to_front" ? { kind: "switched" } : undefined));
    const tm = makeTM();
    const opened: string[] = [];
    tm.onOpenHere = (sid) => opened.push(sid);
    for (const [sid, origin] of [["l1", LOCAL], ["r1", "devbox"]] as const) {
      mockInvoke.mockClear();
      tm.ensureTab(sid, "/w", "p", origin);
      tm.onFace(sid, liveFace("working", true));
      tm.switchTo(sid);
      tm.frontActive();
      await settle();
      expect(frontSent(), sid).toEqual(["bring_terminal_to_front"]);
    }
    expect(opened).toEqual([]);
  });

  it("★ 看它的终端：开底部抽屉的终端页、不发拉窗的 IPC；做不了 ⇒ 那一句进消息", async () => {
    const tm = makeTM();
    const opened: string[] = [];
    tm.onOpenHere = (sid) => opened.push(sid);
    tm.ensureTab("r1", "/p", "p", "devbox");
    tm.onFace("r1", liveFace("working", true));
    tm.switchTo("r1");
    tm.viewActive();
    await settle();
    expect(frontSent()).toEqual([]);
    expect(opened).toEqual(["r1"]);
    const bare = liveFace("working", false);
    tm.onFace("r1", bare);
    tm.viewActive();
    expect(opened).toEqual(["r1"]);
    expect(noteMessage).toHaveBeenCalledWith(copyText("sessionHead.act.terminal"), bare.can.view.can === "no" ? bare.can.view.why : "量具自检：夹具该是不能");
  });
});

describe("tab 行尾 ↗ 拉前（照 5.1.1）：一直在，做不了灰着、悬停说为什么", () => {
  const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "bring_terminal_to_front" ? { kind: "switched" } : undefined));
  });
  const focusBtn = (tm: TabManager, sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root.querySelector<HTMLElement>(".tab-focus")!;
  const sent = (): number => (mockInvoke.mock.calls as [string, unknown][]).filter(([c]) => c === "bring_terminal_to_front").length;

  it("★ 远端 tmux 会话：↗ 点得了，点了走拉窗那一条（不切 tab）", async () => {
    const tm = makeTM();
    tm.ensureTab("r1", "/p", "p", "devbox");
    tm.onFace("r1", liveFace("working", true));
    const b = focusBtn(tm, "r1");
    expect(b, "行尾有 ↗").not.toBeNull();
    expect(b.getAttribute("aria-disabled")).toBeNull();
    b.click();
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(sent()).toBe(1);
  });

  it("★ 已结束：↗ 灰着、data-why 是那台那一句；点了不发 IPC", async () => {
    const tm = makeTM();
    tm.ensureTab("l1", "/w", "p", LOCAL);
    const ended = goldenFace("ended");
    tm.onFace("l1", ended);
    const b = focusBtn(tm, "l1");
    expect(b.getAttribute("aria-disabled")).toBe("true");
    expect(b.dataset.why).toBe(ended.can.front.can === "no" ? ended.can.front.why : "量具自检：夹具该是不能");
    b.click();
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(sent()).toBe(0);
  });
});

describe("F15 每行代价的现状基线", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.useFakeTimers();
    tm = makeTM();
    f15.cardsAdded = 0;
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
    record: { t: "reply", id: `u-${seq}`, timestamp: "2026-08-06T00:00:00Z" },
  });

  it("★ live 每来一行建了卡，折叠层就听一次「有新卡」（帧末合批在它里面）", () => {
    tm.ensureTab("s1", "/w", "/w/s1.jsonl", LOCAL);
    for (let i = 1; i <= 5; i++) tm.onLine(line("s1", i) as never);
    expect(
      f15.cardsAdded,
      "喂 5 行、折叠层听到的次数变了（该是 5）。0 ⇒ 多半是 `renderContentRecord` 的 mock 又退回空壳（`inserted` 恒假）。",
    ).toBe(5);
  });

  it("★ 会话流的 branch 格 ⇒ 那个 tab 的折叠层按整份清单重折；没有那个 tab ⇒ 不收", () => {
    tm.ensureTab("s1", "/w", "/w/s1.jsonl", LOCAL);
    f15.setList.length = 0;
    tm.onSessionBranch({ sid: "s1", off: ["u-2", "u-3"], runs: [{ at: 0, n: 2, said: "×2" }] });
    tm.onSessionBranch({ sid: "nope", off: ["x"], runs: [{ at: 0, n: 1, said: "×1" }] });
    expect(f15.setList).toEqual([["u-2", "u-3"]]);
  });

  it("★ 后台 tab 每来一行，tab bar 就整刷一次", () => {
    tm.ensureTab("front", "/w", "/w/front.jsonl", LOCAL);
    tm.ensureTab("bg", "/w", "/w/bg.jsonl", LOCAL);
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
    // ⚠ 「灰着但还在」这句话来自 `tabs.ts` 自己的旧头注，也是要求原话。
    document.body.innerHTML = "";
    const orphanBar = document.createElement("div"); // 刻意不 append 到 body
    const streamRootEl = document.createElement("div");
    document.body.append(streamRootEl);
    const tm2 = owned(announced(new TabManager(orphanBar, streamRootEl)));
    tm2.ensureTab("a", "/c", "p", LOCAL);
    tm2.ensureTab("b", "/c", "p", LOCAL);
    tm2.switchTo("a");
    tm2.onFace("b", goldenFace("ended"));
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
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    tm.ensureTab("c", "/c3", "p", LOCAL);
    flushBar();
    stubRects();
    expect(order()).toEqual(["a", "b", "c"]);
    // 拖第三个（c）到最上面：clientY=10 落在第一条（0..40）的上半 ⇒ 插到 a 之前。
    dragTo("c", 10);
    expect(order()).toEqual(["c", "a", "b"]);
  });

  // 〔「删掉树」〕原「★ P7a2-Y3：拖交互 tab 时它的 bg 子串跟着走」一格删了 ——
  //   现在反过来钉：拖同 cwd 的宿主**只有它自己动**（本文件末尾「〔BG1〕」那组的 ②，两种 tab 各跑）。

  it("★ P7a2-D：拖动时**落点看得见**（插入线），撕离那一路不指示，拖完必须清掉", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    tm.ensureTab("c", "/c3", "p", LOCAL);
    flushBar();
    stubRects();
    const roots = [...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab")) as HTMLElement[];
    const line = (): HTMLElement | null => document.body.querySelector<HTMLElement>(`.${tg.dropLine}`);
    const shown = (): boolean => line() !== null && !line()!.hidden;

    roots[2].dispatchEvent(
      new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0, bubbles: true }),
    );
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 5, bubbles: true }),
    );
    // 落点 = 第一条（a）之前：线画在 a 的上沿。
    expect(shown()).toBe(true);
    expect(line()!.style.top).toBe("-1px");

    // 拖出右缘 ⇒ armed，那一路根本不重排 ⇒ 不许再指一条不会发生的落点。
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 9999, clientY: 5, bubbles: true }),
    );
    expect(shown(), "撕离那一路不指示落点").toBe(false);

    // 先拖回非 armed 让线重新出来，否则松手时本来就没线，「拖完清干净」那句恒真。
    document.dispatchEvent(
      new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 5, bubbles: true }),
    );
    expect(shown(), "拖回栏内应重新指示").toBe(true);

    document.dispatchEvent(
      new MouseEvent("mouseup", { clientX: 10, clientY: 5, bubbles: true }),
    );
    expect(line(), "拖完必须清干净，线不许挂在那儿").toBeNull();
  });

  it("★ P7a2-Y2：armed（拖出右缘）时**顺序一个字不动**", () => {
    tm.ensureTab("a", "/c", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
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
   * ★ 6d：**拖到一半，tab 不许自己跳位置。**
   *
   * 「拖拽进行中注入一次活动事件 ⇒ 松手之前 `#tab-bar` 子节点顺序一次都不变；
   * 松手后那一次重画必须补上（守卫是推迟不是丢弃）。住 `tests/frontend/ui/tabs.vitest.ts`「6d」。」
   *
   * # 机制（已核实，不是推测）
   *
   * `refreshTabBar` 挂在活动路上：`onFace` / `ensureTab` 末尾都调它。
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
    tm.ensureTab("a", "/c1", "p", LOCAL); // 首个 tab ⇒ 它是 active（`switchTo(_, "auto")`）
    tm.ensureTab("b", "/c2", "p", LOCAL);
    tm.ensureTab("c", "/c3", "p", LOCAL);
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
    tm.ensureTab("d", "/c4", "p", LOCAL);
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
    // 栏上那两个字母照会话事实里的项目名（后端切好的）：这里直接落上。
    const named = (sid: string, dir: string): void => {
      tm.ensureTab(sid, dir, "p", LOCAL);
      home(tm).store.tabs.get(sid)!.projectName = dir.slice(1);
    };
    named("a", "/c1");
    named("b", "/c2");
    named("c", "/c3");
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
    named("d", "/c4");
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

  it("★ P7a3-Y2：成员进它的组，非成员照常直接挂主栏", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    tm.ensureTab("c", "/c3", "p", LOCAL);
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
      tm.ensureTab("a", "/c1", "p", LOCAL);
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
    tm.ensureTab("a", "/c1", "p", LOCAL);
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
    tm.ensureTab("a", "/c1", "p", LOCAL);
    await groupBackend().connect(tm); // 账是空的 ⇒ 集合为空，但「读到过」为真
    flushBar();
    const root = [...(bar.querySelector(".tab-list") ?? bar).children].find((e) => e.classList.contains("tab")) as HTMLElement;
    root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const labels = [...document.querySelectorAll("[role=menu] button")].map(
      (e) => e.querySelector("[data-part=label]")?.textContent ?? "",
    );
    expect(labels.join("|")).toContain(copyText("tabMenu.collection.add"));
  });

  it("右键「新建分组」⇒ 不问名字直接建「分组 N」，这个 tab 就在里面", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    await groupBackend().connect(tm);
    flushBar();
    const root = [...(bar.querySelector(".tab-list") ?? bar).children].find((e) => e.classList.contains("tab")) as HTMLElement;
    root.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const btn = [...document.querySelectorAll("[role=menu] button")].find(
      (e) => e.querySelector("[data-part=label]")?.textContent === copyText("tabMenu.collection.new"),
    ) as HTMLButtonElement | undefined;
    expect(btn, "菜单里要有「新建分组」（否则本判据在空转）").toBeTruthy();
    btn!.click();
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    expect(noAskDialog(), "不问名字").toBe(true);
    const gid = home(tm).store.tabs.get("a")!.group;
    expect(home(tm).prefs.collections.find((c) => c.id === gid)?.name, "建了「分组 N」").toBe(copyText("tabDrop.group.defaultName", { n: 1 }));
  });

  // 成员上界随成员名单一起作废 ⇒ 原「那一组满了 ⇒ 说一句」一格删了（被测的东西不在了）；
  //   换成「加入 / 移出就是改这个 tab 自己的组 id」正反各一格。
  it("〔GRP1〕右键「加入集合 › 某组」⇒ 这个 tab 的 `group` 就是那一组、一句话都不说；「移出」⇒ 回到散 tab", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    const be = groupBackend();
    be.group("g", "白天");
    be.note(LOCAL, "b").groups = ["g"];
    await be.connect(tm);
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
    click("a", "白天");
    expect(showActionFailureToast).not.toHaveBeenCalled();
    expect(membersOf(tm, "g"), "a 该进组、排最后（b 原本就在）").toEqual(["b", "a"]);
    click("a", copyText("tabMenu.collection.remove"));
    expect(membersOf(tm, "g"), "移出只动 a").toEqual(["b"]);
    expect(home(tm).store.tabs.get("a")!.group).toBeNull();
  });

  it("★ 组与散的混排：组在它第一个组员那一格（不再整块排在散的前面）", () => {
    tm.ensureTab("b", "/c2", "p", LOCAL); // 散的，先到
    tm.ensureTab("a", "/c1", "p", LOCAL); // 归组
    tm.ensureTab("c", "/c3", "p", LOCAL); // 散的，后到
    setCols([{ id: "g1", name: "白天", tabs: ["a"] }]);
    flushBar();
    const kids = [...(bar.querySelector(".tab-list") ?? bar).children];
    const kind = (e: Element): string => (e.classList.contains("tab-group") ? "G" : e.classList.contains("tab") ? "t" : "");
    expect(kids.map(kind).join(""), "散的 b · 组（a）· 散的 c").toBe("tGt");
    expect(home(tm).bar.visibleOrder()).toEqual(["b", "a", "c"]);
  });


  it("★ P7a3-Y3：解散集合**一个会话都不许少**", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    setCols([{ id: "g1", name: "白天", tabs: ["a"] }]);
    flushBar();
    const before = [...order()];
    expect(bar.querySelector(".tab-group")).toBeTruthy();

    (bar.querySelector(".tab-group-more") as HTMLButtonElement).click();
    groupMenuItem(copyText("tabMenu.group.dissolve")).click();
    flushBar();
    // 「没删掉会话」是**没有发生的事** ⇒ 钉逐项相等，不是钉「没崩」。
    expect(order(), "集合是个视图，不是容器").toEqual(before);
    expect(bar.querySelector(".tab-group"), "组容器该没了").toBeNull();
    expect([...(bar.querySelector(".tab-list") ?? bar).children].filter((e) => e.classList.contains("tab"))).toHaveLength(2);
    expect(home(tm).store.tabs.get("a")!.group, "解散 ⇒ 组员回到散 tab").toBeNull();
  });

  // 「空」今天只剩一种来路：重启后组员还没到（意图）—— 组表里有它就画。
  it("空集合也留着 —— 刚建的集合不该看不见", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    setCols([{ id: "g1", name: "空的" }]);
    flushBar();
    expect(bar.querySelector(".tab-group")).toBeTruthy();
    expect(bar.querySelectorAll(".tab-group-list > .tab")).toHaveLength(0);
  });
});

// ==========================================================================
// 固定（pinned）—— 「关了 app 再打开它还在」
//
// `§B.3` 的承重：它**必须是正交的一维**，不能做成 `TabStatus` 的第三态 ——
//   因为 `archived + pinned` 才是用户的主用例（固定住一个已经跑完的会话）。
// 固定是会话注解（住会话那一台，`history/groups.rs`）：这一组的后端是替身 `groupBackend`（照那几条命令的形状），
//   「交没交到那台」是**看替身里那条会话的注解**，不是「有没有调过某个函数」。
// ==========================================================================
describe("步 17·B 固定：交那台 · 复活 · 正交", () => {
  let tm: TabManager;
  let be: ReturnType<typeof groupBackend>;
  const tabOf = (sid: string): Tab => home(tm).store.tabs.get(sid)!;

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    vi.mocked(invoke).mockImplementation(withHistoryReads(() => Promise.resolve(undefined)));
    tm = makeTM();
    be = groupBackend();
  });

  it("🔴 量具自检：替身里固定的那条真的被读进来（不过这格，下面全是空转）", async () => {
    be.note(LOCAL, "写进去的").pinned = true;
    await be.connect(tm);
    expect(home(tm).store.tabs.has("写进去的"), "账上的东西没被读进来 ⇒ 下面的复活判据全是空转").toBe(true);
    await be.book.again(); // 幂等：再读一次不该多出东西
    expect(home(tm).store.orderedIds).toEqual(["写进去的"]);
  });

  it("复活：账上一条固定、没有标签 ⇒ 照那台给的那几格造出一个骨架 tab，字段逐个对拍", async () => {
    const m = be.note(LOCAL, "s1");
    m.pinned = true;
    m.facts = { title: "存下来的标题", cwd: "/home/u/proj", record: "/p/s1.jsonl", account: "work", bg: false, at: 1700000000000 };
    await be.connect(tm);
    const t = tabOf("s1");
    // 〔说不清〕没人说过它 ⇒ **说不清**；清单报完了也不替那台下结论：去向只由那台答。
    expect(t.face, "没人说过它 ⇒ 不画点，不许说成已结束").toBeNull();
    tm.markOriginSeen(LOCAL);
    expect(t.face, "清单报完了不替那台下结论").toBeNull();
    tm.onFace("s1", goldenFace("ended"));
    expect(t.face?.dot, "那台答了已结束 ⇒ 已结束").toBe("ended");
    expect(t.pinned, "复活出来的当然是固定的").toBe(true);
    expect(t.title, "🔴 标题照那台给的 —— 不等读文件").toBe("存下来的标题");
    expect(t.parentPath, "记录路径是复活的必需品，没落到 tab 上等于白给").toBe("/p/s1.jsonl");
    expect(t.projectDir).toBe("/home/u/proj");
  });

  it("🔴 账上那条**已经活着**时：只补 `pinned`，`status` 一个字不碰", async () => {
    tm.ensureTab("s1", "/c", "/real.jsonl", LOCAL);
    be.note(LOCAL, "s1").pinned = true;
    await be.connect(tm);
    expect(tabOf("s1").face?.dot, "🔴 把活着的会话按成已结束了").toBe("running");
    expect(tabOf("s1").pinned).toBe(true);
    expect(tabOf("s1").parentPath, "活着那条的真路径不许被账上的覆盖").toBe("/real.jsonl");
  });

  it("固定住会话那一台：固定一个远端的 tab ⇒ 那台那条注解收到 `pinned`（本机那台一条都不收）", async () => {
    await be.connect(tm);
    tm.ensureTab("s1", "/home/u/proj", "/p/s1.jsonl", "devbox");
    tm.togglePin("s1");
    await settleGroups();
    expect(be.note("devbox", "s1").pinned).toBe(true);
    expect(be.calls.filter((c) => c[1] === "history-annotate").map((c) => c[0])).toEqual(["devbox"]);
  });

  it("`togglePin` ⇒ 内存翻转 ＋ **那台的注解真的翻过去**", async () => {
    await be.connect(tm);
    tm.ensureTab("s1", "/home/u/proj", "/p/s1.jsonl", LOCAL);
    tm.togglePin("s1");
    await settleGroups();
    expect(tabOf("s1").pinned).toBe(true);
    expect(be.note(LOCAL, "s1").pinned).toBe(true);
    tm.togglePin("s1");
    await settleGroups();
    expect(tabOf("s1").pinned).toBe(false);
    expect(be.note(LOCAL, "s1").pinned, "取消固定之后那台还留着 ⇒ 下次启动它又回来了").toBe(false);
  });

  it("★ 取消固定：那台回包成了才出「已取消固定」［撤销］；那台拒了 ⇒ 全程没有那一条、行回到固定（A2-17-7-3）", async () => {
    await be.connect(tm);
    tm.ensureTab("s1", "/home/u/proj", "/p/s1.jsonl", LOCAL);
    tm.togglePin("s1");
    await settleGroups();
    const unpinned = copyText("tabBar.unpinned.toast", { title: tabOf("s1").title });
    const said = (): string[] => vi.mocked(undoToast).mock.calls.map((c) => c[0]).filter((t) => t === unpinned);
    vi.mocked(undoToast).mockClear();
    be.refuse.pin = true;
    tm.togglePin("s1");
    expect(said(), "回包之前不出").toEqual([]);
    await settleGroups();
    expect(said()).toEqual([]);
    expect(tabOf("s1").pinned, "拒了 ⇒ 重读那台、回到固定").toBe(true);
    be.refuse.pin = false;
    tm.togglePin("s1");
    expect(said()).toEqual([]);
    await settleGroups();
    expect(said()).toEqual([unpinned]);
  });

  it("🔴 没接组的账的实例（撕离出来的查看窗）：`togglePin` 什么都不做", async () => {
    tm.ensureTab("s1", "/c", "p", LOCAL);
    tm.togglePin("s1");
    await settleGroups();
    expect(tabOf("s1").pinned, "没资格就不许改内存").toBe(false);
    expect(be.calls, "没接账就一条都不交").toEqual([]);
  });

  it("`closeTab` 一个固定的灰 tab ⇒ 那台那条取消固定（点了 × 就不该第二天还在）", async () => {
    await be.connect(tm);
    tm.ensureTab("s1", "/c", "/p/s1.jsonl", LOCAL);
    tm.togglePin("s1");
    await settleGroups();
    expect(be.note(LOCAL, "s1").pinned).toBe(true);
    tm.onFace("s1", goldenFace("ended"));
    vi.useFakeTimers();
    try {
      tm.closeTab("s1");
      await vi.advanceTimersByTimeAsync(10_000);
    } finally {
      vi.useRealTimers();
    }
    await settleGroups();
    expect(be.note(LOCAL, "s1").pinned, "关掉了还固定着 ⇒ 下次开 app 它又回来了").toBe(false);
    expect(home(tm).store.tabs.has("s1"), "取消固定以后账重读一次，关掉的不许又复活").toBe(false);
  });

  it("🔴 正交：`archived` 与 `pinned` **同时成立**（`§B.3` 的主用例）", async () => {
    await be.connect(tm);
    tm.ensureTab("s1", "/c", "p", LOCAL);
    tm.togglePin("s1"); // live 也能 pin（`§B.3b`：效果等它变灰才显现）
    expect(tabOf("s1").face?.dot).toBe("running");
    expect(tabOf("s1").pinned).toBe(true);
    tm.onFace("s1", goldenFace("ended"));
    expect(tabOf("s1").face?.dot, "四种组合的第四格 —— pin 唯一真正生效的那一格").toBe("ended");
    expect(tabOf("s1").pinned, "🔴 归档把 pin 冲掉了 ⇒ 正交性破了，主用例没了").toBe(true);
  });

  it("🔴 `§B.3b`：固定**不影响位置** —— `orderedIds` 一个字不许动", async () => {
    await be.connect(tm);
    for (const s of ["a", "b", "c"]) tm.ensureTab(s, "/c", "p", LOCAL);
    const before = [...home(tm).store.orderedIds];
    tm.togglePin("b"); // 固定中间那个
    expect(home(tm).store.orderedIds, "有人把固定的排到前面去了 —— 那是被 `§B.3b` 删掉的「固定区」").toEqual(before);
    expect(before).toEqual(["a", "b", "c"]);
  });

  it("📌 角标：`.tab.pinned` 跟着 `Tab.pinned` 走，且角标元素真的在（正反两控）", async () => {
    await be.connect(tm);
    tm.ensureTab("s1", "/c", "p", LOCAL);
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
      [...(document.body.querySelector("[role=menu]")?.querySelectorAll("[role^=menuitem]") ?? [])].map(
        (b) => b.querySelector("[data-part=label]")?.textContent ?? "",
      );

    tm.ensureTab("s1", "/c", "p", LOCAL);
    rightClick("s1");
    expect(labels(), "🔴 没读到账就给入口").not.toContain(copyText("tabMenu.pin.pin"));

    await be.connect(tm);
    rightClick("s1");
    expect(labels()).toContain(copyText("tabMenu.pin.pin"));
    (
      [...document.body.querySelectorAll("[role^=menuitem]")].find(
        (b) => b.querySelector("[data-part=label]")?.textContent === copyText("tabMenu.pin.pin"),
      ) as HTMLButtonElement
    ).click();
    expect(tabOf("s1").pinned, "菜单项点了没反应").toBe(true);
    rightClick("s1");
    expect(labels(), "已经固定了还显「固定此标签」⇒ 用户没法取消").toContain(copyText("tabMenu.pin.unpin"));
  });

  it("🔴 `§B.6` 第一格：复活出来的空 tab 里**有话说**，两种情形两种话（正反两控）", async () => {
    be.note(LOCAL, "ok").pinned = true;
    const bad = be.note(LOCAL, "bad");
    bad.pinned = true;
    bad.facts = { ...bad.facts, record: null };
    await be.connect(tm);
    const hintOf = (sid: string): HTMLElement => tabOf(sid).streamEl.querySelector(".pin-revived-hint") as HTMLElement;

    expect(hintOf("ok"), "复活的 tab 里一片空白 —— 那与坏了没有区别").toBeTruthy();
    expect(hintOf("ok").querySelector(".pin-revived-hint-btn")?.textContent).toBe(copyText("history.row.resume"));
    expect(hintOf("bad").querySelector("strong")?.textContent, "记录已不在的要提示「这个会话没有留下记录」").toBe(
      copyText("sessionState.pinnedEmpty.head"),
    );
    expect(hintOf("bad").querySelector(".pin-revived-hint-btn"), "🔴 没有记录还给一个 Resume 按钮 ⇒ 点了必失败").toBeNull();
  });

  it("resume 真接上了（那台发来活着的脸）⇒ 那块空态提示自己走掉", async () => {
    be.note(LOCAL, "s1").pinned = true;
    await be.connect(tm);
    expect(tabOf("s1").streamEl.querySelector(".pin-revived-hint")).toBeTruthy();
    tm.onFace("s1", liveFace("working", true));
    expect(tabOf("s1").streamEl.querySelector(".pin-revived-hint"), "会话已经活了，那块「只能 resume」的话还挂在内容上面").toBeNull();
  });
});

// 拖动合并成组的纯判定（落点 · 停留 · 默认组名 · 归属）住 `tab-group-drop.vitest.ts`；这里只打接到真 DOM 上那一半。
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

  it("🔴 量具自检 ＋ 正题：`measureRows()` 量到的标签页 == 栏里所有 tab（**含组里的**），组头也量", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c1", "p", LOCAL);
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

    const all = home(tm).bar.measureRows();
    expect(all.filter((r) => r.kind === "head").map((r) => r.id), "组头也是一行（落在组头 ＝ 进组）").toEqual(["g1"]);
    const rects = all.filter((r) => r.kind === "tab");
    expect(
      rects.map((r) => r.id).sort(),
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
  const colsOf = (): TabGroup[] => home(tm).prefs.collections;
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    home(tm).prefs.collectionsLoaded = true;
  });

  it("`onto` ⇒ 建组 ＋ 入组 ＋ 组就在目标那一格（目标在前、被拖的在后），名字框立刻打开、给可撤的 toast", async () => {
    tm.ensureTab("x", "/home/u/other", "p", LOCAL);
    tm.ensureTab("b", "/home/u/web", "p", LOCAL);
    tm.ensureTab("a", "/home/u/web", "p", LOCAL);
    // 项目名只来自会话事实（后端切好的）：这里直接落上，界面不从目录里切。
    for (const sid of ["b", "a"]) home(tm).store.tabs.get(sid)!.projectName = "web";
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf().length, "没建组").toBe(1);
    expect(membersOf(tm, colsOf()[0].id)).toEqual(["b", "a"]);
    expect(colsOf()[0].name, "两个项目目录相同 ⇒ 会话事实里的项目名").toBe("web");
    expect(order(), "组在目标那一格，没跳到最上面").toEqual(["x", "b", "a"]);
    expect(home(tm).bar.visibleOrder()).toEqual(["x", "b", "a"]);
    const input = document.body.querySelector<HTMLInputElement>(".tab-group-head input");
    expect(input, "建完组头名字框立刻打开").not.toBeNull();
    expect(document.activeElement, "打字即改").toBe(input);
    await settleGroups();
    expect(vi.mocked(undoToast).mock.calls.map((c) => c[0])).toEqual([copyText("tabBar.group.founded", { name: "web" })]);
    expect(silentUndo).not.toHaveBeenCalled();
  });

  it("撤销建组 ⇒ 组没了、两个回原位（顺序与组员一起退回）", async () => {
    tm.ensureTab("b", "/w/b", "p", LOCAL);
    tm.ensureTab("x", "/w/x", "p", LOCAL);
    tm.ensureTab("a", "/w/a", "p", LOCAL);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    await settleGroups();
    expect(order()).toEqual(["b", "a", "x"]);
    const undo = vi.mocked(undoToast).mock.calls[0][1];
    undo();
    expect(colsOf()).toEqual([]);
    expect(order()).toEqual(["b", "x", "a"]);
    expect([...home(tm).store.tabs.values()].map((t) => t.group)).toEqual([null, null, null]);
  });

  it("只排了顺序 ⇒ 不出 toast，`Ctrl+Z` 那一步照样记上；进组 ⇒ 「已移到「组名」」可撤", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    tm.ensureTab("c", "/c3", "p", LOCAL);
    home(tm).dragger.applyDrop("c", before("a"));
    expect(order()).toEqual(["c", "a", "b"]);
    expect(undoToast).not.toHaveBeenCalled();
    expect(silentUndo).toHaveBeenCalledTimes(1);
    vi.mocked(silentUndo).mock.calls[0][0]();
    expect(order(), "Ctrl+Z 撤回原位").toEqual(["a", "b", "c"]);

    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["a"] }]);
    home(tm).dragger.applyDrop("b", before("a", "g1"));
    expect(membersOf(tm, "g1")).toEqual(["b", "a"]);
    await settleGroups();
    expect(vi.mocked(undoToast).mock.calls.map((c) => c[0])).toEqual([copyText("tabBar.group.joined", { name: "白天" })]);
  });

  it("拖回原位（顺序与组都没变）⇒ 零写、不给撤销", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    home(tm).dragger.applyDrop("a", before("b"));
    expect(order()).toEqual(["a", "b"]);
    expect(undoToast).not.toHaveBeenCalled();
    expect(silentUndo).not.toHaveBeenCalled();
  });

  it("🔴 顺序没变、只有归属变了的那一拍**不许被吞掉**", () => {
    // 这是把 `applyReorder` 改成 `applyDrop` 的正题：旧实现 `if (顺序没变) return`，
    // 而「把组里的 tab 原地拖出来」恰好就是顺序不变、归属变。
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["a"] }]);
    home(tm).dragger.applyDrop("a", before("b")); // a 本来就在 b 前面 ⇒ 顺序不变
    expect(order(), "顺序确实没变（前提成立，这一格才有意义）").toEqual(["a", "b"]);
    expect(membersOf(tm, "g1"), "🔴 归属那一半被「没变化就 return」吞了").toEqual([]);
  });

  it("`end` ⇒ 拖出组并落到末尾", () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["a"] }]);
    home(tm).dragger.applyDrop("a", END);
    expect(order()).toEqual(["b", "a"]);
    expect(membersOf(tm, "g1")).toEqual([]);
    expect(colsOf(), "拖出的是在栏里的最后一个 ⇒ 组消失").toEqual([]);
  });

  it("拖放压在一个散 tab 上建组 ⇒ 建出来、恰是这两个、不出声；本机回了新 id 再把两条写进各自那台", async () => {
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", "devbox");
    const be = groupBackend();
    await be.connect(tm);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf().length).toBe(1);
    expect(membersOf(tm, colsOf()[0].id), "建出来的组里恰是这两个").toEqual(["b", "a"]);
    expect(showActionFailureToast).not.toHaveBeenCalled();
    await settleGroups();
    expect(colsOf().map((c) => c.id), "占位的 id 换成本机回的").toEqual(["g1"]);
    expect(be.note(LOCAL, "a").groups).toEqual(["g1"]);
    expect(be.note("devbox", "b").groups, "组员照它那台写").toEqual(["g1"]);
  });

  it("没 `loadCollections` 过的实例：归属一个字不动（顺序照常）", () => {
    // 与右键菜单那道门同一条理由：没读过盘就写，等于把用户已有的集合清空。
    home(tm).prefs.collectionsLoaded = false;
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    expect(colsOf(), "没读过盘还敢建组 ⇒ 下一次落盘会把用户的集合全冲掉").toEqual([]);
    expect(home(tm).store.tabs.get("a")!.group, "组 id 也一个字不动").toBeNull();
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
describe("步 17·D ⑤ 中间一半停 400ms 才成组（假手势打真事件链）", () => {
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
    tm.ensureTab("a", "/home/u/proj/x", "p", LOCAL);
    tm.ensureTab("b", "/home/u/proj/y", "p", LOCAL);
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

  const lineShown = (): boolean => {
    const l = document.body.querySelector<HTMLElement>(`.${tg.dropLine}`);
    return l !== null && !l.hidden;
  };

  it("🔴 不到 400ms 是插入，满了才翻成 `onto`（时间这一维真的在承重）", () => {
    move(40); // 落在 b 的中间一半、偏上 ⇒ 先当插入（b 之前）
    expect(lineShown(), "落点指示没出来 ⇒ 这一格在空转").toBe(true);
    expect(rootOf("b").classList.contains("drop-onto"), "还没压够就成组 = `§D.4` 要避的误触").toBe(
      false,
    );

    vi.advanceTimersByTime(DWELL_MS - 1);
    expect(rootOf("b").classList.contains("drop-onto"), "差 1ms 就翻 ⇒ 门槛是假的").toBe(false);

    vi.advanceTimersByTime(1);
    expect(
      rootOf("b").classList.contains("drop-onto"),
      "🔴 压满 400ms 还不成组 —— 指针停住之后没有 `mousemove` 了，没有计时器这件事就永远不发生",
    ).toBe(true);
    expect(lineShown(), "插入线与目标框不许同时挂着").toBe(false);
    expect(rootOf("b").textContent, "右侧小标签写松手会发生什么").toContain(copyText("tabDrop.pill.found"));
  });

  it("攒满之后抖一下（>4px）⇒ 退回插入（可逆）", () => {
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
    expect(membersOf(tm, cols[0].id), "组在 b 那一格：b 在前、被拖的 a 在后").toEqual(["b", "a"]);
    expect(cols[0].name, "两个 cwd 不同 ⇒ 分组 N（不取共同前缀）").toBe(copyText("tabDrop.group.defaultName", { n: 1 }));
  });

  it("🔴 松手早于 400ms ⇒ 计时器必须**已经被清掉**（数在飞的定时器，不是只看后果）", () => {
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
  it("🔴 停在一行的上 / 下 1/4 永远只插入：不攒停留、停多久都不建组", () => {
    const base = vi.getTimerCount();
    move(30); // b 的上 1/4
    expect(vi.getTimerCount(), "上 1/4 不排停留计时器").toBe(base);
    vi.advanceTimersByTime(DWELL_MS * 3);
    expect(rootOf("b").classList.contains("drop-onto")).toBe(false);
    expect(lineShown()).toBe(true);
  });

  it("拖的时候按 Esc ＝ 取消：什么都不变、线与影子都收掉", () => {
    dispatcher.applyOverrides({});
    dispatcher.start();
    move(40);
    vi.advanceTimersByTime(DWELL_MS);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    expect(document.body.querySelector(".tab-drag-ghost"), "影子收掉").toBeNull();
    expect(rootOf("b").classList.contains("drop-onto")).toBe(false);
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    expect(home(tm).prefs.collections, "取消之后松手不建组").toEqual([]);
    expect(home(tm).store.orderedIds).toEqual(["a", "b"]);
  });

  it("往上 / 下 / 左拖出栏 ＝ 取消：影子写「松开取消」，松手什么都不变", () => {
    const bar = document.body.firstElementChild as HTMLElement;
    bar.getBoundingClientRect = (): DOMRect => ({ left: 0, right: 260, top: 0, bottom: 600, width: 260, height: 600 }) as DOMRect;
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true })); // 收掉 beforeEach 起的那一次（它量的是没钉过的栏）
    rootOf("b").dispatchEvent(new MouseEvent("mousedown", { button: 0, buttons: 1, clientX: 10, clientY: 40, bubbles: true }));
    document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 2, bubbles: true }));
    expect(lineShown(), "栏内照常指示").toBe(true);
    document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: 900, bubbles: true }));
    expect(document.body.querySelector(".tab-drag-ghost")!.textContent).toBe(copyText("tabDrop.ghost.cancel"));
    expect(lineShown(), "出栏不画插入线").toBe(false);
    document.dispatchEvent(new MouseEvent("mouseup", { clientX: 10, clientY: 900, bubbles: true }));
    expect(home(tm).store.orderedIds, "松手什么都不变").toEqual(["a", "b"]);
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
//   `src/frontend/ui/main.ts` 里那一行是 `loadCollections().finally(() => loadOrder())` ——
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
  /** 交出去的补丁（替身只记、不应用：落盘语义归 Rust 写口）。 */
  let sent: Edit[];
  /** 落盘是 fire-and-forget（`先改内存再落盘`）⇒ 看补丁前把那条链子跑完。 */
  const flushDisk = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
  /** 最后一次交给 `tabBar.order` 的那一份。 */
  const orderSent = (): unknown => sent.filter((e) => e.op === "set" && e.path.join(".") === "tabBar.order").map((e) => (e as { value: unknown }).value).at(-1);

  /** 往「盘」上摆一份顺序（模拟上次关 app 前落下的那份）。 */
  const putOrderOnDisk = (order: string[]): void => {
    disk = { tabBar: { order } };
  };

  /**
   * **真实启动时序**（逐行对着 `src/frontend/ui/main.ts` 那三句摆）：
   *   `new TabManager` → `loadCollections()` → `loadOrder()` → 会话**随后**陆续到。
   * @param arriving 会话到达的次序（＝后端清单/事件流给的次序，通常不等于用户拖出来的次序）
   */
  const startupThenSessionsArrive = async (arriving: string[]): Promise<void> => {
    await tm.loadCollections();
    await tm.loadOrder();
    for (const sid of arriving) {
      // 每个 sid 一个自己的 cwd（原先是为了避开 bg 树状锚定；树删了之后留着无害）。
      tm.ensureTab(sid, `/proj/${sid}`, `/p/${sid}.jsonl`, LOCAL, false);
    }
  };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    sent = [];
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      if (cmd === "patch_config") {
        // 写只交补丁；替身只记下来。
        sent.push(...(args as { edits: Edit[] }).edits);
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
    tm.ensureTab("a", "/proj/a", "/p/a.jsonl", LOCAL, false);
    tm.ensureTab("b", "/proj/b", "/p/b.jsonl", LOCAL, false);
    tm.ensureTab("c", "/proj/c", "/p/c.jsonl", LOCAL, false);
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
    home(tm).dragger.applyDrop("a", before("c"));
    expect(home(tm).store.orderedIds, "拖动本身没生效，这一格后面就没意义了").toEqual(["a", "c", "b"]);
    await flushDisk();
    expect(orderSent(), "拖完要落盘（`§C.3`：applyReorder 之后提交）").toEqual(["a", "c", "b"]);

    // 又来一条新会话。
    tm.ensureTab("d", "/proj/d", "/p/d.jsonl", LOCAL, false);
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

  /** 第 s 行是哪一类记录（行与索引两边同一个算法）。 */
  const kindAt = (s: number): "said" | "reply" => (s % 3 === 0 ? "said" : "reply");
  const mk = (sid: string, seq: number, uuid: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      record: { t: kindAt(seq), id: uuid } as never,
    }) as never;
  /** 索引：第 i 行的类是第 `i + shift` 行的（shift ≠ 0 模拟「seq 空间对不上」） */
  const idx = (n: number, shift = 0) => ({
    available: true,
    from: 0,
    end: n * 10,
    rows: Array.from({ length: n }, (_, i) => ({
      o: i * 10,
      n: 10,
      t: kindAt(i + shift),
      dr: "card",
      ch: 100,
      pl: 1,
    })),
  });
  // 骨架索引改走通道：一发 `chan_call` 译回「哪一问 ＋ 旧形参」（`chan-fake.ts::sessionReadCalls`）。
  const indexCalls = () =>
    sessionReadCalls(vi.mocked(invoke).mock.calls, "read_session_index").map((a) => ["read_session_index", a]);
  const settle = () => new Promise((r) => setTimeout(r, 0));

  /** 重放形状：尾巴先到（钉 floor=200），老的 [0,200) 后到 ⇒ 全收纳。`skip` 里的 seq 不发（模拟迟到）。 */
  function replay(sid: string, skip: Set<number> = new Set(), make: typeof mk = mk): Tab {
    tm.onLine(make(sid, 200, "u200")); // 首个 tab ⇒ active，非批期直渲钉 floor
    // jsdom 没布局 ⇒ `contentReachesBottom` 退回算术判据；给它一个「已满屏」的几何，
    // 否则批结束会把 [0,200) 全物化掉（那是没有骨架时的正确行为，不是本组要测的）
    const el = home(tm).store.tabs.get(sid)!.streamEl;
    Object.defineProperty(el, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(el, "clientHeight", { value: 800, configurable: true });
    tm.onBatchStart();
    for (let s = 201; s < 300; s++) tm.onLine(make(sid, s, `u${s}`));
    for (let s = 0; s < 200; s++) if (!skip.has(s)) tm.onLine(make(sid, s, `u${s}`));
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
  // 「停住」就是 `TabStreamView.STAY_MS`（150ms）这段时长 ⇒ 假时钟拨到界上两侧，不真等（数照写字面量，常量被改了要红）。
  it("★ 索引回来时 tab 已经切走 ⇒ 不在后台接；切回来停住了再接，不重问", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
        Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
      ) as never));
      const t = replay("pk"); // pk 是当前的，批结束要了索引（还没回来）
      tm.onLine(mk("pkOther", 5, "po5"));
      tm.switchTo("pkOther"); // 索引回来之前切走
      await vi.advanceTimersByTimeAsync(0);
      expect(t.skeleton, "切走了还在后台接骨架 ⇒ 旧切换的活还在干").toBeNull();
      tm.switchTo("pk");
      expect(t.skeleton, "同步段里不接（接要量几何）").toBeNull();
      await vi.advanceTimersByTimeAsync(149);
      expect(t.skeleton, "还没停够就接了").toBeNull();
      await vi.advanceTimersByTimeAsync(1);
      expect(t.skeleton, "切回来停住了该接上").not.toBeNull();
      expect(t.skeleton!.pendingRows).toBe(200);
      expect(indexCalls().filter(([, a]) => (a as { jsonlPath: string }).jsonlPath === "/p/pk.jsonl").length, "停着的那一份直接用，不重问").toBe(1);
    } finally {
      vi.useRealTimers();
    }
  });

  // 按住「下一个 tab」连切：每个 tab 只在眼前几十毫秒。要骨架索引（后端整份读那个会话的记录文件）、接骨架、刷大纲
  // 都是给「停下来看」的人准备的 —— 路过的 tab 一概不发、不接；在眼前停住了才做。
  it("★ 路过的 tab（切进来又马上切走）不要骨架索引；停住了才要", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
        Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
      ) as never));
      tm.onLine(mk("dwA", 1, "dwa1")); // 首个 tab ⇒ 当前
      tm.onLine(mk("dwB", 200, "u200")); // 后台 tab：直渲、钉 floor，还没要过索引
      const ofB = () => indexCalls().filter(([, a]) => (a as { jsonlPath: string }).jsonlPath === "/p/dwB.jsonl").length;
      tm.switchTo("dwB");
      tm.switchTo("dwA"); // 路过
      await vi.advanceTimersByTimeAsync(150);
      expect(ofB(), "路过的 tab 也去要索引 ⇒ 按住切一圈就是一圈整份读").toBe(0);
      tm.switchTo("dwB");
      expect(ofB(), "同步段里不要").toBe(0);
      await vi.advanceTimersByTimeAsync(149);
      expect(ofB(), "还没停够就要了").toBe(0);
      await vi.advanceTimersByTimeAsync(1);
      expect(ofB(), "停住了该要").toBe(1);
    } finally {
      vi.useRealTimers();
    }
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

  // 冷切停住那一帧：占位先按「没折」的高插进去、DOM 变动回调排版之后账本才拿到折叠 ⇒ 占位改高、按视口钉，同一帧多排两次版。
  // 接之前先把这个 tab 的折叠交给账本（`TurnFold.seedFolds`）⇒ 占位插进去就是折后的高，补可见区也不会去建折着的过程行。
  it("★ 接骨架：账本先拿到折叠、再插占位（不是插完再改高）", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
    const order: string[] = [];
    const folds = vi.spyOn(SkeletonLedger.prototype, "setFolds").mockImplementation(function (this: SkeletonLedger) {
      order.push("折叠");
      return false;
    });
    const attach = vi.spyOn(SkeletonView.prototype, "attach").mockImplementation(function (this: SkeletonView) {
      order.push("插占位");
      return 0;
    });
    onTestFinished(() => {
      folds.mockRestore();
      attach.mockRestore();
    });
    const t = replay("sf");
    await settle();
    expect(t.skeleton, "前置：骨架要接上").not.toBeNull();
    expect(order[0], `接骨架的顺序是 ${order.join(" → ")}：占位插进去的时候账本还没拿到折叠`).toBe("折叠");
    expect(order).toContain("插占位");
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

  it("🔴 seq 空间对不上（索引里同一个 seq 那一行是另一类记录）⇒ 不接 —— 不许硬对", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300, 7) : undefined),
    ) as never));
    const t = replay("shift");
    await settle();
    expect(t.skeleton).toBeNull();
    expect(t.window.pendingCount).toBe(200);
  });

  // 一个 tab 都没开（启动时 · 全关掉以后）：快捷键 · 会话头开关 · 重放批结束 · 探针都可能照来，一样都不碰谁、不问后端。
  it("没有当前 tab ⇒ Ctrl+F · Alt+↑ · 显示系统注入 · 批结束 · 探针快照什么都不做", () => {
    const view = home(tm).view;
    tm.openFind();
    tm.stepTurn(1);
    view.toggleInjected();
    view.toggleInjected();
    tm.onBatchStart();
    tm.onBatchEnd();
    expect(vi.mocked(invoke).mock.calls.filter((c) => c[0] === "chan_call"), "没有 tab 也去问后端").toEqual([]);
    expect(tm.debugSnapshot()).toBe(JSON.stringify({ active: null }));
    expect(view.trailerOf("nope")).toBeNull();
  });

  // 缺得多、正文整份重来（`onSessionWatch` → `catchUpTo` → `restartTail`）：已经拿过的骨架账本接着续 ——
  // 只要新尾段之上它没覆盖到的那一截（从上次索引的末字节要），不从 0 再要；续不上 ⇒ 这一份不用了，下一次触发点从 0 要。
  describe("★ 正文重来，骨架账本接着续", () => {
    const fromOffsets = (sid: string) =>
      indexCalls()
        .filter(([, a]) => (a as { jsonlPath: string }).jsonlPath === `/p/${sid}.jsonl`)
        .map(([, a]) => (a as { fromOffset: number }).fromOffset);
    /** 索引从 `fromOffset` 字节往后 `n` 行（第 i 行占 [10i, 10i + 10)，uuid 与 `mk` 同一套）。 */
    const idxFrom = (fromOffset: number, n: number) => ({
      available: true,
      from: fromOffset,
      end: fromOffset + n * 10,
      // 第 i 行的类与 `mk` 那一行同一个算法（重来之后取回的那一截收在账本里，接骨架前要对拍 seq 空间）
      rows: Array.from({ length: n }, (_, k) => ({ o: fromOffset + k * 10, n: 10, t: kindAt(fromOffset / 10 + k), dr: "card", u: `u${fromOffset / 10 + k}`, ch: 100, pl: 1 })),
    });
    /**
     * 一个 tab 先接上 `[0, first)` 的骨架，再从 `watchAt` 起上流（缺得多 ⇒ 正文重来）；按行号取一页答到 `end`。
     * `more`：续传那一问的答法（拿到这一刻新尾段的 floor）。
     */
    async function restarted(sid: string, first: number, watchAt: number, end: number, more: (floor: number, fromOffset: number) => unknown) {
      vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string, args: Record<string, unknown>) => {
        if (cmd === "read_session_index") {
          const from = args.fromOffset as number;
          return Promise.resolve(from === 0 ? idx(first) : more(home(tm).store.tabs.get(sid)!.window.floorSeq!, from));
        }
        if (cmd === "read_session_lines") {
          const from = args.from as number;
          const until = (args.until as number | undefined) ?? end;
          return Promise.resolve({ from, next: until, eof: args.until === undefined, payloads: Array.from({ length: until - from }, (_, k) => mk(sid, from + k, `u${from + k}`)) });
        }
        return Promise.resolve(undefined);
      }) as never));
      const t0 = replay(sid);
      await settle();
      expect(t0.skeleton, "前置：骨架要接上").not.toBeNull();
      tm.onSessionWatch(LOCAL, { sid, path: `/p/${sid}.jsonl`, seq: watchAt });
      const t = home(tm).store.tabs.get(sid)!;
      expect(t, "缺得多却没重来正文").not.toBe(t0);
      // 新的一代同样给一个「已满屏」的几何（同 `replay`），不然 jsdom 里补批会一路按行号取到第 0 行
      Object.defineProperty(t.streamEl, "scrollHeight", { value: 2000, configurable: true });
      Object.defineProperty(t.streamEl, "clientHeight", { value: 800, configurable: true });
      return t;
    }

    it("续得上 ⇒ 从上次索引的末字节只要到新尾段之上，接上", async () => {
      const t = await restarted("cs", 300, 2000, 2500, (floor, from) => idxFrom(from, floor - from / 10));
      await vi.waitFor(() => expect(fromOffsets("cs").length, "重来之后没再要骨架").toBe(2), UNTIL_CEILING);
      expect(fromOffsets("cs"), "重来之后骨架又从 0 整份要").toEqual([0, 3000]);
      await vi.waitFor(() => expect(t.skeleton, "续上了却没接").not.toBeNull(), UNTIL_CEILING);
      expect(t.skeleton!.pendingRows).toBe(t.window.floorSeq);
    });

    it("账本已经盖过新尾段 ⇒ 一问都不发，直接接上", async () => {
      const t = await restarted("cw0", 1000, 1000, 1000, () => {
        throw new Error("不该续传");
      });
      await vi.waitFor(() => expect(t.skeleton, "账本够用却没接").not.toBeNull(), UNTIL_CEILING);
      expect(fromOffsets("cw0")).toEqual([0]);
      expect(t.skeleton!.pendingRows).toBe(t.window.floorSeq);
    });

    it("续不上 ⇒ 这一份不用了：下一次触发点从 0 要", async () => {
      const t = await restarted("cf", 300, 2000, 2500, (_f, from) => ({ available: false, reason: "抖了一下", failure: "transport", from, end: from, rows: [] }));
      await vi.waitFor(() => expect(t.skeletonFetch).toBe("again"), UNTIL_CEILING);
      expect(t.skeleton).toBeNull();
      tm.onBatchStart();
      tm.onBatchEnd();
      await settle();
      // 从 0 要回来的那一份盖不到新尾段 ⇒ 从它的末字节续；还是续不上 ⇒ 不接（盖不到尾段的账本接上去，占位里那一截没有行），重问过一次 ⇒ 定死
      expect(fromOffsets("cf")).toEqual([0, 3000, 0, 3000]);
      expect(t.skeleton, "续不上也接了：占位里尾段之上那一截账本里没有").toBeNull();
      expect(t.skeletonFetch).toBe("done");
    });

    it("续传回来时已经切走 ⇒ 停着；切回来停住了再接，不重问", async () => {
      let answer!: () => void;
      const t = await restarted("cp", 300, 2000, 2500, (floor, from) => new Promise((r) => (answer = () => r(idxFrom(from, floor - from / 10)))));
      await vi.waitFor(() => expect(fromOffsets("cp")).toEqual([0, 3000]), UNTIL_CEILING);
      tm.onLine(mk("cpOther", 5, "cpo5"));
      tm.switchTo("cpOther");
      answer();
      await settle();
      expect(t.skeleton, "切走了还在后台接骨架").toBeNull();
      tm.switchTo("cp");
      await vi.waitFor(() => expect(t.skeleton, "切回来停住了该接上").not.toBeNull(), UNTIL_CEILING);
      // 接上之后从账本的末字节往后续一截（`extendIndex`：尾段之后来的行进账本，收起时交得回去）不算重问
      expect(fromOffsets("cp").filter((o) => o <= 3000), "停着的那一份直接用，不重问").toEqual([0, 3000]);
    });
  });

  it("门控：占位里的行收纳；岛（ensure 物化过的一段）里迟到的行就地建卡，不收纳", async () => {
    // 按偏移取 60 那一问一直在路上（取回中的行不算还在占位里；取不到才放回占位）
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      cmd === "read_session_range" ? new Promise(() => {}) : Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
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
    // 🔴 取回的是**历史**：按重放语义建卡 —— sink 不接 onRealUserInput（历史 user 卡不许自动切 tab）
    for (const c of spy.mock.calls.filter((c) => (c[0] as { seq: number }).seq >= 100 && (c[0] as { seq: number }).seq <= 102)) {
      expect((c[2] as { onRealUserInput?: unknown }).onRealUserInput).toBeUndefined();
    }
    // 150 不在这一段里 ⇒ 没被要
    expect(ranges.some((c) => c.seqBase === 150)).toBe(false);
  });

  // 系统注入那一行（101）不建卡、建一条细条：按偏移取回的那一段要与直接建出来的那一截逐行一样（开着「显示系统注入」时露的就是它）。
  it("★ 按偏移取回的一段与直接建出来的一截逐行一样：夹在中间的系统注入也取回、照建", async () => {
    const INJ = 101;
    const make = ((sid: string, seq: number, uuid: string) =>
      seq === INJ
        ? { session_id: sid, cwd: "/p", path: `/p/${sid}.jsonl`, seq, record: { t: "said", id: uuid, who: { speaker: { kind: "system", body: "注入" } }, draw: "strip", human: false } }
        : mk(sid, seq, uuid)) as typeof mk;
    const index = idx(300);
    index.rows[INJ] = { ...index.rows[INJ], t: "said", dr: "strip" } as never;
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string, args?: unknown) => {
      if (cmd === "read_session_index") return Promise.resolve(index);
      if (cmd === "read_session_range") {
        const a = args as { jsonlPath: string; seqBase: number; offset: number; until: number };
        const sid = a.jsonlPath.replace(/^.*\//, "").replace(/\.jsonl$/, "");
        return Promise.resolve(Array.from({ length: (a.until - a.offset) / 10 }, (_, k) => make(sid, a.seqBase + k, `u${a.seqBase + k}`)));
      }
      return Promise.resolve(undefined);
    }) as never));
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    const built = async (sid: string, skip: Set<number>): Promise<unknown[]> => {
      tm = makeTM();
      const t = replay(sid, skip, make);
      await settle();
      expect(t.skeleton, "前置：骨架要接上").not.toBeNull();
      spy.mockClear();
      t.skeleton!.ensure(INJ, 3); // [98, 105)
      await settle();
      return spy.mock.calls
        .map((c) => c[0] as { seq: number; record: unknown })
        .sort((x, y) => x.seq - y.seq)
        .map((p) => [p.seq, p.record]);
    };
    const direct = await built("injDirect", new Set());
    const fetched = await built("injFetched", new Set([100, 101, 102]));
    expect(direct.map((r) => (r as [number])[0]), "正控：直接建出来的那一截带注入那一行").toContain(INJ);
    expect(fetched).toEqual(direct);
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
    // ③ 滚到被丢掉的那段：按偏移要回来、只建卡
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    t.skeleton!.ensure(300, 2); // [298, 303)
    await settle();
    expect(recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_range")).toEqual([
      { origin: "<local>", jsonlPath: "/p/big.jsonl", offset: 2980, until: 3030, seqBase: 298 },
    ]);
    expect(spy.mock.calls.map((c) => (c[0] as { seq: number }).seq)).toEqual([298, 299, 300, 301, 302]);
  });

  it("大纲跳转：点到还在占位里的一条 ⇒ 先按后端给的行号物化那一段再跳", async () => {
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads((cmd: string) =>
      Promise.resolve(cmd === "read_session_index" ? idx(300) : undefined),
    ) as never));
    const t = replay("jump");
    await settle();
    expect(t.skeleton!.isPending(42)).toBe(true);
    const host = (t.inputsPanel as unknown as { host: { jumpTo(to: { uuid: string; line: number }): unknown } }).host;
    // jsdom 没有 `CSS.escape`（`revealCard` 要它）；本格判的是「跳之前先物化」，找不找得到卡不在本格
    const g = globalThis as unknown as { CSS?: { escape(s: string): string } };
    g.CSS ??= { escape: (x: string) => x };
    host.jumpTo({ uuid: "u42", line: 42 });
    expect(t.skeleton!.isPending(42)).toBe(false);
  });

  // 数据源换成后端之后，这条路必须照旧通：行是后端清单给的（不是流上攒的），
  //   点**那一行**（不是直接调宿主）⇒ 先按那一行带的行号物化再跳。
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
                  { at: 70, line: 7, uuid: "u7", excerpt: "七", timestamp: "" },
                  { at: 420, line: 42, uuid: "u42", excerpt: "四十二", timestamp: "" },
                  { at: 2500, line: 250, uuid: "u250", excerpt: "二百五十", timestamp: "" },
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
    vi.mocked(invoke).mockImplementation(withHistoryReads(withSessionReads(launchRenderShim((cmd: string) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      // 写只交补丁（`patch_config`）：这一组不看，照默认回空。
      if (cmd === "probe_session_record")
        return probe ? Promise.resolve(probe) : Promise.reject(new Error("没有控制通道"));
      return Promise.resolve(undefined);
    }))));
    tm = makeTM();
  });

  it("★ G3 行为不回退：活着、只是不在 tmux 里的会话 ≠ 已结束（× 不露、↗ 还在）", () => {
    tm.ensureTab("c2", "/x", "p", "pi");
    tm.onFace("c2", liveFace("working", false));
    expect(btn().classList.contains("ended")).toBe(false);
    expect(btn().hasAttribute("data-reconnectable")).toBe(false);
  });

  it("★ 固定复活 ⇒ 不画点、字里零处「已结束」（界面不替那台下结论）；清单报完了照旧；那台答了已结束 ⇒ 已结束；宣告了 ⇒ 活", async () => {
    const be = groupBackend();
    be.note("pi", "p1").pinned = true;
    be.note(LOCAL, "p2").pinned = true;
    be.note(LOCAL, "p3").pinned = true;
    await be.connect(tm);
    for (const sid of ["p1", "p2", "p3"]) expect(tabOf(sid).face, "复活出来还没人说过它 ⇒ 不画点").toBeNull();
    // T4：还没人说过它们那一刻，tab 的提示句 · 状态名 · 固定空态，一处都不许说「已结束」。
    const said = (): string =>
      ["p1", "p2", "p3"].map((sid) => cardText(tm, sid)).join("\n") +
      [...document.querySelectorAll<HTMLElement>(".pin-revived-hint")].map((n) => n.textContent).join("\n");
    expect(said()).not.toContain(copyText("sessionState.ended.name"));
    // 清单报完不下结论；p3 被宣告 ⇒ 活。
    tm.markOriginSeen(LOCAL);
    tm.markOriginSeen("pi");
    tm.onFace("p3", liveFace("working", true));
    expect([tabOf("p1").face?.dot, tabOf("p2").face?.dot, tabOf("p3").face?.dot]).toEqual([undefined, undefined, "running"]);
    expect(said()).not.toContain(copyText("sessionState.ended.name"));
    // 那台答了 p1 已结束（会话流 `ended` 格）。正控：这时才出现「已结束」。
    tm.onFace("p1", goldenFace("ended"));
    expect(tabOf("p1").face?.dot).toBe("ended");
    expect(said()).toContain(copyText("sessionState.ended.name"));
  });

  it("★ 说不清：那台先报完、固定后复活 ⇒ 照旧说不清（不替那台下结论）；那台先答了已结束 ⇒ 复活出来就是已结束；远端又宣告它 ⇒ 活", async () => {
    tm.markOriginSeen("pi");
    tm.onFace("q2", goldenFace("ended")); // 那台的答早于复活到（tab 还没建，暂存）
    const be = groupBackend();
    be.note("pi", "q1").pinned = true;
    be.note("pi", "q2").pinned = true;
    await be.connect(tm);
    expect(tabOf("q1").face, "复活出来还没人说过它 ⇒ 不画点（不替那台下结论）").toBeNull();
    expect(tabOf("q2").face?.dot).toBe("ended");
    tm.onFace("q1", liveFace("working")); // 远端又宣告它：那一帧带着脸
    tm.createSkeletonTab("q1", "/x", "pi");
    expect(tabOf("q1").face?.dot).toBe("running");
  });

  it("★ 恢复只交那台；记录在不在由那台的账本重发一帧脸，界面不自己落", async () => {
    tm.ensureTab("g1", "/home/u/p", "/p/g1.jsonl", LOCAL);
    tm.onFace("g1", goldenFace("ended"));
    await home(tm).actions.resume("g1");
    expect(vi.mocked(openSession).mock.calls[0], "恢复不再带回调").toHaveLength(2);
    expect(tabOf("g1").face?.dot).toBe("ended");
  });
});

/**
 * **远端断连 ⇒ 说不清，不是已结束**（「`Unseen` 不许被显示成已结束」）。
 * 本机后端断线那一刻给那台活的 · 可重连的逐条发 `left`（说不清那一张脸）→ `TabManager.onFace`，TabManager 真走。
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
    tm.onFace("u2", goldenFace("reconnectable")); // 可重连
    tm.ensureTab("o1", "/x", "p", "mu"); // 别的机器上的活会话
    for (const sid of ["u1", "u2"]) tm.onFace(sid, goldenFace("unseen")); // 本机后端逐条说
    expect([tabOf("u1").face?.dot, tabOf("u2").face?.dot]).toEqual(["unknown", "unknown"]);
    expect(tabOf("o1").face?.dot, "别的机器不受牵连").toBe("running");
    // 两颗的状态句都是那台的离线那一句（机器成品 `offline_said` 同一处取字）、零处「已结束」（改之前断连那一刻这里是两句「这个会话已结束」）。
    expect(titles().split(goldenFace("unseen").line).length - 1).toBe(2);
    expect(titles()).not.toContain(copyText("sessionState.ended.name"));
    // 死透了的本机后端不说：已结束 / 记录没了照旧（正控：这时才出现「已结束」）。
    tm.ensureTab("u3", "/x", "p", "pi");
    tm.onFace("u3", goldenFace("ended"));
    tm.ensureTab("u4", "/x", "p", "pi");
    tm.onFace("u4", goldenFace("ended"));
    tm.onFace("u4", goldenFace("gone"));
    expect([tabOf("u3").face?.dot, tabOf("u4").face?.dot]).toEqual(["ended", "gone"]);
    expect(titles()).toContain(copyText("sessionState.ended.name"));
  });

  it("★ 重连之后：重宣告的翻回活；那台报完清单、没有它的照旧说不清，等那台答（它在手上的 tab 里，随名单报上去）", () => {
    tm.ensureTab("r1", "/x", "p", "pi");
    tm.ensureTab("r2", "/x", "p", "pi");
    for (const sid of ["r1", "r2"]) tm.onFace(sid, goldenFace("unseen"));
    expect([tabOf("r1").face?.dot, tabOf("r2").face?.dot]).toEqual(["unknown", "unknown"]);
    tm.onFace("r1", liveFace("working")); // 重连后后端初扫重宣告 r1：那一帧带着脸
    tm.createSkeletonTab("r1", "/x", "pi");
    tm.markOriginSeen("pi"); // `sessions_replayed` ⇒ 报完了，没有 r2
    expect([tabOf("r1").face?.dot, tabOf("r2").face?.dot]).toEqual(["running", "unknown"]);
    tm.onFace("r2", goldenFace("ended")); // 那台答的已结束
    expect(tabOf("r2").face?.dot).toBe("ended");
  });

  it("★ 手上的 tab 逐台报给壳（那台不在说的它答已结束）：多一个 · 少一个才报，整份换", () => {
    const kept = (): unknown[] =>
      vi.mocked(invoke).mock.calls.filter(([c]) => c === "kept_sessions").map(([, a]) => (a as { sessions: unknown }).sessions);
    const before = kept().length;
    tm.ensureTab("k1", "/x", "p", "pi");
    tm.ensureTab("k2", "/x", "p", LOCAL);
    expect(kept().at(-1)).toEqual([
      { origin: LOCAL, sessionId: "k2" },
      { origin: "pi", sessionId: "k1" },
    ]);
    const n = kept().length;
    expect(n - before).toBe(2);
    tm.onFace("k1", goldenFace("ended"));
    expect(kept().length, "状态变了不算多一个少一个").toBe(n);
    tm.closeTab("k1");
    expect(kept().at(-1)).toEqual([{ origin: LOCAL, sessionId: "k2" }]);
  });

  // 〔GP1 问 3〕要求：「可重连 → 断连 → 重连后，tmux 里还在的那几条重新宣告为可重连（不是落已结束）」·
  // 转移表（说不清 + idle ⇒ 可重连；已结束 + idle 不动 ⇒ 次序承重）。
  it("〔TL2 · GP1 问 3〕可重连 → 断连（说不清）→ 重连：先重宣告 idle、再报完清单 ⇒ 可重连；tmux 不在的那条 ⇒ 已结束", () => {
    tm.ensureTab("k1", "/x", "p", "pi");
    tm.ensureTab("k2", "/x", "p", "pi");
    tm.onFace("k1", goldenFace("reconnectable"));
    tm.onFace("k2", goldenFace("reconnectable"));
    for (const sid of ["k1", "k2"]) tm.onFace(sid, goldenFace("unseen"));
    expect([tabOf("k1").face?.dot, tabOf("k2").face?.dot]).toEqual(["unknown", "unknown"]);
    // emitter 那一笔：k1 的 tmux 还在 ⇒ idle 格；k2 不在 ⇒ ended 格；**然后**才是 listed 格。
    tm.onFace("k1", goldenFace("reconnectable"));
    tm.onFace("k2", goldenFace("ended"));
    tm.markOriginSeen("pi");
    expect([tabOf("k1").face?.dot, tabOf("k2").face?.dot]).toEqual(["exited", "ended"]);
    const k1Title = home(tm).bar.tabButtons.get("k1")!.root.title;
    expect(k1Title, "重连后 tmux 还在的那条被说成已结束了").not.toContain(copyText("sessionState.ended.name"));
  });
});

// **接线判据**：`main.ts` 起步那几行（标本机清单报完 · 两个新事件交给 TabManager）没有 DOM 判据够得着（整个 `main.ts` 是入口脚本）⇒ 读源码数调用点，两向恰好一处。
describe("〔U4b〕main.ts 接线", () => {
  it("★ 脸 · 清单报完（本机远端同一格）· 各台都报完那一拍各恰一处；本机不再另拉清单", () => {
    const main = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/main.ts"), "utf8");
    const n = (needle: string): number => main.split(needle).length - 1;
    expect([
      n("      tabs.onFace(sessionId, face);"),
      n("    onOriginSessionsListed: (origin) => tabs.markOriginSeen(origin),"), // 那一格只说哪台报完了
      n("      if (f.allListed) tabs.markAllListed();"), // 本机后端那一句「各台都报完」只记下、不在这里判：别的台可能还没在自己那条流上报完
      n("  tabs.onAllListed = () => startup?.onAllListed();"), // 那一刻由 TabManager 认（每台都在自己那条流上报完了）：交「启动时记住的那一格」
      n("startup?.onAllListed()"), // 不许别处绕过那一刻直接判
    ]).toEqual([1, 1, 1, 1, 1]);
  });
  // 机器离线条照各台的状态成品（离线 · 几条状态不明都是本机后端说的）主窗接一处；独立查看窗不建标签页，看不见走它那一条订阅（`followSession` 的 `sight`）。
  it("★〔GP1〕机器状态接到栏：main.ts 恰一处 · entry-viewer.ts 零处（不建标签页）", () => {
    const n = (file: string, needle: string): number =>
      readFileSync(resolve(REPO_ROOT, file), "utf8").split(needle).length - 1;
    expect([
      n("src/frontend/ui/main.ts", "tabs.machineChanged(origin, m);"),
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
  /** 帧换成手摇的（`fillAbove` 补完一批靠 rAF 自链复检）：用例收场时没摇的帧丢掉，不漏进下一条的计数。 */
  let frames: FrameRequestCallback[] = [];
  beforeEach(() => {
    vi.clearAllMocks();
    frames = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    tm = makeTM();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.mocked(invoke).mockResolvedValue(undefined as never);
  });
  /** 摇一帧：此刻排着的那几个回调跑一遍。 */
  const oneFrame = (): void => {
    const due = frames;
    frames = [];
    for (const cb of due) cb(0);
  };

  const mk = (sid: string, seq: number, origin?: string) =>
    ({
      session_id: sid,
      cwd: "/p",
      path: `/p/${sid}.jsonl`,
      seq,
      ...(origin ? { origin } : {}),
      record: { t: "reply", id: `${sid}-${seq}` } as never,
    }) as never;
  /** 一直摇到停：在途的读落下来（让几拍）、排着的帧跑完，连着几拍都没有新排的帧为止。 */
  const settle = async (): Promise<void> => {
    for (let quiet = 0; quiet < 3; ) {
      await new Promise((r) => setTimeout(r, 0));
      if (frames.length === 0) quiet++;
      else {
        quiet = 0;
        oneFrame();
      }
    }
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
    oneFrame(); // 切进来那一脚补批在下一帧（同步段不读几何）
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

  it("主人拆掉以后，排着的补批复检不再往下问", async () => {
    vi.mocked(invoke).mockImplementation(answerAll("dz"));
    tm.onLine(mk("head", 1)); // 首个 tab ⇒ active
    tm.onLine(mk("dz", 300)); // 后台 tab：账本空、想往下问
    tm.switchTo("dz");
    oneFrame(); // 切进来那一脚：问 [100, 300)
    await new Promise((r) => setTimeout(r, 0)); // 那一问落下来：补上屏、排下一帧复检
    expect(frames.length, "前提：落下来之后排了下一帧复检").toBeGreaterThan(0);
    tm.dispose();
    vi.mocked(invoke).mockClear();
    await settle();
    expect(asks(), "拆掉之后又问了").toEqual([]);
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
    oneFrame(); // 切进来那一脚补批在下一帧（同步段不读几何）
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

  it("★ L4：取回的历史行 · 实时行都不改那台的脸（活不活只由那台的帧说）", async () => {
    vi.mocked(invoke).mockImplementation(answerAll("rm", "box"));
    tm.onLine(mk("head", 1));
    tm.onLine(mk("rm", 80, "box"));
    const t = home(tm).store.tabs.get("rm")!;
    tm.onFace("rm", goldenFace("ended"));
    expect(t.face?.dot).toBe("ended");
    tm.switchTo("rm"); // 往下取 [0, 80)：80 条远端历史行
    await settle();
    expect(asks().length).toBeGreaterThan(0);
    expect(t.face?.dot, "取回来的历史行把已结束的远端 tab 翻活了").toBe("ended");
    tm.onLine(mk("rm", 81, "box")); // 实时行
    expect(t.face?.dot, "实时行也不翻活").toBe("ended");
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
    const mk = (seq: number) => ({ session_id: "s", cwd: null, path: "/p/s.jsonl", seq, record: {} }) as never;
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
// 守的要求：「后台（bg）会话不再自动挂到宿主下排成树：
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
  const colsOf = (): TabGroup[] => home(tm).prefs.collections;
  /** `host` 与 `sub` 同 cwd；`other` 另一个项目。三颗按 `arriving` 的次序宣告。 */
  const arrive = (arriving: string[]): void => {
    for (const sid of arriving) {
      if (sid === "host") tm.createSkeletonTab("host", "/proj/a", LOCAL, false);
      if (sid === "sub") tm.createSkeletonTab("sub", "/proj/a", LOCAL, subKind);
      if (sid === "other") tm.createSkeletonTab("other", "/proj/z", LOCAL, false);
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
    expect(colsOf().map((c) => membersOf(tm, c.id)), "组在目标那一格：目标在前、被拖的在后").toEqual([["other", "host"]]);
    expect(order()).toEqual(["sub", "other", "host"]);
  });

  it("④ host 与 sub 同组，把 host 拖出组 ⇒ sub 留在组里", () => {
    arrive(["host", "sub", "other"]);
    setGroups(tm, [{ id: "g1", name: "白天", tabs: ["host", "sub"] }]);
    home(tm).dragger.applyDrop("host", END);
    expect(colsOf().map((c) => [c.id, c.name])).toEqual([["g1", "白天"]]);
    expect(membersOf(tm, "g1")).toEqual(["sub"]);
    expect(order()).toEqual(["sub", "other", "host"]);
  });

  it("⑤ 同 sid 后到一份 interactive 宣告（升格）⇒ 位置不动", () => {
    arrive(["host", "sub", "other"]);
    tm.createSkeletonTab("sub", "/proj/a", LOCAL, false);
    expect(home(tm).store.tabs.get("sub")!.background).toBe(false);
    expect(order()).toEqual(["host", "sub", "other"]);
  });
});

// ════════════════════════════════════════════════════════════════════════════
// 会话事实：**后端给了什么 ⇒ tab 上是什么**
// ════════════════════════════════════════════════════════════════════════════
//
// 分叉血缘 · 改动文件集 · 最新 usage 由后端出成品（帧命令 `history-facts`，`session-reads.ts` 第五问）。
// 口径（首条锁定 · 近因序 · 上界 · 取文件序最后一条）全在后端判据（`tests/backend/observe/recognize/facts_tests.rs`）；
// 这一组只钉前端那一侧：成品原样落到 tab 上、每问只交路径、只刷变了的那几块、要不到就出声。
// 子 agent 的列表与状态不在会话事实里（运行表，`tests/frontend/ui/runs.vitest.ts`）。
// 夹具只造结构（sid / 路径 / 占位 id），不采会话正文。
describe("〔STC〕会话事实：后端给了什么 ⇒ tab 上是什么", () => {
  const facts = (p: Partial<SessionFacts> = {}): SessionFacts => ({
    agent: "claude",
    startedBy: null,
    children: [],
    startedByText: null,
    childrenText: null,
    touchedFiles: [],
    usage: null,
    projectDir: null,
    writers: [],
    pending: [],
    lastSay: null,
    needs: null,
    retries: [],
    notices: [],
    permissionMode: null,
    permissionModeText: null,
    modelText: null,
    modelPick: null,
    tokens: null,
    background: null,
    mcp: [],
    autoAnswer: null,
    autoAnswerChoices: [],
    askEnd: null,
    ...p,
  });
  const line = (sid: string, seq: number, origin: string | null = null) =>
    ({
      session_id: sid,
      cwd: "/home/u/proj",
      path: `/home/u/proj/${sid}.jsonl`,
      seq,
      origin,
      record: { t: "said", id: `${sid}-${seq}` },
    }) as never;
  /** 等在途的那几趟通道往返落完（每趟是若干个微任务 ＋ 一次宏任务，多等几轮）。 */
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
  };
  /** 后台 tab 的第一趟会话事实排在当前那一趟之后、空闲时才要（`tab-cold-reads.ts`）：等到 `sid` 那一问真发出去、答落下来。 */
  const idleTurn = async (sid: string): Promise<void> => {
    await vi.waitFor(() => expect(asked.some((a) => a.path.endsWith(`/${sid}.jsonl`)), `后台 ${sid} 那一趟没轮到`).toBe(true), UNTIL_CEILING);
    await settle();
  };
  /** 记下每一问的 `(origin, path, body)`；答法由 `answer` 现算（抛 ⇒ 通道那一跳的失败形状原样抛）。 */
  let asked: { origin: string; path: string; body: unknown }[] = [];
  const answerFacts = (answer: (path: string) => unknown): void => {
    asked = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (!isChanCall(cmd, args, "history-facts")) return undefined;
      const body = chanArgsJson(args) as { path: string };
      asked.push({ origin: args.origin, path: body.path, body });
      return chanReply(answer(body.path));
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

  it("#63① 成品带分叉来的那一句 ⇒ 悬停卡照摆那一句（字是那台照父会话起名写好的，界面不拼编号；标题里的 ↳ 是核心写进整行的，界面不加）", async () => {
    const said = "那台写的分叉自那一句";
    answerFacts(() => facts({ forkedLine: said }));
    tm.onLine(line("fork-sid", 0));
    await settle();
    const tab = home(tm).store.tabs.get("fork-sid")!;
    expect(tab.forkedLine).toBe(said);
    expect(tab.title.includes("↳"), "界面不往标题上加记号").toBe(false);
    expect(cardText(tm, "fork-sid").split("\n")).toContain(said);
    expect(cardText(tm, "fork-sid"), "不再自己拼「分叉自 编号前 8 位」").not.toContain("abcd1234");
  });

  it("★ 监控板那一格目录照会话事实的 `cwdShown`（那台写好的 `~/…`），不显原样 `projectDir`；事实没到 ⇒ null", async () => {
    answerFacts(() => facts({ projectDir: "/home/u/proj", cwdShown: "~/proj" }));
    tm.onLine(line("gd", 0));
    tm.onFace("gd", liveFace("working"));
    const dir = () => tm.snapshotSessions().find((x) => x.sessionId === "gd")?.cwdShown;
    expect(dir(), "事实没到").toBeNull();
    await settle();
    expect(dir()).toBe("~/proj");
  });

  it("★ 成品说是哪一家 ⇒ tab 记下它（之前不知道 ⇒ `null`，不落 claude）", async () => {
    (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = true;
    try {
      answerFacts(() => facts({ agent: "codex" }));
      tm.onLine(line("cx-sid", 0));
      const tab = home(tm).store.tabs.get("cx-sid")!;
      expect(tab.agent, "事实到之前不知道是哪一家").toBeNull();
      await settle();
      expect(tab.agent).toBe("codex");
    } finally {
      (globalThis as { __keepAgentUnknown?: boolean }).__keepAgentUnknown = false;
    }
  });

  it("成品带「机器 · 目录」那一句 ⇒ 悬停卡照摆那一行，不另摆机器名与目录两行；不带 ⇒ 不说", async () => {
    // 夹具那一句与目录、机器名都故意不同：界面若还在自己摆 machineName · projectDir，就红。
    const said = "那台写的在哪一句";
    let whereLine: string | undefined = said;
    answerFacts(() => facts({ projectDir: "/w/where-proj", ...(whereLine === undefined ? {} : { whereLine }) }));
    tm.onLine(line("wh", 0));
    await settle();
    const tab = home(tm).store.tabs.get("wh")!;
    const tip = () => cardText(tm, "wh").split("\n");
    expect(tab.whereLine).toBe(said);
    expect(tip()).toContain(said);
    expect(tip()).not.toContain("/w/where-proj");
    whereLine = undefined;
    tm.onLine(line("wh", 1));
    await settle();
    expect(tab.whereLine).toBeNull();
    expect(tip()).not.toContain(said);
  });

  it("悬停卡不写「写入进程 ×n」（开发者向、点一下就是同一件事），也不写「切到此处 · n」", async () => {
    const said = "那台写的写入进程句";
    answerFacts(() => facts({ writers: [4242, 4243], writersLine: said }));
    tm.onLine(line("dup", 0));
    await settle();
    expect(home(tm).store.tabs.get("dup"), "量具自检：那条会话起来了").toBeDefined();
    const tip = cardText(tm, "dup");
    expect(tip).not.toContain(said);
  });

  // 刚起的会话宣告时还没有记录（或老后端宣告不带）⇒ 后端读到记录开头之后经会话事实给项目目录，标题跟上；行里的 cwd 不读。
  it("成品带项目目录 ⇒ tab 记下；没带（null）⇒ 不动 tab 上那一份", async () => {
    answerFacts(() => facts({ projectDir: "/a/proj" }));
    tm.onLine({ ...(line("pd", 0) as object), cwd: "/a/proj/sub" } as never);
    await settle();
    const tab = home(tm).store.tabs.get("pd")!;
    expect(tab.projectDir).toBe("/a/proj");
    answerFacts(() => facts());
    tm.onLine({ ...(line("pd", 1) as object), cwd: "/a/proj/sub" } as never);
    await settle();
    expect(tab.projectDir).toBe("/a/proj");
  });

  it("#63① 成品说不是分叉 ⇒ 不加徽标（区分性）", async () => {
    answerFacts(() => facts());
    tm.onLine(line("plain", 0));
    await settle();
    const tab = home(tm).store.tabs.get("plain")!;
    expect(tab.forkedLine).toBeNull();
    expect(tab.title.startsWith("↳")).toBe(false);
  });

  it("#63① 分叉 ＋ 后到的名字 ⇒ 标题就是核心那一行（恰一个 ↳，界面不叠加）", async () => {
    answerFacts(() => facts({ forkedLine: "分叉自「parent-x」" }));
    tm.createSkeletonTab("f3", "/home/u/proj", LOCAL); // 项目目录由会话宣告给
    tm.onLine(line("f3", 0));
    await settle();
    const tab = home(tm).store.tabs.get("f3")!;
    tm.updateName("f3", "我的功能", "proj ↳ 我的功能");
    expect(tab.title).toBe("proj ↳ 我的功能");
    expect((tab.title.match(/↳/g) ?? []).length).toBe(1);
  });

  it("又长了再问：每一问只交路径；成品整份替换（后端说什么就是什么）", async () => {
    let n = 0;
    const first = facts({ touchedFiles: ["/a"] });
    const second = facts({ touchedFiles: ["/b", "/a"] });
    answerFacts(() => (n++ === 0 ? first : second));
    tm.onLine(line("s", 0));
    await settle();
    tm.onLine(line("s", 1));
    await settle();
    expect(asked.map((a) => Object.keys(a.body as object))).toEqual([["path"], ["path"]]);
    const tab = home(tm).store.tabs.get("s")!;
    expect([...tab.touchedFiles]).toEqual(["/b", "/a"]);
  });

  it("批期不问；批结束当前 tab 当场问，后台 tab 空闲时轮到再问（不只 active，一个一个）", async () => {
    answerFacts((path) => facts(path.includes("b1") ? { forkedLine: "分叉自「src」" } : {}));
    tm.onBatchStart();
    tm.onLine(line("b1", 0));
    tm.onLine(line("b1", 1));
    tm.onLine(line("b2", 0));
    await settle();
    expect(asked).toEqual([]);
    tm.onBatchEnd();
    await settle();
    expect(asked.map((a) => a.path), "批结束当场只问当前那一个").toEqual(["/home/u/proj/b1.jsonl"]);
    await idleTurn("b2");
    expect(asked.map((a) => a.path).sort()).toEqual(["/home/u/proj/b1.jsonl", "/home/u/proj/b2.jsonl"]);
    expect(home(tm).store.tabs.get("b1")!.forkedLine).toBe("分叉自「src」");
  });

  it("改动文件集：成品原样透传给监控板 peek（近因序由后端排）；远端问的是那台", async () => {
    answerFacts(() => facts({ touchedFiles: ["/proj/b.rs", "/proj/a.ts"] }));
    tm.onLine(line("s-local", 0));
    tm.onLine(line("s-remote", 0, "devbox"));
    await settle();
    await idleTurn("s-remote");
    expect(tm.peekSession("s-local")!.recentFiles).toEqual(["/proj/b.rs", "/proj/a.ts"]); // F91b：尾 = 最近改
    expect(asked.find((a) => a.path.includes("s-remote"))?.origin).toBe("devbox"); // 远端问的是那台
  });

  it("F88b usage：active 的成品一到就推给 HUD；后台 tab 不推；切过去时推那一格", async () => {
    const seen: [string | null, string | null][] = [];
    tm.active.subscribe((a) => seen.push([a.usage?.model ?? null, a.usage?.promptTokensText ?? null])); // 订阅 store（原先是回调）
    answerFacts((path) =>
      facts({ usage: path.includes("u1") ? usageOf(42, "m-a") : usageOf(7, null) }),
    );
    tm.onLine(line("u1", 0)); // 第一个 tab 自动成为 active
    tm.onLine(line("u2", 0));
    await settle();
    // 切换那一格的推送在 rAF 里（`switchTo`），与事实到达谁先谁后不定 ⇒ 等「最后一次」落定，不钉次序。
    await vi.waitFor(() => expect(seen.at(-1)).toEqual(["m-a", "核心·42"]), UNTIL_CEILING);
    expect(seen.some(([, t]) => t === "核心·7")).toBe(false); // 后台 tab 的事实到了不推
    tm.switchTo("u2");
    await vi.waitFor(() => expect(seen.at(-1)).toEqual([null, "核心·7"]), UNTIL_CEILING);
    expect(home(tm).store.tabs.get("u2")!.usage?.promptTokensText).toBe("核心·7"); // 监控板那一格读的也是它（`snapshotSessions`）
    expect(tm.peekSession("u1")!.model).toBe("m-a");
  });

  // 「当前 tab 变了」改订阅 store 之后的时机差：同值不通知（原先两个回调同值也照调）。
  it("★ 〔GAP1〕当前 tab 那一格同值不通知：可用性重报一次同样的值 ⇒ 零通知；切到别的 tab ⇒ 恰一次", async () => {
    answerFacts((path) => facts({ usage: path.includes("v1") ? usageOf(5, "m") : null }));
    tm.onLine(line("v1", 0));
    tm.onLine(line("v2", 0));
    await settle();
    await vi.waitFor(() => expect(tm.active.get().usage?.promptTokensText).toBe("核心·5"), UNTIL_CEILING);
    const seen: unknown[] = [];
    tm.active.subscribe((a) => seen.push(a));
    (tm as unknown as { onFactsAvailability(sid: string): void }).onFactsAvailability("v1");
    expect(seen, "值没变却通知了").toEqual([]);
    tm.switchTo("v2");
    await vi.waitFor(() => expect(seen.length).toBe(1), UNTIL_CEILING);
    expect(seen).toEqual([{ sid: "v2", title: home(tm).store.tabs.get("v2")!.title, usage: null, unavailable: null, projectDir: null, modelText: null, account: null }]);
  });

  // 状态栏账号按钮读当前 tab 那一格：那台脸上说的号到了 / 变了 ⇒ 当场跟上；别的 tab 的脸不惊动它。
  it("当前 tab 那一格的号照脸上说的：到了 ⇒ 跟上；后台 tab 的脸不通知", async () => {
    tm.onLine(line("w1", 0));
    tm.onLine(line("w2", 0));
    await settle();
    await vi.waitFor(() => expect(tm.active.get().sid).toBe("w1"), UNTIL_CEILING);
    const seen: Array<string | null> = [];
    tm.active.subscribe((a) => seen.push(a.account));
    tm.onFace("w2", { ...liveFace("working"), account: "c" });
    expect(seen, "后台 tab 的脸惊动了当前那一格").toEqual([]);
    tm.onFace("w1", { ...liveFace("working"), account: "b" });
    expect(seen).toEqual(["b"]);
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
    await vi.waitFor(() => expect(said.at(-1)).toBeNull(), UNTIL_CEILING); // 切换那一格的推送在 rAF 里
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
    record: { t: "reply", id: `rf-${seq}` } as never,
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
    const before = home(tm).store.tabs.get("rf-sid")!.face;
    // 那一句是后端写好的（格里带来的），这里照摆、不照取值自己映。
    rf.onSessionFileNotice("rf-sid", "gone", "甲句");
    expect(noticeText(tm)).toBe("甲句");
    expect(home(tm).store.tabs.get("rf-sid")!.face, "记录文件不见了就改了会话状态（误判结束）").toEqual(before);
    line(1);
    expect(noticeText(tm), "又来了一行，「不见了」那句还挂着").toBeNull();
    rf.onSessionFileNotice("rf-sid", "rewritten", "乙句");
    line(2);
    expect(noticeText(tm), "重读那句被下一行收掉了").toBe("乙句");
    rf.onSessionFileNotice("rf-sid", "truncated", "丙句");
    expect(noticeText(tm), "新的一句没盖掉旧的").toBe("丙句");
    // 别的取值：那一句照样是后端写好的 ⇒ 照摆（界面不另抄一份取值表来挡）；只有 `gone` 那一句下一行收。没有这个 tab ⇒ 不画、不抛。
    rf.onSessionFileNotice("rf-sid", "moved", "丁句");
    expect(noticeText(tm)).toBe("丁句");
    line(3);
    expect(noticeText(tm), "不是「不见了」那句，下一行不该收").toBe("丁句");
    expect(() => rf.onSessionFileNotice("no-such-sid", "gone", "甲句")).not.toThrow();
  });
});

// ==========================================================================
// 组是后端的数据（A1-5 · A1-8 · A95-9）：组跟着组员走（每台只存自己有组员的那一片）、组员与固定是会话注解（住会话那台）；会话自己结束后仍算组员，人点 × 就出组。
// K1 这台 config.json 不存组 · K1b 组员在两台 · K2 点 × 出组（会话自己结束不算）· K3 谁都不剩栏里那一组才没 · K4 重启照账带组。真 TabManager ＋ 组的后端替身。
// ==========================================================================
describe("组是后端的数据：栏照账画 · 写到会话那台 · 点 × 出组", () => {
  let tm: TabManager;
  let be: ReturnType<typeof groupBackend>;
  let disk: Record<string, unknown>;
  /** 交给这台 `config.json` 的补丁。 */
  let sent: Edit[];
  const colsOf = (): TabGroup[] => home(tm).prefs.collections;
  const add = (...sids: string[]): void => {
    for (const sid of sids) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL);
  };
  const close = async (sid: string): Promise<void> => {
    tm.onFace(sid, goldenFace("ended")); // × 只关已结束的
    vi.useFakeTimers();
    try {
      tm.closeTab(sid);
      await vi.advanceTimersByTimeAsync(10_000);
    } finally {
      vi.useRealTimers();
    }
    await settleGroups();
  };
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    disk = {};
    sent = [];
    vi.mocked(invoke).mockImplementation(withHistoryReads((cmd: string, args?: unknown) => {
      if (cmd === "load_config") return Promise.resolve(JSON.parse(JSON.stringify(disk)));
      // 写只交补丁；替身只记下来（落盘语义归 Rust 写口）。
      if (cmd === "patch_config") sent.push(...(args as { edits: Edit[] }).edits);
      return Promise.resolve(undefined);
    }));
    tm = makeTM();
    be = groupBackend();
  });

  it("K1 · 走遍写组的入口：这台 `config.json` 一个组的字都不写（段里只有顺序 · 收着的组），每一下都落在组员那台 · 发给各片", async () => {
    await be.connect(tm);
    await tm.loadCollections();
    add("a", "b", "c", "d");
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" }); // 拖放现建
    await settleGroups();
    const g = home(tm).store.tabs.get("a")!.group!;
    expect(g, "本机回了新 id").toBe("g1");
    home(tm).dragger.applyDrop("c", before("a", g)); // 拖放进组
    await settleGroups();
    expect(be.note(LOCAL, "c").groups).toEqual(["g1"]);
    home(tm).dragger.applyDrop("c", END); // 拖出
    await settleGroups();
    expect(be.note(LOCAL, "c").groups).toEqual([]);
    void home(tm).prefs.joinGroup("d", g);
    void home(tm).prefs.renameGroup(g, "改");
    void home(tm).prefs.setCollapsed([g], true);
    await settleGroups();
    expect(be.note(LOCAL, "d").groups).toEqual(["g1"]);
    expect(be.calls).toContainEqual([LOCAL, "groups-write", { op: "rename", id: "g1", name: "改", machines: [LOCAL] }]);
    expect([...new Set(sent.map((e) => e.path.join(".")))].sort(), "只写 tab 栏那一段里这扇窗的看法（顺序 · 收着的组）").toEqual(["tabBar.collapsed", "tabBar.order"]);
    void home(tm).prefs.dissolveGroup(g);
    await settleGroups();
    expect(be.calls).toContainEqual([LOCAL, "groups-write", { op: "dissolve", id: "g1", machines: [LOCAL] }]);
    expect(["a", "b", "d"].map((s) => be.note(LOCAL, s).groups), "解散 ⇒ 各组员那台摘掉那一组").toEqual([[], [], []]);
  });

  it("K1b · 组员在两台：进组带着那一组的记录问组员那台；改名 · 解散交本机、带上两台", async () => {
    await be.connect(tm);
    await tm.loadCollections();
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", "devbox");
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    await settleGroups();
    const rec = { id: "g1", name: expect.any(String), made: 1, named: 1 };
    expect(be.calls).toContainEqual(["devbox", "groups-write", { op: "move", sid: "b", join: rec }]);
    expect(be.calls).toContainEqual([LOCAL, "groups-write", { op: "move", sid: "a", join: rec }]);
    const sentTo = (op: string): string[] | undefined => {
      const c = be.calls.find((x) => x[0] === LOCAL && (x[2] as { op?: string }).op === op);
      return (c?.[2] as { machines?: string[] } | undefined)?.machines?.slice().sort();
    };
    void home(tm).prefs.renameGroup("g1", "两台");
    await settleGroups();
    expect(be.calls).toContainEqual([LOCAL, "groups-write", expect.objectContaining({ op: "rename", id: "g1", name: "两台" })]);
    expect(sentTo("rename"), "发给组员在的两台").toEqual([LOCAL, "devbox"].sort());
    void home(tm).prefs.dissolveGroup("g1");
    await settleGroups();
    expect(sentTo("dissolve"), "解散也发给两台").toEqual([LOCAL, "devbox"].sort());
    expect(be.calls.some((c) => c[0] === "devbox" && c[1] === "groups-write" && (c[2] as { op: string }).op !== "move"), "改名 · 解散不直接问别的台").toBe(false);
  });

  it("★ 进组 · 出组 · 拖放 · 解散：那台回包成了才出［撤销］；那台拒了 ⇒ 全程没有那一条、只有没成那一句（A2-17-7-3）", async () => {
    await be.connect(tm);
    await tm.loadCollections();
    for (const s of ["a", "b", "c"]) tm.ensureTab(s, `/w/${s}`, "p", LOCAL);
    home(tm).dragger.applyDrop("a", { kind: "onto", sid: "b" });
    await settleGroups();
    const g = home(tm).store.tabs.get("a")!.group!;
    const name = home(tm).prefs.collections.find((c) => c.id === g)!.name;
    const offered = (): string[] => vi.mocked(undoToast).mock.calls.map((c) => c[0]);
    const failedSaid = (): number => vi.mocked(failToast).mock.calls.filter((c) => c[0] === copyText("tabBar.persist.collectionsFailed")).length;
    const closeMenus = (): void => document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
    const steps: Array<[string, () => void, string]> = [
      ["拖放进组", () => home(tm).dragger.applyDrop("c", before("a", g)), copyText("tabBar.group.joined", { name })],
      ["右键进组", () => home(tm).dragger.joinNow(["c"], g), copyText("tabBar.group.joined", { name })],
      ["出组", () => home(tm).dragger.leaveNow(["a"]), copyText("tabBar.group.left", { name })],
      [
        "解散",
        () => {
          (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
          (document.body.querySelector(".tab-group-more") as HTMLButtonElement).click();
          groupMenuItem(copyText("tabMenu.group.dissolve")).click();
          closeMenus();
        },
        copyText("tabBar.group.dissolved", { name }),
      ],
    ];
    for (const [what, run, title] of steps) {
      vi.mocked(undoToast).mockClear();
      vi.mocked(failToast).mockClear();
      be.refuse.group = true;
      run();
      expect(offered(), `${what}：回包之前不出［撤销］`).toEqual([]);
      await settleGroups();
      expect(offered(), `${what}：那台拒了 ⇒ 没有［撤销］那一条`).toEqual([]);
      expect(failedSaid(), `${what}：只有没成那一句`).toBeGreaterThan(0);
      be.refuse.group = false;
      run();
      expect(offered(), `${what}：回包之前不出［撤销］`).toEqual([]);
      await settleGroups();
      expect(offered(), `${what}：那台收下了 ⇒ 出［撤销］`).toEqual([title]);
      vi.mocked(undoToast).mock.calls[0][1]();
      await settleGroups();
    }
  });

  // 守原话「分组不应该单独存会话记录. x就是没了, 不存在还要移出分组」：点 × ＝ 出组（连固定一起取消）。
  //   「会话结束后仍算组员」说的是会话自己结束（进程退出），不是人点 ×：那一形是 K4（没来的 b 照账还在组里）。
  it("K2 · 点 × 关掉组里的 tab ＝ 出组（那台那条摘掉那一组）；组里谁都不剩 ⇒ 栏里那一组没了、不另发解散；会话只是结束了 ⇒ 照旧是组员", async () => {
    be.group("g", "白天");
    be.note(LOCAL, "a").groups = ["g"];
    be.note(LOCAL, "b").groups = ["g"];
    add("a", "b");
    await be.connect(tm);
    expect(membersOf(tm, "g")).toEqual(["a", "b"]);
    tm.onFace("b", goldenFace("ended")); // b 自己结束了：不是人点 ×
    await settleGroups();
    expect(be.note(LOCAL, "b").groups, "会话结束 ≠ 出组").toEqual(["g"]);
    await close("a");
    expect(be.note(LOCAL, "a").groups, "点 × ⇒ 出组").toEqual([]);
    expect(colsOf().map((c) => c.id), "还有 b ⇒ 组在").toEqual(["g"]);
    await close("b");
    expect(be.note(LOCAL, "b").groups).toEqual([]);
    expect(colsOf(), "最后一个点 × ⇒ 组也没").toEqual([]);
    expect(be.calls.some((c) => (c[2] as { op?: string }).op === "dissolve"), "最后一位出组那一片自己删，不另发解散").toBe(false);
  });

  it("K3 · 拖出 / 右键移出：组里谁都不剩（账上的 ＋ 栏里的）⇒ 栏里那一组没了；还有不在栏里的组员 ⇒ 组在", async () => {
    be.group("g", "白天");
    be.group("h", "夜里");
    be.note(LOCAL, "a").groups = ["g"];
    be.note(LOCAL, "b").groups = ["g"];
    be.note(LOCAL, "c").groups = ["h"];
    be.note("devbox", "x").groups = ["h"]; // 不在栏里的组员
    add("a", "b", "c");
    await be.connect(tm);
    home(tm).dragger.applyDrop("a", END); // 拖出，b 还在
    await settleGroups();
    expect(colsOf().map((c) => c.id), "还有 b ⇒ g 在").toEqual(["g", "h"]);
    void home(tm).prefs.leaveGroup("b"); // 右键移出最后一个
    await settleGroups();
    expect(colsOf().map((c) => c.id), "g 谁都不剩 ⇒ 没了").toEqual(["h"]);
    void home(tm).prefs.leaveGroup("c");
    await settleGroups();
    expect(colsOf().map((c) => c.id), "x 还在 h 里 ⇒ h 在").toEqual(["h"]);
    expect(be.calls.some((c) => (c[2] as { op?: string }).op === "dissolve"), "出组从不另发解散").toBe(false);
  });

  it("K4 · 重启后：在栏里的 tab 照账带组；固定了却没标签的复活出来、也带组；没来的不凭空造", async () => {
    be.group("G", "甲");
    be.group("H", "乙");
    be.note(LOCAL, "a").groups = ["G"];
    be.note(LOCAL, "b").groups = ["G"];
    be.note(LOCAL, "c").groups = ["H"];
    const p = be.note(LOCAL, "p");
    p.groups = ["H"];
    p.pinned = true;
    await be.connect(tm);
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
    ({ session_id: "rr-sid", cwd: "/p", path: "/p/rr-sid.jsonl", seq, record: { t: "reply", id: uuid } }) as never;
  const rendered = async (): Promise<string[]> => {
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    return (renderContentRecord as unknown as ReturnType<typeof vi.fn>).mock.calls.map(
      (c) => (c[0] as { record: { id: string } }).record.id,
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
            record: { t: "reply", id: `sq-${s}` },
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
  it("两条长正文合起来超预算 ⇒ 第一截只有最高那条、第二截是另一条连同其余短的 ⇒ 建卡顺序 == 手算的 [149, 129‥148, 109‥128, …]", async () => {
    const { renderContentRecord } = await import("../../../src/frontend/ui/render-stream-record");
    const spy = renderContentRecord as unknown as ReturnType<typeof vi.fn>;
    const tm = makeTM();
    tm.onLine({ session_id: "bgA", cwd: "/p", path: "/p/bgA.jsonl", seq: 0, record: { t: "reply", id: "a0" } } as never);
    tm.onBatchStart();
    const big = 40 * 1024; // 两条合起来超过 64 Ki，各自一条不超
    for (let s = 0; s < 150; s++) {
      const text = s >= 148 ? "x".repeat(big) : "y";
      tm.onLine({
        session_id: "bgB",
        cwd: "/p",
        path: "/p/bgB.jsonl",
        seq: s,
        record: { t: "reply", id: `b${s}`, blocks: [{ type: "text", text }] },
      } as never);
    }
    tm.onBatchEnd();
    spy.mockClear();
    tm.switchTo("bgB");
    const order = spy.mock.calls.map((c) => (c[0] as { seq: number }).seq);
    // 冷开一截 20 条、一次同步调用至多 8 截（jsdom 恒不满一屏 ⇒ 8 截建满）：第一截被预算截成只有 149，之后每截 20 条、截内升序
    const chunk = (hi: number): number[] => Array.from({ length: 20 }, (_, i) => hi - 19 + i);
    expect(order).toEqual([149, ...[148, 128, 108, 88, 68, 48, 28].flatMap(chunk)]);
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
    for (const sid of ["a", "b", "c", "d", "e"]) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL);
    // a、e 在组里 ⇒ 组员聚在第一个组员那一格：条上看得到的顺序是 a e（组）· b c d（散），不是到达的顺序。
    setGroups(tm, [{ id: "g", name: "组", tabs: ["a", "e"] }]);
    tm.switchTo("a");
    home(tm).bar.refresh();
  });

  it("S1 · Ctrl 加减（第一次把当前 tab 带上、不切 tab）· Shift 按条上的顺序连选 · 单击 / 空白 / Esc 清掉", () => {
    expect(home(tm).bar.visibleOrder(), "正控：组员聚在第一个组员那一格").toEqual(["a", "e", "b", "c", "d"]);
    click("b", { ctrlKey: true });
    expect(drawnSelected()).toEqual(["a", "b"]);
    expect(home(tm).store.activeId, "Ctrl 单击不切 tab").toBe("a");
    click("a", { ctrlKey: true });
    expect(drawnSelected()).toEqual(["b"]);
    click("a");
    expect(drawnSelected(), "单击清掉多选").toEqual([]);
    click("c", { shiftKey: true });
    // 锚点 a（上一次单击）到 c：条上是 a e b c —— 按到达顺序会是 a b c。
    expect(drawnSelected()).toEqual(["a", "b", "c", "e"]);
    click("e", { shiftKey: true });
    expect(drawnSelected(), "锚点不动，换一头（条上 a · e 相邻）").toEqual(["a", "e"]);
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
    tm.onFace("e", goldenFace("ended"));
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
    tm.onFace("b", goldenFace("ended"));
    for (const sid of ["a", "c"]) tm.onFace(sid, goldenFace("live-needs-you-tmux"));
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
    tm.createSkeletonTab("a", "/p/a", LOCAL, null);
    tm.createSkeletonTab("b", "/p/b", LOCAL, null);
    tm.switchTo("a");
    const shown: boolean[] = [];
    tm.onSlotShown = (on) => shown.push(on);
    tm.addLaunchSlot({ origin: "devbox", cwd: "/home/u/srv/billing", tmuxName: "billing-2", arrival: { by: { kind: "cwd", cwd: "/home/u/srv/billing" }, since: 0, session: null, host: null } });
    tm.createSkeletonTab("c", "/p/c", LOCAL, null);
    setGroups(tm, [{ id: "g1", name: "组一", tabs: ["b"] }]);
    tm.repaintTabBar();
    const list = home(tm).bar.listEl;
    const last = list.lastElementChild as HTMLElement;
    expect(last.querySelector("[data-slot]")?.textContent).toBe(`devbox/home/u/srv/billing${copyText("launch.placeholder.title")}`);
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
    tm.createSkeletonTab("a", "/p/a", LOCAL, null);
    tm.createSkeletonTab("s1", "/p/s1", LOCAL, null);
    tm.switchTo("a");
    const taken = tm.addLaunchSlot({ origin: LOCAL, cwd: "/p", tmuxName: null, arrival: { by: { kind: "sid", sid: "s1" }, since: 0, session: null, host: null } });
    expect(taken, "已有标签页 ⇒ 不接").toBe(false);
    expect(tm.debugSlots().ids, "没长占位页").toEqual([]);
    expect(tm.activeSessionId(), "切到了那个标签页").toBe("s1");
    expect(tm.addLaunchSlot({ origin: LOCAL, cwd: "/p", tmuxName: null, arrival: { by: { kind: "sid", sid: "s9" }, since: 0, session: null, host: null } }), "没有那个标签页 ⇒ 照旧长").toBe(true);
    expect(tm.addLaunchSlot({ origin: LOCAL, cwd: "/p", tmuxName: null, arrival: { by: { kind: "cwd", cwd: "/p" }, since: 0, session: null, host: null } })).toBe(true);
    expect(tm.debugSlots().ids).toEqual([1, 2]);
  });

  it("★ 占位那一页显着 ⇒ 作用于当前会话的快捷键（查找 · 折叠 · 翻轮 · 到底 · 关 · 开目录 · 开窗 · 菜单 · 终端）不落到底下那个真标签页；全局的照常", () => {
    const tm = makeTM();
    tm.createSkeletonTab("a", "/p/a", LOCAL, null);
    tm.switchTo("a");
    const sessionKeys = ["session.find", "session.toggle-process", "session.prev-turn", "session.next-turn", "session.to-bottom", "tab.close-archived", "tab.open-cwd", "tab.pop-out", "tab.context-menu", "terminal.bring-front", "panel.toggle-terminal"] as const;
    expect(sessionKeys.filter((k) => tm.shadowedBySlot(k)), "没有占位页时都照常").toEqual([]);
    tm.addLaunchSlot({ origin: "devbox", cwd: "/w/x", tmuxName: null, arrival: { by: { kind: "cwd", cwd: "/w/x" }, since: 0, session: null, host: null } });
    expect(sessionKeys.filter((k) => !tm.shadowedBySlot(k))).toEqual([]);
    expect(tm.shadowedBySlot("app.open-command-bar"), "全局的照常").toBe(false);
    expect(tm.shadowedBySlot("needs.next")).toBe(false);
    tm.switchTo("a");
    expect(tm.shadowedBySlot("session.find"), "切回真标签页 ⇒ 照常").toBe(false);
  });
});

// ==========================================================================
// 组头：收起 / 展开（落盘）· 组员数 · 收着时汇总 · 「⋯」菜单 · 跳到收着的组里自动展开 · 数字键跳过收着的
// ==========================================================================
describe("组头", () => {
  let tm: TabManager;
  let bar: HTMLElement;
  const flushBar = (): void => (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
  const head = (): HTMLElement => bar.querySelector<HTMLElement>(".tab-group-head")!;
  const collapsed = (): boolean => bar.querySelector<HTMLElement>(".tab-group")!.classList.contains("is-collapsed");
  beforeEach(() => {
    vi.clearAllMocks();
    closeMenu();
    tm = makeTM();
    bar = document.body.firstElementChild as HTMLElement;
    home(tm).prefs.collectionsLoaded = true;
    for (const sid of ["x", "a", "b", "c", "y"]) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL);
    setGroups(tm, [{ id: "g", name: "订单", tabs: ["a", "b", "c"] }]);
    // 组头那几颗是后端数好的（`groups-read` 的 `counts`），界面照画。
    home(tm).prefs.collections[0].counts = [
      { dot: "needs-you", n: 1, word: goldenFace("live-needs-you-tmux").name, text: "核心·需手动 1" },
      { dot: "running", n: 1, word: goldenFace("live-working-plain").name, text: "核心·在跑 1" },
    ];
    home(tm).store.tabs.get("a")!.face = liveFace("needs_you");
    home(tm).store.tabs.get("b")!.face = liveFace("working");
    home(tm).store.tabs.get("c")!.face = liveFace("idle");
    tm.switchTo("x");
    flushBar();
  });
  afterEach(() => closeMenu());

  it("组头：名字 · 组员数；点 ⌄ 收起（落盘）⇒ 组员藏起、汇总出「等你 1 · 在跑 1」；再点展开、汇总收掉", () => {
    expect(head().querySelector(".tab-group-name")!.textContent).toBe("订单");
    expect(head().querySelector(".tab-group-count")!.textContent).toBe("3");
    expect(head().querySelector(".tab-group-sum")!.textContent, "展开时不汇总").toBe("");
    head().querySelector<HTMLElement>(".tab-group-caret")!.click();
    expect(collapsed()).toBe(true);
    expect(home(tm).prefs.collections[0].collapsed, "收着记在组上（落盘那一格）").toBe(true);
    expect(head().querySelector(".tab-group-sum")!.textContent).toBe(
      "核心·需手动 1" + "核心·在跑 1", // 照抄后端写好的那一句
    );
    expect(head().querySelector(".tab-group-caret")!.getAttribute("aria-expanded")).toBe("false");
    head().querySelector<HTMLElement>(".tab-group-caret")!.click();
    expect(collapsed()).toBe(false);
    expect(home(tm).prefs.collections[0].collapsed).toBeUndefined();
    expect(head().querySelector(".tab-group-sum")!.textContent).toBe("");
  });

  it("点组头空白处 ＝ 收起 / 展开；点名字 ＝ 改名（不收起）", () => {
    head().querySelector<HTMLElement>(".tab-group-sp")!.click();
    expect(collapsed()).toBe(true);
    head().querySelector<HTMLElement>(".tab-group-name")!.click();
    expect(collapsed(), "点名字不切收展").toBe(true);
    expect(head().querySelector("input"), "名字框打开").not.toBeNull();
  });

  it("数字键 · ] [ 跳过收着的组里的；关掉邻居不落进收着的组", () => {
    head().querySelector<HTMLElement>(".tab-group-caret")!.click();
    expect(home(tm).bar.navOrder()).toEqual(["x", "y"]);
    tm.jumpToIndex(2);
    expect(home(tm).store.activeId, "第 2 个是 y（a b c 收着）").toBe("y");
    tm.cycleActive(-1);
    expect(home(tm).store.activeId).toBe("x");
  });

  it("切到收着的组里的那一个（Ctrl+J · 命令面板 · 通知都走这里）⇒ 那个组展开；当前在收着的组里 ⇒ 组头左边一道 accent", () => {
    head().querySelector<HTMLElement>(".tab-group-caret")!.click();
    tm.switchTo("a");
    expect(collapsed(), "跳进去 ⇒ 展开").toBe(false);
    head().querySelector<HTMLElement>(".tab-group-caret")!.click();
    expect(head().classList.contains("is-current"), "当前标签页在收着的组里").toBe(true);
    tm.switchTo("x");
    expect(head().classList.contains("is-current")).toBe(false);
  });

  it("「⋯」菜单：改名 · 收起 · 全部收起 · 全部展开 · 在计划中查看本组接手的块 · 关闭组里已结束的（n，0 时灰）· 解散分组", () => {
    head().querySelector<HTMLElement>(".tab-group-more")!.click();
    const labels = [...document.body.querySelectorAll("[role=menu] [role^=menuitem]")].map((b) => b.textContent ?? "");
    expect(labels.map((l) => l.split("\n")[0])).toEqual([
      copyText("rsFilewinWriteops.label.rename"),
      copyText("kit.dock.close"),
      copyText("tabMenu.group.collapseAll"),
      copyText("history.list.expandAll"),
      copyText("tabMenu.group.inPlan"),
      copyText("sessionState.closeEnded.inGroup", { n: 0 }),
      copyText("tabMenu.group.dissolve"),
    ]);
    expect((groupMenuItem(copyText("sessionState.closeEnded.inGroup", { n: 0 })) as HTMLButtonElement).disabled, "一个已结束的都没有 ⇒ 灰").toBe(true);
    groupMenuItem(copyText("tabMenu.group.collapseAll")).click();
    expect(collapsed()).toBe(true);
  });

  it("关闭组里已结束的：一次关掉、一条「已关闭 N 个［撤销］」；撤销 ⇒ 原位放回", () => {
    tm.onFace("a", goldenFace("ended"));
    tm.onFace("c", goldenFace("ended"));
    flushBar();
    head().querySelector<HTMLElement>(".tab-group-more")!.click();
    groupMenuItem(copyText("sessionState.closeEnded.inGroup", { n: 2 })).click();
    expect(home(tm).store.orderedIds).toEqual(["x", "b", "y"]);
    expect(vi.mocked(undoToast).mock.calls.map((c) => c[0])).toEqual([copyText("tabBar.close.doneMany", { n: 2 })]);
    vi.mocked(undoToast).mock.calls[0][1]();
    expect(home(tm).store.orderedIds).toEqual(["x", "a", "b", "c", "y"]);
    expect(membersOf(tm, "g")).toEqual(["a", "b", "c"]);
  });

  it("组头右键 ＝ 同一份组的菜单", () => {
    head().dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 }));
    expect(groupMenuItem(copyText("tabMenu.group.dissolve"))).toBeTruthy();
  });

  it("拖到收着的组头上 ⇒ 进组排最后，组保持收着", () => {
    head().querySelector<HTMLElement>(".tab-group-caret")!.click();
    home(tm).dragger.applyDrop("y", { kind: "insert", at: { sid: "c", side: "after" }, gid: "g", head: "g" });
    expect(membersOf(tm, "g")).toEqual(["a", "b", "c", "y"]);
    expect(collapsed()).toBe(true);
    expect(head().querySelector(".tab-group-count")!.textContent).toBe("4");
  });
});

// ==========================================================================
// 拖整个组 · 多选一起拖 · 拖的时候栏自己滚（假手势打真事件链）
// ==========================================================================
describe("拖整组 · 多选拖 · 自动滚", () => {
  let tm: TabManager;
  let bar: HTMLElement;
  const flushBar = (): void => (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
  const rootOf = (sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root;
  const headEl = (): HTMLElement => bar.querySelector<HTMLElement>(".tab-group-head")!;
  const rect = (el: HTMLElement, top: number, height: number): void => {
    el.getBoundingClientRect = (): DOMRect => ({ top, height, bottom: top + height, left: 0, right: 260, width: 260 }) as DOMRect;
  };
  const move = (y: number): void => {
    document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: y, bubbles: true }));
  };
  const up = (y: number): void => {
    document.dispatchEvent(new MouseEvent("mouseup", { clientX: 10, clientY: y, bubbles: true }));
  };
  const down = (el: HTMLElement, y: number): void => {
    el.dispatchEvent(new MouseEvent("mousedown", { button: 0, buttons: 1, clientX: 10, clientY: y, bubbles: true }));
  };
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
    bar = document.body.firstElementChild as HTMLElement;
    home(tm).prefs.collectionsLoaded = true;
    for (const sid of ["x", "a", "b", "y", "z"]) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL);
    setGroups(tm, [{ id: "g", name: "订单", tabs: ["a", "b"] }]);
    flushBar();
    // 栏：x 0–30 · 组头 34–60 · a 60–90 · b 90–120 · y 126–156 · z 156–186
    rect(rootOf("x"), 0, 30);
    rect(headEl(), 34, 26);
    rect(rootOf("a"), 60, 30);
    rect(rootOf("b"), 90, 30);
    rect(rootOf("y"), 126, 30);
    rect(rootOf("z"), 156, 30);
  });
  afterEach(() => up(0));

  it("按住组头拖到最后 ⇒ 整个组挪过去、组员次序不变、组还是那个组；给「已移动分组」可撤；松手那次 click 不收展", () => {
    down(headEl(), 40);
    move(50);
    expect(document.body.querySelector(".tab-drag-ghost")!.textContent).toBe(copyText("tabDrop.ghost.group", { name: "订单", n: 2 }));
    expect(bar.querySelector(".tab-group")!.classList.contains("dragging"), "整个组变暗").toBe(true);
    move(180); // z 的下半 ⇒ z 后面
    up(180);
    expect(home(tm).store.orderedIds).toEqual(["x", "y", "z", "a", "b"]);
    expect(membersOf(tm, "g")).toEqual(["a", "b"]);
    expect(vi.mocked(undoToast).mock.calls.map((c) => c[0])).toEqual([copyText("tabBar.group.moved", { name: "订单" })]);
    headEl().click();
    expect(bar.querySelector(".tab-group")!.classList.contains("is-collapsed"), "拖完那次 click 被吞掉").toBe(false);
  });

  it("拖组压在别的标签页中间停多久都不合并（不攒停留）", () => {
    vi.useFakeTimers();
    try {
      down(headEl(), 40);
      move(50);
      move(141); // y 的正中
      expect(document.body.querySelector(".tab-drag-ghost"), "前置：真在拖整组").not.toBeNull();
      vi.advanceTimersByTime(DWELL_MS * 3);
      expect(rootOf("y").classList.contains("drop-onto")).toBe(false);
      up(141);
      expect(membersOf(tm, "g")).toEqual(["a", "b"]);
      expect(home(tm).store.tabs.get("y")!.group).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("多选后拖其中一个 ⇒ 选中的一起走，按原先后挨在一起、仍选着；影子写「+N」", () => {
    tm.switchTo("y");
    home(tm).bar["host"].pick("z", "toggle"); // 第一次 Ctrl 点把当前那个（y）带上
    down(rootOf("z"), 170);
    move(160);
    move(5); // x 的上 1/4 ⇒ x 前面
    expect(document.body.querySelector(".tab-drag-ghost")!.textContent).toContain(copyText("tabDrop.ghost.many", { title: "z", n: 1 }).slice(-2));
    up(5);
    expect(home(tm).store.orderedIds).toEqual(["y", "z", "x", "a", "b"]);
    const selected = ["x", "y", "z", "a", "b"].filter((s) => rootOf(s).classList.contains("selected"));
    expect(selected, "仍选着").toEqual(["y", "z"]);
    expect(bar.querySelectorAll(".tab.selected").length).toBe(2);
  });

  it("多选一起压在组员中间停住 ⇒ 一起进组，「已移到「订单」 ×2」", async () => {
    vi.useFakeTimers();
    try {
      tm.switchTo("y");
      home(tm).bar["host"].pick("z", "toggle");
      down(rootOf("y"), 140);
      move(130);
      move(75); // a 的正中
      vi.advanceTimersByTime(DWELL_MS);
      up(75);
      expect(membersOf(tm, "g")).toEqual(["a", "y", "z", "b"]);
      await vi.advanceTimersByTimeAsync(0);
      expect(vi.mocked(undoToast).mock.calls.map((c) => c[0])).toEqual([copyText("tabBar.group.joinedMany", { name: "订单", n: 2 })]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("拖到栏底边 24px 内 ⇒ 栏每帧往下滚（离边越近越快），离开那一段就停", () => {
    const list = bar.querySelector<HTMLElement>(".tab-list")!;
    rect(list, 0, 200);
    let top = 0;
    Object.defineProperty(list, "scrollTop", { configurable: true, get: () => top, set: (v: number) => (top = v) });
    const frames: FrameRequestCallback[] = [];
    const raf = vi.spyOn(globalThis, "requestAnimationFrame").mockImplementation((cb) => (frames.push(cb), frames.length));
    try {
      down(rootOf("x"), 10);
      move(100); // 中间：不滚
      expect(frames.length, "中间那一段不排帧").toBe(0);
      move(198); // 离栏底 2px
      expect(frames.length, "排上了一帧").toBe(1);
      frames.shift()!(0);
      expect(top, "贴边那一段一帧滚 11px").toBe(11);
      expect(frames.length, "还在那一段 ⇒ 接着排下一帧").toBe(1);
      move(100); // 回到中间
      frames.shift()!(0);
      expect(top, "离开那一段 ⇒ 不再滚").toBe(11);
    } finally {
      raf.mockRestore();
    }
  });
});

// ==========================================================================
// 键盘（焦点在栏里）· 右键「分组 ▸」· 命令（分组第四批）
// ==========================================================================
describe("栏里的键盘 · 右键「分组 ▸」", () => {
  let tm: TabManager;
  let bar: HTMLElement;
  const flushBar = (): void => (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
  const rootOf = (sid: string): HTMLElement => home(tm).bar.tabButtons.get(sid)!.root;
  const headEl = (): HTMLElement => bar.querySelector<HTMLElement>(".tab-group-head")!;
  const press = (key: string, init: KeyboardEventInit = {}): void => {
    (document.activeElement as HTMLElement).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
  };
  const focused = (): string => {
    const r = home(tm).bar.focusedRow();
    return r === null ? "-" : r.kind === "tab" ? r.sid : `H:${r.gid}`;
  };
  const toasts = (): string[] => vi.mocked(undoToast).mock.calls.map((c) => String(c[0]));
  const menuLabels = (): string[] =>
    [...document.body.querySelectorAll<HTMLElement>("[role=menu] [role^=menuitem]")].map((b) => b.querySelector("[data-part=label]")?.textContent ?? b.textContent ?? "");
  const closeMenus = (): void => document.body.querySelectorAll("[role=menu]").forEach((n) => n.remove());
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
    bar = document.body.firstElementChild as HTMLElement;
    home(tm).prefs.collectionsLoaded = true;
    for (const sid of ["x", "a", "b", "y", "z"]) tm.ensureTab(sid, `/w/${sid}`, "p", LOCAL);
    setGroups(tm, [{ id: "g", name: "订单", tabs: ["a", "b"] }]);
    flushBar();
  });
  afterEach(closeMenus);

  it("↑↓ 走行：组头也是一行；收着的组只走组头", () => {
    rootOf("x").focus();
    const walk: string[] = [focused()];
    for (let i = 0; i < 5; i++) {
      press("ArrowDown");
      walk.push(focused());
    }
    expect(walk).toEqual(["x", "H:g", "a", "b", "y", "z"]);
    press("ArrowUp");
    expect(focused()).toBe("y");
    home(tm).prefs.collections[0].collapsed = true;
    flushBar();
    rootOf("x").focus();
    press("ArrowDown");
    press("ArrowDown");
    expect(focused(), "收着的组员跳过").toBe("y");
  });

  it("Enter：标签页切过去；组头收起 / 展开。← 组员上跳到组头、组头上收起；→ 展开", () => {
    rootOf("y").focus();
    press("Enter");
    expect(tm.activeSessionId()).toBe("y");
    rootOf("b").focus();
    press("ArrowLeft");
    expect(focused()).toBe("H:g");
    press("ArrowLeft");
    expect(home(tm).prefs.collections[0].collapsed).toBe(true);
    expect(focused(), "收起之后焦点还在组头").toBe("H:g");
    press("ArrowRight");
    expect(home(tm).prefs.collections[0].collapsed ?? false).toBe(false);
    press("Enter");
    expect(home(tm).prefs.collections[0].collapsed).toBe(true);
  });

  it("Space：选中 / 取消（同 Ctrl 点），不切过去", () => {
    tm.switchTo("x");
    rootOf("z").focus();
    press(" ");
    expect(tm.activeSessionId()).toBe("x");
    expect(rootOf("z").classList.contains("selected")).toBe(true);
  });

  it("Alt+↓ / Alt+↑：挪一格，跨过组边界就进 / 出组；焦点跟着那一行；改了分组的给可撤 toast", async () => {
    rootOf("x").focus();
    tm.moveFocused(1);
    expect(membersOf(tm, "g"), "从组上面往下碰到组头 ⇒ 进组排第一").toEqual(["x", "a", "b"]);
    expect(focused()).toBe("x");
    await settleGroups();
    expect(toasts()).toEqual([copyText("tabBar.group.joined", { name: "订单" })]);
    tm.moveFocused(-1);
    expect(home(tm).store.tabs.get("x")!.group, "组里第一个再往上 ⇒ 出组").toBeNull();
    expect(home(tm).bar.visibleOrder()).toEqual(["x", "a", "b", "y", "z"]);
    await settleGroups();
    expect(toasts()[1]).toBe(copyText("tabBar.group.left", { name: "订单" }));
    rootOf("z").focus();
    tm.moveFocused(-1);
    expect(home(tm).bar.visibleOrder(), "散的之间只排序").toEqual(["x", "a", "b", "z", "y"]);
    expect(toasts(), "只排了顺序不出条").toHaveLength(2);
  });

  it("组头上 Alt+↓ ⇒ 整组挪一格；给「已移动分组」", () => {
    headEl().focus();
    tm.moveFocused(1);
    expect(home(tm).bar.visibleOrder()).toEqual(["x", "y", "a", "b", "z"]);
    expect(focused()).toBe("H:g");
    expect(toasts()).toEqual([copyText("tabBar.group.moved", { name: "订单" })]);
  });

  it("Alt+← 出组（放到组后面）；Alt+→ 与上一行成组（进它的组 / 和散的建组 ＋ 名字框打开）", () => {
    rootOf("a").focus();
    tm.leaveFocused();
    expect(home(tm).bar.visibleOrder()).toEqual(["x", "b", "a", "y", "z"]);
    expect(home(tm).store.tabs.get("a")!.group).toBeNull();
    rootOf("a").focus();
    tm.joinPrevFocused();
    expect(membersOf(tm, "g")).toEqual(["b", "a"]);
    rootOf("z").focus();
    tm.joinPrevFocused();
    const gz = home(tm).store.tabs.get("z")!.group;
    expect(gz, "和 y 建了组").not.toBeNull();
    expect(home(tm).store.tabs.get("y")!.group).toBe(gz);
    expect(document.activeElement?.tagName, "名字框打开").toBe("INPUT");
  });

  it("F2 改焦点那一行：组头上 ⇒ 组名；组员 · 散的标签页上 ⇒ 这个标签页的名（不是它所在组的名）", () => {
    headEl().focus();
    tm.renameFocused();
    expect(document.activeElement?.tagName).toBe("INPUT");
    expect((document.activeElement as HTMLInputElement).value).toBe("订单");
    press("Escape");
    expect(focused(), "Esc 收框 ⇒ 焦点回组头").toBe("H:g");
    for (const sid of ["b", "y"]) {
      rootOf(sid).focus();
      tm.renameFocused();
      const box = document.activeElement as HTMLInputElement;
      expect(box.tagName, `${sid} 上开了框`).toBe("INPUT");
      expect(box.value, `${sid} 上改的是标签页名`).toBe(home(tm).store.tabs.get(sid)!.label ?? "");
      expect(box.placeholder).toBe(copyText("history.rename.prompt"));
      box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    }
  });

  it("Shift+F10 / 菜单键：开焦点那一行的菜单（组头 ⇒ 组的菜单）；焦点不在栏里 ⇒ 照旧当前标签页", () => {
    tm.switchTo("x");
    headEl().focus();
    tm.openActiveMenu();
    expect(menuLabels()).toContain(copyText("tabMenu.group.dissolve"));
    closeMenus();
    rootOf("y").getClientRects = () => [new DOMRect(0, 0, 10, 10)] as unknown as DOMRectList;
    rootOf("y").focus();
    tm.openActiveMenu();
    expect(menuLabels()).toContain(copyText("tabMenu.collection.add"));
    expect(menuLabels()).not.toContain(copyText("tabMenu.group.dissolve"));
  });

  it("F6：栏 ↔ 主区轮", () => {
    tm.switchTo("y");
    (document.activeElement as HTMLElement | null)?.blur();
    tm.cycleFocus();
    expect(focused(), "进栏落在当前标签页上").toBe("y");
    tm.cycleFocus();
    expect(home(tm).bar.focusedRow(), "回主区").toBeNull();
  });

  it("右键「分组 ▸」：新建分组 · 各组（所在的打勾）· 移出分组（不在组里灰）；顶层没有单独的「移出分组「x」」", () => {
    rootOf("a").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const labels = menuLabels();
    expect(labels).toContain(copyText("tabMenu.collection.add"));
    expect(labels.filter((l) => l === copyText("tabMenu.collection.remove")), "只在子菜单里出一次").toHaveLength(1);
    const items = [...document.body.querySelectorAll<HTMLElement>("[role=menu] [role^=menuitem]")];
    const of = (l: string): HTMLElement => items.find((b) => (b.querySelector("[data-part=label]")?.textContent ?? b.textContent) === l)!;
    expect(of("订单").getAttribute("aria-checked") ?? of("订单").getAttribute("role"), "所在的打勾").toMatch(/true|checkbox|radio/);
    closeMenus();
    rootOf("y").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    const leave = [...document.body.querySelectorAll<HTMLElement>("[role=menu] [role^=menuitem]")].find(
      (b) => (b.querySelector("[data-part=label]")?.textContent ?? "") === copyText("tabMenu.collection.remove"),
    )!;
    expect(leave.getAttribute("aria-disabled") === "true" || (leave as HTMLButtonElement).disabled, "散的 ⇒ 移出分组灰着").toBe(true);
  });

  it("右键「新建分组」：不弹框，建「分组 N」、名字框打开；可撤", async () => {
    rootOf("y").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    groupMenuItem(copyText("tabMenu.collection.new")).click();
    expect(document.body.querySelector('[aria-modal="true"]'), "不弹对话框").toBeNull();
    const gid = home(tm).store.tabs.get("y")!.group!;
    expect(home(tm).prefs.collections.find((c) => c.id === gid)?.name).toBe(copyText("tabDrop.group.defaultName", { n: 1 }));
    expect(document.activeElement?.tagName, "名字框打开").toBe("INPUT");
    await settleGroups();
    expect(toasts()).toEqual([copyText("tabBar.group.founded", { name: copyText("tabDrop.group.defaultName", { n: 1 }) })]);
  });

  it("右键点别的组 ⇒ 挪过去（排最后），给「已移到」可撤", async () => {
    rootOf("y").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    groupMenuItem("订单").click();
    expect(membersOf(tm, "g")).toEqual(["a", "b", "y"]);
    await settleGroups();
    expect(toasts()).toEqual([copyText("tabBar.group.joined", { name: "订单" })]);
  });

  it("命令面板三条：列不列看组数与作用对象；新建 / 移出作用于当前（选了几个 ⇒ 选中的）；加入分组…开「分组 ▸」那一层", async () => {
    tm.switchTo("y");
    expect(tm.groupCommandState(), "一个组、当前散着").toEqual({ groups: 1, grouped: 0 });
    tm.foundGroupFromCommand();
    const gy = home(tm).store.tabs.get("y")!.group!;
    expect(gy).not.toBeNull();
    expect(document.activeElement?.tagName, "名字框打开").toBe("INPUT");
    await settleGroups();
    (document.activeElement as HTMLElement).blur();
    expect(toasts()).toEqual([copyText("tabBar.group.founded", { name: copyText("tabDrop.group.defaultName", { n: 1 }) })]);
    expect(tm.groupCommandState()).toEqual({ groups: 2, grouped: 1 });
    tm.leaveGroupFromCommand();
    expect(home(tm).store.tabs.get("y")!.group).toBeNull();
    // 选了几个 ⇒ 作用于选中的。
    tm.switchTo("a");
    for (const sid of ["a", "z"]) rootOf(sid).dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
    expect(tm.groupCommandState()).toEqual({ groups: 1, grouped: 1 });
    tm.leaveGroupFromCommand();
    expect(home(tm).store.tabs.get("a")!.group).toBeNull();
    expect(home(tm).store.tabs.get("z")!.group).toBeNull();
    rootOf("a").getClientRects = () => [new DOMRect(0, 0, 10, 10)] as unknown as DOMRectList;
    tm.openGroupMenuFromCommand();
    expect(menuLabels()).toContain(copyText("tabBatch.menu.found", { n: 2 }));
    expect(menuLabels()).toContain(copyText("tabBatch.menu.joinOne", { name: "订单", n: 2 }));
  });
});

// 〔流只发要看的会话〕主窗口报「在看哪个会话」；会话刚进那台那条流的名单（`watch` 格）⇒ 没在看那段没上流的补上。
describe("〔stream-watch〕在看哪个会话 · 补上没上流的那段", () => {
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    tm = makeTM();
    // 当前 tab 是别的那一个：被补的 tab 在后台（切进来那一脚往上翻的取回不掺进来）。
    tm.ensureTab("front", null, "/p/front.jsonl", LOCAL);
    tm.switchTo("front");
  });
  const line = (sid: string, seq: number) =>
    ({ session_id: sid, cwd: "/p", path: `/p/${sid}.jsonl`, seq, record: { t: "reply", id: `${sid}-${seq}` } as never }) as never;
  /** 按行号取：`[from, until ?? END)` 一页答完。 */
  const END = 3000;
  const answerLines = () =>
    vi.mocked(invoke).mockImplementation(withSessionReads((cmd: string, args?: Record<string, unknown>) => {
      if (cmd !== "read_session_lines") return Promise.resolve(undefined);
      const sid = String(args!.jsonlPath).replace(/^\/p\/|\.jsonl$/g, "");
      const from = args!.from as number;
      const until = (args!.until as number | undefined) ?? END;
      return Promise.resolve({
        from,
        next: until,
        eof: args!.until === undefined,
        payloads: Array.from({ length: until - from }, (_, k) => line(sid, from + k)),
      });
    }) as never);
  const lineReads = () =>
    recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_lines").filter((a) => (a as { jsonlPath: string }).jsonlPath !== "/p/front.jsonl").map((a) => {
      const { from, until } = a as { from: number; until?: number };
      return { from, until };
    });
  const settle = () => new Promise((r) => setTimeout(r, 0));

  it("切 tab ⇒ 报壳「主窗口在看这一个」（哪台 · 哪个会话）", () => {
    tm.ensureTab("wa", null, "/p/wa.jsonl", LOCAL);
    tm.ensureTab("wb", null, "/p/wb.jsonl", "box");
    tm.switchTo("wb");
    const reports = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "watch_sessions").map((c) => c[1]);
    expect(reports.at(-1)).toEqual({ sessions: [{ origin: "box", sessionId: "wb" }] });
  });

  it("缺口不大 ⇒ 从见过的、上流点之下最大那一行之后按行号取到上流点，取回的落进 tab", async () => {
    answerLines();
    for (let s = 0; s < 10; s++) tm.onLine(line("wg", s));
    tm.onLine(line("wg", 50)); // 上流点那一行比应答先到
    tm.onSessionWatch(LOCAL, { sid: "wg", path: "/p/wg.jsonl", seq: 50 });
    await settle();
    expect(lineReads()).toEqual([{ from: 10, until: 50 }]);
    const t = home(tm).store.tabs.get("wg")!;
    for (let s = 0; s <= 50; s++) expect(t.seenSeqs.has(s), `第 ${s} 行没补上`).toBe(true);
  });

  it("★ 补上的那段落进 tab 之后，过程的划段重新要一次（不然新补的卡按补之前那一份归进还开着的那一段过程、折着看不见）", async () => {
    answerLines();
    const lines = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation((async (cmd: string, args?: unknown) =>
      isChanCall(cmd, args, "history-turns") ? chanReply({ from: 0, end: 0, turns: [] }) : lines(cmd, args as never)) as never);
    const turnsAsks = () =>
      vi.mocked(invoke).mock.calls.filter((c) => {
        const a = c[1] as { op?: string; payload?: number[] };
        return c[0] === "chan_call" && a.op === "history-turns" && (JSON.parse(new TextDecoder().decode(new Uint8Array(a.payload ?? []))) as { path?: string }).path === "/p/wf.jsonl";
      }).length;
    for (let s = 0; s < 10; s++) tm.onLine(line("wf", s));
    tm.switchTo("wf"); // 后台 tab 的第一趟排着（`tab-cold-reads.ts`）；切过来才当场要
    for (let k = 0; k < 5; k++) await settle();
    const before = turnsAsks();
    tm.onLine(line("wf", 50));
    for (let k = 0; k < 5; k++) await settle();
    const live = turnsAsks();
    expect(live, "实时来一行就要一次").toBe(before + 1);
    tm.onSessionWatch(LOCAL, { sid: "wf", path: "/p/wf.jsonl", seq: 50 });
    for (let k = 0; k < 5; k++) await settle();
    expect(lineReads()).toEqual([{ from: 10, until: 50 }]);
    expect(turnsAsks(), "补上的那段落了以后没再要划段").toBeGreaterThan(live);
  });

  it("缺得多 ⇒ tab 整份重来，只取上流点之下那一截尾巴：先取视口那一截到末尾（重来时丢掉的实时行也在里面），再取更早的那一截", async () => {
    answerLines();
    for (let s = 0; s < 10; s++) tm.onLine(line("wbig", s));
    tm.onLine(line("wbig", 2000));
    const before = home(tm).store.tabs.get("wbig")!;
    tm.onSessionWatch(LOCAL, { sid: "wbig", path: "/p/wbig.jsonl", seq: 2000 });
    for (let k = 0; k < 5; k++) await settle();
    const reads = lineReads();
    expect(reads, "先取视口那一截、再取更早那一截，两截接得上、合起来正好是上流点之下 600 行").toEqual([
      { from: reads[0]!.from, until: undefined },
      { from: 1400, until: reads[0]!.from },
    ]);
    expect(reads[0]!.from, "视口那一截不在上流点之下 600 行里").toBeGreaterThan(1400);
    const t = home(tm).store.tabs.get("wbig")!;
    expect(t, "缺得多却没整份重来").not.toBe(before);
    expect(t.seenSeqs.has(5), "重来之后旧的那段还在账上").toBe(false);
    expect(t.seenSeqs.has(1400) && t.seenSeqs.has(2000) && t.seenSeqs.has(END - 1)).toBe(true);
  });

  it("★ 缺得多整份重来时，已经拿过的轮次与大纲接着续：轮次从还没收尾那一轮的开头、大纲从上次的末端，不从 0 再要", async () => {
    answerLines();
    const lines = vi.mocked(invoke).getMockImplementation()!;
    const turn = { at: 100, uuid: "wr-u", shown: [], folds: [], landed: [], done: false } as never;
    vi.mocked(invoke).mockImplementation((async (cmd: string, args?: unknown) => {
      if (isChanCall(cmd, args, "history-turns")) {
        const from = (chanArgsJson(args as never) as { from: number }).from;
        return chanReply({ from, end: 900, next: null, turns: from <= 100 ? [turn] : [] });
      }
      if (isChanCall(cmd, args, "history-user-inputs")) {
        const from = (chanArgsJson(args as never) as { from: number }).from;
        return chanReply({ from, end: 500, next: null, entries: [] });
      }
      return lines(cmd, args as never);
    }) as never);
    const asks = (op: string) =>
      vi.mocked(invoke).mock.calls
        .filter((c) => isChanCall(c[0] as string, c[1], op))
        .map((c) => chanArgsJson(c[1] as never) as { path: string; from: number })
        .filter((a) => a.path === "/p/wr.jsonl")
        .map((a) => a.from);
    for (let s = 0; s < 10; s++) tm.onLine(line("wr", s));
    tm.switchTo("wr");
    // 停够了（`STAY_MS`）才要第一趟：等那一问真发出去。
    await vi.waitFor(() => expect(asks("history-turns"), "切进来第一趟轮次从 0").toContain(0), UNTIL_CEILING);
    for (let k = 0; k < 5; k++) await settle();
    const t0 = home(tm).store.tabs.get("wr")!;
    await t0.outline.refresh();
    tm.switchTo("front");
    const turnsBefore = asks("history-turns").length;
    const inputsBefore = asks("history-user-inputs").length;
    tm.onSessionWatch(LOCAL, { sid: "wr", path: "/p/wr.jsonl", seq: 2000 });
    for (let k = 0; k < 5; k++) await settle();
    const t = home(tm).store.tabs.get("wr")!;
    expect(t, "缺得多却没重来正文").not.toBe(t0);
    tm.switchTo("wr");
    await vi.waitFor(() => expect(asks("history-turns").length, "重来之后没再要轮次").toBeGreaterThan(turnsBefore), UNTIL_CEILING);
    await t.outline.refresh();
    for (let k = 0; k < 5; k++) await settle();
    const turnsAfter = asks("history-turns").slice(turnsBefore);
    const inputsAfter = asks("history-user-inputs").slice(inputsBefore);
    expect(turnsAfter.length, "重来之后没再要轮次").toBeGreaterThan(0);
    expect(turnsAfter, "重来之后轮次又从 0 整份要").not.toContain(0);
    expect(inputsAfter, "重来之后大纲又从 0 整份要").not.toContain(0);
    expect(t.turnFold.all.map((x) => x.at), "轮次没接过来").toEqual([100]);
  });

  it("冷起那一形（tab 一行都没见过、刚进「在看」名单）⇒ 取回的行喂进去时打首批那一句，取完之后启动 timeline 落一次盘", async () => {
    answerLines();
    tm.ensureTab("wcold", null, "/p/wcold.jsonl", LOCAL);
    tm.switchTo("wcold");
    tm.onSessionWatch(LOCAL, { sid: "wcold", path: "/p/wcold.jsonl", seq: 2000 });
    for (let k = 0; k < 5; k++) await settle();
    const first = lineReads()[0]!;
    expect(first.until, "冷起那一趟：视口那一截取到末尾").toBeUndefined();
    expect(first.from, "冷起那一趟先取的那一截不在上流点之下 600 行里").toBeGreaterThan(1400);
    const arrived = vi.mocked(startupPerf.noteHistoryArrived).mock.invocationCallOrder;
    const fed = vi.mocked(startupPerf.noteHistoryFed).mock;
    const filled = vi.mocked(startupPerf.noteColdFilled).mock.invocationCallOrder;
    expect(arrived.length, "取回的行喂进去时没记首批").toBeGreaterThan(0);
    expect(fed.calls[0]?.[0], "冷起那一趟先喂进去的是视口那一截").toBe(END - first.from);
    expect(filled, "视口那一截落地之后 timeline 该落一次盘").toHaveLength(1);
    expect(filled[0], "落盘要在那一截喂完之后").toBeGreaterThan(fed.invocationCallOrder[0]!);
  });

  it("★ 冷起那一形：先取上流点之下视口那一截、物化出来，更早的那一截还没回也不等（点开新 tab 不空白着等 600 行）", async () => {
    answerLines();
    const lines = vi.mocked(invoke).getMockImplementation()!;
    // 从最老那一行（上流点之下第 600 行）起的取回一直不回：真记录一行几十 KB，那一截要好几页、后端每页从文件头数行
    vi.mocked(invoke).mockImplementation(((cmd: string, args?: unknown) => {
      const read = recordReadOf(cmd, args);
      return read?.[0] === "read_session_lines" && read[1].from === 1400 ? new Promise(() => {}) : lines(cmd, args as never);
    }) as never);
    tm.ensureTab("wfirst", null, "/p/wfirst.jsonl", LOCAL);
    tm.switchTo("wfirst");
    tm.onSessionWatch(LOCAL, { sid: "wfirst", path: "/p/wfirst.jsonl", seq: 2000 });
    for (let k = 0; k < 5; k++) await settle();
    const t = home(tm).store.tabs.get("wfirst")!;
    expect(t.window.floorSeq, "更早那一截没回，视口那一截也没物化（还在等整份尾段）").not.toBeNull();
    expect(t.seenSeqs.has(END - 1), "视口那一截没取到末尾").toBe(true);
    expect(lineReads().some((r) => r.from === 1400), "更早那一截没去取").toBe(true);
  });

  it("连着的不取；别的那份记录（路径对不上）· 别的机器上的同名会话不归这个 tab", async () => {
    answerLines();
    for (let s = 0; s < 10; s++) tm.onLine(line("wc", s));
    tm.onSessionWatch(LOCAL, { sid: "wc", path: "/p/wc.jsonl", seq: 10 });
    tm.onSessionWatch(LOCAL, { sid: "wc", path: "/q/other.jsonl", seq: 40 });
    tm.onSessionWatch("box", { sid: "wc", path: "/p/wc.jsonl", seq: 40 });
    await settle();
    expect(lineReads()).toEqual([]);
  });

  it("Codex 样本 ⇒ 它那个 tab 的活动跟着一轮走：一轮开始在跑、一轮完了空闲（帧上那几格照抄）", () => {
    type F = { kind: string; sid?: string; path?: string; face?: SessionFace };
    const frames = readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/codex-session-stream.golden.jsonl"), "utf8")
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l) as F);
    const announced = frames.find((f) => f.kind === "session_added")!;
    const codexSid = announced.sid!;
    tm.ensureTab(codexSid, null, announced.path!, LOCAL);
    const seen: (string | null)[] = [];
    for (const f of frames.filter((x) => x.kind === "session_added" || x.kind === "session_status")) {
      tm.onFace(codexSid, f.face!);
      const live = home(tm).store.tabs.get(codexSid)!.face!.life;
      const doing = live.is === "live" ? (live.activity ?? null) : null;
      if (seen[seen.length - 1] !== doing) seen.push(doing);
    }
    // 宣告不等记录（这一家没有进程状态文件 ⇒ 宣告那一帧判不出，照实写「读记录中」、不先说「运行中」）；读完记录紧跟一帧说闲着。
    expect(seen).toEqual([null, "idle", "working", "idle", "working", "idle"]);
    expect(announced.face!.name).toBe(copyText("beSession.activity.reading"));
    const last = home(tm).store.tabs.get(codexSid)!.face!;
    expect([last.name, last.tone]).toEqual([copyText("beSession.activity.idle"), "plain"]);
  });
});

describe("SeqSet.maxBelow：见过的、比 x 小的最大行号", () => {
  it("落在段里 ⇒ x − 1；落在洞里 ⇒ 前一段的末尾；之前什么都没有 ⇒ -1", async () => {
    const { SeqSet } = await import("../../../src/frontend/ui/live-window");
    const set = new SeqSet();
    expect(set.maxBelow(5)).toBe(-1);
    set.addRange(0, 10);
    set.addRange(50, 60);
    expect(set.maxBelow(5)).toBe(4);
    expect(set.maxBelow(10)).toBe(9);
    expect(set.maxBelow(30)).toBe(9);
    expect(set.maxBelow(50)).toBe(9);
    expect(set.maxBelow(55)).toBe(54);
    expect(set.maxBelow(100)).toBe(59);
    expect(set.maxBelow(0)).toBe(-1);
  });
});

// 叫会话什么是核心的账（`session_label` · `session_line`）：标签页照抄会话帧上的名字 `label` 与写好的整行 `title`，不排版。
// 不在看的会话改了名，那一格照样来、标签页照样跟着变；标题记录不再喂标签页。
describe("标签页的名字照抄核心那一格", () => {
  it("名字先于 tab 到 ⇒ 建 tab 时用上；之后改名 ⇒ 跟着变；整行原样（远端也不加机器名）", async () => {
    document.body.innerHTML = "";
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(announced(new TabManager(barEl, streamRootEl, () => {})));
    tm.updateName("n1", "修一下登录", "proj 修一下登录");
    tm.ensureTab("n1", "/w/proj", "/p/n1.jsonl", LOCAL);
    const tab = home(tm).store.tabs.get("n1")!;
    expect(tab.title).toBe("proj 修一下登录");
    expect(tab.label).toBe("修一下登录");
    tm.updateName("n1", "改", "proj ↳ 改");
    expect(tab.title).toBe("proj ↳ 改");
    tm.ensureTab("n2", "/w/q", "/p/n2.jsonl", "box");
    tm.updateName("n2", "登录修复", "q 登录修复");
    expect(home(tm).store.tabs.get("n2")!.title).toBe("q 登录修复");
  });

  // 会话头 · 窗口标题跟「当前 tab」那一格刷：当前那条改了名也得通知（原先那一格不带名字 ⇒ 同值不通知，会话头停在
  // 「没有说过话的会话」，标签页那一行却已经是第一句）。别的 tab 改名不惊动它。
  it("当前 tab 改了名 ⇒ 「当前 tab」那一格通知一次、带上新标题；别的 tab 改名不通知", async () => {
    document.body.innerHTML = "";
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(announced(new TabManager(barEl, streamRootEl, () => {})));
    tm.ensureTab("a1", "/w/proj", "/p/a1.jsonl", LOCAL);
    tm.ensureTab("a2", "/w/proj", "/p/a2.jsonl", LOCAL);
    tm.switchTo("a1");
    await vi.waitFor(() => expect(tm.active.get().sid).toBe("a1"), UNTIL_CEILING);
    const seen: Array<string | null> = [];
    tm.active.subscribe((a) => seen.push(a.title));
    tm.updateName("a2", "别的那条", "proj 别的那条");
    expect(seen, "别的 tab 改名惊动了当前那一格").toEqual([]);
    tm.updateName("a1", "RT1 第一句", "proj RT1 第一句");
    expect(seen).toEqual([home(tm).store.tabs.get("a1")!.title]);
    expect(seen[0]).toContain("RT1 第一句");
  });
});

// ==========================================================================
// 组画成一块。块边说「在组里」，拖的时候「进组」点亮的是那一块、
// 插入线画在块里面；组外的线画在块外。窄栏组员的悬停卡首行带组名（I8）。
// ==========================================================================
describe("ui3 · 组成块：落点反馈看块、窄栏悬停带组名", () => {
  let tm: TabManager;
  const parts = (gid: string): { wrap: HTMLElement; head: HTMLElement; list: HTMLElement } => home(tm).bar.groupParts(gid)!;
  const rect = (el: HTMLElement, left: number, right: number, top = 0, height = 0): void => {
    el.getBoundingClientRect = (): DOMRect => ({ left, right, top, height, bottom: top + height, width: right - left }) as DOMRect;
  };
  const mark = (t: DropTarget | null): void => (home(tm).dragger as unknown as { mark(t: DropTarget | null): void }).mark(t);
  const line = (): HTMLElement => document.body.querySelector<HTMLElement>(`.${tg.dropLine}`)!;
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    tm = makeTM();
    home(tm).prefs.collectionsLoaded = true;
    tm.ensureTab("a", "/c1", "p", LOCAL);
    tm.ensureTab("b", "/c2", "p", LOCAL);
    tm.ensureTab("c", "/c3", "p", LOCAL);
    setGroups(tm, [{ id: "g1", name: "订单", tabs: ["b", "c"] }]);
    (tm as unknown as { refreshTabBar: () => void }).refreshTabBar();
    rect(home(tm).bar.listEl, 0, 260, 0, 400);
    rect(parts("g1").wrap, 6, 254);
    // 起拖（a），线才建出来。
    home(tm).bar.tabButtons.get("a")!.root.dispatchEvent(new MouseEvent("mousedown", { button: 0, buttons: 1, clientX: 0, clientY: 5, bubbles: true }));
    document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 0, clientY: 40, bubbles: true }));
  });
  afterEach(() => {
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    document.body.querySelectorAll(".tab-drag-ghost").forEach((n) => n.remove());
    delete document.body.dataset.tabBar;
  });

  it("落在组里 ⇒ 点亮的是整块（不是组员那一列）；线画在块里面", () => {
    mark(before("c", "g1"));
    expect(parts("g1").wrap.classList.contains(tg.hot), "进组要看得出：块边变 accent").toBe(true);
    expect(parts("g1").list.classList.contains(tg.hot), "引导线那一套撤了：组员那一列不再挂 hot").toBe(false);
    expect(line().hidden).toBe(false);
    expect(line().style.left, "线从块的左内边起（块左 6 ＋ 线内缩 6），不再按缩进 12 算").toBe("12px");
    expect(line().style.width, "线止于块的右内边").toBe(`${254 - 6 - 12}px`);
  });

  it("落在组外 ⇒ 哪块都不亮，线通栏", () => {
    mark(before("a"));
    expect(parts("g1").wrap.classList.contains(tg.hot)).toBe(false);
    expect(line().style.left).toBe("6px");
    expect(line().style.width).toBe(`${260 - 6 - 6}px`);
  });

  it("压在组员中间停住（进组）· 落在组头 ⇒ 块亮；压在散的上（建组）⇒ 不亮", () => {
    mark({ kind: "onto", sid: "b" });
    expect(parts("g1").wrap.classList.contains(tg.hot), "压住组员 ＝ 进这一组").toBe(true);
    mark({ kind: "insert", at: null, gid: null, head: "g1" } as DropTarget);
    expect(parts("g1").wrap.classList.contains(tg.hot), "落在组头 ＝ 进这一组").toBe(true);
    mark({ kind: "onto", sid: "a" });
    expect(parts("g1").wrap.classList.contains(tg.hot), "建新组不点亮别的块").toBe(false);
    mark(null);
    expect(parts("g1").wrap.classList.contains(tg.hot), "收尾清干净").toBe(false);
  });

  it("当前标签页在收着的组里 ⇒ 块边升一档（块上挂 has-current）；展开着不挂", () => {
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true })); // 先收掉 beforeEach 起的拖（拖拽中整刷挂起）
    tm.switchTo("b");
    const caret = (): HTMLElement => parts("g1").head.querySelector<HTMLElement>(".tab-group-caret")!;
    caret().click();
    expect(parts("g1").wrap.classList.contains("is-collapsed"), "前提：收起来了").toBe(true);
    expect(parts("g1").wrap.classList.contains("has-current")).toBe(true);
    caret().click();
    expect(parts("g1").wrap.classList.contains("has-current"), "展开着：当前行自己有 accent，块边不变色").toBe(false);
  });

  it("窄栏：组员悬停卡首行「组名 · 标题」；散的照旧；宽栏照旧", () => {
    const title = (sid: string): string => cardOf(tm, sid)!.querySelector(".tab-hover-title")!.textContent!;
    const plain = title("b");
    document.body.dataset.tabBar = "folded";
    expect(title("b")).toBe(copyText("tabBar.group.memberHint", { group: "订单", title: plain }));
    expect(title("b").startsWith("订单 · ")).toBe(true);
    expect(title("a"), "散的不带组名").toBe(home(tm).store.tabs.get("a")!.title);
    delete document.body.dataset.tabBar;
    expect(title("b"), "宽栏看得见组，不重复").toBe(plain);
  });

  it("块的样子：`--group-fill` 是 3% 白；组是淡底 ＋ 细边 ＋ 圆角 8，组员不缩进、没有引导线", () => {
    const tokens = readFileSync(resolve(REPO_ROOT, "src/shared/theme/tokens.css"), "utf8");
    expect(tokens).toMatch(/--group-fill:\s*#ffffff08;/);
    const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles.css"), "utf8");
    const rule = (sel: string): string => {
      const m = new RegExp(`\\n${sel.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")} \\{([^}]*)\\}`).exec(css);
      expect(m, `styles.css 里没有 ${sel} 这一条`).not.toBeNull();
      return m![1];
    };
    const group = rule(".tab-group");
    expect(group).toMatch(/background:\s*var\(--group-fill\)/);
    expect(group).toMatch(/border:\s*1px solid var\(--border-soft\)/);
    expect(group).toMatch(/border-radius:\s*var\(--radius-l\)/);
    const list = rule(".tab-group-list");
    expect(list, "组员不另缩进").not.toMatch(/margin-left/);
    expect(list, "引导线去掉").not.toMatch(/border-left/);
  });
});

// 8a ④：N 个 tab 冷起，到当前 tab 画出尾段、它那一趟落地之前，后端只收到当前那一个会话的冷读命令；
// 后台的冷读排在它之后、空闲时一个一个来；切进来停住的插到最前。数命令，不按墙钟。
// 冷读 ＝ 后端要从头扫的那几问：一个 tab 的头一问 `history-facts` · `history-turns` from 0 · 骨架索引 · 大纲清单。夹具只造结构（sid / 路径）。
describe("〔8a ④〕冷起只先问当前 tab：后台的冷读排在当前 tab 那一趟之后、一个一个来", () => {
  const COLD_OPS = new Set(["history-facts", "history-turns", "history-index", "history-user-inputs"]);
  const facts = (): SessionFacts => ({
    agent: "claude",
    startedBy: null,
    children: [],
    startedByText: null,
    childrenText: null,
    touchedFiles: [],
    usage: null,
    projectDir: null,
    writers: [],
    pending: [],
    lastSay: null,
    needs: null,
    retries: [],
    notices: [],
    permissionMode: null,
    permissionModeText: null,
    modelText: null,
    modelPick: null,
    tokens: null,
    background: null,
    mcp: [],
    autoAnswer: null,
    autoAnswerChoices: [],
    askEnd: null,
  });
  const line = (sid: string, seq: number) =>
    ({ session_id: sid, cwd: "/p", path: `/p/${sid}.jsonl`, seq, record: { t: "reply", id: `${sid}-${seq}` } }) as never;
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
  };
  /** 发出去的冷读：哪一问 · 哪个会话（按路径认）。 */
  let cold: { op: string; sid: string }[] = [];
  /** 问过会话事实的会话（头一问才是冷读）。 */
  let factsAsked = new Set<string>();
  /** 扣着不答的会话（它的 facts / turns 挂起，`release` 才答）。 */
  let holding = new Set<string>();
  let held: { sid: string; go: () => void }[] = [];
  let idle: IdleRequestCallback[] = [];
  /** 排过的空闲回调一共几个 · 撤掉了哪几个（编号 ＝ 排的次序）。 */
  let idleAsked = 0;
  let idleCancelled: number[] = [];
  let hadIdle: typeof window.requestIdleCallback;
  let hadCancel: typeof window.cancelIdleCallback;
  const sidOf = (path: string): string => path.replace(/^.*\//, "").replace(/\.jsonl$/, "");
  const release = (sid: string): void => {
    for (const h of held.filter((x) => x.sid === sid)) h.go();
    held = held.filter((x) => x.sid !== sid);
  };
  /** 在途的冷读各属哪个会话（发了、还没答）。 */
  const inflight = (): Set<string> => new Set(held.map((h) => h.sid));
  let tm: TabManager;
  beforeEach(() => {
    vi.clearAllMocks();
    cold = [];
    factsAsked = new Set();
    holding = new Set();
    held = [];
    idle = [];
    idleAsked = 0;
    idleCancelled = [];
    hadIdle = window.requestIdleCallback;
    hadCancel = window.cancelIdleCallback;
    window.requestIdleCallback = ((cb: IdleRequestCallback) => {
      idle.push(cb);
      return ++idleAsked;
    }) as typeof window.requestIdleCallback;
    window.cancelIdleCallback = ((id: number) => void idleCancelled.push(id)) as typeof window.cancelIdleCallback;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd !== "chan_call") return undefined;
      const a = args as { op: string };
      if (!COLD_OPS.has(a.op)) return undefined;
      const body = chanArgsJson(args as never) as { path: string; from?: number };
      const sid = sidOf(body.path);
      if (!["A", "B", "C"].includes(sid)) return undefined; // 前面几组留下的 TabManager 还在排它们的空闲回调：不算这一组的
      const isCold = a.op === "history-facts" ? !factsAsked.has(sid) : a.op === "history-turns" ? body.from === 0 : true;
      if (a.op === "history-facts") factsAsked.add(sid);
      if (isCold) cold.push({ op: a.op, sid });
      if (a.op !== "history-facts" && a.op !== "history-turns") throw UNSUPPORTED;
      const reply = chanReply(a.op === "history-facts" ? facts() : { from: 0, end: 0, turns: [] });
      if (!holding.has(sid) && !holding.has("*")) return reply;
      return new Promise((resolve) => held.push({ sid, go: () => resolve(reply) }));
    });
    tm = makeTM();
  });
  afterEach(() => {
    window.requestIdleCallback = hadIdle;
    window.cancelIdleCallback = hadCancel;
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });
  /** 冷起：一批里三个会话（A 是第一个 ⇒ 当前），每个一格红绿灯，那台推一次「需手动变了」⇒ 各 tab 再问会话事实。 */
  const coldStart = (): void => {
    tm.onBatchStart();
    for (const sid of ["A", "B", "C"]) for (let s = 0; s < 3; s++) tm.onLine(line(sid, s));
    for (const sid of ["A", "B", "C"]) tm.onFace(sid, liveFace("idle"));
    tm.rereadFactsOn(LOCAL);
    tm.onBatchEnd();
  };

  it("当前 tab 那一趟落地之前，冷读只有当前那一个会话的（正控：它的 facts 与 turns 都已发）", async () => {
    holding = new Set(["*"]);
    coldStart();
    await settle();
    expect(home(tm).store.activeId).toBe("A");
    expect(home(tm).store.tabs.get("A")!.window.floorSeq, "当前 tab 尾段已物化").not.toBeNull();
    expect(cold.map((c) => c.op).sort(), "正控：当前 tab 的冷读发了").toEqual(expect.arrayContaining(["history-facts", "history-turns"]));
    // 空闲回调也跑一遍：前台那一趟没落地，空闲时也不发后台的。
    for (let i = 0; i < 10 && idle.length > 0; i++) idle.shift()!({ didTimeout: false, timeRemaining: () => 50 });
    await settle();
    expect([...new Set(cold.map((c) => c.sid))], "后端只收到当前那个会话的冷读").toEqual(["A"]);
  });

  it("当前那一趟落地 ⇒ 后台空闲时一个一个来：任一时刻在途的后台冷读只属一个会话", async () => {
    holding = new Set(["B", "C"]); // 当前那一趟照答（落地），后台的扣着、一个一个放
    coldStart();
    await settle();
    const seen: string[] = [];
    for (let round = 0; round < 10; round++) {
      for (let i = 0; i < 10 && idle.length > 0; i++) idle.shift()!({ didTimeout: false, timeRemaining: () => 50 });
      await settle();
      const now = [...inflight()];
      expect(now.length, `第 ${round} 轮在途的会话：${now.join(",")}`).toBeLessThanOrEqual(1);
      if (now.length === 0) break;
      seen.push(now[0]);
      release(now[0]);
      await settle();
    }
    expect(seen, "B · C 都轮到了，一个一个").toEqual(["B", "C"]);
    expect(cold.filter((c) => c.op === "history-facts").map((c) => c.sid), "每个会话只冷读一次 facts").toEqual(["A", "B", "C"]);
  });

  it("当前那一趟还扣着时切到 C、停住 ⇒ C 的冷读先于排在前面的 B 发出", async () => {
    holding = new Set(["A", "B", "C"]);
    coldStart();
    await settle();
    tm.switchTo("C");
    // 停住了（`STAY_MS`）C 的第一趟会话事实才插到最前：等它真发出去。B 只能经空闲回调轮到，这里的空闲回调不自己跑 ⇒ 之后再判「B 还在排着」是确定的。
    await vi.waitFor(() => expect(cold.some((c) => c.sid === "C" && c.op === "history-facts"), "停住了 C 的会话事实没插到前面").toBe(true), UNTIL_CEILING);
    await settle();
    const sids = [...new Set(cold.map((c) => c.sid))];
    expect(sids.slice(0, 2), "C 插到 B 前面").toEqual(["A", "C"]);
    expect(sids, "B 还在排着").not.toContain("B");
  });
  // 主人拆掉之后才回来的读（测试拆了环境、窗口关了）：不许再排下一趟、不许碰 window；挂着的空闲回调当场撤掉。
  it("主人拆掉以后，在途的冷读再回来 ⇒ 不再排、不碰 window；挂着的空闲回调撤掉", async () => {
    holding = new Set(["B", "C"]); // 当前那一趟照答；后台的扣着
    coldStart();
    await settle();
    for (let i = 0; i < 10 && idle.length > 0 && cold.every((c) => c.sid !== "B"); i++) idle.shift()!({ didTimeout: false, timeRemaining: () => 50 });
    await settle();
    expect([...inflight()], "前提：B 那一趟在途").toEqual(["B"]);
    const pendingIds = Array.from({ length: idle.length }, (_, k) => idleAsked - idle.length + 1 + k);
    tm.dispose();
    expect(idleCancelled.sort(), "挂着的空闲回调（冷读排班那几个）当场撤掉").toEqual(pendingIds.sort());
    const askedAtDispose = idleAsked;
    const coldAtDispose = cold.length;
    // 拆掉以后 window 上的空闲回调一碰就记一笔（测试环境拆掉之后 window 已经不在：碰了就是 ReferenceError）。
    let touched = 0;
    window.requestIdleCallback = (() => ++touched) as typeof window.requestIdleCallback;
    release("B");
    await settle();
    expect(touched, "回来以后又去排下一趟（碰了 window）").toBe(0);
    expect(idleAsked).toBe(askedAtDispose);
    expect(cold.length, "拆掉以后不再发新的冷读").toBe(coldAtDispose);
  });
});

describe("那台握手时说过做不到的，菜单置灰并说为什么", () => {
  it("［结束］（`sessions-stop`）在「没有 tmux」的那台上不可点、字后面带原因；那台没说的项与没问过的机器照常", async () => {
    const { gateByOffer } = await import("../../../src/frontend/ui/tab-menu");
    const { chan } = await import("../../../src/comms/inward/chan");
    vi.mocked(invoke).mockImplementation(((cmd: string, args: { origin?: string }) =>
      Promise.resolve(
        cmd === "chan_offer" && args.origin === "net2-box"
          ? { ops: ["sessions-stop", "terminal-preview"], unavailable: [["sessions-stop", "no_tmux", "net2-box 无 tmux"]] }
          : null,
      )) as never);
    await chan.offer("net2-box");
    const click = vi.fn();
    const end = gateByOffer("net2-box", { id: "end", label: copyText("tabMenu.kill.label"), onClick: click });
    expect(end.enabled).toBe(false);
    expect(end.why, "第二行写为什么（不拼进项名）").toBe(copyText("control.unavailable.noTmux", { machine: "net2-box" }));
    expect(end.label).toBe(copyText("tabMenu.kill.label"));
    end.onClick?.();
    expect(click).not.toHaveBeenCalled();
    const preview = { id: "preview", label: "预览", onClick: click };
    expect(gateByOffer("net2-box", preview)).toBe(preview);
    const elsewhere = { id: "end", label: copyText("tabMenu.kill.label"), onClick: click };
    expect(gateByOffer("net2-other", elsewhere)).toBe(elsewhere);
    vi.mocked(invoke).mockResolvedValue(undefined);
  });
});
