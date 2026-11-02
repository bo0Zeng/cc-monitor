/**
 * 正文排版器：后端解析好的块树（`cmd_wire::md::Doc`）→ DOM。界面只照树排，不解析 Markdown、不判高亮类别。
 *
 * - 字一律经 `textContent` / 文本节点落，不拼 HTML 串 ⇒ 正文里的尖括号永远是字，不要清洗。
 * - 代码块沿用全局 `.code-block` 那一套结构（复制按钮走 main.ts 的全局委托）。
 * - 高亮先出字、后上色：一段建出来代码照纯字排，同时把这一段的代码块（先序）交给 `pull`（后端 `prose-hl`），
 *   拉到了 `paint` 把那几块的字换成着色的 span：第 n 块取 `hl[n]`，三元组 `[起点差, 长, 类]`，偏移按 UTF-16 码元（JS 串直接切）。
 *   拉不到 ⇒ 照纯字。
 * - 公式只出占位 `data-math-pending`，进视口由 `render.ts::enhanceCard` 排（KaTeX 只住那一处）。
 * - 长正文按 `segs` 分段：`mountDoc` 只建第 0 段，`buildNext` 一段段往下接。
 */
import type { Doc } from "./generated/Doc";
import type { HlCode } from "./generated/HlCode";
import type { Inl } from "./generated/Inl";
import type { MdAlign } from "./generated/MdAlign";
import type { MdBlock } from "./generated/MdBlock";
import type { MdItem } from "./generated/MdItem";
import type { SegHl } from "./generated/SegHl";
import { copyText } from "./copy-table";
import { el } from "./kit/el";
import s from "./md-view.module.css";

/** 代码块复制钮上的字：每块都一样，不随树里的值变。 */
const COPY_LABEL = copyText("render.codeBlock.copy");

/** 拉一段的高亮：交这一段的代码块（先序），回 `hl[n]` 对第 n 块。 */
export type HlPull = (codes: readonly HlCode[]) => Promise<SegHl>;

/** 建出来的一个代码块：它的语言 · 原文 ＋ 上色时要换字的那个 `<code>`。 */
export interface CodeSlot extends HlCode {
  readonly el: HTMLElement;
}

/** 一串块建出来的：片段 ＋ 里面的代码块（先序）。 */
export interface Built {
  readonly frag: DocumentFragment;
  readonly codes: readonly CodeSlot[];
}

/** 一份排好的正文：`root` 挂进卡里；`buildNext` 接着建下一段，建完了（或本来只有一段）回 false。 */
export interface DocView {
  readonly root: HTMLElement;
  /** 一共几段。 */
  readonly segCount: number;
  /** 已建几段。 */
  built(): number;
  /** 建下一段（代码先照纯字，给了 `pull` 就去拉这一段的高亮、拉到再上色）。 */
  buildNext(): boolean;
}

/** 第 i 段占 `blocks` 的哪一截：`[起, 止)`。 */
export function segRange(doc: Doc, i: number): [number, number] {
  const from = i === 0 ? 0 : (doc.segs[i - 1] ?? doc.blocks.length);
  const to = i < doc.segs.length ? doc.segs[i] : doc.blocks.length;
  return [from, to];
}

/** 建根、排第 0 段；`pull` 不给 ⇒ 代码一直是纯字。 */
export function mountDoc(doc: Doc, pull?: HlPull): DocView {
  const root = document.createElement("div");
  root.className = s.mdDoc;
  const segCount = doc.segs.length + 1;
  let built = 0;
  const view: DocView = {
    root,
    segCount,
    built: () => built,
    buildNext(): boolean {
      if (built >= segCount) return false;
      const [from, to] = segRange(doc, built);
      const { frag, codes } = buildBlocks(doc.blocks.slice(from, to));
      root.append(frag);
      built += 1;
      if (pull && codes.length > 0) {
        // 拉不到（断线 · 那台没这条命令 · 超了上限）就停在纯字：字早已排好，少的只是颜色。
        pull(codes.map(({ lang, x }) => ({ lang, x }))).then(
          (hl) => paint(codes, hl),
          () => undefined,
        );
      }
      return true;
    },
  };
  view.buildNext();
  return view;
}

