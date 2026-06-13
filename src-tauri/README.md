# 后端模块导览（`src-tauri/`）

Rust + Tauri 2。crate 名 `monitor`（lib 名 `monitor_lib`）。

本文件做"开发者打开 src-tauri/ 后第一眼看到的导航"。前端结构见 [`../src/README.md`](../src/README.md)。

## 目录结构

```
src-tauri/
├── Cargo.toml         # 依赖 + 包元数据 + release profile (opt-level=z, lto, strip)
├── tauri.conf.json    # 应用元数据 + bundle (msi / nsis) + CSP + 窗口
├── build.rs           # tauri_build::build()
├── capabilities/
│   └── default.json   # IPC 权限（core / opener / dialog）
├── icons/             # 全套图标（ico / icns / png 各尺寸）
├── gen/               # tauri-build 生成的 schemas (自动)
├── scripts/
│   └── cc.ps1.tpl     # cc 集成 PowerShell helper 模板（include_str! 进 profile_installer）
└── src/
    ├── main.rs        # → lib::run()
    ├── lib.rs         # Tauri Builder + 工作线程编排 + IPC 注册
    ├── paths.rs       # CLAUDE_CONFIG_DIR 三级解析
    ├── messages.rs    # JsonlRecord enum (覆盖全部 type)
    ├── parser.rs      # 按行解析 + BOM
    ├── watcher.rs     # 递归监听 ~/.claude/projects + 活跃过滤
    ├── session_map.rs # 直读 ~/.claude/sessions/<PID>.json + 进程探活
    ├── bind.rs        # cc 集成绑定：ps-await/ps-registry 文件 IPC + EnumWindows 找 marker + SidHwndCache + bring_terminal_to_front
    ├── profile_installer.rs # PowerShell profile 块插入/卸载 + 命令冲突扫描
    ├── auto_launch.rs # auto-launch monitor 开关持久化（~/.claude/claudecode-frontend/auto-launch.json）
    ├── subagent.rs    # load_subagent IPC + description 关联
    ├── event_replay.rs # F5 重放（持锁严格按序）
    ├── history.rs     # 历史浏览器：两级懒加载 + 元数据 + 删除 + resume
    ├── search.rs      # issue #6 历史全文搜索：后台建内存索引 + substring 查询
    ├── tasks.rs       # v2.3.0 (issue #11) Claude task tracker 文件 ~/.claude/tasks/<sid>/ 监听 + IPC
    ├── data_paths.rs  # v2.3.0 (issue #3 A) 透明化：枚举所有持久数据路径 + WebView2 + profile 备份
    ├── config.rs      # load/save_config + Windows 原子写
    ├── logging.rs     # v2.0.0 (issue #4) 滚动 log + EnvFilter reload + ErrorEmitterLayer
    └── bridge.rs      # IPC 事件常量
```

## 模块分工

