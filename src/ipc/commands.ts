/**
 * C04a（rust-ts-boundary）：**类型化 `invoke` 包装层**。
 *
 * ## 为什么是手写的
 *
 * `ts-rs` 只生成**类型**，不生成命令签名（主计划 §8 的选型订正说明了这一点：
 * `tauri-specta` 会生成签名，但它对 Tauri 2 只有 `2.0.0-rc.1`，而本仓是要 Windows 打包发版的
 * 生产应用 ⇒ 不引入预发布依赖）。所以签名这一层手写，**漂移由守卫兜**
 * （`src/ipc/commands.vitest.ts`）。
 *
 * ## 成文规则（主计划 §5）：名字钉死是普遍的，类型生成是按需的
 *
 * - **命令名**：119/119 全部纳入守卫。名字错了是运行时必错（`invoke` 直接 reject），
 *   与有没有人用返回值无关。
 * - **返回类型**：分**三桶**（Phase D 审计 Z2 订正——原来只写两桶，会把 34 个
 *   返回 `()` 的命令判成 `unknown`，那是净退化）：
 *   ① Rust 返回 `()` / `Result<(), _>` ⇒ `Promise<void>`（**34 个**）；
 *   ② 有 payload 但 TS 侧不读字段 ⇒ `unknown` **并在那一行注明**（**4 个**：
 *      `sftp_stat` · `rebuild_search_index` · `start_forward` · `aggregate_usage_all`）；
 *   ③ TS 侧真消费字段 ⇒ 生成物类型（**81 个**）。
 *
 * ## 本文件今天覆盖多少
 *
 * **89 个命令**（C04a 样板 1 + C04d 批 1-6c 的 88）。
 * 其余 10 个仍走各模块里的裸 `invoke`（119 − 109 = 10），由 **C04d** 后续批次迁进来。
 *
 * **条目按字母序**（键名排序），加新条目时插到对的位置——这样 diff 只显示真正的新增。
 *
 * **所以守卫里绝不能写「每个命令都必须经过包装层」**——那会假红，而假红的守卫会被人关掉。
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

import type { AccountUsageProbeResult } from "../generated/AccountUsageProbeResult";
import type { AcctIsoStatus } from "../generated/AcctIsoStatus";
import type { ActiveSessionPayload } from "../generated/ActiveSessionPayload";
import type { AutoLaunchConfig } from "../generated/AutoLaunchConfig";
import type { BranchResult } from "../generated/BranchResult";
import type { CcBusMessage } from "../generated/CcBusMessage";
import type { CcBusState } from "../generated/CcBusState";
import type { CcPreviewResponse } from "../generated/CcPreviewResponse";
import type { CcStatusResponse } from "../generated/CcStatusResponse";
import type { ConnectStage } from "../generated/ConnectStage";
import type { ConnTestResult } from "../generated/ConnTestResult";
import type { CcmProbeResult } from "../generated/CcmProbeResult";
import type { ConfigSurfaceReport } from "../generated/ConfigSurfaceReport";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
// **panorama 一族的返回类型指向 `src/panorama/types.ts` 的手写类型，不是生成物。**
// 不是漏了——那 10 个类型（`Overview`/`NodeView`/`SubGraph`/`Edge`/`ImpactSet`/`Symbol`/
// `DocLink`/`Annotation`/`DriftItem`/`IndexStats`）住在 **vendored** 的
// `src-tauri/vendor/code-picture-core/src/model.rs`，而 `VENDOR.md` 有一条明写的铁律：
// 「**副本是上游的镜子，不是分身**（SS-10）：只照上游改，绝不在副本里改出自己的版本」。
// 给它们加 `ts_rs::TS` 派生就是在副本里改出自己的版本；而「先改上游再 re-vendor」要动
// `code-picture` 仓——**本会话在册的红线**。⇒ 按 §5 那条「名字钉死是普遍的、类型生成是按需的」，
// 本批只做**名字钉死 + 实参把关**，类型生成如实登记为结构性阻塞（BACKLOG E38）。
// `PanoramaStatus` 例外：它在 `panorama.rs`、是本仓自己的类型 ⇒ 已生成。
import type {
  Annotation,
  DocLink,
  DriftItem,
  Edge,
  ImpactSet,
  IndexStats,
  NodeView,
  Overview,
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
import type { TransferProgress } from "../generated/TransferProgress";
import type { PanoramaStatus } from "../generated/PanoramaStatus";
import type { ProfileScan } from "../generated/ProfileScan";
import type { SftpEntry } from "../generated/SftpEntry";
import type { PushResult } from "../generated/PushResult";
import type { RemoteProjectsResult } from "../generated/RemoteProjectsResult";
import type { ResolvedHost } from "../generated/ResolvedHost";
import type { SearchIndexStatus } from "../generated/SearchIndexStatus";
import type { SearchResponse } from "../generated/SearchResponse";
import type { LogFileInfo } from "../generated/LogFileInfo";
import type { McpServerEntry } from "../generated/McpServerEntry";
import type { RestartHint } from "../generated/RestartHint";
import type { SessionAccountsResult } from "../generated/SessionAccountsResult";
import type { SessionUsageRow } from "../generated/SessionUsageRow";
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
export const commands = {
  /**
   * 起一个 tmux 会话跑 `/usage` 并 capture-pane 抓屏。返回值字段被真消费 ⇒ 生成物（桶③）。
   * **解析是 TS 侧纯函数 `parseUsageCapture` 的职责**——`captured=true` 只代表拿到了文本。
   */
  account_usage: (args: { origin: string; accountName: string; launchPayload: string }) =>
    invoke<AccountUsageProbeResult>("account_usage", args),

  /**
   * 远端 daemon 服务端聚合的用量行（非流式，一次返 `Vec`）。返回值字段被真消费 ⇒ 生成物（桶③）。
   * **注意它没有 `Result` 包装**——Rust 签名是 `-> Vec<SessionUsageRow>`，失败在 Rust 内部吞成空表。
   */
  aggregate_remote_usage_all: () => invoke<SessionUsageRow[]>("aggregate_remote_usage_all"),

  /**
   * 本地用量聚合，**经 `Channel` 流式**逐行推（第一个进包装层的 Channel 参数）。
   *
   * Rust 返回 `Result<u32, String>`（处理了多少行）。TS 侧今天**不读它**——
   * 但按 §5 桶② 写 `unknown` 在这里是**过度**的：桶② 的用意是「不为没人消费的
   * **payload 结构**生成类型」，而这是个**原始类型**，写 `number` 零成本且更诚实。
   * **这是对三桶规则的一处细化**，已记进 C04d 计划。
   */
  aggregate_usage_all: (args: { onRow: Channel<SessionUsageRow> }) =>
    invoke<number>("aggregate_usage_all", args),

  /** 往 bus 上某个 agent 发一条消息。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。 */
  cc_bus_send: (args: { origin: string; id: string; text: string }) =>
    invoke<string>("cc_bus_send", args),

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

  /** PowerShell profile cc 集成：装。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  cc_integration_install: (args: {
    path: string;
    commandName: string;
    includeCcFunction: boolean;
  }) => invoke<void>("cc_integration_install", args),

  /** 预览将写入 profile 的代码（含 BEGIN/END marker）。 */
  cc_integration_preview: (args: { commandName: string; includeCcFunction: boolean }) =>
    invoke<CcPreviewResponse>("cc_integration_preview", args),

  /** 扫一个指定 profile 路径。 */
  cc_integration_scan_path: (args: { path: string; commandName: string }) =>
    invoke<ProfileScan>("cc_integration_scan_path", args),

  /** PowerShell profile cc 集成的总状态。`commandName` 在 Rust 侧是 `Option<String>` ⇒ 可省。 */
  cc_integration_status: (args?: { commandName?: string }) =>
    invoke<CcStatusResponse>("cc_integration_status", args),

  /** PowerShell profile cc 集成：卸。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  cc_integration_uninstall: (args: { path: string }) =>
    invoke<void>("cc_integration_uninstall", args),

  /** 写 `cc_set_auto_launch`。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  cc_set_auto_launch: (args: { enabled: boolean }) => invoke<void>("cc_set_auto_launch", args),

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

  /** F72：批准一条 Proposed 批注。 */
  panorama_approve_annotation: (args: { repo: string; id: string }) =>
    invoke<boolean>("panorama_approve_annotation", args),

  /** F72：新增批注，返回批注 id。 */
  panorama_add_annotation: (args: {
    repo: string;
    file: string;
    symbol: string | null;
    body: string;
    author: string;
  }) => invoke<string>("panorama_add_annotation", args),

  /** 某符号的被调者边。`depth` 是 `u32` ⇒ `number`。 */
  panorama_callees: (args: { repo: string; symbol: string; depth: number }) =>
    invoke<Edge[]>("panorama_callees", args),

  /** 某符号的调用者边。 */
  panorama_callers: (args: { repo: string; symbol: string; depth: number }) =>
    invoke<Edge[]>("panorama_callers", args),

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

  /** F72：提一条待审批注，返回 id。 */
  panorama_propose_annotation: (args: {
    repo: string;
    file: string;
    symbol: string | null;
    body: string;
    author: string;
  }) => invoke<string>("panorama_propose_annotation", args),

  /** 全量重建索引。 */
  panorama_reindex: (args: { repo: string }) => invoke<IndexStats>("panorama_reindex", args),

  /** F72：删批注。 */
  panorama_remove_annotation: (args: { repo: string; id: string }) =>
    invoke<boolean>("panorama_remove_annotation", args),

  /** 删一条文档链接。 */
  panorama_remove_doc_link: (args: { repo: string; doc: string; target: string }) =>
    invoke<boolean>("panorama_remove_doc_link", args),

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

  /** 写一条文档链接。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  panorama_write_doc_link: (args: { repo: string; doc: string; target: string }) =>
    invoke<void>("panorama_write_doc_link", args),

  /** 取消一次进行中的传输。Rust **无返回值**（`fn … -> ()`）⇒ **桶①**。 */
  sftp_cancel_transfer: (args: { transferId: string }) =>
    invoke<void>("sftp_cancel_transfer", args),

  /** 删远端文件/目录。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  sftp_delete: (args: { cfg: unknown; path: string; isDir: boolean }) =>
    invoke<void>("sftp_delete", args),

  /** 下载（带 `Channel<TransferProgress>` 进度）。**桶①**。 */
  sftp_download: (args: {
    cfg: unknown;
    remotePath: string;
    localPath: string;
    transferId: string;
    onProgress: Channel<TransferProgress>;
  }) => invoke<void>("sftp_download", args),

  /** 列目录。返回值字段被真消费 ⇒ 生成物（桶③；`size: u64` C03 已按字节数量纲论证）。 */
  sftp_list_dir: (args: { cfg: unknown; path: string }) =>
    invoke<SftpEntry[]>("sftp_list_dir", args),

  /** 建远端目录。**桶①**。 */
  sftp_mkdir: (args: { cfg: unknown; path: string }) => invoke<void>("sftp_mkdir", args),

  /** 读远端文本供编辑。Rust 返回 `Result<Option<String>, String>` ⇒ `string | null`。 */
  sftp_read_text_for_edit: (args: { cfg: unknown; path: string }) =>
    invoke<string | null>("sftp_read_text_for_edit", args),

  /** 解析远端真实路径（`realpath`）。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  sftp_realpath: (args: { cfg: unknown; path: string }) =>
    invoke<string>("sftp_realpath", args),

  /** 远端重命名/移动。**桶①**。 */
  sftp_rename: (args: { cfg: unknown; from: string; to: string }) =>
    invoke<void>("sftp_rename", args),

  /**
   * stat 一个远端路径。**返回 `unknown`（§5 桶②）而不是生成物**：
   * `SftpStat` 是 C03 **刻意跳过**没生成的那一个——TS 侧一直是裸 `invoke` 无类型参数、
   * **字段没人读**，为它生成类型就是「为假想消费者建抽象」。
   * 这里写 `unknown` 并在这一行注明，不留下让人以为「漏了」的空白。
   */
  sftp_stat: (args: { cfg: unknown; path: string }) => invoke<unknown>("sftp_stat", args),

  /** 上传（带 `Channel<TransferProgress>` 进度）。**桶①**。 */
  sftp_upload: (args: {
    cfg: unknown;
    localPath: string;
    remotePath: string;
    transferId: string;
    onProgress: Channel<TransferProgress>;
  }) => invoke<void>("sftp_upload", args),

  /** 写远端文本。**桶①**。 */
  sftp_write_text: (args: { cfg: unknown; path: string; content: string }) =>
    invoke<void>("sftp_write_text", args),

  /** 起一条端口转发。Rust 返回 `Result<String, String>`（转发 id）⇒ 原始类型。 */
  start_forward: (args: {
    spec: { origin: string; localPort: number; remoteHost: string; remotePort: number };
  }) => invoke<string>("start_forward", args),

  /** 停一条端口转发。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  stop_forward: (args: { id: string }) => invoke<void>("stop_forward", args),

  /**
   * 测一条远端配置：连 SSH → 读指纹 → exec daemon → 等 hello。
   *
   * **`onStage` 是 `Channel<ConnectStage>`**（第二个进包装层的 Channel 参数）。
   * `ConnectStage` 本轮一并生成——TS 侧 `describeStage` 里有 `const _never: never = st`
   * 穷尽性兜底，而**手写类型时 Rust 加一个 variant 并不会让它红**；
   * 换成生成物后那条 `never` 检查才真正对 Rust 的改动有牙。
   */
  test_remote_connection: (args: { cfg: unknown; onStage: Channel<ConnectStage> }) =>
    invoke<ConnTestResult>("test_remote_connection", args),

  /**
   * 流式读**远端**会话 jsonl（issue #16，SSH 拉取）。Rust 返回 `Result<u32, String>`（条数）。
   *
   * **注意它与 `stream_read_session_jsonl` 的签名刻意不同**：远端这条 `origin: String` 是
   * **必填**，本地那条**根本没有 origin**。此前 TS 侧用一个三元选命令名 + 给两边传同一个
   * 超集 args（本地传 `origin: undefined` 靠 Tauri 丢掉）——包装层收成两条精确签名后，
   * 「给本地命令传 origin」变成**编译期错误**。
   */
  stream_read_remote_session: (args: {
    jsonlPath: string;
    origin: string;
    onChunk: Channel<JsonlLinePayload[]>;
  }) => invoke<number>("stream_read_remote_session", args),

  /** 流式读**本地**会话 jsonl。**无 origin 参数**（见上条）。 */
  stream_read_session_jsonl: (args: {
    jsonlPath: string;
    onChunk: Channel<JsonlLinePayload[]>;
  }) => invoke<number>("stream_read_session_jsonl", args),

  /**
   * 流式列**远端**某项目的会话（issue #16）。**`origin` 必填**——与下面本地那条的
   * Rust 签名不同（同批 6a 的两个 `stream_read_*`）。
   */
  stream_remote_history_sessions: (args: {
    projectDir: string;
    origin: string;
    onEntry: Channel<HistorySessionEntry>;
  }) => invoke<number>("stream_remote_history_sessions", args),

  /** 流式列**本机**某项目的会话。**无 origin 参数**（见上条）。 */
  stream_history_sessions_in_project: (args: {
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
  read_mcp_servers: (args: { projectDir: string | null }) =>
    invoke<McpServerEntry[]>("read_mcp_servers", args),

  /** 读远端的 MCP server 清单。 */
  read_remote_mcp_servers: (args: { origin: string }) =>
    invoke<McpServerEntry[]>("read_remote_mcp_servers", args),

  /** 读远端某项目目录的 `.mcp.json`。 */
  read_remote_project_mcp: (args: { origin: string; projectDir: string }) =>
    invoke<McpServerEntry[]>("read_remote_project_mcp", args),

  /** 删本机项目 `.mcp.json` 里的一个 server。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  remove_project_mcp_server: (args: { projectDir: string; name: string }) =>
    invoke<void>("remove_project_mcp_server", args),

  /** 删远端项目 `.mcp.json` 里的一个 server。**桶①**。 */
  remove_remote_mcp_server: (args: { origin: string; projectDir: string; name: string }) =>
    invoke<void>("remove_remote_mcp_server", args),

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
     * - `{ kind: "named", configDir }` = 具名账号 ⇒ `export CLAUDE_CONFIG_DIR='…'`
     *
     * **「账号 0」不等于「什么都不加」**：本地拉起故意加载 shell rc，而 rc 里很可能有
     * `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是它）⇒
     * 什么都不加会静默落到别的账号上。远端那条路一直渲染成 `unset`，本地此前不是（Phase G 修）。
     */
    account?: { kind: "base" } | { kind: "named"; configDir: string };
  }) => invoke<void>("resume_history_session", args),

  /** 某会话的 TodoWrite 任务快照。`TaskEntry` C02 已生成 ⇒ **桶③**。 */
  get_session_tasks: (args: { sessionId: string }) =>
    invoke<TaskEntry[]>("get_session_tasks", args),

  /** 在远端起一个终端跑给定命令。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  launch_remote_terminal: (args: { origin: string; remoteCmd: string }) =>
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

  /** 部署内嵌的 daemon 到远端。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。 */
  deploy_remote_daemon: (args: { cfg: unknown }) => invoke<string>("deploy_remote_daemon", args),

  /** 从某一轮建分支（F62）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  create_branch_session: (args: { sourceJsonlPath: string; messageUuid: string }) =>
    invoke<BranchResult>("create_branch_session", args),

  /**
   * G6：**远端**分叉——经 ssh 让 daemon 在那台机器上分叉。返回体与本地那条同形（桶③）。
   * 参数刻意收 `sourceSessionId` 而**不是**路径：daemon 只认 sid（少一个可构造的路径入参）。
   */
  /**
   * E79：**本机**版「某会话现在跑在哪个账号下」——远端 `--session-accounts` 的对侧。
   * **Linux 才有**（要读 `/proc/<pid>/environ`）；别的平台返回 `available:false` + 原因，
   * 不是静默空表。返回值字段被真消费 ⇒ 生成物（桶③）。
   */
  list_local_session_accounts: () =>
    invoke<SessionAccountsResult>("list_local_session_accounts"),

  create_remote_branch_session: (args: {
    origin: string;
    sourceSessionId: string;
    messageUuid: string;
  }) => invoke<BranchResult>("create_remote_branch_session", args),

  /** 删本机历史会话（带 projects 目录内的路径守卫）。**桶①**。 */
  delete_history_session: (args: { sessionId: string; jsonlPath: string }) =>
    invoke<void>("delete_history_session", args),

  /**
   * G6：列远端 tmux 会话。`null` = 那台机器上没装 tmux（前端据此隐藏 attach 类操作）。
   * 返回值字段被真消费 ⇒ 生成物（桶③）。
   */
  list_remote_tmux: (args: { origin: string }) =>
    invoke<TmuxSession[] | null>("list_remote_tmux", args),

  /** 删远端历史会话。**桶①**。 */
  delete_remote_history_session: (args: { origin: string; jsonlPath: string }) =>
    invoke<void>("delete_remote_history_session", args),

  /** 一次配置面审计（只读、一次性，不新增轮询）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  config_surface_report: () => invoke<ConfigSurfaceReport>("config_surface_report"),

  /** 把内嵌的 vendor `cc-acct-iso` 部署到远端。返回人话结果串 ⇒ 原始类型，无需生成物。 */
  deploy_remote_acct_iso: (args: { cfg: unknown; destDir: string }) =>
    invoke<string>("deploy_remote_acct_iso", args),
  /** Z05：抓远端 `cc-acct-iso shellinit` 的输出（只读）。返回带 BEGIN/END 围栏的 rc 片段。 */
  remote_acct_iso_shellinit: (args: { cfg: unknown }) =>
    invoke<string>("remote_acct_iso_shellinit", args),

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
  }) => invoke<SubagentLoadResult>("load_subagent", args),

  /**
   * 启动时先拉本地活跃会话建骨架 Tab。返回值字段被真消费 ⇒ 生成物（桶③）。
   * **线上是 snake_case**（`ActiveSessionPayload` 没有 `rename_all`），C04b 已论证过。
   */
  list_active_sessions: () => invoke<ActiveSessionPayload[]>("list_active_sessions"),

  /** `~/.ssh/config` 里的 host 别名清单（不展开 Include、不解析 Match）。 */
  list_ssh_host_aliases: () => invoke<string[]>("list_ssh_host_aliases"),

  /** 本机历史项目列表。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  list_history_projects: () => invoke<HistoryProject[]>("list_history_projects"),

  /** 本机有 `.mcp.json` 的项目目录候选。Rust 签名**无 `Result` 包装**（`-> Vec<String>`）。 */
  list_mcp_project_dirs: () => invoke<string[]>("list_mcp_project_dirs"),

  /**
   * 每个 sid 最近一次用的账号（sid → 账号名）。Rust 返回 `HashMap<String, String>`
   * **无 `Result` 包装** ⇒ `Record<string, string>`，无需生成物。
   */
  list_last_accounts: () => invoke<Record<string, string>>("list_last_accounts"),

  /** 远端历史项目列表（含失败主机名单）。 */
  list_remote_history_projects: () => invoke<RemoteProjectsResult>("list_remote_history_projects"),

  /** 远端有 `.mcp.json` 的项目目录候选。 */
  list_remote_mcp_project_dirs: (args: { origin: string }) =>
    invoke<string[]>("list_remote_mcp_project_dirs", args),

  /** 批量导入 `~/.ssh/config` 的预览分组（F57）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  import_ssh_hosts: () => invoke<ImportGroup[]>("import_ssh_hosts"),

  /** 往远端 `~/.bashrc` 装 ccm wrapper。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  install_remote_ccm_helper: (args: { cfg: unknown; profile: string }) =>
    invoke<string>("install_remote_ccm_helper", args),

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

  /** 在某目录起一个新的本地会话。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  new_local_session: (args: { cwd: string; launcher: string | null }) =>
    invoke<void>("new_local_session", args),

  /** 开独立设置窗口（非浮层）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_settings_window: () => invoke<void>("open_settings_window"),

  /** 从远端 `~/.bashrc` 卸 ccm wrapper。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  uninstall_remote_ccm_helper: (args: { cfg: unknown; profile: string }) =>
    invoke<string>("uninstall_remote_ccm_helper", args),

  /** 卸远端 daemon。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  uninstall_remote_daemon: (args: { cfg: unknown }) =>
    invoke<string>("uninstall_remote_daemon", args),

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
   * 写本机项目 `.mcp.json` 的一个 server。**桶①**。
   * `server` 是不透明 JSON（Rust 侧 `serde_json::Value`）⇒ `unknown`，与生成物一致。
   */
  write_project_mcp_server: (args: { projectDir: string; name: string; server: unknown }) =>
    invoke<void>("write_project_mcp_server", args),

  /** 写远端项目 `.mcp.json` 的一个 server。**桶①**。 */
  write_remote_mcp_server: (args: {
    origin: string;
    projectDir: string;
    name: string;
    server: unknown;
  }) => invoke<void>("write_remote_mcp_server", args),
} as const;
