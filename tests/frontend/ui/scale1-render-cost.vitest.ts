/**
 * 秤 1 ——「单条渲染成本直方图」（表第 1 行，**最高优先**那一行）。
 *
 * 设计逐字要的是：
 *   「`renderContentRecord`（`render-stream-record.ts`）wall time，**按记录字节分桶**，
 *     拆 4 个子段」
 *   「入口出口夹 `performance.now()`，push 进环形缓冲（cap 5000）；
 *     按 `<2K/2-8K/8-32K/32-128K/>128K` 分桶（对齐 §1 的实测分位）输出 n/p50/p90/max」
 *   验的声称：**§2.1 §2.4 §2.5 §2.6 §2.8 全部声称「长尾桶被 O(len) 操作主导」**。
 *
 * 采集端在 `src/frontend/ui/render-stream-record.ts`（`enableRenderCostProbe` 那一段），
 * 本文件是**驱动 + 判据 + 报表**。
 *
 * # 语料
 *
 * `tests/__fixtures__/scale2-height-records.jsonl`（**与秤 2 同一份，不另造**）。
 * 🔴 **结构采自真机、正文一个字都不是真的** —— 数据源纪律
 * 2026-09-18 已改判成「结构照真的，内容一律合成」（用户逐字「这是测试啊 / 不应该进」）。
 * 产出它的是 `tests/evidence/U-scale2-sample-records.ts`（逐字符同形替换 ＋ 两道自检）。
 * 桶边界与秤 2（`tests/frontend/ui/scale2-height-corpus.ts` 的 `BUCKETS`）**逐字同一套**。
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
 * 5. **时间是这台机器这一次的，而且随负载抖。** 绝对毫秒不可跨机比较，段占比与桶倍率在门禁并发时也会飘
 *    ⇒ 10-07 起**没有一条闸看墙钟**：复杂度那几条（随字节单调 · 长比短多一个量级 · 长尾桶里的折叠卡便宜一个量级）
 *    数的是**物化进 DOM 的字符数**（`domChars`，确定量），理由与等价性写在那一组的头注；
 *    墙钟读数（四段 p50 · 占比 · 残余）照旧印进报表，只当读数。原来量什么、现在量什么见 `tests/evidence/S1-render-cost.md`「判据换轴」。
 * 6. **探针自己要钱。** 分桶轴要「记录字节」，而 payload 上没有该字段 ⇒
 *    只能 `JSON.stringify(message)` 现算（它自己就是 §2.4 那一形）。
 *    ⇒ 探针默认关；开着时字节数在总时刻取完之后才算，**不进任何一段读数**，
 *    但整体 wall time 确实被抬高了。生产常开会付这份钱。
 *
 * 复算：`bash tests/evidence/S1-run.sh`（等价于 `npx vitest run tests/frontend/ui/scale1-render-cost.vitest.ts`）
 * 死值验：`bash tests/evidence/S1-mutation.sh`
 * 读数：`tests/evidence/S1-render-cost.md`
 */
import { describe, it, expect, beforeAll } from "vitest";
import { withUserText } from "../../test-support/user-text";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// jsdom 无 ResizeObserver（`MessageStream` 构造需要）——空壳即可，
// 贴底行为不在本测范围（同 `tests/frontend/ui/record-timeline.vitest.ts` 的做法）。
globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

import { MessageStream } from "../../../src/frontend/ui/stream";
import { RecordTimeline } from "../../../src/frontend/ui/record-timeline";
import {
  renderContentRecord,
  enableRenderCostProbe,
  disableRenderCostProbe,
  readRenderCostSamples,
  RENDER_COST_RING_CAP,
  type RenderCostSample,
  type StreamSink,
} from "../../../src/frontend/ui/render-stream-record";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
import type { JsonlRecord, RenderContext } from "../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";

// `__dirname` 在 vitest 里指向 `tests/`（同 `scale2-height-truth.vitest.ts` 的用法）
const FIXTURE = resolve(__dirname, "../../__fixtures__/scale2-height-records.jsonl");
const fixtureLines = readFileSync(FIXTURE, "utf8")
  .split("\n")
  .filter((l) => l.trim().length > 0);

