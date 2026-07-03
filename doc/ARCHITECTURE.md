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
   │   watcher.rs ─batch handler─► parser.rs ────► messages::JsonlRecord│
   │       │ (同步同线程调用)                       │                   │
   │       ▼ active filter                          ▼                   │
   │   session_map.rs (PID 探活)        event_replay::on_line_batch     │
   │       │                                       │  按大小分流（每行带 │
   │       │                                       │  watcher 给的 seq）│
   │       │                                       │  小→jsonl-line     │
   │       │                                       │  大→jsonl-batch    │
   │       ▼                                       ▼   (切块 chunk_i/N) │
   │   bind.rs (ps-await/ps-registry/SidHwndCache, EnumWindows)         │
   │       │                                       │                    │
   │       │  invoke("bring_terminal_to_front")    │                    │
   │       │  invoke("bring_monitor_to_front")     │                    │
   │       └──────────────────┬────────────────────┘                    │
   │                          ▼                                          │
   │                     Tauri IPC                                       │
   └──────────────────────────┬──────────────────────────────────────────┘
                              │
                              ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │                  TypeScript 前端 (WebView2)                       │
   │                                                                    │
   │   events.ts (订阅 + 批量调度 + onBatchStart/End 哨兵)               │
   │       │                                                            │
   │       ▼                                                            │
   │   tabs.ts (TabManager: switchTo manual/auto, userActive, override) │
   │       │                                                            │
   │       ▼                                                            │
   │   render-stream-record.ts ⭐ v2.6 三 caller 共享渲染管线 +          │
   │       │                  tool-group 后处理合并（看 timeline 左邻居）│
   │       ▼                                                            │
   │   record-timeline.ts ⭐ v2.6 按 seq binary insert + DOM insertBefore│
   │       │                                                            │
   │       ▼                                                            │
   │   stream.ts (MessageStream: insertNode + 守卫式 snap 贴底,           │
   │             重放期上方插入靠 overflow-anchor + 延后批量挂载)        │
   │       │                                                            │
   │       ▼                                                            │
   │   render.ts (marked + KaTeX + hljs + DOMPurify; opts.lazy 参数)    │
   │       │                                                            │
   │       └─► DOM                                                      │
   └──────────────────────────────────────────────────────────────────┘
