/**
 * 终端页看标签页的那个终端：名单里没有它时照脸上「看它的终端」那一格说（那台写好的那一句，界面不另拼）；
 * 画面尾部的空行不画（打开即贴着有内容的最后一行，不停在下方空白处）。读写换成假的。
 */
import { describe, expect, it, vi } from "vitest";

import { TerminalPage, type InputHolds, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { SessionFace } from "../../../src/frontend/ui/generated/SessionFace";
import { liveFace } from "../../test-support/session-faces";

function row(over: Partial<TerminalRow>): TerminalRow {
  return { terminal: "%1", name: "x", sid: null, clients: 0, input: "shared", programExited: false, inputNo: null, ...over };
}

const tabOf = (face: SessionFace): Watched => ({
  kind: "tab",
  tab: { sessionId: "s1", title: "proj", origin: "devbox", projectDir: "/p", cwdShown: "~/p", face } as unknown as Tab,
});

function rig(rows: TerminalRow[], watched: Watched, shot: TerminalShot): TerminalPage {
  const reads = {
    list: vi.fn(async () => rows),
    shot: vi.fn(async () => shot),
    send: vi.fn(),
    follow: vi.fn(() => ({ stop: () => {} })),
  };
  const holds = { take: vi.fn(), release: vi.fn() };
  const page = new TerminalPage({ active: () => watched, focusStream: () => {} }, reads as unknown as TerminalReads, holds as unknown as InputHolds);
  document.body.replaceChildren(page.el);
  return page;
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

const EMPTY_SHOT: TerminalShot = { lines: [], text: "", screen: "fp", atText: "12:00:00" };

describe("名单里没有这条活会话", () => {
  it("★ 脸上「看它的终端」做不了 ⇒ 照那一句说，不说「非 cc-monitor 起的」", async () => {
    const face = liveFace("idle");
    face.can.view = { can: "no", why: "那台写好的原因" };
    const page = rig([], tabOf(face), EMPTY_SHOT);
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    const text = page.el.textContent ?? "";
    expect(text).toContain("那台写好的原因");
    expect(text).not.toContain(copyText("terminal.empty.notOurs"));
  });
});

describe("画面停在有内容的那一屏", () => {
  it("★ 三行字 ＋ 二十行空 ⇒ 只画到第三行（尾部空行不画，贴底即贴着内容）", async () => {
    const words = ["fake claude", "type a line", "> "].map((text) => ({ text, spans: [] }));
    const blank = Array.from({ length: 20 }, () => ({ text: "   ", spans: [] }));
    const shot: TerminalShot = { lines: [...words, ...blank], text: "", screen: "fp", atText: "12:00:00" };
    const page = rig([row({ terminal: "%1", sid: "s1" })], tabOf(liveFace("idle", true)), shot);
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    const pre = page.el.querySelector("pre");
    expect(pre?.textContent).toBe("fake claude\ntype a line\n> ");
  });
});
