/**
 * 〔骨架〕**卡正文缓存**：一个 tab 一份，seq → 那一行的载荷，按字节封顶、超了丢最久没用的。
 *
 * 进来的三路（宿主 `tab-stream-view.ts`）：建卡时顺手放（交回占位的卡、滚回来不用再取）· 视口远处预取回来的（`prefetch`）。
 * 出去：物化时先从尾部窗口拿、再从这里拿（`takeRange`，拿走即出账，建卡那一下又放回来），剩下的才按偏移取。
 * 字节按 `JSON.stringify(record).length` 估（字符数，与正文在内存里的量级一致，不求精确）。
 */
import type { JsonlLinePayload } from "./events";

/** 每个 tab 至多缓存多少字节的正文。 */
export const BODY_CACHE_BYTES = 8 * 1024 * 1024;

export class BodyCache {
  /** Map 的插入序就是用的先后：放 · 取都挪到最后。 */
  private rows = new Map<number, { p: JsonlLinePayload; bytes: number }>();
  private used = 0;

  constructor(private readonly cap: number = BODY_CACHE_BYTES) {}

  get size(): number {
    return this.rows.size;
  }

  get bytes(): number {
    return this.used;
  }

  has(seq: number): boolean {
    return this.rows.has(seq);
  }

  /** 放进来（同一个 seq 换成这一份）；超了从最久没用的丢起。一行就超上限的不放。 */
  put(p: JsonlLinePayload): void {
    const bytes = JSON.stringify(p.record).length;
    this.drop(p.seq);
    if (bytes > this.cap) return;
    this.rows.set(p.seq, { p, bytes });
    this.used += bytes;
    for (const [seq] of this.rows) {
      if (this.used <= this.cap) break;
      this.drop(seq);
    }
  }

  /** seq ∈ `[lo, hi)` 的那几行（升序、出账）。 */
  takeRange(lo: number, hi: number): JsonlLinePayload[] {
    const out: JsonlLinePayload[] = [];
    if (this.rows.size === 0) return out;
    for (let s = lo; s < hi; s++) {
      const e = this.rows.get(s);
      if (!e) continue;
      out.push(e.p);
      this.drop(s);
    }
    return out;
  }

  /** 这一行的载荷（不出账，挪到最后）。 */
  get(seq: number): JsonlLinePayload | undefined {
    const e = this.rows.get(seq);
    if (!e) return undefined;
    this.rows.delete(seq);
    this.rows.set(seq, e);
    return e.p;
  }

  clear(): void {
    this.rows.clear();
    this.used = 0;
  }

  private drop(seq: number): void {
    const e = this.rows.get(seq);
    if (!e) return;
    this.rows.delete(seq);
    this.used -= e.bytes;
  }
}
