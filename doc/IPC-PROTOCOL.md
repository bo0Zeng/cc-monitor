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

**生命周期**：短暂。PS 写完后**轮询**（30ms 步，deadline **3000ms**），退出条件二选一：await 文件被 monitor 删除，**或** `ps-registry/<PID>.json` 落地且 `ps_proc_start` 指纹匹配。monitor 在线时正常几十 ms 内完事；到 deadline 仍没绑上则 PS 自删 await + 报"绑定超时"——**但存在指纹不匹配的陈旧 registry 时不告警**（`cc.ps1.tpl` 的告警条件是 `-not $bound -and -not (Test-Path $regFile)`）。（v2 之前只认"await 被删"一种信号、deadline 是 800ms —— monitor 冷启动来不及。）

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
| `name` | string? | Claude 给会话起的语义名（aka aiTitle）。**已在用**：`session_map.rs::snapshot_active` 透传下游（有测试钉住），`session_added` 帧与 bridge 事件也带它 |
| `kind` | string? | 会话类型（Batch6-F21 起双端消费）：`"interactive"` = 交互会话；`"bg"` = CC 2.1.x daemon 后台任务（`--fork-session`，另带 `jobId`）。**Batch7-F24 起 bg 门是配置门**：`showBgSessions` 开（默认）→ bg 正常算会话（建 Tab 带 ⚙ 标识 + 树状挂同 cwd 宿主后、行流出）；关 → 回 F21 行为（不建 Tab、行不流出；历史浏览器仍可看）。**缺失 = 旧 CC = 视为交互**（保守放行），本地 `session_map::scan_dir`（读启动时配置）与远端 daemon kind 门（`--with-bg` 参数）规则一字一致 |
| `jobId` | string? | 仅 `kind:"bg"` 时有，后台任务 ID（monitor 不消费，仅留档） |

### 9.1 ★ `kind` 是**排他式**契约：不在白名单里就被隐藏（E72）

**给要自己写 pidfile 的外部集成方**（aterm 等）：真实判据在 `remote-daemon-proto/src/observe/watcher.rs`，
形如 `if kind != "interactive" && !with_bg { 排除 }`。展开成矩阵：

| `kind` 的值 | 结果 |
|---|---|
| **字段缺席** | **放行**（旧版 CC 不写它，保守视为交互） |
| `"interactive"` | 放行 |
| `"bg"` | 仅当 `showBgSessions` 开（本地）/ `--with-bg`（远端）时放行 |
| **其它任意值**（`"bridge"`、`"agent"`、你新造的任何名字） | **隐藏** |

**为什么必须写在这里**：它此前只活在 daemon 的 Rust 注释里，而外部集成方最自然的直觉是
「加一个新 `kind` = 加一个新类型，monitor 顶多不认识它」—— 事实相反，**不认识就等于隐藏**。
这条误解已经真实发生过一次（2026-07-31 的跨项目问答里，**我自己第一次也答反了**，
说写 `"bridge"` 会被当成交互会话；实际是被排除）。

⇒ **想让你的会话可见，`kind` 要么不写、要么写 `"interactive"`。**

**副作用一条**：同一个 pidfile 的 `kind` 从 `"interactive"` 翻成别的值时，会走
`retire_sid_if_unreferenced(Gone)` 退休路径 —— 也就是说「改 kind」不是改标签，是**让会话消失**。

### 9.3 ★ `attachable`：新增字段，**给自己写 pidfile 的集成方**（E73，2026-08-01 定，契约冻结）

| 字段 | 类型 | 说明 |
|---|---|---|
| `attachable` | boolean? | **attach 进去对人有没有意义。** `false` ⇒ monitor 不提供 attach / `↗` 拉前 / 「杀死空 tmux」。**缺席 = `true`**（存量会话与旧 daemon 一律照旧，零迁移） |

**为什么要这个字段（`kind` 答不了）**：`kind` 把两件事压在一个轴上 ——
① 这个会话该不该在 UI 里出现（§9.1 的排他矩阵）② 它是不是「一个人坐在终端里跟它对话」。
SDK / 脚本驱动的会话正好是 **①要 ②不要**，现有字段表达不了。

**判据不是「有没有终端后端」**。那样问答不出来：这类会话**确实有** tmux、`@ccm_sid` 也设了。
决定性的事实是 **`stdin` 不接键盘**（`stdin=DEVNULL`）—— 用户敲进去的字会被脚本吃掉。
所以字段问的是「attach 进去对人有没有意义」，答案由**写 pidfile 的那一方**给，因为只有它知道。

