/**
 * 正文排版器：后端解析好的块树（`cmd_wire::md::Doc`）→ DOM。界面只照树排，不解析 Markdown、不判高亮类别。
 *
 * - 字一律经 `textContent` / 文本节点落，不拼 HTML 串 ⇒ 正文里的尖括号永远是字，不要清洗。
 * - 代码块沿用全局 `.code-block` 那一套结构（复制按钮走 main.ts 的全局委托）；高亮照那一段的 `SegHl`：
 *   段里的代码块按先序第 n 块取 `hl[n]`，三元组 `[起点差, 长, 类]`，偏移按 UTF-16 码元（JS 串直接切）。
 * - 公式只出占位 `data-math-pending`，进视口由 `render.ts::enhanceCard` 排（KaTeX 只住那一处）。
 * - 长正文按 `segs` 分段：`mountDoc` 只建第 0 段，`buildNext` 一段段往下接。
 */
import type { Doc } from "./generated/Doc";
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

/** 高亮类：下标就是 `HlClass` 的序号（`src/common/cmd-wire/src/md.rs`），名单的先后只许跟着那边改。 */
const HL_CLASSES: readonly string[] = [
  s.hlKeyword,
  s.hlString,
  s.hlComment,
  s.hlNumber,
  s.hlFunction,
  s.hlType,
  s.hlConstant,
  s.hlVariable,
  s.hlAttribute,
  s.hlTag,
  s.hlInserted,
  s.hlDeleted,
  s.hlMeta,
];

/** 一份排好的正文：`root` 挂进卡里；`buildNext` 接着建下一段，建完了（或本来只有一段）回 false。 */
export interface DocView {
  readonly root: HTMLElement;
  /** 一共几段。 */
  readonly segCount: number;
  /** 已建几段。 */
  built(): number;
  /** 建下一段（`hl` 是那一段的高亮回包，没拉到就不给、代码照纯字排）。 */
  buildNext(hl?: SegHl): boolean;
}

/** 第 i 段占 `blocks` 的哪一截：`[起, 止)`。 */
export function segRange(doc: Doc, i: number): [number, number] {
  const from = i === 0 ? 0 : (doc.segs[i - 1] ?? doc.blocks.length);
  const to = i < doc.segs.length ? doc.segs[i] : doc.blocks.length;
  return [from, to];
}

/** 建根、排第 0 段。 */
export function mountDoc(doc: Doc, hl0?: SegHl): DocView {
  const root = document.createElement("div");
  root.className = s.mdDoc;
  const segCount = doc.segs.length + 1;
  let built = 0;
  const view: DocView = {
    root,
    segCount,
    built: () => built,
    buildNext(hl?: SegHl): boolean {
      if (built >= segCount) return false;
      const [from, to] = segRange(doc, built);
      root.append(buildBlocks(doc.blocks.slice(from, to), hl));
      built += 1;
      return true;
    },
  };
  view.buildNext(hl0);
  return view;
}

/** 一串块 → 片段；`hl` 按这一串里代码块的先序取。 */
export function buildBlocks(blocks: readonly MdBlock[], hl?: SegHl): DocumentFragment {
  const ctx: Ctx = { hl: hl?.hl ?? [], code: 0 };
  const frag = document.createDocumentFragment();
  for (const b of blocks) frag.append(block(b, ctx));
  return frag;
}

interface Ctx {
  readonly hl: readonly (readonly number[])[];
  /** 下一个代码块在先序里是第几个。 */
  code: number;
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
      return codeBlock(b.lang, b.x, ctx.hl[ctx.code++] ?? []);
    case "math":
      return mathSlot(b.tex, true);
    case "list":
      return list(b.start, b.items, ctx);
    case "quote":
      return box("blockquote", blocksOf(b.blocks, ctx));
    case "alert": {
      const q = box("blockquote", blocksOf(b.blocks, ctx), s.mdAlert);
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
      return box("section", [ol], s.mdNotes);
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

function codeBlock(lang: string | null, x: string, hl: readonly number[]): HTMLElement {
  const label = el("span", "code-lang", lang ?? "text");
  const copy = el("button", "code-copy", COPY_LABEL);
  copy.type = "button";
  copy.dataset.copy = "";
  const bar = box("div", [label, copy], "code-bar");
  const code = box("code", highlighted(x, hl), s.mdCode);
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
    const cls = HL_CLASSES[hl[i + 2]];
    if (to > from) {
      if (cls === undefined) {
        out.push(document.createTextNode(x.slice(from, to)));
      } else {
        out.push(el("span", cls, x.slice(from, to)));
      }
    }
    at = to;
  }
  if (at < x.length) out.push(document.createTextNode(x.slice(at)));
  return out;
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
