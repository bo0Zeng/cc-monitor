# 全局不变量

跨模块约束清单。修改代码时**违反任一条都是 bug**，code review 时应该被指出。

每条都给出"理由"——为什么这条不能松动。

---

## 1. monitor 零侵入 Claude Code 数据源

`monitor` 对 `<claude_dir>/projects/**/*.jsonl` 和 `<claude_dir>/sessions/<PID>.json` **只读**。

**唯一例外**：`history::delete_history_session` —— 用户**显式**点删除二次确认后才执行；路径白名单 `starts_with(<claude_dir>/projects)` 防御越界。

**为什么不能松动**：cc-monitor 的核心价值主张是 "看 claude 的输出不破坏它"。一旦允许 monitor 写 jsonl，用户对 "数据源 = 我自己的命令痕迹" 的信任就崩了。

---

## 2. monitor 自己的 data dir 永远是 `~/.claude/claudecode-frontend/`

不跟随用户在 UI 改的 `claudeDir` 漂移。

**为什么不能松动**：
- 避免循环依赖：读 config 不能先解析 claudeDir，否则用户填错路径就再也打不开设置面板。
- 用户切换 Claude 数据目录后主题 / 字体偏好不丢。
- profile backup / sid-hwnd-cache / ps-await 等跨进程文件位置稳定，PS 端不需要动态查询。

---

## 3. 所有跨进程 JSON 文件 = UTF-8 无 BOM

- 写入端：Rust 用 `std::fs::write` 直接 UTF-8；PS 用 `[System.IO.File]::WriteAllText` + `UTF8Encoding($false)`（**禁用** `Out-File -Encoding utf8` —— PS 5.1 那个写 BOM）。
- 读取端：Rust 解析前 `raw.trim_start_matches('\u{feff}')` 兜底剥任何 BOM。

**为什么不能松动**：v1.7.0-1.7.7 整套 cc 集成"装上没用"7 个版本的真凶就是 BOM。`serde_json` 不剥 BOM 直接解析失败 → 早 return → 后续逻辑全跑不到。

---

## 4. profile 等用户文件写入 = ReplaceFileW + backup + 写后校验

不能用 `std::fs::rename` / `MoveFileExW` 直接覆盖用户文件。必须：

1. 备份原文件到 `<path>.ccm-backup-<ms>`
2. 写 `.tmp` → `ReplaceFileW(dst, tmp, NULL, REPLACEFILE_WRITE_THROUGH, ...)` 替换
3. `read_to_string` 回读校验长度
4. 不匹配 → 从 backup 恢复

**为什么不能松动**：
- `MoveFileExW(tmp, dst)` 用 tmp 的 ACL（继承父目录）覆盖 dst → 用户 explicit ACE 丢失 → Documents 重定向到非默认盘的用户读不了自己 profile。**ReplaceFileW 专门设计来保留 dst 的 ACL/ADS/创建时间**。
- OneDrive online-only placeholder / 杀软可能让 `read_to_string` 返 `Ok("")` 即"磁盘有内容读到空"，纯写就是清空用户内容 → backup + 校验是双保险。

详 [`profile_installer.rs`](../src-tauri/src/profile_installer.rs)。

---

## 5. JSONL 单一时序由 seq 字段 + RecordTimeline binary insert 共同保证（v2.6 B 重构）

**后端契约**：`watcher.rs::process_file` 给每读出的一行分配 per-file 单调递增的
`seq: u64`（`seqs: HashMap<PathBuf, u64>` 跨 process_file 调用累加）；
`bridge::JsonlLinePayload` 携带该 seq 字段；所有 emit 路径（jsonl-line / jsonl-batch）
都透传 seq 不变。

**前端契约**：每个 Tab / SessionViewer 持一个 `RecordTimeline`，
`insert(seq, element)` 用 binary search 找位置 → `stream.insertNode(element, anchor)`
按 seq 单调维护 DOM 顺序。后端 emit 顺序、chunked emit 块到达顺序、live / batch
路径混合，**对前端视觉顺序都无影响**。

