/**
 * 〔hot2〕长会话的骨架账本：轮数上万时 `setFolds` 不许把整列号当实参展开（`Math.min(...)` / `push(...)`）。
 *
 * 起因：V8（Windows 上的 WebView2）给一次调用的实参个数有上限，约 3.4 万轮时 `setFolds` 抛 RangeError、界面崩；
 * Linux 上的 WebKitGTK 是 JSC，给 15 万个也不抛，所以只在 Windows 上见到。vitest 跑在 node 上 = V8，这里就能复现。
 * 夹具全是合成的事实行（没有正文）：40 万行、每 10 行一轮 ⇒ 4 万轮，每轮的过程那 8 行折着、开头带一条过程行。
 */
import { describe, expect, it } from "vitest";
import type { SkeletonFacts } from "../../../src/frontend/ui/height-estimate";
import { SkeletonLedger } from "../../../src/frontend/ui/live-window";
import { indexPages } from "../../test-support/pack-index";

const ROWS = 400_000;
const TURN = 10;

function synthRows(): SkeletonFacts[] {
  const out: SkeletonFacts[] = [];
  for (let i = 0; i < ROWS; i++) out.push(i % TURN === 0 ? { o: 0, n: 1, t: "said", dr: "card", hu: true, ch: 20, pl: 1 } : { o: 0, n: 1, t: "reply", dr: "steps", fd: 1 });
  return out;
}

/** 前 `turns` 轮的折叠：每轮过程那几行（`[开头+1+shift, 下一轮开头-1)`）折着，开头带一条过程行。 */
function synthFolds(turns: number, shift = 0): { folded: Array<[number, number]>; lines: number[] } {
  const folded: Array<[number, number]> = [];
  const lines: number[] = [];
  for (let k = 0; k < turns; k++) {
    const s = k * TURN;
    folded.push([s + 1 + shift, s + TURN - 1]);
    lines.push(s);
  }
  return { folded, lines };
}

describe("SkeletonLedger.setFolds 在 4 万轮的账本上", () => {
  it("折上 · 新来一轮 · 换一组 · 全清都不抛（V8 的实参上限碰不到），折着的行照样按 0 高", () => {
    const l = new SkeletonLedger(0, indexPages(synthRows()));
    const full = l.totalHeight;
    const turns = ROWS / TURN;
    expect(turns).toBeGreaterThanOrEqual(40_000);
    expect(() => l.setFolds({ ...synthFolds(turns - 1), linePx: 30 })).not.toThrow();
    // 新来一轮（界面上最常见的那一次）：新旧两份折叠 ＋ 新旧两份开头都算「动到的」，合计 16 万个号。
    expect(() => l.setFolds({ ...synthFolds(turns), linePx: 30 })).not.toThrow();
    const last = (turns - 1) * TURN;
    expect(l.heightOf(last + 1, last + TURN - 1)).toBe(0);
    expect(l.heightOf(1, TURN - 1)).toBe(0);
    // 换一组（每段往后挪一行）：每轮过程的第一行露出来。
    expect(() => l.setFolds({ ...synthFolds(turns, 1), linePx: 30 })).not.toThrow();
    expect(l.heightOf(2, TURN - 1)).toBe(0);
    expect(l.heightOf(1, 2)).toBeGreaterThan(0);
    expect(() => l.setFolds({ folded: [], lines: [], linePx: 0 })).not.toThrow();
    expect(l.totalHeight).toBeCloseTo(full);
  });
});

describe("改列宽只当场估视口附近（read8b，瓶颈 #10）", () => {
  it("40 万行：给了附近那一段 ⇒ 当场只估那几行，其余一段一段补；补完与整份重估的总高一样", () => {
    const l = new SkeletonLedger(0, indexPages(synthRows()), 700);
    const before = l.estimated;
    l.relayout(420, [200_000, 200_300]);
    expect(l.estimated - before, "改列宽当场估了整份").toBe(300);
    expect(l.staleRows).toBe(ROWS - 300);
    let steps = 0;
    const mark = l.estimated;
    while (l.staleRows > 0) {
      l.settle(20_000);
      steps++;
    }
    expect(l.estimated - mark, "补估多估了 / 少估了").toBe(ROWS - 300);
    expect(steps, "补估不是一段一段做的").toBe(Math.ceil((ROWS - 300) / 20_000));
    const whole = new SkeletonLedger(0, indexPages(synthRows()), 420);
    expect(l.totalHeight).toBeCloseTo(whole.totalHeight, 3);
    expect(l.seqAt(0, ROWS, whole.heightOf(0, 123_456) + 0.5)).toBe(whole.seqAt(0, ROWS, whole.heightOf(0, 123_456) + 0.5));
  });
});

/**
 * 〔A17-45-3-5〕前缀和按块存：改一行只重算它那一块（块长 1 024 行）＋ 块和那一列，不从改动那一行一路重算到尾。
 * 起因：用户真会话 15.5 万行，视口在顶部时 WebKitGTK 上交回占位一次 15 ms、精算一批 8 ms（整列从改动处重合）。
 * 判据数行（`summed`：重合前缀和摸过几行），不卡毫秒。
 */
