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

   **作用范围**：本条 `MoveFileExW` 路径**仅适用于** `~/.claude/claudecode-frontend/` 下 monitor 自己产物（`config.json` / `sid-hwnd-cache.json` / `auto-launch.json` / `history-metadata.json` / `ps-registry/<PID>.json` 等）。**写用户文件**（PowerShell profile 等 monitor data dir 之外的文件）**必须**改走 `ReplaceFileW + backup + 写后校验`——理由是保留 dst 的 ACL/ADS/创建时间 + OneDrive placeholder 风险，详 [INVARIANTS.md § 4](INVARIANTS.md)。两者边界由 INVARIANT § 2（monitor data dir 永远在 `~/.claude/claudecode-frontend/`）锁定，不会漂移。
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

## 6. `logs/monitor.YYYY-MM-DD.log`（v2.0.0 起，issue #4）

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

## 8. `<claude_dir>/tasks/<sid>/<id>.json`（v2.3.0，issue #11）

Claude Code CLI 的 task tracker 持久文件。**monitor 只读不写**——本协议仅记录字段假设，方便后续校验 / CLI 更新时同步。

**位置**：`<claude_dir>/tasks/<session_id>/<id>.json`
- `<claude_dir>` = `paths::resolve_claude_dir()` 三级回退
- `<session_id>` = Claude session UUID（跟 jsonl 文件名同款）
- `<id>` = 数字字符串（CLI 用 `.highwatermark` 自增）

**附属控制文件（monitor 必须忽略）**：
- `<sid>/.lock` — CLI 写入期间的文件锁，半截 JSON 可能存在
- `<sid>/.highwatermark` — 下一个 id 的计数器，非 task 数据

**写入方**：Claude Code CLI（`TaskCreate` / `TaskUpdate` / `TaskStop` 工具）
**读取方**：monitor `tasks.rs::read_session_tasks`（变更时由 watcher 触发整目录重读）

**Schema**：

```json
{
  "id": "15",
  "subject": "#1a 前端 priority queue",
  "description": "JSONL_BATCH 按 session 分组, Phase 1 同步建 tab, Phase 2 优先 active session",
  "activeForm": "实现 priority queue",
  "status": "in_progress",
  "blocks": [],
  "blockedBy": []
}
```

**字段约定**：
- `id` 字符串型数字。monitor 用 `<digits>.json` 文件名筛 + parse 到 u64 排序，非数字命名一律跳过
- `subject` 必填，UI 主显示
- `description` / `activeForm` 可选，UI 进 hover tooltip
- `status` 已知值 `pending` / `in_progress` / `completed`，可能新增 `deleted` 等。monitor 不强类型化（用 String 容错），未知值显示为兜底 icon `•`
- `blocks` / `blockedBy` 暂未在 UI 用，保留兼容

**容错**：
- 读到半截 JSON（CLI 持 `.lock` 中途）→ 单条 catch 跳过，notify 下次 100ms debounce 重读自然修正
- `<claude_dir>/tasks/` 整个不存在（用户从没用过 task tracker）→ watcher 静默不 spawn，IPC 返空数组

**变更触发**：
- monitor `tasks.rs::spawn_task_watcher` 用 `notify-debouncer-mini` 监听 `tasks/` **递归**
- 100ms debounce 后按 sid dedup，重读整个 `tasks/<sid>/` 后通过 `task-update` 事件 emit 完整列表（**不**做 diff）

**生命周期**：跟 session 同寿；session 删除时 CLI 是否清理对应 tasks/<sid>/ 由 CLI 决定，monitor 不主动写。

---

## 9. `<claude_dir>/sessions/<PID>.json`（Claude Code 官方写，monitor 只读）

**不是** cc-monitor 的 IPC 文件——这是 Claude Code CLI 自己维护的活跃会话登记表，monitor 只读不写（INVARIANTS § 1）。在此记录是因为多个核心能力依赖它的字段契约：活跃 session 探测（`session_map.rs`）、PID 探活（§ 6 PID + procStart 双校验）、会话红绿灯（issue #23）。每个活跃会话一个 `<PID>.json`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `pid` | number | 进程 PID（= 文件名 stem） |
| `sessionId` | string | 会话 UUID（= jsonl 文件名 stem） |
| `cwd` | string | 工作目录 |
| `startedAt` | number? | 会话起始时间戳 |
| `procStart` | string? | .NET `DateTime.ToFileTime()`（FILETIME 100ns 自 1601-01-01 UTC，字符串）。**某些 /resume 启动路径不写此字段** → Option；缺失时 PID 探活退化为只看 `STILL_ACTIVE`（详 § 6 / `session_map.rs`） |
| `status` | string? | 会话状态枚举：`"busy"`（运行中 → 🟢）/ `"idle"`、`"shell"`（等输入 → 🔴）/ `"waiting"`（等用户决定 → 🟡）。**CLI 仅在状态转换时重写本文件**（信号天然稀疏）。旧版 CC 无此字段 → `null`，前端按未知处理（沿用原绿点） |
| `waitingFor` | string? | 仅 `status=="waiting"` 时有，细分原因：`"permission prompt"` / `"dialog open"` / `"input needed"` / `"worker request"` / `"sandbox request"` |
| `name` | string? | Claude 给会话起的语义名（aka aiTitle）；当前保留未用 |
| `kind` | string? | 会话类型（Batch6-F21 起双端消费）：`"interactive"` = 交互会话；`"bg"` = CC 2.1.x daemon 后台任务（`--fork-session`，另带 `jobId`）。**Batch7-F24 起 bg 门是配置门**：`showBgSessions` 开（默认）→ bg 正常算会话（建 Tab 带 ⚙ 标识 + 树状挂同 cwd 宿主后、行流出）；关 → 回 F21 行为（不建 Tab、行不流出；历史浏览器仍可看）。**缺失 = 旧 CC = 视为交互**（保守放行），本地 `session_map::scan_dir`（读启动时配置）与远端 daemon kind 门（`--with-bg` 参数）规则一字一致 |
| `jobId` | string? | 仅 `kind:"bg"` 时有，后台任务 ID（monitor 不消费，仅留档） |

