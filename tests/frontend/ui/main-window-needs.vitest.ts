/**
 * 主窗口的标签页栏 · 「需手动」· 会话头：一个会话读成什么、谁在等你、去哪答。
 *
 * - `session-face.ts`：状态点 · 状态句 · peek · 需手动（活动信号说不在等了 ⇒ 手上那份当场不认；说在等、种类没到 ⇒ 不猜）；
 * - `needs-book.ts`：「需手动」的账问后端（各台 `sessions-needs` 整份，本机 `sessions-needs-merge` 合一份 ＋ 总数），界面不排不数；
 * - 标签页栏：「需手动 N」只在 N ≥ 1 时出、点它开浮层（浮层本身见 `needs-panel.vitest.ts`）、`Ctrl+J` 跳等得最久的；机器离线条（`{machine} 离线 · N 会话状态不明`）＋［重新连接］；
 * - 窗口标题（消息流底部的答题块另见 `answer-block.vitest.ts`）；
 * - 会话头：按状态多一颗（已结束［恢复 ▾］· Claude 已退出（远端）［在终端里打开］· 状态不明［重新连接］）。
 *
 * 夹具只造结构（sid · 路径 · 占位工具名），不采会话正文。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { TabStore } from "../../../src/frontend/ui/tab-store";

/** `needs-book.ts` 经 `callTyped` 问：那一条换成手上的假的，别处照真的。 */
const typed = vi.hoisted(() => ({ fake: null as ((...a: unknown[]) => Promise<unknown>) | null }));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/ipc/chan-caller")>();
  return { ...real, callTyped: (...a: unknown[]) => (typed.fake ? typed.fake(...a) : (real.callTyped as (...b: unknown[]) => Promise<unknown>)(...a)) };
});
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { Needs } from "../../../src/frontend/ui/session-reads";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import type { FactsLine } from "../../../src/frontend/ui/generated/FactsLine";
import type { WireSessionActivity } from "../../../src/frontend/ui/generated/WireSessionActivity";
import { goldenFace, liveFace } from "../../test-support/session-faces";
import { abbrOf, dotOf, fullTitle, groupSummary, needsOf, nextNeedsStep, peekLine, stateLine, stateWord, titleParts } from "../../../src/frontend/ui/session-face";
import { NeedsBook } from "../../../src/frontend/ui/needs-book";
import { ChanError } from "../../../src/comms/inward/chan";
import { fakeNeedsBook, listOf, sessionItem } from "../../test-support/needs-fake";
import { NeedsTitle } from "../../../src/frontend/ui/needs-bar";
import { SessionHead, terminalActsOf } from "../../../src/frontend/ui/session-head";
import { buildApiErrorCard } from "../../../src/frontend/ui/cards/api-error";
import { TerminalPage, type TerminalReads } from "../../../src/frontend/ui/terminal-page";
import type { TerminalSend, TerminalSent, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import type { FollowEvents } from "../../../src/frontend/ui/terminal-follow";
import { fmtDur } from "../../../src/frontend/ui/duration-format";
import { copyText } from "../../../src/frontend/ui/copy-table";

vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));
import { terminalFrontAvailable } from "../../../src/frontend/ui/terminal-front";

const NOW = Date.parse("2026-10-05T12:00:00Z");
/** 会话头那两颗（换模型 · 切权限档）要的宿主：这里不换，清单问不到。 */
const NO_SWITCHING = { active: () => null, facts: () => new Promise<never>(() => {}), send: vi.fn(), unavailable: () => null, reread: vi.fn(), toast: vi.fn() };

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: sid,
    background: false,
    bgName: null,
    label: null,
    labelProject: null,
    projectDir: `/w/${sid}`,
    face: liveFace("working", true),
    line: null,
    pinned: false,
    group: null,
    origin: LOCAL_ORIGIN,
    forkedFromSessionId: null,
    writers: [],
    unread: 0,
    needs: null,
    autoAnswer: null,
    autoAnswered: null,
    backgroundWork: null,
    pending: [],
    lastSay: null,
    ...over,
  } as unknown as Tab;
}

