/**
 * **会话读面上的三条一次性查询，经通道直接问那台机器的后端**：
 * 骨架索引（`history-index`）· 大纲清单（`history-user-inputs`）· 会话内查找（`history-find`）。
 * 后端的帧应答就是成品（`src/backend/faces/read_face.rs`：`{from,end,rows}` / `{from,end,entries}` / `{total,hits}`），
 * 本文件经 `chan.call` 直接问、按形状收；monitor 那一跳只搬字节。本机与远端同一条路（本机那台由 `<local>` 那条长连接答）。
 *
 * # 本文件做的只有两件（都是调用方那一侧的事，不是通信层成员）
 *
 * 1. **按形状收**：字段类型不对 ⇒ 抛（不猜、不补默认值；哪一格不对只进日志）—— 这不是解释，是收货验形。
 * 2. **失败怎么说**（`§3.3.2`「说法归调用方」）：通道的三层错误折成界面那个 `available:false ＋ reason`；
 *    大纲另带种类（`oldBackend` 结构性 / 其余瞬时），那份分档**只在这里**。
 *
 * # 期限
 *
 * 每一问 30 秒（调用点显式给）：盖的是「那台后端扫一遍会话 ＋ 回程」，不含握手（长连接早就连着）。
 *
 * # 第五问：会话事实（`history-facts`）
 *
 * 分叉血缘 · 改动文件集 · agent 列表 · 最新 usage 四样由后端读一遍文件出成品（`observe/facts_query.rs`）。续传令牌就是**上一份成品原样**
 * （[`readSessionFacts`] 的 `prior`）—— 本文件与调用方都不读它、不改它、不合并它，只原样交回去。
 */
