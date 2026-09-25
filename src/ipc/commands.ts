/**
 * C04a（rust-ts-boundary）：**类型化 `invoke` 包装层**。
 *
 * ## 为什么是手写的
 *
 * `ts-rs` 只生成**类型**，不生成命令签名（主计划 §8 的选型订正说明了这一点：
 * `tauri-specta` 会生成签名，但它对 Tauri 2 只有 `2.0.0-rc.1`，而本仓是要 Windows 打包发版的
 * 生产应用 ⇒ 不引入预发布依赖）。所以签名这一层手写，**漂移由守卫兜**
 * （`tests/ipc/commands.vitest.ts`）。
 *
 * ## 成文规则（主计划 §5）：名字钉死是普遍的，类型生成是按需的
 *
 * - **命令名**：119/119 全部纳入守卫。名字错了是运行时必错（`invoke` 直接 reject），
 *   与有没有人用返回值无关。
 * - **返回类型**：分**三桶**（Phase D 审计 Z2 订正——原来只写两桶，会把 34 个
 *   返回 `()` 的命令判成 `unknown`，那是净退化）：
 *   ① Rust 返回 `()` / `Result<(), _>` ⇒ `Promise<void>`（**34 个**）；
 *   ② 有 payload 但 TS 侧不读字段 ⇒ `unknown` **并在那一行注明**（**3 个**：
 *      `sftp_stat` · `rebuild_search_index` · `start_forward`）；
 *   ③ TS 侧真消费字段 ⇒ 生成物类型（**81 个**）。
 *
 * ## 本文件今天覆盖多少
 *
 * ~~**89 个命令**（C04a 样板 1 + C04d 批 1-6c 的 88）。其余 10 个仍走各模块里的裸 `invoke`。~~〔那是 C04d 时的读数〕
 * ★★〔C4a · 第四波 · 子步 2〕**全部**：Rust 侧每一条命令在本表里都有且只有一个条目（今天 142/142，
 * 数由 `commands.vitest.ts` 的包装层计数与 `RUST_COMMAND_COUNT` 两处钉着），
 * 全仓 TS **只有本文件**直呼 `invoke`（判据：`commands.vitest.ts` 末尾「裸 invoke 只在包装层」那一节
 * ＋ `generated-boundary-guard.vitest.ts` 的「直接 import invoke 的生产文件恰好 1 个」）。
 *
 * ~~**条目按字母序**~~〔现打：早就不是了 —— 各批按落地顺序追加〕。加新条目时追加即可。
 *
 * 〔C4a〕上一版这里写「守卫里绝不能写『每个命令都必须经过包装层』—— 那会假红」：
 * 那句话记的是还剩裸 `invoke` 的年代；今天那条**就是**判据（零命中带正控）。
 *
 * ## 守卫实际钉住的四条（别少说也别多说）
 *
 * 1. Rust 侧「`#[tauri::command]` 声明集 == `invoke_handler` 注册集」，且计数 == 119；
 * 2. 本文件的**键名** ⊆ Rust 命令集，且计数 == 包装层条目数；
 * 3. 本文件每个条目的**键名 == 它传给 `invoke` 的字符串字面量**
 *    （Phase D 审计的阻塞项：键不动、只把字面量抄成另一个**真实存在**的命令时，
 *    `tsc` 0 错、10 条守卫全绿，而运行时会调错命令并让消费方拿到 `null` 崩掉）；
 * 4. 全仓 TS **字面量**命令名 ⊆ Rust 命令集，且唯一名数 == 112，且
 *    「Rust 有而 TS 静态看不见」的那 7 个动态名逐字钉死。
 */
import { invoke, type Channel } from "@tauri-apps/api/core";

/**
 * devbench F03：一个接入的 skill 的当前状态。
 *
 * ⚠ **本类型是手写的**（不是 ts-rs 生成）—— 照 `launch-cli-wire.ts` 的先例。
 * ⇒ 字段名与 Rust 侧 `skill_host::SkillView`（serde 默认 snake_case）**必须手动同步**，
 * 由 Rust 侧的 `the_ts_view_type_matches_this_struct` 钉住：那条判据读本文件的源码，
 * 逐个字段对拍，漏一个就红。
 */
/**
 * `K-H2a` `KS6`：apikey 表那把第三方 API key 的**状态**。
 *
 * ⚠⚠ **这个类型里没有明文那个字段 —— 那是本件最要紧的一条，不是省略。**
 * `KS6` 逐字：一旦回显，key 就从「只住在后端」变成「**每次打开那个界面都往前端传一遍**」
 * ⇒ 泄漏面从一次变成无数次，每一次都新增前端日志 / 崩溃报告 / 截图 / 录屏四个出口。
 * ⇒ 要改 key 就**重新输**，前端永远拿不到旧值。
 *
 * ⚠ **本类型是手写的**（不是 ts-rs 生成）—— 照 `SkillView` 的先例。
 * 走手写而不是 `#[ts(export)]` 的理由是现打的（08-27）：`ts-rs` 导出会在 `src/generated/`
 * **新增一个文件**，而那个目录的清单由 `tests/generated-boundary-guard.vitest.ts`
 * 逐项等号对拍，那个文件不在 `K-H2a` 的写区。
 * ⇒ 字段与 Rust 侧 `creds_store::ApikeyCredentialsStatus`（serde 默认 snake_case）
 * **必须手动同步**，由 Rust 侧 `the_ts_status_type_matches_this_struct` **双向**对拍
 *（Rust 的字段名从结构体源码派生、TS 的从本接口体派生，**两边条数相等**，多一个少一个都红）。
 */
import type { ApikeyRoutingView, RawAccountsResult, TrustResult } from "../accounts";

export interface ApikeyCredentialsStatus {
  /** 配了没配。 */
  configured: boolean;
  /** 掩码形（前后各留几位；短到看不出前后缀的整条遮掉）。没配 = 空串。**永远不是明文。** */
  masked: string;
  /** 那份文件在哪 —— 给「我想自己拿编辑器改」的人看。 */
  path: string;
  /** 权限过宽 / 查不出来时的提醒（`KS11`：要在界面上显出来）。 */
  notice: string | null;
  /** 文件读坏了时的说法（人手编打错一个逗号）。 */
  problem: string | null;
}

export interface SkillView {
  id: string;
  label: string;
  /** `null` = 在场；否则是带身份的缺席原因（哪个 skill 的哪条前提没满足）。 */
  missing_reason: string | null;
  /** 实例名（planned-build 的工作区名…）。 */
  instances: string[];
  /** 可编辑文件的绝对路径（后端算好的，UI 直接拿去请求读/写）。 */
  editable: string[];
}
// U8c-2c-2：手写 wire 镜像（不是 ts-rs 生成的）——与 Rust 的一致性由 launch-cli-wire.vitest.ts 钉。
import type {
  CliRenderRequest,
  CliRenderResponse,
  PayloadRenderRequest,
  SendIntoRequest,
  SendIntoResponse,
} from "../launch-cli-wire.ts";

