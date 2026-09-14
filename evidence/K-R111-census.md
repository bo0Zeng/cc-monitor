# K-R111 普查原文 —— **乙一那 14 个 id 今天到底还剩多少接线活**（第一拍：只读摸底）

> 件文件 `§3`/`§8` 只放结论与分母，逐处读数住这里。
> 🔴 **本拍只读**：一个字节的生产代码都没改；一条开发测试都没跑（`K31`）。

## §0 量点 —— 每个数都写「哪个面 · 什么量法 · 量于哪棵树的哪个提交」

| 项 | 值 |
|---|---|
| 工作树 | `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r111`，分支 `track/k-r111` |
| **量于** | **`b0953e5`**（`git log --oneline -1` 现打于 2026-09-13 22:42） |
| 量具 | `evidence/K-R111-ruler.py`（本件新写，只读；住址就是这一份，别处没有同名的） |
| 被测对象 | `--src-root <树>/src-tauri/src` ＋ `--daemon-root <树>/remote-daemon-proto/src`（两个都是入参 ⇒ 同一把尺子量得了任意一棵树） |
| 复跑 | `python3 evidence/K-R111-ruler.py`（人读）· `--json`（机器读）· `--drop-baseline`（阴性对照） |

⚠ **主干在本件这一拍里动了三次** —— 读数必须钉在 sha 上，别写「今天主干是几」（那句话下一次合并就自动变成假话）：

| 主干尖 | 什么时候 | 带进来的 | 本尺子跑它 |
|---|---|---|---|
| `b0953e5` | 本件**基点** | —— | 14 / 4 / 13，绿 |
| `8389771` | 22:42 现打 | `track/k-r102` | 14 / 4 / 13，绿 |
| `fb83cf1` | 22:50 现打 | `track/k-r109`（`7b8cb05`）＋ `track/k-r110`（`fb83cf1`） | 14 / 4 / 13，绿 |

🔴 **三个尖上那 31 条判定逐条相同**（尺子三趟都退出码 0）⇒ **本普查的结论不因这三次合并而变**。
唯一变的是渲染面尺子A：`b0953e5`/`8389771` 是 **5 行**，`fb83cf1` 是 **4 行**（`K-R109` 摘掉了第 5 行）—— 见 `§E4`。

⚠ **三棵在飞的树一个字节都没碰**（`k-r102` / `k-r109` / `k-r110`；本拍收尾时前者的工作树已撤、后两棵已并进主干）。
只把尺子**指过去读了一趟** `k-r109`（`d9ce975`）—— 见 `§E4`。

---

## §A 分母怎么数的

### A1 主分母：31 条命令 / 14 个能力 id

`parity_ledger::LEDGER` 现打 **147 条命令 / 65 个能力 id**（脚本解析 `const LEDGER` 的三元组表；
它由 `ledger_shape_is_pinned` 与 `lib.rs::generate_handler!` 双向钉死）。
本件只看其中 **14 个 id**，它们名下现打 **31 条命令** —— 与件文件 `§0` 那句「14 个能力 id / 31 条命令」逐字对上。

🔴 **这 14 个是怎么来的（写清楚，别再让它被读成别的东西）**：
`K-R107` 普查 **B1 档 = 19 个 id / 40 条命令**，减去**历史族 5 个**
（`history.list-projects` · `history.list-sessions` · `history.read-session` · `history.branch` · `search.history`）
= **14 个 id / 31 条命令**。
⚠ **B1 是「归属」清单，不是「还有活」清单** —— 它的原意是「该归后端 · 后端已有对侧」。
本件要答的恰恰是「这 14 个里今天**真的还有前端自己实现**的是哪几个」。

### A2 三值判据的口径（只有一个住址：`ruler.py::verdict_of`）

| 档 | 判据 |
|---|---|
| **已完** | 被点名的那个函数体里，`读面/执行面/渲染面` 三类针**一根都没中** ⇒ 前端一处自己的实现都没有 |
| **还有接线活** | 还有自实现，**且**它点名的那条 daemon 原语**今天真在**现打的两张具名表里 |
| **不是接线** | 还有自实现，**而**后端今天没有对侧 ⇒ 要么先补后端能力，要么先要一句产品判断 |

**id 的档 = 名下各条命令的卷积**（`ruler.py::roll_up`）：全已完 ⇒ 已完；
有残留且残留**全部**是接线活 ⇒ 还有接线活（**纯接线**）；
**任一条**是「不是接线」⇒ 不是接线（**不是纯接线**，这个 id 被劈开了）。

### A3 后端分母

现打 daemon 两张具名表：`main.rs::SUBCOMMANDS` **26 条**一次性子命令
＋ `inbound.rs::COMMANDS` **10 条**流式帧命令。**对侧存不存在按这两张表的并集判，不抄名单。**

---

## §B 🔴 14 个 id 逐条三档

### B-已完 · **7 个 id / 14 条命令**

