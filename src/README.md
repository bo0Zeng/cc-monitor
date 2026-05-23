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
| **events.ts** | 订阅后端事件，**批量调度让出主线程**（防 replay 卡死） | `bindEvents({onLine, onSessionEnded})` |
| **tabs.ts** | TabManager 状态机：Tab 生命周期（live / archived）+ 工具组聚合 | `onLine() / archiveTab() / closeTab() / cycleActive()` |
| **stream.ts** | 单 Tab 的消息流容器，ResizeObserver 自动贴底滚动 | `MessageStream.append() / scrollToBottom() / dispose()` |
| **cards/index.ts** | renderMessage 主分发：user 气泡 / assistant 卡 / 工具组合并 / tool_result 注入到 tool_use | `renderMessage(rec, ctx) → RenderResult` |
| **cards/slash.ts** | `/` 命令紧凑卡 | `parseSlashCommand / buildSlashCommandCard` |
| **cards/compact.ts** | `/compact` 续接消息折叠 | `isCompactSummary / buildCompactSummaryCard` |
| **cards/subagent.ts** | Task/Agent tool_use 折叠卡 + 懒加载 subagent JSONL | `isAgentTool / buildAgentCard` |
| **render.ts** | marked + KaTeX + highlight.js + DOMPurify | `renderMarkdown(md) / renderPlainText(text)` |
| **config.ts** | invoke `load_config` / `save_config` 桥 | `loadConfig / saveConfig` |
| **paths.ts** | 操作 config.json 里 `claudeDir` 字段（设置面板调） | `getClaudeDirOverride / setClaudeDirOverride` |
| **theme.ts** | 把 ThemeConfig 应用到 :root CSS 变量 | `loadTheme / applyTheme / saveTheme` |
| **settings/panel.ts** | 抽屉式设置面板（数据目录 + 字体 + 颜色） | `SettingsPanel.open() / close()` |
| **views/history.ts** | 历史浏览器（项目分组 + 两级懒加载 + 增删改） | `HistoryView.open() / handleEscape()` |
| **views/session-viewer.ts** | 只读消息查看器（点击历史条目进入） | `SessionViewer.load(opts) / dispose()` |
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

## 不变量

- **renderMessage 是纯函数**：给定 record + ctx 返回 RenderResult，无副作用（除了写 ctx.toolUseElements）
- **MessageStream 一个实例对应一个 Tab**：closeTab 必须调 stream.dispose() 释放 ResizeObserver
- **innerHTML 赋值前必过 DOMPurify**：见 render.ts 的 `renderMarkdown`
- **批量 jsonl-line 事件让出主线程**：events.ts 不能改成 sync 派发（会让 replay 卡死光标）

## 添加新功能的入口

| 需求 | 入口文件 |
|---|---|
| 新的消息类型（jsonl 出现新 type） | `cards/index.ts` 的 `renderMessage` switch |
| 新的工具卡渲染 | `cards/index.ts` 的 `renderBlock` 或新建 `cards/<tool>.ts` |
| 新的设置项 | `settings/panel.ts` 的 `FIELDS` 数组 + `theme.ts` 的 `TOKENS` 或新分组 |
| 新的全局快捷键 | `main.ts` 的 `keydown` handler |
| 新的 IPC 命令调用 | 直接 `invoke('cmd_name', args)`；TS 类型在调用处声明 |
| 新的 CSS token | `styles.css` 的 `:root` + `theme.ts` 的 `TOKENS` 数组 |
