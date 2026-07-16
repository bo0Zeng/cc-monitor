# 全局不变量

跨模块约束清单。修改代码时**违反任一条都是 bug**，code review 时应该被指出。

每条都给出"理由"——为什么这条不能松动。

---

## 1. monitor 零侵入 Claude Code 数据源

`monitor` 对 `<claude_dir>/projects/**/*.jsonl` 和 `<claude_dir>/sessions/<PID>.json` **只读**。

**例外（穷举，各自路径白名单防越界）**：
1. **本地删除**：`history::delete_history_session` —— 用户**显式**点删除二次确认后才执行；白名单 `starts_with(<claude_dir>/projects)`。
2. **远端 daemon 自部署（issue #29，`sftp::ensure_daemon_deployed`）**：经 SFTP 写远端 `~/.cc-monitor/bin/`（daemon 二进制 + `.build_id` 标记）—— **非用户数据、幂等、版本门控**；只写 cc-monitor 自己的 bin 目录，**绝不碰** `~/.claude/`。
3. **远端历史删除（issue F11，`remote_history::delete_remote_history_session` → `sftp::remove_remote_file`）**：用户**主动**点删除 + 前端**二次确认**后，经 SFTP 移除远端 `~/.claude/projects/` 下的 jsonl；**双重路径守卫**（`is_safe_remote_jsonl`：须 `.jsonl` + 含 `/projects/` + 无 `..`；并 SFTP `canonicalize` 解 symlink 后再校验）。**注**：标星 / 重命名 / 隐藏是 **monitor 本地元数据**（`history-metadata.json` 按 sid），**不写远端**——唯一写远端的用户数据操作就是删除 jsonl。

**为什么不能松动**：cc-monitor 的核心价值主张是 "看 claude 的输出不破坏它"。一旦允许 monitor 在用户数据上**非显式**写，用户对 "数据源 = 我自己的命令痕迹" 的信任就崩了。上述豁免要么是**非用户数据**（自部署 bin），要么是**用户显式动作**（删除 / metadata），且各带独立 realpath 白名单。

**F47 SFTP 文件面板不在本约管辖内（澄清，非例外/非松动）**：Batch14-F47 起 cc-monitor 挂了一个**用户亲自驱动的通用 SFTP 文件传输面板**（浏览/上传/下载/改名/删除任意用户文件）。它是**独立文件传输功能**，与本约「monitor 作为监视器只读 Claude 数据源」**正交**——它写的是用户浏览到的普通文件，不是 Claude 的 jsonl/pidfile，且每次写都是面板内一次直接用户手势（绝无自动/后台写）。**防误伤守卫**（`sftp_pool::is_protected_claude_data_path`）:SFTP 写命令**拒碰** `~/.claude/projects/**/*.jsonl` 与 `~/.claude/sessions/*.json`（往正被 Claude 打开的会话文件写会损坏会话；要管这些用历史浏览器）。SFTP 面板走独立 utility 连接池，与数据源流连接分离。

**F62 从历史某轮建分支不在本约管辖内（澄清，非例外/非松动，用户 2026-07-12 拍板）**：`history::create_branch_session` 在用户**显式**点历史查看器里某条消息的 `⑂` 时，把 `[根…该消息]` 前缀**复制**成一个**全新** `<new-sid>.jsonl`（原生 `/branch` 的 `forkedFrom` 格式）。这与本约**正交**——本约防的是 monitor **改坏/覆盖/后台写**它正在监视的**现存**会话文件；建分支是**纯新增产出**（用户框定："复制产出一个文件，而非侵入式改动"），**原会话一字节不改**，且只写**新生成、collision-check 过的 sid**（`out_path.exists()` 则拒，绝不覆盖任何现存会话）。防越界守卫 `validate_branch_source`（canonicalize + `starts_with(projects)` + `.jsonl`）与 delete 同构。破坏性上它比已放行的「显式删除」更弱（只增不减）。

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
都透传 seq 不变。seq 保证**时序**、不保证**投递次数**——投递语义是 at-least-once
（截断重读会换新 seq 重投整个文件），详 § 25。

