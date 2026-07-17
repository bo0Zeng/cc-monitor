/**
 * Edit / Write / MultiEdit 工具调用的行级 diff 渲染（issue #14）。
 *
 * 两层严格分离：
 * - **纯函数层**（本文件上半，DOM-free）：`diffLines` + 三个 input normalizer。
 *   不 import 任何 DOM / render 模块，可被 `diff.test.ts` 用 `npx tsx` 独立单测。
 * - **DOM 层**（下半，见 step 3 追加）：`buildDiffBody` 仅用 createElement + textContent
 *   构建元素 —— 绕开 DOMPurify（sanitize 只作用于 renderMarkdown 的字符串输出），
 *   且三引号 / HTML / CJK / emoji 都按字面安全渲染。
 *
 * 关键约束：
 * - **迭代 LCS，非递归**（INVARIANT §17b）：上千行的 Edit 在 WebView2 上递归会 RangeError。
 *   用 DP 矩阵 + 迭代回溯。再加 cell-budget 守卫，超大输入退化成"整删+整增"而不分配巨矩阵。
 * - **容错 schema**（INVARIANT §18）：`tool_use.input` 是 `unknown`，normalizer 运行时校验
 *   字段类型，任何缺失 / 非字符串一律返 `null`，交由调用方回退 prettyJson。
 */

// F-MA：agent-profile 是纯常量模块（无 DOM/render），不破坏本文件"可 tsx 独立单测"的性质。
import { AGENT_PROFILE } from "../agent-profile";

// === 类型（纯数据，无 DOM） ===

export type DiffRowType = "add" | "del" | "ctx";

export interface DiffRow {
  type: DiffRowType;
  text: string;
  /** 旧文件 1-based 行号；add 行为 null */
  oldNo: number | null;
  /** 新文件 1-based 行号；del 行为 null */
  newNo: number | null;
}

export interface DiffResult {
  rows: DiffRow[];
  /** 是否因 maxLines 截断了 rows（addCount/delCount 仍是全量） */
  truncated: boolean;
  /** 全量新增行数（不受截断影响） */
  addCount: number;
  /** 全量删除行数（不受截断影响） */
  delCount: number;
}

export interface DiffOpts {
  /** 最多发射多少行 row（超出截断，truncated=true）。默认 400。 */
  maxLines?: number;
  /** 单行字符上限（超出截断加 '…'）。默认 2000。 */
  maxCharsPerLine?: number;
  /** m*n 超过此值不分配矩阵，退化为整删+整增。默认 4_000_000。 */
  cellBudget?: number;
}

const DEFAULT_MAX_LINES = 400;
const DEFAULT_MAX_CHARS = 2000;
const DEFAULT_CELL_BUDGET = 4_000_000;

// === 纯函数：归一化 + 切行 ===

/** CRLF / 裸 CR → LF；再剥**恰好一个**尾随 \n（不贪心，blank line 不被吃）。 */
function normalizeText(s: string): string {
  let t = s.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  if (t.endsWith("\n")) t = t.slice(0, -1);
  return t;
}

/** 归一化后切行。空串 → 零行（而非 [""]），让 pure-add / pure-del 干净。 */
function splitLines(normalized: string): string[] {
  return normalized === "" ? [] : normalized.split("\n");
}

/** 单行截断到 cap 字符，超出加 '…'。 */
function capLine(text: string, cap: number): string {
  return text.length > cap ? text.slice(0, cap) + "…" : text;
}

/** 把一组同类型行构造成 DiffResult（用于 all-ctx / all-add / all-del 快路径）。 */
function uniformResult(
  lines: string[],
  type: DiffRowType,
  maxLines: number,
  cap: number,
): DiffResult {
  const rows: DiffRow[] = [];
  let truncated = false;
  for (let i = 0; i < lines.length; i++) {
    if (rows.length >= maxLines) {
      truncated = true;
      break;
    }
    const no = i + 1;
    rows.push({
      type,
      text: capLine(lines[i], cap),
      oldNo: type === "add" ? null : no,
      newNo: type === "del" ? null : no,
    });
  }
  return {
    rows,
    truncated,
    addCount: type === "add" ? lines.length : 0,
    delCount: type === "del" ? lines.length : 0,
  };
}