```

**关键路径**：

- **watcher batch 同步 + seq 分配**（v2.4 重构 + v2.6 加 seq）：`spawn_watcher` 接 `on_batch: BatchHandler` 回调；一次 `process_file` 把读到的所有行收集成 `Vec<JsonlLine>` **同步**调 `on_batch`，**没有 mpsc 中间层、没有 async drain task**。lib.rs 里 closure 在 watcher 线程内 parse + `replay.on_line_batch(handle, payloads)`。v2.6 加 `seqs: HashMap<PathBuf, u64>` 给每行分配 per-file 单调 seq → `JsonlLinePayload.seq` 透传到前端，**前端 RecordTimeline 按 seq 排序，后端 emit 顺序不再影响视觉**。
- **大小分流 emit**（v2.4.2，v2.6 chunked emit 简化，Batch5-F17 async 化）：`event_replay::on_line_batch` 按 batch 大小分流：
  - `payloads.len() < 50`（用户日常敲键 1~N 行）→ 逐条 `emit("jsonl-line")` live 路径
  - `payloads.len() >= 50`（`claude --resume` 灌历史 / 远端 snapshot 攒批 / 大量追加）→ 切块走 `emit("jsonl-batch")`，前端进入 batch 模式（lazy hljs）。v2.6 简化：删 head/older 区分，统一按 `CHUNK_SIZE=600` 末块先发，前端按 seq 自动排到正确位置。**Batch5-F17**：大批块序列 spawn 到 async_runtime（块间 tokio sleep）——spawn 返回≠emit 完成，顺序敏感调用方（ssh_source 攒批 flush，行须先于断连归档）用 `on_line_batch_awaited`（INVARIANTS § 10）；前端 events.ts 另有突发检测兜底（jsonl-line 积压 >50 主动进 batch 模式）
- **启动序**（v2.4 修首次启动乱序；Batch5-F18/F19 骨架+优先级）：前端 DOMContentLoaded 后先 invoke `list_active_sessions` 把本地活跃会话**骨架 tab** 全部建出（远端骨架走 `remote-session-added` 事件），再按 localStorage 记忆选 active（上次所在 tab），然后 `emit("frontend-ready", {prioritySid})`。后端 listener 在 async task 里 spin-wait（10ms poll，10s timeout）等 watcher 同步全量扫置位的 `initial_scan_done` → `replay_and_mark_ready(priority_sid)` **按 session 分组**切块 emit（prioritySid 的块先发、组内末块先发、chunk 全局连续编号保 batch-start 哨兵）→ mark ready → **按活跃集对账补发 `session-ended`**（#19 本地用 session_map / #20 远端用 remote_active；前端把 session-ended 与行同队列同序处理，归档落在全部重放行之后，INVARIANTS § 24）。v2.6 简化：**删了 `replaying` flag + catch-up tail 路径**，chunked emit 期间 watcher 真新行直接走 jsonl-line live emit，前端 timeline 按 seq 自动放到正确位置
- **前端按 seq 排序**（v2.6 B 重构）：`RecordTimeline.insert(seq, element)` 用 binary search 找位置 → `stream.insertNode(element, anchor)` 同步处理 stickToBottom 贴底。**消除了** PayloadSource batch/live / inPrependMode / pendingPrependFragment 等 5 个 flag。tool-group 合并改后处理算法：插入时 `timeline.peekPrev(seq)` 看左邻居，是 tool-group 就 `addToToolGroup`，否则建新 group 入 timeline（详 render-stream-record.ts）
- **active session 自动同步**（v2.4 issue #2）：tabs.ts `onLine` 透传 payload 给 `renderStreamRecord`；sink.onRealUserInput 仅在 `result.kind === "card" && message.type === "user"` 触发（v2.6 删 source 参数后用 message.type 判定）→ TabManager.userActive 检查 `autoFollowUserActive` toggle + 5s `manualOverrideUntil` → `switchTo(sid, "auto")` + 可选 `invoke("bring_monitor_to_front")`
- **cc 集成绑定**：PS 跑 `__ccm_bind` 写 `ps-await/<PID>.json` + 改窗口标题为 marker → `bind.rs` 监听 + EnumWindows → 写 `ps-registry/<PID>.json` + 删 await → PS 检测到删除恢复标题
- **历史浏览（流式）**：v2.2 起，点 Ctrl+H → `list_history_projects`（async + spawn_blocking，不阻塞 IPC）→ 用户展开项目 → 前端创建 `Channel<HistorySessionEntry>` 传给 `stream_history_sessions_in_project` → 后端边解析 jsonl 元数据边 `on_entry.send()` → 前端 onmessage rAF 节流增量插入到 fork 树。点单 session → `Channel<Vec<JsonlLinePayload>>` + `stream_read_session_jsonl` 100 行一 chunk emit → session-viewer 边收边 `renderMessage`，几百毫秒看到首屏
- **Task 面板（v2.3 issue #11）**：`tasks.rs::spawn_task_watcher` 用 notify-debouncer-mini 监听 `<claude_dir>/tasks/` 递归 → 文件变更（CLI 跑 `TaskCreate` / `TaskUpdate` / `TaskStop`）→ debounce 100ms + 按 sid dedup → 重读整个 `tasks/<sid>/` 目录（跳过 `.lock` / `.highwatermark` / 非数字命名）→ emit `task-update` 携完整 task 列表。前端 `tasks-panel.ts` 按 sid 路由到对应 Tab 的 sticky 折叠卡。Tab 创建时另调 `get_session_tasks` IPC 拿初始快照（async + spawn_blocking）。0 task 时 panel 隐藏

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
│              tasks.rs      v2.3 CLI task tracker 读 + watcher + emit task-update
├── 集成层      bind.rs       cc 集成绑定核心（ps-await/registry/SidHwndCache）
│              profile_installer.rs  PowerShell profile 块插入/卸载
│              auto_launch.rs  auto-launch monitor 开关
├── 远端层      ssh_source.rs  russh 远端数据源（连接/鉴权/流帧解析 + 版本协商 + 测试连接 + Batch5-F17 Line 帧攒批 Batcher——snapshot 聚批走 chunked 路径）
│              remote_history.rs  远端历史浏览 + 全文搜索查询（一次性 exec daemon 子命令；多台 join_all 并发 fan-out，墙钟 Σ→max）
│              (Batch8) ssh_source 内快照拉取：SnapshotQueue+dispatcher+fetch——
│              tail-only 下每会话独立连接 --read-session 旁路拉历史（行号 seq 缝合）
│              sftp.rs       SS-D SFTP 写层（daemon 自动部署 + 手动安装/卸载 + 远端删除 + ccm 安装/卸载）
└── 持久层      config.rs     monitor config.json R/W（Windows MoveFileExW 原子）
```