**前端契约**：每个 Tab / SessionViewer 持一个 `RecordTimeline`，
`insert(seq, element)` 用 binary search 找位置 → `stream.insertNode(element, anchor)`
按 seq 单调维护 DOM 顺序。后端 emit 顺序、chunked emit 块到达顺序、live / batch
路径混合，**对前端视觉顺序都无影响**。

**为什么不能松动**：
- 之前用"多 flag 协调"路径（PayloadSource batch/live + inPrependMode + pendingPrependFragment
  + EventReplay.replaying 等 5 个 flag）反复出 inter-flag 相位 bug。
  v2.6 B 重构把所有 flag 替换为 seq + binary insert。
- chunked emit 期间 watcher push 的真新行直接走 jsonl-line emit 出去；前端 timeline
  按 seq 把它们放到正确位置——不再需要"replaying 期间 push 等末块后 catch-up"的
  特殊路径。

**演进**：v2.3 加 chunked emit / v2.4 加 PayloadSource / v2.5 加 replaying flag + catch-up tail —— 都在试图修补"多 flag
状态机相位 bug"。v2.6 B 重构是一次性消除整套机制。

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

**为什么不能松动**：jsonl 的行顺序就是用户对话的时间顺序。乱序 = 看到 Claude 先回复、再出现 user 提问 → 完全没法用。seq 不依赖 emit 时机，所以跨 snapshot / live / chunked replay 各种边界都成立；任何"按到达顺序"的捷径都会在某个边界破坏时序（历史上多 flag 状态机的相位 bug 根因，详 § 5）。

---

## 10. 长耗时 IO / 系统调用 = `tokio::task::spawn_blocking`

任何**可能阻塞数十毫秒以上**的同步调用必须走 `tokio::task::spawn_blocking`，不能直接在 IPC handler 跑。前端拉前类 IPC 再加 5s timeout 兜底。

具体包括：

- **Win32 同步调用**：`EnumWindows` / `SetForegroundWindow` / `ShellExecuteW` / `OpenProcess` 等（窗口枚举 / 进程查询 / shell execute 可能数十 ms 到秒级）
- **文件系统 IO**：`history.rs` 全部 IPC（`list_history_projects` / `stream_history_sessions_in_project` / `stream_read_session_jsonl`）也走 spawn_blocking —— 扫几十个项目 / 读几 MB jsonl 都属此类
- **`std::process::Command::spawn`**：spawn 外部进程（如 resume 的 wt.exe / powershell.exe 跑 `cc`/`claude --resume`，v2.8.1 起）
- **async task 内禁止 `std::thread::sleep` / 同步阻塞**（issue #20 增补）：`tauri::async_runtime::spawn` 的 task 里节流用 `tokio::time::sleep(..).await`，真长阻塞走 spawn_blocking。一次同步 sleep 压住一个 tokio worker，worker 数有限，攒多了饿死全部 async 任务（`replay_and_mark_ready` 为此 async 化；`on_line_batch` 大 batch 路径的 sleep 已于 Batch5-F17 一并移除——块序列 spawn + `tokio::time::sleep`。**附带顺序契约**：spawn 入口返回≠emit 完成，与其他通道的相对顺序不保证——顺序敏感调用方（ssh_source 攒批 flush，行 emit 必须先于 SessionRemoved 归档）必须用 `on_line_batch_awaited`）

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

