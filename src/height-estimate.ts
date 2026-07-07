/**
 * Batch13-F38（issue #35 Phase 0）:虚拟化高度模型。
 *
 * 建卡时给顶层卡片估算 `contain-intrinsic-size` 初值,配合 CSS 的
 * `content-visibility: auto` 让视口外卡片 skip layout/paint。
 *
 * 关键语义:估值**只是初值**——`auto <h>px` 的 auto 关键字意味着元素渲染过一次后
 * 浏览器记住真实尺寸,估值只影响"从未渲染过的卡片"贡献的滚动条精度,不影响正确性。
 * 所以这里追求"够准 + 绝不抛错",不追求像素级真值。
 *
 * 文本估高用 pretext(@chenglou/pretext,pin 0.0.8):prepare() 一次分词+canvas
 * 测宽,layout() 纯算术出高度,不触 DOM 不 reflow。pretext 不可用(canvas 缺失、
 * 字体未就绪等)时降级为字符宽度算术估计——两条路都不许抛。
 *
 * 宽度/字体常数镜像 styles.css tokens(--stream-max-width/--font-*):780px 定宽
 * 列是本模块成立的前提(宽度不变 → 高度长期有效),若列宽 token 改动需同步这里。
 */

// 镜像 styles.css 的字体 token(canvas font 接受完整 fallback 栈)
const FONT_PROSE = '15px "Source Serif 4", Georgia, serif';
const FONT_BASE = 'Inter, "Segoe UI", system-ui, sans-serif';
const LH_PROSE = 15 * 1.65; // --font-size-prose × --line-height-prose
const LH_BASE = 14 * 1.55;
const LH_MONO = 13 * 1.55;

const COL_W = 780; // --stream-max-width
const USER_BODY_W = COL_W * 0.8 - 34; // 气泡 max-width 80% - padding 16×2 - border 2
const SUMMARY_H = 38; // 折叠 <details> 只剩 summary 行
const CODE_BAR_H = 27; // .code-bar + border
const CODE_PAD_V = 30; // pre 上下 padding 14×2 + border 2
const CARD_HEADER_H = 22; // .card-header 一行 + 间距
const BLOCK_GAP = 10; // 块间 margin 均摊

/** 超长文本只测前缀、按长度比例外推——防 pretext prepare 开销失控(HN 实测大批量偏慢) */
const MEASURE_PREFIX_CHARS = 2400;

// pretext 懒加载:失败(无 canvas 的测试环境等)记住并永久走算术降级
type PretextModule = typeof import("@chenglou/pretext");
let pretextMod: PretextModule | null = null;
let pretextBroken = false;

async function loadPretext(): Promise<void> {
  if (pretextMod || pretextBroken) return;
  try {
    pretextMod = await import("@chenglou/pretext");
  } catch {
    pretextBroken = true;
  }
}
// 模块加载即开始拉(不阻塞);拉完前建的卡走算术降级,同样是合法初值
void loadPretext();

/**
 * 算术降级:CJK 全宽、其余按 0.52em 均宽,逐硬行折行计数。
 * 导出仅为单测(拍住估算不漂移)。
 */
export function fallbackTextHeight(
  text: string,
  fontSizePx: number,
  lineHeightPx: number,
  widthPx: number,
): number {
  if (!text) return 0;
  let lines = 0;
  for (const hard of text.split("\n")) {
    let w = 0;
    for (const ch of hard) {
      w += ch.charCodeAt(0) > 0x2e80 ? fontSizePx : fontSizePx * 0.52;
    }
    lines += Math.max(1, Math.ceil(w / widthPx));
  }
  return lines * lineHeightPx;
}

function textHeight(
  text: string,
  font: string,
  fontSizePx: number,
  lineHeightPx: number,
  widthPx: number,
): number {
  if (!text.trim()) return 0;
  const overflow = text.length > MEASURE_PREFIX_CHARS;
  const sample = overflow ? text.slice(0, MEASURE_PREFIX_CHARS) : text;
  let h: number | null = null;
  if (pretextMod) {
    try {
      const prepared = pretextMod.prepare(sample, font);
      h = pretextMod.layout(prepared, widthPx, lineHeightPx).height;
    } catch {
      pretextBroken = true;
      pretextMod = null;
    }
  }
  if (h === null) h = fallbackTextHeight(sample, fontSizePx, lineHeightPx, widthPx);
  // 前缀外推:按字符比例放大(粗,但只影响超长卡的初值)
  if (overflow) h = (h * text.length) / MEASURE_PREFIX_CHARS;
  return h;
}

/** 代码块:等宽字体纯算术——行数 × 行高 + bar/padding 常数,不需要 pretext */
export function codeBlockHeight(codeText: string): number {
  const lines = codeText ? codeText.split("\n").length : 1;
  return lines * LH_MONO + CODE_BAR_H + CODE_PAD_V;
}

/** assistant 正文里单个块的估高 */
function blockHeight(el: Element): number {
  if (el.classList.contains("code-block")) {
    return codeBlockHeight(el.querySelector("pre")?.textContent ?? "");
  }
  // 折叠 <details>(thinking / tool_use / 大输出)只剩 summary
  if (el.tagName === "DETAILS" && !(el as HTMLDetailsElement).open) return SUMMARY_H;
  // 文本块(markdown 已渲染,用 textContent 近似——列表/标题的额外高度靠 BLOCK_GAP 均摊)
  const text = el.textContent ?? "";
  return textHeight(text, FONT_PROSE, 15, LH_PROSE, COL_W) + BLOCK_GAP;
}

/**
 * 顶层卡片估高。认不出的形态返回 null(CSS 兜底 120px + 渲染后 auto 记忆)。
 */
export function estimateStreamNodeHeight(el: HTMLElement): number | null {
  // 折叠态顶层 <details>(工具组 / compact 卡):summary 一行,与 units 数量无关
  if (el.tagName === "DETAILS" && !(el as HTMLDetailsElement).open) return SUMMARY_H;

  if (el.classList.contains("card-user")) {
    const text = el.querySelector(".card-body")?.textContent ?? "";
    return textHeight(text, `14px ${FONT_BASE}`, 14, LH_BASE, USER_BODY_W) + 24;
  }

  if (el.classList.contains("card-assistant")) {
    let h = CARD_HEADER_H;
    const body = el.querySelector(".card-body");
    if (!body) return h + 20;
    for (const child of Array.from(body.children)) h += blockHeight(child);
    return h;
  }

  return null;
}

/**
 * 建卡处统一入口:算出估值写 `contain-intrinsic-size: auto <h>px`。
 * F39(viewer 窗口化)复用同一模块供高——不要另写第二套(账本 §3)。
 */
export function applyIntrinsicSize(el: HTMLElement): void {
  const h = estimateStreamNodeHeight(el);
  if (h !== null) {
    el.style.setProperty("contain-intrinsic-size", `auto ${Math.max(24, Math.round(h))}px`);
  }
}
