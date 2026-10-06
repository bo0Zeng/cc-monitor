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
 * ⚠ 上面那句「解钉升级须重跑估值精度对照」在 0.0.8 → 0.0.9 这一跳**没有被执行**,
 * 但对照表本身 2026-09-18 已经有了:「秤 2」落成
 * `tests/frontend/ui/scale2-height-truth.vitest.ts`(真高来自 Chromium 153 + WebKitGTK 2.52.6
 * 两个真引擎的金标准,估值门禁跑的时候现算)。⇒ 本模块常数的准确度**不再是「未测」**,
 * 逐 class 的 p90 相对误差写在 `tests/evidence/U-scale2-height-truth.md`。
 * ⇒ 改这里任何一个常数,都要跑一次 `npx vitest run tests/frontend/ui/scale2-height-truth.vitest.ts`;
 *   0.0.8→0.0.9 那一跳的对照仍然缺(没有 0.0.8 的读数),那一格今天仍然判不了。
 */
import type { JsonlRecord } from "./generated/JsonlRecord";
import { drawsCard } from "./speaker";

// 镜像 styles.css 的字体 token(canvas font 接受完整 fallback 栈——必须逐字同栈,
// 否则"装了 Source Serif Pro 没装 4"的机器上 DOM 与 canvas 各走各的字体,度量漂移)
const FONT_PROSE =
  '15px "Source Serif 4", "Source Serif Pro", "Iowan Old Style", "PingFang SC", "Microsoft YaHei", Georgia, serif';
const FONT_BASE = 'Inter, "Segoe UI", system-ui, sans-serif';
const LH_PROSE = 15 * 1.65; // --font-size-prose × --line-height-prose
const LH_BASE = 14 * 1.55;
const LH_MONO = 13 * 1.55;

// 〔「`COL_W = 780` 改成从容器实测」〕列宽不再写死：模块求值时在 `#message-stream`
// 里临时摆一个生产形状的 `.stream > .stream-content`（`.stream` 是名为 `stream` 的尺寸容器，见 styles.css），
// 量出真列宽 = min(`--stream-max-width`, 流宽 − 两侧内边距) 立刻撤掉。
// 量不到（jsdom 没有布局 / 该窗口没有消息流）才退回 780 —— 那个数与 tokens.css 的 `--stream-max-width`
// 由 `tests/frontend/ui/css-ledger.vitest.ts` 格 ⑥ 对拍。
// 这是**起始值**：骨架账本建起来时用它；之后列宽变了由宿主现量、交账本 `relayout`
//   （`tab-stream-view.ts::relayoutOnColumnChange`），账本从此带着自己的列宽。
//   ⚠ 本文件的 `USER_BODY_W` 与建卡那一侧的估高（`estimateHeight`）仍只用这个起始值 —— 那一侧估的是「刚建、还没渲染」
//   那一瞬的占位，渲染过一次就由 `contain-intrinsic-size: auto` 用真值，列宽漂了影响的只是那一瞬。
const COL_W: number = ((fallback: number): number => {
  const host = typeof document === "undefined" ? null : document.getElementById("message-stream");
  if (!host) return fallback;
  const probe = document.createElement("div");
  probe.className = "stream";
  const col = document.createElement("div");
  col.className = "stream-content";
  probe.appendChild(col);
  host.appendChild(probe);
  const w = col.getBoundingClientRect().width;
  probe.remove();
  return w > 0 ? w : fallback;
})(780);
/** 起始列宽（上面量出来的那个；账本没交过列宽时就是它）。 */
export function initialColumnWidth(): number {
  return COL_W;
}
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
// 接管。⚠ 这三条是**候选成因**不是已证实的根因: 那条「虚高 ⇒ 提前停止补批
// ⇒ 只渲染半屏」的链条是**手算**的(按 CSS token 推真高、按视口 800px 推轮次)。
// 秤 2(2026-09-18)把「估得准不准」量出来了:`card-bash-output` 的 p90 相对误差 8.6%,
// 这一支是准的;但「半屏修没修掉」归秤 3,不是这里能判的。
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

