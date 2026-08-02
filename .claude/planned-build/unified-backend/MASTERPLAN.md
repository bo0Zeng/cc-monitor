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
| 1 | 「五套 `ccm-*` e2e **无最小断言数地板**」，列为必须最先做的 U0 | **全都有**，而且是运行期 PASS 数地板 + fail-closed（`e2e/assert-pass-floor.sh`），CI 还有一道逐对校验的元门禁（`ci.yml` 的 G-A 覆盖面地板，**16 套**（U8a-2a 起，原 15；行号随改动漂移，按步骤名找））。这是 `gate-integrity` 早已收官的 G-A/G-C | **第一梯队本来是空的**。病根：把摸底 C 转述的陈旧 BACKLOG 条目（E11）当现状，而我这轮开头才读过写着「✅ 全部完成」的工作区索引 |
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
| S1 | **护栏的扫描面**<br>**U-1 已交付**：`no_timer_guard` 递归 + **字节地板与数量相等双判据**（单靠字节挡不住单文件被剥空）；`readonly_guard` 的**两条过剥（fail-open）已修**——锚点钉行首 + 无花括号体声明不吃后文，扫描面 217_853→221_928；欠剥方向新增机器判据 `no_test_code_leaks_into_any_production_section`。**遍历部分收敛**（U8a-2a：strips-clean 那份抽进 `guard_core::assert_tree_strips_clean`，两侧共用，`min_files` 地板同时棘到实测 34/52）；daemon 侧仍有 6 处 `read_dir` + monitor `parity_ledger` 1 处，留 U1a | 剩余：`readonly_guard` 钉 `observe/`；`readonly_guard` 钉 `observe/`；`control/` 窄写护栏（三条子判据原样迁）；`platform/` 与顶层谁管**必须写明**（今天改钉 observe 后是真空） | U-1 · U1a · U1b |
| S2 | **守卫的测试段 marker**<br>**U-1 已交付**：`guard_support.rs`（`#[cfg(test)]`-only，`pub(crate)`）收敛了 3 个守卫的剥法 + `assert_no_test_code` 自检。<br>**U8a-2a 再搬一次**：剥法本体进**共享 crate `guard-core`**（两侧 `[dev-dependencies]`），`guard_support.rs` 降为再导出。动因是 monitor 够不着 daemon 的 `cfg(test)` 模块，于是那边各写便宜近似 —— 在 `ssh_source.rs` 上会把扫描面砍掉三分之二。顺带把锚点放宽到**复合 cfg**（`#[cfg(all(test, …))]`，`session_map.rs::linux_liveness` 一直漏剥）。⚠ **`readonly_guard` 仍是第二套且不能 naive 换**——它能剥 `#[cfg(test)]` **自由函数**（`history_query.rs:232/309`，两者对该文件差 1246 字节），换过去会让它变弱；要收敛得做**并集剥法**，留 U1a | 原病灶：三个守卫共用的 `"\n#[cfg(test)]\nmod tests"` 对 `main.rs`（`mod stream_flag_tests`）不匹配 ⇒ 抽成一个共享 helper + **能真正检出「没剥掉测试段」的自检**（现有的 `main_prod.len() < main_raw.len()` 光靠剥注释就满足） | U-1 |
| S3 | **平台原语**<br>**U2 交付的是「目标归属地」+ 把 11/12 个 Windows 编译错集中到 `platform/pidwatch.rs`，**不是收口**。已收：`pidfd_open`/`watch_pid_until_exit`（切开了 observe 回边）· `/proc` 一族 9 项 · `path_key`（单列 `platform/paths.rs`，U4 加 Windows 分支时零二次搬）· `proc_claude_config_dir`（Phase D 审计要求补收）。<br>⚠ **生产段还有 4 处在外**：`tmux_hook.rs` 的 `libc::kill`（§1.1 已裁定 tmux_hook 归 control ⇒ **U3 连它一起处理**）· `main.rs` 三处（组装根，可辩护为留，**但不能因此说「唯一」**）。<br>⚠ **U4 伏笔**：`is_same_live_process` 那张判定表要上提到 `platform/liveness.rs`（Windows 判活复用它），别留在按 `/proc` 命名的模块里 | U2 · U3 · U4 |
| S4 | **`observe/ → control/` 的窄接口**<br>**U3 已交付**：`layering_guard.rs` 两条机检 —— 反向零容忍 + 正向符号集**恰好等于**登记表。今天登记表**只有一条**：`crate::control::tmux_hook::install_hooks`。<br>摸底时真有一条反向边（`fork_write` → `accounts_query::read_regular_capped`），**没开例外** —— 那个函数不是 observe 的域逻辑，搬进 `common/fs.rs` 后边自然消失。铁律 6 的正例。<br>⚠ 加登记项前先答：**为什么这件事非得由观测侧发起、control 能不能自己做**（`install_hooks` 的答案：不能 —— 触发时机是「tmux server 起来了」，那是 socket inotify 观测到的事实，control 侧没这个信号，硬要它自己发现只能靠轮询，与 §41 正面冲突） | U3 ✅ |
| S5 | **`common/`**<br>**U2 已交付**：`projects_root`（**5 处不是 4 处** —— 第五处是 `watcher.rs::watch_loop` 里内联的，`grep fn` 找不到）· `mtime_ms`（2 份）。门槛写在 `common/mod.rs`：≥2 **层**用 · **平台无关** · 无域知识。<br>⚠ **「时间换算 · quote」这两项要划掉**：Phase D 审计逐条核过，daemon crate 内**没有可合的逐字副本** —— 时间换算三处语义/单位各不相同（`file_mtime_epoch` 单位是**秒**不是毫秒），quote 在 daemon 内只有一份、多份在 monitor 侧**跨 crate**、`common/` 收不了。<br>⚠ **U3 必须复查**：`mtime_ms` 的两个调用点同属 observe，「≥2 层」按层口径**今天不成立**；`observe/` 一建出来就要重判 | U2 · U3 |
| S6 | **wire 协议 + `IPC-PROTOCOL.md`**<br>**U8a-2a**：monitor 侧补齐 `hello.commands` 解析（此前整个字段被丢弃）+ `Reply`/`Cancelled` 从「认识但不消费」改成**真消费**；文档入方向节改写并订正 4 处漂移（漏列 `commands` 的线上字节序 · 「写半边从来没人用过」· `biased` 少说了有界 · 超时只覆盖半段） | 双向；**文档先修再冻结**（该文件 7 处在说谎，见 §3-U6）；wire 字段名双向 `include_str!` 对拍 | U6a / U6b / U8a-2a |
| S6a | **跨进程握手时序约束**（U6a 新登记）<br>**U6a 已交付**：`cc.ps1.tpl`「先设标题、后写 await 文件」这个顺序，同时被写在**四处**：PS 模板本身 · `bind.rs` 模块头 · `doc/IPC-PROTOCOL.md` 时序图 · `cc_integration.ts` 给用户看的超时文案。U6a 之前**后三处全是旧的**（旧顺序 + 800ms + 100ms + 漏画 ≤600ms 重试）—— 文档在教人复刻 v2.21 那个「每个新 shell 首次 `cc` 固定烧满超时」。 | 四处保持一致，由 `profile_installer.rs::handshake_doc_guard` 三条护栏钉住（顺序 + 模板侧 deadline/轮询步长 + monitor 侧 debouncer/重试）。**不放 `bind.rs`**：它几乎整个 `#[cfg(windows)]`，护栏放那儿在 Linux CI 上一条都不跑 | U6a ✅ |
| S16 | **入方向命令面**（U6b-1 新登记）<br>`inbound::COMMANDS` ↔ `dispatch` 分派臂 ↔ `hello.commands` ↔ `IPC-PROTOCOL.md` 入方向小节 ↔ **`e2e/inbound-daemon-frames.sh` 的命令名清单**，**五处** | 前三处由 `hello_commands_match_the_dispatch_table` 钉成**同一份真相源**（声明了却不接 ⇒ 客户端石沉大海；接了却不声明 ⇒ 客户端不知道能用）；第四处由 U6a 的字段对拍逼进文档；第五处由 U8a-2a 的 `the_e2e_command_list_matches_the_daemon_command_table` 钉住。<br>monitor 侧的 `InboundClient::accepts` **不是第六份副本** —— 它直接吃 `hello.commands`，无独立清单 | U6b-1 ✅ · U6b-3 ✅ · U8a-2a ✅ · U8a-2b ✅ · **U8a-2d ✅ 改写**：不再是「N 处副本互相对拍」，而是**注册表（`inbound::REGISTRY`）+ 三条派生 + 一面镜子** —— 名字与处理器绑在同一个值里 ⇒ 声明↔实现漂移**在注册表这一侧不可表示**；`COMMANDS` 保留为镜子（monitor 与 e2e 都在文本抽取它），由**数据对数据**钉住 |
| S17 | **用量口径**（U7-2 新登记）<br>**U7-2 已交付**：抽进共享 crate `usage-core`，两侧各自 `path` 依赖。此前是无护栏的口径双写，且**已漂开两处**（BOM · 有 requestId 无 uuid） | 唯一实现在 crate 里；判据是「改内核一处 ⇒ 三侧同时红」，不是任何一侧的单侧测试 | U7-2 ✅ ·（U8a-2a 补账：当初 **CI 三样一条都没补**，test/fmt/clippy 全缺 ⇒ 那 8 条测试在 CI 里等于不存在）|
| S18 | **账号契约 + 名字安全判据**（U7-3 新登记）<br>**U7-3 已交付**：四条常量 + `is_deceptive_char`（并集）进 `acct-core`；两条守卫**退役**，因为漂移已不可表示 | 唯一定义在 crate 里。**`is_safe_config_dir` / `norm_dir` 刻意不合** —— 那是平台特化不是漂移（本机要认 Windows 盘符且必须允许 `\`） | U7-3 ✅ ·（U8a-2a 补账：同 S17，4 条测试此前不进 CI）|
| S7 | **daemon argv 面** | 三类；`split_stream_flags` + `every_capability_token_is_strippable` 同步扩。⚠ **起点比以为的更糟**：现有的二分表本身就漏了 5 条子命令（`--tmux-notify` / `--resolve` / `--fork-session` / `--account-trust-zero` / `--read-session-from-offset`），其中 `--account-trust-zero` 漏登记**出过 v3.4.0 事故** | U6 |
| S8 | **`BUILD_ID` 单源链条**<br>**U-1 已交付**：① `ssh_source.rs::embedded_build_id_single_source_wired` 断言 ≠ `"unknown"`；② `build.rs` 三条硬 panic（抠不到源码 `BUILD_ID` / 有二进制但缺清单 / 清单与源码不符）；③ 半 bump **真修掉**（两个 arch 从 p1v 源码现编，`rust-lld` 零安装）。<br>⚠ **措辞订正**：原写「缺文件从 warn 改 fail」不准 —— 三条 panic 都以「`embedded-daemons/` 里真有二进制」为前提；**整个目录缺失时仍是优雅降级**（那是 dev/CI 常态），兜那一档的是①不是 `build.rs`。发版链两头都够得着（`release.yml:56-58` 写清单、`:113-118` 再对拍） | U-1 · U13 |
| S9 | **读面七组 → 四组**（§0.1 三类） | monitor 侧退役 | U7a–U7e |
| S10 | **`shared/ccm`** | 零决策执行臂。**保住**：三个 exec 出口 · `eval "$CCM_ENV"` · `--detach` · `--ccm-probe` 首行 · codex `CC_BUS_ID` 无条件覆盖 · **`@ccm_sid` 回填（§0.5-5）** · `ccm:264-279` 的撞名分叉 | U9 |
| S11 | **`sftp.rs::ccm_cli_has_required_elements`**<br>**U1a 已交付前半（基线）**：强度读数由 `ccm_cli_contract::measure()` **单一产出**，迁移前后同一函数跑两份脚本文本 —— **`needles`/`channel_a`/`t_targets_checked` 三个 `>=`，`t_violations` 是 `<=`（必须 0）**。⚠ 最后这半句不能少：Phase D 审计实测，只比前三个字段时「4 处精确目标全改裸目标」四字段一字不变、全绿。5 条**编译期**钉子（`const _: () = assert!`）钉住基线与阈值两个旋钮。<br>**U9 的迁移清单（现在写死，免得只搬一半）**：`measure()` 喂新构造点文本 · **`require()` 必须一起搬**（它看 violations，读数是它的镜子不是替身）· `pin_t_def()` · `doc/INVARIANTS.md:686` 的指向 | 迁移后逐条对拍 = U9；护栏按新边界重钉 = U1b |
| S12 | **会话名生成** | 两族各一个函数 + 计数守卫 == 2。⚠ **守卫挂 U11 不是 U8**（ccm 到 U9、cc-spawn 到 U11 才收编，挂 U8 是**做完必红**的 DoD） | U8 · U9 · U11 |
| S13 | **`parity_ledger`**<br>**U-1 已交付**：`command_signatures()` 改递归 + `files.sort()`（不排序时「首个胜」会让同名命令随文件系统顺序漂）。实测 `adapter/` 下两文件的 `#[tauri::command]` 命中数都是 **0** ⇒ 递归当下是**纯预防性**，`LEDGER.len()==123` 与 `checked==68` 都没变 | 本区天然验收面。⚠ 它只钉命令**这一层**，别把「数字没动」读成「读面没搬成」 | U-1 · U7 |
| S14 | **`--resolve`** | 吸收进 backend 的计划面；线上形状逐字不变（aterm 契约 2026-07-18 冻结）；`sessionName` 漂移随之消失 | U6 · U8 |
| S15 | **本机分发链** | Tauri sidecar，**与 `embed_daemons` 完全另一套**；⚠ `sftp.rs:1262` 的 `assert_eq!(…, b"\x7fELF")` 与 `:1252` 的 arch 表会被 Windows PE 打红 | U5 |
| S19 | **`npm test` 套件链 + tsx 套件登记表**（U0 新增；⚠ **原编号 S16 与「入方向命令面」撞号**，U8a-2a 改号 —— 那一轮恰好同时碰了这两条，撞号让「本轮碰了哪几条」无法唯一指称）<br>**U0 已交付**：`src/node-suite-registry-guard.vitest.ts` 六条判据（条数 / 全仓总量地板 / 集合 / 路径 / 链路+`&&` / 失败收尾）。⚠ **U8c 退役两个 TS 渲染器时必须同步改四处**：`NODE_SUITES` · `TOTAL_FLOOR` · `package.json` 的 `test:*` 定义 · `npm test` 链。被碰到的是 3 个套件：`test:launch-render-cli`(26, **整删**) · `test:launch-dimensions`(28, **整删**) · `test:remote-launch`(40, **改不删**)。只删一半会当场红 —— 这正是本条要的效果 | U0 · U8c · U8a-2a（新增 `inbound-frames` 套件：`package.json` `test:inbound-frames` · `ci.yml` 地板 15 · G-A 覆盖面 15→16 三处 · shellcheck 覆盖面 39→40） |

