# 起会话入口全量清单（unify-launch 的输入与验收矩阵）

2026-07-27 逐条回盘核实（含文件行号）。§0 成功标准 1「每个入口行为一致」以本表为验收面。

---

## A. 远端 — 全部经 `launch_remote_terminal`（`launch.rs:215`）→ wt.exe/PowerShell → `ssh -t <host> "bash -lic '<remoteCmd>'"`

| # | 执行器 | 载荷构造器 | 起 tmux | 能带账号 | 会话名 | @ccm_sid | UI 入口 |
|---|---|---|---|---|---|---|---|
| 1 | `runRemoteResume` `remote-launch-run.ts:37` | `buildResumeDirectCmd` | 否（除非 launcher 自建） | 是 | — | — | tab 右键「Resume（直连）」`tabs.ts:1999`、「切到账号 X（resume）」`tabs.ts:2221`；历史右键 resume `history.ts:1499` |
| 2 | `runRemoteResumeTmux` `:84` | `buildResumeTmuxCmd` | 是（新建） | 是 | `cc-<sid8>[-N]` | 建时 set | tab「Resume（tmux）」分支② `tabs.ts:2089`；换号重启第⑤步 `account-restart.ts:164` |
| 3 | `runRemoteResumeIntoExistingTmux` `:125` | `buildResumeIntoExistingTmuxCmd` | 否（复用空壳） | 是 | 沿用 | 沿用 | tab「Resume（tmux）」分支①.5 `tabs.ts:2068` |
| 4 | `runRemoteLauncher` `:186` | `buildLauncherCmd` | 是（新建） | 是 | `cc-<basename>` | **从不设** | 设置→开新 Claude `remote-section.ts:862`；历史右键「在该目录起新会话」经 `runNewSessionRemote` `history.ts:1536` |
| 5 | `runRemoteAttach` `:217` | `buildAttachCmd` | 否（接回） | 否（焊死） | 已有名 | — | `tabs.ts:2054 / 2135 / 2159 / 2869 / 2892` |
| 6 | SFTP「在此打开终端」`sftp/panel.ts:545` | `buildOpenTerminalCmd` | 否 | 否 | — | — | SFTP 面板 |
| 7 | `launchStep` `accounts-section.ts:154` | `buildAcctIsoCmd`（7 种 step） | 否 | **自带** | — | — | 设置→账号：init-preview / init-apply / verify / shellinit / sync-apply / add-apply / **login** |

`#7` 的 `login` = `cc-acct-iso run <名>`（`acct-deploy.ts:95`）。

## B. 本地 — 全部经 `launch_powershell_window`（`launch.rs:149`）

| # | Rust command | 构造器 | 能带账号 | UI 入口 |
|---|---|---|---|---|
| 8 | `resume_history_session` `history.rs:883` | `build_resume_ps_command` | **无账号维度** | tab resume（本地分支）`tabs.ts:2020`；历史 resume `history.ts:1515`；分支 resume `session-viewer.ts:355` |
| 9 | `new_local_session` `history.rs:981` | `build_new_session_ps_command` | **无账号维度** | 历史右键起新会话（本地）`history.ts:1542` |

本地这两条另有独立的「F34 自定义命令 → `cc` 别名探测 → 默认 launcher」三段逻辑，与远端零共享。

## C. 终端侧（`~/.bashrc`，cc-monitor 管不着）

| # | 命令 | 起 tmux | 账号模型 | 会话名 | 改 cwd |
|---|---|---|---|---|---|
| 10 | `cc` `:144` | 否 | `_cc_acct_last` 拷凭据 | — | **是**（`_cc_resolve_target`） |
| 11 | `ccm` `:147` | 否 | 无 | — | 否 |
| 12 | `cct` `:150` | **是**，再 send-keys `ccm` | `_cc_acct_last` 拷凭据 | `<dir>_cc[-N]` | **是** |
| 13 | `zcc`/`zcct`/`bcc`/`bcct` `:272` 动态生成 | 同 10/12 | `_cc_acct <名>` 拷凭据 | 同上 | 同上 |
| 14 | `cc-acct-iso run <名>` | 否 | **CLAUDE_CONFIG_DIR 隔离** | — | 否 |
| 15 | `oo`/`oom`/`oot` `:215-235` | oot 是 | 无 | `<dir>_oo[-N]` | 是 |

---