```
src/
├── 入口        main.ts       快捷键、HMR full reload、behavior 初始化
├── 事件        events.ts     订阅 + 批量调度 + onBatchStart/End 哨兵（v2.6 删 source/onChunk）
│              remote-health.ts  订阅 remote-health 事件 + 按 origin 节流弹 toast（overflow/version）
├── 状态        tabs.ts       TabManager 状态机 + switchTo manual/auto + userActive
│              stream.ts     MessageStream（insertNode + 守卫式 snap；重放消抖 § 5）
├── 渲染        render.ts     marked + KaTeX + hljs + DOMPurify（v2.6 opts.lazy 参数）
│              cards/        折叠卡组件（slash / bash / diff / api-error / interactive / compact / subagent / tool）
│                            cards/index.ts::stripInternalNoise 剥 CLI 注入 + ESC 中断
│ ⭐ v2.6 新模块  record-timeline.ts  按 seq binary insert + DOM 挂载，消除 inPrependMode
│              render-stream-record.ts  三 caller 共享渲染管线 + tool-group 后处理合并
│              local-storage.ts  LS_KEYS 集中 + safeGet/safeSet
│              format.ts  formatTimestampShort/Smart + formatBytes 合并
├── 视图        views/history.ts  历史浏览器（v2.6 fixed overlay 不替换 streamRoot）
│              views/session-viewer.ts  只读会话查看器
│              tasks-panel.ts  v2.3 Tab stream 顶部 sticky task 折叠卡
├── 设置        settings/panel.ts   总控 + onBehaviorChange 回调
│              settings/cc_integration.ts  PowerShell 集成区
│              settings/info-icon.ts  portal tooltip 组件
│              settings/data-section.ts  v2.3 数据存储透明化
├── 配置        config.ts     invoke load/save_config
│              paths.ts      claudeDir 字段读写
│              theme.ts      CSS token 应用
│              behavior.ts   v2.4 autoFollowUserActive / bringMonitorToFront toggles
└── 样式        styles.css    全部样式 + token 系统
```

---

## 3. Tauri State 注册矩阵

`src-tauri/src/lib.rs::run().setup()` 注册 7 个 Arc-shared State：

