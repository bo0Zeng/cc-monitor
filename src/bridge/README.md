# 后端模块导览（`src/bridge/`）

Rust + Tauri 2。crate 名 `monitor`（lib 名 `monitor_lib`）。

本文件做"开发者打开 src/bridge/ 后第一眼看到的导航"。前端结构见 [`../README.md`](../README.md)。

> ## ⚠ `src/backend/` 的逐文件清单**不在本文件里**〔G3 加〕
>
> 它住 **`src/bridge/src/backend/mod.rs` 的 `BACKEND_FILES` 登记表** —— 那是**机检**的：
> 往 `backend/` 加文件而不在表里写明「属哪条能力线、为什么在这里」，`cargo test` **当场红**
> （`every_file_under_backend_is_registered_with_a_reason`）。`src/`src/doc/ARCHITECTURE.md` §2.7` 就是这么分派的。
>
> ⇒ **本文件刻意不复制那份清单**：复制出来的第二份没有机检看着，
> 而本仓反复治的正是「同一个事实两处各写一份然后漂移」。
> 本文件收录的是 `backend/` **之外**那些模块。
>
> ⚠ 顺带说清一件容易读错的事：`backend/` 下那些 `.rs`（控制面搬进后端的产物）
> 在下面两张表里**看不到**，那**不是遗漏**，是分派。
> ⚠ **这里刻意不写它有几个**（audit-0805 F18）—— 上一段刚说「本文件刻意不复制那份清单」，
> 紧接着又存一个数就是同一个病的小号：那个数当时写的是 11，实际已经是 15。
> 家在 `backend/mod.rs` 的 `BACKEND_FILES`（有机检看着）。

## 目录结构

```
src/bridge/
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
    ├── local_lines.rs # 本机会话内容的入口通道（本机后端的 line 帧 → 与远端同一个 LineIntake，CF1）
    ├── session_map.rs # 直读 ~/.claude/sessions/<PID>.json + 进程探活
    ├── bind.rs        # cc 集成绑定：ps-await/ps-registry 文件 IPC + EnumWindows 找 marker + SidHwndCache + bring_terminal_to_front
    ├── profile_installer.rs # PowerShell profile 块插入/卸载 + 命令冲突扫描
    ├── auto_launch.rs # auto-launch monitor 开关持久化（~/.claude/claudecode-frontend/auto-launch.json）
    ├── subagent.rs    # load_subagent IPC + description 关联
    ├── adapter.rs     # F-MA agent 适配层：会话布局/解析/活性/resume 假设收敛到 AgentAdapter（CC 第一个实例）
    ├── adapter/
    │   └── claude_code.rs # Claude Code 适配器（第一个实例，零行为变化包旧逻辑）
    ├── event_replay.rs # F5 重放（v2.6 起出锁 emit、顺序靠前端按 seq 排；非旧「持锁严格按序」）
    ├── history.rs     # 历史浏览器：两级懒加载 + 元数据 + 删除 + resume
    ├── launch.rs      # B14-F41 终端拉起单一入口（wt.exe→PowerShell）+ 远端 ssh 拉起（本地 resume 与 F41/F51/F52/F53 共用）
    ├── search.rs      # issue #6 历史全文搜索：后台建内存索引 + substring 查询（含远端结果合并）
    ├── mcp.rs         # F87 MCP 管理：跨 scope 宽容读 / 只写项目 .mcp.json（SS-14 读写分界）
    ├── panorama_call.rs # 〔RM1c · RM1f〕全景（本机远端同一条）：按 origin 问那台后端的 `panorama` 帧命令（后端经插件口起独立全景小程序）· 撤票
    ├── ssh_source.rs  # russh 远端数据源：连接/鉴权/指纹校验 + 后端流帧解析 + 版本协商 + ssh-config 导入 + 测试连接 + B14-F59 daemonless 降级读取(纯 tail 轮询)
    ├── remote_history.rs # 远端历史浏览 + 远端全文搜索查询（一次性 exec 后端子命令，多机 fan-out）
    ├── sftp.rs        # SS-D 统一 SFTP 写层：后端自动部署 (#29) + 远端历史删除 (F11) + ccm 安装 (F10)
    ├── sftp_pool.rs   # SFTP 只做传输（F7c）：per-host 连接池 + 传输台（暂存区上传 · 下载 · 进度流）；〔第四波 S4〕零条 Tauri 命令
    ├── pubkey.rs      # B14-F50 公钥一键推送 authorized_keys（防注入 sanitize + 幂等去重）
    ├── port_forward.rs # B14-F58 本地端口转发(-L)管理台（复用 SSH 连接隧道 direct-tcpip）
    ├── tmux.rs        # B14-F51/F60 tmux 反查 attach + 画面预览快照（capture-pane 只读）
    ├── tmux_reconcile.rs # F74c(#60-A) tmux 存活对账 poller：带外杀 tmux → tab 有界变灰（retire 送 remote_tx 单写者）
    ├── tasks.rs       # v2.3.0 (issue #11) Claude task tracker 文件 ~/.claude/tasks/<sid>/ 监听 + IPC
    ├── data_paths.rs  # v2.3.0 (issue #3 A) 透明化：枚举所有持久数据路径 + WebView2 + profile 备份
    ├── config.rs      # load/save_config + Windows 原子写
    ├── logging.rs     # v2.0.0 (issue #4) 滚动 log + EnvFilter reload + ErrorEmitterLayer
    ├── bridge.rs      # IPC 事件常量
    └── utils.rs       # ⭐ v2.6 跨模块共享 helper（日期/时间换算 + newtype + 目录扫 + 原子写 + PS EncodedCommand）
```

## 模块分工

