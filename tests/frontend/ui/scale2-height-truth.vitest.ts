/**
 * 秤 2 ——「估值 vs 真实布局高度」对照表 + 门禁（表第 2 行）。
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
 *   ⇒ 改坏一个常数，下面的门禁当场红。这条是「没红 ≠ 守住了」的解药。
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
 *    本机一个都没装。字体一换，
 *    正文卡的真高就会变。**跨平台那一格今天没有。**
 * 3. 「一次强制布局读多少钱」本文件**不答**，它在「分不清」里挂着。
 *
 * 复算（真浏览器那一半）：`bash tests/evidence/U-scale2-run.sh`
 * 复算（本文件）：       `npx vitest run tests/frontend/ui/scale2-height-truth.vitest.ts`
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { buildCorpus, htmlFingerprint } from "./scale2-height-corpus";
import { estimateStreamNodeHeight, appliedIntrinsicPx } from "../../../src/frontend/ui/height-estimate";
import { nonEmpty } from "../../test-support/non-empty";

// `__dirname` 在 vitest 里指向 `tests/`（同 `scale3-one-screen-gate.vitest.ts` 的用法）
const FIXTURE = resolve(__dirname, "../../__fixtures__/scale2-height-line-records.jsonl");
const GOLDEN = resolve(__dirname, "../../evidence/U-scale2-truth-golden.json");

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
  // 🔴 〔09-19 加〕打这份金标准那一趟的**显示时区与 locale**。
  //   语料 94 张卡全部带时间戳 ⇒ 这两个值决定渲染出来的字面与字数，进而决定真高。
  //   设成可选：加这条判据之前打的金标准没有它们，那一形由下面那条判据点名报出来，
  //   **不靠类型系统悄悄放过去**。
  env?: { displayLocale?: string; timeZone?: string };
  crossCheck: { name: string; ua: string } | null;
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
 * 估得出高 → 走 **`appliedIntrinsicPx`（`src/frontend/ui/height-estimate.ts`，全仓唯一住址）**。
 * 两条路都要按"浏览器实际用了哪个数"算误差。
 *
 * 🔴 这里原先是 `Math.max(APPLIED_FLOOR, Math.round(raw))`，而那个 `APPLIED_FLOOR = 24`
 * 是 `applyIntrinsicSize` 里那个 `Math.max(24, …)` 的**手抄副本**（探针里还有第三份）。
 * 一个数三个住址 ⇒ 改源码那份，这杆秤不会跟着变，会**安静地在量一个不再出货的配置**。
 * 地板去掉时一并收成一个住址：本文件从此**调**它，不**抄**它。
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
 * 给秤 2 的门槛逐字是「**各 class 的 p90 相对误差 < 30%**」。
 * 🔴 **装上去当天，9 个卡型里 5 个不满足。** 所以这张表分两段：
 *
 * - **达标段**（≤ 0.30）：就按设计门槛钉死。新卡型默认进这一段。
 * - **超标段**（> 0.30）：逐条登记今天的实测值 + 一点裕度，并写明**为什么超**（今天一条都没有）。
 *
 * ⚠ 上限只许降。
 */
