/**
 * 秤 2 ——「估值 vs 真实布局高度」对照表 + 门禁（`设计/17 §6` 表第 2 行）。
 *
 * 设计逐字要的是：
 *   「每张卡的 `(class, 估值, 真值, 相对误差)`」
 *   「**必须真浏览器**（jsdom 无布局引擎）」
 *   「**门禁化**：『各 class 的 p90 相对误差 < 30%』——
 *     这是唯一能防止下次加新卡型又静默回到 120px 的机制」
 *
 * # 它怎么做到「在 jsdom 里跑，却不是自己跟自己比」
 *
 * **真高是金标准，估值是现算。**
 * - 真高来自真浏览器：`tests/evidence/U-scale2-probe-entry.ts` 在
 *   **Chromium 153（Blink，生产 WebView2 同引擎家族）** 与
 *   **WebKitGTK 2.52.6（Linux 侧 Tauri/wry 同一引擎）** 两个引擎里各跑一遍，
 *   逐卡读 `getBoundingClientRect().height`，固化进
 *   `tests/evidence/U-scale2-truth-golden.json`。
 *   真高只取决于 DOM + CSS + 引擎，**与 `height-estimate.ts` 一个字都无关** ⇒ 静态固化是合法的。
 * - 估值**每次跑门禁都现算**（`estimateStreamNodeHeight` 现场调用）。
 *   ⇒ 改坏一个常数，下面的门禁当场红。这条是「没红 ≠ 守住了」的解药，
 *     变异自检的原文记在 `tests/evidence/U-scale2-height-truth.md`。
 *
 * # ⚠ 它量不到什么（这段不完整本身就是缺陷）
 *
 * 1. **jsdom 没有 canvas ⇒ pretext 必然降级**。生产里 `textHeight` 走
 *    pretext（canvas 测宽），本文件里走的是 `fallbackTextHeight`（0.52em 均宽算术）。
 *    ⇒ **本文件现算的是「算术降级路」的估值**，不是生产主路的估值。
 *    生产主路（pretext）那一份存在金标准里（`estBrowser`），下面单独有一格量它，
 *    但那一格是**金标准**，改常数不会让它变 —— 两格的分工写在各自的用例名里，别混。
 * 2. **真高是"这台 Linux 无头机 + 这套 fallback 字体"的真高**，不是生产 Windows
 *    WebView2 的真高。`--font-prose` 里的 Source Serif 4 / PingFang SC / Microsoft YaHei
 *    本机一个都没装（读数见 `U-scale2-height-truth.md` 的环境段）。字体一换，
 *    正文卡的真高就会变。**跨平台那一格今天没有。**
 * 3. `设计/17 §5` 第 2 条「一次强制布局读多少钱」本文件**不答**，它在「分不清」里挂着。
 *
 * 复算（真浏览器那一半）：`bash tests/evidence/U-scale2-run.sh`
 * 复算（本文件）：       `npx vitest run tests/scale2-height-truth.vitest.ts`
 * 读数：                 `tests/evidence/U-scale2-height-truth.md`
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { buildCorpus, htmlFingerprint } from "./scale2-height-corpus";
import { estimateStreamNodeHeight, appliedIntrinsicPx } from "../src/height-estimate";

// `__dirname` 在 vitest 里指向 `tests/`（同 `scale3-one-screen-gate.vitest.ts` 的用法）
const FIXTURE = resolve(__dirname, "__fixtures__/scale2-height-records.jsonl");
const GOLDEN = resolve(__dirname, "evidence/U-scale2-truth-golden.json");

interface GoldenRow {
  id: string;
  cls: string;
  source: string;
  bytes: number;
  bucket: string;
  htmlLen: number;
  htmlHash: string;
  estBrowser: number | null;
  estAppliedBrowser: number;
  trueBorderBox: number;
  trueContentBox: number;
  padBorder: number;
}
interface Golden {
  generatedAt: string;
  engine: { name: string; ua: string };
  crossCheck: { name: string; ua: string } | null;
  intrinsic: {
    label: string;
    cls: string;
    declared: number | null;
    padBorder: number;
    measured: number;
    /**
     * 这一格的 `declared` 是**探针现算的落地值**（`appliedIntrinsicPx`），不是写死的常数。
     * 悬案③ 按它找，不按数值找 —— 按数值找就是「靠一个恰好相等的数生效」，改动一来会静默取消。
     */
    shipped?: boolean;
  }[];
  rows: GoldenRow[];
  crossRows: { id: string; trueContentBox: number }[] | null;
}

const golden = JSON.parse(readFileSync(GOLDEN, "utf8")) as Golden;
const corpus = buildCorpus(readFileSync(FIXTURE, "utf8"));
const byId = new Map(golden.rows.map((r) => [r.id, r]));

/** `styles.css` 的 `.stream-content > * { contain-intrinsic-size: auto 120px }` */
const CSS_FALLBACK_PX = 120;

/**
 * 估不出高 → 不写 inline style → 浏览器用 CSS 的 120px 兜底。
 * 估得出高 → 走 **`appliedIntrinsicPx`（`src/height-estimate.ts`，全仓唯一住址）**。
 * 两条路都要按"浏览器实际用了哪个数"算误差。
 *
 * 🔴 这里原先是 `Math.max(APPLIED_FLOOR, Math.round(raw))`，而那个 `APPLIED_FLOOR = 24`
 * 是 `applyIntrinsicSize` 里那个 `Math.max(24, …)` 的**手抄副本**（探针里还有第三份）。
 * 一个数三个住址 ⇒ 改源码那份，这杆秤不会跟着变，会**安静地在量一个不再出货的配置**。
 * 2026-09-18（`99 条 75`）地板去掉时一并收成一个住址：本文件从此**调**它，不**抄**它。
 */
function appliedEstimate(el: HTMLElement): number {
  const raw = estimateStreamNodeHeight(el);
  return raw === null ? CSS_FALLBACK_PX : appliedIntrinsicPx(raw);
}

