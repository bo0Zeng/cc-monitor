/**
 * 骨架的高度模型：不建卡、不看 DOM，按记录估一条在骨架里占多高（骨架账本 `skeleton-view.ts` 与尾部窗口 `live-window.ts` 用）。
 *
 * 两级：第一级（`estimateFromFacts`）只看后端索引给的宽度无关料 × 列宽；第二级（`refineItemOf` ＋ `refinedHeight`）把正文那一段
 * 交估高 Worker 用 pretext 排（精确钉版 1.x；同一份语料逐条量高对拍，判据在 `height-estimate.vitest.ts`）。Worker 排高只走 `pretextHeight` 这一句。
 * 估值只是占位：建了卡就是真高 ⇒ 求够准、绝不抛错，不求像素真值。
 * 字号 · 字族 · 行高 · 回退列宽不另抄令牌的数：模块求值时读一次 `:root` 的计算样式（见 `token`）；外框常数由 `skel-outer.vitest.ts` 对拍真浏览器的金样。
 */
import type { LineRecord } from "./generated/LineRecord";
import type { Draw } from "./generated/Draw";
import type { Doc } from "./generated/Doc";
import type { Inl } from "./generated/Inl";
import type { MdBlock } from "./generated/MdBlock";
import TOKENS_CSS from "../../shared/theme/tokens.css?raw";

/** tokens.css 第一块 `:root` 的令牌（没有文档的地方 —— 估高 Worker · jsdom 没挂样式表 —— 照它）。 */
const TOKEN_FILE: ReadonlyMap<string, string> = ((): Map<string, string> => {
  const css = TOKENS_CSS.replace(/\/\*[\s\S]*?\*\//g, "");
  const at = css.indexOf(":root {");
  const body = css.slice(at, css.indexOf("}", at));
  return new Map([...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2]]));
})();
const ROOT_STYLE = typeof document === "undefined" ? null : getComputedStyle(document.documentElement);
/** 一个令牌此刻的值：`:root` 的计算样式；读出空串（没挂样式表）⇒ tokens.css 那一份。只在模块求值时读。 */
function token(name: string): string {
  const v = (ROOT_STYLE?.getPropertyValue(name) || TOKEN_FILE.get(name) || "").replace(/\s+/g, " ").trim();
  if (!v) throw new Error(`theme token ${name} unreadable`);
  return v;
}
const px = (name: string): number => parseFloat(token(name));

// canvas font 接受完整 fallback 栈 —— 照令牌逐字同栈，否则「装了 Source Serif Pro 没装 4」的机器上 DOM 与 canvas 各走各的字体、度量漂移。
const FS_PROSE = px("--font-size-prose");
const FS_BASE = px("--font-size-base");
const FONT_PROSE = `${FS_PROSE}px ${token("--font-prose")}`;
const FONT_BASE = `${FS_BASE}px ${token("--font-base")}`;
const LH_PROSE = FS_PROSE * Number(token("--line-height-prose"));
const LH_BASE = FS_BASE * Number(token("--line-height-base"));
const LH_MONO = px("--font-size-mono") * Number(token("--line-height-mono"));

// 列宽：模块求值时在 `#message-stream` 里临时摆一个生产形状的 `.stream > .stream-content`（`.stream` 是名为 `stream` 的尺寸容器），
// 量出 min(`--stream-max-width`, 流宽 − 两侧内边距) 立刻撤掉；量不到（jsdom / 没有消息流的窗口）才退回 `--stream-max-width`。
// 这是起始值：之后列宽变了由宿主现量、交账本 `relayout`（`tab-stream-view.ts::relayoutOnColumnChange`）。
const COL_W: number = ((fallback: number): number => {
  const host = typeof document === "undefined" ? null : document.getElementById("message-stream");
  if (!host) return fallback;
  const probe = document.createElement("div");
  probe.className = "stream";
  probe.classList.add("active"); // 当前那一形（收起的 `.stream` 内容不排版，量不到列宽）
  const col = document.createElement("div");
  col.className = "stream-content";
  probe.appendChild(col);
  host.appendChild(probe);
  const w = col.getBoundingClientRect().width;
  probe.remove();
  return w > 0 ? w : fallback;
})(px("--stream-max-width"));
/** 起始列宽（上面量出来的那个；账本没交过列宽时就是它）。 */
export function initialColumnWidth(): number {
  return COL_W;
}
const SUMMARY_H = 38; // 折叠 <details> 只剩 summary 行
const STEPS_GAP = 4; // .card-steps 的 gap
const CODE_BAR_H = 30; // .code-bar(copy 按钮撑高)+ border
const CODE_PAD_V = 30; // pre 上下 padding 14×2 + border 2
const CODE_MARGIN = 24; // .code-block margin 12×0(上下)
const CARD_HEADER_H = 22; // .card-header 一行 + 间距
const BLOCK_GAP = 10; // 块间 margin 均摊
const P_GAP = 12; // 段落/列表项之间的 margin 均摊(浏览器默认 p margin 1em 折叠后 ~15px,取偏保守)

