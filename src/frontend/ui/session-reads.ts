/**
 * **会话读面上的三条一次性查询，经通道直接问那台机器的后端**：
 * 骨架索引（`history-index`）· 大纲清单（`history-user-inputs`）· 会话内查找（`history-find`）。
 *
 * # 它顶掉了什么
 *
 * 此前这三条是 monitor 的三条 Tauri 命令（`read_session_index` / `list_user_inputs` / `find_in_session`〔散文墓碑〕）：
 * 本机每问一次 exec 一个一次性后端进程、远端走帧面按行拿回「头 ＋ 行 ＋ 尾」，monitor 再核头尾、剥行、
 * 把失败分档 —— 那一份解释住在 monitor 中层（`frame_query_tests::HELD_BACK` 那三行）。
 * 今天后端的帧应答**就是成品**（`src/backend/faces/read_face.rs`：`{from,end,rows}` / `{from,end,entries}` / `{total,hits}`），
 * 本文件经 `chan.call` 直接问、按形状收；monitor 那一跳只搬字节，三条命令与那一份解释一起删了。
 * **本机与远端同一条路**（本机那台由 `<local>` 那条长连接答）。
 *
 * # 本文件做的只有两件（都是调用方那一侧的事，不是通信层成员）
 *
 * 1. **按形状收**：字段类型不对 ⇒ 抛（不猜、不补默认值；哪一格不对只进日志）—— 这不是解释，是收货验形。
 * 2. **失败怎么说**（`§3.3.2`「说法归调用方」）：通道的三层错误折成界面那个 `available:false ＋ reason`；
 *    大纲另带种类（`oldBackend` 结构性 / 其余瞬时），那份分档**只在这里**（此前住 Rust `outline_kind`）。
 *
 * # 期限
 *
 * 每一问 30 秒（`X6`：调用点显式给）—— 与它们上一个住址（monitor `frame_query::LINES_BUDGET`）同值；
 * 盖的是「那台后端扫一遍会话 ＋ 回程」，不含握手（长连接早就连着）。
 *
 * # 第五问：会话事实（`history-facts`）
 *
 * 分叉血缘 · 改动文件集 · agent 列表 · 最新 usage 四样由后端读一遍文件出成品（`observe/facts_query.rs`），
 * 此前是活 tab 在 `onLine` 旁路上一条一条攒的。续传令牌就是**上一份成品原样**
 * （[`readSessionFacts`] 的 `prior`）—— 本文件与调用方都不读它、不改它、不合并它，只原样交回去。
 */
