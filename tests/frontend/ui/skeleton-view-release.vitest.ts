/**
 * 〔骨架〕视口外的卡交回占位 —— **真 `MessageStream` ＋ 真 `RecordTimeline`**，几何由 `skeleton-view-rig` 的一维布局模型给。
 * 一屏一屏从底滚到顶再滚回底：已建卡数一路不过上界、交回前后总高不变、滚回来同一处还是那几张卡。
 */
import { describe, expect, it } from "vitest";
import type { SkeletonFacts } from "../../../src/frontend/ui/height-estimate";
import { SkeletonView } from "../../../src/frontend/ui/skeleton-view";
import { VIEW_H, cardsIn, makeFetchingHost, setup } from "../../test-support/skeleton-view-rig";

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