**派生 IPC 事件 `session-activity`**（issue #23 红绿灯）：watcher 每次重扫/心跳后比对，仅对 `status`/`waitingFor` 发生变化（含新出现）的会话 emit `SessionActivityPayload` = `{sessionId, status, waitingFor}`（见 `bridge.rs::SessionActivityPayload`；启动快照走 `list_session_activity`，详 STATE-MATRIX）。

---

## 10. 远端 daemon wire 协议（issue #15 / #16，**流式，非文件 IPC**）

唯一的非文件 IPC：SSH 远端模式下，远端 `cc-monitor-remote` daemon 经 **SSH stdout** 把远端会话流式传回 monitor。在此集中协议契约；部署见 [REMOTE-PHASE0-DEPLOY.md](REMOTE-PHASE0-DEPLOY.md)。

### 实时流（无参数启动 daemon）

线约束：**每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`**（`serde_json` 紧凑输出把内部换行转义成 `\n` 两字符）。帧用外部 `kind` tag（snake_case）：

| `kind` | 字段 | 说明 |
|---|---|---|
| `hello` | `v, build_id, host_arch, claude_dir` | 连接建立时**首帧**发一次（握手）；monitor 据 `v` + `build_id` 做版本协商（#33：`v` 不符=不兼容、`build_id` 不符=偏旧，均经 `remote-health` 提示但不 hard-disconnect），`build_id` 单源自 daemon 源码（编译期 env 同步） |
| `line` | `session_id, path, seq, raw` | tail 到的一行原始 jsonl（`seq` = per-file 单调，口径同本地 watcher） |
| `session_added` | `sid`, `session_kind?`, `cwd?`, `name?` | 远端新会话文件出现（Batch5-F18 起 ssh_source 收到即同步透传前端 `remote-session-added {session_id, origin, kind, cwd, name}` 事件建骨架 Tab，先于该会话的任何行）。Batch7-F24（p1e）：附加 pidfile 元信息——wire 帧字段叫 `session_kind`（避开帧 tag `kind`），bridge 事件 payload 统一叫 `kind`（与本地 `list_active_sessions`/`session-started` 一致）；**additive 兼容**：None 不序列化（旧行为字节不变）、旧 monitor 忽略未知字段、旧 daemon 缺字段前端视为交互。daemon 默认不宣告 bg（F21）；monitor 仅对 **auto-deploy 确认为当前版本**的 daemon 且 `showBgSessions` 开（默认）时传 `--with-bg`（旧 daemon 会把未知参数当一次性查询→无 hello，确认不了一律降级不传）。本地对称通道：`session-started` payload 扩为 `{session_id, cwd, kind, name}`——前端无 Tab 则建骨架（中途出现的本地 bg 会话由此获得 ⚙/树状） |
| `session_removed` | `sid` | 远端会话文件消失 |
| `overflow` | `dropped: u64` | issue #32：daemon 发送通道被慢/卡的 SSH 管道反压、丢了 `dropped` 帧的哨兵信号（通道排空到能再容纳时发一次）→ monitor 经 SS-F `remote-health` 事件 toast 提示用户可能丢实时行（丢的行仍在远端 jsonl，重开会话可看完整历史） |

### 一次性历史查询（带参数启动 daemon，issue #16）

带参数 exec = 一次性查询模式，干完即退、**不进流式协议**：

- `--list-projects` → 每行 `{dirName, projectPath, sessionCount, lastActivityMs}`
- `--list-sessions <project_dir>` → 每行 `{sessionId, jsonlPath, startedAtMs, updatedAtMs, messageCountApprox, firstUserExcerpt, aiTitle, cwd}`
- `--read-session <jsonl_path>` → 原样透传该 jsonl 字节（monitor 侧走既有 `parse_line` 管线）
- `--search <query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]`（issue #28）→ 服务端在远端 CPU 扫 `projects/**/*.jsonl` 做全文搜索（避免拉整库回本地），**每命中会话一行** camelCase `SessionHits` JSON（形状严格对齐 monitor `search::SessionHits`，monitor 补 `origin` 后与本地结果合并）

错误写 stderr + 退出码 2。所有路径参数严格限制在 `<claude_dir>/projects/` 内（canonicalize 后前缀校验，拒穿越 / symlink 逃逸 / 非 jsonl）。**旧 daemon 兼容**：不认参数的旧版会照常发 `hello` 进流模式——monitor 以"首行是 hello 帧"识别旧版并提示升级（优雅降级，无版本协商）。

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
