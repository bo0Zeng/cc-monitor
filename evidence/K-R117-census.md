# K-R117 第一拍（只读摸底）· 现打普查

> **量于 2026-09-14**，工作树 `.claude/worktrees/k-r117`，分支 `track/k-r117`，**HEAD `703b4d0`**。
> 量具：`evidence/K-R117-ruler.py`（**跟着本树走**；被测对象默认 = 它所在目录的父目录，
> `--root` 可指到别处）。下面每一节的数都能用 `python3 evidence/K-R117-ruler.py` 现打重现。
> 🔴 **本件一个字节的生产代码都没改**（三处 `git status` 见件文件 `§8`）。

---

## §A 分母怎么数的 —— 五把源码派生的尺子，逐把写清盖不到什么

| 尺子 | 现打 | 它盖到什么 | **盖不到什么**（诚实边界） |
|---|---|---|---|
| `tool_registry::TOOLS` | **8** 条工具（`installable: true` **7** 条） | 「声明了的受管工具」 | 真实发生的安装动作（那是下一把的事）——本仓头注自己写着这句 |
| `tool_registry::UNMANAGED_ENV` | **11** 条（`AppShips` **2** · `UserProvides` **9**） | 「app 会碰、而没有 `ToolSpec` 的环境项」 | 有没有装口（由 `EnvTier::of` 另算） |
| `tool_registry::claims()` | **8** 条（工具 → 装 / 卸实现住址） | 「哪个符号是哪个工具的装 / 卸实现」 | 它只覆盖 `TOOLS` 这 8 条 |
| `parity_ledger::LEDGER` | **148** 行命令 / **66** 个能力 id | 全部 Tauri 命令 → (能力, 服务哪一侧) | 命令内部两侧行为一不一致（头注自陈） |
| `write_site_registry::WRITE_SITES` | **32** 行，其中带 `Some(tool)` 的 **7** 行 | Rust 侧 `fs::` 写盘落点 | 经 `Command` 起外部进程间接写盘（头注自陈的真缺口） |

住址一律**从源码派生**：命令的 `文件:行号` 取自 `#[tauri::command]` 扫描（现打认出 **148** 条，
与 `LEDGER` 的 148 行**逐条对得上**，一条不缺）。

---

## §B 🔴 `installable: true`：**19 与 7 差在哪** —— 逐行分档，Σ 恰好等于 19

PM 现打的 `grep -c "installable: true" src-tauri/src/tool_registry.rs` = **19**；
`K-R107`（09-13）写的是 **7**。**7 是对的，19 是朴素 grep 的行数**，两个数的差逐行摊开如下
（分母 = 那 19 行本身；同串全文出现 19 次 ⇒ 一行一次，不存在「某行数了两遍」）：

| 档 | 条数 | 行号 |
|---|---|---|
| **真·字段声明**（`^installable: true,$`） | **7** | 430 · 520 · 589 · 646 · 729 · 758 · 790 |
| 被 **`uninstallable: true`** 这个**更长的串**顺带命中 | **5** | 431 · 654 · 730 · 759 · 791 |
| 测试段（`#[cfg(test)]` 起于第 **1376** 行） | **4** | 1830 · 1833 · 2065 · 3627 |
| 文档 / 行注释 | **3** | 175 · 752 · 1825 |
| **Σ** | **19** | —— |

🔴 **PM 的猜测只对了一半，而错的那一半更要紧**：单子里写的是「更可能是 PM 这条 grep 把
**测试段/注释**算进去了」—— 测试段 4 ＋ 注释 3 = 7 条，**只解释了 12 个差额里的 7 个**。
**最大的一档（5 条）是子串问题**：`uninstallable: true` 里**含有** `installable: true`。
这一形与「`fs::` 6→0 · `Command::new` 9→1 · `EXEC_SITES` 19→16」**不同族** ——
那三次是「剥法不对（数进了注释 / 测试）」，这一次是**匹配单位比事实小**
（本仓 `guard_core::pin_exactly_once` 的头注逐字治的就是它：needle 是子串、事实是一整行）。
⇒ 改 grep 为 `grep -cE '^\s*installable: true,\s*$'` 当场给 7。

**两把独立的尺子对拍**（尺子 R2）：按文本行数出的「真·字段声明」= **7**；
解 `TOOLS` 声明表数出 `installable == true` = **7**。两边相等 ⇒ 这个 7 不是一把尺子的自证。

**⇒ 本件此后一律用 7。`K-R107 §F2` 那个 7 站得住，不作废。**

---

## §C 安装面人群 —— 现打 **22 条命令 ＋ 8 处内部动作 = 30 项**

### §C1 · 与 `K-R107 §F2`「21 ＋ 3 / 9 个能力 id / 6 个文件」逐项对账

