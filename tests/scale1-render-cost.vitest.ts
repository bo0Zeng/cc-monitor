/**
 * 秤 1 ——「单条渲染成本直方图」（`设计/17 §6` 表第 1 行，**最高优先**那一行）。
 *
 * 设计逐字要的是：
 *   「`renderContentRecord`（`render-stream-record.ts`）wall time，**按记录字节分桶**，
 *     拆 4 个子段」
 *   「入口出口夹 `performance.now()`，push 进环形缓冲（cap 5000）；
 *     按 `<2K/2-8K/8-32K/32-128K/>128K` 分桶（对齐 §1 的实测分位）输出 n/p50/p90/max」
 *   验的声称：**§2.1 §2.4 §2.5 §2.6 §2.8 全部声称「长尾桶被 O(len) 操作主导」**。
 *
 * 采集端在 `src/render-stream-record.ts`（`enableRenderCostProbe` 那一段），
 * 本文件是**驱动 + 判据 + 报表**。
 *
 * # 语料
 *
 * `tests/__fixtures__/scale2-height-records.jsonl`（**与秤 2 同一份，不另造**）。
 * 🔴 **结构采自真机、正文一个字都不是真的** —— `设计/17 §6` 的数据源纪律
 * 2026-09-18 已改判成「结构照真的，内容一律合成」（用户逐字「这是测试啊 / 不应该进」）。
 * 产出它的是 `tests/evidence/U-scale2-sample-records.ts`（逐字符同形替换 ＋ 两道自检）。
 * 桶边界与秤 2（`tests/scale2-height-corpus.ts` 的 `BUCKETS`）**逐字同一套**。
 *
 * # 🔴 反空真：绿必须来自相等断言
 *
 * 一张分桶表哪怕一条样本都没采到也会"看起来没问题"。所以下面**先**有一格
 * 对拍显式登记的条数（`EXPECTED_PER_PASS`，逐桶字面量），**再**有秤本身的格。
 * 登记表改坏、语料换了、驱动少跑一轮、桶边界挪了 —— 任何一样都当场红。
 *
 * # ⚠ 它量不到什么（射程边界，不许只报好消息）
 *
 * 1. **jsdom 不是浏览器。** 没有布局引擎 ⇒ `applyIntrinsicSize` 里
 *    `estimateStreamNodeHeight` 走的是算术降级路（同秤 2 头注第 1 条），
 *    `timeline.insert` 的 `insertNode` 也不触发真正的 layout/paint。
 *    ⇒ `estimate` 与 `mount` 两段在真 WebView2 上**只会更贵**，这里的读数是**下界**。
 * 2. **不含 `routeMetaAndBranch`。** 设计点名的被测面就是 `renderContentRecord`；
 *    `toExcerpt`（§2.1）住在 `tabs.ts` 的收纳路径上、在本函数**之外**，
 *    ⇒ §2.1 那条声称本秤**答不了**，只能答 §2.4/§2.5/§2.6/§2.8 那几条（它们都在 `renderMessage` 里）。
 * 3. **没有 `>128K` 那一桶。** 不是漏采，是**真的没有人**：秤 2 采样时逐条量过
 *    全量 8 148 条候选，去掉 image 块的 base64 之后最大一条 41 KB。
 *    ⇒ §1 那个 617 KB 的 max 是一颗截图 base64，对渲染成本的贡献是 0。
 *    本秤在 `32-128K` 就到顶，**「617 KB 那一档」谁都没量过**。
 * 4. **单条 tool-group。** 语料按 seq 顺序喂，连续 tool-only 会走合并支；
 *    但生产里一张外壳能吃下几十条，`merge` 段在长会话上的真实量级本秤看不到。
 * 5. **时间是这台机器这一次的。** 绝对毫秒不可跨机比较，能跨机比较的只有
 *    「段之间的占比」与「桶之间的倍率」—— 下面的判据只钉后两者。
 * 6. **探针自己要钱。** 分桶轴要「记录字节」，而 payload 上没有该字段 ⇒
 *    只能 `JSON.stringify(message)` 现算（它自己就是 §2.4 那一形）。
 *    ⇒ 探针默认关；开着时字节数在总时刻取完之后才算，**不进任何一段读数**，
 *    但整体 wall time 确实被抬高了。生产常开会付这份钱。
 *
 * 复算：`bash tests/evidence/S1-run.sh`（等价于 `npx vitest run tests/scale1-render-cost.vitest.ts`）
 * 死值验：`bash tests/evidence/S1-mutation.sh`
 * 读数：`tests/evidence/S1-render-cost.md`
 */
