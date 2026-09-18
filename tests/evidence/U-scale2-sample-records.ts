#!/usr/bin/env node
/**
 * 秤 2（`设计/17 §6` 表第 2 行）的**语料采样器**。
 *
 * # 🔴 2026-09-18 改判：采结构，不采内容
 *
 * 上一版逐字照着 `设计/17 §6` 当时那条「**不要造合成数据。用真机
 * `~/.claude/projects` 下 p99 与 max 的那几条记录，固化进 fixtures**」做，
 * 结果把 **74 条真实会话记录（57 544 字符正文 + 本机绝对路径 + 会话 id）**
 * 写进了 `tests/__fixtures__/` —— 而那 74 条**全部来自当时正在进行的那次会话本身**。
 *
 * 用户裁决逐字：「**这是测试啊 / 不应该进**」。`设计/17 §6` 的那条纪律已随之改判，
 * 新纪律逐字是「**结构照真的，内容一律合成**」。本文件是那条新纪律的实现。
 *
 * ## 做法：逐字符**同形替换**，不是"按分布重新生成"
 *
 * 设计当初写「不要造合成数据」，担心的是**分布**——怕有人拿均匀长度的假数据
 * 去量长尾。同形替换比"照分布重采"更强地满足那个担心：
 *
 * | 量 | 保不保 | 怎么保的 |
 * |---|---|---|
 * | 字符数 / 行数 / 每行长度 | **逐位相同** | 一个字符换一个字符，不增不减 |
 * | 每个字符的**显示宽度** | **逐位相同** | 全角换全角（`一二三…`），半角换半角 |
 * | 断词位置（决定折行） | **逐位相同** | 空格与标点原样留下 |
 * | markdown 结构 | **逐位相同** | `#` `-` `|` `>` ``` `*` 都是标点，原样留下 |
 * | 块的种类与数量 | **逐位相同** | text/thinking/tool_use/tool_result 一条不改 |
 * | CJK : ASCII 比例 | **逐位相同** | 按字符类映射，类不跨界 |
 * | **正文语义** | **全毁** | 字母/数字/汉字一律换成**只跟位置有关**的填充字符 |
 *
 * ⇒ 估高只关心「多少字符折多少行」，**不关心那些字符是什么意思** ⇒ 真高与估值都不变形。
 *
 * ## 还有哪些东西被换掉了（不只是正文）
 *
 * - `cwd` / 任何绝对路径 → 路径**结构**留下，每一段的字母全换
 * - `sessionId` / `uuid` / `parentUuid` / `requestId` / `toolu_*` → 换成计数器 id，
 *   ⚠ **同一个原 id 恒映射到同一个新 id**，否则 `tool_result` 找不回它的 `tool_use`，
 *   语料的卡型分布会变（真语料里大多数 tool_result 本来就找不回去，那个"找不回"也要保住）
 * - `timestamp` → 从 2026-01-01T00:00 起每条 +1 分钟（卡头渲染 `HH:MM`，等宽，不影响高度）
 * - `gitBranch` / `slug` / `lastPrompt` / `aiTitle` / `snapshot` / `toolUseResult` …
 *   → **整个字段不落盘**：本文件走**白名单**，只写 `renderMessage` 真正读的那几个字段
 *
 * ## 有哪几样是**逐字保留**的，以及为什么（这段必须诚实，别省）
 *
 * 1. **CLI 协议标记**：`<bash-input>` `<command-name>` `<system-reminder>`
 *    `This session is being continued…` 等。换掉它们 ⇒ `stripInternalNoise` /
 *    `parseSlashCommand` / `isCompactSummary` 全部失配 ⇒ 卡型分布变 ⇒ 秤失去意义。
 *    它们是 Claude Code 的公开协议字面，不含用户内容。
 * 2. **工具名**（`Bash` / `Read` / `mcp__…`）：驱动 `isInteractiveTool` 与
 *    `defaultModeForTool`，换掉会改渲染路径。本文件会把保留下来的工具名**全部打印出来**，
 *    让"到底放了哪些字面过去"这件事看得见。
 * 3. **模型 id**（`claude-opus-5` 等）与 **代码块语言标签**（``` 后面那截，且必须在
 *    下面的白名单里，否则照样换）。两样都是公开词汇，且都进渲染宽度。
 * 4. **标点、空白、emoji**：它们是结构本身。emoji 还影响宽度。
 *
 * ⇒ **残留的可识别信息只有"词长序列 + 标点分布"**。折行必须靠词边界，
 *   所以词长没法抹；抹了这杆秤就废了。这是本文件刻意付的代价，写在这里不藏。
 *
 * ## 顺带解掉的另一个毛病：读数会飘
 *
 * 采样源里有**当前正在被写的那份 jsonl**，跑一次长一点，均匀取样挑出的记录就漂
 * （实测同一天两次跑：语料 86→87 张卡、`card-assistant` 的 p90 100.8%→96.9%，
 * 而被测代码一行没改）。
 *
 * 治法：**活文件只读它的前 `LIVE_PREFIX_LINES` 行**（追加写永远不改已写过的行
 * ⇒ 这个前缀是冻结的，跑多少次都一样），且该文件行数不够就整份跳过。
 *
 * ⚠ 第一版写的是「整份跳过活文件」，当场丢掉了 `>128K` 那一桶 ——
 * 而 `设计/17 §6` 点名要的就是 **p99 与 max**，长尾桶空了这杆秤最值钱的一格就没了。
 * **"跳过活文件"本来是为了稳定，不是为了隐私** —— 隐私已经由同形替换解掉了，
 * 所以正确的做法是冻前缀，不是扔掉。
 *
 * ## 其余保持不变的口径
 *
 * - 分桶逐字对齐 `设计/17 §6` 秤 1 那一行（`<2K/2-8K/8-32K/32-128K/>128K`）。
 *   ⚠ 按**落盘后**那行的字节数分桶（白名单砍掉了不少字段，按原行分桶会不诚实）。
 * - 只扫 `-home-zbl----claudecode-frontend/`（本项目自己的会话）。
 * - `card-api-retry` / `card-api-error` / `card-bash-input` / `card-bash-output`
 *   在真语料里 0 条（本脚本顺带普查过：8 个项目 32 674 条记录里
 *   `system.subtype == "api_error"` 0 条、`<bash-input>` 0 条）
 *   ⇒ 这四个卡型的构造体在 `tests/scale2-height-corpus.ts` 里，标着 `synthetic`。
 *
 * 复算：`npx tsx tests/evidence/U-scale2-sample-records.ts`
 *
 * ⚠ 为什么是 `.ts` 而不是 `.mjs`：`tests/eslint-baseline.vitest.ts` 的第②格要求
 * 每个被 lint 到的 `.mjs` 目录都要在 `eslint.config.js` 里有 `files:` 块认领，
 * 而 `eslint.config.js` 不在本轮写区里。**这不是绕过判据，是走它指的另一条合规路。**
 * 同理本文件不碰 `process` / `Buffer`（`tests/**` 那格只配了 browser globals）。
 */