import { chan, ChanError, type CallError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { SkeletonFacts } from "./height-estimate";
import { copyText } from "./copy-table";
import { exactKeys, isObj } from "./ipc/decode";

// ─── 成品的形状（后端 `read_face.rs` 那三条的应答；跨语言金样 `tests/__fixtures__/session-reads.golden.json`）───

/** 一条命中（后端 `search_query::scan_session_find` 那一形，键名一字不差；片段三段同全局搜索的 `Hit`）。 */
export interface FindHit {
  /** 命中的那条记录的 uuid —— 前端按骨架索引 `uuid → seq` 跳过去。 */
  uuid: string;
  /** `"user"` | `"assistant"` | `"report"`（agent 回报）| `"tool"` */
  kind: string;
  before: string;
  matched: string;
  after: string;
  /** 第几轮（这条之前含它你说过几句；第一句之前 ＝ 0）。 */
  turn: number;
  /** 那条记录的时刻（毫秒；读不出 ＝ 0）。 */
  tsMs: number;
  /** 它写给人看的样子（那台后端按它的本地钟写好；读不出 ⇒ 空串）。 */
  tsText: string;
}

/** 查找的回包。`available == false` 时 `hits` 为空、`reason` 是给人看的原因（**不是错误**）。 */
export interface FindResult {
  available: boolean;
  reason?: string;
  /** 按文件顺序（= 对话顺序），最多 [`FIND_LIMIT`] 条。 */
  hits: FindHit[];
  /** 全量命中数（≥ `hits.length`；大于 ⇒ 被上限砍过，面板要说出来）。 */
  total: number;
}

/** 一条可点的大纲项（后端 `user_inputs::UserInputRow`，键名一字不差）。 */
export interface UserInputEntry {
  uuid: string;
  /** 列表上显示的摘要（后端已折叠、已截断）。 */
  excerpt: string;
  /** 记录的 `timestamp`；没有 ⇒ 空串。 */
  timestamp: string;
}

/**
 * **要不到大纲的种类** —— 大纲据它决定「还要不要再要」，**只 match 它，不解析 `reason` 的文字**。
 *
 * | 种类 | 从通道的哪一层来 | 大纲怎么办 |
 * |---|---|---|
 * | `oldBackend` | 对端**事前**就说不认这条命令（`peer/unsupported`：那台后端比这条查询老） | 结构性：不再要 |
 * | `truncated` | 对端说「装不下」（`too_large`：整份超过一帧的上限，后端明拒、不截断） | 瞬时：下一次触发再要 |
 * | `transport` | 其余：够不着那台 · 期限到 · 撤了 · 对端说别的「不行」（含越过文件尾）· 应答形状不对 | 瞬时：下一次触发再要 |
 */
export type OutlineFailure = "oldBackend" | "truncated" | "transport";

/** 大纲清单的回包。`available == false` 时 `entries` 为空、`failure` 是种类、`reason` 是给人看的原因（**不是错误**）。 */
export interface UserInputsResult {
  available: boolean;
  reason?: string;
  failure?: OutlineFailure;
  /** 这份清单从哪个字节起（= 请求的 `fromOffset`）。 */
  from: number;
  /** 最后一个完整行的末字节 ＝ **下一次增量该带的 `fromOffset`**。 */
  end: number;
  /** 按文件顺序（= 对话顺序）。 */
  entries: UserInputEntry[];
}

/** 骨架索引的回包。`available == false` 时 `rows` 为空、`reason` 说清为什么（**不是错误**）。 */
export interface SessionIndexResult {
  available: boolean;
  reason?: string;
  /** 索引从哪个字节起（= 请求的 `fromOffset`）。 */
  from: number;
  /** 最后一个完整行的末字节 ＝ **下一次续传该带的 `fromOffset`**。 */
  end: number;
  /** 每个可计行一条（形状 = 后端 `IndexRow`，`IPC-PROTOCOL.md §10.3`）。本文件**不解释**这些行。 */
  rows: SkeletonFacts[];
  /** 要不到的种类（分档同大纲：`oldBackend` 结构性 / 其余瞬时 ⇒ 下一次触发点再问一次）。 */
  failure?: OutlineFailure;
}

/** 一格字的语气（后端 `common::cells::Tone`）：出口按它选颜色。 */
export type Tone = "plain" | "fail" | "now" | "need" | "warn";

/** 最新 usage ＋ 上下文上限 ＋ 写好的字（后端 `facts_query::UsageFact`：上限、百分比、字都由后端定，界面照抄）。 */
export interface UsageFact {
  promptTokens: number;
  model: string | null;
  /** 全会话最大的一轮。 */
  peakPromptTokens: number;
  /** 上下文上限（恒 ≥ `peakPromptTokens`）。 */
  limit: number;
  /** 上限从哪来：中转看见的请求 · 设置 · 模型名 · 见过超过 200k 的一轮 · 判不出（`limit` 只是占位，界面不算百分比）。 */
  limitFrom: "relay" | "setting" | "model" | "observed" | "assumed";
  /** 最新一轮占上限的百分比（0–100）；上限判不出 ⇒ `null`。 */
  percent: number | null;
  /** 上下文那一格的字（判得出写 `35%`，判不出写 `350k`）。 */
  contextText: string;
  /** 那一格的语气（快满 ⇒ `warn`）。 */
  contextTone: Tone;
  /** 最新一轮用了多少（`350k`）。 */
  promptTokensText: string;
  /** 上限（`1M`）。 */
  limitText: string;
  /** 上限从哪来的字；判不出 ⇒ `null`。 */
  limitFromText: string | null;
}

const LIMIT_FROM: ReadonlySet<string> = new Set(["relay", "setting", "model", "observed", "assumed"]);

/**
 * 一份会话的事实（后端 `facts_query::SessionFacts`，**帧面成品的形状**；跨语言金样
 * `tests/__fixtures__/session-reads.golden.json` 的 `history-facts` 一格）。它同时就是下一次的续传令牌。
 */
export interface SessionFacts {
  /** 最后一个完整行的末字节。 */
  end: number;
  /** 源会话 sid；不是分叉来的 ⇒ `null`。 */
  forkedFrom: string | null;
  /** 写类工具碰过的文件，近因序（最近碰的在末尾）。 */
  touchedFiles: string[];
  usage: UsageFact | null;
  /** 会话的项目目录（会话起在哪个目录；那台后端读记录开头给的）。开头里还没有 ⇒ `null`。 */
  projectDir: string | null;
  /** 这份记录是哪一家的（线上的 kind，那台后端按记录认）；认不出 ⇒ `null`。 */
  agent: string | null;
  /** 此刻持着这条会话的活进程 pid（那台的 pidfile，升序）。不止一个 ⇒ 几个进程在同时写这条会话。 */
  writers: number[];
  /** 还没有结果的工具调用（文件序：正在跑 / 在等批准的那几步）。 */
  pending: PendingCall[];
  /** 最后一段正文的头一行（悬停卡「它最后一句」）。 */
  lastSay: { text: string; at: string | null } | null;
  /** 需手动：那台说在等、等的是什么（后端 `facts_query::needs_of` 判；界面不猜）。不在等 ⇒ `null`。 */
  needs: Needs | null;
  /** 交回了的子运行（子 agent 的 id，文件序）：同一个子运行的收场通知以交回为准，消息流里不再另画。 */
  handedBack: string[];
  /** 一串一串的 API 重试与结局（后端按首条的 uuid 记；消息流里那条细条挂在首条上，按它的 id 读）。 */
  retries: RetryRun[];
  /** 此刻的许可档（原样）；没有 ⇒ `null`。 */
  permissionMode: string | null;
  /** 全会话用量（后端按请求去重、写缓存分两档；`text` 是写好的成品）。 */
  tokens: TokenUse | null;
  /** 全会话花费（记录里那一家自己记的；`text` 是写好的成品）。记录里没有 ⇒ `null`。 */
  cost: { micros: number; partial: boolean; text: string } | null;
  /** 还没收场的后台命令（后端累加的那份账，续传时原样交回；界面不读）。 */
  bgTasks: BgTask[];
  /** 后台任务运行中那一句（后端 `facts_query::background_of` 写；界面照抄、时长那一截按会走的那一句走字）。不是这一态 ⇒ `null`。 */
  background: BackgroundWork | null;
}

/** 一条还没收场的后台命令（续传令牌的一部分）。 */
export interface BgTask {
  call: string;
  task: string | null;
  cmd: string | null;
  at: string | null;
}

/** 后台任务运行中那一态的成品：`text` 发出那一刻写好的一句 · `clock` 同一句时长留 `{dur}`（填 现在 − `from`）· `what` 命令那一格 · `count` 几条。 */
export interface BackgroundWork {
  text: string;
  clock: { text: string; from: number } | null;
  what: string | null;
  count: number;
  tone: string;
}

/** 全会话用量（后端 `facts_query::TokenUse`）。`last` 只是续传要的，界面不读。 */
export interface TokenUse {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite5m: number;
  cacheWrite1h: number;
  requests: number;
  text: string;
  last: { id: string; tokens: number[] } | null;
}

/** 一串相邻的 API 重试（后端 `facts_query::RetryRun`）。 */
export interface RetryRun {
  /** 首条重试记录的 uuid。 */
  id: string;
  outcome: RetryOutcome;
}

/** 一串重试的结局：还没下文 · 接上了 · 没接上（来了报错那条）· 人发了一句或打断。 */
export type RetryOutcome = "retrying" | "recovered" | "failed" | "interrupted";
const RETRY_OUTCOME: ReadonlySet<string> = new Set<RetryOutcome>(["retrying", "recovered", "failed", "interrupted"]);

/** 一个还没有结果的工具调用（后端 `facts_query::PendingCall`）。 */
export interface PendingCall {
  id: string;
  name: string;
  /** 主参数一行（Bash 的命令 · 读写工具的路径 · 提问的第一问）；没有 ⇒ `null`。 */
  what: string | null;
  /** 那条记录的时刻（ISO 原样）；没有 ⇒ `null`。 */
  at: string | null;
  /** 这一步此刻的样子（后端 `facts_query::settle_pending` 判）：在跑 · 在等你 · 状态不明。界面只读它，不按「没有结果」当在跑。 */
  state: StepWait;
  /** 状态不明的原因（后端给的码）；别的状态 ⇒ `null`。 */
  why: UnclearWhy | null;
}

/** 一步状态不明的原因：没有活进程持着这条会话 · 这一家不留 pidfile、判不了活。 */
export type UnclearWhy = "noWriter" | "untracked";
const UNCLEAR_WHY: ReadonlySet<string> = new Set<UnclearWhy>(["noWriter", "untracked"]);

/** 一步还没结果时的样子（后端 `facts_query::StepWait`）。 */
export type StepWait = "running" | "awaiting" | "unclear";
const STEP_WAIT: ReadonlySet<string> = new Set<StepWait>(["running", "awaiting", "unclear"]);

/** 「需手动」的种类（后端 `facts_query::NeedsKind` 判好）：批准一步 · 回答一问 · 批准计划 · 放行联网 · 批准协作请求 · 确认会话目标 · 在对话框里选 · 判不出。 */
export type NeedsKind = "approve" | "answer" | "plan" | "network" | "worker" | "goal" | "choose" | "unknown";

/** 「需手动」的成品（后端 `facts_query::Needs`）。 */
export interface Needs {
  kind: NeedsKind;
  /** 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。 */
  tool: string | null;
  /** 那个调用的 id（过程里那一步据此画成「在等你批准」）；判不出 ⇒ `null`。 */
  call: string | null;
  /** 批准：那一步的主参数；回答：问题头一行；其余 ⇒ `null`。 */
  what: string | null;
  /** 何时起等（epoch ms）；没有 ⇒ `null`。 */
  sinceMs: number | null;
  /** 核心写好的字（等批准 · 等回答 · 需手动），照抄。 */
  text: string;
  /** 语气（恒 `need`）。 */
  tone: string;
  /** 先答哪个（0 最先：顶上的框先答，再按危险度；后端 `facts_query::NEEDS_BY_DANGER`）。 */
  rank: number;
  /** 到那台答出那一刻已等多久（毫秒，那台的钟上算的；不拿本机钟减 `sinceMs`）；没有起点 ⇒ `null`。 */
  waitedMs: number | null;
  /** `waitedMs` 写好的字（答出那一刻）；没有起点 ⇒ `null`。 */
  waitedText: string | null;
  /** 本机收到这一份的时刻（本机钟，不在线上）：会走的钟从它起接着加（`cards/step-line.ts::waitedNow`）。 */
  receivedAt: number;
}

const NEEDS_KIND: ReadonlySet<string> = new Set<NeedsKind>(["approve", "answer", "plan", "network", "worker", "goal", "choose", "unknown"]);

/** 会话事实的回包。`available == false` 时 `facts` 缺席、`failure` 是种类、`reason` 是给人看的原因（**不是错误**）。 */
export type FactsResult =
  | { available: true; facts: SessionFacts }
  | { available: false; reason: string; failure: OutlineFailure };

/** 查找一次最多列多少条。**显式带上**，不靠对面的缺省（对面换了缺省，「被砍过」的提示就对不上）。 */
export const FIND_LIMIT = 100;

/** 这三问各自的期限（见头注）。 */
const READ_BUDGET_MS = 30_000;

// ─── 收货验形 ───

const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v) && v >= 0;
const isStr = (v: unknown): v is string => typeof v === "string";
const strOrNull = (x: unknown): x is string | null => x === null || isStr(x);

