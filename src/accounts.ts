// A3：多账号（cc-acct-iso）前端 store —— 账号模型的**单一真相**。
//
// 账号 = 一个 CLAUDE_CONFIG_DIR。本模块：
//   1. 包装 A2 的三个只读 Tauri 命令（list_remote_accounts / list_remote_session_accounts /
//      check_account_trust），带 per-origin TTL 缓存 + 手动刷新。
//   2. 存/取"我这台 cc-monitor 起新会话时用哪个账号"（config.json 的 accounts.defaultName）。
//   3. 提供纯函数（降级判定 / 会话徽章映射）供 UI 与 vitest。
//
// **不注入 env（A4）、不重启会话（A5）、不碰本地账号（A7）**。全程走 A2 的
// available:false 降级：未迁移 / 旧 daemon / daemonless 一律安静隐藏账号 UI，不报错。
import { invoke } from "@tauri-apps/api/core";
import { loadConfig, saveConfig } from "./config";

// ---- 对齐 A2（src-tauri/src/accounts.rs）的返回结构 ----
export interface Account {
  name: string;
  email: string;
  configDir: string;
  isDefault: boolean;
  /** "isolated"（正常）/ "in-place"（逃生口，不支持切换）。 */
  mode: string;
  exists: boolean;
  /** 仅 stat .credentials.json 存在性——不代表凭据有效。 */
  loggedIn: boolean;
}

export interface AccountsMeta {
  enabled: boolean;
  acctsDir: string;
  manifestPath: string;
  updatedAt: string | null;
  sharedStore: string | null;
  count: number;
  error: string | null;
}

interface RawAccountsResult {
  available: boolean;
  error: string | null;
  meta: AccountsMeta | null;
  accounts: Account[];
}

export interface SessionAccount {
  pid: number;
  sessionId: string | null;
  cwd: string | null;
  configDir: string | null;
  /** configDir 反查得到的账号名；null = 不知道（不猜）。 */
  account: string | null;
  bare: boolean;
  alive: boolean;
}

interface RawSessionAccountsResult {
  available: boolean;
  error: string | null;
  sessions: SessionAccount[];
}

/** 账号功能在某台远端的整体状态（UI 直接消费）。 */
export interface AccountsState {
  origin: string;
  available: boolean;
  error: string | null;
  meta: AccountsMeta | null;
  accounts: Account[];
  /** 本机选择的默认账号（config.json）；缺省跟随 manifest 的 isDefault。 */
  defaultName: string | null;
}

/** chip / 设置组据此决定怎么显示。纯派生自 AccountsState。 */
export type AccountsUi =
  | { kind: "hidden"; reason: string } // daemonless：完全不显示账号 UI
  | { kind: "needs-update"; reason: string } // 旧 daemon
  | { kind: "not-enabled"; manifestPath: string | null; reason: string } // 未迁移/无账号
  | { kind: "ready"; accounts: Account[]; defaultName: string | null };

// ------------------------------------------------------------ 纯函数

/** A2 返回 → UI 判定（DESIGN §7 降级矩阵）。纯函数，vitest 锁死。 */
export function deriveUi(state: AccountsState): AccountsUi {
  if (!state.available) {
    const e = state.error ?? "";
    if (e.includes("daemonless")) return { kind: "hidden", reason: e };
    if (e.includes("过旧") || e.includes("不支持账号")) {
      return { kind: "needs-update", reason: e || "远端 daemon 需要更新" };
    }
    // 其它不可用（查询失败等）：当作"需更新/不可用"，可点开设置看原因
    return { kind: "needs-update", reason: e || "账号功能暂不可用" };
  }
  if (!state.meta?.enabled || state.accounts.length === 0) {
    return {
      kind: "not-enabled",
      manifestPath: state.meta?.manifestPath ?? null,
      reason: state.meta?.error ?? "该远端尚未启用多账号",
    };
  }
  return { kind: "ready", accounts: state.accounts, defaultName: state.defaultName };
}

/** 当前生效的默认账号名：本机 defaultName 优先，否则取 manifest isDefault，再否则第一个。 */
export function effectiveDefault(state: AccountsState): Account | null {
  if (state.accounts.length === 0) return null;
  if (state.defaultName) {
    const hit = state.accounts.find((a) => a.name === state.defaultName);
    if (hit) return hit;
  }
  return state.accounts.find((a) => a.isDefault) ?? state.accounts[0];
}

