/**
 * 秤 2（`设计/17 §6` 表第 2 行）的**真浏览器探针**。
 *
 * `设计/17 §6` 对秤 2 逐字要求「**必须真浏览器**（jsdom 无布局引擎）」。
 * 本文件被 vite 打成一个 IIFE，由 `U-scale2-run.sh` 塞进
 * WebKitGTK（PyGObject + WebKit2-4.1）与 Chromium（Playwright）两个引擎跑，
 * 起法照抄 `真相源/90 §3.1/§3.7` 那条已经被证明可行的路子，没有重新发明。
 *
 * # 它打三段读数
 *
 * - **A 段 · 真高**：整份语料（`tests/scale2-height-corpus.ts`）插进真的
 *   `.stream > .stream-content`（真的 `src/styles.css`），**关掉 `content-visibility`**
 *   让每张卡都真排版，逐卡读 `getBoundingClientRect().height`（border-box）
 *   ＋ 扣掉 padding/border 得到 **content-box**。
 *   关 c-v 的理由：c-v:auto 的卡在视口外会被 skip，那时读到的是**估值本身**，
 *   拿它当真值就是自己跟自己比（`设计/17 §5.3` 点名的那个病）。
 *
 * - **B 段 · `contain-intrinsic-size` 到底是 content-box 还是 border-box**：
 *   这一段专门回答 `height-estimate.ts` 自陈的那句「不加 padding/border：
 *   `contain-intrinsic-size` 是 content-box」到底成不成立。做法：一张真的
 *   `card-api-retry`（padding 3px 12px）放在**视口外、从没渲染过**的位置，
 *   inline 写 `contain-intrinsic-size: auto 24px`，读它的 `getBoundingClientRect().height`。
 *   **读到 24 ⇒ border-box；读到 24+padding+border ⇒ content-box。** 二选一，没有第三种。
 *
 * - **C 段 · 环境**：UA、视口、devicePixelRatio、`.stream-content` 实际列宽、
 *   几个承重 CSS token 的 computed 值（字号/行高/字体族是否真的落到了 fallback）。
 *   **这一段必须留**：真高是"这台机器上这套字体"的真高，不是生产 Windows WebView2 的真高。
 *
 * 结果写 `window.__RESULT`（JSON 文本）＋ `window.__DONE = true`，与
 * `真相源/90` 的两个 runner 的取值约定一致。
 */
import "../../src/styles.css";
import fixtureJsonl from "../__fixtures__/scale2-height-records.jsonl?raw";
import { buildCorpus, htmlFingerprint } from "../scale2-height-corpus";
import {
  estimateStreamNodeHeight,
  applyIntrinsicSize,
  extractProseText,
  codeBlockHeight,
} from "../../src/height-estimate";

declare global {
  interface Window {
    __RESULT?: string;
    __DONE?: boolean;
  }
}

const APPLIED_FLOOR = 24; // `applyIntrinsicSize` 里那个 `Math.max(24, …)`

interface Row {
  id: string;
  cls: string;
  source: string;
  bytes: number;
  bucket: string;
  /** 卡片 outerHTML 的长度 + 指纹 —— 金标准过期检测用（语料变了就必须重跑探针） */
  htmlLen: number;
  htmlHash: string;
  /** `estimateStreamNodeHeight` 的原始返回（null = 认不出，落 CSS 兜底 120px） */
  estRaw: number | null;
  /** `applyIntrinsicSize` 真正写进 style 的那个数（含 `Math.max(24,…)` 地板）；认不出时 = CSS 的 120 */
  estApplied: number;
  /** 地板有没有把 estRaw 顶上去 */
  flooredBy: number;
  /** 真高（border-box） */
  trueBorderBox: number;
  /** 真高（content-box）= border-box − padding − border */
  trueContentBox: number;
  padBorder: number;
  /** 这张卡写没写 inline contain-intrinsic-size（= estimateStreamNodeHeight 非 null） */
  estimated: boolean;
}

function boxOf(el: HTMLElement): { border: number; content: number; padBorder: number } {
  const rect = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  const pad =
    parseFloat(cs.paddingTop || "0") +
    parseFloat(cs.paddingBottom || "0") +
    parseFloat(cs.borderTopWidth || "0") +
    parseFloat(cs.borderBottomWidth || "0");
  return { border: rect.height, content: rect.height - pad, padBorder: pad };
}

