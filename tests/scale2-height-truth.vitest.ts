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
import { estimateStreamNodeHeight } from "../src/height-estimate";

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
  }[];
  rows: GoldenRow[];
  crossRows: { id: string; trueContentBox: number }[] | null;
}

const golden = JSON.parse(readFileSync(GOLDEN, "utf8")) as Golden;
const corpus = buildCorpus(readFileSync(FIXTURE, "utf8"));
const byId = new Map(golden.rows.map((r) => [r.id, r]));

/** `applyIntrinsicSize` 里那个 `Math.max(24, …)` 地板 */
const APPLIED_FLOOR = 24;
/** `styles.css` 的 `.stream-content > * { contain-intrinsic-size: auto 120px }` */
const CSS_FALLBACK_PX = 120;

/** 估不出高 → 不写 inline style → 浏览器用 CSS 的 120px 兜底。两条路都要按"浏览器实际用了哪个数"算。 */
function appliedEstimate(el: HTMLElement): number {
  const raw = estimateStreamNodeHeight(el);
  return raw === null
    ? CSS_FALLBACK_PX
    : Math.max(APPLIED_FLOOR, Math.round(raw));
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
  // ⚠ 下面每条注释里的实测值都是 **2026-09-18 换成"真形语料"之后**重打的
  //   （Chromium 153 / WebKitGTK 2.52.6，content-box 口径）。
  //   换语料前后 p90 几乎没动（`card-assistant` 96.8/105.9 → 97.6/104.1，
  //   `card-user` 10.7/14.3 → 10.7/14.3，`card-tool-group` 9.8/11.8 → 9.8/11.8）
  //   —— 这本身就是「同形替换不改排版」的一条实证：正文换成填充字符，估高与真高一起不动。
  // ── 达标段：设计门槛 ──
  "card-bash-output": 0.3, //  实测 p90 8.6% / 10.0%，max 15.2%
  "card-compact": 0.3, //      实测 p90 9.8% / 11.8%
  "card-tool-group": 0.3, //   实测 p90 9.8% / 11.8%
  "card-user": 0.3, //         实测 p90 10.7% / 14.3%
  // ── 超标段：逐条写明成因 ──
  // 常数 24 是**含 padding 的 border-box 值**（23.05 ≈ 3×2 + 11×1.55），
  // 而 `contain-intrinsic-size` 实测是 **content-box**（本文件 B 段那一格）
  // ⇒ 24 对上的真值是 17.05，差 6px = 恰好一份 padding。
  "card-api-retry": 0.45, //   实测 p90 40.8% / 41.2%
  // 同一个病：常数 32 vs content-box 真值 18.6（12px×1.55），差 12 = 一份 padding 6×2。
  "card-bash-input": 0.8, //   实测 p90 72.1% / 68.4%
  // 同一个病：常数 34 vs content-box 真值 18.6，差 12 + 2.4 常数本身偏大。
  "card-slash": 0.95, //       实测 p90 82.9% / 88.9%
  // 反向：常数 40 是**单行**错误卡的高度，可 `card-api-error` 有 `.api-error-body`
  // （`white-space: pre-wrap`）会多行 ⇒ 长报错整个虚低。`设计/17 §2.2` 早就点过这一条。
  "card-api-error": 0.9, //    实测 p90 70.7% / 69.8%（**虚低**方向）
  // 🔴 最值钱的一格：正文卡系统性 **~2× 虚高**。成因见 `U-scale2-height-truth.md` 的
  // 「`extractProseText` 把 markdown 产物里的排版换行当成硬断行」那一节。
  // ⚠ max 从旧语料的 134.0% / 141.9% 降到 **103.9% / 106.7%** —— 不是修好了，
  //   是旧语料里那条最极端的记录（住在活会话文件的后半段）不在新语料里了。
  //   **p90 没动**，所以结论没变；但"最坏能坏到多少"这一格，新语料盖得比旧语料浅。
  "card-assistant": 1.2, //    实测 p90 97.6% / 104.1%（**虚高**），max 103.9% / 106.7%
};

/** 今天**不满足**设计那句「p90 < 30%」的 class（只许变短；变长变短都要回来改这里）。 */
const EXCEEDS_DESIGN_GATE = [
  "card-api-error",
  "card-api-retry",
  "card-assistant",
  "card-bash-input",
  "card-slash",
];

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
        "        估值 = 浏览器实际用的那个数（`Math.max(24,…)` 之后；估不出高的按 CSS 兜底 120px）。",
        "        真高取 content-box，因为 B 段实测 `contain-intrinsic-size` 就是 content-box。",
        "  card class         n 语料 |  主路(pretext) p50/p90/max | 降级路(算术) p50/p90/max | 方向",
        ...rows,
      ].join("\n"),
    );
    expect(rows.length).toBeGreaterThanOrEqual(8);
  });

  it("读数：`Math.max(24, …)` 这块地板到底顶起了几张卡", () => {
    const floored = corpus
      .map((c) => ({ c, raw: estimateStreamNodeHeight(c.element) }))
      .filter((x) => x.raw !== null && Math.round(x.raw) < APPLIED_FLOOR);
    console.log(
      [
        `[秤2] \`applyIntrinsicSize\` 的 24px 地板：${floored.length}/${corpus.length} 张卡被它顶起来`,
        ...floored.map(
          (x) =>
            `  ${x.c.id}（${x.c.cls}）估值 ${x.raw!.toFixed(1)} → 写进 style 的是 24`,
        ),
        "  ⇒ 凡是常数本身 ≥24 的卡型（api-retry 24 / bash-input 32 / api-error 40 / slash 34），",
        "     地板**碰不到它们**：`Math.max(24, 24) === 24`。改常数与地板是两件事。",
      ].join("\n"),
    );
    expect(floored.length).toBeLessThan(corpus.length); // 只产读数
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
        "  ⇒ `card-api-retry` 写 24 ⇒ 视口外实际占 **30px**，而真高是 23.05px ⇒ **虚高 30%**，",
        "     不是 `真相源` 里按手算推的 4%。R 路那条「24 比 23.05 高 4%」的前提在这里被替换掉了。",
      ].join("\n"),
    );
    expect(golden.intrinsic.length).toBeGreaterThan(0);
  });
});