/** 超长文本只测前缀、按长度比例外推 —— 防 pretext prepare 开销失控（大批量偏慢） */
const MEASURE_PREFIX_CHARS = 2400;

type PretextModule = typeof import("@chenglou/pretext");

/**
 * pretext 排一段正文的高：估高 Worker（`height-worker.ts`）排第二级那一件走这一句。
 * pre-wrap：保留 \n 硬断行（默认 normal 会折叠成空格），顺带由库归一 CRLF。
 * 每次现 prepare、不留 handle（1.0 不认 0.0.9 造的 handle，这里本来就不留）。
 */
export function pretextHeight(
  p: Pick<PretextModule, "prepare" | "layout">,
  text: string,
  font: string,
  widthPx: number,
  lineHeightPx: number,
): number {
  return p.layout(p.prepare(text, font, { whiteSpace: "pre-wrap" }), widthPx, lineHeightPx).height;
}

// ═══════════════════════════════════════════════════════════════════════
// 〔骨架〕第一级粗估：**不看 DOM、不看正文**，只看后端索引给的宽度无关料。
// ═══════════════════════════════════════════════════════════════════════

/**
 * 后端骨架索引的一行（帧命令 `history-index`，形状住 `IPC-PROTOCOL.md §10.3`）。
 * 零值 / 假值不上线 ⇒ 这里全是可选。
 */
export interface SkeletonFacts {
  /** 行起点字节偏移（绝对） */
  o: number;
  /** 行字节长（含 `\n`） */
  n: number;
  /** 这一行会翻成的通用记录的类（与读正文时那条记录的 `t` 同一个词；不进界面 ⇒ 缺） */
  t?: LineRecord["t"];
  /** 画成哪一形（就是那条通用记录的 `draw`；不画 ⇒ 缺） */
  dr?: Draw;
  /** 人在这条里说了话（就是那条通用记录的 `human`） */
  hu?: true;
  /** 正文字符数（代码块外） */
  ch?: number;
  /** 其中 CJK */
  cj?: number;
  /** 正文非空硬行 */
  pl?: number;
  /** 围栏代码块数 */
  cb?: number;
  /** 代码行数 */
  cl?: number;
  /** 折叠单元数（tool_use / tool_result / thinking / image） */
  fd?: number;
}

/**
 * 卡片外框（padding ＋ border ＋ 卡间 margin）的常数。
 * 金标准 `tests/evidence/skel-golden.json`（真 Chromium 里每张卡占的位置 − 头 − 体），
 * 门禁在 `skel-outer.vitest.ts`：每一格 == 中位数向上取整（粗估宁可偏高）。
 */
const SKEL_USER_CHROME = 50;
const SKEL_CARD_CHROME = 19;
/** 「几步」卡的外距（每步一行另按 `SUMMARY_H`、步间按 `STEPS_GAP` 算）。 */
const SKEL_STEPS_CHROME = 4; // 上下各 4px 外距，与邻卡的外距折叠后只剩一份（RENDER2 金样：位置 − 那几行 = 4）
const SKEL_SYSTEM_H = 28;
/** 系统注入的细条（「显示系统注入」开着时）：一行 summary ＋ 上下外距。关着 ⇒ 0。 */
const SKEL_INJECTED_H = 30;
let injectedShown = false;