**不写会怎样（这是本字段存在的直接原因）**：monitor 的 idle-tmux 判据是
「`@ccm_sid` 精确命中 **且** 前台命令不是 claude」。脚本驱动的会话（前台是 `python3` 之类）
**精确落在里面** ⇒ monitor 认为那是个**空壳**，于是给出「杀死会话（kill 空 tmux）」和
「就地 resume」。它以为里面没东西，实际正跑着你的脚本。

**消费侧（monitor）怎么用**：daemon 把它 additive 放上 `session_added` 帧
（`remote-daemon-proto/src/wire.rs`，最小 `BUILD_ID` = **`p1v-attachable`**），
monitor 记进一张 sid 表，用它 ① 拦掉 `↗` 并给出正确说法 ② 把这些 sid 从 idle-tmux 判定里排除。

**只认真正的布尔**：字符串 `"false"` 之类当没写（⇒ 视为可以）。宁可少一次门控，
也不要把一个拼错的值读成「不可 attach」而把功能吞掉。

### 9.2 `procStart` 参与 PID 复用检测，不只是展示（E72）

`procStart` 不是元信息，它是**判活的第二个判据**：monitor 用 `(pid, procStart)` 这一对来区分
「同一个进程还活着」与「PID 被系统复用给了另一个进程」。缺失时（某些 `/resume` 启动路径不写）
退化为只看进程是否存在 —— 那条退化路径**认不出 PID 复用**。
写 pidfile 的一方若能提供它，就应当提供。

**派生 IPC 事件 `session-activity`**（issue #23 红绿灯）：watcher 每次重扫/心跳后比对，仅对 `status`/`waitingFor` 发生变化（含新出现）的会话 emit `SessionActivityPayload` = `{sessionId, status, waitingFor}`（见 `bridge.rs::SessionActivityPayload`；启动快照走 `list_session_activity`，详 STATE-MATRIX）。

---

## 10. 远端 daemon wire 协议（issue #15 / #16，**流式，非文件 IPC**）

唯一的非文件 IPC：SSH 远端模式下，远端 `cc-monitor-remote` daemon 经 **SSH stdout** 把远端会话流式传回 monitor。在此集中协议契约；部署见 [REMOTE-PHASE0-DEPLOY.md](REMOTE-PHASE0-DEPLOY.md)。

### 实时流（流模式启动 daemon）

流模式 flag（monitor 仅对 hello 帧**声明了对应能力（`capabilities`）**的 daemon 传，见 INVARIANTS §26；F66/#58③ 起**取代**旧的「build_id 精确匹配」门控）：

| flag | 起 | 语义 |
|---|---|---|
| （无参数） | Phase 0 | 全量重放所有活跃会话历史 + 尾随（旧 monitor / 未确认 daemon 的兼容路径） |
| `--with-bg` | p1e (F24) | 放行 kind:"bg" 后台任务会话（宣告+流行，帧带元信息） |
| `--tail-only` | p1f (F25) | 不重放历史：连接时各文件 seq 计数器初始化为当前完整行数（seq=行号），只尾随新行；历史由 monitor 旁路 `--read-session` 快照拉取 |