/** 应答形状不对（那台回的东西认不出）。`message` 只是细目（哪条命令、缺了什么，进日志）；给人看的那句由 [`reasonOf`] 按码取。 */
class ShapeError extends ReplyUnreadable {
  constructor(op: string, what: string) {
    super(`${op} reply: ${what}`);
    this.name = "ShapeError";
  }
}

/** `history-find` 的成品 ⇒ `(total, hits)`。形状不对 ⇒ 抛。 */
export function decodeFind(v: unknown): { total: number; hits: FindHit[] } {
  if (!isObj(v) || !isNum(v.total) || !Array.isArray(v.hits)) throw new ShapeError("history-find", copyText("sessionReads.missing.find"));
  const hits = v.hits.map((h): FindHit => {
    if (!isObj(h) || ![h.uuid, h.kind, h.before, h.matched, h.after, h.tsText].every(isStr) || !isNum(h.turn) || !isNum(h.tsMs)) {
      throw new ShapeError("history-find", copyText("sessionReads.missing.findHit"));
    }
    return {
      uuid: h.uuid as string,
      kind: h.kind as string,
      before: h.before as string,
      matched: h.matched as string,
      after: h.after as string,
      turn: h.turn,
      tsMs: h.tsMs,
      tsText: h.tsText as string,
    };
  });
  return { total: v.total, hits };
}

