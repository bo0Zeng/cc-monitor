# 跨进程文件 IPC 协议

cc-monitor 跟外部进程（PowerShell `__ccm_bind` helper、Claude Code CLI）的所有通信都走 `~/.claude/claudecode-frontend/` 下的 JSON 文件。

本文档定义每个文件的字段、编码约束、写入方原子性语义、读取方反序列化容错策略，以及握手时序图。

> ## ⚠ 读之前：两处会误导你的旧称
>
> 1. 🔴 **`shared/ccm` 这个文件已经不存在了** —— 那 1592 行 bash 启动器删于 `e8f9e08e`
>    （`K-R48` 第二拍④，同拍清零了 19 处 `include_str!`）。今天这条路走的是**后端的 `ccm`**
>    （Rust，`src/backend/control/ccm/`），别名文本在 `src/shared/ccm-aliases.sh`。
>    ⇒ **下文凡是提到 `shared/ccm` 的地方都是历史**，留着是为了解释「今天为什么长这样」，
>    不是现状。看到它请读成「当年那个 bash 启动器」。
> 2. **目录名**：2026-09-17 重组之后顶层**只有 `src/` 与 `tests/`**。
>    `src-tauri/` → `src/bridge/` · `remote-daemon-proto/` → `src/backend/` ·
>    `doc/` → `src/doc/` · `e2e/`·`evidence/`·`scripts/`·`hooks/` → `tests/` 下。
>    下文若出现旧名，同样按历史读。

不在本文档范围：
- Tauri 内部 IPC（前后端 `invoke` / `emit`）— 见 [`../../src/bridge/README.md`](../../src/bridge/README.md) IPC 清单
- monitor 自己的 user config — `config.json` schema 在 TS 端 [`../config.ts`](../config.ts) 定义

---

## 通用约束

所有 JSON 文件**必须**满足：

