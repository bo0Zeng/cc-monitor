# 全局不变量

跨模块约束清单。违反任一条都是 bug，code review 时应该被指出。每条写：性质 · 为什么不能松动 · 谁在守（判据名）。
已由判据钉住的条目只写一句规矩加判据名，细节看判据头注；沿革不写在这里（看 git 历史）。节号被代码与判据头注引用，不改号；删掉的条目留一行「已删，理由是…」。

---

## 1. monitor 零侵入 Claude Code 数据源

后端对 `<claude_dir>/projects/**/*.jsonl` 与 `<claude_dir>/sessions/<PID>.json` **只读**；monitor 进程**一个字节都不直接写用户文件**。写用户的文件只经那台机器后端的文件管理面（本机也算），后端没连上 ⇒ 明确报错、不回落。

**例外（穷举，都是用户显式动作）**：

1. **删历史会话**：帧命令 `files-delete-session`，只收 sid；落点由后端按 sid 在自己的记录树里找（`agents::claudecode::paths::session_file_for_delete`：解到底必须恰是 `projects/<项目>/<sid>.jsonl`，链接出界不跟），删前二次确认。
2. **后端自部署**：写那台的 `~/.cc-monitor/bin/`（非用户数据、幂等、按 `deploy-plan` 只升不降），绝不碰 `<claude_dir>`。
3. 已并进第 1 条（远端删历史也是 `files-delete-session`）。
4. **别名块与别名文件**：rc / `$PROFILE` 里的 BEGIN / END 块（`aliases_block_install` / `aliases_block_remove`，本机远端同一条），经 `files-put`（§4）。
5. **项目 MCP 配置**：帧命令 `mcp-server-put` / `mcp-server-remove`，写面只有项目 `.mcp.json`（落点必须是那台上的绝对项目目录，不含 `..`），绝不写 `~/.claude.json` / `settings.json`。
6. **公钥推送**：帧命令 `pubkey-push`，只追加到远端 `~/.ssh/authorized_keys`（幂等去重）。那台后端不在时走一次 SSH exec：这是建立信任那一跳，按构造发生在后端可达之前，是「用户文件只经后端写」唯一的 monitor 侧远端例外。
7. **装 cc-bus skill**：往 `<claude_dir>/skills/cc-bus/` 写（`src/backend/assets/cc_bus_install.rs`），四个配套一条不许省：只由扩展页确认卡触发 · realpath 白名单（解到底仍在 claude 目录底下）· 逐文件比、一致就不写 · 覆盖前整目录改名成 `cc-bus.bak-<秒>`。

**分叉不是例外**：从某一轮分叉（帧命令 `session-fork`，本体 `control/fork_write.rs`）只新增一份 `<new-sid>.jsonl`：`create_new(true)`（`O_EXCL`）、只收 sid、原会话零改动。后端写盘只许发生在登记过的模块里（§41.6）。

**起进程不算后端写**：后端起 `ccm` / `claude` 之后由被起的程序写它自己的文件；后端的责任是不替用户决定往它的配置里塞什么。起进程的面逐条登记（`readonly_guard.rs` 的 `ALLOWED`）。`ccm` 不写 agent 的信任项，也不往会话里按键去答信任框。

**文件管理器不在本约管辖内**：原生文件窗口经后端文件管理写面改用户浏览到的任意文件（会话文件也能改），每次写都是一次直接的用户手势，没有自动 / 后台写。

**多账号的读面**（`src/backend/observe/accounts_query.rs`，零写入、不 shell out）：账号清单 manifest 整份只读、`configDir` 逐条过白名单；`/proc/<pid>/environ` 只抠写死的两个键（`CLAUDE_CONFIG_DIR` · `ANTHROPIC_BASE_URL`，后者只折成「走不走本机中转」一个布尔）；`<configDir>/.claude.json` 只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔，且 `configDir` 必须逐字是清单里的一个；`.credentials.json` 只 stat。`sessions/` 必须留在账号库的共享集里（判活靠它）。账号库的改动住 `accounts/manage/`，每次先备份。

**发按键只进本工具建的 tmux 会话**：会话名由后端派生（`control/ccm/plan.rs`），`control/gate_rules.rs::is_ccm_tmux_name` 白名单门控送键；kill 只在精确命中 `@ccm_sid` 时做（§34）。

**为什么不能松动**：核心价值是「看 claude 的输出不破坏它」。用户对「数据源就是我自己的命令痕迹」的信任一旦破了就回不来；写只能是显式的、可见的（足迹页逐条列出）。

**谁在守**：`write_site_registry_tests.rs::every_monitor_write_site_lands_outside_the_users_files`（monitor 写盘落点闭集，没有「用户文件」一档）· `write_site_registry_tests.rs::the_files_that_used_to_write_users_files_write_nothing_now` · `remote_write_registry_tests.rs::every_remaining_sftp_write_lands_outside_the_users_files` · `readonly_guard.rs::every_process_spawn_in_production_is_registered` · `cc_bus_install_tests.rs::a_symlinked_skills_dir_is_refused`。

---

## 2. monitor 自己的 data dir 永远是 `~/.cc-monitor/`

不跟随界面上改的 `claudeDir` 漂移。一台机器一个家：与后端同一个目录，只给本人（0700）。

**唯一的明文例外 `CCM_DATA_DIR`**（`config.rs::DATA_DIR_ENV`）：只为把整个进程挪到别处跑（自动化测试、一次性复算）。只认绝对路径；空串 == 没设；给了但不合法 ⇒ `None`，**不退回用户真家目录**；它挪的是整个 data dir；默认住址只在 `creds_core::store::monitor_data_dir` 拼，monitor 全树只经 `config.rs::resolve_monitor_data_dir` 派生。常驻后端的套接字也按家算（`relay_route_core::listen_socket_for`），隔离跑的 monitor 有它自己的常驻后端；一个家只有一个常驻后端攥得住 `run/` 的锁。

**为什么不能松动**：读配置不能先解析 `claudeDir`（填错就再也打不开设置）；换 Claude 目录后偏好不丢；跨进程文件位置稳定。

**谁在守**：`paths_tests.rs` 一族（`with_nothing_set_it_is_the_documented_default` · `a_relative_override_refuses_instead_of_quietly_using_the_real_profile` · `nothing_else_in_the_monitor_tree_builds_that_path_itself` · `the_data_dir_is_spelled_in_one_place` …）。

### 2.1 真相 vs 缓存必须分得清（F65 / issue #58 单向门④）

| 文件 | 类 | 写它的 | 说明 |
|---|---|---|---|
| `config.json` | **真相** | `config.rs` | 主题 · 字体 · `claudeDir` · 快捷键 · 机器表 · 诊断开关 —— 全是用户手填 |
| `history-metadata.json` | **真相** | 本机常驻后端 `history_annotations.rs::answer_annotate` | 按 sid 的星标 · 改名 · 隐藏 |
| `filewin-bookmarks.json` | **真相** | `filewin/src/bookmarks.rs`（文件窗口进程，旁件 `.lock` 上独占锁读改写） | 每台机器一份收藏目录 |
| `filewin-view.json` | **真相** | `filewin/src/workspace.rs`（整份原子换，后写者赢） | 文件窗口的整窗缩放 |
| `auto-launch.json` | **混（良性）** | `auto_launch.rs` | `enabled` 是真相；`monitor_exe_path` 每次启动自愈改写 |
| `ps-registry/` `ps-await/` | **缓存/IPC** | `bind.rs` | Linux bash / zsh 接入块留的终端记录与认出来的窗口，启动重扫 |
| `logs/` | **缓存/派生** | `logging.rs` · 本机常驻后端 | `monitor/` 按天滚动、留 3 天（§15）；`backend/` 是脱离运行的本机后端 stderr |
| `bin/` `staging/` `logs/backend/` `assets-catalog.json` `last-seen.json` `launch-pending/` `known_hosts` `profiles-written.json` | **缓存** | 本机后端 | 能重建的：程序 · 上传暂存区 · 错误输出 · 资产目录 · 离线那台的上次值 · 起会话便条 · 主机钥匙 · 上次写出的配置指纹 |
| `relay-key` `run/` `backend.json` `profiles.toml` `profiles-migrated.json` `aliases.sh` `aliases.ps1` `skill-installs.json` `chores.json` `plan-review.json` `backups/` `accounts/` `accounts-mcp.json` `apikey-credentials.json` `quota.json` `rotation.json` `lineage.json` `launch-accounts.json` | **真相** | 本机后端 | 删了会丢的：中转根钥匙 · 常驻后端的套接字与进程记录 · 退出行为 · 别名配置 · 装记录 · 「待办」里的选择 · 计划需手动的认可 · 卸外来资产前的备份 · 账号库 · API key · 各号最近的用量与轮换设置 · 会话血缘 · 每条会话上次用的号。名字取契约常量（`relay_route_core` · `creds_core::store`），`data_paths.rs::backend_entries` 列它们 |

