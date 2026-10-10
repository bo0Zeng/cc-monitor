/**
 * 大折叠 × 骨架（真 `MessageStream` · `RecordTimeline` · `SkeletonView` · `TurnFold`，几何用一维布局模型）：
 * 「这块占位折没折」只有一个来源 —— 占位带不带 `proc-hidden` ⇔ 账本里它整块落在折着的那几段（按 0 高）里；
 * 折着的轮的过程占位一定藏、展开的一定露。滚动物化之后、展开别的轮、展开再折回去都一样。
 * 夹具每轮：开头 · 几行过程 · 结尾 · 结尾之后又来的两行（收场通知那种，也是过程）。
 */
import { describe, expect, it, vi } from "vitest";
import type { SkeletonFacts } from "../../../src/frontend/ui/height-estimate";
import { SkeletonLedger } from "../../../src/frontend/ui/live-window";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";
import { SKELETON_GAP_CLASS, SkeletonView } from "../../../src/frontend/ui/skeleton-view";
import { MessageStream } from "../../../src/frontend/ui/stream";
import { TurnFold, PROC_LINE_CLASS } from "../../../src/frontend/ui/turn-fold";
import type { TurnSummary, TurnsResult } from "../../../src/frontend/ui/session-reads";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));

globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

const VIEW_H = 800;
const PER_TURN = 10;
const TURNS = 30;
/** 结尾在一轮里的第几行；它后面那两行仍是过程。 */
const ENDING_AT = 7;

/** 一维布局：子元素自上而下堆，高 = 占位内联高 / 卡的 data-h；`proc-hidden` 的按 0（display:none）；过程行按 `linePx`（缺省 0）。 */
function installLayout(scrollEl: HTMLElement, content: HTMLElement, linePx = 0): { scrollTop: number } {
  const state = { scrollTop: 0 };
  Object.defineProperty(scrollEl, "scrollTop", { get: () => state.scrollTop, set: (v: number) => (state.scrollTop = v), configurable: true });
  const h = (el: Element): number => {
    const e = el as HTMLElement;
    if (e.classList.contains("proc-hidden")) return 0;
    if (e.classList.contains(PROC_LINE_CLASS)) return linePx;
    if (e.style.height) return parseFloat(e.style.height);
    return Number(e.dataset.h ?? 0);
  };
  const rect = (top: number, hh: number): DOMRect => ({ top, bottom: top + hh, height: hh, left: 0, right: 100, width: 100, x: 0, y: top }) as DOMRect;
  scrollEl.getBoundingClientRect = () => rect(0, VIEW_H);
  const orig = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement): DOMRect {
    if (this.parentElement !== content) return orig.call(this);
    // display:none 的量出来是一个全 0 的框（真引擎也是）。
    if (this.classList.contains("proc-hidden")) return rect(0, 0);
    let y = -state.scrollTop;
    for (const c of Array.from(content.children)) {
      const hh = h(c);
      if (c === this) return rect(y, hh);
      y += hh;
    }
    return rect(0, 0);
  };
  return state;
}

const facts = (i: number): SkeletonFacts => ({ o: i * 100, n: 100, t: i % PER_TURN === 0 ? "said" : "reply", u: `u${i}`, ch: 120, pl: 2 });

const turn = (k: number): TurnSummary => ({
  at: k * PER_TURN * 100,
  uuid: `u${k * PER_TURN}`,
  start: "2026-10-06T02:01:00Z",
  end: "2026-10-06T02:04:02Z",
  startText: "02:01",
  endText: "02:04",
  said: "q",
  tools: 3,
  thinking: 1,
  agents: 0,
  background: 0,
  retries: 0,
  peers: 0,
  fails: 0,
  ending: [`u${k * PER_TURN + ENDING_AT}`],
  reply: "",
  done: true,
  phase: "idle",
  parts: [{ text: "p", tone: "plain" }],
  span: { text: "", from: null, to: null },
});

async function rig() {
  const scrollEl = document.createElement("div");
  document.body.replaceChildren(scrollEl);
  const stream = new MessageStream(scrollEl);
  const timeline = new RecordTimeline(stream);
  const content = stream.contentElement;
  const layout = installLayout(scrollEl, content);
  const total = TURNS * PER_TURN;
  const ledger = new SkeletonLedger(0, Array.from({ length: total }, (_, i) => facts(i)));
  const view = new SkeletonView(ledger, scrollEl, timeline, {
    materialize(lo: number, hi: number): void {
      for (let s = lo; s < hi; s++) {
        const el = document.createElement("div");
        el.dataset.h = "60";
        el.dataset.seq = String(s);
        el.setAttribute("data-id", `u${s}`);
        timeline.insert({ seq: s, element: el, kind: "card", toolGroup: null });
      }
    },
  });
  view.attach(total);
  const read = vi.fn(async (): Promise<TurnsResult> => ({ available: true, from: 0, end: total * 100, turns: Array.from({ length: TURNS }, (_, k) => turn(k)) }));
  const fold = new TurnFold(content, scrollEl, () => ({ origin: "local" as never, jsonlPath: "/p/s.jsonl" }), read, () => view);
  await fold.refresh();
  /** 一次滚动事件那么多：骨架物化一趟，等 DOM 变动回调排完版（真窗口里两次滚动之间就是这个样子）。 */
  const settle = async (): Promise<void> => {
    view.fillVisible();
    await new Promise((r) => setTimeout(r, 0));
  };
  const scrollHeight = (): number => Array.from(content.children).reduce((a, c) => a + ((c as HTMLElement).classList.contains("proc-hidden") ? 0 : parseFloat((c as HTMLElement).style.height || "0") || Number((c as HTMLElement).dataset.h ?? 0)), 0);
  return { content, view, layout, fold, settle, scrollHeight };
}

