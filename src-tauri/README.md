# 后端模块导览（`src-tauri/`）

Rust + Tauri 2。crate 名 `monitor`（lib 名 `monitor_lib`）。

本文件做"开发者打开 src-tauri/ 后第一眼看到的导航"。前端结构见 [`../src/README.md`](../src/README.md)。

## 目录结构

```
src-tauri/
├── Cargo.toml         # 依赖 + 包元数据 + release profile (opt-level=z, lto, strip)
├── tauri.conf.json    # 应用元数据 + bundle (msi / nsis) + CSP + 窗口
├── build.rs           # tauri_build::build()
├── capabilities/
│   └── default.json   # IPC 权限（core / opener / dialog）
├── icons/             # 全套图标（ico / icns / png 各尺寸）
├── gen/               # tauri-build 生成的 schemas (自动)
└── src/
    ├── main.rs        # → lib::run()
    ├── lib.rs         # Tauri Builder + 工作线程编排 + IPC 注册
    ├── paths.rs       # CLAUDE_CONFIG_DIR 三级解析
    ├── messages.rs    # JsonlRecord enum (覆盖全部 type)
    ├── parser.rs      # 按行解析 + BOM
    ├── watcher.rs     # 递归监听 ~/.claude/projects + 活跃过滤
    ├── session_map.rs # 直读 ~/.claude/sessions/<PID>.json + 进程探活 + 终端跳焦
    ├── subagent.rs    # load_subagent IPC + description 关联
    ├── event_replay.rs # F5 重放（持锁严格按序）
    ├── history.rs     # 历史浏览器：两级懒加载 + 元数据 + 删除 + resume
    ├── config.rs      # load/save_config + Windows 原子写
    └── bridge.rs      # IPC 事件常量
```

## 模块分工

| 文件 | 角色 | 关键 API |
|---|---|---|
| **lib.rs** | Tauri Builder + setup() + IPC handler 注册 | `pub fn run()` |
| **paths.rs** | 解析 `.claude` 数据目录（三级回退） | `resolve_claude_dir() / resolve_monitor_data_dir() / resolve_config_path()` |
| **messages.rs** | `JsonlRecord` enum + `ApiMessage` + `ContentBlock` | `JsonlRecord::is_displayable()` |
| **parser.rs** | 单行 JSONL → JsonlRecord | `parse_line(raw)` |
| **watcher.rs** | notify_debouncer_mini 递归监听 projects；ActiveFilter 过滤死 session | `spawn_watcher(root, active) → mpsc::UnboundedReceiver` |
| **session_map.rs** | 读 sessions/<PID>.json + Win32 进程探活 + 终端窗口匹配 | `SessionMap::load_with_changes() / is_session_active() / bring_terminal_to_front()` |
| **subagent.rs** | 父 session 的 Agent tool_use 关联 `<parent>/subagents/agent-*.jsonl` | IPC `load_subagent` |
| **event_replay.rs** | 内存 buffer + frontend-ready 时持锁完整 emit | `EventReplay::record() / replay_and_mark_ready() / forget()` |
| **history.rs** | 历史浏览器后端：两级 IPC + metadata + 物理删除 + resume | IPC `list_history_projects / list_history_sessions_in_project / read_session_jsonl / delete / update_metadata / resume` |
| **config.rs** | monitor 自己的 config.json R/W（Windows MoveFileExW 原子） | IPC `load_config / save_config` |
| **bridge.rs** | 事件 / payload 常量与 schema | `events::JSONL_LINE / SESSION_ENDED`，`JsonlLinePayload / SessionEndedPayload` |

## IPC 清单

注册位置：`lib.rs::run() → invoke_handler![...]`。前端调用方式：`invoke<T>('cmd_name', { args })`。

