/**
 * 〔拆 `tabs.ts` ④〕**拖动排序 / 拖动成组的落点算术** —— 全是纯函数，判据直接打在它们身上。
 *
 * 零 DOM、零 IPC：量尺寸（`getBoundingClientRect`）与挂监听是 `tab-bar-drag.ts` 的事，
 * 这里只回答「栏里这几行 ＋ 这个指针位置 ⇒ 落在哪」「这次落点对顺序与组意味着什么」。
 *
 * 模型：一个顺序（`orderedIds`），组与散的标签页混排在里面；**每个组的组员永远挨着**，
 * 组在栏里的位置 ＝ 它第一个组员的位置（组自己不另存位置）。读回来的顺序组员不挨着 ⇒ 按 [`contiguous`] 聚到第一个组员那一格。
 */
import { copyText } from "./copy-table";

/** 停留多久才算「压住」（建组 / 进组）。只在一行的中间一半攒。 */
export const DWELL_MS = 400;
/** 停留期间允许的抖动；超过就清零重计。 */
export const DWELL_MOVE_PX = 4;
/** 指针移动超过这么多才算「拖」，否则是点击。 */
export const DRAG_THRESHOLD_PX = 4;

/**
 * 把每个组的组员聚到它第一个组员那一格（组员之间、组外的相对次序都不变）。
 * `known` 之外的组 id（组表里已没有）⇒ 当散的。
 */
export function contiguous(
  order: readonly string[],
  groupOf: (sid: string) => string | null,
  known: ReadonlySet<string>,
): string[] {
  const members = new Map<string, string[]>();
  for (const sid of order) {
    const g = groupOf(sid);
    if (g === null || !known.has(g)) continue;
    const list = members.get(g);
    if (list) list.push(sid);
    else members.set(g, [sid]);
  }
  const out: string[] = [];
  const done = new Set<string>();
  for (const sid of order) {
    const g = groupOf(sid);
    if (g === null || !known.has(g)) out.push(sid);
    else if (!done.has(g)) {
      done.add(g);
      out.push(...members.get(g)!);
    }
  }
  return out;
}

/** 栏里的一行：组头（`collapsed` ＝ 收着）或标签页（`hidden` ＝ 在收着的组里，不显示）。 */
export type BarRow =
  | { kind: "head"; gid: string; collapsed?: true }
  | { kind: "tab"; sid: string; gid: string | null; hidden?: true };

/**
 * 栏里从上到下的行：组头紧在它第一个组员之前；一个组员都还没到的组（重启后等组员）⇒ 组头排在最后。
 * 数字键 · `]` `[` · 关掉当前标签页后落到哪 · Shift 连选读的「看到的顺序」就是这里的标签页那几行。
 */
export function barRows(
  order: readonly string[],
  groupOf: (sid: string) => string | null,
  groups: readonly string[],
  collapsed: ReadonlySet<string> = new Set(),
): BarRow[] {
  const known = new Set(groups);
  const rows: BarRow[] = [];
  const seen = new Set<string>();
  const head = (gid: string): BarRow => (collapsed.has(gid) ? { kind: "head", gid, collapsed: true } : { kind: "head", gid });
  for (const sid of contiguous(order, groupOf, known)) {
    const g = groupOf(sid);
    const gid = g !== null && known.has(g) ? g : null;
    if (gid !== null && !seen.has(gid)) {
      seen.add(gid);
      rows.push(head(gid));
    }
    rows.push(gid !== null && collapsed.has(gid) ? { kind: "tab", sid, gid, hidden: true } : { kind: "tab", sid, gid });
  }
  for (const gid of groups) if (!seen.has(gid)) rows.push(head(gid));
  return rows;
}

/** 一行在纵轴上占的那一段（标签页：`id` ＝ sid；组头：`id` ＝ 组 id）。判据直接喂这个，不必先造真布局。 */
export interface RowRect {
  kind: "tab" | "head";
  id: string;
  /** 这一行属于哪个组（散的 ⇒ `null`；组头 ⇒ 它自己）。 */
  gid: string | null;
  top: number;
  /** 在收着的组里的标签页量出来是 0 ⇒ 不参与落点。 */
  height: number;
  /** 组头：收着。 */
  collapsed?: boolean;
}

