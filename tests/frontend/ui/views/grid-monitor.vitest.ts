// F91（#27）：多 agent 监控视图（jsdom：版面照后端那一份摆 / 渲染 / 点击 switchTo / 空态 / Esc / 差量更新）。
import { describe, it, expect, vi } from "vitest";

// dispatcher overlay 栈桩（同 panel-groups.vitest 范式）——聚焦本视图逻辑，不接真键位派发。
const { pushOverlay, popOverlay } = vi.hoisted(() => ({
  pushOverlay: vi.fn(),
  popOverlay: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay, popOverlay },
}));

import { GridMonitorView, type GridSource } from "../../../../src/frontend/ui/views/grid-monitor";
import type { GridSessionSnapshot, SessionPeek } from "../../../../src/frontend/ui/session-status";
import type { MonitorBoard } from "../../../../src/frontend/ui/generated/MonitorBoard";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import { machineName } from "../../../../src/frontend/ui/control-said";
import { goldenFace, liveFace } from "../../../test-support/session-faces";
import type { SessionFace } from "../../../../src/frontend/ui/generated/SessionFace";
import { copyText } from "../../../../src/frontend/ui/copy-table";

/** 那张脸换一个序。 */
const ordered = (f: SessionFace, order: number): SessionFace => ({ ...f, order });
const snap = (over: Partial<GridSessionSnapshot>): GridSessionSnapshot => ({
  sessionId: "s",
  title: "t",
  origin: LOCAL,
  cwd: null,
  face: liveFace("working"),
  needs: null,
  running: null,
  agents: null,
  context: null,
  unread: 0,
  background: false,
  backgroundWork: null,
  account: null,
  ...over,
});

/** 等那一问回来（桩是 async，几拍微任务就落地；假钟下也走得动）。 */
const settle = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

/**
 * 本机后端那一份的替身（夹具）：按机器首见的先后分组、组内按脸上的序、组头与顶上那一句写成「核心·…」。
 * 只为给界面一份成品；真的排法 · 字住后端判据（`tests/backend/control/monitor_board_tests.rs`）。
 */
function stubBoard(sessions: readonly GridSessionSnapshot[]): MonitorBoard {
  const by = new Map<string, GridSessionSnapshot[]>();
  for (const s of sessions) {
    const a = by.get(s.origin);
    if (a) a.push(s);
    else by.set(s.origin, [s]);
  }
  return {
    summary: sessions.length === 0 ? "" : `核心·汇总 ${sessions.length}`,
    groups: [...by].map(([machine, ss]) => ({
      machine,
      title: `核心·${machine}（${ss.length}）`,
      sids: [...ss].sort((a, b) => a.face.order - b.face.order).map((s) => s.sessionId),
    })),
  };
}

/** 数据源桩：快照每次照 `get()` 取，版面那一问照同一份快照给替身。 */
const mkSource = (get: () => GridSessionSnapshot[], peekSession?: (sid: string) => SessionPeek | null) => ({
  snapshotSessions: get,
  switchTo: vi.fn(),
  board: vi.fn(async () => stubBoard(get())),
  ...(peekSession ? { peekSession } : {}),
});
const fixed = (sessions: GridSessionSnapshot[]) => mkSource(() => sessions);

/** 开板并等版面落地。 */
const openSettled = async (view: GridMonitorView): Promise<void> => {
  view.open();
  await settle();
};

