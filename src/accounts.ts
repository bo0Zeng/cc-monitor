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
import { commands } from "./ipc/commands";
import type { AuthKind } from "./generated/AuthKind";
import type { RemoteAccount } from "./generated/RemoteAccount";
import { loadConfig, saveConfig } from "./config";
import { isValidModelName } from "./shell-quote";
import type { LaunchModifiers } from "./launch-plan";

// ---- 账号的形状是**生成物**（K-A1），不再是一份手抄 ----
//
// 这里原先是一份手写 `interface Account` + 一行「对齐 A2（src-tauri/src/accounts.rs）的
// 返回结构」的注释。那句注释是**纪律，不是判据**：往任一侧加一个字段，全仓没有一条门禁会红
// （K-A1 Bx 复量确认：`RemoteAccount` 当时没有 `ts_rs::TS` derive，`src/generated/` 底下
// 也没有对应文件）。现在两侧由 `src/generated/RemoteAccount.ts` 对齐 ——
// 改 Rust 不跑 `npm run gen:types`，`generated-boundary-guard` 与 CI 的
// `git diff --exit-code -- src/generated/` 会红。
//
// 名字仍叫 `Account`（全仓几十处消费点读作「账号」，而 Rust 那侧叫 `RemoteAccount`
// 是因为它先有远端那一份）——**别名不是手抄**：字段一个都不在这儿重写。
export type Account = RemoteAccount;
export type { AuthKind };

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

/**
 * K-A1：**鉴权方式这一维不再阻塞这个号被选中吗。**
 *
 * 这是全仓**唯一**读 `loggedIn` 的地方（`KAY4` 的零命中守卫钉住这句话，
 * 判据住 `src/account-availability-guard.vitest.ts`）。
 *
 * 规则本身**不在这儿** —— 它住 `acct_core::auth_ready`，两个 Rust 生产者调它、
 * 把结果放进 `authReady` 字段。本函数只做一件事：**对面没说时回落到旧行为**。
 *
 * `authReady === undefined` 只有一种来因：**旧 daemon**（本字段之前的版本压根不出这个键，
 * monitor 会连任意版本的远端）。那时回落到 `loggedIn` = 逐字节旧行为。
 *
 * ⚠ **`KA6b`（诚实边界，第四轮补的标签）：这里回落到的 `loggedIn` 只是 stat 了一下
 * `.credentials.json` 在不在 —— 凭据过期 / 被吊销看不出来。**
 * 本件一格没改这件事：它改的是「按 kind 分别判」，不是「判得准不准」。
 * 真去验一次凭据归另一件（今天不存在、也没人认领；而且那要联网，撞用户 07-17
 * 「无 API key / 不联网」那条板）。
 * ⚠ 这个标签先前**只落在 Rust 侧 `auth_ready` 字段上**，`logged_in` 那一半只有实质、
 * 没有标签（D 阶段审计 `S3`）⇒ `grep KA6b` 找不到它那一半。Rust 侧那份头注不在第四轮
 * 写区里（改它会连带重写 `src/generated/RemoteAccount.ts` —— ts-rs 把 doc 一起导出），
 * 所以标签先补在 TS 这一侧**唯一读 `loggedIn` 的地方**，Rust 侧那一半交回 PM。
 * ⚠ 连带的诚实边界：旧 daemon 那一侧，一个 api-key 号会被判成「未登录的订阅号」
 * ——那是**看得见**的降级（徽章写「未登录」，用户能修：更新远端 daemon）。
 * 刻意**不**为它加一个 `authKindAware` 能力标记：新 daemon 恒出这两个键，
 * 那个标记的「不认识」分支在结构上不可达，写出来就是一段永远不跑的代码。
 */