/** 插到某个标签页前 / 后；`null` ＝ 末尾。 */
export type InsertAt = { sid: string; side: "before" | "after" } | null;

/**
 * 落点。
 * - `insert`：插到 `at`，落下后属于 `gid`（`null` ＝ 散的）。`head` ＝ 指针在那个组头上（组头整块高亮、不画线）。
 * - `onto`：中间停够了 ⇒ 和它建组（它是散的）/ 进它的组（它在组里）。
 */
export type DropTarget =
  | { kind: "insert"; at: InsertAt; gid: string | null; head?: string }
  | { kind: "onto"; sid: string };

const inRow = (r: RowRect, y: number): boolean => y >= r.top && y < r.top + r.height;
const visible = (rows: readonly RowRect[]): RowRect[] => rows.filter((r) => r.height > 0).sort((a, b) => a.top - b.top);

/** 停留能攒在谁身上：指针在某个（没被拖的）标签页中间一半里 ⇒ 它；别处 ⇒ `null`。 */
export function dwellCandidate(rows: readonly RowRect[], y: number, dragged: ReadonlySet<string>): string | null {
  for (const r of visible(rows)) {
    if (r.kind !== "tab" || !inRow(r, y)) continue;
    const q = (y - r.top) / r.height;
    return q >= 0.25 && q < 0.75 && !dragged.has(r.id) ? r.id : null;
  }
  return null;
}

/**
 * 算落点：上 1/4 插前 · 中 1/2 停够才建组 / 进组 · 下 1/4 插后；组头上 1/3 插到组前（组外）、下 2/3 进组排第一；
 * 行间的空 ＝ 上一行之后、组外；最后一行以下 ＝ 末尾。三段的唯一判定处。
 *
 * @param dwellArmed 停留已经攒满的那一行（`null` ＝ 还没满）。攒满由计时器判；「指针还在它中段」在这里再判一次，
 *   两个来源都同意才给 `onto`。
 */
export function pickDropTarget(
  rows: readonly RowRect[],
  y: number,
  dragged: ReadonlySet<string>,
  dwellArmed: string | null,
): DropTarget {
  const sorted = visible(rows);
  const end: DropTarget = { kind: "insert", at: null, gid: null };
  if (sorted.length === 0) return end;
  // 组员按全部行找（收着的组员藏着、量出来高 0，但顺序里有它们）。
  const members = (gid: string): string[] => rows.filter((r) => r.kind === "tab" && r.gid === gid).map((r) => r.id);
  const onHead = (r: RowRect, q: number): DropTarget => {
    const m = members(r.id);
    if (r.collapsed) return { kind: "insert", at: m.length === 0 ? null : { sid: m[m.length - 1], side: "after" }, gid: r.id, head: r.id };
    if (q < 1 / 3) return m.length === 0 ? end : { kind: "insert", at: { sid: m[0], side: "before" }, gid: null };
    return { kind: "insert", at: m.length === 0 ? null : { sid: m[0], side: "before" }, gid: r.id, head: r.id };
  };
  const row = sorted.find((r) => inRow(r, y)) ?? (y < sorted[0].top ? sorted[0] : null);
  if (row === null) {
    // 行与行之间的空（组下沿那 6px）⇒ 上一行之后、组外；最后一行以下 ⇒ 末尾。
    const prev = [...sorted].reverse().find((r) => r.top + r.height <= y);
    const last = sorted[sorted.length - 1];
    if (!prev || prev === last) return end;
    if (prev.kind === "tab") return { kind: "insert", at: { sid: prev.id, side: "after" }, gid: null };
    const m = members(prev.id); // 收着的组头（或还没有组员的组头）下面 ⇒ 这个组后面
    return m.length === 0 ? end : { kind: "insert", at: { sid: m[m.length - 1], side: "after" }, gid: null };
  }
  const q = Math.max(0, (y - row.top) / row.height);
  if (row.kind === "head") return onHead(row, q);
  if (dwellArmed === row.id && !dragged.has(row.id) && q >= 0.25 && q < 0.75) return { kind: "onto", sid: row.id };
  return { kind: "insert", at: { sid: row.id, side: q < 0.5 ? "before" : "after" }, gid: row.gid };
}

