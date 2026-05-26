# Changelog

本文档记录 cc-monitor 用户**可感知**的功能 / 修复 / 行为变更。
内部重构与文档调整通常不入。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。
版本遵循 [SemVer](https://semver.org/)。

---

## [2.4.0] — 2026-05-26

### 新功能 — 终端 active session 自动同步到 monitor Tab（issue #2）

你在多个 PowerShell tab 里跑多个 claude session，切到某个 tab 在 claude 里敲回车 → monitor 自动切到对应的 Tab，不用再手动到 monitor 窗口点。

**信号源：watcher 反推 `type=user`**

- 不依赖 OS 焦点检测（Windows Terminal 单进程多 tab，`GetForegroundWindow` 永远拿 WT 主进程 HWND 无法区分 tab —— v1 早期 `FOCUS_SWITCH` 就是因此废弃）
- 不依赖 ConPTY FOCUS_EVENT（PSReadLine 独占 console input，第三方进程抢句柄不稳）
- 不依赖 claude code hooks（违反零侵入）
- 不依赖 Windows Terminal 公开 API（[microsoft/terminal#19818](https://github.com/microsoft/terminal/issues/19818) 仍在 Backlog，"Spec Needed"，无 ETA）

watcher 反推天然零侵入：jsonl 已经在监听，用户在终端敲回车 → claude 写一行 `type=user` 到对应 jsonl → monitor 立即识别 → 切对应 Tab。

**严格分辨"真用户输入" vs "工具回灌"**

Claude Code JSONL 里 `type=user` 实际三种形态：
1. 真用户敲键的文本 → ✓ 触发自动切
2. CLI 内部 prompt 包装（被 `stripInternalNoise` 剥光的）→ ✗ 不触发
3. 工具结果回灌（`content: [{type: "tool_result", ...}]`，Anthropic API schema 把工具返回挂在 user role 上）→ ✗ 不触发

判别复用前端 `cards/index.ts::renderMessage` 既有逻辑——`result.kind === "card"` 已经过滤了 noise + tool_result。**无后端新事件**，复用既有 `jsonl-line` 路径在 `tabs.onLine` 末尾加判断。

**Manual override：5 秒不抢回**

你手动点 monitor 的 Tab Bar / Ctrl+Tab 切到别的 Tab 后，**5 秒内**任何 user-active 信号都不会抢回。给"我现在主动看另一个 tab"的意图留缓冲。5 秒后才恢复自动跟随。

**两个独立 toggle**

设置面板新增「行为」分组：

- **「用户在终端里输入时自动切到对应 Tab」**（默认开）
- **「自动切 Tab 时同时把 monitor 窗口拉到前台」**（默认关）—— 默认不抢焦避免打断浏览器/IDE 工作；想让 monitor 主动浮上来时勾上

第二个 toggle 仅当第一个开启时生效（前者关时灰显）。

### 新增 IPC
- `bring_monitor_to_front` — `unminimize + show + set_focus` 主窗口，复用 single-instance plugin 回调同款逻辑。受 `AllowSetForegroundWindow` 限制，某些场景 OS 仅闪任务栏不真拉前（Windows 设计）。

### 新增 config 字段
- `autoFollowUserActive: bool`（默认 true）
- `bringMonitorToFrontOnUserActive: bool`（默认 false）

均与 `theme` / `claudeDir` / `diagnostics` 字段平级。**运行时热更**（不像 claudeDir 需要重启）。

---

## [2.3.1] — 2026-05-26

### 修复 — 首次启动消息乱序（必须按 F5 才正常）

**症状**：dev mode / 安装后首次启动 monitor，stream 里消息顺序错乱；用户必须按 F5 刷新一次才显示正确顺序。

**根因双重**：

1. **后端 watcher 异步初始扫与 frontend-ready 竞态**：v2.3 架构下，`spawn_watcher` 把全量扫扔进独立线程异步执行，setup() 立刻返回。同时另起一个 `tauri::async_runtime::spawn(while rx.recv())` async task 慢慢从 mpsc channel 把行 drain 给 `EventReplay::record`。前端 emit("frontend-ready") 时（~T+450ms）watcher 还没扫完 + async task 还没 drain 完 → `replay_and_mark_ready` 持锁 snapshot 看到的 history 不完整 → 部分历史漏到 ready=true 后的 live emit 路径，跟 chunked replay 的 chunks 错位到达前端。

2. **前端 inPrependMode 误捕获 live emit**：`tabs.appendCardOrBuffer` 检查全局 `inPrependMode`，对**所有** payload 一视同仁。chunked replay 进入 prepend 模式（chunk index > 0）时，任何 `jsonl-line` live emit（不管是漏出来的旧历史还是用户实时敲的新行）都被错误丢进 `pendingPrependFragment`，最终被推到 stream 顶部。

F5 不出 bug：刷新时 backend 已稳定几秒，history 完整，replay 一次成型，无 live emit 干扰。

**修复**：

后端：
- `watcher.rs::spawn_watcher` 改接 `on_line: LineHandler` 闭包，**干掉 mpsc 中间层**。watcher 线程内同步调 record()，history buffer 在 watcher 线程内同步落盘。
- `WatcherHandle` 增加 `initial_scan_done: Arc<AtomicBool>`，watcher 同步全量扫完才置 true 然后进 debouncer 监听阶段。
- `lib.rs` frontend-ready listener 在 async task 里 spin-wait `initial_scan_done`（10ms poll，10s timeout 兜底），就绪才调 replay。保证 snapshot 时 history 完整。

前端：
- `events.ts` 新增 `PayloadSource = "batch" | "live"` 类型。`jsonl-batch` 拆出的 payload 标 `source: "batch"`，`jsonl-line` 标 `source: "live"`。
- `tabs.ts::appendCardOrBuffer(tab, element, source)`：source==="batch" 且 inPrependMode 时才走 prepend fragment buffer，**source==="live" 永远 stream.append 贴底**。

**用户体验**：首次启动跟 F5 后行为完全一致，无需手动刷新。加载期间用户在终端敲的新消息仍然实时贴到 stream 底部（绕开切块 prepend 逻辑）。

### 内部
- 删除 `tauri::async_runtime::spawn(while rx.recv())` async drain task。
- watcher 模块的 `mpsc::UnboundedReceiver` import 移除。

---

## [2.3.0] — 2026-05-25

里程碑：启动加速 10× 量级 + 三个 feat 同发 + tool-result UI 全面重做。

### 改进 — 启动加载速度 10× 提速（issue #1）

之前 v2.2 启动重放 ~3920 条 record 前端 drain + 渲染管线耗时 **22s** 才完全可交互。

本版本通过 **历史切块 + DOM prepend + lazy 代码高亮** 三层联防：

#### 历史切块 emit

后端 `event_replay::replay_and_mark_ready` 按 history 数量切块：

- N < 200 → 单次 emit（保持 v2.2 行为，无切块开销）
- N ≥ 200 → 按 session 分组取每 session 最新 N 条到 head 块，剩余按 600 条切 mid 块
- **chunk 0** (head 最新) → 前端 append 到 stream 底部 → 用户**立刻可见最新消息**
- **chunk 1..N** (older) → 前端 prepend 到 stream 顶部 → 后台默默 prepend 老内容
- 块之间释放锁停顿 10ms，让 watcher 真新消息能 live emit 并行插入

最终 DOM 顺序：`[最老 ... 次新 ... head 最新]` 时间升序保持。

#### Lazy 代码高亮 (highlight.js)

batch 重放期间 markdown 渲染走 lazy 路径：marked + DOMPurify + KaTeX 同步出 HTML，但 hljs **不跑** —— 留 `<div class="code-block code-pending">` 占位。

全局 IntersectionObserver（`rootMargin: 300px`）观察卡片进可视区时调 `enhanceCard` 补跑 hljs。

每条 record 渲染管线 5.6ms → 1.5ms（砍 ~70%）。KaTeX 保留同步（耗时占比小 + 拆开复杂度高收益小）。

#### 实测结果

| 阶段 | v2.2 | v2.3.0 |
|---|---|---|
| 首屏 head 可见 | ~22s | **~600ms** ⚡ |
| 全部 drain 完毕 | ~22s | ~1.7s |
| 用户可交互 | ~22s | **~600ms** |

详 [v2.3 启动加速学习笔记](https://github.com/bo0Zeng/cc-monitor/blob/main/CHANGELOG.md#230---2026-05-25)。

### 新增 — Tab Task 面板（issue #11）

Claude Code CLI 终端底部的 task tracker（`TaskCreate` / `TaskUpdate` / `TaskStop` 工具维护）现在能直接在 monitor 看到，再不用切回终端确认任务进度。

- 每个 Tab 的消息流顶部多一个 **sticky 折叠卡**：「N tasks (X done, Y in progress, Z open)」摘要 + 展开看完整列表（subject + 状态 icon），跟终端视觉对齐
  - `□ pending` / `■ in_progress` / `✓ completed` / `✗ deleted`，未知值兜底 `•`
  - 已完成的任务删除线 + 60% opacity，进行中的高亮一点背景色
  - `description` / `activeForm` 进 hover tooltip（点行不展开，省视觉空间）
- **默认折叠**；折叠状态写 `localStorage cc-monitor.tasks-panel.collapsed` **全局**持久（所有 Tab 同步偏好，重启 monitor 保留）
- **0 task 时 panel 完全隐藏**（display:none），不占视觉空间
- **实时同步**：CLI 跑 `TaskCreate` / `TaskUpdate` → ~100ms 内 monitor 对应 Tab 更新

### 实现

后端新增 `src-tauri/src/tasks.rs`：
- `read_session_tasks(tasks_root, sid)` 扫 `<claude_dir>/tasks/<sid>/<id>.json`
  - 跳过 `.lock` / `.highwatermark` / 非 `<digits>.json` 命名
  - 半截 JSON 单条 catch 跳过（写者持锁中途读到 → 下次 debounce 自然修正），不会冻死整次重读
  - 按 id 数字升序排序
- `spawn_task_watcher(tasks_root, app)` 用 `notify-debouncer-mini` 监听 `tasks/` 递归
  - 100ms debounce → 同批次按 sid `HashSet` dedup → 每 sid 重读整目录 → emit `task-update`（**完整重发**，无 diff）
  - `tasks_root` 不存在时静默不 spawn（用户从没用过 task tracker 的兼容态）
- `get_session_tasks` IPC（`async fn` + `spawn_blocking`）给 Tab 创建时拿初次快照
- 新事件 `task-update` + `TasksUpdatePayload { sessionId, tasks }`
- 9 个单元测试（empty / skip .lock / sort by numeric id / partial JSON tolerance / camelCase serde 契约 / session_id 路径反推 etc.）

前端新增 `src/tasks-panel.ts`：
- `TasksPanel` 组件（sticky 面板 + 摘要 header + 列表 body）
- 全 panel 实例共享 `LS_KEY = cc-monitor.tasks-panel.collapsed`，一个 Tab 折叠所有同步
- 完整 replace 渲染（task ≤ ~30 条无 diff 必要）
- `tabs.ts` 在 ensureTab 时挂 panel 到 stream 顶部 + 异步 `fetchSessionTasks`；`updateTasks(sid, tasks)` 路由 `task-update`；closeTab 时 dispose
- `events.ts` 加 `onTasksUpdate` 句柄；`task-update` 直接同步派发不进批量队列（稀疏事件）

### 新增 — 数据存储透明化（issue #3 A 阶段）

设置面板加「**数据存储**」折叠分组，列出 monitor 所有持久化数据位置 + WebView2 用户数据 + localStorage keys，每项配 [打开] 按钮直接进资源管理器查看。**纯展示，不动数据**。

- monitor 持久化目录（`~/.claude/claudecode-frontend/`）的所有文件：`config.json` / `sid-hwnd-cache.json` / `auto-launch.json` / `history-metadata.json` / `ps-await/` / `ps-registry/` / `logs/`
- WebView2 用户数据目录（`%LOCALAPPDATA%\<bundle>\EBWebView\`）— cache / localStorage / IndexedDB / cookies
- PowerShell profile 备份（v1.7.10+ 自动备份的 `.ccm-backup-<时间戳>` 文件，仅在装过 cc 集成时显示）
- 前端 localStorage 所有 `cc-monitor.*` keys + value（折叠 / 渲染模式 / profile 选项 / task panel 状态等）
- 卸载说明：NSIS 默认不清这些数据，想彻底清除手动删

后端新模块 `src-tauri/src/data_paths.rs`（4 个单元测试）+ IPC `get_data_paths`（async + spawn_blocking，stat 不递归算大小避免大目录卡 IPC）。前端 `src/settings/data-section.ts` 渲染分类卡片。

### 新增 — Tool result 渲染模式切换 + 长 output 性能修复

Tool 调用结果展开后顶部新加 [文本 | Markdown] 切换 toolbar：

- **Markdown 模式**复用 `renderMarkdown`（marked + DOMPurify + KaTeX + hljs），含 LaTeX / 代码高亮
- **行号前缀启发式 strip**：Read / Grep 等工具输出带 `<n>\t<content>` 或 `<n>:<content>` 行号前缀，MD 模式渲染前自动 strip 让 `#` 标题等结构暴露
- **per-tool-name 偏好持久** `localStorage cc-monitor.tool-render.<toolName>`；`Read` / `Grep` / `WebFetch` / `NotebookRead` / `TodoWrite` 默认 MD，其他默认 text

性能改进：

- `.block-body` 加 `content-visibility: auto` + `contain: layout style paint` + `contain-intrinsic-size`，浏览器跳过 viewport 外的 layout/paint，长 output 滚动不再卡
- 大 output (>200 KB) 默认只渲染前 800 行 + `[显示完整内容 (N KB)]` 按钮，避免一次性 marked 解析卡死主线程

### 单元测试

后端 44 → 57（+9 tasks tests +4 data_paths tests）。

---

## [2.2.0] — 2026-05-25

里程碑：历史浏览器从「全量同步加载、UI 死锁等几秒」升级到「流式 + 不阻塞 + fork 树形」。重放路径同步打通 batch 模式，启动 monitor 速度数量级提升。

### 新增 — 历史 fork 树形组织（issue #12）

- Claude Code CLI 的 `/branch` 命令分叉出新 session 时，jsonl 顶层有 `forkedFrom: { sessionId, messageUuid }` 字段。本版本在历史浏览器把 fork 关系展现成 **child 缩进显示在 parent 下** 的树。
- 项目内独立树（跨项目 fork 不连接 —— child 当 root 显示「↳ 原 session 不见了」marker）。
- 默认折叠；点 ▶ 展开看 children；折叠/展开状态本地持久 (`localStorage cc-monitor.history.expanded-forks`)，重启 monitor 保留。
- 后端 `messages.rs::JsonlRecord` 的 User / Assistant 加 `forked_from: Option<ForkedFrom>` 字段；`history.rs::HistorySessionEntry` 加 `forkedFromSessionId` / `forkedFromMessageUuid`。

### 新增 — 历史浏览器流式加载（issue #12）

- **session 列表流式**：用 Tauri 2 [Channel API](https://v2.tauri.app/develop/calling-rust/#channel)，后端 `stream_history_sessions_in_project` 边解析边发，前端 onmessage 增量插入到 fork 树。大项目（几十个 session）首条 < 100ms 出现，不再等齐。
- **session 内容流式**：`stream_read_session_jsonl` 按 100 行一 chunk emit，前端边收边 `renderMessage`。10MB 大文件几百毫秒看到首屏。
- **取消**：用户中途关闭历史视图 / 切走 → JS Channel 被 GC → 后端下次 `send()` 返 Err → 自动 break 出读取循环，不浪费 IO。
- 进度提示：「加载中 · 已 N 条…」/「继续加载中…」实时更新；完成后切到「N 条记录 · 只读历史视图」（修了 channel resolve vs onmessage 的竞态）。

### 改进 — 历史 IPC 异步化

历史浏览器全部 IPC（`list_history_projects` / `list_history_sessions_in_project` / `read_session_jsonl`）改 `async fn` + `tokio::task::spawn_blocking` 包同步 IO。**加载期间其他 IPC（拉前 / 切设置 / 切 Tab 等）能正常响应**，不再整个 UI 死锁。

### 改进 — 启动重放速度大幅提升（同 issue #12 路径）

之前启动 monitor 重放历史 jsonl 时，前端 `tabs.ts::onLine` 对**每一条** record 都调 BranchFolder.recordAdded → computeMainBranch（O(N)）→ 可能 DOM rebuild。2000 条 record = O(N²) ≈ 4M ops + 数十次重排 + events.ts 每 40 条让出主线程 1 帧（50 帧起步）→ 总耗时几秒。

本版本引入 **batch 模式**：

- `BranchFolder` 加 `setBatchMode(bool)` + `flushPending()`：batch 模式下 `recordAdded` 只 push 不算，flushPending 一次性 compute + rebuild。
- `events.ts` 的 `jsonl-batch` 事件包裹 `batch-start` ... payloads ... `batch-end` 哨兵；drain 按 kind 派发。
- `TabManager.onBatchStart` / `onBatchEnd` 在重放期把所有 Tab 的 BranchFolder 切 batch，结束时 flush + 切回 live。重放期新建的 Tab 也自动进 batch 模式。
- **结果**：2000 条重放从 ~3-5s 降到 ~200ms（**15-20× 加速**），跟历史只读视图同量级。真实时新消息照旧 per-record 走 live 模式不变。

## [2.1.1] — 2026-05-25

### 修复

- **长 session 启动只渲染头几条消息 + console RangeError**（v2.1.0 起的回归）：
  - 症状：~1000+ 条记录的 session 打开 monitor 后，前端只显示前几条消息，再就停了；F12 console 看到 `RangeError: Maximum call stack size exceeded`。
  - 根因：v2.1.0 issue #8 的 `computeMainBranch` 用真递归 (`dfsLatest` + `walkMain`) 算主线。Claude session 的 parent 链典型几乎线性，递归深度 = 链长度。WebView2 (Chromium) 默认 JS stack 在 ~1000 frames 附近触底 → BranchFolder.recordAdded 抛 RangeError → events.ts 的 drain 异常逃逸 → 后续 record 永久滞留 queue 不渲染。
  - 修法：
    1. `src/branching.ts`：两个 DFS 都改迭代。`latestDescTs` 用 Kahn 拓扑序自底向上累加 O(N) 无递归；`walkMain` 本来就是 tail-recursive，改 `while` 循环深度 1 帧。
    2. `src/events.ts`：drain 加 try/catch 包单条 `onLine` —— 防御未来类似的单条记录处理异常冻死整个 replay queue（详 [`doc/INVARIANTS.md § 17`](doc/INVARIANTS.md)）。

## [2.1.0] — 2026-05-25

### 新增

- **ESC 回退分支折叠**（issue #8）：Claude Code CLI 双击 ESC 回退到之前某条 user 重新发送，jsonl 里产生 `parentUuid` 分叉。本版本识别这种分叉并把"被回退"的连续消息段折叠到「已被 ESC 回退（含 N 条消息）」可展开容器里，主线显示一气呵成。
  - **算法**："只在 fork 点选 latest-descendant 赢家"。单链 / 多 root（含 /compact 切断的多树）/ 无分叉的会话**完全不折叠**；只有真正的 ESC 回退（同 parent 多个 child）才把被抛弃的兄弟子树折叠。
  - **链完整性**：jsonl 链里 attachment 和 system 记录夹在 user→assistant 之间（实测占 8% parent 指向）。后端把 attachment 也 emit 给前端（虽然不渲染卡片），系统级别保证 parent 链不断 → 主线判定正确。
  - **工具组**：tool-group 卡（连续 tool-only assistant）也写 data-uuid（取首条贡献者），跟 user/assistant 一起参与折叠 → 被回退段不会因 tool-group 被切成碎片。
  - 历史浏览器（Ctrl+H 进只读视图）同样支持。
  - 折叠/展开状态本地持有，刷新（F5）不丢。
  - 详 [`src/branching.ts`](src/branching.ts)、[`src/branch-fold.ts`](src/branch-fold.ts)。

- **single-instance lock**（issue #9）：同一个用户同一台机器只允许一个 cc-monitor 进程。第二次双击 `cc-monitor.exe`（或装多份 exe 双击别处那份）→ 第二个实例立即退出，第一个窗口被 unminimize + show + set_focus 拉到前台。修复历史上"两个 monitor 同时跑导致双重渲染 + cc 集成 race"的混乱。底层走 Tauri 官方 [`tauri-plugin-single-instance`](https://v2.tauri.app/plugin/single-instance/)，user-scoped mutex，跨用户登录不冲突。详 [`doc/INVARIANTS.md § 16`](doc/INVARIANTS.md)。

### 改进

- **设置面板折叠分组**（issue #7）：低频 section（外观 = 字体 + 颜色 13 字段；诊断）默认折叠到可展开 ▶ 分组里。设置面板纵向缩短超过 1 屏，找 PowerShell 集成不用再滚很远。展开/折叠状态本地持久（localStorage `cc-monitor.settings.collapsed.<id>`），重启 monitor 保留。展开动画用 grid-template-rows 0fr↔1fr 技巧，200ms 平滑过渡。高频 section（数据 / PowerShell 集成）保持默认展开。

---

## [2.0.0] — 2026-05-25

里程碑：补齐 GUI app 诊断短板。v1.7 系列结尾的 BOM bug "7 个版本带病发布无人察觉" 暴露的结构性问题（`windows_subsystem = "windows"` 无 stderr → tracing 输出不可见）在本版本彻底解决。

### 新增 — 诊断 / log 文件可视化（issue #4）

**背景**：cc-monitor 用 `windows_subsystem = "windows"` 编译，**没有 stderr 控制台**。所有 `tracing::warn!` / `tracing::error!` 用户和开发者都看不到。v1.7.0-1.7.7 的 BOM 解析失败（`bind: parse ... failed`）一直在打 warn，但 7 个版本带病发布无人察觉，cc 集成 "装上没用" —— 用户和开发者都没有任何反馈渠道。

本版本补齐这个结构性短板：

#### 滚动 log 文件
- 写到 `~/.claude/claudecode-frontend/logs/monitor.YYYY-MM-DD.log`
- 按天滚动，默认保留最近 3 天
- `tracing-appender` `non_blocking` writer，不阻塞业务线程
- log 文件失败时自动 fallback 到 stdout-only —— **monitor 启动绝不会被 log 卡住**

#### 设置面板「诊断」区
- ☑ 启用 log 文件（默认开；切换需要重启 monitor）
- 日志级别 [info ▼]（trace / debug / info / warn / error / off；**切换立即生效，无需重启**）
- ☑ 后端 ERROR 时显示右下角 toast（默认开；立即生效）
- log 文件路径 + 当前大小显示
- [打开 log 文件] / [打开 log 目录] / [刷新信息] 三个按钮

#### 后端 ERROR 红色 toast
- 任何 `tracing::error!` → 右下角红色 toast（headline = tracing target 如 `bind`，body = 完整 message）
- 多条 ERROR 垂直堆叠（不互相覆盖）
- 6 秒自动消失；**点击 toast 直接打开 log 文件**
- 限频 60 秒 / 20 条，避免错误风暴时屏幕被刷满

#### Config schema 扩展
`~/.claude/claudecode-frontend/config.json` 顶层新增 `diagnostics`：
```json
{
  "diagnostics": {
    "log_enabled": true,
    "log_level": "info",
    "error_toast": true,
    "max_files": 3
  }
}
```
所有字段 `#[serde(default)]` —— 老 v1.7.x 用户 config.json 无 diagnostics 字段时自动 fallback 到默认值，**完全向后兼容**。

### 新增 IPC

| 命令 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `get_diagnostics_config` | — | `DiagnosticsConfig` | 设置面板拉当前配置 |
| `set_diagnostics_config` | `{ cfg }` | `RestartHint` | 写新配置；返回是否需要重启 |
| `get_log_file_info` | — | `LogFileInfo` | dir / current_file / size / all_files |
| `open_log_file` | — | `()` | 用系统默认编辑器打开当前 log |
| `open_log_dir` | — | `()` | 用资源管理器打开 log 目录 |

### 新增前端 / 后端模块
- `src-tauri/src/logging.rs` —— tracing init + 滚动 appender + EnvFilter reload + ErrorEmitterLayer + DiagnosticsConfig R/W（含 8 个单元测试）
- `src/error-toast.ts` —— listen `monitor-error` 弹堆叠 toast
- `src/settings/diagnostics-section.ts` —— 设置面板「诊断」区

### CSS
- 新增通用 `.ccm-toast-stack` / `.ccm-toast` / `.ccm-toast-error` 类（落实 INVARIANT § 12）
- 旧的 `#bring-terminal-toast` 保留作向后兼容；后续可重构复用 `.ccm-toast`

### 项目管理
- 新依赖：`tracing-appender = "0.2"`
- 单元测试 36 → 44（+8 logging tests）
- `tracing-subscriber` 现 init 走 `logging::init()` 而非 lib.rs 直接 `fmt().init()`

### 不破坏现有行为
- 不勾选诊断任何选项 → 行为跟 v1.7.13 一样（log 仍写但用户感知不到）
- 关掉 log 文件 → 老 log 文件保留，新内容不写
- 关掉 error toast → ERROR 仍写 log 文件，只是不弹 toast

---

## [1.7.13] — 2026-05-24

### 修复 — 设置面板 `?` tooltip 完全看不到

v1.7.12 改 right-anchored 后，靠左的 `?`（如"PowerShell 集成"标题旁）hover 时 tooltip 向左溢出 panel 左边界被裁。换 `position: fixed` + JS 算 viewport 坐标后**仍然看不到** — DevTools 显示 inline style 完全正确（`display: block; left: 735.946px; top: 161.223px; visibility: visible`），但 `getBoundingClientRect()` 实际给的 left 是 1476.5（viewport 外）。

**根因**：`.settings-panel` 有 `transform: translateX(0)` 做 slide-in 动画。CSS spec 规定：**祖先有 transform 时，position: fixed 后代的 containing block 从 viewport 重置到那个祖先**。我设的 `left: 735.946px` 不再相对 viewport，是相对 panel —— 视觉上跑到屏幕外。

**修法**：tooltip DOM 改成挂 `document.body`（不是 `?` icon 的子节点），脱离 .settings-panel 的 transform 子树，`position: fixed` 才真相对 viewport。把 makeInfoIcon + swapFileName 也拆到独立模块 `src/settings/info-icon.ts`，未来其他设置区可复用。

### 改进 — 启动 batch emit

`event_replay::replay_and_mark_ready` 之前对 history 里每条 jsonl 单独 `emit(JSONL_LINE, p)`，N=3000 时累计 ~400ms Tauri IPC 序列化 + 派发 overhead，阻塞主线程导致 F5/冷启动后白屏 + tabs 突然涌出的延迟感。

加 `JSONL_BATCH` 事件，replay 时单次 `emit(JSONL_BATCH, Vec<JsonlLinePayload>)` —— 一次序列化整个 Vec，前端 listener 拿到 array 后 push 进原批量 drain queue。实测启动到可交互省 200-400ms。`record()`（实时单条）走 JSONL_LINE 不变。

### 项目管理

- **删未用依赖**：`Cargo.toml` 的 `anyhow` + `thiserror` 全仓 grep 0 引用，纯死依赖。删了减编译时间 + 包体积。
- **`opener:allow-open-path` scope** 维持 `**`：考虑过收紧到 `$DOCUMENT/WindowsPowerShell/**` 但会破坏"Custom 路径"功能（用户可能选 Documents 外的位置）。
- 文档更新：发版后另起一次文档大重整（doc/CONTRIBUTING.md / ARCHITECTURE.md / IPC-PROTOCOL.md / INVARIANTS.md / STATE-MATRIX.md / DEVELOPMENT.md / BUILDING.md / RELEASING.md 等），覆盖测试列表 + 关键设计理由 + 跨进程协议 schema + 全局不变量。

## [1.7.12] — 2026-05-24

### 改动 — 设置面板 / PowerShell 集成 UX 修复 + 概念修正

#### Tooltip 溢出修复
- `?` 图标 hover tooltip 之前 `left: 50% + translateX(-50%)` 居中 + 320px 宽，靠右的 `?` 会让 tooltip 右半部分超出 360px 宽的设置面板被 `overflow-y: auto` 裁切。改成 right-anchored（`right: -4px`），宽度收到 240px max，永远向左展开不出 panel 右边界。

#### Legacy 文案中性化（修正概念错误）
- v1.7.2 起设置面板检测到 `profile.ps1` 有 cc-monitor 块时会弹"⚠ v1.7.0-1.7.1 旧位置遗留，PowerShell 启动时不读，实际无效"。**这个文案是错的**：`profile.ps1` = CurrentUserAllHosts，PowerShell 启动**会读它**（所有 host 都读）。
- 改成中性文案"ℹ 在 profile.ps1 (AllHosts) 也检测到 cc-monitor 块"，把判断权给用户：故意装那的话保留，重复安装的话清理一份。

#### Profile 路径下拉新增 AllHosts 选项
- 之前下拉只有 `PS 5.1 / PS 7.x / Custom`，默认指向 `Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost，只有 powershell.exe 控制台读）。
- 新下拉 5 项：
  - `PowerShell 5.1 - $PROFILE（默认）` → Microsoft.PowerShell_profile.ps1
  - `PowerShell 5.1 - 所有 host（profile.ps1）` ⭐ 推荐：VSCode 终端 / ISE / SSH 都生效
  - `PowerShell 7.x - $PROFILE`
  - `PowerShell 7.x - 所有 host`
  - `自定义路径...`
- 旁边 `?` tooltip 解释 AllHosts vs CurrentHost 的实际差别。

#### 路径选择持久化
- 之前用户手动改 Profile 路径，关闭面板下次打开就被默认 PS 5.1 - $PROFILE 覆盖。
- 现在用 `localStorage` 持久化用户选的下拉项 + 自定义路径，下次打开恢复。

#### 备份机制说明
- [安装] 按钮加 hover 提示："v1.7.10+ 写入前自动备份原 profile 到 `<profile>.ccm-backup-<时间戳>`，写入失败自动回滚，用 Win32 ReplaceFileW 保留原 ACL"。

## [1.7.11] — 2026-05-24

### 修复 — [打开 profile] 按钮无效

设置面板 PowerShell 集成区的 [打开 profile] 按钮点了无效，alert 报：
```
打开失败: opener.open_path not allowed. Permissions associated with this command: opener:allow-open-path
```

**根因**：`src-tauri/capabilities/default.json` 里只有 `opener:default`，而它**不含** `allow-open-path`（实测 `gen/schemas/acl-manifests.json` 中 default permission set 是 `["allow-open-url", "allow-reveal-item-in-dir", "allow-default-urls"]`）。Tauri runtime 在 invoke `plugin:opener|open_path` 时直接拒。

**进一步坑**：单独加 `"opener:allow-open-path"` 仍不工作——`allow-open-path` 的 description 写明 "Enables the open_path command **without any pre-configured scope**"，默认 scope 为空 = 没有任何路径被允许打开。

**修复**：capability 用 inline scoped permission entry：

```json
{
  "identifier": "opener:allow-open-path",
  "allow": [{ "path": "**" }]
}
```

Tauri dev 模式实测：第一版改动后 alert 文本完全相同（permission denied），加 scope 后 [打开 profile] 直接用默认编辑器打开 .ps1（notepad / VSCode 等）。

## [1.7.10] — 2026-05-24 🚨 **紧急修复**

### 修复 — 严重事故：profile_installer 可能写坏用户 profile

v1.7.9 及更早版本在用户**已有内容的 PowerShell profile** 上点 [安装] 时存在两个事故路径，可能导致 profile 变 0 字节 / 普通用户读不了。**症状**：PowerShell 启动卡在 `Access to the path 'X' is denied` 报错，用户的别名/函数等全部失效。

### 两个根因

1. **非原子写**：`atomic_write_string` 走 `write(tmp) → remove(path) → rename(tmp, path)` 三步——如果 rename 因为 OneDrive 同步占用、杀软介入等失败，**原文件已被 remove** → profile 永久丢失。

2. **ACL 被覆盖**：即使 rename 成功，**tmp 文件 ACL（继承父目录）会替换掉 dst 上原有的 explicit ACE**。如果用户把 Documents 重定向到非默认盘（如 `E:\<user>\Documents`），父目录 ACL 通常只给 Administrators + Everyone 部分权限，没有当前用户的 explicit ACE——atomic replace 后用户自己都读不了自己的 profile。这是 v1.7.0–1.7.9 在某些机器上"装上后 PS 启动全报 Access denied"的真凶。

### 修复

1. **`atomic_write_string` 改用 Win32 `ReplaceFileW`** —— 这个 API 专门做"原子替换内容但**保留 dst 的 ACL / ADS / 创建时间**"。MoveFileExW 不保留 ACL，所以 v1.7.10 早期尝试用 MoveFileExW 修也不够。dst 不存在时 fallback 到 rename（首次安装，没东西可保留）。
2. **写之前必做 backup**：把原 profile 复制到 `<path>.ccm-backup-<ms>`，写入失败自动从 backup 恢复。备份文件保留给用户做最后手段。
3. **写之后回读校验长度**：不匹配从 backup 回滚并报错。
4. **`path.exists() == true` 但读到 `""` 时直接 abort**：不再用空字符串覆盖磁盘上有内容的文件（OneDrive placeholder / 文件锁等罕见场景）。
5. **`uninstall_from_profile` 加同样保护**：backup + 校验 + 回滚。
6. **新增 5 个端到端测试**：包括 `install_preserves_existing_user_content` 验证用户原内容不丢、`install_preserves_explicit_acl_entries`（Windows-only）验证 explicit ACE 被 ReplaceFileW 保留、`reinstall_replaces_block_keeps_user_content` 验证重装幂等。

### 受影响用户的应急步骤

如果你在 v1.7.0–1.7.9 装过 cc 集成后 PowerShell 启动报 `Access to the path … is denied`：

**情况 A — profile 完全无法读（普通用户和管理员都报错）**：
1. 用**文件资源管理器**（不要用 PowerShell）打开 profile 所在目录
2. 把 `Microsoft.PowerShell_profile.ps1` 和 `profile.ps1` 改名加 `.broken-bak` 后缀
3. 重启 PowerShell，错误消失；用 cmd `type` 看 `.broken-bak` 内容，抢救你自己的脚本

**情况 B — 管理员能读、普通用户报 access denied**（ACL bug）：
1. 用**管理员 PowerShell** 跑：`icacls "你的 profile 路径" /grant "$env:USERDOMAIN\$env:USERNAME:(F)"`
2. 这一条给你自己加一个 explicit Full Control ACE，普通 PS 立即能读 profile
3. 然后装 v1.7.10 再 [安装] cc 集成，新 ReplaceFileW 不会再吃 ACL

## [1.7.9] — 2026-05-24

### 改动 — 设置面板 / PowerShell 集成 UI 清理

- **"Wrapper 命令名"输入框移除**，命令名固定 `cc`：避免用户填错（最坑：填 `claude` → PowerShell function 跟 `claude.exe` 同名导致无限递归）。需要其他名字的用户直接编辑 profile。
- **新增 "同时安装 cc wrapper" 复选框**，**默认不勾选**：默认只装 `__ccm_bind` helper，不动用户已有的命令；勾选才装 `function cc { __ccm_bind; & claude $args }`。这样新用户的 profile 不会被无意中覆盖。
- **UI 干净化**：所有冗长 hint 改成 `?` 图标 hover 显示 tooltip。说明 = 想看才看；面板不再被 5-6 段说明文字塞满。
- 状态行加 `?` 图标解释"已注册 PowerShell session" 的语义（很多人误以为 0 就是没装好）。

### 文档

- 新增 `doc/ARCHITECTURE.md`：数据流图 + State 矩阵摘要 + 跨进程文件 IPC 协议表 + 设计分层 + 历史踩坑表。新贡献者第一站。
- `README.md` 新增"PowerShell 集成（可选）"章节，写清楚装 / 不装的影响，反映 v1.7.9 默认不勾选 wrapper 的新行为。
- `README.md` 安装包名示例从 `1.5.0` 改成 `<version>` 占位，避免每次 bump 都得改 README。
- `src-tauri/README.md` IPC 清单补全 v1.7 的 7 个命令（`bring_terminal_to_front` / `cc_integration_*` / `cc_*auto_launch`）；模块表加 `bind.rs` / `profile_installer.rs` / `auto_launch.rs`；不变量节加握手协议 + UTF-8 无 BOM 约束；工程坑节补 v1.7.0–1.7.1 profile.ps1 错位 + v1.7.8 BOM。
- `scripts/README.md` 提 `src-tauri/scripts/cc.ps1.tpl` 模板的存在。

## [1.7.8] — 2026-05-24

### 修复（v1.7.0–1.7.7 一直没修对的真凶）

- **PS 5.1 `Out-File -Encoding utf8` 写 UTF-8 BOM，serde_json 解析失败** ——
  这才是 cc 集成"装上没用"的真正根因。从 v1.7.0 起所有"修了又没用"的发版本质都是这个 bug，
  之前 v1.7.5 / v1.7.7 改的 `GW_OWNER` / `GetWindowTextLengthW` 都在 EnumWindows
  那一层，但**根本走不到那里**——`process_await_file` 在 `serde_json::from_str`
  那一步就 fail 了，直接删 await + return。
  - 实测：PS 5.1 `Out-File -Encoding utf8` 输出的文件前 3 字节是 `EF BB BF`（UTF-8 BOM）。
    `serde_json::from_str` 看到非 `{` 字符开头直接 `Err`。
  - 现象完美吻合：用户跑 cc 后 ps-await 被删（解析失败也删，避免重试）但 ps-registry
    永远不生成（fn 早 return 了）。
  - **修法 A**（核心）：`bind.rs::process_await_file` 读文件后
    `raw.trim_start_matches('\u{feff}')` 喂给 serde_json。一行兜底，任何 BOM/无 BOM
    UTF-8 输入都吃下。**已装 cc 集成的用户装 v1.7.8 monitor 立即 work，不需要重装 cc**。
  - **修法 B**（源头清洁）：`cc.ps1.tpl` 改用 `[System.IO.File]::WriteAllText` +
    `UTF8Encoding($false)` 显式无 BOM 写入。新装 cc 的用户拿到正确模板。
- 这是 v1.7.x 系列的最后一根稻草。**至此 4 层 bug 全部找出**：

| 版本 | bug | 实际"装上没用"原因 |
|---|---|---|
| v1.7.0–7.4 | 不知道 cc 没 work | 不知道 |
| v1.7.5 | 以为是 `GW_OWNER` 过滤过紧 | 修了但还没用——因为根本到不了那一步 |
| v1.7.7 | 以为是 `GetWindowTextLengthW` 对 WinUI 返 0 | 修了但还没用——同上 |
| v1.7.8 | **PS Out-File 写 BOM + serde_json 不剥 BOM** | **真凶**，修了立即 work |

### 教训

`tracing::warn!("bind: parse ... failed")` 在 GUI app（windows-subsystem = "windows"）
里**用户看不到**——v1.7.0 起这个 warn 一直在打，但没人能看到。下次必须给 GUI 加
本地 log 文件或者 IPC log 命令。**已加入** `doc/CONTRIBUTING.md` § 1.5 发版前 checklist。

## [1.7.7] — 2026-05-24

### 修复（接 v1.7.5 GW_OWNER 修复后发现的第二层 bug）

- **`GetWindowTextLengthW` 对 WinUI / Microsoft.UI.Xaml.Controls 控件返回 0** ——
  Windows Terminal 用的 XAML 控件（WT 1.18+ tab 容器之类）兼容 Win32 API 时有
  quirk：`GetWindowTextLengthW` 报"长度 0"（说"无 title"），但**实际有 title**——
  直接调 `GetWindowTextW(hwnd, buf, 512)` 给固定 buffer 能拿到。
  - v1.7.5 去掉 `GW_OWNER` 过滤后 monitor 能枚举到 WT XAML 子窗口，但
    `GetWindowTextLengthW` 返回 0 → title 当空字符串 → 永远 marker_match=false →
    跳过 → ps-registry 仍不生成。
  - 用户端诊断脚本用 `StringBuilder 512` 固定 buffer 调，能拿到 title，所以诊断
    脚本能找到 marker，但 monitor 找不到。两者行为不一致就是这里。
  - 修法：`find_window_for_marker` 的 callback **不再用 `GetWindowTextLengthW`**，
    直接固定 512 buffer 调 `GetWindowTextW`。marker 长度 ≤ 50 字符，512 buffer
    肯定够。

### v1.7.x cc 集成回顾

至此 cc 集成 4 个层级 bug 全部修完：
- v1.7.2：profile 文件名错（`profile.ps1` vs `Microsoft.PowerShell_profile.ps1`）
- v1.7.3：一键安装覆盖用户已有 `function cc`
- v1.7.5：`GW_OWNER` 过滤拒绝 WT XAML 子窗口
- v1.7.7：`GetWindowTextLengthW` 对 WinUI 控件 quirk 让 title 拿不到

## [1.7.6] — 2026-05-24

### 改动

- **Wrapper 命令名默认值改回 `cc`**（v1.7.5 改空又改回来）。
  placeholder 提示 "cc / ccm / 留空只装 helper"，仍允许留空 / 改别的。
  留空 + 已有同名 cc function 时的"只装 helper"逻辑保留。
  填 `claude` 阻止（防无限递归）保留。

## [1.7.5] — 2026-05-24

### 新增

- **"打开 profile"按钮** —— 设置面板 PowerShell 集成区加按钮，调系统默认编辑器
  打开当前路径的 profile（用 `tauri-plugin-opener`）。方便用户手动编辑 profile
  加 `__ccm_bind` 调用。

### 改动（UI 默认值调整）

- **Wrapper 命令名默认留空** —— 之前默认 `cc`，但 `cc` 是用户自己常用的别名，
  cc-monitor 不该默认抢这个名字。**新默认：留空**，placeholder "留空只装 helper（推荐）"。
  - 留空：只装 `__ccm_bind` helper，**不装任何 wrapper function**。
    用户在自己的 wrapper（如自定义 `cc` / `mc` / 直接在 prompt 里）调 `__ccm_bind` 即可。
  - 填名字（如 `ccm`）：装 `function 名字 { __ccm_bind; & claude $args }`。
  - **填 `claude` 时阻止**：弹 alert 警告——PowerShell function 跟 exe 同名时
    function 优先，会**无限递归**。
- 移除 v1.7.3 加的"也装默认 function cc"复选框——逻辑改成"命令名是否非空"，
  UI 更简洁。
- 介绍文案重写：不再假设用户用 `cc` 命令，引导用户"自己有 wrapper 就在里面调 `__ccm_bind`"。

### 修复（release-blocker，v1.7.0 起的）

- **cc 命令握手成功但 ps-registry 不生成** —— monitor 处理 ps-await 文件
  但 `find_window_for_marker` 返回 None，导致绑定永远建立不起来，Tab ↗
  始终报"未绑定窗口"。
  - 根因：`bind.rs::find_window_for_marker` 的 `EnumWindows` callback 过滤
    `GetWindow(hwnd, GW_OWNER) != 0` 的窗口（只看顶层无 owner 窗口）。
    这是从 v1.6.x 4-tier 算法继承的——当时为了排除 popup/dialog。
  - 实测：用户的 PS 是从 explorer 启动的，Windows Terminal 接管 console。
    `$Host.UI.RawUI.WindowTitle = $marker` 设的 title 同步到 **WT 内的
    Microsoft.UI.Xaml.* 子窗口（owner != 0，owner = WT 主窗口）**，
    而**不是** WT 主窗口本身。monitor 因为 owner 过滤直接跳过这些窗口。
  - 影响版本：v1.7.0 / v1.7.1 / v1.7.2 / v1.7.3 / v1.7.4 全部带病——
    cc 集成实际上从来没在 WT 接管 console 的常见场景下 work 过。
    单测全过 + 终端流程跑通 + 文件 trace 正确，但**窗口找不到**，binding
    永不生成。
  - 修法：去掉 `GW_OWNER` 过滤。marker 字符串 = `ccm-bind-{PID}-{UUID 8 char}`
    极独特，不需要 owner=0 这个"防 popup 误命中"的保险。

### 诊断脚本（如本次复现）

附 `ccm-diag.ps1`（本仓库外）可在用户 PS 跑：模拟 cc 握手并对比 PS 端 vs
monitor 端 `EnumWindows` 看到的窗口差异。本 bug 就是这样定位的——PS 端能找到
marker，monitor 端找不到 → 一定是过滤条件差异。

### v1.7.x 教训

v1.7.0-1.7.4 看似都"装上能用"，实际除非用户是从 WT 内开新 tab 启动 PS
（owner=0 那种），否则握手永远失败。这次 bug 之所以拖到 v1.7.5 才发现：
1. 自动化测试全是纯函数单测，没法测真实窗口枚举
2. monitor 处理 await 后 silent drop（没写 ps-registry 也没报错日志可见）

---

## [1.7.4] — 2026-05-24

### 修复（release-blocker，v1.6.7 起的回归）

- **历史浏览器打不开**："加载失败：state not managed for field `map` on command
  `list_history_projects`. You must call `.manage()` before using this command"。
  - 根因：v1.6.7 撤 `bring_terminal_to_front` 时把 `app.manage(session_map.clone())`
    一并删了，但 `history.rs::list_history_projects` 和
    `list_history_sessions_in_project` 也接 `State<Arc<SessionMap>>`，没补回去就 dead。
  - v1.6.7 / 1.7.0 / 1.7.1 / 1.7.2 / 1.7.3 都带这个 bug——单测过（不跑 IPC dispatch），
    我也没实测过历史浏览器。
  - 修法：lib.rs setup 补 `app.manage(session_map.clone())`。

## [1.7.3] — 2026-05-23

### 修复

- **v1.7.2 一键安装会覆盖用户已有的 `function cc`** —— 模板默认包含完整
  `function cc { __ccm_bind; & claude $args }`，安装到 profile 时由于
  PowerShell **后定义同名 function 覆盖前面**的机制，用户在 profile 中已有的
  自定义 `function cc`（含 cd / 代理 / 自定义参数处理等逻辑）会被无声覆盖。
  虽然 BEGIN/END 块外的代码本身没被改，但运行时实际生效的是 cc-monitor 的版本。

### 改动

- **模板拆成 `__ccm_bind` helper + 可选 `function cc` 两部分**
  - `cc.ps1.tpl` 用 `{{CC_FUNCTION_BLOCK}}` placeholder，`render_cc_code`
    根据 `include_cc_function` 决定是否填充
  - `__ccm_bind` 永远装（cc 集成的核心）
  - `function cc` 现在是**可选**部分
- **UI 智能默认值**：扫描结果发现 profile 已含自定义 `function {命令名}` 时
  自动取消勾选"也装默认 function cc"复选框，安装时跳过 cc function 段
- 用户已有 cc 时的指引：在 cc 开头加一行 `__ccm_bind` 即可。例如：
  ```powershell
  function cc {
      __ccm_bind                    # ← 加这一行
      if ((Get-Location).Path -eq $env:USERPROFILE) {
          Set-Location 'D:\Sync\文档\claude-conversation'
      }
      # ... 用户自定义代理 / 其他逻辑 ...
      claude @args
  }
  ```

### IPC 改动

- `cc_integration_preview({command_name, include_cc_function})` ← 新增 bool 参数
- `cc_integration_install({path, command_name, include_cc_function})` ← 新增 bool 参数

### 用户操作

v1.7.2 已安装 + 自定义 cc 被覆盖的用户：
1. 装 v1.7.3 → 启动 monitor
2. 设置面板 → PowerShell 集成
3. 扫描会发现你已有 `function cc` → 复选框自动取消勾选
4. 点"安装" → 只装 `__ccm_bind` helper（不动你的 cc）
5. 编辑 profile，在你的 `function cc` 开头加一行 `__ccm_bind`
6. 重启 PS

## [1.7.2] — 2026-05-22

### 修复（release-blocker）

- **v1.7.0/1.7.1 装错 profile 文件名导致 cc 集成形同虚设** ——
  - 错的：`Documents/WindowsPowerShell/profile.ps1`（CurrentUserAllHosts，PS 启动**不**自动读）
  - 对的：`Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost，即默认 `$PROFILE`）
  - 用户在 PS 里跑 `$PROFILE` 看到的就是后者。v1.7.0/1.7.1 装到前者 PowerShell 启动根本不加载，整个 cc 集成无效。
  - v1.7.2 `profile_installer::discover_profiles` 改用正确文件名。
  - 新增 `scan_legacy_profiles()` 检测 v1.7.0/1.7.1 错位的 profile.ps1 中是否含
    cc-monitor 块。UI 在状态扫描时显示警告 + 列出文件路径，引导用户手动清理。

### 改动（UX 大改）

- **设置面板"PowerShell 集成"区单卡片重构**：
  - PowerShell 版本下拉（Windows PowerShell 5.1 [默认] / PowerShell 7.x / 自定义路径）
  - profile 路径**可编辑输入框**（默认按版本下拉自动填充 `Microsoft.PowerShell_profile.ps1`，
    用户可手动改成任意路径——比如非标准的 OneDrive 同步路径、portable PowerShell、
    或者特殊 host 的 profile）
  - 选"自定义路径..."后路径输入框获焦让用户填
  - "重新扫描"按钮配合 flash 视觉反馈（之前点了没反应的设计 bug）
  - 状态徽章 (未安装/已安装/文件不存在)
  - 旧位置遗留警告框（紧贴主操作下方）
- **自动识别**：PS 5.1 永远显示（Windows 自带）；PS 7.x **只在 `Documents/PowerShell/` 目录存在时**才作为可选项展示，否则隐藏（绝大多数用户没装 7.x，UI 不再误导）

### 重构（后端 IPC）

- `cc_integration_install({path, command_name})` ← 之前 `{kind, command_name}` 改成接受路径直接
- `cc_integration_uninstall({path})` ← 同上
- 新增 `cc_integration_scan_path({path, command_name})` —— 用户改路径后扫描那个路径
- `cc_integration_status` response 新增 `legacy_profile_paths_with_block` 字段
- `ProfileKind` 加 `Custom` 变体

### 用户操作流（v1.7.2 安装）

1. 装 v1.7.2 后**首次启动 monitor**（auto-launch.json 会自动更新 monitor_exe_path）
2. 设置面板 → PowerShell 集成
3. 版本下拉默认 **PS 5.1**，路径已自动填 `Microsoft.PowerShell_profile.ps1`
4. 如果有 v1.7.0/1.7.1 遗留块，会看到"⚠ 检测到旧位置遗留" + 路径列表 → 手动用编辑器
   打开那个 profile.ps1 删除 BEGIN/END 之间内容（或整个文件删掉）
5. 点"预览代码"看完整内容
6. 点"安装" → 把 cc function 写到正确的 `Microsoft.PowerShell_profile.ps1`
7. **重启 PowerShell**
8. 跑 `cc` → 应该自动握手成功，Tab ↗ 能拉对应 WT 窗口

## [1.7.1] — 2026-05-22

### 新增

- **cc → 自动启动 monitor**（可选 toggle）—— v1.7.0 要求先开 monitor 后跑 cc，
  顺序反了 cc 会 fail-open（仍能启 claude，但没绑定）。v1.7.1 让 cc function
  能主动启动 monitor，但**不硬编码安装路径**（保持 portable exe 特性）：
  - monitor 每次启动调 `std::env::current_exe()` 写自身路径到
    `<monitor_data_dir>/auto-launch.json` 的 `monitor_exe_path` 字段
  - 用户移动 exe 后下次启动会自动更新（不需要重新装 cc function）
  - 设置面板新加 toggle "用 cc 启动 claude 时自动打开 monitor"
  - cc function 读 auto-launch.json：
    - `auto_launch_enabled` = true 且 monitor 没在跑且记录的路径存在 →
      `Start-Process` 启动 + `Start-Sleep -Milliseconds 2000` 等 watcher 起来
    - 已在跑（按绝对路径比对 Get-Process 的 .Path）→ 跳过启动
    - 任何检查失败 → fail-open（仍走握手，超时后 fail-open 启动 claude）
- 新 IPC：`cc_get_auto_launch` / `cc_set_auto_launch`
- 新模块 `src-tauri/src/auto_launch.rs`（含 3 个单测）

### 改动

- `scripts/cc.ps1.tpl` 加 auto-launch 段（读 auto-launch.json + Start-Process）
- 设置面板 PowerShell 集成区底部新增 toggle + monitor 路径显示

### 用户操作

第一次启用 auto-launch：
1. 至少启动一次 v1.7.1 monitor（让它记录自身路径到 auto-launch.json）
2. 设置面板 → PowerShell 集成 → 勾选 "用 cc 启动 claude 时自动打开 monitor"
3. 之后即使 monitor 没在跑，跑 cc 时会自动启动 monitor + 等 ~2s + 正常握手

## [1.7.0] — 2026-05-22

### 新增

- **cc 命令注入式绑定 Tab ↔ 终端窗口**——v1.6.x 的 4-tier 启发式算法在
  explorer 启 PowerShell + WT DefTerm 接管 console 的常见架构下不可靠（claude
  祖先链与 WT 窗口完全脱节）。v1.7 改成 PS 主动跟 monitor 握手：
  - 用户用 `cc` 命令替代 `claude` 启动会话（cc 是 PS function，包装 claude）
  - cc function 写 `ps-await/<PID>.json` + 设独特 WindowTitle marker
  - monitor 后台 watcher 调 EnumWindows 找含 marker 的窗口 → 拿到 hwnd
  - 写 `ps-registry/<PID>.json`（PS_PID ↔ hwnd 映射）→ 解除 PS 阻塞
  - 之后 claude 启动写 `sessions/<PID>.json`，monitor 用 ToolHelp 查
    claude.exe 的 parent_pid 反推 PS_PID → ps-registry → 拿 hwnd
  - 写永久 `sid-hwnd-cache.json`（含复合指纹：hwnd + owner_pid + procStart）
  - Tab ↗ / Ctrl+\` 查缓存 + 校验指纹 + SetForegroundWindow

- **设置面板"PowerShell 集成"区** —— 一键扫描 + 安装 + 卸载 cc function
  到 PS profile：
  - 同时扫描 PS 5.1 (`Documents/WindowsPowerShell/profile.ps1`) + PS 7.x
    (`Documents/PowerShell/profile.ps1`) 两个 profile 路径
  - 检测命令名冲突（profile 已有同名 function 时 UI 警告，建议改名）
  - 命令名可自定义（默认 `cc`，用户可输入 `ccm` / `monclaude` 等）
  - "预览代码"按钮弹 modal 展示完整将要写入的代码（含 BEGIN/END marker）
  - 块标记隔离：`# === cc-monitor BEGIN v1 ===` / `# === cc-monitor END ===`
    重装时整块替换、卸载时整块删除，用户在块外任何内容不动
  - 实时显示当前活跃 PS 注册数

- **rust 后端新增模块**：
  - `bind.rs`：BindRegistry（ps-await 监听 + EnumWindows + ps-registry 持久化）
    + SidHwndCache（sid → hwnd 持久化）+ verify_binding / activate 拉前
    + 心跳 10s 清死 PS 注册
  - `profile_installer.rs`：profile 路径解析 + 块插入/卸载 + 命令名冲突检测
  - `scripts/cc.ps1.tpl`：cc function 模板（include_str! 嵌入二进制）

- **rust 后端新增 4 个 Tauri IPC 命令**：
  - `cc_integration_status` — 扫描两个 profile 状态
  - `cc_integration_preview` — 渲染将要写入的代码（不修改文件）
  - `cc_integration_install` — 写入指定 profile（PS 5.1 或 PS 7.x）
  - `cc_integration_uninstall` — 移除 BEGIN/END 块
  - `bring_terminal_to_front` — 拉前命令（v1.6.7 删除后恢复，但实现完全重写）

### 改动

- Cargo.toml 恢复 `Win32_System_Diagnostics_ToolHelp`（用于 claude.exe →
  parent_pid 查询）+ `Win32_UI_WindowsAndMessaging`（EnumWindows / GetWindowTextW /
  SetForegroundWindow）feature
- 前端恢复 Tab ↗ 按钮 + Ctrl+\` 快捷键 + 失败时右下角 fixed toast

### 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| profile 修改方式 | 一键安装 + 预览 + 卸载 | 默认便利但完全透明，BEGIN/END 块隔离不动用户其他内容 |
| 默认命令名 | `cc` | 短易记；UI 可改 |
| 没装 cc 时 | 报"未绑定窗口"不 fallback | 老 4-tier 算法已彻底删除 |
| 复合指纹 | hwnd + owner_pid + owner_proc_start + ps_proc_start | 防 HWND 复用 + PID 复用 |

### 用户操作流（首次安装）

1. 设置面板（Ctrl+,）→ 滚到"PowerShell 集成"区
2. 点 PS 5.1 或 PS 7.x 卡片的"安装"
3. 重启 PowerShell
4. 新 session 启动时 PS function 自动跟 monitor 握手（< 100ms，无感知）
5. 用 `cc` 替代 `claude` 启动会话
6. 之后 Tab ↗ / Ctrl+\` 直接拉对应 WT 窗口

### 设计文档

`D:/Sync/文档/cc-monitor-v1.7-cc-integration-plan.md`（plan + 时序图 + 数据结构）

## [1.6.7] — 2026-05-22

### 移除

- **`bring_terminal_to_front` 整条链路撤回**（v1.6.0–1.6.6 的"Tab ↗ 拉对应
  终端窗口"功能）。在 explorer 启 PowerShell + Windows Terminal DefTerm 接管
  console 的常见架构下，claude.exe 的祖先链与 WT 窗口完全脱节，4-tier
  启发式（祖先链 / 终端类进程 + title 匹配）无法可靠定位"哪个 WT 窗口跑了
  这个 session"。Ambiguous 报错让用户疲于配置独特 title，"Claude Code"
  fallback 又引入新歧义（误命中无 ai-title session 的同名窗口）。算法层修不
  动这个问题——需要 OS API 不暴露的"PowerShell PID ↔ WT HWND"映射。
  - Rust：删 `session_map.rs` 里 `bring_terminal_to_front` 方法 + 整个
    WindowMatcher（`SelectResult` / `MatchTier` / `build_ancestors` /
    `build_search_terms` / `classify_window` / `select_best_window` /
    `ProcInfo` / `WindowSnap` / `is_system_shell_process` /
    `is_terminal_process` / `process_info_snapshot` /
    `enumerate_top_level_windows` / `activate_window`）+ 14 个对应单测
  - `lib.rs`：删 `bring_terminal_to_front` Tauri 命令注册
  - `Cargo.toml`：删 `Win32_System_Diagnostics_ToolHelp` /
    `Win32_System_ProcessStatus` / `Win32_UI_WindowsAndMessaging` 三个 feature
  - 前端：删 `tabs.ts` 的 `bringActiveTerminalToFront` / `bringTerminalToFront` /
    `showBringTerminalToast` + Tab 上的 ↗ 按钮 + `main.ts` 的 Ctrl+\` 快捷键 +
    `styles.css` 的 `.tab-focus` / `#bring-terminal-toast` /
    `.status-msg.status-error`
  - 文档：删 `src-tauri/src/README.md`（专讲拉终端机制的设计文档）
- 保留 `SessionInfo.name` 字段（标记 `#[allow(dead_code)]`），为 v1.7 注入式
  绑定方案准备。

### 保留

- session_map.rs 心跳（2s 探活清死 session，v1.6.3 引入）
- watcher.rs force_rescan 通道 + SessionChange.added 字段（v1.6.3 引入，修
  /resume 竞态的 session 新增鲁棒重扫，跟拉终端无关）

### 下一步

v1.7 通过 `cc` 命令注入式绑定实现拉终端：用户用包装后的 `cc` 启动 claude，
wrapper 主动把 (sid, hwnd) 映射注册给 monitor，绕开"无法从进程树定位窗口"
的 OS 限制。

## [1.6.6] — 2026-05-22

### 修复

- **无 ai-title 的 session 拉前歧义** —— claude CLI 启动时默认 console title
  是 "Claude Code"，要等会话生成 ai-title 后才改成项目语义名。**没生成 ai-title
  之前**，对应的 WT 窗口 title 就是 "✳ Claude Code"。之前 `build_search_terms`
  只用 cwd / 项目名做匹配，没有任何 term 能命中 "Claude Code"——所有终端类
  窗口都 tier D，select 报歧义。
  - `build_search_terms`：当 `ai_title is None` 时把 `"Claude Code"` 加入 terms。
    "Claude Code" 窗口 title_match → 升 tier C (TerminalWithTitle)，唯一命中
    时 select 取它。其他有 ai-title 的窗口（"filter-active..." / "Analyze
    shengwu..."）仍 tier D，不参与竞争。
  - 角落情况：多个无 ai-title 的 session 并存 → 所有 "Claude Code" 窗口同 tier
    C 多候选 → 仍歧义，需要用户配独特 title（toast 提示）。

### 测试

- 单元测试 34 → 36。新增 2 个：`search_terms_include_claude_code_fallback_when_no_ai_title`
  + `search_terms_skip_claude_code_fallback_when_ai_title_present`。

## [1.6.5] — 2026-05-22

### 修复

- **点 ↗ 按钮 monitor 假死 + 消息区域被挤位**（强烈关联 Bug 1 "拉不起来"）——
  根因有两个，一起修：
  1. `bring_terminal_to_front` 是 sync `#[tauri::command]`。Tauri 2 sync 命令
     在 main IPC thread 跑（不是 spawn_blocking），命令期间整个 webview 假死
     不响应任何输入。改 `async` + 显式 `tokio::task::spawn_blocking` 包 Win32
     调用，IPC 主线程立即返回，webview 全程可点。
  2. v1.6.4 把错误写进状态栏文字（`statusMsg.textContent`）会触发 flex 重排，
     长错误字符让 `.status-msg` 内部 layout 变化，间接挤压上面的 message stream
     区域 → 用户看到"消息往右移动"。改 fixed 定位的 `#bring-terminal-toast`
     固定在右下角，完全脱离文档流，绝对不影响其他 element。
- **前端 invoke 加 5s timeout** —— 若极端情况下后端仍卡（如 EnumWindows
  callback 撞上 hung window），5s 后强制 reject 显示"invoke 超时"toast，
  不再让 monitor 看上去假死。

### 已废弃

- `.status-msg.status-error` CSS 规则保留但不再使用（v1.6.4 引入 + v1.6.5 替换）。

## [1.6.4] — 2026-05-22

### 修复

- **`bring_terminal_to_front` 失败时用户看不到原因** —— v1.6.3 加了
  Ambiguous / NoMatch 详细错误，但前端只 `console.warn` 没人开 DevTools。
  这次把后端 Err 字符串抬到状态栏显示 8s（红色 `⚠` 前缀 + `title` 属性
  保留完整文本，hover 可看截断前的全文）。现在"拉不起来"能直接读到
  "歧义：A 命中 4 个终端窗口 (sid=..., terms=[...])；候选: [...]；
  修复：在 PowerShell startup 给当前会话窗口设独特 title"。

## [1.6.3] — 2026-05-22

### 修复

- **多 Tab 拉错终端（同一窗口被反复选中）** —— Windows Terminal 单进程多窗口
  共享同一个 PID，所有 WT 窗口都"在 claude 的祖先链上"——`classify_window`
  把它们全部归到 tier A 或 tier B，而 `select_best_window` 旧实现"同 tier 只
  记第一个候选" 导致多个 session 撞同一窗口（EnumWindows Z-order 的第一个）。
  - 新增 `SelectResult { Single | Ambiguous | NoMatch }`：tier 内多候选时返
    `Ambiguous`，调用方报详细错（含命中 tier + 候选 hwnd/title + 配置建议）
    而非随机选一个。**拉错 → 拉不到，但用户得知该如何修**。
  - `build_search_terms` 加完整 cwd 路径作 term（含反斜杠 / 正斜杠两个版本）：
    用户在 PS startup 设 `$Host.UI.RawUI.WindowTitle = $PWD` 时能精确匹配
    每个会话独有的窗口。

- **关闭终端窗口后 Tab 不归档** —— Claude Code 异常退出时
  `~/.claude/sessions/<PID>.json` 可能不会被删，session_map 仅靠文件事件触发
  扫描 → 死 session 永远不发 `session-ended`。`session_map::run_watcher`
  加 **2 秒心跳**：`recv_timeout(2s)`，timeout 分支主动 `is_process_alive`
  探活所有 by_id 条目，死的自动 remove + emit removed → 前端 Tab 在 ≤2s 内灰显。

- **/resume 历史会话偶发不出现 Tab（多个并发时尤明显）** —— jsonl 行可能
  在 `sessions/<PID>.json` 之前到达 watcher；此时 `active(sid)` 返 false →
  `process_file` early return，且无任何机制重新触发该文件的扫描。新增
  **session-added → 强制重扫**安全网：
  - `SessionChange` 加 `added: Vec<String>`
  - `watcher::spawn_watcher` 返回 `WatcherHandle { rx, force_rescan_tx }`
  - lib.rs 收到 session_map 的 added 列表 → 通过 `force_rescan_tx` 通知
    jsonl-watcher 主动重扫该 session 的所有 jsonl 文件
  - jsonl-watcher 主循环改 `recv_timeout(100ms)` 兼容 rescan 通道（jsonl-line
    总延迟从 ~100ms 上升到 ~200ms，对流式渲染可接受）

### 测试

- 单元测试 29 → 34。新增 5 个：tier A 多候选 → Ambiguous、tier D 多候选 →
  Ambiguous、低 tier 唯一命中 → Single、完整 cwd 加入 terms、短 cwd 跳过完整路径。

## [1.6.2] — 2026-05-21

### 修复

- **`/compact` 等本地命令的 stdout 漏到 user 消息里渲染** —— Claude Code CLI
  把 `/compact` 写进 JSONL 时格式是 `<command-name>/compact</command-name>
  <command-message>compact</command-message><command-args></command-args>
  <local-command-stdout>Compacted...</local-command-stdout>`。v1.5 已过滤
  `<local-command-caveat>` 等 3 个标签但漏了 `<local-command-stdout>`，
  整条 user 消息因尾部多了一段无法匹配 slash 紧凑卡正则，回落到普通
  user 气泡把整段连同 stdout 一起渲染出来。这次：
  - 前端 `isInternalUserNoise` 重构为 `stripInternalNoise(text): string`
    返回剥过的文本（而非 boolean）；剥噪声列表补 `local-command-stdout`；
    user 分支用剥过的文本喂下游 `parseSlashCommand` / `buildUserCard`，
    `/compact` 现正确识别为 "⌘ /compact" 紧凑卡。
  - 后端 `history.rs::clean_user_text` 历史预览的 tag 列表同步补一项。

## [1.6.1] — 2026-05-21

### 修复

- **设置面板拖 color picker 卡顿** —— 每次 `input` 事件原本调 `applyTheme()`
  全量遍历 14 个 token 调 `setProperty`，60Hz 拖动下整棵 :root 子树重算被
  压垮。新增 `applyThemeToken(key, value)` 只更单 token；`onFieldChange`
  改调它。重算量降到 1/14。

### 新增

- **设置面板每行 "↺ 恢复默认" 按钮** —— 24×24 单项重置，仅回退该字段到
  styles.css :root 默认值。底栏的全量 "恢复默认" 按钮保留。

## [1.6.0] — 2026-05-21

v1.5.0 的迭代版。首次通过 `release.yml` 自动发布（v1.5.0 tag 指向的 commit
当时 release.yml 还未引入，无法触发自动 build → 跳过 v1.5.0 release）。

### 新增

- **历史浏览器"全量加载"按钮** —— 顶栏新增；点击后并发（max 4）拉取所有项目的会话详情进缓存。完成后搜索可命中 session 内容（ai-title / 自定义标题 / 首条消息 / sessionId）。状态条显示进度 `加载 N/M …`。

### 变更

- **图标改为纯字符**（去 emoji，避免跨平台字体差异）：
  - 顶栏历史按钮 `📜` → `◷` (U+25F7 时钟样圆形)
  - 重命名 `✏️` → `✎` (U+270E pencil)
  - 隐藏 `🙈` → `–` / 取消隐藏 `👁️` → `+`
  - 恢复 `↩️` → `↺` (U+21BA anticlockwise circle arrow)
  - 删除 `🗑️` → `✕` (U+2715 X)
  - 项目组前的 `📁` 移除（折叠指示器 `▸` 已够，多余）
  - **星标 `★/☆` 保留**（颜色高亮区分状态，且没有跨平台问题）
- **GitHub Actions CI** —— `.github/workflows/ci.yml`（push/PR 触发：rust fmt + clippy + test + frontend tsc + vite build）+ `release.yml`（`v*` tag 触发：tauri build + SHA256 + 自动 GitHub Release 发布）。
- **关键路径 tracing 埋点** —— `list_history_projects` / `list_history_sessions_in_project` / `read_session_jsonl` / `replay_and_mark_ready` 各加 elapsed_ms 日志，便于生产诊断慢点。

### 变更

- **TabBar 局部更新（refreshTabBar 差量 DOM）** —— 引入 `TabManager.tabButtons` 缓存：每个 Tab button 只创建一次，refresh 时只同步 class（active/archived/has-unread）+ 文本，按 `orderedIds` 顺序用 `insertBefore` 排序。Visibility 全交 CSS 控制。长 session 每秒数十次 `onLine` 时 DOM thrash 减少约 80%。
- **`TabManager.orderedIds: string[]`** —— 与 `tabs.keys()` 顺序一致的稳定数组，避免 `cycleActive` / `closeTab` 每次 `Array.from` O(N) 分配。
- **`session_map.bring_terminal_to_front` 重构** —— 160 行内嵌逻辑拆为 4 个纯函数（`build_ancestors` / `build_search_terms` / `classify_window` / `select_best_window`）+ `enum MatchTier`。主函数缩到 ~40 行做 orchestration。
- **`utils::days_from_civil`** —— `subagent.rs` 与 `history.rs` 各自的副本合并到新 `utils.rs`，单源。

### 修复

- `session_map.SessionInfo.status` / `SessionMap::load` / `SessionMap::get` / `SessionChange.added` / `messages::ContentBlock` 等死代码清理。cargo check 0 warnings。

### 测试

- 单元测试 15 → 29。新增 14 个覆盖 `build_ancestors`（链 / 环 / 缺失 parent）、`build_search_terms`（边界）、`classify_window`（5 个 tier 分支 + explorer 排除 + unrelated）、`select_best_window`（多 tier 共存 + 全无命中）。

---

## [1.5.0] — 2026-05-20

首个发 exe 的 release。

### 新增

- **历史会话浏览器**（顶栏 📜 / `Ctrl+H` / Esc）
  - 按**工作目录分组**展示所有历史 jsonl，项目默认折叠
  - **两级懒加载**：初次打开仅读项目级元数据（< 100ms，500 项目）；展开某项目才读其下会话详情；同项目再次展开秒开（缓存）
  - 操作：`★` 标星 · `✎` 重命名（中文 OK）· `–` 隐藏 · `↺` 恢复（拉起 wt.exe / cmd 跑 `claude --resume`）· `✕` 物理删除（二次确认）（v1.5 时是 emoji，v1.6 改纯字符）
  - 点击会话行进入**只读消息查看器**（复用实时 Tab 的渲染管线：Markdown / KaTeX / 代码高亮 / 折叠卡）；Esc 二级关闭（先关查看器再关视图）
  - 搜索框：匹配项目名 / 路径；已缓存项目附加匹配 ai_title / customTitle / first_user_excerpt
  - 用户元数据存 `<monitor_data_dir>/history-metadata.json`（永远在默认位置，不随 claudeDir 切换）

- **Claude 数据目录可配置**（设置面板 → 数据 → Claude 数据目录）
  - 三级回退：① 设置面板配置 `claudeDir` → ② `$CLAUDE_CONFIG_DIR` 环境变量 → ③ `~/.claude` 默认
  - 改后弹"需要重启 monitor"提示
  - 支持文件夹选择对话框（`tauri-plugin-dialog`）

- **vite 端口可配** —— `VITE_PORT` 环境变量覆盖默认 1420，HMR 端口自动 = port + 1

### 修复

- **鼠标光标卡死**（选中文本 / 关闭终端后偶发"鼠标卡为手型、点击无响应、滚动可用"）
  - 根因：jsonl-line 事件大量积压时主线程被 `marked.parse` + `hljs.highlightAuto` 同步渲染压垮
  - 修复：`events.ts` 改批量调度（≤40 条/批，≤8ms/批，`setTimeout(0)` 让出主线程）；`render.ts` 砍 `hljs.highlightAuto`（无 lang 时直接 escape，10kB 代码块 30-50ms → ~0ms）

- **resume 报错 0x80070002**（ERROR_FILE_NOT_FOUND）
  - 根因：旧代码 `wt.exe -d <cwd> pwsh -NoExit -Command "..."`，但 `pwsh.exe` 是 PowerShell Core 独立安装包，不是 Windows 自带
  - 修复：改用 `cmd /K "claude --resume <id>"`，`cmd.exe` 永远在系统目录可用；Plan B 用 `CREATE_NEW_CONSOLE` flag 兜底

- **关闭 Tab 后 DOM 引用残留**：`closeTab` 显式 `clear()` toolUseNames / toolUseElements Map，加速 GC

- **跨电脑硬编码路径**：`paths.rs` 抽出 `resolve_claude_dir()` 三级回退；`session_map.rs:147` 把 `cwd.rsplit(['\\','/'])` 换成 `Path::file_name()`

- **生产 panic 路径**：`watcher.rs:32` 把 `.expect()` 改成日志降级；`session_map.rs:78` `.ok()` 吞错改成 `tracing::error!`

### 变更

- **event_replay 取消 5000 条 cap** —— 历史塞全部，重启清
- **watcher 取消初始 1500 行截断** —— 全量读，由 event_replay 持锁保证顺序
- **HMR 强制 `window.location.reload()`** —— 避免部分热替换导致状态错乱
- **过滤 Claude Code CLI 内部 prompt 包装** —— `<task-notification>` / `<system-reminder>` / `<local-command-caveat>` / `<synthetic>` 不入消息流

### 打包

- `productName` 从 `Claude Code` 改为 `cc-monitor`（避免与 Anthropic 官方品牌冲突）
- `identifier` 从 `com.local.monitor` 改为 `com.ccmonitor.app`（稳定反域名）
- 新增 `publisher` / `copyright` / `longDescription` / NSIS `installMode: perMachine` + 中英双语
- 新增项目根 `LICENSE` (MIT)；`Cargo.toml` / `package.json` 补 metadata
- 删除 `tray-icon` feature（实际未使用）

---

## v1.5.0 之前 — pre-release dev 阶段（无独立 tag）

第一个公开发布是 v1.5.0（2026-05-20）。在那之前，所有功能（实时渲染 / 多 Tab / SessionMap 进程探活 / LaTeX + 代码高亮 / tool_use 折叠卡 / subagent / 设置面板 GUI / `bring_terminal_to_front` 4 阶段 HWND 启发式 / 撤销 SessionStart hook 路线 / 撤销 U2 焦点同步等）都在 dev 阶段内完成，由一个 Initial scaffold + 数十个 feat/fix commit 累积成 v1.5.0 的初始功能集。

详细的关键转向与设计演化叙事见 [`../doc/HISTORY.md`](../doc/HISTORY.md)（不在 git 仓库的工作目录文档）。