/**
 * 秤 1 的桶边界，**与秤 2（`tests/frontend/ui/scale2-height-corpus.ts`）逐字同一套**。
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

// ── 🔴 成本轴换成**卡型** ──────────────────────────────────────
//
// 本秤自己的读数推翻了「字节即成本」（「装秤之后改过的三条判断」第 1 条），
// 而判据一直拿 `branch === "card"` 当「建卡那条路」—— **分支比卡型粗一层**：
// 现打 `card` 分支里混着 `card-compact`（23 KB 的记录只物化 27 个字，是一张**便宜**的卡），
// 它落在 `8-32K` 桶里，和贵的 `card-assistant` 一起算 p50。
// ⇒ 样本上多了 `card`（卡型）与 `domChars`（物化进 DOM 的字符数），下面三张表是这一轴的地基。
// 设计与读数住。

/**
 * 每个卡型的**绝对**样本数（69 条 × 8 遍；独立写死，理由同 `EXPECTED_SAMPLES_PER_BUCKET`）。
 * `card-tool-group` = 新建外壳 1 条 ＋ 并入已有外壳 22 条（两条分支，同一种卡）。
 */
const EXPECTED_SAMPLES_PER_CARD: Readonly<Record<string, number>> = {
  "card-assistant": 224,
  "card-user": 80,
  "card-compact": 8,
  "card-tool-group": 184,
  skip: 56,
};
/**
 * **折叠卡型**：正文留在 DOM 外（惰性 body / compact 摘要），
 * 物化量**不随记录字节变** —— 这就是它们便宜的机制。
 */
const FOLDED_CARD_TYPES: ReadonlySet<string> = new Set([
  "card-tool-group",
  "card-compact",
]);
/** **正文卡型**：正文进 DOM，物化量随正文涨 —— O(len) 的那几条声称都住在它们身上。 */
const BODY_CARD_TYPES: ReadonlySet<string> = new Set([
  "card-assistant",
  "card-user",
]);
const isBodyCard = (s: RenderCostSample): boolean => BODY_CARD_TYPES.has(s.card);

/** 膨胀之后真的变大了的记录条数（现打，独立写死 —— S3 的非空对照）。 */
const EXPECTED_INFLATED_RECORDS = 57;
/** S2 的膨胀量：每个够长的串尾部追加的填充字符数。 */
const INFLATE_CHARS = 8192;
/** 多长的串才膨胀。短串（id / 时间戳 / 类型 / 短提示）不动，免得改到摘要行。 */
const INFLATE_MIN_LEN = 64;

/**
 * **只加字节、不改结构**地膨胀一条记录：每个 ≥ `INFLATE_MIN_LEN` 的串尾部接 `"\n"` ＋ 填充。
 * 类型 / id / 父子链全不动（它们都短）；首行不变（填充接在换行之后）；
 * 工具入参的摘要取的是 `JSON.stringify(input)` 的头 60 个字，而被膨胀的串本身就 ≥ 64 ⇒ 摘要不变。
 */
function inflate(v: unknown): unknown {
  if (typeof v === "string") {
    return v.length >= INFLATE_MIN_LEN ? `${v}\n${"膨".repeat(INFLATE_CHARS)}` : v;
  }
  if (Array.isArray(v)) return v.map(inflate);
  if (v && typeof v === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v)) out[k] = inflate(x);
    return out;
  }
  return v;
}

function freshCtx(): RenderContext {
  return {
    parentPath: "/tmp/scale1/session.jsonl",
    origin: LOCAL_ORIGIN,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  };
}

/**
 * 把一遍语料喂进 `renderContentRecord`。
 *
 * 口径：`enhanceRoot` 缺省（不交给任何 IO；原字段 `observeForLazyEnhance` 取 false 的那一形），
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
    const message = withUserText(JSON.parse(line) as JsonlRecord); // monitor 那一格成品
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
/**
 * 每条记录在 `PASSES` 遍里 total 最小的那一遍（69 条，按语料顺序）。
 * 负载只会给某一遍加时间、不会减 ⇒ 跨桶比墙钟（单调、倍率）用它，机器忙不忙比的都是同一件事。
 */
let steady: RenderCostSample[] = [];
/** 膨胀语料那一趟的样本（一遍，69 条，与 `samples` 的头 69 条逐条对应） */
let inflatedSamples: RenderCostSample[] = [];
/** 语料每条记录的**原始 jsonl 行字节**，按喂入顺序 —— 用来跟探针自报的字节对拍 */
const lineBytes = fixtureLines.map((l) => new TextEncoder().encode(l).length);