| S20 | **共享 crate 家族的约定**（U8a-2a 新登记）<br>今天四个：`branch-core` / `usage-core` / `acct-core` / `guard-core` | 零（或仅 `serde_json`）依赖 · 平台无关 · 单向 path dep（不制造 workspace 成员关系）· **新增一个就要在 `ci.yml` 补 test/fmt/clippy 三样**（那条纪律 `ci.yml` 里早写着，U7-2/U7-3 两次没跟，U8a-2a 补齐）。<br>⚠ `guard-core` 是唯一**带 fs 遍历 + panic 语义**的一个，也是唯一**两侧都只作 dev-dependency**的（守卫全在 `cfg(test)`，不进发布二进制）—— 头注里已写明这条例外，别照抄错家族不变量 | U8a-2a ✅ · 新增 crate 时 |
| S21 | **monitor 侧那条长连接的写半边**（U8a-2a 新登记）<br>今天只许经 `inbound_client::split_and_park` 出手；`ssh_source` 生产段**既不切流也不写流** | 由 `write_half_guard` 两条**零命中型**护栏钉住（不许出现 `tokio::io::split(` + 不许出现任何写方法/UFCS），外加一条「共享剥法 vs 便宜近似」的扫描面自检。<br>⚠ 判据形状换过一次：原来是「每处 split 后 240 字符内要有 park」，D 审计用**尾随注释**和**split 后先偷写一句**两种普通写法绕过 ⇒ 改成把切与停收成一个函数、判据改零命中。<br>⚠ **最终形态**：`inbound_client` 落 `control/`、帧类型单拎 wire 模块之后，这条护栏要跟着模块边界重钉（同 S1 的形状），归 U1b | U8a-2a ✅ · U1b |
| S22 | **`inbound_client` → `ssh_source::InboundFrame` 这条反向边**（U8a-2a 新登记）<br>`§1.1-2` 裁定「允许 observe → control 的窄接口，**反向不许**」，daemon 侧为此立了 `layering_guard`；monitor 侧**没有**，所以这条边今天不会红 | 最终形态：帧类型（`InboundFrame` + `parse_frame` + `KNOWN_FRAME_KINDS`）单拎 `wire_frames.rs`，得到 `wire_frames ← inbound_client ← ssh_source`，无环、与 daemon 对称。**见证强度一字不变**（`DaemonHello` 的强度来自私有字段 + 唯一构造函数，与帧住哪无关）。<br>⚠ **刻意不在 U8a-2a 顺手做**：`parse_frame` 一搬，`emits_parity::known_kinds_matches_parse_frame` 与 `write_half_guard` 的锚点都要改文件面 —— 与 monitor 侧 layering guard 一起落 | U1b |

