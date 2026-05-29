# 前端模块导览（`src/`）

Vanilla TypeScript + Vite + Tauri 2 IPC。不引入框架（React/Vue 都没有）—— 这是个 ~3k 行的小应用，原生 DOM 足够。

本文件做"开发者打开 src/ 后第一眼看到的导航"。后端结构见 [`../src-tauri/README.md`](../src-tauri/README.md)。

## 入口

```
index.html  ─> /src/main.ts (defer)
                  │
                  ├─ loadTheme()       从 config.json 应用 CSS 变量
                  ├─ new TabManager()  → tabs.ts
                  ├─ new SettingsPanel() → settings/panel.ts
                  ├─ new HistoryView()   → views/history.ts
                  ├─ bindEvents()         → events.ts （订阅后端 jsonl-line / session-ended）
                  └─ emit("frontend-ready")  通知后端 replay 历史
```

## 模块分工

| 文件 | 角色 | 关键 API |
|---|---|---|
| **main.ts** | 启动 + 全局快捷键 + 错误捕获 + HMR 强制 reload | DOMContentLoaded handler |
| **events.ts** | 订阅后端 `jsonl-line` / `jsonl-batch` / `session-ended` / `task-update`，**批量调度让出主线程**；v2.2 (issue #12) `jsonl-batch` 包 `batch-start`/`batch-end` 哨兵；v2.3.1 (issue #1) 300ms grace 续期。**v2.6 B 重构**：删了 PayloadSource / onChunk callback / BatchChunkMeta；JsonlLinePayload 新增 `seq: u64` 字段（后端 watcher 给的 per-file 单调） | `bindEvents({onLine, onSessionEnded, onBatchStart, onBatchEnd, onTasksUpdate})` |
| **tabs.ts** | TabManager 状态机：Tab 生命周期（live / archived）+ BranchFolder + v2.3 TasksPanel 路由；v2.4 (issue #2) `switchTo(sid, "manual"\|"auto")` + `userActive(sid)` + 5s `manualOverrideUntil`；`applyBehavior(cfg)` 同步设置面板 toggle。**v2.6 B 重构**：删了 inPrependMode / pendingPrependFragment / pendingToolGroup / appendCardOrBuffer / onChunk / markCardUuid / feedBranchFolder；onLine 改调 renderStreamRecord 共享管线 | `onLine(payload) / onBatchStart() / onBatchEnd() / userActive() / applyBehavior() / archiveTab() / closeTab() / cycleActive()` |
| **record-timeline.ts** ⭐ v2.6 | 按 seq 排序的 TimelineEntry 数组 + DOM 挂载：`insert(entry)` binary search 找位置 → `stream.insertNode(element, anchor)`；`peekPrev(seq)` 给 tool-group 后处理用。**消除了** inPrependMode/pendingPrependFragment/chunkIndex 全部状态机。**deferMode（启动重放消抖）**：重放期插到非末尾的"视口上方"旧内容只进数组不挂 DOM，`flushDeferred()` 在 onBatchEnd 一次性批量挂回（详 INVARIANT § 21） | `new RecordTimeline(stream).insert / peekPrev / setDeferMode / flushDeferred / dispose / size` |
| **render-stream-record.ts** ⭐ v2.6 | 三 caller（TabManager / SessionViewer / Subagent）共享的渲染管线：renderMessage + markCardUuid + feedBranchFolder + tool-group 后处理合并（看 timeline 左邻居）+ userActive 检测。sink 接口抽象 caller 差异 | `renderStreamRecord(payload, ctx, sink: StreamSink)` |
| **stream.ts** | 单 Tab 的消息流容器，ResizeObserver + **守卫式 `snap()`** 自动贴底；`contentElement` 暴露真实卡片容器给 BranchFolder。`insertNode(node, anchor)` 给 RecordTimeline 按 seq 挂卡；`attachBatch(fragment, anchor)` 给 deferMode 一次性挂延后的上方内容。贴底稳定性三约束见 INVARIANT § 21（守卫 snap + overflow-anchor + 延后批量挂载） | `MessageStream.insertNode() / attachBatch() / scrollToBottom() / dispose()` |
| **branching.ts** (issue #8) | parentUuid 拓扑分析：识别 ESC 回退主线 vs 被回退分支。`computeMainBranch` 算法 = "只在 fork 点选 latest-descendant 赢家"，无 fork 即全 on-main（多 root / 单链 / /compact 不误折叠） | `computeMainBranch(records) / extractBranchRecord(rec)` |
| **branch-fold.ts** (issue #8) | DOM 重排：把连续的 off-main 卡片包到 `.branch-fold-wrap`，header「已被 ESC 回退（含 N 条）」；策略 = unwrap-then-rewrap 全量重建。v2.2 加 batch mode (`setBatchMode/flushPending`)，重放期延后到 batch 结束才算一次 mainBranch，省 O(N²)。**v2.6 起由 render-stream-record.ts 统一调用 recordAdded**（之前 tabs.ts 直接调） | `new BranchFolder(container).recordAdded / setRecordsAndRebuild / setBatchMode / flushPending` |
| **cards/index.ts** | renderMessage 主分发：user 气泡 / assistant 卡 / 工具组合并 / tool_result 注入到 tool_use。**v2.6 RenderContext.pendingToolResults 改必填 + 加 lazy 字段**（透传给 renderMarkdown 控代码块占位） | `renderMessage(rec, ctx) → RenderResult` |
| **cards/slash.ts** | `/` 命令紧凑卡 | `parseSlashCommand / buildSlashCommandCard` |
| **cards/compact.ts** | `/compact` 续接消息折叠 | `isCompactSummary / buildCompactSummaryCard` |
| **cards/subagent.ts** | Task/Agent tool_use 折叠卡 + 懒加载 subagent JSONL | `isAgentTool / buildAgentCard` |
| **render.ts** | marked + KaTeX + highlight.js + DOMPurify。**v2.6 改 opts.lazy 参数化**（替代原全局 setRenderLazyMode flag）— 同步调用栈 save/restore 模式，避免 SessionViewer / Subagent 被 tabs 的 batch 模式污染走 lazy 路径 | `renderMarkdown(md, { lazy? }) / renderPlainText(text) / enhanceCard / observeForEnhance` |
| **local-storage.ts** ⭐ v2.6 | localStorage 统一接入层：LS_KEYS 集中常量（含动态 key 工厂 `settingsCollapsed(id)` / `toolRender(toolName)`）+ safeGet/safeSet/safeGetJson/safeSetJson/enumeratePrefix 包 try/catch。INVARIANT § 14 守护 | `LS_KEYS / safeGet / safeSet / safeGetJson / safeSetJson / safeRemove / enumeratePrefix` |
| **format.ts** ⭐ v2.6 | 时间 / 字节格式化合并：消息卡用 `formatTimestampShort`（永远 hh:mm）；历史浏览器用 `formatTimestampSmart`（当天 hh:mm，跨天加日期）；`formatBytes` 统一精度 | `formatTimestampShort / formatTimestampSmart / formatBytes` |
| **config.ts** | invoke `load_config` / `save_config` 桥 | `loadConfig / saveConfig` |
| **paths.ts** | 操作 config.json 里 `claudeDir` 字段（设置面板调） | `getClaudeDirOverride / setClaudeDirOverride` |
| **behavior.ts** (v2.4 issue #2) | 操作 config.json 顶层两个行为 toggle：`autoFollowUserActive` (默认 true) + `bringMonitorToFrontOnUserActive` (默认 false)。运行时热更（不需要重启 monitor，跟 claudeDir 不同） | `getBehavior() / setBehavior(cfg)` |
| **theme.ts** | 把 ThemeConfig 应用到 :root CSS 变量 | `loadTheme / applyTheme / saveTheme` |
| **settings/panel.ts** | 抽屉式设置面板（数据目录 + 行为 + PowerShell 集成 + 折叠：外观/诊断/数据存储）。v2.4 (issue #2) 新增「行为」分组挂两个 toggle + onBehaviorChange 回调实时同步 TabManager | `new SettingsPanel({ onBehaviorChange }).open() / close()` |
| **settings/cc_integration.ts** | PowerShell 集成子区（profile 选项 + wrapper toggle + 5 个预设下拉） | `CcIntegrationSection.element` |
| **settings/info-icon.ts** | `?` 信息图标 portal tooltip 组件 + 路径工具 | `makeInfoIcon(text) / swapFileName(path, newName)` |
| **settings/collapsible-group.ts** (issue #7) | 通用可折叠分组，localStorage 持久 + grid-rows 平滑动画 | `new CollapsibleGroup({id, title, defaultCollapsed}).appendChild(...)` |
| **settings/diagnostics-section.ts** (v2.0.0+) | 设置面板「诊断」区：log_enabled toggle / log_level select / error_toast toggle / log 路径 / [打开 log/dir]；支持 `{ headless: true }` 给 CollapsibleGroup 复用 | `DiagnosticsSection.element` |
| **error-toast.ts** (v2.0.0+) | listen `monitor-error` 事件，右下角垂直堆叠红色 toast，点击直接打开 log 文件 | `bindErrorToast()` |
| **views/history.ts** | 历史浏览器（项目分组 + 两级懒加载 + 增删改 + v2.2 fork 树形 + 流式 session 列表） | `HistoryView.open() / handleEscape()` |
| **views/session-viewer.ts** | 只读消息查看器（点击历史条目进入）；v2.2 改用 `stream_read_session_jsonl` + Channel 边收边渲染 | `SessionViewer.load(opts) / dispose()` |
| **tasks-panel.ts** (v2.3.0 issue #11) | Tab stream 顶部 sticky 折叠卡：显示 Claude Code CLI 的 task 列表（`~/.claude/tasks/<sid>/`）。完整 replace 渲染（无 diff），0 task 时整 panel 隐藏。折叠状态 localStorage 全局持久 (`cc-monitor.tasks-panel.collapsed`) | `new TasksPanel().update(tasks) / fetchSessionTasks(sid)` |
| **settings/data-section.ts** (v2.3.0 issue #3 A) | 设置面板「数据存储」折叠分组：调 `get_data_paths` 拉所有持久路径 + WebView2 UserDataFolder + localStorage keys；每项配 [打开] 按钮调 opener。纯展示，无危险操作 | `new DataSection({ headless }).element / refresh()` |
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

启动重放（jsonl-batch，末块先发）走同一管线，但 timeline 处于 deferMode：插到"视口上方"的旧内容只进数组不挂 DOM，onBatchEnd 时 `flushDeferred()` 一次性 `attachBatch` 挂回（消抖，INVARIANT § 21）。

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
  → 每 100 行一 chunk 增量渲染（复用 renderMessage，与实时 Tab 同一套）
```

## 关键设计选择 + 理由

### 不引入框架（React/Vue）
~3k 行小应用，原生 DOM 够用；少 100-200KB 依赖体积；HMR 强制 full reload 简化心智模型。

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
- § 21 — 启动重放贴底消抖：守卫式 snap + 不手动补偿（靠 overflow-anchor）+ 延后批量挂载
- DOMPurify 防 XSS：所有 innerHTML 赋值前必过 `render.ts::renderMarkdown`

---

## 添加新功能的入口

详细 cookbook 见 [doc/CONTRIBUTING.md § 2](../doc/CONTRIBUTING.md#2-添加新东西-cookbook)。速查：

| 需求 | 入口文件 |
|---|---|
| 新的消息类型（jsonl 出现新 type） | `cards/index.ts` 的 `renderMessage` switch |
| 新的工具卡渲染 | `cards/index.ts` 的 `renderBlock` 或新建 `cards/<tool>.ts` |
| 新的设置项 | `settings/panel.ts` 的 `FIELDS` 数组 + `theme.ts` 的 `TOKENS` |
| 新的全局快捷键 | `main.ts` 的 `keydown` handler |
| 新的 IPC 命令调用 | 直接 `invoke('cmd_name', args)`；TS 类型在调用处声明 |
| 新的 CSS token | `styles.css` 的 `:root` + `theme.ts` 的 `TOKENS` 数组 |