/** `history-user-inputs` 的成品 ⇒ `(from, end, entries)`。形状不对 ⇒ 抛。 */
export function decodeUserInputs(v: unknown): { from: number; end: number; entries: UserInputEntry[] } {
  if (!isObj(v) || !isNum(v.from) || !isNum(v.end) || !Array.isArray(v.entries)) {
    throw new ShapeError("history-user-inputs", copyText("sessionReads.missing.inputs"));
  }
  const entries = v.entries.map((e): UserInputEntry => {
    if (!isObj(e) || ![e.uuid, e.excerpt, e.timestamp].every(isStr)) {
      throw new ShapeError("history-user-inputs", copyText("sessionReads.missing.inputsEntry"));
    }
    return { uuid: e.uuid as string, excerpt: e.excerpt as string, timestamp: e.timestamp as string };
  });
  return { from: v.from, end: v.end, entries };
}

/** 一轮的摘要（后端 `observe/turns.rs::TurnRow`，键名一字不差）：过程行 · 刻度悬停 · 折哪几条都按它。 */
export interface TurnSummary {
  /** 这一轮开头（你那句）那一行的字节位置；还在跑的最后一轮下次从这里再取。 */
  at: number;
  uuid: string;
  start: string;
  end: string;
  /** 起止各自的钟面（后端按那台本地钟写好的 `HH:MM`；解不出 ⇒ 空串）。 */
  startText: string;
  endText: string;
  /** 你那句的第一行（≤ 50 字）。 */
  said: string;
  /** 工具调用（派 agent 不算）· 思考 · 派出 agent · 后台任务通知 · API 重试 · 来话 · 失败合计（后端 `turns.rs` 数）。 */
  tools: number;
  thinking: number;
  agents: number;
  background: number;
  retries: number;
  peers: number;
  fails: number;
  /** 结尾：留在过程折叠外面的那几条记录的 uuid（结论正文；没有 ⇒ 停下这一轮的中断标记 / 报错卡）。 */
  ending: string[];
  /** 回复头三行（≤ 120 字）。 */
  reply: string;
  /** 这一轮收尾了（后面又有你的一句，或 Claude 说完了，或你中断了它）。 */
  done: boolean;
  /** 此刻：没在跑 · 在跑 · 在等你（后端按这台的会话事实判）。 */
  phase: TurnPhase;
  /** 过程行的字（后端写好；空 ⇒ 这一轮不出过程行、不折）。 */
  parts: TurnPart[];
  /** 过程行右端那一截：字里的 `{dur}` 由界面填 `to − from`（`to` 缺 ⇒ 到现在）。 */
  span: TurnSpan;
}

export type TurnPhase = "idle" | "running" | "awaiting";
export type TurnTone = "plain" | "fail" | "now" | "need";
export interface TurnPart {
  text: string;
  tone: TurnTone;
}
export interface TurnSpan {
  text: string;
  from: number | null;
  to: number | null;
}

