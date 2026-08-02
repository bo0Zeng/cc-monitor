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

**G6 远端分叉：本约的写面从「monitor 写远端」扩到「daemon 在远端写」，故单列一段（澄清 + 收窄，用户 2026-07-30 拍板「要对远端也 branch」）**：
远端会话的 jsonl 在另一台机器上，monitor 够不着 ⇒ 分叉这件事由 **daemon 自己在那台机器上做**
（`remote-daemon-proto/src/control/fork_write.rs`，monitor 侧入口 `remote_branch::create_remote_branch_session`）。
它与上面 F62 那段是**同一件事的远端形态**（用户显式点 `⑂` → 复制 `[根…该消息]` 前缀成一个全新
`<new-sid>.jsonl`，**原会话一字节不改**），但因为写的人从 monitor 变成了 daemon，多出三条收窄：

1. **daemon 的写面被守卫钉死在一个模块**（`readonly_guard`，E50 两层收窄）：默认层禁 11 类写操作，
   白名单层**只放行 `control/fork_write.rs` 这一个路径**（U3 起是**路径**不是文件名，见 §41.6），
   且该文件必须含 `.create_new(true)`、
   不得含 remove/rename/truncate/append/overwrite/`create(true)`/`set_len`。
   ⇒ 「daemon 会写盘」这件事**不可能悄悄扩散到第二个模块**。
2. **`create_new(true)` = `O_EXCL`**：目标已存在直接失败。既消掉 `exists()→write` 的 TOCTOU 窗口，
   也自证「绝不覆盖任何现存会话」——两个 monitor 同时分叉同一会话，后到的拿到错误而不是把先到的盖掉。
3. **daemon 只收 sid、不收路径**（`fork_write::find_session_file`）。daemon 是被 ssh 远程调起来的，
   少一个可被构造的路径入参就少一条路径穿越面；sid 先过 `[A-Za-z0-9-]` 白名单，再**只在
   `<claude_dir>/projects` 下按文件名匹配**。monitor 侧 `remote_branch::validate_fork_id` 同一字符集
   再拦一道（fail-fast，不是最后一道）。

#### ★ D1 裁决（U8a-2，2026-08-02）：铁律收窄为「**daemon 进程自身**不许写用户既有数据」

**问题**：daemon 一旦起 `ccm` / `claude`，用户既有数据**一定会被改**（CC 写 jsonl、重写 pidfile；
`shared/ccm` 头注还写明 `--tmux` 会顺带写 `~/.claude.json`）。
而 `readonly_guard` 只认**文件系统写模式**、**不认 `Command` / `spawn`** ——
散文铁律的字面意思不放行这件事，**而 CI 永远不会红**（§0.2 登记过：「护栏与散文说的不是一件事」）。

**裁决**：取主计划 §5 的选项 ① —— 铁律收窄，**间接写不算违反**。边界如下，**这三句就是边界本身**：

1. **责任在被起的那个程序。** CC 写它自己的 jsonl / pidfile 是 CC 的行为，不是 daemon 的写。
2. **daemon 的责任是「不越权替它决定写什么」。** 起一个程序、让它按用户的意图去跑 = 允许；
   替用户决定往它的配置里塞什么 = 不允许（除非走下面那条受管例外）。
3. **收窄不许退化成「隔一层 exec 就绕过」** —— 所以它**必须**配一份逐条清单，见下。

**强制条件（与裁决同时生效，不是建议）**：起进程的面**逐条登记**，且登记是**机检**不是散文：
`readonly_guard::spawn_registry::every_process_spawn_in_production_is_registered`
—— 生产段每一处 `Command::new` 都必须在清单里并写明「做什么、为什么不违反收窄后的铁律」。
今天清单上有 **3 处**（`control/tmux_hook.rs` 起 `tmux` 装 hook；`observe/watcher.rs` 两处起 `sh`
跑 `command -v tmux && tmux ls`，只读）。新增一处而不登记 ⇒ **红**（已变异复验）。

⚠ **它挡什么、不挡什么**（如实登记，别以为它保证了更多）：挡「悄悄新增一个起进程点」；
**不挡**「已登记那条改成起别的东西」—— 登记的是**文件名**不是完整 argv，
而 argv 里有格式化变量（`tmux_probe_script()` 拼的脚本），钉不住也不该钉死。

**预信任那条是单列的受管例外**：daemon **主动**让 `ccm` 去写 `~/.claude.json`（首次进某目录的信任确认）
—— 那是第 2 条里「替用户决定写什么」的一个**明确例外**，因为不做它自动化会卡在弹窗上。
它今天还没落到 daemon 侧（`--account-trust` 只**读**、只回三个布尔）；真落地时要在这里再列一行写面。

**为什么不算松动**：破坏性上它与已放行的「远端历史删除」（例外 3）不在一个量级——那条真的会让
用户的会话消失，这条只增不减。且它**没有引入新的写入者**：daemon 早就在写远端
（`~/.cc-monitor/bin/` 自部署，例外 2），G6 只是让它多写一个 `projects/` 下的**新** jsonl，
并第一次给它的写面套上了机检守卫。

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

> **`zero-poll-liveness` 更新（P5）——事件路同样只经 emitter**：daemon 现在会主动推正向死亡帧
> `TmuxSessionClosed { name }`（tmux hook → SIGUSR1 → 差分算出消失的会话名，见 §41）。
> 它是 `remote_tx` 的**第四个**生产者，与前三者一样**必须**走 `SessionChange{removed}`
> ——`ssh_source.rs` 收到该帧后拿 name 反查 sid，然后送 emitter，**零新写点**。
> 查不到 sid 时（never-bound 会话 / 快照还没到）**不猜**，交回收割器兜底。
> 事件路只是**绕过 miss 计数**的快路径，`RETIRE_MISS_THRESHOLD >= 2` 与快照对账路径一字未动。

> **audit-fixes F03.2 更新——收割器已从 8s poller 改为「收帧驱动」**：原 `tmux_reconcile.rs::run_tmux_reconcile_poller`（8s 定时轮询 + `snapshot_announced_by_origin`）**已删**——cc-monitor 侧零轮询（唯一周期=daemon 内部 tmux ls）。收割逻辑现内联在 `ssh_source.rs::stream_loop` 的 `InboundFrame::TmuxSessions` 帧臂：daemon 每 ~8s 推该 origin 的 `tmux ls` 原文即对账一次，`tracked = 本连接 announced（keys=live sids）∪ 本 origin idle 会话`，调 `reconcile_step` 去抖 retire → 送 `removed` 给 emitter。`reconcile_step` 与 `ReconcileState` 保留为纯决策（F90 可 lift）。

## 24bis. 远端 idle-tmux 灰灯（audit-fixes F03.2）：`REMOTE_IDLE` 与 `remote_active` 正交、同一 emitter 单写

远端会话三态：**live**（claude 在跑）/ **idle-tmux**（claude 退出但 tmux 会话仍在 → 灰灯、可 attach 复用）/ **archived**（tmux 也没了）。承接 §24 单写者不变量，落地约束：

1. **`REMOTE_IDLE` 是独立账本，唯一写者仍是 emitter**：`ssh_source.rs` 的 `REMOTE_IDLE`（origin → idle sid 集）与 `remote_active` **正交**——`mark_idle`/`clear_idle` 只由 `remote-session-emitter` 调（`run()` 的 removed/added 臂），其余路径（收割器、F5 对账）只 `snapshot_idle_*` 读。**绝不**给 `SessionChange` 加字段承载 idle（收割器仍只发 `{removed}`）。
2. **emitter removed 臂据 tmux 存活分流**（`classify_removed` 纯函数 + `find_tmux_origin_for_sid`）——
   ★ **S0（2026-07-31）加了一道前置：`cause` 先于快照裁决**。daemon 现在在 `session_removed` 帧上
   带 `cause`（additive，`Gone` 不上线、缺省即 `Gone`）：`Superseded` = 同一个 pidfile **原地换了 sid**
   （`/branch`、`/clear` —— claude 进程不重启，只是 sessionId 变了）⇒ **恒归档，根本不查快照**。
   为什么必须绕开快照：那个入参对这个场景**恒错** —— 旧 sid 的 tmux 格子确实还在，但它现在挂的是
   **新** sid；而这份快照在 P5 删掉 8s ticker 之后，`/branch` 不触发任何事件路径去刷新它（§41.3 盲区 ④）
   ⇒ 判成 idle 就是一个**永远消不掉、也 attach 不上**的灰点（用户 2026-07-30 实测「杀不掉」）。
   `Gone` 维持下述原语义：`Some(origin)`=tmux 会话尚在 → `mark_idle` + emit `SESSION_IDLE` + **不 forget**（不进归档、`remote_active` 早已在上方移出该 sid，idle 天然在集合外，**不新增 `remote_active` 写点**）；`None`=tmux 也没了 → `clear_idle` + `forget` + emit `SESSION_ENDED`（原归档路径）。判据 **command-agnostic**：`TmuxSessions` 帧最长 8s 陈旧，退出瞬间 command 列可能仍是 claude，故「claude 死」由 daemon-removed 边沿判、「tmux 在」由 `@ccm_sid` present 判（见 `tmux_origin_for_sid`）。
3. **idle→archived 的产出者 = 收帧收割器**：idle sid 并入收割器 `tracked`；且因 `@ccm_sid` 铁证其绑过 tmux，作 `reconcile_step` 的 `pre_bound` 直接播种 `ever_bound`——否则「SessionRemoved 删 announced」与「emitter mark_idle」之间的跨线程缝里那帧会漏置 `ever_bound`，令 idle sid 永不累计缺失 = 连接内卡灰关不掉。tmux 真消失 → 收割器去抖后 retire → emitter 走 `None` 归档。
4. **前端灰灯与 §24 第 2 条同源**：`session-idle` 与 `session-ended` 同进 `events.ts` 的 queue（对同一 sid 二者互斥、emitter 择一）。`Tab.tmuxIdle` 与 `TabStatus` **正交**（status 仍 live、仅灯变灰，不碰任何 archived 门控）。清灰**主**信号 = `ensureTab`（远端 tab 又收 daemon 重宣告/行 = claude 复活，queue 内与行保序）；`session-activity` 为次要（非 queue、null-activity daemon 下不可靠，**不可**作唯一清灰路径）。

**单写者已机器化**（Phase G）：第 1 条「`mark_idle`/`clear_idle` 唯一写者=emitter」原靠注释约定、`cargo check` 抓不住；现有 `ssh_source.rs::f032_idle_tests::remote_idle_single_writer_guard` 扫源码断言这两个写函数**只被 lib.rs 调用**，emitter 之外新增写者即测红（同 F08 daemon 只读护栏的机器化思路）。