线约束：**每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`**（`serde_json` 紧凑输出把内部换行转义成 `\n` 两字符）。帧用外部 `kind` tag（snake_case）：

| `kind` | 字段 | 说明 |
|---|---|---|
| `hello` | `v, build_id, host_arch, claude_dir, capabilities, codex_dir?, kinds?, emits?`<br>⚠ **本表的列举顺序不是线上字节序**。线上按 `wire.rs` 声明序：`claude_dir, codex_dir, kinds, capabilities, emits`。`dg3_codex_fields_serialize_when_present` 用**精确字节串**钉住它（aterm 拿来做 fixture 真值）—— 要对字节就以 `wire.rs` 为准 | 连接建立时**首帧**发一次（握手）。**三轴正交（§26/§28）**：`v`（proto 版本，只留破坏性变更、F66 **绝不 bump**，不符=不兼容）；`build_id`（**身份**，单源自 daemon 源码/编译期 env，管 staleness/重部署提示，不符=偏旧、经 `remote-health` 提示但不 hard-disconnect）；**`capabilities`（能力 token 集，加法式）——monitor 按声明发流模式 flag**（F66/#58③，`decide_stream_flags`；缺该字段=空集=最保守、不发任何 flag，§27）。**绝不用身份（build_id）匹配代理能力声明**（那正是 2026-07-09 事故根因）。**DG3（#2D，additive）`codex_dir`**：对称 `claude_dir` 的 Codex 记录根（`<codex_dir>/sessions`）；Codex 未启用 / 旧 daemon 省略。**DG3 `kinds`**：本 daemon 服务的 agent kind 集（如 `["claude","codex"]`）——消费侧**显式判支持**，而不是从「`codex_dir` 存在」反推；空/缺 = 只 claude。**daemon-08（additive）`emits`**：本 daemon **会发射的帧 kind 集**（snake_case），供消费侧**门控消费**（含该 kind → 依赖它；不含 → 回退 β/watchdog）。⚠ `emits` 与 `capabilities` **正交、别混**：`capabilities` 是**流 flag 的可剥离能力**（受 §26 死循环护栏 + `every_capability_token_is_strippable` 强制每 token 有对应 flag），`emits` 是**纯发射声明、无对应 flag、不受 §26** |
| `line` | `session_id, path, seq, raw, byte_offset` | tail 到的一行原始 jsonl（`seq` = per-file 单调，口径同本地 watcher）。**`byte_offset`**：该行**末尾**的字节偏移，语义**逐字节对齐 aterm `LineFramer.endOffset`**——计 CRLF 的 `\r`、含 `\n`、残行不计；resume 到 N ⇒ `tail -c +(N+1)`。给 offset 续拉 / 截断检测用（**`seq` 是 per-stream 序数、不是 resume 键**，别拿它续）。**只 `line` 帧带**——`turn_end` 明确不带（`daemon-09` 钉住） |
| `session_added` | `sid`, `session_kind?`, `cwd?`, `name?`, `path?`, `lines?`, `status?`, `waiting_for?`, `agent_kind?`, `liveness_confidence?`, `attachable?` | 远端新会话文件出现（Batch5-F18 起 ssh_source 收到即同步透传前端 `remote-session-added {session_id, origin, kind, cwd, name}` 事件建骨架 Tab，先于该会话的任何行）。Batch7-F24（p1e）：附加 pidfile 元信息——wire 帧字段叫 `session_kind`（避开帧 tag `kind`），bridge 事件 payload 统一叫 `kind`（与本地 `list_active_sessions`/`session-started` 一致）；**additive 兼容**：None 不序列化（旧行为字节不变）、旧 monitor 忽略未知字段、旧 daemon 缺字段前端视为交互。daemon 默认不宣告 bg（F21）；monitor 仅对 hello **声明了 `bg` 能力**的 daemon 且 `showBgSessions` 开（默认）时传 `--with-bg`（F66/#58③；旧 daemon 不声明该能力→不传，且它会把未知参数当一次性查询→无 hello，护栏「声明 ⟹ 会剥离该 flag」保成立）。本地对称通道：`session-started` payload 扩为 `{session_id, cwd, kind, name}`——前端无 Tab 则建骨架（中途出现的本地 bg 会话由此获得 ⚙/树状）。**Batch8-F25/26（p1f）**：帧再附 `path`（远端 jsonl 绝对路径）；monitor 见 daemon 声明 `tail-only` 能力后 exec 追加 `--tail-only`（Batch9 起快照换 `--read-session-tail` 尾部优先，见查询表）——daemon 不再重放历史（连接时把各文件 seq 计数器初始化为当前完整行数 L，之后新行 seq=行号），历史由 monitor 按 path 经**独立连接**跑 `--read-session` 旁路快照拉回（0..L'-1 行号编 seq、并发 ≤2、F19 priority 先拉、完就断、失败重试 1 次后 remote-health 提示）；两路 seq 同处行号空间，重叠区被 (sid,seq) 去重精确吸收。旧 daemon 不声明能力 → 不传 flag → 全量推流（=2.18.0）；session_added 无 path（会话尚无 jsonl）→ 不拉快照，后续行从 tail 全量到达。**DG3（#2D，additive）`agent_kind`**：本会话属哪个 agent——`"codex"`；Claude 会话**省略** ⇒ **缺 = claude**。**DG3 `liveness_confidence`**：判活置信度——`"heuristic"`（Codex 无 pidfile，靠 mtime/proc 启发）；Claude 走 pidfile 权威故**省略** ⇒ **缺 = authoritative**。两者都是「缺字段有确定含义」，消费侧别把缺当未知。⚠ **今天的消费方是仓外 aterm，不是 cc-monitor** —— monitor 的 `ssh_source::parse_frame` 把这两个字段（以及 `byte_offset` / `codex_dir` / `kinds` / `emits`）**整个丢掉**。缺省值碰巧等于丢弃行为，不等于 monitor 实现了默认值：真发 `agent_kind:"codex"` monitor 一样当 claude。（daemon 今天也还没产出它们 —— DG1 未接线，`codex_dir`/`kinds`/`agent_kind`/`liveness_confidence` 硬写 None/空。）|
| `session_status` | `sid`, `status?`, `waiting_for?`, `liveness_confidence?` | Batch9-F27（p1g）：会话红绿灯状态变化（daemon 对 pidfile modify 做 diff，CC 仅状态转换时重写故天然稀疏）。monitor 转发进 `SessionChange.status_changed` → `session-activity` 事件——**远端灯与本地共用前端链路**。宣告帧另带初始 `status`（连接建立灯就对）。旧 monitor 未知 kind 忽略。**DG3 `liveness_confidence`** 同 `session_added`（状态变化时带；Claude 省略 ⇒ 缺 = authoritative） |
| `session_removed` | `sid`, `cause?` | 远端会话文件消失。**S0（additive）`cause`**：`"gone"`（真没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind）/ `"superseded"`（同一个 pidfile **原地换了 sid**，即 `/branch`、`/clear`）。`gone` 是默认值且**不序列化** ⇒ 缺字段 = `gone`，旧 daemon × 新 monitor 行为一字不变。**monitor 收到 `superseded` 必须直接归档、不许再去查 tmux 快照**（那份快照对这个场景恒错——这正是「branch 之后原 tab 永久灰点」的成因）。字面量与 `ssh_source.rs` 的解析处由 `removal_cause_wire_literal_stays_in_sync` 钉住 |
| `turn_end` | `session_id`, `uuid` | 一轮对话结束（monitor 用它对齐轮次边界） |
| `tmux_session_closed` | `name` | **P5（zero-poll-liveness，additive）**：某个 tmux 会话**关闭了**——正向死亡帧。**刻意不带 sid**：`#{@ccm_sid}` 在 hook 上下文里取不到（P0 实测会拿到空 ⇒ 把活会话判灰），daemon 这边是**差分算出来的名字**，sid 由 monitor 用最新快照反查 |
| `tmux_sessions` | `raw`, `observation?` | **B2**：daemon 在远端本地跑 `tmux ls -F '<TMUX_LS_FMT>'` 的**原始 stdout**（或哨兵 `NO_TMUX`），替掉 monitor 每 8s 新建 SSH 跑 tmux ls 的刷屏轮询。**送 raw、client 解析**（照 `line` 帧哲学，复用 monitor 现有 `tmux::parse_tmux_ls`）。**P1（additive）`observation`**：`"zero_sessions"` / `"no_tmux"` / `"unobservable"`——**有会话时省略**，热路径字节与 P1 之前逐字节一致。没有它时 `raw` 的空串同时意味着「零会话」与「`tmux ls` 出错被 `\|\| true` 吞了」，两者不可分 |
| `overflow` | `dropped: u64` | issue #32：daemon 发送通道被慢/卡的 SSH 管道反压、丢了 `dropped` 帧的哨兵信号（通道排空到能再容纳时发一次）→ monitor 经 SS-F `remote-health` 事件 toast 提示用户可能丢实时行（丢的行仍在远端 jsonl，重开会话可看完整历史） |