- **真相** = 用户手写 / 意图，删了丢东西、要备份、要迁移友好；**缓存/派生** = 能从别处重建，随便删。
- **新增任何 data dir 文件，必须在 `data_paths.rs` 的枚举里声明它是哪类**：`DataPathInfo.class: DataClass` 非可选，设置页「数据位置」每行据它显示「删了会丢 / 可随手删」。每一项的类与上表两向相等（`data_paths_tests.rs`，异源判据）。

---

## 3. 所有跨进程 JSON 文件 = UTF-8 无 BOM

写入端 Rust 直接写 UTF-8；PowerShell 用 `[System.IO.File]::WriteAllText` ＋ `UTF8Encoding($false)`（禁用 PS 5.1 的 `Out-File -Encoding utf8`，它写 BOM）。读取端解析前剥 BOM。

**为什么不能松动**：`serde_json` 不剥 BOM 直接失败；这曾让整套 cc 集成七个版本「装上没用」。

---

## 4. profile 等用户文件写入 = 后端 `files-put`：CAS ＋ backup ＋ 原子替换 ＋ 写后校验 ＋ 回滚

用户文件（rc / `$PROFILE` / `.mcp.json` / skills 目录）只由后端写，规则只有一份：`src/backend/control/files_write.rs::put_text`（线上 `files-put`）——
① `expect` 必给（读改写之间盘上那份被改过 ⇒ `stale`、不写）· ② 与盘上逐字节相同 ⇒ 不写 · ③ 要备份 ⇒ `O_EXCL` 另存 `<名>.ccm-backup-<毫秒>-<序号>`（沿用原权限位）· ④ 替换：unix 同目录暂存旁名写满后换名上位（原子）；Windows 已在的目标**就地覆盖写**，保住 dst 的 ACL / ADS / 创建时间 · ⑤ 回读逐字节比对，不符回滚 · 盘上有字节却读到空 ⇒ 停。最后一段是链接 ⇒ 改真文件、链接留着。

**为什么不能松动**：用 `MoveFileExW` / rename 拿暂存文件覆盖，会用暂存文件继承来的 ACL 盖掉用户文件上的显式 ACE（Documents 重定向到别的盘的用户会读不了自己的 profile）——这正是从前用 `ReplaceFileW` 的理由，今天由「Windows 就地覆盖」承担；OneDrive 占位 / 杀软会让读回成空，纯写就是清空用户内容 ⇒ 备份加回读双保险。

**谁在守**：`files_write_tests.rs` 的 `put_*` 一族（含只在 Windows 上跑的 `put_keeps_explicit_acl_entries_on_windows`）· `atomic_replace_registry_tests.rs`（monitor 自己的文件与用户文件两类语义分开登记）。

---

## 5. JSONL 单一时序由 seq 字段 + RecordTimeline binary insert 共同保证（v2.6 B 重构）

后端给每一行分配 per-file 单调递增的 `seq`（从 0 重读时先发 `session_file_reread` 再从 0 数；流不重放历史，起点是当前完整行数 ⇒ seq 就是行号，与旁路快照同一个编号空间，§25a）。本机与远端同一个来源（本机会话的行也是本机后端的 `line` 帧）。所有交付路径透传 seq。前端每个 Tab / 查看器持一个 `RecordTimeline`，按 seq 二分插入 DOM。后端交格顺序、切块到达顺序、实时与成批混合，都不影响视觉顺序。seq 保时序、不保投递次数（§25）。

**为什么不能松动**：此前用五个 flag 协调「批 / 实时 / 前插」反复出相位 bug；seq ＋ 二分插入把「后端保序」变成「前端排序」，一次性消掉了那整套机制。

---

## 6. session 探活双重校验（PID + procStart）

判「同一个活进程」要 PID ＋ 启动时刻两样，住后端：`observe/watcher.rs` 宣告前比 `procStart`，`platform/pidwatch`（Linux pidfd · Windows 死亡事件）看守。读法在 `platform/`（Windows `GetProcessTimes`，Linux `/proc/<pid>/stat` 第 22 字段，从最后一个 `)` 之后数），判定两平台共用 `liveness::is_same_live_process`。`procStart` 可缺（§18），缺了只看 PID。

**为什么不能松动**：Windows 的 PID 短期复用很常见，只看进程在不在会把旧条目判成活的 ⇒ 僵尸 Tab。

---

## 7. HWND 拉前三重校验

拉前之前必须同时满足：窗口句柄仍有效 ∧ 此刻的属主 pid 等于找到它时的 ∧ 属主的进程创建时间等于找到它时的。任一不符 ⇒ 拒绝拉前并说原因。本机与远端的窗口都是点 ↗ 那一刻沿进程链现找的，过同一道校验。

**为什么不能松动**：窗口句柄复用比 PID 复用还频繁，不校验会把不相干的窗口拉到前面。

---

## 8. Tauri State 必须 `app.manage`

任何 `State<'_, X>` 参数都对应 `setup()` 里一次 `app.manage(x)`。漏了 `cargo check` 抓不住，运行时第一次调用才 panic。改 State 时照 [CONTRIBUTING.md § 3.1](CONTRIBUTING.md) 那几条 grep 找全消费者与跨线程持有者（从代码现查，不另抄清单）。

**为什么不能松动**：撤一个 State 时漏 `manage` 曾让一处功能带病五个版本。

---

## 9. 排序的硬规则：一律按 `seq`，禁止按到达顺序

§5 的派生约束，单列便于引用：禁止任何按到达顺序排序 / 拼接的代码（前端临时缓冲、按块先后假设新旧、实时与成批各维护一套顺序）。一切顺序取自 `seq`。

**为什么不能松动**：行顺序就是对话的时间顺序；按到达顺序的捷径总会在某个边界（快照 / 实时 / 切块重放）打乱它。

---

## 10. 长耗时 IO / 系统调用 = `tokio::task::spawn_blocking`

可能阻塞数十毫秒以上的同步调用（Win32 窗口枚举 / 拉前 · 文件系统扫描 · 起子进程）不许直接跑在 IPC 派发线程上：命令写成 `async`，体包进 `spawn_blocking`；async 任务里不许 `std::thread::sleep`（用 `tokio::time::sleep`）。拉前类命令前端再加超时兜底。

**为什么不能松动**：同步的 Tauri 命令跑在 IPC 派发线程上，一个慢命令阻塞期间其余 IPC 全排队，整个界面没反应。

**谁在守**：`sync_command_registry_tests.rs::every_sync_command_is_on_the_whitelist_with_a_reason` · `sync_command_registry_tests.rs::no_sync_command_waits_on_the_outside_except_the_registered_deviations`。

---

## 11. 跨平台分裂边界

平台原语与平台 cfg 只住 `platform/`（后端与壳各一份）。判据不是「cfg 出现在哪」，是跨 target 编译：后端 `cargo check --all-targets --target x86_64-pc-windows-msvc`（CI）、本地门禁 `winchk` / `winchk-backend` 两格；不带 cfg 的平台代码由 `tests/backend/platform/cfgless_guard.rs` 补盲区；平台降级分支不许凭空返回「成功」（`platform/fallback_guard.rs`）。

**为什么不能松动**：平台代码散出去之后，换平台就要全文 review 一遍；降级分支恒真曾让会话永远不被归档且毫无信号。

---

## 12. 前端 alert 不算错误反馈

关键失败必须：留日志 ＋ 状态栏红色 toast；严重或持续的错误再加顶部 banner。壳推给界面的出错只有一种事件（`ui_error.rs`，事件 `monitor-error`，界面 `backend-errors.ts` 收）；日志行不上屏。

**为什么不能松动**：alert 一闪就被关掉，用户看到了也抓不住关键信息。

---

## 13. CSS portal 元素必须真挂 body

`position: fixed` 的浮层（提示 · 弹窗 · 下拉）必须挂到 `document.body`，不挂当前组件的子树。

**为什么不能松动**：祖先有 `transform` / `filter` / `perspective` / `will-change: transform` 时，fixed 后代的包含块从视口变成那个祖先，坐标就不再是视口坐标。

---

## 14. localStorage / IndexedDB key 必须前缀 `cc-monitor.`

前端持久化到 localStorage / IndexedDB 的键一律以 `cc-monitor.` 开头，只经 `local-storage.ts` 的 `LS_KEYS` 取。主窗口、查看窗、agent 窗共享同一 origin 的存储：非主窗写共享键前必须显式隔离（例：`TabManager.persistLastActive` 在查看窗里不写）。

**为什么不能松动**：同一 origin 的存储在多个窗口与可能的别的 Tauri 应用之间共享；前缀避免撞键，也便于「数据位置」页过滤展示。

---

## 15. logging 子系统失败不能阻塞 monitor 启动

