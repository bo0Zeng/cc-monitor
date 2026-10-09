// 主窗口剩下的几处行为：状态栏两块浮层（任务 · 子 agent）· tab 右键菜单（躲窗口边 · 关得掉已结束的 · 说不清的不当已结束）·
// 顶栏远端文件选单 · 命令面板里的机器名 · 工具组收着时说出失败与子 agent · 命令卡展开显示命令本身。
// 行为口径：同一时刻同一层只开一块；看得见 ⇔ 在 Esc 弹层栈上，Esc 一次只关最上一层；浮层躲窗口边、贴边内缩 8px；
// 收起后看不见的动作在右键菜单里有；「说不清」是一个单独的状态，不并进「已结束」。
import { applyRetries, buildApiErrorCard, buildApiRetryCard, mergeRetry } from "../../../src/frontend/ui/cards/api-error";
import { buildStepLine, fmtStepDur, middleEllipsis, paintWaiting, settleStepLine, stateOf, stepRight } from "../../../src/frontend/ui/cards/step-line";
import { applyHandedBack, mergeNotice } from "../../../src/frontend/ui/cards/speaker-bar";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
// TabManager 的重协作者换成空壳（流 · 时间线 · 分支折叠）。
vi.mock("../../../src/frontend/ui/stream", () => ({
  MessageStream: class {
    contentElement = document.createElement("div");
    park(): void {}
    trailerElement = document.createElement("div");
    get stuckToBottom(): boolean {
      return true;
    }
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
// 顶栏远端文件：两台可去的机器；开文件窗口记一笔。
const files = vi.hoisted(() => ({ opened: [] as string[] }));
vi.mock("../../../src/frontend/ui/remote-config", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/remote-config")>()),
  readRemoteConfig: () =>
    Promise.resolve({
      enabled: true,
      hosts: ["pi", "gpu"].map((label) => ({ label, host: label, port: 22, user: "u", keyPath: "", hostKeyFingerprint: "", addresses: [], jump: "", resumeCommand: "" })),
    }),
}));
vi.mock("../../../src/frontend/ui/file-window", () => ({
  openFileWindow: (h: { label: string }) => {
    files.opened.push(h.label);
    return Promise.resolve();
  },
}));

// 行为开关读写换成内存里的一份；广播记一笔。
const behavior = vi.hoisted(() => ({ now: {} as Record<string, unknown>, saved: [] as Record<string, unknown>[] }));
vi.mock("../../../src/frontend/ui/behavior", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/behavior")>()),
  getBehavior: () => Promise.resolve({ ...behavior.now }),
  setBehavior: (b: Record<string, unknown>) => {
    behavior.saved.push(b);
    behavior.now = b;
    return Promise.resolve();
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));

import { emit } from "@tauri-apps/api/event";
import { flipBehavior } from "../../../src/frontend/ui/behavior-toggle";
import { dispatcher, type OverlayHandle } from "../../../src/frontend/ui/keybindings/registry";
import { TasksPanel, type TaskEntry } from "../../../src/frontend/ui/tasks-panel";
import { AgentsPanel } from "../../../src/frontend/ui/agents-panel";
import { MainDrawer } from "../../../src/frontend/ui/main-drawer";
import { TerminalPage, type TerminalReads } from "../../../src/frontend/ui/terminal-page";
import { closeMenu, openMenu } from "../../../src/frontend/ui/kit/menu";
import { TabManager, type Tab } from "../../../src/frontend/ui/tabs";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ENDED, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import type { TabStore } from "../../../src/frontend/ui/tab-store";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { toggleSftpFromTopbar } from "../../../src/frontend/ui/sftp-host-picker";
import { sessionCommands } from "../../../src/frontend/ui/session-commands";
import { buildToolGroup, addToToolGroup, renderMessage, type RenderContext } from "../../../src/frontend/ui/cards/index";
import type { JsonlRecord } from "../../../src/frontend/ui/generated/JsonlRecord";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import { copyPattern } from "../../test-support/copy-pattern";

const inside = (tm: TabManager): { store: TabStore } => tm as unknown as { store: TabStore };

function makeTabs(sids: readonly string[], origin: string = LOCAL_ORIGIN): TabManager {
  const bar = document.createElement("div");
  const root = document.createElement("div");
  document.body.append(bar, root);
  const tm = new TabManager(bar, root);
  for (const sid of sids) tm.ensureTab(sid, `/w/${sid}`, `/p/${sid}.jsonl`, origin as never);
  return tm;
}

function esc(): void {
  const t = document.activeElement ?? document.body;
  t.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }));
}

const shown = (el: HTMLElement): boolean => el.isConnected && el.style.display !== "none";

/** 栈底垫一层，记它收到几下 Esc（判「这一下 Esc 有没有越过上面那层」）。 */
function floor(): OverlayHandle & { hits: number } {
  const h = { hits: 0, handleEsc: () => void h.hits++ } as OverlayHandle & { hits: number };
  dispatcher.pushOverlay(h);
  return h;
}

beforeEach(() => {
  document.body.innerHTML = "";
  localStorage.clear();
  dispatcher.applyOverrides({});
  dispatcher.start();
});

const task = (id: string): TaskEntry => ({ id, subject: `任务 ${id}`, status: "pending", blocks: [], blockedBy: [] });
const run = (id: string): RunInfo => ({ run: id, state: "running", label: `子 ${id}`, kind: null, tool: null, last: null }) as unknown as RunInfo;

const NO_TERMINALS: TerminalReads = { list: async () => [], shot: async () => ({ lines: [], text: "", screen: "", atText: "" }), send: async () => ({ result: "delivered" }), follow: () => ({ stop: () => {} }) };

