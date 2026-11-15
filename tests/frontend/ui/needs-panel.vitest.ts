/**
 * 侧栏顶上「需手动 n」点开的浮层：读的是各台 `sessions-needs` 整份、本机合好的那一份（`needs-book.ts`）。
 *
 * - n ＝ 合好的总数（在等的会话 ＋ 计划项 ＋ 会话外的每一项）；窗口标题同一个数（`needs-bar` 那一份见 `main-window-needs.vitest.ts`）。
 * - 浮层：会话行（照「需手动」的序）· 分隔线 · 会话外每一项（第一行 `said`、第二行机器名；`fix` 为空 ⇒ 第二行写 `why`、不画按钮）；
 *   没问到的那台一条，照后端写的那一句（够不着 · 常驻不在 · 答了失败）；那台会外那一截读不出 ⇒ 入口不带数字、虚线框，底部一条「{machine} · 需手动清单无法读取」（那台那一句在悬停里）。
 * - 点会话行 ⇒ 切过去；点会话外那一行或［登录…］⇒ 开登录抽屉（交那一项）；↑↓ · Enter · Esc。最后一项刚解决、浮层开着 ⇒「无需手动事项」，关了入口才消失。
 * - 各台从哪来：本机后端 `sessions-needs-merge` 问到各台再合（替身 `needs-fake.ts`；扇出与合的判法在后端判据里）。
 *
 * 夹具只造结构（占位的号名 · 服务器名 · 句子），不采真正文。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));

import { invoke } from "@tauri-apps/api/core";
import { TabStore } from "../../../src/frontend/ui/tab-store";
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Needs } from "../../../src/frontend/ui/session-reads";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import { LOCAL, type Machine } from "../../../src/frontend/ui/ipc/origin";
import { liveFace } from "../../test-support/session-faces";
import type { ManualItem } from "../../../src/frontend/ui/login-reads";
import { fakeNeedsBook, listOf, manualItem, sessionItem, type NeedsRead } from "../../test-support/needs-fake";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { icon } from "../../../src/frontend/ui/kit/icon";
import { renderNeedsPanel } from "../../../src/frontend/ui/needs-panel";
import { noteMachine } from "../../../src/frontend/ui/ipc/chan-caller";
import { KeybindingDispatcher } from "../../../src/frontend/ui/keybindings/registry";
import type { NeedsAt } from "../../../src/frontend/ui/generated/NeedsAt";
import type { NeedsColumn } from "../../../src/frontend/ui/generated/NeedsColumn";
import type { NeedsMerged } from "../../../src/frontend/ui/generated/NeedsMerged";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 6; i++) await tick();
};

/** 答得了的那一条里的一个选项（线上形要有至少一项）。 */
const CHOICE = { id: "allow", text: "核心·允许", look: "main", note: "none", row: "main", picks: false } as const;

/** 在等人：脸 ＋ 需手动那一份。 */
const waiting = {
  face: liveFace("needs_you"),
  needs: { kind: "approve", tool: null, call: null, what: null, text: "核心·需手动", head: "核心·需手动", headCode: null, tone: "need", rank: 1, waitedMs: null, ask: null, manualText: "核心·只能在终端里答", receivedAt: 0 } as Needs,
};

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid, title: sid, background: false, bgName: null, label: null, labelProject: null, projectDir: `/w/${sid}`, face: liveFace("working"), pinned: false, group: null,
    origin: LOCAL, forkedLine: null, whereLine: null, unreadSaid: null, needs: null, backgroundWork: null, pending: [], lastSay: null, ...over,
  } as unknown as Tab;
}

const acctItem = (account: string): ManualItem => ({ kind: "accountLogin", agent: "claude", account, said: `核心·${account} 需登录`, fix: { command: "login-start", args: { kind: "account", agent: "claude", account } } });
const mcpItem = (server: string, fix = true): ManualItem =>
  fix
    ? { kind: "mcpLogin", account: "w1", server, scope: "user", said: `核心·MCP ${server} 需登录`, fix: { command: "login-start", args: { kind: "mcp", agent: "claude", account: "w1", server, projectDir: null } } }
    : { kind: "mcpLogin", account: "w1", server, scope: "user", said: `核心·MCP ${server} 需登录`, fix: null, why: "核心·不支持在 app 里登录" };