**附带契约（Batch5-F19）**：主窗口/viewer/tear-off 共享同一 origin 的 localStorage——**非主窗视图写共享 key 前必须显式隔离**。现行履约点：`TabManager.persistLastActive`（viewer 置 false，防独立窗口看会话 X 污染主窗口的 `cc-monitor.last-active-sid` 记忆；vitest 钉住）。新加共享 key 的写入方须同样审视多窗口写者问题。

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
- `sessions/<PID>.json` (`session_map::SessionInfo`) —— procStart 字段在 v2.6 后端按 `Option<String>` 反序列化；调用点用 `utils::NetTicks::parse_str` 转 typed value 比较。同样 wire 字符串可缺，Rust 内部用 newtype 隔离避免跟 `bind.rs::HwndEntry.owner_proc_start` (FILETIME) 单位混用
- `tasks/<sid>/<id>.json` (`tasks::TaskEntry`) — 已经按宽容处理
- `projects/**/*.jsonl` 的 `messages::JsonlRecord` enum — 非核心字段一律 `Option`/`default`。**未知 type 的处理见 § 18.1（F63 起变了）**。
- 未来添加任何 Claude Code 数据源读取一律照此办

### 18.1 看不懂的记录不静默丢 —— 抢救成 `Unrecognized`（F63 / issue #49）

宽容 schema 挡的是「已知类型多了未知字段」（serde 默认忽略，整行不丢）。但**两条丢失路径它挡不住**，F63（2026-07-16）补上：

1. **未知 `type`** → serde 落到 `#[serde(other)] Unknown`。**`Unknown` 现在只是 serde 落点，绝不出 `parser::parse_line`** —— 它被抢救成 `JsonlRecord::Unrecognized`（留 `raw` 原文 + `uuid`/`parentUuid`/`timestamp`）。
2. **已知 `type` 但字段解析失败**（`from_str` 返回 Err）且原文仍是合法 JSON → 同样抢救成 `Unrecognized`（`reason="parse-failed: …"`，并 `tracing::warn` 一条）。只有**连 JSON 语法都不成立**的行才仍返回 `Err`。

**为什么**：记录一旦静默消失，它的 children 的 `parentUuid` 就指向集合外 → `branching.ts:100-106` 判孤儿 root → 死胡同 plain user root **整棵误折叠**（`branching.ts:24` 早预警、2026-06-13 咬过一次）。`Unrecognized` 进 `is_displayable()` 白名单（照 `Attachment` 先例：不建卡但进链）；前端 `branching.ts::extractBranchRecord` 白名单含 `"cc-monitor-unrecognized"`。

**实测**（本机 771 会话 / 16 万行）：7 个未知 type 共 ~8,800 条以前被静默丢弃（占 5.6%），**uuid 全为 0**——即此刻并没有在误折叠，F63 是**保险**（不再丢 + Claude 发带链身份新类型时自动扛住）。`cc-monitor-unrecognized` 是**本地自造信封**（前缀防撞真类型），**不是** Claude 真实 jsonl 类型、不参与两端 schema 对账。

**反过来**：monitor **自己写的**文件（`config.json` / `auto-launch.json` / `ps-registry/<PID>.json` 等）schema 可以严格——这是 monitor 控制的产物，schema 演进有版本管理。

---

## 19. 跨 windows crate 版本 HWND 互操作走 `as isize`

