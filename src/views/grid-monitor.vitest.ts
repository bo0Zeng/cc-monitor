// F91（#27）：多 agent 监控 —— 纯函数（分组/排序/汇总）+ 视图（jsdom：渲染/点击 switchTo/空态/Esc）。
import { describe, it, expect, vi } from "vitest";

// dispatcher overlay 栈桩（同 panel-groups.vitest 范式）——聚焦本视图逻辑，不接真键位派发。
const { pushOverlay, popOverlay } = vi.hoisted(() => ({
  pushOverlay: vi.fn(),
  popOverlay: vi.fn(),
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay, popOverlay },
}));

import {
  groupSessionsByOrigin,
  sortSessionsInGroup,
  summarizeSessions,
  GridMonitorView,
} from "./grid-monitor";
import type { GridSessionSnapshot } from "../session-status";

const snap = (over: Partial<GridSessionSnapshot>): GridSessionSnapshot => ({
  sessionId: "s",
  title: "t",
  origin: null,
  cwd: null,
  status: "live",
  activityStatus: null,
  waitingFor: null,
  runningAgents: 0,
  totalAgents: 0,
  contextPct: null,
  unread: 0,
  kind: null,
  ...over,
});

describe("F91 groupSessionsByOrigin", () => {
  it("本机组恒在最前，远端组按 label 升序，组内保输入序", () => {
    const groups = groupSessionsByOrigin([
      snap({ sessionId: "r-b", origin: "beta" }),
      snap({ sessionId: "l1", origin: null }),
      snap({ sessionId: "r-a", origin: "alpha" }),
      snap({ sessionId: "l2", origin: null }),
      snap({ sessionId: "r-a2", origin: "alpha" }),
    ]);
    expect(groups.map((g) => g.label)).toEqual(["本机", "alpha", "beta"]);
    expect(groups[0].origin).toBeNull();
    expect(groups[0].sessions.map((s) => s.sessionId)).toEqual(["l1", "l2"]); // 保输入序
    expect(groups[1].sessions.map((s) => s.sessionId)).toEqual(["r-a", "r-a2"]);
  });
  it("无本地会话 → 不产出本机组", () => {
    const groups = groupSessionsByOrigin([snap({ origin: "h1" })]);
    expect(groups.map((g) => g.label)).toEqual(["h1"]);
  });
  it("空输入 → 空数组", () => {
    expect(groupSessionsByOrigin([])).toEqual([]);
  });
});

describe("F91 sortSessionsInGroup", () => {
  it("活会话先于归档；活内 waiting>busy>idle/shell>未知；同档稳定", () => {
    const sorted = sortSessionsInGroup([
      snap({ sessionId: "arch", status: "archived" }),
      snap({ sessionId: "idle", activityStatus: "idle" }),
      snap({ sessionId: "unknown", activityStatus: null }),
      snap({ sessionId: "wait", activityStatus: "waiting" }),
      snap({ sessionId: "busy", activityStatus: "busy" }),
      snap({ sessionId: "shell", activityStatus: "shell" }),
    ]);
    expect(sorted.map((s) => s.sessionId)).toEqual([
      "wait",
      "busy",
      "idle",
      "shell",
      "unknown",
      "arch",
    ]);
  });
  it("不改入参（返回新数组）", () => {
    const input = [snap({ sessionId: "a", activityStatus: "idle" }), snap({ sessionId: "b", activityStatus: "busy" })];
    const before = input.map((s) => s.sessionId);
    sortSessionsInGroup(input);
    expect(input.map((s) => s.sessionId)).toEqual(before);
  });
});

describe("F91 summarizeSessions", () => {
  it("机器数（本机算一台）/ 活会话数 / 运行中 agent 总数", () => {
    const r = summarizeSessions([
      snap({ origin: null, status: "live", runningAgents: 2 }),
      snap({ origin: null, status: "archived", runningAgents: 0 }),
      snap({ origin: "h1", status: "live", runningAgents: 1 }),
      snap({ origin: "h1", status: "live", runningAgents: 0 }),
    ]);
    expect(r).toEqual({ machines: 2, liveSessions: 3, runningAgents: 3 });
  });
  it("空 → 全 0", () => {
    expect(summarizeSessions([])).toEqual({ machines: 0, liveSessions: 0, runningAgents: 0 });
  });
});

