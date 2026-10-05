// F86（#45）历史「来源筛选保持」+「多主机默认折叠」的 jsdom 回归测试。
//
// 为什么需要它：history.ts 1613 行零 TS 单测，折叠/筛选持久化极易静默回归（默认折叠污染成偏好、
// 或用户展开后又被默认折叠盖掉、或隐藏偏好丢失）。这里在真 HistoryView 实例 + 真 history-prefs 上
// 锁死：隐藏跨重启保持、远端首见默认折叠、用户展开后跨重启保持展开、折回默认清键。
//
// 照 history-source-cache.vitest.ts 的 mock 骨架把重协作者 mock 成空壳；「重启」= close 旧实例后
// new 一个新 HistoryView（共享 localStorage，不清）。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("../../../../src/frontend/ui/views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_close: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/remote-launch-run", () => ({ runRemoteResume: vi.fn() }));
vi.mock("../../../../src/frontend/ui/behavior", () => ({ getBehavior: () => ({}) }));
vi.mock("../../../../src/frontend/ui/format", () => ({ formatTimestampSmart: () => "时间" }));

import { invoke } from "@tauri-apps/api/core";
// 历史清单改走通道（问本机常驻后端）：旧命令名照旧当「哪一问」的名字，译法住 chan-fake。
import { withHistoryReads } from "../../../test-support/chan-fake";
import { HistoryView } from "../../../../src/frontend/ui/views/history";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

// 一个本地项目（origin 缺省）+ 一个远端项目（origin=hostA）→ distinctOrigins==2 → 触发分组 + chip 行。
function localProj(): Record<string, unknown> {
  return { projectPath: "/l/p", projectName: "本地项目", projectDir: "lp", sessionCount: 1, starredCount: 0, hiddenCount: 0, lastActivity: 1, hasLive: false };
}
function remoteProj(): Record<string, unknown> {
  return { projectPath: "/r/p", projectName: "远端项目", projectDir: "rp", sessionCount: 1, starredCount: 0, hiddenCount: 0, lastActivity: 1, hasLive: false, origin: "hostA" };
}
function setupInvoke(): void {
  invokeMock.mockReset();
  invokeMock.mockImplementation(withHistoryReads((cmd: string) => {
    if (cmd === "list_history_projects") return Promise.resolve([localProj()]);
    if (cmd === "list_remote_history_projects")
      return Promise.resolve({ projects: [remoteProj()], failedHosts: [] });
    return Promise.resolve(undefined);
  }));
}

type ViewInternals = {
  hiddenOrigins: Set<string>;
  originOpenOverrides: Record<string, boolean>;
  projects: unknown[];
};

function group(name: string): HTMLDetailsElement | undefined {
  return [...document.querySelectorAll<HTMLDetailsElement>(".history-origin-group")].find(
    (g) => g.querySelector(".history-origin-name")?.textContent === name,
  );
}
function chip(text: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>(".history-origin-chip")].find(
    (c) => c.textContent === text,
  );
}

describe("HistoryView 来源筛选保持 + 多主机默认折叠 (F86 #45)", () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.replaceChildren();
    setupInvoke();
  });

  it("远端首见默认折叠、本地默认展开，且不污染偏好表", async () => {
    const view = new HistoryView();
    await view.open();
    expect(group("[hostA]")?.open).toBe(false); // 远端默认折叠
    expect(group("本地")?.open).toBe(true); // 本地默认展开
    // 首见默认折叠不写偏好（即便初始程序化 open=false 触发了 toggle，nextOverrides 也删键）
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({});
  });

  it("用户展开远端 → 偏好持久 → 重启后保持展开（不被默认折叠盖掉）", async () => {
    const view = new HistoryView();
    await view.open();
    const hostA = group("[hostA]")!;
    // 模拟用户展开
    hostA.open = true;
    hostA.dispatchEvent(new Event("toggle"));
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({ hostA: true });
    expect(localStorage.getItem("cc-monitor.history.origin-open")).toContain("hostA");

    // 重启：close + 新实例（共享 localStorage）
    view.close();
    const view2 = new HistoryView();
    await view2.open();
    expect(group("[hostA]")?.open).toBe(true); // 保持展开
    expect(group("本地")?.open).toBe(true); // 本地仍展开
  });

  it("展开后折回默认 → 偏好表清空该键（回落默认折叠）", async () => {
    const view = new HistoryView();
    await view.open();
    const hostA = group("[hostA]")!;
    hostA.open = true;
    hostA.dispatchEvent(new Event("toggle"));
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({ hostA: true });
    // 折回
    hostA.open = false;
    hostA.dispatchEvent(new Event("toggle"));
    expect((view as unknown as ViewInternals).originOpenOverrides).toEqual({});
  });

  it("隐藏某来源 → 跨重启保持隐藏 + chip inactive", async () => {
    const view = new HistoryView();
    await view.open();
    // 点掉 hostA chip
    chip("[hostA]")!.click();
    const inner = view as unknown as ViewInternals;
    expect(inner.hiddenOrigins.has("hostA")).toBe(true);
    expect(localStorage.getItem("cc-monitor.history.hidden-origins")).toContain("hostA");

    // 重启
    view.close();
    const view2 = new HistoryView();
    await view2.open();
    const inner2 = view2 as unknown as ViewInternals;
    expect(inner2.hiddenOrigins.has("hostA")).toBe(true); // 仍隐藏
    expect(chip("[hostA]")?.classList.contains("active")).toBe(false); // chip inactive
    // hostA 项目不渲染（无 [hostA] 分区）
    expect(group("[hostA]")).toBeUndefined();
  });
});

