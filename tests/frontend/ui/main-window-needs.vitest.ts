/**
 * 主窗口照稿第一批（主窗口稿 §5.1 标签页栏 · §5.6「需要你」· §5.13 会话头）：一个会话读成什么、谁在等你、去哪答。
 *
 * - `session-face.ts`：状态点（V10）· 状态句 · peek · 需要你（活动信号说不在等了 ⇒ 手上那份当场不认；说在等、种类没到 ⇒ 不猜）· `Ctrl+J` 的顺序；
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
import { ENDED, GONE, LIVE, LIVE_ATTACHABLE, RECONNECTABLE, UNSEEN } from "../../../src/frontend/ui/tab-session-state";
import { abbrOf, dotOf, needsOf, needsOrder, nextNeeds, peekLine, stateLine, titleParts } from "../../../src/frontend/ui/session-face";
import { NeedsBar, NeedsWatch, NOTIFY_WAIT_MS, answerWhere, needsHeadline } from "../../../src/frontend/ui/needs-bar";
import { SessionHead } from "../../../src/frontend/ui/session-head";
import { fmtDur } from "../../../src/frontend/ui/quota-lines";
import { copyText } from "../../../src/frontend/ui/copy-table";

vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));
import { terminalFrontAvailable } from "../../../src/frontend/ui/terminal-front";

const NOW = Date.parse("2026-10-05T12:00:00Z");

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: sid,
    kind: null,
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

const waiting = { status: "waiting", waitingFor: "permission prompt" };
const approve = (sinceMs: number | null = NOW - 120_000): Needs => ({ kind: "approve", tool: "Bash", what: "rm -rf build/", sinceMs });

beforeEach(() => {
  document.body.innerHTML = "";
  vi.mocked(terminalFrontAvailable).mockReturnValue(false);
});

describe("一个会话读成什么（session-face）", () => {
  it("★ 需要你：活着 ＋ 活动信号说在等才算；种类没到 ⇒「需要你」（不猜）；活动信号说不在等了 ⇒ 手上那份不认；死了 ⇒ 不算", () => {
    expect(needsOf(tab("a", { activity: waiting }))).toEqual({ kind: "unknown", tool: null, what: null, sinceMs: null });
    expect(needsOf(tab("a", { activity: waiting, needs: approve() }))?.kind).toBe("approve");
    expect(needsOf(tab("a", { activity: { status: "busy", waitingFor: null }, needs: approve() })), "已经答完：不留一条需要你").toBeNull();
    expect(needsOf(tab("a", { state: RECONNECTABLE, activity: waiting, needs: approve() })), "Claude 已退出：陈旧的在等不算").toBeNull();
  });

  it("★ 状态点（V10）：颜色 ＝ 在干什么，形状 ＝ 进程在不在（状态不明不当已结束画）", () => {
    const got = [
      tab("a", { activity: { status: "busy", waitingFor: null } }),
      tab("b", { activity: waiting }),
      tab("c", { activity: { status: "idle", waitingFor: null } }),
      tab("d", { state: RECONNECTABLE }),
      tab("e", { state: ENDED }),
      tab("f", { state: GONE }),
      tab("g", { state: UNSEEN }),
      tab("h", { activity: null }),
    ].map(dotOf);
    expect(got).toEqual(["running", "needs-you", "idle", "exited", "ended", "gone", "unknown", "running"]);
  });

  it("★ 状态句：等批准 · 等了多久 / 运行中 · 调用哪个工具 · 多久 / 空闲 · 完成多久前（没看 ⇒ 多说一个「未看」）/ 状态不明 · 哪台", () => {
    expect(stateLine(tab("a", { activity: waiting, needs: approve() }), NOW)).toEqual({ text: copyText("sessionFace.state.waiting", { kind: "等批准", waited: "2m" }), needs: true });
    const running = tab("b", { activity: { status: "busy", waitingFor: null }, pending: [{ id: "x", name: "Bash", what: "pytest", at: new Date(NOW - 65_000).toISOString() }] });
    expect(stateLine(running, NOW).text).toBe(copyText("sessionFace.state.runningFor", { tool: "Bash", dur: "1m" }));
    const idle = (unread: number) => tab("c", { unread, activity: { status: "idle", waitingFor: null }, lastSay: { text: "改好了", at: new Date(NOW - 240_000).toISOString() } });
    expect(stateLine(idle(0), NOW).text).toBe(copyText("sessionFace.state.idleSeen", { ago: "4m" }));
    expect(stateLine(idle(2), NOW).text).toBe(copyText("sessionFace.state.idleUnseen", { ago: "4m" }));
    expect(stateLine(tab("d", { state: UNSEEN, origin: "gpu-01" as never }), NOW).text).toBe(copyText("sessionFace.state.unseen", { machine: "gpu-01" }));
    expect(stateLine(tab("e", { state: ENDED }), NOW).text).not.toContain(copyText("sessionState.unseen.name"));
  });

  it("peek：在等你 ⇒ 等的那一句；在跑 ⇒ 正在做的那一步；空闲 ⇒ 最后一句；拿不到 ⇒ 不出", () => {
    expect(peekLine(tab("a", { activity: waiting, needs: approve() }))).toBe("rm -rf build/");
    expect(peekLine(tab("b", { pending: [{ id: "x", name: "Read", what: "src/a.ts", at: null }] }))).toBe("src/a.ts");
    expect(peekLine(tab("c", { activity: { status: "idle", waitingFor: null }, lastSay: { text: "结论", at: null } }))).toBe("结论");
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
    (r.store.tabs.get("b") as Tab).activity = { status: "busy", waitingFor: null };
    r.view.refresh();
    expect(strip.style.display).toBe("none");
    expect(b.badge.textContent).toBe("3");
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
    expect(needsHeadline({ kind: "answer", tool: "AskUserQuestion", what: "要不要也重试？", sinceMs: null })).toEqual({ label: copyText("needs.bar.answer"), code: "要不要也重试？" });
    expect(needsHeadline({ kind: "plan", tool: "ExitPlanMode", what: null, sinceMs: null }).label).toBe(copyText("needs.bar.plan"));
    expect(needsHeadline({ kind: "unknown", tool: null, what: null, sinceMs: null })).toEqual({ label: copyText("needs.bar.unknown"), code: null });
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
    cur.activity = { status: "busy", waitingFor: null };
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
      expect(r.sent).toEqual([[copyText("needs.notify.title", { title: "a", machine: copyText("sessionFace.machine.local"), kind: "等批准" }), "rm -rf build/"]]);
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
    const host = { active: () => t, openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn() };
    const h = new SessionHead(host);
    document.body.appendChild(h.el);
    h.render(NOW);
    return { h, host };
  };
  const labels = (el: HTMLElement): string[] => [...el.querySelectorAll("button")].map((b) => b.getAttribute("aria-label") ?? b.textContent ?? "");

  it("★ 标题全名 · 机器 · 目录 · 状态一句；在等你时状态那一句琥珀；右边：目录（本机）· 查找 · 更多", () => {
    const { h, host } = head(tab("a", { aiTitle: "加重试", projectDir: "/w/orders", activity: waiting, needs: approve() }));
    expect(h.el.textContent).toContain("orders 加重试");
    expect(h.el.textContent).toContain(copyText("sessionFace.machine.local"));
    expect(h.el.textContent).toContain("/w/orders");
    expect(labels(h.el)).toEqual([copyText("tabBarView.tab.cwdHint"), copyText("sessionHead.act.find"), copyText("tabBarView.tab.moreHint")]);
    h.el.querySelectorAll("button")[1].click();
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
    const live = labels(head(tab("d")).h.el);
    for (const x of ["sessionHead.act.resume", "sessionHead.act.openTerm", "sessionHead.act.reconnect"] as const) expect(live).not.toContain(copyText(x));
  });

  it("没有当前会话 ⇒ 不显示", () => {
    expect(head(null).h.el.style.display).toBe("none");
  });
});
