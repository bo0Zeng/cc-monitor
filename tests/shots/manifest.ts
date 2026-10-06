/**
 * 截图工具的清单页：把场景清单（不含世界与动作）交给 `run.mjs`。
 */
import { SCENES } from "./scenes";

declare global {
  interface Window {
    __shotsManifest?: { id: string; page: string; query: string; dir: string; title: string; desc: string; width: number; height: number; scale?: number }[];
  }
}

window.__shotsManifest = SCENES.map(({ id, page, query, dir, title, desc, width, height, scale }) => ({ id, page, query: query ?? "", dir, title, desc, width, height, scale }));
