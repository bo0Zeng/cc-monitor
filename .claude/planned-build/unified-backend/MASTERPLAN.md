# 主计划 / MASTERPLAN — unified-backend（把 cc-monitor 拆成 frontend + backend）

- 建区 2026-08-01 · **v3（四视角计划自审后重写）**
- 状态：**Phase A 完成（含一轮计划自审）**，进 Phase B / U-1
- 摸底底账：[A 全路径](PHASE-A-摸底-A-起停接会话全路径.md) · [B daemon 平台与通道](PHASE-A-摸底-B-daemon平台与通道.md) · [C ccm 能力盘点](PHASE-A-摸底-C-ccm能力盘点.md) · [D 文档改写清单](PHASE-A-摸底-D-文档改写清单.md)
- 计划自审：[PHASE-A-计划自审-四视角.md](PHASE-A-计划自审-四视角.md)

---

## §0.0 一句话

**把 cc-monitor 拆成两半：frontend（UI + 开窗）与 backend（读 observe + 控制 control）。**
backend 一份代码、两种承载（本机进程 / 远端进程），本地与远端同一套分解。

> **用户定框（2026-08-01 原话）**：「you can just regard it as we decomposed the monitor.
> we separate it into 2 functions, read and control. this is the same at local.」

backend **不是 monitor 的外部依赖，是 monitor 自己的一半**。

```
cc-monitor（产品）
├── frontend   UI · 状态 · 在桌面上开终端窗口（本机 OS 的事）
└── backend    ← 一份代码，两种承载
    ├── platform/   ★ 唯一允许平台 cfg 的地方（linux | windows）
    ├── observe/    读：会话流 · 判活 · tmux 快照 · 历史 · 用量 · 账号
    ├── control/    控制：起 · 停 · 接 · 分叉 · 预信任
    └── common/     两边都要的纯工具（paths / 时间换算 / quote）※ 三分不够用，见 §0.5-6
```

终端里的 thin ccm 是 **backend 的客户端**，不是第三个宿主。

---

## §0.1 病灶：能力按「本地 / 远端」劈成两半 —— 但只有**四组**是真重复

| 能力 | 本地 | 远端 | 类别 |
|---|---|---|---|
| jsonl tail + seq | `src-tauri/src/watcher.rs`（生产段 ~361 行） | `remote-daemon-proto/src/watcher.rs`（生产段 1772 行） | **① 已对拍的小内核**：真正重叠的只有 `process_file`（`:202-350`）↔ `read_new_lines`（`:1029-1098`）+ `ReadCursor` + `SeqCounter`，**约 100 行**，且 daemon 侧 `:1008` 逐字写着 `Mirrors ../src-tauri/src/watcher.rs process_file`，两侧测试同名同义逐条对拍 |
| 用量 | `usage.rs`（353 生产行） | `usage_query.rs`（345 生产行） | **② 真同语义双写**（`usage_query.rs:10-15` 自陈「改口径必须同步改本地 `usage.rs`」） |
| 账号 | `local_accounts.rs` | `accounts_query.rs` | **② 真同语义双写**，已有跨源守卫 `contract_matches_the_daemon_implementation` |
| 判活 | `session_map.rs:461-516`（`#[cfg(windows)]` Win32）· `:517-520` **非 Windows 恒 `false`** | `watcher.rs:1417-1429`（`/proc`）· 非 Linux **恒 `true`** | **③ 互补平台残桩，零行逻辑重叠**。两个恒定分支还**互相取反** |
| tmux 观测 | `tmux.rs`（monitor 自己开 ssh 跑 `tmux ls`） | `watcher.rs:415` + hook | **③ 本机 Windows 上不存在**（无 tmux） |
| 起会话 | Rust 拼 PowerShell | TS 渲染器 **或** `shared/ccm` | 三份，本区正题 |
| 停 / send-keys | 无 | monitor 开一次性 ssh | 单边 |

> **三类的收益与风险完全不同**（v1/v2 混成「七组双写」会误导排期）：
> ① 是全仓最有守卫的一对，收益最低；② 是真该合的；③ 合并 = **写新实现**，不是删重复。

`TMUX_LS_FMT` 双写点、跨语言漂移、会话名分歧，是这张表的下游症状。

---

## §0.2 「daemon 只读」这个词今天已经在骗人

`readonly_guard` 的真实判据（`readonly_guard.rs:61-73`）= 源码里不许有 11 类**文件系统写**模式。**它不认 `Command` / `spawn`。**
daemon 今天已有的副作用：`tmux_hook.rs:107` 跑 `tmux set-hook -g`（改 tmux server 状态）·
`fork_write.rs` 真写文件（`O_EXCL`，唯一白名单）· `watcher.rs:416/450` 起子进程。
而 `INVARIANTS §41.6` 散文写的是「不许改动**用户既有数据**」—— **护栏与散文说的不是一件事。**

本区的答案不是「放宽只读」，是 **§1.1-3 护栏跟着模块边界走** + **§5-D1 的显式裁决**。

---

## §0.3 当前事实（✅ = 主线程亲自复核）

| 事实 | 值 / 位置 |
|---|---|
| 「起/停/接会话」独立路径 | **19 条**，无一经 daemon |
| unify-launch 的验收面 | **12 个「起会话」入口** —— 与 19 **不同口径**，见 §0.4 |
| ✅ daemon 在 Windows 编不过 | `cargo check --target x86_64-pc-windows-msvc` **RC=101 / 11 错**；**`--all-targets` 是 12 错**（多 `watcher.rs:2392 libc::getuid`）。全在 `watcher.rs:156-166`+`211-262` |
| ✅ 那 11 个错**一个 cfg 都不涉及** | `pidfd_open`（`:156`）是**无条件编译**的 Linux-only 代码 ⇒ **cfg 位置扫描抓不到它** |
| ✅ `pid_alive` 非 Linux 恒 `true` | `watcher.rs:1422-1428`（静默错误地雷） |
| ✅ `session_map` 非 Windows 恒 `false` | `session_map.rs:517-520`，而 `watcher.rs:214` 拿它做 active 过滤 ⇒ Linux 宿主上本机会话全被滤掉（v1 仅 Windows 的已知缺口） |
| ✅ Linux 宿主远端终端拉起 100% 失败 | `launch.rs:304` 无条件调 `launch_powershell_window`，后者非 Windows `Err`（`:268-271`） |
| ✅ `no_timer_guard` 登记表 1 条 · **且扫描非递归** | `no_timer_guard.rs:63-68` / `:87-108`（单层 `read_dir`，目录被跳过） |
| ✅ `readonly_guard` 已递归 | `readonly_guard.rs:152-168`，Phase G 修的，注释逐字预演了非递归的失效场景 |
| ✅ 三个守卫共用坏 marker | 八处 `"\n#[cfg(test)]\nmod tests"`，而 `main.rs:183` 是 `mod stream_flag_tests` ⇒ **main.rs 的 284 行测试段今天正被当生产段扫** |
| ✅ 内嵌 daemon 处于「半 bump」 | 源码 `main.rs:132` = `p1v-attachable`，`embedded-daemons/*.build_id` = `p1u-fork-session`；`build.rs:262-266` 只 `cargo:warning` |
| ✅ `parity_ledger` = 123 | `local-as-remote/MASTERPLAN.md:119` 写的 120 已过期 |
| ✅ `ccm-aliases.sh` 只有 3 个别名 | `unify-launch/MASTERPLAN.md:252` 承诺的 8 个从未存在 |
| ✅ `__ccm_rbind` 无定义却有 10 处引用 | 含 `launch-plan.ts:92` 拿它给 IR 的 `prelude` 字段做说明 |
| `watcher.rs` 规模 | 3837 行中**生产段 1772 / 测试段 2065（76 个 `#[test]`）** |
| `shared/ccm` 规模 | 662 行中**非注释非空 341 行** |
| **E79（我这轮自己做的）** | `local_accounts.rs:583 list_local_session_accounts`，`lib.rs:992-993` 注册，`parity_ledger` 的 123 含它的 +1。**v1/v2 的病灶表漏了它** |