**原「已知残留」已修（2026-07-30，`zero-poll-liveness` P1；用户当日松了「daemon 零改」红线）**：收帧收割器原先对**空 backend 一律保守跳过**（`ssh_source.rs` 的 `!backend.is_empty()` 门），代价是当**被杀的是该 origin 最后一个 tmux 会话**时（tmux server 随之退出、`run_tmux_ls` 回空串）收割器整段跳过 ⇒ 该 idle-tmux 灰灯**卡到断连 flush 才清**。

**修法**（比原记档设想的更细，因为 P0 实测把状态空间量清了）：
1. **daemon 让 rc 透出**——`run_tmux_ls` 原先 `tmux ls … 2>/dev/null || true` 把 tmux 的 rc **吞掉**，五种观测压成"空串/有内容"两种；现改为 `exec tmux …`（rc 原样成为 `sh` 的 rc）+ 一个约定 rc 表示"PATH 里无 tmux"，折成四态 `Sessions / ZeroSessions / NoTmux / Unobservable`（`watcher.rs::classify_tmux_probe`）。
2. **P0 实测订正了原记档的措辞**：原文说的「命令成功但零会话」在默认 `exit-empty on` 下**不出现**（server 随最后一个会话退出、rc=1）；但 `exit-empty off` 下**确实出现**且 **rc=0 + stdout 空**。两者对 retire 决策等价 ⇒ 合成一个 `ZeroSessions`（区别只对 P3 的复活监视有意义 ⇒ 将来加细分**不必改帧契约**）。
3. **wire additive**：`TmuxSessions` 帧加 `observation: Option<String>`（`"zero_sessions"` / `"no_tmux"` / `"unobservable"`），**有会话时省略** ⇒ `raw` 载荷与之前逐字节一致 ⇒ **旧 monitor 行为零变化**（空 raw 照旧保守跳过）。**不 bump `PROTO_VERSION`**。取值集是 monitor↔daemon 的**第三个双写点**（前两个：`TMUX_LS_FMT` · `NO_TMUX`），由 `tmux.rs::observation_tokens_double_write_point_stays_in_sync` 钉住。
4. **monitor 把那条内联 if 提成纯函数** `tmux::classify_tmux_observation`（原判断住在需要真远端连接的 `async fn` 里、单测碰不到）。`ZeroSessions` ⇒ 返回**空集但有效**的 `Backend(∅)` ⇒ 照常进 `reconcile_step` 累计缺失；`NoTmux`/`Unobservable` 才跳过。

**修完后的延迟**：该场景从「永不（卡到断连）」变成 **`RETIRE_MISS_THRESHOLD`(2) × daemon 推帧间隔(8s) ≈ 16s**——**是有界化，不是即时化**。

> **已落地（2026-07-30，P3/P5）**：8s 推帧间隔本身**已删**，四路信号全部事件驱动 ⇒ 该场景走 tmux server 的 `pidfd`，**实测 27ms**（跨 cgroup 整锅 SIGKILL 30ms）。「多个中杀一个」走 hook→SIGUSR1 + 正向死亡帧，**实测 126ms**（对照组：拆掉 hook 5042ms）。详见 **§41**。

**一处刻意的保守**：`rc=1` 直接判 `ZeroSessions`，意味着"socket 权限异常"这类罕见情形也会被判成零会话（理论上可能误 retire）。缓解：socket 路径 uid 隔离。~~**P3 落地后收紧**~~ **✅ P3 已收紧**（`watcher.rs::classify_tmux_probe`）：「server 活着但 `tmux ls` rc=1」= 真异常 ⇒ 归 `Unobservable`，帧契约未改。**★ 一处比原设想更细的地方**：判据**刻意不依赖「pidfd 是否醒过」**——那会在 pidfd 路失效时把 rc=1 **永久**压成 `Unobservable` ⇒ 永不 retire；改成直接查 `/proc` 里 server pid 还在不在（一次存在性读，无挂死风险），有专门的变异钉住。

~~**尚未真机生效**~~ **✅ `BUILD_ID` 已 bump 为 `p1r-event-liveness`**（2026-07-30，P7）——在此之前它一直是 `p1q-accounts`，已部署的旧 daemon 不会被判 stale、不自动重装 ⇒ 本修复在远端**休眠**。**如实记**：这次 bump 原计划排在 P5，**P5 漏做了**，是 P7 开工复测时才抓出来的（「有些遗漏不会红任何测试」的又一例：BUILD_ID 是个字符串常量，改不改都全绿）。真机生效仍需重部署。

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

**契约**：身份回填 poller（F02 起住 `shared/ccm` 内部，取代已删除的 `shared/ccm-wrapper.sh`/
`__ccm_rbind`）每秒从 pidfile 读当前 sid，写进 tmux user option **`@ccm_sid`**（随 /branch 实时
更新）。选 user option 而非 pane title：**title 会被 Claude 自己的活动标题（`⠂ …`）抢写、不可靠；
user option Claude 碰不到** = 「这个 tmux 此刻在跑哪个 sid」的权威带外信号。后端
`tmux.rs::TMUX_LS_FMT` 末列 `#{@ccm_sid}` 读它，`TmuxSession.sid` 承载；空串（未装 ccm CLI /
未经它启动）→ `None`。poller 读 `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/sessions/`（账号感知；
默认布局下与 `$HOME/.claude/sessions/` 同一 inode，故行为不变——D7 已证伪其为独立改造点）。

**铁律**（守 SS-5/SS-9「tab 身份钉在会话身份，找不到就报『不存在』，绝不静默换一个」）：
- **attach / resume 定位后端，一律先按 `sid===@ccm_sid` 精确匹配**（`tabs.ts::findClaudeTmux`）。
- **`@ccm_sid` 已知却无一命中 → 判「目标会话不在任何 tmux」，绝不回退按 cwd 抓同目录别的 claude**
  （那正是撞错会话的老 bug）。只有**整张列表都无 `@ccm_sid`**（老 wrapper）才回退旧 cwd 匹配，
  向后兼容。
- **灰会话 resume 找不到活后端 → 起全新 `--resume`，tmux 名用 `pickFreshTmuxName` 挑不撞名**
  （避免复用被漂移占着的 `cc-<sid8>`），保证落进原会话而非 attach 漂移的别人。
- **`@ccm_sid` 是阶段② daemon `session.status()` RPC 的先声**（SS-13：tmux 每条能力都是一个
  daemon RPC 的 shell 仿真）；别把它固化成"只有 tmux 能这样"，它是"后端自报当前 sid"的通用形态。

**F04 扩展（R10 根治）——命中 >1 时同样不许静默换一个**：`@ccm_sid` 只写不清（见上，`__ccm_rbind`
明写"不 unset"），resume 前"是否已存活"的判断只在点击瞬间查一次远端、终端手动 resume 与 app 内
resume 之间也没有互斥，故一个 sid 可能同时活在 ≥2 个 tmux 容器里。SS-5/SS-9 原文只讲了「零命中」
这一半（找不到就报不存在），F04 把同一条准则对称扩展到「多命中」：

- **`tabs.ts::findClaudeTmuxMatches`** 返回**全部**精确命中（不折叠成第一个）——`findClaudeTmux`
  只是它的单值投影（`matches[0]`），供不关心"是否有重复"的调用方沿用。
- **命中数决定后续动作的严重度分级，不是一刀切**：非破坏性操作（attach/resume）命中 ≥2 个时**警告
  并按第一个继续**（可撤销：重新点一次就能换目标）；破坏性操作（kill/换号重启）命中 ≥2 个时**拒绝**
  （代价不可逆——选错的那次可能杀掉了对的那个、留下错的那个继续跑/计费）。**绝不**在破坏性操作上
  静默挑一个了事。
- 这条分级本身（哪些动作该警告继续、哪些该拒绝）是产品判断，不是纯粹的正确性问题——加新的
  "命中多个"消费方时，先问「这个动作选错了目标，后果能否撤销」，而不是照搬某个已有先例。

**F04 扩展——`@ccm_sid_expect`（意图）与 `@ccm_sid`（事实）是两个独立的 key，不是同义词**：
`shared/ccm` 建会话/exec 时刻会**立即**声明"打算跑这个 sid"（通道A，写 `@ccm_sid_expect`）——
这只是声明，resume 可能瞬间失败（会话已不存在/网络抖动），从未被独立确认过。只有后台 poller
独立读到 Claude Code 自己的会话文件、确认这个 sid 真的在跑之后，才写 `@ccm_sid`（通道B，唯一
写者）。**任何破坏性判断（Gate 2 远端半支、kill/send-keys 的身份核验）只认 `@ccm_sid`，绝不认
`@ccm_sid_expect`**——否则一个从未真正跑起来过的声明会永久冒充"事实"，被后续的身份核验采信。
两个 key 都遵循"只写不清"的既有约定（见上）；`_expect` 不进 `TMUX_LS_FMT`（守 daemon 零改动的
范围排除），只在窄场景（idle-tmux 置信度判断，F04 本轮未做，留待以后按需评估）按需惰性查。

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

## 31a. tmux `-t` 目标恒用 `=<名>:` 精确形态——三处同源，禁止任一处退回裸目标（F01 / unify-launch）

**事实（tmux 3.6 实测，隔离 `-L` socket）**：裸 `-t <名>` 不是精确匹配，tmux 依次按
「精确名 → **名字开头** → **glob**」解析。只有 `sib-2` 存在时：`kill-session -t sib` **杀掉 `sib-2` 且 rc=0**
（当成功回报）、`send-keys -t sib` 投进 `sib-2`、`capture-pane -p -t sib` 抓的是 `sib-2`、
`kill-session -t 'si*'` glob 命中。本仓**必然踩**：`pickFreshTmuxName` 刻意造 `cc-<sid8>-2/-3`，
终端 `cct` 造 `<dir>_cc-2/-3`。

**造成过的真实损坏**：`restartWithAccount` 第④步向上一次快照的会话名发 `Escape`+`/exit`。目标已自然结束时
前缀命中兄弟 `cc-<sid8>-2` → **把 `/exit` 敲进另一个还活着的 claude**，④c 的 kill 再销毁它，输出为空 →
判定成功 → 继续 resume + 写 pin。**净结果：无关会话被静默销毁 + pin 写错，而 UI 报告「已重启」。**