import {
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir } from "node:os";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const SRC_DIR = join(
  homedir(),
  ".claude",
  "projects",
  "-home-zbl----claudecode-frontend",
);
const OUT = resolve(
  REPO_ROOT,
  "tests/__fixtures__/scale2-height-records.jsonl",
);
const SHAPE_OUT = resolve(
  REPO_ROOT,
  "tests/evidence/U-scale2-corpus-shape.json",
);

/** mtime 落在这么多分钟内的 jsonl 视为「正在被写」 */
const LIVE_WINDOW_MIN = 30;
/** 活文件只读这么多行（冻结前缀）；不够这么多行就整份跳过。见头注「读数会飘」。 */
const LIVE_PREFIX_LINES = 4000;

const byteLen = (s: string): number => new TextEncoder().encode(s).length;

// ════════════════════════════════════════════════════════════════════════
// 一、同形替换
// ════════════════════════════════════════════════════════════════════════

const LOWER = "abcdefghijklmnopqrstuvwxyz";
const UPPER = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGIT = "0123456789";
/** 全角填充字符。必须个个都是 CJK 统一表意文字 ⇒ 与被替换的字等宽。 */
const HAN = "一二三四五六七八九十";

/**
 * 必须逐字保留的 CLI 协议字面（理由见头注第 1 条）。
 * ⚠ 加一条进来就等于多放一串字面过闸 —— 加之前先问「它含不含用户内容」。
 */