| S23 | **tmux `=name:` 精确匹配**（U8a-2b 新登记）<br>今天**三处**：TS `session-backend.ts::exactTarget` · monitor `tmux.rs::exact_target`（外包一层 `shell_quote`）· **daemon `control/launch.rs::exact_target`（argv 版，不引号化）** | 规则同源、引号化各自按传输定。daemon 那处由 `exact_target_shape_matches_the_monitor_side` 跨轨对拍（`include_str!` monitor 源码）。<br>⚠ 这条有事故背书（裸 `-t` 会打到兄弟会话上，本仓踩过 `cc-<sid8>-2`），**新增第四处必须同时加对拍** | U8a-2b ✅ · U10（停/接/send-keys 会加新的 `-t` 构造点）|

| S24 | **数据面漂移记账的四个落点**（U-CC1 新登记）<br>未知记录 `type`（`parser.rs`）· 已知类型解析失败（`parser.rs`）· 未登记的会话 `kind`（`session_map.rs`）· daemon 未知能力 token（`ssh_source.rs`） | 只记账、**零行为变化**；有界（64 键 + `<overflow>`，样例按字符边界截 400 字节）；每个面必须写明**「这么降级之后会怎样」**（`DriftFace::consequence`，有计数自检）。<br>⚠ **第五个面「未登记的 `status`」刻意不加** —— 它在前端（`session-status.ts`），Rust 侧对 `status` 无任何白名单分支。加一个没有落点的面 = 「登记了但不产生信号」，比不加更糟。<br>⚠ **计数量纲逐面不同**（每条记录 / 每次扫描 / 每次握手），前端 `countUnit()` 逐面给单位并有测试钉住 | U-CC1 ✅ · TS 侧 status 面另记 |

| S25 | **平面 ③（本机开窗面）的 POSIX 取舍**（U8b 新登记）<br>L1 裁决：POSIX 上**刻意不开 GUI 终端窗口**（没有「唯一的终端」，挑一个是会在别人机器上错的决定；会话容器是 tmux） | 由 `launch.rs::no_terminal_emulator_is_ever_spawned_from_this_file` **零命中**钉住（挡的是很自然的一次「顺手改进」）；文案由 `POSIX_NO_TERMINAL_WINDOW` 单点出，前端按标记分档、`the_posix_marker_is_the_one_the_frontend_matches_on` 跨轨对拍。<br>⚠ **今天的不对称**：本机 resume 有 OS 分派、远端没有 ⇒ POSIX 上只复制命令。补它与 **U8a-2c** 同一个阻塞（渲染好的串拆不回「只建不接」），**别单独硬补** —— fire-and-forget 会静默失败 | U8b ✅ · U8a-2c |

| S26 | **`control/`/`observe/` 的 serde 类型**（U8a-2d 新登记）<br>今天 6 个：`resolve_query` 4（aterm 冻结契约）· `fork_write::ForkResult`（一次性子命令出参）· `accounts_query::RawAccount`（**文件 schema，根本不是 wire**） | **登记制**（逐条列举 + 写明它是什么 + 机检 + 幽灵条目检查），形状照 `spawn_registry`。<br>⚠ 设计稿原提的是「白名单恰好一条」——**实测那个前提错了**，而且后两个搬进 `wire/` 是错误归类。<br>⚠ 「上线类型收进 `src/wire/` 目录树」等**真出现第二个流协议类型文件**时再做（今天只有 `wire.rs` 一个，为一个文件建目录树是空转） | U8a-2d ✅ |

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

## §5 尚待拍板（**已清空** —— D1 于 2026-08-02 裁决，见下）

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
- 2026-08-02 **U6a 闭环，四个计划数字全被机检推翻**。计划的缺口数是手工 grep 数的，无一正确：
  帧字段漏的是 **7 个不是 8 个**（多报的 `next` 是 `SeqCounter` 的进程内计数器、根本不上线）；
  查询表漏的是 **3 个不是 6 个**（`--list-accounts` / `--search` / `--session-accounts` 早在表里）；
  另发现 **1 处比漏写更糟的**：`tmux_sessions` 帧的字段文档里叫 `classification`，
  **全仓没有任何东西叫这个名字**（线上真名 `observation`）—— 照文档写的客户端永远读到 `None`，
  退回「保守跳过」，正是这字段当初要修的 idle 灰灯。
  ⇒ **纪律**：Phase B 的「实测核计划」这一步，手工 grep 不算实测；能上机检的一律先把机检写出来、
  让它报第一版清单，再动手补。本轮机检是在自己上线**之前**挣回成本的。
- 2026-08-02 **U6a 最重的一条：文档在教人复刻一个已经修掉的 bug**。
  `doc/IPC-PROTOCOL.md` 的握手时序图画的是 v2 修复**之前**的顺序（先写 await 文件、后设窗口标题），
  而 `cc.ps1.tpl` 早已反转 —— 旧顺序下 monitor 扫得越快越容易找不到窗口，v2.21 实测
  「每个新 shell 首次 `cc` 固定烧满超时」。同图还有 deadline 800ms（实际 3000ms）、
  debouncer 100ms（实际 50ms）、monitor 侧 ≤600ms 重试**整个没画**。
  同一处漂移还在 `bind.rs` 自己的模块头和 `cc_integration.ts` 给用户看的文案里。
  ⇒ **账本新增一条共享面**：**跨进程时序约束**（PS 模板 ↔ bind.rs ↔ IPC-PROTOCOL.md ↔ UI 文案，
  四处双写）。最终形态 = 由 `profile_installer.rs::handshake_doc_guard` 三条护栏钉住顺序与全部数字。
  这类约束**字段级对拍抓不到** —— 两个文件单看都合理，只有合起来看才错。
- 2026-08-02 **护栏放哪儿要先验证它在 CI 上真的跑**。握手护栏本来该放 `bind.rs`（代码在那），
  但 `bind.rs` 几乎整个 `#[cfg(windows)]`，放那儿在 Linux CI 上**一条都不会跑**
  （实测 `cargo test bind::` = 0 个）。改放 `profile_installer.rs`（用 `include_str!` 内嵌
  `cc.ps1.tpl`、测试在 Linux 真跑）。⇒ 与 U4a 的教训同族：**「写了护栏」≠「护栏会执行」。**
- 2026-08-02 **U6b 拆成三件**（U6a 拆过一次，这是第二次拆）。Phase B 实测三处与计划叙述不同：
  ① 「双向」的起点**不是零** —— `--resolve` 已经在读 stdin（一次性 exec 的 `ResumeSpec`），
  缺的是**流式连接上**的入方向；② 载体现成 —— `ssh_source.rs` 里 `stdin` 零命中，
  而 `russh::ChannelStream` 是双工的，那半条通道**完全没人用**；③ 「argv 三分」不是给现有分类加一档，
  今天是**二分**（剥流 flag → 剩下非空即查询），第三类「子命令自己的选项」从没进过那张表。
  拆法按**承载关系**：**U6b-1** 入方向骨架（信封 + `id` + 取消 + 上限 + 两条时序/线程机检）→
  **U6b-2** 能力协商扩到入方向 + argv 三分重建 + §26 护栏扩 → **U6b-3** `--resolve` 吸收 + F90 稳定键。
  先钉信封与线程纪律，否则后面每加一条命令都要重谈一次。
- 2026-08-02 **做入方向的实证（不是推测）**：`--tmux-notify` 存在的唯一理由就是 hook 子进程
  没法给正在跑的 daemon 发消息、只能新起进程发 `SIGUSR1`；`--resolve` 为一次极小的 RPC
  单开一整条 SSH exec。**两者都是「没有常驻入方向」的代偿**。
- 2026-08-02 **入方向的归属先定后写**（U6b-1 步骤 2）：传输层（读行/信封/上限）放**顶层 `inbound.rs`**、
  与 `wire.rs` 同类；**每条命令的处理器归 `control/`**。依赖方向 `inbound → control`，
  与既有 `observe → control` 同向 ⇒ `layering_guard` 判据不需放宽。
  塞进 `control/` 会让「control = 做事」这条线变浑 —— §1.1 的线是按**职责**画的，不是按调用关系。