## D. 按 UI 位置（用户视角）

**1. tab 右键（活跃）**：Attach（tmux）· 把此会话切到账号 X（重启）· 把此会话切到账号 X（先压缩上下文再重启）
**2. tab 右键（灰）**：Resume（直连）· Resume（tmux）· 用基座 resume（直连，不隔离）· 用基座 resume（tmux，不隔离）· 把此会话切到账号 X（resume）
**3. tab 上的 ⇄**：用当前账号重启对齐
**4. 历史页**：行内 ↺「在新终端 resume」· 右键「在新终端 resume」/「在该目录起新会话」/「用账号 X resume」· 搜索卡片同套
**5. 会话查看器**：在新终端 resume 此分支
**6. 设置→远端机器**：开新 Claude（cwd / tmux 名 / 启动命令 / 账号下拉）
**7. 设置→账号**：预览迁移 · 执行迁移 · 自检 · 装 shell 集成 · 同步 · 加号 · **每账号「登录」**
**8. SFTP**：在此打开终端
**9. Ctrl+K**：账号：把当前会话对齐到当前账号 · 账号：对齐全部不一致的会话
**10. 状态栏 chip**：只改「当前账号」设定，不起会话

---

## E. 三个用户意图 vs 现状（F09 的收敛目标）

**A「把这个会话再跑起来」——10 个入口，4 种后端行为。**
「直连 / tmux / 基座 / 账号」是四条**正交**维度，今天被摊平成并列菜单项做排列组合，于是同一级菜单里同时出现「Resume（tmux）」和「用基座 resume（tmux，不隔离）」。

**B「起一个新会话」——3 个入口，2 种行为。**
历史那条起 tmux 但不让选账号（跟随）；设置那条起 tmux 且可选账号。

**C「换个账号跑」——9 个入口，5 种机制，只有 2 个真的生效。**

| 入口 | 机制 | 现状 |
|---|---|---|
| 设置→账号→登录 | `cc-acct-iso run` 同 shell 内设 env 再 exec | **有效** |
| 设置→开新 Claude→账号下拉 | export 写进 send-keys 载荷**内** | **有效** |
| tab 右键「切到账号 X（重启）」 | 需精确 `@ccm_sid` + `cc-*` 名 | 基本全废 |
| tab 右键「（先压缩再重启）」 | 同上 | 同上 |
| tab ⇄ | 同上 | 同上 |
| Ctrl+K「对齐当前会话」 | 同上 | 同上 |
| Ctrl+K「对齐全部」 | 同上 × N | 同上 |
| tab 右键「切到账号 X（resume）」 | export 在 `cct` **外层** | **被 tmux 边界吃掉** |
| 历史右键「用账号 X resume」 | 同上 | 同上 |
| 状态栏 chip | 只改设定 | 名字像切号、其实不切 |

九个地方让你选账号，两个管用，而且都在设置里——最不像「我要换号跑这个会话」的地方。

---

## F. 失效根因索引（每条指向负责修的功能）

| 根因 | 证据 | 修它的功能 |
|---|---|---|
| `export` 落在 tmux 进程边界外被吃掉（`cct`） | `update-environment` 默认列表不含 `CLAUDE_CONFIG_DIR`（实测） | **F02** |
| cc-monitor「开新 Claude」起的会话**永不带 @ccm_sid** | `buildLauncherCmd` 调 `createRunAttach` 未传 `ccmSid`（`remote-launch.ts:236-241`，对比 `:154` 传了）；载荷是裸 `claude`、不经 `__ccm_rbind` | **F04** |
| 终端 `cct` 会话名 `<dir>_cc` 被 Rust 白名单拒 | `is_ccm_tmux_name` 只认 `cc-` 前缀（`tmux.rs:247`） | **F04** |
| `-t <name>` 是前缀/glob 匹配，不是精确匹配 | 实测 `kill-session -t sib` 杀掉 `sib-2` rc=0；`-t 'si*'` glob 命中 | **F01** |
| 通道A 的 `@ccm_sid` 焊死建时 sid，`/branch` 后漂移 | 审计 D6 | **F04** |
| 本地路径完全没有账号维度 | `history.rs:883/981` | **F06** |
| 已证伪：wrapper 读死 `~/.claude/sessions/` | `~/.claude-accts/*/sessions` symlink 回 `~/.claude`，同一 inode | 无需修 |
