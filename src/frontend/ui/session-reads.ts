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
 * 分叉血缘 · 改动文件集 · agent 列表 · 最新 usage 四样由后端读一遍文件出成品（`observe/facts_query.rs`）。续传令牌是上一份成品里的
 * `{end, rev}` 两格（[`readSessionFacts`] 的 `prior`；那台常驻后端记着那一份累加器，`observe/facts_cache.rs`）—— 本文件与调用方不读、不合并成品。
 */
import { ChanError, type CallError } from "../../comms/inward/chan";
import { budgetWithin, callTyped, refusalOf } from "./ipc/chan-caller";
import type { RecordProbe } from "./generated/RecordProbe";
import type { FindHit } from "./generated/FindHit";
import type { HistoryIndex } from "./generated/HistoryIndex";
import type { HistoryIndexReq } from "./generated/HistoryIndexReq";
import type { HistoryTurns } from "./generated/HistoryTurns";
import type { UserInputs } from "./generated/UserInputs";
import type { UserInputRow } from "./generated/UserInputRow";
import type { Origin } from "./ipc/origin";
import type { SkeletonFacts } from "./height-estimate";
import type { TurnRow } from "./generated/TurnRow";
import type { SessionFacts as WireFacts } from "./generated/SessionFacts";
import type { HistoryFactsReq } from "./generated/HistoryFactsReq";
import type { Needs as WireNeeds } from "./generated/Needs";
import type { NeedsKind } from "./generated/NeedsKind";
import type { NeedsAsk } from "./generated/NeedsAsk";
import type { NeedsChoice } from "./generated/NeedsChoice";
import type { NeedsQuestion } from "./generated/NeedsQuestion";
import type { UsageFact } from "./generated/UsageFact";
import type { RetryOutcome } from "./generated/RetryOutcome";
import type { PendingCall } from "./generated/PendingCall";
import type { FactsBackground } from "./generated/FactsBackground";
import { saidOf } from "./kit/detail";

// ─── 成品的形状（后端 `read_face.rs` 那三条的应答；跨语言金样 `tests/__fixtures__/session-reads.golden.json`）───

/** 一条命中（类型从后端生成：`search_query::FindHit`）。 */
export type { FindHit };

/** 查找的回包。`available == false` 时 `hits` 为空、`reason` 是给人看的原因（**不是错误**）。 */
export interface FindResult {
  available: boolean;
  reason?: string;
  /** 按文件顺序（= 对话顺序），最多 [`FIND_LIMIT`] 条。 */
  hits: FindHit[];
  /** 全量命中数（≥ `hits.length`；大于 ⇒ 被上限砍过，面板要说出来）。 */
  total: number;
}

/** 一条可点的大纲项（类型从后端生成：`user_inputs::UserInputRow`）。 */
export type UserInputEntry = UserInputRow;

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

/** 最新 usage ＋ 上下文上限 ＋ 写好的字（后端 `facts_query::UsageFact`：上限、百分比、字都由后端定，界面照抄）。 */
export type { UsageFact };
/** 一串重试的结局 · 一个还没有结果的工具调用 · 「需手动」的种类（类型从后端生成）。 */
export type { RetryOutcome, PendingCall, NeedsKind };
/** 后台任务运行中那一态的成品（后端 `facts_query::Background`）。 */
export type BackgroundWork = FactsBackground;
/** 一条挂着的待答请求 · 一个选项 · 一道题（后端 `agents/asks.rs`，会话 `needs.ask`）。 */
export type Ask = NeedsAsk;
export type AskChoice = NeedsChoice;
export type AskQuestionItem = NeedsQuestion;

/** 「需手动」的成品（后端 `facts_query::Needs`），多一格本机收到它的时刻。 */
export type Needs = WireNeeds & {
  /** 本机收到这一份的时刻（本机钟，不在线上）：会走的钟从它起接着加（`duration-format.ts::waitedNow`）。 */
  receivedAt: number;
};

/**
 * 一份会话的事实（后端 `facts_query::SessionFacts`，类型从后端生成），`needs` 多一格本机收到的时刻。
 */
export type SessionFacts = Omit<WireFacts, "needs"> & { needs: Needs | null };

/** 会话事实的回包。`available == false` 时 `facts` 缺席、`failure` 是种类、`reason` 是给人看的原因（**不是错误**）。 */
export type FactsResult =
  | { available: true; facts: SessionFacts }
  | { available: false; reason: string; failure: OutlineFailure };

