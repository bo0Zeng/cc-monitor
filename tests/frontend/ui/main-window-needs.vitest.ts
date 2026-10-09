/**
 * 主窗口的标签页栏 · 「需要你」· 会话头：一个会话读成什么、谁在等你、去哪答。
 *
 * - `session-face.ts`：状态点 · 状态句 · peek · 需要你（活动信号说不在等了 ⇒ 手上那份当场不认；说在等、种类没到 ⇒ 不猜）· `Ctrl+J` 的顺序；
 * - 标签页栏：「需要你 N」只在 N ≥ 1 时出、点它跳等得最久的；机器离线条（`{machine} 离线 · N 会话状态不明`）＋［重新连接］；
 * - 钉条：种类 ＋ 工具 ＋ 那一句、去哪答（Windows ↗ / 远端 tmux / 都不行不给按钮）；窗口标题与系统通知；
 * - 会话头：按状态多一颗（已结束［恢复 ▾］· Claude 已退出（远端）［在终端里打开］· 状态不明［重新连接］）。
 *
 * 夹具只造结构（sid · 路径 · 占位工具名），不采会话正文。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { TabStore } from "../../../src/frontend/ui/tab-store";
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { Needs } from "../../../src/frontend/ui/session-reads";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ENDED, GONE, LIVE, LIVE_ATTACHABLE, LIVE_RESUMABLE, RECONNECTABLE, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import { abbrOf, dotOf, fullTitle, needsOf, needsOrder, nextNeeds, peekLine, stateLine, titleParts } from "../../../src/frontend/ui/session-face";
import { NeedsBar, NeedsWatch, NOTIFY_WAIT_MS, answerWhere, needsHeadline } from "../../../src/frontend/ui/needs-bar";
import { SessionHead, terminalActsOf } from "../../../src/frontend/ui/session-head";
import { buildApiErrorCard } from "../../../src/frontend/ui/cards/api-error";
import { TerminalPage, type TerminalReads } from "../../../src/frontend/ui/terminal-page";
import type { TerminalSend, TerminalSent, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import type { FollowEvents } from "../../../src/frontend/ui/terminal-follow";
import { fmtDur } from "../../../src/frontend/ui/quota-lines";
import { copyText } from "../../../src/frontend/ui/copy-table";

vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));
import { terminalFrontAvailable } from "../../../src/frontend/ui/terminal-front";

const NOW = Date.parse("2026-10-05T12:00:00Z");

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: sid,
    background: false,
    bgName: null,
    aiTitle: null,
    projectDir: `/w/${sid}`,
    state: LIVE,
    pinned: false,
    group: null,
    origin: LOCAL_ORIGIN,
    activity: null,
    forkedFromSessionId: null,
    writers: [],
    unread: 0,
    needs: null,
    pending: [],
    lastSay: null,
    ...over,
  } as unknown as Tab;
}

const waiting = { doing: "needs_you" as const, waitingFor: "permission prompt" };
const approve = (sinceMs: number | null = NOW - 120_000): Needs => ({ kind: "approve", tool: "Bash", call: "toolu_b", what: "rm -rf build/", sinceMs });

beforeEach(() => {
  document.body.innerHTML = "";
  vi.mocked(terminalFrontAvailable).mockReturnValue(false);
});

describe("一个会话读成什么（session-face）", () => {
  it("★ 需要你：活着 ＋ 活动信号说在等才算；种类没到 ⇒「需要你」（不猜）；活动信号说不在等了 ⇒ 手上那份不认；死了 ⇒ 不算", () => {
    expect(needsOf(tab("a", { activity: waiting }))).toEqual({ kind: "unknown", tool: null, call: null, what: null, sinceMs: null });
    expect(needsOf(tab("a", { activity: waiting, needs: approve() }))?.kind).toBe("approve");
    expect(needsOf(tab("a", { activity: { doing: "working", waitingFor: null }, needs: approve() })), "已经答完：不留一条需要你").toBeNull();
    expect(needsOf(tab("a", { state: RECONNECTABLE, activity: waiting, needs: approve() })), "Claude 已退出：陈旧的在等不算").toBeNull();
  });

  it("★ 状态点：颜色 ＝ 在干什么，形状 ＝ 进程在不在（状态不明不当已结束画）", () => {
    const got = [
      tab("a", { activity: { doing: "working", waitingFor: null } }),
      tab("b", { activity: waiting }),
      tab("c", { activity: { doing: "idle", waitingFor: null } }),
      tab("d", { state: RECONNECTABLE }),
      tab("e", { state: ENDED }),
      tab("f", { state: GONE }),
      tab("g", { state: UNSEEN }),
      tab("h", { activity: null }),
    ].map(dotOf);
    expect(got).toEqual(["running", "needs-you", "idle", "exited", "ended", "gone", "unknown", "running"]);
  });

  it("★ 状态句：等批准 · 等了多久 / 运行中 · 调用哪个工具 · 多久 / 空闲 · 完成多久前（没看 ⇒ 多说一个「未看」）/ 状态不明 · 哪台", () => {
    expect(stateLine(tab("a", { activity: waiting, needs: approve() }), NOW)).toEqual({ text: copyText("sessionFace.state.waiting", { kind: copyText("tabBar.needsKind.approve"), waited: "2m" }), needs: true });
    const running = tab("b", { activity: { doing: "working", waitingFor: null }, pending: [{ id: "x", name: "Bash", what: "pytest", at: new Date(NOW - 65_000).toISOString(), state: "running", why: null }] });
    expect(stateLine(running, NOW).text).toBe(copyText("sessionFace.state.runningFor", { tool: "Bash", dur: "1m" }));
    const idle = (unread: number) => tab("c", { unread, activity: { doing: "idle", waitingFor: null }, lastSay: { text: "改好了", at: new Date(NOW - 240_000).toISOString() } });
    expect(stateLine(idle(0), NOW).text).toBe(copyText("sessionFace.state.idleSeen", { ago: "4m" }));
    expect(stateLine(idle(2), NOW).text).toBe(copyText("sessionFace.state.idleUnseen", { ago: "4m" }));
    expect(stateLine(tab("d", { state: UNSEEN, origin: "gpu-01" as never }), NOW).text).toBe(copyText("sessionFace.state.unseen", { machine: "gpu-01" }));
    expect(stateLine(tab("e", { state: ENDED }), NOW).text).not.toContain(copyText("sessionState.unseen.name"));
  });

  it("peek：在等你 ⇒ 等的那一句；在跑 ⇒ 正在做的那一步；空闲 ⇒ 最后一句；拿不到 ⇒ 不出", () => {
    expect(peekLine(tab("a", { activity: waiting, needs: approve() }))).toBe("rm -rf build/");
    expect(peekLine(tab("b", { pending: [{ id: "x", name: "Read", what: "src/a.ts", at: null, state: "running", why: null }] }))).toBe("src/a.ts");
    expect(peekLine(tab("c", { activity: { doing: "idle", waitingFor: null }, lastSay: { text: "结论", at: null } }))).toBe("结论");
    expect(peekLine(tab("d", { state: ENDED, lastSay: { text: "结论", at: null } }))).toBeNull();
  });

  it("★ `Ctrl+J`：在等你的里面等得最久的在前（不知道何时起等的排后、同档按条上顺序）；当前就是 ⇒ 下一个、转回头；没有 ⇒ null", () => {
    const tabs = [
      tab("a"),
      tab("b", { activity: waiting, needs: approve(NOW - 10_000) }),
      tab("c", { activity: waiting }),
      tab("d", { activity: waiting, needs: approve(NOW - 90_000) }),
    ];
    const order = needsOrder(tabs);
    expect(order).toEqual(["d", "b", "c"]);
    expect([nextNeeds(order, "a"), nextNeeds(order, "d"), nextNeeds(order, "c"), nextNeeds([], "a")]).toEqual(["d", "b", "d", null]);
  });

  it("窄窗两个字母取项目目录名里的字母；标题拆成项目名 ＋ 标题（没有 ai 标题 ⇒ 项目名当标题）", () => {
    expect([abbrOf(tab("x", { projectDir: "/w/orders" })), abbrOf(tab("x", { projectDir: "/w/web-console" })), abbrOf(tab("x", { projectDir: "/w/数据集" }))]).toEqual(["or", "we", "数据"]);
    expect(titleParts(tab("x", { projectDir: "/w/orders", aiTitle: "加重试" }))).toEqual({ proj: "orders", title: "加重试", bg: false, forked: false });
    expect(titleParts(tab("x", { projectDir: "/w/orders" })).title).toBe("orders");
  });

  it("时长一种写法：45s · 6m · 1h50m · 2h · 3d", () => {
    expect([45, 360, 6600, 7200, 3 * 86_400 + 5].map(fmtDur)).toEqual(["45s", "6m", "1h50m", "2h", "3d"]);
  });
});

/** 标签页栏台架：store 里摆好 tab，宿主全是替身。 */
function bar(tabs: Tab[]): { view: TabBarView; host: TabBarViewHost; store: TabStore; el: HTMLElement } {
  const store = new TabStore();
  for (const t of tabs) {
    store.tabs.set(t.sessionId, t);
    store.orderedIds.push(t.sessionId);
  }
  store.activeId = tabs[0]?.sessionId ?? null;
  const host = {
    refreshTabBar: vi.fn(),
    openTabCwd: vi.fn().mockResolvedValue(undefined),
    bringTerminalToFront: vi.fn().mockResolvedValue(undefined),
    bringRemoteTerminalToFront: vi.fn().mockResolvedValue(undefined),
    closeTab: vi.fn(),
    switchTo: vi.fn(),
    pick: vi.fn(),
    isSelected: vi.fn().mockReturnValue(false),
    beginDrag: vi.fn(),
    takeSuppressedClick: vi.fn().mockReturnValue(false),
    openMenu: vi.fn(),
    openMenuAt: vi.fn(),
    rereadAll: vi.fn().mockResolvedValue(undefined),
    reconnect: vi.fn(),
  } satisfies TabBarViewHost;
  const el = document.createElement("div");
  document.body.appendChild(el);
  const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, host);
  view.refresh();
  return { view, host, store, el };
}

