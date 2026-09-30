/**
 * 〔RENDER2 · `设计/10 §2.5b` 第二级 · `§6` 步 9〕**第二级估高的主线程那一半**：把记录拆成「正文 ＋ 其余常数」
 * （`height-estimate.ts::refineItemOf`），交 Worker 排（`height-worker.ts`），回来的高换进骨架账本（`SkeletonView.applyRefined`）。
 *
 * 形状照设计：**按需 ＋ 后台** —— 只精算视口上下几屏之内还在占位里的行（`SkeletonView.nearbyUnrefined`），更远的不算；
 * 正文只在 Worker 里过一遍就丢。拿不到 Worker / `OffscreenCanvas`（jsdom · 老 WebKitGTK）⇒ 第二级不开、说一次，第一级照旧。
 * 零定时器：由宿主在滚动 / 骨架接上之后调。
 */
import { refineItemOf, type RefineItem } from "./height-estimate";
import type { JsonlRecord } from "./generated/JsonlRecord";
import type { SkeletonView } from "./skeleton-view";

/** 一批交出去、按同序回高（排不出的那一件回 `null`，留第一级）。 */
export type MeasureBatch = (items: RefineItem[]) => Promise<Array<number | null>>;

/** 生产的量法：一个 Worker（懒建，整个窗口共用）。环境里没有 Worker / `OffscreenCanvas` ⇒ `null`。 */
export function workerMeasure(): MeasureBatch | null {
  if (typeof Worker === "undefined" || typeof OffscreenCanvas === "undefined") return null;
  let worker: Worker | null = null;
  let nextId = 1;
  const waiting = new Map<number, (h: Array<number | null>) => void>();
  return (items) => {
    if (!worker) {
      worker = new Worker(new URL("./height-worker.ts", import.meta.url), { type: "module" });
      worker.onmessage = (ev: MessageEvent<{ id: number; heights: Array<number | null> }>): void => {
        waiting.get(ev.data.id)?.(ev.data.heights);
        waiting.delete(ev.data.id);
      };
      worker.onerror = (): void => {
        // Worker 起不来 / 崩了：在等的全部回「排不出」（留第一级），不挂住宿主的在途标记。
        for (const done of waiting.values()) done([]);
        waiting.clear();
      };
    }
    const id = nextId++;
    return new Promise((resolve) => {
      waiting.set(id, (h) => resolve(items.map((_, i) => h[i] ?? null)));
      worker!.postMessage({ id, items });
    });
  };
}

export class HeightRefiner {
  private said = false;

  constructor(private readonly measure: MeasureBatch | null) {}

  get enabled(): boolean {
    if (!this.measure && !this.said) {
      this.said = true;
      console.info("[height] 这个环境里没有 Worker / OffscreenCanvas ⇒ 第二级估高不开，滚动条只有第一级粗估");
    }
    return this.measure !== null;
  }

  /**
   * 这几行精算、换进账本。不值得精算的（`refineItemOf` 回 `null`）与排不出的留第一级。
   * 〔P3〕算的途中列宽变了（账本 `relayout` 过）⇒ 回来的高是旧列宽下的，整批丢掉、回 `false`（调用方让这几行以后再问）。
   */
  async refine(view: SkeletonView, rows: ReadonlyArray<{ seq: number; rec: JsonlRecord }>): Promise<boolean> {
    if (!this.measure) return true;
    const colW = view.ledger.columnWidth;
    const work: Array<[number, RefineItem]> = [];
    for (const r of rows) {
      const it = refineItemOf(r.rec, colW);
      if (it) work.push([r.seq, it]);
    }
    if (work.length === 0) return true;
    const heights = await this.measure(work.map(([, it]) => it));
    if (view.ledger.columnWidth !== colW) return false;
    const got: Array<[number, number]> = [];
    work.forEach(([seq], i) => {
      const h = heights[i];
      if (typeof h === "number" && Number.isFinite(h)) got.push([seq, h]);
    });
    view.applyRefined(got);
    return true;
  }
}
