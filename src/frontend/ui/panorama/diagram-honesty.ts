/**
 * PN1b（`设计/97 §7.2` · CP4）：图下常驻的那一行诚实信号。
 *
 * 数据**只**来自上游 `Diagram.honesty`（公共结构 `Honesty`），本仓不另算。
 * 这里只有一张「字段 → 人话」的表，而这张表的**键集合与上游 `Honesty` 的字段两向相等**
 * （`tests/frontend/ui/panorama/diagram-guards.vitest.ts` 读 vendored 源码取字段，异源）——
 * 上游加一格、这里没跟 ⇒ 红；这里多写一格上游没有 ⇒ 也红。
 *
 * 🔴 `null` ≠ 0：`null` = 这张图不量这一格（如类图不画调用），写「不适用」；
 *    0 = 量了、没有，照写 0。把 `null` 写成 0 会让读者以为「一处都没漏」。
 *
 * 买到：每张图下面都说清看不见 / 分不清 / 滤掉 / 排除 / 省略了多少、读库有没有出错。
 * **买不到**：它不判断这些数「多不多」—— 那是读图的人的事。
 */
import type { Honesty } from "./types";
import { copyText } from "../copy-table";

type Cell = (h: Honesty) => string;

/** 〔CP2b〕不适用 / 有数 两句各自整句进表（不再拼「名 ＋ 数 ＋ 量词」三截碎片）。 */
const count = (v: number | null, na: string, some: (n: number) => string): string => (v === null ? na : some(v));

/** 字段 → 人话。**键集合 == 上游 `Honesty` 的字段**（判据两向钉）。顺序即显示顺序。 */
export const HONESTY_CELLS: Record<keyof Honesty, Cell> = {
  unresolved_calls: (h) =>
    count(h.unresolved_calls, copyText("diagramHonesty.unresolved.na"), (n) => copyText("diagramHonesty.unresolved.some", { n })),
  ambiguous_calls: (h) =>
    count(h.ambiguous_calls, copyText("diagramHonesty.ambiguous.na"), (n) => copyText("diagramHonesty.ambiguous.some", { n })),
  filtered_guess_links: (h) =>
    count(h.filtered_guess_links, copyText("diagramHonesty.filtered.na"), (n) => copyText("diagramHonesty.filtered.some", { n })),
  excluded_test_symbols: (h) =>
    count(h.excluded_test_symbols, copyText("diagramHonesty.excluded.na"), (n) => copyText("diagramHonesty.excluded.some", { n })),
  omitted: (h) =>
    h.omitted === null
      ? copyText("diagramHonesty.omitted.na")
      : copyText("diagramHonesty.omitted.some", {
          nodes: h.omitted.nodes,
          symbols: h.omitted.symbols,
          links: h.omitted.links,
        }),
  db_errors: (h) =>
    h.db_errors.length === 0
      ? copyText("diagramHonesty.dbErrors.none")
      : copyText("diagramHonesty.dbErrors.some", { n: h.db_errors.length }),
};

/** 逐格的人话（按表的顺序）。 */
export function honestyCells(h: Honesty): string[] {
  return (Object.keys(HONESTY_CELLS) as (keyof Honesty)[]).map((k) => HONESTY_CELLS[k](h));
}

/** 常驻的那一行。 */
export function honestyLine(h: Honesty): string {
  return honestyCells(h).join(copyText("diagramHonesty.line.sep"));
}