function authReady(a: Account): boolean {
  return a.authReady ?? a.loggedIn;
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
 * 本件落地那一刻，它对**一部分号**就成了假话（本机、且中转表里有它那一行、且中转在跑
 * 的那些号，cc-monitor **真的**会替它配）。⇒ 按「这个号属于哪一半 / 那两个前置成不成立」
 * 分别说各自的话。
 *
 * | `relay` | 用户看到 | 那句话为什么是真的 |
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
 * 而新注册一条命令会让 `src-tauri/src/parity_ledger.rs` 的
 * `every_tauri_command_is_declared_in_the_ledger` 当场红（本轮实测过，报文点名了那条命令），
 * 那个文件不在 `K-H2b` 的写区。⇒ 两个生产调用点今天分别传 `{scope:"remote"}`（设置里那张表
 * 是**远端专用**的：`accounts-section.ts` 的 `reload` 对 `origin` 为空时直接早退）与
 * 「远端就 `{scope:"remote"}`、本机就缺席」（chip）。
 * **这是「本机那三档有实现、没接线」，别读成「接上了」。** 经过住件文件 `§4`。
 */
export type AccountRelayState =
  /** 远端那一半：`K-H2b` `§0e` 裁四明写不做 ⇒ 对它确实没人配端点。 */
  | { scope: "remote" }
  /** 本机那一半：两个前置各自成不成立。 */
  | { scope: "local"; hasRow: boolean; running: boolean };

/**
 * `K-H2b` `KH2B7` 的**产出方**：问后端「这几个**本机** configDir 走不走中转」。
 *
 * # 它为什么是一条只答本机的命令（而不是账号列表上的两个字段）
 *
 * 中转是**每台机器自己的一个进程**，注入的又是回环地址（自指）⇒ 「本机这台的中转
 * 在不在跑」这个问题，本机这一侧**在结构上答不了远端那台**。往账号列表里加字段，
 * 就是让远端那些行也带上两个这一侧答不出来的值。
 * 命令面的登记（`relay.routing`，`NaturallyAsymmetric`）写着同一条理由。
 *
 * ★〔第四拍〕**取数那一跳接上了**：走包装层 `commands.relay_routing_for`。
 * ⚠ 经过如实记：第三拍它退回过一次 —— 注册一条命令会同时动两个钉死计数
 * （`parity_ledger.rs` 5 个数 + `src/ipc/commands.vitest.ts` 两处 `144`），
 * 而后者当时不在写区。**那两个数是联动的**：注册了不调 ⇒ 前一个红；调了没注册 ⇒ 编不过。
 *
 * ⚠ **两个字段各自的射程，别读宽**：`routed` 说的是「中转表里有这一行」，
 * **不是**「那把 key 能用」；`running` 说的是「我们起过它而且没停过」，
 * **不是**「那个口上真有人听」。
 */
export interface RelayRoutingView {
  /** 传进去的那些 configDir 里，中转表里**有对应行**的那几个（原样回）。 */
  routed: string[];
  /** 本机中转在不在跑。 */
  running: boolean;
}

export async function fetchLocalRelayRouting(configDirs: string[]): Promise<RelayRoutingView> {
  return await commands.relay_routing_for({ configDirs });
}

/**
 * 把上面那份读数落到**一个账号**上。
 *
 * `configDir` 缺席（账号 0）⇒ `null`：账号 0 在 manifest 里没有目录名，
 * **推不出中转表里的 id** ⇒ 说不出就不表态（与 Rust 侧 `relay_account_id` 的三态同形）。
 */
export function localRelayStateFor(
  a: Account,
  routing: RelayRoutingView,
): AccountRelayState | undefined {
  if (!a.configDir) return undefined;
  return { scope: "local", hasRow: routing.routed.includes(a.configDir), running: routing.running };
}

export function accountStatusBadge(
  a: Account,
  relay?: AccountRelayState,
): AccountStatusBadge {
  if (a.mode === "in-place") {
    return {
      text: "逃生口",
      warn: true,
      title: "in-place 模式：cc-monitor 不支持对它按会话切号",
    };
  }
  if (a.authKind === "api-key") {
    const local = relay?.scope === "local" ? relay : null;
    if (local?.hasRow && local.running) {
      return {
        text: "api-key（经本机中转）",
        warn: false,
        title:
          "这个号在中转凭据文件里有一行，本机中转也在跑 —— 起本机会话时 cc-monitor 会把 " +
          "ANTHROPIC_BASE_URL 指向本机中转，由中转按账号换上这一行的 key。\n" +
          "⚠ 它保证的是「请求发得到中转、中转按这一行转发」；" +
          "那把 key 本身对不对、上游认不认，仍然要到 claude 那边才知道。",
      };
    }
    if (local?.hasRow) {
      return {
        text: "api-key（中转未运行）",
        warn: true,
        title:
          "这个号在中转凭据文件里有一行，但本机中转没在跑 —— 起会话会被**当场拒**" +
          "（不是静默失败：中转没起来与网络坏了在 claude 那边长得一模一样，" +
          "所以这一条在起会话那一侧就拦下来）。请先起本机后端。",
      };
    }
    // 三种「没配上」的成因，各说各的 —— **合成一句就等于又写下一句说不准的话**。
    const why =
      local != null
        ? "中转凭据文件里**没有这个账号的一行** ⇒ cc-monitor 不会替它配 base URL。" +
          "要用它：在那份 JSON 里给这个账号加一行（端点 + key），或者在该账号自己的 " +
          "shell 环境里配好第三方端点。"
        : relay?.scope === "remote"
          ? "cc-monitor 今天只给**本机**会话配 base URL；**远端**这一半还不做" +
            "（把 key 送到远端那台机器是另一件事）⇒ 这个号要用，得在远端那台机器上" +
            "自己配好第三方端点。"
          : "cc-monitor 只在两件事都成立时替它配端点（base URL）：① 中转凭据文件里有这个" +
            "账号 id 的一行；② 本机中转在跑。**这一处没被告知它属于哪一半、那两条成不成立**，" +
            "所以不替它下判断。";
    return {
      text: "api-key（未配置端点）",
      warn: true,
      title:
        "这个号用 API key 鉴权，不看 ~/.claude 里的订阅凭据 —— 所以它可以被设为当前账号、" +
        "会话也起得来。但请求要发得出去还差一格：" +
        why +
        "\n没配好就起会话，请求会在 claude 那边报鉴权失败。",
    };
  }
  if (!authReady(a)) {
    return {
      text: "未登录",
      warn: true,
      title: "该账号尚未登录——请在终端里用它 /login",
    };
  }
  return { text: "已登录", warn: false, title: "" };
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
      label: "打开终端",
      title: "用该账号打开一个远端终端（api-key 号不需要 /login，要在里面配好端点环境变量）",
    };
  }
  return {
    label: authReady(a) ? "登录终端" : "去登录",
    title: "用该账号打开一个远端终端（在里面 /login）",
  };
}

