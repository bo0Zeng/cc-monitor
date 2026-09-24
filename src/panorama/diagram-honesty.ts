/**
 * PN1b（`设计/97 §7.2` · CP4）：图下常驻的那一行诚实信号。
 *
 * 数据**只**来自上游 `Diagram.honesty`（公共结构 `Honesty`），本仓不另算。
 * 这里只有一张「字段 → 人话」的表，而这张表的**键集合与上游 `Honesty` 的字段两向相等**
 * （`tests/panorama/diagram-guards.vitest.ts` 读 vendored 源码取字段，异源）——
 * 上游加一格、这里没跟 ⇒ 红；这里多写一格上游没有 ⇒ 也红。
 *
 * 🔴 `null` ≠ 0：`null` = 这张图不量这一格（如类图不画调用），写「不适用」；
 *    0 = 量了、没有，照写 0。把 `null` 写成 0 会让读者以为「一处都没漏」。
 *
 * 买到：每张图下面都说清看不见 / 分不清 / 滤掉 / 排除 / 省略了多少、读库有没有出错。
 * **买不到**：它不判断这些数「多不多」—— 那是读图的人的事。
 */
import type { DiagramHonesty } from "./types";

type Cell = (h: DiagramHonesty) => string;

const count = (v: number | null, name: string, unit: string): string =>
  v === null ? `${name}：不适用` : `${name} ${v} ${unit}`;

/** 字段 → 人话。**键集合 == 上游 `Honesty` 的字段**（判据两向钉）。顺序即显示顺序。 */
export const HONESTY_CELLS: Record<keyof DiagramHonesty, Cell> = {
  unresolved_calls: (h) => count(h.unresolved_calls, "看不见", "处调用"),
  ambiguous_calls: (h) => count(h.ambiguous_calls, "分不清", "处调用"),
  filtered_guess_links: (h) => count(h.filtered_guess_links, "滤掉", "条全靠名字凑的连接"),
  excluded_test_symbols: (h) => count(h.excluded_test_symbols, "排除", "个测试符号"),
  omitted: (h) =>
    h.omitted === null
      ? "省略：不适用"
      : `省略 ${h.omitted.nodes} 个节点（${h.omitted.symbols} 个符号）、${h.omitted.links} 条连接`,
  db_errors: (h) =>
    h.db_errors.length === 0 ? "读索引没出错" : `读索引出错 ${h.db_errors.length} 处，这张图不完整`,
};

/** 逐格的人话（按表的顺序）。 */
export function honestyCells(h: DiagramHonesty): string[] {
  return (Object.keys(HONESTY_CELLS) as (keyof DiagramHonesty)[]).map((k) => HONESTY_CELLS[k](h));
}

/** 常驻的那一行。 */
export function honestyLine(h: DiagramHonesty): string {
  return honestyCells(h).join(" · ");
}