function p(values: number[], q: number): number {
  const v = [...values].sort((a, b) => a - b);
  if (!v.length) return NaN;
  const k = (v.length - 1) * q;
  const f = Math.floor(k);
  const c = Math.min(f + 1, v.length - 1);
  return v[f] + (v[c] - v[f]) * (k - f);
}

/** 按 class 归并，返回 `class -> 相对误差数组` */
function relErrByClass(
  estOf: (row: GoldenRow, el: HTMLElement) => number,
): Map<string, number[]> {
  const out = new Map<string, number[]>();
  for (const item of corpus) {
    const row = byId.get(item.id);
    if (!row || row.trueContentBox <= 0) continue;
    const err =
      Math.abs(estOf(row, item.element) - row.trueContentBox) /
      row.trueContentBox;
    if (!out.has(item.cls)) out.set(item.cls, []);
    out.get(item.cls)!.push(err);
  }
  return out;
}

/**
 * ★ **各 class 的 p90 相对误差上限（登记表，只许降）**。
 *
 * `设计/17 §6` 给秤 2 的门槛逐字是「**各 class 的 p90 相对误差 < 30%**」。
 * 🔴 **装上去当天，9 个卡型里 5 个不满足。** 所以这张表分两段：
 *
 * - **达标段**（≤ 0.30）：就按设计门槛钉死。新卡型默认进这一段。
 * - **超标段**（> 0.30）：逐条登记今天的实测值 + 一点裕度，并写明**为什么超**。
 *   登记不是豁免 —— `EXCEEDS_DESIGN_GATE` 那一格会把这份名单原样抬到人眼前，
 *   名单变长变短都红，谁也没法安静地多加一个。
 *
 * ⚠ 上限只许降。修好了就回来把数字改小（并把名字从 `EXCEEDS_DESIGN_GATE` 里删掉）——
 * 那一步会让门禁红一次，**那正是它想要的**：没有人能靠"修好了"绕过登记。
 */
const P90_CEILING: Record<string, number> = {
  // ⚠ 下面每条注释里的实测值都是 **2026-09-18 去掉地板之后**重打的
  //   （Chromium 153 / WebKitGTK 2.52.6，content-box 口径，金标准 `U-scale2-truth-golden.json`）。
  //   走到今天这一步的三次修，逐条：
  //   ① `extractProseText` 把「markdown 产物里的排版换行」归一成空格（正文卡 ~2× 虚高的根）；
  //   ② 三条细条常数从 border-box 手算值改成 content-box 实测值（24/32/34 → 17/19/19）；
  //   ③ 🔴 `applyIntrinsicSize` 的 `Math.max(24, …)` 地板**整个去掉**（`99 条 75` / `设计/17 订正④`）
  //      —— ② 改的那三条常数此前被地板顶回 24，**一个字都没出货**；③ 才让它们真的落地。
  //   ②后 → ③后（Chromium p90 / WebKitGTK p90）：
  //      api-retry  40.8% / 41.2%  →  **0.3% / 0.0%**
  //      bash-input 29.1% / 26.3%  →  **2.2% / 0.0%**
  //      slash      29.1% / 33.3%  →  **2.2% / 5.6%**（WebKit 那个**超设计门槛**的 33.3% 没了）
  //      user       10.7% / 14.3%  →  **1.4% / 4.8%**
  //      其余五个 class 一个点都没动（它们的估值最小值都 ≥ 38，够不着旧地板）。
  // ── 达标段：设计门槛 ──
  // 🔴 正文卡（最值钱的一格）：`extractProseText` 修完之后**第一次**够到设计门槛。
  //   ⚠ WebKitGTK 上是 26.7%（字体 fallback 不同，真高差几个点）——两引擎都在 30% 内，
  //     但裕度只有 3.3 个点，**这条线是擦着过的**，别再往上放语料就不看。
  "card-assistant": 0.3, //    实测 p90 21.8% / 26.7%，max 21.8% / 26.7%（双向）
  "card-bash-output": 0.3, //  实测 p90 8.6% / 10.0%，max 15.2%
  "card-compact": 0.3, //      实测 p90 9.8% / 11.8%
  "card-tool-group": 0.3, //   实测 p90 9.8% / 11.8%
  "card-user": 0.3, //         实测 p90 1.4% / 4.8%（地板去掉前 10.7% / 14.3%）
  // 🔴 这三条此前**不是常数不准，是地板把常数吃了**：常数 17/19/19 对真值 17.05/18.59/18.59
  //   本来只差 0.3%/2.2%/2.2%，地板一律顶成 24 ⇒ 落地误差停在 40.8%/29.1%/29.1%。
  //   地板去掉之后它们**第一次**量的是常数自己。
  "card-bash-input": 0.3, //   实测 p90 2.2% / 0.0%（地板去掉前 29.1% / 26.3%）
  "card-slash": 0.3, //        实测 p90 2.2% / 5.6%（地板去掉前 29.1% / **33.3% 超线**）
  // 🔴 这一条 2026-09-18 从超标段**回到达标段**：上限 0.45 → 0.3，名字同步从
  //   `EXCEEDS_DESIGN_GATE` 里删掉（那一步让门禁红了一次，那正是登记制度想要的）。
  "card-api-retry": 0.3, //    实测 p90 0.3% / 0.0%（地板去掉前 40.8% / 41.2%）
  // ── 超标段：逐条写明成因 ──
  // 反向：常数 40 是**单行**错误卡的高度，可 `card-api-error` 有 `.api-error-body`
  // （`white-space: pre-wrap`）会多行 ⇒ 长报错整个虚低。`设计/17 §2.2` 早就点过这一条。
  // ⚠ 这一条**与地板无关**（估值 40 ≥ 旧地板 24，从来没被顶过）⇒ 本轮它一个点都没动，
  //   这也正是「地板改动的爆炸半径只有四个 class」那句话的对照组。
  "card-api-error": 0.9, //    实测 p90 70.7% / 69.8%（**虚低**方向）
};

