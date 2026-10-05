// 主窗口的几处交互逻辑：输入法组字 · 弹层栈挡单键 · 确认框焦点 · 子 agent 时间线 · agent 面板原地更新。
// 行为口径：单键只在上面没有浮层 / 对话框 / 输入焦点时生效、模态只放行 Esc；组字中的 Enter 归输入法；
// 危险确认默认焦点在「取消」；实时更新不夺焦点、读失败不丢已显示的。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
// TabManager 的重协作者换成空壳（流 · 时间线 · 分支折叠）；流记着「贴没贴底」与被强制贴底了几次。
const streams = vi.hoisted(() => ({ all: [] as { stuck: boolean; scrolled: number }[] }));
vi.mock("../../../src/frontend/ui/stream", () => ({
  MessageStream: class {
    contentElement = document.createElement("div");
    trailerElement = document.createElement("div");
    stuck = true;
    scrolled = 0;
    constructor(_root: HTMLElement) {
      streams.all.push(this);
    }
    get stuckToBottom(): boolean {
      return this.stuck;
    }
    insertNode(): void {}
    batchInsert(fn: () => void): void {
      fn();
    }
    scrollToBottom(): void {
      this.stuck = true;
      this.scrolled++;
    }
    dispose(): void {}
  },
}));
vi.mock("../../../src/frontend/ui/record-timeline", () => ({
  RecordTimeline: class {
    _seqs = new Set<number>();
    _maxSeq = -Infinity;
    constructor(_s: unknown) {}
    insert(e: { seq: number }): void {
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
    constructor(_e: unknown) {}
    setBatchMode(): void {}
    flushPending(): void {}
    recordAdded(): void {}
    unwrapAll(): void {}
    rebuildNow(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../../src/frontend/ui/tasks-panel", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/tasks-panel")>()),
  fetchSessionTasks: vi.fn().mockResolvedValue([]),
}));
vi.mock("../../../src/frontend/ui/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
// 子运行的记录按页读：判据换成假的那一页（`pages.load(from)`）。
const pages = vi.hoisted(() => ({ load: null as null | ((from: number) => Promise<unknown>), calls: 0 }));
vi.mock("../../../src/frontend/ui/record-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/record-reads")>()),
  loadRunPage: (_o: unknown, _p: unknown, _w: unknown, from = 0) => {
    pages.calls++;
    return pages.load!(from);
  },
}));

import { CommandBarView } from "../../../src/frontend/ui/views/command-bar";
import { SessionFindPanel } from "../../../src/frontend/ui/views/session-find";
import { dispatcher, type OverlayHandle } from "../../../src/frontend/ui/keybindings/registry";
import { confirmDialog } from "../../../src/frontend/ui/kit/dialog";
import { TabManager, type Tab } from "../../../src/frontend/ui/tabs";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ENDED, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import { invoke } from "@tauri-apps/api/core";
import type { TabStore } from "../../../src/frontend/ui/tab-store";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { RunTimeline } from "../../../src/frontend/ui/run-timeline";
import { buildAgentCard, markRunCard } from "../../../src/frontend/ui/cards/subagent";
import { AgentsPanel } from "../../../src/frontend/ui/agents-panel";
import { applyFacts } from "../../../src/frontend/ui/tab-session-facts";
import type { SessionFacts } from "../../../src/frontend/ui/session-reads";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";

/** 读 TabManager 的几处内部状态（判据只读，不经它改）。 */
type Inside = { store: TabStore; prefs: TabBarPrefs };
const inside = (tm: TabManager): Inside => tm as unknown as Inside;

function makeTabs(sids: readonly string[]): TabManager {
  const bar = document.createElement("div");
  const root = document.createElement("div");
  document.body.append(bar, root);
  const tm = new TabManager(bar, root);
  for (const sid of sids) tm.ensureTab(sid, `/w/${sid}`, `/p/${sid}.jsonl`, LOCAL_ORIGIN);
  return tm;
}

