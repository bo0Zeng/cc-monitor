/**
 * Batch13-F40a:live Tab 尾部优先的窗口账本(纯数据,零 DOM,对标 render-window.ts)。
 *
 * live 与 viewer(F39 UnrenderedRanges)的关键差异:live 无深链入口,已渲染集
 * **恒为按 seq 连续的尾后缀 [floor, +∞)**——无岛、无中缝(单洞不变量)。
 * 所以不用区间代数,一个 floor 水位 + 待渲染数组就够。
 *
 * 启动重放到达序:同 session 末块先发、块内 seq 升序。尾块被 admit 直渲(进步式
 * 首屏),更早的块整块 defer——pending 此刻是「按块降序、块内升序」的非连续集合,
 * 但唯一消费方式是 takeTail(取 seq 最高的 k 条),消费前惰性 sort 一次即可,
 * 洞的几何从不需要被查询。补批后窗口仍是后缀。
 *
 * 若未来 live 出现深链跳转需求(跳到任意历史位置),本结构需升级为
 * UnrenderedRanges 语义(区间集)——见 plan/designs/f40-fanout-synthesis.md §1.5。
 */
import type { JsonlLinePayload } from "./events";

export class TailWindow {
  /** 窗口低水位;null = virgin(该 tab 尚未渲染任何 content 记录) */
  private floor: number | null = null;
  /** 未渲染 payload;尾追加免排序,乱序块标 dirty 惰性 sort */
  private pending: JsonlLinePayload[] = [];
  private dirty = false;

  get floorSeq(): number | null {
    return this.floor;
  }

  /** 压低水位(幂等取 min——物化/直渲只会让窗口向下扩) */
  pinFloor(seq: number): void {
    this.floor = this.floor === null ? seq : Math.min(this.floor, seq);
  }

  /** 该 seq 是否属于渲染窗口(virgin 时恒 false,由 caller 决定钉 floor 直渲还是收纳) */
  admit(seq: number): boolean {
    return this.floor !== null && seq >= this.floor;
  }

  /** 收纳一条未渲染 payload。到达序通常块内升序 → 尾追加免排序。 */
  defer(p: JsonlLinePayload): void {
    const last = this.pending[this.pending.length - 1];
    if (last !== undefined && p.seq < last.seq) this.dirty = true;
    this.pending.push(p);
  }

  /**
   * 弹出 pending 中 seq 最高的 ≤k 条(升序返回,已出账),并把 floor 压到取出段
   * 的最低 seq——上翻补批/物化的口粮。空账返回 [],floor 不动。
   */
  takeTail(k: number): JsonlLinePayload[] {
    if (this.pending.length === 0 || k <= 0) return [];
    if (this.dirty) {
      this.pending.sort((a, b) => a.seq - b.seq);
      this.dirty = false;
    }
    const taken = this.pending.splice(Math.max(0, this.pending.length - k));
    if (taken.length > 0) this.pinFloor(taken[0].seq);
    return taken;
  }

  get pendingCount(): number {
    return this.pending.length;
  }

  /** Tab 关闭时调:pending 持整段历史 payload(大会话数十 MB 级),必须断引用 */
  dispose(): void {
    this.pending = [];
    this.dirty = false;
  }
}