const P90_CEILING: Record<string, number> = {
  // ⚠ 下面每条注释里的实测值都是 **2026-09-18 去掉地板之后**重打的
  //   （Chromium 153 / WebKitGTK 2.52.6，content-box 口径，金标准 `U-scale2-truth-golden.json`）。
  //   走到今天这一步的三次修，逐条：
  //   ① `extractProseText` 把「markdown 产物里的排版换行」归一成空格（正文卡 ~2× 虚高的根）；
  //   ② 三条细条常数从 border-box 手算值改成 content-box 实测值（24/32/34 → 17/19/19）；
  //   ③ 🔴 `applyIntrinsicSize` 的 `Math.max(24, …)` 地板**整个去掉**
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
  "card-steps": 0.3, //        实测 p90 6.4% / 5.0%，max 14.8% / 13.6%（虚低：带结果一句的那一步比 summary 高 2.6px；10-10 换形，原「工具组」整组按 38 算）
  "card-user": 0.3, //         实测 p90 1.4% / 4.8%（地板去掉前 10.7% / 14.3%）
  // 🔴 这三条此前**不是常数不准，是地板把常数吃了**：常数 17/19/19 对真值 17.05/18.59/18.59
  //   本来只差 0.3%/2.2%/2.2%，地板一律顶成 24 ⇒ 落地误差停在 40.8%/29.1%/29.1%。
  //   地板去掉之后它们**第一次**量的是常数自己。
  "card-bash-input": 0.3, //   实测 p90 2.2% / 0.0%（地板去掉前 29.1% / 26.3%）
  "card-slash": 0.3, //        实测 p90 2.2% / 5.6%（地板去掉前 29.1% / **33.3% 超线**）
  // 🔴 这一条 2026-09-18 从超标段**回到达标段**：上限 0.45 → 0.3。
  "card-api-retry": 0.3, //    实测 p90 0.3% / 0.0%（地板去掉前 40.8% / 41.2%）
  // ── 超标段：逐条写明成因 ──
  // 反向：常数 40 是**单行**错误卡的高度，可 `card-api-error` 有 `.api-error-body`
  // （`white-space: pre-wrap`）会多行 ⇒ 长报错整个虚低。早就点过这一条。
  // ⚠ 这一条**与地板无关**（估值 40 ≥ 旧地板 24，从来没被顶过）⇒ 它一个点都没动，
  //   这也正是「地板改动的爆炸半径只有四个 class」那句话的对照组。
  "card-api-error": 0.3, //    实测 p90 7.5% / 4.8%（虚低）。主窗口第 2 批：报错卡改成固定三行（抬头 · 下一步 · 原文折叠），常数 40 → 60；此前 70.7% / 69.8%
};