cc-monitor 直接依赖 `windows = "0.56"`（`HWND.0 = isize`），但 Tauri 2 内部用 `windows = "0.61"`（`HWND.0 = *mut c_void`）—— Cargo.lock 两个版本共存。F12 起 cc-monitor 也经 `windows-wv2`（package rename，见 Cargo.toml）**直连** 0.61：nudge 的 WebView2 controller COM 调用参数（`RECT`）必须用 webview2-com 0.38 配对的 0.61 类型，与 0.56 **不互通**——仍是两版本共存、不强行统一，需要跨界的值按本条规则 cast。

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
2. **「视口上方」插入不手动补偿 scrollTop**：`insertNode` 对 anchor≠null（插到中间/上方）的情况不调整 scrollTop，交给浏览器原生 CSS `overflow-anchor`（默认 auto，**禁止**给 `.stream` 设 `overflow-anchor: none`——补批在**同一同步任务内**的临时关闭是唯一豁免类——两个实现点：`tabs.fillAbove`（F40b）与 `session-viewer.maybeFillAbove`（F39），见第 3 条，且还原必须在 finally）维持视觉稳定。手动补偿 + anchoring 会 double-shift。
3. **重放期「视口上方」旧内容不建 DOM（Batch13-F40a 尾部优先收纳）**：`TabManager.onLine` 按 seq 门控——`seq < tab.window.floorSeq` 的旧记录只进 `TailWindow` 账本（meta/branch 数据经 `routeMetaAndBranch` 照喂），**根本不建卡不挂 DOM**；后台 virgin tab 在 `onBatchEnd` 空闲物化尾段 / `switchTo` 时同步物化。**启动重放**的上方插入从源头消失——严格强于历史方案 deferMode（延后到一帧批量挂载，2026-07 F40a 退役）。**大增量批**（>600 行落已渲染 tab → 后端切块、末块先发）的老块中部插入由 F40b 关闭：批期「`seq ≥ floor` 且 `< timeline.maxSeq`」的记录进 per-tab `midBatchBuffer`，`onBatchEnd` 排序后一次 `batchInsert` 挂载——逐帧中部插入为 0。**上翻补批（F40b fillAbove）**：临时 `overflow-anchor:none`（防 WebView2 原生锚定与手动补偿 double-shift）、测量→渲染→`scrollTop += ΔscrollHeight` 回写必须在**同一同步任务**内完成（不许 await/rAF 打断）；选区进行中暂缓补批（unwrap/rebuild 会杀选区）。**物化/补批插卡前必须 `branchFolder.unwrapAll()` 摊平、插完 `rebuildNow()` 无条件重折**（`flushPending` 的 setsEqual 短路会把摊平永久化），批间孤儿 tool_result 由 `reconcilePendingToolResults` 回填——注意 fallback 单元是**组内实体**（timeline entry 是组 root），reconcile 摘单元后若组壳已空会连根摘壳并返回 root，**调用方必须对返回元素 `timeline.removeByElement` 出账**（否则账上挂已离场的组 root）；`MessageStream.insertNode` 对「anchor 不是 contentEl 直接子节点」有爬升/降级防御（防 NotFoundError 丢记录）。

**为什么不能松动**：根因实测定位 —— 末块先发的重放把旧消息逐条插到"贴底视口的上方"，持续约 60 帧；每次上方插入都触发浏览器重排 + 重做 scroll anchoring，而 HiDPI / 高刷屏分数像素下，整数 `scrollHeight` 与分数布局的舍入误差**每帧不同** → 整块内容逐帧 ±0.5px 高频重绘。deferMode 时代压成一帧后实测抖动帧数 66 → 1；F40a 后上方插入次数为 0。注意：`scrollTop` 本身并不震荡（单调增长），所以**只测 `scrollTop` 发现不了这个 bug**，要测可见元素 `getBoundingClientRect().top` 的逐帧反转。

---

## 22. 独立 viewer 窗口（issue #10）四条契约

独立只读窗口（`viewer-<sid>`，`bootstrapViewer`）依赖四条，违反任一条都会让窗口白屏 / 卡死 / 收不到数据：