describe("秤 2 · 悬案③：一屏门控的三段边界，换上实测值之后还成不成立", () => {
  /**
   * `设计/17 §6` 那条订正里的三段边界表，是**带着「真高 = 手算 23.05px、估值 = 24px」
   * 这个前提**算出来的（R 路自陈）。秤 2 把两个数都量出来了，其中一个变了：
   *
   * | 量 | 手算前提 | 秤 2 实测 |
   * |---|---|---|
   * | 一张 `card-api-retry` 的真高（border-box） | 23.05px | **23.047px**（Chromium）/ 23px（WebKitGTK）✅ 手算是对的 |
   * | 它在视口外对 `scrollHeight` 的贡献 | 24px | **30px** ❌ 差一份 padding（B 段：CIS 是 content-box） |
   *
   * ⇒ 「估值比真高高 4%」这个前提被换成「**高 30%**」，三段边界必须重算。
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

  it("★ 悬案③：R 路那个「n=17 是洞、n=34 仍红」，换上实测值之后**洞变成了两条带**", () => {
    const retryRow = golden.rows.find((r) => r.cls === "card-api-retry");
    expect(retryRow, "语料里没有 card-api-retry —— 下面全是空话").toBeDefined();
    const trueH = retryRow!.trueBorderBox; // 实测真高（border-box）
    const cis = golden.intrinsic.find(
      (x) => x.cls === "card card-api-retry" && x.declared === 24,
    );
    expect(cis, "B 段没有 `auto 24px` 那一格").toBeDefined();
    const estH = cis!.measured; // 实测：视口外这张卡实际占多少 = 24 + padding

    const handCalc = redBand(24, 23.05); // R 路的前提
    const measured = redBand(estH, trueH); // 秤 2 的实测值

    expect(
      estH,
      "B 段测到的不是 30 —— 下面这段推导的前提就不成立了",
    ).toBeCloseTo(30, 1);
    // R 路的结论在它自己的前提下复现得上：红点 = 1..8 ∪ {17} ∪ {34}
    expect(handCalc.filter((n) => n > 8)).toEqual([17, 34]);
    // 换上实测值之后：17 还在红里，但它**不再是孤点**
    expect(measured).toContain(17);
    expect(measured.filter((n) => n > 11)).toEqual([
      14, 15, 16, 17, 27, 28, 29, 30, 31, 32, 33, 34,
    ]);
    // ⇒ 设计里那句「取 18–30 条/150 最稳」在实测值下**不成立**：27–30 是红的
    for (const n of [27, 28, 29, 30]) {
      expect(
        gateRed(n, estH, trueH),
        `n=${n} 按设计的建议密度应当是绿的，实测是红的`,
      ).toBe(true);
    }
    console.log(
      [
        "[秤2] 悬案③ · 一屏门控的三段边界，换上实测值之后",
        `  前提：clientHeight=${GATE.clientHeight} · round<${GATE.maxRounds} · 每轮 ${GATE.tailK} 条`,
        `  R 路的手算前提（估值 24 / 真高 23.05）：红点 = 1–8 ∪ {${handCalc.filter((n) => n > 8).join(", ")}}`,
        `  秤 2 实测（估值 ${estH} / 真高 ${trueH.toFixed(3)}）：红点 = 1–11 ∪ {${measured.filter((n) => n > 11).join(", ")}}`,
        "  ⇒ ① 「n=17 是洞」**仍然成立**，但它不再是孤点 —— 变成 [14,17] 一整条带；",
        "     ② 新出现一条 [27,34] 红带（估值虚高 30% ⇒ 第一轮就判「满了」，而 27×23 < 800）；",
        "     ③ 最低那一段从 n≤8 扩到 **n≤11**；",
        "     ④ 🔴 `设计/17 §6` 那句「取 **18–30** 条/150 最稳」**在实测值下是错的** —— 27–30 全红。",
        "        实测下整段都成立的密度是 **18–26**（或 12–13，或 ≥35）。",
        "  ⚠ 口径与 R 路逐字对齐：两边都**不算 margin**（`.card-api-retry` 的 4px 上下 margin",
        "     在估值侧和真高侧同时存在，算进去红带整体左移到 1–9 ∪ [12,14] ∪ [24,29]，",
        "     **四条结论的方向一个都不变**）。",
      ].join("\n"),
    );
  });
});
