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
4. **profile 写（F10，本机 `profile_installer` cc 集成 + 远端 `sftp::install_remote_ccm_helper`/`uninstall_remote_ccm_helper`）**：写用户自己的 shell profile（本机 `~/.bashrc` / **远端 `~/.bashrc`** via SFTP）装/卸 cc(m) 助手——用户显式触发、BEGIN/END 块 + 备份 + 写后校验回滚。**注（batch20 审计修）**：F10 **含远端 `~/.bashrc` 写**（原 `sftp.rs` 头「非远端」措辞已订正）。
5. **MCP 项目配置写（F87 本机 / F89a 远端，`mcp::write_project_mcp_server` / `write_remote_mcp_server` / `remove_*`）**：用户**显式**点「添加/更新」或「删除」（删带二次确认）后，写**项目** `.mcp.json`（本机 `mcp_json_path` 硬编码 / 远端 `is_safe_remote_mcp_json` 守卫：绝对 + 尾 `/.mcp.json` + 无 `..` + 非裸；远端经 `sftp::upload_atomic` 原子 tmp+备份+rename）。**SS-14**：写面**只** `.mcp.json`，**绝不**写 `~/.claude.json`/settings.json。`.mcp.json` 是**用户项目配置**（决定项目用哪些 MCP server），**非** Claude 会话数据（jsonl/pidfile）——与本约正交（同 F47 SFTP 面板性质），非驱动运行中会话。
6. **公钥推送（F50，`pubkey::push_public_key`）**：用户显式点推送后，经 SSH-exec 把本地公钥**追加**到远端 `~/.ssh/authorized_keys`（`build_authorized_keys_cmd`：key 经 `shell_quote` 防注入、`grep -qxF` 幂等去重、只 append 不删）——用户自己的免密配置、非 Claude 数据。

**为什么不能松动**：cc-monitor 的核心价值主张是 "看 claude 的输出不破坏它"。一旦允许 monitor 在用户数据上**非显式**写，用户对 "数据源 = 我自己的命令痕迹" 的信任就崩了。上述豁免要么是**非用户数据**（自部署 bin），要么是**用户显式动作**（删除 / metadata），且各带独立 realpath 白名单。

**F47 SFTP 文件面板不在本约管辖内（澄清，非例外/非松动）**：Batch14-F47 起 cc-monitor 挂了一个**用户亲自驱动的通用 SFTP 文件传输面板**（浏览/上传/下载/改名/删除任意用户文件）。它是**独立文件传输功能**，与本约「monitor 作为监视器只读 Claude 数据源」**正交**——它写的是用户浏览到的普通文件，不是 Claude 的 jsonl/pidfile，且每次写都是面板内一次直接用户手势（绝无自动/后台写）。**防误伤守卫**（`sftp_pool::is_protected_claude_data_path`）:SFTP 写命令**拒碰** `~/.claude/projects/**/*.jsonl` 与 `~/.claude/sessions/*.json`（往正被 Claude 打开的会话文件写会损坏会话；要管这些用历史浏览器）。SFTP 面板走独立 utility 连接池，与数据源流连接分离。

**A2 多账号只读查询是本约的「读」面延伸（澄清，非例外/非松动）**：`remote-daemon-proto/src/accounts_query.rs` + `src-tauri/src/accounts.rs` 为「按会话切账号」新增三条**纯只读**远端查询（`--list-accounts` / `--session-accounts` / `--account-trust`），**零写入**、**不 shell out**。它把 daemon 的读面从 `<claude_dir>` 扩到三处新位置，各自有硬边界:

1. **`$ACCTS_DIR/accounts.json`**（cc-acct-iso 的 manifest，契约 v1）—— 只读整份 JSON;`configDir` 视为**不可信字符串**，逐条过 shell-safe 白名单（与 cc-acct-iso 的 `path_shell_safe` 同一套字符集），不合格的账号直接丢弃。
2. **`/proc/<pid>/environ`** —— **只抠 `CLAUDE_CONFIG_DIR` 一个键**，绝不回传整个环境快照（那里面有用户全部的密钥类环境变量）。pid 来自 `<claude_dir>/sessions/<PID>.json` 的文件名。
3. **`<configDir>/.claude.json`** —— **只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔**，绝不回传文件内容（内含 `mcpServers` 的环境变量，可能有 API key）。且 `configDir` **必须逐字等于 manifest 里某个账号的 configDir**，否则拒绝——否则 `--account-trust` 就退化成任意文件读原语。