/** 会话事实那一句（写它时那一态是 `doing`）。 */
const lineFor = (doing: WireSessionActivity): FactsLine => ({ for: doing, text: `${doing}·核心`, clock: null, peek: null });
/** 在等人：脸 ＋ 同一刻的那一句。 */
const waiting = { face: liveFace("needs_you"), line: lineFor("needs_you") };
/** 那台在 `NOW` 答出、本机同一刻收到（两台钟一样快）；已等多久由那台算好。 */
const approve = (sinceMs: number | null = NOW - 120_000): Needs => {
  const waitedMs = sinceMs === null ? null : NOW - sinceMs;
  return { kind: "approve", tool: "Bash", call: "toolu_b", what: "rm -rf build/", sinceMs, text: copyText("beSession.needs.approve"), head: "核心·等批准 · Bash", headCode: "rm -rf build/", tone: "need", rank: 1, waitedMs, waitedText: waitedMs === null ? null : fmtDur(waitedMs / 1000), ask: null, manualText: null, receivedAt: NOW };
};

beforeEach(() => {
  document.body.innerHTML = "";
  vi.mocked(terminalFrontAvailable).mockReturnValue(false);
});

describe("一个会话读成什么（session-face）：点 · 字照抄那台的脸，会话事实那一句与脸同一刻才用", () => {
  it("★ 点 · 短名 ＝ 脸上那两格；还没收到 / 那台看不见 ⇒ 说不清那一张（带机器名）", () => {
    for (const k of ["live-working-plain", "live-idle", "reconnectable", "ended", "gone"] as const) {
      const f = goldenFace(k);
      expect([dotOf(tab("a", { face: f })), stateWord(tab("a", { face: f }))], k).toEqual([f.dot, f.name]);
    }
    const u = tab("u", { face: null, origin: "gpu-01" as never });
    expect([dotOf(u), stateWord(u), stateLine(u, NOW).text]).toEqual(["unknown", copyText("sessionState.unseen.name"), copyText("sessionFace.state.unseen", { machine: "gpu-01" })]);
  });

  it("★ 需手动：脸说在等、会话事实那一句也是在等那一刻 ⇒ 认事实里那一份；对不上（事实还没到 · 已经答完 · 进程不在）⇒ 不算", () => {
    expect(needsOf(tab("a", { ...waiting, needs: approve() }))?.kind).toBe("approve");
    expect(needsOf(tab("a", { face: liveFace("needs_you"), line: null, needs: approve() })), "事实还没到：不猜").toBeNull();
    expect(needsOf(tab("a", { face: liveFace("working"), line: lineFor("needs_you"), needs: approve() })), "已经答完：不留一条需手动").toBeNull();
    expect(needsOf(tab("a", { face: goldenFace("reconnectable"), line: lineFor("needs_you"), needs: approve() })), "Claude 已退出：陈旧的在等不算").toBeNull();
  });

  it("收着的组头汇总：等你 · 在跑 · 后台各数一遍（空闲 · 已结束 · 状态不明 · Claude 已退出不算），字照抄脸上的短名", () => {
    const members = [
      tab("a", waiting),
      tab("b", { face: liveFace("working") }),
      tab("c", { face: liveFace("working") }),
      tab("d", { face: liveFace("idle") }),
      tab("e", { face: goldenFace("ended") }),
      tab("f", { face: null }),
      tab("g", { face: goldenFace("reconnectable") }),
      tab("h", { face: liveFace("background_work") }),
    ];
    expect(groupSummary(members)).toEqual({ needs: 1, running: 2, background: 1, needsWord: goldenFace("live-needs-you-tmux").name, runningWord: goldenFace("live-working-plain").name, backgroundWord: goldenFace("live-background").name });
  });

  it("★ 状态一句：会话事实那一句与脸同一刻 ⇒ 用它（会走的那一截按此刻填、钟在走）；不同刻 ⇒ 脸上那一句；在等 ⇒ 琥珀", () => {
    const t = tab("a", { face: liveFace("working"), line: { for: "working", text: "L·核心", clock: { text: "L · {dur}", from: NOW - 65_000 }, peek: "pytest" } });
    expect(stateLine(t, NOW)).toEqual({ text: "L · 1m", needs: false });
    expect(stateLine(t, NOW + 60_000).text, "钟在走").toBe("L · 2m");
    expect(stateLine(tab("b", { face: liveFace("working"), line: { for: "working", text: "L·核心", clock: null, peek: null } }), NOW).text).toBe("L·核心");
    expect(stateLine(tab("c", { face: liveFace("idle"), line: lineFor("working") }), NOW).text, "两份不是同一刻").toBe(goldenFace("live-idle").line);
    expect(stateLine(tab("d", { face: goldenFace("ended"), line: lineFor("idle") }), NOW).text).toBe(goldenFace("ended").line);
    expect(stateLine(tab("e", waiting), NOW).needs).toBe(true);
  });

  it("peek：会话事实写的那一句（同一刻才认）；拿不到 ⇒ 不出", () => {
    expect(peekLine(tab("a", { face: liveFace("working"), line: { ...lineFor("working"), peek: "src/a.ts" } }))).toBe("src/a.ts");
    expect(peekLine(tab("b", { face: liveFace("idle"), line: { ...lineFor("working"), peek: "src/a.ts" } }))).toBeNull();
    expect(peekLine(tab("c", { face: goldenFace("ended") }))).toBeNull();
  });

  it("★ `Ctrl+J` 照合好的那一份走：当前就是 ⇒ 下一个、转回头；会话走完接计划项", () => {
    const order = ["e", "b", "c"];
    expect([nextNeedsStep(order, "a", null, 0), nextNeedsStep(order, "e", null, 0), nextNeedsStep(order, "c", null, 0), nextNeedsStep([], "a", null, 0)]).toEqual([{ sid: "e" }, { sid: "b" }, { sid: "e" }, null]);
    expect([nextNeedsStep(order, "c", null, 2), nextNeedsStep(order, null, 0, 2), nextNeedsStep(order, null, 1, 2)]).toEqual([{ plan: 0 }, { plan: 1 }, { sid: "e" }]);
  });

  it("★ 问后端：各台问 `sessions-needs`、原样按台交本机 `sessions-needs-merge`；断开的那台不交、记成断开；数 ＝ 合好的 `total`，界面不自己加", async () => {
    const asked: { origin: string; op: string; args: unknown }[] = [];
    const devbox = { items: [sessionItem("d1", approve())], answered: [] };
    const local = { items: [sessionItem("l1", approve()), sessionItem("l2", approve())], answered: [] };
    const call = vi.fn(async (origin: string, op: string, args: unknown) => {
      asked.push({ origin, op, args });
      if (op === "sessions-needs") {
        if (origin === "gpu-01") throw new ChanError({ layer: "hop" } as never, "够不着");
        return origin === "devbox" ? devbox : local;
      }
      return { items: [], total: 5, unreadable: [] };
    });
    typed.fake = call as never;
    const book = new NeedsBook();
    await book.refreshAll(["<local>", "devbox", "gpu-01"]);
    expect(asked.filter((x) => x.op === "sessions-needs").map((x) => x.origin).sort()).toEqual(["<local>", "devbox", "gpu-01"]);
    const merge = asked.filter((x) => x.op === "sessions-needs-merge");
    expect(merge.every((x) => x.origin === "<local>")).toBe(true);
    expect(merge.at(-1)!.args).toEqual({ machines: [{ origin: "<local>", list: local }, { origin: "devbox", list: devbox }] });
    expect(book.total(), "照后端合好的总数").toBe(5);
    expect(book.offline()).toEqual(["gpu-01"]);
    typed.fake = null;
  });

  it("窄窗两个字母取项目目录名里的字母；标题拆成项目那一格 ＋ 名字（都是核心给的；核心还没报 ⇒ sid 前 8 位）", () => {
    expect([abbrOf(tab("x", { projectDir: "/w/orders" })), abbrOf(tab("x", { projectDir: "/w/web-console" })), abbrOf(tab("x", { projectDir: "/w/数据集" }))]).toEqual(["or", "we", "数据"]);
    expect(titleParts(tab("x", { projectDir: "/w/orders", label: "加重试", labelProject: "orders" }))).toEqual({ proj: "orders", title: "加重试", bg: false, forked: false });
    expect(titleParts(tab("x", { projectDir: "/w/orders", label: "orders" }))).toEqual({ proj: null, title: "orders", bg: false, forked: false });
    expect(titleParts(tab("0123456789", { projectDir: "/w/orders" })).title).toBe("01234567");
  });

  it("时长一种写法：45s · 6m · 1h50m · 2h · 3d", () => {
    expect([45, 360, 6600, 7200, 3 * 86_400 + 5].map(fmtDur)).toEqual(["45s", "6m", "1h50m", "2h", "3d"]);
  });
});