import { chan, ChanError, type CallError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { SkeletonFacts } from "./height-estimate";
import { copyText } from "./copy-table";

// ─── 成品的形状（后端 `read_face.rs` 那三条的应答；跨语言金样 `tests/__fixtures__/session-reads.golden.json`）───

/** 一条命中（后端 `search_query::scan_session_find` 那一形，键名一字不差；片段三段同全局搜索的 `Hit`）。 */
export interface FindHit {
  /** 命中的那条记录的 uuid —— 前端按骨架索引 `uuid → seq` 跳过去。 */
  uuid: string;
  /** `"user"` | `"assistant"` | `"tool"` */
  kind: string;
  before: string;
  matched: string;
  after: string;
  /** 第几轮（这条之前含它你说过几句；第一句之前 ＝ 0）。 */
  turn: number;
  /** 那条记录的时刻（毫秒；读不出 ＝ 0）。 */
  tsMs: number;
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

/** 最新 usage ＋ 上下文上限（后端 `facts_query::UsageFact`，上限的唯一判定在后端；百分比是排版，`views/context-limit.ts`）。 */
export interface UsageFact {
  promptTokens: number;
  model: string | null;
  /** 全会话最大的一轮。 */
  peakPromptTokens: number;
  /** 上下文上限（恒 ≥ `peakPromptTokens`）。 */
  limit: number;
  /** 上限从哪来：中转看见的请求 · 设置 · 模型名 · 见过超过 200k 的一轮 · 判不出（`limit` 只是占位，界面不算百分比）。 */
  limitFrom: "relay" | "setting" | "model" | "observed" | "assumed";
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
  /** 此刻持着这条会话的活进程 pid（那台的 pidfile，升序）。不止一个 ⇒ 几个进程在同时写这条会话。 */
  writers: number[];
  /** 还没有结果的工具调用（文件序：正在跑 / 在等批准的那几步）。 */
  pending: PendingCall[];
  /** 最后一段正文的头一行（悬停卡「它最后一句」）。 */
  lastSay: { text: string; at: string | null } | null;
  /** 需要你：那台说在等、等的是什么（后端 `facts_query::needs_of` 判；界面不猜）。不在等 ⇒ `null`。 */
  needs: Needs | null;
}

/** 一个还没有结果的工具调用（后端 `facts_query::PendingCall`）。 */
export interface PendingCall {
  id: string;
  name: string;
  /** 主参数一行（Bash 的命令 · 读写工具的路径 · 提问的第一问）；没有 ⇒ `null`。 */
  what: string | null;
  /** 那条记录的时刻（ISO 原样）；没有 ⇒ `null`。 */
  at: string | null;
}

/** 「需要你」的种类：批准一步 · 回答一问 · 批准计划 · 判不出（只说在等你）。 */
export type NeedsKind = "approve" | "answer" | "plan" | "unknown";

/** 「需要你」的成品（后端 `facts_query::Needs`）。 */
export interface Needs {
  kind: NeedsKind;
  /** 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。 */
  tool: string | null;
  /** 批准：那一步的主参数；回答：问题头一行；其余 ⇒ `null`。 */
  what: string | null;
  /** 何时起等（epoch ms）；没有 ⇒ `null`。 */
  sinceMs: number | null;
}

const NEEDS_KIND: ReadonlySet<string> = new Set(["approve", "answer", "plan", "unknown"]);

/** 会话事实的回包。`available == false` 时 `facts` 缺席、`failure` 是种类、`reason` 是给人看的原因（**不是错误**）。 */
export type FactsResult =
  | { available: true; facts: SessionFacts }
  | { available: false; reason: string; failure: OutlineFailure };

/** 查找一次最多列多少条。**显式带上**，不靠对面的缺省（对面换了缺省，「被砍过」的提示就对不上）。 */
export const FIND_LIMIT = 100;

/** 这三问各自的期限（见头注）。 */
const READ_BUDGET_MS = 30_000;

// ─── 收货验形 ───

const isObj = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === "object" && !Array.isArray(v);
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v) && v >= 0;
const isStr = (v: unknown): v is string => typeof v === "string";
const strOrNull = (x: unknown): x is string | null => x === null || isStr(x);

/**
 * 应答形状不对。给人看的那句（`message`）不带内部名；哪条命令、缺了什么住 `detail`，只进日志。
 */
class ShapeError extends Error {
  readonly detail: string;
  constructor(op: string, what: string) {
    super(copyText("sessionReads.ctor.unreadable"));
    this.name = "ShapeError";
    this.detail = copyText("sessionReads.ctor.badShape", { op, what });
  }
}

/** `history-find` 的成品 ⇒ `(total, hits)`。形状不对 ⇒ 抛。 */
export function decodeFind(v: unknown): { total: number; hits: FindHit[] } {
  if (!isObj(v) || !isNum(v.total) || !Array.isArray(v.hits)) throw new ShapeError("history-find", copyText("sessionReads.missing.find"));
  const hits = v.hits.map((h): FindHit => {
    if (!isObj(h) || ![h.uuid, h.kind, h.before, h.matched, h.after].every(isStr) || !isNum(h.turn) || !isNum(h.tsMs)) {
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
  /** 你那句的第一行（≤ 50 字）。 */
  said: string;
  tools: number;
  thinking: number;
  fails: number;
  /** 结论那几条 assistant 记录的 uuid（这一轮最后一次工具调用之后带正文的）；其余都是过程。 */
  conclusion: string[];
  /** 回复头三行（≤ 120 字）。 */
  reply: string;
  /** 这一轮收尾了（后面又有你的一句，或 Claude 说完了）。 */
  done: boolean;
}

const TURN_KEYS = ["at", "conclusion", "done", "end", "fails", "reply", "said", "start", "thinking", "tools", "uuid"] as const;

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
      ![t.at, t.tools, t.thinking, t.fails].every(isNum) ||
      ![t.uuid, t.start, t.end, t.said, t.reply].every(isStr) ||
      typeof t.done !== "boolean" ||
      !Array.isArray(t.conclusion) ||
      !t.conclusion.every(isStr)
    ) {
      return bad();
    }
    return {
      at: t.at as number,
      uuid: t.uuid as string,
      start: t.start as string,
      end: t.end as string,
      said: t.said as string,
      tools: t.tools as number,
      thinking: t.thinking as number,
      fails: t.fails as number,
      conclusion: [...(t.conclusion as string[])],
      reply: t.reply as string,
      done: t.done,
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

/** 键集合恰好是 `keys`（多一格 / 缺一格都不收）。 */
const exactKeys = (v: Record<string, unknown>, keys: readonly string[]): boolean => {
  const got = Object.keys(v).sort();
  const want = [...keys].sort();
  return got.length === want.length && got.every((k, i) => k === want[i]);
};

/**
 * `history-facts` 的成品 ⇒ [`SessionFacts`]。**每一层键集合恰好是后端出的那一形**（多一格 / 缺一格 / 类型不对 ⇒ 抛
 * 「两端契约对不上」）—— 这份成品要原样当续传令牌交回去，后端那一侧收它时同样按恰好的键集合拒（`prior_from`）。
 */
export function decodeFacts(v: unknown): SessionFacts {
  const bad = (): never => {
    throw new ShapeError("history-facts", copyText("sessionReads.missing.facts"));
  };
  if (!isObj(v) || !exactKeys(v, ["end", "forkedFrom", "lastSay", "needs", "pending", "projectDir", "touchedFiles", "usage", "writers"])) return bad();
  if (!Array.isArray(v.pending)) return bad();
  const pending: PendingCall[] = [];
  for (const p of v.pending) {
    if (!isObj(p) || !exactKeys(p, ["at", "id", "name", "what"]) || !isStr(p.id) || !isStr(p.name) || !strOrNull(p.what) || !strOrNull(p.at)) return bad();
    pending.push({ id: p.id, name: p.name, what: p.what, at: p.at });
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
    if (!isObj(n) || !exactKeys(n, ["kind", "sinceMs", "tool", "what"]) || !(isStr(n.kind) && NEEDS_KIND.has(n.kind)) || !strOrNull(n.tool) || !strOrNull(n.what) || !(n.sinceMs === null || isNum(n.sinceMs))) return bad();
    needs = { kind: n.kind as NeedsKind, tool: n.tool, what: n.what, sinceMs: n.sinceMs as number | null };
  }
  if (!Array.isArray(v.writers) || !v.writers.every(isNum)) return bad();
  if (!isNum(v.end) || !(v.forkedFrom === null || isStr(v.forkedFrom))) return bad();
  if (!(v.projectDir === null || isStr(v.projectDir))) return bad();
  if (!Array.isArray(v.touchedFiles) || !v.touchedFiles.every(isStr)) return bad();
  let usage: UsageFact | null = null;
  if (v.usage !== null) {
    const u = v.usage;
    if (
      !isObj(u) ||
      !exactKeys(u, ["limit", "limitFrom", "model", "peakPromptTokens", "promptTokens"]) ||
      !isNum(u.promptTokens) ||
      !(u.model === null || isStr(u.model)) ||
      !isNum(u.peakPromptTokens) ||
      !isNum(u.limit) ||
      !(isStr(u.limitFrom) && LIMIT_FROM.has(u.limitFrom))
    ) {
      return bad();
    }
    usage = {
      promptTokens: u.promptTokens,
      model: u.model as string | null,
      peakPromptTokens: u.peakPromptTokens,
      limit: u.limit,
      limitFrom: u.limitFrom as UsageFact["limitFrom"],
    };
  }
  return {
    end: v.end,
    forkedFrom: v.forkedFrom as string | null,
    touchedFiles: v.touchedFiles as string[],
    usage,
    projectDir: v.projectDir as string | null,
    writers: v.writers as number[],
    pending,
    lastSay,
    needs,
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

/**
 * 一次失败 ⇒ 给人看的那句话。按层说的那一份住 `ipc/chan-caller.ts::saidOf`（各调用方共用）；
 * 本文件只加一格：应答形状不对时，哪一格不对进日志（给人看的那句不带内部名）。
 */
export function reasonOf(e: unknown, oldBackendSays: string): string {
  if (e instanceof ShapeError) console.warn(`[session-reads] ${e.detail}`);
  return saidOf(e, oldBackendSays);
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
    const reason = reasonOf(e, copyText("sessionReads.find.oldBackend"));
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
    const reason = reasonOf(e, copyText("sessionReads.inputs.oldBackend"));
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
    return { available: false, reason: reasonOf(e, copyText("sessionReads.turns.oldBackend")) };
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
    const reason = reasonOf(e, "这台机器上的后端版本旧，还给不出骨架索引（重装后端之后就有）");
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
    return { available: false, reason: reasonOf(e, copyText("sessionReads.facts.oldBackend")), failure };
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
  if (!isObj(v) || Object.keys(v).length !== 2 || typeof v.present !== "boolean" || !isStr(v.root)) {
    throw new ShapeError("history-record", copyText("sessionReads.record.badShape"));
  }
  return { present: v.present, root: v.root };
}

/**
 * **resume 之前问那台机器：这条会话的记录还在不在**（最后一条）。本机与远端同一条路。
 *
 * 此前是 Tauri 命令 `probe_session_record`〔散文墓碑〕：它在 monitor 里只做「转一条 `history-record`、核两格」
 * （`frame_query::record` / `parse_record`〔散文墓碑〕）—— 后端早已出成品，那一跳一行解释都不该有 ⇒ 改成界面经通道直接问。
 *
 * **失败就抛**（不折成一个答案）：调用方（`tab-session-actions.ts::recordStillThere`）把「问不到」当「不知道」，
 * 与「不在」分开处置 —— 折成 `present:false` 会把一条接得上的 resume 拦掉。期限与上一个住址同值（30 秒）。
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