1. **开窗 IPC 必须 `async`**：`open_session_in_new_window` 等创建 `WebviewWindow` 的命令必须是 `async fn`。Tauri 2 同步 `fn` 命令在**主线程**执行，而 `WebviewWindowBuilder::build()` 内部把创建派发到主线程并阻塞等 → 在主线程等主线程 = 死锁（新窗口白屏 + 整个 app 卡死连关闭都点不了）。
2. **定向事件 target-kind 对齐**：给单个 viewer 窗口定向投递（如 `replay_session_to_window`）用 Rust `emit_to(EventTarget::webview_window(label))` ↔ 前端 `getCurrentWebviewWindow().listen`（`bindEvents({windowScoped:true})`）。**禁止**用 `&str` 目标（→`EventTarget::AnyLabel`）配模块级 `listen`（→`Any`）—— Tauri 2 按 kind 匹配，`Any` 监听命不中 `AnyLabel` 发射，事件静默丢弃。`AppHandle::emit` 广播（`Any`）是通配，带标签监听仍收得到 live 增量。
3. **`bindEvents` 必须 await 再触发 emit**：`listen()` 异步注册，注册完成前后端 emit 的事件会丢。viewer 在 bindEvents 后立刻调 replay，所以 `bindEvents` 返回 Promise 且 caller 必须 await（主窗口 emit frontend-ready 同理）。
4. **viewer-mode CSS grid 行数随可见 item 定义**：`#tab-bar` `display:none` 会把它**从 grid item 移除**，剩余子元素自动前移一行。所以 viewer 必须只为剩余 item 定义对应行数（`auto 1fr 24px`），否则 message-stream 落进多余行被压成 0 高（整窗只剩状态栏）。

**为什么不能松动**：四条都是实测踩出来的，且都"静默失败"——不报错，只是白屏 / 收不到 / 卡死，极难凭看代码发现。

---

## 23. 复用 `.stream` 容器的非-Tab 视图必须显式恢复可见

基类 `.stream`（[`src/styles.css`](../src/styles.css)）默认 `visibility: hidden` + `pointer-events: none`，仅 `.stream.active` 才可见——这是**多 Tab 机制**：N 个 tab 各持一个 `.stream`，TabManager 只给当前 tab 加 `.active`。

任何**复用 `.stream` 样式但不归 TabManager 管的视图**（`SessionViewer` 应用内只读查看器、未来别的只读流）**必须在自己的 CSS 里显式 `visibility: visible`**，因为它们的元素永远不会拿到 `.active` 类。

**为什么不能松动**：v2.8.1 的"历史会话点进去空白"就是这个坑——`SessionViewer` 流元素 class 是 `stream session-viewer-stream` 没有 `.active`，命中基类 `visibility: hidden`，2000+ 张卡片全渲染进 DOM 却不可见，而状态栏（不在 `.stream` 内）照常显示记录数 → "有记录却空白"的迷惑现象。独立 viewer 窗口（§ 22）复用 TabManager 所以有 `.active`，不受影响；只有自建流的 `SessionViewer` 中招。**禁止**给非-Tab 视图的流元素只复用 `.stream` 而不补 `visibility: visible`。

---

## 24. 远端活跃集 `remote_active` 恒等于"前端当前应视为 live 的远端 sid"（issue #20）

`lib.rs` 的 `remote_active`（`Arc<Mutex<HashSet<String>>>`）**唯一写者**是 `remote-session-emitter` 线程：daemon 的 session added/removed 与断连 flush 都经同一 `remote_tx` 通道到达，且**先维护集合、再做 emit 等副作用**。`frontend-ready` 对账用"sid 在 EventReplay buffer 里、但不在集合里"判死、补发 `session-ended`。

两条派生约束：

1. **任何让远端行进入 EventReplay buffer 的路径，其 session-added 必须先于（或同批于）行到达该通道**——目前由 daemon 协议保证（added 帧先于该会话的行帧）。绕过集合注入远端行（多 host 扩展、远端历史 #16、测试灌数据）会让 F5 把活会话误归档。
2. **前端必须把 `session-ended` 与行事件同序处理**（`events.ts` 的 queue，#20 一并改）。ended 若抢在积压重放行之前执行，归档会被后续远端行的 un-archive（`tabs.ts` ensureTab，仅远端）翻回 live，对账等于无效——这正是 #20 初版后端-only 方案被审计打回的原因。

**为什么不能松动**：对账是把"一次性 ended 信号"在重载后重建出来的唯一机制；集合不准 = 要么僵尸 live Tab 复现（漏归档），要么活会话被误杀且无后续行救活（误归档）。断连窗口期的误归档是**有意取舍**（重连后 daemon 重发 added + 重放行 → un-archive 自愈）。