### 会话名生成器 —— 两族五处，族内各一处真分歧

| 族 | 生产者 | 输出 | 状态 |
|---|---|---|---|
| cwd → 名 | `ccm:254` / `remote-launch.ts:172` | `<safe>-cc` | 逐字同规则，`e2e/ccm-cli.test.sh:199` 真值对拍守着 |
| | `cc-spawn:81` | `<bn>_cc` | ⚠ **真分歧、无守卫**（下划线、不折叠、不截 32） |
| sid → 名 | `remote-launch.ts:157` | `<sid8>-cc` | |
| | `resolve_query.rs:251` | `cc-<sid8>` | ⚠ **真漂移，对着 aterm** ⇒ 现网 bug |
| | `launch-requests.ts:67` | `cc-<sid8>` | 走不到的遗留默认 |

---

## §0.4 与 unify-launch 的关系（19 vs 12）

unify-launch 统一的是「**命令串怎么拼**」：15 套拼法 → 2 个渲染目标 + 1 个 CLI。
真做成的：F02（`~/.bashrc` 4 个 block / 187 行 / 4 套并存 → 一份 `shared/ccm`）· F03（IR + 维度注册表）·
F01（`-t` 全改 `=名:`，修了正在杀错会话的真 bug）· F04（`@ccm_sid` 两通道 + 三道门）。
明确划在界外的：本地两条 · SFTP 开终端 · 账号 step · **daemon 侧零改**。

**19 vs 12 的差**是口径：我数的含「停 + 接 + 观测 + 只产计划 + 探测」以及之后新增的 fork。
**但三条是真残留**：本地两条 · 用量探针（IR 之外第 6 个手拼 builder，E23）· `cc-spawn` 会话名分歧。

**根本区别**：unify-launch 收敛了**表达**，但「谁来拼」还是三个（TS / Rust / bash）⇒
结构上消灭不了跨语言双写点，只能靠守卫钉住。本区收的是「谁来拼」。

---

## §0.5 计划自审打掉了什么（我错的地方，逐条留档）

| # | v1/v2 写的 | 实际 | 影响 |
|---|---|---|---|
| 1 | 「五套 `ccm-*` e2e **无最小断言数地板**」，列为必须最先做的 U0 | **全都有**，而且是运行期 PASS 数地板 + fail-closed（`e2e/assert-pass-floor.sh`），CI 还有一道逐对校验的元门禁（`ci.yml:310-326`，15 套）。这是 `gate-integrity` 早已收官的 G-A/G-C | **第一梯队本来是空的**。病根：把摸底 C 转述的陈旧 BACKLOG 条目（E11）当现状，而我这轮开头才读过写着「✅ 全部完成」的工作区索引 |
| 2 | 「`ccm-rbind-title` 实测 6，CI 写 8，对不上」 | **实测 8**（子 agent 实跑 `合计 PASS=8`；另一路逐行推演同为 8）。「6」= `grep -c '\bok\b'` 的伪计数（把 helper 定义行算一次，把 4 元循环算一次） | 按 6 去「核清」= **把地板松 2 条** |
| 3 | U2 机检「`platform/` 之外出现平台 cfg 就红」 | **安慰剂**：`pidfd_open` 根本没有 cfg，11 个错一个 cfg 都不涉及 | 真判据只有跨 target 编译，且必须 `--all-targets` |
| 4 | S11「ccm 掏空后 `ccm_cli_has_required_elements` 空转变绿」 | **反了**：它已有 `require(10)` 计数自检（`structural_scan::require` 对 `min_checked==0` 硬失败）+ `pin_definition`，掏空后**四路一起红** | 真风险是**重写时降强度**，U1 的 DoD 要改写 |
| 5 | 开放-4「只有非 tmux 路径失去 OSC marker」 | **错**：`set-titles-string '#{?@ccm_sid,…}'` 读的正是 poller 写的 `@ccm_sid` ⇒ 删了 poller **tmux 路径一样瞎** | U9 的「保住」清单必须补 `@ccm_sid` 回填 |
| 6 | `platform` / `observe` / `control` 三分 | **不够**：`projects_root` 四份逐字相同，其中 `fork_write.rs` 属 control、另三个属 observe；`proc_starttime` 在 crate 内就有两份（`watcher.rs:1435` vs `accounts_query.rs:311`） | 加 `common/` |
| 7 | §1.1-2「`observe/` 与 `control/` 互不 import 对方内部」 | **第一天就过不去**：`watcher.rs:822` 在 `watch_loop` 里调 `install_tmux_hooks_best_effort()` → `tmux_hook::install_hooks`。hook 活在 server 内存、每次重起要重装，而「server 起来了」只有 observe 知道 | U3 必须先在两种形态里选一个 |
| 8 | D2 / E15「未选账号 = 不注入 还是 强制基座」列为待拍板 | **答案早在代码里**：`history.rs:869-870` 逐字写着「**显式** `unset CLAUDE_CONFIG_DIR`（不是「什么都不加」——那会被 shell rc 里的 `export` 顶掉 = 静默串号）」 | D2 降级为「确认既有选择 + 写进文档」，**不是开放问题** |
| 9 | 病灶表「七组双写」 | 分三类后**真重复只有两组**（用量 · 账号）；jsonl tail 是已对拍的 ~100 行；判活/tmux 是互补残桩 | 排期口径 |
| 10 | 未提 E79 | 是我这轮自己做的（`8a0b6e4`），正是 Windows 账号那一格的本机实现 | 病灶表补一行 |

---

## §1 架构

### 1.1 三条解耦线

1. **平台线** —— `platform/` 是唯一允许平台原语与平台 cfg 的地方。
   **判据不是 cfg 位置扫描（§0.5-3），是跨 target 编译**：
   `cargo check --all-targets --target x86_64-pc-windows-msvc` 必须绿，**进 CI**（在 ubuntu 上就能跑，`check` 不链接）。
2. **能力线** —— `observe/`（读）与 `control/`（控制）分模块。
   ⚠ **不是「互不 import」**（§0.5-7 证伪）：允许 **observe → control 的一条窄接口**，
   接口面必须显式列举并有测试钉住条数。反向（control → observe）不许。
3. **护栏跟着模块边界走** —— `readonly_guard` **不放宽**，改钉 `observe/`；
   `control/` 单立窄写护栏（白名单逐条列举 + 理由 + `create_new` 判据）。
   ⚠ 三条子判据必须**原样迁**，不许简化：`WHITELIST_STILL_FORBIDDEN` 13 模式 ·
   `WHITELIST_REQUIRED = ".create_new(true)"`（**带前导点**，N5 实测出来的）· `open_calls_are_all_exclusive` 配对计数。

**`observe/` 的「只读」定义（本区裁定，写进护栏文档）**：
指**不改文件系统、不改用户既有数据**。`tmux ls` / `tmux display-message` 这类**只读查询子进程**允许，
且必须在 `observe/tmux.rs` 里逐处登记；**任何改状态的 tmux 命令（`set-hook` / `set-option` / `new-session` / `kill` / `send-keys`）一律归 `control/`。**
⇒ `tmux_hook.rs` 归 `control/`，由 observe 经那条窄接口触发。

### 1.2 一份代码，两种生命周期

| | 本机 backend | 远端 backend |
|---|---|---|
| 从哪来 | **安装包的一部分**（Tauri sidecar —— `tauri.conf.json` 今天**没有** `externalBin`/`resources`，全是新增） | SFTP 推（`sftp.rs:408-476`） |
| 谁拥有生命周期 | **frontend 拥有**：启动 · 监督 · 崩了重启 · 退出收尾 | 自治：app 退出后**它得继续活着**才能观测 |
| 版本一致 | 构造上一致（同一个包） | `build_id` 判陈旧 + 自动重装 |
| 传输 | **必须是监听型**（unix socket / named pipe）—— **stdio 出局**，因为 `ccm` 是用户终端里独立起的进程、不是 frontend 的子进程（§0.5 自审 B3） | SSH channel |
| 「没装」这个状态 | 不存在 | 存在 ⇒ daemonless 只对远端有意义 |

