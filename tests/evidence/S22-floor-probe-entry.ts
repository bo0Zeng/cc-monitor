/**
 * S22 · `applyIntrinsicSize` 里那个 `Math.max(24, …)` 地板的**真浏览器读数**。
 *
 * 这一路只产读数，不改 `src/`。要回答的四问（原话）：
 *   ① 那个 24 是怎么来的？（→ 走 git，不在本文件）
 *   ② 它在防什么？`contain-intrinsic-size` 写一个过小的值会怎样 —— **真浏览器里量**：
 *      0/1/5/10/17/24 各会怎样 · 滚动条抖不抖 · 跳转落不落错位 · 引擎有没有下限
 *   ③ 今天盘上最矮的卡真高是多少（逐 class 的 min）
 *   ④ 地板该是多少 / 该不该有
 *
 * 起法与取值约定照抄 `tests/evidence/U-scale2-probe-entry.ts`（`window.__RESULT` +
 * `window.__DONE`），两个真引擎各跑一遍。**B 段的盒模型做法就是照它抄的**。
 *
 * # 它打六段读数
 *
 * - **A 段 · 逐 class 真高**：与秤 2 同一份冻结语料，关掉 `content-visibility` 让每张卡
 *   真排版，读 content-box。⚠ **这是现打的，不读 `U-scale2-truth-golden.json`**
 *   （那份金标准归另一路刷；这里现打是为了「读数一律现打」这条纪律）。
 *   产出：逐 class 的 min/p50/max content-box ⇒ 「最小合理占位」的下界在这里。
 *
 * - **B 段 · 声明值扫描（0…120 + 非法负值）**：一张**视口外、从没渲染过**的卡，
 *   inline 写 `contain-intrinsic-size: auto <D>px`，读 `getBoundingClientRect().height`。
 *   skipped 时读到的必然是 D（+ padding/border）⇒ 一次回答三件事：
 *   盒模型是 content-box 还是 border-box · 引擎有没有把小值往上夹 · 负值会不会被拒。
 *
 * - **C 段 · `auto` 的「记住真实尺寸」到底成不成立**：一张写 `auto 0px` 的卡，
 *   滚进视口渲染一次、再滚出去，量它 skipped 时的高度。
 *   读到 0+pad ⇒ 没记住（地板的影响是**永久**的）；读到真高 ⇒ 记住了
 *   （地板只影响**从没渲染过**的卡的首屏几何）。两个引擎分别判 —— 这一格决定地板的爆炸半径。
 *
 * - **D 段 · 重试风暴的滚动几何**：300 张真 `card-api-retry` 串成一条流，
 *   逐个声明值 D 跑三件事：估出来的 `scrollHeight` vs 真 `scrollHeight`；
 *   「跳到第 250 张」落点误差；从头滚到尾时 `scrollHeight` 的累计抖动量。
 *   ⇒ 「写小了会怎样」不是推理出来的，是量出来的。
 *
 * - **E 段 · 真语料上改地板的实际差额**：同一份 83 张卡的语料，地板取 0 / 17 / 24
 *   各跑一遍，量总高误差与跳转落点误差。⇒ 今天盘上改地板到底值多少像素。
 *
 * - **F 段 · 环境**：UA / 视口 / dpr / 各特性 `CSS.supports` / 承重 token 的 computed 值。
 *
 * # 🔴 反空真
 *
 * 每一段都带自己的计数与 `ok`。任何一段量到 0 行，`window.__RESULT.ok` 就是 `false`
 * 且 `failures[]` 写明哪一段空了；`S22-floor-run.sh` 的汇总段见到 `ok:false` **退 1**。
 * 「一张卡都没量到」必须是**失败**，不许被当成「没发现问题」。
 */
import "../../src/styles.css";
import fixtureJsonl from "../__fixtures__/scale2-height-records.jsonl?raw";
import { buildCorpus } from "../scale2-height-corpus";
import { estimateStreamNodeHeight } from "../../src/height-estimate";
import { renderMessage } from "../../src/cards/index";
import type { JsonlRecord, RenderContext } from "../../src/cards/index";