/**
 * 今天**不满足**设计那句「p90 < 30%」的 class（只许变短；变长变短都要回来改这里）。
 *
 * 这份名单 2026-09-18 一天里缩了两次：
 *   · 上半场 **5 条 → 2 条**：`card-assistant`（正文卡 ~2× 虚高，根是 `extractProseText`
 *     把排版换行当硬断行）、`card-bash-input`、`card-slash` 三条随 `extractProseText`
 *     与三条 content-box 常数一起修掉；
 *   · 🔴 下半场 **2 条 → 1 条**：`applyIntrinsicSize` 的 `Math.max(24, …)` 地板去掉之后，
 *     `card-api-retry` 的 p90 从 40.8% 掉到 **0.3%**（`99 条 75` / `设计/17 订正④`）。
 *     它此前之所以在名单上，**不是常数算错**，是常数 17 被地板顶回 24 —— 那一格是**地板的账**。
 *
 * 剩下这一条不是地板的账，也还没修：
 *   · `card-api-error` —— 常数 40 假设"单行"，而构造体里有多行长报错 ⇒ 虚低；
 *     而且这个卡型真语料里 **0 条**，读数带着"构造体像不像真的"这个前提。
 */
const EXCEEDS_DESIGN_GATE = ["card-api-error"];

/**
 * ★ **F1 · 每个 class 的估值最小值 ≥ 该 class 登记的常数**（`99 条 75` / `设计/17 订正④`）。
 *
 * # 它是来顶班的
 *
 * `applyIntrinsicSize` 里原本有个 `Math.max(24, …)` 地板，注释自陈「防 0/负值」。
 * 2026-09-18 把它去掉了，三条理由（读数 `tests/evidence/S22-floor-readings.md`）：
 *   ① 全局地板**构造上不可能对** —— 正确值逐 class 不同，一个数管九个 class 必然错几个；
 *   ② 按 class 分档 ＝ **把常数抄第二遍**（class 判定整套住在 `estimateStreamNodeHeight` 里）；
 *   ③ 🔴 **它该出声，不该静默夹取** —— `Math.max` 把「估值荒谬地小」抹平了，
 *      估算器真坏了也没人发现（本仓纪律：要么答对，要么出声，不许静默兜底）。
 *
 * ⇒ **保险职责搬到这里**。地板夹取的那件事（估值塌到不合理的小），由本判据**红**出来。
 *
 * # 为什么这里抄一份常数是对的，而地板里分档抄一份是错的
 *
 * 同一个数落两处，在**实现**里是病（下次加卡型漏改一处 ⇒ 静默落回兜底），
 * 在**判据**里恰恰是机制：判据这一份就是拿来和源码那一份对撞的 ——
 * 改源码不改这里就红，那正是登记制度要的那一下。
 *
 * # 射程（写清，别让人当成"估高全对"）
 *
 * - 对六个**常数驱动**的 class（retry / slash / bash-input / api-error / compact / tool-group），
 *   登记值 **＝ 源码那个常数本身** ⇒ 常数被改小**当场红**（变异 M11/M12 实证，见
 *   `tests/evidence/U-scale2-mutation-log.txt`）。
 * - 对三个**算出来**的 class（user / assistant / bash-output），登记的是它们**结构上的下界**
 *   （一行正文 / 卡头 / 输出头），⚠ **比今天实测的 min 松**（见每条的注释）——
 *   它逮得住「塌到荒谬」，逮不住「小幅变差」。小幅变差那一格归 `P90_CEILING`。
 * - 本判据在 **jsdom** 里现算（pretext 降级），所以 user/assistant 两条与真浏览器的
 *   estRaw 略有出入；登记的是下界，不受这点出入影响。
 */
const CLASS_MIN_EST: Record<string, { min: number; why: string }> = {
  // ── 常数驱动：登记值 = 源码常数，逐位相等 ──
  "card-api-retry": { min: 17, why: "常数 17（11px × 1.55 = 17.05，content-box 实测真高 17.05）" },
  "card-slash": { min: 19, why: "常数 19（12px × 1.55 = 18.59）" },
  "card-bash-input": { min: 19, why: "常数 19（同一套紧凑系 token）" },
  "card-api-error": { min: 40, why: "常数 40（单行错误卡）" },
  "card-compact": { min: 38, why: "SUMMARY_H = 38（折叠 <details> 只剩 summary 行）" },
  "card-tool-group": { min: 38, why: "SUMMARY_H = 38（同上）" },
  // ── 算出来的：登记结构下界，⚠ 比今天语料实测的 min 松 ──
  "card-bash-output": {
    min: 19,
    why: "BASH_OUTPUT_HEADER_H = 19（只有 header 的极限）；⚠ 今天语料实测 min 42（= header 19 + 空态 23），登记值松 23px",
  },
  "card-user": {
    min: 21.7,
    why: "LH_BASE = 14 × 1.55（一行正文）；⚠ 今天语料实测 min 21.7 ⇒ **这一条是紧的**。正文为空时 textHeight 返回 0 ⇒ 本判据会红 —— 那正是地板从前抹平的那一形",
  },
  "card-assistant": {
    min: 22,
    why: "CARD_HEADER_H = 22（body 一个块都没有的极限）；⚠ 今天语料实测 min 56.75，登记值松 34.75px",
  },
};

/** 这些卡型的估值是**纯常数 / 纯算术**，不碰 pretext ⇒ jsdom 与真浏览器必须逐位相同。 */
const PRETEXT_FREE = new Set([
  "card-api-retry",
  "card-api-error",
  "card-slash",
  "card-bash-input",
  "card-bash-output",
  "card-compact",
  "card-tool-group",
]);