**两条硬 DoD**：① 本机 backend 崩溃后 frontend 必须**自愈 + 如实提示**，绝不静默空列表；
② 远端那份的自治性**不许被「app 拥有」的逻辑污染**。

⚠ **绝不能把 Windows 加进 `build.rs:241` 的 `["x86_64","aarch64"]` 循环** ——
`:270-276` 任一缺失就不置 `embedded_daemons` cfg ⇒ `sftp::daemon_binary()` 返回 None ⇒
**远端 SFTP 自动部署静默关闭**。这正是 `release.yml:96-99` 记的 v2.19–v2.22 事故形状。
本机分发要走**另一条**打包链。

### 1.3 那条搬不动的物理边界

最后那次 `exec` 必须在用户那个终端进程里：PID 要等于 pidfile 文件名、tty 与 Ctrl-C 要落在 agent 上、
`tmux attach` 必须占据调用者终端。区别不在「那个进程存不存在」，在**它有没有决策权**：
今天 `shared/ccm` 自己算账号/名字/容器/载荷；之后只上报上下文 → 拿 argv → 设 env → `exec`。
**它是执行臂，不是宿主。**

⚠ **一个已知例外必须写下来**：**预信任的「等信任框」在零 `Duration` 下写不出来。**
「信任框出现了」**没有任何内核事件源**（inotify 看不见 tmux pane 内容），它本质就是轮询。
今天能与 `no_timer_guard` 共存，唯一原因是它是**一段被塞进 tmux 的 shell 字符串**、由目标 shell 执行。
⇒ `control/` **继续以 shell 字符串形态产出它**。这是「谁来拼」收敛的**已登记例外**，
不写下来，实现期必然有人用 Rust 重写然后撞 `no_timer_guard.rs:50-56`。

### 1.4 frontend 剩什么

**只剩「在用户桌面上开一个终端窗口」**。窗口里跑什么由 backend 给（沿用 `--resolve` 的 `CommandPlan` 形状）。
⇒ TS 两个渲染器整体退役；顺带修掉 Linux 宿主拉不起终端那个真 bug。

### 1.5 三条已定的技术决策

| | 决定 | 含义 |
|---|---|---|
| 下行通道 | **长连接双向** | 在既有 SSH 长连接上开反向请求。**决定性理由是陈旧可判定**：U7 之后每个读事实都同时有推路径与拉路径，多路 channel 之间没有全序 ⇒ 无法判断谁更新。这个仓已经为「一处重叠」付过价：`tmux_reconcile.rs` 一个模块 + 四道防误判 + 一个要编译期断言的时间常量（`RETIRE_MISS_THRESHOLD >= 2`）。⚠ **背压是代价不是好处** |
| 零定时器 | **超时一律推给客户端** | daemon 永远不等：命令收下即回「已受理」，结果靠既有内核事件推回。⇒ §41 铁律一字不改，登记表仍 1 条。**唯一例外见 §1.3 预信任** |
| Windows 账号 | **daemon 起的就记账** | 不需要 `/proc/environ`。⚠ **残缺不是边缘是窗口**：U7 退役 `local_accounts.rs` 而 daemon 要 U8 才会起会话 ⇒ **U7↔U8 之间 Windows 上全部会话账号未知**。⇒ U7e（账号）必须排在 U8 之后 |

**双向通道的两个坑**（自审实测）：
① `into_stream()` 只搬 `ChannelMsg::Data`，**stderr 与 exit status 永久拿不到**（`ssh_source.rs:1566-1572`）
⇒ 控制面的错误只能走 stdout 的 wire 帧，而 §4.2 #19「stdout 只跑 wire」在 `control/` 起子进程后更脆、**今天没有任何机器护栏**。
② **未协商前往 stdin 写会挂死 monitor 自己**（旧 daemon 从不读 stdin，写满 channel window 后 `write_all` 永久 pending）
⇒ 「见到 `Hello.capabilities` 之前一个字节都不许写」**必须做成机检**。

---

## §2 ★共享面账本