---

## 25. 行事件投递是 at-least-once —— 按 uuid 累积状态的前端模块必须自行幂等（issue #25）

后端到前端的 jsonl 行投递**不保证 exactly-once**。已知重投路径：

- **本地 watcher 截断重读**（`watcher.rs::process_file`：`len < cursor.seen_len → start=0`）：整个文件**换新 seq** 重投（seq 不重置，见 § 5）；触发时有 `jsonl truncated` warn 留痕。**已知静默缺口**：截断到空（len==0）那一轮刻意不喊（无重读发生），文件随后重新长出内容的全量重投也无 warn——排查重投时「日志无 truncated」不能排除此路径；
- **远端 daemon 截断重读**（`remote-daemon-proto watcher.rs::read_new_lines`，Batch4-F14 起与本地同判定、同 warn）：同样换新 seq 重投整个文件；
- **远端 daemon 重连重放**（issue #17）：从 seq 0 重发整个活跃会话（**通常同 seq**——daemon SeqCounter 是进程内存态，重连即新进程从 0 起编号。**已知缺口**：若断连前发生过远端截断重读，旧 seq 已爬过文件行数，重连后前端 `tab.seenSeqs`（重连不清空）会把断连期间新增行的 seq 误判为已见而拒渲染，直到 seq 超过旧高水位——三条件叠加的低概率场景，daemon 转正前需修：重连时按 origin 清 seenSeqs 或 uuid 去重前置，见 Batch4 Phase G 验收记录）。

Batch4-F14 起两端只消费以 `\n` 结尾的**完整行**（torn tail 延迟到补全，offset 按实际消费推进，截断判定用 seen_len 高水位）——"读中文件增长导致 offset 回退换 seq 重投"这条历史路径已消除。**已接受的取舍**：①最后一行是完整 JSON 但永远等不到尾 `\n`（写端在两次 write 之间被 kill 且文件从此不再增长）→ 该行永不投递、无日志；实测 Claude Code 每条记录以 `\n` 收尾（2026-07-03 抽查 8/8），此情形视为非标准写端。②长度基截断检测的固有盲区：两次事件之间文件先长到 X > seen_len 再被重写为 Y ∈ [seen_len, X) → 任何仅凭长度的方案都检不出（pre-F14 同样检不出，非回归）；需内容指纹才能封死，成本不值。

`tab.seenSeqs`（#17）只挡**同 seq** 重投；换新 seq 的重投在 seq 层不可见。因此：**任何按 uuid（记录身份）累积状态或构建拓扑的前端模块，必须对"同一记录再来一遍"幂等**——入口按 uuid 拒重（保首见），不得假设上游只投一次。现有履约点：`tabs.ts onLine` 的 `processedUuids`（入口整体拒重——渲染与 trackAgents 等副作用一并挡住；无 uuid 的元信息记录放行，issue #26）+ `computeMainBranch` 入口去重 + `BranchFolder.seenUuids`（issue #25）三层。新增"消费行事件"的模块（如 viewer 新路径、#16 远端历史）必须同样履约。

**为什么不能松动**：实测 1 条重复 attachment 即把 1541/4331 条主线误折成「已被 ESC 回退」、全文件重投折掉 4137/4331 且首条 user root 出局（issue #25 两次实锤）。重复记录毒化 Kahn 拓扑的 remaining 计数 → 重复点全部祖先落 leftover fallback（latestDescTs/hasAssistant 全错）→ 被 fork 赢家 / 多 root 分类（#22）放大成整段历史折叠。且重复常是 attachment/isMeta 等**不渲染 DOM 的记录**——肉眼不可见、每次重算复现、进了 event_replay buffer 后 F5 带毒，不自愈。

---

## 25a. 远端 seq 是行号空间（Batch8 起）——重连碰撞在无截断前提下源头已除