describe("F91 GridMonitorView", () => {
  it("open 渲染分组标题 + 摘要 + cell；F91b 点 cell = 高亮不关（不 switchTo、板保持开）", async () => {
    document.body.replaceChildren();
    const source = fixed([
      snap({ sessionId: "l1", title: "本地会话", origin: LOCAL, running: "核心·在跑 2", face: liveFace("working") }),
      snap({ sessionId: "r1", title: `${copyText("rsConfigSurface.host.remote")}${copyText("commandBar.group.sessions")}`, origin: "pi", needs: "核心·等批准", face: liveFace("needs_you") }),
    ]);
    const view = new GridMonitorView(source);
    await openSettled(view);

    expect(view.isVisible()).toBe(true);
    expect(pushOverlay).toHaveBeenCalledWith(view);
    expect(document.querySelector(".grid-monitor-summary")?.textContent).toBe("核心·汇总 2");
    const groupTitles = [...document.querySelectorAll(".grid-monitor-group-title")].map((e) => e.textContent);
    expect(groupTitles).toEqual([`核心·${LOCAL}（1）`, "核心·pi（1）"]);
    let cells = document.querySelectorAll<HTMLElement>(".grid-monitor-cell");
    expect(cells.length).toBe(2);
    expect(document.querySelector(".badge-waiting")?.textContent).toBe("核心·等批准");
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
    // 本机远端一律写那台的名字（同一个名字来处）
    expect(document.querySelector(".grid-monitor-peek-origin")?.textContent).toBe(machineName(LOCAL));
    cells[1].click();
    expect(document.querySelector(".grid-monitor-peek-origin")?.textContent).toBe(machineName("pi"));
    view.close();
  });

  it("★ 格子的点与标签栏同一个点：点与名字照抄脸上那两格（空闲不画成红）", async () => {
    document.body.replaceChildren();
    const source = fixed([
      snap({ sessionId: "gi", title: "灰会话", origin: "pi", face: goldenFace("reconnectable") }),
      snap({ sessionId: "id", title: "闲", face: { ...liveFace("idle"), name: "核心·空闲" } }),
      snap({ sessionId: "bg", title: "bg", face: { ...liveFace("background_work"), name: "核心·后台" } }),
      snap({ sessionId: "nk", title: "说不清", face: { ...liveFace(null), name: "核心·说不清" } }),
    ]);
    const view = new GridMonitorView(source);
    await openSettled(view);
    const dot = (sid: string): HTMLElement => document.querySelector<HTMLElement>(`.grid-monitor-cell[data-sid="${sid}"] [role=img]`)!;
    expect(dot("gi").dataset.state).toBe("exited");
    expect([dot("id").dataset.state, dot("id").title]).toEqual(["idle", "核心·空闲"]);
    expect([dot("bg").dataset.state, dot("bg").title], "后台任务运行中：单独一色").toEqual(["background", "核心·后台"]);
    expect([dot("nk").dataset.state, dot("nk").title], "判不出在干什么：那台给的那一颗").toEqual([goldenFace("live-unclear").dot, "核心·说不清"]);
    expect(document.querySelector(".live-dot")).toBeNull();
    view.close();
  });

  it("后台任务运行中：徽标写命令那一格（核心写的），悬停给整句、时长按此刻走；不是这一态不出", async () => {
    document.body.replaceChildren();
    const from = Date.now() - 12 * 60_000 - 5_000;
    const bw = { text: "x", clock: { text: "L · {dur}", from }, what: "make test-all", count: 1, tone: "busy" as const };
    const source = fixed([
      snap({ sessionId: "bw", origin: "pi", backgroundWork: bw, face: liveFace("background_work") }),
      snap({ sessionId: "id", origin: "pi", backgroundWork: bw, face: liveFace("idle") }),
    ]);
    const view = new GridMonitorView(source);
    await openSettled(view);
    const badge = document.querySelector<HTMLElement>('.grid-monitor-cell[data-sid="bw"] .badge-bgwork')!;
    expect(badge.textContent).toBe(copyText("gridMonitor.renderBadges.background", { cmd: "make test-all" }));
    expect(badge.title).toBe("L · 12m");
    expect(document.querySelector('.grid-monitor-cell[data-sid="id"] .badge-bgwork')).toBeNull();
    view.close();
  });

  it("F91b peek「跳转到该会话」按钮 → switchTo + close（显式导航）", async () => {
    document.body.replaceChildren();
    const source = fixed([snap({ sessionId: "l1", title: "本地会话" })]);
    const view = new GridMonitorView(source);
    await openSettled(view);
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click(); // 选中出 peek
    document.querySelector<HTMLElement>(".grid-monitor-peek-jump")!.click();
    expect(source.switchTo).toHaveBeenCalledWith("l1");
    expect(view.isVisible()).toBe(false);
    expect(popOverlay).toHaveBeenCalledWith(view);
    expect(document.querySelector(".grid-monitor")).toBeNull();
  });

  it("F91b 再点已选 cell → 取消选中（peek 收起）", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(fixed([snap({ sessionId: "l1", title: "a" })]));
    await openSettled(view);
    const cell = () => document.querySelector<HTMLElement>(".grid-monitor-cell")!;
    cell().click();
    expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(false);
    cell().click(); // toggle off
    expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(true);
    expect(cell().classList.contains("is-selected")).toBe(false);
    view.close();
  });

  it("F91b 选中会话在下轮快照消失 → 自动清选中、收 peek、格子摘掉（1Hz 重画驱动，不等下一问）", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let sessions = [snap({ sessionId: "l1", title: "a" }), snap({ sessionId: "l2", title: "b" })];
      const source = mkSource(() => sessions);
      const view = new GridMonitorView(source);
      await openSettled(view);
      document.querySelectorAll<HTMLElement>(".grid-monitor-cell")[0].click();
      expect(document.querySelector(".grid-monitor-peek-title")?.textContent).toBe("a");
      // l1 消失，下一次 1Hz 渲染
      sessions = [snap({ sessionId: "l2", title: "b" })];
      vi.advanceTimersByTime(1000);
      expect(document.querySelector(".grid-monitor-peek")?.classList.contains("is-empty")).toBe(true);
      expect([...document.querySelectorAll(".grid-monitor-cell")].some((c) => c.classList.contains("is-selected"))).toBe(false);
      expect([...document.querySelectorAll<HTMLElement>(".grid-monitor-cell")].map((c) => c.dataset.sid)).toEqual(["l2"]);
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("上限判不出（核心没给百分比）⇒ 格子上照抄核心那一格的字，不标红，悬停说上限未知", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(fixed([snap({ sessionId: "l1", context: { text: "核心·350k", tone: "plain", percent: null } })]));
    await openSettled(view);
    const b = document.querySelector<HTMLElement>(".badge-ctx");
    expect(b?.textContent).toBe(copyText("gridMonitor.renderBadges.ctx", { ctx: "核心·350k" }));
    expect(b?.title).toBe(copyText("gridMonitor.renderBadges.ctxTokensHint"));
    expect(b?.classList.contains("is-high")).toBe(false);
    view.close();
  });

  it("F91b peekSession 缺省（旧桩）→ peek 仍显 snapshot 字段、不报错", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(fixed([snap({ sessionId: "l1", title: "a", cwd: "/x/y", context: { text: "42%", tone: "plain", percent: 42 } })]));
    await openSettled(view);
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

  it("F91b peekSession 提供 → peek 显示 model / subagent / 改过的文件", async () => {
    document.body.replaceChildren();
    const source = mkSource(
      () => [snap({ sessionId: "l1", title: "a" })],
      (sid) =>
        sid === "l1"
          ? {
              model: "claude-opus-4-8",
              recentFiles: ["/a/b.ts", "/a/c.ts"],
              summary: "核心·agent 2 · 1 在跑",
              more: null,
              agents: [
                { label: "explorer", status: "running" as const },
                { label: "planner", status: "done" as const },
              ],
            }
          : null,
    );
    const view = new GridMonitorView(source);
    await openSettled(view);
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
    expect(document.querySelector(".grid-monitor-peek-facts")?.textContent).toContain("claude-opus-4-8");
    const agents = [...document.querySelectorAll(".grid-monitor-peek-agent")].map((e) => e.textContent);
    expect(agents).toContain("explorer");
    expect(agents).toContain("planner");
    const files = [...document.querySelectorAll(".grid-monitor-peek-file")].map((e) => e.textContent);
    expect(files).toEqual(["/a/b.ts", "/a/c.ts"]);
    view.close();
  });

  it("F91b peek 1Hz 重渲染下内容未变 → 不重建（保住选区/滚动）；内容变 → 重建", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let model = "m1";
      const source = mkSource(
        () => [snap({ sessionId: "l1", title: "a" })],
        () => ({ model, recentFiles: [] as string[], agents: [] as never[], summary: null, more: null }),
      );
      const view = new GridMonitorView(source);
      await openSettled(view);
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

  it("F91b-fix 签名排除 unread/contextPct → 后台会话计数跳动不重建 peek（保住选区）", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let unread = 0;
      let contextPct = 40;
      const source = mkSource(
        () => [snap({ sessionId: "l1", title: "a", unread, context: { text: `${contextPct}%`, tone: "plain", percent: contextPct } })],
        () => ({ model: "m", recentFiles: [] as string[], agents: [] as never[], summary: null, more: null }),
      );
      const view = new GridMonitorView(source);
      await openSettled(view);
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

  it("F91b-fix 签名只签可见 slice(8)+计数 → 第 9+ 个文件变化不重建（但数量变会重建）", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      // 12 个文件，只显后 8 个（idx 4..11）。改前 4 个（不可见）不该重建；数量变则重建。
      let files = Array.from({ length: 12 }, (_, i) => `/f${i}.ts`);
      const source = mkSource(
        () => [snap({ sessionId: "l1", title: "a" })],
        () => ({ model: "m", recentFiles: files, agents: [] as never[], summary: null, more: null }),
      );
      const view = new GridMonitorView(source);
      await openSettled(view);
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

  it("空会话 → 空态文案、摘要空", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(fixed([]));
    await openSettled(view);
    expect(document.querySelector(".grid-monitor-empty")?.textContent).toContain(copyText("gridMonitor.render.empty"));
    expect(document.querySelector(".grid-monitor-summary")?.textContent).toBe("");
    view.close();
  });

  it("核心说 warn 才加 is-high（界面不按百分比自己判）", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(
      fixed([
        snap({ sessionId: "hot", context: { text: "核心·88", tone: "warn", percent: 88 } }),
        snap({ sessionId: "cool", context: { text: "核心·95", tone: "plain", percent: 95 } }),
      ]),
    );
    await openSettled(view);
    const ctx = [...document.querySelectorAll<HTMLElement>(".badge-ctx")];
    expect(ctx.length).toBe(2);
    const hot = ctx.find((e) => e.textContent === copyText("gridMonitor.renderBadges.ctx", { ctx: "核心·88" }));
    const cool = ctx.find((e) => e.textContent === copyText("gridMonitor.renderBadges.ctx", { ctx: "核心·95" }));
    expect(hot?.classList.contains("is-high")).toBe(true);
    expect(cool?.classList.contains("is-high")).toBe(false);
    view.close();
  });

  it("在跑 agent 的徽标照抄后端写好的那一句（界面不数在跑的、不拼字）；没有那一句 ⇒ 不画", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(fixed([snap({ sessionId: "l1", running: "核心·在跑 · agent 5", agents: "agent 448 · 2 在跑" }), snap({ sessionId: "l2" })]));
    await openSettled(view);
    expect([...document.querySelectorAll(".badge-agents")].map((e) => e.textContent)).toEqual(["核心·在跑 · agent 5"]);
    // 悬停那一句照抄后端写好的那一句（总数后端照实数，界面不拿运行表的长度自己拼）。
    expect((document.querySelector(".badge-agents") as HTMLElement | null)?.title).toBe("agent 448 · 2 在跑");
    view.close();
  });

  it("handleEsc → close", async () => {
    document.body.replaceChildren();
    const view = new GridMonitorView(fixed([snap({})]));
    await openSettled(view);
    expect(view.isVisible()).toBe(true);
    view.handleEsc();
    expect(view.isVisible()).toBe(false);
  });
});