### 一次性历史查询（带参数启动 daemon，issue #16）

带参数 exec = 一次性查询模式，干完即退、**不进流式协议**：

- `--list-projects` → 每行 `{dirName, projectPath, sessionCount, lastActivityMs}`
- `--list-sessions <project_dir>` → 每行 `{sessionId, jsonlPath, startedAtMs, updatedAtMs, messageCountApprox, firstUserExcerpt, aiTitle, cwd}`
- `--read-session <jsonl_path>` → 原样透传该 jsonl 字节（monitor 侧走既有 `parse_line` 管线）
- `--read-session-tail <jsonl_path> <N>`（Batch9-F30，p1g）→ **尾部优先**：首行 meta `{"kind":"snapshot_meta","total":T,"tail_from":F}`（可计行口径 = watcher 行号空间），随后原样输出行 [F,T)（最新 N 行）再输出 [0,F)。快照拉取用它——最新内容第一批就位、旧历史回填；monitor 按 meta 两段编 seq（前端 seq 二分插入天然支持乱序），`total` 做精确完整性对账。回填在途经 `snapshot-inflight` 事件驱动前端 batch 模式（替代纯 300ms 静默启发式，5min 防呆上限）
- `--read-session-from-offset <jsonl_path> <offset>`（daemon-02，Phase 1 offset 续拉，`observe/history_query.rs`）→ seek 到字节 `offset`（0-based）后**原样透传 [offset, EOF]**，语义**逐字节 = aterm `tail -c +(offset+1)`**。`offset` 就是客户端从 `line` 帧 `byte_offset` 持久化下来的续点（重连/断线后带上）——**别拿 `seq` 当续点**，那是 per-stream 序数。**截断/重写不在此判**：远端 size < offset 时 seek 过 EOF → 读空 → 透传空，安全无副作用；客户端另经 size 查检测后自行决策 reset（同 aterm 的 `offsetByPath`）。透传而非逐行，理由同 `--read-session`。路径守卫与 `--read-session` 同一套
- `--search <query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]`（issue #28）→ 服务端在远端 CPU 扫 `projects/**/*.jsonl` 做全文搜索（避免拉整库回本地），**每命中会话一行** camelCase `SessionHits` JSON（形状严格对齐 monitor `search::SessionHits`，monitor 补 `origin` 后与本地结果合并）
- `--usage`（F88a-remote / #52，`remote-daemon-proto/src/observe/usage_query.rs`）→ 服务端在远端聚合用量（**per-requestId 每字段 MAX**，口径对齐 monitor `usage.rs`——有 `per_request_field_max_matches_local_kou_jing` 跨轨对账测），**每会话一行** camelCase 用量行 JSON，monitor 侧 `remote_history::aggregate_remote_usage_all` fan-out 合并（各带 `origin`）。**additive 子命令、未 bump PROTO_VERSION**。

