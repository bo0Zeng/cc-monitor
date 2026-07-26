# 前端模块导览（`src/`）

Vanilla TypeScript + Vite + Tauri 2 IPC。不引入框架（React/Vue 都没有）—— ~12K 行 TS 的中型应用（根 README 口径），原生 DOM 依旧足够——分层靠模块边界与本导览维持。

本文件做"开发者打开 src/ 后第一眼看到的导航"。后端结构见 [`../src-tauri/README.md`](../src-tauri/README.md)。

## 入口

```
index.html  ─> /src/main.ts (defer)
                  │
                  ├─ loadTheme()       从 config.json 应用 CSS 变量
                  ├─ new TabManager()  → tabs.ts
                  ├─ 设置改独立窗口（F82a）：⚙/快捷键 invoke("open_settings_window")；
                  │   ?settings=1 → bootstrapSettings 挂 SettingsPanel(windowMode)
                  ├─ new HistoryView()   → views/history.ts
                  ├─ bindEvents()         → events.ts （订阅后端 jsonl-line / session-ended）
                  └─ emit("frontend-ready")  通知后端 replay 历史
```

## 模块分工

| 文件 | 角色 | 关键 API |
|---|---|---|
| **main.ts** | 启动 + 全局快捷键 + 错误捕获 + HMR 强制 reload；**issue #10**：URL 带 `?viewer=<sid>` 时走 `bootstrapViewer`（复用 TabManager 过滤到该 sid + 隐藏 chrome + 定向 replay 代替 frontend-ready）| DOMContentLoaded handler |
| **events.ts** | 订阅后端 `jsonl-line` / `jsonl-batch` / `session-ended` / `task-update`，**批量调度让出主线程**；v2.2 (issue #12) `jsonl-batch` 包 `batch-start`/`batch-end` 哨兵；v2.3.1 (issue #1) 300ms grace 续期。**v2.6 B 重构**：删了 PayloadSource / onChunk callback / BatchChunkMeta；JsonlLinePayload 新增 `seq: u64` 字段（后端 watcher 给的 per-file 单调）。**issue #10**：`bindEvents` 改 async（await listener 注册完成再返回，防 emit-before-listen 丢事件）+ `{windowScoped}` 选项（viewer 用 `getCurrentWebviewWindow().listen` 接定向事件） | `await bindEvents({...}, {windowScoped?})` |
| **tabs.ts** | TabManager 状态机：Tab 生命周期（live / archived）+ BranchFolder + v2.3 TasksPanel 路由；v2.4 (issue #2) `switchTo(sid, "manual"\|"auto")` + `userActive(sid)` + 5s `manualOverrideUntil`；`applyBehavior(cfg)` 同步设置面板 toggle。**v2.6 B 重构**：删了 inPrependMode / pendingPrependFragment / pendingToolGroup / appendCardOrBuffer / onChunk / markCardUuid / feedBranchFolder；onLine 改调 renderStreamRecord 共享管线。**issue #10**：`tab.cwd` 取最早 seq 记录的 cwd（项目根，防会话 cwd 漂移到子目录）；`openActiveInNewWindow()` + Tab 右键「在新窗口打开」。onLine 入口双层去重：`seenSeqs`（#17 同 seq）+ `processedUuids`（#26 换 seq 重投按 uuid 拒重，INVARIANTS § 25） | `onLine(payload) / onBatchStart() / onBatchEnd() / userActive() / openActiveInNewWindow() / archiveTab() / closeTab() / cycleActive()` |
| **account-chip.ts / account-commands.ts / account-restart.ts** (A3–A5, #68/#69) | 多账号 UI + 编排：`account-chip` 徽章 + 切号菜单（mismatch/align 态）；`account-commands` 的 `withAccount` 按会话选账号起/Resume（账号解析 + lastAccount 记账）；`account-restart` 换号**破坏性**重启（[compact]→kill→resume 同 sid，失败语义严格照 DESIGN §5.2：compact 失败不阻断 / kill 失败必中止）。三者共用 `accounts.ts` 原语（fetchAccounts / accountConfigDir / recordLastAccount）。`account-color` 账号色板 / `settings/acct-deploy` app 内部署向导 | `withAccount() / restartSessionWithAccount() / accountConfigDir()` |
| **record-timeline.ts** ⭐ v2.6 | 按 seq 排序的 TimelineEntry 数组 + DOM 挂载：`insert(entry)` binary search 找位置 → `stream.insertNode(element, anchor)`；`peekPrev(seq)` 给 tool-group 后处理用。**消除了** inPrependMode/pendingPrependFragment/chunkIndex 全部状态机。Batch13-F40a：deferMode（启动重放延后挂载）退役——旧记录由 TailWindow 收纳不建卡，本类回归纯「seq 有序 + 邻居查询」 | `new RecordTimeline(stream).insert / peekPrev / dispose / size` |
| **live-window.ts** ⭐ F40a | live tab 尾部优先窗口账本（单洞后缀不变量）：`floor` 水位 + 惰性排序 pending。启动重放旧块 `defer` 收纳（不建卡）；`takeTail(k)` 弹 seq 最高段供物化/上翻补批（F40b） | `new TailWindow().admit / pinFloor / defer / takeTail / pendingCount / dispose` |
| **render-window.ts** ⭐ F39 | viewer 未渲染区间账本：有序不相交半开区间集（尾段+深链岛+洞的几何），`markRendered` 区间减法 / `gapAbove` 找最近上方洞——上翻补批的口粮来源 | `new UnrenderedRanges(total).markRendered / gapAbove / lowestRenderedIdx / contains / remaining` |
| **e2e-probe.ts**（F40c，DEV-only） | E2E 探针，生产构建零包含（main.ts 以 `import.meta.env.DEV` 门控动态 import 接线于 onBatchStart/End）：重放抖动 rAF 采样（密度绊线，标定见头注释）+ 状态快照（Ctrl+Alt+F9 / 中键状态栏 → `TabManager.debugSnapshot()`），经 fe_perf 日志落盘——无 devtools 环境的 E2E 断言出口（详 e2e/README.md） | `startReplayJitterProbe / stopReplayJitterProbe / registerSnapshotHotkey / countReversals` |
| **render-stream-record.ts** ⭐ v2.6 | 三 caller（TabManager / SessionViewer / Subagent）共享的渲染管线。F40a 拆两段式：`routeMetaAndBranch`（title/queue 路由 + branch 喂送——收纳与渲染路径的单一来源）+ `renderContentRecord`（建卡 / tool-group 合并 / userActive）；`renderStreamRecord` = 组合壳。sink 接口抽象 caller 差异。**F62**：加可选 `onCardRendered(el,msg)` 钩子——仅 SessionViewer 用（"card" 分支调，给卡挂建分支按钮），TabManager/Subagent 不设 = 零影响 | `routeMetaAndBranch(payload, sink)` / `renderContentRecord(payload, ctx, sink)` / `renderStreamRecord(...)` |
| **stream.ts** | 单 Tab 的消息流容器，ResizeObserver + **守卫式 `snap()`** 自动贴底；`contentElement` 暴露真实卡片容器给 BranchFolder。`insertNode(node, anchor)` 给 RecordTimeline 按 seq 挂卡。贴底稳定性三约束见 INVARIANT § 21（守卫 snap + overflow-anchor + 尾部优先收纳） | `MessageStream.insertNode() / scrollToBottom() / dispose()` |
| **branching.ts** (issue #8/#22/#25) | parentUuid 拓扑分析：识别 ESC 回退主线 vs 被回退分支。`computeMainBranch` = "fork 点选 latest-descendant 赢家" + "多 root 折叠被 ESC 回撤废弃的首条/重发"（#22）。单链全 on-main；多 root 时只折叠死胡同的 plain user root，/compact·/clear·链断·pre-compact 历史保留。入口按 uuid 去重对重投幂等（#25，INVARIANTS § 25） | `computeMainBranch(records) / extractBranchRecord(rec)` |
| **branch-fold.ts** (issue #8) | DOM 重排：把连续的 off-main 卡片包到 `.branch-fold-wrap`，header「已被 ESC 回退（含 N 条）」；策略 = unwrap-then-rewrap 全量重建。v2.2 加 batch mode (`setBatchMode/flushPending`)，重放期延后到 batch 结束才算一次 mainBranch，省 O(N²)。**v2.6 起由 render-stream-record.ts 统一调用 recordAdded**（之前 tabs.ts 直接调）；`seenUuids` 拒重（#25） | `new BranchFolder(container).recordAdded / setRecordsAndRebuild / setBatchMode / flushPending` |
| **cards/index.ts** | renderMessage 主分发：user 气泡 / assistant 卡 / 工具组合并 / tool_result 注入到 tool_use。**v2.6 RenderContext.pendingToolResults 改必填 + 加 lazy 字段**（透传给 renderMarkdown 控代码块占位） | `renderMessage(rec, ctx) → RenderResult` |
| **cards/slash.ts** | `/` 命令紧凑卡 | `parseSlashCommand / buildSlashCommandCard` |
| **cards/compact.ts** | `/compact` 续接消息折叠 | `isCompactSummary / buildCompactSummaryCard` |
| **cards/subagent.ts** | Task/Agent tool_use 折叠卡 + 懒加载 subagent JSONL | `isAgentTool / buildAgentCard` |
| **cards/diff.ts** (issue #14) | Edit/Write/MultiEdit 的行级 diff 卡（tool_use 折叠条 body 级替换；上半纯逻辑 DOM-free，diff.test.ts 锁） | `isDiffTool / buildDiffBody` |
| **cards/interactive.ts** (issue #21) | AskUserQuestion / ExitPlanMode 默认展开卡（提问+选项 / plan 正文直接可见，不进工具组；答复后降噪+选中高亮） | `isInteractiveTool / buildInteractiveCard / markInteractiveAnswer` |
| **cards/api-error.ts** (issue #21) | API 报错可见化：最终失败红色报错卡 + 重试中间态单行细条（双 shape error 解析，api-error.test.ts 锁） | `buildApiErrorCard / buildApiRetryCard / describeRetryError` |
| **render.ts** | marked + KaTeX + highlight.js + DOMPurify。**v2.6 改 opts.lazy 参数化**（替代原全局 setRenderLazyMode flag）— 同步调用栈 save/restore 模式，避免 SessionViewer / Subagent 被 tabs 的 batch 模式污染走 lazy 路径 | `renderMarkdown(md, { lazy? }) / renderPlainText(text) / enhanceCard / observeForEnhance` |
| **local-storage.ts** ⭐ v2.6 | localStorage 统一接入层：LS_KEYS 集中常量（含动态 key 工厂 `settingsCollapsed(id)` / `toolRender(toolName)`）+ safeGet/safeSet/safeGetJson/safeSetJson/enumeratePrefix 包 try/catch。INVARIANT § 14 守护 | `LS_KEYS / safeGet / safeSet / safeGetJson / safeSetJson / safeRemove / enumeratePrefix` |
| **format.ts** ⭐ v2.6 | 时间 / 字节格式化合并：消息卡用 `formatTimestampShort`（永远 hh:mm）；历史浏览器用 `formatTimestampSmart`（当天 hh:mm，跨天加日期）；`formatBytes` 统一精度 | `formatTimestampShort / formatTimestampSmart / formatBytes` |
| **remote-launch.ts** (B14-F41) | 远端命令构造纯函数（sid 白名单 / launcher denylist / POSIX 引号 / 嵌套 env unset）：`buildResumeDirectCmd` + F48 `buildOpenTerminalCmd` + F51 `buildAttachCmd`/`isValidTmuxName` + F52 `buildResumeTmuxCmd` + F53 `buildLauncherCmd`/`deriveTmuxName`。DOM-free，`remote-launch.test.ts` 锁 | `buildResumeDirectCmd / buildResumeTmuxCmd / buildLauncherCmd / buildAttachCmd / posixQuote / isValidSessionId` |
| **remote-launch-run.ts** (B14-F41) | 远端拉起执行器：`invoke('launch_remote_terminal')` → 失败回退把命令复制到剪贴板 + toast 提示（happy-path 拉终端，degrade-path 复制） | `runRemoteResume / runRemoteResumeTmux / runRemoteLauncher / runRemoteAttach` |
| **agent-profile.ts** (F-MA) | 前端侧 agent 画像——把散落在 `cards/*` / `tabs.ts` 的 **Claude Code 专属工具名 / 进程名 / 嵌套 env** 常量收敛到一处（对应 Rust `src-tauri/src/adapter`）。第一刀值不变、行为零变化，接第二个具体 agent 时这里按 agent 切换；只收敛「工具名字符串」，不拆记录模型 | `AGENT_PROFILE`（agentTools / interactiveTools / diffTools / mdTools / livenessProcessNames 常量集） |
| **session-backend.ts** (F90 SS-12 #48) | **会话后端**前端座——把 remote-launch.ts 里硬编码的 `tmux …` 命令字面量收敛到一处（守 INVARIANTS §31「一端起的会话另一端能接」）。与 agent-profile.ts 两轴正交（**哪个 AI** vs **哪个多路复用器**），皆纯模块被 remote-launch.ts 消费。**阶段①只做形状**：唯一后端 = tmux、`SESSION_BACKEND ≡ TMUX_BACKEND`、无运行时后端选择（abduco/dtach + daemon RPC = 阶段②） | `SessionBackend / TMUX_BACKEND / SESSION_BACKEND` |
| **turn-notify.ts** (B14-F42) | 完成一轮系统通知：`turnEndNotifier` 单例 `observe(sid, tabTitle, payload, inBatch)` 判 turn-end 弹通知，四门（批量 / 新鲜度 / 防抖 / 聚焦）+ 插件权限懒检查；`turn-notify.vitest.ts` 锁 | `turnEndNotifier.observe(...)` |
| **remote-health.ts** (SS-F #32) | listen `remote-health` 事件，按 `(origin,kind)` 节流后弹灰色 info toast：`overflow`（拥塞丢行）/ `version`（旧 daemon 降级）/ `degraded`（B14-F59 daemonless 降级模式，`headlineFor` 映射「远端降级模式」）。`remote-health.test.ts` 锁纯逻辑 | `bindRemoteHealth() / headlineFor(kind)` |
| **settings/remote-section.ts** (issue #15 + B14) | 设置面板「远端」区：全局启用 toggle + 每台主机卡（label/host/port/user/密钥/daemonPath/指纹+重置/**备用地址** F45/**跳板** F56/**daemonless 降级勾选** F59）+ 测试连接（阶段日志 F46）+ 公钥推送 F50 + 文件面板/端口转发 F58 入口 + 「开新 Claude」F53 + ssh-config 批量导入·智能聚合 F57。`readRemoteConfig`/`writeRemoteConfig`（config.json `remote` 段 R/W，逐字段写全防丢）+ `findHostByOrigin` F54 反查；`remote-section.vitest.ts` 锁往返/聚合/反查 | `RemoteSection.element / readRemoteConfig() / writeRemoteConfig() / findHostByOrigin()` |
| **config.ts** | invoke `load_config` / `save_config` 桥 | `loadConfig / saveConfig` |
| **paths.ts** | 操作 config.json 里 `claudeDir` 字段（设置面板调） | `getClaudeDirOverride / setClaudeDirOverride` |
| **behavior.ts** (v2.4 issue #2) | 操作 config.json 顶层两个行为 toggle：`autoFollowUserActive` (默认 true) + `bringMonitorToFrontOnUserActive` (默认 false)。运行时热更（不需要重启 monitor，跟 claudeDir 不同） | `getBehavior() / setBehavior(cfg)` |
| **theme.ts** | 把 ThemeConfig 应用到 :root CSS 变量 | `loadTheme / applyTheme / saveTheme` |
| **settings/panel.ts** | 设置面板（数据目录 + 行为 + 快捷键 + 外观 + 远端 + PowerShell 集成 + 诊断/数据存储）。**F82a 起挂独立 `settings` 窗口**（`windowMode`，SS-3 终态）：保存/行为/键位改动 `emit('settings-applied')` 广播，主窗口 listen 后重读应用 theme+behavior+keybindings（跨窗回调够不到）。`windowMode:false` 抽屉路径保留供回退 | `new SettingsPanel({ windowMode }).open()`；`bootstrapSettings`（main.ts） |
| **settings/cc_integration.ts** | PowerShell 集成子区（profile 选项 + wrapper toggle + 5 个预设下拉） | `CcIntegrationSection.element` |
| **settings/info-icon.ts** | `?` 信息图标 portal tooltip 组件 + 路径工具 | `makeInfoIcon(text) / swapFileName(path, newName)` |
| **sftp/panel.ts** (B14-F48/F49/F54) | SFTP 文件面板 overlay（每 host 从设置卡「文件」入口开）：浏览（面包屑/列表/排序）+ 传输（下载/上传 dialog picker + 进度 Channel + 取消）+ 拖入上传 + 写（新建目录/改名/删除 + 确认）+ 目录书签 + 「在此打开终端」+ 小文件编辑（F49：面板内浮层 textarea + 字符/字节数 + 覆盖确认 + 失败保留）+ F54 `revealPath` 定位高亮（会话工具卡→文件跳转）。消费 F47/F49 `sftp_*` 命令，独立 overlay 不碰 TabManager | `openSftpPanel(cfg, revealPath?)` |
| **sftp/paths.ts** (B14-F48) | 面板纯路径逻辑（面包屑/normalize/join/parent/basename/排序/书签增删/uuid），可单测 | — |
| **settings/collapsible-group.ts** (issue #7) | 通用可折叠分组，localStorage 持久 + grid-rows 平滑动画 | `new CollapsibleGroup({id, title, defaultCollapsed}).appendChild(...)` |
| **settings/diagnostics-section.ts** (v2.0.0+) | 设置面板「诊断」区：log_enabled toggle / log_level select / error_toast toggle / log 路径 / [打开 log/dir]；支持 `{ headless: true }` 给 CollapsibleGroup 复用 | `DiagnosticsSection.element` |
| **error-toast.ts** (v2.0.0+) | listen `monitor-error` 事件，右下角垂直堆叠红色 toast，点击直接打开 log 文件 | `bindErrorToast()` |
| **views/history.ts** | 历史浏览器（项目分组 + 两级懒加载 + 增删改 + v2.2 fork 树形 + 流式 session 列表）。**issue #6 加「全文」模式**：调 `search_history` 搜会话内容（默认 user/assistant 文本，可勾选含工具内容）+ 结果 snippet `<mark>` 高亮 + 点击跳 viewer 定位 | `HistoryView.open() / handleEscape()` |
| **views/session-viewer.ts** | 只读消息查看器（点击历史条目进入）；v2.2 改用 `stream_read_session_jsonl` + Channel 边收边渲染；**issue #6 加 `scrollToUuid`**：从搜索结果跳进来时加载后定位命中消息 + 临时高亮。**v2.8.1 修空白 bug**：流元素 class `stream session-viewer-stream` 没有 `.active`，命中基类 `.stream{visibility:hidden}`（多 Tab 机制：仅 `.active` 流可见）→ 卡片全渲染却不可见；`.session-viewer-stream` 显式 `visibility:visible` 修复（详 INVARIANT § 23）。另逐条 `try/catch` 渲染，单条失败不再整屏空白。**Batch13-F39 尾部优先增量渲染**：Channel 阶段只收集 payload（meta/branch 经 `routeMetaAndBranch`），首屏渲染尾 150 条（深链另渲目标 ±100 岛），上翻自动补批（同步手动补偿视口），`UnrenderedRanges`（render-window.ts）记账未渲染洞——37MB 会话首屏 65.5s→1.1s。**F62 建分支**：本地会话经 `onCardRendered` 钩子给每张 user/assistant 卡挂 `⑂`→invoke `create_branch_session`→info toast 一键 resume 分支（远端会话不挂） | `SessionViewer.load(opts) / dispose()` |
| **views/pane-preview.ts** (B14-F60) | 远端 tmux 画面只读预览 overlay（body-level fixed，点外关 + Esc + ✕）：`invoke('capture_remote_pane')` 抓 `tmux capture-pane -p` 的屏幕文本，等宽 `<pre>` 展示 + 「重新抓取」手动刷新；非 attach、只读非实时；一次只开一个 | `openPanePreview(origin, target) / closePanePreview()` |
| **views/port-forward.ts** (B14-F58) | 本地端口转发(-L)管理台 overlay（照 SFTP 面板范式）：列当前转发 + 加转发表单（选主机 / 本地端口 / 远端 host:port）+ 启停 + 刷新；消费后端 `start_forward`/`stop_forward`/`list_forwards`，转发经已有 SSH 连接隧道 | `openPortForwardPanel()` |
| **views/grid-monitor.ts** (F91 #27) | 多 agent 并排**监控**——跨机只读 mission-control 状态板（body-level fixed overlay）：一屏 grid、一 cell/会话按机器分组，显红绿灯 / 标题 / cwd / 运行中 subagent 数 / context% / unread / ⚙bg；点 cell = `switchTo` 导航。零后端零写，数据取 `TabManager.snapshotSessions()`，开着时 1Hz 轮询重渲；分组 / 排序 / 汇总纯函数可测 | `GridMonitorView.open()/close() / groupSessionsByOrigin / sortSessionsInGroup / summarizeSessions` |
| **views/command-bar.ts** (F84 #57) | 键盘命令栏（⌘K/Ctrl-K 命令面板 overlay，照 HistoryView 范式）：输入框 + 子串过滤 + 方向键选 + 回车执行 + Esc/点背景关。**首刀只列只读命令**（开 overlay / 窗口操作 / 导航），resume/new-session/kill 等写驱动动作首刀排除；命令列表由 main.ts 组装注入；`filterCommands` 纯函数可测 | `CommandBarView.open(commands) / filterCommands()` |
| **views/panorama.ts** (Batch15-P2) | 代码全景视图（纯 canvas 自研，零图库依赖）：顶栏「全景」→ body-level overlay，对当前本地会话 cwd 建 code-picture 索引 → 画代码库地图（子系统聚类气泡 + 脊柱文件圆 + 入口点描环）+ 覆盖信号 banner + 符号搜索 + 节点详情侧栏；`close()` 只 `display:none` 不卸载、同仓重开复用已算布局 | `PanoramaView.open() / close()` |
| **panorama/** (Batch15-P2) | 全景前端纯逻辑（无 DOM，vitest 可测）：`api.ts`（后端全景命令 invoke 封装，参数 camelCase）+ `layout.ts`（坐标变换 / 命中测试 / 气泡打包 / 覆盖文案）+ `types.ts`（core 直出 snake_case 类型镜像）+ `session-files.ts`（F70 从 jsonl Edit/Write 抽本轮改动文件喂高亮） | `api.index/overview/node/callers/callees/impact/search + layout / session-files 纯函数` |
| **agents-panel.ts** (issue #23) | 当前会话 subagent 列表 + 每 agent 状态灯：status-bar 一枚 chip（`N agents (M 运行中)`，0 隐藏）+ 点击展开 popover，每行一 agent（🟢 运行中 / ✓ 完成 / ✗ 中止 + [类型] 描述）。数据纯前端推断（TabManager 配对 Task/Agent 的 tool_use↔tool_result，会话 idle/归档时仍 running 标 aborted 防僵尸绿灯）；折叠状态 localStorage | `new AgentsPanel().setSession(sid, agents)` |
| **usage-hud.ts** (F88b #52) | 用量 HUD chip（挂 status-bar）：显活跃会话 **context 占用%**（最新一轮 assistant 的 input+cache token ÷ 模型上限），≥80% 高亮预警。纯前端零后端（数据来自 live 流 TabManager onLine）；**只 token 不 $**；模型上限表在 `views/pricing.ts`（未知模型显 `?`） | `new UsageHud().setActive(model, promptTokens) / onClick()` |
| **session-status.ts** (F91 #27) | 会话活动状态的**共享纯逻辑**（零 import，node 可测）：红绿灯类名映射（`idle`/`shell`=红、`waiting`=黄、`busy`/未知=绿）+ 跨会话监控快照 DTO。此前红绿灯语义内联在 tabs.ts `updateTabButton`，F91 抽出让 **tab-bar 灯与 grid cell 共用**一套（对 tab-bar 是逐字节等价重构） | `activityLightClass(status) / GridSessionSnapshot` |
| **tasks-panel.ts** (v2.3.0 issue #11) | Tab stream 顶部 sticky 折叠卡：显示 Claude Code CLI 的 task 列表（`~/.claude/tasks/<sid>/`）。完整 replace 渲染（无 diff），0 task 时整 panel 隐藏。折叠状态 localStorage 全局持久 (`cc-monitor.tasks-panel.collapsed`) | `new TasksPanel().update(tasks) / fetchSessionTasks(sid)` |
| **settings/data-section.ts** (v2.3.0 issue #3 A) | 设置面板「数据存储」折叠分组：调 `get_data_paths` 拉所有持久路径 + WebView2 UserDataFolder + localStorage keys；每项配 [打开] 按钮调 opener。纯展示，无危险操作 | `new DataSection({ headless }).element / refresh()` |
| **settings/mcp-section.ts** (F87 #50+#51) | 设置面板「集成」组内 MCP 一节（SS-14 读写分界）：**读**跨 scope 展示（用户 / local 只读、项目可写），后端 `read_mcp_servers` 宽容读三处；**写只**项目 scope（`write_project_mcp_server` / `remove_project_mcp_server` 硬编码只碰 `.mcp.json`）；设置窗拿不到活跃会话 cwd → 项目目录输入框（datalist 从 `list_mcp_project_dirs` 补全用过的项目）；纯函数可测 | `new McpSection().element / groupByScope / serverSummary / parseServerConfig` |
| **keybindings/registry.ts** (issue #5) | 全局快捷键派发 + 弹层栈管理（app 单例 `dispatcher`）：维护 chord↔ActionId + ActionId↔callback 映射，挂 window keydown listener dispatch 命中 callback，维护 overlay stack（`overlay.close` / 默认 Esc 关栈顶） | `KeybindingDispatcher / dispatcher.bind / applyOverrides / pushOverlay / startRecording` |
| **keybindings/actions.ts** (issue #5) | 快捷键 **Action 清单 = 单一真相源**：改一条（新增 / 改默认 / 标可用性）全套（dispatcher / 编辑器 UI / 持久化 schema）自动收敛。每条含 `id`（config.json 持久化 key）/ `default`（默认 chord，null=未绑）/ `available`（未上线灰显）/ `category`（分组） | `ACTIONS / ActionId / findAction / groupByCategory` |
| **keybindings/editor.ts** (issue #5) | 快捷键编辑器 modal overlay（前端 fixed lightbox，非跨窗口 IPC）：按 Category 分组列出所有 Action + 行内录制（监听一次 keydown 拿 chord）+ 冲突覆盖确认 + 解绑 / 恢复默认 | `new KeybindingsEditor().open()` |
| **keybindings/store.ts** (issue #5) | 快捷键覆盖的持久化：存 config.json 顶层 `keybindings` 字段（值=chord 串 / null=显式解绑） | `getKeybindings / setKeybindings` |
| **styles.css** | 全部样式 + token 系统 | — |

## 关键数据流

### 实时消息（活跃 session）

```
后端 jsonl-line emit
  → events.ts 入队 (BATCH_SIZE=40 / BATCH_MS=8 让出主线程)
  → tabs.ts onLine(payload)
     ├─ ensureTab(sessionId, cwd, path)
     └─ renderStreamRecord(payload, ctx, sink)        // 三 caller 共享管线
          ├─ renderMessage(record, ctx)
          │    ├─ user → buildUserCard / Slash / Compact
          │    ├─ assistant 含 text → buildAssistantCard
          │    └─ assistant 全工具 / user 全 tool_result → tool-group 折叠卡
          └─ timeline.insert({seq, element, ...})      // 按 seq binary insert
               → stream.insertNode(element, anchor)    // 守卫式 snap 贴底
```

启动重放（jsonl-batch，末块先发）走同一管线,但经 F40a 尾部优先门控:active tab 首条 content 钉 `floor`,尾块直渲;更老的块与后台 virgin tab 全部收纳进 `TailWindow`(不建卡,meta/branch 经 `routeMetaAndBranch` 照喂),批后空闲物化尾 150 / switchTo 同步物化(消抖 + 免建卡,INVARIANT § 21)。F40b:active tab 上翻到顶部 800px 内自动补批(200 条/批,同步手动补偿视口),顶端哨兵显示「还有 N 条更早消息」;批期大增量老块经 `midBatchBuffer` 批末一次挂载。

### 历史浏览（懒加载）

```
点击 📜 按钮 / Ctrl+H
  → HistoryView.open()
  → invoke('list_history_projects') → HistoryProject[]  （轻量，不读内容）
  → 渲染项目组 header（默认全折叠）

点击某个项目组展开
  → invoke('stream_history_sessions_in_project', { projectDir, onEntry: Channel })
  → 每条 entry 通过 channel 增量到达；缓存到 sessionCache.set(projectDir, items)
  → 渲染该组内的 session 行

点击某个 session 行
  → SessionViewer.load({ jsonlPath, ... })
  → invoke('stream_read_session_jsonl', { jsonlPath, onChunk: Channel })
  → Channel 阶段只收集 payload（F39 起**不边收边渲**）；收齐后渲染尾段 150 条
  → 上翻自动补批 200 条/批（渲染仍复用 renderMessage，与实时 Tab 同一套管线）
```

## 关键设计选择 + 理由

### 不引入框架（React/Vue）
起步时是 ~3k 行小应用，如今 ~12K 行仍未引框架：原生 DOM + 明确模块边界够用；少 100-200KB 依赖体积；HMR 强制 full reload 简化心智模型。

### `events.ts` 批量调度让出主线程
replay 一次性 emit 整个 history Vec，前端用 BATCH_SIZE=40 + BATCH_MS=8 + `setTimeout(0)` 让出主线程。
**为什么用 setTimeout 不用 queueMicrotask**：microtask 同一 tick 内连续清空，无让出效果，UI 仍卡。setTimeout(0) 强制让一帧。

### `theme.applyThemeToken` 单 token 增量应用
拖 color picker `input` 事件 ~60Hz 高频。`applyThemeToken(key, value)` 只动一个 CSS var，比 `applyTheme(全部)` 便宜 ~14 倍。否则每帧 setProperty 14 次会触发整棵 :root 子树重算。

### `info-icon.ts` 真挂 body 实现 portal
父 `.settings-panel` 有 `transform`，按 CSS spec 会让 `position: fixed` 的 containing block 从 viewport 重置到 panel → fixed 元素相对 panel 定位而非屏幕。挂 body 脱离 transform 子树是唯一可靠路径。详 [doc/INVARIANTS § 13](../doc/INVARIANTS.md#13-css-portal-元素必须真挂-body)。

### `renderMessage` 是纯函数
给定 record + ctx 返回 RenderResult，无副作用（除写 ctx.toolUseElements 配 tool_use ↔ tool_result）。实时 Tab 和历史只读视图复用同一套渲染，保证视觉一致。

### `cards/` 分模块（slash / compact / subagent）
不全塞 index.ts —— 这三种都有独立解析逻辑（regex / prefix / IPC），index.ts 是分发器。subagent 用回调注入主 renderMessage 避免运行时循环 require。

---

## 不变量

前端特有不变量：

- **renderMessage 是纯函数**：给定 record + ctx 返回 RenderResult，无副作用（除了写 ctx.toolUseElements）
- **MessageStream 一个实例对应一个 Tab**：closeTab 必须调 stream.dispose() 释放 ResizeObserver
- **批量 jsonl-line 事件让出主线程**：events.ts 不能改成 sync 派发（会让 replay 卡死光标）

全局约束（前端必读，定义在 [`doc/INVARIANTS.md`](../doc/INVARIANTS.md)）：

- § 12 — alert 不算错误反馈，关键失败用状态栏 toast
- § 13 — portal 浮层（tooltip/modal/dropdown）必须真挂 `document.body`
- § 14 — localStorage / IndexedDB key 必须前缀 `cc-monitor.`
- § 21 — 启动重放贴底消抖：守卫式 snap + 不手动补偿（靠 overflow-anchor）+ 尾部优先收纳（F40a）
- § 25 — 行投递 at-least-once：按 uuid 累积状态/构建拓扑的模块必须入口拒重自行幂等（onLine processedUuids #26 / computeMainBranch+BranchFolder #25）
- DOMPurify 防 XSS：所有 innerHTML 赋值前必过 `render.ts::renderMarkdown`

---

## 添加新功能的入口

详细 cookbook 见 [doc/CONTRIBUTING.md § 2](../doc/CONTRIBUTING.md#2-添加新东西-cookbook)。速查：

| 需求 | 入口文件 |
|---|---|
| 新的消息类型（jsonl 出现新 type） | `cards/index.ts` 的 `renderMessage` switch |
| 新的工具卡渲染 | `cards/index.ts` 的 `renderBlock` 或新建 `cards/<tool>.ts` |
| 新的设置项 | `settings/panel.ts` 的 `FIELDS` 数组 + `theme.ts` 的 `TOKENS` |
| 新的全局快捷键 | `keybindings/actions.ts` 的 `ACTIONS` 加一行 + `main.ts` `dispatcher.bind(id, cb)`（**不再往 main.ts 加 keydown case**；详 CONTRIBUTING §2.5） |
| 新的 IPC 命令调用 | 直接 `invoke('cmd_name', args)`；TS 类型在调用处声明 |
| 新的 CSS token | `styles.css` 的 `:root` + `theme.ts` 的 `TOKENS` 数组 |
