// 多账号前端的账号模型（纯，零 IO）。账号 = 一个 CLAUDE_CONFIG_DIR。
// 只装「一个账号长什么样、能不能选、该显示成什么」：形状（`Account` · `AccountsState`）· 降级判定（`deriveUi`）·
// 可用性照后端写好的 `selectable`（`selectableAccounts`）· 徽章文案。起会话用哪个号由会话所在那台的后端判。
// 别处：经通道读 → `account-reads.ts`；每号模型偏好 → `account-prefs.ts`；起会话「要哪个号」→ `launch-account.ts`。
import { machineName } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import type { AccountRow } from "./generated/AccountRow";
import type { ListMeta } from "./generated/ListMeta";
import type { SeenAt } from "./generated/SeenAt";
import type { AuthKind } from "./generated/judgment-rules";

// ---- 账号的形状：后端 `accounts-list` 的成品（生成类型，`observe/accounts_query.rs::list_product` 出）----
// 号色 · 号牌字 · 那几句写好的字线上恒有，本地造的夹具可以不写（不写 ⇒ 号色画中性、号牌画「—」）。

type FixtureOptional = "label" | "configDirShown" | "keyMasked" | "baseUrl" | "color" | "plate" | "quota" | "unselectable" | "login" | "newSession";
/** manifest 里的一个账号（后端已剔除 configDir 不安全的条目）。 */
export type Account = Omit<AccountRow, FixtureOptional> & Partial<Pick<AccountRow, FixtureOptional>>;

type MetaOptional = "unsupported" | "nextDefault";
/** 那台机器的账号库（manifest）的概况 —— `accounts-list` 成品的 `meta` 那一格。 */
export type AccountsMeta = Omit<ListMeta, MetaOptional> & Partial<Pick<ListMeta, MetaOptional>>;

/** 账号功能在某台远端的整体状态（UI 直接消费）。 */
export interface AccountsState {
  /** 哪台机器（本机 = `LOCAL_ORIGIN`）。 */
  origin: Origin;
  available: boolean;
  error: string | null;
  /** `available:false` 的那一次是不是「那台后端比这条查询老」（`chan-caller.ts::isOldBackend` 判的；那一句 monitor 写好、在 `error`）；其余失败 ⇒ `false`。 */
  oldBackend: boolean;
  meta: AccountsMeta | null;
  accounts: Account[];
  /** 能用但有缺时的人话说明（`available` 仍是 true）；null = 无缺。不出这一句，列表里少一行用户看不出来。 */
  notice: string | null;
  /**
   * 这一次没问到（`available:false`）时，本机后端记着的那台上次答成的那一份 ＋ 上次读数那一格（本机后端写好的「采样于 …」；本机那台不记）：
   * 只给「画上次的」用，不当作此刻的事实（起会话、换默认一律不读它）。没有 ⇒ `null`。
   */
  last?: { meta: AccountsMeta; accounts: Account[]; seen: SeenAt } | null;
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
    if (state.oldBackend) return { kind: "needs-update", reason: e };
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

/** API key 号那一种（取值集从 `acct_core::AUTH_KINDS` 现生成：那边改了名，这里编不过）。 */
const API_KEY: AuthKind = "api-key";

export function accountRowKind(a: Account): AccountRowKind {
  if (a.authKind === API_KEY) return "apikey";
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