- `--list-accounts [--accts-dir <p>]`（A2 多账号，`remote-daemon-proto/src/observe/accounts_query.rs`）→ 读 cc-acct-iso 的 manifest（`$ACCTS_DIR/accounts.json`，契约 v1）。**首行** `{"kind":"accounts-meta","enabled":bool,"acctsDir","manifestPath","updatedAt","sharedStore","count","error"}`，其后每账号一行 `{name,email,configDir,isDefault,mode,exists,loggedIn}`。**"未启用多账号"是正常状态**：manifest 缺失/坏/版本不支持 → `enabled:false` + `error` 人话原因 + **exit 0**（不是错误）。`loggedIn` 仅 stat `.credentials.json` 存在性。账号库目录解析：`--accts-dir` > `~/.cc-acct-iso/config` 的 `ACCTS_DIR=`（**正则抠值，绝不 source**）> `$HOME/.claude-accts`
- `--session-accounts [--accts-dir <p>]`（A2）→ 扫 `<claude_dir>/sessions/<PID>.json` 拿 pid，读 `/proc/<pid>/environ` **只抠 `CLAUDE_CONFIG_DIR` 一个键**，反查 manifest 得账号名。每条一行 `{pid,sessionId,cwd,configDir,account,bare,alive}`。`account:null` = 查不到（**不猜**）；`bare:true` = 进程活着但没设该变量（裸起）。这是"某条**正在跑**的会话属于哪个账号"的唯一硬真相（会话 jsonl 里没有任何账号字段）
- `--account-trust <configDir> <cwd> [--accts-dir <p>]`（A2）→ 换号 resume 前的信任预检（首次用某账号进某目录，CC 会弹信任确认、会卡住自动化）。单行 `{"trusted":bool,"known":bool,"error":null}`。**安全**：`configDir` 必须逐字 ∈ manifest 的账号列表，否则 exit 2 + stderr `{"code":"unknown_config_dir",...}`——避免退化成任意文件读原语；**只回三个布尔/字符串字段，绝不回传 `.claude.json` 内容**（内含 `mcpServers` 的环境变量，可能有 API key）
- `--account-trust-zero <cwd>`（A2）→ **账号 0**（未启用多账号时那个原生身份）的信任预检，返回形状同 `--account-trust`。**为什么单开一个动词而不是给 `--account-trust` 传空 `configDir`**：账号 0 没有 config dir，而空串是被明令禁止的拼法（空值 ≠ 未设）；且它的 `.claude.json` 原生根是 `$HOME`、不在共享账号库里 ⇒ 路径来源本就不同，合并只能靠哨兵值区分，比多一个动词更易错。**不收任何文件/配置目录路径参数**：它收 `cwd`，但那只当 `projects` 里的**查表键**，`.claude.json` 的根写死 `$HOME` ⇒ 连"任意文件读"的面都没有（`account_trust_zero_takes_no_path_argument` 钉住）
- `--fork-session <args>`（G2 branch-anywhere，`remote-daemon-proto/src/control/fork_write.rs`）→ 从指定消息处分叉出一个新会话文件。**daemon 唯一的写盘入口**——其余一切子命令只读；`readonly_guard` 的写白名单按路径单独盯着 `control/fork_write.rs` 这一个文件（`doc/INVARIANTS.md` §41.6）
- `--tmux-notify <daemon_pid> <daemon_starttime>`（P4b zero-poll-liveness）→ **不是查询**，是 tmux hook 子进程走的通路：校验身份后给正在跑的 daemon 发一个信号叫它立刻重扫 tmux，**完全不碰文件系统**。两个参数缺一或非整数 ⇒ exit 2。**必须同时比对 starttime 而不只看 pid 存在**：daemon 退出后那个 pid 可能已被别的进程占用，误发信号轻则无效、重则打断无关进程（很多程序把该信号当自定义控制信号，默认处置直接终止）。身份对不上 ⇒ **静默 exit 0，不做事**

