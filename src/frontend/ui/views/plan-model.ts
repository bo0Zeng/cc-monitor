/**
 * 计划页的排版模型（纯函数，零 DOM）：把 `plan-read` 的一片排成大纲的行、概览的几张表。
 *
 * 只排版：状态、原因、接手是谁都是后端给的成品；这里只做「按排期先后摆」「按过滤藏」「按标题找」「按时间排签收」。
 */
import { copyText } from "../copy-table";
import type { PlanBlock, PlanCell, PlanSign, PlanSlice } from "../plan-reads";

/** 三种状态画哪一种（图标 · 颜色）：照后端给的码；认不出的状态当没做完画（字照出）。 */
export type StatusLook = "done" | "open" | "dropped";

export function statusLook(c: Pick<PlanCell, "statusCode">): StatusLook {
  return c.statusCode ?? "open";
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
  /** 是某一块的根格 ⇒ 那一块（行尾挂接手 ＋ 阶段）。 */
  block: PlanBlock | null;
  /** 有 agent 站在这一格（某一块的 `at` 是它）。 */
  standing: boolean;
}

/** 行尾那一个原因短词：没签 · d/m · 等收下（等收下用正文色）。后端没拆好 ⇒ 照出 pb 的原话。 */
export function whyShort(c: PlanCell): { text: string; wait: boolean } | null {
  const w = c.whyCode;
  if (w?.kind === "nosign") return { text: copyText("plan.whyShort.nosign"), wait: false };
  if (w?.kind === "upper") return { text: copyText("plan.whyShort.upper"), wait: true };
  if (w?.kind === "inside") return { text: copyText("plan.whyShort.inside", { done: w.done, of: w.of }), wait: false };
  return c.why ? { text: c.why, wait: false } : null;
}

/** 一片里按编号找格。 */
export function cellIndex(slice: PlanSlice): Map<string, PlanCell> {
  return new Map(slice.cells.map((c) => [c.id, c]));
}

/** 格 ⇒ 以它为根的那一块（`blocks[].cells` 列的是块根）。顶块 `project` 不挂在格上。 */
export function blockRoots(slice: PlanSlice): Map<string, PlanBlock> {
  const m = new Map<string, PlanBlock>();
  for (const b of slice.blocks) {
    if (b.id === "project") continue;
    for (const c of b.cells) m.set(c, b);
  }
  return m;
}

/** 顶块（pb 里 id 叫 `project` 的那一块）。 */
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
  /** 不做了的藏起。 */
  hideDropped: boolean;
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
    if (opts.hideDropped && statusLook(c) === "dropped") return false;
    if (opts.filter === "open" && statusLook(c) !== "open") return false;
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
    if (opts.hideDropped && statusLook(c) === "dropped") {
      memo.set(id, false);
      return false;
    }
    const v = keep(c) || c.children.some(wanted);
    memo.set(id, v);
    return v;
  };
  const out: OutlineRow[] = [];
  const walk = (id: string, depth: number): void => {
    const c = byId.get(id);
    if (!c) return;
    if (narrowing ? !wanted(id) : opts.hideDropped && statusLook(c) === "dropped") return;
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

/** 直接子里做完了几格 / 一共几格（不做了的不算）。 */
export function kidsDone(slice: PlanSlice, cell: PlanCell): { done: number; of: number } {
  const byId = cellIndex(slice);
  let done = 0;
  let of = 0;
  for (const k of cell.children) {
    const c = byId.get(k);
    if (!c) continue;
    const look = statusLook(c);
    if (look === "dropped") continue;
    of++;
    if (look === "done") done++;
  }
  return { done, of };
}

export interface SignItem {
  cell: PlanCell;
  sign: PlanSign;
}

/** 全片的签收，新的在上（`at` 是 pb 给的 ISO 时刻；排不出时刻的排后）。 */
export function signsNewestFirst(slice: PlanSlice): SignItem[] {
  const all: SignItem[] = [];
  for (const c of slice.cells) for (const g of c.signs) all.push({ cell: c, sign: g });
  const t = (x: SignItem): number => (x.sign.at ? Date.parse(x.sign.at) || 0 : 0);
  return all.sort((a, b) => t(b) - t(a));
}

/** 一格最后一条签收（作数的那条）。 */
export function lastSign(cell: PlanCell): PlanSign | null {
  return cell.signs.length > 0 ? cell.signs[cell.signs.length - 1] : null;
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