**为什么不能松动**：
- 之前用"多 flag 协调"路径（PayloadSource batch/live + inPrependMode + pendingPrependFragment
  + EventReplay.replaying 等 5 个 flag）反复出 inter-flag 相位 bug（详 ADR-021）。
  v2.6 B 重构把所有 flag 替换为 seq + binary insert。
- chunked emit 期间 watcher push 的真新行直接走 jsonl-line emit 出去；前端 timeline
  按 seq 把它们放到正确位置——不再需要"replaying 期间 push 等末块后 catch-up"的
  特殊路径。

**演进**：v2.3 加 chunked emit / v2.4 加 PayloadSource / v2.5 加 replaying flag + catch-up tail
（详 v2.4-active-tab-sync-notes.md + v2.6-b-refactor-notes.md）—— 都在试图修补"多 flag
状态机相位 bug"。v2.6 B 重构是一次性消除整套机制。

详 [v2.6-b-refactor-notes § 3](../../doc/v2.6-b-refactor-notes.md) + ADR-021/022。

---

## 6. session 探活双重校验（PID + procStart）

`is_session_active(sid)` = `OpenProcess(QUERY_LIMITED) + GetExitCodeProcess == STILL_ACTIVE` + `GetProcessTimes` creation FILETIME 与 sessions/<PID>.json 里 `procStart` 字段（= .NET `DateTime.ToFileTime()` 字符串）100ms 容差比对。

**为什么不能松动**：Windows PID 短期复用非常常见。仅靠 STILL_ACTIVE 会把"旧 PID 已被无关进程占用"误判为活跃 session → 僵尸 Tab。

---

## 7. HWND 拉前三重校验

`bring_terminal_to_front(sid)` 拉前**之前**必须：

1. `IsWindow(hwnd)` 返回 true
2. 当前 `GetWindowThreadProcessId.owner_pid == 绑定时 owner_pid`
3. 当前 `GetProcessTimes(owner).creation == 绑定时 owner_proc_start`

任一失败 → 拒拉前 + toast 报告失败原因。

**为什么不能松动**：HWND 复用比 PID 复用还高频（Windows 重用窗口句柄）。不校验会拉起无关的窗口。

---

## 8. Tauri State 必须 `app.manage`

任何 `#[tauri::command] fn cmd(state: State<'_, Arc<X>>)` 都对应 `setup()` 里 `app.manage(x.clone())`。漏 `manage` 不会被 `cargo check` 抓住，运行时调用该 IPC 时 panic。

修改 State 注册矩阵时**强制**走 [STATE-MATRIX.md](STATE-MATRIX.md) § 修改规则 的 grep checklist。

**为什么不能松动**：撤回某 State 时漏 `manage` 别的 IPC 依赖造成"5 个版本带病"是真实历史事故。`cargo check` 通过不代表运行时通过。

---

## 9. 排序的硬规则：一律按 `seq`，禁止按到达顺序

§ 5 描述了 seq + RecordTimeline 的机制（链路与契约不在此重复）。本条是从中派生的一条**强制约束**，单列以便 grep / review 时引用：

**禁止**任何"按到达顺序排序 / 拼接"的代码——包括前端临时缓冲、后端 chunked emit 假设的 head/older 区分、live 与 batch 路径分别维护顺序等。一切顺序**必须**取自 `seq` 字段。

**为什么不能松动**：jsonl 的行顺序就是用户对话的时间顺序。乱序 = 看到 Claude 先回复、再出现 user 提问 → 完全没法用。seq 不依赖 emit 时机，所以跨 snapshot / live / chunked replay 各种边界都成立；任何"按到达顺序"的捷径都会在某个边界破坏时序（历史上多 flag 状态机的相位 bug 根因，详 § 5 + ADR-021/022）。

---

## 10. 长耗时 IO / 系统调用 = `tokio::task::spawn_blocking`