- 2026-08-02 **U6b-1/U6b-2 闭环（`217835c`）。D 审计改变了我对「护栏」的判断，这条要写进横切约定。**
  前几轮的模式是「加护栏 → 用变异证明有效 → 收工」。这一轮把那个模式的**上限**测出来了：
  我加的每条护栏**自变异都通过**，却仍被**七种普通写法**绕过 —— 包括 **rustfmt 自己的输出**
  （derive 超 100 列会折行，`Serialize` 不在 `#[derive(` 那行 ⇒ 整个类型隐形，而 `fmt --check` 干净）
  和**一行沿革注释**（握手三条读的是整份原文、没剥注释）。
  根因同一个：**我的变异是照着自己的实现设计的**。我知道判据看哪一行，就去改那一行的内容；
  而绕过它只需要**让那行不再是那种形状**。
  ⇒ **自变异只能证明「判据被执行了」，证明不了「判据画的范围对」。**
  ⇒ **新横切约定**：高风险档的护栏，D 审计必须含一个「设法绕过」的对抗视角，且必须在 **worktree 隔离**里能真改真跑。
- 2026-08-02 **两个「今天就活」的阻塞，都是我自己造的**（U6b-1）：
  ① `tokio::io::stdin()` 走阻塞线程池 ⇒ stdin 开着时 SIGTERM 不再退出（**正是 monitor SSH exec 的生产形状**），
  实测 3/3 挂死、父提交 100ms 退出；② 1 MiB 上限在整行已进内存之后才判 —— 我**抄了 `--resolve` 的常数、
  没抄它的机制**（`stdin().take(MAX)`），还在 DoD 里写下「内存不涨」，**那句当时没有任何测试**。
  ⇒ **纪律**：DoD 里每一条断言，写下的同时就要问「它对应哪个测试」；答不上来就别写成验收项。
- 2026-08-02 **新增测试自己也要变异验证**。那条内存测试**改到第三版才不是安慰剂**：
  ① 读完再量 —— buffer 已 free、glibc 已 munmap，RSS 掉回去了；
  ② 边跑边采 —— `Flood` 永远 Ready，reader 在**一次 poll 里**读完 256 MiB，采样循环插不进去；
  ③ 最后用内核的 `VmHWM` 高水位才抓到。前两版都被「全程累积、读完再判」这个真变异骗过。
- 2026-08-02 **S7（argv 三分）交付，且判据比账本设想的更换了主语**：从「非空即查询」换成
  「`args[0]` ∈ 子命令表才查询」。副作用是**漏登记的后果变糟了** —— 从 exit 2（吵但看得见）
  变成「那条子命令静默变成起了个流」。⇒ 完备性机检从附属品升格成**判据能成立的唯一理由**。
  实测 17/18 个子命令输出与改动前逐字相同，唯一差异正是有意的那处（未知 flag 改成照常起流）。
- 2026-08-02 **U6b-3 闭环 ⇒ U6b 系列完成。最值得记的一条：别再往判据上加正则。**
  D 审计击穿两条结构机检之后，我的第一反应是「把判据写得更严」——审计给的方向是反的：
  **让违规不可表示**。做完发现那两条机检可以**整条删掉**：
  `inbound::spawn` 要一个只能由 `write_and_flush_hello` 产出的 `HelloFlushed` 见证；
  `dispatch` 改成非 async ⇒ 分派臂里写 `.await` 是 **`E0728` 编译错误**，不是测试红。
  ⇒ **横切约定补一条**：判据被绕过时，先问「能不能让它不可表示」，再考虑加判据。
  加正则是追着变异跑、永远慢一步；改成不可表示是一次性的。
- 2026-08-02 **S14（`--resolve`）交付，「吸收」落成并存而非搬走**。它的契约与仓外 aterm
  冻结在 2026-07-18，而 aterm 现走 β TailTransport、**暂不消费** —— 也就是说这条契约
  **随时可能开始被消费**，现在拆掉是拿别人的集成期赌。两条路复用同一个纯函数，
  一次性那条实测三条用例逐字节相同。
- 2026-08-02 **S6 最终形态达成**（U6a 文档冻结 + U6b-1 入方向 + U6b-2 能力协商 + U6b-3 首条真命令）。
  **给 U7–U11 的结构收益**：控制面每搬一条动作进 daemon = 一条 `Disposition::spawn` 臂 +
  一条 `COMMANDS` 登记，三条纪律（不许跑在读循环上 / 必须声明 / 必须进文档）自动生效，
  且第一条是编译期的。
- 2026-08-02 **U7 的路线改了：从「monitor 侧读面退役」改成「抽共享 crate」。四项实测推翻原排期。**
  ① **monitor 从不起本机 daemon** —— `cc-monitor-remote` 在非部署路径零命中，本机读面就是
  进程内的 `watcher::spawn_watcher`（`lib.rs:441` 唯一消费者）；
  ② U7a 的排期理由是「同时验证本机 backend 那套管道」，而 U5 已被用户收窄成「本机 backend
  生命周期我自己搞」、U5-走查 又实测「装新版之后本机一个工具都不会被安装或更新」⇒ **那个理由 void**；
  ③ **§0.1 自己**把 jsonl tail 评为「已对拍的小内核……全仓最有守卫的一对，**收益最低**」；
  ④ 用量那对（②「真该合的」）**零个同名函数** —— daemon 刻意不带 `parse_line`/`JsonlRecord`，
  在裸 `Value` 上抽取；所谓双写是**口径**双写不是代码双写。
  ⇒ **U7a 若照原样做，本机会话在没人起 daemon 的机器上直接不可用** —— 不是风险，是确定的回归。
  ⇒ 新路线由 `branch-core` 证明可行：它被两侧**双方依赖**（daemon 刻意不在 workspace 里，
  照样 `path = "../src-tauri/crates/branch-core"`），两侧各有真实调用点。
  **合流 = 把内核抽进共享 crate，不是让一侧退役** —— 双写被真正消灭，且不需要任何本机 daemon。
  `U7a`（jsonl tail）**降级登记，不再排第一**。
- 2026-08-02 **U7-1 交付（计划外，Phase B 摸底冒出来的）**：在讨论「哪一组该合」之前发现
  **两侧的帧集根本没对过账**，而且已经漏了一个 —— `turn_end` 在 daemon 的 `EMITS` 里
  （那个常量注释逐条写着「登记 = 承诺真发，已接线」）、`watcher.rs:1080` 每轮对话真发，
  而 monitor 的 `parse_frame` **压根不认它** ⇒ 落进未知分支 ⇒ **每轮对话刷一条 warn 并丢弃**，
  真正的坏帧会淹没在里面。
  ⇒ 加两条对拍：daemon `EMITS` ⊆ monitor 已知集；已知集 ↔ `parse_frame` 分支表同一份。
  ⇒ **账本 S9 的最终形态改写**：不是「monitor 侧退役」，是「①产出面↔消费面对拍（U7-1 ✅）+
  ②真双写抽共享 crate + ③互补残桩各自补齐」。
- 2026-08-02 **U7-2 交付：用量口径抽进共享 crate。最值得记的是那条「跨轨对拍测试」根本不跨轨。**
  `per_request_field_max_matches_local_kou_jing` 只调 daemon 自己的实现、断言人手写下的数字，
  **从不碰 monitor**（那个测试模块里对「本地 usage.rs」的唯一提及是一句 doc 注释）。
  名字里的 `matches_local` 是一句没有判据的声明 —— 与 U6a 抓到的
  「注释宣称『今天零个 rename（已核）』却没有机检」是同一种病：**把一次人工核对写成了长期保证**。
  ⇒ **横切约定补一条**：名字里带 `matches` / `parity` / `in_sync` 的测试，
  必须能指出它**实际引用了对面哪个符号**；指不出来就是单轨测试，改名或补判据。
- 2026-08-02 **无护栏的双写已经漂了两处**（实测）：BOM（daemon 剥、monitor 零处理）·
  有 `requestId` 无 `uuid`（monitor 经 `parse_line` 丢、daemon 计入）。两处可达性都低，
  但正是「无护栏双写」会积累的那种 —— 合并后统一到 daemon 的行为。
- 2026-08-02 **判据升级**：这次不靠「两侧各自的测试都绿」，而是
  **改内核一处口径 ⇒ usage-core / daemon / monitor 三侧同时红**。
  那是双写被真正消灭的结构性证据；此前改一侧另一侧照样绿。
- 2026-08-02 **U7-3 交付。这次先验守卫，结论与上一轮相反 —— 账号那条是真的。**
  `contract_matches_the_daemon_implementation` 运行期读 daemon 源文件、剥注释、剥测试段、
  有字节地板与锚点自检，注释里还记着第一版是安慰剂、被变异证伪后修好。**如实说明，不套用上一轮结论。**
  但它只钉四个字符串常量、不钉逻辑 ⇒ 看不见安全函数上的漂移。