/**
 * R1 的回摆(秤 2 现打):R1 保留了**所有** \n,而其中绝大多数根本不是断行——
 * `marked` 产出的 HTML 里标签之间/段落内部全是**源码排版换行**(表格尤甚:一行 4 个
 * `<td>` 各占一行源码)。浏览器对 `white-space: normal` 的内容把它们折叠成一个空格,
 * 而 `textHeight` 用 `pre-wrap` 喂 pretext ⇒ 每一个都被当成一次硬断行。
 * 实测 28 张 `card-assistant`:提取文本 2726 个 \n,有 DOM 依据的(块边界)只有 961 个,
 * `<br>` 0 个 ⇒ **1765 个凭空多出来的行** ⇒ 正文卡系统性 ~2× 虚高
 * (`tests/evidence/U-scale2-height-truth.md` §2)。
 *
 * ⇒ 口径定死:**只有块边界与 `<br>` 算硬断行,其余位置的空白一律按 `white-space: normal`
 * 折叠成一个空格**;真正 `pre`/`pre-wrap` 的容器(见 `preservesWhitespace`)才原样保留。
 * 两头都不对:吞掉断行 = 8 倍低估(R1 修的那个),全留 = 2 倍虚高(这次修的这个)。
 */
function preservesWhitespace(e: Element): boolean {
  // styles.css `.block-collapsible .block-body { white-space: pre-wrap }`,
  // 被 `.block-body-md { white-space: normal }` 覆盖回正常折叠(markdown 正文)。
  return (
    e.tagName === "PRE" ||
    (e.classList.contains("block-body") && !e.classList.contains("block-body-md"))
  );
}