/** 标签页栏台架：store 里摆好 tab，宿主全是替身；「需手动」的账里本机答的是在等的那几个（照 `ranked` 的先后；不给 ⇒ 条上的顺序）。 */
async function bar(tabs: Tab[], ranked?: string[]) {
  const store = new TabStore();
  for (const t of tabs) {
    store.tabs.set(t.sessionId, t);
    store.orderedIds.push(t.sessionId);
  }
  store.activeId = tabs[0]?.sessionId ?? null;
  const host = {
    refreshTabBar: vi.fn(),
    closeEndedIn: vi.fn(),
    openTabCwd: vi.fn().mockResolvedValue(undefined),
    bringTerminalToFront: vi.fn().mockResolvedValue(undefined),
    bringRemoteTerminalToFront: vi.fn().mockResolvedValue(undefined),
    closeTab: vi.fn(),
    switchTo: vi.fn(),
    answerAt: vi.fn(),
    pick: vi.fn(),
    isSelected: vi.fn().mockReturnValue(false),
    beginDrag: vi.fn(),
    takeSuppressedClick: vi.fn().mockReturnValue(false),
    beginGroupDrag: vi.fn(),
    takeSuppressedHeadClick: vi.fn().mockReturnValue(false),
    openMenu: vi.fn(),
    toggleSelect: vi.fn(),
    openMenuAt: vi.fn(),
    rereadAll: vi.fn().mockResolvedValue(undefined),
    reconnect: vi.fn(),
  } satisfies TabBarViewHost;
  const el = document.createElement("div");
  document.body.appendChild(el);
  const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, host);
  const waits = ranked ?? tabs.filter((t) => needsOf(t) !== null).map((t) => t.sessionId);
  const needs = fakeNeedsBook({ [LOCAL_ORIGIN]: listOf(waits.map((sid) => sessionItem(sid, needsOf(store.tabs.get(sid)!) ?? approve()))) });
  view.needs = { book: needs.book, login: vi.fn() };
  needs.book.subscribe(() => view.planNeedsChanged());
  view.refresh();
  await needs.book.refresh(LOCAL_ORIGIN);
  return { view, host: host as TabBarViewHost & typeof host, store, el, needs };
}