| 项 | `K-R107`（09-13） | 现打（09-14） | 差在哪 |
|---|---|---|---|
| Tauri 命令 | **21** | **22** | ＋`write_skill_file`。`K-R107 §F2` 的 21 条里**没有** `skill.inbox`，而同一份 `§F3`／件文件 `§0b` 的目标形状里 ③ 明写含「`skill.inbox` **写侧**」⇒ **同一个目标形状下人群就是 22**。差的就是这一条 |
| 能力 id | **9** | **14**（不含 `skill.inbox` 则 13） | 🔴 **9 数的是 `§F2` 那张表的表行数，不是能力 id 数**。那张表有两行各装了两个 id（`cc-bus.deploy`＋`cc-bus.install-state` 一行、`mcp.write`＋`mcp.remove` 一行）、一行装了三个（`acct-iso.*`）⇒ 9 行 = 13 个 id |
| 后端文件 | **6** | **7**（不含 `skill.inbox` 则 6） | 6 站得住；＋`skill_host.rs` |
| 内部动作 | **3** | **8** | 见 C3 |

### §C2 · 逐条落哪一处（**住址从源码派生**，`evidence/K-R117-ruler.py` 现打）

| 归处 | 能力 id | 命令 | `Side` | 今天住哪 | 依据 |
|---|---|---|---|---|---|
| ①装后端 | `acct-iso.check` | `check_remote_acct_iso` | Remote | `acct_iso_deploy.rs:94` | `R75` |
| ①装后端 | `acct-iso.deploy` | `deploy_remote_acct_iso` | Remote | `acct_iso_deploy.rs:167` | `R75`〔用@09-14「account进后端」〕 |
| ①装后端 | `ccm.install` | `cc_integration_install` | Local | `lib.rs:2209` | `R64`＋`K26` |
| ①装后端 | `ccm.install` | `install_remote_ccm_helper` | Remote | `sftp.rs:1153` | `R64`＋`K26` |
| ①装后端 | `ccm.install-ui` | `cc_integration_preview` | Local | `lib.rs:2195` | `R64`＋`K26` |
| ①装后端 | `ccm.install-ui` | `cc_integration_scan_path` | Local | `lib.rs:2224` | `R64`＋`K26` |
| ①装后端 | `ccm.status` | `cc_integration_status` | Local | `lib.rs:2158` | `R64`＋`K26`＋`K-R111` |
| ①装后端 | `ccm.status` | `local_ccm_entry_status` | Local | `ccm_probe.rs:418` | 同上 |
| ①装后端 | `ccm.status` | `probe_ccm_cli` | Remote | `ccm_probe.rs:250` | 同上 |
| ①装后端 | `ccm.uninstall` | `cc_integration_uninstall` | Local | `lib.rs:2252` | `R64`＋`K26` |
| ①装后端 | `ccm.uninstall` | `uninstall_remote_ccm_helper` | Remote | `sftp.rs:1075` | `R64`＋`K26` |
| ①装后端 | `daemon.deploy` | `deploy_remote_daemon` | Remote | `sftp.rs:743` | `K33`＋`K27` |
| ①装后端 | `daemon.deploy` | `uninstall_remote_daemon` | Remote | `sftp.rs:797` | `K33`＋`K27` |
| ②生成rc片段 | `acct-iso.shellinit` | `remote_acct_iso_shellinit` | Remote | `acct_iso_deploy.rs:126` | `K33` |
| ②生成rc片段 | `alias.account-commands` | `write_account_aliases` | Local | `lib.rs:1772` | `K33`＋`K34` |
| ③装MCP/skill | `cc-bus.deploy` | `deploy_local_cc_bus` | Local | `cc_bus_deploy.rs:387` | `K34` |
| ③装MCP/skill | `cc-bus.install-state` | `cc_bus_install_state` | Local | `cc_bus_deploy.rs:281` | `K34` |
| ③装MCP/skill | `mcp.remove` | `remove_project_mcp_server` | Local | `mcp.rs:395` | `K34` |
| ③装MCP/skill | `mcp.remove` | `remove_remote_mcp_server` | Remote | `mcp.rs:479` | `K34` |
| ③装MCP/skill | `mcp.write` | `write_project_mcp_server` | Local | `mcp.rs:371` | `K34` |
| ③装MCP/skill | `mcp.write` | `write_remote_mcp_server` | Remote | `mcp.rs:461` | `K34` |
| ③装MCP/skill | `skill.inbox` | `write_skill_file` | Local | `skill_host.rs:405` | `K34` |

**合计 22 条**：① **13** · ② **2** · ③ **7**。跨 **14 个能力 id** / **7 份后端文件**
（`acct_iso_deploy.rs` · `cc_bus_deploy.rs` · `ccm_probe.rs` · `lib.rs` · `mcp.rs` · `sftp.rs` · `skill_host.rs`）。
`skill.inbox` 的另外两条（`list_skills` / `read_skill_file`）**只读**，由 `CMD_OVERRIDE` 划出安装面。

### §C3 · 内部动作（非 Tauri 命令）—— 现打 **8 处**，不是 3 处

`K-R107 §F2` 的括注逐字是「`write_site_registry::WRITE_SITES` 里**带 tool id 的那几行**」。
**那个判别式现打给 7 行，而 `K-R107` 只点了 2 行**；第 3 行（`build.rs::embed_daemons`）
的 tool id 恰恰是 **`None`** ⇒ **判别式与点名互相矛盾**（这不是「少数了几行」，是那句括注不成立）。

