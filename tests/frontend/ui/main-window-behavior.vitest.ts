// 主窗口剩下的几处行为：状态栏两块浮层（任务 · 子 agent）· tab 右键菜单（躲窗口边 · 关得掉已结束的 · 说不清的不当已结束）·
// 顶栏远端文件选单 · 命令面板里的机器名 · 工具组收着时说出失败与子 agent · 命令卡展开显示命令本身。
// 行为口径：同一时刻同一层只开一块；看得见 ⇔ 在 Esc 弹层栈上，Esc 一次只关最上一层；浮层躲窗口边、贴边内缩 8px；
// 收起后看不见的动作在右键菜单里有；「说不清」是一个单独的状态，不并进「已结束」。
import { buildApiErrorCard, buildApiRetryCard, mergeRetry, settleRetry } from "../../../src/frontend/ui/cards/api-error";
import { fmtStepDur, middleEllipsis, stateOf, stepRight } from "../../../src/frontend/ui/cards/step-line";
import { mergeNotice } from "../../../src/frontend/ui/cards/speaker-bar";
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
    // 右键菜单里能关，悬停说它若还在跑、连上那台之后会回来。
    const menu = (tm as unknown as { menu: { open(e: MouseEvent, sid: string): void } }).menu;
    menu.open(new MouseEvent("contextmenu", { clientX: 1, clientY: 1 }), "u");
    const item = [...document.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")].find(
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

// 重试细条与报错卡（主窗口稿 §5.2.5）：原因一词是后端给的种类；相邻并条与结局是排版。
describe("重试细条：相邻并成一条 · 接上 / 没接上", () => {
  it("相邻重试并成一条：次数取后来的、起始留前一条；接上了 ⇒ 重试 ×N 后恢复", () => {
    const a = buildApiRetryCard({ timeLabel: "02:10", reason: "overloaded", retryAttempt: 1, maxRetries: 10 });
    expect(a.textContent).toBe("服务器过载 · 重试 1/10 · 02:10 起");
    mergeRetry(a, buildApiRetryCard({ timeLabel: "02:11", reason: "overloaded", retryAttempt: 3, maxRetries: 10 }));
    expect(a.textContent).toBe("服务器过载 · 重试 3/10 · 02:10 起");
    settleRetry(a, document.createElement("div"));
    expect(a.textContent).toBe("服务器过载 · 重试 ×3 后恢复 · 02:11");
  });

  it("后面是报错卡 ⇒ 细条收起、次数进报错卡；报错卡标题带原因、原文进折叠", () => {
    const bar = buildApiRetryCard({ timeLabel: "02:10", reason: "overloaded", retryAttempt: 10, maxRetries: 10 });
    const card = buildApiErrorCard({ timeLabel: "02:12", reason: "overloaded", text: "API Error: Overloaded", status: 529 });
    settleRetry(bar, card);
    expect(bar.dataset.state).toBe("failed");
    expect(card.querySelector(".api-error-label")?.textContent).toBe("本轮中断 · 服务器过载");
    expect(card.querySelector(".api-error-next")?.textContent).toBe("重试 10/10 后停止 · 终端里重发可继续");
    expect(card.querySelector(".api-error-body")?.textContent).toBe("529 · API Error: Overloaded");
  });
});

// 过程里的一步一行（主窗口稿 §5.2.3 · B5）：主参数 · 说明 · 结果一句都是后端给的；界面只排、按 id 配对、时刻相减。
describe("一步一行：后端的 toolSteps / toolResults 排成一行", () => {
  it("★ 发出时在跑（转圈）；结果到了 ⇒ 对勾 ＋ 右侧小字（改动 +N −M · 读了几行 · 否则耗时）；失败 ⇒ 叉 ＋「失败 · 耗时」", () => {
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
    expect([line(0).dataset.state, line(0).querySelector(".step-arg")?.textContent, line(0).querySelector(".step-note")?.textContent]).toEqual(["running", "rg -n x src", "找调用点"]);
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
      ["ok", "212 行"],
      ["failed", "失败 · 1m00s"],
    ]);
  });

  it("认不出的工具 ⇒ 问号 ＋「未识别结果 · 原文」；人拒了 ⇒「未批准」；没有 toolSteps（老后端）⇒ 工具名 ＋ 入参一句兜底", () => {
    expect(stepRight({ tool: "mcp__x", known: false }, { ok: true }, stateOf({ tool: "mcp__x", known: false }, { ok: true }, false), 10)).toBe("未识别结果 · 原文");
    expect(stateOf(undefined, { ok: false, rejected: true }, true)).toBe("rejected");
    expect(stepRight(undefined, { ok: false, rejected: true }, "rejected", 10)).toBe("未批准");
    expect([fmtStepDur(300), fmtStepDur(41_000), fmtStepDur(182_000)]).toEqual(["0.3s", "41s", "3m02s"]);
    expect(middleEllipsis(`/${"a".repeat(100)}/file.py`, 40)).toMatch(/^\/a+…\/file\.py$/);
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
    expect([el.open, el.querySelector(".speaker-title")?.textContent, el.querySelector(".speaker-tag")?.textContent]).toEqual([true, "agent「审面板交互」", "交回"]);
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
    expect(i.element.textContent).toMatch(/^你中断了本轮/);
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
    expect(a.querySelector(".notice-head")?.textContent).toMatch(/^后台任务 ×3 · .+–.+ · 失败 1$/);
    expect(a.dataset.failed).toBe("true");
  });
});