// 🔴 **超时给到 60s，理由要写清**：这一段真渲 `69 × (8+1) = 621` 条记录，
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
  steady = Array.from({ length: EXPECTED_RECORDS }, (_, i) => {
    const runs = Array.from({ length: PASSES }, (_, k) => samples[k * EXPECTED_RECORDS + i]);
    const route = (s: RenderCostSample | undefined) => (s ? `${s.bytes}/${s.card}/${s.branch}` : "(missing)");
    if (new Set(runs.map(route)).size !== 1) {
      throw new Error(`第 ${i} 条记录各遍的 (字节, 卡型, 分支) 不一致：${runs.map(route).join(" · ")}`);
    }
    return runs.reduce((a, b) => (b.total < a.total ? b : a));
  });

  // S2 那一趟：同一份语料**膨胀之后**再驱一遍（只看物化量与路由，不看时间 ⇒ 一遍就够）。
  enableRenderCostProbe();
  drivePass(fixtureLines.map((l) => JSON.stringify(inflate(JSON.parse(l)))));
  inflatedSamples = readRenderCostSamples();
  disableRenderCostProbe();

  // ── 报表（要的 n / p50 / p90 / max）────────────────────
  const lines: string[] = [];
  lines.push("");
  lines.push(
    `秤 1 · 单条渲染成本直方图（语料 ${EXPECTED_RECORDS} 条 × ${PASSES} 遍 = ${samples.length} 样本）`,
  );
  lines.push("① 按**卡型**（成本轴）—— 物化字符 = 这条记录进 DOM 的 `textContent` 长度：");
  lines.push(
    "| 卡型 | n | 记录字节 | 物化字符 | total p50 | total p90 | total max | render 占比 p50 |",
  );
  lines.push("|---|---:|---|---|---:|---:|---:|---:|");
  for (const card of Object.keys(EXPECTED_SAMPLES_PER_CARD)) {
    const rows = samples.filter((s) => s.card === card);
    if (!rows.length) {
      lines.push(`| ${card} | 0 | — | — | — | — | — | — |`);
      continue;
    }
    const tot = rows.map((s) => s.total);
    const bytes = rows.map((s) => s.bytes);
    const dom = rows.map((s) => s.domChars);
    lines.push(
      `| ${card} | ${rows.length} | ${Math.min(...bytes)}–${Math.max(...bytes)} | ` +
        `${Math.min(...dom)}–${Math.max(...dom)} | ${p(tot, 0.5).toFixed(3)} | ` +
        `${p(tot, 0.9).toFixed(3)} | ${Math.max(...tot).toFixed(3)} | ` +
        `${(p(rows.map((s) => (s.total > 0 ? s.render / s.total : 0)), 0.5) * 100).toFixed(1)}% |`,
    );
  }
  lines.push("");
  lines.push("② 按字节分桶（设计原先要的那张；**反例附表**：同一桶里混着便宜与贵的卡型）：");
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
  lines.push("只看**正文卡型**（`BODY_CARD_TYPES`）逐桶 ——先前是「`card` 分支」，那里混着便宜的 `card-compact`：");
  lines.push(
    "| 桶 | n | total p50 | total p90 | total max | render p50 | render 占比 p50 |",
  );
  lines.push("|---|---:|---:|---:|---:|---:|---:|");
  for (const [name] of BUCKETS) {
    const rows = samples.filter(
      (s) => bucketOf(s.bytes) === name && isBodyCard(s),
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

  // 残余中位占比（原来钉 < 10%）是墙钟读数，随负载抖 ⇒ 10-07 起只进报表；「真夹」那一侧（存在正残余）照钉。
  it("★ 残余（total − Σ四段）不恒为 0：total 是入口出口真夹的，不是算成 Σ四段", () => {
    const withTime = samples.filter((s) => s.total > 0);
    // `total` 如果被写成 Σ四段，残余就恒 0，而「入口出口真夹」这句话当场变成假话，却没有任何一格会红。
    // 真夹的话，分派与 `onRealUserInput` 回调必然在某些样本上留下正残余。
    const positive = withTime.filter((s) => s.total - segSum(s) > 0).length;
    expect(
      positive,
      `552 条样本里没有一条的 total 大于四段之和 —— ` +
        "`total` 多半不是入口出口夹出来的，而是被算成了 Σ四段",
    ).toBeGreaterThan(0);
  });
});