function mountPanels(): { tasks: TasksPanel; agents: AgentsPanel; drawer: MainDrawer } {
  const tasks = new TasksPanel();
  const agents = new AgentsPanel();
  const term = new TerminalPage({ active: () => null, front: () => {}, focusStream: () => {} }, NO_TERMINALS);
  const drawer = new MainDrawer(tasks, agents, term, () => 800);
  document.body.append(tasks.summaryElement, agents.summaryElement, drawer.dock.el);
  return { tasks, agents, drawer };
}

describe("状态栏这个会话的几枚：字从文案表来，没内容就不渲染", () => {
  it("★ 任务 做完/全部（悬停分项）· agent N · M 在跑 / 都结束了只写 agent N · 没有就不渲染", () => {
    const { tasks, agents } = mountPanels();
    const t = (id: string, status: string): TaskEntry => ({ ...task(id), status });
    tasks.setSession("s", [t("1", "completed"), t("2", "completed"), t("3", "in_progress"), t("4", "pending")]);
    expect([tasks.summaryElement.textContent, shown(tasks.summaryElement)]).toEqual([copyText("statusBar.tasks.chip", { done: "2", total: "4" }), true]);
    agents.setSession("s", [run("r1"), { ...run("r2"), state: "done" } as RunInfo]);
    expect(agents.summaryElement.textContent).toBe(copyText("agentsPanel.render.summary", { n: "2", running: "1" }));
    agents.setSession("s", [{ ...run("r1"), state: "done" } as RunInfo, { ...run("r2"), state: "done" } as RunInfo, { ...run("r3"), state: "done" } as RunInfo]);
    expect(agents.summaryElement.textContent).toBe("agent 3");
    tasks.setSession("t", []);
    agents.setSession("t", []);
    expect([shown(tasks.summaryElement), shown(agents.summaryElement)]).toEqual([false, false]);
  });
});

describe("底部抽屉：任务 · agent 共用一个；看得见 ⇔ 在 Esc 弹层栈上", () => {
  const open = (d: MainDrawer): string | null => (d.dock.el.hidden ? null : d.dock.current);
  const tabText = (d: MainDrawer): string[] => [...d.dock.el.querySelectorAll("[role=tab]")].map((b) => b.textContent ?? "");
  const pageOf = (d: MainDrawer): string => d.dock.el.querySelector<HTMLElement>("[role=tabpanel]:not([hidden])")?.textContent ?? "";

  it("★ 点 chip 开到那一页；开着点另一枚 ⇒ 换页不关；点当前那枚 ⇒ 收起；页签就是 chip 的字", () => {
    const { tasks, agents, drawer } = mountPanels();
    tasks.setSession("s", [task("1")]);
    agents.setSession("s", [run("r1")]);
    expect(open(drawer)).toBeNull();
    tasks.summaryElement.click();
    expect([open(drawer), tasks.summaryElement.getAttribute("aria-expanded")]).toEqual(["tasks", "true"]);
    expect(tabText(drawer)).toEqual([copyText("statusBar.tasks.chip", { done: "0", total: "1" }), copyText("agentsPanel.render.summary", { n: "1", running: "1" }), copyText("terminal.page.title")]);
    agents.summaryElement.click();
    expect([open(drawer), tasks.summaryElement.getAttribute("aria-expanded"), agents.summaryElement.getAttribute("aria-expanded")]).toEqual(["agents", "false", "true"]);
    agents.summaryElement.click();
    expect(open(drawer)).toBeNull();
  });

  it("★ 一下 Esc 收起（不越过下面那层）；收着时 Esc 落到下面那层", () => {
    const below = floor();
    const { tasks, drawer } = mountPanels();
    tasks.setSession("s", [task("1")]);
    drawer.toggle("tasks");
    esc();
    expect([open(drawer), below.hits]).toEqual([null, 0]);
    esc();
    expect(below.hits).toBe(1);
    dispatcher.popOverlay(below);
  });

  it("★ 切到没有内容的会话：抽屉照开、页里写空态（不藏），Esc 照样收它；页签退成页名", () => {
    const below = floor();
    const { tasks, agents, drawer } = mountPanels();
    tasks.setSession("a", [task("1")]);
    drawer.toggle("tasks");
    tasks.setSession("b", []);
    agents.setSession("b", []);
    expect(open(drawer)).toBe("tasks");
    expect(pageOf(drawer)).toBe(copyText("tasksPanel.page.empty"));
    expect(tabText(drawer)).toEqual([copyText("tasksPanel.page.title"), "agent", copyText("terminal.page.title")]);
    esc();
    expect([open(drawer), below.hits]).toEqual([null, 0]);
    dispatcher.popOverlay(below);
  });

  it("★ 开没开、开哪页、多高记在本机：下次启动照上次；读不懂的格回到收着 · 240", () => {
    const a = mountPanels();
    a.drawer.toggle("agents");
    expect(JSON.parse(localStorage.getItem("cc-monitor.bottom-drawer") ?? "{}")).toEqual({ page: "agents", height: 240 });
    const b = mountPanels();
    expect(open(b.drawer)).toBe("agents");
    expect(b.drawer.dock.el.style.height).toBe("240px");
    a.drawer.toggle("agents");
    expect(JSON.parse(localStorage.getItem("cc-monitor.bottom-drawer") ?? "{}").page).toBeNull();
    localStorage.setItem("cc-monitor.bottom-drawer", '{"page":"bogus","height":12}');
    const c = mountPanels();
    expect([open(c.drawer), c.drawer.dock.el.style.height]).toEqual([null, "240px"]);
    b.drawer.dock.close();
  });

  it("★ 拖上边缘调高：夹在 120 到主区一半之间、记住；双击回到 240；键盘 ↑ 加 16", () => {
    const { drawer } = mountPanels();
    drawer.toggle("tasks");
    const edge = drawer.dock.el.querySelector<HTMLElement>("[role=separator]")!;
    const rect = vi.spyOn(drawer.dock.el, "getBoundingClientRect").mockReturnValue({ height: 240 } as DOMRect);
    const ptr = (type: string, clientY: number): void => {
      const e = new MouseEvent(type, { button: 0, clientY, bubbles: true });
      Object.defineProperty(e, "pointerId", { value: 1 });
      edge.dispatchEvent(e);
    };
    ptr("pointerdown", 500);
    ptr("pointermove", 300);
    expect(drawer.dock.el.style.height).toBe("400px"); // 主区 800 的一半
    ptr("pointermove", 700);
    expect(drawer.dock.el.style.height).toBe("120px");
    ptr("pointerup", 700);
    expect(JSON.parse(localStorage.getItem("cc-monitor.bottom-drawer") ?? "{}").height).toBe(120);
    edge.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    expect(drawer.dock.el.style.height).toBe("240px");
    edge.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
    expect(drawer.dock.el.style.height).toBe("256px");
    rect.mockRestore();
    drawer.dock.close();
  });

  it("页签上 ← → 换页", () => {
    const { tasks, agents, drawer } = mountPanels();
    tasks.setSession("s", [task("1")]);
    agents.setSession("s", [run("r1")]);
    drawer.toggle("tasks");
    drawer.dock.el.querySelector("[role=tablist]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    expect(open(drawer)).toBe("agents");
    drawer.dock.close();
  });

  it("任务表又来一份：同 id 的行原地改（不整表重建）", () => {
    const { tasks, drawer } = mountPanels();
    drawer.toggle("tasks");
    tasks.setSession("s", [task("1"), task("2")]);
    const row1 = drawer.dock.el.querySelector('[data-task-id="1"]');
    tasks.setSession("s", [{ ...task("1"), status: "completed" }, task("2"), task("3")]);
    expect(drawer.dock.el.querySelector('[data-task-id="1"]')).toBe(row1);
    expect(row1?.className).toContain("status-completed");
    expect(drawer.dock.el.querySelectorAll(".tasks-item").length).toBe(3);
    drawer.dock.close();
  });
});

describe("tab 右键菜单躲窗口边：放不下就往上 / 往左翻，贴边内缩 8px", () => {
  let rect: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 200,
      bottom: 300,
      width: 200,
      height: 300,
      toJSON: () => ({}),
    } as DOMRect);
  });
  afterEach(() => rect.mockRestore());

  const at = (x: number, y: number): [string, string] => {
    openMenu({ x, y }, [{ label: "一项", onClick: () => {} }]);
    const m = document.querySelector<HTMLElement>("body > [role=menu]")!;
    return [m.style.left, m.style.top];
  };

  it("放得下 ⇒ 就在光标处；靠下 ⇒ 往上翻；靠右 ⇒ 往左翻；两边都放不下 ⇒ 贴边内缩", () => {
    const [w, h] = [window.innerWidth, window.innerHeight];
    expect(at(100, 100)).toEqual(["100px", "100px"]);
    expect(at(100, h - 20)).toEqual(["100px", `${h - 20 - 300}px`]);
    expect(at(w - 20, 100)).toEqual([`${w - 20 - 200}px`, "100px"]);
    // 窗口比菜单还矮：翻上去会出顶 ⇒ 贴顶内缩 8px。
    const was = Object.getOwnPropertyDescriptor(window, "innerHeight");
    Object.defineProperty(window, "innerHeight", { value: 250, configurable: true });
    try {
      expect(at(100, 200)[1]).toBe("8px");
    } finally {
      if (was) Object.defineProperty(window, "innerHeight", was);
    }
  });
});

