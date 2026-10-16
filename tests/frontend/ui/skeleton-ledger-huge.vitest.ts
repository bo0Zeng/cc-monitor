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
  for (let i = 0; i < ROWS; i++) out.push(i % TURN === 0 ? { o: 0, n: 1, t: "said", ch: 20, pl: 1 } : { o: 0, n: 1, t: "reply", fd: 1 });
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
