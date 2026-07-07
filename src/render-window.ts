/**
 * Batch13-F39:未渲染区间管理(纯数据,零 DOM 依赖,便于单测)。
 *
 * viewer 尾部优先增量渲染的账本:全量 payload 按下标 [0,total) 排列,
 * 已渲染的段从集合里挖掉,剩下的就是"洞"。上翻补批/深链岛都是
 * 「找洞 → 渲染一段 → 挖掉」的循环。
 *
 * 区间为半开 [lo, hi),集合始终有序且不相交(操作保序)。
 */
export class UnrenderedRanges {
  private ranges: Array<[number, number]> = [];

  constructor(total: number) {
    if (total > 0) this.ranges = [[0, total]];
  }

  get isEmpty(): boolean {
    return this.ranges.length === 0;
  }

  /** 剩余未渲染条数 */
  get remaining(): number {
    return this.ranges.reduce((s, [a, b]) => s + (b - a), 0);
  }

  /** 标记 [lo,hi) 已渲染(与现有洞求差;越界/空区间安全) */
  markRendered(lo: number, hi: number): void {
    if (hi <= lo) return;
    const next: Array<[number, number]> = [];
    for (const [a, b] of this.ranges) {
      if (hi <= a || lo >= b) {
        next.push([a, b]);
        continue;
      }
      if (a < lo) next.push([a, lo]);
      if (hi < b) next.push([hi, b]);
    }
    this.ranges = next;
  }

  /** idx 是否未渲染 */
  contains(idx: number): boolean {
    return this.ranges.some(([a, b]) => idx >= a && idx < b);
  }

  /**
   * 严格位于 idx 上方(区间起点 < idx)的**最近**洞,右边界截断到 idx。
   * 上翻补批用:传"当前已渲染内容的最低下标",得到该往上补的洞。
   */
  gapAbove(idx: number): [number, number] | null {
    let best: [number, number] | null = null;
    for (const [a, b] of this.ranges) {
      if (a >= idx) break;
      best = [a, Math.min(b, idx)];
    }
    return best;
  }

  /** 全集最低的已渲染下标之前的洞不存在时,返回已渲染的最低下标估算辅助:
   *  即第一个洞若从 0 开始,最低已渲染下标 = 洞的右边界;否则 0。 */
  lowestRenderedIdx(): number {
    if (this.ranges.length === 0) return 0;
    const [a, b] = this.ranges[0];
    return a === 0 ? b : 0;
  }
}
