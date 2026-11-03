/**
 * 正文排版器：后端解析好的块树（`cmd_wire::md::Doc`）→ DOM。界面只照树排，不解析 Markdown、不判高亮类别。
 *
 * - 字一律经 `textContent` / 文本节点落，不拼 HTML 串 ⇒ 正文里的尖括号永远是字，不要清洗。
 * - 代码块沿用全局 `.code-block` 那一套结构（复制按钮走 main.ts 的全局委托）。
 * - 高亮先出字、后上色：一段建出来代码照纯字排，同时把这一段的代码块（先序）交给 `pull`（后端 `prose-hl`），
 *   拉到了 `paint` 把那几块的字换成着色的 span：第 n 块取 `hl[n]`，三元组 `[起点差, 长, 类]`，偏移按 UTF-16 码元（JS 串直接切）。
 *   拉不到 ⇒ 照纯字。
 * - 公式由核心排好（`MathBox` 画单），这里当场照画成 SVG；排不出的照原文等宽排。
 * - 长正文按 `segs` 分段：`mountDoc` 只建第 0 段，`buildNext` 一段段往下接。
 * - 计划格引用（`ref`）只排编号那几个字、带上 `data-ref-*`，怎么点由宿主接（[`refSlots`]）。
 */
import type { Doc } from "./generated/Doc";
import type { HlCode } from "./generated/HlCode";
import type { Inl } from "./generated/Inl";
import type { MdAlign } from "./generated/MdAlign";
import type { MdBlock } from "./generated/MdBlock";
import type { MdItem } from "./generated/MdItem";
import type { MathBox } from "./generated/MathBox";
import type { MathFont } from "./generated/MathFont";
import type { MathItem } from "./generated/MathItem";
import type { MathPath } from "./generated/MathPath";
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
      return mathSlot(b.tex, b.m, true);
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

/**
 * 一条公式：核心排好了（`m`）⇒ 照画单画成内联 SVG；排不出 ⇒ `<code>` 照原文。
 * SVG 的 viewBox 就是画单的千分之一 em，宽高写成 em ⇒ 跟着所在那一处的字号缩放；颜色一律 `currentColor`（跟正文色）。
 */
function mathSlot(tex: string, m: MathBox | null, display: boolean): HTMLElement {
  const at = display ? "display" : "inline";
  if (!m) {
    const c = el("code", "", tex);
    c.dataset.math = at;
    return c;
  }
  const box = document.createElement(display ? "div" : "span");
  box.dataset.math = at;
  box.setAttribute("role", "img");
  box.setAttribute("aria-label", tex);
  const total = m.h + m.d;
  const svg = svgEl("svg", { width: em(m.w), height: em(total), viewBox: `0 0 ${m.w} ${total}`, "aria-hidden": "true" });
  // 行内的基线对正文的基线：SVG 底边在基线下 `d` 处。
  if (!display) svg.style.verticalAlign = em(-m.d);
  for (const it of m.items) svg.appendChild(mathItem(it));
  box.appendChild(svg);
  if (m.items.some((it) => it.t === "g")) loadMathFonts();
  return box;
}

const SVG_NS = "http://www.w3.org/2000/svg";

