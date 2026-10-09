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

/** 一维布局：子元素自上而下堆，高 = 占位内联高 / 卡的 data-h；`proc-hidden` 的按 0（display:none）。 */
function installLayout(scrollEl: HTMLElement, content: HTMLElement): { scrollTop: number } {
  const state = { scrollTop: 0 };
  Object.defineProperty(scrollEl, "scrollTop", { get: () => state.scrollTop, set: (v: number) => (state.scrollTop = v), configurable: true });
  const h = (el: Element): number => {
    const e = el as HTMLElement;
    if (e.classList.contains("proc-hidden")) return 0;
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

const facts = (i: number): SkeletonFacts => ({ o: i * 100, n: 100, t: i % PER_TURN === 0 ? "user" : "assistant", u: `u${i}`, ch: 120, pl: 2 });

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
        el.setAttribute("data-uuid", `u${s}`);
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