| id | 命令数 | 今天前端还有没有自己的实现 | 它把活交给谁 |
|---|---|---|---|
| `accounts.list` | 2 | **没有** | 远端 `run_list_query --list-accounts` · 本机 `local_query::run_query --list-accounts` |
| `accounts.session-accounts` | 2 | **没有** | 同上，`--session-accounts` |
| `accounts.trust` | 1 | **没有** | 远端 `--account-trust`（`Remote` 独有，本机侧没有这条命令） |
| `usage.aggregate` | 2 | **没有** | 两侧都问 `--usage` |
| `usage.per-account` | 2 | **没有** | `K-R104` 把整条探针编排搬上帧面（`oneshot-session` 等四条帧） |
| `subagent.load` | 1 | **没有** | `Backend::query` 一处分流，本机 `run_query` / 远端 `run_list_query`，同一份出参形状 |
| `launch.send-into` | 1 | **没有** | `launch` 帧的 `send-into` 模式；**坏数据与协议漂移一律不回落** |

### B-还有接线活 · **0 个 id**（但有 **4 条命令**，见下）

🔴 **没有一个 id 是「纯接线活」。** 4 条真接线活分散在两个被劈开的 id 里
（`tmux.manage` 与 `cc-bus.cockpit`），而那两个 id 各自还带着「不是接线」的命令
⇒ 按 `roll_up` 它们落「不是接线」。

### B-不是接线 · **7 个 id / 17 条残留命令**（其中 4 条是接线活、13 条不是）

| id | 命令数 | 残留在哪一面 | 为什么不是纯接线（现打的依据） |
|---|---|---|---|
| `launch.render-cli` | 1 | **渲染面** | 渲染器住在界面进程里（`backend/control/launch_wire.rs::render_ccm_launch` → `CliSpec`），后端另有一份 `control/ccm/plan.rs`；**两份不共享 crate**（`src-tauri/Cargo.toml` 现打：`shell-quote-core` 只共享单引号 quote，载荷编译器与 ccm 调用行决策核 `P4b` 已归位 monitor 侧）。`K-R109` 判 **A**：座今天删不掉 |
| `launch.render-payload` | 1 | **渲染面** | 同上（`render_launch_payload` → `payload::EnvOp`/`WrapSpec`） |
| `tmux.manage` | 4 | 执行面 | 2 条已完（`kill_remote_tmux`/`tmux_send_keys` 只剩后端一条路）· 1 条接线活（`capture_remote_pane`）· **1 条不是接线**（`list_remote_tmux`，见 `§C`） |
| `cc-bus.cockpit` | 7 | 执行面 | 1 条已完 · **3 条接线活** · **3 条不是接线**（`read_cc_bus_state`/`read_cc_bus_inbox`/`cc_bus_spawn`）⇒ **这个 id 是劈开的** |
| `ccm.status` | 3 | 执行面 ＋ 读面 | 三条**全部**没有对侧，而且三条问的不是同一件事（见 `§C`）；探针形态本身是 `R64` 的病灶 |
| `session.launch` | 3 | 执行面 | 三条最终都落到本机那两处 spawn；`launch_wire` 头注逐字「**这是路由事实，不是禁令**」⇒ 先要一句产品判断 |
| `relay.routing` | 1 | **读面** | 自己 `fs::read_to_string` 读那份中转凭据表；后端 `relay/creds.rs` 读的是**同一份文件**，但 26 条子命令里**没有一条**把「路由表长什么样」答出来 |

### B-对账

| 档 | id | 命令 |
|---|---|---|
| 已完 | **7** | **14** |
| 还有接线活 | **0** | **4** |
| 不是接线 | **7** | **13** |
| 合计 | **14** | **31** |

---

## §C 逐条命令 —— **那一处到底在做什么**（不数 grep，逐处看过）

> `KR111D1` 的失效方向逐字：「按 `fs::` / `Command::new` 的 grep 计数分档 ——
> `fs::` 有大量正当用途（路径拼接），数它会把『已完』的判成『还有活』。**要看那一处到底在做什么。**」
> 下表每一行的第四列就是这一步，人读过的。

### C1 已完那 14 条