/** 导出仅为单测。返回带 \n 的文本 + 块数(段间 margin 按块数均摊)。 */
export function extractProseText(root: Element): { text: string; blockCount: number } {
  let out = "";
  let blockCount = 0;
  /** 硬断行(块边界 / <br>):插之前吃掉排版留下的行尾空格 */
  const pushBreak = (): void => {
    if (!out || out.endsWith("\n")) return;
    out = out.replace(/ +$/, "") + "\n";
  };
  const walk = (node: Node, preserve: boolean): void => {
    for (const child of Array.from(node.childNodes)) {
      if (child.nodeType === 3) {
        const raw = child.nodeValue ?? "";
        if (preserve) {
          out += raw.replace(/\r\n?/g, "\n");
          continue;
        }
        // 折叠:连续空白(含源码换行/缩进)→ 一个空格;行首的那个空格没有宽度,丢掉
        const s = raw.replace(/\s+/g, " ");
        out += !out || out.endsWith("\n") ? s.replace(/^ /, "") : s;
        continue;
      }
      if (child.nodeType !== 1) continue;
      const e = child as Element;
      if (e.tagName === "BR") {
        out += "\n"; // <br> 是**无条件**断行:连着两个 <br> 就是两行,不能被 pushBreak 去重
        continue;
      }
      if (e.classList.contains("code-block") || e.classList.contains("katex-mathml")) continue;
      const isBlock = BLOCK_TAGS.has(e.tagName);
      // 块**前后**都断:源码换行归一成空格之后,块边界是唯一还认得出断行的依据
      if (isBlock) {
        blockCount++;
        pushBreak();
      }
      walk(e, preserve || preservesWhitespace(e));
      if (isBlock) pushBreak();
    }
  };
  walk(root, preservesWhitespace(root));
  return { text: out.trim(), blockCount };
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
 * 修法②:card-bash-output = header + Σ min(行数, 20) × mono 行高 + 几个细条行。
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
 * 认不出的 class 在 DEV 下每个只喊一次(修法③)。
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
      "若它是细条卡,120px 会虚高数倍并污染「够不够一屏」的判据。",
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

  // 高频细条卡:120px 兜底偏大 3 倍,常数更准(渲染后 auto 记忆接管)。
  //
  // ⚠ 这三条是 **content-box** 值(`contain-intrinsic-size` 吃的就是 content-box,
  //   秤 2 的 B 段在两个真引擎里实测过:声明 N ⇒ 实际占 N + padding + border)。
  //   2026-09-18 之前写的是 24 / 32 / 34,全是**按 border-box 手算**的
  //   ⇒ 每条各多一份 padding,p90 相对误差 40.8% / 72.1% / 82.9%。现值逐条:
  //     card-api-retry  17 ← 11px(--font-size-xs)   × 1.55 = 17.05(实测 content-box 真高 17.05)
  //     card-bash-input 19 ← 12px(--font-size-small) × 1.55 = 18.59
  //     card-slash      19 ← 同一套紧凑系 token,真高同 18.59
  //   padding 不在这里加:api-retry 3×2 / bash-input 6×2 / slash 6×2 由盒模型另算(S1)。
  // 🔴 2026-09-18（`99 条 75` /）之前,`applyIntrinsicSize` 里那个
  //   `Math.max(24, …)` 地板会把 17/19 一律顶成 **24** ⇒ 写进 style 的仍是 24,
  //   三条卡的落地误差停在 40.8% / 29.1% / 29.1%,**不是**常数本身对应的 0.3% / 2.2% / 2.2%。
  //   地板已去掉(见 `appliedIntrinsicPx`),这三条常数从此**原样出货**。
  //   读数与登记见 `tests/evidence/U-scale2-height-truth.md` §2/§4 与 `S23-floor-removal.md`。
  // 报错卡（§5.2.5）：抬头 · 下一步 · 「原文」三行。
  if (el.classList.contains("card-api-error")) return 60;
  // 事件条（「谁说的」稿 A）：抬头一行 28；展开着（交回 · 另一会话）加正文的一段粗估。中断标记 · 后台通知一行细线。
  if (el.classList.contains("card-speaker")) return (el as HTMLDetailsElement).open ? 28 + 60 : 28;
  if (el.classList.contains("card-event-line") || el.classList.contains("card-notice")) return 19;
  if (el.classList.contains("card-slash")) return 19;
  // 修法①:两行常数。card-api-retry 是"重试风暴"时成批出现的那一种。
  if (el.classList.contains("card-api-retry")) return 17;
  if (el.classList.contains("card-bash-input")) return 19;
  // 修法②:card-bash-output 按 header + min(行数, 20) 算。
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
 * 估值 → **真正写进 `contain-intrinsic-size` 的那个数**。全仓只有这一处。
 *
 * 导出不是为了给别人算高,是为了让**秤**有一个住址可问:
 * 秤 2 的门禁与 `U-scale2-probe-entry.ts` 都必须按"浏览器实际用了哪个数"算误差,
 * 而此前那个数被**各自抄了一份**(两处 `const APPLIED_FLOOR = 24`)
 * ⇒ 改这里不会让它们变,秤会在量一个不再出货的配置(`99 条 75` 落地清单第 3 处)。
 *
 * 🔴 **2026-09-18:这里原本有个 `Math.max(24, …)` 地板,已去掉**
 * (`99 条 75` /,读数 `tests/evidence/S22-floor-readings.md`)。
 * 三条理由,不要再把它加回来:
 *  ① **全局地板构造上不可能对** —— 正确值逐 class 不同(content-box 真高 min:
 *    retry 17.05 · slash/bash-input 18.59 · user 21.69 · …… · api-error 49.38);
 *    24 让 13/83 张卡虚高,300 张细条卡的风暴里总高虚高 **+20.5%/+23.8%**、
 *    滚动条抖 1683/1946px —— 就它自称要防的那件事而言,24 比真值**更坏**。
 *  ② **按 class 分档 ＝ 把常数抄第二遍**:class 判定整套住在 `estimateStreamNodeHeight`,
 *    这里只拿得到 `HTMLElement`。
 *  ③ **它该出声,不该静默夹取**:`Math.max` 会把「估值荒谬地小」抹平,估算器真坏了没人发现。
 *    保险职责已搬到判据层 —— `scale2-height-truth.vitest.ts` 的 **F1**
 *    「每个 class 的估值最小值 ≥ 该 class 登记的常数」,那一格会红,`Math.max` 不会。
 *
 * 注释原先自陈的两个防御对象都是空集(两个真引擎实测):0 只有 `card-user` 空正文一条路,
 * 而 `renderMessage` 根本不给空正文建卡;负值写不出来(各支全是非负项相加),
 * 就算写出来浏览器也**丢弃整条声明**落回 CSS 的 `auto 120px` —— 是虚高,不是塌陷。
 */
export function appliedIntrinsicPx(h: number): number {
  return Math.round(h);
}

/**
 * 建卡处统一入口:算出估值写 `contain-intrinsic-size: auto <h>px`。
 * F39(viewer 窗口化)复用同一模块供高——不要另写第二套(账本 §3)。
 */
export function applyIntrinsicSize(el: HTMLElement): void {
  const h = estimateStreamNodeHeight(el);
  if (h !== null) {
    el.style.setProperty("contain-intrinsic-size", `auto ${appliedIntrinsicPx(h)}px`);
  }
}

// ═══════════════════════════════════════════════════════════════════════
// 〔骨架〕第一级粗估：**不看 DOM、不看正文**，只看后端索引给的宽度无关料。
// ═══════════════════════════════════════════════════════════════════════

/**
 * 后端骨架索引的一行（`--read-session-from-offset … --index`，形状住 `IPC-PROTOCOL.md §10.3`）。
 * 零值 / 假值不上线 ⇒ 这里全是可选。
 */
export interface SkeletonFacts {
  /** 行起点字节偏移（绝对） */
  o: number;
  /** 行字节长（含 `\n`） */
  n: number;
  /** 记录 `type`（解析不出 ⇒ 缺） */
  t?: string;
  u?: string;
  sc?: boolean;
  /** user 记录是谁说的（后端判好的来源；人说的与工具结果省略） */
  sp?: string;
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
 * 一条记录在骨架里算哪一类：
 * - `none`：**不建卡**（meta / attachment / ai-title / 解析不出的…）——高 0，**也不打断**工具组的连续
 *   （渲染时它们根本不进 timeline，`peekPrev` 看得见的左邻居仍是上一个工具组）；
 * - `tool`：只有折叠单元的记录（纯 tool_use / 纯 tool_result）——连续的一串并成**一张**工具组卡；
 * - `card`：其余。
 */
export type SkeletonKind = "none" | "tool" | "card";

/**
 * 卡片外框（padding + border + 卡间 margin）的常数。
 * 有秤了：`tests/evidence/RENDER2-skel-golden.json`（真 Chromium 里读每张卡在流里占的位置
 * − 头 − 体，`RENDER2-skel-run.ts` 复算），门禁在 `tests/frontend/ui/scale2-height-truth.vitest.ts`「骨架外框」那一组：
 * 每一格 == 向上取整的中位数（「粗估宁可偏高」—— 取整只往上，不再拍）。
 * 原先拍的值与秤的差：user 36 → 50（少算了 14）· 工具组 46 → 49 · 系统细条 32 → 28 · assistant 20 → 19。
 */
const SKEL_USER_CHROME = 50;
const SKEL_CARD_CHROME = 19;
const SKEL_TOOL_GROUP_H = 49;
const SKEL_SYSTEM_H = 28;

/** 第一级里「正文之外那一段」四格（秤对拍的就是它们；assistant 那一格含头与块距）。只给判据读。 */
export const SKEL_OUTER = {
  user: SKEL_USER_CHROME,
  assistant: CARD_HEADER_H + BLOCK_GAP + SKEL_CARD_CHROME,
  toolGroup: SKEL_TOOL_GROUP_H,
  system: SKEL_SYSTEM_H,
} as const;

/** 字宽算术（口径 = `fallbackTextHeight`：CJK 全宽、其余 0.52em），折成行数的**上界**。 */
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

/** 一条记录属于哪一类（不看邻居）。 */
export function skeletonKind(f: SkeletonFacts): SkeletonKind {
  if (f.t !== "user" && f.t !== "assistant" && f.t !== "system") return "none";
  if (f.sp && !drawsCard(f.sp)) return "none";
  if (f.t === "system") return "card";
  const hasBody = (f.ch ?? 0) > 0 || (f.cb ?? 0) > 0;
  if (!hasBody) return (f.fd ?? 0) > 0 ? "tool" : "none";
  return "card";
}

/**
 * **第一级粗估**：由宽度无关料 × 当前列宽算一条记录在骨架里占多高（px，border-box 口径）。
 *
 * `prevKind` = 上一条**建卡**记录的类别（`none` 不算）—— 连续的 `tool` 只有第一条出高，其余并进同一个组。
 * `colW` 默认 `COL_W`（写死的 780 **留给 U1** 改成容器实测；本函数已经吃参数，U1 只需传进来）。
 *
 * 买到：骨架落地那一瞬间就有总高（滚动条大致对、跳转可用）。
 * **买不到**：精度。它不知道 slash/compact 细条、ESC 折叠、`stripInternalNoise` 剥空这些渲染期决定；
 * 第二级（建卡时 `applyIntrinsicSize` ＋ `contain-intrinsic-size: auto` 记住真值）接管已渲染的那部分。
 */
export function estimateFromFacts(
  f: SkeletonFacts,
  prevKind: SkeletonKind,
  colW: number = COL_W,
): number {
  const kind = skeletonKind(f);
  if (kind === "none") return 0;
  if (kind === "tool") return prevKind === "tool" ? 0 : SKEL_TOOL_GROUP_H;
  if (f.t === "system") return SKEL_SYSTEM_H;
  const folded = (f.fd ?? 0) * SUMMARY_H;
  if (f.t === "user") {
    const userW = colW * 0.8 - 34;
    return factLines(f, 14, userW) * LH_BASE + factCode(f) + folded + SKEL_USER_CHROME;
  }
  const paras = Math.max(0, (f.pl ?? 0) - 1) * P_GAP;
  return (
    CARD_HEADER_H +
    factLines(f, 15, colW) * LH_PROSE +
    paras +
    factCode(f) +
    folded +
    BLOCK_GAP +
    SKEL_CARD_CHROME
  );
}

/**
 * 〔「字节不是成本轴，卡型才是 …… 同一种卡正文越长越贵」〕**这条记录建卡时要当场物化的正文字符数**：
 * user 的文本（字符串 content 或 text 块）· assistant 的 text 块 · queue-operation 的 content。thinking / 工具调用 / 工具结果
 * 是折叠卡（展开才物化）、元数据不建卡 ⇒ 0。O(块数)：只取 `.length`，不序列化。批闸（`TabStreamView.BATCH_BODY_CHARS`）按它算。
 */
export function eagerBodyChars(message: JsonlRecord): number {
  switch (message.type) {
    case "user":
    case "assistant": {
      const c: unknown = (message.message as { content?: unknown } | undefined)?.content; // 残缺记录（夹具）不抛
      if (typeof c === "string") return message.type === "user" ? c.length : 0;
      if (!Array.isArray(c)) return 0;
      let n = 0;
      for (const b of c as Array<{ type?: unknown; text?: unknown }>) {
        if (b && b.type === "text" && typeof b.text === "string") n += b.text.length;
      }
      return n;
    }
    case "queue-operation":
      return message.content?.length ?? 0;
    default:
      return 0;
  }
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
  /** 超长正文只交前缀（`MEASURE_PREFIX_CHARS`，同 `textHeight`）：排出来的高 × `scale` 外推回全长（不超长 ⇒ 1）。 */
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

/**
 * 一条记录 ⇒ 第二级那一件；不值得精算的（不建卡 · 工具组 · 细条卡 · 没有正文）⇒ `null`（第一级就是它的高）。
 * 与 `estimateFromFacts` 逐项同形：只把 `factLines(…) × 行高` 换成 pretext 排出来的高。`colW` 同第一级。
 */
export function refineItemOf(rec: JsonlRecord, colW: number = COL_W): RefineItem | null {
  if (rec.type !== "user" && rec.type !== "assistant") return null;
  if (rec.type === "user" && !drawsCard(rec.userText.speaker.kind)) return null;
  const c: unknown = (rec.message as { content?: unknown } | undefined)?.content;
  let md = "";
  let folded = 0;
  if (typeof c === "string") md = c;
  else if (Array.isArray(c)) {
    const texts: string[] = [];
    for (const b of c as Array<{ type?: unknown; text?: unknown }>) {
      if (b?.type === "text" && typeof b.text === "string") texts.push(b.text);
      else if (b?.type === "tool_use" || b?.type === "tool_result" || b?.type === "thinking" || b?.type === "image") folded++;
    }
    md = texts.join("\n");
  }
  const { prose, codeLines, codeBlocks, paras } = splitFences(md);
  if (!prose.trim()) return null; // 纯工具 / 纯代码：第一级那份算术已经是它
  const code = codeLines * LH_MONO + codeBlocks * (CODE_BAR_H + CODE_PAD_V + CODE_MARGIN);
  const over = prose.length > MEASURE_PREFIX_CHARS;
  const text = over ? prose.slice(0, MEASURE_PREFIX_CHARS) : prose;
  const scale = over ? prose.length / MEASURE_PREFIX_CHARS : 1;
  if (rec.type === "user") {
    return {
      text,
      scale,
      font: `14px ${FONT_BASE}`,
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
