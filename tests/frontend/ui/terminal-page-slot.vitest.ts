/**
 * 抽屉终端页看占位标签页的那个终端：宿主交「占位那一形」（哪台 · 终端句柄 · tmux 会话名 · 标题）⇒ 名单里按句柄认（不按会话认）、抓那一屏、订实时，
 * 页头是占位的标题与那台；那个终端不在了 ⇒ 说那个 tmux 会话不在。托管那一形：写谁在输入、［接管输入］/［交还］、被拒成「输入在另一端」⇒［接管再送］。读写换成假的。
 */
import { describe, expect, it, vi } from "vitest";

import { TerminalPage, type InputHolds, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";

const SLOT: Watched = { kind: "slot", origin: "devbox", terminal: "%7", name: "billing-2", title: "billing" };

function row(over: Partial<TerminalRow>): TerminalRow {
  return { terminal: "%1", name:"x", sid: null, clients: 0, input: "shared", programExited: false, inputNo: null, ...over };
}

const SHOT: TerminalShot = { lines: [], text: "$", screen: "fp", atText: "12:00:00" };

function rig(rows: TerminalRow[]): {
  page: TerminalPage;
  reads: { [K in keyof TerminalReads]: ReturnType<typeof vi.fn> };
  holds: { [K in keyof InputHolds]: ReturnType<typeof vi.fn> };
} {
  const reads = {
    list: vi.fn(async () => rows),
    shot: vi.fn(async () => SHOT),
    send: vi.fn(),
    follow: vi.fn(() => ({ stop: () => {} })),
  };
  const holds = { take: vi.fn(), release: vi.fn() };
  const page = new TerminalPage({ active: () => SLOT, front: () => {}, focusStream: () => {} }, reads as unknown as TerminalReads, undefined, holds as unknown as InputHolds);
  document.body.replaceChildren(page.el);
  return { page, reads, holds };
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

describe("终端页看占位标签页的那个终端", () => {
  it("★ 按句柄认那一行（不按会话认）、抓那一屏、订实时；页头是占位的标题与那台", async () => {
    const { page, reads } = rig([row({ terminal: "%3", name:"other" }), row({ terminal: "%7", name:"billing-2" })]);
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

describe("托管那一形：谁在输入 · 接管 · 交还", () => {
  const button = (page: TerminalPage, label: string): HTMLButtonElement | undefined =>
    [...page.el.querySelectorAll("button")].find((b) => b.textContent === label && b.style.display !== "none");

  it("★★ 别的一端持有 ⇒ 写后端那一句、给［接管输入］；点了交 `terminal-take-input`，回的那一格就地换上 ⇒［交还］", async () => {
    const { page, holds } = rig([row({ terminal: "%7", name:"billing-2", input: "held", holder: { said: "S-in-phone", mine: false } })]);
    holds.take.mockResolvedValue({ kind: "held", holder: { who: "desktop", mine: true, said: "S-here" } });
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    expect(page.el.textContent).toContain("S-in-phone");
    button(page, copyText("terminal.input.take"))?.click();
    await flush();
    expect(holds.take).toHaveBeenCalledWith("devbox", "%7");
    expect(page.el.textContent).toContain("S-here");
    expect(button(page, copyText("terminal.input.release")), "这一端持有 ⇒［交还］").toBeDefined();
    holds.release.mockResolvedValue({ kind: "held", holder: { who: "none", mine: false, said: "S-none" } });
    button(page, copyText("terminal.input.release"))?.click();
    await flush();
    expect(holds.release).toHaveBeenCalledWith("devbox", "%7");
    expect(button(page, copyText("terminal.input.take"))).toBeDefined();
  });

  it("★ tmux 那一形不出［接管输入］/［交还］", async () => {
    const { page } = rig([row({ terminal: "%7", name:"billing-2" })]);
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    expect(button(page, copyText("terminal.input.take"))).toBeUndefined();
    expect(button(page, copyText("terminal.input.release"))).toBeUndefined();
  });

  it("★★ 送字被拒成「输入在另一端」（下一步带 take）⇒ 照那句说、给［接管再送］：同一份字带 take 再送", async () => {
    const { page, reads } = rig([row({ terminal: "%7", name:"billing-2", input: "held", holder: { said: "S-in-phone", mine: false } })]);
    reads.send.mockResolvedValueOnce({ result: "refused", said: "S-held", screen: null, acts: ["take"] }).mockResolvedValue({ result: "delivered", acts: [] });
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    await page.send({ key: "esc" });
    await flush();
    expect(page.el.textContent).toContain(copyText("terminal.input.failed", { why: "S-held" }));
    button(page, copyText("terminal.input.takeSend"))?.click();
    await flush();
    expect(reads.send.mock.calls.map((c) => [c[1], c[2], c[4]])).toEqual([
      ["%7", { key: "esc" }, undefined],
      ["%7", { key: "esc" }, true],
    ]);
  });
});