/** 每块占位：藏没藏 ⇔ 账本说它整块折着；落在某一轮过程里（不含结尾那一行）的，那一轮折着 ⇒ 藏、展开 ⇒ 露。 */
function misfolded(content: HTMLElement, view: SkeletonView, open: ReadonlySet<number>): string[] {
  const bad: string[] = [];
  const folded = view.ledger.foldedRanges;
  const coveredByLedger = (lo: number, hi: number): boolean => folded.some(([a, b]) => a <= lo && hi <= b);
  for (const g of content.querySelectorAll<HTMLElement>(`.${SKELETON_GAP_CLASS}`)) {
    const lo = Number(g.dataset.skeletonLo);
    const hi = Number(g.dataset.skeletonHi);
    const hidden = g.classList.contains("proc-hidden");
    if (hidden !== coveredByLedger(lo, hi)) bad.push(`${lo}-${hi} class ${hidden ? "藏" : "露"}、账本说${hidden ? "没折" : "折着"}`);
    const k = Math.floor(lo / PER_TURN);
    const e = k * PER_TURN + ENDING_AT;
    const inProc = lo > k * PER_TURN && hi <= (k + 1) * PER_TURN && !(lo <= e && e < hi);
    if (!inProc) continue;
    if (!open.has(k) && !hidden) bad.push(`${lo}-${hi} 折着却没藏`);
    if (open.has(k) && hidden) bad.push(`${lo}-${hi} 展开了还藏着`);
  }
  return bad;
}

const lineOf = (content: HTMLElement, k: number): HTMLElement =>
  content.querySelector<HTMLElement>(`.${PROC_LINE_CLASS}[data-turn="u${k * PER_TURN}"]`)!;

