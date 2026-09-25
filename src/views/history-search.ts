/**
 * 〔C4a · 第四波 · 子步 3〕**历史全文搜索：本机索引 ＋ 各台远端**，合成一份结果。
 *
 * # 它从哪来
 *
 * 在此之前这一整件事住 Rust：`search.rs::search_history` 里 `tokio::join!` 了本机索引与 `remote_history.rs` 那份 fan-out，
 * 后者对每台远端经 monitor 侧 `frame_query::lines(…, "history-search", …)` 问一遍、逐行反序列化、补 `origin`，
 * 最后在 `search.rs` 里合并。C4a 把「问远端」那半挪到前端、**经通道**说：
 * `chan.call(origin, "history-search", …)` —— 帧命令直接问那台的后端，monitor 那一跳只搬字节。
 * ⇒ `search_history` 只剩本机索引；fan-out ＋ 补 origin ＋ 合并三件事搬到这里，**每一件都只有这一个家**
 * （Rust 那三处同拍删掉）。
 *
 * # 行为逐格与它上一个住址相同
 *
 * - 逐台并发、逐台失败只 `console.warn` 并跳过（不拖垮其余台）；
 * - 远端选项只下发后端认的：`include_tools` 只在真时给、`scope` 只给 `user` / `assistant`、`after_ms` 只给正数；
 * - 合并：拼接 → `updatedAt` 倒序（稳定排序）→ 命中数相加；有远端结果时 `status` 一律 `ready`
 *   （不让本机的 `indexing` 吞掉远端结果）；远端任一会话 `hitsTruncated` ⇒ 整体 `truncated`（`K-R100`）；
 *   远端一条都没有 ⇒ 原样返回本机那一份（含 `indexing`）。
 * - 「哪几台远端」问的是 `list_remote_mcp_origins`：它回的正是上一个住址 fan-out 用的那张表
 *   （`load_remote_configs()` 的 `origin_label()`，含同名去重后缀）—— 名字里的 `mcp` 是它第一个用户留下的，不是限定。
 *
 * # 买不到
 *
 * - `limit` 原样下发，**不在这里夹紧**：上一个住址先 `search_core::clamp_limit` 再发，这里交给后端自己
 *   （`read_face` 把选项摊回 CLI 那一臂同一个 `parse_opts`）。主界面今天只传 300（在夹紧区间内），行为不变。
 */
import { commands } from "../ipc/commands";
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody, linesOf } from "../ipc/chan-caller";
import type { SearchResponse } from "../generated/SearchResponse";
import type { SessionHits } from "../generated/SessionHits";
import type { Hit } from "../generated/Hit";

/** 与 `commands.search_history` 同一组入参。 */
export interface FullTextQuery {
  query: string;
  includeTools: boolean;
  scope: string | null;
  afterMs: number | null;
  limit: number | null;
}

/**
 * 一台远端那一问的期限：30 秒 —— 与它上一个住址（monitor 侧 `frame_query` 的 `LINES_BUDGET`）同值：
 * 盖「远端跑一趟全文扫描 ＋ 回程」，不含握手（长连接早就连着）。
 */
const REMOTE_SEARCH_BUDGET_MS = 30_000;

/** 本机索引 ＋ 各台远端，合成一份。 */
export async function searchAllMachines(q: FullTextQuery): Promise<SearchResponse> {
  const [local, remote] = await Promise.all([commands.search_history(q), searchRemotes(q)]);
  return mergeSearchResults(local, remote);
}

/** 远端那一问的请求体（只下发后端认的那几格）。 */
export function remoteSearchArgs(q: FullTextQuery): Record<string, unknown> {
  const args: Record<string, unknown> = { query: q.query };
  if (q.limit !== null) args.limit = q.limit;
  if (q.includeTools) args.include_tools = true;
  if (q.scope === "user" || q.scope === "assistant") args.scope = q.scope;
  if (q.afterMs !== null && q.afterMs > 0) args.after_ms = q.afterMs;
  return args;
}