`logging::init()` 在 `tauri::Builder` 之前调用；建日志目录、构造滚动写入器任一失败 ⇒ 退化成只写 stdout，monitor 照样起。已有 subscriber（测试场景）只提示一句、不 panic。

**为什么不能松动**：日志是诊断辅助，不是核心功能；「日志写不了所以 monitor 打不开」不可接受。

---

## 16. monitor 单实例运行

同一用户同一台机器只跑一个 cc-monitor。Windows 由 `tauri-plugin-single-instance` 强制；Linux 由壳自己的 `platform/single_instance.rs`（会话总线上的名字，转交激活令牌，否则 Wayland 上第一个拉不到前台）。两者都必须是 Builder 链上第一个 plugin；第二个实例把参数交给第一个（`platform::window::raise_main` 拉回主窗口）后立即退出。

**为什么不能松动**：两个 monitor 会抢同一批跨进程文件（`auto-launch.json` · `ps-await/` · `ps-registry/` · 日志），双重渲染、认窗口互相踩。

---

## 17. 前端 IPC drain 必须捕获单条记录异常 + 用户数据遍历必须迭代

### 17a. drain 必须 try/catch 单条 record

`events.ts` 的 drain 循环每处理一条都 try/catch：单条抛错只记日志、跳过该条，异常不许逃出循环（否则后面上千条永远滞留）。

### 17b. 用户数据深度的遍历必须迭代而非递归

对记录链、Tab 历史、子运行嵌套这类深度由用户数据决定的遍历，一律用迭代（显式栈 / 拓扑序），不靠 JS 调用栈（例：`cards/diff.ts::diffLines` 用迭代 LCS 加单元预算）。

**为什么不能松动**：会话的 parent 链几乎是线性的、几千条是常态，WebView 的栈在一千到一万帧附近触底；炸栈是异常，那一条从此渲染不出来。

---

## 18. Claude Code 写的元数据文件按"宽容 schema" 反序列化

对 Claude Code 写的文件反序列化时，非核心字段一律 `#[serde(default)]` / `Option`，只有绝对必填（`sessionId` · `pid`）强制。适用于 `sessions/<PID>.json`（`procStart` 可缺）· `tasks/<sid>/*.json` · `projects/**/*.jsonl` 的 `agents/claudecode/schema.rs::JsonlRecord`。monitor 自己写的文件 schema 可以严格。

**为什么不能松动**：Claude Code 写 pidfile 偶发漏 `procStart`；字段一必填，整个会话就被静默忽略、漏一个 Tab。

### 18.1 看不懂的记录不静默丢 —— 抢救成 `Unrecognized`（F63 / issue #49）

未知 `type`、或已知 `type` 但字段解析失败而原文仍是合法 JSON ⇒ 抢救成 `JsonlRecord::Unrecognized`（留原文与链身份），不出 `parse_line` 之外丢掉；只有连 JSON 都不成立的行才回 `Err`。带链身份（`uuid` / `parentUuid`）的记录不论认不认得都进链（`chain.rs`）。各降级点各记一笔有界的账，住那台后端 `src/backend/agents/claudecode/drift.rs`（帧命令 `drift-report`）；在设置「机器 → 足迹 → 未识别的数据」查看。

**为什么不能松动**：一条记录静默消失，它的子记录就成了孤儿 ⇒ 主线判定把整支误判成「回退掉的」。

---

## 19. 跨 windows crate 版本 HWND 互操作走 `as isize`

壳直接依赖的 `windows` 与 Tauri 内部用的版本不同（`HWND.0` 一边是 `isize`、一边是 `*mut c_void`），两版本共存。跨版本传 HWND 一律 `tauri_hwnd.0 as isize` 再包成本侧的 `HWND`；不许 `transmute`，也不许强行统一两个版本。

**为什么不能松动**：字段类型不同时 `transmute` 是 UB；Tauri 锁死它的内部依赖，强行对齐会引发连锁升级。

---

## 20. 用户角色的记录要先分「谁说的」

Claude Code 把很多不是人说的东西也写成 `type=user`。判定只有一份，在适配层（Claude：`agents/claudecode/text.rs::user_text`；Codex：`agents/codex/record.rs`），随记录成品带出 `userText.speaker`；渲染 · 排队消息 · 自动切 Tab · 大纲 · 搜索 · 摘录 · 估高 · 子运行收场都读它，界面不看正文认标记。

| 来源 | 怎么认（先字段、后具名框 / 固定句） | 算人说的 |
|---|---|:---:|
| 人打的（含粘贴、采纳的建议、排队时打的） | `origin.kind = human`；认不出的都归这里 | ✓ |
| 斜杠命令 · `!` 输入 | `<command-name>` 三标签 · `<bash-input>` | ✓ |
| `!` 输出 · 本地命令输出 | `<bash-stdout>` / `<bash-stderr>` · `<local-command-stdout>` | ✗ |
| 后台任务通知 | `origin.kind = task-notification` · `<task-notification>` 框 | ✗ |
| 子 agent 来话 / 交回 · 另一个实例来话 | `origin.kind = peer` · `<agent-message>` / `<cross-session-message>` 框 | ✗ |
| （子 agent 侧）主会话派的活 · 后来发的话 | 子 agent 记录首条 · `origin.kind = coordinator` | ✗ |
| 系统注入（提醒 · 技能展开 · 续跑样板 · 定时触发 · 额度恢复续跑） | `isMeta` · `origin.kind = auto-continuation` · 五种包装剥空 | ✗ |
| 压缩摘要 | `isCompactSummary` | ✗ |
| ESC 中断标记 | 整条恰是 `[Request interrupted by user…]` | ✗ |
| 工具结果回灌 | 全是 `tool_result` 块 | ✗ |

**不许松动的三条**：① 认不出的归人（宁可漏判，不吃掉人话）；② 只认具名框（要有收尾）与固定句，不拿「以 `<` 开头」当判据；③ 字段比正文优先。

**为什么**：ESC 中断标记、子 agent 交回、后台通知都被画成过用户气泡、抢过前台。新的「用户行为感知」一律读 `userText.speaker`。

---

## 21. 启动重放滚动稳定性（贴底不抖）

`MessageStream`（`src/frontend/ui/stream.ts`）维持贴底时必须守三条：

1. **`snap()` 必须守卫**：只在落后底部超过 1px 时才写 `scrollTop`，禁止每帧无脑重钉。
2. **视口上方插入不手动补偿 `scrollTop`**，交给原生 `overflow-anchor`（禁止给 `.stream` 设 `overflow-anchor: none`；唯一豁免是上翻补批那一个同步任务内临时关、`finally` 还原：`tabs.fillAbove` 与 `session-viewer.maybeFillAbove`）。手动补偿叠加原生锚定会双重位移。
3. **重放期视口上方的旧内容不建 DOM**：没接骨架的 tab 由 `TailWindow` 收纳，`seq < floor` 的只进账本不建卡，后台 tab 空闲时、切过去时再物化尾段；大增量批的中部插入攒到批末一次挂。物化 / 补批插卡前先 `branchFolder.unwrapAll()`、插完 `rebuildNow()`。

   **3b. 接上骨架的 tab / 查看器**：已渲染集 = 尾后缀 ∪ 若干岛，「哪些 seq 没物化」以 `SkeletonView` 的占位为准；往占位里建卡只许经骨架（`fillVisible` / `ensure`）；每次只物化与视口相交的那一段；物化时钉住视口里最上面那张已渲染卡的屏幕位置；接之前对拍 seq 空间，对不上就不接；接上之后正文不驻留，滚到时按偏移向那台后端取回。

**为什么不能松动**：末块先发的重放会把旧消息逐条插到贴底视口上方，高分屏上分数像素的舍入每帧不同 ⇒ 整块上下抖。只测 `scrollTop` 发现不了（它单调增长），要测可见元素 `getBoundingClientRect().top` 的逐帧反转。

---

## 22. 独立窗口契约（viewer #10 / settings F82a）

独立窗口（查看窗 · agent 窗 · 设置窗）违反下列任一条都是**静默失败**（白屏 · 收不到 · 卡死 · 点了没反应）：