| 命令 | 住址 | 那一处在做什么 |
|---|---|---|
| `list_remote_accounts` | `accounts.rs:378` | 只做三件事：取远端配置 → `run_list_query(cfg, "--list-accounts")` → 解析 JSON 行。**整份 `accounts.rs` 现打 `fs::`=0 · `Command::new`=0**（裸 grep，全文，含测试段） |
| `list_remote_session_accounts` | `accounts.rs:415` | 同形，`--session-accounts` |
| `check_account_trust` | `accounts.rs:451` | 同形，位置参数各自 `shell_quote` 后拼子命令 |
| `list_local_accounts` | `local_accounts.rs:477` | `spawn_blocking` 里 `run_query(CCM_TARGET_TRIPLE, ["--list-accounts"])`，出参过 `classify_local_accounts` |
| `list_local_session_accounts` | `local_accounts.rs:1165` | 同形，`--session-accounts`；三档降级（后端不在 / 查询失败 / 零行）分得开 |
| `aggregate_usage_all` | `usage.rs:279` | `run_query(--usage)` 后逐行反序列化推给 `Channel`。头注逐字记着退役那一拍与三条诚实边界 |
| `aggregate_remote_usage_all` | `remote_history.rs:184` | `run_list_query(--usage)` |
| `account_usage` / `account_usage_local` | `account_usage.rs:505/524` | 两条同一个函数体（`probe_account_usage`），origin 只是入参。`K-R104` 之后**本模块一个 shell 字符都不渲染** |
| `load_subagent` | `subagent.rs:83`（`Backend::query`） | 两条 transport 一处分流；本机 `run_local_query` / 远端 `run_list_query`，出参形状对齐 |
| `daemon_send_into` | `backend/control/daemon_launch.rs:168` | `client.call("launch", …)`；坏数据与协议漂移**都不回落**（回落会让「键没键入」变成未知） |
| `kill_remote_tmux` | `tmux.rs:670` | `K-R72` 已删 SSH 回落，只剩 `daemon_kill`；三态 `Done/Refused/NoChannel` 都 return |
| `tmux_send_keys` | `tmux.rs:709` | 同上，只剩 `daemon_send_keys` |
| `cc_bus_send` | `cc_bus.rs:1477` | `K-R98` 之后**函数体里连「本机怎么走 / 远端怎么走」这个分支都没有了**，只剩 `send_via_daemon` |

### C2 还有接线活那 4 条 —— **后端对侧今天就在**

| 命令 | 住址 | 那一处在做什么 | 对侧（现打在 daemon 表里） |
|---|---|---|---|
| `capture_remote_pane` | `tmux.rs:618` | `build_capture_pane_cmd` 拼串 → `connect_and_exec_cmd` 一次性 SSH。**本机那一支今天直接回一句「还看不了」** | `--capture-pane`（子命令）＋ `capture-pane`（帧）—— `K-R86` 补的 |
| `check_cc_bus_agent_online` | `cc_bus.rs:403` | **主路已经走 `online_via_daemon`（`bus-list`）**，答不上时回落到 `build_online_cmd` + `local_shell_read`/`exec_read` ⇒ 残的是**回落** | `bus-list` |
| `cc_bus_broadcast` | `cc_bus.rs:1501` | **主路已经是 `broadcast_via_daemon`（`bus-list` ＋ 逐个 `bus-send` 的组合）**，远端拿不到通道时回落到 `build_broadcast_cmd` + `exec_read` ⇒ 残的是**回落**（本机本来就没有回落） | `bus-send`（广播是组合，不需要新原语） |
| `cc_bus_kill` | `cc_bus.rs:1537` | 主路就是 SSH：`refuse_local_write` 拦本机 → `build_kill_cmd` + `exec_read` ⇒ **整条还没改走** | `bus-kill`（帧 ＋ 子命令都在） |

### C3 不是接线那 13 条 —— **后端今天没有对侧，或者先要一句产品判断**

