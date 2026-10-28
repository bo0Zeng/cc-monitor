/**
 * 〔骨架〕骨架层的判据 —— **真 `MessageStream` ＋ 真 `RecordTimeline`**，
 * 几何由下面那个极小的布局模型给（jsdom 没有布局引擎，所有 rect 恒 0）。
 *
 * 🔴 主判据是「**只物化可见区**」那一组：骨架快的主语是它，不是占位本身
 * （先全量渲染再盖一层骨架 = 假骨架，那一组会红）。
 *
 * 买到：占位总高 = 账本估高 · 滚到中部只建视口 ±0.5 屏那几行 · 视口里有已渲染卡时钉住它不跳 ·
 * 视口整个在占位里时不补偿 · 岛的左邻居是占位（相邻合并不跨空洞）· 没布局时什么都不做。
 * **买不到**：真引擎里的像素（布局模型是一维的累加，没有 margin 折叠 / content-visibility）；
 * 秤 3（半屏）那种端到端读数得在真 webview 里打。
 */
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { estimateFromFacts, initialColumnWidth, type SkeletonFacts } from "../../../src/frontend/ui/height-estimate";
import { SkeletonLedger } from "../../../src/frontend/ui/live-window";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";
import { SKELETON_GAP_CLASS, SkeletonView, ledgerFromIndex } from "../../../src/frontend/ui/skeleton-view";
import { MessageStream } from "../../../src/frontend/ui/stream";
import { nonEmpty } from "../../test-support/non-empty";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";
import { indexPages } from "../../test-support/pack-index";

// 列宽那一组要真 `TabStreamView`（它只在被问时才碰到 IPC；这里一次都不问）
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn() }));

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

const asst = (): SkeletonFacts => ({ o: 0, n: 1, t: "reply", dr: "card", ch: 200, pl: 2 });

