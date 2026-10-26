/**
 * 计划页的排版模型（纯函数，零 DOM）：把 `plan-read` 的一个仓库排成大纲的行、概览的几张表。
 *
 * 只排版：状态、原因、负责人是谁、几格签收了都是后端给的成品；这里只做「按排期先后摆」「按过滤藏」「按标题找」。
 */
import type { PlanBlock, PlanCell, PlanRef, PlanSlice } from "../plan-reads";

/** 状态画哪一种（图标 · 颜色）：照后端给的码；待定还没人定另画一种；类不许记收的值（测试 · 规矩 · 验收记录 …）⇒ 不算活，画淡的。 */
export type StatusLook = "done" | "open" | "undecided" | "none";

export function statusLook(c: Pick<PlanCell, "statusCode" | "undecided">): StatusLook {
  if (c.undecided) return "undecided";
  return c.statusCode ?? "none";
}

/** 一格写成「编号 标题」：后端拼好的那一份（`labels`）；表里没有 ⇒ 只写编号。界面不自己拼。 */
export function labelOf(slice: Pick<PlanSlice, "labels">, id: string): string {
  return slice.labels[id] ?? id;
}

/**
 * 一段话按提到的格切开：原文那几段（字符串）与提到格的那几段（`PlanRef`）照先后排。下标是 UTF-16（后端从 pb 的码位换好的），
 * 直接按 JS 字符串切；越界 · 空段 · 与前一段叠着的不算（照原文）。
 */
export function refSpans(text: string, refs: readonly PlanRef[]): (string | PlanRef)[] {
  const out: (string | PlanRef)[] = [];
  let at = 0;
  for (const r of [...refs].sort((a, b) => a.start - b.start)) {
    if (r.start < at || r.end > text.length || r.start >= r.end) continue;
    if (r.start > at) out.push(text.slice(at, r.start));
    out.push(r);
    at = r.end;
  }
  if (at < text.length) out.push(text.slice(at));
  return out;
}

/** 过滤三枚。 */
export type PlanFilter = "all" | "open" | "needs";

export interface OutlineRow {
  cell: PlanCell;
  depth: number;
  /** 有子 ⇒ 折叠箭头。 */
  hasKids: boolean;
  /** 折着。 */
  folded: boolean;
  /** 是某个划出的权力范围的根节点 ⇒ 那个权力范围（行尾挂负责人 ＋ 阶段）。 */
  block: PlanBlock | null;
  /** 有 agent 站在这一格（某个权力范围的 `at` 是它）。 */
  standing: boolean;
}

/** 一片里按编号找格。 */
export function cellIndex(slice: PlanSlice): Map<string, PlanCell> {
  return new Map(slice.cells.map((c) => [c.id, c]));
}

/** 格 ⇒ 以它为根节点的那个权力范围（`blocks[].cells` 列的是根节点）。`project` 不挂在格上。 */
export function blockRoots(slice: PlanSlice): Map<string, PlanBlock> {
  const m = new Map<string, PlanBlock>();
  for (const b of slice.blocks) {
    if (b.id === "project") continue;
    for (const c of b.cells) m.set(c, b);
  }
  return m;
}

/** 最上面那个权力范围（pb 里叫 `project`）。 */
export function topBlock(slice: PlanSlice): PlanBlock | null {
  return slice.blocks.find((b) => b.id === "project") ?? null;
}

function matches(c: PlanCell, q: string): boolean {
  if (!q) return true;
  const n = q.toLowerCase();
  return (c.title ?? "").toLowerCase().includes(n) || c.id.toLowerCase().includes(n);
}

export interface OutlineOpts {
  filter: PlanFilter;
  /** 折着的格（编号）。 */
  folded: ReadonlySet<string>;
  /** 按标题找（编号也能搜到）。 */
  query: string;
  /** 「要你看」那几格（编号）；过滤为 `needs` 时只留它们与它们的上级。 */
  needsCells: ReadonlySet<string>;
}

/**
 * 大纲的行：从顶层格起照排期深度优先摆；过滤 / 找的时候命中的格连同它的上级都留（看得出在哪），
 * 命中格的子孙不另列（除非自己也命中）。折着的格不往下摆（找的时候不管折没折）。
 */
export function outlineRows(slice: PlanSlice, opts: OutlineOpts): OutlineRow[] {
  const byId = cellIndex(slice);
  const roots = blockRoots(slice);
  const standing = new Set(slice.blocks.map((b) => b.at).filter((x): x is string => x !== null));
  const narrowing = opts.filter !== "all" || opts.query !== "";
  const keep = (c: PlanCell): boolean => {
    // 「没签收的」：算活没签收的 ＋ 待定还没人定的。
    if (opts.filter === "open" && statusLook(c) !== "open" && statusLook(c) !== "undecided") return false;
    if (opts.filter === "needs" && !opts.needsCells.has(c.id)) return false;
    return matches(c, opts.query);
  };
  // 找的时候：一格留不留 ＝ 它自己命中，或它底下有命中的。
  const memo = new Map<string, boolean>();
  const wanted = (id: string): boolean => {
    const hit = memo.get(id);
    if (hit !== undefined) return hit;
    const c = byId.get(id);
    if (!c) return false;
    const v = keep(c) || c.children.some(wanted);
    memo.set(id, v);
    return v;
  };
  const out: OutlineRow[] = [];
  const walk = (id: string, depth: number): void => {
    const c = byId.get(id);
    if (!c) return;
    if (narrowing && !wanted(id)) return;
    const kids = c.children.filter((k) => byId.has(k));
    const folded = !narrowing && opts.folded.has(id);
    out.push({ cell: c, depth, hasKids: kids.length > 0, folded, block: roots.get(id) ?? null, standing: standing.has(id) });
    if (folded) return;
    for (const k of kids) walk(k, depth + 1);
  };
  for (const t of slice.top) walk(t, 1);
  return out;
}