describe("会话头「⋯」＝ 右键菜单 ＋ 流的两个开关（过程默认展开 · 显示系统注入），每扇窗一份", () => {
  it("★ 两个开关在菜单末尾；点「显示系统注入」流根上的类跟着切、记住；点「过程默认展开」记住；Alt+↑↓ 没有轮时不出错", () => {
    localStorage.clear();
    const tm = makeTabs(["a", "b"]);
    const root = document.body.lastElementChild as HTMLElement;
    const anchor = document.createElement("button");
    document.body.appendChild(anchor);
    const item = (label: string): HTMLButtonElement | undefined =>
      [...document.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")].find((b) => b.textContent?.includes(label));
    const injected = copyText("stream.injected.toggle");
    const proc = copyText("stream.proc.toggle");
    tm.openMenuFor(anchor, "a");
    expect([!!item(injected), !!item(proc)]).toEqual([true, true]);
    item(injected)!.click();
    expect(root.classList.contains("show-injected")).toBe(true);
    expect(localStorage.getItem("cc-monitor.stream.show-injected")).toBe("1");
    tm.openMenuFor(anchor, "a");
    expect(item(injected)!.getAttribute("aria-checked")).toBe("true");
    item(injected)!.click();
    expect(root.classList.contains("show-injected")).toBe(false);
    tm.openMenuFor(anchor, "a");
    item(proc)!.click();
    expect(localStorage.getItem("cc-monitor.stream.process-expanded")).toBe("1");
    tm.toggleProcessDefault();
    expect(localStorage.getItem("cc-monitor.stream.process-expanded")).toBe("0");
    expect(() => {
      tm.stepTurn(1);
      tm.stepTurn(-1);
    }).not.toThrow();
  });
});

describe("已结束的 tab 在右键菜单里关得掉（窄窗里那颗 × 看不见）", () => {
  it("已结束 ⇒ 菜单里有「关闭标签」，点了 tab 就没了；活着的没有这一项", () => {
    const tm = makeTabs(["a", "b"]);
    const st = inside(tm).store;
    (st.tabs.get("b") as Tab).state = ENDED;
    const menu = (tm as unknown as { menu: { open(e: MouseEvent, sid: string): void } }).menu;
    const items = (): HTMLButtonElement[] => [...document.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")];
    const close = copyText("tabMenu.item.close");
    menu.open(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), "a");
    expect(items().some((b) => b.textContent === close)).toBe(false);
    menu.open(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), "b");
    const item = items().find((b) => b.textContent === close);
    expect(item, "已结束的 tab 菜单里没有关闭").toBeTruthy();
    item!.click();
    expect(st.tabs.has("b")).toBe(false);
  });
});