describe("秤 2 · 金标准没过期（过期的秤比没有秤更坏）", () => {
  it("抽取器自检：语料真的建出卡来了，而且覆盖到多个卡型", () => {
    expect(
      corpus.length,
      "一张卡都没建出来 —— 下面每一格都会零命中地绿",
    ).toBeGreaterThan(50);
    expect(
      new Set(corpus.map((c) => c.cls)).size,
      "只建出一个卡型 —— 分桶表没有意义",
    ).toBeGreaterThanOrEqual(8);
  });

  it("金标准与现算语料逐条对得上（id / class / DOM 指纹）", () => {
    const mismatches: string[] = [];
    for (const item of corpus) {
      const row = byId.get(item.id);
      if (!row) {
        mismatches.push(`${item.id}（${item.cls}）金标准里没有`);
        continue;
      }
      if (row.cls !== item.cls)
        mismatches.push(
          `${item.id} 卡型漂了：金标准 ${row.cls} → 现算 ${item.cls}`,
        );
      const fp = htmlFingerprint(item.element.outerHTML);
      if (fp !== row.htmlHash) {
        mismatches.push(
          `${item.id}（${item.cls}）DOM 指纹漂了：${row.htmlHash} → ${fp}`,
        );
      }
    }
    expect(
      mismatches,
      "★ 金标准过期了 —— 语料或卡片渲染改过，真高必须重打：`bash tests/evidence/U-scale2-run.sh`。\n" +
        "  不重打的话，下面所有误差都是拿新估值去比旧真高，**数字还在，含义没了**。\n" +
        mismatches
          .slice(0, 10)
          .map((m) => `    ${m}`)
          .join("\n"),
    ).toEqual([]);
    expect(corpus.length).toBe(golden.rows.length);
  });

  it("两个引擎的真高互相对得上（差 >5% 的格子要点名）", () => {
    if (!golden.crossRows) return; // 只跑了一个引擎时不假装有对照
    const cross = new Map(
      golden.crossRows.map((r) => [r.id, r.trueContentBox]),
    );
    const far: string[] = [];
    for (const row of golden.rows) {
      const other = cross.get(row.id);
      if (other === undefined || row.trueContentBox <= 0) continue;
      const d = Math.abs(other - row.trueContentBox) / row.trueContentBox;
      if (d > 0.05)
        far.push(
          `${row.id}（${row.cls}）${row.trueContentBox} vs ${other} = ${(d * 100).toFixed(1)}%`,
        );
    }
    // 不拿它当门禁（字体/引擎差异是真实存在的），但差得多要在日志里看得见
    if (far.length)
      console.log(
        `[秤2] 两引擎真高差 >5% 的格子（${far.length} 个）：\n  ${far.join("\n  ")}`,
      );
    expect(
      far.length,
      `两引擎真高在超过 1/3 的格子上对不上，其中一份多半是坏的：\n  ${far.slice(0, 8).join("\n  ")}`,
    ).toBeLessThan(golden.rows.length / 3);
  });
});

describe("秤 2 · 门禁：各 class 的 p90 相对误差", () => {
  it("★ 现算路（估值现场算 vs 真浏览器真高）：每个 class 都在登记上限内", () => {
    const byCls = relErrByClass((_row, el) => appliedEstimate(el));
    const over: string[] = [];
    for (const [cls, errs] of byCls) {
      const ceiling = P90_CEILING[cls];
      if (ceiling === undefined) {
        over.push(
          `${cls}：登记表里没有这个卡型（p90=${(p(errs, 0.9) * 100).toFixed(1)}%）`,
        );
        continue;
      }
      const p90 = p(errs, 0.9);
      if (p90 > ceiling) {
        over.push(
          `${cls}：p90=${(p90 * 100).toFixed(1)}% > 上限 ${(ceiling * 100).toFixed(0)}%（n=${errs.length}）`,
        );
      }
    }
    expect(
      over,
      "★ 估高精度回归了，或者新卡型没登记。\n" +
        "  这一格是**现算**的（`estimateStreamNodeHeight` 当场调用），所以改坏一个常数它就会红。\n" +
        "  新加卡型的话：先跑 `bash tests/evidence/U-scale2-run.sh` 拿真高，再把它写进 `P90_CEILING`。\n" +
        "  ⚠ 上限只许降。把上限调上去让今天好过 = 把这杆秤关掉。\n" +
        over.map((o) => `    ${o}`).join("\n"),
    ).toEqual([]);
  });

  it("★ 生产主路（pretext）：金标准里那份估值也在同一批上限内", () => {
    const byCls = relErrByClass((row) => row.estAppliedBrowser);
    const over: string[] = [];
    for (const [cls, errs] of byCls) {
      const ceiling = P90_CEILING[cls] ?? 0.3;
      const p90 = p(errs, 0.9);
      if (p90 > ceiling)
        over.push(
          `${cls}：p90=${(p90 * 100).toFixed(1)}% > ${(ceiling * 100).toFixed(0)}%`,
        );
    }
    expect(
      over,
      "⚠ 这一格量的是**金标准里存着的**那份估值（真浏览器里 pretext 路算的），" +
        "改常数不会让它变 —— 要它动必须重跑探针。它在这里是为了让「降级路准不准」和「主路准不准」分开可见。\n" +
        over.map((o) => `    ${o}`).join("\n"),
    ).toEqual([]);
  });

  it("★ 登记表：今天不满足设计那句「p90 < 30%」的就是这几个（名单只许变短）", () => {
    const byCls = relErrByClass((row) => row.estAppliedBrowser);
    const over = [...byCls.entries()]
      .filter(([, errs]) => p(errs, 0.9) > 0.3)
      .map(([cls]) => cls)
      .sort();
    expect(
      over,
      "★ `设计/17 §6` 给秤 2 的门槛逐字是「各 class 的 p90 相对误差 < 30%」。\n" +
        "  这一格把**今天还够不着那条线的卡型**原样抬到人眼前。\n" +
        "  名单变长 ⇒ 有卡型的估高退步了或新卡型没调准；\n" +
        "  名单变短 ⇒ 修好了，回来把它从 `EXCEEDS_DESIGN_GATE` 删掉、并把 `P90_CEILING` 拧到 0.3。\n" +
        "  **两个方向都红是故意的** —— 登记表不是豁免，是让豁免没法安静地留着。",
    ).toEqual(EXCEEDS_DESIGN_GATE);
  });

  it("★ 常数型卡：jsdom 现算的估值必须与真浏览器逐位相同", () => {
    const diffs: string[] = [];
    for (const item of corpus) {
      if (!PRETEXT_FREE.has(item.cls)) continue;
      const row = byId.get(item.id);
      if (!row) continue;
      const live = appliedEstimate(item.element);
      if (live !== row.estAppliedBrowser) {
        diffs.push(
          `${item.id}（${item.cls}）现算 ${live} ≠ 金标准 ${row.estAppliedBrowser}`,
        );
      }
    }
    expect(
      diffs,
      "★ 这几个卡型的估值是**纯常数 / 纯算术**（不碰 pretext、不碰 canvas），\n" +
        "  所以 jsdom 与真浏览器算出来必须一模一样。对不上只有两种可能：\n" +
        "  ① 有人改了常数（那就去重跑探针、重定上限）；② 金标准过期。\n" +
        "  这是本文件**最灵敏**的一格 —— 改一个数字就红。\n" +
        diffs.map((d) => `    ${d}`).join("\n"),
    ).toEqual([]);
  });
});