| 命令 | 参数 | 返回 | 调用方 |
|---|---|---|---|
| `load_config` | — | `Value` | 启动时 / 设置面板打开时 |
| `save_config` | `{ value: Value }` | `()` | 设置面板保存时 |
| `load_subagent` | `{ parentJsonlPath, description, toolUseTimestamp }` | `SubagentLoadResult` | 用户展开 Task 折叠卡 |
| `forget_session` | `{ sessionId }` | `()` | 用户关闭 archived Tab |
| `bring_terminal_to_front` | `{ sessionId }` | `()` | Tab ↗ 按钮 / `Ctrl+\`` |
| `list_history_projects` | — | `HistoryProject[]` | 历史浏览器打开 |
| `list_history_sessions_in_project` | `{ projectDir }` | `HistorySessionEntry[]` | 项目组展开 |
| `read_session_jsonl` | `{ jsonlPath }` | `JsonlLinePayload[]` | 点击历史会话进入只读视图 |
| `delete_history_session` | `{ sessionId, jsonlPath }` | `()` | 物理删除会话（二次确认后） |
| `update_history_metadata` | `{ sessionId, patch }` | `EntryMetadata` | star / 重命名 / 隐藏 |
| `resume_history_session` | `{ sessionId, cwd }` | `()` | ↩️ 按钮（拉起 wt.exe / cmd） |

## 事件

后端 → 前端（`Emitter::emit`）：

| 事件 | payload | 时机 |
|---|---|---|
| `jsonl-line` | `JsonlLinePayload` | watcher 解析到一行 + event_replay 回放 |
| `session-ended` | `SessionEndedPayload` | sessions/<PID>.json 被删（session 退出） |

前端 → 后端（`Listener::listen`）：

| 事件 | 用途 |
|---|---|
| `frontend-ready` | 触发 event_replay 完整回放历史（持锁严格按序） |

## 不变量

- **零侵入**：watcher 只读，绝不修改 `~/.claude/projects/` 或 `~/.claude/sessions/` 下的文件
- **历史浏览器的物理删除是例外**：必须用户**显式触发**（点 🗑️ + 二次确认），且仅限 `<claude_dir>/projects/**.jsonl`，由 history.rs 做路径安全校验
- **event_replay 持锁完整 emit**：替代旧的"snapshot + live"两步走，避免乱序
- **session_map.rs 探活两道关卡**：`OpenProcess + GetExitCodeProcess == STILL_ACTIVE` + `GetProcessTimes` 与 `procStart` 100ms 容差校验（防 PID 复用导致僵尸 Tab）
- **跨平台分裂**：所有 Win32 调用都在 `#[cfg(windows)]` 块；非 Windows 给降级实现或 Err 字符串。当前 v1 仅支持 Windows

## 工程坑（避雷）

- **Windows 路径大小写不敏感导致 notify 重复回放** → `watcher.rs:path_key()` 用小写归一
- **`std::fs::rename` Windows 目标存在时失败** → `config.rs:atomic_replace()` 用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
- **WT 单进程多窗口共享同一 PID**：`bring_terminal_to_front` 落到 D 级匹配时所有 session 聚焦同一窗口 → 需用户在 PowerShell startup 设独特 console title
- **`pwsh.exe` 不是 Windows 自带**（PowerShell Core 独立安装包）→ `history.rs:resume_impl()` 用 `cmd /K`（cmd.exe 永远存在）+ `CREATE_NEW_CONSOLE` flag
- **WT 默认终端无 OS API 暴露 active tab/window** → 焦点同步功能整体已移除，Tab 切换走手动点击 + `Ctrl+Tab` 快捷键

## 添加新功能入口

| 需求 | 入口文件 |
|---|---|
| 新 jsonl 记录类型 | `messages.rs:JsonlRecord` enum 加 variant |
| 新 IPC 命令 | 新建模块 `<feature>.rs` → 在 `lib.rs::run().invoke_handler![]` 注册 |
| 新事件 | `bridge.rs::events` 加常量 + payload 结构 → 在 lib.rs 适当处 emit |
| 新 Win32 调用 | 在 `Cargo.toml::[target.cfg(windows)].dependencies.windows.features` 加对应 feature；用 `#[cfg(windows)]` 包裹 |
| 改 release 打包配置 | `tauri.conf.json::bundle`；详 [`../README.md` § 生产构建 / 打包](../README.md) |