describe("说不清（那台暂时看不见）不当已结束：tab 上是另一个状态，只在右键菜单里关", () => {
  it("说不清的 tab 挂 `.unseen` 不挂 `.ended`；中键与 W 不关它，菜单里能关并说它会回来", () => {
    const tm = makeTabs(["u", "e"], "pi");
    const st = inside(tm).store;
    (st.tabs.get("u") as Tab).state = UNSEEN;
    (st.tabs.get("e") as Tab).state = ENDED;
    tm.switchTo("e");
    tm.switchTo("u");
    const btn = (sid: string): HTMLElement =>
      (tm as unknown as { bar: { tabButtons: Map<string, { root: HTMLElement }> } }).bar.tabButtons.get(sid)!.root;
    expect([btn("u").classList.contains("unseen"), btn("u").classList.contains("ended")]).toEqual([true, false]);
    expect([btn("e").classList.contains("unseen"), btn("e").classList.contains("ended")]).toEqual([false, true]);
    // 中键：已结束的关、说不清的不关。
    btn("u").dispatchEvent(new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true }));
    expect(st.tabs.has("u")).toBe(true);
    // W：当前是说不清的那个 ⇒ 不关。
    tm.closeActiveIfArchived();
    expect(st.tabs.has("u")).toBe(true);
    // 右键菜单里能关，第二行说连上那台之后会回来。
    const menu = (tm as unknown as { menu: { open(e: MouseEvent, sid: string): void } }).menu;
    menu.open(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), "u");
    const item = [...document.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")].find(
      (b) => b.querySelector("[data-part=label]")?.textContent === copyText("tabMenu.item.close"),
    )!;
    expect(item.disabled).toBe(false);
    expect(item.querySelector("[data-part=why]")?.textContent).toBe(copyText("tabMenu.close.unseen", { machine: "pi" }));
    item.click();
    expect(st.tabs.has("u")).toBe(false);
    btn("e").dispatchEvent(new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true }));
    expect(st.tabs.has("e")).toBe(false);
  });
});

describe("顶栏远端文件的多机选单：再点按钮收起；Esc 只关它", () => {
  afterEach(() => closeMenu());

  const press = (el: Element): void => {
    el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true }));
  };

  it("开着时再点按钮 ⇒ 收起（不是先关又开）；点外面也收起", async () => {
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    await toggleSftpFromTopbar(btn);
    expect(document.querySelectorAll("[role=menu]")).toHaveLength(1);
    press(btn);
    await toggleSftpFromTopbar(btn);
    expect(document.querySelectorAll("[role=menu]")).toHaveLength(0);
    await toggleSftpFromTopbar(btn);
    press(document.body);
    expect(document.querySelectorAll("[role=menu]")).toHaveLength(0);
  });

  it("Esc 只关选单，不越过下面那层（多选 / 查找）", async () => {
    const below = floor();
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    await toggleSftpFromTopbar(btn);
    esc();
    expect([document.querySelectorAll("[role=menu]").length, below.hits]).toEqual([0, 0]);
    esc();
    expect(below.hits).toBe(1);
    dispatcher.popOverlay(below);
  });
});

describe("命令面板：远端会话的机器名只出一次", () => {
  it("会话行：机器名只在徽标那一格（标题里不再带一遍）", () => {
    const tm = makeTabs(["r"], "pi");
    const [cmd] = sessionCommands(tm.tabsInOrder(), () => {}, () => undefined);
    expect(cmd.session?.machine).toBe("pi");
    expect(`${cmd.title} ${cmd.session?.title}`, cmd.title).not.toContain("pi");
  });
});

/** 一条只有工具调用的 assistant 记录。 */
function toolCalls(calls: { id: string; name: string; input: unknown }[], cards: Record<string, string> = {}): JsonlRecord {
  return {
    type: "assistant",
    uuid: "a",
    timestamp: "2026-01-01T02:02:00.000Z",
    message: { role: "assistant", content: calls.map((c) => ({ type: "tool_use", ...c })) },
    toolCards: cards,
  } as never;
}
function results(rs: { id: string; text: string; error?: boolean }[]): JsonlRecord {
  return {
    type: "user",
    uuid: "u",
    timestamp: "2026-01-01T02:03:00.000Z",
    message: { role: "user", content: rs.map((r) => ({ type: "tool_result", tool_use_id: r.id, content: r.text, is_error: r.error ?? false })) },
    userText: { speaker: { kind: "toolResult" }, text: "" },
  } as never;
}
const ctx = (): RenderContext => ({
  parentPath: "/p/s.jsonl",
  origin: LOCAL_ORIGIN,
  toolUseNames: new Map(),
  toolUseElements: new Map(),
  pendingToolResults: new Map(),
});