const PROTOCOL_LITERALS = [
  "task-notification",
  "system-reminder",
  "local-command-caveat",
  "local-command-stdout",
  "command-name",
  "command-message",
  "command-args",
  "bash-input",
  "bash-stdout",
  "bash-stderr",
  "synthetic",
];
const PROTECT_RE = new RegExp(
  "</?(?:" +
    PROTOCOL_LITERALS.join("|") +
    ")>" +
    "|This session is being continued from a previous conversation" +
    "|[Cc]ontinue from where you left off" +
    "|[Nn]o response requested" +
    "|Request interrupted by user",
  "g",
);

/** 代码块语言标签白名单：只有在册的才逐字留下，其余照换（见头注第 3 条）。 */
const LANGS = new Set(
  (
    "ts tsx js jsx mjs cjs json jsonl bash sh shell zsh fish rust rs python py go java c cpp cc h hpp cs css scss less " +
    "html xml svg yaml yml toml ini sql diff patch md markdown text txt plaintext console makefile dockerfile " +
    "vue svelte kotlin kt swift ruby rb php lua r scala perl powershell ps1 tsv csv mermaid graphql proto " +
    "nginx apache vim awk sed regex http ebnf asm"
  ).split(" "),
);
const FENCE_RE = /^(\s*)(`{3,}|~{3,})([A-Za-z0-9+#._-]*)\s*$/;

function isWide(cp: number): boolean {
  return (
    (cp >= 0x3400 && cp <= 0x4dbf) || // CJK 扩展 A
    (cp >= 0x4e00 && cp <= 0x9fff) || // CJK 统一表意
    (cp >= 0xf900 && cp <= 0xfaff) || // 兼容表意
    (cp >= 0x3040 && cp <= 0x30ff) || // 平假名 / 片假名
    (cp >= 0xac00 && cp <= 0xd7af) //   谚文
  );
}

/** 填充位置计数器：跨整份语料连续，**只跟位置有关，与被换掉的字符无关**。 */
let fillPos = 0;

/** 逐字符同形替换。长度、宽度、标点、空白一律不动；字母/数字/表意文字全毁。 */
function transliterate(s: string): string {
  let out = "";
  for (const ch of s) {
    const cp = ch.codePointAt(0)!;
    fillPos++;
    if (cp >= 0x61 && cp <= 0x7a) out += LOWER[fillPos % 26];
    else if (cp >= 0x41 && cp <= 0x5a) out += UPPER[fillPos % 26];
    else if (cp >= 0x30 && cp <= 0x39) out += DIGIT[fillPos % 10];
    else if (isWide(cp)) out += HAN[fillPos % 10];
    else out += ch; // 标点 / 空白 / emoji / 全角标点：结构本身，原样留下
  }
  return out;
}

/** 在保护标记之外做同形替换。 */
function scrubSegment(s: string): string {
  let out = "";
  let last = 0;
  PROTECT_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = PROTECT_RE.exec(s)) !== null) {
    out += transliterate(s.slice(last, m.index));
    out += m[0];
    fillPos += m[0].length; // 保护段也推进计数器，保持"只跟位置有关"
    last = m.index + m[0].length;
  }
  return out + transliterate(s.slice(last));
}

/** 逐行处理：围栏行（``` + 白名单语言）整行留下，其余走 `scrubSegment`。 */
function scrubText(s: string): string {
  return s
    .split("\n")
    .map((line) => {
      const f = FENCE_RE.exec(line);
      if (f && (f[3] === "" || LANGS.has(f[3].toLowerCase()))) {
        fillPos += line.length;
        return line;
      }
      return scrubSegment(line);
    })
    .join("\n");
}

// ════════════════════════════════════════════════════════════════════════
// 二、id / 时间戳：恒映射，不是随机
// ════════════════════════════════════════════════════════════════════════

const idMap = new Map<string, string>();
function mapId(orig: unknown, prefix: "u" | "t"): string | null {
  if (typeof orig !== "string" || !orig) return null;
  const hit = idMap.get(orig);
  if (hit) return hit;
  const n = String(idMap.size + 1).padStart(12, "0");
  // uuid 形：只有十六进制位与固定的 4/8 版本位，没有任何字母词
  const made = prefix === "u" ? `00000000-0000-4000-8000-${n}` : `toolu_${n}`;
  idMap.set(orig, made);
  return made;
}

let tsSeq = 0;
function mapTimestamp(orig: unknown): string | undefined {
  if (typeof orig !== "string") return undefined;
  return new Date(
    Date.UTC(2026, 0, 1, 0, 0, 0) + tsSeq++ * 60_000,
  ).toISOString();
}

// ════════════════════════════════════════════════════════════════════════
// 三、白名单重建记录：只写 `renderMessage` 真正读的字段
// ════════════════════════════════════════════════════════════════════════

const keptToolNames = new Set<string>();
const keptModels = new Set<string>();

/**
 * `tool_result.content` 里的 image 块在监控视图里**从不展开** ——
 * `renderResultContent` 只吐一行 `[image image/png]`，那颗几 MB 的 base64
 * 对高度的贡献是 **0**。
 * ⇒ 只留 `media_type`（它决定那一行的宽度），`data` 整个丢掉。
 * 留着它既是把用户的截图原封不动搬进仓，也是白白往语料里压几 MB。
 */
function imageSurrogate(v: unknown): string | null {
  const o = v as { type?: unknown; source?: { media_type?: unknown } } | null;
  if (!o || typeof o !== "object" || o.type !== "image") return null;
  return `[image ${typeof o.source?.media_type === "string" ? o.source.media_type : "unknown"}]`;
}

/** `tool_use.input` / `tool_result.content`：键原样、字符串值同形替换、数字与布尔原样。 */
function scrubValue(v: unknown): unknown {
  if (typeof v === "string") return scrubText(v);
  if (imageSurrogate(v) !== null) {
    const src = (v as { source?: { media_type?: unknown } }).source;
    return {
      type: "image",
      source: { media_type: src?.media_type ?? "unknown" },
    };
  }
  if (Array.isArray(v)) return v.map(scrubValue);
  if (v && typeof v === "object") {
    const o: Record<string, unknown> = {};
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
      if (k === "type" && typeof val === "string")
        o[k] = val; // 块类型是判据，不换
      else if (k === "tool_use_id" || k === "id")
        o[k] = mapId(val, "t") ?? scrubValue(val);
      else o[k] = scrubValue(val);
    }
    return o;
  }
  return v;
}