/** 一台的整份：条上在等的那几个（`waits`）＋ 会外几项；`unreadable` ⇒ 会外那一截读不出（那一句）。 */
const machine = (waits: string[], manual: ManualItem[], unreadable?: string): NeedsRead =>
  listOf([...waits.map((sid) => sessionItem(sid, waiting.needs)), ...(unreadable === undefined ? manual.map(manualItem) : [])], unreadable);

/** 台架：标签页栏 ＋ 一本账（每台答什么由 `answers` 定，改了再 `refresh`）。 */
function rig(tabs: Tab[], answers: Record<string, NeedsRead>, reply?: Parameters<typeof fakeNeedsBook>[1]) {
  const store = new TabStore();
  for (const t of tabs) {
    store.tabs.set(t.sessionId, t);
    store.orderedIds.push(t.sessionId);
  }
  store.activeId = tabs[0]?.sessionId ?? null;
  const host = {
    refreshTabBar: vi.fn(), closeEndedIn: vi.fn(),
    closeTab: vi.fn(), switchTo: vi.fn(), answerAt: vi.fn(), pick: vi.fn(),
    isSelected: vi.fn().mockReturnValue(false), beginDrag: vi.fn(), takeSuppressedClick: vi.fn().mockReturnValue(false), beginGroupDrag: vi.fn(),
    takeSuppressedHeadClick: vi.fn().mockReturnValue(false), openMenu: vi.fn(), toggleSelect: vi.fn(), openMenuAt: vi.fn(), front: vi.fn(), fixMachine: vi.fn(),
  } satisfies TabBarViewHost;
  const el = document.createElement("div");
  document.body.appendChild(el);
  const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, host);
  const { book, asked } = fakeNeedsBook(answers, reply);
  const login = vi.fn();
  view.needs = { book, login };
  book.subscribe(() => view.planNeedsChanged());
  view.refresh();
  const strip = btn(el, "manual");
  const asksBtn = btn(el, "answerable");
  return { view, host, store, el, book, login, asked, strip, asksBtn, answers };
}

/** 标签栏上那一列的那颗（待答请求 · 需手动）。 */
const btn = (el: HTMLElement, col: "answerable" | "manual"): HTMLElement => el.querySelector<HTMLElement>(`.tab-needs[data-col="${col}"]`)!;
/** 那颗上的数（不带数 ⇒ 空）。 */
const countOf = (b: HTMLElement): string => b.querySelector(".tab-needs-count")?.textContent ?? "";

/** 合好的一列（替身回包用：几项 · 头那一句 · 说没说准由这里摆）。 */
const col = (items: NeedsAt[], over: Partial<NeedsColumn> = {}): NeedsColumn => ({ items, total: items.length, head: `核心·头 ${items.length}`, sure: true, ...over });
/** 合好的整份（两列 ＋ 别的照空）。 */
const cols = (asks: NeedsColumn, manual: NeedsColumn, over: Partial<NeedsMerged> = {}) => (): NeedsMerged => ({ asks, manual, unreadable: [], down: [], ended: [], windowTitle: "", ...over });
const atLocal = (item: NeedsAt["item"]): NeedsAt => ({ origin: LOCAL, item });
/** 界面答得了的那一条（那台交了 ask）· 答不了的那一条（那台写好了 manualText）。 */
const answerable = { ...waiting.needs, manualText: null, askHead: "核心·需手动", ask: { id: "ask-1", kind: "approve", tool: "Bash", run: null, what: null, detail: "", choices: [CHOICE], questions: [] } } as Needs;
const inTerminal = { ...waiting.needs, manualText: "核心·在终端里答" } as Needs;
const planAt = (slice: string): NeedsAt => atLocal({ kind: "plan", rank: 9, workspace: "/w", slice });

const panel = () => document.querySelector<HTMLElement>('[role="dialog"] [role="listbox"]');
const rows = () => [...(panel()?.querySelectorAll<HTMLElement>("[data-row]") ?? [])];

beforeEach(() => {
  document.body.innerHTML = "";
  invokeMock.mockReset();
});