**`.credentials.json` 只 stat 存在性、永不读内容**（`loggedIn` 字段就是这么来的）。**动凭据的部署操作（`cc-acct-iso … --apply`）绝不经 daemon**——那会往只读组件里塞写权限;一律由 cc-monitor 拼好命令后弹一个**用户可见的终端窗口**执行（`launch_remote_terminal`，同时也是 `/login` 必须走 TTY 的唯一出路）。见 `.claude/planned-build/account-isolation/DESIGN-account-switching.md` §6。

**`sessions/` 必须留在 cc-acct-iso 的共享集**：daemon 靠 `<claude_dir>/sessions/<PID>.json` 判活并拿 pid，进而探测账号。若哪天把 `sessions/` 挪进隔离集，各账号的 pidfile 会散到各自 config-dir，cc-monitor 会看不见非默认账号的会话。

**F62 从历史某轮建分支不在本约管辖内（澄清，非例外/非松动，用户 2026-07-12 拍板）**：`history::create_branch_session` 在用户**显式**点历史查看器里某条消息的 `⑂` 时，把 `[根…该消息]` 前缀**复制**成一个**全新** `<new-sid>.jsonl`（原生 `/branch` 的 `forkedFrom` 格式）。这与本约**正交**——本约防的是 monitor **改坏/覆盖/后台写**它正在监视的**现存**会话文件；建分支是**纯新增产出**（用户框定："复制产出一个文件，而非侵入式改动"），**原会话一字节不改**，且只写**新生成、collision-check 过的 sid**（`out_path.exists()` 则拒，绝不覆盖任何现存会话）。防越界守卫 `validate_branch_source`（canonicalize + `starts_with(projects)` + `.jsonl`）与 delete 同构。破坏性上它比已放行的「显式删除」更弱（只增不减）。

**A5 tmux 会话名契约是跨语言隐性耦合，改一端必须同步另一端**：本工具建的远端 tmux 会话名恒为 `cc-<sid8>[-N]`（前端 `deriveTmuxName`/`pickFreshTmuxName` at `src/remote-launch.ts` 生成）。Rust 侧 `tmux::is_ccm_tmux_name`（`src-tauri/src/tmux.rs`）用 `cc-` 前缀 + `[A-Za-z0-9_-]` 白名单**门控 `tmux_send_keys`**（A5 换号重启在旧号 send `/compact`），**绝不向用户自己的其它 tmux 会话发按键**。两端各写一份该契约、仅靠测试对齐（跨语言无法共享函数）。**若改了前端的 tmux 名前缀/字符集，必须同步 Rust 白名单**，否则 send-keys 会被静默拒绝、compact 悄悄失效（不阻断重启，但优化白丢）。注：`kill_remote_tmux`（F79）沿用既有行为**无此白名单**，故 A5 破坏性重启在 `restartTabWithAccount` 里用 `live.sid === sid` 精确守卫兜底——只精确命中 `@ccm_sid` 才 kill，绝不按 cwd 回退猜（防杀错会话 + 双进程）。**A5+**：`tmux_send_keys` 加了可选形参 `enter`（`Option<bool>`，**缺省 true**）——`enter=false` 时命令省去尾 `Enter`（优雅退出发 `Escape` 打断当前回合时用，防误提交输入框队列文本），`/compact`、`/exit` 等仍附回车。前端旧调用不传 `enter` → 逐字节等价旧行为，向后兼容。

---

## 2. monitor 自己的 data dir 永远是 `~/.claude/claudecode-frontend/`

不跟随用户在 UI 改的 `claudeDir` 漂移。

