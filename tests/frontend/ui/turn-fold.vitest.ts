// 按轮折叠（主窗口稿 §5.2.2）：完成的轮过程折成一行、结论常显；正在跑的那一轮展开；点过程行这一轮单独记住；
// 默认开关（Ctrl+O）全部摊开 / 收回；骨架占位与 ESC 折叠段之后不再归轮（宁可不折）；续取从还没收尾的那一轮起、整轮替换。
// 轮次刻度（§5.2.6）：一轮一格、超过 60 轮并格 · 当前那一轮 · 在等你的那一格琥珀 · 点了 / Alt+↑↓ 跳 · 悬停卡在左侧。
import { describe, it, expect, beforeEach, vi } from "vitest";
import { TurnFold, PROC_LINE_CLASS, PROC_HIDDEN_CLASS, setProcessExpandedDefault } from "../../../src/frontend/ui/turn-fold";
import { TurnRail, railGroups, RAIL_MAX_TICKS } from "../../../src/frontend/ui/turn-rail";
import { placeCardLeft } from "../../../src/frontend/ui/kit/tooltip";
import type { TurnSummary, TurnsResult } from "../../../src/frontend/ui/session-reads";
import { renderContentRecord, type StreamSink } from "../../../src/frontend/ui/render-stream-record";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";
import { estimateFromFacts, setInjectedShown, skeletonKind } from "../../../src/frontend/ui/height-estimate";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";

const turn = (uuid: string, at: number, over: Partial<TurnSummary> = {}): TurnSummary => ({
  at,
  uuid,
  start: "2026-10-06T02:01:00Z",
  end: "2026-10-06T02:04:02Z",
  said: "改一下",
  tools: 2,
  thinking: 1,
  fails: 0,
  conclusion: [],
  reply: "",
  done: true,
  ...over,
});

function card(cls: string, uuid?: string): HTMLElement {
  const el = cls === "card-tool-group" ? document.createElement("details") : document.createElement("div");
  el.className = `card ${cls}`;
  if (uuid) el.setAttribute("data-uuid", uuid);
  return el;
}

function rig(cards: HTMLElement[], reads: TurnsResult[]) {
  const content = document.createElement("div");
  content.append(...cards);
  document.body.replaceChildren(content);
  const read = vi.fn(async (_o: unknown, _p: string, _from: number): Promise<TurnsResult> => reads.shift() ?? { available: false, reason: "x" });
  const fold = new TurnFold(content, content, () => ({ origin: "local" as never, jsonlPath: "/p/s.jsonl" }), read);
  return { content, fold, read };
}

const hidden = (el: Element): boolean => el.classList.contains(PROC_HIDDEN_CLASS);
const lines = (root: HTMLElement): HTMLElement[] => [...root.querySelectorAll<HTMLElement>(`.${PROC_LINE_CLASS}`)];

