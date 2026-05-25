# 跨进程文件 IPC 协议

cc-monitor 跟外部进程（PowerShell `__ccm_bind` helper、Claude Code CLI）的所有通信都走 `~/.claude/claudecode-frontend/` 下的 JSON 文件。

本文档定义每个文件的字段、编码约束、写入方原子性语义、读取方反序列化容错策略，以及握手时序图。

不在本文档范围：
- Tauri 内部 IPC（前后端 `invoke` / `emit`）— 见 [`../src-tauri/README.md`](../src-tauri/README.md) IPC 清单
- monitor 自己的 user config — `config.json` schema 在 TS 端 [`../src/config.ts`](../src/config.ts) 定义

---

## 通用约束

所有 JSON 文件**必须**满足：

1. **UTF-8 无 BOM**。PS 5.1 `Out-File -Encoding utf8` 会写 BOM（前 3 字节 `EF BB BF`），导致 `serde_json::from_str` 失败。源头：PS 端用 `[System.IO.File]::WriteAllText(path, json, [System.Text.UTF8Encoding]::new($false))`。接收端 Rust：`raw.trim_start_matches('\u{feff}')` 兜底剥任何 BOM 再 parse。
2. **原子写**。两种实现：
   - **Rust 端**：写 `<path>.tmp` → `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` 一步替换。`std::fs::rename` 在 Windows 上 dst 存在会失败，必须用 `MoveFileExW`。详 [`config.rs::atomic_replace`](../src-tauri/src/config.rs)。
   - **PS 端**：直接 `[System.IO.File]::WriteAllText` 即可，单调用本身原子。
3. **路径必须**在 `~/.claude/claudecode-frontend/` 下。**严禁**任何路径越界（用户主目录、Claude 数据目录等）。
4. **目录不存在时自动创建**（`create_dir_all`）。

---

## 1. `config.json`

monitor 自己的设置（主题 / 字体 / claudeDir override / 诊断）。

**位置**：`~/.claude/claudecode-frontend/config.json`

**写入方**：monitor 设置面板（前端 `theme.ts` / `paths.ts` / `diagnostics-section.ts` 通过 IPC `save_config` / `set_diagnostics_config`）
**读取方**：monitor 启动时 `paths::resolve_claude_dir` + 前端启动时 `load_config` + `logging::init()` 读 `diagnostics` 子对象

**Schema**（schema 收敛在 TS 端 / Rust logging 模块；其他 Rust 代码只读写 `serde_json::Value` 不解释）：

```json
{
  "claudeDir": "C:\\Users\\you\\.claude",   // 可选；用户在设置面板 override
  "theme": {                                  // 可选；前端 theme.ts 定义的 13 个 token
    "bg": "#1f1b16",
    "text": "#d6cfc6",
    "font-base": "Inter, ...",
    "font-size-base": 14
    // ... 见 src/theme.ts TOKENS
  },
  "diagnostics": {                            // 可选；v2.0.0 起；缺省值见 src-tauri/src/logging.rs
    "log_enabled": true,                      // 写 logs/monitor.YYYY-MM-DD.log；切换需重启
    "log_level": "info",                      // trace/debug/info/warn/error/off；reload 立即生效
    "error_toast": true,                      // ERROR 级别弹右下角 toast；立即生效
    "max_files": 3                            // 保留最近 N 天 log；切换需重启
  }
}
```

**生命周期**：持久。卸载 monitor **不删**（用户元数据保留）。

**写入语义**：MoveFileExW 原子替换。失败回错给前端，不破坏旧文件。

---

## 2. `ps-await/<PID>.json`

PS 端 `__ccm_bind` 通知 monitor "我想绑定，去找标题 = marker 的窗口"。

**位置**：`~/.claude/claudecode-frontend/ps-await/<PowerShell_PID>.json`

**写入方**：PowerShell `__ccm_bind` helper（profile 里）
**读取方**：monitor `bind::BindRegistry` 的 `bind-await-watcher` 线程（notify-debouncer）