1. **建窗的命令必须 `async`**：同步命令跑在主线程，`WebviewWindowBuilder::build()` 又要派发到主线程等 ⇒ 死锁。判据 `lib_window_lifecycle_tests.rs::every_window_building_command_is_async`。
2. **定向事件 target-kind 对齐**：给单个窗口投递用 `emit_to(EventTarget::webview_window(label))` ↔ 前端 `getCurrentWebviewWindow().listen`；禁止 `&str` 目标（`AnyLabel`）配模块级 `listen`（`Any`），Tauri 按 kind 匹配、静默丢。广播（`Any` ↔ `Any`）不受影响。判据 `lib_window_lifecycle_tests.rs::every_emit_to_targets_a_webview_window_not_a_bare_label`。
3. **先注册监听，再触发会交格的订阅**：`listen()` 是异步注册，注册完之前到的事件会丢。
4. **查看窗的精简布局不许塌 grid 行**：隐藏一个 grid item 会让后面的前移落行 ⇒ 只为剩下的 item 定义行数。三窗各自的模块图与 CSS 清单由 `tests/frontend/ui/entry-graphs.vitest.ts` 钉。
5. **关窗要 `core:window:allow-close`**（`core:window:default` 只含读）。
6. **复用 `dispatcher` 的独立窗口自己调 `dispatcher.start()` ＋ `applyOverrides`**，面板作 overlay 栈底；别手搓窗口级 Esc 监听（会双触发）。
7. **接管了关窗又放行的窗口要 `core:window:allow-destroy`**。判据 `lib_window_lifecycle_tests.rs::a_window_that_lets_close_through_may_destroy_itself`。
8. **界面里调到的每个窗口写口都要在权限里**（`unminimize` · `setFocus` · `minimize` · `hide` · `setTitle` · `setFullscreen` · `setZoom` 各一格）。判据 `capability_registry_tests.rs::every_window_write_the_ui_calls_is_granted`。

---

## 23. 复用 `.stream` 容器的非-Tab 视图必须显式恢复可见

基类 `.stream` 默认 `visibility: hidden`，只有 `.stream.active` 可见（多 Tab 机制）。复用 `.stream` 样式但不归 `TabManager` 管的视图（`SessionViewer`）必须在自己的 CSS 里显式 `visibility: visible`。

**为什么不能松动**：「历史会话点进去空白」就是它：卡片全进了 DOM 却不可见，状态栏照常显示记录数。

---

## 24. 远端活跃集 `remote_active` 恒等于"前端当前应视为 live 的远端 sid"（issue #20）

monitor 里已没有 `remote_active`。会话活 / 可重连 / 已结束由那台后端的会话账本裁（`observe/session_ledger.rs::SessionLedger`，单写者 = 发帧唯一出口），成品帧 `session_state`；monitor 只留成品缓存 `session_book.rs`（本机远端同一条 `session-book-emitter` 线程转交，不裁决），自己唯一知道的事实是「到那台的连接断了」⇒ 那台的成品作废、界面说「说不清」，不许显示成「已结束」。

两条派生约束照旧：① 一个会话的 `session_added` 必须先于（或同批于）它的行到达；② 前端把起停帧与行同序处理（`events.ts` 同一条队列）。

**为什么不能松动**：「一次性的结束信号」要在重载后重建出来，账一不准，要么僵尸 Tab，要么活会话被误归档。

**谁在守**：`tests/backend/observe/session_ledger_tests.rs`。

## 24bis. 远端 idle-tmux 灰灯（audit-fixes F03.2）：`REMOTE_IDLE` 与 `remote_active` 正交、同一 emitter 单写

已并进 §24：「claude 退了、tmux 会话还在」就是账本里的**可重连**（tab 两轴 `Tab.state`，`tab-session-state.ts::SessionState`；转移只在 `nextState` 一处，行为只经 `isResumeOnly` / `hasTerminal` / `isLive` 读）。`/branch`、`/clear` 原地换 sid ⇒ 旧 sid 恒归档、不查 tmux 快照。tmux 的观测分四态（`tmux_observe.rs::classify_tmux_probe`：有会话 · 零会话 · 没有 tmux · 观测不到），只有前两态参与收割。

---

## 25. 行事件投递是 at-least-once —— 按 uuid 累积状态的前端模块必须自行幂等（issue #25）

行投递不保证恰好一次：重连重放、快照与实时的重叠区是同 seq 重投；截断重读时后端先发 `session_file_reread`、从 0 重数（前端那个会话整份重来）。两端只消费以 `\n` 结尾的完整行。`tab.seenSeqs` 挡同 seq 重投；**任何按 uuid 累积状态或构建拓扑的前端模块必须对「同一条再来一遍」幂等**（入口按 uuid 拒重、保首见），例：`BranchFolder.seenUuids`。

**为什么不能松动**：一条重复记录就能毒化拓扑计数，把整段历史误折成「已被 ESC 回退」；重复的常是不出 DOM 的记录，肉眼看不见、F5 后带毒不自愈。

已知不管的两形：最后一行合法却永远等不到 `\n`（写端被杀）⇒ 永不投递；两次事件之间文件先长再被重写到中间长度 ⇒ 只凭长度检不出截断。

## 25a. 远端 seq 是行号空间（Batch8 起）——重连碰撞在无截断前提下源头已除

后端连接时把各文件的 seq 计数器初始化为当前完整行数、新行 seq = 行号；旁路快照按行号编号 ⇒ 无截断时重连后的新行 seq 不低于断连前的高水位。断连期间截断（`/clear`）仍可能短暂吞行，靠快照重拉与 §25 的幂等兜底。

## 26. bg 会话门是数据层配置门；后端流模式 flag 必须先于查询模式判定剥离（Batch7-F24）

- 会话是不是后台、此刻在干什么只看后端判好的 `background` · `activity`（那一家的原词不上线）。后台会话是**标注而非过滤**：后端照宣告（`background: true`），显示与否由出口按用户开关定（本机远端两条流同一个口径）。
- **后端任何新的流模式旗标必须在一次性查询模式判定之前从参数里剥掉**（`lib.rs::split_stream_flags`），否则旗标落进查询分支、后端打印结果就退出，monitor 等不到 hello。
- **流要什么不靠协商旗标**：流模式起参只有 `--stream` · `--tz` · `--view`，这条流要什么由声明说（attach 行的 `view` / `--view`）；hello 不带能力 token。`build_id` 管换不换后端，`v` 只为破坏性变更加。

## 27. 远端会话生命周期信号的两条载荷型约束（Batch9）

- **F5 时起停与骨架先于重放**：就绪点（`event_replay.rs::ready_point`）之前，这条流上的起停帧已经先交（起停帧不吃 credit、不许丢）。
- **缺省一律按最保守的待**：`status` 缺 ⇒ 未知（不加灯）。

## 28. 自造持久身份的护栏（F64 / issue #58 单向门①）

任何会落盘、跨进程或上线的身份标识，必须 **opaque ＋ 稳定 ＋ 出生一次 ＋ 永不从名字 / 路径 / 位置算**。想不清就先别发，用外部已有的稳定 id 顶着（Claude Code 的 `sessionId`）。

今天的持久身份全挂外部稳定 id：会话与历史注解按 `sessionId`（`history_annotations.rs::Table`）· `ps-registry` 按 pid。`origin`（`stream_source/config.rs` 的 `RemoteConfig.label`，空则回退 `host`）是用户可控的外部 id，只做结构字段与内存表的键，不当任何落盘登记表的主键；别把它换成从 IP / 路径现算的值。会话 / 后端登记表的主键不许拿 tmux 会话名 · 主机名 · 路径。

**为什么是单向门**：id 一旦被别处引用或落盘就锁死；拿会变的值当键（Claude Code 自己的 `enc(cwd)`）一搬目录历史全对不上。

## 29. 会话生命周期解析规则以权威规格为准，两端不许各自发明（F67 / issue #58 单向门⑤）

会话生命周期的解析规则（未知记录抢救 · ESC 回退主线 · `end_turn` 判定 · 排队折叠 · api_error 两形 · attachment / isMeta 链 · pidfile 保守缺省 · PID 复用防护）只在后端适配层实现一份，两个前端都吃它的成品（§7 冻结面见 IPC-PROTOCOL）。改规则先改后端那一份；发现别处漂移就对回它，不各自发明。核心判据不许近似（`end_turn` 必须是 `stop_reason == "end_turn"`）。

## 30. tmux ↔ 会话精确映射靠 `@ccm_sid`，不靠名字/目录反推（F74 / #63 / SS-5 / SS-9）

`/branch` 在同一个 tmux 里把会话从 A 换成 B，tmux 名不变；同一目录常有多个 claude tmux ⇒ 按名字或 `cwd` 反推会撞进别的会话。tmux 用户选项 **`@ccm_sid`** 记「此刻在跑哪个 sid」，写者是后端 `src/backend/control/identity_tag.rs`（看到 pidfile 的那一刻读那个进程的 `TMUX_PANE` 定位、对会话句柄打标，`/clear`、`/branch` 跟着重打，零新增节拍）；读它的是终端名单 `terminals-list`（`terminals.rs::parse_rows`）。`@ccm_sid_expect` 是起会话时声明的意图，**任何破坏性判断只认 `@ccm_sid`**。

**铁律**：attach / resume 一律先按 `sid == @ccm_sid` 精确匹配；已知 sid 无一命中 ⇒ 判「不在任何 tmux」，绝不回退按 `cwd` 抓别的会话。命中多个时：非破坏性动作警告并按第一个继续，破坏性动作（kill · 换号重启）拒绝。没有后端时在 tmux 里起有身份的 agent ⇒ `ccm` 响亮失败（逃生口 `CCM_NO_BACKEND=1` 也要说一句）。