/** 宿主：建卡 = 往 timeline 里插一个带 `data-h`（真高）的 div；记下每次被要了哪一段。 */
function makeHost(timeline: RecordTimeline, realH: (seq: number) => number, builds = (_s: number) => true) {
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
function makeFetchingHost(timeline: RecordTimeline, realH: (seq: number) => number) {
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

function setup(total: number, renderedFrom: number, realH = (_s: number) => 60, rows?: SkeletonFacts[], builds?: (s: number) => boolean) {
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
    expect(n, "视口与行高都定死 ⇒ 建的行数是定的").toBe(25);
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

  it("岛的第一行左邻居是占位（kind=card），不会并进空洞上方的那一张", () => {
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
    expect(n, "视口与行高都定死 ⇒ 建的行数是定的").toBe(12);
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

describe("🔴 视口外的卡交回占位", () => {
  /** 流里此刻的总高（一维布局模型：占位的内联高 ＋ 卡的真高）。 */
  const totalOf = (content: HTMLElement) =>
    Array.from(content.children).reduce((a, c) => a + (parseFloat((c as HTMLElement).style.height) || Number((c as HTMLElement).dataset.h ?? 0)), 0);
  /** 视口里那几张卡：seq 与屏幕上的位置。 */
  const inView = (content: HTMLElement) =>
    cardsIn(content)
      .map((c) => [Number((c as HTMLElement).dataset.seq), c.getBoundingClientRect()] as const)
      .filter(([, r]) => r.height > 0 && r.bottom > 0 && r.top < VIEW_H)
      .map(([seq, r]) => [seq, r.top]);
  /** 滚一下：补可见区补到收敛，建了卡就交回远处的（宿主滚动监听里就是这两步）；交回前后总高不许变。 */
  const scrollTo = (s: ReturnType<typeof setup>, y: number) => {
    s.layout.scrollTop = Math.max(0, y);
    let built = 0;
    for (let k = 0, n = 1; k < 6 && n > 0; k++) built += n = s.view.fillVisible();
    if (built === 0) return;
    const h = totalOf(s.content);
    s.view.release();
    expect(totalOf(s.content), "交回占位改了总高").toBeCloseTo(h, 6);
  };
  const realH = (q: number) => 40 + (q % 7) * 10; // 真高与估高对不上、行行不同
  /** 上界：视口 ± 留着的屏数 ＋ 补可见区的余量（各半屏）里最多装几张卡，再加两头各一张没切开的。 */
  const bound = (screens: number) => Math.ceil(((2 * screens + 2) * VIEW_H) / 40) + 2;
  /** 从底一屏一屏滚到顶（`dir` −1）或从顶滚到底（+1）；每一步已建卡数都不许过上界。 */
  const sweep = (s: ReturnType<typeof setup>, dir: -1 | 1) => {
    for (let k = 0; k < 2000; k++) {
      const bottom = totalOf(s.content) - VIEW_H;
      const y = dir < 0 ? s.layout.scrollTop - VIEW_H * 0.9 : Math.min(bottom, s.layout.scrollTop + VIEW_H * 0.9);
      scrollTo(s, y);
      expect(cardsIn(s.content).length, `滚到 ${Math.round(y)}`).toBeLessThanOrEqual(bound(SkeletonView.KEEP_SCREENS));
      if (dir < 0 ? s.layout.scrollTop <= 0 : s.layout.scrollTop >= totalOf(s.content) - VIEW_H - 0.5) return;
    }
    throw new Error("滚不到头");
  };

  it("从底滚到顶再滚回底：已建卡数一路不过上界，交回的那几行回到占位里", () => {
    const s = setup(600, 550, realH);
    s.view.attach(550);
    scrollTo(s, totalOf(s.content) - VIEW_H);
    sweep(s, -1);
    sweep(s, 1);
    expect(s.host.gaveBack.length).toBeGreaterThan(0);
    for (const [lo, hi] of s.host.gaveBack.slice(-3)) expect(s.view.isPending(lo) || s.view.isPending(hi - 1) || cardsIn(s.content).some((c) => Number((c as HTMLElement).dataset.seq) === lo)).toBe(true);
  });

  it("滚回来内容与高度不变：同一处视口里还是那几张卡、在同一个位置；再走一趟总高一点不动", () => {
    const s = setup(600, 550, realH);
    s.view.attach(550);
    scrollTo(s, totalOf(s.content) - VIEW_H);
    sweep(s, -1);
    sweep(s, 1); // 走过一趟：每一行都建过、量过
    const y = s.layout.scrollTop;
    const seen = inView(s.content);
    const h = totalOf(s.content);
    sweep(s, -1);
    expect(totalOf(s.content), "滚到顶：总高变了").toBeCloseTo(h, 6);
    sweep(s, 1);
    expect(totalOf(s.content), "滚回底：总高变了").toBeCloseTo(h, 6);
    expect(s.layout.scrollTop).toBeCloseTo(y, 6);
    expect(inView(s.content)).toEqual(seen);
  });

  it("只在轮与回答的开头切：工具调用与它的结果、合进左邻居的那几行不会被切开", () => {
    // 一轮六行：人说 · 回答（带正文）· 调用 · 结果（注入进调用那一行，不建卡）· 调用 · 结果
    const shape = (q: number): SkeletonFacts => {
      switch (q % 6) {
        case 0:
          return { o: 0, n: 1, t: "said", dr: "card", hu: true, ch: 80, pl: 1 };
        case 1:
          return { o: 0, n: 1, t: "reply", dr: "card", ch: 200, pl: 2 };
        case 2:
        case 4:
          return { o: 0, n: 1, t: "reply", dr: "steps", fd: 1 };
        default:
          return { o: 0, n: 1, t: "said", dr: "steps", fd: 1 };
      }
    };
    const rows = Array.from({ length: 1200 }, (_, q) => shape(q));
    const cut = (q: number) => q % 6 === 0 || q % 6 === 1;
    const s = setup(1200, 1140, realH, rows, (q) => q % 6 !== 3 && q % 6 !== 5);
    s.view.attach(1140);
    scrollTo(s, totalOf(s.content) - VIEW_H);
    sweep(s, -1);
    expect(s.host.gaveBack.length).toBeGreaterThan(0);
    for (const [lo, hi] of s.host.gaveBack) {
      expect(cut(lo), `从第 ${lo} 行切`).toBe(true);
      expect(cut(hi), `切到第 ${hi} 行`).toBe(true);
    }
  });

  it("一下跳到顶、正文还在路上（已建的全在视口下方远处）：底下那一截照样交回", () => {
    const s = setup(600, 550, realH);
    const host = { ...makeFetchingHost(s.timeline, realH), gaveBack: [] as Array<readonly [number, number]> };
    host.released = (r: ReadonlyArray<readonly [number, number]>) => void host.gaveBack.push(...r);
    const view = new SkeletonView(s.ledger, s.scrollEl, s.timeline, host);
    view.attach(550);
    s.layout.scrollTop = 0;
    view.fillVisible();
    expect(cardsIn(s.content).length).toBe(50);
    view.release();
    expect(host.gaveBack.length).toBe(1);
    expect(cardsIn(s.content).length).toBeLessThanOrEqual(1);
  });

  it("收起时一次做（只留视口那一截）", () => {
    const s = setup(600, 550, realH);
    s.view.attach(550);
    scrollTo(s, totalOf(s.content) - VIEW_H);
    sweep(s, -1);
    scrollTo(s, totalOf(s.content) / 2);
    const seen = inView(s.content);
    const h = totalOf(s.content);
    s.view.release(0);
    expect(totalOf(s.content)).toBeCloseTo(h, 6);
    expect(inView(s.content)).toEqual(seen);
    expect(cardsIn(s.content).length).toBeLessThanOrEqual(bound(0));
  });

  it("没有布局（视口高 0）：一张都不交 —— 不猜", () => {
    const s = setup(500, 0, realH);
    s.view.attach(0);
    s.scrollEl.getBoundingClientRect = () => ({ top: 0, bottom: 0, height: 0 }) as DOMRect;
    expect(s.view.release(0)).toBe(0);
    expect(cardsIn(s.content).length).toBe(500);
  });
});

describe("🔴 取回中的行留着占位（10-10「视口里的占位一直不填」）", () => {
  function fetching(realH = (_s: number) => 60) {
    const s = setup(5000, 4900, realH);
    const host = makeFetchingHost(s.timeline, realH);
    const view = new SkeletonView(s.ledger, s.scrollEl, s.timeline, host);
    view.attach(4900);
    const total = () => Array.from(s.content.children).reduce((a, c) => a + (parseFloat((c as HTMLElement).style.height) || Number((c as HTMLElement).dataset.h ?? 0)), 0);
    const inView = () =>
      cardsIn(s.content).filter((c) => {
        const r = c.getBoundingClientRect();
        return r.height > 0 && r.bottom > 0 && r.top < VIEW_H;
      });
    return { ...s, host, view, total, inView };
  }

  it("落在占位中部：一次只要视口那一段；占位高不撤（总高不变）、视口没被带到后面去", () => {
    const f = fetching();
    const gapH = f.ledger.heightOf(0, 4900);
    f.layout.scrollTop = gapH / 2;
    const h0 = f.total();
    const mid = f.ledger.seqAt(0, 4900, gapH / 2);
    f.view.fillVisible();
    expect(f.host.calls.length).toBe(1);
    const [lo, hi] = f.host.calls[0];
    const rowH = f.ledger.heightOf(0, 1);
    expect(hi - lo).toBeLessThanOrEqual(Math.ceil((VIEW_H * 2) / rowH) + 2);
    expect(lo).toBeLessThanOrEqual(mid);
    expect(hi).toBeGreaterThan(mid);
    expect(f.total()).toBeCloseTo(h0);
    expect(f.layout.scrollTop).toBe(gapH / 2);
  });

  it("取回在途：再来 scroll / resize 也不再要（取回次数有上限），这几行不算还在占位里", () => {
    const f = fetching();
    f.layout.scrollTop = f.ledger.heightOf(0, 4900) / 2;
    f.view.fillVisible();
    const [lo, hi] = f.host.calls[0];
    for (let k = 0; k < 20; k++) f.view.fillVisible();
    expect(f.host.calls.length).toBe(1);
    for (let q = lo; q < hi; q++) expect(f.view.isPending(q)).toBe(false);
    expect(f.view.isPending(lo - 1)).toBe(true);
  });

  it("正文落地：同一个任务里占位换成卡，视口里出现真卡、不跳", () => {
    const f = fetching();
    const gapH = f.ledger.heightOf(0, 4900);
    f.layout.scrollTop = gapH / 2;
    f.view.fillVisible();
    const [lo, hi] = f.host.calls[0];
    const block = f.content.querySelector<HTMLElement>(`.${SKELETON_GAP_CLASS}[data-skeleton-lo="${lo}"]`)!;
    const blockTop = block.getBoundingClientRect().top;
    f.view.landed(lo, hi, () => f.host.build(lo, hi));
    expect(block.isConnected).toBe(false);
    nonEmpty(f.inView());
    const first = cardsIn(f.content).find((c) => (c as HTMLElement).dataset.seq === String(lo))!;
    expect(first.getBoundingClientRect().top).toBeCloseTo(blockTop);
    expect(f.layout.scrollTop).toBe(gapH / 2);
  });

  it("取不到：那一段放回占位，下次滚到再要", () => {
    const f = fetching();
    f.layout.scrollTop = f.ledger.heightOf(0, 4900) / 2;
    f.view.fillVisible();
    const [lo, hi] = f.host.calls[0];
    f.view.failed(lo, hi);
    expect(f.view.isPending(lo) && f.view.isPending(hi - 1)).toBe(true);
    expect(f.view.gapCount).toBe(1);
    f.view.fillVisible();
    expect(f.host.calls.length).toBe(2);
  });
});

describe("大折叠：折着的过程不物化", () => {
  it("账本说 [lo, hi) 折着 ⇒ 滚到那里只建它两边的行，它留成 0 高的占位；ensure（跳转）照样建", () => {
    const s = setup(200, 190);
    s.view.attach(190);
    s.view.setFolds({ folded: [[3, 150]], lines: [], linePx: 0 });
    s.layout.scrollTop = 0;
    s.view.fillVisible();
    const built = new Set(cardsIn(s.content).map((c) => Number((c as HTMLElement).dataset.seq)));
    for (let q = 3; q < 150; q++) expect(built.has(q)).toBe(false);
    expect([0, 1, 2, 150].every((q) => built.has(q))).toBe(true);
    const folded = [...s.content.querySelectorAll<HTMLElement>(`.${SKELETON_GAP_CLASS}`)].find((g) => g.dataset.skeletonLo === "3");
    expect(folded?.dataset.skeletonHi).toBe("150");
    expect(parseFloat(folded!.style.height)).toBe(0);
    s.view.ensure(80, 2);
    expect(cardsIn(s.content).some((c) => (c as HTMLElement).dataset.seq === "80")).toBe(true);
  });

  it("只剩不占高的行（工具结果注入进调用那一行那种）的占位落在视口里 ⇒ 照样物化（不因高 0 永远留着）；扣掉折着的那几段", () => {
    const s = setup(30, 20);
    const rows = Array.from({ length: 30 }, (_, i): SkeletonFacts => (i === 11 ? { o: 0, n: 1, t: "said", dr: "steps", fd: 1 } : i === 10 ? { o: 0, n: 1, t: "reply", dr: "steps", fd: 1 } : asst()));
    const ledger = new SkeletonLedger(0, indexPages(rows));
    const view = new SkeletonView(ledger, s.scrollEl, s.timeline, s.host);
    view.attach(20);
    expect(ledger.heightOf(11, 12)).toBe(0);
    view.setFolds({ folded: [[11, 12]], lines: [], linePx: 0 });
    s.layout.scrollTop = 0;
    view.fillVisible();
    const built = () => new Set(cardsIn(s.content).map((c) => Number((c as HTMLElement).dataset.seq)));
    expect(built().has(11)).toBe(false);
    view.setFolds({ folded: [], lines: [], linePx: 0 });
    view.fillVisible();
    expect(built().has(11)).toBe(true);
  });

  it("setFolds 改了 ⇒ 占位改高（展开那一段 ⇒ 回到估高），视口钉住", () => {
    const s = setup(200, 190);
    s.view.attach(190);
    const h0 = s.view.pendingHeight;
    s.view.setFolds({ folded: [[20, 150]], lines: [], linePx: 0 });
    expect(s.view.pendingHeight).toBeCloseTo(s.ledger.heightOf(0, 190));
    expect(s.view.pendingHeight).toBeLessThan(h0);
    s.view.setFolds({ folded: [], lines: [], linePx: 0 });
    expect(s.view.pendingHeight).toBeCloseTo(h0);
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
    const ledger = new SkeletonLedger(0, indexPages(Array.from({ length: 5 }, () => asst())));
    const view = new SkeletonView(ledger, s.scrollEl, s.timeline, s.host);
    const h0 = view.attach(10);
    expect(h0).toBeCloseTo(ledger.heightOf(0, 5));
    ledger.append(indexPages(Array.from({ length: 5 }, () => asst())));
    view.refreshHeights();
    expect(view.pendingHeight).toBeCloseTo(ledger.heightOf(0, 10));
    const gap = s.content.querySelector(`.${SKELETON_GAP_CLASS}`) as HTMLElement;
    expect(parseFloat(gap.style.height)).toBeCloseTo(ledger.heightOf(0, 10));
  });

  it("ledgerFromIndex：available=false ⇒ 带原因退回，不建账本", () => {
    const r = ledgerFromIndex({ available: false, reason: "老后端", end: 0, pages: [] });
    expect(r).toEqual({ ok: false, reason: "老后端" });
    const ok = ledgerFromIndex({ available: true, end: 9, pages: indexPages([asst()]) });
    expect(ok.ok && ok.ledger.count === 1 && ok.end === 9).toBe(true);
  });
});

describe("〔U3b〕attachGaps（查看器：已渲染集不是后缀）", () => {
  it("占位同时插在视口那张卡的上方和下方 ⇒ 钉住它的屏幕位置（ΔscrollHeight 补偿在这里是错的）", () => {
    // 已渲染：岛 [100,110) ＋ 尾巴 [4900,5000)；视口停在岛上
    const s = setup(5000, 4900);
    s.host.materialize(100, 110);
    s.host.calls.length = 0;
    s.layout.scrollTop = 0; // 岛第一张卡此刻在 y=0
    const islandFirst = cardsIn(s.content)[0] as HTMLElement;
    expect(islandFirst.dataset.seq).toBe("100");
    const before = islandFirst.getBoundingClientRect().top;
    const h = s.view.attachGaps([
      [0, 100],
      [110, 4900],
    ]);
    expect(h).toBeCloseTo(s.ledger.heightOf(0, 100) + s.ledger.heightOf(110, 4900));
    expect(islandFirst.getBoundingClientRect().top).toBeCloseTo(before);
    expect(s.view.gapCount).toBe(2);
    expect(s.view.isPending(50) && s.view.isPending(200) && !s.view.isPending(105)).toBe(true);
  });

  it("空区间跳过", () => {
    const s = setup(100, 50);
    s.view.attachGaps([
      [10, 10],
      [0, 50],
    ]);
    expect(s.view.gapCount).toBe(1);
  });
});

// ===== 〔第二级 · `§6` 步 9〕Worker 精算：按需 ＋ 后台 =====
// 要求：「Worker 做成『按需 ＋ 后台』：窗口附近上下各 N 屏优先精算，其余空闲时补或干脆不算」
// ＋「第二级（Worker 算完回来）：pretext 精确高度覆盖粗估」。量法在这里是替身（jsdom 没有 Worker / canvas）。
describe("〔RENDER2〕第二级估高", () => {
  it("账本换进精算高之后，任意区间的高 == 逐行（精算 ?? 粗估）相加（随机）", () => {
    let x = 7;
    const rnd = (n: number): number => ((x = (x * 1103515245 + 12345) % 2147483648) % n);
    const rows: SkeletonFacts[] = Array.from({ length: 400 }, (_, i) =>
      i % 5 === 0 ? { o: i, n: 1 } : { o: i, n: 1, t: "reply", dr: "card", ch: 30 + rnd(900), pl: 1 + rnd(4) },
    );
    const ledger = new SkeletonLedger(0, indexPages(rows));
    const first = Array.from({ length: 400 }, (_, i) => ledger.heightOf(i, i + 1));
    const want = [...first];
    for (let k = 0; k < 60; k++) {
      const s = rnd(400);
      const h = 10 + rnd(500);
      ledger.refine([[s, h]]);
      if (rows[s].t !== undefined) want[s] = h; // 不建卡的行不收
    }
    for (let k = 0; k < 100; k++) {
      const a = rnd(400);
      const b = a + rnd(400 - a + 1);
      expect(ledger.heightOf(a, b)).toBeCloseTo(want.slice(a, b).reduce((p, q) => p + q, 0), 6);
    }
    // 列宽变了 ⇒ 精算过的那些交回来重算、账本回到第一级
    const redo = ledger.relayout(undefined);
    expect(new Set(redo)).toEqual(new Set([...want.keys()].filter((i) => want[i] !== first[i])));
    expect(ledger.totalHeight).toBeCloseTo(first.reduce((p, q) => p + q, 0), 6);
  });

  it("只交视口上下 2 屏之内的占位行；回来的高换进占位、视口里那张已渲染卡的屏幕位置不动", async () => {
    const { HeightRefiner } = await import("../../../src/frontend/ui/height-refiner");
    const s = setup(3000, 2990);
    s.view.attach(2990);
    // 视口停在占位最下沿、露出已渲染的第一张卡（seq 2990）
    s.layout.scrollTop = s.ledger.heightOf(0, 2990) - VIEW_H / 2;
    const anchor = cardsIn(s.content)[0] as HTMLElement;
    const before = anchor.getBoundingClientRect().top;
    const rowH = s.ledger.heightOf(0, 1);
    const asked = s.view.nearbyUnrefined(2);
    // 期望集合手算：占位在视口里的那半屏 ＋ 上面 2 屏 ⇒ 下沿往上 (VIEW_H/2 + 2·VIEW_H) 像素覆盖的行
    const px = VIEW_H / 2 + 2 * VIEW_H;
    const lo = 2990 - Math.ceil(px / rowH);
    expect(asked).toEqual(Array.from({ length: 2990 - lo }, (_, i) => lo + i));
    const seen: number[] = [];
    const refiner = new HeightRefiner(async (items) => {
      seen.push(items.length);
      return items.map(() => 90);
    });
    await refiner.refine(
      s.view,
      asked.map((seq) => ({
        seq,
        rec: { agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x".repeat(200) }], autoReply: false, draw: "card", human: false } as never,
      })),
    );
    expect(seen).toEqual([asked.length]);
    expect(s.ledger.heightOf(lo, 2990)).toBeCloseTo(90 * (2990 - lo), 6);
    const gap = s.content.querySelector<HTMLElement>(`.${SKELETON_GAP_CLASS}`)!;
    expect(parseFloat(gap.style.height)).toBeCloseTo(s.ledger.heightOf(0, 2990), 6);
    expect(anchor.getBoundingClientRect().top, "精算把视口推走了").toBeCloseTo(before, 6);
    // 再问：精算过的不再给
    expect(s.view.nearbyUnrefined(2).filter((q) => q >= lo)).toEqual([]);
  });
});

describe("〔RENDER2〕第二级与第一级同一套外框常数", () => {
  it("正文量法换成第一级那份算术时，第二级 == 第一级（user / assistant、带代码块与折叠单元）", async () => {
    const { refineItemOf, refinedHeight, estimateFromFacts } = await import("../../../src/frontend/ui/height-estimate");
    // 第一级的字宽算术（`factLines`：CJK 全宽、其余 0.52em，硬行数 ＋ 总宽 / 列宽）——替身量法照抄它
    const arith = (it: { text: string; font: string; lineHeightPx: number; widthPx: number }): number => {
      const size = it.font.startsWith("14px") ? 14 : 15;
      let w = 0;
      for (const ch of it.text) if (ch !== "\n") w += ch.charCodeAt(0) > 0x2e80 ? size : size * 0.52;
      const pl = it.text.split("\n").filter((l) => l.trim()).length;
      return (pl + w / it.widthPx) * it.lineHeightPx;
    };
    const cases: Array<[unknown, SkeletonFacts]> = [
      [
        { agent: "claude", t: "said", id: "a", who: { speaker: { kind: "human" }, text: "第一行abc\nsecond line" }, blocks: [{ type: "text", text: "第一行abc\nsecond line" }], draw: "card", human: true },
        { o: 0, n: 1, t: "said", dr: "card", hu: true, ch: "第一行abc".length + "second line".length, cj: 3, pl: 2 },
      ],
      [
        {
          agent: "claude",
          t: "reply",
          id: "b",
          blocks: [
            { type: "text", text: "para one\n\npara two\n```\ncode 1\ncode 2\n```" },
            { type: "tool_use", id: "x", name: "Read", input: {} },
          ],
          autoReply: false,
          draw: "card",
          human: false,
        },
        { o: 0, n: 1, t: "reply", dr: "card", ch: "para one".length + "para two".length, pl: 2, cb: 1, cl: 2, fd: 1 },
      ],
    ];
    for (const [rec, facts] of cases) {
      const it = refineItemOf(rec as never)!;
      expect(refinedHeight(it, arith), JSON.stringify(facts)).toBeCloseTo(estimateFromFacts(facts), 6);
    }
  });
});

// ===== 列宽变了：账本按新列宽重估、只重算精算过的 =====
// 要求：「列宽变化只重算已精算过的」＋「今天列宽只量一次（`COL_W`），列宽变了只重算精算过的那一步（`relayout`）还没有入口」；同一行。
describe("〔P3〕列宽变了", () => {
  const rec = (seq: number) =>
    ({ agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x".repeat(200) }], autoReply: false, draw: "card", human: false }) as never;

  it("SkeletonView.relayout：每行高 == 新列宽下的第一级；精算过的那几行进待重交；占位改高、视口钉住；差不到 1px 不动", () => {
    const s = setup(3000, 2990);
    s.view.attach(2990);
    s.layout.scrollTop = s.ledger.heightOf(0, 2990) - VIEW_H / 2;
    const anchor = cardsIn(s.content)[0] as HTMLElement;
    const before = anchor.getBoundingClientRect().top;
    s.view.applyRefined([
      [10, 77],
      [1500, 88],
      [2980, 99],
    ]);
    const w = initialColumnWidth() / 2;
    expect(s.view.relayout(w)).toBe(true);
    for (let i = 0; i < 3000; i += 97) {
      const f = s.ledger.factsOf(i)!;
      expect(s.ledger.heightOf(i, i + 1), `第 ${i} 行`).toBeCloseTo(estimateFromFacts(f, w), 6);
    }
    const gap = s.content.querySelector<HTMLElement>(`.${SKELETON_GAP_CLASS}`)!;
    expect(parseFloat(gap.style.height)).toBeCloseTo(s.ledger.heightOf(0, 2990), 6);
    expect(anchor.getBoundingClientRect().top, "重估把视口推走了").toBeCloseTo(before, 6);
    expect(s.view.relayout(w + 0.5), "差不到 1px 也重估").toBe(false);
    expect(s.view.takeStale(10)).toEqual([10, 1500, 2980]);
    expect(s.view.staleCount).toBe(0);
  });

  it("算的途中列宽变了 ⇒ 回来的高整批丢掉（回 false），账本里一行都不收", async () => {
    const { HeightRefiner } = await import("../../../src/frontend/ui/height-refiner");
    const s = setup(300, 290);
    s.view.attach(290);
    let release!: () => void;
    const refiner = new HeightRefiner(
      (items) => new Promise((r) => (release = () => r(items.map(() => 40)))),
    );
    const p = refiner.refine(s.view, [{ seq: 5, rec: rec(5) }]);
    s.view.relayout(initialColumnWidth() / 2);
    release();
    expect(await p).toBe(false);
    expect(s.ledger.isRefined(5)).toBe(false);
  });

  it("宿主接线：量到列宽变了 ⇒ 重估，离视口再远的精算行也按新列宽重交 Worker", async () => {
    const { HeightRefiner } = await import("../../../src/frontend/ui/height-refiner");
    const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
    const { TabStore } = await import("../../../src/frontend/ui/tab-store");
    const s = setup(3000, 2990);
    s.view.attach(2990);
    s.layout.scrollTop = s.ledger.heightOf(0, 2990) - VIEW_H / 2; // 视口在最下面，第 10 行远在几十屏之外
    s.view.applyRefined([[10, 77]]);
    const widths: number[] = [];
    const refiner = new HeightRefiner(async (items) => {
      widths.push(...items.map((it) => it.widthPx));
      return items.map(() => 55);
    });
    const store = new TabStore();
    const tab = {
      sessionId: "t",
      skeleton: s.view,
      parentPath: "/p/t.jsonl",
      origin: "<local>",
      window: { peekSeqs: (seqs: Set<number>) => [...seqs].map((seq) => ({ seq, record: rec(seq) })) },
      stream: { contentElement: { getBoundingClientRect: () => ({ width: 500 }) } },
    };
    store.tabs.set("t", tab as never);
    const tsv = new TabStreamView(store, document.createElement("div"), {} as never, refiner);
    (tsv as unknown as { relayoutOnColumnChange(t: unknown): void }).relayoutOnColumnChange(tab);
    await new Promise((r) => setTimeout(r, 0));
    expect(s.ledger.columnWidth).toBe(500);
    expect(s.ledger.isRefined(10), "离视口远的那一行没重交").toBe(true);
    expect(s.ledger.heightOf(10, 11)).toBe(55);
    expect(new Set(widths)).toEqual(new Set([500])); // assistant 正文宽 == 列宽
    expect(s.view.staleCount).toBe(0);
  });

  it("宿主接线：大账本改列宽只当场估视口附近，远处的行空闲时一段一段补完（同一个排着的不重复排）", async () => {
    const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
    const { TabStore } = await import("../../../src/frontend/ui/tab-store");
    const total = 45_000; // 过了 `SkeletonView.LAZY_ROWS` 才只当场估附近；远处的比一段（`SETTLE_ROWS`）多 ⇒ 要补好几段
    const s = setup(total, total - 10);
    s.view.attach(total - 10);
    s.layout.scrollTop = s.ledger.heightOf(0, total - 10) - VIEW_H / 2;
    const w = initialColumnWidth() / 2;
    expect(s.view.relayout(w)).toBe(true);
    const far = s.ledger.staleRows;
    expect(far, "前置：远处要留着没估、比两段多").toBeGreaterThan(2 * 20_000);
    const steps = vi.spyOn(s.view, "settleLayout");
    const tsv = new TabStreamView(new TabStore(), document.createElement("div"), {} as never) as unknown as {
      settleLayoutLater(sk: SkeletonView): void;
      dispose(): void;
    };
    tsv.settleLayoutLater(s.view);
    tsv.settleLayoutLater(s.view);
    await vi.waitFor(() => expect(s.ledger.staleRows).toBe(0), UNTIL_CEILING);
    expect(steps.mock.calls.length, "一段至多补 `SETTLE_ROWS` 行；同一个排了两次也只补一遍").toBe(Math.ceil(far / 20_000));
    for (let i = 0; i < total; i += 997) {
      expect(s.ledger.heightOf(i, i + 1), `第 ${i} 行`).toBeCloseTo(estimateFromFacts(s.ledger.factsOf(i)!, w), 6);
    }
    tsv.dispose();
  });

  it("宿主接线·第二级：只要会建卡的行、连着的并成一段一问；算的途中列宽变了 ⇒ 放回、接着交；回来时 tab 已换代 ⇒ 不收", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { recordReadCalls } = await import("../../test-support/chan-fake");
    const { HeightRefiner } = await import("../../../src/frontend/ui/height-refiner");
    const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
    const { TabStore } = await import("../../../src/frontend/ui/tab-store");
    vi.mocked(invoke).mockClear();
    // 第 i 行在文件里占 [10i, 10i + 10)；第 7 行不建卡（没有正文）
    const s = setup(20, 10, () => 60, Array.from({ length: 20 }, (_, i) => (i === 7 ? { o: i * 10, n: 10, t: "reply" } : { ...asst(), o: i * 10, n: 10 })));
    s.view.attach(10);
    vi.spyOn(s.view, "nearbyUnrefined").mockReturnValue([5, 6, 7, 8, 9]);
    const releases: Array<() => void> = [];
    const refiner = new HeightRefiner((items) => new Promise((r) => releases.push(() => r(items.map(() => 33)))));
    const store = new TabStore();
    const tab = {
      sessionId: "t",
      skeleton: s.view,
      parentPath: "/p/t.jsonl",
      origin: "<local>",
      // 第 9 行账本里有正文（不必去取）
      window: { peekSeqs: (seqs: Set<number>) => (seqs.has(9) ? [{ seq: 9, record: rec(9) }] : []) },
    };
    store.tabs.set("t", tab as never);
    const tsv = new TabStreamView(store, document.createElement("div"), {} as never, refiner) as unknown as { refineNearby(t: unknown): void };
    tsv.refineNearby(tab);
    const ranges = () => recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_range").map((a) => [a.offset, a.until, a.seqBase]);
    expect(ranges(), "5、6 并成一段，7 不建卡不要，8 另起一段，9 账本里有").toEqual([
      [50, 70, 5],
      [80, 90, 8],
    ]);
    await vi.waitFor(() => expect(releases.length).toBe(1), UNTIL_CEILING);
    s.view.relayout(initialColumnWidth() / 2); // 算的途中列宽变了 ⇒ 这一批作废
    releases[0]!();
    await vi.waitFor(() => expect(releases.length, "作废的那一批没放回重交").toBe(2), UNTIL_CEILING);
    releases[1]!();
    await vi.waitFor(() => expect(s.ledger.isRefined(9)).toBe(true), UNTIL_CEILING);
    expect(s.ledger.heightOf(9, 10)).toBe(33);
    expect(s.view.staleCount).toBe(0);
    // 都要过了 ⇒ 不再问
    vi.mocked(invoke).mockClear();
    tsv.refineNearby(tab);
    expect(ranges()).toEqual([]);
    // 回来时 tab 已换代（正文重来）⇒ 不收、不接着交
    s.view.returnStale([5]);
    tsv.refineNearby(tab);
    store.tabs.set("t", { ...tab } as never);
    s.view.returnStale([6]);
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(releases.length, "换代之后还交了 Worker").toBe(2);
    expect(s.view.staleCount, "换代之后还接着交").toBe(1);
  });
});

// ===== 〔A17-45-3-2〕冷开只建视口那一截，折着的过程不建卡 =====
// 冷开一个没建过卡的 tab：尾部窗口里是重放推来的 600 行（壳留存每会话的尾巴），之前一轮 150 条、四轮全建（489 个条目，
// 205 个在折着的过程里 0 高）—— WebKit 冷切停住那一帧 1.6 s 跟着它走。改成：只建填满一屏的那一截，接上骨架、轮次落地之后
// 照账本补可见区（折着的扣掉）。真 `TabStreamView` ＋ 真 `MessageStream` · `RecordTimeline` · `TurnFold`；几何同上面的一维模型，
// 只是卡没有 `data-h`（真渲染出来的）⇒ 一律 60px，`proc-hidden` 的 0、过程行 0；IPC 只有骨架索引与轮次两问是替身。
describe("〔A17-45-3-2〕冷开只建一屏", () => {
  const N = 600;
  const PER = 20;
  const CARD_H = 60;
  const reply = (seq: number) =>
    ({ agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x" }], autoReply: false, draw: "card", human: false }) as never;
  const facts = (i: number) => ({ o: i * 100, n: 100, t: "reply", dr: "card", ch: 200, pl: 2 });
  /** 每轮：开头那一行露着、结尾（最后一行）露着，中间是过程（折着）。 */
  const turn = (k: number) => ({
    at: k * PER * 100,
    line: k * PER,
    uuid: `u${k * PER}`,
    start: "2026-10-06T02:01:00Z",
    end: "2026-10-06T02:04:02Z",
    startText: "02:01",
    endText: "02:04",
    said: "q",
    tools: 3,
    thinking: 0,
    agents: 0,
    background: 0,
    retries: 0,
    peers: 0,
    fails: 0,
    shown: [`u${k * PER + PER - 1}`],
    reply: "",
    done: true,
    phase: "idle",
    folds: [{ after: `u${k * PER}`, afterLine: k * PER, until: `u${k * PER + PER - 1}`, untilLine: k * PER + PER - 1, parts: [{ text: "p", tone: "plain" }], span: { text: "", from: null, to: null }, phase: "idle", firstFail: null }],
  });

  /** 一维布局（同上），外加 `scrollHeight` / `clientHeight`：流的贴底读它们。 */
  function layoutOf(scrollEl: HTMLElement, content: HTMLElement): { scrollTop: number } {
    const state = { scrollTop: 0 };
    const h = (el: Element): number => {
      const e = el as HTMLElement;
      if (e.classList.contains("proc-hidden") || e.classList.contains("proc-line") || e.classList.contains("stream-more-above")) return 0;
      if (e.style.height) return parseFloat(e.style.height);
      return CARD_H;
    };
    const total = (): number => Array.from(content.children).reduce((a, c) => a + h(c), 0);
    // 内容变矮了 scrollTop 跟着夹回去（同真引擎）
    const top = (): number => (state.scrollTop = Math.max(0, Math.min(state.scrollTop, total() - VIEW_H)));
    Object.defineProperty(scrollEl, "scrollTop", { get: top, set: (v: number) => ((state.scrollTop = v), top()), configurable: true });
    Object.defineProperty(scrollEl, "scrollHeight", { get: () => Math.max(VIEW_H, total()), configurable: true });
    Object.defineProperty(scrollEl, "clientHeight", { get: () => VIEW_H, configurable: true });
    const rect = (y: number, hh: number): DOMRect => ({ top: y, bottom: y + hh, height: hh, left: 0, right: 100, width: 100, x: 0, y }) as DOMRect;
    scrollEl.getBoundingClientRect = () => rect(0, VIEW_H);
    content.getBoundingClientRect = () => rect(-top(), total());
    const orig = HTMLElement.prototype.getBoundingClientRect;
    HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      if (this.parentElement !== content) return orig.call(this);
      if (h(this) === 0 && !this.style.height) return rect(0, 0);
      let y = -top();
      for (const c of Array.from(content.children)) {
        const hh = h(c);
        if (c === this) return rect(y, hh);
        y += hh;
      }
      return rect(0, 0);
    };
    onTestFinished(() => {
      HTMLElement.prototype.getBoundingClientRect = orig;
    });
    return state;
  }

  async function rig(late = false) {
    const { invoke } = await import("@tauri-apps/api/core");
    const { isChanCall, chanArgsJson, chanReply, withSessionReads } = await import("../../test-support/chan-fake");
    const { packIndex } = await import("../../test-support/pack-index");
    const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
    const { TabStore } = await import("../../../src/frontend/ui/tab-store");
    vi.stubGlobal("CSS", { escape: (s: string) => s });
    onTestFinished(() => void vi.unstubAllGlobals());
    const rows = Array.from({ length: N }, (_, i) => facts(i));
    // 骨架索引照后端：从 `offset` 起、至多 `limit` 行；按偏移取正文：第 i 行占字节 [100i, 100i + 100)
    const payload = (s: number) => ({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, record: reply(s) });
    const others = withSessionReads((which, a) =>
      which === "read_session_range"
        ? Array.from({ length: (Number(a.until) - Number(a.offset)) / 100 }, (_, k) => payload(Number(a.offset) / 100 + k))
        : undefined,
    );
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (isChanCall(cmd, args, "history-index")) {
        const b = chanArgsJson(args) as { offset: number; limit?: number };
        const lo = b.offset / 100;
        const hi = Math.min(rows.length, b.limit === undefined ? rows.length : lo + b.limit);
        return chanReply({ ...packIndex(lo * 100, hi * 100, rows.slice(lo, hi)), next: null });
      }
      return (await others(cmd, args)) ?? null;
    });
    onTestFinished(() => void vi.mocked(invoke).mockReset().mockResolvedValue(null));
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const store = new TabStore();
    const host = { onLine() {}, refreshTabBar() {}, scheduleTabBarRefresh() {}, userActive() {}, forkFrom() {}, runLabelOf: () => undefined, markNewRunCard() {} };
    const tsv = new TabStreamView(store, root, host);
    onTestFinished(() => tsv.dispose());
    const dom = tsv.mountTabDom("c");
    // 轮次那一问：`late` ⇒ 等测试放行（骨架先接上、轮次后到那一形）
    let letTurns!: () => void;
    const gate = late ? new Promise<void>((res) => (letTurns = res)) : Promise.resolve();
    const turns = vi.fn(async () => {
      await gate;
      return { available: true, from: 0, end: N * 100, turns: Array.from({ length: N / PER }, (_, k) => turn(k)) };
    });
    (dom.turnFold as unknown as { read: unknown }).read = turns;
    const tab = {
      sessionId: "c",
      ...dom,
      parentPath: "/p/c.jsonl",
      origin: "<local>",
      agent: null,
      face: null,
      needs: null,
      pending: [],
      retries: new Map(),
      toolUseNames: new Map(),
      runCards: new Map(),
      unread: 0,
      fullRecord: async () => null,
      facts: { reset() {}, markStale() {}, refresh: async () => {}, needsFetch: false },
    };
    store.tabs.set("c", tab as never);
    const layout = layoutOf(dom.streamEl, dom.stream.contentElement);
    // 重放推来的尾巴：全收纳（后台 tab，还没建过卡）
    for (let s = 0; s < N; s++) {
      tab.seenSeqs.add(s);
      tab.window.defer({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, record: reply(s) } as never);
    }
    const built = (): number[] =>
      Array.from(dom.stream.contentElement.children)
        .map((c) => c.getAttribute("data-id"))
        .filter((id): id is string => id !== null && /^u\d+$/.test(id))
        .map((id) => Number(id.slice(1)));
    const open = (): void => {
      store.activeId = "c";
      tsv.showOnly("c");
      tsv.activate(tab as never);
      tsv.scrollToBottom(tab as never); // 宿主切进来照「贴着底」贴底（`tabs.ts::switchTo`）
    };
    /** 实时来一行（记录文件同时长了一行：索引里也有它）。 */
    const live = (s: number): void => {
      rows.push(facts(s));
      const p = payload(s) as never;
      if (tsv.admit(tab as never, p)) tsv.ingest(tab as never, p);
    };
    return { tab, dom, layout, built, open, turns, letTurns: () => letTurns?.(), store, tsv, live };
  }

  /** 视口 ±`KEEP_SCREENS` 屏放得下几张卡。 */
  const keepCards = (1 + 2 * SkeletonView.KEEP_SCREENS) * Math.ceil(VIEW_H / CARD_H);

  it("冷开返回时：已建的只有填满一屏那一截（≤ 视口 ±KEEP_SCREENS 屏），贴底，最后一张卡顶到视口下沿", async () => {
    const r = await rig();
    r.open();
    const ids = r.built();
    expect(ids.length, `冷开建了 ${ids.length} 张卡`).toBeGreaterThan(0);
    expect(ids.length, `冷开建了 ${ids.length} 张卡（视口 ±${SkeletonView.KEEP_SCREENS} 屏放得下 ${keepCards} 张）`).toBeLessThanOrEqual(keepCards);
    expect(Math.max(...ids)).toBe(N - 1);
    const last = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${N - 1}"]`)!;
    expect(last.getBoundingClientRect().bottom, "没贴底").toBeCloseTo(VIEW_H, 0);
  });

  it.each([
    ["轮次先到", false],
    ["骨架先接上、轮次后到", true],
  ])("接上骨架、轮次落地之后（%s）：补可见区不建折着的过程，已建的都在视口 ±KEEP_SCREENS 屏里，视口里不空", async (_name, late) => {
    const r = await rig(late);
    r.open();
    const atOpen = new Set(r.built());
    await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
    r.letTurns();
    await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
    const sk = r.tab.skeleton!;
    const folded = sk.ledger.foldedRanges;
    expect(folded.length, "前置：账本拿到了折叠").toBeGreaterThan(0);
    const ids = r.built();
    const inFold = ids.filter((s) => !atOpen.has(s) && folded.some(([a, b]) => s >= a && s < b));
    expect(inFold, "骨架补可见区建了折着的过程").toEqual([]);
    expect(ids.length, `已建 ${ids.length} 张`).toBeLessThanOrEqual(keepCards);
    for (const s of ids) {
      const el = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!;
      if (el.classList.contains("proc-hidden")) continue;
      const box = el.getBoundingClientRect();
      expect(box.bottom, `第 ${s} 行离视口超过 ${SkeletonView.KEEP_SCREENS} 屏`).toBeGreaterThan(-VIEW_H * SkeletonView.KEEP_SCREENS);
      expect(box.top).toBeLessThan(VIEW_H * (1 + SkeletonView.KEEP_SCREENS));
    }
    // 视口里有多高是卡（占位不算）：过程藏掉之后露出来的占位要补成卡，不许留着一块空的
    const covered = ids.reduce((a, s) => {
      const b = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!.getBoundingClientRect();
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
    expect(covered, "过程藏掉之后视口里露出的占位没补").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });

  // 〔A17-45-3-3〕收起的 tab 只留视口那一截：冷开建的那一截尾巴与接骨架之后建的都要交得回去
  //（骨架索引从前只要 `[0, floor)`，冷开建的尾巴落在账本外，`release` 认不到它的切点 ⇒ 一条都不交）。
  /** 一屏放得下几张卡，再加切点两头各一张。 */
  const screenCards = Math.ceil(VIEW_H / CARD_H) + 2;
  const settled = async (r: Awaited<ReturnType<typeof rig>>): Promise<void> => {
    await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
    await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
  };
  const hide = (r: Awaited<ReturnType<typeof rig>>): void => {
    r.store.activeId = "other";
    r.tsv.showOnly("other");
  };

  it("〔A17-45-3-3〕冷开、接上骨架之后切走：留下的已建条目 ≤ 一屏", async () => {
    const r = await rig();
    r.open();
    await settled(r);
    hide(r);
    const ids = r.built();
    expect(ids.length, `切走之后还留着 ${ids.length} 张：${ids.join(",")}`).toBeLessThanOrEqual(screenCards);
  });

  it("〔A17-45-3-3〕接上骨架之后又来了几屏实时行：切走 · 切回 · 再切走 ⇒ 留下的 ≤ 一屏（账本续到了末尾）", async () => {
    const r = await rig();
    r.open();
    await settled(r);
    for (let s = N; s < N + 40; s++) r.live(s);
    expect(r.built().filter((s) => s >= N).length, "前置：实时行建了卡").toBe(40);
    hide(r);
    await vi.waitFor(() => expect(r.tab.skeleton!.ledger.endSeq, "收起时没把账本续到末尾").toBe(N + 40), UNTIL_CEILING);
    r.store.activeId = "c";
    r.tsv.showOnly("c");
    r.tsv.activate(r.tab as never);
    await new Promise((res) => requestAnimationFrame(() => res(null)));
    hide(r);
    const kept = r.built();
    expect(kept.length, `切走之后还留着 ${kept.length} 张：${kept.join(",")}`).toBeLessThanOrEqual(screenCards);
  });

  it("〔A17-45-3-3〕往上翻过几屏再切走：留下的 ≤ 一屏；切回来建的 ≤ 一屏、视口不空", async () => {
    const r = await rig();
    r.open();
    await settled(r);
    for (let k = 0; k < 6; k++) {
      r.layout.scrollTop = Math.max(0, r.layout.scrollTop - VIEW_H);
      r.dom.streamEl.dispatchEvent(new Event("scroll"));
      await new Promise((res) => setTimeout(res, 0));
    }
    await vi.waitFor(() => expect(r.dom.stream.contentElement.querySelector("[data-skeleton-fetching]"), "取回中的还没落").toBeNull(), UNTIL_CEILING);
    expect(r.built().length, "前置：翻过之后建了卡").toBeGreaterThan(0);
    hide(r);
    const kept = r.built();
    expect(kept.length, `切走之后还留着 ${kept.length} 张：${kept.join(",")}`).toBeLessThanOrEqual(screenCards);
    // 切回来：照「切进来」那一路（非 virgin ⇒ 下一帧补可见区）
    r.store.activeId = "c";
    r.tsv.showOnly("c");
    r.tsv.activate(r.tab as never);
    await new Promise((res) => requestAnimationFrame(() => res(null)));
    await vi.waitFor(() => expect(r.dom.stream.contentElement.querySelector("[data-skeleton-fetching]")).toBeNull(), UNTIL_CEILING);
    const back = r.built();
    // 补可见区多建视口上下各半屏（`OVERSCAN`）⇒ 至多两屏
    expect(back.length - kept.length, "切回来建了不止视口 ±半屏").toBeLessThanOrEqual(2 * Math.ceil(VIEW_H / CARD_H) + 2);
    const covered = back.reduce((a, s) => {
      const b = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!.getBoundingClientRect();
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
    expect(covered, "切回来视口里空着").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });

  // 〔A17-45-3-4〕后台 tab 只进账本：冷切只建一屏之后不值得提前建，看不见的 tab 一张卡都不建（从前批结束后空闲时一截 20 条建满 150 条）。
  it("〔A17-45-3-4〕重放批结束、过了好一阵：后台 tab（能开终端的活会话、还没建过卡）一张卡都不建", async () => {
    const { goldenFace } = await import("../../test-support/session-faces");
    const { hasTerminal } = await import("../../../src/frontend/ui/face-can");
    const r = await rig();
    (r.tab as { face: unknown }).face = goldenFace("live-idle");
    expect(hasTerminal(r.tab.face as never), "前置：能开终端（从前进后台物化队列的那一类）").toBe(true);
    r.store.activeId = "other";
    vi.useFakeTimers();
    onTestFinished(() => void vi.useRealTimers());
    r.tsv.batchEnd();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(r.built(), "后台 tab 在空闲时建了卡").toEqual([]);
    expect(r.tab.window.pendingCount, "账本照留").toBe(N);
  });
});