错误写 stderr + 退出码 2（`--account-trust` 用 `--resolve` 那套结构化 `{code,message}` JSON）。**读会话那一族**（`--read-session` / `--read-session-tail` / `--read-session-from-offset` / `--fork-session`）的路径参数严格限制在 `<claude_dir>/projects/` 内（canonicalize 后前缀校验，拒穿越 / symlink 逃逸 / 非 jsonl）。**账号一族不走这条**，各有各的判据：`--accts-dir <p>` 解析到 `~/.cc-acct-iso/config` 或 `$HOME/.claude-accts`；`--account-trust <configDir>` 靠「逐字 ∈ manifest」而非 projects 前缀；`--tmux-notify` 根本不碰文件系统。**旧 daemon 兼容**：不认参数的旧版会照常发 `hello` 进流模式——monitor 以"首行是 hello 帧"识别旧版并提示升级（优雅降级，无版本协商）。

### 10.1 ★ `--resolve` 的返回值里**哪些是探测出来的、哪些是派生的**（E71）

`--resolve` 读 stdin 的 `ResumeSpec`、往 stdout 写一个 `CommandPlan`。**这三个字段的可信度不一样**，
而字段名读起来一模一样 —— 消费方（已经有一个了）很容易把派生值当事实用：

| 字段 | 它到底是什么 | 能不能当事实用 |
|---|---|---|
| `command` | 调用方给的候选启动器 + `--resume <调用方给的 sid>` | **能**。这是唯一真正可信的一项 |
| `sessionName` | **纯从 sid 派生**（`cc-<sid8>` / `cx-<sid8>`），`session_name_for(sid, is_codex)` | **不能**。它**没读过任何 pidfile、没查过 tmux** —— 据它去 attach 一个**并不存在**的 tmux 会话是现实风险 |
| `capabilities` | tmux/pty 的**典型档**（硬编码的常见组合），文件头自陈「待后续 backend 探测细化」 | **不能**。它描述的是「这类后端通常支持什么」，不是「这台机器此刻支持什么」 |

**为什么记在这里**：`resolve_query.rs` 的文件头写了这件事，但**跨项目的消费方不会读我们的 Rust 源码**。
实现上也留着痕迹：`run(_claude_dir, …)` 的参数带下划线、`claude_dir` 标着
`#[allow(dead_code)] // MVP 未用（不做 pidfile 消解）` —— 也就是说它**手上有 claude_dir 却没用**。

**要判断某个 tmux 会话是否真的存在**，用 `tmux_sessions` 帧（那是真 `tmux ls`）或自己在远端跑一次，
别拿 `sessionName` 当答案。

## 11. 远端终端拉起（ccm-rbind，issue #18）——注册与拉起全链路