describe("工具组收着时说出里面有失败的、有子 agent", () => {
  it("一组里一个失败的命令、一个子 agent ⇒ 收着那一行说「1 个失败 · 1 个子 agent」；全成的不说", () => {
    const c = ctx();
    const r = renderMessage(
      toolCalls(
        [
          { id: "t1", name: "Bash", input: { command: "pytest -q" } },
          { id: "t2", name: "Agent", input: { prompt: "看看" } },
          { id: "t3", name: "Read", input: { file_path: "/a" } },
        ],
        { t2: "agent" },
      ),
      c,
    );
    if (r.kind !== "tool-group") throw new Error(`期望工具组，得到 ${r.kind}`);
    const g = buildToolGroup(r.time);
    addToToolGroup(g, r.units);
    const quiet = g.summary.textContent ?? "";
    expect(quiet).not.toMatch(/失败/);
    expect(quiet).toMatch(copyPattern("cards.toolGroup.summaryAgents", { agents: 1 }));
    renderMessage(results([{ id: "t1", text: "1 failed", error: true }, { id: "t3", text: "ok" }]), c);
    expect(g.summary.textContent).toMatch(copyPattern("cards.toolGroup.summaryBoth", { failed: 1, agents: 1 }));
  });
});

describe("命令卡展开显示命令本身（说明作注），不显示入参 JSON", () => {
  it("后端说是命令卡 ⇒ 收着那一行是命令、展开是「# 说明」＋ 命令；没有卡型的照旧", () => {
    const r = renderMessage(
      toolCalls([{ id: "t1", name: "Bash", input: { command: 'rg -n "x" src | head', description: "找调用点" } }], { t1: "command" }),
      ctx(),
    );
    if (r.kind !== "tool-group") throw new Error(`期望工具组，得到 ${r.kind}`);
    const d = r.units[0] as HTMLDetailsElement;
    // 一步一行（§5.2.3）：工具名 · 主参数 · 说明由后端给（`toolSteps`）；这条夹具没带 ⇒ 工具名 ＋ 命令兜底。
    expect([...d.querySelectorAll(".block-summary .step-tool, .block-summary .step-arg")].map((e) => e.textContent)).toEqual(["Bash", 'rg -n "x" src | head']);
    d.open = true;
    d.dispatchEvent(new Event("toggle"));
    const body = d.querySelector(".block-args")?.textContent ?? "";
    expect(body).toBe('# 找调用点\nrg -n "x" src | head');
    expect(body).not.toMatch(/"command"/);
    // 没有卡型（界面不认工具名）⇒ 照旧画入参。
    const plain = renderMessage(toolCalls([{ id: "t2", name: "Bash", input: { command: "ls" } }]), ctx());
    if (plain.kind !== "tool-group") throw new Error(plain.kind);
    const p = plain.units[0] as HTMLDetailsElement;
    p.open = true;
    p.dispatchEvent(new Event("toggle"));
    expect(p.querySelector(".block-args")?.textContent).toMatch(/"command": "ls"/);
  });
});

describe("快捷键翻「自动跟随 / 自动切到前台」：说一句翻成了什么，并告诉设置窗", () => {
  it("翻一下 ⇒ 写盘、主窗口用上、右下角说「自动跟随：关」、发给设置窗翻完之后的两格", async () => {
    behavior.now = { autoFollowUserActive: true, bringMonitorToFrontOnUserActive: false } as never;
    const applied: unknown[] = [];
    await flipBehavior("autoFollowUserActive", (b) => applied.push(b.autoFollowUserActive));
    expect(behavior.saved.map((b) => b.autoFollowUserActive)).toEqual([false]);
    expect(applied).toEqual([false]);
    expect([...document.querySelectorAll("#kit-toast-stack > *")].map((t) => t.textContent ?? "").join("|")).toContain(
      copyText("behaviorToggle.autoFollow.off"),
    );
    expect(vi.mocked(emit).mock.calls).toEqual([
      ["behavior-toggled", { autoFollowUserActive: false, bringMonitorToFrontOnUserActive: false }],
    ]);
  });
});

// 重试细条与报错卡：原因一词是后端给的种类；相邻并条是排版；一串的结局是会话事实（`retries`，按首条的 id 读）。
describe("重试细条：相邻并成一条 · 结局照会话事实", () => {
  const stream = (...els: HTMLElement[]) => {
    const root = document.createElement("div");
    root.append(...els);
    return root;
  };
  it("相邻重试并成一条：次数取后来的、起始留前一条；事实说接上了 ⇒ 重试 ×N 后恢复；后面来什么都不自己判", () => {
    const a = buildApiRetryCard({ timeLabel: "02:10", reason: "overloaded", retryAttempt: 1, maxRetries: 10, id: "r1" });
    expect(a.textContent).toBe(copyText("apiError.retry.line", { reason: copyText("apiError.reason.overloaded"), retry: copyText("apiError.retry.count", { retryAttempt: 1, maxRetries: 10 }), time: "02:10" }));
    mergeRetry(a, buildApiRetryCard({ timeLabel: "02:11", reason: "overloaded", retryAttempt: 3, maxRetries: 10, id: "r2" }));
    expect(a.textContent).toBe(copyText("apiError.retry.line", { reason: copyText("apiError.reason.overloaded"), retry: copyText("apiError.retry.count", { retryAttempt: 3, maxRetries: 10 }), time: "02:10" }));
    const root = stream(a, document.createElement("div"));
    applyRetries(root, new Map([["r2", "recovered"]]));
    expect(a.dataset.state, "按首条的 id 读，并进来的那条的 id 不算").toBe("retrying");
    applyRetries(root, new Map([["r1", "recovered"]]));
    expect(a.textContent).toBe(copyText("apiError.retry.recovered", { reason: copyText("apiError.reason.overloaded"), a: "3", time: "02:11" }));
    applyRetries(root, new Map([["r1", "interrupted"]]));
    expect(a.textContent).toBe(copyText("apiError.retry.interrupted", { reason: copyText("apiError.reason.overloaded"), a: "3", time: "02:11" }));
  });

  it("建卡时事实已经到了 ⇒ 照它画", () => {
    const a = buildApiRetryCard({ timeLabel: "02:10", reason: "network", retryAttempt: 2, maxRetries: 10, id: "r1", outcome: "recovered" });
    expect(a.textContent).toBe(copyText("apiError.retry.recovered", { reason: copyText("apiError.reason.network"), a: "2", time: "02:10" }));
  });

  it("事实说没接上 ⇒ 细条收起、次数进紧跟着的报错卡（那两颗终端按钮不被冲掉）；报错卡标题带原因、原文进折叠", () => {
    const bar = buildApiRetryCard({ timeLabel: "02:10", reason: "overloaded", retryAttempt: 10, maxRetries: 10, id: "r1" });
    const card = buildApiErrorCard({ timeLabel: "02:12", reason: "overloaded", text: "API Error: Overloaded", status: 529 });
    applyRetries(stream(bar, card), new Map([["r1", "failed"]]));
    expect(bar.dataset.state).toBe("failed");
    expect(card.querySelector(".api-error-label")?.textContent).toBe(copyText("apiError.card.title", { reason: copyText("apiError.reason.overloaded") }));
    expect(card.querySelector(".api-error-next-text")?.textContent).toBe(copyText("apiError.card.next", { a: "10", b: "10" }));
    expect(card.querySelectorAll(".api-error-acts [data-act]").length).toBe(2);
    expect(card.querySelector(".api-error-body")?.textContent).toBe("529 · API Error: Overloaded");
  });
});