describe("秤 2 · F1：估值最小值不许塌（地板去掉之后接手保险的那一格）", () => {
  /** `class -> 语料里 estimateStreamNodeHeight 的最小返回值`（null 不算，它走 CSS 兜底那条路） */
  function minEstByClass(): Map<string, { min: number; id: string; nulls: number }> {
    const out = new Map<string, { min: number; id: string; nulls: number }>();
    for (const item of corpus) {
      const raw = estimateStreamNodeHeight(item.element);
      const cur = out.get(item.cls) ?? { min: Infinity, id: "", nulls: 0 };
      if (raw === null) cur.nulls++;
      else if (raw < cur.min) {
        cur.min = raw;
        cur.id = item.id;
      }
      out.set(item.cls, cur);
    }
    return out;
  }

  it("★ F1：每个 class 的 `estimateStreamNodeHeight` 最小值 ≥ 该 class 登记的常数", () => {
    const mins = minEstByClass();
    expect(mins.size, "一个卡型都没量到 —— 本判据在零命中地绿").toBeGreaterThanOrEqual(8);
    const bad: string[] = [];
    for (const [cls, v] of mins) {
      const reg = CLASS_MIN_EST[cls];
      if (!reg) {
        bad.push(`${cls}：F1 登记表里没有这个卡型（今天实测 min=${v.min.toFixed(2)}）`);
        continue;
      }
      if (v.min < reg.min) {
        bad.push(
          `${cls}：min=${v.min.toFixed(2)}（${v.id}）< 登记的 ${reg.min} —— ${reg.why}`,
        );
      }
    }
    expect(
      bad,
      "★ **F1 红 = 估算器给某个卡型算出了一个荒谬地小的值。**\n" +
        "  这一格是 `applyIntrinsicSize` 那个 `Math.max(24, …)` 地板的**替身**：\n" +
        "  地板会把这种值**静默顶上去**（谁也不知道估算器坏了），本判据把它**喊出来**。\n" +
        "  （`99 条 75` / `设计/17 订正④`；地板为什么不该留，见 `tests/evidence/S22-floor-readings.md`）\n" +
        "  两条路：① 估高真写坏了 ⇒ 去修 `src/height-estimate.ts`；\n" +
        "          ② 常数是**故意**改小的 ⇒ 回来改 `CLASS_MIN_EST`，并跑一次\n" +
        "             `bash tests/evidence/U-scale2-run.sh` 重打金标准、重定 `P90_CEILING`。\n" +
        bad.map((b) => `    ${b}`).join("\n"),
    ).toEqual([]);
  });

  it("★ F1 登记表不许有幽灵条目（语料里不存在的 class 登记了也是零命中地绿）", () => {
    const present = new Set(corpus.map((c) => c.cls));
    const ghosts = Object.keys(CLASS_MIN_EST).filter((c) => !present.has(c)).sort();
    expect(
      ghosts,
      "这些卡型登记了 F1 下界，但今天的语料里一张都没有 ⇒ 那几条登记**从来没被执行过**。\n" +
        "  要么把语料补上，要么把条目删掉 —— 留着会让人以为它在守。",
    ).toEqual([]);
  });

  it("读数：逐 class 的估值最小值 vs F1 登记的下界（裕度就是本判据的钝度）", () => {
    const mins = minEstByClass();
    const rows = [...mins.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([cls, v]) => {
        const reg = CLASS_MIN_EST[cls];
        const slack = reg ? v.min - reg.min : NaN;
        return (
          `  ${cls.padEnd(17)} min=${v.min.toFixed(2).padStart(8)}（${v.id.padEnd(8)}）` +
          ` 登记下界=${String(reg?.min ?? "—").padStart(5)} 裕度=${slack.toFixed(2).padStart(7)}` +
          (v.nulls ? `  ⚠ 另有 ${v.nulls} 张估不出高（走 CSS 120 兜底）` : "")
        );
      });
    console.log(
      [
        "[秤2] F1 · 逐 class 估值最小值（jsdom 现算）vs 登记下界",
        "  裕度 0 = 这一条是紧的（常数改小就红）；裕度大 = 这一条只逮得住「塌到荒谬」。",
        ...rows,
      ].join("\n"),
    );
    expect(rows.length).toBeGreaterThanOrEqual(8);
  });
});