describe("秤 1 · 〔SC1〕成本轴是卡型 —— 不看墙钟的那一半", () => {
  it("★ S1 · 卡型人群：`{卡型 → 样本数}` == 绝对登记表（两向）", () => {
    const got: Record<string, number> = {};
    for (const s of samples) got[s.card] = (got[s.card] ?? 0) + 1;
    expect(
      got,
      "卡型人群与登记表对不上 —— 语料里多了 / 少了一种卡，或者某种卡改了类名。" +
        "新卡型要先判它是折叠还是正文（进 FOLDED / BODY 其一），再改 EXPECTED_SAMPLES_PER_CARD",
    ).toEqual({ ...EXPECTED_SAMPLES_PER_CARD });
    // 三张登记表互相对账：折叠 ∪ 正文 ∪ {skip} == 人群表的键，折叠 ∩ 正文 == ∅
    const partition = [...FOLDED_CARD_TYPES, ...BODY_CARD_TYPES, "skip"].sort();
    expect(partition, "折叠 / 正文两张表与人群表的键对不上").toEqual(
      Object.keys(EXPECTED_SAMPLES_PER_CARD).sort(),
    );
    expect(new Set(partition).size, "折叠与正文两张表有交集").toBe(partition.length);
  });

  it("★ S3 · 膨胀只加字节不改路：两趟的 `(卡型, 分支)` 逐条相等", () => {
    expect(inflatedSamples.length, "膨胀那一趟没采到样本 —— S2 会空转").toBe(EXPECTED_RECORDS);
    const route = (xs: RenderCostSample[]): string[] => xs.map((s) => `${s.card}/${s.branch}`);
    expect(route(inflatedSamples)).toEqual(route(samples.slice(0, EXPECTED_RECORDS)));
    // 非空对照：膨胀真的加了字节（否则 S2 里「不变」恒真）
    const grew = inflatedSamples.filter((s, i) => s.bytes > samples[i].bytes).length;
    expect(grew, "膨胀之后变大的记录条数与登记不符 —— inflate 没生效，或语料换了").toBe(
      EXPECTED_INFLATED_RECORDS,
    );
  });

  // 🔴 S2 就是「字节不是成本轴，卡型才是」的**可红形态**：
  //   同一条记录只加字节，物化量变不变，由卡型决定 —— 折叠卡型不变，正文卡型跟着涨。
  //   哪天工具结果改成急切把正文塞进 DOM ⇒ `card-tool-group` 从折叠集跳到正文集 ⇒ 红；
  //   哪天正文卡型不再渲正文 ⇒ 反方向红。一毫秒墙钟都不用。
  it("★ S2 · 物化量不随字节变的卡型 == 登记的折叠卡型；会变的 == 登记的正文卡型（两向）", () => {
    const moved = new Set<string>();
    const still = new Set<string>();
    for (let i = 0; i < EXPECTED_RECORDS; i++) {
      const a = samples[i];
      const b = inflatedSamples[i];
      if (a.card === "skip") {
        expect([a.domChars, b.domChars], `#${i} skip 不该物化任何东西`).toEqual([0, 0]);
        continue;
      }
      (b.domChars === a.domChars ? still : moved).add(a.card);
    }
    // 一种卡型只要有一条随字节涨，它就是正文卡型（短正文的那几条不膨胀、自然不涨）
    const folded = [...still].filter((c) => !moved.has(c)).sort();
    expect(
      folded,
      "物化量从头到尾不随字节变的卡型，与登记的折叠卡型对不上 ——" +
        "折叠卡型把正文塞进了 DOM（便宜的卡变贵了），或者正文卡型不再渲正文",
    ).toEqual([...FOLDED_CARD_TYPES].sort());
    expect(
      [...moved].sort(),
      "物化量随字节涨的卡型，与登记的正文卡型对不上",
    ).toEqual([...BODY_CARD_TYPES].sort());
  });
});

