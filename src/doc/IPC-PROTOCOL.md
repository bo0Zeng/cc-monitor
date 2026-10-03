# 跨进程文件 IPC 协议

cc-monitor 跟外部进程（PowerShell `__ccm_bind` helper、Claude Code CLI）的所有通信都走 `~/.cc-monitor/` 下的 JSON 文件。

本文档定义每个文件的字段、编码约束、写入方原子性语义、读取方反序列化容错策略，以及握手时序图。

> ## ⚠ 读之前：两处会误导你的旧称
>
> 1. 🔴 **`shared/ccm` 这个文件已经不存在了** —— 那 1592 行 bash 启动器删于 `e8f9e08e`
>    （`K-R48` 第二拍④，同拍清零了 19 处 `include_str!`）。今天这条路走的是**后端的 `ccm`**
>    （Rust，`src/backend/control/ccm/`），别名文本在 `src/shared/ccm-aliases.sh`。
>    ⇒ **下文凡是提到 `shared/ccm` 的地方都是历史**，留着是为了解释「今天为什么长这样」，
>    不是现状。看到它请读成「当年那个 bash 启动器」。
> 2. **目录名**：2026-09-17 重组之后顶层**只有 `src/` 与 `tests/`**。
>    `src-tauri/` → `src/frontend/shell/` · `remote-daemon-proto/` → `src/backend/` ·
>    `doc/` → `src/doc/` · `e2e/`·`evidence/`·`scripts/`·`hooks/` → `tests/` 下。
>    下文若出现旧名，同样按历史读。

不在本文档范围：
- Tauri 内部 IPC（前后端 `invoke` / `emit`）— 见 [`../../src/frontend/shell/README.md`](../../src/frontend/shell/README.md) IPC 清单
- monitor 自己的 user config — `config.json` schema 在 TS 端 [`../config.ts`](../config.ts) 定义

---

## 通用约束

所有 JSON 文件**必须**满足：

1. **UTF-8 无 BOM**。PS 5.1 `Out-File -Encoding utf8` 会写 BOM（前 3 字节 `EF BB BF`），导致 `serde_json::from_str` 失败。源头：PS 端用 `[System.IO.File]::WriteAllText(path, json, [System.Text.UTF8Encoding]::new($false))`。接收端 Rust：`raw.trim_start_matches('\u{feff}')` 兜底剥任何 BOM 再 parse。
2. **原子写**。两种实现：
   - **Rust 端**：写 `<path>.tmp` → `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` 一步替换。`std::fs::rename` 在 Windows 上 dst 存在会失败，必须用 `MoveFileExW`。详 [`platform/fs.rs::atomic_replace`](../../src/frontend/shell/src/config.rs)。
   - **PS 端**：直接 `[System.IO.File]::WriteAllText` 即可，单调用本身原子。

   **作用范围**：本条 `MoveFileExW` 路径**仅适用于** `~/.cc-monitor/` 下 monitor 自己产物（`config.json` / `sid-hwnd-cache.json` / `auto-launch.json` / `history-metadata.json` / `ps-registry/<PID>.json` 等）。**写用户文件**（PowerShell profile 等 monitor data dir 之外的文件）**必须**改走 `ReplaceFileW + backup + 写后校验`——理由是保留 dst 的 ACL/ADS/创建时间 + OneDrive placeholder 风险，详 [INVARIANTS.md § 4](INVARIANTS.md)。两者边界由 INVARIANT § 2（monitor data dir 永远在 `~/.cc-monitor/`）锁定，不会漂移。
3. **路径必须**在 `~/.cc-monitor/` 下。**严禁**任何路径越界（用户主目录、Claude 数据目录等）。
4. **目录不存在时自动创建**（`create_dir_all`）。

---

## 1. `config.json`

monitor 自己的设置（主题 / 字体 / claudeDir override / 诊断）。

**位置**：`~/.cc-monitor/config.json`

**写入方**：monitor 的主窗与设置窗（前端各模块经 IPC `patch_config`；诊断经 `set_diagnostics_config`）。**只有一个写函数** `config.rs::patch_config_at`：
写者只交「改哪几条路径」（`ConfigEdit`：`{op:"set", path, value}` / `{op:"remove", path}`，`path[0]` 是顶层键），
它在一把进程级锁里现读盘、逐条应用、原子替换 ⇒ 谁写的键谁的值留在盘上。盘上那份读不懂 / 最外层不是对象 ⇒ 拒写、一个字节不动；
路径为空 ⇒ 整批拒。整份替换的写口（旧 `save_config`）不存在。 〔散文墓碑〕
**读取方**：起 / 停本机后端时 `config.rs::claude_dir_override`（只取 `claudeDir` 原值，显式交给后端）+ 前端启动时 `load_config` + `logging::init()` 读 `diagnostics` 子对象

**Schema**（schema 收敛在 TS 端 / Rust logging 模块；其他 Rust 代码只读写 `serde_json::Value` 不解释）：

```json
{
  "claudeDir": "C:\\Users\\you\\.claude",   // 可选；用户在设置面板 override
  "theme": {                                  // 可选；前端 theme.ts 定义的 token（数量见下方指针）
    "bg": "#1f1b16",
    "text": "#d6cfc6",
    "font-base": "Inter, ...",
    "font-size-base": 14
    // ... 见 src/frontend/ui/theme.ts TOKENS
  },
  "diagnostics": {                            // 可选；v2.0.0 起；缺省值见 src/frontend/shell/src/logging.rs
    "log_enabled": true,                      // 写 logs/monitor/monitor.YYYY-MM-DD.log；切换需重启
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

**位置**：`~/.cc-monitor/ps-await/<PowerShell_PID>.json`

**写入方**：PowerShell `__ccm_bind` helper（profile 里）。两个时机：**每开一个 PowerShell**（块尾 `__ccm_bind -Background`，认领在后台一个空 runspace 里做，不等、不出声）· **敲 `cc` 时**（前台，最多等 3 秒）。同一时刻只有一份认领在改窗口标题（进程内一把锁）。
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
| `marker` | `string` | 唯一字符串，PS 同时把它写到自己的控制台标题（`[System.Console]::Title`）；monitor 用 EnumWindows 查这个字符串 |
| `proc_start` | `string` | .NET `Process.StartTime.ToFileTime()`，用于二次校验 PID 不被复用 |

**生命周期**：短暂。PS 写完后**轮询**（30ms 步，deadline **3000ms**），退出条件二选一：await 文件被 monitor 删除，**或** `ps-registry/<PID>.json` 落地且 `ps_proc_start` 指纹匹配。monitor 在线时正常几十 ms 内完事；退出时标题若还是 marker 就还原（中途被别人改了就不动），await 还在就自删。敲 `cc` 那一份没绑上且没有 registry 时报"绑定超时"；开 PowerShell 时那一份从不出声。（v2 之前只认"await 被删"一种信号、deadline 是 800ms —— monitor 冷启动来不及。）

**后台那一份什么时候认领**：不定时醒，只在下面那两样系统对象上阻塞等。等到「起来了」⇒ 先看一眼「还活着」：拿得到 ⇒ 那个置位是上一个没正常退出的 monitor 留下的，趁拿着复位、放手，接着等；拿不到 ⇒ monitor 在跑，认领（开 PowerShell 时它已在跑，或它后来才起来）。没认上（中途标题被别人改了 / monitor 没回话 / 标题一直挂着 marker 也没被认出，比如这个标签页当时不在前面）⇒ 等这个 monitor 走（拿到「还活着」）、复位「起来了」，再等下一个 monitor 起来；或者下一次敲 `cc`。登记上就收工。

**「monitor 起来了 / 还活着」**：两样有名字的系统对象（同一登录会话可见；名字住 `shell_quote_core::MONITOR_UP_NAME` · `MONITOR_ALIVE_NAME`，别名块渲染时填进模板）。
- `Local\cc-monitor-alive`：互斥量。monitor 起来时在一个专门的线程（`monitor-up-mark`，一直不退）上占住它，进程一没、系统替它放手（被遗弃）。
- `Local\cc-monitor-up`：手动复位的事件。monitor **先占住互斥量、再置位**；正常退出时复位（崩了复位不了，那一态由 PS 看互斥量认出来，见上）。
- PS 两样都是「有就打开、没有就建」；PS 只在拿到互斥量（= 此刻没有 monitor）的那一下复位事件，下一个 monitor 要先拿到互斥量才置位 ⇒ 清不掉它的。

**握手时序**：见下文 § 跨进程握手时序图。

---

## 3. `ps-registry/<PID>.json`

monitor 通知 PS "绑定成功，HWND = X"，同时是个**持久映射**让 monitor 后续查 (PS_PID → HWND)。

**位置**：`~/.cc-monitor/ps-registry/<PowerShell_PID>.json`

**写入方**：monitor `bind::BindRegistry`
**读取方**：monitor `SidHwndCache::record` 在 session 新建时从 claude_pid 往上沿进程链查这里（走到终端窗口的属主为止；中间可以隔着 ccm / cmd）；远端会话的 ↗（`bind::bring_chain_window`）先按进程链上的进程号查这里（§11）；PS 端 `__ccm_bind` 启动时也读这个看是否已注册（指纹比对）

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

**位置**：`~/.cc-monitor/sid-hwnd-cache.json`

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

**位置**：`~/.cc-monitor/auto-launch.json`

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

**为什么自动写路径**：让 cc-monitor.exe 是 portable（用户可以随意移动），下次启动自动更新最新路径，PS 端不需要硬编码。

---

## 6. `logs/monitor/monitor.YYYY-MM-DD.log`（v2.0.0 起，issue #4）

GUI app 诊断日志（解决 `windows_subsystem = "windows"` 无 stderr 的结构性问题）。

**位置**：`~/.cc-monitor/logs/monitor/monitor.YYYY-MM-DD.log`（本机常驻后端那份在隔壁 `logs/backend/`）

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

**位置**：`~/.cc-monitor/history-metadata.json`

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
- `<claude_dir>` = 那台后端解析的 Claude 目录（`agents/claudecode/paths.rs::resolve_home`；本机设置里的覆盖由起本机后端那一处显式交过去）
- `<session_id>` = Claude session UUID（跟 jsonl 文件名同款）
- `<id>` = 数字字符串（CLI 用 `.highwatermark` 自增）

**附属控制文件（monitor 必须忽略）**：
- `<sid>/.lock` — CLI 写入期间的文件锁，半截 JSON 可能存在
- `<sid>/.highwatermark` — 下一个 id 的计数器，非 task 数据

**写入方**：Claude Code CLI（`TaskCreate` / `TaskUpdate` / `TaskStop` 工具）
**读取方**：那台机器的后端 `observe/tasks_query.rs::session_task_lines`（帧命令 `tasks-list`，本机与远端同一条路）；变更由那台后端自己盯（`tasks_changed{sid}` 帧 ⇒ 通道 `session-tasks`），界面收到重问这一条

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
- 读到半截 JSON（CLI 持 `.lock` 中途）→ 单条跳过，下一次变更再发一帧、界面重问自然修正
- `<claude_dir>/tasks/` 整个不存在（用户从没用过 task tracker）→ `tasks-list` 返空；它后来被建出来时作为 agent 家里的一个事件被听见、挂上

**变更触发**：
- 变更的监视住那台后端：`observe/watcher.rs` 递归盯 `tasks/`，一批事件按 sid 去重发 `tasks_changed{sid}`（monitor 那条 notify 删了）
- 帧里只带 sid，不带清单：界面收到后重问 `tasks-list` 拿整份成品（**不**做 diff；清单的唯一出口仍是那条查询）

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
| `name` | string? | Claude 给会话起的语义名（aka aiTitle）。**已在用**：`session_book.rs::LiveMeta` 透传下游（活会话的成品缓存，经会话流 `live` 那一格到前端）（有测试钉住），`session_added` 帧与 bridge 事件也带它 |
| `kind` | string? | 会话类型（Batch6-F21 起双端消费）：`"interactive"` = 交互会话；`"bg"` = CC 2.1.x daemon 后台任务（`--fork-session`，另带 `jobId`）。**Batch7-F24 起 bg 门是配置门**：`showBgSessions` 开（默认）→ bg 正常算会话（建 Tab 带 ⚙ 标识、行流出；与普通 Tab 平铺，不再挂同 cwd 宿主后）；关 → 回 F21 行为（不建 Tab、行不流出；历史浏览器仍可看）。**缺失 = 旧 CC = 视为交互**（保守放行），本地 `session_map::scan_dir`（读启动时配置）与远端后端 kind 门（`--with-bg` 参数）规则一字一致 |
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
（`src/backend/stream/wire.rs`，最小 `BUILD_ID` = **`p1v-attachable`**），
monitor 记进一张 sid 表，用它 ① 拦掉 `↗` 并给出正确说法 ② 把这些 sid 从 idle-tmux 判定里排除。

**只认真正的布尔**：字符串 `"false"` 之类当没写（⇒ 视为可以）。宁可少一次门控，
也不要把一个拼错的值读成「不可 attach」而把功能吞掉。

### 9.2 `procStart` 参与 PID 复用检测，不只是展示（E72）

`procStart` 不是元信息，它是**判活的第二个判据**：monitor 用 `(pid, procStart)` 这一对来区分
「同一个进程还活着」与「PID 被系统复用给了另一个进程」。缺失时（某些 `/resume` 启动路径不写）
退化为只看进程是否存在 —— 那条退化路径**认不出 PID 复用**。
写 pidfile 的一方若能提供它，就应当提供。

**派生 IPC 事件 `session-activity`**（issue #23 红绿灯）：watcher 每次重扫/心跳后比对，仅对 `status`/`waitingFor` 发生变化（含新出现）的会话 emit `SessionActivityPayload` = `{sessionId, status, waitingFor}`（见 `ui_contract.rs::SessionActivityPayload`；启动快照走 `list_session_activity`〔散文墓碑〕，详 STATE-MATRIX）。

---

## 10. 远端后端 wire 协议（issue #15 / #16，**流式，非文件 IPC**）

唯一的非文件 IPC：SSH 远端模式下，远端 `cc-monitor-backend` 后端经 **SSH stdout** 把远端会话流式传回 monitor。在此集中协议契约；部署见 [REMOTE-PHASE0-DEPLOY.md](REMOTE-PHASE0-DEPLOY.md)。

### 实时流（流模式启动后端）

流模式 flag（monitor 仅对 hello 帧**声明了对应能力（`capabilities`）**的后端传，见 INVARIANTS §26；F66/#58③ 起**取代**旧的「build_id 精确匹配」门控）：

| flag | 起 | 语义 |
|---|---|---|
| （无参数） | Phase 0 | 全量重放所有活跃会话历史 + 尾随（旧 monitor / 未确认后端的兼容路径） |
| `--stream` | | **「我是流模式后端」的显式词**，不对应能力、不改行为。后端二进制就叫 `ccm`（`~/.cc-monitor/bin/ccm`）时零参数是「起会话」⇒ 起流一律写成 `ccm -- --stream …`（打头的 `--` 紧跟查询子命令 / 流模式 flag 才进后端，其余进 ccm：`<交给 claude 的…> -- <ccm 自己的…>`） |
| `--with-bg` | p1e (F24) | 放行 kind:"bg" 后台任务会话（宣告+流行，帧带元信息） |
| `--tail-only` | p1f (F25) | 不重放历史：连接时各文件 seq 计数器初始化为当前完整行数（seq=行号），只尾随新行；历史由 monitor 旁路 `--read-session` 快照拉取 |
| `--with-pid` | | `session_added` 帧附 `pid`（本机 ↗ 按它找父 PowerShell 绑窗口）。只有本机那条流发它（本机后端与 monitor 同一份构建），不对应能力 token；**默认关** ⇒ 没发这条 flag 的客户端收到的 `session_added` 字节与本字段加进来之前**一字不差** |
| `--with-raw` | | `line` 帧附 `raw`（那一行记录的原文，去掉行尾换行）。给自己解析记录的第二个前端；同 `--with-pid` 不对应能力 token（老后端把它当未知旗标忽略、照常起流，客户端按 `line` 上有没有 `raw` 认）；**默认关** ⇒ 没发这条 flag 的客户端收到的 `line` 字节与本字段加进来之前**一字不差**。监听口那一形写进 attach 行的 `flags`（只认本表里的旗标） |

**两个前端吃同一个后端**：桌面端（monitor）与第二个前端（手机端）。第二个前端按字段读下面这几格，缺一格就把整帧当坏帧丢 ⇒ 它们**冻结**：不许改名、删、换类型，只许加新字段。清单（帧 kind → 字段）：`hello` `v` `build_id` `host_arch` `claude_dir` `capabilities` `emits` · `line` `session_id` `path` `seq` `byte_offset` `raw`（`--with-raw`）· `session_added` `sid` `path` `session_kind` `cwd` `name` `lines` `status` `waiting_for` `agent_kind` `liveness_confidence` `attachable` · `session_status` `sid` `status` `waiting_for` `liveness_confidence` · `session_removed` `sid` `cause` · `overflow` `dropped` `lost` `lost_truncated` · `turn_end` `session_id` `uuid` · 入方向请求信封 `id` `cmd` `args`。判据 `wire_tests::the_shapes_the_second_frontend_reads_stay_put`（表在那里，类型逐格对）。
它调的一次性子命令同样冻结（叫法 · 位置参数个数 · 输出里它读的那几格）：`--list-projects`（`dirName` `projectPath` `sessionCount` `lastActivityMs`）· `--list-sessions <项目目录名>`（`sessionId` `aiTitle` `cwd` `jsonlPath` `messageCountApprox` `startedAtMs` `updatedAtMs` `isBg`）· `--read-session <路径>` · `--read-session-tail <路径> <N>` · `--read-session-from-offset <路径> <偏移>` · `--search <查询串>` · `--fork-session <会话 id> <消息 uuid>` · `--resolve`（stdin，另有冻结金样 `resolve-contract.golden.json`）；会话 id 的校验规则（非空 · ≤128 · 只 `[0-9A-Za-z_-]`）同样不许改。判据 `wire_tests::the_subcommands_the_second_frontend_calls_stay_put` · `resolve_query_tests::the_session_id_rule_the_second_frontend_relies_on_stays_put`。长期第二个前端改吃 `message` 成品，`message` 的形状按只增不改演进。

线约束：**每行恰好一个 UTF-8 JSON 对象，`\n` 结尾，对象内无裸 `\n`/`\r`**（`serde_json` 紧凑输出把内部换行转义成 `\n` 两字符）。帧用外部 `kind` tag（snake_case）：

| `kind` | 字段 | 说明 |
|---|---|---|
| `hello` | `v, build_id, host_arch, claude_dir, capabilities, homes?, emits?, commands?, unavailable?, host_env?, uncancellable?`<br>⚠ **本表的列举顺序不是线上字节序**。线上按 `wire.rs` 声明序：`v, build_id, host_arch, claude_dir, homes, capabilities, emits, commands, unavailable, host_env, uncancellable`。`dg3_codex_fields_serialize_when_present` 用**精确字节串**钉住它（aterm 拿来做 fixture 真值）—— 要对字节就以 `wire.rs` 为准。⚠ 那个测试名里的 `codex_fields` 是历史名：`S4` 把它钉的两个字段换成了 `homes`，**名字刻意没改**（本文档与仓外 aterm 都按这个名字引用它当 fixture 真值） | 连接建立时**首帧**发一次（握手）。**三轴正交（§26/§28）**：`v`（proto 版本，只留破坏性变更、F66 **绝不 bump**，不符=不兼容）；`build_id`（**身份**，单源自后端源码/编译期 env，管 staleness/重部署提示，不符=偏旧、经 `remote-health` 提示但不 hard-disconnect）；**`capabilities`（能力 token 集，加法式）——monitor 按声明发流模式 flag**（F66/#58③，`decide_stream_flags`；缺该字段=空集=最保守、不发任何 flag，§27）。**绝不用身份（build_id）匹配代理能力声明**（那正是 2026-07-09 事故根因）。**backend-split `S4`（additive）`homes`**：本机上**各 agent 的 home 目录**表 —— `[{agent_kind, path}]`，如 `[{"agent_kind":"claude","path":"/home/u/.claude"},{"agent_kind":"codex","path":"/home/u/.codex"}]`。它一次替掉了 DG3 那两个字段：`codex_dir`（并列的 `<名>_dir`）与 `kinds`（服务的 agent 集 —— 现在由本表的 `agent_kind` 直接读出，不必并列第二个来源）。**为什么换**：`backend-split` 的 `D3` 逐字裁定「agent 维度只许出现在**值**里（`agent_kind`），不许出现在**字段名**里」——并列 `<名>_dir` 那条路的终点是 hello 里五个并列的目录字段，而客户端要靠 `if/else` 猜哪个有值。⇒ 接第三个 agent 是**多一个元素**，不是多一个字段。**为什么可以直接换而不是 bump**：`codex_dir`/`kinds` **从来没上过线**（后端一直硬写 `None`/空 ⇒ `skip_serializing_if` 省略），换掉对任何已部署的消费方都是零影响。**消费侧口径**：空/缺 = 这台后端没声明任何 home ⇒ **回退 `claude_dir`**（monitor 的 `claude_home_from_hello` 就是这条，`parses_hello_homes_and_falls_back_to_claude_dir` 钉住）；坏项（元素非对象 / 缺 `agent_kind` 或 `path` / 类型不对）**逐项丢掉**，不让整帧变 garbage。**今天恒空**——`main.rs` 硬写 `Vec::new()`（`production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen` 钉住）⇒ **hello 帧对 Claude 的线上字节与 `S4` 之前逐字节相同**。⚠ **`backend-split S5`（08-14）订正了"为什么恒空"**：不再是"发现做不出来"——后端已经**有能力**答出本机看得见哪些 agent（`agents::visible_homes()`，判准 = **该 agent 的 home 目录存在**；`the_backend_can_already_discover_homes_it_just_does_not_send_them` 钉住这半），**恒空是一次刻意的排期决定**：填 = 一次跨仓契约变更（aterm 的 fixture 按精确字节对），⇒ 留成一次**纯发布决策**。真填那天要**同轮**做三件事：① 改 `main.rs` 那一行；② 更新 `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent` 的期望串；③ **bump `BUILD_ID`**（那天线上字节真的变了，已部署的远端得被判 stale 重装）。⚠ 与"会话级发现"（DG1：有哪些会话、活没活）**别混**：那一格仍未接线。**`claude_dir` 为什么留着**：它是 hello 里**今天真在线上、且有仓外消费方**（aterm，契约冻结 2026-07-18）的那个目录字段，改名是破坏性变更 ⇒ 走 additive 迁移，原地冻结；解锁条件（monitor 与 aterm 都改读 `homes`）登记在后端的 `agent_boundary_guard::FROZEN_COMPAT`，monitor 那半已经做完。**backend-08（additive）`emits`**：本后端 **需要消费侧门控的帧 kind 集**（snake_case）——含该 kind → 依赖它；不含 → 回退 β/watchdog。⚠**这里原本写的是「会发射的帧 kind 集」，那与它的值对不上**：`EMITS` 8 项**不含** `hello`/`reply`/`cancelled`，而这三个后端 **确实会发**。按字面读它是错的；按意图读它是「门控用」帧集 —— 握手与应答不需要门控（`hello` 是首帧、必然收到；`reply`/`cancelled` 只在你发过命令之后才来，由 `commands` 那一轴管）。**这三个缺席者今天由 `emits_is_a_subset_of_frame_kinds_with_named_exemptions` 逐个点名钉住**，新增帧种漏进 `EMITS` 会红。⚠ `emits` 与 `capabilities` **正交、别混**：`capabilities` 是**流 flag 的可剥离能力**（受 §26 死循环护栏 + `every_capability_token_is_strippable` 强制每 token 有对应 flag），`emits` 是**纯发射声明、无对应 flag、不受 §26**。**U6b-2（additive）`commands`**：本后端 **接受的入方向命令集**（见下「入方向」小节）。能力协商此前只有出方向那一半（`capabilities` 说「我认识哪些流 flag」）；客户端还得知道**发什么过去有人接**，否则只能试错。**空/缺 = 这个后端不读 stdin**（U6b-1 之前的所有版本），别发命令。**`K-P4`（09-04，additive）`unavailable`**：本后端 **接得下、但在这台机器上做不到**的命令及原因 —— `[{command, code}]`，如 `[{"command":"kill","code":"no_tmux"}]`。**它买的是「事前」那一半**：`commands` 说的是「我**接**这条命令」，不是「我**做得到**这件事」；差额今天只有**调用之后**才知道（Windows 上没有 tmux，握手帧照样宣称接 `kill`/`launch`，前端照样画按钮，点了才收到 `no_tmux`）。用户 09-04 逐字裁「**事前协商是要的**」。**为什么是第四条面而不是塞进前三条**：`capabilities` 受 §26 死循环护栏 + `every_capability_token_is_strippable` 约束（每个 token 必须有一条能被 `split_stream_flags` 剥掉的流 flag，而「做得到 kill」没有），且它的默认方向相反（缺=最小能力集，往下降级安全；本字段缺=**没有把握**，往下降级会让能用的功能消失）；`emits` 的取值空间是**帧 kind**；`commands` 的取值空间是**命令名**，而且把做不到的从里面摘掉是错的（客户端 `accepts()` 会直接拒发，连「点了告诉你为什么」这条兜底路都没了，`COMMANDS`/`REGISTRY` 双向相等那条判据也要被迫按平台分叉）。⇒ 本字段的键是 **(命令名 × 命令级 code)** 这个**对**，前三条面没有一条是这个形状。**消费侧口径三句，缺一句就会读错**：① **空/缺 = 这台后端没有任何「做不到」的把握**，不是「全都做得到」—— 客户端照今天的样子办（照发、点了看 code），旧后端天然落这一格；② 列出来的那条 = 别画那个按钮（或画成灰的，配 `code` 那句人话 —— 那张翻译表今天住界面 `src/frontend/ui/tmux-control.ts`（文案表 `tmuxControl.*`），此前是 monitor 的三个发送端各一份）；③ 🔴 **它是提示，不是闸门**：后端自己**绝不**拿这张表拒命令（读数是握手那一刻的，之后世界会变；真去拒 = 把一份会过期的读数变成一次真停机），客户端硬发照样走真路，成不成由 `no_tmux` 那条老路回答。**判准与真调用同源**：`tmux` 在不在 `PATH` 上（`control/kill.rs` 等三处走 `Command::new("tmux")`，unix 上就是 `execvp` 的 `PATH` 查找）；表本身**从 `inbound::REGISTRY` 的 `codes` 派生**（谁登记了 `no_tmux` 谁就依赖 tmux），不是手写的第二份真相。**「判不出来」不倒向「做不到」**：`PATH` 没设、或**非 unix**（那儿 `CreateProcess` 还看进程自身目录与当前目录，且真装了叫 `tmux.exe`）⇒ **不列进表**，退回今天的行为 —— 把「不知道」压成「做不到」会让能用的功能从界面上消失，而那种消失没有回音。（〔K-P4 下一拍〕Windows 那一档已改成平台默认「确证没有」，不再落「判不出来」。）**已真填**：`main.rs` 填 `unavailable_here()`（`production_hello_fills_unavailable_from_this_machine` 钉住），两维 —— tmux（上文判准）· unix 权限位（非 unix ⇒ `files-chmod` 列 `no_unix_mode`）。有 tmux 的 unix 机器上表为空 ⇒ 字节不变；没 tmux / Windows 上字节变了 ⇒ 该拍 bump `BUILD_ID`。仓外 aterm 只读核过：它的 `parseHello` 按通用 map 解、**不读本字段**、未知字段忽略 ⇒ 不受影响。monitor 读它：进那台机器的 `chan::wire::Offer`（能力事实的一个家），调用侧据此**事前拒**（`inbound_client::CallError::Unavailable`，body 里的 code 与事后回的同一个），界面据此置灰（上面口径③的「客户端硬发」不再成立）。<br>★ **`uncancellable`**：`commands` 里**撤不动**的那几条（阻塞档，`cancel` 命中回 `not_cancellable`），从 `inbound::REGISTRY` 的 `Run::Blocking` 派生（`inbound::uncancellable`）；空表省略，落在最末（`wire_tests::net2_uncancellable_is_additive_and_last`）。客户端本地撤单时据此说「那台停不了它、可能还在跑」、不为它补发撤单；空/缺 = 没把握（旧后端），照旧补发、看回话。<br>★ **〔additive〕`host_env`**：起这个后端的宿主交给它的那几格环境，**原样回显**（`{名: 值}`，名单 `wire::HOST_ECHO_ENVS` 两格：`CCM_RELAY_PORT` · `CCM_DATA_DIR`（只在隔离跑时交）；监听口的钥匙 **永不**在内）。它回答「这个后端住哪个家」：常驻监听口按家算（`relay_route_core::listen_port_for(家)`，与 Claude 目录无关），撞口的仍可能是另一个家的后端 ⇒ monitor 读完 hello 拿这一格与自己要交的那份两向比，不等就拒、出声（`local_backend_host.rs::hello_verdict`），不接一个会把写落进别的家的后端。凭据文件与历史注解都住家里，由那台后端按家推，没有另指它们位置的变量。**一格都没被交 ⇒ 省略** ⇒ 远端 / 被 ssh exec 起的 / aterm 连的那些 hello 线上字节**逐字节不变**（`wire_tests::hx2_production_hello_bytes_do_not_change_when_nothing_was_handed`）。 |
| `line` | `session_id, path, seq, message?, cwd?, byte_offset, rid?, raw?` | tail 到的一行 jsonl 的**成品**（`message` ＝ 它在渲染模型里的样子，缺 ＝ 不进界面、照占号；`cwd` ＝ 这条记录自己的；原文 `raw` 只在客户端发了 `--with-raw` 时带，见上表）（`seq` = per-file 单调，口径同本地 watcher）。**`byte_offset`**：该行**末尾**的字节偏移，语义**逐字节对齐 aterm `LineFramer.endOffset`**——计 CRLF 的 `\r`、含 `\n`、残行不计；resume 到 N ⇒ `tail -c +(N+1)`。给 offset 续拉 / 截断检测用（**`seq` 是 per-stream 序数、不是 resume 键**，别拿它续）。**只 `line` 帧带**——`turn_end` 明确不带（`backend-09` 钉住）。**`rid`**：这一行的对账键（适配层 `RecordFace::response_id` 给，同一次上游应答写出的几行共用；`tap` 帧的归一事件 `start` 带同一个值）；没有 ⇒ 省略。只有主运行的记录走 `line`：子运行的记录进运行簿、经 `session_runs` 与 `history-run` 给出 |
| `session_added` | `sid`, `session_kind?`, `cwd?`, `project_dir?`, `name?`, `path?`, `lines?`, `status?`, `waiting_for?`, `agent_kind?`, `liveness_confidence?`, `attachable?`, `container?`, `pid?` | 远端新会话文件出现（Batch5-F18 起 ssh_source 收到即同步透传前端 `remote-session-added {session_id, origin, kind, cwd, name}` 事件建骨架 Tab，先于该会话的任何行）。Batch7-F24（p1e）：附加 pidfile 元信息——wire 帧字段叫 `session_kind`（避开帧 tag `kind`），bridge 事件 payload 统一叫 `kind`（与本地 `list_active_sessions`〔散文墓碑〕/`session-started` 一致）；**additive 兼容**：None 不序列化（旧行为字节不变）、旧 monitor 忽略未知字段、旧后端缺字段前端视为交互。后端默认不宣告 bg（F21）；monitor 仅对 hello **声明了 `bg` 能力**的后端且 `showBgSessions` 开（默认）时传 `--with-bg`（F66/#58③；旧后端不声明该能力→不传，且它会把未知参数当一次性查询→无 hello，护栏「声明 ⟹ 会剥离该 flag」保成立）。本地对称通道：`session-started` payload 扩为 `{session_id, cwd, kind, name}`——前端无 Tab 则建骨架（中途出现的本地 bg 会话由此获得 ⚙ 标识；树状归属已删，bg 会话与普通 tab 平铺）。**Batch8-F25/26（p1f）**：帧再附 `path`（远端 jsonl 绝对路径）；monitor 见后端声明 `tail-only` 能力后 exec 追加 `--tail-only`（Batch9 起快照换 `--read-session-tail` 尾部优先，见查询表）——后端不再重放历史（连接时把各文件 seq 计数器初始化为当前完整行数 L，之后新行 seq=行号），历史由 monitor 按 path 经**独立连接**跑 `--read-session` 旁路快照拉回（0..L'-1 行号编 seq、并发 ≤2、F19 priority 先拉、完就断、失败重试 1 次后 remote-health 提示）；两路 seq 同处行号空间，重叠区被 (sid,seq) 去重精确吸收。旧后端不声明能力 → 不传 flag → 全量推流（=2.18.0）；session_added 无 path（会话尚无 jsonl）→ 不拉快照，后续行从 tail 全量到达。**DG3（#2D，additive）`agent_kind`**：本会话属哪个 agent——`"codex"`；Claude 会话**省略** ⇒ **缺 = claude**。**DG3 `liveness_confidence`**：判活置信度——`"heuristic"`（Codex 无 pidfile，靠 mtime/proc 启发）；Claude 走 pidfile 权威故**省略** ⇒ **缺 = authoritative**。两者都是「缺字段有确定含义」，消费侧别把缺当未知。⚠ **今天的消费方是仓外 aterm，不是 cc-monitor** —— monitor 的 `ssh_source::parse_frame` 把这两个字段（以及 `emits`）**整个丢掉**（`byte_offset` 例外：monitor 读它，续点记第 `next` 行从哪个字节起，`snapshot_resume` 头注）。缺省值碰巧等于丢弃行为，不等于 monitor 实现了默认值：真发 `agent_kind:"codex"` monitor 一样当 claude。（后端今天也还没产出它们 —— DG1 未接线，`homes`/`agent_kind`/`liveness_confidence` 硬写空/None。）⚠ `S4` 起 **`hello.homes` 是个例外：monitor 真的解析它了**（`claude_home_from_hello`，优先 `homes`、回退 `claude_dir`）——别把它算进"整个丢掉"那一族。<br>★ **〔additive〕`container`：这条活着的会话住在什么容器里** —— `"tmux"` / `"none"`，**判不了就缺席**（缺席 = 不知道，**不是** `none`）。：可恢复性由容器类型决定（在 tmux 里的，claude 退了终端还在 ⇒ 可重连；不在的只能 resume），而活着时这一格此前没人报。判定住 `control::identity_tag::Outcome::container`，喂它的是 `process_session_added` 里打标（`@ccm_sid`）那一次探测的结局 —— **零新进程、零新节拍**：`Tagged` / `AlreadyCurrent` ⇒ `tmux`；环境读得到、`TMUX_PANE` 没设 ⇒ `none`；环境读不到（exec 窗口 / 僵尸 / 非 Linux）· pane id 形状不对 · 默认 socket 上的 tmux 不认那个 pane（私有 `-S`）· sid 形状不对 · tmux 报错 ⇒ 缺席。monitor：`ssh_source::parse_frame` 读它（未知取值当缺席），经 `session_facts` 发前端 `session-container` 事件（本机那条流同一个口）。<br>★ **〔additive〕`pid`：那个 claude 进程的 pid**，只在客户端发了 `--with-pid` 时带（没索要的客户端收到的字节与本字段加进来之前一字不差，仓外 aterm 不受影响）。给谁：本机 monitor —— 本机判活改由本机后端的这组帧来之后（`session_map` 的本机活会话表），本机 ↗ 按 pid 找父 PowerShell 绑窗口（`bind::SidHwndCache::record`）只能从这一格拿 pid。monitor 本机那条流因此恒带 `--with-pid`（`local_backend::LOCAL_STREAM_ARGS`）；monitor 只认装得进 u32 的非负整数。<br>★ **`project_dir`：会话的项目目录**（会话起在哪个目录）。`cwd` 是 pidfile 记的那一格（客户端认「刚起的那条」用）；`project_dir` 由后端读记录开头给出（适配层 `RecordFace.project_dir`：Claude 是第一条带 `cwd` 的记录，Codex 是开头那条 `session_meta` 的 `cwd`；只读开头、有上界），之后 shell 进了子目录也不漂；记录还没写出来 ⇒ 取 pidfile 那一格。monitor 原样放进会话流 `live` 那一格，前端 tab 标题 · 打开工作目录 · 分组只认它。缺席 ⇒ 标题退到 aiTitle / sid，前端不从行里猜。 |
| `session_status` | `sid`, `status?`, `waiting_for?`, `liveness_confidence?` | Batch9-F27（p1g）：会话红绿灯状态变化（后端对 pidfile modify 做 diff，CC 仅状态转换时重写故天然稀疏）。monitor 转发进 `SessionChange.status_changed` → `session-activity` 事件——**远端灯与本地共用前端链路**。宣告帧另带初始 `status`（连接建立灯就对）。旧 monitor 未知 kind 忽略。**DG3 `liveness_confidence`** 同 `session_added`（状态变化时带；Claude 省略 ⇒ 缺 = authoritative） |
| `session_removed` | `sid`, `cause?` | 远端会话文件消失。**S0（additive）`cause`**：`"gone"`（真没了：pidfile 被删 / 进程退出 / 原地翻成非交互 kind）/ `"superseded"`（同一个 pidfile **原地换了 sid**，即 `/branch`、`/clear`）。`gone` 是默认值且**不序列化** ⇒ 缺字段 = `gone`，旧后端 × 新 monitor 行为一字不变。**monitor 收到 `superseded` 必须直接归档、不许再去查 tmux 快照**（那份快照对这个场景恒错——这正是「branch 之后原 tab 永久灰点」的成因）。字面量与 `ssh_source.rs` 的解析处由 `removal_cause_wire_literal_stays_in_sync`〔散文墓碑〕 钉住 |
| `turn_end` | `session_id`, `uuid` | 一轮对话结束（monitor 用它对齐轮次边界） |
| `overflow` | `dropped: u64`, `lost?`, `lost_truncated?` | issue #32：后端发送通道被慢/卡的 SSH 管道反压、丢了 `dropped` 帧的哨兵信号（通道排空到能再容纳时发一次）→ monitor 经 SS-F `remote-health` 事件 toast 提示用户可能丢实时行。<br>★ **`lost`（audit-0805 F03，additive）**：`[{kind, subject?}]` —— 那批丢帧里**不可恢复**的那些的身份。⚠ 下面 `#入方向` 那句「出方向丢一帧**可恢复**（行还在远端 jsonl 里）」**只对内容帧成立**：`line`/`turn_end`/`tmux_sessions` 丢了别处还有，而 `session_added`/`session_removed`/`tmux_session_closed`/`session_status` 是**一次差分的结果、别处不存在** —— 只知道「丢了 N 条」是没法重同步的。⇒ 本字段给那些帧带上 `kind` 与 `subject`（sid / tmux 会话名），客户端据此**精确重取那几个主体**。空集时**不序列化**（旧客户端看到的字节与从前一字不差）。<br>★ **`lost_truncated`（additive，bool）**：身份表**有界**（后端侧 `LOST_IDENTITY_CAP = 64`）。超出的那些**仍计入 `dropped`**，只是不再留身份并置本位 —— **不是静默截断**。置位 = 「这份清单不全，理性做法是整体重取快照」。为 `false` 时不序列化 |
| `reply` | `id`, `ok`, `data?`, `code?`, `message?` | **U6b-1**：**入方向命令的应答**。它刻意**复用出方向的 `kind` tag 空间**（不另开一条流），所以它在本表里有一行 —— 而它的完整语义（信封、`id` 不透明性、超时后登记谁摘、逐命令的 `data` 形状与错误码）住在下面的「入方向」小节。⚠ **本行只是清册登记**，不重复那一节的内容（同一份契约不许两处各写一份）。⚠ 它此前**只活在那一节的示例里、本表没有它的行** —— 仓外 aterm 的 KDoc 里那个错的帧数就是数本表数出来的 |
| `cancelled` | `id` | **U6b-1**：某条入方向命令**被取消了**（`id` = 被取消的那条）。同 `reply`：复用 tag 空间、完整语义在「入方向」小节（含「不可取消」时为什么回 `reply{ok:false,code:"not_cancellable"}` 而不是本帧）。⚠ 同上，此前本表无此行 |
| `accounts_changed` | —（无载荷） | **这台机器上的账号清单变了**（watcher 盯账号 manifest 所在目录，一批文件事件里 manifest 动了几次都只发一帧）。客户端收到就重拉一次账号清单（`accounts-list`）—— 清单本身不在帧里，唯一出口仍是那条查询。manifest 所在目录起步时不在 / 后来被删掉重建 ⇒ 这一路听不见（已知边界，代价只是不推帧）。monitor 收到交给前端：经通道 `subscribe(origin, "accounts-changed")` 那条流里一格 `Frame`（原先是裸 Tauri 事件 `remote-backend-ready`，已退役；句柄 `event_replay.rs`），账号表与 chip 随之重取 |
| `tasks_changed` | `sid` | **这台机器上某个会话的任务清单变了**（watcher 递归盯 `<agent 家>/tasks/`，一批文件事件里同一个 sid 动了几次都只发一帧；`tasks/` 起步不在 ⇒ 它作为 `agent_home` 里的一个事件出现时挂上）。只带 sid：客户端收到就重问一次 `tasks-list` —— 清单本身不在帧里，唯一出口仍是那条查询。本机远端同一个二进制 ⇒ 同形（monitor 自己那份 notify 删了）。monitor 收到交给前端：经通道 `subscribe(origin, "session-tasks")` 那条流里一格 `Frame`（体 `{"sid": …}`）。丢了不可恢复（`overflow.lost` 带身份，subject = sid）|
| `sessions_replayed` | —（无载荷） | **这台机器的活会话清单报完了**：`watch_loop` 的 Phase 1（同步扫 `sessions/`、对每个活 pidfile 发一帧 `session_added`）走完那一刻发**一次**，排在 Phase 1 所有帧之后、Phase 2 任何帧之前（同一个 sink、同一条线程）；`sessions/` 不在也照发（清单是空的，也是说完了）。**为什么要它**：客户端手里有一条「固定」的会话条目而这台还没报过它时，得分清「这台还没说完」（显示**说不清**）与「说完了、里面没有它」（显示**已结束**）—— 那张表的判据，此前线上没有任何东西分得开。丢了不可恢复（`overflow.lost` 带身份、subject 无）：客户端停在「说不清」，不会被说成已结束。monitor 收到发前端 `origin-sessions-listed {origin}`（与 `remote-session-added` 同一条线程、同序）|
| `session_state` | `sid`, `state` | **会话账本的成品**：这条会话离开「活」之后是 `"reconnectable"`（claude 退了、它的 tmux 会话还挂着 `@ccm_sid`）还是 `"ended"`（进程没了、容器也没了，或被 `superseded` 顶替）。**由这台后端自己裁**（`observe/session_ledger.rs`：摘除原因 ＋ 它自己那份 tmux 快照；收割：tmux 会话关了当场、`@ccm_sid` 连续两份不见才落）；紧跟在引起它的 `session_removed` / `tmux_sessions` 之后（同一个 sink）。新连接第一份可观测的 tmux 快照里「挂着 `@ccm_sid`、却不在活会话里」的各发一帧 `reconnectable`，**排在 `sessions_replayed` 之前**（清单压到第一份快照之后才放）。客户端只收成品、不再查 tmux 原文。丢了不可恢复（`overflow.lost` 带身份）|
| `session_file_gone` | `session_id`, `path` | **活会话的记录文件不见了**（被删 / 被改名走了）。文件管理器改得动活会话的 jsonl；观察侧当它是「看的、不是管的」：不崩、**不误判结束**（判活看 pidfile / pid，不看 jsonl），出声一次 —— 每次「在 → 不在」只发一帧；同时丢掉那份文件的游标 ⇒ 同名文件再出现（agent 按路径追加重建）从 0 读、`seq` 照旧往上，之后再不见才再发。丢了不可恢复（`overflow.lost` 带身份 subject = sid）。monitor 收到交给那个会话的内容流一格 `{"file_gone": …}`，该 tab 说「记录文件不见了」 |
| `session_file_reread` | `session_id`, `path`, `why` | **活会话的记录文件被改过了、已从头重读**：`why` = `"truncated"`（比读到过的最长还短）/ `"rewritten"`（没变短，但游标之前那一截被原地改写过：游标旁记着已读前缀的末尾 64 字节，续读前核，对不上即是）。紧排在这一趟重读出来的 `line` 帧**之前**（同一个 sink、同一条线程）；重读的 `seq` 照旧往上（`INVARIANTS §25`），前端按 uuid 去重，本帧只负责出声。⚠ 买不到：长度一字不差的原地改写对得齐、不出声。丢了不可恢复（subject = sid）|
| `link_data` | `link`, `data` | **一条链路的下行字节**（`data` = base64，标准字母表带补位；解码后 ≤ 32 KiB）。只在客户端开了链路（`link-open`）之后才出现；链路上的字节与 C2 拨号代理的 stdout 逐字节同形。**不丢**：走应答那条独立通道。完整语义在「入方向」那一节的「链路四条」 |
| `link_end` | `link`, `error?` | **这条链路不会再有字节了**，后端已忘掉这个 id。`error` 缺席 = 正常收尾；在 = 非正常收尾的人话。拨不通**不**走这里（那是链路字节里那一行失败的 ack） |
| `transfer` | `id`, `got`, `total`, `end?` | **一趟传输此刻的样子**（`transfer-start` 之后才出现）：每一帧是整份快照（`got` / `total` 字节），不是增量 ⇒ 后端按变更合并、堵住时只合并不堆积。带 `end` 的那一帧是这一趟的**最后一帧**：`{"state":"done","bytes","sha256"?}` · `{"state":"failed","why"}` · `{"state":"cancelled"}`（`sha256` 只有上传那一路有：整份本机文件的摘要，窗口提交 `files-commit-upload` 时原样交回当 `expect`）。**不丢**：走应答那条独立通道。完整语义在「入方向」那一节的「传输四条」 |
| `probe` | `ticket`, `cell` | **测试连接那一趟的一格进度**（`remote-probe` 在跑时才出现）：`cell` 恰好一个键 —— `stage`（拨号阶段行）· `reached`（`ssh` / `hello` / `control`）· `end`（结局，最后一格）。**不丢**：走应答那条独立通道。完整语义在「入方向」那一节的 `remote-probe` |
| `tap` | `stream`, `run?`, `resp`, `n`, `ev?`, `end?` | **中转抄出来的一段流里的一件归一事件**（或一段的收尾）。只有**进程里住着中转的那个后端**（常驻后端，本机远端同形）会发。`stream` = 请求自带的会话标识头的值（Claude Code 是 `x-claude-code-session-id`，头名由适配层登记；没带 / 过不了段闸 ⇒ 这段不发）；`resp` = 本进程第几段；`n` = 这一段里第几件，**从 0 连续**（后端归位之后重新编号）⇒ 接收侧看 `n` 连不连得上就知道缺在哪。上游的原始事件**在后端按上游协议面折过**（适配层 `StreamFace`，按协议分），界面只收 `ev`：`{"t":"start","rid":…}` · `{"t":"block","i":…,"kind":"text"\|"thinking"\|"tool"\|"other","tool"?:…}` · `{"t":"text","i":…,"s":…}` · `{"t":"stop","ok":…}`；`end`（`"done"` 上游说完 · `"broken"` 转发以错误收尾或上游那一侧缺了号）与 `ev` 恰有一个。**`run`**：这段归哪个子运行（主运行 ⇒ 省略）。归位在后端：请求自报了运行（适配层登记的头，Claude Code 是 `x-claude-code-agent-id`）⇒ 就是它；这一家登记了那个头而请求没带 ⇒ 就是主运行（当场定）；这一家没登记那个头 ⇒ 这个会话此刻没有在跑的子运行就归主运行，有就先挂起、等哪条记录的 `rid` 对上再按它的归属放出（挂起那段对上之前不上任何活卡）。**可丢**：走后端自己那条有界 tap 通道（256 件、单件原文 ≤ 16 KiB），不挤出方向的内容帧、不回推中转；SSE 只保快，jsonl 保对 |
| `session_runs` | `sid`, `runs`, `ended` | **一个会话的运行表**（主运行之外的子运行），表一变就整份发一次。`runs` 每项 `{run, label?, kind?, tool?, state, last?}`：`run` 子运行标识（适配层 `run_of` 的值域）· `label` / `kind` 派出它的那次调用给的标签与类别 · `tool` 父侧工具调用 id（还没对上 ⇒ 省略）· `state` `running` / `done` / `failed` / `stopped`（被叫停）/ `unknown`（没有任何收场信号、子记录又 15 分钟没再写；到点就判 —— watcher 有在跑的子运行时至多等到最早那个期限，读记录 / 出帧时也判，不轮询）· `last` 最近一件事 `{"t":"say"}` / `{"t":"think"}` / `{"t":"tool","name":…}`。按最近一次动静排，最早动过的在前；每个会话至多列 64 个，挤出的先挑最久没动静的已收场的。`ended` 每项 `{run, tool, state}`：被挤出运行表的已收场子运行（对上了派出它的那次调用的那些，先挤出的在前），派出它们的那几张卡照样标终态；「已收场」不随挤出忘（每会话另记至多 4096 个），被挤出之后再被说到不会重新立成在跑。收场以派出那一方为准（前台：那次调用拿到结果；后台：父记录里关于它的收场通知），子记录自己写出终局也算（API 报错的那条本身就是失败），先到先算；收场只粘同一轮 —— 之后同一个子运行又写出新的一次应答（被续跑：自己写过终局之后的别的应答，或收场通知之后才写的）⇒ 回到 `running`。子记录里说到的派出 / 收场用那份子记录的写入时刻。子运行的记录住哪、哪条记录属于谁、哪条派出了谁 / 说它收场了，全问适配层（`ChildFace` · `run_of` · `child_link`）；只读尾巴的流接上会话时，父记录已有的那一截按 `ChildFace::hint` 预筛、只解析说到子运行的那几行；watcher 走与主记录同一条文件事件管线读它们。丢了不可恢复（`overflow.lost` 带身份，subject = sid），下一次表变了自然补上。monitor 收到交给前端：会话流 `session-lines` 里一格 `{"runs": {session_id, runs, ended}}`（不吃 credit、F5 照最新一份重放） |

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
| 单行上限 **1 MiB** | 超过即整行**不解析**（解析它本身就是被攻击面）+ 回 `line_too_long`。量级同 `--resolve` 的 stdin 上限。应答的 `id` 从这一行**开头至多 4 KiB**（`inbound::ID_SNIFF_BYTES`）里尽力抠：顶层对象、值是字符串的那个 `"id"`；抠不出（不在那一段 / 不是字符串 / 形状不对）才回空串 ⇒ 发这一行的调用方当场收到自己的错，不用熬满预算超时 |
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

**今天有三条命令**：`ping` / `cancel`（骨架验收用）+ **`resolve`**（第一条真业务命令，见下），它们随 `hello` 的 `commands` 字段上线 —— 那是与分派表**同一份源头**（`inbound_structure_guards.rs::the_commands_mirror_matches_the_registry` 钉住：声明了却不接 ⇒ 客户端发过去石沉大海；接了却不声明 ⇒ 客户端不知道能用）。真业务命令从 `--resolve` 吸收开始。

> ⚠ 上面那句原先点的是 `hello_commands_match_the_dispatch_table`〔散文墓碑〕——
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

错误码：`invalid_args`（缺 id，或 `cc-kill` 自己拒了）· `bad_id`（id 的形状过不了 `shell_quote_core::bus_id_ok` —— 空 · `-` 开头 · `[A-Za-z0-9_-]` 以外的字符；**交给 `cc-kill` 之前**就拒，一个进程都不起，`INVARIANTS §47` ①）· `not_installed` · `timed_out` · `failed`。

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
| `bad_id` | 收件人的形状过不了 `shell_quote_core::bus_id_ok`（空 · `-` 开头 · `[A-Za-z0-9_-]` 以外的字符）—— **交给 `cc-send` 之前**就拒（`INVARIANTS §47` ①）；给了的 `from` 同样判（它作 `CC_BUS_ID` 交给 `cc-send`） | 后端入口 |
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

**收件人是否存在（成员资格）归 cc-bus 自己**，后端不维护第二份名单（`registered` 那一格如实回）。
**收件人的形状**不再「交给 cc-send 去拒」：`INVARIANTS §47`「对端会校验不是理由」⇒ 后端入口先判（`bad_id`），
规则住共享 crate `shell_quote_core::bus_id_ok`（全仓唯一一份，monitor 读收件箱也用它）—— 不是第二份白名单，是那一份换了住址。

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
此前这个组合住 monitor；界面改成经通道直接说后端之后（「业务解释只有一个家」）收进这里，
界面只收一份成品。

挑人规则（P4f 08-13 实测出来的）：身份空间答得上 ⇒ **只发 `live == true` 的**（老 `cc-broadcast` 发给名册的
每一行：实测 86 行登记、只有 8 个会话还活着 ⇒ 78 个没人读的收件箱）；全是 `null`（问不到 tmux）⇒ 退回
「发给所有登记的」并回 `liveness_unknown: true`（「问不到」≠「都不在」）；不发给 `from` 自己。

回值三个数**分开说**：`sent`（投出去几条）· `skipped_offline`（因不在线跳过几个）· `failed`（逐个列，
`{id, error, detail}`，`error` 是 `bus-send` 那一套码；键刻意不叫 `code` / `message` —— 那一对是整条失败的错误信封）。⚠ **部分失败不整条回错**：已经投出去一部分之后再失败，整条回错会让
调用方以为一条都没发、再发一遍 ⇒ 一部分人收到两遍。

错误码（整条失败，一条都还没发）：`invalid_args`（`text` 缺 / 空）· `not_installed` · `timed_out` · `failed`（列名单那一步）·
`bad_id`（给的 `from` 形状过不了 `shell_quote_core::bus_id_ok` —— **交给 `cc-send` 之前**就拒，`INVARIANTS §47`）。
名单里的收件人（`cc-list` 的输出，对端来的值）也在交给 `cc-send` 之前逐个过同一个判定：过不了的**不发**、照实列进 `failed`
（`{id, error:"bad_id", detail}`），不整条回错（可能已经投出去几个了）。

#### `bus-state`：总线名单 ＋ spawn 台账**一次回全**（`K-R113`，09-13；09-26 改读 cc-bus 的机器可读形）

```text
→ {"id":"B4","cmd":"bus-state","args":{}}
← {"kind":"reply","id":"B4","ok":true,"data":{
     "agents":[{"id":"proj_cc","target":"proj_cc:0.0","registered_at":"2026-09-26T10:00:00+08:00","unread":2,"live":true,"ccm_sid":"1a2b3c4d"}],
     "spawned":[{"id":"proj_cc","dir":"/home/zbl/proj","spawned_at":"2026-09-26T09:59:58+08:00","task":"跑门禁","live":true}],
     "skipped":1}}
```

**入参：无**（不读 stdin —— 与 `bus-list` 同一条纪律：声明无输入的命令必须秒回）。转调 `cc-list --tsv` ＋ `cc-agents --tsv`
（cc-bus 的机器可读形：首行形状标记、末行坏行数；**后端仍不读 cc-bus 的任何文件**）。

- `agents` —— 名册：`id` · `target`（登记的 pane 地址）· `registered_at`（登记时间，cc-bus 原样）· `unread` · `live` · `ccm_sid`（后两格与 `bus-list` 同一套：对身份空间对账，「登记 ≠ 在线」）。
- `spawned` —— `cc-spawn` 派生过的会话：`id` · `dir` · `spawned_at` · `task`（余下全部，含 TAB）· `live`（三态：`true` / `false` / `null` = 核不了，**不是**「不在」）。
- `skipped` —— 两张表里读不懂的行数（cc-bus 数的 ＋ 本侧 id 形状判定拒掉的，`INVARIANTS §47`）。真空行不算。

★ 两半在**同一条命令**里回：两份名单互相引用（判派生会话活不活要回名册借 pane pid），分两次取是两个时刻。
**要么两半都答，要么明说失败**，不回看上去完整的半份。老 cc-bus（不认 `--tsv`，没有首行标记）⇒ `failed`，话里说「先重新部署 cc-bus」，**不猜着按人读表解**；末行缺 ⇒ `failed`（半份）。

错误码：`not_installed`（找不到 `cc-list` / `cc-agents`，消息里带查过哪些位置）· `timed_out` · `failed`。**只读**：两条被调命令都不写任何文件。

#### `bus-inbox`：只读看一个 agent 收件箱的尾巴（SH1，09-26）

```text
→ {"id":"B6","cmd":"bus-inbox","args":{"id":"proj_cc","lines":200}}
← {"kind":"reply","id":"B6","ok":true,"data":{"messages":[{"from":"peer_cc","ts":"2026-09-26T10:01:00+08:00","text":"……","class":""}],"skipped":0,"truncated":false}}
```

入参：`id`（必给；交给 `cc-log` 之前先过 `bus_id_ok`，不过 ⇒ `bad_id`、一个进程都不起）· `lines`（可缺席，1..=2000，缺省 200）。
转调 `cc-log <id> -n <lines>`：**不推已读位置、不写任何状态**（与 `cc-peek` 同一条零写面；消费性读只走 `cc-peek` / `cc-commit`）。
回值：`messages`（逐行解析，只取 `from` · `ts` · `text` · `class`；`from` 与 `text` 都空的行不算消息）· `skipped`（读不懂的行数）·
`truncated`（回显超过 4 MiB ⇒ 保尾，`true`）。老 cc-bus（没有 `cc-log` 或没有首行标记）⇒ `not_installed` / `failed`，话里说「先重新部署 cc-bus」。

错误码：`invalid_args`（缺 `id` / `lines` 越界 / `cc-log` 拒了参数）· `bad_id` · `not_installed` · `timed_out` · `failed`。

#### `bus-spawn`：派生一个协作 agent（BS1b，09-24）

```text
→ {"id":"B5","cmd":"bus-spawn","args":{"tool":"claude","dir":"/home/zbl/proj","task":"跑门禁","account":"a1"}}
← {"kind":"reply","id":"B5","ok":true,"data":{"spawned":true,"id":"proj_cc-2","said":"已 spawn: proj_cc-2   (目录: /home/zbl/proj  初始任务: 跑门禁)\n…"}}
```

入参：`tool`（起哪一家 agent：缺 / 空 ⇒ 注册表里声明默认的那一家；注册表里没有的名字 ⇒ `invalid_args`，那句话列出认得的几家）· `dir`（工作目录，非空）·
`task`（初始任务，可空）· **`account` 与 `base:true` 恰好给一个** —— 两样都不给 ⇒ `invalid_args`。
⚠ 为什么逼调用方表态：不传的话 `ccm` 落 manifest 的默认号，等于**替用户选了一个他没选过的号**去起一个真
agent、烧真额度。

回值：`spawned`（恒 `true`）· `id`（新会话的总线身份；从 `cc-spawn` 的回显里认，**认不出就是 `null`** ——
那是「起了，但名字没认出来」，**不是**「没起来」）· `said`（`cc-spawn` 的原始回显，给人看）。

★ **本机与远端同一条路**：monitor 对每台机器（含 `<local>`）都走这条原语，不再有「远端拼一条 `cc-spawn …`
shell 串走 SSH、本机拒绝」的分叉。命名避让 / 登记进总线 / spawn 台账全在 `cc-spawn`
（它内部再经 `ccm`），后端**只转调**。发给 `cc-spawn` 的 `--tool` / `--account` / `--base` 是**子进程的**
旗标，不是后端 argv（`protocol_doc_guard::CHILD_PROCESS_FLAGS` 登记 ＋ 两向判据）。

错误码：

| 码 | 什么情况 |
|---|---|
| `invalid_args` | `tool` 不是注册表里的一家 · 缺 `dir`、账号没表态或两样都给；或 `cc-spawn` 自己 rc=2（目录不存在 · ccm 太旧） |
| `bad_id` | 给的 `account` 形状过不了 `shell_quote_core::bus_id_ok` —— **交给 `cc-spawn` 之前**就拒（`INVARIANTS §47` ①） |
| `not_installed` | 找不到 `cc-spawn` |
| `timed_out` | 子进程跑过期限被结束。🔴 **会话可能已经起来了**（`cc-spawn` 是建完会话才回显的）⇒ 先 `bus-state` 看一眼，**别直接重试**：重试会再起一个真 agent |
| `failed` | 其它退出码 / 被信号打断 |

⚠ 期限同 `bus-send`：住在子进程里（`timeout` 前缀，默认 10 秒，`CC_BUS_TIMEOUT_SECS` 可调），后端零定时器。
⚠ **这是写面，而且有代价**：它起一个真 agent 进程。UI 侧必须先让用户确认（与收掉 agent 同一条纪律）。

#### `session-fork`：从某条消息处分叉出一个新会话

```text
→ {"id":"f1","cmd":"session-fork","args":{"sid":"0473c3a0-…","uuid":"9a1b2c3d-…"}}
← {"kind":"reply","id":"f1","ok":true,"data":{"sessionId":"5f0e1d2c-…","jsonlPath":"/home/u/.claude/projects/-home-u-proj/5f0e1d2c-….jsonl"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 源会话 id（只收 sid、不收路径：按 sid 在记录树里找那份文件，`branch_core::find_session_file`） |
| `uuid` | → | 从哪条消息处分叉 |
| `sessionId` / `jsonlPath` | ← | 新会话的 id 与落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖） |

与一次性子命令 `--fork-session <sid> <uuid>`（对 aterm 冻结的 argv 形）是**同一份本体**（`control/fork_write.rs::run_inner`：读 → 适配层的分叉变换（`agents/claudecode/branch.rs`，原共享 crate `branch-core`）→ `O_EXCL` 落盘）。
⚠ 名字刻意不叫 `fork-session`：帧面自动派生的 CLI 面会是 `--fork-session`，与那条冻结的 argv 形撞名；这一条的 CLI 面是 `--session-fork`（stdin 一段 JSON）。
本机与远端分叉都经那台机器常驻后端的长连接说这一条（此前本机每次 exec 一个本机后端、远端经拨号链路 exec `--fork-session`）；
今天是**界面经通道直说**（`src/frontend/ui/session-writes.ts`，monitor 那一跳删了），`sid` / `uuid` 在本入口先过共享的 `session_id_ok`（`INVARIANTS §47` ①）。
**错误码**：`bad_args`（`sid` / `uuid` 缺、不是非空串、或形状不过 `session_id_ok`，一个字节都不读不写）· `fork_failed`（找不到 / 读不了 / 变换拒 / 落点已存在，原因原样带着）。

#### `kill`：杀一个 tmux 会话（F04a，**第一条破坏性入方向命令**）

```text
→ {"id":"K1","cmd":"kill","args":{"name":"1a2b3c4d-cc"}}   // 可带 "client":"<自报的前端>"（见下「哪个前端的会话」）
← {"kind":"reply","id":"K1","ok":true,"data":{"session":"1a2b3c4d-cc","killed":true,"bus":{"removed":[],"failed":[],"unread":null}}}
```

杀成之后**顺手从 cc-bus 收掉登记在这个会话上的 id**：过门之后、杀之前读下这个会话全部 pane 的根进程 pid，
杀成之后经 `cc-list --tsv` 读名册、按第 4 列 pane pid 认人（不按会话名猜），逐个 `cc-kill <id>`（名册 · 台账 · 状态 · 收件箱一起清）。
这一步不改结局：cc-bus 没装就跳过；结局进成品的 `bus` 那一格 —— `removed`（注销掉的 id）· `failed`（`{id, why}`，名册里那一行还在）·
`unread`（名册读不到的原话，读得到 ⇒ `null`），界面照它说一句（全空 ⇒ 不说）。

**它必须过 §34 的三道门**
（⚠ **`K-R72` 2026-09-12**：monitor 侧 `kill_remote_tmux` 那条 shell 路**已经删了**——
F04b 先把它从**主路**降为一次性回落，本件把它整块拿掉 ⇒ **杀会话今天只剩本命令这一条路**。
后端通道不在就是**明确失败**，不再换条路悄悄做掉。那条 Tauri 命令本身也迁到界面了：
界面经通道直接说本命令（`src/frontend/ui/tmux-control.ts::killSession`），用户看得见的那句话住文案表 `tmuxControl.channel.*`
（本机 / 远端两句不同的话）；「能不能证明没发出去」那条分流规则仍见 `src/comms/inward/backend_route.rs` 头注那张表）：

| 门 | 判据 | 不通过的错误码 |
|---|---|---|
| Gate 1 | `=name:` 精确匹配（`exact_target`）；`name` 不许含 `:` / `=` / 控制字符 | `invalid_args` |
| Gate 2 | 名字命中 `cc-*`／`<X>-cc` **或** 远端 `@ccm_sid` 已设；**外加「哪个前端的会话」那一维**（见下） | `wrong_owner` |
| Gate 3 | `session_windows == 1`（**只给破坏性动作**） | `too_many_windows` |

目标不存在 ⇒ `no_such_session`；起不来 tmux ⇒ `no_tmux`；过了门却没杀成 ⇒ `kill_failed`。

**「哪个前端的会话」那一维**（两个前端吃同一个后端；`kill` · `launch` 的 `send-into` · `sessions-*` · `terminal-input` 共用，`control/gate_rules.rs::gate_client`）：
会话由起它的那一方声明 —— tmux 会话选项 `@ccm_client=<名>`（1–32 个 `[a-z0-9-]`）。用户在终端里敲的 ccm / 别名起的写 `ccm`（不归任何一个前端，谁都能动；ccm 容器路经 `launch` 的 `create-or-attach` 写）；
某个前端为自己内部的管道起的写那个前端的名字（第二个前端起会话时自己设）。请求可带 `args.client`（自报是哪个前端；形状同上，不合 ⇒ `invalid_args`）：
没声明 / 声明成 `ccm` ⇒ 这一维不拦、照上表 Gate 2；声明的就是这次自报的 ⇒ 放（名字不必像我们铸的）；声明了别的 ⇒ `wrong_owner`，话是「另一个客户端起的，这里只能看」。
后端打标（`@ccm_sid`）照旧，名单里两边都看得见；这一维只管能不能动。它是 tmux 过渡期的最小口子（防一个前端误动另一个前端的会话，不是安全边界）。

**两件它刻意这么做的事**：

1. **杀的是 `#{session_id}` 句柄，不是名字。** 三道门与 `kill-session` 是两次 tmux 调用，
   之间有窗口；对句柄下手 ⇒ 名字被重新绑定也杀不到别人。**破坏性动作尤其不能对名字下手。**
2. **Gate 3 只给它。** `send-keys` 不删除任何东西，窗口数与它无关 ——
   给 `send-into` 加 Gate 3 会让「往多窗口会话里打字」被误拒。
   所以 `admit`（非破坏性）与 `admit_destructive` 是**两个入口，不是一个带 flag 的**。

⚠ ~~本命令存在 ≠ monitor 已经改走它~~ **F04b 2026-08-04：monitor 已经改走它了**（当年是 monitor 的 `kill_remote_tmux` 主路调它自己那个发送端）。🔴 **`K-R72` 2026-09-12：那条一次性 SSH 已删** —— `C7` 说的过渡到此结束，**盘上没有第二条路**。定框 C6 的顺序（先搬门、再切路由）到 **F04c** 走完。🔴 **monitor 那一跳也拿掉了**：界面经通道直接说本命令（`src/frontend/ui/tmux-control.ts::killSession`），成品 `{session, killed}` 由跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 钉；回潮闸今天钉的是「monitor 生产段里一处 `kill-session` 都没有」＋「界面只经那一处说它」（`tmux_backend_gate_guard`）。

#### `sessions-tmux` / `sessions-stop` / `sessions-start`：一批会话一次问（tab 栏的单个菜单与批量菜单）

同一台机器上的那几个**一次**交过来，这台**逐个**答；一个不成不挡下一个。单个菜单就是一个 sid 的一批 —— 两条路是同一条。
「这个 sid 此刻在这台哪个 tmux 会话里」只在这台判一次（带着它的 `@ccm_sid`、前台是不是 agent），三条命令共用；没打上标记的不按目录猜。

```text
→ {"id":"S0","cmd":"sessions-tmux","args":{"sids":["<sid>"]}}
← {"kind":"reply","id":"S0","ok":true,"data":{"results":[{"sid":"<sid>","standing":"running","names":["proj-cc"]}]}}
```

`standing`：`running`（恰好一个在跑的，`names` 是它）· `ambiguous`（在跑的不止一个，`names` 按名单顺序）· `idle`（没有在跑的、有带着它的空 tmux，`names` 是第一个）·
`none`（没有哪个会话带着它）· `no_tmux`（这台没装 tmux）。菜单就绪时问它：亮哪几项、写哪个名字。

```text
→ {"id":"S1","cmd":"sessions-stop","args":{"sids":["<sid>","<sid>"]}}
→ {"id":"S2","cmd":"sessions-start","args":{
     "mode":"tmux" | "window",
     "local":true,                      // 这台是不是 monitor 所在那台（开终端那一形按它选本机 / 远端那一行）
     "items":[{"agent":"claude","sid":"<sid>","cwd":"/x",
               "account":{"kind":"inherit"} | {"kind":"base"} | {"kind":"named","name":"work","configDir":"/h/.cc/work"},
               "model":null,"launcher":"claude","defaultLauncher":"claude"}]}}
← {"kind":"reply","id":"S1","ok":true,"data":{"results":[
     {"sid":"<sid>","outcome":"done" | "skipped" | "failed","why":null,"detail":"",
      "session":"proj-cc","bus":{"removed":[],"failed":[],"unread":null},"cmd":null}]}}
```

`results` 与入参逐个同序。`outcome`：`done` 做成了 · `skipped` 这一个不用做 / 做不了（`why` 说为什么）· `failed` 做了没成（`why` ＋ `detail` 是单个那一条的码与原话）。
`session` = 落在哪个 tmux 会话上；`bus` 只有停那一条有（同 `kill`）；`cmd` 只有开终端那一形有（monitor 拿它开窗）。

- **停**：`running` / `idle` ⇒ 杀那一个，同 `kill`（三道门、杀句柄、顺手注销 cc-bus），门里另核句柄上此刻的 `@ccm_sid` 就是它（认完名字换了人 ⇒ `wrong_owner`）。
  `none` ⇒ `skipped`/`not_in_tmux`；`ambiguous` ⇒ `skipped`/`ambiguous`（`detail` 列名字，不杀）。
- **起**：先问记录还在不在（同 `history-record`，查 `account.configDir` 那棵树）—— 不在 ⇒ `skipped`/`record_gone`（`detail` = 查的那棵树）。
  - `mode:"tmux"`（不接进去）：`running` ⇒ `skipped`/`running`；`ambiguous` ⇒ `skipped`/`ambiguous`（`session` 是第一个）；`idle` ⇒ 同 `launch` 的 `send-into` 键入直路那一行；
    `none` ⇒ 这台铸名（同 `tmux-name-mint`），交**界面「在 tmux 里 Resume」那一行**（远端同 `launch-render-cli`、本机同 `launch-local`，同一份映射、同一个渲染器）只多 `--detach`，
    由这台后端自己当 ccm 跑（环境、中转地址、身份标记、自检都由 ccm 那一趟做）；退出码 3（名字有人了）⇒ `failed`/`name_taken`，别的非零 ⇒ `failed`/`start_failed`（`detail` 是 ccm 的原话）。
  - `mode:"window"`：只渲那一行交回（本机同 `launch-local`：POSIX 上铸名建进 tmux；远端同 `launch-render-cli` 直连），窗口由 monitor 开。渲不出来 ⇒ `failed`/`refused`。
- 这台没装 tmux ⇒ 停与 `mode:"tmux"` 逐个 `skipped`/`no_tmux`。

三条都可带 `client`（自报的前端，同 `kill`）：停与就地键入那一步带它过「哪个前端的会话」那一维（别的前端的 ⇒ 那一个 `failed`/`wrong_owner`）。

命令级码只有两个：`invalid_args`（`sids` / `items` 空、超过 64 个、有重复、sid 形状不对、字段认不出、`client` 形状不对）· `unobservable`（这台的 tmux 名单看不见 —— 不是零会话）。
只给界面用：命令行那一侧没有这两条（逐个 `--kill` / 直接敲 `ccm` 就是它们）。

#### `launch`：平面 ②（远端执行面）——真的建 tmux 会话（U8a-2b）

```text
→ {"id":"L1","cmd":"launch","args":{
     "mode":"create-or-attach" | "send-into",
     "name":"cc-1a2b3c4d",
     "payload":"cd '/x' && claude --resume …",
     "cwd":"/x",             // 可选，仅 create-or-attach
     "ccm_sid":"<完整 sid>",  // 可选，仅 create-or-attach；[A-Za-z0-9_-]
     "agent":"claude",       // 可选，仅 create-or-attach；给了就得是注册表里的一家（空 ⇒ 默认那一家，认不出 ⇒ invalid_args）
     "width":"220",          // 可选，仅 create-or-attach；与 height **同时给或都不给**
     "height":"50",          // 可选；1–4 位十进制**字符串**
     "client":"ccm"          // 可选：自报的前端。send-into 拿它过「哪个前端的会话」那一维（见 `kill`）；create-or-attach 新建成了就写成那个会话的 `@ccm_client`
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
3. **`send-into` 时不新建会话。** 会话不存在 ⇒ 回 `no_such_session`。
   顺手新建就是 #76 的反向：用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell。

##### 🔴 ★★ `create-or-attach` 的**唯一调用方 `ccm` 从此没有退路**（`K-P2` F 拍，2026-09-04）

用户 09-04 逐字裁定：「**ccm不要管找不到, 统一走后端**」。

**这一条是协议的读者要知道的行为变化，不是 ccm 的内部细节** —— 因为 `shared/ccm`
经 `src/frontend/shell/src/sftp.rs` 的 `include_str!` 被**部署到每一台远端机器**上，而这条协议
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

F04c 那个裸键 mode `send-keys-raw`（打断当前回合的 `Escape`、不附尾 `Enter`）已删：换号重启不再发 `Escape`，它没有调用者了。

**`data` 三字段就是失败语义**（能分辨「没起成」与「起了但没确认」）：

| 结局 | `ok` | `code` | `created` / `typed` |
|---|---|---|---|
| 新建 + 键入 | true | — | true / true |
| 会话已存在（幂等短路，**不重复 resume**） | true | — | false / false |
| `send-into` 键入成功 | true | — | false / true |
| tmux 不在 PATH | false | `no_tmux` | 没起成 |
| `send-into` 但会话不存在 | false | `no_such_session` | 没起成 |
| `send-into` 但会话不是本工具的（§34 Gate 2，原先这一行漏了） | false | `wrong_owner` | 没起成 —— 没往别人的会话里打字 |
| 建不出来且也不存在 | false | `create_failed` | 没起成 |
| 会话在，`send-keys` 失败 | false | `typed_unconfirmed` | **起了但没确认** —— 别重试新建 |

⚠⚠ **`typed:true` 只有 `send-keys` 的退出码那么强**〔audit-0805 F10，报告 I-3〕。
pane 处于 **copy-mode**（用户滚了一下轮子）时 `send-keys` **照样退 0**，
而键被 copy-mode 的键表吃掉、载荷根本没进应用。后端侧**没有第二种确认**
（`pane_in_mode` / `-X cancel` 全仓零命中），由
`launch_tests.rs::typed_is_only_as_strong_as_the_send_keys_exit_code` 钉住这个语义边界。
⇒ **消费方别把 `typed:true` 读成「载荷确凿落地」**。补第二种确认要真 tmux 才验得了，
是诚实边界，留给 e2e tier2。
| 形状不合 | false | `invalid_args` | 没起成 |

**错误码分两层**（U8a-2b 定，趁 `launch` 还没有仓外消费方）：
**协议级**由 `inbound.rs` 独占 —— `bad_request`（信封 JSON 坏了）· `line_too_long` ·
`unknown_command` · `duplicate_id` · `handler_panicked` · `not_cancellable`，语义是
「客户端代码写错了，别重试」；另有一个**不是**「写错了」的协议级码 `shutting_down`：后端在收场
（收到停机信号 / 对端走了 / 最后一个客户走了且退出行为是「结束」，先等在飞的阻塞档命令做完再退），这时新来的阻塞档命令
一个字节不动、回它 —— 语义是「这一条没执行，重连之后再发」；它与命令无关，同样只有 `inbound.rs` 判得了。**命令级**由各命令自己定，语义是「参数或环境的问题」。
所以 `launch` 的形状错误叫 `invalid_args` 而**不是** `bad_request`。
（⚠ `resolve` 今天仍回命令级 `bad_request` —— 它与仓外 aterm 的一次性契约冻结在 2026-07-18，
两条路复用同一个纯函数，改它会破坏那份契约。如实登记，不顺手改。）

**后端侧的校验是形状校验，不是安全边界**（同 §「信任边界」那条）：入方向命令来自
已经握着这台机器 SSH 会话的对端，它本来就能在这台机器上跑任意命令。这里只回答
「这组参数能不能构成一次有意义的 tmux 调用」，缺字段/空/超长/含控制字符 ⇒ 结构化错误。

⚠ **F12 2026-08-04 订正**：本段原来断言「这条路的生产切换还没发生、登记为 U8a-2c」——**那句自 U8a-2c-1 起就假了**，而 F07/F11 连着订正了 `INVARIANTS §33b` 里的**三份副本**、**唯独漏了这一份**（是 Phase G 的 `/full-audit` 逮到的）。今天的实况：monitor 生产段 `.call("launch")` **0 处** —— 原来那两处（就地 resume 的 `send-into` = U8a-2c-1 · 送键 = F04c）连同它们的发送端迁到界面，今天由 `src/frontend/ui/tmux-control.ts` 经通道直接说 `launch`；仍未切的是 **`create-or-attach` 与 attach 两格**。⚠ 那个数的唯一家在 `INVARIANTS §33b` 的〔机检〕锚点上，由 `doc_claim_registry` 读它与现场比；本文件从 F12 起也在那条判据的扫描面里 —— 同族副本再写回来就会红。

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
〔：那个调用方已随用量 ③ 轴退役 —— **论证仍然成立，举的例子不在盘上了**。
本条今天的消费者是拉屏预览（界面 `src/frontend/ui/tmux-control.ts::capturePane` 经通道直接问；此前是 monitor 的 `capture_via_backend`，已删）。〕

🔴 **它只抓一次就返回，后端里没有任何「隔 N 毫秒再抓一次」**（零定时器铁律，
`no_timer_guard` 零容忍）。「抓几次 / 隔多久」是**调用方**的事 ——
`K37`〔用@09-11〕逐字「后端只给机制，不给偏好」，而「等画面稳定多久算稳」是偏好。

⚠ **空屏是合法的成功**：刚建起来什么都没打印的 pane 抓回来就是空 `screen` ＋ `ok:true`。
「抓没抓到」看退出码，不看输出是不是空。

⚠ **刻意不过 §34 Gate 2**：这是一次只读快照，与调用方那一侧口径一致（今天是界面 `src/frontend/ui/tmux-control.ts::capturePane`：只拒空目标、不过身份门；
此前 monitor 侧同族那一处的登记逐字：「只读快照，明确不为它加身份门」）。

错误码：`invalid_args`（`name` 缺/空/含 `:` `=`）· `no_tmux`（tmux 这个程序起不来）·
`no_server`（这台机上一个 tmux server 都没有）· `no_such_session`（server 在、目标不存在）·
`capture_failed`（其它失败，**stderr 原样回包** —— 说不清但不撒谎）。

> 〔删用量 —— 这里原有一节 `oneshot-session`（起一个到点自己会死的 tmux 会话）。
> 它是为**用量探针**建的（`K-R87` 的 CLI 面 → `K-R104` 的帧面），而用量 ②③ 两轴整轴退役
> ⇒ 这条命令**零生产调用方**，随之退役：`inbound::REGISTRY` 与 `COMMANDS` **11 → 10**，
> `BUILD_ID` `p2j-bus-state` → `p2k-usage-retired`。
> ⚠ **隔壁那条 `capture-pane` 没退**（拉屏预览真在用它），别把两条读成一刀。〕

#### `terminals-list` / `terminal-preview` / `terminal-input`：终端管理 L1（名单 · 抓一屏 · 送字送键）

两个前端共用，**形状与宿主无关**（这一版宿主是 tmux：一个 tmux 会话 ＝ 一个终端，看它当前窗口的当前窗格；托管终端做出来后换实现、不换形状）。
目标用名单里的不透明句柄 `terminal`（前端不拼、不解析）**或**会话 ID `sid` 指，恰好给一个，否则 `bad_target`；**不收任意 tmux 目标串**——
句柄 / sid 先在这一刻的名单里对上才动手。只抓一次、只送一次，轮询归调用方。CLI 面同名（`ccm -- --terminals-list` 等，stdin 一段 JSON 当 `args`）。金样 `tests/__fixtures__/terminals.golden.json`。

```text
→ {"id":"T1","cmd":"terminals-list","args":{"client":"mobile"}}          // client 可选：自报的前端，决定每行的 mine / can
← {"kind":"reply","id":"T1","ok":true,"data":{"complete":true,"terminals":[{
     "terminal":"tmux-3","host":"tmux","tmux_name":"proj-cc","title":"…","program":"claude","cwd":"/p",
     "session":{"sid":"<sid>","agent":"claude"},          // 里面跑着会话（@ccm_sid）时才有；agent 没标就缺
     "purpose":"normal",
     "started_by":{"client":"ccm","mine":true},          // client ＝ 会话上的 @ccm_client（没声明 ⇒ null）；mine ＝ 这个调用方能不能送字 / 结束
     "clients":[{"kind":"terminal-window","since":1696000000,"last_activity":1696000100}],   // 此刻连着它的 tmux 客户端；空 ＝ 后台
     "input":"shared",                                   // tmux：各端都能打字
     "state":"running" | "idle" | "program-exited",      // 有会话、前台是 shell ⇒ program-exited；没会话、前台是 shell ⇒ idle
     "last_activity":1696000100,                         // 秒
     "can":{"preview":true,"input":true | {"no":"not-yours" | "not-managed"},"end":true | {"no":"not-yours" | "not-managed" | "other-windows"}}}]}}
```

`complete:false` ＝ 名单里有读不懂的行（画「部分」）。这台没装 tmux / 没起 server ⇒ 空名单（不是错）；tmux 在但列不出 ⇒ `unobservable`。

```text
→ {"id":"T2","cmd":"terminal-preview","args":{"terminal":"tmux-3","color":true,"scrollback":0}}   // color 缺省 true；scrollback 缺省 0、上限 2000
← {"kind":"reply","id":"T2","ok":true,"data":{"screen":"<16 位十六进制指纹>","cols":80,"rows":24,
     "cursor":{"x":0,"y":3,"visible":true},
     "lines":[{"text":"…","spans":[{"from":0,"to":4,"fg":"red","bold":true}]}],   // color:false ⇒ 不给 spans
     "scrollback_lines":0,"capped":false,"captured_at":1696000100}}
```

`lines` 自上而下，往回要的那几行在最前（`scrollback_lines` 是实际给了几行）；`capped` ＝ 要的比上限多、截到了上限。
`spans` 的颜色是 16 色名（`red` · `bright-blue` …）或 `#rrggbb`，属性 `bold` · `dim` · `italic` · `underline` · `inverse` 只在为真时出现；`from` 含、`to` 不含，按字符列计。
`screen` ＝ 这一屏的指纹（去掉颜色、行尾空白，只看可见那几行）：内容一变就变，送字送键时带回来（`seen_screen`）。只读、不过身份门（同 `capture-pane`）。
码：`bad_target` · `invalid_args` · `not_known`（名单里没有）· `ambiguous`（这个 sid 落在不止一个终端上，改用 `terminal` 指）· `no_tmux` · `no_server` · `no_such_session`（名单之后没了）· `capture_failed` · `unobservable`。

```text
→ {"id":"T3","cmd":"terminal-input","args":{"terminal":"tmux-3","text":"/usage","enter":true,"seen_screen":"<指纹>","client":"mobile"}}
→ {"id":"T4","cmd":"terminal-input","args":{"sid":"<sid>","key":"esc"}}
← {"kind":"reply","id":"T3","ok":true,"data":{"result":"delivered"}}
← {"kind":"reply","id":"T4","ok":true,"data":{"result":"refused","why":"screen-changed","screen":"<新指纹>"}}
```

送字与送键分开，恰好给一个：`text`（字面字，原样送、不解释成键名；多行按粘贴送，程序开了括号粘贴就带上；`enter` 缺省 true ⇒ 之后补一个回车）·
`key`（有限键表：`esc` · `ctrl-c` · `ctrl-d` · `up` · `down` · `left` · `right` · `tab` · `shift-tab` · `enter` · `backspace` · `page-up` · `page-down`，不收任意转义串）。
数字选项（权限框的 1 / 2 / 3）走「送字 ＋ `enter:false`」。`take` 在 tmux 上无所谓（各端都能打字）。
过身份门（同 `launch` 的 `send-into`，含「哪个前端的会话」那一维，`client` 自报，见 `kill` 一节）。
`result`：`delivered` · `unsure`（tmux 回了非零而会话还在：不知道送没送到，**不重发**）· `refused` ＋ `why`：
`not-known`（名单里没有）· `ambiguous` · `ended`（终端已经没了）· `not-yours`（别的前端起的）· `not-managed`（不归 cc-monitor 管：没声明、名字不像我们铸的、`@ccm_sid` 也没设）·
`screen-changed`（带了 `seen_screen` 而画面已经变了，回话里带新的 `screen`；**不送**）。形状不对才是错：`bad_target` · `invalid_args`；另 `no_tmux` · `no_server` · `no_such_session` · `capture_failed` · `unobservable`。

#### `files-read` 这一族（步 `24f`，2026-09-20；**第五、第六条** 2026-09-21；**第七、第八条** F7a 2026-09-24）——**八条纯读命令，一条都不写盘**

六条一起看，逐条的信封在下面各自的小节里。共同的三条，别读宽：

1. 🔴 **整族只读。** 遍历 · 读文件名 · 存内存 · 答查询，**一个字节都不往盘上写**
   （边界①）。写那一侧（删 / 改名 / 建目录 / 复制）整个留在 SFTP。
   ⚠ 后两条也在这条之内：`files-index-rebuild` 是**遍历 ＋ 换掉内存里那一份**，
   `files-browse` 是**登记名单 ＋ 重列一遍** —— 两条都不往盘上写。
2. 🔴 **线上名用连字符，能力名用点。** 后端内部这一族的能力叫 `files.ls` / `files.stat` /
   `files.find` / `files.index.status` / `files.index.rebuild` / `files.browse`
   （那两张表逐字）；线上与 CLI 那一面一律是 `files-ls` / `files-stat` /
   `files-find` / `files-index-status` / `files-index-rebuild` / `files-browse`。
   两者之间的翻译**只有一处**（`files::answer_wire`）。客户端只看线上那一套。
3. **路径走原始字节。** 凡是路径类的值（入方向的 `path` / `under`、
   出方向的 `path` / `index_root` / `cover_root`），要么是一个 UTF-8 字符串，要么是
   `{"b16":"<十六进制>"}`。**这里刻意不「尽力而为」地猜** —— 非 UTF-8 的文件名被有损
   解码之后就寻址不到了，猜错一个字节就是去看另一个文件。
   ⚠ 出方向能用字符串表达的就用字符串，不能的才出 `b16` 形。

⚠ **今天它们在真机上答得出什么**（如实写，别当功能说明）：`files-ls` / `files-stat` /
`files-index-status` 是完整的；而 `files-find` 查的是**常驻索引**，而
**后端没有任何自己去建索引的节拍** ——：机制在后端、偏好由后端声明、
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
| `link_dir` | ← | 只在 `kind` 是 `symlink` 时出：它指向的是不是目录（跟链接问一次）。断链 ⇒ 不出这个键。文件窗口据此让指向目录的链接点得进去 |
| `size` | ← | 字节数 |
| `mtime_secs` | ← | Unix 纪元秒 |
| `truncated` | ← | 目录里的项数多于回送的条数（被 `limit` 截了） |

**错误码**：`bad_path`（`path` 缺了 / 形状不对 / 空）· `unreadable`（这个目录打不开）。

#### `files-stat`：一个路径的元数据（步 `24f`，**只读**）

```text
→ {"id":"f2","cmd":"files-stat","args":{"path":"/home/u/p/a.rs"}}
← {"kind":"reply","id":"f2","ok":true,"data":{
     "path":"/home/u/p/a.rs","kind":"file","size":1234,"readonly":false,"mode":420,"mtime_secs":1758300000,
     "owner":"u","link_target":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 入方向是要问的那个路径；出方向原样回送（原始字节形） |
| `kind` | ← | 同上那个闭集四个词 |
| `size` / `mtime_secs` | ← | 字节数 / Unix 纪元秒（`mtime_secs` 拿不到就不出这个键） |
| `readonly` | ← | 这个路径此刻是不是只读 |
| `mode` | ← | unix 权限位的低 12 位（十进制数；`420` = `0o644`）。**非 unix 平台不出这个键**（不是 `0`：`0` 是一个真能设的值）。文件窗口改权限那个框拿它显示现值 |
| `owner` | ← | 属主（同上几格，跟链接）：用户名；查不到名字 ⇒ uid 的数字串；非 unix ⇒ `null`。文件窗口「属性」显示它 |
| `link_target` | ← | 路径**本身**是符号链接 ⇒ 它的目标原文（`readlink`，不解不跟；原始字节形：字符串或 `{"b16":…}`）；不是链接 ⇒ `null`。文件窗口「属性」显示它 |

⚠ **它跟 symlink**（拿的是链接指向的那个东西的元数据）；只有 `link_target` 那一格说的是路径本身（`readlink` 读得出 ⇒ 是链接）。
要区分链接本身：看 `link_target` 是不是 `null`，或用 `files-ls` 看它父目录那一行的 `kind`（那一栏不跟链接）。

**错误码**：`bad_path` · `unreadable`（这个路径读不到）。

#### `files-find`：在常驻索引里查（步 `24f`，**整族的存在理由**；Everything 式搜索词 · 分页 · 丢旧号 2026-10-01）

```text
→ {"id":"f3","cmd":"files-find","args":{"query":"report ext:pdf;txt","seq":7,"stream":"w1-3","offset":0,"limit":100}}
← {"kind":"reply","id":"f3","ok":true,"data":{
     "hits":[{"path":"/home/u/docs/report.pdf","kind":"file"}],"total_hits":1,"truncated":false,
     "scanned":20220,"index_age_secs":12,"index_missing":false,"stale":false,
     "index_root":"/home/u","out_of_index":false,"cover_root":"/home/u","seq":7,"offset":0}}
```

🔴 **它是 SFTP 给不了的那一条**：那个协议只能递归 `READDIR`（N 次往返），
而且**没有地方放一份常驻索引**。索引必须住在被搜的那台机器上 ⇒ 只有后端住在那里。
🔴 **搜索词只在后端被读懂**：窗口原样发用户敲的字，语法解析与匹配都在这里。

| 字段 | 向 | 说明 |
|---|---|---|
| `query` | → | 原样的搜索词（字符串）。空白 ⇒ 匹配一切。语法见下 |
| `under` | → | 只搜这个目录**底下**（不含它自己；字符串或 `{"b16":…}`）。不给 ⇒ 这台机器的家目录（家目录说不出 ⇒ 整份索引） |
| `seq` | → ← | 这一趟的号（非负整数，可不给）。同一个 `stream` 里来了**更大**的号 ⇒ 在飞的旧那一趟收手、回 `superseded`；晚到的旧号当场回 `superseded`；同号再来（往下翻页）照常。出方向原样回送（没给 ⇒ `null`） |
| `stream` | → | 这个号属于哪一个搜索框（字符串，不给 ⇒ 空串）。两个搜索框各用各的名字，互不撤。后端最多记 64 个，满了丢最久没来的那个 |
| `offset` | → ← | 从第几条命中起回（前面的只数不回）。不给 ⇒ `0`；出方向原样回送 |
| `limit` | → | 这一屏最多回几条。不给 / 给 0 ⇒ **1000** |
| `hits` | ← | 这一屏的命中，每条一个对象：`path`（原始字节形）· `kind`（同 `files-ls` 那个闭集四个词，不跟链接）。🔴 **只回送命中** —— 未命中的那几十万条路径一个字节都没离开那台机器 |
| `total_hits` | ← | 一共命中几条，**不受分页影响** |
| `truncated` | ← | 这一屏之后还有（往下翻：同号、`offset` 加上这一屏的条数） |
| `scanned` | ← | 这一趟扫了几条（= 索引条目数）。**反空真用**：扫到 0 条的「没命中」与「索引是空的」在界面上一模一样 |
| `index_age_secs` | ← | 答这一趟用的那份索引，是多久以前建的 |
| `index_missing` | ← | 🔴 **索引还没建过** ⇒ 上面几个数全是 0，而那**不是**「没搜到」。见本族总说明那条 ⚠ |
| `stale` | ← | 该重走了（`index_age_secs > rewalk_interval_secs`） |
| `index_root` | ← | 手上那份索引的根（没建过 ⇒ `null`） |
| `out_of_index` | ← | 这一趟的范围（`under` 或家目录）不在手上那份索引里 ⇒ 结果只是索引里碰巧有的那一部分 |
| `cover_root` | ← | 要搜全这一趟，重走该走哪个根：手上那份盖得住 ⇒ 它的根；否则范围在家目录里 ⇒ 家目录；否则 ⇒ 范围本身（都说不出 ⇒ `null`）。🔴 **要不要重走由后端判**：`index_missing` / `stale` / `out_of_index` 任一为真 ⇒ 调用方照 `cover_root` 发 `files-index-rebuild` |

**语法**（默认不分大小写；词里有带大小写的非 ASCII 字母时整段按 Unicode 小写比）：

- 空格 ＝ 且 · `|` 或 `OR` ＝ 或（比空格结合得紧：`a b|c` 是 a 且 (b 或 c)）· `!` 或 `NOT` ＝ 非 · `< >` 或 `( )` 分组 · `"…"` 里的空格与符号照原样。
- `*` 任意串 · `?` 一个字符；带通配的词要对上**整个**名字（或整条路径），不带的是子串。
- 只对**名字**（最后一段）比；词里带 `/`（Windows 上 `\` 也算）⇒ 对全路径。`path:<词>` 强制对全路径。
- `ext:<扩展名>[;<扩展名>…]`：只要文件，扩展名不分大小写、前导点可写可不写。
- `file:` / `folder:`：只要文件（不是目录的都算）/ 只要目录；后面可以跟一个词（`folder:src`）。
- 打到一半的引号 / 分组在末尾自动收口；`ext:` / `path:` 后面还没写东西 ⇒ 先不缩。
- 认不出的 `xx:`（如 `12:30`）当普通字；Everything 认得、这里不支持的（`size:` `dm:` `regex:` `case:` `type:` `parent:` …）⇒ `bad_query`。
- 命中按索引里的顺序给（**没排序**）；翻页之间索引若换了一份，后面几页会错位。

⚠ **它不重走、不阻塞**：拿的是手上那一份，并把年龄与「该不该重走、走哪个根」一起交回去。
⚠ 按内容搜是另一条 `files-grep`（见下）。模糊匹配 · 排序 —— 没做。
⚠ 丢旧号靠号不靠计时：旧那一趟每扫 8192 条看一次号，扫完再看一次。

**错误码**：`bad_args`（`query` 缺了 / 不是字符串 · `seq` 不是非负整数）· `bad_path`（`under` 形状不对 / 空）·
`bad_query`（搜索词里有多出来的右括号，或用了不支持的过滤器；附一句说是哪一处）· `superseded`（同一个搜索框已经来了更新的号）。

#### `files-grep`：在一个目录底下按内容搜（09-28；，**可撤**）

```text
→ {"id":"f9","cmd":"files-grep","args":{"path":"/home/u/p","needle":"TODO","ignore_ascii_case":false,"limit":200}}
← {"kind":"reply","id":"f9","ok":true,"data":{
     "path":"/home/u/p",
     "hits":[{"path":"/home/u/p/src/a.rs","line":12,"text":"// TODO: split","matches":3}],
     "files":480,"bytes":5242880,"links":2,"skipped_binary":7,"skipped_large":1,"skipped_mounts":0,"unreadable":0,
     "limit":200,"truncated":false,"stopped":null}}
```

在那台机器上走一遍 `path` 那棵树、只回命中的那几份（字节不过网）。

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | ↔ | 从哪个目录往下搜（字符串或 `{"b16":…}`；也可以是一份文件）；回送原样那一格 |
| `needle` | → | 要找的**字节**子串（字符串或 `{"b16":…}`），1 至 256 字节 |
| `ignore_ascii_case` | → | 只对 ASCII 段大小写不敏感。不给 ⇒ `false` |
| `limit` | ↔ | 最多回几份命中的文件。不给 / 给 0 ⇒ **200**，至多 **1000** |
| `hits` | ← | 每份命中的文件一项：`path`（字符串或 `{"b16":…}`）· `line`（第一处命中所在行号，从 1 起）· `text`（那一行里命中前后至多 200 字节，首尾剥空白；非 UTF-8 走 `{"b16":…}`）· `matches`（这份里命中了几行）。同一层按名字字节序、先文件后子目录 |
| `files` · `bytes` | ← | 这一趟读了几份、多少字节 |
| `links` | ← | 碰到几条链接 —— **不跟**（它不一定在这棵树里，跟进去就是出界）；根本身是链接 ⇒ 不进去 |
| `skipped_binary` · `skipped_large` · `skipped_mounts` · `unreadable` | ← | 看着像二进制（前 8 KiB 有 NUL）· 超过 8 MiB · 挂在底下的别的文件系统 · 读不了的，各跳过几项 |
| `truncated` · `stopped` | ← | 上界到了没走完：`"hits"`（命中份数到 `limit`）/ `"bytes"`（一趟累计读到 256 MiB）；走完 ⇒ `false` · `null` |

⚠ **可撤**：异步档，`cancel` 打得断 —— 走那一趟在阻塞线程池上，这一问被撤时取消位置上，那一趟下一项就收手（不回应答）。
⚠ **纯读**：一个字节不写（同族边界①）。**错误码**：`bad_args`（`needle` 缺 / 空 / 超长 / 形状不对）· `bad_path` · `unreadable`（根读不到）。

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
| `skipped_mounts` | ← | 根底下挂着的**别的文件系统**没走进去的个数（设备号比对；那个目录本身照样在索引里，它底下的不在）。非 unix 上恒 `0`（设备号问不出，那一判不开口） |
| `truncated` | ← | 上一趟遍历撞到了条目数上限，没走完 |
| `age_secs` | ← | 这份索引建好到现在多少秒 |
| `rewalk_interval_secs` | ← | 🔴 **后端声明的重走周期**（今天 300）。要求这个延迟**显示在界面上**，不许让用户猜为什么搜不到 ⇒ 它必须可查询，不许只活在代码里 |
| `stale` | ← | `age_secs > rewalk_interval_secs` |
| `browse_watches` | ← | 此刻给「用户正在浏览的那几个目录」挂着几个 watch |
| `browse_watch_cap` | ← | 最多挂几个。全挂挂不住：本机 `inotify` 每用户上限现打 262144，而 home 下 640413 条目，且非特权拿不到全文件系统监听 |
| `cold_first_build_secs` | ← | 后端**声明**的冷启动首建大约要几秒（今天 10，出处见 `index.rs::COLD_FIRST_BUILD_SECS`：一台 NVMe 上 `find` 的冷缓存读数取上整，**代理指标、不是实测**）。与 `rewalk_interval_secs` 分开：周期性重走是热的，后端刚起那一趟是冷的、用户看得见 ⇒ 窗口在 `index_missing` 那一趟的重走期间显示「正在建索引（首次约 N 秒）」 |

🔴 **「重走」这件事后端自己不做** —— 后端那条零定时器铁律不许它长出节拍
（机制在后端、偏好由后端声明、**节拍在调用方**）。
〔⚠ 2026-09-21 订正：原话「**今天这一族里没有一条命令能触发重走**」**在 `24f` 第三刀（同日更早）落地时就已经假了** —— `files-index-rebuild` 就是那条命令。而它当时没人发；波 β 的 `P2` 之后，发它的是 `src/frontend/filewin/src/find.rs`。⇒ 今天的真话是：**这一族有一条命令能触发重走，而且窗口那一侧真的会在后端说没索引／过期时发它**。〕

⚠ 三个平台上的保鲜机制**本来就不是一件事**（Linux/Windows/macOS 各一套，
macOS 那一格是文献读数、没实测）。跨 target 要判的是「**能力在不在**」，
**不是「新鲜度一样」**（边界②）—— 把它们判成相等会逼人写假声明。

**错误码**：无（它只读自己手上那份索引的计数）。

#### `files-index-rebuild`：走一遍，就一遍，做完返回（步 `24f` 第三刀，**只读**）

```text
→ {"id":"f5","cmd":"files-index-rebuild","args":{"path":"/home/u"}}
← {"kind":"reply","id":"f5","ok":true,"data":{
     "path":"/home/u","entries":20220,"resident_bytes":2544180,
     "unreadable_dirs":3,"truncated":false}}
```

🔴 **它就是那条裁定里的「机制」那一格**：走一遍、就一遍、做完返回，
**后端里没有任何「隔 N 秒再走一次」**（零定时器铁律，`no_timer_guard` 零容忍）。
「隔多久再叫一次」是**调用方**的事（`K37` 逐字「后端只给机制，不给偏好」），
而后端**声明**的建议周期摆在 `files-index-status` 的 `rewalk_interval_secs` 里。
⚠ ⇒ **不发这条命令，索引就永远不会变新。** 这不是缺陷登记，是这条契约的语义。

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 要走的那个**根**。字符串或 `{"b16":…}`；**不给 ⇒ 这台机器的家目录**（说不出 ⇒ `no_home`）；空 / 形状不对 ⇒ `bad_path`。出方向回送真走的那个根 |
| `entries` | ← | 这一趟走出来多少条（目录 ＋ 文件 ＋ 符号链接，根自己不算） |
| `resident_bytes` | ← | 新那份索引在后端内存里占多少字节（路径总长 ＋ 5×条数：每条 4 字节界桩 ＋ 1 字节类型，**算得出的量**） |
| `unreadable_dirs` | ← | 这一趟有几个子目录读不进去（权限等）。**不是 0 就说明这份索引有洞** |
| `skipped_mounts` | ← | 根底下挂着的**别的文件系统**没走进去的个数（设备号比对；那个目录本身照样在索引里，它底下的不在）。非 unix 上恒 `0`（设备号问不出，那一判不开口） |
| `truncated` | ← | 撞到条目上限、没走完 ⇒ 这份索引是**不完整**的 |

🔴 **根本身读不进去是「拒」，不是「走出一棵空树」**：这一趟回 `unreadable`，
而**常驻那一份一个字节都不动**。理由是结构性的 —— 遍历对一个打不开的目录只会把
`unreadable_dirs` 加一并交一份空快照，而这条命令会把常驻那一份**整份换掉**
⇒ 路径打错一个字母就能把手上那份好索引顶成空的，且回参看起来像成功
（`entries: 0` 与「这台机器上真的没文件」同形）。⇒ 换之前先探一次根。

⚠ **它不跟 symlink 往里走**（链接当一条条目收进索引，但不进它指的那棵树）、
**不跨文件系统边界那一档没做**（根底下挂着别的文件系统时会一起走完）、
**建索引的墙钟没有上限保证**（64 万条现打 0.99 秒是**热缓存**；冷缓存没量过）。
⚠ **同一时刻只走一趟**：上一趟还没走完时再发 ⇒ `already_rebuilding`，手上那份不动。走完整份换掉，不合并。

🔴 **索引是「这一个后端进程」手上的那一份** —— 所以这条命令与 `files-find`
**必须在同一条连接上**（帧面：一条长连接、多次往返）。
⚠ **CLI 面那一对配不起来**，这是现打的读数不是推论：一次 exec = 1 请求 1 响应 1 退出
⇒ `--files-index-rebuild` 建好的那份索引随那个进程一起没了，紧接着的
`--files-find` 那一 exec **照旧回 `index_missing: true`**（本机 debug 档现打：
rebuild 回 `entries:4`，下一个 exec 的 `--files-index-status` 回 `index_missing:true`）。
⇒ CLI 面这一条的用处是**量一趟遍历**（条目数 / 常驻字节 / 有几个目录读不进去），
**不是**给 `--files-find` 预热。要「建完就能查」走帧面。

**错误码**：`bad_path`（`path` 形状不对 / 空）· `no_home`（没给 `path`，而这台机器说不出家目录）· `unreadable`（这个根打不开 ⇒ **拒**）· `already_rebuilding`（上一趟还没走完）。

#### `files-browse`：告诉后端「用户现在在看哪几个目录」（步 `24f` 第三刀，**只读**）

```text
→ {"id":"f6","cmd":"files-browse","args":{"dirs":["/home/u/p","/home/u/q"]}}
← {"kind":"reply","id":"f6","ok":true,"data":{
     "added":2,"removed":0,"rejected":0,"browse_watch_cap":64,"watching":2,"watch_failed":0,"watch_error":null}}
```

它是**保鲜的另一半**（那张三段表的第二段）：把「用户眼前那几个目录」
交给后端，后端对它们单独维护一份 overlay —— 查询时 overlay 里那几个目录的直接子项
**盖掉**大索引里的对应条目。

| 字段 | 向 | 说明 |
|---|---|---|
| `dirs` | → | **此刻的整份名单**（数组，每项是字符串或 `{"b16":…}`）。后端自己算差分：不在名单里的卸掉、新来的挂上 |
| `added` | ← | 这一趟新挂上几个 |
| `removed` | ← | 这一趟卸掉几个（用户不再看它们了） |
| `rejected` | ← | 超过上限被**拒掉**几个。🔴 它必须是个数、必须回给调用方：静默截断会让「这个目录我明明在看、新建的文件却要等重走」变成一个查不出原因的现象 |
| `browse_watch_cap` | ← | 上限（今天 64）。只回一个 `rejected` 而不说上限是多少，调用方没法判该少送几个 |
| `watching` | ← | 此刻**真挂着** watch 的目录数（进程里那一个监听器，跟着名单挂 / 卸） |
| `watch_failed` / `watch_error` | ← | 这一趟没挂上的条数 ＋ 第一条原因（`null` ＝ 都挂上了）。没挂上的那几个登记了、当场重列了，但**不会**跟着新 —— 出声，不静默；监听器本身起不来（如 `inotify` 实例数到顶）也落这里，下一趟再试 |

⚠ **收的是「整份名单」，不是「再加一个」。** 空数组**合法**，语义是
「用户现在什么都没在看」⇒ 全卸（`removed` 说出来卸了几个）。
**而「少了 `dirs` 这个参数」是另一件事** ⇒ `bad_args`（别把漏参数当成「要全卸」）。

🔴 **它今天买到的比「那几个目录此后实时」小，逐条别读宽：**

- 它做的是**登记名单 ＋ 当场把那几个目录各重列一遍 ＋ 让后端进程里那一个监听器跟上名单**
  （此前 `BrowseWatcher` 零生产调用方；今天第一次 `files-browse` 时起、此后一直持有）。
  ⇒ 买到的是「**这几个目录此后一有动静，overlay 就跟着重列**」（`watching` 说挂上了几个）。
  ⚠ 仍然不是全部：watch 绑 inode 不绑路径（浏览的目录删了重建成同名新目录 ⇒ 那一格瞎到下一次 `files-browse`）·
  内核事件队列溢出没判 · 窗口关了没人发空名单 ⇒ 最后那一份名单的 watch 留到下一次 `files-browse`（上限 64 管着）。
- **只盯直接子项**（不递归）：浏览的目录**底下**那棵子树里新建的东西仍然等重走那一档。
- 三个平台的保鲜机制**本来就不是一件事**（Linux `inotify` / Windows
  `ReadDirectoryChangesW` / macOS `FSEvents`，最后一格是文献读数、没实测）。
  跨 target 要判的是「**能力在不在**」，不是「新鲜度一样」（边界②）。

**错误码**：`bad_args`（`dirs` 缺了 / 不是数组）· `bad_path`（数组里**某一项**不是一个路径）。
两个码刻意分得开：前者要改的是调用形状，后者要改的是名单里那一项。

#### `files-read-text`：读一份文本进编辑器（2026-09-24，**只读**）

同族第七条。文件窗口的编辑器此前为这一问单拨一条 SFTP 把字节整份搬过来；现在经通道问后端。

```text
→ {"id":"f7","cmd":"files-read-text","args":{"path":"/home/u/p/a.md","max_bytes":262144}}
← {"kind":"reply","id":"f7","ok":true,"data":{"path":"/home/u/p/a.md","text":"# hi\n","bytes":5,"sha256":"<64 位小写十六进制>"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 要读的那份文件。字符串或 `{"b16":…}`；出方向原样回送（原始字节形） |
| `max_bytes` | → | 🔴 **必须给**：编辑上限是**调用方**的（它答的是「这个文本控件打字卡不卡」）。只收 `1..=8388608`（后端一趟肯交的天花板 8 MiB）；超出 ⇒ `bad_args`，**不替调用方夹小** |
| `text` | ← | 整份内容（合法 UTF-8） |
| `bytes` | ← | 字节数 |
| `sha256` | ← | 交出去的那份字节的 SHA-256（64 位小写十六进制）。编辑器存回去时原样交回当 `expect: {"sha256": …}`（`files-write-text` / `files-commit-text` 的 CAS）；算法只住后端，调用方当不透明令牌 |

🔴 **超上限整趟拒，不截断**（截断过的文本存回去会写坏文件）。大小判两次：`stat` 出来超了 ⇒ 拒；
真读的时候比 `stat` 时大（文件正在长）⇒ 同样拒 —— 最多只多读一个字节就知道，不会把一个刚变大的文件整个读进内存。

**错误码**：`bad_path`（`path` 缺了 / 形状不对 / 空）· `bad_args`（`max_bytes` 缺了 / 越界）·
`unreadable`（读不到）· `too_large`（超上限，话里带着多了多少字节）·
`not_text`（不是普通文件 / 含 NUL 字节 / 不是合法 UTF-8 —— 不猜编码、不有损替换）。

⚠ 它**不过会话数据围栏**：那道围栏立在写侧；把一份会话记录读进编辑框不改任何东西，
存回去那一下（`files-write-text`）照旧会被拒。〔用户〕写侧那道也拿掉了（「文件管理器全部都可以改.
不需要任何围栏」）：会话记录存得回去。

#### `files-home`：后端这个用户的 home（2026-09-24，**只读，不读 stdin**）

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

#### `files-read-chunk`：按字节寻址分块读回（2026-09-27 · 用户）

```
→ {"id":"c1","cmd":"files-read-chunk","args":{"path":{"b16":"2f7372762f66fe"},"offset":0,"len":262144}}
← {"id":"c1","ok":true,"data":{"path":{"b16":"2f7372762f66fe"},"offset":0,"size":5,"eof":true,"content":{"b16":"68656c6c6f"}}}
```

| 字段 | 方向 | 说明 |
|---|---|---|
| `path` | → ← | 一份普通文件（字符串或 `{"b16": …}`；非 UTF-8 名的下载就走这一形，SFTP 库的路径是 `String` 寻址不到） |
| `offset` / `len` | → | 从哪读、读多少；`len` 只收 `1..=READ_CHUNK_MAX_BYTES`（256 KiB），越界 `bad_args`、不夹小 |
| `size` | ← | 此刻整份多大（调用方据此报进度、判读完） |
| `eof` | ← | 这一块读到了末尾（`offset` 越过末尾 ⇒ 空块、`eof: true`） |
| `content` | ← | 这一块的原始字节，恒为 `{"b16": …}` |

- 纯读（读族第十条），与 `files-stage-chunk`（分块写进暂存区）对称。**下载对远端只读**：非 UTF-8 名的下载逐块读回，本机那一头由本机后端 `files-stage-chunk` ＋ `files-commit-upload`（带 `chunks`，`rel` 收 b16）落盘。
- 不是普通文件 ⇒ `not_text`；读不到 / 打不开 ⇒ `unreadable`。
- **CLI 面同样有它**（`--files-read-chunk`，载荷走 stdin）。

#### `files-size`：算一个目录有多大（2026-09-25，**只读**）

同族第九条（「算目录大小」）。在那台机器上走一遍、只回几个数 —— 字节不过网。

```text
→ {"id":"f9","cmd":"files-size","args":{"path":"/home/u/p"}}
← {"kind":"reply","id":"f9","ok":true,"data":{
     "path":"/home/u/p","bytes":123456,"files":42,"dirs":7,"links":1,"other":0,"skipped_mounts":0,"unreadable_dirs":0}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → ← | 要算的那个路径（字符串或 `{"b16":…}`）；出方向原样回送（原始字节形）。顶上这一格**跟链接**（同 `files-stat`） |
| `bytes` | ← | 普通文件的**表观大小**之和（与 `files-ls` 的 `size` 同口径，不是占盘块数） |
| `files` / `dirs` | ← | 普通文件数 / 目录数（含顶上那个目录自己）。顶上是一个文件 ⇒ `files: 1`、`dirs: 0` |
| `links` | ← | 符号链接数 —— **不跟、不算字节**（它指向的东西不一定在这棵树里） |
| `other` | ← | 设备 / 管道 / 套接字之类 |
| `skipped_mounts` | ← | 底下挂着的**别的文件系统**，没走进去的个数（设备号比对）。非 unix 上设备号问不出 ⇒ 恒 `0`（那一判不开口） |
| `unreadable_dirs` | ← | 读不进去、跳过的目录数（不中断） |

⚠ **不设条目上限**（与 `files-index-rebuild` 走整棵树同形）：阻塞档、开跑之后取消不掉，一棵极大的树就是一趟很长的往返 —— 调用方的期限是它唯一的上界。

**错误码**：`bad_path` · `unreadable`（顶上那一格读不到）。

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
| `rel` | → | 相对 `root` 的那一段。字符串或 `{"b16":…}`（此前只收 UTF-8 字符串；围栏改成按 `Path` 逐段判之后，非 UTF-8 的名字照样逐段过围栏） |
| `content` | → | 要写进去的字节。字符串或 `{"b16":…}`。**不给这个参数 ⇒ 新建一份空文件**（那就是「新建空文件」这件事的形状） |
| `path` | ← | 真正落盘的那个绝对路径，**解完 symlink 的**（原始字节形） |
| `bytes` | ← | 这一趟写进去了几个字节 |

🔴 **它做什么、以及它刻意不做什么**（别读宽）：

- 它只会**新建一份此前不存在的文件**（`O_EXCL`）。目标已经在了（哪怕它只是一条
  symlink）就直接失败，**绝不跟随、绝不覆盖**。新文件的权限位是后端进程的缺省（受 umask）；
  落在这台机器 `~/.cc-monitor` 里的 ⇒ `0600`（unix，写进内容之前就定）。
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
**第三层**：能用的改动动词是闭集 · 每一处先过路径解析（原文「先过 Claude 会话数据围栏」）· 后端源码里够得到那个模块的
**只有**命令注册那一处。

五条共用的口径（别读宽）：

- `root` 与 `files-create` 同形（字符串或 `{"b16":…}`，必须已在盘上）；相对段（`rel` / `from` / `to`）
同样收字符串或 `{"b16":…}`（乱码文件名因此改得了名、删得掉、改得了权限；形状不对 ⇒ `bad_args`，不猜），
  上跳段 / 绝对路径 / 盘符 / 空段 / 当前目录段一律拒（`refused`）。
- 〔用户〕**不再有任何数据围栏**：用户原话「文件管理器全部都可以改. 不需要任何围栏」⇒
  会话文件（`projects/<proj>/<sid>.jsonl` · `sessions/<x>.json`）、项目目录、subagent 记录、tasks 与别的文件**同一条路**。
  （此前：「围栏只拦那几份具体的会话文件 …… `~/.claude` 底下的其余东西改得动」，用户 09-23 那一裁。）
  仍会拒的只有**路径解析**那几形：上跳 / 绝对路径 / 空段 / 父目录不在 / 解完链接跑出 `root`（跟链接的动词解到底再判）——
  它不限制改什么，只保证改的就是 `root ＋ rel` 那一格。
- 错误码四个，与 `files-create` 同义：`bad_path` · `bad_args` · `refused`（路径解析拒的 / 形状不对）· `io_failed`（盘上没成）。
- ⚠ **判定与动手之间的窗**（TOCTOU）：能用原子原语闭合的已闭合 —— 改名不覆盖（`renameat2(RENAME_NOREPLACE)`；Windows `MoveFileExW` 不带替换旗）· 复制与 `files-put` 新建那一形先写同目录旁名再不覆盖上位 · 开文件全程 `O_NOFOLLOW`（Windows 没有等价开法）。仍开着：父目录在解析与动手之间被整个换掉 · 递归删 / 递归复制逐条的窗 · CAS 与换名之间 · `files-chmod` 跟链接。那块盘不认不覆盖改名（NFS、部分 FUSE）⇒ 普通文件走 `link` ＋ `unlink`（目标已在时 `link` 原子失败），目录拒（`refused`：「这个盘不支持不覆盖改名目录」），不退回先看后改。
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

权限位是后端进程的缺省（受 umask）；建在这台机器 `~/.cc-monitor` 里的 ⇒ `0700`（unix）。

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
那是一次没人问过的覆盖。一次原子的不覆盖改名，没有先看后改的窗；盘不认这个旗 ⇒ 普通文件 `link` ＋ `unlink`、目录 `refused`（说「这个盘不支持不覆盖改名目录」）。

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
| `recursive` | → | 布尔，**缺省 `false`**。不给 ⇒ 射程与此前一个字节不差（非空目录 ⇒ `io_failed`）；给了不是布尔 ⇒ `bad_args`（不猜） |
| `expect` | → | 可选，字符串或 `{"b16": …}`：「我读到的是这一份」。给了 ⇒ 目标必须是一份**普通文件**（目录 / 链接 ⇒ `refused`），盘上逐字节等于它才删；不等或已经不在 ⇒ `stale`，一个字节不动。`null` ⇒ `bad_args`；与 `recursive: true` 同给 ⇒ `bad_args`。不给 ⇒ 行为不变 |
| `expect`（空目录形） | → | 〔SU1 问 2〕恰好 `{"empty_dir": true}`：「我看到的是一个空目录，删它」。目标（不跟链接地看）必须是**真目录**（文件 / 链接 ⇒ `refused`）；不空 ⇒ `stale`（`remove_dir` 自己拒非空，没有先看后删的窗）；不在 ⇒ `stale`。`{"empty_dir": false}` / 多一个键 ⇒ `bad_args`（按逐字节形取、取不出）；与 `recursive: true` 同给 ⇒ `bad_args`。卸 skill 删完装时写的文件之后用它收掉空目录 |
| `path` | ← | 删掉的那一项 |
| `removed` | ← | 这一趟真删掉了几条（含目标自己；不递归那一支恒 `1`） |

🔴 **不带 `recursive` 就不递归，刻意的**：路径解析的射程是一条路径，递归删动的是整棵子树。
（此前这里的理由是「底下藏着的一份会话文件照样被一起删掉」；那道拦截用户拿掉了，会话文件照删。）

🔴 **带 `recursive: true` ⇒ 逐条目过路径解析**（后端 `control/files_write.rs::delete_tree`）：
- **计划趟（只读）**：不跟链接地走整棵树，**每一条目各过一次路径解析**。任一条被拒（从前含「树里藏着一份会话文件」，今天不含）·
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

🔴 **它跟链接** ⇒ 落点连最后一段也解到底再判一次：根里一条指向根外的链接不许借它把根外那一份改掉（此前的例子是「指向会话文件的链接」，那一形今天放行）。
⚠ 非 unix 平台上**如实回 `no_unix_mode`**（此前回 `io_failed` —— 那是「重试才有意义」的码，说错了），不假装改成了。
这个码同时是本命令**声明过**的命令级码：后端 target 轴（`lib.rs::capabilities_on`）从它现推「Windows 上没有这一条」，
差异登记表 `TARGET_GAPS` 里有对应两行（帧面 · CLI 面，档 = 结构）。

#### `files-write-text`：覆盖写一份**已经在**的普通文件

```text
→ {"id":"w6","cmd":"files-write-text","args":{"root":"/home/u/docs","rel":"a.md","content":"new text","expect":{"sha256":"<打开时 files-read-text 交的那个>"}}}
← {"kind":"reply","id":"w6","ok":true,"data":{"path":"/home/u/docs/a.md","bytes":8,"sha256":"<写进去那份的>"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` / `rel` | → | 目标根 ＋ 相对段。目标必须**已经在、且是普通文件**；新建请走 `files-create`（`O_EXCL`，两条路刻意分开） |
| `content` | → | 字符串或 `{"b16":…}`。🔴 **必须给** —— 不给不默认成空（那等于把那份文件清空） |
| `expect` | → | 🔴 **必须给**，恰好 `{"sha256": "<64 位小写十六进制>"}`：「我看的时候那一份」的摘要（`files-read-text` 交的那个）。盘上那份的摘要不等 ⇒ `stale`，一个字节不写；目标已经不在 ⇒ `stale`；缺了 / 形状不对 ⇒ `bad_args`。没有「不问就盖」这一形 |
| `path` | ← | **解到底**的那个真路径（它跟链接，理由同 `files-chmod`） |
| `bytes` | ← | 写进去了几个字节 |
| `sha256` | ← | 写进去那份的摘要 —— 调用方拿它当下一次存的 `expect`（连存两次不自撞） |

⚠ 没有大小上限、没有「写之前那一版」的备份 —— 本面只做「写」这一件。
CAS 用**摘要形**而不是 `files-put` / `files-delete` 那种逐字节形：编辑器存盘装不进一行时分块走（`files-commit-text`），
逐字节的 `expect` 要么让一行的门槛减半、要么把原文也分块送一遍；摘要定长、两支同形。它仍是 CAS（比的是「我看的时候那一份」），只是换了表示。
⚠ 比对与写之间仍有窗（TOCTOU）：CAS 缩小的是「读 → 写」那一整趟往返的窗。

**原子地换**：unix 上同目录 `O_EXCL` 暂存旁名写满、沿用原权限位、换名上位 ⇒ 写到一半失败或后端在写的中途被收掉，目标原封不动（旁边可能剩一份 `.<名>.ccm-put-<pid>-<序>.part`）。**例外**：目标有硬链接（`nlink > 1`）或属主不是后端这个用户 ⇒ 退回**就地写**（保住硬链接与属主；非原子，后端日志记一句「这一次不是原子写，因为 …」）。已知代价：xattr / ACL 不跟过来。Windows 上照旧就地写（保 ACE）。

#### `files-commit-upload`：把暂存区里一份传完的上传件挪进目标（F7c，2026-09-24）

```text
→ {"id":"w7","cmd":"files-commit-upload","args":{"key":"0123456789abcdef0123456789abcdef","root":"/home/u/docs","rel":"a.bin","overwrite":false,"expect":{"sha256":"<transfer 帧 end.sha256>"}}}
← {"kind":"reply","id":"w7","ok":true,"data":{"path":"/home/u/docs/a.bin","bytes":1048576}}
```

SFTP 缩成只做传输之后，上传**只写** `~/.cc-monitor/staging/<key>.part`；
把它挪进用户目标的那一下**只有这条命令**（用户逐字「现在只允许后端的文件管理部分写文件」）。

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 暂存件的键：**恰好 32 位小写十六进制**。暂存件路径由后端自己拼（`$HOME/.cc-monitor/staging/<key>.part`），调用方指不到暂存区之外任何文件 |
| `root` / `rel` | → | 目标根 ＋ 相对段，先过写面那两道路径解析（词法 ＋ 父目录解 symlink；「会话文件那一问」删了）；`rel` 也收 `{"b16": …}` |
| `overwrite` | → | 🔴 **必须给**（`true` / `false`），不给默认值。`false` ⇒ 先 `O_EXCL` 占位（目标已在 ⇒ `io_failed`），再改名上位；`true` ⇒ 直接改名上位（同盘原子） |
| `expect` | → | 🔴 **必须给**，恰好 `{"sha256": "<64 位小写十六进制>"}`：传输台上传时对**本机那份整份**算的摘要（`transfer` 帧 `end.sha256`，窗口原样交来）。改名上位**之前**对暂存件整份算一遍：不等 ⇒ `stale`、目标一个字节不动、**坏暂存件删掉**（调用方从 0 重传）；缺了 / 形状不对 ⇒ `bad_args`。为什么：续传只对尾块，看不见「前缀 ＋ 洞 ＋ 尾巴」（失败后晚到的写在中间留的洞） |
| `chunks` / `bytes` | → | 可缺席（缺席 ＝ 此前那一形）。给了 ⇒ **块形**：先把 `files-stage-chunk` 送来的 `<key>.0.chunk` … `<key>.<chunks-1>.chunk` 依次拼成 `<key>.part`（`O_EXCL`、流式），总长必须恰好 `bytes`、不多一块，否则 `io_failed`、目标一个字节不动；**不论成败**这一键的块都删掉。拼好之后走下面同一条提交（`expect` 照核）。SFTP 起始目录不是这台后端的 home（chroot / `internal-sftp -d`）时窗口走这一形 |
| `path` | ← | 落点（父目录解过 symlink 的那一个） |
| `bytes` | ← | 暂存件的字节数（改名不搬字节，这个数就是它在盘上的长度） |

⚠ 暂存件不在（传输没跑完 · SFTP 起始目录不是这台后端的 home）⇒ `io_failed`；暂存件是链接或目录 ⇒ `refused`。
上传开单时窗口带上那台后端的 `$HOME`（`transfer-upload` 的 `home`），传输台连上之后比 SFTP 起始目录：不一致 ⇒ 一个字节不传、`transfer` 帧以 `{"state":"failed","code":"sftp_home_mismatch"}` 收场，窗口改走上面的块形并出声一次。
⚠ 暂存区与目标不在同一个文件系统上 ⇒ 改名回 `EXDEV`、`io_failed`（复制 ＋ 删那一形在第三层禁表里，没做）。
- **CLI 面同样有它**（从命令注册那一处派生，与写面同一条理由）：`--files-commit-upload`，载荷走 stdin，与帧面的 `args` 同形。

#### `files-stage-chunk`：存盘的一块进暂存区（2026-09-24）

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

#### `files-commit-text`：按块读回、拼起来、覆盖写（2026-09-24）

```text
→ {"id":"w9","cmd":"files-commit-text","args":{"key":"0123456789abcdef0123456789abcdef","chunks":3,"bytes":3000000,"root":"/home/u/docs","rel":"big.log","expect":{"sha256":"…"}}}
← {"kind":"reply","id":"w9","ok":true,"data":{"path":"/home/u/docs/big.log","bytes":3000000,"sha256":"…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 与 `files-stage-chunk` 同一个键 |
| `chunks` | → | 块数：读回 `0..chunks` 这几块。只收 `1..=bytes`（每块至少 1 字节） |
| `bytes` | → | 拼起来**必须恰好**这么长；最多 8 MiB（`files-read-text` 一趟的天花板：存得回的要读得回来），超了 ⇒ `bad_args` |
| `root` / `rel` | → | 目标，语义与 `files-write-text` **完全相同**：必须已经在、是普通文件；跟链接（解到底再判一次）；原子地换（权限位沿用；属主 / 硬链接不再保留，见 `files-write-text`） |
| `expect` | → | 🔴 **必须给**，与 `files-write-text` 的 `expect` 同形同义（摘要形 CAS）：盘上那份对不上 ⇒ `stale`，目标一个字节没动（块照样删掉） |
| `path` | ← | 解到底的那个真路径 |
| `bytes` | ← | 写进去的字节数 |
| `sha256` | ← | 写进去那份的摘要 |

⚠ 少一块 · 多出第 `chunks` 块 · 总长对不上 ⇒ `io_failed`，目标**一个字节没动**；某一块是链接或目录 ⇒ `refused`。
⚠ **不论成败**都删掉这一次的块；成功时顺手扫一遍暂存区的孤儿（`<key>.part` 与 `<key>.<seq>.chunk` 两种形状，7 天没动过的）。
⚠ 为什么不是「改名上位」（`files-commit-upload` 那种）：改名会换掉权限位 / 属主、把链接本身换成普通文件、跨盘时 `EXDEV` 失败 —— 同一份文件 1 MiB 上下会存出两种结果。
- **CLI 面同样有它**：`--files-commit-text`，载荷走 stdin，与帧面的 `args` 同形。
#### `files-copy`：同根内复制一份普通文件（2026-09-24）

写面第七条。文件窗口的「复制」此前走 SFTP 那条零流量复制；现在经通道问后端 —— 复制发生在
那台机器上、字节不过网，**「退回经本机转发、花 2× 流量」那一形从此不存在**。

```text
→ {"id":"w7","cmd":"files-copy","args":{"root":"/home/u/docs","from":"a.md","to":"a.md.copy"}}
← {"kind":"reply","id":"w7","ok":true,"data":{"path":"/home/u/docs/a.md.copy","bytes":1234}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `root` | → | 目标根（与写面其余几条同形） |
| `from` / `to` | → | 两个相对段。`from` **解到底**再过路径解析（复制是一次跟链接的读：根里一条指向根外的链接不许借它把根外那一份复制进来）；`to` 只解父目录（作用在链接本身上） |
| `overwrite` | → | 🔴 **覆盖策略显式**。不给 / `false` ⇒ `O_EXCL` 直接开目标，目标已在（含它只是一条链接）⇒ `io_failed`、一个字节不动；`true` ⇒ 先写进同目录一个暂存旁名（`O_EXCL`，它自己也过围栏），写满再换名上位 —— **原子地**顶掉目标（顶掉的是链接本身，不跟过去）。不是布尔 ⇒ `bad_args` |
| `recursive` | → | **复制目录显式**。不给 / `false` ⇒ 只复制普通文件（射程与此前一个字节不差）；`true` ⇒ 复制整棵（见下）。与 `overwrite: true` 同给 ⇒ `bad_args`（目录复制不合并、不覆盖）。不是布尔 ⇒ `bad_args` |
| `path` | ← | 落点（父目录解完 symlink 的） |
| `bytes` | ← | 复制了几个字节（整棵时是全部普通文件之和） |
| `files` / `dirs` | ← | 复制了几个普通文件 / 几个目录（单文件那一形恒是 `1` / `0`） |
| `links` | ← | 照原样复制了几条符号链接（单文件那一形恒是 `0`） |

- 不带 `recursive` **只收普通文件**：源是目录 ⇒ `refused`。源与目标是同一份 ⇒ `refused`。
- **`recursive: true`（复制目录）照 `files-delete` 递归删的形状，两趟**：
  ① 计划（只读）：源解到底一次，不跟链接地走整棵，**每一条目过一次路径解析**；树里有设备 / 管道 / 套接字 ⇒ 整趟 `refused`；符号链接复制**链接本身**（记下目标文本、原样重建，不跟进去；= `cp -R` 缺省的 `-P`；非 unix 照旧整趟 `refused`）；
  跨挂载点 ⇒ 整趟 `refused`；条目数超过上限（与递归删同一个 `TREE_ENTRY_CAP`）⇒ 整趟 `refused`；目标在源里面 ⇒ `refused`。计划被拒 ⇒ 一个字节都没建。
  ② 执行（先序）：每一条源与目标**当场再过一次**路径解析、核源的种类没变；目录 ⇒ 建目录（目标已在 ⇒ `io_failed`：不合并）；文件 ⇒ `O_EXCL` 新建、写满、抄权限位；
  全部落完后目录的权限位倒序抄。**中途失败 ⇒ 倒序删掉这一趟自己建的**，话里说停在第几条、撤掉了几条（撤不干净就点名那一条）。
- 两支都先写同目录暂存旁名，再上位（不覆盖 ⇒ 不覆盖改名：写的中途目标那一格冒出来也不盖、`io_failed`；目标那一格只会整份出现）。写到一半失败 ⇒ 删掉**这一趟自己刚建的旁名**，原样带回原因（`io_failed`）。
- 新文件的权限位**从源抄**（unix 是 mode 低 12 位，别处是只读位）；覆盖时目标换成源的权限位。抄不上 ⇒ 删掉自己刚建的那一份、`io_failed`。
- ⚠ 同写面其余几条：判定与动手之间的窗（TOCTOU）没闭合；真远端那一维没有读数（本机文件系统上跑过）。
- **CLI 面同样有它**（`--files-copy`，载荷走 stdin），理由与写面其余几条相同。

**错误码**：`bad_path` · `bad_args` · `refused`（路径解析拦的 / 源不是普通文件 / 源即目标 / 树里有链接等 · 跨挂载点 · 超上限 · 目标在源里面）· `io_failed`（盘上没成：目标已在且没说覆盖 / 读写中断 / 换名失败 / 抄权限位失败 / 复制目录中途停下）。

#### `files-extract`：解压到一个新目录（2026-09-27）

右键「解压到这里」，由那台机器的后端用 Rust 库（`tar` · `flate2` · `zip`，不调外部命令）解。

```
→ {"id":"x1","cmd":"files-extract","args":{"root":"/home/u/dl","rel":"pkg.tar.gz"}}
← {"id":"x1","ok":true,"data":{"path":"/home/u/dl/pkg","files":12,"dirs":3,"links":1,"bytes":40960}}
```

| 字段 | 方向 | 说明 |
|---|---|---|
| `root` | → | 那个目录（字符串或 `{"b16": …}`） |
| `rel` | → | 包在 `root` 下的相对段；格式按名字后缀认：`.zip` · `.tar` · `.tar.gz` · `.tgz`（不分大小写），其余 ⇒ `unsupported`（「不认这种包」） |
| `fresh` | → | 可缺席的布尔。落点 ＝ 包所在目录下「包名去后缀」那个**新**目录（名字规矩只住后端）；它已在 ⇒ `exists`、一个字节不动（窗口据此**问人**）。问过之后带 `fresh: true` ⇒ 取第一个不在的 `名 (2)` `名 (3)` …（Nautilus / Finder / Explorer 撞名时的写法） |
| `path` | ← | 落点目录（父目录解完 symlink 的） |
| `files` / `dirs` / `links` / `bytes` | ← | 建了几份文件（含硬链接落成的拷贝）/ 几个目录（含补出来的上级）/ 几条符号链接 / 文件字节之和 |

- **两趟**（同 `files-copy` 的 `recursive`）：① 计划（只读）逐条目判：名字只许普通段（`..` · 绝对路径 · 盘符 ⇒ 整趟 `refused`；`.` 与空段剥掉；zip 里的 `\` 当分隔符）；
  符号链接的目标必须相对、从它所在目录起算不出落点；硬链接只许指向包里前面的普通文件（落成一份拷贝）；设备 / 管道 ⇒ 整趟 `refused`；
  条目挂在包里一条链接或一份文件底下 ⇒ 整趟 `refused`；同名文件两次 ⇒ 整趟 `refused`；条目数超过 `TREE_ENTRY_CAP` ⇒ 整趟 `refused`。计划被拒 ⇒ 一个字节都没建。
  ② 执行：建落点目录 → 目录 → 文件（`O_EXCL` 新建、写满、权限位只取 rwx 低 9 位）→ 硬链接的拷贝 → 符号链接（最后建）；每一条当场过路径解析。
  **中途失败 ⇒ 倒序撤掉这一趟自己建的（含落点目录）**，话里说撤干净没有。
- 包坏了 / 用了不认的压缩或加密方式 ⇒ `io_failed`（带库的原话）。
- ⚠ 不设解压字节上限（只有条目数上限）；修改时间 · 属主 · 扩展属性 · setuid / setgid / 粘滞位都不还原。真远端那一维没有读数（本机文件系统上跑过）。
- **CLI 面同样有它**（`--files-extract`，载荷走 stdin），理由与写面其余几条相同。

#### `files-link`：建一条符号链接（**写用户文件**）

写面闭集里 FILES2 那个「建链接」动词（`control/files_extract.rs::land_link`，复制目录与解压共用）的帧面入口 —— 只补这一个入口，不另起原语。
账号库管理（`accounts-init` / `accounts-add` / `accounts-repair`）在进程内经它建各号链回共享库的那几条链接。

```
→ {"id":"l1","cmd":"files-link","args":{"root":"/home/u","rel":".cc-monitor/accounts/z/skills","target":"/home/u/.claude/skills"}}
← {"id":"l1","ok":true,"data":{"path":"/home/u/.cc-monitor/accounts/z/skills"}}
```

| 字段 | 方向 | 说明 |
|---|---|---|
| `root` · `rel` | → | 链接自己那条路径：`rel` 在 `root` 下过路径解析（同写面其余几条；字符串或 `{"b16": …}`） |
| `target` | → | 链接的目标文本，**原样**写进去（不解、不判，同 `cp -P`） |
| `path` | ← | 建出来的那条链接（父目录解完 symlink 的） |

- 那儿已经有东西（含一条链接）⇒ 系统拒（`io_failed`），一个字节不动；**不先删再建**。
- 非 unix ⇒ `refused`（没有这个动词）。错误码：`bad_args` · `bad_path` · `io_failed` · `refused`。
- **CLI 面同样有它**（`--files-link`，载荷走 stdin），理由与写面其余几条相同。

#### `files-peek`：读改写的**读那一半**（2026-09-24）

只有后端的文件管理部分写**用户的文件**，本机也算 ⇒ monitor 进程不再直接写用户文件
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

#### `files-put`：整份替换一份文本文件（2026-09-24）

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

- 替换沿用原文件的权限位；新建的文件是后端进程的缺省（受 umask）—— 落在这台机器 `~/.cc-monitor` 里的除外：
  那里新建的文件 `0600`、补出来的目录（含 `~/.cc-monitor` 本身）`0700`（unix；`files-create` · `files-mkdir` 同一条）。
- 回读不符 ⇒ 回滚（原来在 ⇒ 原文换回去；原来不在 ⇒ 删掉刚建的），`io_failed` 的话里说清恢复成没成。
- ⚠ CAS 之后、换名之前那一个窗（TOCTOU）：`expect: null` 那一形已闭合（不覆盖上位，窗里冒出来的那一份不盖、回 `stale`）；`expect` 有值那一形没闭合 —— 窗里被别人改了，那次改动会被盖掉。CAS 缩小的是「monitor 读 → 后端写」那一整趟往返的窗。
- **CLI 面同样有它们**（从命令注册那一处派生）：`--files-peek` · `--files-put` · `--files-delete-session`，载荷走 stdin。

**错误码**：`bad_path` · `bad_args` · `refused`（围栏拦的 / 不是普通文件）· `io_failed` · `stale`（读改写之间盘上那份变了）。

#### `files-delete-session`：删一份历史会话 —— **只收 sid**（2026-09-24；原标题「会话文件围栏唯一的例外」）

```text
→ {"id":"p3","cmd":"files-delete-session","args":{"sid":"2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77"}}
← {"kind":"reply","id":"p3","ok":true,"data":{"path":"/home/u/.claude/projects/-home-u-x/2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77.jsonl"}}
```

历史浏览器里「删掉这场会话」走这一条，本机与远端同一条路（此前本机是 monitor 进程直删、远端是 SFTP 直删）。
此前这里写「写面其余每一条都被会话文件围栏挡在 `projects/<proj>/<sid>.jsonl` 外面，这是唯一能删它的一条」——
文件管理写面今天也删得掉会话文件；这一条的独特之处只剩「只收 sid、落点由后端按 sid 找、必须是会话记录的形状」。

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 🔴 **只收 sid**：多给任何一个键 ⇒ `bad_args`。落点由后端在它自己的记录树里按 sid 找（与分叉那条同一份「找」），解到底必须恰是 `<项目>/<sid>.jsonl` 且在记录树里 |
| `path` | ← | 删掉的那一份 |

**错误码**：`bad_args` · `refused`（sid 形状不对 / 没找到 / 解完链接跑出记录树 / 不是那一形）· `io_failed`。

#### 「退出行为」那个值—— **值住后端所在那台机器**

那个值（「最后一个客户走了之后，这台机器的后端退不退」）住后端所在那台机器的 `~/.cc-monitor/backend.json`，
**只有后端写**。前端要读要改都经下面两条命令；monitor 自己的 `config.json` 里**不再有它**，
monitor 进程内也**不再有它的副本**（原来那条「启动时 / 改动时把生效值推给 monitor」的 tauri 命令整条退役）。

两条命令回同一个形状（原来还有一格 `shell`，恒 `"standalone"`；「折进前端进程」那一档已放弃，那一格随之删掉）：

| 字段 | 向 | 说明 |
|---|---|---|
| `state` | ← | 三态：`"chosen"`（有人选过）· `"absent"`（文件不在 = 没人选过）· `"unreadable"`（文件在但读不出来 / 家目录解析不出来）。🔴 后两态**不许合并** —— 「读不出来」不等于「有人选了默认」 |
| `killOnExit` | ↔ | 生效值。`chosen` 时是选的那个；另两态是缺省 `false`（不结束）。`exit-policy-set` 的入参也是它 |
| `reason` | ← | 只在 `unreadable` 时有：为什么读不出来。其余为 `null` |
| `path` | ← | 那份文件的绝对路径（家目录解析不出来时 `null`） |
| `said` | ← | 成品：「这台退出时会发生什么」那一句（四档：读不出来 · 会结束 · 常驻继续跑、无人监护 · 被监护随 monitor 退）。后端按自己是不是回环常驻判（与选载体同一个 `listen::mode_from`）；界面原样摆，不再判 |

#### `exit-policy-read`：现读一次（**不读 stdin**）

```text
→ {"id":"x1","cmd":"exit-policy-read"}
← {"kind":"reply","id":"x1","ok":true,"data":{"state":"absent","killOnExit":false,"reason":null,"path":"/home/u/.cc-monitor/backend.json","said":"monitor 退出后它继续跑，无人监护：崩了不会自动重起；下次开 monitor 会接上它，接不上才起一个新的"}}
```

**没有错误码**：读不出来是一个**状态**，照样 `ok:true` 回 `state:"unreadable"` ＋ `reason`。
每一次都真去读盘，没有缓存（用户可能刚从另一台 monitor 改过它）。

#### `exit-policy-set`：写那个值，写完读回

```text
→ {"id":"x2","cmd":"exit-policy-set","args":{"killOnExit":true}}
← {"kind":"reply","id":"x2","ok":true,"data":{"state":"chosen","killOnExit":true,"reason":null,"path":"/home/u/.cc-monitor/backend.json","said":"monitor 退出时会结束它"}}
```

回的是**写完之后再读一遍**的那一份（盘上的事实，不是「我以为写进去了」）。
写法：`O_EXCL` 新建一份临时文件 → 写满 → 原子挪到目标上（读者只会看到整份旧的或整份新的）；
`~/.cc-monitor` 不在就建那一层。

**错误码**：`bad_args`（`killOnExit` 缺了或不是布尔）· `io_failed`（家目录解析不出来 / 盘上没写成）。

⚠ **CLI 面也有它们**（`--exit-policy-read` / `--exit-policy-set`），同 `files-create` 那一条理由：CLI 面从 `inbound::REGISTRY` **派生**，不是选的。

**谁按它动手**（两处，都是「最后一个客户走了」那一刻现读）：
① 常驻（脱离）那条载体上，那一条流断了 ⇒ 后端自己现读、`true` 就退出；
② monitor 退出时现问一次本机后端（`exit-policy-read`），按答案收它自己起的那两个子进程（被监护的后端 ＋ 本机中转）。
⚠ `lingerMs`（归零后等一下再决定）**没做**：那是一个会自己醒来的构件，后端零定时器铁律不放行，已上报。
⚠ **远端**那一行的值写得进去、读得回来，但今天**没有任何进程按它动手**：远端后端只走 stdio（SSH exec），
它没有「最后一个客户走了」那一臂，SSH 一断它本来就随管道破裂退出。

#### 上游选择那份凭据文件在「这台机器」上的读写（2026-09-24）—— **上游选择自己的状态，不是用户文件**

第三方 API key 那份文件（`apikey-credentials.json`）归**上游选择**：名字、格式、落点都是本仓定的，
只有中转进程里的上游选择读它 ⇒ 它**不走**文件管理那一面（那一面是给用户文件的），
写口登记在后端 `readonly_guard` 的**第四层**（后端自有状态文件），只从下面 `apikey-key-set` 一条进来。
判清的全文住。

- **每台机器上的程序写者恰好一个**：**那台的后端**（本节两条）—— monitor 所在那台也一样，是本机常驻后端。
界面经 `chan.call(这台, "apikey-key-set", …)` 直接交给那台的后端（本机 ＝ `<local>` 那条长连接）；monitor 只转不透明的字节。
  「本机后端写的那份就是这个 monitor 用的那份」由**家**保证：那份文件住家里（`<家>/apikey-credentials.json`），常驻载体接上之前 hello 的
  `host_env` 已与 monitor 要交的家两向比过（`local_backend_host.rs::hello_verdict`），被监护的 stdio 载体是 monitor 按同一份环境起的。
  〔GP1 那一版：monitor 发之前先问一次 `apikey-read` 核 `path`；RM1a 那一版：「monitor **从不**把 `apikey-key-set` 发给本机那条连接」。〕
- **路径**与那台机器上中转里的上游选择**同一个出处**（`accounts::upstream_select::creds::resolve_path` ＋ 同一个家目录）。
- 🔴 **明文只在 `apikey-key-set` 的 `args.key` 里**：不进 argv、不进 env、不进任何日志；两条的应答都只有**掩码**。
- 两条都**不起中转**；中转那两条（`relay-*`）也**不碰凭据**。

#### `apikey-key-set`：给一个账号写 key，写完读回

```text
→ {"id":"k1","cmd":"apikey-key-set","args":{"configDir":"/home/u/.cc-monitor/accounts/work","key":"<明文>","baseUrl":"https://api.example.com"}}
← {"kind":"reply","id":"k1","ok":true,"data":{"account":"work","path":"/home/u/.cc-monitor/apikey-credentials.json","masked":"sk-a****wxyz","baseUrl":"https://api.example.com"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `configDir` | → | 这个号的账号目录。账号 id 由**这台后端**按全仓唯一那份规则推（`acct_core::apikey_account_id_of_dir`，起会话那一侧同一个）；推不出 ⇒ `bad_args`。还给旧的 `account` ⇒ `bad_args`（不为旧形状留兼容） |
| `account` | ← | 推出来的账号 id。必须当得了路由段 —— 与上游选择装表时**同一个谓词**，写得进去却装不进表 = 那一行永远 404 |
| `key` | → | 明文。空串拒 |
| `baseUrl` | ↔ | 入（可选）：这个账号的第三方端点。缺席 / `null` / 空串 = **不碰那一格**（只配 key 时已有端点原样留着）；给了就先过**那一条**判定（`upstream_url_core::usable`，与上游选择装表同一个：形状 ＋ 明文只许回环；写者只剩后端这一处），不对 ⇒ `bad_args`、整次不写。出：写完读回这一行的端点（没有 ⇒ `null`） |
| `masked` | ← | 写完**再读一遍**、这一行 key 的掩码（盘上的事实） |
| `path` | ← | 那份文件的绝对路径 |

写法：**写的那一刻读盘** → 只改 `accounts.<account>` 的 `api_key`（与给了的 `base_url`）那一两格（别的行与未知键一个不动）→ 临时文件**出生即只给本人**（O_EXCL）→ 写满 → 落盘 → 原子改名；失败删自己的临时文件。
**错误码**：`bad_args`（缺字段 / 推不出账号 id / 账号 id 当不了路由段 / key 空 / 还给了 `account`）· `bad_file`（现有文件解析不了 ⇒ **不覆盖**，人手编的内容不许被抹掉）· `io_failed`。

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
先前还回 `rows`（表里有哪几行）：它只给 monitor 起会话那一侧用，而那一侧的判断归起 agent 那台的 `ccm` 了 ⇒ 退出线上。

⚠ **CLI 面也有它们**（`--apikey-key-set` / `--apikey-read`），从 `inbound::REGISTRY` 派生；`--apikey-key-set` 的入参**从 stdin 读**。

#### `apikey-routing`：这几个号在这台的表里有没有行 · 这台的中转在不在（US1 · 4D）

```text
→ {"id":"k3","cmd":"apikey-routing","args":{"agent":"claude-code","configDirs":["/h/.cc-monitor/accounts/work","/h/.cc-monitor/accounts/me"]}}
← {"kind":"reply","id":"k3","ok":true,"data":{"routed":["/h/.cc-monitor/accounts/work"],"running":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 这一家 agent 的路由名（凭据文件的行只属于 `claude-code`；别家 ⇒ `routed` 恒空）。空串 ⇒ 默认那一家；注册表里没有 ⇒ `bad_args`，那句话列出认得的几家 |
| `configDirs` | → | 要问的那几个号的配置目录（id 由后端按 `acct-core` 那一份规则推，前端一个字都不推）|
| `routed` | ← | 传进来的里面、**表里有对应行**的那几个（原样回）。「表里有行」= 上游选择装表真收进表的那几行（`base_url` 坏的那一行不算）|
| `running` | ← | 这台机器上**我们的**中转在不在听（读常驻后端进程内的监听状态；中转住这里）|

**错误码**：`bad_args`。界面经 `chan.call` 直接问（金样同上）。

#### `relay-optin`：直接敲的 claude 也走中转（可选、生成让你贴）

这台那份用户级设置文件（Claude 是 `~/.claude/settings.json`，各号都链回它）里写没写上游地址、是不是这台中转现在那一条，并给要合并进 `env` 的那一段。
**只读**：那份文件由用户自己合并，后端一个字节不写。该贴的那一条 = 决策表（`decide_launch`）里「没表态是哪个号」那一发（`/t/<agent>/_`，全量注入按「是」）插上这台盘上那把中转钥匙。
只在流面上有（「中转在不在」读本进程的监听状态，一次性进程里恒「不在」）。

```text
→ {"id":"k5","cmd":"relay-optin","args":{}}
← {"kind":"reply","id":"k5","ok":true,"data":{"state":"stale","note":"","missing":"","source":"/h/.claude/settings.json","snippet":"{\n  \"env\": {\n    \"ANTHROPIC_BASE_URL\": \"http://127.0.0.1:8788/<钥匙>/t/claude-code/_\"\n  }\n}","listening":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `state` | ← | `installed`（写着的就是现在那一条）· `stale`（是我们那一形 —— 回环 ＋ 钥匙段 ＋ 我们的路由 —— 但钥匙 / 端口 / 路由不对了）· `absent`（文件不在，或没写 / 写了空串）· `other`（写了别的上游地址）· `unreadable`（读不了 / 读不懂；**不当没装**）|
| `note` | ← | 那份文件为什么读不了（`unreadable` 才有，其余空串）|
| `missing` | ← | 那一段为什么生成不了（这台的中转还没起来过、没有钥匙 · 决策表不给这一条）；已装 / 生成得了 ⇒ 空串 |
| `source` | ← | 读的是哪份文件（这台后端看到的路径）|
| `snippet` | ← | 要合并进 `env` 的那一段（**带钥匙**：设置文件里写不了 `$(cat …)`）；`installed` 或生成不了 ⇒ `null` |
| `listening` | ← | 这台我们的中转此刻在不在听（与 `apikey-routing.running` 同一个判准）|

**错误码**：`failed`（这台没有 HOME / 没有哪一家有这一形）。界面经 `chan.call` 直接问（金样 `tests/__fixtures__/relay-optin.golden.json`）。

#### `launch-render-cli`：远端起会话那一行 `ccm …`

monitor 每一条远端起会话路径（直连 resume · 建 tmux 会话 resume〔换号重启 · 分叉〕· 开新会话 · 接回 · 就地 resume）都问那台后端要这一行；
交给终端的**只是这一行**（就地 resume 回落那一形外层只包一层 `tmux send-keys … ; tmux attach`）。环境、中转地址、身份标记由**那台的 `ccm`**
在最终 exec 那一处定。`ccm` 就是这台后端本身 ⇒ 能力问它自己（与 `--ccm-probe` 同一份）。**纯函数**（不起进程、不碰盘）。
（原 monitor 的 Tauri 命令 `render_ccm_launch`〔散文墓碑〕搬进那台后端；载荷那一条随起会话只交一行 `ccm …` 删了。）

```text
→ {"id":"c1","cmd":"launch-render-cli","args":{"agent":"claude","action":{"kind":"resume","sid":"s1"},"container":{"kind":"tmux","name":"cc-s1","send_into":false},"cwd":"/p","account":{"kind":"account","name":"z","configDir":"/home/u/.cc-monitor/accounts/z"},"ccmSid":"s1","model":null,"launcher":"claude","defaultLauncher":"claude"}}
← {"kind":"reply","id":"c1","ok":true,"data":{"cmd":"ccm --resume s1 -- --ccm-tmux=cc-s1 --ccm-sid=s1 --ccm-agent claude --account z --cwd /p"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | **必填**：这个会话是哪一家（`claude` / `codex`）。resume 按那一家的写法渲（`--resume <sid>` · `resume <sid>`），`--ccm-agent <kind>` 恒显式写出（接回不写）；认不出 ⇒ `refused` 并列出认得的几家；那一家没有账号这一维却给了具名账号 ⇒ `refused` |
| `action` | → | `{"kind":"new"}` · `{"kind":"resume","sid":…}` · `{"kind":"attach","name":…}` |
| `container` | → | `{"kind":"none"}`（直路）· `{"kind":"tmux","name":…,"send_into":bool}`（建会话 / 键进已有 pane）|
| `cwd` · `ccmSid` · `model` · `launcher` · `defaultLauncher` | → | 这次拉起的修饰（`deny_unknown_fields`：多送一格就拒）|
| `account` | → | `{"kind":"base"}` · `{"kind":"account","name"?:…,"configDir"?:…}`（说得出名字 ⇒ `--account`；只有目录 ⇒ `--account-dir`）|
| `cmd` | ← | 那一行 `ccm …` |

**错误码**：`bad_args`（入参形状不对）· `refused`（渲不出来：缺能力 · 说不出的修饰 · 坏值，理由原样；调用方**不回落、不拼第二条**）。
CLI 面（`--launch-render-cli`）从 `inbound::REGISTRY` 派生，入参从 stdin 读。

#### `launch-local`：本机起会话那一行 `ccm …`

本机那几形（新起 · resume · 接回）。回的是要在**本机一个新终端窗口里跑的那一行**；开窗口是 monitor 的事（`open_local_terminal`）。
（原 monitor `history.rs` 的 `new_local_session` / `resume_history_session` / `render_local_attach`〔散文墓碑〕搬进本机后端。）
POSIX 上有会话名 ⇒ `--ccm-tmux=`（建进 tmux）；Windows 没有 tmux ⇒ 直路（`ccm` 在那个 PowerShell 窗口里起 agent），接回说不出 ⇒ 拒。
起 agent 的那几形带 `--ccm-launch-id <token>`（resume 用 sid，新起现铸一个 nonce）：`ccm` 把它放进 agent 进程环境（`CCM_LAUNCH_ID`），调用方拿它回填新会话的 sid。

```text
→ {"id":"l1","cmd":"launch-local","args":{"agent":"claude","action":{"kind":"new"},"cwd":"/w","launcher":null,"account":null,"tmuxName":"w-cc","defaultLauncher":"claude"}}
← {"kind":"reply","id":"l1","ok":true,"data":{"cmd":"ccm -- new --ccm-tmux=w-cc --ccm-agent claude --ccm-launch-id …","launchId":"…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | **必填**，同 `launch-render-cli` |
| `action` | → | `{"kind":"new"}` · `{"kind":"resume","sid":…}` · `{"kind":"attach"}`（接回 `tmuxName` 那个会话，不起 agent）|
| `cwd` | → | 只用来核「新起」那一格的目录在不在 |
| `launcher` | → | 自定义启动命令（空 = 没设）|
| `account` | → | 缺席 / `null`（继承）· `{"kind":"base"}` · `{"kind":"named","configDir":…,"name"?:…}` |
| `tmuxName` | → | 建进 tmux 时的会话名（界面铸名口铸的，这里不铸）；缺 ⇒ 直路 |
| `defaultLauncher` | → | 这一家 agent 的默认启动器（等于它就不吐 `--launcher`）|
| `cmd` | ← | 那一行 `ccm …` |
| `launchId` | ← | 交给 `ccm` 放进 agent 进程环境的身份 token；接回那一格 `null` |

**错误码**：`bad_args` · `refused`（坏输入 · 目录不在 · Windows 上接回）。只上流面（`cli_control::STREAM_ONLY`：回的 token 要交回界面）。

#### `ccm` 在最终 exec 那一处定的几样

- **中转地址**（`ANTHROPIC_BASE_URL`）：问上游选择（`accounts/upstream_select/endpoint.rs::decide_launch` 是唯一实现）。
  全量注入开关读 `ccm` 自己的环境 `CCM_RELAY_ALL_SESSIONS`（默认开，`0` 关）；中转口读 `CCM_RELAY_PORT`，缺省是那个固定常量。
  「这台的中转在不在听」= 钥匙文件读得到 ＋ 回环口连得上（`ccm` 是一次性进程，读不到常驻后端的监听状态 —— 口被别人占着时会被认成在听）。
  非它不可（API 号代入 `/s/`）而没在听 ⇒ **拒绝起会话**、一句话说清；`/t/` 那一格没在听 ⇒ 这一发直连。
  钥匙在 `ccm` 进程里拼进 agent 的环境，不进 argv、不进 shell；用户自己设了 `ANTHROPIC_BASE_URL` ⇒ 不注入、说一句。
  环境里继承来的是**我们的中转那一形**（外层 shell / 上一趟留下的，属于别的号）⇒ 不认：按**这一发的目标账号**重问；
  这一发不注入 ⇒ 清掉它。容器路（`--ccm-tmux`）不把它带进 pane —— pane 里那一趟走到这里自己问（只带用户自己的端点）。
- **身份**：`--ccm-launch-id` ⇒ `CCM_LAUNCH_ID`；`--ccm-sid=` ⇒ tmux 会话上的 `@ccm_sid`。
- **账号**：`--account <名>` · `--account-dir <目录>`（说不出名字时）· `--base`。
- 同号会话**有活着的 agent 进程**才回接那个窗口；只剩窗口上的标记（进程已退）⇒ 原地续上。

#### 中转在「这台机器」上的进程（2026-09-24）

本机的中转住**本机常驻后端进程里**（monitor 起本机后端时交 `CCM_RELAY_PORT`，见 `--relay` 那一条下的「进程内」一格）；
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
| `listening` | ← | 口上有人在听，**而且是我们的中转**：读这台机器上的中转钥匙文件（`~/.cc-monitor/relay-key`），带对的钥匙打一次 `GET /<钥匙>/` 得 404、带一把同形错钥匙得 403 才算。口上有人、而这台还没有钥匙文件 ⇒ `false`（刚起的中转在「绑上口」与「钥匙落盘」之间的窄窗）。⚠ 钥匙**不出线** |

**错误码**：`bad_args`（`port` 缺了或不在 1–65535）· `not_ours`（口上有人、有钥匙文件，但差分探针对不上 —— 多半是升级前起的旧中转或别的程序；`message` 说出两次各得了什么）。

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
**错误码**：`bad_args` · `not_ours`（口上有人，但不是我们的中转 —— 含「有人在听而这台没有钥匙文件」；不起、不抢口）· `spawn_failed`（找不到自己 / 起不动）· `unsupported`（非 unix：不知道怎么起成脱离的一组，没起）。

起出来的中转**绑上口之后**读回或铸这台机器上的钥匙（`~/.cc-monitor/relay-key`，`0600`），之后每条请求路径的第一段必须是它（`INVARIANTS §48.1`）。

⚠ **CLI 面也有它们**（`--relay-status` / `--relay-ensure`），入参从 stdin 读。
这里原是帧面 `relay-status` / `relay-ensure`（远端那台上起一个脱离的 `--relay`）：中转只住常驻后端进程里（本机远端同形），那一族随远端回落一形删了。

#### `drift-report`：这台后端的漂移账（**不读 stdin**，只读、按需）

```text
→ {"id":"q30","cmd":"drift-report"}
← {"kind":"reply","id":"q30","ok":true,"data":{"faces":[{"face":"unknown_record_type","consequence":"…","entries":[{"key":"mode","count":3,"first_sample":"{…}"}],"overflowed":false}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `faces` | ← | 各家记录解释面记下的「看不懂的记录」：`face`（`unknown_record_type` · `known_type_parse_failed`）· `consequence`（看不懂时会发生什么）· `entries`（按次数降序：`key` · `count` · `first_sample` 首见样例、截断）· `overflowed`（键数触顶，新键并进 `<overflow>`）。本进程内计数、不落盘 |

记录解释搬进后端之后，看不懂的那一刻在场的是**这台后端**；monitor 天生观测的两面（未登记的会话 `kind` · `hello` 里不认识的能力 token）仍记在 monitor 那本（`drift_ledger_report`）。

#### `footprint-report`：「足迹」由这台后端出整份成品（2026-09-28，**只读**）

设置里「足迹」那一块（cc-monitor 在这台机器上碰过哪些文件、app 要的东西齐不齐）：申报表与判定（哪一行属于哪个工具、
存在 / 缺失 / 查不动怎么分）都在后端（`footprint/`，Claude 布局那一半在 `agents/claudecode/footprint.rs`），这台 stat 这台自己的盘。
〔墓碑 —— RM1a 那一版是 `footprint-probe`：后端只交路径事实，判定住 monitor，monitor 问两趟。〕

```text
→ {"id":"f1","cmd":"footprint-report","args":{}}
← {"kind":"reply","id":"f1","ok":true,"data":{"rows":[…],"settings_scopes":[…],"claude_config_dir":"/home/u/.claude","home":"/home/u"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `client` | → | 可选。**本机那一栏**才带：monitor 自己进程独有的几条事实 `{home, path?}`（`HostScope::Client` 那一族按它们解，agent 家用这台后端自己解析的那一个；本机后端与 monitor 同一台、同一用户 ⇒ stat 仍由这台做）。不带 ⇒ 远端那一栏：住 monitor 那台的那一族不进人群 |
| `data` | ← | 整份报告：`rows`（每行 `tool_id` · `tool_name` · `tier` · `source_label` · `path_declared` · `path_resolved` · `note` · `host_label` · `effect_label` · `state{kind: present\|absent\|undetermined\|expected_absent, detail?\|why?}` · `installable` · `uninstallable`）· `settings_scopes` · `claude_config_dir` · `home` |

- `expected_absent`（带 `detail`）＝ 该不在、确实不在：旧版遗留那一档（认出是我们放的就删）不在，正是该有的样子 —— 结论由后端给（`footprint/rows.rs::read_absence`），界面照档画、不算缺口。
- 目录最多列 4096 个名字（超了 ⇒ 列不动，**不截断**）· 查 cc-bus 钩子字样的文件最多 1 MiB，**文件内容一个字节都不回**。
- 环境是这个**后端进程**的（`HOME`，没有再退 `USERPROFILE`；`PATH`）—— 用户交互 shell 的 rc 改过的环境这里看不见。

**错误码**：`bad_args`（`client` 形状不对 / 相对路径）· `failed`（这台后端进程没有家目录）。
⚠ **CLI 面也有它**（`--footprint-report`），入参从 stdin 读。

#### `ccm-print`：一条别名实际会执行什么（2026-09-25，**只读**）

设置里「别名」那一块的预览：一条别名的预置参数交过来，回 `ccm --print` 的那一行 ——
与终端里 `ccm --print …` 走**同一个**计划函数（`control/ccm/mod.rs::plan_of` ＋ `plan::render`），不是前端拼串。

```text
→ {"id":"p1","cmd":"ccm-print","args":{"args":["--","--ccm-tmux","--account","z","--cwd","/home/u/w"]}}
← {"kind":"reply","id":"p1","ok":true,"data":{"line":"… tmux new-session … "}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `args` | → | 一条别名的预置参数（原样 ccm argv，`<交给 claude 的…> -- <ccm 的…>` 那一形；最多 64 个、每个最长 4096 字节） |
| `line` | ← | `ccm --print` 那一行。语境写死：**这台机器家目录里的一个新终端** —— 叫的是 `ccm`、cwd = home、不在 tmux 里、不继承账号目录变量；账号表与会话快照照这台机器的真值（撞名退让与真跑同一个名字） |

**错误码**：`bad_args`（缺 `args` / 不是一组字符串 / 超上界）· `refused`（ccm 自己拒了这组参数，原话带回；或给的是 `--help` 这类不起会话的那一形）。
⚠ **没有 CLI 面**：`--ccm-print` 这个词归 ccm 的诊断口；`ccm -- --ccm-print` 在 `--` 右边，后端词与 ccm 的词不许重名。

#### `ccm-probe`：这台的 `ccm` 会哪些（2026-09-27，**只读**）

`ccm` 就是这台后端本身、恒在 `~/.cc-monitor/bin/ccm`⇒「这台 `ccm` 会哪些」问它自己，不再进交互 shell 查 `PATH`。
回的是**成品**（与 `ccm --ccm-probe` 那张名片同一组常量，线上形状由金样 `tests/__fixtures__/ccm-probe.golden.json` 钉）；界面那一问随「渲染进那台后端、能力问它自己」一起删了。纯函数：不起进程、不碰盘。

```text
→ {"id":"q1","cmd":"ccm-probe","args":{}}
← {"kind":"reply","id":"q1","ok":true,"data":{"version":"6","capabilities":["…"],"agents":["claude","codex"],"build":"p…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `version` | ← | CLI 契约版本（`CCM_VERSION`） |
| `capabilities` | ← | 这台 `ccm` 认得的能力（与名片的 `capabilities=` 行同一份） |
| `agents` | ← | 认得的 agent 种类 |
| `build` | ← | 这一份的 `BUILD_ID` |

**错误码**：无。⚠ **没有 CLI 面**：`--ccm-probe` 这个词归 ccm 的诊断口（写成 `ccm -- --ccm-probe`），同 `ccm-print`。

#### `mcp-sync-plan`：MCP 资产同步的判定（2026-09-24，**只读**）

要求：「各管各的，只有显式推 / 拉」· 推 / 拉之前先给看差异，对面有不同就问盖不盖 ·
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

#### `assets-catalog`：资产目录 —— 这台现扫一次、记下、回整份（2026-09-25）

用户原话：「比如本机后端在本机看见一个skill并记录下来, 就会和远端后端同步, 这样远端后端也能在远端装skill或者mcp / mcp保持项目级别」·「目录自动同步，装要你点」。
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
`digest` 按整条原文算（只差密钥值也判得出不同）。理由：目录是**自动**同步到每台的；「原样拷」发生在用户点「装」那一下、从来源那台现读。

**错误码**：`catalog_unreadable`（目录文件在但读不懂 / 是更新的格式 —— **不覆盖**）· `io_failed`（家目录解析不出来 / 写不进去）。
⚠ **CLI 面也有它**（`--assets-catalog`），无入参。

#### `assets-catalog-merge`：把另一台后端的整份目录并进来（2026-09-25）

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

#### `assets-sync`：本机常驻后端沿池里那条 SSH 同步资产目录（2026-09-25）

「目录自动同步」那一半：「观测方沿它本来就拥有的那条连接去拉被观测方」。只有**本机常驻后端**有意义（SSH 连接与可达表都住在它的进程里）。
一趟对一台远端：① 在池里那条连接上多开一个 exec 通道，capture `"$HOME"/.cc-monitor/bin/ccm --assets-catalog`（远端现扫、记下、回它的整份）；
② 并进本机目录（同 `assets-catalog-merge`）；③ 远端缺的 / 比远端新的那几台快照（不含远端自己那格）经 `printf '%s\n' '<json>' | <远端后端> --assets-catalog-merge` 推过去（一块 ≤ 96 KiB，单台超了那一台不推、说出来）；
④ 本机目录因这一趟变了（或开头那一次现扫发现本机自己那份变了）⇒ 对可达表里其余每台各做一趟（只一层）。**不往任何机器装东西**（装要用户点）。
不起远端的流模式（流模式会往 tmux 装指向自己 pid 的全局 hook，一个用完就退的流会把真流的 hook 盖掉）；老远端不认子命令会进流模式 —— capture 见到 hello 就收工、报「太旧」。

```text
→ {"id":"s1","cmd":"assets-sync","args":{"origin":"dev","dial":{"host":"10.0.0.2","port":22,"user":"u","key_path":"~/.ssh/id_ed25519"}}}
← {"kind":"reply","id":"s1","ok":true,"data":{"self":"9f…","synced":[{"origin":"dev","peer":"4c…","changed":true,"pushed":1,"error":null}],"reach":[{"origin":"dev","machine":"4c…"}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 可缺席。给了 ⇒ 记进可达表（内存，后端重启就空）并先对它做一趟；缺席 ⇒ 对可达表里每一台各做一趟 |
| `dial` | → | 可缺席。给了 ⇒ 那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体），记进可达表；本条会把 `use` 改成 `capture`。只给 `origin`（界面直问）⇒ 用可达表里握手那一刻 `remote-reach` 记下的那一行，没有就 `unreachable` |
| `synced` | ← | 每一趟一行 `{origin, peer, changed, pushed, error}`：`peer` 是那台目录的 `self`；`changed` 本机目录因这一趟变了没有；`pushed` 推过去几台快照；`error` 那一趟哪里没办成（`null` = 全办成了） |
| `self` | ← | 本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>` |
| `reach` | ← | 可达表 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）—— 界面据它把目录里的机器 id 对回 origin |

**错误码**：`bad_args`（`origin` 空串 · 给了 `dial` 没给 `origin` · 可达表满）· `io_failed`（本机目录开头那一次现扫没办成）· `unreachable`（只给 `origin` 而可达表里还没有那一台 —— 那台的流还没握过手）。某一台连不上 / 太旧 / 没办成**不是整条失败**，落在那一行的 `error`。
⚠ **CLI 面也有它**（`--assets-sync`，入参从 stdin 读；按派生规则「非内建即上 CLI」），但一次性进程没有常驻那一个的连接池与可达表：
它自己新拨一条 SSH、只对给的那一台做一趟，扇出恒为零台 —— 真正的用法是常驻后端的帧面。

#### `pubkey-push`：把本机公钥推进那台的 `authorized_keys`（MIG-3b 续，09-28；本机常驻后端答）

F50「一键推送公钥」（「monitor 零 SSH」）。入参同 `remote-probe`（`machine` · `saved?` · `jump?`，拨号请求在 `dial/machine.rs` 组）＋ `pubKeyPath?`：
读本机那份 `.pub`（给了就读它，否则私钥同名 `.pub`）→ 校验（恰一行非空 · 无控制字符 · 已知类型前缀 ＋ base64 主体）→
那台此刻在可达表里（长连接握过手）⇒ 问**那台后端** `authorized-keys-add {key}`；不在（密钥登录建立之前 · 后端还没装）⇒ 沿池里那条 SSH **一次** exec
（`printf '%s\n'` 不用 echo · `grep -qxF` 整行去重 · 目录 700 / 文件 600 · 末字节不是换行先补一个），只写这一件。按此刻状态分支，不是失败退回（`D11`）。

```text
→ {"id":"k1","cmd":"pubkey-push","args":{"machine":{"host":"10.0.0.2","label":"dev","user":"u","keyPath":"/home/me/.ssh/id_ed25519"},"saved":null,"jump":null,"pubKeyPath":null}}
← {"kind":"reply","id":"k1","ok":true,"data":{"outcome":"added","pubPath":"/home/me/.ssh/id_ed25519.pub","via":"exec"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `machine` / `saved` / `jump` | → | 同 `remote-probe` |
| `pubKeyPath` | → | 本机那份 `.pub` 的路径；缺席 / 空 ⇒ 私钥同名 `.pub`（两样都没有 ⇒ `refused`，界面让用户挑文件） |
| `outcome` | ← | `added`（新加的）· `already`（本就有整行相等的一行，没写） |
| `pubPath` | ← | 实际推的是哪一份（给人看） |
| `via` | ← | 走了哪条：`backend`（那台后端的文件管理面）· `exec`（那一次 exec） |

**错误码**：`invalid_args` · `bad_jump`（同 `remote-probe`）· `refused`（找不到 / 读不了 / 不像公钥）· `failed`（那台没加上：原话）。
⚠ **CLI 面也有它**（`--pubkey-push`，入参从 stdin 读；按派生规则「非内建即上 CLI」）：一次性进程没有常驻那一个的可达表 ⇒ 只走 exec 那一条。

#### `authorized-keys-add`：把一行公钥并进这台的 `authorized_keys`（MIG-3b 续，09-28；被写那台答）

`{key}` → 校验同上 → 读改写 `~/.ssh/authorized_keys`（经这台自己的文件管理面：CAS ＋ 建父目录；整行相等已有 ⇒ 不写）→ `.ssh` 700 · `authorized_keys` 600。
本机常驻后端的 `pubkey-push` 经 `remote_ask`（可达表里那台的 CLI 面）问它。

```text
→ {"id":"a1","cmd":"authorized-keys-add","args":{"key":"ssh-ed25519 AAAA… me@host"}}
← {"kind":"reply","id":"a1","ok":true,"data":{"outcome":"already"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `key` | → | 一行公钥（本条自己也校验一遍） |
| `outcome` | ← | `added` · `already` |

**错误码**：`bad_args` · `refused`（不像公钥）· `io_failed`（读 / 写 / chmod 没成：原话）。
本机后端问那台走的正是它的 CLI 面（`--authorized-keys-add`，入参从 stdin 读一行 JSON）。

#### `deploy-plan`：那台的后端要不要换、换成哪一格（MIG-3b，09-28；**只读**那台）

「部署决策进后端、monitor 只放字节」。只有**本机常驻后端**有意义（沿池里那条 SSH 问那台，同 `assets-sync`）。
一趟：① capture `uname -s -m` → 表 A（有没有产线）· 表 B（远端承不承诺）· `carried`（这一版带没带那一格）—— 拒绝点在写第一个字节之前；
② 落点 `~/.cc-monitor/bin/ccm`：SFTP stat（没有 / 0 字节就不必再问）→ capture `LC_ALL=C grep -aoE <身份戳正则> -- "$HOME"/.cc-monitor/bin/ccm`（读字节、不跑它）；
不肯说自己是谁时读回来（≤ 64 KiB，先问大小）看是不是从前那份三行入口；③ 按 `BUILD_ID` 可比序只升不降判换不换；④ 旧落点 `~/.cc-monitor/bin/cc-monitor-backend` 同法问身份，判删不删；
⑤ SFTP 列 `~/.cc-monitor/bin/`：上传留下的 `<名>.<trip>.tmp|bak`（`dial/sftp.rs::put_atomic` 的形状）一小时没动过的 ⇒ 进 `leftovers`。
**一个字节都不写**：放字节（mkdir · 原子上传 · 读回比对）与删旧落点 · 删残件是 monitor 经 `files` 链路照计划做。判定住 `control/deploy_plan.rs`（原共享 crate `deploy-core` 的判定那一半）；两侧对上的形状住契约 crate `deploy-contract`。

```text
→ {"id":"d1","cmd":"deploy-plan","args":{"machine":"dev","dial":{"host":"10.0.0.2","port":22,"user":"u","key_path":"~/.ssh/id_ed25519","use":"files"},"carried":[{"os":"Linux","arch":"x86_64","id":"p4z-x"},{"os":"Linux","arch":"arm64","id":"p4z-x"}]}}
← {"kind":"reply","id":"d1","ok":true,"data":{"os":"Linux","arch":"x86_64","label":"Linux / x86_64","expected":"p4z-x","action":"deploy","why":"…","theirs":null,"legacy":"absent","legacy_why":null,"leftovers":[],"ack":{…}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `machine` | → | 那台的名字（只用来说话） |
| `dial` | → | 那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体）；本条按需改成 `capture` / 开 SFTP |
| `carried` | → | 这一版带着的后端字节：每格 `{os, arch, id}`（`os` / `arch` 是表 A 认得的词，`id` 是那份字节自报的身份）。认不出的键 / 空 `id` ⇒ `bad_args` |
| `os` / `arch` / `label` | ← | 那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`） |
| `expected` | ← | 那一格这一版带着的字节自报的身份（对照物） |
| `action` | ← | `skip`（已是这一版）· `deploy`（没装 / 0 字节 / 更旧 / 从前的三行入口）· `keep`（另一版、不比这一版旧 ⇒ 不动它） |
| `why` | ← | 人读原因（`skip` 时空串） |
| `theirs` | ← | `keep` 时那台上那一份自报的身份，否则 `null` |
| `legacy` | ← | 旧落点那一份：`absent`（不在）· `remove`（身份戳恰一个 ⇒ 删）· `keep`（别的 ⇒ 不动）· `unknown`（连问都没问成） |
| `legacy_why` | ← | `unknown` 时的原话，否则 `null` |
| `leftovers` | ← | 落点目录里没人要的上传残件（家目录相对，排序）；列不出那个目录 ⇒ `[]`（下次连上再问） |
| `ack` | ← | 问 `uname` 那一趟拨号的 `DialAck` 原样（逐地址指纹 · 严格与否）：拨号在本机后端里，monitor 按它固化指纹（与自己开链路那几条同一个判定） |

**错误码**：`bad_args`（`carried` / `machine` 缺或认不出）· `unreachable`（`uname` 那一问没问成：链路）· `refused`（表 A / 表 B / 这一版没带 —— `message` 就是对人说的那一句）·
`io_failed`（stat 落点那一问没问成）· `undecidable`（落点那一份不说自己是谁 / 身份不唯一 / 扫不动 —— 显式失败、不覆盖，出路是机器页「卸载后端」）。
⚠ **CLI 面也有它**（`--deploy-plan`，入参从 stdin 读；按派生规则「非内建即上 CLI」）：一次性进程自己新拨一条 SSH，真正的用法是常驻后端的帧面。

#### `deploy-retired`：那台旧入口 `~/.local/bin/ccm` 的去向（THIN，09-29；远端**只读**那台）

「判定只在后端」：旧版放在远端 `~/.local/bin/ccm` 的那一份（三行 shim · 更早的 bash 启动器）
认不认得出是我们放的、删不删，由本机常驻后端判（`control/deploy_plan.rs::retired_verdict`，与 `deploy-plan` 的上传残件同一家：落点上该清的东西）；
monitor 照答经**那台**后端的 `files-delete`（带 `expect`）删（`ccm_legacy.rs`，部署按钮与那台长连接握手完成两个时刻）。
不并进 `deploy-plan` 的答：计划在连上那台常驻后端之前问（预检），那时 `files-delete` 无门可走。

```text
→ {"id":"r1","cmd":"deploy-retired","args":{"dial":{…}}}
← {"kind":"reply","id":"r1","ok":true,"data":{"verdict":"remove","expect":"#!/bin/sh\n# cc-monitor: ccm = …\n…","why":null}}
```

| 字段 | 向 | 意思 |
|---|---|---|
| `dial` | → | 怎么够到那台（与 `files` 链路同一份拨号请求）；沿池里那条 SSH 开只读 SFTP：stat ＋ 至多一次读回（上限 256 KiB，与 `files-peek` 同一个口径） |
| `text` | → | 与 `dial` 二选一：本机 PATH 上另一个 `ccm` 的开头一截（monitor 读的），按同一条规矩认它是不是我们早先放的；不读盘、不拨号（monitor 本机探针只拿来说话，不删） |
| `verdict` | ← | `absent`（不在）· `remove`（第一行 `#!`、第二行认得出两形记号之一 ⇒ 是我们放的）· `keep`（别的一律不动） |
| `expect` | ← | 只在 `remove` 时是字符串：读到的全文，删时原样交 `files-delete` 当期望值（盘上变了就不删）；其余 `null` |
| `why` | ← | 只在 `keep` 时是字符串：为什么不动（不是我们放的 · 读不成文本）；其余 `null` |

错误码：`bad_args`（`dial` / `text` 恰给一个；都不给不许退成问本机落点）· `unreachable`（SFTP 开不成）。
⚠ **CLI 面也有它**（`--deploy-retired`，入参从 stdin 读）：monitor 本机探针就是跑自己落点那份 `ccm -- --deploy-retired` 交 `{text}` 问的（同步命令里不连常驻后端）。

#### `place-verdict`：本机那一份放不放（P1，09-29；只读落点那一个文件）

「判定只在后端」：monitor 放本机后端（`~/.cc-monitor/bin/ccm`）之前还没有常驻后端可问 —— 可 `ccm` 就是后端本体，
手上那份字节自己就答得了。monitor 盘上与手上**逐字节相同**时直接用、不问；不同时把手上那份写成暂存件（`.<名>.<pid>.partial`）、
跑 `<暂存件> -- --place-verdict`（CLI 面，入参走 stdin）问一次，照答放（`rename` 上位）或不放；暂存件在任何结局下都清掉（`local_backend::extract_embedded_to`）。
判定：表 B 本机那一行（这份字节自己的 (OS, arch)）· 落点那一份的身份戳 vs 自己的 `BUILD_ID`（只升不降，HX2 D-b；同一版 ⇒ 放：只在字节不同时被问）。

```text
$ printf '%s' '{"dest":"/home/u/.cc-monitor/bin/ccm","machine":"本机"}' | .ccm.4242.partial -- --place-verdict
{"action":"place","why":"那台上是 p5u-…，这一版是 p5v-…"}
```

| 字段 | 向 | 意思 |
|---|---|---|
| `dest` | → | 落点的绝对路径（不在 ⇒ 没装 ⇒ 放；读不了 ⇒ `undecidable`，不当成没装） |
| `machine` | → | 对人说话时这台叫什么（monitor 交「本机」） |
| `action` | ← | `place`（放 / 换上去）· `keep`（盘上那一份不比这一份旧 ⇒ 不动、用它） |
| `why` | ← | 人读原因（`keep` 时点名两边各是哪一版） |

错误码：`bad_args`（`dest` 缺或不是绝对路径 · `machine` 缺）· `refused`（表 A / 表 B：这台不承诺 —— `message` 就是对人说的那一句）·
`undecidable`（落点那一份不说自己是谁 / 身份不唯一 / 读不了 —— 不覆盖，`message` 说清出路）。
⚠ 本机 (Linux, aarch64) 那一格的「不承诺」落在写暂存件之后（字面是写第一个字节之前；问完即删、净足迹零）。

#### `resident-verdict`：远端常驻后端要不要换一次（THIN，09-29；纯判定）

「判定只在后端」：monitor 接远端常驻后端时读到 hello，把 hello 里的 `build_id` 交给**本机常驻后端**问「换还是接」，
只照答办（`remote_resident::attach`）。规矩与 `deploy-plan` 同一家：`BUILD_ID` 可比序只升不降（HX2 D-b），那台比手上这一版旧 ⇒ 换一次
（`--resident-ensure --replace`），换过一次仍旧 ⇒ 照接；同一版 · 更新 · 序解不出 ⇒ 接。不碰盘、不拨号。

```text
→ {"id":"v1","cmd":"resident-verdict","args":{"mine":"p5a-x","theirs":"p4z-y","replaced":false}}
← {"kind":"reply","id":"v1","ok":true,"data":{"action":"replace","older":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `mine` | → | monitor 手上这一版后端自报的身份（非空） |
| `theirs` | → | 那台 hello 报的 `build_id`（缺 ⇒ 空串） |
| `replaced` | → | 这一趟是不是已经换过一次 |
| `action` | ← | `replace`（换掉再接）· `attach`（接上它） |
| `older` | ← | 那台是不是比手上这一版旧（与 `replaced` 无关；版本提示那句话按它挑「会换掉」还是「不会换回去」） |

**错误码**：`bad_args`（`mine` 缺或空 · 缺 `theirs` / `replaced`）。
⚠ **CLI 面也有它**（`--resident-verdict`，入参从 stdin 读；按派生规则「非内建即上 CLI」）：真正的用法是本机常驻后端的帧面。

#### `history-annotate`：改一条历史注解（2026-09-25）

历史注解（星标 / 改名 / 隐藏 / 上次用哪个号起这个会话）的**读写者是本机常驻后端**（文件留在原处、同一路径，不迁移、一条不丢）。
那份文件就是 monitor 从前读写的 `<家>/history-metadata.json`（家 = `~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`）：那台后端按家推（推不出家 ⇒ `no_annotations`，不猜路径）。
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

#### `history-forget`：删一条历史注解（2026-09-25）

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

#### `history-last-accounts`：sid → 上次用哪个号起（2026-09-25，**只读**）

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

#### `remote-reach`：本机后端的可达表登记（2026-09-25）

「本机后端问远端后端」那一跳（实现住后端 `remote_ask.rs`，全后端只此一处）要先知道「怎么够到那台」。
monitor（宿主，只交事实）在**每台**远端流握手成功那一刻交一次拨号请求（那台后端恒在固定落点 `"$HOME"/.cc-monitor/bin/ccm`，不再交路径）。本机后端记进**内存**可达表（后端重启就空，下次那台连上再填），**只登记、不拨号**。
之后的两路都查这张表：资产目录同步（`assets-sync`）· 历史跨机 join（`history-projects` / `history-sessions` 带 `origin`）。
老远端也登记：历史那一路问它的是 `--list-projects` / `--list-sessions` 这种老子命令。

```text
→ {"id":"r1","cmd":"remote-reach","args":{"origin":"dev","dial":{"host":"10.0.0.2","port":22,"user":"u","key_path":"~/.ssh/id_ed25519"}}}
← {"kind":"reply","id":"r1","ok":true,"data":{"origin":"dev","reach":[{"origin":"dev","machine":null}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 那台的名字（monitor 的 origin 名，本后端只当不透明的键用） |
| `dial` | → | 那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体）。用时会把 `use` 改成 `capture` |
| `reach` | ← | 登记之后的可达表 `[{origin, machine}]`（同 `assets-sync` 的那一格） |

**错误码**：`bad_args`（缺 `origin` / `origin` 空串 · 缺 `dial` · 可达表满）。
⚠ **CLI 面也有它**（`--remote-reach`，入参从 stdin 读），但一次性进程的可达表随进程退出就空 —— 真正的用法是常驻后端的帧面。

#### `skill-read`：读来源那台上的一个 skill（2026-09-25，**只读**）

「装要用户点」那一步的读半边：在**来源那台**跑，交出 `<skill 根>/<名>/` 下每个文件的原文（「内容，原样拷过去」）。

```text
→ {"id":"k1","cmd":"skill-read","args":{"name":"demo"}}
← {"kind":"reply","id":"k1","ok":true,"data":{"root":"/home/u/.claude/skills","dir":"/home/u/.claude/skills/demo","files":[{"path":"SKILL.md","text":"---\n…","bytes":120,"exec":false,"why":null},{"path":"bin/tool","text":null,"bytes":90210,"exec":true,"why":"不是文本文件"}],"skipped":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | skill 的目录名（一段：不许分隔符 / `..` / 点开头） |
| `project` | → | 可缺席：给了 ⇒ 那个项目里的 skill（`<项目>/.claude/skills`，项目目录是这台上的绝对路径）；缺 ⇒ 用户级 |
| `root` / `dir` | ← | 这台 skill 的根 / 这个 skill 的目录（绝对路径） |
| `files` | ← | 每个普通文件一条 `{path, text, bytes, exec, why}`：`path` 是 skill 里的相对路径（`/` 分段）；`text` 是原文，**不是文本 / 超 4 MiB / 读不出来 ⇒ `null` ＋ `why`**（这一个装不过去：今天的写口只收文本）；`exec` 执行位（非 unix 恒 `false`） |
| `skipped` | ← | 没读的那几处（指向目录的链接 / 特殊文件） |

**错误码**：`bad_args`（名字不对）· `not_found`（这台没有这个 skill）· `too_large`（文件超过 512 个 —— 装一半比不装更坏，整趟不读）· `io_failed`。
⚠ **CLI 面也有它**（`--skill-read`），入参从 stdin 读。

#### `skill-install-plan`：在要被写的那一台判 skill 装不装得过来（2026-09-25，**只读**）

在**要被写的那一台**跑（事实是那台的）。差异四态与「不同的要显式说盖、不然整趟拒」那道闸**原样用** `mcp-sync-plan` 的那一份（键 = 文件相对路径，值 = `{text, exec}`）。
一个字节都不写：写经调用方 → 那台后端 `files-put`（`expect` = 这里回的 `target` 里那一份，不存在 = `null`，`parents: true`）＋ `files-chmod`。

```text
→ {"id":"k2","cmd":"skill-install-plan","args":{"name":"demo","source":[{"path":"SKILL.md","text":"---\n…","exec":false}]}}
← {"kind":"reply","id":"k2","ok":true,"data":{"root":"…/skills","dir":"…/skills/demo","rows":[{"path":"SKILL.md","state":"new","suspects":[],"blocked":null}],"target":[],"write":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | skill 的目录名 |
| `project` | → | 同 `skill-read`（装到哪一级） |
| `source` | → | `skill-read` 读到的 `[{path, text, exec}]`（`text` 可为 `null` = 装不过去的那一个） |
| `take` / `overwrite` | → | 可缺席，语义同 `mcp-sync-plan`（给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名） |
| `root` / `dir` | ← | 这台 skill 的根 / 要写进去的目录 |
| `base` / `prefix` | ← | 写的时候 `files-put` 用的 `root` 与相对前缀：`rel` = `<prefix>/<path>`。skill 根在 ⇒ `base` 就是它；不在 ⇒ 是它的上一层（配置根），由 `parents` 建出来 |
| `rows` | ← | 每个路径一行 `{path, state, suspects, blocked}`：`state` 闭集同 `mcp-sync-plan`；`suspects` 只在 `new` / `differs` 上有 `{kind, value, there}` —— `kind` 闭集 `executable`（有执行位或 `#!` 开头）· `binary`（来源读不出原文）· `abs-path`（文本里的绝对路径，`there` 闭集同 `mcp-sync-plan`）· `command-missing`（`#!/usr/bin/env X` 的 `X` 在这台后端的 `PATH` 上找不到）；`blocked` = 这台上那一份盖不了的原因（不是文本等），否则 `null` |
| `target` | ← | 这一趟拷的那几个路径在这台上现有的原文 `[{path, text}]` —— 写的时候当 CAS 期望 |
| `write` | ← | 没给 `take` ⇒ `null`；给了 ⇒ 真要写的路径（排序） |
| `ledger` | ← | 没给 `take` ⇒ `null`；给了 ⇒ `{<path>: {digest, created}}`，恰是 `write` 那几个：`digest` = 来源那一份原文的摘要（FNV-1a 64，16 位小写十六进制），`created` = 这台上原来没有（`new`）。调用方写完把**真写成了的那几个**原样交 `skill-install-record`（`op: "add"`） |

**错误码**：`bad_args`（名字 / 路径不对 · `take` 里有不在 `source` 里的 · `take` 了来源读不出原文的那一个）· `bad_file`（`take` 了这台上盖不了的那一个）· `needs_consent`（同 `mcp-sync-plan`）· `too_large` · `io_failed`。
⚠ **CLI 面也有它**（`--skill-install-plan`），入参从 stdin 读。

#### `skill-install-record`：skill 装记录的写口（2026-09-25，**写后端自有状态**）

「要，只删装时写进去的文件」：装的时候记下写了哪几个文件，卸只删这些。记录住这台后端自己的 `~/.cc-monitor/skill-installs.json`（第四层；一个用户文件都不写）。

```text
→ {"id":"r1","cmd":"skill-install-record","args":{"op":"add","name":"demo","files":{"SKILL.md":{"digest":"8c3e…","created":true}}}}
← {"kind":"reply","id":"r1","ok":true,"data":{"dir":"/home/u/.claude/skills/demo","name":"demo","changed":true,"remaining":1}}
→ {"id":"r2","cmd":"skill-install-record","args":{"op":"drop","dir":"/home/u/.claude/skills/demo","paths":["SKILL.md"]}}
← {"kind":"reply","id":"r2","ok":true,"data":{"dir":"/home/u/.claude/skills/demo","name":"demo","changed":true,"remaining":0}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `op` | → | `add`（装完记）或 `drop`（卸掉 / 已经不在的摘掉）；MCP 那一条：`mcp-add` · `mcp-drop` |
| `name` | → | `add`：skill 的目录名。**目录由这台后端按 `skill 根 / name` 自己算**，不收调用方给的路径 |
| `at` | → | `add` 可缺席：缺 ⇒ 目录按 skill 根算；`"home"` ⇒ 目录 = 记录所在那个家目录（装在家目录底下、不在 skill 根下的东西用）；其余值 ⇒ `bad_args` |
| `files` | → | `add`：`{<相对路径>: {digest, created}}` —— `skill-install-plan` 答的 `ledger` 里真写成了的那几个。同一目录再装一次：新路径加进来、已记的换新摘要、`created` 取第一次的 |
| `dir` | ↔ | `drop` 的入参：记录里那个 skill 目录；应答里是这一条记录的目录 |
| `paths` | → | `drop`：要摘的相对路径（不在记录里 ⇒ `bad_args`，一个字节不动）；摘到零个 ⇒ 整条记录摘掉 |
| `project` | → | `add` 可缺席：给了 ⇒ 目录按那个项目里的 skill 根算 |
| `file` · `digest` | → | `mcp-add` / `mcp-drop`：配置文件的绝对路径（键）· 装进去的那一条的摘要（`mcp-add`；卸时对得上就不用问） |
| `changed` / `remaining` | ← | 记录变没变（没变不写）/ 这一条还剩几个文件（MCP：那份配置里还记着几条） |

**错误码**：`bad_args` · `not_found`（`drop` 的目录没记着）· `ledger_unreadable`（记录读不懂 / 另一个版本写的 —— **不覆盖**）· `too_large`（一趟超过 512 个）· `io_failed`。
⚠ **CLI 面也有它**（`--skill-install-record`），入参从 stdin 读。

#### 只读查询面（`C1`，2026-09-24）—— **八条一次性查询搬上这条长连接**

层 1 ＋。这八条此前**只有**一次性子命令那一面：monitor 每问一次就新拨一条 SSH（握手 ＋ 鉴权 ＋ exec），账号那两条还被一个 10 秒的轮询按台数翻倍。现在它们也在帧面上 —— **跑的是 CLI 那一臂同一个函数**，只是输出从 stdout 换成应答里的 `data`。

| 帧命令 | 同一个函数的 CLI 那一臂 | 应答形状 |
|---|---|---|
| `history-projects` | `--list-projects` | 按行 |
| `history-sessions` | `--list-sessions` | 按行 |
| `history-search` | `--search` | 按行 |
| `history-run` | （无旧形）| 一个子运行的记录，按运行读（`{run, path, rows, end, more}`） |
| `accounts-list` | `--list-accounts` | 成品（`{meta, accounts, notice}`） |
| `accounts-sessions` | `--session-accounts` | 按行 |
| `history-read` | `--read-session` · `--read-session-from-offset`（不带 `--index`） | 按字节分页（逐行成品） |
| `history-tail` | `--read-session-tail` 的那张「尾段在哪」的图 | 四个数 |

- **按行**：`data = {"lines": [...]}`，每个元素就是 CLI 那条 stdout 的一行（trim 过、剔空行）。整份输出超过 32 MiB ⇒ `too_large`，**不截断**（截断的清单会被当成完整的用）。
- **名字刻意不与 CLI 同名**：CLI 面从 `REGISTRY` **自动派生**（`--<名>`），同名就会把 `--list-projects` 抢过去改印一行 JSON。⇒ 代价如实写：八条同拍多出八个 CLI 面 `--history-projects` · `--history-sessions` · `--history-search` · `--history-subagents`（今天是 `--history-run`）· `--history-read` · `--history-tail` · `--accounts-list` · `--accounts-sessions`（stdin 一段 JSON ＝ `args`，stdout 一行 JSON ＝ `data`）。它们与老的那八个子命令是**同一个函数的两个宿主**，不是第二份实现。
- 八条全在阻塞档（做文件 I/O）⇒ `cancel` 命中回 `not_cancellable`。
- 失败的 code 都是**命令级**的；读失败 `failed`，参数缺或类型不对 `bad_args`。

#### `history-projects`：列全部项目（**出成品**：并上注解 ＋ 判活，远端经本机后端问）

历史跨机 join 的唯一的家是**本机常驻后端**（实现 `history_join.rs`）。`origin` 缺席 = 这台机器：记录树里的项目（`--list-projects` 那一行）＋ 合成历史（Codex 按 cwd 分组，项目键 `codex:<cwd>`）＋ 这台后端自己判活（pidfile）；
`origin` 给了 = 可达表里的那一台（`remote-reach` 登记的）：本机后端沿池里那条 SSH 在那台跑 `--list-projects`（CLI 老子命令，stdout 形状一个字节没变 ⇒ **那台的后端不必升级**），判活「不知道」。
两支都并上**这台**的注解（`history-annotate` 那一份：星标数 / 隐藏数；读不到 ⇒ 两个数 `null`、`notice` 说为什么）。项目按「活的 → 有星标的 → 最近动过的」排。

```text
→ {"id":"q1","cmd":"history-projects","args":{"origin":"dev"}}
← {"kind":"reply","id":"q1","ok":true,"data":{"rows":[{"agent":"claude","projectPath":"/home/u/proj","projectName":"proj","projectDir":"-home-u-proj","sessionCount":3,"starredCount":1,"hiddenCount":0,"lastActivity":1727250000000,"hasLive":null,"origin":"dev"}],"notice":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 可缺席：那台的名字（可达表的键）。缺席 = 这台 |
| `rows` | ← | 每项目一行：`agent`（同 `history-sessions`）· `projectPath` · `projectName` · `projectDir`（懒加载的键，原样交回 `history-sessions`）· `sessionCount` · `starredCount` / `hiddenCount`（`null` = 不知道，**不是 0**）· `lastActivity`（毫秒）· `hasLive`（`null` = 这条路上答不了）· `origin`（远端那台才有） |
| `notice` | ← | 注解没并上的那句话；`null` = 并上了 |

**错误码**：`bad_args`（`origin` 空串 / 不是串）· `failed`（这台的记录树读不动）· `unreachable`（可达表里没有那一台 / 那台问不出来 —— 带那台的名字与原因）· `too_large`。
⚠ **CLI 面也有它**（`--history-projects`，入参从 stdin 读）；一次性进程的可达表是空的 ⇒ 只答得了这台。

#### `history-sessions`：列一个项目下的会话（**出成品**，同上）

```text
→ {"id":"q2","cmd":"history-sessions","args":{"project_dir":"-home-u-proj","origin":"dev"}}
← {"kind":"reply","id":"q2","ok":true,"data":{"rows":[{"agent":"claude","sessionId":"0f…","projectPath":"/home/u/proj","projectName":"proj","aiTitle":null,"firstUserExcerpt":"…","startedAt":1727250000000,"updatedAt":1727250001000,"jsonlPath":"/home/u/.claude/projects/-home-u-proj/0f….jsonl","isLive":null,"messageCountApprox":12,"isBg":false,"starred":false,"customTitle":null,"hidden":false,"origin":"dev"}],"notice":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `project_dir` | → | `history-projects` 给的那个项目键：记录树的项目目录名（不是路径；含分隔符 / `..` ⇒ `bad_args`），或合成历史的 `<kind>:<cwd>`（只在这台） |
| `origin` | → | 同 `history-projects` |
| `rows` | ← | 每会话一行：`agent`（哪一家：记录树那一支是记录树那一家，合成历史是那一家）· `sessionId` · `projectPath` · `projectName` · `aiTitle` · `firstUserExcerpt` · `startedAt` / `updatedAt`（毫秒）· `jsonlPath` · `isLive`（`null` = 答不了）· `messageCountApprox` · `isBg` · `starred` / `customTitle` / `hidden`（这台的注解）· `forkedFromSessionId` / `forkedFromMessageUuid`（`/branch` 分叉来的才有）· `origin`（远端那台才有） |
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
| `lines` | ← | 每命中会话一行 `SessionHits`（带 `agent`：只扫记录树 ⇒ 记录树那一家），形状与行序同 `--search` |

⚠ 选项**不在帧面另写一份语义**：这几个字段被摊回 `--include-tools` / `--scope` / `--after-ms` / `--limit`，交给 CLI 那一臂同一个解析。

#### `history-search-merge`：把各台的搜索结果合成一份

```text
→ {"id":"q4","cmd":"history-search-merge","args":{"sessions":[{"sessionId":"a","updatedAt":100,"hitCount":3,"hitsTruncated":false,…},{"sessionId":"b","origin":"pi","updatedAt":300,"hitCount":2,"hitsTruncated":true,…}]}}
← {"kind":"reply","id":"q4","ok":true,"data":{"totalHits":5,"sessionCount":2,"truncated":true,"sessions":[{"sessionId":"b",…},{"sessionId":"a",…}]}}
```

界面照旧逐台经通道问那台常驻后端的 `history-search`（各台内存索引保热），把解码过的会话行（远端的补了 `origin`）一次交给**本机**后端合：
`updatedAt` 倒序、稳定（`search_rules::sort_by_recency`，与每台后端花 snippet 预算同一个函数）· `totalHits` = `hitCount` 之和 ·
任一行 `hitsTruncated` ⇒ `truncated`。只读排序与计数要的那三格，其余原样透传。纯计算。错误码：`bad_args`（不是 `{sessions:[…]}` / 某一行那三格缺或类型不对）。
只上帧面（`STREAM_ONLY`）。

#### `history-run`：一个子运行的记录（按运行读，通用层不认任何一家的目录与字段）

```text
→ {"id":"q4","cmd":"history-run","args":{"parent":"/home/u/.claude/projects/-p/s.jsonl","tool":"toolu_01","from":0}}
← {"kind":"reply","id":"q4","ok":true,"data":{"run":"a1","path":"…","rows":[{"message":{…},"rid":"…"},{"message":{…}}],"end":20480,"more":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `parent` | → | 父会话的记录路径（读会话那道围栏照旧；越界 ⇒ `path_refused`） |
| `run` | → | 子运行标识（运行表 `session_runs` 里那一格）；与 `tool` 至少给一个，都给以它为准 |
| `tool` | → | 派出它的那次工具调用的 id：后端在父记录里找那次调用的派出链接（适配层 `child_link`）；还没对上（前台子运行跑完才写明是哪一个）⇒ `not_found` |
| `from` | → | 从这个字节起读（缺 ＝ 0）；续读拿上一次的 `end` |
| `run` | ← | 读的是哪个子运行 |
| `path` | ← | 那份子运行记录（不透明，给查看器整份打开用） |
| `rows` | ← | 这一页里每一条认得出的记录：`message` 在渲染模型里的样子（与主会话同一套记录成品）· `rid` 它的对账键（没有 ⇒ 省略） |
| `end` | ← | 读到哪了（最后一个整行之后） |
| `more` | ← | 这一页没读到头（再从 `end` 读） |

子运行的记录住哪由适配层给（`ChildFace::sources`）；哪一份是这个子运行，看它头几条记录的归属（`run_of`）。都对不上 ⇒ `not_found`。

#### `accounts-list`：账号清单（**出成品**）

```text
→ {"id":"q5","cmd":"accounts-list","args":{"agent":"claude-code"}}
← {"kind":"reply","id":"q5","ok":true,"data":{"meta":{"enabled":true,…},"accounts":[{"name":…,"authKind":…,"authReady":…},…],"notice":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `agent` | → | 必填：这次起会话的是哪一家（适配器 id；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）。只有它是这台机器 apikey 表的那一家时，表里的行才算数（条 49） |
| `meta` | ← | `{enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error}`（同 `--list-accounts` 首行去掉分帧用的 `kind` / `accountZeroAware`）。账号库目录跟着家走（`~/.cc-monitor/accounts`），没有另指位置的入参 |
| `accounts` | ← | 每账号一个对象，字段同 `--list-accounts` 的账号行；**并上了这台机器自己那份 apikey 表**：表里有行的号 `authKind` 是 `api-key`、`authReady` 按 `acct_core::auth_ready`（规则住 `acct-core`，CLI 那一臂不并表） |
| `notice` | ← | 「能用但有缺」：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）时的一句话；否则 `null` |

**错误码**：`bad_args`（缺 `agent`）· `too_large`。
⚠ 此前应答是 `{"lines": [...]}`（与 CLI 逐行同形）、并表在 monitor 做且只并本机；老后端仍回旧形状 ⇒ 新界面当场认出「两端契约对不上」。

#### `accounts-trust`：换号前的信任预检（替掉逐次拨号的 `--account-trust` / `--account-trust-zero`）

```text
→ {"id":"q7","cmd":"accounts-trust","args":{"configDir":"/home/u/.cc-monitor/accounts/a","cwd":"/home/u/proj"}}
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

#### `accounts-init`：建账号库（**写用户文件**，同步文件 I/O，阻塞档）

这台机器现在登录的那个身份收成名叫 `name` 的默认号：Claude 的身份文件（凭据 · `.claude.json` · 几份本机状态）搬进
`~/.cc-monitor/accounts/<name>/`（`0700`，凭据 `0600`），共享库 `~/.claude` 顶层其余每一项在号的目录里链回共享库；写清单
`~/.cc-monitor/accounts/accounts.json`（schema v1，账号 0 合成在数组末尾、没有 `configDir` 键）；然后给这个号加上 `<名>cc` ＋ `<名>cct` 两条别名。

**别名清单只跟着账号表里号的增减走**（建号 · 删号 · 回滚都是它的特例，实现只有一处：比前后账号表）：新出现的号加 `<名>cc`（`-- --account <号>`）
与 `<名>cct`（再带 `--ccm-tmux`；没有 tmux 的那一份别名文件只加 `<名>cc`），名字被别的别名占着就跳过；消失的号删掉参数指向它的全部（不论名字）。
修复 · 设默认 · 隔离不动清单；平时不回补（改了名、删掉其中一条都保持原样）。这台说哪几种 shell 就改哪几份别名文件（Windows 上 `aliases.sh` 与 `aliases.ps1` 两份）。
别名文件还不在 ⇒ 先带上首建那几条（`cc` · `cct` · `cca`；PowerShell 那一份只有 `cc`）。

```text
→ {"id":"a1","cmd":"accounts-init","args":{"name":"z","dryRun":true}}
← {"kind":"reply","id":"a1","ok":true,"data":{"applied":false,"steps":["建目录 /home/u/.cc-monitor/accounts", …],"notes":[],"backup":null,"account":null,"loginCmd":null,"aliasNames":["zcc","zcct"],"keyMasked":null,"keyProblem":null,"aliases":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 默认号的名字：过 `shell_quote_core::account_name_ok`（与 `ccm … --account` 同一条），`0` 是保留名 |
| `dryRun` | → | 可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步 |
| `applied` | ← | 这一趟真改了盘没有 |
| `steps` | ← | 做了（预演时：将要做）的每一步，一句一行 |
| `notes` | ← | 提示（不挡这一趟），比如共享库里还没有可共享的项 |
| `backup` | ← | 这一趟留的备份（`~/.cc-monitor/accounts/.backup-<这一段>`，回滚用它）；没改动 ⇒ `null` |
| `aliasNames` | ← | 这个号会拿到的别名名字（`accounts-init` / `accounts-add`，预演时也给；别的命令 ⇒ `[]`） |
| `aliases` | ← | 改了的别名文件，一份一条 `{path, changed, added, removed, skipped, note}`：加了 / 删了 / 名字被占跳过的那几条；`note` = 没能自动改它时那一句（比如文件里有认不出的行 ⇒ 不动它）。账号表里没有号增减 ⇒ `[]` |

先备份再改：每一步动一份既有的东西之前先原样拷进备份目录（`0700`），**做成之后**才往备份里的 `undo.tsv` 记一行。
已经建过 · 名字不合规 · 身份文件是一条链接（以前的软链切号方式留下的）⇒ `refused`，一个字节不写。
共享库 `~/.claude` 不在 ⇒ 先建它。

**错误码**：`bad_args`（入参形状不对：多键 · 缺键 · 类型不对）· `refused`（句子说清为什么）· `not_enabled`（其余几条：还没有账号库）·
`io_failed`（盘上那一步没成：话里带做到第几步、用哪份备份回滚）· `unsupported`（这台做不了多账号，今天是 Windows）。
下面几条改账号库的命令码同这一套。⚠ **CLI 面也有它们**（`--accounts-*`，入参从 stdin 读）。

#### `accounts-add`：新建一个号（**写用户文件**，阻塞档）

```text
→ {"id":"a2","cmd":"accounts-add","args":{"name":"b","kind":"subscription","credFile":"~/snap/b.json"}}
← {"kind":"reply","id":"a2","ok":true,"data":{"applied":true,"steps":[…],"notes":[],"backup":"20260930-120000","account":{"name":"b","configDir":"/home/u/.cc-monitor/accounts/b"},"loginCmd":null,"aliasNames":["bcc","bcct"],"keyMasked":null,"keyProblem":null,"aliases":[{"path":"/home/u/.cc-monitor/aliases.sh","changed":true,"added":["bcc","bcct"],"removed":[],"skipped":[],"note":null}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 同 `accounts-init`；已有同名号 / 同名目录 ⇒ `refused` |
| `kind` | → | `"subscription"`（订阅号）或 `"api-key"`（API 号，清单里写 `authKind: "api-key"`） |
| `credFile` | → | 只订阅号：导入哪一份凭据（家目录底下的绝对路径或 `~/…`；是链接 / 空文件 / 不在 ⇒ `refused`）。复制成号里的 `.credentials.json`（`0600`），源不动 |
| `baseUrl` · `key` | → | 只 API 号：上游地址（缺席 = 默认上游）与 key 明文。建号**之前**先判写不写得进 apikey 表（空 key · 装不进表的地址 ⇒ `bad_args`、一个字节不写）；建好后 key 交给这台的 apikey 表（与 `apikey-key-set` 同一个写口），不进清单、不回显 |
| `isDefault` | → | 可缺席的布尔：真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它） |
| `dryRun` | → | 同上 |
| `account` | ← | 建出来的那个号：`{name, configDir}` |
| `configDir` | ← | 那个号的配置目录（`account` 里） |
| `loginCmd` | ← | 订阅号没导入凭据时：在终端里跑这一行登录（`'<家>/.cc-monitor/bin/ccm' -- --account '<名>'`，claude 自己的登录界面）；否则 `null` |
| `keyMasked` · `keyProblem` | ← | API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填） |
| `applied` · `steps` · `notes` · `backup` · `aliasNames` · `aliases` | ← | 同 `accounts-init` |

号的目录：链齐共享项；身份之外那几份本机状态从共享库复制成它自己的一份（共享库那份是模板）；身份本体绝不从别的号复制。

#### `accounts-remove`：删一个号（**写用户文件**，阻塞档）

```text
→ {"id":"a3","cmd":"accounts-remove","args":{"name":"b"}}
← {"kind":"reply","id":"a3","ok":true,"data":{"applied":true,"steps":["删除 /home/u/.cc-monitor/accounts/b","写账号清单 …"],"notes":[…],"backup":"…",…}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 要删的号；`0` · 不认识的号 ⇒ `refused` |
| `force` | → | 可缺席的布尔：删的是默认号时必须给真（剩下的第一个号接着当默认） |
| `dryRun` | → | 同上 |
| `applied` · `steps` · `notes` · `backup` · `aliases` | ← | 同 `accounts-init`；参数指向它的别名随之删掉（`removed`） |

只删它自己的目录（只许在账号库里），共享库一个字节不动；目录整棵先拷进备份（凭据的副本留在备份里，回滚要用）。
这台 key 表（`~/.cc-monitor/apikey-credentials.json`）里有这个号那一行（API 号）⇒ 第一步先清它：整份表先拷进同一份备份，只摘这一行、别的行不动；订阅号那一趟不碰这份表。表读不了 ⇒ 不清，`notes` 里说一句。

#### `accounts-set-default`：设默认号（阻塞档）

```text
→ {"id":"a4","cmd":"accounts-set-default","args":{"name":"b"}}
← {"kind":"reply","id":"a4","ok":true,"data":{"applied":true,"steps":["写账号清单 …"],"notes":[],"backup":"…",…}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 要当默认的号（不认识 ⇒ `refused`） |
| `dryRun` | → | 同上 |
| `applied` · `steps` · `notes` · `backup` · `aliases` | ← | 同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写 |

只改清单里 `isDefault` 那一格（只留一个）。

#### `accounts-repair`：修复（**写用户文件**，阻塞档，幂等）

```text
→ {"id":"a5","cmd":"accounts-repair","args":{}}
← {"kind":"reply","id":"a5","ok":true,"data":{"applied":true,"steps":["建链接 …","改链接 …","改权限 … → 600"],"notes":[],"backup":"…",…,"aliases":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `dryRun` | → | 同上 |
| `applied` · `steps` · `notes` · `backup` · `aliases` | ← | 同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null` |

每个号：目录改回 `0700` · 身份本体改回 `0600` · 缺的共享链接补上 · 指错的改指 · 共享库里已经没有的残链清掉
（身份那几项若还是链向共享库的链接 ⇒ 复制成这个号自己的一份，不是删）· 清单里的邮箱按各号 `.claude.json` 刷新 · 补齐每个号的别名。
共享项在号里是一份实体文件 ⇒ 不动它、`notes` 里说一句。

#### `accounts-isolate`：把一个共享项变成每个号各一份（**写用户文件**，阻塞档）

```text
→ {"id":"a6","cmd":"accounts-isolate","args":{"item":"settings.json","dryRun":true}}
← {"kind":"reply","id":"a6","ok":true,"data":{"applied":false,"steps":["复制成自己的一份 …"],"notes":[…],…}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `item` | → | 共享库顶层的一个名字（含 `/` · `.` · `..` · 共享库里没有 ⇒ `refused`） |
| `dryRun` | → | 同上 |
| `applied` · `steps` · `notes` · `backup` · `aliases` | ← | 同 `accounts-init`（`aliases` 恒 `[]`：这一条不动账号表）；它不在身份表里 ⇒ `notes` 提示之后「核对」会报它不是共享链接 |

复制成私有的那一下：旁边先复制一份、核共享库那份复制期间没被改过、摘掉链接（绝不跟着链接写回共享库）、换名上位、再核一次；不对 ⇒ 还原成链接。

#### `accounts-rollback`：按一份备份还原（**写用户文件**，阻塞档）

```text
→ {"id":"a7","cmd":"accounts-rollback","args":{"dryRun":true}}
← {"kind":"reply","id":"a7","ok":true,"data":{"applied":false,"steps":["还原 /home/u/.cc-monitor/accounts/b/skills","删掉 …（这一趟新建的）"],"notes":[],"backup":"20260930-120000",…}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `backup` | → ← | 入：用哪一份（`.backup-` 后面那一段，只许 `[0-9A-Za-z._-]`、不含 `..`）；缺席 ⇒ 最近一份还没还原过的。出：用的那一份 |
| `dryRun` | → | 同上 |
| `applied` · `steps` · `notes` · `aliases` | ← | 同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名 |

按撤销清单倒着来：先还原（现场的那一份先挪进备份里的 `pre-rollback/`，不直接删）、再删这一趟新建的；每一条自己的错误不挡别的条。
还原只落在账号库 · 共享库 · 家目录底下，删只删账号库里的；删号清掉的 key 表那一行从备份那一份里取回、只放回这一行（表里此后别人写进来的行不动）；做完留 `.rolled-back` 标记。有条没做成 ⇒ `io_failed`（话里说做了几条、哪几条没成、备份在哪）。

#### `accounts-verify`：核对（**只读**，**不读 stdin**）

```text
→ {"id":"a8","cmd":"accounts-verify","args":{}}
← {"kind":"reply","id":"a8","ok":true,"data":{"pass":false,"fails":1,"warns":0,"checks":[{"level":"fail","account":"b","text":"共享链接 skills 指向 /x，应当指向 /home/u/.claude/skills。点「修复」改回来。"},…]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `pass` | ← | 没有一条 `fail` |
| `fails` · `warns` | ← | `fail` / `warn` 各几条 |
| `checks` | ← | 每条 `{level, account, text}` |
| `level` | ← | `ok` · `warn` · `fail` · `skip` |
| `account` | ← | 说的是哪个号；全局那几条 ⇒ `null` |
| `text` | ← | 给人看的那一句 |

致命：身份没隔离开（身份文件是链接 · 两个号邮箱相同 · 共享库里留着原生根在家目录的那份身份）· 权限不对（号的目录不是 `0700` · 凭据不是 `0600`）·
共享没接上（缺链接 · 链错地方 · 断链 · 共享项在号里是实体文件 · 一个共享项都没有）。提示：还没登录（API 号不提示）· 号里有意料之外的实体项 ·
共享库本身的源头断了 · 身份本体（凭据 · `.claude.json`）哪儿都找不到 · 共享库顶层有一份 `0600` 的文件却不在身份表里。
每个号各一份（不链）的是 Claude 那一家的身份表：凭据 · `.claude.json` · `backups/` · `policy-limits.json`（连同 `.stamp.json`）·
`remote-settings.json` · `mcp-needs-auth-cache.json` · `stats-cache.json` · `.last-cleanup` · `.last-update-result.json` · `state/` · `feedback/`。
**错误码**：`io_failed`（问不出家目录）· `unsupported`。⚠ **CLI 面也有它**（`--accounts-verify`，不读 stdin）。

#### `accounts-login-cmd`：在终端里登录一个号的那一行（**只算不做**，阻塞档）

```text
→ {"id":"a9","cmd":"accounts-login-cmd","args":{"name":"b"}}
← {"kind":"reply","id":"a9","ok":true,"data":{"cmd":"'/home/u/.cc-monitor/bin/ccm' -- --account 'b'"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 清单里的一个号（不认识 ⇒ `refused`） |
| `cmd` | ← | 那一行：这台的 `ccm` 带 `--account` 起 claude（claude 自己的登录界面）；值一律经唯一的 quote（`shell_quote_core::posix_quote`） |

界面把它交给开终端那一步（本机 Linux 上不开窗，复制给人自己跑）。**错误码**：`bad_args` · `refused` · `io_failed` · `unsupported`。

#### `accounts-mcp-read`：这台各账号共用的用户级 MCP 此刻的样子（**只读**，**不读 stdin**）

```text
→ {"id":"m1","cmd":"accounts-mcp-read","args":{}}
← {"kind":"reply","id":"m1","ok":true,"data":{"enabled":true,"servers":["anysearch","cclsp"],"conflicts":[{"name":"cclsp","choices":[{"from":null,"holders":["q"],"gone":false},{"from":"z","holders":["z"],"gone":false}]}],"changed":[],"notes":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `enabled` | ← | 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的；用户级 MCP 就是 agent 自己那一份） |
| `servers` | ← | 共享集合里的名字（排好序）。**只有名字** —— 定义里可能带密钥，一个值都不上线 |
| `conflicts` | ← | 两边都改了、等用户挑的那几条：每条 `{name, choices}` |
| `name` | ← | 那一条的名字 |
| `choices` | ← | 每一版 `{from, holders, gone}` |
| `from` | ← | 挑这一版时交回 `accounts-mcp-pick` 的 `from`：`null` = 共享的那一版；号名 = 那个号里的那一版 |
| `holders` | ← | 此刻是这一版的那几个号 |
| `gone` | ← | 这一版是「没有这一条」（在 cc-monitor 里删过） |
| `changed` | ← | 这一趟改写了哪几个号（这一条恒空） |
| `notes` | ← | 提示：某个号的配置读不出来（这一趟不同步它）· 没写进去 |

共享集合住 `~/.cc-monitor/accounts-mcp.json`（`0600`；服务器表与 Claude 配置里那一段同一个键名，另带「上次同步时各号的样子」作对照底）。
同步本身不经帧命令触发：常驻后端盯账号库里每个号的 `.claude.json`（文件事件，不轮询），加号 · 建库之后帧面宿主也同步一趟。
三方对照：号里多了一条（底里没有）⇒ 进共享集合、同步到所有号；一条变了而它等于底 ⇒ 是被旧内容盖回去的 ⇒ 按共享集合写回；
既不等于底也不等于共享集合 ⇒ 采纳并同步；两边都改了 ⇒ 不自动选，列进 `conflicts`；号里少了一条 ⇒ 当作被盖掉、补回 —— 删除只走 `accounts-mcp-remove`。
写各号的配置只换顶层那一个键（别的字节一个不动），先把原文放进 `~/.cc-monitor/backups/accounts-mcp/<号>.claude.json`，再经文件管理面 CAS 写。
**错误码**：`io_failed`（问不出家目录 · 那份共享集合读不了）· `refused`（清单或那份共享集合解不开，不拿来算）。

#### `accounts-mcp-remove`：从各账号共用的用户级 MCP 里删一条（**写用户文件**，阻塞档）

```text
→ {"id":"m2","cmd":"accounts-mcp-remove","args":{"name":"anysearch"}}
← {"kind":"reply","id":"m2","ok":true,"data":{"enabled":true,"servers":["cclsp"],"conflicts":[],"changed":["z","b"],"notes":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 要删的那一条（共享集合里与哪个号里都没有 ⇒ `not_found`） |
| `servers` · `conflicts` · `changed` · `notes` · `enabled` | ← | 同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号 |
| `choices` · `from` · `holders` · `gone` | ← | 同 `accounts-mcp-read` |

删除只有这一条路：在某个号里删掉不算数（分不清是删了还是被盖掉了，按被盖掉补回）。已经在跑的会话不重读配置，新开的会话才用上。
**错误码**：`bad_args` · `not_found` · `io_failed` · `refused`。

#### `accounts-mcp-pick`：两边都改了的那一条用哪一版（**写用户文件**，阻塞档）

```text
→ {"id":"m3","cmd":"accounts-mcp-pick","args":{"name":"cclsp","from":"z"}}
← {"kind":"reply","id":"m3","ok":true,"data":{"enabled":true,"servers":["anysearch","cclsp"],"conflicts":[],"changed":["q"],"notes":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | 那一条 |
| `from` | ↔ | → 那个号里此刻的那一版；缺席 / `null` = 共享的那一版（共享集合里已经删了 ⇒ 删）。← 同 `accounts-mcp-read` |
| `servers` · `conflicts` · `changed` · `notes` · `enabled` · `choices` · `holders` · `gone` | ← | 同 `accounts-mcp-read` |

挑完之后共享集合是那一版，所有号跟上。那个号里现在没有这一条 ⇒ `refused`（刷新之后再挑）。
**错误码**：`bad_args` · `not_found` · `io_failed` · `refused`。

#### `accounts-sessions`：正在跑的会话各属哪个账号（**不读 stdin**）

```text
→ {"id":"q6","cmd":"accounts-sessions","args":{}}
← {"kind":"reply","id":"q6","ok":true,"data":{"lines":["{\"sessionId\":…,\"account\":…}", …]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `lines` | ← | 同 `--session-accounts`：每条运行中会话一行 |

#### `history-read`：按字节分页读一份会话（出逐行成品，给 monitor 的旁路快照）

```text
→ {"id":"q7","cmd":"history-read","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","offset":0}}
← {"kind":"reply","id":"q7","ok":true,"data":{"rows":[{"end":812,"hash":1469598103934665603,"message":{…},"cwd":"/w"},{"end":870,"hash":…}],"next":1048571,"eof":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径，围栏同 `--read-session`（越界 ⇒ `refused`） |
| `offset` | → | 从这个字节起（缺省 0） |
| `until` | → | 可选右端（半开区间 `[offset, until)`），= `--until` |
| `rows` | ← | 这一页里每个**可计行**一条（空白 / 纯 BOM 行不占）：`end` ＝ 这一行（含 `\n`）之后那个字节的偏移（原始字节，永远说得准；没 `\n` 收尾的残尾 ⇒ `null`）· `hash` ＝ 这一行正文（去掉 `\r`）的 FNV-1a 64 摘要（续传前核「还是不是那一行」）· `message` ＝ 这一行在渲染模型里的样子（**缺 ＝ 不进界面**，照占号）· `cwd` ＝ 这条记录自己的 `cwd`（缺 ＝ 没有）。页**不超过 1 MiB 原文、切在行尾**；区间到头时余下的全给 |
| `next` | ← | 下一页从这里起（= `offset` ＋ 这一页的原始字节数） |
| `eof` | ← | 区间到头了（`until` 或读时的文件长度） |

原先这里回 `text`（原文），由 monitor 切行、解析；记录解释搬进后端之后 monitor 只拿成品。

#### `history-page`：按字节分页读，出记录行（界面直接问）

```text
→ {"id":"q8","cmd":"history-page","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","offset":0,"seq":0,"whole":true}}
← {"kind":"reply","id":"q8","ok":true,"data":{"lines":[{"session_id":"s","path":"…/s.jsonl","seq":0,"cwd":"/w","message":{…}}],"next":1048571,"nextSeq":812,"eof":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` · `offset` · `until` | → | 同 `history-read` |
| `seq` | → | `offset` 那一行的行号（缺省 0）；续页交上一页的 `nextSeq` |
| `whole` | → | 这是「整份读进查看器」那一件：读过 256 MiB 就明拒 `too_large`（那句话说读到了哪；不许静默截断，F06） |
| `lines` | ← | 只装**进界面**的记录行：`session_id` · `path` · `seq`（第 k 个可计行 ＝ `seq + k`）· `cwd` · `message`（`JsonlRecord`） |
| `next` · `eof` | ← | 同 `history-read` |
| `nextSeq` | ← | 下一页第一行的行号 |

🔴 **为什么分页**：一帧应答要整个进内存、整个过线；本仓见过 270 MB 的会话，而 monitor 单帧上限 64 MiB。单行比一页还长时续读到行尾，但超过 32 MiB ⇒ `oversized_line`（不叫 `line_too_long`：那是入方向信封的协议级 code）。

#### `history-lines`：按行号取回一段（不依赖骨架索引）

```text
→ {"id":"q13","cmd":"history-lines","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","from":1200,"until":1400}}
← {"kind":"reply","id":"q13","ok":true,"data":{"from":1200,"next":1400,"eof":false,"lines":["{…}","{…}"]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径，围栏同 `history-read`（越界 ⇒ `refused`） |
| `from` | → | 第一行的行号（缺省 0） |
| `until` | → | 可选右端（半开区间 `[from, until)`）；缺 ＝ 到最后一个完整行为止 |
| `lines` | ← | **记录行**（形状同 `history-page` 的 `lines`）：`[from, next)` 里进界面的那些，第 k 个可计行的行号是 `from + k`（不进界面的照占号、不出现）。原先回可计行原文、由 monitor 解析 |
| `next` | ← | 下一段从这一行起（恒 ＝ `from` ＋ 这一段的可计行数） |
| `eof` | ← | 读到了最后一个完整行之后 |

**行号口径**：与实时 `line` 帧的 `seq`、`history-tail` 的 `total`、`history-index` 的行同一个空间 —— BOM 与全空白的行不占号、没 `\n` 收尾的残尾不计（判定只住 `history_query::line_counts`）。**一帧装得下**：交出的原文累计到 1 MiB 就停（至少一行）；单行超过 32 MiB ⇒ `oversized_line`。**代价**：后端零状态、每次从文件头数（O(`from` 之前的字节)），依据与读数。CLI 面随之自动多一条 `--history-lines`。

#### `history-record`：这条会话的记录还在不在（resume 之前问）

```text
→ {"id":"q12","cmd":"history-record","args":{"sid":"9d66c46d-bf88-4f99-877e-455555555555"}}
← {"kind":"reply","id":"q12","ok":true,"data":{"present":false,"root":"/home/u/.claude/projects"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 会话 id。**找哪一份只凭 sid、不收文件路径**（`INVARIANTS §41.6` 收窄第 3 条的同一个取向）；形状不合法（`[A-Za-z0-9-]`，1..=64）⇒ `bad_args`，先于任何 IO |
| `configDir` | → | 可选：这次 resume 要用的**账号配置目录**（`CLAUDE_CONFIG_DIR` 那一个）。给了 ⇒ 在 `<configDir>/projects` 里找；缺席 / `null` ⇒ 这台后端自己的家目录（与加这一格之前逐字同一问）。先过账号库那一个形状关（绝对路径 · 不上跳 · 无 shell 元字符与欺骗字符，`accounts_query::is_safe_config_dir`），不过 ⇒ `bad_args`，先于任何 IO。它选的是**哪棵树**，不是哪个文件 —— 找文件那一步照旧只凭 sid |
| `present` | ← | `<sid>.jsonl` 在那棵记录树里找得到（根那一层或项目目录那一层；符号链接不算命中）—— 找文件那一步与 `--fork-session` / `files-delete-session` 同一份（`branch_core::find_session_file`） |
| `root` | ← | 查的那棵记录树的根（报错时说清查了什么） |

**为什么要它**：最后一条逐字「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」。前端 resume 一跳在开终端之前问一次，答 `false` 就不开、把原因说出来。**射程**：查的是**那一棵**记录树 —— 调用方的话要说成「这棵树里没有」。从前只查这台后端的家目录，会话起在另一个账号的配置根下时答 `false` 而那边其实有；今天 monitor 带上这次 resume 要用的那个 `configDir`。CLI 面随之自动多一条 `--history-record`。

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

#### `history-index`：会话骨架索引（2026-09-24 上帧面）

```text
→ {"id":"q9","cmd":"history-index","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","offset":0}}
← {"kind":"reply","id":"q9","ok":true,"data":{"from":0,"end":5120088,"rows":[{"o":0,"n":812,…},…]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `offset` | → | 从哪个字节起（缺省 0；续传带上次尾行的 `end`） |
| `until` | → | 可选：只收起点 `< until` 的行 |
| `from` / `end` / `rows` | ← | **成品**：`rows` 是每个可计行一条（形状见 §10.3 的行），`end` = 下一次续传该带的 `offset`。与 `--read-session-from-offset --index` 的 stdout **中段逐行相同**（同一个扫描；CLI 那一臂照旧写头尾三段，帧面这一臂不写 —— 一帧是原子的，没有「有头没尾」这一形） |

**为什么上帧面**：它是**每开一个大会话就要一次**的查询（骨架），此前在远端走逐次拨号（`frame_query::STILL_DIALED` 那一行）。
整份超过 32 MiB ⇒ `too_large`（不截断）。界面经通道直接问（`src/frontend/ui/session-reads.ts`），本机与远端同一条路。

#### `history-user-inputs`：「你说过的话」清单（2026-09-24 上帧面）

```text
→ {"id":"q10","cmd":"history-user-inputs","args":{"path":"/home/u/.claude/projects/-p/s.jsonl","from":0}}
← {"kind":"reply","id":"q10","ok":true,"data":{"from":0,"end":5120088,"entries":[{"uuid":…,"timestamp":…,"excerpt":…}]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `from` | → | 增量起点（缺省 0；传上次尾行的 `end`）。超过文件长度 ⇒ `failed`（文件被截断或重写过） |
| `from` / `end` / `entries` | ← | **成品**（口径见 §10.4）：与 `--list-user-inputs` 的 stdout **中段逐行相同**（同一个扫描；头尾只属于 CLI 那一臂） |

**为什么上帧面**：大纲同样是每开一个会话就要一次（此前远端走逐次拨号）。⚠ CLI 面随之自动多两条
`--history-index` / `--history-user-inputs`（从 `REGISTRY` 派生，stdin 一段 JSON ＝ `args`，stdout 一行 JSON ＝ `data`）。

#### 功能侧只读查询—— 远端会话的任务

出处：`parity_ledger` 的 `session.tasks` / `plugins.marketplaces` 两笔 `ParityDebt`。这几样此前只有 monitor **直读本机**那一条路，远端机器上的同一份数据答不出来。本机后端与远端后端是同一个二进制 ⇒ 读法搬进后端，monitor 按 origin 问那一台（**本机也走这里**，monitor 的直读实现随之退役）。

- 宿主是 `feature_face`（不是 `read_face`，理由在它头注），本体在 `observe/`。
- 应答一律**按行**：`data = {"lines": [...]}`；整份超过 32 MiB ⇒ `too_large`（与 `C1` 同一个口径、同一个常量）。
- CLI 面同样自动派生（`--tasks-list`），已进 `SUBCOMMANDS`。〔插件市场那一条随界面上的插件只读列表一起删了〕
- 全在阻塞档（同步文件 I/O）⇒ `cancel` 命中回 `not_cancellable`。

#### `mcp-read`：这台机器的 MCP 列表成品（SH1，09-26，**只读**）

```text
→ {"id":"m2","cmd":"mcp-read","args":{"projectDir":"/home/u/proj"}}
← {"kind":"reply","id":"m2","ok":true,"data":{"entries":[{"scope":"user","name":"fs","server":{"command":"/opt/fs"},"sourcePath":"/home/u/.claude.json"}],"dirs":["/home/u/proj"],"problems":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `projectDir` | → | 可缺席 / `null`。给了就必须是绝对路径（拒 NUL / CR / LF）⇒ 多读 local 段（`.claude.json` 的 `projects[<它>].mcpServers`）与 project 段（`<它>/.mcp.json`） |
| `entries` | ← | `{scope, name, server, sourcePath}`：`scope` 闭集 `user` · `local` · `project`；`server` 原样（未知字段不丢） |
| `dirs` | ← | `.claude.json` 里那张项目表的键（排序）—— 「用过的项目目录」 |
| `problems` | ← | 在而读不出 / 不是 JSON 的那几份各一句（「这台没有」与「那份坏了」不合成一句）；不在的静默 |

读法住适配层（`agents::Adapter.mcp`，Claude 那一格 `agents/claudecode/mcp.rs`；`.claude.json` 找哪一份与资产目录同一处）。
错误码：`bad_args`（`projectDir` 不是绝对路径）· `too_large`（成品超过一帧上限，不截断）。⚠ **CLI 面也有它**（`--mcp-read`，入参从 stdin 读）。

#### `mcp-server-put`：增 / 改这台一个项目 `.mcp.json` 里的一条（MIG-3a，09-27，**写用户文件**）

```text
→ {"id":"m3","cmd":"mcp-server-put","args":{"projectDir":"/home/u/proj","name":"fs","server":{"command":"/opt/fs"}}}
← {"kind":"reply","id":"m3","ok":true,"data":{"path":"/home/u/proj/.mcp.json","changed":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `projectDir` | → | 这台机器上的绝对路径（不含 `..`）；落点恒是 `<它>/.mcp.json`（写面只此一个，`~/.claude.json` / `settings.json` 一个字节不碰） |
| `name` | → | server 名（空 ⇒ 拒） |
| `server` | → | 那一条的配置，原样写进 `mcpServers[name]` |
| `path` | ← | 写到了哪（解过链接的那一份） |
| `changed` | ← | 真写了吗（算出来与盘上逐字节相同 ⇒ `false`，一个字节不动） |

D 组「monitor 算好、后端写」按用户 09-27「一处后端」收进这台：读 → 规划（`assets/mcp_edit.rs::plan_project_mcp`，已存在但读不懂 ⇒ **拒绝覆盖**）→ 本进程文件管理面 `files-put`（CAS = 刚读到的那一份；`stale` 重读重算，最多三趟）。不留备份、不建父目录。
错误码：`bad_args`（缺 / 类型不对 · 名字空）· `bad_path`（`projectDir` 不是绝对路径）· `refused`（读不懂原文 / 写面拒）。⚠ **CLI 面也有它**（`--mcp-server-put`）。

#### `mcp-server-remove`：删这台一个项目 `.mcp.json` 里的一条（MIG-3a，09-27，**写用户文件**）

```text
→ {"id":"m4","cmd":"mcp-server-remove","args":{"projectDir":"/home/u/proj","name":"fs"}}
← {"kind":"reply","id":"m4","ok":true,"data":{"path":"/home/u/proj/.mcp.json","changed":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `projectDir` | → | 同 `mcp-server-put` |
| `name` | → | 要删的那一条 |
| `path` | ← | 那份文件 |
| `changed` | ← | 文件不在 / 那一条不在 ⇒ `false`（一个字节不写、不建文件） |

错误码同 `mcp-server-put`。⚠ **CLI 面也有它**（`--mcp-server-remove`）。

#### `mcp-sync-source`：装到别的机器时来源那一条（MIG-3a，09-27；09-30 改成只交那一条，**只读**）

```text
→ {"id":"s1","cmd":"mcp-sync-source","args":{"name":"fs","at":{"level":"project","dir":"/home/u/proj"}}}
← {"kind":"reply","id":"s1","ok":true,"data":{"path":"/home/u/proj/.mcp.json","def":{"command":"fs","env":{"API_KEY":null}},"slots":[{"field":"env","key":"API_KEY"}],"token":"8c3e…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` | → | server 名 |
| `at` | → | 在这台上哪一级：`{level:"project", dir}`（`<dir>/.mcp.json`）或 `{level:"user"}`（agent 自己那份用户级配置，只读） |
| `path` | ← | 读的是哪一份 |
| `def` | ← | 那一条；**`env` / `headers` 的值在这台就换成 `null`**（值不出来源机，只交键名） |
| `slots` | ← | 换成空位的那几格 `[{field, key}]` |
| `token` | ← | 按原样那一条（含密钥值）算的记号：它变了（连只改了一个密钥值也算）⇒ 应用时判 `stale` |

错误码：`bad_args` · `bad_file`（不是合法 JSON 等）· `bad_path` · `io_failed` · `missing`（那份不存在 / 里面没有这一条）· `refused`（读不出）。⚠ **CLI 面也有它**。

#### `mcp-sync-preview`：在要被写的那一台看装上之后那一条（MIG-3a，09-27；09-30 改成只看那一条，**只读**）

```text
→ {"id":"s2","cmd":"mcp-sync-preview","args":{"name":"fs","at":{"level":"project","dir":"/srv/proj"},"def":{"command":"fs","env":{"API_KEY":null}}}}
← {"kind":"reply","id":"s2","ok":true,"data":{"path":"/srv/proj/.mcp.json","state":"new","def":{"command":"fs","env":{"API_KEY":null}},"slots":[{"field":"env","key":"API_KEY","kept":false}],"suspects":[],"target":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` · `at` | → | 同 `mcp-sync-source`；`at` 只收项目那一级（用户级只读 ⇒ `refused`） |
| `def` | → | 来源那台交回的那一条，原样；`env` / `headers` 里夹着值 ⇒ `bad_args` |
| `path` | ← | 这台那份的路径 |
| `state` | ← | `new`（这台没有这一条）· `same`（除空位外一样）· `differs` |
| `slots` | ← | 每个空位 `{field, key, kept}`：`kept` = 这台那一条原来就有这个键的值（不填就沿用） |
| `suspects` | ← | 可疑项，每条 `{kind, field, value, there}`，闭集同 `mcp-sync-plan` |
| `target` | ← | 这台那份的记号（不存在 ⇒ `null`）—— 写的时候原样交回 |

错误码：`bad_args` · `bad_file` · `bad_path` · `refused`。⚠ **CLI 面也有它**。

#### `mcp-sync-apply`：在要被写的那一台把那一条写进去（MIG-3a，09-27；09-30 改成只写那一条，**写用户文件**）

```text
→ {"id":"s3","cmd":"mcp-sync-apply","args":{"name":"fs","at":{"level":"project","dir":"/srv/proj"},"def":{"command":"fs","env":{"API_KEY":null}},"fill":{"env":{"API_KEY":"…"}},"target":null}}
← {"kind":"reply","id":"s3","ok":true,"data":{"path":"/srv/proj/.mcp.json","written":true,"recordFailed":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` · `at` · `def` | → | 同 `mcp-sync-preview` |
| `fill` | → | 用户在确认卡上填的值 `{env: {键: 值}, headers: {…}}`；没填的键沿用这台原有的值，两样都没有 ⇒ `needs_input`、一个字节不写 |
| `target` | → | 看卡时这台那份的记号：这台在那之后变了 ⇒ `stale`，**一个字节不写、不重读重算** |
| `path` · `written` | ← | 写到了哪 · 真写了吗（与原有那一条逐字相同 ⇒ `false`） |
| `recordFailed` | ← | 装记录没记下来时那一句（`null` = 记下了） |

与 `mcp-server-put` 同一份规划（`plan_project_mcp`）；写完记进装记录（`skill-install-record` 的 `mcp-add`）。错误码：`bad_args` · `bad_file` · `bad_path` · `needs_input` · `refused` · `stale`。⚠ **CLI 面也有它**。

#### `skill-install-apply`：在要被写的那一台把勾的那几个 skill 文件写进去（MIG-3a，09-27，**写用户文件**）

```text
→ {"id":"k1","cmd":"skill-install-apply","args":{"name":"demo","source":[{"path":"SKILL.md","text":"…","exec":false,"why":null}],"target":[],"take":["SKILL.md"],"overwrite":[]}}
← {"kind":"reply","id":"k1","ok":true,"data":{"dir":"/home/u/.claude/skills/demo","written":["SKILL.md"],"chmodFailed":[],"recordFailed":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `name` · `project` | → | 同 `skill-read` |
| `source` | → | 来源那台 `skill-read` 的 `files`，原样（`path` · `text` · `exec`；`text` 为 `null` 的装不过去） |
| `target` | → | 看差异时这台 `skill-install-plan` 回的 `target`（这台那几份原文），原样 —— 写时的 CAS 期望 |
| `take` · `overwrite` | → | 同 `skill-install-plan`（`differs` 的必须在 `overwrite` 里点名，否则整趟拒 `needs_consent`） |
| `dir` | ← | 装到了哪 |
| `written` | ← | 真写成了的那几个（按写的顺序） |
| `chmodFailed` | ← | 写成了但执行位没置上的那几个 |
| `recordFailed` | ← | 装记录没记下来时那一句（`null` = 记下了）—— 记不下来 ⇒ 这一趟装的卸不掉 |

判（`skill_install::answer_plan_with`）· 写（本进程文件管理面 `files-put`，`parents`，不备份）· 记（`skill-install-record` 同一个写口）都在这一台。
看过之后这台那一份变了 ⇒ **停在那一个**（`stale`），话里说清前面写了哪几个；写了的照记。错误码：`bad_args` · `bad_file` · `io_failed` · `needs_consent` · `stale`。⚠ **CLI 面也有它**。

#### `ext-list`：设置「扩展」页那张表（09-30，**只读用户文件 · 写后端自有状态**）

这台现扫一次、记进资产目录，各台目录合成「条目 × 机器」一张表。判定全在这里：每格的点、那台上的各处与各自的态和「卸载」、机器那一行的「装到…」（从哪台拿哪一版 · 建议装到哪 · 能装到哪几处、不能的为什么）。**线上没有摘要**（界面没有东西可比）。

```text
→ {"id":"e1","cmd":"ext-list","args":{"visit":true}}
← {"kind":"reply","id":"e1","ok":true,"data":{"machines":[{"key":null,"here":true,"reachable":true,"name":"u@h","projects":["/home/u/p"]}],"rows":[{"kind":"skill","name":"demo","about":"…","detail":[{"label":"…","value":"…"}],"new":false,"builtin":null,"note":null,"cells":[{"state":"same","places":[{"at":{"level":"user"},"state":"same","dir":"/home/u/.claude/skills/demo","uninstall":true,"note":null}],"bring":{"from":null,"fromName":"本机","scope":{"from":{"level":"user"},"to":{"level":"project","dir":"/home/u/p"}},"targets":[{"at":{"level":"user"},"ok":false,"note":"…"},{"at":{"level":"project","dir":"/home/u/p"},"ok":true,"note":null}]},"note":null}]}],"problems":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `visit` | → | 可缺席：`true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算 |
| `machines` | ← | 每台一列 `{key, here, reachable, name, projects}`：`key` = 枢纽认它的键（本机后端自己 = `null`；没连上的也是 `null` 且 `reachable: false`）；`projects` = 那台上开过会话的项目目录 |
| `rows` | ← | 每个条目一行 `{kind, name, about, detail, new, builtin, note, cells}`，`cells` 与 `machines` 同序；cc-monitor 自带的（cc-bus）没人装过也有一行 |
| `builtin` | ← | 自带的扩展才有：`{note, hooks}` —— 内置备注 · 要不要在抽屉里列各台的钩子状态（界面问那台 `hooks-diag`） |
| `note` | ← | 用户写的备注（`ext-note-set`；随目录同步，各台里最新的那一条），没有 ⇒ `null` |
| `cells[].state` | ← | 表上那个点，闭集 `same`（用户级有，且是持有人最多的那一版；打平时本机那一份优先；自带的 = 本机后端二进制里那一份）· `differs` · `missing` · `project`（用户级没有、只在项目里有） |
| `cells[].places` | ← | 那台上的各处：全局一行在前（没有也列），再是每个装着它的项目，各 `{at, state, dir, uninstall, note}`：`state` 全局那一行 `same` / `differs` / `missing`、项目那几行 `same` / `differs`（与「这一版」比）；`uninstall` = 这一处有「卸载」；有它却不能卸 ⇒ `note` 说为什么（那台没建账号库时用户级 MCP 只读） |
| `cells[].bring` | ← | 机器那一行的「装到…」：`{from, fromName, scope:{from, to}, targets}`（`from` 同枢纽的键；`scope.to` = 建议的那一处，与来源同级）；`targets` = 能选的各处 `{at, ok, note}`，全局一项 ＋ 那台每个开过会话的项目，不能选的 `ok: false` 带一句（那台没建账号库时用户级 MCP 只读 · 自带的只装全局 · 就是来源那一处）；那台有账号库 ⇒ MCP 的全局那一项可选（写进那台各账号共用的那一份、同步到所有号）；没有 ⇒ `null`，`cells[].note` 说为什么（没连上 · 没有来源 · 那台没有项目） |
| `problems` | ← | 这台扫的时候读不出来的那几份 |

错误码：`bad_args` · `catalog_unreadable` · `io_failed`。读本进程的可达表 ⇒ **只在帧面上**（没有 CLI 面）。

#### `ext-note-set`：写 / 改 / 清一个扩展的备注（10-01，**写后端自有状态**）

记进这台资产目录自己那一格（`rev` = 这台目录里同一条目各台备注的最大值 ＋ 1），随目录同步到别的后端；生效的是各台里 `rev` 最大的那一条。

```text
→ {"id":"n1","cmd":"ext-note-set","args":{"kind":"skill","name":"cc-bus","text":"先加钩子"}}
← {"kind":"reply","id":"n1","ok":true,"data":{"note":"先加钩子"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` · `name` | → | `skill` / `mcp` · 名字 |
| `text` | → | 备注正文（首尾空白去掉；空串 = 清掉；最长 2000 字，超了拒、不截断） |
| `note` | ← | 现在生效的那一份（清掉了 ⇒ `null`） |

错误码：`bad_args` · `catalog_unreadable` · `io_failed`。⚠ **CLI 面也有它**（`--ext-note-set`）。

#### `ext-hub-preview`：装到一台之前那张确认卡，本机后端当枢纽（09-30，**只读**）

skill 与 MCP 同一条：本机后端向 `from` 取、交 `to` 判，拼成确认卡。skill 走 `skill-read` → `skill-install-plan`；MCP 走 `mcp-sync-source` → `mcp-sync-preview`；
cc-monitor 自带的那一个（cc-bus）不从别的机器拿，交被写那台用它自己二进制里那一份（`cc-bus-install-state`）。

```text
→ {"id":"e2","cmd":"ext-hub-preview","args":{"kind":"mcp","name":"fs","from":null,"to":"aya","scope":{"from":{"level":"project","dir":"/home/u/p"},"to":{"level":"project","dir":"/srv/q"}}}}
← {"kind":"reply","id":"e2","ok":true,"data":{"kind":"mcp","name":"fs","path":"/srv/q/.mcp.json","writes":["/srv/q/.mcp.json"],"unchanged":false,"suspects":[],"stop":null,"config":"{…}","slots":[{"field":"env","key":"API_KEY","kept":false}],"tokens":{"source":"…","target":null}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` · `name` | → | `skill` / `mcp` · 名字 |
| `from` · `to` | → | 来源那台 · 被写那台：可达表的键，**`null` = 这台自己** |
| `scope` | → | `{from, to}`，各是 `{level:"user"}` 或 `{level:"project", dir}`（那台上的绝对路径）；`to` = 用户在确认卡上选的那一处 |
| `path` · `writes` | ← | 被写那台上的落点 · 要写的那几个（skill：目录里的相对路径；MCP：那份配置文件） |
| `unchanged` | ← | 装上之后和现在一样 |
| `suspects` | ← | 要留意的几件（说人话） |
| `stop` | ← | 装不了的原因（非文本文件 · 这台那一份盖不了）；有它就不该确认 |
| `config` · `slots` | ← | MCP：装上之后那一条（待填的值是 `null`）· 每个空位 `{field, key, kept}` |
| `tokens` | ← | 两头看过的那一份的记号 `{source, target}` —— 应用时原样交回 |

`scope.to` 先过「自带的只装全局」那一道，不行 ⇒ `refused`、一跳都不发；MCP 装到全局由被写那台自己判：它有账号库 ⇒ 写进各账号共用的那一份（卡上的 `suspects` 带「删除只在 cc-monitor 里做」「新开的会话才用上」两句），没有 ⇒ `refused`；同一台同一处 ⇒ `refused`。错误码：`bad_args` · `bad_file` · `missing` · `refused` · `unreachable`（可达表里没有那台）· `io_failed`；远端那一跳的码原样转回。**只在帧面上**。

#### `ext-hub-apply`：装到一台，本机后端当枢纽（09-30，**写用户文件**）

```text
→ {"id":"e3","cmd":"ext-hub-apply","args":{"kind":"mcp","name":"fs","from":null,"to":"aya","scope":{…},"tokens":{"source":"…","target":null},"fill":{"env":{"API_KEY":"…"}}}}
← {"kind":"reply","id":"e3","ok":true,"data":{"path":"/srv/q/.mcp.json","changed":["fs"],"note":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` · `name` · `from` · `to` · `scope` | → | 同 `ext-hub-preview` |
| `tokens` | → | 确认卡上那一份：枢纽两头都再看一次，任一头对不上 ⇒ `stale`、**一个字节不写** |
| `fill` | → | MCP：用户填的值（同 `mcp-sync-apply`）；来源机上的值从不经过这里 |
| `path` · `changed` · `note` | ← | 写到了哪 · 写了的那几个 · 做成了但要知道的一件（执行位没改成 · 没记下来） |

skill 交 `skill-install-apply`（`take` = 卡上 `writes`，`differs` 的算用户已同意盖）；MCP 交 `mcp-sync-apply`；自带的 cc-bus 交 `cc-bus-install`（`note` 带原来那个目录留作备份的去处）。错误码：`bad_args` · `bad_file` · `missing` · `needs_input` · `refused` · `stale` · `unreachable` · `io_failed`。**只在帧面上**。

#### `ext-uninstall-preview`：从这台卸一个扩展之前那张卡（09-30，**只读**）

```text
→ {"id":"e4","cmd":"ext-uninstall-preview","args":{"kind":"skill","name":"demo","at":{"level":"user"}}}
← {"kind":"reply","id":"e4","ok":true,"data":{"kind":"skill","name":"demo","path":"/home/u/.claude/skills/demo","recorded":true,"files":["SKILL.md"],"backup":null,"said":"…","token":"…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` · `name` · `at` | → | 种类 · 名字 · 在这台哪一级（用户级 MCP：这台有账号库 ⇒ 从各账号共用的那一份里删、所有号一起撤；没有 ⇒ 只读、`refused`） |
| `path` | ← | skill 目录 / MCP 配置文件 |
| `recorded` | ← | 装记录里有（cc-monitor 装的）⇒ 只撤装时写进去的；没有 ⇒ 不是 cc-monitor 装的 |
| `files` | ← | 要删的那几个（skill：相对路径；MCP：那一条） |
| `backup` | ← | 不是 cc-monitor 装的：删之前先放到哪（`~/.cc-monitor/backups/`）；否则 `null` |
| `said` | ← | 这一趟会做什么（说人话：改过没有 · 删了回不回得去） |
| `token` | ← | 看到的那一份的记号 —— 卸的时候原样交回 |

错误码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `ledger_unreadable` · `not_found` · `refused`。⚠ **CLI 面也有它**（`--ext-uninstall-preview`）。

#### `ext-uninstall-apply`：从这台卸一个扩展（09-30，**写用户文件**）

```text
→ {"id":"e5","cmd":"ext-uninstall-apply","args":{"kind":"skill","name":"demo","at":{"level":"user"},"token":"…"}}
← {"kind":"reply","id":"e5","ok":true,"data":{"path":"/home/u/.claude/skills/demo","changed":["SKILL.md"],"note":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `kind` · `name` · `at` | → | 同 `ext-uninstall-preview` |
| `token` | → | 卡上那一份：现在对不上 ⇒ `stale`、一个字节不动 |
| `path` · `changed` · `note` | ← | 卸的是哪 · 删了的那几个 · 要知道的一件（挪 / 抄到了哪 · 没从装记录里摘掉 · 空目录没收掉） |

cc-monitor 装的：skill 按装记录逐文件删（带逐字节 `expect`），收掉装时建出来、此刻已空的目录，从装记录摘掉；MCP 删那一条、摘记录。
不是的：skill 目录整个挪进 `~/.cc-monitor/backups/<毫秒>-skill-<名>`（不在家目录底下 ⇒ 不删、说出来）；MCP 先把整份配置抄一份进去再删那一条。
全局的 MCP（这台有账号库）：经账号库那一侧从共用的那一份里删、同步到所有号（各号改写前的原文由那一侧留备份），不记装记录。
写都经本进程文件管理面。错误码：`bad_args` · `bad_file` · `bad_path` · `io_failed` · `ledger_unreadable` · `needs_consent` · `not_found` · `refused` · `stale`。⚠ **CLI 面也有它**（`--ext-uninstall-apply`）。

#### `aliases-render`：清单 → 代码（MIG-3a，09-28，**纯**）

```text
→ {"id":"a1","cmd":"aliases-render","args":{"aliases":[{"name":"zcc","args":["--","--account","z"],"restTo":"agent"}],"shell":"posix"}}
← {"kind":"reply","id":"a1","ok":true,"data":{"fileText":"…","lines":["zcc() { ccm \"$@\" -- --account z; }"],"problems":[],"collisions":[]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` | → | 清单：每条 `{name, args, restTo}`（`args` 是原样的 ccm argv；`restTo` = 调用时跟的词交给谁：`agent` 接在 `--` 左边（交给 agent），`ccm` 接在右边末尾 —— 只许单放 `--attach` 那一形，即 `cca`） |
| `shell` | → | `posix` / `powershell`（这台后端不在 Windows ⇒ `powershell` 拒） |
| `fileText` · `lines` | ← | 整份文件 · 每条合格别名的写法 |
| `problems` | ← | 不合格的那几条 `{name, message}`（非空时 `aliases-install` 一个字节都不写） |
| `collisions` | ← | 撞名提示（自带别名块 · **这台** `PATH` 上的同名程序 · PowerShell 内建别名；只出声、不拦） |

一个字节都不写。规则 · 方言住 `assets/aliases/`（`mod.rs` · `dialect.rs`）。错误码：`bad_args` · `refused`（这台不说那种方言）。⚠ **CLI 面也有它**（`--aliases-render`）。

#### `aliases-read`：读回清单 ＋ 启动文件候选（MIG-3a，09-28，**只读**）

```text
→ {"id":"a2","cmd":"aliases-read","args":{"shell":"posix","rcPath":null}}
← {"kind":"reply","id":"a2","ok":true,"data":{"aliasPath":"/home/u/.cc-monitor/aliases.sh","exists":true,"aliases":[…],"groups":[{"account":"z","tmux":false},null],"accounts":["z","b"],"missing":[{"account":"b","tmux":false,"alias":{…}}],"fingerprint":"41-5b6a…","unparsed":[],"rcCandidates":[…],"otherRc":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `shell` | → | 同 `aliases-render` |
| `rcPath` | → | 人另指的那一份（`null` = 不指）：过围栏（只许落在 home 之内 · 符号链接不许跑出去）后并进候选 |
| `aliasPath` · `exists` · `aliases` · `unparsed` | ← | 这台上那份别名文件的路径 · 在不在 · 读回的清单（不在 ⇒ 首建会带上的 `cc` · `cct` · `cca`，PowerShell 只有 `cc`）· 认不出的行（原文带原因） |
| `groups` | ← | 与 `aliases` 逐条对应：参数恰是「某号」或「某号 ＋ tmux」⇒ `{account, tmux}`，其余 ⇒ `null`（只看参数，不看名字；界面照这一格分组） |
| `accounts` | ← | 这台的账号表（具名号，按账号库的顺序；没有账号库 ⇒ `[]`） |
| `missing` | ← | 账号表里的号缺哪一条：`{account, tmux, alias}`（`alias` 就是点「加上」要加进清单的那一条；没有 tmux 的目标只看 `<号>cc`） |
| `fingerprint` | ← | 盘上那份别名文件的指纹（不透明的串：长度 ＋ 一个 64 位散列；不在 ⇒ `null`），存的时候交回 `aliases-install` |
| `rcCandidates` | ← | 启动文件候选（方言答列哪几份）：每份 `{path, sourced, exists, block, unreadable, policy}`，`block` = 别名块现状 `{present, version, outdated, conflictingFunctions, manualCleanupHint}`（`conflictingFunctions` = 块外自己定义的、与清单里某条同名的函数 `{name, line}`）；`policy`（只有 `$PROFILE` 那几份有）= 加载它的那一代 PowerShell 的执行策略，现问 `{host, effective, loads, groupPolicy, error}`（`host` = `powershell` / `pwsh`；`loads` = 这一档下它会不会跑这份未签名的本地文件，说不清 ⇒ `null`；`groupPolicy` = 组策略钉着） |
| `otherRc` | ← | `rcPath` 过了围栏之后的绝对路径 |

读经本进程文件管理面（`files-home` · `files-peek` · `files-stat`）。已握手的终端数不在这里（住 monitor 进程里）。错误码：`bad_args` · `refused`。⚠ **CLI 面也有它**（`--aliases-read`）。

#### `aliases-install`：写别名文件（MIG-3a，09-28，**写用户文件**）

```text
→ {"id":"a3","cmd":"aliases-install","args":{"aliases":[…],"shell":"posix","fingerprint":"41-5b6a…"}}
← {"kind":"reply","id":"a3","ok":true,"data":{"aliasPath":"/home/u/.cc-monitor/aliases.sh","wroteAliasFile":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `aliases` · `shell` | → | 同 `aliases-render`（有一条不合格 ⇒ 整批不写、`refused`） |
| `fingerprint` | → | 必给（字符串或 `null`）：读回时那份的指纹（`aliases-read` 的 `fingerprint`）。盘上此刻不是那一份（被别处改过 / 删了 / 新出现了）⇒ `stale`、一个字节不写 |
| `aliasPath` · `wroteAliasFile` | ← | 写到哪 · 真写了没有（内容一致就一个字节不写） |

写经本进程 `files-put`（读改写一次、CAS，逐级补目录、不备份：那是 cc-monitor 自己的文件）。错误码：`bad_args` · `refused` · `stale`（界面重读再让人存）。⚠ **CLI 面也有它**（`--aliases-install`）。

#### `aliases-block-render`：别名块预览（MIG-3a，09-28，**纯**）

```text
→ {"id":"a4","cmd":"aliases-block-render","args":{"rcPath":"~/.bashrc"}}
← {"kind":"reply","id":"a4","ok":true,"data":{"text":"# === cc-monitor remote ccm BEGIN v2 ===\n…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `rcPath` | → | 目标文件（方言由它的扩展名定：`.ps1` ⇒ PowerShell） |
| `text` | ← | 往一份空文件里装一次会写成什么（与 `aliases-block-install` 调同一个 `plan_install`）。块只管接入：POSIX 让 `ccm` 进 PATH ＋ 接上别名文件；PowerShell 是拉前握手 `__ccm_bind` ＋ 接上别名文件 |

错误码：`bad_args` · `refused`。⚠ **CLI 面也有它**（`--aliases-block-render`）。

#### `aliases-block-install`：别名块装进人选的那份启动文件（MIG-3a，09-28，**写用户文件**）

```text
→ {"id":"a5","cmd":"aliases-block-install","args":{"rcPath":"~/.bashrc"}}
← {"kind":"reply","id":"a5","ok":true,"data":{}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `rcPath` | → | 人选的那份启动文件（过围栏；方言由扩展名定，再过方言那一道闸） |

幂等、整块替换，块外一个字节不动；围栏损坏（有 BEGIN 没 END）⇒ 中止。写经本进程 `files-put`（带备份、逐级补目录）。
错误码：`bad_args` · `refused`。⚠ **CLI 面也有它**（`--aliases-block-install`）。

#### `aliases-block-remove`：别名块卸掉（MIG-3a，09-28，**写用户文件**）

```text
→ {"id":"a6","cmd":"aliases-block-remove","args":{"rcPath":"~/.bashrc"}}
← {"kind":"reply","id":"a6","ok":true,"data":{}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `rcPath` | → | 同 `aliases-block-install` |

整块删，块外一个字节不动；没有块 ⇒ 原样；围栏损坏 ⇒ 中止。写经本进程 `files-put`（带备份）。
错误码：`bad_args` · `refused`。⚠ **CLI 面也有它**（`--aliases-block-remove`）。

#### `aliases-to-form`：一条别名摊成表单那几格（10-02，**纯**）

```text
→ {"id":"a8","cmd":"aliases-to-form","args":{"alias":{"name":"mine","args":["--append-system-prompt","be brief","--","--account-dir","/srv/acc","--cwd","/w"],"restTo":"agent"}}}
← {"kind":"reply","id":"a8","ok":true,"data":{"form":{"name":"mine","cwdIf":[],"cwd":"/w","account":"","base":false,"tmux":"none","tmuxName":"","agent":"","model":"","launcher":"","tmuxSize":"","detach":false,"busRegister":false,"busNote":"","passthru":"--append-system-prompt 'be brief'","ccmOther":"--account-dir /srv/acc"}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `alias` | → | 一条别名 `{name, args, restTo}`（同 `aliases-render` 清单里的一条） |
| `form` | ← | 表单那几格：`name` · `cwdIf`（`[{at, to}]`，按序）· `cwd`（空 = 当前目录）· `account`（空 = 不指定）· `base`（显式不带账号）· `tmux`（`none` / `auto` / `named` / `base` / `attach`）· `tmuxName` · `agent` · `model` · `launcher` · `tmuxSize` · `detach` · `busRegister` · `busNote` · `passthru`（交给 agent 的其余参数，一串）· `ccmOther`（表单没有格子的 ccm 参数，一串） |

不会失败：参数按 ccm 自己的解析器切（最后一个 `--` 分两边 · 一组几个词 · 什么意思）；认不得的词原样进它原来那一边的那一串（左边进 `passthru`、右边进 `ccmOther`），不挪边、不丢。
`passthru` / `ccmOther` 的写法：空白分词；`'…'` 里原样；`"…"` 里 `\"` 与 `\\` 是转义；引号外 `\` 只转义空白、引号与它自己（别的照原样，`C:\work` 不用改写）。
恰是 `-- --attach`、`restTo` 为 `ccm` ⇒ `tmux` 为 `attach`、别的格都空。错误码：`bad_args`。⚠ **CLI 面也有它**（`--aliases-to-form`）。

#### `aliases-from-form`：表单拼回一条别名（10-02，**纯**）

```text
→ {"id":"a9","cmd":"aliases-from-form","args":{"form":{…},"orig":{"name":"mine","args":[…],"restTo":"agent"}}}
← {"kind":"reply","id":"a9","ok":true,"data":{"alias":{"name":"work","args":["--append-system-prompt","be brief","--","--account-dir","/srv/acc","--cwd","/w"],"restTo":"agent"}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `form` | → | 表单那几格（同 `aliases-to-form` 的 `form`） |
| `orig` | → | 必给：正在改的那一条（`aliases-to-form` 收的那一条），新增 ⇒ `null` |
| `alias` | ← | 拼出来的那一条 `{name, args, restTo}` |

按组比 `orig`：没动过的那一组原样留在原位（写法 · 顺序 · 重复都不改）；动了的换成规范写法、放在它原来第一次出现的位置；原来没有的追加（ccm 那一侧接在末尾，`--model` 放最前）。
⇒ `aliases-from-form(aliases-to-form(a), a)` 与 `a` 逐字相等，只改名字参数一个字都不变。控件上关掉的组合照样不拼（不进 tmux ⇒ 容器那几格不出现；不 `--detach` ⇒ 不登记 cc-bus；只有半边的情况不拼）。
合不合格不在这里判（那是 `aliases-render`）。`passthru` / `ccmOther` 引号没配对 ⇒ `refused`（一句人话）。错误码：`bad_args` · `refused`。⚠ **CLI 面也有它**（`--aliases-from-form`）。

#### `powershell-policy-set`：那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`（WF1，09-29，**写用户设置**）

```text
→ {"id":"a7","cmd":"powershell-policy-set","args":{"host":"powershell"}}
← {"kind":"reply","id":"a7","ok":true,"data":{"policy":{"host":"powershell","effective":"RemoteSigned","loads":true,"groupPolicy":false,"error":null},"setError":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `host` | → | 哪一代：`powershell`（5.1）· `pwsh`（7） |
| `policy` | ← | 设完现问的那一份（形状同 `aliases-read` 候选里的 `policy`） |
| `setError` | ← | 设的那一下 PowerShell 的原话（组策略压着时它会报）；`null` = 没报 |

只做一件固定的事（`Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned`），不收策略值；界面只在用户点了、确认了之后发（不代改）。
这台不说 PowerShell ⇒ `refused`。错误码：`bad_args` · `refused`。⚠ **CLI 面也有它**（`--powershell-policy-set`）。

#### `cc-bus-install-state`：装 cc-bus 到这台之前看一眼（扩展页的确认卡用，09-28，**只读**）

```text
→ {"id":"c1","cmd":"cc-bus-install-state"}
← {"kind":"reply","id":"c1","ok":true,"data":{"dest":"/home/u/.claude/skills/cc-bus","writes":["SKILL.md"],"existing":true,"version":"…"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `dest` | ← | 落点（`<skills 根>/cc-bus`） |
| `writes` | ← | 与这台二进制带着的那一份逐文件比，内容会变的那几个（缺的 ＋ 不一样的；清单外的文件不算）；空 ⇒ 已是这一版 |
| `existing` | ← | 落点上已经有东西（要写时先整个改名留作备份） |
| `version` | ← | 内嵌那一份的摘要（只答「相同 / 不同」；枢纽拿它当卡上的记号） |

枢纽经可达表问被写那台（本机那一跳不走 ssh）。错误码：`refused`（这台后端不认得带 skill 的 agent）。⚠ **CLI 面也有它**（`--cc-bus-install-state`）。

#### `cc-bus-install`：把这台二进制带着的 cc-bus 装到这台（09-28，**写用户文件**）

```text
→ {"id":"c2","cmd":"cc-bus-install"}
← {"kind":"reply","id":"c2","ok":true,"data":{"dest":"/home/u/.claude/skills/cc-bus","written":["SKILL.md","…"],"backup":null,"recordFailed":null}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `dest` | ← | 落点（`<skills 根>/cc-bus`，过独立 realpath 围栏） |
| `written` | ← | 写了的那几个相对路径（全一致 ⇒ 空：一个字节不写、不备份、不记） |
| `backup` | ← | 覆盖前整个目录改名成的那一份（`cc-bus.bak-<秒>`，`null` = 之前没装过） |
| `recordFailed` | ← | 装好了但没记进 skill 装记录时那一句（这一趟装的卸不掉）；装卸账复用 `skill-install-record` 那一份 |

写经本进程文件管理面（`files-rename` / `files-put` / `files-chmod`）。只由用户在扩展页确认卡上点确认触发（经枢纽 `ext-hub-apply`，`INVARIANTS` 第 7 条例外）。错误码：`bad_file` · `refused`。⚠ **CLI 面也有它**（`--cc-bus-install`）。

#### `tmux-list`：这台机器的 tmux 会话（SH1，09-26，**只读**）

```text
→ {"id":"t9","cmd":"tmux-list","args":{}}
← {"kind":"reply","id":"t9","ok":true,"data":{"installed":true,"sessions":[{"name":"proj-cc","path":"/home/u/proj","command":"claude","attached":false,"windows":1,"sid":"sid-1","agent":true}]}}
```

**入参：无**。与会话账本那份 tmux 观测**同一趟** `tmux ls -F`（同一段脚本、同一个格式串、同一个四态分类，`observe/watcher.rs`）：
`installed:false` = 那台没装 tmux（`sessions:[]`）；没有 server / 零会话 ⇒ `installed:true, sessions:[]`。
**出成品**（`observe/tmux_list.rs`，解析从 monitor 那一份搬来）：`sessions` 每项 `name` · `path`（`pane_current_path`）·
`command`（`pane_current_command`）· `attached` · `windows`（非数字回退 0）· `sid`（`@ccm_sid`；未设 / 不是 `[A-Za-z0-9_-]` ⇒ `null`）·
`agent`（前台命令是注册表里某一家 agent 的进程 —— Claude 是 `claude` / `node`；从前界面按画像表自己判，判定进了后端）。
段数不对的行丢掉（下溢 / 过溢各出一句日志）。界面经 `chan.call(origin, …)` 直接问（`src/frontend/ui/tmux-reads.ts`，本机远端同一形）。
错误码：`unobservable`（输出被改写 —— 段数下溢 ——、超时或起不来：**不是零会话**）· `too_large`。
⚠ **CLI 面也有它**（`--tmux-list`，不读 stdin）。

#### `session-terminals`：此刻是哪个终端在显示这个会话（10-01，**只读**；点 ↗ 时问那台一次）

```text
→ {"id":"st1","cmd":"session-terminals","args":{"sid":"9d66c46d-bf88-4f99-877e-455555555555"}}
← {"kind":"reply","id":"st1","ok":true,"data":{"terminals":[{"ssh":{"clientAddr":"10.0.0.5","clientPort":62414,"serverAddr":"10.0.0.9","serverPort":22},"activity":1790916735}]}}
← {"kind":"reply","id":"st2","ok":true,"data":{"terminals":[],"why":"detached"}}
```

入参：`sid`（会话 id 的形状）。这台按起步初扫那一份找到那个 claude 进程，然后：
- **不在 tmux 里**（它的环境里没有 `TMUX`）：它自己的环境就是答案（从那个终端的登录 shell 继承来）；进程已经没有控制终端 ⇒ `why: "no-terminal"`。
- **在 tmux 里**：问 `TMUX` 里那个 socket「现在有哪些客户端连着 `TMUX_PANE` 所在的会话」（`tmux -u -S <socket> list-clients -t <pane>`），
  逐个读那些客户端进程的环境，**最近动静在前**（tmux 的 `client_activity`）。一个都没有 ⇒ `why: "detached"`。tmux 只被问「谁连着」，不读也不设标题。

出：`terminals` 每格 `{ssh, activity}` —— `ssh` 是那个终端环境里 `SSH_CONNECTION` 的四段（对面地址 · 对面端口 · 本机地址 · 本机端口；
没有 / 认不出 ⇒ `null`，那个终端不是经 ssh 连的）·
`activity` 是最近动静（Unix 秒，不在 tmux 里 ⇒ `null`），最多 16 格。读不了那个进程 / 那些客户端的环境（别的用户 · 权限）⇒ `why: "unreadable"`。
只读写死的那几个键（`TMUX` · `TMUX_PANE` · `SSH_CONNECTION`），不回整份环境。零定时器：只在被问时答。
码：`bad_args`（缺 `sid` / 形状不对）· `no_such_session`（这台没有在跑的这个会话）· `failed`（tmux 起不来 / 报错，带它的原话）。只上帧面（`STREAM_ONLY`）。

#### `terminal-processes`：那台报来的终端连接是这台电脑上哪个进程开的（10-02；↗ 的本机一半）

```text
→ {"id":"tp1","cmd":"terminal-processes","args":{"terminals":[{"ssh":{"clientAddr":"10.0.0.5","clientPort":62414,"serverAddr":"10.0.0.9","serverPort":22},"activity":1790916735}]}}
← {"kind":"reply","id":"tp1","ok":true,"data":{"chain":[{"pid":700,"name":"ssh.exe","start":133722000000000000},{"pid":600,"name":"powershell.exe","start":133721990000000000},{"pid":500,"name":"WindowsTerminal.exe","start":133721980000000000}]}}
← {"kind":"reply","id":"tp2","ok":true,"data":{"chain":[],"why":"elsewhere","addr":"203.0.113.8"}}
```

入参：`terminals` —— 那台 `session-terminals` 回话里那一格原样（1–16 格；每格要有 `ssh`：四段或 `null`，别的格不看）。
这台**只做一趟只读的系统查询**（`powershell.exe -NoProfile -NonInteractive`，固定脚本、不吃入参）：`Get-NetTCPConnection -State Established`
（两端地址与端口 ＋ 拥有者进程号）＋ `Get-CimInstance Win32_Process` **只取四格**（进程号 · 父进程号 · 名字 · 启动时刻）。
不读任何进程的命令行、不读别的进程的内存。然后按交来的顺序逐格试，第一个对上的就是它：
- 四元组全等（本机地址 = `clientAddr`、本机端口 = `clientPort`、对面 = `serverAddr`:`serverPort`；IPv4 映射的 IPv6 认成 IPv4，作用域去掉）⇒
  拥有那条连接的进程，往上数父进程，到桌面外壳（`explorer.exe`）之前为止；父进程不在表里 / 比子进程晚起（进程号被复用过）⇒ 链在那里断。
- 对不上 ⇒ 原因只看这台的连接表：这台有 `clientAddr` 这个地址、或这台有 `ssh.exe` 连着 `clientAddr` ⇒ `mismatch`（经跳板机 / 端口转换，这台电脑对不上）；
  都没有 ⇒ `elsewhere`（带 `addr` = 那台看到的对面地址）。`ssh: null` ⇒ `not-ssh`。都对不上 ⇒ 报第一格的原因。

出：`chain` 每格 `{pid, name, start}`（开着那条连接的进程在前；`start` 是启动时刻 FILETIME，0 = 系统没给）；对不上 ⇒ `chain: []` ＋ `why`
（`not-ssh` · `elsewhere`（＋ `addr`）· `mismatch` · `query-failed` —— 这台没有 PowerShell / 查询报错 / 两张表对不上，原话进日志）。
窗口那一跳不在这里（归 monitor）。码：`bad_args`（缺 `terminals` / 空 / 超 16 格 / 某格缺 `ssh` / 地址端口认不出；验不过不去问系统）。只上帧面（`STREAM_ONLY`）。

#### `terminal-ssh`：给一台远端开终端要跑的那一串（09-28；「待迁」最后一行：ssh 外壳与 PowerShell 窗口载荷由本机后端渲，monitor 只开终端）

```text
→ {"id":"ts1","cmd":"terminal-ssh","args":{"machine":{"host":"pi.local","user":"pi","port":22,"label":"pi"},"command":"claude --resume s1"}}
← {"kind":"reply","id":"ts1","ok":true,"data":{"command":"& ssh -t -p 22 pi@pi.local -- 'bash -lic ''claude --resume s1'''"}}
```

入参：那台机器的配置 `machine`（＋ `saved?` · `jump?` · `prefer?`，与 `remote-probe` / `pubkey-push` 同形，组请求走 `dial/machine.rs::resolve`）＋ 要在那台跑的
`command`。`command` 也可以换成意图 `cwd`（**恰给一格**）：
当前目录的线上形（字符串或 `{"b16": …}`，同 `files-*` 的路径），由后端拼成 `cd <目录> && exec ${SHELL:-bash} -l`（空 ⇒ 只有后半段；目录过 POSIX 自由文本路径那一关，
不过 ⇒ `refused`）—— 文件窗口「在此打开终端」经 monitor 接下的通道那一问（`terminal-open`）走这一形，窗口不拼命令。出：一行 PowerShell `& ssh -t[ -J <跳板用户>@<跳板>[:口]] -p <口>[ -i '<钥匙>'] <用户>@<地址> -- '<bash -lic ''…''>'` —— 地址取竞速顺序第一条
（交了 `prefer` 且仍在这台的地址里 ⇒ 上次赢的那条）；命令包成 `bash -lic '<命令>'` 再以 PowerShell 单引号字面量嵌入；钥匙尾 `\` 剥掉；没钥匙 ⇒ 不带 `-i`（走 agent）。
**只算不起**：不拨号、不开窗（开窗是 monitor 的事）。码：`invalid_args`（缺 `machine` / `command`）· `bad_jump`（跳板交不来 / 指自己）·
`refused`（命令空 / 超长 / 含控制符 / 含双引号 —— PowerShell 5.1 传参畸变面；用户名 · 地址 · 跳板出了白名单）。只上帧面（`STREAM_ONLY`）。

#### `tmux-name-mint`：起会话要的 tmux 名（09-28；「派生 ＋ 避让只留后端」）

```text
→ {"id":"tm1","cmd":"tmux-name-mint","args":{"cwd":"/home/u/proj"}}
← {"kind":"reply","id":"tm1","ok":true,"data":{"name":"proj-cc-2"}}
→ {"id":"tm2","cmd":"tmux-name-mint","args":{"forkOf":"proj-cc"}}
← {"kind":"reply","id":"tm2","ok":true,"data":{"name":"proj-fork-cc"}}
```

**入参恰给一格**：`cwd`（起新会话 / 全新 resume：基名 `<项目名>-cc`，派生规则同 `ccm` 不给名时那一条）或 `forkOf`（分叉：源会话的 tmux 名，
源已退出时交它的 cwd；基名 `<去掉末尾 -cc 再净化>-fork-cc`）。净化：取末段路径 → 非 `[A-Za-z0-9_-]` 换 `-` → 折叠 → 截 32 → 剥首尾 `-`；
空 ⇒ `session-cc` / `session-fork-cc`。撞了往后排（`-2` / `-3` …），避让问的是**这台**那张会话快照（问一次更新一次，与 `ccm` 起会话、
`--ccm-print` 同一份）；这台没装 tmux ⇒ 交基名。**只算不起**：不建会话、不写盘。前端 `src/frontend/ui/tmux-name-mint.ts` 问（本机远端同一形），
问不到 ⇒ 不铸名（空集铸名 = 不避让，issue #76 的形状）。错误码：`invalid_args`。只上帧面（`STREAM_ONLY`：CLI 那一侧 `ccm` 自己就铸）。

#### `ssh-config-aliases`：这台 `~/.ssh/config` 里可点的别名（MIG-1，09-27，**只读**）

```text
→ {"id":"s1","cmd":"ssh-config-aliases","args":{}}
← {"kind":"reply","id":"s1","ok":true,"data":{"aliases":["aya-lan","aya-wan","pi"]}}
```

**入参：无**。只看 `Host` 行：非通配（`*` / `?` / `!` 开头的不要）、去重保序；不展开 `Include`、不解析 `Match`；别的指令的值一个都不出线。
文件不在 / 读不了 ⇒ `aliases:[]`（没有 config 是正常的）。：解读与拨号同一个家（`dial/ssh_config.rs`），界面问 `<local>`。

#### `ssh-config-resolve`：一个别名的有效连接参数（MIG-1，09-27，**只读**）

```text
→ {"id":"s2","cmd":"ssh-config-resolve","args":{"alias":"aya-lan"}}
← {"kind":"reply","id":"s2","ok":true,"data":{"host":"10.0.0.2","port":2222,"user":"zbl","keyPath":null,"proxyJump":"bastion"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `alias` | → | 必填。过 allowlist（`[A-Za-z0-9._@:-]`）且不以 `-` 开头，否则 `bad_alias` |
| `host` / `port` / `user` | ← | `ssh -G` 的 `hostname`（缺省回退别名）/ `port`（缺省 22）/ `user`（缺省空串） |
| `keyPath` | ← | 第一个**展开后存在**的 `identityfile`（只问在不在，不读内容）；都不存在 ⇒ `null` |
| `proxyJump` | ← | `proxyjump`（`none` ⇒ `null`） |

系统 `ssh -G` 只读配置、不建连接。错误码：`invalid_args`（缺 `alias`）· `bad_alias` · `failed`（起不来 / 退出非 0，原话带在 message 里）。

#### `ssh-config-import`：批量导入预览（MIG-1，09-27，**只读**）

```text
→ {"id":"s3","cmd":"ssh-config-import","args":{}}
← {"kind":"reply","id":"s3","ok":true,"data":{"groups":[{"label":"aya","host":"10.0.0.2","port":2222,"user":"zbl","keyPath":null,"addresses":["aya.example.com:22"],"jump":"bastion","members":[{"alias":"aya-lan","host":"10.0.0.2","port":2222,"proxyJump":"bastion"},{"alias":"aya-wan","host":"aya.example.com","port":22,"proxyJump":null}]}]}}
```

**入参：无**。全部别名逐个 `ssh-config-resolve`（解析失败的跳过），同 `(keyPath, user, 基名前缀)` 的聚成一组 = 同一台机器的多个地址：
`label`（单成员组 = 完整别名，多成员 = 基名）· `host` / `port` / `user` / `keyPath`（组首）· `addresses`（其余地址，端口不同则 `host:port`）·
`jump`（组内首个非空 proxyjump）· `members`（`alias` / `host` / `port` / `proxyJump`，界面「拆分」时据此还原）。

#### `forward-start`：起一条本地端口转发（MIG-1，09-28，F58 `-L`）

转发账住本机常驻后端（`dial/forwards.rs`），界面问 `<local>`；monitor 那三条 Tauri 命令退役。

```text
→ {"id":"f1","cmd":"forward-start","args":{"origin":"dev","localPort":15432,"remoteHost":"localhost","remotePort":5432}}
← {"kind":"reply","id":"f1","ok":true,"data":{"id":"fwd-1"}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `origin` | → | 那台的名字（可达表 `remote-reach` 的键：那台的流握手过 ⇒ 用那一份拨号请求） |
| `machine` / `saved?` / `jump?` | → | 那台的配置（界面 `remote-config` 那一格，camelCase）· 已保存的那一份 · 跳板那一台：可达表里**没有**那台（流没起来）时按它自己组请求去拨（`dial/machine.rs`，同 `remote-probe`），不拒 |
| `localPort` / `remoteHost` / `remotePort` | → | 绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip |
| `id` | ← | 这条转发的号（`fwd-<n>`，本进程内单调） |

先过围栏（端口 0 / 远端 host 空白 ⇒ `bad_spec`，在任何查表 / 拨号之前），再查可达表（没有 ⇒ 按 `machine` 组），再在池里那条 SSH 上开 `use: forward` 链路、**等 ack**：口绑好了、连上了才回成功（才进账）。
错误码：`invalid_args`（缺格 / 类型不对）· `bad_spec` · `unreachable`（可达表里没有那台、也没交 `machine`）· `bad_jump`（跳板没交来 / 指自己）· `full`（账上已满 64 条）· `failed`（ack 说不成，原话带在 message 里）。
只有帧面（`cli_control::STREAM_ONLY`）：账住常驻那一个进程，一次性进程开出来的转发随进程退出就没了。

#### `remote-probe`：测试连接（MIG-1 续，09-28）

「后端持有全部 SSH」：界面把设置页表单里那台（**可能还没保存**的）配置交过来，本机后端组拨号请求（`dial/machine.rs`：
地址四形态 · 指纹只继承同一个 host 的 · 跳板查无 / 环都拒）、**拨一次**（短命探活，不进连接池）、回结局。monitor 的 `test_remote_connection` 退役。

```text
→ {"id":"p1","cmd":"remote-probe","args":{"ticket":"6f1c…","machine":{"host":"10.0.0.2","label":"aya","port":22,"user":"u","keyPath":"","hostKeyFingerprint":"","addresses":[],"jump":""},"saved":null,"jump":null}}
← {"kind":"probe","ticket":"6f1c…","cell":{"stage":{"kind":"dialing","endpoint":"10.0.0.2:22"}}}
← …（握手那几行各一格）
← {"kind":"probe","ticket":"6f1c…","cell":{"reached":"ssh"}}
← {"kind":"probe","ticket":"6f1c…","cell":{"reached":"hello"}}
← {"kind":"probe","ticket":"6f1c…","cell":{"reached":"control"}}
← {"kind":"probe","ticket":"6f1c…","cell":{"end":{"sshOk":true,"fingerprint":"SHA256:…","endpoint":"10.0.0.2:22","backendOk":true,"backendHello":"版本 p5o · 能用 40 项、这台做不到 3 项 · 往返 12 毫秒","backendGaps":[{"code":"no_tmux","count":3}],"message":"SSH 与后端均正常。"}}}
← {"kind":"reply","id":"p1","ok":true,"data":null}
```

三步：拨号 ＋ 鉴权 ＋ exec 那台后端（流模式）→ 读首行 hello → 那台认 `ping` ⇒ 同一条流上往返一次。本后端零定时器：**期限归发起方**（界面给 15 s，到点撤单）。
〔「进度不许倒退」〕**边拨边推**：每走一段往本连接的应答通道推一帧 `probe`（见出方向那张表），`cell` 恰好一个键 ——
`stage`（拨号阶段行，与界面 `ConnectStage` 同形）· `reached`（`ssh` 握手过了 · `hello` 那台后端回了 hello · `control` ping 往返了）· `end`（结局，**最后一格**）；
应答本身不带体。`ticket` 是界面交来的票（1..=64 个 `[A-Za-z0-9-]`，进度流 `probe-progress/<ticket>` 的名字），本后端只当不透明的串回填。
结局里每步结论都在（部分成功照样回）：`sshOk: false` 时**不回指纹**（免得把失配的 key 固化）。`backendHello` 是那台后端的三格人话（版本 `build_id` · 能用几项 / hello 的 `unavailable` 说做不到几项 · `ping` 往返毫秒；按文案表 `beProbe.hello.*` 拼），`backendGaps` 是做不到的那几类（`[{code, count}]`，按 hello 的 `unavailable` 分；码的人话归 monitor，与置灰那一句同一个家 `control-said.ts::unavailableReason`，界面点开看）；`v=… build=… caps=[…]` 那一形只进后端日志（`wire::hello_summary`）。界面到点没等到 `end` ⇒ 最后收到的那一格说得出停在哪一段。
错误码：`invalid_args`（缺 `ticket` / `machine` / 缺 host · user / 端口不对）· `bad_jump` · `failed`（链路那一侧回话读不懂 · 发起它的那条连接关了）。**只在帧面**（硬臂：要拿本连接的应答通道）。

#### `forward-stop`：停一条转发（MIG-1，09-28）

```text
→ {"id":"f2","cmd":"forward-stop","args":{"id":"fwd-1"}}
← {"kind":"reply","id":"f2","ok":true,"data":{"id":"fwd-1"}}
```

从账上摘掉 ⇒ 那条链路被收 ⇒ 放掉本地口、收掉在飞隧道（到那台的 SSH 连接是共用的，不断）。错误码：`invalid_args`（缺 `id`）· `not_found`。只有帧面。

#### `forward-list`：列转发（MIG-1，09-28）

```text
→ {"id":"f3","cmd":"forward-list","args":{}}
← {"kind":"reply","id":"f3","ok":true,"data":{"forwards":[{"id":"fwd-1","origin":"dev","localPort":15432,"remoteHost":"localhost","remotePort":5432,"state":"running","connCount":3}]}}
```

**入参：无**。`state`：`running`（链路还在）/ `error`（链路自己收工了：远端断开 · 本地口 accept 失败；留在账上等用户停）。
`connCount`：累计接进的连接数（链路那一侧每接一条报一行，照抄）。账是本进程一张：界面重开之后列得出上次开的；后端重启 ⇒ 空。只有帧面。

#### `resync`：手动对齐（RESYNC，09-27）

```text
→ {"id":"r1","cmd":"resync","args":{}}
← {"kind":"reply","id":"r1","ok":true,"data":{"added":0,"removed":1,"retagged":1,"caught_up":2,"watchers":1,"unavailable":[],"uncancellable":["launch"]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 可选。给了 ⇒ 只对这一个会话（关卡 2「对齐后重试」：重验 ＋ 重打标签）；不给 ⇒ 整机。给了却不是非空字符串 ⇒ `bad_args` |
| `added` / `removed` | ← | 这次补宣告 / 补移除的会话数（各份 watcher 取最大：看的是同一台机器） |
| `retagged` | ← | 这次真写了几处 `@ccm_sid`（值一样的不写） |
| `caught_up` | ← | 在跟的会话这次从游标补读出几行（带 `sid` 只数那一个；各份 watcher 相加）。补出来的行照常经流到达（REREAD） |
| `watchers` | ← | 几份 watcher 做完了对齐（常驻后端每条连接一份 ＋ 空转那一份）。只有帧面，没有 CLI 面 |
| `unavailable` / `uncancellable` | ← | 这台**当下**的能力事实，与 hello 那两格同一个函数、同形（`[{command, code}]` · `[op]`）。monitor 拿它换掉握手那一刻的 `Offer`（例：握手之后才装上 tmux） |

整机那一趟与起步初探是**同一套**：耳朵重挂 · 账号清单（发一帧 `accounts_changed`）· pidfile 目录对后端的表（多的补 `session_added`，少的补 `session_removed`，在跟的顺手对账标签）· 重探 tmux。只对差异发帧；不另发 `sessions_replayed`。
错误码：`bad_args`。

#### `hooks-diag`：cc-bus 钩子诊断（MIG-3b，09-27；**不读 stdin**）

```text
→ {"id":"h1","cmd":"hooks-diag","args":{}}
← {"kind":"reply","id":"h1","ok":true,"data":{"diagnosis":{"session_start":{"kind":"installed-via-path","command":"cc-register"},"stop":{"kind":"not-installed"},"note":""},"snippet":"{…}","source":"/home/u/.claude/settings.json","supported":true}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `diagnosis` | ← | `session_start`（→ `cc-register`）· `stop`（→ `cc-bus-stop-hook`）各一态 ＋ `note`（读不到 / 坏 JSON / 顶层不是对象时说原因，否则空串） |
| `session_start` / `stop` / `note` | ← | 见上一行 |
| `kind` | ← | 一态：`not-installed` · `installed-via-path` · `installed-at-path` · `path-missing` · `unknown`；除第一态都带 `command`，两种显式路径态另带 `path` |
| `command` / `path` | ← | 钩子原文（去首尾空白）· 它点名的路径（原样，`$HOME` 未展开） |
| `supported` | ← | 这台跑得了 cc-bus（它要 tmux；这份后端编到的平台没有原生 tmux ⇒ `false`，`snippet` 恒 `null`，界面只说这台不支持自动收信） |
| `snippet` | ← | 要合并进那份文件的内容：两条钩子直接指向这台 `<skills 根>/cc-bus/scripts/` 里那两个脚本（家目录底下写 `"$HOME/…"`，否则绝对路径），不依赖 `PATH`；那两个脚本不在（cc-bus 没装）⇒ `null` |
| `source` | ← | 读的是哪份文件：这台后端的 agent 配置根下的 `settings.json` |

本体 `observe/cc_bus_hooks.rs`：读这台自己的 `settings.json`（只读，不写）、按这台的 `HOME` 展开 `$HOME/…` 就地 stat。界面在扩展页 cc-bus 那一行的抽屉里每台问一次。本机远端同一条（monitor 那两条本机 / 远端各一条的诊断 Tauri 命令删了）；界面按形状严格收，线上形状由跨语言金样 `tests/__fixtures__/hooks-diag.golden.json` 钉住。
错误码：`failed`（序列化失败）· `too_large`。

#### `tasks-list`：一个会话的任务列表

```text
→ {"id":"t1","cmd":"tasks-list","args":{"sid":"0c1d…"}}
← {"kind":"reply","id":"t1","ok":true,"data":{"tasks":[{"id":"1","subject":"…","status":"in_progress","blocks":[],"blockedBy":[],"activeForm":"…"}, …]}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `sid` | → | 会话 id。只许一段普通路径名（空 / 含分隔符 / `.` / `..` ⇒ `bad_args`） |
| `tasks` | ← | **成品**：`<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序（此前是原样对象的 `lines`，字段由 monitor 解） |
| `id` / `subject` / `status` | ← | 每格必有、是串（缺 / 不是串的那个对象不算任务，跳过） |
| `description` / `activeForm` | ← | 可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串 |
| `blocks` / `blockedBy` | ← | 串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务） |

字段语义只住后端 `observe/tasks_query.rs::task_entry`；界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 两端契约对不上），线上形状由跨语言金样 `tests/__fixtures__/tasks-list.golden.json` 钉住。

- 那个 sid **没有任务目录** ⇒ 空 `lines`（诚实的空）；目录**在但读不了** ⇒ `failed`（不说成「没有任务」）。
- 半截 / 解不成对象的文件跳过（写者持锁那一刻读到半截是正常时序）；单个文件超过 1 MiB ⇒ 跳过并 `warn!` 点名。

#### `backend-log`：这台后端的 stderr 诊断文件尾部（2026-09-26）

```text
→ {"id":"q20","cmd":"backend-log","args":{"maxBytes":262144}}
← {"kind":"reply","id":"q20","ok":true,"data":{"path":"/home/u/.cc-monitor/logs/backend/stderr.log","size":81920,"text":"…","truncated":false}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `maxBytes` | → | 可选，缺省 = 封顶 256 KiB：只回尾部这么多字节 |
| `path` | ← | 本进程 stderr 此刻落在的那份文件；没装（stdio 载体 · 没被交路径）⇒ `null` |
| `size` | ← | 那份文件的总字节数 |
| `text` | ← | 尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行 |
| `truncated` | ← | 前面还有没回的字节 |

**为什么**：远端常驻后端的 stderr 落它自己那台的 `~/.cc-monitor/logs/backend/stderr.log`（`--resident-ensure` 交路径），
而那台没有人看得见；机器页「日志」经这台后端的只读面取回来看（不另开通道）。只读，不写盘。CLI 面随之自动多一条 `--backend-log`。

#### `history-find`：会话内查找（2026-09-24 上帧面）

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
| `total` / `hits` | ← | **成品**（口径见 §10.5）：`hits` 与 `--find-in-session` 的 stdout **中段逐行相同**（同一个扫描；头尾只属于 CLI 那一臂），`total` = 全量命中数 |

**为什么上帧面**：与上面两条同一个处境（新子命令、此前在远端逐次拨号）；用户每按一次 Enter 就要一次。
CLI 面随之自动多一条 `--history-find`。

#### `history-facts`：会话事实（阶段 C）

```text
→ {"id":"q12","cmd":"history-facts","args":{"path":"/home/u/.claude/projects/-p/s.jsonl"}}
← {"kind":"reply","id":"q12","ok":true,"data":{"end":5120088,"forkedFrom":null,"projectDir":"/p","touchedFiles":["/p/a.ts"],"usage":{"promptTokens":41250,"model":…}}}
→ {"id":"q13","cmd":"history-facts","args":{"path":"…/s.jsonl","prior":{上一次的 data 原样}}}
```

| 字段 | 向 | 说明 |
|---|---|---|
| `path` | → | jsonl 路径（围栏同 `history-read`） |
| `prior` | → | 可选：**上一次应答的 `data` 原样**（续传令牌）。缺席 / `null` ⇒ 从字节 0 扫；给了 ⇒ 从它的 `end` 接着扫、把新的一截累加在它上面（后端零状态）。形状必须恰好是本命令出的那一形（缺格 / 多格 / 类型不对 ⇒ `bad_args`）。它的 `end` 越过文件尾、或不在行边界上（文件第 `end-1` 字节不是换行）⇒ `failed`（文件被截断或重写过；调用方从 0 重要一份） |
| `end` | ← | 最后一个完整行的末字节 |
| `forkedFrom` | ← | 源会话 sid：首条带 `forkedFrom`（`sessionId` 与 `messageUuid` 都是串）的 user / assistant 记录；不是分叉来的 ⇒ `null`。判定与 `history-sessions` 行的 `forkedFromSessionId` 是同一个函数 |
| `touchedFiles` | ← | 写类工具（Edit / Write / MultiEdit → `file_path`，NotebookEdit → `notebook_path`）碰过的文件，原样、去重、近因序（最近碰的在末尾），至多 1000 条（超 ⇒ 丢最久没碰的） |
| `usage` | ← | 文件序最后一条 `input_tokens + cache_creation_input_tokens + cache_read_input_tokens > 0` 的 assistant 记录 ⇒ `{promptTokens, model}`（`model` 缺 ⇒ `null`）；一条都没有 ⇒ `null` |
| `projectDir` | ← | 会话的项目目录：适配层读记录开头给（与 `session_added.project_dir` 同一个函数），读到即锁定；开头里还没有 ⇒ `null`（下一次再读） |

- 本体 `observe/facts_query.rs`（claude 的写类工具表也住那里：进适配层会让「加一个 agent 通用层要改几处」那只许降的棘轮涨一格）。子 agent 的列表与状态不在这里：那是运行表（`session_runs`），判定只有那一处。
- 整份超过 32 MiB ⇒ `too_large`（不截断）。界面经通道直接问（`src/frontend/ui/session-reads.ts`），本机与远端同一条路；老后端不认 ⇒ `unsupported`（界面说「不可用」，不当成空）。
- CLI 面随之自动多一条 `--history-facts`（stdin 一段 JSON ＝ `args`，stdout 一行 JSON ＝ `data`）。

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

##### ★ 跨仓承诺 —— **这一格的字节不许随手动**

仓内**零调用方**（monitor 那边唯一的调用方随 `K-R48` 删了），仓外 aterm 当时也说「暂不消费」；
**留着**：它是**给仓外 aterm 的承诺**，契约冻结在 **2026-07-18**（与上面那段说的是同一份），**随时可能开始被消费**。
⇒ 下面四行列表是那份承诺的全部线上形状，**改任何一格 = 一次跨仓契约变更**，要同轮做三件事：① 改代码；② 改本节与冻结金样 `tests/__fixtures__/resolve-contract.golden.json`；③ **bump `BUILD_ID`**（已部署的后端得被判 stale 重装）—— 并且先问 aterm 那边。
两条入口**都算承诺的一部分**：流命令 `resolve`（`inbound::REGISTRY`）与一次性 `--resolve`（`main.rs` 分派 ＋ `SUBCOMMANDS`），同一个纯函数、差别只在信封（上表）。
〔用户 09-27〕**命令行形状变了**：后端二进制就是 `ccm`（`~/.cc-monitor/bin/ccm`），叫它的一次性子命令写成 `ccm -- --resolve`（`ccm -- --fork-session <sid> <uuid>` 同理）—— 没有打头的 `--` 整行原样交给 claude。
名字不是 `ccm` 的开发树二进制 `-- --resolve` 与裸 `--resolve` 都认。**aterm 仓要跟着改成 `ccm -- …` 形**（本仓不碰那个仓）；stdin / stdout 的线上形状一格没变。
钉它的判据：`resolve_query_tests.rs` 里带 `` 的那一族（样例逐字节 · 入参字段 · 错误码全集 · 两条入口 · 本节四行列表与金样两向相等）。

**hello 那一帧对 aterm 只做 additive**：`unavailable` · `uncancellable` 两格都是空表省略、有值落在既有字段之后；aterm 的 `parseHello` 按通用 map 解、只取它认得的键、未知字段忽略（只读核过）⇒ 不算本节意义上的契约变更（仍 bump `BUILD_ID`，已部署的后端得换成会发它们的那一版）。

- **入参**（stdin / `args`，camelCase）：`sessionId` · `launchCandidates` · `claudeDir` · `fallbackCwd` · `alreadyInTmux` · `agentKind`
  （`sessionId` 必填；其余缺省。`claudeDir` · `fallbackCwd` · `alreadyInTmux` 今天读进来不用 —— 字段照样冻结，不许改名。
  `agentKind` 缺 / 空 ⇒ 注册表里声明默认的那一家；注册表里没有的名字 ⇒ `bad_request`，那句话列出认得的几家，不落默认）
- **出参**（stdout 一行紧凑 JSON / `data`）：`command` · `mode` · `capabilities` · `sessionName` · `launchLabel` · `substitutedFrom`
  （后三个缺席即省略；`mode` 今天恒 `PtyInject`，另一个保留值 `ExecOnce`；可信度见 `§10.1`）
- **`capabilities` 四名**（逐字复用 aterm `SessionCapabilities`）：`supportsSendKeys` · `supportsCapture` · `supportsMultiClient` · `supportsMultiWindow`
- **错误码**（一次性那条 stderr 一行 `{code, message}` ＋ 退出码 **2**；流那条走 `ok:false` 的 `code` / `message`）：`stdin_read_failed` · `bad_request` · `invalid_session_id` · `unsafe_launch_candidate` · `serialize_failed`
  （`stdin_read_failed` 只有一次性那条会出；`bad_request` 与协议级那个同名，**刻意不改**：改它就破了这份冻结契约）

#### 链路四条（2026-09-24）—— **本机只常驻一个后端，所有 SSH 连接由它持有并复用**

单一常驻后端：monitor 不再每条链路起一个 `--dial` 子进程（C2 那一版），而是经它与**本机常驻后端**之间
**这条已有的流**开「链路」。后端把到同一台远端的所有链路**复用在同一族 SSH 连接上**（按拨号身份：`host · port · user ·
key_path · host_key_fingerprint · 竞速地址 · 跳板`；默认一条、按需多开至多 3 条；最后一条链路走了连接就断，没有空闲定时器）。

〔2026-09-24，用户「今天每台机器只有一条连接可以看情况多开. 智能一点」〕**一族连接怎么多开、怎么收**（`dial/pool.rs`）——
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
  闸 `RUSSH_ZLIB_SOUND` **开着**：上游 russh 0.61 的 zlib 解压一包最多交出约两倍包长（真 sshd 上一开压缩第一条通道就卡死），
  后端链的是仓内补过的副本（`Cargo.toml` 的 `[patch.crates-io]` → `src/vendor/russh`，改了哪几行见那里的 `VENDOR.md`）。
  协商偏好序：压 ⇒ `zlib@openssh.com, zlib, none`（远端关了压缩照样连得上）；跳板自己那条永远不压，答案给隧道里的目标。

**一条链路上的字节 = C2 拨号代理原来的 stdout，逐字节同形**：`stages=true` 时若干行 `{"stage":{…}}` → **恰好一行** ack
`{"ok","error","fingerprint","endpoint","v":2,"uses":[…]}` → `stream` 原样双向字节 · `capture` 一行 `{"stdout","stderr","exit_status"}` 后结束 ·
`forward` 每接进一条连接一行 `{"accepted":n}` · `tunnel` 原样双向字节（远端 `127.0.0.1:<tunnel_port>` 那条 direct-tcpip）。上行（`link-data`）= 原来子进程的 stdin；`link-close` = 原来「界面走了」。

**流控**：下行逐链路信用 —— `link-open` 给初始窗口，后端发一块扣一块，扣不到就等；客户端读走之后 `link-credit` 还回来
⇒ 一条不读的链路在这条流上最多占一个窗口，堵不住别的链路、别的帧与应答。上行一次一块：`link-data` 的应答在那块**写进链路之后**才回。
**链路属于开它的那条流连接**：连接没了（monitor 走了）⇒ 它开的链路全部收掉。四条都是 `Run::Builtin`，**只在帧面**（CLI 面不派生：一次性进程没有「连接」可言）。

#### `link-open`：开一条链路

| 方向 | 形状 |
|---|---|
| `args` | `{"link":"<不透明 id，客户端给、客户端负责唯一>","window":<初始信用，字节；必须在 [32 KiB, 16 MiB] 之内，否则 `invalid_args`>,"dial":{DialRequest}}` |
| `data` | 无（登记上、任务起了就回 `ok` —— **不等拨通**：拨通与否在链路字节里那一行 ack） |

`dial` 就是 C2 那份蛇形键请求：`host · port · user · key_path · host_key_fingerprint · command · endpoints · jump · use（stream｜capture｜forward｜files｜tunnel）·
capture{max_bytes,abort_marker,stdin} · forward{local_port,remote_host,remote_port} · tunnel_port · stages · probe`，外加 **`agent_sock`**（Unix：客户端此刻的
`SSH_AUTH_SOCK` —— 常驻后端活得比任何一个客户端都长，它自己身上那份可能早就不指向活的 agent；缺席 = 用后端自己的环境）。
`capture.stdin`（可缺）：exec 之后原样写进远端进程 stdin 的字节，**不关 stdin**（收的一侧用 CLI 面的 `--stdin-line`）。
`probe` / `stages` 的链路**不进连接池**（测试连接要看的就是一次真拨号）。
`use:"files"`：在池里那条连接上开 sftp 子系统（`dial/sftp.rs`），ack 之后**一问一答** ——
上行每一行一个请求 `{"op":…}`，下行每一行一个应答；`op` ∈ `home` · `read{path,max}` · `put{path,size,mode,verify}`（该行之后紧跟 `size` 个原始字节，
上限 64 MiB）· `remove{path}` · `mkdirs{path}`（`stat` 那一问删了：唯一的问者随部署判定进了本机后端）。失败那一形 `{"code","message"}`，`code` ∈ `fenced`（远端写围栏拒）· `io` · `too_big` · `bad_request` · `unknown_op`。
🔴 **写只许两处**：远端 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`（用户；`INVARIANTS §41.6` 的 SR1b 订正）。读不受限。
它服务自部署（F08 后端二进制）：后端二进制那一路的判定在本机常驻后端（`deploy-plan`），monitor 照计划经它放字节（`dial_host::RemoteFs`）；账号库不经这里部署，由那台后端自己建（`accounts-init`）。
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

#### 传输四条（2026-09-24）—— **传输台住本机常驻后端，SFTP 与其它 SSH 同一条连接**

用户「SFTP 进本机常驻后端，只写暂存区」：传输台从界面进程搬进本机常驻后端；
窗口那一侧的 `call(transfer-upload | transfer-download)` / `subscribe(transfer/<id>)` 一个字不变，monitor 只做中继。
**上传只写远端暂存区** `~/.cc-monitor/staging/<key>.part`（落进用户目录由远端后端 `files-commit-upload` 做）；**下载远端只读**，
本机落点 `<落点>.part` ＋ 改名上位（本机那一下写是文件管理那一面的写，先过路径解析；会话文件围栏拿掉了）。
存亡规矩：**撤** ⇒ 上传删暂存件、下载留 `.part`；**失败** ⇒ 上传留暂存件、下载**也留** `.part`（弱网断线就是失败，删了续传的本钱就没了；一个字节都没落的空 `.part` 才清）；续传两侧都先对尾块。上传失败之后先把已发出的写全部等到回话再走（不留晚到的写）。
票表**每条流连接一张**：连接没了（monitor 走了）⇒ 在册的一律撤。四条都是 `Run::Builtin`，**只在帧面**。
一条连接上的通道预算按连接记：session 通道 8 格（长流 · 查询 · sftp 同一道闸），其中传输至多 4 格（排队，不报错）。

#### `transfer-upload`：开单（上传）

| 方向 | 形状 |
|---|---|
| `args` | `{"dial":{DialRequest，同 link-open},"local_path":"<本机一份普通文件的路径>"}` |
| `data` | `{"id":"xfer-<n>","key":"<32 位十六进制>"}` —— `key` = 暂存件的键（本机路径 · 大小 · 修改时间派生；同一份文件重拖一次同一个键 ⇒ 续传），提交时交给远端后端 |

**不起跑**。错误 code：`bad_args` · `io_failed`（读不到本机文件）· `busy`（同一个键已有一张票在册）· `too_many_transfers`（每连接 64 张）。
`args` 可带 `"home":"<那台后端的 $HOME>"`：起跑连上之后与 SFTP 起始目录（`realpath(".")`）比，去尾 `/` 不等 ⇒ 一个字节不写，
终局 `{"state":"failed","why":…,"code":"sftp_home_mismatch"}`（chroot / `internal-sftp -d`：暂存件会落到后端看不见的地方）。

#### `transfer-download`：开单（下载）

| 方向 | 形状 |
|---|---|
| `args` | `{"dial":{…},"remote_path":"<远端路径>","local_path":"<本机落点，绝对路径>"}` |
| `data` | `{"id":"xfer-<n>"}` |

本机落点**当场**过路径解析（绝对路径 · 有文件名 · 父目录在盘上；出声早），起跑时再过一次。错误 code：`bad_args` · `refused`（路径解析拒）· `too_many_transfers`。
此前两处都判「落点是不是会话文件」（monitor 开单时一道、后端一道），都删了。
`local_path` 也收 `{"b16": "<十六进制>"}`（与文件管理面同一个字节形）：有损名下载在 Linux 上按原始字节落名（monitor 中继原样转）。

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

二进制叫 `ccm` 时，下面这张表只在打头的 `--` 之后生效（`ccm -- <后端的词…>`；`control/ccm/mod.rs::route`）。

后端认识的每个 `--token` 恰好属于三类之一：

| 类 | 成员 | 语义 |
|---|---|---|
| **流模式 flag** | `--stream` · `--with-bg` · `--tail-only` · `--with-pid` · `--with-raw` | 出现即剥离并置位，**不影响模式判定** |
| **一次性查询子命令** | 上面那张查询表的全部 | **只有 `args[0]` 是其中之一才进查询模式** |
| **子命令选项** | `--after-ms` · `--include-tools` · `--limit` · `--scope` | 只在某条子命令之后才有意义，后端顶层不解释 |

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
> 〔删用量 —— 这里原有一条 `--usage`（服务端在远端聚合用量，per-requestId 每字段 MAX）。
> 用量的**聚合轴**整轴退役 ⇒ 子命令与它的实现（那份 `usage_query.rs`，**已删**）一起删了，
> monitor 侧的 fan-out 消费者同拍删除。`SUBCOMMANDS` 27 → 25（另一条是 `--oneshot-session`）。〕

- `--list-accounts`（A2 多账号，`src/backend/observe/accounts_query.rs`）→ 读账号库清单（`~/.cc-monitor/accounts/accounts.json`，契约 v1）。**首行** `{"kind":"accounts-meta","enabled":bool,"acctsDir","manifestPath","updatedAt","sharedStore","count","error"}`，其后每账号一行 `{name,email,configDir,isDefault,mode,exists,loggedIn}`。**"未启用多账号"是正常状态**：manifest 缺失/坏/版本不支持 → `enabled:false` + `error` 人话原因 + **exit 0**（不是错误）。`loggedIn` 仅 stat `.credentials.json` 存在性。账号库目录只跟着家走：`$HOME/.cc-monitor/accounts`（没有另指位置的参数或环境变量）
- `--session-accounts`（A2；`launchId` 是 `K-P5f`）→ 扫 `<claude_dir>/sessions/<PID>.json` 拿 pid，读 `/proc/<pid>/environ` **只抠三个写死的键**（`CLAUDE_CONFIG_DIR` · `CCM_LAUNCH_ID` ·`ANTHROPIC_BASE_URL`——最后那个的值带中转钥匙，只折成 `viaRelay` 一个布尔、值本身不出参；**键名不是参数**，所以这条查询不是「任意环境变量读」原语，也**绝不回传整个环境快照**），`CLAUDE_CONFIG_DIR` 反查 manifest 得账号名。每条一行 `{pid,sessionId,cwd,configDir,account,bare,alive,launchId,viaRelay}`（`viaRelay` = 这条会话的上游地址是不是本机中转那一形：`true` / `false` / `null` = 不知道（进程已死 / 环境这一刻取不到）；机器页「停」本机后端之前据它数几条会断；老后端不出这个键 ⇒ 读成 `null`）。`account:null` = 查不到（**不猜**）；**`bare:true` = 进程活着、`/proc/<pid>/environ` 这一刻读得到、而没设 `CLAUDE_CONFIG_DIR`（裸起）——这个布尔的语义钉死在那一个变量上，加了第二个键也没有拓宽它**（没设 `CCM_LAUNCH_ID` 由 `launchId:null` 自己表达）。⚠ 「读得到」这个合取项是 `K-R21`（09-03）补的，**语义是收窄不是拓宽**：environ 在 exec 窗口里（60–140 µs）与进程成僵尸之后**读得到却回 0 字节 / 读不到**，从前那一刻会被报成斩钉截铁的 `account:"<账号0>"` + `bare:true`，而 `alive` 仍是 `true`（判活读的是 `/proc/<pid>/stat`，与 `environ` 不是同一次读）⇒ **一条真跑在别的账号下的会话会被报成账号 0 的，且无声无息**。现在那一刻报 `configDir:null` + `account:null` + `bare:false`（=「不知道」，**出参形状没变、没有新字段**）。`launchId` = 起会话方铸进这条会话进程环境的**身份 token**（写侧是 `ccm` 自己：`control/ccm/plan.rs::LAUNCH_ID_ENV`），`null` = **不作数**，五种原因合并且**刻意不区分**：没设 / 形状过不了白名单（`[A-Za-z0-9_-]`，1..=128）/ **同一个 token 落在一条以上活会话上** / 进程已死 / **读那一刻环境取不到**。⚠ 第五种是 `K-R21` 现打出来的，**它一直都在、只是从前混在「没设」里数不出来**（读侧那个 `Option` 装着四件事）——这不是新增了一种行为，是把「四种」这句旧话订正成实话；`configDir` 那一半已经把它拆出来了，身份这一半仍按「要区分就得给出参加状态位 = 改上线契约」那条裁定合并着。⚠ **`launchId` 不是硬真相**：它是**继承型**环境变量（claude spawn 的子进程原样继承），后端只能判「同一批里唯一」，判不出「确实是它的」——父会话已退出时那个继承值仍会被报出来。**additive**：老后端不出这个键，下游读成 `null`。⇒ 账号那一半（`configDir`/`account`/`bare`）仍是"某条**正在跑**的会话属于哪个账号"的唯一硬真相（会话 jsonl 里没有任何账号字段）；身份那一半（`launchId`）**不是**，别把上一句读到它头上
- `--account-trust <configDir> <cwd>`（A2）→ 换号 resume 前的信任预检（首次用某账号进某目录，CC 会弹信任确认、会卡住自动化）。单行 `{"trusted":bool,"known":bool,"error":null}`。**安全**：`configDir` 必须逐字 ∈ manifest 的账号列表，否则 exit 2 + stderr `{"code":"unknown_config_dir",...}`——避免退化成任意文件读原语；**只回三个布尔/字符串字段，绝不回传 `.claude.json` 内容**（内含 `mcpServers` 的环境变量，可能有 API key）
- `--account-trust-zero <cwd>`（A2）→ **账号 0**（未启用多账号时那个原生身份）的信任预检，返回形状同 `--account-trust`。**为什么单开一个动词而不是给 `--account-trust` 传空 `configDir`**：账号 0 没有 config dir，而空串是被明令禁止的拼法（空值 ≠ 未设）；且它的 `.claude.json` 原生根是 `$HOME`、不在共享账号库里 ⇒ 路径来源本就不同，合并只能靠哨兵值区分，比多一个动词更易错。**不收任何文件/配置目录路径参数**：它收 `cwd`，但那只当 `projects` 里的**查表键**，`.claude.json` 的根写死 `$HOME` ⇒ 连"任意文件读"的面都没有（`account_trust_zero_takes_no_path_argument` 钉住）
- `--fork-session <args>`（G2 branch-anywhere，`src/backend/control/fork_write.rs`）→ 从指定消息处分叉出一个新会话文件。**后端唯一的写盘入口**——其余一切子命令只读；`readonly_guard` 的写白名单按路径单独盯着 `control/fork_write.rs` 这一个文件（`src/doc/INVARIANTS.md` §41.6）
- `--tmux-notify <backend_pid> <backend_starttime>`（P4b zero-poll-liveness）→ **不是查询**，是 tmux hook 子进程走的通路：校验身份后给正在跑的后端发一个信号叫它立刻重扫 tmux，**完全不碰文件系统**。两个参数缺一或非整数 ⇒ exit 2。**必须同时比对 starttime 而不只看 pid 存在**：后端退出后那个 pid 可能已被别的进程占用，误发信号轻则无效、重则打断无关进程（很多程序把该信号当自定义控制信号，默认处置直接终止）。身份对不上 ⇒ **静默 exit 0，不做事**

- `--list-user-inputs [--from <offset>] <jsonl_path>`（SE1）→ 大纲的数据源：该会话里每一条**主线用户输入**（头 ＋ 每条一行 ＋ 尾），形状与口径见 **§10.4**

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

**只读一行 stdin**：收 stdin 的那几条（`--<帧命令>`）后面可以跟 `--stdin-line` ——
stdin **只读到第一个换行**就动手，不等 EOF（上限与超限的拒法同默认那一形：1 MiB，`args_too_large`，不截断）。
给「stdin 关不掉」的调用方：本机常驻后端经 SSH capture 问远端后端时，capture 不关远端 stdin，载荷由它写进去一行
（`capture.stdin`，见链路那一节的 `dial` 字段）。这样载荷不必拼进远端命令行 —— 那要求远端登录 shell 认 POSIX 单引号与管道，fish 一类不认。
**argv 一族**（`--list-sessions` 这类老子命令）同一个修饰词：恰好 `--<子命令> --stdin-line` 两个词 ⇒ stdin 那一行是**其余 argv 的 JSON 字符串数组**
（`control/cli_control.rs::expand_stdin_argv`），拼回去照常分派；读不动 / 不是字符串数组 ⇒ exit 2 ＋ `{code:"bad_request"}`。
今天的发送方：资产目录推那一趟（`'<远端后端>' --assets-catalog-merge --stdin-line`）· 历史跨机那一问（`'<远端后端>' --list-sessions --stdin-line`，项目目录名走那一行；`remote_ask::ask_with`）。旧后端不认这个修饰词（它会照旧读到 EOF、一直等）⇒
随 `BUILD_ID` 换代，远端按身份重部署之后才发。

错误写 stderr + 退出码 2（`--account-trust` 用 `--resolve` 那套结构化 `{code,message}` JSON）。**读会话那一族**（`--read-session` / `--read-session-tail` / `--read-session-from-offset` / `--fork-session`）的路径参数严格限制在 `<claude_dir>/projects/` 内（canonicalize 后前缀校验，拒穿越 / symlink 逃逸 / 非 jsonl）。**账号一族不走这条**，各有各的判据：账号库只在 `$HOME/.cc-monitor/accounts`（不收另指位置的参数）；`--account-trust <configDir>` 靠「逐字 ∈ manifest」而非 projects 前缀；`--tmux-notify` 根本不碰文件系统。**旧后端兼容**：不认参数的旧版会照常发 `hello` 进流模式——monitor 以"首行是 hello 帧"识别旧版并提示升级（优雅降级，无版本协商）。

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

> 〔删用量 —— 这里原有 `--oneshot-session <名字后缀> <存活秒数>` 的整段说明
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

**LOC1a 追加一条（09-25）**：`--session-fork`（读 stdin）—— 见上面它自己那一小节。同上，与帧面同一个 `run`。

**SH1 追加一条（09-26）**：`--bus-inbox` —— 只读看一个 agent 收件箱的尾巴（见上面它自己那一小节）。同上，与帧面同一个 `run`；**读 stdin**（`{id, lines?}`）。

**SH1 追加一条（09-26）**：`--mcp-read` —— 这台机器的 MCP 列表成品（见上面它自己那一小节）。同上，与帧面同一个 `run`；**读 stdin**（`{projectDir?}`）。

**MIG-3a 追加二十二条（09-27 · 09-28；09-30 起其中两台之间的旧枢纽四条、卸那一条与收件箱三条已删）**：`--cc-bus-install` · `--cc-bus-install-state`（cc-bus 装到这台）· `--mcp-server-put` · `--mcp-server-remove` · `--mcp-sync-source` · `--mcp-sync-preview` · `--mcp-sync-apply` · `--skill-install-apply` · `--aliases-render` · `--aliases-read` · `--aliases-install` · `--aliases-block-render` · `--aliases-block-install` · `--aliases-block-remove` —— D 组 MCP · skill · 别名那几件收进这台后端（见各自那一小节）。与帧面同一个 `run`；**读 stdin**。

**追加一条**：`--files-link`（写面「建链接」的入口，**读 stdin**）—— 见上面它自己那一小节。与帧面同一个 `run`。

**账号库那一族追加九条**：`--accounts-init` · `--accounts-add` · `--accounts-remove` · `--accounts-set-default` · `--accounts-repair` · `--accounts-isolate` · `--accounts-rollback` · `--accounts-login-cmd`（读 stdin；`--accounts-add` 的 key 只走 stdin、不收 argv）· `--accounts-verify`（不读 stdin）—— 见上面各自那一小节。与帧面同一个 `run`。

**各账号共用的 MCP 那三条**：`--accounts-mcp-read`（不读 stdin）· `--accounts-mcp-remove` · `--accounts-mcp-pick`（读 stdin）—— 见上面各自那一小节。与帧面同一个 `run`。

**扩展页追加两条（09-30）**：`--ext-uninstall-preview` · `--ext-uninstall-apply` —— 从这台卸一个扩展（见上面各自那一小节）。与帧面同一个 `run`；**读 stdin**。`ext-list` · `ext-hub-preview` · `ext-hub-apply` 读本进程的可达表，只在帧面上。

**SH1 追加一条（09-26）**：`--tmux-list` —— 这台机器的 tmux 会话（见上面它自己那一小节）。同上，与帧面同一个 `run`；**不读 stdin**。

**终端管理 L1 三条**：`--terminals-list` · `--terminal-preview` · `--terminal-input` —— 名单 · 抓一屏 · 送字送键（见上面它们那一小节）。与帧面同一个 `run`；**读 stdin**（那条命令的 `args`）。

**MIG-3b 追加一条（09-27）**：`--hooks-diag` —— cc-bus 钩子诊断（见上面它自己那一小节）。同上，与帧面同一个 `run`；**不读 stdin**。
**MOD 追加三条、换一条（09-28）**：`--history-page`（读 stdin）· `--drift-report`（**不读 stdin**）· `--history-subagent`（读 stdin，替掉只列候选的 `--history-subagents`；后来又被按运行读的 `--history-run` 替掉）—— 记录解释进了后端（见上面各自那一小节）。同上，与帧面同一个 `run`。
**MIG-1 追加三条（09-27）**：`--ssh-config-aliases` · `--ssh-config-import`（不读 stdin）· `--ssh-config-resolve`（读 stdin：`{alias}`）—— 见上面各自那一小节。同上，与帧面同一个 `run`。
**MIG-1 续追加一条（09-28）**：`--remote-probe`（读 stdin：`{machine, saved?, jump?}`）—— 测试连接，一次性进程里照样拨得了一次（短命探活，不进连接池）。`forward-*` 三条只有帧面（转发账住常驻那一个进程）。

`resync` **没有 CLI 面**（`cli_control::STREAM_ONLY`）：它对齐的是进程里在跑的 watcher，一次性进程里一份都没有。

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

**F7a 追加两条（2026-09-24）**：`--files-read-text` / `--files-home` —— 同族第七、第八条
（逐条见上面各自那一小节）。同上，与帧面走**同一个 `run`**；`--files-read-text` 读 stdin
（那段 JSON 就是它的 `args`），`--files-home` **不读 stdin**。两条都只读。

**W5-FILES 追加一条（2026-09-25）**：`--files-size` —— 同族第九条（算目录大小，逐条见上面 `files-size` 那一小节）。
与帧面走**同一个 `run`**，读 stdin（那段 JSON 就是它的 `args`）。只读。

**FILES3 追加一条（2026-09-28）**：`--files-grep` —— 同族第十一条（按内容搜，逐条见上面 `files-grep` 那一小节）。

**WF1 追加一条（09-29）**：`--powershell-policy-set` —— 见上面它自己那一小节。与帧面同一个 `run`；**读 stdin**（`{host}`）。

**别名表单追加两条（10-02）**：`--aliases-to-form` · `--aliases-from-form` —— 表单与参数互转（见上面各自那一小节）。与帧面同一个 `run`；**读 stdin**。
读 stdin（那段 JSON 就是它的 `args`）；一次性进程里没有要撤的在飞那一趟，上界照旧。只读。

**`K-H1` 那一条：HTTP 中转**（搬字节那半）。独立进程那一形（子命令 `--relay`）删了：
中转只住常驻后端进程里（本机与远端），下面说的是它。

- **只监听 `127.0.0.1`**，不对外暴露；端口由宿主以 `CCM_RELAY_PORT` 交（值是 `relay_route_core::PORT`，`8788`），**没有缺省值**（认不出 ⇒ 不开）。
- 流模式（stdio 或常驻监听口两条载体一样）的后端**被交了** `CCM_RELAY_PORT`
  ⇒ 在本进程里起中转（中转 ＋ 上游选择同一份代码，`relay::listen::host` ＋ `accounts::upstream_select::host_relay`），接受循环跑一条专属线程，
  随进程生死（常驻后端按「退出行为」留或退，中转一起）。**tee 落 tap 口**（`tap` 帧；stdout 是 wire）·
  **起不来不退出**（出声，后端照常服务）。没交端口 ⇒ 不开（测试连接探针那一趟流模式就是这一格）。
  宿主：本机 monitor 起后端时交；远端 `--resident-ensure` 起常驻子进程时交同一个值。
  凭据文件住家里（`<家>/apikey-credentials.json`），那台后端按家推，没有另指它位置的变量。
- 默认上游**每个 agent 一行**〔条 59 / 条 60，2026-09-24 订正；先前这里写的是一个**进程级**的上游默认
  （`https://api.anthropic.com`，由一个进程级环境变量盖）—— 已整删〕。它是**上游选择**的表，不是中转的配置：
  今天只登记了 `claude-code`（默认 `https://api.anthropic.com`，`CCM_AGENT_UPSTREAM_CLAUDE_CODE` 只盖这一家；
这个变量先前叫中转的名字，改名不留兼容读旧名）；
  **codex 刻意没登记**（它的默认上游本仓零证据）⇒ 它走 `/t/` 被拒（**404** ＋ 原因头 `agent-not-registered`；FIX3 之前是 502），不回落到任何一家。
  表住 `agents::Adapter::upstream`（跟着适配层）。**基址里可以带一段路径前缀**
  （`K-R1`，形如 `https://<host>/<前缀>`）。
  ⚠ **订正**：这一行先前逐字写着「`http://` 只给本机夹具用」——**那半句今天不准确了**。
  今天的分界线是**回环**：`http://` 打到本机回环是一等公民（〔用 09-04〕逐字要「还可以接本地部署的」），
  而**明文 + 非回环**的那一行会被装表那一步**拒掉并出声**
  （判据 `accounts::table::tests::a_plaintext_upstream_is_only_allowed_on_loopback`；上游选择 2026-09-24 搬出了 `relay/`）。
  `裁-1`「只准 TLS」**没有被推翻**，升的只有回环这一格。
- 路由：`claude` 把 `ANTHROPIC_BASE_URL` 指到 `http://127.0.0.1:<port>/s/<agent>/<account>`
  （**代入**：apikey 表里有这一行，换上这一行的 key）或 `…/t/<agent>/<account>`
  （**直通**：永不代入，下游那份鉴权头逐字节原样上去；表里没这一行时发到那个 agent 自己的默认上游），
  中转把前缀与两段剥掉、其余路径与查询串**原样**转给上游。地址里没有会话段（先前的 `<key>`：resume 时是 sid、
  新开时是启动器铸的 nonce）：会话 id 归 claude，中转从它请求头 `x-claude-code-session-id` 认会话、给 tee / tap 打标签。
  ⚠ **那一行的基址若带路径前缀，前缀会被接在这段原样路径的前面**〔`K-R1`，住址
  `upstream::Base::upstream_target`〕—— 前缀为空时与改前**逐字节相同**。
  🔴 中转**不查重、不合并重复的段**：配了 `<host>/v1` 而客户端发 `/v1/messages` 的人
  会得到 `/v1/v1/messages`（装表时给那一行记一条 note 说出来，见 `table::NOTE_PATH_PREFIX`）。
  ⚠ **`<account>` 那一段是 `K-H2` 加的**。〔条 49，2026-09-24 订正：先前这里写「中转……只拿 `<account>` 查表」，
  拆键之后不准了〕apikey 表的键是 **`<agent>` ＋ `<account>` 两段**（claude-code 的 3 号与 codex 的 3 号是两行）：
  `/s/` 表里查不到那一对 ⇒ **404，一个字节都不发上游**（不回落到别的账号的 key，
  也不回落到默认上游）。两段对**中转**都是不透明串 —— 它不解释它们，原样交给上游选择去查。
  ⚠⚠ **退役的会话段 `/s/<agent>/<account>/<sid>/…` 不会被解析器拒掉**，它会被读成真路径的一截
  （上游收到 `/<sid>/v1/messages` ⇒ 上游 404）；升级之前起的、带老地址的会话要重起
  （判据 `route::tests::the_retired_session_segment_is_not_rejected_here_it_becomes_part_of_the_real_path`）。
- 谁设 `ANTHROPIC_BASE_URL`〔2026-09-24 订正；先前这里是 08-28 的读数「生产代码 0 处、没有入口」，`K-H2b` 之后不成立。
  〔审计 F 🔴-3〕再订正：先前这里写「monitor 起本机会话时判（`payload::relay_endpoint_for`）· 远端机器那一半不注入」，
  US1（4D）之后两句都反了〕：
  起 agent 的那台机器上的 `ccm`（**本机与远端同一条路**）在最终 exec 那一处问上游选择（见上面「`ccm` 在最终 exec 那一处定的几样」），
  决策表的唯一实现是后端上游选择 `accounts/upstream_select/endpoint.rs::decide_launch`，monitor 不经手：
  那台的 apikey 表里有 (agent, 账号) 这一行 ⇒ 注入 `/s/`（`whenDown: refuse`：中转不在 ⇒ **拒绝起会话**；
  中转住那台的常驻后端里，本机远端同形，不另起一个）；没有这一行 ⇒ 默认**不注入**。
  全量注入（订阅号 · 不带账号的本机会话也注 `/t/`）**带开关、默认开**：起 agent 那台 `ccm` 的环境里 `CCM_RELAY_ALL_SESSIONS=0` 才关；
  开了也只给登记了默认上游的 agent 注（codex 不注）；`/t/` 那一格中转不在 ⇒ 照旧直连（`whenDown: direct`，不拒绝）。
- 中转**自己造**的状态码〔（FIX3）：照 HTTP 代理通行做法，每个码只有一处常量〕：
  **我们拒的一律 4xx** —— 门 403 / 421 · 请求读不懂 400 / 411 / 413 · 路由不成立 404（`/s/` 表里无行 · `/t/` 的 agent 没登记默认上游 · 路径根本不是路由形状）；
  在途连接顶满 503；🔴 **上游那侧 5xx** —— 超时（连接超时 · 等响应超时）⇒ 504，其余（连不上 · 发到一半断了 · 没回应就断 · 回的不是 HTTP · 只给 1xx）⇒ 502。
  中转自己回的每一条响应都带原因头 `X-Cc-Monitor-Reason: <短词>`（`bad-key` · `no-account-row` · `agent-not-registered` · `upstream-connect` · `upstream-send` …），
  拒绝与传输失败的响应体第二行是一句人话（传输失败形如 `上游 <主机>:<端口> <结果>。卡在<哪一步>这一步。`）。
  ⚠ 上游**自己**答的码原样转发、不带原因头 —— 同一个码是谁说的，看有没有这个头。
- 响应**逐块透传绝不缓冲**；同一批字节里的 SSE 事件抄一份交给宿主的 tap 口（`relay/tee.rs::TeeSink`），
  宿主把每一件变成出方向帧 `tap`（`{"kind":"tap","stream":…,"resp":N,"n":M,"data":"<上游 data: 后面那段，一个 JSON 串>"}`，
  收尾那一件 `"end":"done"|"broken"`、`n` = 这个响应一共占了几个号），走 wire 自己那条有界通道（见上面出方向帧表 `tap` 那一行）。
  **不带路由那两段**（① 不问账号）；丢了的号在接收侧原位看得出（先占号再投递）。
先前独立的 `--relay` 进程把 tee 写成 NDJSON 行落自己的 stdout（`__meta__` · 事件行 · `__dropped__` 三种行，零消费者）——
  那一形随它删了；这一格原先那两轮订正与「钉的是哪一维」的记账随之销（记录在 git 历史里）。
- **请求头原样转发，但一个都不落进 tee、不落进日志。**
  ⚠ **例外都在鉴权头上**（`K-H2a` 换头 + `K-R1` 把人群扩到第二个头名）：那一行**有自己的 key**
  （或它的 `auth_style` 声明了「一个鉴权头都不发」）时，客户端自带的 `Authorization` 与 `x-api-key`
  **都被丢掉**，换上这一行自己那一个（写哪个头由该行 `auth_style` 定）。
  ⚠ **`Proxy-Authorization` 仍然照旧转发** —— 它说的是与代理之间的鉴权，收掉它是另一件事，
  **登记为射程外**。那一行**没有** key 时（订阅登录那一档）一个字节都不动。
- ⚠ tee 事件**不带 `t_ns`**，也**不设上游超时** —— 两处都受后端零定时器护栏所限，
  理由与代价见 `src/comms/outward/mod.rs` 头注。

**`K-P6b` 追加一条**：`--dial` —— 起 **SSH 拨号代理**（候选 E 的字节代理）。它与当时的 `--relay` 同族：
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
  `src/frontend/shell/src/ssh_source.rs` 那一侧是 `.env(DIAL_REQUEST_ENV, …)`。
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
  `src/frontend/shell/tauri.sidecar.conf.json`，发版那一步用 `--config` 注入（**刻意不进基础
  `tauri.conf.json`**，进了 `cargo test` 也要一份当前 target 的二进制）。
  ⇒ 「安装包没有 `externalBin`」与「exe 旁那处也空」**两句都不成立**；
  ★ 这也说明**「基础配置里没有」≠「没配」**——照 `tauri.conf.json` grep 得到的是只在开发树为真的答案。

  🔴 **但结论不跟着翻 —— 那一格没测，本节不替它下结论。** 前提假不蕴含结论假：
  代理二进制解析得到（回落①不触发）之后，**还有回落②**——配置里没填 `keyPath` 的那台走 ssh-agent，
  而代理只会 publickey，仍旧留在进程内（当时 `ssh_source.rs` 起远端流那一处是 `proxy && has_key` 两个条件；那一处已随远端流模式一形删了）。
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

**`--dial` 从「搬走 1 处」变成「SSH 的全部活」。**
上面 `K-P6b` 那一段是它当时的样子，**原文不改**；下面几句是今天的样子，与上面冲突时以这里为准：

- 🔴 **拨号搬出去了**（上面那句「任何地方都不许把它写成拨号搬出去了」到此作废）：界面进程里除 SFTP 外的
  **全部**拨号 —— 后端长连接流 · 一次性 exec · 收全 exec · 测试连接 · 端口转发 —— 都经这个代理；
  界面侧拿链路的唯一入口是宿主 `dial_host.rs`，读应答的是通信层成员 `ssh_link.rs`。
  **唯一的例外是 SFTP**（`sftp.rs`，`F7c` 独占）：仍用 `inproc_dial.rs` 那一份进程内拨号，
  理由（池预算 · 红线 I7）写在。
**这个例外没了**：SFTP 进了本机常驻后端（`dial/sftp.rs`，与其它 SSH 同一条连接），
  `inproc_dial.rs` 整份删了，界面进程**零 SSH**（`russh` / `russh-sftp` 出了 monitor 的清单）。传输走入方向「传输四条」，
  部署走链路 `use:"files"`（见「链路」那一节）；远端写只许 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`。
- **两条回落都删了**（`D11`「后端是给定的，不要退路」）：拿不到代理二进制 ⇒ **报**；没配 `keyPath` ⇒ 代理自己走 ssh-agent。
- 请求只**加**了可选字段（v1 那六个一个没改）：`endpoints`（竞速顺序）· `jump`（跳板那一台）·
  `use`（`stream` 缺省 · `capture` · `forward`）· `capture{max_bytes,abort_marker}` · `forward{local_port,remote_host,remote_port}` ·
  `stages` · `probe`。**SFTP 子系统不在 `use` 里**（后端的远端写那一层把它判作远端写能力）。
`use` 多了 `files`（受限远端文件一问一答，写只许两处）；原始 SFTP 字节（`use:"subsystem"`）照旧不开。
- 应答：`stages=true` 时 ack 之前先有若干行 `{"stage":{"kind":…}}`（与 `ConnectStage` 同形）；ack 多了
  `endpoint`（竞速胜者）· `v`（`2`）· `uses`（认得的用法）—— **界面据 `uses` 认出老代理并出声**，不去解它的字节；
  `capture` 在 ack 之后回一行 `{"stdout","stderr","exit_status"}`；`forward` 每接一条连接回一行 `{"accepted":n}`，stdin EOF 即收工。
- 鉴权：`key_path` 或 **ssh-agent**（Unix `SSH_AUTH_SOCK` —— 新能力；Windows OpenSSH 命名管道 —— 零真机读数）。
  竞速**同时起拨**、不错开（后端零定时器护栏禁 `sleep`/`timeout`）；握手看门狗改在界面侧等 ack 时执行（45 s）。
- 读数：`tests/evidence/C2-dial-loopback.py` 对真回环 sshd 八项（竞速 · 严格指纹 · 流与收工 · 跳板 · agent · 转发）。
- ⚠ 后端行为变了（多了用法与字段），**子命令集不变** ⇒ 合并时 bump `BUILD_ID`，否则开发树里按旧 id 释放出来的
  老代理会被界面判成「本机后端太旧」。

**`--dial` 这条子命令删了。** 上面 `K-P6b` 与 C2 两段是它当时的样子，**原文不改**；今天以这里为准：
拨号挪进**本机那一个常驻后端**，经流上的链路做（本节「入方向」里的「链路四条」）—— 不再每条链路起一个进程，
`CCM_DIAL_REQUEST` / `CCM_DIAL_PROXY` 两个环境变量随之退场。请求的形状一个字没改（从环境变量搬进 `link-open` 的 `dial` 字段，
多一个可选的 `agent_sock`），链路上的应答与 C2 代理的 stdout 逐字节同形。常驻后端不在 ⇒ monitor **报**，不起代理进程、不进程内拨（`D11`）。
⚠ 子命令集少了一条 ⇒ `build_id_guard` 红，合并时 bump `BUILD_ID`。

**远端常驻后端：`--resident-ensure` / `--resident-stop`**（一次性，monitor 经本机常驻后端的链路 `capture` 在远端跑；住 `control/resident.rs`）。
- `--resident-ensure`（可带 `--replace`）：口上已有人在听 ⇒ 不起，stdout 一行 `{"port","token","pid":null}`（`token` = 盘上 `<家>/listen-token` 那一把，
  它起来时写的；家 = `~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`）；读不动 / 空 ⇒ `no_token`。没人 ⇒ 起一个脱离的自己（常驻载体：
  `CCM_LISTEN_PORT` = `relay_route_core::listen_port_for(家)`（门牌只跟着家走，与 Claude 目录无关）· `CCM_LISTEN_TOKEN_FILE` = 钥匙文件**路径** · `CCM_RELAY_PORT`（远端中转进程内起）·
  `CCM_BACKEND_STDERR_LOG`）→ stdout 一行 `{"port","token":null,"pid"}`、退出 0：钥匙由它绑上口之后自己换一把写进那份文件（0600，原子写），客户端读到 hello 之后再问一次。
  常驻后端（本机远端同一份）每次绑上口都换一把新钥匙写回钥匙文件，抢不到口的后起者不碰它；连上来的客户端每次读文件。
  `--replace`：先按口上那一位自己记的 `<家>/listen-<口>.pid`（核 `/proc/<pid>/exe`）用下面同一个停法停掉它，再起。
  本机 monitor 停本机常驻后端也走 `--resident-stop`，交的环境与起它时同一份（`local_backend_host::backend_env`：中转口 ·（隔离跑时）家 ·（设置里填了覆盖时）`CLAUDE_CONFIG_DIR`）。
- `--resident-stop`（可带 `--grace` `<秒>`）：同机监督者 —— 拿进程把手（Linux pidfd）→ 核身份 → SIGTERM →
  宽限期内等它退（默认 35 秒；`--grace` 必须大于后端退出排空上限 30 秒）→ 到点 SIGKILL → 再等至多 5 秒；
  stdout `{"stopped":"graceful"|"killed"|"not_running","pid":<pid>|null}`；强杀之后仍在 ⇒ `stop_failed`；`--grace` 不对 ⇒ `bad_args`。失败：stderr `{code,message}`、退出 2（`code` ∈ `no_home` · `no_token` · `replace_failed` · `stop_failed` · `spawn_failed` · `unsupported` · `bad_args`）。
- 常驻监听口的握手多一格：attach 行可带 `"flags":[…]`（`STREAM_FLAGS` 的子集，这条连接的流模式旗标；缺 = 进程起参那一份；表外的 ⇒ `malformed-attach`）。
  **多客户**：钥匙对上就交流，每条连接各一份 watcher / inbound / writer；连接计数归零才按「退出行为」办；`stream-busy` 不再发。
- 链路多一种用法 `tunnel`（`tunnel_port`）：本机常驻后端开 direct-tcpip 到远端 `127.0.0.1:<口>`，monitor 经它讲上面这条监听协议（不另开公网口）。
- ⚠ 子命令集多两条 ⇒ `build_id_guard` 红，合并时 bump `BUILD_ID`。

⚠ 加一条 CLI 命令要动**两处**：`inbound::REGISTRY`（实现与分派臂）+ `main::SUBCOMMANDS`
（`is_query_mode` 的闸门）。只动前者的后果是**静默的** —— 后端把它当未知 flag、
打一行 warn 之后照常进流模式，调用方拿到一堆 jsonl 行。08-13 实测撞到过，
现由 `cli_control::tests::every_cli_exposed_command_is_in_the_query_mode_gate` 钉住。

### 10.1 ★ `--resolve` 的返回值里**哪些是探测出来的、哪些是派生的**（E71）

`--resolve` 读 stdin 的 `ResumeSpec`、往 stdout 写一个 `CommandPlan`。**这三个字段的可信度不一样**，
而字段名读起来一模一样 —— 消费方（已经有一个了）很容易把派生值当事实用。
整份线上形状（入参 / 出参 / 错误码）是给 aterm 的**跨仓承诺**，全集与「改了要同轮做哪三件事」住 `§10`「`resolve`」一节的「跨仓承诺」小节：

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

**要判断某个 tmux 会话是否真的存在**，问那台后端的 `tmux-list`（那是真 `tmux ls`；原来的 `tmux_sessions` 帧删了）或自己在远端跑一次，
别拿 `sessionName` 当答案。

### 10.2 ~~代码全景本机后端的按需拉取~~ —— 🔴 **整节作废（条 67 · 2026-09-18）**

**这条交付路径不做了，代码已删。**

用户逐字「**不在现在设计里的全部删掉**」。原文这一节描述的是：后端第一次收到全景请求时
**按需从网上拉一个外部二进制**、按哈希钉校验、落到 `~/.cc-monitor/bin/`。
实现住 `src/backend/sidecars/codepicture/`（**2 008 行**），今天已整棵删除，
连同它在后端那侧的四格机器判据与那 11 个 `local_backend_*` 失败码。

**为什么删**：同一个能力有**两份完全独立的实现**，
而只有一份接上了 ——

| | 活的那份 | 删掉的这份 |
|---|---|---|
| 怎么拿到能力 | **vendored crate 进程内 link**（不走 MCP、不下载） | HTTP 按需拉一个外部二进制 |
| 命令面 | **21 条** | 0 |
| 生产调用方 | **有**（14 条在用 / 7 条死） | **0** |

⇒ 2 008 行是为一个**从没定下的交付方式**写的，而实际发出去的那条走完全不同的路。
当时逐字判定「**它的处置取决于产品裁定，不是工程问题**」——本条就是那个裁定。

⚠ **`hello.unavailable[]` 这个机制本身没废**（§10 那一节仍然有效），
废的只是往它上面挂 `local_backend_*` 那一族 `code`。

⚠ **删代码不许连道理一起删** —— 这条路留下过两条今天仍然成立的教训，
已就地记在它们该在的地方，别再重新发现一次：
- **一个上限罩着两个量，超限那一刻分不出被截的是谁**（头与体共用上限 ⇒ 头把体的额度吃掉
  158 字节，剩下 42 字节被当成一份完整的资产 ⇒ 静默截断）。墓碑在 `byte_cap_registry.rs`。
- **光在闭集里加一行是「申报」，申报会在那一层被掏空之后照样绿着**
  ⇒ 「app 自带某个二进制」这类条目，右边必须去钉真源码。墓碑在 `cross_half_edge_registry.rs`。

### 10.3 会话骨架索引：`--read-session-from-offset` 的 `--index` / `--until` 两个选项（骨架，2026-09-24）

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
   `{"o":<行起点绝对字节>,"n":<行字节长含\n>,"t":<type>,"u":<uuid>,"sc":true,"sp":"system","ch":…,"cj":…,"pl":…,"cb":…,"cl":…,"fd":…}`
   —— `sc`=属于子运行 · `sp`=user 记录是谁说的（来源的 `kind`，同记录成品 `userText.speaker.kind`；人说的与工具结果省略，
   客户端据它定这一行建不建卡）· `ch` 正文字符数（代码块外）· `cj` 其中 CJK（`> U+2E80`）·
   `pl` 正文非空硬行 · `cb` 围栏代码块数 · `cl` 代码行数 · `fd` 折叠单元数（tool_use / tool_result / thinking / image）。
   **零值与假值不序列化**；解析不出的行**仍占一行**（只有 `o`/`n`），丢了它后面的 seq 全错一位；
这一行是一条**用户输入**（口径 = §10.4 那四条，判定同一个函数 `user_inputs::user_input_of`）⇒ 多两个键：
   `x` = 摘要（同 §10.4 的 `excerpt`）· `ts` = `timestamp`（空 ⇒ 省略）；uuid 就是本行的 `u`。
   ⇒ 首屏「索引」与「大纲清单」合成一趟读：客户端见到**至少一个 `x`** ⇒ 对面是会出它的后端、每条用户输入都带着 ⇒
   `[from,end)` 的清单就在这里、不必再发 §10.4；**一个 `x` 都没有 ⇒ 分不清**（老后端 / 真的零条）⇒ 照旧发 §10.4。
   头 `v` 不变（只加可选键，老客户端忽略）；
3. 尾 `{"kind":"session_index_end","count":N,"end":E}` —— `E` = 最后一个**完整行**的末字节（torn 残尾不计）＝
   **下一次续传该带的 `offset`**。**没有尾行 ⇒ 输出被截断**，调用方不许把前面那些行当全量。

`--until` 与 `--index` 同用：只收**起点** `< end` 的行（起点在界内的那一行整行收）。

**这些是「宽度无关料」，不是高度**：高度依赖列宽，后端不知道列宽。前端拿它做第一级粗估。

🔴 **为什么是选项不是新子命令**：新子命令会进 `build_id_guard` 的指纹、逼出 `BUILD_ID` bump；那一拍本轮不许做。
⇒ **在已部署的老后端上这两个选项是休眠的**，而且老后端只读 `args[1]`（路径）/`args[2]`（offset）：
- 选项写在**后面** ⇒ 老后端照旧把**整份会话**透传回来（现打：基线 `161ffa6` 的 release 后端，50 955 695 字节的会话
  ⇒ stdout 50 955 695 字节、退出 0）—— 弱网上几十 MB，只为让客户端看一眼首行认出「它不会」；
- 选项写在**前面** ⇒ 老后端拿路径当 offset 解析（`--index` 形）或拿 `--until` 当路径（`--until` 形）⇒ **stdout 0 字节、退出 2**（同一份会话现打）。
⇒ **客户端一律把选项写在前面**（monitor 侧 `session_skeleton::range_argv`，有判据钉着；索引那一形 monitor 不再经 argv 发，界面经通道说帧命令 `history-index`）。
无论哪种失败，客户端都认「**首行不是 `session_index` 头**」⇒ 判「对面不会出索引」并**诚实降级**（不许把 jsonl 行当索引行解析）；
万一拿到超出 `end` 的正文（老后端不认 `--until`），客户端按索引给的行数自己截掉。
新后端上写错的 `--选项` 与多余的位置参数都**报错退出 2**，不静默忽略。
⚠ 要让远端真的用上它，得等下一次 bump `BUILD_ID`（判 stale → 重装）。路径守卫与 `--read-session` 同一套。

### 10.4 「你说过的话」清单：`--list-user-inputs`（SE1，2026-09-24）

**是什么**：大纲（原名「我说过的 N 句」）的数据源。**判定只有后端这一个住址**（`observe/user_inputs.rs`）——
前端从前在 `onLine` 旁路里一条一条攒（到达序 ≠ 对话序、monitor 起得晚就不全），查看器那边再用同一份 TS 判定扫全量；
两份都删了，两个宿主都来问这条。

| 调用 | 出什么 |
|---|---|
| `--list-user-inputs [--from <offset>] <p>` | 从字节 `offset`（缺省 0）起的清单，逐行 JSON，见下 |

选项在位置参数前后都认；**客户端写在前面**（与 §10.3 同一条纪律；monitor 不再经 argv 发它 —— 界面经通道说帧命令 `history-user-inputs`）。

**口径**：user 记录里**人说了话**的那几条 —— 「谁说的」由适配层判（与记录成品的 `userText.speaker` 同一个函数）：
人打的 / 粘贴的 / 斜杠命令 / `!` 输入算；agent 发来的、后台任务通知、系统注入、压缩摘要、中断标记、输出回显、工具结果都不算。
属于子运行的不算（子 agent 收到的是主线派的活 —— 选出来的，不是漏的）。**没有 uuid 的不要。**
斜杠命令的 `excerpt` 是 `/名字 参数`，`!` 输入是 `!命令`。

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

### 10.5 会话内查找：`--find-in-session`（SE2，2026-09-24）

**是什么**：实时 tab 里 Ctrl+F 的数据源 —— 在**一份**会话里找一段文字，告诉前端它落在哪几条记录上（uuid），前端按骨架索引跳过去。

| 调用 | 出什么 |
|---|---|
| `--find-in-session [--include-tools] [--limit <n>] --query <q> <p>` | 这份会话里所有命中的记录，逐行 JSON，见下 |

**口径与 §10 的 `--search` 是同一份**：一条记录拿哪两段文本去搜（user 正文先剥 CLI 注入的包装、按 `MAIN_CAP` 截；
`--include-tools` 时再加工具内容、按 `TOOL_CAP` 截）、命中算哪一种（先正文、后工具）、片段怎么切 ——
后端 `observe/search_query.rs::record_text` / `record_hit` ＋ 口径的家（`observe/search_rules.rs` · `agents/claudecode/text.rs`），两条子命令调同一对函数。
差别只在「扫哪些文件、给多少条」：只扫 `<p>` 这一份；**按文件序**（= 对话序）；**没有 uuid 的记录不列**（跳不过去）。

- `--query <q>`：**必填**，查询串是这个选项的**值**（不是位置参数）⇒ 以 `--` 起头的查询（`--force`）不会被当成选项。
  大小写不敏感子串；trim 后为空 ⇒ 零条。
- `--limit <n>`：最多列几条，缺省 500、封顶 2000（超出按封顶）；`0` ⇒ 只数不列。
- 选项在位置参数前后都认；**客户端写在前面**（monitor 不再经 argv 发它 —— 界面经通道说帧命令 `history-find`）。
  未知的 `--选项`、缺 `--query`、位置参数不是恰好一个 ⇒ **报错退出 2**（不静默忽略）。

**三段**：
1. 头 `{"kind":"session_find","v":1}`；
2. 每条命中一行 `{"uuid":…,"kind":"user"|"assistant"|"tool","before":…,"matched":…,"after":…}`（片段同 `--search` 的 `hits[]`），最多 `limit` 条；
3. 尾 `{"kind":"session_find_end","count":N,"total":T}` —— `T` = **全量**命中数（≥ `N`；`T > N` ⇒ 被上限砍过）。**没有尾行 ⇒ 输出被截断**，客户端不许当全量。

只看完整行（torn 残尾不看）。每次都从头扫一遍文件 —— 没有常驻索引。

🔴 **为什么不是给 `--search` 加一个「只搜这一份」的选项**：`--search` 对未知选项**容错忽略** ⇒ 老后端会把整台机器的会话全扫一遍、
按单会话 30 条封顶回来 —— 慢，而且静默少条。新子命令在老后端上是 `unknown argument`、**stdout 0 字节、退出 2**，客户端认「首行不是 `session_find` 头」⇒ 诚实降级（说清原因）。
⇒ **它是新子命令**：进 `build_id_guard` 指纹，要 bump `BUILD_ID` 才会让已部署的远端判 stale 重装。路径守卫与 `--read-session` 同一套。

## 11. 远端会话的 ↗：点那一刻现查「此刻谁在显示它」（issue #18）

不读也不设 tmux 标题、起会话时不注任何令牌、不改用户的 tmux / shell / PowerShell 配置、不轮询、不落盘。
远端那台的别名块与此无关：那个其实是 `src/shared/ccm-aliases.sh`，**3 行、一个别名都不定义**（只让 `ccm` 进 PATH 并接上别名文件），（`KR58D2` 那条对账钉着这一句：判据住
`src/frontend/shell/src/sftp.rs::tests::the_protocol_doc_sentence_about_the_alias_block_matches_the_file`）。

1. **那台**：`session-terminals {sid}`（§10）—— 此刻连着这个会话的终端：`SSH_CONNECTION` 四段 · 最近动静；
   或一条原因（`detached` · `no-terminal` · `unreadable`）。有原因 ⇒ 界面照它说，不往下走。
2. **本机**：`terminal-processes {terminals}`（§10）—— 界面把那台回话里的 `terminals` 原样交来；本机后端按 TCP 连接表四元组全等认出拥有连接的进程，
   回它往上的进程链；或一条原因（`not-ssh` · `elsewhere` · `mismatch` · `query-failed`）。
3. **monitor**：`bring_remote_terminal_to_front {chain}` —— 沿进程链从下往上，每一级先查握手表（`ps-registry`，§3）：
   登记过、登记的就是此刻这个进程（起始时刻对得上）且窗口校验通过 ⇒ 用它登记的窗口，精确到窗口；校验不过的不用。
   再看这一级名下有没有可见顶层窗口：有就停在这一级（终端窗口的属主；它以上的进程不在这个窗口里，它们的登记不拿来用）——
   恰好一个窗口 ⇒ 它；好几个 ⇒ 照实说分不清，并说怎么办（在那个窗口里新开一个 PowerShell 标签页、从那里重新连），不挑。
   整条链都没有 ⇒ 说没有窗口。拉前三重指纹校验（窗口还在 · 属主 pid · 属主起始时刻，`bind::bring_found_window`）。

### 已知边界

- Windows Terminal 只能拉到那个窗口，切不到具体标签页（没有按进程切标签的接口）；它开着好几个窗口时，只有连接所在的 PowerShell 登记过才认得出是哪一个
  （接入块 v7 起每开一个 PowerShell 就登记；当时不在前面的标签页、或接入之前就开着的，认不出 ⇒ 照实说）。
- 经跳板机 / 改端口的路由连过去的：这台的连接表对不上（`mismatch`），拉不到。
- mosh 这类不走 ssh 会话的：`SSH_CONNECTION` 是建立时那条，对不上。

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
     → Start-Process cc-monitor.exe --background
       （不抢前台焦点；v2 起不再死等 2s）

3. 生成 marker = "ccm-bind-<PID>-<8 字符 GUID>"

4. ★ 先设窗口标题 WindowTitle = marker（[System.Console]::Title）
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
    - 标题还是 marker ⇒ 恢复成 oldTitle（中途被别人改了就不动）
    - 循环外**再补查一次 registry**（吃「退出瞬间 registry 刚落地」）
    - ps-await 还在 ⇒ 自删
```

上图是敲 `cc` 那一份（前台）。开 PowerShell 时那一份（`__ccm_bind -Background`）第 1 步之后不做第 2 步，第 3–7' 步
交给后台一个空 runspace：等到「monitor 起来了」、且看得出它真在跑时才做（§2），做的是同一段认领，不出声。

### ★ 为什么第 4 步必须在第 5 步之前（v2 竞态修复）

monitor 的 notify 在 **await 文件落地那一瞬**就 EnumWindows 找 marker。旧顺序（先写文件、后设标题）
下 **monitor 扫得越快越容易找不到窗口** —— 然后它删掉 await 走失败路径，绑定成败全凭时序运气。
v2.21 实测：**每个新 shell 的首次 `cc` 固定烧满超时**。

两侧各修了一半，缺一不可：
- **PS 侧**（`src/shared/cc.ps1.tpl`）反转顺序 ⇒ 首次即中。
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
- 让 cc-monitor.exe portable：用户从 D 盘搬到 C 盘也无需重设
- monitor 启动时自动更新该路径，PS 端永远拿到最新值

---

## 添加新的跨进程协议文件

如果未来要加新的文件 IPC，必须：

1. **位置**：必须在 `~/.cc-monitor/` 下，路径白名单严格
2. **schema**：在本文档新增一节，定义所有字段 + 类型 + 默认值 + 可选性
3. **编码**：UTF-8 无 BOM，双向防御（写端无 BOM + 读端剥 BOM）
4. **原子写**：双端都用原子机制（PS `[IO.File]::WriteAllText` / Rust `MoveFileExW`）
5. **反序列化容错**：未知字段忽略（serde `#[serde(default)]` + `#[serde(other)]` enum variant）
6. **生命周期**：明确"短暂 vs 持久"，短暂的要明确超时机制
7. **更新 [`../../src/frontend/shell/README.md`](../../src/frontend/shell/README.md) 模块表 + [INVARIANTS.md](INVARIANTS.md)**