| 归处 | 落点 | `WRITE_SITES` 的 tool id | 依据 |
|---|---|---|---|
| ①装后端 | `build.rs::embed_daemons` | **`None`**（`§0b` 点名，判别式接不住） | `K33` |
| ①装后端 | `local_backend.rs::install_local_ccm_entry` | `ccm` | `R64`＋`K26` |
| ②生成rc片段 | `profile_installer.rs::install_to_profile` | `ccm` | `K33` |
| ②生成rc片段 | `profile_installer.rs::uninstall_from_profile` | `ccm` | `K33` |
| ②生成rc片段 | `profile_installer.rs::atomic_write_string` | `ccm` | `K33` |
| ②生成rc片段 | `profile_installer.rs::atomic_replace_path` | `ccm` | `K33` |
| ③装MCP/skill | `cc_bus_deploy.rs::deploy_into` | `cc-bus` | `K34` |
| ③装MCP/skill | `mcp.rs::write_json_atomic` | `project-mcp` | `K34` |

**① 2 处 · ② 4 处 · ③ 2 处**。②的那 4 处正是件文件 `§0b` 里点名的
「`profile_installer.rs` 那四个写盘落点」——**它们本来就在 `WRITE_SITES` 里带着 tool id**，
只是 `K-R107` 数「3 处」时没把它们算进来。

### §C4 · 🔴 顺手逮到的一处：**两张源码权威表对同一个符号说法不一致，而没有判据在对拍**

| 落点 | `WRITE_SITES` 说 | `claims()` 说 |
|---|---|---|
| `profile_installer.rs::install_to_profile` | 是 **`ccm`** 的安装动作 | 是 **`posix-rc-aliases`** 与 **`powershell-profile`** 的 install 实现 |
| `profile_installer.rs::uninstall_from_profile` | 是 **`ccm`** 的安装动作 | 同上，uninstall 实现 |

而 `claims()` 给 `ccm` 登记的装 / 卸实现是 **`sftp.rs::install_remote_ccm_helper`**，
不是 `profile_installer.rs` 里的任何一处。
`write_site_registry` 那条对拍**只判「这个 id 在 `TOOLS` 里存在吗」**
（`table.contains(&format!("id: \"{id}\""))`），**不判「它与 `claims()` 说的是不是同一个工具」**
⇒ 这一格今天没有任何东西在数。**它直接影响 ①/② 的归属**：按 `WRITE_SITES` 读，这 4 处归 `ccm`（①）；
按 `claims()` 读，归两条 profile 系工具（②）。本件按 `§0b` 的目标形状归 **②**，
并把这处矛盾单列出来交 PM ——**不自己改表**。

### §C5 · 前端落点（**切件的写区按这个切**，`src/**.ts` 现打，排除 `*.vitest.ts` 与 `src/generated/`）

`src/ipc/commands.ts` 是**共用包装层**（22 条逐条都在它里面）⇒ 它是**唯一一处会被所有件撞到的写区**。
包装层之外 **10 份文件**：

| 前端文件 | 条数 | 哪几条 |
|---|---|---|
| `src/settings/cc_integration.ts` | 5 | `cc_integration_install/uninstall/preview/scan_path/status` |
| `src/launcher-diagnostics.ts` | 5 | `cc_integration_install/uninstall/scan_path` · `local_ccm_entry_status` · `write_account_aliases` |
| `src/settings/machine-card.ts` | 4 | `deploy_remote_daemon` · `uninstall_remote_daemon` · `install_remote_ccm_helper` · `uninstall_remote_ccm_helper` |
| `src/settings/mcp-section.ts` | 4 | `write/remove_project_mcp_server` · `write/remove_remote_mcp_server` |
| `src/accounts.ts` | 3 | `check/deploy_remote_acct_iso` · `remote_acct_iso_shellinit` |
| `src/settings/accounts-section.ts` | 3 | 同上 |
| `src/settings/cc-bus-section.ts` | 2 | `deploy_local_cc_bus` · `cc_bus_install_state` |
| `src/ccm-probe.ts` | 1 | `probe_ccm_cli` |
| `src/settings/panel.ts` | 1 | `cc_integration_status` |
| `src/views/inbox-view.ts` | 1 | `write_skill_file` |

---

## §D `cc-acct-iso` —— 落 ① 之后的代价（`KR117D2`）

### §D1 · 甲：**现在要做的那一半**，落地要动哪几处

**三条命令今天的形状（`acct_iso_deploy.rs`，441 行）**：三条**全是远端专属**
（都吃 `RemoteConfig`、都走 `connect_and_exec_cmd` / SFTP），`Side` 栏三条都是 `Remote`：

