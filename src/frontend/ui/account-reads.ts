/**
 * **账号域的读面**：经通道问那台机器（本机也一样）的后端，带 per-origin TTL 缓存 ＋ 手动刷新。
 *
 * 三条帧命令：账号清单 `accounts-list`· 会话 ↔ 账号 `accounts-sessions`· 信任预检 `accounts-trust`，
 * 外加本机 apikey 表那一问（`apikey_routing_for`）。全程走 A2 的 `available:false` 降级：未迁移 / 旧后端一律安静隐藏账号 UI，不报错。
 *
 * 从 `accounts.ts` 拆出来（审计 B §6 必须拆 4；守的要求「一个判定只有一个家」）：
 * 形状与规则留在 `accounts.ts`（纯），这里只管「去问、收、缓存」。
 */
import { putAccounts } from "./app-store";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, isOldBackend, jsonBody, linesOf, readJson, saidFrom } from "./ipc/chan-caller";
import { DEFAULT_AGENT, lookupAgentProfile, type AgentProfileRow } from "./agent-profile";
import { AGENT_PROFILE_TABLE } from "./generated/agent-profile-table";
import { decodeAccountsList, decodeTrust } from "./accounts-decode";
import type { AccountsState, SessionAccount } from "./accounts";
// API key 那两问的成品（`apikey-routing`）住 `apikey-reads.ts`；本文件只给账号面包一层（`agent` 与账号清单同一个出处）。
import { fetchApikeyRouting, type ApikeyRoutingView } from "./apikey-reads";

const ACCOUNTS_TTL_MS = 30_000; // 账号列表极少变（迁移/登录才变），缓存久一点省 SSH
const SESSION_ACCOUNTS_TTL_MS = 8_000; // 会话账号归属随起停变，照 tabs.ts tmuxCache 的 8s
interface CacheEntry<T> {
  at: number;
  value: T;
}
const accountsCache = new Map<string, CacheEntry<AccountsState>>();
const sessionAccountsCache = new Map<string, CacheEntry<SessionAccount[]>>();

/** 每台最近一次答成的那一份 ＋ 时刻（这次运行里、这个窗口里；那台没问到时画「上次的」）。 */
const lastGood = new Map<string, { meta: NonNullable<AccountsState["meta"]>; accounts: AccountsState["accounts"]; atMs: number }>();

/**
 * 取那台机器（**本机也一样**）的账号状态（带 TTL 缓存）。force=true 或缓存过期时重发。
 *
 * **走通道，后端出成品**：`chan.call(origin, "accounts-list", {agent}, …)` —— 帧命令直接问那台机器的后端
 * （本机那台由 `<local>` 那条长连接答），应答就是界面要的形状（[`decodeAccountsList`] 按形状收，不解释）。
 * 在此之前是两条 Tauri 命令（远端 `list_remote_accounts` 走帧面、本机 `list_local_accounts` 每次 exec 一次性后端），
 * monitor 在两条路上各把行解析一遍、本机那条另并一次 apikey 表 —— 那一份解释与并表整个挪进了后端
 * （`observe/accounts_query.rs::list_product`，规则住 `acct-core`），远端从此也并上**它自己**那份表。
 *
 * `agent`：这次起会话的是哪一家（适配器 id）。并表只认那一家的行（条 49），后端不猜 ⇒ 由这里带过去。
 * 失败（没有控制通道 / 后端不认 / 对端说不行 / 期限到 / 形状对不上）⇒ `available:false` ＋ 一句人话（通道那一层的说法
 * 住 `ipc/chan-caller.ts::saidFrom`，不在账号面再写一份）；「不可用」不是错误（前端据此降级，不弹错）。
 */
export async function fetchAccounts(origin: Origin, force = false): Promise<AccountsState> {
  const now = Date.now();
  const cached = accountsCache.get(origin);
  if (!force && cached && now - cached.at < ACCOUNTS_TTL_MS) return cached.value;

  let state: AccountsState;
  try {
    const body = jsonBody({ agent: launchAgentId() });
    const budget = budgetWithin(ACCOUNTS_BUDGET_MS);
    const reply = await chan.call(origin, "accounts-list", body, budget);
    const got = decodeAccountsList(readJson(reply));
    state = {
      origin,
      available: true,
      error: null,
      oldBackend: false,
      meta: got.meta,
      accounts: got.accounts,
      // Z01：后端说的「能用但有缺」（manifest 里没有账号 0）。
      notice: got.notice,
    };
    if (got.meta !== null) lastGood.set(origin, { meta: got.meta, accounts: got.accounts, atMs: now });
  } catch (e) {
    state = {
      origin,
      available: false,
      error: saidFrom(e, origin),
      oldBackend: isOldBackend(e),
      meta: null,
      accounts: [],
      notice: null,
      last: lastGood.get(origin) ?? null,
    };
  }
  accountsCache.set(origin, { at: now, value: state });
  putAccounts(origin, state); // 每台的账号快照只住 store 一处，读者订阅它
  return state;
}

