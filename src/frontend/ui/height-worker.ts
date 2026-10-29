/**
 * 〔第二级 · `§6` 步 9〕**估高 Worker**：pretext 在 Worker 里排正文（`OffscreenCanvas` 测字宽），
 * 只回高度；正文过一遍就丢（不留任何一份）。主线程那一侧是 `height-refiner.ts`，一件的形状与组合式住 `height-estimate.ts`
 * （`RefineItem` · `refinedHeight` · `pretextHeight`）。不排定时器、不自己取数：来一批算一批。
 */
import { layout, prepare } from "@chenglou/pretext";
import { pretextHeight, refinedHeight, type RefineItem } from "./height-estimate";

interface Batch {
  id: number;
  items: RefineItem[];
}

const PRETEXT = { layout, prepare };

/** 这一件正文排出来多高（判据 `height-estimate.vitest.ts` 直接量它）。 */
export const measure = (it: RefineItem): number =>
  pretextHeight(PRETEXT, it.text, it.font, it.widthPx, it.lineHeightPx);

self.onmessage = (ev: MessageEvent<Batch>): void => {
  const { id, items } = ev.data;
  const heights = items.map((it) => {
    try {
      return refinedHeight(it, measure);
    } catch {
      return null; // 这一件排不出（病态输入 · 字体串 canvas 不收）⇒ 留第一级
    }
  });
  (self as unknown as Worker).postMessage({ id, heights });
};
