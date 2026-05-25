# 架构总览

新贡献者第一站。读完应该能回答：数据从哪儿来、经过谁、停在哪儿、为什么这么分。

每个模块的"当下设计 + 为什么"详见各子目录 README — [`../src/README.md`](../src/README.md)、[`../src-tauri/README.md`](../src-tauri/README.md)、[`../scripts/README.md`](../scripts/README.md)。

跨切面文档：
- 全局不变量 → [INVARIANTS.md](INVARIANTS.md)
- 跨进程文件 IPC 协议详解 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- Tauri State 注册矩阵 → [STATE-MATRIX.md](STATE-MATRIX.md)
- 贡献者操作手册 + cookbook → [CONTRIBUTING.md](CONTRIBUTING.md)

---

## 1. 数据流（实时通道）

```
   ┌──────────────────────┐                    ┌──────────────────────┐
   │  Claude Code CLI     │                    │  PowerShell session  │
   │  (你跑 `claude`)     │                    │  (跑 cc / __ccm_bind)│
   └──────────┬───────────┘                    └──────────┬───────────┘
              │ 写                                         │ 改窗口标题 + 写
              ▼                                            ▼
   ~/.claude/projects/                              ~/.claude/claudecode-frontend/
       <encoded-cwd>/<sid>.jsonl                        ps-await/<PID>.json
   ~/.claude/sessions/<PID>.json                        ps-registry/<PID>.json
              │                                            │
              │ notify-debouncer 监听              EnumWindows 找 marker
              │                                            │
              ▼                                            ▼
   ┌───────────────────────────────────────────────────────────────────┐
   │                          Rust 后端 (Tauri)                         │
   │                                                                    │
   │   watcher.rs  ────► parser.rs ────► messages::JsonlRecord         │
   │       │                                       │                    │
   │       ▼ active filter                         ▼                    │
   │   session_map.rs (PID 探活)            event_replay.rs (内存 buf)  │
   │       │                                       │                    │
   │       │                                       │ emit("jsonl-line") │
   │       │                                       │ / "jsonl-batch"    │
   │       ▼                                       ▼                    │
   │   bind.rs (ps-await/ps-registry/SidHwndCache, EnumWindows)         │
   │       │                                       │                    │
   │       │  invoke("bring_terminal_to_front")    │                    │
   │       └──────────────────┬────────────────────┘                    │
   │                          ▼                                          │
   │                     Tauri IPC                                       │
   └──────────────────────────┬──────────────────────────────────────────┘
                              │
                              ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │                  TypeScript 前端 (WebView2)                       │
   │                                                                    │
   │   events.ts (订阅 + 批量调度) ─► tabs.ts (TabManager)              │
   │                                       │                            │
   │                                       ▼                            │
   │                              stream.ts (MessageStream)             │
   │                                       │                            │
   │                                       ▼                            │
   │     render.ts (marked + KaTeX + hljs + DOMPurify) ─► DOM           │
   └──────────────────────────────────────────────────────────────────┘
```

**关键路径**：

- **实时增量**：jsonl 新行 → `notify-debouncer-mini` (100ms 合并) → `watcher.rs` 增量读 → `parser::parse_line` → `event_replay.record` → `emit("jsonl-line")` → 前端 `events.ts` 批量调度 → `tabs.ts.onLine` → `render.ts`（BranchFolder 在 **live 模式**，每条 record 增量算 mainBranch）
- **启动重放**：F5 / 冷启动后，前端 `emit("frontend-ready")` → `event_replay.replay_and_mark_ready` 持锁 → 单次 `emit("jsonl-batch", Vec<...>)` 整个 history → 前端 events.ts 用 **`batch-start` / `batch-end` 哨兵**包裹整批 push 进 queue → TabManager.onBatchStart 把所有 Tab 的 BranchFolder 切 **batch 模式**（recordAdded 只 push 不算）→ drain 完最后一条 → onBatchEnd 调 flushPending **一次性**算主线 + rebuild → 切回 live。避免 O(N²) 重放成本（v2.2 优化 ~15-20×）
- **cc 集成绑定**：PS 跑 `__ccm_bind` 写 `ps-await/<PID>.json` + 改窗口标题为 marker → `bind.rs` 监听 + EnumWindows → 写 `ps-registry/<PID>.json` + 删 await → PS 检测到删除恢复标题
- **历史浏览（流式）**：v2.2 起，点 Ctrl+H → `list_history_projects`（async + spawn_blocking，不阻塞 IPC）→ 用户展开项目 → 前端创建 `Channel<HistorySessionEntry>` 传给 `stream_history_sessions_in_project` → 后端边解析 jsonl 元数据边 `on_entry.send()` → 前端 onmessage rAF 节流增量插入到 fork 树。点单 session → `Channel<Vec<JsonlLinePayload>>` + `stream_read_session_jsonl` 100 行一 chunk emit → session-viewer 边收边 `renderMessage`，几百毫秒看到首屏