// 还没结果的那一步什么样子只照会话事实画（`pending[].state`）：在跑 · 在等你 · 状态不明；事实到之前不画状态（不当它在跑）；结果到了照结果。
describe("一步还没结果时：照会话事实画", () => {
  const icon = (row: HTMLElement) => row.querySelector(".step-icon")!.firstElementChild?.tagName.toLowerCase() ?? "";
  it("★ 刚建出来不画状态；事实说在跑 ⇒ 转圈；在等你批准 ⇒ 琥珀点「等你批准」＋ 已等多久；等的不是批准 ⇒「在等你」；状态不明 ⇒ 问号「状态不明」；结果到了照结果、之后的事实不改它", () => {
    const row = buildStepLine("Bash", { tool: "Bash", arg: "rm -rf build/", note: "清掉构建目录", known: true } as never, "", "t1");
    expect([row.dataset.state, row.dataset.call, icon(row)], "事实到之前不当它在跑").toEqual(["pending", "t1", ""]);
    paintWaiting(row, "running", null, false);
    expect([row.dataset.state, row.querySelector(".step-icon [role=progressbar], .step-icon > span") !== null]).toEqual(["running", true]);
    paintWaiting(row, "awaiting", "2m", true);
    expect([row.dataset.state, row.querySelector(".step-await")?.textContent, row.querySelector(".step-right")?.textContent]).toEqual(["awaiting", copyText("stream.step.awaiting"), "2m"]);
    expect(row.querySelector(".step-icon .step-await-dot"), "琥珀点").not.toBeNull();
    paintWaiting(row, "awaiting", null, false);
    expect(row.querySelector(".step-await")?.textContent).toBe(copyText("stream.step.awaitingYou"));
    paintWaiting(row, "unclear", null, false, "untracked");
    expect([row.dataset.state, row.querySelector(".step-await"), row.querySelector(".step-right")?.textContent, icon(row)]).toEqual(["unclear", null, copyText("stream.step.unclear"), "svg"]);
    expect(row.querySelector<HTMLElement>(".step-right")?.title, "悬停说后端给的原因").toBe(copyText("stream.step.unclearUntracked"));
    paintWaiting(row, "unclear", null, false, "noWriter");
    expect(row.querySelector<HTMLElement>(".step-right")?.title).toBe(copyText("stream.step.unclearNoWriter"));
    settleStepLine(row, undefined, { ok: true } as never, false, 1200);
    expect([row.dataset.state, row.querySelector(".step-await")]).toEqual(["ok", null]);
    paintWaiting(row, "running", null, false);
    expect(row.dataset.state, "结果已经到了的那一行不改").toBe("ok");
  });

  it("★ 会话事实比那一步的卡先到：建卡时就照它画（事实里没有的那一步不画，等下一份事实）", () => {
    const c = { ...ctx(), needs: { kind: "approve", call: "t2", sinceMs: Date.now() - 65_000 }, stepWait: (id: string) => ({ t1: { state: "running", why: null }, t2: { state: "awaiting", why: null } } as Record<string, { state: "running" | "awaiting"; why: null }>)[id] };
    const use = { ...(toolCalls([{ id: "t1", name: "Bash", input: {} }, { id: "t2", name: "Bash", input: {} }, { id: "t3", name: "Bash", input: {} }]) as object) } as unknown as JsonlRecord;
    const r = renderMessage(use, c);
    if (r.kind !== "tool-group") throw new Error(r.kind);
    const st = [0, 1, 2].map((i) => r.units[i].querySelector<HTMLElement>(".step-line")!.dataset.state);
    expect(st).toEqual(["running", "awaiting", "pending"]);
    expect(r.units[1].querySelector(".step-right")?.textContent).toBe("1m05s");
  });
});