function svgEl(tag: string, attrs: Record<string, string | number>): SVGElement {
  const e = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

/** 千分之一 em ⇒ CSS 长度。 */
function em(milli: number): string {
  return `${milli / 1000}em`;
}

function mathItem(it: MathItem): SVGElement {
  switch (it.t) {
    case "g": {
      const t = svgEl("text", { x: it.x, y: it.y, "font-size": it.s, "font-family": MATH_FAMILY[it.f], fill: "currentColor" });
      t.textContent = String.fromCodePoint(it.u);
      return t;
    }
    case "l":
      if (it.dash) {
        return svgEl("line", { x1: it.x, y1: it.y, x2: it.x + it.w, y2: it.y, stroke: "currentColor", "stroke-width": it.th, "stroke-dasharray": `${it.th * 3} ${it.th * 3}` });
      }
      return svgEl("rect", { x: it.x, y: it.y - it.th / 2, width: it.w, height: it.th, fill: "currentColor" });
    case "r":
      return svgEl("rect", { x: it.x, y: it.y, width: it.w, height: it.h, fill: "currentColor" });
    case "p": {
      const paint: Record<string, string> = it.fill ? { fill: "currentColor" } : { fill: "none", stroke: "currentColor" };
      return svgEl("path", { d: it.d.map(pathCmd).join(""), transform: `translate(${it.x} ${it.y})`, ...paint });
    }
  }
}

function pathCmd(c: MathPath): string {
  switch (c.t) {
    case "m":
      return `M${c.x} ${c.y}`;
    case "l":
      return `L${c.x} ${c.y}`;
    case "c":
      return `C${c.x1} ${c.y1} ${c.x2} ${c.y2} ${c.x} ${c.y}`;
    case "q":
      return `Q${c.x1} ${c.y1} ${c.x} ${c.y}`;
    case "z":
      return "Z";
  }
}

/** 画单里的字体 ⇒ 字体族名（每份 TTF 一族，族名就是文件名去掉扩展名）；`sys` 交系统字体。 */
const MATH_FAMILY: Record<MathFont, string> = {
  ams: "KaTeX_AMS-Regular",
  caligraphic_regular: "KaTeX_Caligraphic-Regular",
  fraktur_regular: "KaTeX_Fraktur-Regular",
  fraktur_bold: "KaTeX_Fraktur-Bold",
  main_regular: "KaTeX_Main-Regular",
  main_bold: "KaTeX_Main-Bold",
  main_italic: "KaTeX_Main-Italic",
  main_bold_italic: "KaTeX_Main-BoldItalic",
  math_italic: "KaTeX_Math-Italic",
  math_bold_italic: "KaTeX_Math-BoldItalic",
  sans_serif_regular: "KaTeX_SansSerif-Regular",
  sans_serif_bold: "KaTeX_SansSerif-Bold",
  sans_serif_italic: "KaTeX_SansSerif-Italic",
  script_regular: "KaTeX_Script-Regular",
  size1: "KaTeX_Size1-Regular",
  size2: "KaTeX_Size2-Regular",
  size3: "KaTeX_Size3-Regular",
  size4: "KaTeX_Size4-Regular",
  typewriter: "KaTeX_Typewriter-Regular",
  sys: "serif",
};

/** 字体文件（两端共用 `src/shared/fonts/katex/` 那一份；`new URL` 的字面量路径让构建把它们当资源打进产物）。 */
const MATH_FONT_URL: Record<Exclude<MathFont, "sys">, URL> = {
  ams: new URL("../../shared/fonts/katex/KaTeX_AMS-Regular.ttf", import.meta.url),
  caligraphic_regular: new URL("../../shared/fonts/katex/KaTeX_Caligraphic-Regular.ttf", import.meta.url),
  fraktur_regular: new URL("../../shared/fonts/katex/KaTeX_Fraktur-Regular.ttf", import.meta.url),
  fraktur_bold: new URL("../../shared/fonts/katex/KaTeX_Fraktur-Bold.ttf", import.meta.url),
  main_regular: new URL("../../shared/fonts/katex/KaTeX_Main-Regular.ttf", import.meta.url),
  main_bold: new URL("../../shared/fonts/katex/KaTeX_Main-Bold.ttf", import.meta.url),
  main_italic: new URL("../../shared/fonts/katex/KaTeX_Main-Italic.ttf", import.meta.url),
  main_bold_italic: new URL("../../shared/fonts/katex/KaTeX_Main-BoldItalic.ttf", import.meta.url),
  math_italic: new URL("../../shared/fonts/katex/KaTeX_Math-Italic.ttf", import.meta.url),
  math_bold_italic: new URL("../../shared/fonts/katex/KaTeX_Math-BoldItalic.ttf", import.meta.url),
  sans_serif_regular: new URL("../../shared/fonts/katex/KaTeX_SansSerif-Regular.ttf", import.meta.url),
  sans_serif_bold: new URL("../../shared/fonts/katex/KaTeX_SansSerif-Bold.ttf", import.meta.url),
  sans_serif_italic: new URL("../../shared/fonts/katex/KaTeX_SansSerif-Italic.ttf", import.meta.url),
  script_regular: new URL("../../shared/fonts/katex/KaTeX_Script-Regular.ttf", import.meta.url),
  size1: new URL("../../shared/fonts/katex/KaTeX_Size1-Regular.ttf", import.meta.url),
  size2: new URL("../../shared/fonts/katex/KaTeX_Size2-Regular.ttf", import.meta.url),
  size3: new URL("../../shared/fonts/katex/KaTeX_Size3-Regular.ttf", import.meta.url),
  size4: new URL("../../shared/fonts/katex/KaTeX_Size4-Regular.ttf", import.meta.url),
  typewriter: new URL("../../shared/fonts/katex/KaTeX_Typewriter-Regular.ttf", import.meta.url),
};

/** 头一回画公式时把那几族字体注册进文档（浏览器到真用上某一族才去取那一份）；没公式的窗口一份都不注册。 */
const mathFonts = { added: false };

function loadMathFonts(): void {
  if (mathFonts.added || typeof FontFace === "undefined") return;
  mathFonts.added = true;
  for (const [f, url] of Object.entries(MATH_FONT_URL) as [Exclude<MathFont, "sys">, URL][]) {
    document.fonts.add(new FontFace(MATH_FAMILY[f], `url(${url.href})`));
  }
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
      return mathSlot(k.tex, k.m, false);
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
    case "ref": {
      // 计划格引用：编号原样是字；怎么点、标题怎么摆是宿主的事（回复正文跟一枚淡色标题 · 计划页换成格链接），宿主照这几个属性认。
      const r = el("span", "", k.x);
      r.dataset.refIds = k.ids.join(" ");
      r.dataset.refLabel = k.label;
      return r;
    }
  }
}

/** 树里排出来的计划格引用（[`inline`] 的 `ref` 那一种）：编号的那几个字 · 那几格 · 后端写好的标题（空 ⇒ 宿主照编号写）。 */
export interface RefSlot {
  readonly el: HTMLElement;
  readonly ids: readonly string[];
  readonly label: string;
}

/** `root` 里排出来的计划格引用，照先后。 */
export function refSlots(root: ParentNode): RefSlot[] {
  return [...root.querySelectorAll<HTMLElement>("[data-ref-ids]")].map((e) => ({
    el: e,
    ids: (e.dataset.refIds ?? "").split(" ").filter((x) => x !== ""),
    label: e.dataset.refLabel ?? "",
  }));
}
