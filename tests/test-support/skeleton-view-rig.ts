/**
 * 骨架层判据共用的台子（`tests/frontend/ui/skeleton-view*.vitest.ts`）：一维布局模型 · 宿主替身 · 建一份骨架。
 * jsdom 没有布局引擎，所有 rect 恒 0 ⇒ 几何由这里的一维累加给。
 */
import { onTestFinished } from "vitest";
import type { SkeletonFacts } from "../../src/frontend/ui/height-estimate";
import { SkeletonLedger } from "../../src/frontend/ui/live-window";
import { RecordTimeline } from "../../src/frontend/ui/record-timeline";
import { SKELETON_GAP_CLASS, SkeletonView } from "../../src/frontend/ui/skeleton-view";
import { MessageStream } from "../../src/frontend/ui/stream";
import { indexPages } from "./pack-index";

// jsdom 没有 `ResizeObserver`，`MessageStream` 建的时候就要它；导入这一份即装上一个什么都不做的。
globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

export const VIEW_H = 800;

/**
 * 一维布局模型：`.stream-content` 的子元素自上而下堆叠，高度 = 占位的内联 `height` /
 * 卡的 `data-h`；`scrollEl` 的 rect 恒为 `[0, VIEW_H]`，子元素的 rect 减去 `scrollTop`。
 * 换的是 `HTMLElement.prototype.getBoundingClientRect`，这条用例跑完换回去（不留给下一条叠一层）。
 */
export function installLayout(scrollEl: HTMLElement, content: HTMLElement): { scrollTop: number } {
  const state = { scrollTop: 0 };
  Object.defineProperty(scrollEl, "scrollTop", {
    get: () => state.scrollTop,
    set: (v: number) => {
      state.scrollTop = v;
    },
    configurable: true,
  });
  const heightOf = (el: Element): number => {
    const h = (el as HTMLElement).style.height;
    if (h) return parseFloat(h);
    return Number((el as HTMLElement).dataset.h ?? 0);
  };
  const rect = (top: number, h: number): DOMRect =>
    ({ top, bottom: top + h, height: h, left: 0, right: 100, width: 100, x: 0, y: top }) as DOMRect;
  scrollEl.getBoundingClientRect = () => rect(0, VIEW_H);
  const orig = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement): DOMRect {
    if (this.parentElement !== content) return orig.call(this);
    let y = -state.scrollTop;
    for (const c of Array.from(content.children)) {
      const h = heightOf(c);
      if (c === this) return rect(y, h);
      y += h;
    }
    return rect(0, 0);
  };
  onTestFinished(() => {
    HTMLElement.prototype.getBoundingClientRect = orig;
  });
  return state;
}

export const asst = (): SkeletonFacts => ({ o: 0, n: 1, t: "reply", dr: "card", ch: 200, pl: 2 });

/** 宿主：建卡 = 往 timeline 里插一个带 `data-h`（真高）的 div；记下每次被要了哪一段。 */
export function makeHost(timeline: RecordTimeline, realH: (seq: number) => number, builds = (_s: number) => true) {
  const calls: Array<[number, number]> = [];
  const gaveBack: Array<readonly [number, number]> = [];
  return {
    calls,
    gaveBack,
    materialize(lo: number, hi: number): Array<[number, number]> {
      calls.push([lo, hi]);
      buildCards(timeline, lo, hi, realH, builds);
      return [];
    },
    released(ranges: ReadonlyArray<readonly [number, number]>): void {
      gaveBack.push(...ranges);
    },
  };
}

function buildCards(timeline: RecordTimeline, lo: number, hi: number, realH: (seq: number) => number, builds = (_s: number) => true): void {
  for (let s = lo; s < hi; s++) {
    if (!builds(s)) continue;
    const el = document.createElement("div");
    el.dataset.h = String(realH(s));
    el.dataset.seq = String(s);
    timeline.insert({ seq: s, element: el, kind: "card" });
  }
}

/** 正文本地一行都没有的宿主（骨架接上后账本只留尾巴）：交来的每一段都要去取 ⇒ 整段报「取回中」，落地由测试手动放。 */
export function makeFetchingHost(timeline: RecordTimeline, realH: (seq: number) => number) {
  const calls: Array<[number, number]> = [];
  return {
    calls,
    materialize(lo: number, hi: number): Array<[number, number]> {
      calls.push([lo, hi]);
      return [[lo, hi]];
    },
    build: (lo: number, hi: number) => buildCards(timeline, lo, hi, realH),
    released(_ranges: ReadonlyArray<readonly [number, number]>): void {},
  };
}

export function setup(total: number, renderedFrom: number, realH = (_s: number) => 60, rows?: SkeletonFacts[], builds?: (s: number) => boolean) {
  const scrollEl = document.createElement("div");
  document.body.appendChild(scrollEl);
  const stream = new MessageStream(scrollEl);
  const timeline = new RecordTimeline(stream);
  const content = stream.contentElement;
  const layout = installLayout(scrollEl, content);
  const ledger = new SkeletonLedger(
    0,
    indexPages(rows ?? Array.from({ length: total }, () => asst())),
  );
  const host = makeHost(timeline, realH, builds);
  // 已渲染的尾巴
  host.materialize(renderedFrom, total);
  host.calls.length = 0;
  const view = new SkeletonView(ledger, scrollEl, timeline, host);
  return { scrollEl, content, timeline, ledger, host, view, layout };
}

export const cardsIn = (content: HTMLElement) =>
  Array.from(content.children).filter((c) => !c.classList.contains(SKELETON_GAP_CLASS));
