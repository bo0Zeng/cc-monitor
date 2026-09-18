/**
 * 秤 2（`设计/17 §6` 表第 2 行）的**语料构造器** —— 真浏览器那一侧与 jsdom 门禁那一侧
 * **共用同一份**，否则"估值"与"真值"就不是同一张卡的两个读数，整杆秤失去意义。
 *
 * 它做的事只有一件：`JSONL 记录 → 真实卡片 DOM`，走的是生产的总分发器
 * `renderMessage(rec, ctx)`（`src/cards/index.ts`），不是手搭 DOM。
 *
 * # 语料从哪来
 *
 * - **真形语料**（`source: "shaped"`）：`tests/__fixtures__/scale2-height-records.jsonl`，
 *   由 `tests/evidence/U-scale2-sample-records.ts` 产出。
 *   🔴 **结构采自真机 `~/.claude/projects`，正文一个字都不是真的。**
 *   逐字符同形替换：字符数 / 行数 / 每行长度 / 显示宽度 / 断词位置 / markdown 结构 /
 *   块的种类与数量 / CJK:ASCII 比例**逐位相同**，而字母、数字、汉字全部换成
 *   只跟位置有关的填充字符。⇒ 排版量到的是真机的形状，仓里存的不是用户的对话。
 *   〔2026-09-18 改判，用户逐字「**这是测试啊 / 不应该进**」；
 *     `设计/17 §6` 的数据源纪律已同步改成「**结构照真的，内容一律合成**」。
 *     上一版逐字照旧纪律做，把 74 条真实会话记录（57 544 字符正文 + 本机绝对路径 +
 *     会话 id）写进了仓，且那 74 条全来自当时正在进行的那次会话。〕
 *   分桶仍逐字对齐 `设计/17 §6` 秤 1（`<2K/2-8K/8-32K/32-128K/>128K`），
 *   覆盖 `card-user` / `card-assistant` / `card-tool-group`。
 *   ⚠ **`>128K` 那一桶是空的，而且它本来就不该有人**：实测全量 8 148 条候选记录里，
 *   去掉 image 块的 base64 之后最大的一条只有 **41 KB**（p99 13.0 KB / p90 3.9 KB）。
 *   上一版报告里那条「631 KB 的 max 尾记录」**是一颗截图的 base64** ——
 *   而 `renderResultContent` 对 image 块只吐一行 `[image image/png]`，
 *   那颗 blob 对高度的贡献是 **0**。⇒ 旧报告的长尾覆盖是**虚的**，这一版把它改实了。
 * - **手工构造**：`card-api-retry` / `card-api-error` / `card-bash-input` /
 *   `card-bash-output` / `card-slash` / `card-compact`。
 *   ⚠ **不是偷懒，是没有真样本**：`设计/17 §6` 数据源纪律逐字「已查证：`evidence/` 里
 *   没有现成样本」，`真相源/90 §5.3` 又加强过一次；本轮采样器顺带复核：全部 8 个项目
 *   32 674 条记录里 `system.subtype == "api_error"` **0 条**、bash 模式 `<bash-input>`
 *   **0 条**、`<command-name>`（slash）只有 **2 条**。
 *   ⇒ 这几个卡型的读数**带着"构造体长得像不像真的"这个前提**，写报告时不许省略这句。
 *
 * # 口径（改这里等于改秤，改之前先想清楚）
 *
 * - 一条 `kind:"tool-group"` 记录单独包一张 `card-tool-group` 外壳（`buildToolGroup`）。
 *   生产里 TabManager 会把**连续多条**合并进同一张外壳；这里一条一张，
 *   ⇒ 本秤量到的 `card-tool-group` 是**单条那一档**，多条合并档没量。
 * - 折叠态：`<details>` 一律保持默认（关），与 `estimateStreamNodeHeight` 的
 *   `SUMMARY_H` 那一支对齐。
 */
import {
  renderMessage,
  buildToolGroup,
  addToToolGroup,
} from "../src/cards/index";
import type {
  JsonlRecord,
  RenderContext,
  ContentBlock,
} from "../src/cards/index";

export interface CorpusItem {
  /** 稳定 id：同一份语料在两侧必须产出同一串，否则对不上号 */
  id: string;
  /** 第一个 `card-*` 类名 */
  cls: string;
  /**
   * 语料来源：
   * - `shaped` —— **结构**采自真机、**正文全部合成**（见头注）。不叫 real：
   *   叫 real 会让读金标准的人以为仓里存着真对话，那正是这一轮要根除的误解。
   * - `synthetic` —— 连结构也是手搭的（真语料里 0 条的那几个卡型）。
   */
  source: "shaped" | "synthetic";
  /** 该条记录序列化后的字节数（真形语料才有意义） */
  bytes: number;
  /** `设计/17 §6` 秤 1 的字节桶 */
  bucket: string;
  element: HTMLElement;
}

