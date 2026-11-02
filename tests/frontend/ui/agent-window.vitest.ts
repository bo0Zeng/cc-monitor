/**
 * 要求：点 agent 面板的一行开它自己的窗口看它的完整内容 —— 要求「点进一个列表就开一个窗口或者什么」。
 * 窗口里：标题 · 五态与一句说明（状态只读后端运行表）· 它派出的 agent（孙 agent 各开各的窗口）· 完整消息流（派活那段话不画成用户气泡）·
 * 在跑时跟着长 · 往上翻不拽人 · 结束后的结束线 · 系统标题随状态改。
 *
 * 真 `AgentWindow` ＋ 真卡；替身只在边上：会话那一行（`fetchList`）· 按运行读（`loadRunPage`）· 会话流（`followSession` 交来的运行表）· 窗口与事件。
 */
import { liveFace } from "../../test-support/session-faces";
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { FollowEvent } from "../../../src/frontend/ui/events";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import type { Doc } from "../../../src/frontend/ui/generated/Doc";

const h = vi.hoisted(() => ({
  sink: null as ((e: unknown) => void) | null,
  pages: [] as Array<{ rows: unknown[]; more?: boolean }>,
  titles: [] as string[],
  opened: [] as Array<Record<string, unknown>>,
  emitted: [] as Array<[string, unknown]>,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setTitle: (t: string) => {
      h.titles.push(t);
      return Promise.resolve();
    },
    onCloseRequested: () => Promise.resolve(() => {}),
    unminimize: () => Promise.resolve(),
    setFocus: () => Promise.resolve(),
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: (name: string, p: unknown) => {
    h.emitted.push([name, p]);
    return Promise.resolve();
  },
  listen: () => Promise.resolve(() => {}),
}));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    open_session_in_new_window: (a: Record<string, unknown>) => {
      h.opened.push(a);
      return Promise.resolve();
    },
  },
}));
vi.mock("../../../src/frontend/ui/history-list-reads", () => ({
  fetchList: () =>
    Promise.resolve({
      rows: [{ sessionId: "s1", face: liveFace("working"), jsonlPath: "/p/s1.jsonl", label: "主会话", untitled: false, agent: "claude", projectName: "proj" }],
    }),
}));
vi.mock("../../../src/frontend/ui/record-reads", () => ({
  loadRunPage: (_o: unknown, _p: unknown, _w: unknown, from: number) => {
    const page = h.pages.shift() ?? { rows: [] };
    return Promise.resolve({ run: "a1", path: "/p/s1/a1.jsonl", rows: page.rows, end: from + page.rows.length, more: page.more ?? false });
  },
  readBranch: () => Promise.resolve({ off: [], end: 0 }),
}));
vi.mock("../../../src/frontend/ui/events", () => ({
  followSession: (_o: unknown, _sid: string, sink: (e: unknown) => void) => {
    h.sink = sink;
    return Promise.resolve({ stop: () => {} });
  },
}));

import { AgentWindow } from "../../../src/frontend/ui/views/agent-window";
import { copyText } from "../../../src/frontend/ui/copy-table";

const T0 = Date.parse("2026-10-01T10:00:00Z");

function brief(text: string): unknown {
  return { record: { agent: "claude", t: "said", id: "u0", at: "2026-10-01T10:00:00Z", blocks: [{ type: "text", text }], who: { speaker: { kind: "agentTask" }, text } } };
}
/** 助手说的一段：后端交的是块树（`prose`）；只给一句字 ⇒ 一段落，`bytes` 是那句字的字节数。 */
function say(uuid: string, body: string | Doc): unknown {
  const doc: Doc = typeof body === "string" ? { blocks: [{ t: "p", k: [body] }], segs: [] } : body;
  const bytes = typeof body === "string" ? new TextEncoder().encode(body).length : 0;
  return {
    record: { agent: "claude", t: "reply", id: uuid, at: "2026-10-01T10:01:00Z", blocks: [{ type: "prose", doc, bytes }], model: "m", autoReply: false },
  };
}

/** 运行表那一帧里的一项还没写字的样子：`frame` 照后端那样写上状态字与名字（`observe::runs` 出帧那一下写）。 */
type Bare = Omit<RunInfo, "status_text" | "label_text">;

function frame(runs: Bare[]): void {
  const dressed: RunInfo[] = runs.map((r) => ({ ...r, status_text: copyText(`runs.state.${r.state}`), label_text: r.label ?? copyText("runs.label.unnamed") }));
  h.sink?.({ t: "runs", payload: { sid: "s1", runs: dressed, ended: [], groups: { running: [], recent: [], older: [] } } } satisfies FollowEvent);
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
}