| 命令 | 住址 | 它做什么 | 并进 ① 要动什么 |
|---|---|---|---|
| `deploy_remote_acct_iso` | `acct_iso_deploy.rs:167` | SFTP 推 6 份内嵌文件 ＋ 远端跑 `cc-acct-iso-install.sh` | 与 `deploy_remote_daemon`（`sftp.rs:743`）**并成一处**；两者已共用 `sftp::deploy_decision` 的 skip-if-current 与 `is_safe_remote_managed_path`（后者逐字「2 个消费者」）⇒ 合并是**收口**，不是新写 |
| `check_remote_acct_iso` | `acct_iso_deploy.rs:94` | 远端 `command -v cc-acct-iso` 解 stdout | 与 `probe_ccm_cli`（`ccm_probe.rs:250`）同族（本模块头注逐字已经这么判过）⇒ 并进 ① 的「查装态」那一格 |
| `remote_acct_iso_shellinit` | `acct_iso_deploy.rs:126` | 远端跑 `cc-acct-iso shellinit`，**把输出原样吐回界面** | 🔴 **它不归 ①，归 ②** —— 它一个字节都不写盘，产出的就是一段待贴 rc 文本 |

**要动的处数（现打）**：后端 **3 份**（`acct_iso_deploy.rs` · `sftp.rs` · `ccm_probe.rs`）·
前端 **3 份**（`src/accounts.ts` · `src/settings/accounts-section.ts` ＋ 共用包装层 `src/ipc/commands.ts`）·
账本 **1 份**（`parity_ledger::LEDGER` 那三行的能力 id）。

**`cc-acct-iso-local` 那条欠口怎么补 —— 🔴 它不是「照抄远端那一半」**：

- `UNMANAGED_ENV` 里那条的 `named` 逐字是 **`~/.local/bin/cc-acct-iso`**，
  而远端装口（`cc-acct-iso-install.sh`，66 行）做的正是 `ln -sfn` 到 `$HOME/.local/bin`。
- 🔴 **本机照这个形状装，与 `K31`／`K34` 正面冲突**，而本仓已经为同一个问题裁过一次：
  `WRITE_SITES` 里 `install_local_ccm_entry` 那条的说法逐字写着
  「**落点刻意不是 `~/.local/bin/ccm`** —— 那是用户那份旧 `ccm` 住的地方……产品一个字节都不动它，
  只在自己的目录里放一份」。
  ⇒ **①的形状 = 落 `~/.cc-monitor/bin/cc-acct-iso`**（与 `install_local_ccm_entry` 同一套：
  `.partial` ＋ 置可执行位 ＋ `rename`），**不碰 `~/.local/bin`**。
- 代价：`UNMANAGED_ENV` 里那条的 `named` 与 `probe` 要跟着改
  （`EnvTier` 会从 `AppShipsNoInstallerYet` 自动变成 `AppInstalls`，那一步是派生的、不用手填），
  并且 `cc-acct-iso` 要多进一条 `ToolSpec` 的**本机载体**（`Carrier`）。

**补了之后与 ② 重不重叠 —— 🔴 重叠，而且今天就已经重叠了**（现打，两处同名 shell 函数）：

| 谁产的 | 产出逐字 | 住址 |
|---|---|---|
| ② `write_account_aliases` | `zcc() { ccm --account 'z' "$@"; }` | `account_aliases.rs`（渲染样例在它自己的判据里） |
| `cc-acct-iso shellinit` | `zcc() { CLAUDE_CONFIG_DIR=<dir> command claude "$@"; }` | `vendor/cc-acct-iso/scripts/cc-acct-iso::cmd_shellinit` |

**同一个函数名 `<账号名>cc`，两套不同实现**；谁后被 source 谁生效。
`cc-acct-iso-install.sh` 的第 5 条手动步骤自己逐字警告过这一形：
「如果你原来有『swap .credentials.json』式的旧切号方案……**两套并存会互相打架**」。
⇒ **①补本机装口之后，必然长出一条 ② 的活**：落点从 `~/.local/bin` 挪进 `~/.cc-monitor/bin` 之后，
用户 PATH 上够不着它 ⇒ 必须由 ② 生成那段 rc 片段（PATH 或 alias）。**①与②在这条上是串联，不是并列。**

### §D2 · 乙：**真吞那 11 个子命令要多少代价**（只出读数，不做）

**先订正人群**：PM 给的是 11 个子命令。源码现打的**分派臂是 13 个**
（`cc-acct-iso::main` 的 `case "$cmd"`，第 991–1006 行）：
那 11 条 ＋ `config` ＋ `help|-h|--help|""`；另外 `list|ls` 与 `which|who` 各带一个别名。

**代码体量现打**（`src-tauri/vendor/cc-acct-iso/`）：

| 文件 | 行数 |
|---|---|
| `scripts/cc-acct-iso`（主脚本） | **1009** |
| `scripts/lib.sh`（plan 引擎 / 锁 / 备份 / manifest） | **741** |
| `scripts/test/run-tests.sh` | **781** |
| `scripts/cc-acct-iso-install.sh` | **66** |
| `SKILL.md` · `VENDOR.md` | 115 · 31 |
| 前端 `src-tauri/src/acct_iso_deploy.rs` | **441** |

🔴 **PM 那句「1009 行 bash」少算了一半** —— 真要吞，`lib.sh` 的 **741 行**（plan / lock / backup / undo
那套引擎）是**跑不掉的那一半**：6 条写盘子命令全部经它。**1009 不是工作量，1750 也不是。**

**逐条：这一条吞进来要动什么**（✍=写盘 · ⚡=起进程 · 👁=只读）：