| State 类型 | 持有者 | 喂给的 IPC 命令 |
|---|---|---|
| `Arc<SessionMap>` | setup 闭包 + `session-changes-emitter` 线程 + active-filter 闭包 + `app.manage` | `list_history_projects` / `stream_history_sessions_in_project` |
| `Arc<EventReplay>` | setup 闭包 + frontend-ready listener + jsonl async pump + `app.manage` | `forget_session` |
| `Arc<BindRegistry>` | setup 闭包 + `bind-await-watcher` 线程 + `bind-heartbeat` 线程 + `session-changes-emitter` 线程 + `app.manage` | `cc_integration_status` |
| `Arc<SidHwndCache>` | setup 闭包 + `session-changes-emitter` 线程 + `app.manage` | `bring_terminal_to_front` |
| `Arc<LoggingState>` (v2.0.0+) | `run()` 局部（持有 WorkerGuard） + setup 闭包（install_error_emitter 注入 closure） + `app.manage` | `get_diagnostics_config` / `set_diagnostics_config` / `get_log_file_info` / `open_log_file` / `open_log_dir` |
| `Arc<SearchIndex>` (issue #6) | setup 闭包 + `search-index-build` 后台线程（`build_blocking`） + `app.manage` | `search_history` / `get_search_index_status` / `rebuild_search_index` |
| `Arc<RemoteHwndCache>` (issue #18) | setup 闭包 + `remote-session-emitter` 线程（每 sid scan 子线程 `try_bind` / removed 时 `forget`） + `app.manage` | `bring_remote_terminal_to_front` |

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

**只读外部数据源**（不属于 monitor 写入域，但 monitor 读取并展示）：

| 路径 | 写入方 | 读取方 | 用途 |
|---|---|---|---|
| `<claude_dir>/projects/<encoded-cwd>/<sid>.jsonl` | Claude Code CLI | monitor `watcher.rs` / `history.rs` | session 消息流，monitor 实时增量 + 历史浏览 |
| `<claude_dir>/sessions/<PID>.json` | Claude Code CLI | monitor `session_map.rs` | 活跃 session 探活（PID + procStart 双校验；procStart 缺失时自动降级仅 STILL_ACTIVE，详 INVARIANTS § 18） |
| `<claude_dir>/tasks/<sid>/<id>.json` (v2.3) | Claude Code CLI (`TaskCreate`/`TaskUpdate`/`TaskStop` 工具) | monitor `tasks.rs` | Tab task 面板数据源；附 `.lock` / `.highwatermark` 控制文件需忽略 |

每个文件的字段定义、编码约束（UTF-8 无 BOM）、写入方原子性语义、握手时序图 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)。

---

## 5. 关键设计选择 + 理由

每条都是踩过坑总结出来的"为什么不能用别的方案"。

### 零侵入 = 不写 Claude Code 数据源
watcher / session_map 只读 `~/.claude/projects/` 和 `~/.claude/sessions/`。唯一写入是用户**显式**触发：历史浏览器 `delete_history_session`（Batch4-F15 起 exists → 双边 canonicalize → canonical 前缀 + `.jsonl` 扩展名四段守卫，`..`/symlink 穿越拒绝）+ PowerShell profile [安装]（只动 BEGIN/END **块内**内容，块外用户其他代码完全不动）。

**为什么**：cc-monitor 是个监控渲染器，写 jsonl 会破坏用户对"数据源 = 我自己的命令痕迹"的认知；profile 写入则是必要的可选副作用（用户显式 opt-in 装 `__ccm_bind`），仍然走完整的 backup + ACL 保留路径。

### event_replay 顺序保证 = 前端按 seq 排序（v2.6 起；本节曾描述已废弃的"持锁完整 emit"）
v2.6 B 重构前顺序靠"持锁完整 emit"（record 排队等锁）；**现行设计**：`replay_and_mark_ready` 持锁只做 snapshot + 置 ready，emit 全在锁外——顺序保证整体转移给 per-file 单调 seq + 前端 RecordTimeline 二分插入（ADR-021/022）。emit 期间并发到达的 live 行先于 snapshot 旧行到达也无碍：前端按 seq 排到正确位置。

**为什么能放弃持锁 emit**：旧方案的代价是 replay 期间 watcher 阻塞数十毫秒到秒级；seq 排序把"后端保序"变"前端排序"后，emit 顺序成为纯性能自由度（Batch5-F19 的 priority 分组正是利用这一自由度）。跨通道顺序（行 vs session-ended）不由 seq 覆盖，由队列同序（INVARIANTS § 20）与 `on_line_batch_awaited`（INVARIANTS § 10）分别兜住。

### JSONL_BATCH 单次 emit 替代 N 次 JSONL_LINE
replay 时一次性发整个 Vec<JsonlLinePayload>，前端 push 进同一 queue 走原批量调度。

**为什么**：Tauri IPC 每次 emit 都有序列化 + 派发 overhead。N=3000 时累计 ~400ms 主线程阻塞，启动可见显著卡顿。BATCH 单次序列化降到 ~50ms。

### 启动重放贴底消抖 = 守卫式 snap + overflow-anchor + 延后批量挂载
重放"末块先发"，最新一段先到并贴底，更老的内容随后**插到视口上方**。三条协同保证贴底时不抖（详 INVARIANTS § 21）：

- **守卫式 `snap()`**：只在落后底部 >1px 时才写 scrollTop，不每帧重钉。
- **上方插入交给原生 `overflow-anchor`**：不手动补偿 scrollTop（叠加会 double-shift）。
- **`RecordTimeline` deferMode**：重放期插到非末尾的旧内容**只进数组不挂 DOM**，`onBatchEnd` 用 `attachBatch` 一次性挂回（在 `branchFolder.flushPending` 之前）。

**为什么**：旧内容逐条插到贴底视口上方会让浏览器逐帧重排 + 重做 scroll anchoring，HiDPI/高刷屏分数像素下整数 `scrollHeight` 与分数布局的舍入误差每帧不同 → 整块内容 ±0.5px 高频抖动（实测抖动帧 66→1）。渲染仍按 40/帧推进（响应不卡），只把"上方插入"压成一帧。注意 `scrollTop` 本身不震荡，故只测 scrollTop 发现不了此 bug。

### 独立只读窗口复用主渲染管线 + 定向 replay（issue #10）
`open_session_in_new_window` 建 `viewer-<sid>` WebviewWindow 加载 `index.html?viewer=<sid>`；前端 `main.ts` 检测参数走精简 bootstrap（`bootstrapViewer`）—— **复用 TabManager**（过滤到该 sid、`body.viewer-mode` 隐藏 tab/设置/历史 chrome），自动继承分支折叠 / 启动滚动消抖 / tool-group 合并。顶部一条 slim 栏：项目名标题 + ↗调出终端 + 📂打开 cwd（复用 TabManager 的 active-tab 动作）。

**为什么不另写 viewer 渲染器**：再写一套渲染会与主管线漂移（SessionViewer 漏 pendingToolResults 是历史教训）。复用 TabManager 零功能差。

**历史 + 实时一致性**：独立窗口订阅 `jsonl-line`（按 sid 过滤）拿实时增量；历史经 `replay_session_to_window` 从 event_replay buffer **定向 emit 给本窗口**——两者都是 watcher 的 **per-file seq 空间**，混进同一 RecordTimeline 顺序天然正确，重叠由前端 `seen` set 去重。**不发 frontend-ready**（那会触发对所有窗口的全量 replay）。仅活跃 session 在 buffer；archived 走前端一次性文件读。capability 必须含 `viewer-*`（见 capabilities/default.json）。

**四个踩过的坑（INVARIANT § 22）**：
1. **开窗命令必须 `async`**：Tauri 2 同步 `fn` 命令在主线程跑，`WebviewWindowBuilder::build()` 又要派发回主线程并阻塞等 → 自死锁（白屏 + 整窗卡死）。async 命令在 runtime 线程跑才行。
2. **事件 target-kind 必须对齐**：定向投递要 Rust `emit_to(EventTarget::webview_window(label))` ↔ 前端 `getCurrentWebviewWindow().listen`（窗口作用域，`bindEvents({windowScoped:true})`）。用 `&str` 目标（→`AnyLabel`）配模块级 `listen`（→`Any`）**收不到**（实测白屏只剩状态栏）。live 广播 `Any` 是通配，带标签监听仍能收。
3. **`bindEvents` 须 await**：`listen()` 异步注册，注册完成前 emit 的事件会丢；viewer 紧接着调 replay，必须先 await。
4. **viewer-mode grid 行数**：`#tab-bar` `display:none` 会把它从 grid item 移除，剩下的子元素**前移一行** → message-stream 落进多余的 0 高行被压没。只能给剩余 item 定义对应行数（`auto 1fr 24px`）。

### session 探活双重校验（PID + procStart，procStart 可缺）
`OpenProcess(QUERY_LIMITED) + GetExitCodeProcess == STILL_ACTIVE` + 当 sessions/<PID>.json 含 `procStart` 字段时再加 `GetProcessTimes` creation FILETIME 100ms 容差比对。

**为什么不**只查 PID：Windows PID 短期复用非常常见，仅靠 STILL_ACTIVE 会把僵尸条目误判为活跃（旧 PID 被一个无关进程占用）。procStart 二次校验**有的话**就必须做。

**为什么 procStart 可缺**：v2.4.2 实测 Claude Code 2.1.150 在某些启动路径下（/resume 或类似）写 `sessions/<PID>.json` 漏 procStart。之前 schema 必填导致 serde 整条解析失败 → 整个 session 被静默忽略 → monitor 漏 Tab。改 `Option<String>` 后缺失就跳过 procStart 校验仅 STILL_ACTIVE。INVARIANTS § 18 完整论证。

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

### bring_monitor_to_front 三层 hack（v2.4 issue #2）
用户在终端敲键 → monitor 不是前台进程 → `SetForegroundWindow` 直接调被 Win10/11 拒绝（OS 防恶意软件偷焦点）。修复方案是叠加三层：

1. **`keybd_event(VK_MENU)` 模拟 Alt 按键**：OS 检测后视当前进程为"刚有用户输入" → 临时获得前台资格（PowerToys / TranslucentTB 同款 trick）
2. **`AttachThreadInput`**：附加到当前前台线程的输入队列，借用其拉前权限
3. **`SetWindowPos(TOPMOST → NOTOPMOST)`**：强制 Z 序拉顶，即使 `SetForegroundWindow` 失败也至少视觉浮顶

单层（v2.4.0 只 set_focus / v2.4.1 加 AttachThreadInput）实测在 Win10 1903+ 都不够，三层叠加才稳。详 `lib.rs::bring_monitor_to_front` + CHANGELOG v2.4.2。

---

## 6. 关于"不在主线 / 已废弃"特性

设计中**主动放弃**的方向，写在这里给后续重构者参考避免重蹈：

### OS 焦点同步（已删 / v2.4 用 watcher 反推替代）
原 `SetWinEventHook` 监听 `EVENT_SYSTEM_FOREGROUND` 然后切对应 Tab 的设计。

**为什么放弃**：Windows Terminal 单进程多窗口/多 tab 架构，`GetForegroundWindow` 只能拿到 WT 主进程的 HWND，**无法区分同一 WT 窗口内哪个 tab active**。SidHwndCache 里 N 个 tab → 同一 HWND 的映射也反查不出。已彻底删除 `lookup_by_foreground_pid` 和 `FOCUS_SWITCH` IPC。

**v2.4 issue #2 用 watcher 反推 `type=user` 替代**：用户在 claude 里敲回车 → claude 写一行 type=user 到 jsonl → watcher 识别 → 切对应 Tab。零侵入、信号准（详 doc/INVARIANTS.md § 20）。OS API 路径仍废弃；公开 API（[microsoft/terminal#19818](https://github.com/microsoft/terminal/issues/19818)）2026 年 5 月仍在 Backlog。

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
