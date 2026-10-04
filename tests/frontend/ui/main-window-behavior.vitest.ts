// 主窗口剩下的几处行为：状态栏两块浮层（任务 · 子 agent）· tab 右键菜单（躲窗口边 · 关得掉已结束的 · 说不清的不当已结束）·
// 顶栏远端文件选单 · 命令面板里的机器名 · 工具组收着时说出失败与子 agent · 命令卡展开显示命令本身。
// 行为口径：同一时刻同一层只开一块；看得见 ⇔ 在 Esc 弹层栈上，Esc 一次只关最上一层；浮层躲窗口边、贴边内缩 8px；
// 收起后看不见的动作在右键菜单里有；「说不清」是一个单独的状态，不并进「已结束」。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
// TabManager 的重协作者换成空壳（流 · 时间线 · 分支折叠）。
vi.mock("../../../src/frontend/ui/stream", () => ({
  MessageStream: class {
    contentElement = document.createElement("div");
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
import { showTabContextMenu } from "../../../src/frontend/ui/tab-context-menu";
import { TabManager, type Tab } from "../../../src/frontend/ui/tabs";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ENDED, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import type { TabStore } from "../../../src/frontend/ui/tab-store";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { closeSftpHostPicker, toggleSftpFromTopbar } from "../../../src/frontend/ui/sftp-host-picker";
import { sessionCommands } from "../../../src/frontend/ui/session-commands";
import { buildToolGroup, addToToolGroup, renderMessage, type RenderContext } from "../../../src/frontend/ui/cards/index";
import type { JsonlRecord } from "../../../src/frontend/ui/generated/JsonlRecord";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";

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

function mountPanels(): { tasks: TasksPanel; agents: AgentsPanel } {
  const tasks = new TasksPanel();
  const agents = new AgentsPanel();
  document.body.append(tasks.summaryElement, tasks.popoverElement, agents.summaryElement, agents.popoverElement);
  return { tasks, agents };
}

describe("状态栏两块浮层：看得见 ⇔ 在 Esc 弹层栈上；同一时刻只开一块", () => {
  it("上次退出时任务面板开着 ⇒ 启动后一有任务就看得见，一下 Esc 收起（不越过下面那层）", () => {
    localStorage.setItem("cc-monitor.tasks-panel.collapsed", "0");
    const below = floor();
    const { tasks } = mountPanels();
    tasks.setSession("s", [task("1")]);
    expect(shown(tasks.popoverElement)).toBe(true);
    esc();
    expect(shown(tasks.popoverElement)).toBe(false);
    expect(below.hits).toBe(0);
    dispatcher.popOverlay(below);
  });

  it("展开着切到没任务的 tab ⇒ 藏起来、让出 Esc（这一下落到下面那层）", () => {
    const below = floor();
    const { tasks } = mountPanels();
    tasks.setSession("s", [task("1")]);
    tasks.summaryElement.click();
    expect(shown(tasks.popoverElement)).toBe(true);
    tasks.setSession("t", []);
    expect(shown(tasks.popoverElement)).toBe(false);
    esc();
    expect(below.hits).toBe(1);
    dispatcher.popOverlay(below);
  });

  it("子 agent 面板认 Esc；开了任务面板再开子 agent 面板 ⇒ 任务面板收起（反过来同样）", () => {
    const below = floor();
    const { tasks, agents } = mountPanels();
    tasks.setSession("s", [task("1")]);
    agents.setSession("s", [run("r1")]);
    tasks.summaryElement.click();
    expect([shown(tasks.popoverElement), shown(agents.popoverElement)]).toEqual([true, false]);
    agents.summaryElement.click();
    expect([shown(tasks.popoverElement), shown(agents.popoverElement)]).toEqual([false, true]);
    tasks.summaryElement.click();
    expect([shown(tasks.popoverElement), shown(agents.popoverElement)]).toEqual([true, false]);
    agents.summaryElement.click();
    esc();
    expect([shown(tasks.popoverElement), shown(agents.popoverElement), below.hits]).toEqual([false, false, 0]);
    esc();
    expect(below.hits).toBe(1);
    dispatcher.popOverlay(below);
  });

  it("任务面板展开着、切到没任务的 tab 上开子 agent 面板 ⇒ 切回来开着的仍是子 agent 面板（藏着的那块也收）", () => {
    const { tasks, agents } = mountPanels();
    tasks.setSession("a", [task("1")]);
    agents.setSession("a", [run("r1")]);
    tasks.summaryElement.click();
    tasks.setSession("b", []);
    agents.setSession("b", [run("r2")]);
    agents.summaryElement.click();
    expect([shown(tasks.popoverElement), shown(agents.popoverElement)]).toEqual([false, true]);
    tasks.setSession("a", [task("1")]);
    agents.setSession("a", [run("r1")]);
    expect([shown(tasks.popoverElement), shown(agents.popoverElement)]).toEqual([false, true]);
    esc();
    expect([shown(tasks.popoverElement), shown(agents.popoverElement)]).toEqual([false, false]);
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
    showTabContextMenu(x, y, [{ label: "一项", onClick: () => {} }]);
    const m = document.querySelector<HTMLElement>("body > .tab-context-menu")!;
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

describe("已结束的 tab 在右键菜单里关得掉（窄窗里那颗 × 看不见）", () => {
  it("已结束 ⇒ 菜单里有「关闭标签」，点了 tab 就没了；活着的没有这一项", () => {
    const tm = makeTabs(["a", "b"]);
    const st = inside(tm).store;
    (st.tabs.get("b") as Tab).state = ENDED;
    const menu = (tm as unknown as { menu: { open(e: MouseEvent, sid: string): void } }).menu;
    const items = (): HTMLButtonElement[] => [...document.querySelectorAll<HTMLButtonElement>(".tab-context-menu-item")];
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
      [...document.querySelectorAll<HTMLElement>(".tab")].find((el) => el.title.startsWith((st.tabs.get(sid) as Tab).title))!;
    expect([btn("u").classList.contains("unseen"), btn("u").classList.contains("ended")]).toEqual([true, false]);
    expect([btn("e").classList.contains("unseen"), btn("e").classList.contains("ended")]).toEqual([false, true]);
    // 中键：已结束的关、说不清的不关。
    btn("u").dispatchEvent(new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true }));
    expect(st.tabs.has("u")).toBe(true);
    // W：当前是说不清的那个 ⇒ 不关。
    tm.closeActiveIfArchived();
    expect(st.tabs.has("u")).toBe(true);
    // 右键菜单里能关，悬停说它若还在跑、连上那台之后会回来。
    const menu = (tm as unknown as { menu: { open(e: MouseEvent, sid: string): void } }).menu;
    menu.open(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), "u");
    const item = [...document.querySelectorAll<HTMLButtonElement>(".tab-context-menu-item")].find(
      (b) => b.textContent === copyText("tabMenu.item.close"),
    )!;
    expect(item.title).toBe(copyText("tabMenu.close.unseenHint", { machine: "pi" }));
    item.click();
    expect(st.tabs.has("u")).toBe(false);
    btn("e").dispatchEvent(new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true }));
    expect(st.tabs.has("e")).toBe(false);
  });
});

describe("顶栏远端文件的多机选单：再点按钮收起；Esc 只关它", () => {
  afterEach(() => closeSftpHostPicker());

  const press = (el: Element): void => {
    el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true }));
  };

  it("开着时再点按钮 ⇒ 收起（不是先关又开）；点外面也收起", async () => {
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    await toggleSftpFromTopbar(btn);
    expect(document.querySelectorAll(".sftp-host-picker")).toHaveLength(1);
    press(btn);
    await toggleSftpFromTopbar(btn);
    expect(document.querySelectorAll(".sftp-host-picker")).toHaveLength(0);
    await toggleSftpFromTopbar(btn);
    press(document.body);
    expect(document.querySelectorAll(".sftp-host-picker")).toHaveLength(0);
  });

  it("Esc 只关选单，不越过下面那层（多选 / 查找）", async () => {
    const below = floor();
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    await toggleSftpFromTopbar(btn);
    esc();
    expect([document.querySelectorAll(".sftp-host-picker").length, below.hits]).toEqual([0, 0]);
    esc();
    expect(below.hits).toBe(1);
    dispatcher.popOverlay(below);
  });
});