1. **UTF-8 无 BOM**。PS 5.1 `Out-File -Encoding utf8` 会写 BOM（前 3 字节 `EF BB BF`），导致 `serde_json::from_str` 失败。源头：PS 端用 `[System.IO.File]::WriteAllText(path, json, [System.Text.UTF8Encoding]::new($false))`。接收端 Rust：`raw.trim_start_matches('\u{feff}')` 兜底剥任何 BOM 再 parse。
2. **原子写**。两种实现：
   - **Rust 端**：写 `<path>.tmp` → `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` 一步替换。`std::fs::rename` 在 Windows 上 dst 存在会失败，必须用 `MoveFileExW`。详 [`config.rs::atomic_replace`](../../src/bridge/src/config.rs)。
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
  "theme": {                                  // 可选；前端 theme.ts 定义的 token（数量见下方指针）
    "bg": "#1f1b16",
    "text": "#d6cfc6",
    "font-base": "Inter, ...",
    "font-size-base": 14
    // ... 见 src/theme.ts TOKENS
  },
  "diagnostics": {                            // 可选；v2.0.0 起；缺省值见 src/bridge/src/logging.rs
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
**读取方**：〔RM1b · 第四波〕那台机器的后端 `observe/tasks_query.rs::session_task_lines`（帧命令 `tasks-list`，本机与远端同一条路）；本机另由 monitor 的 watcher（`tasks.rs::spawn_task_watcher`）在变更时经本机后端重读那个 sid

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
| `kind` | string? | 会话类型（Batch6-F21 起双端消费）：`"interactive"` = 交互会话；`"bg"` = CC 2.1.x daemon 后台任务（`--fork-session`，另带 `jobId`）。**Batch7-F24 起 bg 门是配置门**：`showBgSessions` 开（默认）→ bg 正常算会话（建 Tab 带 ⚙ 标识、行流出；〔BG1 · V125〕与普通 Tab 平铺，不再挂同 cwd 宿主后）；关 → 回 F21 行为（不建 Tab、行不流出；历史浏览器仍可看）。**缺失 = 旧 CC = 视为交互**（保守放行），本地 `session_map::scan_dir`（读启动时配置）与远端后端 kind 门（`--with-bg` 参数）规则一字一致 |
| `jobId` | string? | 仅 `kind:"bg"` 时有，后台任务 ID（monitor 不消费，仅留档） |

### 9.1 ★ `kind` 是**排他式**契约：不在白名单里就被隐藏（E72）

**给要自己写 pidfile 的外部集成方**（aterm 等）：真实判据在 `src/backend/observe/watcher.rs`，
形如 `if kind != "interactive" && !with_bg { 排除 }`。展开成矩阵：

| `kind` 的值 | 结果 |
|---|---|
| **字段缺席** | **放行**（旧版 CC 不写它，保守视为交互） |
| `"interactive"` | 放行 |
| `"bg"` | 仅当 `showBgSessions` 开（本地）/ `--with-bg`（远端）时放行 |
| **其它任意值**（`"bridge"`、`"agent"`、你新造的任何名字） | **隐藏** |

**为什么必须写在这里**：它此前只活在后端的 Rust 注释里，而外部集成方最自然的直觉是
「加一个新 `kind` = 加一个新类型，monitor 顶多不认识它」—— 事实相反，**不认识就等于隐藏**。
这条误解已经真实发生过一次（2026-07-31 的跨项目问答里，**我自己第一次也答反了**，
说写 `"bridge"` 会被当成交互会话；实际是被排除）。

⇒ **想让你的会话可见，`kind` 要么不写、要么写 `"interactive"`。**

**副作用一条**：同一个 pidfile 的 `kind` 从 `"interactive"` 翻成别的值时，会走
`retire_sid_if_unreferenced(Gone)` 退休路径 —— 也就是说「改 kind」不是改标签，是**让会话消失**。

### 9.3 ★ `attachable`：新增字段，**给自己写 pidfile 的集成方**（E73，2026-08-01 定，契约冻结）

| 字段 | 类型 | 说明 |
|---|---|---|
| `attachable` | boolean? | **attach 进去对人有没有意义。** `false` ⇒ monitor 不提供 attach / `↗` 拉前 / 「杀死空 tmux」。**缺席 = `true`**（存量会话与旧后端一律照旧，零迁移） |

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

**消费侧（monitor）怎么用**：后端把它 additive 放上 `session_added` 帧
（`src/backend/wire.rs`，最小 `BUILD_ID` = **`p1v-attachable`**），
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

## 10. 远端后端 wire 协议（issue #15 / #16，**流式，非文件 IPC**）

唯一的非文件 IPC：SSH 远端模式下，远端 `cc-monitor-backend` 后端经 **SSH stdout** 把远端会话流式传回 monitor。在此集中协议契约；部署见 [REMOTE-PHASE0-DEPLOY.md](REMOTE-PHASE0-DEPLOY.md)。

### 实时流（流模式启动后端）

流模式 flag（monitor 仅对 hello 帧**声明了对应能力（`capabilities`）**的后端传，见 INVARIANTS §26；F66/#58③ 起**取代**旧的「build_id 精确匹配」门控）：

| flag | 起 | 语义 |
|---|---|---|
| （无参数） | Phase 0 | 全量重放所有活跃会话历史 + 尾随（旧 monitor / 未确认后端的兼容路径） |
| `--with-bg` | p1e (F24) | 放行 kind:"bg" 后台任务会话（宣告+流行，帧带元信息） |
| `--tail-only` | p1f (F25) | 不重放历史：连接时各文件 seq 计数器初始化为当前完整行数（seq=行号），只尾随新行；历史由 monitor 旁路 `--read-session` 快照拉取 |
| `--with-rbind-token` | 〔`设计/80 §8.7` 步 2〕 | **索要启动期令牌**：`session_added` 帧附 `rbind_token`（见下面帧表那一格）。**默认关**，因为令牌是敏感数据（`设计/80 §8.6 ③`）——没发这条 flag 的客户端收到的 `session_added` 字节与本字段加进来之前**一字不差**。能力 token 是 `rbind-token`；老后端不声明它 ⇒ monitor **不发**这条 flag、**诚实降级**回今天的 `@ccm_sid` 标题路，不许假装有 |

线约束：**每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`**（`serde_json` 紧凑输出把内部换行转义成 `\n` 两字符）。帧用外部 `kind` tag（snake_case）：

| `kind` | 字段 | 说明 |
|---|---|---|
| `hello` | `v, build_id, host_arch, claude_dir, capabilities, homes?, emits?, commands?, unavailable?`<br>⚠ **本表的列举顺序不是线上字节序**。线上按 `wire.rs` 声明序：`v, build_id, host_arch, claude_dir, homes, capabilities, emits, commands, unavailable`。`dg3_codex_fields_serialize_when_present` 用**精确字节串**钉住它（aterm 拿来做 fixture 真值）—— 要对字节就以 `wire.rs` 为准。⚠ 那个测试名里的 `codex_fields` 是历史名：`S4` 把它钉的两个字段换成了 `homes`，**名字刻意没改**（本文档与仓外 aterm 都按这个名字引用它当 fixture 真值） | 连接建立时**首帧**发一次（握手）。**三轴正交（§26/§28）**：`v`（proto 版本，只留破坏性变更、F66 **绝不 bump**，不符=不兼容）；`build_id`（**身份**，单源自后端源码/编译期 env，管 staleness/重部署提示，不符=偏旧、经 `remote-health` 提示但不 hard-disconnect）；**`capabilities`（能力 token 集，加法式）——monitor 按声明发流模式 flag**（F66/#58③，`decide_stream_flags`；缺该字段=空集=最保守、不发任何 flag，§27）。**绝不用身份（build_id）匹配代理能力声明**（那正是 2026-07-09 事故根因）。**backend-split `S4`（additive）`homes`**：本机上**各 agent 的 home 目录**表 —— `[{agent_kind, path}]`，如 `[{"agent_kind":"claude","path":"/home/u/.claude"},{"agent_kind":"codex","path":"/home/u/.codex"}]`。它一次替掉了 DG3 那两个字段：`codex_dir`（并列的 `<名>_dir`）与 `kinds`（服务的 agent 集 —— 现在由本表的 `agent_kind` 直接读出，不必并列第二个来源）。**为什么换**：`backend-split` 的 `D3` 逐字裁定「agent 维度只许出现在**值**里（`agent_kind`），不许出现在**字段名**里」——并列 `<名>_dir` 那条路的终点是 hello 里五个并列的目录字段，而客户端要靠 `if/else` 猜哪个有值。⇒ 接第三个 agent 是**多一个元素**，不是多一个字段。**为什么可以直接换而不是 bump**：`codex_dir`/`kinds` **从来没上过线**（后端一直硬写 `None`/空 ⇒ `skip_serializing_if` 省略），换掉对任何已部署的消费方都是零影响。**消费侧口径**：空/缺 = 这台后端没声明任何 home ⇒ **回退 `claude_dir`**（monitor 的 `claude_home_from_hello` 就是这条，`parses_hello_homes_and_falls_back_to_claude_dir` 钉住）；坏项（元素非对象 / 缺 `agent_kind` 或 `path` / 类型不对）**逐项丢掉**，不让整帧变 garbage。**今天恒空**——`main.rs` 硬写 `Vec::new()`（`production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen` 钉住）⇒ **hello 帧对 Claude 的线上字节与 `S4` 之前逐字节相同**。⚠ **`backend-split S5`（08-14）订正了"为什么恒空"**：不再是"发现做不出来"——后端已经**有能力**答出本机看得见哪些 agent（`agents::visible_homes()`，判准 = **该 agent 的 home 目录存在**；`the_backend_can_already_discover_homes_it_just_does_not_send_them` 钉住这半），**恒空是一次刻意的排期决定**：填 = 一次跨仓契约变更（aterm 的 fixture 按精确字节对），⇒ 留成一次**纯发布决策**。真填那天要**同轮**做三件事：① 改 `main.rs` 那一行；② 更新 `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent` 的期望串；③ **bump `BUILD_ID`**（那天线上字节真的变了，已部署的远端得被判 stale 重装）。⚠ 与"会话级发现"（DG1：有哪些会话、活没活）**别混**：那一格仍未接线。**`claude_dir` 为什么留着**：它是 hello 里**今天真在线上、且有仓外消费方**（aterm，契约冻结 2026-07-18）的那个目录字段，改名是破坏性变更 ⇒ 走 additive 迁移，原地冻结；解锁条件（monitor 与 aterm 都改读 `homes`）登记在后端的 `agent_boundary_guard::FROZEN_COMPAT`，monitor 那半已经做完。**backend-08（additive）`emits`**：本后端 **需要消费侧门控的帧 kind 集**（snake_case）——含该 kind → 依赖它；不含 → 回退 β/watchdog。⚠〔audit-0805 F18〕**这里原本写的是「会发射的帧 kind 集」，那与它的值对不上**：`EMITS` 8 项**不含** `hello`/`reply`/`cancelled`，而这三个后端 **确实会发**。按字面读它是错的；按意图读它是「门控用」帧集 —— 握手与应答不需要门控（`hello` 是首帧、必然收到；`reply`/`cancelled` 只在你发过命令之后才来，由 `commands` 那一轴管）。**这三个缺席者今天由 `emits_is_a_subset_of_frame_kinds_with_named_exemptions` 逐个点名钉住**，新增帧种漏进 `EMITS` 会红。⚠ `emits` 与 `capabilities` **正交、别混**：`capabilities` 是**流 flag 的可剥离能力**（受 §26 死循环护栏 + `every_capability_token_is_strippable` 强制每 token 有对应 flag），`emits` 是**纯发射声明、无对应 flag、不受 §26**。**U6b-2（additive）`commands`**：本后端 **接受的入方向命令集**（见下「入方向」小节）。能力协商此前只有出方向那一半（`capabilities` 说「我认识哪些流 flag」）；客户端还得知道**发什么过去有人接**，否则只能试错。**空/缺 = 这个后端不读 stdin**（U6b-1 之前的所有版本），别发命令。**`K-P4`（09-04，additive）`unavailable`**：本后端 **接得下、但在这台机器上做不到**的命令及原因 —— `[{command, code}]`，如 `[{"command":"kill","code":"no_tmux"}]`。**它买的是「事前」那一半**：`commands` 说的是「我**接**这条命令」，不是「我**做得到**这件事」；差额今天只有**调用之后**才知道（Windows 上没有 tmux，握手帧照样宣称接 `kill`/`launch`，前端照样画按钮，点了才收到 `no_tmux`）。用户 09-04 逐字裁「**事前协商是要的**」。**为什么是第四条面而不是塞进前三条**：`capabilities` 受 §26 死循环护栏 + `every_capability_token_is_strippable` 约束（每个 token 必须有一条能被 `split_stream_flags` 剥掉的流 flag，而「做得到 kill」没有），且它的默认方向相反（缺=最小能力集，往下降级安全；本字段缺=**没有把握**，往下降级会让能用的功能消失）；`emits` 的取值空间是**帧 kind**；`commands` 的取值空间是**命令名**，而且把做不到的从里面摘掉是错的（客户端 `accepts()` 会直接拒发，连「点了告诉你为什么」这条兜底路都没了，`COMMANDS`/`REGISTRY` 双向相等那条判据也要被迫按平台分叉）。⇒ 本字段的键是 **(命令名 × 命令级 code)** 这个**对**，前三条面没有一条是这个形状。**消费侧口径三句，缺一句就会读错**：① **空/缺 = 这台后端没有任何「做不到」的把握**，不是「全都做得到」—— 客户端照今天的样子办（照发、点了看 code），旧后端天然落这一格；② 列出来的那条 = 别画那个按钮（或画成灰的，配 `code` 那句人话 —— 〔C4e〕那张翻译表今天住界面 `src/tmux-control.ts`（文案表 `tmuxControl.*`），此前是 monitor 的三个发送端各一份）；③ 🔴 **它是提示，不是闸门**：后端自己**绝不**拿这张表拒命令（读数是握手那一刻的，之后世界会变；真去拒 = 把一份会过期的读数变成一次真停机），客户端硬发照样走真路，成不成由 `no_tmux` 那条老路回答。**判准与真调用同源**：`tmux` 在不在 `PATH` 上（`control/kill.rs` 等三处走 `Command::new("tmux")`，unix 上就是 `execvp` 的 `PATH` 查找）；表本身**从 `inbound::REGISTRY` 的 `codes` 派生**（谁登记了 `no_tmux` 谁就依赖 tmux），不是手写的第二份真相。**「判不出来」不倒向「做不到」**：`PATH` 没设、或**非 unix**（那儿 `CreateProcess` 还看进程自身目录与当前目录，且真装了叫 `tmux.exe`）⇒ **不列进表**，退回今天的行为 —— 把「不知道」压成「做不到」会让能用的功能从界面上消失，而那种消失没有回音。⚠ 因此**本字段今天在 Windows 上恒为空**，解锁要一次真 Windows 读数。**今天恒空**——`main.rs` 硬写 `Vec::new()`（`production_hello_leaves_unavailable_empty_so_the_wire_bytes_stay_frozen` 钉住）⇒ **hello 帧的线上字节逐字节不变**；而「不是没能力」由 `the_answer_is_a_function_of_the_machine_not_of_the_build` 钉另一半（同 `homes` 的 `S5` 口径：**能填不真填**，填 = 一次跨仓契约变更，本机没有 aterm 仓验不了它的运行时 ⇒ 留成一次纯发布决策）。真填那天要**同轮**做三件事：① 换 `main.rs` 那一行；② 更新 `hello_unavailable_is_additive_present_and_absent` 的期望串；③ **bump `BUILD_ID`**。🔴 **真填之前，这一格买到的是形状 + 一条能验的填法，不是「界面已经不画死按钮了」** |
| `line` | `session_id, path, seq, raw, byte_offset` | tail 到的一行原始 jsonl（`seq` = per-file 单调，口径同本地 watcher）。**`byte_offset`**：该行**末尾**的字节偏移，语义**逐字节对齐 aterm `LineFramer.endOffset`**——计 CRLF 的 `\r`、含 `\n`、残行不计；resume 到 N ⇒ `tail -c +(N+1)`。给 offset 续拉 / 截断检测用（**`seq` 是 per-stream 序数、不是 resume 键**，别拿它续）。**只 `line` 帧带**——`turn_end` 明确不带（`backend-09` 钉住） |
| `session_added` | `sid`, `session_kind?`, `cwd?`, `name?`, `path?`, `lines?`, `status?`, `waiting_for?`, `agent_kind?`, `liveness_confidence?`, `attachable?`, `rbind_token?`, `container?` | 远端新会话文件出现（Batch5-F18 起 ssh_source 收到即同步透传前端 `remote-session-added {session_id, origin, kind, cwd, name}` 事件建骨架 Tab，先于该会话的任何行）。Batch7-F24（p1e）：附加 pidfile 元信息——wire 帧字段叫 `session_kind`（避开帧 tag `kind`），bridge 事件 payload 统一叫 `kind`（与本地 `list_active_sessions`/`session-started` 一致）；**additive 兼容**：None 不序列化（旧行为字节不变）、旧 monitor 忽略未知字段、旧后端缺字段前端视为交互。后端默认不宣告 bg（F21）；monitor 仅对 hello **声明了 `bg` 能力**的后端且 `showBgSessions` 开（默认）时传 `--with-bg`（F66/#58③；旧后端不声明该能力→不传，且它会把未知参数当一次性查询→无 hello，护栏「声明 ⟹ 会剥离该 flag」保成立）。本地对称通道：`session-started` payload 扩为 `{session_id, cwd, kind, name}`——前端无 Tab 则建骨架（中途出现的本地 bg 会话由此获得 ⚙/树状）。**Batch8-F25/26（p1f）**：帧再附 `path`（远端 jsonl 绝对路径）；monitor 见后端声明 `tail-only` 能力后 exec 追加 `--tail-only`（Batch9 起快照换 `--read-session-tail` 尾部优先，见查询表）——后端不再重放历史（连接时把各文件 seq 计数器初始化为当前完整行数 L，之后新行 seq=行号），历史由 monitor 按 path 经**独立连接**跑 `--read-session` 旁路快照拉回（0..L'-1 行号编 seq、并发 ≤2、F19 priority 先拉、完就断、失败重试 1 次后 remote-health 提示）；两路 seq 同处行号空间，重叠区被 (sid,seq) 去重精确吸收。旧后端不声明能力 → 不传 flag → 全量推流（=2.18.0）；session_added 无 path（会话尚无 jsonl）→ 不拉快照，后续行从 tail 全量到达。**DG3（#2D，additive）`agent_kind`**：本会话属哪个 agent——`"codex"`；Claude 会话**省略** ⇒ **缺 = claude**。**DG3 `liveness_confidence`**：判活置信度——`"heuristic"`（Codex 无 pidfile，靠 mtime/proc 启发）；Claude 走 pidfile 权威故**省略** ⇒ **缺 = authoritative**。两者都是「缺字段有确定含义」，消费侧别把缺当未知。⚠ **今天的消费方是仓外 aterm，不是 cc-monitor** —— monitor 的 `ssh_source::parse_frame` 把这两个字段（以及 `byte_offset` / `emits`）**整个丢掉**。缺省值碰巧等于丢弃行为，不等于 monitor 实现了默认值：真发 `agent_kind:"codex"` monitor 一样当 claude。（后端今天也还没产出它们 —— DG1 未接线，`homes`/`agent_kind`/`liveness_confidence` 硬写空/None。）⚠ `S4` 起 **`hello.homes` 是个例外：monitor 真的解析它了**（`claude_home_from_hello`，优先 `homes`、回退 `claude_dir`）——别把它算进"整个丢掉"那一族。<br>★★ **〔`设计/80 §8.7` 步 2，09-22，additive〕`rbind_token`：**这条会话的**启动期令牌**（环境变量 `CCM_RBIND_TOKEN`，形状 `[0-9a-f]{32}`）。**它是干什么的**：「↗ 拉前终端」需要的全部东西是一个映射 `(sid) → (本地 HWND)`，而今天那个映射靠 tmux 会话级 option `@ccm_sid` ＋ `set-titles-string` 合成的窗口标题，**跳五次、无回执**。`设计/80 §8.1` 的判断逐字：「tmux 不是在做**发现身份**，是在做**把身份广播到本地**」—— 而广播这件事本协议就是一条正经的、有分帧的、双向的通道。⇒ 起会话的那一方注一个随机令牌，**本地**用它绑 HWND，**远端**后端从 `/proc/<pid>/environ` 读出来经本字段报回，↗ 做一次 join。读侧住 `control::identity_tag::rbind_token_of`（那份头注是这条路的论证正文）。<br>**消费侧口径三句，缺一句就会读错**：① **缺席 ≠ 「这台后端不报令牌」**。这两件事由**握手**分开：hello 的 `capabilities` 含 `rbind-token` ⇒ 这台后端报得出；不含（老后端）⇒ **诚实降级**回今天的标题路，**不能假装有**。② 声明了能力、客户端也发了 `--with-rbind-token`，而本字段仍然缺席 ⇒ 「**这条会话真的没有令牌**」= 它不是 monitor 起的（用户自己裸 `ssh` 进去敲 `claude` 那一档，`§8.6 ①`）。这一句就是 `§8.5 ②` 要的那个布尔 —— 归因从「四档猜」收成一句准确的话，**不需要往远端打 RPC 去猜**。③ 🔴 **它不承载任何权限语义**（`§8.6 ③` 逐字）：只是一个不可猜的关联 id。拿到它顶多能让某人的 ↗ 拉错窗口，**不能越权**。别拿它当鉴权材料。<br>**读得不陈旧的理由**：那个值属于**这个进程自己**（起它时注进环境、`exec` 原样继承）—— 与 `identity_tag` 拿 `TMUX_PANE` 那条是同一条自指性质。**零新节拍**：读它的那一刻就是 `sessions/` inotify 看到 `<PID>.json` 的那一刻（pid 与 sid 同时在手），没有新循环、新通道、新平台原语。<br>**形状 fail closed**：不 `trim`、不认大写、长度必须恰好 32 —— 任何偏离一律当**没有**（而不是当「大概是它」）：`§8.5 ②` 那个布尔只有在「有 = 形状确定对」时才说得准。<br>〔订正 · 令牌步 3 · p2o〕**已到 monitor**：步 1（启动命令注这个变量）与步 3（铸币口 ＋ 本地半认这个 marker）已落，monitor 协商到 `rbind-token` 时发 `--with-rbind-token`，`ssh_source::parse_frame` 读出本字段（形状 fail closed）。⚠ 仍**不是**「↗ 已经不依赖 tmux 了」：本地表今天还没有生产写入方往 `ps-await` 写带令牌的 marker，消费点也还不改分派（步 4）。别把这两句读成一句。<br>★ **〔U4b · 第四波，additive〕`container`：这条活着的会话住在什么容器里** —— `"tmux"` / `"none"`，**判不了就缺席**（缺席 = 不知道，**不是** `none`）。`设计/30 §3.5.6`：可恢复性由容器类型决定（在 tmux 里的，claude 退了终端还在 ⇒ 可重连；不在的只能 resume），而活着时这一格此前没人报（`第四波记录/U4.md §0.1` G3）。判定住 `control::identity_tag::Outcome::container`，喂它的是 `process_session_added` 里打标（`@ccm_sid`）那一次探测的结局 —— **零新进程、零新节拍**：`Tagged` / `AlreadyCurrent` ⇒ `tmux`；环境读得到、`TMUX_PANE` 没设 ⇒ `none`；环境读不到（exec 窗口 / 僵尸 / 非 Linux）· pane id 形状不对 · 默认 socket 上的 tmux 不认那个 pane（私有 `-S`）· sid 形状不对 · tmux 报错 ⇒ 缺席。monitor：`ssh_source::parse_frame` 读它（未知取值当缺席），经 `session_facts` 发前端 `session-container` 事件（本机那条流同一个口）。 |
| `session_status` | `sid`, `status?`, `waiting_for?`, `liveness_confidence?` | Batch9-F27（p1g）：会话红绿灯状态变化（后端对 pidfile modify 做 diff，CC 仅状态转换时重写故天然稀疏）。monitor 转发进 `SessionChange.status_changed` → `session-activity` 事件——**远端灯与本地共用前端链路**。宣告帧另带初始 `status`（连接建立灯就对）。旧 monitor 未知 kind 忽略。**DG3 `liveness_confidence`** 同 `session_added`（状态变化时带；Claude 省略 ⇒ 缺 = authoritative） |
| `session_removed` | `sid`, `cause?` | 远端会话文件消失。**S0（additive）`cause`**：`"gone"`（真没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind）/ `"superseded"`（同一个 pidfile **原地换了 sid**，即 `/branch`、`/clear`）。`gone` 是默认值且**不序列化** ⇒ 缺字段 = `gone`，旧后端 × 新 monitor 行为一字不变。**monitor 收到 `superseded` 必须直接归档、不许再去查 tmux 快照**（那份快照对这个场景恒错——这正是「branch 之后原 tab 永久灰点」的成因）。字面量与 `ssh_source.rs` 的解析处由 `removal_cause_wire_literal_stays_in_sync` 钉住 |
| `turn_end` | `session_id`, `uuid` | 一轮对话结束（monitor 用它对齐轮次边界） |
| `tmux_session_closed` | `name` | **P5（zero-poll-liveness，additive）**：某个 tmux 会话**关闭了**——正向死亡帧。**刻意不带 sid**：`#{@ccm_sid}` 在 hook 上下文里取不到（P0 实测会拿到空 ⇒ 把活会话判灰），后端这边是**差分算出来的名字**，sid 由 monitor 用最新快照反查 |
| `tmux_sessions` | `raw`, `observation?` | **B2**：后端在远端本地跑 `tmux ls -F '<TMUX_LS_FMT>'` 的**原始 stdout**（或哨兵 `NO_TMUX`），替掉 monitor 每 8s 新建 SSH 跑 tmux ls 的刷屏轮询。**送 raw、client 解析**（照 `line` 帧哲学，复用 monitor 现有 `tmux::parse_tmux_ls`）。**P1（additive）`observation`**：`"zero_sessions"` / `"no_tmux"` / `"unobservable"`——**有会话时省略**，热路径字节与 P1 之前逐字节一致。没有它时 `raw` 的空串同时意味着「零会话」与「`tmux ls` 出错被 `\|\| true` 吞了」，两者不可分 |
| `overflow` | `dropped: u64`, `lost?`, `lost_truncated?` | issue #32：后端发送通道被慢/卡的 SSH 管道反压、丢了 `dropped` 帧的哨兵信号（通道排空到能再容纳时发一次）→ monitor 经 SS-F `remote-health` 事件 toast 提示用户可能丢实时行。<br>★ **`lost`（audit-0805 F03，additive）**：`[{kind, subject?}]` —— 那批丢帧里**不可恢复**的那些的身份。⚠ 下面 `#入方向` 那句「出方向丢一帧**可恢复**（行还在远端 jsonl 里）」**只对内容帧成立**：`line`/`turn_end`/`tmux_sessions` 丢了别处还有，而 `session_added`/`session_removed`/`tmux_session_closed`/`session_status` 是**一次差分的结果、别处不存在** —— 只知道「丢了 N 条」是没法重同步的。⇒ 本字段给那些帧带上 `kind` 与 `subject`（sid / tmux 会话名），客户端据此**精确重取那几个主体**。空集时**不序列化**（旧客户端看到的字节与从前一字不差）。<br>★ **`lost_truncated`（additive，bool）**：身份表**有界**（后端侧 `LOST_IDENTITY_CAP = 64`）。超出的那些**仍计入 `dropped`**，只是不再留身份并置本位 —— **不是静默截断**。置位 = 「这份清单不全，理性做法是整体重取快照」。为 `false` 时不序列化 |
| `reply` | `id`, `ok`, `data?`, `code?`, `message?` | **U6b-1**：**入方向命令的应答**。它刻意**复用出方向的 `kind` tag 空间**（不另开一条流），所以它在本表里有一行 —— 而它的完整语义（信封、`id` 不透明性、超时后登记谁摘、逐命令的 `data` 形状与错误码）住在下面的「入方向」小节。⚠ **本行只是清册登记**，不重复那一节的内容（同一份契约不许两处各写一份）。⚠ 〔F06c 补〕它此前**只活在那一节的示例里、本表没有它的行** —— 仓外 aterm 的 KDoc 里那个错的帧数就是数本表数出来的 |
| `cancelled` | `id` | **U6b-1**：某条入方向命令**被取消了**（`id` = 被取消的那条）。同 `reply`：复用 tag 空间、完整语义在「入方向」小节（含「不可取消」时为什么回 `reply{ok:false,code:"not_cancellable"}` 而不是本帧）。⚠ 〔F06c 补〕同上，此前本表无此行 |
| `accounts_changed` | —（无载荷） | **〔SR1a · `设计/05 §13.6 ③`〕这台机器上的账号清单变了**（watcher 盯账号 manifest 所在目录，一批文件事件里 manifest 动了几次都只发一帧）。客户端收到就重拉一次账号清单（`accounts-list`）—— 清单本身不在帧里，唯一出口仍是那条查询。manifest 所在目录起步时不在 / 后来被删掉重建 ⇒ 这一路听不见（已知边界，代价只是不推帧）。monitor 收到发前端既有的 `remote-backend-ready`（带 `reason: "accounts_changed"`），账号表与 chip 随之重取 |
| `sessions_replayed` | —（无载荷） | **〔U4b · 第四波〕这台机器的活会话清单报完了**：`watch_loop` 的 Phase 1（同步扫 `sessions/`、对每个活 pidfile 发一帧 `session_added`）走完那一刻发**一次**，排在 Phase 1 所有帧之后、Phase 2 任何帧之前（同一个 sink、同一条线程）；`sessions/` 不在也照发（清单是空的，也是说完了）。**为什么要它**：客户端手里有一条「固定」的会话条目而这台还没报过它时，得分清「这台还没说完」（显示**说不清**）与「说完了、里面没有它」（显示**已结束**）—— `设计/30 §3.5.7a` 那张表的判据，此前线上没有任何东西分得开。丢了不可恢复（`overflow.lost` 带身份、subject 无）：客户端停在「说不清」，不会被说成已结束。monitor 收到发前端 `origin-sessions-listed {origin}`（与 `remote-session-added` 同一条线程、同序）|
| `link_data` | `link`, `data` | **〔SR1a〕一条链路的下行字节**（`data` = base64，标准字母表带补位；解码后 ≤ 32 KiB）。只在客户端开了链路（`link-open`）之后才出现；链路上的字节与 C2 拨号代理的 stdout 逐字节同形。**不丢**：走应答那条独立通道。完整语义在「入方向」那一节的「链路四条」 |
| `link_end` | `link`, `error?` | **〔SR1a〕这条链路不会再有字节了**，后端已忘掉这个 id。`error` 缺席 = 正常收尾；在 = 非正常收尾的人话。拨不通**不**走这里（那是链路字节里那一行失败的 ack） |
| `transfer` | `id`, `got`, `total`, `end?` | **〔SR1b〕一趟传输此刻的样子**（`transfer-start` 之后才出现）：每一帧是整份快照（`got` / `total` 字节），不是增量 ⇒ 后端按变更合并、堵住时只合并不堆积。带 `end` 的那一帧是这一趟的**最后一帧**：`{"state":"done","bytes"}` · `{"state":"failed","why"}` · `{"state":"cancelled"}`。**不丢**：走应答那条独立通道。完整语义在「入方向」那一节的「传输四条」 |

### 入方向：流连接上的命令信封（U6b-1）

在此之前这条协议是**单向**的：后端只写、从不读。这不是设计选择，是缺口 —— 它已经在制造绕路：

- **`--tmux-notify` 存在的唯一理由**就是 tmux hook 子进程没法给正在跑的后端发消息，
  只能新起一个进程、校验身份、发信号。
- **`--resolve` 为一次极小的 RPC 单开一整条 SSH exec。**

载体本来就在：monitor 那头拿的是 `russh::ChannelStream`，**双工**，而写半边
**在 U8a-2a 之前从来没人用过**（U8a-2 摸底逐个 `write_all` 核过：零数据字节）。
现在后端在**同一条流连接**上读 stdin，monitor 侧的发送端见下「客户端侧语义」。

```text
→ {"id":"<opaque>","cmd":"<name>","args":{...}}          请求（一行一个）
← {"kind":"reply","id":"<opaque>","ok":true,"data":{…}}   成功（`data` 是命令的返回值，无返回值时省略）
← {"kind":"reply","id":"<opaque>","ok":false,"code":"…","message":"…"}
← {"kind":"cancelled","id":"<被取消的 id>"}
→ {"id":"<opaque>","cmd":"cancel","args":{"target":"<id>"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `id` | → ← | **不透明串**。后端 **不解析、不校验格式、不规范化，只回显**。谁生成谁负责唯一 —— 客户端。后端自己发号的话重连后号段会撞（同 §F90「不许拿会变的东西当持久键」）。含 emoji / 超长 / 纯数字都照原样回 |
| `cmd` | → | 命令名 |
| `args` | → | 命令自己的参数对象，缺省 `{}` |
| `ok` | ← | 成败。`true` 时 `code` / `message` **不上线**（skip_if_none） |
| `data` | ← | 命令的返回值（如 `resolve` 的 CommandPlan）。无返回值的命令省略 |
| `code` / `message` | ← | 失败原因。形状**对齐 `--resolve` 已冻结的那套**（协议 v1 §3），不发明第二种错误 JSON |

**四条刻意的选择：**

1. **应答复用出方向的 `kind` tag 空间**（`reply` / `cancelled` 是新 kind），不另开一条流 ——
   旧 monitor 见到未知 kind 会**忽略**（§10 已有的 additive 规律），新后端 × 旧 monitor 天然安全。
2. **取消是一条普通命令、不是带外信号**。带外要么另开通道要么发明转义序列，两者都要新的解析纪律；
   而取消排队等一下并无妨（它只在长命令上有意义）。取消一个不存在的 `id` 是**幂等**的、不是错误。
3. **应答走独立通道**（容量 256，与出方向的 10 000 分开）。出方向丢一条**内容帧**可恢复
   （`overflow` 会说丢了多少，行还在远端 jsonl 里）；**丢一条应答会让客户端永远等下去**。
   writer **有界地**优先应答（`main.rs::REPLY_BURST = 8`，连发 8 条后强制让位一次）——
   不优先的话一万行的洪峰会把应答排在后面，客户端那头看起来就是「命令没反应」；
   而**无条件**优先又会反过来饿死实时行（D 审计实测：500ms 内应答 70 789 条 vs 实时行 **4** 条，
   机理是应答通道满时读循环阻塞 ⇒ 通道恒满 ⇒ `biased` 每次都命中应答）。
4. **应答通道满时阻塞入方向，不丢弃**。满 = 客户端连应答都读不过来，这时候把背压顶回去正是想要的。

**信任边界**（出方向后端是唯一写者；入方向它变成读取不可信输入的一方）：

| 约束 | 行为 |
|---|---|
| 单行上限 **1 MiB** | 超过即整行**不解析**（解析它本身就是被攻击面）+ 回 `line_too_long`。量级同 `--resolve` 的 stdin 上限。〔F9c · 第四波〕应答的 `id` 从这一行**开头至多 4 KiB**（`inbound::ID_SNIFF_BYTES`）里尽力抠：顶层对象、值是字符串的那个 `"id"`；抠不出（不在那一段 / 不是字符串 / 形状不对）才回空串 ⇒ 发这一行的调用方当场收到自己的错，不用熬满预算超时 |
| 坏 JSON | 回 `bad_request` 并**继续读下一行**。`id` 无从得知 ⇒ 回空 `id`，客户端按「上一条没应答」超时处理 |
| 任何失败 | **绝不 panic、绝不结束读循环、绝不结束进程** |

**★ 两条时序 / 线程约束，两侧都做成了「不可表示」——不是机检，更不是注释：**

> ⚠ 本节此前指名了两条机检（`hello_is_flushed_before_the_inbound_reader_starts`〔散文墓碑〕 /
> `handlers_never_run_on_the_reader_task`〔散文墓碑〕）。**它们在 U6b-3 已经被删掉了** ——
> 因为 D 审计用普通写法把两条都绕过去了，处置是「让违规不可表示」而不是往判据上加正则。
> 文档没跟上，指着两个不存在的测试名。U8a-2a 订正。

- **客户端在读到 `hello` 之前不许写命令**（客户端要先知道对面是什么版本、有什么能力）。
  - 后端侧：`inbound::spawn` 的参数里有一个 `wire::HelloFlushed` 见证，
    而它**只能**由 `wire::write_and_flush_hello` 产出 ⇒ reader 抢在 Hello 之前起来**编译不过**。
  - monitor 侧（U8a-2a）：写半边一拿到就被 `inbound_client::park` 收进 `ParkedWriter`，
    那个类型**身上没有任何写方法**；唯一出口 `into_client` 要一个 `BackendHello`，
    而 `BackendHello` 只能由一帧真的 `InboundFrame::Hello` 换出来。
    （诚实边界：能绕过的唯一方式是自己造一个假 Hello 帧 —— 那在调用点是显眼的胡来。）
- **命令处理器一律不许跑在读循环那个 task 上**（`cancel` 是唯一例外，且必须例外：
  它就是用来打断别的命令的，自己再排到那些命令后面就永远不生效）。
  一条慢命令占住读循环 ⇒ 后续命令全排队，而症状表现成「远端没反应」、几乎归因不到具体哪条。
  做法：`dispatch` 是**非 async 函数** ⇒ 分派臂里根本没有 `.await` 可写（写了是 `E0728`）。
  要跑活只能返回 `Disposition::Spawn` 把 future 交给调用方。

#### 客户端侧语义（monitor `inbound_client`，U8a-2a）

后端侧刻意**不管**这些 —— 零定时器铁律不改，超时一律推给客户端。

| 事项 | 归属 | 做法 |
|---|---|---|
| `id` 生成 | 客户端 | `<连接 nonce>-<单调序号>`。nonce 每条连接一套 ⇒ 重连后号段不撞，后端的 `duplicate_id` 正常打不到 |
| 超时 | 客户端 | `call(cmd, args, timeout)` 自带，且**覆盖「写入 + 等应答」两段**（共用一个 deadline）。只裹后半段的话，写半边被反压卡住时它会无视自己的 timeout 永久挂起 —— 上面第 4 条（应答通道满时阻塞入方向）正是那条链的一环。等应答超时后**补发一条 `cancel`**（best-effort）让后端别白跑；写入超时则不补（那条命令根本没入队） |
| 超时后的登记 | 客户端 | **不摘**。摘了的话晚到的 `reply`/`cancelled` 会落进「未登记的 id」，每次超时刷一条 warn ——而那是预期内的事。登记由路由侧摘，`oneshot` 送不出去即知调用方已走 |
| 并发上限 | 客户端 | 同时在等的命令 ≤ 256（同后端应答通道容量）。**满之前先回收「调用方已走」的登记**（`oneshot::Sender::is_closed`）—— 否则背压路径下后端的 cancel 应答被 `try_send` 丢掉，那条 id 永远等不到帧，表只涨不落且不自愈 |
| 能力门控 | 客户端 | `hello.commands` 里没有的命令**直接拒**，一个字节都不发。旧后端无该字段 ⇒ 空集 ⇒ 不发任何入方向命令 |
| 关写半边 | 客户端 | 显式指令（`close_write`），**只有一次性探测该调**。⚠ 关它**不会让后端退出** ——stdin EOF 只结束后端的入方向 reader task；进程只在 stdout 关掉或收到停机信号时退出 |

真进程端到端在 `tests/e2e/inbound-backend-frames.sh`（15 条断言，进 CI 带地板）——
它喂给真后端的那条 ping 行**逐字节**由 monitor 的编码器钉住
（`the_e2e_ping_line_is_exactly_what_the_encoder_produces`），否则那套件只是在验证一个
monitor 永远不会发的形状。

**今天有三条命令**：`ping` / `cancel`（骨架验收用）+ **`resolve`**（第一条真业务命令，见下），它们随 `hello` 的 `commands` 字段上线 —— 那是与分派表**同一份真相源**（`inbound_structure_guards.rs::the_commands_mirror_matches_the_registry` 钉住：声明了却不接 ⇒ 客户端发过去石沉大海；接了却不声明 ⇒ 客户端不知道能用）。真业务命令从 `--resolve` 吸收开始。

> ⚠ 〔`K-R20` 订正 09-03〕上面那句原先点的是 `hello_commands_match_the_dispatch_table`〔散文墓碑〕——
> **那个名字全仓零定义**（`U8a-2d` 把「扫 `dispatch` 分派臂文本」换成了「`COMMANDS` 对 `REGISTRY` 数据对数据」
> 的时候换掉的），而这句话一直**当现状在说**。同一个已删名今天在 `inbound.rs` 里也有一处
> 当现状说的（PM 09-03 自己收了）与一处自陈「上一版是 …」的 —— **一份文件里一处当现状、一处说是历史**。

#### `bus-list`：谁在线 + 各自待读多少（P4f，第一条 cc-bus 命令）

```text
→ {"id":"B1","cmd":"bus-list","args":{}}
← {"kind":"reply","id":"B1","ok":true,"data":{"agents":[
     {"id":"proj_cc","target":"proj_cc:0.0","unread":2,"live":true,"ccm_sid":"1a2b3c4d"}]}}
```

`agents` 每项五个字段：`id`（总线身份）· `target`（tmux 地址）· `unread`（待读条数）
· `live`（这个地址**今天还在不在**）· `ccm_sid`（那个会话绑的 Claude sid，没绑 ⇒ `null`）。

★★ **总线成员是身份空间的子集，不是第二套名单**〔用@08-13：「那他不应该是身份空间的
子集吗? 他应该去调用身份空间啊」〕。cc-bus 自己那份 `agents.tsv` 记的地址**会过期** ——
会话名被重用是常态（`cc-spawn` 按目录基名取名），实测后果是敲门文字被打进**陌生占用者**
的屏幕。⇒ `live` / `ccm_sid` 由后端 **去问 tmux**（一次 `list-sessions` 列全部，
不是每个成员探一次），cc-bus 那份只回答「谁登记过 + 邮箱里还剩几条」。

⚠ **`live` 有三态**：`true` / `false` / `null`。拿不到身份空间（没装 tmux、起不来）时是
`null` —— 「不知道」与「不在」是两件事，混起来会让调用方把一屋子活人当成死人。

**它是只读的**，而且**刻意只读**：cc-bus 那边真正"读消息"的命令是 `cc-recv`，
而 `cc-recv` **会推进已读位置** —— 后端代替人去读，等于把消息从人那里偷走
（agent 自己再跑 `cc-recv` 就什么都看不到了）。⇒ 「有没有新的」这个问题由 `unread` 回答，
**没有 `bus-recv` 这条命令**。要做代读的那天，先得给 cc-bus 一个「读了但不算数」的两阶段口。

错误码：`not_installed`（找不到 `cc-list`，消息里带查过哪些位置）· `timed_out` · `failed`。

#### `bus-kill`：收掉一个总线成员（P4f）

```text
→ {"id":"B3","cmd":"bus-kill","args":{"id":"proj_cc"}}
← {"kind":"reply","id":"B3","ok":true,"data":{"id":"proj_cc","killed":true,"stale_only":false}}
```

`killed` = 会话真的被杀了；`stale_only` = **身份对不上**，只摘掉了那条陈旧登记
（会话与收件箱都没动）。两种情况 `cc-kill` 都算成功，**必须分得开** ——
「收掉了」和「那个名字现在是别人的」对用户是两件事。

★ **门在懂语义的那一侧**：后端自己那条 `kill` 用 §34 三道门，因为它的归属证据弱
（名字前缀 / `@ccm_sid`）；`cc-kill` 用的是强证据 —— `agents.tsv` 第 4 列（登记时记下的
pane 根进程 pid）+ 登记的完整地址。08-13 实测过不核的后果：**杀掉占了同名的无辜进程与会话**
（`kill -9` 整棵树 + 删收件箱）。⇒ 这里不重写一遍门，转调它。

⚠ 它做的事比后端的 `kill` 多：杀会话 + 进程树 + 清名册 + 清台账 + 清那个 id 的状态。
「多窗口要不要拦」是产品判断（账本 `U18` 待裁）；今天照收，`cc-kill` 会把窗口数打出来。

错误码：`invalid_args`（id 非法/缺）· `not_installed` · `timed_out` · `failed`。

#### `bus-send`：发一条消息（P4f）

```text
→ {"id":"B2","cmd":"bus-send","args":{"to":"proj_cc","text":"结论在 #82"}}
← {"kind":"reply","id":"B2","ok":true,"data":{"to":"proj_cc","sent":true,"registered":true,"live":true}}
```

`to` 收件人身份，`text` 正文，`from`（**可选**）以谁的身份发；回 `sent` + **有没有人会读**：`registered`（在总线名单里吗）
· `live`（那个会话今天活着吗，与 `bus-list` 同一套三态）。argv 直传、**不过 shell**。

★ 为什么要这两个字段：两种「没人会读」今天都长得像成功 —— ① 收件人**压根没登记**
（名字打错一个字母就会造出一个没人读的收件箱）；② 登记过、**会话早没了**。
⚠ 它们**不改变投递**：先发后到是正当用法（对方待会儿才 `cc-register`），
所以照发不误，只是把话说清楚。

★★ **`from` 不给会怎样**：后端跑 `cc-send` 时不在任何 tmux pane 里，
`cc-whoami` 解不出身份 ⇒ 实测收信人看到的是「**来自 unknown**」，
而它给的回复方式是 `cc-send unknown "…"` —— **回复直接掉进一个没人读的收件箱**。
⇒ 调用方应当给 `from`；后端把它作为 `CC_BUS_ID` 传给子进程
（那是 `cc-whoami` 优先级第一条，**cc-bus 现成的契约**，不改它本体）。
回值里的 `from` 回显的就是这次用的身份，**不给就是 `null`** —— 让调用方看得见这件事，
而不是等收信人来问「谁发的」。

错误码分三档，**刻意分得开**（合成一个的话用户分不出「名字写错了」和「被规则拦了」）：

| 码 | 什么情况 | 来源 |
|---|---|---|
| `invalid_args` | 缺 `to`/`text`，或 cc-send 判定收件人非法 | 形状校验 / `cc-send` rc=2 |
| `rejected` | 被路由层拦下（ACL / 限流 / 去重 / 灭环），`bus.log` 里有对应一行 | `cc-send` rc=3 |
| `not_installed` | 找不到 `cc-send` | 查找规则全落空 |
| `timed_out` | 子进程跑过了期限被结束（默认 10 秒，`CC_BUS_TIMEOUT_SECS` 可调） | 子进程退出码 124 |
| `too_long` | 正文塞不进一次命令调用（内核单参数上限 **128 KiB**），诊断里带实测字节数 | 起进程时 `E2BIG` |
| `failed` | 其它 | 其它退出码 / 起不来 |

⚠ **期限住在子进程里，不在后端里**：后端侧一个计时器都不加（零定时器铁律 +
本文档上面那条「超时一律推给客户端」），而是给子进程套一个 `timeout` 前缀 ——
`ccm` 问后端那条早就是这么写的。找不到 `timeout` 这个命令时**如实降级**：裸跑、没有期限。
不这么做的后果实测过：`cc-send` 卡在 flock 上时后端 **无限等**，
而这两条是阻塞档、`cancel` 对 `spawn_blocking` 是空操作 ⇒ 一个 worker 被占死。

**收件人合法性归 cc-bus 自己**，后端这一层不再写第二份白名单 —— 两处规则会漂。

★ **这两条命令都是转调本机的 cc-bus 命令，后端不读 cc-bus 的任何数据文件**
（地址簿 / 收件箱 / 已读位置）。用户 08-13 逐字说过「后面我可能要改ccbus」⇒
这一层只把 cc-bus 的**命令**当接口：命令是给外人用的，文件格式是给自己用的。
由 `control/cc_bus.rs` 里的 `no_cc_bus_data_layout_leaks_into_the_backend` 钉住。

#### `bus-broadcast`：给总线上**在线**的成员群发一条（C4e，09-25）

```text
→ {"id":"B5","cmd":"bus-broadcast","args":{"text":"门禁全绿了","from":"cc-monitor"}}
← {"kind":"reply","id":"B5","ok":true,"data":{"sent":7,"skipped_offline":78,"liveness_unknown":false,"failed":[{"id":"x_cc","error":"timed_out","detail":"…"}]}}
```

`text` 正文（非空）· `from`（**可选**，同 `bus-send`：以谁的身份发，也用来「不发给自己」）。
它是一个**组合**，住后端这一侧：列名单（与 `bus-list` 同一个函数）→ 挑人 → 逐个投递（与 `bus-send` 同一处起 `cc-send`）。
此前这个组合住 monitor；界面改成经通道直接说后端之后（`设计/05 §14.3`「业务解释只有一个家」）收进这里，
界面只收一份成品。

挑人规则（P4f 08-13 实测出来的）：身份空间答得上 ⇒ **只发 `live == true` 的**（老 `cc-broadcast` 发给名册的
每一行：实测 86 行登记、只有 8 个会话还活着 ⇒ 78 个没人读的收件箱）；全是 `null`（问不到 tmux）⇒ 退回
「发给所有登记的」并回 `liveness_unknown: true`（「问不到」≠「都不在」）；不发给 `from` 自己。

回值三个数**分开说**：`sent`（投出去几条）· `skipped_offline`（因不在线跳过几个）· `failed`（逐个列，
`{id, error, detail}`，`error` 是 `bus-send` 那一套码；键刻意不叫 `code` / `message` —— 那一对是整条失败的错误信封）。⚠ **部分失败不整条回错**：已经投出去一部分之后再失败，整条回错会让
调用方以为一条都没发、再发一遍 ⇒ 一部分人收到两遍。

错误码（整条失败，一条都还没发）：`invalid_args`（`text` 缺 / 空）· `not_installed` · `timed_out` · `failed`（列名单那一步）。

#### `bus-state`：总线名单 ＋ spawn 台账**一次回全**（`K-R113`，09-13）

```text
→ {"id":"B4","cmd":"bus-state","args":{}}
← {"kind":"reply","id":"B4","ok":true,"data":{
     "agents":[{"id":"proj_cc","target":"proj_cc:0.0","unread":2,"live":true,"ccm_sid":"1a2b3c4d"}],
     "spawned":[{"id":"proj_cc","live":true,"dir":"/home/zbl/proj","task":"跑门禁"}]}}
```

**入参：无**（不读 stdin —— 与 `bus-list` 同一条纪律：声明无输入的命令必须秒回）。

回值两半：

- `agents` —— **与 `bus-list` 逐字同一份**（同一个实现，不是长得像）：
  `id` · `target` · `unread` · `live` · `ccm_sid`，语义见上面 `bus-list` 那一小节。
- `spawned` —— `cc-spawn` **派生**过的会话（区别于手工起的）：
  `id`（总线身份）· `live`（三态，见下）· `dir`（那个 agent 的工作目录）·
  `task`（spawn 时给的初始任务，自由文本）。

★ **为什么是一条命令而不是两条**：这两份名单**互相引用** —— 判一个 spawn 记录还活不活着，
要回总线名册借「登记时记下的 pane 根进程 pid」去核身份。分两条命令取回来的是**两个时刻**的两份，
在调用方拼起来会拼出一份**盘上从没存在过**的状态。

⚠ **`spawned` 的 `live` 与 `bus-list` 的 `live` 是同一套三态**：`true` / `false` / `null`。
`null` = **核不了**（那份 spawn 台账没有身份列，借不到 pid），**不是**「不在」。
把「核不了」并进「活着」是这一族全部事故的共同起点（会话名被重用是常态，
实测后果是敲门文字打进**陌生占用者**的屏幕）。

⚠ **一次回全的另一半含义：要么两半都答，要么明说失败。** 任何一半取不到都回错误，
**不回一份看上去完整的半份** —— `spawned` 是空数组还是「问不到」，在调用方那儿长得一模一样。

🔴 **它今天答不出的三样，如实登记**（下游要用就得先给 cc-bus 加一条机器可读的输出，
**不是**绕到它背后去读那两份 `.tsv`）：

| 拿不到的 | 为什么 |
|---|---|
| 登记时间（`agents` 那半） | `cc-list` 不打印它 |
| spawn 时间（`spawned` 那半） | `cc-agents` 读了台账第 3 列却不打印它 |
| 坏行计数 | 两条命令都**静默跳过**读不懂的行，数不出来 |

⚠ 还有一处**认不准**：`cc-agents` 的输出是定宽 `printf` 的表，列间没有唯一分隔符 ⇒
**目录名里含空格时**，`dir` 只取到第一段、余下的并进 `task`。`id` 与 `live` 两列不受影响
（前者过 `[A-Za-z0-9_-]` 白名单、后者是三个固定字面量之一）。

错误码：`not_installed`（找不到 `cc-list` / `cc-agents`，消息里带查过哪些位置）·
`timed_out` · `failed`。**只读**：两条被调命令都不写任何文件。

#### `bus-spawn`：派生一个协作 agent（BS1b，09-24）

```text
→ {"id":"B5","cmd":"bus-spawn","args":{"tool":"claude","dir":"/home/zbl/proj","task":"跑门禁","account":"a1"}}
← {"kind":"reply","id":"B5","ok":true,"data":{"spawned":true,"id":"proj_cc-2","said":"已 spawn: proj_cc-2   (目录: /home/zbl/proj  初始任务: 跑门禁)\n…"}}
```

入参：`tool`（起哪种 agent，**后端只判非空**，认不认归 `cc-spawn` 自己）· `dir`（工作目录，非空）·
`task`（初始任务，可空）· **`account` 与 `base:true` 恰好给一个** —— 两样都不给 ⇒ `invalid_args`。
⚠ 为什么逼调用方表态：不传的话 `ccm` 落 manifest 的默认号，等于**替用户选了一个他没选过的号**去起一个真
agent、烧真额度。

回值：`spawned`（恒 `true`）· `id`（新会话的总线身份；从 `cc-spawn` 的回显里认，**认不出就是 `null`** ——
那是「起了，但名字没认出来」，**不是**「没起来」）· `said`（`cc-spawn` 的原始回显，给人看）。

★ **本机与远端同一条路**：monitor 对每台机器（含 `<local>`）都走这条原语，不再有「远端拼一条 `cc-spawn …`
shell 串走 SSH、本机拒绝」的分叉。命名避让 / 登记进总线 / spawn 台账 / 预信任目录全在 `cc-spawn`
（它内部再经 `ccm`），后端**只转调**。发给 `cc-spawn` 的 `--tool` / `--account` / `--base` 是**子进程的**
旗标，不是后端 argv（`protocol_doc_guard::CHILD_PROCESS_FLAGS` 登记 ＋ 两向判据）。

错误码：

| 码 | 什么情况 |
|---|---|
| `invalid_args` | 缺 `tool`/`dir`、账号没表态或两样都给；或 `cc-spawn` 自己 rc=2（目录不存在 · 不认的 tool · ccm 太旧） |
| `not_installed` | 找不到 `cc-spawn` |
| `timed_out` | 子进程跑过期限被结束。🔴 **会话可能已经起来了**（`cc-spawn` 是建完会话才回显的）⇒ 先 `bus-state` 看一眼，**别直接重试**：重试会再起一个真 agent |
| `failed` | 其它退出码 / 被信号打断 |

⚠ 期限同 `bus-send`：住在子进程里（`timeout` 前缀，默认 10 秒，`CC_BUS_TIMEOUT_SECS` 可调），后端零定时器。
⚠ **这是写面，而且有代价**：它起一个真 agent 进程。UI 侧必须先让用户确认（与收掉 agent 同一条纪律）。

#### `kill`：杀一个 tmux 会话（F04a，**第一条破坏性入方向命令**）

```text
→ {"id":"K1","cmd":"kill","args":{"name":"1a2b3c4d-cc"}}
← {"kind":"reply","id":"K1","ok":true,"data":{"session":"1a2b3c4d-cc","killed":true}}
```

**它必须过 §34 的三道门**
（⚠ **`K-R72` 2026-09-12**：monitor 侧 `kill_remote_tmux` 那条 shell 路**已经删了**——
F04b 先把它从**主路**降为一次性回落，本件把它整块拿掉 ⇒ **杀会话今天只剩本命令这一条路**。
后端通道不在就是**明确失败**，不再换条路悄悄做掉。〔C4e · 第四波 4C〕那条 Tauri 命令本身也迁到界面了：
界面经通道直接说本命令（`src/tmux-control.ts::killSession`），用户看得见的那句话住文案表 `tmuxControl.channel.*`
（本机 / 远端两句不同的话）；「能不能证明没发出去」那条分流规则仍见 `backend/control/backend_route.rs` 头注那张表）：

| 门 | 判据 | 不通过的错误码 |
|---|---|---|
| Gate 1 | `=name:` 精确匹配（`exact_target`）；`name` 不许含 `:` / `=` / 控制字符 | `invalid_args` |
| Gate 2 | 名字命中 `cc-*`／`<X>-cc` **或** 远端 `@ccm_sid` 已设 | `wrong_owner` |
| Gate 3 | `session_windows == 1`（**只给破坏性动作**） | `too_many_windows` |

目标不存在 ⇒ `no_such_session`；起不来 tmux ⇒ `no_tmux`；过了门却没杀成 ⇒ `kill_failed`。

**两件它刻意这么做的事**：

1. **杀的是 `#{session_id}` 句柄，不是名字。** 三道门与 `kill-session` 是两次 tmux 调用，
   之间有窗口；对句柄下手 ⇒ 名字被重新绑定也杀不到别人。**破坏性动作尤其不能对名字下手。**
2. **Gate 3 只给它。** `send-keys` 不删除任何东西，窗口数与它无关 ——
   给 `send-into` 加 Gate 3 会让「往多窗口会话里打字」被误拒。
   所以 `admit`（非破坏性）与 `admit_destructive` 是**两个入口，不是一个带 flag 的**。

⚠ ~~本命令存在 ≠ monitor 已经改走它~~ **F04b 2026-08-04：monitor 已经改走它了**（当年是 monitor 的 `kill_remote_tmux` 主路调它自己那个发送端）。🔴 **`K-R72` 2026-09-12：那条一次性 SSH 已删** —— `C7` 说的过渡到此结束，**盘上没有第二条路**。定框 C6 的顺序（先搬门、再切路由）到 **F04c** 走完。🔴 **〔C4e · 第四波 4C〕monitor 那一跳也拿掉了**：界面经通道直接说本命令（`src/tmux-control.ts::killSession`），成品 `{session, killed}` 由跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 钉；回潮闸今天钉的是「monitor 生产段里一处 `kill-session` 都没有」＋「界面只经那一处说它」（`tmux_backend_gate_guard`）。

#### `launch`：平面 ②（远端执行面）——真的建 tmux 会话（U8a-2b）

```text
→ {"id":"L1","cmd":"launch","args":{
     "mode":"create-or-attach" | "send-into" | "send-keys-raw",
     "name":"cc-1a2b3c4d",
     "payload":"cd '/x' && claude --resume …",
     "cwd":"/x",             // 可选，仅 create-or-attach
     "ccm_sid":"<完整 sid>",  // 可选，仅 create-or-attach；[A-Za-z0-9_-]
     "agent":"claude",       // 可选，仅 create-or-attach；[A-Za-z0-9_-]
     "width":"220",          // 可选，仅 create-or-attach；与 height **同时给或都不给**
     "height":"50"           // 可选；1–4 位十进制**字符串**
   }}
← {"kind":"reply","id":"L1","ok":true,"data":{"session":"cc-1a2b3c4d","created":true,"typed":true}}
```

##### ★ `agent` / `width` / `height`（`K-P2` D3，2026-09-03）：**接线补的那三个，不是「顺手加的功能」**

它们是「`shared/ccm` 的 `--tmux` 真的改走这条路」逼出来的。ccm 那条本地编排里
`new-session` 之后紧跟着 `set-option @ccm_agent <agent>`，`new-session` 自己还带 `-x/-y`
（`--tmux-size`）—— 而这条命令此前**没有任何字段能表达它们** ⇒ 不补就是**静默丢修饰**：
会话照建、载荷照送，只是 `@ccm_agent` 没了、窗口回到 80x24 把 agent 输出折行。
**「看起来生效了、只是少了一件」正是这条协议一路在消灭的形状。**

⚠ `width` / `height` 是**字符串**不是数字：它们原样进 tmux 的 argv，而 `parse_request`
只有 `get_str` 一种取法（换第二种取法就得给 `launch_fields_match_its_parser_and_output`
那面镜子加第二种抽取，而那条判据的全部价值就在于镜子自己不会漂）。值域由
`check_size` 收窄成「非空、纯十进制、≤4 位」——**不靠类型靠校验**。
⚠ 两个**必须同时给**：只给一半 tmux 会用默认值补另一半 ⇒ `invalid_args`。
⚠ **没有 `avoid_collision`**：撞名避让住在 ccm 要搬的那一块**之外**，而「撞了」这件事
本命令已经用 `created:false` 表达完了 —— 调用方据此走它自己的响亮失败。

**三件事它刻意不做**：

1. **不 attach。** U8a 把「起会话」分成三个平面 —— ① 计划面（`resolve`）· ② 远端执行面（本命令）·
   ③ 本机开窗面。attach 属于 ③，而后端在远端，开不了你面前的窗。
2. **不过 shell。** tmux 用 `Command::new("tmux").args([...])` 直传 argv ⇒
   引号 / 转义 / 注入这一整类问题在这条路上**不存在**，不是「被挡住了」。
   ⇒ monitor `launch.rs` 里那条「禁双引号」是 **PowerShell 专属**（`wt.exe` 传参畸变），
   **这条路上不成立、也不许照抄**。
3. **`send-into` / `send-keys-raw` 时不新建会话。** 会话不存在 ⇒ 回 `no_such_session`。
   顺手新建就是 #76 的反向：用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell。

##### 🔴 ★★ `create-or-attach` 的**唯一调用方 `ccm` 从此没有退路**（`K-P2` F 拍，2026-09-04）

用户 09-04 逐字裁定：「**ccm不要管找不到, 统一走后端**」。

**这一条是协议的读者要知道的行为变化，不是 ccm 的内部细节** —— 因为 `shared/ccm`
经 `src/bridge/src/sftp.rs` 的 `include_str!` 被**部署到每一台远端机器**上，而这条协议
就是它与那台机器上的后端之间的契约。

| | 09-04 之前 | 09-04 之后 |
|---|---|---|
| 后端在，答 `created:true` | ccm 什么都不再做（会话与载荷都由后端做完） | 一样 |
| 后端在，答 `created:false`（撞名） | 退回本机 `new-session` ⇒ 撞上 ⇒ `exit 3` | **直接** `exit 3`（同一句文案，少绕一趟） |
| 后端 **不在** / 跑不起来 / 答非所问 | **退回本机 tmux 直起**，往 stderr 说一句「已降级」，`rc=0` | **`exit 4` ＋ 一句人话**。ccm 不铺、不起、不找退路 |

⇒ **对协议实现方的两条硬结论：**

1. **这条命令的可达性从「锦上添花」变成了「起会话的唯一路径」。**
   后端在这台机器上不可达 ⇒ 那台机器上 `ccm --tmux` **起不来会话**，
   不再有「它自己悄悄用本机 tmux 兜住了」这回事。
   ⇒ 远端部署（`sftp::ensure_backend_deployed`）从「装了更好」变成**硬前置**。
2. **`created:false` 是撞名，不是失败** —— 它仍然走调用方自己的响亮失败（`exit 3`），
   与「后端不可达」（`exit 4`）**是两个码**。别把它们并成一个：
   前者是「这个名字有人用了」，后者是「这台机器上没有后端」，可修的方法完全不同。

⚠ **一格如实登记**：`ccm --tmux --print` 吐的**仍然是本机那条 tmux 编排的配方** ——
那是「`--print` 该描述什么」这个**接口问题**还没裁（件计划 `§6-5 上报③`），
**不是留了一条退路**：exec 路上那段编排一步都走不到
（钉住它的那条判据 `ccm_cli_contract::the_local_launch_recipe_is_reachable_only_from_print` 〔散文墓碑〕 已随 `shared/ccm` 于 `K-R48` 删除；今天 `--print` 与真跑读同一个 `Plan`，由 `control::ccm::plan::tests::print_and_exec_cannot_drift_because_they_read_the_same_plan` 接住）。
⇒ 读 `--print` 的输出时别把它当成「真跑时会发生什么」的描述，这一格今天**对不上**。

##### ★ `send-keys-raw`（F04c）：发裸键、**不附尾 `Enter`**

与 `send-into` 的**唯一**区别就是那个回车。它存在的理由不是「更灵活」，而是：
monitor 的 `tmux_send_keys(…, enter=false)` 生产上唯一的用途是**优雅退出时发 `Escape`
打断当前回合**，多一个 `Enter` 就变成「**提交用户输入框里排队的文本**」。

⚠ **为什么是一个新 mode 名，而不是给 `launch` 加一个 `enter` 字段**：
`parse_request` 是手工从 `Map` 取键的、**不 deny unknown fields** ⇒ 旧版本后端会
**静默忽略**那个字段、照样附 `Enter`（静默做错）。而未知 **mode** 会回 `invalid_args`
⇒ 客户端拿到明确错误、可以干净回落。**能力协商在 mode 名上是免费的。**

⚠ 它与 `send-into` **同一道门**（§34 Gate 2），**没有** Gate 3 ——
`send-keys` 不删除任何东西，给它加窗口数判断会让「往多窗口会话里打字」被误拒。

⚠ 客户端侧：`enter=true` 用的是**既有**的 `send-into` ⇒ 旧后端也接得住；
只有 `Escape` 那一支需要新版本后端。**兼容面不是全有全无。**

**`data` 三字段就是失败语义**（能分辨「没起成」与「起了但没确认」）：

| 结局 | `ok` | `code` | `created` / `typed` |
|---|---|---|---|
| 新建 + 键入 | true | — | true / true |
| 会话已存在（幂等短路，**不重复 resume**） | true | — | false / false |
| `send-into` / `send-keys-raw` 键入成功 | true | — | false / true |
| tmux 不在 PATH | false | `no_tmux` | 没起成 |
| `send-into` / `send-keys-raw` 但会话不存在 | false | `no_such_session` | 没起成 |
| `send-into` / `send-keys-raw` 但会话不是本工具的（§34 Gate 2，〔TL2〕原先这一行漏了） | false | `wrong_owner` | 没起成 —— 没往别人的会话里打字 |
| 建不出来且也不存在 | false | `create_failed` | 没起成 |
| 会话在，`send-keys` 失败 | false | `typed_unconfirmed` | **起了但没确认** —— 别重试新建 |

⚠⚠ **`typed:true` 只有 `send-keys` 的退出码那么强**〔audit-0805 F10，报告 I-3〕。
pane 处于 **copy-mode**（用户滚了一下轮子）时 `send-keys` **照样退 0**，
而键被 copy-mode 的键表吃掉、载荷根本没进应用。后端侧**没有第二种确认**
（`pane_in_mode` / `-X cancel` 全仓零命中），由
`launch_tests.rs::typed_is_only_as_strong_as_the_send_keys_exit_code` 钉住这个语义边界。
⇒ **消费方别把 `typed:true` 读成「载荷确凿落地」**。补第二种确认要真 tmux 才验得了，
登记在 `ROADMAP §5` 的诚实边界，留给 e2e tier2。
| 形状不合 | false | `invalid_args` | 没起成 |

**错误码分两层**（U8a-2b 定，趁 `launch` 还没有仓外消费方）：
**协议级**由 `inbound.rs` 独占 —— `bad_request`（信封 JSON 坏了）· `line_too_long` ·
`unknown_command` · `duplicate_id` · `handler_panicked` · `not_cancellable`，语义是
「客户端代码写错了，别重试」；**命令级**由各命令自己定，语义是「参数或环境的问题」。
所以 `launch` 的形状错误叫 `invalid_args` 而**不是** `bad_request`。
（⚠ `resolve` 今天仍回命令级 `bad_request` —— 它与仓外 aterm 的一次性契约冻结在 2026-07-18，
两条路复用同一个纯函数，改它会破坏那份契约。如实登记，不顺手改。）

**后端侧的校验是形状校验，不是安全边界**（同 §「信任边界」那条）：入方向命令来自
已经握着这台机器 SSH 会话的对端，它本来就能在这台机器上跑任意命令。这里只回答
「这组参数能不能构成一次有意义的 tmux 调用」，缺字段/空/超长/含控制字符 ⇒ 结构化错误。

⚠ **F12 2026-08-04 订正**：本段原来断言「这条路的生产切换还没发生、登记为 U8a-2c」——**那句自 U8a-2c-1 起就假了**，而 F07/F11 连着订正了 `INVARIANTS §33b` 里的**三份副本**、**唯独漏了这一份**（是 Phase G 的 `/full-audit` 逮到的）。今天的实况：〔C4e · 第四波 4C〕monitor 生产段 `.call("launch")` **0 处** —— 原来那两处（就地 resume 的 `send-into` = U8a-2c-1 · 送键 = F04c）连同它们的发送端迁到界面，今天由 `src/tmux-control.ts` 经通道直接说 `launch`；仍未切的是 **`create-or-attach` 与 attach 两格**。⚠ 那个数的唯一家在 `INVARIANTS §33b` 的〔机检〕锚点上，由 `doc_claim_registry` 读它与现场比；本文件从 F12 起也在那条判据的扫描面里 —— 同族副本再写回来就会红。

#### `capture-pane`：把某个 tmux 会话此刻那一屏抓回来（`K-R104`，只读）

```text
→ {"id":"C1","cmd":"capture-pane","args":{"name":"cc-ab12cd34"}}
← {"kind":"reply","id":"C1","ok":true,"data":{"name":"cc-ab12cd34","screen":"Welcome …"}}
```

`name` 要抓的会话名；回 `name`（回显）＋ `screen`（**那一屏的原文**，不解析、不裁剪、不归一 ——
`R58`〔用@09-13〕逐字「直接抓屏给我看」）。

★ **它是 CLI 面 `--capture-pane`（`K-R86`）的同一个本体**（`control/capture_pane.rs::capture`），
两个面只差取参数与包信封的方式（`K33` 逐字「所有命令只许有一处」）。
**帧面这一条是 `K-R104` 新加的**，理由是结构性的、不是性能取舍：CLI 面每调一次就是一次
SSH 握手，而当时的调用方（用量探针）两段轮询上限 12+20 轮 ⇒ 单次探测最多 **36** 次握手，
撑破 monitor 侧的 25s 硬超时。帧面是**一条长连接上多次往返**，握手恒 1 次。
〔`设计/50`：那个调用方已随用量 ③ 轴退役 —— **论证仍然成立，举的例子不在盘上了**。
本条今天的消费者是拉屏预览（〔C4e〕界面 `src/tmux-control.ts::capturePane` 经通道直接问；此前是 monitor 的 `capture_via_backend`，已删）。〕

🔴 **它只抓一次就返回，后端里没有任何「隔 N 毫秒再抓一次」**（零定时器铁律，
`no_timer_guard` 零容忍）。「抓几次 / 隔多久」是**调用方**的事 ——
`K37`〔用@09-11〕逐字「后端只给机制，不给偏好」，而「等画面稳定多久算稳」是偏好。

⚠ **空屏是合法的成功**：刚建起来什么都没打印的 pane 抓回来就是空 `screen` ＋ `ok:true`。
「抓没抓到」看退出码，不看输出是不是空。

⚠ **刻意不过 §34 Gate 2**：这是一次只读快照，与调用方那一侧口径一致（〔C4e〕今天是界面 `src/tmux-control.ts::capturePane`：只拒空目标、不过身份门；
此前 monitor 侧同族那一处的登记逐字：「只读快照，MASTERPLAN 明确不为它加身份门」）。

错误码：`invalid_args`（`name` 缺/空/含 `:` `=`）· `no_tmux`（tmux 这个程序起不来）·
`no_server`（这台机上一个 tmux server 都没有）· `no_such_session`（server 在、目标不存在）·
`capture_failed`（其它失败，**stderr 原样回包** —— 说不清但不撒谎）。

> 〔`设计/50` 删用量 —— 这里原有一节 `oneshot-session`（起一个到点自己会死的 tmux 会话）。
> 它是为**用量探针**建的（`K-R87` 的 CLI 面 → `K-R104` 的帧面），而用量 ②③ 两轴整轴退役
> ⇒ 这条命令**零生产调用方**，随之退役：`inbound::REGISTRY` 与 `COMMANDS` **11 → 10**，
> `BUILD_ID` `p2j-bus-state` → `p2k-usage-retired`。
> ⚠ **隔壁那条 `capture-pane` 没退**（拉屏预览真在用它），别把两条读成一刀。〕

#### `files-read` 这一族（步 `24f`，2026-09-20；**第五、第六条** 2026-09-21；**第七、第八条** F7a 2026-09-24）——**八条纯读命令，一条都不写盘**

六条一起看，逐条的信封在下面各自的小节里。共同的三条，别读宽：

1. 🔴 **整族只读。** 遍历 · 读文件名 · 存内存 · 答查询，**一个字节都不往盘上写**
   （`设计/96 §2.9` 边界①）。写那一侧（删 / 改名 / 建目录 / 复制）整个留在 SFTP。
   ⚠ 后两条也在这条之内：`files-index-rebuild` 是**遍历 ＋ 换掉内存里那一份**，
   `files-browse` 是**登记名单 ＋ 重列一遍** —— 两条都不往盘上写。
2. 🔴 **线上名用连字符，能力名用点。** 后端内部这一族的能力叫 `files.ls` / `files.stat` /
   `files.find` / `files.index.status` / `files.index.rebuild` / `files.browse`
   （`设计/96 §2.9` 那两张表逐字）；线上与 CLI 那一面一律是 `files-ls` / `files-stat` /
   `files-find` / `files-index-status` / `files-index-rebuild` / `files-browse`。
   两者之间的翻译**只有一处**（`files::answer_wire`）。客户端只看线上那一套。
3. **路径与查询串走原始字节。** 凡是路径类的值（入方向的 `path` / `needle`、
   出方向的 `path` / `hits`），要么是一个 UTF-8 字符串，要么是
   `{"b16":"<十六进制>"}`。**这里刻意不「尽力而为」地猜** —— 非 UTF-8 的文件名被有损
   解码之后就寻址不到了（`设计/60 §2 档②`），猜错一个字节就是去看另一个文件。
   ⚠ 出方向能用字符串表达的就用字符串，不能的才出 `b16` 形。

⚠ **今天它们在真机上答得出什么**（如实写，别当功能说明）：`files-ls` / `files-stat` /
`files-index-status` 是完整的；而 `files-find` 查的是**常驻索引**，而
**后端没有任何自己去建索引的节拍** —— `设计/60 §3.5.2a`：机制在后端、偏好由后端声明、
**节拍在调用方**（后端那条零定时器铁律不许它自己长出节拍）。
⇒ 一台刚起来的后端上 `files-find` 恒回 `index_missing: true`，那**不是**「没搜到」。
🔴 **客户端必须自己发 `files-index-rebuild`**（2026-09-21 起有这条命令了），
而且是它自己按节拍发：**不发就永远是 `index_missing: true`**。
「多久该再发一次」由 `files-index-status` 的 `rewalk_interval_secs` / `stale` 两个字段回答。
⚠ 别把「这条命令存在了」读成「索引会自己变新」—— 那件事今天仍然在调用方这一侧，没人替它做。

#### `files-ls`：列一个目录的直接子项（步 `24f`，**只读**）

```text
→ {"id":"f1","cmd":"files-ls","args":{"path":"/home/u/p","limit":1000}}
← {"kind":"reply","id":"f1","ok":true,"data":{
     "entries":[{"path":"/home/u/p/a.rs","kind":"file","size":1234,"mtime_secs":1758300000}],
     "truncated":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | 要列的那个目录。字符串或 `{"b16":…}`；空 ⇒ `bad_path` |
| `limit` | → | 这一趟最多回几条。不给 / 给 0 ⇒ **1000**。上限存在的理由不是省内存，是「一次往返」那句话要成立 |
| `entries` | ← | 每项一个对象：`path`（原始字节形）· `kind` · `size` · `mtime_secs`（后两个拿不到就**不出这个键**，不填 0） |
| `kind` | ← | **闭集四个词**：`dir` / `file` / `symlink` / `other`。🔴 这一栏走目录项自己的类型，**不跟 symlink** —— 指向别处的链接不会被报成它的目标 |
| `size` | ← | 字节数 |
| `mtime_secs` | ← | Unix 纪元秒 |
| `truncated` | ← | 目录里的项数多于回送的条数（被 `limit` 截了） |

**错误码**：`bad_path`（`path` 缺了 / 形状不对 / 空）· `unreadable`（这个目录打不开）。

#### `files-stat`：一个路径的元数据（步 `24f`，**只读**）

```text
→ {"id":"f2","cmd":"files-stat","args":{"path":"/home/u/p/a.rs"}}
← {"kind":"reply","id":"f2","ok":true,"data":{
     "path":"/home/u/p/a.rs","kind":"file","size":1234,"readonly":false,"mode":420,"mtime_secs":1758300000}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 入方向是要问的那个路径；出方向原样回送（原始字节形） |
| `kind` | ← | 同上那个闭集四个词 |
| `size` / `mtime_secs` | ← | 字节数 / Unix 纪元秒（`mtime_secs` 拿不到就不出这个键） |
| `readonly` | ← | 这个路径此刻是不是只读 |
| `mode` | ← | 〔GP1 · 第四波〕unix 权限位的低 12 位（十进制数；`420` = `0o644`）。**非 unix 平台不出这个键**（不是 `0`：`0` 是一个真能设的值）。文件窗口改权限那个框拿它显示现值（`设计/60 §7`） |

⚠ **它跟 symlink**（拿的是链接指向的那个东西的元数据）。不跟的那个读法要给后端的
只读动词白名单**加一个词**，那是**放宽一条红线**，所以没做 —— **这是一条真实的局限**。
要区分链接本身：用 `files-ls` 看它父目录那一行的 `kind`（那一栏不跟链接）。

**错误码**：`bad_path` · `unreadable`（这个路径读不到）。

#### `files-find`：在常驻索引里查（步 `24f`，**整族的存在理由**）

```text
→ {"id":"f3","cmd":"files-find","args":{"needle":"origin.rs","ignore_ascii_case":false,"limit":1000}}
← {"kind":"reply","id":"f3","ok":true,"data":{
     "hits":["/home/u/p/origin.rs"],"total_hits":1,"truncated":false,
     "scanned":20220,"index_age_secs":12,"index_missing":false}}
```

🔴 **它是 SFTP 给不了的那一条**：那个协议只能递归 `READDIR`（N 次往返），
而且**没有地方放一份常驻索引**。索引必须住在被搜的那台机器上 ⇒ 只有后端住在那里。

| 字段 | 向 | 说明 |
|---|---|---|
| `needle` | → | 要找的**字节**子串（字符串或 `{"b16":…}`）。空串 = 匹配一切（用来数总条目） |
| `ignore_ascii_case` | → | 只对 **ASCII** 段大小写不敏感（非 ASCII 字节逐字节比）。不给 ⇒ `false` |
| `limit` | → | 这一趟最多回几条。不给 / 给 0 ⇒ **1000** |
| `hits` | ← | 命中的路径，**最多 `limit` 条**。🔴 **只回送命中** —— 未命中的那几十万条路径一个字节都没离开那台机器，这就是「零流量搜索」那句话的全部内容 |
| `total_hits` | ← | 一共命中几条，**不受 `limit` 影响** |
| `truncated` | ← | 因为 `limit` 而没回全 |
| `scanned` | ← | 这一趟扫了几条（= 索引条目数）。**反空真用**：扫到 0 条的「没命中」与「索引是空的」在界面上一模一样 |
| `index_age_secs` | ← | 答这一趟用的那份索引，是多久以前建的 |
| `index_missing` | ← | 🔴 **索引还没建过** ⇒ 上面几个数全是 0，而那**不是**「没搜到」。见本族总说明那条 ⚠ |

⚠ **它不重走、不阻塞**：拿的是手上那一份，并把年龄一起交回去。
⚠ **只有「文件名子串匹配」这一档**：按内容搜 · 模糊匹配 · 排序 —— 一条都没设计。

**错误码**：`bad_args`（`needle` 缺了 / 形状不对）。

#### `files-index-status`：索引的新鲜度 / 条目数 / 常驻字节（步 `24f`，**不读 stdin**）

```text
→ {"id":"f4","cmd":"files-index-status","args":{}}
← {"kind":"reply","id":"f4","ok":true,"data":{
     "index_missing":false,"entries":20220,"resident_bytes":2544180,"unreadable_dirs":3,
     "truncated":false,"age_secs":12,"rewalk_interval_secs":300,"stale":false,
     "cold_first_build_secs":10,
     "browse_watches":4,"browse_watch_cap":64}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `index_missing` | ← | 还没建过。为真时下面几个数都是 0，**别当成「这台机器上没文件」** |
| `entries` | ← | 索引里有几条 |
| `resident_bytes` | ← | 它在后端内存里占多少字节（索引**只在内存里，重启重建**） |
| `unreadable_dirs` | ← | 上一趟遍历里有几个目录读不进去（权限等）。**不是 0 就说明这份索引有洞** |
| `truncated` | ← | 上一趟遍历撞到了条目数上限，没走完 |
| `age_secs` | ← | 这份索引建好到现在多少秒 |
| `rewalk_interval_secs` | ← | 🔴 **后端声明的重走周期**（今天 300）。`设计/60 §3.5.3` 要求这个延迟**显示在界面上**，不许让用户猜为什么搜不到 ⇒ 它必须可查询，不许只活在代码里 |
| `stale` | ← | `age_secs > rewalk_interval_secs` |
| `browse_watches` | ← | 此刻给「用户正在浏览的那几个目录」挂着几个 watch |
| `browse_watch_cap` | ← | 最多挂几个。全挂挂不住：本机 `inotify` 每用户上限现打 262144，而 home 下 640413 条目，且非特权拿不到全文件系统监听 |
| `cold_first_build_secs` | ← | 〔第四波 S4 · `设计/99 §2 Q5`〕后端**声明**的冷启动首建大约要几秒（今天 10，出处见 `index.rs::COLD_FIRST_BUILD_SECS`：一台 NVMe 上 `find` 的冷缓存读数取上整，**代理指标、不是实测**）。与 `rewalk_interval_secs` 分开：周期性重走是热的，后端刚起那一趟是冷的、用户看得见 ⇒ 窗口在 `index_missing` 那一趟的重走期间显示「正在建索引（首次约 N 秒）」 |

🔴 **「重走」这件事后端自己不做** —— 后端那条零定时器铁律不许它长出节拍
（`设计/60 §3.5.2a`：机制在后端、偏好由后端声明、**节拍在调用方**）。
〔⚠ 2026-09-21 订正：原话「**今天这一族里没有一条命令能触发重走**」**在 `24f` 第三刀（同日更早）落地时就已经假了** —— `files-index-rebuild` 就是那条命令。而它当时没人发；波 β 的 `P2` 之后，发它的是 `src/bridge/src/filewin/find.rs`。⇒ 今天的真话是：**这一族有一条命令能触发重走，而且窗口那一侧真的会在后端说没索引／过期时发它**。〕

⚠ 三个平台上的保鲜机制**本来就不是一件事**（Linux/Windows/macOS 各一套，
macOS 那一格是文献读数、没实测）。跨 target 要判的是「**能力在不在**」，
**不是「新鲜度一样」**（`设计/96 §2.9` 边界②）—— 把它们判成相等会逼人写假声明。

**错误码**：无（它只读自己手上那份索引的计数）。

#### `files-index-rebuild`：走一遍，就一遍，做完返回（步 `24f` 第三刀，**只读**）

```text
→ {"id":"f5","cmd":"files-index-rebuild","args":{"path":"/home/u"}}
← {"kind":"reply","id":"f5","ok":true,"data":{
     "path":"/home/u","entries":20220,"resident_bytes":2544180,
     "unreadable_dirs":3,"truncated":false}}
```

🔴 **它就是 `设计/60 §3.5.2a` 那条裁定里的「机制」那一格**：走一遍、就一遍、做完返回，
**后端里没有任何「隔 N 秒再走一次」**（零定时器铁律，`no_timer_guard` 零容忍）。
「隔多久再叫一次」是**调用方**的事（`K37` 逐字「后端只给机制，不给偏好」），
而后端**声明**的建议周期摆在 `files-index-status` 的 `rewalk_interval_secs` 里。
⚠ ⇒ **不发这条命令，索引就永远不会变新。** 这不是缺陷登记，是这条契约的语义。

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 要走的那个**根**。字符串或 `{"b16":…}`；空 / 形状不对 ⇒ `bad_path`。出方向原样回送 |
| `entries` | ← | 这一趟走出来多少条（目录 ＋ 文件 ＋ 符号链接，根自己不算） |
| `resident_bytes` | ← | 新那份索引在后端内存里占多少字节（路径总长 ＋ 4×条数，**算得出的量**） |
| `unreadable_dirs` | ← | 这一趟有几个子目录读不进去（权限等）。**不是 0 就说明这份索引有洞** |
| `truncated` | ← | 撞到条目上限、没走完 ⇒ 这份索引是**不完整**的 |

🔴 **根本身读不进去是「拒」，不是「走出一棵空树」**：这一趟回 `unreadable`，
而**常驻那一份一个字节都不动**。理由是结构性的 —— 遍历对一个打不开的目录只会把
`unreadable_dirs` 加一并交一份空快照，而这条命令会把常驻那一份**整份换掉**
⇒ 路径打错一个字母就能把手上那份好索引顶成空的，且回参看起来像成功
（`entries: 0` 与「这台机器上真的没文件」同形）。⇒ 换之前先探一次根。

⚠ **它不跟 symlink 往里走**（链接当一条条目收进索引，但不进它指的那棵树）、
**不跨文件系统边界那一档没做**（根底下挂着别的文件系统时会一起走完）、
**建索引的墙钟没有上限保证**（64 万条现打 0.99 秒是**热缓存**；冷缓存没量过）。
⚠ **它没有并发保护**：两个调用方同时发，后完成的那一趟胜出（整份换掉，不合并）。

🔴 **索引是「这一个后端进程」手上的那一份** —— 所以这条命令与 `files-find`
**必须在同一条连接上**（帧面：一条长连接、多次往返）。
⚠ **CLI 面那一对配不起来**，这是现打的读数不是推论：一次 exec = 1 请求 1 响应 1 退出
⇒ `--files-index-rebuild` 建好的那份索引随那个进程一起没了，紧接着的
`--files-find` 那一 exec **照旧回 `index_missing: true`**（本机 debug 档现打：
rebuild 回 `entries:4`，下一个 exec 的 `--files-index-status` 回 `index_missing:true`）。
⇒ CLI 面这一条的用处是**量一趟遍历**（条目数 / 常驻字节 / 有几个目录读不进去），
**不是**给 `--files-find` 预热。要「建完就能查」走帧面。

**错误码**：`bad_path`（`path` 缺了 / 形状不对 / 空）· `unreadable`（这个根打不开 ⇒ **拒**）。

#### `files-browse`：告诉后端「用户现在在看哪几个目录」（步 `24f` 第三刀，**只读**）

```text
→ {"id":"f6","cmd":"files-browse","args":{"dirs":["/home/u/p","/home/u/q"]}}
← {"kind":"reply","id":"f6","ok":true,"data":{
     "added":2,"removed":0,"rejected":0,"browse_watch_cap":64}}
```

它是**保鲜的另一半**（`设计/60 §3.5.2` 那张三段表的第二段）：把「用户眼前那几个目录」
交给后端，后端对它们单独维护一份 overlay —— 查询时 overlay 里那几个目录的直接子项
**盖掉**大索引里的对应条目。

| 字段 | 向 | 说明 |
|---|---|---|
| `dirs` | → | **此刻的整份名单**（数组，每项是字符串或 `{"b16":…}`）。后端自己算差分：不在名单里的卸掉、新来的挂上 |
| `added` | ← | 这一趟新挂上几个 |
| `removed` | ← | 这一趟卸掉几个（用户不再看它们了） |
| `rejected` | ← | 超过上限被**拒掉**几个。🔴 它必须是个数、必须回给调用方：静默截断会让「这个目录我明明在看、新建的文件却要等重走」变成一个查不出原因的现象 |
| `browse_watch_cap` | ← | 上限（今天 64）。只回一个 `rejected` 而不说上限是多少，调用方没法判该少送几个 |

⚠ **收的是「整份名单」，不是「再加一个」。** 空数组**合法**，语义是
「用户现在什么都没在看」⇒ 全卸（`removed` 说出来卸了几个）。
**而「少了 `dirs` 这个参数」是另一件事** ⇒ `bad_args`（别把漏参数当成「要全卸」）。

🔴 **它今天买到的比「那几个目录此后实时」小，逐条别读宽：**

- 它做的是**登记名单 ＋ 当场把那几个目录各重列一遍**。真把 `inotify` 挂上去那一跳
  （后端侧的 `BrowseWatcher`）**至今零生产调用方** —— 那要有人在后端进程里**长期持有**
  那个监听器，是生命周期那一维的活。
  ⇒ 今天这条命令买到的是「**这几个目录的子项在你发命令那一刻是新的**」，
  **不是**「此后一有动静就跟着新」。要更新就再发一次。
- **只盯直接子项**（不递归）：浏览的目录**底下**那棵子树里新建的东西仍然等重走那一档。
- 三个平台的保鲜机制**本来就不是一件事**（Linux `inotify` / Windows
  `ReadDirectoryChangesW` / macOS `FSEvents`，最后一格是文献读数、没实测）。
  跨 target 要判的是「**能力在不在**」，不是「新鲜度一样」（`设计/96 §2.9` 边界②）。

**错误码**：`bad_args`（`dirs` 缺了 / 不是数组）· `bad_path`（数组里**某一项**不是一个路径）。
两个码刻意分得开：前者要改的是调用形状，后者要改的是名单里那一项。

#### `files-read-text`：读一份文本进编辑器（F7a · 第三波，2026-09-24，**只读**）

同族第七条。文件窗口的编辑器此前为这一问单拨一条 SFTP 把字节整份搬过来；现在经通道问后端。

```text
→ {"id":"f7","cmd":"files-read-text","args":{"path":"/home/u/p/a.md","max_bytes":262144}}
← {"kind":"reply","id":"f7","ok":true,"data":{"path":"/home/u/p/a.md","text":"# hi\n","bytes":5}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 要读的那份文件。字符串或 `{"b16":…}`；出方向原样回送（原始字节形） |
| `max_bytes` | → | 🔴 **必须给**：编辑上限是**调用方**的（它答的是「这个文本控件打字卡不卡」）。只收 `1..=8388608`（后端一趟肯交的天花板 8 MiB）；超出 ⇒ `bad_args`，**不替调用方夹小** |
| `text` | ← | 整份内容（合法 UTF-8） |
| `bytes` | ← | 字节数 |

🔴 **超上限整趟拒，不截断**（截断过的文本存回去会写坏文件）。大小判两次：`stat` 出来超了 ⇒ 拒；
真读的时候比 `stat` 时大（文件正在长）⇒ 同样拒 —— 最多只多读一个字节就知道，不会把一个刚变大的文件整个读进内存。

**错误码**：`bad_path`（`path` 缺了 / 形状不对 / 空）· `bad_args`（`max_bytes` 缺了 / 越界）·
`unreadable`（读不到）· `too_large`（超上限，话里带着多了多少字节）·
`not_text`（不是普通文件 / 含 NUL 字节 / 不是合法 UTF-8 —— 不猜编码、不有损替换）。

⚠ 它**不过会话数据围栏**：那道围栏立在写侧；把一份会话记录读进编辑框不改任何东西，
存回去那一下（`files-write-text`）照旧会被拒。〔FN1 · 第四波 4C · 2026-09-25 · 用户 V119〕写侧那道也拿掉了（「文件管理器全部都可以改.
不需要任何围栏」）：会话记录存得回去。

#### `files-home`：后端这个用户的 home（F7a · 第三波，2026-09-24，**只读，不读 stdin**）

同族第八条。文件窗口「开在 home」时要一条绝对路径作起点；此前 monitor 为这一问单拨一条 SFTP 问 `.` 解成什么。

```text
→ {"id":"f8","cmd":"files-home"}
← {"kind":"reply","id":"f8","ok":true,"data":{"path":"/home/u"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | ← | 后端这个进程环境里的 home（原始字节形）。Windows 上先看 `HOME`、没有再看 `USERPROFILE` |

🔴 **说不出就拒，不猜**：没有这一格 / 是空的 / 不是绝对路径 ⇒ `no_home`。拿当前目录或根目录兜底，
就是「窗口开出来了、却开在一个用户没要的地方」。

⚠ **为什么是一条命令而不是往 `hello.homes` 里填一格**：`homes` 声明的是**各 agent 的数据在哪**
（`[{agent_kind, path}]`），不是这个用户的 home；而且填它是一次**跨仓契约变更**（仓外按精确字节读 `hello`）。
一条按需问的命令两样都不碰。

**错误码**：`no_home`。

#### `files-create`：在文件管理目标根底下新建一份**此前不存在**的文件（波 5 ㈠，2026-09-23）

🔴 **这是后端第一条会往用户盘上写东西的线上命令**，而它的射程被刻意收得很窄。
它**不属于** `files-read` 那一族 —— 那一族整族纯读（上面那一节的第 1 条），
本条的处理器住 `control/`（「会改变世界」那一层）。两者只是共用 `files-` 这个线上前缀。

```text
→ {"id":"w1","cmd":"files-create","args":{"root":"/home/u/docs","rel":"note/a.md","content":"hi"}}
← {"kind":"reply","id":"w1","ok":true,"data":{"path":"/home/u/docs/note/a.md","bytes":2}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` | → | **用户指定的那个文件管理目标根**。字符串或 `{"b16":…}`；空 ⇒ `bad_path`。它必须已经在盘上（本命令不建目录） |
| `rel` | → | 相对 `root` 的那一段。字符串或 `{"b16":…}`（〔FW5 · 第四波〕此前只收 UTF-8 字符串；围栏改成按 `Path` 逐段判之后，非 UTF-8 的名字照样逐段过围栏） |
| `content` | → | 要写进去的字节。字符串或 `{"b16":…}`。**不给这个参数 ⇒ 新建一份空文件**（那就是「新建空文件」这件事的形状） |
| `path` | ← | 真正落盘的那个绝对路径，**解完 symlink 的**（原始字节形） |
| `bytes` | ← | 这一趟写进去了几个字节 |

🔴 **它做什么、以及它刻意不做什么**（别读宽）：

- 它只会**新建一份此前不存在的文件**（`O_EXCL`）。目标已经在了（哪怕它只是一条
  symlink）就直接失败，**绝不跟随、绝不覆盖**。
- 它**不**删、**不**改名、**不**建目录、**不**改权限、**不**以截断或追加的方式
  开一个既有文件。这几条不是自律：`readonly_guard` 的白名单层**逐条扫源码**扫着，
  换一种写法那一层当场红。
- `rel` 里有上跳段 / 绝对路径 / 盘符 / 空段 ⇒ 当场拒（`refused`），**一个字节都不落盘**。
- 目标根里藏一条指向别处的 symlink ⇒ 父目录解成真路径之后**再判一次**，
  跑出目标根 或 落进 Claude 的会话数据 ⇒ 拒。

**错误码**：`bad_path`（`root` 缺了 / 形状不对 / 空）· `bad_args`（`rel` 不是字符串、
或 `content` 形状不对）· `refused`（**围栏**拦的 —— 换条路径才有意义）·
`io_failed`（围栏放行了、盘上这一步没成：目标已存在 / 父目录不在或不可写 / 盘满 ——
重试才有意义）。
⚠ `refused` 与 `io_failed` **刻意分得开**：压成一个码，调用方就只能靠猜字符串前缀
去分「这条路径本来就不许写」与「这一次没写成」，而那是会漂的。

⚠ **CLI 面也有它**（`--files-create`），而那不是选择：CLI 面是从
`inbound::REGISTRY` **派生**的（非硬臂即上线）。⇒ 「入口窄」这件事**不靠命令面**，
靠的是后端源码里「谁引用得到那个写模块」那一层护栏。

⚠ **真远端那一维本轮买不到**：这一条在本机文件系统上跑过；
「在一台真远端机器上经这条命令写成过」**没有读数**。

#### 文件管理写面的另外五条（波 5 ㈡，2026-09-23）—— **改动既有数据**

🔴 用户逐字：「**现在只允许后端的文件管理部分写文件**」。下面五条都会改动盘上**已经在**的东西，
它们与上面 `files-create` 同住一个后端模块，而后端的写盘护栏（`readonly_guard`）为它们长出了
**第三层**：能用的改动动词是闭集 · 每一处先过路径解析（〔FN1 · V119〕原文「先过 Claude 会话数据围栏」）· 后端源码里够得到那个模块的
**只有**命令注册那一处。

五条共用的口径（别读宽）：

- `root` 与 `files-create` 同形（字符串或 `{"b16":…}`，必须已在盘上）；相对段（`rel` / `from` / `to`）
  〔FW5 · 第四波〕同样收字符串或 `{"b16":…}`（乱码文件名因此改得了名、删得掉、改得了权限；形状不对 ⇒ `bad_args`，不猜），
  上跳段 / 绝对路径 / 盘符 / 空段 / 当前目录段一律拒（`refused`）。
- 〔FN1 · 第四波 4C · 2026-09-25 · 用户 V119〕**不再有任何数据围栏**：用户原话「文件管理器全部都可以改. 不需要任何围栏」⇒
  会话文件（`projects/<proj>/<sid>.jsonl` · `sessions/<x>.json`）、项目目录、subagent 记录、tasks 与别的文件**同一条路**。
  （此前：「围栏只拦那几份具体的会话文件 …… `~/.claude` 底下的其余东西改得动」，用户 09-23 那一裁。）
  仍会拒的只有**路径解析**那几形：上跳 / 绝对路径 / 空段 / 父目录不在 / 解完链接跑出 `root`（跟链接的动词解到底再判）——
  它不限制改什么，只保证改的就是 `root ＋ rel` 那一格。
- 错误码四个，与 `files-create` 同义：`bad_path` · `bad_args` · `refused`（路径解析拒的 / 形状不对）· `io_failed`（盘上没成）。
- ⚠ **判定与动手之间有一个窗**（TOCTOU），本面没有闭合它。
- ⚠ **真远端那一维没有读数**：五条全在本机文件系统上跑过。
- **CLI 面同样有它们**（从命令注册那一处派生，与 `files-create` 同一条理由）：
  `--files-mkdir` · `--files-rename` · `--files-delete` · `--files-chmod` · `--files-write-text`，
  载荷走 stdin，与帧面的 `args` 同形。

#### `files-mkdir`：新建一个目录

```text
→ {"id":"w2","cmd":"files-mkdir","args":{"root":"/home/u/docs","rel":"new"}}
← {"kind":"reply","id":"w2","ok":true,"data":{"path":"/home/u/docs/new"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 目标根 ＋ 相对段。**只建最后那一段**：父目录不在 ⇒ `refused`（不顺手补中间几层） |
| `path` | ← | 建出来的那个目录（父目录解完 symlink 的） |

#### `files-rename`：改名 / 同根内移动

```text
→ {"id":"w3","cmd":"files-rename","args":{"root":"/home/u/docs","from":"a.md","to":"b.md"}}
← {"kind":"reply","id":"w3","ok":true,"data":{"path":"/home/u/docs/b.md"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` | → | 目标根 |
| `from` / `to` | → | 两个相对段，**各过一遍路径解析**（只解 `from` 的话，`to` 半路一条链接就能把东西搬到根外） |
| `path` | ← | 新名字的落点 |

🔴 **`to` 已经在了 ⇒ 拒（`io_failed`），不覆盖**：unix 上系统那一步会静默顶掉已有目标，
那是一次没人问过的覆盖。看与改之间的窗没闭合，如实写。

#### `files-delete`：删一个文件或一个**空**目录（显式 `recursive` 才删整棵树）

```text
→ {"id":"w4","cmd":"files-delete","args":{"root":"/home/u/docs","rel":"old.md"}}
← {"kind":"reply","id":"w4","ok":true,"data":{"path":"/home/u/docs/old.md","removed":1}}
→ {"id":"w4b","cmd":"files-delete","args":{"root":"/home/u/docs","rel":"build","recursive":true}}
← {"kind":"reply","id":"w4b","ok":true,"data":{"path":"/home/u/docs/build","removed":37}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 目标根 ＋ 相对段。**删的是链接本身**，不跟过去 |
| `recursive` | → | 〔FW5 · 第四波〕布尔，**缺省 `false`**。不给 ⇒ 射程与此前一个字节不差（非空目录 ⇒ `io_failed`）；给了不是布尔 ⇒ `bad_args`（不猜） |
| `expect` | → | 〔RM1e · 第四波〕可选，字符串或 `{"b16": …}`：「我读到的是这一份」。给了 ⇒ 目标必须是一份**普通文件**（目录 / 链接 ⇒ `refused`），盘上逐字节等于它才删；不等或已经不在 ⇒ `stale`，一个字节不动。`null` ⇒ `bad_args`；与 `recursive: true` 同给 ⇒ `bad_args`。不给 ⇒ 行为不变 |
| `path` | ← | 删掉的那一项 |
| `removed` | ← | 这一趟真删掉了几条（含目标自己；不递归那一支恒 `1`） |

🔴 **不带 `recursive` 就不递归，刻意的**：路径解析的射程是一条路径，递归删动的是整棵子树。
（〔FN1 · V119〕此前这里的理由是「底下藏着的一份会话文件照样被一起删掉」；那道拦截用户拿掉了，会话文件照删。）

🔴 **带 `recursive: true` ⇒ 逐条目过路径解析**（〔FW5〕后端 `control/files_write.rs::delete_tree`）：
- **计划趟（只读）**：不跟链接地走整棵树，**每一条目各过一次路径解析**。任一条被拒（〔FN1〕从前含「树里藏着一份会话文件」，今天不含）·
  跨了挂载点（unix）· 超过 10 万条 ⇒ **整趟拒（`refused`），一个字节不动**，话里点名是哪一条。
- **执行趟**：按计划倒序逐条删，**每一条当场再过一次路径解析**；只删计划里的 ⇒ 计划之后新长出来的东西不被连带删掉
  （它所在的目录不空 ⇒ 停，`io_failed`，话里带「删了几条之后停在哪」）。
- 不用那个一步递归删的库函数（它的遍历不经过路径解析）；树里的链接只删链接本身，不走进去。
- ⚠ 每一条「判」与「删」之间仍有窗（TOCTOU），如实登记；执行趟中途停下 ⇒ 已删的删掉了（与任何递归删同形）。

#### `files-chmod`：改 unix 权限位

```text
→ {"id":"w5","cmd":"files-chmod","args":{"root":"/home/u/docs","rel":"run.sh","mode":493}}
← {"kind":"reply","id":"w5","ok":true,"data":{"path":"/home/u/docs/run.sh","mode":493}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 目标根 ＋ 相对段 |
| `mode` | → ← | **十进制数值**（`493` = `0o755`），只收低 12 位；超出 ⇒ `refused`。出方向原样回送 |
| `path` | ← | **解到底**的那个真路径 |

🔴 **它跟链接** ⇒ 落点连最后一段也解到底再判一次：根里一条指向根外的链接不许借它把根外那一份改掉（〔FN1〕此前的例子是「指向会话文件的链接」，那一形今天放行）。
⚠ 非 unix 平台上**如实回 `no_unix_mode`**（〔FW5 · 第四波〕此前回 `io_failed` —— 那是「重试才有意义」的码，说错了），不假装改成了。
这个码同时是本命令**声明过**的命令级码：后端 target 轴（`lib.rs::capabilities_on`）从它现推「Windows 上没有这一条」，
差异登记表 `TARGET_GAPS` 里有对应两行（帧面 · CLI 面，档 = 结构）。

#### `files-write-text`：覆盖写一份**已经在**的普通文件

```text
→ {"id":"w6","cmd":"files-write-text","args":{"root":"/home/u/docs","rel":"a.md","content":"new text"}}
← {"kind":"reply","id":"w6","ok":true,"data":{"path":"/home/u/docs/a.md","bytes":8}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 目标根 ＋ 相对段。目标必须**已经在、且是普通文件**；新建请走 `files-create`（`O_EXCL`，两条路刻意分开） |
| `content` | → | 字符串或 `{"b16":…}`。🔴 **必须给** —— 不给不默认成空（那等于把那份文件清空） |
| `path` | ← | **解到底**的那个真路径（它跟链接，理由同 `files-chmod`） |
| `bytes` | ← | 写进去了几个字节 |

⚠ 没有大小上限、没有「写之前那一版」的备份 —— 本面只做「写」这一件，编辑器的那些语义不在它里面。

#### `files-commit-upload`：把暂存区里一份传完的上传件挪进目标（F7c，2026-09-24）

```text
→ {"id":"w7","cmd":"files-commit-upload","args":{"key":"0123456789abcdef0123456789abcdef","root":"/home/u/docs","rel":"a.bin","overwrite":false}}
← {"kind":"reply","id":"w7","ok":true,"data":{"path":"/home/u/docs/a.bin","bytes":1048576}}
```

SFTP 缩成只做传输之后（`设计/60 §13`），上传**只写** `~/.cc-monitor/staging/<key>.part`；
把它挪进用户目标的那一下**只有这条命令**（用户逐字「现在只允许后端的文件管理部分写文件」）。

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 暂存件的键：**恰好 32 位小写十六进制**。暂存件路径由后端自己拼（`$HOME/.cc-monitor/staging/<key>.part`），调用方指不到暂存区之外任何文件 |
| `root` / `rel` | → | 目标根 ＋ 相对段，先过写面那两道路径解析（词法 ＋ 父目录解 symlink；〔FN1〕「会话文件那一问」删了） |
| `overwrite` | → | 🔴 **必须给**（`true` / `false`），不给默认值。`false` ⇒ 先 `O_EXCL` 占位（目标已在 ⇒ `io_failed`），再改名上位；`true` ⇒ 直接改名上位（同盘原子） |
| `path` | ← | 落点（父目录解过 symlink 的那一个） |
| `bytes` | ← | 暂存件的字节数（改名不搬字节，这个数就是它在盘上的长度） |

⚠ 暂存件不在（传输没跑完 · SFTP 起始目录不是这台后端的 home）⇒ `io_failed`；暂存件是链接或目录 ⇒ `refused`。
⚠ 暂存区与目标不在同一个文件系统上 ⇒ 改名回 `EXDEV`、`io_failed`（复制 ＋ 删那一形在第三层禁表里，没做）。
- **CLI 面同样有它**（从命令注册那一处派生，与写面同一条理由）：`--files-commit-upload`，载荷走 stdin，与帧面的 `args` 同形。

#### `files-stage-chunk`：存盘的一块进暂存区（F9c · 第四波，2026-09-24）

```text
→ {"id":"w8","cmd":"files-stage-chunk","args":{"key":"0123456789abcdef0123456789abcdef","seq":0,"content":"……"}}
← {"kind":"reply","id":"w8","ok":true,"data":{"bytes":1048000}}
```

文件窗口存一份**装不进一条请求行**的文本（入方向一行上限 1 MiB）时，先把它切成几块逐块送来，
每块落成 `$HOME/.cc-monitor/staging/<key>.<seq>.chunk`（暂存区不在就建）。块只进我们自己的暂存区，
落进用户文件的那一下是下一条 `files-commit-text`。

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 这一次存盘的键：**恰好 32 位小写十六进制**（调用方每次存盘现造一个） |
| `seq` | → | 块号，从 0 起的非负整数 |
| `content` | → | 字符串或 `{"b16":…}`，**至少 1 字节**（空块 ⇒ `bad_args`） |
| `bytes` | ← | 这一块写进去的字节数 |

⚠ `O_EXCL` 新建：同一 `key` 的同一块已经在（重发 / 撞键）⇒ `io_failed`，一个字节不盖。写到一半失败 ⇒ 删掉自己刚建的那一份。
- **CLI 面同样有它**：`--files-stage-chunk`，载荷走 stdin，与帧面的 `args` 同形。

#### `files-commit-text`：按块读回、拼起来、原地覆盖（F9c · 第四波，2026-09-24）

```text
→ {"id":"w9","cmd":"files-commit-text","args":{"key":"0123456789abcdef0123456789abcdef","chunks":3,"bytes":3000000,"root":"/home/u/docs","rel":"big.log"}}
← {"kind":"reply","id":"w9","ok":true,"data":{"path":"/home/u/docs/big.log","bytes":3000000}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 与 `files-stage-chunk` 同一个键 |
| `chunks` | → | 块数：读回 `0..chunks` 这几块。只收 `1..=bytes`（每块至少 1 字节） |
| `bytes` | → | 拼起来**必须恰好**这么长；最多 8 MiB（`files-read-text` 一趟的天花板：存得回的要读得回来），超了 ⇒ `bad_args` |
| `root` / `rel` | → | 目标，语义与 `files-write-text` **完全相同**：必须已经在、是普通文件；跟链接（解到底再判一次）；原地覆盖（权限位 / 属主不变） |
| `path` | ← | 解到底的那个真路径 |
| `bytes` | ← | 写进去的字节数 |

⚠ 少一块 · 多出第 `chunks` 块 · 总长对不上 ⇒ `io_failed`，目标**一个字节没动**；某一块是链接或目录 ⇒ `refused`。
⚠ **不论成败**都删掉这一次的块；成功时顺手扫一遍暂存区的孤儿（`<key>.part` 与 `<key>.<seq>.chunk` 两种形状，7 天没动过的）。
⚠ 为什么不是「改名上位」（`files-commit-upload` 那种）：改名会换掉权限位 / 属主、把链接本身换成普通文件、跨盘时 `EXDEV` 失败 —— 同一份文件 1 MiB 上下会存出两种结果。
- **CLI 面同样有它**：`--files-commit-text`，载荷走 stdin，与帧面的 `args` 同形。
#### `files-copy`：同根内复制一份普通文件（F7a · 第三波，2026-09-24）

写面第七条。文件窗口的「复制」此前走 SFTP 那条零流量复制；现在经通道问后端 —— 复制发生在
那台机器上、字节不过网，**「退回中转、花 2× 流量」那一形从此不存在**。

```text
→ {"id":"w7","cmd":"files-copy","args":{"root":"/home/u/docs","from":"a.md","to":"a.md.copy"}}
← {"kind":"reply","id":"w7","ok":true,"data":{"path":"/home/u/docs/a.md.copy","bytes":1234}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` | → | 目标根（与写面其余几条同形） |
| `from` / `to` | → | 两个相对段。`from` **解到底**再过路径解析（复制是一次跟链接的读：根里一条指向根外的链接不许借它把根外那一份复制进来）；`to` 只解父目录（作用在链接本身上） |
| `overwrite` | → | 🔴 **覆盖策略显式**。不给 / `false` ⇒ `O_EXCL` 直接开目标，目标已在（含它只是一条链接）⇒ `io_failed`、一个字节不动；`true` ⇒ 先写进同目录一个暂存旁名（`O_EXCL`，它自己也过围栏），写满再换名上位 —— **原子地**顶掉目标（顶掉的是链接本身，不跟过去）。不是布尔 ⇒ `bad_args` |
| `path` | ← | 落点（父目录解完 symlink 的） |
| `bytes` | ← | 复制了几个字节 |

- **只收普通文件**：源是目录 ⇒ `refused`（目录递归复制没做，理由同 `files-delete` 不递归）。源与目标是同一份 ⇒ `refused`。
- 写到一半失败 ⇒ 删掉**这一趟自己刚建的那一份**（暂存旁名或新目标），原样带回原因（`io_failed`）。
- ⚠ 新文件的权限位是后端进程的缺省（受 umask），**不从源那里抄**；覆盖时旧目标的权限位随它一起换掉。
- ⚠ 同写面其余几条：判定与动手之间的窗（TOCTOU）没闭合；真远端那一维没有读数（本机文件系统上跑过）。
- **CLI 面同样有它**（`--files-copy`，载荷走 stdin），理由与写面其余几条相同。

**错误码**：`bad_path` · `bad_args` · `refused`（围栏拦的 / 源不是普通文件 / 源即目标）· `io_failed`（盘上没成：目标已在且没说覆盖 / 读写中断 / 换名失败）。

#### `files-peek`：读改写的**读那一半**（RW1 · 第四波，2026-09-24）

用户裁「只允许后端的文件管理部分写文件」**只管用户的文件、本机也管** ⇒ monitor 进程不再直接写用户文件
（rc 里的别名块 · PowerShell `$PROFILE` · 项目 `.mcp.json` · skill 收件箱 · `~/.claude/skills/cc-bus/` ·
cc-bus 收件箱）。这些改动都是「读 → 算 → 写回」，本机与远端**同一条路**：先 `files-peek`、再 `files-put`。

```text
→ {"id":"p1","cmd":"files-peek","args":{"root":"/home/u","rel":".bashrc"}}
← {"kind":"reply","id":"p1","ok":true,"data":{"path":"/home/u/.bashrc","exists":true,"text":"# my rc\n"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 与写面其余几条同形；**与 `files-put` 同一道围栏**（读的那一份就是写的那一份） |
| `exists` | ← | `false` ⇒ **确定不存在**（`text` 为 `null`）。「读不出来」不是这一形，是 `io_failed` |
| `text` | ← | 全文（UTF-8）；不在时 `null` |
| `path` | ← | 读的是哪一份（最后一段是链接时是解到底的那一份） |

⚠ 上限 256 KiB（`files_write::PEEK_MAX_BYTES`）：写那一半要把新内容与读到的那一份装进同一行请求，
而一行上限是 1 MiB ⇒ 读得回来的写得回去。超了 ⇒ `too_large`，不截断。不是 UTF-8 ⇒ `not_text`；不是普通文件 ⇒ `refused`。

**错误码**：`bad_path` · `bad_args` · `refused` · `io_failed` · `not_text` · `too_large`。

#### `files-put`：整份替换一份文本文件（RW1 · 第四波，2026-09-24）

```text
→ {"id":"p2","cmd":"files-put","args":{"root":"/home/u","rel":".bashrc","content":"# my rc\nnew\n","expect":"# my rc\n","backup":true}}
← {"kind":"reply","id":"p2","ok":true,"data":{"path":"/home/u/.bashrc","bytes":13,"changed":true,"created":false,"backup":"/home/u/.bashrc.ccm-backup-1727150000000-0"}}
```

🔴 **用户文件的写规则只有这一份**（monitor 那一侧从前的 `fenced_block::apply` 搬到了这里）：
CAS → 相同不写 → 备份 → 同目录 `O_EXCL` 暂存旁名写满、换名上位 → 回读逐字节比对 → 不符回滚。

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 同 `files-peek`。最后一段是链接 ⇒ 解到底、**改真文件**（不把用户的链接换成一份普通文件） |
| `content` | → | 新全文（字符串或 `{"b16":…}`），**必须给** |
| `expect` | → | 🔴 **必须给**：`null` = 「我读的时候它不在」；字符串 / b16 = 「我读到的就是这一份」。盘上那份对不上 ⇒ `stale`、一个字节不写（重读重算，别重发同一份） |
| `backup` | → | 可缺席的布尔（缺省否）。真 ⇒ 原文非空时另存 `<名>.ccm-backup-<毫秒>-<序号>`（`O_EXCL`，沿用原权限位） |
| `parents` | → | 可缺席的布尔（缺省否）。真 ⇒ 父目录不在就逐级建（每一级各过一遍围栏） |
| `path` | ← | 落点 |
| `bytes` | ← | 新内容的字节数 |
| `changed` | ← | 真的写了吗（新内容与盘上逐字节相同 ⇒ `false`，一个字节不动） |
| `created` | ← | 这份文件是这一次新建的 |
| `backup` | ← | 备份落在哪；没备份 ⇒ `null` |

- 替换沿用原文件的权限位；新建的文件是后端进程的缺省（受 umask）。
- 回读不符 ⇒ 回滚（原来在 ⇒ 原文换回去；原来不在 ⇒ 删掉刚建的），`io_failed` 的话里说清恢复成没成。
- ⚠ CAS 之后、换名之前那一个窗（TOCTOU）没闭合：窗里被别人改了，那次改动会被盖掉。CAS 缩小的是「monitor 读 → 后端写」那一整趟往返的窗。
- **CLI 面同样有它们**（从命令注册那一处派生）：`--files-peek` · `--files-put` · `--files-delete-session`，载荷走 stdin。

**错误码**：`bad_path` · `bad_args` · `refused`（围栏拦的 / 不是普通文件）· `io_failed` · `stale`（读改写之间盘上那份变了）。

#### `files-delete-session`：删一份历史会话 —— **只收 sid**（RW1 · 第四波，2026-09-24；〔FN1〕原标题「会话文件围栏唯一的例外」）

```text
→ {"id":"p3","cmd":"files-delete-session","args":{"sid":"2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77"}}
← {"kind":"reply","id":"p3","ok":true,"data":{"path":"/home/u/.claude/projects/-home-u-x/2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77.jsonl"}}
```

历史浏览器里「删掉这场会话」走这一条，本机与远端同一条路（此前本机是 monitor 进程直删、远端是 SFTP 直删）。
〔FN1 · V119〕此前这里写「写面其余每一条都被会话文件围栏挡在 `projects/<proj>/<sid>.jsonl` 外面，这是唯一能删它的一条」——
文件管理写面今天也删得掉会话文件；这一条的独特之处只剩「只收 sid、落点由后端按 sid 找、必须是会话记录的形状」。

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 🔴 **只收 sid**：多给任何一个键 ⇒ `bad_args`。落点由后端在它自己的记录树里按 sid 找（与分叉那条同一份「找」），解到底必须恰是 `<项目>/<sid>.jsonl` 且在记录树里 |
| `path` | ← | 删掉的那一份 |

**错误码**：`bad_args` · `refused`（sid 形状不对 / 没找到 / 解完链接跑出记录树 / 不是那一形）· `io_failed`。

#### 「退出行为」那个值（B2 · 条 66，`设计/01 §3.3b`，2026-09-24）—— **值住后端所在那台机器**

那个值（「最后一个客户走了之后，这台机器的后端退不退」）住后端所在那台机器的 `~/.cc-monitor/backend.json`，
**只有后端写**。前端要读要改都经下面两条命令；monitor 自己的 `config.json` 里**不再有它**，
monitor 进程内也**不再有它的副本**（原来那条「启动时 / 改动时把生效值推给 monitor」的 tauri 命令整条退役）。

两条命令回同一个形状（〔S5 · 第四波〕原来还有一格 `shell`，恒 `"standalone"`；「折进前端进程」那一档已放弃（V105），那一格随之删掉）：

| 字段 | 向 | 说明 |
|---|---|---|
| `state` | ← | 三态：`"chosen"`（有人选过）· `"absent"`（文件不在 = 没人选过）· `"unreadable"`（文件在但读不出来 / 家目录解析不出来）。🔴 后两态**不许合并** —— 「读不出来」不等于「有人选了默认」 |
| `killOnExit` | ↔ | 生效值。`chosen` 时是选的那个；另两态是缺省 `false`（不结束）。`exit-policy-set` 的入参也是它 |
| `reason` | ← | 只在 `unreadable` 时有：为什么读不出来。其余为 `null` |
| `path` | ← | 那份文件的绝对路径（家目录解析不出来时 `null`） |

#### `exit-policy-read`：现读一次（**不读 stdin**）

```text
→ {"id":"x1","cmd":"exit-policy-read"}
← {"kind":"reply","id":"x1","ok":true,"data":{"state":"absent","killOnExit":false,"reason":null,"path":"/home/u/.cc-monitor/backend.json"}}
```

**没有错误码**：读不出来是一个**状态**，照样 `ok:true` 回 `state:"unreadable"` ＋ `reason`。
每一次都真去读盘，没有缓存（用户可能刚从另一台 monitor 改过它）。

#### `exit-policy-set`：写那个值，写完读回

```text
→ {"id":"x2","cmd":"exit-policy-set","args":{"killOnExit":true}}
← {"kind":"reply","id":"x2","ok":true,"data":{"state":"chosen","killOnExit":true,"reason":null,"path":"/home/u/.cc-monitor/backend.json"}}
```

回的是**写完之后再读一遍**的那一份（盘上的事实，不是「我以为写进去了」）。
写法：`O_EXCL` 新建一份临时文件 → 写满 → 原子挪到目标上（读者只会看到整份旧的或整份新的）；
`~/.cc-monitor` 不在就建那一层。

**错误码**：`bad_args`（`killOnExit` 缺了或不是布尔）· `io_failed`（家目录解析不出来 / 盘上没写成）。

⚠ **CLI 面也有它们**（`--exit-policy-read` / `--exit-policy-set`），同 `files-create` 那一条理由：CLI 面从 `inbound::REGISTRY` **派生**，不是选的。

**谁按它动手**（两处，都是「最后一个客户走了」那一刻现读）：
① 常驻（脱离）那条载体上，那一条流断了 ⇒ 后端自己现读、`true` 就退出；
② monitor 退出时现问一次本机后端（`exit-policy-read`），按答案收它自己起的那两个子进程（被监护的后端 ＋ 本机中转）。
⚠ `设计/01 §3.3b ⑦` 的 `lingerMs`（归零后等一下再决定）**没做**：那是一个会自己醒来的构件，后端零定时器铁律不放行，已上报。
⚠ **远端**那一行的值写得进去、读得回来，但今天**没有任何进程按它动手**：远端后端只走 stdio（SSH exec），
它没有「最后一个客户走了」那一臂，SSH 一断它本来就随管道破裂退出。

#### 上游选择那份凭据文件在「这台机器」上的读写（RM1a · 第四波，2026-09-24）—— **上游选择自己的状态，不是用户文件**

第三方 API key 那份文件（`apikey-credentials.json`）归**上游选择**：名字、格式、落点都是本仓定的，
只有中转进程里的上游选择读它 ⇒ 它**不走**文件管理那一面（那一面是给用户文件的），
写口登记在后端 `readonly_guard` 的**第四层**（后端自有状态文件），只从下面 `apikey-key-set` 一条进来。
判清的全文住 `调研/第四波记录/RM1a.md §1`。

- **每台机器上的程序写者恰好一个**：**那台的后端**（本节两条）—— 〔GP1 · 第四波〕monitor 所在那台也一样，是本机常驻后端
  （monitor 把 `apikey-key-set` 发给本机那条连接，与远端同一条路；monitor 一个字节都不写）。
  发之前 monitor 先问一次 `apikey-read`：本机后端答的 `path` 必须就是这个 monitor 认的那一份（`CCM_DATA_DIR` 隔离跑时
  接上的可能是别的数据目录起的那个常驻后端），不等 ⇒ 不写。〔RM1a 那一版这里写「monitor 所在那台是 monitor 自己；
  monitor **从不**把 `apikey-key-set` 发给本机那条连接」。〕
- **路径**与那台机器上 `--relay` 进程的上游选择**同一个出处**（`accounts::upstream::creds::resolve_path` ＋ 同一个家目录）。
- 🔴 **明文只在 `apikey-key-set` 的 `args.key` 里**：不进 argv、不进 env、不进任何日志；两条的应答都只有**掩码**。
- 两条都**不起中转**；中转那两条（`relay-*`）也**不碰凭据**。

#### `apikey-key-set`：给一个账号写 key，写完读回

```text
→ {"id":"k1","cmd":"apikey-key-set","args":{"account":"work","key":"<明文>","baseUrl":"https://api.example.com"}}
← {"kind":"reply","id":"k1","ok":true,"data":{"account":"work","path":"/home/u/.claude/claudecode-frontend/apikey-credentials.json","masked":"sk-a****wxyz","baseUrl":"https://api.example.com"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `account` | ↔ | 账号 id（monitor 用全仓唯一那份规则从账号目录推出来，后端不再推）。必须当得了路由段 —— 与上游选择装表时**同一个谓词**，写得进去却装不进表 = 那一行永远 404 |
| `key` | → | 明文。空串拒 |
| `baseUrl` | ↔ | 入（可选）：这个账号的第三方端点。缺席 / `null` / 空串 = **不碰那一格**（只配 key 时已有端点原样留着）；给了就先过**那一条**形状关（`creds_core::store::check_base_url_shape`；〔GP1〕写者只剩后端这一处），不对 ⇒ `bad_args`、整次不写。出：写完读回这一行的端点（没有 ⇒ `null`） |
| `masked` | ← | 写完**再读一遍**、这一行 key 的掩码（盘上的事实） |
| `path` | ← | 那份文件的绝对路径 |

写法：**写的那一刻读盘** → 只改 `accounts.<account>` 的 `api_key`（与给了的 `base_url`）那一两格（别的行与未知键一个不动）→ 临时文件**出生即只给本人**（O_EXCL）→ 写满 → 落盘 → 原子改名；失败删自己的临时文件。
**错误码**：`bad_args`（缺字段 / 账号 id 当不了路由段 / key 空）· `bad_file`（现有文件解析不了 ⇒ **不覆盖**，人手编的内容不许被抹掉）· `io_failed`。

#### `apikey-read`：文件级的状态（**不读 stdin**）

```text
→ {"id":"k2","cmd":"apikey-read"}
← {"kind":"reply","id":"k2","ok":true,"data":{"configured":false,"masked":"","path":"…/apikey-credentials.json","notice":null,"problem":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `configured` · `masked` | ← | **顶层那一把**（历史格式那一行）配没配、掩码 |
| `path` | ← | 那份文件的绝对路径 |
| `notice` | ← | 权限过宽 / 查不出来时的一句话（文件不在时 `null`）|
| `problem` | ← | 读不动 / 解析不了时的一句话。🔴 **解析不了不退化成「没配」** |

**没有错误码**：读不动是一个**状态**（`problem`），照样 `ok:true`。界面经 `chan.call` 直接问它、按形状严格收（金样 `tests/__fixtures__/apikey.golden.json`）。
〔US1 · 4D〕先前还回 `rows`（表里有哪几行）：它只给 monitor 起会话那一侧用，而那一侧的判断搬进了下面的 `launch-endpoint` ⇒ 退出线上。

⚠ **CLI 面也有它们**（`--apikey-key-set` / `--apikey-read`），从 `inbound::REGISTRY` 派生；`--apikey-key-set` 的入参**从 stdin 读**。

#### `apikey-routing`：这几个号在这台的表里有没有行 · 这台的中转在不在（US1 · 4D）

```text
→ {"id":"k3","cmd":"apikey-routing","args":{"agent":"claude-code","configDirs":["/h/.claude-accts/work","/h/.claude-accts/me"]}}
← {"kind":"reply","id":"k3","ok":true,"data":{"routed":["/h/.claude-accts/work"],"running":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 这一家 agent 的路由名（凭据文件的行只属于 `claude-code`；别家 ⇒ `routed` 恒空）|
| `configDirs` | → | 要问的那几个号的配置目录（id 由后端按 `acct-core` 那一份规则推，前端一个字都不推）|
| `routed` | ← | 传进来的里面、**表里有对应行**的那几个（原样回）。「表里有行」= 上游选择装表真收进表的那几行（`base_url` 坏的那一行不算）|
| `running` | ← | 这台机器上**我们的**中转在不在听（与 `relay-status` 同一个判准）|

**错误码**：`bad_args`。界面经 `chan.call` 直接问（金样同上）。

#### `launch-endpoint`：这个号这一发走哪、注入什么（US1 · 4D）

起会话那一侧（本机与远端同一条）问一次：往 `ANTHROPIC_BASE_URL` 里写哪个中转地址，或者不写。决策表是 `设计/20 §3.2` 那一张（上游选择 `accounts/upstream/endpoint.rs::decide_launch` 是唯一实现）。

```text
→ {"id":"k4","cmd":"launch-endpoint","args":{"agent":"claude-code","account":{"kind":"named","configDir":"/h/.claude-accts/work"},"key":"<sid 或 nonce>","allSessions":false}}
← {"kind":"reply","id":"k4","ok":true,"data":{"baseUrl":"http://127.0.0.1:8788/s/claude-code/work/<key>","listening":true,"whenDown":"refuse","account":"work"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 这一家 agent 的路由名（第 1 段）|
| `account` | → | `{"kind":"named","configDir":…}` · `{"kind":"base"}` · 缺席 / `null`（没表态）|
| `key` | → | 第 3 段（流标签）：resume ⇒ sid；新开 ⇒ 起会话那一侧铸的 nonce。过不了段闸 ⇒ `bad_args` |
| `allSessions` | → | 全量注入开关（`/t/` 那几格；默认关）|
| `baseUrl` | ← | 注入的地址（不带钥匙；渲染成 `$(cat "$HOME/.cc-monitor/relay-key")` 那一形是起会话那一侧的事）；`null` = 不注入 |
| `listening` | ← | 这台机器上我们的中转在不在听（只在 `baseUrl` 非空时探；为空时 `false`）|
| `whenDown` | ← | 中转不在时：`refuse`（`/s/`，拒绝起会话）· `direct`（`/t/`，照旧直连）；`baseUrl` 为空时 `null` |
| `account` | ← | `/s/` 那一格的表 id（拒绝时点名用）；否则 `null` |

四个键恒在（形状恒定）。**错误码**：`bad_args`。远端「中转不在就起、有界等」那一截要定时器 ⇒ 在 monitor（`relay-status` → `relay-ensure` → 再问）。

⚠ **CLI 面也有它们**（`--apikey-routing` / `--launch-endpoint`），从 `inbound::REGISTRY` 派生，入参从 stdin 读。

#### 中转在「这台机器」上的进程（RM1a · 第四波，2026-09-24）

〔RL1 · V107〕本机的中转住**本机常驻后端进程里**（monitor 起本机后端时交 `CCM_RELAY_PORT`，见 `--relay` 那一条下的「进程内」一格）；
**远端那台机器上的中转由那台的后端起**（下面两条）—— 远端后端随 SSH 退、远端会话活得比 SSH 长，中转必须是脱离的那一个。
两条都**只收端口**，一个凭据 / 账号的名字都不经过它们（上游选择那份文件由上面 `apikey-*` 两条管）。
monitor **从不**对本机那条连接发 `relay-ensure`（本机那一个就在本机后端进程里，不许再起第二个去抢口）。

起出来的 `--relay` 继承后端的环境，它里面的上游选择与后端账号域那份写口按同一个函数、同一个家目录出处解凭据路径
⇒ 两边是同一份文件，不必在这两条命令里传路径。

#### `relay-status`：这个口上**我们的中转**在不在听（只读）

```text
→ {"id":"r1","cmd":"relay-status","args":{"port":8788}}
← {"kind":"reply","id":"r1","ok":true,"data":{"port":8788,"listening":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `port` | ↔ | 中转的端口（monitor 那边的 `payload::RELAY_PORT` 是权威，入参给出） |
| `listening` | ← | 〔RK1〕口上有人在听，**而且是我们的中转**：读这台机器上的中转钥匙文件（`~/.cc-monitor/relay-key`），带对的钥匙打一次 `GET /<钥匙>/` 得 404、带一把同形错钥匙得 403 才算。口上有人、而这台还没有钥匙文件 ⇒ `false`（刚起的中转在「绑上口」与「钥匙落盘」之间的窄窗）。⚠ 钥匙**不出线** |

**错误码**：`bad_args`（`port` 缺了或不在 1–65535）· `not_ours`（〔RK1〕口上有人、有钥匙文件，但差分探针对不上 —— 多半是升级前起的旧中转或别的程序；`message` 说出两次各得了什么）。

#### `relay-ensure`：没人在听就起一个脱离的中转

```text
→ {"id":"r2","cmd":"relay-ensure","args":{"port":8788}}
← {"kind":"reply","id":"r2","ok":true,"data":{"port":8788,"listening":false,"started":true,"pid":4242}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `port` | ↔ | 同上 |
| `listening` | ← | 起之前那一刻口上是不是**我们的中转**（是 ⇒ 什么都不做，`started:false`；判法同 `relay-status`） |
| `started` | ← | 这一趟起了一个进程。⚠ **不等它 bind**（后端零定时器）：`true` 只说「进程起了」，要知道口上有没有人再问一次 `relay-status` |
| `pid` | ← | 只在 `started:true` 时有 |

起法：本后端这个二进制自己带 `--relay`，环境多一格 `CCM_RELAY_PORT`，stdio 全接空，自成一个进程组（SSH 断了它不跟着走）。
⚠ 它的诊断因此到不了人（远端那台上没有监护者收它的 stderr）。
**错误码**：`bad_args` · `not_ours`（〔RK1〕口上有人，但不是我们的中转 —— 含「有人在听而这台没有钥匙文件」；不起、不抢口）· `spawn_failed`（找不到自己 / 起不动）· `unsupported`（非 unix：不知道怎么起成脱离的一组，没起）。

〔RK1〕起出来的中转**绑上口之后**读回或铸这台机器上的钥匙（`~/.cc-monitor/relay-key`，`0600`），之后每条请求路径的第一段必须是它（`INVARIANTS §48.1`）。

⚠ **CLI 面也有它们**（`--relay-status` / `--relay-ensure`），入参从 stdin 读。

#### `footprint-probe`：「足迹」的这台机器那一半（RM1a · 第四波，2026-09-24，**只读**）

设置里「足迹」那一块（cc-monitor 在这台机器上碰过哪些文件）的远端那一半：**判定只住 monitor**
（`config_surface::build_rows`：哪一行属于哪个工具、存在 / 缺失 / 查不动怎么分），后端只交**路径事实**。
monitor 问两趟：先空问一趟拿环境（它要用那台的家目录解 `~/…`），再把解出来的路径一次问完。

```text
→ {"id":"f1","cmd":"footprint-probe","args":{"stat":["/home/u/.local/bin/ccm"],"hooks":{"paths":["/home/u/.claude/settings.json"],"needles":["cc-register"]}}}
← {"kind":"reply","id":"f1","ok":true,"data":{"env":{"home":"/home/u","path":"/usr/bin:/bin","agentHome":"/home/u/.claude","agentHomeIsDir":true},"stat":{"/home/u/.local/bin/ccm":{"kind":"file","size":1234}},"hooks":{"/home/u/.claude/settings.json":true}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `stat` | ↔ | 入：一组**绝对**路径（最多 256 条）。出：逐条 `{kind:"file",size}` / `{kind:"dir",size:0,entries:[一层文件名]}` / `null`（不在或读不动）。目录名字超过 4096 个 / 列不动 ⇒ `entries:null` ＋ `notice`（**不截断**） |
| `hooks` | ↔ | 入：`{paths, needles}`（一组绝对路径 · 最多 16 个非空字样）。出：逐条文件**有没有**任何一个字样（`true`/`false`），读不动 / 超过 1 MiB / 不是文件 ⇒ `null`。**文件内容一个字节都不回** |
| `notices` | ← | `hooks` 里答 `null` 的那几条各自为什么（不在 / 太大 / 读不动）。`stat` 里列不动的目录那一格自带 `notice` |
| `env` | ← | 这个**后端进程**看到的 `home`（`HOME`，没有再退 `USERPROFILE`）· `path`（`PATH`）· `agentHome`（agent 的家目录，同帧面其余几条的出处）· `agentHomeIsDir`。⚠ 用户交互 shell 的 rc 改过的环境这里看不见 |

**错误码**：`bad_args`（不是数组 / 相对路径 / 给了路径没给字样）· `too_large`（超过条数上限）。
⚠ **CLI 面也有它**（`--footprint-probe`），入参从 stdin 读。

#### `mcp-sync-plan`：MCP 资产同步的判定（AS1 · 第四波 4B，2026-09-24，**只读**）

用户裁（`设计/96` 的 B）：「各管各的，只有显式推 / 拉」· 推 / 拉之前先给看差异，对面有不同就问盖不盖 ·
「内容，原样拷过去并标出可疑项」、不替用户改写。本条是那一件的**判定**：两份原文进，差异 ＋ 可疑项 ＋「写哪几条」出。
由**要被写的那一台**的后端跑 —— 可疑项里「有没有这个路径 / 这个命令」是那台机器上的事实。
原文由调用方经 `files-peek` 读来，写经 `files-put`（`expect` = 看差异时读到的那一份）；**本条一个字节都不读、不写用户文件**。

```text
→ {"id":"m1","cmd":"mcp-sync-plan","args":{"source":"{\"mcpServers\":{\"fs\":{\"command\":\"/opt/fs\"}}}","target":null}}
← {"kind":"reply","id":"m1","ok":true,"data":{"rows":[{"name":"fs","state":"new","suspects":[{"kind":"abs-path","field":"command","value":"/opt/fs","there":"absent"}]}],"write":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `source` | → | 拷出来的那一份原文（字符串，必给） |
| `target` | → | 要写进去的那一份原文；那份文件不存在 ⇒ `null`（**必给**：缺席不当成不存在） |
| `take` | → | 可缺席。用户勾了哪几条（条目名数组）；给了才答 `write` |
| `overwrite` | → | 可缺席。`take` 里哪几条**说了要盖掉对面不同的那一条**；给了它就必须给 `take`，且是它的子集 |
| `rows` | ← | 每个条目名一行 `{name, state, suspects}`。`state` 闭集：`new`（对面没有）· `same`（值相等，键序无关）· `differs` · `only-there`（只在对面，**永远不碰**）。`suspects` 只在 `new` / `differs` 上有：`{kind, field, value, there}` —— `kind` 闭集 `abs-path` · `command-missing` · `command-relative`；`field` 是 `command` / `args[i]` / `env.<键>` / `cwd`；`there` 闭集 `present` · `absent` · `foreign`（不是这台那种系统的路径写法）· `unknown`，`command-relative` 那种是 `null` |
| `write` | ← | 没给 `take` ⇒ `null`；给了 ⇒ 真要写的条目名（排序）：`same` 不写、`new` 写、`differs` 在 `overwrite` 里才写 |

可疑项的规则：只看 stdio 那四个字段（`url` / `headers` 不看）。`abs-path` = 字段值是绝对路径（POSIX `/…` 或 Windows `X:\…` / `X:/…` / `\\…`），或 `--opt=<绝对路径>` 的右边是；
`command-missing` = `command` 是裸名字、且这个**后端进程**的 `PATH` 上找不到同名文件（有 `PATHEXT` 就按它补后缀；找得到不标）；
`command-relative` = `command` 带分隔符却不是绝对路径。⚠ `PATH` 是后端进程的，不是用户登录 shell 的。

**错误码**：`bad_args`（缺 / 类型不对 · `take` 里有拷出来那一份里没有的名字 · `overwrite` 不是 `take` 的子集）·
`bad_file`（任一份不是合法 JSON / 最外层不是对象 / `mcpServers` 不是对象 —— 不拿骨架比、也不覆盖）·
`needs_consent`（`take` 里有 `differs` 的一条没在 `overwrite` 里点名 —— **整趟拒**，不静默跳过）。
⚠ **CLI 面也有它**（`--mcp-sync-plan`），入参从 stdin 读。

#### `assets-catalog`：资产目录 —— 这台现扫一次、记下、回整份（AS2 · 第四波 4B，2026-09-25）

用户裁（`99 §1` V113，逐字）：「比如本机后端在本机看见一个skill并记录下来, 就会和远端后端同步, 这样远端后端也能在远端装skill或者mcp / mcp保持项目级别」·「目录自动同步，装要你点」。
本条是「记下来」那一半：这台后端现扫一次它看到的 skill（`<配置根>/skills/<名>/`）与项目级 MCP（`.claude.json` 的 `projects` × `<项目>/.mcp.json` 的 `mcpServers`），
放进**后端自有**的目录文件 `~/.cc-monitor/assets-catalog.json`（第四层：只有本后端写它；**一个用户文件都不写**），变了才写，回整份目录 ＋「别的机器有的，这台怎样」。

```text
→ {"id":"a1","cmd":"assets-catalog","args":{}}
← {"kind":"reply","id":"a1","ok":true,"data":{"self":"9f…","machines":[{"id":"9f…","label":"u@h","gen":3,"seenAt":1727250000,"assets":[{"kind":"skill","name":"demo","digest":"…","summary":{"description":"…","files":2,"bytes":120,"binary":false,"truncated":false}}]}],"rows":[{"kind":"mcp","name":"gh","state":"missing","from":[{"machine":"4c…","project":"/home/r/x","digest":"…","summary":{…}}]}],"problems":[],"changed":false,"path":"/home/u/.cc-monitor/assets-catalog.json"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `self` | ← | 这台机器的 id（第一次记目录时生成，之后不变） |
| `machines` | ← | 各台一份快照 `{id, label, gen, seenAt, assets}`：`gen` 是那台**自己**的代数（它自己那份变了才 +1）；`seenAt` 是那一代的时刻（那台的钟，只给人看、不参与合并）。`assets[]` 是 `{kind, name, project?, digest, summary}`：`kind` 闭集 `skill` · `mcp`；`project` 只有 MCP 有（**项目级**）；`digest` 只答相同 / 不同（FNV-1a 64，不防篡改） |
| `rows` | ← | 别的机器有的每个（`kind`, `name`）一行 `{kind, name, state, from}`：`state` 闭集 `missing`（这台一条同名的都没有）· `differs`（有同名的，摘要都不同）· `same`（有一条摘要相同）；`from` 是别处那几条 `{machine, project, digest, summary}` |
| `problems` | ← | 这一趟扫描读不出来的那几份（一句话一份）—— 「这台没有」与「这台那份读不出来」分开说 |
| `changed` | ← | 这一趟目录有没有变（变了才写盘） |
| `path` | ← | 目录文件在这台上的路径 |

🔴 **MCP 的 `env` / `headers` 的值不进目录**，只进键名（`envKeys` / `headersKeys`）；`args` / `command` / `cwd` / `type` / `url` 原样，其余字段只记键名（`otherKeys`）。
`digest` 按整条原文算（只差密钥值也判得出不同）。理由：目录是**自动**同步到每台的；「原样拷」（V112）发生在用户点「装」那一下、从来源那台现读。

**错误码**：`catalog_unreadable`（目录文件在但读不懂 / 是更新的格式 —— **不覆盖**）· `io_failed`（家目录解析不出来 / 写不进去）。
⚠ **CLI 面也有它**（`--assets-catalog`），无入参。

#### `assets-catalog-merge`：把另一台后端的整份目录并进来（AS2 · 第四波 4B，2026-09-25）

同 `assets-catalog`（先现扫这台、记下），再把 `catalog` 里各台的快照并进来：**同一台取 `gen` 大的那一份整份**（删除随整份替换传播）；
这台自己那一格**只认自己扫的**（别处传来的一律不收）。不比墙钟、幂等、与到达顺序无关 ⇒ 反复同步收敛。回并完之后的整份（形状同上）。

```text
→ {"id":"a2","cmd":"assets-catalog-merge","args":{"catalog":{"self":"4c…","machines":[{"id":"4c…","label":"r@x","gen":2,"seenAt":1727250100,"assets":[…]}]}}}
← {"kind":"reply","id":"a2","ok":true,"data":{"self":"9f…","machines":[…],"rows":[…],"problems":[],"changed":true,"path":"…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `catalog` | → | 另一台后端的整份目录，形状就是 `assets-catalog` 的应答（只读它的 `machines`） |
| 其余 | ← | 同 `assets-catalog` |

**错误码**：`bad_args`（缺 `catalog` / 某台缺格 / 类型不对 / `kind` 不在闭集 —— 不猜）· `catalog_unreadable` · `io_failed`。
⚠ **CLI 面也有它**（`--assets-catalog-merge`），入参从 stdin 读。

#### `assets-sync`：本机常驻后端沿池里那条 SSH 同步资产目录（AS2 · 第四波 4B，2026-09-25）

「目录自动同步」那一半（V113）：`设计/01 §3.5`「观测方沿它本来就拥有的那条连接去拉被观测方」。只有**本机常驻后端**有意义（SSH 连接与可达表都住在它的进程里）。
一趟对一台远端：① 在池里那条连接上多开一个 exec 通道，capture `<远端后端> --assets-catalog`（远端现扫、记下、回它的整份）；
② 并进本机目录（同 `assets-catalog-merge`）；③ 远端缺的 / 比远端新的那几台快照（不含远端自己那格）经 `printf '%s\n' '<json>' | <远端后端> --assets-catalog-merge` 推过去（一块 ≤ 96 KiB，单台超了那一台不推、说出来）；
④ 本机目录因这一趟变了（或开头那一次现扫发现本机自己那份变了）⇒ 对可达表里其余每台各做一趟（只一层）。**不往任何机器装东西**（装要用户点）。
不起远端的流模式（流模式会往 tmux 装指向自己 pid 的全局 hook，一个用完就退的流会把真流的 hook 盖掉）；老远端不认子命令会进流模式 —— capture 见到 hello 就收工、报「太旧」。

```text
→ {"id":"s1","cmd":"assets-sync","args":{"origin":"dev","dial":{"host":"10.0.0.2","port":22,"user":"u","key_path":"~/.ssh/id_ed25519"},"backend":"/home/u/.cc-monitor/bin/ccm"}}
← {"kind":"reply","id":"s1","ok":true,"data":{"self":"9f…","synced":[{"origin":"dev","peer":"4c…","changed":true,"pushed":1,"error":null}],"reach":[{"origin":"dev","machine":"4c…"}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 可缺席。给了 ⇒ 记进可达表（内存，后端重启就空）并先对它做一趟；缺席 ⇒ 对可达表里每一台各做一趟 |
| `dial` | → | 给了 `origin` 就必给：那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体）。本条会把 `use` 改成 `capture` |
| `backend` | → | 给了 `origin` 就必给：那台上后端的路径 |
| `synced` | ← | 每一趟一行 `{origin, peer, changed, pushed, error}`：`peer` 是那台目录的 `self`；`changed` 本机目录因这一趟变了没有；`pushed` 推过去几台快照；`error` 那一趟哪里没办成（`null` = 全办成了） |
| `self` | ← | 本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>` |
| `reach` | ← | 可达表 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）—— 界面据它把目录里的机器 id 对回 origin |

**错误码**：`bad_args`（`origin` 空串 · 给了 `origin` 缺 `dial` / `backend` · 给了 `dial` / `backend` 没给 `origin` · 可达表满）· `io_failed`（本机目录开头那一次现扫没办成）。某一台连不上 / 太旧 / 没办成**不是整条失败**，落在那一行的 `error`。
⚠ **CLI 面也有它**（`--assets-sync`，入参从 stdin 读；按派生规则「非内建即上 CLI」），但一次性进程没有常驻那一个的连接池与可达表：
它自己新拨一条 SSH、只对给的那一台做一趟，扇出恒为零台 —— 真正的用法是常驻后端的帧面。

#### `history-annotate`：改一条历史注解（C4d · 第四波 4B，2026-09-25）

历史注解（星标 / 改名 / 隐藏 / 上次用哪个号起这个会话）的**读写者是本机常驻后端**（主会话 09-25 裁：文件留在原处、同一路径，不迁移、一条不丢）。
那份文件就是 monitor 从前读写的 `<monitor 数据目录>/history-metadata.json`：路径由 monitor 起本机后端时用环境变量 `CCM_HISTORY_METADATA` 显式交（没交 ⇒ `no_annotations`，不猜路径）。
写是后端**自有状态**（`readonly_guard` 第四层）：先严格读一遍，读不懂 ⇒ `annotations_unreadable`、原文件一个字节不动；再在原文上只改那一条（其余条目与认不出的键原样留着）→ `O_EXCL` 临时文件 → 原子挪过去。

```text
→ {"id":"a1","cmd":"history-annotate","args":{"sid":"0f…","patch":{"starred":true}}}
← {"kind":"reply","id":"a1","ok":true,"data":{"entry":{"starred":true,"customTitle":null,"hidden":false,"updatedAt":1727250000000,"lastAccount":null}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 会话 id |
| `patch` | → | 要改的那几格：`starred` / `customTitle` / `hidden` / `lastAccount`（后两格也认蛇形 `custom_title` / `last_account`）。缺格或 `null` = 不改；标题 / 账号名给空白串 = 清空；多一格 ⇒ `bad_args` |
| `entry` | ← | 改完的那一条：`starred` · `customTitle`（`null` = 没改过名）· `hidden` · `updatedAt`（毫秒，= 这一次）· `lastAccount`（`null` = 没记过） |

**错误码**：`bad_args` · `no_annotations`（这个后端没被交路径）· `annotations_unreadable`（那份文件读不懂 / 读不动 —— 没有覆盖它）· `io_failed`。
⚠ **CLI 面也有它**（`--history-annotate`，入参从 stdin 读）；一次性进程多半没被交路径 ⇒ `no_annotations`。

#### `history-forget`：删一条历史注解（C4d · 第四波 4B，2026-09-25）

删会话时连带（monitor 删完那份会话文件之后问本机后端）。写法同 `history-annotate`；那一条不在 ⇒ 不写。

```text
→ {"id":"f1","cmd":"history-forget","args":{"sid":"0f…"}}
← {"kind":"reply","id":"f1","ok":true,"data":{"removed":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 会话 id |
| `removed` | ← | 真删了一条没有（`false` = 本来就没有这一条，文件没动） |

**错误码**：同 `history-annotate`。
⚠ **CLI 面也有它**（`--history-forget`，入参从 stdin 读）；一次性进程多半没被交路径 ⇒ `no_annotations`。

#### `history-last-accounts`：sid → 上次用哪个号起（C4d · 第四波 4B，2026-09-25，**只读**）

账号徽章的回落来源（「上次用本工具带账号起」）与带账号 resume 前的现读。只含真记过账号的那几条。

```text
→ {"id":"l1","cmd":"history-last-accounts","args":{}}
← {"kind":"reply","id":"l1","ok":true,"data":{"accounts":{"0f…":"work"}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `accounts` | ← | `{sid: 账号名}` |

**错误码**：`no_annotations` · `annotations_unreadable`（读不懂不说成「一条都没有」）。
⚠ **CLI 面也有它**（`--history-last-accounts`，不读 stdin）；一次性进程多半没被交路径 ⇒ `no_annotations`。

#### `remote-reach`：本机后端的可达表登记（C4d · 第四波 4B，2026-09-25）

「本机后端问远端后端」那一跳（`设计/01 §3.5`；实现住后端 `remote_ask.rs`，全后端只此一处）要先知道「怎么够到那台」。
monitor（宿主，只交事实）在**每台**远端流握手成功那一刻交一次：拨号请求 ＋ 那台后端的路径。本机后端记进**内存**可达表（后端重启就空，下次那台连上再填），**只登记、不拨号**。
之后的两路都查这张表：资产目录同步（`assets-sync`）· 历史跨机 join（`history-projects` / `history-sessions` 带 `origin`）。
老远端也登记：历史那一路问它的是 `--list-projects` / `--list-sessions` 这种老子命令。

```text
→ {"id":"r1","cmd":"remote-reach","args":{"origin":"dev","dial":{"host":"10.0.0.2","port":22,"user":"u","key_path":"~/.ssh/id_ed25519"},"backend":"/home/u/.cc-monitor/bin/ccm"}}
← {"kind":"reply","id":"r1","ok":true,"data":{"origin":"dev","reach":[{"origin":"dev","machine":null}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 那台的名字（monitor 的 origin 名，本后端只当不透明的键用） |
| `dial` | → | 那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体）。用时会把 `use` 改成 `capture` |
| `backend` | → | 那台上后端的路径 |
| `reach` | ← | 登记之后的可达表 `[{origin, machine}]`（同 `assets-sync` 的那一格） |

**错误码**：`bad_args`（缺 `origin` / `origin` 空串 · 缺 `dial` / `backend` · 可达表满）。
⚠ **CLI 面也有它**（`--remote-reach`，入参从 stdin 读），但一次性进程的可达表随进程退出就空 —— 真正的用法是常驻后端的帧面。

#### `skill-read`：读来源那台上的一个 skill（AS2 · 第四波 4B，2026-09-25，**只读**）

「装要用户点」（V113）那一步的读半边：在**来源那台**跑，交出 `<skill 根>/<名>/` 下每个文件的原文（V112「内容，原样拷过去」）。

```text
→ {"id":"k1","cmd":"skill-read","args":{"name":"demo"}}
← {"kind":"reply","id":"k1","ok":true,"data":{"root":"/home/u/.claude/skills","dir":"/home/u/.claude/skills/demo","files":[{"path":"SKILL.md","text":"---\n…","bytes":120,"exec":false,"why":null},{"path":"bin/tool","text":null,"bytes":90210,"exec":true,"why":"不是文本文件"}],"skipped":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | skill 的目录名（一段：不许分隔符 / `..` / 点开头） |
| `root` / `dir` | ← | 这台 skill 的根 / 这个 skill 的目录（绝对路径） |
| `files` | ← | 每个普通文件一条 `{path, text, bytes, exec, why}`：`path` 是 skill 里的相对路径（`/` 分段）；`text` 是原文，**不是文本 / 超 4 MiB / 读不出来 ⇒ `null` ＋ `why`**（这一个装不过去：今天的写口只收文本）；`exec` 执行位（非 unix 恒 `false`） |
| `skipped` | ← | 没读的那几处（指向目录的链接 / 特殊文件） |

**错误码**：`bad_args`（名字不对）· `not_found`（这台没有这个 skill）· `too_large`（文件超过 512 个 —— 装一半比不装更坏，整趟不读）· `io_failed`。
⚠ **CLI 面也有它**（`--skill-read`），入参从 stdin 读。

#### `skill-install-plan`：在要被写的那一台判 skill 装不装得过来（AS2 · 第四波 4B，2026-09-25，**只读**）

在**要被写的那一台**跑（事实是那台的）。差异四态与「不同的要显式说盖、不然整趟拒」那道闸**原样用** `mcp-sync-plan` 的那一份（键 = 文件相对路径，值 = `{text, exec}`）。
一个字节都不写：写经调用方 → 那台后端 `files-put`（`expect` = 这里回的 `target` 里那一份，不存在 = `null`，`parents: true`）＋ `files-chmod`。

```text
→ {"id":"k2","cmd":"skill-install-plan","args":{"name":"demo","source":[{"path":"SKILL.md","text":"---\n…","exec":false}]}}
← {"kind":"reply","id":"k2","ok":true,"data":{"root":"…/skills","dir":"…/skills/demo","rows":[{"path":"SKILL.md","state":"new","suspects":[],"blocked":null}],"target":[],"write":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | skill 的目录名 |
| `source` | → | `skill-read` 读到的 `[{path, text, exec}]`（`text` 可为 `null` = 装不过去的那一个） |
| `take` / `overwrite` | → | 可缺席，语义同 `mcp-sync-plan`（给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名） |
| `root` / `dir` | ← | 这台 skill 的根 / 要写进去的目录 |
| `base` / `prefix` | ← | 写的时候 `files-put` 用的 `root` 与相对前缀：`rel` = `<prefix>/<path>`。skill 根在 ⇒ `base` 就是它；不在 ⇒ 是它的上一层（配置根），由 `parents` 建出来 |
| `rows` | ← | 每个路径一行 `{path, state, suspects, blocked}`：`state` 闭集同 `mcp-sync-plan`；`suspects` 只在 `new` / `differs` 上有 `{kind, value, there}` —— `kind` 闭集 `executable`（有执行位或 `#!` 开头）· `binary`（来源读不出原文）· `abs-path`（文本里的绝对路径，`there` 闭集同 `mcp-sync-plan`）· `command-missing`（`#!/usr/bin/env X` 的 `X` 在这台后端的 `PATH` 上找不到）；`blocked` = 这台上那一份盖不了的原因（不是文本等），否则 `null` |
| `target` | ← | 这一趟拷的那几个路径在这台上现有的原文 `[{path, text}]` —— 写的时候当 CAS 期望 |
| `write` | ← | 没给 `take` ⇒ `null`；给了 ⇒ 真要写的路径（排序） |
| `ledger` | ← | 〔SU1 · 4C · V116〕没给 `take` ⇒ `null`；给了 ⇒ `{<path>: {digest, created}}`，恰是 `write` 那几个：`digest` = 来源那一份原文的摘要（FNV-1a 64，16 位小写十六进制），`created` = 这台上原来没有（`new`）。调用方写完把**真写成了的那几个**原样交 `skill-install-record`（`op: "add"`） |

**错误码**：`bad_args`（名字 / 路径不对 · `take` 里有不在 `source` 里的 · `take` 了来源读不出原文的那一个）· `bad_file`（`take` 了这台上盖不了的那一个）· `needs_consent`（同 `mcp-sync-plan`）· `too_large` · `io_failed`。
⚠ **CLI 面也有它**（`--skill-install-plan`），入参从 stdin 读。

#### `skill-install-record`：skill 装记录的写口（SU1 · 第四波 4C，2026-09-25，**写后端自有状态**）

V116「要，只删装时写进去的文件」：装的时候记下写了哪几个文件，卸只删这些。记录住这台后端自己的 `~/.cc-monitor/skill-installs.json`（第四层；一个用户文件都不写）。

```text
→ {"id":"r1","cmd":"skill-install-record","args":{"op":"add","name":"demo","files":{"SKILL.md":{"digest":"8c3e…","created":true}}}}
← {"kind":"reply","id":"r1","ok":true,"data":{"dir":"/home/u/.claude/skills/demo","name":"demo","changed":true,"remaining":1}}
→ {"id":"r2","cmd":"skill-install-record","args":{"op":"drop","dir":"/home/u/.claude/skills/demo","paths":["SKILL.md"]}}
← {"kind":"reply","id":"r2","ok":true,"data":{"dir":"/home/u/.claude/skills/demo","name":"demo","changed":true,"remaining":0}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `op` | → | `add`（装完记）或 `drop`（卸掉 / 已经不在的摘掉） |
| `name` | → | `add`：skill 的目录名。**目录由这台后端按 `skill 根 / name` 自己算**，不收调用方给的路径 |
| `files` | → | `add`：`{<相对路径>: {digest, created}}` —— `skill-install-plan` 答的 `ledger` 里真写成了的那几个。同一目录再装一次：新路径加进来、已记的换新摘要、`created` 取第一次的 |
| `dir` | ↔ | `drop` 的入参：记录里那个 skill 目录；应答里是这一条记录的目录 |
| `paths` | → | `drop`：要摘的相对路径（不在记录里 ⇒ `bad_args`，一个字节不动）；摘到零个 ⇒ 整条记录摘掉 |
| `changed` / `remaining` | ← | 记录变没变（没变不写）/ 这一条还剩几个文件 |

**错误码**：`bad_args` · `not_found`（`drop` 的目录没记着）· `ledger_unreadable`（记录读不懂 / 另一个版本写的 —— **不覆盖**）· `too_large`（一趟超过 512 个）· `io_failed`。
⚠ **CLI 面也有它**（`--skill-install-record`），入参从 stdin 读。

#### `skill-installs`：这台记着的、从别的机器装来的 skill（SU1 · 第四波 4C，2026-09-25，**只读**）

```text
→ {"id":"r3","cmd":"skill-installs"}
← {"kind":"reply","id":"r3","ok":true,"data":{"installs":[{"dir":"/home/u/.claude/skills/demo","name":"demo","files":3}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `installs` | ← | 每条 `{dir, name, files}`（`files` = 记着的文件数）。还没装过 ⇒ `[]` |

**错误码**：`ledger_unreadable`（读不懂 ≠ 没装过）· `io_failed`。
⚠ **CLI 面也有它**（`--skill-installs`），不收输入。

#### `skill-uninstall-plan`：在被卸的那一台判卸哪几个（SU1 · 第四波 4C，2026-09-25，**只读**）

只卸记录里那几个文件（V116）。一个字节都不写：删经调用方 → 那台后端 `files-delete`（`root` = `dir`，`rel` = 路径，`expect` = 这里回的 `seen` 里那一份）；删完把删掉的 ＋ `forget` 交 `skill-install-record`（`op: "drop"`）。

```text
→ {"id":"r4","cmd":"skill-uninstall-plan","args":{"dir":"/home/u/.claude/skills/demo"}}
← {"kind":"reply","id":"r4","ok":true,"data":{"dir":"…/demo","name":"demo","rows":[{"path":"SKILL.md","state":"modified","created":true,"deletable":true,"ask":true}],"seen":[{"path":"SKILL.md","text":"…"}],"delete":null,"forget":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `dir` | ↔ | 记录里那个 skill 目录（`skill-installs` 给的） |
| `take` / `confirm` | → | 可缺席。给了 `take` 才答 `delete`；`ask` 为真的那几个要在 `confirm` 里点名，不然整趟拒（`needs_consent`）；`confirm` 必须 ⊆ `take` |
| `name` | ← | 装时那个 skill 名（给人看） |
| `rows` | ← | 记录里每个文件一行 `{path, state, created, deletable, ask}`：`state` 闭集 `intact`（在、摘要 == 装时那一份）· `modified`（装完被改过）· `gone`（已经不在）· `unreadable`（不是普通文件 / 不是文本，没法按原文删）；`deletable` = `intact` 或 `modified`；`ask` = 能删且（`modified` 或装之前就在 —— `created: false`，删了回不到装之前那一份） |
| `seen` | ← | 能删的那几份现有原文 `[{path, text}]` —— 删的时候当 CAS 期望 |
| `delete` / `forget` | ← | 没给 `take` ⇒ `null`；给了 ⇒ 真要删的（排序）/ 已经不在、要从记录里摘的 |

**错误码**：`bad_args`（`take` 里有不在记录里的 / 删不了的 · 两张单子对不上）· `needs_consent` · `not_found`（这台没记着这个目录）· `ledger_unreadable` · `io_failed`。
⚠ **CLI 面也有它**（`--skill-uninstall-plan`），入参从 stdin 读。

#### 只读查询面（`C1`，2026-09-24）—— **八条一次性查询搬上这条长连接**

出处 `设计/15 §3.2` 层 1 ＋ `设计/99 §4.19.2 ⑥`。这八条此前**只有**一次性子命令那一面：monitor 每问一次就新拨一条 SSH（握手 ＋ 鉴权 ＋ exec），账号那两条还被一个 10 秒的轮询按台数翻倍。现在它们也在帧面上 —— **跑的是 CLI 那一臂同一个函数**，只是输出从 stdout 换成应答里的 `data`。

| 帧命令 | 同一个函数的 CLI 那一臂 | 应答形状 |
|---|---|---|
| `history-projects` | `--list-projects` | 按行 |
| `history-sessions` | `--list-sessions` | 按行 |
| `history-search` | `--search` | 按行 |
| `history-subagents` | `--list-subagents` | 按行 |
| `accounts-list` | `--list-accounts` | 〔C4c〕成品（`{meta, accounts, notice}`） |
| `accounts-sessions` | `--session-accounts` | 按行 |
| `history-read` | `--read-session` · `--read-session-from-offset`（不带 `--index`） | 按字节分页 |
| `history-tail` | `--read-session-tail` 的那张「尾段在哪」的图 | 四个数 |

- **按行**：`data = {"lines": [...]}`，每个元素就是 CLI 那条 stdout 的一行（trim 过、剔空行）。整份输出超过 32 MiB ⇒ `too_large`，**不截断**（截断的清单会被当成完整的用）。
- **名字刻意不与 CLI 同名**：CLI 面从 `REGISTRY` **自动派生**（`--<名>`），同名就会把 `--list-projects` 抢过去改印一行 JSON。⇒ 代价如实写：八条同拍多出八个 CLI 面 `--history-projects` · `--history-sessions` · `--history-search` · `--history-subagents` · `--history-read` · `--history-tail` · `--accounts-list` · `--accounts-sessions`（stdin 一段 JSON ＝ `args`，stdout 一行 JSON ＝ `data`）。它们与老的那八个子命令是**同一个函数的两个宿主**，不是第二份实现。
- 八条全在阻塞档（做文件 I/O）⇒ `cancel` 命中回 `not_cancellable`。
- 失败的 code 都是**命令级**的；读失败 `failed`，参数缺或类型不对 `bad_args`。

#### `history-projects`：列全部项目（〔C4d · 第四波 4B〕**出成品**：并上注解 ＋ 判活，远端经本机后端问）

历史跨机 join 的唯一的家是**本机常驻后端**（主会话 09-25 裁；实现 `history_join.rs`）。`origin` 缺席 = 这台机器：记录树里的项目（`--list-projects` 那一行）＋ 合成历史（Codex 按 cwd 分组，项目键 `codex:<cwd>`）＋ 这台后端自己判活（pidfile）；
`origin` 给了 = 可达表里的那一台（`remote-reach` 登记的）：本机后端沿池里那条 SSH 在那台跑 `--list-projects`（CLI 老子命令，stdout 形状一个字节没变 ⇒ **那台的后端不必升级**），判活「不知道」。
两支都并上**这台**的注解（`history-annotate` 那一份：星标数 / 隐藏数；读不到 ⇒ 两个数 `null`、`notice` 说为什么）。项目按「活的 → 有星标的 → 最近动过的」排。

```text
→ {"id":"q1","cmd":"history-projects","args":{"origin":"dev"}}
← {"kind":"reply","id":"q1","ok":true,"data":{"rows":[{"projectPath":"/home/u/proj","projectName":"proj","projectDir":"-home-u-proj","sessionCount":3,"starredCount":1,"hiddenCount":0,"lastActivity":1727250000000,"hasLive":null,"origin":"dev"}],"notice":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 可缺席：那台的名字（可达表的键）。缺席 = 这台 |
| `rows` | ← | 每项目一行：`projectPath` · `projectName` · `projectDir`（懒加载的键，原样交回 `history-sessions`）· `sessionCount` · `starredCount` / `hiddenCount`（`null` = 不知道，**不是 0**）· `lastActivity`（毫秒）· `hasLive`（`null` = 这条路上答不了）· `origin`（远端那台才有） |
| `notice` | ← | 注解没并上的那句话；`null` = 并上了 |

**错误码**：`bad_args`（`origin` 空串 / 不是串）· `failed`（这台的记录树读不动）· `unreachable`（可达表里没有那一台 / 那台问不出来 —— 带那台的名字与原因）· `too_large`。
⚠ **CLI 面也有它**（`--history-projects`，入参从 stdin 读）；一次性进程的可达表是空的 ⇒ 只答得了这台。

#### `history-sessions`：列一个项目下的会话（〔C4d · 第四波 4B〕**出成品**，同上）

```text
→ {"id":"q2","cmd":"history-sessions","args":{"project_dir":"-home-u-proj","origin":"dev"}}
← {"kind":"reply","id":"q2","ok":true,"data":{"rows":[{"sessionId":"0f…","projectPath":"/home/u/proj","projectName":"proj","aiTitle":null,"firstUserExcerpt":"…","startedAt":1727250000000,"updatedAt":1727250001000,"jsonlPath":"/home/u/.claude/projects/-home-u-proj/0f….jsonl","isLive":null,"messageCountApprox":12,"isBg":false,"starred":false,"customTitle":null,"hidden":false,"origin":"dev"}],"notice":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `project_dir` | → | `history-projects` 给的那个项目键：记录树的项目目录名（不是路径；含分隔符 / `..` ⇒ `bad_args`），或合成历史的 `<kind>:<cwd>`（只在这台） |
| `origin` | → | 同 `history-projects` |
| `rows` | ← | 每会话一行：`sessionId` · `projectPath` · `projectName` · `aiTitle` · `firstUserExcerpt` · `startedAt` / `updatedAt`（毫秒）· `jsonlPath` · `isLive`（`null` = 答不了）· `messageCountApprox` · `isBg` · `starred` / `customTitle` / `hidden`（这台的注解）· `forkedFromSessionId` / `forkedFromMessageUuid`（`/branch` 分叉来的才有）· `origin`（远端那台才有） |
| `notice` | ← | 同 `history-projects` |

**错误码**：`bad_args` · `failed` · `unreachable` · `too_large`。⚠ 远端那一支在那台跑 `--list-sessions <project_dir>`。

#### `history-search`：全文搜索

```text
→ {"id":"q3","cmd":"history-search","args":{"query":"deploy","limit":50}}
← {"kind":"reply","id":"q3","ok":true,"data":{"lines":["{\"sessionId\":…,\"hitCount\":3,…}", …]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `query` | → | 搜索词（必填） |
| `include_tools` | → | 可选布尔，= `--include-tools` |
| `scope` | → | 可选，`user` / `assistant`，= `--scope` |
| `after_ms` | → | 可选，= `--after-ms` |
| `limit` | → | 可选，= `--limit` |
| `lines` | ← | 每命中会话一行 `SessionHits`，形状与行序同 `--search` |

⚠ 选项**不在帧面另写一份语义**：这几个字段被摊回 `--include-tools` / `--scope` / `--after-ms` / `--limit`，交给 CLI 那一臂同一个解析。

#### `history-subagents`：列一个父会话的 subagent 候选

```text
→ {"id":"q4","cmd":"history-subagents","args":{"parent":"/home/u/.claude/projects/-p/s.jsonl"}}
← {"kind":"reply","id":"q4","ok":true,"data":{"lines":["{\"path\":…,\"description\":…,\"timestamp\":…}"]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `parent` | → | 父会话 jsonl 路径（`projects/` 围栏照旧；越界 ⇒ `path_refused`，推不出目录 ⇒ `bad_parent`） |
| `lines` | ← | 每候选一行 `{path, description, timestamp}`，同 `--list-subagents`（只列不挑） |

#### `accounts-list`：账号清单（〔C4c · 第四波 4B〕**出成品**）

```text
→ {"id":"q5","cmd":"accounts-list","args":{"agent":"claude-code"}}
← {"kind":"reply","id":"q5","ok":true,"data":{"meta":{"enabled":true,…},"accounts":[{"name":…,"authKind":…,"authReady":…},…],"notice":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 必填：这次起会话的是哪一家（适配器 id）。只有它是这台机器 apikey 表的那一家时，表里的行才算数（条 49） |
| `meta` | ← | `{enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error}`（同 `--list-accounts` 首行去掉分帧用的 `kind` / `accountZeroAware`）。账号库目录走默认解析，**帧面不收 `--accts-dir`** |
| `accounts` | ← | 每账号一个对象，字段同 `--list-accounts` 的账号行；**并上了这台机器自己那份 apikey 表**：表里有行的号 `authKind` 是 `api-key`、`authReady` 按 `acct_core::auth_ready`（规则住 `acct-core`，CLI 那一臂不并表） |
| `notice` | ← | 「能用但有缺」：启用了却一个账号 0 都没有（cc-acct-iso 写侧旧）时的一句话；否则 `null` |

**错误码**：`bad_args`（缺 `agent`）· `too_large`。
⚠ 〔C4c〕此前应答是 `{"lines": [...]}`（与 CLI 逐行同形）、并表在 monitor 做且只并本机；老后端仍回旧形状 ⇒ 新界面当场认出「两端契约对不上」。

#### `accounts-trust`：换号前的信任预检（〔C4c · 第四波 4B〕替掉逐次拨号的 `--account-trust` / `--account-trust-zero`）

```text
→ {"id":"q7","cmd":"accounts-trust","args":{"configDir":"/home/u/.claude-accts/a","cwd":"/home/u/proj"}}
← {"kind":"reply","id":"q7","ok":true,"data":{"trusted":true,"known":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `configDir` | → | 目标账号的 config dir（必须逐字 ∈ manifest，否则 `unknown_config_dir`）；**缺席或 `null` = 账号 0**（读 `$HOME/.claude.json`，不收路径） |
| `cwd` | → | 要预检的工作目录（必填） |
| `trusted` | ← | 这个账号接受过该目录的信任对话框 |
| `known` | ← | 这个账号的 `.claude.json` 里有该目录的记录（`false` ⇒ 首次进入，大概率会弹确认） |

**错误码**：`bad_args` · `unsafe_config_dir` · `unknown_config_dir` · `manifest_unavailable` · `no_home` · `failed`（读 / 解析那个账号的配置文件失败等 agent 那一层的码一律落它，原因原样带着 —— 通用层不认 agent 的名字）。
与 `--account-trust` / `--account-trust-zero` 是**同一个函数的两个宿主**；CLI 面照例自动派生一个 `--accounts-trust`（stdin 一段 JSON）。

#### `accounts-sessions`：正在跑的会话各属哪个账号（**不读 stdin**）

```text
→ {"id":"q6","cmd":"accounts-sessions","args":{}}
← {"kind":"reply","id":"q6","ok":true,"data":{"lines":["{\"sessionId\":…,\"account\":…}", …]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `lines` | ← | 同 `--session-accounts`：每条运行中会话一行 |

#### `history-read`：按字节分页读一份会话

```text
→ {"id":"q7","cmd":"history-read","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","offset":0}}
← {"kind":"reply","id":"q7","ok":true,"data":{"text":"{…}\n{…}\n","next":1048571,"eof":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径，围栏同 `--read-session`（越界 ⇒ `refused`） |
| `offset` | → | 从这个字节起（缺省 0） |
| `until` | → | 可选右端（半开区间 `[offset, until)`），= `--until` |
| `text` | ← | 这一页（UTF-8 有损解码）。**不超过 1 MiB，切在行尾**；区间到头时余下的全给（含 torn 残尾） |
| `next` | ← | 下一页从这里起（= `offset` ＋ 这一页的原始字节数） |
| `eof` | ← | 区间到头了（`until` 或读时的文件长度） |

🔴 **为什么分页**：一帧应答要整个进内存、整个过线；本仓见过 270 MB 的会话，而 monitor 单帧上限 64 MiB。单行比一页还长时续读到行尾，但超过 32 MiB ⇒ `oversized_line`（不叫 `line_too_long`：那是入方向信封的协议级 code）。

#### `history-lines`：按行号取回一段（〔CF2 · 第四波 4B〕不依赖骨架索引）

```text
→ {"id":"q13","cmd":"history-lines","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","from":1200,"until":1400}}
← {"kind":"reply","id":"q13","ok":true,"data":{"from":1200,"next":1400,"eof":false,"lines":["{…}","{…}"]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径，围栏同 `history-read`（越界 ⇒ `refused`） |
| `from` | → | 第一行的行号（缺省 0） |
| `until` | → | 可选右端（半开区间 `[from, until)`）；缺 ＝ 到最后一个完整行为止 |
| `lines` | ← | 可计行的原文（UTF-8 有损解码；不含行尾 `\n`）。第 k 条就是第 `from + k` 行 |
| `next` | ← | 下一段从这一行起（恒 ＝ `from` ＋ `lines` 的条数） |
| `eof` | ← | 读到了最后一个完整行之后 |

**行号口径**：与实时 `line` 帧的 `seq`、`history-tail` 的 `total`、`history-index` 的行同一个空间 —— BOM 与全空白的行不占号、没 `\n` 收尾的残尾不计（判定只住 `history_query::line_counts`）。**一帧装得下**：交出的原文累计到 1 MiB 就停（至少一行）；单行超过 32 MiB ⇒ `oversized_line`。**代价**：后端零状态、每次从文件头数（O(`from` 之前的字节)），依据与读数见 `调研/第四波记录/CF2.md §1`。CLI 面随之自动多一条 `--history-lines`。

#### `history-record`：这条会话的记录还在不在（〔U4b · 第四波〕resume 之前问）

```text
→ {"id":"q12","cmd":"history-record","args":{"sid":"9d66c46d-bf88-4f99-877e-455555555555"}}
← {"kind":"reply","id":"q12","ok":true,"data":{"present":false,"root":"/home/u/.claude/projects"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 会话 id。**找哪一份只凭 sid、不收文件路径**（`INVARIANTS §41.6` 收窄第 3 条的同一个取向）；形状不合法（`[A-Za-z0-9-]`，1..=64）⇒ `bad_args`，先于任何 IO |
| `configDir` | → | 〔GP1 · 第四波〕可选：这次 resume 要用的**账号配置目录**（`CLAUDE_CONFIG_DIR` 那一个）。给了 ⇒ 在 `<configDir>/projects` 里找；缺席 / `null` ⇒ 这台后端自己的家目录（与加这一格之前逐字同一问）。先过账号库那一个形状关（绝对路径 · 不上跳 · 无 shell 元字符与欺骗字符，`accounts_query::is_safe_config_dir`），不过 ⇒ `bad_args`，先于任何 IO。它选的是**哪棵树**，不是哪个文件 —— 找文件那一步照旧只凭 sid |
| `present` | ← | `<sid>.jsonl` 在那棵记录树里找得到（根那一层或项目目录那一层；符号链接不算命中）—— 找文件那一步与 `--fork-session` / `files-delete-session` 同一份（`branch_core::find_session_file`） |
| `root` | ← | 查的那棵记录树的根（报错时说清查了什么，`设计/01 §6.9`） |

**为什么要它**：`设计/01 §6.2` 最后一条逐字「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」。前端 resume 一跳在开终端之前问一次，答 `false` 就不开、把原因说出来。**射程**：查的是**那一棵**记录树 —— 调用方的话要说成「这棵树里没有」。〔GP1 · 第四波〕从前只查这台后端的家目录，会话起在另一个账号的配置根下时答 `false` 而那边其实有（`设计/30 §8` 第 4 条）；今天 monitor 带上这次 resume 要用的那个 `configDir`。CLI 面随之自动多一条 `--history-record`。

#### `history-tail`：尾段在哪（快照「尾部优先」那张图）

```text
→ {"id":"q8","cmd":"history-tail","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","n":500}}
← {"kind":"reply","id":"q8","ok":true,"data":{"total":1200,"tail_from":700,"split_at":3310442,"end":5120088}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同上） |
| `n` | → | 要最新几行 |
| `total` | ← | 可计行总数（口径同 `--read-session-tail` 的 `snapshot_meta`） |
| `tail_from` | ← | 尾段第一行的行号 |
| `split_at` | ← | 尾段第一行的字节起点 |
| `end` | ← | 最后一个完整行之后的字节位置 |

客户端先读 `[split_at, end)`（最新 N 行）再读 `[0, split_at)`（回填），都走 `history-read` 带 `until`；与 `--read-session-tail` 一趟印出的两段**逐字节相同**（扫的是同一个函数）。

#### `history-index`：会话骨架索引（〔SR1a〕2026-09-24 上帧面）

```text
→ {"id":"q9","cmd":"history-index","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","offset":0}}
← {"kind":"reply","id":"q9","ok":true,"data":{"from":0,"end":5120088,"rows":[{"o":0,"n":812,…},…]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `offset` | → | 从哪个字节起（缺省 0；续传带上次尾行的 `end`） |
| `until` | → | 可选：只收起点 `< until` 的行 |
| `from` / `end` / `rows` | ← | 〔C4b〕**成品**：`rows` 是每个可计行一条（形状见 §10.3 的行），`end` = 下一次续传该带的 `offset`。与 `--read-session-from-offset --index` 的 stdout **中段逐行相同**（同一个扫描；CLI 那一臂照旧写头尾三段，帧面这一臂不写 —— 一帧是原子的，没有「有头没尾」这一形） |

**为什么上帧面**：它是**每开一个大会话就要一次**的查询（骨架），此前在远端走逐次拨号（`frame_query::STILL_DIALED` 那一行）。
整份超过 32 MiB ⇒ `too_large`（不截断）。〔C4b〕界面经通道直接问（`src/session-reads.ts`），本机与远端同一条路。

#### `history-user-inputs`：「你说过的话」清单（〔SR1a〕2026-09-24 上帧面）

```text
→ {"id":"q10","cmd":"history-user-inputs","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","from":0}}
← {"kind":"reply","id":"q10","ok":true,"data":{"from":0,"end":5120088,"entries":[{"uuid":…,"timestamp":…,"excerpt":…}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `from` | → | 增量起点（缺省 0；传上次尾行的 `end`）。超过文件长度 ⇒ `failed`（文件被截断或重写过） |
| `from` / `end` / `entries` | ← | 〔C4b〕**成品**（口径见 §10.4）：与 `--list-user-inputs` 的 stdout **中段逐行相同**（同一个扫描；头尾只属于 CLI 那一臂） |

**为什么上帧面**：大纲同样是每开一个会话就要一次（此前远端走逐次拨号）。⚠ CLI 面随之自动多两条
`--history-index` / `--history-user-inputs`（从 `REGISTRY` 派生，stdin 一段 JSON ＝ `args`，stdout 一行 JSON ＝ `data`）。

#### 功能侧只读查询（RM1b，第四波）—— 远端会话的任务 · 远端插件市场

出处：`parity_ledger` 的 `session.tasks` / `plugins.marketplaces` 两笔 `ParityDebt`。这几样此前只有 monitor **直读本机**那一条路，远端机器上的同一份数据答不出来。本机后端与远端后端是同一个二进制 ⇒ 读法搬进后端，monitor 按 origin 问那一台（**本机也走这里**，monitor 的直读实现随之退役）。

- 宿主是 `feature_face`（不是 `read_face`，理由在它头注），本体在 `observe/`。
- 应答一律**按行**：`data = {"lines": [...]}`；整份超过 32 MiB ⇒ `too_large`（与 `C1` 同一个口径、同一个常量）。
- CLI 面同样自动派生（`--tasks-list` · `--plugins-marketplaces`），已进 `SUBCOMMANDS`。
- 全在阻塞档（同步文件 I/O）⇒ `cancel` 命中回 `not_cancellable`。

#### `tasks-list`：一个会话的任务列表

```text
→ {"id":"t1","cmd":"tasks-list","args":{"sid":"0c1d…"}}
← {"kind":"reply","id":"t1","ok":true,"data":{"lines":["{\"id\":\"1\",\"subject\":…}", …]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 会话 id。只许一段普通路径名（空 / 含分隔符 / `.` / `..` ⇒ `bad_args`） |
| `lines` | ← | 每个任务一行：`<tasks>/<sid>/<数字>.json` 里那个 JSON 对象**原样**（后端不认字段），按那个数字升序 |

- 那个 sid **没有任务目录** ⇒ 空 `lines`（诚实的空）；目录**在但读不了** ⇒ `failed`（不说成「没有任务」）。
- 半截 / 解不成对象的文件跳过（写者持锁那一刻读到半截是正常时序）；单个文件超过 1 MiB ⇒ 跳过并 `warn!` 点名。

#### `plugins-marketplaces`：这台机器登记的插件市场（**不读 stdin**）

```text
→ {"id":"p1","cmd":"plugins-marketplaces","args":{}}
← {"kind":"reply","id":"p1","ok":true,"data":{"entries":[{"id":"mk","declared_plugins":276,…}],"file_absent":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `entries` / `file_absent` | ← | 〔C4b〕**成品**：整份 survey 就是 `data`（此前裹成「恰一行」的 `lines`）。每条 entry 六个字段 `id` / `source` / `install_location` / `last_updated` / `declared_plugins` / `declared_error`（读不出就是 `null`，不编默认值；界面按形状收，多一格 / 少一格都当契约对不上） |

- 读的是 `<home>/plugins/known_marketplaces.json` 与 `<各落点>/.claude-plugin/marketplace.json`；它回答「有哪些 marketplace、从哪来、**声明**了几个插件」，**不是**「装了 / 启用了哪些」。
- 三条出口分开：文件不在 ⇒ `file_absent: true`（诚实的空）；读 / 解析失败 ⇒ `failed`；某一条数不出 ⇒ 那一条 `declared_plugins: null` ＋ `declared_error` 理由，整张表照出。

#### `panorama`：代码全景（〔RM1c〕第四波，用户 09-24 V108 选 B）

后端**不链**全景引擎：它经插件通用调用口（找它 → `--probe` 问它会不会这个 op → 传 argv 起它，期限走 `timeout` 前缀）起那个只装引擎的独立小程序 `cc-monitor-panorama`，解析发生在被起的那个进程里；索引落**这台机器上后端自己的数据目录**（`~/.cc-monitor/panorama/`），不落进被分析的仓。本机与远端同一条命令。

```text
→ {"id":"g1","cmd":"panorama","args":{"op":"overview","repo":"/home/me/proj","args":{"budget":4000}}}
← {"kind":"reply","id":"g1","ok":true,"data":{"result":{…}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `op` | → | **只说查询语义**：`status` · `index` · `reindex` · `overview` · `node` · `subgraph` · `callers` · `callees` · `impact` · `search` · `docs_for` · `touching` · `symbols_in_file` · `drift` · `list_annotations` · `diagram_kinds` · `diagram`；〔RM1d〕「算」：`plan_add_annotation` · `plan_propose_annotation` · `plan_approve_annotation` · `plan_remove_annotation` · `plan_write_doc_link` · `plan_remove_doc_link`；`refresh_doc_links`（存储 / grammar / 解析开关一个都不上线，`protocol_doc_guard` 钉着） |
| `repo` | → | 被分析的仓在**这台机器上**的绝对路径（`diagram_kinds` 不要） |
| `args` | → | 这个 op 自己的参数（JSON 对象；拼错的字段名被拒，不静默忽略） |
| `result` | ← | 小程序应答里的 `data` **原样**（形状与 monitor 进程内那套全景命令逐字相同；`node` 查不到是 `null`） |

- CLI 面同样自动派生（`--panorama`，stdin 一段 JSON = 上面那个 `args`），已进 `SUBCOMMANDS`。
- 期限：`index` / `reindex` / `refresh_doc_links` 900 秒，其余 60 秒 —— 给**子进程**的（后端零定时器）。客户端那一侧的等待要比它长。
- 错误码：`bad_args`（op 不在词表 / 参数不合形，小程序自己那句话原样带回）· `not_installed`（这台没有那个小程序，或找到的那个身份行对不上；整句说清查过哪儿）· `unsupported`（装的那份不会这个 op，点名缺的那一个）· `timed_out`（说清是哪一档期限）· `too_large`（参数塞不进一次命令调用，或结果超过 32 MiB）· `failed`（仓打不开 / 引擎报错 / 被信号打断，带诊断）。
- 〔RM1d · V110「引擎只算、文件管理来写」〕本命令**不写用户文件**。批注 / 文档关联的 `plan_*` 只读盘上那一两份、回一份编辑计划
  `{"value": …, "edit": null | {"rel", "before", "after", "parents"}}`（`rel` 仓相对；`before` = 算的那一刻盘上原样、`null` = 不存在；
  `after = null` = 删；`edit = null` = 盘上已经是想要的样子）。落盘是调用方拿着计划另发 `files-put`（`root` = 仓、`expect = before`、`parents`）
  或 `files-delete`；`stale` ⇒ 重新 `plan_*`。写了 `.md` 之后发 `refresh_doc_links` 让文档关联的查询跟上（只写索引）。
- 〔RM1f〕**可取消**：异步档（起进程走插件口的 `run_abortable`，异步等子进程）⇒ `cancel` 命中时处理器被撤、小程序连同 `timeout` 前缀那一组子进程一起被杀，回 `cancelled` 帧。〔墓碑 —— RM1c 那一版是阻塞档：`cancel` 命中回 `not_cancellable`。〕
- 〔RM1e · V108「只传给开过远端全景的机器」〕`not_installed` / `unsupported` 是**推字节的触发条件**：monitor 听到这两个码
  （只对远端）⇒ `uname -s -m` 选内嵌字节 → 经本机常驻后端那条 `files` 链路（部署那一问一答，写只许 `~/.cc-monitor/bin/` 与暂存区）
  推到 `~/.cc-monitor/bin/cc-monitor-panorama`（`0755`，后端读回逐字节比对）→ **再问一次**；仍是这两个码 ⇒ 原话交给人，不循环。
  本命令自己不推、不写。
  〔RM1f · 本机对称〕本机那一台同一个触发点：本机后端答这两个码 ⇒ monitor 把它自己带着的那一份（按 `TARGET` 内嵌的原生小程序，Linux 本机退用 musl 那份）
  放到 `~/.cc-monitor/bin/cc-monitor-panorama[.exe]`（逐字节相等就不写）→ 再问一次。Windows 上后端找的文件名带 `.exe`、插件口的 Windows 臂只认 `.exe`。

#### `history-find`：会话内查找（〔SR1a × SE2〕2026-09-24 上帧面）

```text
→ {"id":"q11","cmd":"history-find","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","query":"--force","include_tools":false,"limit":500}}
← {"kind":"reply","id":"q11","ok":true,"data":{"total":1,"hits":[{"uuid":…,"kind":…,"before":…,"matched":…,"after":…}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `query` | → | 查询串（原样；以 `--` 起头也照样是查询，不是选项） |
| `include_tools` | → | 可选，缺省 `false`：工具结果也搜 |
| `limit` | → | 可选，缺省 500、封顶 2000（与 CLI 的 `--limit` 同一对常量） |
| `total` / `hits` | ← | 〔C4b〕**成品**（口径见 §10.5）：`hits` 与 `--find-in-session` 的 stdout **中段逐行相同**（同一个扫描；头尾只属于 CLI 那一臂），`total` = 全量命中数 |

**为什么上帧面**：与上面两条同一个处境（新子命令、此前在远端逐次拨号）；用户每按一次 Enter 就要一次。
CLI 面随之自动多一条 `--history-find`。

#### `resolve`：一次性 exec 与流命令**并存**（U6b-3）

```text
→ {"id":"r1","cmd":"resolve","args":{ResumeSpec}}
← {"kind":"reply","id":"r1","ok":true,"data":{CommandPlan}}
```

**一次性 `--resolve` 那条路逐字不动。** 它的契约与仓外 aterm **冻结在 2026-07-18**
（`daemon-协议-v1 §3`），而 aterm 现走 β TailTransport、DaemonTransport 未建、
**暂不消费 resolve** —— 也就是说这条契约**随时可能开始被消费**，现在拆掉它是拿别人的集成期赌。

两条路**复用同一个纯函数**（`resolve_query::resolve_from_json`），差别只在信封：

| | 一次性 exec | 流命令 |
|---|---|---|
| 请求/响应配对 | 1 exec = 1 请求 1 响应 1 退出，**天然 1:1、无 request-id** | 靠 `id` |
| 取消 | 客户端杀 exec | `cancel` 命令 |
| 代价 | 为一次极小的 RPC 单开一整条 SSH exec | 复用已有连接 |

##### ★ 跨仓承诺（`V126` · 用户 2026-09-25 裁「留」）—— **这一格的字节不许随手动**

仓内**零调用方**（monitor 那边唯一的调用方随 `K-R48` 删了），仓外 aterm 当时也说「暂不消费」；
用户裁**留**：它是**给仓外 aterm 的承诺**，契约冻结在 **2026-07-18**（与上面那段说的是同一份），**随时可能开始被消费**。
⇒ 下面四行列表是那份承诺的全部线上形状，**改任何一格 = 一次跨仓契约变更**，要同轮做三件事：① 改代码；② 改本节与冻结金样 `tests/__fixtures__/resolve-contract.golden.json`；③ **bump `BUILD_ID`**（已部署的后端得被判 stale 重装）—— 并且先问 aterm 那边。
两条入口**都算承诺的一部分**：流命令 `resolve`（`inbound::REGISTRY`）与一次性 `--resolve`（`main.rs` 分派 ＋ `SUBCOMMANDS`），同一个纯函数、差别只在信封（上表）。
钉它的判据：`resolve_query_tests.rs` 里带 `〔V126〕` 的那一族（样例逐字节 · 入参字段 · 错误码全集 · 两条入口 · 本节四行列表与金样两向相等）。

- **入参**（stdin / `args`，camelCase）：`sessionId` · `launchCandidates` · `claudeDir` · `fallbackCwd` · `alreadyInTmux` · `agentKind`
  （`sessionId` 必填；其余缺省。`claudeDir` · `fallbackCwd` · `alreadyInTmux` 今天读进来不用 —— 字段照样冻结，不许改名）
- **出参**（stdout 一行紧凑 JSON / `data`）：`command` · `mode` · `capabilities` · `sessionName` · `launchLabel` · `substitutedFrom`
  （后三个缺席即省略；`mode` 今天恒 `PtyInject`，另一个保留值 `ExecOnce`；可信度见 `§10.1`）
- **`capabilities` 四名**（逐字复用 aterm `SessionCapabilities`）：`supportsSendKeys` · `supportsCapture` · `supportsMultiClient` · `supportsMultiWindow`
- **错误码**（一次性那条 stderr 一行 `{code, message}` ＋ 退出码 **2**；流那条走 `ok:false` 的 `code` / `message`）：`stdin_read_failed` · `bad_request` · `invalid_session_id` · `unsafe_launch_candidate` · `serialize_failed`
  （`stdin_read_failed` 只有一次性那条会出；`bad_request` 与协议级那个同名，**刻意不改**：改它就破了这份冻结契约）

#### 链路四条（〔SR1a〕2026-09-24）—— **本机只常驻一个后端，所有 SSH 连接由它持有并复用**

用户裁「改成单一常驻后端」：monitor 不再每条链路起一个 `--dial` 子进程（C2 那一版），而是经它与**本机常驻后端**之间
**这条已有的流**开「链路」。后端把到同一台远端的所有链路**复用在同一族 SSH 连接上**（按拨号身份：`host · port · user ·
key_path · host_key_fingerprint · 竞速地址 · 跳板`；〔NT1〕默认一条、按需多开至多 3 条；最后一条链路走了连接就断，没有空闲定时器）。

〔NT1 · 2026-09-24，用户 V23「今天每台机器只有一条连接可以看情况多开. 智能一点」〕**一族连接怎么多开、怎么收**（`dial/pool.rs`）——
线上字节一个没变，只是后端那一侧的放置：
- 交互（`stream` · `capture` · `files`）落在**最老的有空格的**那条上；所有成员的通道闸都满了 ⇒ 多开一条（每条连接 8 格 session 通道）。
- 远端回拒一条 session 通道（sshd `MaxSessions`，原话 `no more sessions`）⇒ 那条连接**学到上限**（空格作废，不摘它）、换一条放。
- 传输（`transfer-*`）：主连接上有长流 ⇒ 另开**一条批量连接**（批量字节不排在交互字节前面），它**被主连接托着**：长流在它就在、
  主连接没了它随之断；主连接上没有长流 ⇒ 传输就在主连接上。
- 封顶 3 条（主连接 · 批量 · 溢出）；到顶了就等一格还回来（零定时器）。
- 传输的 sftp 会话用完**停进它那条连接的一个空位**（连同那一格许可），下一趟传输先取它（1 个往返验活，省下开通道的 4 个）；
  别的放置借不到格时先挤掉它。空位不托连接。
- 在一条连接上等远端回话（开通道 / 验活）时**链路被关**（界面的握手期限到点）⇒ 当它可能是黑洞，从族里摘掉（摘掉 ≠ 关掉），
  下一条链路拨新的 —— 不再等 keepalive 连错三次（约 90 s）才换。
- 压缩：判准住 `dial/connect.rs::compression_for`（回环不压 · 内核量到的握手往返 ≥ 5 ms 才压 · 读不到就压），
  闸 `RUSSH_ZLIB_SOUND` **开着**〔CZ1 · V118〕：上游 russh 0.61 的 zlib 解压一包最多交出约两倍包长（真 sshd 上一开压缩第一条通道就卡死），
  后端链的是仓内补过的副本（`Cargo.toml` 的 `[patch.crates-io]` → `src/bridge/vendor/russh`，改了哪几行见那里的 `VENDOR.md`）。
  协商偏好序：压 ⇒ `zlib@openssh.com, zlib, none`（远端关了压缩照样连得上）；跳板自己那条永远不压，答案给隧道里的目标。

**一条链路上的字节 = C2 拨号代理原来的 stdout，逐字节同形**：`stages=true` 时若干行 `{"stage":{…}}` → **恰好一行** ack
`{"ok","error","fingerprint","endpoint","v":2,"uses":[…]}` → `stream` 原样双向字节 · `capture` 一行 `{"stdout","stderr","exit_status"}` 后结束 ·
`forward` 每接进一条连接一行 `{"accepted":n}`。上行（`link-data`）= 原来子进程的 stdin；`link-close` = 原来「界面走了」。

**流控**：下行逐链路信用 —— `link-open` 给初始窗口，后端发一块扣一块，扣不到就等；客户端读走之后 `link-credit` 还回来
⇒ 一条不读的链路在这条流上最多占一个窗口，堵不住别的链路、别的帧与应答。上行一次一块：`link-data` 的应答在那块**写进链路之后**才回。
**链路属于开它的那条流连接**：连接没了（monitor 走了）⇒ 它开的链路全部收掉。四条都是 `Run::Builtin`，**只在帧面**（CLI 面不派生：一次性进程没有「连接」可言）。

#### `link-open`：开一条链路

| 方向 | 形状 |
|---|---|
| `args` | `{"link":"<不透明 id，客户端给、客户端负责唯一>","window":<初始信用，字节；必须在 [32 KiB, 16 MiB] 之内，否则 `invalid_args`>,"dial":{DialRequest}}` |
| `data` | 无（登记上、任务起了就回 `ok` —— **不等拨通**：拨通与否在链路字节里那一行 ack） |

`dial` 就是 C2 那份蛇形键请求：`host · port · user · key_path · host_key_fingerprint · command · endpoints · jump · use（stream｜capture｜forward｜files）·
capture{max_bytes,abort_marker} · forward{local_port,remote_host,remote_port} · stages · probe`，外加 **`agent_sock`**（Unix：客户端此刻的
`SSH_AUTH_SOCK` —— 常驻后端活得比任何一个客户端都长，它自己身上那份可能早就不指向活的 agent；缺席 = 用后端自己的环境）。
`probe` / `stages` 的链路**不进连接池**（测试连接要看的就是一次真拨号）。
〔SR1b · 2026-09-24〕`use:"files"`：在池里那条连接上开 sftp 子系统（`dial/sftp.rs`），ack 之后**一问一答** ——
上行每一行一个请求 `{"op":…}`，下行每一行一个应答；`op` ∈ `home` · `stat{path}` · `read{path,max}` · `put{path,size,mode,verify}`（该行之后紧跟 `size` 个原始字节，
上限 64 MiB）· `remove{path}` · `mkdirs{path}`。失败那一形 `{"code","message"}`，`code` ∈ `fenced`（远端写围栏拒）· `io` · `too_big` · `bad_request` · `unknown_op`。
🔴 **写只许两处**：远端 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`（用户 V89；`INVARIANTS §41.6` 的 SR1b 订正）。读不受限。
它服务自部署（F08 后端二进制 · `.build_id` · `ccm` 入口 · cc-acct-iso），业务判定在 monitor（`dial_host::RemoteFs`）。
`use:"subsystem"`（把原始 SFTP 字节交给客户端）**不开** ⇒ 回 `unsupported_use`：SFTP 协议住后端，客户端只有 `files` 与 `transfer-*` 两条路。
错误 code：`invalid_args` · `unsupported_use` · `duplicate_link` · `too_many_links`（每连接 256 条）。

#### `link-data`：往链路里送一块上行字节

| 方向 | 形状 |
|---|---|
| `args` | `{"link","data":"<base64；解码后 ≤ 32 KiB>"}` |
| `data` | 无。应答在这一块**写进链路之后**才回（背压：客户端同一条链路同时只该有一块在途） |

就地分派（不进独立任务）⇒ 同一条链路的块按到达顺序写。错误 code：`invalid_args` · `no_such_link` · `link_busy`（上行队列满 —— 客户端没守「一次一块」）· `link_closed`。

#### `link-credit`：还下行信用

| 方向 | 形状 |
|---|---|
| `args` | `{"link","bytes":<客户端读走了多少>}` |
| `data` | 无 |

monitor 的做法：链路的读者每读走半个窗口就还一次（`link_mux.rs`）⇒ 没人读的链路不还，后端的下行泵停在信号量上。
累计信用不许超过 16 MiB：还得比读走的多（不守约）⇒ `invalid_args`，不替它夹。错误 code：`invalid_args` · `no_such_link`。

#### `link-close`：关一条链路

| 方向 | 形状 |
|---|---|
| `args` | `{"link"}` |
| `data` | 无 |

那条链路的拨号 / 服务 / 两台泵一起收掉；关一条不存在的链路是幂等的（同 `cancel`）。错误 code：`invalid_args`。
它就是 C2 那一版「界面走了、子进程的管子断了」：`stream` 的对拷就地停、`forward` 放掉本机口并收掉在飞的隧道；
那条 SSH 连接**不跟着断**（别的链路可能还在用它 —— 最后一条走了它才断）。monitor 侧的链路句柄被丢时自动发这一条。

#### 传输四条（〔SR1b〕2026-09-24）—— **传输台住本机常驻后端，SFTP 与其它 SSH 同一条连接**

用户 V89「SFTP 进本机常驻后端，只写暂存区」：传输台从界面进程搬进本机常驻后端（`设计/60 §4.6`）；
窗口那一侧的 `call(transfer-upload | transfer-download)` / `subscribe(transfer/<id>)` 一个字不变，monitor 只做中继。
**上传只写远端暂存区** `~/.cc-monitor/staging/<key>.part`（落进用户目录由远端后端 `files-commit-upload` 做）；**下载远端只读**，
本机落点 `<落点>.part` ＋ 改名上位（本机那一下写是文件管理那一面的写，先过路径解析；〔FN1 · V119〕会话文件围栏拿掉了）。
存亡规矩：**撤** ⇒ 上传删暂存件、下载留 `.part`；**失败** ⇒ 上传留暂存件、下载**也留** `.part`（〔DP1〕弱网断线就是失败，删了续传的本钱就没了；一个字节都没落的空 `.part` 才清）；续传两侧都先对尾块。上传失败之后先把已发出的写全部等到回话再走（不留晚到的写）。
票表**每条流连接一张**：连接没了（monitor 走了）⇒ 在册的一律撤。四条都是 `Run::Builtin`，**只在帧面**。
一条连接上的通道预算按连接记：session 通道 8 格（长流 · 查询 · sftp 同一道闸），其中传输至多 4 格（排队，不报错）。

#### `transfer-upload`：开单（上传）

| 方向 | 形状 |
|---|---|
| `args` | `{"dial":{DialRequest，同 link-open},"local_path":"<本机一份普通文件的路径>"}` |
| `data` | `{"id":"xfer-<n>","key":"<32 位十六进制>"}` —— `key` = 暂存件的键（本机路径 · 大小 · 修改时间派生；同一份文件重拖一次同一个键 ⇒ 续传），提交时交给远端后端 |

**不起跑**。错误 code：`bad_args` · `io_failed`（读不到本机文件）· `busy`（同一个键已有一张票在册）· `too_many_transfers`（每连接 64 张）。

#### `transfer-download`：开单（下载）

| 方向 | 形状 |
|---|---|
| `args` | `{"dial":{…},"remote_path":"<远端路径>","local_path":"<本机落点，绝对路径>"}` |
| `data` | `{"id":"xfer-<n>"}` |

本机落点**当场**过路径解析（绝对路径 · 有文件名 · 父目录在盘上；出声早），起跑时再过一次。错误 code：`bad_args` · `refused`（路径解析拒）· `too_many_transfers`。
〔FN1 · V119〕此前两处都判「落点是不是会话文件」（monitor 开单时一道、后端一道），都删了。

#### `transfer-start`：起跑

| 方向 | 形状 |
|---|---|
| `args` | `{"id"}` |
| `data` | 无。之后进度与终局走出方向 `transfer` 帧（第一帧是此刻；带 `end` 的是最后一帧），终局之后票摘掉 |

错误 code：`bad_args` · `no_such_transfer` · `already_started`（一趟只起跑一次）。

#### `transfer-stop`：撤

| 方向 | 形状 |
|---|---|
| `args` | `{"id"}` |
| `data` | 无（撤一张不在册的票是幂等的，同 `cancel`） |

没起跑的票当场摘掉；起跑了的由它自己收场（上传删暂存件 / 下载留 `.part`），终局帧 `cancelled`。错误 code：`bad_args`。

### argv 三分（U6b-2）

后端认识的每个 `--token` 恰好属于三类之一：

| 类 | 成员 | 语义 |
|---|---|---|
| **流模式 flag** | `--with-bg` · `--tail-only` · `--with-rbind-token` | 出现即剥离并置位，**不影响模式判定** |
| **一次性查询子命令** | 上面那张查询表的全部 | **只有 `args[0]` 是其中之一才进查询模式** |
| **子命令选项** | `--accts-dir` · `--after-ms` · `--include-tools` · `--limit` · `--scope` | 只在某条子命令之后才有意义，后端顶层不解释 |

**在此之前是二分**（剥掉流 flag、剩下非空就当查询），实测后果：

```text
$ cc-monitor-backend --some-future-flag
cc-monitor-backend query error: unknown argument: --some-future-flag
rc=2
```

**未知 flag 在流位置 ⇒ exit 2、一个字节都不输出、没有 hello。** monitor 看到的和「后端崩了」无法区分 ⇒ 重连 ⇒ 发同一个 flag ⇒ **死循环**（2026-07-09 事故的形状）。§26 的 `every_capability_token_is_strippable` 挡不住它 —— 那条只覆盖**与已声明能力绑定**的 flag。

现在：**未知 `--flag` 忽略 + 一行 warn，照常进流模式发 hello**（新版 monitor × 旧版后端能拿到握手、看出对面旧、自行降级）；**未知裸参数仍报错 exit 2**（那是明确的调用错误，任何未来协议都不会把裸参数放 `args[0]`）。

⚠ **这张表漏一项的后果比旧行为更糟**：`args[0]` 认不出来 ⇒ 当成流模式 ⇒ 那条子命令**静默变成起了个流**，调用方拿到一堆 jsonl 行而不是查询结果（v3.4.0 `--account-trust-zero` 漏登记那次事故的加强版）。由 `every_dispatched_token_is_classified` + `the_three_classes_do_not_overlap` + `every_listed_subcommand_is_actually_dispatched` 三条机检钉住。

### 一次性历史查询（带参数启动后端，issue #16）

带参数 exec = 一次性查询模式，干完即退、**不进流式协议**：

- `--list-projects` → 每行 `{dirName, projectPath, sessionCount, lastActivityMs, sessionIds}`。`sessionIds`（`K-R83`，09-12）= 该项目下**全部会话 sid**（升序，`<sid>.jsonl` 的 stem，与 `--list-sessions` 的 `sessionId` 同一个字符串），**与 `sessionCount` 恒等长**。★ 它在的理由：客户端侧的「星标数 / 隐藏数 / 有没有活会话」三个数**全部按 sid 索引**，缺的一直是「这个项目下有哪几个 sid」——带上它，客户端**一次调用**就算得出，不必每个项目再发一次 `--list-sessions`（那是 N 次进程 spawn，而项目列表是常开界面）。⚠ **它不在 `sessionCount` 之外多读一个字节**（同一趟 `read_dir`）。⚠ **客户端读法**：字段**不在**（旧后端）⇒ 那三个数是「**不知道**」，**不是 0**；字段在但与 `sessionCount` 长度对不上 ⇒ 这一行坏了，同样按「不知道」处理，**不许**拿手上那几个算出一个看起来像真值的少数
- `--list-sessions <project_dir>` → 每行 `{sessionId, jsonlPath, startedAtMs, updatedAtMs, messageCountApprox, firstUserExcerpt, aiTitle, cwd}`
- `--read-session <jsonl_path>` → 原样透传该 jsonl 字节（monitor 侧走既有 `parse_line` 管线）
- `--read-session-tail <jsonl_path> <N>`（Batch9-F30，p1g）→ **尾部优先**：首行 meta `{"kind":"snapshot_meta","total":T,"tail_from":F}`（可计行口径 = watcher 行号空间），随后原样输出行 [F,T)（最新 N 行）再输出 [0,F)。快照拉取用它——最新内容第一批就位、旧历史回填；monitor 按 meta 两段编 seq（前端 seq 二分插入天然支持乱序），`total` 做精确完整性对账。回填在途经 `snapshot-inflight` 事件驱动前端 batch 模式（替代纯 300ms 静默启发式，5min 防呆上限）
- `--read-session-from-offset <jsonl_path> <offset>`（backend-02，Phase 1 offset 续拉，`observe/history_query.rs`）→ seek 到字节 `offset`（0-based）后**原样透传 [offset, EOF]**，语义**逐字节 = aterm `tail -c +(offset+1)`**。`offset` 就是客户端从 `line` 帧 `byte_offset` 持久化下来的续点（重连/断线后带上）——**别拿 `seq` 当续点**，那是 per-stream 序数。**截断/重写不在此判**：远端 size < offset 时 seek 过 EOF → 读空 → 透传空，安全无副作用；客户端另经 size 查检测后自行决策 reset（同 aterm 的 `offsetByPath`）。透传而非逐行，理由同 `--read-session`。路径守卫与 `--read-session` 同一套
- `--search <query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]`（issue #28）→ 服务端在远端 CPU 扫 `projects/**/*.jsonl` 做全文搜索（避免拉整库回本地），**每命中会话一行** camelCase `SessionHits` JSON（形状严格对齐 monitor `search::SessionHits`，monitor 补 `origin` 后与本地结果合并）
  - 🔴 `K-R100`（09-13）两处改动，**都在 wire 上看得见**：
    ① **行序 = snippet 预算顺序 = 最近优先**（按 jsonl mtime 降序）。此前按 `WalkDir`（`readdir`）先走到的顺序花预算，与 monitor 的 `updatedAt desc` 几乎正交（实测前 3 重合 0/3）——而两侧的**展示**顺序都是最近优先。
    ② 每行多一个 **`hitsTruncated: bool`**：本会话有命中因**全局 `--limit` 用完**而拿不到 snippet。此前 `hitCount: 12, hits: []` 在下游与「这个会话没什么可看的」同形，monitor 合并时又逐字 `truncated: local.truncated` 把远端那一半丢掉 ⇒ **远端截断界面一个字不说**。⚠ 与「本会话超 `PER_SESSION_CAP`(30) 条只列前 30」**不是一回事**，后者不置这个位。
    - 兼容：旧后端不发这个字段 ⇒ monitor 侧 `serde(default)` = `false`，退化成收口前的行为，不炸。
> 〔`设计/50` 删用量 —— 这里原有一条 `--usage`（服务端在远端聚合用量，per-requestId 每字段 MAX）。
> 用量的**聚合轴**整轴退役 ⇒ 子命令与它的实现（那份 `usage_query.rs`，**已删**）一起删了，
> monitor 侧的 fan-out 消费者同拍删除。`SUBCOMMANDS` 27 → 25（另一条是 `--oneshot-session`）。〕

- `--list-accounts [--accts-dir <p>]`（A2 多账号，`src/backend/observe/accounts_query.rs`）→ 读 cc-acct-iso 的 manifest（`$ACCTS_DIR/accounts.json`，契约 v1）。**首行** `{"kind":"accounts-meta","enabled":bool,"acctsDir","manifestPath","updatedAt","sharedStore","count","error"}`，其后每账号一行 `{name,email,configDir,isDefault,mode,exists,loggedIn}`。**"未启用多账号"是正常状态**：manifest 缺失/坏/版本不支持 → `enabled:false` + `error` 人话原因 + **exit 0**（不是错误）。`loggedIn` 仅 stat `.credentials.json` 存在性。账号库目录解析：`--accts-dir` > `~/.cc-acct-iso/config` 的 `ACCTS_DIR=`（**正则抠值，绝不 source**）> `$HOME/.claude-accts`
- `--session-accounts [--accts-dir <p>]`（A2；`launchId` 是 `K-P5f`）→ 扫 `<claude_dir>/sessions/<PID>.json` 拿 pid，读 `/proc/<pid>/environ` **只抠两个写死的键**（`CLAUDE_CONFIG_DIR` 与 `CCM_LAUNCH_ID`；**键名不是参数**，所以这条查询不是「任意环境变量读」原语，也**绝不回传整个环境快照**），`CLAUDE_CONFIG_DIR` 反查 manifest 得账号名。每条一行 `{pid,sessionId,cwd,configDir,account,bare,alive,launchId}`。`account:null` = 查不到（**不猜**）；**`bare:true` = 进程活着、`/proc/<pid>/environ` 这一刻读得到、而没设 `CLAUDE_CONFIG_DIR`（裸起）——这个布尔的语义钉死在那一个变量上，加了第二个键也没有拓宽它**（没设 `CCM_LAUNCH_ID` 由 `launchId:null` 自己表达）。⚠ 「读得到」这个合取项是 `K-R21`（09-03）补的，**语义是收窄不是拓宽**：environ 在 exec 窗口里（60–140 µs）与进程成僵尸之后**读得到却回 0 字节 / 读不到**，从前那一刻会被报成斩钉截铁的 `account:"<账号0>"` + `bare:true`，而 `alive` 仍是 `true`（判活读的是 `/proc/<pid>/stat`，与 `environ` 不是同一次读）⇒ **一条真跑在别的账号下的会话会被报成账号 0 的，且无声无息**。现在那一刻报 `configDir:null` + `account:null` + `bare:false`（=「不知道」，**出参形状没变、没有新字段**）。`launchId` = 起会话方铸进这条会话进程环境的**身份 token**（写侧住 `history.rs::LAUNCH_ID_VAR`），`null` = **不作数**，五种原因合并且**刻意不区分**：没设 / 形状过不了白名单（`[A-Za-z0-9_-]`，1..=128）/ **同一个 token 落在一条以上活会话上** / 进程已死 / **读那一刻环境取不到**。⚠ 第五种是 `K-R21` 现打出来的，**它一直都在、只是从前混在「没设」里数不出来**（读侧那个 `Option` 装着四件事）——这不是新增了一种行为，是把「四种」这句旧话订正成实话；`configDir` 那一半已经把它拆出来了，身份这一半仍按「要区分就得给出参加状态位 = 改上线契约」那条裁定合并着。⚠ **`launchId` 不是硬真相**：它是**继承型**环境变量（claude spawn 的子进程原样继承），后端只能判「同一批里唯一」，判不出「确实是它的」——父会话已退出时那个继承值仍会被报出来。**additive**：老后端不出这个键，下游读成 `null`。⇒ 账号那一半（`configDir`/`account`/`bare`）仍是"某条**正在跑**的会话属于哪个账号"的唯一硬真相（会话 jsonl 里没有任何账号字段）；身份那一半（`launchId`）**不是**，别把上一句读到它头上
- `--account-trust <configDir> <cwd> [--accts-dir <p>]`（A2）→ 换号 resume 前的信任预检（首次用某账号进某目录，CC 会弹信任确认、会卡住自动化）。单行 `{"trusted":bool,"known":bool,"error":null}`。**安全**：`configDir` 必须逐字 ∈ manifest 的账号列表，否则 exit 2 + stderr `{"code":"unknown_config_dir",...}`——避免退化成任意文件读原语；**只回三个布尔/字符串字段，绝不回传 `.claude.json` 内容**（内含 `mcpServers` 的环境变量，可能有 API key）
- `--account-trust-zero <cwd>`（A2）→ **账号 0**（未启用多账号时那个原生身份）的信任预检，返回形状同 `--account-trust`。**为什么单开一个动词而不是给 `--account-trust` 传空 `configDir`**：账号 0 没有 config dir，而空串是被明令禁止的拼法（空值 ≠ 未设）；且它的 `.claude.json` 原生根是 `$HOME`、不在共享账号库里 ⇒ 路径来源本就不同，合并只能靠哨兵值区分，比多一个动词更易错。**不收任何文件/配置目录路径参数**：它收 `cwd`，但那只当 `projects` 里的**查表键**，`.claude.json` 的根写死 `$HOME` ⇒ 连"任意文件读"的面都没有（`account_trust_zero_takes_no_path_argument` 钉住）
- `--acct-iso-status`（A3 第二波，`src/backend/accounts/iso.rs`）→ **这台机器**上装没装 `cc-acct-iso`。单行 `{"installed":bool,"path":string|null,"looked":string|null}`：先查 `$HOME/.local/bin/cc-acct-iso`（install 脚本的软链落点）、再查 `PATH`，与远端那条 `PATH="$HOME/.local/bin:$PATH" command -v cc-acct-iso` 同一个顺序；**「没装」是答案不是错误**（exit 0，`looked` 说清查过哪儿）。按「可执行文件在不在」判 ⇒ 非 unix 上恒 `installed:false`。只读、不起进程。
- `--acct-iso-shellinit`（A3 第二波，同上）→ 起一次本机 `cc-acct-iso shellinit`（经插件通用调用口：argv 直传不过 shell、`timeout` 前缀给子进程期限、环境白名单），退出码 0 时把它的 stdout **原样**吐出（BEGIN/END 围栏由 monitor 那侧校验，本命令不再写第二份围栏常量）。失败 exit 2 + stderr `{"code","message"}`，`code` ∈ `not_installed` · `timed_out` · `tool_failed` · `not_run` · `bad_args`。被起的那一条只读（`cmd_shellinit` 全是 `printf`）。
- `--fork-session <args>`（G2 branch-anywhere，`src/backend/control/fork_write.rs`）→ 从指定消息处分叉出一个新会话文件。**后端唯一的写盘入口**——其余一切子命令只读；`readonly_guard` 的写白名单按路径单独盯着 `control/fork_write.rs` 这一个文件（`src/doc/INVARIANTS.md` §41.6）
- `--tmux-notify <backend_pid> <backend_starttime>`（P4b zero-poll-liveness）→ **不是查询**，是 tmux hook 子进程走的通路：校验身份后给正在跑的后端发一个信号叫它立刻重扫 tmux，**完全不碰文件系统**。两个参数缺一或非整数 ⇒ exit 2。**必须同时比对 starttime 而不只看 pid 存在**：后端退出后那个 pid 可能已被别的进程占用，误发信号轻则无效、重则打断无关进程（很多程序把该信号当自定义控制信号，默认处置直接终止）。身份对不上 ⇒ **静默 exit 0，不做事**

- `--list-subagents <父会话 jsonl 路径>`（P7c-1，p1z）→ 列该会话的 **subagent 候选**，
  每行一个 `{path, description, timestamp}`（拿不到就 `null`，**不猜**）。目录不存在 = 该会话没有
  subagent ⇒ **回空 + exit 0**，不是错。路径走**与读会话族同一条围栏**
  （`fence_under_projects`，subagent 目录本来就在 `<claude_dir>/projects/` 内）。
  ★ **它只列不挑**：按 description 精确匹配、按 `tool_use_timestamp` 挑最近的那一步**留在客户端**
  —— 那套逻辑本机远端共用一份，别在两侧各写一遍。挑中之后用**既有的** `--read-session` 取内容。
- `--list-user-inputs [--from <offset>] <jsonl_path>`（SE1，`设计/10 §2.2b ⑥`）→ 大纲的数据源：该会话里每一条**主线用户输入**（头 ＋ 每条一行 ＋ 尾），形状与口径见 **§10.4**

#### 控制面的 CLI 那一半（P4d，p1y）

上面那些都是**读面**。控制面（起会话 / 杀会话）此前**只走流连接的命令信封**，
bash 脚本与 skill 调不到。p1y 起，它们各有一个一次性 CLI 入口：

- `--launch`（stdin `args` JSON → stdout 应答 JSON）→ 建 tmux 会话 / 往已有会话键入载荷。
- `--kill`（同上）→ 杀一个 tmux 会话，仍过 §34 三道门。
- `--ping`（stdin 可空）→ 存活探测，回 `{}`。
- `--backend-probe`（不读 stdin）→ **能力探测口**，回 `{proto, buildId, commands}`；
  `commands` 是这台后端 **真能派发**的 CLI 控制面子命令清单。
  集成方按**能力**兼容，不要按版本号 —— 范式与 `ccm --ccm-probe` 一致。

★ **这四条不引入任何新语义**：`--launch` / `--kill` / `--ping` 的实现就是流连接上
`launch` / `kill` / `ping` 那几条命令自己的处理器（`control/cli_control.rs` 从
`inbound::REGISTRY` 派发），**一份实现、两个入口**。因此它们的 `args` 字段、错误码
与流那面**逐字相同**，见「入方向：流连接上的命令信封」。

★ `cancel` **不在** CLI 面：它取消的是同一条连接上在飞的另一条命令，而一次性 exec
是「1 请求 1 响应 1 退出、无 request-id」，本进程里没有第二条命令可取消。
要停一条 CLI 命令：杀那个进程。

信封与 `--resolve` 同形：stdin 一段 JSON、stdout 一行紧凑 JSON、exit 0；
错则 exit 2 + stderr 一行 `{code, message}`。

错误写 stderr + 退出码 2（`--account-trust` 用 `--resolve` 那套结构化 `{code,message}` JSON）。**读会话那一族**（`--read-session` / `--read-session-tail` / `--read-session-from-offset` / `--fork-session`）的路径参数严格限制在 `<claude_dir>/projects/` 内（canonicalize 后前缀校验，拒穿越 / symlink 逃逸 / 非 jsonl）。**账号一族不走这条**，各有各的判据：`--accts-dir <p>` 解析到 `~/.cc-acct-iso/config` 或 `$HOME/.claude-accts`；`--account-trust <configDir>` 靠「逐字 ∈ manifest」而非 projects 前缀；`--tmux-notify` 根本不碰文件系统。**旧后端兼容**：不认参数的旧版会照常发 `hello` 进流模式——monitor 以"首行是 hello 帧"识别旧版并提示升级（优雅降级，无版本协商）。

**`K-R86` 追加一条（09-13）**：`--capture-pane <会话名>` —— **只读**地抓一次某个 tmux 会话
**此刻**那一屏的文本。它是上面读面那一族的邻居，但读的不是文件而是屏幕，所以单列在这里。

- **入参只有一个位置参数**：tmux 会话名（**不读 stdin**）。名字的形状与 `kill` 那条逐字同一套
  （非空、无控制字符、不含 `:` / `=` —— 后两个是 tmux 目标语法的一部分），
  后端侧自己拼成精确目标 `=名:`（`F01`：裸 `-t <名>` 会按「精确名 → 名字开头 → glob」
  解析，只有 `sib-2` 在时 `-t sib` 抓的是 `sib-2`）。
- **成功**：那一屏**原样**写 stdout（同 `--read-session` 的透传口径，不包 JSON），exit 0。
  ⚠ **空屏是合法的成功**：一个刚建起来、什么都没打印的 pane 抓回来就是空串 + exit 0。
  ⇒ 客户端判「抓没抓到」**看退出码**，不看输出是不是空。
- **失败**：stderr 一行 `{code, message}` + exit 2（同 `--resolve` 那套信封）。四个码
  **刻意分得开**，别在客户端把它们压成一句：
  - `no_tmux` —— 这台机上 `tmux` 这个程序起不来（没装 / 不在 PATH）；
  - `no_server` —— tmux 在，但一个 server 都没有 ⇒ 无屏可抓；
  - `no_such_session` —— server 在，而这个会话不存在；
  - `capture_failed` —— 上面三档都不是，**tmux 的原话原样带回**（认不出来时不猜一个具体原因）；
  - `invalid_args` —— 会话名的形状过不了，**在起进程之前**就拒了。
- **只读**：`tmux -u capture-pane -p -t '=名:'`，argv 直传不过 shell；不 attach、
  不落 tmux buffer、不写任何文件。这一处起进程在后端侧登记于
  `readonly_guard::spawn_registry::ALLOWED`，而「它只读」由
  `readonly_guard::capture_is_read_only` 逐元素钉住 argv（那张登记表的键分不出被调的子命令）。
- 🔴 **它只抓一次**：「抓几次 / 隔多久再抓」由**调用方**决定 —— 后端侧那条零定时器铁律
  （`no_timer_guard`）不许它自己长出节拍。
- ⚠ **`--ping` 那族的「不读 stdin」纪律同样适用**：声明无输入的命令必须秒回，
  不许挂住等一个永远不来的输入（`P4f` 实测过 `bus-list` 那次 6 秒被掐死）。

> 〔`设计/50` 删用量 —— 这里原有 `--oneshot-session <名字后缀> <存活秒数>` 的整段说明
> （`K-R87` 09-13 加的一次性会话原语，含五个错误码与那条外部看门狗的形状）。
> 它当初就是为用量探针建的；探针轴退役后它零生产调用方，连同那份 `oneshot_session.rs`（1122 行，**已删**）一起删除。其中「看门狗必须是外部进程、因为零定时器铁律只管本 crate 源码文本」
> 这条论证仍然成立、只是今天在本仓没有实例了。〕

**P4f 追加三条**：`--bus-list` / `--bus-send` / `--bus-kill`（cc-bus 的基础命令，见上面各自的小节）。
它们与帧面走**同一个 `run`**，CLI 面这一层不写第二份实现。

**`K-R113` 追加一条（09-13）**：`--bus-state` —— 总线名单 ＋ spawn 台账**一次回全**
（见上面它自己那一小节）。同上，与帧面同一个 `run`；**不读 stdin**，声明无输入的命令必须秒回。

**BS1b 追加一条（09-24）**：`--bus-spawn` —— 派生一个协作 agent（见上面它自己那一小节）。
同上，与帧面同一个 `run`；**读 stdin**（那段 JSON 就是它的 `args`）。⚠ 它**起一个真 agent**。

**C4e 追加一条（09-25）**：`--bus-broadcast` —— 给总线上在线的成员群发一条（见上面它自己那一小节）。
同上，与帧面同一个 `run`；**读 stdin**（那段 JSON 就是它的 `args`）。

**步 `24f` 追加四条（09-20）**：`--files-ls` / `--files-stat` / `--files-find` /
`--files-index-status` —— `files-read` 这一族的 CLI 面（逐条见上面各自那一小节）。
同上，与帧面走**同一个 `run`**，CLI 面这一层不写第二份实现。
前三条读 stdin（那段 JSON 就是它们的 `args`）；`--files-index-status` **不读 stdin** ——
声明无输入的命令必须秒回，不许挂住等一个永远不来的输入。
⚠ **它们整族只读**，与上面读面那一族同档；🔴 但 `--files-find` 在一台刚起来的后端上
恒回 `index_missing:true`（后端没有自己去建索引的节拍），别把它当成「搜不到」。

**步 `24f` 第三刀追加两条（09-21）**：`--files-index-rebuild` / `--files-browse` ——
同族的第五、第六条（逐条见上面各自那一小节）。同上，与帧面走**同一个 `run`**，
两条**都读 stdin**（那段 JSON 就是它们的 `args`）。
🔴 **但那条出路在 CLI 面上配不起来** —— 现打，别读宽：索引是「那一个后端进程」手上的
那一份，而 CLI 面是「1 exec = 1 请求 1 响应 1 退出」⇒ `--files-index-rebuild` 建好的
索引随那个进程一起没了，紧接着的 `--files-find` 那一 exec **照旧回 `index_missing:true`**
（本机 debug 档现打：rebuild 回 `entries:4`，下一个 exec 的 `--files-index-status`
回 `index_missing:true`）。
⇒ **要「建完就能查」得走帧面**（一条长连接上多次往返；现打：`files-find` →
`files-index-rebuild` → `files-find` 三条同连接，第三条 `index_missing:false` 并命中）。
CLI 面这两条的用处是**量一趟遍历** ／ **在一个常驻后端进程里换名单**，
不是给下一个 exec 预热。
⚠ 同一条边界也适用于 `--files-browse`（浏览名单同样是进程级的那一份）。
⚠ 而「隔多久再发一次」始终是调用方的事 —— 后端**声明**的建议周期在
`--files-index-status` 的 `rewalk_interval_secs` 里，后端自己**不会**按那个周期动
（零定时器铁律）。

**F7a 追加两条（第三波，2026-09-24）**：`--files-read-text` / `--files-home` —— 同族第七、第八条
（逐条见上面各自那一小节）。同上，与帧面走**同一个 `run`**；`--files-read-text` 读 stdin
（那段 JSON 就是它的 `args`），`--files-home` **不读 stdin**。两条都只读。

**`K-H1` 追加一条**：`--relay` —— 起 **HTTP 中转**（搬字节那半）。它与上面每一条都不同族：
不是一次性查询，而是一个**常驻**进程，起来就不返回。

- **只监听 `127.0.0.1`**，不对外暴露；端口默认 `8788`，`CCM_RELAY_PORT` 可盖。
- 〔RL1 · V107〕**进程内那一形**：流模式（`--tail-only` 等，stdio 或常驻监听口两条载体一样）的后端**被交了** `CCM_RELAY_PORT`
  ⇒ 在本进程里起同一个中转（中转 ＋ 上游选择同一份代码，`relay::listen::host` ＋ `accounts::upstream::host_relay`），接受循环跑一条专属线程，
  随进程生死（常驻后端按「退出行为」留或退，中转一起）。与上面独立那一形的差别只有三格：**端口没有缺省值**（认不出 ⇒ 不开）·
  **tee 丢弃**（stdout 是 wire）· **起不来不退出**（出声，后端照常服务）。没交端口 ⇒ 不开（远端经 SSH exec 起的流模式后端就是这一格）。
  凭据文件路径同样由 `CCM_APIKEY_CREDENTIALS` 交（monitor 起本机后端时交，与它自己写的那份同一个路径）。
- 默认上游**每个 agent 一行**〔条 59 / 条 60，2026-09-24 订正；先前这里写的是一个**进程级**的上游默认
  （`https://api.anthropic.com`，由一个进程级环境变量盖）—— 已整删〕。它是**上游选择**的表，不是中转的配置：
  今天只登记了 `claude-code`（默认 `https://api.anthropic.com`，`CCM_AGENT_UPSTREAM_CLAUDE_CODE` 只盖这一家；
  〔R3〕这个变量先前叫中转的名字，改名不留兼容读旧名）；
  **codex 刻意没登记**（它的默认上游本仓零证据）⇒ 它走 `/t/` 回 **502**，不回落到任何一家。
  表住 `agents::Adapter::upstream`（〔NT2 · V25〕跟着适配层）。**基址里可以带一段路径前缀**
  （`K-R1`，形如 `https://<host>/<前缀>`）。
  ⚠ **订正〔`K-R1` 09-04〕**：这一行先前逐字写着「`http://` 只给本机夹具用」——**那半句今天不准确了**。
  今天的分界线是**回环**：`http://` 打到本机回环是一等公民（〔用 09-04〕逐字要「还可以接本地部署的」），
  而**明文 + 非回环**的那一行会被装表那一步**拒掉并出声**
  （判据 `accounts::table::tests::a_plaintext_upstream_is_only_allowed_on_loopback`；上游选择 2026-09-24 搬出了 `relay/`）。
  `裁-1`「只准 TLS」**没有被推翻**，升的只有回环这一格。
- 路由：`claude` 把 `ANTHROPIC_BASE_URL` 指到 `http://127.0.0.1:<port>/s/<agent>/<account>/<key>`
  （**代入**：apikey 表里有这一行，换上这一行的 key）或 `…/t/<agent>/<account>/<key>`
  （**直通**〔`设计/20 §2`〕：永不代入，下游那份鉴权头逐字节原样上去；表里没这一行时发到那个 agent 自己的默认上游），
  中转把前缀与三段剥掉、其余路径与查询串**原样**转给上游。
  ⚠ **那一行的基址若带路径前缀，前缀会被接在这段原样路径的前面**〔`K-R1`，住址
  `upstream::Base::upstream_target`〕—— 前缀为空时与改前**逐字节相同**。
  🔴 中转**不查重、不合并重复的段**：配了 `<host>/v1` 而客户端发 `/v1/messages` 的人
  会得到 `/v1/v1/messages`（装表时给那一行记一条 note 说出来，见 `table::NOTE_PATH_PREFIX`）。
  ⚠ **`<account>` 那一段是 `K-H2` 加的**。〔条 49，2026-09-24 订正：先前这里写「中转……只拿 `<account>` 查表」，
  拆键之后不准了〕apikey 表的键是 **`<agent>` ＋ `<account>` 两段**（claude-code 的 3 号与 codex 的 3 号是两行）：
  `/s/` 表里查不到那一对 ⇒ **404，一个字节都不发上游**（不回落到别的账号的 key，
  也不回落到默认上游）。三段对**中转**仍然都是不透明串 —— 它不解释它们，原样交给上游选择去查。
  ⚠ 线上字节一个没变，变的是查表语义。
  ⚠⚠ **老的三段形状 `/s/<agent>/<key>/…` 不会被解析器拒掉**，它会被重读成
  `account=<key>`；挡住它的是「表里查不到」那一格，不是解析器
  （判据 `route::tests::the_old_three_segment_shape_is_not_rejected_here_it_is_reread_as_a_different_route`）。
- 谁设 `ANTHROPIC_BASE_URL`〔2026-09-24 订正；先前这里是 08-28 的读数「生产代码 0 处、没有入口」，`K-H2b` 之后不成立〕：
  monitor 起**本机**会话时（`payload::relay_endpoint_for`，`设计/20 §3.2` 的 monitor 半）——
  apikey 表里有 (agent, 账号) 这一行 ⇒ 注入 `/s/`（中转没在跑 ⇒ **拒绝起会话**）；
  没有这一行 ⇒ 默认**不注入**。全量注入（订阅号也注 `/t/`）**带开关、默认关**：
  monitor 进程环境里 `CCM_RELAY_ALL_SESSIONS=1` 才开；开了也只给登记了默认上游的 agent 注（codex 不注），
  中转没在跑 ⇒ 不注（照旧直连，不拒绝）。远端机器那一半不注入。
- 中转**自己造**的状态码〔`设计/20 §3.1a`，每个码只有一处常量，三组两两不相交〕：
  请求读不懂 400 / 411 / 413 · 路由不成立 404（`/s/` 表里无行，或路径根本不是路由形状）与 502（`/t/` 的 agent 没登记默认上游）·
  在途连接顶满 503 · 🔴 **上游连不上 / 没回应 / 回的不是 HTTP ⇒ 504**（2026-09-24 前是 502，与上游选择撞码），
  响应体第二行是一句人话：`上游 <主机>:<端口> <结果>。卡在<哪一步>这一步。`。
  ⚠ 上游**自己**答的 5xx 原样转发，与上面这几个码共用值域 —— 分得开它们的只有那句话。
- 响应**逐块透传绝不缓冲**；同一批字节里的 SSE 事件抄一份到**本进程的 stdout**
  （NDJSON；要落文件由启动方重定向）。**这条流上今天是三种行** —— 分母不是印象，是
  `relay/tee.rs` **生产段**（剥掉 `#[cfg(test)]` 后 304 行）里**把一行送出去的全部落点，
  现打 08-28 恰好 4 处**：`:220` · `:241` · `:267` · `:190`（后两处产的是同一种行）⇒ 三种：
  - **每个响应的首行 `__meta__`**（`tee.rs::open` `:211-220`）：
    `{"__meta__":{"source":"relay","proto":"passthrough-v0","agent":…,"account":…,"key":…,"seq":N}}`
    ⇒ 三格路由键**带**，但在 `__meta__` 对象**里面**。
  - **事件行**（`tee.rs::event` `:233-241`）：
    `{"agent":…,"account":…,"key":…,"event":"<上游 data: 后面那段，转义成一个 JSON 串>"}`
    ⇒ 三格路由键**带**，在**顶层**。
  - **`__dropped__` 报账行**（写线程 `:189-190` · `note_dropped_bytes` `:266-267`）：
    `{"__dropped__":{"lines":N,"bytes":M}}`
    ⇒ ⚠ **那三格一格都不带**，而它走的是**同一条 stdout**（`:183` 与 `:190` 同一个 `w`，
    `:267` 与 `:282` 同一个 `tx`）⇒ 消费方得认得它，它不是「异常时才另开一路」的东西。
    `lines` / `bytes` 的定义见 `tee.rs:48` 那一节。

  ⚠ **「首行」指「每个响应的首行」，不是「这条流的第一行」**：同一个进程里 `__meta__`
  会出现多次，每条连接一次、`seq` 递增。现打（下面那一刀的判定行里逐字带出来的同一段流）：
  `seq:0` 是 `acct-a`、`seq:1` 是 `acct-b`，**同一个后端进程**。
  ⚠ `seq` 在上面**这三种行里只有 `__meta__` 那种**有；`event` 的值是**一个 JSON 串**
  （上游那段逐字节保住但不参与本行结构，理由见 `tee.rs` 头注）。

  ⚠⚠ **这一格被订正过两轮，两个方向都记下来**：
  ① 〔`E` 逮到，08-28〕先前逐字写「每行带 `agent` / `key`」，**漏了 `account`** ——
  路由键从两段变三段是 `K-H2` 自己干的，而这一行没跟着改。
  ② 〔`D4` 逮到，08-28〕修 ① 的那一拍写成了加粗全称「**每行都带那三格路由键**」，
  并自称「把**两种**行的字面形状逐字列出」——**实际是三种**，而 `__dropped__` 那种
  **一格都不带**。⇒ **去修一句假话，换来一句射程更大的假话。**
  病灶逐字记着：**写了一个没有量过人群的全称。** 第三种行就写在 `tee.rs:48` 那个小标题里
  （「还有一种行：`__dropped__`（本轮新增，写进契约）」），**与被引的那份头注在同一个文件**，
  没往下读就下了全称。⇒ 纪律：**写「每 / 所有 / 全部 / 唯一 / 没有任何」之前先现打它的人群、
  把分母写在旁边；给不出分母就不许写全称。**
  ⚠ 归属：`__dropped__` 在本文档缺席是 `K-H1` 期留下的（`git log -S` ⇒ `e4f95ba` / `38f27b3`）；
  `K-H2` 造的是 ② 那句全称与那张自称完整的二分表。

  ⚠⚠ **这一格有牙，但钉的不是内容 —— 要说清是哪一维**〔`D5` 逮到，口径按 `K20`，08-28〕：

  - ⭐ **「这一格里提到的路径还在不在」有人钉着**：
    `doc_claim_registry::tests::every_repo_path_named_in_the_docs_still_resolves`。
    现打变异（**只改这份文档，一行代码没动**）：把上面那个 `relay/tee.rs` 改成一个不存在的文件名
    ⇒ **当场红**，判定行逐字把住址与那个改坏的名字一起印出来（`src/doc/IPC-PROTOCOL.md` 第 807 行）；
    那一趟 **12 passed / 1 failed**。
    ⇒ 上面那些 `tee.rs::open` / `::event` 的住址**改了名会被抓**，不会烂在这里。
  - ❌ **这一格的「内容」与 `tee.rs` 生产段对不对得上，没人对拍。**
    这句**不靠「数判据」成立**，靠**直接演示**：本拍把这一整块改写、`tee.rs` 一个字没动
    ⇒ **全量门禁各道读数逐个不变、`GATE: OK`**。
    ⇒ 三种行的形状 · `seq` 在哪一种上 · `__dropped__` 带不带那三格 —— **这些字今天漂了，门禁不会说。**

  🔴 **别在这里重建「哪些判据看着这份文档」的名单** —— 试过两轮，两轮都被证伪。
  根因**不是数错**：这一族判据按**形状**认人（反引号里带目录的路径 · 散文模式 · 数量形态），
  **不按主题名** —— `doc_claim_registry` 里 `tee` 一个字都没有，而咬住上面那一刀的正是它。
  ⇒ 「grep 判据源码里有没有这个词」这把尺子**在构造上**就量不出「它会不会咬这一格」，
  建在这把尺子上的枚举**必然**不完备，写进散文只会随时间腐、而且腐的方式是让读者更信它。
  ⇒ 那次测量的读数与它的限定，留在件计划 `§4 上报`（**带日期的一次测量**，不是长期断言）。
  ⇒ 这一族的诚实边界：**别把「文档里写着」读成「有人钉着」—— 要接着问「钉的是哪一维」。**
  ⇒ 这份文档这一族的**诚实边界**：别把「文档里写着」读成「有人钉着」。
  ★ 把中转这一族纳进那道判据的人群是**另立一件**的活（放宽人群要单独想形状），**不在这里顺手加**。
- **请求头原样转发，但一个都不落进 tee、不落进日志。**
  ⚠ **例外都在鉴权头上**（`K-H2a` 换头 + `K-R1` 把人群扩到第二个头名）：那一行**有自己的 key**
  （或它的 `auth_style` 声明了「一个鉴权头都不发」）时，客户端自带的 `Authorization` 与 `x-api-key`
  **都被丢掉**，换上这一行自己那一个（写哪个头由该行 `auth_style` 定）。
  ⚠ **`Proxy-Authorization` 仍然照旧转发** —— 它说的是与代理之间的鉴权，收掉它是另一件事，
  **登记为射程外**。那一行**没有** key 时（订阅登录那一档）一个字节都不动。
- ⚠ 本刀的 tee 行**不带 `t_ns`**，也**不设上游超时** —— 两处都受后端零定时器护栏所限，
  理由与代价见 `src/backend/relay/mod.rs` 头注。

**`K-P6b` 追加一条**：`--dial` —— 起 **SSH 拨号代理**（候选 E 的字节代理）。它与 `--relay` 同族：
不是一次性查询，而是一个**常驻**进程，起来就搬字节直到某一头断开。

🔴 **先写死它买到了多少，别读大**：它搬走的是 **后端那条长连接流**的那一跳 SSH 握手 ——
界面侧 `connect_session` 的生产调用点共 **7 处 / 3 份**，本条覆盖 **1 处**；
**SFTP · 端口转发 · 跳板 · 其余一次性 exec 与测试连接，界面进程仍然自己拨号。**
⇒ **任何地方都不许把它写成「拨号搬出去了」。**

- **配置走环境变量 `CCM_DIAL_REQUEST`，argv 与 stdin 都不走** —— argv 在同机任何用户的
  `ps` 里都看得见，而这一份 JSON 里有主机名、用户名、私钥**路径**；
  `/proc/<pid>/environ` 只有本人（与 root）读得到。

  🔴 **订正 2026-09-10：这一格是文档漂移，不是同族的「前提翻了」。**

  **墓碑（原话逐字，2026-09-10 之前）**：

  > - **配置从 stdin 第一行进，不走 argv** —— argv 在同机任何用户的 `ps` 里都看得见，
  >   而这一行里有主机名、用户名、私钥**路径**。
  >
  > ```text
  > 界面 → 代理  stdin  第一行 JSON：{"host","port","user","key_path","host_key_fingerprint","command"}
  > ```

  它记的是**第一版**的形状。代码早就改掉了，这份文档没跟着走。

  **证据（现打的读数，不是从代码推的）**：09-10 在本树拿 `--dial` 实跑两趟 ——
  ① 不设 `CCM_DIAL_REQUEST`、stdin 给 `/dev/null` ⇒ **退出码 2**，stderr 逐字
  `dial: 环境变量 CCM_DIAL_REQUEST 没设（或是空的）—— 界面没交请求`；
  ② **把那份 JSON 原样喂进 stdin 第一行**、仍不设那个环境变量 ⇒ **还是退出码 2、还是同一句话**
  ⇒ 「stdin 第一行」那条路今天**一个字节都不被读**。
  ⚠ 两趟都是 **Linux gnu debug 构建**，**不是** Windows local_backend；stdout 两趟都空
  （这一档连 `DialAck` 都不发 —— 界面还没交请求，没什么可回的）。

  两侧代码各自自陈：`src/backend/dial/mod.rs` 头注写着**为什么**改
  （`ssh_source` 有一条判据**逐字禁止它自己往流里写** —— 写的能力在 `U8a-2a` 整个交给了
  `ParkedWriter`；硬走 stdin 就得去放宽那条判据，代价不值），
  `src/bridge/src/ssh_source.rs` 那一侧是 `.env(DIAL_REQUEST_ENV, …)`。
- 线上形状（**不是** §10 上面那套 wire 协议，那份一个字节没动）：

  ```text
  界面 → 代理  环境变量 CCM_DIAL_REQUEST：一份 JSON
                      {"host","port","user","key_path","host_key_fingerprint","command"}
              stdin   **全部**是原始字节 → SSH channel（远端后端的 stdin）
  代理 → 界面  stdout 第一行 JSON：{"ok":bool,"error":string|null,"fingerprint":string|null}
                      其后：原始字节 ← SSH channel（远端后端的 stdout）
  ```

  ⚠ **键名是蛇形**（`key_path`，不是配置面那套 camelCase 的 `keyPath`）：
  这条管子两端都是我们自己，不该被前端的字段名契约拴住。
- 退出码：`2` = **拿不到请求**（`CCM_DIAL_REQUEST` 没设 / 是空的 / 不是一份合法 `DialRequest`）；
  `3` = 请求读得懂但拨不通（TCP / 指纹 / 鉴权 / exec 任一步）。
  〔订正 2026-09-10：原话逐字「`2` = **请求行读不懂**」——「行」记的是 stdin 那一版，
  同一次漂移。上面那两趟实跑的退出码都是 `2`。〕
- **鉴权只支持 `key_path`（publickey）**：`ssh-agent` 那条界面侧只在 Windows 有实现（命名管道），
  而本 crate 今天只出 Linux musl 二进制。缺 `key_path` 直接 `{"ok":false}`，**不静默回落**。
- **不做多地址竞速（F45）、不做跳板（F56）**：那两样是界面侧 `connect_session` 的上游逻辑。
- **host key 指纹**语义与界面侧逐条对齐：给了期望值就严格比（`trim` 之后），没给就 TOFU 接受 + `warn`。
- ⚠ **`D3③`：它是界面起的子进程 —— 界面一关，`stdin` EOF，它跟着走。**
  别读成「搬出去之后它独立跑着」。
- 🔴 **「默认装机走不走这条路」——本节 2026-09-10 起判不了，别当有答案。**

  **墓碑（原话逐字，2026-09-10 之前）**：

  > ⚠ **默认装机今天走不到这条路**：界面侧只从环境变量与 exe 旁的本机后端解析代理二进制，
  > 而安装包今天**没有 `externalBin`**（F05b）⇒ 两处都空 ⇒ 界面**回落到进程内拨号**。

  **前提翻了（这一格有读数）**：09-10 干净 win11 虚拟机上现打（PM，真安装包 + 真裸 exe 各一趟）——
  装出来那份 `C:\Program Files\cc-monitor\` 下 `cc-monitor-backend.exe` **2 个进程在跑**、
  裸 `monitor.exe` 那份 **0 个**。F05b 已随 v3.7.0 发出去：`externalBin` 住
  `src/bridge/tauri.sidecar.conf.json`，发版那一步用 `--config` 注入（**刻意不进基础
  `tauri.conf.json`**，进了 `cargo test` 也要一份当前 target 的二进制）。
  ⇒ 「安装包没有 `externalBin`」与「exe 旁那处也空」**两句都不成立**；
  ★ 这也说明**「基础配置里没有」≠「没配」**——照 `tauri.conf.json` grep 得到的是只在开发树为真的答案。

  🔴 **但结论不跟着翻 —— 那一格没测，本节不替它下结论。** 前提假不蕴含结论假：
  代理二进制解析得到（回落①不触发）之后，**还有回落②**——配置里没填 `keyPath` 的那台走 ssh-agent，
  而代理只会 publickey，仍旧留在进程内（`ssh_source.rs::connect_and_exec` 是 `proxy && has_key` 两个条件）。
  ⇒ **「默认装机走哪条」根本不是一个常数**，它按**每一行远端配置**分叉；
  而「装完之后配了 `keyPath` 的那台到底走没走代理」，**今天一台机器上都没人跑出过读数**。

  **要什么才测得了**（两个读数就够，缺一不可）：

  1. 干净 win11 + **真安装包**装一次，配一行**填了 `keyPath`** 的远端，开一条后端流；
  2. 读两样：① monitor 日志里那条 `K-P6b: 不走拨号代理（有二进制=… · 配了 keyPath=…）` warn
     **出没出现**（它就是「这台机器上拨号仍在界面进程里」的运行期证据，出现即回落）；
     ② 流开着的时候 monitor 底下**有没有**一个 `cc-monitor-backend.exe --dial` 子进程。
     两样一致才算数；再各跑一趟**不填 `keyPath`** 的对照，把回落②那一支也钉住。

  ⚠ **今天没有任何自动化会碰到这一格**：`CCM_DIAL_PROXY` 在 `tests/e2e/` 与 `scripts/` 下**零命中**，
  ⇒ 走代理那一支**没有 e2e**，门禁全绿证不出它在真机上通。

  ★ 已经有读数的只有**代理二进制自己**那一环（2026-09-10 现打，**Linux gnu debug 构建**，
  **不是** Windows local_backend）：`--dial` 打本机 sshd，`DialAck` 回 `{"ok":true,…,"fingerprint":"SHA256:…"}`，
  远端 `echo` 的字节原样从 stdout 回来；指纹给错则 `{"ok":false,"error":"…Unknown server key"}` 且退出码 `3`。
  ⇒ **代理拨得通**这一环成立；**界面在默认装机上选哪一支**仍然是上面那个没测的格子。

**〔C2 · 2026-09-24，`调研/设计/05-通信层.md §13`〕`--dial` 从「搬走 1 处」变成「SSH 的全部活」。**
上面 `K-P6b` 那一段是它当时的样子，**原文不改**；下面几句是今天的样子，与上面冲突时以这里为准：

- 🔴 **拨号搬出去了**（上面那句「任何地方都不许把它写成拨号搬出去了」到此作废）：界面进程里除 SFTP 外的
  **全部**拨号 —— 后端长连接流 · 一次性 exec · 收全 exec · 测试连接 · 端口转发 —— 都经这个代理；
  界面侧拿链路的唯一入口是宿主 `dial_host.rs`，读应答的是通信层成员 `ssh_link.rs`。
  **唯一的例外是 SFTP**（`sftp.rs`，第三波 `F7c` 独占）：仍用 `inproc_dial.rs` 那一份进程内拨号，
  理由（池预算 · 红线 I7）写在 `设计/05 §13.5`。
  〔SR1b · 2026-09-24 订正〕**这个例外没了**：用户 V89 把 SFTP 放进本机常驻后端（`dial/sftp.rs`，与其它 SSH 同一条连接），
  `inproc_dial.rs` 整份删了，界面进程**零 SSH**（`russh` / `russh-sftp` 出了 monitor 的清单）。传输走入方向「传输四条」，
  部署走链路 `use:"files"`（见「链路」那一节）；远端写只许 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`。
- **两条回落都删了**（`D11`「后端是给定的，不要退路」）：拿不到代理二进制 ⇒ **报**；没配 `keyPath` ⇒ 代理自己走 ssh-agent。
- 请求只**加**了可选字段（v1 那六个一个没改）：`endpoints`（竞速顺序）· `jump`（跳板那一台）·
  `use`（`stream` 缺省 · `capture` · `forward`）· `capture{max_bytes,abort_marker}` · `forward{local_port,remote_host,remote_port}` ·
  `stages` · `probe`。**SFTP 子系统不在 `use` 里**（后端的远端写那一层把它判作远端写能力）。
  〔SR1b 订正〕`use` 多了 `files`（受限远端文件一问一答，写只许两处）；原始 SFTP 字节（`use:"subsystem"`）照旧不开。
- 应答：`stages=true` 时 ack 之前先有若干行 `{"stage":{"kind":…}}`（与 `ConnectStage` 同形）；ack 多了
  `endpoint`（竞速胜者）· `v`（`2`）· `uses`（认得的用法）—— **界面据 `uses` 认出老代理并出声**，不去解它的字节；
  `capture` 在 ack 之后回一行 `{"stdout","stderr","exit_status"}`；`forward` 每接一条连接回一行 `{"accepted":n}`，stdin EOF 即收工。
- 鉴权：`key_path` 或 **ssh-agent**（Unix `SSH_AUTH_SOCK` —— 新能力；Windows OpenSSH 命名管道 —— 零真机读数）。
  竞速**同时起拨**、不错开（后端零定时器护栏禁 `sleep`/`timeout`）；握手看门狗改在界面侧等 ack 时执行（45 s）。
- 读数：`tests/evidence/C2-dial-loopback.py` 对真回环 sshd 八项（竞速 · 严格指纹 · 流与收工 · 跳板 · agent · 转发）。
- ⚠ 后端行为变了（多了用法与字段），**子命令集不变** ⇒ 合并时 bump `BUILD_ID`，否则开发树里按旧 id 释放出来的
  老代理会被界面判成「本机后端太旧」。

**〔SR1a · 2026-09-24〕`--dial` 这条子命令删了。** 上面 `K-P6b` 与 C2 两段是它当时的样子，**原文不改**；今天以这里为准：
拨号挪进**本机那一个常驻后端**，经流上的链路做（本节「入方向」里的「链路四条」）—— 不再每条链路起一个进程，
`CCM_DIAL_REQUEST` / `CCM_DIAL_PROXY` 两个环境变量随之退场。请求的形状一个字没改（从环境变量搬进 `link-open` 的 `dial` 字段，
多一个可选的 `agent_sock`），链路上的应答与 C2 代理的 stdout 逐字节同形。常驻后端不在 ⇒ monitor **报**，不起代理进程、不进程内拨（`D11`）。
⚠ 子命令集少了一条 ⇒ `build_id_guard` 红，合并时 bump `BUILD_ID`。

⚠ 加一条 CLI 命令要动**两处**：`inbound::REGISTRY`（实现与分派臂）+ `main::SUBCOMMANDS`
（`is_query_mode` 的闸门）。只动前者的后果是**静默的** —— 后端把它当未知 flag、
打一行 warn 之后照常进流模式，调用方拿到一堆 jsonl 行。08-13 实测撞到过，
现由 `cli_control::tests::every_cli_exposed_command_is_in_the_query_mode_gate` 钉住。

### 10.1 ★ `--resolve` 的返回值里**哪些是探测出来的、哪些是派生的**（E71）

`--resolve` 读 stdin 的 `ResumeSpec`、往 stdout 写一个 `CommandPlan`。**这三个字段的可信度不一样**，
而字段名读起来一模一样 —— 消费方（已经有一个了）很容易把派生值当事实用。
〔`V126`〕整份线上形状（入参 / 出参 / 错误码）是给 aterm 的**跨仓承诺**，全集与「改了要同轮做哪三件事」住 `§10`「`resolve`」一节的「跨仓承诺」小节：

| 字段 | 它到底是什么 | 能不能当事实用 |
|---|---|---|
| `command` | 调用方给的候选启动器 + `--resume <调用方给的 sid>` | **能**。这是唯一真正可信的一项 |
| `sessionName` | **纯从 sid 派生**（`cc-<sid8>` / `cx-<sid8>`），`session_name_for(sid, is_codex)` | **不能**。它**没读过任何 pidfile、没查过 tmux** —— 据它去 attach 一个**并不存在**的 tmux 会话是现实风险 |
| `capabilities` | tmux/pty 的**典型档**（硬编码的常见组合），文件头自陈「待后续 backend 探测细化」 | **不能**。它描述的是「这类后端通常支持什么」，不是「这台机器此刻支持什么」 |

**为什么记在这里**：`resolve_query.rs` 的文件头写了这件事，但**跨项目的消费方不会读我们的 Rust 源码**。
实现上也留着痕迹：`run(_agent_home, …)` 的参数带下划线、入参字段 `ResumeSpec.claude_dir`
（线上是 `claudeDir`）标着 `#[allow(dead_code)] // MVP 未用（不做 pidfile 消解）` ——
也就是说它**手上有 home 目录却没用**。
⚠ 后端仓内的那个参数 08-14 起叫 `agent_home`（`backend-split` 的 `S4b`：通用层的标识符里
不许出现 agent 名字）；**`ResumeSpec` 的 `claudeDir` 字段名不受影响、原地冻结** ——
它是与 aterm 冻结在 2026-07-18 的 stdin 契约的一部分。

**要判断某个 tmux 会话是否真的存在**，用 `tmux_sessions` 帧（那是真 `tmux ls`）或自己在远端跑一次，
别拿 `sessionName` 当答案。

### 10.2 ~~代码全景本机后端的按需拉取~~ —— 🔴 **整节作废（条 67 · 2026-09-18）**

**这条交付路径不做了，代码已删。**

用户逐字「**不在现在设计里的全部删掉**」。原文这一节描述的是：后端第一次收到全景请求时
**按需从网上拉一个外部二进制**、按哈希钉校验、落到 `~/.cc-monitor/bin/`。
实现住 `src/backend/sidecars/codepicture/`（**2 008 行**），今天已整棵删除，
连同它在后端那侧的四格机器判据与那 11 个 `local_backend_*` 失败码。

**为什么删**（`设计/15 §4.6` 逐字）：同一个能力有**两份完全独立的实现**，
而只有一份接上了 ——

| | 活的那份 | 删掉的这份 |
|---|---|---|
| 怎么拿到能力 | **vendored crate 进程内 link**（不走 MCP、不下载） | HTTP 按需拉一个外部二进制 |
| 命令面 | **21 条** | 0 |
| 生产调用方 | **有**（14 条在用 / 7 条死） | **0** |

⇒ 2 008 行是为一个**从没拍板的交付方式**写的，而实际发出去的那条走完全不同的路。
`15 §4.6` 当时逐字判定「**它的处置取决于产品裁定，不是工程问题**」——本条就是那个裁定。

⚠ **`hello.unavailable[]` 这个机制本身没废**（§10 那一节仍然有效），
废的只是往它上面挂 `local_backend_*` 那一族 `code`。

⚠ **删代码不许连道理一起删** —— 这条路留下过两条今天仍然成立的教训，
已就地记在它们该在的地方，别再重新发现一次：
- **一个上限罩着两个量，超限那一刻分不出被截的是谁**（头与体共用上限 ⇒ 头把体的额度吃掉
  158 字节，剩下 42 字节被当成一份完整的资产 ⇒ 静默截断）。墓碑在 `byte_cap_registry.rs`。
- **光在闭集里加一行是「申报」，申报会在那一层被掏空之后照样绿着**
  ⇒ 「app 自带某个二进制」这类条目，右边必须去钉真源码。墓碑在 `cross_half_edge_registry.rs`。

### 10.3 会话骨架索引：`--read-session-from-offset` 的 `--index` / `--until` 两个选项（`设计/10` 骨架，2026-09-24）

**是什么**：同一条「从偏移读」多两种出法 —— 不带选项时**字节一个不变**（原样透传 `[offset, EOF]`）。

| 调用 | 出什么 |
|---|---|
| `--read-session-from-offset --until <end> <p> <offset>` | 原样透传 **`[offset, end)`**（半开；`end ≤ offset` ⇒ 空）。两端取自索引里的行边界 ⇒ 切出来恰好是整行。**不替调用方对齐行边界** |
| `--read-session-from-offset --index [--until <end>] <p> <offset>` | **骨架索引**（不含正文），逐行 JSON，见下 |

选项在位置参数前后都认；**客户端应当写在前面**（理由见本节末「老后端」那段）。

**索引三段**：
1. 头 `{"kind":"session_index","v":1,"from":<offset>}` —— **首行就认得出对面会出索引**；
2. 每个**可计行**一行（口径 = `line_counts`，与 watcher / `--read-session-tail` 的行号空间一字一致 ⇒
   第 k 行就是 seq `base+k`，`base` = `offset` 之前的可计行数，**由调用方持有**，本命令不回）：
   `{"o":<行起点绝对字节>,"n":<行字节长含\n>,"t":<type>,"u":<uuid>,"sc":true,"mt":true,"ch":…,"cj":…,"pl":…,"cb":…,"cl":…,"fd":…}`
   —— `sc`=isSidechain · `mt`=isMeta · `ch` 正文字符数（代码块外）· `cj` 其中 CJK（`> U+2E80`）·
   `pl` 正文非空硬行 · `cb` 围栏代码块数 · `cl` 代码行数 · `fd` 折叠单元数（tool_use / tool_result / thinking / image）。
   **零值与假值不序列化**；解析不出的行**仍占一行**（只有 `o`/`n`），丢了它后面的 seq 全错一位；
   〔SE2〕这一行是一条**用户输入**（口径 = §10.4 那四条，判定同一个函数 `user_inputs::user_input_of`）⇒ 多两个键：
   `x` = 摘要（同 §10.4 的 `excerpt`）· `ts` = `timestamp`（空 ⇒ 省略）；uuid 就是本行的 `u`。
   ⇒ 首屏「索引」与「大纲清单」合成一趟读：客户端见到**至少一个 `x`** ⇒ 对面是会出它的后端、每条用户输入都带着 ⇒
   `[from,end)` 的清单就在这里、不必再发 §10.4；**一个 `x` 都没有 ⇒ 分不清**（老后端 / 真的零条）⇒ 照旧发 §10.4。
   头 `v` 不变（只加可选键，老客户端忽略）；
3. 尾 `{"kind":"session_index_end","count":N,"end":E}` —— `E` = 最后一个**完整行**的末字节（torn 残尾不计）＝
   **下一次续传该带的 `offset`**。**没有尾行 ⇒ 输出被截断**，调用方不许把前面那些行当全量。

`--until` 与 `--index` 同用：只收**起点** `< end` 的行（起点在界内的那一行整行收）。

**这些是「宽度无关料」，不是高度**：高度依赖列宽，后端不知道列宽（`设计/10 §2.5b`）。前端拿它做第一级粗估。

🔴 **为什么是选项不是新子命令**：新子命令会进 `build_id_guard` 的指纹、逼出 `BUILD_ID` bump；那一拍本轮不许做。
⇒ **在已部署的老后端上这两个选项是休眠的**，而且老后端只读 `args[1]`（路径）/`args[2]`（offset）：
- 选项写在**后面** ⇒ 老后端照旧把**整份会话**透传回来（现打：基线 `3662e17` 的 release 后端，50 955 695 字节的会话
  ⇒ stdout 50 955 695 字节、退出 0）—— 弱网上几十 MB，只为让客户端看一眼首行认出「它不会」；
- 选项写在**前面** ⇒ 老后端拿路径当 offset 解析（`--index` 形）或拿 `--until` 当路径（`--until` 形）⇒ **stdout 0 字节、退出 2**（同一份会话现打）。
⇒ **客户端一律把选项写在前面**（monitor 侧 `session_skeleton::range_argv`，有判据钉着；〔C4b〕索引那一形 monitor 不再经 argv 发，界面经通道说帧命令 `history-index`）。
无论哪种失败，客户端都认「**首行不是 `session_index` 头**」⇒ 判「对面不会出索引」并**诚实降级**（不许把 jsonl 行当索引行解析）；
万一拿到超出 `end` 的正文（老后端不认 `--until`），客户端按索引给的行数自己截掉。
新后端上写错的 `--选项` 与多余的位置参数都**报错退出 2**，不静默忽略。
⚠ 要让远端真的用上它，得等下一次 bump `BUILD_ID`（判 stale → 重装）。路径守卫与 `--read-session` 同一套。

### 10.4 「你说过的话」清单：`--list-user-inputs`（SE1 · `设计/10 §2.2b ⑥`，2026-09-24）

**是什么**：大纲（原名「我说过的 N 句」）的数据源。**判定只有后端这一个住址**（`observe/user_inputs.rs`）——
前端从前在 `onLine` 旁路里一条一条攒（到达序 ≠ 对话序、monitor 起得晚就不全），查看器那边再用同一份 TS 判定扫全量；
两份都删了，两个宿主都来问这条。

| 调用 | 出什么 |
|---|---|
| `--list-user-inputs [--from <offset>] <p>` | 从字节 `offset`（缺省 0）起的清单，逐行 JSON，见下 |

选项在位置参数前后都认；**客户端写在前面**（与 §10.3 同一条纪律；〔C4b〕monitor 不再经 argv 发它 —— 界面经通道说帧命令 `history-user-inputs`）。

**口径**（四条同时满足才算一条）：`type == "user"` · `isMeta != true` · `isSidechain != true`（子 agent 的 prompt 不算 —— 选出来的，不是漏的）·
`message.content` 抽出的**纯文本**（字符串本身，或 `type:"text"` 块用 `\n` 拼）trim 后非空（工具结果回灌靠这条排除）。**没有 uuid 的不要。**
⚠ 已知不等价：渲染那边还会再剥一层 `stripInternalNoise`，剥空了不建卡 ⇒ 清单可能多出极少数「没有卡」的项；前端跳空时标出来。

**三段**：
1. 头 `{"kind":"user_inputs","v":1,"from":<offset>}` —— 首行就认得出对面会出这份清单；
2. 每条一行 `{"uuid":…,"timestamp":…,"excerpt":…}`，**按文件顺序**（= 对话顺序）。`timestamp` 没有 ⇒ `""`；
   `excerpt` = 正文多空白折成一个空格、截到 80 个 Unicode 标量（超出加 `…`）；
3. 尾 `{"kind":"user_inputs_end","count":N,"end":E}` —— `E` = 最后一个**完整行**的末字节（torn 残尾不计）＝
   **下一次增量该带的 `--from`**。**没有尾行 ⇒ 输出被截断**，调用方不许当全量。

**增量**：`--from <上次的 end>` 只读新写进来的那一截；两段拼起来与一次全量逐条相等（判据：`user_inputs_tests` 在每个行边界切一刀）。
`offset` **超过文件长度 ⇒ 报错退出 2**（`past EOF`：文件被截断或重写过，调用方手上的 `end` 已不指向这份文件）——
不回一份空清单假装「没有新的」；客户端据此从 0 重要一份。⚠ **「被重写但更长」这里认不出来**（同 §10.3 的 seq 空间假设），
客户端按「增量里出现了已有的 uuid」兜一道。

🔴 **它是新子命令**（不是 §10.3 那种选项）⇒ 进 `build_id_guard` 指纹、要 bump `BUILD_ID` 才会让已部署的远端判 stale 重装。
老后端上：路径是裸参数 ⇒ 落进查询分支报 `unknown argument`、**stdout 0 字节、退出 2**；客户端认「首行不是 `user_inputs` 头」⇒ 诚实降级（大纲灰掉、说清原因）。
路径守卫与 `--read-session` 同一套。

### 10.5 会话内查找：`--find-in-session`（SE2 · `设计/10 §6 步 6`，2026-09-24）

**是什么**：实时 tab 里 Ctrl+F 的数据源 —— 在**一份**会话里找一段文字，告诉前端它落在哪几条记录上（uuid），前端按骨架索引跳过去。

| 调用 | 出什么 |
|---|---|
| `--find-in-session [--include-tools] [--limit <n>] --query <q> <p>` | 这份会话里所有命中的记录，逐行 JSON，见下 |

**口径与 §10 的 `--search` 是同一份**：一条记录拿哪两段文本去搜（user 正文先剥 CLI 注入的包装、按 `MAIN_CAP` 截；
`--include-tools` 时再加工具内容、按 `TOOL_CAP` 截）、命中算哪一种（先正文、后工具）、片段怎么切 ——
后端 `observe/search_query.rs::record_text` / `record_hit` ＋ `search-core`，两条子命令调同一对函数。
差别只在「扫哪些文件、给多少条」：只扫 `<p>` 这一份；**按文件序**（= 对话序）；**没有 uuid 的记录不列**（跳不过去）。

- `--query <q>`：**必填**，查询串是这个选项的**值**（不是位置参数）⇒ 以 `--` 起头的查询（`--force`）不会被当成选项。
  大小写不敏感子串；trim 后为空 ⇒ 零条。
- `--limit <n>`：最多列几条，缺省 500、封顶 2000（超出按封顶）；`0` ⇒ 只数不列。
- 选项在位置参数前后都认；**客户端写在前面**（〔C4b〕monitor 不再经 argv 发它 —— 界面经通道说帧命令 `history-find`）。
  未知的 `--选项`、缺 `--query`、位置参数不是恰好一个 ⇒ **报错退出 2**（不静默忽略）。

**三段**：
1. 头 `{"kind":"session_find","v":1}`；
2. 每条命中一行 `{"uuid":…,"kind":"user"|"assistant"|"tool","before":…,"matched":…,"after":…}`（片段同 `--search` 的 `hits[]`），最多 `limit` 条；
3. 尾 `{"kind":"session_find_end","count":N,"total":T}` —— `T` = **全量**命中数（≥ `N`；`T > N` ⇒ 被上限砍过）。**没有尾行 ⇒ 输出被截断**，客户端不许当全量。

只看完整行（torn 残尾不看）。每次都从头扫一遍文件 —— 没有常驻索引。

🔴 **为什么不是给 `--search` 加一个「只搜这一份」的选项**：`--search` 对未知选项**容错忽略** ⇒ 老后端会把整台机器的会话全扫一遍、
按单会话 30 条封顶回来 —— 慢，而且静默少条。新子命令在老后端上是 `unknown argument`、**stdout 0 字节、退出 2**，客户端认「首行不是 `session_find` 头」⇒ 诚实降级（说清原因）。
⇒ **它是新子命令**：进 `build_id_guard` 指纹，要 bump `BUILD_ID` 才会让已部署的远端判 stale 重装。路径守卫与 `--read-session` 同一套。

## 11. 远端终端拉起（ccm-rbind，issue #18）——注册与拉起全链路

本地 `__ccm_bind`（§2/§3，文件 IPC + PowerShell 握手）的**远端对偶**：远端没有共享
文件系统，注册信道改走**终端窗口标题**（OSC 转义经 tmux/ssh 透传到本地），monitor
按标题扫窗口。全部代码：远端 **`shared/ccm`**（部署为 `~/.local/bin/ccm`，字节源是
`sftp.rs` 的 `CCM_CLI_SCRIPT`）—— **不是** `remote-section.ts::CCM_WRAPPER_SNIPPET`，
那个其实是 `src/shared/ccm-aliases.sh`，**36 行、别名只有 `cc`/`cct` 这 2 个**，
无任何 rbind / 标题 / poller 逻辑（`sftp.rs` 的守卫①明令该块不得含实现）+ 本地 `bind.rs::RemoteHwndCache` + `lib.rs::bring_remote_terminal_to_front`。

> ⚠ **上面那句里的「N 行」与那份名单由机器对账**（`KR58D2`，判据住
> `src/bridge/src/sftp.rs::tests::the_protocol_doc_sentence_about_the_alias_block_matches_the_file`）：
> 两样都现算自 `src/shared/ccm-aliases.sh` 自己，**改一半会当场红**。本区最高频的那条病
> 就是「数与名单同句、只改一半」，09-11 现打逮到的活体正是这一句 ——
> 它当时写着「29 行」而文件已经 35 行。
>
> ⚠ **本节其余部分是 `K-R48` 第二拍之后的存量馊话，上面那条对账够不着**：
> `shared/ccm` 这个文件已经删了，`sftp.rs` 的 `CCM_CLI_SCRIPT` 也已经没了
> （那里现在是一块墓碑）。`K-R58` 不动它 —— 写在这里，免得被读成「这一节核过了」。
>
> 〔SR1b · 2026-09-24〕远端那份入口（三行 shim，转给后端的 `ccm` 子命令）今天落在 **`~/.cc-monitor/bin/ccm`**
> （部署后端按钮经本机常驻后端的 `files` 链路放，远端写只许 `~/.cc-monitor/{staging,bin}`）；更早放在 `~/.local/bin/ccm` 的那份不删、照旧能用。

### 注册流程（远端 shell → 本地 HWND 缓存）

> ⚠ **不存在名叫 `__ccm_rbind` 的函数。** 本节此前把它写成入口与对外契约
> （`( __ccm_rbind; exec claude ... )`），全仓**没有任何定义** —— 实现早已搬进
> `shared/ccm`（部署为 `~/.local/bin/ccm`），且 `sftp.rs` 有测试**明令**别名块不得含它。
> 照旧文档写的外部集成方会调一个不存在的函数：bash 下只打一行 command-not-found、
> **rc=0 继续跑** ⇒ 静默不注册、↗ 永远「未绑定窗口」。入口就是 `ccm` 本身。

1. **启动**：用户经 `ccm` 起 claude。`ccm` 最后 `exec` 掉自己变身 claude
   （pidfile `sessions/<PID>.json` 按 claude 自己的 PID 命名，这是支点）。
   ⚠ **`U-NP④`（2026-08-14）**：这一步原文还有一句「`exec` 后 PID 不变，所以 ccm 记的 `$$`
   就是 claude 的 PID；注意是 `$$` 不是 `$BASHPID`，poller 跑在 `( … ) &` 子 shell 里」——
   **那条 poller 已整条删除**（见下面第 3 步），ccm 不再需要认识自己的 PID，那句注意事项也随之作废。
2. **tmux 直通**：`$TMUX` 内先 `tmux set-option set-titles on`，再
   `set-titles-string '#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}'`
   （**当前 session 级、运行时选项**，不写 tmux.conf）——否则 OSC 只落 pane title、
   到不了外层终端窗口标题（Batch7 真机排查的主断链）。
   **不是 `#T`**：`#T` 是 pane 标题，而 **claude 也在往 pane 标题写自己的状态**（转圈 + 在干什么），
   两者抢同一个位置、claude 一忙就把 marker 冲掉。真机实测（2026-07-31）：四个空闲会话
   marker 都在、唯独忙碌那个被冲成「⠐ 理解…」，点 ↗ 必弹「未绑定窗口」。
   改成由 tmux 从 `@ccm_sid` **自己合成**之后 marker 常驻，两条路不再交叉。
   `#{?@ccm_sid,…,#T}` 的 `#T` 只是 sid 尚未回填时的回退，避免产出一个空的 `ccm-rbind-`。
3. **marker 刷新**：**由后端打 `@ccm_sid`，tmux 自己合成标题**〔`U-NP④`，2026-08-14〕。
   ⚠ 本步原文是「后台 poller 每 1s 读 `sessions/<PID>.json` … 或每 20 次循环（≈20s）自愈重打，
   发 `\033]0;ccm-rbind-<sid>\007`」。用户裁定「不要轮询」「ccm 做到必须走 daemon」之后，
   **那条每会话一条、与会话同寿、跑在远端的每秒循环被整条删除，不留轮询退路**。
   今天的链路：
   后端（`control/identity_tag.rs`，触发源是它本来就有的 `sessions/` inotify）
   → 读 `/proc/<pid>/environ` 的 `TMUX_PANE` 定位会话
   → `tmux set-option -t <session_id> @ccm_sid <sid>`
   → **tmux 按第 2 步那条 `set-titles-string` 现算标题**并推给 attach 着的 client
   （`ESC ]0;ccm-rbind-<sid> BEL`）→ 直通外层终端 → 经 ssh 显示层透传 → **本地 WT 窗口标题**。
   ⇒ 「自愈重打」不再需要（标题是 tmux 现算的，不是谁定期喷的）；
   `/clear`、`/branch` 原地换 sid 时 pidfile 被重写 ⇒ inotify modify ⇒ 后端跟着重打。
   契约与 `src/doc/INVARIANTS.md` §30 同源。
   ⚠ **代价（如实登记）**：**不在 tmux 里**跑的 `ccm` 从此**没有** rbind marker ——
   旧 poller 是直接 `printf` OSC 到终端的，而 `@ccm_sid` 是 tmux 会话级 option、
   没有 tmux 就没有地方放身份。ccm 会为此往 stderr 说一句，不静默。
4. **扫描绑定**（`lib.rs` remote-session-emitter）：后端 `session_added` 后对该 sid
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
- **PS 侧**（`src/bridge/scripts/cc.ps1.tpl`）反转顺序 ⇒ 首次即中。
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
7. **更新 [`../../src/bridge/README.md`](../../src/bridge/README.md) 模块表 + [INVARIANTS.md](INVARIANTS.md)**