**为什么不能松动**：
- 避免循环依赖：读 config 不能先解析 claudeDir，否则用户填错路径就再也打不开设置面板。
- 用户切换 Claude 数据目录后主题 / 字体偏好不丢。
- profile backup / sid-hwnd-cache / ps-await 等跨进程文件位置稳定，PS 端不需要动态查询。

### 2.1 真相 vs 缓存必须分得清（F65 / issue #58 单向门④）

data dir 里两类东西**语义上一刀两断**，别搅混到「迁移/重建时不敢下手」：

| 文件 | 类 | 写它的 | 说明 |
|---|---|---|---|
| `config.json` | **真相** | `config.rs` | theme/font/claudeDir/keybindings/`remote.hosts[]`(含 label)/resume 命令/诊断开关——全用户手填 |
| `history-metadata.json` | **真相** | `history.rs::save_metadata` | 按 sid 的 star/重命名/隐藏——用户策展意图 |
| `auto-launch.json` | **混（良性）** | `auto_launch.rs` | `enabled`=真相；`monitor_exe_path`=派生(每次启动 `current_exe()` 自愈改写) |
| `sid-hwnd-cache.json` | **缓存** | `bind.rs` | sid→HWND，能从 PS 握手重建 |
| `ps-registry/` `ps-await/` | **缓存/IPC** | `bind.rs` | 跨进程握手，启动重扫 |
| `logs/` | **缓存/派生** | `logging.rs` | 诊断日志，滚动保留 3 天（§15） |

- **真相** = 用户手写/意图，**删了丢东西、要备份、要迁移友好**。
- **缓存/派生** = 能从别处重建，**随便删**。
- **规矩**：**新增任何 data dir 文件，必须在 `data_paths.rs` 的枚举里声明它是哪类**（那里是逐个 data dir 文件的唯一权威枚举点，带 description）。truth 的格式要迁移友好；cache 允许随手删。
- **两笔边界别误读**：① `auto-launch.json` 同文件混真相+派生，是**良性**的（派生位自愈，整体迁移不坏）；② `ps-registry/`/`ps-await/`/`logs/` 在子目录，那是**按用途/IPC 对端分**的，**不是按真相/缓存分**——`sid-hwnd-cache.json` 这个纯缓存反而在根、跟 `config.json` 平级。
- **机器强制形态（推荐，F90 落地）**：给 `data_paths.rs::DataPathInfo` 加一个**非可选** `class: truth|cache` 枚举字段，让「新文件必须选类」由类型系统兜住（强过散文规矩），并可在设置面板「数据」区显示。**现在不做**——F90 加 daemon 登记表时正在动 data dir 结构，那时顺手加最自然；提前做是 speculative。

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

## 22. 独立窗口契约（viewer #10 / settings F82a）

独立窗口（`viewer-<sid>` `bootstrapViewer` / `settings` `bootstrapSettings`）依赖下列契约，违反任一条都会让窗口白屏 / 卡死 / 收不到数据 / 关不掉——**全是"静默失败"**（不报错，只是白屏 / 收不到 / 卡死 / 点了没反应），极难凭看代码发现，都是实测踩出来的。**任何新独立窗口照抄这套脚手架，逐条对照适用性。**

