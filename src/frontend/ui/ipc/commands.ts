/**
 * C04a（rust-ts-boundary）：**类型化 `invoke` 包装层**。
 *
 * ## 为什么是手写的
 *
 * `ts-rs` 只生成**类型**，不生成命令签名（主计划 §8 的选型订正说明了这一点：
 * `tauri-specta` 会生成签名，但它对 Tauri 2 只有 `2.0.0-rc.1`，而本仓是要 Windows 打包发版的
 * 生产应用 ⇒ 不引入预发布依赖）。所以签名这一层手写，**漂移由守卫兜**
 * （`tests/frontend/ui/ipc/commands.vitest.ts`）。
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
import { invoke } from "@tauri-apps/api/core";

// 〔MIG-3a〕`SkillView` 搬进 `src/frontend/ui/skill-inbox-reads.ts`（收件箱三问改走通道、后端出成品）。
// 〔US1 · 第四波 4D〕`ApikeyCredentialsStatus` 与 `ApikeyRoutingView` 两个类型搬进 `src/frontend/ui/apikey-reads.ts`（那两问改走通道、后端出成品，
//   形状由跨语言金样 `tests/__fixtures__/apikey.golden.json` 两侧对拍）；本文件那条 `import type … from "../accounts"`（B-decouple §6
//   必须拆 4 点名的「闭合类型环的那条边」）随 `apikey_routing_for` 一起走了。


import type { AutoLaunchConfig } from "../generated/AutoLaunchConfig";
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
//   判据 `tests/frontend/ui/ipc/commands.vitest.ts` 末尾「TS 侧 origin 去 null」那一节，全 TS ＋ 生成物）；
//   〔C4b〕生成物 `RemoteHealthPayload.origin` 那最后一处也改成了 `string`（那一节的 `PENDING` 从此为空）。
import type { Origin } from "../generated/Origin";
// `K-R69`：本机那条 `ccm` 入口这一格（我们那一份 · PATH 上那一份 · 判词 · 那句话）。
import type { LocalCcmEntry } from "../generated/LocalCcmEntry";
import type { ConfigEdit } from "../generated/ConfigEdit";
import type { DriftLedgerReport } from "../generated/DriftLedgerReport";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
// 〔RM1f〕panorama 一族〔MIG-3b 续〕今天走通道（`panorama` / `panorama-edit`），返回 `unknown`，
// 由 `src/frontend/ui/panorama/api.ts` 按 op 收窄成 `src/frontend/ui/panorama/types.ts` 的手写类型（那 10 个住 vendored
// `code-picture-core/src/model.rs`，`VENDOR.md` 铁律「副本是上游的镜子」⇒ 不在副本里加 `ts_rs` 派生）。
// 〔改前这里 import 那十几个类型给进程内那十七条包装用，`PanoramaStatus` 用生成物；三样都随内嵌引擎退役了。〕
import type { DiagnosticsConfig } from "../generated/DiagnosticsConfig";
import type { LogFileInfo } from "../generated/LogFileInfo";
import type { RestartHint } from "../generated/RestartHint";

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
   * `src/frontend/ui/generated/`，那份目录**不在 `K-R135` 的写区里**。⇒ 手写一份，并且**不是裸奔的**：
   * Rust 侧有一条判据（`profile_installer` 的
   * `the_user_path_status_wire_fields_match_the_hand_written_ts`）把**线上字段名**
   * （serde 序列化一个样本实例现算出来的）与本文件这份接口逐字对拍 ⇒ 改了 Rust 不改这里当场红。
   * 哪天写区放开 `src/frontend/ui/generated/`，换成生成物、把那条判据删掉。
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

  // 〔MIG-2〕`probe_ccm_cli` 退役：渲染进了那台后端，能力问它自己，界面不再先探一遍。

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

  // 〔HX2 · 第四波 4D〕墓碑：这里从前是 `write_apikey_credentials_key`〔散文墓碑〕（`K-H2a` `KS10`：从界面配一把 key）。
  //   写 key 改走通道 `apikey-key-set`（`src/frontend/ui/apikey-reads.ts::writeApikeyKey`），交那台机器的后端；`KH2C1` 那一条照旧成立 ——
  //   前端交 `configDir`，账号 id 由后端按全仓唯一那份规则（`acct_core::apikey_account_id_of_dir`）推。

  // 〔MIG-3a · 主会话 09-27 裁〕别名六条（`aliases_*`〔散文墓碑〕）退役：规则 · 方言 · 围栏进了那台后端，界面经通道直问（`src/frontend/ui/alias-reads.ts`）。
  /** 〔MIG-3a〕这台已跟 monitor 完成拉前握手的终端数（住 monitor 进程里的 `BindRegistry`，从前夹在别名读回口里）。 */
  bound_terminal_count: () => invoke<number>("bound_terminal_count"),

  // 〔MIG-3b 续 · 主会话 09-28 裁〕全景三条（问 · 写 · 撤）退役：界面经通道直问那台后端 `panorama` / `panorama-edit`，撤单过通道那一跳。
  /**
   * 那台缺小程序 / 装的太旧（后端回 `not_installed` / `unsupported`）⇒ monitor **放字节**：远端推到 `~/.cc-monitor/bin/`、本机放到同一处
   * （`panorama_bytes.rs::panorama_place`；每台一把锁，排队的那一问等到前一个放完就不再放）。成功 **桶①**。
   */
  panorama_place: (args: { origin: Origin }) => invoke<null>("panorama_place", args),

  // 〔RM1f · V108 后半句〕本机那十七条进程内全景命令的包装随内嵌引擎退役删了（panorama_callees · panorama_callers ·
  //   panorama_diagram · panorama_diagram_kinds · panorama_docs_for · panorama_drift · panorama_impact · panorama_index ·
  //   panorama_list_annotations · panorama_node · panorama_overview · panorama_reindex · panorama_search · panorama_status ·
  //   panorama_subgraph · panorama_symbols_in_file · panorama_touching）：本机远端同一条（〔MIG-3b 续〕今天是通道上的 `panorama`）。

  // 〔第四波 S4〕`sftp_copy`（远端内部复制，步 23b）的包装随那条命令退役删了：窗口的复制走后端 `files-copy`。

  // 〔F7c 收尾 09-24〕池子那十二条的包装一起走了（老面板删了、窗口改走通道；`设计/60 §13b`）：
  //   sftp_cancel_transfer · sftp_chmod · sftp_delete · sftp_download · sftp_list_dir · sftp_mkdir ·
  //   sftp_read_text_for_edit · sftp_realpath · sftp_rename · sftp_stat · sftp_upload · sftp_write_text。

  // 〔MIG-1 · `设计/99 §2.1 ⑬`〕端口转发三条（起 · 停 · 列）的包装随命令退役删了：转发账住本机常驻后端，
  //   界面经通道直接说帧命令 `forward-start` / `forward-stop` / `forward-list`（`src/frontend/ui/port-forward-reads.ts`）。

  // 〔MIG-1 续 · `设计/99 §2.1 ⑬`〕测试连接的包装（带 `Channel<ConnectStage>` 阶段泳道那一条）随命令退役删了：
  //   界面把那台配置交给本机后端（帧命令 `remote-probe`，`src/frontend/ui/remote-probe.ts`），阶段行随结局一并回来。

  // 〔MOD · `05 §14.3` C 组〕会话正文四条（`stream_read_session_jsonl` · `load_subagent` · `read_session_range` ·
  //   `read_session_lines`〔散文墓碑〕）的包装随命令退役删了：那台后端出记录行 / 成品，界面经通道直问（`src/frontend/ui/record-reads.ts`）。

  // 〔SH1 · V136〕`read_cc_bus_inbox` / `read_cc_bus_state` 两条退役：驾驶舱读面经通道直接问后端（`src/frontend/ui/cc-bus-control.ts`）。

  // 〔MIG-3b 续 · ⑬〕把本机公钥推到远端那一条退役：界面经通道问本机后端 `pubkey-push`（`src/frontend/ui/pubkey-push.ts`），读 `.pub` · 组请求 · 写都在后端。

  /** 读本机 MCP server 清单（user/local/project 三档）。Rust 签名**无 `Result` 包装**。 */
  // 〔US1 · 第四波 4D〕`read_apikey_credentials_status` / `apikey_routing_for` 退役：界面经通道直接问那台后端
  //   `apikey-read` / `apikey-routing`（`src/frontend/ui/apikey-reads.ts`）。

  // 〔MIG-2 · `99 §2.1 ⑬`〕「这次拉起写哪个中转地址」那一问退役成那台后端的成品（界面经通道直接问 `launch-endpoint`，
  //   `src/frontend/ui/launch-render.ts::launchEndpoint`）。monitor 这边只剩那个开关自己的值（下一条）。

  /** 〔MIG-2〕全量注入开关：monitor 进程环境 `CCM_RELAY_ALL_SESSIONS`（默认开，`=0` 才关）。monitor 自己的配置，界面带给那台后端。 */
  relay_all_sessions_switch: () => invoke<boolean>("relay_all_sessions_switch"),

  // 〔CF2 · 第四波 4B〕独立窗口的定向重放（`replay_session_to_window`〔散文墓碑〕）退役：独立窗口自己订
  //   `session-lines/<sid>`（`chan.subscribe`），留存由那条订阅当场交。

  // 〔MIG-2 · `99 §2.1 ⑬`〕本机起会话三条（resume · 新起 · 接回那一句）退役：计划与渲染问本机后端 `launch-local`
  //   （`src/frontend/ui/launch-render.ts::planLocalLaunch`），monitor 只剩开终端窗口（下一条）。

  /** 〔MIG-2〕在本机开一个终端窗口跑 `cmd`（工作目录 `cwd`）：POSIX 上交用户自己的终端 / Windows 上 PowerShell。桶①。 */
  open_local_terminal: (args: { cmd: string; cwd: string | null }) => invoke<void>("open_local_terminal", args),

  // 〔C4c · 第四波 4B〕「resume 之前问记录还在不在」那一条退役：界面经通道直接问后端 `history-record`
  //   （`src/frontend/ui/session-reads.ts::probeSessionRecord`，成品 `{present, root}`）。

  // 〔MIG-1 续 · `99 §2.1 ⑬`〕列 tmux 会话那两条（本机 · 远端）的包装随命令退役删了：界面经通道直接问那台后端的 `tmux-list`
  //   （成品，`src/frontend/ui/tmux-reads.ts::listTmux`，本机远端同一形）。

  /** `K-R69`：**本机那条 `ccm` 入口现在是什么样** —— 我们放下去的那一份在哪、它自报什么身份、
   *  你 PATH 上那个 `ccm` 是不是它，以及给人读的那句话。`LocalCcmEntry` 是生成物 ⇒ **桶③**。
   *
   *  ⚠ 它**只读**：跑两次 `--ccm-probe`，一个字节都不写；产品也**不删**用户 `~/.local/bin/ccm`
   *  下那份旧的（用户逐字「原本的配置要手动删除」）。 */
  local_ccm_entry_status: (fresh?: boolean) => invoke<LocalCcmEntry>("local_ccm_entry_status", { fresh: fresh ?? null }),

  // 〔LOC1a · 第四波 4D · C4e 批 4〕某会话的任务快照那一条退役：界面经通道直接问那台机器的后端 `tasks-list`
  //   （后端出成品，`tasks-panel.ts::fetchSessionTasks` / `decodeTasks`）。

  /** 〔FIX4 · `设计/99 §2.1 ⑬`〕开一个终端窗口跑 `command`（**成品**：远端那一行由本机后端 `terminal-ssh` 渲好、本机那一串由 `terminal-local` 交回；
   *  〔P5〕这次拉起带令牌时，两条都已接好令牌握手前奏 —— monitor 只开窗）。
   *  Rust 返回 `Result<(), String>` ⇒ **桶①**。`ssh`：这一行要跑本机的 ssh（Windows 上先查 ssh.exe 在不在）。
   *  只经 `src/frontend/ui/terminal-open.ts::openTerminal` 调（开终端只有一个家）。 */
  open_terminal_window: (args: { command: string; ssh: boolean }) =>
    invoke<void>("open_terminal_window", args),

  /** 〔FIX4 · `设计/99 §2.1 ⑬`〕开终端那一问要的机器事实 `{machine, saved, jump, prefer}`（monitor 的机器表 ＋ 上次赢的那条）。
   *  **桶②**：TS 不读它的字段，原样转交本机后端 `terminal-ssh`（组请求与渲染都在那里）。 */
  terminal_dial: (args: { origin: string }) => invoke<unknown>("terminal_dial", args),

  /** 诊断配置（log 开关 / 级别 / error toast / 保留天数）。返回值字段被真消费 ⇒ 生成物（桶③）。 */
  get_diagnostics_config: () => invoke<DiagnosticsConfig>("get_diagnostics_config"),

  /** log 目录与文件清单。`current_size_bytes`/`size_bytes` 是**字节数**、`modified_ms` 是**毫秒时间戳**——两个量纲的上限论证在 Rust 侧分开写（C03 纪律）。 */
  get_log_file_info: () => invoke<LogFileInfo>("get_log_file_info"),

  /** 部署内嵌的后端到远端。Rust 返回 `Result<String, String>`（人话结果）⇒ 原始类型。 */
  /** 部署远端后端（〔MC1〕连同 `ccm` 入口，一次）。 */
  deploy_remote_backend: (args: { cfg: unknown }) => invoke<string>("deploy_remote_backend", args),

  // 〔MIG-3b〕分叉那条退役：分叉经通道直说那台后端 `session-fork`（`src/frontend/ui/session-writes.ts::forkSession`）。

  // 〔C4a · 子步 3〕E79 那条本机版「某会话跑在哪个账号下」退役：
  //   本机与远端同一条路 —— `account-reads.ts::fetchSessionAccounts` 经通道 `chan.call(origin, "accounts-sessions", …)`。

  // 〔MIG-3b〕删会话那条退役：删会话经通道直说那台后端 `files-delete-session`（`src/frontend/ui/session-writes.ts::deleteSession`）。

  /**
   * 〔MIG-3b 续 · 主会话 09-28 裁①〕「足迹」里 monitor 自己那台那几行（`HostScope::Client`）要的、只有 monitor 知道的事实（`footprint_client.rs`）：
   * 它自己进程的 `{home, agentHome, path}`。成品由本机后端出（`src/frontend/ui/settings/footprint-reads.ts` 经通道问 `footprint-report`，把这一份原样带过去），
   * 本侧不认识它的形状 ⇒ **桶②**。〔`config_surface_report`〔散文墓碑〕随判定进后端删了。〕
   */
  footprint_client_facts: () => invoke<Record<string, unknown>>("footprint_client_facts"),

  // U-CC1：数据面漂移记账（只读、按需一次，不轮询）。
  // 〔ST3〕按机器分：问哪台答哪台，回包带回 `origin`（界面按回声判）。monitor 自己的命令，不经后端。
  // 〔MOD〕只答 monitor 天生观测的两面；记录那两面问那台后端（`record-reads.ts::readRecordDrift`）。
  drift_ledger_report: (args: { origin: Origin }) =>
    invoke<DriftLedgerReport>("drift_ledger_report", args),
  // 〔C4b · 第四波 4B〕P8a 的 marketplace 面（`list_plugin_marketplaces`〔散文墓碑〕）退役：经通道直接说帧命令
  //   `plugins-marketplaces`，后端出成品（`src/frontend/ui/settings/plugins-section.ts::fetchSurvey`）。
  // 〔MIG-3a · 子步 3〕PS1 / PS2 那两条（`deploy_local_cc_bus`〔散文墓碑〕· `cc_bus_install_state`〔散文墓碑〕）退役：
  //   cc-bus 是后端代管的资产：装在扩展页，经本机后端的枢纽交被写那台装（`ext-hub-preview` / `-apply`）。
  /** 装出来的 `cc-spawn` 在本机跑不跑得起来：本机 `ccm` 够不够新（monitor 探本机 ccm；`null` = 够新）。扩展页把 cc-bus 装到本机之后问一次。 */
  cc_bus_ccm_precheck: () => invoke<string | null>("cc_bus_ccm_precheck"),
  // 〔MIG-2〕`ccm …` 调用行 · 载荷渲染 · 本机接回那一句三条退役：那台后端的帧命令（`src/frontend/ui/launch-render.ts`）。

  // 〔MIG-3a · 主会话 09-28 预裁〕`deploy_remote_acct_iso`〔散文墓碑〕 退役：账号库今天由那台后端自己管（`src/frontend/ui/account-ops.ts` 经通道问它）。

  // 〔MIG-3b〕cc-bus 钩子诊断两条（本机 / 远端）退役：界面经通道直问那台后端 `hooks-diag`（`settings/cc-bus-hooks-section.ts::fetchHooksReport`）。


  // 〔C4b · 第四波 4B〕骨架索引那一条（`read_session_index`〔散文墓碑〕）退役：经通道直接说帧命令 `history-index`，
  //   后端出成品（`src/frontend/ui/session-reads.ts::readSessionIndex`）。


  // 〔CF2 · 第四波 4B〕「接上骨架 ⇒ 重放缓冲只留尾巴」那一条（`replay_keep_tail_only`〔散文墓碑〕）退役：
  //   重放缓冲对每个会话都只留尾巴，前端不再登记。

  // 〔C4b · 第四波 4B〕大纲清单与会话内查找那两条（`list_user_inputs` / `find_in_session`〔散文墓碑〕）退役：
  //   经通道直接说帧命令 `history-user-inputs` / `history-find`，后端出成品（`src/frontend/ui/session-reads.ts`）。



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
   * `24e` 第二刀（`设计/60 §4 戊` / `§5` 第三段）：在**原生窗口**（egui，同进程、次线程）
   * 里打开远端 `path` 这个目录。
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
  //   `accounts-list` / `accounts-trust`（`src/frontend/ui/account-reads.ts::fetchAccounts` / `checkTrust`），后端出成品。


  /**
   * 〔C4a · 子步 3〕**通道在 Tauri IPC 这一跳上的那条命令**（`chan/webview.rs`）。
   * ⚠ 调用方**不直接用它**：一律经 `src/comms/inward/chan.ts` 的 `chan.call(origin, op, payload, budget)`
   * （期限换算、本地撤单、三层错误解码都住那里）。载荷去程是字节数组、回程是原样字节（`ArrayBuffer`）。
   * **桶②**：回的是不透明字节，本表不认识它的形状。
   */
  chan_call: (args: { origin: Origin; op: string; payload: number[]; leftMs: number; callId: string | null }) =>
    invoke<ArrayBuffer>("chan_call", args),

  /**
   * 〔MIG-3b 续 · 主会话 09-28 裁「撤单不许回退」〕撤掉 webview 这一跳上带编号的那一问（`chan/webview.rs::chan_cancel`）。
   * 调用方同样**不直接用它**：`chan.call` 在 `Budget.cancel` 拨下时自己发。回那一问此刻在不在飞。**桶①**。
   */
  chan_cancel: (args: { id: string }) => invoke<boolean>("chan_cancel", args),

  /**
   * 〔NET2〕那台机器的能力事实（`chan/webview.rs::chan_offer`）：认哪些 op · 这台做不到哪几条（附码）·
   * 撤掉之后停得下的那几条。判断在 monitor 那边做完，本侧只查成员。`null` = 今天没有控制通道。
   * ⚠ 调用方经 `src/comms/inward/chan.ts` 的 `chan.offer` / `chan.cachedOffer` 用它。
   */
  chan_offer: (args: { origin: Origin }) =>
    invoke<{ ops: string[]; unavailable: [string, string][]; stoppable: string[] } | null>("chan_offer", args),

  /**
   * 〔CF2 · 第四波 4B〕**通道 `subscribe` 在 Tauri IPC 这一跳上的三条命令**（`chan/webview.rs`）。
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
  chan_want: (args: { id: number; more: number }) => invoke<void>("chan_want", args),
  chan_stop: (args: { id: number }) => invoke<void>("chan_stop", args),
} as const;
