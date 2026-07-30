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
import { isValidModelName } from "./shell-quote";
import type { LaunchModifiers } from "./launch-plan";

// ---- 对齐 A2（src-tauri/src/accounts.rs）的返回结构 ----
export interface Account {
  name: string;
  email: string;
  /**
   * Z01：**可以是 null** —— 那就是账号 0（「不设 CLAUDE_CONFIG_DIR」这个状态本身）。
   * 起它 = **什么都不设**（不是设成空串：空值 ≠ 未设）。
   * 消费侧一律 `?? fallback`，**绝不 `|| ""` 后拼进命令行**。
   */
  configDir: string | null;
  isDefault: boolean;
  /** "isolated"（正常）/ "in-place"（逃生口，不支持切换）/ "bare"（账号 0）。 */
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
  /** Z01：远端 daemon 认不认「configDir 缺席 = 账号 0」。旧 daemon 不出这个键 ⇒ undefined。 */
  accountZeroAware?: boolean;
}

interface RawAccountsResult {
  available: boolean;
  error: string | null;
  meta: AccountsMeta | null;
  accounts: Account[];
  /** Z01：能用但有缺（远端版本旧到看不见账号 0）时的人话说明。 */
  notice?: string | null;
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
  /**
   * Z01：**能用但有缺**时的人话说明（`available` 仍是 true）。null = 无缺。
   * 「绝不静默降级」是它存在的全部理由——旧 daemon / 旧 cc-acct-iso 会让账号 0
   * 从列表里凭空少一行，用户看不出区别。
   */
  notice: string | null;
}

/** chip / 设置组据此决定怎么显示。纯派生自 AccountsState。 */
export type AccountsUi =
  | { kind: "hidden"; reason: string } // daemonless：完全不显示账号 UI
  | { kind: "needs-update"; reason: string } // 旧 daemon
  | { kind: "not-enabled"; manifestPath: string | null; reason: string } // 未迁移/无账号
  | { kind: "ready"; accounts: Account[]; defaultName: string | null; notice: string | null };

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
  return {
    kind: "ready",
    accounts: state.accounts,
    defaultName: state.defaultName,
    notice: state.notice,
  };
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
 * 「当前账号」(account-ux)——`effectiveDefault` 的语义别名,值完全一致。
 * account-isolation 时期它只用来预选新会话对话框;本轮升格为 resume/新会话的**跟随默认**。
 * 换名不换存储(仍 config.json `accounts.defaultName`);给别名是让 follow 解析 / mismatch
 * 比对的调用点读作"当前账号"而非"默认",避免理解漂移。
 */
export function currentWorkingAccount(state: AccountsState): Account | null {
  return effectiveDefault(state);
}

/** 某账号是否可被选为默认 / 用来起会话。 */
export function isSelectable(a: Account): boolean {
  // Z01：账号 0（mode "bare"）在这里**天然落选**，这正是当前想要的——从 UI 起它需要
  // 「显式 unset CLAUDE_CONFIG_DIR」这条注入路径，而 launch-plan 今天只会 export。
  // 若哪天放开，必须**同时**给出 unset 的注入形态：只是「不注入」是错的，远端 rc 里
  // 那句 `export CLAUDE_CONFIG_DIR=<默认账号>` 会让它落到别的账号上（静默串号）。
  return a.mode === "isolated" && a.loggedIn && a.exists;
}

/** account-ux U8：可选账号列表（`isSelectable` 过滤）。休眠判据 / 计数一律走它，别各处再 filter 一遍。 */
export function selectableAccounts(state: AccountsState): Account[] {
  return state.accounts.filter(isSelectable);
}

/**
 * account-ux U8：**账号色系统是否该激活**（休眠固化）。
 *
 * 只有一个可选账号时，彩色头像不携带任何信息——它区分不了任何东西，纯属噪音；等用户加了
 * 第二个号，颜色才开始有意义。故 `≥2 个可选账号 && 该 origin 账号确实可查询` 才激活。
 *
 * **作用面只有状态栏 chip 与 tab 徽章**。设置里的账号表（U7 的横幅 + 表格行）**恒显豁免**：
 * 那是全应用唯一能让用户学到「色块 ↔ 账号 ↔ 邮箱」映射的图例面，单账号期把它也休眠掉，
 * 等加了第二个号就会突然满屏彩块。（MASTERPLAN 变更记录 2026-07-25 已拍板。）
 */