import type { Alias } from "../generated/Alias";
import type { AliasInstallReport } from "../generated/AliasInstallReport";
import type { AliasListing } from "../generated/AliasListing";
import type { AliasRender } from "../generated/AliasRender";
import type { Shell } from "../generated/Shell";
import type { AcctIsoStatus } from "../generated/AcctIsoStatus";
import type { ActiveSessionPayload } from "../generated/ActiveSessionPayload";
import type { AutoLaunchConfig } from "../generated/AutoLaunchConfig";
import type { BranchResult } from "../generated/BranchResult";
// 〔步 12·C〕合并后的命令一律收 `origin`。**`Origin` 是生成物**
// ⇒ 这里不许手写 `string | null`：手写的那一份与 Rust 的 `origin::Origin` 之间没有任何东西钉着。
//
// 🔴 **〔`设计/05 §8` 步 2 · 09-20〕`Origin.ts` 现在是 `string`，不再是 `null | string`。**
// 上一版这里逐字写着「生成物允许 `null`（那是「调用方没说」的线上形状），而合并后的命令
// 一条都不接受 `null` —— Rust 侧 `Origin::route` 当场拒」。那句话描述的是一道**运行时**闸：
// 类型上装得下 `null`，靠 Rust 在运行时挡。步 2 把它换成了**类型上装不下** ——
// `Origin::Unspecified` 那个变体退役，`null` 连反序列化都过不去
// ⇒ `§8` 承诺的「**`tsc` 就能验**」从这一行开始成立：给任何一个 origin 参数传 `null`
// 现在是**编译错**，不是一次运行时报错。
// ⚠ 本机要逐字送 `LOCAL_ORIGIN`（`"<local>"`，住 `../backend-policy.ts`）。
// ⚠ 射程：这一条只管**入方向**。〔C4a〕TS 侧其余各处的 origin 也收成了同一个表示（本机 = `LOCAL_ORIGIN`，
//   判据 `tests/ipc/commands.vitest.ts` 末尾「TS 侧 origin 去 null」那一节，全 TS ＋ 生成物）；
//   〔C4b〕生成物 `RemoteHealthPayload.origin` 那最后一处也改成了 `string`（那一节的 `PENDING` 从此为空）。
import type { Origin } from "../generated/Origin";
import type { SessionRecordProbe } from "../generated/SessionRecordProbe";
import type { CcBusMessage } from "../generated/CcBusMessage";
import type { CcBusState } from "../generated/CcBusState";
import type { ConnectStage } from "../generated/ConnectStage";
import type { ConnTestResult } from "../generated/ConnTestResult";
import type { CcmProbeResult } from "../generated/CcmProbeResult";
// `K-R69`：本机那条 `ccm` 入口这一格（我们那一份 · PATH 上那一份 · 判词 · 那句话）。
import type { LocalCcmEntry } from "../generated/LocalCcmEntry";
import type { ConfigSurfaceReport } from "../generated/ConfigSurfaceReport";
import type { DriftLedgerReport } from "../generated/DriftLedgerReport";
import type { CcBusDeployReport } from "../generated/CcBusDeployReport";
import type { CcBusInstallState } from "../generated/CcBusInstallState";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
// **panorama 一族的返回类型指向 `src/panorama/types.ts` 的手写类型，不是生成物。**
// 不是漏了——那 10 个类型（`Overview`/`NodeView`/`SubGraph`/`Edge`/`ImpactSet`/`Symbol`/
// `DocLink`/`Annotation`/`DriftItem`/`IndexStats`）住在 **vendored** 的
// `src/bridge/vendor/code-picture-core/src/model.rs`，而 `VENDOR.md` 有一条明写的铁律：
// 「**副本是上游的镜子，不是分身**（SS-10）：只照上游改，绝不在副本里改出自己的版本」。
// 给它们加 `ts_rs::TS` 派生就是在副本里改出自己的版本；而「先改上游再 re-vendor」要动
// `code-picture` 仓——**本会话在册的红线**。⇒ 按 §5 那条「名字钉死是普遍的、类型生成是按需的」，
// 本批只做**名字钉死 + 实参把关**，类型生成如实登记为结构性阻塞（BACKLOG E38）。
// `PanoramaStatus` 例外：它在 `panorama.rs`、是本仓自己的类型 ⇒ 已生成。
import type {
  Annotation,
  DiagramKindInfo,
  DiagramRequest,
  DocLink,
  DriftItem,
  Edge,
  ImpactSet,
  IndexStats,
  NodeView,
  Overview,
  PanoramaDiagram,
  SubGraph,
  Symbol as PanoramaSymbol,
} from "../panorama/types";
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import type { TmuxSession } from "../generated/TmuxSession";
import type { EntryMetadata } from "../generated/EntryMetadata";
import type { ForwardStatus } from "../generated/ForwardStatus";
import type { HistoryProject } from "../generated/HistoryProject";
import type { HistorySessionEntry } from "../generated/HistorySessionEntry";
import type { HooksReport } from "../generated/HooksReport";
import type { ImportGroup } from "../generated/ImportGroup";
import type { JsonlLinePayload } from "../generated/JsonlLinePayload";
import type { PanoramaStatus } from "../generated/PanoramaStatus";
import type { PushResult } from "../generated/PushResult";
import type { RemoteProjectsResult } from "../generated/RemoteProjectsResult";
import type { ResolvedHost } from "../generated/ResolvedHost";
import type { SearchIndexStatus } from "../generated/SearchIndexStatus";
import type { SearchResponse } from "../generated/SearchResponse";
import type { LogFileInfo } from "../generated/LogFileInfo";
import type { AssetsSynced } from "../generated/AssetsSynced";
import type { McpServerEntry } from "../generated/McpServerEntry";
import type { McpSyncApplied } from "../generated/McpSyncApplied";
import type { McpSyncPreview } from "../generated/McpSyncPreview";
import type { RestartHint } from "../generated/RestartHint";
import type { SessionActivityPayload } from "../generated/SessionActivityPayload";
import type { SubagentLoadResult } from "../generated/SubagentLoadResult";
import type { TaskEntry } from "../generated/TaskEntry";

/**
 * 类型化命令表。**键名必须逐字节等于 Rust 侧的命令名**，
 * **且必须逐字节等于本条目传给 `invoke` 的那个字面量**（两条都由守卫机检，见上）。
 *
 * 加新条目时：① 键名照抄 Rust 的 fn 名；② 返回类型按上面的三桶规则选；
 * ③ 把对应模块里的裸 `invoke` 换掉（否则等于两条路并存，比只有一条更糟）。
 *
 * **形状约束（主计划 §3 账本第 7 行）**：永远是**扁平的 命令名 → 函数** 映射。
 * 不许按模块嵌套（`commands.sftp.delete`），不许塞非命令键——动态派发之类的逃生口
 * 必须是**另一个导出**。塞了会被守卫第 2 条当场抓红（fail-safe）。
 */
/**
 * `K-R135`：用户级 PATH 那一格的现状。**手写**（理由见 `ccm_user_path_status` 那一条），
 * 字段名与 Rust 侧 `profile_installer::UserPathStatus` 的**线上名**由判据对拍。
 */
export interface UserPathStatus {
  /** 这台机器有没有「用户级 PATH」这一档 —— 它是 Windows 独有的。 */
  supported: boolean;
  /** 我们那个 bin 目录的绝对路径。探不动 ⇒ null。 */
  dir: string | null;
  /** 在不在用户级 PATH 上。🔴 `error` 非空时这一格恒 false，**别单看它**。 */
  onUserPath: boolean;
  /** 「加」那条命令的逐字文本（与按钮跑的是同一份字节）。 */
  addCommand: string | null;
  /** 「撤」那条命令的逐字文本。 */
  removeCommand: string | null;
  /** 探不动时的原话。🔴 **探不动 ≠ 不在 PATH 上**，界面必须把它显示出来。 */
  error: string | null;
}

