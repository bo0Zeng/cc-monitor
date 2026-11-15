/**
 * 终端页看标签页的那个终端：名单里没有它时照脸上「看它的终端」那一格说（那台写好的那一句，界面不另拼）；
 * 画面尾部的空行不画（打开即贴着有内容的最后一行，不停在下方空白处）；实时流停了（不是那台断开）⇒ 当场重问名单、页头照新那一行；
 * 没往上翻就一直贴底，「回到最新」只在用户真往上翻过之后出。读写换成假的。
 */
import { describe, expect, it, vi } from "vitest";

import { TerminalPage, type InputHolds, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { SessionFace } from "../../../src/frontend/ui/generated/SessionFace";
import type { FollowEvents } from "../../../src/frontend/ui/terminal-follow";
import { goldenFace, liveFace } from "../../test-support/session-faces";

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

/** 订实时的假的：记下页面交来的那两件事，测试里照它推帧 · 收尾。 */
function liveRig(lists: TerminalRow[][], watched: Watched): {
  page: TerminalPage;
  reads: { [K in keyof TerminalReads]: ReturnType<typeof vi.fn> };
  events: () => FollowEvents;
} {
  let got: FollowEvents | null = null;
  let n = 0;
  const reads = {
    list: vi.fn(async () => lists[Math.min(n++, lists.length - 1)]),
    shot: vi.fn(async () => EMPTY_SHOT),
    send: vi.fn(async () => ({ result: "delivered", note: "S-delivered", acts: [] })),
    follow: vi.fn((_o: string, _t: string, ev: FollowEvents) => {
      got = ev;
      return { stop: () => {} };
    }),
  };
  const holds = { take: vi.fn(), release: vi.fn() };
  const page = new TerminalPage({ active: () => watched, focusStream: () => {} }, reads as unknown as TerminalReads, holds as unknown as InputHolds);
  document.body.replaceChildren(page.el);
  return {
    page,
    reads,
    events: () => {
      if (got === null) throw new Error("页面没订实时");
      return got;
    },
  };
}

const linesOf = (n: number, tag: string): TerminalShot => ({
  lines: Array.from({ length: n }, (_, i) => ({ text: `${tag} ${i}`, spans: [] })),
  text: "",
  screen: `${tag}-${n}`,
  atText: "12:00:01",
});

describe("实时流收尾之后页头照名单重写（A17-166-92-1）", () => {
  const held = row({ terminal: "%1", sid: "s1", clients: 1, input: "held", holder: { said: "S-in-window", mine: false } });

  it("★ 收尾帧（终端已不在）⇒ 当场重问名单；名单里没了 ⇒ 不再写持有者与窗口数，改写已结束那一句", async () => {
    const { page, reads, events } = liveRig([[held], []], tabOf(goldenFace("ended")));
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    expect(page.el.textContent).toContain("S-in-window");
    events().screen(linesOf(3, "a"));
    events().stop({ kind: "stopped", said: "S-gone", offline: false });
    await flush();
    expect(reads.list).toHaveBeenCalledTimes(2);
    const text = page.el.textContent ?? "";
    expect(text).not.toContain("S-in-window");
    expect(text).not.toContain(copyText("terminal.tag.windows", { n: 1 }));
    expect(text).toContain(copyText("terminal.empty.ended", { state: copyText("sessionState.ended.name") }));
  });

  it("收尾之后终端还在 ⇒ 页头照新那一行（窗口数 · 持有者）", async () => {
    const after = row({ terminal: "%1", sid: "s1", clients: 0, input: "held", holder: { said: "S-nobody", mine: false } });
    const { page, events } = liveRig([[held], [after]], tabOf(liveFace("idle", true)));
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    events().stop({ kind: "stopped", said: "S-lost", offline: false });
    await flush();
    const text = page.el.textContent ?? "";
    expect(text).toContain("S-nobody");
    expect(text).not.toContain("S-in-window");
    expect(text).toContain(copyText("terminal.tag.background"));
  });

  it("重问名单在途时又抓了一屏（送完那几拍 ·「重新看」）⇒ 名单那一份照样换上", async () => {
    const { page, events } = liveRig([[held], []], tabOf(goldenFace("ended")));
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    events().stop({ kind: "stopped", said: "S-gone", offline: false });
    void page.recapture();
    await flush();
    expect(page.el.textContent).not.toContain("S-in-window");
  });

  it("停在那台断开 ⇒ 不重问名单（问不到；等那台连回来）", async () => {
    const { page, reads, events } = liveRig([[held], []], tabOf(liveFace("idle", true)));
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    events().stop({ kind: "stopped", said: "S-offline", offline: true });
    await flush();
    expect(reads.list).toHaveBeenCalledTimes(1);
  });
});

/** 一行画面多高（假的几何里）。 */
const LINE_PX = 20;

/**
 * 画面那一格的几何换成可控的（jsdom 不排版）：内容高 ＝ 画着的行数 × {@link LINE_PX} · 框高 `g.c`；
 * 滚动位置照浏览器夹在 [0, 内容高 − 框高]（内容变矮时读出来的也夹）。
 */
function fakeGeometry(screen: HTMLElement, g: { c: number }): void {
  let top = 0;
  const h = (): number => Math.max(1, (screen.querySelector("pre")?.textContent ?? "").split("\n").length) * LINE_PX;
  const max = (): number => Math.max(0, h() - g.c);
  Object.defineProperty(screen, "scrollHeight", { configurable: true, get: h });
  Object.defineProperty(screen, "clientHeight", { configurable: true, get: () => g.c });
  Object.defineProperty(screen, "scrollTop", {
    configurable: true,
    get: () => {
      top = Math.min(top, max());
      return top;
    },
    set: (v: number) => {
      top = Math.max(0, Math.min(v, max()));
    },
  });
}

describe("没往上翻就一直贴底（A17-166-92-2）", () => {
  const live = row({ terminal: "%1", sid: "s1", clients: 0 });
  const followHidden = (page: TerminalPage): boolean => {
    const b = [...page.el.querySelectorAll("button")].find((x) => x.textContent === copyText("terminal.screen.follow"));
    return (b?.parentElement as HTMLElement).hidden;
  };

  /** 开页：框高 200；第一帧 50 行（1000 高）⇒ 落到底（800）。 */
  async function opened(): Promise<{
    page: TerminalPage;
    screen: HTMLElement;
    g: { c: number };
    events: () => FollowEvents;
    reads: { [K in keyof TerminalReads]: ReturnType<typeof vi.fn> };
  }> {
    const { page, events, reads } = liveRig([[live]], tabOf(liveFace("idle", true)));
    page.sessionChanged();
    page.setVisible(true);
    await flush();
    const screen = page.el.querySelector("pre")?.parentElement as HTMLElement;
    const g = { c: 200 };
    fakeGeometry(screen, g);
    events().screen(linesOf(50, "a"));
    expect(screen.scrollTop).toBe(800);
    return { page, screen, g, events, reads };
  }

  it("画面框被换下又挂回（出错 → ［刷新］回来）⇒ 浏览器归零的位置不算用户往上翻，仍贴底", async () => {
    const { page, screen, reads } = await opened();
    reads.shot.mockResolvedValue(linesOf(50, "a"));
    reads.list.mockRejectedValueOnce(new Error("S-down"));
    await page.refresh();
    expect(page.el.textContent).toContain("S-down");
    // 换下来再挂回：浏览器把滚动位置归零、不发滚动事件。
    screen.scrollTop = 0;
    await page.refresh();
    await flush();
    expect(screen.scrollTop).toBe(50 * LINE_PX - 200);
    expect(followHidden(page)).toBe(true);
  });

  it("抽屉收起再开 ⇒ 仍贴底", async () => {
    const { page, screen, reads } = await opened();
    reads.shot.mockResolvedValue(linesOf(50, "a"));
    page.setVisible(false);
    screen.scrollTop = 0;
    page.setVisible(true);
    await flush();
    expect(screen.scrollTop).toBe(50 * LINE_PX - 200);
    expect(followHidden(page)).toBe(true);
  });

  it("★ 画面框变矮（送完 Ctrl-C 那一句撑高输入区）、用户没滚，再来一帧 ⇒ 在最底、不出「回到最新」", async () => {
    const { page, screen, g, events } = await opened();
    g.c = 150;
    // 页面自己设到底的那一下引起的滚动事件，排到框变矮之后才到：不算用户往上翻。
    screen.dispatchEvent(new Event("scroll"));
    events().screen(linesOf(60, "c"));
    expect(screen.scrollTop).toBe(60 * LINE_PX - 150);
    expect(followHidden(page)).toBe(true);
  });

  it("清屏重画（内容先缩到一屏之内再长回来）、用户没滚 ⇒ 在最底", async () => {
    const { page, screen, events } = await opened();
    events().screen(linesOf(3, "b"));
    events().screen(linesOf(60, "c"));
    expect(screen.scrollTop).toBe(60 * LINE_PX - 200);
    expect(followHidden(page)).toBe(true);
  });

  it("★ 用户真往上翻过 ⇒ 再来一帧不拽回、出「回到最新」；点它 ⇒ 回到底、接着贴底", async () => {
    const { page, screen, events } = await opened();
    screen.scrollTop = 100;
    screen.dispatchEvent(new Event("scroll"));
    events().screen(linesOf(60, "c"));
    expect(screen.scrollTop).toBe(100);
    expect(followHidden(page)).toBe(false);
    const b = [...page.el.querySelectorAll("button")].find((x) => x.textContent === copyText("terminal.screen.follow")) as HTMLButtonElement;
    b.click();
    expect(screen.scrollTop).toBe(60 * LINE_PX - 200);
    expect(followHidden(page)).toBe(true);
    events().screen(linesOf(70, "d"));
    expect(screen.scrollTop).toBe(70 * LINE_PX - 200);
  });

  it("★ 用户往上滚了、滚动事件还没派发就来了一帧 ⇒ 不拽回、出「回到最新」，随后那一下事件也不把它算回贴底", async () => {
    const { page, screen, events } = await opened();
    screen.scrollTop = 100;
    events().screen(linesOf(60, "c"));
    expect(screen.scrollTop).toBe(100);
    expect(followHidden(page)).toBe(false);
    screen.dispatchEvent(new Event("scroll"));
    events().screen(linesOf(70, "d"));
    expect(screen.scrollTop).toBe(100);
  });

  it("往上翻着、清屏把内容缩进一屏（浏览器夹回底）⇒ 不算用户滚回底，仍不贴", async () => {
    const { page, screen, events } = await opened();
    screen.scrollTop = 100;
    screen.dispatchEvent(new Event("scroll"));
    events().screen(linesOf(3, "b"));
    screen.dispatchEvent(new Event("scroll"));
    events().screen(linesOf(60, "c"));
    expect(screen.scrollTop).toBe(0);
    expect(followHidden(page)).toBe(false);
  });

  it("往上翻了又自己滚回底 ⇒ 接着贴底", async () => {
    const { page, screen, events } = await opened();
    screen.scrollTop = 100;
    screen.dispatchEvent(new Event("scroll"));
    screen.scrollTop = 800;
    screen.dispatchEvent(new Event("scroll"));
    events().screen(linesOf(60, "c"));
    expect(screen.scrollTop).toBe(60 * LINE_PX - 200);
    expect(followHidden(page)).toBe(true);
  });
});