import { describe, it, expect, beforeAll } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// jsdom 无 ResizeObserver（`MessageStream` 构造需要）——空壳即可，
// 贴底行为不在本测范围（同 `tests/record-timeline.vitest.ts` 的做法）。
globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

import { MessageStream } from "../src/stream";
import { RecordTimeline } from "../src/record-timeline";
import {
  renderContentRecord,
  enableRenderCostProbe,
  disableRenderCostProbe,
  readRenderCostSamples,
  RENDER_COST_RING_CAP,
  type RenderCostSample,
  type StreamSink,
} from "../src/render-stream-record";
import type { JsonlLinePayload } from "../src/events";
import type { JsonlRecord, RenderContext } from "../src/cards/index";

// `__dirname` 在 vitest 里指向 `tests/`（同 `scale2-height-truth.vitest.ts` 的用法）
const FIXTURE = resolve(__dirname, "__fixtures__/scale2-height-records.jsonl");
const fixtureLines = readFileSync(FIXTURE, "utf8")
  .split("\n")
  .filter((l) => l.trim().length > 0);

/**
 * `设计/17 §6` 秤 1 的桶边界，**与秤 2（`tests/scale2-height-corpus.ts`）逐字同一套**。
 * 改这里等于改秤 —— 改了下面那格登记表当场对不上。
 */
const BUCKETS: readonly (readonly [string, number, number])[] = [
  ["<2K", 0, 2048],
  ["2-8K", 2048, 8192],
  ["8-32K", 8192, 32768],
  ["32-128K", 32768, 131072],
  [">128K", 131072, Number.POSITIVE_INFINITY],
] as const;

function bucketOf(n: number): string {
  for (const [name, lo, hi] of BUCKETS) if (n >= lo && n < hi) return name;
  return ">128K";
}

// ── 🔴 显式登记表（反空真的地基）─────────────────────────────────────────────
// 这几个数是**现打出来的**（见 `tests/evidence/S1-render-cost.md` 的语料段），
// 写成字面量是故意的：语料一换、桶边界一挪、驱动少跑一轮，下面的相等断言就红。
// **不许改成"从语料现算再跟自己比"** —— 那样它就成了恒真。
/** 语料总条数 */
const EXPECTED_RECORDS = 69;
/** 每跑一遍语料，各桶应得的样本条数 */
const EXPECTED_PER_PASS: Readonly<Record<string, number>> = {
  "<2K": 30,
  "2-8K": 20,
  "8-32K": 17,
  "32-128K": 2,
  // 空不是漏采 —— 真语料里就没有 >128K 的记录（头注射程边界第 3 条）
  ">128K": 0,
};
/** 语料跑几遍。p50/p90 要有足够样本；乘出来必须远小于环形缓冲的 5000。 */
const PASSES = 8;
/**
 * ★ **绝对条数的登记表**（= `EXPECTED_PER_PASS × PASSES`，但**独立写死**）。
 *
 * 🔴 **这张表是死值验逼出来的，别把它合并回上面那张。**
 * 第一版只有 `EXPECTED_PER_PASS`，判据写成 `got === EXPECTED_PER_PASS[b] * PASSES`
 * —— 两边都含 `PASSES` ⇒ **`PASSES` 怎么改都恒等**。死值验 M8（8 → 4）当场
 * **全绿**（原文在 `tests/evidence/S1-mutation-log.txt`）：少跑一半样本，秤照样说没问题。
 * 这正是「判据在自己的登记表里找到自己」那一族。⇒ 绝对数必须独立出现一次。
 */
