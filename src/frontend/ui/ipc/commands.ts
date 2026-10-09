/**
 * 类型化 `invoke` 包装层：全仓 TS 只有本文件直呼 `invoke`，Rust 侧每一条命令在本表里有且只有一个条目。
 *
 * 签名手写：`ts-rs` 只生成类型、不生成命令签名（`tauri-specta` 对 Tauri 2 只有预发布版，发版应用不引）。
 * 漂移由 `tests/frontend/ui/ipc/commands.vitest.ts` 兜：
 * 1. Rust 侧 `#[tauri::command]` 声明集 == `invoke_handler` 注册集；
 * 2. 本表的键集 == Rust 命令集（两向，不钉条数）；
 * 3. 每个条目的键名 == 它传给 `invoke` 的字面量（只把字面量抄成另一个真实命令时 `tsc` 看不出来，运行时调错）；
 * 4. 全仓 TS 字面量命令名 ⊆ Rust 命令集，静态看不见的动态名逐字钉死；裸 `invoke` 只在本文件
 *    （另有 `generated-boundary-guard.vitest.ts`「直接 import invoke 的生产文件恰好 1 个」）。
 *
 * 返回类型分三桶：① Rust 返回 `()` / `Result<(), _>` ⇒ `Promise<void>`；
 * ② 有载荷但 TS 不读字段 ⇒ `unknown`（或不透明 JSON）并在那一行注明；③ TS 真消费字段 ⇒ 生成物类型。
 * 条目按落地顺序追加，不按字母序。
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { Said } from "../generated/Said";

/**
 * 壳命令失败（Rust `detail::Said`：一句 ＋ 复制详情那几行）⇒ 一个 `Error`：`message` 与 `String(e)` 都是那一句，
 * `detail` 是复制详情（`kit/detail.ts::detailOf` 取）。全仓壳命令的失败只这一形；别的形状原样抛（不猜）。
 */
export class SaidError extends Error {
  readonly detail: string;
  constructor(said: Said) {
    super(said.said);
    this.name = "SaidError";
    this.detail = said.detail;
  }
  override toString(): string {
    return this.message;
  }
}

function isSaid(v: unknown): v is Said {
  return v !== null && typeof v === "object" && typeof (v as Said).said === "string" && typeof (v as Said).detail === "string";
}

async function callShell<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (e) {
    throw isSaid(e) ? new SaidError(e) : e;
  }
}

/** 本文件里每一条命令都经这一口：失败是 [`Said`] ⇒ 换成 [`SaidError`]。 */
const invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> = callShell;

import type { AutoLaunchConfig } from "../generated/AutoLaunchConfig";
// 合并后的命令一律收 `origin`，类型用生成物 `Origin`（`string`，不手写）：给 origin 参数传 `null` 是编译错。
// 本机逐字送 `LOCAL_ORIGIN`（`"<local>"`，住 `../backend-policy.ts`）；TS 侧其余各处的 origin 也是这一个表示
//（判据：`commands.vitest.ts` 末尾「TS 侧 origin 去 null」那一节）。
import type { Origin } from "../generated/Origin";
import type { FrontOutcome } from "../generated/FrontOutcome";
import type { LocalCcmEntry } from "../generated/LocalCcmEntry";
import type { TerminalChoices } from "../generated/TerminalChoices";
import type { ConfigEdit } from "../generated/ConfigEdit";
import type { MachineFault } from "../generated/MachineFault";
import type { DriftLedgerReport } from "../generated/DriftLedgerReport";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import type { DiagnosticsReport } from "../generated/DiagnosticsReport";
import type { LogFileInfo } from "../generated/LogFileInfo";
import type { RestartHint } from "../generated/RestartHint";

/** 「停」的结局：三个词 ＋ 被停的 pid（没在跑 ⇒ `null`）。与 Rust `remote_resident::StopAnswer` 同形。 */
export interface StopAnswer {
  stopped: "graceful" | "killed" | "not_running";
  pid: number | null;
}

/**
 * 用户级 PATH 那一格的现状。手写（理由见 `ccm_user_path_status` 那一条），
 * 字段名与 Rust 侧 `profile_installer::UserPathStatus` 的线上名由判据对拍。
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

/** 两条开窗命令的结局（壳 `platform/terminal.rs::TerminalOpen`）：开了 · 一个终端都没探到 · 设置里指定的那个不在。 */
export type TerminalOpened = "opened" | "noWindow" | "setMissing";

