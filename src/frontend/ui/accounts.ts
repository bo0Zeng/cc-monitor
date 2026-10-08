// A3：多账号前端的**账号模型** —— 形状与规则的单一真相（纯，零 IO）。
//
// 账号 = 一个 CLAUDE_CONFIG_DIR。本模块只装「一个账号长什么样、它能不能选、它该显示成什么」：
//   形状（`Account` · `AccountsState` · `SessionAccount`）· 降级判定（`deriveUi`）· 可用性唯一出口（`isSelectable`）·
//   徽章与本机那一节的界面文案。起会话用哪个号由会话所在那台的后端判（这里不判）。
//
// 先前这一个文件（1527 行 · 61 export · 20 个生产 importer）跨账号 · 起停 · 历史三个域（审计 B §6 必须拆 4）。
// 守的要求：「一个判定只有一个家」。按域拆开之后各住各的：
//   - 经通道读 ＋ 缓存（`accounts-list` · `accounts-sessions` · `accounts-trust`）→ `account-reads.ts`
//   - config.json 里的账号偏好（每号模型）→ `account-prefs.ts`
//   - 起会话那一格「要哪个号」与「选不了」那个选择框 → `launch-account.ts`
// 本文件从此**不 import 任何有 IO 的模块**（不碰通道、不碰 config、不碰历史注解）。
import { machineName, peerVersionSaid } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
// API key 那两问的成品（`apikey-routing`）住 `apikey-reads.ts`（经通道、后端出成品）；本文件只把那份读数落到账号上。
// 先前这个类型住本文件、是 `ipc/commands.ts` 的返回类型 ⇒ 通信层在类型上依赖账号域，七模块类型环靠这一条边闭合。
import type { ApikeyRoutingView } from "./apikey-reads";
import { copyText } from "./copy-table";
import type { AuthKind } from "./generated/judgment-rules";

// ---- 账号的形状：从生成物改回手写，形状由后端成品 ＋ 跨语言金样定 ----
//
// K-A1 起这一格是 ts-rs 生成物（`src/frontend/ui/generated/RemoteAccount.ts`，Rust 那份在 monitor 的 `accounts.rs`）。
// C4c 起账号清单由**那台机器的后端出成品**、界面严格收（`accounts-decode.ts::decodeAccountsList`）；
// monitor 里最后一个产出那份 Rust 结构的是一份零生产调用方的本机参照实现，已删 ⇒ 生成源没了。
// 形状今天由两样东西钉：后端 `observe/accounts_query.rs::list_product`（产）＋ 跨语言金样
// `tests/__fixtures__/accounts.golden.json`（Rust 与 TS 两侧同读）＋ 解码器逐键核（多一格 / 缺一格 / 类型不对都抛）。
// `AuthKind` 那两个字面量是 `acct-core` 的契约常量（`AUTH_KIND_SUBSCRIPTION` / `AUTH_KIND_API_KEY`）；
// 类型从生成物派生（`acct_core::AUTH_KINDS` 现生成进 `src/frontend/ui/generated/judgment-rules.ts`），不再手写字面量；
// 后端改了它 ⇒ 生成物与金样跟着变 ⇒ 解码器认不出旧的 ⇒ `accounts-decode.vitest.ts` 金样那条红。
/** 一个账号的鉴权方式（`acct-core` 的契约字面量）。 */
export type { AuthKind };