export function accountColorsActive(state: AccountsState): boolean {
  return state.available && selectableAccounts(state).length >= 2;
}

/**
 * account-ux U6：**真正可用**的当前账号——`currentWorkingAccount` 再过一道 `isSelectable`。
 *
 * `currentWorkingAccount`(=`effectiveDefault`) 只挑"被指定/第一个"，不管它能不能用；而拿未过滤
 * 的值去判"不一致"，会让徽章指着一个系统自己永远不会 follow 过去的账号说"你不一致"。故账号
 * 徽章的 mismatch 判定统一用这个（F09 后：⚠k/⇄/批量对齐已删除，本函数现在只喂徽章
 * `tabs.ts::updateAccountBadge` 一处消费者）。与 U1 `resolveFollowAccount`「每级候选不可选就
 * 下沉」同一套语义。
 */
export function currentAccountForBadge(state: AccountsState): Account | null {
  const cur = currentWorkingAccount(state);
  return cur && isSelectable(cur) ? cur : null;
}

/**
 * account-ux U1:普通 resume 的**跟随账号**解析器(纯函数,vitest 锁死)。
 * 优先级(用户拍板:粘性优先):`会话 lastAccount → 当前账号 → null(基座)`。
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
 * account-ux U1:活会话账号是否与当前账号**不一致**(纯函数)。
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

/**
 * Z01：这个账号是不是账号 0（「不设 CLAUDE_CONFIG_DIR」这个状态本身）。
 *
 * 判据是**结构性**的（`configDir` 缺席），**不认名字**——manifest 想把它叫什么都行，
 * 前端不硬编码 "0"。空串**不算**：那是非法拼法，daemon 侧已挡掉。
 */
export function isAccountZero(a: Account): boolean {
  return a.configDir === null || a.configDir === undefined;
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

const MODEL_MAP_KEY = "modelByAccount";

/** F07：读某账号配置的默认模型偏好（config.json accounts.modelByAccount[name]）。无则 undefined。
 *  结构上是 `defaultName`（单值）的复数版——按账号名索引，同样是本机、不跨机器同步的偏好。 */
export async function getModelForAccount(name: string): Promise<string | undefined> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    if (a && typeof a === "object") {
      const map = (a as Record<string, unknown>)[MODEL_MAP_KEY];
      if (map && typeof map === "object") {
        const v = (map as Record<string, unknown>)[name];
        if (typeof v === "string" && v) return v;
      }
    }
  } catch (e) {
    console.warn("getModelForAccount failed:", e);
  }
  return undefined;
}

/** 写某账号的模型偏好。`model === null` 清除该账号这一条（其余账号不受影响）。
 *
 *  Phase D 审计发现的阻塞项：校验必须在**写入点**做，不能只留给
 *  `MODEL_DIMENSION.apply()`（起会话时）——那样一个非法值一旦落盘，会让该账号**此后每一次**
 *  resume/新建/tmux resume 在 `buildLaunchPlan` 里统一 throw，用户只看到一堆"无法构造 resume
 *  命令"的 toast，且设置面板的输入框不会标出"当前值非法"，很难把两者联系起来。fail-closed：
 *  非法即 throw，调用方（UI）负责 catch 并提示，绝不静默落盘。 */