function key(target: EventTarget, init: KeyboardEventInit): KeyboardEvent {
  const e = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e;
}

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("输入法组字时 Enter 归输入法", () => {
  it("命令面板：组字中的 Enter 不执行命令，组完的 Enter 才执行", () => {
    let ran = 0;
    const bar = new CommandBarView(() => [{ id: "x", title: "切到会话", run: () => ran++ }]);
    bar.open();
    const input = document.querySelector<HTMLInputElement>(".command-bar-input")!;
    key(input, { key: "Enter", isComposing: true });
    key(input, { key: "Enter", keyCode: 229 } as KeyboardEventInit);
    expect(ran).toBe(0);
    key(input, { key: "Enter" });
    expect(ran).toBe(1);
  });

  it("会话内查找：组字中的 Enter 不发查询", async () => {
    const search = vi.fn().mockResolvedValue({ available: true, reason: "", hits: [], total: 0 });
    const p = new SessionFindPanel({ search, jumpTo: () => null, unjumpableHint: "" });
    document.body.appendChild(p.el);
    p.open("search");
    const input = p.el.querySelector<HTMLInputElement>("input[type=search]")!;
    input.value = "qie";
    key(input, { key: "Enter", isComposing: true });
    await Promise.resolve();
    expect(search).not.toHaveBeenCalled();
    input.value = "切到";
    key(input, { key: "Enter" });
    await Promise.resolve();
    expect(search).toHaveBeenCalledTimes(1);
    p.close(); // 出弹层栈：别把一层看不见的面板留给后面的判据
  });
});

describe("弹层栈：上面有浮层 / 对话框时单键不落到底下的 tab 上", () => {
  const hits: string[] = [];
  const press = (code: string, mods: KeyboardEventInit = {}): void => {
    const t = document.activeElement ?? document.body;
    key(t, { code, key: code, ...mods });
  };
  beforeEach(() => {
    hits.length = 0;
    dispatcher.applyOverrides({});
    dispatcher.bind("tab.close-archived", () => hits.push("W"));
    dispatcher.bind("tab.jump-1", () => hits.push("1"));
    dispatcher.bind("app.toggle-history", () => hits.push("H"));
    dispatcher.bind("app.open-command-bar", () => hits.push("Ctrl+K"));
    dispatcher.start();
  });

  it("确认框（模态）开着：W / 1 / Ctrl+K 都不生效，只有 Esc；关掉之后照常", async () => {
    const p = confirmDialog({ title: "结束会话 x", action: "结束", danger: true });
    press("KeyW");
    press("Digit1");
    press("KeyK", { ctrlKey: true });
    expect(hits).toEqual([]);
    press("Escape");
    await expect(p).resolves.toBe(false);
    press("KeyW");
    expect(hits).toEqual(["W"]);
  });

  it("全屏视图（非模态）开着：单键不生效、带修饰键的照常、它自己声明的键放行；多选那一层不拦", () => {
    const full: OverlayHandle & { passes: readonly "app.toggle-history"[] } = {
      handleEsc: () => true,
      passes: ["app.toggle-history"],
    };
    dispatcher.pushOverlay(full);
    press("KeyW");
    press("Digit1");
    press("KeyK", { ctrlKey: true });
    press("KeyH");
    expect(hits).toEqual(["Ctrl+K", "H"]);
    dispatcher.popOverlay(full);
    hits.length = 0;
    const selection: OverlayHandle = { handleEsc: () => true, passes: "all" };
    dispatcher.pushOverlay(selection);
    press("Digit1");
    expect(hits).toEqual(["1"]);
    dispatcher.popOverlay(selection);
  });

  it("Esc 一次只关最上一层", () => {
    const closed: string[] = [];
    const a: OverlayHandle = { handleEsc: () => void closed.push("a") };
    const b: OverlayHandle = { handleEsc: () => void closed.push("b") };
    dispatcher.pushOverlay(a);
    dispatcher.pushOverlay(b);
    press("Escape");
    expect(closed).toEqual(["b"]);
    dispatcher.popOverlay(a);
    dispatcher.popOverlay(b);
  });
});