interface Block {
  type?: string;
  [k: string]: unknown;
}

/**
 * 一个块换完之后的三件东西。
 *
 * 🔴 `src` / `out` 只收**会被渲染成文字的那部分载荷**，不收 id / 工具名 / signature。
 * 第一版把整个原始块和整个落盘块对着数，当场红了 87 处 —— 因为两边本来就不是一回事：
 * 原始块有 `caller`、`toolu_01AbC…`（24 字）这些字段，落盘块是白名单重建的
 * （`toolu_000000000001`，18 字）。**那 87 处不是同形替换坏了，是尺子量错了东西。**
 * 形状要守的是"多少字符折多少行"，id 和工具名换长换短不进正文排版。
 */
interface Scrubbed {
  b: Block;
  /** 源侧：这个块里会进排版的字符串 */
  src: string[];
  /** 落盘侧：同一批字符串换完之后的样子。两边逐项形状必须相同 */
  out: string[];
}

/** 递归收集一个值里所有字符串（键名不收 —— 键名两边恒等，不进正文）。 */
function collectStrings(v: unknown, into: string[]): string[] {
  const img = imageSurrogate(v);
  if (img !== null) {
    into.push(img);
    return into;
  } // 渲染成什么，形状就按什么算
  if (typeof v === "string") into.push(v);
  else if (Array.isArray(v)) v.forEach((x) => collectStrings(x, into));
  else if (v && typeof v === "object") {
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
      if (k === "tool_use_id" || k === "id" || k === "type") continue; // 换长换短不进排版
      collectStrings(val, into);
    }
  }
  return into;
}

