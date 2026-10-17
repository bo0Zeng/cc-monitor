/**
 * 抽屉终端页看占位标签页的那个终端：宿主交「占位那一形」（哪台 · 终端句柄 · tmux 会话名 · 标题）⇒ 名单里按句柄认（不按会话认）、抓那一屏、订实时，
 * 页头是占位的标题与那台；那个终端不在了 ⇒ 说那个 tmux 会话不在。读写换成假的。
 */
import { describe, expect, it, vi } from "vitest";

import { TerminalPage, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";

const SLOT: Watched = { kind: "slot", origin: "devbox", terminal: "%7", name: "billing-2", title: "billing" };

function row(over: Partial<TerminalRow>): TerminalRow {
  return { terminal: "%1", tmuxName: "x", sid: null, clients: 0, input: "shared", programExited: false, inputNo: null, ...over };
}

const SHOT: TerminalShot = { lines: [], text: "$", screen: "fp", atText: "12:00:00" };

function rig(rows: TerminalRow[]): { page: TerminalPage; reads: { [K in keyof TerminalReads]: ReturnType<typeof vi.fn> } } {
  const reads = {
    list: vi.fn(async () => rows),
    shot: vi.fn(async () => SHOT),
    send: vi.fn(),
    follow: vi.fn(() => ({ stop: () => {} })),
  };
  const page = new TerminalPage({ active: () => SLOT, front: () => {}, focusStream: () => {} }, reads as unknown as TerminalReads);
  document.body.replaceChildren(page.el);
  return { page, reads };
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

describe("终端页看占位标签页的那个终端", () => {
  it("★ 按句柄认那一行（不按会话认）、抓那一屏、订实时；页头是占位的标题与那台", async () => {
    const { page, reads } = rig([row({ terminal: "%3", tmuxName: "other" }), row({ terminal: "%7", tmuxName: "billing-2" })]);
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    expect(reads.list).toHaveBeenCalledWith("devbox");
    expect(reads.shot).toHaveBeenCalledWith("devbox", "%7");
    expect(reads.follow.mock.calls[0].slice(0, 2)).toEqual(["devbox", "%7"]);
    expect(page.el.textContent).toContain("billing");
    expect(page.el.textContent).toContain(copyText("terminal.tag.tmux", { name: "billing-2" }));
  });

  it("那个终端已不在 ⇒ 说那个 tmux 会话不在、不抓屏", async () => {
    const { page, reads } = rig([row({ terminal: "%3" })]);
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    expect(reads.shot).not.toHaveBeenCalled();
    expect(page.el.textContent).toContain(copyText("launch.slot.tmuxGone", { machine: "devbox", name: "billing-2" }));
  });
});