/**
 * 「当前工作账号」(account-ux)——`effectiveDefault` 的语义别名,值完全一致。
 * account-isolation 时期它只用来预选新会话对话框;本轮升格为 resume/新会话的**跟随默认**。
 * 换名不换存储(仍 config.json `accounts.defaultName`);给别名是让 follow 解析 / mismatch
 * 比对的调用点读作"当前工作账号"而非"默认",避免理解漂移。
 */
export function currentWorkingAccount(state: AccountsState): Account | null {
  return effectiveDefault(state);
}

/** 某账号是否可被选为默认 / 用来起会话。 */
export function isSelectable(a: Account): boolean {
  return a.mode === "isolated" && a.loggedIn && a.exists;
}

/**
 * account-ux U1:普通 resume 的**跟随账号**解析器(纯函数,vitest 锁死)。
 * 优先级(用户拍板:粘性优先):`会话 lastAccount → 当前工作账号 → null(基座)`。
 * 每级候选必须 `isSelectable`(存在的 isolated + 已登录 + 目录在)否则**下沉**下一级;
 * 都不可选 → null(=不注入、落基座、逐字节旧行为)。
 * **显式选号不走此函数**——那条路维持 A4 语义(withAccount 的非空 accountName 分支)。
 */
export function resolveFollowAccount(
  state: AccountsState,
  opts: { lastAccount?: string | null; current?: string | null },
): string | null {
  const pickable = (name: string | null | undefined): name is string => {
    if (!name) return false;
    const a = state.accounts.find((x) => x.name === name);
    return !!a && isSelectable(a);
  };
  if (pickable(opts.lastAccount)) return opts.lastAccount;
  if (pickable(opts.current)) return opts.current;
  return null;
}

/**
 * account-ux U1:活会话账号是否与当前工作账号**不一致**(纯函数)。
 * 仅当两者都确知且不同才判 true;任一未知(live 探不到 / 无当前账号)→ false(不误报)。
 */
export function detectAccountMismatch(
  liveAccount: string | null,
  current: string | null,
): boolean {
  return liveAccount !== null && current !== null && liveAccount !== current;
}

/**
 * A4：账号名 → 该账号的 CLAUDE_CONFIG_DIR（用来带账号 resume/起会话）。
 * 仅当账号存在且**可选**（isolated + 已登录 + 目录在）才给；否则 null（不可选的绝不注入）。
 */
export function accountConfigDir(state: AccountsState, name: string): string | null {
  const acc = state.accounts.find((a) => a.name === name);
  if (!acc || !isSelectable(acc)) return null;
  return acc.configDir || null;
}

/** 账号徽章文本（tab 行用）：账号名首字符（ASCII 取前 2，其它取 1 个 code point）。 */
export function badgeText(name: string): string {
  const cps = Array.from(name);
  if (cps.length === 0) return "?";
  if (/^[A-Za-z0-9]/.test(name)) return name.slice(0, 2);
  return cps[0];
}

/**
 * 会话 sid → 徽章信息（DESIGN §3 三源优先级）：
 *   源① live 探测（`/proc/<pid>/environ` 硬真相）——优先；
 *   源② lastAccount（history-metadata：上次用本工具带账号起该会话时记的）——探测不到时兜底，标"上次"；
 *   源③ 都无 → `—`、不猜。
 * 本地会话（origin 为 null）不产徽章。
 */
export interface SessionBadge {
  text: string; // 显示文本；"—" = 未知
  known: boolean; // 是否确知账号
  tooltip: string;
  /** account-ux U5:徽章数据来源。'live'=实时探测(硬真相,实心头像)/ 'last'=上次记录(源②,幽灵头像)/ 'unknown'=不猜。 */
  source: "live" | "last" | "unknown";
  /** account-ux U5:确知时的账号名(text 是缩写,这里是全名,供 mismatch 比对/tooltip);未知为 null。 */
  account: string | null;
}
export function sessionBadge(
  sid: string,
  origin: string | null,
  liveByS: Map<string, SessionAccount>,
  emailByName: Map<string, string>,
  lastAccountByS?: Map<string, string>,
): SessionBadge | null {
  if (origin === null) return null; // 本地会话 A7 前不支持
  // 源①：live 探测——唯一硬真相，优先。
  const live = liveByS.get(sid);
  if (live && live.alive && live.account) {
    const email = emailByName.get(live.account) ?? "";
    return {
      text: badgeText(live.account),
      known: true,
      tooltip: `账号 ${live.account}${email ? ` · ${email}` : ""} · 来源：实时探测`,
      source: "live",
      account: live.account,
    };
  }
  // 源②：cc-monitor 记的 lastAccount（归档/未在跑的会话探测不到 live，用它兜底）；
  // 标"上次用本工具起"避免误当成当前实时。
  const last = lastAccountByS?.get(sid);
  if (last) {
    const email = emailByName.get(last) ?? "";
    return {
      text: badgeText(last),
      known: true,
      tooltip: `账号 ${last}${email ? ` · ${email}` : ""} · 来源：上次用本工具起`,
      source: "last",
      account: last,
    };
  }
  // 源③：都没有 → 未知，不猜。
  return {
    text: "—",
    known: false,
    tooltip: "该会话不是本工具启动的，或已停止，无法判定账号",
    source: "unknown",
    account: null,
  };
}