describe("F91 GridMonitorView", () => {
  const mkSource = (sessions: GridSessionSnapshot[]) => ({
    snapshotSessions: () => sessions,
    switchTo: vi.fn(),
  });

  it("open 渲染分组标题 + 摘要 + cell；F91b 点 cell = 高亮不关（不 switchTo、板保持开）", () => {
    document.body.replaceChildren();
    const source = mkSource([
      snap({ sessionId: "l1", title: "本地会话", origin: null, activityStatus: "busy", runningAgents: 2 }),
      snap({ sessionId: "r1", title: "远端会话", origin: "pi", activityStatus: "waiting", waitingFor: "permission prompt" }),
    ]);
    const view = new GridMonitorView(source);
    view.open();

    expect(view.isVisible()).toBe(true);
    expect(pushOverlay).toHaveBeenCalledWith(view);
    const summary = document.querySelector(".grid-monitor-summary")?.textContent ?? "";
    expect(summary).toContain("2 台机器");
    expect(summary).toContain("2 个活跃会话");
    expect(summary).toContain("2 个 agent 运行中");
    const groupTitles = [...document.querySelectorAll(".grid-monitor-group-title")].map((e) => e.textContent);
    expect(groupTitles).toEqual(["本机（1）", "pi（1）"]);
    let cells = document.querySelectorAll<HTMLElement>(".grid-monitor-cell");
    expect(cells.length).toBe(2);
    expect(document.querySelector(".badge-waiting")?.textContent).toContain("permission prompt");
    // peek 初始收起
    expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(true);

    // F91b：点第一个 cell → 不 switchTo、不关板；该 cell 高亮 + peek 展开
    cells[0].click();
    expect(source.switchTo).not.toHaveBeenCalled();
    expect(view.isVisible()).toBe(true);
    expect(document.querySelector(".grid-monitor")).not.toBeNull();
    cells = document.querySelectorAll<HTMLElement>(".grid-monitor-cell");
    const selected = [...cells].filter((c) => c.classList.contains("is-selected"));
    expect(selected.length).toBe(1);
    const peek = document.querySelector(".grid-monitor-peek");
    expect(peek?.classList.contains("is-empty")).toBe(false);
    expect(document.querySelector(".grid-monitor-peek-title")?.textContent).toBe("本地会话");
    view.close();
  });

  it("F91b peek「跳转到该会话」按钮 → switchTo + close（显式导航）", () => {
    document.body.replaceChildren();
    const source = mkSource([snap({ sessionId: "l1", title: "本地会话" })]);
    const view = new GridMonitorView(source);
    view.open();
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click(); // 选中出 peek
    document.querySelector<HTMLElement>(".grid-monitor-peek-jump")!.click();
    expect(source.switchTo).toHaveBeenCalledWith("l1");
    expect(view.isVisible()).toBe(false);
    expect(popOverlay).toHaveBeenCalledWith(view);
    expect(document.querySelector(".grid-monitor")).toBeNull();
  });

  it("F91b 再点已选 cell → 取消选中（peek 收起）", () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(mkSource([snap({ sessionId: "l1", title: "a" })]));
    view.open();
    const cell = () => document.querySelector<HTMLElement>(".grid-monitor-cell")!;
    cell().click();
    expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(false);
    cell().click(); // toggle off
    expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(true);
    expect(cell().classList.contains("is-selected")).toBe(false);
    view.close();
  });

  it("F91b 选中会话在下轮快照消失 → 自动清选中、收 peek（1Hz 重渲染驱动）", () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let sessions = [snap({ sessionId: "l1", title: "a" }), snap({ sessionId: "l2", title: "b" })];
      const source = { snapshotSessions: () => sessions, switchTo: vi.fn() };
      const view = new GridMonitorView(source);
      view.open();
      // 选中 l1（本机组内 waiting/busy 排序稳定，这里都 idle-null，保输入序 → 第一个是 l1）
      document.querySelectorAll<HTMLElement>(".grid-monitor-cell")[0].click();
      expect(document.querySelector(".grid-monitor-peek-title")?.textContent).toBe("a");
      // l1 消失，下一次 1Hz 渲染
      sessions = [snap({ sessionId: "l2", title: "b" })];
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(true);
      expect([...document.querySelectorAll(".grid-monitor-cell")].some((c) => c.classList.contains("is-selected"))).toBe(false);
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("F91b peekSession 缺省（旧桩）→ peek 仍显 snapshot 字段、不报错", () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(mkSource([snap({ sessionId: "l1", title: "a", cwd: "/x/y", contextPct: 42 })]));
    view.open();
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
    const facts = document.querySelector(".grid-monitor-peek-facts")?.textContent ?? "";
    expect(facts).toContain("/x/y");
    // F91b-fix(round2)：ctx%/未读不再进 peek（在 cell 徽标上实时显示，避免与 peek 滞后值矛盾）
    expect(facts).not.toContain("42%");
    expect(document.querySelector(".badge-ctx")?.textContent).toContain("42%"); // 仍在 cell 徽标上
    // 无 peekSession → 无 agent/文件区
    expect(document.querySelector(".grid-monitor-peek-agents")).toBeNull();
    expect(document.querySelector(".grid-monitor-peek-files")).toBeNull();
    view.close();
  });

  it("F91b peekSession 提供 → peek 显示 model / subagent / 改过的文件", () => {
    document.body.replaceChildren();
    const source = {
      snapshotSessions: () => [snap({ sessionId: "l1", title: "a" })],
      switchTo: vi.fn(),
      peekSession: (sid: string) =>
        sid === "l1"
          ? {
              model: "claude-opus-4-8",
              recentFiles: ["/a/b.ts", "/a/c.ts"],
              agents: [
                { label: "explorer", status: "running" as const },
                { label: "planner", status: "done" as const },
              ],
            }
          : null,
    };
    const view = new GridMonitorView(source);
    view.open();
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
    expect(document.querySelector(".grid-monitor-peek-facts")?.textContent).toContain("claude-opus-4-8");
    const agents = [...document.querySelectorAll(".grid-monitor-peek-agent")].map((e) => e.textContent);
    expect(agents).toContain("explorer");
    expect(agents).toContain("planner");
    const files = [...document.querySelectorAll(".grid-monitor-peek-file")].map((e) => e.textContent);
    expect(files).toEqual(["/a/b.ts", "/a/c.ts"]);
    view.close();
  });

  it("F91b peek 1Hz 重渲染下内容未变 → 不重建（保住选区/滚动）；内容变 → 重建", () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let model = "m1";
      const source = {
        snapshotSessions: () => [snap({ sessionId: "l1", title: "a" })],
        switchTo: vi.fn(),
        peekSession: () => ({ model, recentFiles: [] as string[], agents: [] as never[] }),
      };
      const view = new GridMonitorView(source);
      view.open();
      document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
      const titleNode = document.querySelector(".grid-monitor-peek-title");
      expect(titleNode).not.toBeNull();
      // 无变化 tick：peek 节点原样保留（未被 replaceChildren 清掉）
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek-title")).toBe(titleNode);
      // 内容变（model）→ 重建：节点换新、显新值
      model = "m2";
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek-title")).not.toBe(titleNode);
      expect(document.querySelector(".grid-monitor-peek-facts")?.textContent).toContain("m2");
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("F91b-fix 签名排除 unread/contextPct → 后台会话计数跳动不重建 peek（保住选区）", () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let unread = 0;
      let contextPct = 40;
      const source = {
        snapshotSessions: () => [snap({ sessionId: "l1", title: "a", unread, contextPct })],
        switchTo: vi.fn(),
        peekSession: () => ({ model: "m", recentFiles: [] as string[], agents: [] as never[] }),
      };
      const view = new GridMonitorView(source);
      view.open();
      document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
      const titleNode = document.querySelector(".grid-monitor-peek-title");
      expect(titleNode).not.toBeNull();
      // 后台会话来新消息 + context 涨 —— 这俩是高频 glance 字段，不该触发 peek 重建（否则清掉用户选区）
      unread = 7;
      contextPct = 55;
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek-title")).toBe(titleNode); // 同一节点 = 未重建
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("F91b-fix 签名只签可见 slice(8)+计数 → 第 9+ 个文件变化不重建（但数量变会重建）", () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      // 12 个文件，只显后 8 个（idx 4..11）。改前 4 个（不可见）不该重建；数量变则重建。
      let files = Array.from({ length: 12 }, (_, i) => `/f${i}.ts`);
      const source = {
        snapshotSessions: () => [snap({ sessionId: "l1", title: "a" })],
        switchTo: vi.fn(),
        peekSession: () => ({ model: "m", recentFiles: files, agents: [] as never[] }),
      };
      const view = new GridMonitorView(source);
      view.open();
      document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
      const titleNode = document.querySelector(".grid-monitor-peek-title");
      // 改不可见的前 4 个（可见 slice(-8) 与计数都不变）→ 不重建
      files = ["/x0.ts", "/x1.ts", "/x2.ts", "/x3.ts", ...files.slice(4)];
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek-title")).toBe(titleNode);
      // 数量变（新增一个可见文件）→ 重建
      files = [...files, "/new.ts"];
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek-title")).not.toBe(titleNode);
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("空会话 → 空态文案、摘要空", () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(mkSource([]));
    view.open();
    expect(document.querySelector(".grid-monitor-empty")?.textContent).toContain("暂无会话");
    expect(document.querySelector(".grid-monitor-summary")?.textContent).toBe("");
    view.close();
  });

  it("context% ≥80 加 is-high；<80 不加", () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(
      mkSource([
        snap({ sessionId: "hot", contextPct: 88 }),
        snap({ sessionId: "cool", contextPct: 30 }),
      ]),
    );
    view.open();
    const ctx = [...document.querySelectorAll<HTMLElement>(".badge-ctx")];
    expect(ctx.length).toBe(2);
    const hot = ctx.find((e) => e.textContent === "ctx 88%");
    const cool = ctx.find((e) => e.textContent === "ctx 30%");
    expect(hot?.classList.contains("is-high")).toBe(true);
    expect(cool?.classList.contains("is-high")).toBe(false);
    view.close();
  });

  it("handleEsc → close", () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(mkSource([snap({})]));
    view.open();
    expect(view.isVisible()).toBe(true);
    view.handleEsc();
    expect(view.isVisible()).toBe(false);
  });
});

