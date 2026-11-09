/**
 * 会话的整行标题只住后端一处（`search_rules::session_line`，帧上 `NameFace.title`）：界面照抄、零拼接。
 *
 * - 照抄：标签页那一行 · 悬停卡第一行 · 会话头标题写的都是 `Tab.title` 原样（项目 · `↳` · `⚙` · 名字都在那一行里，界面不另摆项目格、不另加记号）。
 * - 零拼接：界面不再有拼标题的那几个函数（`computeTitleFor` · `titleParts` · `fullTitle`）、不再记 `labelProject`；
 *   画标题的那几份里不写 `↳` · `⚙` 这两个记号，也不拿会话号头 8 位顶名字。
 *
 * 夹具只造结构（sid · 写好的一行标题），不采会话正文。
 */
import { describe, expect, it, vi } from "vitest";

import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { TabStore } from "../../../src/frontend/ui/tab-store";
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import { SessionHead } from "../../../src/frontend/ui/session-head";
import { goldenFace } from "../../test-support/session-faces";
import { productionTsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";

vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));

const NOW = Date.parse("2026-11-02T12:00:00Z");
/** 后端写好的那一行（项目 · 分叉 · 后台 · 名字）。 */
const LINE = "billing ↳ ⚙ 账单导出改成流式";

function tab(over: Partial<Tab> = {}): Tab {
  return {
    sessionId: "0123456789abcdef",
    title: LINE,
    label: "账单导出改成流式",
    background: true,
    projectDir: "/w/billing",
    face: goldenFace("live-idle"),
    pinned: false,
    group: null,
    origin: LOCAL,
    forkedLine: "分叉自「父」",
    whereLine: null,
    unread: 0,
    needs: null,
    autoAnswer: null,
    autoAnswerChoices: [],
    askEnd: null,
    backgroundWork: null,
    pending: [],
    lastSay: null,
    ...over,
  } as unknown as Tab;
}

function tabBar(t: Tab): TabBarView {
  const store = new TabStore();
  store.tabs.set(t.sessionId, t);
  store.orderedIds.push(t.sessionId);
  const el = document.createElement("div");
  document.body.appendChild(el);
  const host = new Proxy({}, { get: () => vi.fn() }) as unknown as TabBarViewHost;
  const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, host);
  view.refresh();
  return view;
}

describe("会话标题：后端写好的一行，界面照抄", () => {
  it("标签页那一行 · 悬停卡第一行 · 会话头标题都是 Tab.title 原样", () => {
    const t = tab();
    const view = tabBar(t);
    expect(document.querySelector(".tab-title")?.textContent).toBe(LINE);
    expect(view.hoverCard(t.sessionId)?.firstElementChild?.textContent).toBe(LINE);

    const host = { active: () => t, openCwd: vi.fn(), front: vi.fn(), view: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), planMark: () => null, menuItem: () => null, displayItems: () => [], tabOf: () => null, switchTo: vi.fn(), switching: { active: () => null, facts: () => new Promise<never>(() => {}), send: vi.fn(), unavailable: () => null, reread: vi.fn(), toast: vi.fn() } };
    const h = new SessionHead(host);
    document.body.appendChild(h.el);
    h.render(NOW);
    expect(h.el.querySelector("[data-head-title]")?.textContent).toBe(LINE);
  });

  it("界面零拼接：拼标题的函数 · labelProject 零处；画标题的几份不写记号、不拿会话号顶名字", () => {
    const hits: string[] = [];
    const drawers = new Set(["src/frontend/ui/session-face.ts", "src/frontend/ui/tab-model.ts", "src/frontend/ui/tab-bar-view.ts", "src/frontend/ui/session-head.ts"]);
    for (const { file, text } of productionTsFiles("src/frontend/ui")) {
      const code = stripComments(text, "ts");
      for (const name of ["computeTitleFor", "titleParts", "fullTitle", "labelProject", "computeTitle("]) {
        if (code.includes(name)) hits.push(`${file}: ${name}`);
      }
      if (!drawers.has(file)) continue;
      for (const needle of [copyText("sessionTitle.mark.forked"), copyText("sessionTitle.mark.background"), "sessionId.slice(0, 8)"]) {
        if (code.includes(needle)) hits.push(`${file}: ${needle}`);
      }
    }
    expect(hits).toEqual([]);
  });
});