| 命令 | 住址 | 那一处在做什么 | 缺的是什么 |
|---|---|---|---|
| `render_ccm_launch` | `launch_wire.rs:134` | 在界面进程里把 `CliSpec` 渲染成 ccm 调用行 | wire 上**三态压成两态**（`K-R95` 登记的缺口：`ccm-probe` 上游是 `installed`/`not-installed`/`unknown` 三态，`is_ssh`/`caps` 这条线只有两态）⇒ TS 兜底座今天删不掉（`K-R109` 判 A） |
| `render_launch_payload` | `launch_wire.rs:248` | 同上，载荷编译器 | 同上 |
| `list_remote_tmux` | `tmux.rs:259` | 自己拼 `tmux ls -F` 一条 SSH 串（带 `command -v tmux` 门控与 `NO_TMUX` 哨兵） | **daemon 没有「一次性拉一份 tmux 列表」的原语**（26 条子命令 + 10 条帧里都没有）。它有的是**推**过来的 `Frame::TmuxSessions` 快照，monitor 侧账本 `ssh_source::tmux_raw_for(origin)` **对任意 origin 都拿得到**（`list_local_tmux` 就是这么写的）。🔴 **但改用快照不是纯接线**：`tmux.rs:300` 那段头注逐字写着快照的 `command` 那一列**可能是陈旧的**（`HOOK_EVENTS` 只覆盖会话名集合的三件事），而 `tabs.ts::awaitExitFor` 等的正是 pane 前台命令的变化 ⇒ **要一句产品判断，或者给后端补一条一次性拉取** |
| `read_cc_bus_state` | `cc_bus.rs:369` | 本机 `local_shell_read(CC_BUS_CAT_CMD)` / 远端 `fetch_remote_cc_bus`（同一条串包进 ssh）。那条串**读两张 tsv**：`agents.tsv` ＋ `spawned.tsv` | daemon 的 `bus-list` 只回 `{"agents": …}`（`control/cc_bus.rs::list_for_inbound`，转调 `cc-list` 再补一次身份空间）。🔴 **`spawned` 那半没有对侧** —— `grep -rn "spawned" remote-daemon-proto/src/control/cc_bus.rs` = **0 命中**，`spawned.tsv` 全仓 daemon 侧 **0 命中**。而 `cc_bus.rs` 自己的头注逐字写着解锁条件**不是**「`P4b` 落地」，是「**格式契约稳下来**」，届时正确形状多半是「daemon 出一条**具名的读命令**」 |
| `read_cc_bus_inbox` | `cc_bus.rs:1453` | `build_inbox_cmd`（`tail`）→ `local_shell_read` / `exec_read` | daemon 侧 `inbox` **2 命中，都不是读口**（一处在 `cc_bus_boundary_guard` 的字面量拼接，一处在 `readonly_guard` 的反例串）⇒ **没有对侧** |
| `cc_bus_spawn` | `cc_bus.rs:1560` | `build_spawn_cmd` → `exec_read`；本机被 `refuse_local_write` 拦着 | daemon 帧面 10 条里没有 spawn；`cc-spawn` 只以 shell 脚本形态存在于 `shared/cc-bus/scripts/` ⇒ **要先给后端补能力** |
| `cc_integration_status` | `profile_installer.rs:502`（`scan_profile`） | `fs::read_to_string` 扫两个 PowerShell profile，报「cc 块装没装 / 有没有遗留块」 | 🔴 **这一条恐怕根本不该在 B1**：它动的是**安装面**，定框 `K27`「部署是产品的一部分，由客户端做」＋ `K34` 已经把这一族判在客户端侧（`K-R107` 普查 B3 的第二行逐字收了 `ccm.install`/`ccm.install-ui` 那 13 条）⇒ 提请 PM **重判它的归属**，不是补后端 |
| `local_ccm_entry_status` | `ccm_probe.rs:182`（`probe_binary_uncached`） | 起两次进程问身份：`<我们那份 ccm> --ccm-probe`（不经 shell）＋ `bash -lic <常量探测串>`（问 PATH 上那个是谁） | 后端答得出「**我**是谁」（`--ccm-probe` 就是它自己那份），答不出「**你 PATH 上那个**是谁」——那要在用户的登录 shell 里跑一趟，daemon 今天没有这条口 |
| `probe_ccm_cli` | `ccm_probe.rs:250` | 一条常量 SSH 串 `CCM_PROBE_CMD`（`command -v ccm` 门控 + `NO_CCM` 哨兵） | 同上；而且这整个探针形态是 `R64` 的病灶（用户逐字「不存在什么没装 ccm 装了后端的情况」）⇒ 要连「有后端 ⟺ 有 ccm」一起处置 |
| `resume_history_session` | `launch.rs:290`（`launch_local_posix_via`） | 本机 POSIX：渲染出命令后 `Command::new(终端)` 直接 spawn，**不绕 IPC** | 后端**有**对侧（`--launch` / 帧 `launch` / `--oneshot-session`），但 `launch_wire` 头注逐字「POSIX 本机那条路住在 Rust 里，**不必绕一圈 IPC 问自己** ⇒ 这是**路由事实**，不是禁令」⇒ **先要一句产品判断**：`K28` 裁定二要不要吃掉这条捷径 |
| `new_local_session` | 同上 | 同上 | 同上 |
| `launch_remote_terminal` | `launch.rs:500`（`launch_powershell_window`） | 拉一个真终端窗口（`wt.exe`/`powershell.exe`；POSIX 臂回 `POSIX_NO_TERMINAL_WINDOW` 让用户自己粘） | 与 `terminal.focus`（`K-R107` 判 **B4 要用户裁**）**是同一件事**：`K28` 裁定二说调出终端该找本地后端，而本机后端今天是 headless、**没有窗口原语**（26+10 条里零窗口原语）⇒ 缺一条产品判断 |
| `relay_routing_for` | `history.rs:1968`（`relay_rows_at`） | `fs::read_to_string` 读 `~/.claude/claudecode-frontend/<creds 文件>`，解析出账号 id 列表；再问 `local_daemon::relay_running`（monitor 自己起没起中转） | 后端 `relay/creds.rs` 读**同一份文件**，但**没有一条子命令把「今天路由表里有哪几行」答出来**；而这份文件住在 **monitor 自己的数据目录**里 ⇒ 归属本身要一句话（`K28` 裁定一「自己的配置」vs 裁定二「一切对外经后端」） |

---

## §D 复量 PM 那几个零散读数 —— **两条证实、四条推翻**

> `brief` 13：转述别人的结论要么自己重打，要么写明「这个数我没重打」。下面每一条都重打了。