// 过程里的一步一行：主参数 · 说明 · 结果一句都是后端给的；界面只排、按 id 配对、时刻相减。
describe("一步一行：后端的 toolSteps / toolResults 排成一行", () => {
  it("★ 发出时不画状态（等会话事实）；结果到了 ⇒ 对勾 ＋ 右侧小字（改动 +N −M · 读了几行 · 否则耗时）；失败 ⇒ 叉 ＋「失败 · 耗时」", () => {
    const c = ctx();
    const use = {
      ...(toolCalls([
        { id: "t1", name: "Bash", input: {} },
        { id: "t2", name: "Edit", input: {} },
        { id: "t3", name: "Read", input: {} },
        { id: "t4", name: "Bash", input: {} },
      ]) as object),
      toolSteps: {
        t1: { tool: "Bash", arg: "rg -n x src", note: "找调用点", known: true },
        t2: { tool: "Edit", arg: "/w/src/a.py", path: true, known: true },
        t3: { tool: "Read", arg: "/w/src/b.py", path: true, known: true },
        t4: { tool: "Bash", arg: "pytest -q", known: true },
      },
    } as unknown as JsonlRecord;
    const r = renderMessage(use, c);
    if (r.kind !== "tool-group") throw new Error(r.kind);
    const line = (i: number) => r.units[i].querySelector<HTMLElement>(".step-line")!;
    expect([line(0).dataset.state, line(0).querySelector(".step-arg")?.textContent, line(0).querySelector(".step-note")?.textContent]).toEqual(["pending", "rg -n x src", "找调用点"]);
    const res = {
      ...(results([
        { id: "t1", text: "…" },
        { id: "t2", text: "…" },
        { id: "t3", text: "…" },
        { id: "t4", text: "Exit code 1", error: true },
      ]) as object),
      toolResults: { t1: { ok: true, lines: 3 }, t2: { ok: true, added: 38, removed: 6 }, t3: { ok: true, lines: 212 }, t4: { ok: false } },
    } as unknown as JsonlRecord;
    renderMessage(res, c);
    expect([0, 1, 2, 3].map((i) => [line(i).dataset.state, line(i).querySelector(".step-right")?.textContent])).toEqual([
      ["ok", "1m00s"],
      ["ok", "+38 −6"],
      ["ok", copyText("stream.step.lines", { n: "212" })],
      ["failed", copyText("stream.step.failedFor", { dur: "1m00s" })],
    ]);
  });

  it("★ 退出码只读后端给的 `exitCode`：结果原文里写着「Exit code 1」而后端没给 ⇒ 不出 exit；给了 ⇒ 照写", () => {
    const c = ctx();
    const use = toolCalls([
      { id: "e1", name: "Bash", input: {} },
      { id: "e2", name: "Bash", input: {} },
    ]) as unknown as JsonlRecord;
    const r = renderMessage(use, c);
    if (r.kind !== "tool-group") throw new Error(r.kind);
    const res = {
      ...(results([
        { id: "e1", text: "Exit code 1", error: true },
        { id: "e2", text: "Exit code 1", error: true },
      ]) as object),
      toolResults: { e1: { ok: false }, e2: { ok: false, exitCode: 2 } },
    } as unknown as JsonlRecord;
    renderMessage(res, c);
    const said = (i: number) => r.units[i].querySelector(".block-tool-result-inline summary")?.textContent ?? "";
    expect(said(0)).not.toMatch(/exit/);
    expect(said(1)).toMatch(/exit 2/);
  });

  it("认不出的工具 ⇒ 问号 ＋「未识别结果 · 原文」；人拒了 ⇒「未批准」；没有 toolSteps（老后端）⇒ 工具名 ＋ 入参一句兜底", () => {
    expect(stepRight({ tool: "mcp__x", known: false }, { ok: true }, stateOf({ tool: "mcp__x", known: false }, { ok: true }, false), 10)).toBe(copyText("stream.step.unknown"));
    expect(stateOf(undefined, { ok: false, rejected: true }, true)).toBe("rejected");
    expect(stepRight(undefined, { ok: false, rejected: true }, "rejected", 10)).toBe(copyText("stream.step.rejected"));
    expect([fmtStepDur(300), fmtStepDur(41_000), fmtStepDur(182_000)]).toEqual(["0.3s", "41s", "3m02s"]);
    expect(middleEllipsis(`/${"a".repeat(100)}/file.py`, 40)).toMatch(/^\/a+…\/file\.py$/);
  });
});

// 人粘贴的块（「谁说的」稿 A）：边界 / 正文那一截 / 行数是后端的 `userText.pasted`；超过 12 行折起，不超过的只露正文。
describe("粘贴块", () => {
  const said = (text: string, pasted: object[]): JsonlRecord =>
    ({ type: "user", uuid: "p", timestamp: "2026-01-01T14:02:00.000Z", message: { role: "user", content: text }, userText: { speaker: { kind: "human" }, text, pasted } }) as never;
  const span = (text: string, open: string, body: string, close: string, lines: number) => {
    const start = text.indexOf(open);
    const bodyStart = start + open.length;
    return { start, end: bodyStart + body.length + close.length, bodyStart, bodyEnd: bodyStart + body.length, lines };
  };
  it("★ 超过 12 行折成「粘贴的内容 · N 行」、点开是正文；12 行以内照排、两头的标记不露", () => {
    const long = Array.from({ length: 13 }, (_, i) => `行${i}`).join("\n");
    const short = "甲\n乙";
    const text = `看这段：<P1>${long}</P1>再看<P2>${short}</P2>完`;
    const r = renderMessage(said(text, [span(text, "<P1>", long, "</P1>", 13), span(text, "<P2>", short, "</P2>", 2)]), ctx());
    if (r.kind !== "card") throw new Error(r.kind);
    const folds = r.element.querySelectorAll<HTMLDetailsElement>(".paste-fold");
    expect(folds.length).toBe(1);
    expect([folds[0].open, folds[0].querySelector("summary")?.textContent]).toEqual([false, copyText("speaker.paste.fold", { n: "13" })]);
    expect(folds[0].querySelector(".paste-body")?.textContent).toBe(long.replace(/\n/g, ""));
    const all = r.element.querySelector(".card-body")?.textContent ?? "";
    expect(all).not.toMatch(/<P|<\/P/);
    expect(all).toContain("甲乙");
  });
});