/** 查找一次最多列多少条。**显式带上**，不靠对面的缺省（对面换了缺省，「被砍过」的提示就对不上）。 */
export const FIND_LIMIT = 100;

/** 这三问各自的期限（见头注）。 */
const READ_BUDGET_MS = 30_000;

/**
 * `history-index` 的成品装回逐行（帧面上它**按列排**，后端 `history_face.rs::pack_index`：偏移是与上一行末尾的差、
 * `t` 是 `classes` 里的号、稀疏的几格各一张小表、`at` 是行号差）；下游照旧读 [`SkeletonFacts`]。行本身**不解释**。
 * 形状由握手时的形状指纹管，这里只装、不验。
 */
export function unpackIndex(v: HistoryIndex): SkeletonFacts[] {
  const rows: Array<SkeletonFacts & { x?: string; ts?: string }> = [];
  let at = v.from;
  for (let i = 0; i < v.count; i++) {
    const r: SkeletonFacts = { o: at + v.o[i], n: v.n[i] };
    at = r.o + r.n;
    const k = v.t[i];
    if (k > 0) r.t = v.classes[k - 1] as SkeletonFacts["t"];
    const id = v.u[i];
    if (id !== null) r.u = id;
    if (v.fd[i] > 0) r.fd = v.fd[i];
    rows.push(r);
  }
  /** 一张稀疏小表的行号列 ⇒ 每一项落在第几行（`at` 是与上一项的行号差，第一项与 −1 比）。 */
  const lines = (ats: number[]): number[] => {
    let i = -1;
    return ats.map((g) => (i += g + 1));
  };
  lines(v.body.at).forEach((i, j) => {
    const r = rows[i];
    const b = v.body;
    if (b.ch[j] > 0) r.ch = b.ch[j];
    if (b.cj[j] > 0) r.cj = b.cj[j];
    if (b.pl[j] > 0) r.pl = b.pl[j];
    if (b.cb[j] > 0) r.cb = b.cb[j];
    if (b.cl[j] > 0) r.cl = b.cl[j];
  });
  lines(v.sp.at).forEach((i, j) => (rows[i].sp = v.sp.v[j]));
  lines(v.inputs.at).forEach((i, j) => {
    rows[i].x = v.inputs.x[j];
    if (v.inputs.ts[j] !== "") rows[i].ts = v.inputs.ts[j];
  });
  return rows;
}