**为什么是 `=<名>:` 而不是 `=<名>`（尾冒号不能省）**：`=` 前缀只在 target-**session** 解析路径上被识别。
`send-keys` / `capture-pane` 收的是 target-**pane**，`set-option` / `show-options` 走 pane 解析后上溯——
这些动词上 `=名`（无冒号）直接 `can't find pane`、**rc=1 完全失效**。尾冒号把串强制成 `session:` 形态
（当前 window、活动 pane），`=` 才落在会话名段上被正确识别。`=名:` 是唯一在 send-keys / capture-pane /
set-option / show-options / kill-session / has-session / attach **全部**动词上都既通用又精确的形式。
（此坑真实踩过：第一版修复写成 `=名` 无冒号，三门禁全绿而 send-keys 实际一个键都发不出去。）

**`new-session -s <名>` 收的是名字不是目标，绝不加 `=`/`:`。**

**四处同源**（F02 新增第四处，照 §I8 `TMUX_LS_FMT` 双写范式立条）：
1. `src/session-backend.ts` 的 `exactTarget()` —— 前端 shell 渲染面
2. `src-tauri/src/tmux.rs` 的 `exact_target()` —— IPC 控制面
3. `e2e/restart-shims/core.mjs` —— Tauri IPC 边界的 mock，**结构上无法 import Rust，去重不可能**；
   必须**与生产同构**，否则 e2e 对这条假绿
4. `shared/ccm`（F02 统一启动 CLI）—— 独立的 tmux 命令构造器（不复用前三处，语言/执行环境不同）；
   守卫见 `src-tauri/src/sftp.rs` 的 `ccm_cli_has_required_elements`：**结构性扫描**每个 `-t ` 目标
   （不是固定 needle——固定 needle 版本实测空转，把 `=名:` 全改回裸目标三门禁仍全绿）

e2e 的 shell 探针（`has-session` / `set-option` / `kill-session`）同样要用 `=名:`——**探针本身前缀匹配会说谎**
（只剩 `X-2` 时 `has-session -t X` 返 0，"会话还在"假阳；`set-option -t $S` 会把 `@ccm_sid` 写到错的会话上、
直接污染 fixture）。F01 的整个论点就是"前缀匹配会说谎"，探针不能例外。

**漂移守卫**：`session-backend.test.ts` 有一条读 `e2e/restart-shims/core.mjs` 的断言把 shim 形态与座钉在一起；
Rust 侧 `tmux_targets_use_exact_match` 钉死三个命令构造点（且显式断言**不含**裸目标，防被"简化"回去）。

**第二道防线**：`isValidNewTmuxName`（**仅创建路径**）禁 glob 字符 `*`/`?` —— 本工具永远不把 glob 建进名字。
attach 已有会话走宽松的 `isValidTmuxName`：那些名字不是我们建的，禁它既无收益（`=名:` 已关闭 glob 这一级，
实测 `-t '=st*ar:'` rc=0 且精确）又是行为回归。

## 32. 本仓只有暗色主题——别声称"明暗两套都覆盖了"（仓库级事实）

**事实**：`styles.css` `:root` 设 `color-scheme: dark`，**全仓无 `prefers-color-scheme`**；`theme.ts` 的
`TOKENS` 是一组**固定的**可换肤 token（bg/text/accent 等），换肤只在这组暗色调色板内动，**没有第二套浅色
主题**。`--overlay-*` / `--border-*` 是 `:root` 固定值、不进换肤范围（也不进设置面板）。

**为什么记这条**：过去有人以为"主题系统 = 支持明暗两套"。不是。用户若把 `bg` 调成浅色，`--overlay-hover`
等固定叠加值不会跟着反相 → 观感崩。改动涉及主题 / 配色断言前先认这条事实：**只有暗色**，别在文档/宣传里
声称覆盖了浅色。（本条 audit-fixes F11 从 `account-ux/MASTERPLAN.md` 上移至此，作仓库级事实沉淀。）

## 33. LaunchPlan 双渲染器——CLI 渲染器对无法诚实表达的维度/容器形态必须放弃，不得近似（F03 / unify-launch）

**背景**：F03 把 7 个 builder 收敛成 `LaunchPlan` IR（`src/launch-plan.ts`）+ 维度注册表
（`src/launch-dimensions.ts`）+ 两个渲染器：`renderFallback`（`src/launch-render-fallback.ts`，
编译 IR 成裸 shell 串，逐字节等于 F03 之前的输出）与 `renderCli`（`src/launch-render-cli.ts`，
翻译成对 `ccm`（F02）的一次调用）。`canRenderCli` 是两者之间的**唯一分流点**。

**铁律**：`canRenderCli` 对以下两类情形必须返回 `false`（强制走 `renderFallback`），**不得**为了
让更多场景走上"看起来更先进"的 CLI 路径而近似渲染：

1. **任一已触发维度的 `cliFlags(ctx)` 返回 `null`**——这是维度作者的显式声明"我在当前 `ctx` 下
   无法用 CLI 语法表达"。F05 之前 `account` 维度恒如此（调用方只有 `configDir` 没有账号
   「名字」）；**F05 后账号名已线通**，`cliFlags` 对 `account`（有名字时）/`base` 两态都吐实际
   flag——只有"账号存在但名字缺失"（`remote-launch.ts` 保留的老式直调路径）这一种情形才继续
   返回 `null`，见 §35 的完整讨论。
2. **`container.kind==="tmux"` 且 `mode==="send-into"`**（往已存在的 idle tmux 就地复用，不新建）——
   `shared/ccm` 的 `--tmux` 只有幂等 create-or-attach 一种形态，没有这个能力。硬套会让 #76（claude
   已退出但 tmux 还在，短路跳过 send-keys，用户 attach 进空 shell）以 CLI 路径的新形式复发，且现有
   回归测试测不到（它们测的是兜底路径的 builder，不测 `renderCli`）。**`mode==="attach-only"` 不受此
   限**——`ccm attach <名>` 与 `shared/ccm` 源码核对就是 `exec tmux attach -t "=$名:"`，与兜底渲染器
   的 `SESSION_BACKEND.attach()` 逐字同构，没有 create-or-attach vs 就地复用那种歧义，可安全走 CLI
   渲染器（F03 Phase D 架构审计发现：早期实现把两种模式并入同一把闸门，导致 `renderCli` 的 attach
   分支在生产路径上永不可达——已收窄为只挡 `send-into`）。

**attach 分支的显式豁免（R04 Phase D 审计要求补写）**：上面铁律#1、以及 R04② 引入的
`requiredCaps` 收集，**都不适用于 `action.kind === "attach"`**——`tryRenderCli` 的 attach 分支
在维度循环**之前**就 return（沿用"attach 不读其余修饰"的既有结构）。这是**刻意放宽**：
`ccm attach <名>` 不接受 `--account`/`--base`/`--model` 任何修饰 flag，
对一次纯 attach 要求远端 ccm 支持这些能力是过度收紧；且 attach 是接回一个**已经在跑**的进程，
它的账号在创建时就定了，此刻注入任何 env 都不改变那个已存在进程的身份
（`INVENTORY.md` §A #6 已把这件事写成设计而非缺口）。
改造前静态 `CLI_REQUIRED_CAPS` 是无条件检查的，故 attach 一次放宽了**三**道闸门：
`account` 能力、`model` 能力、以及铁律#1 本身。**`model` 那道的放宽是真实可达的**——
`model` 能力是 `06a9c76`（F08）才加进 `shared/ccm` 的 `capabilities=`，
所以装了 F02～F08 之间任一版 ccm 的远端就处在"缺 model"状态。
豁免由 `launch-render-cli.test.ts` 的「attach 豁免组」三条测试钉住（各对应一道闸门），
**不是"碰巧没人测到"**。若将来 `ccm attach` 学会接受修饰 flag，这条豁免必须同时撤销。

**为什么钉成不变量而非留作注释**：未来任何人加新维度或新容器形态，若忘记正确实现 `cliFlags`
（或忘记声明某形态 CLI 表达不了），`canRenderCli` 会**默认放行**（`cliFlags` 未定义时视为
"这维度不影响 CLI 可行性"），静默把一个 CLI 表达不了的 plan 送进 `renderCli`，产出一条**语法正确
但语义错误**的命令——这类 bug 不会在类型检查或黄金串测试里现形，只会在真机上表现为诡异的会话
行为。加新维度/新容器形态时，必须显式想清楚它在 `cliFlags` 下的行为，而不是留给默认值蒙混过关。

**验证**：`src/launch-render-cli.test.ts` 的 #76 防线测试组——通过临时删除 `canRenderCli` 里的
`mode !== "create-or-attach"` 判断、确认恰好 2 条测试转红，证明该判断不是摆设（见测试文件头注）。


### R04① 更新（2026-07-28）：这条从「调用约定」升级为「结构保证」

原文说"`canRenderCli` 是两者之间的**唯一分流点**"——那是**意图**，不是当时的事实。
当时是两个独立导出：`canRenderCli` 检查 `null` 并返回 `false`，而 `renderCli` 里那句
`if (flags) tokens.push(...flags)` 对 `null` **静默跳过**、继续渲染。
即：只要有人直接调 `renderCli`（不先问 `canRenderCli`），就会产出一条**丢了那个修饰**的命令，
而丢的恰好是账号这类东西——症状即 R11/R08 那族"看起来生效了，只是用了错的号"。

现已合成单一导出 `tryRenderCli(plan, ctx, probe) → { ok:true; cmd } | { ok:false; reason }`：
`null` 在同一次遍历里直接变成 `ok:false`，**拿不到命令**。上面那段"默认放行"的担忧因此从
"靠加维度的人自觉"变成了"结构上做不到"。`reason` 同时把"为什么降级"这个此前丢掉的信息带出来。

**验证**：`src/launch-render-cli.test.ts` 的 R04① 两条测试——用真实可达的
"账号有 configDir 但无名字"（老式直调路径，见 `launch-requests.ts::accountOf` 头注）
断言 `ok:false` 且**结果里没有 `cmd` 字段**。

## 34. tmux 破坏性/半破坏性命令三道门 + 原子 verify+act（F04 / unify-launch / R10）

**背景**：F04 根治 R10——过去 `kill_remote_tmux`/`tmux_send_keys` 只有一道门（`is_ccm_tmux_name`
名字前缀判据），且"查一次状态、再发一条动作命令"是两次独立远端往返，中间留 TOCTOU 窗口。

