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
 * 文本估高用 pretext(@chenglou/pretext,**精确钉版 0.0.9**——上游 0.0.x 属
 * pre-stable,API 与折行语义可能不打招呼地变;本模块的常数是对着该版本的 layout
 * 行为标定的,解钉升级须重跑估值精度对照再动):prepare() 一次分词+canvas
 * 测宽,layout() 纯算术出高度,不触 DOM 不 reflow。pretext 不可用(canvas 缺失、
 * 字体未就绪等)时降级为字符宽度算术估计——两条路都不许抛。
 *
 * 宽度/字体常数镜像 styles.css tokens(--stream-max-width/--font-*):780px 定宽
 * 列是本模块成立的前提(宽度不变 → 高度长期有效),若列宽 token 改动需同步这里。
 *
 * ⚠ 上面那句「解钉升级须重跑估值精度对照」在 0.0.8 → 0.0.9 这一跳**没有被执行**:
 * 本模块今天**没有任何「估值 vs 真实布局高度」的读数**(`height-estimate.vitest.ts`
 * 全部是自一致测试——拿估算函数的输出跟它自己的常数比;jsdom 无布局引擎也比不了)。
 * 该对照表是 `设计/17 §6` 的「秤 2」,要真浏览器 e2e 才做得出来,尚未落地。
 * ⇒ 本模块所有常数的准确度,目前状态是**未测**,不是「已验证」。
 */

// 镜像 styles.css 的字体 token(canvas font 接受完整 fallback 栈——必须逐字同栈,
// 否则"装了 Source Serif Pro 没装 4"的机器上 DOM 与 canvas 各走各的字体,度量漂移)
const FONT_PROSE =
  '15px "Source Serif 4", "Source Serif Pro", "Iowan Old Style", "PingFang SC", "Microsoft YaHei", Georgia, serif';
const FONT_BASE = 'Inter, "Segoe UI", system-ui, sans-serif';
const LH_PROSE = 15 * 1.65; // --font-size-prose × --line-height-prose
const LH_BASE = 14 * 1.55;
const LH_MONO = 13 * 1.55;

const COL_W = 780; // --stream-max-width
const USER_BODY_W = COL_W * 0.8 - 34; // 气泡 max-width 80% - padding 16×2 - border 2
const SUMMARY_H = 38; // 折叠 <details> 只剩 summary 行
const CODE_BAR_H = 30; // .code-bar(copy 按钮撑高)+ border
const CODE_PAD_V = 30; // pre 上下 padding 14×2 + border 2
const CODE_MARGIN = 24; // .code-block margin 12×0(上下)
const CARD_HEADER_H = 22; // .card-header 一行 + 间距
const BLOCK_GAP = 10; // 块间 margin 均摊
const P_GAP = 12; // 段落/列表项之间的 margin 均摊(浏览器默认 p margin 1em 折叠后 ~15px,取偏保守)

// 下面三条细条卡常数补的是「建得出卡、估不出高」的三个 class ——
// 它们此前全部走 `return null` ⇒ 由 styles.css:1599 的 `contain-intrinsic-size: auto 120px`
// 接管。⚠ 这三条是**候选成因**不是已证实的根因:`设计/17 §2.2` 那条「虚高 ⇒ 提前停止补批
// ⇒ 只渲染半屏」的链条是**手算**的(按 CSS token 推真高、按视口 800px 推轮次),
// 而「估得准不准」今天零读数(同文档 §5.3)。装上秤 2(估值 vs 真值 e2e 对照表)之后再判
// 这三条改动到底修没修掉半屏 —— 在那之前,它们只是「把没估的估上」,不是「已修复」。
const BASH_OUTPUT_HEADER_H = 19; // .bash-output-header 单行 flex:12px × 1.55 ≈ 18.6
const BASH_OUTPUT_BODY_MARGIN = 6; // .bash-output-body margin: 6px 0 0
/** `buildOutputPre` 超 30 行只展示头 20 行(cards/bash.ts OUTPUT_HEAD_LINES) */
const BASH_OUTPUT_MAX_LINES = 20;
const BASH_STDERR_LABEL_H = 23; // .bash-stderr-label margin-top 6 + 11px × 1.55
const BASH_EMPTY_H = 23; // .bash-output-empty margin-top 4 + 12px × 1.55
const SHOW_FULL_BTN_H = 43; // .block-body-show-full margin 6+10 + padding 4×2 + border 2 + 11px × 1.55

/** 超长文本只测前缀、按长度比例外推——防 pretext prepare 开销失控(HN 实测大批量偏慢) */
const MEASURE_PREFIX_CHARS = 2400;