/** 收到的那一份 ⇒ [`SessionFacts`]（`needs` 记上本机收到的时刻）。 */
export function factsOf(v: WireFacts, receivedAt: number = Date.now()): SessionFacts {
  return { ...v, needs: v.needs === null ? null : { ...v.needs, receivedAt } };
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

// ─── 三问 ───

/**
 * **在这一份会话里找** `query`（大小写不敏感子串；`includeTools` 同全局搜索那个勾）；`skip` ＝ 跳过前几条（续下一页）。
 * `scope: "user"` ＝ 只搜用户说的（那台判，与 `history-search` 同名同义）；这时不带工具那一格（用户说的话没有工具输出）。
 */
export async function findInSession(
  origin: Origin,
  jsonlPath: string,
  query: string,
  includeTools: boolean,
  skip = 0,
  scope?: "user",
): Promise<FindResult> {
  try {
    const which = scope ? { scope } : { include_tools: includeTools };
    const budget = budgetWithin(READ_BUDGET_MS);
    const { total, hits } = await callTyped(origin, "history-find", { path: jsonlPath, query, ...which, limit: FIND_LIMIT, ...(skip > 0 ? { skip } : {}) }, budget);
    return { available: true, hits, total };
  } catch (e) {
    const reason = saidOf(e);
    return { available: false, reason, hits: [], total: 0 };
  }
}

/** **「你说过的话」清单**：从字节 `fromOffset` 起（冷启动 0 / 增量传上次的 `end`）；那台按页给（一页有字节上限），照 `next` 一页一页接着问、拼成一份。 */
export async function listUserInputs(origin: Origin, jsonlPath: string, fromOffset: number): Promise<UserInputsResult> {
  try {
    const entries: UserInputEntry[] = [];
    let at: number | null = fromOffset;
    let end = fromOffset;
    while (at !== null) {
      const page: UserInputs = await callTyped(origin, "history-user-inputs", { path: jsonlPath, from: at }, budgetWithin(READ_BUDGET_MS));
      entries.push(...page.entries);
      end = page.end;
      at = page.next ?? null;
    }
    return { available: true, from: fromOffset, end, entries };
  } catch (e) {
    const failure: OutlineFailure = e instanceof ChanError ? failureOf(e.error) : "transport";
    const reason = saidOf(e);
    return { available: false, reason, failure, from: fromOffset, end: fromOffset, entries: [] };
  }
}

/** 每轮的摘要的回包：要不到 ⇒ `available:false`（过程行与刻度不出，正文照常）。 */
export type TurnsResult = { available: true; from: number; end: number; turns: TurnRow[] } | { available: false; reason: string };

/** **每轮的摘要**：从字节 `fromOffset` 起（冷启动 0 / 续取传还在跑的那一轮的 `at`）；照 `next` 一页一页接着问、拼成一份。 */
export async function readTurns(origin: Origin, jsonlPath: string, fromOffset: number): Promise<TurnsResult> {
  try {
    const turns: TurnRow[] = [];
    let at: number | null = fromOffset;
    let end = fromOffset;
    while (at !== null) {
      const page: HistoryTurns = await callTyped(origin, "history-turns", { path: jsonlPath, from: at }, budgetWithin(READ_BUDGET_MS));
      turns.push(...page.turns);
      end = page.end;
      at = page.next ?? null;
    }
    return { available: true, from: fromOffset, end, turns };
  } catch (e) {
    return { available: false, reason: saidOf(e) };
  }
}

/**
 * **骨架索引**：从字节 `fromOffset` 起（冷启动 0 / 续传传上次的 `end`）；照 `next` 一页一页接着问、拼成一份。
 * `want` ＝ 只要前这么多行（正在看的 tab 只要尾段之上那一截：每页 `limit` ＝ 还差几行，够了就停；`end` 是最后一行的末字节，续传从那里接）。
 * 要不到 ⇒ `available:false`（退回尾部窗口）。
 */
export async function readSessionIndex(origin: Origin, jsonlPath: string, fromOffset: number, want?: number): Promise<SessionIndexResult> {
  try {
    const rows: SkeletonFacts[] = [];
    let at: number | null = fromOffset;
    let end = fromOffset;
    while (at !== null && (want === undefined || rows.length < want)) {
      const args: HistoryIndexReq = { path: jsonlPath, offset: at };
      if (want !== undefined) args.limit = want - rows.length;
      const page = await callTyped(origin, "history-index", args, budgetWithin(READ_BUDGET_MS));
      rows.push(...unpackIndex(page));
      end = page.end;
      at = page.next ?? null;
    }
    return { available: true, from: fromOffset, end, rows };
  } catch (e) {
    const reason = saidOf(e);
    const failure: OutlineFailure = e instanceof ChanError ? failureOf(e.error) : "transport";
    return { available: false, reason, failure, from: fromOffset, end: fromOffset, rows: [] };
  }
}

/**
 * **会话事实**：`prior` = 上一次拿到的那份成品（只交它的 `{end, rev}` 当续传令牌；`null` ⇒ 从字节 0 扫）。
 * 那台续算表里没有了 ⇒ `stale_prior`，与别的失败一样落进 `available:false`，调用方不带令牌重要（`facts-source.ts`）。
 * 要不到 ⇒ `available:false` ＋ 种类（分档同大纲：`oldBackend` 结构性 / 其余瞬时）＋ 一句人话。
 */
export async function readSessionFacts(
  origin: Origin,
  jsonlPath: string,
  prior: SessionFacts | null,
): Promise<FactsResult> {
  try {
    const args: HistoryFactsReq = { path: jsonlPath };
    if (prior) args.prior = { end: prior.end, rev: prior.rev };
    const budget = budgetWithin(READ_BUDGET_MS);
    return { available: true, facts: factsOf(await callTyped(origin, "history-facts", args, budget)) };
  } catch (e) {
    const failure: OutlineFailure = e instanceof ChanError ? failureOf(e.error) : "transport";
    return { available: false, reason: saidOf(e), failure };
  }
}

// ─── 第四问：这条会话的记录还在不在（resume 一跳先问）───

/** 那台机器的后端对「这条会话的记录还在不在」的答案（`present` · `root` = 查的那棵记录树的根；类型从后端生成）。 */
export type { RecordProbe };

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
  const budget = budgetWithin(READ_BUDGET_MS);
  return await callTyped(origin, "history-record", configDir ? { sid, configDir } : { sid }, budget);
}