describe("命令面板：远端会话的机器名只出一次", () => {
  it("「切到会话」那一条的标题里机器名恰一次", () => {
    const tm = makeTabs(["r"], "pi");
    const [cmd] = sessionCommands(tm.snapshotSessions(), () => {});
    expect(cmd.title.split("[pi]").length - 1, cmd.title).toBe(1);
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
    const g = buildToolGroup(r.timestamp);
    addToToolGroup(g, r.units);
    const quiet = g.summary.textContent ?? "";
    expect(quiet).not.toMatch(/失败/);
    expect(quiet).toMatch(/1 个子 agent/);
    renderMessage(results([{ id: "t1", text: "1 failed", error: true }, { id: "t3", text: "ok" }]), c);
    expect(g.summary.textContent).toMatch(/1 个失败 · 1 个子 agent/);
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
    expect(d.querySelector(".block-summary")?.textContent).toBe('🔧 Bash  rg -n "x" src | head');
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
    expect([...document.querySelectorAll(".ccm-toast")].map((t) => t.textContent ?? "").join("|")).toContain(
      copyText("behaviorToggle.autoFollow.off"),
    );
    expect(vi.mocked(emit).mock.calls).toEqual([
      ["behavior-toggled", { autoFollowUserActive: false, bringMonitorToFrontOnUserActive: false }],
    ]);
  });
});