| # | 共享面 | 最终形态 | 功能 |
|---|---|---|---|
| S1 | **护栏的扫描面**<br>**U-1 已交付**：`no_timer_guard` 递归 + **字节地板与数量相等双判据**（单靠字节挡不住单文件被剥空）；`readonly_guard` 的**两条过剥（fail-open）已修**——锚点钉行首 + 无花括号体声明不吃后文，扫描面 217_853→221_928；欠剥方向新增机器判据 `no_test_code_leaks_into_any_production_section`。**遍历未收敛**（daemon 5 份 + monitor 1 份），留 U1a | 剩余：`readonly_guard` 钉 `observe/`；`readonly_guard` 钉 `observe/`；`control/` 窄写护栏（三条子判据原样迁）；`platform/` 与顶层谁管**必须写明**（今天改钉 observe 后是真空） | U-1 · U1a · U1b |
| S2 | **守卫的测试段 marker**<br>**U-1 已交付**：`guard_support.rs`（`#[cfg(test)]`-only，`pub(crate)`）收敛了 3 个守卫的剥法 + `assert_no_test_code` 自检。⚠ **`readonly_guard` 仍是第二套且不能 naive 换**——它能剥 `#[cfg(test)]` **自由函数**（`history_query.rs:232/309`，两者对该文件差 1246 字节），换过去会让它变弱；要收敛得做**并集剥法**，留 U1a | 原病灶：三个守卫共用的 `"\n#[cfg(test)]\nmod tests"` 对 `main.rs`（`mod stream_flag_tests`）不匹配 ⇒ 抽成一个共享 helper + **能真正检出「没剥掉测试段」的自检**（现有的 `main_prod.len() < main_raw.len()` 光靠剥注释就满足） | U-1 |
| S3 | **平台原语**<br>**U2 交付的是「目标归属地」+ 把 11/12 个 Windows 编译错集中到 `platform/pidwatch.rs`，**不是收口**。已收：`pidfd_open`/`watch_pid_until_exit`（切开了 observe 回边）· `/proc` 一族 9 项 · `path_key`（单列 `platform/paths.rs`，U4 加 Windows 分支时零二次搬）· `proc_claude_config_dir`（Phase D 审计要求补收）。<br>⚠ **生产段还有 4 处在外**：`tmux_hook.rs` 的 `libc::kill`（§1.1 已裁定 tmux_hook 归 control ⇒ **U3 连它一起处理**）· `main.rs` 三处（组装根，可辩护为留，**但不能因此说「唯一」**）。<br>⚠ **U4 伏笔**：`is_same_live_process` 那张判定表要上提到 `platform/liveness.rs`（Windows 判活复用它），别留在按 `/proc` 命名的模块里 | U2 · U3 · U4 |
| S4 | **`observe/ → control/` 的窄接口**<br>**U3 已交付**：`layering_guard.rs` 两条机检 —— 反向零容忍 + 正向符号集**恰好等于**登记表。今天登记表**只有一条**：`crate::control::tmux_hook::install_hooks`。<br>摸底时真有一条反向边（`fork_write` → `accounts_query::read_regular_capped`），**没开例外** —— 那个函数不是 observe 的域逻辑，搬进 `common/fs.rs` 后边自然消失。铁律 6 的正例。<br>⚠ 加登记项前先答：**为什么这件事非得由观测侧发起、control 能不能自己做**（`install_hooks` 的答案：不能 —— 触发时机是「tmux server 起来了」，那是 socket inotify 观测到的事实，control 侧没这个信号，硬要它自己发现只能靠轮询，与 §41 正面冲突） | U3 ✅ |
| S5 | **`common/`**<br>**U2 已交付**：`projects_root`（**5 处不是 4 处** —— 第五处是 `watcher.rs::watch_loop` 里内联的，`grep fn` 找不到）· `mtime_ms`（2 份）。门槛写在 `common/mod.rs`：≥2 **层**用 · **平台无关** · 无域知识。<br>⚠ **「时间换算 · quote」这两项要划掉**：Phase D 审计逐条核过，daemon crate 内**没有可合的逐字副本** —— 时间换算三处语义/单位各不相同（`file_mtime_epoch` 单位是**秒**不是毫秒），quote 在 daemon 内只有一份、多份在 monitor 侧**跨 crate**、`common/` 收不了。<br>⚠ **U3 必须复查**：`mtime_ms` 的两个调用点同属 observe，「≥2 层」按层口径**今天不成立**；`observe/` 一建出来就要重判 | U2 · U3 |
| S6 | **wire 协议 + `IPC-PROTOCOL.md`** | 双向；**文档先修再冻结**（该文件 7 处在说谎，见 §3-U6）；wire 字段名双向 `include_str!` 对拍 | U6 |
| S7 | **daemon argv 面** | 三类；`split_stream_flags` + `every_capability_token_is_strippable` 同步扩。⚠ **起点比以为的更糟**：现有的二分表本身就漏了 5 条子命令（`--tmux-notify` / `--resolve` / `--fork-session` / `--account-trust-zero` / `--read-session-from-offset`），其中 `--account-trust-zero` 漏登记**出过 v3.4.0 事故** | U6 |
| S8 | **`BUILD_ID` 单源链条**<br>**U-1 已交付**：① `ssh_source.rs::embedded_build_id_single_source_wired` 断言 ≠ `"unknown"`；② `build.rs` 三条硬 panic（抠不到源码 `BUILD_ID` / 有二进制但缺清单 / 清单与源码不符）；③ 半 bump **真修掉**（两个 arch 从 p1v 源码现编，`rust-lld` 零安装）。<br>⚠ **措辞订正**：原写「缺文件从 warn 改 fail」不准 —— 三条 panic 都以「`embedded-daemons/` 里真有二进制」为前提；**整个目录缺失时仍是优雅降级**（那是 dev/CI 常态），兜那一档的是①不是 `build.rs`。发版链两头都够得着（`release.yml:56-58` 写清单、`:113-118` 再对拍） | U-1 · U13 |
| S9 | **读面七组 → 四组**（§0.1 三类） | monitor 侧退役 | U7a–U7e |
| S10 | **`shared/ccm`** | 零决策执行臂。**保住**：三个 exec 出口 · `eval "$CCM_ENV"` · `--detach` · `--ccm-probe` 首行 · codex `CC_BUS_ID` 无条件覆盖 · **`@ccm_sid` 回填（§0.5-5）** · `ccm:264-279` 的撞名分叉 | U9 |
| S11 | **`sftp.rs::ccm_cli_has_required_elements`**<br>**U1a 已交付前半（基线）**：强度读数由 `ccm_cli_contract::measure()` **单一产出**，迁移前后同一函数跑两份脚本文本 —— **`needles`/`channel_a`/`t_targets_checked` 三个 `>=`，`t_violations` 是 `<=`（必须 0）**。⚠ 最后这半句不能少：Phase D 审计实测，只比前三个字段时「4 处精确目标全改裸目标」四字段一字不变、全绿。5 条**编译期**钉子（`const _: () = assert!`）钉住基线与阈值两个旋钮。<br>**U9 的迁移清单（现在写死，免得只搬一半）**：`measure()` 喂新构造点文本 · **`require()` 必须一起搬**（它看 violations，读数是它的镜子不是替身）· `pin_t_def()` · `doc/INVARIANTS.md:686` 的指向 | 迁移后逐条对拍 = U9；护栏按新边界重钉 = U1b |
| S12 | **会话名生成** | 两族各一个函数 + 计数守卫 == 2。⚠ **守卫挂 U11 不是 U8**（ccm 到 U9、cc-spawn 到 U11 才收编，挂 U8 是**做完必红**的 DoD） | U8 · U9 · U11 |
| S13 | **`parity_ledger`**<br>**U-1 已交付**：`command_signatures()` 改递归 + `files.sort()`（不排序时「首个胜」会让同名命令随文件系统顺序漂）。实测 `adapter/` 下两文件的 `#[tauri::command]` 命中数都是 **0** ⇒ 递归当下是**纯预防性**，`LEDGER.len()==123` 与 `checked==68` 都没变 | 本区天然验收面。⚠ 它只钉命令**这一层**，别把「数字没动」读成「读面没搬成」 | U-1 · U7 |
| S14 | **`--resolve`** | 吸收进 backend 的计划面；线上形状逐字不变（aterm 契约 2026-07-18 冻结）；`sessionName` 漂移随之消失 | U6 · U8 |
| S15 | **本机分发链** | Tauri sidecar，**与 `embed_daemons` 完全另一套**；⚠ `sftp.rs:1262` 的 `assert_eq!(…, b"\x7fELF")` 与 `:1252` 的 arch 表会被 Windows PE 打红 | U5 |
| S16 | **`npm test` 套件链 + tsx 套件登记表**（U0 新增）<br>**U0 已交付**：`src/node-suite-registry-guard.vitest.ts` 六条判据（条数 / 全仓总量地板 / 集合 / 路径 / 链路+`&&` / 失败收尾）。⚠ **U8c 退役两个 TS 渲染器时必须同步改四处**：`NODE_SUITES` · `TOTAL_FLOOR` · `package.json` 的 `test:*` 定义 · `npm test` 链。被碰到的是 3 个套件：`test:launch-render-cli`(26, **整删**) · `test:launch-dimensions`(28, **整删**) · `test:remote-launch`(40, **改不删**)。只删一半会当场红 —— 这正是本条要的效果 | U0 · U8c |

### 跨工作区冲突协议

- **`branch-anywhere/`**：Phase G 已完成、**只剩发版**，下一步是给 aterm 发 `--fork-session` 契约冻结通报。
  U11 会动这个刚要冻结的两仓契约 ⇒ **U11 之前先对账**；R-4 的 aterm lockstep 清单要补上 `--fork-session`。
- **`account-onboarding/`**：红线逐字「daemon 起会话机制零改」+ 待做 F6 终端起号 / F7 用量 ⇒ 与 U8/U9/U10 正面冲突。**该区是否仍活着要先确认。**
- **`auto-e2e/`**：已交付的 5 套 e2e **全部断言现行启动与读取架构**（含「GUI-触发 resume 在 Linux 结构性不可达」这条被写进 e2e 的事实 —— 而 §1.4 正要修掉它）。
- **`daemon-codex/`**：`--resolve` 同向，U6 前对账。
- **issue #82**（`tmux -C` 住进 daemon）：与本区同向，但其正文引的 `TMUX_EMIT_INTERVAL = 8s` **已在 P5 删除** ⇒ 前提过期，本区收官前不动。

---

## §3 功能清单与顺序

### 第零梯队 · 修当下就坏着的门禁（**新增，v3 加**）

| # | 功能 | DoD |
|---|---|---|
| **U-1** | **护栏当下缺陷修复** | ① `no_timer_guard::daemon_sources` 改递归（照抄 `readonly_guard::scan` 的栈），地板从 5 棘到实际文件数；② 三个守卫的测试段 marker 抽共享 helper + 自检要能真正检出「没剥掉测试段」；③ `parity_ledger::command_signatures` 改递归；④ 加 `DAEMON_BUILD_ID != "unknown"` 断言；⑤ 修掉当前的半 bump（re-zigbuild 内嵌二进制或让 `build.rs` fail）。**每条配变异验证** |

### 第一梯队 · 门禁（**U0 重定义**）