/** 一串块 → 片段 ＋ 里面的代码块（先序）；代码照纯字排。 */
export function buildBlocks(blocks: readonly MdBlock[]): Built {
  const ctx: Ctx = { codes: [] };
  const frag = document.createDocumentFragment();
  for (const b of blocks) frag.append(block(b, ctx));
  return { frag, codes: ctx.codes };
}

/** 上色：第 n 块代码的字照 `hl[n]` 换成着色的 span（没有那一份 ⇒ 不动）。 */
export function paint(codes: readonly CodeSlot[], hl: SegHl): void {
  codes.forEach((c, n) => {
    const one = hl.hl[n];
    if (one && one.length > 0) c.el.replaceChildren(...highlighted(c.x, one));
  });
}

interface Ctx {
  /** 已建的代码块，先序。 */
  readonly codes: CodeSlot[];
}

/** 一个元素连它的孩子。 */
function box<K extends keyof HTMLElementTagNameMap>(tag: K, kids: readonly Node[] = [], cls = ""): HTMLElementTagNameMap[K] {
  const e = el(tag, cls);
  e.append(...kids);
  return e;
}

function blocksOf(bs: readonly MdBlock[], ctx: Ctx): Node[] {
  return bs.map((b) => block(b, ctx));
}

function block(b: MdBlock, ctx: Ctx): Node {
  switch (b.t) {
    case "p":
      return box("p", inlines(b.k));
    case "h":
      return box(HEADINGS[Math.min(Math.max(b.level, 1), 6) - 1], inlines(b.k));
    case "code":
      return codeBlock(b.lang, b.x, ctx);
    case "math":
      return mathSlot(b.tex, true);
    case "list":
      return list(b.start, b.items, ctx);
    case "quote":
      return box("blockquote", blocksOf(b.blocks, ctx));
    case "alert": {
      const q = box("blockquote", blocksOf(b.blocks, ctx));
      q.className = s.mdAlert;
      q.dataset.kind = b.kind;
      return q;
    }
    case "table":
      return table(b.align, b.head, b.rows);
    case "rule":
      return el("hr");
    case "notes": {
      const ol = box(
        "ol",
        b.items.map((n) => {
          const li = box("li", blocksOf(n.blocks, ctx));
          li.value = n.n;
          return li;
        }),
      );
      const notes = box("section", [ol]);
      notes.className = s.mdNotes;
      return notes;
    }
  }
}

const HEADINGS = ["h1", "h2", "h3", "h4", "h5", "h6"] as const;

function list(start: number | null, items: readonly MdItem[], ctx: Ctx): HTMLElement {
  const lis = items.map((it) => {
    const li = el("li");
    if (it.task !== null) {
      const check = el("input");
      check.type = "checkbox";
      check.disabled = true;
      check.checked = it.task;
      li.className = s.mdTask;
      li.append(check);
    }
    li.append(...blocksOf(it.blocks, ctx));
    return li;
  });
  if (start === null) return box("ul", lis);
  const ol = box("ol", lis);
  if (start !== 1) ol.start = start;
  return ol;
}

function table(align: readonly MdAlign[], head: readonly (readonly Inl[])[], rows: readonly (readonly (readonly Inl[])[])[]): HTMLElement {
  const cell = (tag: "th" | "td", k: readonly Inl[], i: number): HTMLElement => {
    const c = box(tag, inlines(k));
    const a = align[i];
    if (a && a !== "none") c.dataset.align = a;
    return c;
  };
  const thead = box("thead", [box("tr", head.map((k, i) => cell("th", k, i)))]);
  const tbody = box("tbody", rows.map((r) => box("tr", r.map((k, i) => cell("td", k, i)))));
  return box("table", rows.length > 0 ? [thead, tbody] : [thead]);
}