describe("标签栏两颗：待答请求 · 需手动各管一列", () => {
  it("★ 一列一条 ⇒ 两颗各带 1（待答请求在前）；一列说不准 ⇒ 那颗不带数、另一颗照带；两列都 0 且准 ⇒ 两颗都不在", async () => {
    const a = atLocal(sessionItem("a", answerable));
    const m = atLocal(sessionItem("m", inTerminal));
    const r = rig([tab("a", waiting), tab("m", waiting)], {}, cols(col([a]), col([m])));
    await r.book.again();
    expect([r.asksBtn.style.display, r.strip.style.display]).toEqual(["", ""]);
    expect([countOf(r.asksBtn), countOf(r.strip)]).toEqual(["1", "1"]);
    expect(r.asksBtn.querySelector(".tab-needs-text")?.textContent).toBe(copyText("needs.asks.title"));
    expect(r.strip.querySelector(".tab-needs-text")?.textContent).toBe(copyText("beSession.needs.unknown"));
    expect([r.asksBtn.getAttribute("aria-label"), r.strip.getAttribute("aria-label")], "读屏念各列核心写好的那一句").toEqual(["核心·头 1", "核心·头 1"]);
    const order = [...r.el.querySelectorAll<HTMLElement>(".tab-needs")].map((e) => e.dataset.col);
    expect(order).toEqual(["answerable", "manual"]);

    const unsure = rig([tab("a", waiting)], {}, cols(col([a]), col([], { sure: false, head: "核心·需手动" })));
    await unsure.book.again();
    expect([countOf(unsure.asksBtn), unsure.asksBtn.classList.contains("is-unknown")]).toEqual(["1", false]);
    expect([unsure.strip.style.display, countOf(unsure.strip), unsure.strip.classList.contains("is-unknown")], "说不准：照画、不带数").toEqual(["", "", true]);

    const none = rig([tab("a")], {}, cols(col([]), col([])));
    await none.book.again();
    expect([none.asksBtn.style.display, none.strip.style.display]).toEqual(["none", "none"]);
  });

  it("★ 待答请求浮层里只有会话行（没有计划项 · 会外项）；需手动浮层里没有答得了的会话，答不了的会话行第二行接那台那一句（manualText）", async () => {
    const a = atLocal(sessionItem("a", answerable));
    const m = atLocal(sessionItem("m", inTerminal));
    const r = rig([tab("a", waiting), tab("m", waiting)], {}, cols(col([a]), col([m, planAt("片"), atLocal(manualItem(acctItem("t1")))])));
    await r.book.again();
    r.asksBtn.click();
    const titleOf = (e: HTMLElement): string => e.children[1]!.children[0]!.textContent ?? "";
    expect(rows().map((e) => e.dataset.row)).toEqual(["session"]);
    expect(titleOf(rows()[0])).toBe("a");
    expect(panel()!.getAttribute("aria-label")).toBe(copyText("needs.asks.title"));
    expect(rows()[0].querySelector("[data-state]")?.getAttribute("data-state"), "待答请求那一行的点").toBe("asks");
    r.asksBtn.click();
    r.strip.click();
    expect(rows().map((e) => e.dataset.row)).toEqual(["session", "plan", "manual"]);
    expect(titleOf(rows()[0]), "答得了的 a 不在需手动浮层").toBe("m");
    expect(rows()[0].querySelector("[data-state]")?.getAttribute("data-state")).toBe("needs-you");
    const l2 = rows()[0].children[1]!.children[1]!.textContent ?? "";
    expect(l2, "第二行：那台那一句之后接 manualText").toBe(["核心·需手动", "核心·在终端里答"].join(copyText("kit.text.sep")));
  });

  it("★ 悬停那颗（窄栏只剩点和数时靠它）：核心写好的那一列头那一句 ＋ 那一列「下一个」此刻的键；没绑键 ⇒ 只那一句", async () => {
    vi.useFakeTimers();
    try {
      const a = atLocal(sessionItem("a", answerable));
      const r = rig([tab("a", waiting)], {}, cols(col([a, a], { head: "核心·待答请求 2" }), col([atLocal(sessionItem("m", inTerminal))], { head: "核心·需手动 1" })));
      await r.book.again();
      const tips = (): string[] => [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].map((t) => t.textContent ?? "");
      r.asksBtn.querySelector(".tab-needs-text")!.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
      await vi.advanceTimersByTimeAsync(1_000);
      expect(tips()).toEqual([["核心·待答请求 2", KeybindingDispatcher.prettyChord("Ctrl+KeyJ")].join(copyText("kit.text.sep"))]);
      r.asksBtn.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: r.strip }));
      r.strip.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: r.asksBtn }));
      await vi.advanceTimersByTimeAsync(1_000);
      expect(tips(), "manual.next 默认不绑").toEqual(["核心·需手动 1"]);
      for (const t of document.querySelectorAll('[role="tooltip"]')) t.remove();
    } finally {
      vi.useRealTimers();
    }
  });

  it("★ 待答请求浮层开着、最后一条被答掉 ⇒ 空态「无待答请求」", async () => {
    const a = atLocal(sessionItem("a", answerable));
    let reply = cols(col([a]), col([]));
    const r = rig([tab("a", waiting)], {}, () => reply());
    await r.book.again();
    r.asksBtn.click();
    reply = cols(col([]), col([]));
    await r.book.again();
    expect(panel()?.textContent).toBe(copyText("needs.asks.empty"));
  });

  it("★ Ctrl+J（asks.next）只在待答请求那几条会话里绕；manual.next 先走答不了的会话、再走计划项（会外不进）", async () => {
    const r = rig([tab("a1", waiting), tab("m1", waiting), tab("a2", waiting)], {}, cols(
      col([atLocal(sessionItem("a1", answerable)), atLocal(sessionItem("a2", answerable))]),
      col([atLocal(sessionItem("m1", inTerminal)), planAt("片"), atLocal(manualItem(acctItem("t1")))]),
    ));
    const opened: number[] = [];
    let planAtNow: number | null = null;
    r.view.plan = { at: () => planAtNow, open: (i) => { opened.push(i); planAtNow = i; } };
    r.host.answerAt.mockImplementation((sid: string) => { r.store.activeId = sid; planAtNow = null; });
    r.store.activeId = "m1";
    const seen: string[] = [];
    for (let i = 0; i < 3; i++) {
      await r.view.nextNeeds("answerable");
      seen.push(r.store.activeId ?? "");
    }
    expect(seen, "只绕 a1 · a2").toEqual(["a1", "a2", "a1"]);
    const steps: string[] = [];
    for (let i = 0; i < 3; i++) {
      await r.view.nextNeeds("manual");
      steps.push(planAtNow === null ? (r.store.activeId ?? "") : `plan${planAtNow}`);
    }
    expect(steps, "答不了的会话在前、计划项在后，绕回").toEqual(["m1", "plan0", "m1"]);
  });
});