describe("秤 2 · 读数（不做判据，只产表）", () => {
  it("读数：按 card class 的相对误差分布", () => {
    const live = relErrByClass((_r, el) => appliedEstimate(el));
    const browser = relErrByClass((r) => r.estAppliedBrowser);
    const rows: string[] = [];
    for (const cls of [...browser.keys()].sort()) {
      const b = browser.get(cls)!;
      const l = live.get(cls) ?? [];
      const items = corpus.filter((c) => c.cls === cls);
      const src = [
        ...new Set(items.map((c) => (c.source === "shaped" ? "真形" : "构造"))),
      ].join("+");
      const signed = items
        .map((c) => byId.get(c.id))
        .filter((r): r is GoldenRow => !!r && r.trueContentBox > 0)
        .map(
          (r) => (r.estAppliedBrowser - r.trueContentBox) / r.trueContentBox,
        );
      const dir = signed.every((s) => s >= 0)
        ? "虚高"
        : signed.every((s) => s <= 0)
          ? "虚低"
          : "双向";
      const f = (x: number) => `${(x * 100).toFixed(1)}%`.padStart(7);
      rows.push(
        `  ${cls.padEnd(17)}${String(b.length).padStart(3)} ${src.padEnd(5)}|${f(p(b, 0.5))}${f(p(b, 0.9))}${f(Math.max(...b))} |` +
          `${f(p(l, 0.5))}${f(p(l, 0.9))}${f(Math.max(...l))} | ${dir}`,
      );
    }
    console.log(
      [
        `[秤2] 估值 vs 真实布局高度 · 相对误差（真高金标准：${golden.engine.name}，${golden.generatedAt}）`,
        "  口径：|估值 − 真高(content-box)| / 真高(content-box)。",
        "        估值 = 浏览器实际用的那个数（`appliedIntrinsicPx`，今天 = round；估不出高的按 CSS 兜底 120px）。",
        "        真高取 content-box，因为 B 段实测 `contain-intrinsic-size` 就是 content-box。",
        "  card class         n 语料 |  主路(pretext) p50/p90/max | 降级路(算术) p50/p90/max | 方向",
        ...rows,
      ].join("\n"),
    );
    expect(rows.length).toBeGreaterThanOrEqual(8);
  });

  it("★ 没有第二个地板悄悄回来：写进 style 的值 == round(估值)，逐张卡", () => {
    // 🔴 这一格是**判据**，不是读数。它顶替的是原先那条「地板顶起了几张卡」的读数 ——
    //    读数只会打印，不会拦人；而「有人给估值加了个夹取」这件事必须拦。
    //    `appliedIntrinsicPx` 今天只做一件事：四舍五入。任何 `Math.max` / `Math.min` /
    //    「小于 N 就按 N 算」重新长出来，这一格当场红，并把是哪几张卡点名。
    const clamped = corpus
      .map((c) => ({ c, raw: estimateStreamNodeHeight(c.element) }))
      .filter(
        (x): x is { c: (typeof corpus)[number]; raw: number } =>
          x.raw !== null && appliedIntrinsicPx(x.raw) !== Math.round(x.raw),
      );
    expect(
      clamped.map(
        (x) =>
          `${x.c.id}（${x.c.cls}）估值 ${x.raw.toFixed(2)} → 写进 style 的是 ${appliedIntrinsicPx(x.raw)}`,
      ),
      "★ `applyIntrinsicSize` 又开始夹取估值了。\n" +
        "  2026-09-18 之前这里有个 `Math.max(24, …)` 地板，它把 **13/83 张卡**顶高，\n" +
        "  其中三条细条常数（17/19/19）**整个没出货** —— 改了常数、p90 一个点不动，\n" +
        "  人却以为修好了。地板已按 `99 条 75` / `设计/17 订正④` 去掉，理由与两个真引擎的读数\n" +
        "  见 `tests/evidence/S22-floor-readings.md`：全局地板构造上不可能对、分档等于把常数抄\n" +
        "  第二遍、而「估值荒谬地小」**该出声不该静默夹取**（那一格现在归 F1）。\n" +
        "  要再加夹取，先去推翻那份读数。",
    ).toEqual([]);
  });

  it("读数：地板去掉之后，写进 style 的数变了哪几张卡（对照金标准里那一列）", () => {
    // 金标准 `estAppliedBrowser` 是**重打过**的（地板去掉之后那一轮），所以这里两列应当逐位相同；
    // 它与 `P90_CEILING` 注释里那份「修前」数字的差，就是这次改动的全部爆炸半径。
    const rows = corpus
      .map((c) => ({ c, raw: estimateStreamNodeHeight(c.element), row: byId.get(c.id) }))
      .filter((x) => x.raw !== null && x.row && Math.round(x.raw) < 24);
    console.log(
      [
        `[秤2] 旧地板（24px）从前顶起过的卡：${rows.length}/${corpus.length} 张 —— 现在它们原样出货`,
        ...rows.map(
          (x) =>
            `  ${x.c.id}（${x.c.cls}）估值 ${x.raw!.toFixed(1)} → 写进 style 的是 ${appliedIntrinsicPx(x.raw!)}（旧地板下是 24）`,
        ),
        "  ⇒ 四个 class：card-api-retry(5) · card-bash-input(3) · card-slash(3) · card-user(2)。",
        "     其余五个 class 的估值最小值都 ≥ 38，够不着旧地板 ⇒ **一个点都不该动**（上面的 p90 表可核）。",
      ].join("\n"),
    );
    expect(rows.length).toBeGreaterThan(0); // 只产读数；为 0 说明语料里的细条卡没了，那是另一回事
  });
});