declare global {
  interface Window {
    __RESULT?: string;
    __DONE?: boolean;
  }
}

const failures: string[] = [];

function requireRows(label: string, n: number, min = 1): void {
  if (n < min) failures.push(`${label}：量到 ${n} 行（要求 ≥ ${min}）—— 这一段是空的，判失败`);
}

const rafs = (n: number): Promise<void> =>
  new Promise((resolve) => {
    let left = n;
    const tick = (): void => {
      if (--left <= 0) resolve();
      else requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });

/** 垂直方向的 padding + border 合计（`contain-intrinsic-size` 之外盒模型另加的那一份） */
function padBorderOf(el: HTMLElement): number {
  const cs = getComputedStyle(el);
  return (
    parseFloat(cs.paddingTop || "0") +
    parseFloat(cs.paddingBottom || "0") +
    parseFloat(cs.borderTopWidth || "0") +
    parseFloat(cs.borderBottomWidth || "0")
  );
}

function isSkipped(el: HTMLElement): boolean | null {
  try {
    const fn = (el as unknown as { checkVisibility?: (o: unknown) => boolean }).checkVisibility;
    if (typeof fn !== "function") return null;
    return !fn.call(el, { contentVisibilityAuto: true });
  } catch {
    return null;
  }
}

/** 造一个生产形状的滚动容器：`#message-stream`(relative) > `.stream.active` > `.stream-content` */
function makeScroller(id: string, w: number, h: number): { host: HTMLElement; stream: HTMLElement; content: HTMLElement } {
  const host = document.createElement("div");
  host.id = id;
  host.style.position = "fixed"; // 钉在视口里：c-v:auto 的「与用户相关」判定按视口算
  host.style.left = "0";
  host.style.top = "0";
  host.style.width = `${w}px`;
  host.style.height = `${h}px`;
  host.style.zIndex = "0";
  const stream = document.createElement("div");
  stream.className = "stream active";
  const content = document.createElement("div");
  content.className = "stream-content";
  stream.appendChild(content);
  host.appendChild(stream);
  document.body.appendChild(host);
  return { host, stream, content };
}

const VIEW_W = 900;
const VIEW_H = 600;

// ────────────────────────────────────────────────────────────────────────────
// A 段 · 逐 class 真高（现打，不读金标准）
// ────────────────────────────────────────────────────────────────────────────
interface TruthRow {
  id: string;
  cls: string;
  source: string;
  trueBorderBox: number;
  trueContentBox: number;
  padBorder: number;
  marginTopBottom: number;
  estRaw: number | null;
}

async function sectionA(): Promise<{ rows: TruthRow[]; retryTemplate: HTMLElement | null }> {
  const { host, content } = makeScroller("s22-truth", VIEW_W, VIEW_H);
  // 关 c-v：c-v:auto 的卡在视口外会被 skip，那时读到的是**估值本身**，
  // 拿它当真值就是自己跟自己比（`设计/17 §5.3` 点名的那个病）。
  const off = document.createElement("style");
  off.textContent = `#s22-truth .stream-content > * { content-visibility: visible !important; }`;
  document.head.appendChild(off);
  host.style.height = "auto"; // 让它自然撑开，别让 overflow 影响排版
  host.style.position = "absolute";

  const corpus = buildCorpus(fixtureJsonl);
  for (const item of corpus) content.appendChild(item.element);
  void content.getBoundingClientRect();
  await rafs(2);

  const rows: TruthRow[] = [];
  let retryTemplate: HTMLElement | null = null;
  for (const item of corpus) {
    const el = item.element;
    const rect = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    const pad = padBorderOf(el);
    rows.push({
      id: item.id,
      cls: item.cls,
      source: item.source,
      trueBorderBox: rect.height,
      trueContentBox: rect.height - pad,
      padBorder: pad,
      marginTopBottom: parseFloat(cs.marginTop || "0") + parseFloat(cs.marginBottom || "0"),
      estRaw: estimateStreamNodeHeight(el),
    });
    if (!retryTemplate && item.cls === "card-api-retry") retryTemplate = el.cloneNode(true) as HTMLElement;
  }
  requireRows("A 段 · 逐 class 真高", rows.length, 10);
  host.remove();
  off.remove();
  return { rows, retryTemplate };
}

// ────────────────────────────────────────────────────────────────────────────
// B 段 · 声明值扫描：盒模型 / 引擎下限 / 负值合法性
// ────────────────────────────────────────────────────────────────────────────
interface BoxRow {
  cls: string;
  /** 写进 inline style 的字面量；null = 不写（落 CSS 的 `auto 120px` 兜底） */
  declaredLiteral: string | null;
  /** 解析出来的数（负值/非法写 NaN） */
  declaredPx: number | null;
  padBorder: number;
  measured: number;
  /** measured − padBorder：这就是引擎真正当成 intrinsic height 的那个数 */
  effectiveIntrinsic: number;
  computedCIS: string;
  computedCV: string;
  skipped: boolean | null;
}

async function sectionB(): Promise<BoxRow[]> {
  const { host, content } = makeScroller("s22-box", VIEW_W, VIEW_H);
  host.style.position = "absolute";
  host.style.top = "500000px"; // 远在视口外 ⇒ c-v:auto 一定 skip
  host.style.height = "auto";

  const CLASSES = [
    "card card-api-retry",
    "card card-bash-input",
    "card card-slash",
    "card card-user",
    "card card-assistant",
  ];
  // 0 起步逐格扫，专找「引擎有没有把小值往上夹」；-1px 与 -0.5px 测非法值会不会被拒
  const DECLS: (string | null)[] = [
    null,
    "auto -1px",
    "auto 0px",
    "auto 1px",
    "auto 2px",
    "auto 3px",
    "auto 4px",
    "auto 5px",
    "auto 6px",
    "auto 8px",
    "auto 10px",
    "auto 12px",
    "auto 16px",
    "auto 17px",
    "auto 19px",
    "auto 24px",
    "auto 30px",
    "auto 120px",
  ];

  const made: { el: HTMLElement; cls: string; lit: string | null }[] = [];
  for (const cls of CLASSES) {
    for (const lit of DECLS) {
      const el = document.createElement("div");
      el.className = cls;
      el.textContent = "x";
      if (lit !== null) el.style.setProperty("contain-intrinsic-size", lit);
      content.appendChild(el);
      made.push({ el, cls, lit });
    }
  }
  void content.getBoundingClientRect();
  await rafs(2);

  const rows: BoxRow[] = [];
  for (const { el, cls, lit } of made) {
    const cs = getComputedStyle(el);
    const pad = padBorderOf(el);
    const m = lit === null ? null : /(-?[\d.]+)px/.exec(lit);
    const measured = el.getBoundingClientRect().height;
    rows.push({
      cls,
      declaredLiteral: lit,
      declaredPx: m ? parseFloat(m[1]) : null,
      padBorder: pad,
      measured,
      effectiveIntrinsic: measured - pad,
      computedCIS: cs.containIntrinsicSize || cs.getPropertyValue("contain-intrinsic-size"),
      computedCV: cs.contentVisibility || cs.getPropertyValue("content-visibility"),
      skipped: isSkipped(el),
    });
  }
  requireRows("B 段 · 声明值扫描", rows.length, CLASSES.length * DECLS.length);
  host.remove();
  return rows;
}

// ────────────────────────────────────────────────────────────────────────────
// C 段 · `auto` 关键字：渲染过一次之后，引擎记不记得真实尺寸？
// ────────────────────────────────────────────────────────────────────────────
interface RememberRow {
  declared: number;
  padBorder: number;
  /** 从没渲染过、skipped 时的高度 */
  beforeSkipped: number;
  /** 滚进视口、真排版时的高度 */
  whileVisible: number;
  /** 再滚出视口、重新 skipped 时的高度 */
  afterSkipped: number;
  skippedBefore: boolean | null;
  skippedAfter: boolean | null;
  /** afterSkipped ≈ whileVisible ⇒ 记住了 */
  remembered: boolean;
}

async function sectionC(template: HTMLElement): Promise<RememberRow[]> {
  const rows: RememberRow[] = [];
  for (const declared of [0, 17, 24]) {
    const { host, stream, content } = makeScroller(`s22-rem-${declared}`, VIEW_W, VIEW_H);
    // 前面垫一段足够高的占位，让目标卡一开始在视口外
    const spacer = document.createElement("div");
    spacer.style.height = "3000px";
    spacer.className = "s22-spacer";
    content.appendChild(spacer);
    const target = template.cloneNode(true) as HTMLElement;
    target.style.setProperty("contain-intrinsic-size", `auto ${declared}px`);
    content.appendChild(target);
    const tail = document.createElement("div");
    tail.style.height = "3000px";
    content.appendChild(tail);
    void content.getBoundingClientRect();
    await rafs(2);

    const pad = padBorderOf(target);
    const beforeSkipped = target.getBoundingClientRect().height;
    const skippedBefore = isSkipped(target);

    // 滚到它头上 ⇒ 强制材料化
    stream.scrollTop = target.offsetTop - 100;
    await rafs(3);
    const whileVisible = target.getBoundingClientRect().height;

    // 滚回顶 ⇒ 重新 skip
    stream.scrollTop = 0;
    await rafs(3);
    const afterSkipped = target.getBoundingClientRect().height;
    const skippedAfter = isSkipped(target);

    rows.push({
      declared,
      padBorder: pad,
      beforeSkipped,
      whileVisible,
      afterSkipped,
      skippedBefore,
      skippedAfter,
      remembered: Math.abs(afterSkipped - whileVisible) < 1,
    });
    host.remove();
  }
  requireRows("C 段 · auto 记忆", rows.length, 3);
  return rows;
}

// ────────────────────────────────────────────────────────────────────────────
// D/E 段共用的一次「滚动几何」试验
// ────────────────────────────────────────────────────────────────────────────
interface Trial {
  label: string;
  cards: number;
  /** 全部 skipped 时的 scrollHeight（= 估出来的总高） */
  estScrollHeight: number;
  /** 同一份内容关掉 c-v 后的 scrollHeight（= 真总高）；由调用方填 */
  trueScrollHeight: number | null;
  /** 跳转试验：按估值几何算出目标卡的 offsetTop 再跳过去，落点偏差（px，0=正中） */
  jumpRequested: number;
  jumpLandErr1: number;
  /** 生产里 scrollToMessage 有「双 rAF 幂等重发」一次；这是重发后的残差 */
  jumpLandErr2: number;
  /** 从头滚到尾，scrollHeight 的累计变化量与单步最大跳变 */
  sweepSteps: number;
  sweepSumAbsDelta: number;
  sweepMaxAbsDelta: number;
  sweepFinalScrollHeight: number;
  /** 每一步 requested vs actual scrollTop 的最大偏差（滚动位置被内容顶走的量） */
  sweepMaxScrollSlip: number;
}

type Declare = (el: HTMLElement) => number | null;

/**
 * ⚠ 一次试验只做一件事（`mode`）。
 * 理由是踩过的坑：跳转与扫一遍**放在同一批卡上顺序做**，跳转那一下已经把沿途的卡
 * 材料化并让引擎记住了真尺寸（C 段证实两个引擎都会记），接着扫出来的抖动量
 * 就是个**被污染的下界**（Chromium 在 declared=0 上一度扫出 Σ|Δ|=0）。
 * ⇒ 每个 mode 各起一批全新的卡、全新的滚动容器。
 */
async function runTrial(
  label: string,
  makeCards: () => HTMLElement[],
  declare: Declare,
  jumpIndex: number,
  cvOff: boolean,
  mode: "jump" | "sweep",
): Promise<Trial> {
  const id = `s22-trial-${Math.random().toString(36).slice(2, 8)}`;
  const { host, stream, content } = makeScroller(id, VIEW_W, VIEW_H);
  let off: HTMLElement | null = null;
  if (cvOff) {
    off = document.createElement("style");
    off.textContent = `#${id} .stream-content > * { content-visibility: visible !important; }`;
    document.head.appendChild(off);
  }
  const cards = makeCards();
  for (const el of cards) {
    const d = declare(el);
    if (d !== null) el.style.setProperty("contain-intrinsic-size", `auto ${d}px`);
    content.appendChild(el);
  }
  void content.getBoundingClientRect();
  await rafs(2);

  const estScrollHeight = stream.scrollHeight;

  let requested = NaN;
  let landErr1 = NaN;
  let landErr2 = NaN;
  let prev = stream.scrollHeight;
  let sum = NaN;
  let maxD = NaN;
  let maxSlip = NaN;
  let steps = 0;

  if (mode === "jump") {
    // ── 跳转：按当下（估值）几何算目标位置，跳过去，看落点 ────────────────
    const target = cards[Math.min(jumpIndex, cards.length - 1)];
    requested = target.offsetTop - stream.offsetTop;
    stream.scrollTop = requested;
    await rafs(3);
    const streamTop = stream.getBoundingClientRect().top;
    landErr1 = target.getBoundingClientRect().top - streamTop;
    // 生产 `scrollToMessage` 的「双 rAF 幂等重发」一次
    stream.scrollTop = target.offsetTop - stream.offsetTop;
    await rafs(3);
    landErr2 = target.getBoundingClientRect().top - stream.getBoundingClientRect().top;
  } else {
    // ── 扫一遍：从头滚到尾，记 scrollHeight 的抖动 ─────────────────────────
    sum = 0;
    maxD = 0;
    maxSlip = 0;
    const STEP = VIEW_H;
    for (let want = STEP; steps < 300; want += STEP) {
      stream.scrollTop = want;
      await rafs(2);
      steps++;
      const sh = stream.scrollHeight;
      const d = Math.abs(sh - prev);
      sum += d;
      if (d > maxD) maxD = d;
      prev = sh;
      const slip = Math.abs(stream.scrollTop - Math.min(want, sh - stream.clientHeight));
      if (slip > maxSlip) maxSlip = slip;
      if (stream.scrollTop >= sh - stream.clientHeight - 1) break;
    }
  }

  const out: Trial = {
    label,
    cards: cards.length,
    estScrollHeight,
    trueScrollHeight: null,
    jumpRequested: requested,
    jumpLandErr1: landErr1,
    jumpLandErr2: landErr2,
    sweepSteps: steps,
    sweepSumAbsDelta: sum,
    sweepMaxAbsDelta: maxD,
    sweepFinalScrollHeight: prev,
    sweepMaxScrollSlip: maxSlip,
  };
  host.remove();
  off?.remove();
  return out;
}

/** 跳转与扫一遍各起一批全新的卡，再把两半读数并成一行（理由见 `runTrial` 头注） */
async function runBoth(
  label: string,
  makeCards: () => HTMLElement[],
  declare: Declare,
  jumpIndex: number,
  cvOff: boolean,
): Promise<Trial> {
  const j = await runTrial(label, makeCards, declare, jumpIndex, cvOff, "jump");
  const s = await runTrial(label, makeCards, declare, jumpIndex, cvOff, "sweep");
  return {
    ...j,
    sweepSteps: s.sweepSteps,
    sweepSumAbsDelta: s.sweepSumAbsDelta,
    sweepMaxAbsDelta: s.sweepMaxAbsDelta,
    sweepFinalScrollHeight: s.sweepFinalScrollHeight,
    sweepMaxScrollSlip: s.sweepMaxScrollSlip,
  };
}

// ────────────────────────────────────────────────────────────────────────────
// G 段 · 地板自陈「防 0/负值」—— 0 到底出不出得来？出来了对不对？
// ────────────────────────────────────────────────────────────────────────────
interface DegenRow {
  label: string;
  /** renderMessage 到底有没有建出卡（建不出 ⇒ 这条路在生产里根本不存在） */
  built: boolean;
  cls: string | null;
  estRaw: number | null;
  estRounded: number | null;
  trueContentBox: number | null;
  trueBorderBox: number | null;
  padBorder: number | null;
  /** 不加地板时会写进 style 的那个数与真值的差 */
  errNoFloor: number | null;
  /** 加 24 地板后与真值的差 */
  errFloor24: number | null;
}

function degenCtx(): RenderContext {
  return {
    parentPath: "/tmp/s22/session.jsonl",
    origin: null,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  } as unknown as RenderContext;
}

async function sectionG(): Promise<DegenRow[]> {
  const { host, content } = makeScroller("s22-degen", VIEW_W, VIEW_H);
  host.style.position = "absolute";
  host.style.height = "auto";
  const off = document.createElement("style");
  off.textContent = `#s22-degen .stream-content > * { content-visibility: visible !important; }`;
  document.head.appendChild(off);

  const mkUser = (text: string, uuid: string): JsonlRecord =>
    ({
      type: "user",
      uuid,
      parentUuid: null,
      timestamp: "2026-09-18T12:34:56.000Z",
      message: { role: "user", content: text },
    }) as unknown as JsonlRecord;
  const mkAssistantText = (text: string, uuid: string): JsonlRecord =>
    ({
      type: "assistant",
      uuid,
      parentUuid: null,
      timestamp: "2026-09-18T12:34:56.000Z",
      message: { role: "assistant", model: "claude", content: [{ type: "text", text }] },
    }) as unknown as JsonlRecord;

  const cases: [string, JsonlRecord][] = [
    ["user / 空字符串", mkUser("", "degen-u0")],
    ["user / 只有空白", mkUser("   \n  \t ", "degen-u1")],
    ["user / 单个空格", mkUser(" ", "degen-u2")],
    ["user / 一个字", mkUser("好", "degen-u3")],
    ["assistant / 空文本块", mkAssistantText("", "degen-a0")],
    ["assistant / 只有空白", mkAssistantText("   ", "degen-a1")],
  ];

  const made: { label: string; el: HTMLElement | null }[] = [];
  for (const [label, rec] of cases) {
    let el: HTMLElement | null = null;
    try {
      const out = renderMessage(rec, degenCtx()) as unknown as {
        kind: string;
        element?: HTMLElement;
      };
      el = out && out.kind === "card" ? (out.element ?? null) : (out?.element ?? null);
    } catch {
      el = null;
    }
    if (el) content.appendChild(el);
    made.push({ label, el });
  }
  void content.getBoundingClientRect();
  await rafs(2);

  const rows: DegenRow[] = [];
  for (const { label, el } of made) {
    if (!el) {
      rows.push({
        label,
        built: false,
        cls: null,
        estRaw: null,
        estRounded: null,
        trueContentBox: null,
        trueBorderBox: null,
        padBorder: null,
        errNoFloor: null,
        errFloor24: null,
      });
      continue;
    }
    const pad = padBorderOf(el);
    const bb = el.getBoundingClientRect().height;
    const cb = bb - pad;
    const raw = estimateStreamNodeHeight(el);
    const rounded = raw === null ? null : Math.round(raw);
    rows.push({
      label,
      built: true,
      cls: Array.from(el.classList).find((c) => c.startsWith("card-")) ?? el.tagName,
      estRaw: raw,
      estRounded: rounded,
      trueContentBox: cb,
      trueBorderBox: bb,
      padBorder: pad,
      errNoFloor: rounded === null ? null : rounded - cb,
      errFloor24: rounded === null ? null : Math.max(24, rounded) - cb,
    });
  }
  requireRows("G 段 · 退化输入", rows.length, cases.length);
  host.remove();
  off.remove();
  return rows;
}

// ────────────────────────────────────────────────────────────────────────────
async function main(): Promise<void> {
  try {
    await (document as unknown as { fonts?: { ready?: Promise<unknown> } }).fonts?.ready;
  } catch {
    /* 没有 FontFaceSet 的引擎照跑 */
  }

  const { rows: truthRows, retryTemplate } = await sectionA();
  const boxRows = await sectionB();
  const rememberRows = retryTemplate ? await sectionC(retryTemplate) : [];
  if (!retryTemplate) failures.push("A 段没有产出 card-api-retry 模板 ⇒ C/D 段无法跑");

  // ── D 段 · 重试风暴 ────────────────────────────────────────────────────
  const STORM_N = 300;
  const stormTrials: Trial[] = [];
  if (retryTemplate) {
    const make = (): HTMLElement[] =>
      Array.from({ length: STORM_N }, () => retryTemplate.cloneNode(true) as HTMLElement);
    // 真值：关 c-v 跑一遍
    const truth = await runBoth("storm/真高(c-v off)", make, () => null, 250, true);
    stormTrials.push(truth);
    for (const d of [0, 1, 5, 10, 17, 24, 120]) {
      const t = await runBoth(`storm/declared ${d}px`, make, () => d, 250, false);
      t.trueScrollHeight = truth.estScrollHeight;
      stormTrials.push(t);
    }
  }
  requireRows("D 段 · 重试风暴", stormTrials.length, 8);

  // ── E 段 · 真语料上改地板的差额 ─────────────────────────────────────────
  const corpusTrials: Trial[] = [];
  {
    const make = (): HTMLElement[] => buildCorpus(fixtureJsonl).map((i) => i.element);
    const truth = await runBoth("corpus/真高(c-v off)", make, () => null, 82, true);
    corpusTrials.push(truth);
    for (const floor of [0, 17, 24]) {
      const declare: Declare = (el) => {
        const h = estimateStreamNodeHeight(el);
        return h === null ? null : Math.max(floor, Math.round(h));
      };
      const t = await runBoth(`corpus/floor ${floor}`, make, declare, 82, false);
      t.trueScrollHeight = truth.estScrollHeight;
      corpusTrials.push(t);
    }
  }
  requireRows("E 段 · 真语料", corpusTrials.length, 4);

  const degenRows = await sectionG();

  // ── F 段 · 环境 ────────────────────────────────────────────────────────
  const rootCs = getComputedStyle(document.documentElement);
  const env = {
    ua: navigator.userAgent,
    viewport: { w: innerWidth, h: innerHeight, dpr: devicePixelRatio },
    supports: {
      contentVisibilityAuto: CSS.supports("content-visibility", "auto"),
      cisAutoLen: CSS.supports("contain-intrinsic-size", "auto 120px"),
      cisZero: CSS.supports("contain-intrinsic-size", "auto 0px"),
      cisNegative: CSS.supports("contain-intrinsic-size", "auto -1px"),
      cisNone: CSS.supports("contain-intrinsic-size", "auto none"),
      overflowAnchor: CSS.supports("overflow-anchor", "auto"),
      checkVisibility: typeof document.body.checkVisibility === "function",
    },
    tokens: {
      "--stream-max-width": rootCs.getPropertyValue("--stream-max-width").trim(),
      "--font-size-xs": rootCs.getPropertyValue("--font-size-xs").trim(),
      "--font-size-small": rootCs.getPropertyValue("--font-size-small").trim(),
      "--font-size-prose": rootCs.getPropertyValue("--font-size-prose").trim(),
    },
  };

  window.__RESULT = JSON.stringify(
    {
      ok: failures.length === 0,
      failures,
      counts: {
        truthRows: truthRows.length,
        boxRows: boxRows.length,
        rememberRows: rememberRows.length,
        stormTrials: stormTrials.length,
        corpusTrials: corpusTrials.length,
        degenRows: degenRows.length,
      },
      env,
      truthRows,
      boxRows,
      rememberRows,
      stormTrials,
      corpusTrials,
      degenRows,
    },
    null,
    1,
  );
  window.__DONE = true;
}

void main().catch((e) => {
  window.__RESULT = JSON.stringify({
    ok: false,
    failures: [`探针抛异常：${String((e as Error)?.stack ?? e)}`],
    counts: {},
  });
  window.__DONE = true;
});
