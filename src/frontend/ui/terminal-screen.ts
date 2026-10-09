/**
 * 终端画面：后端 `terminal-preview` 那份成品里的每行 `text` ＋ `spans` ⇒ 带颜色的 DOM。抽屉终端页与起会话那一处的浮层同一个画法。
 *
 * - 颜色与属性全由后端解析好（SGR ⇒ 段），这里只照着画，不认任何转义。
 * - `from` / `to` 按字符计（后端按 Unicode 标量数列）⇒ 这边按码点切，不按 UTF-16 下标。
 * - 16 色名（`red` · `bright-black` …）走 `data-fg` / `data-bg`，CSS 映射到 `--term-*`；`#rrggbb` 原样写进内联颜色；认不出的不画。
 * - 反显 ＝ 前景背景对调，缺的那一边用画面的默认色（`default-fg` · `default-bg`）。
 * - 段越界截到行尾；重叠的后一段从前一段结束处起（字一个不丢不重）。
 */
import { isObj } from "./ipc/decode";
import { unreadable } from "./control-said";
import type { Origin } from "./ipc/origin";
import css from "./terminal-screen.module.css";

/** 一段属性（列下标按字符，`from` 含、`to` 不含）。 */
export interface ScreenSpan {
  from: number;
  to: number;
  fg?: string;
  bg?: string;
  bold?: true;
  dim?: true;
  italic?: true;
  underline?: true;
  inverse?: true;
}

/** 一行：字 ＋ 属性段（没颜色 ⇒ 空）。 */
export interface ScreenLine {
  text: string;
  spans: ScreenSpan[];
}

const FLAGS = ["bold", "dim", "italic", "underline", "inverse"] as const;

const BASE = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
/** 认得的颜色名：8 色 ＋ 亮 8 色 ＋ 反显时补位的两格默认色。 */
const NAMED = new Set([...BASE, ...BASE.map((c) => `bright-${c}`), "default-fg", "default-bg"]);
const HEX = /^#[0-9a-f]{6}$/i;

function spanOf(v: unknown): ScreenSpan | null {
  if (!isObj(v) || !Number.isInteger(v.from) || !Number.isInteger(v.to)) return null;
  const from = v.from as number;
  const to = v.to as number;
  if (from < 0 || to < from) return null;
  const s: ScreenSpan = { from, to };
  if (typeof v.fg === "string") s.fg = v.fg;
  if (typeof v.bg === "string") s.bg = v.bg;
  for (const f of FLAGS) if (v[f] === true) s[f] = true;
  return s;
}

/** `terminal-preview` 的成品 ⇒ 每一行。`lines` 缺 / 某行没有字符串 `text` / `spans` 不是合法的段 ⇒ 抛。空屏是合法的成功。 */
export function decodeScreenLines(origin: Origin, v: unknown): ScreenLine[] {
  const lines = isObj(v) ? v.lines : undefined;
  if (!Array.isArray(lines)) throw unreadable(origin, "terminal-preview", "has no `lines: [{text, spans?}]`");
  return lines.map((l) => {
    if (!isObj(l) || typeof l.text !== "string") throw unreadable(origin, "terminal-preview", "has no `lines: [{text, spans?}]`");
    if (l.spans === undefined) return { text: l.text, spans: [] };
    if (!Array.isArray(l.spans)) throw unreadable(origin, "terminal-preview", "`spans` is not a list");
    const spans = l.spans.map(spanOf);
    if (spans.some((s) => s === null)) throw unreadable(origin, "terminal-preview", "a span is not `{from, to}`");
    return { text: l.text, spans: spans as ScreenSpan[] };
  });
}

/** 一块画面（`pre`）：只挂画面那一族的字与底；版位与滚动归调用方套在外面的那一格。 */
export function screenPre(): HTMLPreElement {
  const pre = document.createElement("pre");
  pre.className = css.screen;
  return pre;
}

/** 整屏的纯文字（每行用换行接起来）。 */
export function screenText(lines: readonly ScreenLine[]): string {
  return lines.map((l) => l.text).join("\n");
}