/**
 * 一次落点对**被拖的那几个的组**意味着什么（它们落下后都在同一个组里，或都散着）。
 */
export type GroupMove =
  | { kind: "stay" } // 归属不变（零写盘：拖动是高频动作）
  | { kind: "join"; gid: string } // 进一个已有的组
  | { kind: "found"; with: string } // 与落点那个散的现建一个组
  | { kind: "leave" }; // 移出所在的组，变成散的

export function groupMoveForDrop(
  groupOf: (sid: string) => string | null,
  dragged: readonly string[],
  target: DropTarget,
): GroupMove {
  let dest: string | null;
  if (target.kind === "onto") {
    if (dragged.includes(target.sid)) return { kind: "stay" };
    dest = groupOf(target.sid);
    if (dest === null) return { kind: "found", with: target.sid };
  } else dest = target.gid;
  if (dragged.every((s) => groupOf(s) === dest)) return { kind: "stay" };
  return dest === null ? { kind: "leave" } : { kind: "join", gid: dest };
}

/** 现建的那个组在算顺序时的占位 id（只活在 [`planDrop`] 里）。 */
const FOUNDING = "\u0000found";

/**
 * 一次落下的全部后果：新顺序（组员挨着）＋ 归属怎么变。
 * 先按看到的样子（[`contiguous`]）把被拖的几个拿出来、按原先后放到落点，再按落下后的归属聚一次。
 * 建组 / 进组（`onto`）⇒ 放在目标后面。
 */
export function planDrop(
  order: readonly string[],
  groupOf: (sid: string) => string | null,
  known: ReadonlySet<string>,
  dragged: readonly string[],
  target: DropTarget,
): { order: string[]; move: GroupMove } {
  const base = contiguous(order, groupOf, known);
  if (target.kind === "onto" && dragged.includes(target.sid)) return { order: base, move: { kind: "stay" } };
  const move = groupMoveForDrop(groupOf, dragged, target);
  const at: InsertAt = target.kind === "onto" ? { sid: target.sid, side: "after" } : target.at;
  const set = new Set(dragged);
  const anchor = at === null ? -1 : base.indexOf(at.sid);
  const idx = at === null || anchor < 0 ? base.length : anchor + (at.side === "after" ? 1 : 0);
  const rest = base.filter((s) => !set.has(s));
  const cut = idx - base.slice(0, idx).filter((s) => set.has(s)).length;
  const next = [...rest.slice(0, cut), ...base.filter((s) => set.has(s)), ...rest.slice(cut)];
  const dest =
    move.kind === "found" ? FOUNDING : move.kind === "join" ? move.gid : move.kind === "leave" ? null : undefined;
  const after = (s: string): string | null =>
    set.has(s) && dest !== undefined ? dest : move.kind === "found" && s === move.with ? FOUNDING : groupOf(s);
  return { order: contiguous(next, after, new Set([...known, FOUNDING])), move };
}

/**
 * 拖整个组的落点：只认组外（散的之间 · 两组之间 · 末尾）。压在散的上 ⇒ 上半插前、下半插后；
 * 压在别的组里（组头或组员）⇒ 那个组的前面或后面（按离哪头近），不合并、不嵌套；压在自己组上 ⇒ 原位。`null` ＝ 末尾。
 */
export function pickGroupDropTarget(rows: readonly RowRect[], y: number, gid: string): InsertAt {
  // 一行一段：散的标签页各是一段；一个组（组头 ＋ 看得见的组员）合成一段。
  const units: { top: number; bottom: number; first: string | null; last: string | null; own: boolean }[] = [];
  for (const r of visible(rows)) {
    if (r.kind === "tab" && r.gid === null) {
      units.push({ top: r.top, bottom: r.top + r.height, first: r.id, last: r.id, own: false });
      continue;
    }
    const g = r.gid!;
    const members = rows.filter((x) => x.kind === "tab" && x.gid === g).map((x) => x.id);
    const u = units.find((x) => x.first === (members[0] ?? null) && x.own === (g === gid) && members.length > 0);
    if (u) u.bottom = Math.max(u.bottom, r.top + r.height);
    else units.push({ top: r.top, bottom: r.top + r.height, first: members[0] ?? null, last: members[members.length - 1] ?? null, own: g === gid });
  }
  for (const u of units) {
    if (y >= u.bottom) continue;
    const upper = y < (u.top + u.bottom) / 2 || y < u.top;
    if (u.own || u.first === null) return u.first === null ? null : { sid: u.first, side: "before" };
    return upper ? { sid: u.first, side: "before" } : { sid: u.last!, side: "after" };
  }
  return null;
}