const TURN_KEYS = ["agents", "at", "background", "done", "end", "endText", "ending", "fails", "parts", "peers", "phase", "reply", "retries", "said", "span", "start", "startText", "thinking", "tools", "uuid"] as const;
const PHASES: readonly string[] = ["idle", "running", "awaiting"];
const TONES: readonly string[] = ["plain", "fail", "now", "need", "warn"] satisfies readonly Tone[];
const numOrNull = (v: unknown): v is number | null => v === null || isNum(v);

/** `history-turns` 的成品 ⇒ `(from, end, turns)`。键集合恰好、类型逐格对；不对 ⇒ 抛。 */
export function decodeTurns(v: unknown): { from: number; end: number; turns: TurnSummary[] } {
  const bad = (): never => {
    throw new ShapeError("history-turns", copyText("sessionReads.missing.turns"));
  };
  if (!isObj(v) || !exactKeys(v, ["end", "from", "turns"]) || !isNum(v.from) || !isNum(v.end) || !Array.isArray(v.turns)) return bad();
  const turns = v.turns.map((t): TurnSummary => {
    if (
      !isObj(t) ||
      !exactKeys(t, TURN_KEYS) ||
      ![t.at, t.tools, t.thinking, t.agents, t.background, t.retries, t.peers, t.fails].every(isNum) ||
      ![t.uuid, t.start, t.end, t.startText, t.endText, t.said, t.reply].every(isStr) ||
      typeof t.done !== "boolean" ||
      !isStr(t.phase) ||
      !PHASES.includes(t.phase) ||
      !Array.isArray(t.ending) ||
      !t.ending.every(isStr) ||
      !Array.isArray(t.parts) ||
      !t.parts.every((p) => isObj(p) && exactKeys(p, ["text", "tone"]) && isStr(p.text) && isStr(p.tone) && TONES.includes(p.tone)) ||
      !isObj(t.span) ||
      !exactKeys(t.span, ["from", "text", "to"]) ||
      !isStr(t.span.text) ||
      !numOrNull(t.span.from) ||
      !numOrNull(t.span.to)
    ) {
      return bad();
    }
    const span = t.span as { text: string; from: number | null; to: number | null };
    return {
      at: t.at as number,
      uuid: t.uuid as string,
      start: t.start as string,
      end: t.end as string,
      startText: t.startText as string,
      endText: t.endText as string,
      said: t.said as string,
      tools: t.tools as number,
      thinking: t.thinking as number,
      agents: t.agents as number,
      background: t.background as number,
      retries: t.retries as number,
      peers: t.peers as number,
      fails: t.fails as number,
      ending: [...(t.ending as string[])],
      reply: t.reply as string,
      done: t.done,
      phase: t.phase as TurnPhase,
      parts: (t.parts as TurnPart[]).map((p) => ({ text: p.text, tone: p.tone })),
      span: { text: span.text, from: span.from, to: span.to },
    };
  });
  return { from: v.from, end: v.end, turns };
}

/** `history-index` 的成品 ⇒ `(from, end, rows)`。行本身**不解释**（只验「是对象、带偏移与行长」）。 */
export function decodeIndex(v: unknown): { from: number; end: number; rows: SkeletonFacts[] } {
  if (!isObj(v) || !isNum(v.from) || !isNum(v.end) || !Array.isArray(v.rows)) {
    throw new ShapeError("history-index", copyText("sessionReads.missing.index"));
  }
  for (const r of v.rows) {
    if (!isObj(r) || !isNum(r.o) || !isNum(r.n)) throw new ShapeError("history-index", copyText("sessionReads.missing.indexRow"));
  }
  return { from: v.from, end: v.end, rows: v.rows as SkeletonFacts[] };
}


/**
 * `history-facts` 的成品 ⇒ [`SessionFacts`]。**每一层键集合恰好是后端出的那一形**（多一格 / 缺一格 / 类型不对 ⇒ 抛
 * 「两端契约对不上」）—— 这份成品要原样当续传令牌交回去，后端那一侧收它时同样按恰好的键集合拒（`prior_from`）。
 */