本地 `__ccm_bind`（§2/§3，文件 IPC + PowerShell 握手）的**远端对偶**：远端没有共享
文件系统，注册信道改走**终端窗口标题**（OSC 转义经 tmux/ssh 透传到本地），monitor
按标题扫窗口。全部代码：远端 **`shared/ccm`**（部署为 `~/.local/bin/ccm`，字节源是
`sftp.rs` 的 `CCM_CLI_SCRIPT`）—— **不是** `remote-section.ts::CCM_WRAPPER_SNIPPET`，
那个其实是 `shared/ccm-aliases.sh`，**29 行、只有 `cc`/`cch`/`cct` 三个别名**，
无任何 rbind / 标题 / poller 逻辑（`sftp.rs` 的守卫①明令该块不得含实现）+ 本地 `bind.rs::RemoteHwndCache` + `lib.rs::bring_remote_terminal_to_front`。

### 注册流程（远端 shell → 本地 HWND 缓存）

> ⚠ **不存在名叫 `__ccm_rbind` 的函数。** 本节此前把它写成入口与对外契约
> （`( __ccm_rbind; exec claude ... )`），全仓**没有任何定义** —— 实现早已搬进
> `shared/ccm`（部署为 `~/.local/bin/ccm`），且 `sftp.rs` 有测试**明令**别名块不得含它。
> 照旧文档写的外部集成方会调一个不存在的函数：bash 下只打一行 command-not-found、
> **rc=0 继续跑** ⇒ 静默不注册、↗ 永远「未绑定窗口」。入口就是 `ccm` 本身。

1. **启动**：用户经 `ccm` 起 claude。`ccm` 最后 `exec` 掉自己变身 claude，
   **exec 后 PID 不变**，所以它记的 `$$` **就是 claude 的 PID**
   （pidfile `sessions/<PID>.json` 按 PID 命名，这是支点）。
   注意是 `$$` 不是 `$BASHPID`：poller 跑在 `( … ) &` 子 shell 里，那里的 `$BASHPID` 是子 shell 的。
2. **tmux 直通**：`$TMUX` 内先 `tmux set-option set-titles on`，再
   `set-titles-string '#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}'`
   （**当前 session 级、运行时选项**，不写 tmux.conf）——否则 OSC 只落 pane title、
   到不了外层终端窗口标题（Batch7 真机排查的主断链）。
   **不是 `#T`**：`#T` 是 pane 标题，而 **claude 也在往 pane 标题写自己的状态**（转圈 + 在干什么），
   两者抢同一个位置、claude 一忙就把 marker 冲掉。真机实测（2026-07-31）：四个空闲会话
   marker 都在、唯独忙碌那个被冲成「⠐ 理解…」，点 ↗ 必弹「未绑定窗口」。
   改成由 tmux 从 `@ccm_sid` **自己合成**之后 marker 常驻，两条路不再交叉。
   `#{?@ccm_sid,…,#T}` 的 `#T` 只是 sid 尚未回填时的回退，避免产出一个空的 `ccm-rbind-`。
3. **marker 刷新**：后台 poller 每 1s 读 `sessions/<PID>.json` 的 `sessionId`。
   **sid 变化时**（首现 / `/clear` / `/resume` 换 sid）**或每 20 次循环（≈20s）自愈重打**一次，
   发 `\033]0;ccm-rbind-<sid>\007` → tmux pane title → 直通外层终端 → 经 ssh 显示层透传 →
   **本地 WT 窗口标题**；同一分支还 `tmux set-option @ccm_sid "$s"`（上面那条 format 的取值来源，
   也是 `doc/INVARIANTS.md` §30 的判据来源）。claude 退出（`kill -0` 失败）poller 自灭。
4. **扫描绑定**（`lib.rs` remote-session-emitter）：daemon `session_added` 后对该 sid
   起独立线程，**每 600ms 重试扫描一次、最多 15 次（≈9s）**（等远端 shell 起 + OSC 透传；`lib.rs` 里那句注释说明为什么比固定 4 次更稳健），
   `EnumWindows` + `GetWindowTextW` 找**标题子串含** `ccm-rbind-<sid>` 的首个可见窗口，
   命中即组 `SidHwndBinding{hwnd, owner_pid, owner_proc_start}` 存入 `RemoteHwndCache`。
   **无持久化**——monitor 重启靠重连的 session_added 重扫重绑（对比本地 §4 持久化缓存）。

### 拉起流程（backtick / ↗ → 窗口前置）

1. 前端 IPC `bring_remote_terminal_to_front(session_id)` → `spawn_blocking`（Win32 同步，
   INVARIANT §10）。