// 不是人说的（「谁说的」稿 A · 事件条）：来源只看后端给的 speaker，正文是后端补的 `speaker.body`。
describe("事件条：agent 交回 / 来话 · 另一会话 · 后台通知并条 · 中断标记", () => {
  const userRec = (speaker: object, ts = "2026-01-01T14:02:00.000Z"): JsonlRecord =>
    ({ type: "user", uuid: "s", timestamp: ts, message: { role: "user", content: "x" }, userText: { speaker, text: "" } }) as never;

  it("★ 交回默认展开、正文是 body、带「打开窗口 ›」（点了发 ccm:reveal-run）；途中来话默认收起；标签按运行表查", () => {
    const c = { ...ctx(), runLabelOf: (run: string) => (run === "a7" ? "审面板交互" : undefined) };
    const r = renderMessage(userRec({ kind: "agentMessage", from: "a7", handback: true, body: "甲乙丙已查完" }), c);
    if (r.kind !== "card") throw new Error(r.kind);
    const el = r.element as HTMLDetailsElement;
    expect([el.open, el.querySelector(".speaker-title")?.textContent, el.querySelector(".speaker-tag")?.textContent]).toEqual([true, "agent「审面板交互」", copyText("speaker.tag.handback")]);
    expect(el.querySelector(".speaker-body")?.textContent).toContain("甲乙丙已查完");
    document.body.appendChild(el);
    const got: unknown[] = [];
    document.addEventListener("ccm:reveal-run", (e) => got.push((e as CustomEvent).detail), { once: true });
    el.querySelector<HTMLButtonElement>(".speaker-link")!.click();
    expect(got).toEqual([{ run: "a7" }]);
    const m = renderMessage(userRec({ kind: "agentMessage", from: "b2", name: "改中转", handback: false, body: "戊" }), c);
    if (m.kind !== "card") throw new Error(m.kind);
    expect([(m.element as HTMLDetailsElement).open, m.element.querySelector(".speaker-title")?.textContent]).toEqual([false, "agent「改中转」"]);
    const i = renderMessage(userRec({ kind: "interrupt" }), c);
    if (i.kind !== "card") throw new Error(i.kind);
    expect(i.element.textContent).toMatch(copyPattern("speaker.interrupt.line", {}, { whole: true }));
    expect(renderMessage(userRec({ kind: "system" }), c).kind, "系统注入默认不露").toBe("skip");
  });

  it("★ 相邻的后台通知并成一条：后台任务 ×3 · 时段 · 失败 N；展开看逐条", () => {
    const n = (status: string, ts: string) => {
      const r = renderMessage(userRec({ kind: "taskNotification", taskId: "t", status, summary: "跑测试" }, ts), ctx());
      if (r.kind !== "card") throw new Error(r.kind);
      return r.element as HTMLDetailsElement;
    };
    const a = n("completed", "2026-01-01T14:02:00.000Z");
    mergeNotice(a, n("completed", "2026-01-01T14:03:00.000Z"));
    mergeNotice(a, n("failed", "2026-01-01T14:07:00.000Z"));
    expect(a.querySelectorAll(".notice-row").length).toBe(3);
    expect(a.querySelector(".notice-head")?.textContent).toMatch(copyPattern("speaker.notice.many", { n: 3, fail: copyText("speaker.notice.fails", { n: 1 }) }, { whole: true }));
    expect(a.dataset.failed).toBe("true");
  });

  it("★ 交回与收场通知去重：会话事实说哪几个子运行交回了 ⇒ 它们的收场通知行收起（整条都是 ⇒ 整条收起），计数 / 时段 / 失败数只算露着的；交回后到也照样收", () => {
    const n = (task: string, status: string, ts: string) => {
      const r = renderMessage(userRec({ kind: "taskNotification", taskId: task, status, summary: `跑${task}` }, ts), ctx());
      if (r.kind !== "card") throw new Error(r.kind);
      return r.element as HTMLDetailsElement;
    };
    const root = document.createElement("div");
    const a = n("w1", "completed", "2026-01-01T14:02:00.000Z");
    mergeNotice(a, n("w2", "failed", "2026-01-01T14:03:00.000Z"));
    mergeNotice(a, n("w3", "completed", "2026-01-01T14:07:00.000Z"));
    const solo = n("w4", "completed", "2026-01-01T15:00:00.000Z");
    root.append(a, solo);
    applyHandedBack(root, new Set());
    expect(a.querySelector(".notice-head")?.textContent).toMatch(copyPattern("speaker.notice.many", { n: 3, fail: copyText("speaker.notice.fails", { n: 1 }) }, { whole: true }));

    applyHandedBack(root, new Set(["w2", "w4"]));
    expect([...a.querySelectorAll<HTMLElement>(".notice-row")].map((r) => r.hidden)).toEqual([false, true, false]);
    expect(a.querySelector(".notice-head")?.textContent, "只剩露着的两条、没有失败").toMatch(copyPattern("speaker.notice.many", { n: 2, fail: "" }, { whole: true }));
    expect(a.dataset.failed).toBe("false");
    expect(solo.hidden, "整条都交回了 ⇒ 整条收起").toBe(true);

    applyHandedBack(root, new Set(["w1", "w2", "w3", "w4"]));
    expect(a.hidden).toBe(true);
    applyHandedBack(root, new Set(["w2", "w3"]));
    expect(a.hidden).toBe(false);
    expect(a.querySelector(".notice-head")?.textContent, "只剩一条 ⇒ 就是那一条").toContain("跑w1");
  });
});