/**
 * 行级 diff。迭代 LCS（DP + 迭代回溯，无递归 — INVARIANT §17b）。
 *
 * 归一化（CRLF / 单尾随换行）后比较，所以"仅行尾风格不同"不会误报整文件 diff。
 * 快路径：完全相同 → 全 ctx；旧空 → 全 add；新空 → 全 del。
 * 守卫：m*n 超 cellBudget → 退化为整删+整增（仍可读，避免巨矩阵分配 / 卡顿）。
 */
export function diffLines(
  oldStr: string,
  newStr: string,
  opts?: DiffOpts,
): DiffResult {
  const maxLines = opts?.maxLines ?? DEFAULT_MAX_LINES;
  const cap = opts?.maxCharsPerLine ?? DEFAULT_MAX_CHARS;
  const cellBudget = opts?.cellBudget ?? DEFAULT_CELL_BUDGET;

  const oldNorm = normalizeText(oldStr);
  const newNorm = normalizeText(newStr);

  if (oldNorm === newNorm) {
    return uniformResult(splitLines(oldNorm), "ctx", maxLines, cap);
  }

  const oldLines = splitLines(oldNorm);
  const newLines = splitLines(newNorm);
  const m = oldLines.length;
  const n = newLines.length;

  if (m === 0) return uniformResult(newLines, "add", maxLines, cap);
  if (n === 0) return uniformResult(oldLines, "del", maxLines, cap);

  // cell-budget 守卫：在分配矩阵**之前**判断，超限退化为整删+整增。
  if (m * n > cellBudget) {
    const rows: DiffRow[] = [];
    let truncated = false;
    for (let i = 0; i < m && !truncated; i++) {
      if (rows.length >= maxLines) {
        truncated = true;
        break;
      }
      rows.push({ type: "del", text: capLine(oldLines[i], cap), oldNo: i + 1, newNo: null });
    }
    for (let j = 0; j < n && !truncated; j++) {
      if (rows.length >= maxLines) {
        truncated = true;
        break;
      }
      rows.push({ type: "add", text: capLine(newLines[j], cap), oldNo: null, newNo: j + 1 });
    }
    return { rows, truncated, addCount: n, delCount: m };
  }

  // DP：C[i*(n+1)+j] = LCS(oldLines[0..i), newLines[0..j))。Int32 足够计行数。
  const width = n + 1;
  const C = new Int32Array((m + 1) * width);
  for (let i = 1; i <= m; i++) {
    const oi = oldLines[i - 1];
    const rowBase = i * width;
    const prevBase = (i - 1) * width;
    for (let j = 1; j <= n; j++) {
      if (oi === newLines[j - 1]) {
        C[rowBase + j] = C[prevBase + (j - 1)] + 1;
      } else {
        const up = C[prevBase + j];
        const left = C[rowBase + (j - 1)];
        C[rowBase + j] = up >= left ? up : left;
      }
    }
  }

  // 迭代回溯（reversed 收集，最后 reverse）。
  const ops: DiffRow[] = [];
  let i = m;
  let j = n;
  while (i > 0 && j > 0) {
    if (oldLines[i - 1] === newLines[j - 1]) {
      ops.push({ type: "ctx", text: oldLines[i - 1], oldNo: i, newNo: j });
      i--;
      j--;
    } else if (C[i * width + (j - 1)] >= C[(i - 1) * width + j]) {
      // 平局优先 add（left）：回溯是逆序收集再 reverse，逆序先收 add → 正序里
      // del 落在 add 前面，得到惯例的"红删在上、绿增在下"。
      ops.push({ type: "add", text: newLines[j - 1], oldNo: null, newNo: j });
      j--;
    } else {
      ops.push({ type: "del", text: oldLines[i - 1], oldNo: i, newNo: null });
      i--;
    }
  }
  while (i > 0) {
    ops.push({ type: "del", text: oldLines[i - 1], oldNo: i, newNo: null });
    i--;
  }
  while (j > 0) {
    ops.push({ type: "add", text: newLines[j - 1], oldNo: null, newNo: j });
    j--;
  }
  ops.reverse();

  // 全量计数（不受截断影响），再按 maxLines 截断发射 + 单行截断。
  let addCount = 0;
  let delCount = 0;
  for (const op of ops) {
    if (op.type === "add") addCount++;
    else if (op.type === "del") delCount++;
  }
  const rows: DiffRow[] = [];
  let truncated = false;
  for (const op of ops) {
    if (rows.length >= maxLines) {
      truncated = true;
      break;
    }
    rows.push({ ...op, text: capLine(op.text, cap) });
  }
  return { rows, truncated, addCount, delCount };
}