| 子命令 | 行数 | 性质 | 吞进后端要动的那张表 |
|---|---|---|---|
| `init` | 70 | ✍ 经 `plan_commit` | 只读白名单（`readonly_guard::WRITE_WHITELIST_MODULES`，今天 **2** 条） |
| `add` | 74 | ✍ 经 `plan_commit` ＋ `lock_acquire` | 同上 ＋ 见下「锁」 |
| `rm` | 39 | ✍ 经 `plan_commit` ＋ `lock_acquire` | 同上 |
| `sync` | 88 | ✍ 经 `plan_commit` ＋ `lock_acquire` | 同上 |
| `isolate` | 36 | ✍ 经 `plan_commit` ＋ `lock_acquire` | 同上 |
| `rollback` | 89 | ✍ **直接** `mkdir`/`cp`/`mv`/`rm` | 同上，**而且它是本组里唯一不经 plan 引擎的** |
| `verify` | 319 | ✍ **一处活体探测文件**（`: >"$c1/$pdir/$pname"` 写完立刻 `rm -f`） | 同上 —— 🔴 **PM 把它读成只读了** |
| `run` | 29 | ⚡ `exec env CLAUDE_CONFIG_DIR=… <launcher>` | **起进程白名单** `readonly_guard::spawn_registry::ALLOWED`（今天 **13** 条），**不是**写白名单 |
| `list` / `ls` | 55 | 👁 只 `printf` | 无 |
| `which` / `who` | 45 | 👁 只 `printf` | 无 |
| `shellinit` | 30 | 👁 只 `printf` | 无 |
| （`config`） | —— | 👁 `cfg_dump` | 无 |
| （`help`） | 36 | 👁 `usage` | 无 |

**⇒ 写盘的是 7 条，不是 PM 读出来的 3 条**（`isolate`/`sync`/`rollback` 之外还有
`init`/`add`/`rm`，外加 `verify` 那一处活体探测）。**只读的是 3 条**（＋`config`/`help` = 5）。

🔴 **而「再动一次只读白名单」这句话把代价说小了一格**。白名单**不是**「进了名单就能随便写」：

- 白名单模块里仍被**禁**的写法逐条住 `readonly_guard::WHITELIST_STILL_FORBIDDEN`（现打 **14** 条），
  含 `fs::write` · `fs::rename` · `fs::copy` · `fs::remove_file` · `fs::remove_dir` · `fs::soft_link` ……
- 配对判据 `open_calls_are_all_exclusive`：`.open(` 出现几次，`.create_new(true)` 就必须出现几次。
- ⇒ 白名单买到的只有 **「用 `O_EXCL` 新建一份此前不存在的文件」**。
- 而 `cc-acct-iso` 的写侧**整份都是「改动既有数据」**：`manifest` 整份重写 · 软链 relink ·
  `mv` 到备份目录 · `rm` 旧链 · `chmod 700`（`set_permissions` 这个动词
  `platform/landing.rs` 的头注逐字写着「在只读白名单里**根本不存在**」）。
- **⇒ 真吞写侧不是「加第 3 个白名单模块」，是要重新裁一次 daemon 的只读铁律本体**
  （`D1` 收窄后那条：「daemon 进程自身不许改动用户既有数据」）。这是一次**定框级**的裁决，
  不是一次接线。

**哪几条会撞零定时器 —— 现打答案是 0 条，而 PM 问错了那张表**：
`no_timer_guard::periodic_wake_patterns()` 禁的是 `thread::sleep` / `time::sleep` / `recv_timeout` /
`time::interval` / `Instant::now` / **`Duration::from_secs`**。脚本里**没有任何轮询循环**
（现打：`cc-acct-iso` ＋ `lib.sh` 两份里 `sleep` 零命中）。
⚠ **但有一处会撞，而它不是「定时器」**：`lib.sh:555` 的 `flock -w 30`。
落进 Rust 就是一处 `Duration::from_secs(30)`（或 `from_millis(30_000)`）⇒ 逐字落在禁用表上
⇒ 要进 `no_timer_guard::REGISTERED_DURATION_USES`（今天 **4** 条）并写明
「它是一次阻塞的上限，不是节拍」＋解锁条件。**先例现成**：`server.rs` 那条
`Duration::from_millis(30_000)` 就是同一形（`SO_RCVTIMEO`）。

**便宜的那几条到底有多便宜 —— 现打它们已经有一半在后端了**：

- 后端已经**依赖** `acct-core`（`remote-daemon-proto/Cargo.toml:60` 的 path 依赖），
  账号库契约四个常量（`ACCTS_DIR_NAME` / `MANIFEST_NAME` / `CREDENTIALS_NAME` / `SUPPORTED_SCHEMA`）
  **只有一个住址**（`src-tauri/crates/acct-core/src/lib.rs`，447 行）。
- 后端 `observe/accounts_query.rs`（2616 行）**已经实现了 `list` / `which` 那一族的读侧**：
  `resolve_accts_dir` ＋ `load_manifest` ＋ 鉴权态判定，并有 `--list-accounts` 这条 CLI。
- ⇒ **`list` / `which` 这两条的「吞」其实是「改调已有的后端读口」，不是新写。**

