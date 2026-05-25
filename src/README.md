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
| **events.ts** | 订阅后端 `jsonl-line` / `jsonl-batch` / `session-ended`，**批量调度让出主线程**；v2.2 (issue #12) `jsonl-batch` 包 `batch-start`/`batch-end` 哨兵让 TabManager 切 BranchFolder batch 模式 | `bindEvents({onLine, onSessionEnded, onBatchStart, onBatchEnd})` |
| **tabs.ts** | TabManager 状态机：Tab 生命周期（live / archived）+ 工具组聚合 + BranchFolder 接入 (issue #8) + v2.2 batch mode 切换（重放期 setBatchMode(true)，结束 flushPending + 切 live） | `onLine() / onBatchStart() / onBatchEnd() / archiveTab() / closeTab() / cycleActive()` |
| **stream.ts** | 单 Tab 的消息流容器，ResizeObserver 自动贴底滚动；`contentElement` 暴露真实卡片容器给 BranchFolder | `MessageStream.append() / scrollToBottom() / dispose()` |
| **branching.ts** (issue #8) | parentUuid 拓扑分析：识别 ESC 回退主线 vs 被回退分支。`computeMainBranch` 算法 = "只在 fork 点选 latest-descendant 赢家"，无 fork 即全 on-main（多 root / 单链 / /compact 不误折叠） | `computeMainBranch(records) / extractBranchRecord(rec)` |
| **branch-fold.ts** (issue #8) | DOM 重排：把连续的 off-main 卡片包到 `.branch-fold-wrap`，header「已被 ESC 回退（含 N 条）」；策略 = unwrap-then-rewrap 全量重建。v2.2 加 batch mode (`setBatchMode/flushPending`)，重放期延后到 batch 结束才算一次 mainBranch，省 O(N²) | `new BranchFolder(container).recordAdded / setRecordsAndRebuild / setBatchMode / flushPending` |
| **cards/index.ts** | renderMessage 主分发：user 气泡 / assistant 卡 / 工具组合并 / tool_result 注入到 tool_use | `renderMessage(rec, ctx) → RenderResult` |
| **cards/slash.ts** | `/` 命令紧凑卡 | `parseSlashCommand / buildSlashCommandCard` |
| **cards/compact.ts** | `/compact` 续接消息折叠 | `isCompactSummary / buildCompactSummaryCard` |
| **cards/subagent.ts** | Task/Agent tool_use 折叠卡 + 懒加载 subagent JSONL | `isAgentTool / buildAgentCard` |
| **render.ts** | marked + KaTeX + highlight.js + DOMPurify | `renderMarkdown(md) / renderPlainText(text)` |
| **config.ts** | invoke `load_config` / `save_config` 桥 | `loadConfig / saveConfig` |
| **paths.ts** | 操作 config.json 里 `claudeDir` 字段（设置面板调） | `getClaudeDirOverride / setClaudeDirOverride` |
| **theme.ts** | 把 ThemeConfig 应用到 :root CSS 变量 | `loadTheme / applyTheme / saveTheme` |
| **settings/panel.ts** | 抽屉式设置面板（数据目录 + PowerShell 集成 + 折叠：外观/诊断） | `SettingsPanel.open() / close()` |
| **settings/cc_integration.ts** | PowerShell 集成子区（profile 选项 + wrapper toggle + 5 个预设下拉） | `CcIntegrationSection.element` |
| **settings/info-icon.ts** | `?` 信息图标 portal tooltip 组件 + 路径工具 | `makeInfoIcon(text) / swapFileName(path, newName)` |
| **settings/collapsible-group.ts** (issue #7) | 通用可折叠分组，localStorage 持久 + grid-rows 平滑动画 | `new CollapsibleGroup({id, title, defaultCollapsed}).appendChild(...)` |
| **settings/diagnostics-section.ts** (v2.0.0+) | 设置面板「诊断」区：log_enabled toggle / log_level select / error_toast toggle / log 路径 / [打开 log/dir]；支持 `{ headless: true }` 给 CollapsibleGroup 复用 | `DiagnosticsSection.element` |
| **error-toast.ts** (v2.0.0+) | listen `monitor-error` 事件，右下角垂直堆叠红色 toast，点击直接打开 log 文件 | `bindErrorToast()` |
| **views/history.ts** | 历史浏览器（项目分组 + 两级懒加载 + 增删改 + v2.2 fork 树形 + 流式 session 列表） | `HistoryView.open() / handleEscape()` |
| **views/session-viewer.ts** | 只读消息查看器（点击历史条目进入）；v2.2 改用 `stream_read_session_jsonl` + Channel 边收边渲染 | `SessionViewer.load(opts) / dispose()` |
| **styles.css** | 全部样式 + token 系统 | — |

## 关键数据流

### 实时消息（活跃 session）

```
后端 jsonl-line emit
  → events.ts 入队 (BATCH_SIZE=40 / BATCH_MS=8 让出主线程)
  → tabs.ts onLine(payload)
     ├─ ensureTab(sessionId, cwd, path)
     ├─ renderMessage(record, ctx)
     │    ├─ user → buildUserCard / Slash / Compact
     │    ├─ assistant 含 text → buildAssistantCard
     │    └─ assistant 全工具 / user 全 tool_result → tool-group 折叠卡
     └─ stream.append(element) → ResizeObserver 贴底
```

### 历史浏览（懒加载）

```
点击 📜 按钮 / Ctrl+H
  → HistoryView.open()
  → invoke('list_history_projects') → HistoryProject[]  （轻量，不读内容）
  → 渲染项目组 header（默认全折叠）

点击某个项目组展开
  → invoke('list_history_sessions_in_project', { projectDir }) → HistorySessionEntry[]
  → 缓存到 sessionCache.set(projectDir, items)
  → 渲染该组内的 session 行

点击某个 session 行
  → SessionViewer.load({ jsonlPath, ... })
  → invoke('read_session_jsonl', { jsonlPath }) → JsonlLinePayload[]
  → 复用 renderMessage 渲染（与实时 Tab 同一套）
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
