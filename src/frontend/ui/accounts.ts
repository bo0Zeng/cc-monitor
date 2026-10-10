// 多账号前端的账号模型（纯，零 IO）。账号 = 一个 CLAUDE_CONFIG_DIR。
// 只装「一个账号长什么样、能不能选、该显示成什么」：形状（`Account` · `AccountsState` · `SessionAccount`）· 降级判定（`deriveUi`）·
// 可用性照后端写好的 `selectable`（`selectableAccounts`）· 徽章文案。起会话用哪个号由会话所在那台的后端判。
// 别处：经通道读 ＋ 缓存 → `account-reads.ts`；每号模型偏好 → `account-prefs.ts`；起会话「要哪个号」→ `launch-account.ts`。
import { machineName, peerVersionSaid } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
// 先前这个类型住本文件、是 `ipc/commands.ts` 的返回类型 ⇒ 通信层在类型上依赖账号域，七模块类型环靠这一条边闭合。
import { copyText } from "./copy-table";
import type { AuthKind } from "./generated/judgment-rules";

// ---- 账号的形状（手写；由后端成品 ＋ 跨语言金样定）----
//
// 产：后端 `observe/accounts_query.rs::list_product`；钉：跨语言金样 `tests/__fixtures__/accounts.golden.json`（Rust 与 TS 同读）
// ＋ 解码器 `accounts-decode.ts::decodeAccountsList` 逐键核（多一格 / 缺一格 / 类型不对都抛）。
// `AuthKind` 从生成物派生（`acct_core::AUTH_KINDS` 生成进 `generated/judgment-rules.ts`）：后端改了它，金样那条判据红。
/** 一个账号的鉴权方式（`acct-core` 的契约字面量）。 */
export type { AuthKind };

/** manifest 里的一个账号（后端已剔除 configDir 不安全的条目；逐键同后端成品）。 */
export interface Account {
  name: string;
  email: string;
  /** `null` = 账号 0（不设 `CLAUDE_CONFIG_DIR`）。 */
  configDir: string | null;
  isDefault: boolean;
  /** `isolated`（正常）/ `in-place`（逃生口，前端应拒绝使用）/ `bare`（账号 0）。 */
  mode: string;
  exists: boolean;
  /** 只是 stat 了 `.credentials.json` 在不在，**不代表凭据有效**；可用性走 `authReady`。前端零处读它（收成品那一格除外）。 */
  loggedIn: boolean;
  /** 后端按 `acct_core::auth_kind_from_manifest` ＋ apikey 表并出的结论（缺省落订阅那一格也在那里判）。 */
  authKind: AuthKind;
  /**
   * 能不能起会话（鉴权那一维）：规则住 `acct_core::auth_ready`，后端算好，前端只读。
   * 订阅那一支只看 `.credentials.json` 在不在（凭据过期 / 被吊销看不出来）；api-key 那一支恒真，也不等于真能连上（徽章文案说出来）。
   */
  authReady: boolean;
  /** 能拿来起会话 / 选为默认：规则住 `acct_core::account_selectable`，那台后端写好，界面只读。 */
  selectable: boolean;
  /** API 号在那台 apikey 表里那一行的 key 掩码（只留末四位，那台后端遮好的）；订阅号 / 没配 ⇒ `null`。线上恒有这一格，本地造的夹具可以不写。 */
  keyMasked?: string | null;
  /** API 号那一行的端点（没写 ⇒ `null` ＝ 官方地址）。线上恒有这一格，本地造的夹具可以不写。 */
  baseUrl?: string | null;
  /**
   * 账号菜单每一项右边那一枚徽章（那台后端写好：原地模式 · API key 号经不经中转 · 订阅号登录了没有，`observe/accounts_query.rs::badge_of`）。
   * 只出自这一件成品，界面不按号的类型去挑来源。
   */
  badge: { text: string; warn: boolean; title: string };
}

