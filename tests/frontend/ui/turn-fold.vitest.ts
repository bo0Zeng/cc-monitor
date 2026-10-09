// 大折叠（按轮）：两句人话之间除结尾外全收进一行（后端给的结尾与行上的字，界面不按卡型猜）；正在跑的那一轮也折着；
// 点过程行这一轮单独记住、跑完不收；跳卡 / 查找落在折着的过程里自动展开；点「失败 ×N」滚到第一处失败；
// 默认开关（Ctrl+O）全部摊开 / 收回；骨架占位与 ESC 折叠段之后不再归轮（宁可不折）；续取从还没收尾的那一轮起、整轮替换。
// 轮次刻度（§5.2.6）：一轮一格、超过 60 轮并格 · 当前那一轮 · 在等你的那一格琥珀 · 点了 / Alt+↑↓ 跳 · 悬停卡在左侧。
import { describe, it, expect, beforeEach, vi } from "vitest";
import { TurnFold, PROC_LINE_CLASS, setProcessExpandedDefault, fillDur } from "../../../src/frontend/ui/turn-fold";
import { revealCard } from "../../../src/frontend/ui/views/session-viewer";
import { TurnRail, railGroups, RAIL_MAX_TICKS } from "../../../src/frontend/ui/turn-rail";
import { placeFloat } from "../../../src/frontend/ui/kit/place";

/** 轮次刻度的悬停卡：左侧、竖直居中、间距 6（`tooltip.ts` 的 PLACEMENT 表）。 */
const placeCardLeft = (host: DOMRect, tip: { width: number; height: number }, view: { width: number; height: number }) => placeFloat({ rect: host, side: "left", align: "center", gap: 6 }, tip, view);
import type { TurnSummary, TurnsResult } from "../../../src/frontend/ui/session-reads";
import { renderContentRecord, type StreamSink } from "../../../src/frontend/ui/render-stream-record";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";
import { estimateFromFacts, setInjectedShown, skeletonKind } from "../../../src/frontend/ui/height-estimate";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
import { copyText } from "../../../src/frontend/ui/copy-table";