// pretext 懒加载:失败(无 canvas 的测试环境等)记住并永久走算术降级
type PretextModule = typeof import("@chenglou/pretext");
let pretextMod: PretextModule | null = null;
let pretextBroken = false;
let pretextFailures = 0; // 三振出局:偶发单条病态输入不应永久禁用整个会话的精确估高
const PRETEXT_MAX_FAILURES = 3;

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
  for (const hard of text.replace(/\r/g, "").split("\n")) {
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
      // pre-wrap:保留 \n 硬断行(默认 normal 会折叠成空格,与 fallback 语义相反),
      // 顺带由库归一 CRLF
      const prepared = pretextMod.prepare(sample, font, { whiteSpace: "pre-wrap" });
      h = pretextMod.layout(prepared, widthPx, lineHeightPx).height;
    } catch (e) {
      pretextFailures++;
      if (pretextFailures >= PRETEXT_MAX_FAILURES) {
        console.warn("[height-estimate] pretext 连续失败,永久转算术降级:", e);
        pretextBroken = true;
        pretextMod = null;
      }
    }
  }
  if (h === null) h = fallbackTextHeight(sample, fontSizePx, lineHeightPx, widthPx);
  // 前缀外推:按字符比例放大(粗,但只影响超长卡的初值)
  if (overflow) h = (h * text.length) / MEASURE_PREFIX_CHARS;
  return h;
}

// R1(D 审计):textContent 会把 <br> 和 <p>/<li> 边界全部吞掉——40 行日志被当一条
// 长串估,8 倍低估。块感知提取:块级边界与 <br> 插 \n;跳过 .code-block(单独按
// 行数估)与 .katex-mathml(KaTeX 的 aria 重复文本,计入会双算)。
const BLOCK_TAGS = new Set([
  "P",
  "LI",
  "PRE",
  "BLOCKQUOTE",
  "TR",
  "H1",
  "H2",
  "H3",
  "H4",
  "H5",
  "H6",
]);

/** 导出仅为单测。返回带 \n 的文本 + 块数(段间 margin 按块数均摊)。 */
export function extractProseText(root: Element): { text: string; blockCount: number } {
  let out = "";
  let blockCount = 0;
  const walk = (node: Node): void => {
    for (const child of Array.from(node.childNodes)) {
      if (child.nodeType === 3) {
        out += child.nodeValue ?? "";
        continue;
      }
      if (child.nodeType !== 1) continue;
      const e = child as Element;
      if (e.tagName === "BR") {
        out += "\n";
        continue;
      }
      if (e.classList.contains("code-block") || e.classList.contains("katex-mathml")) continue;
      const isBlock = BLOCK_TAGS.has(e.tagName);
      if (isBlock) blockCount++;
      walk(e);
      if (isBlock && !out.endsWith("\n")) out += "\n";
    }
  };
  walk(root);
  return { text: out.replace(/\n+$/, ""), blockCount };
}

/** 代码块:等宽字体纯算术——行数 × 行高 + bar/padding 常数,不需要 pretext */
export function codeBlockHeight(codeText: string): number {
  const lines = codeText ? codeText.split("\n").length : 1;
  return lines * LH_MONO + CODE_BAR_H + CODE_PAD_V;
}

/** assistant 正文里单个块的估高 */
function blockHeight(el: Element): number {
  // 折叠 <details>(thinking / tool_use / 大输出)只剩 summary
  if (el.tagName === "DETAILS" && !(el as HTMLDetailsElement).open) return SUMMARY_H;
  // 文本块(.block-text 等):R2(D 审计)——.code-block 嵌在其内部而非 card-body
  // 直接子元素,必须下钻:代码块逐个按行数估,其余文本走 prose 估
  const { text, blockCount } = extractProseText(el);
  let h = textHeight(text, FONT_PROSE, 15, LH_PROSE, COL_W) + BLOCK_GAP;
  h += Math.max(0, blockCount - 1) * P_GAP;
  for (const pre of Array.from(el.querySelectorAll(".code-block pre"))) {
    h += codeBlockHeight(pre.textContent ?? "") + CODE_MARGIN;
  }
  return h;
}

/**
 * `设计/17 §2.2` 修法②:card-bash-output = header + Σ min(行数, 20) × mono 行高 + 几个细条行。
 * 行数数的是 DOM 里**已经被 `buildOutputPre` 截过**的 pre,所以不需要知道原始输出多长;
 * `.bash-output-body` 另有 `max-height: 480px` 硬顶(≈23 个 mono 行),20 这个上限落在它之内。
 */
