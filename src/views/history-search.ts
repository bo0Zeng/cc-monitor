/**
 * 历史全文搜索：**本机与各台远端同一条路** —— 逐台经通道问那台后端的 `history-search`，合成一份结果。
 *
 * # 从哪来
 *
 * 〔C4a · 第四波 · 子步 3〕远端 fan-out ＋ 补 `origin` ＋ 合并从 Rust `search.rs` 搬到这里（每一件只有这一个家）。
 * 〔LOC1b · 第四波 4D〕**本机那一半也改问本机后端**（`chan.call(LOCAL_ORIGIN, "history-search", …)`）：
 * monitor 进程内那份内存索引（`search.rs::SearchIndex`〔散文墓碑〕与它的三条 Tauri 命令：搜索 · 查索引状态 · 重建索引）删了。要求住址：`设计/00 §2.5 ①`「历史 / 账号 / tmux / MCP 四个面，
 * 本机与远端走同一条代码路径」· `01 §6.8`「不存在本机一条、远端一条的同义双份」· `90 §4 F`「搜索收口到 search-core ＋ 后端」。
 * ⚠ 偏离 `05 §14.3` 表 E 行（「进程内索引 ⇒ 不迁」），主会话 09-25 按目标形裁（05 那一行由文档路改）。
 * 代价如实写：本机从此没有索引、每次现扫（读数在 `调研/第四波记录/LOC1b.md §3`）；「索引中」那一态与它的 1 秒重跑一起没了。
 *
 * # 行为
 *
 * - 逐台并发。**本机那一台失败 ⇒ 整次失败**（与迁前「本机索引那一问抛了 ⇒ 搜索失败」同形：本机是必答的那一台）；
 *   远端逐台失败只 `console.warn` 并跳过（不拖垮其余台）。
 * - 选项只下发后端认的：`include_tools` 只在真时给、`scope` 只给 `user` / `assistant`、`after_ms` 只给正数；`limit` 原样。
 * - 合并：拼接 → `updatedAt` 倒序（稳定排序）→ 命中数相加；任一会话 `hitsTruncated` ⇒ 整体 `truncated`（`K-R100`）。
 * - 本机的行不带 `origin`（界面按「没有 origin ＝ 本机」画，与迁前逐字相同）；远端的行补上那台的名字。
 * - 「哪几台远端」问的是 `list_remote_mcp_origins`（名字里的 `mcp` 是它第一个用户留下的，不是限定）。
 */
import { commands } from "../ipc/commands";
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody, linesOf } from "../ipc/chan-caller";
import { LOCAL_ORIGIN } from "../ipc/origin";

/**
 * 一条命中（后端 `--search` 行里 `hits` 的一格；形状由 [`parseHit`] 严格收）。
 * 〔LOC1b〕这三个类型原是 Rust `search.rs` 的 ts-rs 生成物；那份文件删了，线上形状的家是后端 `observe/search_query.rs`，
 * TS 这一侧只有解码器认它（多一格 / 缺一格 / 类型不对 ⇒ 那一行坏，跳过）。
 */
export interface Hit {
  /** 消息 uuid，前端打开 viewer 后据此滚动定位 + 高亮 */
  uuid: string;
  tsMs: number;
  /** "user" | "assistant" | "tool" */
  kind: string;
  before: string;
  matched: string;
  after: string;
}

/** 一个会话的命中（后端 `--search` 的一行 ＋ 这边补的 `origin`）。 */
export interface SessionHits {
  sessionId: string;
  projectPath: string;
  projectName: string;
  jsonlPath: string;
  /** ai-title / 首条 user 摘要 / sid 前 8 位 之一 */
  title: string;
  updatedAt: number;
  /** 本会话命中总数（可能 > 返回的 hits 长度） */
  hitCount: number;
  hits: Hit[];
  /** 本会话有命中被「全局 snippet 预算用完」挡下了（`K-R100`；老后端缺 ⇒ false）。 */
  hitsTruncated: boolean;
  /** 缺 ＝ 本机；否则那台远端的名字。 */
  origin?: string;
}

