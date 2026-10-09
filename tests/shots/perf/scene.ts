/**
 * 性能台架的那一个场景：不截图（不进 `SCENES` 清单），只把一屋子 tab 摆好，量由 `bench.mjs` 从页外驱动。
 */
import type { Scene } from "../scenes/index";
import { sleep, waitCount } from "../scenes/helpers";
import { PERF_TURNS, perfWorld } from "./world";

export const PERF_SCENES: Scene[] = [
  {
    id: "perf-tabs",
    page: "index",
    dir: "性能",
    title: "一屋子 tab",
    desc: "二十几个会话（其中几条是几千条记录的长会话），给性能台架量切换 · 首屏 · 滚动",
    width: 1280,
    height: 800,
    world: perfWorld,
    act: async () => {
      // 开发服务器下 WebKit 头一回按需编译整张模块图要很久 ⇒ 时限放宽（量从安静之后才开始，不算进读数）
      await waitCount("#tab-bar .tab", PERF_TURNS.length, 180_000);
      await sleep(900);
    },
  },
];