// ════════════════════════════════════════════════════════════════════════════
// 历史页的行为缺陷：没加载上 · 全量加载后刷新 · 空白标题 · 父会话被筛掉 · 搜索失败 · 键盘 · 开页焦点 · 同名目录 · 按标题搜
// ════════════════════════════════════════════════════════════════════════════

import { copyText } from "../../../../src/frontend/ui/copy-table";
import { chanArgsJson, chanReply, isChanCall, linesReply, NO_CHANNEL } from "../../../test-support/chan-fake";

type Sess = Record<string, unknown>;
function sess(sid: string, over: Sess = {}): Sess {
  return {
    sessionId: sid,
    projectPath: "/l/p",
    projectName: "p",
    aiTitle: `标题-${sid}`,
    firstUserExcerpt: "第一句",
    startedAt: 1,
    updatedAt: 1,
    jsonlPath: `/h/.claude/projects/lp/${sid}.jsonl`,
    isLive: false,
    messageCountApprox: 1,
    isBg: false,
    starred: false,
    customTitle: null,
    hidden: false,
    ...over,
  };
}

/** 替身：项目清单 `projects` · 会话清单按 `sessions(args)` 答（抛 ⇒ 那一问失败）· 别的问 `other`。 */
function wireHistory(
  projects: Record<string, unknown>[],
  sessions: (a: { projectDir: string; projectPath?: string }) => Sess[],
  other: (cmd: string, args: unknown) => unknown = () => undefined,
): { sessionCalls: number } {
  const seen = { sessionCalls: 0 };
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    withHistoryReads((cmd: string, args: Record<string, unknown>) => {
      if (cmd === "list_history_projects") return Promise.resolve(projects);
      if (cmd === "list_remote_history_projects") return Promise.resolve({ projects: [], failedHosts: [] });
      if (cmd === "stream_history_sessions_in_project") {
        seen.sessionCalls += 1;
        const onEntry = args.onEntry as { onmessage: (e: Sess) => void };
        for (const e of sessions(args as { projectDir: string; projectPath?: string })) onEntry.onmessage(e);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(other(cmd, args));
    }),
  );
  return seen;
}

type Inner = {
  lazyDrained(): Promise<void>;
  loadAllSessions(): Promise<void>;
  refresh(force?: boolean): Promise<void>;
  runFullTextSearch(): Promise<void>;
  loadedAll: boolean;
  viewer: unknown;
  filter: string;
  searchMode: string;
  searchInput: HTMLInputElement;
  statusEl: HTMLElement;
  resultsEl: HTMLElement;
};
const inner = (v: HistoryView): Inner => v as unknown as Inner;
// 每一轮也等一帧：加载落地后的重画排在下一帧（`scheduleRender`），不等它，`expand` 回的可能是
// 马上要被换掉的那一组 —— 负载一高，测试就去收起一个已经不在页面上的元素。
const settle = async (v: HistoryView): Promise<void> => {
  for (let i = 0; i < 5; i++) {
    await inner(v).lazyDrained();
    await new Promise((r) => requestAnimationFrame(() => r(undefined)));
    await new Promise((r) => setTimeout(r, 0));
  }
};