export function decodeFacts(v: unknown, receivedAt: number = Date.now()): SessionFacts {
  const bad = (): never => {
    throw new ShapeError("history-facts", copyText("sessionReads.missing.facts"));
  };
  if (!isObj(v) || !exactKeys(v, ["agent", "background", "bgTasks", "cost", "end", "forkedFrom", "handedBack", "lastSay", "needs", "pending", "permissionMode", "projectDir", "retries", "tokens", "touchedFiles", "usage", "writers"])) return bad();
  if (!strOrNull(v.permissionMode)) return bad();
  let tokens: TokenUse | null = null;
  if (v.tokens !== null) {
    const p = v.tokens;
    const nums = ["input", "output", "cacheRead", "cacheWrite5m", "cacheWrite1h", "requests"] as const;
    if (!isObj(p) || !exactKeys(p, ["cacheRead", "cacheWrite1h", "cacheWrite5m", "input", "last", "output", "requests", "text"])) return bad();
    if (!nums.every((k) => isNum(p[k])) || !isStr(p.text)) return bad();
    let last: TokenUse["last"] = null;
    if (p.last !== null) {
      const l = p.last;
      if (!isObj(l) || !exactKeys(l, ["id", "tokens"]) || !isStr(l.id) || !Array.isArray(l.tokens) || !l.tokens.every(isNum)) return bad();
      last = { id: l.id, tokens: [...(l.tokens as number[])] };
    }
    tokens = {
      input: p.input as number,
      output: p.output as number,
      cacheRead: p.cacheRead as number,
      cacheWrite5m: p.cacheWrite5m as number,
      cacheWrite1h: p.cacheWrite1h as number,
      requests: p.requests as number,
      text: p.text,
      last,
    };
  }
  let cost: SessionFacts["cost"] = null;
  if (v.cost !== null) {
    const c = v.cost;
    if (!isObj(c) || !exactKeys(c, ["micros", "partial", "text"]) || !isNum(c.micros) || typeof c.partial !== "boolean" || !isStr(c.text)) return bad();
    cost = { micros: c.micros, partial: c.partial, text: c.text };
  }
  if (!Array.isArray(v.pending)) return bad();
  const pending: PendingCall[] = [];
  for (const p of v.pending) {
    if (!isObj(p) || !exactKeys(p, ["at", "id", "name", "state", "what", "why"]) || !isStr(p.id) || !isStr(p.name) || !strOrNull(p.what) || !strOrNull(p.at)) return bad();
    if (!(isStr(p.state) && STEP_WAIT.has(p.state)) || !(p.why === null || (isStr(p.why) && UNCLEAR_WHY.has(p.why)))) return bad();
    pending.push({ id: p.id, name: p.name, what: p.what, at: p.at, state: p.state as StepWait, why: p.why as UnclearWhy | null });
  }
  if (!Array.isArray(v.retries)) return bad();
  const retries: RetryRun[] = [];
  for (const r of v.retries) {
    if (!isObj(r) || !exactKeys(r, ["id", "outcome"]) || !isStr(r.id) || !(isStr(r.outcome) && RETRY_OUTCOME.has(r.outcome))) return bad();
    retries.push({ id: r.id, outcome: r.outcome as RetryOutcome });
  }
  let lastSay: SessionFacts["lastSay"] = null;
  if (v.lastSay !== null) {
    const l = v.lastSay;
    if (!isObj(l) || !exactKeys(l, ["at", "text"]) || !isStr(l.text) || !strOrNull(l.at)) return bad();
    lastSay = { text: l.text, at: l.at };
  }
  let needs: Needs | null = null;
  if (v.needs !== null) {
    const n = v.needs;
    if (!isObj(n) || !exactKeys(n, ["call", "kind", "rank", "sinceMs", "text", "tone", "tool", "waitedMs", "waitedText", "what"]) || !(isStr(n.kind) && NEEDS_KIND.has(n.kind)) || !strOrNull(n.tool) || !strOrNull(n.call) || !strOrNull(n.what) || !(n.sinceMs === null || isNum(n.sinceMs)) || !isStr(n.text) || !isStr(n.tone) || !isNum(n.rank) || !numOrNull(n.waitedMs) || !(n.waitedText === null || isStr(n.waitedText))) return bad();
    needs = { kind: n.kind as NeedsKind, tool: n.tool, call: n.call, what: n.what, sinceMs: n.sinceMs as number | null, text: n.text, tone: n.tone, rank: n.rank, waitedMs: n.waitedMs, waitedText: n.waitedText, receivedAt };
  }
  if (!Array.isArray(v.bgTasks)) return bad();
  const bgTasks: BgTask[] = [];
  for (const t of v.bgTasks) {
    if (!isObj(t) || !exactKeys(t, ["at", "call", "cmd", "task"]) || !isStr(t.call) || !strOrNull(t.task) || !strOrNull(t.cmd) || !strOrNull(t.at)) return bad();
    bgTasks.push({ call: t.call, task: t.task, cmd: t.cmd, at: t.at });
  }
  let background: BackgroundWork | null = null;
  if (v.background !== null) {
    const b = v.background;
    if (!isObj(b) || !exactKeys(b, ["clock", "count", "text", "tone", "what"]) || !isStr(b.text) || !strOrNull(b.what) || !isNum(b.count) || !isStr(b.tone)) return bad();
    let clock: BackgroundWork["clock"] = null;
    if (b.clock !== null) {
      const c = b.clock;
      if (!isObj(c) || !exactKeys(c, ["from", "text"]) || !isStr(c.text) || !isNum(c.from)) return bad();
      clock = { text: c.text, from: c.from };
    }
    background = { text: b.text, clock, what: b.what, count: b.count, tone: b.tone };
  }
  if (!Array.isArray(v.writers) || !v.writers.every(isNum)) return bad();
  if (!isNum(v.end) || !(v.forkedFrom === null || isStr(v.forkedFrom))) return bad();
  if (!(v.projectDir === null || isStr(v.projectDir))) return bad();
  if (!(v.agent === null || isStr(v.agent))) return bad();
  if (!Array.isArray(v.touchedFiles) || !v.touchedFiles.every(isStr)) return bad();
  if (!Array.isArray(v.handedBack) || !v.handedBack.every(isStr)) return bad();
  let usage: UsageFact | null = null;
  if (v.usage !== null) {
    const u = v.usage;
    if (
      !isObj(u) ||
      !exactKeys(u, ["contextText", "contextTone", "limit", "limitFrom", "limitFromText", "limitText", "model", "peakPromptTokens", "percent", "promptTokens", "promptTokensText"]) ||
      !isNum(u.promptTokens) ||
      !(u.model === null || isStr(u.model)) ||
      !isNum(u.peakPromptTokens) ||
      !isNum(u.limit) ||
      !(isStr(u.limitFrom) && LIMIT_FROM.has(u.limitFrom)) ||
      !numOrNull(u.percent) ||
      !isStr(u.contextText) ||
      !(isStr(u.contextTone) && TONES.includes(u.contextTone)) ||
      !isStr(u.promptTokensText) ||
      !isStr(u.limitText) ||
      !(u.limitFromText === null || isStr(u.limitFromText))
    ) {
      return bad();
    }
    usage = {
      promptTokens: u.promptTokens,
      model: u.model as string | null,
      peakPromptTokens: u.peakPromptTokens,
      limit: u.limit,
      limitFrom: u.limitFrom as UsageFact["limitFrom"],
      percent: u.percent as number | null,
      contextText: u.contextText,
      contextTone: u.contextTone as Tone,
      promptTokensText: u.promptTokensText,
      limitText: u.limitText,
      limitFromText: u.limitFromText as string | null,
    };
  }
  return {
    end: v.end,
    forkedFrom: v.forkedFrom as string | null,
    touchedFiles: v.touchedFiles as string[],
    usage,
    projectDir: v.projectDir as string | null,
    agent: v.agent as string | null,
    writers: v.writers as number[],
    pending,
    lastSay,
    needs,
    handedBack: v.handedBack as string[],
    retries,
    permissionMode: v.permissionMode as string | null,
    tokens,
    cost,
    bgTasks,
    background,
  };
}