describe("F91 GridMonitorView interval 生命周期", () => {
  it("open 启 1Hz 轮询重渲染；close 停 timer（不再拉快照）", () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let sessions: GridSessionSnapshot[] = [snap({ sessionId: "a", title: "A" })];
      const source = { snapshotSessions: vi.fn(() => sessions), switchTo: vi.fn() };
      const view = new GridMonitorView(source);
      view.open();
      expect(source.snapshotSessions).toHaveBeenCalledTimes(1); // open 首渲
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(1);

      // 数据变了 → 下一 tick 重渲染
      sessions = [snap({ sessionId: "a", title: "A" }), snap({ sessionId: "b", title: "B" })];
      vi.advanceTimersByTime(1000);
      expect(source.snapshotSessions).toHaveBeenCalledTimes(2);
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(2);

      // close 后 timer 停 → 再多久都不拉快照
      view.close();
      const callsAtClose = source.snapshotSessions.mock.calls.length;
      vi.advanceTimersByTime(5000);
      expect(source.snapshotSessions.mock.calls.length).toBe(callsAtClose);
    } finally {
      vi.useRealTimers();
    }
  });

  it("双开/双关幂等（不叠 interval、pushOverlay/popOverlay 各一次）", () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      pushOverlay.mockClear();
      popOverlay.mockClear();
      const source = { snapshotSessions: vi.fn(() => [] as GridSessionSnapshot[]), switchTo: vi.fn() };
      const view = new GridMonitorView(source);
      view.open();
      view.open(); // 双开 no-op（isOpen 守卫，不叠第二个 interval）
      expect(pushOverlay).toHaveBeenCalledTimes(1);
      // 单一 interval：一 tick 只多一次拉取
      const before = source.snapshotSessions.mock.calls.length;
      vi.advanceTimersByTime(1000);
      expect(source.snapshotSessions.mock.calls.length).toBe(before + 1);
      view.close();
      view.close(); // 双关 no-op
      expect(popOverlay).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