Batch8-F25/26 起（p1f daemon + tail-only）：daemon 连接时把各文件 seq 计数器
初始化为**当前完整行数**、新行 seq=行号；monitor 旁路快照按 0..L'-1 行号编 seq。
推论：**无截断前提下**重连后新行 seq ≥ 断连前高水位（文件只增长）——§25 留档的
"重连 seq 从 0 重数 → seenSeqs 碰撞吞行"的**常规形态**源头消失（旧 daemon 全量
推流路径仍存在，随 daemon 升级自然消亡）。**截断残余**（审计 D 收窄措辞）：
断连期间 /clear 截断 → 新 daemon prime 到 L_new < 旧高水位；或上一连接内截断
重读把计数器抬超实际行数——两者仍可短暂吞行，靠快照重拉 + uuid 幂等（§25）
兜底，属 §25 三条件缺口的收窄而非消除。快照重拉（每次连接重建队列）整体幂等：
重复 (sid,seq) 行被去重吸收，代价只是带宽（增量协商留 backlog）。

## 26. bg 会话门是数据层配置门；daemon 流模式 flag 必须先于查询模式判定剥离（Batch7-F24）

`kind:"bg"` 的取舍史：F21 一刀切不算会话 → 用户实测"工作跑在 bg 里但 tab 停住"（可观测性洞）→ F24 反转为**标注而非过滤**。三条子规则：

- **kind 缺失恒视为交互**（旧 CC 兼容），双端一字一致。
- **bg 门在数据层生效**（本地 scan_dir 过滤 / 远端 daemon `--with-bg` 参数），不是前端隐藏——关掉 = bg 数据完全不流（省带宽与 buffer，bg 历史可达 10MB+）。开（默认）= bg 建 Tab 带 ⚙ + 树状挂同 (cwd, origin) 交互宿主后。
- **daemon 任何新增流模式 flag 必须在一次性查询模式判定之前从 args 剥离**（`main.rs` 先 `retain` 再判 `!args.is_empty()`）——否则 flag 落进 query 分支，daemon 打印查询结果退出，monitor 无 hello 死循环。同理，**monitor 只对确认 ≥ 该 flag 版本的 daemon 传新 flag**（auto-deploy build_id 确认；确认不了就降级不传）。既有实例：`--with-bg`（F24）、`--tail-only`（Batch8-F25）。

## 27. 远端会话生命周期信号的两条载荷型约束（Batch9）

- **F5 重发必须先于 replay**：`remote-session-added` 不进 replay buffer，F5 后远端
  骨架/bg 元数据/初始灯全靠 frontend-ready 时 `ssh_source::reannounce_all` 重发；
  "宣告先于该会话的行"契约在 F5 路径的唯一保证是 lib.rs frontend-ready 处理器里
  reannounce 调用**先于** `replay_and_mark_ready` 的顺序（同一 task 内顺序 emit +
  前端同 queue FIFO）。改动该顺序 = 破坏骨架先行契约。
- **status 缺失恒为"未知"**：pidfile 无 `status`（旧 CC）→ 帧不带 → 前端 `act=null`
  不加灯类——双端一字一致（与 §26 "kind 缺失恒视为交互"同族的保守缺省规则）。

## 28. 自造持久身份的护栏（F64 / issue #58 单向门①）

**铁律**：任何会被**持久化（落盘）或跨进程 / 上 wire 协议暴露**的身份标识，必须
**opaque + 稳定 + 出生一次 + 永不从名字 / 路径 / 位置算**。**想不清就先别发**——用外部
已有的稳定 id 顶着（Claude Code 的 `sessionId`、或 code-picture 的 uuid）。