---

## 2. 设计分层

```
src-tauri/src/
├── 入口层      lib.rs        Tauri builder + setup + invoke_handler 注册
├── 边界层      paths.rs      claudeDir 三级解析
│              bridge.rs     IPC 事件常量 + payload schema（前后端契约的单一来源）
├── 读取层      watcher.rs    notify_debouncer 监听 projects + active filter
│              parser.rs     单行 JSONL → JsonlRecord（剥 BOM）
│              messages.rs   JsonlRecord enum + ApiMessage schema
│              session_map.rs  读 sessions/<PID>.json + Win32 进程探活
│              subagent.rs   按需加载 subagents/*.meta.json
├── 业务层      event_replay.rs  内存 buffer + 持锁 batch emit
│              history.rs    两级懒加载 + metadata + 物理删除 + resume
├── 集成层      bind.rs       cc 集成绑定核心（ps-await/registry/SidHwndCache）
│              profile_installer.rs  PowerShell profile 块插入/卸载
│              auto_launch.rs  auto-launch monitor 开关
└── 持久层      config.rs     monitor config.json R/W（Windows MoveFileExW 原子）
```

```
src/
├── 入口        main.ts       快捷键、HMR full reload
├── 事件        events.ts     订阅 + 批量调度让出主线程
├── 状态        tabs.ts       TabManager 状态机 + Tab Bar 增量 DOM 更新
│              stream.ts     MessageStream（ResizeObserver 贴底滚动）
├── 渲染        render.ts     marked + KaTeX + hljs + DOMPurify
│              cards/        折叠卡组件（slash / compact / subagent / tool）
├── 视图        views/history.ts  历史浏览器
│              views/session-viewer.ts  只读会话查看器
├── 设置        settings/panel.ts   总控
│              settings/cc_integration.ts  PowerShell 集成区
│              settings/info-icon.ts  portal tooltip 组件
├── 配置        config.ts     invoke load/save_config
│              paths.ts      claudeDir 字段读写
│              theme.ts      CSS token 应用
└── 样式        styles.css    全部样式 + token 系统
```

---

## 3. Tauri State 注册矩阵

`src-tauri/src/lib.rs::run().setup()` 注册 4 个 Arc-shared State：

| State 类型 | 持有者 | 喂给的 IPC 命令 |
|---|---|---|
| `Arc<SessionMap>` | setup 闭包 + `session-changes-emitter` 线程 + active-filter 闭包 + `app.manage` | `list_history_projects` / `list_history_sessions_in_project` |
| `Arc<EventReplay>` | setup 闭包 + frontend-ready listener + jsonl async pump + `app.manage` | `forget_session` |
| `Arc<BindRegistry>` | setup 闭包 + `bind-await-watcher` 线程 + `bind-heartbeat` 线程 + `session-changes-emitter` 线程 + `app.manage` | `cc_integration_status` |
| `Arc<SidHwndCache>` | setup 闭包 + `session-changes-emitter` 线程 + `app.manage` | `bring_terminal_to_front` |
| `Arc<LoggingState>` (v2.0.0+) | `run()` 局部（持有 WorkerGuard） + setup 闭包（install_error_emitter 注入 closure） + `app.manage` | `get_diagnostics_config` / `set_diagnostics_config` / `get_log_file_info` / `open_log_file` / `open_log_dir` |