describe("入口的数 ＝ 两份清单相加", () => {
  it("★ n ＝ 合好的总数（会话 ＋ 会话外，各台加起来）；只有会话外的也出入口；读回来之前照上一份", async () => {
    const r = rig([tab("a", waiting), tab("b")], { [LOCAL]: machine(["a"], [acctItem("t1"), mcpItem("s1")]), "gpu-01": machine([], [acctItem("t2")]) });
    await r.book.again();
    expect(r.strip.querySelector(".tab-needs-count")?.textContent).toBe("4");
    const only = rig([tab("a")], { [LOCAL]: machine([], [acctItem("t1")]) });
    expect(only.strip.style.display, "还没读回来：没有就不出").toBe("none");
    await only.book.again();
    expect(only.strip.style.display).toBe("");
    expect(only.strip.querySelector(".tab-needs-count")?.textContent).toBe("1");
  });
});

describe("浮层", () => {
  it("★ agent 发的那一条：第二行摆核心挑好的批什么，另起一行照摆谁要批；主会话发的没有那一行", async () => {
    const fromAgent = {
      ...waiting.needs,
      what: "核心·sed -n 1,40p",
      ask: { id: "ask-1", kind: "approve", tool: "Bash", run: { id: "a1", text: "核心·agent「Audit」 · Explore" }, what: "核心·sed -n 1,40p", detail: "", choices: [CHOICE], questions: [] },
      manualText: null,
      askHead: "核心·需手动",
    } as Needs;
    const r = rig([tab("a", { ...waiting, needs: fromAgent }), tab("b", waiting)], {}, cols(col([atLocal(sessionItem("a", fromAgent)), atLocal(sessionItem("b", answerable))]), col([])));
    await r.book.again();
    r.asksBtn.click();
    const [a, b] = rows();
    expect(a.querySelector("[data-part=by]")?.textContent).toBe("核心·agent「Audit」 · Explore");
    expect(a.textContent).toContain("核心·sed -n 1,40p");
    expect(b.querySelector("[data-part=by]")).toBeNull();
  });

  it("★ 点入口开浮层（不跳）：会话行 · 分隔线 · 会话外每一项（said ＋ 机器名 ＋［登录…］）；开的那一下各台重问一次", async () => {
    const r = rig([tab("a", waiting)], { [LOCAL]: machine(["a"], [acctItem("t1")]), "gpu-01": machine([], [mcpItem("s1")]) });
    await r.book.again();
    r.asked.length = 0;
    r.strip.click();
    expect(r.host.answerAt, "点入口只开浮层").not.toHaveBeenCalled();
    expect(rows().map((e) => e.dataset.row)).toEqual(["session", "manual", "manual"]);
    expect(panel()!.children[1].tagName, "会话与会话外之间一条分隔线").toBe("SPAN");
    const [, m1, m2] = rows();
    expect(m1.textContent).toContain("核心·t1 需登录");
    expect(m1.textContent).toContain(copyText("control.machine.unsure"));
    expect(m2.textContent).toContain("gpu-01");
    expect(m1.querySelector('[data-action="manual-login"]')?.textContent).toBe(copyText("newSession.account.login"));
    await settle();
    expect(r.asked.sort(), "开浮层 ⇒ 问过的每台重问").toEqual([LOCAL, "gpu-01"].sort());
  });

  it("★ 点会话行 ⇒ 切过去、浮层收起；点会话外那一行 · 它的［登录…］⇒ 开登录抽屉（交那一项与它那台）", async () => {
    const it1 = acctItem("t1");
    const it2 = mcpItem("s1");
    const r = rig([tab("a", waiting)], { [LOCAL]: machine(["a"], [it1]), "gpu-01": machine([], [it2]) });
    await r.book.again();
    r.strip.click();
    rows()[0].click();
    expect(r.host.answerAt).toHaveBeenCalledWith("a");
    expect(panel(), "点完收起").toBeNull();
    r.strip.click();
    rows()[1].click();
    expect(r.login).toHaveBeenLastCalledWith(LOCAL, it1);
    expect(panel()).toBeNull();
    r.strip.click();
    rows()[2].querySelector<HTMLElement>('[data-action="manual-login"]')!.click();
    expect(r.login).toHaveBeenLastCalledWith("gpu-01", it2);
    expect(r.login).toHaveBeenCalledTimes(2);
  });

  it("★ fix 为空：第二行写后端给的 why，不画按钮，那一行点不了", async () => {
    const r = rig([], { [LOCAL]: machine([], [mcpItem("s1", false), acctItem("t1")]) });
    await r.book.again();
    r.strip.click();
    const inert = panel()!.querySelector<HTMLElement>('[data-inert="true"]')!;
    expect(inert.textContent).toContain("核心·不支持在 app 里登录");
    expect(inert.querySelector("button")).toBeNull();
    inert.click();
    expect(r.login).not.toHaveBeenCalled();
    expect(rows().filter((e) => e.dataset.inert !== "true").length, "能点的只剩那一行").toBe(1);
  });

  it("★ 权限太宽的秘密文件：第一行 said、第二行机器名 ＋ 怎么修（why），警示图标，不画按钮、点不了；计进入口的数", async () => {
    const file: ManualItem = { kind: "secretFile", said: "核心·钥匙文件 /h/k：太宽", fix: null, why: "核心·只给本人" };
    const r = rig([], { [LOCAL]: machine([], [file]) });
    await r.book.again();
    expect(r.strip.querySelector(".tab-needs-count")?.textContent).toBe("1");
    r.strip.click();
    const inert = panel()!.querySelector<HTMLElement>('[data-inert="true"]')!;
    expect(inert.dataset.row).toBe("manual");
    expect(inert.textContent).toContain("核心·钥匙文件 /h/k：太宽");
    expect(inert.textContent).toContain(copyText("control.machine.unsure"));
    expect(inert.textContent).toContain("核心·只给本人");
    expect(inert.querySelector("button")).toBeNull();
    expect(inert.querySelector("svg")?.outerHTML).toBe(icon("warning", "compact").outerHTML);
    inert.click();
    expect(r.login).not.toHaveBeenCalled();
  });

  it("★ 一台没问到 ⇒ 底部一条照后端写的那一句、入口说不准（不带数）；回了话、清单无法读取 ⇒ 入口不带数字、虚线框，底部那一句、原话在悬停里", async () => {
    const r = rig([tab("a", waiting)], { [LOCAL]: machine(["a"], [acctItem("t1")]), devbox: { t: "offline" } });
    await r.book.again();
    expect([r.strip.querySelector(".tab-needs-count")?.textContent, r.strip.classList.contains("is-unknown")], "有台没问到 ⇒ 核心说不准").toEqual(["", true]);
    r.strip.click();
    const said = r.book.offline().find((d) => d.origin === "devbox")?.said;
    expect(said, "回包里那台带着后端写的那一句").toBeTruthy();
    expect(panel()!.querySelector('[data-note="offline"]')?.textContent, "后端那一句原样摆").toBe(said);
    r.strip.click();
    r.answers["gpu-01"] = machine([], [], "核心·账号库无法解析");
    await r.book.again();
    expect(r.strip.classList.contains("is-unknown")).toBe(true);
    expect(r.strip.querySelector(".tab-needs-count")?.textContent, "数说不准 ⇒ 不带数字").toBe("");
    r.strip.click();
    const note = panel()!.querySelector<HTMLElement>('[data-note="unreadable"]')!;
    expect(note.textContent).toBe(copyText("needs.panel.unreadable", { machine: "gpu-01" }));
    expect(note.title).toContain("核心·账号库无法解析");
    expect(rows().map((e) => e.dataset.row), "读到的照常列").toEqual(["session", "manual"]);
  });

  it("★ 那台答了、答的是失败 ⇒ 底部那一条是那一句，不说成离线", async () => {
    const r = rig([], {}, () => ({ asks: { items: [], total: 0, head: "核心·待答请求", sure: false }, manual: { items: [], total: 0, head: "核心·头", sure: false }, windowTitle: "核心·标题", unreadable: [], down: [{ origin: "devbox" as Machine, said: "核心·devbox 答了失败", detail: "" }], ended: [] }));
    await r.book.again();
    r.strip.click();
    const note = panel()!.querySelector<HTMLElement>('[data-note="offline"]')!;
    expect(note.textContent).toBe("核心·devbox 答了失败");
    expect(note.textContent).not.toContain(copyText("machinePage.state.down"));
  });

  it("★ 最后一项刚解决、浮层开着 ⇒「无需手动事项」、入口去掉数字、不自己关；关了入口才消失", async () => {
    const r = rig([], { [LOCAL]: machine([], [acctItem("t1")]) });
    await r.book.again();
    r.strip.click();
    r.answers[LOCAL] = machine([], []);
    await r.book.again();
    expect(panel()?.textContent).toBe(copyText("needs.panel.empty"));
    expect(r.strip.style.display).toBe("");
    expect(r.strip.querySelector(".tab-needs-count")?.textContent).toBe("");
    r.strip.click();
    expect(panel()).toBeNull();
    expect(r.strip.style.display).toBe("none");
  });

  it("★ 一台连不上（核心两列都说不准）、读到的都是 0 ⇒ 两颗浮层都不说「无…」，说读到的各台没有，底下照摆那台那一句", async () => {
    const down = { down: [{ origin: "devbox" as Machine, said: "核心·devbox 连接失败", detail: "" }] };
    const r = rig([], {}, cols(col([], { sure: false }), col([], { sure: false }), down));
    await r.book.again();
    r.asksBtn.click();
    expect(panel()?.textContent).toBe(copyText("needs.asks.emptyUnsure") + "核心·devbox 连接失败");
    r.asksBtn.click();
    r.strip.click();
    expect(panel()?.textContent).toBe(copyText("needs.panel.emptyUnsure") + "核心·devbox 连接失败");
  });

  it("↑↓ 选行 · Enter ＝ 点（Esc 是浮层那一套的弹层栈，kit 判据管）", async () => {
    const it1 = acctItem("t1");
    const r = rig([tab("a", waiting)], { [LOCAL]: machine(["a"], [it1]) });
    await r.book.again();
    r.strip.click();
    const p = panel()!;
    expect(document.activeElement).toBe(p);
    expect(rows()[0].getAttribute("aria-selected")).toBe("true");
    p.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(rows()[1].getAttribute("aria-selected")).toBe("true");
    p.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(r.login).toHaveBeenCalledWith(LOCAL, it1);
  });
});