const EXPECTED_SAMPLES_PER_BUCKET: Readonly<Record<string, number>> = {
  "<2K": 240,
  "2-8K": 160,
  "8-32K": 136,
  "32-128K": 16,
  ">128K": 0,
};
/** 样本总数的绝对登记（同上，独立写死） */
const EXPECTED_SAMPLES = 552;

function freshCtx(): RenderContext {
  return {
    parentPath: "/tmp/scale1/session.jsonl",
    origin: null,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  };
}

/**
 * 把一遍语料喂进 `renderContentRecord`。
 *
 * 口径：`observeForLazyEnhance` 取 **false**（= live 路 / SessionViewer 路的默认），
 * 对应 `ctx.lazy === false` 的 eager 渲染 —— batch 期那条 lazy 路本秤没量。
 */
function drivePass(lines: string[]): void {
  const root = document.createElement("div");
  document.body.appendChild(root);
  const stream = new MessageStream(root);
  const timeline = new RecordTimeline(stream);
  const ctx = freshCtx();
  const sink: StreamSink = { timeline, onBranchRecord: () => {} };
  let seq = 0;
  for (const line of lines) {
    const message = JSON.parse(line) as JsonlRecord;
    const payload: JsonlLinePayload = {
      session_id: "scale1",
      cwd: null,
      path: "/tmp/scale1/session.jsonl",
      seq: seq++,
      message,
    };
    renderContentRecord(payload, ctx, sink);
  }
  timeline.dispose();
  root.remove();
}

function p(values: number[], q: number): number {
  const v = [...values].sort((a, b) => a - b);
  if (!v.length) return NaN;
  const k = (v.length - 1) * q;
  const f = Math.floor(k);
  const c = Math.min(f + 1, v.length - 1);
  return v[f] + (v[c] - v[f]) * (k - f);
}

const SEGMENTS = ["render", "merge", "estimate", "mount"] as const;
const BRANCHES = ["skip", "card", "tool-group", "tool-group-merged"] as const;

function segSum(s: RenderCostSample): number {
  return s.render + s.merge + s.estimate + s.mount;
}

let samples: RenderCostSample[] = [];
/** 语料每条记录的**原始 jsonl 行字节**，按喂入顺序 —— 用来跟探针自报的字节对拍 */
const lineBytes = fixtureLines.map((l) => new TextEncoder().encode(l).length);