**三道门**（`src-tauri/src/tmux.rs`）：
1. **Gate 1（恒强制）**：`is_safe_tmux_target`——只拒**空** target（`=:` 会被 tmux 解析成「当前
   会话」，是唯一真正危险的默认值）。**不额外收紧字符集**——glob/元字符交给 `shell_quote` 安全
   引号化，字符集收紧是 TS 侧 `isValidNewTmuxName`（仅创建路径）/`isValidTmuxName`（attach 故意
   宽松）的职责，见 §31a「第二道防线」。折进 `exact_target` 本身（fallible），任何未来新增的
   tmux 命令构造点结构性不可能绕过它。
2. **Gate 2（identity，union）**：`is_ccm_tmux_name`（本地、零 IO，前缀命中）**或** `@ccm_sid`
   已设（远端核验）。**`is_ccm_tmux_name` 不删除**——F02 之前的老 `cc-*` 会话没有 `@ccm_sid`，
   仍必须可 kill/send-keys，否则是向后兼容回归；F02 之后 `--tmux=<自定义名>` 建的会话没有前缀，
   必须靠远端 `@ccm_sid` 核验才放行，不能被一刀切拒绝（那本身是 F02 引入的真实网开一面缺口）。
3. **Gate 3（仅破坏性动作，即 kill）**：远端 `session_windows==1`——拒绝 kill 一个已长出额外
   window 的会话（signal：有独立于本工具的用户活动，不该被这一个 kill 动作连坐端掉）。send-keys
   不删任何东西，不受此门。

**原子 verify+act**：Gate 2 远端半支 + Gate 3 折进**一条**远端命令（`build_guarded_tmux_cmd`），
用 `tmux display-message -p -t <target> '<fmt>'` 一次性取 `session_windows`/`@ccm_sid`，同一
round-trip 内判断后再执行动作——不是"先查一次、再另发一条动作命令"（那正是 R10/#76 的共同根因：
两次远端往返之间的窗口可被抢跑）。用 `display-message` 而非 `show-options`：后者对未设置的
option 是 `rc=1` + stderr、需要脆弱的 rc/stderr 联合判断；`display-message` 走这个仓库已验证的
格式串插值惯例（`TMUX_LS_FMT` 同款），未设置的 option 静默展开成空串，`session_windows` 恒为
存在会话的正整数、天然当"目标是否存在"的判据（空捕获串 = 目标不存在）。

**性能纪律**：`Gate 2` 本地命中（`cc-*` 前缀）时**完全跳过**远端半支——kill 仍需 Gate 3 的
`windows` 核验（不能跳），但 send-keys 在这种情形下退化成今天的零 Gate 一行，**零额外 round
trip**，覆盖 100% 的既有真实流量。别为了"统一形态"让不需要远端核验的路径也走一趟——这不是过早
优化，是"新增门禁不该让最常见路径变慢"这条纪律的具体应用。

**验证**：`e2e/tmux-guarded-acceptance.sh`（真机验收，隔离 `-L` socket，14 项）——输入来自
`cargo test --lib -- --ignored --nocapture emit_guarded_commands_for_e2e`（真 builder 产出，不
手搓等价命令），验证 2-window 真会话真的拒绝 kill 且存活、1-window 真的被 kill、无 `@ccm_sid`
的自定义名会话被挡（kill 存活/send-keys 不污染 pane）、有 `@ccm_sid` 时真的放行。Rust 单测只锁
字符串形状，真机验收锁"这条嵌套 if/cut 的 shell 语法真的按预期分支执行"（R1 教训：门禁全绿过仍
放行过一个让 send-keys 完全失效的改动，字符串断言测不出真实行为）。

## 35. 维度的 `applies` 绝不能条件性跳过 `cliFlags` 的 `null` 安全网（F05 / unify-launch）

**背景**：§33 铁律①依赖一个隐藏前提——"任一已触发维度的 `cliFlags` 返回 `null` 就强制降级"这条
检查（`canRenderCli` 循环里的 `if (dim.applies(ctx) && dim.cliFlags && dim.cliFlags(ctx) === null)
return false;`）**只在 `dim.applies(ctx)` 为真时才会跑**。F03 刚落地时，`ACCOUNT_DIMENSION.applies`
写成 `ctx.account.kind === "account"`——也就是说，对**没有选中账号**这个最常见场景（`kind==="base"`），
`applies` 恒 `false`，整条安全网连问都没问过这个维度就被循环跳过了。后果：一个解析成"基座"的
plan，只要满足其余 CLI 渲染条件，会被 `renderCli` 吐成一条**既不带 `--account` 也不带 `--base`**
的 `ccm resume …`——R11（`ccm` 在两者都不传时静默落 manifest 默认账号）的病灶以新形式复发，且
影响的是多数用户（单账号/未装账号库）而非少数（F05 才发现并修复，见 MASTERPLAN §6 R11/R13）。

**铁律**：**一个维度是否要在 CLI 语境下"发声"，不能靠 `applies` 的条件性真假来决定它是否接受
`null` 检查的审视**——`applies` 只应该回答"这个维度在当前 `ctx` 下有没有效果要摊平进
`LaunchPlan`"（兜底渲染器视角），不能被拿来当"这个维度要不要在 CLI 语境下老实交代能不能说清楚"
的代理判据（CLI 渲染器视角）。两件事分属两个不同渲染器的问题，混在一个布尔值里，任何一个
维度只覆盖了"发声"的部分状态（如 F03 的 `account` 只在选中账号时发声），另一部分状态
（未选账号）就会被循环结构性跳过、永远问不到 `cliFlags`。

**实践准则**：新增/修改一个维度时，若它在 CLI 语境下**理应对某个状态有话可说**（哪怕这句话是
"什么都不做"，也要用 `[]`——空数组不是 `null`，`if (flags)` 对空数组仍真值判断成立、循环仍会
`tokens.push(...[])`（no-op）而不会误触发降级），就不能让 `applies` 对那个状态返回 `false`。
`ACCOUNT_DIMENSION` 修复后的形态（`applies` 恒 `true`；`cliFlags` 对 `account` 有名字/`base`
两态吐真实 flag，只对"账号存在但名字未知"这一种情形吐 `null`）是这条准则的落地范例：`null`
只用来表达"这个具体状态我说不出来"，不是被 `applies` 的疏忽间接代出来的。

**验证**：F05 Phase D 审计逐一核对了 `IDENTITY_DIMENSION`/`ENV_RESET_DIMENSION`/
`NESTED_ENV_RESET_DIMENSION`——三者的 `cliFlags` 无论 `applies` 真假都从不返回 `null`（恒
`[]` 或非空数组），结构上不可能重蹈这个坑；当前代码库里只有 `ACCOUNT_DIMENSION` 踩过、且已修。
未来加新维度（如 F07 的 `model`）时，若 `cliFlags` 可能对某状态返回 `null`，必须同时检查
`applies` 是否会在那个状态下把循环挡在门外——这条不是"记得检查"，是加维度时的强制 checklist 项。

## 36. 本地（Windows）路径不经 IR 产出命令——嵌套 env 污染保护已在进程启动期做完，别在本地渲染器里重复实现（F06 / unify-launch；R07 收紧）

**背景**：F06 曾把本地 resume/新建两条路径折进 `LaunchContext`/`LaunchPlan` IR（`src/launch-requests.ts::planLocal`），跑一遍 `LAUNCH_DIMENSIONS` 注册表。**R07 已把这一遍删掉**（理由见下方 R07 段），该函数现名 `validateLocalLaunch`、只做 sid 校验、不构造任何 IR。下面这段描述的是"当时为什么算了却不消费"，其结论（**别给本地渲染器补一段读 env 的代码**）在 R07 之后依然是铁律，只是理由更直接了：本地路径压根不产出 `plan.env`。

（**当时**的机制：`NESTED_ENV_RESET_DIMENSION`（issue #24：清 Claude 自己的嵌套会话标记 `CLAUDECODE`/`CLAUDE_CODE_SESSION_ID` 等）的 `applies` 只看 `ctx.action.kind==="new"||"resume"`、不看 `transport`——local 场景走到这里恒真，`plan.env` 会真的被塞进一条 `unset` `EnvOp`，而本地渲染器 `src-tauri/src/history.rs::build_local_ps_command` **故意完全不读**它。这条"注册表对 local 也会产出 env op"的事实**今天依然成立**，只是本地路径不再去调它了——证据见 `src/launch-requests.vitest.ts` 的「维度注册表在 transport:local 下的行为」那组测试，它直接冲 `buildLaunchPlan` 去验，不借道任何生产函数。）

**为什么不消费是对的**：`NESTED_ENV_RESET_DIMENSION` 保护的攻击面是"tmux **持久 server** 进程的环境表跨多次 resume 累积污染"——远端场景里，同一个 tmux server 可能存活很久，每次新 resume 进去的 shell 都从 server 环境继承，之前一次 `claude` 进程留下的 `CLAUDECODE=1` 等标记会一直挂在那，必须每次显式 `unset`。本地 Windows 场景没有这个"持久 server"概念——`launch_powershell_window`（`src-tauri/src/launch.rs`）每次都是全新 `Command::new("wt.exe"/"powershell.exe").spawn()`，唯一可能的污染源是"cc-monitor.exe 自己被某个带毒环境启动"（如从一个嵌套的 Claude 会话终端里启动 cc-monitor 自身）——这条攻击面已经在**进程启动阶段一次性堵死**：`src-tauri/src/lib.rs::run()` 里 `scrub_env_vars(adapter::active().nested_env_to_scrub())` 是 Tauri `Builder` 构造之前就跑的第一批实质语句，直接 `std::env::remove_var` 清掉 cc-monitor.exe 自己进程的环境；`Command::new(...)` 默认继承（已清洗过的）父进程环境，无需每次 launch 前再清一次。

**铁律**：**给本地渲染器补一段读 `plan.env`、把 `unset` 翻成 PowerShell `Remove-Item Env:\X` 的代码，是错的"修复"**——两层保护本来就分工不同（远端：渲染期逐次清；本地：启动期一次清），本地补一层不会更安全，只会引入一段从未有真机（Windows/`pwsh`）验证过的新 PowerShell 语法，纯增加风险不增加收益。若未来真的发现本地场景存在启动期清洗覆盖不到的污染路径（例如 cc-monitor 在自己生命周期内某处被重新 exec、绕开了 `run()` 的这次清洗），应该去修**启动期清洗本身的覆盖面**，而不是在本地渲染器里加一段渗透式的补丁。

**验证**：`src/launch-requests.vitest.ts` 的「维度注册表在 transport:local 下的行为」组锁死本地 `LaunchContext` 经 `buildLaunchPlan` 产出的 `plan.env` 对 new/resume 两个动作恒非空（证明维度确实触发了）、account 维度对 base 态是 no-op、`cwd: null` 原样透传（R07 Phase D 审计发现拆分中丢过这条，已补回；它是**共享代码**，远端路径也吃），且 `history.rs` 侧未新增任何消费 `plan.env`/`unset`/`Remove-Item` 的代码路径（Phase D 审计已核对 `scrub_env_vars` 的调用时点严格早于任何窗口 spawn，且全仓无绕开它的自重启路径）。