/** manifest 里的一个账号（后端已剔除 configDir 不安全的条目；逐键同后端成品）。 */
export interface Account {
  name: string;
  email: string;
  /** Z01：`null` = 账号 0（「不设 `CLAUDE_CONFIG_DIR`」这个状态）。 */
  configDir: string | null;
  isDefault: boolean;
  /** `isolated`（正常）/ `in-place`（逃生口，前端应拒绝使用）/ `bare`（账号 0）。 */
  mode: string;
  exists: boolean;
  /** 只是 stat 了 `.credentials.json` 在不在，**不代表凭据有效**；可用性走 `authReady`。前端零处读它（收成品那一格除外）。 */
  loggedIn: boolean;
  /** 后端按 `acct_core::auth_kind_from_manifest` ＋ apikey 表并出的结论（缺省落订阅那一格也在那里判，`KA6d`）。 */
  authKind: AuthKind;
  /**
   * 鉴权方式这一维**不再阻塞**这个号被选中 —— 规则住 `acct_core::auth_ready`，后端算好放进这一格，前端只读。
   *
   * ⚠ **`KA6b`（诚实边界）**：订阅那一支只是 stat 了 `.credentials.json` 在不在 —— 凭据过期 / 被吊销看不出来；
   * api-key 那一支恒真（它不用那个文件），`true` 也不等于「真能连上」（`KA6a`，徽章那段文案说出来）。
   *
   * 这一格与 `authKind` 原来都是**可缺**的，缺了由 `accounts.ts::authReady`〔散文墓碑〕
   * 回落到 `loggedIn`、`authKind` 读成订阅 —— 那是 `auth_ready` 订阅分支与 `auth_kind_from_manifest` 缺省那一格
   * 在 TS 里的第二份，而且是「旧后端」的退路（`D11`）。解码器（`accounts-decode.ts`）早就逐键要求这两格、
   * 缺了就抛 ⇒ 回落今天不可达，删了。
   */
  authReady: boolean;
  /** API 号在那台 apikey 表里那一行的 key 掩码（只留末四位，那台后端遮好的）；订阅号 / 没配 ⇒ `null`。线上恒有这一格，本地造的夹具可以不写。 */
  keyMasked?: string | null;
  /** API 号那一行的端点（没写 ⇒ `null` ＝ 官方地址）。线上恒有这一格，本地造的夹具可以不写。 */
  baseUrl?: string | null;
}

/**
 * 那台机器的账号库（manifest）的概况 —— 后端 `accounts-list` 成品的 `meta` 那一格，逐键照收。
 * `accountZeroAware` 那一格退役：出成品的后端按构造认得账号 0（老后端回的是旧形状，当场认出）。
 */
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
  /**
   * Z01：**能用但有缺**时的人话说明（`available` 仍是 true）。null = 无缺。
   * 「绝不静默降级」是它存在的全部理由——旧 backend / 旧的写清单那一侧会让账号 0
   * 从列表里凭空少一行，用户看不出区别。
   */
  notice: string | null;
  /**
   * 这一次没问到（`available:false`）时，这次运行里那台最近一次答成的那一份＋ 答成的时刻：
   * 只给「画上次的 ＋ 采样 n 前」用，不当作此刻的事实（起会话、换默认一律不读它）。没有 ⇒ `null`。
   */
  last?: { meta: AccountsMeta; accounts: Account[]; atMs: number } | null;
}

/** chip / 设置组据此决定怎么显示。纯派生自 AccountsState。 */
// 🔴 `K-R59`（09-11）：这里原来还有一档 `{ kind: "hidden" }` —— 它**只由**
// 「该主机配置为 daemonless（无后端）」那条错误串产出，而 `K35` 把那一档整个删了
//（`accounts.rs::cfg_for` 那个早返回一起走）⇒ 留着就是一档**再也到不了**的 UI 状态。
export type AccountsUi =
  | { kind: "needs-update"; reason: string } // 旧 backend（只有对端说「不认这条命令」那一形）
  | { kind: "query-failed"; reason: string } // 没问出来（够不着 / 期限到 / 对端说不行 / 形状不对）
  | { kind: "not-enabled"; manifestPath: string | null; reason: string } // 未迁移/无账号
  | { kind: "ready"; accounts: Account[]; notice: string | null };

// ------------------------------------------------------------ 纯函数