任何**可能阻塞数十毫秒以上**的同步调用必须走 `tokio::task::spawn_blocking`，不能直接在 IPC handler 跑。前端拉前类 IPC 再加 5s timeout 兜底。

具体包括：

- **Win32 同步调用**：`EnumWindows` / `SetForegroundWindow` / `ShellExecuteW` / `OpenProcess` 等（窗口枚举 / 进程查询 / shell execute 可能数十 ms 到秒级）
- **文件系统 IO**：`history.rs` 全部 IPC（`list_history_projects` / `stream_history_sessions_in_project` / `stream_read_session_jsonl`）也走 spawn_blocking —— 扫几十个项目 / 读几 MB jsonl 都属此类
- **`std::process::Command::spawn`**：spawn 外部进程（如 resume 的 wt.exe / powershell.exe 跑 `cc`/`claude --resume`，v2.8.1 起）

**为什么不能松动**：Tauri 的 `#[tauri::command] fn`（非 async）跑在 IPC 派发线程上。一个慢命令阻塞期间，其他 IPC 全部排队 → 整个 UI 没反应（切设置 / 拉前 / 切 Tab 全失灵）。即便代码"看起来快"（如 read_dir + stat 几百次），磁盘冷状态下也能轻松超过 100ms 阈值。

**实施口诀**：IPC 命令默认写 `pub async fn`，函数体包 `tokio::task::spawn_blocking(move || { ... }).await.map_err(...)?`。State 参数前先 `state.inner().clone()` 拿 Arc 再 move 进 closure。

---

## 11. 跨平台分裂边界

所有 Win32 调用必须在 `#[cfg(windows)]` 块；非 Windows 平台给降级实现（返 `Err("not supported")` 或 stub）。当前 v1.x 仅 Windows，但代码必须保持非 Windows 平台**能编译通过**。

**为什么不能松动**：方便未来 v2 跨平台移植；现在不维护这个约束，将来要全文 review 修一遍 cfg。

---

## 12. 前端 alert 不算错误反馈

`alert("xxx 失败：")` 在生产 build 弹窗用户可能没看清就关掉。**关键失败必须**：

1. `tracing::error!` / `console.error` 留 log（F4 issue #4 实现 GUI log 文件后会持久化）
2. **状态栏 toast 红色 3-5s 提示**（不是 alert 弹窗）
3. 严重 / 持续性错误 → banner 顶部提示

**为什么不能松动**：v1.7.9 设置面板 [打开 profile] 按钮的 "Permission denied" alert 用户看到了但没注意到关键信息，导致以为按钮坏了。alert 是糟糕的错误 UX，必须辅以更可见的反馈。

---

## 13. CSS portal 元素必须真挂 body

任何用 `position: fixed` 实现的浮层（tooltip / modal / dropdown）**必须**挂到 `document.body` 而不是当前组件的子节点。

**为什么不能松动**：CSS spec：祖先有 `transform` / `filter` / `perspective` / `will-change: transform` 时，`position: fixed` 后代的 containing block 从 viewport 重置到那个祖先 → fixed 失去 viewport-anchored 特性。`.settings-panel` 有 `transform: translateX(0)` 做 slide-in 动画，挂它子树的 fixed 元素会乱跑。挂 body 脱离 transform 子树是唯一可靠路径。

---

## 14. localStorage / IndexedDB key 必须前缀 `cc-monitor.`

前端任何持久化到 localStorage / IndexedDB 的 key 必须以 `cc-monitor.` 开头。

**为什么不能松动**：WebView2 的 origin 是 `tauri://localhost`，跟其他可能也用这个 origin 的 Tauri 应用共享存储（理论上）。前缀避免冲突 + 数据透明化展示时容易过滤。

---

## 15. logging 子系统失败不能阻塞 monitor 启动

`logging::init()` 在 `tauri::Builder` 之前调用（tracing 全局 dispatcher 必须在 Builder 之前 init）。它内部做的所有事情——创建 logs 目录、构造 rolling appender、注册 ErrorEmitterLayer——**任一失败都必须 fallback 到 stdout-only，让 monitor 仍能起来**。

