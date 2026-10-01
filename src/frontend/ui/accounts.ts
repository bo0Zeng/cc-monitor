// A3：多账号前端的**账号模型** —— 形状与规则的单一真相（纯，零 IO）。
//
// 账号 = 一个 CLAUDE_CONFIG_DIR。本模块只装「一个账号长什么样、它能不能选、它该显示成什么」：
//   形状（`Account` · `AccountsState` · `SessionAccount`）· 降级判定（`deriveUi`）· 可用性唯一出口（`isSelectable`）·
//   跟随 / 显式选号的解析（`resolveAccount`）· 徽章与本机那一节的界面文案。
//
// 先前这一个文件（1527 行 · 61 export · 20 个生产 importer）跨账号 · 起停 · 历史三个域（审计 B §6 必须拆 4）。
// 守的要求：「一个判定只有一个家」。按域拆开之后各住各的：
//   - 经通道读 ＋ 缓存（`accounts-list` · `accounts-sessions` · `accounts-trust`）→ `account-reads.ts`
//   - config.json 里的账号偏好（默认账号 · 每号模型）→ `account-prefs.ts`
//   - 起会话挑哪个号（`withAccount` · 本机快照 · 载荷上的 `account` · 记 pin）→ `launch-account.ts`
//   - 本机起新会话后拿身份 token 反查 sid 的待办表 → `local-launch-backfill.ts`
//   - 换号重启定位不到时的那句话 → `account-restart.ts`
// 本文件从此**不 import 任何有 IO 的模块**（不碰通道、不碰 config、不碰历史注解）。
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
// monitor 里最后一个产出那份 Rust 结构的是一份零生产调用方的本机参照实现，主会话 09-25 裁删（C4d）⇒ 生成源没了。
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
   * `K-P5f`：起会话方铸进这条会话进程环境的**身份 token**（`CCM_LAUNCH_ID`），由后端从
   * `/proc/<pid>/environ` 读回来。`null` = **不作数**（没设 / 形状不合格 / 同一 token 落在
   * 一条以上活会话上 / 进程已死，四种原因刻意合并，见 `src/frontend/shell/src/accounts.rs`
   * 的 `SessionAccount::launch_id`）。
   *
   * ⚠ **可选是因为老后端的出参里逐字节没有这个键**（additive）。本机那条路来的行是
   * `src/frontend/ui/generated/SessionAccount.ts`（必有此键），远端那条路是后端直出的原始 JSON
   * （老后端缺键 ⇒ `undefined`）—— 两者在这里合流，所以这一格写成可选、
   * 而消费方一律把 `undefined` 与 `null` 当同一件事。
   */
  launchId?: string | null;
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
  /** 本机选择的默认账号（config.json）；缺省跟随 manifest 的 isDefault。 */
  defaultName: string | null;
  /**
   * Z01：**能用但有缺**时的人话说明（`available` 仍是 true）。null = 无缺。
   * 「绝不静默降级」是它存在的全部理由——旧 backend / 旧的写清单那一侧会让账号 0
   * 从列表里凭空少一行，用户看不出区别。
   */
  notice: string | null;
}

/** chip / 设置组据此决定怎么显示。纯派生自 AccountsState。 */
// 🔴 `K-R59`（09-11）：这里原来还有一档 `{ kind: "hidden" }` —— 它**只由**
// 「该主机配置为 daemonless（无后端）」那条错误串产出，而 `K35` 把那一档整个删了
//（`accounts.rs::cfg_for` 那个早返回一起走）⇒ 留着就是一档**再也到不了**的 UI 状态。
export type AccountsUi =
  | { kind: "needs-update"; reason: string } // 旧 backend（只有对端说「不认这条命令」那一形）
  | { kind: "query-failed"; reason: string } // 没问出来（够不着 / 期限到 / 对端说不行 / 形状不对）
  | { kind: "not-enabled"; manifestPath: string | null; reason: string } // 未迁移/无账号
  | { kind: "ready"; accounts: Account[]; defaultName: string | null; notice: string | null };

// ------------------------------------------------------------ 纯函数