describe("秤 2 · `contain-intrinsic-size` 的盒模型（B 段真浏览器实测）", () => {
  it("★ 实测：它是 content-box —— 声明 N ⇒ 实际占 N + padding + border", () => {
    const withDecl = golden.intrinsic.filter((x) => x.declared !== null);
    expect(
      withDecl.length,
      "B 段一格都没有 —— 探针没跑到那一段",
    ).toBeGreaterThanOrEqual(3);
    for (const x of withDecl) {
      expect(
        x.measured,
        `${x.label}：声明 ${x.declared}px，padding+border ${x.padBorder}px，实测 ${x.measured}px。\n` +
          "  若实测 == 声明 ⇒ 它是 border-box，那 `height-estimate.ts` 头上那句" +
          "「不加 padding/border：contain-intrinsic-size 是 content-box」就是错的。",
      ).toBeCloseTo((x.declared as number) + x.padBorder, 1);
    }
  });

  it("读数：三个细条常数落地后实际占多少位置", () => {
    console.log(
      [
        "[秤2] B 段 · `contain-intrinsic-size` 是 content-box（两引擎读数完全一致）",
        ...golden.intrinsic.map(
          (x) =>
            `  ${x.label.padEnd(44)} 声明=${String(x.declared).padStart(4)} +padding/border=${String(x.padBorder).padStart(3)} ⇒ 实测占 ${x.measured}px`,
        ),
        "  🔴 第一格（`shipped`）的声明值**不是写死的** —— 它是 `appliedIntrinsicPx` 今天真往",
        "     一张 `card-api-retry` 的 style 里写的那个数，探针现算。改了估高它自己跟着动。",
        "     其余几格是写死的对照值（24/100/32/40 与无 inline 的 CSS 兜底），只用来答「盒模型是哪个」。",
        "  ⇒ 今天 `card-api-retry` 写 **17** ⇒ 视口外实际占 **23px**，而真高 23.05px ⇒ 虚高 **0.2%**。",
        "     2026-09-18 之前那个 `Math.max(24, …)` 地板下它写 24 ⇒ 占 **30px** ⇒ 虚高 **30%**",
        "     （而不是 `真相源` 里按手算推的 4% —— 手算漏了一份 padding，B 段这一列就是订正）。",
        "  ⇒ 下面悬案③那段推导的前提**跟着换了**：每张 retry 从 30px 变成 23px。那一段已重算。",
      ].join("\n"),
    );
    expect(golden.intrinsic.length).toBeGreaterThan(0);
  });
});

