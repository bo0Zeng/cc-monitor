/**
 * 〔`设计/10` 骨架 · 子步 4〕骨架层的判据 —— **真 `MessageStream` ＋ 真 `RecordTimeline`**，
 * 几何由下面那个极小的布局模型给（jsdom 没有布局引擎，所有 rect 恒 0）。
 *
 * 🔴 主判据是「**只物化可见区**」那一组：骨架快的主语是它，不是占位本身
 * （先全量渲染再盖一层骨架 = 假骨架，那一组会红）。
 *
 * 买到：占位总高 = 账本估高 · 滚到中部只建视口 ±0.5 屏那几行 · 视口里有已渲染卡时钉住它不跳 ·
 * 视口整个在占位里时不补偿 · 岛的左邻居是占位（工具组不跨空洞合并）· 没布局时什么都不做。
 * **买不到**：真引擎里的像素（布局模型是一维的累加，没有 margin 折叠 / content-visibility）；
 * 秤 3（半屏）那种端到端读数得在真 webview 里打。
 */
import { beforeEach, describe, expect, it } from "vitest";
import type { SkeletonFacts } from "../src/height-estimate";
import { SkeletonLedger } from "../src/live-window";
import { RecordTimeline } from "../src/record-timeline";
import { SKELETON_GAP_CLASS, SkeletonView, ledgerFromIndex } from "../src/skeleton-view";
import { MessageStream } from "../src/stream";

globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

const VIEW_H = 800;

/**
 * 一维布局模型：`.stream-content` 的子元素自上而下堆叠，高度 = 占位的内联 `height` /
 * 卡的 `data-h`；`scrollEl` 的 rect 恒为 `[0, VIEW_H]`，子元素的 rect 减去 `scrollTop`。
 */
function installLayout(scrollEl: HTMLElement, content: HTMLElement): { scrollTop: number } {
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
  return state;
}

const asst = (u: string): SkeletonFacts => ({ o: 0, n: 1, t: "assistant", u, ch: 200, pl: 2 });

/** 宿主：建卡 = 往 timeline 里插一个带 `data-h`（真高）的 div；记下每次被要了哪一段。 */
function makeHost(timeline: RecordTimeline, realH: (seq: number) => number) {
  const calls: Array<[number, number]> = [];
  return {
    calls,
    materialize(lo: number, hi: number): void {
      calls.push([lo, hi]);
      for (let s = lo; s < hi; s++) {
        const el = document.createElement("div");
        el.dataset.h = String(realH(s));
        el.dataset.seq = String(s);
        timeline.insert({ seq: s, element: el, kind: "card", toolGroup: null });
      }
    },
  };
}

function setup(total: number, renderedFrom: number, realH = (_s: number) => 60) {
  const scrollEl = document.createElement("div");
  document.body.appendChild(scrollEl);
  const stream = new MessageStream(scrollEl);
  const timeline = new RecordTimeline(stream);
  const content = stream.contentElement;
  const layout = installLayout(scrollEl, content);
  const ledger = new SkeletonLedger(
    0,
    Array.from({ length: total }, (_, i) => asst(`u${i}`)),
  );
  const host = makeHost(timeline, realH);
  // 已渲染的尾巴
  host.materialize(renderedFrom, total);
  host.calls.length = 0;
  const view = new SkeletonView(ledger, scrollEl, timeline, host);
  return { scrollEl, content, timeline, ledger, host, view, layout };
}

const cardsIn = (content: HTMLElement) =>
  Array.from(content.children).filter((c) => !c.classList.contains(SKELETON_GAP_CLASS));

describe("骨架：占位与总高", () => {
  it("attach 画出 [0, renderedFrom) 一块占位，高 = 账本估高，排在已渲染卡之前", () => {
    const { view, ledger, content } = setup(5000, 4900);
    const h = view.attach(4900);
    expect(h).toBeCloseTo(ledger.heightOf(0, 4900));
    const first = content.firstElementChild as HTMLElement;
    expect(first.classList.contains(SKELETON_GAP_CLASS)).toBe(true);
    expect(parseFloat(first.style.height)).toBeCloseTo(h);
    expect(view.pendingRows).toBe(4900);
    expect(view.isPending(0) && view.isPending(4899)).toBe(true);
    expect(view.isPending(4900)).toBe(false);
  });

  it("renderedFrom ≤ base ⇒ 没什么要顶的，不建占位", () => {
    const { view, content } = setup(10, 0);
    expect(view.attach(0)).toBe(0);
    expect(content.querySelector(`.${SKELETON_GAP_CLASS}`)).toBeNull();
  });
});