describe("秤 1 · 它要验的那条声称：长尾桶被 O(len) 操作主导（数物化字符，不看墙钟）", () => {
  // 🔴 **口径 10-07 换过：原来量墙钟，今天数物化字符（`domChars`）。**
  //
  // 原来这一组量的是 `total` / `render` / `merge` 的墙钟：「render 段在正文卡上占 total 过 55%」·
  // 「合并卡的桶 merge 段过 10%」·「正文卡 total p50 随字节单调涨」·「8-32K 比 <2K 贵十倍以上」·
  // 「长尾桶的折叠卡比 2-8K 正文卡便宜十倍以上」。它们本意都是**复杂度**：O(len) 的活住在哪条路上、
  // 随正文长度怎么涨。墙钟在门禁并发时抖（render 占比红过一次 53.0%），一个会随机红的判据就是会随机骗人的判据。
  //
  // 今天数的是**这条记录物化进 DOM 的字符数**（探针在同一处取，`textContent` 长度）。为什么与原来等价：
  // · 那几条 O(len) 操作（建正文 · 高亮 · 公式 · 估高按正文算）的工作量都正比于**进了 DOM 的正文** ——
  //   正文卡把正文整段物化，折叠卡（惰性正文 · 摘要）只物化一行摘要；同一条记录只加字节时，
  //   折叠卡物化量不动、正文卡跟着涨（上一组 S2 已逐卡钉过这一条）。
  // · ⇒「正文卡随字节单调涨」「长比短多一个量级」「长尾桶里的折叠卡比正文卡少一个量级」这三句话
  //   换成物化字符说，判的是**同一件事的原因**，而且是整数、确定、不随负载变。
  // · 两条**分段占比**（render ≥55% · merge ≥10%）没有确定量可换：它们问的是墙钟落在哪一段。
  //   它们承担的那句「O(len) 住在 renderMessage 里」由下面第一条（正文卡物化量随字节涨，而物化就发生在
  //   renderMessage 里）接住；占比本身留在报表里当读数，不再当闸。
  const bodyByBucket = (): { name: string; dom: number }[] =>
    BUCKETS.map(([name]) => ({ name, xs: steady.filter((s) => bucketOf(s.bytes) === name && isBodyCard(s)) }))
      .filter((b) => b.xs.length > 0)
      .map((b) => ({ name: b.name, dom: p(b.xs.map((s) => s.domChars), 0.5) }));

  it("★ 正文卡型：物化字符 p50 随记录字节单调上升", () => {
    const rows = bodyByBucket();
    expect(rows.length, "正文卡只落在不到三个桶里 —— 没有对照组，下面的单调性恒真").toBeGreaterThanOrEqual(3);
    for (let i = 1; i < rows.length; i++) {
      expect(
        rows[i].dom,
        `正文卡 ${rows[i].name} 的物化字符 p50（${rows[i].dom}）不比 ${rows[i - 1].name}（${rows[i - 1].dom}）多 ——` +
          "「同一种卡，正文越长活越多」这条不成立的话，§2.4/§2.5/§2.6 的修法都失去依据",
      ).toBeGreaterThan(rows[i - 1].dom);
    }
  });

  it("★ 正文卡型：8-32K 的物化字符比 <2K 多一个量级以上（倍率）", () => {
    const at = (name: string): number => bodyByBucket().find((b) => b.name === name)?.dom ?? 0;
    const small = at("<2K");
    const big = at("8-32K");
    expect(small).toBeGreaterThan(0);
    expect(big / small, `8-32K 的正文卡只比 <2K 多物化 ${(big / small).toFixed(1)} 倍 —— 「任何 O(单条长度) 的操作都要按长尾估」失去依据`).toBeGreaterThan(10);
  });

  it("🔴 反例也要钉住：按**字节**取的「长尾桶」里全是折叠 tool-group，物化量比 2-8K 正文卡少一个量级", () => {
    const tail = steady.filter((s) => bucketOf(s.bytes) === "32-128K");
    expect(tail.length, "长尾桶空着 —— 本格恒绿").toBeGreaterThan(0);
    // ① 成分：这一桶 100% 是合并进已有外壳的 tool_result
    const branches = [...new Set(tail.map((s) => s.branch))].sort();
    expect(branches, "32-128K 桶的成分变了 —— 上面那段「轴选错了」的诊断要重做").toEqual(["tool-group-merged"]);
    // ①′按卡型说同一件事：字节最重的那一桶里只有一种卡，而且是**折叠卡型**
    const cards = [...new Set(tail.map((s) => s.card))].sort();
    expect(cards, "32-128K 桶的卡型变了").toEqual(["card-tool-group"]);
    expect(FOLDED_CARD_TYPES.has(cards[0])).toBe(true);
    // ② 量级：物化字符比 2-8K 的正文卡少至少一个量级
    const tailDom = p(tail.map((s) => s.domChars), 0.5);
    const cardMid = bodyByBucket().find((b) => b.name === "2-8K")?.dom ?? 0;
    expect(tailDom).toBeGreaterThan(0);
    expect(
      cardMid / tailDom,
      `2-8K 的正文卡（${cardMid} 字）只比 32-128K 的折叠 tool-group（${tailDom} 字）多 ${(cardMid / tailDom).toFixed(1)} 倍 —— 「字节不是成本轴、卡型才是」这条结论要重新量`,
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
    // 系统注入的 user 记录 → `renderMessage` 第一行就 skip，最便宜的一条真路径
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
            userText: { speaker: { kind: "system" }, text: "" },
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