// === 纯函数：input normalizer（运行时校验 unknown → 旧/新文本对） ===

export interface OldNew {
  old: string;
  new: string;
}

function asRecord(input: unknown): Record<string, unknown> | null {
  return input && typeof input === "object" ? (input as Record<string, unknown>) : null;
}

/** Edit: {old_string, new_string}。任一缺失 / 非字符串 → null。 */
export function normalizeEditInput(input: unknown): OldNew | null {
  const r = asRecord(input);
  if (!r) return null;
  const o = r.old_string;
  const n = r.new_string;
  if (typeof o !== "string" || typeof n !== "string") return null;
  return { old: o, new: n };
}

/** Write: {content} → 整块新增（old=''）。content 非字符串 → null。 */
export function normalizeWriteInput(input: unknown): OldNew | null {
  const r = asRecord(input);
  if (!r) return null;
  const c = r.content;
  if (typeof c !== "string") return null;
  return { old: "", new: c };
}

/** MultiEdit: {edits:[{old_string,new_string}]}。非数组 / 空 / 任一项不合规 → null（整卡回退）。 */
export function normalizeMultiEditInput(input: unknown): OldNew[] | null {
  const r = asRecord(input);
  if (!r) return null;
  const edits = r.edits;
  if (!Array.isArray(edits) || edits.length === 0) return null;
  const out: OldNew[] = [];
  for (const e of edits) {
    const er = asRecord(e);
    if (!er) return null;
    const o = er.old_string;
    const n = er.new_string;
    if (typeof o !== "string" || typeof n !== "string") return null;
    out.push({ old: o, new: n });
  }
  return out;
}

// === DOM 层：仅 createElement + textContent（绕开 DOMPurify） ===
//
// 这一段是唯一碰 DOM 的代码。纯函数层（上方）import 不到这里，故 diff.test.ts
// 能在无 DOM 的 node 下独立单测纯逻辑。buildDiffBody 用 document，只能手测（step 6）。

/** 哪些工具走 diff 渲染。NotebookEdit **不在内**（v1 回退 raw JSON，0 真实样本）。 */
export function isDiffTool(name: string): boolean {
  return AGENT_PROFILE.diffTools.has(name);
}

/**
 * 纯（无 DOM）：按工具名把 `unknown` input 归一化成 diff 段（old/new 对数组）。
 * 未知工具 / 任一畸形 → null（调用方回退 prettyJson）。可被 node 单测。
 */
export function diffSegments(toolName: string, input: unknown): OldNew[] | null {
  if (toolName === "Edit") {
    const e = normalizeEditInput(input);
    return e ? [e] : null;
  }
  if (toolName === "Write") {
    const w = normalizeWriteInput(input);
    return w ? [w] : null;
  }
  if (toolName === "MultiEdit") {
    return normalizeMultiEditInput(input);
  }
  return null;
}

function gutterMark(type: DiffRowType): string {
  if (type === "add") return "+";
  if (type === "del") return "-";
  return "";
}