**R07 补充（2026-07-28）：本地路径是「借 IR 做校验、不消费其输出」，这是设计不是半成品。**
上面说的"`plan.env` 算出来不消费"其实是更大一件事的一个切面——**整个 `LaunchPlan` 都不被消费**。
`validateLocalLaunch` 的 4 个生产调用点（`views/history.ts` ×2、`views/session-viewer.ts`、`tabs.ts`）
**全部把返回值当语句丢弃**，真命令由 Rust 独立构造。R07 之前这个函数叫 `planLocal` 且返回
`LaunchPlanBuild`，其单测头注还写着"证明本地路径真的在用同一套维度注册表（不是套了个类型皮的
假装）"——**那句话是假的**：跑了注册表，但结果没人要。已改名 `validateLocalLaunch` + 返回 `void`，
让名字与事实一致。

**为什么不"真接上"（理由经 R07 Phase D 审计订正——我原先引错了论据）**：
初稿引的是 F06 的 `Get-Command` 论证（`features/F06-local-path-ir.md:27-30`）。那条**真实存在**，
但它排除的是"**TS 全量渲染好字符串、Rust 只管 exec**"这一形态，**并不排除**
"TS 构造 IR、Rust 只补 `Get-Command` 那一步"。**真正支撑否决的是 F06 §3.2 实现期修正**：
`plan.action`/`plan.cwd` 在当前维度注册表下**恒等于输入**，取回来**没有信息增量**；
`plan.launcher` 更是恒 `""`（本地不传 `launcherOverride`）。
即"不接"是因为**接了也拿不到新东西**，不是因为技术上不可能——这两个理由的强度与适用范围完全不同，
别再把 `Get-Command` 当成万能挡箭牌。

**L2 复测确认（2026-07-30，`local-as-remote`）**：本节铁律**仍然成立**，且 `local-as-remote` 主计划原本的 L2（「PowerShell 渲染器 honour `plan.env`」）**正是它禁止的那件事** ⇒ **已否决，不做**。启动期清洗的时序也实证过：`lib.rs:124` 的 `scrub_env_vars` 早于 `:161` 的 `tauri::Builder`。L2 改做的是「别让本地/远端静默漂移」这个真意图落在真实漂移点上 —— Rust `adapter` ↔ TS `AGENT_PROFILE`（`src/agent-profile-parity.vitest.ts`）。**那条守卫不违反本铁律**：它只读、不给任何渲染器加读 `plan.env` 的代码。

**另注（同一审计发现）**：`F06-local-path-ir.md` §1 有一条**已勾 `[x]`** 的 DoD 逐字要求
"从产出的 `LaunchPlan` 取 `action`/`cwd`/`launcher` 三个字段映射回现有 Tauri 调用参数"
——**它从未实现**，且已在同文件 §3.2 被撤回（理由即上述"无信息增量"）。那条勾已就地标注撤回。

**R07 为什么连 `buildLaunchPlan` 那一遍也删了**：初稿保留它并声称是"一道便宜的一致性检查"，
但审计实测该声称**零门禁守护**（删掉整段 ctx 构造 + 调用、只留 `void cwd;` → `tsc` 与
`npm test` 705 全绿；改造前同一变异红 5 条，因为那时返回类型让它在**类型层**承重）。
而它想验的东西 `launch-render-cli.test.ts` 已在验（`ctxOf({transport:{kind:"local"}})` → `buildLaunchPlan`）。
生产侧它纯属浪费，且是 **fail-closed 风险**：将来任何对 `transport:local` 抛异常的新维度，
都会让本地 resume 彻底拉不起来而收益为零。

## 37. 新维度的 `applies` 该不该恒真，看这个维度的"沉默"是否等价于用户期望——不是看它是不是账号相关（F07 / unify-launch）

**背景**：F07（每账号默认模型，`MODEL_DIMENSION`）是维度注册表落地以来第一个真实的新维度，是
MASTERPLAN §0.1 成功标准②（"加一个新启动维度 = 注册一个 dimension + CLI 加一个 flag + UI 加
一个修饰项，零改 builder / renderer / 调用点"）的架构验收点。落地后核对：`buildLaunchPlan`/
`renderCli`/`canRenderCli`/`renderFallback` 的既有分支结构 diff 为零，唯一改动是
`renderEnvOps`（`launch-render-fallback.ts`）的 `switch` 加一个 `"export-model"` 分支——这是
成比例的既定触点，不是"渲染器主体"。承诺兑现。

**§35 的教训不能被机械照搬**：`ACCOUNT_DIMENSION.applies` 在 F05 被改成恒 `true`，因为 F03
遗留的 bug 是"最常见场景（未选账号）静默不表态，导致 `canRenderCli` 的 null 安全网检查根本
问不到这个维度，`ccm` 会静默落到 manifest 默认账号——一个和用户期望不同的身份"。如果看到
"这也是个账号相关的维度"就照抄"`applies` 必须恒真"，`MODEL_DIMENSION` 会变成：`applies:
() => true`，`cliFlags` 对"未配置模型偏好"这个最常见状态返回 `null`——把**几乎所有会话**强制
拖进兜底渲染器，纯属自伤，且完全不必要。

**铁律**：一个维度的 `applies` 该不该恒真，取决于**这个维度在"不触发"时的行为，是否等价于
用户的期望**，不是"这个维度是不是账号相关"或任何其它表面相似性：

- `ACCOUNT_DIMENSION`：不触发 = "不表态"，而 `ccm` 对"不表态"的解读是"落 manifest 默认账号"
  ——一个可能与用户期望不同的身份。**沉默 ≠ 用户的期望**，必须恒 `true`、强制显式表态。
- `MODEL_DIMENSION`：不触发 = "不下发 `ANTHROPIC_MODEL` 覆盖"，远端 `claude` 就用它自己已经
  配置好的默认模型——这**正是**用户没配置 override 时应该发生的事。**沉默 = 用户的期望**，
  `applies` 应该是条件式（`!!ctx.modelOverride`），恒真反而是错的。

**判断步骤**（加新维度时的强制 checklist，配合 §35 一起过）：
1. 这个维度不触发时，下游（`ccm`/远端 shell）会怎么解读"没有这个信号"？
2. 那个解读，是不是用户没配置这个维度时**本来就期望**发生的事？
3. 是 → `applies` 可以是条件式，`cliFlags` 对"不触发"这个状态不需要操心（循环压根不会问）。
4. 否（下游会做出某种和用户期望不同的默认选择）→ `applies` 必须恒真，`cliFlags` 必须对
   "不触发"这个状态也给出诚实的显式表达（`null` 或真实 flag，绝不能让循环跳过去问都不问）。

**`cliFlags` 恒 `null`（配了模型偏好时）是另一个独立决策，不要和上面混为一谈**：`ccm` 今天没有
`--model` flag，`MODEL_DIMENSION.cliFlags` 对"配了偏好"这个状态诚实返回 `null`，强制走兜底
渲染器——这与 §35 修的坑**外观相似但机制不同**：F05 的坑是"`applies` 恒假导致 null 检查
根本跑不到"（结构性检测不到）；这里 `applies` 会在配了偏好时正确变真，null 检查确实跑到并
正确返回 `false`——是"检测到了、诚实报告降级"，不是"检测不到、悄悄放过"。`canRenderCli` 对
这条降级有专门的端到端测试锁定（`launch-render-cli.test.ts` 的两条 `modelOverride` 用例），
不只是孤立测 `cliFlags()` 的返回值。

## 38. 一条新正交轴该进 `LAUNCH_DIMENSIONS` 注册表，还是该做 `LaunchPlan`/`LaunchContext` 的硬编码一等字段——三条 checklist（F09 / unify-launch，R12）

**背景**：F09（UI 收敛：动作 × 修饰）设计阶段要处理 R12——`container`（tmux/none，及
`create-or-attach`/`send-into`/`attach-only` 三种 mode）与 `agent`（claude/codex）两条正交轴,
至今仍是 `LaunchPlan`/`LaunchContext` 的硬编码一等字段,不像 `account`/`model` 那样注册进
`LAUNCH_DIMENSIONS`、有 `applies`/`apply`/`cliFlags` 接口。F09 Phase B 开了两个独立 Plan agent
论证"该不该扩大注册表覆盖面"，结论是**维持三轴三机制，只在 UI 层收敛**——理由与判断准则记在此处，
供未来任何新轴（不止 container/agent）参考，防止"看起来不统一"被当成 bug 顺手"修掉"。

**判断准则**（三条都满足才该进注册表；任一条不满足就该继续硬编码）：

1. **它的效果能不能完全表达成"追加/修改 `plan.env`/`plan.args`/`plan.identity`"，不需要两个
   渲染器的主体控制流（`action`/`container` 分支结构）本身长出新分支？**——`account`/`model`
   满足（`renderEnvOps` 的 `switch` 加一个成比例分支）；`container` 不满足：`plan.container`
   从 `buildLaunchPlan` 起就是直接透传定型的载体字段（两个渲染器读它决定该调
   `SESSION_BACKEND` 哪个方法），不是"追加一段 env/args"的效果。
2. **它"不触发"时的默认行为，能不能用 §37 的判据（沉默=用户期望 or 沉默=意外）干净地归入
   `applies` 恒真/条件式二选一？**——`container`/`agent` 都不是"触发与否"的二元问题，而是
   "选哪一个值"的多选问题，这条判据对它们本身就不太适用，是又一个信号：它们的形状和
   environment 轴不同类。
3. **它的影响半径是不是仅限于"这一条要渲染的命令"，不会跨到消息解析/liveness/工具分类等
   其它子系统？**——`agent` 明确不满足：`AGENT_PROFILE` 被 12 个文件直接消费，其中至少 7 个
   跟"启动"无关（`cards/*` 的工具名分类、`tabs.ts` 的 liveness 判定、`shell-quote.ts` 的
   fail-closed 回退）——参数化它是"把一个单例常量变成按 agent 查表"的独立工程，波及面远超
   "给 F09 加一个 UI 修饰项"。