/**
 * A4/§7 降级：某会话是否**该显**账号徽章。只有「账号可查询」的远端才显（即 available 且非
 * daemonless 的 origin,由 main.ts 收进 readyOrigins）。本地会话（origin null）与不可查询的远端
 * （daemonless / 未迁移 / 旧 daemon）一律不显——否则满屏 `—` 是噪音、违反 §7「不可用即安静隐藏」。
 */
export function shouldShowAccountBadge(
  origin: string | null,
  readyOrigins: Set<string>,
): boolean {
  if (origin === null) return false; // 本地会话 A7 前不支持
  return readyOrigins.has(origin);
}

// ------------------------------------------------------------ config.json

const CFG_KEY = "accounts";

/** 读本机默认账号名（config.json accounts.defaultName）。无则 null。 */
export async function getDefaultName(): Promise<string | null> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    if (a && typeof a === "object") {
      const dn = (a as Record<string, unknown>).defaultName;
      if (typeof dn === "string" && dn) return dn;
    }
  } catch (e) {
    console.warn("getDefaultName failed:", e);
  }
  return null;
}

/** 写本机默认账号名。null = 清除（回退跟随 manifest）。枚举全字段写回，防静默丢失。 */
export async function setDefaultName(name: string | null): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const prev =
    cfg[CFG_KEY] && typeof cfg[CFG_KEY] === "object"
      ? (cfg[CFG_KEY] as Record<string, unknown>)
      : {};
  cfg[CFG_KEY] = {
    ...prev,
    defaultName: name ?? undefined,
  };
  // undefined 键会被 serde_json 序列化时忽略——等效于删除
  if (name === null) delete (cfg[CFG_KEY] as Record<string, unknown>).defaultName;
  await saveConfig(cfg);
}

// ------------------------------------------------------------ 带缓存的取数

const ACCOUNTS_TTL_MS = 30_000; // 账号列表极少变（迁移/登录才变），缓存久一点省 SSH
const SESSION_ACCOUNTS_TTL_MS = 8_000; // 会话账号归属随起停变，照 tabs.ts tmuxCache 的 8s
interface CacheEntry<T> {
  at: number;
  value: T;
}
const accountsCache = new Map<string, CacheEntry<AccountsState>>();
const sessionAccountsCache = new Map<string, CacheEntry<SessionAccount[]>>();

/** 取某台远端的账号状态（带 TTL 缓存）。force=true 或缓存过期时重发。 */
export async function fetchAccounts(origin: string, force = false): Promise<AccountsState> {
  const now = Date.now();
  const cached = accountsCache.get(origin);
  if (!force && cached && now - cached.at < ACCOUNTS_TTL_MS) return cached.value;

  let raw: RawAccountsResult;
  try {
    raw = await invoke<RawAccountsResult>("list_remote_accounts", { origin });
  } catch (e) {
    // Rust 侧只有"该远端根本没配"才 Err；当作不可用而非崩溃
    const state: AccountsState = {
      origin,
      available: false,
      error: String(e),
      meta: null,
      accounts: [],
      defaultName: null,
    };
    accountsCache.set(origin, { at: now, value: state });
    return state;
  }
  const defaultName = await getDefaultName();
  const state: AccountsState = {
    origin,
    available: raw.available,
    error: raw.error,
    meta: raw.meta,
    accounts: raw.accounts ?? [],
    defaultName,
  };
  accountsCache.set(origin, { at: now, value: state });
  return state;
}

/** 取某台远端正在跑的会话账号归属（带 TTL 缓存）。 */
export async function fetchSessionAccounts(
  origin: string,
  force = false,
): Promise<SessionAccount[]> {
  const now = Date.now();
  const cached = sessionAccountsCache.get(origin);
  if (!force && cached && now - cached.at < SESSION_ACCOUNTS_TTL_MS) return cached.value;
  try {
    const raw = await invoke<RawSessionAccountsResult>("list_remote_session_accounts", { origin });
    const value = raw.available ? (raw.sessions ?? []) : [];
    sessionAccountsCache.set(origin, { at: now, value });
    return value;
  } catch (e) {
    console.warn(`fetchSessionAccounts(${origin}) failed:`, e);
    sessionAccountsCache.set(origin, { at: now, value: [] });
    return [];
  }
}