/** 把一个 DiffResult 的行追加到 root。 */
function appendDiffRows(root: HTMLElement, result: DiffResult): void {
  for (const row of result.rows) {
    const line = document.createElement("div");
    line.className = `block-diff-line block-diff-${row.type}`;

    const gutter = document.createElement("span");
    gutter.className = "block-diff-gutter";
    gutter.textContent = gutterMark(row.type);

    const code = document.createElement("span");
    code.className = "block-diff-code";
    // textContent 自动转义：三引号 / HTML / 尖括号 / CJK / emoji 一律按字面安全渲染，
    // DOMPurify 永不介入（sanitize 只作用于 renderMarkdown 的字符串输出）。
    code.textContent = row.text;

    line.appendChild(gutter);
    line.appendChild(code);
    root.appendChild(line);
  }
}

interface RenderTally {
  totalRows: number;
  truncated: boolean;
  addCount: number;
  delCount: number;
}

/** 把所有 diff 段渲染进 root（MultiEdit 多段加 "Edit N" 标签），统计行数/增删/截断。 */
function renderSegmentsInto(root: HTMLElement, segments: OldNew[], opts?: DiffOpts): RenderTally {
  const multi = segments.length > 1;
  const tally: RenderTally = { totalRows: 0, truncated: false, addCount: 0, delCount: 0 };
  for (let i = 0; i < segments.length; i++) {
    if (multi) {
      const label = document.createElement("div");
      label.className = "block-diff-edit-label";
      label.textContent = `Edit ${i + 1}`;
      root.appendChild(label);
    }
    const res = diffLines(segments[i].old, segments[i].new, opts);
    appendDiffRows(root, res);
    tally.totalRows += res.rows.length;
    tally.addCount += res.addCount;
    tally.delCount += res.delCount;
    if (res.truncated) tally.truncated = true;
  }
  return tally;
}

/** "显示完整 diff" 用：解除行数上限（单行字符上限仍保留，防单条巨行）。 */
const FULL_OPTS: DiffOpts = { maxLines: Number.MAX_SAFE_INTEGER };

/**
 * 构建 diff 折叠条 body 元素。仅 createElement + textContent，**从不** innerHTML / renderMarkdown。
 *
 * 任何异常 / 畸形 input / 未知工具 / 无可显示内容（如空 Write）→ 返 null，
 * 调用方回退现有 prettyJson `<pre>`（INVARIANT §17a / §18）。返回单个 root 元素
 * （非 fragment），便于以 `wrap.insertBefore(el, wrap.firstChild)` 替换原 `<pre>`。
 *
 * 截断时附 "显示完整 diff" 按钮（复用 .block-body-show-full 样式）：用户点击（一次）后
 * 整体重渲染完整内容 —— 这发生在主动展开之后、脱离 lazy/replay 期，不影响 §21 滚动稳定性。
 */
export function buildDiffBody(toolName: string, input: unknown): HTMLElement | null {
  try {
    const segments = diffSegments(toolName, input);
    if (!segments || segments.length === 0) return null;

    const root = document.createElement("div");
    root.className = "block-body block-args block-diff";

    const tally = renderSegmentsInto(root, segments);
    // 没有任何 diff 行可显示（如 content='' 的空 Write）→ 回退 prettyJson 更有信息量。
    if (tally.totalRows === 0) return null;

    if (tally.truncated) {
      const btn = document.createElement("button");
      btn.className = "block-body-show-full";
      btn.textContent = `↕ 显示完整 diff（+${tally.addCount} −${tally.delCount}）`;
      btn.addEventListener(
        "click",
        () => {
          // 点击在事件回调里、不在 seam 的 try/catch 内：自守，完整重渲染失败就退回
          // 截断视图，绝不因抛错留下空卡。
          try {
            root.replaceChildren();
            renderSegmentsInto(root, segments, FULL_OPTS);
          } catch {
            root.replaceChildren();
            renderSegmentsInto(root, segments);
          }
        },
        { once: true },
      );
      root.appendChild(btn);
    }
    return root;
  } catch {
    return null;
  }
}