describe("标签页栏：「需要你 N」与机器离线条", () => {
  it("★ 「需要你」只在有人等你时出、写数；点它跳等得最久的，再点跳下一个；行尾「等批准」换掉未读数", () => {
    const r = bar([tab("a"), tab("b", { activity: waiting, needs: approve(NOW - 10_000), unread: 3 }), tab("c")]);
    const strip = r.el.querySelector<HTMLElement>(".tab-needs")!;
    expect(strip.style.display).toBe("");
    expect(strip.querySelector(".tab-needs-count")?.textContent).toBe("1");
    const b = r.view.tabButtons.get("b")!;
    expect([b.needs.textContent, b.badge.textContent]).toEqual([copyText("tabBar.needsKind.approve"), ""]);
    strip.click();
    expect(r.host.switchTo).toHaveBeenCalledWith("b");
    // 答完了（活动信号说不在等）⇒ 那一条收起，未读数回来。
    (r.store.tabs.get("b") as Tab).activity = { doing: "working", waitingFor: null };
    r.view.refresh();
    expect(strip.style.display).toBe("none");
    expect(b.badge.textContent).toBe("3");
  });

  it("★ 悬停「需要你」500ms ⇒ kit 菜单：每个在等你的会话一行（等得最久的在前 · 状态 · 那一句作第二行）；点一行切过去；没到点就移开不开", () => {
    vi.useFakeTimers();
    try {
      const r = bar([tab("a", { activity: waiting, needs: approve(NOW - 10_000) }), tab("b", { activity: waiting, needs: approve(NOW - 60_000) })]);
      const strip = r.el.querySelector<HTMLElement>(".tab-needs")!;
      strip.dispatchEvent(new Event("mouseenter"));
      vi.advanceTimersByTime(400);
      strip.dispatchEvent(new Event("mouseleave"));
      vi.advanceTimersByTime(400);
      expect(document.querySelector('[role="menu"]'), "没到 500ms 就移开 ⇒ 不开").toBeNull();
      strip.dispatchEvent(new Event("mouseenter"));
      vi.advanceTimersByTime(500);
      const rows = [...document.querySelectorAll<HTMLButtonElement>('[role="menu"] [role="menuitem"]')];
      expect(rows.map((b) => b.querySelector('[data-part="label"]')?.textContent)).toEqual(["b", "a"]);
      expect(rows[0].querySelector('[data-part="body"]')?.textContent).toBe("rm -rf build/");
      rows[1].click();
      expect(r.host.switchTo).toHaveBeenCalledWith("a");
      expect(document.querySelector('[role="menu"]'), "选了一项就关").toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("★ 机器离线：那台看不见、还有状态不明的会话 ⇒ 一条（离线 · 几个状态不明 · 采样多久前）＋［重新连接］；连上了自己消失", () => {
    const r = bar([tab("a"), tab("g1", { origin: "gpu-01" as never, state: UNSEEN }), tab("g2", { origin: "gpu-01" as never, state: UNSEEN })]);
    r.view.markOriginDown("gpu-01", true, Date.now() - 180_000);
    r.view.refresh();
    const row = r.el.querySelector<HTMLElement>(".tab-machine-down-row")!;
    expect(row.textContent).toContain(copyText("tabBar.machineDown.body", { machine: "gpu-01", n: 2 }));
    expect(row.textContent).toContain(copyText("tabBar.machineDown.seen", { ago: "3m" }));
    row.querySelector<HTMLElement>("[data-reconnect]")!.click();
    expect(r.host.reconnect).toHaveBeenCalledWith("gpu-01");
    expect(r.host.pick, "点离线条不算点在列表空白上").not.toHaveBeenCalled();
    r.view.markOriginDown("gpu-01", false);
    r.view.refresh();
    expect(r.el.querySelector(".tab-machine-down-row")).toBeNull();
    // 看不见、但那台一个状态不明的会话都没有（全是已结束的）⇒ 不出条。
    r.view.markOriginDown("gpu-01", true);
    for (const sid of ["g1", "g2"]) (r.store.tabs.get(sid) as Tab).state = ENDED;
    r.view.refresh();
    expect(r.el.querySelector(".tab-machine-down-row")).toBeNull();
  });

  it("状态不明的行不当已结束画：虚线环、标题常色、行尾没有 ×（只右键能关）", () => {
    const r = bar([tab("g1", { origin: "gpu-01" as never, state: UNSEEN })]);
    const b = r.view.tabButtons.get("g1")!;
    expect(b.dot.dataset.state).toBe("unknown");
    expect([b.root.classList.contains("unseen"), b.root.classList.contains("ended")]).toEqual([true, false]);
    expect(b.machine.textContent, "远端：标题前一个机器徽标（不再用方括号前缀）").toBe("gpu-01");
  });
});

describe("「需要你」钉条 · 窗口标题 · 系统通知", () => {
  it("★ 钉条第一行：批准写工具名 ＋ 那一步；回答写问题；计划 · 等批准；判不出只写需要你", () => {
    expect(needsHeadline(approve())).toEqual({ label: copyText("needs.bar.approve", { tool: "Bash" }), code: "rm -rf build/" });
    expect(needsHeadline({ kind: "answer", tool: "AskUserQuestion", call: null, what: "要不要也重试？", sinceMs: null })).toEqual({ label: copyText("needs.bar.answer"), code: "要不要也重试？" });
    expect(needsHeadline({ kind: "plan", tool: "ExitPlanMode", call: null, what: null, sinceMs: null }).label).toBe(copyText("needs.bar.plan"));
    expect(needsHeadline({ kind: "unknown", tool: null, call: null, what: null, sinceMs: null })).toEqual({ label: copyText("needs.bar.unknown"), code: null });
  });

  it("★ 去哪答：Windows 有 ↗ ⇒ 切到终端；远端在 tmux 里 ⇒ 在终端里打开；本机 Linux 不在 ↗ 上 ⇒ 不给按钮", () => {
    expect(answerWhere(tab("a", { origin: "devbox" as never, state: LIVE_ATTACHABLE }))).toBe("attach");
    expect(answerWhere(tab("a"))).toBeNull();
    expect(answerWhere(tab("a", { origin: "devbox" as never, state: LIVE })), "远端、不在 tmux 里：没有可接的终端").toBeNull();
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    expect(answerWhere(tab("a"))).toBe("front");
  });

  it("钉条只跟当前会话：在等 ⇒ 出（两行 ＋ 按钮）；答完 ⇒ 消失、不留痕", () => {
    const cur = tab("a", { origin: "devbox" as never, state: LIVE_ATTACHABLE, activity: waiting, needs: approve() });
    const attach = vi.fn();
    const nb = new NeedsBar({ active: () => cur, front: vi.fn(), attach });
    nb.render(NOW);
    expect(nb.el.style.display).toBe("");
    expect(nb.el.textContent).toContain("rm -rf build/");
    expect(nb.el.textContent).toContain(copyText("needs.bar.sub", { waited: "2m" }));
    nb.el.querySelector("button")!.click();
    expect(attach).toHaveBeenCalledWith("a");
    cur.activity = { doing: "working", waitingFor: null };
    nb.render(NOW);
    expect([nb.el.style.display, nb.el.childElementCount]).toEqual(["none", 0]);
  });

  describe("窗口标题与通知", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());
    const rig = (focused: boolean) => {
      const titles: string[] = [];
      const sent: [string, string][] = [];
      let now = NOW;
      const w = new NeedsWatch({
        setTitle: (t) => titles.push(t),
        isFocused: () => focused,
        enabled: async () => true,
        send: async (t, b) => void sent.push([t, b]),
        now: () => now,
      });
      return { w, titles, sent, tick: (ms: number) => (now += ms) };
    };

    it("★ 标题：`cc-monitor · 需要你 N`，0 个时只有 `cc-monitor`；变了才写", () => {
      const r = rig(true);
      r.w.observe([tab("a")]);
      r.w.observe([tab("a")]);
      r.w.observe([tab("a", { activity: waiting }), tab("b", { activity: waiting })]);
      expect(r.titles).toEqual([copyText("needs.windowTitle.base"), copyText("needs.windowTitle.count", { n: 2 })]);
    });

    it("★ 通知：一个会话**开始**等你、主窗口不在前台才发一条；种类没到先等它（不先发一条需要你）；同一次等待不重发", async () => {
      const r = rig(false);
      const t = tab("a", { activity: waiting });
      r.w.observe([t]);
      await vi.runAllTimersAsync();
      expect(r.sent, "种类还没到：先不发").toEqual([]);
      t.needs = approve();
      r.w.observe([t]);
      await vi.runAllTimersAsync();
      r.w.observe([t]);
      await vi.runAllTimersAsync();
      expect(r.sent).toEqual([[copyText("needs.notify.title", { title: "a", machine: copyText("sessionFace.machine.local"), kind: copyText("tabBar.needsKind.approve") }), "rm -rf build/"]]);
      // 老后端：种类一直不来 ⇒ 等满 NOTIFY_WAIT_MS 照发「需要你」。
      const old = tab("b", { activity: waiting });
      r.w.observe([old]);
      r.tick(NOTIFY_WAIT_MS);
      r.w.observe([old]);
      await vi.runAllTimersAsync();
      expect(r.sent.at(-1)?.[0]).toContain(copyText("tabBar.needsKind.unknown"));
    });

    it("主窗口在前台 ⇒ 不发", async () => {
      const r = rig(true);
      r.w.observe([tab("a", { activity: waiting, needs: approve() })]);
      await vi.runAllTimersAsync();
      expect(r.sent).toEqual([]);
    });
  });
});

describe("会话头", () => {
  const head = (t: Tab | null) => {
    const host = { active: () => t, viewTerminal: vi.fn(), openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn() };
    const h = new SessionHead(host);
    document.body.appendChild(h.el);
    h.render(NOW);
    return { h, host };
  };
  const labels = (el: HTMLElement): string[] => [...el.querySelectorAll("button")].map((b) => b.getAttribute("aria-label") ?? b.textContent ?? "");

  it("★ 标题全名 · 机器 · 目录 · 状态一句；在等你时状态那一句琥珀；右边：目录（本机）· 看它的终端 · 查找 · 更多", () => {
    const { h, host } = head(tab("a", { aiTitle: "加重试", projectDir: "/w/orders", activity: waiting, needs: approve() }));
    expect(h.el.textContent).toContain("orders 加重试");
    expect(h.el.textContent).toContain(copyText("sessionFace.machine.local"));
    expect(h.el.textContent).toContain("/w/orders");
    expect(labels(h.el)).toEqual([copyText("tabBarView.tab.cwdHint"), copyText("sessionHead.act.terminal"), copyText("sessionHead.act.find"), copyText("tabBarView.tab.moreHint")]);
    h.el.querySelectorAll("button")[1].click();
    expect(host.viewTerminal).toHaveBeenCalled();
    h.el.querySelectorAll("button")[2].click();
    expect(host.find).toHaveBeenCalled();
  });

  it("★ 按状态多一颗：已结束［恢复］· Claude 已退出（远端）［在终端里打开］· 状态不明［重新连接］；活着不多", () => {
    expect(labels(head(tab("a", { state: ENDED })).h.el)).toContain(copyText("sessionHead.act.resume"));
    const exited = head(tab("b", { origin: "devbox" as never, state: RECONNECTABLE }));
    expect(labels(exited.h.el)).toContain(copyText("sessionHead.act.openTerm"));
    expect(labels(head(tab("b2", { state: RECONNECTABLE })).h.el), "本机：没有接回终端的那条路，不画点了没反应的按钮").not.toContain(copyText("sessionHead.act.openTerm"));
    const unseen = head(tab("c", { origin: "gpu-01" as never, state: UNSEEN }));
    const re = [...unseen.h.el.querySelectorAll("button")].find((b) => b.textContent === copyText("sessionHead.act.reconnect"))!;
    re.click();
    expect(unseen.host.reconnect).toHaveBeenCalledWith("gpu-01");
    expect(labels(head(tab("e", { state: ENDED })).h.el), "已结束：没有终端可去，不出「看它的终端」").not.toContain(copyText("sessionHead.act.terminal"));
    const live = labels(head(tab("d")).h.el);
    for (const x of ["sessionHead.act.resume", "sessionHead.act.openTerm", "sessionHead.act.reconnect"] as const) expect(live).not.toContain(copyText(x));
  });

  it("没有当前会话 ⇒ 不显示", () => {
    expect(head(null).h.el.style.display).toBe("none");
  });
});

describe("底部抽屉的终端页（L1：快照 ＋ 一行输入 ＋ 常用键）", () => {
  type Row = Awaited<ReturnType<TerminalReads["list"]>>[number];
  const row = (sid: string, over: Partial<Row> = {}): Row => ({ terminal: `tmux-${sid}`, tmuxName: `${sid}-cc`, sid, clients: 0, input: "shared", programExited: false, inputNo: null, ...over });
  const flush = async (): Promise<void> => {
    for (let i = 0; i < 6; i++) await Promise.resolve();
  };
  /** 实时那一格怎么答：缺省「那台只能快照」（下面 L1 那几条照旧量快照的样子）；`live` ⇒ 判据手里拿着推帧 / 停的口。 */
  type FollowRig = { terminal: string; events: FollowEvents; stopped: boolean };
  function rig(rows: Row[], cur: { t: Tab | null }, mode: "snapOnly" | "live" = "snapOnly") {
    let shots = 0;
    const sent: { what: TerminalSend; seen: string | null }[] = [];
    let reply: () => Promise<TerminalSent> = async () => ({ result: "delivered" });
    let shotFails: Error | null = null;
    const follows: FollowRig[] = [];
    const reads: TerminalReads = {
      follow: vi.fn((_o, terminal, events) => {
        const f: FollowRig = { terminal, events, stopped: false };
        follows.push(f);
        if (mode === "snapOnly") events.stop({ kind: "snapshotOnly", said: "tmux 3.1 不支持实时画面 · 需要 3.2 以上" });
        return { stop: () => void (f.stopped = true) };
      }),
      list: vi.fn(async () => rows),
      shot: vi.fn(async () => {
        if (shotFails) throw shotFails;
        shots++;
        return { lines: [{ text: `屏 ${shots}`, spans: [] }], text: `屏 ${shots}`, screen: `fp${shots}`, atText: "22:13:20" };
      }),
      send: vi.fn(async (_o, _t, what, seen) => {
        sent.push({ what, seen });
        return reply();
      }),
    };
    const host = { active: () => cur.t, front: vi.fn(), focusStream: vi.fn() };
    const page = new TerminalPage(host, reads);
    document.body.appendChild(page.el);
    page.sessionChanged();
    return { page, reads, host, sent, follows, shotCount: () => shots, setReply: (r: () => Promise<TerminalSent>) => (reply = r), failShots: (e: Error | null) => (shotFails = e) };
  }
  const box = (p: TerminalPage): HTMLTextAreaElement => p.el.querySelector("textarea")!;
  const key = (el: HTMLElement, k: string, o: KeyboardEventInit = {}): void => void el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...o }));

  it("★ 开到这一页：按会话认出那一行、抓一屏；后台那一行写「后台 · 输入直达」；开着不轮询", async () => {
    vi.useFakeTimers();
    try {
      const r = rig([row("x"), row("a")], { t: tab("a") });
      r.page.setVisible(true);
      await flush();
      expect(r.page.el.querySelector("pre")?.textContent).toBe("屏 1");
      expect(r.page.el.textContent).toContain(copyText("terminal.tag.tmux", { name: "a-cc" }));
      expect(r.page.el.textContent).toContain(copyText("terminal.input.nobody"));
      expect(r.page.el.textContent).toContain(copyText("terminal.input.to", { title: "a", machine: copyText("sessionFace.machine.local") }));
      vi.advanceTimersByTime(60_000);
      await flush();
      expect([vi.mocked(r.reads.list).mock.calls.length, r.shotCount()]).toEqual([1, 1]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("★ 有终端窗口连着（tmux）⇒「另有终端窗口 ×1 · 输入会混在一起」；名单里没有：活着 ⇒ 非 cc-monitor 启动，已结束 ⇒ 已结束 · 无终端", async () => {
    const a = rig([row("a", { clients: 1 })], { t: tab("a") });
    a.page.setVisible(true);
    await flush();
    expect(a.page.el.textContent).toContain(copyText("terminal.input.shared", { n: 1 }));
    expect(a.page.el.textContent).toContain(copyText("terminal.tag.windows", { n: 1 }));
    const b = rig([], { t: tab("b") });
    b.page.setVisible(true);
    await flush();
    expect(b.page.el.textContent).toContain(copyText("terminal.empty.notOurs"));
    expect(vi.mocked(b.reads.shot)).not.toHaveBeenCalled();
    const c = rig([], { t: tab("c", { state: ENDED }) });
    c.page.setVisible(true);
    await flush();
    expect(c.page.el.textContent).toContain(copyText("terminal.empty.ended", { state: copyText("sessionState.ended.name") }));
  });

  it("★ 活着、不在 tmux 里（容器 none）⇒ 照实说「不在 tmux 里」，不说「非 cc-monitor 启动」；↗ 真能用时给［切到终端］", async () => {
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    try {
      const d = rig([], { t: tab("d", { state: LIVE_RESUMABLE }) });
      d.page.setVisible(true);
      await flush();
      expect(d.page.el.textContent).toContain(copyText("terminal.empty.notInTmux"));
      expect(d.page.el.textContent).toContain(copyText("terminal.empty.notInTmuxHint"));
      expect(d.page.el.textContent).not.toContain(copyText("terminal.empty.notOurs"));
      const front = [...d.page.el.querySelectorAll("button")].find((x) => x.textContent?.includes(copyText("terminal.head.front")));
      expect(front, "Windows 上不在 tmux 里的会话还能切到它的终端窗口").toBeDefined();
      front!.click();
      expect(d.host.front).toHaveBeenCalledWith("d");
      vi.mocked(terminalFrontAvailable).mockReturnValue(false);
      const e = rig([], { t: tab("e", { state: LIVE_RESUMABLE }) });
      e.page.setVisible(true);
      await flush();
      expect([...e.page.el.querySelectorAll("button")].some((x) => x.textContent?.includes(copyText("terminal.head.front")))).toBe(false);
      const f = rig([], { t: tab("f", { state: LIVE_ATTACHABLE }) });
      f.page.setVisible(true);
      await flush();
      expect(f.page.el.textContent, "在 tmux 里、名单却认不出的才是「非 cc-monitor 启动」").toContain(copyText("terminal.empty.notOurs"));
    } finally {
      vi.mocked(terminalFrontAvailable).mockReturnValue(false);
    }
  });

  it("★ 回车送字并补回车、带上看到的那一屏的指纹；送到了清框、写「已送达」，0.5 · 1.5 · 3 秒各再抓一次；Ctrl+回车只送字；Shift+回车与组字不送", async () => {
    vi.useFakeTimers();
    try {
      const r = rig([row("a")], { t: tab("a") });
      r.page.setVisible(true);
      await flush();
      const b = box(r.page);
      b.value = "/usage";
      key(b, "Enter", { shiftKey: true });
      key(b, "Enter", { isComposing: true });
      await flush();
      expect(r.sent).toEqual([]);
      key(b, "Enter");
      await flush();
      expect(r.sent).toEqual([{ what: { text: "/usage", enter: true }, seen: "fp1" }]);
      expect(b.value).toBe("");
      expect(r.page.el.textContent).toContain(copyText("terminal.input.delivered"));
      for (const ms of [500, 1000, 1500]) {
        vi.advanceTimersByTime(ms);
        await flush();
      }
      expect(r.shotCount()).toBe(4);
      b.value = "y";
      key(b, "Enter", { ctrlKey: true });
      await flush();
      expect(r.sent[1].what).toEqual({ text: "y", enter: false });
      b.value = "字".repeat(2001);
      b.dispatchEvent(new Event("input"));
      const send = [...r.page.el.querySelectorAll("button")].find((x) => x.textContent?.startsWith(copyText("terminal.input.send")))!;
      expect(send.textContent).toBe(copyText("terminal.input.sendLong", { n: "2,001" }));
    } finally {
      vi.useRealTimers();
    }
  });

  it("★ 画面已经变了 ⇒ 不送、字留着、重抓一次；不知道送没送到 ⇒ 照实说、不重发；没送到 ⇒ 字留着、给［重试］", async () => {
    const r = rig([row("a")], { t: tab("a") });
    r.page.setVisible(true);
    await flush();
    const b = box(r.page);
    b.value = "1";
    r.setReply(async () => ({ result: "refused", why: "screen_changed", said: "那台写的一句 · 画面", screen: "fp9" }));
    key(b, "Enter");
    await flush();
    expect([b.value, r.shotCount()]).toEqual(["1", 2]);
    expect(r.page.el.textContent).toContain(copyText("terminal.input.screenChanged"));
    r.setReply(async () => ({ result: "unsure" }));
    key(b, "Enter");
    await flush();
    expect(r.page.el.textContent).toContain(copyText("terminal.input.unsure", { machine: copyText("sessionFace.machine.local") }));
    expect([r.sent.length, b.value]).toEqual([2, "1"]);
    r.setReply(async () => {
      throw new Error(copyText("beServer.words.cantConnect"));
    });
    key(b, "Enter");
    await flush();
    expect(b.value).toBe("1");
    const retry = [...r.page.el.querySelectorAll("button")].find((x) => x.textContent === copyText("terminal.input.retry"));
    expect(retry).toBeDefined();
  });

  it("★ 送字被拒 ⇒ 那一句（后端写好）排进「未送达 · … · 内容保留」；终端已不在 ⇒ 重找", async () => {
    const r = rig([row("a")], { t: tab("a") });
    r.page.setVisible(true);
    await flush();
    const b = box(r.page);
    b.value = "1";
    r.setReply(async () => ({ result: "refused", why: "not_known", said: "那台写的一句 · 不在", screen: null }));
    const lists = vi.mocked(r.reads.list).mock.calls.length;
    key(b, "Enter");
    await flush();
    expect(r.page.el.textContent).toContain(copyText("terminal.input.failed", { why: "那台写的一句 · 不在" }));
    expect(b.value).toBe("1");
    expect(vi.mocked(r.reads.list).mock.calls.length, "终端已不在 ⇒ 重问名单").toBe(lists + 1);
  });

  it("★ 没送到、那一端写了详情 ⇒ 红字是人话那一句（原话不进句子）＋［重试］＋［复制详情］，复制的是那句 ＋ 详情", async () => {
    const r = rig([row("a")], { t: tab("a") });
    r.page.setVisible(true);
    await flush();
    const b = box(r.page);
    b.value = "1";
    r.setReply(async () => {
      throw Object.assign(new Error("送不出去-甲"), { detail: "码：timeout\n原话：夹具原话-乙" });
    });
    key(b, "Enter");
    await flush();
    const note = r.page.el.textContent ?? "";
    expect(note).toContain("送不出去-甲");
    expect(note).not.toContain("夹具原话-乙");
    const labels = [...r.page.el.querySelectorAll("button")].map((x) => x.textContent);
    expect(labels).toContain(copyText("terminal.input.retry"));
    expect(labels).toContain(copyText("detail.act.copy"));
  });

  it("★ 后端说送不了（别的前端的会话）⇒ 框与键都灰、写为什么，点了也不送", async () => {
    const r = rig([row("a", { inputNo: "那台写的一句 · 不归这边" })], { t: tab("a") });
    r.page.setVisible(true);
    await flush();
    expect(box(r.page).disabled).toBe(true);
    expect(box(r.page).placeholder, "后端那一句原样排进去").toBe(copyText("terminal.input.readOnly", { why: "那台写的一句 · 不归这边" }));
    const esc = [...r.page.el.querySelectorAll("button")].find((x) => x.textContent === "Esc")!;
    expect(esc.getAttribute("aria-disabled")).toBe("true");
    expect(await r.page.send({ key: "esc" })).toBe(false);
    expect(vi.mocked(r.reads.send)).not.toHaveBeenCalled();
  });

  it("★ 没送的字按会话各记一份；切走时慢回来的名单不落到新会话上", async () => {
    const cur: { t: Tab | null } = { t: tab("a") };
    let release!: (v: Row[]) => void;
    const r = rig([row("a"), row("b")], cur);
    r.page.setVisible(true);
    await flush();
    const b = box(r.page);
    b.value = "半句话";
    vi.mocked(r.reads.list).mockImplementationOnce(() => new Promise((res) => (release = res)));
    cur.t = tab("b");
    r.page.sessionChanged();
    expect(b.value).toBe("");
    cur.t = tab("a");
    r.page.sessionChanged();
    await flush();
    expect(box(r.page).value).toBe("半句话");
    release([row("a", { clients: 3 }), row("b", { clients: 3 })]);
    await flush();
    r.page.sessionChanged(); // 同一个会话：只重画
    expect(r.page.el.textContent).not.toContain(copyText("terminal.input.shared", { n: 3 }));
  });

  const liveShot = (text: string, at = "22:14:00"): TerminalShot => ({ lines: [{ text, spans: [{ from: 0, to: 2, fg: "green" }] }], text, screen: `fp-${text}`, atText: at });
  const visibleText = (el: HTMLElement): string =>
    [...el.querySelectorAll<HTMLElement>("*")].filter((e) => e.children.length === 0 && !e.closest("[hidden]")).map((e) => e.textContent ?? "").join("|");

  it("★★ 实时：开到这一页 ⇒ 订那一行的终端；第一帧到 ⇒ 头上「实时」、不再写「画面几点」与「重新看」，画面带颜色换成那一帧", async () => {
    const r = rig([row("a")], { t: tab("a") }, "live");
    r.page.setVisible(true);
    await flush();
    expect(r.follows.map((f) => f.terminal)).toEqual(["tmux-a"]);
    expect(visibleText(r.page.el)).toContain(copyText("terminal.head.joining"));
    r.follows[0].events.screen(liveShot("ok live"));
    const shown = visibleText(r.page.el);
    expect(shown).toContain(copyText("terminal.head.live"));
    expect(shown).not.toContain(copyText("terminal.head.recapture"));
    expect(shown).not.toContain(copyText("terminal.head.snapAt", { time: "22:14:00" }));
    expect(r.page.el.querySelector("pre")?.textContent).toBe("ok live");
    expect(r.page.el.querySelector("pre span")?.getAttribute("data-fg")).toBe("green");
  });

  it("★★ 实时中送字 ⇒ 写「已送达」、不再排 0.5 · 1.5 · 3 秒重抓；收起抽屉 · 切标签页 ⇒ 退订", async () => {
    vi.useFakeTimers();
    try {
      const cur = { t: tab("a") as Tab | null };
      const r = rig([row("a"), row("b")], cur, "live");
      r.page.setVisible(true);
      await flush();
      r.follows[0].events.screen(liveShot("ok live"));
      const before = r.shotCount();
      box(r.page).value = "1";
      key(box(r.page), "Enter");
      await flush();
      expect(r.page.el.textContent).toContain(copyText("terminal.input.delivered"));
      vi.advanceTimersByTime(5000);
      await flush();
      expect(r.shotCount(), "实时中还在送完重抓").toBe(before);
      r.page.setVisible(false);
      expect(r.follows[0].stopped, "收起抽屉没退订").toBe(true);
      r.page.setVisible(true);
      await flush();
      expect(r.follows).toHaveLength(2);
      cur.t = tab("b");
      r.page.sessionChanged();
      expect(r.follows[1].stopped, "切标签页没退订").toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("★★ 实时断了（那台断开）⇒ 头下条「实时已停 · 那台断开」＋［重新接上］、画面变淡、输入灰；点了再订；不自己重连", async () => {
    const r = rig([row("a")], { t: tab("a", { origin: "devbox" }) }, "live");
    r.page.setVisible(true);
    await flush();
    r.follows[0].events.screen(liveShot("ok live", "22:15:07"));
    r.follows[0].events.stop({ kind: "stopped", said: "devbox 断开", offline: true });
    const said = copyText("terminal.bar.liveStopped", { why: "devbox 断开" });
    expect(r.page.el.textContent).toContain(said);
    expect(visibleText(r.page.el)).toContain(copyText("terminal.head.snapAt", { time: "22:15:07" }));
    expect(r.page.el.querySelector("[data-stale]")?.getAttribute("data-stale")).toBe("true");
    expect(box(r.page).disabled).toBe(true);
    expect(r.follows).toHaveLength(1);
    [...r.page.el.querySelectorAll("button")].find((b) => b.textContent?.includes(copyText("terminal.bar.liveRetry")))!.click();
    expect(r.follows).toHaveLength(2);
  });

  it("★★ 实时停在「那台断开」、那台自己重连回来（机器状态推「已连上」）⇒ 自动重新订上、输入框恢复；别的机器回来不动", async () => {
    const r = rig([row("a")], { t: tab("a", { origin: "devbox" }) }, "live");
    r.page.setVisible(true);
    await flush();
    r.follows[0].events.screen(liveShot("ok live"));
    r.follows[0].events.stop({ kind: "stopped", said: "devbox 断开", offline: true });
    expect(box(r.page).disabled).toBe(true);
    r.page.machineUp("gpu-01");
    await flush();
    expect(r.follows, "别的机器回来不该重订").toHaveLength(1);
    r.page.machineUp("devbox");
    await flush();
    expect(r.follows.map((f) => f.terminal), "那台回来 ⇒ 重新订上").toEqual(["tmux-a", "tmux-a"]);
    expect(box(r.page).disabled, "输入框恢复").toBe(false);
    expect(r.page.el.textContent).not.toContain(copyText("terminal.bar.liveStopped", { why: "devbox 断开" }));
    r.follows[1].events.screen(liveShot("back"));
    expect(visibleText(r.page.el)).toContain(copyText("terminal.head.live"));
    r.page.machineUp("devbox");
    await flush();
    expect(r.follows, "实时中再来一帧「已连上」不重订").toHaveLength(2);
  });

  it("★ 输入框的读屏名是头上「送往 …」那一行（提示句只当占位，读屏不把同一句念成名字又念成说明）", async () => {
    const r = rig([row("a")], { t: tab("a", { origin: "devbox" }) });
    r.page.setVisible(true);
    await flush();
    const by = box(r.page).getAttribute("aria-labelledby") ?? "";
    const named = by.split(/\s+/).map((id) => r.page.el.querySelector(`[id="${id}"]`)?.textContent ?? "").join(" ").trim();
    expect(named).toBe(copyText("terminal.input.to", { title: fullTitle(tab("a", { origin: "devbox" })), machine: "devbox" }));
    expect(named).not.toBe(box(r.page).placeholder);
  });

  it("★ 抓屏失败 ⇒ 只出那一条错误条、不去订实时；点［刷新］抓到了 ⇒ 照常订上", async () => {
    const r = rig([row("a")], { t: tab("a") }, "live");
    r.failShots(new Error("no tmux server"));
    r.page.setVisible(true);
    await flush();
    expect(r.follows, "抓屏都失败了还去订实时（订也会被拒，叠出第二条警告）").toHaveLength(0);
    expect(r.page.el.textContent).not.toContain(copyText("terminal.bar.liveRetry"));
    r.failShots(null);
    [...r.page.el.querySelectorAll("button")].find((b) => b.textContent === copyText("terminal.state.refresh"))!.click();
    await flush();
    expect(r.follows.map((f) => f.terminal)).toEqual(["tmux-a"]);
  });

  it("★ 那台只能快照 ⇒「仅快照」，悬停说为什么；照旧有「画面几点 ＋ 重新看」、送完照旧重抓", async () => {
    vi.useFakeTimers();
    try {
      const r = rig([row("a")], { t: tab("a", { origin: "devbox" }) });
      r.page.setVisible(true);
      await flush();
      const shown = visibleText(r.page.el);
      expect(shown).toContain(copyText("terminal.head.snapshotOnly"));
      expect(shown).toContain(copyText("terminal.head.recapture"));
      expect(shown).not.toContain(copyText("terminal.head.live"));
      const tag = [...r.page.el.querySelectorAll<HTMLElement>("span")].find((e) => e.textContent === copyText("terminal.head.snapshotOnly"))!;
      expect(tag.title, "悬停是那一句原样").toBe("tmux 3.1 不支持实时画面 · 需要 3.2 以上");
      const before = r.shotCount();
      box(r.page).value = "1";
      key(box(r.page), "Enter");
      await flush();
      vi.advanceTimersByTime(3000);
      await flush();
      expect(r.shotCount()).toBe(before + 3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("输入框里 Esc 只把焦点还给消息流；「切到终端」只在 ↗ 真能用时出", async () => {
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    const r = rig([row("a")], { t: tab("a") });
    r.page.setVisible(true);
    await flush();
    box(r.page).focus();
    expect(r.page.escFromInput()).toBe(true);
    expect(r.host.focusStream).toHaveBeenCalled();
    const front = [...r.page.el.querySelectorAll("button")].find((x) => x.textContent?.includes(copyText("terminal.head.front")))!;
    expect(front.style.display).toBe("");
    front.click();
    expect(r.host.front).toHaveBeenCalledWith("a");
    vi.mocked(terminalFrontAvailable).mockReturnValue(false);
    r.page.sessionChanged();
    expect(front.style.display).toBe("none");
  });
});

describe("报错卡上去它的终端那两颗（与会话头同一道）", () => {
  it("★ 切到终端：↗ 真能用且还有终端可去；在终端里打开：远端、Claude 已退出；别的都不出", () => {
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    expect(terminalActsOf(tab("a"))).toEqual({ front: true, attach: false });
    expect(terminalActsOf(tab("b", { state: ENDED })).front).toBe(false);
    vi.mocked(terminalFrontAvailable).mockReturnValue(false);
    expect(terminalActsOf(tab("a"))).toEqual({ front: false, attach: false });
    expect(terminalActsOf(tab("c", { origin: "devbox" as never, state: RECONNECTABLE }))).toEqual({ front: false, attach: true });
    expect(terminalActsOf(tab("d", { state: RECONNECTABLE })).attach, "本机：没有接回终端的那条路").toBe(false);
  });

  it("报错卡摆出两颗（看不看得见由消息流根上的标记管），点了的那颗带着它是哪条路", () => {
    const card = buildApiErrorCard({ timeLabel: "02:10", text: "overloaded" });
    expect([...card.querySelectorAll<HTMLElement>(".api-error-acts [data-act]")].map((b) => [b.dataset.act, b.textContent])).toEqual([
      ["front", copyText("terminal.head.front")],
      ["attach", copyText("sessionHead.act.openTerm")],
    ]);
  });
});

// 窗口标题起步就是产品名：三份入口 HTML 的 <title> 与壳 / 需要你那一路设的基础标题同字（不写死别的产品名）。
describe("入口 HTML 的 <title> == 窗口的基础标题", () => {
  it("index · settings · viewer 三份都是 needs.windowTitle.base", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const { REPO_ROOT } = await import("../../test-support/repo-root.ts");
    const got = ["index.html", "settings.html", "viewer.html"].map((f) => /<title>([^<]*)<\/title>/.exec(readFileSync(resolve(REPO_ROOT, f), "utf8"))?.[1]);
    expect(got).toEqual(Array(3).fill(copyText("needs.windowTitle.base")));
  });
});