function codeBlock(lang: string | null, x: string, ctx: Ctx): HTMLElement {
  const label = el("span", "code-lang", lang ?? "text");
  const copy = el("button", "code-copy", COPY_LABEL);
  copy.type = "button";
  copy.dataset.copy = "";
  const bar = box("div", [label, copy], "code-bar");
  const code = el("code", s.mdCode, x);
  ctx.codes.push({ lang, x, el: code });
  const wrap = box("div", [bar, box("pre", [code])], "code-block");
  if (lang !== null) wrap.dataset.lang = lang;
  return wrap;
}

/** 照三元组切：没着色的地方出文本节点，着色的出 `<span class=类>`；越界的段按串长截住，认不出的类当没着色。 */
function highlighted(x: string, hl: readonly number[]): Node[] {
  const out: Node[] = [];
  let at = 0;
  for (let i = 0; i + 2 < hl.length; i += 3) {
    const from = Math.min(at + hl[i], x.length);
    const to = Math.min(from + hl[i + 1], x.length);
    if (from > at) out.push(document.createTextNode(x.slice(at, from)));
    if (to > from) out.push(hlSpan(hl[i + 2], x.slice(from, to)) ?? document.createTextNode(x.slice(from, to)));
    at = to;
  }
  if (at < x.length) out.push(document.createTextNode(x.slice(at)));
  return out;
}

/**
 * 着色的一截：`cls` 是 `HlClass` 的序号（`src/common/cmd-wire/src/md.rs`），分支的先后只许跟着那边改；认不出的序号回 null（当没着色）。
 * 类名直接从 CSS 模块对象取、赋给 `className`，不经名单或小工具函数。
 */
function hlSpan(cls: number, text: string): HTMLElement | null {
  const sp = document.createElement("span");
  switch (cls) {
    case 0:
      sp.className = s.hlKeyword;
      break;
    case 1:
      sp.className = s.hlString;
      break;
    case 2:
      sp.className = s.hlComment;
      break;
    case 3:
      sp.className = s.hlNumber;
      break;
    case 4:
      sp.className = s.hlFunction;
      break;
    case 5:
      sp.className = s.hlType;
      break;
    case 6:
      sp.className = s.hlConstant;
      break;
    case 7:
      sp.className = s.hlVariable;
      break;
    case 8:
      sp.className = s.hlAttribute;
      break;
    case 9:
      sp.className = s.hlTag;
      break;
    case 10:
      sp.className = s.hlInserted;
      break;
    case 11:
      sp.className = s.hlDeleted;
      break;
    case 12:
      sp.className = s.hlMeta;
      break;
    default:
      return null;
  }
  sp.textContent = text;
  return sp;
}

/** 公式占位：`render.ts::enhanceCard` 读 `data-math-pending` 与 `textContent` 排 KaTeX。 */
function mathSlot(tex: string, display: boolean): HTMLElement {
  const sp = el("span", "", tex);
  sp.dataset.mathPending = display ? "display" : "inline";
  return sp;
}

function inlines(ks: readonly Inl[]): Node[] {
  return ks.map(inline);
}

function inline(k: Inl): Node {
  if (typeof k === "string") return document.createTextNode(k);
  switch (k.t) {
    case "b":
      return box("strong", inlines(k.k));
    case "i":
      return box("em", inlines(k.k));
    case "del":
      return box("del", inlines(k.k));
    case "c":
      return el("code", "", k.x);
    case "m":
      return mathSlot(k.tex, false);
    case "a": {
      // 树里只有放行的 scheme（http · https · mailto）才成链接 ⇒ 一律交系统浏览器开（main.ts 认 data-external）。
      const a = box("a", inlines(k.k));
      a.href = k.href;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      a.dataset.external = "";
      return a;
    }
    case "img": {
      const img = el("img");
      img.src = k.src;
      img.alt = k.alt;
      img.loading = "lazy";
      img.referrerPolicy = "no-referrer";
      return img;
    }
    case "br":
      return el("br");
    case "fn":
      return el("sup", s.mdFnRef, String(k.n));
  }
}
