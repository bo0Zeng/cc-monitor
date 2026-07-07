/** Batch13-F39:未渲染区间账本单测。 */
import { describe, it, expect } from "vitest";
import { UnrenderedRanges } from "./render-window";

describe("UnrenderedRanges", () => {
  it("初始整段未渲染;挖尾段后剩前段", () => {
    const r = new UnrenderedRanges(100);
    expect(r.remaining).toBe(100);
    r.markRendered(80, 100);
    expect(r.remaining).toBe(80);
    expect(r.contains(79)).toBe(true);
    expect(r.contains(80)).toBe(false);
  });
  it("中间挖岛 → 分裂成两个洞;gapAbove 取最近的上方洞", () => {
    const r = new UnrenderedRanges(100);
    r.markRendered(90, 100); // 尾屏
    r.markRendered(40, 60); // 深链岛
    expect(r.remaining).toBe(70);
    // 已渲染最低下标 = 40(岛起点):首洞 [0,40) 从 0 起,其右边界即最低已渲染
    expect(r.lowestRenderedIdx()).toBe(40);
    // 岛上方的洞
    expect(r.gapAbove(40)).toEqual([0, 40]);
    // 尾屏上方最近的洞是 [60,90)
    expect(r.gapAbove(90)).toEqual([60, 90]);
  });
  it("gapAbove 右边界截断到 idx", () => {
    const r = new UnrenderedRanges(100);
    r.markRendered(90, 100);
    expect(r.gapAbove(50)).toEqual([0, 50]);
  });
  it("越界/空区间/重复挖安全;挖穿后 isEmpty", () => {
    const r = new UnrenderedRanges(10);
    r.markRendered(5, 5);
    r.markRendered(-5, 3);
    r.markRendered(3, 20);
    r.markRendered(0, 10);
    expect(r.isEmpty).toBe(true);
    expect(r.gapAbove(10)).toBeNull();
  });
  it("total=0 即空", () => {
    expect(new UnrenderedRanges(0).isEmpty).toBe(true);
  });
  it("首洞不从 0 起时 lowestRenderedIdx=0", () => {
    const r = new UnrenderedRanges(100);
    r.markRendered(0, 10);
    expect(r.lowestRenderedIdx()).toBe(0);
    const r2 = new UnrenderedRanges(100);
    r2.markRendered(50, 100);
    expect(r2.lowestRenderedIdx()).toBe(50);
  });
});