/** 那台机器的账号库（manifest）的概况 —— 后端 `accounts-list` 成品的 `meta` 那一格，逐键照收。 */
export interface AccountsMeta {
  enabled: boolean;
  acctsDir: string;
  manifestPath: string;
  updatedAt: string | null;
  sharedStore: string | null;
  count: number;
  error: string | null;
  /** 这台做不了多账号时后端说的那一句（做得了 ⇒ `null`；线上恒有这一格，本地造的夹具可以不写）。 */
  unsupported?: string | null;
  /** 删掉默认号之后新会话默认谁（那台后端按删号同一条规则答；没有 ⇒ `null`）。线上恒有这一格，本地造的夹具可以不写。 */
  nextDefault?: string | null;
  /** 那台的默认号（标了的第一个，没标 ⇒ 第一个；规则住 `acct_core::effective_default`）；清单空 ⇒ `null`。 */
  effectiveDefault: string | null;
  /** 那台的家目录（界面把路径里的它缩成 `~`；推不出 ⇒ `null`）。线上恒有这一格，本地造的夹具可以不写。 */
  home?: string | null;
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
  /**
   * 这条会话的上游地址是不是**本机中转**那一形（后端从 `/proc/<pid>/environ` 的 `ANTHROPIC_BASE_URL` 折出的一个布尔，
   * 值本身带中转钥匙、不出参）。`null` = 不知道（进程已死 / 环境这一刻取不到 / 老后端没有这个键）。
   * 读者：机器页「停」本机后端之前数几条会断（`settings/backend-section.ts::stopWarning`）。
   */
  viaRelay?: boolean | null;
}

/** 账号功能在某台远端的整体状态（UI 直接消费）。 */
export interface AccountsState {
  /** 哪台机器（本机 = `LOCAL_ORIGIN`）。也是账号缓存的键 —— 两者从此是同一个值。 */
  origin: Origin;
  available: boolean;
  error: string | null;
  /** `available:false` 的那一次是不是「那台后端比这条查询老」（`chan-caller.ts::isOldBackend` 判的）；其余失败 ⇒ `false`。 */
  oldBackend: boolean;
  meta: AccountsMeta | null;
  accounts: Account[];
  /** 能用但有缺时的人话说明（`available` 仍是 true）；null = 无缺。不出这一句，列表里少一行用户看不出来。 */
  notice: string | null;
  /**
   * 这一次没问到（`available:false`）时，这次运行里那台最近一次答成的那一份＋ 答成的时刻：
   * 只给「画上次的 ＋ 采样 n 前」用，不当作此刻的事实（起会话、换默认一律不读它）。没有 ⇒ `null`。
   */
  last?: { meta: AccountsMeta; accounts: Account[]; atMs: number } | null;
}

/** chip / 设置组据此决定怎么显示。纯派生自 AccountsState。 */
export type AccountsUi =
  | { kind: "needs-update"; reason: string } // 旧 backend（只有对端说「不认这条命令」那一形）
  | { kind: "query-failed"; reason: string } // 没问出来（够不着 / 期限到 / 对端说不行 / 形状不对）
  | { kind: "not-enabled"; manifestPath: string | null; reason: string } // 未迁移/无账号
  | { kind: "ready"; accounts: Account[]; notice: string | null };

// ------------------------------------------------------------ 纯函数

/** 账号读回 → 界面判定（降级矩阵）。纯函数。 */
export function deriveUi(state: AccountsState): AccountsUi {
  if (!state.available) {
    const e = state.error ?? "";
    // 按失败的种类分（不按原因串猜：远端没部署上 / 拒转发不是「需更新」）。
    if (state.oldBackend) return { kind: "needs-update", reason: peerVersionSaid("backend_old", state.origin) };
    return { kind: "query-failed", reason: e || copyText("accounts.deriveUi.unavailable") };
  }
  if (!state.meta?.enabled || state.accounts.length === 0) {
    return {
      kind: "not-enabled",
      manifestPath: state.meta?.manifestPath ?? null,
      reason: state.meta?.error ?? copyText("accounts.deriveUi.notEnabled", { machine: machineName(state.origin) }),
    };
  }
  return {
    kind: "ready",
    accounts: state.accounts,
    notice: state.notice,
  };
}

/** 那台的默认号：照那台后端写好的 `meta.effectiveDefault` 取那一行（判定住 `acct_core::effective_default`，这里不另判）。 */
export function defaultAccount(state: AccountsState): Account | null {
  const name = state.meta?.effectiveDefault ?? null;
  return name === null ? null : (state.accounts.find((a) => a.name === name) ?? null);
}