/** 「显示系统注入」开没开（估高跟着它：关着注入行高 0，开着按一条细条算）。改了之后调用方让骨架账本重排。 */
export function setInjectedShown(on: boolean): void {
  injectedShown = on;
}

/** 第一级里「正文之外那一段」四格（秤对拍的就是它们；assistant 那一格含头与块距）。只给判据读。 */
export const SKEL_OUTER = {
  user: SKEL_USER_CHROME,
  assistant: CARD_HEADER_H + BLOCK_GAP + SKEL_CARD_CHROME,
  steps: SKEL_STEPS_CHROME,
  system: SKEL_SYSTEM_H,
} as const;

/** 字宽算术（码点 > 0x2E80 的 CJK / 全宽按一个字号宽、其余 0.52em），折成行数的**上界**。 */
function factLines(f: SkeletonFacts, fontSizePx: number, widthPx: number): number {
  const ch = f.ch ?? 0;
  const cj = Math.min(f.cj ?? 0, ch);
  const w = cj * fontSizePx + (ch - cj) * fontSizePx * 0.52;
  // Σ ceil(wᵢ/W) ≤ 硬行数 + Σwᵢ/W —— 取上界（偏保守，见 SKEL_* 那段头注）
  return (f.pl ?? 0) + w / widthPx;
}

function factCode(f: SkeletonFacts): number {
  const cb = f.cb ?? 0;
  return (f.cl ?? 0) * LH_MONO + cb * (CODE_BAR_H + CODE_PAD_V + CODE_MARGIN);
}

/**
 * **第一级粗估**：由宽度无关料 × 当前列宽算一条记录在骨架里占多高（px，border-box 口径）。
 *
 * `colW` 默认 `COL_W`（模块求值时量出来的起始列宽）。
 *
 * 买到：骨架落地那一瞬间就有总高（滚动条大致对、跳转可用）。
 * **买不到**：精度。它不知道 slash/compact 细条、ESC 折叠、`stripInternalNoise` 剥空这些渲染期决定；
 * 第二级（`refineItemOf`，Worker 排正文）换掉正文那一段；建了卡的那几条用真高。
 */
export function estimateFromFacts(f: SkeletonFacts, colW: number = COL_W): number {
  // 画成哪一形是后端那一格（`dr`）：
  // 系统注入的细条露不露、占不占高只看「显示系统注入」；几步卡每步一行（人那一侧的工具结果注入进它那个调用的那一行，高 0）；不画 ⇒ 0。
  switch (f.dr) {
    case "strip":
      return injectedShown ? SKEL_INJECTED_H : 0;
    case "steps": {
      const n = f.fd ?? 0;
      return f.t === "said" ? 0 : n * SUMMARY_H + Math.max(0, n - 1) * STEPS_GAP + SKEL_STEPS_CHROME;
    }
    case "card":
      break;
    default:
      return 0;
  }
  if (f.t === "retry" || f.t === "unread") return SKEL_SYSTEM_H;
  const folded = (f.fd ?? 0) * SUMMARY_H;
  if (f.t === "said") {
    const userW = colW * 0.8 - 34;
    return factLines(f, FS_BASE, userW) * LH_BASE + factCode(f) + folded + SKEL_USER_CHROME;
  }
  const paras = Math.max(0, (f.pl ?? 0) - 1) * P_GAP;
  return (
    CARD_HEADER_H +
    factLines(f, FS_PROSE, colW) * LH_PROSE +
    paras +
    factCode(f) +
    folded +
    BLOCK_GAP +
    SKEL_CARD_CHROME
  );
}

/**
 * 〔「字节不是成本轴，卡型才是 …… 同一种卡正文越长越贵」〕**这条记录建卡时要当场物化的正文字符数**：
 * said / reply 的正文块（助手正文是树：建卡只排第 0 段，只数那一段的字）。thinking / 工具调用 / 工具结果
 * 是折叠卡（展开才物化）、别的不建卡 ⇒ 0。不序列化。批闸（`TabStreamView.BATCH_BODY_CHARS`）按它算。
 */