**为什么是单向门**：id 一旦被别处引用或落盘就锁死，事后没法把「会变的 key」换成
「稳定 id」而不断掉所有引用。反例是 Claude Code 自己的 `enc(cwd)`——拿位置相关的
可读路径当 key，用户一搬目录，历史全对不上。**往模型里加字段永远便宜；把当 key 用
错的 id 改对极贵**（要迁数据 / 破引用 / 两个前端各改一遍）。这类错误漏到代码外面，
`serde` 宽容（§18）救不了。

**判据**（照 issue #58）：只有自己代码调、不落盘、不跨 aterm、不进 wire → 可逆，晚点抽。
占了其中任何反面 → 单向门，现在就得把「形状」定对。

### 现状签收（2026-07-16，F64 全库核查，无违反）
cc-monitor **没有一个**「自铸 opaque id + 落盘/上 wire + 从路径算」的东西。持久身份
全挂**外部稳定 id**：
- 会话表 / 历史 metadata / 窗口句柄缓存 key = Claude Code `sessionId`（`session_map.rs`、`history.rs::HistoryMetadata.entries`、`bind.rs::SidHwndBinding`）。
- ps-registry key = OS `pid`（`bind.rs`）。
- 唯一自铸的 opaque token = bind 握手 marker `ccm-bind-{PID}-{随机8字符UUID}`（`bind.rs`）——**瞬时握手、用完即删、不从路径算**，不当持久身份，合规。
- panorama 进程内选 Engine 的 key 用仓根路径，但**纯内存、绝不落盘**（保持现状，别存盘）。持久的节点身份由 **code-picture-core** 写进侧车 DB、守它自己的 uuid 规矩，cc-monitor 只消费不自铸。

### `origin` 边界（有意的外部稳定 id，别手滑）
`RemoteConfig.label`（`ssh_source.rs`，空则回退 `host`）是唯一「持久（config.json）+
上 wire（每条远端行带 `origin`）+ 名字派生」的 key，形态上最接近 `enc(cwd)` 反模式。
但它**合规**——它是**用户可控的外部稳定 id**（主机名或用户填的 label），正是本约钦定的
兜底手段「用外部已有稳定 id 顶着」，且**只做结构字段 + 内存 map 的 key，不是任何落盘
登记表的主键**（history-metadata 主键是 `sessionId`）。**守则**：别哪天把它换成从 IP /
路径现算的脆弱值，也别把它降格当 cc-monitor 内部的 opaque id。

### 对齐 code-picture（要持久身份就复用，别自造）
cc-monitor 一旦需要**持久化**「哪个仓 / 哪个节点」，直接复用 code-picture 的 uuid
（code-picture `decisions.md` **D3** crypto-RNG uuid、**D18** 存中央 journal、**D26**
惰性激活才发号），**绝不另发一套从路径算的持久 key**。

### 未来约束（F70 / F90 落地那天守本约）
- **F70**（#51 点会话高亮改动）：当「某会话改过哪些节点」要**跨会话留存/引用**时，节点
  身份必须用 code-picture uuid，**不许每次从 `file_path` 现算**（否则一改路径高亮全丢）。
  现状 F70 缝即时返回、不落盘，仍在安全侧。
- **F90**（#48 daemon 登记表）：会话/后端登记表主键必须 opaque + 稳定，用 Claude Code
  `sessionId` 顶着，**不许拿 tmux 会话名 / 主机名 / 路径当持久主键**——否则 §SS-12
  「一端起的会话另一端必须能接」当场崩（换后端 / 换机名字变了就对不上）。

## 修改本文档

加新的不变量时：

1. 加到本文档对应位置 + 编号
2. 在 `src/` 或 `src-tauri/` 对应模块的 doc comment 里加引用 `// 违反此约束见 doc/INVARIANTS.md § N`
3. 如果不变量需要 grep checklist（如 State 注册），加到 [CONTRIBUTING.md](CONTRIBUTING.md) 对应 checklist

删除某条不变量（极少）：

1. 写 RFC 解释**新的约束**是什么、为什么旧的可以松动
2. PR 描述里链到这条 RFC + 全代码库 grep 受影响处确认全修
