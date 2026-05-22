# Changelog

本文档记录 cc-monitor 用户**可感知**的功能 / 修复 / 行为变更。
内部重构与文档调整通常不入。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。
版本遵循 [SemVer](https://semver.org/)。

---

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

## [1.4.0] — 2026-05-15（v1.5 前最后一版基准）

### 新增

- **`bring_terminal_to_front`**（Tab ↗ 按钮 / `Ctrl+\``）
  - 4 阶段 HWND 匹配：① 祖先链 PID + title 含 ai-title/项目名 ② 祖先链任意窗口 ③ 终端类进程 + title 匹配 ④ 终端类任一窗口
  - WT 单进程多窗口下落到 D 级，需用户在 PowerShell startup 设独特 console title 才能区分

- **`tool_result` 合并到 `tool_use` 折叠条**：展开同一个折叠看 args + output，output 自身嵌套二级 details

- **代码块"复制"按钮 + 语言标签**：每个 code block 顶部条

### 修复

- **PID 复用导致 4 个僵尸 Tab**：探活补回 procStart 校验（100ms 容差）
- **save_config 第二次失败**：Windows `std::fs::rename` 目标存在时失败，改用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`

### 移除

- **U2 焦点自动同步**：Win11 WT 单进程多窗口 OS API 无法区分 tab/window，整功能删除
- **SessionStart hook 路线**：改为直读 `~/.claude/sessions/<PID>.json`，零侵入
- **subagent 独立 Tab + `↳` 前缀**：嵌入到父 Task 折叠卡

---

## [1.0.0 – 1.3.x] — 2026-04 至 2026-05 早期

M1 + M2 + M3 + M4 + M5 阶段（依次）：

- 单 session MVP + watcher + 全类型 JSONL 解析 + 基础 Markdown
- 多 Tab + SessionMap + 进程探活
- 富渲染：LaTeX + 代码高亮 + tool 卡 + thinking + ai-title
- subagent Task 折叠卡 + description 关联
- 设置面板 GUI（颜色 + 字体）+ Ctrl+Tab/W/, 快捷键
- UI 全面对齐 claude.ai 视觉语言：warm gray-brown + 橙 accent + serif 正文 + user 气泡靠右
