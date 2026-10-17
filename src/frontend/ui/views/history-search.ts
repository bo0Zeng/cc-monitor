/**
 * 历史全文搜索：本机与各台远端同一条路 —— 逐台经通道问那台后端的 `history-search`，合成一份结果。
 * 本机也问本机后端（没有进程内索引，每次现扫）。
 *
 * - 逐台并发。本机那一台失败 ⇒ 整次失败（本机是必答的那一台）；远端逐台失败只 `console.warn` 并跳过。
 * - 选项只下发后端认的：`include_tools` 只在真时给、`scope` 只给 `user` / `assistant`、`after_ms` 只给正数；`limit` 原样。
 * - 合并：各台的会话行按台一次交给本机后端 `history-search-merge`（`updatedAt` 倒序、稳定；命中数相加；
 *   任一会话 `hitsTruncated` ⇒ 整体 `truncated`；远端的行由它添上那台的名字，本机的不添）—— 规则只住 Rust，扇出在这里。
 * - 两条都是 typed 帧命令（`callTyped`）：入参 · 回包的类型从 Rust 生成，形状由握手时的指纹核过，这里不再逐格解。
 * - 「哪几台远端」问 `list_remote_mcp_origins`（名字里的 `mcp` 是历史，不是限定）。
 */
import { commands } from "../ipc/commands";
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../ipc/origin";
import type { Origin } from "../generated/Origin";
import type { HistorySearchReq } from "../generated/HistorySearchReq";
import type { MachineHits } from "../generated/MachineHits";
import type { MergedRow } from "../generated/MergedRow";
import type { SearchHit } from "../generated/SearchHit";

/** 一条命中（后端 `observe/search_query.rs::SearchHit` 生成的类型）。 */
export type Hit = SearchHit;

/** 合好的一个会话的命中（后端 `MergedRow`：那台答的会话行 ＋ 哪台（本机缺）＋ 写好的 `atText` · `spanText`）。 */
export type SessionHits = MergedRow;

/** 合成之后的一份结果。 */
export interface SearchResult {
  /** 全局命中总数（各会话摆出来的 ＋ `moreHits`） */
  totalHits: number;
  sessionCount: number;
  /** 整份结果被 `limit` 砍过（任一台、任一会话 `hitsTruncated`）。 */
  truncated: boolean;
  sessions: SessionHits[];
  /** 没答上的那几台远端（它们的会话没搜到）。 */
  failedHosts: string[];
  /** 各台读不动、没搜到的会话记录份数之和。 */
  unreadable: number;
  /** 内容搜索不覆盖的那几家（各台后端说的，对用户的叫法，去重）。 */
  skipped: string[];
}

/** 一台答回来的：那台的名字 ＋ 会话行 ＋ 读不动几份 ＋ 不覆盖的那几家。 */
interface OneMachine {
  rows: MachineHits;
  unreadable: number;
  skipped: string[];
}

/** 全文搜索的一组入参。 */
export interface FullTextQuery {
  query: string;
  includeTools: boolean;
  scope: string | null;
  afterMs: number | null;
  limit: number | null;
  /** 只比会话标题与第一句（「按项目」那一路的搜索，后端搜全部会话）。 */
  titles?: boolean;
}

/**
 * 一台机器那一问的期限：30 秒 —— 盖「那台后端现扫一遍全部会话 ＋ 回程」，不含握手（长连接早就连着）。
 * 本机也用这一个（本机没有索引了，同样是现扫）。
 */
const SEARCH_BUDGET_MS = 30_000;

/** 合一份那一问的期限：本机后端里的纯计算（排序 ＋ 求和），盖住本机那条流的往返与那份载荷即可。 */
const MERGE_BUDGET_MS = 10_000;

/** 本机 ＋ 各台远端，合成一份（合并排序问本机后端，见头注）。 */
export async function searchAllMachines(q: FullTextQuery): Promise<SearchResult> {
  const req = searchArgs(q);
  const [local, remote] = await Promise.all([askOne(LOCAL_ORIGIN, req), searchRemotes(req)]);
  const answered = [local, ...remote.answered];
  const merged = await callTyped(LOCAL_ORIGIN, "history-search-merge", { machines: answered.map((m) => m.rows) }, budgetWithin(MERGE_BUDGET_MS));
  return {
    ...merged,
    failedHosts: remote.failed,
    unreadable: answered.reduce((n, m) => n + m.unreadable, 0),
    skipped: [...new Set(answered.flatMap((m) => m.skipped))],
  };
}

/** 那一问的请求体（只下发后端认的那几格）。本机远端同一份。 */
export function searchArgs(q: FullTextQuery): HistorySearchReq {
  const req: HistorySearchReq = { query: q.query };
  if (q.limit !== null) req.limit = q.limit;
  if (q.includeTools) req.include_tools = true;
  if (q.scope === "user" || q.scope === "assistant" || q.scope === "report") req.scope = q.scope;
  if (q.afterMs !== null && q.afterMs > 0) req.after_ms = q.afterMs;
  if (q.titles) req.titles = true;
  return req;
}

/** 问一台。失败原样抛给调用方定怎么办。 */
async function askOne(origin: Origin, req: HistorySearchReq): Promise<OneMachine> {
  const reply = await callTyped(origin, "history-search", req, budgetWithin(SEARCH_BUDGET_MS));
  return { rows: { origin, sessions: reply.sessions }, unreadable: reply.unreadable, skipped: reply.skipped };
}

/** 各台远端：答上的那几份 ＋ 没答上的那几台（界面说「X 断开，它的会话没搜到」）。 */
async function searchRemotes(req: HistorySearchReq): Promise<{ answered: OneMachine[]; failed: string[] }> {
  let origins: string[];
  try {
    origins = await commands.list_remote_mcp_origins();
  } catch (e) {
    console.warn("远端全文搜索：拿不到远端清单（只搜本机）:", e);
    return { answered: [], failed: [] };
  }
  const per = await Promise.all(
    origins.map(async (origin) => {
      try {
        return { origin, got: await askOne(origin, req) };
      } catch (e) {
        console.warn(`远端 [${origin}] 全文搜索失败（跳过该台）:`, e);
        return { origin, got: null };
      }
    }),
  );
  return {
    answered: per.flatMap((p) => (p.got === null ? [] : [p.got])),
    failed: per.filter((p) => p.got === null).map((p) => p.origin),
  };
}