/** 一格颜色写到元素上：名字 ⇒ data 属性；真彩 ⇒ 内联颜色；认不出 ⇒ 不画。 */
function setColor(el: HTMLElement, which: "fg" | "bg", c: string | undefined): void {
  if (c === undefined) return;
  if (NAMED.has(c)) el.dataset[which] = c;
  else if (HEX.test(c)) {
    if (which === "fg") el.style.color = c;
    else el.style.backgroundColor = c;
  }
}

function styled(text: string, s: ScreenSpan): HTMLSpanElement {
  const el = document.createElement("span");
  el.textContent = text;
  const fg = s.inverse ? (s.bg ?? "default-bg") : s.fg;
  const bg = s.inverse ? (s.fg ?? "default-fg") : s.bg;
  setColor(el, "fg", fg);
  setColor(el, "bg", bg);
  for (const f of FLAGS) if (f !== "inverse" && s[f]) el.dataset[f] = "";
  return el;
}

/** 一行画成的节点（第二行起头上带那个换行）。 */
function lineNodes(line: ScreenLine, i: number): Node[] {
  const out: Node[] = [];
  if (i > 0) out.push(document.createTextNode("\n"));
  const chars = Array.from(line.text);
  let at = 0;
  for (const s of [...line.spans].sort((a, b) => a.from - b.from)) {
    const from = Math.max(s.from, at);
    const to = Math.min(s.to, chars.length);
    if (to <= from) continue;
    if (from > at) out.push(document.createTextNode(chars.slice(at, from).join("")));
    out.push(styled(chars.slice(from, to).join(""), s));
    at = to;
  }
  if (at < chars.length) out.push(document.createTextNode(chars.slice(at).join("")));
  return out;
}

const lineKey = (line: ScreenLine): string => JSON.stringify([line.text, line.spans]);

/** 每块画面上一次画成什么样（每行的键 ＋ 那一行的节点）：下一帧只换变了的行。 */
const lastPainted = new WeakMap<HTMLElement, { keys: string[]; nodes: Node[][] }>();

/**
 * 把整屏画进 `pre`（替换原有内容）。实时画面一帧一帧来、多半只变几行（底下那一行在动）：
 * 上一帧是这里画的、`pre` 里还是那一份 ⇒ 只换变了的行（没变的行节点原样留着，不整屏拆了重建）；别处动过 ⇒ 整屏重画。
 */
export function renderScreen(pre: HTMLElement, lines: readonly ScreenLine[]): void {
  const keys = lines.map(lineKey);
  const prev = lastPainted.get(pre);
  const flat = prev ? prev.nodes.flat() : null;
  const intact = flat !== null && flat.length === pre.childNodes.length && flat.every((n, i) => pre.childNodes[i] === n);
  if (!prev || !intact) {
    const nodes = lines.map(lineNodes);
    pre.replaceChildren(...nodes.flat());
    lastPainted.set(pre, { keys, nodes });
    return;
  }
  // 每一行之后第一个节点（插新行的锚）：从后往前记
  const nextAfter: (Node | null)[] = new Array(prev.nodes.length + 1).fill(null);
  for (let i = prev.nodes.length - 1; i >= 0; i--) nextAfter[i] = prev.nodes[i + 1]?.[0] ?? nextAfter[i + 1];
  const nodes: Node[][] = [];
  for (let i = 0; i < lines.length; i++) {
    const old = prev.nodes[i];
    if (old !== undefined && prev.keys[i] === keys[i]) {
      nodes.push(old);
      continue;
    }
    const fresh = lineNodes(lines[i], i);
    const anchor = old?.[0] ?? (old !== undefined ? nextAfter[i] : null);
    for (const n of fresh) pre.insertBefore(n, anchor);
    for (const n of old ?? []) n.parentNode?.removeChild(n);
    nodes.push(fresh);
  }
  for (let i = lines.length; i < prev.nodes.length; i++) for (const n of prev.nodes[i]) n.parentNode?.removeChild(n);
  lastPainted.set(pre, { keys, nodes });
}