describe("大折叠 × 骨架：折没折只有一个来源", () => {
  it("滚动物化之后、展开别的轮、展开再折回去：折着的轮的过程占位都带 proc-hidden，展开的都不带", async () => {
    const { content, layout, settle, scrollHeight, view } = await rig();
    // 从底往上一屏一屏地滚（同读数那张图）。
    layout.scrollTop = scrollHeight();
    await settle();
    for (let k = 0; k < 40 && layout.scrollTop > 0; k++) {
      layout.scrollTop = Math.max(0, layout.scrollTop - VIEW_H);
      await settle();
    }
    expect(content.querySelectorAll(`.${PROC_LINE_CLASS}`).length).toBeGreaterThan(2);
    expect(misfolded(content, view, new Set())).toEqual([]);
    lineOf(content, 1).click();
    await settle();
    expect(misfolded(content, view, new Set([1]))).toEqual([]);
    lineOf(content, 1).click();
    await settle();
    expect(misfolded(content, view, new Set())).toEqual([]);
    lineOf(content, 2).click();
    await settle();
    lineOf(content, 2).click();
    await settle();
    expect(misfolded(content, view, new Set())).toEqual([]);
  });

  it("占位前面夹着别的东西（ESC 回退段）：顺着 DOM 认不出它归哪一轮，也照账本那一段认 —— 折着就藏", async () => {
    const { content, layout, settle, view } = await rig();
    layout.scrollTop = 0;
    await settle();
    expect(misfolded(content, view, new Set())).toEqual([]);
    const wrap = document.createElement("div");
    wrap.className = "branch-fold-wrap";
    lineOf(content, 1).after(wrap);
    await new Promise((r) => setTimeout(r, 0));
    expect(misfolded(content, view, new Set())).toEqual([]);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 冷切停住那一帧（接骨架、补可见区）：轮早就到了、骨架后到 —— 这是冷切进一个长会话的常态。
// 修前那一帧里：占位先按「没折」的高插进去（排一次版）→ DOM 变动回调排版 → 量过程行高（又排一次版）→ 账本换成折后的高 →
// 占位改高、按视口钉（再排一次版，还从流顶一张张量到视口）。修后：接之前先把折叠交给账本（`seedFolds`），占位插进去就是折后的高；
// 过程行高量过一次就记着（流尺寸变了才重量）⇒ 之后那一趟排版一下几何都不读、占位一下不改高。
// ═══════════════════════════════════════════════════════════════════════
describe("冷切停住那一帧：骨架后到", () => {
  /** 尾巴（最后 `TAIL_TURNS` 轮）已经建成卡、轮已经到了，骨架还没接（`view` 之后才建）。几何同上，过程行按 40 高。 */
  async function lateRig() {
    const TAIL_TURNS = 5;
    const scrollEl = document.createElement("div");
    document.body.replaceChildren(scrollEl);
    const stream = new MessageStream(scrollEl);
    const timeline = new RecordTimeline(stream);
    const content = stream.contentElement;
    const layout = installLayout(scrollEl, content, 40);
    const total = TURNS * PER_TURN;
    const floor = (TURNS - TAIL_TURNS) * PER_TURN;
    const card = (s: number): void => {
      const el = document.createElement("div");
      el.dataset.h = "60";
      el.dataset.seq = String(s);
      el.setAttribute("data-id", `u${s}`);
      timeline.insert({ seq: s, element: el, kind: "card", toolGroup: null });
    };
    for (let s = floor; s < total; s++) card(s);
    let view: SkeletonView | null = null;
    const read = vi.fn(async (): Promise<TurnsResult> => ({ available: true, from: 0, end: total * 100, turns: Array.from({ length: TURNS }, (_, k) => turn(k)) }));
    const fold = new TurnFold(content, scrollEl, () => ({ origin: "local" as never, jsonlPath: "/p/s.jsonl" }), read, () => view);
    await fold.refresh();
    layout.scrollTop = 1e9; // 贴底（尾巴那一屏）
    const ledger = new SkeletonLedger(0, Array.from({ length: total }, (_, i) => facts(i)));
    const mkView = (): SkeletonView => (view = new SkeletonView(ledger, scrollEl, timeline, { materialize: (lo, hi) => { for (let s = lo; s < hi; s++) card(s); } }));
    return { content, scrollEl, fold, ledger, mkView, floor };
  }

  it("★ 接之前先交折叠：占位插进去就是折后的高；之后那一趟排版不改占位高、不读一下几何", async () => {
    const { content, scrollEl, fold, ledger, mkView, floor } = await lateRig();
    expect(content.querySelectorAll(`.${PROC_LINE_CLASS}`).length, "前置：尾巴那几轮的过程行已经画出来").toBe(5);
    const view = mkView();
    fold.seedFolds(ledger);
    view.attach(floor);
    const gap = content.querySelector<HTMLElement>(`.${SKELETON_GAP_CLASS}`)!;
    const h0 = gap.style.height;
    const reads: string[] = [];
    const orig = HTMLElement.prototype.getBoundingClientRect;
    HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      reads.push(`getBoundingClientRect(${this.className || this.tagName})`);
      return orig.call(this);
    };
    // 视口高也是一下几何读（同样逼当场排版）
    Object.defineProperty(scrollEl, "clientHeight", { get: () => (reads.push("clientHeight"), VIEW_H), configurable: true });
    try {
      await new Promise((r) => setTimeout(r, 0)); // DOM 变动回调排版
    } finally {
      HTMLElement.prototype.getBoundingClientRect = orig;
      delete (scrollEl as { clientHeight?: number }).clientHeight;
    }
    expect(view.ledger.foldedRanges.length, "前置：账本里有折着的过程").toBeGreaterThan(0);
    expect(gap.style.height, "占位插进去之后又改了高 ⇒ 插的时候账本还没拿到折叠（这一帧多排一次版、多钉一次视口）").toBe(h0);
    expect(reads, "占位插进去之后的那一趟排版还在读几何（量过程行高 / 钉视口 / 一轮都没展开也量视口高）⇒ 逼浏览器当场再排一次版").toEqual([]);
  });

  it("★ 过程行高量过一次就记着：之后每趟排版不再量（流尺寸变了才重量）", async () => {
    const { content, fold, mkView, floor } = await lateRig();
    mkView().attach(floor);
    await new Promise((r) => setTimeout(r, 0));
    const line = content.querySelector<HTMLElement>(`.${PROC_LINE_CLASS}`)!;
    const spy = vi.spyOn(line, "getBoundingClientRect");
    fold.apply();
    fold.apply();
    expect(spy.mock.calls.length, "每趟排版都去量一次过程行（DOM 刚改过 ⇒ 每一下都是一次强制排版）").toBeLessThanOrEqual(1);
  });

  it("★ 过程行上的字没变就不重画：再排一次版，行里的节点原样不动（不白白作废样式与排版）", async () => {
    const { content, fold } = await lateRig();
    const before = [...content.querySelectorAll<HTMLElement>(`.${PROC_LINE_CLASS}`)].map((l) => l.firstChild);
    expect(before.length, "前置：有过程行").toBeGreaterThan(0);
    fold.apply();
    const after = [...content.querySelectorAll<HTMLElement>(`.${PROC_LINE_CLASS}`)].map((l) => l.firstChild);
    expect(after.every((n, i) => n === before[i]), "字一个没变也整行拆了重建 ⇒ 每趟排版都作废这几行的样式与排版").toBe(true);
  });
});