| 文件 | 角色 | 关键 API |
|---|---|---|
| **lib.rs** | Tauri Builder + setup() + IPC handler 注册 + single-instance plugin (issue #9) + 启动清洗嵌套 CLAUDECODE/CLAUDE_CODE_* 标记 (#24) | `pub fn run()` |
| **paths.rs** | 解析 `.claude` 数据目录（三级回退） | `resolve_claude_dir() / resolve_monitor_data_dir() / resolve_config_path()` |
| **messages.rs** | `JsonlRecord` enum + `ApiMessage` + `ContentBlock` | `JsonlRecord::is_displayable()` |
| **parser.rs** | 单行 JSONL → JsonlRecord | `parse_line(origin, raw)` |
| **local_lines.rs** (CF1) | 本机会话内容的入口通道：本机两条读循环（常驻 TCP · stdio 监护）把本机后端发来的内容帧（`line` / `session_added` / `session_removed`）送进来，交给 `ssh_source::consume_local`，再进与远端同一个 `LineIntake`（攒批 ＋ 静默窗 ＋ 旁路快照 ＋ 续点）。有界通道 ⇒ 消费者跟不上时读循环停读、后端写阻塞（背压，不丢）。原先 monitor 自己那套 jsonl watcher（第二套游标与 seq）随 CF1 删了 | `install / deliver / deliver_blocking / stream_ended` |
| **session_map.rs** | 读 sessions/<PID>.json + Win32 进程探活 + 心跳清死 session；**procStart 可选** —— Claude Code 偶发漏写时降级仅 STILL_ACTIVE 判活（详 INVARIANTS § 18）。**v2.6 procStart 比较走 `utils::NetTicks::parse_str` typed API**（newtype 单位隔离） | `SessionMap::load_with_changes() / is_session_active()` |
| **bind.rs** | cc 集成的核心：监听 `ps-await/`、PS 改窗口标题、EnumWindows 找 marker、写 `ps-registry/`、`SidHwndCache` 持久化 sid↔hwnd、`bring_terminal_to_front` | `BindRegistry::spawn() / SidHwndCache::load() / bring_terminal_to_front` |
| **profile_installer.rs** | 别名块（POSIX `cc` / `cct` · PowerShell `__ccm_bind` ＋ 可选 `cc`）的渲染 / 插入 / 卸载 / 现状 / 冲突检测；〔AL1d〕`$PROFILE` 在哪不归它（只有 `shell_dialect.rs` 答） | `block_state / render_block / install_to_profile / uninstall_from_profile / render_cc_code` |
| **auto_launch.rs** | "用 cc 启动 claude 时自动开 monitor" 开关持久化（模块级函数，非 impl 方法） | `auto_launch::{load, save, get_config, set_enabled, update_monitor_path_on_startup}` |
| **subagent.rs** | 父 session 的 Agent tool_use 关联 `<parent>/subagents/agent-*.jsonl` | IPC `load_subagent` |
| **adapter.rs** + **adapter/claude_code.rs** (F-MA) | agent 适配层：把 cc-monitor 对「Claude Code 具体形态」的假设（会话目录布局 / 记录解析 / 活性 / resume 命令）收敛到 `AgentAdapter` 后，`claude_code.rs` 是第一个实例（**零行为变化**）。第一刀只抽浅耦合点（会话源布局等字面量），不碰记录模型（`JsonlRecord` 暂当规范模型） | `SessionLayout / AgentAdapter`（增量长 trait） |
| **event_replay.rs**（〔CF2 · 第四波 4B〕会话流的句柄 ＋ 重放缓冲） | 内存 buffer（**每个会话只留 seq 最高的 600 条**，摊还余量 150；丢掉的前端按字节 / 按行号取回）＋ 会话流订阅表：`on_line_batch_awaited`（〔CF1〕唯一入口）进缓冲并**当场**交进各条已过就绪点的订阅（< 50 行逐行一格；≥ 50 行切块、带 `batch` 边界、块间 tokio sleep，交完才返回 —— 行先于随后的归档）；有 credit 才交，没有就丢、原位报 `Gap`；`ready_point(priority_sid)`（frontend-ready 那个任务里）按 credit 交留存（不丢，等 `want`），优先会话的块先发 | `EventReplay::on_line_batch_awaited() / ready_point(priority_sid)（async）/ subscribe() / want() / stop() / origin_seen() / forget() / buffered_{local,remote}_session_ids()（#19/#20 重放后对账）` |
| **history.rs** | 历史会话的 monitor 这一侧：读一整份会话（Channel 分块）+ 物理删除（经那台机器的后端）+ resume / 分叉的命令渲染。〔C4d · 第四波 4B〕项目 / 会话清单与注解（星标 / 改名 / 隐藏 / 上次账号）搬进本机常驻后端（`history-projects` / `history-sessions` / `history-annotate` / `history-last-accounts`，界面经 `src/history-reads.ts` 问），注解那份文件原地不动、路径仍由本文件 `metadata_path` 算 | IPC `stream_read_session_jsonl / delete_history_session / create_branch_session / resume` |
| **launch.rs** (B14-F41) | 终端拉起单一入口：`launch_powershell_window`（从 `history.rs::resume_impl` 抽出，wt.exe Plan A → `CREATE_NEW_CONSOLE` Plan B，`-NoExit -EncodedCommand` 不带 `-NoProfile`）+ `build_remote_ssh_ps_command`（`ssh -t … "bash -lic '<cmd>'"`）+ `launch_remote_terminal`；本地 resume 与远端族 F41 resume / F51 attach / F52 tmux / F53 launcher 共用此单一入口；命令为 async（`spawn_blocking` 起窗）。三层引号/注入防线各自独立 | `launch_powershell_window() / build_remote_ssh_ps_command() / launch_remote_terminal()` + IPC `launch_remote_terminal` |
| **search.rs** (issue #6) | 历史全文搜索：后台线程扫 projects/**/*.jsonl 建内存索引（按 session 分组 + 原文/小写副本两份）；默认搜 user/assistant 文本，`include_tools` 附加 tool_use/result/thinking；CLI 注入噪声按 INVARIANT § 20 剥掉；两级匹配（lc.contains 粗筛 + find_ci 精定位 snippet）+ 文本截断封顶。`Arc<SearchIndex>` State | IPC `search_history / get_search_index_status / rebuild_search_index` |
| **mcp.rs** (F87 #50+#51 / F89a-b #远端MCP) | MCP 管理（SS-14 读写分界）：**读**跨 scope 宽容展示（本机用户/local/项目 + **远端** user scope via SSH-exec cat / **远端项目** `.mcp.json` via SFTP）；**写只**项目 `.mcp.json`——本机 `mcp_json_path` 硬编码 / **远端** `is_safe_remote_mcp_json` 守卫经 `sftp::upload_atomic`（SS-14：绝不写 `~/.claude.json`/`settings.json`，本机远端皆然；SS-G 用户显式触发，见 INVARIANTS §1 例外 #5） | `read_mcp_servers / list_mcp_project_dirs / write_project_mcp_server / remove_project_mcp_server`（〔步 12·C〕后三条**都吃 `origin`，两侧各一条**）` / read_remote_mcp_servers / read_remote_project_mcp / list_remote_mcp_origins` —— 远端那三个写函数（`write_remote_mcp_server` / `remove_remote_mcp_server` / `list_remote_mcp_project_dirs`）**已不是命令**，是上面那三条的远端分支 |
| **sftp_pool.rs** (B14-F47/F49) | SFTP 文件面板后端:per-host utility 连接池(与后端流分离)+ 浏览/传输/写命令 + 小文件编辑(F49:`decode_editable` 三防护 + `sftp_read_text_for_edit`〔散文墓碑〕/`sftp_write_text`);防误伤守卫**已搬走** —— 见下面 `claude_data_fence.rs` 那一行(池子这边只剩一行 `pub use`) | `with_sftp() / sftp_list_dir / sftp_download / sftp_upload / ...`(11 命令) |
| **claude_data_fence.rs** (步 H2) | **哪些路径是 Claude 自己的数据,不许我们写** —— 一个判定 ＋ 它的拒绝,别无他物(零 IO / 零 async ⇒ 「被挡住」在一台没有连接的机器上判得动)。`INVARIANTS §1` 底下 **F47**(SFTP 面板 / 原生文件窗口)与 **F03b**(收件箱编辑的纵深②)两段澄清共用它这**一个**判定;〔用户 2026-09-21 逐字裁「拆」,`设计/99 §2 Q2`〕从 `sftp_pool.rs` 搬出,**判定的射程一个字没动**。⚠ 方向相反的那一道从前是 `sftp.rs` 里的 `is_safe_remote_jsonl`〔散文墓碑〕(〔RW1 · 第四波 09-24〕随 F11 改经远端后端删走了,今天住后端 `session_file_for_delete`;下面是原话:「只许删 projects 下的 jsonl」),**两道不许互相替代** | `is_protected_claude_data_path() / guard_write()` |
| **ssh_source.rs** (issue #15) | russh 远端数据源：`connect_session` 全套 host-key 指纹校验 + publickey/agent 鉴权；`run` 长连接 exec 后端把流帧（`InboundFrame`）的内容那一半交 `LineIntake`（〔CF1〕本机那条流 `consume_local` 用同一个）；hello 带 `build_id` 做版本协商（#33）；`Overflow` 帧 → remote-health 提示（#32）；ssh-config 导入 + 测试连接。**B14-F56 跳板**：`RemoteConfig.jump`（另一主机 label）有值时 `connect_via_jump`——connect_session(跳板)→ `channel_open_direct_tcpip` → `connect_stream` 跑目标 SSH（隧道上验目标指纹）；跳板 session 存 `jump_holders` 保活；fail-closed（环/查无/连不上 → Err）。**B14-F59 daemonless 降级**：`RemoteConfig.daemonless=true`（per-host 开关）时 `run()` 顶层二选一走 `daemonless_stream_loop`（不连后端，持久会话上 `exec_on_session` 跑 `find`+`tail -c +offset` 轮询读 jsonl，`drain_complete_lines`/`plan_file_read` 复刻 watcher 增量语义、复用 `flush_lines` 下游），default-false 时后端路径 `stream_loop` 一行不动；能力子集经 `degraded` remote-health 如实提示 | `run() / stream_loop() / daemonless_stream_loop() / connect_session() / connect_via_jump() / connect_and_exec_cmd() / parse_frame()` + IPC `list_ssh_host_aliases / resolve_ssh_host / test_remote_connection` |
| **remote_history.rs** (issue #16/#28/#30) | 远端会话的 monitor 这一侧：读一整份远端会话（长连接 `history-read` 分页）+ 删一份远端会话（经那台的后端）。〔C4d · 第四波 4B〕远端项目 / 会话清单与它们的 join 搬进本机常驻后端（`history_join.rs`，远端那一跳经 `remote_ask`），逐次拨号那条路也删了；〔C4a〕远端全文搜索早已搬去前端 `views/history-search.ts`。〔`设计/50`：原先还有一条**远端用量聚合**（`--usage` / `aggregate_remote_usage_all`），随用量 ② 轴整轴退役〕 | 两个远端分支函数：`stream_read_remote_session` · `delete_remote_history_session`（〔步 12·C 09-20〕并进了本机那两条同名命令，`origin` 成了参数） |  〔散文墓碑〕
| **pubkey.rs** (B14-F50) | 公钥一键推送 authorized_keys（aterm N2）：`sanitize_public_key`（单一非空行防注入 + 类型校验）+ `build_authorized_keys_cmd`（`printf`/`grep -qxF`/`chmod 700/600` + ADDED/ALREADY）+ `parse_push_outcome`；复用 `connect_and_exec_cmd`（只消费不改形）+ `shell_quote`。三纯函数单测 | `sanitize_public_key() / build_authorized_keys_cmd() / parse_push_outcome()` + IPC `push_public_key` |
| **port_forward.rs** (B14-F58) | 本地端口转发(-L)管理台后端:每转发一条独立 `connect_session`（继承竞速/跳板）+ 本地 `TcpListener` + accept 循环,每连接开 `channel_open_direct_tcpip` + `copy_bidirectional`；session 存 `Arc` 注册表保活（russh Handle 不 Clone）；停 = abort accept + **`session.disconnect` 主动断连**（仅 drop 关不掉连接:Handle::drop no-op + 在飞连接持 sender clone,D 审计实证）。v1 即席不持久化 | `start_forward()/stop_forward()/list_forwards()` + 同名 IPC |
| **tmux.rs** (B14-F51/F60) | tmux 反查（tab 右键 attach）+ **F60 画面预览**：`parse_tmux_ls`（**真 TAB 分列**，调研 03 §3.1 坑）+ `list_remote_tmux`（`command -v tmux` 门控→哨兵 NO_TMUX 返 None）+ **`capture_remote_pane`（`tmux capture-pane -p -t <会话>` 抓当前屏只读快照，`command -v tmux` 门控 + `\|\| printf NO_PANE` 兜会话不存在，`classify_capture_output` 纯函数判哨兵）**；均走 `connect_and_exec_cmd` 只消费不改形，target 经 `shell_quote`。前端按 cwd+`pane_current_command∈{claude,node}` 反查，命中并列「Attach」+「预览画面」。（F52 resume-tmux 短路门未扩本模块） | `parse_tmux_ls() / classify_capture_output()` + IPC `list_remote_tmux / capture_remote_pane` |
| **tmux_reconcile.rs** (F74c issue #60-A) | tmux 存活对账 poller：补一条独立 tmux 存活信号，让带外（`Ctrl-b &` / `tmux kill-session`）杀掉某会话 tmux 后端时对应 tab 有界变灰。**§24 单写者不破**——retire 的 sid 当 `SessionChange{removed}` 送进 `remote_tx` 的一个 clone、由唯一写者处理；source-agnostic（`reconcile_step` 吃裸 HashSet，F90 可整段 lift）；三重防误判（ever_bound 门 + debounce + /branch 漂移剔除） | `reconcile_step()`（纯函数）+ poller |
| **sftp.rs** (SS-D, issue #29) | 统一 SFTP 写层（复用 ssh_source 鉴权起 sftp 子系统）：F08 后端自动部署（arch 探测 + build_id 版本门控 + 原子上传）+ F11 远端历史 jsonl 删除（双重路径白名单 + realpath 防 symlink 逃逸）+ F10 远端 ccm 装进 `~/.bashrc`（BEGIN/END 块 + 备份 + 写后校验）+ **F89a 远端项目 `.mcp.json` 增改删（`mcp.rs` 经 `upload_atomic`，`is_safe_remote_mcp_json` 守卫，SS-14 只 .mcp.json）**。`upload_atomic` 加固：tmp 用 EXCLUDE 防 symlink clobber + 旧目标备份 `.bak`（失败可恢复、成功即清）。只读铁律豁免（穷举）见 `src/`src/doc/INVARIANTS.md` §1` + 模块文档 | `ensure_backend_deployed() / remove_remote_file() / upload_atomic()` + IPC `install_remote_alias_block`（〔MC1〕从前叫 `install_remote_ccm_helper`〔散文墓碑〕；〔步 12·C 收尾 09-20〕原先这里还列着 `write_remote_mcp_server`，它**已不是 IPC** —— 今天是 `mcp::write_project_mcp_server` 的远端分支） |
| **tasks.rs** (v2.3.0 issue #11；〔RM1b〕读法归后端) | Claude Code CLI 的 task 列表：按 `origin` 问那台机器的后端 `tasks-list`（本机与远端同一条路；读 `<tasks>/<sid>/<数字>.json` 那一段住后端 `observe/tasks_query.rs`），本侧只剩字段语义与本机 watcher：notify-debouncer 100ms 监听整个 tasks 目录递归；变更 → 反推 sid → 经本机后端重读 → emit `task-update`。tasks_root 不存在时静默不 spawn | `parse_task_lines() / spawn_task_watcher()` + IPC `get_session_tasks(origin, sessionId)` |
| **data_paths.rs** (v2.3.0 issue #3 A) | 透明化展示：枚举 monitor 所有持久路径（config / sid-hwnd-cache / auto-launch / history-metadata / ps-await / ps-registry / logs）+ WebView2 UserDataFolder（用 `app_local_data_dir().join("EBWebView")` 推断）+ PowerShell profile 备份目录。stat 不递归算大小，避免大目录卡 IPC | `collect()` + IPC `get_data_paths` |
| **config.rs** | monitor 自己的 config.json R/W（Windows MoveFileExW 原子） | IPC `load_config / save_config` |
| **logging.rs** (v2.0.0+) | tracing init（在 `tauri::Builder` 之前）+ 滚动 log 文件 + EnvFilter reload Handle + ErrorEmitterLayer（拦 ERROR emit `monitor-error` 给前端弹 toast）+ DiagnosticsConfig R/W | `init() / install_error_emitter() / update_config() / log_file_info()` + 5 个 IPC |
| **bridge.rs** | 事件 / payload 常量与 schema。**v2.6 `JsonlLinePayload` 加 `seq: u64`** 字段（后端给的 per-file 行号，前端 RecordTimeline 按 seq 排到 DOM）。〔CF2〕会话内容不再是事件：流里一格的体是 `SessionStreamFrame`（`{"line": …}` / `{"batch": "start"｜"end"}`） | `events::SESSION_ENDED / TASKS_UPDATE / SESSION_ACTIVITY …`，`JsonlLinePayload { session_id, cwd, path, seq, origin?, message } / SessionStreamFrame / SessionEndedPayload / TasksUpdatePayload / SessionActivityPayload` |
| **utils.rs** ⭐ v2.6 大归并 | 跨模块共享 helper：`days_from_civil` (日期换算) / `NetTicks` + `FileTime` newtype (procStart 单位隔离) / `parse_iso8601_ms` + `systime_to_ms` + `now_ms` (时间换算，归并 history/subagent/bind 三处) / `scan_dir_jsons<T, K, F>` (泛型目录扫，归并 session_map+bind 两处) / `atomic_write_json<T>` (Windows ReplaceFileW + dst-not-exist rename fallback) / **v2.8.1** `powershell_encoded_command` (命令 → UTF-16LE base64，给 resume 的 `-EncodedCommand` 用，穿 wt/cmd 不被引号/`;` 切碎，零依赖) | (pub items 完整列表见模块 doc 注释) |

## IPC 清单

注册位置：`lib.rs::run() → invoke_handler![...]`。前端调用方式：`invoke<T>('cmd_name', { args })`。

| 命令 | 参数 | 返回 | 调用方 |
|---|---|---|---|
| `load_config` | — | `Value` | 启动时 / 设置面板打开时 |
| `save_config` | `{ value: Value }` | `()` | 设置面板保存时 |
| `read_mcp_servers` (F87 #50) | `{ projectDir? }` | `McpServerEntry[]` | MCP 段打开：跨 scope 宽容读（用户 `~/.claude.json` / local / 项目 `.mcp.json`；缺/坏跳过） |
| `list_mcp_project_dirs` (F87 #50) | — | `String[]` | MCP 段项目目录输入框 datalist（用过的项目自动补全） |
| `write_project_mcp_server` (F87 #51) | `{ origin, projectDir, name, server }` | `()` | 增/改项目 `<dir>/.mcp.json` 的一个 server（**只写 .mcp.json**）。〔步 12·C 收尾 09-20〕**吃 `origin`，两侧一条**；本机逐字送 `"<local>"` |
| `remove_project_mcp_server` (F87 #51) | `{ origin, projectDir, name }` | `()` | 从项目 `<dir>/.mcp.json` 删一个 server。〔步 12·C 收尾〕同上，**吃 `origin`，两侧一条** |
| `read_remote_mcp_servers` (F87b #52) | `{ origin }` | `McpServerEntry[]` | 跨机**只读**远端 user scope（SSH-exec `cat ~/.claude.json`，机器全局 MCP） |
| `read_remote_project_mcp` (F89a) | `{ origin, projectDir }` | `McpServerEntry[]` | 只读远端某项目 `.mcp.json`（SFTP） |
| `list_remote_mcp_origins` (F87b) | `{}` | `String[]` | 远端机器选择器 |
<!-- 〔步 12·C 09-20〕`list_remote_mcp_project_dirs` **已退役** —— 并进了
     `list_mcp_project_dirs`（见下面那张表），两侧算它的那份代码本来就只有一份
     （`project_dirs_from`），差别只在那份 `~/.claude.json` 的字节从哪来。 -->
<!-- 〔步 12·C 收尾 09-20〕`write_remote_mcp_server` / `remove_remote_mcp_server`
     **已退役** —— 并进了上面那两条 `*_project_*`（同一个写面 `<dir>/.mcp.json`、
     同一份纯核心 `upsert_mcp_server_value` / `remove_mcp_server_value`，
     差别只在字节走 SFTP 还是走盘）。
     ⚠ **它们作为函数还在**（`mcp.rs` 里的 `pub(crate) async fn`，是合并后那两条命令的
     远端分支，路由表与欠账表都按这两个名字登记着）—— 别把「函数还在」读成「命令还在」。 -->
| `list_remote_accounts / check_account_trust` (A2 #68/#69) | `{ origin }` / `{ origin, dir }` | `AccountsResult / bool` | 多账号**只读**查询（各账号名/邮箱/登录态 · 目录是否可信）——账号=一个 `CLAUDE_CONFIG_DIR`，经后端纯只读（`accounts.rs`，全 stateless）。〔C4a〕「某会话属哪个账号」那一条退役：前端经通道 `chan_call` 直接说帧命令 `accounts-sessions`（本机与远端同一条路） |
| `load_subagent` | `{ parentJsonlPath, description, toolUseTimestamp }` | `SubagentLoadResult` | 用户展开 Task 折叠卡 |
| `forget_session` | `{ sessionId }` | `()` | 用户关闭 archived Tab |
| `open_session_in_new_window` (issue #10) | `{ sessionId, title }` | `()` | Tab 右键「在新窗口打开」/ Ctrl+Shift+N，建 `viewer-<sid>` 独立只读窗口 |
| `chan_subscribe` / `chan_want` / `chan_stop`（〔CF2〕） | `{ origin, kind, from, want, id }` / `{ id, more }` / `{ id }`（webview 注入） | `()` | 通道 `subscribe` 在 Tauri IPC 那一跳：会话内容流（`session-lines` / `session-lines/<sid>`），交格走事件 `chan-items`；经 `src/ipc/chan.ts` 用 |
| 〔C4d · 第四波 4B〕历史清单那两条（本机项目 · 展开一个项目）已删 | — | — | 界面经通道问本机常驻后端 `history-projects` / `history-sessions`（`src/history-reads.ts`；远端那台由它沿池里那条 SSH 去问） |
| `stream_read_session_jsonl` | `{ origin, jsonlPath, onChunk }` | `u32` (count) | 点击历史会话进入只读视图（流式 Channel）。〔步 12·C 09-20〕**本机与远端合成了一条**，`origin` 是它的参数（`设计/00 §2.5 ①`）。旧的远端命令名**已退役、不留别名** |
| `delete_history_session` | `{ origin, sessionId, jsonlPath }` | `()` | 物理删除会话（二次确认后）。〔步 12·C 09-20〕**本机与远端合成了一条**，`origin` 是它的参数（`设计/00 §2.5 ①`）。旧的远端命令名**已退役、不留别名** |
| `create_branch_session` (F62) | `{ origin, sourceSessionId, messageUuid }` | `BranchResult{ sessionId, jsonlPath }` | 〔步 12·C 09-20〕**本机与远端合成了一条**，`origin` 是它的参数（`设计/00 §2.5 ①`）。旧的远端命令名**已退役、不留别名**。 从历史某轮建分支：复制 `[根…该消息]` 前缀成新 `<new-sid>.jsonl`（原生 `forkedFrom` 格式，原会话不改，只写新 sid、已存在则拒）。〔`K-R88`〕入参与远端那条同形（都收 sid）；找那份源文件走两侧共用的 `branch_core::find_session_file`。§1 正交澄清 |
| 〔C4d · 第四波 4B〕改注解 / 上次账号表那两条已删 | — | — | 注解的读写者是本机常驻后端：`history-annotate` / `history-last-accounts`（删会话之后界面另交 `history-forget`） |
| `resume_history_session` | `{ sessionId, cwd, launcher? }` | `()` | ↺ 按钮（v2.8.1：拉起 wt.exe / powershell.exe，读 profile + `cc` 优先回退 `claude`；F34 起 `launcher` 自定义启动命令）。F62 建分支后一键 resume 复用此命令 |
| `launch_remote_terminal` (B14-F41) | `{ origin, remoteCmd }` | `()` | 远端一键 resume（tab 右键 / 历史 ↺）：按 origin 取 RemoteConfig，拉起 wt.exe/PowerShell 跑 `ssh -t … "bash -lic '<remoteCmd>'"`；ssh.exe 预检失败/校验拒 → Err（前端回退复制命令）。remote_cmd 双层防线（前端构造校验 + 本侧控制字符/双引号/长度再验） |
| 〔F7c 收尾 09-24〕池子那十二条（`sftp_realpath` · `sftp_list_dir` · `sftp_stat` · `sftp_download` · `sftp_upload` · `sftp_cancel_transfer`〔散文墓碑〕 · `sftp_mkdir` · `sftp_rename` · `sftp_delete` · `sftp_read_text_for_edit`〔散文墓碑〕 · `sftp_write_text` · `sftp_chmod`）已删 | — | — | 老面板删了；文件窗口的浏览 / 写 / 读文本走后端 `files-*`，上传 / 下载经通道开单（`transfer-upload` / `transfer-download`）、订阅 `transfer/<id>` 进度（`设计/60 §13b`）。池子的 Tauri 命令只剩 `sftp_copy`；〔第四波 S4〕它也随门禁 `f3-copy` 那一格退役删了 ⇒ 零条 |
| `push_public_key` (B14-F50) | `{ cfg, pubKeyPath? }` | `PushResult {outcome,pubPath}` | 公钥推送 authorized_keys;pubKeyPath 空则取 `{keyPath}.pub`;`sanitize_public_key`(单行防注入)+ `grep -qxF` 去重,返回 added/already |
| `list_remote_tmux` (B14-F51) | `{ origin }` | `TmuxSession[] \| null` | tab 右键 attach 反查;`command -v tmux` 无 → `null`(隐藏 attach);否则 `tmux ls -F`(真 TAB)解析成会话列表;走 exec 通道 |
| `capture_remote_pane` (B14-F60) | `{ origin, target }` | `String` | tab 右键「预览远端 tmux 画面」;`tmux capture-pane -p -t <会话>` 抓当前屏只读快照;`command -v tmux` 门控 + `NO_PANE` 哨兵(会话不存在),`classify_capture_output` 纯函数判;走 exec 通道 target 经 `shell_quote` |
| `panorama_call` (RM1c · RM1f) | `{ origin, op, repo, args, ticket? }` | `unknown`（形状随 op） | 全景的每一问（本机远端同一条，RM1f 起本机也是）：按 origin 问那台机器的后端 `panorama`（后端经插件口起全景小程序）；那台缺 / 旧 ⇒ 远端推一份、本机放一份，再问一次；带 `ticket` 的那一问能被 `panorama_cancel` 撤掉。〔RM1f：进程内那十七条 `panorama_*` 本机命令随内嵌引擎删了〕 |
| `panorama_edit` (RM1d) | `{ origin, repo, op, args }` | `unknown`（id / 在不在 / `null`，随 op） | 批注 / 文档关联的写（本机远端同一条，V110「引擎只算、文件管理来写」）：`op` ∈ 人写 / 提议 / 批准 / 删批注 · 写 / 删文档关联；那台机器算出新内容，落盘经那台机器后端的 `files-put`（带 CAS）/ `files-delete` |
| `panorama_cancel` (RM1f) | `{ ticket }` | `boolean`（那张票此刻在不在飞） | 撤掉一问在飞的全景（「取消建立索引」）：那一问被丢 ⇒ 补发 `cancel` ⇒ 后端杀掉小程序那一组子进程 |
| `start_forward` (B14-F58) | `{ spec: {origin,localPort,remoteHost,remotePort} }` | `String`(id) | 启动本地端口转发:校验→connect_session→bind 127.0.0.1:localPort→accept 循环隧道 direct-tcpip;返回转发 id |
| `stop_forward` (B14-F58) | `{ id }` | `()` | 停止转发:abort accept 循环 + drop session 关连接 |
| `list_forwards` (B14-F58) | — | `ForwardStatus[]` | 列所有转发(id/spec/state/connCount) |
| `search_history` (issue #6) | `{ query, includeTools, scope?, afterMs?, limit? }` | `SearchResponse` | 历史浏览器「全文」模式回车搜索（scope=all/user/assistant；afterMs=时间下界） |
| `get_search_index_status` (issue #6) | — | `SearchIndexStatus` | 进入全文模式时显示索引就绪 / 进度 |
| `rebuild_search_index` (issue #6) | — | `SearchIndexStatus` | 「重新索引」按钮（大量新会话后） |
| `bring_terminal_to_front` | `{ sessionId }` | `()` | Tab ↗ / `Ctrl+\`` 跳焦 |
| `bring_remote_terminal_to_front` (issue #18) | `{ sessionId }` | `()` | 远端 Tab ↗（按 ccm-rbind 标题缓存的 HWND 拉本地 ssh 窗口；未绑定则现扫一次兜底） |
| `list_session_activity` (issue #23) | — | `SessionActivityPayload[]` | 启动/F5 后拉一次红绿灯快照（增量走 `session-activity` 事件，双路收敛） |
| `list_active_sessions` (Batch5-F18) | — | `ActiveSessionPayload[] {session_id, cwd}` | frontend-ready 前拉一次本地活跃清单建骨架 Tab（按 (cwd,sid) 排序防 tab 栏洗牌；远端骨架走 `remote-session-added` 事件） |
| `bring_monitor_to_front` (v2.4.0 issue #2) | — | `()` | watcher 反推用户在终端输入时，可选拉前 monitor 自身窗口（unminimize + show + set_focus） |
| `aliases_read` (〔AL1d〕并进了原「终端集成」的状态 / 扫一份) | `{ shell, rcPath? }` | `AliasListing` | 展开「别名」：清单 ＋ 启动文件候选（各带别名块现状）＋ 握手终端数 |
| `aliases_block_render` | `{ rcPath, withCc }` | `string` | [预览别名块] 按钮（方言按那份文件的扩展名定） |
| `aliases_block_install` | `{ rcPath, withCc }` | `()` | [装别名块] 按钮（写入 BEGIN/END 块，经本机后端） |
| `aliases_block_remove` | `{ rcPath }` | `()` | [卸载别名块] 按钮（删除 BEGIN/END 块，经本机后端） |
| `cc_get_auto_launch` | — | `AutoLaunchConfig` | 设置面板加载 auto-launch 状态 |
| `cc_set_auto_launch` | `{ enabled }` | `()` | 用户勾选/取消 auto-launch |
| `get_diagnostics_config` (v2.0.0+) | — | `DiagnosticsConfig` | 设置面板「诊断」区拉当前配置 |
| `set_diagnostics_config` (v2.0.0+) | `{ cfg }` | `RestartHint` | 写新配置 + reload；返回是否需要重启 |
| `get_log_file_info` (v2.0.0+) | — | `LogFileInfo` | 当前 log 路径 / 大小 / 全部 .log 文件列表 |
| `open_log_file` (v2.0.0+) | — | `()` | 用系统默认编辑器打开当前 log 文件 |
| `open_log_dir` (v2.0.0+) | — | `()` | 用资源管理器打开 log 目录 |
| `get_session_tasks` (v2.3.0 issue #11) | `{ sessionId }` | `TaskEntry[]` | Tab 创建时拉一次初始 task 列表（之后变更由 `task-update` 事件推） |
| `get_data_paths` (v2.3.0 issue #3 A) | — | `DataPathsResponse` | 设置面板「数据存储」区打开时调一次，拉所有持久路径 + WebView2 + profile 备份 |
| 〔C4d · 第四波 4B〕远端项目清单那一条已删 | — | — | fan-out 搬到前端（`src/history-reads.ts::fetchRemoteProjects`：问 `list_remote_mcp_origins` 拿台名单，逐台经本机后端问） |
〔步 12·C 09-20〕**这里原先有三行** —— `stream_remote_history_sessions`〔散文墓碑〕（〔C4d〕这个函数也删了：远端会话清单改由本机后端问） ·
`stream_read_remote_session` · `delete_remote_history_session`。三条都已**退役**：
它们并进了本机那三条同名能力，`origin` 成了参数（见下面「本机历史」那张表）。
⚠ 它们作为**函数**还在 `remote_history.rs` 里（合并后那条命令的远端分支），
只是不再是 IPC 命令 —— 别把「函数还在」读成「命令还在」。
| `install_remote_alias_block` (F10, SFTP) | `{ cfg, profile }` | `String` | 把别名块写进远端 rc（BEGIN/END 块 + 备份 + 写后校验）。〔MC1〕从前叫 `install_remote_ccm_helper`〔散文墓碑〕，推入口那一半并进了 `deploy_remote_backend` |
| `uninstall_remote_alias_block` (F10, SFTP) | `{ cfg, profile }` | `String` | 从远端 rc 删别名块（备份 + 写后校验回滚；块外内容不动） |
| `deploy_remote_backend` (F08c, SFTP) | `{ cfg }` | `String` | 设置面板「安装后端」：按远端 arch 选内嵌二进制 + build_id 版本门控 + SFTP 原子上传到 backendPath（已最新则跳过）；返回人读结果，无 arch/路径含 `~` 等显式报错 |
| `uninstall_remote_backend` (F08c, SFTP) | `{ cfg }` | `String` | 设置面板「卸载后端」：删远端后端二进制 + 同目录 `.build_id`（`is_safe_remote_backend_path` 守卫；机器仍启用会自动装回的提示） |
| `list_ssh_host_aliases` (issue #15) | — | `String[]` | 设置面板「从 ~/.ssh/config 导入」下拉 |
| `resolve_ssh_host` (issue #15) | `{ alias }` | `ResolvedHost` | 选中别名后用 `ssh -G` 解析有效连接参数自动填表（F57 加 proxyJump） |
| `import_ssh_hosts` (B14-F57) | — | `ImportGroup[]` | 批量导入:list 别名→逐个 `ssh -G`→`aggregate_ssh_hosts` 智能聚合(同 key+user+基名前缀→同机多地址,ProxyJump→jump)→预览组（含 members 供拆分） |
| `test_remote_connection` (issue #15) | `{ cfg }` | `ConnTestResult` | 「测试连接」：实连一次回 SSH ✓/✗ + 指纹 + 后端 hello |

## 事件

后端 → 前端（`Emitter::emit`），全部常量在 `bridge::events`：

| 常量 | 事件名 | payload | 时机 |
|---|---|---|---|
| （`chan/webview.rs::ITEMS_EVENT`） | `chan-items` | `{ sub, items }` | 〔CF2〕会话流的交格（原 `jsonl-line` / `jsonl-batch` 两个事件退役）；定向发给订阅所在的 webview |
| `SESSION_ENDED` | `session-ended` | `SessionEndedPayload` | sessions/<PID>.json 被删（session 退出） |
| `SESSION_STARTED` (resume 复活) | `session-started` | `SessionStartedPayload {session_id}` | 本地会话重新变活（sessions/<PID>.json 新增 **且 PID 探活通过**）时 emit——session-ended 的对称面；前端 `tabs.reviveTab` 复活已归档本地 Tab（`/resume` 免 F5）。`is_session_active` 门控避免崩溃残留旧 PID.json 误复活 |
| `TASKS_UPDATE` (v2.3.0 issue #11) | `task-update` | `TasksUpdatePayload {sessionId, tasks}` | tasks/<sid>/ 内任何文件变更（debounce 100ms + dedup by sid） |
| `SESSION_ACTIVITY` (issue #23) | `session-activity` | `SessionActivityPayload {session_id, status, waiting_for}` | sessions/<PID>.json 的官方 status 字段变化时（CLI 仅状态转换时重写文件，天然稀疏；红绿灯：busy=绿 idle/shell=红 waiting=黄） |
| `REMOTE_HEALTH` (SS-F, issue #32/#33) | `remote-health` | `RemoteHealthPayload {origin, kind, message}` | 远端健康提示：后端管道拥塞丢帧（kind=`overflow`，#32）/ 版本不符（kind=`version`，#33）→ 前端 remote-health.ts 按 origin 节流弹 toast |
| `REMOTE_SESSION_ADDED` (Batch5-F18) | `remote-session-added` | `RemoteSessionAddedPayload {session_id, origin}` | 后端 session_added 帧透传（ssh_source 同步直发，先于该会话的行）→ 前端建远端骨架 Tab；进 events.ts 同一 queue 保序 |
| (logging::ERROR_EVENT) | `monitor-error` (v2.0.0+) | `MonitorErrorPayload {level,target,message,timestamp}` | tracing::error! 触发；前端 error-toast.ts 监听 |

前端 → 后端（`Listener::listen`）：

| 事件 | 用途 |
|---|---|
| `frontend-ready` | 触发 event_replay 完整回放历史。Batch5-F19 起 payload 带 `{prioritySid}`（`FrontendReadyPayload`，bridge.rs）——replay 按 session 分组、该 tab 的块先发；缺省 → 不分组。（"持锁严格按序"已废：v2.6 起 snapshot 出锁 emit、前端按 seq 排） |

详 [src/`src/doc/IPC-PROTOCOL.md`](../`src/doc/IPC-PROTOCOL.md`)（跨进程文件协议）与 [src/`src/doc/ARCHITECTURE.md` § 5](../`src/doc/ARCHITECTURE.md`#5-关键设计选择--理由)（事件设计理由）。

---

## 不变量

完整清单在 [src/`src/doc/INVARIANTS.md`](../`src/doc/INVARIANTS.md`)，本模块特别相关：

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

### 本机会话内容不再由 monitor 自己 watch（CF1 · 2026-09-24）
这里原来记着 monitor 那套 jsonl watcher 的两条设计选择：Windows 路径小写归一（今天住后端 `platform/paths.rs` 的 `path_key`），
与「行先于 pidfile 落地 ⇒ 会话出现时强制重扫一次」那条兜底通道。本机内容改走本机后端的 `line` 帧之后两条都随它删了：
后端宣告会话时先把游标 prime 到当前行数、历史走旁路快照（与远端同一套），那个竞态不在了。

### `config.rs::atomic_replace` 用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
`std::fs::rename` 在 Windows 上 dst 存在时失败（POSIX rename atomic overwrite 行为在 Windows 上没有）。MoveFileExW 是 Windows 原生原子替换 API，专门设计来实现"覆盖现有文件"语义。

### `profile_installer::atomic_write_string`〔散文墓碑〕 用 `ReplaceFileW` 而非 `MoveFileExW`
〔RW1 · 第四波 2026-09-24〕**那个原语已删**：用户裁「只允许后端的文件管理部分写文件」也管本机 ⇒ `$PROFILE` / rc / 项目 `.mcp.json` 改经后端写（`user_files` → `files-put`）。下面这条「替换要保住 dst 的 explicit ACE」的性质跟着搬到了后端（`control/files_write.rs` 的 `swap_in`：Windows 上已在的目标就地覆盖写）。以下是原文：
`MoveFileExW(tmp, dst)` 用 tmp 的 ACL 覆盖 dst → 用户 explicit ACE 丢失（Documents 重定向到非默认盘的用户读不了自己的 profile）。**ReplaceFileW 专门设计来保留 dst 的 ACL/ADS/创建时间**。这是 Windows 文档明确推荐用于"替换配置文件"的 API。详 [doc/INVARIANTS § 4](../`src/doc/INVARIANTS.md`#4-profile-等用户文件写入--replacefilew--backup--写后校验)。

### `history::resume_impl` 用 `powershell.exe -NoExit -EncodedCommand`（v2.8.1 修复）
旧版用 `cmd /K "claude --resume <sid>"`，有两个 bug：(1) cmd.exe 不是 PowerShell、**更不加载用户 profile** → `cc` wrapper / `__ccm_bind` / 代理 env 全不生效，跑的是裸 `claude`；(2) 退出 claude 后那个壳是 cmd，不认 `cc`。旧注释还把 `pwsh.exe`（PS7，需装）和 `powershell.exe`（PS5.1，系统自带）混为一谈才退回 cmd。

改用系统自带 `powershell.exe -NoExit -EncodedCommand <base64>`：**不带 `-NoProfile`** → 加载 profile → 代理 / `cc` 生效；命令体 `if (Get-Command cc) { cc --resume <sid> } else { claude --resume <sid> }`（装了 wrapper 走 `cc`，没装回退 `claude`，回退也在加载了 profile 的真 PowerShell 里）；`-NoExit` 让 claude 退出后窗口保留且 `cc` 可继续用。命令经 `utils::powershell_encoded_command` 编码（UTF-16LE base64）透过 wt.exe / cmd 多层 shell（绕开引号 / `;` 分隔符），并对 `session_id` 做注入校验（仅 `[A-Za-z0-9_-]`，抽成可测试的 `build_resume_ps_command`）。

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

详细 cookbook 见 [src/`src/doc/CONTRIBUTING.md` § 2](../`src/doc/CONTRIBUTING.md`#2-添加新东西-cookbook)。速查：

| 需求 | 入口文件 |
|---|---|
| 新 jsonl 记录类型 | `messages.rs:JsonlRecord` enum 加 variant |
| 新 IPC 命令 | 新建模块 `<feature>.rs` → 在 `lib.rs::run().invoke_handler![]` 注册 |
| 新事件 | `bridge.rs::events` 加常量 + payload 结构 |
| 新跨进程协议文件 | 见 [src/`src/doc/IPC-PROTOCOL.md` § 添加新的跨进程协议文件](../`src/doc/IPC-PROTOCOL.md`#添加新的跨进程协议文件) |
| 新 Win32 调用 | `Cargo.toml::[target.cfg(windows)].dependencies.windows.features` 加 feature；用 `#[cfg(windows)]` 包裹 |
| 改 release 打包配置 | `tauri.conf.json::bundle`；详 [src/`src/doc/BUILDING.md`](../`src/doc/BUILDING.md`) |