- 2026-08-02 **安全函数 `is_deceptive_char` 两侧双向漂了**（那条守卫的盲区）：
  daemon 缺 word joiner / Ogham /
  各类空白（`U+2060..2064` · `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+3000`）。
  一个能骗过其中一侧的名字就是能骗人的名字 ⇒ 取并集。
- 2026-08-02 **两条守卫退役，因为漂移变成不可表示**（U6b-3 那条约定的第三次应用）：
  四条常量两侧 import 同一个 `const`；`credential_filename_...` 的「本文件真在用这个字面量」
  那一半结构上不可能不成立。**跟 bash 声明对账的那一半保留并搬进 crate** ——
  常量住哪儿检查就住哪儿，否则又是两份。
- 2026-08-02 **§1.5 的窗口问题自动消失**：它的前提是「U7 退役 `local_accounts.rs`」，
  而路线已改成抽共享 crate、monitor 侧不退役。⇒ 账号不必排在 U8 之后。
- 2026-08-02 **如实登记一处薄弱**：内核变异时 monitor 侧两次都绿 ——
  不是接线没生效，是 `local_accounts.rs` 只有 5 条测试、既不覆盖 NEL 也不覆盖 manifest 名。
  **不在本功能里顺手补** —— 补测试要单独设计，混在重构里做等于用新测试给新代码背书。
- 2026-08-02 **★ 订正我在 U7-3 报错的一条安全发现。**
  我说「本机缺 `U+0085`（NEL）⇒ 安全洞」，**错了**：Rust 里 `'\u{0085}'.is_control() == true`
  （NEL 属 Cc 类），`is_safe_config_dir` 本来就靠 `is_control()` 拒了它。
  **集合差过一项，可观察行为没差。** daemon 源码里那句「NEL 不在 `char::is_control` 里」
  是事实错误，我**照抄了它没验**。U7-4 写完覆盖测试后做变异 —— 删掉内核里的 NEL，
  三侧全绿 —— 才发现不对，随后单独跑 `is_control()` 实测证伪。
  daemon 那半（word joiner / 各类空白，`is_control()` 全 false）**是真的**，那半站得住。
  ⇒ **横切约定补一条**：**从别处源码抄来的事实性断言，要么自己验一遍，要么标成「转述」。**
  抄注释比抄代码更危险 —— 代码错了会红，注释错了会被当成依据继续传播。
- 2026-08-02 **U7-4 交付。根因不是「没测到」，是「测不到」。**
  沙盒 `write_manifest` 用的是 `self.0.join(MANIFEST_NAME)` —— 测试的**写侧**与生产的**读侧**
  用同一个 `const`，常量一起变、测试恒绿。**自洽夹具**：看起来覆盖了，结构上不可能因那个原因失败。
  ⇒ **横切约定补一条**：夹具造数据时，凡是「契约名」（文件名 / 目录名 / 协议字段名）
  一律写**字面量**，不许复用生产常量。常量是实现，名字是契约，测试钉契约。
  这个失效模式不限于账号 —— `usage-core` 那侧也该查一遍。
- 2026-08-02 **验收标准照搬会逼出假测试**。计划说「U7-4 做完那两个变异都该让 monitor 红」，
  实际只有一个该红（NEL 那个本来就不是洞）。硬要它红只能：删掉 `is_control()` 让洞变真，
  或写一个绕过 `is_safe_config_dir` 直调 `is_deceptive_char` 的测试假装覆盖。
  **两条都是为了让红灯好看而改代码。** ⇒ 验收标准与实测冲突时，先查标准对不对。
- 2026-08-02 **U7-5：自洽夹具扫查结论 —— `usage-core` / `branch-core` 没有这个问题**（变异验过，
  它们的契约是 JSON 字段名，内核与夹具各写各的字面量）。如实说明，没为凑一条修复而改东西。
  `inbound` 的 `MAX_LINE_BYTES` 略弱（测试用它自己算洪水大小），但那不是跨边界契约名，**判定不改**。
- 2026-08-02 **补上一个真空白：入方向命令名没钉到文档。**
  `hello_commands_match_the_dispatch_table` 钉的是 `COMMANDS ↔ 分派臂` —— 两边都在代码里。
  实测把 `ping` 在三处（COMMANDS / 分派臂 / 行为测试）**彻底重命名**后 **什么都不红**，
  文档里的 `ping` 成了不存在的命令。客户端照文档发 ⇒ `unknown_command`，而两边各自看都「对」。
  ⇒ 加 `every_inbound_command_appears_in_the_protocol_doc`。
- 2026-08-02 **★ §0.1 分类 ③ 的描述已过期一半，且「判活」的后果被严重低估。**
  ① 「daemon 非 Linux 恒 `true`」**已过期** —— U4a 换成了 `unimplemented!()`。
  ② 「本机非 Windows 恒 `false`」仍成立，但实测调用链是
  `spawn_watcher → active_filter → is_session_active → is_process_alive → false`
  ⇒ **Linux/macOS 上本机 watcher 拒绝每一个会话，一行本机 jsonl 都不 emit**；
  另有每 2s 的心跳收割器（**无平台门**）把会话表清空。
  **即 cc-monitor 的本机读面在那两个平台上整体不工作**，只能当远端监视器。
  这与仓库 Windows-first 的定位自洽，但**没有出现在任何面向用户的文档里**。
  ⇒ **U7d 的性质改写**：不是「合并两个残桩」的清理，是「本机读面在两个平台上不存在」本身。
- 2026-08-02 **§0.1 对 tmux 观测的归类也不对**：`tmux.rs` 的入口是 `list_remote_tmux(origin)`，
  **开 SSH exec 列远端 tmux**，根本不是本机实现。那不是平台残桩对，是
  **同一份远端数据的旧轮询 vs 新推送**（B2 的 `tmux_sessions` 帧正为替掉 8s 轮询）。
  ⇒ 真问题是「旧路径退役了没有」，是**退役**不是「写新实现」。
  ⇒ U7b 与 U7d **性质不同，不该并成一件**，也都不按「抽共享 crate」排。
- 2026-08-02 **U7b：连着两轮修同一处判断，这次实测到底。**
  §0.1 说它是「本机 tmux vs 远端 tmux 的平台残桩对」——错（`tmux.rs` 的入口是
  `list_remote_tmux(origin)`，开 SSH exec 列**远端**）。U7-5 改判「旧轮询 vs 新推送，
  该退役旧的」——**也不够准**：轮询**早就退役了**（`ssh_source.rs:1006` 注释写着推送账本
  「替掉每 8s 新建 SSH 的 `list_remote_tmux` 轮询」，前端零 `setInterval`，
  对账路径 `grep -c list_remote_tmux` = **0**）。
  **退役的是轮询，保留的是按需查** —— `tabs.ts` 有 6 处决策点在用，且账本只在该 origin
  有 daemon 流连着时才有数据。⇒ **两条路刻意并存**，硬合只能二选一：对账退回轮询（撤销 B2），
  或决策点在没有 daemon 时失去数据。同 U7-3 判 `is_safe_config_dir` 那个形状。
  ⇒ 交付一条**防回潮**护栏：对账路径生产段不许出现 `list_remote_tmux`。
- 2026-08-02 **一个反复出现的教训**：§0.1 那张病灶表，**逐条查下来错了三处**
  （分类③ 的 daemon 半已过期 · tmux 的归类 · 「旧路径没退役」）。
  它是 v1/v2 时期写的，此后 U4a/B2 都动过对应实现。
  ⇒ **横切约定补一条**：动某一格之前，**先重测那一格**，别把病灶表当现状读。
  它记的是「当时看到的病」，不是「现在的代码」。
- 2026-08-02 **U7d 的文档那一半交付**：`ARCHITECTURE.md` 新增一节 +
  `README` / `README.en` 顶部提示，写明「本机读面只在 Windows 上工作」及完整调用链。
  ⚠ 起初我在 README 里写了「给 localhost 配一条远端即可」当变通 —— **未经验证的断言**，
  已改成「理论上应当可行，但没实测过，不作为方案写在这里」。
- 2026-08-02 **U7d 功能半交付：Linux 判活。本轮提示（也是我上一轮写的计划）给的前提又是错的。**
  计划说「`procStart` 是 .NET Ticks，Linux 的 `/proc` starttime 量纲不同，别硬套 ⇒ 准备降级成
  只查存在性 + 标注置信度」。**实测推翻**：本机 6 个真实会话的 `procStart` 与
  `/proc/<pid>/stat` 第 22 字段 **6/6 完全相等**（量级 ~10^6，一眼不是 ~6.4e17 的 .NET Ticks）。
  ⇒ **`procStart` 是平台原生的**，两个平台各自与本平台查询口径同源，
  **PID 复用防御在 Linux 上是满精度**，不需要任何启发式。
  这是纪律 8「动某一格之前先重测那一格」的第二次兑现（上一格 tmux 同形）。