**Schema**：

```json
{
  "ps_pid": 12345,
  "marker": "ccm-bind-12345-7f3a9b2c",
  "proc_start": "133456789012345678"
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `ps_pid` | `u32` | 调用 `__ccm_bind` 的 PowerShell 进程 PID |
| `marker` | `string` | 唯一字符串，PS 同时把它写到自己的 `$Host.UI.RawUI.WindowTitle`；monitor 用 EnumWindows 查这个字符串 |
| `proc_start` | `string` | .NET `Process.StartTime.ToFileTime()`，用于二次校验 PID 不被复用 |

**生命周期**：短暂。PS 写完后**忙等** 800ms 看文件被 monitor 删除。如果 monitor 在线，正常 50-200ms 内删；超过 800ms PS 自删 + 报"绑定超时"。

**握手时序**：见下文 § 跨进程握手时序图。

---

## 3. `ps-registry/<PID>.json`

monitor 通知 PS "绑定成功，HWND = X"，同时是个**持久映射**让 monitor 后续查 (PS_PID → HWND)。

**位置**：`~/.claude/claudecode-frontend/ps-registry/<PowerShell_PID>.json`

**写入方**：monitor `bind::BindRegistry`
**读取方**：monitor `SidHwndCache::record` 在 session 新建时按 claude_pid 反查 parent_pid 然后查这里；PS 端 `__ccm_bind` 启动时也读这个看是否已注册（指纹比对）

**Schema**：

```json
{
  "ps_pid": 12345,
  "hwnd": 198342,
  "owner_pid": 8888,
  "owner_proc_start": 133456000000000000,
  "ps_proc_start": "133456789012345678",
  "title_at_bind": "ccm-bind-12345-7f3a9b2c",
  "registered_at": 1716553496789
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `ps_pid` | `u32` | PS 进程 PID |
| `hwnd` | `isize` | 找到的窗口句柄 |
| `owner_pid` | `u32` | HWND 实际拥有进程的 PID（通常是 WT / conhost / VSCode integrated terminal） |
| `owner_proc_start` | `u64` | owner 的 procStart FILETIME 数值（0 = 拿不到，不致命） |
| `ps_proc_start` | `string` | PS 自己的 procStart（.NET `Process.StartTime.ToFileTime()` 字符串，跟 SessionInfo.proc_start 同语义）|
| `title_at_bind` | `string` | 绑定瞬间 marker 字符串，供调试 |
| `registered_at` | `i64` | Unix 毫秒时间戳 |

**生命周期**：与 PS 进程同寿。PS 退出后由 monitor `bind-heartbeat` 线程每 10s 检测 PID 死亡时清理。

---

## 4. `sid-hwnd-cache.json`

session_id → HWND 持久缓存。新 session 出现时查这里复用绑定，monitor 重启不丢。

**位置**：`~/.claude/claudecode-frontend/sid-hwnd-cache.json`

**写入方**：monitor `SidHwndCache::record / forget`
**读取方**：monitor 启动恢复 + IPC `bring_terminal_to_front` 拉前时查

**Schema**：

```json
{
  "<session_id_1>": {
    "hwnd": 198342,
    "owner_pid": 8888,
    "owner_proc_start": 133456000000000000,
    "ps_pid": 12345,
    "ps_proc_start": "133456789012345678",
    "title_at_bind": "ccm-bind-12345-7f3a9b2c",
    "registered_at": 1716553496789
  },
  "<session_id_2>": { ... }
}
```

字段语义同 `ps-registry/<PID>.json`（`SidHwndBinding` struct 直接复用 `HwndEntry` 的字段，注意 `hwnd` 是 `isize`、`owner_proc_start` 是 `u64`、`registered_at` 是 `i64` Unix ms）。

**生命周期**：持久。monitor 启动加载到内存；session 退出时由 `session-changes-emitter` 线程调 `cache.forget` 删除该 sid。

**写入语义**：原子写（MoveFileExW）。

---

## 5. `auto-launch.json`

"用 cc 启动 claude 时自动开 monitor" 开关 + monitor exe 路径。

**位置**：`~/.claude/claudecode-frontend/auto-launch.json`

**写入方**：
- monitor 设置面板（toggle 时通过 IPC `cc_set_auto_launch`）
- monitor 启动时（`update_monitor_path_on_startup` 自动写 `std::env::current_exe()` 当前路径）

**读取方**：PowerShell `__ccm_bind` 启动头部读这个，如果 `auto_launch_enabled == true` 且 monitor 没在跑就 `Start-Process` 启动它

**Schema**：

```json
{
  "auto_launch_enabled": true,
  "monitor_exe_path": "D:\\Idm_download\\Programs\\cc-monitor\\cc-monitor.exe"
}
```

**生命周期**：持久。

**为什么自动写路径**：让 monitor.exe 是 portable（用户可以随意移动），下次启动自动更新最新路径，PS 端不需要硬编码。

---

## 6.5. `logs/monitor.YYYY-MM-DD.log`（v2.0.0 起，issue #4）

GUI app 诊断日志（解决 `windows_subsystem = "windows"` 无 stderr 的结构性问题）。

**位置**：`~/.claude/claudecode-frontend/logs/monitor.YYYY-MM-DD.log`

**写入方**：monitor 自身（`tracing-appender::rolling::daily` + `non_blocking` writer）
**读取方**：用户（设置面板 [打开 log 文件] → 系统默认编辑器；或手动用记事本/VSCode 打开）

**滚动规则**：按天滚动，文件名形如 `monitor.2026-05-25.log`。`max_files` 默认 3 → 保留最近 3 天的文件，老文件由 rolling appender 自动删除。

**格式**：`tracing_subscriber::fmt::Layer` 默认格式，`with_ansi(false) + with_target(true)`：
```
2026-05-25T14:23:11.234567Z  INFO monitor_lib::bind: registered ps_pid=12345 hwnd=0x1a2b3c
2026-05-25T14:23:15.987654Z  WARN monitor_lib::bind: bind: parse C:\...\ps-await\9876.json failed: ...
```

**生命周期**：持久。`log_enabled = false` 时不创建 logs 目录，已有文件不删（用户自己删）。

**编码**：UTF-8 无 BOM（appender 默认行为）。

**ERROR 级别同时 emit Tauri 事件**：自定义 `ErrorEmitterLayer` 拦 `Level::ERROR` → `emit("monitor-error", {level, target, message, timestamp})` 给前端 → 弹右下角红色 toast。限频 60s/20 条。

---

## 7. `history-metadata.json`

历史浏览器的用户元数据（star / 重命名 / 隐藏）。**与 jsonl 数据源完全分离**，绝不污染原始数据。

**位置**：`~/.claude/claudecode-frontend/history-metadata.json`

**写入方**：monitor 历史浏览器（IPC `update_history_metadata`）
**读取方**：monitor 历史浏览器（IPC `list_history_*` 时合并）

**Schema**：

```json
{
  "<session_id>": {
    "starred": true,
    "custom_title": "我的重命名",
    "hidden": false
  }
}
```

**生命周期**：持久。

---

## 跨进程握手时序图（cc 集成）

```
PS (__ccm_bind)                          File System                    monitor (bind.rs)
─────────────────                        ─────────────                  ────────────────
1. 检查 ps-registry/<PID>.json
   - 存在且 ps_proc_start 匹配
     → 已注册，返回（avoid title flicker）
   - 不匹配 → 继续

2. 检查 auto-launch.json
   - auto_launch_enabled && monitor 不在跑
     → Start-Process monitor.exe

3. 生成 marker = "ccm-bind-<PID>-<8 字符 GUID>"
4. 写 ps-await/<PID>.json     ────────►  ps-await/<PID>.json
   含 ps_pid + marker + proc_start                │
                                                  │ notify-debouncer (100ms)
5. 设 $Host.UI.RawUI.WindowTitle = marker         │
                                                  │
                                                  ▼
                                                  4. 读 ps-await/<PID>.json
                                                     - 剥 BOM
                                                     - parse JSON
                                                  5. EnumWindows
                                                     找 GetWindowTextW.contains(marker) 的窗口
                                                  6. 拿到 HWND + GetWindowThreadProcessId 拿 owner_pid
                                                  7. GetProcessTimes(owner) 拿 owner_proc_start
                                                  8. 写 ps-registry/<PID>.json
                                                     ▲
6. 忙等（每 30ms poll）：     ◄────────  ps-registry/<PID>.json 出现
   while (ps-await 存在)
     && (Get-Date) < deadline + 800ms

                                                  9. 删 ps-await/<PID>.json
                                                     │
7. 检测到 ps-await 删除  ◄────────────────────────────┘
   - 恢复 $Host.UI.RawUI.WindowTitle = oldTitle
   - 成功！

如果超时（800ms 未删）：
   - 自删 ps-await/<PID>.json
   - Write-Warning "cc-monitor: 绑定超时"
```

**典型耗时**：50-200ms（notify-debouncer 合并 100ms + 解析 + EnumWindows + 写回）。

---

## 设计选择 + 理由

### 为什么走文件 IPC 不走命名管道 / TCP
- **简单**：PS 写文件 + Rust notify 是两边都 trivial 的事
- **可追溯**：用户 / 开发者出问题时可以直接 `Get-Content` 看
- **Tauri-friendly**：notify-debouncer-mini 已经是仓库依赖，没必要为了 cc 集成再引一个 IPC 框架
- **无需 connect**：管道有连接 / 断开管理，文件是 set-and-forget
- **跨进程权限简单**：用户态读写自己 home 目录的文件不需要任何 ACL 配置

### 为什么 ps-await 用 PID 当文件名
- 自带"每个 PS 进程一个"的并发隔离 — 多个 PS 同时跑 `__ccm_bind` 不会互相覆盖
- monitor 端可以快速 `for_each` ps-await 目录知道有多少待处理握手
- PID 复用风险通过 marker uniqueness（含 GUID8）+ proc_start 校验兜底

### 为什么 marker 含 GUID 而不是只 PID
- PID 复用可能在两次 `__ccm_bind` 之间发生（PS 退出 → 新 PS 拿到同 PID → 立即跑 cc）
- GUID8 让 marker 在时间上唯一，EnumWindows 不会匹配到陈旧的另一个 PS 窗口

### 为什么 sid-hwnd-cache 持久化
- monitor 重启不丢已建立的绑定
- PS 不需要每次 monitor 重启都重新跑 `__ccm_bind`
- 失效检测在拉前时三重校验（IsWindow + owner_pid + owner_proc_start），过期条目自动清

### 为什么 auto-launch 写 monitor exe path
- 让 monitor.exe portable：用户从 D 盘搬到 C 盘也无需重设
- monitor 启动时自动更新该路径，PS 端永远拿到最新值

---

## 添加新的跨进程协议文件

如果未来要加新的文件 IPC，必须：

1. **位置**：必须在 `~/.claude/claudecode-frontend/` 下，路径白名单严格
2. **schema**：在本文档新增一节，定义所有字段 + 类型 + 默认值 + 可选性
3. **编码**：UTF-8 无 BOM，双向防御（写端无 BOM + 读端剥 BOM）
4. **原子写**：双端都用原子机制（PS `[IO.File]::WriteAllText` / Rust `MoveFileExW`）
5. **反序列化容错**：未知字段忽略（serde `#[serde(default)]` + `#[serde(other)]` enum variant）
6. **生命周期**：明确"短暂 vs 持久"，短暂的要明确超时机制
7. **更新 [`../src-tauri/README.md`](../src-tauri/README.md) 模块表 + [INVARIANTS.md](INVARIANTS.md)**
