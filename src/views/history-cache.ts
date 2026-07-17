/**
 * 历史「来源列表」缓存的纯判定逻辑（F76 · issue #46）。
 *
 * 抽成**无 import 的纯模块**便于 node 轨断言——`history.ts` 依赖 Tauri IPC / DOM，
 * 直接测不了；把最易错的 TTL 判定单拎出来钉契约。
 *
 * 背景：历史浏览器每次 `open()` 都对每台远端重新 SSH fan-out（`remote_history.rs`：每条
 * 查询独立连接一次性 exec daemon，30s/台超时，零缓存）——哪怕刚关掉再打开也逐台重连，
 * 就是 #46「历史来源每次重新加载」。本模块给**远端批**结果加 TTL 门控：TTL 内 reopen
 * 复用上次快照、不重连；**本地批便宜（<100ms）不缓存**，调用方每次重扫、天然防陈旧。
 *
 * SS-4 共享面①「来源列表带缓存」的判定核心。②筛选/折叠持久化、③动作表见各自功能
 * （F86/F85/F96）——本模块不预建（守 SS-1）。
 */

/** 一批远端来源的项目列表快照 + 抓取时刻（epoch ms）。P 由调用方指定（= HistoryProject）。 */
export interface RemoteSourceCache<P = unknown> {
  projects: P[];
  loadedAt: number;
}

/** 远端来源缓存 TTL：此窗口内 reopen 复用上次 fan-out，不重新 SSH 连所有远端。 */
export const HISTORY_REMOTE_TTL_MS = 30_000;

/**
 * 是否需要重新 fan-out 远端来源。
 *
 * - 无缓存（`null`）→ 必须抓（true）。
 * - 距上次抓取 `now - loadedAt >= ttlMs` → 过期，重抓（true）。
 * - TTL 内 → 复用（false）。
 *
 * `now`/`ttlMs` 参数化便于测。时钟回拨（`now < loadedAt`）→ 差为负 `< ttlMs` → 复用（保守，
 * 不因系统时间倒退而无谓重连所有远端）。
 */
export function shouldRefetchRemote(
  cache: { loadedAt: number } | null,
  now: number,
  ttlMs: number,
): boolean {
  if (cache === null) return true;
  return now - cache.loadedAt >= ttlMs;
}