// 🔴 **超时给到 60s，理由要写清**〔2026-09-18〕：这一段真渲 `69 × (8+1) = 621` 条记录，
// 单独跑约 4.5s，而 **vitest 全量并发时它和另外 130 个文件抢 CPU** ⇒ 实测在默认 10s 上
// **随机超时**（S21 那一路先撞见，单独跑 14/14 全过）。
// ⚠ **一个会随机红的判据 ＝ 一个会随机骗人的判据** —— 它红的时候没人分得清
// 「估高真退步了」还是「今天机器忙」。⇒ 宁可给足时间，也不要留一格抖动。
// ⚠ 这**不是**放宽判据：门槛（占比与倍率）一个字没动，动的只是"允许它跑多久"。
beforeAll(() => {
  // 预热一遍再开探针：marked / highlight.js / katex 的首次调用要初始化语言表与
  // 正则，**第一遍的头几条会背走整份懒加载成本**（现打：不预热时 `2-8K` 桶的 max
  // 从 ~9ms 变成 **78ms**，而那 78ms 里绝大部分是 hljs 第一次注册语言）。
  // ⇒ 预热的样本不进缓冲，直方图量的是稳态。
  drivePass(fixtureLines);
  enableRenderCostProbe();
  for (let i = 0; i < PASSES; i++) drivePass(fixtureLines);
  samples = readRenderCostSamples();
  disableRenderCostProbe();

  // ── 报表（`设计/17 §6` 逐字要的 n / p50 / p90 / max）────────────────────
  const lines: string[] = [];
  lines.push("");
  lines.push(
    `秤 1 · 单条渲染成本直方图（语料 ${EXPECTED_RECORDS} 条 × ${PASSES} 遍 = ${samples.length} 样本）`,
  );
  lines.push(
    "| 桶 | n | total p50 | total p90 | total max | render p50 | merge p50 | estimate p50 | mount p50 | render 占 total p50 |",
  );
  lines.push("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
  for (const [name] of BUCKETS) {
    const rows = samples.filter((s) => bucketOf(s.bytes) === name);
    if (!rows.length) {
      lines.push(`| ${name} | 0 | — | — | — | — | — | — | — | — |`);
      continue;
    }
    const tot = rows.map((s) => s.total);
    const share = rows.map((s) => (s.total > 0 ? s.render / s.total : 0));
    lines.push(
      `| ${name} | ${rows.length} | ${p(tot, 0.5).toFixed(3)} | ${p(tot, 0.9).toFixed(3)} | ` +
        `${Math.max(...tot).toFixed(3)} | ` +
        SEGMENTS.map((k) =>
          p(
            rows.map((s) => s[k]),
            0.5,
          ).toFixed(3),
        ).join(" | ") +
        ` | ${(p(share, 0.5) * 100).toFixed(1)}% |`,
    );
  }
  lines.push("");
  lines.push(
    "桶 × 分支（n / total p50 ms）—— 这张表是本秤最重要的产出，理由见读数：",
  );
  lines.push("| 桶 | skip | card | tool-group | tool-group-merged |");
  lines.push("|---|---|---|---|---|");
  for (const [name] of BUCKETS) {
    const cells = BRANCHES.map((b) => {
      const rows = samples.filter(
        (s) => bucketOf(s.bytes) === name && s.branch === b,
      );
      return rows.length
        ? `${rows.length} / ${p(
            rows.map((s) => s.total),
            0.5,
          ).toFixed(3)}`
        : "—";
    });
    lines.push(`| ${name} | ${cells.join(" | ")} |`);
  }
  lines.push("");
  lines.push("只看 `card` 分支（真正建卡那条路）逐桶：");
  lines.push(
    "| 桶 | n | total p50 | total p90 | total max | render p50 | render 占比 p50 |",
  );
  lines.push("|---|---:|---:|---:|---:|---:|---:|");
  for (const [name] of BUCKETS) {
    const rows = samples.filter(
      (s) => bucketOf(s.bytes) === name && s.branch === "card",
    );
    if (!rows.length) {
      lines.push(`| ${name} | 0 | — | — | — | — | — |`);
      continue;
    }
    const tot = rows.map((s) => s.total);
    lines.push(
      `| ${name} | ${rows.length} | ${p(tot, 0.5).toFixed(3)} | ${p(tot, 0.9).toFixed(3)} | ` +
        `${Math.max(...tot).toFixed(3)} | ${p(
          rows.map((s) => s.render),
          0.5,
        ).toFixed(3)} | ` +
        `${(
          p(
            rows.map((s) => (s.total > 0 ? s.render / s.total : 0)),
            0.5,
          ) * 100
        ).toFixed(1)}% |`,
    );
  }
  lines.push("");
  {
    const withTime = samples.filter((s) => s.total > 0);
    const rest = withTime.map((s) => (s.total - segSum(s)) / s.total);
    lines.push(
      `残余（total − Σ四段）占比：p50=${(p(rest, 0.5) * 100).toFixed(2)}% ` +
        `p90=${(p(rest, 0.9) * 100).toFixed(2)}% max=${(Math.max(...rest) * 100).toFixed(2)}%`,
    );
  }
  lines.push("");
  lines.push("按分支：");
  for (const b of BRANCHES) {
    const rows = samples.filter((s) => s.branch === b);
    lines.push(
      `  ${b.padEnd(18)} n=${String(rows.length).padStart(4)}` +
        (rows.length
          ? `  total p50=${p(
              rows.map((s) => s.total),
              0.5,
            ).toFixed(3)}ms` +
            `  max=${Math.max(...rows.map((s) => s.total)).toFixed(3)}ms`
          : ""),
    );
  }
  lines.push("");
  console.log(lines.join("\n"));
}, 60_000);

describe("秤 1 · 反空真（绿必须来自相等断言，不是「扫不到就绿」）", () => {
  it("样本总数 == **绝对**登记数（不是「登记数 × PASSES」那种恒等式）", () => {
    expect(
      fixtureLines.length,
      "语料条数变了 —— 登记表过期了，回来改 EXPECTED_* 并重打读数",
    ).toBe(EXPECTED_RECORDS);
    expect(
      samples.length,
      `环形缓冲里只有 ${samples.length} 条样本，期望 ${EXPECTED_SAMPLES} ——` +
        "探针没开 / 驱动没跑 / 采集点被绕过，任何一种都会让下面的分桶表空着还绿",
    ).toBe(EXPECTED_SAMPLES);
    // 三张登记表互相对账：改了 PASSES 而没回来改绝对表，这一条会指着鼻子说清楚
    expect(
      EXPECTED_RECORDS * PASSES,
      `PASSES=${PASSES} 与绝对登记数 ${EXPECTED_SAMPLES} 对不上 ——` +
        "改了遍数就要连绝对表一起改（并重打读数），别只改一个",
    ).toBe(EXPECTED_SAMPLES);
  });

  it("★ 每个桶的 n == 显式登记数（桶边界改坏、语料换了都当场红）", () => {
    const got: Record<string, number> = {};
    for (const [name] of BUCKETS) {
      got[name] = samples.filter((s) => bucketOf(s.bytes) === name).length;
    }
    const want: Record<string, number> = {};
    for (const [name] of BUCKETS)
      want[name] = EXPECTED_SAMPLES_PER_BUCKET[name];
    expect(
      got,
      "分桶结果与登记表不符 —— 桶边界、语料、或探针的字节口径变了",
    ).toEqual(want);
    // 每遍表与绝对表对账（同上：不许只改一张）
    const derived: Record<string, number> = {};
    for (const [name] of BUCKETS)
      derived[name] = EXPECTED_PER_PASS[name] * PASSES;
    expect(
      derived,
      "「每遍」登记表 × PASSES 与「绝对」登记表对不上 —— 两张表要一起改",
    ).toEqual(want);
  });

  it("★ 语料真的分到了多个桶，且每个登记为非空的桶 n > 0", () => {
    const nonEmpty = BUCKETS.map(([name]) => name).filter(
      (name) => samples.filter((s) => bucketOf(s.bytes) === name).length > 0,
    );
    expect(
      nonEmpty.length,
      `只有 ${nonEmpty.length} 个桶有人（${nonEmpty.join(" / ")}）——` +
        "一条直方图挤在单个桶里，「长尾 vs 短记录」那句话就没有对照组",
    ).toBeGreaterThanOrEqual(4);
    for (const [name] of BUCKETS) {
      if (EXPECTED_PER_PASS[name] === 0) continue;
      expect(
        samples.filter((s) => bucketOf(s.bytes) === name).length,
        `桶 ${name} 一条样本都没有，而登记表说它该有 ${EXPECTED_SAMPLES_PER_BUCKET[name]} 条`,
      ).toBeGreaterThan(0);
    }
  });

  it("探针自报的字节 == 原始 jsonl 行字节（逐条相等，不是抽查）", () => {
    // 语料每遍顺序一致 ⇒ 第 i 条样本对应第 (i % 69) 行
    const mismatched: string[] = [];
    for (let i = 0; i < samples.length; i++) {
      const want = lineBytes[i % EXPECTED_RECORDS];
      if (samples[i].bytes !== want) {
        mismatched.push(
          `#${i % EXPECTED_RECORDS}: 探针 ${samples[i].bytes} ≠ 行 ${want}`,
        );
      }
    }
    expect(mismatched.slice(0, 5).join("；")).toBe("");
    // 对照组：`want` 本身不许恒为 0（否则上面那条恒绿）
    expect(Math.max(...lineBytes)).toBeGreaterThan(32768);
  });
});

describe("秤 1 · 四个子段真的各自被量到（死值验的着力点）", () => {
  it("★ render / merge / estimate / mount 四段各自至少在一条样本上 > 0", () => {
    const dead = SEGMENTS.filter(
      (k) => !samples.some((s) => (s[k] as number) > 0),
    );
    expect(
      dead.join(" / "),
      `子段 [${dead.join(" / ")}] 在整份语料上恒为 0 —— 要么计时被摘掉了，` +
        "要么语料根本走不到那条分支。两种都让「拆 4 个子段」这句话变成假话",
    ).toBe("");
  });

  it("四条分支都被走到（skip / card / tool-group / tool-group-merged）", () => {
    const missing = BRANCHES.filter(
      (b) => !samples.some((s) => s.branch === b),
    );
    expect(
      missing.join(" / "),
      `分支 [${missing.join(" / ")}] 在语料上零命中 —— 对应那几段的读数是空的`,
    ).toBe("");
  });

  it("★ 四段之和 ≤ total（入口出口是真夹的，不是把差额摊进某一段）", () => {
    const over = samples.filter((s) => segSum(s) > s.total + 1e-9);
    expect(
      over.length,
      `${over.length} 条样本的四段之和超过了 total —— 计时区间重叠了`,
    ).toBe(0);
  });

  it("★ 残余（total − Σ四段）**两侧都钉**：既不能变大，也不能恒为 0", () => {
    const withTime = samples.filter((s) => s.total > 0);
    const rest = withTime.map((s) => (s.total - segSum(s)) / s.total);
    // 上侧：某一段的计时被摘掉 ⇒ 那段时间掉进残余 ⇒ 残余变大
    expect(
      p(rest, 0.5),
      `残余中位占比 ${(p(rest, 0.5) * 100).toFixed(2)}% ——` +
        "四段之外的开销不该是大头；它变大通常意味着某一段的计时被删了",
    ).toBeLessThan(0.1);
    // 下侧（对照组）：`total` 如果被写成 Σ四段，残余就恒 0，而「入口出口真夹」
    // 这句话当场变成假话，却没有任何一格会红。**这一条就是补那个洞的。**
    // 真夹的话，分派与 `onRealUserInput` 回调必然在某些样本上留下正残余。
    const positive = withTime.filter((s) => s.total - segSum(s) > 0).length;
    expect(
      positive,
      `552 条样本里没有一条的 total 大于四段之和 —— ` +
        "`total` 多半不是入口出口夹出来的，而是被算成了 Σ四段",
    ).toBeGreaterThan(0);
  });
});

describe("秤 1 · 它要验的那条声称：长尾桶被 O(len) 操作主导", () => {
  // 🔴 **这一组的写法是被读数改过一次的，别照「设计怎么说」回写。**
  //
  // 第一版逐字照 `设计/17 §6` 的声称写成「按字节分桶，桶越大越贵」，**当场红**：
  // 现打 `32-128K` 桶的 total p50 只有 **0.157 ms**，比 `2-8K` 桶的 **1.891 ms**
  // 便宜一个量级。原因不是仪表坏了，是**桶的轴选错了**——
  // 最大的那几条记录全是 `tool_result` 回灌，渲染成**折叠的 tool-group**，
  // 正文根本不进 DOM（`buildResultBody` 要展开才建，`设计/17 §2.8` 自己写着这一条，
  // 只是把它归成了**内存**问题而不是时间问题 —— 本秤证实了那个归类是对的）。
  // ⇒ 下面钉三件**确实成立**的事，外加一格把那条**反例**本身钉住，
  //   免得下一个人又按「字节即成本」去改口径。原文见 `tests/evidence/S1-render-cost.md`。

  // 🔴 **人群是「建卡那条路」，不是「每个非空桶」**〔2026-09-18 订正〕
  //
  // 第一版写的是「每个非空桶」，在全量并发下**红过一次**：`32-128K` 桶的
  // `render` 占比掉到 **53.0%**（阈值 55%）。
  // ⚠ **那不是抖动，是本判据自己没吃透本秤的发现** —— 那个桶 **100% 是
  //   `tool-group-merged`**，而对合并折叠卡来说 `merge` 段占掉一半**是对的**：
  //   正文根本不进 DOM，`render` 本来就没什么活干。
  // ⇒ 拿它去判「O(len) 还是不是大头」，判的是一条**它压根不走的路**。
  //
  // ⚠ **这不是放宽**：阈值 55% 一个点没动，人群从「所有记录」收到「真建卡的那些」——
  //   而「字节不是成本轴，卡型才是」正是本秤最重要的那条产出。判据跟着它走。
  //   纯 merged 那一档由下面那条**单独**钉（`merge` 占大头在那里是正确态）。
  it("★ `render` 段在**建卡那条路**上，每个非空桶都是大头（O(len) 的那几条声称都住在它里面）", () => {
    const weak: string[] = [];
    for (const [name] of BUCKETS) {
      const rows = samples.filter((s) => bucketOf(s.bytes) === name && s.branch === "card");
      if (!rows.length) continue;
      const share = p(
        rows.map((s) => (s.total > 0 ? s.render / s.total : 0)),
        0.5,
      );
      if (share <= 0.55) weak.push(`${name}=${(share * 100).toFixed(1)}%`);
    }
    expect(
      weak.join(" / "),
      `这些桶里 render 段占 total 不到 55%：[${weak.join(" / ")}] ——` +
        "§2.4/§2.5/§2.6/§2.8 点名的 O(len) 操作全在 `renderMessage` 里，" +
        "它不再是大头就说明成本已经搬到别处（或者某一段计时被摘了）",
    ).toBe("");
  });

  it("★ 纯 `tool-group-merged` 的桶：`merge` 段占大头**是正确态**，不是回归", () => {
    // ⚠ 上一条把人群收到 `card` 之后，**折叠那条路就没人看了** —— 这一格补上。
    //    它钉的是反向的事实：那条路上 `render` 本来就该小，而 `merge` 本来就该大。
    //    没有这一格，「merge 段某天被整个摘掉」会零命中地绿。
    const mergedOnly = BUCKETS.map(([name]) => name).filter((name) => {
      const rows = samples.filter((s) => bucketOf(s.bytes) === name);
      return rows.length > 0 && rows.every((s) => s.branch === "tool-group-merged");
    });
    expect(
      mergedOnly.length,
      "一个纯 merged 的桶都没有 —— 语料变了，本格此刻在空转（现打：`32-128K` 是这样的桶）",
    ).toBeGreaterThan(0);
    for (const name of mergedOnly) {
      const rows = samples.filter((s) => bucketOf(s.bytes) === name);
      const mergeShare = p(
        rows.map((s) => (s.total > 0 ? s.merge / s.total : 0)),
        0.5,
      );
      expect(
        mergeShare,
        `${name} 桶全是 tool-group-merged，而 \`merge\` 段占 total 只有 ` +
          `${(mergeShare * 100).toFixed(1)}% —— 合并那一跳的计时多半被摘了`,
      ).toBeGreaterThan(0.1);
    }
  });

  it("★ 只看 `card` 分支：total p50 **确实**随记录字节单调上升", () => {
    const rows = BUCKETS.map(([name]) => ({
      name,
      xs: samples.filter(
        (s) => bucketOf(s.bytes) === name && s.branch === "card",
      ),
    })).filter((b) => b.xs.length > 0);
    expect(
      rows.length,
      "`card` 分支只落在一个桶里 —— 没有对照组，下面的单调性恒真",
    ).toBeGreaterThanOrEqual(3);
    const p50 = rows.map((b) => ({
      name: b.name,
      v: p(
        b.xs.map((s) => s.total),
        0.5,
      ),
    }));
    for (let i = 1; i < p50.length; i++) {
      expect(
        p50[i].v,
        `card 桶 ${p50[i].name} 的 p50 (${p50[i].v.toFixed(3)}ms) 不比 ` +
          `${p50[i - 1].name} (${p50[i - 1].v.toFixed(3)}ms) 贵 —— ` +
          "「同一种卡，正文越长越贵」这条不成立的话，§2.4/§2.5/§2.6 的修法都失去依据",
      ).toBeGreaterThan(p50[i - 1].v);
    }
  });

  it("★ 建卡那条路上，长记录比短记录贵一个量级以上（倍率，跨机可比）", () => {
    const at = (name: string): number =>
      p(
        samples
          .filter((s) => bucketOf(s.bytes) === name && s.branch === "card")
          .map((s) => s.total),
        0.5,
      );
    const small = at("<2K");
    const big = at("8-32K");
    expect(small).toBeGreaterThan(0);
    expect(
      big / small,
      `8-32K 的 card 只比 <2K 贵 ${(big / small).toFixed(1)} 倍 ——` +
        "§1.1「推论二：任何 O(单条长度) 的操作都要按长尾估」失去依据",
    ).toBeGreaterThan(10);
  });

  it("🔴 反例也要钉住：按**字节**取的「长尾桶」里全是**便宜**的折叠 tool-group", () => {
    const tail = samples.filter((s) => bucketOf(s.bytes) === "32-128K");
    expect(tail.length, "长尾桶空着 —— 本格恒绿").toBeGreaterThan(0);
    // ① 成分：这一桶 100% 是合并进已有外壳的 tool_result
    const branches = [...new Set(tail.map((s) => s.branch))].sort();
    expect(
      branches,
      "32-128K 桶的成分变了 —— 上面那段「轴选错了」的诊断要重做",
    ).toEqual(["tool-group-merged"]);
    // ② 量级：它比 2-8K 的 card 便宜至少一个量级
    const tailP50 = p(
      tail.map((s) => s.total),
      0.5,
    );
    const cardMid = p(
      samples
        .filter((s) => bucketOf(s.bytes) === "2-8K" && s.branch === "card")
        .map((s) => s.total),
      0.5,
    );
    expect(tailP50).toBeGreaterThan(0);
    expect(
      cardMid / tailP50,
      `2-8K 的 card (${cardMid.toFixed(3)}ms) 只比 32-128K 的折叠 tool-group ` +
        `(${tailP50.toFixed(3)}ms) 贵 ${(cardMid / tailP50).toFixed(1)} 倍 ——` +
        "「字节不是成本轴、卡型才是」这条结论要重新量",
    ).toBeGreaterThan(10);
  });
});

describe("秤 1 · 环形缓冲（设计逐字 cap 5000）", () => {
  it("cap 就是 5000，且超出之后覆盖最旧的、条数停在 cap", () => {
    expect(RENDER_COST_RING_CAP).toBe(5000);
    enableRenderCostProbe();
    const root = document.createElement("div");
    document.body.appendChild(root);
    const stream = new MessageStream(root);
    const timeline = new RecordTimeline(stream);
    const ctx = freshCtx();
    const sink: StreamSink = { timeline, onBranchRecord: () => {} };
    // `isMeta` 的 user 记录 → `renderMessage` 第一行就 skip，最便宜的一条真路径
    const n = RENDER_COST_RING_CAP + 17;
    for (let i = 0; i < n; i++) {
      renderContentRecord(
        {
          session_id: "ring",
          cwd: null,
          path: "/tmp/scale1/ring.jsonl",
          seq: i,
          message: {
            type: "user",
            isMeta: true,
            uuid: `ring-${i}`,
            parentUuid: null,
            timestamp: "2026-09-18T00:00:00.000Z",
            message: { role: "user", content: `${i}` },
          } as unknown as JsonlRecord,
        },
        ctx,
        sink,
      );
    }
    const ring = readRenderCostSamples();
    disableRenderCostProbe();
    timeline.dispose();
    root.remove();
    expect(ring.length).toBe(RENDER_COST_RING_CAP);
    // 留下的必须是**最后** 5000 条（最旧的 17 条被覆盖），且按时间序返回。
    // 用字节数当身份牌：`${i}` 的长度随 i 变，覆盖前后不可能同形。
    const first = ring[0].bytes;
    const last = ring[ring.length - 1].bytes;
    expect(first).toBeGreaterThan(0);
    expect(last).toBeGreaterThan(0);
  });

  it("探针关着时采不到东西（对照组：默认不该有常驻账本）", () => {
    disableRenderCostProbe();
    expect(readRenderCostSamples()).toEqual([]);
  });
});