- 2026-08-02 **一个差点踩进去的实现坑**：`/proc/<pid>/stat` 的第 2 字段 `comm`
  **允许含空格与括号**，朴素 `split_whitespace()` 会错位。扫本机 400 个进程就踩中一个：
  **`comm = "tmux: server"` —— 朴素读到 `0`，正确值 `1042`**。而 tmux server 正是本仓核心依赖。
  ⇒ 找**最后一个** `)` 再切。测试里连**反向断言**都写了（朴素切法在同一输入上必须得到别的值，
  否则那条测试没有区分力）。
- 2026-08-02 **macOS 明确不做并写进文档**：没有 `/proc`，要 `sysctl KERN_PROC` 的 FFI，
  本仓无 macOS CI、无法实测 ⇒ **不写没验过的实现**。分支保留 `false` 而非 `unimplemented!()`：
  daemon 那边是 CLI，panic 是「没人能忽略的信号」；这边是 GUI 常驻进程，panic 会崩窗口。
  `false` 是 fail-safe（少显示，而不是显示永不消失的僵尸会话），**且已写进 ARCHITECTURE + 双语 README**。
- 2026-08-02 **端到端验收用真数据**：一次性探针扫本机 6 个真实 pidfile，**判活 6/6、与 `/proc`
  存在性逐个一致** —— 改动前这 6 个全会被判死。探针跑完即删（环境相关，不该进 CI）。
- 2026-08-02 **U8a Phase B：「起会话」不是三种形态，是三个平面，归属完全不同。**
  | 平面 | 归谁 | 现状 |
  |---|---|---|
  | ① 计划面（决定跑什么） | daemon `control/` | **已在那儿** —— `--resolve` 的 CommandPlan，U6b-3 已吸收进流通道 |
  | ② 远端执行面（真的建 tmux / send-into） | daemon `control/`（**该搬的是这个**） | 今天由 monitor 拼串经 SSH 送 |
  | ③ 本机开窗面（在用户机器上开终端） | **只能是 monitor**（daemon 在远端，开不了你面前的窗） | `launch_powershell_window`，Windows-only |
  ⇒ **U8a 不是「把起会话整个搬进 daemon」**：③ 结构上搬不了，① 已搬完。
  ⇒ U8a 拆成 **U8a（本件，判定）+ U8a-2（平面 ② 的实现）**。
- 2026-08-02 **`send-into` 为什么特殊，说清楚了**：它是平面 ② 里唯一**不能靠「拼一条命令串扔过去」**
  完成的 —— 要对**已存在**的 tmux 会话做 send-keys，而 `ccm` 语法没有「就地复用、不新建」。
  实测 `canRenderCli` 的 6 条 `ok:false` 里只有它是**表达力缺口**，其余是能力缺失/参数校验。
  ⇒ 搬进 daemon 之后它不再需要 CLI 等价语法，**#76 防线的形态会变**：
  从「渲染器拒绝渲染」变成「daemon 有一条专门的命令」。U8a-2 必须显式承接这条，不能默认它还在。
- 2026-08-02 **差点误报一处「Linux 缺口」**：U7d 让 Linux 成为一等本机监听平台后，
  我去查「Linux 上点 ↗ 会怎样」—— `launch_powershell_window` 非 Windows 直接 Err，
  全仓也没有任何 `gnome-terminal` 之类实现。**但那不是缺口**：`remote-launch-run.ts` 头注
  逐字写着「失败回退 = 复制命令 + toast（**非 Windows dev** …功能永不变砖）」。
  即 Linux 上 = 命令进剪贴板。**刻意设计，不改。** ⇒ 纪律 8 的第三次兑现。
- 2026-08-02 **★ D1 裁决：铁律收窄为「daemon 进程自身不许写用户既有数据」（取选项 ①）。**
  U8a-2 Phase B 一查就撞上门 —— §5 唯一待拍板的 D1，问的正好是 U8a-2 要做的事。
  **不先裁决就写那条命令，等于从一条自己知道违规的路上绕过去。**
  取 ① 的理由不是「推荐里写了」：选项 ② 的后果是「**本区不成立**」，
  与用户 2026-08-01 的定调「**把所有控制交给 daemon**」直接冲突 ⇒ ② 不是真实可选项。
  **但这是一次铁律边界变更，单独成件、单独 commit，显式说明，不埋在实现里。**
- 2026-08-02 **D1 的强制条件落成机检，不是散文**：推荐里要求「逐条列举起进程的写面」。
  只在散文里列一遍不行 —— 那正是 §0.2 批评的「护栏与散文说的不是一件事」。
  ⇒ `readonly_guard::spawn_registry`：生产段每处 `Command::new` 必须登记 + **写明理由**。
  实测今天 3 处（`tmux_hook` 起 tmux 装 hook；`watcher` 两处起 sh 跑 `tmux ls`，只读）。
  变异：在 `control/` 里悄悄加一个 `Command::new("bash")` ⇒ 红。
- 2026-08-02 **「抽取面画小了」第四次**：我第一版扫起进程点时按**第一个** `#[cfg(test)]`
  截断文件，而 `observe/watcher.rs` 前部就有内联测试模块 ⇒ 后面两处 `sh` 全被吃掉，只报 1 处。
  改成按 `#[cfg(test)] mod` 块逐块配平排除才得到 3 处。
  ⇒ 这个错误形态已经在本工作区出现四次（U6a 抽取器两次 / U6a 分派文件面 / 本次），
  **共同点都是「用一个便宜的近似去切生产段」**。下次直接用 `guard_support::production_code`。
- 2026-08-02 **U8a-2 实现半的摸底把顺序改了：先接发送端，再写命令。**
  实测 `ssh_source.rs` 写半边的**唯一**用法是 `shutdown()`（关写端让 daemon 见 EOF），
  **零数据字节**；全仓**没有任何地方在构造入方向请求**。
  ⇒ **U6b-1/2/3 建起来的入方向通道，今天在生产里不可达。**（**U8a-2a 已闭合** —— monitor 侧发送端接上，
  「测试连接」的控制通道往返探测是第一个生产调用方，另有 15 条真进程 e2e。）骨架先行是刻意的，
  但它决定顺序：现在加 `launch`，是在一条没人能调的通道上再加一条没人能调的命令。
  ⇒ U8a-2 拆成 **2a（接发送端）+ 2b（launch 本体）**。2a 还能让已写好的
  `ping`/`cancel`/`resolve` 第一次真正跑起来 —— 现成的验收载体。
- 2026-08-02 **精确化我自己在 U6b-3 的说法**：我称 `resolve` 为「第一条真业务命令」——
  准确，但读者可能推断它已在线上被调用。**实际没有**：流式那条无调用方；
  一次性 `--resolve` 也没有 monitor 侧调用方，它是给**仓外 aterm** 的（且 aterm 暂不消费）。
- 2026-08-02 **daemon 侧不该有「第三层安全防线」—— 那会是安全剧场。**
  入方向命令来自**已经握着该机器 SSH 会话**的对端，它本来就能在那台机器上跑任意命令；
  daemon 再校验一遍挡不住任何它原本挡不住的东西。
  daemon 该做的是**形状校验**（fail-fast + 可诊断），**两者的区别要写在代码里** ——
  否则下一个人会以为那层是安全边界而放松上游。
  顺带：`launch.rs` 的「禁双引号」是 **PowerShell 专属**（防 wt.exe 传参畸变），
  改走入方向通道时**不成立也不该照抄**。
- 2026-08-02 **U8a-2a 闭环：monitor 侧入方向发送端接上，那条通道在生产里可达了。**
  形状 = `split_and_park` → `ParkedWriter`（无写方法）→ 拿一帧真 Hello 换 `DaemonHello` 见证
  → `InboundClient`。超时/重试/并发上限全在客户端（daemon 零定时器铁律不动）。
  真进程端到端 `e2e/inbound-daemon-frames.sh` 15 条进 CI，喂给真 daemon 的 ping 行
  **逐字节由 monitor 编码器钉住**。
- 2026-08-02 **D 审计打掉的三条「删掉功能本身也全绿」**（这一轮最要紧的收获）：
  ① hello 臂不 `into_client`/不 `register`；② `reply` 臂不路由；③ 控制通道探测直接返回成功。
  根因是**单测走自造客户端、e2e 走真 daemon，两者之间的接缝没有任何判据**。
  处置：把接缝抽成 `attach_inbound_client` / `route_inbound_frame` 并立 `seam_tests`。
  ⇒ 一般化的教训：**「两端各自有测试」不等于「接起来是对的」，接缝要单独有判据。**