export function eagerBodyChars(record: LineRecord): number {
  switch (record.t) {
    case "said":
    case "reply": {
      let n = 0;
      for (const b of record.blocks ?? []) {
        // 残缺记录（夹具）不抛
        if (b.type === "text") n += b.text.length;
        else if (b.type === "prose") for (const p of docPieces(b.doc.blocks.slice(0, b.doc.segs[0] ?? b.doc.blocks.length))) n += p.text.length;
      }
      return n;
    }
    default:
      return 0;
  }
}

// ═══════════════════════════════════════════════════════════════════════
// **助手正文的树摊成字**：估高量它，报错卡的「原文」也照它取（树上的字只在这里摊一处）。
// ═══════════════════════════════════════════════════════════════════════

/** 摊开的一项：一行字（段落 · 标题 · 表格一行 · 公式），或整个代码块（`code`）；`gap` ⇒ 它是一个顶层块的头一项（前面有段距）。 */
export interface DocPiece {
  text: string;
  code: boolean;
  gap: boolean;
}

/** 行内的字连起来：强调 · 链接取里面的字，行内代码 · 公式取原文，图取替代字，硬换行成 `\n`，脚注引用写号。 */
function inlineText(ks: readonly Inl[]): string {
  let out = "";
  for (const k of ks) {
    if (typeof k === "string") {
      out += k;
      continue;
    }
    switch (k.t) {
      case "b":
      case "i":
      case "del":
      case "a":
        out += inlineText(k.k);
        break;
      case "c":
        out += k.x;
        break;
      case "m":
        out += k.tex;
        break;
      case "img":
        out += k.alt;
        break;
      case "br":
        out += "\n";
        break;
      case "fn":
        out += String(k.n);
        break;
    }
  }
  return out;
}

/** 一串块摊成一项项（先序；列表 · 引用 · 提示块 · 脚注往里走，分隔线按一行算）。 */
export function docPieces(blocks: readonly MdBlock[]): DocPiece[] {
  const out: DocPiece[] = [];
  const walk = (bs: readonly MdBlock[], top: boolean): void => {
    for (const b of bs) {
      const at = out.length;
      switch (b.t) {
        case "p":
        case "h":
          out.push({ text: inlineText(b.k), code: false, gap: false });
          break;
        case "code":
          out.push({ text: b.x, code: true, gap: false });
          break;
        case "math":
          out.push({ text: b.tex, code: false, gap: false });
          break;
        case "list":
          for (const it of b.items) walk(it.blocks, false);
          break;
        case "quote":
        case "alert":
          walk(b.blocks, false);
          break;
        case "notes":
          for (const n of b.items) walk(n.blocks, false);
          break;
        case "table":
          out.push({ text: b.head.map(inlineText).join(" | "), code: false, gap: false });
          for (const r of b.rows) out.push({ text: r.map(inlineText).join(" | "), code: false, gap: false });
          break;
        case "rule":
          out.push({ text: "---", code: false, gap: false });
          break;
      }
      if (top && out.length > at) out[at].gap = true;
    }
  };
  walk(blocks, true);
  return out;
}

/** 整棵树的字：一项一行（报错卡的「原文」照它）。 */
export function docText(doc: Doc): string {
  return docPieces(doc.blocks)
    .map((p) => p.text)
    .join("\n");
}

// ═══════════════════════════════════════════════════════════════════════
// **第二级估高**：同一套外框常数，正文那一段换成 pretext 精算（在 Worker 里跑）。
// ═══════════════════════════════════════════════════════════════════════

/** 交 Worker 精算的一件：`text` 按 `font` / `widthPx` / `lineHeightPx`（pre-wrap）排出来的高，加上 `fixed` 就是这一行的高。 */
export interface RefineItem {
  text: string;
  font: string;
  lineHeightPx: number;
  widthPx: number;
  /** 正文以外那几段（外框 · 头 · 代码 · 折叠单元 · 段距），与第一级 `estimateFromFacts` 同一套常数。 */
  fixed: number;
  /** 超长正文只交前缀（`MEASURE_PREFIX_CHARS`）：排出来的高 × `scale` 外推回全长（不超长 ⇒ 1）。 */
  scale: number;
}

