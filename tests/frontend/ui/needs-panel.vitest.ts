/**
 * 侧栏顶上「需手动 n」点开的浮层：读的是各台 `sessions-needs` 整份、本机合好的那一份（`needs-book.ts`）。
 *
 * - n ＝ 合好的总数（在等的会话 ＋ 计划项 ＋ 会话外的每一项）；窗口标题同一个数（`needs-bar` 那一份见 `main-window-needs.vitest.ts`）。
 * - 浮层：会话行（照「需手动」的序）· 分隔线 · 会话外每一项（第一行 `said`、第二行机器名；`fix` 为空 ⇒ 第二行写 `why`、不画按钮）；
 *   断开那台一条「{machine} 离线 · 未计入」；那台会外那一截读不出 ⇒ 入口不带数字、虚线框，底部一条「{machine} · 需手动清单无法读取」（那台那一句在悬停里）。
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
import { LOCAL_ORIGIN, type Origin } from "../../../src/frontend/ui/ipc/origin";
import { liveFace } from "../../test-support/session-faces";
import type { ManualItem } from "../../../src/frontend/ui/login-reads";
import { fakeNeedsBook, listOf, manualItem, sessionItem, type NeedsRead } from "../../test-support/needs-fake";
import { copyText } from "../../../src/frontend/ui/copy-table";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 6; i++) await tick();
};

/** 在等人：脸 ＋ 同一刻的会话事实（种类那一份在事实里）。 */
const waiting = {
  face: liveFace("needs_you"),
  line: { for: "needs_you" as const, text: "", clock: null, peek: null },
  needs: { kind: "approve", tool: null, call: null, what: null, sinceMs: null, text: "核心·需手动", head: "核心·需手动", headCode: null, tone: "need", rank: 1, waitedMs: null, waitedText: null, ask: null, manualText: null, receivedAt: 0 } as Needs,
};

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid, title: sid, background: false, bgName: null, label: null, labelProject: null, projectDir: `/w/${sid}`, face: liveFace("working"), line: null, pinned: false, group: null,
    origin: LOCAL_ORIGIN, forkedFromSessionId: null, writers: [], unread: 0, needs: null, backgroundWork: null, pending: [], lastSay: null, ...over,
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
function rig(tabs: Tab[], answers: Record<string, NeedsRead>) {
  const store = new TabStore();
  for (const t of tabs) {
    store.tabs.set(t.sessionId, t);
    store.orderedIds.push(t.sessionId);
  }
  store.activeId = tabs[0]?.sessionId ?? null;
  const host = {
    refreshTabBar: vi.fn(), closeEndedIn: vi.fn(), openTabCwd: vi.fn().mockResolvedValue(undefined), bringTerminalToFront: vi.fn().mockResolvedValue(undefined),
    bringRemoteTerminalToFront: vi.fn().mockResolvedValue(undefined), closeTab: vi.fn(), switchTo: vi.fn(), answerAt: vi.fn(), pick: vi.fn(),
    isSelected: vi.fn().mockReturnValue(false), beginDrag: vi.fn(), takeSuppressedClick: vi.fn().mockReturnValue(false), beginGroupDrag: vi.fn(),
    takeSuppressedHeadClick: vi.fn().mockReturnValue(false), openMenu: vi.fn(), toggleSelect: vi.fn(), openMenuAt: vi.fn(), rereadAll: vi.fn().mockResolvedValue(undefined), reconnect: vi.fn(),
  } satisfies TabBarViewHost;
  const el = document.createElement("div");
  document.body.appendChild(el);
  const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, host);
  const { book, asked } = fakeNeedsBook(answers);
  const login = vi.fn();
  view.needs = { book, login };
  book.subscribe(() => view.planNeedsChanged());
  view.refresh();
  const strip = el.querySelector<HTMLElement>(".tab-needs")!;
  return { view, host, store, el, book, login, asked, strip, answers };
}