describe("按轮折叠", () => {
  beforeEach(() => setProcessExpandedDefault(false));

  it("完成的轮：过程折成一行（紧跟你那句），结论与人说的、事件条常显", async () => {
    const u1 = card("card-user", "u1");
    const a1 = card("card-assistant", "a1");
    const g1 = card("card-tool-group", "a2");
    const bar = card("card-speaker", "s1");
    const c1 = card("card-assistant", "a3");
    const u2 = card("card-user", "u2");
    const live = card("card-assistant", "a4");
    const { content, fold } = rig([u1, a1, g1, bar, c1, u2, live], [
      { available: true, from: 0, end: 900, turns: [turn("u1", 0, { conclusion: ["a3"], fails: 1 }), turn("u2", 500, { done: false })] },
    ]);
    await fold.refresh();
    expect(lines(content)).toHaveLength(1);
    expect(u1.nextElementSibling?.classList.contains(PROC_LINE_CLASS)).toBe(true);
    expect([a1, g1].map(hidden)).toEqual([true, true]);
    expect([u1, bar, c1, u2, live].map(hidden)).toEqual([false, false, false, false, false]);
    const line = lines(content)[0];
    expect(line.getAttribute("aria-expanded")).toBe("false");
    expect(line.textContent).toContain("工具 ×2");
    expect(line.textContent).toContain("思考 ×1");
    expect(line.querySelector(".proc-fails")?.textContent).toBe("失败 ×1");
  });

  it("正在跑的那一轮：不出过程行、过程展开，工具组整组摊开", async () => {
    const g = card("card-tool-group", "a1") as HTMLDetailsElement;
    const { content, fold } = rig([card("card-user", "u1"), g], [{ available: true, from: 0, end: 10, turns: [turn("u1", 0, { done: false })] }]);
    await fold.refresh();
    expect(lines(content)).toHaveLength(0);
    expect(hidden(g)).toBe(false);
    expect(g.open).toBe(true);
    expect(g.dataset.procOf).toBe("u1");
  });

  it("没有工具也没有思考的轮不出过程行", async () => {
    const { content, fold } = rig([card("card-user", "u1"), card("card-assistant", "a1")], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { tools: 0, thinking: 0, conclusion: ["a1"] })] },
    ]);
    await fold.refresh();
    expect(lines(content)).toHaveLength(0);
  });

  it("点过程行：这一轮就地展开 / 收起；Ctrl+O 换默认、各轮单独记的作废", async () => {
    const a = card("card-assistant", "a1");
    const b = card("card-assistant", "b1");
    const { content, fold } = rig([card("card-user", "u1"), a, card("card-user", "u2"), b], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0), turn("u2", 5)] },
    ]);
    await fold.refresh();
    lines(content)[0].click();
    expect([hidden(a), hidden(b)]).toEqual([false, true]);
    expect(lines(content)[0].getAttribute("aria-expanded")).toBe("true");
    fold.setDefault(true);
    expect([hidden(a), hidden(b)]).toEqual([false, false]);
    fold.setDefault(false);
    expect([hidden(a), hidden(b)]).toEqual([true, true]);
  });

  it("骨架占位 / ESC 折叠段之后不再归轮，认出下一轮开头再接着折", async () => {
    const gap = document.createElement("div");
    gap.className = "stream-skeleton-gap";
    const after = card("card-assistant", "x1");
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    const after2 = card("card-assistant", "x2");
    const p3 = card("card-assistant", "x3");
    const { fold } = rig([card("card-user", "u1"), gap, after, wrap, after2, card("card-user", "u3"), p3], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0), turn("u3", 8)] },
    ]);
    await fold.refresh();
    expect([after, after2].map(hidden)).toEqual([false, false]);
    expect(hidden(p3)).toBe(true);
  });

  it("卡后到（上翻补批 / 骨架物化）：DOM 一变就重排", async () => {
    const { content, fold } = rig([card("card-user", "u1")], [{ available: true, from: 0, end: 10, turns: [turn("u1", 0)] }]);
    await fold.refresh();
    const late = card("card-assistant", "a9");
    content.appendChild(late);
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    expect(hidden(late)).toBe(true);
  });

  it("续取：从还没收尾的那一轮的 at 起要，回来的整轮替换；续点不在了 ⇒ 从 0 重要", async () => {
    const { fold, read } = rig([], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0), turn("u2", 40, { done: false })] },
      { available: true, from: 40, end: 20, turns: [turn("u2", 40), turn("u3", 90, { done: false })] },
      { available: false, reason: "截断" },
      { available: true, from: 0, end: 5, turns: [turn("u1", 0)] },
    ]);
    await fold.refresh();
    await fold.refresh();
    expect(read.mock.calls.map((c) => c[2])).toEqual([0, 40]);
    expect(fold.all.map((t) => [t.uuid, t.done])).toEqual([
      ["u1", true],
      ["u2", true],
      ["u3", false],
    ]);
    await fold.refresh();
    expect(read.mock.calls.map((c) => c[2])).toEqual([0, 40, 90, 0]);
    expect(fold.all.map((t) => t.uuid)).toEqual(["u1"]);
  });

  it("收尾那一刻视口在它的过程里：先不收，滚出去 / 切走再收", async () => {
    const a = card("card-assistant", "a1");
    const { content, fold } = rig([card("card-user", "u1"), a], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { done: false })] },
      { available: true, from: 0, end: 20, turns: [turn("u1", 0)] },
    ]);
    const box = (top: number, bottom: number) => ({ top, bottom, height: bottom - top }) as DOMRect;
    content.getBoundingClientRect = () => box(0, 500);
    a.getBoundingClientRect = () => box(100, 200);
    await fold.refresh();
    await fold.refresh();
    expect(hidden(a)).toBe(false);
    a.getBoundingClientRect = () => box(-300, -200);
    fold.releaseOnScroll();
    expect(hidden(a)).toBe(true);
  });
});