/** 一格的「在哪」：从顶层到它的上一级（标题串用）。 */
export function ancestry(slice: PlanSlice, id: string): PlanCell[] {
  const byId = cellIndex(slice);
  const out: PlanCell[] = [];
  let at = byId.get(id)?.parent ?? null;
  while (at) {
    const c = byId.get(at);
    if (!c) break;
    out.unshift(c);
    at = c.parent;
  }
  return out;
}

/** 类在领域表里的先后（颜色按它取，七色循环）；不在表里 ⇒ `null`（画淡色）。 */
export function kindSlot(slice: PlanSlice, kind: string | null): number | null {
  if (kind === null) return null;
  const i = slice.kinds.findIndex((k) => k.name === kind);
  return i < 0 ? null : i % KIND_COLORS;
}

/** 类颜色有几档（令牌 `--plan-kind-0` … `--plan-kind-6`）。 */
export const KIND_COLORS = 7;

/** 认不出那一位的 id 只露前后几位。 */
export function shortId(id: string): string {
  return id.length <= 12 ? id : `${id.slice(0, 6)}…${id.slice(-4)}`;
}

/** 新登记的仓库还没有图：读好了（不是读不成那两种）、一个节点都没有（pb repo 建了 project，等某个会话认领它画图）。 */
export function isFresh(slice: Pick<PlanSlice, "bare" | "error" | "cells">): boolean {
  return !slice.bare && !slice.error && slice.cells.length === 0;
}

/** 圈色的取法：同一套七色，按规矩在图上的先后从第六色起每隔两色取（6 · 2 · 5 · 1 · 4 · 7 · 3），挨着的两圈不同色系。 */
const RING_SLOTS = [5, 1, 4, 0, 3, 6, 2] as const;

/** 一格在哪几道括弧里。 */
export interface Ring {
  /** 写 covers 的那一格（规矩）。 */
  rule: string;
  /** 色位（类色那一套令牌的第几档）。 */
  slot: number;
}

/** 画圈的那几条规矩（照图上的先后）与各自的色位：大纲底下的图例照它列。 */
export function ringRules(slice: Pick<PlanSlice, "cells">): Ring[] {
  return slice.cells.filter((c) => c.covers.some((x) => x !== "project")).map((c, i) => ({ rule: c.id, slot: RING_SLOTS[i % RING_SLOTS.length] }));
}

/**
 * covers 画圈不连线（大纲左侧槽）：格 ⇒ 圈住它的那几道（`coveredBy` 是 pb 算的，连底下整棵）。
 * 只写了 `project` 的规矩不画圈（在它那一格标「管全部」）；圈住的格才有一项。
 */
export function ringsOf(slice: Pick<PlanSlice, "cells">): Map<string, Ring[]> {
  const all = ringRules(slice);
  const rules = all.map((g) => g.rule);
  const slot = new Map(all.map((g) => [g.rule, g.slot]));
  const out = new Map<string, Ring[]>();
  for (const c of slice.cells) {
    const rs = rules.filter((r) => c.coveredBy.includes(r)).map((r) => ({ rule: r, slot: slot.get(r) ?? 0 }));
    if (rs.length > 0) out.set(c.id, rs);
  }
  return out;
}

/** 圈槽里的一道：哪条规矩 · 色位 · 第几列 · 这一段在这一行起 / 收（带钩）· 选中了规矩时亮还是淡。 */
export interface RingMark extends Ring {
  col: number;
  start: boolean;
  end: boolean;
  look: "hi" | "dim" | null;
}

/** 大纲一行的圈槽：几道 ＋ 选中的那条规矩圈住它 ⇒ 铺它的色位（否则 `null`）。 */
export interface RingRow {
  marks: RingMark[];
  tint: number | null;
}

/**
 * 圈槽照此刻看得见的行摆（`rows` 是大纲那几行的编号，过滤 / 折起的不在里面，不占位）：看得见的规矩一条固定一列（照图上先后），
 * 一段连着的行第一行带上钩、末一行带下钩。折起的格：底下被圈住的也算在它这一行（「圈那个框」）。
 * `focus` 是选中的那一格：它是画圈的规矩 ⇒ 它那几道亮、别的淡，圈住的行铺它的色位。
 */
export function ringLanes(slice: Pick<PlanSlice, "cells">, rows: readonly string[], folded: ReadonlySet<string>, focus: string | null): RingRow[] {
  const per = ringsOf(slice);
  const byId = new Map(slice.cells.map((c) => [c.id, c]));
  const under = (id: string, seen: Set<string>): void => {
    for (const k of byId.get(id)?.children ?? []) {
      for (const g of per.get(k) ?? []) seen.add(g.rule);
      under(k, seen);
    }
  };
  const order = ringRules(slice);
  const sets = rows.map((id) => {
    const seen = new Set((per.get(id) ?? []).map((g) => g.rule));
    if (folded.has(id)) under(id, seen);
    return seen;
  });
  const shown = order.filter((g) => sets.some((s) => s.has(g.rule)));
  const lit = order.find((g) => g.rule === focus) ?? null;
  return sets.map((s, i) => ({
    marks: shown
      .map((g, col) => ({ g, col }))
      .filter(({ g }) => s.has(g.rule))
      .map(({ g, col }) => ({
        rule: g.rule,
        slot: g.slot,
        col,
        start: !(sets[i - 1]?.has(g.rule) ?? false),
        end: !(sets[i + 1]?.has(g.rule) ?? false),
        look: lit === null ? null : g.rule === lit.rule ? "hi" : "dim",
      })),
    tint: lit !== null && s.has(lit.rule) ? lit.slot : null,
  }));
}