## 31. 一端起的会话另一端必须能接——前端绝不硬编码会话后端命令（F90 / #48 / SS-12）

会话后端（多路复用器）是机器的属性，不是界面的属性。**前端绝不出现可执行的 `tmux attach` / `new-session` / `send-keys` 字面量**：起 / 接会话的命令一律问后端要（只交一行 `ccm …`，§33）。加任何第二种会话后端之前必须先答「另一端还接得上吗」；登记表主键守 §28。

**谁在守**：`tests/frontend/ui/launch-no-shell-in-ts.vitest.ts`（`src/**/*.ts` 生产段零处 tmux 动词 / `&&` 字面量，不开例外）。

## 31a. tmux `-t` 目标恒用 `=<名>:` 精确形态——三处同源，禁止任一处退回裸目标（F01 / unify-launch）

裸 `-t <名>` 按「精确 → 名字开头 → glob」解析：只有 `sib-2` 在时 `kill-session -t sib` 杀掉 `sib-2` 且 rc=0。`=` 只在 target-session 路径上被认，`send-keys` / `capture-pane` / `set-option` 走 pane 解析 ⇒ 必须带尾冒号 `=<名>:`，它是所有动词上都既通用又精确的唯一形式。`new-session -s <名>` 收的是名字，不加 `=` / `:`。e2e 的 shell 探针同样用 `=<名>:`。

今天的构造点：后端 `control/launch.rs::exact_target`（argv 直传）与调用行渲染器（`ccm_invocation.rs` 就地 resume 那一形，由 `cli-golden.json` 逐字节钉）· `control/ccm/plan.rs`。新建的名字走 `gate_rules::new_tmux_name_issue`（禁 glob 与目标语法字符、前导 `-`、控制符、视觉欺骗字符、超长），attach 已有会话走 `gate_rules::existing_tmux_name_issue`（只拒空 · 控制符 · 视觉欺骗字符）。

**谁在守**：`exact_target_is_the_exact_match_shape` · `tmux_backend_gate_guard_tests.rs::every_target_placeholder_comes_from_exact_target`（monitor 生产段零处 `-t {…}`）。

## 32. 本仓只有暗色主题——别声称"明暗两套都覆盖了"（仓库级事实）

`:root` 设 `color-scheme: dark`，全仓没有 `prefers-color-scheme`；`theme.ts` 的 `TOKENS` 只是暗色调色板内的一组可换值，`--overlay-*` / `--border-*` 是固定值。别在文档或宣传里声称覆盖了浅色。

## 33. LaunchPlan 双渲染器——CLI 渲染器对无法诚实表达的维度/容器形态必须放弃，不得近似（F03 / unify-launch）

界面那份 IR 与两个渲染器都已删，理由是起会话只剩一处。今天：monitor 的每一条起会话路径（新起 · resume · 换号重启 · 分叉 · 远端开新会话 · 本机拉起 · cc-bus 派生 · 就地 resume）问那台后端要的都只是一行 `ccm [交给 agent 的…] -- [ccm 自己的…]`；环境、中转地址、身份标记由那台的 `ccm` 在最终 exec 那一处定，外层容器只包这一行。**表达不了就放弃**照旧：渲不出 ⇒ 回码 `refused` 带那一句，调用方不回落、不拼第二条。

**谁在守**：`launch_cli_parity_tests.rs::every_monitor_launch_path_hands_over_one_ccm_line` · `local_tests.rs::every_local_launch_shape_is_one_ccm_line` · `ccm_tests.rs::nothing_but_ccm_renders_a_command_that_starts_an_agent`（后端生产段除 `ccm` 自己的最终 exec 外零处渲出直接起 agent 的命令；唯一登记的例外是给仓外 aterm 冻结的 `resolve_query.rs`）。

## 33a. `ccm --print` 是平价预言机——它对**环境变量**说的必须逐条等于真跑做的（U9a / unified-backend）

`ccm --print` 打印将要执行的命令而不执行，全仓把它当离线预言机用。① 凡是真 exec 会设的、`ccm` 自己决定的环境变量，`--print` 必须说出来且顺序对齐；② `--print` 必须是纯的（不查实时 tmux、不写文件、对宿主环境逐字节稳定）——值不知道就打印配方（执行时才求值，如 `BUS_ID_RECIPE`）。它刻意不说撞名避让退到第几个（要查实时状态）。`--ccm-probe` 首行逐字 `name=ccm`（`src/frontend/shell/src/ccm_probe.rs::parse_probe_output` 靠它判装没装），`capabilities=` 必须覆盖 `src/backend/control/launch_render/ccm_invocation.rs::CLI_REQUIRED_CAPS`（⊇，不是 ==）。

**谁在守**：`tests/e2e/ccm-contract-parity.sh`（差分 ＋ 每条保住项各一条绝对断言 ＋ 「真跑确实产出了环境」自检 ＋ 顺序两侧各钉一条）。

## 33b. 起会话那一行只由后端渲

起会话交给终端的只有一行 `ccm …`，由那台后端渲（`control/launch_render/`：远端 `launch-render-cli` · 本机 `launch-local`）。界面没有渲染器、不留回落：后端答不出就明说失败。建得出来、主路杀不掉的名字不许被铸出来（`backend_kill_tests`）。

## 34. tmux 破坏性/半破坏性命令三道门 + 原子 verify+act（F04 / unify-launch / R10）

送键与杀会话只走后端（界面经 `src/frontend/ui/tmux-control.ts` 说 `kill` / `launch`；monitor 一道门都没有、生产段一处 `kill-session` 都没有）。三道门都在后端：

1. **Gate 1（恒强制）**：目标名过 `gate_rules::existing_tmux_name_issue`（空 · 控制符 · 视觉欺骗字符）；入口 `kill.rs::admit_existing_name`（kill · capture 共用）与 `src/backend/control/launch.rs::parse_request`。空目标（`=:` 会被当成当前会话）是唯一真正危险的默认值。
2. **Gate 2（身份，并集）**：`control/gate.rs::admit`：`is_ccm_tmux_name` 前缀命中**或** `@ccm_sid` 已设（只认事实，不认 `_expect`）。判定本体只有 `control/gate_rules.rs` 一份。
3. **Gate 3（只对 kill）**：`control/gate.rs::admit_destructive`：会话只有一个 window 才许 kill（长出了别的 window ⇒ 有独立于本工具的用户活动）。

**原子 verify ＋ act**：`control/gate.rs::probe` 一次 `display-message -p -t <目标>` 取回 `session_id` · `@ccm_sid` · `session_windows`，判完之后对拿回来的 `#{session_id}` 句柄下命令，不另按名字发第二条（两次往返之间的窗口可被抢跑）。所有形态都先 probe 一次。后端不在 ⇒ 明确失败（文案 `tmuxControl.channel.*`），不走另一条路做掉。

**谁在守**：`tests/e2e/backend-gate2-acceptance.sh`（真后端 ＋ 真 tmux、私有 `-L` socket，用例逐行来自判定表 `gate2-golden.tsv`）· `tmux_backend_gate_guard_tests.rs`（monitor 侧零第二条路）· `gate_singleton_guard_tests.rs::the_monitor_holds_no_gate_of_its_own`。

## 35. 维度的 `applies` 绝不能条件性跳过 `cliFlags` 的 `null` 安全网（F05 / unify-launch）

已删：维度注册表随界面的 IR 一起删了（§33）。它留下的规矩今天落在后端调用行渲染器上：用到的能力缺一项就拒，不近似（`ccm_invocation_tests.rs::unconditional_caps_are_always_required_and_conditional_ones_only_when_used`）。

## 36. 本地（Windows）路径不经 IR 产出命令——嵌套 env 污染保护已在进程启动期做完，别在本地渲染器里重复实现（F06 / unify-launch；R07 收紧）

本机那几形由本机后端 `control/launch_render/local.rs::plan` 出成品；Windows 那一格也只是一行 `ccm --resume <sid> -- …`（没有 tmux ⇒ 直路；接回说不出 ⇒ 拒）。**本地渲染器不渲任何清变量的代码**：清 Claude 自己的嵌套会话标记归 `ccm` 在最终 exec 那一处（进程内摘环境，不是一段 PowerShell 语法），本机远端、两个平台同一处。这一形要 `ccm` 在用户的 `PATH` 上。

**谁在守**：`local_tests.rs::windows_launches_go_the_direct_way_and_attach_is_refused`。

## 37. 新维度的 `applies` 该不该恒真，看这个维度的"沉默"是否等价于用户期望——不是看它是不是账号相关（F07 / unify-launch）

已删：维度注册表随 IR 删了（§33）。同一个问题今天问在调用行渲染器的能力上：一个修饰「不出现」时，下游的默认若不等于用户的期望，就必须恒要求那一项能力（见 §35 的判据）。