describe("🔴 只物化可见区", () => {
  let s: ReturnType<typeof setup>;
  beforeEach(() => {
    s = setup(5000, 4900);
    s.view.attach(4900);
  });

  it("滚到占位中部：只建视口 ±0.5 屏覆盖的那几行，远处一张都不建", () => {
    const gapH = s.ledger.heightOf(0, 4900);
    s.layout.scrollTop = gapH / 2;
    const n = s.view.fillVisible();
    const rowH = s.ledger.heightOf(0, 1);
    // 视口 800 ± 400 = 1600px 的估高行数（+2 条边界）是**一轮**的上限；真高 60 < 估高 ⇒ 可能多跑几轮
    const perPass = Math.ceil((VIEW_H * 2) / rowH) + 2;
    expect(n).toBeGreaterThan(0);
    expect(n).toBeLessThanOrEqual(perPass * 4);
    // 建的全在视口附近：中点那一行 ± 几屏
    const mid = s.ledger.seqAt(0, 4900, gapH / 2);
    for (const [lo, hi] of s.host.calls) {
      expect(lo).toBeGreaterThanOrEqual(mid - perPass * 4);
      expect(hi).toBeLessThanOrEqual(mid + perPass * 4);
    }
    // 🔴 远没有全量：已建卡 = 尾巴 100 + 这几行
    expect(cardsIn(s.content).length).toBe(100 + n);
    expect(s.view.pendingRows).toBe(4900 - n);
    // 骨架被切成上下两块，岛夹在中间
    expect(s.view.gapCount).toBe(2);
  });

  it("DOM 顺序：[占位 0..i) · 岛 i..j) · [占位 j..4900) · 尾巴]", () => {
    s.layout.scrollTop = s.ledger.heightOf(0, 4900) / 2;
    s.view.fillVisible();
    const kids = Array.from(s.content.children) as HTMLElement[];
    const gaps = kids.filter((k) => k.classList.contains(SKELETON_GAP_CLASS));
    expect(gaps.length).toBe(2);
    const i = Number(gaps[0].dataset.skeletonHi);
    const j = Number(gaps[1].dataset.skeletonLo);
    const g0 = kids.indexOf(gaps[0]);
    const g1 = kids.indexOf(gaps[1]);
    const island = kids.slice(g0 + 1, g1).map((k) => Number(k.dataset.seq));
    expect(island[0]).toBe(i);
    expect(island[island.length - 1]).toBe(j - 1);
    expect(Number(kids[g1 + 1].dataset.seq)).toBe(4900);
  });

  it("岛的第一行左邻居是占位（kind=card），不会并进空洞上方的工具组", () => {
    s.layout.scrollTop = s.ledger.heightOf(0, 4900) / 2;
    s.view.fillVisible();
    const gap0 = s.content.querySelector(`.${SKELETON_GAP_CLASS}`) as HTMLElement;
    const i = Number(gap0.dataset.skeletonHi);
    const prev = s.timeline.peekPrev(i);
    expect(prev?.element).toBe(gap0);
    expect(prev?.kind).toBe("card");
  });

  it("视口里有已渲染的卡：钉住它的屏幕位置（真高 ≠ 估高也不跳）", () => {
    // 让占位下沿停在视口中线：尾巴第一张卡在 y=400
    const gapH = s.ledger.heightOf(0, 4900);
    s.layout.scrollTop = gapH - VIEW_H / 2;
    const tailFirst = cardsIn(s.content)[0] as HTMLElement;
    const before = tailFirst.getBoundingClientRect().top;
    expect(before).toBeCloseTo(VIEW_H / 2);
    const n = s.view.fillVisible();
    expect(n).toBeGreaterThan(0);
    expect(tailFirst.getBoundingClientRect().top).toBeCloseTo(before);
  });

  it("视口整个在占位里：不补偿（上半块占位高不变 ⇒ 视口停在它估算的那一行）", () => {
    const gapH = s.ledger.heightOf(0, 4900);
    s.layout.scrollTop = gapH / 3;
    const before = s.layout.scrollTop;
    s.view.fillVisible();
    expect(s.layout.scrollTop).toBe(before);
  });

  it("收敛：真高 < 估高会让视口里再露出占位，几次之内补平，之后再滚同一处一行都不再要", () => {
    s.layout.scrollTop = s.ledger.heightOf(0, 4900) / 2;
    let rounds = 0;
    while (s.view.fillVisible() > 0) {
      rounds++;
      expect(rounds).toBeLessThan(6);
    }
    const calls = s.host.calls.length;
    expect(s.view.fillVisible()).toBe(0);
    expect(s.host.calls.length).toBe(calls);
  });

  it("没有布局（视口高 0）：什么都不做 —— 不猜", () => {
    s.scrollEl.getBoundingClientRect = () => ({ top: 0, bottom: 0, height: 0 }) as DOMRect;
    expect(s.view.fillVisible()).toBe(0);
    expect(s.host.calls.length).toBe(0);
  });
});

describe("跳转与续传", () => {
  it("ensure(seq)：不在视口里也把它 ± radius 物化（给「大纲」跳转用）", () => {
    const s = setup(1000, 900);
    s.view.attach(900);
    s.view.ensure(123, 5);
    expect(s.host.calls).toEqual([[118, 129]]);
    expect(s.view.isPending(123)).toBe(false);
    expect(s.view.isPending(117)).toBe(true);
  });

  it("续传：账本 append 之后 refreshHeights 把占位高补上", () => {
    const s = setup(10, 10);
    // 账本只有前 5 行，已渲染从 10 起 ⇒ [5,10) 的高先是 0
    const ledger = new SkeletonLedger(0, Array.from({ length: 5 }, (_, i) => asst(`a${i}`)));
    const view = new SkeletonView(ledger, s.scrollEl, s.timeline, s.host);
    const h0 = view.attach(10);
    expect(h0).toBeCloseTo(ledger.heightOf(0, 5));
    ledger.append(Array.from({ length: 5 }, (_, i) => asst(`b${i}`)));
    view.refreshHeights();
    expect(view.pendingHeight).toBeCloseTo(ledger.heightOf(0, 10));
    const gap = s.content.querySelector(`.${SKELETON_GAP_CLASS}`) as HTMLElement;
    expect(parseFloat(gap.style.height)).toBeCloseTo(ledger.heightOf(0, 10));
  });

  it("ledgerFromIndex：available=false ⇒ 带原因退回，不建账本", () => {
    const r = ledgerFromIndex({ available: false, reason: "老后端", end: 0, rows: [] });
    expect(r).toEqual({ ok: false, reason: "老后端" });
    const ok = ledgerFromIndex({ available: true, end: 9, rows: [asst("x")] });
    expect(ok.ok && ok.ledger.count === 1 && ok.end === 9).toBe(true);
  });
});