详细 consumer 矩阵 + 修改规则 → [STATE-MATRIX.md](STATE-MATRIX.md)。

**约束**：撤回任何 State 必须先 grep 所有 `State<'_, Arc<X>>` 消费者；漏 `manage` 不会被 `cargo check` 抓住，运行时 panic。

---

## 4. 跨进程文件 IPC 简表

monitor 与外部进程的所有通信都在 `~/.claude/claudecode-frontend/` 下：

| 路径 | 写入方 | 读取方 | 用途 | 生命周期 |
|---|---|---|---|---|
| `config.json` | monitor 设置面板 | monitor 启动 | 主题 / 字体 / claudeDir override / diagnostics | 持久 |
| `ps-await/<PID>.json` | PowerShell (`__ccm_bind`) | monitor (`bind::BindRegistry`) | PS 通知 monitor "去找标题 = marker 的窗口" | 短暂 (800ms 超时) |
| `ps-registry/<PID>.json` | monitor | PowerShell (查 + 比较 procStart) | monitor 通知 PS "绑定成功，HWND = X" | 与 PS 进程同寿 |
| `sid-hwnd-cache.json` | monitor | monitor 启动恢复 | sid → hwnd 持久缓存，新 session 出现时查这里复用绑定 | 持久 |
| `auto-launch.json` | monitor 设置面板 + 启动时回写 | PowerShell (`__ccm_bind` 头部) | "用 cc 启动 claude 时自动开 monitor" 开关 + monitor exe 路径 | 持久 |
| `history-metadata.json` | monitor 历史浏览器 | monitor 历史浏览器 | star / 重命名 / 隐藏 | 持久 |
| `logs/monitor.YYYY-MM-DD.log` (v2.0.0+) | monitor (tracing-appender) | 用户（设置面板 [打开 log] / 编辑器） | GUI app 诊断日志，按天滚动保留 3 天 | 持久（自动清理老文件） |

每个文件的字段定义、编码约束（UTF-8 无 BOM）、写入方原子性语义、握手时序图 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)。

---

## 5. 关键设计选择 + 理由

每条都是踩过坑总结出来的"为什么不能用别的方案"。

### 零侵入 = 不写 Claude Code 数据源
watcher / session_map 只读 `~/.claude/projects/` 和 `~/.claude/sessions/`。唯一写入是用户**显式**触发：历史浏览器 `delete_history_session`（双校验路径白名单）+ PowerShell profile [安装]（只动 BEGIN/END **块内**内容，块外用户其他代码完全不动）。

**为什么**：cc-monitor 是个监控渲染器，写 jsonl 会破坏用户对"数据源 = 我自己的命令痕迹"的认知；profile 写入则是必要的可选副作用（用户显式 opt-in 装 `__ccm_bind`），仍然走完整的 backup + ACL 保留路径。

### event_replay 持锁完整 emit 保证顺序
record 期间 watcher 必须排队等锁，前端绝不会先收到 live emit 再收到 snapshot。

**为什么不**用"锁外 emit snapshot + 锁内 push 后 ready 判断 live emit"：emit 期间 record 能并发拿锁，看到 ready=true 走 live emit → 前端先收到新 record 的 live emit、再收到 snapshot 的旧 emit → **顺序错乱、时间线断裂**。持锁完整发是顺序保证的硬性要求。代价是 replay 期间 watcher 阻塞数十毫秒到秒级（取决于 history 大小），可接受。

### JSONL_BATCH 单次 emit 替代 N 次 JSONL_LINE
replay 时一次性发整个 Vec<JsonlLinePayload>，前端 push 进同一 queue 走原批量调度。

**为什么**：Tauri IPC 每次 emit 都有序列化 + 派发 overhead。N=3000 时累计 ~400ms 主线程阻塞，启动可见显著卡顿。BATCH 单次序列化降到 ~50ms。

### session 探活双重校验（PID + procStart）
`OpenProcess(QUERY_LIMITED) + GetExitCodeProcess == STILL_ACTIVE` + `GetProcessTimes` creation FILETIME 与 .NET DateTime.Ticks 100ms 容差比对。