## 38. 一条新正交轴该进 `LAUNCH_DIMENSIONS` 注册表，还是该做 `LaunchPlan`/`LaunchContext` 的硬编码一等字段——三条 checklist（F09 / unify-launch，R12）

已删：`LAUNCH_DIMENSIONS` · `LaunchPlan` · `LaunchContext` 随 IR 删了（§33）。新轴今天直接是后端起会话请求里的一格（`control/launch_render/`），界面上「有哪些可选值」现查后端（例：账号 `src/frontend/ui/account-reads.ts::fetchAccounts`）。

## 39. `WrapSpec` 是纯数据 `{ id, order, prelude }`，不是闭包——且 rbind 走不走 wrap 这件事必须先定（R04④ / unify-launch）

已删：`WrapSpec` 随 IR 删了（§33）。外层容器今天只包那一行 `ccm …`，身份由 `ccm` 与后端打标负责（§30），没有第二个写者。

---

## 修改本文档

加一条不变量：加到对应位置并编号；在守它的模块头注或判据头注里引 `INVARIANTS §N`（`doc_claim_registry_tests.rs::every_invariants_section_cited_in_code_exists` 核那一节真在）；写清「谁在守」。已由判据钉住的条目正文只留一句规矩加判据名。松动一条：先写清新的约束是什么、为什么旧的可以松，全仓找全受影响处。

---

## 40. 「本地」= 不走 ssh 的远端 —— 一条路径，transport 是它唯一的差异

用户原话：「我的目的就是把本地当成不走 ssh 的远端。后面都要这么搞。」凡新增「起会话 / 看会话 / 管配置」的能力，先问它能不能只是远端那条路少一跳 ssh；能，就不许另起一套。本机与远端是同一个后端二进制、同一套帧命令，差别只在载体（本机：本人套接字 / 管道；远端：SSH 上的链路）。

**功能面也要一致**（用户 2026-07-29：「本地的功能要和远程功能一致」）：新能力必须同时落在本机与远端，或者显式登记为天然不对称并写理由。既有缺口逐条登记、按节奏还。天然不对称的：端口转发（本地没有「转发到自己」）· 多地址故障切换（本地没有地址）· 本机那份后端由宿主启动时自己释放、不经部署命令。Windows 本机是唯一的平台例外（没有 tmux；起会话走直路，§36）。

**谁在守**：`parity_ledger`（Tauri 命令逐条：两侧都有，或在白名单里带理由；新增不登记就红）。

---

## 41. 后端的判活信号全部由内核事件驱动 —— 四路事件、零定时器、四个盲区如实分类（zero-poll-liveness P0-P7）

后端生产段零定时器：判活与 tmux 观测全部由内核事件驱动，四路事件汇进 `watcher.rs` 的同一个通道，`watch_loop` 阻塞在无超时的 `recv()` 上。

### 41.1 四路事件（延迟均为真机实测，标明测法）

| 场景 | 事件源 |
|---|---|
| claude 进程退出 / 被强杀 | pidfile inotify ＋ `pidfd`（绑进程实例本身），实测约 18ms |
| 杀掉某台仅剩的会话（tmux server 随之退出） | tmux server 的 `pidfd`，实测约 27ms |
| tmux server 复活 | socket 所在目录的 inotify，实测约 150ms（含去抖） |
| 多个会话里杀掉其中一个 | tmux `session-created / closed / renamed` hook → `--tmux-notify` → SIGUSR1（hook 槽在段 `[50, 100)` 里每个后端实例一格、起时清死槽），实测约 126ms |

第五个触发条件：pidfile 里绑的 sid **真的变了**才重探 tmux（不是每次收到 `.json` 事件 —— CC 每次状态转换都重写 pidfile，那等于变相轮询）；`tmux_reprobe_triggers_on_sid_drift_not_on_every_json_event` 钉着。

### 41.2 一条正确性改进（不只是延迟改进）

PID 死亡判定用 `pidfd` 绑进程实例本身 ⇒ PID 复用在机制上无从发生；`--tmux-notify` 先核 starttime 再发信号，不符即不发。

### 41.3 四个盲区，如实分类（标题原写「三个」，表里一直是四行 —— 2026-08-01 订正）

| # | 盲区 | 处置 |
|---|---|---|
| ① | tmux server 复活后 hook 丢失 | socket 目录 inotify ⇒ 「server 起来了」本身是事件，起来就重装 hook |
| ② | 会话活着但卡死 | 不做（轮询从前也没做这件事） |
| ③ | 整台机器挂掉 | 机器内部无解，靠 monitor 断连说「说不清」、重连自愈 |
| ④ | 已存在的会话上 `@ccm_sid` 变了（`/branch`、`/clear`） | 四路事件一个都不响 ⇒ 由 §41.1 的第五个触发条件接住 |

另有一条：inotify 队列溢出会被去抖库静默吞掉；`pidfd` 对溢出免疫。**绝不为它补定时器。**

### 41.4 红线：绝不为了让守卫变绿而删掉唯一信号源

`no_timer_guard.rs` 扫后端全 crate 生产段：判据落在「会让线程自己醒来的构件」，不落在「出现过 `Duration`」；非定时器的 `Duration` 用途逐条登记、处数恰好等于条数；会醒来的构件只许住 `REGISTERED_DEADLINE_WAKES`（每行写住址 · 理由 · 醒在什么期限上，今天都是一次性期限，不是节拍）。

三条派生纪律：① 后端源码的散文里不逐字引守卫的禁用模式（守卫连注释一起扫，改措辞，不改守卫）；② 删一个周期性信号时，要连依赖那个节拍的 e2e 一起跑；③ 源码扫描型守卫「剥测试段」的剥法必须同时防欠剥与过剥 —— 剥法只有共享 crate `src/common/guard-core` 一份，剥完断言残留 `#[test]` 为 0（`guard_support::assert_no_test_code`），扫描面用字节总量下限 ＋ 文件数与独立遍历相等双判据钉住。验证一条守卫只有一种算数：把违规代码分别放进「应被扫到」与「应被剥掉」的位置，看红绿是否相反。

### 41.6 后端的写盘边界（铁律 I7，2026-07-31 G2 起收窄）

**后端不许改动用户既有数据，只有登记过的几层可以写**（`readonly_guard.rs`，各层按仓相对路径匹配）：

| 层 | 范围 | 判据 |
|---|---|---|
| 默认层 | 下面几层之外的全部后端生产源码 | 写模式一条都不许出现 |
| 新建层 | `control/fork_write.rs` | 必须含 `.create_new(true)`（带前导点：只许由调用满足，不许被注释喂饱）；不得有删除 / 改名 / 复制 / 链接 / 截断 / 追加 / 覆盖写 / 建目录 / `set_len` / `.create(true)` |
| 文件管理写面 | `control/files_write.rs` · `control/files_commit.rs` | 改动动词是闭集；每一处改动之前先过路径解析（`files_write::resolve_in_root`：逐段只许普通段、拼出来仍在根下、父目录解链接后仍在根下；跟链接的动词用 `resolve_existing_in_root`），针 `readonly_guard::RESOLVE_CALLS`；这一面不问数据是谁的（文件管理器可以改会话文件） |
| 后端自有状态文件 | 按文件登记（第四层表，相等断言） | 每份只写它自己那一个文件 |
| 远端（SFTP） | `dial/sftp.rs` | 只往远端 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/` 写，每处改动先过 `dial/sftp.rs::fenced_remote`；落进用户目录只经远端后端 `files-commit-upload` |

删历史会话（`files-delete-session`）那一道单列：只收 sid、必须是一份会话记录，`readonly_guard::the_file_manager_face_never_asks_the_session_shape` 钉着「问会话形状的只有删会话那三处」。TOCTOU（判定与动手之间的窗）未闭合，如实登记。

**为什么不能松动**：写的能力被钉在几个可审计的洞里，「后端会写盘」不可能悄悄扩散到第二个模块。

### 41.5 兼容与部署

线上只做加法：新帧、新格不加 `PROTO_VERSION`；后端改了行为必须加 `BUILD_ID`，否则已部署的旧后端报同一个 id、不被判陈旧、不自动换，整轮改动在远端休眠（monitor 比的是内嵌字节自报的 id，`byte_table.rs::my_backend_id`）。

---

## 42. 协议文档从代码生成，不手抄

逐格那一半（每一帧的字段 · 每一条命令的字段 / 向 / 码 / 档位 · 每一个 CLI 子命令）只有一个家：代码（`wire.rs` 的 serde 类型与它们的 `///` · 入方向命令注册表 · CLI 子命令表），`src/doc/IPC-COMMANDS.md` 由它生成；`src/doc/IPC-PROTOCOL.md` 只写不随命令变的总述。别的文档提到帧命令，名字必须在注册表里。