/** A2 返回 → UI 判定（DESIGN §7 降级矩阵）。纯函数，vitest 锁死。 */
export function deriveUi(state: AccountsState): AccountsUi {
  if (!state.available) {
    const e = state.error ?? "";
    // 按失败的**种类**分（从前按原因串里有没有「过旧」猜，又把其余一律并进「需更新」—— 真机上远端没部署上 / 拒转发都说成要更新）。
    if (state.oldBackend) return { kind: "needs-update", reason: e || copyText("accounts.deriveUi.needsUpdate") };
    return { kind: "query-failed", reason: e || copyText("accounts.deriveUi.unavailable") };
  }
  if (!state.meta?.enabled || state.accounts.length === 0) {
    return {
      kind: "not-enabled",
      manifestPath: state.meta?.manifestPath ?? null,
      reason: state.meta?.error ?? copyText("accounts.deriveUi.notEnabled"),
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
 * `K-H2b` `KH2B7`：**api-key 号那一格今天不是一态。**
 *
 * 本件之前那句 hover 文案逐字是「cc-monitor 今天还不会替它配 API key 与 base URL」——
 * 本件落地那一刻，它对**一部分号**就成了假话（本机、且apikey 表里有它那一行、且中转在跑
 * 的那些号，cc-monitor **真的**会替它配）。⇒ 按「这个号属于哪一半 / 那两个前置成不成立」
 * 分别说各自的话。
 *
 * | `endpoint` | 用户看到 | 那句话为什么是真的 |
 * |---|---|---|
 * | `{scope:"local",hasRow:true,running:true}` | 「api-key（经本机中转）」 | 两个前置都成立 |
 * | `{scope:"local",hasRow:true,running:false}` | 「api-key（中转未运行）」 | 起会话那一侧会**当场拒**（`KH2B2`②） |
 * | `{scope:"local",hasRow:false}` | 「api-key（未配置端点）」 | 表里没有这一行 ⇒ 确实没人替它配 |
 * | `{scope:"remote"}` | 「api-key（未配置端点）」 | **远端那一半本件明写不做**（`§0e` 裁四） |
 * | 缺席 | 「api-key（未配置端点）」 | 调用方没说是哪一半 ⇒ **不替它下判断**，只把条件说清 |
 *
 * ⚠ **不许从「不会配」直接跳成「已登录」** —— 中间隔着这两格。
 *
 * ⚠⚠ **诚实边界（本件没做完的那一格）**：`{scope:"local"}` 那三档今天**没有生产调用方** ——
 * 要把「表里有没有这一行」「中转在不在跑」端到前端，得注册一条**只答本机**的 tauri 命令，
 * 而新注册一条命令会让 `src/frontend/shell/src/parity_ledger.rs` 的
 * `every_tauri_command_is_declared_in_the_ledger` 当场红（本轮实测过，报文点名了那条命令），
 * 那个文件不在 `K-H2b` 的写区。⇒ 两个生产调用点今天分别传 `{scope:"remote"}`（设置里那张表
 * 是**远端专用**的：`accounts-section.ts` 的 `reload` 对 `origin` 为空时直接早退）与
 * 「远端就 `{scope:"remote"}`、本机就缺席」（chip）。
 * **这是「本机那三档有实现、没接线」，别读成「接上了」。** 经过住件文件 `§4`。
 */
export type ApikeyEndpointState =
  /** 远端那一半：`K-H2b` `§0e` 裁四明写不做 ⇒ 对它确实没人配端点。 */
  | { scope: "remote" }
  /** 本机那一半：两个前置各自成不成立。 */
  | { scope: "local"; hasRow: boolean; running: boolean };


/**
 * 把上面那份读数落到**一个账号**上。
 *
 * `configDir` 缺席（账号 0）⇒ `null`：账号 0 在 manifest 里没有目录名，
 * **推不出apikey 表里的 id** ⇒ 说不出就不表态（与 Rust 侧 `apikey_account_id` 的三态同形）。 〔散文墓碑〕
 */
export function localApikeyEndpointStateFor(
  a: Account,
  routing: ApikeyRoutingView,
): ApikeyEndpointState | undefined {
  if (!a.configDir) return undefined;
  return { scope: "local", hasRow: routing.routed.includes(a.configDir), running: routing.running };
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
    const local = endpoint?.scope === "local" ? endpoint : null;
    if (local?.hasRow && local.running) {
      return {
        text: copyText("accounts.badge.apikeyRelayed"),
        warn: false,
        title:
          copyText("accounts.badge.apikeyRelayedHint"),
      };
    }
    if (local?.hasRow) {
      return {
        text: copyText("accounts.badge.apikeyRelayDown"),
        warn: true,
        title:
          copyText("accounts.badge.apikeyRelayDownHint"),
      };
    }
    // 三种「没配上」的成因，各说各的 —— **合成一句就等于又写下一句说不准的话**。
    const why =
      local != null
        ? copyText("accounts.badge.whyNoRow")
        : endpoint?.scope === "remote"
          ? // 旧句「把 key 送到远端那台机器是另一件事」半过期了：key 今天送得到那台机器上
            //   （设置里配 key 按页上那台机器写），差的是远端起的会话还不经中转换上它（注入归 4B RL1）。
            copyText("accounts.badge.whyRemote")
          : copyText("accounts.badge.whyUnknown");
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

/**
 * 「打开该账号终端」那个按钮的文案（A6）。
 *
 * 与徽章同一个道理：对 api-key 号说「去登录」是**假话** —— 它不需要 `/login`，
 * `/login` 也修不了它缺端点这件事。
 */
export function accountLoginActionLabel(a: Account): { label: string; title: string } {
  if (a.authKind === "api-key") {
    return {
      label: copyText("accounts.loginAction.openTerminal"),
      title: copyText("accounts.loginAction.openTerminalHint"),
    };
  }
  return {
    label: a.authReady ? copyText("accounts.loginAction.loginTerminal") : copyText("accounts.loginAction.goLogin"),
    title: copyText("accounts.loginAction.loginHint"),
  };
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
  //   1. `accountConfigDir()` 对它返回 null ⇒ `resolveAccount` 说不出「用户显式选了账号 0」
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
 * 等加了第二个号就会突然满屏彩块。（变更记录 2026-07-25 已拍板。）
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
 * 每级候选必须 `isSelectable`(isolated + **鉴权前提就绪** + 目录在)否则**下沉**下一级;
 * (K-A1 起第二项不再是「已登录」——订阅号那一支等价，api-key 号不看凭据文件)
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
 * 仅当账号存在且**可选**（isolated + 鉴权前提就绪 + 目录在）才给；否则 null（不可选的绝不注入）。
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
 * 前端不硬编码 "0"。空串**不算**：那是非法拼法，backend 侧已挡掉。
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

/**
 * `N-F1b` `NF1bD2`：**本机那条路上的界面文案，只此一家。**
 *
 * # 为什么它住在这儿而不住面板里
 *
 * 远端那套文案早就住在本模块（`deriveUi` 那几句、`accountStatusBadge` 那一族），
 * 而面板只调。本机这一支要照同一个形状长，理由不是对称好看，是**它有一个真的对侧**：
 * 只要本机那句话散在面板里，「这一句在讲哪台机器」就没有任何东西钉着 ——
 * 而本仓已经有过一次教训，`accounts-section.ts` 那句
 * 「账号功能在远端 Linux 上——先在「连接」组配一台远端」是**唯一**能说的话，
 * 于是一台本来就有账号的机器上，用户看到的是「你先去买一台远端」。
 *
 * # 🔴 这里的每一句都不许出现「远端」两个字
 *
 * 判据住 `tests/frontend/ui/settings/accounts-section.vitest.ts` 里 `NF1bD2` 那一族：
 * 一条量**这张表**（人群 = `Object.values` 现算，不写死条数），
 * 一条量**真渲染出来的 DOM**（人群 = 本机那一支的叶子文本 + 全部 title）。
 * 两条都在，是因为「表里干净」与「用户看到的干净」是两件事：
 * 面板完全可以绕开这张表、就地写一句带「远端」的话，那时只有后一条会红。
 *
 * ⚠ **诚实边界**：判据管得住**这张表里的固定文案**；
 * 面板往里插的**动态值**（后端回的错误串、manifest 路径）不在人群里 ——
 * 它们不是本件写的字，本件也没有办法替后端保证措辞。
 *
 * ⚠ **尤其不许复用** `deriveUi` 那句「该远端尚未启用多账号」（本文件 `not-enabled` 那一支）：
 * 对一台本机来说那句话有两个字是假的。
 */
// 每一格是取值器：用到时才取文（模块顶层不留取文口调用，见 accountsOldBackend 那一句）。
export const LOCAL_ACCOUNTS_COPY = {
  /** 这一节的题头 —— 先把「在讲哪台机器」说清楚。 */
  get heading(): string {
    return copyText("accounts.local.heading");
  },
  /** 计数那一行的后半（前半是现算的数字）。 */
  get countSuffix(): string {
    return copyText("accounts.local.countSuffix");
  },
  /** 清单路径那一格的前缀。 */
  get manifestPrefix(): string {
    return copyText("accounts.local.manifestPrefix");
  },
  /** 一个隔离账号都没有时的正题。 */
  get emptyTitle(): string {
    return copyText("accounts.local.emptyTitle");
  },
  /**
   * 空态的下一步：账号库已建、还没有具名账号 ⇒ 指向同一页的新建表单（新建由后端执行）。
   */
  get emptyNext(): string {
    return copyText("accounts.local.emptyNext");
  },
  /** 读不出来时的正题 —— **不许**渲染成「你没有账号」。 */
  get loadFailed(): string {
    return copyText("accounts.local.loadFailed");
  },
  /** 后端连原因都没给时的兜底（`loadFailed` 后面那一格不许空着）。 */
  get unknownReason(): string {
    return copyText("accounts.local.unknownReason");
  },
  /** 当前账号那一行的标记。 */
  get currentMark(): string {
    return copyText("accounts.local.currentMark");
  },
  /**
   * 这一节的管辖范围。
   *
   * ⚠ 它只说**本件真做到的事**（把清单列出来），不替下一件许愿：
   * 切号 / 加号 / 走后端都还没接上，写进来就是一句提前兑现的话。
   */
  get scopeHint(): string {
    return copyText("accounts.local.scopeHint");
  },
} as const;

/**
 * F05：判别联合形态的账号解析结果——`AccountResolver` 目标（账本）。取代
 * "只吐 configDir、名字在解析完就被丢弃"的旧口径：`kind==="account"` 时同时带 `name` 和
 * `configDir`——线通给调用方后，`name` 才能继续往下传进 `LaunchContext`（F05 的核心交付：
 * 让 Rust `ccm_invocation.rs::DIMENSION_ORDER` 里 `account` 那一维说得出 `--account <名>`）。
 */
export type AccountResolution =
  | { kind: "account"; name: string; configDir: string }
  | { kind: "base" }
  /**
   * 要的那个号选不了 ⇒ **不起**（D-h）。`pinned` = 这个号是会话自己的 pin（跟随那一支），
   * 不是用户这一次点的。
   */
  | { kind: "unavailable"; requestedName?: string; pinned?: boolean };

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
    // 🔴 D-h（主会话 4D 裁，照「「哪个账号」非有不可 —— 缺了 resume 会静默落到默认号，撞 `D4`」）：
    //   会话有 pin、而 pin 那个号选不了 ⇒ **不下沉**，回 `unavailable`（调用方不起、说清、给「用当前账号」的显式选择）。
    //   先前这里下沉到当前号 / 基座、不说一个字（E7）—— 用另一个号的订阅或 key 续了这场会话。
    //   没有 pin 的会话照旧 当前号 → 基座（没有「原账号」，谈不上换号）。
    if (priorPin) {
      const a = state.accounts.find((x) => x.name === priorPin);
      if (!a || !isSelectable(a)) return { kind: "unavailable", requestedName: priorPin, pinned: true };
    }
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
 * 要的那个号选不了时，给用户的那个**显式选择**：当前账号（可选、且不是要的那个）；
 * 没有这样的号 ⇒ `null` = 「不指定账号」（落 `~/.claude` 那一份登录）。
 */
export function alternativeAccountOf(state: AccountsState, requested: string): string | null {
  const cur = currentWorkingAccount(state);
  return cur && isSelectable(cur) && cur.name !== requested ? cur.name : null;
}