/**
 * 账号清单那一问的期限：30 秒 —— 与它上一个住址（monitor 侧 `frame_query` 的 `LINES_BUDGET`）同值：
 * 盖的是「那台后端读一份 manifest ＋ stat 各账号目录 ＋ 读一份 apikey 表 ＋ 回程」，不含握手（长连接早就连着）。
 */
const ACCOUNTS_BUDGET_MS = 30_000;
/** 信任预检那一问的期限：30 秒 —— 与它上一个住址（逐次拨号那条的 `LIST_TIMEOUT`）同值。 */
const TRUST_BUDGET_MS = 30_000;

/**
 * 账号库那一家（适配器 id，后端并 apikey 表时认它）：新会话默认起的那一家 —— 账号 chip · 设置页 · 「新会话默认」说的都是它。
 * **值从后端来**：生成物里的 `DEFAULT_AGENT`（注册表里声明默认的那一家）与那一行的 `adapterId`（后端 `src/backend/agents/<名>/` 注册表 `Adapter.launch` 那张表，从前是 monitor `adapter.rs`），与 monitor 侧 `history::launch_agent_id` 〔散文墓碑〕
 * 同一个事实（起会话那一侧写进中转路由键第 1 段的就是它）。表里没有 ⇒ 抛（不回落到任何一家）。
 */
export function launchAgentId(): string {
  const got = lookupAgentProfile(DEFAULT_AGENT);
  if (!got.known) throw new Error(got.message);
  return got.facts.adapterId;
}

/** 账号页那一家的画像（与 [`launchAgentId`] 同一个出处）：叫法 · 认得的模型。查不到 ⇒ `null`。 */
export function accountsAgentProfile(): AgentProfileRow | null {
  let id: string;
  try {
    id = launchAgentId();
  } catch {
    return null;
  }
  return AGENT_PROFILE_TABLE.find((r) => r.adapterId === id) ?? null;
}

/** 那台机器的那两格事实（`apikey-routing`，本机由 `<local>` 那条长连接答）；`agent` 与账号清单同一个出处（后端不猜是哪一家）。 */
export async function fetchMachineApikeyRouting(origin: Origin, configDirs: string[]): Promise<ApikeyRoutingView> {
  return await fetchApikeyRouting(origin, launchAgentId(), configDirs);
}

/**
 * 取**这台机器**的账号状态 —— 就是 `fetchAccounts(LOCAL_ORIGIN, …)`：本机与远端同一条路、同一个缓存键空间。
 *
 * 〔来历〕L3a 起它是 `fetchAccounts` 的本地对侧（`list_local_accounts`：`N-F1c` 之后 exec 一次本机后端的
 * `--list-accounts`，另在 monitor 里并一次本机 apikey 表）；C4b 把缓存键统一成 `LOCAL_ORIGIN`；C4c 把两条路合成一条
 * （问 `<local>` 那条长连接的 `accounts-list`，并表挪进后端）。名字留着：设置页 / chip / 起会话三处读作「这台机器的账号」。
 */
export async function fetchLocalAccounts(force = false): Promise<AccountsState> {
  return fetchAccounts(LOCAL_ORIGIN, force);
}

/**
 * 那台机器（**本机也一样**）上正在跑的会话各属于哪个账号（带 TTL 缓存；`force` 绕过缓存）。
 *
 * **走通道**：`chan.call(origin, "accounts-sessions", …)` —— 帧命令直接问那台机器的后端，
 * 本机与远端**同一条路**（本机那台由 `<local>` 那条长连接答）。在此之前是两条 Tauri 命令
 * （远端那条走帧面 · 本机那条每次 exec 一个本机后端），
 * 各在 Rust 里把同一种行解析一遍 —— 两条命令与那两份解析一起退役，行的解释只剩 [`parseSessionAccountLines`] 一处。
 *
 * 失败（没有控制通道 / 后端不认 / 对端说不行 / 期限到）一律按「这一次没问出来」：空表、`console.warn`，
 * 不猜 —— 与旧那两条的 `available:false ⇒ 空表` 同形（调用方从来只看行）。
 */
export async function fetchSessionAccounts(
  origin: Origin,
  force = false,
): Promise<SessionAccount[]> {
  const now = Date.now();
  const cached = sessionAccountsCache.get(origin);
  if (!force && cached && now - cached.at < SESSION_ACCOUNTS_TTL_MS) return cached.value;
  try {
    const empty = jsonBody({});
    const budget = budgetWithin(SESSION_ACCOUNTS_BUDGET_MS);
    const reply = await chan.call(origin, "accounts-sessions", empty, budget);
    const value = parseSessionAccountLines(linesOf(reply));
    sessionAccountsCache.set(origin, { at: now, value });
    return value;
  } catch (e) {
    console.warn(`fetchSessionAccounts(${origin}) failed:`, e);
    sessionAccountsCache.set(origin, { at: now, value: [] });
    return [];
  }
}