- 2026-08-02 **判据被绕过 ⇒ 让违规不可表示（本区第二次用这一招）**：
  「每处 `tokio::io::split` 后面要有 `park`」被**尾随注释**和**split 后先偷写一句**两种
  普通写法绕过（后者就是一次 Hello 之前的写）。改成把切与停收成一个函数、`park` 本身
  `#[cfg(test)]` gate 掉 ⇒ 生产段里**不存在**可写的裸 `WriteHalf`，判据随之变成零命中型。
- 2026-08-02 **`call()` 的超时原先不覆盖写入路径**（D 审计判为阻塞）。死锁链每一环都是本仓
  自己写下的事实：monitor 读侧停 → daemon stdout 反压 → 应答通道满 → daemon 停读 stdin →
  `write_all` 永久 pending → 写队列填满 → `call()` 无视 timeout 永久挂起。
  ⇒ 两段共用一个 deadline。**教训：契约文档里写下的保证要逐条问「它覆盖到哪一步为止」。**
- 2026-08-02 **「超时不摘登记」这条设计在背压路径上不成立**：daemon 侧 cancel 的两条应答都是
  `try_send`，通道满时静默丢弃 ⇒ 那条 id 永远等不到帧，每次超时吃 2 格、128 次封死且不自愈。
  ⇒ `register` 满之前先用 `oneshot::Sender::is_closed()` 回收「调用方已走」的登记。
- 2026-08-02 **铁律 4 我自己犯了一次**：计划「不做什么」写着「第一个生产调用方是 2b 的
  `launch`」，实现却给「测试连接」加了真发 ping 的探测。应当**先改计划再做**。
  裁决是保留（它是唯一在真 SSH 上跑过客户端的路径）+ 订正计划，并在 feature 文件里留档。
- 2026-08-02 **护栏公共机件再收敛一步**：剥法从 daemon 私有的 `guard_support` 搬进共享 crate
  `guard-core`（两侧 dev-dependency）。动因：monitor 够不着它，那边的守卫各写便宜近似
  （`split("\n#[cfg(test)]").next()`），在 `ssh_source.rs` 上会把扫描面砍掉三分之二。
  顺带把锚点放宽到复合 cfg —— `session_map.rs::linux_liveness` 的 5 个 `#[test]`
  一直漏剥，是新加的 monitor 全树自检第一次跑就逮出来的。
- 2026-08-02 **补账：`usage-core`（U7-2）与 `acct-core`（U7-3）此前一条 CI 步骤都没有** ——
  test/fmt/clippy 三样全缺，12 条测试在 CI 里等于不存在，而 `ci.yml` 里就写着那条纪律。
  **它们恰恰是「改内核一处 ⇒ 三侧同时红」那条验收标准的载体，漏跑等于把标准关掉。**
- 2026-08-02 账本：新增 **S20**（共享 crate 家族约定）· **S21**（monitor 侧写半边）·
  **S22**（`inbound_client → ssh_source` 反向边，归 U1b）；修掉 **S16 撞号**
  （入方向命令面 / `npm test` 套件链两条同号，而本轮恰好同时碰了它们 ⇒ 后者改 S19）。
- 2026-08-02 **U8a-2b 闭环：daemon 侧长出真正的「远端执行面」（平面 ②）。**
  `control/launch.rs` 一条 `launch` 命令：**argv 直传不过 shell**（引号/转义/注入这一整类问题
  在这条路上**不存在**，不是被挡住了）· `send-into` 成为一等模式 · **不 attach**（平面 ③ 结构上搬不了）·
  失败语义分「没起成 / 起了但没确认」。真 tmux e2e 27 条（建会话 / `@ccm_sid` / 载荷真落 /
  幂等短路 / #76 反向防线），私有 socket 隔离。
  ⚠ **生产路径没切**：tauri 命令收到的已经是渲染好的 shell 串，拆不回结构化计划 ⇒ 登记 **U8a-2c**（依赖 U8c）。
- 2026-08-02 **用户中途点名的架构题 ⇒ 两份设计稿落盘**（`DESIGN-命令面怎么长大.md` /
  `DESIGN-CC新功能的扩展缝.md`），由两个专职设计 agent 并行产出、主线程逐条复核。
  **它们在实现中途就逮到两条真缺陷**，比事后审计值：
  ① **`cancel` 对同步处理器无效，而 `launch` 就是同步的** —— 客户端收到 `Cancelled`，
     而远端 tmux 会话照样建出来。控制面在骗调用方。⇒ `not_cancellable`。
  ② **阻塞处理器跑在 tokio worker 上** —— 单核机器（Pi，正是目标机型）上一条 `launch`
     占住唯一 worker，把出方向 writer 一起卡死，症状是「远端还活着但一句话不说」。⇒ `SpawnBlocking`。
- 2026-08-02 **第 10 条纪律当场又验证一次**：`not_cancellable` 的第一条判据直接调 `spawn_handler`，
  把 `dispatch` 里那一档改回可取消它**照样绿**。⇒ 补 `the_dispatch_table_puts_blocking_commands_on_the_blocking_arm`
  （按 `dispatch` 返回的**变体**判，不是扫文本）。**「两端各自有测试」≠「接起来是对的」。**
- 2026-08-02 **`spawn_registry` 的扫描面是手写文件名单、没有反查** —— 新增 `control/launch.rs`
  起 `tmux` 时**落在盲区**，D1 那条 DoD 不落地也不会红。这是「扫描面画小了」那一族的**第五次**，
  而且是 D1 那轮我自己埋的（同文件上方的 `scan()` 早就因同样理由改成递归了）。⇒ 递归遍历 + 文件数自检。
- 2026-08-02 **护栏自称的强度比实际高一档**（`protocol_doc_guard` 头注写「必须落进 §10 的两张表」，
  实现是 `DOC.contains()` 全文子串）；**命令名的文档对拍能被同名字段白嫖**（§10 帧字段表里本来就有
  `status`/`name`/`sid`，一条叫 `status` 的命令零文档直接通过 —— **已实证**）。两条都收紧。
- 2026-08-02 **趁 `launch` 还没有仓外消费方，把错误码分成两层**：协议级（`inbound.rs` 独占）
  vs 命令级。`launch` 的形状错误改叫 `invalid_args`。`resolve` 那条**刻意不动** ——
  它与仓外 aterm 的一次性契约冻结在 2026-07-18，两条路复用同一纯函数。如实登记，不顺手改。
- 2026-08-02 **设计稿 B 的两条读面实测，刻意不塞进本轮**（本轮是控制面，混进去 commit 讲不清一件事）：
  ① **CC 在 17 天里加了 3 个 jsonl 记录类型**（`started`/`result`/`fork-context-ref`），
     而仓里没有任何东西知道 —— `INVARIANTS §18.1` 记的数字（7 种/8,774 条）已过期（今天 10 种/27,696 条）；
  ② **`subagent.rs` 对 CC 新的 `subagents/workflows/wf_*/` 目录形状今天就加载失败**（非递归 + 匹配
     `description`，而那类 meta 根本没有该字段）。⇒ 新开 **U-CC1（数据面漂移记账）** 与一条 bugfix。
- 2026-08-02 账本：S16 命令面五处 → 六处；新增 **S23**（tmux `=name:` 精确匹配的第三处副本，
  daemon argv 版，有跨轨对拍；这条有事故背书，新增第四处必须同时加对拍）。
  新开件登记：**U8a-2c**（生产切换，依赖 U8c）· **U8a-2d**（命令面注册表，卡在 U10 之前）·
  **U-CC1**（数据面漂移记账）。
- 2026-08-02 **U-CC1 闭环：数据面漂移记账。** 四个降级点各记一笔有界的账（未知记录 `type` ·
  已知类型解析失败 · 未登记的会话 `kind` · daemon 未知能力 token）+ 设置面板一节。
  **零行为变化**：不改任何白名单、不新增 warn、不新增轮询，`parse_line` 输出逐字节不变。
- 2026-08-02 **它把「CC 变了」从不可观测变成看一眼就知道 —— 而这条是实测出来的，不是设想**：
  `INVARIANTS §18.1` 记的是 7 种未知 type / 8,774 条 / 157,385 行（2026-07-16）。
  17 天后复测：**10 种 / 27,747 条 / 472,115 行**，新增 `started` · `result` · `fork-context-ref`。
  **是人手工扫语料才发现的** —— 那正是问题所在。§18.1 已更新并注明「以后靠那一页看，
  别再往散文里手抄数字」。