describe("骨架账本改一行只重算它那一块（40 万行 · 4 万轮折着）", () => {
  const BLOCK = 1024;
  const fresh = (): SkeletonLedger => {
    const l = new SkeletonLedger(0, indexPages(synthRows()), 800);
    l.setFolds({ ...synthFolds(ROWS / TURN), linePx: 30 });
    return l;
  };

  it("顶部精算 20 行（一行一次）· 交回占位量回一行 · 展开再收起顶部一轮：每一下至多重算一两块", () => {
    const l = fresh();
    let mark = l.summed;
    for (let k = 0; k < 20; k++) l.refine([[k * TURN, 100 + k]]);
    expect(l.summed - mark, "顶部精算一行从那一行重算到了尾").toBeLessThanOrEqual(20 * BLOCK);
    mark = l.summed;
    l.measured([[3 * TURN, 77]]);
    expect(l.summed - mark, "交回占位量回一行从那一行重算到了尾").toBeLessThanOrEqual(BLOCK);
    const all = synthFolds(ROWS / TURN);
    mark = l.summed;
    l.setFolds({ folded: all.folded.slice(1), lines: all.lines, linePx: 30 });
    l.setFolds({ folded: all.folded, lines: all.lines, linePx: 30 });
    expect(l.summed - mark, "展开 / 收起一轮重算了整份").toBeLessThanOrEqual(2 * BLOCK);
  });

  it("一串改动之后与照同一份结果整份建的账本处处一样高、像素落在同一行", () => {
    const l = fresh();
    const all = synthFolds(ROWS / TURN);
    const refined: Array<[number, number]> = [];
    // 跨块边上的、块里的、尾巴上的；同一行改两遍
    for (const seq of [0, 10, 1020, 1030, 2040, 2050, 199_990, 200_000, ROWS - 10, 10]) refined.push([seq, 50 + (seq % 97)]);
    for (const e of refined) l.refine([e]);
    l.measured([[1023 * TURN, 61], [ROWS - 1, 0]]);
    const keep = all.folded.filter((_, k) => k % 1000 !== 7);
    l.setFolds({ folded: keep, lines: all.lines, linePx: 26 });
    l.refine([[5000 * TURN + 1, 999]]); // 折着的行精算回来也收（展开了才见）

    // 同一份结果、一次一大批：整份折上 → 精算一批 → 量回 → 换折叠 → 精算
    const whole = fresh();
    whole.refine([...new Map(refined).entries()]);
    whole.measured([[1023 * TURN, 61], [ROWS - 1, 0]]);
    whole.setFolds({ folded: keep, lines: all.lines, linePx: 26 });
    whole.refine([[5000 * TURN + 1, 999]]);

    expect(l.totalHeight).toBeCloseTo(whole.totalHeight, 6);
    for (let i = 0; i <= ROWS; i += 997) expect(l.heightOf(0, i), `前 ${i} 行`).toBeCloseTo(whole.heightOf(0, i), 6);
    for (const i of [BLOCK - 1, BLOCK, BLOCK + 1, 2 * BLOCK, ROWS - 1, ROWS]) expect(l.heightOf(i - 1, i), `第 ${i - 1} 行`).toBeCloseTo(whole.heightOf(i - 1, i), 6);
    for (let k = 0; k < 400; k++) {
      const y = (k * 7919.37) % whole.totalHeight;
      expect(l.seqAt(0, ROWS, y), `y=${y}`).toBe(whole.seqAt(0, ROWS, y));
      expect(l.seqAt(150_000, 160_000, y / 50), `段内 y=${y / 50}`).toBe(whole.seqAt(150_000, 160_000, y / 50));
    }
    // 像素落在哪一行另照 heightOf 逐行找一遍（不走 seqAt 的二分）：块边上 · 零高的块（整块折着）· 段顶段尾
    const at = (lo: number, hi: number, y: number): number => {
      for (let i = lo; i < hi; i++) if (l.heightOf(lo, i + 1) > Math.max(0, y)) return i;
      return hi - 1;
    };
    const quiet = new SkeletonLedger(0, indexPages(synthRows()), 800);
    quiet.setFolds({ folded: [[2 * BLOCK, 4 * BLOCK]], lines: [], linePx: 0 });
    for (const [lo, hi] of [[0, 6 * BLOCK], [BLOCK - 3, 3 * BLOCK + 5], [2 * BLOCK + 1, 5 * BLOCK]]) {
      for (const y of [-1, 0, 0.5, 1e9, ...Array.from({ length: 60 }, (_, k) => k * 913.7)]) {
        expect(l.seqAt(lo, hi, y), `[${lo},${hi}) y=${y}`).toBe(at(lo, hi, y));
      }
      const top = quiet.heightOf(lo, 2 * BLOCK);
      for (const y of [top - 0.5, top, top + 0.5]) {
        let want = hi - 1;
        for (let i = lo; i < hi; i++) if (quiet.heightOf(lo, i + 1) > Math.max(0, y)) { want = i; break; }
        expect(quiet.seqAt(lo, hi, y), `整块零高 [${lo},${hi}) y=${y}`).toBe(want);
      }
    }
  });
});