function scrubBlock(b: Block): Scrubbed | null {
  switch (b.type) {
    case "text": {
      const src = String(b.text ?? "");
      const out = scrubText(src);
      return { b: { type: "text", text: out }, src: [src], out: [out] };
    }
    case "thinking": {
      const src = String(b.thinking ?? "");
      const out = scrubText(src);
      return {
        b: {
          type: "thinking",
          thinking: out,
          // signature 是不渲染的密码学串：不留原值，只按原长度补等长填充，保住字节数
          signature:
            typeof b.signature === "string"
              ? transliterate(b.signature)
              : undefined,
        },
        src: [src],
        out: [out],
      };
    }
    case "tool_use": {
      const name = typeof b.name === "string" ? b.name : "Unknown";
      keptToolNames.add(name);
      const input = b.input ?? {};
      const scrubbedInput = scrubValue(input);
      return {
        b: {
          type: "tool_use",
          id: mapId(b.id, "t"),
          name,
          input: scrubbedInput,
        },
        src: collectStrings(input, []),
        out: collectStrings(scrubbedInput, []),
      };
    }
    case "tool_result": {
      const content = b.content;
      const scrubbedContent = scrubValue(content);
      return {
        b: {
          type: "tool_result",
          tool_use_id: mapId(b.tool_use_id, "t"),
          content: scrubbedContent,
          ...(b.is_error === true ? { is_error: true } : {}),
        },
        src: collectStrings(content, []),
        out: collectStrings(scrubbedContent, []),
      };
    }
    default:
      return null; // 不认识的块型不落盘 —— 白名单，不是黑名单
  }
}

interface RawRecord {
  type?: string;
  isMeta?: boolean;
  uuid?: unknown;
  parentUuid?: unknown;
  timestamp?: unknown;
  isApiErrorMessage?: unknown;
  error?: unknown;
  apiErrorStatus?: unknown;
  message?: { role?: unknown; model?: unknown; content?: unknown };
}

function scrubRecord(
  rec: RawRecord,
): { line: Record<string, unknown>; src: string[]; out: string[] } | null {
  const content = rec.message?.content;
  let blocks: Block[];
  if (typeof content === "string") blocks = [{ type: "text", text: content }];
  else if (Array.isArray(content))
    blocks = (content as Block[]).filter((b) => b && typeof b === "object");
  else return null;

  const scrubbed = blocks
    .map(scrubBlock)
    .filter((x): x is Scrubbed => x !== null);
  if (scrubbed.length === 0) return null;

  const model =
    typeof rec.message?.model === "string" ? rec.message.model : undefined;
  if (model) keptModels.add(model);

  return {
    line: {
      type: rec.type,
      uuid: mapId(rec.uuid, "u"),
      parentUuid: mapId(rec.parentUuid, "u"),
      timestamp: mapTimestamp(rec.timestamp),
      ...(rec.isApiErrorMessage === true ? { isApiErrorMessage: true } : {}),
      ...(typeof rec.error === "string" ? { error: rec.error } : {}),
      ...(typeof rec.apiErrorStatus === "number"
        ? { apiErrorStatus: rec.apiErrorStatus }
        : {}),
      message: {
        role: rec.type === "user" ? "user" : "assistant",
        ...(model ? { model } : {}),
        content: scrubbed.map((x) => x.b),
      },
    },
    src: scrubbed.flatMap((x) => x.src),
    out: scrubbed.flatMap((x) => x.out),
  };
}

// ════════════════════════════════════════════════════════════════════════
// 四、分桶采样（口径不变）
// ════════════════════════════════════════════════════════════════════════

const BUCKETS: [string, number, number][] = [
  ["<2K", 0, 2048],
  ["2-8K", 2048, 8192],
  ["8-32K", 8192, 32768],
  ["32-128K", 32768, 131072],
  [">128K", 131072, Infinity],
];
const QUOTA: Record<string, number> = {
  "<2K": 10,
  "2-8K": 10,
  "8-32K": 8,
  "32-128K": 2,
  ">128K": 1,
};

function bucketOf(n: number): string {
  for (const [name, lo, hi] of BUCKETS) if (n >= lo && n < hi) return name;
  return ">128K";
}

