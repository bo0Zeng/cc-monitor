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
 *   ② 有 payload 但 TS 侧不读字段 ⇒ `unknown` **并在那一行注明**（当时 **3 个**：
 *      `sftp_stat` · 重建搜索索引那条（〔LOC1b〕随本机内存索引删了）· `start_forward`）；
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
// 〔US1 · 第四波 4D〕`ApikeyCredentialsStatus` 与 `ApikeyRoutingView` 两个类型搬进 `src/apikey-reads.ts`（那两问改走通道、后端出成品，
//   形状由跨语言金样 `tests/__fixtures__/apikey.golden.json` 两侧对拍）；本文件那条 `import type … from "../accounts"`（B-decouple §6
//   必须拆 4 点名的「闭合类型环的那条边」）随 `apikey_routing_for` 一起走了。

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
} from "../launch-cli-wire.ts";

import type { Alias } from "../generated/Alias";
import type { AliasInstallReport } from "../generated/AliasInstallReport";
import type { AliasListing } from "../generated/AliasListing";
import type { AliasRender } from "../generated/AliasRender";
import type { Shell } from "../generated/Shell";
import type { AcctIsoStatus } from "../generated/AcctIsoStatus";
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
import type { ConnectStage } from "../generated/ConnectStage";
import type { ConnTestResult } from "../generated/ConnTestResult";
import type { CcmProbeResult } from "../generated/CcmProbeResult";
// `K-R69`：本机那条 `ccm` 入口这一格（我们那一份 · PATH 上那一份 · 判词 · 那句话）。
import type { LocalCcmEntry } from "../generated/LocalCcmEntry";
import type { ConfigEdit } from "../generated/ConfigEdit";
import type { ConfigSurfaceReport } from "../generated/ConfigSurfaceReport";
import type { DriftLedgerReport } from "../generated/DriftLedgerReport";
import type { CcBusDeployReport } from "../generated/CcBusDeployReport";
import type { CcBusInstallState } from "../generated/CcBusInstallState";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
// 〔RM1f〕panorama 一族今天只剩 `panorama_call` / `panorama_edit` / `panorama_cancel` 三条，返回 `unknown`，
// 由 `src/panorama/api.ts` 按 op 收窄成 `src/panorama/types.ts` 的手写类型（那 10 个住 vendored
// `code-picture-core/src/model.rs`，`VENDOR.md` 铁律「副本是上游的镜子」⇒ 不在副本里加 `ts_rs` 派生）。
// 〔改前这里 import 那十几个类型给进程内那十七条包装用，`PanoramaStatus` 用生成物；三样都随内嵌引擎退役了。〕
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import type { TmuxSession } from "../generated/TmuxSession";
import type { ForwardStatus } from "../generated/ForwardStatus";
import type { HooksReport } from "../generated/HooksReport";
import type { JsonlLinePayload } from "../generated/JsonlLinePayload";
import type { SessionLinesPage } from "../generated/SessionLinesPage";
import type { PushResult } from "../generated/PushResult";
import type { LogFileInfo } from "../generated/LogFileInfo";
import type { RestartHint } from "../generated/RestartHint";
import type { SubagentLoadResult } from "../generated/SubagentLoadResult";

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
/** 〔STOP〕「停」的结局：三个词 ＋ 被停的 pid（没在跑 ⇒ `null`）。与 Rust `remote_resident::StopAnswer` 同形。 */
export interface StopAnswer {
  stopped: "graceful" | "killed" | "not_running";
  pid: number | null;
}

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

  /**
   * 写配置：只交「改哪几条路径」（〔CFG1〕整份替换的 `save_config` 删了）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 〔散文墓碑〕
   * 补丁的形状由生成物 `ConfigEdit` 钉（Rust `config.rs::ConfigEdit`）；值本身仍是不透明 JSON（见 `load_config`）。
   */
  patch_config: (args: { edits: ConfigEdit[] }) => invoke<void>("patch_config", args),

  /**
   * 写诊断配置。返回 `RestartHint` —— **它是个只有 unit variant 的外部标记枚举**
   * （`#[serde(rename_all = "snake_case")]`，没有 `tag`）⇒ 线上就是字符串
   * `"none"` / `"needs_restart"`，生成物给的正是那个字面量联合。
   */
  set_diagnostics_config: (args: { cfg: DiagnosticsConfig }) =>
    invoke<RestartHint>("set_diagnostics_config", args),

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
  // 〔HX2 · 第四波 4D〕墓碑：这里从前是 `write_apikey_credentials_key`〔散文墓碑〕（`K-H2a` `KS10`：从界面配一把 key）。
  //   写 key 改走通道 `apikey-key-set`（`src/apikey-reads.ts::writeApikeyKey`），交那台机器的后端；`KH2C1` 那一条照旧成立 ——
  //   前端交 `configDir`，账号 id 由后端按全仓唯一那份规则（`acct_core::apikey_account_id_of_dir`）推。

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
  // 〔AL2 · 第四波 4D〕六条都带 `origin`（本机远端同一条命令，`设计/71 §6`）；远端那两条装 / 卸别名块并进 `aliases_block_*`。
  aliases_render: (args: { origin: Origin; aliases: Alias[]; shell: Shell }) =>
    invoke<AliasRender>("aliases_render", args),

  /**
   * 〔AL1〕读回口：这台机器上那份别名文件今天有哪几条（认不出的行原文带原因列出来，不静默丢）。
   * 〔AL1d〕启动文件候选各带别名块的现状 ＋ 完成拉前握手的终端数；`rcPath` = 人另指的一份（过围栏后并进候选）。
   */
  aliases_read: (args: { origin: Origin; shell: Shell; rcPath?: string | null }) =>
    invoke<AliasListing>("aliases_read", args),

  /**
   * 〔AL1〕第②跳：**唯一的副作用**。收的是清单，后端用第①跳同一个渲染落盘 ⇒ 写的就是预览的那一份。
   * ⚠ `rcPath` 可选且没有默认值：用户的 shell 配置是哪一份只能由界面上的人选。
   */
  aliases_install: (args: { origin: Origin; aliases: Alias[]; rcPath?: string | null; shell: Shell }) =>
    invoke<AliasInstallReport>("aliases_install", args),

  /**
   * 〔AL1d · 第四波 4B〕**别名块**（`cc` / `cct` · `__ccm_bind`）第①跳：纯 —— 块 → 代码（装进一份空文件会写成什么）。
   * 两种方言都答，方言由 `rcPath` 那份文件的扩展名定（后端判，与装那一跳同一个判法）；`withCc` 只对 PowerShell 有意义。
   */
  aliases_block_render: (args: { origin: Origin; rcPath: string; withCc: boolean }) =>
    invoke<string>("aliases_block_render", args),

  /** 〔AL1d〕别名块装进人选的那份启动文件（方言按那份文件的扩展名定）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  aliases_block_install: (args: { origin: Origin; rcPath: string; withCc: boolean }) =>
    invoke<void>("aliases_block_install", args),

  /** 〔AL1d〕别名块卸掉（整块删，块外一个字节不动）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  aliases_block_remove: (args: { origin: Origin; rcPath: string }) =>
    invoke<void>("aliases_block_remove", args),

  /**
   * 〔RM1c · 第四波〕代码全景**经那台机器的后端**走（V108 选 B）：发帧命令 `panorama`，拿回 `result`。
   * `result` 的形状随 `op` 而定（与本机那几条逐字同形）⇒ 这里是 `unknown`，由 `panorama/api.ts` 按 op 收窄。
   */
  panorama_call: (args: {
    origin: Origin;
    op: string;
    repo: string | null;
    args: unknown;
    /** 〔RM1f〕给了 ⇒ 这一问能被 `panorama_cancel` 撤掉（每一问一张新票）。 */
    ticket?: string | null;
  }) => invoke<unknown>("panorama_call", args),

  /** 〔RM1f〕撤掉一问在飞的全景（「取消建索引」）。回那张票此刻在不在飞。 */
  panorama_cancel: (args: { ticket: string }) => invoke<boolean>("panorama_cancel", args),

  /**
   * 〔RM1d · 第四波〕批注 / 文档关联的**写**（V110「引擎只算、文件管理来写」）：问那台机器要编辑计划、
   * 经那台机器后端的文件管理落盘（本机远端同一条）。`op` ∈ `add_annotation` · `propose_annotation` ·
   * `approve_annotation` · `remove_annotation` · `write_doc_link` · `remove_doc_link`；回的值随 op 而定。
   */
  panorama_edit: (args: { origin: Origin; repo: string; op: string; args: unknown }) =>
    invoke<unknown>("panorama_edit", args),

  // 〔RM1f · V108 后半句〕本机那十七条进程内全景命令的包装随内嵌引擎退役删了（panorama_callees · panorama_callers ·
  //   panorama_diagram · panorama_diagram_kinds · panorama_docs_for · panorama_drift · panorama_impact · panorama_index ·
  //   panorama_list_annotations · panorama_node · panorama_overview · panorama_reindex · panorama_search · panorama_status ·
  //   panorama_subgraph · panorama_symbols_in_file · panorama_touching）：本机远端同一条 `panorama_call`。

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
   * 🔴 **〔步 12·C 2026-09-20〕`stream_read_remote_session`〔散文墓碑〕已退役，两条收成这一条。**〔LOC1b · 4D〕Rust 那一侧两支也合成了一条（本机也经本机后端 `history-read`）。
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

  // 〔SH1 · V136〕`read_cc_bus_inbox` / `read_cc_bus_state` 两条退役：驾驶舱读面经通道直接问后端（`src/cc-bus-control.ts`）。

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

  /** 读本机 MCP server 清单（user/local/project 三档）。Rust 签名**无 `Result` 包装**。 */
  // 〔US1 · 第四波 4D〕`read_apikey_credentials_status` / `apikey_routing_for` 退役：界面经通道直接问那台后端
  //   `apikey-read` / `apikey-routing`（`src/apikey-reads.ts`）。

  /**
   * 〔RL1 · 第四波〕这次拉起往 `ANTHROPIC_BASE_URL` 里写哪个中转地址（`null` = 不注入，照旧直连）。
   * apikey 号那台的中转不在 ⇒ reject（拒绝起会话，说得出是哪台）；中转只住那台的常驻后端里，不另起。
   * 〔US1〕判断在那台机器的后端（`launch-endpoint` 出成品），monitor 只转交、照成品执行；前端拿到地址原样放进载荷（`export-relay-base-url`）。
   * 〔V141〕不带会话身份：地址不随会话变，中转从 claude 的请求头认会话。
   */
  relay_endpoint_for_launch: (args: {
    origin: Origin;
    account: { kind: "base" } | { kind: "named"; configDir: string; name?: string } | null;
  }) => invoke<string | null>("relay_endpoint_for_launch", args),

  // 〔CF2 · 第四波 4B〕独立窗口的定向重放（`replay_session_to_window`〔散文墓碑〕）退役：独立窗口自己订
  //   `session-lines/<sid>`（`chan.subscribe`），留存由那条订阅当场交。

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
     * 取值口只有一个：`launch-account.ts::localLaunchAccountSync`（名字与目录同源）。
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

  // 〔C4c · 第四波 4B〕「resume 之前问记录还在不在」那一条退役：界面经通道直接问后端 `history-record`
  //   （`src/session-reads.ts::probeSessionRecord`，成品 `{present, root}`）。

  /** **本机今天有哪些 tmux 会话** —— 与远端 `list_remote_tmux` 同形（本机没有 SSH 那一跳）。
   *
   *  两个消费者：① 铸名时当 `existing`（P3t-Y2b）② 杀会话的菜单按 `@ccm_sid` 认归属（P3 刀 2 UI）。
   *
   *  ⚠ `null` 是「**不知道**」（本机后端通道没起 / 还没推过帧），**不是**「一个都没有」。
   *  拿 `null` 当空表：铸名那侧会不避让（issue #76），菜单那侧会说「没有会话」而其实有。
   *
   *  ⚠ **`command` 那一列可能陈旧**：它由 tmux hook 驱动刷新，而 hook 只有
   *  `session-created/closed/renamed` 三条 —— pane 前台命令从 claude 变回 shell **不触发任何一条**。
   *  ⇒ 依赖它判活的流程**不许**改读本机这条（〔V154〕当年那一条 `awaitExitFor` 已删）。 */
  list_local_tmux: () => invoke<TmuxSession[] | null>("list_local_tmux"),

  /** `K-R69`：**本机那条 `ccm` 入口现在是什么样** —— 我们放下去的那一份在哪、它自报什么身份、
   *  你 PATH 上那个 `ccm` 是不是它，以及给人读的那句话。`LocalCcmEntry` 是生成物 ⇒ **桶③**。
   *
   *  ⚠ 它**只读**：跑两次 `--ccm-probe`，一个字节都不写；产品也**不删**用户 `~/.local/bin/ccm`
   *  下那份旧的（用户逐字「原本的配置要手动删除」）。 */
  local_ccm_entry_status: () => invoke<LocalCcmEntry>("local_ccm_entry_status"),

  // 〔LOC1a · 第四波 4D · C4e 批 4〕某会话的任务快照那一条退役：界面经通道直接问那台机器的后端 `tasks-list`
  //   （后端出成品，`tasks-panel.ts::fetchSessionTasks` / `decodeTasks`）。

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
  // 〔SH1 · `设计/00 §2.5 ①`〕`acct-iso.check` 本机 / 远端两条合成这一条（带 `origin`；本机是 `"<local>"`）。
  acct_iso_status: (args: { origin: Origin }) => invoke<AcctIsoStatus>("acct_iso_status", args),

  /** 诊断配置（log 开关 / 级别 / error toast / 保留天数）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  get_diagnostics_config: () => invoke<DiagnosticsConfig>("get_diagnostics_config"),

  /** log 目录与文件清单。`current_size_bytes`/`size_bytes` 是**字节数**、`modified_ms` 是**毫秒时间戳**——两个量纲的上限论证在 Rust 侧分开写（C03 纪律）。 */
  get_log_file_info: () => invoke<LogFileInfo>("get_log_file_info"),

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
  //   本机与远端同一条路 —— `account-reads.ts::fetchSessionAccounts` 经通道 `chan.call(origin, "accounts-sessions", …)`。

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

  /** 把内嵌的 vendor `cc-acct-iso` 部署到远端。返回人话结果串 ⇒ 原始类型，无需生成物。 */
  deploy_remote_acct_iso: (args: { cfg: unknown; destDir: string }) =>
    invoke<string>("deploy_remote_acct_iso", args),
  /** Z05：抓那台 `cc-acct-iso shellinit` 的输出（只读）。返回带 BEGIN/END 围栏的 rc 片段。
   *  〔SH1 · `设计/00 §2.5 ①`〕本机 / 远端两条合成这一条（带 `origin`）。 */
  acct_iso_shellinit: (args: { origin: Origin }) => invoke<string>("acct_iso_shellinit", args),

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
   * 〔CF2 · 第四波 4B〕**按行号取一段正文** `[from, until)`（`until` 缺 ＝ 到末尾）—— 不依赖骨架索引的那条取回路。
   * 调用点：`TabStreamView.fetchBelow`（没接骨架的 tab 往上翻过了账本里最老那一条）· `TabStreamView.recoverFromGap`（会话流丢格之后往后补）。
   * 〔DL1〕`leftMs`：那一**件**事还剩多少毫秒（`设计/05 §3.3.2` 一件事一个绝对时刻；多问的那一件每问交剩下的，不重新计时）。
   */
  read_session_lines: (args: { origin: Origin; jsonlPath: string; from: number; until?: number; leftMs: number }) =>
    invoke<SessionLinesPage>("read_session_lines", args),

  // 〔CF2 · 第四波 4B〕「接上骨架 ⇒ 重放缓冲只留尾巴」那一条（`replay_keep_tail_only`〔散文墓碑〕）退役：
  //   重放缓冲对每个会话都只留尾巴，前端不再登记。

  // 〔C4b · 第四波 4B〕大纲清单与会话内查找那两条（`list_user_inputs` / `find_in_session`〔散文墓碑〕）退役：
  //   经通道直接说帧命令 `history-user-inputs` / `history-find`，后端出成品（`src/session-reads.ts`）。



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

  // 〔C4c · 第四波 4B〕「退出行为」那两条（〔B2 · 条 66〕问 / 交写，monitor 只转 ＋ 原样交回）退役：
  //   设置页经通道直接说后端 `exit-policy-read` / `exit-policy-set`（`settings/backend-section.ts::askExitPolicy` /
  //   `putExitPolicy`），形状仍由 `readExitAnswer` 收。

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

  /**
   * P2s（C8）：停这台机的后端 ⇒ **桶②**。〔STOP〕本机远端同形：那台机器上的一次性 `--resident-stop` 做「请它收尾 → 宽限期内等 → 到点强杀」，
   * 这里拿回结局（`remote_resident.rs::StopAnswer`）。
   */
  backend_stop: (args: { origin: string }) => invoke<StopAnswer>("backend_stop", args),

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
   * 这个 token 就是为那件事存在的：拿它去 `local-launch-backfill.ts::sidOfLaunch` 反查，
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
  }) => invoke<number>("open_file_window", args),

  /** 开独立设置窗口（非浮层）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_settings_window: () => invoke<void>("open_settings_window"),

  /** 卸远端后端。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  uninstall_remote_backend: (args: { cfg: unknown }) =>
    invoke<string>("uninstall_remote_backend", args),

  /** 用系统默认程序打开 monitor 的 log **目录**。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_dir: () => invoke<void>("open_log_dir"),

  /** 用系统默认程序打开 monitor 的 log 文件。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_file: () => invoke<void>("open_log_file"),

  // ════════════════════════════════════════════════════════════════════════
  // 〔C4a · 子步 2〕**最后十条**：原先在 `tab-session-actions.ts`（tab 层）与 `accounts.ts`
  // 里直呼裸 `invoke` 的那 16 处调的命令里，没进包装层的这十条。自此 **142/142** 条全部经本表，
  // 全仓 TS 只有本文件直呼 `invoke`（判据 `commands.vitest.ts` 末尾「裸 invoke 只在包装层」那一节）。
  // ════════════════════════════════════════════════════════════════════════

  /**
   * issue #10：把某会话在一个独立只读窗口（`viewer-<sid>`）里打开。`x`/`y` = 拖拽撕离的落点。**桶①**。
   * 〔CF2〕`origin`：窗口自己订那个会话的流（`session-lines/<sid>`），要知道它在哪台机器上。
   */
  open_session_in_new_window: (args: {
    sessionId: string;
    origin: Origin;
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

  // 〔C4a · 子步 3〕远端那条「某会话跑在哪个账号下」退役（子步 2 刚收进来，子步 3 连同本机那条一起改走通道）。
  // 〔C4c · 第四波 4B〕账号清单两条（远端 / 本机）与换号前的信任预检退役：前端经通道直接说帧命令
  //   `accounts-list` / `accounts-trust`（`src/account-reads.ts::fetchAccounts` / `checkTrust`），后端出成品。


  /**
   * 〔C4a · 子步 3〕**通道在 Tauri IPC 这一跳上的那条命令**（`chan/webview.rs`）。
   * ⚠ 调用方**不直接用它**：一律经 `src/ipc/chan.ts` 的 `chan.call(origin, op, payload, budget)`
   * （期限换算、本地撤单、三层错误解码都住那里）。载荷去程是字节数组、回程是原样字节（`ArrayBuffer`）。
   * **桶②**：回的是不透明字节，本表不认识它的形状。
   */
  chan_call: (args: { origin: Origin; op: string; payload: number[]; leftMs: number }) =>
    invoke<ArrayBuffer>("chan_call", args),

  /**
   * 〔NET2〕那台机器的能力事实（`chan/webview.rs::chan_offer`）：认哪些 op · 这台做不到哪几条（附码）·
   * 撤掉之后停得下的那几条。判断在 monitor 那边做完，本侧只查成员。`null` = 今天没有控制通道。
   * ⚠ 调用方经 `src/ipc/chan.ts` 的 `chan.offer` / `chan.cachedOffer` 用它。
   */
  chan_offer: (args: { origin: Origin }) =>
    invoke<{ ops: string[]; unavailable: [string, string][]; stoppable: string[] } | null>("chan_offer", args),

  /**
   * 〔CF2 · 第四波 4B〕**通道 `subscribe` 在 Tauri IPC 这一跳上的三条命令**（`chan/webview.rs`）。
   * ⚠ 调用方**不直接用它们**：一律经 `src/ipc/chan.ts` 的 `chan.subscribe(origin, kind, from, want, sink)`
   * （编号、窗口作用域的交格事件、解码都住那里）。`id` 由那一侧给（每页从 1 起）。**不回错**：说不了的在流里原位说。
   */
  chan_subscribe: (args: {
    origin: Origin;
    kind: string;
    from: number[] | null;
    want: number;
    id: number;
  }) => invoke<void>("chan_subscribe", args),
  chan_want: (args: { id: number; more: number }) => invoke<void>("chan_want", args),
  chan_stop: (args: { id: number }) => invoke<void>("chan_stop", args),
} as const;