/** 一段 markdown：围栏代码块外的正文 ＋ 代码行数 ＋ 代码块数 ＋ 非空硬行数（口径同后端索引的 `ch` / `cl` / `cb` / `pl`）。 */
function splitFences(md: string): { prose: string; codeLines: number; codeBlocks: number; paras: number } {
  const prose: string[] = [];
  let codeLines = 0;
  let codeBlocks = 0;
  let inCode = false;
  for (const line of md.split("\n")) {
    if (/^\s*(```|~~~)/.test(line)) {
      if (!inCode) codeBlocks++;
      inCode = !inCode;
      continue;
    }
    if (inCode) codeLines++;
    else prose.push(line);
  }
  const text = prose.join("\n");
  const paras = prose.filter((l) => l.trim().length > 0).length;
  return { prose: text, codeLines, codeBlocks, paras };
}

/** 助手正文的树：同 `splitFences` 那四样（顶层块之间按一个空行算，与原文里块之间的空行同口径）。 */
function shapeOfDoc(doc: Doc): { prose: string; codeLines: number; codeBlocks: number; paras: number } {
  const prose: string[] = [];
  let codeLines = 0;
  let codeBlocks = 0;
  for (const p of docPieces(doc.blocks)) {
    if (p.code) {
      codeBlocks++;
      codeLines += p.text.split("\n").length;
      continue;
    }
    if (p.gap && prose.length > 0) prose.push("");
    prose.push(...p.text.split("\n"));
  }
  const paras = prose.filter((l) => l.trim().length > 0).length;
  return { prose: prose.join("\n"), codeLines, codeBlocks, paras };
}

/**
 * 一条记录 ⇒ 第二级那一件；不值得精算的（不建卡 · 几步卡 · 细条卡 · 没有正文）⇒ `null`（第一级就是它的高）。
 * 与 `estimateFromFacts` 逐项同形：只把 `factLines(…) × 行高` 换成 pretext 排出来的高。`colW` 同第一级。
 */
export function refineItemOf(rec: LineRecord, colW: number = COL_W): RefineItem | null {
  if (rec.draw !== "card" || (rec.t !== "said" && rec.t !== "reply")) return null;
  let folded = 0;
  const shapes: ReturnType<typeof splitFences>[] = [];
  for (const b of rec.blocks ?? []) {
    if (b.type === "text") shapes.push(splitFences(b.text));
    else if (b.type === "prose") shapes.push(shapeOfDoc(b.doc));
    else folded++;
  }
  const prose = shapes.map((x) => x.prose).join("\n");
  const codeLines = shapes.reduce((a, x) => a + x.codeLines, 0);
  const codeBlocks = shapes.reduce((a, x) => a + x.codeBlocks, 0);
  const paras = shapes.reduce((a, x) => a + x.paras, 0);
  if (!prose.trim()) return null; // 纯工具 / 纯代码：第一级那份算术已经是它
  const code = codeLines * LH_MONO + codeBlocks * (CODE_BAR_H + CODE_PAD_V + CODE_MARGIN);
  const over = prose.length > MEASURE_PREFIX_CHARS;
  const text = over ? prose.slice(0, MEASURE_PREFIX_CHARS) : prose;
  const scale = over ? prose.length / MEASURE_PREFIX_CHARS : 1;
  if (rec.t === "said") {
    return {
      text,
      scale,
      font: FONT_BASE,
      lineHeightPx: LH_BASE,
      widthPx: colW * 0.8 - 34,
      fixed: code + folded * SUMMARY_H + SKEL_USER_CHROME,
    };
  }
  return {
    text,
    scale,
    font: FONT_PROSE,
    lineHeightPx: LH_PROSE,
    widthPx: colW,
    fixed: CARD_HEADER_H + Math.max(0, paras - 1) * P_GAP + code + folded * SUMMARY_H + BLOCK_GAP + SKEL_CARD_CHROME,
  };
}

/** 一件的高（给定「这段正文排出来多高」的量法）。Worker 与判据共用这一份组合式。 */
export function refinedHeight(it: RefineItem, measure: (it: RefineItem) => number): number {
  return it.fixed + measure(it) * it.scale;
}