🔴 **顺带订正 PM 那个「后端今天已经读 `~/.claude-accts` 的有 16 处 / 4 份文件」**：
16 是裸 grep 的行数，逐行分档（分母 = 那 16 行）——

| 档 | 条数 | 住址 |
|---|---|---|
| **生产段字面量** | **1** | `control/ccm/argv.rs:102` `ACCTS_MANIFEST_REL = ".claude-accts/accounts.json"` |
| 文档注释 | **1** | `observe/accounts_query.rs:254`（docstring） |
| 测试段 | **14** | `accounts_query.rs` 12 处 · `agent_locality_guard.rs:203` · `agents/fake/mod.rs:917` |

**而真正的生产读路径那把尺子根本没扫到**：`accounts_query.rs` 用的是 `acct_core::ACCTS_DIR_NAME`
（住 `src-tauri/crates/acct-core/src/lib.rs:40`，**在 `remote-daemon-proto/src/` 之外**）。
按「用 `acct_core::` 的生产段行」重打：daemon **3 处**、monitor **15 处**。
**「16 处 / 4 份文件」不是工作量，「3 处」也不是** —— 前者是 grep 的行数（其中 14 行在测试段），
后者是 import 的行数。

---

## §E `Side` 栏复量（`KR117D4`）—— **只量代价，一行都没翻**

### §E1 · `K-R115` 那三个数：**三个全部复现**

| `K-R115` 交回的 | 我现打 | 怎么量的 |
|---|---|---|
| 55 行的直方图 `RemoteOnly 39 · FramePlane 6 · Mixed 0 · Unclassified 10` | **一致** | ① `LEDGER` 里 `Side::Remote` = **55** 行（分母 148）· ② `REMOTE_SIDE_SIGNOFF` 四档合计 = 55 · ③ 独立复刻 `command_dispatch_class()` 现打 **39/6/0/10**，逐档相等 |
| 说假话的 **5** 行 | **一致** | `FRAME_PLANE_VERDICTS` **6** 条，`LiesTodayOwedACorrection` **5** 条：`capture_remote_pane` · `kill_remote_tmux` · `tmux_send_keys` · `cc_bus_broadcast` · `cc_bus_kill` |
| 连锁 **4** 处 | **4 处全在** | `EXPECTED_LOCAL_OR_BOTH = 93`（`parity_ledger.rs:816`）· `ORIGIN_TAKING_BOTH`（现打 **9** 条）· `tmux.manage` 那条 `ASYMMETRY_REASONS`（`:471`）· `the_tmux_manage_row_stops_waiting_for_a_daemon_primitive`（`:962`） |

⚠ **复刻器的可信度怎么来的**：我在 Python 里重写了一份 `command_dispatch_class()` 的近似
（`production_code` 的剥法是**近似**的，不做词法掩码）——**它不进量具**（`ruler.py` 的诚实边界 B3），
只用来点名那 10 行。**合格判据**是：四档直方图逐档等于 39/6/0/10 **且** `FramePlane` 那 6 个名字
逐字等于人裁表的键集。**两条都过了**，所以下面那 10 个名字可信；
量具住址 `/tmp/…/scratchpad/kr117/side-derive-probe.py`（**临时件，不随树走**，
要复现就照本节这段说明重写 —— 别照那个住址找）。

### §E2 · 🔴 翻这 5 行要同拍改哪几处（逐条）

| # | 要改的 | 住址 | 翻 5 行之后它变成什么 |
|---|---|---|---|
| 1 | `EXPECTED_LOCAL_OR_BOTH` | `parity_ledger.rs:816` | `93 → 98`（那条判据数的是「声明 `Local`/`Both` 的命令条数」） |
| 2 | `ORIGIN_TAKING_BOTH` | `parity_ledger.rs:673`（现打 9 条） | 5 条里**吃 `origin:` 参数的**要逐条登记并写理由。⚠ 这一处**不是无脑 +5**：登记的触发条件是「签名里出现 `origin:`」，要逐条看签名，别照抄「+5」 |
| 3 | `tmux.manage` 那条 `ASYMMETRY_REASONS` 散文 | `parity_ledger.rs:471` | 它逐字写着 ②`kill_remote_tmux` ⇒ 本机已通、③`tmux_send_keys` ⇒ 本机已通 —— 翻了 `Side` 之后这段要改成「已结」 |
| 4 | `the_tmux_manage_row_stops_waiting_for_a_daemon_primitive` | `parity_ledger.rs:962` | 它**逐字断言那句理由里必须含 `Side::Remote`** ⇒ 翻 `Side` 要**同拍改掉一条现行判据的断言** |
| 5 | `FRAME_PLANE_VERDICTS` ＋ `REMOTE_SIDE_SIGNOFF` | `parity_ledger.rs`（两张表） | 人裁表少 5 条、签字表 `FramePlane 6→1` 且合计 `55→50`。⚠ `K-R115` 的「4 处」**没把这两张表算进去**（它们是那条判据自己的表，改 `Side` 必然连带） |

🔴 **另外两处，`K-R115` 没点、而它们今天就已经在说假话**（现打，不是「翻了才会假」）：