| # | 功能 | DoD |
|---|---|---|
| **U0** | **地板的脆点与孤儿** | ① 修 `ccm-cli.test.sh:206` 的 `command -v npx` 脆点（无 npx ⇒ PASS=39 < 地板 44 ⇒ CI 红但诊断写成「断言数缩水」，实为环境缺失）；② `graylight-suite` / `f40-suite` 两个孤儿套件明确处置（接线或删）；③ **16 个 `*.test.ts` / **242** 例（原写 241 —— 那是 e2e 地板合计，串号了）既无断言地板又被 `coverage.exclude` 排掉 —— 双重不设防**（E64①），补一条；④ 订正计划自己的 6/8 误记。**不再是「加地板」** |
| **U1a** | **守卫强度对拍（今天可做的一半）** | `ccm_cli_has_required_elements` 迁移前先把强度基线记下来（needle 数 / `require` 的实际 checked / `pin_definition`），作为 U9 迁移后的对拍表。**不是「加计数自检」——它已经有了** |

### 第二梯队 · backend 内部解耦（纯重构，行为逐字不变）

| # | 功能 | DoD |
|---|---|---|
| **U2** | **抽 `platform/` + `common/`** | 收 `pidfd_open` · `spawn_pid_watcher` · `pid_alive` · `proc_starttime`（**含 `accounts_query.rs:311` 那份重复**）· `proc_cmdline` · `parse_btime` · `path_key`；`common/` 收 `projects_root`×4 / `mtime_ms`×2。⚠ **`session_alive` 是 platform↔observe 的回边**（`spawn_pid_watcher:228` 调它），先定形态：下沉 platform 还是谓词参数化。修 `pid_alive` 地雷 |
| **U3** | **拆 `observe/` + `control/`** | `tmux_hook` 归 `control/`（§1.1 裁定）；定 `observe → control` 窄接口形态；`readonly_guard` 改钉 `observe/`（会当场红 `whitelisted==1`，逼出 control 护栏）。**验收：行为逐字不变**，全量 e2e 绿 + wire 输出逐字节对拍 |
| **U1b** | **护栏按新边界重钉** | 三条白名单子判据原样迁；`platform/` 与顶层的 fs 写归谁**写明**；`default_scanned` 地板按实际文件数重设 |

### 第三梯队 · Windows backend

| # | 功能 | DoD |
|---|---|---|
| **U4a** ✅ | **backend 在 Windows 编得过**（U4b 拆出去了，见下）| `cargo check --all-targets --target x86_64-pc-windows-msvc` 绿（**12 个错清零，不是 11**）**并进 CI**（ubuntu 上跑，`check` 不链接，成本近零）；`WaitForSingleObject` 换 pidfd（**等价性仓里无实测，第一步先验**）；判活/procStart 双格式。⚠ 搬 `session_map.rs` 的 Win32 实现要加 `windows` crate（daemon 今天零 windows 依赖，`windows-sys ≠ windows`）；⚠ daemon job 先加 `--locked` |
| **U4b** | **backend 在 Windows 跑得对**（U4a 实现期拆出：DoD 自己写着「等价性仓里无实测，**第一步先验**」，而那个验**必须在 Windows 真机上做** ⇒ 对应 STATUS 停止条件第 4 条）| `pidwatch/fallback.rs` 的诚实空壳换真实现（`OpenProcess`+`WaitForSingleObject`，要加 `windows = "0.56"`）· `pid_alive` 的 `unimplemented!()` 换 `OpenProcess`+退出码 · **等价性真机验**（开放-1）· `send_sigusr1` 的 Windows 等价物 · `layering_guard` 登记表的 cfg 措辞 |
| **U5**（**范围收窄，不是砍掉** —— 我 2026-08-01 一度误读成「整条划掉」并擅自换成一个新功能，用户澄清：他说的是**本机那套他自己起**，要我做的是**走查两类用户的使用流程**）<br>⇒ 保留：daemon 在本机可用、可被手工起。**去掉**：app 的启停监督 / 崩溃自愈 / sidecar 自动化 |
| **U5-走查** | **新用户 / 旧版本用户两条使用流程走查**（用户 2026-08-01 原话：「我只是说你要模拟新用户和旧版本用户的使用流程」）|  **今天的实况（2026-08-01 实测）**：`tauri.conf.json` 的 `bundle.resources` 与 `externalBin` **都是空的** ⇒ `.deb`/`.msi` 里**只有 app 二进制**。三个受管工具没有一个能装到本机：`ccm` 的 destination 是 `RemoteHomeRelative` + `HostScope::Remote`（只往远端装）· `cc-bus` 是 `LocalHomeRelative` 但 **`installable: false`**（app 根本装不了）· `cc-acct-iso` 的实际部署走远端 SFTP。**⇒ 装新版之后本机一个工具都不会被安装或更新**，这正是开放-6 的一般形态（用户报的「terminal 里敲命令还是 attach」只是它的一个症状）。<br>**DoD（用户设的前提）**：装完新版之后，用户**自己就能把本机那套搭起来** —— ① 三个工具的本机安装路径（app 里一个动作，或包里带、文档写清怎么放）② 装完能**看出版本对不对**（今天连「你本机那份是 2026-07-27 的」都无处可查）③ 文档写清手工步骤，且步骤可照做（不是「见某某」的转指）<br>**不做**：不做 sidecar、不做启停监督、不做崩溃自愈 —— 用户明确说那部分他自己搞 |

### 第四梯队 · 协议

| # | 功能 | DoD |
|---|---|---|
| **U6** | **双向 wire + IPC-PROTOCOL 先修再冻结** | ⚠ **文档先修**：`IPC-PROTOCOL.md` 563 行里 §2/§3/§7/§9/§10/§10.1/§11 + 握手时序图**七处在说谎**（时序图画的正是已修掉的 v2.21 竞态 bug；帧字段表漏 8 个线上字段含刚冻结的 `attachable`；一次性查询表漏 5 个子命令）。**拿它当冻结基线 = 把错误固化进新协议。**<br>然后：request-id · 取消 · 背压 · `version` + 能力协商 · 主键 opaque 稳定（issue #48 单向门）· argv 三分 + 死循环护栏扩 · `--resolve` 吸收 · **wire 字段名双向 `include_str!` 对拍**（没有它，那 8+5 条漏列一条都发不出来）· **「见到 Hello 之前不许写 stdin」做成机检** · **命令处理器一律不许跑在流线程上**（抄 `run_tmux_ls` 头注的纪律） |

### 第五梯队 · 读面合流（**拆开，一条一条**）

| # | 功能 | 备注 |
|---|---|---|
| **U7a** | jsonl tail 内核（~100 行，已有两侧对拍测试） | 最低风险，先做，**同时验证本机 backend 那套管道**（解掉批次③「交付一个没有消费者的进程」） |
| **U7b** | tmux 观测（**仅 Linux 宿主有意义**） | |
| **U7c** | 用量 | ⚠ 踩 E42：`/usage` 解析**从未真机验证** |
| **U7d** | 判活 | ⚠ 不是删重复，是**写新实现**（两个恒定分支互相取反） |
| **U7e** | 账号 | ⚠ **必须排在 U8 之后**（§1.5 的窗口问题） |

**两处未声明的行为降级，必须在 U7a 显式处理**：
① daemon `process_jsonl` 每次事件**整文件读**（`watcher.rs:1110 std::fs::read`），monitor 是**增量 seek**（`:268`）⇒ O(delta) → O(file)；
② daemon 走 `FrameSink` **通道满就丢帧**（`:1718-1727`），monitor 是同步 `on_batch` **无损**，且专为 `/resume` 灌历史做过大小分流（v2.4.2 issue #2 的修复）⇒ **搬过去等于撤销那个修复**。

### 第六梯队 · 控制面（**拆开**）