/**
 * ★ **F1 · 每个 class 的估值最小值 ≥ 该 class 登记的常数**。
 *
 * # 它是来顶班的
 *
 * `applyIntrinsicSize` 里原本有个 `Math.max(24, …)` 地板，注释自陈「防 0/负值」。
 * 2026-09-18 把它去掉了，三条理由：
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
 * - 对六个**常数驱动**的 class（retry / slash / bash-input / api-error / compact / steps），
 *   登记值 **＝ 源码那个常数本身** ⇒ 常数被改小**当场红**（变异 M11/M12 实证）。
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
  "card-api-error": { min: 60, why: "常数 60（抬头 · 下一步 · 原文折叠三行）" },
  "card-compact": { min: 38, why: "SUMMARY_H = 38（折叠 <details> 只剩 summary 行）" },
  "card-steps": { min: 38, why: "一步一行 SUMMARY_H = 38（收着的 <details> 只剩 summary；一条记录至少一步）" },
  // ── 算出来的：登记结构下界，⚠ 比今天语料实测的 min 松 ──
  "card-bash-output": {
    min: 19,
    why: "BASH_OUTPUT_HEADER_H = 19（只有 header 的极限）；⚠ 今天语料实测 min 42（= header 19 + 空态 23），登记值松 23px",
  },
  "card-user": {
    min: 21.7,
    why: "LH_BASE = 14 × 1.55（一行正文）；⚠ 今天语料实测 min 21.7 ⇒ **这一条是紧的**。正文为空时 textHeight 返回 0 ⇒ 本判据会红 —— 那正是加一层地板会抹平的那一形",
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
  "card-steps",
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

  // 🔴 〔09-19 立〕**这一条要排在指纹那条前面。**
  //   起因：断网沙箱里下面那条红，而宿主上它绿 —— 同一个提交、同一份 node_modules。
  //   它报的是「金标准过期了 —— 语料或卡片渲染改过」，**那句话是错的**：一个字都没改，
  //   差的是**这台机器的显示时区与 locale**（宿主 zh-CN / America-LA，容器 UTC / en-US）。
  //   语料 94 张卡**全部带时间戳**，渲染走 `src/frontend/ui/format.ts` 的 `toLocaleTimeString([], …)`
  //   ⇒ 宿主出 `05:34`、容器出 `12:34 PM`，**字面与字数都不同** ⇒ 指纹全漂、真高也会变。
  //
  // 🔴 **把「环境不同」与「语料改了」分开报，是这一条存在的全部理由** ——
  //   两者下面那条的报错长得一模一样，而**处置完全相反**：
  //     · 语料真改了 ⇒ 重打金标准（`bash tests/evidence/U-scale2-run.sh`）
  //     · 环境不同   ⇒ **别重打**，把环境调对；重打会把「这台机器」再烤进去一次
  //   ⚠ 09-19 之前这两件事在输出面上分不开，于是那条红在沙箱里被读成「语料过期」。
  it("跑本秤这台机器的显示时区与 locale，与金标准那一趟一致", () => {
    const now = Intl.DateTimeFormat().resolvedOptions();
    const want = golden.env ?? {};
    expect(
      want.displayLocale,
      "金标准里没有 `env.displayLocale` 戳 —— 那份金标准是加这条判据之前打的，" +
        "它把打它那台机器的 locale 悄悄烤了进去。重打一次即可：" +
        "`bash tests/evidence/U-scale2-run.sh`",
    ).toBeTruthy();
    expect(
      { locale: now.locale, timeZone: now.timeZone },
      "★ **本秤在这台机器上判不了**（不是语料过期，别去重打金标准）。\n" +
        `  金标准那一趟：locale=${want.displayLocale} · TZ=${want.timeZone}\n` +
        `  这台机器现打：locale=${now.locale} · TZ=${now.timeZone}\n` +
        "  语料 94 张卡全部带时间戳，locale/TZ 一变，渲染出来的字面与字数都变 ⇒ 真高也变。\n" +
        "  这两个值由 `vitest.config.ts` 顶层钉死（`process.env.TZ` / `process.env.LANG`；Windows 的 ICU 不读 `LANG`，\n" +
        "  JS 那一层由它 `setupFiles` 里的 `tests/test-support/pin-locale.ts` 按同一个值钉）——\n" +
        "  对不上说明那几行被改了、或本文件不是经 vitest 跑的。",
    ).toEqual({ locale: want.displayLocale, timeZone: want.timeZone });
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
        "  两条路：① 估高真写坏了 ⇒ 去修 `src/frontend/ui/height-estimate.ts`；\n" +
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
        "        真高取 content-box：估值是 content-box 口径（padding 与 border 不算在内）。",
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
        "  人却以为修好了。地板已去掉，理由：\n" +
        "  全局地板构造上不可能对、分档等于把常数抄\n" +
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
        `[秤2] 24px 地板会顶起的卡：${rows.length}/${corpus.length} 张 —— 现在它们原样出货`,
        ...rows.map(
          (x) =>
            `  ${x.c.id}（${x.c.cls}）估值 ${x.raw!.toFixed(1)} → 写进 style 的是 ${appliedIntrinsicPx(x.raw!)}（旧地板下是 24）`,
        ),
        "  ⇒ 四个 class：card-api-retry(5) · card-bash-input(3) · card-slash(3) · card-user(2)。",
        "     其余五个 class 的估值最小值都 ≥ 38，够不着旧地板 ⇒ **一个点都不该动**（上面的 p90 表可核）。",
      ].join("\n"),
    );
    nonEmpty(rows); // 只产读数；为 0 说明语料里的细条卡没了，那是另一回事
  });
});

// ===== 骨架外框：第一级的 `SKEL_*` 有秤对拍 =====
// 要求：「第一级的外框常数（`height-estimate.ts` 的 `SKEL_*`）是估的、没有秤对拍」。
// 金样由真 Chromium 打（`tests/evidence/skel-run.ts`；读的是卡在流里占的位置 − 头 − 体），与本文件的秤 2 同一份语料。
// ⚠ 买不到：WebKitGTK 那一个引擎没打（秤 2 两个都打了）；卡间距按语料里的邻居折叠，真会话里邻居不同会差几 px。
describe("秤 2 · 骨架外框（RENDER2）", () => {
  it("★ 四格 `SKEL_*` == 真浏览器里「位置 − 体」的中位数向上取整（user · assistant 含头 · 几步卡的外距 · 系统细条）", async () => {
    const { SKEL_OUTER } = await import("../../../src/frontend/ui/height-estimate");
    const g = JSON.parse(readFileSync(resolve(__dirname, "../../evidence/skel-golden.json"), "utf8")) as {
      rows: Array<{ cls: string; slot: number; body: number }>;
    };
    const median = (xs: number[]): number => {
      const s = [...xs].sort((a, b) => a - b);
      const m = s.length >> 1;
      return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
    };
    const of = (cls: string): number => {
      const v = g.rows.filter((r) => r.cls === cls).map((r) => r.slot - r.body);
      nonEmpty(v);
      return Math.ceil(median(v));
    };
    expect({ ...SKEL_OUTER }).toEqual({
      user: of("card-user"),
      assistant: of("card-assistant"),
      steps: of("card-steps"),
      system: of("card-api-retry"),
    });
  });
});