| PM 的话 | 复量结果 | 量法 |
|---|---|---|
| `accounts.rs` `local_query`=0 · `fs::`=0 · `Command::new`=0 —— 一处自己的实现都没有 | ✅ **证实**（三个数逐字对上）。⚠ 但要补一句：`local_query`=0 **不是因为它没交出去**，是因为 `accounts.rs` 是**远端那一半**，它走的是 `run_list_query`（4 处）；本机那半在 `local_accounts.rs` | 裸 `grep -c`，全文含测试段 |
| `usage.rs` `fs::`=1 | ✅ 数对了，但**那一处是 `use std::fs::File;`（第 14 行的 import）**，不是一次读。真正的 `File::open` 有 2 处，在 `analyze_codex_usage`/`analyze_usage_in_session` —— 而 `aggregate_usage_all` **不调它们**（它已经走 `--usage`） | 生产段扫针 + 逐处读过 |
| `subagent.rs` `fs::`=6 | 🔴 **推翻**：生产段 **0 处**。6 处**全在 `#[cfg(test)]` 里**（`mod tests` 从第 252 行起） | 剥 `#[cfg(test)]` 后重数 |
| `local_accounts.rs` `fs::`=15 | 🔴 **推翻**：生产段 **2 处**，都在 `read_capped`（`fs::metadata` 判大小 + `fs::read` 读那份 manifest）。另 13 处全在测试沙箱里（`mod tests` 从第 487 行起）。⚠ 而那 2 处**也不在这 14 个 id 的路上**：`list_local_accounts`/`list_local_session_accounts` 两条命令体里一处都不碰它 | 同上 |
| `cc_bus.rs` **9 处 `Command::new`** | 🔴 **推翻**：生产段 **1 处**（`local_shell_read:1063`）。其余 8 处 = 6 处在**文档注释**里（709/2806/3632/3643/3645/3655）＋ 2 处在测试段（2822/2863） | 同上，逐处点开看过 |
| `cc_bus.rs` 的 `Command::new` **占 `EXEC_SITES` 3 条** | 🔴 **推翻（作用域对不上）**：`EXEC_SITES` 的人群是 **`connect_and_exec_cmd(` 的调用点**（它自己的头注逐字：「本条只管 `connect_and_exec_cmd` 这一个扼流点」），**它一条 `Command::new` 都不数**。`cc_bus.rs` 在 `EXEC_SITES` 里确实占 3 条，但那是三处**远端 SSH**（`fetch_remote_cc_bus`/`check_cc_bus_agent_online`/`exec_read`），与那 1 处本机 `Command::new` 是两个面。本机执行面的尺子是 **`write_site_registry::SPAWNS`**（现打 15 条） | 解析两张具名表 |
| `relay.routing` 在普查里「挡路的」栏写着**「无」** | 🔴 **推翻**：`--relay` 是「**把中转跑起来**」，不是「**告诉我路由表长什么样**」。26 条子命令里没有后者 ⇒ 挡路的不是「无」，是**没有对侧 ＋ 归属没裁** | 现打 `SUBCOMMANDS` + 读 `relay_rows_at` |
| PM 预判「真正还有接线活的大概只有 **4–6 个 id**」 | 🔴 **推翻（但数字巧合）**：按 **id** 数是 **0 个**（没有一个 id 是纯接线活）；按**命令**数恰好是 **4 条**，分散在 2 个被劈开的 id 里。⇒ **单位错了一格**，这正是本件要治的那种错的同族 | 见 `§B`/`§C2` |

---

## §E 四把现成尺子的现打读数 —— 以及**每一把的作用域对不上事实的地方**

> ★ 本区最高频的错是「量具的作用域对不上事实」。下面每一把都写清它**数的是什么、数不到什么**，
> 以及**属于这 14 个 id 的有几条**。

### E1 读面 · `local_read_surface_registry::REGISTERED`

现打：登记 **22 行**，其中 `reader` 类 **8 个模块 / 34 处**
（`plugins.rs`5 · `search.rs`4 · `adapter.rs`3 · `tasks.rs`3 · `mcp.rs`8 · `adapter/claude_code.rs`1 · `config_surface.rs`3 · `hooks_diag.rs`7）。

🔴 **属于这 14 个 id 的：0 处。**
PM 说的「属于乙一的只有 `src/search.rs`」是对的 —— 但 `search.history` 属于**被减掉的历史族**，
**不在这 14 个里** ⇒ 对本件而言这把尺子的读数是 **0**，它一条活都指不出来。

⚠ **它数不到什么**：针是闭集（`claude_dir` / `CLAUDE_CONFIG_DIR` / `.claude/projects` / `records_dir` / `.claude`），
⇒ **读 monitor 自己数据目录的那些点它一处都看不见**。活体就在本件里：
`relay.routing` 的 `relay_rows_at` 真的在 `fs::read_to_string`，而它**不在这张表的人群里**。
**「读面尺子是 0」不等于「没有人在读」。**

### E2 执行面·远端 · `exec_site_registry::EXEC_SITES`

现打 **16 条**（`K-R109` 订正 PM 报的 19 —— 复量证实是 16）。
属于这 14 个 id 的文件占 **8 条**：