| # | 功能 | 备注 |
|---|---|---|
| **U8a** | `control/` 起会话 RPC（三种容器形态各自独立行为） | ⚠ `send-into` 是 `ccm` 表达不了的唯一形态，**渲染器退役后必须有承接方**，#76 防线以新形式保住 |
| **U8b** | frontend 按 OS 分的开窗实现 | 顺带修 `launch.rs:304` 那个真 bug |
| **U8c** | 两个 TS 渲染器 + IR 退役 | 牵 E24（五个导出生产零调用点却塑造了 IR 类型） |
| **U8d** | 换号重启编排（`account-restart.ts`）+ SFTP 开终端 + 账号 step 的归属 | v2 遗漏的三条路 |
| **U9** | thin ccm 零决策执行臂 | S10 的保住清单；预信任仍以 shell 串形态由 `control/` 产出（§1.3 已登记例外） |
| **U10** | 停 / 接 / send-keys / 用量探针 | 三道门原样保住；⚠ **TOCTOU 不会因为同机而消失** |
| **U11** | 分叉 + cc-spawn 收编 + **会话名计数守卫落地** | ⚠ 先与 aterm 对 `--fork-session` |

### 第七梯队 · 收尾

| # | 功能 |
|---|---|
| **U12** | daemonless 处置（读面留，文档收窄适用面） |
| **U13** | **仓库级重命名**（`remote-daemon-proto` 46 个文件 / `cc-monitor-remote` 31 个文件 + `build.rs` 4 处硬路径 + `release.yml` 2 份正则副本 + 6 个 e2e 脚本 + UI 4 处）。**独立功能，不是文档收尾。**⚠ 必须先有 S8 的 `!= "unknown"` 断言。⚠ **仓库级命名观察（2026-08-01 顺带查出，尚未纳入范围）**：仓里四套命名并存，`monitor`(crate) / `cc-monitor`(productName) / `ccmonitor`(identifier) **三处三拼**；目录 `remote-daemon-proto/` 与包名 `cc-monitor-remote` 不符；且统一之后 `remote` 会变成**假话**（同一二进制既是远端后端也是本机后端，见 §1.2 + U5），`proto` 也早已不是原型。⇒ U13 动手前需用户就此单独拍板。**tmux 会话名不在此列**——已定 `-cc` 后缀且 2026-08-01 闭合（§5 D3） |
| **U14** | 文档全局一致性 + 索引（**各功能的文档改动已分散进各自 DoD**，这里只收口） |
| **U15** | Phase G |

### 依赖图

```
U-1 ─► U0 ─► U1a ─► U2 ─► U3 ─► U1b ─┬─► U4 ─► U5 ─┐
                                      │              ├─► U6 ─► U7a ─► U7b ─► U7c ─► U7d ─┐
                                      └──────────────┘                                    │
   ┌────────────────────────────────────────────────────────────────────────────────────┘
   └─► U8a ─► U8b ─► U8c ─► U8d ─► U7e ─► U9 ─► U10 ─► U11 ─► U12 ─► U13 ─► U14 ─► U15
```

**每批的可交付中间价值**（v3 补，v2 的批次③曾是净负值）：
① U-1/U0/U1a：门禁真的在守 · ② U2/U3/U1b：daemon 内部可读、护栏钉准 ·
③ U4/U5 **+ U7a**：本机 backend 有真实数据流经它，崩溃自愈可验证 ·
④ U6/U7b-d：读面四组双写折掉三组 · ⑤ U8-U11：控制面 · ⑥ U12-U15。

---

## §4 横切约定

### 4.1 红线
不改 `TMUX_LS_FMT` / `RETIRE_MISS_THRESHOLD` · **`no_timer_guard` 登记表不许为过关而 +1，
也不许为过关而缩小扫描面**（v3 新增后半句 —— 自审证明前半句单独是不够的）·
`readonly_guard` **收窄不删** · 不写用户 `settings.json`/`.bashrc`/PS profile/`.tmux.conf` ·
不碰 `~/.claude-accts/` 真实文件 · **绝不把真实对话内容拷进本仓** · 不装包 ·
不改 workflow **触发条件**（改 job 内容可以）· 绝不启动真实已认证的 claude/codex ·
**绝不碰 `~/.cc-monitor/bin/` 那个在跑的 daemon 进程** · commit 不加署名、显式 `git add` ·
tmux 私有 socket（`unset TMUX` + `-L`），裸 `tmux kill-server` 是禁用词。

### 4.2 20 条正交约束 → 必须落 DoD
见 [摸底 D](PHASE-A-摸底-D-文档改写清单.md) §二。**v2 只把它们列在散文里，v3 要求做成「功能 × 约束」矩阵**，
每条至少落一个 DoD 单元格。自审点名 **8 条今天零承接**：#2（绝不向用户其它 tmux 发按键 ——
同机后风险更高）· #6（`remote_active` 唯一写者 —— 本区**新增第五个信号源**）· #7 · #13 保守缺省族 ·
#14 `kind` 排他式契约 · #15 seq 单调 · #16 at-least-once 幂等 · #18 data dir 归 `data_paths.rs`（U5 必然新增）。

### 4.3 审计强度
每功能 D/E 高风险档 + **至少两轮**：第一轮四视角并行 → 修 → **第二轮专查「修复本身 +
有没有制造新双写点 / 让哪个守卫空转」** → E 主线程对账 + 1 个聚焦 agent。
**变异纪律**：一律退出码判定；改完先 `grep` 计数确认变异真落地；Rust 侧再核「红得对不对」
（rc=101 也可能是编译失败＝假红）；**变异报绿先怀疑夹具**。

### 4.4 硬门槛
① 跨 target `--all-targets` 编译进 CI；② `no_timer_guard` 扫到的 `.rs` 数 == `find src -name '*.rs'` 计数；
③ `readonly_guard` 在 `observe/` 上零豁免；④ 登记表条数不变**且**扫描面不缩；
⑤ 会话名生成点 == 2（**U11 验收**）；⑥ `parity_ledger` 同步登记；⑦ `--print` 逐字节 parity；
⑧ **每个功能的 DoD 各带一条文档面 + 一条工程面**（用户 2026-08-01「文档工程和代码工程都要管理好」）。

### 4.5 lint 缺口（自审补）
clippy 全仓 **advisory 无 `-D warnings`**、无 `clippy.toml`、无 crate 级 `deny` ⇒
14 个功能全程唯一硬门是 `cargo fmt --check`。
⇒ **给新建的 `platform/` / `control/` 加窄清单 `#![deny(clippy::…)]`**（不追全仓清零）；
daemon job 加 `--target x86_64-pc-windows-msvc` 的 clippy（与 U4 的 check 同一步）。
`src-tauri/scripts/cc.ps1.tpl` 102 行 PowerShell **零静态检查**却被 `include_str!` 进二进制（E64⑥）—— 登记，本区不做。

---

## §5 尚待拍板（**只剩 1 条**）

| # | 问题 | 选项 | 推荐 |
|---|---|---|---|
| **D1** | **「间接写」算不算违反 §41.6？** daemon 一旦起 `ccm`/`claude`，**用户既有数据一定会被改**（CC 写 jsonl、重写 pidfile；`shared/ccm:44-46` 头注明写 `--tmux` 会顺带写 `~/.claude.json`）。机器护栏放行（它只认 fs 写模式），**散文铁律的字面意思不放行，CI 永远不会红** | ① 铁律收窄为「daemon **进程自身**不许写用户既有数据」，间接写不算 —— 但必须在 §41.6 明写这条边界，否则退化成「隔一层 exec 就绕过」② 铁律维持字面，则 `control/` 起 agent 这件事本身违规，本区不成立 | **①**，且**必须同时**：在 §41.6 写下「间接写的责任在被起的那个程序，daemon 的责任是不越权替它决定写什么」+ 把预信任那条（daemon **主动**让 ccm 去写 `~/.claude.json`）单列为**受管的例外**，逐条列举写面 |