| 文件 | 角色 | 关键 API |
|---|---|---|
| **lib.rs** | Tauri Builder + setup() + IPC handler 注册 + single-instance plugin (issue #9) + 启动清洗嵌套 CLAUDECODE/CLAUDE_CODE_* 标记 (#24) | `pub fn run()` |
| **paths.rs** | 解析 `.claude` 数据目录（三级回退） | `resolve_claude_dir() / resolve_monitor_data_dir() / resolve_config_path()` |
| **messages.rs** | `JsonlRecord` enum + `ApiMessage` + `ContentBlock` | `JsonlRecord::is_displayable()` |
| **parser.rs** | 单行 JSONL → JsonlRecord | `parse_line(raw)` |
| **watcher.rs** (v2.4 重构 + v2.6 seq) | notify_debouncer_mini 递归监听 projects；ActiveFilter 过滤死 session；**同步全量初始扫**完成后设 `initial_scan_done: AtomicBool`；**一次 process_file 读完一个文件后把所有行作为一批同步调 on_batch**。**v2.6 加 `seqs: HashMap<PathBuf, u64>`** 给每行分配 per-file 单调 seq → 前端 RecordTimeline 按 seq 排序（详 ADR-021） | `spawn_watcher(root, active, on_batch: BatchHandler) → WatcherHandle { force_rescan_tx, initial_scan_done }` |
| **session_map.rs** | 读 sessions/<PID>.json + Win32 进程探活 + 心跳清死 session；**procStart 可选** —— Claude Code 偶发漏写时降级仅 STILL_ACTIVE 判活（详 INVARIANTS § 18）。**v2.6 procStart 比较走 `utils::NetTicks::parse_str` typed API**（ADR-024 newtype 隔离） | `SessionMap::load_with_changes() / is_session_active()` |
| **bind.rs** | cc 集成的核心：监听 `ps-await/`、PS 改窗口标题、EnumWindows 找 marker、写 `ps-registry/`、`SidHwndCache` 持久化 sid↔hwnd、`bring_terminal_to_front` | `BindRegistry::spawn() / SidHwndCache::load() / bring_terminal_to_front` |
| **profile_installer.rs** | PowerShell profile 解析 + cc-monitor BEGIN/END 块插入 / 卸载 / 扫描 / 冲突检测 | `discover_profiles() / install_to_profile / scan_profile / render_cc_code` |
| **auto_launch.rs** | "用 cc 启动 claude 时自动开 monitor" 开关持久化（模块级函数，非 impl 方法） | `auto_launch::{load, save, get_config, set_enabled, update_monitor_path_on_startup}` |
| **subagent.rs** | 父 session 的 Agent tool_use 关联 `<parent>/subagents/agent-*.jsonl` | IPC `load_subagent` |
| **event_replay.rs** (v2.4.2 大小分流，v2.6 状态机简化) | 内存 buffer + frontend-ready 时切块 emit；`on_line_batch` 按 batch 大小分流：< 50 行走 `jsonl-line` 单条 live emit，>= 50 行（如 /resume 灌历史）走 `jsonl-batch` 切块 emit。**v2.6 删了 `replaying` flag + catch-up tail 路径** —— chunked emit 期间 watcher 真新行直接 emit，前端 RecordTimeline 按 seq 自动排到正确位置（详 ADR-022）；切块策略简化为统一 CHUNK_SIZE=600 末块先发（删 head/older 区分）；`replay_and_mark_ready` 自 #20 起 async（块间 pause 用 tokio sleep） | `EventReplay::on_line_batch() / replay_and_mark_ready()（async）/ forget() / buffered_{local,remote}_session_ids()（#19/#20 重放后对账）` |
| **history.rs** | 历史浏览器后端：两级 IPC + metadata + 物理删除 + resume；v2.2 (issue #12) 全部 async + spawn_blocking + Channel 流式 IPC | IPC `list_history_projects / stream_history_sessions_in_project / stream_read_session_jsonl / delete / update_metadata / resume` |
| **search.rs** (issue #6) | 历史全文搜索：后台线程扫 projects/**/*.jsonl 建内存索引（按 session 分组 + 原文/小写副本两份）；默认搜 user/assistant 文本，`include_tools` 附加 tool_use/result/thinking；CLI 注入噪声按 INVARIANT § 20 剥掉；两级匹配（lc.contains 粗筛 + find_ci 精定位 snippet）+ 文本截断封顶。`Arc<SearchIndex>` State | IPC `search_history / get_search_index_status / rebuild_search_index` |
| **tasks.rs** (v2.3.0 issue #11) | Claude Code CLI 的 task 列表读取 + watcher：扫 `<claude_dir>/tasks/<sid>/<id>.json` 跳过 `.lock`/`.highwatermark`/非数字命名；notify-debouncer 100ms 监听整个 tasks 目录递归；变更 → 反推 sid → 重读整目录 → emit `task-update`。tasks_root 不存在时静默不 spawn；半截 JSON 单条 catch 跳过 | `read_session_tasks() / spawn_task_watcher()` + IPC `get_session_tasks` |
| **data_paths.rs** (v2.3.0 issue #3 A) | 透明化展示：枚举 monitor 所有持久路径（config / sid-hwnd-cache / auto-launch / history-metadata / ps-await / ps-registry / logs）+ WebView2 UserDataFolder（用 `app_local_data_dir().join("EBWebView")` 推断）+ PowerShell profile 备份目录。stat 不递归算大小，避免大目录卡 IPC | `collect()` + IPC `get_data_paths` |
| **config.rs** | monitor 自己的 config.json R/W（Windows MoveFileExW 原子） | IPC `load_config / save_config` |
| **logging.rs** (v2.0.0+) | tracing init（在 `tauri::Builder` 之前）+ 滚动 log 文件 + EnvFilter reload Handle + ErrorEmitterLayer（拦 ERROR emit `monitor-error` 给前端弹 toast）+ DiagnosticsConfig R/W | `init() / install_error_emitter() / update_config() / log_file_info()` + 5 个 IPC |
| **bridge.rs** | 事件 / payload 常量与 schema。**v2.6 `JsonlLinePayload` 加 `seq: u64`** 字段（watcher per-file 单调，前端 RecordTimeline 按 seq 排到 DOM） | `events::JSONL_LINE / JSONL_BATCH / SESSION_ENDED / TASKS_UPDATE / SESSION_ACTIVITY`，`JsonlLinePayload { session_id, cwd, path, seq, origin?, message } / SessionEndedPayload / TasksUpdatePayload / SessionActivityPayload` |
| **utils.rs** ⭐ v2.6 大归并 | 跨模块共享 helper：`days_from_civil` (日期换算) / `NetTicks` + `FileTime` newtype (procStart 单位隔离 ADR-024) / `parse_iso8601_ms` + `systime_to_ms` + `now_ms` (时间换算，归并 history/subagent/bind 三处) / `scan_dir_jsons<T, K, F>` (泛型目录扫，归并 session_map+bind 两处) / `atomic_write_json<T>` (Windows ReplaceFileW + dst-not-exist rename fallback) / **v2.8.1** `powershell_encoded_command` (命令 → UTF-16LE base64，给 resume 的 `-EncodedCommand` 用，穿 wt/cmd 不被引号/`;` 切碎，零依赖) | (pub items 完整列表见模块 doc 注释) |

## IPC 清单

注册位置：`lib.rs::run() → invoke_handler![...]`。前端调用方式：`invoke<T>('cmd_name', { args })`。

| 命令 | 参数 | 返回 | 调用方 |
|---|---|---|---|
| `load_config` | — | `Value` | 启动时 / 设置面板打开时 |
| `save_config` | `{ value: Value }` | `()` | 设置面板保存时 |
| `load_subagent` | `{ parentJsonlPath, description, toolUseTimestamp }` | `SubagentLoadResult` | 用户展开 Task 折叠卡 |
| `forget_session` | `{ sessionId }` | `()` | 用户关闭 archived Tab |
| `open_session_in_new_window` (issue #10) | `{ sessionId, title }` | `()` | Tab 右键「在新窗口打开」/ Ctrl+Shift+N，建 `viewer-<sid>` 独立只读窗口 |
| `replay_session_to_window` (issue #10) | `{ sessionId }` (window 注入) | `()` | 独立窗口加载后调，把该 sid 历史定向 emit 给本窗口 |
| `list_history_projects` | — | `HistoryProject[]` | 历史浏览器打开 |
| `stream_history_sessions_in_project` | `{ projectDir, onEntry }` | `u32` (count) | 项目组展开（流式 Channel；v2.2 取代非流式版） |
| `stream_read_session_jsonl` | `{ jsonlPath, onChunk }` | `u32` (count) | 点击历史会话进入只读视图（流式 Channel） |
| `delete_history_session` | `{ sessionId, jsonlPath }` | `()` | 物理删除会话（二次确认后） |
| `update_history_metadata` | `{ sessionId, patch }` | `EntryMetadata` | star / 重命名 / 隐藏 |
| `resume_history_session` | `{ sessionId, cwd }` | `()` | ↺ 按钮（v2.8.1：拉起 wt.exe / powershell.exe，读 profile + `cc` 优先回退 `claude`） |
| `search_history` (issue #6) | `{ query, includeTools, scope?, afterMs?, limit? }` | `SearchResponse` | 历史浏览器「全文」模式回车搜索（scope=all/user/assistant；afterMs=时间下界） |
| `get_search_index_status` (issue #6) | — | `SearchIndexStatus` | 进入全文模式时显示索引就绪 / 进度 |
| `rebuild_search_index` (issue #6) | — | `SearchIndexStatus` | 「重新索引」按钮（大量新会话后） |
| `bring_terminal_to_front` | `{ sessionId }` | `()` | Tab ↗ / `Ctrl+\`` 跳焦 |
| `bring_remote_terminal_to_front` (issue #18) | `{ sessionId }` | `()` | 远端 Tab ↗（按 ccm-rbind 标题缓存的 HWND 拉本地 ssh 窗口；未绑定则现扫一次兜底） |
| `list_session_activity` (issue #23) | — | `SessionActivityPayload[]` | 启动/F5 后拉一次红绿灯快照（增量走 `session-activity` 事件，双路收敛） |
| `bring_monitor_to_front` (v2.4.0 issue #2) | — | `()` | watcher 反推用户在终端输入时，可选拉前 monitor 自身窗口（unminimize + show + set_focus） |
| `cc_integration_status` | `{ commandName }` | `CcStatusResponse` | 设置面板打开 PowerShell 集成区 |
| `cc_integration_scan_path` | `{ path, commandName }` | `ProfileScan` | 用户改路径 / 重新扫描 |
| `cc_integration_preview` | `{ commandName, includeCcFunction }` | `{ code }` | [预览代码] 按钮 |
| `cc_integration_install` | `{ path, commandName, includeCcFunction }` | `()` | [安装] 按钮（写入 BEGIN/END 块） |
| `cc_integration_uninstall` | `{ path }` | `()` | [卸载] 按钮（删除 BEGIN/END 块） |
| `cc_get_auto_launch` | — | `AutoLaunchConfig` | 设置面板加载 auto-launch 状态 |
| `cc_set_auto_launch` | `{ enabled }` | `()` | 用户勾选/取消 auto-launch |
| `get_diagnostics_config` (v2.0.0+) | — | `DiagnosticsConfig` | 设置面板「诊断」区拉当前配置 |
| `set_diagnostics_config` (v2.0.0+) | `{ cfg }` | `RestartHint` | 写新配置 + reload；返回是否需要重启 |
| `get_log_file_info` (v2.0.0+) | — | `LogFileInfo` | 当前 log 路径 / 大小 / 全部 .log 文件列表 |
| `open_log_file` (v2.0.0+) | — | `()` | 用系统默认编辑器打开当前 log 文件 |
| `open_log_dir` (v2.0.0+) | — | `()` | 用资源管理器打开 log 目录 |
| `get_session_tasks` (v2.3.0 issue #11) | `{ sessionId }` | `TaskEntry[]` | Tab 创建时拉一次初始 task 列表（之后变更由 `task-update` 事件推） |
| `get_data_paths` (v2.3.0 issue #3 A) | — | `DataPathsResponse` | 设置面板「数据存储」区打开时调一次，拉所有持久路径 + WebView2 + profile 备份 |

## 事件

后端 → 前端（`Emitter::emit`），全部常量在 `bridge::events`：

| 常量 | 事件名 | payload | 时机 |
|---|---|---|---|
| `JSONL_LINE` | `jsonl-line` | `JsonlLinePayload` | watcher 解析到一行后实时单条 emit |
| `JSONL_BATCH` | `jsonl-batch` | `Vec<JsonlLinePayload>` | event_replay 启动重放时一次性发整个 history Vec |
| `SESSION_ENDED` | `session-ended` | `SessionEndedPayload` | sessions/<PID>.json 被删（session 退出） |
| `TASKS_UPDATE` (v2.3.0 issue #11) | `task-update` | `TasksUpdatePayload {sessionId, tasks}` | tasks/<sid>/ 内任何文件变更（debounce 100ms + dedup by sid） |
| `SESSION_ACTIVITY` (issue #23) | `session-activity` | `SessionActivityPayload {session_id, status, waiting_for}` | sessions/<PID>.json 的官方 status 字段变化时（CLI 仅状态转换时重写文件，天然稀疏；红绿灯：busy=绿 idle/shell=红 waiting=黄） |
| (logging::ERROR_EVENT) | `monitor-error` (v2.0.0+) | `MonitorErrorPayload {level,target,message,timestamp}` | tracing::error! 触发；前端 error-toast.ts 监听 |

前端 → 后端（`Listener::listen`）：

| 事件 | 用途 |
|---|---|
| `frontend-ready` | 触发 event_replay 完整回放历史（持锁严格按序） |

详 [doc/IPC-PROTOCOL.md](../doc/IPC-PROTOCOL.md)（跨进程文件协议）与 [doc/ARCHITECTURE.md § 5](../doc/ARCHITECTURE.md#5-关键设计选择--理由)（事件设计理由）。

---

## 不变量

完整清单在 [doc/INVARIANTS.md](../doc/INVARIANTS.md)，本模块特别相关：

- § 1 — 零侵入（watcher 只读 projects/ + sessions/；history 物理删除是显式例外）
- § 2 — monitor data dir 永远 `~/.claude/claudecode-frontend/`，不跟 claudeDir
- § 3 — 跨进程 JSON UTF-8 无 BOM（双向防御）
- § 4 — profile 写入 `ReplaceFileW` + backup + 校验
- § 5 — JSONL 单一时序（seq 字段 + RecordTimeline binary insert）
- § 6 — session 探活双重校验（PID + procStart）
- § 7 — HWND 拉前三重校验
- § 8 — Tauri State 必须 `app.manage`
- § 9 — 排序硬规则：一律按 seq，禁止按到达顺序
- § 10 — Win32 sync 必须 `spawn_blocking`
- § 11 — 跨平台分裂边界（所有 Win32 调用都在 `#[cfg(windows)]` 块；非 Windows 给 stub）

---

## 关键设计选择 + 理由

### `watcher.rs::path_key()` 用小写归一
Windows 路径大小写不敏感而 `PathBuf::eq` 是字节级比较，notify 偶发以不同大小写回放同文件导致重复 emit。归一到 lowercase 一次性解决。

### `force_rescan_tx` 通道兜底竞态
jsonl 行先于 `sessions/<PID>.json` 落地时，`active_filter` 返 false → `process_file` early return 但 offset 不变 → 下次扫描也不会重读。session-added 信号通过 force_rescan_tx 显式触发一次重扫，把 early return 漏掉的那段补上。

### `config.rs::atomic_replace` 用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
`std::fs::rename` 在 Windows 上 dst 存在时失败（POSIX rename atomic overwrite 行为在 Windows 上没有）。MoveFileExW 是 Windows 原生原子替换 API，专门设计来实现"覆盖现有文件"语义。

### `profile_installer::atomic_write_string` 用 `ReplaceFileW` 而非 `MoveFileExW`
`MoveFileExW(tmp, dst)` 用 tmp 的 ACL 覆盖 dst → 用户 explicit ACE 丢失（Documents 重定向到非默认盘的用户读不了自己的 profile）。**ReplaceFileW 专门设计来保留 dst 的 ACL/ADS/创建时间**。这是 Windows 文档明确推荐用于"替换配置文件"的 API。详 [doc/INVARIANTS § 4](../doc/INVARIANTS.md#4-profile-等用户文件写入--replacefilew--backup--写后校验)。

### `history::resume_impl` 用 `powershell.exe -NoExit -EncodedCommand`（v2.8.1 修复）
旧版用 `cmd /K "claude --resume <sid>"`，有两个 bug：(1) cmd.exe 不是 PowerShell、**更不加载用户 profile** → `cc` wrapper / `__ccm_bind` / 代理 env 全不生效，跑的是裸 `claude`；(2) 退出 claude 后那个壳是 cmd，不认 `cc`。旧注释还把 `pwsh.exe`（PS7，需装）和 `powershell.exe`（PS5.1，系统自带）混为一谈才退回 cmd。

改用系统自带 `powershell.exe -NoExit -EncodedCommand <base64>`：**不带 `-NoProfile`** → 加载 profile → 代理 / `cc` 生效；命令体 `if (Get-Command cc) { cc --resume <sid> } else { claude --resume <sid> }`（装了 wrapper 走 `cc`，没装回退 `claude`，回退也在加载了 profile 的真 PowerShell 里）；`-NoExit` 让 claude 退出后窗口保留且 `cc` 可继续用。命令经 `utils::powershell_encoded_command` 编码（UTF-16LE base64）透过 wt.exe / cmd 多层 shell（绕开引号 / `;` 分隔符），并对 `session_id` 做注入校验（仅 `[A-Za-z0-9_-]`，抽成可测试的 `build_resume_ps_command`）。详 `doc/v2.8.1-bugfix-notes.md`。

### `session_map` 双触发（事件 + 2s 心跳）
仅靠 notify 文件事件不够：用户强杀 claude.exe 时 `~/.claude/sessions/<PID>.json` 不会被 Claude Code 退出 hook 删 → notify 永不触发 → 死 Tab 永远 live。2s 心跳对当前内存中每个 PID 跑 `is_process_alive`，捕获这种"文件还在但进程死了"的状态。

### `bind.rs` 用 marker 字符串而非 PID 反查窗口
PowerShell 进程**不直接拥有终端窗口**（Windows Terminal 是单独进程；conhost 是另一个进程；VSCode integrated terminal 又是另一个）。`EnumWindows + GetWindowThreadProcessId` 反查 owner 会找到 WT / conhost / VSCode 进程，不会找到 PS 自己。改让 PS 把自己窗口标题改成 unique marker（`ccm-bind-<PID>-<8 字符 GUID>`）+ monitor `EnumWindows` 反查 title `contains(marker)` 是唯一可靠的跨进程握手方式。

### cc 集成走文件 IPC 而非命名管道 / TCP
- 简单（PS 写文件 + Rust notify 两边都 trivial）
- 可追溯（用户 / 开发者出问题时可以 `Get-Content` 直接看）
- 无连接管理（管道有 connect / disconnect 状态机，文件是 set-and-forget）
- 跨进程权限简单（用户态读写自己 home 目录的文件不需要任何 ACL 配置）

### 焦点同步功能完全移除
原 `SetWinEventHook` 监听 `EVENT_SYSTEM_FOREGROUND` 然后切对应 Tab 的方向：在 Win11 默认 WT 单进程多窗口/多 tab 架构下，`GetForegroundWindow` 只能拿到 WT 主进程的 HWND，**无法区分同一 WT 窗口内哪个 tab active**。已彻底删除 `FOCUS_SWITCH` IPC 和相关代码。Tab 切换走手动点击 + `Ctrl+Tab` 快捷键。

### `bring_terminal_to_front` 从启发式改为注入式绑定
旧的 "4-tier 启发式"（parent chain + WT 进程 + 终端类进程 + ai-title 匹配）在 explorer 启 PowerShell + WT DefTerm 接管 console 的常见架构下不可靠：claude 祖先链与 WT 窗口完全脱节（claude 的 parent 是 PS，PS 的 parent 是 explorer；WT 是另一个独立进程，跟 claude/PS 没有 parent 关系）。改为 cc 命令注入式绑定（`__ccm_bind` 主动通知 monitor "我是哪个 PID + HWND"）。

详细模块设计见各 `.rs` 文件顶部的 `//!` doc comment。

---

## 添加新功能入口

详细 cookbook 见 [doc/CONTRIBUTING.md § 2](../doc/CONTRIBUTING.md#2-添加新东西-cookbook)。速查：

| 需求 | 入口文件 |
|---|---|
| 新 jsonl 记录类型 | `messages.rs:JsonlRecord` enum 加 variant |
| 新 IPC 命令 | 新建模块 `<feature>.rs` → 在 `lib.rs::run().invoke_handler![]` 注册 |
| 新事件 | `bridge.rs::events` 加常量 + payload 结构 |
| 新跨进程协议文件 | 见 [doc/IPC-PROTOCOL.md § 添加新的跨进程协议文件](../doc/IPC-PROTOCOL.md#添加新的跨进程协议文件) |
| 新 Win32 调用 | `Cargo.toml::[target.cfg(windows)].dependencies.windows.features` 加 feature；用 `#[cfg(windows)]` 包裹 |
| 改 release 打包配置 | `tauri.conf.json::bundle`；详 [doc/BUILDING.md](../doc/BUILDING.md) |