describe("标签页栏：「需手动 N」与机器离线条", () => {
  it("★ 「需手动」只在有人等你时出、写数；点它开浮层、点那一行跳过去；行尾「等批准」换掉未读数", async () => {
    const r = await bar([tab("a"), tab("b", { ...waiting, needs: approve(NOW - 10_000), unread: 3 }), tab("c")]);
    const strip = r.el.querySelector<HTMLElement>(".tab-needs")!;
    expect(strip.style.display).toBe("");
    expect(strip.querySelector(".tab-needs-count")?.textContent).toBe("1");
    const b = r.view.tabButtons.get("b")!;
    expect([b.needs.textContent, b.badge.textContent]).toEqual([copyText("beSession.needs.approve"), ""]);
    strip.click();
    document.querySelector<HTMLElement>('[data-row="session"]')!.click();
    expect(r.host.answerAt, "浮层里那一行 ⇒ 跳过去、焦点进答题块").toHaveBeenCalledWith("b");
    expect(r.host.switchTo).not.toHaveBeenCalled();
    // 答完了（那台重问回来不在等了）⇒ 那一条收起，未读数回来。
    (r.store.tabs.get("b") as Tab).face = liveFace("working");
    r.needs.answers[LOCAL_ORIGIN] = listOf([]);
    await r.needs.book.refresh(LOCAL_ORIGIN);
    r.view.refresh();
    expect(strip.style.display).toBe("none");
    expect(b.badge.textContent).toBe("3");
  });

  it("★ 悬停「需手动」不再开菜单（并进点开的那个浮层）；`Ctrl+J` 照旧直接跳后端排在最前的、不开浮层", async () => {
    vi.useFakeTimers();
    try {
      const r = await bar([tab("a", { ...waiting, needs: approve(NOW - 10_000) }), tab("b", { ...waiting, needs: approve(NOW - 60_000) })], ["b", "a"]);
      const strip = r.el.querySelector<HTMLElement>(".tab-needs")!;
      strip.dispatchEvent(new Event("mouseenter"));
      vi.advanceTimersByTime(2_000);
      expect(document.querySelector('[role="menu"]'), "悬停不开").toBeNull();
      expect(document.querySelector('[role="dialog"]'), "悬停不开").toBeNull();
      r.needs.asked.length = 0;
      await r.view.nextNeeds();
      expect(r.needs.asked, "跳之前先重问一回").toEqual([LOCAL_ORIGIN]);
      expect(r.host.answerAt).toHaveBeenCalledWith("b");
      expect(document.querySelector('[role="dialog"]'), "Ctrl+J 不开浮层").toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("★ 数 ＝ 后端合好的 total：合回 5、条上只有一个在等 ⇒ 入口写 5（界面不自己加）", async () => {
    const store = new TabStore();
    const t = tab("a", { ...waiting, needs: approve() });
    store.tabs.set("a", t);
    store.orderedIds.push("a");
    const el = document.createElement("div");
    document.body.appendChild(el);
    const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, (await bar([])).host);
    const needs = fakeNeedsBook({ [LOCAL_ORIGIN]: listOf([sessionItem("a", approve())]) }, (m) => ({ items: m.flatMap((x) => x.list.items.map((item) => ({ origin: x.origin, item }))), total: 5, unreadable: [] }));
    view.needs = { book: needs.book, login: vi.fn() };
    needs.book.subscribe(() => view.planNeedsChanged());
    await needs.book.refresh(LOCAL_ORIGIN);
    view.refresh();
    expect(view.needsCountNow()).toBe(5);
    expect(el.querySelector(".tab-needs .tab-needs-count")?.textContent).toBe("5");
  });

  it("★ 点标签上那颗「需手动」的点 ⇒ 跳过去、焦点进答题块；点标签别处 / 不在等的点 ⇒ 照常切", async () => {
    const r = await bar([tab("a"), tab("b", { ...waiting, needs: approve(NOW - 10_000) })]);
    r.view.tabButtons.get("b")!.root.querySelector<HTMLElement>(".tab-dot")!.click();
    expect(r.host.answerAt).toHaveBeenCalledWith("b");
    r.view.tabButtons.get("a")!.root.querySelector<HTMLElement>(".tab-dot")!.click();
    expect(r.host.switchTo).toHaveBeenCalledWith("a");
    expect(r.host.answerAt).toHaveBeenCalledTimes(1);
  });

  it("★ 挂着的请求换了（挂上 · 答掉 · 只能在终端里答那一句变了）⇒ 摘要变、订阅者（答题块 · 窗口标题）重画；种类没变也算", async () => {
    const r = await bar([tab("a", { ...waiting, needs: approve(NOW - 10_000) })]);
    const seen: unknown[] = [];
    r.store.subscribe((x) => seen.push(x));
    const n0 = seen.length;
    const t = r.store.tabs.get("a") as Tab;
    t.needs = { ...approve(NOW - 10_000), ask: { id: "ask-1" } as never };
    r.store.notify();
    expect(seen.length, "ask 挂上").toBe(n0 + 1);
    t.needs = { ...approve(NOW - 10_000), ask: null, manualText: "核心·那一句" };
    r.store.notify();
    expect(seen.length, "换成只能在终端里答").toBe(n0 + 2);
  });

  it("★ 机器离线：那台看不见、还有状态不明的会话 ⇒ 一条（离线 · 几个状态不明 · 采样多久前）＋［重新连接］；连上了自己消失", async () => {
    const r = await bar([tab("a"), tab("g1", { origin: "gpu-01" as never, face: null }), tab("g2", { origin: "gpu-01" as never, face: null })]);
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
    for (const sid of ["g1", "g2"]) (r.store.tabs.get(sid) as Tab).face = goldenFace("ended");
    r.view.refresh();
    expect(r.el.querySelector(".tab-machine-down-row")).toBeNull();
  });

  it("状态不明的行不当已结束画：虚线环、标题常色、行尾没有 ×（只右键能关）", async () => {
    const r = await bar([tab("g1", { origin: "gpu-01" as never, face: null })]);
    const b = r.view.tabButtons.get("g1")!;
    expect(b.dot.dataset.state).toBe("unknown");
    expect([b.root.classList.contains("unseen"), b.root.classList.contains("ended")]).toEqual([true, false]);
    expect(b.machine.textContent, "远端：标题前一个机器徽标（不再用方括号前缀）").toBe("gpu-01");
  });
});

describe("「需手动」· 窗口标题（消息流底部那一块见 answer-block.vitest.ts）", () => {
  it("★ 标题：`cc-monitor · 需手动 N`，0 个时只有 `cc-monitor`；变了才写", () => {
    const titles: string[] = [];
    const w = new NeedsTitle((t) => titles.push(t));
    w.observe(0);
    w.observe(0);
    w.observe(2);
    // N 同侧栏入口（会话 ＋ 计划项 ＋ 会话外的每一项）：会话没变、会话外多了一项 ⇒ 标题跟着变。
    w.observe(3);
    expect(titles).toEqual([copyText("needs.windowTitle.base"), copyText("needs.windowTitle.count", { n: 2 }), copyText("needs.windowTitle.count", { n: 3 })]);
  });
});

describe("会话头", () => {
  const head = (t: Tab | null) => {
    const host = { active: () => t, viewTerminal: vi.fn(), openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn(), planMark: () => null, menuItem: () => null, displayItems: () => [], tabOf: () => null, switchTo: vi.fn(), switching: NO_SWITCHING };
    const h = new SessionHead(host);
    document.body.appendChild(h.el);
    h.render(NOW);
    return { h, host };
  };
  // 右边那一排（换模型 · 切权限档那两颗另有判据：`session-switch.vitest.ts`）。
  const labels = (el: HTMLElement): string[] => [...el.querySelectorAll("button:not([data-switch])")].map((b) => b.getAttribute("aria-label") ?? b.textContent ?? "");

  it("★ 标题全名 · 机器 · 目录 · 状态一句；在等你时状态那一句琥珀；右边：目录（本机）· 看它的终端 · 显示 · 查找 · 更多", () => {
    const { h, host } = head(tab("a", { label: "加重试", labelProject: "orders", projectDir: "/w/orders", ...waiting, needs: approve() }));
    expect(h.el.textContent).toContain("orders 加重试");
    expect(h.el.textContent).toContain(copyText("sessionFace.machine.local"));
    expect(h.el.textContent).toContain("/w/orders");
    // 默认那一排（ui3 · `session-head-buttons.vitest.ts`）；这台宿主不给「⋯」里的项 ⇒ 借那份菜单的几颗（在新窗口打开 · 账号）不出。
    expect(labels(h.el)).toEqual([copyText("tabBarView.tab.cwdHint"), copyText("sessionHead.act.terminal"), copyText("sessionHead.act.display"), copyText("sessionHead.act.find"), copyText("tabBarView.tab.moreHint")]);
    h.el.querySelectorAll<HTMLButtonElement>("button:not([data-switch])")[1].click();
    expect(host.viewTerminal).toHaveBeenCalled();
    h.el.querySelectorAll<HTMLButtonElement>("button:not([data-switch])")[3].click();
    expect(host.find).toHaveBeenCalled();
  });

  it("★ 按状态多一颗：已结束［恢复］· Claude 已退出（远端）［在终端里打开］· 状态不明［重新连接］；活着不多", () => {
    expect(labels(head(tab("a", { face: goldenFace("ended") })).h.el)).toContain(copyText("sessionHead.act.resume"));
    const exited = head(tab("b", { origin: "devbox" as never, face: goldenFace("reconnectable") }));
    expect(labels(exited.h.el)).toContain(copyText("sessionHead.act.openTerm"));
    expect(labels(head(tab("b2", { face: goldenFace("reconnectable") })).h.el), "本机：没有接回终端的那条路，不画点了没反应的按钮").not.toContain(copyText("sessionHead.act.openTerm"));
    const unseen = head(tab("c", { origin: "gpu-01" as never, face: null }));
    const re = [...unseen.h.el.querySelectorAll("button")].find((b) => b.textContent === copyText("sessionHead.act.reconnect"))!;
    re.click();
    expect(unseen.host.reconnect).toHaveBeenCalledWith("gpu-01");
    expect(labels(head(tab("e", { face: goldenFace("ended") })).h.el), "已结束：没有终端可去，不出「看它的终端」").not.toContain(copyText("sessionHead.act.terminal"));
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
    const host = { active: () => (cur.t ? { kind: "tab" as const, tab: cur.t } : null), front: vi.fn(), focusStream: vi.fn() };
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
    const c = rig([], { t: tab("c", { face: goldenFace("ended") }) });
    c.page.setVisible(true);
    await flush();
    expect(c.page.el.textContent).toContain(copyText("terminal.empty.ended", { state: copyText("sessionState.ended.name") }));
  });

  it("★ 活着、不在 tmux 里（容器 none）⇒ 照实说「不在 tmux 里」，不说「非 cc-monitor 启动」；↗ 真能用时给［切到终端］", async () => {
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    try {
      const d = rig([], { t: tab("d", { face: liveFace("working", false) }) });
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
      const e = rig([], { t: tab("e", { face: liveFace("working", false) }) });
      e.page.setVisible(true);
      await flush();
      expect([...e.page.el.querySelectorAll("button")].some((x) => x.textContent?.includes(copyText("terminal.head.front")))).toBe(false);
      const f = rig([], { t: tab("f", { face: liveFace("working", true) }) });
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

  it("★ 输入框占位只写「发消息…」；快捷键那句挪进悬停（文案表那一句）；送不了时不挂（键不起作用）", async () => {
    const r = rig([row("a")], { t: tab("a") });
    r.page.setVisible(true);
    await flush();
    const tip = (b: HTMLTextAreaElement): string => b.getAttribute("title") ?? b.dataset.kitTitle ?? "";
    expect(box(r.page).placeholder).toBe(copyText("chat.input.placeholder"));
    expect(tip(box(r.page))).toBe(copyText("terminal.input.keysHint"));
    const no = rig([row("a", { inputNo: "那台写的一句 · 不归这边" })], { t: tab("a") });
    no.page.setVisible(true);
    await flush();
    expect(tip(box(no.page))).toBe("");
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
    expect(terminalActsOf(tab("b", { face: goldenFace("ended") })).front).toBe(false);
    vi.mocked(terminalFrontAvailable).mockReturnValue(false);
    expect(terminalActsOf(tab("a"))).toEqual({ front: false, attach: false });
    expect(terminalActsOf(tab("c", { origin: "devbox" as never, face: goldenFace("reconnectable") }))).toEqual({ front: false, attach: true });
    expect(terminalActsOf(tab("d", { face: goldenFace("reconnectable") })).attach, "本机：没有接回终端的那条路").toBe(false);
  });

  it("报错卡摆出两颗（看不看得见由消息流根上的标记管），点了的那颗带着它是哪条路", () => {
    const card = buildApiErrorCard({ timeLabel: "02:10", text: "overloaded" });
    expect([...card.querySelectorAll<HTMLElement>(".api-error-acts [data-act]")].map((b) => [b.dataset.act, b.textContent])).toEqual([
      ["front", copyText("terminal.head.front")],
      ["attach", copyText("sessionHead.act.openTerm")],
    ]);
  });
});

// 窗口标题起步就是产品名：三份入口 HTML 的 <title> 与壳 / 需手动那一路设的基础标题同字（不写死别的产品名）。
describe("入口 HTML 的 <title> == 窗口的基础标题", () => {
  it("index · settings · viewer 三份都是 needs.windowTitle.base", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const { REPO_ROOT } = await import("../../test-support/repo-root.ts");
    const got = ["index.html", "settings.html", "viewer.html"].map((f) => /<title>([^<]*)<\/title>/.exec(readFileSync(resolve(REPO_ROOT, f), "utf8"))?.[1]);
    expect(got).toEqual(Array(3).fill(copyText("needs.windowTitle.base")));
  });
});