| 条目 | 归谁 | 它是不是「自实现」 |
|---|---|---|
| `cc_bus.rs::fetch_remote_cc_bus` | `cc-bus.cockpit` | **是** |
| `cc_bus.rs::check_cc_bus_agent_online` | `cc-bus.cockpit` | **是**（回落那一半） |
| `cc_bus.rs::exec_read` | `cc-bus.cockpit` | **是**（inbox/broadcast/kill/spawn 四条的转发者） |
| `ccm_probe.rs::probe_ccm_cli` | `ccm.status` | **是** |
| `tmux.rs::capture_remote_pane` | `tmux.manage` | **是** |
| `tmux.rs::list_remote_tmux` | `tmux.manage` | **是** |
| `remote_history.rs::run_list_query` | accounts/usage/subagent 的**传输层** | 🔴 **不是** —— 这是「调远端后端」那条路本身 |
| `remote_history.rs::stream_read_remote_session` | 历史族（不在这 14 个里） | 不是 |

⚠ **`EXEC_SITES` 的条数不等于「还有几处自实现」**：它的人群是「有几处远端执行」，
而**把活交给远端后端也要执行一次**（`run_list_query` 就是）。⇒ **6 条是活，2 条是路。**

### E3 执行面·本机 · `write_site_registry::SPAWNS` —— **PM 那三把尺子漏了这一把**

现打 **15 条**。属于这 14 个 id 的 **6 条**：

`ccm_probe.rs::probe_with` · `ccm_probe.rs::probe_binary_uncached`（→ `ccm.status`）·
`cc_bus.rs::local_shell_read`（→ `cc-bus.cockpit`）·
`launch.rs::launch_local_posix_via` · `launch.rs::launch_powershell_window` · `launch.rs::ssh_client_available`（→ `session.launch`）。

🔴 **这把尺子非有不可**：`session.launch` 三条命令的自实现**全部**落在这里，
而它们在读面（0）、`EXEC_SITES`（0 条）、渲染面（0 行）三把尺子上**一处都不露面**。
只看 PM 给的三把尺子，`session.launch` 会被读成「已经干净了」。

### E4 渲染面 · `TS_FALLBACK_KEEPERS`（尺子A）/ `TS_FALLBACK_REACH`（尺子B）

| 量于（钉在 sha 上） | 尺子A 行数 | 尺子B 家数 / `On` |
|---|---|---|
| **`b0953e5`（本件基点）** | **5** | 4 / 2 |
| `8389771`（22:42 的主干） | **5** | 4 / 2 |
| `track/k-r109` 的 `d9ce975`（当时在飞） | **4** | 4 / 2 |
| **`fb83cf1`（22:50 的主干，`K-R109` 已并）** | **4** | 4 / 2 |

🔴 **PM 报「尺子A = 4 行」的那一刻，主干还是 5 行** —— 那个 4 量的是 `k-r109` 那棵在飞的树。
第 5 行是 `SESSION_BACKEND@src/remote-launch-run.ts×2`，`K-R109` 摘掉的正是它。
**本拍进行中 `K-R109` 合进了主干（`7b8cb05`）**，于是那个 4 从「在飞树上的数」变成了「主干的数」——
⚠ **这恰恰说明为什么每个数都要钉 sha**：同一句「尺子A 是 4」，22:42 说是错的、22:50 说是对的
（`K-R105` 那条病的活体：**一个数不写清它量在哪棵树的哪个尖上，就是半句假话**）。

⚠ 尺子A 与尺子B **问的不是同一件事**（`R72` 已订正）：**进度看尺子A ＋ `EXEC_SITES`，收工看尺子B 的 `On` 归零。**
本件把这把尺子指到 `k-r109` 那棵树上跑过一趟：**14 个 id 的 31 条判定一条都没变**
⇒ `K-R109` 落地**不改**本普查的任何结论，只把尺子A 从 5 拧到 4。

---

## §F 死值验 —— 五刀，逐刀记锚点与命中数

> 🔴 **全部跑在副本上**（`/tmp/…/scratchpad/kr111/cut{1,4,5}/`），
> 生产树与三棵在飞的树**一个字节都没动**（收工时 `git status --short` 三处都贴在件文件 `§8`）。
> 每一刀都**先断言锚点恰好命中 N 次再改，并打印「变异已落地」**（`brief` 第 7 条）。

