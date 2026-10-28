/**
 * 抽屉终端页看占位标签页的那个终端：宿主交「占位那一形」（哪台 · 终端句柄 · tmux 会话名 · 标题）⇒ 名单里按句柄认（不按会话认）、抓那一屏、订实时，
 * 页头是占位的标题与那台；那个终端不在了 ⇒ 说那个 tmux 会话不在。托管那一形：写谁在输入、［接管输入］/［交还］；别端持有 ⇒ 按键 · Ctrl+回车灰掉、
 * 输入框占位说直接打字需接管，实时流推来新的持有者就地换（A3-3-32-7）。读写换成假的。
 */
import { describe, expect, it, vi } from "vitest";

import { TerminalPage, type InputHolds, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import type { FollowEvents } from "../../../src/frontend/ui/terminal-follow";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { ControlError } from "../../../src/frontend/ui/control-said";

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

  it("★★ 别端持有 ⇒ 按键全灰、尾巴写那一句 ＋「接管后可按键」、占位说直接打字需接管、Ctrl+回车一问都不发；实时流推来「此处持有」⇒［交还］、按键亮", async () => {
    const { page, reads } = rig([row({ terminal: "%7", name: "billing-2", input: "held", holder: { said: "S-in-window", mine: false } })]);
    let live: FollowEvents | null = null;
    reads.follow.mockImplementation((_o: string, _t: string, ev: FollowEvents) => {
      live = ev;
      return { stop: () => {} };
    });
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    const keys = [...page.el.querySelectorAll("button")].filter((b) => b.textContent === copyText("terminal.key.esc"));
    expect(keys).toHaveLength(1);
    expect(keys[0].getAttribute("aria-disabled") === "true" || keys[0].disabled, "按键灰着").toBe(true);
    expect(page.el.textContent).toContain(copyText("terminal.input.keysHeld", { said: "S-in-window" }));
    const box = page.el.querySelector("textarea") as HTMLTextAreaElement;
    expect(box.placeholder).toBe(copyText("terminal.input.placeholderHeld"));
    box.value = "ls";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }));
    await flush();
    expect(reads.send, "Ctrl+回车不发").not.toHaveBeenCalled();
    expect(page.el.textContent).toContain(copyText("terminal.input.keysHeld", { said: "S-in-window" }));
    // 实时流推来新的持有者（画面没变）⇒ 就地换。
    live!.screen(SHOT, { said: "S-here", mine: true });
    await flush();
    expect(page.el.textContent).toContain("S-here");
    expect(button(page, copyText("terminal.input.release"))).toBeDefined();
    expect(keys[0].getAttribute("aria-disabled") === "true" || keys[0].disabled, "按键亮了").toBe(false);
    expect(box.placeholder).not.toBe(copyText("terminal.input.placeholderHeld"));
  });

  it("★ 没有［接管再送］：送字被拒成「输入在另一端」只照那句说", async () => {
    const { page, reads } = rig([row({ terminal: "%7", name: "billing-2", input: "held", holder: { said: "S-here", mine: true } })]);
    reads.send.mockResolvedValue({ result: "refused", said: "S-held", screen: null, acts: [] });
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    await page.send({ key: "esc" });
    await flush();
    expect(page.el.textContent).toContain(copyText("terminal.input.failed", { why: "S-held" }));
    expect([...page.el.querySelectorAll("button")].filter((b) => b.textContent === copyText("terminal.input.retry"))).toHaveLength(0);
    expect(reads.send.mock.calls.map((c) => c.length)).toEqual([4]);
  });
});

describe("A2-16 对稿留下的两处（托管那一形）", () => {
  it("★ 接管被拒 ⇒ 结局那一行是那台写好的那一句（不是空的、不是笼统的「被拒」）", async () => {
    const { page, holds } = rig([row({ terminal: "%7", name: "billing-2", input: "held", holder: { said: "S-in-window", mine: false } })]);
    holds.take.mockRejectedValue(new ControlError("S-take-refused", "raw detail"));
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    [...page.el.querySelectorAll("button")].find((b) => b.textContent === copyText("terminal.input.take"))?.click();
    await flush();
    expect(holds.take).toHaveBeenCalled();
    expect(page.el.textContent).toContain("S-take-refused");
  });

  it("★ 手机经终端 tab 持有输入（它就是一个终端窗口）⇒ 页头写「终端窗口 ×1」，不写「后台」", async () => {
    const { page } = rig([row({ terminal: "%7", name: "billing-2", input: "held", clients: 1, holder: { said: "S-in-window", mine: false } })]);
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    expect(page.el.textContent).toContain(copyText("terminal.tag.windows", { n: 1 }));
    expect(page.el.textContent).not.toContain(copyText("terminal.tag.background"));
    expect(page.el.textContent).toContain("S-in-window");
  });
});