/** 按**预测**卡型分层（理由同上一版：按 `record.type` 分层会让两个最值钱的卡型采样量最小）。 */
function predictClass(rec: RawRecord): string {
  const content = rec.message?.content;
  const blocks: Block[] = Array.isArray(content)
    ? (content as Block[])
    : typeof content === "string"
      ? [{ type: "text", text: content }]
      : [];
  const hasText = blocks.some(
    (b) => b?.type === "text" && String(b.text ?? "").trim(),
  );
  if (rec.type === "user") return hasText ? "card-user" : "tool-group";
  return hasText ? "card-assistant" : "tool-group";
}

/** 结构指纹：用来证明「换完之后形状没变」。原记录与落盘记录必须逐项相同。 */
interface Shape {
  chars: number;
  lines: number;
  words: number;
  cjk: number;
  latin: number;
  digits: number;
  fences: number;
  tableRows: number;
  maxLine: number;
}
function shapeOf(strings: string[]): Shape {
  const s: Shape = {
    chars: 0,
    lines: 0,
    words: 0,
    cjk: 0,
    latin: 0,
    digits: 0,
    fences: 0,
    tableRows: 0,
    maxLine: 0,
  };
  for (const v of strings) {
    s.chars += [...v].length;
    for (const ch of v) {
      const cp = ch.codePointAt(0)!;
      if (isWide(cp)) s.cjk++;
      else if ((cp >= 0x61 && cp <= 0x7a) || (cp >= 0x41 && cp <= 0x5a))
        s.latin++;
      else if (cp >= 0x30 && cp <= 0x39) s.digits++;
    }
    const lines = v.split("\n");
    s.lines += lines.length;
    for (const l of lines) {
      s.maxLine = Math.max(s.maxLine, l.length);
      if (/^\s*(`{3,}|~{3,})/.test(l)) s.fences++;
      if (/^\s*\|.*\|\s*$/.test(l)) s.tableRows++;
    }
    s.words += (v.match(/[A-Za-z0-9]+/g) ?? []).length;
  }
  return s;
}

interface Pick {
  line: string;
  bytes: number;
  bucket: string;
  srcShape: Shape;
  outShape: Shape;
}

// ── 扫源 ─────────────────────────────────────────────────────────────────
const now = Date.now();
const notes: string[] = [];
/** 返回这个文件要参与采样的那些行。活文件只给冻结前缀。 */
function linesOf(name: string): string[] {
  const all = readFileSync(join(SRC_DIR, name), "utf8").split("\n");
  const ageMin = (now - statSync(join(SRC_DIR, name)).mtimeMs) / 60_000;
  if (ageMin >= LIVE_WINDOW_MIN) {
    notes.push(
      `${name}：静态文件（mtime ${ageMin.toFixed(0)} 分钟前），全读 ${all.length} 行`,
    );
    return all;
  }
  if (all.length < LIVE_PREFIX_LINES) {
    notes.push(
      `${name}：⚠ 正在被写且只有 ${all.length} 行 < 冻结前缀 ${LIVE_PREFIX_LINES} ⇒ 整份跳过`,
    );
    return [];
  }
  notes.push(
    `${name}：正在被写（mtime ${ageMin.toFixed(0)} 分钟前）⇒ 只读冻结前缀 ${LIVE_PREFIX_LINES}/${all.length} 行`,
  );
  return all.slice(0, LIVE_PREFIX_LINES);
}

const files = readdirSync(SRC_DIR)
  .filter((n) => n.endsWith(".jsonl"))
  .sort();
if (files.length === 0)
  throw new Error(`${SRC_DIR} 下没有 jsonl —— 不许当成"采到了空语料"`);

const pool = new Map<string, Pick[]>();
let scanned = 0;
for (const f of files) {
  for (const line of linesOf(f)) {
    if (!line.trim()) continue;
    let rec: RawRecord;
    try {
      rec = JSON.parse(line) as RawRecord;
    } catch {
      continue;
    }
    if (rec.type !== "user" && rec.type !== "assistant") continue;
    if (rec.isMeta) continue;
    scanned++;
    const fillMark = fillPos;
    const out = scrubRecord(rec);
    if (!out) {
      fillPos = fillMark;
      continue;
    }
    const outLine = JSON.stringify(out.line);
    const bytes = byteLen(outLine); // 🔴 按**落盘后**的字节数分桶
    const key = `${predictClass(rec)}|${bucketOf(bytes)}`;
    if (!pool.has(key)) pool.set(key, []);
    pool.get(key)!.push({
      line: outLine,
      bytes,
      bucket: bucketOf(bytes),
      srcShape: shapeOf(out.src),
      outShape: shapeOf(out.out),
    });
  }
}

/** 均匀取样（不是随机）—— 同一份磁盘上重跑必须挑出同一批，否则金标准不可复算。 */
const picked: Pick[] = [];
for (const key of [...pool.keys()].sort()) {
  const [, bucket] = key.split("|");
  const arr = pool.get(key)!;
  const want = Math.min(QUOTA[bucket], arr.length);
  for (let i = 0; i < want; i++)
    picked.push(arr[Math.floor(((i + 0.5) * arr.length) / want)]);
}

if (picked.length === 0)
  throw new Error('一条都没采到 —— 不许当成"采到了空语料"（反空真自检）');
const text = picked.map((p) => p.line).join("\n") + "\n";

// ════════════════════════════════════════════════════════════════════════
// 五、两道自检 —— 不过就抛，绝不悄悄落一份脏语料
// ════════════════════════════════════════════════════════════════════════

// ① 形状自检：换完之后每一项结构量必须与原记录**逐位相同**
const shapeDiffs: string[] = [];
picked.forEach((p, i) => {
  for (const k of Object.keys(p.srcShape) as (keyof Shape)[]) {
    if (p.srcShape[k] !== p.outShape[k])
      shapeDiffs.push(
        `#${i} ${k}: 原 ${p.srcShape[k]} → 落盘 ${p.outShape[k]}`,
      );
  }
});
if (shapeDiffs.length) {
  throw new Error(
    `★ 同形替换把形状改了（${shapeDiffs.length} 处）—— 这份语料量出来的真高不再代表真机。\n  ` +
      shapeDiffs.slice(0, 12).join("\n  "),
  );
}

// ② 泄漏自检：语料里每一段 ≥4 的字母串，要么来自填充轮（只跟位置有关），
//    要么在**那份打印得出来的**保留词表里。除此之外一个字都不许有。
//
// 🔴 两个踩过的坑，都写在这里，别再踩：
//   ① **必须扫解码后的字符串，不能扫 JSON 原文**。JSON 里换行是 `\` + `n` 两个字符，
//      正则 `[A-Za-z]{4,}` 会把那个 `n` 和后面的词粘成一串（`ncdefgh`），
//      于是每一条合法填充都被报成泄漏 —— **假红**，而假红会逼人去把自检关小。
//   ② **大小写要先归一再比**。`transliterate` 对大小写用的是同一个下标
//      （`LOWER[i%26]` / `UPPER[i%26]`），所以 `Rstu` 是合法填充；
//      不归一就只认纯大写或纯小写，同样假红。
const ALLOW = new Set<string>(
  [
    ...PROTOCOL_LITERALS.flatMap((x) => x.split(/[^A-Za-z]+/)),
    ...LANGS,
    ...[...keptToolNames].flatMap((x) => x.split(/[^A-Za-z]+/)),
    ...[...keptModels].flatMap((x) => x.split(/[^A-Za-z]+/)),
    // 本文件自己写进 JSON 的键与固定值
    ...(
      "type uuid parentUuid timestamp message role model content text thinking signature " +
      "tool use result id toolu user assistant input name is error isApiErrorMessage apiErrorStatus " +
      "true false null " +
      // image 块的替身（`[image image/png]`）与 media_type 里的词
      "image unknown source media png jpeg webp " +
      // 受保护的整句（见 PROTECT_RE）
      "This session being continued from previous conversation continue where you left off " +
      "no response requested Request interrupted by"
    ).split(/\s+/),
  ].map((x) => x.toLowerCase()),
);
/**
 * 填充轮产出的字母串一定是「**逐位 +1 (mod 26)**」的一段（大小写归一后）。
 *
 * ⚠ 第一版写的是「是 `abcdefghij…` 重复两遍的子串」，被 `signature` 那几条
 * 46 字的长串顶穿了 —— 46 字从第 10 位起就超出 52 字的窗口。改成算术判定后
 * **与长度无关**，不会再因为"填充太长"而假红。
 */
function isFiller(w: string): boolean {
  const t = w.toLowerCase();
  for (let i = 1; i < t.length; i++) {
    if ((t.charCodeAt(i) - t.charCodeAt(i - 1) + 26) % 26 !== 1) return false;
  }
  return true;
}

/** 解码后逐个字符串值扫 —— 覆盖正文、工具入参、工具结果、id、模型名，全都在内。 */
function allStrings(v: unknown, into: string[] = []): string[] {
  if (typeof v === "string") into.push(v);
  else if (Array.isArray(v)) v.forEach((x) => allStrings(x, into));
  else if (v && typeof v === "object")
    Object.values(v as Record<string, unknown>).forEach((x) =>
      allStrings(x, into),
    );
  return into;
}

const leaks = new Map<string, number>();
for (const line of text.split("\n")) {
  if (!line.trim()) continue;
  for (const str of allStrings(JSON.parse(line))) {
    for (const w of str.match(/[A-Za-z]{4,}/g) ?? []) {
      if (ALLOW.has(w.toLowerCase()) || isFiller(w)) continue;
      leaks.set(w, (leaks.get(w) ?? 0) + 1);
    }
  }
}
if (leaks.size) {
  const top = [...leaks.entries()].sort((a, b) => b[1] - a[1]).slice(0, 25);
  throw new Error(
    `★ 泄漏自检失败：落盘语料里有 ${leaks.size} 个既不是填充、也不在保留词表里的字母串。\n` +
      `  这正是「这是测试啊 / 不应该进」那条裁决治的东西 —— **没有落盘**。\n  ` +
      top.map(([w, n]) => `${w} ×${n}`).join("\n  "),
  );
}

// ── 落盘 ─────────────────────────────────────────────────────────────────
mkdirSync(resolve(REPO_ROOT, "tests/__fixtures__"), { recursive: true });
writeFileSync(OUT, text, "utf8");

const agg = (sel: (p: Pick) => Shape): Shape =>
  picked.reduce(
    (a, p) => {
      const s = sel(p);
      for (const k of Object.keys(a) as (keyof Shape)[])
        a[k] = k === "maxLine" ? Math.max(a[k], s[k]) : a[k] + s[k];
      return a;
    },
    {
      chars: 0,
      lines: 0,
      words: 0,
      cjk: 0,
      latin: 0,
      digits: 0,
      fences: 0,
      tableRows: 0,
      maxLine: 0,
    },
  );
writeFileSync(
  SHAPE_OUT,
  JSON.stringify(
    {
      note: "秤 2 语料的形状指纹。`src` = 真机原记录，`out` = 落盘语料。两者必须逐项相同 —— 这就是「结构照真的，内容一律合成」那句话的可验证形式。",
      generatedAt: new Date().toISOString(),
      records: picked.length,
      sourceFiles: notes,
      src: agg((p) => p.srcShape),
      out: agg((p) => p.outShape),
      keptVerbatim: {
        note: "落盘语料里仅有的逐字保留词汇。审这份语料泄不泄漏，看的就是这张表。",
        protocolLiterals: PROTOCOL_LITERALS,
        toolNames: [...keptToolNames].sort(),
        models: [...keptModels].sort(),
      },
    },
    null,
    2,
  ) + "\n",
  "utf8",
);

console.log(`扫了 ${files.length} 个文件 / ${scanned} 条候选记录`);
for (const n of notes) console.log(`  ${n}`);
console.log(`WROTE ${OUT}`);
console.log(`  ${picked.length} 条 · ${(byteLen(text) / 1024).toFixed(1)} KB`);
console.log(`WROTE ${SHAPE_OUT}（形状指纹：src 与 out 逐项相同才落的盘）`);
console.log(
  `  逐字保留的工具名 ${keptToolNames.size} 个：${[...keptToolNames].sort().join(" ")}`,
);
console.log(
  `  逐字保留的模型 id：${[...keptModels].sort().join(" ") || "（无）"}`,
);
for (const key of [...pool.keys()].sort()) {
  const n = pool.get(key)!.length;
  console.log(
    `  ${key.padEnd(20)} 取 ${Math.min(QUOTA[key.split("|")[1]], n)}/${n}`,
  );
}