| 刀 | 打哪儿 | 锚点命中 | 变了什么 | 读数 | 是不是**正是那一格** |
|---|---|---|---|---|---|
| **①**（DoD 指定） | `local_accounts.rs::list_local_accounts` 体内 `spawn_blocking(\|\| {` ＋ `classify_local_accounts(run_query(` 那两行之间 | **1** | 塞回一处真读：`let _probe = std::fs::read_to_string("/dev/null");` | **1 红**：`[棘轮] list_local_accounts：基线 已完 ≠ 现打 还有接线活 —— 回潮了 ⚠` | ✅ 是那一格、且**只有**那一格（最小面） |
| **②** 阳性回测 | 不改任何东西，干净树 | — | — | `cc-bus.cockpit` 现打落 **`不是接线`**；逐条 `{broadcast:还有接线活, kill:还有接线活, send:已完, spawn:不是接线, online:还有接线活, inbox:不是接线, state:不是接线}` | ✅ 落到了 ⇒ 尺子真跑了 |
| **③** 阴性对照 | 刀① 的副本 ＋ `--drop-baseline`（三档表整段拿掉） | — | — | **0 红，退出码 0** | ✅ 「一条都不红」⇒ 牙确实长在三档表上 |
| **④** 对侧那一列有没有牙 | daemon 两张具名表**块内**各摘掉 `bus-kill` 一行 | `SUBCOMMANDS` 块内 **1** · `COMMANDS` 块内 **1**（⚠ 全文数是 1 与 **4** —— 不收窄到块内就会切错地方，第一趟就撞上了） | 后端分母 26/10 → 25/9 | **2 红**，都在 `cc_bus_kill` 上：`[对侧] 点名的原语不在现打的两张表里` ＋ `[棘轮] 还有接线活 → 不是接线` | ✅ 证明「后端对侧」是从 daemon 源码派生的，不是手写断言 |
| **⑤** 棘轮往**好**的方向也咬 | `tmux.rs::capture_remote_pane` 体内那条 `connect_and_exec_cmd` | **1**（先定位函数再在块内数） | 换成 `daemon_route::capture_pane(...)`（模拟真退役） | **2 红**：`[棘轮] 还有接线活 → 已完 —— 退役了 ✅ 把刻度拧下来` ＋ `[面] 登记为「执行面」而现打命中的是 ['后端']` | ✅ 这是**递减棘轮**要的形状：退完了表跟着变，而且逼人把刻度与面这两列一起拧 |

**量具自检**（`selftest_stripper`，每趟先跑，不过就 `exit 2` 不出读数）：7 格，其中 **2 格是回归格**，
钉的是本尺子首版真栽过的两个坑 ——
① 文档注释里的 `/**非全零**` 被当块注释开头（`usage.rs` 生产段被读成 **64 行**，真值 **211**）；
② 不认字符字面量 ⇒ `'{'` 把大括号配歪，`cc_bus.rs` 的 `#[cfg(test)]` 块提前 **1748 行**「闭合」。
第三个坑（无回归格，改在取体那一步）：**取函数体用「下一个顶格行为界」** ⇒
`pub async fn list_local_session_accounts() -> Result<…>` 把 `{` 单独顶格写，函数体当场被截成一行签名，
两条真·已完的命令被现打成「一根针都没中」。现在按**大括号配对**取。

**稳不稳**：干净树连跑 **3 趟**，`--json` 输出 md5 逐趟相同（`102f724ccfa8e5be9c6ba1ba6bf7b0fd`）。

---

## §G 切件方案（`KR111D2`）—— **按写区不相交切**

> 前提：本拍**不立件**（件文件 `§0d` 明令）。下面是给 PM 的方案，件由 PM 定。
> ⚠ `R72` 已订正：**进度看尺子A ＋ `EXEC_SITES`，收工看尺子B** —— 每一件下面都写清自己看哪一个。

### 第一梯队 · **真接线活，可并发三路，写区两两不相交**

| 件 | 干什么 | 写区（点名文件） | 收工看哪把尺子的哪个数 |
|---|---|---|---|
| **甲1 · cc-bus 三条改走原语** | `cc_bus_kill` 整条改走 `bus-kill` 帧；`check_cc_bus_agent_online` 与 `cc_bus_broadcast` **删回落** | `src-tauri/src/cc_bus.rs` · `src-tauri/src/exec_site_registry.rs`（申报表要跟着少行）· `src-tauri/src/write_site_registry.rs`（`local_shell_read` 那行的去留） | `EXEC_SITES` **16 → 13**（`fetch_remote_cc_bus` 留给甲3；`check_cc_bus_agent_online` 与 `exec_read` 两行出表）· 本尺子：这三条从 `还有接线活` → `已完`，基线跟着拧 |
| **甲2 · 抓屏改走 `capture-pane` 帧** | `capture_remote_pane` 换掉那条一次性 SSH；顺带把本机那一支从「回一句还看不了」变成真能抓 | `src-tauri/src/tmux.rs` · `src-tauri/src/exec_site_registry.rs` | `EXEC_SITES` 少 1 行（`tmux.rs::capture_remote_pane`）· 本尺子该条 → `已完` |
| **甲3 · `read_cc_bus_state` 的前置：给 daemon 一条具名读命令** | 🔴 **这一件先动 daemon，不动 monitor** —— 补 `bus-state`（agents ＋ spawned 一次回全），把 `cc_bus.rs` 头注那句「格式契约稳下来」兑现掉 | `remote-daemon-proto/src/control/cc_bus.rs` · `remote-daemon-proto/src/inbound.rs` · `remote-daemon-proto/src/main.rs` | 后端分母 26/10 → 27/11；本尺子的 `read_cc_bus_state` 从 `不是接线` → `还有接线活`（**这就是它的收工判据**：一件把它从第三档挪到第二档） |