async function searchRemotes(q: FullTextQuery): Promise<SessionHits[]> {
  let origins: string[];
  try {
    origins = await commands.list_remote_mcp_origins();
  } catch (e) {
    console.warn("远端全文搜索：拿不到远端清单（只搜本机）:", e);
    return [];
  }
  if (origins.length === 0) return [];
  const payload = jsonBody(remoteSearchArgs(q));
  const per = await Promise.all(
    origins.map(async (origin) => {
      try {
        const budget = budgetWithin(REMOTE_SEARCH_BUDGET_MS);
        const reply = await chan.call(origin, "history-search", payload, budget);
        return parseSessionHitsLines(linesOf(reply), origin);
      } catch (e) {
        console.warn(`远端 [${origin}] 全文搜索失败（跳过该台）:`, e);
        return [];
      }
    }),
  );
  return per.flat();
}

const isStr = (v: unknown): v is string => typeof v === "string";
const isInt = (v: unknown): v is number => typeof v === "number" && Number.isInteger(v);

function parseHit(v: unknown): Hit | null {
  if (v === null || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  if (!isStr(o.uuid) || !isInt(o.tsMs) || !isStr(o.kind)) return null;
  if (!isStr(o.before) || !isStr(o.matched) || !isStr(o.after)) return null;
  return { uuid: o.uuid, tsMs: o.tsMs, kind: o.kind, before: o.before, matched: o.matched, after: o.after };
}

/**
 * 后端 `--search` 的逐行（camelCase，**不带** origin）⇒ `SessionHits`，补上 `origin`。坏行跳过（`console.warn`）。
 * 口径照它上一个住址（Rust `search.rs::SessionHits` 的 serde）：`hitsTruncated` 缺 ⇒ `false`（老后端）；
 * 其余各格必有且类型对，否则整行坏。
 */
export function parseSessionHitsLines(lines: string[], origin: string): SessionHits[] {
  const out: SessionHits[] = [];
  for (const line of lines) {
    let v: unknown;
    try {
      v = JSON.parse(line);
    } catch (e) {
      console.warn(`远端 [${origin}] 搜索结果行解析失败（跳过）:`, e);
      continue;
    }
    const o = (v !== null && typeof v === "object" ? v : {}) as Record<string, unknown>;
    const hits = Array.isArray(o.hits) ? o.hits.map(parseHit) : null;
    if (
      !isStr(o.sessionId) ||
      !isStr(o.projectPath) ||
      !isStr(o.projectName) ||
      !isStr(o.jsonlPath) ||
      !isStr(o.title) ||
      !isInt(o.updatedAt) ||
      !isInt(o.hitCount) ||
      o.hitCount < 0 ||
      hits === null ||
      hits.some((h) => h === null) ||
      !(o.hitsTruncated === undefined || typeof o.hitsTruncated === "boolean")
    ) {
      console.warn(`远端 [${origin}] 搜索结果行形状不对（跳过）`);
      continue;
    }
    out.push({
      sessionId: o.sessionId,
      projectPath: o.projectPath,
      projectName: o.projectName,
      jsonlPath: o.jsonlPath,
      title: o.title,
      updatedAt: o.updatedAt,
      hitCount: o.hitCount,
      hits: hits as Hit[],
      hitsTruncated: o.hitsTruncated ?? false,
      origin,
    });
  }
  return out;
}

/**
 * 合并本机索引结果与远端结果（issue #28）。口径逐格照它上一个住址（`search.rs` 里那份合并，已删）：
 * 远端空 ⇒ 原样返回本机（含 `indexing`）；否则拼接、`updatedAt` 倒序、命中数相加、`status` 一律 `ready`；
 * 🔴 `K-R100`：远端也会被自己的 `limit` 砍 ⇒ 任一远端会话 `hitsTruncated` ⇒ 整体 `truncated`。
 */
export function mergeSearchResults(local: SearchResponse, remote: SessionHits[]): SearchResponse {
  if (remote.length === 0) return local;
  const remoteHits = remote.reduce((a, s) => a + s.hitCount, 0);
  const remoteStarved = remote.some((s) => s.hitsTruncated);
  const sessions = [...local.sessions, ...remote].sort((a, b) => b.updatedAt - a.updatedAt);
  return {
    status: "ready",
    totalHits: local.totalHits + remoteHits,
    sessionCount: sessions.length,
    truncated: local.truncated || remoteStarved,
    indexedSessions: local.indexedSessions,
    indexedMessages: local.indexedMessages,
    sessions,
  };
}