describe("空了那一句照核心那一列准不准（浮层不另判）", () => {
  const acts = { answer: () => {}, plan: () => {}, login: () => {}, toAccount: () => {} };
  const empty = (col: "answerable" | "manual", sure: boolean) =>
    renderNeedsPanel({ col, sure, sessions: [], plans: [], manual: [], offline: [], unreadable: [] }, acts, 0).root.textContent;
  it("说得准 ⇒「无…」；说不准 ⇒ 读到的各台没有（就算底下一条没问到的都没有，比如那台还在读）", () => {
    expect([empty("answerable", true), empty("manual", true)]).toEqual([copyText("needs.asks.empty"), copyText("needs.panel.empty")]);
    expect([empty("answerable", false), empty("manual", false)]).toEqual([copyText("needs.asks.emptyUnsure"), copyText("needs.panel.emptyUnsure")]);
  });
});

describe("远端那台照机器名称呼（会话行的徽标 · 计划项第二行末尾）", () => {
  it("写那台的名字（机器成品里的 `name`），不写它的键", () => {
    noteMachine("box-n" as Machine, { name: "〈box-n 的名字〉", offlineSaid: "" });
    const sessions = [{ sid: "s", origin: "box-n" as Machine, title: "题", state: "等", what: null, by: null, manualText: null }];
    const plans = [{ origin: "box-n" as Machine, workspace: "/w/p", slice: "片", cell: null, i: 0 }];
    const { root } = renderNeedsPanel({ col: "manual", sure: true, sessions, plans, manual: [], offline: [], unreadable: [] }, { answer: () => {}, plan: () => {}, login: () => {}, toAccount: () => {} }, 0);
    const session = root.querySelector<HTMLElement>("[data-row=session]");
    const plan = root.querySelector<HTMLElement>("[data-row=plan]");
    expect([session, plan].map((r) => r !== null), "两行都画出来").toEqual([true, true]);
    expect(session!.textContent).toContain("〈box-n 的名字〉");
    const text = plan!.textContent ?? "";
    expect(text.endsWith(`${copyText("kit.text.sep")}〈box-n 的名字〉`), text).toBe(true);
  });
});

describe("写了认不出的家的号：那一行照后端那一句，按钮去设置里那个号", () => {
  it("［去设置］交出那台 ＋ 号名 ＋ 号表原样的家；不出登录按钮", () => {
    const item: ManualItem = { kind: "accountNoAgent", agent: "zeta", account: "z1", said: "〈认不出 z1〉", go: "〈去设置〉", fix: null };
    const went: unknown[] = [];
    const { root } = renderNeedsPanel(
      { col: "manual", sure: true, sessions: [], plans: [], manual: [{ origin: LOCAL, item }], offline: [], unreadable: [] },
      { answer: () => {}, plan: () => {}, login: () => went.push("login"), toAccount: (o, a) => went.push([o, a]) },
      0,
    );
    const row = root.querySelector<HTMLElement>("[data-row=manual]")!;
    expect(row.textContent).toContain("〈认不出 z1〉");
    expect(row.querySelector("[data-action=manual-login]")).toBeNull();
    const go = row.querySelector<HTMLButtonElement>("[data-action=manual-settings]")!;
    expect(go.textContent).toBe("〈去设置〉");
    go.click();
    expect(went).toEqual([[LOCAL, { name: "z1", unknownAgent: "zeta" }]]);
  });
});
