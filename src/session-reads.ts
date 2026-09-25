/**
 * 〔C4b · 第四波 4B · `设计/05 §8` 步 5〕**会话读面上的三条一次性查询，经通道直接问那台机器的后端**：
 * 骨架索引（`history-index`）· 大纲清单（`history-user-inputs`）· 会话内查找（`history-find`）。
 *
 * # 它顶掉了什么
 *
 * 此前这三条是 monitor 的三条 Tauri 命令（`read_session_index` / `list_user_inputs` / `find_in_session`〔散文墓碑〕）：
 * 本机每问一次 exec 一个一次性后端进程、远端走帧面按行拿回「头 ＋ 行 ＋ 尾」，monitor 再核头尾、剥行、
 * 把失败分档 —— 那一份解释住在 monitor 中层（`frame_query_tests::HELD_BACK` 那三行）。
 * 今天后端的帧应答**就是成品**（`src/backend/read_face.rs`：`{from,end,rows}` / `{from,end,entries}` / `{total,hits}`），
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
 */
import { chan, ChanError, type CallError } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { SkeletonFacts } from "./height-estimate";

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
}

/** 查找一次最多列多少条。**显式带上**，不靠对面的缺省（对面换了缺省，「被砍过」的提示就对不上）。 */
export const FIND_LIMIT = 500;

/** 这三问各自的期限（见头注）。 */
const READ_BUDGET_MS = 30_000;

// ─── 收货验形 ───

const isObj = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === "object" && !Array.isArray(v);
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v) && v >= 0;
const isStr = (v: unknown): v is string => typeof v === "string";

/**
 * 应答形状不对。给人看的那句（`message`）不带内部名；哪条命令、缺了什么住 `detail`，只进日志。
 */
class ShapeError extends Error {
  readonly detail: string;
  constructor(op: string, what: string) {
    super("那台机器的后端回的内容本程序读不懂，多半是两边版本对不上（重装那台机器的后端试试）");
    this.name = "ShapeError";
    this.detail = `${op} 的应答${what}（两端契约对不上）`;
  }
}

/** `history-find` 的成品 ⇒ `(total, hits)`。形状不对 ⇒ 抛。 */
export function decodeFind(v: unknown): { total: number; hits: FindHit[] } {
  if (!isObj(v) || !isNum(v.total) || !Array.isArray(v.hits)) throw new ShapeError("history-find", "缺 `total` / `hits`");
  const hits = v.hits.map((h): FindHit => {
    if (!isObj(h) || ![h.uuid, h.kind, h.before, h.matched, h.after].every(isStr)) {
      throw new ShapeError("history-find", "里有一条命中缺字段");
    }
    return { uuid: h.uuid as string, kind: h.kind as string, before: h.before as string, matched: h.matched as string, after: h.after as string };
  });
  return { total: v.total, hits };
}

/** `history-user-inputs` 的成品 ⇒ `(from, end, entries)`。形状不对 ⇒ 抛。 */
export function decodeUserInputs(v: unknown): { from: number; end: number; entries: UserInputEntry[] } {
  if (!isObj(v) || !isNum(v.from) || !isNum(v.end) || !Array.isArray(v.entries)) {
    throw new ShapeError("history-user-inputs", "缺 `from` / `end` / `entries`");
  }
  const entries = v.entries.map((e): UserInputEntry => {
    if (!isObj(e) || ![e.uuid, e.excerpt, e.timestamp].every(isStr)) {
      throw new ShapeError("history-user-inputs", "里有一条缺字段");
    }
    return { uuid: e.uuid as string, excerpt: e.excerpt as string, timestamp: e.timestamp as string };
  });
  return { from: v.from, end: v.end, entries };
}

/** `history-index` 的成品 ⇒ `(from, end, rows)`。行本身**不解释**（只验「是对象、带偏移与行长」）。 */
export function decodeIndex(v: unknown): { from: number; end: number; rows: SkeletonFacts[] } {
  if (!isObj(v) || !isNum(v.from) || !isNum(v.end) || !Array.isArray(v.rows)) {
    throw new ShapeError("history-index", "缺 `from` / `end` / `rows`");
  }
  for (const r of v.rows) {
    if (!isObj(r) || !isNum(r.o) || !isNum(r.n)) throw new ShapeError("history-index", "里有一行缺 `o` / `n`");
  }
  return { from: v.from, end: v.end, rows: v.rows as SkeletonFacts[] };
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

/** **在这一份会话里找** `query`（大小写不敏感子串；`includeTools` 同全局搜索那个勾）。 */
export async function findInSession(
  origin: Origin,
  jsonlPath: string,
  query: string,
  includeTools: boolean,
): Promise<FindResult> {
  try {
    const body = jsonBody({ path: jsonlPath, query, include_tools: includeTools, limit: FIND_LIMIT });
    const budget = budgetWithin(READ_BUDGET_MS);
    const reply = await chan.call(origin, "history-find", body, budget);
    const { total, hits } = decodeFind(readJson(reply));
    return { available: true, hits, total };
  } catch (e) {
    const reason = reasonOf(e, "这台机器上的后端版本旧，还不能在会话里查找（重装后端之后就有）");
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
    const reason = reasonOf(e, "这台机器上的后端版本旧，还列不出大纲（重装后端之后就有）");
    return { available: false, reason, failure, from: fromOffset, end: fromOffset, entries: [] };
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
    return { available: false, reason, from: fromOffset, end: fromOffset, rows: [] };
  }
}