- log 目录创建失败 → `eprintln!` 报错，file layer = None，subscriber 仍 init 但只发到 stdout
- rolling appender 构造失败（罕见——磁盘满 / NTFS quota）→ 同上
- tracing `try_init` 已经有 subscriber 报错（测试场景）→ `eprintln!` + 继续，不 panic
- `monitor_data_dir` 解析失败（极罕见）→ fallback 到 `temp_dir().join("cc-monitor-fallback")`

**为什么不能松动**：log 是诊断辅助，不是核心功能。"装了 monitor 但 log 文件没法写所以打不开" 是用户最反感的反讨厌设计。INVARIANT § 2 说 monitor data dir 永远在 `~/.claude/claudecode-frontend/`——log dir 是 `<data_dir>/logs/`，data dir 解析永远应该成功（dirs::home_dir 在 Windows 99.99% 有值），剩下唯一失败路径是文件系统级 error 必须容忍。

详 [`logging.rs`](../src-tauri/src/logging.rs) 的 `init()` 函数。

---

## 16. monitor 单实例运行

同 user / 同机器同时只允许一个 cc-monitor 进程。由 [`tauri-plugin-single-instance`](https://v2.tauri.app/plugin/single-instance/) 强制 —— **必须是 Builder 链上第一个 plugin**（plugin 文档约束）。第二个实例启动时：

1. plugin 通过 OS mutex 检测到第一个实例存在
2. 通知第一个实例的回调（在 [`lib.rs::run()`](../src-tauri/src/lib.rs) 里 `unminimize + show + set_focus` 主窗口）
3. 第二个实例自身立即退出

**为什么不能松动**：cc-monitor 全局共享多个文件状态 —— `auto-launch.json`、`ps-await/`、`ps-registry/`、`sid-hwnd-cache.json`、jsonl watcher、`logs/monitor.YYYY-MM-DD.log`。两个 monitor 同时跑会触发：

- 双重渲染（两个窗口都监听同一 jsonl）
- cc 握手 race（两个 monitor 都 EnumWindows 找 marker，先到先赢 / 后到的写不到 `ps-registry/`）
- 不可预测的 `auto-launch.json` last-writer-wins 覆盖

跨 user session（同一台机器两个用户登录）不冲突 —— plugin 默认 mutex 是 user-scoped。

详见 issue #9。

---

## 17. 前端 IPC drain 必须捕获单条记录异常 + 用户数据遍历必须迭代

两条相关的稳定性约束，都来自 v2.1.1 hotfix。

### 17a. drain 必须 try/catch 单条 record

[`src/events.ts`](../src/events.ts) 的 `drain` 循环每次 `handlers.onLine(p)` 必须 try/catch。单条 record 处理抛错时**只**记 log + 跳过该条，**不能**让异常逃出 while 循环 —— 否则后续上千条 record 永远滞留 queue 不被处理，前端看上去就停止刷新了。

### 17b. 用户数据深度的遍历必须迭代而非递归

任何对 jsonl 记录、Tab 历史、subagent 链等**用户数据驱动深度**的图 / 树遍历，必须用迭代算法（while 循环 + 显式 stack / Kahn 拓扑序等），不能依赖 JS 函数调用栈。

**为什么不能松动**：
- WebView2 (Chromium) 的 JS stack 在 ~1000-10000 frames 附近触底，跟 V8 配置和宿主有关
- Claude session parent 链典型几乎线性，几千条记录是常态
- 真递归一旦炸 stack 就是 RangeError，**异常**而不是返回错误值 —— 17a 的 drain 防御能保证整个 replay 不被冻死，但**单条数据从此渲染不出来**仍然是 bad UX
- 写算法时凭直觉用真递归（"O(N) 嘛"）但忽略 stack 深度，是已经踩过的坑

**已纠正**：v2.1.0 `computeMainBranch` 的 `dfsLatest` + `walkMain` 真递归 → v2.1.1 改 Kahn 拓扑序 + while。issue #14 的 `src/cards/diff.ts::diffLines` 亦据此用**迭代 LCS**（DP 矩阵 + 迭代回溯 + `m*n` cell-budget 守卫退化），上千行的 Edit 不爆栈、不分配巨矩阵。

**未来加新代码注意**：处理 jsonl 记录树、event_replay 历史、subagent 嵌套时如果想写 `function f(node) { ... f(child) }`，停一下，改成 `while (stack.length > 0)` 风格。

---

## 18. Claude Code 写的元数据文件按"宽容 schema" 反序列化

任何对 Claude Code CLI 写入的文件做 `serde_json::from_str` 时，**所有非核心字段**必须 `#[serde(default)]` 或 `Option<T>`，**只把绝对必填**（如 `sessionId`、`pid`）当强制字段。

**为什么不能松动**：实测 Claude Code 2.1.150 写 `~/.claude/sessions/<PID>.json` 时**偶发漏 `procStart` 字段**——同版本不同 session 写法不一致，可能是某种启动路径（`/resume`？多线程 race）下 procStart 还没拿到就先写文件，后续 status 更新路径不补写。

v2.4.2 之前 `SessionInfo.proc_start: String` 必填 → serde 直接解析失败 → `read_one` 返 None → 整个 session 被静默忽略 → monitor 漏 Tab。修复 `Option<String>` 后，缺失时 `is_process_alive` 跳过 PID 复用校验只看 STILL_ACTIVE，**代价是极小概率误判活跃但远好过完全看不见 Tab**。

**应用范围**：
- `sessions/<PID>.json` (`session_map::SessionInfo`) —— procStart 字段在 v2.6 后端按 `Option<String>` 反序列化；调用点用 `utils::NetTicks::parse_str` 转 typed value 比较（详 ADR-024）。同样 wire 字符串可缺，Rust 内部用 newtype 隔离避免跟 `bind.rs::HwndEntry.owner_proc_start` (FILETIME) 单位混用
- `tasks/<sid>/<id>.json` (`tasks::TaskEntry`) — 已经按宽容处理
- `projects/**/*.jsonl` 的 `messages::JsonlRecord` enum — 用 `#[serde(other)] Unknown` 兜任何未知 type
- 未来添加任何 Claude Code 数据源读取一律照此办

**反过来**：monitor **自己写的**文件（`config.json` / `auto-launch.json` / `ps-registry/<PID>.json` 等）schema 可以严格——这是 monitor 控制的产物，schema 演进有版本管理。

---

## 19. 跨 windows crate 版本 HWND 互操作走 `as isize`

cc-monitor 直接依赖 `windows = "0.56"`（`HWND.0 = isize`），但 Tauri 2 内部用 `windows = "0.61"`（`HWND.0 = *mut c_void`）—— Cargo.lock 两个版本共存。

跨版本传 HWND 必须：

```rust
let tauri_hwnd = win.hwnd()?;                              // 0.61 HWND
let hwnd_value = tauri_hwnd.0 as isize;                    // pointer → isize cast
let h = windows::Win32::Foundation::HWND(hwnd_value);      // 0.56 HWND
```

**为什么不能松动**：直接 `transmute` 在两版本字段类型不同时是 UB。`as` cast `*mut c_void → isize` 在 64-bit Windows 上是合法的 pointer-to-integer cast，编译器保证语义正确。

**反过来**：不要尝试统一两个 crate 版本——Tauri 锁死其内部依赖版本，外部强制 align 会触发其他依赖的连锁升级风暴。两版本共存 + 边界处 cast 是最干净的解法。

---

## 20. 用户 input 检测必须分辨"真用户输入" vs "CLI 注入 noise"

任何"用户在终端输入"的行为感知（如 v2.4 issue #2 的自动切 Tab）**不能**只看 `JsonlRecord::User`——Claude Code 把多种非用户行为也写成 `type=user`：

| 形态 | content 长啥样 | 是不是真用户输入 |
|---|---|:---:|
| 真用户敲键 | `[{type:"text", text:"..."}]` 或纯字符串 | ✓ |
| Slash 命令 / compact summary | 用户主动行为 | ✓ |
| CLI 内部 prompt 包装 | 被 `stripInternalNoise` 剥光 | ✗ |
| **ESC 中断标记** | `[Request interrupted by user]` / `[Request interrupted by user for tool use]` | ✗ |
| 工具结果回灌 | `[{type:"tool_result", ...}]` Anthropic API schema | ✗ |
| `<synthetic>` 包裹 | claude 内部应答 | ✗ |

**判别标准**：复用前端 `cards/index.ts::renderMessage` 的 `result.kind === "card"`，它已经把以上所有非真输入路径过滤到 `kind: "skip"` 或 `kind: "tool-group"`。判 user-active 时**只看 card 且 type === "user"**。

**为什么不能松动**：v2.4 issue #2 v2.4.0 没考虑 ESC 中断的 `[Request interrupted by user]` 也是 `type=user`，导致用户按 ESC 时 monitor 错误抢前台。v2.4.2 把它加进 `stripInternalNoise` 让走 skip 修复。

**未来加新的"用户行为感知"特性**（如：跨 Tab 跳焦提示、统计用户敲键次数等）必须先确认信号来源是否经过 `renderMessage` 过滤；如果走 raw `JsonlRecord::User` 必须独立维护一份等价过滤。

---

## 21. 启动重放滚动稳定性（贴底不抖）

`MessageStream`（[`src/stream.ts`](../src/stream.ts)）维持贴底时必须遵守三条，违反任一条都会让启动重放期间"最新消息整行高频上下微抖"回归：

1. **`snap()` 必须守卫**：只在 `scrollHeight - clientHeight - scrollTop > 1`（确实落后底部）时才写 `scrollTop`。**禁止**每帧无脑 `scrollTop = scrollHeight`。
2. **「视口上方」插入不手动补偿 scrollTop**：`insertNode` 对 anchor≠null（插到中间/上方）的情况不调整 scrollTop，交给浏览器原生 CSS `overflow-anchor`（默认 auto，**禁止**给 `.stream` 设 `overflow-anchor: none`）维持视觉稳定。手动补偿 + anchoring 会 double-shift。
3. **重放期「视口上方」旧内容延后批量挂载**：`RecordTimeline` 在 deferMode（启动重放）下，插到非末尾的旧消息只进数组不挂 DOM；`flushDeferred()` 在 `onBatchEnd` 一次性挂回，且**必须在 `branchFolder.flushPending()` 之前**（后者要扫完整 DOM 算主线/折叠）。末尾追加（最新内容、用户正看着的）仍立即挂，首屏不受影响。

**为什么不能松动**：根因实测定位 —— 末块先发的重放把旧消息逐条插到"贴底视口的上方"，持续约 60 帧；每次上方插入都触发浏览器重排 + 重做 scroll anchoring，而 HiDPI / 高刷屏分数像素下，整数 `scrollHeight` 与分数布局的舍入误差**每帧不同** → 整块内容逐帧 ±0.5px 高频重绘。压成"一次性批量挂载"后实测抖动帧数 66 → 1。注意：`scrollTop` 本身并不震荡（单调增长），所以**只测 `scrollTop` 发现不了这个 bug**，要测可见元素 `getBoundingClientRect().top` 的逐帧反转。

详 `D:/Sync/文档/claudecode-frontend/doc/scroll-jitter-investigation.md`（项目外排查复盘）。

---

## 22. 独立 viewer 窗口（issue #10）四条契约

独立只读窗口（`viewer-<sid>`，`bootstrapViewer`）依赖四条，违反任一条都会让窗口白屏 / 卡死 / 收不到数据：

1. **开窗 IPC 必须 `async`**：`open_session_in_new_window` 等创建 `WebviewWindow` 的命令必须是 `async fn`。Tauri 2 同步 `fn` 命令在**主线程**执行，而 `WebviewWindowBuilder::build()` 内部把创建派发到主线程并阻塞等 → 在主线程等主线程 = 死锁（新窗口白屏 + 整个 app 卡死连关闭都点不了）。
2. **定向事件 target-kind 对齐**：给单个 viewer 窗口定向投递（如 `replay_session_to_window`）用 Rust `emit_to(EventTarget::webview_window(label))` ↔ 前端 `getCurrentWebviewWindow().listen`（`bindEvents({windowScoped:true})`）。**禁止**用 `&str` 目标（→`EventTarget::AnyLabel`）配模块级 `listen`（→`Any`）—— Tauri 2 按 kind 匹配，`Any` 监听命不中 `AnyLabel` 发射，事件静默丢弃。`AppHandle::emit` 广播（`Any`）是通配，带标签监听仍收得到 live 增量。
3. **`bindEvents` 必须 await 再触发 emit**：`listen()` 异步注册，注册完成前后端 emit 的事件会丢。viewer 在 bindEvents 后立刻调 replay，所以 `bindEvents` 返回 Promise 且 caller 必须 await（主窗口 emit frontend-ready 同理）。
4. **viewer-mode CSS grid 行数随可见 item 定义**：`#tab-bar` `display:none` 会把它**从 grid item 移除**，剩余子元素自动前移一行。所以 viewer 必须只为剩余 item 定义对应行数（`auto 1fr 24px`），否则 message-stream 落进多余行被压成 0 高（整窗只剩状态栏）。

**为什么不能松动**：四条都是实测踩出来的（详见排查复盘），且都"静默失败"——不报错，只是白屏 / 收不到 / 卡死，极难凭看代码发现。

详 `D:/Sync/文档/claudecode-frontend/doc/viewer-window-investigation.md`（项目外排查复盘）。

---

## 23. 复用 `.stream` 容器的非-Tab 视图必须显式恢复可见

基类 `.stream`（[`src/styles.css`](../src/styles.css)）默认 `visibility: hidden` + `pointer-events: none`，仅 `.stream.active` 才可见——这是**多 Tab 机制**：N 个 tab 各持一个 `.stream`，TabManager 只给当前 tab 加 `.active`。

任何**复用 `.stream` 样式但不归 TabManager 管的视图**（`SessionViewer` 应用内只读查看器、未来别的只读流）**必须在自己的 CSS 里显式 `visibility: visible`**，因为它们的元素永远不会拿到 `.active` 类。

**为什么不能松动**：v2.8.1 的"历史会话点进去空白"就是这个坑——`SessionViewer` 流元素 class 是 `stream session-viewer-stream` 没有 `.active`，命中基类 `visibility: hidden`，2000+ 张卡片全渲染进 DOM 却不可见，而状态栏（不在 `.stream` 内）照常显示记录数 → "有记录却空白"的迷惑现象。独立 viewer 窗口（§ 22）复用 TabManager 所以有 `.active`，不受影响；只有自建流的 `SessionViewer` 中招。**禁止**给非-Tab 视图的流元素只复用 `.stream` 而不补 `visibility: visible`。

详 `D:/Sync/文档/claudecode-frontend/doc/v2.8.1-bugfix-notes.md`（项目外排查复盘）。

---

## 修改本文档

加新的不变量时：

1. 加到本文档对应位置 + 编号
2. 在 `src/` 或 `src-tauri/` 对应模块的 doc comment 里加引用 `// 违反此约束见 doc/INVARIANTS.md § N`
3. 如果不变量需要 grep checklist（如 State 注册），加到 [CONTRIBUTING.md](CONTRIBUTING.md) 对应 checklist

删除某条不变量（极少）：

1. 写 RFC 解释**新的约束**是什么、为什么旧的可以松动
2. PR 描述里链到这条 RFC + 全代码库 grep 受影响处确认全修