// ═══ 版面照本机后端那份 `MonitorBoard`：组序 · 组头 · 组内序 · 顶上那一句界面一概不算 ═══
describe("监控板照后端成品摆", () => {
  it("组序 · 组头 · 组内序 · 顶上那一句照 board() 回的那一份（与 origin 字典序 · 脸上的序都相反也照摆）", async () => {
    document.body.replaceChildren();
    const sessions = [
      snap({ sessionId: "a1", title: "甲一", origin: "alpha", face: ordered(liveFace("needs_you"), 0) }),
      snap({ sessionId: "z1", title: "末一", origin: "zeta", face: ordered(liveFace("idle"), 3) }),
      snap({ sessionId: "z2", title: "末二", origin: "zeta", face: ordered(liveFace("needs_you"), 0) }),
    ];
    const board = vi.fn(async () => ({
      summary: "后端·两台三条",
      groups: [
        { machine: "zeta", title: "工作站 B（1）", sids: ["z1", "z2"] },
        { machine: "alpha", title: "alpha 机（2）", sids: ["a1"] },
      ],
    }));
    const view = new GridMonitorView({ snapshotSessions: () => sessions, switchTo: vi.fn(), board });
    await openSettled(view);
    expect(board).toHaveBeenCalledWith([
      { machine: "alpha", sid: "a1" },
      { machine: "zeta", sid: "z1" },
      { machine: "zeta", sid: "z2" },
    ]);
    const titles = [...document.querySelectorAll(".grid-monitor-group-title")].map((e) => e.textContent);
    expect(titles).toEqual(["工作站 B（1）", "alpha 机（2）"]);
    const order = [...document.querySelectorAll<HTMLElement>(".grid-monitor-cell")].map((c) => c.dataset.sid);
    expect(order).toEqual(["z1", "z2", "a1"]);
    expect(document.querySelector(".grid-monitor-summary")?.textContent).toBe("后端·两台三条");
    view.close();
  });

  it("peek：头那一句照抄 summary、chip 照给的三组序摆（在跑的排在后面也不重排）", async () => {
    document.body.replaceChildren();
    const sessions = [snap({ sessionId: "l1", title: "a" })];
    const view = new GridMonitorView({
      snapshotSessions: () => sessions,
      switchTo: vi.fn(),
      board: async () => ({ summary: "", groups: [{ machine: LOCAL, title: "本机·1", sids: ["l1"] }] }),
      peekSession: () => ({
        model: null,
        recentFiles: [],
        summary: "核心·agent 9 · 1 在跑", // 总数后端照实数（连被挤出表的），与界面手上那三条对不上
        more: null,
        agents: [
          { label: "done1", status: "done" as const },
          { label: "stop1", status: "stopped" as const },
          { label: "run1", status: "running" as const },
        ],
      }),
    });
    await openSettled(view);
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
    expect(document.querySelector(".grid-monitor-peek-agents .grid-monitor-peek-k")?.textContent).toBe("核心·agent 9 · 1 在跑");
    expect([...document.querySelectorAll(".grid-monitor-peek-agent")].map((e) => e.textContent)).toEqual(["done1", "stop1", "run1"]);
    expect(document.querySelector(".grid-monitor-peek-agents .grid-monitor-peek-more")).toBeNull();
    view.close();
  });

  it("peek：chip 只照后端挑好的那几个画（不截）、「+N」照抄后端那一句（不拿总数减）", async () => {
    document.body.replaceChildren();
    const sessions = [snap({ sessionId: "l1", title: "a" })];
    const agents = Array.from({ length: 10 }, (_, i) => ({ label: `a${i}`, status: "done" as const }));
    const view = new GridMonitorView({
      snapshotSessions: () => sessions,
      switchTo: vi.fn(),
      board: async () => ({ summary: "", groups: [{ machine: LOCAL, title: "本机·1", sids: ["l1"] }] }),
      peekSession: () => ({ model: null, recentFiles: [], summary: "核心·agent 14", more: "核心·+4", agents }),
    });
    await openSettled(view);
    document.querySelector<HTMLElement>(".grid-monitor-cell")!.click();
    expect(document.querySelectorAll(".grid-monitor-peek-agent")).toHaveLength(10);
    expect(document.querySelector(".grid-monitor-peek-agents .grid-monitor-peek-more")?.textContent).toBe("核心·+4");
    view.close();
  });

  it("那台没答 ⇒ 那一组带它的那一句，格子照列；答上了那一句摘掉", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      const sessions = [snap({ sessionId: "p1", origin: "pi" })];
      let down: string | undefined = "核心·pi 离线";
      const view = new GridMonitorView({
        snapshotSessions: () => sessions,
        switchTo: vi.fn(),
        board: async () => ({ summary: "核心·0 台", groups: [{ machine: "pi", title: "核心·pi（1）", sids: ["p1"], ...(down ? { down } : {}) }] }),
      });
      await openSettled(view);
      expect(document.querySelector(".grid-monitor-group [data-down]")?.textContent).toBe("核心·pi 离线");
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(1);
      down = undefined;
      view.poke();
      vi.advanceTimersByTime(1000);
      await settle();
      expect(document.querySelector(".grid-monitor-group [data-down]")).toBeNull();
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("头一问就失败 ⇒ 空态写那一句；有过一份之后再失败 ⇒ 留着上一份", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      const sessions = [snap({ sessionId: "l1" })];
      let fail = true;
      const view = new GridMonitorView({
        snapshotSessions: () => sessions,
        switchTo: vi.fn(),
        board: async () => {
          if (fail) throw new Error("核心·本机后端没答");
          return stubBoard(sessions);
        },
      });
      await openSettled(view);
      expect(document.querySelector(".grid-monitor-empty")?.textContent).toBe("核心·本机后端没答");
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(0);
      fail = false;
      view.poke();
      vi.advanceTimersByTime(1000);
      await settle();
      expect(document.querySelector(".grid-monitor-empty")).toBeNull();
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(1);
      fail = true;
      view.poke();
      vi.advanceTimersByTime(1000);
      await settle();
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(1);
      expect(document.querySelector(".grid-monitor-summary")?.textContent).toBe("核心·汇总 1");
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("F91 GridMonitorView 问版面的时机 · interval 生命周期", () => {
  it("开板问一次；1Hz 只重画格子不问；连着几次 poke 合成一问；新来的会话等那一问回来才摆", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      let sessions: GridSessionSnapshot[] = [snap({ sessionId: "a", title: "A" })];
      const source = mkSource(() => sessions);
      const view = new GridMonitorView(source);
      await openSettled(view);
      expect(source.board).toHaveBeenCalledTimes(1);
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(1);

      sessions = [snap({ sessionId: "a", title: "A" }), snap({ sessionId: "b", title: "B" })];
      vi.advanceTimersByTime(3000);
      await settle();
      expect(source.board, "每秒重画不问版面").toHaveBeenCalledTimes(1);
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(1);

      view.poke();
      view.poke();
      view.poke();
      vi.advanceTimersByTime(1000);
      await settle();
      expect(source.board, "三次 poke 合成一问").toHaveBeenCalledTimes(2);
      expect(document.querySelectorAll(".grid-monitor-cell").length).toBe(2);
      view.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it("关着 poke 不问；close 停 timer（不再拉快照）", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      const sessions: GridSessionSnapshot[] = [snap({ sessionId: "a", title: "A" })];
      const snapshotSessions = vi.fn(() => sessions);
      const source = { ...mkSource(() => sessions), snapshotSessions };
      const view = new GridMonitorView(source);
      view.poke();
      vi.advanceTimersByTime(1000);
      expect(source.board).not.toHaveBeenCalled();
      await openSettled(view);
      view.close();
      const callsAtClose = snapshotSessions.mock.calls.length;
      view.poke();
      vi.advanceTimersByTime(5000);
      expect(snapshotSessions.mock.calls.length).toBe(callsAtClose);
      expect(source.board).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("双开/双关幂等（不叠 interval、pushOverlay/popOverlay 各一次）", async () => {
    vi.useFakeTimers();
    try {
      document.body.replaceChildren();
      pushOverlay.mockClear();
      popOverlay.mockClear();
      const snapshotSessions = vi.fn(() => [] as GridSessionSnapshot[]);
      const view = new GridMonitorView({ ...mkSource(() => []), snapshotSessions });
      await openSettled(view);
      view.open(); // 双开 no-op（isOpen 守卫，不叠第二个 interval）
      expect(pushOverlay).toHaveBeenCalledTimes(1);
      // 单一 interval：一 tick 只多一次拉取
      const before = snapshotSessions.mock.calls.length;
      vi.advanceTimersByTime(1000);
      expect(snapshotSessions.mock.calls.length).toBe(before + 1);
      view.close();
      view.close(); // 双关 no-op
      expect(popOverlay).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

// ═══ 机器总览按行更新：1Hz 那一下只写真变了的格子 ═══════════════════
// 量具：`MutationObserver`（subtree ＋ attributes ＋ childList ＋ characterData）收到的记录条数，
// `takeRecords()` 同步取。判据全是相等（设计与读数住）。
describe("UP1 机器总览按行更新", () => {
  /** 三台机器、七个会话；`snapshotSessions` 每拍都返回**新对象**（与 `TabManager.snapshotSessions` 同形）。 */
  const base = (): GridSessionSnapshot[] => [
    snap({ sessionId: "l1", title: "本机一", cwd: "/w/a", running: "核心·在跑 2", agents: "agent 3 · 2 在跑", face: ordered(liveFace("working"), 1) }),
    snap({ sessionId: "l2", title: "本机二", cwd: "/w/b", context: { text: "85%", tone: "warn", percent: 85 }, unread: 4 }),
    snap({ sessionId: "l3", title: "本机三", face: goldenFace("ended") }),
    snap({ sessionId: "p1", title: "派一", origin: "pi", needs: "核心·等批准", face: ordered(liveFace("needs_you"), 0) }),
    snap({ sessionId: "p2", title: "派二", origin: "pi", background: true, face: goldenFace("reconnectable") }),
    snap({ sessionId: "n1", title: "诺一", origin: "nano", cwd: "/srv", unread: 120 }),
    snap({ sessionId: "n2", title: "诺二", origin: "nano", context: { text: "12%", tone: "plain", percent: 12 } }),
  ];
  const sourceOf = (mutate: () => (s: GridSessionSnapshot[]) => void): GridSource => {
    const get = (): GridSessionSnapshot[] => {
      const s = base();
      mutate()(s);
      return s;
    };
    return mkSource(get);
  };
  /** 宿主转来「变了」⇒ 再问一次版面、等它落地。 */
  const repoll = async (view: GridMonitorView): Promise<void> => {
    view.poke();
    vi.advanceTimersByTime(1000);
    await settle();
  };
  const setup = async (): Promise<{
    view: GridMonitorView;
    set: (f: (s: GridSessionSnapshot[]) => void) => void;
    take: () => MutationRecord[];
    cellOf: (sid: string) => HTMLElement;
    done: () => void;
  }> => {
    vi.useFakeTimers();
    document.body.replaceChildren();
    let mutate: (s: GridSessionSnapshot[]) => void = () => {};
    const view = new GridMonitorView(sourceOf(() => mutate));
    await openSettled(view);
    const root = document.querySelector(".grid-monitor")!;
    const mo = new MutationObserver(() => {});
    mo.observe(root, { subtree: true, attributes: true, childList: true, characterData: true });
    return {
      view,
      set: (f) => {
        mutate = f;
      },
      take: () => mo.takeRecords(),
      cellOf: (sid) => document.querySelector<HTMLElement>(`.grid-monitor-cell[data-sid="${sid}"]`)!,
      done: () => {
        mo.disconnect();
        view.close();
        vi.useRealTimers();
      },
    };
  };

  it("快照没变的一拍 ⇒ 整块（摘要 ＋ 格子 ＋ peek）0 条 DOM 写；连跑 5 拍也是 0；再问一次版面没变也是 0", async () => {
    const t = await setup();
    try {
      expect(document.querySelectorAll(".grid-monitor-cell").length, "前置：七格都在").toBe(7);
      expect(document.querySelectorAll(".grid-monitor-badge").length, "前置：徽标真画出来了").toBe(6);
      t.take();
      vi.advanceTimersByTime(5000);
      expect(t.take().length).toBe(0);
      await repoll(t.view);
      expect(t.take().length).toBe(0);
    } finally {
      t.done();
    }
  });

  it("只有一个会话的未读数变了 ⇒ 恰好 1 条记录，落在那一格上（两向：别的格子 0 条）", async () => {
    const t = await setup();
    try {
      t.take();
      t.set((s) => {
        s[1].unread = 5;
      });
      vi.advanceTimersByTime(1000);
      const recs = t.take();
      expect(recs.length).toBe(1);
      expect(recs[0].target).toBe(t.cellOf("l2"));
      expect(t.cellOf("l2").querySelector(".badge-unread")?.textContent).toBe("5");
    } finally {
      t.done();
    }
  });

  it("按行 ＝ 格子跨拍留着：换了几处内容、版面里的顺序也变了，七个格子仍是同七个 DOM 对象", async () => {
    const t = await setup();
    try {
      const before = new Map(["l1", "l2", "l3", "p1", "p2", "n1", "n2"].map((sid) => [sid, t.cellOf(sid)]));
      t.set((s) => {
        s[1].face = ordered(liveFace("needs_you"), 0); // l2 排到本机组最前
        s[1].needs = "核心·等批准";
        s[3].face = ordered(liveFace("working"), 1); // p1 不再等
        s[3].needs = null;
        s[5].cwd = null; // n1 的 cwd 行摘掉
        s[6].unread = 1;
      });
      await repoll(t.view);
      for (const [sid, el] of before) expect(t.cellOf(sid), sid).toBe(el);
      const order = [...document.querySelectorAll<HTMLElement>(".grid-monitor-cell")].map((c) => c.dataset.sid);
      expect(order).toEqual(["l2", "l1", "l3", "p1", "p2", "n1", "n2"]); // 组序照版面（替身按首见的先后）
      expect(t.cellOf("n1").querySelector(".grid-monitor-cell-cwd")).toBeNull();
      expect(t.cellOf("p1").querySelector(".badge-waiting")).toBeNull();
      expect(t.cellOf("l2").querySelector(".badge-waiting")?.textContent).toBe("核心·等批准");
    } finally {
      t.done();
    }
  });

  it("每拍重建出来的样子 == 从零建出来的样子（差量更新不漂：同一份快照，两条路 DOM 逐字相等）", async () => {
    const t = await setup();
    try {
      const steps: ((s: GridSessionSnapshot[]) => void)[] = [
        (s) => {
          s[0].running = null;
          s[2].face = liveFace("working");
        },
        (s) => {
          s.splice(4, 1); // p2 没了
          s[1].context = null;
          s.push(snap({ sessionId: "z1", title: "新机", origin: "zeta", cwd: "/z", unread: 2 }));
        },
        (s) => {
          s.splice(3, 2); // pi 整组没了
          s[0].title = "改名了";
          s[0].cwd = null;
        },
        (s) => {
          s.length = 0; // 空态
        },
        () => {}, // 从空态回来
      ];
      for (const f of steps) {
        t.set(f);
        await repoll(t.view);
        const incremental = document.querySelector(".grid-monitor-body")!.innerHTML;
        const fresh = new GridMonitorView(sourceOf(() => f));
        await openSettled(fresh);
        const bodies = document.querySelectorAll(".grid-monitor-body");
        expect(bodies.length).toBe(2);
        expect(incremental).toBe(bodies[1].innerHTML);
        fresh.close();
      }
    } finally {
      t.done();
    }
  });

  it("键盘焦点在一格上、别的格子在变 ⇒ 焦点不动；它自己被挪了位置 ⇒ 焦点跟着回到它", async () => {
    const t = await setup();
    try {
      t.cellOf("n2").focus();
      t.set((s) => {
        s[0].unread = 9;
      });
      vi.advanceTimersByTime(1000);
      expect(document.activeElement).toBe(t.cellOf("n2"));
      t.set((s) => {
        s[6].face = ordered(liveFace("needs_you"), 0); // n2 排到 nano 组最前（被 insertBefore 挪动）
        s[6].needs = "x";
      });
      await repoll(t.view);
      expect(document.activeElement).toBe(t.cellOf("n2"));
    } finally {
      t.done();
    }
  });
});