| 处 | 逐字 | 现打 |
|---|---|---|
| `parity_ledger.rs:454` `cc-bus.cockpit` 那条 `ASYMMETRY_REASONS` | 「`cc_bus_spawn` / `cc_bus_kill` 对 `<local>` 走 `refuse_local_write`」 | **`refuse_local_write(&origin,` 的生产段调用点现打只有 1 处**：`cc_bus.rs:1657`（`cc_bus_spawn`）。另 3 处在 `#[cfg(test)]` 段里 |
| `tool_registry.rs:574` | 「写面（`cc_bus_send`/`_spawn`/`_broadcast`/`_kill`）至今**只动远端**（`refuse_local_write`）」 | 同上 —— 而且 `cc_bus_send` 那一行 `K-R98`（09-13）已经改成 `Side::Both` 了 |

⇒ **同一句假话现打住在 3 个地方**（`LEDGER` 的 `Side` 栏 · 两处散文），
而 `K-R115` 的「连锁 4 处」只覆盖了 tmux 那一族的散文。**翻 5 行的真代价是 7 处，不是 4 处。**

### §E3 · 两问

**① 那 10 行 `Unclassified` 今天到底是什么 —— 「够不着的是哪一层」**

逐条点名（现打，复刻器与 Rust 直方图对拍过）：

| 命令 | 住址 | 够不着的是哪一层 |
|---|---|---|
| `render_ccm_launch` | `backend/control/launch_wire.rs:134` | **没有那一跳**：纯渲染器，`fn(req) -> resp`，不做任何 IO |
| `render_launch_payload` | `backend/control/launch_wire.rs:248` | 同上 |
| `list_ssh_host_aliases` | `ssh_source.rs:5747` | **本机文件读**：解 `~/.ssh/config`，从不连远端 |
| `resolve_ssh_host` | `ssh_source.rs:5833` | 同上 |
| `import_ssh_hosts` | `ssh_source.rs:6009` | 同上 |
| `list_forwards` | `port_forward.rs:172` | **进程内状态**：读 app 自己的转发表 |
| `stop_forward` | `port_forward.rs:152` | 同上，按 id 停 |
| `sftp_cancel_transfer` | `sftp_pool.rs:315` | 同上，按 `transfer_id` 取消 |
| `list_remote_mcp_origins` | `mcp.rs:247` | **读本机配置**：`load_remote_configs()` 列 origin 标签 |
| `bring_remote_terminal_to_front` | `lib.rs:2074` | **纯 Win32 本机动作**：`RemoteHwndCache` 是纯内存缓存 |

⇒ **10 行分成两族，而两族的含义完全不同**：
**9 条是「没有派发跳可派生」**（纯函数 / 读本机文件 / 进程内状态 / Win32）——
对它们，`Unclassified` 说的是**这把尺子的形状不适用**，不是「有一跳而看不见」；
**1 条（`bring_remote_terminal_to_front`）名字里带 `remote` 而一步远端 IO 都不做** ——
它的 `Side::Remote` 说的是「**给远端会话用的**」，不是「**去那台远端机器**」。
🔴 **⇒ `K-R115` 那句「`Unclassified` 不是『安全』，是『够不着』」要再收窄一格**：
够不着的**不是某一层代码**，是**这 10 条里 9 条根本不在「派发跳」这个维度上**。
这不推翻 `K-R115`（它没说是哪一层），但它把「10 行悬着」这个印象改小：
**真正悬着的是 1 条**（`Side::Remote` 这一格在它身上到底在说什么），另 9 条该做的是
**把它们从这把尺子的人群里划出去**（而那要动 `REMOTE_SIDE_SIGNOFF`，属第二拍）。

**② 「本机真的抓得到一屏」这类判不了的，要用什么才判得了 —— 缺的是判据，不是真机**

三者逐条排除，都给住址：

- **不是缺产品判断**：`C1`／`K15` 早就裁了「本地 = 不走 ssh 的远端」，`K36` 裁了「两份后端逐字相同」。
  「本机该不该抓得到一屏」这一问**已经有答案**。
- **不是缺真机**：后端侧原语**已经在了** —— `remote-daemon-proto/src/control/capture_pane.rs`
  （`tmux -u capture-pane -p -t '=名:'`，argv 直传不过 shell，只读；起进程登记在
  `readonly_guard::spawn_registry::ALLOWED`），帧面也在了（`ch:capture-pane`，`K-R104` 09-13），
  monitor 侧 `inbound_client::capture_pane_args` 也在了。沙箱里有 tmux
  （`e2e/tmux-target-acceptance.sh` · `e2e/p3t-local-tmux.sh` · `e2e/gen-idle-tmux.sh` 都在用）。
- **⇒ 缺的是判据**，形状很具体：**一趟「起本机后端 ＋ 建一个 tmux 会话 ＋ 经 `<local>` 走
  `ch:capture-pane` 拿回非空一屏」的 e2e**。先例现成：`e2e/local-backend-supervise.sh`
  （起本机后端）＋ `e2e/tmux-target-acceptance.sh`（造 tmux 会话）。