/** 某账号是否可被选为默认 / 用来起会话。 */
export function isSelectable(a: Account): boolean {
  // 账号 0（mode "bare"）在这里**天然落选**。
  //
  // ★ **Z02 订正了 Z01 在这儿写的一句错话**。Z01 写的是「从 UI 起它需要『显式 unset』的
  // 注入路径，而 launch-plan 今天只会 export」——**不对**：那条路径早就有了，两条渲染路各一份
  //   · CLI 路径：`ACCOUNT_DIMENSION.cliFlags` 对非 account 态吐 `--base`，
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
  // ★ **K-A1 把第二项从 `a.loggedIn` 换成了 `authReady(a)`。**
  // 订阅号那一支的值与 `loggedIn` **逐字节相同**（`acct_core::auth_ready` 的订阅分支就是
  // 「凭据文件在不在」）⇒ 订阅号一格没变，包括「缺凭据 ⇒ 不可选」那道保护（`KAY3`）。
  // 变的只有 api-key 号：它压根不用那个文件，所以不再因为缺文件而被判不可用（`KAY2`）。
  return a.mode === "isolated" && authReady(a) && a.exists;
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

/**
 * L3a（local-as-remote）：取**本机**的账号状态 —— `fetchAccounts` 的本地对侧。
 *
 * 后端 `list_local_accounts` 直接读 `$HOME/.claude-accts/accounts.json`（只读、不起进程），
 * **返回类型与远端那条逐字段相同** ⇒ 上层拿到的 `AccountsState` 形状一致，
 * 这正是 §40「本地 = 不走 ssh 的远端」在这一格上的意思。
 *
 * `origin` 用一个固定哨兵：本机只有一台，没有「哪台」这个维度。
 * 走同一个缓存 Map（TTL 相同）——本地读盘虽便宜，但缓存语义一致比省那点 IO 更要紧。
 */
export const LOCAL_ORIGIN = "__local__";

export async function fetchLocalAccounts(force = false): Promise<AccountsState> {
  const now = Date.now();
  const cached = accountsCache.get(LOCAL_ORIGIN);
  if (!force && cached && now - cached.at < ACCOUNTS_TTL_MS) return cached.value;

  let raw: RawAccountsResult;
  try {
    raw = await invoke<RawAccountsResult>("list_local_accounts");
  } catch (e) {
    // Rust 侧只有「取不到 HOME」才 Err；当作不可用而非崩溃（与远端那条同处理）。
    const state: AccountsState = {
      origin: LOCAL_ORIGIN,
      available: false,
      error: String(e),
      meta: null,
      accounts: [],
      defaultName: null,
      notice: null,
    };
    accountsCache.set(LOCAL_ORIGIN, { at: now, value: state });
    return state;
  }
  const defaultName = await getDefaultName();
  const state: AccountsState = {
    origin: LOCAL_ORIGIN,
    available: raw.available,
    error: raw.error,
    meta: raw.meta,
    accounts: raw.accounts ?? [],
    defaultName,
    notice: raw.notice ?? null,
  };
  accountsCache.set(LOCAL_ORIGIN, { at: now, value: state });
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