/**
 * 「会话 ↔ 账号」那一问的期限：30 秒 —— 与它上一个住址（monitor 侧 `frame_query` 的 `LINES_BUDGET`）同值，
 * 它盖的是「那台后端扫一遍活会话 ＋ 回程」，不含握手（长连接早就连着）。
 */
const SESSION_ACCOUNTS_BUDGET_MS = 30_000;

const optStr = (v: unknown): v is string | null | undefined =>
  v === undefined || v === null || typeof v === "string";
const optBool = (v: unknown): v is boolean | undefined => v === undefined || typeof v === "boolean";
const optBoolOrNull = (v: unknown): v is boolean | null | undefined => v === null || optBool(v);

/**
 * `accounts-sessions`（= `--session-accounts`）的逐行 ⇒ [`SessionAccount`]。**坏行跳过**（`console.warn`），不毁整次。
 *
 * 口径逐格照它上一个住址（Rust `accounts.rs::SessionAccount` 的 serde：`camelCase`、`pid` 必有、
 * 其余可缺；`bare` / `alive` 缺 ⇒ `false`）：
 * - 某一格**在但类型不对** ⇒ 整行坏（serde 同样整行拒）—— 不替后端猜一个值。
 */
export function parseSessionAccountLines(lines: string[]): SessionAccount[] {
  const out: SessionAccount[] = [];
  for (const line of lines) {
    let v: unknown;
    try {
      v = JSON.parse(line);
    } catch (e) {
      console.warn("session-accounts 行解析失败（跳过）:", e);
      continue;
    }
    if (v === null || typeof v !== "object" || Array.isArray(v)) {
      console.warn("session-accounts 行不是对象（跳过）");
      continue;
    }
    const o = v as Record<string, unknown>;
    if (
      typeof o.pid !== "number" ||
      !Number.isInteger(o.pid) ||
      o.pid < 0 ||
      !optStr(o.sessionId) ||
      !optStr(o.cwd) ||
      !optStr(o.configDir) ||
      !optStr(o.account) ||
      !optBool(o.bare) ||
      !optBool(o.alive) ||
      !optBoolOrNull(o.viaRelay)
    ) {
      console.warn("session-accounts 行字段类型不对（跳过）");
      continue;
    }
    out.push({
      pid: o.pid,
      sessionId: o.sessionId ?? null,
      cwd: o.cwd ?? null,
      configDir: o.configDir ?? null,
      account: o.account ?? null,
      bare: o.bare ?? false,
      alive: o.alive ?? false,
      viaRelay: o.viaRelay ?? null,
    });
  }
  return out;
}

/** 换号前的目录信任预检（A5 会用；A3 先提供）。available:false → 视为"未知"，调用方只警告不拦。 */
export interface TrustResult {
  available: boolean;
  trusted: boolean;
  known: boolean;
  error: string | null;
}
/**
 * Z01：`configDir` 传 `null` = 问账号 0（它的 `.claude.json` 在那台机器的 `$HOME`）。**绝不传空串**——那会被后端判成不安全路径拒掉。
 *
 * **走通道**：`chan.call(origin, "accounts-trust", {configDir, cwd}, …)`，本机与远端同一条路。
 * 在此之前是 Tauri 命令 `check_account_trust`：远端每问一次经本机后端开一条链路、在那台 exec 一次后端（最后两条仍逐次拨号的
 * 子命令），本机每问一次 exec 一次性本机后端。失败一律 `available:false` ＋ 一句人话（调用方按「未知信任状态」只警告不拦）。
 */
export async function checkTrust(
  origin: Origin,
  configDir: string | null,
  cwd: string,
): Promise<TrustResult> {
  try {
    const body = jsonBody({ configDir, cwd });
    const budget = budgetWithin(TRUST_BUDGET_MS);
    const reply = await chan.call(origin, "accounts-trust", body, budget);
    return { available: true, error: null, ...decodeTrust(readJson(reply)) };
  } catch (e) {
    return {
      available: false,
      trusted: false,
      known: false,
      error: saidFrom(e, origin),
    };
  }
}

/** 手动刷新：清某台（或全部）缓存，下次 fetch 必重发。 */
export function invalidateAccountsCache(origin?: string): void {
  if (origin) {
    accountsCache.delete(origin);
    sessionAccountsCache.delete(origin);
  } else {
    accountsCache.clear();
    sessionAccountsCache.clear();
  }
}

/** 测试专用：清空所有内存缓存。 */
export function __resetAccountsCacheForTest(): void {
  accountsCache.clear();
  lastGood.clear();
  sessionAccountsCache.clear();
}