describe("秤 2 · 悬案③：一屏门控的三段边界，换上实测值之后还成不成立", () => {
  /**
   * # 这一段被改过两次前提，第二次差点**没人回来改**
   *
   * `设计/17 §6` 那条订正里的三段边界表，原是**带着「真高 = 手算 23.05px、估值 = 24px」
   * 这个前提**算出来的（R 路自陈）。两次订正：
   *
   * | 订正 | 一张 `card-api-retry` 在视口外对 `scrollHeight` 的贡献 |
   * |---|---|
   * | R 路手算 | 24px（估值本身） |
   * | 秤 2 的 B 段（2026-09-18 上半场） | **30px** —— 差一份 padding（CIS 是 content-box） |
   * | 🔴 地板去掉之后（`99 条 75`，本轮） | **23px** ＝ 常数 17 ＋ padding 6 |
   *
   * ⚠ **第二次订正这一段不会自己红**，而它确实没红 —— 本轮实测过：
   * `src/height-estimate.ts` 的地板已经去掉、金标准已经重打，这一整段**照样全绿**，
   * 因为它的 `estH` 是去金标准里找 `intrinsic[declared === 24]` 那一格读的，
   * 而那一格（作为对照）还在。⇒ 它会**安静地继续量一个不再出货的配置**。
   * 🔴 **那正是本仓治的那一族**：靠一个恰好相等的数生效的边界，被改动静默取消。
   *
   * ⇒ 修法不是把 24 换成 17（那只是把同一个巧合往后推一次），而是**改成从源码推**：
   * `estH = appliedIntrinsicPx(估值) + padBorder`，两项都不是常数 ——
   * 前者是 `applyIntrinsicSize` 今天真写进 style 的那个数（现算），
   * 后者是金标准量到的 padding+border。下面还钉了两条哨兵，任何一条对不上就红：
   *   ① 金标准里那个 `shipped` 格的 `declared` 必须 **== 今天现算的落地值**（金标准过期就红）；
   *   ② 该格的 `measured` 必须 **== declared + padBorder**（content-box 模型不成立就红）。
   */
  const GATE = { clientHeight: 800, maxRounds: 4, tailK: 150 };

  /** 旧 `materializeUntilFilled` 的算术（与 `scale3-one-screen-gate.vitest.ts::simulateGate` 同形，只保留 retry 那一路） */
  function gateRed(
    n: number,
    estPerCard: number,
    truePerCard: number,
  ): boolean {
    let cards = 0;
    let est = 0;
    for (let round = 0; round < GATE.maxRounds; round++) {
      if (round > 0 && est - GATE.clientHeight > 1) break;
      cards += n; // 一批 150 条里 n 条 retry，其余 skip
      est += n * estPerCard;
    }
    return cards * truePerCard < GATE.clientHeight; // 真高不足一屏 = 红
  }

  function redBand(estPerCard: number, truePerCard: number): number[] {
    const red: number[] = [];
    for (let n = 1; n <= GATE.tailK; n++)
      if (gateRed(n, estPerCard, truePerCard)) red.push(n);
    return red;
  }

  /** 连续区间压成 `a–b`，方便读也方便断言 */
  function bands(ns: number[]): string {
    const out: string[] = [];
    for (let i = 0; i < ns.length; ) {
      let j = i;
      while (j + 1 < ns.length && ns[j + 1] === ns[j] + 1) j++;
      out.push(i === j ? `${ns[i]}` : `${ns[i]}–${ns[j]}`);
      i = j + 1;
    }
    return out.join(" ∪ ") || "（空）";
  }

  it("★ 悬案③：地板去掉之后，那两条红带**是地板造出来的，跟着地板一起没了**", () => {
    const retryRow = golden.rows.find((r) => r.cls === "card-api-retry");
    expect(retryRow, "语料里没有 card-api-retry —— 下面全是空话").toBeDefined();
    const retryItem = corpus.find((c) => c.cls === "card-api-retry");
    expect(retryItem, "语料里建不出 card-api-retry —— 下面全是空话").toBeDefined();

    // ① 今天**真写进 style** 的那个数（现算，走生产同一条 `appliedIntrinsicPx`）
    const shippedPx = appliedEstimate(retryItem!.element);
    // ② 金标准里那个跟着源码走的 B 段格子 —— 两条哨兵钉住它
    const cis = golden.intrinsic.find((x) => x.shipped);
    expect(
      cis,
      "金标准里没有 `shipped` 那一格 —— 探针是旧版打的，重跑 `bash tests/evidence/U-scale2-run.sh`",
    ).toBeDefined();
    expect(
      cis!.declared,
      "★ 金标准过期了：它记着的落地值与今天现算的不一样 ⇒ 下面整段在量一个不再出货的配置。\n" +
        "  重跑 `bash tests/evidence/U-scale2-run.sh`。",
    ).toBe(shippedPx);
    expect(
      cis!.measured,
      "★ `contain-intrinsic-size` 不是 content-box 了（或这一格坏了）—— 下面的 estH 推导不成立",
    ).toBeCloseTo(shippedPx + cis!.padBorder, 1);

    const trueH = retryRow!.trueBorderBox; // 实测真高（border-box）
    const estH = shippedPx + cis!.padBorder; // 视口外一张 retry 实际占的位置（**从源码推，不从写死的 declared 读**）

    const handCalc = redBand(24, 23.05); // R 路的手算前提
    const floor24 = redBand(30, trueH); // 旧地板（24 + padding 6 = 30px/张）
    const measured = redBand(estH, trueH); // 今天：地板去掉之后

    // 前提本身先钉住：今天每张 retry 占 23px（= 常数 17 + padding 6），不再是 30px
    expect(estH, "每张 retry 视口外占的位置变了 —— 下面四条结论要重打").toBeCloseTo(23, 1);

    // R 路的结论在它自己的前提下复现得上：红点 = 1..8 ∪ {17} ∪ {34}
    expect(handCalc.filter((n) => n > 8)).toEqual([17, 34]);
    // 旧地板（本轮之前这一段量的就是这个配置）：1–11 ∪ [14,17] ∪ [27,34]
    expect(floor24.filter((n) => n > 11)).toEqual([
      14, 15, 16, 17, 27, 28, 29, 30, 31, 32, 33, 34,
    ]);
    // 🔴 地板去掉之后：两条带整个消失，只剩最低那一段
    expect(measured).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    // 设计里那句「取 18–30 条/150 最稳」——在旧地板下 27–30 是红的，现在**重新成立**
    for (const n of [18, 26, 27, 28, 29, 30]) {
      expect(
        gateRed(n, estH, trueH),
        `n=${n} 在今天的估值下应当是绿的`,
      ).toBe(false);
    }
    // 但 n ≤ 8 那一段**不是**估值的账，估得再准它也红 —— 见下面的说明
    expect(gateRed(8, trueH, trueH), "n=8 在「估值 == 真高」下仍应红（四轮上限）").toBe(true);

    console.log(
      [
        "[秤2] 悬案③ · 一屏门控的三段边界（🔴 2026-09-18 地板去掉之后**重算**）",
        `  前提：clientHeight=${GATE.clientHeight} · round<${GATE.maxRounds} · 每轮 ${GATE.tailK} 条`,
        `  ① R 路手算（估值 24 / 真高 23.05）       ：红 = ${bands(handCalc)}`,
        `  ② 旧地板（估值 30 = 24+6 / 真高 ${trueH.toFixed(3)}）：红 = ${bands(floor24)}`,
        `  ③ **今天**（估值 ${estH} = ${shippedPx}+${cis!.padBorder} / 真高 ${trueH.toFixed(3)}）：红 = ${bands(measured)}`,
        "  ⇒ ① 「n=17 是洞」**没了**：[14,17] 那条带是**估值虚高 30% 造出来的**，地板一去就消失；",
        "     ② [27,34] 那条带同理**没了**（它的成因是「第一轮就判满了，而真高不够一屏」）；",
        "     ③ 最低那一段从 n≤11 缩回 **n≤8**；",
        "     ④ 🔴 `设计/17 §6` 那句「取 **18–30** 条/150 最稳」**重新成立** —— 旧地板下 27–30 全红，",
        "        今天全绿。上一版这里登记的「实测下 18–26 最稳」是**地板的账**，已作废。",
        "  🔴 **剩下的 n≤8 不是估值的账**：把估值换成真高本身（估得完全准）它照样红 ——",
        "     成因是 `maxRounds=4` 这个上限：4×8×23.05 = 737.6 < 800，补四轮也填不满一屏。",
        "     ⇒ 估高再准也治不了它，那一格归秤 3 / 归补批轮次，不归这里。",
        "  ⚠ 口径与 R 路逐字对齐：两边都**不算 margin**（`.card-api-retry` 的 4px 上下 margin",
        "     在估值侧和真高侧同时存在，算进去红带从 1–8 左移到 1–6，**四条结论的方向一个都不变**）。",
        "  ⚠ 这一段此前的 `estH` 是去金标准找 `declared === 24` 那一格读的 ⇒ 地板一改它照样绿，",
        "     只是从此在量一个不再出货的配置。现在改成从 `appliedIntrinsicPx` 现算 + 两条哨兵钉住，",
        "     **源码一动这里就跟着动或者当场红**。",
      ].join("\n"),
    );
  });

  it("★ 口径自检：算上 margin 之后四条结论的方向不变（不然上面那句 ⚠ 是空话）", () => {
    const retryRow = golden.rows.find((r) => r.cls === "card-api-retry")!;
    const retryItem = corpus.find((c) => c.cls === "card-api-retry")!;
    const cis = golden.intrinsic.find((x) => x.shipped)!;
    const MARGIN = 8; // .card-api-retry 上下各 4px
    const estH = appliedEstimate(retryItem.element) + cis.padBorder;
    const withMargin = redBand(estH + MARGIN, retryRow.trueBorderBox + MARGIN);
    expect(withMargin).toEqual([1, 2, 3, 4, 5, 6]);
    console.log(
      `[秤2] 悬案③ · 算上 margin（每张 +8px）：红 = ${bands(withMargin)}` +
        "（不算 margin 时是 1–8）⇒ 只是整段左移，没有新带出现。",
    );
  });
});