- 2026-08-02 ★ **验 agent 的断言，三条里打掉一条**（血泪 7 的正例）。设计稿 B 说
  「`subagent.rs` 对 CC 新的 `subagents/workflows/wf_*/` **今天就加载失败**」，
  两个成因（非递归 + meta 无 `description`）**都属实**，但那条路径**今天根本走不到**：
  展开子 agent 的入口是 `AGENT_PROFILE.agentTools = {Agent, Task}`，而产出 workflow 的那个会话里
  是 `Agent`×135 + **`Workflow`×1**、`Task` 0 个 —— `Workflow` 不在集合里 ⇒ 不生成可展开的卡；
  那 135 个 `Agent` 卡**135/135 全部命中**。且全机器只有 **1 个** `wf_*` 目录（agent 说 7 个）。
  ⇒ 正确说法是「**`Workflow` 是 CC 的新工具面，我们还不支持**」（缺功能），不是缺陷。
  **登记不做**（一个实例、无用户可见故障、且 CC 侧 meta 缺 `description`，自动定位原理上做不到）。
  **差一点就照着它「修」了一个不存在的缺陷。**
- 2026-08-02 **全局状态的测试必然 flaky，除非它跑在局部实例上。** 漂移账本是进程内全局的，
  **任何跑过 `parse_line` 的测试都会往里写**。第一版单测「reset + 断言整表形状」⇒
  6 次全量跑红 4 次；加一把跨模块串行闸**也救不了**（那些测试不知道有这把闸）。
  正解：把 `record`/`snapshot` 拆出**显式传账本**的纯形式，单测跑局部账本；
  接缝测试碰全局但写成**容忍污染**的形状。**连跑 5 次确认稳定，不是跑一次就宣布修好。**
- 2026-08-02 **新增一个 tauri 命令，七族既有护栏全部咬人**（逐条处置，这是本仓最值钱的资产之一）：
  parity ledger 三条（登记 / 命令数 123→124 / 能力数 50→51 / Local·Both 68→69）·
  generated-boundary 三条（ts-rs 源文件 27→28 / 生成目录清单 / **`u64` 没配 `ts(type)` 会回落成
  `bigint`**）· paste-block 白名单 · commands 三条计数 · panel-groups 叶子块 14→15。
- 2026-08-02 **clippy「零新增」要用集合差，不是比总数**：中途多出一条
  `reset_for_test` never used（重写测试后它真的死了），是 `git worktree` 拉 HEAD 做
  set difference 才抓到的。删掉后新增告警 0。
- 2026-08-02 订正 `cards/slash.ts` 一条被语料证伪的注释：原文说「`/clear`、`/help`、`/model`
  等 CLI-only 命令不会写 JSONL」，实测 `<command-name>` 共 **56 种**，`/model` **74 条**、
  `/context` 11、`/login` 4、`/doctor` 3、`/ide` 3、`/exit` 3 都在；只有 `/clear`、`/help` 是 0。
  该渲染器**已完全数据驱动**，56 种命令名零改动跑通 —— 加白名单是负收益。
- 2026-08-02 账本新增 **S24**（数据面漂移记账的四个落点）。新开件登记：
  **U-CC1 ✅** · TS 侧「未登记 status」面另记 · `Workflow` 工具面支持（登记不做）。
- 2026-08-02 **U8b 闭环：平面 ③ 的判定 —— 不新增功能，改掉一句撒谎的文案。**
  重测 `launch.rs:304`：**不是 bug**（剪贴板兜底刻意、功能不变砖）。真问题在旁边 ——
  它把一条**既定设计**报成「拉起失败」，而且文案里的 `(v1)` 暗示「v2 会支持开窗」，
  **方向是反的**：L1 早就裁决过「POSIX 上没有『唯一的终端』，挑一个是平白引入一个
  会在别人机器上错的决定」。Linux 用户每次点 ↗ 都在读那句谎话。
  ⇒ 文案说实话 + 零命中护栏（生产段不许出现任何终端模拟器）+ 决定从一句代码注释
  提升进 `ARCHITECTURE.md` 与双语 README。
- 2026-08-02 **前端分档按「后端自己的声明」，不按 `hostOs`。**
  `hostOs !== "windows"` 会把**真失败**（配置缺失 / 命令被拒 / spawn 崩）也一起软化成
  「这是设计」—— 那是另一种撒谎。改成匹配后端那句话里的稳定标记，并配**反面测试**
  （真失败仍叫「拉起失败」）+ 跨轨对拍（Rust `include_str!` 读 TS 里的标记字面量）。
- 2026-08-02 **真正缺的那一半（远端 POSIX 拉起）与 U8a-2c 是同一个阻塞，不硬补。**
  本机 resume 有 OS 分派、远端没有 ⇒ POSIX 上只复制命令、会话根本没建。
  按 L1 形态补齐在语义上成立（`new-session -d … && send-keys …; tmux attach …` 里
  前两步无 TTY 也成功），**但**渲染好的 shell 串拆不回「只建不接」：
  `tmux attach` 必然失败 ⇒ 退出码永远非零 ⇒ **分不出「auth 失败」与「只是没 TTY」**
  ⇒ fire-and-forget 会静默失败，比今天更糟。等 U8c 让前端发结构化请求后走 daemon 的 `launch`。
- 2026-08-02 ⚠ **又一次把测试块插进了「`#[test]` 属性与它的 fn」之间**（第三次），
  受害者是 L1 那条逐字节对拍 `local_and_remote_share_the_same_payload`：**它丢了属性、
  变成死代码，而 `cargo test` 全绿**（它只是不再跑了）。
  是 **clippy 集合差**抓到的（`duplicated attribute` + `never used`）。
  ⇒ 血泪 12 后半句又救一次；也是血泪 10 的另一种形态：**判据自己被摘掉时，没有任何判据会红。**
- 2026-08-02 账本新增 **S25**（平面 ③ 的 POSIX 取舍 + 零命中护栏 + 前后端标记对拍）。
- 2026-08-02 **U8a-2d 闭环：命令面注册表。** `CommandSpec { name, doc_anchor, codes, fields, run }`
  + `Run::{Async, Blocking, Builtin}`，`dispatch` 改成查表。**净效果：文本扫描型护栏 −1
  （删掉扫分派臂文本、按 8 空格缩进切的 `hello_commands_match_the_dispatch_table`），
  数据对数据 +1，新增护栏 +4。** 名字与处理器绑在同一个值里 ⇒ 「声明了却不接 / 接了却不声明」
  **在注册表这一侧不可表示**；`COMMANDS` 保留为镜子（monitor 与 e2e 都在文本抽取它）。
- 2026-08-02 **被删的那条护栏是被它自己的计数自检送走的** —— 改查表之后它只切出 1 条臂，
  当场红，而不是静默变绿。**那正是「该退休了」的正确信号形状**。
- 2026-08-02 ★ **第三轮验 agent 断言、第三次打掉一条**：设计稿 R2 说
  「`control`/`observe` 不许 derive serde，白名单**恰好一条** `resolve_query`」——
  实测是 **3 个文件 6 个类型**，而且后两个**搬进 `wire/` 是错误归类**：
  `fork_write::ForkResult` 是一次性子命令的**出参**，`accounts_query::RawAccount`
  **根本不是 wire**（它在解析 cc-acct-iso 写的清单**文件**）。
  ⇒ 目标不变、形状改成**登记制**（逐条列举 + 写明它是什么 + 机检 + 幽灵条目检查）。
  「上线类型收进 `src/wire/` 目录树」等真出现第二个流协议类型文件时再做。
- 2026-08-02 **R3 闭掉了设计审计 P2**：命令的 `args`/`data` 字段名此前**一个都不在护栏视野内**
  （`every_wire_field_appears_in_the_protocol_doc` 只读 `wire.rs`，而命令载荷两端都不在那儿）。
  现在每条命令的字段必须出现在**它自己那一小节**里。
  ⚠ `fields` 是**手写镜子**，所以必须再钉一层（与解析器/输出构造器实测对拍）——
  **用一个手写清单去证明另一个手写清单是没有意义的**。
- 2026-08-02 **R4 错误码分两层落地**：协议级闭集（`line_too_long` / `unknown_command` /
  `duplicate_id` / `handler_panicked` / `not_cancellable`）**只有 `inbound.rs` 可以发**，
  `control/` 生产段零命中。⚠ `resolve` 的命令级 `bad_request` 与协议级同名是
  **登记在案的例外**（与 aterm 的一次性契约冻结在 2026-07-18，改它会破坏那份契约）。
- 2026-08-02 账本：**S16 改写**（命令面从「N 处副本互相对拍」→「注册表 + 三条派生 + 一面镜子」）；
  新增 **S26**（`control`/`observe` 的 serde 类型登记制）。