const panel = () => document.querySelector<HTMLElement>('[role="dialog"] [role="listbox"]');
const rows = () => [...(panel()?.querySelectorAll<HTMLElement>("[data-row]") ?? [])];

beforeEach(() => {
  document.body.innerHTML = "";
  invokeMock.mockReset();
});

describe("入口的数 ＝ 两份清单相加", () => {
  it("★ n ＝ 合好的总数（会话 ＋ 会话外，各台加起来）；只有会话外的也出入口；读回来之前照上一份", async () => {
    const r = rig([tab("a", waiting), tab("b")], { [LOCAL_ORIGIN]: machine(["a"], [acctItem("t1"), mcpItem("s1")]), "gpu-01": machine([], [acctItem("t2")]) });
    await r.book.refreshAll([LOCAL_ORIGIN, "gpu-01" as Origin]);
    expect(r.strip.querySelector(".tab-needs-count")?.textContent).toBe("4");
    expect(r.view.needsCountNow()).toBe(4);
    const only = rig([tab("a")], { [LOCAL_ORIGIN]: machine([], [acctItem("t1")]) });
    expect(only.strip.style.display, "还没读回来：没有就不出").toBe("none");
    await only.book.refresh(LOCAL_ORIGIN);
    expect(only.strip.style.display).toBe("");
    expect(only.strip.querySelector(".tab-needs-count")?.textContent).toBe("1");
  });
});