const BUCKETS: [string, number, number][] = [
  ["<2K", 0, 2048],
  ["2-8K", 2048, 8192],
  ["8-32K", 8192, 32768],
  ["32-128K", 32768, 131072],
  [">128K", 131072, Number.POSITIVE_INFINITY],
];

function bucketOf(n: number): string {
  for (const [name, lo, hi] of BUCKETS) if (n >= lo && n < hi) return name;
  return ">128K";
}

function freshCtx(): RenderContext {
  return {
    parentPath: "/tmp/scale2/session.jsonl",
    origin: null,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  };
}

/**
 * 便宜的字符串指纹（FNV-1a 32bit）—— **只用来发现"语料变了"**，不做安全用途。
 * 探针侧与门禁侧必须用同一个，否则金标准过期哨兵恒绿。
 */
export function htmlFingerprint(s: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function cardClassOf(el: HTMLElement): string {
  return (
    Array.from(el.classList).find((c) => c.startsWith("card-")) ??
    `<${el.tagName.toLowerCase()}>`
  );
}

// ── 手工构造的那几条（真语料里 0 条，见头注）────────────────────────────────

function userText(text: string, timestamp: string, uuid: string): JsonlRecord {
  return {
    type: "user",
    uuid,
    parentUuid: null,
    timestamp,
    message: { role: "user", content: text },
  } as unknown as JsonlRecord;
}

/** `card-api-retry`：`system` + `subtype:"api_error"`（`cards/index.ts` 的 api_error 那一支） */
function apiRetry(i: number, formatted: string): JsonlRecord {
  return {
    type: "system",
    subtype: "api_error",
    uuid: `syn-retry-${i}`,
    parentUuid: null,
    timestamp: "2026-09-18T12:34:56.000Z",
    retryAttempt: (i % 5) + 1,
    maxRetries: 5,
    error: { formatted },
  } as unknown as JsonlRecord;
}

/** `card-api-error`：`assistant` + `isApiErrorMessage` */
function apiError(i: number, text: string): JsonlRecord {
  return {
    type: "assistant",
    uuid: `syn-apierr-${i}`,
    parentUuid: null,
    timestamp: "2026-09-18T12:34:56.000Z",
    isApiErrorMessage: true,
    error: "overloaded_error",
    apiErrorStatus: 529,
    message: {
      role: "assistant",
      content: [{ type: "text", text } as ContentBlock],
    },
  } as unknown as JsonlRecord;
}

const SYNTHETIC: { rec: JsonlRecord; note: string }[] = [
  // ── card-api-retry：细条，本轮三个悬案的主角 ──
  ...[
    "Connection error (ECONNRESET)",
    "Overloaded",
    "API Error: 500 Internal Server Error",
    "socket hang up",
  ].map((msg, i) => ({ rec: apiRetry(i, msg), note: `retry/${msg.length}字` })),
  // 超长文案：CSS 是 `white-space: nowrap; overflow: hidden`，所以**再长也只有一行** —— 这一条专门钉它
  {
    rec: apiRetry(9, "API Error: " + "x".repeat(400)),
    note: "retry/超长文案（nowrap 下仍应一行）",
  },
  // ── card-api-error：有 body、会多行的胖卡（`设计/17 §2.2` 点名说它"给了 40 ⇒ 对多行错误反而低估"）──
  { rec: apiError(0, "API Error: 529 Overloaded"), note: "api-error/单行" },
  {
    rec: apiError(
      1,
      'API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}',
    ),
    note: "api-error/两三行",
  },
  {
    rec: apiError(
      2,
      ("API Error: 400 " + "invalid request body field ".repeat(40)).trim(),
    ),
    note: "api-error/长正文多行",
  },
  // ── card-bash-input ──
  ...[
    "npm run build",
    "npm ci && npm run test:dom -- --reporter=verbose",
    "git log --oneline -20 | grep -i fix",
  ].map((cmd, i) => ({
    rec: userText(
      `<bash-input>${cmd}</bash-input>`,
      "2026-09-18T12:34:56.000Z",
      `syn-bashin-${i}`,
    ),
    note: `bash-input/${cmd.length}字`,
  })),
  // ── card-bash-output：空 / 短 / 刚好 20 行 / 超 30 行（截断 + 展开按钮）/ 带 stderr ──
  {
    rec: userText(
      "<bash-stdout></bash-stdout>",
      "2026-09-18T12:34:56.000Z",
      "syn-bashout-empty",
    ),
    note: "bash-output/空",
  },
  {
    rec: userText(
      `<bash-stdout>${Array.from({ length: 5 }, (_, i) => `line ${i}`).join("\n")}</bash-stdout>`,
      "2026-09-18T12:34:56.000Z",
      "syn-bashout-5",
    ),
    note: "bash-output/5 行",
  },
  {
    rec: userText(
      `<bash-stdout>${Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n")}</bash-stdout>`,
      "2026-09-18T12:34:56.000Z",
      "syn-bashout-20",
    ),
    note: "bash-output/20 行",
  },
  // ⚠ 这一条是**变异自检逼出来的**：没有它，`BASH_OUTPUT_MAX_LINES = 20` 这个常数
  // 在整份语料上**不可达** —— `buildOutputPre` 超 30 行就只留头 20 行，所以 DOM 里的
  // pre 要么 ≤20 行（min 取到行数本身）、要么正好 20 行（min 取到哪个都一样）。
  // 实测：把 20 改成 60，门禁**全绿**。25 行这一条落在 21–30 那个窗口里（DOM 不截、
  // 但 min(25,20) 会把它压到 20），常数这才真的被量到。
  {
    rec: userText(
      `<bash-stdout>${Array.from({ length: 25 }, (_, i) => `line ${i}`).join("\n")}</bash-stdout>`,
      "2026-09-18T12:34:56.000Z",
      "syn-bashout-25",
    ),
    note: "bash-output/25 行（DOM 不截，但 min(行数,20) 会压 ⇒ 让那个常数可达）",
  },
  {
    rec: userText(
      `<bash-stdout>${Array.from({ length: 80 }, (_, i) => `line ${i}`).join("\n")}</bash-stdout>`,
      "2026-09-18T12:34:56.000Z",
      "syn-bashout-80",
    ),
    note: "bash-output/80 行（超 30 ⇒ 截头 20 + 展开按钮）",
  },
  {
    rec: userText(
      "<bash-stdout>ok\nok2</bash-stdout><bash-stderr>warning: deprecated\nwarning: again</bash-stderr>",
      "2026-09-18T12:34:56.000Z",
      "syn-bashout-stderr",
    ),
    note: "bash-output/stdout+stderr",
  },
  // ── card-slash ──
  ...[
    ["/compact", ""],
    ["/full-audit", "全面理解这个项目"],
    ["/loop", "5m /babysit-prs --verbose --dry-run"],
  ].map(([name, args], i) => ({
    rec: userText(
      `<command-message>${name.slice(1)}</command-message><command-name>${name}</command-name><command-args>${args}</command-args>`,
      "2026-09-18T12:34:56.000Z",
      `syn-slash-${i}`,
    ),
    note: `slash/${name}`,
  })),
  // ── card-compact（折叠 <details>，估高走 SUMMARY_H 那一支）──
  {
    rec: userText(
      "This session is being continued from a previous conversation that ran out of context. " +
        "The conversation is summarized below:\n\n".concat(
          "摘要正文。".repeat(600),
        ),
      "2026-09-18T12:34:56.000Z",
      "syn-compact-0",
    ),
    note: "compact/折叠摘要",
  },
];

/**
 * 构造整份语料。`fixtureJsonl` 是 `tests/__fixtures__/scale2-height-records.jsonl`
 * 的全文（浏览器侧用 vite 的 `?raw`，jsdom 侧用 `readFileSync`）。
 */
export function buildCorpus(fixtureJsonl: string): CorpusItem[] {
  const items: CorpusItem[] = [];
  const ctx = freshCtx();
  let realIdx = 0;

  for (const line of fixtureJsonl.split("\n")) {
    if (!line.trim()) continue;
    let rec: JsonlRecord;
    try {
      rec = JSON.parse(line) as JsonlRecord;
    } catch {
      continue;
    }
    const bytes = new TextEncoder().encode(line).length;
    const res = renderMessage(rec, ctx);
    realIdx++;
    if (res.kind === "skip") continue;
    if (res.kind === "card") {
      items.push({
        id: `real#${realIdx}`,
        cls: cardClassOf(res.element),
        source: "shaped",
        bytes,
        bucket: bucketOf(bytes),
        element: res.element,
      });
      continue;
    }
    // tool-group：单条一张外壳（口径见头注）
    const group = buildToolGroup(res.timestamp);
    addToToolGroup(group, res.units);
    items.push({
      id: `real#${realIdx}`,
      cls: cardClassOf(group.root),
      source: "shaped",
      bytes,
      bucket: bucketOf(bytes),
      element: group.root,
    });
  }

  for (let i = 0; i < SYNTHETIC.length; i++) {
    const { rec, note } = SYNTHETIC[i];
    const res = renderMessage(rec, freshCtx());
    if (res.kind !== "card") {
      throw new Error(
        `构造体 #${i}（${note}）没产出卡片，而是 ${res.kind} —— 构造体写错了或分发器改了`,
      );
    }
    const bytes = new TextEncoder().encode(JSON.stringify(rec)).length;
    items.push({
      id: `syn#${i}`,
      cls: cardClassOf(res.element),
      source: "synthetic",
      bytes,
      bucket: bucketOf(bytes),
      element: res.element,
    });
  }

  return items;
}