export const commands = {
  /** 往 bus 上某个 agent 发一条消息。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。 */
  cc_bus_send: (args: { origin: string; id: string; text: string }) =>
    invoke<string>("cc_bus_send", args),
  /** P4c（#77/#78）：向**所有**已登记 agent 广播。爆炸半径大 —— UI 侧确认必须带数字。 */
  cc_bus_broadcast: (args: { origin: string; text: string }) =>
    invoke<string>("cc_bus_broadcast", args),
  /** P4c（#77/#78）：收掉一个 agent。**破坏性且不可撤销** —— UI 侧两步确认。 */
  cc_bus_kill: (args: { origin: string; id: string }) => invoke<string>("cc_bus_kill", args),

  /**
   * 在某目录派生一个协作 agent。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。
   * `account` 空串 = **显式基座**（后端翻成 `--base`）——**不存在「什么都不传」这一档**。
   */
  cc_bus_spawn: (args: {
    origin: string;
    dir: string;
    task: string;
    tool: string;
    // Rust 侧是 `Option<String>`；TS 侧传**空串**表示显式基座（后端翻成 `--base`）。
    account: string;
  }) => invoke<string>("cc_bus_spawn", args),

  /** 读 `cc_get_auto_launch`。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  cc_get_auto_launch: () => invoke<AutoLaunchConfig>("cc_get_auto_launch"),

  // 〔AL1d · 第四波 4B〕这里原来是「终端集成」那五条（`cc_integration_*`）：并进了 `aliases_*` 同一族命令面
  //   （状态 ＋ 扫一份 → `aliases_read` · 预览 → `aliases_block_render` · 装 / 卸 → `aliases_block_install` / `aliases_block_remove`）。

  /** 写 `cc_set_auto_launch`。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  cc_set_auto_launch: (args: { enabled: boolean }) => invoke<void>("cc_set_auto_launch", args),

  /**
   * `K-R135` / `R85`：用户级 PATH 那一格的**现状**。**现算，不缓存** ——
   * 每调一次后端真跑一趟 `powershell.exe`（几百 ms）⇒ **只在打开那一格 / 点刷新时调，不许轮询**。
   *
   * ⚠ **返回类型是手写的（`UserPathStatus`），这是一处有意的例外，理由写出来**：
   * 本文件头注那「三桶」规则说桶③（TS 真消费字段）该用生成物，而生成物要落进
   * `src/generated/`，那份目录**不在 `K-R135` 的写区里**。⇒ 手写一份，并且**不是裸奔的**：
   * Rust 侧有一条判据（`profile_installer` 的
   * `the_user_path_status_wire_fields_match_the_hand_written_ts`）把**线上字段名**
   * （serde 序列化一个样本实例现算出来的）与本文件这份接口逐字对拍 ⇒ 改了 Rust 不改这里当场红。
   * 哪天写区放开 `src/generated/`，换成生成物、把那条判据删掉。
   */
  ccm_user_path_status: () => invoke<UserPathStatus>("ccm_user_path_status"),

  /** `K-R135`：把我们那个 bin 目录**加**到用户级 PATH。桶①。 */
  ccm_user_path_add: () => invoke<void>("ccm_user_path_add"),

  /** `K-R135`：从用户级 PATH 上**只摘掉我们那一格**。桶①。 */
  ccm_user_path_remove: () => invoke<void>("ccm_user_path_remove"),

  /** 远端 `tmux capture-pane -p` 的画面文本。返回**原始类型**，无需生成物（桶③）。 */
  capture_remote_pane: (args: { origin: string; target: string }) =>
    invoke<string>("capture_remote_pane", args),

  /**
   * 前端性能日志落进 monitor 日志（无 devtools 环境下的唯一取证通道，grep `fe_perf`）。
   * Rust 侧无返回值 ⇒ **桶①** `Promise<void>`。
   *
   * **两个调用方**（`e2e-probe.ts` 与 `events.ts`）—— 这正是包装层的价值：
   * 原来两处各自手写这个命令名。
   */
  frontend_perf_log: (args: { lines: string }) => invoke<void>("frontend_perf_log", args),

  /** 设置面板「数据」区：枚举 monitor 写到磁盘的所有路径。返回值字段被真消费 ⇒ 用生成物（桶③）。 */
  get_data_paths: () => invoke<DataPathsResponse>("get_data_paths"),

  /** 探测远端有没有装 `ccm` CLI 及其能力集。返回**线上形状**（TS 侧另有领域类型）⇒ 桶③。 */
  probe_ccm_cli: (args: { origin: string }) => invoke<CcmProbeResult>("probe_ccm_cli", args),

  /** 写配置。Rust 返回 `Result<(), String>` ⇒ **桶①**。入参同样是不透明 JSON（见 `load_config`）。 */
  save_config: (args: { value: Record<string, unknown> }) => invoke<void>("save_config", args),

  /**
   * 写诊断配置。返回 `RestartHint` —— **它是个只有 unit variant 的外部标记枚举**
   * （`#[serde(rename_all = "snake_case")]`，没有 `tag`）⇒ 线上就是字符串
   * `"none"` / `"needs_restart"`，生成物给的正是那个字面量联合。
   */
  set_diagnostics_config: (args: { cfg: DiagnosticsConfig }) =>
    invoke<RestartHint>("set_diagnostics_config", args),

  /**
   * 全文搜索历史。`afterMs`/`limit` 在 Rust 侧是 `Option<i64>`/`Option<usize>` ⇒ `number | null`。
   * `afterMs` 是**毫秒时间戳**量纲（同 C03：2^53-1 ms ≈ 28.5 万年）。
   */
  search_history: (args: {
    query: string;
    includeTools: boolean;
    scope: string | null;
    afterMs: number | null;
    limit: number | null;
  }) => invoke<SearchResponse>("search_history", args),

  /**
   * devbench F03：列出接入的 skill 及其状态。
   *
   * `missingReason` 非 null 时是**带身份的**缺席原因（哪个 skill 的哪条前提没满足）——
   * UI 要原样显示它，不许退化成「不可用」（定框 C6）。
   */
  // 〔RW1 · 第四波 09-24〕三条都吃 `origin`（本机 = `"<local>"`）：远端项目的收件箱也能编辑，
  //   读写都经那台机器的后端（本机同一条路）。
  list_skills: (args: { origin: Origin; cwd: string }) =>
    invoke<SkillView[]>("list_skills", args),

  /** devbench F03：读一个 skill 的可编辑文件（读也过写面围栏，免得变成任意文件读取口）。 */
  read_skill_file: (args: { origin: Origin; cwd: string; skillId: string; path: string }) =>
    invoke<string>("read_skill_file", args),

  /**
   * devbench F03：写一个 skill 的可编辑文件。
   *
   * 后端三道围栏（路径 canonicalize 后做集合判定 · 过 Claude 数据保护守卫 · 目标必须已存在）
   * + 〔RW1〕经那台机器后端的文件管理那一面写（回读逐字节比对与回滚住后端）。**前端不做安全判断** ——
   * 判定的真相源只有 `skill_host::resolve_editable` / `remote_editable_rel`。
   * `expected` = 打开时读到的那一份（CAS：盘上那份在这之后被改过 ⇒ 一个字节不写）。
   */
  /**
   * `K-H2a` `KS10`：从界面配一把 key。
   *
   * ⚠ 它和**人手编那份文件**是同一份文件的两个写者 —— 后端在**写的那一刻**才读盘，
   * 未知键一个不吃、字段顺序按名字排、原子替换、写完立刻把文件收窄成只给本人。
   *
   * ⚠⚠ `K-H2c` `KH2C1`：**`configDir` 是承重的入参，别换成账号名。**
   * 那把 key 落进 `accounts.<账号 id>` 那一格，而 `<账号 id>` 由 Rust 用**全仓唯一那份规则**
   * （`history::apikey_account_id_of_dir`）从 `configDir` 推 —— 起会话那一侧调的是同一个函数。
   * 前端**一个字都不许自己推那个 id**（`split('/').pop()` 那一形）：那是在长第二份规则，
   * 漂开的那天症状是「设置里说走 apikey 端点改写、起会话时没走」，而两边看起来都没错。
   *
   * 〔RM1a · 第四波〕**收 `origin`**：key 落在会话跑的**那台机器**上 —— 本机进 monitor 自己那一份，
   * 远端交那台机器的后端写（`apikey-key-set`）。先前远端账号页配的 key 落在本机，远端会话用不上。
   */
  // 〔第四波 ST2〕`baseUrl`：加账号表单 apikey 那一支的 Base URL（`设计/70 §4.4`）；缺席 = 用默认上游。
  // 〔RM1a〕`origin`：key 与 Base URL 一起落在那台机器上（本机进 monitor 那一份，远端交那台的后端）。
  write_apikey_credentials_key: (args: {
    origin: Origin;
    key: string;
    configDir: string;
    baseUrl?: string | null;
  }) =>
    invoke<void>("write_apikey_credentials_key", args),

  write_skill_file: (args: {
    origin: Origin;
    cwd: string;
    skillId: string;
    path: string;
    content: string;
    expected: string;
  }) => invoke<void>("write_skill_file", args),

  /**
   * 〔AL1 · 2026-09-24〕`设计/71 §12.6` 第①跳：**纯** —— 清单 → 代码（＋ 每条的问题 ＋ 撞名提示）。
   * 预览与「复制去手贴」都只调这一条，后端一个字节都不写。
   */
  // 〔AL1c〕`shell` 必给：同一份清单渲染 / 读 / 写成哪种 shell 的方言（`71 §4.4`），不留缺省（缺了就是替人猜）。
  aliases_render: (args: { aliases: Alias[]; shell: Shell }) =>
    invoke<AliasRender>("aliases_render", args),

  /**
   * 〔AL1〕读回口：这台机器上那份别名文件今天有哪几条（认不出的行原文带原因列出来，不静默丢）。
   * 〔AL1d〕启动文件候选各带别名块的现状 ＋ 完成拉前握手的终端数；`rcPath` = 人另指的一份（过围栏后并进候选）。
   */
  aliases_read: (args: { shell: Shell; rcPath?: string | null }) =>
    invoke<AliasListing>("aliases_read", args),

  /**
   * 〔AL1〕第②跳：**唯一的副作用**。收的是清单，后端用第①跳同一个渲染落盘 ⇒ 写的就是预览的那一份。
   * ⚠ `rcPath` 可选且没有默认值：用户的 shell 配置是哪一份只能由界面上的人选。
   */
  aliases_install: (args: { aliases: Alias[]; rcPath?: string | null; shell: Shell }) =>
    invoke<AliasInstallReport>("aliases_install", args),

  /**
   * 〔AL1d · 第四波 4B〕**别名块**（`cc` / `cct` · `__ccm_bind`）第①跳：纯 —— 块 → 代码（装进一份空文件会写成什么）。
   * 两种方言都答，方言由 `rcPath` 那份文件的扩展名定（后端判，与装那一跳同一个判法）；`withCc` 只对 PowerShell 有意义。
   */
  aliases_block_render: (args: { rcPath: string; withCc: boolean }) =>
    invoke<string>("aliases_block_render", args),

  /** 〔AL1d〕别名块装进人选的那份启动文件（方言按那份文件的扩展名定）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  aliases_block_install: (args: { rcPath: string; withCc: boolean }) =>
    invoke<void>("aliases_block_install", args),

  /** 〔AL1d〕别名块卸掉（整块删，块外一个字节不动）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  aliases_block_remove: (args: { rcPath: string }) => invoke<void>("aliases_block_remove", args),

  /**
   * 〔RM1c · 第四波〕代码全景**经那台机器的后端**走（V108 选 B）：发帧命令 `panorama`，拿回 `result`。
   * `result` 的形状随 `op` 而定（与本机那几条逐字同形）⇒ 这里是 `unknown`，由 `panorama/api.ts` 按 op 收窄。
   */
  panorama_call: (args: { origin: Origin; op: string; repo: string | null; args: unknown }) =>
    invoke<unknown>("panorama_call", args),

  /**
   * 〔RM1d · 第四波〕批注 / 文档关联的**写**（V110「引擎只算、文件管理来写」）：问那台机器要编辑计划、
   * 经那台机器后端的文件管理落盘（本机远端同一条）。`op` ∈ `add_annotation` · `propose_annotation` ·
   * `approve_annotation` · `remove_annotation` · `write_doc_link` · `remove_doc_link`；回的值随 op 而定。
   */
  panorama_edit: (args: { origin: Origin; repo: string; op: string; args: unknown }) =>
    invoke<unknown>("panorama_edit", args),

  /** 某符号的被调者边。`depth` 是 `u32` ⇒ `number`。 */
  panorama_callees: (args: { repo: string; symbol: string; depth: number }) =>
    invoke<Edge[]>("panorama_callees", args),

  /** 某符号的调用者边。 */
  panorama_callers: (args: { repo: string; symbol: string; depth: number }) =>
    invoke<Edge[]>("panorama_callers", args),

  /** PN1b：画一张图（图种 id 来自注册表，不在本仓写死）。认不出的 kind / 缺符号都是 reject。 */
  panorama_diagram: (args: { repo: string; kind: string; request: DiagramRequest }) =>
    invoke<PanoramaDiagram>("panorama_diagram", args),

  /** PN1b：图种注册表原样透出（选图下拉从它现读，CP2）。 */
  panorama_diagram_kinds: () => invoke<DiagramKindInfo[]>("panorama_diagram_kinds"),

  /** 某符号关联的文档链接。 */
  panorama_docs_for: (args: { repo: string; symbol: string }) =>
    invoke<DocLink[]>("panorama_docs_for", args),

  /** 悬空文档链接清单。 */
  panorama_drift: (args: { repo: string }) => invoke<DriftItem[]>("panorama_drift", args),

  /** 某符号的影响面（blast radius）。 */
  panorama_impact: (args: { repo: string; symbol: string }) =>
    invoke<ImpactSet>("panorama_impact", args),

  /** 建/增量更新索引。 */
  panorama_index: (args: { repo: string }) => invoke<IndexStats>("panorama_index", args),

  /** F72：列全部批注（含 Proposed，给审批队列）。 */
  panorama_list_annotations: (args: { repo: string }) =>
    invoke<Annotation[]>("panorama_list_annotations", args),

  /**
   * 单符号视图。**Rust 返回 `Result<Option<NodeView>, String>`** ⇒ `NodeView | null`
   * （符号不存在时是 `null`，不是抛错）。
   */
  panorama_node: (args: { repo: string; symbol: string }) =>
    invoke<NodeView | null>("panorama_node", args),

  /** 全局概览（脊柱 / 子系统 / 入口点）。`budget` 是 `Option<usize>` ⇒ `number | null`。 */
  panorama_overview: (args: { repo: string; budget?: number | null }) =>
    invoke<Overview>("panorama_overview", args),

  /** 全量重建索引。 */
  panorama_reindex: (args: { repo: string }) => invoke<IndexStats>("panorama_reindex", args),

  /** 按名子串搜符号 → 拿全限定 id。`limit` 是 `Option<usize>` ⇒ `number | null`。 */
  panorama_search: (args: { repo: string; query: string; limit?: number | null }) =>
    invoke<PanoramaSymbol[]>("panorama_search", args),

  /** 索引状态（是否过期 / 建立时刻 / 符号数）。**这个类型是本仓的 ⇒ 用生成物。** */
  panorama_status: (args: { repo: string }) => invoke<PanoramaStatus>("panorama_status", args),

  /** 某符号周边子图。 */
  panorama_subgraph: (args: { repo: string; symbol: string; depth: number }) =>
    invoke<SubGraph>("panorama_subgraph", args),

  /** 某文件里的符号清单。 */
  panorama_symbols_in_file: (args: { repo: string; file: string }) =>
    invoke<PanoramaSymbol[]>("panorama_symbols_in_file", args),

  /**
   * 给定文件集（可带行范围）触及的符号 id。返回原始类型数组。
   *
   * **`ranges` 是我第一版漏掉的参数**：Rust 签名是 `ranges: Vec<(usize, usize)>`
   * （1-based `[start,end]`，空则整文件），而我提取参数的正则用了 `[^,]+?`
   * ——**被元组里的逗号截断了**。是包装层的精确签名让 `tsc` 当场报
   * 「'ranges' does not exist」才发现的。
   * ⇒ **量 Rust 签名时，参数类型里可能有逗号（元组/泛型），别用 `[^,]` 切。**
   */
  panorama_touching: (args: {
    repo: string;
    files: string[];
    ranges: [number, number][];
  }) => invoke<string[]>("panorama_touching", args),

  // 〔第四波 S4〕`sftp_copy`（远端内部复制，步 23b）的包装随那条命令退役删了：窗口的复制走后端 `files-copy`。

  // 〔F7c 收尾 09-24〕池子那十二条的包装一起走了（老面板删了、窗口改走通道；`设计/60 §13b`）：
  //   sftp_cancel_transfer · sftp_chmod · sftp_delete · sftp_download · sftp_list_dir · sftp_mkdir ·
  //   sftp_read_text_for_edit · sftp_realpath · sftp_rename · sftp_stat · sftp_upload · sftp_write_text。

  /** 起一条端口转发。Rust 返回 `Result<String, String>`（转发 id）⇒ 原始类型。 */
  start_forward: (args: {
    spec: { origin: string; localPort: number; remoteHost: string; remotePort: number };
  }) => invoke<string>("start_forward", args),

  /** 停一条端口转发。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  stop_forward: (args: { id: string }) => invoke<void>("stop_forward", args),

  /**
   * 测一条远端配置：连 SSH → 读指纹 → exec backend → 等 hello。
   *
   * **`onStage` 是 `Channel<ConnectStage>`**（第二个进包装层的 Channel 参数）。
   * `ConnectStage` 本轮一并生成——TS 侧 `describeStage` 里有 `const _never: never = st`
   * 穷尽性兜底，而**手写类型时 Rust 加一个 variant 并不会让它红**；
   * 换成生成物后那条 `never` 检查才真正对 Rust 的改动有牙。
   */
  test_remote_connection: (args: { cfg: unknown; onStage: Channel<ConnectStage> }) =>
    invoke<ConnTestResult>("test_remote_connection", args),

  /**
   * 流式读会话 jsonl。Rust 返回 `Result<u32, String>`（条数）。
   *
   * 🔴 **〔步 12·C 2026-09-20〕`stream_read_remote_session` 已退役，两条收成这一条。**
   *
   * 这一行原先逐字写着「**注意它与 `stream_read_session_jsonl` 的签名刻意不同**：
   * 远端这条 `origin: String` 是**必填**，本地那条**根本没有 origin**」——
   * 那句「刻意不同」正是 `设计/00 §2.5 ①` 要治的东西：**两侧走的不是同一条路，
   * 而让它们不同的只是一个参数**。
   *
   * ⚠ 它原先买到的那条编译期保护（「给本地命令传 origin」是编译错）**没有丢，是换了形状**：
   * 现在是「**不传** origin」编译错（`origin` 必填、且类型不是 `string | undefined`）。
   * ⇒ 本机要逐字送 `LOCAL_ORIGIN`，不许省。
   */
  stream_read_session_jsonl: (args: {
    origin: Origin;
    jsonlPath: string;
    onChunk: Channel<JsonlLinePayload[]>;
  }) => invoke<number>("stream_read_session_jsonl", args),

  /**
   * 流式列某项目的会话。
   *
   * 🔴 **〔步 12·C〕`stream_remote_history_sessions` 已退役，两条收成这一条。**
   * `projectDir` 两侧早已同形（`K-R97`：都是**编码目录名**，不是绝对路径）。
   */
  stream_history_sessions_in_project: (args: {
    origin: Origin;
    projectDir: string;
    onEntry: Channel<HistorySessionEntry>;
  }) => invoke<number>("stream_history_sessions_in_project", args),

  /**
   * 往远端 tmux 会话发按键。Rust 返回 `Result<(), String>` ⇒ **桶①**。
   * `enter` 缺省时 Rust 侧按 true 处理（`account-restart.ts` 有一处显式传 `false`）。
   */
  tmux_send_keys: (args: { origin: string; target: string; keys: string; enter?: boolean }) =>
    invoke<void>("tmux_send_keys", args),

  /** 读某 agent 的 inbox。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  read_cc_bus_inbox: (args: { origin: string; id: string }) =>
    invoke<CcBusMessage[]>("read_cc_bus_inbox", args),

  /** 读 bus 的完整状态（agents + spawned + 坏行数）。`skipped: usize` → `number`。 */
  read_cc_bus_state: (args: { origin: string }) => invoke<CcBusState>("read_cc_bus_state", args),

  /**
   * 把本机公钥推到远端 `authorized_keys`。返回值字段被真消费 ⇒ 生成物（桶③）。
   *
   * **这条是我漏掉又补回来的**：我用 `grep -P` 逐文件列调用点时，
   * 它写成**跨行**形式（`invoke<…>(\n  "push_public_key",`）而 grep 是**按行**匹配的
   * ⇒ 漏计一处。守卫里的 JS 正则跨行、一直数对（`toBe(112)` 含它）。
   * **临时 grep 比守卫弱，别拿它当账本。**
   */
  push_public_key: (args: { cfg: unknown; pubKeyPath: string | null }) =>
    invoke<PushResult>("push_public_key", args),

  /** 重建搜索索引。返回新状态 ⇒ 生成物（桶③）。 */
  rebuild_search_index: () => invoke<SearchIndexStatus>("rebuild_search_index"),

  /** 读本机 MCP server 清单（user/local/project 三档）。Rust 签名**无 `Result` 包装**。 */
  /**
   * `K-H2a` `KS6`：读 apikey 表那把 key 的状态。**返回里永远只有掩码。**
   * 〔RM1a〕收 `origin`：远端读的是**那台机器上**那一份（问那台的后端 `apikey-read`）。
   */
  read_apikey_credentials_status: (args: { origin: Origin }) =>
    invoke<ApikeyCredentialsStatus>("read_apikey_credentials_status", args),

  /**
   * `K-H2b` `KH2B7`：问「这几个 configDir 在 apikey 表里有没有行 · 中转在不在」。
   *
   * 〔RM1a · 第四波〕**收 `origin`**：两件事都问**那台机器**（远端由那台的后端答：
   * 表里有哪几行 `apikey-read` · 口上有没有人在听 `relay-status`）。先前「只答本机」的理由是
   * 「本机这一侧在结构上答不了远端那台」—— 今天远端那台自己答。
   * ⚠ 返回类型是**手写镜像**（`ApikeyRoutingView` 住 `src/accounts.ts`），
   * 与 Rust 的 `ApikeyRouting` **手动同步、今天没有判据对拍** —— 如实记，别读成有人守。
   */
  apikey_routing_for: (args: { origin: Origin; configDirs: string[] }) =>
    invoke<ApikeyRoutingView>("apikey_routing_for", args),

  /**
   * 〔RL1 · 第四波〕这次拉起往 `ANTHROPIC_BASE_URL` 里写哪个中转地址（`null` = 不注入，照旧直连）。
   * 远端那台**用到才起**它的中转；apikey 号的中转起不来 ⇒ reject（拒绝起会话，说得出是哪台）。
   * 判断只在后端 `payload::relay_endpoint_for` 一处；前端拿到地址原样放进载荷（`export-relay-base-url`）。
   * 它接替了 RM1a 那条零调用方的 `relay_ensure`。
   */
  relay_endpoint_for_launch: (args: {
    origin: Origin;
    account: { kind: "base" } | { kind: "named"; configDir: string; name?: string } | null;
    sid: string | null;
  }) => invoke<string | null>("relay_endpoint_for_launch", args),

  read_mcp_servers: (args: { projectDir: string | null }) =>
    invoke<McpServerEntry[]>("read_mcp_servers", args),

  /** 读远端的 MCP server 清单。 */
  read_remote_mcp_servers: (args: { origin: string }) =>
    invoke<McpServerEntry[]>("read_remote_mcp_servers", args),

  /** 读远端某项目目录的 `.mcp.json`。 */
  read_remote_project_mcp: (args: { origin: string; projectDir: string }) =>
    invoke<McpServerEntry[]>("read_remote_project_mcp", args),

  /**
   * 删项目 `.mcp.json` 里的一个 server。Rust 返回 `Result<(), String>` ⇒ **桶①**。
   *
   * 🔴 **〔步 12·C 收尾〕`remove_remote_mcp_server` 已退役，两条收成这一条。**
   * 本机要**逐字**送 `LOCAL_ORIGIN`（`"<local>"`）—— 省掉它就是线上 `null`，
   * 而 `null` ≠ 本机，Rust 侧 `Origin::route` 当场拒。
   */
  remove_project_mcp_server: (args: { origin: Origin; projectDir: string; name: string }) =>
    invoke<void>("remove_project_mcp_server", args),

  /** 把某 sid 的历史定向重放到当前窗口（viewer 用，不发 frontend-ready）。**桶①**。 */
  replay_session_to_window: (args: { sessionId: string }) =>
    invoke<void>("replay_session_to_window", args),

  /** `ssh -G` 解析一个别名。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  resolve_ssh_host: (args: { alias: string }) => invoke<ResolvedHost>("resolve_ssh_host", args),

  /** 在新终端 resume 一个历史会话。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  resume_history_session: (args: {
    sessionId: string;
    cwd: string;
    launcher: string | null;
    /**
     * G3b-1 / Phase G：本机拉起用哪个账号。**三态，别退回两态**：
     *
     * - 参数缺席 = 调用方**没表态** ⇒ 一个字都不注入（既有调用点逐字节等价旧行为）
     * - `{ kind: "base" }` = 用户**显式**选了账号 0 ⇒ 后端产出 `unset CLAUDE_CONFIG_DIR`
     * - `{ kind: "named", configDir, name? }` = 具名账号 ⇒ `export CLAUDE_CONFIG_DIR='…'`
     *
     * **「账号 0」不等于「什么都不加」**：本地拉起故意加载 shell rc，而 rc 里很可能有
     * `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是它）⇒
     * 什么都不加会静默落到别的账号上。远端那条路一直渲染成 `unset`，本地此前不是（Phase G 修）。
     *
     * 🔴 `K-R53`（09-11）：`named` 那一态**说得出名字就一起传**。后端那条 ccm 路只会
     * `--account <名字>`（`shared/ccm:606`）⇒ 不传名字 = 那次拉起**结构上到不了后端那条路**，
     * 必然落回第二实现（`history.rs::build_local_posix_command`，没有 tmux 容器）。
     * 取值口只有一个：`accounts.ts::localLaunchAccountSync`（名字与目录同源）。
     * `name` 缺席是**合法的**（例：分叉时继承的是源会话的目录、没有名字）—— 那时后端诚实短路，
     * **绝不从目录名反推**（推错 ⇒ `shared/ccm` 当场 `die`，一次能起的会话变成一条报错）。
     */
    account?: { kind: "base" } | { kind: "named"; configDir: string; name?: string };
    /**
     * P3t（`C12`）：**POSIX 本机**把会话建进 tmux 时的会话名。
     *
     * 缺席 ⇒ 后端诚实降级回旧路（`cc --resume <sid>`，不进容器）。
     * 传了也**只对 POSIX 本机生效** —— Windows 那一侧连读都不读（用户逐字：「windows不要tmux」）。
     *
     * ⚠ 只许传 `mintTmuxName` 铸出来的名字。它是全仓唯一带**撞名避让**的铸造口；
     * 自己拼一个 `<sid8>-cc` 就是 F13 修掉的那个坑（另一处精心让出 `-2`，你直接撞上去）。
     */
    tmuxName?: string | null;
  }) => invoke<void>("resume_history_session", args),

  /**
   * 〔U4b · 第四波〕**resume 之前问：这条会话的记录还在那台机器上吗**（`设计/01 §6.2` 最后一条）。
   * 判定住那台的后端（`history-record`，只收 sid）；本机与远端同一个口。**问不到 ⇒ reject**：
   * 调用方当「不知道」，**不当「不在」**。返回值字段被真消费 ⇒ 生成物（桶③）。
   */
  probe_session_record: (args: { origin: Origin; sessionId: string }) =>
    invoke<SessionRecordProbe>("probe_session_record", args),

  /** **本机今天有哪些 tmux 会话** —— 与远端 `list_remote_tmux` 同形（本机没有 SSH 那一跳）。
   *
   *  两个消费者：① 铸名时当 `existing`（P3t-Y2b）② 杀会话的菜单按 `@ccm_sid` 认归属（P3 刀 2 UI）。
   *
   *  ⚠ `null` 是「**不知道**」（本机后端通道没起 / 还没推过帧），**不是**「一个都没有」。
   *  拿 `null` 当空表：铸名那侧会不避让（issue #76），菜单那侧会说「没有会话」而其实有。
   *
   *  ⚠ **`command` 那一列可能陈旧**：它由 tmux hook 驱动刷新，而 hook 只有
   *  `session-created/closed/renamed` 三条 —— pane 前台命令从 claude 变回 shell **不触发任何一条**。
   *  ⇒ 依赖它判活的流程（换号重启的 `awaitExitFor`）**不许**改读本机这条。 */
  list_local_tmux: () => invoke<TmuxSession[] | null>("list_local_tmux"),

  /** `K-R69`：**本机那条 `ccm` 入口现在是什么样** —— 我们放下去的那一份在哪、它自报什么身份、
   *  你 PATH 上那个 `ccm` 是不是它，以及给人读的那句话。`LocalCcmEntry` 是生成物 ⇒ **桶③**。
   *
   *  ⚠ 它**只读**：跑两次 `--ccm-probe`，一个字节都不写；产品也**不删**用户 `~/.local/bin/ccm`
   *  下那份旧的（用户逐字「原本的配置要手动删除」）。 */
  local_ccm_entry_status: () => invoke<LocalCcmEntry>("local_ccm_entry_status"),

  /** 某会话的 TodoWrite 任务快照。`TaskEntry` C02 已生成 ⇒ **桶③**。 */
  // 〔RM1b · 第四波〕收 `origin`：问那台机器的后端 `tasks-list`（本机逐字 `LOCAL_ORIGIN`）。
  get_session_tasks: (args: { origin: Origin; sessionId: string }) =>
    invoke<TaskEntry[]>("get_session_tasks", args),

  /** 在远端起一个终端跑给定命令。Rust 返回 `Result<(), String>` ⇒ **桶①**。
   *  〔`设计/80 §8.7` 步 3 收尾，第二波 T4〕`rbindToken`：这次拉起铸的启动期令牌 ——
   *  后端据此在新窗口里先做一次令牌握手（本地 `token → HWND` 表的生产写入方）。
   *  可省（Rust 侧是 `Option<String>`）：账号部署那几个不起 agent 进程的调用方不带。 */
  launch_remote_terminal: (args: { origin: string; remoteCmd: string; rbindToken?: string | null }) =>
    invoke<void>("launch_remote_terminal", args),

  /**
   * 远端有没有装 `cc-acct-iso` + 命中路径 + 内嵌 vendor 指纹。
   *
   * **本批次抓到的漂移**：TS 侧原来写 `invoke<{ installed: boolean }>` —— 只认 1/3 个字段，
   * 把 `path` 与 `vendor_id` 藏掉了。而 Rust 那两个字段的注释明写「附带回传，
   * 避免以后要它时再加一趟往返」⇒ **是手写镜像把后端的好意抹掉了**。
   */
  check_remote_acct_iso: (args: { cfg: unknown }) =>
    invoke<AcctIsoStatus>("check_remote_acct_iso", args),

  /** 诊断配置（log 开关 / 级别 / error toast / 保留天数）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  get_diagnostics_config: () => invoke<DiagnosticsConfig>("get_diagnostics_config"),

  /** log 目录与文件清单。`current_size_bytes`/`size_bytes` 是**字节数**、`modified_ms` 是**毫秒时间戳**——两个量纲的上限论证在 Rust 侧分开写（C03 纪律）。 */
  get_log_file_info: () => invoke<LogFileInfo>("get_log_file_info"),

  /** 某个 bus agent 在不在线。返回 `Result<bool, String>` ⇒ 原始类型。 */
  check_cc_bus_agent_online: (args: { origin: string; id: string }) =>
    invoke<boolean>("check_cc_bus_agent_online", args),

  /** 部署内嵌的后端到远端。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。 */
  /** 部署远端后端（〔MC1〕连同 `ccm` 入口，一次）。 */
  deploy_remote_backend: (args: { cfg: unknown }) => invoke<string>("deploy_remote_backend", args),

  /**
   * 从某一轮建分支（F62）。返回值字段被真消费 ⇒ 生成物（桶③）。
   *
   * 〔`K-R88` 09-13〕入参从 `sourceJsonlPath` 收成 `sourceSessionId` ——
   * 与下面远端那条**形状一致**，两侧后端走的也是同一份「按 sid 找那份文件」。
   */
  create_branch_session: (args: {
    origin: Origin;
    sourceSessionId: string;
    messageUuid: string;
  }) => invoke<BranchResult>("create_branch_session", args),

  // 〔C4a · 子步 3〕E79 那条本机版「某会话跑在哪个账号下」退役：
  //   本机与远端同一条路 —— `accounts.ts::fetchSessionAccounts` 经通道 `chan.call(origin, "accounts-sessions", …)`。

  /**
   * 删历史会话。**桶①**。
   *
   * 🔴 **〔步 12·C〕`delete_remote_history_session` 已退役，两条收成这一条。**
   * 〔RW1 · 第四波〕两侧都经那台机器的后端删（`files-delete-session`，只收 sid）；
   * `jsonlPath` 是一致性闸的另一半：`sessionId` 必须恰是那份文件名的 stem。
   */
  delete_history_session: (args: {
    origin: Origin;
    sessionId: string;
    jsonlPath: string;
  }) => invoke<void>("delete_history_session", args),

  /**
   * G6：列远端 tmux 会话。`null` = 那台机器上没装 tmux（前端据此隐藏 attach 类操作）。
   * 返回值字段被真消费 ⇒ 生成物（桶③）。
   */
  list_remote_tmux: (args: { origin: string }) =>
    invoke<TmuxSession[] | null>("list_remote_tmux", args),

  /**
   * 一次「足迹」（原「配置面审计」）：只读、一次性，不新增轮询。返回值字段被真消费 ⇒ 生成物（桶③）。
   * 〔RM1a · 第四波〕**收 `origin`**：本机照旧在 monitor 进程里扫；远端问那台机器的后端要路径事实
   * （`footprint-probe`），判定走同一份 `build_rows`。远端那一栏的界面归 ST2 接。
   */
  config_surface_report: (args: { origin: Origin }) =>
    invoke<ConfigSurfaceReport>("config_surface_report", args),

  /**
   * 〔AS2 · 第四波 4B · V113〕资产目录同步：让本机常驻后端对 `origin` 那台做一趟「拉 · 并 · 推」
   * （本机那一页逐字 `LOCAL_ORIGIN` ⇒ 对它够得到的每一台各一趟）。回每一趟的结局 ＋ 可达表（origin ↔ 目录里的机器 id）。
   */
  assets_sync: (args: { origin: Origin }) => invoke<AssetsSynced>("assets_sync", args),
  // U-CC1：数据面漂移记账（只读、按需一次，不轮询）。
  // 〔ST3〕按机器分：问哪台答哪台，回包带回 `origin`（界面按回声判）。monitor 自己的命令，不经后端。
  drift_ledger_report: (args: { origin: Origin }) =>
    invoke<DriftLedgerReport>("drift_ledger_report", args),
  // 〔C4b · 第四波 4B〕P8a 的 marketplace 面（`list_plugin_marketplaces`〔散文墓碑〕）退役：经通道直接说帧命令
  //   `plugins-marketplaces`，后端出成品（`src/settings/plugins-section.ts::fetchSurvey`）。
  // PS1：把内嵌的 cc-bus 装到 `<claude_dir>/skills/cc-bus/`。
  // ⚠ **只读铁律的第 7 条例外**（`U10b` 用@08-13 裁「开」）⇒ 它是本仓**唯一**往
  // `<claude_dir>` 写的口子，必须由**用户显式点击**触发，绝不放进任何自动路径。
  deploy_local_cc_bus: () => invoke<CcBusDeployReport>("deploy_local_cc_bus"),
  // PS2：本机装的是哪一版（**只读**）。三态刻意不合并 ——
  // 「没装」「已是最新」「装了但不是这一版」合并任意两个都会骗人。
  cc_bus_install_state: () => invoke<CcBusInstallState>("cc_bus_install_state"),
  // U8c-2c-2：`ccm 调用行`改由 Rust 渲染（`backend::control::ccm_invocation`）。
  // **`ok:false` 不是错误，是诚实降级** —— 调用方拿着 `reason` 去走兜底渲染器（§33）。
  render_ccm_launch: (args: { req: CliRenderRequest }) =>
    invoke<CliRenderResponse>("render_ccm_launch", args),
  // U8a-2c-pre：兜底那支 `container:"none"` 的载荷也由 Rust 渲染（`backend::control::payload::render_payload`）。
  // 非法输入（空 configDir / shell 元字符 / 会裂的 arg）⇒ Rust 侧 `Err` ⇒ 这里 reject。
  render_launch_payload: (args: { req: PayloadRenderRequest }) =>
    invoke<string>("render_launch_payload", args),
  // 🔴 `K-R109`：**本机后端产「把终端接进那个会话」那一句**（`ccm attach <名>`）。
  // `R61` 裁定三〔用 09-13 逐字「归本机后端就好了啊」〕。
  // ⚠ 它**没有 `origin`**：本机后端就在这台机器上，问它要不必绕 ssh 那一跳
  //（远端那一侧的同一件事由 `render_ccm_launch` 的 `action:"attach"` 产）。
  // ⚠ 渲不出来 ⇒ Rust 侧 `Err` ⇒ 这里 reject。**调用方不许拿前端自己拼一条糊过去**：
  // 那就是 §31 最终形态第①条逐字禁的「前端硬编码后端命令」。
  render_local_attach: (args: { tmuxName: string }) =>
    invoke<string>("render_local_attach", args),

  // U8a-2c-1：**「控制搬进后端」的第一条生产通道** —— 往已存在的远端 tmux 会话键入载荷
  // （`send-keys` 那半边）。`attach` 那半边**不走它**：§1.3 要求最终 exec 落在用户自己的
  // 终端进程里，backend 在远端、开不了你面前的窗。
  backend_send_into: (args: { req: SendIntoRequest }) =>
    invoke<SendIntoResponse>("backend_send_into", args),

  /** 把内嵌的 vendor `cc-acct-iso` 部署到远端。返回人话结果串 ⇒ 原始类型，无需生成物。 */
  deploy_remote_acct_iso: (args: { cfg: unknown; destDir: string }) =>
    invoke<string>("deploy_remote_acct_iso", args),
  /** Z05：抓远端 `cc-acct-iso shellinit` 的输出（只读）。返回带 BEGIN/END 围栏的 rc 片段。 */
  remote_acct_iso_shellinit: (args: { cfg: unknown }) =>
    invoke<string>("remote_acct_iso_shellinit", args),
  /** 〔`A3` 第二波〕上面两条的**本机**对侧：问本机后端（`--acct-iso-status` / `--acct-iso-shellinit`）。
   *  出参与远端那条逐字相同。⚠ 今天**还没有界面调用点** —— 设置页账号那一节归 A2+ST1，
   *  接线在那边；这里先把口开好（账本 `acct-iso.check` / `acct-iso.shellinit` 两笔欠账随之结清）。 */
  check_local_acct_iso: () => invoke<AcctIsoStatus>("check_local_acct_iso"),
  local_acct_iso_shellinit: () => invoke<string>("local_acct_iso_shellinit"),

  /** 本机 cc-bus 钩子诊断。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  diagnose_local_cc_bus_hooks: () => invoke<HooksReport>("diagnose_local_cc_bus_hooks"),

  /** 远端 cc-bus 钩子诊断。同上。 */
  diagnose_remote_cc_bus_hooks: (args: { origin: string }) =>
    invoke<HooksReport>("diagnose_remote_cc_bus_hooks", args),

  /** 展开子 agent 折叠条时拉它的 jsonl。`records` 是 `JsonlRecord[]`（C04c 生成）⇒ 桶③。 */
  load_subagent: (args: {
    parentJsonlPath: string;
    description: string;
    toolUseTimestamp: string;
    /**
     * P7c-1：远端会话的 subagent 记录在远端机器上 —— 后端按它分流。
     *
     * 🔴 **〔`设计/05 §8` 步 2 · 09-20〕这一行原先是 `origin: string | null`，`null` = 本机。**
     * 那是全仓最后一处**在入方向的线上**用 `null` 表示本机的命令参数。
     * 步 2 逐字「`origin` 去 `null` 化 —— 本机也带 origin」⇒ 本机逐字送 `LOCAL_ORIGIN`
     * （`"<local>"`，住 `backend-policy.ts`）。Rust 那一侧同拍换成了 `origin::Origin`，
     * 而 `Origin` 里**已经没有**「没说」这一档（`null` 连反序列化都过不去）。
     */
    origin: Origin;
  }) => invoke<SubagentLoadResult>("load_subagent", args),

  // 〔C4b · 第四波 4B〕骨架索引那一条（`read_session_index`〔散文墓碑〕）退役：经通道直接说帧命令 `history-index`，
  //   后端出成品（`src/session-reads.ts::readSessionIndex`）。

  /**
   * 〔`设计/10` 骨架 · 子步 3〕**按偏移取一段正文** `[offset, until)` —— 两端、`seqBase`、`lineCount`
   * 都取自骨架索引（这一段第一行的 seq、这一段的可计行数）。「只物化可见区」要的那一段。
   */
  read_session_range: (args: {
    origin: Origin;
    jsonlPath: string;
    offset: number;
    until: number;
    seqBase: number;
    lineCount: number;
  }) => invoke<JsonlLinePayload[]>("read_session_range", args),

  /**
   * 〔U3b · `设计/10` 步 8〕这个会话**接上了骨架** ⇒ monitor 的重放缓冲只留尾巴（F5 之后也只重放尾巴，
   * 其余按偏移要回来）。返回这次丢掉的条数。**只许在骨架接上之后调**（唯一调用点：`tabs.ts` 的骨架接入）。
   */
  replay_keep_tail_only: (args: { sessionId: string }) =>
    invoke<number>("replay_keep_tail_only", args),

  // 〔C4b · 第四波 4B〕大纲清单与会话内查找那两条（`list_user_inputs` / `find_in_session`〔散文墓碑〕）退役：
  //   经通道直接说帧命令 `history-user-inputs` / `history-find`，后端出成品（`src/session-reads.ts`）。

  /**
   * 启动时先拉本地活跃会话建骨架 Tab。返回值字段被真消费 ⇒ 生成物（桶③）。
   * **线上是 snake_case**（`ActiveSessionPayload` 没有 `rename_all`），C04b 已论证过。
   */
  list_active_sessions: () => invoke<ActiveSessionPayload[]>("list_active_sessions"),

  /** `~/.ssh/config` 里的 host 别名清单（不展开 Include、不解析 Match）。 */
  list_ssh_host_aliases: () => invoke<string[]>("list_ssh_host_aliases"),

  /** 本机历史项目列表。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  list_history_projects: () => invoke<HistoryProject[]>("list_history_projects"),

  /**
   * 有 `.mcp.json` 的项目目录候选（`~/.claude.json` 的 `projects` 键）。
   *
   * 🔴 **〔步 12·C〕`list_remote_mcp_project_dirs` 已退役，两条收成这一条。**
   * 两侧算它的那一份代码本来就只有一份（Rust `project_dirs_from`），
   * 差别只在「那份 `~/.claude.json` 的字节从哪来」。
   */
  list_mcp_project_dirs: (args: { origin: Origin }) =>
    invoke<string[]>("list_mcp_project_dirs", args),

  /**
   * 每个 sid 最近一次用的账号（sid → 账号名）。Rust 返回 `HashMap<String, String>`
   * **无 `Result` 包装** ⇒ `Record<string, string>`，无需生成物。
   */
  list_last_accounts: () => invoke<Record<string, string>>("list_last_accounts"),

  /** 远端历史项目列表（含失败主机名单）。 */
  list_remote_history_projects: () => invoke<RemoteProjectsResult>("list_remote_history_projects"),

  /** 批量导入 `~/.ssh/config` 的预览分组（F57）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  import_ssh_hosts: () => invoke<ImportGroup[]>("import_ssh_hosts"),

  /** 往远端 rc 里装别名块（〔MC1〕从前叫「装 ccm 助手」）。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  install_remote_alias_block: (args: { cfg: unknown; profile: string }) =>
    invoke<string>("install_remote_alias_block", args),

  /** 搜索索引状态。Rust 签名**无 `Result` 包装**（`-> SearchIndexStatus`）。 */
  get_search_index_status: () => invoke<SearchIndexStatus>("get_search_index_status"),

  /** 杀掉远端某个 tmux 会话。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  kill_remote_tmux: (args: { origin: string; target: string }) =>
    invoke<void>("kill_remote_tmux", args),

  /** 当前活着的端口转发列表。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  list_forwards: () => invoke<ForwardStatus[]>("list_forwards"),

  /** 装了 MCP 的远端 host 列表。原始类型数组，无需生成物。 */
  list_remote_mcp_origins: () => invoke<string[]>("list_remote_mcp_origins"),

  /**
   * 读配置。**Rust 侧返回 `Result<serde_json::Value, String>`——它把配置当不透明 JSON 透传**，
   * 所以这个边界**结构性无法**由生成物加固：Rust 自己就不知道形状。
   * `Record<string, unknown>` 已经是最诚实的类型（TS 侧的 `Config` 就是它的别名）。
   * **这是一处如实登记的结构性缺口，不是我引入的缺陷。**
   */
  load_config: () => invoke<Record<string, unknown>>("load_config"),

  /**
   * 〔B2 · 条 66〕问那台机器的后端：「退出行为」那个值现在是什么（值住那台机器上，`设计/01 §3.3b`）。
   * Rust 那边是后端回的不透明 JSON（`state` / `killOnExit` / `reason` / `path`）⇒ **桶②**，
   * 形状由 `settings/backend-section.ts` 的 `readExitAnswer` 逐格取、缺一格就当问不到。
   * `origin` 本机是 `"<local>"`（见 `backend-policy.ts` 的 `LOCAL_ORIGIN`，两侧有判据对拍）。
   */
  backend_exit_policy: (args: { origin: string }) =>
    invoke<Record<string, unknown>>("backend_exit_policy", args),

  /**
   * 〔B2 · 条 66〕交那台机器的后端去写那个值；回**写完读回来的那一份**（同上一条的形状）⇒ **桶②**。
   * 前端从不碰那份文件 —— 这条命令就是前端改它的唯一一条路（`§3.3b ③`）。
   */
  set_backend_exit_policy: (args: { origin: string; kill: boolean }) =>
    invoke<Record<string, unknown>>("set_backend_exit_policy", args),

  /**
   * P2s（C8）：这台机的后端现在什么状态。Rust 那边是不透明 JSON（同 `load_config` 那处的
   * 结构性缺口）⇒ **桶②**：不为没人消费的字段造生成物。
   */
  backend_status: (args: { origin: string }) =>
    invoke<Record<string, unknown>>("backend_status", args),

  /**
   * P2s（C8）：**开关面该列哪几台机** —— 由后端的注册表说了算，前端不自己算。
   * 前端自己拼会与 Rust 的 origin 分叉四处（trim / 重复 label 后缀化 / 忽略 enabled /
   * 启动后新增的没注册），见 `backend_control::backend_machines` 头注。⇒ **桶②**。
   */
  backend_machines: () => invoke<string[]>("backend_machines"),

  /** P2s（C8）：起这台机的后端。返回一句人话（已起 / 已经在跑 / 起不来的理由）⇒ **桶②**。 */
  backend_start: (args: { origin: string }) => invoke<string>("backend_start", args),

  /** P2s（C8）：停这台机的后端。⚠ 远端返回的是「已断流」不是「已停进程」⇒ **桶②**。 */
  backend_stop: (args: { origin: string }) => invoke<string>("backend_stop", args),

  /**
   * 在某目录起一个**全新**本机会话。
   *
   * `account`：`K-H2b` `D1 阻-1` 加的。**三态**，与 `resume_history_session` 同形：
   * 缺席 = 调用方没表态（逐字节旧行为）· `base` = 用户显式选了账号 0 ·
   * `named` = 具名账号。⚠ 它**不是**「从某条旧会话继承账号」（那是 fork 的语义）——
   * 它是「用户此刻选中的当前账号」。没有它，这条主路上一个账号都说不出，
   * 后果有两条：起会话落到 shell rc 那个默认号上（静默串号），
   * 以及中转那一格**永远拼不出路由键**。
   *
   * # 🔴 返回值〔`K-P5h` `KP5HD1`〕：**这次拉起的身份 token**（不是 sid）
   *
   * Rust 侧从 `Result<(), String>` 改成 `Result<String, String>` ⇒ 这里从 `invoke<void>`
   * 改成 `invoke<string>`。`K-P5 §3 三` 现打「5 处起会话方没有一处在起新会话时知道 sid」——
   * 这个 token 就是为那件事存在的：拿它去 `accounts.ts::sidOfLaunch` 反查，
   * 起会话方才说得出「我刚起的那条是哪个会话」。
   * ⚠ 它是**内部 nonce**：不许显示给用户，也不许当 sid 用。
   */
  new_local_session: (args: {
    cwd: string;
    launcher: string | null;
    account?: { kind: "base" } | { kind: "named"; configDir: string; name?: string };
  }) => invoke<string>("new_local_session", args),

  /**
   * `24e` 第二刀（`设计/60 §4 戊` / `§5` 第三段）：在**原生窗口**（egui，同进程、次线程）
   * 里打开远端 `path` 这个目录。
   *
   * 回的是**这一趟列到的行数** ⇒ 桶③那一支里的「原始类型」（`number`，不用生成物）。
   *
   * 🔴 **它为什么不是「发个请求就回」**：Rust 侧**先真的把那个目录列出来**，
   * 列不出来就带着 `sftp_pool` 那边的原文 reject ⇒ 这一条 `await` 真的能失败，
   * 调用方该接住它并出声（`src/file-window.ts::openFileWindow`，全仓唯一调用点）。
   * 没有这一层的话「点了按钮什么都没发生」与「开成功了」在界面上分不开
   * —— 本机没有图形会话时那正是必然发生的事。
   *
   * ⚠ 它**不**保证「窗口出现在屏幕上」：那要一个图形会话，命令这一侧看不到。
   */
  open_file_window: (args: {
    cfg: unknown;
    path: string;
    /**
     * 🔴〔第十刀 2026-09-22〕**三者优先级与老面板逐字相同**：
     * `path`（非空）> `revealFile` > 远端 home。
     *
     * - `path` 非空 ⇒ 直接进那个**目录**（老面板 `initialDir`，F78）。
     * - `path` 空 ＋ `revealFile` = 一条远端**文件**绝对路径 ⇒ 进它父目录
     *   **并高亮那一行、滚进视野**（老面板 `revealPath`，F54）。
     *   ⚠ 父目录与尾段**由 Rust 侧算**（`filewin::source::parent_dir` /
     *   `remote_basename`）—— 前端不许自己切远端路径（那会长出第二份路径逻辑）。
     * - 两个都空 ⇒ 问远端 `realpath('.')`（第七刀）。
     */
    revealFile?: string | null;
    /**
     * 〔FW34〕〔待退役〕老 SFTP 面板留在 webview 里的目录书签（机器名 → 目录），开窗前并进
     * 原生窗口的书签文件（Rust 侧 `filewin::entry::carry_legacy`）；并不进去整趟报错、不开窗。
     * 没有旧书签就不带这一格。
     */
    carryBookmarks?: Record<string, string[]>;
  }) => invoke<number>("open_file_window", args),

  /** 开独立设置窗口（非浮层）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_settings_window: () => invoke<void>("open_settings_window"),

  /** 从远端 rc 里卸别名块。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  uninstall_remote_alias_block: (args: { cfg: unknown; profile: string }) =>
    invoke<string>("uninstall_remote_alias_block", args),

  /** 卸远端后端。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  uninstall_remote_backend: (args: { cfg: unknown }) =>
    invoke<string>("uninstall_remote_backend", args),

  /** 用系统默认程序打开 monitor 的 log **目录**。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_dir: () => invoke<void>("open_log_dir"),

  /** 用系统默认程序打开 monitor 的 log 文件。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_file: () => invoke<void>("open_log_file"),
  /**
   * 改一条会话的用户元数据（星标 / 自定义标题 / 隐藏 / 上次账号）。
   *
   * **`patch` 刻意不用生成物**：Rust 侧 `MetadataPatch` 的字段是 `Option<Option<String>>`
   * （双层 Option），语义**无法忠实映射到 TS**——`#[serde(default)]`（非 double_option）下
   * **JSON `null` 到不了 `Some(None)`**，外层直接是 `None`
   * ⇒ **`null` 的含义是「不改」，不是「清空」**。
   * 生成一个 `customTitle?: string | null` 会让人以为 `null` 是清空，那是**说谎的类型**。
   *
   * **⚠ 已登记的真 bug（BACKLOG E35，本轮刻意不修——修它是行为改动）**：
   * `views/history.ts` 的「留空恢复默认」传的正是 `null` ⇒ 后端**什么都不做**、标题清不掉。
   * Rust struct 注释里作者的意图是「清空走空串」，两边不一致。
   */
  update_history_metadata: (args: {
    sessionId: string;
    patch: {
      starred?: boolean;
      /** **注意语义**：`null` = **不改**（不是清空）。清空要传空串 `""`。见 E35。 */
      customTitle?: string | null;
      hidden?: boolean;
      /** 同上：`null` = 不改。 */
      lastAccount?: string | null;
    };
  }) => invoke<EntryMetadata>("update_history_metadata", args),

  /**
   * 写项目 `.mcp.json` 的一个 server。**桶①**。
   * `server` 是不透明 JSON（Rust 侧 `serde_json::Value`）⇒ `unknown`，与生成物一致。
   *
   * 🔴 **〔步 12·C 收尾〕`write_remote_mcp_server` 已退役，两条收成这一条。**
   * 本机同样要逐字送 `LOCAL_ORIGIN`，理由见上面 `remove_project_mcp_server`。
   */
  write_project_mcp_server: (args: {
    origin: Origin;
    projectDir: string;
    name: string;
    server: unknown;
  }) => invoke<void>("write_project_mcp_server", args),

  /**
   * 〔AS1 · 第四波 4B〕MCP 推 / 拉（`设计/96` 的 B）：看差异。`from` 那台 `fromDir` 的 `.mcp.json` 拷到
   * `to` 那台 `toDir` 会发生什么 —— 判定由 `to` 那台的后端做（`mcp-sync-plan`）。两台都是 `Origin`，本机逐字 `LOCAL_ORIGIN`。
   */
  mcp_sync_preview: (args: { from: Origin; fromDir: string; to: Origin; toDir: string }) =>
    invoke<McpSyncPreview>("mcp_sync_preview", args),
  /**
   * 〔AS1〕写：把勾的那几条原样合进 `to` 那台那份（`overwrite` = 对面不同、用户说了要盖的那几条）。
   * `sourceText` / `targetText` 原样送回看差异时拿到的那两份（后者是 CAS 期望：对面在那之后变了 ⇒ 一个字节不写）。
   */
  mcp_sync_apply: (args: {
    to: Origin;
    toDir: string;
    sourceText: string;
    targetText: string | null;
    take: string[];
    overwrite: string[];
  }) => invoke<McpSyncApplied>("mcp_sync_apply", args),

  // ════════════════════════════════════════════════════════════════════════
  // 〔C4a · 子步 2〕**最后十条**：原先在 `tab-session-actions.ts`（tab 层）与 `accounts.ts`
  // 里直呼裸 `invoke` 的那 16 处调的命令里，没进包装层的这十条。自此 **142/142** 条全部经本表，
  // 全仓 TS 只有本文件直呼 `invoke`（判据 `commands.vitest.ts` 末尾「裸 invoke 只在包装层」那一节）。
  // ════════════════════════════════════════════════════════════════════════

  /** issue #10：把某会话在一个独立只读窗口（`viewer-<sid>`）里打开。`x`/`y` = 拖拽撕离的落点。**桶①**。 */
  open_session_in_new_window: (args: {
    sessionId: string;
    title: string;
    x?: number;
    y?: number;
  }) => invoke<void>("open_session_in_new_window", args),

  /** 本机会话拉前（Windows 按 sid→HWND 缓存）。**桶①**。 */
  bring_terminal_to_front: (args: { sessionId: string }) =>
    invoke<void>("bring_terminal_to_front", args),

  /** 远端会话拉前（后端唯一分派点：先启动令牌、后标题退路）。**桶①**。 */
  bring_remote_terminal_to_front: (args: { sessionId: string }) =>
    invoke<void>("bring_remote_terminal_to_front", args),

  /** 关 tab 时让事件重放忘掉这个会话。**桶①**。 */
  forget_session: (args: { sessionId: string }) => invoke<void>("forget_session", args),

  /** 把监控窗口拉到最前。**桶①**。 */
  bring_monitor_to_front: () => invoke<void>("bring_monitor_to_front"),

  /** 某台远端的账号清单。**桶③**（手写形状 `RawAccountsResult`，住 `accounts.ts`，理由见那里）。 */
  list_remote_accounts: (args: { origin: Origin }) =>
    invoke<RawAccountsResult>("list_remote_accounts", args),

  /** 本机账号清单（问本机后端 `--list-accounts`）。**桶③**，同上。 */
  list_local_accounts: () => invoke<RawAccountsResult>("list_local_accounts"),

  // 〔C4a · 子步 3〕远端那条「某会话跑在哪个账号下」退役（子步 2 刚收进来，子步 3 连同本机那条一起改走通道）。

  /**
   * 换号前的目录信任预检。**桶③**（手写 `TrustResult`，住 `accounts.ts`）。
   * `configDir: null` = 问账号 0（后端走 `--account-trust-zero`）—— **绝不传空串**（Z01）。
   */
  check_account_trust: (args: { origin: Origin; configDir: string | null; cwd: string }) =>
    invoke<TrustResult>("check_account_trust", args),

  /** issue #23：红绿灯快照（启动 / F5 后拉一次做初始收敛）。**桶③**（生成物）。 */
  list_session_activity: () => invoke<SessionActivityPayload[]>("list_session_activity"),

  /**
   * 〔C4a · 子步 3〕**通道在 Tauri IPC 这一跳上的那条命令**（`chan/webview.rs`）。
   * ⚠ 调用方**不直接用它**：一律经 `src/ipc/chan.ts` 的 `chan.call(origin, op, payload, budget)`
   * （期限换算、本地撤单、三层错误解码都住那里）。载荷去程是字节数组、回程是原样字节（`ArrayBuffer`）。
   * **桶②**：回的是不透明字节，本表不认识它的形状。
   */
  chan_call: (args: { origin: Origin; op: string; payload: number[]; leftMs: number }) =>
    invoke<ArrayBuffer>("chan_call", args),
} as const;