async function main(): Promise<void> {
  // 等字体就绪 —— 不等的话首帧可能用 fallback 度量，真高会漂
  try {
    await (document as unknown as { fonts?: { ready?: Promise<unknown> } }).fonts?.ready;
  } catch {
    /* 没有 FontFaceSet 的引擎照跑 */
  }

  // ── 生产形状的容器 ──────────────────────────────────────────────────────
  const stream = document.createElement("div");
  stream.className = "stream active";
  const content = document.createElement("div");
  content.className = "stream-content";
  stream.appendChild(content);
  document.body.appendChild(stream);

  // A 段：关掉 c-v，逼每张卡真排版（理由见头注）
  const off = document.createElement("style");
  off.textContent = `
    #scale2-truth .stream-content > * { content-visibility: visible !important; }
    html, body { margin: 0; }
  `;
  document.head.appendChild(off);
  stream.id = "scale2-truth";

  const corpus = buildCorpus(fixtureJsonl);
  for (const item of corpus) content.appendChild(item.element);

  // 强制一次布局后再读（读一次就够，不要逐卡强制）
  void content.getBoundingClientRect();
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));

  const rows: Row[] = [];
  for (const item of corpus) {
    const el = item.element;
    // ⚠ 指纹必须在 `applyIntrinsicSize` **之前**取 —— 它会往 style 里写 inline
    // `contain-intrinsic-size`，写完再取指纹，jsdom 侧（没写过）就永远对不上，
    // 那条过期哨兵会变成**恒红**，与恒绿同样没用。
    const html = el.outerHTML;
    const estRaw = estimateStreamNodeHeight(el);
    applyIntrinsicSize(el);
    const applied = estRaw === null ? 120 : Math.max(APPLIED_FLOOR, Math.round(estRaw));
    const box = boxOf(el);
    rows.push({
      id: item.id,
      cls: item.cls,
      source: item.source,
      bytes: item.bytes,
      bucket: item.bucket,
      htmlLen: html.length,
      htmlHash: htmlFingerprint(html),
      estRaw,
      estApplied: applied,
      flooredBy: estRaw !== null && Math.round(estRaw) < APPLIED_FLOOR ? APPLIED_FLOOR - Math.round(estRaw) : 0,
      trueBorderBox: box.border,
      trueContentBox: box.content,
      padBorder: box.padBorder,
      estimated: estRaw !== null,
    });
  }

  const colWidth = content.getBoundingClientRect().width;

  // ── A′ 段 · `card-assistant` 的逐块拆账 ─────────────────────────────────
  // 主表一旦显示某个 class 系统性虚高，光有一个百分比没法改任何东西 ——
  // 这一段把误差摊到 `blockHeight` 的四条通道上（折叠 details / 正文 / 代码块 / 块间 gap），
  // 让"哪一条通道在虚高"变成读数而不是猜测。**只产读数，不参与门禁。**
  interface BlockRow {
    cardId: string;
    childIdx: number;
    tag: string;
    cls: string;
    /** 这个块的真实布局高度（border-box，含它自己的 margin 之外的部分） */
    trueH: number;
    /** 真实 marginTop + marginBottom（估高的 BLOCK_GAP/P_GAP 想替代的就是它） */
    trueMargin: number;
    detailsClosed: boolean;
    textLen: number;
    hardBreaks: number;
    blockCount: number;
    /** DOM 里真的 `<br>` 个数 —— 与 `hardBreaks` 对照，差额就是"源码换行被当成硬断行"的量 */
    brCount: number;
    /** 提取文本的前 200 字（`\n` 显式写成 `⏎`），肉眼判读断行从哪来 */
    sample: string;
    codeBlocks: number;
    /** Σ codeBlockHeight(pre)（估值侧的代码块那一支） */
    codeEstH: number;
    /** 真实代码块高度合计（含 .code-block 自己的 margin） */
    codeTrueH: number;
  }
  const blockRows: BlockRow[] = [];
  const assistants = corpus.filter((c) => c.cls === "card-assistant").slice(0, 12);
  for (const item of assistants) {
    const body = item.element.querySelector(".card-body");
    if (!body) continue;
    Array.from(body.children).forEach((node, idx) => {
      const child = node as HTMLElement;
      const cs = getComputedStyle(child);
      const { text, blockCount } = extractProseText(child);
      const pres = Array.from(child.querySelectorAll(".code-block pre"));
      let codeEstH = 0;
      for (const pre of pres) codeEstH += codeBlockHeight(pre.textContent ?? "");
      let codeTrueH = 0;
      for (const cb of Array.from(child.querySelectorAll(".code-block"))) {
        const ccs = getComputedStyle(cb);
        codeTrueH +=
          (cb as HTMLElement).getBoundingClientRect().height +
          parseFloat(ccs.marginTop || "0") +
          parseFloat(ccs.marginBottom || "0");
      }
      blockRows.push({
        cardId: item.id,
        childIdx: idx,
        tag: child.tagName,
        cls: child.className,
        trueH: child.getBoundingClientRect().height,
        trueMargin: parseFloat(cs.marginTop || "0") + parseFloat(cs.marginBottom || "0"),
        detailsClosed: child.tagName === "DETAILS" && !(child as HTMLDetailsElement).open,
        textLen: text.length,
        hardBreaks: (text.match(/\n/g) ?? []).length,
        blockCount,
        brCount: child.querySelectorAll("br").length,
        sample: text.slice(0, 200).replace(/\n/g, "⏎"),
        codeBlocks: pres.length,
        codeEstH,
        codeTrueH,
      });
    });
  }

  // ── B 段：contain-intrinsic-size 的盒模型 ────────────────────────────────
  // 视口外、从没渲染过的卡：读到的高度必然来自 contain-intrinsic-size 那个数。
  const farStream = document.createElement("div");
  farStream.className = "stream active";
  farStream.style.position = "absolute";
  farStream.style.top = "500000px"; // 远在视口外 ⇒ c-v:auto 一定 skip
  farStream.style.width = "1000px";
  const farContent = document.createElement("div");
  farContent.className = "stream-content";
  farStream.appendChild(farContent);
  document.body.appendChild(farStream);

  interface IntrinsicProbe {
    label: string;
    cls: string;
    declared: number | null;
    padBorder: number;
    measured: number;
    computedCIS: string;
    computedCV: string;
    skipped: boolean | null;
  }
  const intrinsic: IntrinsicProbe[] = [];
  const cases: [string, string, number | null][] = [
    ["card-api-retry / inline auto 24px（今天的常数）", "card card-api-retry", 24],
    ["card-api-retry / inline auto 100px（拉开差距好判读）", "card card-api-retry", 100],
    ["card-api-retry / 无 inline（落 CSS 兜底 auto 120px）", "card card-api-retry", null],
    ["card-bash-input / inline auto 32px（今天的常数）", "card card-bash-input", 32],
    ["card-api-error / inline auto 40px（今天的常数）", "card card-api-error", 40],
  ];
  for (const [, cls, declared] of cases) {
    const el = document.createElement("div");
    el.className = cls;
    el.textContent = "x";
    if (declared !== null) el.style.setProperty("contain-intrinsic-size", `auto ${declared}px`);
    farContent.appendChild(el);
  }
  void farContent.getBoundingClientRect();
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  Array.from(farContent.children).forEach((node, i) => {
    const el = node as HTMLElement;
    const [label, cls, declared] = cases[i];
    const cs = getComputedStyle(el);
    const pad =
      parseFloat(cs.paddingTop || "0") +
      parseFloat(cs.paddingBottom || "0") +
      parseFloat(cs.borderTopWidth || "0") +
      parseFloat(cs.borderBottomWidth || "0");
    let skipped: boolean | null = null;
    try {
      skipped = !(el as unknown as { checkVisibility: (o: unknown) => boolean }).checkVisibility({
        contentVisibilityAuto: true,
      });
    } catch {
      skipped = null;
    }
    intrinsic.push({
      label,
      cls,
      declared,
      padBorder: pad,
      measured: el.getBoundingClientRect().height,
      computedCIS: cs.containIntrinsicSize || cs.getPropertyValue("contain-intrinsic-size"),
      computedCV: cs.contentVisibility || cs.getPropertyValue("content-visibility"),
      skipped,
    });
  });

  // ── C 段：环境 ──────────────────────────────────────────────────────────
  const probeRetry = document.createElement("div");
  probeRetry.className = "card card-api-retry";
  probeRetry.textContent = "⚠ API 调用失败：Connection error · 重试 1/5 · 12:34";
  content.appendChild(probeRetry);
  void probeRetry.getBoundingClientRect();
  const retryCs = getComputedStyle(probeRetry);
  const rootCs = getComputedStyle(document.documentElement);
  const bodyCs = getComputedStyle(document.body);

  const env = {
    ua: navigator.userAgent,
    viewport: { w: innerWidth, h: innerHeight, dpr: devicePixelRatio },
    streamContentWidth: colWidth,
    // `height-estimate.ts` 的 COL_W = 780 假设这一格必须核
    tokens: {
      "--stream-max-width": rootCs.getPropertyValue("--stream-max-width").trim(),
      "--font-size-prose": rootCs.getPropertyValue("--font-size-prose").trim(),
      "--line-height-prose": rootCs.getPropertyValue("--line-height-prose").trim(),
      "--font-size-xs": rootCs.getPropertyValue("--font-size-xs").trim(),
      "--font-size-small": rootCs.getPropertyValue("--font-size-small").trim(),
      "--font-mono": rootCs.getPropertyValue("--font-mono").trim(),
    },
    bodyComputed: { font: bodyCs.font, lineHeight: bodyCs.lineHeight, fontFamily: bodyCs.fontFamily },
    apiRetryComputed: {
      fontSize: retryCs.fontSize,
      lineHeight: retryCs.lineHeight,
      paddingTop: retryCs.paddingTop,
      paddingBottom: retryCs.paddingBottom,
      borderTopWidth: retryCs.borderTopWidth,
      borderBottomWidth: retryCs.borderBottomWidth,
      whiteSpace: retryCs.whiteSpace,
      height: probeRetry.getBoundingClientRect().height,
    },
    supports: {
      contentVisibility: CSS.supports("content-visibility", "auto"),
      containIntrinsicSize: CSS.supports("contain-intrinsic-size", "auto 120px"),
    },
  };
  probeRetry.remove();

  window.__RESULT = JSON.stringify({ env, rows, intrinsic, blockRows }, null, 1);
  window.__DONE = true;
}

void main().catch((e) => {
  window.__RESULT = JSON.stringify({ error: String(e && (e as Error).stack) || String(e) });
  window.__DONE = true;
});