describe("W 关掉已结束的当前 tab：给 8 秒撤销，撤销 ⇒ 原位 · 原固定放回", () => {
  it("撤销放回原位并切回它；不撤销 ⇒ 到点才真关（取消固定）", () => {
    vi.useFakeTimers();
    try {
      const tm = makeTabs(["a", "b", "c"]);
      const st = inside(tm).store;
      const b = st.tabs.get("b") as Tab;
      b.state = ENDED;
      b.pinned = true;
      tm.switchTo("b");
      tm.closeActiveIfArchived();
      expect(st.orderedIds).toEqual(["a", "c"]);
      const toast = [...document.querySelectorAll("#kit-toast-stack > *")].find((t) =>
        t.textContent?.includes(copyText("tabBar.close.doneUnpinned", { title: b.title })),
      );
      expect(toast, "关了要说一句、带撤销").toBeTruthy();
      toast!.querySelector<HTMLButtonElement>("button")!.click();
      expect(st.orderedIds).toEqual(["a", "b", "c"]);
      expect(st.activeId).toBe("b");
      expect((st.tabs.get("b") as Tab).pinned).toBe(true);

      tm.closeActiveIfArchived();
      expect(b.pinned, "撤销期里还没真关").toBe(true);
      vi.advanceTimersByTime(8000);
      expect(st.tabs.has("b")).toBe(false);
      expect(b.pinned, "到点 ⇒ 关闭做完（关掉 = 取消固定）").toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("有分组时数字键 / ] [ / 关掉当前 tab 后的落点都按条上看到的顺序", () => {
  function grouped(): TabManager {
    const tm = makeTabs(["a", "b", "c", "d", "e"]);
    const { store, prefs } = inside(tm);
    prefs.collections = [{ id: "g", name: "G" }];
    (store.tabs.get("c") as Tab).group = "g";
    (store.tabs.get("d") as Tab).group = "g";
    return tm;
  }

  it("条上 c d a b e：1 ⇒ c；b 上 ] ⇒ e；d 上 ] ⇒ a；a 上 [ ⇒ d", () => {
    const tm = grouped();
    const st = inside(tm).store;
    tm.switchTo("e");
    tm.jumpToIndex(1);
    expect(st.activeId).toBe("c");
    tm.switchTo("b");
    tm.cycleActive(1);
    expect(st.activeId).toBe("e");
    tm.switchTo("d");
    tm.cycleActive(1);
    expect(st.activeId).toBe("a");
    tm.cycleActive(-1);
    expect(st.activeId).toBe("d");
  });

  it("关掉当前的 d ⇒ 落到条上它后面的 a", () => {
    const tm = grouped();
    const st = inside(tm).store;
    tm.switchTo("d");
    (st.tabs.get("d") as Tab).state = ENDED;
    tm.closeTab("d");
    expect(st.activeId).toBe("a");
  });
});

describe("远端还没到时拖动顺序，没到的 tab 在盘上的位置留着", () => {
  it("盘上 x a b c（x 还没到），拖成 b a c ⇒ 落盘 x b a c；关掉的 a 才从盘上摘掉", async () => {
    const tm = makeTabs(["a", "b", "c"]);
    const { store, prefs } = inside(tm);
    store.savedOrder = ["x", "a", "y", "b", "c"];
    store.orderedIds = ["b", "a", "c"];
    await prefs.persistOrder();
    expect(store.savedOrder).toEqual(["x", "b", "a", "y", "c"]);
    (store.tabs.get("a") as Tab).state = ENDED;
    tm.closeTab("a");
    await prefs.persistOrder();
    expect(store.savedOrder).toEqual(["x", "b", "y", "c"]);
  });
});

describe("说不清（那台暂时看不见）的 tab 不给恢复", () => {
  it("右键菜单里恢复置灰并说为什么；直接调恢复也不起，说一句", async () => {
    const tm = makeTabs(["u"]);
    const st = inside(tm).store;
    (st.tabs.get("u") as Tab).state = UNSEEN;
    (tm as unknown as { menu: { open(e: MouseEvent, sid: string): void } }).menu.open(
      new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }),
      "u",
    );
    const items = [...document.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")];
    const resume = items.filter((b) => b.textContent?.startsWith(copyText("tabMenu.item.resume")));
    expect(resume.length, "恢复那一项还在（置灰、说为什么）").toBe(1);
    expect(resume[0].disabled).toBe(true);
    expect(resume[0].textContent).toContain(copyText("sessionState.unseen.tooltip"));
    const actions = (tm as unknown as { actions: { resumeTab(sid: string): Promise<void> } }).actions;
    await actions.resumeTab("u");
    const said = [...document.querySelectorAll("#kit-toast-stack > *")].some((t) =>
      t.textContent?.includes(copyText("sessionState.unseen.noResume")),
    );
    expect(said, "点了要有回应：说不清 ⇒ 不起、说一句").toBe(true);
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => String(cmd).includes("launch")), "不许起本机 claude").toBe(false);
  });
});

describe("重启后各台都报完了、组员一个都没回来的组不留空组头", () => {
  it("没报完之前留着（组员可能还在路上）；各台都报完 ⇒ 没回来的收掉、空组消失，有人在的组留着", () => {
    const tm = makeTabs(["a"]);
    const { store, prefs } = inside(tm);
    prefs.collectionsLoaded = true;
    prefs.collections = [
      { id: "g", name: "已结束的那组" },
      { id: "h", name: "还有人" },
    ];
    prefs.savedGroupOf = new Map([
      ["gone", "g"],
      ["late", "h"],
    ]);
    (store.tabs.get("a") as Tab).group = "h";
    tm.expectMachines([LOCAL_ORIGIN, "pi"]);
    tm.markOriginSeen(LOCAL_ORIGIN);
    expect(prefs.collections.map((c) => c.id)).toEqual(["g", "h"]);
    tm.markOriginSeen("pi" as never);
    expect(prefs.collections.map((c) => c.id)).toEqual(["h"]);
    expect([...prefs.savedGroupOf]).toEqual([]);
    expect(document.querySelectorAll(".tab-group")).toHaveLength(1);
  });
});

describe("tab 的悬停提示第一行是标题", () => {
  it("活着的普通 tab 也有提示（标题全名），已结束的标题之后再说状态", () => {
    const tm = makeTabs(["a", "b"]);
    const st = inside(tm).store;
    (st.tabs.get("b") as Tab).state = ENDED;
    tm.switchTo("b");
    tm.switchTo("a");
    const tips = [...document.querySelectorAll<HTMLElement>(".tab")].map((el) => el.title.split("\n"));
    expect(tips.map((t) => t[0])).toEqual([(st.tabs.get("a") as Tab).title, (st.tabs.get("b") as Tab).title]);
    expect(tips[1][1]).toBe(copyText("sessionState.ended.tooltip"));
  });
});

/** 一页子运行记录：从 `from` 起 `n` 条，每条渲染成写着自己序号的一张卡。 */
function page(from: number, n: number): unknown {
  return {
    run: "r",
    path: "/p/r.jsonl",
    end: from + n,
    more: false,
    rows: Array.from({ length: n }, (_, i) => ({ message: { i: from + i } })),
  };
}
const renderNum = (rec: unknown): { kind: "card"; element: HTMLElement } => {
  const el = document.createElement("div");
  el.className = "n";
  el.textContent = String((rec as { i: number }).i);
  return { kind: "card", element: el };
};
const nums = (root: HTMLElement): string[] => [...root.querySelectorAll(".n")].map((e) => e.textContent ?? "");

describe("子 agent 的时间线：读失败一次不丢之前显示过的", () => {
  it("读到 0 1 2 → 读失败（尾巴上一句错）→ 再读成 3 4 ⇒ 0..4 都在、错那句摘掉", async () => {
    let call = 0;
    const load = async (from: number) => {
      call++;
      if (call === 2) throw new Error("瞬时失败");
      return page(from, from === 0 ? 3 : 2) as never;
    };
    const t = new RunTimeline({ origin: LOCAL_ORIGIN, parent: "x", which: { run: "r" }, load, render: renderNum });
    await t.refresh();
    await t.refresh();
    expect(nums(t.body)).toEqual(["0", "1", "2"]);
    expect(t.body.querySelector(".block-agent-error")).not.toBeNull();
    await t.refresh();
    expect(nums(t.body)).toEqual(["0", "1", "2", "3", "4"]);
    expect(t.body.querySelector(".block-agent-error")).toBeNull();
  });
});

describe("消息流里的子 agent 卡：展开之后跟着长、读失败能再读", () => {
  it("运行表每来一帧（在跑）续读一次；收起再展开也续读；第一次读失败之后能接上", async () => {
    pages.calls = 0;
    let n = 0;
    pages.load = async (from: number) => {
      n++;
      if (n === 1) throw new Error("瞬时失败");
      return page(from, 1);
    };
    const ctx = {
      parentPath: "/p/s.jsonl",
      origin: LOCAL_ORIGIN,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
    };
    const card = buildAgentCard("tool1", "Task", undefined, ctx as never, renderNum as never) as HTMLDetailsElement;
    document.body.appendChild(card);
    const settle = () => new Promise((r) => setTimeout(r, 0));
    card.open = true; // 展开（`toggle` 由 DOM 自己发）
    await settle();
    await settle();
    expect(card.querySelector(".block-agent-error"), "第一次读失败 ⇒ 说一句").not.toBeNull();
    markRunCard(card, "r", "running"); // 运行表到了：续读（也就是重试）
    await settle();
    expect(nums(card)).toEqual(["0"]);
    markRunCard(card, "r", "running"); // 又一帧
    await settle();
    expect(nums(card)).toEqual(["0", "1"]);
    card.open = false;
    await settle();
    card.open = true; // 再展开：续读一次
    await settle();
    await settle();
    expect(nums(card)).toEqual(["0", "1", "2"]);
  });
});

describe("agent 面板：运行表每来一帧不整表重建", () => {
  it("同一份再来 ⇒ 一个节点都不换；状态变了 ⇒ 行原地改字，键盘焦点与展开的时间线留着", () => {
    localStorage.setItem("cc-monitor.agentsPanelCollapsed", "0");
    const timelines = new Map<string, HTMLElement>();
    const p = new AgentsPanel();
    p.host = {
      timeline: (_sid, run) => {
        let el = timelines.get(run);
        if (!el) timelines.set(run, (el = document.createElement("div")));
        return el;
      },
      closed: () => {},
      liveOf: () => null,
    };
    document.body.append(p.summaryElement, p.popoverElement);
    const run = (id: string, state: string): RunInfo => ({ run: id, state, label: id, kind: "Explore", tool: `t-${id}` }) as never;
    p.setSession("s", [run("r1", "running"), run("r2", "running")]);
    if (p.popoverElement.style.display === "none") p.summaryElement.click();
    const rowOf = (id: string) => p.popoverElement.querySelector<HTMLElement>(`.agent-row[data-run="${id}"]`)!;
    rowOf("r2").click(); // 展开 r2 的时间线
    const r1 = rowOf("r1");
    r1.focus();
    const tl = timelines.get("r2")!;
    p.setSession("s", [run("r1", "running"), run("r2", "running")]);
    expect(rowOf("r1")).toBe(r1);
    expect(document.activeElement).toBe(r1);
    p.setSession("s", [run("r1", "done"), run("r2", "running")]);
    expect(rowOf("r1"), "状态变了：同一个行节点原地改").toBe(r1);
    expect(document.activeElement, "键盘焦点还在那一行").toBe(r1);
    expect(r1.classList.contains("agent-done")).toBe(true);
    expect(tl.isConnected, "展开着的时间线留在原处").toBe(true);
  });
});

describe("切走再切回一个 tab：回到离开时的位置", () => {
  it("往上翻着看的不拽到底；切走时贴着底的照样贴底", async () => {
    const tm = makeTabs(["a", "b"]);
    const st = inside(tm).store;
    const sa = st.tabs.get("a")!.stream as unknown as { stuck: boolean; scrolled: number };
    const sb = st.tabs.get("b")!.stream as unknown as { stuck: boolean; scrolled: number };
    const frames = () => new Promise((r) => setTimeout(r, 80));
    tm.switchTo("b");
    await frames();
    sa.stuck = false; // a 里往上翻着
    const a0 = sa.scrolled;
    const b0 = sb.scrolled;
    tm.switchTo("a");
    await frames();
    expect(sa.scrolled, "往上翻着的不许被拽到底").toBe(a0);
    tm.switchTo("b");
    await frames();
    expect(sb.scrolled, "贴着底的照样贴底").toBeGreaterThan(b0);
  });
});

describe("上下文占用：状态栏与监控板读同一个上限（后端定的）", () => {
  const facts = (limit: number, limitFrom: "relay" | "assumed"): SessionFacts => ({
    end: 1,
    forkedFrom: null,
    touchedFiles: [],
    projectDir: null,
    writers: [],
    usage: { promptTokens: 350_000, model: "claude-opus-5-5", peakPromptTokens: 350_000, limit, limitFrom },
  });
  it("中转说是 1M：35%（不是 175%）；状态栏与监控板同一个数", () => {
    const tm = makeTabs(["a"]);
    const t = inside(tm).store.tabs.get("a") as Tab;
    applyFacts(t, facts(1_000_000, "relay"));
    expect(tm.snapshotSessions()[0].contextPct).toBe(35);
    (tm as unknown as { publishActive(): void }).publishActive();
    expect(tm.active.get().contextLimit).toBe(1_000_000);
  });
  it("判不出（assumed）：不出百分比，只交用了多少", () => {
    const tm = makeTabs(["a"]);
    const t = inside(tm).store.tabs.get("a") as Tab;
    applyFacts(t, facts(1_000_000, "assumed"));
    const snap = tm.snapshotSessions()[0];
    expect([snap.contextPct, snap.contextTokens]).toEqual([null, 350_000]);
    (tm as unknown as { publishActive(): void }).publishActive();
    expect(tm.active.get().contextLimit).toBeNull();
  });
});