**已由架构/代码闭合、不再是选项的**：
- D（旧）daemonless → 留（本机不存在「没装 backend」，只对远端有意义）
- D（旧）预信任归属 → `control/`，配窄写护栏 + 保持 shell 串形态（§1.3）
- D（旧）兜底渲染器 → 由构造消失
- D（旧）Windows `ccm.exe` → 不需要单独交付物
- **D2 / E15 → 答案早在代码里**（`history.rs:869-870` 显式 `unset`），降级为「确认既有选择 + 写进文档」
- **D3（命名）→ 用户 2026-08-01 已定：只管 tmux 会话名，后缀 `-cc`，其余命名一律不动。**
  查证结果：这条**早在 2026-07-31 S4b-3b 就反转过了**，`pickFreshTmuxName` / `deriveTmuxName` /
  `shared/ccm::derive_tmux_name` / `forkTmuxName` 都已是 `<X>-cc`。**但那次反转漏了一处生产生成点**：
  `launch-requests.ts::planResumeTmux` 的默认值仍是 `cc-<sid8>`。
  它当时没在线上冒出来，只因三条真实调用路径**碰巧都传了显式 name**
  （`tabs.ts:2173` 走 `pickFreshTmuxName` · `account-restart.ts` 复用既有名 · `fork-flow.ts:193` 有非空守卫）
  —— 是个**公开导出 API 的默认值**，下一个省略 name 的调用方就会静默拿到旧前缀。
  **2026-08-01 已修**，并加对拍断言把「默认名 == `pickFreshTmuxName` 基名」钉死（不再靠调用方都传参兜着）。
  连带修 9 处仍在描述旧形态的注释，其中 `remote-launch.ts::pickFreshTmuxName` 的头注**与它自己下一行的代码自相矛盾**。
  **`cc-` 前缀的识别半支保留不动**（`tmux.rs::is_ccm_tmux_name`，头注写明理由：F02 之前的老会话没有 `@ccm_sid`，
  删了就是把用户**正在跑的**会话变成 issue #76 那种失管会话）。`ccm-` 运行期命名空间与其余仓内命名**未动**。

---

## §6 风险

| 风险 | 缓解 |
|---|---|
| **R-1 护栏空转**（本仓栽过三次） | U-1 先修 `no_timer_guard` 递归 + marker + `parity_ledger` 递归；硬门槛② 用「扫到数 == find 计数」而不是一个魔数地板 |
| **R-2 协议是纯新建面，且冻结基线不可信** | U6 先修 `IPC-PROTOCOL.md` 七处谎言再冻结；wire 字段双向 `include_str!` 对拍 |
| **R-3 R08 的 5 条 e2e 断言会被推翻** | 它们断的是内层载荷的**语法形态**；ccm 自递归一去掉就失效 ⇒ 同步重写且**强度不许降** |
| **R-4 两仓 lockstep（aterm）** | `--resolve` · `attachable` · 会话名 · **`--fork-session`**（v3 补）。`sessionName` 漂移对 aterm 是现网 bug，**可先单独修**，不必等本区 |
| **R-5 半 bump** | U-1 先修掉当前这个；S8 写死「bump + re-zigbuild + 清单」一套做 |
| **R-6 死循环两个方向** | 旧 daemon 遇新参数误入流模式（抄 `abort_marker`）**与** 新 flag 落进 query 分支致无 hello 重连死循环（`every_capability_token_is_strippable`）—— **两个方向要两套缓解，别读成同一件事** |
| **R-7 规模** | 17 个功能 / 六批；每批都有可交付中间价值（§3 末） |
| **开放-1** | `WaitForSingleObject` ≡ `poll(pidfd)` 无实测 ⇒ U4 第一步验 |
| **开放-2** | `--print` 变 RPC 后「必须是纯的」靠什么保住 —— U9 给机制 |
| **开放-3** | E19（相对 `--cwd` 应用两次 + 产孤儿会话）搬家时**要么修要么明确带走** |
| **开放-4** | 非 tmux 路径的 OSC marker 确实拿不回来（daemon 无 tty）—— **如实降级**。tmux 路径靠 daemon 写 `@ccm_sid` + `set-titles-string` 合成 |
| **开放-6（2026-08-01 用户报「终端里敲命令还是 attach」实测查出）** | **`ccm` 在本机没有任何安装/更新路径 —— 只有远端有。** 受管工具注册表 `tool_registry.rs:251-263` 里 `ccm` 的 destination 是 `RemoteHomeRelative(".local/bin/ccm")` + `HostScope::Remote`：app 只往**远端**装。本机那份 `~/.local/bin/ccm` 是当初手工 `cp` 的，**之后仓里每一次 ccm 修复它都收不到**。<br>实测：仓里 662 行（2026-08-01）· 本机 542 行（2026-07-27），逐字节匹配到 commit `29bdda4`。用户报的病（同目录新开终端敲 `cct` 被 attach 进正用着的会话）**在仓里 `666cc14` 就修好了**，而本机跑的那份还是修复之前的「同目录 → 幂等复用」。私有 socket 实测：仓里那份连开三次得到 `myproj-cc` / `-2` / `-3`，避让生效。<br>⇒ **这不是「还没修」，是「修了送不到本机」。** 落点 **U5**（本机 backend 生命周期由 frontend 拥有 —— 本机分发链本来就是它的正题，账本 S15）；U9 让 ccm 变薄之后这条链更要有。 |
| **开放-5** | `account-onboarding/` 是否仍活着（其红线「daemon 起会话机制零改」与本区正面冲突）—— U8 前确认 |

---

## §7 变更记录

- 2026-08-01 建区，Phase A 四视角摸底。
- 2026-08-01 v1（决策内核 crate + 三宿主）→ **用户否决**（「别搞成三个宿主」「把所有控制交给 daemon」）。
- 2026-08-01 v2：daemon = 唯一后端 + monitor 退纯前端；用户定框「decomposed the monitor into read and control」
  ⇒ 改写为 frontend/backend 分解，新增两种生命周期与命名订正。定下三条：长连接双向 · 超时推客户端 · Windows 账号记账。
- 2026-08-01 **v3：四视角计划自审后重写。** 自审打掉我 10 处错（§0.5），其中 3 处是**安慰剂式的机检或门禁**
  （U0 的前提、U2 的 cfg 扫描、S11 的诊断方向）。新增 **U-1 第零梯队**（修当下就坏着的三个护栏 + 半 bump）；
  U0 重定义；U1 拆 a/b（护栏不能钉到还不存在的目录上）；U7 拆五、U8 拆四；重命名独立成 U13；
  文档改动**分散进各功能 DoD**；`IPC-PROTOCOL.md` 并入 U6（先修再冻结）。
  待拍板从 6 条收敛到 **1 条**（间接写裁决）。
- 2026-08-01 **U-1 Phase D 后修计划**：Phase D 逮到一条阻塞 —— **修护栏的动作当场制造了同一类的新盲区**
  （`#[cfg(test)] mod guard_support;` 是无花括号体声明，剥法把 `main.rs:26–179` 整段吞掉）。
  据此在 `doc/INVARIANTS.md` §41.4 新增第 3 条派生纪律「剥法必须同时防欠剥与过剥」，
  并把「护栏的公共机件必须收敛到一处」写成元教训（同一个坑 `readonly_guard` 早填过、另外三处没跟）。
  账本 **S1** 的地板判据定为**字节下限 + 数量相等两条并存**（单靠字节地板挡不住单文件被剥空，实测 `watcher.rs` 整个消失仍绿）。