**为什么不能松动**：手抄再对拍的那几年，照文档写的客户端静默读错过字段名；对拍只能查「名字在」，查不了「说得对」。

**谁在守**：`tests/backend/protocol_doc_gen.rs`（重新生成 == 仓里那份）· `doc_claim_registry_tests.rs::every_frame_command_named_in_the_durable_docs_is_registered`（耐久文档里紧跟在「帧命令」「帧」后面的名字都在注册表里）。

---

## 43. **monitor 侧**的周期性唤醒也要逐条登记 —— 它与 `§41.4` 的范围差写在这里

monitor 生产段每一处会让线程自己醒来的构件都登记在 `rust_timer_registry`，写明类别（ticker / wait-for-condition / throttle / startup-delay），ticker 还要写事件源与谁退役它。它不禁周期行为（界面刷新、重连退避是正当的），要的是节拍不许无声无息地长出来；§41.4 只管后端。

**谁在守**：`tests/frontend/shell/rust_timer_registry_tests.rs`（盘上处数 == 登记条数，两向）。

---

## 44. **monitor 侧**起进程的面也要逐条登记 —— 而它**不是**「防写盘」

monitor 生产段每一处起进程的地方都登记在 `exec_site_registry`，写明起的是什么、谁是它唯一的出口。它防的是同一件事出现第二条起法（参数拼装、环境擦洗、错误归因一定会漂）与没人认领的子进程；与 §41.6 正交。

**谁在守**：`tests/frontend/shell/exec_site_registry_tests.rs`。

---

## 45. webview 的权限清单**默认拒绝** —— 它是「前端碰不到机器」那一族登记表的共同前提

webview 拿得到的 Tauri 权限只许是登记过的那一批；webview 里跑的代码只许是我们自己的（`withGlobalTauri` 关着、CSP 不放开脚本执行面）。写盘 · 远端执行 · 本机起进程那几张登记表只扫 Rust，它们共同的前提是「前端碰不到机器、只能 `invoke` 我们自己的命令」；本产品渲染的是不受信的会话文本，这个前提一破，那几张表的绿就不说明任何事。文件窗口是独立进程、不是 webview，不在本条射程里。

**谁在守**：`capability_registry_tests.rs::every_webview_permission_is_registered`（清单 ↔ 登记表两向）· `capability_registry_tests.rs::the_webview_execution_surface_stays_closed` · `capability_registry_tests.rs::tauri_loads_capabilities_from_exactly_one_source`。

---

## 46. 每一套检查**要么进门禁，要么登记为什么不进** —— 人群从盘上全集派生，默认拒绝

仓里每一套检查（e2e 套件 · shell 脚本 lint · 共享 crate 的测试 · CI 的每一步 · `package.json` 的测试脚本 · 每一条 `#[ignore]`）必须进门禁（有具体的一步真跑它），或登记在册写清为什么进不了（豁免行不许变成死行）。人群从盘上全集取，不从「已经接上的那批」取。

**为什么不能松动**：不在执行链上的检查与不存在没有区别，还让人以为那件事有人守。

**谁在守**：

| 管哪一类 | 判据 |
|---|---|
| e2e 套件 | `e2e_gate_registry_tests.rs::every_e2e_suite_is_either_gated_or_registered_as_exempt` ＋ 豁免不许变死行；「进门禁」= `tests/scripts/gate.sh` 里一行 `run_e2e <套件>` |
| shell 脚本的 lint | `shell_lint_registry_tests.rs::every_shell_script_is_either_linted_or_registered_as_exempt` |
| 共享 crate 的测试 | `shared_crate_registry_tests.rs::every_shared_crate_is_a_workspace_member` ＋ 门禁 `cargo` 那一格 |
| CI 的每一步 | Linux 那几个 job 一律调门禁（`GATE_ONLY=<格>`）；Windows 两个 job 自己写命令 |
| `package.json` 的测试脚本 | `shared_crate_registry_tests.rs::every_test_script_is_either_run_by_ci_or_registered_as_manual` |
| `#[ignore]` 的判据 | `shared_crate_registry_tests.rs::every_ignored_test_still_has_someone_who_triggers_it` |

它不买「进了门禁的那一步真的在跑」（登记的是接线，不是执行）。

---

## 47. 外部来的值拼进 shell、或交给对端之前，**本侧**先过放行判定 —— 不许拿「对端会校验」「这是我们自己的数据」免检

一个值只要从本进程外面来（盘上文件 · manifest · 对端回话 · 用户输入 · 远端目录名），在被拼进 shell 命令串、或交给对端去执行 / 寻址之前，本侧先过一道按这个值的种类写成的放行判定；判不过就在本侧拒，说清哪个值、为什么，一个请求都不发出去。三形：

- **① 标识符类**（cc-bus agent id · 账号名 · 分叉的 sid / 消息 uuid · 本工具建的 tmux 会话名 · ssh 别名 · 端口转发规格）：字符集白名单（闭集）＋ 不许 `-` 开头 ＋ 有上界就钉上界。
- **② 自由文本类**（路径 · 已有 tmux 会话的 attach 目标）：走唯一的 quote ＋ 这种值的形式判定（绝对 · 不含 `..` · 不空）＋ 拒绝集权威表（控制字符 · shell 元字符 · 视觉欺骗字符，`acct-core` 的 `config_dir_char_unsafe` 一族）。这一形是拒绝集，不是白名单。
- **③ 命令片段类**（启动器）：不 quote（要被 shell 拆词、按 alias / PATH 解析），只能是一张对 bash 与 PowerShell 都安全的白名单（ASCII 字母数字 · 空格 · `-_./` · 打头的 `~/`），住 `shell_quote_core::launcher_refused_char`。

本侧判的是**形状**，不是成员资格（「这个账号存不存在」归拥有那份名单的一方）。cc-bus 的 id 在后端交给 `cc-send` / `cc-kill` / `cc-spawn` 之前判（拒码 `bad_id`），规则住 `shell_quote_core::bus_id_ok`。

**为什么不能松动**：对端校验的是它自己的入口，本侧不判，拼错的那一段已经在路上了（收掉一个 agent 的后果是杀掉一棵进程树）；盘上真出现过没人预料的 id（一份名叫 `--help.jsonl` 的收件箱文件）。

**谁在守**：`ccm_invocation_tests.rs::every_value_is_judged_before_it_becomes_a_ccm_argument` · `argv_tests.rs::a_session_id_is_judged_before_it_goes_anywhere` · `fork_write_tests.rs::the_fork_ids_are_whitelisted_at_the_frame_face` · `cc_bus_tests.rs::bus_ids_are_judged_here_before_they_reach_cc_bus` · `plan_tests.rs::a_session_name_that_would_confuse_tmux_is_refused` · `plan_tests.rs::free_text_values_pass_real_names_and_refuse_what_the_quote_cannot_hold` · `plan_tests.rs::a_launcher_is_one_command_fragment_from_the_shared_whitelist` · `accounts_query_tests.rs::unsafe_config_dirs_are_dropped` · `dial_forwards_tests.rs::the_spec_fence_stands_before_any_lookup_or_dial` · `dial_terminal_tests.rs::the_open_terminal_cwd_passes_real_names_and_refuses_what_quote_cannot_hold` · `mcp_edit_tests.rs::a_broken_mcp_json_is_never_overwritten_and_relative_dirs_are_refused` · `exec_site_registry_tests.rs::every_remote_exec_declares_where_its_command_came_from` · `lib_invariant_population_tests.rs::every_file_that_quotes_a_value_into_a_shell_line_is_registered`（拼值进 shell 的文件逐份登记）。

---

## 48. 本机常驻后端的宿主三条：**门由内核给 · 脱离后不留僵尸 · 测试里起真后端必须 fail-closed 地隔离用户 tmux**

常驻后端一旦脱离了起它的 monitor，就不再有父进程看着：谁能连上它、它死了谁收、测试里起的那一个会不会碰到用户的东西，各有一条硬规矩。

### 48.1 门由内核给（本人通道，没有钥匙）

常驻后端只听这台家里的一个 Unix 套接字 `<家>/run/backend.sock`（路径只由共享 crate `relay_route_core::listen_socket_for` 算）。`run/` 只给本人（`0700`），每条连接再核对端 uid（共享 crate `own-chan`），不同 ⇒ 关掉、出声；在听的那一个攥着 `run/` 的独占锁，抢不到就带「已有人在听」的退出码退。attach 行只是 `{"attach":true}`（可带 `flags`），形状不对 ⇒ `malformed-attach`。远端经 ssh 跑 `ccm -- --resident-attach` 小中继去连那台的套接字。常驻后端起子进程时清掉常驻开关、诊断文件与后端内部那几族环境（`platform/child_env.rs`）。沙箱跑（`CCM_SANDBOX=1`）却要占本账号真家目录里的门牌 ⇒ 拒绝起。文件窗口的通道不监听（父子管道，`chan/host.rs::serve_window`）。