**`container` 的具体结论**：`kind`（tmux/none）是用户在 UI 上真正选的值,但 `mode`
（`create-or-attach`/`send-into`/`attach-only`）**不是**——它是点击那一刻现查远端 tmux 状态
派生出来的值（`tabs.ts::resumeTabTmuxInner` 的探测-派发逻辑：命中活会话→`attach-only`，命中
空 tmux→`send-into`，都不命中→`create-or-attach`），用户从未也不该在 flyout 里选它。即便只
收编 `kind` 部分，也换不来真实简化——`canRenderCli` 的 `mode==="send-into"` 强制降级检查（防
#76 复发）挪进某个维度的 `cliFlags` 后,判断逻辑和验证方式（临时删除、确认恰好 2 条测试转红）
完全不变，只是换了个位置。**维持 `container` 完全硬编码是零风险、零多余改动的选择**。

**`agent` 的具体结论**：现在不该收编，不是"工作量大"，是"收了也是假的"——前端对 codex 零消费
能力，`AGENT_PROFILE` 是单例常量非查找表；且 resume/attach 对已存在会话没有"换 agent"的自由度
（sid 对应特定 agent 的 JSONL 格式），参数化后也只有 `new` 动作能真的用上，打破"修饰对任意
动作正交"的故事。这件事已经有独立计划轨道负责（`src/agent-profile.ts` 头注的
MA-multi-agent-adapter；另有独立的 `codex-phase2` 计划，其架构结论是 Codex 的 resume 走
`remote-daemon-proto` 的 `--resolve` RPC，完全不经过 `LaunchPlan`/`ccm` 管线——agent 轴未来
真正的落点很可能根本不在 `LaunchDimension` 这个接口体系里，现在塞进去是给自己挖了一个将来要
迁移出去的坑）。

**R12 风险登记状态**：本轮决策**不是**"root cause fixed"，而是"open → accepted with
documented rationale"——三条轴两种机制的不对称依然存在，但现在有据可查，不再是每次重新审视
的开放问题。MASTERPLAN §6 R12 行照此措辞。

**给 UI 层"枚举可用修饰"的启示**：即便 `account`/`model` 已注册进 `LAUNCH_DIMENSIONS`，
`LaunchDimension` 接口本身也从未回答过"这个维度当前有哪些可选值"——`ACCOUNT_DIMENSION`
能在 UI 上显示成列表，靠的是 `src/accounts.ts::fetchAccounts`/`isSelectable` 现查，不是遍历
`LAUNCH_DIMENSIONS`。F09 的 `src/launch-menu.ts` 因此是一个独立于 `LaunchDimension`
的新发现层，account 组手写调 `fetchAccounts`/`selectableAccounts`——这不是"该注册就注册"没做完，
是这条轴本来就该用另一种方式回答"有哪些可选值"这个问题。

**R05 更新（2026-07-28）**：本段原写「account 组手写调 `fetchAccounts`，**container 组手写两个
硬编码值**——两者形式不同」。那个对比现在不成立了：`enumerateModifierGroups` 已改名
`enumerateAccountModifiers`，**container 组已作为死代码删除**（全仓唯一生产调用点从不读它，
第二参恒传 `"tmux"`，`"none"` 分支只被测试驱动过）。容器那两项的 UI 渲染现在住在
`tabs.ts::containerLeaves`，是全仓唯一来源。
**论证本身不受影响、反而更强**：container 轴的可选值本就固定为两个字面量、不需要"现查"，
所以它根本不需要一个发现层——这恰恰印证了本节的结论（两条轴该用不同方式回答，
而 container 那条的"方式"简单到不配拥有一个函数）。

## 39. `WrapSpec` 是纯数据 `{ id, order, prelude }`，不是闭包——且 rbind 走不走 wrap 这件事必须先定（R04④ / unify-launch）

**背景**：`LaunchPlan.wrap` 表达 `( <prelude>; exec <inner> )` 这类**包裹**（不是片段追加——
扁平字符串没有闭括号槽位，审计 C1 三方独立指出；`exec` 不可省，wrapper 用 `$BASHPID` 读
`sessions/$cpid.json`，不 exec 则 PID 对不上）。F03 起它就是空数组，结构留给 F04 的 rbind。

**铁律一：`WrapSpec` 只能是纯数据。** 不得回退成 `wrap: (inner) => string` 闭包。三条理由：
① 闭包让 `LaunchPlan` 不可序列化、不可结构比较——黄金串测试只能断言"渲染出来的字符串"，
无法断言"这个 plan 的 wrap 意图是什么"，也就无法对拍；
② 闭包能做任意事，等于在 IR 里开一个"绕过渲染器自己拼字符串"的后门，
与 `launch-plan.ts` 头注"绝不拼字符串——字符串化是渲染器的事"直接冲突；
③ 折叠逻辑（`( prelude; exec inner )`、`order` = 嵌套深度）属于渲染器职责，本就不该住在 IR 里。

R04④ 之所以**现在**做：`plan.wrap` 今天恒为 `[]`（全仓唯一赋值点是 `buildLaunchPlan` 的
`wrap: []`，零生产者），改造成本为零；等 rbind 真落进来就不是零了。

**铁律二：`prelude` 单字段是刻意收窄，不是能力不足。** 它只能表达
`( <prelude>; exec <inner> )` 这一种形态——这正好是已知的唯一用例（rbind）。
**若将来出现表达不了的包裹形态，那是"该重新设计这个契约"的信号，不是"该把闭包加回来"的理由。**

**开放问题（R04④ 顺带暴露，必须在 rbind 落地前定）**：`__ccm_rbind` 到底走不走 `plan.wrap`？
今天这是**悬空设计**——`wrap` 为它预留了结构，但 rbind 实际并未使用它：
- **CLI 路径**不需要：`shared/ccm` 内部自己负责 rbind（`ccm` 是最终 exec 的那一层），
  IR 的 `wrap` 对它完全无效。
- **兜底路径**理论上需要，但今天兜底渲染器也没有产出任何 wrap；
  身份是靠 `session-backend.ts` 直写 `@ccm_sid` + poller 回填达成的（见本文档 §33 上方与
  `sftp.rs` 里 R09 那段关于"两个写者"的记录）。

→ 结论：`wrap` 目前是**为一个尚未发生的需求预留的结构**。保留它（成本已经付过、且纯数据后
几乎为零），但**任何人要用它之前**必须先回答"这条路径的身份/setup 到底该由 `ccm` 负责还是由
IR 的 wrap 负责"，别两边都做（那会 rbind 两次）。

## 修改本文档

加新的不变量时：

1. 加到本文档对应位置 + 编号
2. 在 `src/` 或 `src-tauri/` 对应模块的 doc comment 里加引用 `// 违反此约束见 doc/INVARIANTS.md § N`
3. 如果不变量需要 grep checklist（如 State 注册），加到 [CONTRIBUTING.md](CONTRIBUTING.md) 对应 checklist

删除某条不变量（极少）：

1. 写 RFC 解释**新的约束**是什么、为什么旧的可以松动
2. PR 描述里链到这条 RFC + 全代码库 grep 受影响处确认全修

---

## 40. 「本地」= 不走 ssh 的远端 —— 一条路径，transport 是它唯一的差异（用户 2026-07-29 拍板）

**用户原话**：「我的目的就是把本地当成不走 ssh 的远端。**后面都要这么搞。**」

**这条是方向性约束，不是某个功能的实现细节**，所以住在 INVARIANTS 而不是某个工作区的
MASTERPLAN 里。凡新增「起一个会话」的能力，一律先问：**它能不能只是远端那条路少一跳 ssh？**
能，就不许另起一套。

### 为什么

`MASTERPLAN §0` 立项时的病灶是「『起一个会话』被写死成 15 套实现」。R/B/P 三段把**远端**那条路
收成了一条（6 个 executor → `planXxx` → `renderLaunchCommand` → 双渲染器），
但**本地那条路完全在 IR 之外**：

- `src-tauri/src/history.rs:930` `build_local_ps_command` **不引用任何 IR 类型**
  （`grep -n "LaunchPlan\|launch_plan" src-tauri/src/history.rs` 为空）
- `planLocal` 在生产代码里**零调用点**（`src/launch-requests.ts:139` 只剩一句注释记录 R07 删掉了
  那次 `buildLaunchPlan` 调用；R07 的理由「接了也拿不到新东西」在当时成立）
- 而 `unify-launch/MASTERPLAN.md` 曾把 F06「本地路径并入 IR」标为**完成**
  —— 2026-07-29 Phase G 文档-代码交叉对比证伪并已订正：F06 真正交付的是
  「两套 PowerShell 拼装收成一个函数」，**那不叫并入 IR**

R07 当时的判断在**只有 Windows 本地**的前提下是对的：本地就是 PowerShell + `wt.exe`，
跟远端的 ssh + tmux + `ccm` 没有可复用面。**Linux 支持推翻了这个前提**——
Linux 本地是 POSIX + tmux + `ccm`，跟远端那条路**只差一跳 ssh**。

### 怎么做

**类型已经在了**：`src/launch-plan.ts:97,158` 的 `transport: {kind:"local"} | {kind:"ssh"}`
是一个零 payload 标记（`origin` 不进 transport，见该文件头注第 14 行）。
两个渲染器已经在按它分支。所以这条约束的落地不是新建抽象，是**让 `{kind:"local"}` 真正有含义**：

| | transport | 载荷怎么送到 |
|---|---|---|
| 远端 | `{kind:"ssh"}` | `bash -lic '<payload>'` 经 ssh（`launch.rs:133`） |
| **POSIX 本地** | `{kind:"local"}` | **同一个 payload，本地 exec，不经 ssh** |
| Windows 本地 | `{kind:"local"}` | PowerShell + `wt.exe`（**唯一的例外，见下**） |

`shared/ccm` 在两边都跑得动，它已经是「一个动作 + 若干正交修饰」，
所以 POSIX 本地不需要新的启动器、不需要新的账号注入、不需要新的身份回填。

### 例外，以及例外必须显式

**Windows 本地那两套 PowerShell 是唯一被允许的例外**，理由是那台机器上没有 tmux、
启动器是 `wt.exe`、profile 是 `$PROFILE`。它必须：

1. **在类型上是一个显式分支**，不是「IR 管不到的地方」；
2. **不允许再长出第三套**。任何「本地要做点什么」的新需求，默认答案是走 `{kind:"local"}`
   + POSIX 那条路；要走 PowerShell 分支，得写下为什么这台机器上做不到。

### 与既有约束的关系

- **不改 `shared/ccm` 本体**：它在 POSIX 本地上已经够用（`--tmux`/`--detach`/`--account`/
  `--model`/预信任/身份回填全套）。
