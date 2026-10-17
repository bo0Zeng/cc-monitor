/**
 * 历史全文搜索：问本机后端一次 `history-search-merge {machines, search}` —— 它自己问到各台（本机同进程搜、别的台经它的连接问那台的常驻
 * `history-search`），合成一份再答（`updatedAt` 倒序、稳定；命中数 · 读不动的份数相加；不覆盖的那几家去重；远端的行带那台的名字；
 * 没答上的那几台进 `down`）。本机那一台没答上 ⇒ 整次失败（本机是必答的那一台）。规则与扇出都只住 Rust，这里只交问哪几台、怎么搜。
 *
 * - 选项只下发后端认的：`include_tools` 只在真时给、`scope` 只给 `user` / `assistant` / `report`、`after_ms` 只给正数；`limit` 原样。
 * - typed 帧命令（`callTyped`）：入参 · 回包的类型从 Rust 生成，形状由握手时的指纹核过，这里不再逐格解。
 * - 「哪几台远端」问 `list_remote_mcp_origins`（名字里的 `mcp` 是历史，不是限定）。
 */
import { commands } from "../ipc/commands";
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../ipc/origin";
import type { Origin } from "../generated/Origin";
import type { HistorySearchReq } from "../generated/HistorySearchReq";
import type { Merged } from "../generated/Merged";
import type { MergedRow } from "../generated/MergedRow";
import type { SearchHit } from "../generated/SearchHit";

/** 一条命中（`cmd_wire::search::SearchHit` 生成的类型）。 */
export type Hit = SearchHit;

/** 合好的一个会话的命中（`MergedRow`：那台答的会话行 ＋ 那台写好的 `atText` · `spanText` ＋ 哪台（本机缺））。 */
export type SessionHits = MergedRow;

/** 合成之后的一份结果（`Merged`：总数 · 截没截 · 各行 · 读不动几份 · 不覆盖的那几家 · 没答上的那几台）。 */
export type SearchResult = Merged;

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
 * 那一问的期限：30 秒 —— 盖「各台后端现扫一遍全部会话 ＋ 回程 ＋ 本机后端合一份」（各台同时问），不含握手（连接早就连着）。
 */
const SEARCH_BUDGET_MS = 30_000;

/** 本机 ＋ 各台远端，本机后端问到各台、合成一份（见头注）。 */
export async function searchAllMachines(q: FullTextQuery): Promise<SearchResult> {
  let remotes: string[] = [];
  try {
    remotes = await commands.list_remote_mcp_origins();
  } catch (e) {
    console.warn("远端全文搜索：拿不到远端清单（只搜本机）:", e);
  }
  const machines: Origin[] = [LOCAL_ORIGIN, ...remotes];
  return callTyped(LOCAL_ORIGIN, "history-search-merge", { machines, search: searchArgs(q) }, budgetWithin(SEARCH_BUDGET_MS));
}

/** 那一问的请求体（只下发后端认的那几格）。各台同一份。 */
export function searchArgs(q: FullTextQuery): HistorySearchReq {
  const req: HistorySearchReq = { query: q.query };
  if (q.limit !== null) req.limit = q.limit;
  if (q.includeTools) req.include_tools = true;
  if (q.scope === "user" || q.scope === "assistant" || q.scope === "report") req.scope = q.scope;
  if (q.afterMs !== null && q.afterMs > 0) req.after_ms = q.afterMs;
  if (q.titles) req.titles = true;
  return req;
}