2. 查缓存；未命中 → **点击时现扫一次**（兜住：eager 扫描 4 次全错过；`/resume` 换 sid
   后 marker 已被 watcher 刷新）。
3. `verify_binding` 安全网：HWND 的 owner_pid + 进程创建时间须与绑定时一致——**句柄被
   OS 复用给无关进程时拒绝拉起**。
4. `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`；OS 拒绝抢焦点 → 返回明确错误。

### 已知边界

- 裸 `claude` 起的会话无 marker，拉起报"未绑定窗口"。
- 多 ssh 会话共用一个 WT 窗口的多 tab：标题是窗口级的，只能拉起窗口、切不到 tab。
- marker 与 claude 自设的动态标题（`⠂ 任务名`）在标题上交替——绑定一次命中即缓存，
  不受影响；扫描窗口期与交替节奏理论上可能错开（低概率），点击现扫兜底。

---

## 跨进程握手时序图（cc 集成）

```
PS (__ccm_bind)                          File System                    monitor (bind.rs)
─────────────────                        ─────────────                  ────────────────
1. 检查 ps-registry/<PID>.json
   - 存在且 ps_proc_start 匹配
     → 已注册，return（avoid title flicker）
   - 不匹配 → 继续

2. 检查 auto-launch.json
   - auto_launch_enabled && monitor 不在跑
     → Start-Process monitor.exe --background
       （不抢前台焦点；v2 起不再死等 2s）

3. 生成 marker = "ccm-bind-<PID>-<8 字符 GUID>"

4. ★ 先设 $Host.UI.RawUI.WindowTitle = marker
   （v2 竞态修复，顺序不可换 —— 见下）

5. 后写 ps-await/<PID>.json  ────────►  ps-await/<PID>.json
   .NET WriteAllText + UTF8Encoding($false)          │
   （显式无 BOM）                                     │ notify-debouncer (50ms)
                                                      ▼
                                                  6. 读 ps-await/<PID>.json
                                                     - 剥 BOM（兜老模板）
                                                     - parse JSON
                                                  7. EnumWindows
                                                     找 GetWindowTextW.contains(marker)
                                                     ★ 找不到 → 重试 ≤600ms（12 × 50ms）
                                                  8. GetWindowThreadProcessId → owner_pid
                                                     GetProcessTimes(owner) → owner_proc_start
                                                  9. 写 ps-registry/<PID>.json
                                                     ▲
6'. 轮询（每 30ms，deadline 3000ms）：◄──  ps-registry/<PID>.json 出现
    while (ps-await 存在) && 未到 deadline
      读 ps-registry：ps_proc_start 匹配 ⇒ bound，break
                                                 10. 删 ps-await/<PID>.json
                                                     │
7'. 循环退出 ◄────────────────────────────────────────┘
    - 恢复 $Host.UI.RawUI.WindowTitle = oldTitle
    - 循环外**再补查一次 registry**（吃「退出瞬间 registry 刚落地」）
    - ps-await 还在 ⇒ 自删
```

### ★ 为什么第 4 步必须在第 5 步之前（v2 竞态修复）

monitor 的 notify 在 **await 文件落地那一瞬**就 EnumWindows 找 marker。旧顺序（先写文件、后设标题）
下 **monitor 扫得越快越容易找不到窗口** —— 然后它删掉 await 走失败路径，绑定成败全凭时序运气。
v2.21 实测：**每个新 shell 的首次 `cc` 固定烧满超时**。

两侧各修了一半，缺一不可：
- **PS 侧**（`src-tauri/scripts/cc.ps1.tpl`）反转顺序 ⇒ 首次即中。
- **monitor 侧**（`bind.rs`）加 ≤600ms 重试 ⇒ 兜住**旧模板**用户和慢标题传播。
  旧模板不会自动更新，这条重试是它们唯一的活路。

### ★ 退出条件是「二选一」，不是「等 await 被删」

v2 之前 PS 只认「await 文件消失」一种信号，于是 monitor 的清理时序一变就卡。
现在**registry 落地且指纹匹配**同样可以立刻返回 —— monitor 任何清理时序下都能走通。

**典型耗时**：几十 ms（debouncer 合并 50ms + 解析 + EnumWindows + 写回）。
deadline 是 **3000ms**（v2 从 800ms 提上来，覆盖 monitor 冷启动；循环"好了就走"，正常绑定仍是几十 ms 量级）。


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