// ─── 失败怎么说（唯一住址）───

/** 通道的三层错误 ⇒ 大纲的种类。**穷尽**、不看文字。 */
export function failureOf(e: CallError): OutlineFailure {
  switch (e.layer) {
    case "peer":
      if (e.why === "unsupported") return "oldBackend";
      return refusalOf(e.body)?.code === "too_large" ? "truncated" : "transport";
    case "hop":
    case "ours":
      return "transport";
  }
}

/** 问 `origin` 那台的一次失败 ⇒ 给人看的那句话（`ipc/chan-caller.ts::saidFrom`，各调用方共用）。 */
export function reasonOf(e: unknown, origin: Origin): string {
  return saidFrom(e, origin);
}

// ─── 三问 ───

/** **在这一份会话里找** `query`（大小写不敏感子串；`includeTools` 同全局搜索那个勾）；`skip` ＝ 跳过前几条（续下一页）。 */
export async function findInSession(
  origin: Origin,
  jsonlPath: string,
  query: string,
  includeTools: boolean,
  skip = 0,
): Promise<FindResult> {
  try {
    const body = jsonBody(skip > 0 ? { path: jsonlPath, query, include_tools: includeTools, limit: FIND_LIMIT, skip } : { path: jsonlPath, query, include_tools: includeTools, limit: FIND_LIMIT });
    const budget = budgetWithin(READ_BUDGET_MS);
    const reply = await chan.call(origin, "history-find", body, budget);
    const { total, hits } = decodeFind(readJson(reply));
    return { available: true, hits, total };
  } catch (e) {
    const reason = reasonOf(e, origin);
    return { available: false, reason, hits: [], total: 0 };
  }
}

/** **「你说过的话」清单**：从字节 `fromOffset` 起（冷启动 0 / 增量传上次的 `end`）。 */
export async function listUserInputs(origin: Origin, jsonlPath: string, fromOffset: number): Promise<UserInputsResult> {
  try {
    const body = jsonBody({ path: jsonlPath, from: fromOffset });
    const budget = budgetWithin(READ_BUDGET_MS);
    const reply = await chan.call(origin, "history-user-inputs", body, budget);
    const { from, end, entries } = decodeUserInputs(readJson(reply));
    return { available: true, from, end, entries };
  } catch (e) {
    const failure: OutlineFailure = e instanceof ChanError ? failureOf(e.error) : "transport";
    const reason = reasonOf(e, origin);
    return { available: false, reason, failure, from: fromOffset, end: fromOffset, entries: [] };
  }
}