/** A2 返回 → UI 判定（DESIGN §7 降级矩阵）。纯函数，vitest 锁死。 */
export function deriveUi(state: AccountsState): AccountsUi {
  if (!state.available) {
    const e = state.error ?? "";
    // 按失败的**种类**分（从前按原因串里有没有「过旧」猜，又把其余一律并进「需更新」—— 真机上远端没部署上 / 拒转发都说成要更新）。
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

/** 那台的默认账号：清单里 `isDefault` 的那一个，没有就第一个（起会话跟随时那台判的是同一条，这里只为画）。 */
export function effectiveDefault(state: AccountsState): Account | null {
  if (state.accounts.length === 0) return null;
  return state.accounts.find((a) => a.isDefault) ?? state.accounts[0];
}

/** 「当前账号」—— [`effectiveDefault`] 的语义别名（那台账号库清单里的默认号；设它走那台的 `accounts-set-default`）。 */
export function currentWorkingAccount(state: AccountsState): Account | null {
  return effectiveDefault(state);
}

/**
 * 账号状态徽章（`KA6a` 的那段文案）——**两处渲染同一个概念，取值只许有一处**。
 *
 * 设置里的账号表（`settings/accounts-section.ts`）与状态栏 chip 的账号菜单
 * （`account-chip.ts`）各渲染一份这个三态。K-A1 之前两处各写一遍
 * `a.mode === "in-place" ? … : !a.loggedIn ? … : "已登录"`。
 *
 * ★ **`KA6a`：api-key 号今天「选得中、起得来、但请求发不出去」** ——
 * 配端点那条路要等第三方 API 路线裁定（`BACKLOG.md` `E36` 的甲/乙/丙）。
 * 所以它的徽章**不许写「已登录」**：那会让用户以为可以用，起了会话才在 claude 里
 * 撞一个鉴权失败，而 UI 说这个号没问题。
 *
 * ⚠ 「已登录」这一档仍然只代表 `.credentials.json` 在（`KA6b`）：凭据过期/被吊销看不出来。
 */
export interface AccountStatusBadge {
  /** 徽章文本。 */
  text: string;
  /** 是否该显示成警示态（调用方加 `warn` class）。 */
  warn: boolean;
  /** hover 说明；空串 = 不加 title。 */
  title: string;
}
/**
 * api-key 号那一格：**这个号所在那台机器**（本机 / 远端同一条路：那台的后端答 `apikey-routing`）的两格事实。
 * 起会话时由那台的 `ccm` 定往哪发、那台的中转按那台 key 表里这一行换上 key ⇒ 两格都成立才替它配好。
 *
 * | `endpoint` | 用户看到 | 那句话为什么是真的 |
 * |---|---|---|
 * | `{hasRow:true,running:true}` | 「API key（经中转）」 | 两个前置都成立 |
 * | `{hasRow:true,running:false}` | 「API key（中转未运行）」 | 起会话那一侧会**当场拒** |
 * | `{hasRow:false}` | 「API key（未配置端点）」 | 表里没有这一行 ⇒ 确实没人替它配 |
 * | 缺席 | 「API key（未配置端点）」 | 没问到 ⇒ **不替它下判断**，只把条件说清 |
 *
 * ⚠ **不许从「不会配」直接跳成「已登录」** —— 中间隔着这两格。
 */
export type ApikeyEndpointState = { hasRow: boolean; running: boolean };

/**
 * 账号表那一行第二行要的那一档（设置窗账号页）：API key 号 · 订阅号已登录 · 订阅号还没登录。
 * 「已登录」仍只代表凭据文件在（`KA6b`）。
 */
export type AccountRowKind = "apikey" | "subscription" | "notLoggedIn";

export function accountRowKind(a: Account): AccountRowKind {
  if (a.authKind === "api-key") return "apikey";
  return a.loggedIn ? "subscription" : "notLoggedIn";
}

/**
 * 把那台机器的读数落到**一个账号**上。
 *
 * `configDir` 缺席（账号 0）⇒ `undefined`：账号 0 在 manifest 里没有目录名，
 * 推不出 key 表里的 id ⇒ 说不出就不表态。
 */
export function apikeyEndpointStateFor(
  a: Account,
  routing: ApikeyRoutingView,
): ApikeyEndpointState | undefined {
  if (!a.configDir) return undefined;
  return { hasRow: routing.routed.includes(a.configDir), running: routing.running };
}

export function accountStatusBadge(
  a: Account,
  endpoint?: ApikeyEndpointState,
): AccountStatusBadge {
  if (a.mode === "in-place") {
    return {
      text: copyText("accounts.badge.inPlace"),
      warn: true,
      title: copyText("accounts.badge.inPlaceHint"),
    };
  }
  if (a.authKind === "api-key") {
    if (endpoint?.hasRow && endpoint.running) {
      return {
        text: copyText("accounts.badge.apikeyRelayed"),
        warn: false,
        title:
          copyText("accounts.badge.apikeyRelayedHint"),
      };
    }
    if (endpoint?.hasRow) {
      return {
        text: copyText("accounts.badge.apikeyRelayDown"),
        warn: true,
        title:
          copyText("accounts.badge.apikeyRelayDownHint"),
      };
    }
    // 两种「没配上」的成因，各说各的 —— 合成一句就等于又写下一句说不准的话。
    const why = endpoint != null ? copyText("accounts.badge.whyNoRow") : copyText("accounts.badge.whyUnknown");
    return {
      text: copyText("accounts.badge.apikeyNoEndpoint"),
      warn: true,
      title:
        copyText("accounts.badge.apikeyNoEndpointHint", { why }),
    };
  }
  if (!a.authReady) {
    return {
      text: copyText("accounts.badge.notSignedIn"),
      warn: true,
      title: copyText("accounts.badge.notSignedInHint"),
    };
  }
  return { text: copyText("accounts.badge.signedIn"), warn: false, title: "" };
}

/** 某账号是否可被选为默认 / 用来起会话。 */
export function isSelectable(a: Account): boolean {
  // 账号 0（mode "bare"）在这里**天然落选**。
  //
  // ★ **Z02 订正了 Z01 在这儿写的一句错话**。Z01 写的是「从 UI 起它需要『显式 unset』的
  // 注入路径，而 launch-plan 今天只会 export」——**不对**：那条路径早就有了，两条渲染路各一份
  //   · CLI 路径：CLI 渲染器的 `account` 维度（今天只在 Rust `ccm_invocation.rs`）对非 account 态吐 `--base`，
  //     而 `shared/ccm` 收到 `--base` 会 `unset CLAUDE_CONFIG_DIR`（两处落点，
  //     由 `base-flag-contract-guard.vitest.ts` 钉住）
  //   · 兜底渲染路径：`ENV_RESET_DIMENSION` 推 `unset-config-dir` op
  //
  // 真正缺的是**选择链路**，不是注入形态：
  //   1. `accountConfigDir()` 对它返回 null ⇒ 选号那一步说不出「用户显式选了账号 0」
  //      （只能说 `unavailable`，那是「你要的号不能用」，语义不同）
  //   2. `AccountModifierOption` 没有账号 0 这个选项
  //   3. `tabs.ts:2283` 那个 `opt.kind === "base" ? … : …` 三元**不会编译报错**地把新变体
  //      送进 else 分支（拿 `opt.name === undefined` 去起会话）⇒ 加变体前必须先改它
  // 第 3 条卡 `tabs.ts` 红线 ⇒ 放开这条门槛要等红线松（见 features/Z02-PARTIAL.md）。
  //
  // ★ **K-A1 把第二项从 `a.loggedIn` 换成了后端算好的 `a.authReady`**（原先经一个带「旧后端回落」的包装读，包装删了）。
  // 订阅号那一支的值与 `loggedIn` **逐字节相同**（`acct_core::auth_ready` 的订阅分支就是
  // 「凭据文件在不在」）⇒ 订阅号一格没变，包括「缺凭据 ⇒ 不可选」那道保护（`KAY3`）。
  // 变的只有 api-key 号：它压根不用那个文件，所以不再因为缺文件而被判不可用（`KAY2`）。
  return a.mode === "isolated" && a.authReady && a.exists;
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
 * 等加了第二个号就会突然满屏彩块。
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
 * `tabs.ts::updateAccountBadge` 一处消费者）。
 */
export function currentAccountForBadge(state: AccountsState): Account | null {
  const cur = currentWorkingAccount(state);
  return cur && isSelectable(cur) ? cur : null;
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
 * 仅当账号存在且**可选**（isolated + 鉴权前提就绪 + 目录在）才给；否则 null（不可选的绝不注入）。
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
 * 本地会话（`LOCAL_ORIGIN`）不产徽章。
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
  origin: Origin,
  liveByS: Map<string, SessionAccount>,
  emailByName: Map<string, string>,
  lastAccountByS?: Map<string, string>,
): SessionBadge | null {
  if (isLocalOrigin(origin)) return null; // 本地会话 A7 前不支持
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
 * A4/§7 降级：某会话是否**该显**账号徽章。只有「账号可查询」的远端才显（即 available 的
 * origin,由 main.ts 收进 readyOrigins）。本地会话（`LOCAL_ORIGIN`）与不可查询的远端
 * （未迁移 / 旧后端）一律不显——否则满屏 `—` 是噪音、违反 §7「不可用即安静隐藏」。
 */
export function shouldShowAccountBadge(
  origin: Origin,
  readyOrigins: Set<string>,
): boolean {
  if (isLocalOrigin(origin)) return false; // 本地会话 A7 前不支持
  return readyOrigins.has(origin);
}

