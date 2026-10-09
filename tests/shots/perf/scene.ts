/**
 * 性能台架的那一个场景：不截图（不进 `SCENES` 清单），只把一屋子 tab 摆好，量由 `bench.mjs` 从页外驱动。
 */
import type { Scene } from "../scenes/index";
import { sleep, waitCount, waitFor } from "../scenes/helpers";
import { PERF_TURNS, perfWorld } from "./world";
import { sidOf } from "../fake/world";

/** 最长那条会话（`PERF_TURNS[0]`，420 轮）的 sid：查看窗那一项开它。 */
export const LONGEST_SID = sidOf(0x100);

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
  {
    id: "perf-viewer",
    page: "viewer",
    query: `viewer=${sidOf(0x100)}`,
    dir: "性能",
    title: "独立查看窗开最长那条",
    desc: "查看窗整份读一条几千条记录的长会话，量首屏与往上滚",
    width: 1280,
    height: 800,
    world: perfWorld,
    act: async () => {
      await waitFor(".session-viewer [data-uuid]", 180_000);
    },
  },
];