/** 每轮的摘要的回包：要不到 ⇒ `available:false`（过程行与刻度不出，正文照常）。 */
export type TurnsResult = { available: true; from: number; end: number; turns: TurnSummary[] } | { available: false; reason: string };

/** **每轮的摘要**：从字节 `fromOffset` 起（冷启动 0 / 续取传还在跑的那一轮的 `at`）。 */
export async function readTurns(origin: Origin, jsonlPath: string, fromOffset: number): Promise<TurnsResult> {
  try {
    const body = jsonBody({ path: jsonlPath, from: fromOffset });
    const budget = budgetWithin(READ_BUDGET_MS);
    const reply = await chan.call(origin, "history-turns", body, budget);
    return { available: true, ...decodeTurns(readJson(reply)) };
  } catch (e) {
    return { available: false, reason: reasonOf(e, origin) };
  }
}

/** **骨架索引**：从字节 `fromOffset` 起（冷启动 0 / 续传传上次的 `end`）。要不到 ⇒ `available:false`（退回尾部窗口）。 */
export async function readSessionIndex(origin: Origin, jsonlPath: string, fromOffset: number): Promise<SessionIndexResult> {
  try {
    const body = jsonBody({ path: jsonlPath, offset: fromOffset });
    const budget = budgetWithin(READ_BUDGET_MS);
    const reply = await chan.call(origin, "history-index", body, budget);
    const { from, end, rows } = decodeIndex(readJson(reply));
    return { available: true, from, end, rows };
  } catch (e) {
    const reason = reasonOf(e, origin);
    const failure: OutlineFailure = e instanceof ChanError ? failureOf(e.error) : "transport";
    return { available: false, reason, failure, from: fromOffset, end: fromOffset, rows: [] };
  }
}

/**
 * **会话事实**：`prior` = 上一次拿到的那份成品**原样**（续传令牌；`null` ⇒ 从字节 0 扫）。
 * 要不到 ⇒ `available:false` ＋ 种类（分档同大纲：`oldBackend` 结构性 / 其余瞬时）＋ 一句人话。
 */
export async function readSessionFacts(
  origin: Origin,
  jsonlPath: string,
  prior: SessionFacts | null,
  limits: Readonly<Record<string, number>> = {},
): Promise<FactsResult> {
  try {
    const args: Record<string, unknown> = { path: jsonlPath };
    if (prior) args.prior = prior;
    if (Object.keys(limits).length > 0) args.limits = limits; // 设置里的上限表：上限在那台后端定
    const body = jsonBody(args);
    const budget = budgetWithin(READ_BUDGET_MS);
    const reply = await chan.call(origin, "history-facts", body, budget);
    return { available: true, facts: decodeFacts(readJson(reply)) };
  } catch (e) {
    const failure: OutlineFailure = e instanceof ChanError ? failureOf(e.error) : "transport";
    return { available: false, reason: reasonOf(e, origin), failure };
  }
}

// ─── 第四问：这条会话的记录还在不在（resume 一跳先问）───

/**
 * 那台机器的后端对「这条会话的记录还在不在」的答案（后端 `read_face.rs` 的 `history-record`，成品 `{present, root}`）。
 * `root` = 查的那棵记录树的根（答「不在」时要说清查的是哪里）。
 */
export interface RecordProbe {
  present: boolean;
  root: string;
}

/** `history-record` 的成品 ⇒ [`RecordProbe`]。两格缺一格 / 多一格 / 类型不对 ⇒ 抛 —— **绝不**把缺字段读成「不在」。 */
export function decodeRecord(v: unknown): RecordProbe {
  if (!isObj(v) || !exactKeys(v, ["present", "root"]) || typeof v.present !== "boolean" || !isStr(v.root)) {
    throw new ShapeError("history-record", copyText("sessionReads.record.badShape"));
  }
  return { present: v.present, root: v.root };
}

/**
 * **resume 之前问那台机器：这条会话的记录还在不在**（最后一条）。本机与远端同一条路。
 *
 * 后端出成品（`history-record`），界面经通道直接问。
 *
 * **失败就抛**（不折成一个答案）：调用方（`tab-session-actions.ts::recordStillThere`）把「问不到」当「不知道」，
 * 与「不在」分开处置 —— 折成 `present:false` 会把一条接得上的 resume 拦掉。期限 30 秒。
 */
export async function probeSessionRecord(
  origin: Origin,
  sid: string,
  configDir?: string,
): Promise<RecordProbe> {
  // 带上这次 resume 要用的账号配置目录（`CLAUDE_CONFIG_DIR`）：会话起在那个账号根下时，
  //   只查那台后端自己的家目录会答「不在」、误拦 resume。基座（没有账号）⇒ 不带。
  const body = jsonBody(configDir ? { sid, configDir } : { sid });
  const budget = budgetWithin(READ_BUDGET_MS);
  const reply = await chan.call(origin, "history-record", body, budget);
  return decodeRecord(readJson(reply));
}