⚠ **甲1 与甲3 都碰 cc-bus，但写区不相交**（甲1 只写 `src-tauri/`，甲3 只写 `remote-daemon-proto/`）⇒ 可并发。
⚠ 甲1 与甲2 都要改 `exec_site_registry.rs` ⇒ **这两件不能并发**，串行或合成一件。

### 第二梯队 · **要先补后端能力**（每件先给 daemon 长一条原语，再回来接线）

| 件 | 缺的原语 | 写区 | 收工看什么 |
|---|---|---|---|
| **乙1 · inbox 读口** | daemon 补「读某个 id 的 inbox」 | daemon 三份 ＋ 之后 `cc_bus.rs` | 后端分母 +1；`read_cc_bus_inbox` 挪档 |
| **乙2 · spawn 原语** | daemon 补 spawn（`K33` 逐字「不要有什么 bash 脚本」⇒ 顺手把 `cc-spawn` 那条 shell 收掉） | daemon ＋ `shared/cc-bus/scripts/` | 后端分母 +1；`cc_bus_spawn` 挪档 |
| **乙3 · 一次性 tmux 列表** | daemon 补「拉一份 tmux ls」，或裁「推送快照够用」 | daemon ＋ `tmux.rs` | `EXEC_SITES` 少 1 行；`list_remote_tmux` 挪档 |
| **乙4 · wire 上的三态** | 补 `K-R95` 那个缺口（`ccm-probe` 三态别在 wire 上压成两态） | `launch_wire.rs` ＋ TS 侧 | 🔴 **这一件的收工看尺子B**：`TS_FALLBACK_REACH` 的 `On` **2 → 0**。⚠ 别拿尺子A 的行数说它 |

### 第三梯队 · **不是代码活，是一句产品判断**（PM 拿去问用户，别派 agent）

1. `session.launch` 三条：本机 POSIX 那条捷径要不要吃掉（`K28` 裁定二 vs `launch_wire` 头注那句「路由事实，不是禁令」）。
2. `launch_remote_terminal` / `terminal.focus`：本机后端要不要长出「操作桌面窗口」这一面。
3. `ccm.status` 的 `cc_integration_status`：**归属重判** —— 它是安装面（`K27`/`K34`），恐怕该从 B1 挪进 B3。
4. `relay.routing`：monitor 自己数据目录下那份中转凭据表，算不算「对外行为」。
5. `ccm.status` 的两条探针：连 `R64`「有后端 ⟺ 有 ccm」一起处置，别单独接线。

---

## §H 诚实边界 / 判不了

1. **本尺子只看被点名的那一个函数体，不追调用链。** 追了的话 `run_list_query` 的体里有 `connect_and_exec_cmd`，
   「走远端后端协议」会被读成「自己拼 shell」。⇒ 射程换成「谁被点名」，而点名是手写的、由「函数找不到就红」钉着。
   **代价**：某条命令的实现如果搬到另一个函数里而名字没改，本尺子看不见 —— 那要下一个人来核。
2. **`面 == "后端"` 那一格没有独立机检**，它只是个标签；承重的是判定本身（有自实现 ⇒ 判定不可能是 `已完`）。
   刻意不给它第二副牙，理由写在 `ruler.py` 那一处注释里（两副牙会在阴性对照那一格替三档表挡枪）。
3. **「后端有对侧」只证明那条原语的名字今天在 daemon 的表里**，不证明两侧**等价**。
   等价要真跑两趟逐字段比 —— `K31` 挡着，**本拍一趟都没跑**。
4. **针是闭集**（住 `ruler.py::NEEDLES`），认不出「换个名字做同一件事」。**比没有强，别读成证明。**
5. **门禁**：⚠ **本拍没跑门禁** —— 沙箱镜像 `ccmon-devbox:latest` 正在重建。
   本拍是只读摸底、**不改生产代码** ⇒ 门禁不是本拍的验收条件；也**没有**退回宿主跑 `cargo`/`npm`（硬红线）。
6. **`ccm.status` 那 3 条命令问的不是同一件事**（安装面 / 我们那份的身份 / 你 PATH 上那份的身份）。
   本尺子按 `LEDGER` 的 id 归组，所以它们卷在一个档里 —— **档是对的，但「这个 id 该不该是一个 id」我判不了**，那是归属重判，落 `§G` 第三梯队。
7. **daemon 的 `bus-list`/`bus-kill`/`bus-send` 三条自己也在 `run("cc-list")` 转调 shell 脚本**
   （`control/cc_bus.rs::list_for_inbound` 现打）—— 按 `K33` 逐字「不要有什么 bash 脚本」，
   **后端那一侧也还有活**。⚠ 这不在本件的题面里（本件问的是前端），**只登记，不判**。