/** 合成之后的一份结果。 */
export interface SearchResult {
  /** 全局命中总数（各会话 `hitCount` 之和，可能 > 返回的 hit 条数） */
  totalHits: number;
  sessionCount: number;
  /** 整份结果被 `limit` 砍过（任一台、任一会话 `hitsTruncated`）。 */
  truncated: boolean;
  sessions: SessionHits[];
}

/** 全文搜索的一组入参。 */
export interface FullTextQuery {
  query: string;
  includeTools: boolean;
  scope: string | null;
  afterMs: number | null;
  limit: number | null;
}

/**
 * 一台机器那一问的期限：30 秒 —— 盖「那台后端现扫一遍全部会话 ＋ 回程」，不含握手（长连接早就连着）。
 * 〔LOC1b〕本机也用这一个（本机没有索引了，同样是现扫）。
 */
const SEARCH_BUDGET_MS = 30_000;

/** 本机 ＋ 各台远端，合成一份。 */
export async function searchAllMachines(q: FullTextQuery): Promise<SearchResult> {
  const payload = jsonBody(searchArgs(q));
  const [local, remote] = await Promise.all([askOne(LOCAL_ORIGIN, payload), searchRemotes(payload)]);
  return mergeSearchResults([...local, ...remote]);
}

/** 那一问的请求体（只下发后端认的那几格）。本机远端同一份。 */
export function searchArgs(q: FullTextQuery): Record<string, unknown> {
  const args: Record<string, unknown> = { query: q.query };
  if (q.limit !== null) args.limit = q.limit;
  if (q.includeTools) args.include_tools = true;
  if (q.scope === "user" || q.scope === "assistant") args.scope = q.scope;
  if (q.afterMs !== null && q.afterMs > 0) args.after_ms = q.afterMs;
  return args;
}

/** 问一台。本机的行不补 `origin`（界面按「缺 ＝ 本机」画）。失败原样抛给调用方定怎么办。 */
async function askOne(origin: string, payload: Uint8Array): Promise<SessionHits[]> {
  const budget = budgetWithin(SEARCH_BUDGET_MS);
  const reply = await chan.call(origin, "history-search", payload, budget);
  return parseSessionHitsLines(linesOf(reply), origin === LOCAL_ORIGIN ? undefined : origin);
}

async function searchRemotes(payload: Uint8Array): Promise<SessionHits[]> {
  let origins: string[];
  try {
    origins = await commands.list_remote_mcp_origins();
  } catch (e) {
    console.warn("远端全文搜索：拿不到远端清单（只搜本机）:", e);
    return [];
  }
  const per = await Promise.all(
    origins.map(async (origin) => {
      try {
        return await askOne(origin, payload);
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
export function parseSessionHitsLines(lines: string[], origin: string | undefined): SessionHits[] {
  const who = origin === undefined ? "本机" : `远端 [${origin}]`;
  const out: SessionHits[] = [];
  for (const line of lines) {
    let v: unknown;
    try {
      v = JSON.parse(line);
    } catch (e) {
      console.warn(`${who} 搜索结果行解析失败（跳过）:`, e);
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
      console.warn(`${who} 搜索结果行形状不对（跳过）`);
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
      ...(origin === undefined ? {} : { origin }),
    });
  }
  return out;
}

/**
 * 合并各台的结果（issue #28）：拼接、`updatedAt` 倒序（稳定）、命中数相加；
 * 🔴 `K-R100`：每一台都会被自己的 `limit` 砍 ⇒ 任一会话 `hitsTruncated` ⇒ 整体 `truncated`。
 * 〔LOC1b〕本机不再是「一份带 `indexing` / `truncated` 的整份应答」，与远端一样是一组会话行 ⇒ 合并只有这一形。
 */
export function mergeSearchResults(all: SessionHits[]): SearchResult {
  const sessions = [...all].sort((a, b) => b.updatedAt - a.updatedAt);
  return {
    totalHits: sessions.reduce((a, s) => a + s.hitCount, 0),
    sessionCount: sessions.length,
    truncated: sessions.some((s) => s.hitsTruncated),
    sessions,
  };
}