- 2026-08-01 **D3 命名 —— 提出、澄清、当场闭合**。用户先问「是不是还有命名漂移」，我按**仓库级**命名答了
  （查出四套并存、`monitor`/`cc-monitor`/`ccmonitor` 三拼）；用户随即澄清**问的是 tmux 会话名，要后缀 `-cc`，其余不动**。
  ⇒ 我原先那版答复答偏了问题。按澄清后的范围复查，逮到 S4b-3b 反转漏掉的**最后一个生产生成点**
  （`launch-requests.ts:67` 的默认值），已修 + 加对拍断言 + 修 9 处自相矛盾的注释。详见 §5「已闭合」D3 条。
  仓库级命名（`monitor` crate 名三拼、`remote-daemon-proto` 目录与包名不符、统一后 `remote` 会变成假话）
  **不在用户这次的范围内**，仅作为观察留在 U13 行里。
- 2026-08-01 **U0 完成**。Phase B 复查推翻了原 DoD 的两条：`graylight-suite` **不是孤儿**（`e2e/README.md:52` 有排除论证 + 有 npm 脚本），真问题是 `f40-suite` 连 npm 脚本都没有；「16 个 tsx 套件**双重**不设防」里的「双重」不成立（`coverage.exclude` 排的是测试文件自身，标准做法）。
  Phase D 逮到 7 条重要项，其中 **D1 是我补的守卫自己漏了同族更重的那条洞**（收尾 `if (failed>0) throw` 被删 ⇒ 测试照跑照打 ✗ 而退出码 0），且我的「挡不住什么」段没写它。
  连带修：另外两处同型的静默 SKIP（`tmux-target-acceptance.sh` 缺 `script(1)` / `restart-daemon-frames.sh` 缺 daemon）·`ci.yml:306/313` 残留的「8 套」· `DEVELOPMENT.md` 的「14 组」且漏两个名字 · `RELEASING.md` 的 f40 悬空引用 ·`.gitignore` 补 `src-tauri/crates/**/Cargo.lock`。
  新增账本 **S16**。U0 行的「241 例」订正为 **242**（241 是 e2e 地板合计，串号了）。
- 2026-08-01 **U1a 完成**。实现期证伪了功能计划自己的一条 DoD（「`require` 阈值改小 ⇒ 不影响」——变异实测：阈值 10→3 全套依旧 4 passed），而**账本 S11 早就写着「`min_checked` 不低于迁移前的 checked」**，是功能计划抄漏了账本。五条钉子按 clippy 的提示改成**编译期** `const _: () = assert!`（改坏了编不过）。
  Phase D 逮到 3 条重要项：**A1 是我造成的静默退化**（`mod` 插错位置拆散 `#[cfg(test)]` 配对，`structural_scan` 变无条件编译、dead_code +5，而 `cargo build` 无 `-D warnings` 故不会红）；**A2 是本功能核心目的上的缺口**（读数漏了 `violations`，对 F01 的裸目标事故形状全瞎）；A3 参数两处各写一遍。
  顺带订正一处陈旧断言：`sftp.rs` 注释写「checked=11、留 1 余量」，**实测 10、余量为 0**，逐 commit 追到 `666cc14` 属正当行为变更（净 −1）。不下调阈值。
- 2026-08-01 **U2 完成**（第一个真动结构的）。Phase B 复查推翻主计划一条：**回边判断偏了一个函数** ——
  `session_alive` 全在 platform 域内、无回边；真回边是 `spawn_pid_watcher` 依赖 `WatchEvent`/`PidWatchTarget`。
  定形态为**切开**（`watch_pid_until_exit(pid, expected_start, on_dead)`）而非参数化谓词。
  **「行为逐字不变」有硬证据**：wire 输出搬家前后 sha256 相同；审计独立做到 14 条子命令 + stderr + 退出码全 diff 为空，
  外加流模式真杀进程触发 pidfd 判死、帧逐字节相同；`proc_starttime` 去重用 **60 万输入差分 fuzz** 证等价（0 mismatch）。
  **唯一例外如实记**：`tracing` 事件的 target 从 `::watcher` 变成 `::platform::pidwatch`（影响面实测为零）。
  Phase D 两条阻塞**都是我把「规则」写成了「现状」**（README 宣告平台线「已落地」，而它自己三行后就说判据是 U4 的；
  「唯一允许平台 cfg 的层」写成现状而生产段还有 4 处在外）。据此真收两处 + 其余如实列表。
  另：`no_timer_guard` 的实测基线注释**第二次过期**（那段自己第一句就是「别手抄这个数」）⇒ 这次不改数字、改做法：
  删掉快照值只留复测办法。`REGISTERED_DURATION_USES` 的 `ends_with` 分支抽成 `matches_registered` + 真值表钉住
  （它今天被执行但恒 false，U3 搬 `watcher.rs` 时才第一次真生效，写错会报出「登记表在腐烂」这条指向错误方向的诊断）。
- 2026-08-01 **U3 完成**（`d0db4f9`）。§1.1 第二条解耦线交付：两层 + 方向固定 + 接口面恰好一个符号（机检）。
  **计划预言的「`readonly_guard` 会当场红」没兑现，而没红本身就是缺陷** —— 白名单按裸文件名匹配，
  文件搬进 `control/` 后护栏毫无察觉；同样的逻辑意味着任何目录下的 `fork_write.rs` 都会被当白名单放行。
  改成按仓库相对路径，两个变异都咬。
  **`mtime_ms` 从 `common/` 搬回 `observe/`** —— 我自己定的「≥2 层」门槛第一次要求我改自己的代码。
  U2 交接四条全部兑现，其中 `matches_registered` 的 `ends_with` 分支**第一次真生效**（有变异证明）。
  monitor 侧两处跨 crate 硬路径如 U2 审计预言的那样断了（都**响**，不是静默假绿），收成两个落点。
  行为逐字不变：wire 与 **U2 之前**的基线仍逐字节相同。
- 2026-08-01 **U4a 完成**。跨 target `cargo check --all-targets --target x86_64-pc-windows-msvc`
  **12 错 → 0 并进 CI**（ubuntu 上跑，`check` 不链接）。拆 `pidwatch/{linux,fallback}` 之后 11→2，
  剩下 2 个都在测试段、一个 cfg 解决 —— **印证 U2「把 11/12 个错集中到一个文件」是对的判断**。
  **U4 拆成 a/b**（铁律 4）：真 Win32 实现的等价性必须在真机上验，写一份验不了的实现再宣布完成
  就是「把没做的标成做完」。
  两轮推迟的 `pid_alive` 地雷在此处置：**静默说谎（恒真）→ 大声未实现**，
  并新增 `platform/fallback_guard.rs` 钉住整族（fallback 分支不许凭空返回成功值）。
  那条护栏**第一次跑就咬到我自己** —— `unimplemented!()` 的文案里写了那个布尔字面量，按纪律改措辞。
- 2026-08-01 **用户砍掉 U5、换成 U5'**：「本机 backend 生命周期不管，我后面装新版再自己搞
  （前提是你把东西都打包完善了）」。⇒ sidecar/监督/自愈全部划掉；**打包面变成硬要求**。
  实测确认这不是小事：`bundle.resources` 与 `externalBin` **都是空的**，三个受管工具没有一个
  能装到本机 —— 装新版之后本机一个工具都不会更新。开放-6 因此升级成一条正式功能。
- 2026-08-01 **订正我自己的一次误读**：用户说「本机 backend 生命周期不管，我后面装新版再自己搞
  （前提是你把东西都打包完善了）」，我读成「删掉 U5，换一个打包功能」并擅自新立了 U5'。
  用户澄清：**他说的是本机那套他自己起；要我做的是走查新用户 / 旧版本用户两条使用流程。**
  ⇒ U5 改为**范围收窄**（去掉 app 侧的启停监督/自愈，保留「daemon 在本机可用、可手工起」），
  U5' 改成 **U5-走查**。**教训**：把一句「这部分我自己来」翻成「删掉一个功能并新立一个」，
  是在用户没要求的方向上改计划 —— 铁律 4 说的是「计划≠现实就停下改计划」，不是「凭我的理解改」。