const turn = (uuid: string, at: number, over: Partial<TurnSummary> = {}): TurnSummary => ({
  at,
  uuid,
  start: "2026-10-06T02:01:00Z",
  end: "2026-10-06T02:04:02Z",
  startText: "02:01",
  endText: "02:04",
  said: "改一下",
  tools: 2,
  thinking: 1,
  agents: 0,
  background: 0,
  retries: 0,
  peers: 0,
  fails: 0,
  ending: [],
  reply: "",
  done: true,
  phase: "idle",
  parts: [
    { text: "头", tone: "plain" },
    { text: "工具 ×2", tone: "plain" },
  ],
  span: { text: "02:01–02:04 · {dur}", from: 0, to: 182_000 },
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

const hidden = (el: Element): boolean => el.classList.contains("proc-hidden");
const lines = (root: HTMLElement): HTMLElement[] => [...root.querySelectorAll<HTMLElement>(`.${PROC_LINE_CLASS}`)];

describe("按轮折叠", () => {
  beforeEach(() => setProcessExpandedDefault(false));

  it("完成的轮：开头之后、结尾以外的全收进一行（交回条 · 后台任务 · 压缩 · 报错卡也收），结尾与人说的常显；行上的字照后端写", async () => {
    const u1 = card("card-user", "u1");
    const a1 = card("card-assistant", "a1");
    const g1 = card("card-tool-group", "a2");
    const bar = card("card-speaker", "s1");
    const notice = card("card-notice", "n1");
    const compact = card("card-compact", "k1");
    const c1 = card("card-assistant", "a3");
    const u2 = card("card-user", "u2");
    const parts = [
      { text: "头", tone: "plain" as const },
      { text: "agent ×2", tone: "plain" as const },
      { text: "失败 ×1", tone: "fail" as const },
    ];
    const { content, fold } = rig([u1, a1, g1, bar, notice, compact, c1, u2], [
      { available: true, from: 0, end: 900, turns: [turn("u1", 0, { ending: ["a3"], parts }), turn("u2", 500, { parts: [] })] },
    ]);
    await fold.refresh();
    expect(lines(content)).toHaveLength(1);
    expect(u1.nextElementSibling?.classList.contains(PROC_LINE_CLASS)).toBe(true);
    expect([a1, g1, bar, notice, compact].map(hidden)).toEqual([true, true, true, true, true]);
    expect([u1, c1, u2].map(hidden)).toEqual([false, false, false]);
    const line = lines(content)[0];
    expect(line.getAttribute("aria-expanded")).toBe("false");
    expect(line.querySelector(".proc-text")?.textContent).toBe(["头", "agent ×2", "失败 ×1"].join(copyText("kit.text.sep")));
    expect(line.querySelector(".proc-fails")?.textContent).toBe("失败 ×1");
    expect(line.querySelector(".proc-span")?.textContent).toBe("02:01–02:04 · 3m02s");
  });

  it("正在跑的那一轮也折着：转圈 ＋「现在：…」；在等你 ⇒ 琥珀点；右端用时到现在", async () => {
    const g = card("card-tool-group", "a1") as HTMLDetailsElement;
    const live = { text: "现在：Grep x", tone: "now" as const };
    const { content, fold } = rig([card("card-user", "u1"), g], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { done: false, phase: "running", parts: [{ text: "头", tone: "plain" }, live], span: { text: "02:16 起 · {dur}", from: 0, to: null } })] },
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { done: false, phase: "awaiting", parts: [{ text: "头", tone: "plain" }, { text: "等你批准：Bash x", tone: "need" }] })] },
    ]);
    await fold.refresh();
    expect(hidden(g)).toBe(true);
    const line = lines(content)[0];
    expect(line.dataset.phase).toBe("running");
    expect(line.querySelector(".proc-now")?.textContent).toBe("现在：Grep x");
    expect(line.querySelector('[aria-hidden="true"]')).not.toBeNull();
    await fold.refresh();
    expect(lines(content)[0].dataset.phase).toBe("awaiting");
    expect(lines(content)[0].querySelector(".proc-need")?.textContent).toBe("等你批准：Bash x");
    expect(fillDur({ text: "02:16 起 · {dur}", from: 1000, to: null }, 43_000)).toBe("02:16 起 · 42s");
    expect(fillDur({ text: "02:16", from: null, to: null }, 43_000)).toBe("02:16");
  });

  it("后端不给行（parts 空：过程是空的 / 只有一条压缩摘要）⇒ 不出过程行、一张不折", async () => {
    const k = card("card-compact", "k1");
    const { content, fold } = rig([card("card-user", "u1"), k], [{ available: true, from: 0, end: 10, turns: [turn("u1", 0, { parts: [] })] }]);
    await fold.refresh();
    expect(lines(content)).toHaveLength(0);
    expect(hidden(k)).toBe(false);
  });

  it("手动展开的正在跑那一轮：跑完不收；收起的也不再自己展开", async () => {
    const a = card("card-assistant", "a1");
    const { content, fold } = rig([card("card-user", "u1"), a], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { done: false, phase: "running" })] },
      { available: true, from: 0, end: 20, turns: [turn("u1", 0)] },
    ]);
    await fold.refresh();
    lines(content)[0].click();
    expect(hidden(a)).toBe(false);
    await fold.refresh();
    expect(hidden(a)).toBe(false);
  });

  it("跳卡 / 查找落在折着的过程里（revealCard）：展开那一轮再滚；工具组里的一步也找得到", async () => {
    const g = card("card-tool-group", "a1") as HTMLDetailsElement;
    const unit = document.createElement("div");
    unit.dataset.memberUuid = "a2";
    g.appendChild(unit);
    const { content, fold } = rig([card("card-user", "u1"), g, card("card-assistant", "a3")], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { ending: ["a3"] })] },
    ]);
    Element.prototype.scrollIntoView = vi.fn();
    vi.stubGlobal("CSS", { escape: (x: string) => x });
    await fold.refresh();
    expect(hidden(g)).toBe(true);
    expect(revealCard(content, "a2")).toBe(unit);
    expect(hidden(g)).toBe(false);
    expect(lines(content)[0].getAttribute("aria-expanded")).toBe("true");
  });

  it("点「失败 ×N」：展开并滚到第一处失败；点行别处 ＝ 开合", async () => {
    const ok = card("card-tool-group", "a1");
    const bad = card("card-tool-group", "a2");
    const step = document.createElement("div");
    step.className = "step-line";
    step.dataset.state = "failed";
    bad.appendChild(step);
    const parts = [
      { text: "头", tone: "plain" as const },
      { text: "失败 ×1", tone: "fail" as const },
    ];
    const { content, fold } = rig([card("card-user", "u1"), ok, bad], [{ available: true, from: 0, end: 10, turns: [turn("u1", 0, { parts })] }]);
    const scrolled = vi.fn();
    step.scrollIntoView = scrolled;
    await fold.refresh();
    lines(content)[0].querySelector<HTMLElement>(".proc-fails")!.click();
    expect([hidden(ok), hidden(bad)]).toEqual([false, false]);
    expect(scrolled).toHaveBeenCalled();
    lines(content)[0].querySelector<HTMLElement>(".proc-fails")!.click();
    expect(hidden(bad)).toBe(false);
    lines(content)[0].click();
    expect(hidden(bad)).toBe(true);
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

  it("骨架占位按账本（uuid→seq）认：里面没有哪一轮的开头 ⇒ 它属于这一轮的过程，折着就藏（不物化）、后面接着折；里面有 ⇒ 后面的卡归最后那个开头的轮（开头还没建 ⇒ 先不画行）；展开 ⇒ 露出并叫骨架物化", async () => {
    const gapIn = document.createElement("div");
    gapIn.className = "stream-skeleton-gap";
    gapIn.dataset.skeletonLo = "3";
    gapIn.dataset.skeletonHi = "9";
    const after = card("card-assistant", "x1");
    const gapCross = document.createElement("div");
    gapCross.className = "stream-skeleton-gap";
    gapCross.dataset.skeletonLo = "12";
    gapCross.dataset.skeletonHi = "30";
    const tail = card("card-assistant", "x2");
    const content = document.createElement("div");
    content.append(card("card-user", "u1"), gapIn, after, card("card-assistant", "e1"), gapCross, tail);
    document.body.replaceChildren(content);
    const fill = vi.fn(() => 0);
    const seqs = new Map([["u1", 1], ["u2", 20]]);
    const read = vi.fn(async (): Promise<TurnsResult> => ({ available: true, from: 0, end: 10, turns: [turn("u1", 0, { ending: ["e1"] }), turn("u2", 5)] }));
    const fold = new TurnFold(content, content, () => ({ origin: "local" as never, jsonlPath: "/p/s.jsonl" }), read, () => ({ ledger: { uuidToSeq: seqs, endSeq: 40 }, fillVisible: fill, setFolds: vi.fn() }));
    await fold.refresh();
    expect([hidden(gapIn), hidden(after)]).toEqual([true, true]);
    expect(gapIn.dataset.procOf).toBe("u1");
    expect([hidden(gapCross), hidden(tail)]).toEqual([false, true]);
    expect(tail.dataset.procOf).toBe("u2");
    expect(lines(content)).toHaveLength(1);
    lines(content)[0].click();
    expect(hidden(gapIn)).toBe(false);
    expect(fill).toHaveBeenCalled();
  });

  it("折着的轮把它的过程行（开头之后到下一轮开头 / 账本尾，扣掉结尾那几行）告诉骨架：按 0 高、不物化；带行的开头多一条行高；展开 ⇒ 撤掉", async () => {
    const content = document.createElement("div");
    content.append(card("card-user", "u1"), card("card-assistant", "e1"), card("card-user", "u2"), card("card-user", "u3"));
    document.body.replaceChildren(content);
    const setFolds = vi.fn();
    const seqs = new Map([["u1", 1], ["e1", 10], ["u2", 12], ["u3", 30]]);
    const read = vi.fn(async (): Promise<TurnsResult> => ({
      available: true,
      from: 0,
      end: 10,
      turns: [turn("u1", 0, { ending: ["e1"] }), turn("u2", 5), turn("u3", 9, { parts: [] })],
    }));
    const sk = { ledger: { uuidToSeq: seqs, endSeq: 40 }, fillVisible: vi.fn(() => 0), setFolds };
    const fold = new TurnFold(content, content, () => ({ origin: "local" as never, jsonlPath: "/p/s.jsonl" }), read, () => sk);
    await fold.refresh();
    expect(setFolds).toHaveBeenLastCalledWith({ folded: [[2, 10], [11, 12], [13, 30]], lines: [1, 12], linePx: 40 });
    lines(content)[0].click();
    expect(setFolds.mock.lastCall?.[0].folded).toEqual([[13, 30]]);
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

  it("暂定结论被降级（又调了工具）：那一刻视口正落在它上面 ⇒ 先不收，滚出去 / 切走再收", async () => {
    const a = card("card-assistant", "a1");
    const g = card("card-tool-group", "a2");
    const { content, fold } = rig([card("card-user", "u1"), a, g], [
      { available: true, from: 0, end: 10, turns: [turn("u1", 0, { done: false, ending: ["a1"] })] },
      { available: true, from: 0, end: 20, turns: [turn("u1", 0, { done: false, ending: [] })] },
    ]);
    const box = (top: number, bottom: number) => ({ top, bottom, height: bottom - top }) as DOMRect;
    content.getBoundingClientRect = () => box(0, 500);
    a.getBoundingClientRect = () => box(100, 200);
    await fold.refresh();
    expect(hidden(a)).toBe(false);
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