/** 换号前的目录信任预检（A5 会用；A3 先提供）。available:false → 视为"未知"，调用方只警告不拦。 */
export interface TrustResult {
  available: boolean;
  trusted: boolean;
  known: boolean;
  error: string | null;
}
export async function checkTrust(
  origin: string,
  configDir: string,
  cwd: string,
): Promise<TrustResult> {
  try {
    return await invoke<TrustResult>("check_account_trust", { origin, configDir, cwd });
  } catch (e) {
    return { available: false, trusted: false, known: false, error: String(e) };
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
  sessionAccountsCache.clear();
}

/**
 * A4：记录「这个会话上次用账号 X 起」到 history-metadata（源②，DESIGN §3）。history / tabs
 * 两处「带账号 resume」共用。失败静默——记忆是非关键路径，不该挡住 resume 本身。
 */
export async function recordLastAccount(sessionId: string, account: string): Promise<void> {
  try {
    await invoke("update_history_metadata", { sessionId, patch: { lastAccount: account } });
  } catch (e) {
    console.warn("record lastAccount failed:", e);
  }
}

/**
 * A4：**统一「带账号起会话」编排**——history resume / tabs resume / 「开新 Claude」对话框
 * 三站点共用，消除各写一遍 `resolve configDir + record lastAccount` 的漂移（DESIGN §4）。
 * A5「换号重启」是本编排的超集（在 run 前插 checkTrust/compact、run 后同样 record），届时在此扩展。
 *
 *   - `accountName == null` **且无 `opts.follow`** → 默认起：`run(undefined)`（不注入、不记账、不 fetch，A4 逐字节旧行为）。
 *   - `accountName == null` **且有 `opts.follow`**（account-ux U2 opt-in 跟随）→ `fetchAccounts` 后
 *     经 `resolveFollowAccount`（lastAccount → 当前工作账号 → null）解析：命中则注入其 configDir +（给了
 *     sessionId 时）记 lastAccount（会话账号 sticky 自增强）；解析不到 → `run(undefined)` 落基座。
 *     **下沉静默不 `onUnselectable`**（用户没显式点号，不该弹提示）。
 *   - `accountName` 非空 → `fetchAccounts` 解析 configDir：
 *       · 解析不到（不可选 / 账号库不可用）→ `onUnselectable(name)`（调用方 toast）后**退化为默认起**；
 *       · 解析到 → `run(configDir)`；再在**给了 sessionId 时**记 lastAccount（源②，新会话无 sid 不记）。
 * `run` 内部的拉起失败由 run 自己处理（runRemote* 有复制命令回退）；本编排只统一 resolve/record 口径。
 */
export async function withAccount(
  origin: string,
  accountName: string | null,
  run: (configDir?: string) => Promise<void>,
  opts: {
    sessionId?: string;
    onUnselectable?: (name: string) => void;
    /** account-ux U2:仅当 accountName===null 时生效——启用「跟随」解析(lastAccount→当前工作账号→基座)。 */
    follow?: { lastAccount?: string | null };
  } = {},
): Promise<void> {
  let configDir: string | undefined;
  let recordName: string | null = null; // 成功注入后要记的账号名(显式=accountName / 跟随=解析名)
  if (accountName) {
    // 显式选号(A4 语义不变)
    try {
      const state = await fetchAccounts(origin);
      configDir = accountConfigDir(state, accountName) ?? undefined;
    } catch {
      configDir = undefined; // 账号库拿不到 → 退化默认起（fetchAccounts 通常不抛，防御性兜底）
    }
    if (!configDir) opts.onUnselectable?.(accountName);
    else recordName = accountName;
  } else if (opts.follow) {
    // account-ux U2:跟随模式(opt-in)。accountName===null 且**无** follow 的老调用不进此分支,
    // 逐字节旧行为(不 fetch、落基座)。下沉静默不 toast(用户没显式点号)。
    try {
      const state = await fetchAccounts(origin);
      const current = currentWorkingAccount(state)?.name ?? null;
      const followName = resolveFollowAccount(state, {
        lastAccount: opts.follow.lastAccount,
        current,
      });
      if (followName) {
        configDir = accountConfigDir(state, followName) ?? undefined;
        if (configDir) recordName = followName;
      }
    } catch {
      configDir = undefined; // 库不可用 → 基座
    }
  }
  await run(configDir);
  if (recordName && configDir && opts.sessionId) {
    void recordLastAccount(opts.sessionId, recordName);
  }
}
