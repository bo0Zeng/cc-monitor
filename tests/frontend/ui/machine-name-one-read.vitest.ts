/**
 * 界面里「这台叫什么」只有一个读口：`machineName(origin)`，照那台机器成品的 `name`（本机远端同一条路）。
 *
 * 喂两台成品（远端 devbox 叫「工作站 B」、本机叫「laptop」）之后，会话头 · 终端页头 · 终端输入「发给 …」
 * 三处写的都是成品里的名字，不写键（devbox）、不按本机 / 远端另挑字。标签悬停卡那一行摆的是会话事实里那台写好的
 * 「在哪」那一句（`whereLine`，判据在 `tabs.vitest.ts`），不读这个口。设置窗去处那一个改叫 `machinePageOf`，
 * `settings-dest` 不再有 `machineOf` 这个名字。
 *
 * 夹具只造结构（sid · 路径），不采会话正文。
 */
import { describe, it, expect, vi, beforeAll } from "vitest";
import { noteMachine } from "../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
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
    forkedLine: null,
    writers: [],
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

/** `key`：成品没到时会上屏的那个键（远端）；本机的键是哨兵、不上屏，只判写成品名字。sid 不含名字（免得名字从标题 · 目录混进来）。 */
const CASES = [
  { sid: "s1", origin: "devbox", name: "工作站 B", key: "devbox" },
  { sid: "s2", origin: LOCAL, name: "laptop", key: null },
] as const;

beforeAll(() => {
  for (const c of CASES) noteMachine(c.origin, { name: c.name, offlineSaid: "" });
});

function headText(t: Tab): string {
  const host = { active: () => t, openCwd: vi.fn(), terminal: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), reconnect: vi.fn(), planMark: () => null, menuItem: () => null, displayItems: () => [], tabOf: () => null, switchTo: vi.fn(), switching: NO_SWITCHING };
  const h = new SessionHead(host);
  document.body.appendChild(h.el);
  h.render(NOW);
  return h.el.textContent ?? "";
}

async function terminalText(t: Tab): Promise<string> {
  const reads: TerminalReads = {
    follow: vi.fn((_o, _terminal, events) => {
      events.stop({ kind: "snapshotOnly", said: "只能快照" });
      return { stop: () => {} };
    }),
    list: vi.fn(async () => [{ terminal: `tmux-${t.sessionId}`, name: `${t.sessionId}-cc`, sid: t.sessionId, clients: 0, input: "shared", programExited: false, inputNo: null }]),
    shot: vi.fn(async () => ({ lines: [{ text: "屏", spans: [] }], text: "屏", screen: "fp", atText: "22:13:20" })),
    send: vi.fn(async () => ({ result: "delivered", note: "S-delivered", acts: [] })),
  } as unknown as TerminalReads;
  const page = new TerminalPage({ active: () => ({ kind: "tab" as const, tab: t }), focusStream: vi.fn() }, reads);
  document.body.appendChild(page.el);
  page.sessionChanged();
  page.setVisible(true);
  for (let i = 0; i < 8; i++) await Promise.resolve();
  return page.el.textContent ?? "";
}

describe("机器叫法只读那台成品里的名字", () => {
  for (const c of CASES) {
    it(`★ ${c.origin}：会话头 · 终端页头 · 「发给 …」写成品名字「${c.name}」`, async () => {
      const t = tab(c.sid, c.origin);
      const texts = { head: headText(t), terminal: await terminalText(t) };
      for (const [where, text] of Object.entries(texts)) {
        expect(text, `${where} 写成品名字`).toContain(c.name);
        if (c.key !== null) expect(text, `${where} 不写键`).not.toContain(c.key);
      }
      expect(texts.terminal.split(c.name).length - 1, "终端页头与「发给 …」两处都写").toBeGreaterThanOrEqual(2);
    });
  }

  it("★ 设置窗去处那一个叫 machinePageOf：settings-dest 不导出 machineOf", () => {
    expect(Object.keys(settingsDest)).not.toContain("machineOf");
    expect(settingsDest.machinePageOf("devbox")).toEqual({ machine: "devbox" });
  });
});