function bashOutputHeight(el: Element): number {
  let h = BASH_OUTPUT_HEADER_H;
  for (const pre of Array.from(el.querySelectorAll("pre.bash-output-body"))) {
    const t = pre.textContent ?? "";
    const lines = t ? t.split("\n").length : 1;
    h += Math.min(lines, BASH_OUTPUT_MAX_LINES) * LH_MONO + BASH_OUTPUT_BODY_MARGIN;
  }
  if (el.querySelector(".bash-stderr-label")) h += BASH_STDERR_LABEL_H;
  if (el.querySelector(".bash-output-empty")) h += BASH_EMPTY_H;
  h += el.querySelectorAll(".block-body-show-full").length * SHOW_FULL_BTN_H;
  return h;
}

/**
 * 认不出的 class 在 DEV 下每个只喊一次(`设计/17 §2.2` 修法③)。
 * 为什么要喊:不喊的话,下次加新卡型又会静默落回 CSS 的 120px 兜底,
 * 而 120px 兜底与真高的偏差**今天没有任何读数**(§5.3),没人会发现。
 * 模块级 Set:同一 class 刷屏一次就够;生产构建整支被 vite 消除。
 */
const unknownCardClassesSeen = new Set<string>();

function warnUnknownCard(el: HTMLElement): void {
  if (!import.meta.env.DEV) return;
  // 取第一个 `card-*` 类名当身份;没有就用 tagName,保证一定报得出个东西
  const key =
    Array.from(el.classList).find((c) => c.startsWith("card-")) ?? `<${el.tagName.toLowerCase()}>`;
  if (unknownCardClassesSeen.has(key)) return;
  unknownCardClassesSeen.add(key);
  console.warn(
    `[height-estimate] 估不出高的卡型:${key} —— 落 CSS 兜底 contain-intrinsic-size: auto 120px。` +
      "若它是细条卡,120px 会虚高数倍并污染「够不够一屏」的判据(设计/17 §2.2)。",
  );
}

/** 导出仅为单测(要能在一个用例里从干净状态起算「每 class 只喊一次」)。 */
export function __resetUnknownCardWarnings(): void {
  unknownCardClassesSeen.clear();
}

/**
 * 顶层卡片估高。认不出的形态返回 null(CSS 兜底 120px + 渲染后 auto 记忆)。
 */
export function estimateStreamNodeHeight(el: HTMLElement): number | null {
  // 折叠态顶层 <details>(工具组 / compact 卡):summary 一行,与 units 数量无关
  if (el.tagName === "DETAILS" && !(el as HTMLDetailsElement).open) return SUMMARY_H;

  if (el.classList.contains("card-user")) {
    const body = el.querySelector(".card-body");
    // renderPlainText 把 \n 换成 <br>——必须走提取器还原硬断行(R1)。
    // 不加 padding/border:contain-intrinsic-size 是 content-box,盒模型自会另加(S1)
    const text = body ? extractProseText(body).text : "";
    return textHeight(text, `14px ${FONT_BASE}`, 14, LH_BASE, USER_BODY_W);
  }

  // 高频细条卡:120px 兜底偏大 3 倍,常数更准(渲染后 auto 记忆接管)
  if (el.classList.contains("card-api-error")) return 40;
  if (el.classList.contains("card-slash")) return 34;
  // `设计/17 §2.2` 修法①:两行常数。card-api-retry 是"重试风暴"时成批出现的那一种。
  if (el.classList.contains("card-api-retry")) return 24;
  if (el.classList.contains("card-bash-input")) return 32;
  // `设计/17 §2.2` 修法②:card-bash-output 按 header + min(行数, 20) 算。
  // 行数直接数 DOM 里**已经截过的** pre(cards/bash.ts 超 30 行只留头 20 行),
  // 所以这里不需要知道原始输出多长。stderr 标签/展开按钮/空态各算一个细条行。
  if (el.classList.contains("card-bash-output")) return bashOutputHeight(el);

  if (el.classList.contains("card-assistant")) {
    let h = CARD_HEADER_H;
    const body = el.querySelector(".card-body");
    if (!body) return h + 20; // 无 body 的空卡:header + 余量
    for (const child of Array.from(body.children)) h += blockHeight(child);
    return h;
  }

  warnUnknownCard(el);
  return null;
}

/**
 * 建卡处统一入口:算出估值写 `contain-intrinsic-size: auto <h>px`。
 * F39(viewer 窗口化)复用同一模块供高——不要另写第二套(账本 §3)。
 */
export function applyIntrinsicSize(el: HTMLElement): void {
  const h = estimateStreamNodeHeight(el);
  if (h !== null) {
    // 24px 下限:单行文本卡的最小合理占位,防 0/负值
    el.style.setProperty("contain-intrinsic-size", `auto ${Math.max(24, Math.round(h))}px`);
  }
}