/** 账号表那一行第二行要的那一档（设置窗账号页）：API key 号 · 订阅号已登录 · 订阅号还没登录（「已登录」只代表凭据文件在）。 */
export type AccountRowKind = "apikey" | "subscription" | "notLoggedIn";

export function accountRowKind(a: Account): AccountRowKind {
  if (a.authKind === "api-key") return "apikey";
  return a.loggedIn ? "subscription" : "notLoggedIn";
}

/** 可选账号列表（照那台后端写好的 `selectable`；判定住 `acct_core::account_selectable`）。休眠判据 / 计数一律走它。 */
export function selectableAccounts(state: AccountsState): Account[] {
  return state.accounts.filter((a) => a.selectable);
}

/**
 * 账号色系统是否该激活。
 *
 * 只有一个可选账号时，彩色头像不携带任何信息——它区分不了任何东西，纯属噪音；等用户加了
 * 第二个号，颜色才开始有意义。故 `≥2 个可选账号 && 该 origin 账号确实可查询` 才激活。
 *
 * 作用面只有状态栏 chip 与 tab 徽章。设置里的账号表恒显：
 * 那是全应用唯一能让用户学到「色块 ↔ 账号 ↔ 邮箱」映射的图例面，单账号期把它也休眠掉，
 * 等加了第二个号就会突然满屏彩块。
 */
export function accountColorsActive(state: AccountsState): boolean {
  return state.available && selectableAccounts(state).length >= 2;
}

/**
 * 真正可用的当前账号：默认号再看它选不选得了（那台写好的 `selectable`）。
 * 拿未过滤的值判「不一致」，徽章会指着一个永远不会被跟过去的号说「你不一致」。消费者：`tabs.ts::updateAccountBadge`。
 */
export function currentAccountForBadge(state: AccountsState): Account | null {
  const cur = defaultAccount(state);
  return cur?.selectable ? cur : null;
}

/** 活会话账号是否与当前账号不一致（纯函数）：两者都确知且不同才 true；任一未知 ⇒ false（不误报）。 */
export function detectAccountMismatch(
  liveAccount: string | null,
  current: string | null,
): boolean {
  return liveAccount !== null && current !== null && liveAccount !== current;
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
 * 本地会话（`LOCAL_ORIGIN`）不产徽章。
 */
export interface SessionBadge {
  text: string; // 显示文本；"—" = 未知
  known: boolean; // 是否确知账号
  tooltip: string;
  /** 徽章数据来源：'live' = 实时探测（实心头像）/ 'last' = 上次记录（幽灵头像）/ 'unknown' = 不猜。 */
  source: "live" | "last" | "unknown";
  /** 确知时的账号名（text 是缩写，这里是全名，供不一致比对 / 悬停）；未知为 null。 */
  account: string | null;
}
export function sessionBadge(
  sid: string,
  origin: Origin,
  liveByS: Map<string, SessionAccount>,
  emailByName: Map<string, string>,
  lastAccountByS?: Map<string, string>,
): SessionBadge | null {
  if (isLocalOrigin(origin)) return null; // 本机会话不出账号徽章
  // 源①：live 探测——唯一硬真相，优先。
  const live = liveByS.get(sid);
  if (live && live.alive && live.account) {
    const email = emailByName.get(live.account) ?? "";
    return {
      text: badgeText(live.account),
      known: true,
      tooltip: copyText("accounts.sessionBadge.live", { account: live.account, email: email ? ` · ${email}` : "" }),
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
      tooltip: copyText("accounts.sessionBadge.last", { last, email: email ? ` · ${email}` : "" }),
      source: "last",
      account: last,
    };
  }
  // 源③：都没有 → 未知，不猜。
  return {
    text: copyText("accounts.sessionBadge.none"),
    known: false,
    tooltip: copyText("accounts.sessionBadge.noneHint"),
    source: "unknown",
    account: null,
  };
}

/**
 * 某会话该不该显账号徽章：只有账号可查询的远端才显（available 的 origin，main.ts 收进 readyOrigins）。
 * 本机会话与不可查询的远端不显 —— 满屏 `—` 是噪音，不可用就安静隐藏。
 */
export function shouldShowAccountBadge(
  origin: Origin,
  readyOrigins: Set<string>,
): boolean {
  if (isLocalOrigin(origin)) return false; // 本机会话不出账号徽章
  return readyOrigins.has(origin);
}