async function open(): Promise<{ w: AgentWindow; root: HTMLElement }> {
  document.body.innerHTML = "";
  const root = document.createElement("div");
  const topbar = document.createElement("div");
  const main = document.createElement("main");
  const foot = document.createElement("div");
  root.append(topbar, main, foot);
  document.body.appendChild(root);
  const w = new AgentWindow("s1", "a1", "<local>", { topbar, main, foot });
  await w.start();
  await settle();
  return { w, root };
}

const RUN: Bare = { run: "a1", label: "审交互", kind: "general-purpose", tool: "t1", state: "running", started_ms: T0, active_ms: T0 + 60_000, calls: 2 };

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
  Element.prototype.scrollIntoView = vi.fn() as unknown as Element["scrollIntoView"];
  h.sink = null;
  h.pages = [];
  h.titles = [];
  h.opened = [];
  h.emitted = [];
  vi.useRealTimers();
});

describe("agent 窗口", () => {
  it("系统标题 ＝ 标签 · 状态 · 会话，状态变了跟着改；收场 ⇒ 说明 ＋ 报错原话 ＋ 结束线", async () => {
    h.pages = [{ rows: [brief("查一下"), say("c1", "先读代码")] }, { rows: [] }];
    const { root } = await open();
    frame([RUN]);
    await settle();
    expect(h.titles.at(-1)).toBe(`审交互 · ${copyText("runs.state.running")} · 主会话`);
    expect(root.querySelector('[data-role="new-content"]')?.hasAttribute("hidden")).toBe(true);
    frame([
      {
        ...RUN,
        state: "failed",
        why: "reported",
        ended_ms: T0 + 120_000,
        error: "API Error: 529",
        started_text: "核心·10:00 起",
        facts: [{ text: "核心·用时 2m" }],
        explain: { line: { text: "核心·失败说明" } },
        end: { head: "核心·失败", sub: { text: "核心·交回 {dur}", from: Date.now() - 6 * 60_000 } },
      },
    ]);
    await settle();
    expect(h.titles.at(-1)).toBe(`审交互 · ${copyText("runs.state.failed")} · 主会话`);
    // 事实 · 说明 · 结束线都照抄运行表写好的字（会走的钟那一截按此刻填），界面不按 `state` 挑字。
    expect(root.textContent).toContain(["核心·10:00 起", "核心·用时 2m"].join(copyText("kit.text.sep")));
    expect(root.textContent).toContain("核心·失败说明");
    expect(root.textContent).not.toContain(copyText("agentWindow.why.failed"));
    expect(root.querySelector("pre")?.textContent).toBe("API Error: 529");
    expect(root.textContent).toContain("核心·交回 6m");
  });

  it("在等工具 ⇒ 说明照写好的那一句（等了多久按此刻填）；运行表没给说明 ⇒ 不出说明", async () => {
    h.pages = [{ rows: [] }, { rows: [] }, { rows: [] }];
    const { root } = await open();
    frame([RUN]);
    await settle();
    expect(root.textContent).not.toContain("核心·等待说明");
    const at = Date.now() - 6 * 60_000;
    frame([{ ...RUN, waiting: "Bash", active_ms: at, explain: { line: { text: "核心·等 Bash · {dur}", from: at }, note: "核心·等待说明" } }]);
    await settle();
    expect(root.textContent).toContain("核心·等 Bash · 6m");
    expect(root.textContent).toContain("核心·等待说明");
  });

  it("派活那段话不是用户气泡：带抬头的框，抬头说是谁派的（父 agent 有 ⇒ 它的标签）", async () => {
    h.pages = [{ rows: [brief("查一下 tasks 浮层")] }, { rows: [] }];
    const { root } = await open();
    expect(root.querySelector(".card-user")).toBeNull();
    const box = root.querySelector('[data-role="brief"]');
    expect(box?.textContent).toContain("查一下 tasks 浮层");
    expect(box?.textContent).toContain(copyText("agentWindow.brief.fromMain"));
    frame([{ run: "p1", label: "父", state: "running" }, { ...RUN, parent: "p1" }]);
    await settle();
    expect(root.querySelector('[data-role="brief"]')?.textContent).toContain(copyText("agentWindow.brief.fromAgent", { parent: "父" }));
  });

  it("它派出的 agent：一排小片，点了开那个孙 agent 自己的窗口", async () => {
    h.pages = [{ rows: [] }, { rows: [] }];
    const { root } = await open();
    frame([RUN, { run: "g1", label: "孙", state: "done", parent: "a1" }, { run: "x9", label: "别人的", state: "running" }]);
    await settle();
    const chips = [...root.querySelectorAll<HTMLButtonElement>("button[data-run]")];
    expect(chips.map((c) => c.dataset.run)).toEqual(["g1"]);
    chips[0].click();
    await settle();
    expect(h.opened).toEqual([expect.objectContaining({ sessionId: "s1", run: "g1" })]);
  });

  it("往上翻了不拽人：新内容到了 ⇒ 底下出「↓ 新内容」；回到底部 ⇒ 收起", async () => {
    h.pages = [{ rows: [say("c1", "一")] }, { rows: [say("c2", "二")] }];
    const { root } = await open();
    const scroll = root.querySelector<HTMLElement>(".session-viewer-stream")!;
    Object.defineProperty(scroll, "scrollHeight", { configurable: true, value: 2000 });
    Object.defineProperty(scroll, "clientHeight", { configurable: true, value: 500 });
    scroll.scrollTop = 100;
    scroll.dispatchEvent(new Event("scroll"));
    frame([RUN]);
    await settle();
    const pill = root.querySelector<HTMLElement>('[data-role="new-content"]')!;
    expect(pill.hidden).toBe(false);
    expect(scroll.scrollTop).toBe(100);
    scroll.scrollTop = 1500;
    scroll.dispatchEvent(new Event("scroll"));
    expect(pill.hidden).toBe(true);
  });

  it("公式推迟到滚进视口（与主窗口 / 查看窗同一套）：几百轮的子运行开窗不当场把每一条公式都排一遍", async () => {
    const seen: Array<{ root: Element | null; els: Element[] }> = [];
    class FakeIO {
      private readonly mine: { root: Element | null; els: Element[] };
      constructor(_cb: IntersectionObserverCallback, opts: IntersectionObserverInit = {}) {
        this.mine = { root: (opts.root as Element | null) ?? null, els: [] };
        seen.push(this.mine);
      }
      observe(el: Element): void {
        this.mine.els.push(el);
      }
      unobserve(): void {}
      disconnect(): void {}
    }
    vi.stubGlobal("IntersectionObserver", FakeIO);
    try {
      h.pages = [{ rows: [brief("查一下"), say("c1", { blocks: [{ t: "p", k: ["改好了："] }, { t: "math", tex: "a^2" }], segs: [] })] }, { rows: [] }];
      const { root } = await open();
      const scroll = root.querySelector<HTMLElement>(".session-viewer-stream")!;
      const block = root.querySelector<HTMLElement>("[data-math-pending]");
      expect(block, "还没滚进视口：公式先只留占位").not.toBeNull();
      const io = seen.find((x) => x.root === scroll);
      expect(io, "看的是这扇窗自己的滚动容器").toBeDefined();
      expect(io!.els.some((el) => el.contains(block)), "那张卡交给它等着滚进视口").toBe(true);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("进来停在哪：收场的停在最上（从派活那段话读起），在跑的不动（跟着长）", async () => {
    for (const [state, want] of [["done", 0], ["running", 1500]] as const) {
      h.pages = [{ rows: [brief("查一下"), say("c1", "一")] }, { rows: [] }];
      const { root } = await open();
      const scroll = root.querySelector<HTMLElement>(".session-viewer-stream")!;
      scroll.scrollTop = 1500;
      frame([{ ...RUN, state }]);
      await settle();
      expect(scroll.scrollTop, state).toBe(want);
    }
  });

  it("孙 agent 回到这扇窗：派出它的那张卡闪一下；卡还没读到 ⇒ 读到了再闪", async () => {
    const agentCall = {
      record: {
        agent: "claude",
        t: "reply",
        id: "c9",
        at: "2026-10-01T10:02:00Z",
        blocks: [{ type: "tool_use", id: "t9", name: "Spawn", input: {} }],
        model: "m",
        autoReply: false,
        cards: { t9: "agent" },
        runs: { t9: { label: "孙", kind: "Explore" } },
      },
    };
    h.pages = [{ rows: [] }, { rows: [agentCall] }];
    const { w, root } = await open();
    w.showCard("t9");
    expect(root.querySelector('[data-role="run-card"]')).toBeNull();
    frame([RUN]);
    await settle();
    expect(root.querySelector('[data-role="run-card"]')?.classList.contains("search-hit-flash")).toBe(true);
  });

  it("回到派出它的地方：主会话派的 ⇒ 交主窗口滚到那张派出卡", async () => {
    h.pages = [{ rows: [] }, { rows: [] }];
    const { root } = await open();
    frame([RUN]);
    await settle();
    const back = [...root.querySelectorAll("button")].find((b) => b.textContent === copyText("agentWindow.back.label"))!;
    back.click();
    await settle();
    expect(h.emitted).toContainEqual(["show-run-card", { sessionId: "s1", tool: "t1", in: null }]);
  });
});
