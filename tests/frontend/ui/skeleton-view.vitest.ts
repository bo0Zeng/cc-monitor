/**
 * 〔骨架〕骨架层的判据 —— **真 `MessageStream` ＋ 真 `RecordTimeline`**，
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
import { beforeEach, describe, expect, it, vi } from "vitest";
import { estimateFromFacts, initialColumnWidth, skeletonKind, type SkeletonFacts } from "../../../src/frontend/ui/height-estimate";
import { SkeletonLedger } from "../../../src/frontend/ui/live-window";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";
import { SKELETON_GAP_CLASS, SkeletonView, ledgerFromIndex } from "../../../src/frontend/ui/skeleton-view";
import { MessageStream } from "../../../src/frontend/ui/stream";

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

  it("只剩不占高的行（工具结果并进工具组那种）的占位落在视口里 ⇒ 照样物化（不因高 0 永远留着）；扣掉折着的那几段", () => {
    const s = setup(30, 20);
    const rows = Array.from({ length: 30 }, (_, i): SkeletonFacts => (i === 11 ? { o: 0, n: 1, t: "user", fd: 1 } : i === 10 ? { o: 0, n: 1, t: "assistant", fd: 1 } : asst(`u${i}`)));
    const ledger = new SkeletonLedger(0, rows);
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
      i % 5 === 0 ? { o: i, n: 1, t: "attachment" } : { o: i, n: 1, t: "assistant", u: `r${i}`, ch: 30 + rnd(900), pl: 1 + rnd(4) },
    );
    const ledger = new SkeletonLedger(0, rows);
    const first = Array.from({ length: 400 }, (_, i) => ledger.heightOf(i, i + 1));
    const want = [...first];
    for (let k = 0; k < 60; k++) {
      const s = rnd(400);
      const h = 10 + rnd(500);
      ledger.refine([[s, h]]);
      if (rows[s].t !== "attachment") want[s] = h; // 不建卡的行不收
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
        rec: { agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x".repeat(200) }], autoReply: false, endsTurn: false } as never,
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
        { agent: "claude", t: "said", id: "a", who: { speaker: { kind: "human" }, text: "第一行abc\nsecond line" }, blocks: [{ type: "text", text: "第一行abc\nsecond line" }] },
        { o: 0, n: 1, t: "user", u: "a", ch: "第一行abc".length + "second line".length, cj: 3, pl: 2 },
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
          endsTurn: false,
        },
        { o: 0, n: 1, t: "assistant", u: "b", ch: "para one".length + "para two".length, pl: 2, cb: 1, cl: 2, fd: 1 },
      ],
    ];
    for (const [rec, facts] of cases) {
      const it = refineItemOf(rec as never)!;
      expect(refinedHeight(it, arith), JSON.stringify(facts)).toBeCloseTo(estimateFromFacts(facts, "none"), 6);
    }
  });
});

// ===== 列宽变了：账本按新列宽重估、只重算精算过的 =====
// 要求：「列宽变化只重算已精算过的」＋「今天列宽只量一次（`COL_W`），列宽变了只重算精算过的那一步（`relayout`）还没有入口」；同一行。
describe("〔P3〕列宽变了", () => {
  const rec = (seq: number) =>
    ({ agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x".repeat(200) }], autoReply: false, endsTurn: false }) as never;

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
    let prev: ReturnType<typeof skeletonKind> = "none";
    for (let i = 0; i < 3000; i += 97) {
      const f = s.ledger.factsOf(i)!;
      prev = i === 0 ? "none" : skeletonKind(s.ledger.factsOf(i - 1)!);
      expect(s.ledger.heightOf(i, i + 1), `第 ${i} 行`).toBeCloseTo(estimateFromFacts(f, prev, w), 6);
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
});