function projectGroups(): HTMLDetailsElement[] {
  return [...document.querySelectorAll<HTMLDetailsElement>(".history-group")];
}
/** 展开第 `i` 个项目组，等加载落地；回落地之后（重画过的）那一组。 */
async function expand(v: HistoryView, i: number): Promise<HTMLDetailsElement> {
  // 只改 `open`：toggle 事件由 DOM 自己发（与用户点开同一形；再手发一次就成了两次）。
  const g = projectGroups()[i];
  g.open = true;
  await settle(v);
  return projectGroups()[i];
}
function titles(g: HTMLElement): string[] {
  return [...g.querySelectorAll(".history-entry .history-title")].map((t) => t.textContent ?? "");
}

describe("历史页行为缺陷", () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.replaceChildren();
  });

  it("某项目的会话没加载上 ⇒ 展开处说「没加载上」（不说「没有会话」），再展开重试一次", async () => {
    let fail = true;
    const seen = wireHistory([localProj()], () => {
      if (fail) throw new Error("那台没答");
      return [sess("s1")];
    });
    const view = new HistoryView();
    await view.open();
    let g = await expand(view, 0);
    expect(g.textContent).toContain(copyText("history.body.loadFailed"));
    expect(g.textContent).not.toContain(copyText("history.body.empty"));
    expect(seen.sessionCalls, "没加载上之后自己一遍遍重试").toBe(1);
    fail = false;
    g.open = false;
    await settle(view);
    g = await expand(view, 0);
    expect(seen.sessionCalls, "再展开没有再问").toBe(2);
    expect(titles(g)).toEqual(["标题-s1"]);
  });

  it("「全量加载」之后刷新 ⇒ 不再算「已全量加载」，再点一次真的会去加载；有没加载上的不记成已全量", async () => {
    const seen = wireHistory([localProj()], () => [sess("s1")]);
    const view = new HistoryView();
    await view.open();
    await inner(view).loadAllSessions();
    expect(inner(view).loadedAll).toBe(true);
    await inner(view).refresh(true);
    expect(inner(view).loadedAll, "刷新清了会话缓存，却还说已全量加载").toBe(false);
    expect(inner(view).searchInput.placeholder).toBe(copyText("history.search.placeholderTree"));
    const before = seen.sessionCalls;
    await inner(view).loadAllSessions();
    expect(seen.sessionCalls, "刷新后再点「全量加载」没有反应").toBe(before + 1);
    // 部分没加载上 ⇒ 不记成已全量。
    wireHistory([localProj()], () => {
      throw new Error("读不动");
    });
    await inner(view).refresh(true);
    await inner(view).loadAllSessions();
    expect(inner(view).loadedAll).toBe(false);
  });

  it("没标题、没说过话的会话用后端给的标题（会话 ID 前 8 位），不是空白", async () => {
    wireHistory([localProj()], () => [sess("abcdef0123456789", { aiTitle: null, firstUserExcerpt: "" })]);
    const view = new HistoryView();
    await view.open();
    const g = await expand(view, 0);
    expect(titles(g)).toEqual(["abcdef01"]);
  });

  it("父会话被隐藏时子会话照样挂在它名下，不说「原会话可能已删除」", async () => {
    wireHistory([localProj()], () => [
      sess("parent", { hidden: true }),
      sess("child", { forkedFromSessionId: "parent", forkedFromMessageUuid: "m1" }),
    ]);
    const view = new HistoryView();
    await view.open();
    const g = await expand(view, 0);
    expect(g.querySelector(".history-fork-orphan"), "父会话只是被隐藏，子会话却挂了孤儿标记").toBeNull();
    expect(titles(g)).toEqual(["标题-parent", "标题-child"]);
    expect(g.querySelector(".history-entry")?.classList.contains("is-context-entry")).toBe(true);
  });

  it("全文搜索失败 ⇒ 上一次的结果不留着", async () => {
    let ok = true;
    wireHistory([localProj()], () => [], (cmd, args) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (isChanCall(cmd, args, "history-search")) {
        if (!ok) throw NO_CHANNEL;
        return linesReply([
          { agent: "claude", sessionId: "s1", projectPath: "/l/p", projectName: "p", jsonlPath: "/s1.jsonl", title: "t", updatedAt: 1, hitCount: 1, hits: [{ uuid: "u", tsMs: 1, kind: "user", before: "", matched: "kw", after: "" }] },
        ]);
      }
      if (isChanCall(cmd, args, "history-search-merge")) {
        const rows = (chanArgsJson(args) as { sessions: unknown[] }).sessions;
        return chanReply({ totalHits: rows.length, sessionCount: rows.length, truncated: false, sessions: rows });
      }
      return undefined;
    });
    const view = new HistoryView();
    await view.open();
    inner(view).searchMode = "fulltext";
    inner(view).searchInput.value = "kw";
    await inner(view).runFullTextSearch();
    expect(inner(view).resultsEl.children.length).toBeGreaterThan(0);
    ok = false;
    await inner(view).runFullTextSearch();
    expect(inner(view).resultsEl.children.length, "搜索失败了，上一个词的结果还摆着").toBe(0);
  });

  it("列表行键盘到得了：可聚焦、回车打开、↓ 走到下一行", async () => {
    wireHistory([localProj()], () => [sess("s1", { updatedAt: 2 }), sess("s2")]);
    const view = new HistoryView();
    await view.open();
    const g = await expand(view, 0);
    const rows = [...g.querySelectorAll<HTMLElement>(".history-entry")];
    expect(rows.map((r) => r.tabIndex)).toEqual([0, 0]);
    rows[0].focus();
    rows[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement).toBe(rows[1]);
    rows[1].dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(inner(view).viewer, "回车没打开会话").not.toBeNull();
  });

  it("开页就给搜索框焦点，不等远端；远端答回来时不把焦点抢回去", async () => {
    let release: (v: unknown) => void = () => {};
    invokeMock.mockReset();
    invokeMock.mockImplementation(
      withHistoryReads((cmd: string) => {
        if (cmd === "list_history_projects") return new Promise((r) => (release = r));
        if (cmd === "list_remote_history_projects") return Promise.resolve({ projects: [], failedHosts: [] });
        return Promise.resolve(undefined);
      }),
    );
    const view = new HistoryView();
    const opened = view.open();
    expect(document.activeElement, "开页没先给焦点").toBe(inner(view).searchInput);
    const elsewhere = document.createElement("input");
    document.body.appendChild(elsewhere);
    elsewhere.focus();
    release([localProj()]);
    await opened;
    expect(document.activeElement, "远端答回来把焦点抢回了搜索框").toBe(elsewhere);
  });

  it("记录目录名相同、真实目录不同的两个项目各是各的（各问各的那一组会话）", async () => {
    const twin = (path: string) => ({ ...localProj(), projectDir: "-h-x", projectPath: path, projectName: "x" });
    wireHistory([twin("/h/文档/x"), twin("/h/桌面/x")], (a) =>
      a.projectPath === "/h/文档/x" ? [sess("doc", { projectPath: "/h/文档/x" })] : [sess("desk", { projectPath: "/h/桌面/x" })],
    );
    const view = new HistoryView();
    await view.open();
    await expand(view, 0);
    await expand(view, 1);
    const [a, b] = projectGroups();
    expect([titles(a), titles(b)].sort()).toEqual([["标题-desk"], ["标题-doc"]]);
  });

  it("「按项目」搜只在会话标题里的词：后端按标题搜全部会话，那个项目照样搜得到、会话列出来", async () => {
    wireHistory([localProj()], () => [sess("s1", { aiTitle: "订单重试" }), sess("s2")], (cmd, args) => {
      if (cmd === "list_remote_mcp_origins") return [];
      if (isChanCall(cmd, args, "history-search")) {
        return linesReply([
          { agent: "claude", sessionId: "s1", projectPath: "/l/p", projectName: "p", jsonlPath: "/h/.claude/projects/lp/s1.jsonl", title: "订单重试", updatedAt: 1, hitCount: 0, hits: [] },
        ]);
      }
      if (isChanCall(cmd, args, "history-search-merge")) {
        const rows = (chanArgsJson(args) as { sessions: unknown[] }).sessions;
        return chanReply({ totalHits: 0, sessionCount: rows.length, truncated: false, sessions: rows });
      }
      return undefined;
    });
    const view = new HistoryView();
    await view.open();
    vi.useFakeTimers();
    inner(view).searchInput.value = "订单";
    inner(view).searchInput.dispatchEvent(new Event("input"));
    vi.advanceTimersByTime(300);
    vi.useRealTimers();
    await settle(view);
    const g = projectGroups()[0];
    expect(g, "只在标题里的词搜不到那个项目").toBeDefined();
    await settle(view);
    expect(titles(projectGroups()[0])).toEqual(["订单重试"]);
  });
});
