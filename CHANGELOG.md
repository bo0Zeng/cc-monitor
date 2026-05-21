# Changelog

本文档记录 cc-monitor 用户**可感知**的功能 / 修复 / 行为变更。
内部重构与文档调整通常不入。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。
版本遵循 [SemVer](https://semver.org/)。

---

## [1.5.0] — 2026-05-20

首个发 exe 的 release。

### 新增

- **历史会话浏览器**（顶栏 📜 / `Ctrl+H` / Esc）
  - 按**工作目录分组**展示所有历史 jsonl，项目默认折叠
  - **两级懒加载**：初次打开仅读项目级元数据（< 100ms，500 项目）；展开某项目才读其下会话详情；同项目再次展开秒开（缓存）
  - 操作：⭐ 标星 · ✏️ 重命名（中文 OK）· 🙈 隐藏 · ↩️ 恢复（拉起 wt.exe / cmd 跑 `claude --resume`）· 🗑️ 物理删除（二次确认）
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