**为什么不能松动**：从前的回环 TCP 口没有权限位，只能靠一把钥匙补，钥匙文件又被别的实例改写过（远端从此一直被拒）⇒ 已删，换成内核给的本人门。能连上套接字的只有本账号自己的进程。

**谁在守**：`listen_tests.rs::the_resident_switch_is_empty_one_or_refused` · `listen_tests.rs::attach_verdicts_judge_the_shape_only` · `listen_tests.rs::refusal_reasons_are_a_closed_set` · `lib_tests.rs::the_dir_lock_is_exclusive_and_released_on_drop` · `lib_tests.rs::a_peer_with_my_uid_is_ours_and_nobody_means_nobody` · `main_claim_tests.rs::a_late_starter_that_cannot_claim_never_touches_the_log_or_the_socket` · `main_claim_tests.rs::a_sandboxed_start_refuses_the_real_home` · `resident_tests.rs::the_sandbox_refusal_truth_table` · `child_tests.rs::a_child_never_inherits_the_backend_internal_families` · `local_backend_host_tests.rs::the_host_never_writes_the_resident_dir_and_carries_no_key` · `local_backend_host_tests.rs::a_stranger_on_our_port_is_refused_out_loud_not_silently_reused` · `remote_resident_tests.rs::the_relay_waits_only_while_nobody_listens` · e2e `local-backend-supervise.sh`。

#### 48.1a 中转口的钥匙

中转口（常驻后端进程内的回环口，本机远端同一份代码）每条请求在读请求体、问上游选择之前过三问：带 `Origin` ⇒ 403；`Host` 不是回环字面量、缺或不止一个 ⇒ 421（防 DNS rebinding）；路径第一段不是钥匙 ⇒ 403（定长时间比对，`comms/outward/door.rs` 的 `tokens_match`）。过了才剥掉那一段交给路由（「钥匙对、没这条路由」是 404，与 403 可分）。

**一把根钥匙、门上两个范围**：根钥匙 `~/.cc-monitor/relay-key`（`0600`，256 位，中转绑上口之后读回或铸，跨重起不变）开 `/s/` 与 `/t/`；只许直通那一把不落盘，由根钥匙派生（`relay/key.rs::pass_of`）、只开 `/t/`（中转在 `/t/` 永不代入凭据），它可以进 argv（有的 agent 只认命令行参数里的上游地址）。根钥匙只从那份文件进 agent 进程自己的环境：交给终端的那一行 `ccm …` 不带地址也不带钥匙，`ccm` 在最终 exec 那一处读文件、拼进 agent 环境（插钥匙只在 `relay/key.rs::keyed_with_key_on_disk` 一处）。唯一的例外：用户选「直接敲的 claude 也走中转」时，`relay-optin` 的成品带着要贴的那一段。

**为什么不能松动**：中转会代入账号的凭据打上游；门开着，同机任何进程（别的用户、浏览器里的网页）就能以这个账号的额度发请求。

**谁在守**：`door_tests.rs::only_the_exact_key_as_the_first_segment_gets_in` · `door_tests.rs::the_pass_key_gets_in_but_only_for_passthrough_routes` · `door_tests.rs::any_origin_header_is_refused_before_the_key_is_looked_at` · `door_tests.rs::only_a_loopback_literal_host_gets_in` · `door_tests.rs::the_three_refusals_are_distinct_faces` · `key_tests.rs::the_key_file_is_minted_once_private_and_read_back_across_restarts` · `server_tests.rs::rk1_the_minted_key_never_shows_up_in_logs_tee_argv_env_or_upstream` · `plan_tests.rs::the_relay_address_is_decided_at_the_final_exec_and_only_there` · `plan_tests.rs::an_inherited_relay_address_of_ours_never_goes_into_the_pane`。诚实边界：`ccm` 是一次性进程，读不到常驻后端的监听状态 ⇒ 钥匙文件在、口被别人占着时会被认成在听（`accounts/upstream_select/endpoint.rs::relay_for_exec`）。

### 48.2 脱离后不留僵尸

脱离起的后端（Linux 上 `process_group`，不改父子关系）无论怎么死，进程表里那个 pid 都要消失、不许停在 `Z`；收尸走 `local_backend_host.rs::reap_detached`，收完 pid 与二进制路径仍留在句柄里（停那一步还要核身份）。只量了 Linux。

**谁在守**：`local_backend_host_tests.rs::e2e_a_detached_backend_that_dies_leaves_no_zombie`。

### 48.3 测试里起真后端必须 fail-closed 地隔离用户 tmux

**仓里每一处起 tmux 的地方都说清连哪台 server**：测试与脚本显式 `-S <私有 socket>` / `-L <私有名>`（或挂强插 `-L` 的共享原语 `tests/e2e/tmux-shim.sh`）并摘掉继承来的 `$TMUX` 与 `TMUX_PANE`（`env -u TMUX -u TMUX_PANE` · `. sandbox-env.sh`）；产品代码只经 `platform::child::Child` 起 tmux（测试构建里那一道 `tests/backend/platform/child_tmux_fence.rs` 摘掉 `$TMUX` / `TMUX_PANE` 并落到本进程自己的空目录）。起真后端二进制的测试必须经唯一的口拿到私有 tmux（shim 放进后端 `PATH` 最前），拿不到就炸、不许降级裸跑；进程内走到 `identity_tag::tag` 的测试必须注入假 tmux，没注入就在入口炸。只靠 `TMUX_TMPDIR` 不算隔离（`$TMUX` 一有值就压过它）。

**为什么不能松动**：后端一上来就往连得到的 tmux server 装全局 hook；不隔离就是去改用户真 tmux。靠 `TMUX_TMPDIR` 的隔离被 `$TMUX` 压过、一句 `kill-server` 打掉开发机上正在用的 server，这事发生过不止一次（最近一次 10-09）。

**谁在守**：`e2e_gate_registry_tests.rs::every_tmux_call_names_its_server_and_drops_the_inherited_tmux`（`src/` ＋ `tests/` 全部脚本与代码）· `e2e_gate_registry_tests.rs::no_e2e_suite_isolates_with_tmux_tmpdir` · `e2e_gate_registry_tests.rs::the_tmux_shim_primitive_has_exactly_one_home` · `local_backend_host_tests.rs::every_test_that_starts_the_real_backend_demands_a_private_tmux` · `local_backend_host_tests.rs::the_one_shim_gate_really_fails_closed` · `local_backend_tests.rs::every_real_backend_e2e_demands_a_private_tmux_dir` · `identity_tag_tests.rs::an_in_process_tag_without_a_fake_tmux_blows_up`。程序名在变量里的调用（`"$REALTMUX"` · `Command::new(&tmux)`）判据认不出。

---

## 49. tmux 的**打印通道必须是 UTF-8**，按 TAB 切出来的段数**下溢必须出声**

每一处按格式串读 tmux 打印通道的调用点（`list-sessions -F` · `ls -F` · `display-message -p`）起的都必须是 UTF-8 客户端：argv 直传与跨 SSH 的命令串用旗 `UTF8_CLIENT_FLAG`（必须排在子命令之前）；`sh -c '<多分支脚本>'` 用环境 `UTF8_CLIENT_ENV`；按 TAB 切的，段数小于预期 ⇒ 出声，这一行不当好数据。口径只有一个家：后端 `common/tmux_utf8.rs`。

**为什么不能松动**：tmux 判客户端是不是 UTF-8 只看 `LC_ALL` → `LC_CTYPE` → `LANG` 第一个非空值；一旦不是，输出里的 TAB 与非 ASCII 全被改写成 `_`、退出码仍是 0 ⇒ 会话从列表里静默消失。改写是每客户端全有全无的，所以下溢是完备检测器。

**谁在守**：`gate_tests.rs::both_tmux_call_sites_ask_for_a_utf8_client_before_the_subcommand` · `tmux_observe_tests.rs::every_sh_call_site_in_this_module_carries_the_utf8_env` · `session_snapshot_tests.rs::the_one_list_sessions_call_asks_for_a_utf8_client_before_the_subcommand` · `gate_tests.rs::the_underflow_predicate_catches_the_real_dirty_bytes` · `tmux_observe_tests.rs::the_underflow_predicate_only_fires_downward` · `session_snapshot_tests.rs::a_tab_starved_line_is_dropped_instead_of_becoming_a_session` · `tmux_utf8_tests.rs::each_kou_jing_has_exactly_one_home_and_it_is_this_file` · `lib_invariant_population_tests.rs::every_tmux_print_site_is_registered_with_how_it_carries_utf8`（人群：每一处「打印子命令 ＋ `-F` / `-p`」== 登记表）· `tmux_backend_gate_guard_tests.rs::utf8_client_kou_jing_has_one_home_and_this_side_has_none`。上溢不管。