- ⚠ **一条真的拦路石，要一起报**：这一族判据挂在 `#[cfg(embedded_daemons)]` 上，
  而门禁那格的分母行现打逐字说「**本树未铺 `src-tauri/embedded-daemons/` ⇒ 少了『本地后端真的能
  起来吗』那一族（4 条）**」。⇒ 新写的那条 e2e 如果也挂在这个 cfg 上，**在没铺的树上它恒不跑**
  —— 那是一条「写了也不响的闹钟」。**这一格归第二拍立件时先裁。**

---

## §F 死值验（`KR117D1` 的三刀 ＋ 我自己补的两刀）

副本：`/tmp/…/scratchpad/kr117/dv/{base,a,c}/src-tauri/src`，各自**一份新副本**，
拷自本树 `703b4d0`，**副本里没有 `.git`**（纪律 12c）。逐刀先断言锚点命中数再改，并打印「变异已落地」。

| 行 | 刀 | 锚点（切在哪 · 命中几次） | 退出码 | 现打输出 |
|---|---|---|---|---|
| M0 | 基线（原树副本 ＋ 原尺子） | —— | 0 | `RULER: OK`；22 条命令（①13 ②2 ③7）/ 14 个能力 id / 7 份文件；8 处内部动作 |
| M1 | **刀①** 往 `LEDGER` 加一条装的命令而不归档 | `("deploy_remote_daemon", "daemon.deploy", Side::Remote),` **1 次**；`acct_iso_deploy.rs` 的 `#[tauri::command]` **3 次**（新命令追加在**文件末尾**，不夺走任何既有属性）| **1** | **`RED [R3a] … widget.deploy`，且只有这一条** |
| M2 | **刀②** 把 `daemon.deploy` 的归属从 ① 改 ③（只动尺子副本，树不动） | `("daemon.deploy", (B1, "K33+K27"` **1 次** | 0 | 表**跟着变**：①13→**11** · ③7→**9**（总数仍 22）⇒ 它数的是**归属**，不是「有没有这条命令」 |
| M3 | **刀③ 阴性对照** 把归档判定整段摘掉（`DV-ARCHIVE-GATES` 两块共 32 行 / 6 处 `red()`）＋ 刀① | 同 M1 | **0** | **`RULER: OK` —— 一条都不红**。⇒ M1 那一红确实出自归档那几条判定 |
| M4 | 刀④（我补）把一条**盘上确凿的装口**归成「非装面」 | `("daemon.deploy", (B1, "K33+K27"` **1 次** | 1 | `RED [R4] 能力 daemon.deploy 有一条盘上确凿的装 / 卸实现，却被归成 非装面` |
| M5 | 刀⑤（我补）写盘落点归档表少一条 | `SITE_ARCHIVE` 里 `mcp.rs::write_json_atomic` 那一条 **1 次** | 1 | `RED [R5a] … mcp.rs::write_json_atomic` |

**「把实现整个退掉还剩多少绿」**：本件没有生产实现，能退的只有量具自己。
M3 就是那一趟 —— 摘掉归档那两块（6 处 `red()`）之后，**刀①当场从 1 红变 0 红**
⇒ 这把尺子的牙**全部**长在 R3/R4/R5 上；S0 地板、R2（19 vs 7）、R6（住址）、R7（连锁）
这四条在 M1/M3 两行里都没响 —— 它们守的是别的性质，**不许拿它们冒充安装面的牙**。

**CRASH：0 次**（六条地板 —— `TOOLS≥5` · `UNMANAGED_ENV≥5` · `LEDGER≥100` · `WRITE_SITES≥20` ·
`claims()≥5` · `#[tauri::command]≥100` —— 六趟全过）。

---

## §G 门禁读数（不是本件的验收条件，只留读数）

沙箱 `.claude/devbox/gate`（镜像 `ccmon-devbox:latest`），工作树 `k-r117`，`PB_WS=backend-consolidation`：

```
GATE: OK —— 16 格全绿（hooks · copy2 · fmt · fmt-daemon · winchk · cargo · deadcode ·
generated · daemon · tsc · npm · 四套 ccm e2e · pb check），可以出货
```

逐格读数（逐字抄自那一趟的输出）：`hooks 11` · `copy2 11` · `fmt 1` · `fmt-daemon 1` ·
`winchk 1` · **`cargo 1613`（9 个包合计）** · `generated 与 Rust 源一致` · `deadcode 41` ·
`daemon 762` · `tsc 366` · `npm 1726` · `ccm-print-parity PASS=12` · `ccm-rbind-title PASS=8` ·
`ccm-cli PASS=46` · `ccm-contract-parity PASS=45` ·
`pb check [backend-consolidation] FAIL=0 BROKEN=0`。

⚠ **`cargo 1613` 这个数带一条分母警示**（门禁自己印的）：
「本树**未铺** `src-tauri/embedded-daemons/` ⇒ `embedded_daemons` cfg 不置 ⇒ 上面那个合计里
少了『本地后端真的能起来吗』那一族（4 条）」。
⇒ **`cargo` 那一格绿，`the_remote_side_column_is_signed_off`（§E1 那三个数的 Rust 侧）
在那 1613 里跑过并且过了**；而 §E3② 点名的那条缺口正好落在被 cfg 摘掉的那 4 条旁边。