**为什么不**只查 PID：Windows PID 短期复用非常常见，仅靠 STILL_ACTIVE 会把僵尸条目误判为活跃（旧 PID 被一个无关进程占用）。procStart 二次校验是必须的。

### HWND 拉前三重校验
`IsWindow(hwnd)` + 当前 `owner_pid == 绑定时 owner_pid` + 当前 owner 的 `procStart == 绑定时 owner_proc_start`。

**为什么**：HWND 复用比 PID 复用还高频（Windows 重用窗口句柄）；如果不校验 owner 的话，会把不相关的窗口拉前。任一失败拒拉前 + 给 toast 原因。

### profile 写入用 ReplaceFileW 不用 MoveFileExW
`ReplaceFileW(dst, tmp, NULL, REPLACEFILE_WRITE_THROUGH, ...)` 原子替换 dst 的**内容**，**保留 dst 的 ACL / ADS / 创建时间**。

**为什么不**用 MoveFileExW：MoveFileExW 用 src（即 tmp 文件）的 ACL 覆盖 dst 的 ACL。如果用户把 Documents 重定向到非默认盘（`E:\<user>\Documents`），那一层目录 ACL 通常只给 Administrators + Everyone 部分权限，没给当前用户 explicit Full Control。原 profile 上的 explicit ACE 被 tmp 的"父目录继承 ACL"覆盖 → 用户自己读不了自己的 profile。ReplaceFileW 专门设计来保留 dst metadata。

### profile 写入必先 backup + 写后校验
写之前 `std::fs::copy(path, <path>.ccm-backup-<ms>)` 备份；写之后 `read_to_string` + 比对长度，不匹配从 backup 回滚。

**为什么**：OneDrive online-only placeholder / 杀软介入等罕见场景下，`read_to_string` 可能返回 `Ok("")` 即"磁盘有内容但读到空"，纯写就是把用户内容冲掉。backup + 校验是双保险。

### marker 握手 + EnumWindows 找窗口（cc 集成）
PS 写 `ps-await/<PID>.json` + 改 `$Host.UI.RawUI.WindowTitle = marker` → monitor `EnumWindows` 找 `GetWindowTextW.contains(marker)` 的窗口拿 HWND。

**为什么不**直接用 `EnumWindows + GetWindowThreadProcessId`：PowerShell 进程**不直接拥有终端窗口**（Windows Terminal 是单独进程；cmd 走 conhost；VSCode 走 integrated terminal）。window owner 不等于 PS owner。用 PS 改自己窗口标题为 unique marker + 反查 title 是唯一可靠的跨进程握手。

### UTF-8 BOM 双向防御
PS 端模板 `cc.ps1.tpl` 用 `[System.IO.File]::WriteAllText(... UTF8Encoding($false))` 显式写无 BOM；Rust 端 `bind::process_await_file` 用 `raw.trim_start_matches('\u{feff}')` 剥任何 BOM 再 `serde_json::from_str`。

**为什么**：PS 5.1 `Out-File -Encoding utf8` 默认**写 UTF-8 BOM**（前 3 字节 `EF BB BF`），`serde_json` 不剥 BOM 直接解析失败。源头修 + 接收端兜底双保险，避免用户用旧模板还能 work。

### CSS portal tooltip 真挂 document.body
`?` 图标 tooltip 不挂自己子节点，而是 `document.body.appendChild(tip)` + `position: fixed` + JS 算 viewport 坐标。

**为什么**：父 `.settings-panel` 有 `transform: translateX(0)`（slide-in 动画）。CSS spec 规定：祖先有 `transform` 时，`position: fixed` 后代的 containing block 从 viewport **重置到那个祖先** → `left/top` 不再是 viewport 坐标 → tooltip 实际跑出屏幕。挂 body 脱离 panel 子树即可恢复真 fixed 行为。

### logging 子系统：tracing init 在 Builder 之前 + ErrorEmitterLayer + reload Handle（v2.0.0）
`logging::init(monitor_data_dir)` 必须在 `tauri::Builder::default()` **之前**调用（tracing 全局 dispatcher 一旦 init 不能再换）。内部组装 `registry().with(reload<EnvFilter>).with(stdout).with(file).with(ErrorEmitter).init()`。