1. **开窗 IPC 必须 `async`**（viewer + settings 都适用）：`open_session_in_new_window` / `open_settings_window` 等创建 `WebviewWindow` 的命令必须是 `async fn`。Tauri 2 同步 `fn` 命令在**主线程**执行，而 `WebviewWindowBuilder::build()` 内部把创建派发到主线程并阻塞等 → 在主线程等主线程 = 死锁（新窗口白屏 + 整个 app 卡死连关闭都点不了）。
2. **定向事件 target-kind 对齐**（viewer 适用；settings **N/A**——无会话流、跨窗同步用广播）：给单个 viewer 窗口定向投递（如 `replay_session_to_window`）用 Rust `emit_to(EventTarget::webview_window(label))` ↔ 前端 `getCurrentWebviewWindow().listen`（`bindEvents({windowScoped:true})`）。**禁止**用 `&str` 目标（→`EventTarget::AnyLabel`）配模块级 `listen`（→`Any`）—— Tauri 2 按 kind 匹配，`Any` 监听命不中 `AnyLabel` 发射，事件静默丢弃。**广播**（前端 `emit()` / Rust `AppHandle::emit`，`Any`）是通配，模块级 `listen`（`Any`）收得到——settings 的 `settings-applied` 跨窗同步正走广播↔模块级 listen（Any↔Any），恰好避开该坑。
3. **异步 `listen`/`bindEvents` 必须先注册再触发 emit**（viewer 适用；settings 因 emit 只在用户开窗后保存才发生、远晚于主窗口启动注册，无竞态）：`listen()` 异步注册，注册完成前后端 emit 的事件会丢。
4. **精简模式 CSS 不能塌 grid 行**（viewer + settings 都适用，解法不同）：`display:none` 一个 grid **item**（如 viewer 的 `#tab-bar`）会把它从 grid 移除、剩余 item 前移落行 → viewer 必须只为剩余 item 定义对应行数（`auto 1fr 24px`）。settings 换了个更稳的解法：`body.settings-window-mode` 直接 `display:none` 隐藏 grid **容器** `#app` 整块（非其内 item，无前移塌缩），面板 `position:fixed` 脱流铺满。
5. **关窗要 `core:window:allow-close` 能力**（settings 适用；任何前端调 `getCurrentWindow().close()` 的窗口都适用）：该 JS API 走 `plugin:window|close`，受 ACL 门控，而 `core:window:default` **只含 getter 类权限、不含 `allow-close`**（同理 minimize/set-fullscreen 也得显式加）。capability 的 `windows` 列了该窗口标签还不够，**必须**把 `core:window:allow-close` 加进 `permissions`，否则 ×/取消/Esc 关窗被 ACL 拒、`void` 吞掉 → 点了没反应（系统标题栏原生 X 仍可关，更隐蔽）。
6. **复用 `dispatcher` 的独立窗口必须自调 `dispatcher.start()` + `applyOverrides`**（settings 适用；任何含 overlay / 快捷键录制的独立窗口都适用）：设置窗有**自己的** dispatcher 实例；`dispatcher.start()` 是唯一挂 window keydown 的地方（快捷键录制的按键捕获 + Esc 经 overlay LIFO 逐层关都在其中）。不调 → 窗内快捷键编辑器录制收不到键、Esc 无法关嵌套 overlay。**别手搓 window 级 Esc 监听**——它会与栈内 overlay 的 Esc 双触发（既关 overlay 又关整窗）。让面板作 overlay 栈底（`pushOverlay`），其 `handleEsc`→关窗。

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

