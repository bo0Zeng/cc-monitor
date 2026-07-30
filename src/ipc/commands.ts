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
 * **53 个命令**（C04a 样板 1 + C04d 批 1-5b 的 52）。
 * 其余 66 个仍走各模块里的裸 `invoke`（119 − 53 = 66），由 **C04d** 后续批次迁进来。
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
import type { CcBusMessage } from "../generated/CcBusMessage";
import type { CcBusState } from "../generated/CcBusState";
import type { CcPreviewResponse } from "../generated/CcPreviewResponse";
import type { CcStatusResponse } from "../generated/CcStatusResponse";
import type { CcmProbeResult } from "../generated/CcmProbeResult";
import type { ConfigSurfaceReport } from "../generated/ConfigSurfaceReport";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import type { ForwardStatus } from "../generated/ForwardStatus";
import type { HooksReport } from "../generated/HooksReport";
import type { ProfileScan } from "../generated/ProfileScan";
import type { LogFileInfo } from "../generated/LogFileInfo";
import type { McpServerEntry } from "../generated/McpServerEntry";
import type { RestartHint } from "../generated/RestartHint";
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

  /** 起一条端口转发。Rust 返回 `Result<String, String>`（转发 id）⇒ 原始类型。 */
  start_forward: (args: {
    spec: { origin: string; localPort: number; remoteHost: string; remotePort: number };
  }) => invoke<string>("start_forward", args),

  /** 停一条端口转发。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  stop_forward: (args: { id: string }) => invoke<void>("stop_forward", args),

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

  /** 一次配置面审计（只读、一次性，不新增轮询）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  config_surface_report: () => invoke<ConfigSurfaceReport>("config_surface_report"),

  /** 把内嵌的 vendor `cc-acct-iso` 部署到远端。返回人话结果串 ⇒ 原始类型，无需生成物。 */
  deploy_remote_acct_iso: (args: { cfg: unknown; destDir: string }) =>
    invoke<string>("deploy_remote_acct_iso", args),

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

  /** 本机有 `.mcp.json` 的项目目录候选。Rust 签名**无 `Result` 包装**（`-> Vec<String>`）。 */
  list_mcp_project_dirs: () => invoke<string[]>("list_mcp_project_dirs"),

  /**
   * 每个 sid 最近一次用的账号（sid → 账号名）。Rust 返回 `HashMap<String, String>`
   * **无 `Result` 包装** ⇒ `Record<string, string>`，无需生成物。
   */
  list_last_accounts: () => invoke<Record<string, string>>("list_last_accounts"),

  /** 远端有 `.mcp.json` 的项目目录候选。 */
  list_remote_mcp_project_dirs: (args: { origin: string }) =>
    invoke<string[]>("list_remote_mcp_project_dirs", args),

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

  /** 开独立设置窗口（非浮层）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_settings_window: () => invoke<void>("open_settings_window"),

  /** 用系统默认程序打开 monitor 的 log **目录**。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_dir: () => invoke<void>("open_log_dir"),

  /** 用系统默认程序打开 monitor 的 log 文件。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_file: () => invoke<void>("open_log_file"),
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
