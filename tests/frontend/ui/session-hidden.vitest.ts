/**
 * 会话隐藏 · 桌面界面（设计稿 `会话隐藏.md` §4.3）：界面只照帧上那一格 `hidden: {by, said}` 画，不判。
 *
 * - 帧上那一格怎么读（读不懂 ⇒ 当没隐藏）；
 * - 标签栏栏底「已隐藏 N」那一行照那一格数、只列活的；隐藏的在等人「需手动」照样数、Ctrl+J 照样到；
 * - 右键 / 批量「隐藏」交 `hide: "on"`、撤销交 `hide: "off"`；推帧到了才挪；挪进去摘固定 · 出组，撤销原位放回；
 * - 会话头「由 X 启动」「子会话 N」读会话事实那两格；
 * - 设置：行为页「显示分身会话」那一格整条没了；机器页两行交 `childTab` · `bgTab`。
 *
 * 夹具只造结构（sid · 路径 · 后端那一句的占位），不采会话正文。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => {}), listen: vi.fn(async () => () => {}) }));
vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));
vi.mock("../../../src/frontend/ui/machine-prefs", () => ({ readPrefs: vi.fn(), setPrefs: vi.fn() }));
vi.mock("../../../src/frontend/ui/history-reads", () => ({ annotate: vi.fn(async () => ({ entry: {} })) }));

import { TabStore } from "../../../src/frontend/ui/tab-store";
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { Needs, SessionFacts } from "../../../src/frontend/ui/session-reads";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ENDED, LIVE } from "../../../src/frontend/ui/tab-session-state";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { hiddenOf, lineageOf, NO_LINEAGE, type HideSet, type SessionHidden } from "../../../src/frontend/ui/session-hidden";
import { setHide } from "../../../src/frontend/ui/session-hidden-cmds";
import { TabHide, type TabHideHost } from "../../../src/frontend/ui/tab-hide";
import { applyFacts } from "../../../src/frontend/ui/tab-session-facts";
import { SessionHead } from "../../../src/frontend/ui/session-head";
import { pushedProducts } from "../../../src/frontend/ui/changed-stream";
import { behaviorIn } from "../../../src/frontend/ui/behavior";
import { KNOWN_CONFIG_KEYS } from "../../../src/frontend/ui/config";
import { annotate } from "../../../src/frontend/ui/history-reads";
import { readPrefs, setPrefs } from "../../../src/frontend/ui/machine-prefs";
import { machineLaunchRows } from "../../../src/frontend/ui/settings/machine-launch-rows";
import { openBatchMenu, type TabBatchHost } from "../../../src/frontend/ui/tab-batch-menu";
import { TabMenu, type TabMenuHost } from "../../../src/frontend/ui/tab-menu";
import type { TabSessionActions } from "../../../src/frontend/ui/tab-session-actions";
import TABLE from "../../../src/shared/copy/table.json";

const NOW = Date.parse("2026-10-14T12:00:00Z");
const BG: SessionHidden = { by: "bg", said: "后端那一句·分身" };
const SELF: SessionHidden = { by: "self", said: "后端那一句·单个" };
const waiting = { doing: "needs_you" as const, waitingFor: "permission prompt", text: copyText("beSession.activity.needsYou"), tone: "need", order: 1 };
const approve = (): Needs => ({ kind: "approve", tool: "Bash", call: "toolu_b", what: "占位命令", sinceMs: NOW - 60_000, text: copyText("beSession.needs.approve"), head: "核心·等批准", headCode: null, tone: "need", rank: 1, waitedMs: 60_000, waitedText: "1m", ask: null, manualText: null, receivedAt: NOW });

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: sid,
    background: false,
    bgName: null,
    label: `名-${sid}`,
    labelProject: null,
    projectDir: `/w/${sid}`,
    state: LIVE,
    pinned: false,
    group: null,
    hidden: null,
    lineage: NO_LINEAGE,
    origin: LOCAL_ORIGIN,
    activity: null,
    forkedFromSessionId: null,
    writers: [],
    unread: 0,
    needs: null,
    backgroundWork: null,
    pending: [],
    lastSay: null,
    touchedFiles: new Set(),
    usage: null,
    retries: new Map(),
    ...over,
  } as unknown as Tab;
}

function bar(tabs: Tab[], prefs: Partial<TabBarPrefs> = {}) {
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
  const p = { collections: [], hiddenOpen: false, setHiddenOpen: vi.fn(async (on: boolean) => void (p.hiddenOpen = on)), ...prefs } as unknown as TabBarPrefs & { hiddenOpen: boolean };
  const view = new TabBarView(store, p, el, host);
  view.refresh();
  return { view, host, store, el, prefs: p };
}

const hiddenRow = (el: HTMLElement): HTMLElement | null => el.querySelector<HTMLElement>(".tab-hidden");
const rowSids = (r: ReturnType<typeof bar>, inHidden: boolean): string[] =>
  [...r.view.tabButtons].filter(([, b]) => r.el.contains(b.root) && (b.root.closest(".tab-hidden") !== null) === inHidden).map(([sid]) => sid);

beforeEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("帧上那一格怎么读（界面不判，读不懂 ⇒ 当没隐藏）", () => {
  it("★ {by, said} 照抄；缺 · null · 认不出的档 · 少 said ⇒ null", () => {
    expect(hiddenOf({ by: "alias", said: "s" })).toEqual({ by: "alias", said: "s" });
    for (const by of ["self", "alias", "child", "bg"]) expect(hiddenOf({ by, said: "x" })?.by).toBe(by);
    expect(hiddenOf(undefined)).toBeNull();
    expect(hiddenOf(null)).toBeNull();
    expect(hiddenOf({ by: "rule", said: "x" })).toBeNull();
    expect(hiddenOf({ by: "self" })).toBeNull();
  });

  it("推帧主题「会话隐藏」一批：带了那一格的按会话落（null ＝ 不隐藏了）", () => {
    const p = pushedProducts({ cells: [{ key: "a", rev: null, body: BG }, { key: "b", rev: null, body: null }], all: false }, hiddenOf);
    expect([...p.got]).toEqual([["a", BG], ["b", null]]);
  });
});

describe("标签栏：栏底「已隐藏 N」那一行照 hidden 格画", () => {
  it("★ 隐藏的不占标签页、进栏底那一行；数只数活的（隐藏了又结束的不列）；没有 ⇒ 不画；格去掉了 ⇒ 回到栏上", () => {
    const r = bar([tab("a"), tab("b", { hidden: BG }), tab("c", { hidden: SELF }), tab("d", { hidden: SELF, state: ENDED })]);
    const row = hiddenRow(r.el)!;
    expect(row, "有活的隐藏会话 ⇒ 栏底一行").not.toBeNull();
    expect(row.querySelector(".tab-group-name")?.textContent).toBe(copyText("tabBar.hidden.row", { n: 2 }));
    expect(r.el.querySelector(".tab-list")!.lastElementChild, "在栏的最后").toBe(row);
    expect(rowSids(r, false)).toEqual(["a"]);
    expect(rowSids(r, true).sort()).toEqual(["b", "c"]);
    expect(r.view.tabButtons.has("d"), "隐藏了又结束的：哪儿都不画").toBe(false);
    expect(row.classList.contains("is-collapsed"), "缺省收着").toBe(true);
    for (const id of ["b", "c"]) (r.store.tabs.get(id) as Tab).hidden = null;
    r.view.refresh();
    expect(hiddenRow(r.el)?.isConnected ?? false, "没有活的隐藏会话 ⇒ 不画").toBe(false);
    expect(rowSids(r, false).sort()).toEqual(["a", "b", "c"]);
  });

  it("★ 隐藏的会话在等人：「需手动」照样数它、点它照样跳过去；收着的那一行汇总那颗需手动的点", () => {
    const r = bar([tab("a"), tab("b", { hidden: BG, activity: waiting, needs: approve() })]);
    const strip = r.el.querySelector<HTMLElement>(".tab-needs")!;
    expect(strip.style.display).toBe("");
    expect(strip.querySelector(".tab-needs-count")?.textContent).toBe("1");
    strip.click();
    expect(r.host.answerAt).toHaveBeenCalledWith("b");
    expect(hiddenRow(r.el)!.querySelector(".tab-group-chip[data-dot='needs-you']")?.textContent).toBe(copyText("tabBar.group.sumNeeds", { n: 1 }));
  });

  it("点组头收 / 展（记进 `tabBar.hiddenOpen`）；收着时数字键走不到里面的，展开了走得到；里面那一行不起拖", () => {
    const r = bar([tab("a"), tab("b", { hidden: SELF })]);
    expect(r.view.navOrder()).toEqual(["a"]);
    expect(r.view.visibleOrder(), "需手动 · 多选的顺序含它").toEqual(["a", "b"]);
    hiddenRow(r.el)!.querySelector<HTMLElement>(".tab-group-head")!.click();
    expect(r.prefs.setHiddenOpen).toHaveBeenCalledWith(true);
    r.view.refresh();
    expect(hiddenRow(r.el)!.classList.contains("is-collapsed")).toBe(false);
    expect(r.view.navOrder()).toEqual(["a", "b"]);
    r.view.tabButtons.get("b")!.root.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    expect(r.host.beginDrag, "不能往外拖").not.toHaveBeenCalled();
    r.view.tabButtons.get("a")!.root.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    expect(r.host.beginDrag).toHaveBeenCalledTimes(1);
  });

  it("悬停卡写后端那一句", () => {
    const r = bar([tab("a"), tab("b", { hidden: BG })]);
    expect(r.view.hoverCard("b")?.textContent).toContain(BG.said);
  });
});

describe("隐藏 / 取消隐藏：交的命令 · 撤销 · 推帧到了才挪", () => {
  function rig(tabs: Tab[]) {
    const store = new TabStore();
    for (const t of tabs) {
      store.tabs.set(t.sessionId, t);
      store.orderedIds.push(t.sessionId);
    }
    store.activeId = tabs[0].sessionId;
    const prefs = {
      collections: [{ id: "g", name: "组" }],
      setPinnedMany: vi.fn((sids: string[], on: boolean) => sids.forEach((s) => ((store.tabs.get(s) as Tab).pinned = on))),
      forgetTab: vi.fn(async (t: Tab) => void (t.group = null)),
      joinGroup: vi.fn(async (sid: string, gid: string) => void ((store.tabs.get(sid) as Tab).group = gid)),
      persistOrder: vi.fn(async () => {}),
    };
    const toasts: { title: string; undo: () => void; settle: () => void }[] = [];
    const host = {
      setHide: vi.fn(async (_t: Tab, _to: HideSet) => {}),
      undoToast: vi.fn((title: string, undo: () => void, settle: () => void) => void toasts.push({ title, undo, settle })),
      failed: vi.fn(),
      refreshTabBar: vi.fn(),
      close: vi.fn(),
    } satisfies TabHideHost;
    const h = new TabHide(store, prefs as unknown as TabBarPrefs, host);
    return { h, host, store, prefs, toasts };
  }

  it("★ 隐藏一个：交 `on`、toast「已隐藏「名」」、栏上先不挪；撤销交 `off`", async () => {
    const r = rig([tab("a"), tab("b")]);
    r.h.hide(["b"]);
    await Promise.resolve();
    expect(r.host.setHide.mock.calls.map((c) => [(c[0]).sessionId, c[1]])).toEqual([["b", "on"]]);
    expect(r.toasts.map((t) => t.title)).toEqual([copyText("tabBar.hide.done", { title: "名-b" })]);
    expect(r.store.tabs.get("b")!.hidden, "界面不先挪：等推帧").toBeNull();
    r.toasts[0].undo();
    await Promise.resolve();
    expect(r.host.setHide.mock.calls.at(-1)?.[1]).toBe("off");
  });

  it("批量隐藏：一条「已隐藏 N 个」；已经隐藏的跳过", () => {
    const r = rig([tab("a"), tab("b"), tab("c", { hidden: SELF })]);
    r.h.hide(["a", "b", "c"]);
    expect(r.host.setHide).toHaveBeenCalledTimes(2);
    expect(r.toasts[0].title).toBe(copyText("tabBar.hide.doneMany", { n: 2 }));
  });

  it("★ 推帧说隐藏了 ⇒ 摘固定、出组；撤销期里推帧说不隐藏了 ⇒ 原位 · 原分组 · 原固定放回", () => {
    const r = rig([tab("a"), tab("b", { pinned: true, group: "g" }), tab("c")]);
    r.h.hide(["b"]);
    expect(r.h.land("b", SELF)).toBe(true);
    const b = r.store.tabs.get("b")!;
    expect([b.hidden, b.pinned, b.group]).toEqual([SELF, false, null]);
    expect(r.h.land("b", SELF), "同一格再来一次 ⇒ 不动").toBe(false);
    r.toasts[0].undo();
    expect(r.h.land("b", null)).toBe(true);
    expect([b.hidden, b.pinned, b.group]).toEqual([null, true, "g"]);
    expect(r.store.orderedIds).toEqual(["a", "b", "c"]);
  });

  it("别处取消隐藏（不是撤销）⇒ 回到散的标签页末尾；取消隐藏规则隐藏的也交 `off`", () => {
    const r = rig([tab("a", { hidden: BG }), tab("b"), tab("c")]);
    r.h.unhide(["a", "b"]);
    expect(r.host.setHide.mock.calls.map((c) => [(c[0]).sessionId, c[1]])).toEqual([["a", "off"]]);
    r.h.land("a", null);
    expect(r.store.orderedIds).toEqual(["b", "c", "a"]);
  });

  it("隐藏了又结束的：不是正看着的就收走；正看着的切走再收", () => {
    const r = rig([tab("a", { hidden: SELF, state: ENDED }), tab("b", { hidden: SELF, state: ENDED })]);
    r.h.reap();
    expect(r.host.close.mock.calls).toEqual([["b"]]);
  });

  it("命令交到会话所在那台：`history-annotate {sid, patch: {hide}}`", async () => {
    await setHide("gpu-01", "s1", "rule");
    expect(vi.mocked(annotate)).toHaveBeenCalledWith("gpu-01", "s1", { hide: "rule" });
  });
});

describe("菜单：右键两项 · 批量两项", () => {
  const menuHost = (t: Tab): TabMenuHost & { hide: ReturnType<typeof vi.fn>; unhide: ReturnType<typeof vi.fn> } =>
    ({
      tab: () => t,
      isAttachable: () => true,
      collectionsLoaded: () => true,
      collections: () => [],
      groupOf: () => null,
      joinGroup: vi.fn(),
      foundGroup: vi.fn(),
      leaveGroup: vi.fn(),
      pinnedLoaded: () => true,
      togglePin: vi.fn(),
      close: vi.fn(),
      hide: vi.fn(),
      unhide: vi.fn(),
      openCwd: vi.fn(),
      front: vi.fn(),
      viewTerminal: vi.fn(),
    }) as never;
  const labels = (): string[] => [...document.body.querySelectorAll("[role^=menuitem] [data-part=label]")].map((x) => x.textContent ?? "");
  const press = (label: string): void => {
    const b = [...document.body.querySelectorAll<HTMLElement>("[role^=menuitem]")].find((x) => x.querySelector("[data-part=label]")?.textContent === label);
    expect(b, `菜单里没有「${label}」：${labels().join(" | ")}`).toBeDefined();
    b!.click();
  };

  it("★ 活的标签页：有「隐藏」、点了交宿主；「已隐藏」里的：只有「取消隐藏」，不固定、不分组", () => {
    const t = tab("a");
    const h = menuHost(t);
    new TabMenu(h, {} as TabSessionActions).open(new MouseEvent("contextmenu"), "a");
    expect(labels()).toContain(copyText("tabMenu.pin.pin"));
    press(copyText("tabBar.menu.hide"));
    expect(h.hide).toHaveBeenCalledWith(["a"]);
    document.body.innerHTML = "";
    const u = tab("u", { hidden: BG });
    const hu = menuHost(u);
    new TabMenu(hu, {} as TabSessionActions).open(new MouseEvent("contextmenu"), "u");
    expect(labels()).not.toContain(copyText("tabBar.menu.hide"));
    expect(labels()).not.toContain(copyText("tabMenu.pin.pin"));
    expect(labels()).not.toContain(copyText("tabMenu.collection.add"));
    press(copyText("tabBar.menu.unhide"));
    expect(hu.unhide).toHaveBeenCalledWith(["u"]);
  });

  it("批量：「隐藏（n）」「取消隐藏（n）」各数各的，交宿主", async () => {
    const tabs = [tab("a"), tab("b"), tab("c", { hidden: SELF })];
    const host = { tab: (s: string) => tabs.find((t) => t.sessionId === s), collectionsLoaded: () => false, collections: () => [], pinnedLoaded: () => false, setPinned: vi.fn(), joinGroup: vi.fn(), foundGroup: vi.fn(), leaveGroup: vi.fn(), closeTabs: vi.fn(), hide: vi.fn(), unhide: vi.fn() } satisfies TabBatchHost;
    openBatchMenu(new MouseEvent("contextmenu"), ["a", "b", "c"], host, { stop: vi.fn(), start: vi.fn(), confirm: vi.fn() } as never);
    expect(labels()).toEqual(expect.arrayContaining([copyText("tabBatch.menu.hide", { n: 2 }), copyText("tabBatch.menu.unhide", { n: 1 })]));
    press(copyText("tabBatch.menu.hide", { n: 2 }));
    await new Promise((r) => setTimeout(r, 0));
    expect(host.hide).toHaveBeenCalledWith(["a", "b"]);
  });
});

describe("会话头：「由 X 启动」「子会话 N」读会话事实那两格", () => {
  const facts = (extra: Record<string, unknown>): SessionFacts =>
    ({ forkedFrom: null, touchedFiles: [], usage: null, projectDir: null, agent: null, writers: [], needs: null, pending: [], lastSay: null, background: null, retries: [], ...extra }) as unknown as SessionFacts;

  it("★ 会话事实带的 `startedBy` · `children` 落到 tab 上；没带 ⇒ 没有；变了才报变", () => {
    const t = tab("a");
    const ch = applyFacts(t, facts({ startedBy: { sid: "p", title: "主·名" }, children: [{ sid: "k1", title: "子·一", live: true, hidden: { by: "child", said: "s" } }, { sid: "k2", title: "子·二", live: false }] }));
    expect(ch.lineage).toBe(true);
    expect(t.lineage).toEqual({ startedBy: { sid: "p", title: "主·名" }, children: [{ sid: "k1", title: "子·一", live: true, hidden: { by: "child", said: "s" } }, { sid: "k2", title: "子·二", live: false, hidden: null }] });
    expect(applyFacts(t, facts({ startedBy: { sid: "p", title: "主·名" }, children: [{ sid: "k1", title: "子·一", live: true, hidden: { by: "child", said: "s" } }, { sid: "k2", title: "子·二", live: false }] })).lineage).toBe(false);
    applyFacts(t, facts({}));
    expect(t.lineage).toEqual(NO_LINEAGE);
    expect(lineageOf({ startedBy: { sid: 1 } })).toEqual(NO_LINEAGE);
  });

  it("★ 会话头：已隐藏标（悬停是那一句）· 由 X 启动（点了切回主会话）· 子会话 N（点开列子会话、点一行切过去）", () => {
    const parent = tab("p");
    const kid = tab("k1");
    const t = tab("a", { hidden: SELF, lineage: { startedBy: { sid: "p", title: "主·名" }, children: [{ sid: "k1", title: "子·一", live: true, hidden: BG }, { sid: "gone", title: "子·二", live: false, hidden: null }] } });
    const host = { active: () => t, viewTerminal: vi.fn(), openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn(), planMark: () => null, menuItem: () => null, displayItems: () => [], tabOf: (sid: string) => (sid === "p" ? parent : sid === "k1" ? kid : null), switchTo: vi.fn() };
    const h = new SessionHead(host);
    document.body.appendChild(h.el);
    h.render(NOW);
    expect(h.el.querySelector("[data-head=hidden]")?.textContent).toBe(copyText("sessionHead.kin.hidden"));
    const p = h.el.querySelector<HTMLButtonElement>("[data-head=parent]")!;
    expect(p.textContent).toBe(copyText("sessionHead.kin.parent", { title: "主·名" }));
    p.click();
    expect(host.switchTo).toHaveBeenCalledWith("p");
    const c = h.el.querySelector<HTMLButtonElement>("[data-head=children]")!;
    expect(c.textContent).toBe(copyText("sessionHead.kin.children", { n: 2 }));
    c.click();
    const rows = [...document.body.querySelectorAll<HTMLElement>("[role^=menuitem]")];
    expect(rows.map((x) => x.querySelector("[data-part=label]")?.textContent)).toEqual(["子·一", "子·二"]);
    rows[0].click();
    expect(host.switchTo).toHaveBeenLastCalledWith("k1");
  });

  it("没有血缘、不隐藏 ⇒ 一颗都不出", () => {
    const t = tab("a");
    const h = new SessionHead({ active: () => t, viewTerminal: vi.fn(), openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn(), planMark: () => null, menuItem: () => null, displayItems: () => [], tabOf: () => null, switchTo: vi.fn() });
    h.render(NOW);
    expect(h.el.querySelectorAll("[data-head=hidden], [data-head=parent], [data-head=children]").length).toBe(0);
  });
});

describe("设置：行为页那个开关不在了 · 机器页两行", () => {
  it("★ 「显示分身会话」整条删了：行为里没有那一格、配置键不登记、文案表里没有那三条", () => {
    expect("showBgSessions" in behaviorIn({ showBgSessions: false })).toBe(false);
    expect(KNOWN_CONFIG_KEYS as readonly string[]).not.toContain("showBgSessions");
    const keys = Object.keys((TABLE as { entries: Record<string, unknown> }).entries);
    for (const k of ["settingsPanel.behavior.showHiddenTasks", "settingsPanel.behavior.showHiddenTasksHelp", "settingsPanel.behavior.hiddenTasks"]) expect(keys).not.toContain(k);
  });

  it("★ 机器页：子会话 · 分身会话两行，照那台读回的画；点了交 `prefs-set {childTab | bgTab}`", async () => {
    vi.mocked(readPrefs).mockResolvedValue({ state: "ok", prefs: { launchers: {}, resumeInTmux: false, models: {}, contextLimits: {}, agentHomes: {}, childTab: "hide", bgTab: "show" } } as never);
    vi.mocked(setPrefs).mockResolvedValue({ state: "ok", prefs: { launchers: {}, resumeInTmux: false, models: {}, contextLimits: {}, agentHomes: {}, childTab: "show", bgTab: "show" } } as never);
    const m = machineLaunchRows(LOCAL_ORIGIN, "m-1");
    document.body.append(...m.rows);
    m.load();
    await new Promise((r) => setTimeout(r, 0));
    const seg = (role: string): HTMLElement => document.body.querySelector<HTMLElement>(`[data-role=${role}]`)!;
    const on = (role: string): string | undefined => seg(role).querySelector<HTMLElement>("[aria-checked=true]")?.dataset.key;
    expect([on("child-tab"), on("bg-tab")]).toEqual(["hide", "show"]);
    expect(seg("child-tab").textContent).toContain(copyText("settings.machine.childTab"));
    seg("child-tab").querySelector<HTMLElement>("[data-key=show]")!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(setPrefs)).toHaveBeenCalledWith(LOCAL_ORIGIN, { childTab: "show" });
    expect(on("child-tab")).toBe("show");
  });
});