- **file layer 用 `tracing-appender::rolling::daily` + `non_blocking` writer**：按天滚动 + 不阻塞业务线程。WorkerGuard 必须挂在 LoggingState 上（drop 时 flush）
- **reload::Layer<EnvFilter>**：`set_diagnostics_config` 能改日志级别**不重启就生效**
- **ErrorEmitterLayer**：自定义 Layer 拦 `Level::ERROR` → 通过注入的 emit closure 发 `monitor-error` 事件 → 前端弹红色 toast。limited 60s/20 条避免风暴
- **AppHandle 通过 closure 注入**：tracing init 时 AppHandle 还没建（在 setup 里才有）→ ErrorEmitter 内部用 `RwLock<Option<closure>>`，setup 里调 `install_error_emitter(handle)` 把 emit closure 写进去
- **失败兜底**：log 目录创建失败 / appender 构造失败 → 退化到 stdout-only，monitor 仍能启动（INVARIANT § 15）

**为什么**：v1.7.0-1.7.7 的 BOM 真凶就是因为 `windows_subsystem = "windows"` 无 stderr，`tracing::warn!("bind: parse ... failed")` 没人看见，cc 集成"装上没用"7 个版本无人察觉。issue #4 就是补这个结构性短板。

### Win32 sync 调用走 spawn_blocking
`bring_terminal_to_front` / `cc_integration_*` 都走 `tokio::task::spawn_blocking`。

**为什么**：Win32 同步调用（`EnumWindows` / `SetForegroundWindow` / `ShellExecuteW` 等）可能阻塞数十 ms 到秒级；放到 Tauri 主 runtime 会卡死 IPC 派发。spawn_blocking 隔离到 blocking thread pool，前端再加 5s timeout 兜底。

---

## 6. 关于"不在主线 / 已废弃"特性

设计中**主动放弃**的方向，写在这里给后续重构者参考避免重蹈：

### 焦点同步（已删）
原 `SetWinEventHook` 监听 `EVENT_SYSTEM_FOREGROUND` 然后切对应 Tab 的设计。

**为什么放弃**：Windows Terminal 单进程多窗口/多 tab 架构，`GetForegroundWindow` 只能拿到 WT 主进程的 HWND，**无法区分同一 WT 窗口内哪个 tab active**。SidHwndCache 里 N 个 tab → 同一 HWND 的映射也反查不出。已彻底删除 `lookup_by_foreground_pid` 和 `FOCUS_SWITCH` IPC。

### subagent 实时流（已隔离）
不走主 watcher，由前端 `invoke("load_subagent")` 在用户展开 Task 折叠卡时按需加载。

**为什么**：subagent jsonl 数量大但展开率低，主 watcher 全 emit 会膨胀 event_replay buffer 数倍但绝大多数没人看。隔离到 on-demand IPC 是 ergonomic + memory 的双赢。

### 4-tier 启发式拉终端（已撤回）
v1.6.x 试过的"从 claude PID 走 parent chain + WT 进程 + 终端类进程 + ai-title 匹配"4 层 fallback。

**为什么放弃**：explorer 启 PowerShell + WT DefTerm 接管 console 的常见架构下，claude 祖先链与 WT 窗口完全脱节（claude 的 parent 是 PS，PS 的 parent 是 explorer；WT 是另一个独立进程，跟 claude/PS 没有 parent 关系）。4 层启发式在主流环境下都不可靠。改走 cc 命令注入式绑定（v1.7+），让 PS 主动告诉 monitor "我的 HWND 是 X"。

---

## 7. 入门读图

- 想理解整体数据流：本文 § 1 + 5
- 想加新 jsonl 类型：见 [CONTRIBUTING.md](CONTRIBUTING.md) § 添加 jsonl 类型
- 想改/加跨进程协议文件：见 [IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- 想加新 IPC 命令：见 [CONTRIBUTING.md](CONTRIBUTING.md) § 添加 IPC + [STATE-MATRIX.md](STATE-MATRIX.md)
- 想改某个具体模块：找对应子目录 README（`src/` 或 `src-tauri/`）的模块表