- **daemon 零改**：本条只涉及启动路径，不涉及会话监视。
- **不新增轮询**。
- 这条约束同时是「Linux 平台」与「aterm 联调」的共同地基：aterm 也是 POSIX + tmux + `ccm`，
  它消费的应该是同一条路的同一个契约，而不是第三套。

### §40 追加（用户 2026-07-29）：本地的功能要和远端一致

**用户原话**：「我们的原则是本地的功能要和远程功能一致（**虽然现在远程是重点**）。」

§40 主体讲的是**路径统一**（一条路 + transport 差异）。这一条追加讲的是**功能面统一**：
**不允许存在「只有远端才有」的能力**，除非它在本地天然没有意义（下方有白名单）。

**这条原则的两种用法，分开记，别混**：

1. **对新功能是硬约束**：新增一条「起会话 / 看会话 / 管配置」的能力，
   **必须同时落在本地与远端**，或者**显式登记为白名单例外并写下理由**。
   不许出现「先做远端，本地以后再说」而没有登记——那正是断链的形状
   （`BACKLOG.md` 头注记的 U6→U8 就是这么丢的）。
2. **对既有缺口是方向而非阻塞**：「现在远程是重点」这句是用户给的排期授权。
   既有缺口**逐条登记 + 定优先级**，按节奏还，不要求一次补齐。

**已核实的平价缺口（2026-07-29，非穷尽——完整盘点见 `local-as-remote` 工作区 L5）**

| 能力 | 远端 | 本地 | 性质 |
|---|---|---|---|
| **多账号**（列表 / 切号 / 按会话切号 / 用量） | 有 | **无** | **最大的一处欠账。** `accounts.rs:1` 自陈「**远端**多账号（cc-acct-iso）的**只读**查询命令」；`acct_iso_deploy.rs` 走 `connect_sftp`/`RemoteConfig`，**只往远端部署**。根因是 cc-acct-iso 是 bash |
| **per-account 默认模型**（`MODEL_DIMENSION`） | 有 | **无** | 依赖账号 ⇒ 随上一条 |
| **嵌套 env 清理**（`unset-nested-env`） | 有 | **无** | `history.rs:930 build_local_ps_command` **不注入任何 env**。**这一条不依赖账号**，可单独还 |
| **`ccm` 全套修饰**（`--tmux`/`--detach`/`--tmux-size`/预信任/身份回填） | 有 | **无** | Windows 本地没有 tmux 也没有 `ccm`。**POSIX 本地（§40 主体）落地后自动就有** |
| **配置面审计页对远端的实况** | **无**（7/10 行恒返回「未确定」，本页明写不连 SSH） | 有 | **反向缺口**——本地能答、远端答不出。说明这条原则是**双向**的 |
| 远端 hooks 诊断读死 `$HOME/.claude/settings.json` | 不认 `CLAUDE_CONFIG_DIR` | 认（B04 已修） | 同型反向漂移，BACKLOG **E17** |
| 远端 profile 的 fail-safe 读取 | Phase G 已补齐 | 本机侧原本就有（v1.7.9 修法） | 已对齐 |

**天然不对称白名单（不是欠账，不必补）**

- **SFTP 文件面板** —— 本地有操作系统的文件管理器，不需要它
- **端口转发管理台** —— 本地没有「转发到自己」这个需求
- **daemon 部署 / 版本协商** —— 本地会话由 `watcher.rs` 直接读 jsonl，不需要 daemon
- **多地址故障切换（happy-eyeballs 竞速）** —— 本地没有地址

**机制（让这条原则有牙，不只是一句好话）**

单靠人记不住。落地要求一条**钉死的对账表 + 计数自检**，形状照 `config_surface.rs` 的
`every_host_declaration_is_pinned`（T02 建立）：**枚举全部 Tauri 命令，每条要么两侧都有、
要么在白名单表里且带理由；新增命令不登记就红。** 具体做法归 `local-as-remote` 工作区 **L5**。

## 41. daemon 的判活信号全部由内核事件驱动 —— 四路事件、零定时器、四个盲区如实分类（zero-poll-liveness P0-P7）

用户 2026-07-29 原话「我要把轮询杀掉」。daemon 里原有 **A/B 两条**轮询（2s 判活 tick + 8s
`tmux ls` tick），**两条都已删除**，生产段现在零定时器（`no_timer_guard.rs` 钉住，见下）。

### 41.1 四路事件（延迟均为真机实测，标明测法）

| 场景 | 事件源 | 实测延迟 | 谁发 |
|---|---|---|---|
| claude 进程退出 / 被强杀 | pidfile inotify **+ `pidfd`**（绑进程实例本身） | **~18ms**（P2 端到端） | `WatchEvent::PidDied` |
| 杀掉某 origin **仅剩的**会话（server 随之退出） | tmux server 的 `pidfd` | **27ms**；跨 cgroup 整锅 SIGKILL **30ms** | `WatchEvent::TmuxServerGone` |
| server 复活 | socket **所在目录**的 inotify `IN_CREATE` | **153ms**（含 `DEBOUNCE_MS` 100ms） | `WatchEvent::TmuxObserved` |
| **多个会话里杀掉其中一个** | tmux `session-created/closed/renamed[50]` hook → `--tmux-notify` → SIGUSR1 | **126ms**（**对照组：拆掉 hook = 5042ms**） | `WatchEvent::Poke` |

四路全部汇进 `watcher.rs` 的**同一个 mpsc channel**（`WatchEvent`），`watch_loop` 阻塞在
**无超时** `recv()` 上。

**第五个触发条件（S0，2026-07-31 补）**：pidfile 里绑的 **sid 变了**（新增 / 消失 / 原地换）
⇒ 顺手起一次 tmux 重探。它不是新的事件**源**（复用同一条 pidfile inotify），而是
「快照该刷新了」的一个额外时机 —— 因为 `tmux ls` 里的 `@ccm_sid` 列此刻已经过期。
**触发条件必须是「sid 真的变了」，不是「收到了 .json 事件」**：CC 每次状态转换都重写
pidfile（远端红绿灯就靠它），拿后者当触发器等于把探测变成变相轮询。
由 `tmux_reprobe_triggers_on_sid_drift_not_on_every_json_event` 钉住。

> **数字的诚实边界**：最后一行 126ms 取自 P5 §6.3 的对照实验（有对照组，故取它作对外数字）；
> P5 §4 另一套测法对同一场景测到 **18ms**。两个数都如实留档，差异在测法不是行为。
> CI 里 `graylight-daemon-frames` 报的那个毫秒数是**上界**（`wait_line` 0.5s 轮询 ⇒ 粒度 500ms），
> 它是数量级判据（阈值 5s），**不是性能基线**。

### 41.2 一条正确性改进（不只是延迟改进）

PID 死亡判定从「pid 存在 + procStart 匹配」的**启发式**升级为 `pidfd` 绑进程实例本身
⇒ **PID 复用问题在机制上不存在**（不是「检测得更准」，是「无从发生」）。
同理 `--tmux-notify` 收到后先核 `/proc/<pid>/stat` 的 starttime 再 `kill`，starttime 不符即静默
no-op（真机反向实测：写错 starttime 时探针存活，不误伤无关进程）。

### 41.3 四个盲区，如实分类（标题原写「三个」，表里一直是四行 —— 2026-08-01 订正）

| # | 盲区 | 处置 |
|---|---|---|
| ① | tmux server 复活后 hook 丢失（hook 活在 server 内存里） | **本工作区解决**：socket 目录 inotify ⇒ 「server 起来了」本身是事件，`ServerState::Alive(pid)` 臂里重装 hook |
| ② | 会话**活着但卡死** | **明确不做**，且要说清：**今天的轮询也没在做这个** —— 卡死的 CC 在 `tmux ls` 里照样在，8s 轮询只检「会话不在了」。⇒ **删轮询在这一格上零损失**，不是拿盲区换延迟 |
| ③ | user manager / 整台机器挂掉 | 机器内部无解，靠 monitor **断连自愈**（既有路径：重连后 daemon 重发 added + 重放行 → un-archive） |
| ④ | **已存在的会话上 `@ccm_sid` 变了**（`/branch`、`/clear`：claude 进程不重启、tmux 会话不动，只是换了个 sid） | **S0 解决，但换了条路**：这个盲区**四路事件一个都不响** —— 会话没建、没关、没改名，socket 没动，进程没死。P5 删掉 8s ticker 之前，是那个 ticker 在兜它；删掉之后它变成**永久**盲区，表现为用户实测的「/branch 后原 tab 永久灰点、杀不掉」。**修法不是补一条事件路径**（`@ccm_sid` 由 `shared/ccm` 的 1 秒 poller 回填，任何"变化后立刻探"都会撞进那 1s 窗口），而是**让 monitor 不再需要这份快照**：daemon 在 `session_removed` 帧上带 `cause=superseded` 明说"这个 sid 是被顶替不是死了"，monitor 直接归档。见 §24bis |

**另有一条既有盲区（不是本区引入）**：`notify-debouncer-mini` 静默吞掉 inotify 队列溢出
（`add_event` 只读 `event.paths`，而溢出事件 `paths` 为空）⇒ 溢出时事件永久丢失。
两端都中招，登记为 BACKLOG **E39**。**`pidfd` 对溢出免疫**（内核直通）⇒ 本工作区让情况变好。
**绝不为它补定时器** —— 那等于零轮询造假。

### 41.4 红线：绝不为了让守卫变绿而删掉唯一信号源

`no_timer_guard.rs`（daemon crate，全在 `#[cfg(test)]` 内）扫**全 crate 生产段**
（**2026-08-01 U-1 之前这句话是假的**，见下面第 3 条派生纪律）：

- **判据落在「周期性唤醒」，不落在「出现过 `Duration`」** —— `Duration` 有大量非定时器的正当
  用途（去抖窗口、超时上限）。禁的是 `thread::sleep` / `time::sleep` / `recv_timeout` /
  `time::interval` / `Instant::now` / `Duration::from_secs` 这类**会让线程自己醒来**的构件。
- **非定时器的 `Duration` 用途逐条登记带理由**，处数必须**恰好等于**登记表条数 ⇒
  用 `from_millis(8000)` 偷渡节拍也会红。**登记表不是豁免清单。**
- 范围**只钉 daemon 生产段**：monitor 侧有 UI 刷新、重连退避等正当周期行为，扩过去会变成噪音。

**三条派生纪律**（都是实测踩出来的）：

