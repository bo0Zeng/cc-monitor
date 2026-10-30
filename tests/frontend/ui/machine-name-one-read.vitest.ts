/**
 * 界面里「这台叫什么」只有一个读口：`machineName(origin)`，照那台机器成品的 `name`（本机远端同一条路）。
 *
 * 喂两台成品（远端 devbox 叫「工作站 B」、本机叫「box-b」）之后，会话头 · 标签悬停卡 · 终端页头 · 终端输入「发给 …」
 * 四处写的都是成品里的名字，不写键（devbox）、不按本机 / 远端另挑字。设置窗去处那一个改叫 `machinePageOf`，
 * `settings-dest` 不再有 `machineOf` 这个名字。
 *
 * 夹具只造结构（sid · 路径），不采会话正文。
 */
import { describe, it, expect, vi, beforeAll } from "vitest";
import { noteMachine } from "../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { TabStore } from "../../../src/frontend/ui/tab-store";
import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import { SessionHead } from "../../../src/frontend/ui/session-head";
import { TerminalPage, type TerminalReads } from "../../../src/frontend/ui/terminal-page";
import { liveFace } from "../../test-support/session-faces";
import * as settingsDest from "../../../src/frontend/ui/settings-dest";

vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));

const NOW = Date.parse("2026-10-05T12:00:00Z");
const NO_SWITCHING = { active: () => null, facts: () => new Promise<never>(() => {}), send: vi.fn(), unavailable: () => null, reread: vi.fn(), toast: vi.fn() };

function tab(sid: string, origin: string, over: Partial<Tab> = {}): Tab {
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
    origin,
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

const CASES = [
  { origin: "devbox", name: "工作站 B", key: "devbox" },
  { origin: LOCAL_ORIGIN, name: "box-b", key: "本机" },
] as const;

beforeAll(() => {
  for (const c of CASES) noteMachine(c.origin, { name: c.name, offlineSaid: "" });
});

function headText(t: Tab): string {
  const host = { active: () => t, viewTerminal: vi.fn(), openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn(), planMark: () => null, menuItem: () => null, displayItems: () => [], tabOf: () => null, switchTo: vi.fn(), switching: NO_SWITCHING };
  const h = new SessionHead(host);
  document.body.appendChild(h.el);
  h.render(NOW);
  return h.el.textContent ?? "";
}

function hoverText(t: Tab): string {
  const store = new TabStore();
  store.tabs.set(t.sessionId, t);
  store.orderedIds.push(t.sessionId);
  const el = document.createElement("div");
  document.body.appendChild(el);
  const host = new Proxy({}, { get: () => vi.fn() }) as unknown as TabBarViewHost;
  const view = new TabBarView(store, { collections: [] } as unknown as TabBarPrefs, el, host);
  return view.hoverCard(t.sessionId)?.textContent ?? "";
}

async function terminalText(t: Tab): Promise<string> {
  const reads: TerminalReads = {
    follow: vi.fn((_o, _terminal, events) => {
      events.stop({ kind: "snapshotOnly", said: "只能快照" });
      return { stop: () => {} };
    }),
    list: vi.fn(async () => [{ terminal: `tmux-${t.sessionId}`, name: `${t.sessionId}-cc`, sid: t.sessionId, clients: 0, input: "shared", programExited: false, inputNo: null }]),
    shot: vi.fn(async () => ({ lines: [{ text: "屏", spans: [] }], text: "屏", screen: "fp", atText: "22:13:20" })),
    send: vi.fn(async () => ({ result: "delivered", acts: [] })),
  } as unknown as TerminalReads;
  const page = new TerminalPage({ active: () => ({ kind: "tab" as const, tab: t }), front: vi.fn(), focusStream: vi.fn() }, reads);
  document.body.appendChild(page.el);
  page.sessionChanged();
  page.setVisible(true);
  for (let i = 0; i < 8; i++) await Promise.resolve();
  return page.el.textContent ?? "";
}

describe("机器叫法只读那台成品里的名字", () => {
  for (const c of CASES) {
    it(`★ ${c.origin}：会话头 · 悬停卡 · 终端页头 · 「发给 …」写成品名字「${c.name}」，不写「${c.key}」`, async () => {
      const t = tab(`s-${c.name}`, c.origin);
      const texts = { head: headText(t), hover: hoverText(t), terminal: await terminalText(t) };
      for (const [where, text] of Object.entries(texts)) {
        expect(text, `${where} 写成品名字`).toContain(c.name);
        expect(text, `${where} 不写键或按本机 / 远端挑的字`).not.toContain(c.key);
      }
    });
  }

  it("★ 设置窗去处那一个叫 machinePageOf：settings-dest 不导出 machineOf", () => {
    expect(Object.keys(settingsDest)).not.toContain("machineOf");
    expect(settingsDest.machinePageOf("devbox")).toEqual({ machine: "devbox" });
  });
});