export async function setModelForAccount(name: string, model: string | null): Promise<void> {
  if (model && !isValidModelName(model)) {
    throw new Error(`非法模型名（拒绝保存）: ${JSON.stringify(model)}`);
  }
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const prev =
    cfg[CFG_KEY] && typeof cfg[CFG_KEY] === "object"
      ? (cfg[CFG_KEY] as Record<string, unknown>)
      : {};
  const prevMap =
    prev[MODEL_MAP_KEY] && typeof prev[MODEL_MAP_KEY] === "object"
      ? (prev[MODEL_MAP_KEY] as Record<string, string>)
      : {};
  const nextMap = { ...prevMap };
  if (model) nextMap[name] = model;
  else delete nextMap[name];
  cfg[CFG_KEY] = { ...prev, [MODEL_MAP_KEY]: nextMap };
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
      notice: null,
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
    // Z01：后端算好的降级说明（旧 daemon / 旧 cc-acct-iso ⇒ 列表里少了账号 0）。
    notice: raw.notice ?? null,
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
/**
 * Z01：`configDir` 传 `null` = 问账号 0（后端走 `--account-trust-zero`，它的
 * `.claude.json` 在 `$HOME`）。**绝不传空串**——那会被 daemon 判成不安全路径拒掉。
 */
export async function checkTrust(
  origin: string,
  configDir: string | null,
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
 * F05：判别联合形态的账号解析结果——`AccountResolver` 目标（MASTERPLAN §3 账本）。取代
 * "只吐 configDir、名字在解析完就被丢弃"的旧口径：`kind==="account"` 时同时带 `name` 和
 * `configDir`——线通给调用方后，`name` 才能继续往下传进 `LaunchContext`（F05 的核心交付：
 * 让 `ACCOUNT_DIMENSION.cliFlags` 吐得出 `--account <名>`）。
 */
export type AccountResolution =
  | { kind: "account"; name: string; configDir: string }
  | { kind: "base" }
  | { kind: "unavailable"; requestedName?: string };

/**
 * F05：纯函数——从 `withAccount` 原内联逻辑抽出（显式选号 / 跟随解析两分支），决策逻辑本身
 * 逐字节不变，只是从"直接算出 configDir 就地用"变成"先返回一个自描述的判别联合"。
 * `opts.explicit` 非空 → 显式选号；命中 `isSelectable` → `account`，否则 → `unavailable`。
 * 否则若 `opts.follow` 存在 → `resolveFollowAccount`（lastAccount→当前账号→都不可选）解析：
 * 命中 → `account`；都不可选 → `base`（跟随下沉是静默语义，不是"不可用"，故不用 `unavailable`）。
 * 两者都不满足（无 accountName 也无 follow）→ `base`（今天的"默认起"逐字节旧行为）。
 */
export function resolveAccount(
  state: AccountsState,
  opts: { explicit?: string | null; follow?: { lastAccount?: string | null } },
): AccountResolution {
  if (opts.explicit) {
    const configDir = accountConfigDir(state, opts.explicit);
    return configDir
      ? { kind: "account", name: opts.explicit, configDir }
      : { kind: "unavailable", requestedName: opts.explicit };
  }
  if (opts.follow) {
    const current = currentWorkingAccount(state)?.name ?? null;
    const priorPin = opts.follow.lastAccount ?? null;
    const followName = resolveFollowAccount(state, { lastAccount: priorPin, current });
    if (followName) {
      const configDir = accountConfigDir(state, followName);
      if (configDir) return { kind: "account", name: followName, configDir };
    }
    return { kind: "base" };
  }
  return { kind: "base" };
}

/**
 * A4：**统一「带账号起会话」编排**——history resume / tabs resume / 「开新 Claude」对话框
 * 三站点共用，消除各写一遍 `resolve configDir + record lastAccount` 的漂移（DESIGN §4）。
 * A5「换号重启」是本编排的超集（在 run 前插 checkTrust/compact、run 后同样 record），届时在此扩展。
 * F05：内部改用 `resolveAccount` 求解（行为逐字节不变，见其头注）；`run` 回调新增第二参数
 * `accountName`——命中账号时非空，否则 `undefined`（F05 交付）。
 * F07：模型偏好 `modelOverride`——命中账号时查一次 `getModelForAccount`（本机 config.json 偏好）。
 * **R03**：这三者不再是三个位置参数，统一收进 `LaunchModifiers` 一次交给 `run`。
 *
 *   - `accountName == null` **且无 `opts.follow`** → 默认起：`run({ 三字段皆 undefined })`（不注入、不记账、不 fetch，A4 逐字节旧行为）。
 *   - `accountName == null` **且有 `opts.follow`**（account-ux U2 opt-in 跟随）→ `fetchAccounts` 后
 *     经 `resolveFollowAccount`（lastAccount → 当前账号 → null）解析：命中则注入其 configDir +（给了
 *     sessionId 时）记 lastAccount（会话账号 sticky 自增强）；解析不到 → `run({ 三字段皆 undefined })` 落基座。
 *     **下沉静默不 `onUnselectable`**（用户没显式点号，不该弹提示）。
 *   - `accountName` 非空 → `fetchAccounts` 解析 configDir：
 *       · 解析不到（不可选 / 账号库不可用）→ `onUnselectable(name)`（调用方 toast）后**退化为默认起**；
 *       · 解析到 → `run({ configDir, accountName, modelOverride })`；再在**给了 sessionId 时**记 lastAccount（源②，新会话无 sid 不记）。
 * `run` 内部的拉起失败由 run 自己处理（runRemote* 有复制命令回退）；本编排只统一 resolve/record 口径。
 */
export async function withAccount(
  origin: string,
  accountName: string | null,
  /** R03：收 `LaunchModifiers` 而非三个位置参数。这里曾是**整条位置参数长列车的车头**——
   *  F05 加 `accountName`、F07 再加 `modelOverride`，每次都要同时改这个签名与全部调用点，
   *  于是 MASTERPLAN §0.1 成功标准②（加维度零改调用点）永远差最后一层。收成 bag 后，
   *  加第 4 个维度只需本函数内部往 `mods` 里多塞一个字段，**6 个**调用点一个字符都不用改
   *  （审计核实是 6 不是 7：`remote-section.ts` 1 + `views/history.ts` 2 + `tabs.ts` 3）。
   *  注意这只对"值能由本函数自己推出"的维度成立；若是用户在 UI 现场勾选的维度
   *  （如 `--dangerously-skip-permissions`），本函数推不出来，届时需给 `opts` 加
   *  `extraModifiers?: LaunchModifiers` 让调用方注入并在内部 merge，那时 lambda 才真的零改。 */
  run: (mods: LaunchModifiers) => Promise<void>,
  opts: {
    sessionId?: string;
    onUnselectable?: (name: string) => void;
    /** account-ux U2:仅当 accountName===null 时生效——启用「跟随」解析(lastAccount→当前账号→基座)。 */
    follow?: { lastAccount?: string | null };
  } = {},
): Promise<void> {
  let state: AccountsState | undefined;
  if (accountName || opts.follow) {
    try {
      state = await fetchAccounts(origin);
    } catch {
      state = undefined; // 账号库拿不到 → 落 base（fetchAccounts 通常不抛，防御性兜底）
    }
  }
  const resolution: AccountResolution = state
    ? resolveAccount(state, { explicit: accountName, follow: opts.follow })
    : accountName
      ? { kind: "unavailable", requestedName: accountName }
      : { kind: "base" };

  let configDir: string | undefined;
  let recordName: string | null = null; // 成功注入后要记的账号名(显式=accountName / 跟随=解析名)
  if (resolution.kind === "account") {
    configDir = resolution.configDir;
    if (accountName) {
      // 显式选号(A4 语义不变)
      recordName = resolution.name;
    } else {
      // 跟随解析命中——U3 审计 重要-1:不 clobber 既有 pin。仅当**无既有 pin**(no-owner → 变
      // sticky)、或**解析结果==既有 pin**(no-op)时才记账;既有 pin 存在但不可选、下沉到
      // current → **不记账**,保住原 pin(守「粘性优先」不变量,避免 history/tab 默认 resume
      // 把会话账号悄悄翻成当前账号)。
      const priorPin = opts.follow?.lastAccount ?? null;
      recordName = !priorPin || resolution.name === priorPin ? resolution.name : null;
    }
  } else if (resolution.kind === "unavailable" && accountName) {
    opts.onUnselectable?.(accountName);
  }
  const modelOverride =
    resolution.kind === "account" ? await getModelForAccount(resolution.name) : undefined;
  await run({
    configDir,
    accountName: resolution.kind === "account" ? resolution.name : undefined,
    modelOverride,
  });
  if (recordName && configDir && opts.sessionId) {
    void recordLastAccount(opts.sessionId, recordName);
  }
}