describe("浮层", () => {
  it("★ 点入口开浮层（不跳）：会话行 · 分隔线 · 会话外每一项（said ＋ 机器名 ＋［登录…］）；开的那一下各台重问一次", async () => {
    const r = rig([tab("a", waiting)], { [LOCAL_ORIGIN]: machine(["a"], [acctItem("t1")]), "gpu-01": machine([], [mcpItem("s1")]) });
    await r.book.refreshAll([LOCAL_ORIGIN, "gpu-01" as Origin]);
    r.asked.length = 0;
    r.strip.click();
    expect(r.host.answerAt, "点入口只开浮层").not.toHaveBeenCalled();
    expect(rows().map((e) => e.dataset.row)).toEqual(["session", "manual", "manual"]);
    expect(panel()!.children[1].tagName, "会话与会话外之间一条分隔线").toBe("SPAN");
    const [, m1, m2] = rows();
    expect(m1.textContent).toContain("核心·t1 需登录");
    expect(m1.textContent).toContain(copyText("control.machine.local"));
    expect(m2.textContent).toContain("gpu-01");
    expect(m1.querySelector('[data-action="manual-login"]')?.textContent).toBe(copyText("newSession.account.login"));
    await settle();
    expect(r.asked.sort(), "开浮层 ⇒ 问过的每台重问").toEqual([LOCAL_ORIGIN, "gpu-01"].sort());
  });

  it("★ 点会话行 ⇒ 切过去、浮层收起；点会话外那一行 · 它的［登录…］⇒ 开登录抽屉（交那一项与它那台）", async () => {
    const it1 = acctItem("t1");
    const it2 = mcpItem("s1");
    const r = rig([tab("a", waiting)], { [LOCAL_ORIGIN]: machine(["a"], [it1]), "gpu-01": machine([], [it2]) });
    await r.book.refreshAll([LOCAL_ORIGIN, "gpu-01" as Origin]);
    r.strip.click();
    rows()[0].click();
    expect(r.host.answerAt).toHaveBeenCalledWith("a");
    expect(panel(), "点完收起").toBeNull();
    r.strip.click();
    rows()[1].click();
    expect(r.login).toHaveBeenLastCalledWith(LOCAL_ORIGIN, it1);
    expect(panel()).toBeNull();
    r.strip.click();
    rows()[2].querySelector<HTMLElement>('[data-action="manual-login"]')!.click();
    expect(r.login).toHaveBeenLastCalledWith("gpu-01", it2);
    expect(r.login).toHaveBeenCalledTimes(2);
  });

  it("★ fix 为空：第二行写后端给的 why，不画按钮，那一行点不了", async () => {
    const r = rig([], { [LOCAL_ORIGIN]: machine([], [mcpItem("s1", false), acctItem("t1")]) });
    await r.book.refresh(LOCAL_ORIGIN);
    r.strip.click();
    const inert = panel()!.querySelector<HTMLElement>('[data-inert="true"]')!;
    expect(inert.textContent).toContain("核心·不支持在 app 里登录");
    expect(inert.querySelector("button")).toBeNull();
    inert.click();
    expect(r.login).not.toHaveBeenCalled();
    expect(rows().filter((e) => e.dataset.inert !== "true").length, "能点的只剩那一行").toBe(1);
  });

  it("★ 一台断开 ⇒ 底部「{machine} 离线 · 未计入」，不计数；回了话、清单无法读取 ⇒ 入口不带数字、虚线框，底部那一句、原话在悬停里", async () => {
    const r = rig([tab("a", waiting)], { [LOCAL_ORIGIN]: machine(["a"], [acctItem("t1")]), devbox: { t: "offline" } });
    await r.book.refreshAll([LOCAL_ORIGIN, "devbox" as Origin]);
    expect(r.strip.querySelector(".tab-needs-count")?.textContent).toBe("2");
    r.strip.click();
    expect(panel()!.querySelector('[data-note="offline"]')?.textContent).toBe(copyText("needs.panel.offline", { machine: "devbox" }));
    expect(panel()!.querySelector<HTMLElement>('[data-note="offline"]')?.title, "后端那一句在悬停里").toBe("devbox 没问到");
    r.strip.click();
    r.answers["gpu-01"] = machine([], [], "核心·账号库无法解析");
    await r.book.refresh("gpu-01" as Origin);
    expect(r.strip.classList.contains("is-unknown")).toBe(true);
    expect(r.strip.querySelector(".tab-needs-count")?.textContent, "数说不准 ⇒ 不带数字").toBe("");
    r.strip.click();
    const note = panel()!.querySelector<HTMLElement>('[data-note="unreadable"]')!;
    expect(note.textContent).toBe(copyText("needs.panel.unreadable", { machine: "gpu-01" }));
    expect(note.title).toContain("核心·账号库无法解析");
    expect(rows().map((e) => e.dataset.row), "读到的照常列").toEqual(["session", "manual"]);
  });

  it("★ 最后一项刚解决、浮层开着 ⇒「无需手动事项」、入口去掉数字、不自己关；关了入口才消失", async () => {
    const r = rig([], { [LOCAL_ORIGIN]: machine([], [acctItem("t1")]) });
    await r.book.refresh(LOCAL_ORIGIN);
    r.strip.click();
    r.answers[LOCAL_ORIGIN] = machine([], []);
    await r.book.refresh(LOCAL_ORIGIN);
    expect(panel()?.textContent).toBe(copyText("needs.panel.empty"));
    expect(r.strip.style.display).toBe("");
    expect(r.strip.querySelector(".tab-needs-count")?.textContent).toBe("");
    r.strip.click();
    expect(panel()).toBeNull();
    expect(r.strip.style.display).toBe("none");
  });

  it("↑↓ 选行 · Enter ＝ 点（Esc 是浮层那一套的弹层栈，kit 判据管）", async () => {
    const it1 = acctItem("t1");
    const r = rig([tab("a", waiting)], { [LOCAL_ORIGIN]: machine(["a"], [it1]) });
    await r.book.refresh(LOCAL_ORIGIN);
    r.strip.click();
    const p = panel()!;
    expect(document.activeElement).toBe(p);
    expect(rows()[0].getAttribute("aria-selected")).toBe("true");
    p.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(rows()[1].getAttribute("aria-selected")).toBe("true");
    p.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(r.login).toHaveBeenCalledWith(LOCAL_ORIGIN, it1);
  });
});


