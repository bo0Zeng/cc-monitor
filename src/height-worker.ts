/**
 * 〔RENDER2 · `设计/10 §2.5b` 第二级 · `§6` 步 9〕**估高 Worker**：pretext 在 Worker 里排正文（`OffscreenCanvas` 测字宽），
 * 只回高度；正文过一遍就丢（不留任何一份）。主线程那一侧是 `height-refiner.ts`，一件的形状与组合式住 `height-estimate.ts`
 * （`RefineItem` · `refinedHeight`）。不排定时器、不自己取数：来一批算一批。
 */
import { layout, prepare } from "@chenglou/pretext";
import { refinedHeight, type RefineItem } from "./height-estimate";

interface Batch {
  id: number;
  items: RefineItem[];
}

const measure = (it: RefineItem): number =>
  layout(prepare(it.text, it.font, { whiteSpace: "pre-wrap" }), it.widthPx, it.lineHeightPx).height;

self.onmessage = (ev: MessageEvent<Batch>): void => {
  const { id, items } = ev.data;
  const heights = items.map((it) => {
    try {
      return refinedHeight(it, measure);
    } catch {
      return null; // 这一件排不出（病态输入）⇒ 留第一级
    }
  });
  (self as unknown as Worker).postMessage({ id, heights });
};