1. **daemon 源码的散文里不许逐字引用守卫的禁用模式** —— `readonly_guard`（铁律 I7，见 §41.6）
   与 `no_timer_guard` 都连注释一起扫，是 fail-closed 的设计。**改措辞，不许为自己方便去改红线守卫。**
   （2026-07-31 G2 又栽了一次：新写的 `fork_write.rs` 头注里列了那几个禁用函数名，
   护栏第一次跑就把这个白名单模块自己判成违规。已改措辞并在该文件里写明「别改护栏」。）
2. **删掉一个周期性信号时，光跑单元 + 集成门禁不够** —— 还要跑依赖那个节拍的 e2e。
   P5 删 8s ticker 时漏了这一步，`graylight-daemon-frames` 第 2 段（它靠 ticker 重发快照）
   静默变红，直到 P6 并 e2e 时才被撞出来；那 6 套是 **CI-only**，不在 `cargo test` / `npm test` 里。
3. **源码扫描型守卫的「剥测试段」剥法，必须同时防欠剥与过剥**（2026-08-01 U-1 新增）。
   这类守卫（`no_timer_guard` / `readonly_guard` / `build_id_guard` / `accounts_query`）
   都要先把 `#[cfg(test)]` 段剥掉再扫。**剥法本身就是护栏的一部分，剥错等于护栏瞎掉，而且是静默的。**
   两个方向各栽过一次：
   - **欠剥**（剥少了）⇒ 守卫开始扫测试代码 ⇒ 被夹具里的字符串打红，然后有人「顺手」放宽守卫。
     防法：剥完断言残留的 `#[test]` 属性数**为 0**（`guard_support::assert_no_test_code`）。
   - **过剥**（剥多了）⇒ 生产代码被当测试段吞掉 ⇒ **全绿，且看不出来**。这条更危险。
     实际形态：旧剥法锚点是 `\n#[cfg(test)]\nmod tests`、取它**之前**的全部 —— 于是
     ① 名字不叫 `tests` 的测试模块永不被剥；② 测试模块**之后**的生产代码被整段丢弃。
     `main.rs` 的 `mod stream_flag_tests` 恰在文件中部（182–247），真正的子命令分发在 275–291
     ⇒ **`no_timer_guard` 一族从来没扫过那段分发**。
     `readonly_guard` 就这**两种形态**而言没有洞 —— 它 2025 年起用按括号配平逐块剥，
     注释里白纸黑字写着「不能简单从首个 `#[cfg(test)]` 截断到 EOF，`main.rs` 的测试模块在文件中部」。
     **同一个坑，同一个 crate 里有人已经填过，另外三处没跟。** 这与递归那条是同一个病：
     `readonly_guard::scan` 早已递归且留了警示注释，`no_timer_guard` 也没跟。
     ⇒ **护栏的公共机件必须收敛**（剥法现为 `guard_support.rs`；**遍历尚未收敛** ——
     daemon crate 里今天仍有 5 份独立目录遍历，monitor 侧还有 1 份，登记为待办），
     否则「已修好的教训」会在隔壁文件里原样复发。
     修这条时我又当场制造了同一类洞：新加的 `#[cfg(test)] mod guard_support;` 是**无花括号体的
     声明**，锚点照样匹配，收尾的列 0 右大括号一路找到 179 行某函数的收尾，把 `main.rs:26–179`
     （含 `const BUILD_ID`、`CAPABILITIES`、`EMITS`）整段吞掉。
     防法三件套：① 匹配到锚点后**必须确认那一行以左大括号收尾**（否则是模块声明，原样保留）；
     ② 逐个剥每个 `#[cfg(test)] mod`，不是「取第一个之前的全部」；
     ③ 用**扫到的字节总量下限** + **文件数与独立目录遍历相等**双判据钉住扫描面
     —— 字节地板挡「代码搬进子目录只剩壳」，数量相等挡「单个文件被剥空」，两条缺一不可。

   **`readonly_guard` 另有两条同类的洞，Phase E 工程审计逮出、当轮修掉（都是 fail-open）：**
   - **锚点没钉行首** ⇒ **注释里**逐字写出 `#[cfg(test)]` 也会起跳。`main.rs:23` 的行尾注释
     正是这个形状，起跳后括号配平一路吃到 `:40` 的 `use tokio::io::{…}` 收尾 ⇒
     **`main.rs:23–40`（15 条 `mod` 声明 + 2 条 `use`）从来不在它的扫描面里**。
     注意这与本节第 1 条纪律**方向相反**：对 `readonly_guard` 而言，注释里出现这个属性不是
     fail-closed 而是 fail-open。
   - **无花括号体的声明会吃掉后文** ⇒ 与上面 `guard_support` 那条是同一个 bug，
     而触发它的正是 U-1 新加的 `#[cfg(test)] mod guard_support;`（洞从 429 B 撑到 497 B）。

   修完扫描面 **217_853 → 221_928 字节**（+4_075）。判定证据：在原先被吞掉的 `main.rs:23–40`
   区间放一处 `fs::write` 探针 —— 旧剥法**看不见（假绿）**，新剥法 **RC=101**。
   同时把欠剥方向也钉成机器判据（`no_test_code_leaks_into_any_production_section`）：
   剥完全 crate 不许残留 `#[test]`。**它第一次跑就咬到了我自己** —— `guard_support.rs` 注释里
   一个孤立的右大括号让配平提前收尾，5 个 `#[test]` 静默留在「生产段」里。
   按第 1 条纪律处置：**改注释措辞，不改护栏**。
   **验证方式只有一种算数：把违规代码分别放进「应被扫到」和「应被剥掉」两个位置，看红绿是否相反。**
   仅仅「守卫跑绿」不构成任何证据。

### 41.6 daemon 的写盘边界（铁律 I7，2026-07-31 G2 起收窄）

**原措辞**：「daemon 对被观测文件系统必须只读，绝不写。」

**现措辞**：**daemon 不许改动用户既有数据；新增文件须 `O_EXCL` 且限于白名单模块。**

**为什么改**：那条铁律的真实意图从来不是「不许碰文件系统」，而是「不许改动既有数据」。
此前 daemon 一个字都不用写，于是用「全面禁写」近似它 —— 够用且实现简单。
`--fork-session`（远端分叉，几十 MB 的 jsonl 不该为了分叉拉过 ssh）要的能力恰好落在
这个近似的**误差**里：用 `O_EXCL` 新建一个此前不存在的文件，不修改、不覆盖、不删除任何既有文件。

**护栏因此分两层，整体比原来更强**（`readonly_guard.rs`）：

| 层 | 范围 | 判据 |
|---|---|---|
| 默认层 | 除白名单外**所有** daemon 生产源码 | 原来那 11 条写模式一条都不许出现（未变） |
| 白名单层 | **恰好一个**模块，按**仓库相对路径**匹配：`control/fork_write.rs` | **必须**含 `.create_new(true)`（带前导点，见下）；**不得**出现删除 / 改名 / 复制 / 硬链软链 / 截断 / 追加 / 覆盖写 / 建目录 / `set_len` / `.create(true)` |

「恰好一个」是断言值，不是描述 —— 多一个 = 写盘能力扩散，零个 = 白名单模块被改名而护栏没跟上。

> **U3（2026-08-01）：匹配从「裸文件名」改成「仓库相对路径」，起因是一次「该红没红」。**
>
> U3 把 `fork_write.rs` 从 `src/` 搬进 `src/control/`。功能计划**预言**这会让「恰好一个」当场红
> （逼出 control 侧护栏）—— **结果它一声不吭**，因为匹配用的是 `path.file_name()`，
> 文件名没变，护栏对整个分层重组毫无察觉。
>
> **「没红」在这里不是好消息，是缺陷的证据**：同样的逻辑意味着**将来任何目录下的
> `fork_write.rs` 都会被当白名单放行**。而白名单层比默认层**松**（它允许 `O_EXCL` 新建），
> 放行错文件 = 给写盘能力开一个没人知道的第二个洞 —— 正是本节第 1 条承诺要杜绝的那件事。
>
> 改成路径之后两个变异都咬：① 别的目录下放一个同名 `fork_write.rs` ⇒ 被**默认层**（更严那层）
> 抓住；② 常量还指旧路径（= 文件搬家护栏没跟）⇒ 真 `fork_write` 也被默认层抓住。
>
> ⚠ **「恰好一个」这条断言现在两个分支都很难摸到**（Phase D 审计实测）：路径唯一 ⇒「多一个」
> 构造上不可能；「零个」在真实改名场景下会被默认层抢先 panic。它仍有价值 —— 它是**唯一**
> 兜得住「写盘模块整个跑到 `src/` 外面」（`#[path="../.."]`）的判据。

**为什么原来更弱**：原护栏对「daemon 将来要写盘」没有任何设计，一旦有人要写就只能**整条删掉**。
现在写的能力被钉死在一个可审计的洞里，洞口还额外挡住了截断 / 追加 / 改名 / 删除。

**必需 token 带前导点是有意的**：护栏是子串扫描、**不剥注释**。只要求裸 `create_new(true)` 的话，
模块文档里那句「`create_new(true)` = O_EXCL」就能把要求喂饱 —— 实测过（G2 的 N5 变异）：
把代码换成 `.create(true)` 之后那条要求**照样通过**，只有行为测试红。带上点就只能由**调用**满足。

### 41.5 兼容与部署

- **wire 两处 additive，`PROTO_VERSION` 不 bump**：`TmuxSessions` 加 `observation`
  （有会话时**省略** ⇒ `raw` 载荷逐字节不变）；新帧 `TmuxSessionClosed { name }` 进 `EMITS`。
  旧 monitor 遇未知 kind 走 `warn` 后跳过（`unknown_kind_returns_none` 钉住），行为退回
  「快照 + miss 计数」。
- **`BUILD_ID` 必须 bump**（现 `p1r-event-liveness`）：不 bump ⇒ 旧 daemon 报同一个 id
  ⇒ 不被判 stale ⇒ 不自动重装 ⇒ **整轮改动在已部署的远端休眠**。
  单一事实源：`build.rs::emit_daemon_build_id` 从 daemon 源码抠出，emit 成 monitor 的
  `EXPECTED_DAEMON_BUILD_ID`。
- **死亡帧只带 name、不带 sid**：`#{@ccm_sid}` 在 hook 上下文会解析到**别的会话**
  （P0 实测；照直觉写会把活着的会话变灰）。name→sid 的映射 monitor 本来就有
  （最新那份 `tmux ls` 原文）⇒ **让知道的人去查，比让不知道的人硬传更稳。**
- **`RETIRE_MISS_THRESHOLD >= 2` 与快照对账路径一字未动** —— 死亡帧是**绕过** miss 计数的
  快路径，不是替换兜底。查不到 sid 时（never-bound / 快照还没到）**不猜**，交回兜底路。