describe("轮次刻度", () => {
  const box = (top: number, bottom: number) => ({ top, bottom, height: bottom - top }) as DOMRect;
  function railRig(n: number, waiting = false) {
    const turns = Array.from({ length: n }, (_, i) => turn(`u${i}`, i * 10, { said: `第 ${i} 句`, reply: `回 ${i}` }));
    const content = document.createElement("div");
    const heads = turns.map((t, i) => {
      const el = card("card-user", t.uuid);
      el.getBoundingClientRect = () => box(i * 100 - 250, i * 100 - 200);
      return el;
    });
    content.append(...heads);
    const scroller = document.createElement("div");
    scroller.getBoundingClientRect = () => box(0, 400);
    Object.defineProperty(scroller, "clientWidth", { value: 1200 });
    const jump = vi.fn();
    const rail = new TurnRail(scroller, content, { turns: () => turns, waiting: () => waiting, jump });
    document.body.replaceChildren(content, rail.el);
    rail.render();
    return { rail, jump, ticks: () => [...rail.el.querySelectorAll<HTMLElement>(".turn-tick")] };
  }

  it("一轮一格；超过 60 轮相邻并格（悬停给范围）", () => {
    expect(railGroups(3)).toEqual([[0, 0], [1, 1], [2, 2]]);
    const g = railGroups(130);
    expect(g.length).toBeLessThanOrEqual(RAIL_MAX_TICKS);
    expect(g[0]).toEqual([0, 2]);
    expect(g[g.length - 1]).toEqual([129, 129]);
    expect(g.flatMap(([a, b]) => Array.from({ length: b - a + 1 }, (_, k) => a + k))).toEqual(Array.from({ length: 130 }, (_, i) => i));
  });

  it("当前那一轮：开头已过视口上沿的最后一轮；在等你 ⇒ 最后一格琥珀", () => {
    const { ticks } = railRig(5, true);
    // 开头在 -250 · -150 · -50 · 50 · 150 ⇒ 过上沿（8）的最后一个是第 3 轮（下标 2）
    expect(ticks().map((t) => t.dataset.viewport)).toEqual(["false", "false", "true", "false", "false"]);
    expect(ticks().map((t) => t.dataset.waiting)).toEqual(["false", "false", "false", "false", "true"]);
  });

  it("点一格 ⇒ 跳到那一轮开头；Alt+↑↓ 上 / 下一轮", () => {
    const { rail, jump, ticks } = railRig(5);
    ticks()[4].click();
    expect(jump).toHaveBeenLastCalledWith("u4");
    rail.step(1);
    expect(jump).toHaveBeenLastCalledWith("u3");
    rail.step(-1);
    expect(jump).toHaveBeenLastCalledWith("u1");
  });

  it("正文列 ≤ 900px 不出；没有轮不出", () => {
    const { rail } = railRig(0);
    expect(rail.el.hidden).toBe(true);
  });

  it("正文列 ≤ 900px 也不出", () => {
    const content = document.createElement("div");
    const scroller = document.createElement("div");
    Object.defineProperty(scroller, "clientWidth", { value: 900 });
    const rail = new TurnRail(scroller, content, { turns: () => [turn("u0", 0)], waiting: () => false, jump: () => {} });
    rail.render();
    expect(rail.el.hidden).toBe(true);
  });

  it("悬停卡在格子左侧、竖直居中；左边放不下翻到右侧", () => {
    const host = { left: 1240, right: 1248, top: 300, bottom: 303, width: 8, height: 3 } as DOMRect;
    expect(placeCardLeft(host, { width: 300, height: 80 }, { width: 1280, height: 800 })).toEqual({ left: 934, top: 261.5 });
    const edge = { left: 100, right: 108, top: 300, bottom: 303, width: 8, height: 3 } as DOMRect;
    expect(placeCardLeft(edge, { width: 300, height: 80 }, { width: 1280, height: 800 }).left).toBe(114);
  });
});

// 系统注入（「谁说的」稿 A ⑤）：旁注细条——开关关着不露、估高 0；不算任何人的邻居（不打散工具组的相邻合并）。
describe("系统注入的旁注细条", () => {
  const tool = (uuid: string, id: string) =>
    ({ type: "assistant", uuid, timestamp: "2026-01-01T02:02:00.000Z", message: { role: "assistant", content: [{ type: "tool_use", id, name: "Read", input: {} }] } }) as never;
  const injected = (uuid: string, body?: string) =>
    ({ type: "user", uuid, timestamp: "2026-01-01T02:02:30.000Z", message: { role: "user", content: "x" }, userText: { speaker: { kind: "system", ...(body ? { body } : {}) }, text: "" } }) as never;

  it("★ 有正文 ⇒ 一条隐藏的细条进流（带 uuid）；夹在两次工具调用之间也不打散工具组；没正文 ⇒ 什么都不放", () => {
    const content = document.createElement("div");
    const stream = { contentElement: content, insertNode: (el: HTMLElement, ref: HTMLElement | null) => content.insertBefore(el, ref) };
    const timeline = new RecordTimeline(stream as never);
    const sink: StreamSink = { timeline, onBranchRecord: () => {} };
    const ctx = { parentPath: "/p/s.jsonl", origin: LOCAL_ORIGIN, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map() };
    const feed = (seq: number, message: never) => renderContentRecord({ session_id: "s", seq, message } as unknown as JsonlLinePayload, ctx, sink);
    feed(1, tool("a1", "t1"));
    feed(2, injected("m1", "注入词乙"));
    feed(3, tool("a2", "t2"));
    feed(4, injected("m2"));
    const kids = [...content.children] as HTMLElement[];
    expect(kids.map((k) => k.classList.contains("card-tool-group") ? "group" : k.classList.contains("card-injected") ? "aside" : "?")).toEqual(["group", "aside"]);
    expect(kids[0].querySelectorAll(".card-tool-group-body > *").length).toBe(2);
    expect([kids[1].getAttribute("data-uuid"), kids[1].querySelector(".injected-body")?.textContent]).toEqual(["m1", "注入词乙"]);
  });

  it("★ 估高跟着开关：关着 0、开着一条细条；它不是任何一类（前后的工具照样并成一组）", () => {
    const f = { o: 0, n: 1, t: "user", sp: "system", ch: 200, pl: 9 } as never;
    expect(skeletonKind(f)).toBe("none");
    setInjectedShown(false);
    expect(estimateFromFacts(f, "tool")).toBe(0);
    setInjectedShown(true);
    expect(estimateFromFacts(f, "tool")).toBeGreaterThan(0);
    setInjectedShown(false);
  });
});