/**
 * 命令表：键名逐字节等于 Rust 侧的命令名，也等于本条目传给 `invoke` 的字面量。
 * 加条目：键名照抄 Rust 的 fn 名；返回类型按三桶选。
 * 永远是扁平的「命令名 → 函数」：不按模块嵌套、不塞非命令键（动态派发之类走另一个导出），塞了守卫第 2 条当场红。
 */
export const commands = {
  /** 读 `cc_get_auto_launch`。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  cc_get_auto_launch: () => invoke<AutoLaunchConfig>("cc_get_auto_launch"),

  /** 写 `cc_set_auto_launch`。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  cc_set_auto_launch: (args: { enabled: boolean }) =>
    invoke<void>("cc_set_auto_launch", args),

  /**
   * 用户级 PATH 那一格的现状。现算、不缓存：每调一次后端真跑一趟 `powershell.exe`（几百 ms）⇒ 只在打开那一格 / 点刷新时调，不许轮询。
   * 返回类型手写（`UserPathStatus`，桶③里唯一不用生成物的一处）：Rust 侧判据
   * `the_user_path_status_wire_fields_match_the_hand_written_ts` 把线上字段名与这份接口逐字对拍，改了 Rust 不改这里当场红。
   */
  ccm_user_path_status: () => invoke<UserPathStatus>("ccm_user_path_status"),

  /** 把我们那个 bin 目录加到用户级 PATH。桶①。 */
  ccm_user_path_add: () => invoke<void>("ccm_user_path_add"),

  /** 从用户级 PATH 上只摘掉我们那一格。桶①。 */
  ccm_user_path_remove: () => invoke<void>("ccm_user_path_remove"),

  /**
   * 前端性能日志落进 monitor 日志（无 devtools 环境下的唯一取证通道，grep `fe_perf`）。
   * Rust 侧无返回值 ⇒ **桶①** `Promise<void>`。调用方：`e2e-probe.ts` 与 `events.ts`。
   */
  frontend_perf_log: (args: { lines: string }) =>
    invoke<void>("frontend_perf_log", args),

  /** 设置面板「数据」区：枚举 monitor 写到磁盘的所有路径。返回值字段被真消费 ⇒ 用生成物（桶③）。 */
  get_data_paths: () => invoke<DataPathsResponse>("get_data_paths"),

  /**
   * 写配置：只交「改哪几条路径」，不整份替换（并发写不互相覆盖）。Rust 返回 `Result<(), String>` ⇒ **桶①**。
   * 补丁的形状由生成物 `ConfigEdit` 钉（Rust `config.rs::ConfigEdit`）；值本身仍是不透明 JSON（见 `load_config`）。
   */
  patch_config: (args: { edits: ConfigEdit[] }) =>
    invoke<void>("patch_config", args),
  machine_table_try: (args: { edits: ConfigEdit[] }) =>
    invoke<MachineFault | null>("machine_table_try", args),

  /**
   * 写诊断配置。返回 `RestartHint` —— **它是个只有 unit variant 的外部标记枚举**
   * （`#[serde(rename_all = "snake_case")]`，没有 `tag`）⇒ 线上就是字符串
   * `"none"` / `"needs_restart"`，生成物给的正是那个字面量联合。
   */
  set_diagnostics_config: (args: { cfg: DiagnosticsConfig }) =>
    invoke<RestartHint>("set_diagnostics_config", args),

  /** 这台已跟 monitor 完成拉前握手的终端数（住 monitor 进程里的 `BindRegistry`）。 */
  bound_terminal_count: () => invoke<number>("bound_terminal_count"),

  /** 在本机开一个终端窗口跑 `cmd`（工作目录 `cwd`）：POSIX 上按设置 / 探到的终端 / Windows 上 PowerShell。
   *  结局同 `open_terminal_window`（`"noWindow"` ＝ 这台找不到终端）。只经 `src/frontend/ui/terminal-open.ts` 调。桶①。 */
  open_local_terminal: (args: { cmd: string; cwd: string | null }) =>
    invoke<TerminalOpened>("open_local_terminal", args),

  /** 本机那条 `ccm` 入口现在是什么样：我们放下去的那一份在哪、它自报什么身份、
   *  你 PATH 上那个 `ccm` 是不是它，以及给人读的那句话。`LocalCcmEntry` 是生成物 ⇒ **桶③**。
   *  只读：跑两次 `--ccm-probe`，一个字节都不写；`~/.local/bin/ccm` 下的旧份由用户自己删，产品不删。 */
  local_ccm_entry_status: (fresh?: boolean) =>
    invoke<LocalCcmEntry>("local_ccm_entry_status", { fresh: fresh ?? null }),

  /** 开一个终端窗口跑 `command`（**成品**：远端那一行由本机后端 `terminal-ssh` 渲好、本机那一串由本机后端起会话那一问交回 —— monitor 只开窗）。
   *  Rust 返回 `Result<TerminalOpen, String>`（`"opened"` / `"noWindow"`）⇒ **桶①**。
   *  只经 `src/frontend/ui/terminal-open.ts::openTerminal` 调（开终端只有一个家）。 */
  open_terminal_window: (args: { command: string }) =>
    invoke<TerminalOpened>("open_terminal_window", args),

  /** 发一条系统通知（「一轮完成」「需要你」）。界面判要不要发，壳只发（`platform/notify.rs`）。桶①。 */
  notify_desktop: (args: { title: string; body: string }) => invoke<void>("notify_desktop", args),

  /** 写系统剪贴板（`clipboard.rs`，回真成败）。桶①。只经 `src/frontend/ui/clipboard.ts` 调（复制只有一个家）。 */
  clipboard_write: (args: { text: string }) => invoke<void>("clipboard_write", args),

  /** 设置页「终端」那一行要的事实：自动会挑谁 · 本机探到哪些 · 现在设的是什么（挑终端的判定在壳的平台层）。
   *  `TerminalChoices` 是生成物 ⇒ **桶③**。 */
  terminal_choices: () => invoke<TerminalChoices>("terminal_choices"),

  /** 开终端那一问要的机器事实 `{machine, saved, jump, prefer}`（monitor 的机器表 ＋ 上次赢的那条）。
   *  **桶②**：TS 不读它的字段，原样转交本机后端 `terminal-ssh`（组请求与渲染都在那里）。 */
  terminal_dial: (args: { origin: string }) =>
    invoke<unknown>("terminal_dial", args),

  /** 诊断配置（log 开关 / 级别 / error toast / 保留天数）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  get_diagnostics_config: () =>
    invoke<DiagnosticsConfig>("get_diagnostics_config"),

  /** log 目录与文件清单。`current_size_bytes`/`size_bytes` 是**字节数**、`modified_ms` 是**毫秒时间戳**——两个量纲的上限论证在 Rust 侧分开写。 */
  get_log_file_info: () => invoke<LogFileInfo>("get_log_file_info"),

  /**
   * 日志页「复制诊断信息」：整段诊断文本 ＋ 那一行「未识别数据」用的数（同一份）。`configUnknown` 是界面那份键表认不出的顶层键；
   * `drift` 是界面经通道问回来的各台记录账（`drift-report` 的应答，问不到 ⇒ `null`）。
   */
  diagnostics_report: (args: {
    configUnknown: string[];
    drift: { origin: string; report: unknown }[];
  }) => invoke<DiagnosticsReport>("diagnostics_report", args),

  /** 设置窗「现在重启」：重起 cc-monitor 自己（会打断什么由调用方先问后端）。 */
  restart_app: () => invoke<void>("restart_app"),

  /** 部署远端后端（连同 `ccm` 入口，一次）。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。 */
  deploy_remote_backend: (args: { cfg: unknown }) =>
    invoke<string>("deploy_remote_backend", args),

  /**
   * 「足迹」里 monitor 自己那台那几行（`HostScope::Client`）要的、只有 monitor 知道的事实（`footprint_client.rs`）：
   * 它自己进程的 `{home, path}`。成品由那台后端的帧命令 `footprint-report` 出（这一份原样带过去），
   * 本侧不认识它的形状 ⇒ **桶②**。
   */
  footprint_client_facts: () =>
    invoke<Record<string, unknown>>("footprint_client_facts"),

  // 数据面漂移记账（只读、按需一次，不轮询）。
  // 按机器分：问哪台答哪台，回包带回 `origin`（界面按回声判）。monitor 自己的命令，不经后端。
  // 只答 monitor 天生观测的两面；记录那两面问那台后端（`record-reads.ts::readRecordDrift`）。
  drift_ledger_report: (args: { origin: Origin }) =>
    invoke<DriftLedgerReport>("drift_ledger_report", args),
  /** 装出来的 `cc-spawn` 在本机跑不跑得起来：本机 `ccm` 够不够新（monitor 探本机 ccm；`null` = 够新）。扩展页把 cc-bus 装到本机之后问一次。 */
  cc_bus_ccm_precheck: () => invoke<string | null>("cc_bus_ccm_precheck"),
  /** 装了 MCP 的远端 host 列表。原始类型数组，无需生成物。 */
  list_remote_mcp_origins: () => invoke<string[]>("list_remote_mcp_origins"),

  /**
   * 读配置。**Rust 侧返回 `Result<serde_json::Value, String>`——它把配置当不透明 JSON 透传**，
   * 所以这个边界**结构性无法**由生成物加固：Rust 自己就不知道形状。
   * `Record<string, unknown>` 已经是最诚实的类型（TS 侧的 `Config` 就是它的别名）。
   */
  load_config: () => invoke<Record<string, unknown>>("load_config"),

  /**
   * 这台机的后端现在什么状态。Rust 那边是不透明 JSON（同 `load_config` 那处的
   * 结构性缺口）⇒ **桶②**：不为没人消费的字段造生成物。
   */
  backend_status: (args: { origin: string }) =>
    invoke<Record<string, unknown>>("backend_status", args),

  /**
   * **开关面该列哪几台机** —— 由后端的注册表说了算，前端不自己算。
   * 前端自己拼会与 Rust 的 origin 分叉四处（trim / 重复 label 后缀化 / 忽略 enabled /
   * 启动后新增的没注册），见 `backend_control::backend_machines` 头注。⇒ **桶②**。
   */
  backend_machines: () => invoke<string[]>("backend_machines"),

  /** 起这台机的后端。返回一句人话（已起 / 已经在跑 / 起不来的理由）⇒ **桶②**。 */
  backend_start: (args: { origin: string }) =>
    invoke<string>("backend_start", args),

  /**
   * 停这台机的后端 ⇒ **桶②**。本机远端同形：那台机器上的一次性 `--resident-stop` 做「请它收尾 → 宽限期内等 → 到点强杀」，
   * 这里拿回结局（`remote_resident.rs::StopAnswer`）。
   */
  backend_stop: (args: { origin: string }) =>
    invoke<StopAnswer>("backend_stop", args),

  /**
   * 在原生文件窗口（egui，同进程、次线程）里打开远端 `path` 这个目录。
   *
   * 回的是**这一趟列到的行数** ⇒ 桶③那一支里的「原始类型」（`number`，不用生成物）。
   *
   * 🔴 **它为什么不是「发个请求就回」**：Rust 侧**先真的把那个目录列出来**，
   * 列不出来就带着 `sftp_pool` 那边的原文 reject ⇒ 这一条 `await` 真的能失败，
   * 调用方该接住它并出声（`src/frontend/ui/file-window.ts::openFileWindow`，全仓唯一调用点）。
   * 没有这一层的话「点了按钮什么都没发生」与「开成功了」在界面上分不开
   * —— 本机没有图形会话时那正是必然发生的事。
   *
   * ⚠ 它**不**保证「窗口出现在屏幕上」：那要一个图形会话，命令这一侧看不到。
   */
  open_file_window: (args: {
    cfg: unknown;
    path: string;
    /**
     * 优先级：`path`（非空）> `revealFile` > 远端 home。
     *
     * - `path` 非空 ⇒ 直接进那个目录。
     * - `path` 空 ＋ `revealFile` = 一条远端文件绝对路径 ⇒ 进它父目录，高亮那一行、滚进视野。
     *   父目录与尾段由 Rust 侧算（`filewin::source::parent_dir` / `remote_basename`），前端不自己切远端路径。
     * - 两个都空 ⇒ 问远端 `realpath('.')`。
     */
    revealFile?: string | null;
    /** 文件窗口的样子：设计令牌名 → 此刻的计算值（`file-window.ts::fileWindowTheme`）。 */
    theme: Record<string, string>;
  }) => invoke<number>("open_file_window", args),

  /** 开设置窗；`target` 是目的地 JSON（`settings/open-settings.ts`）。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_settings_window: (target: string | null = null) =>
    invoke<void>("open_settings_window", { target }),

  /** 机器表热加载：照 config.json 的机器表起 / 断 / 重起远端那几条流（不要重启 cc-monitor）。 */
  remote_reconcile: () =>
    invoke<{ started: string[]; stopped: string[]; restarted: string[] }>(
      "remote_reconcile",
    ),

  /** 卸远端后端。Rust 返回 `Result<String, String>` ⇒ 原始类型。 */
  uninstall_remote_backend: (args: { cfg: unknown }) =>
    invoke<string>("uninstall_remote_backend", args),

  /** 用系统默认程序打开 monitor 的 log **目录**。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_dir: () => invoke<void>("open_log_dir"),

  /** 用系统默认程序打开 monitor 的 log 文件。Rust 返回 `Result<(), String>` ⇒ **桶①**。 */
  open_log_file: () => invoke<void>("open_log_file"),

  /**
   * 把某会话在一个独立只读窗口（`viewer-<sid>`）里打开。`x`/`y` = 拖拽撕离的落点。**桶①**。
   * `origin`：窗口自己订那个会话的流（`session-lines/<sid>`），要知道它在哪台机器上。
   * `run`：开的是这个会话里一个子运行自己的窗口（agent 窗口；一个子运行至多一个，已开着就前置；没给落点就错开叠放）。
   */
  open_session_in_new_window: (args: {
    sessionId: string;
    origin: Origin;
    title: string;
    x?: number;
    y?: number;
    run?: string;
  }) => invoke<void>("open_session_in_new_window", args),

  /** 本机会话拉前（Windows 按 sid→HWND 缓存）；回结局族。**桶①**。 */
  bring_terminal_to_front: (args: { sessionId: string }) =>
    invoke<FrontOutcome>("bring_terminal_to_front", args),

  /** 远端会话拉前，两问：交那台回的 `terminals` ⇒ 按窗口标签找（对不上回 `null`）；交本机后端的进程链 ⇒ 沿链找窗口。回结局族。**桶①**。 */
  bring_remote_terminal_to_front: (
    args: { terminals: unknown[] } | { chain: unknown[] },
  ) => invoke<FrontOutcome | null>("bring_remote_terminal_to_front", args),

  /** 关 tab 时让事件重放忘掉这个会话。**桶①**。 */
  forget_session: (args: { sessionId: string }) =>
    invoke<void>("forget_session", args),

  /** 把监控窗口拉到最前。**桶①**。 */
  bring_monitor_to_front: () => invoke<void>("bring_monitor_to_front"),

  /**
   * **通道在 Tauri IPC 这一跳上的那条命令**（`chan/webview.rs`）。
   * ⚠ 调用方**不直接用它**：一律经 `src/comms/inward/chan.ts` 的 `chan.call(origin, op, payload, budget)`
   * （期限换算、本地撤单、三层错误解码都住那里）。载荷去程是字节数组、回程是原样字节（`ArrayBuffer`）。
   * **桶②**：回的是不透明字节，本表不认识它的形状。
   */
  chan_call: (args: {
    origin: Origin;
    op: string;
    payload: number[];
    leftMs: number;
    callId: string | null;
  }) => invoke<ArrayBuffer>("chan_call", args),

  /**
   * 撤掉 webview 这一跳上带编号的那一问（`chan/webview.rs::chan_cancel`）。
   * 调用方同样**不直接用它**：`chan.call` 在 `Budget.cancel` 拨下时自己发。回那一问此刻在不在飞。**桶①**。
   */
  chan_cancel: (args: { id: string }) => invoke<boolean>("chan_cancel", args),

  /**
   * 那台机器的能力事实（`chan/webview.rs::chan_offer`）：认哪些 op · 这台做不到哪几条（附码）·
   * 撤掉之后停得下的那几条。判断在 monitor 那边做完，本侧只查成员。`null` = 今天没有控制通道。
   * ⚠ 调用方经 `src/comms/inward/chan.ts` 的 `chan.offer` / `chan.cachedOffer` 用它。
   */
  chan_offer: (args: { origin: Origin }) =>
    invoke<{
      ops: string[];
      unavailable: [string, string][];
      stoppable: string[];
    } | null>("chan_offer", args),

  /**
   * **通道 `subscribe` 在 Tauri IPC 这一跳上的三条命令**（`chan/webview.rs`）。
   * ⚠ 调用方**不直接用它们**：一律经 `src/comms/inward/chan.ts` 的 `chan.subscribe(origin, kind, from, want, sink)`
   * （编号、窗口作用域的交格事件、解码都住那里）。`id` 由那一侧给（每页从 1 起）。**不回错**：说不了的在流里原位说。
   */
  chan_subscribe: (args: {
    origin: Origin;
    kind: string;
    from: number[] | null;
    want: number;
    id: number;
  }) => invoke<void>("chan_subscribe", args),
  chan_want: (args: { id: number; more: number }) =>
    invoke<void>("chan_want", args),
  chan_stop: (args: { id: number }) => invoke<void>("chan_stop", args),
} as const;