**F74c(#60-A) 补充——tmux 存活对账是 `remote_tx` 的第三生产者**：tmux 存活收割器（带外杀 tmux 后端 → 变灰）与 daemon 帧、断连 flush **并列**为 `remote_tx` 的生产者，**必须**把 retire 的 sid 当 `SessionChange{removed}` 经该通道下发，**绝不**直接写 `remote_active`、**绝不**让前端直接 archive——唯一写者仍是 `remote-session-emitter`。误判防线（`ever_bound` 门 + debounce + 漂移靠 announced_live 剔除 + 空 backend/NO_TMUX 跳过）在 `tmux_reconcile::reconcile_step`（纯函数、source-agnostic），阈值真机标定。**后人给收割器接线时不许把它直连前端或直写集合。**

> **audit-fixes F03.2 更新——收割器已从 8s poller 改为「收帧驱动」**：原 `tmux_reconcile.rs::run_tmux_reconcile_poller`（8s 定时轮询 + `snapshot_announced_by_origin`）**已删**——cc-monitor 侧零轮询（唯一周期=daemon 内部 tmux ls）。收割逻辑现内联在 `ssh_source.rs::stream_loop` 的 `InboundFrame::TmuxSessions` 帧臂：daemon 每 ~8s 推该 origin 的 `tmux ls` 原文即对账一次，`tracked = 本连接 announced（keys=live sids）∪ 本 origin idle 会话`，调 `reconcile_step` 去抖 retire → 送 `removed` 给 emitter。`reconcile_step` 与 `ReconcileState` 保留为纯决策（F90 可 lift）。

## 24bis. 远端 idle-tmux 灰灯（audit-fixes F03.2）：`REMOTE_IDLE` 与 `remote_active` 正交、同一 emitter 单写

远端会话三态：**live**（claude 在跑）/ **idle-tmux**（claude 退出但 tmux 会话仍在 → 灰灯、可 attach 复用）/ **archived**（tmux 也没了）。承接 §24 单写者不变量，落地约束：

1. **`REMOTE_IDLE` 是独立账本，唯一写者仍是 emitter**：`ssh_source.rs` 的 `REMOTE_IDLE`（origin → idle sid 集）与 `remote_active` **正交**——`mark_idle`/`clear_idle` 只由 `remote-session-emitter` 调（`run()` 的 removed/added 臂），其余路径（收割器、F5 对账）只 `snapshot_idle_*` 读。**绝不**给 `SessionChange` 加字段承载 idle（收割器仍只发 `{removed}`）。
2. **emitter removed 臂据 tmux 存活分流**（`classify_removed` 纯函数 + `find_tmux_origin_for_sid`）：`Some(origin)`=tmux 会话尚在 → `mark_idle` + emit `SESSION_IDLE` + **不 forget**（不进归档、`remote_active` 早已在上方移出该 sid，idle 天然在集合外，**不新增 `remote_active` 写点**）；`None`=tmux 也没了 → `clear_idle` + `forget` + emit `SESSION_ENDED`（原归档路径）。判据 **command-agnostic**：`TmuxSessions` 帧最长 8s 陈旧，退出瞬间 command 列可能仍是 claude，故「claude 死」由 daemon-removed 边沿判、「tmux 在」由 `@ccm_sid` present 判（见 `tmux_origin_for_sid`）。
3. **idle→archived 的产出者 = 收帧收割器**：idle sid 并入收割器 `tracked`；且因 `@ccm_sid` 铁证其绑过 tmux，作 `reconcile_step` 的 `pre_bound` 直接播种 `ever_bound`——否则「SessionRemoved 删 announced」与「emitter mark_idle」之间的跨线程缝里那帧会漏置 `ever_bound`，令 idle sid 永不累计缺失 = 连接内卡灰关不掉。tmux 真消失 → 收割器去抖后 retire → emitter 走 `None` 归档。
4. **前端灰灯与 §24 第 2 条同源**：`session-idle` 与 `session-ended` 同进 `events.ts` 的 queue（对同一 sid 二者互斥、emitter 择一）。`Tab.tmuxIdle` 与 `TabStatus` **正交**（status 仍 live、仅灯变灰，不碰任何 archived 门控）。清灰**主**信号 = `ensureTab`（远端 tab 又收 daemon 重宣告/行 = claude 复活，queue 内与行保序）；`session-activity` 为次要（非 queue、null-activity daemon 下不可靠，**不可**作唯一清灰路径）。

**单写者已机器化**（Phase G）：第 1 条「`mark_idle`/`clear_idle` 唯一写者=emitter」原靠注释约定、`cargo check` 抓不住；现有 `ssh_source.rs::f032_idle_tests::remote_idle_single_writer_guard` 扫源码断言这两个写函数**只被 lib.rs 调用**，emitter 之外新增写者即测红（同 F08 daemon 只读护栏的机器化思路）。

**已知残留（daemon-bound，记档待版本批次）**：收帧收割器对**空 backend 保守跳过**（`ssh_source.rs` `!backend.is_empty()` 门），是为挡 `tmux ls` 瞬时抖动批量误灰。代价：当**被杀的是该 origin 最后一个 tmux 会话**时，tmux server 退出→daemon `run_tmux_ls` 回空串→收割器整段跳过→该 idle-tmux 灰灯**卡到断连 flush 才清**（多会话场景不中招；断连自愈）。干净修法=daemon 对「命令成功但零会话」回确定性哨兵（如 `NO_SESSIONS`）区分于「exec 失败」，monitor 即可安全 retire——**但这要动 daemon（红线：daemon 零行为改动），留 daemon 版本批次**。

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
- **daemon 任何新增流模式 flag 必须在一次性查询模式判定之前从 args 剥离**（`main.rs::split_stream_flags` 先 `retain` 再判 `!args.is_empty()`）——否则 flag 落进 query 分支，daemon 打印查询结果退出，monitor 无 hello 死循环。既有实例：`--with-bg`（F24）、`--tail-only`（Batch8-F25）。
- **monitor 只对「声明了对应能力」的 daemon 发该 flag**（F66/#58③ 起，**取代**原来的「build_id 精确匹配」门控）：daemon 在 hello 帧自报 `capabilities` token 集，monitor 按声明发 flag。**护栏靠「声明 ⟹ 会剥离」成立**——只有会先 `split_stream_flags` 剥离某 flag 的 daemon 才声明对应能力（老到不剥离未知 flag 的 daemon 也老到不声明）。这条约定**由 `every_capability_token_is_strippable` 测试代码强制**（daemon 侧）：`CAPABILITIES` 每个 token 的 flag 必须被 `split_stream_flags` 剥离，否则测试红。**加新能力 token = 同时加剥离分支**，不然埋死循环。
  - **能力 ≠ 身份（两轴正交，呼应 §28）**：`build_id`（身份，SS-B 单源）管 staleness / 重部署提示；`capabilities`（能力，加法式）管发什么 flag；`v`（proto version）只留破坏性变更（F66 **绝不 bump**）。**2026-07-09 事故的根因正是把「能干什么」错编码成「是不是那个精确构建」**——身份链一环断（发布流水线漏拷清单）就全能力静默关。F66 拆开三者：能力由 daemon 自报，即使身份确认不了也照开。**新原则：绝不用身份匹配代理能力声明。**

## 27. 远端会话生命周期信号的两条载荷型约束（Batch9）

- **F5 重发必须先于 replay**：`remote-session-added` 不进 replay buffer，F5 后远端
  骨架/bg 元数据/初始灯全靠 frontend-ready 时 `ssh_source::reannounce_all` 重发；
  "宣告先于该会话的行"契约在 F5 路径的唯一保证是 lib.rs frontend-ready 处理器里
  reannounce 调用**先于** `replay_and_mark_ready` 的顺序（同一 task 内顺序 emit +
  前端同 queue FIFO）。改动该顺序 = 破坏骨架先行契约。
- **status 缺失恒为"未知"**：pidfile 无 `status`（旧 CC）→ 帧不带 → 前端 `act=null`
  不加灯类——双端一字一致（与 §26 "kind 缺失恒视为交互"同族的保守缺省规则）。
- **capabilities 缺失恒为空集（最小能力集）**（F66/#58③）：hello 无 `capabilities` 字段
  （旧 daemon）→ monitor 解析为**空 Vec** → 不发任何流模式 flag（保守降级 = 2.18.0 行为，
  连接正常）。同上两条的保守缺省族——「不认识就按最保守待它」，绝不静默假设有能力。

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

## 29. 会话生命周期解析规则以权威规格为准，两端不许各自发明（F67 / issue #58 单向门⑤）

Claude Code 的 JSONL 会话生命周期解析规则（未知记录抢救、ESC 回退主线检测、end_turn
判定、queue 折叠豁免、api_error 双形态、attachment/isMeta 链、pidfile 保守缺省、PID
复用防护）有**唯一权威规格**：`code-picture/doc/agents/claude-code.md` §9「会话生命周期
解析规则」。

**为什么是单向门**：cc-monitor（Rust）与 aterm（Kotlin）各自解析同一批 JSONL，各自推导
略不同的规则 → **语义漂移**。重复本身可逆（阶段② 抽进 daemon），但漂移贵——「抽成一个」
就变「先调和两套行为、再回归测两个前端」，而非「lift 一个」。

**铁律**：
- **新增或改动任何会话生命周期解析规则前，先对权威规格 §9**；规格没写的先补规格再实现。
- **发现两端漂移 → 对回权威规格修正，绝不各自发明不同规则。**
- 每端**实现可以滞后**（渲染丰富度等细节），但**核心生命周期判据不许换**（如 end_turn
  必须 `stop_reason=="end_turn"`、不许「无 tool_use 近似」）。
- **只对齐行为，不对齐本地信封类型名**：抢救记录的类型名是各端本地产物
  （cc-monitor 的 `cc-monitor-unrecognized`），不参与两端类型对账（§SS-16）。

**当前对账快照（F67，2026-07-16）**：主线算法 / end_turn / attachment 链**一致**；F63
未知记录抢救、queue 折叠豁免、主线白名单第五类在 aterm 侧**仍漂移**（cc-monitor 已修、
aterm 承诺未落地）；kind 门 / status 红绿灯 / #43 kind 冲突消解 aterm **无对应**（#43 转
F74）。清单见权威规格 §9 + MASTERPLAN 轨道二护栏五。

## 30. tmux ↔ 会话精确映射靠 `@ccm_sid`，不靠名字/目录反推（F74 / #63 / SS-5 / SS-9）

**后端身份 ≠ 会话身份**：`/branch` 在同一 tmux 后端里把活跃会话从 A 换成 fork 会话 B，
tmux 名不变（权威规格 `agents/claude-code.md` §4）。同一目录还常有多个 claude tmux（原会话
+ 分支 + cct 同名加后缀的 `<dir>_cc-2/-3`）。**按 `cwd` 取第一个、或按 `cc-<创建时sid>` 幂等
attach，都会撞进漂移 / 别的会话**（#63「两 tab 内容与 attach 不一致」、「灰会话 resume 进最新
branch」的同一根因）。

**契约**：`__ccm_rbind`（`shared/ccm-wrapper.sh`，装进远端 `~/.bashrc`）每秒从 pidfile 读当前
sid，写进 tmux user option **`@ccm_sid`**（随 /branch 实时更新）。选 user option 而非 pane
title：**title 会被 Claude 自己的活动标题（`⠂ …`）抢写、不可靠；user option Claude 碰不到**
= 「这个 tmux 此刻在跑哪个 sid」的权威带外信号。后端 `tmux.rs::TMUX_LS_FMT` 末列
`#{@ccm_sid}` 读它，`TmuxSession.sid` 承载；空串（老 wrapper / 未装）→ `None`。

**铁律**（守 SS-5/SS-9「tab 身份钉在会话身份，找不到就报『不存在』，绝不静默换一个」）：
- **attach / resume 定位后端，一律先按 `sid===@ccm_sid` 精确匹配**（`tabs.ts::findClaudeTmux`）。
- **`@ccm_sid` 已知却无一命中 → 判「目标会话不在任何 tmux」，绝不回退按 cwd 抓同目录别的 claude**
  （那正是撞错会话的老 bug）。只有**整张列表都无 `@ccm_sid`**（老 wrapper）才回退旧 cwd 匹配，
  向后兼容。
- **灰会话 resume 找不到活后端 → 起全新 `--resume`，tmux 名用 `pickFreshTmuxName` 挑不撞名**
  （避免复用被漂移占着的 `cc-<sid8>`），保证落进原会话而非 attach 漂移的别人。
- **`@ccm_sid` 是阶段② daemon `session.status()` RPC 的先声**（SS-13：tmux 每条能力都是一个
  daemon RPC 的 shell 仿真）；别把它固化成"只有 tmux 能这样"，它是"后端自报当前 sid"的通用形态。

## 31. 一端起的会话另一端必须能接——前端绝不硬编码会话后端命令（F90 / #48 / SS-12）

**用户 2026-07-15 原话**：「我不能接受这边产生的会话那边看不到。」

**为什么硬**：**会话后端（多路复用器）是机器的属性，不是界面的属性**。桌面用 abduco 起的会话、手机只会
tmux → **接不上**，会话池当场劈成两半。而「桌面起、手机接着用」正是要两个界面的全部理由。**注意**：
路线图把「tmux→abduco」列为「可逆、尽管拖」——那对「哪天整体换」成立，对「两端各用各的」**不成立**；
后者不是可逆决策，是当场把自己劈开。

**会话后端 vs 后台程序（SS-11）**：会话后端（tmux/abduco/dtach）**扶着**跑着的交互程序、握命脉；后台
程序（daemon）是**旁观者**，读文件流回来、回答问题、不扶任何东西。**协议可合、进程不能合**。

**最终形态**（三条）：
1. **前端绝不硬编码后端命令**（不准出现可执行的字面 `tmux attach` / `tmux new-session` / `tmux send-keys`）
   → **问一层要**（阶段②问 daemon，SS-11 保证它永远在；阶段①问前端座 `src/session-backend.ts`）。
2. **某机有哪些后端靠能力探测**（阶段②，接 SS-8 能力协商 §26）。
3. **任何后端变更，动手前必须先答「另一端还接得上吗」，答不上不准做。**

**阶段① 落地（F90，2026-07-17）**：唯一后端 = tmux。命令语法已从 `remote-launch.ts` 收敛进纯座
`src/session-backend.ts`（`SessionBackend` 接口 + `TMUX_BACKEND` 实现 + `SESSION_BACKEND` 活跃句柄，照
`agent-profile.ts` 两轴正交范式：agent-profile=哪个 AI、session-backend=哪个多路复用器）。`remote-launch.ts`
正文**无 tmux 命令字面量**（只留 doc 注释；机械 grep 门禁 `grep -nE "tmux (new-session|send-keys|attach)"
src/remote-launch.ts` 只命中注释）。**本阶段不做后端探测/协商**（`SESSION_BACKEND` 恒等 `TMUX_BACKEND`、
无运行时选择）——那是阶段②（§9 轨道二 daemon 在场，才补得了 abduco/dtach 缺的 `send-keys`）。

**阶段② 约束**：加任何第二后端前，先过最终形态第②③条；登记表主键守 §28（用 CC `sessionId`，不许拿
tmux 会话名/主机名/路径当持久主键，否则本约当场崩）；`@ccm_sid`（§30）是「后端自报当前 sid」的通用形态、
别固化成只有 tmux 能这样。

## 32. 本仓只有暗色主题——别声称"明暗两套都覆盖了"（仓库级事实）

**事实**：`styles.css` `:root` 设 `color-scheme: dark`，**全仓无 `prefers-color-scheme`**；`theme.ts` 的
`TOKENS` 是一组**固定的**可换肤 token（bg/text/accent 等），换肤只在这组暗色调色板内动，**没有第二套浅色
主题**。`--overlay-*` / `--border-*` 是 `:root` 固定值、不进换肤范围（也不进设置面板）。

**为什么记这条**：过去有人以为"主题系统 = 支持明暗两套"。不是。用户若把 `bg` 调成浅色，`--overlay-hover`
等固定叠加值不会跟着反相 → 观感崩。改动涉及主题 / 配色断言前先认这条事实：**只有暗色**，别在文档/宣传里
声称覆盖了浅色。（本条 audit-fixes F11 从 `account-ux/MASTERPLAN.md` 上移至此，作仓库级事实沉淀。）

## 修改本文档

加新的不变量时：

1. 加到本文档对应位置 + 编号
2. 在 `src/` 或 `src-tauri/` 对应模块的 doc comment 里加引用 `// 违反此约束见 doc/INVARIANTS.md § N`
3. 如果不变量需要 grep checklist（如 State 注册），加到 [CONTRIBUTING.md](CONTRIBUTING.md) 对应 checklist

删除某条不变量（极少）：

1. 写 RFC 解释**新的约束**是什么、为什么旧的可以松动
2. PR 描述里链到这条 RFC + 全代码库 grep 受影响处确认全修