/** 整个组挪到 `at`（组外）：组员按原次序整块搬过去，组还是那个组。 */
export function planGroupDrop(
  order: readonly string[],
  groupOf: (sid: string) => string | null,
  known: ReadonlySet<string>,
  gid: string,
  at: InsertAt,
): string[] {
  const base = contiguous(order, groupOf, known);
  const block = base.filter((s) => groupOf(s) === gid);
  if (block.length === 0 || (at !== null && block.includes(at.sid))) return base;
  const rest = base.filter((s) => groupOf(s) !== gid);
  const anchor = at === null ? -1 : rest.indexOf(at.sid);
  const cut = at === null || anchor < 0 ? rest.length : anchor + (at.side === "after" ? 1 : 0);
  return contiguous([...rest.slice(0, cut), ...block, ...rest.slice(cut)], groupOf, known);
}

/** 拖的时候栏自己滚：指针进列表上 / 下沿 24px 内 ⇒ 每帧滚多少（负 ＝ 往上），离边越近越快、贴边 12px。 */
export const AUTO_SCROLL_EDGE_PX = 24;
export const AUTO_SCROLL_MAX_PX = 12;
export function autoScrollStep(y: number, top: number, bottom: number): number {
  const speed = (d: number): number => Math.round((AUTO_SCROLL_MAX_PX * (AUTO_SCROLL_EDGE_PX - Math.max(0, d))) / AUTO_SCROLL_EDGE_PX);
  if (y < top + AUTO_SCROLL_EDGE_PX) return -speed(y - top);
  if (y > bottom - AUTO_SCROLL_EDGE_PX) return speed(bottom - y);
  return 0;
}

/**
 * 两个 cwd **完全相同** ⇒ 那个目录名；否则 `null`（只共前缀不算：`/work/docs` 与 `/work/cli` 得出「work」没意义）。
 * 两种分隔符都认（客户端常在 Windows 上、会话可能来自 Linux 远端）；盘符（`C:`）不当名字。
 */
export function sameDirName(a: string | null, b: string | null): string | null {
  if (!a || !b) return null;
  const seg = (p: string): string[] => p.split(/[/\\]+/).filter((s) => s !== "");
  const sa = seg(a);
  const sb = seg(b);
  if (sa.length === 0 || sa.length !== sb.length || sa.some((s, i) => s !== sb[i])) return null;
  const last = sa[sa.length - 1];
  return /^[A-Za-z]:$/.test(last) ? null : last;
}

/**
 * 新组叫什么：两个 cwd 相同 ⇒ 目录名；否则「分组 N」（N ＝ 现有默认名的最大号 ＋1）。
 * 建完组头名字框立刻打开（`TabBarView.renameGroupNow`），打字即改。
 */
export function defaultGroupName(
  cwdA: string | null,
  cwdB: string | null,
  existingNames: readonly string[],
): string {
  const dir = sameDirName(cwdA, cwdB);
  if (dir) return dir;
  let max = 0;
  for (const name of existingNames) {
    const n = defaultNameNumber(name);
    if (n !== null) max = Math.max(max, n);
  }
  return copyText("tabDrop.group.defaultName", { n: max + 1 });
}

/** 一个组名是不是默认名、编号几：照「分组 {n}」那一条文案现取模板认（改了文案照样认得），不在代码里写那个字。 */
function defaultNameNumber(name: string): number | null {
  const mark = "\u0000";
  const [pre, post = ""] = copyText("tabDrop.group.defaultName", { n: mark }).split(mark);
  const esc = (x: string) => x.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const m = new RegExp(`^${esc(pre.trim())}\\s*(\\d+)\\s*${esc(post.trim())}$`).exec(name.trim());
  return m ? Number(m[1]) : null;
}
