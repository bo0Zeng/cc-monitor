# 全局不变量

跨模块约束清单。修改代码时**违反任一条都是 bug**，code review 时应该被指出。

每条都给出"理由"——为什么这条不能松动。

> ## ⚠ 读之前：两处会误导你的旧称
>
> 1. 🔴 **`shared/ccm` 这个文件已经不存在了** —— 那 1592 行 bash 启动器删于 `e8f9e08e`
>    （`K-R48` 第二拍④，同拍清零了 19 处 `include_str!`）。今天这条路走的是**后端的 `ccm`**
>    （Rust，`src/backend/control/ccm/`），别名文本在 `src/shared/ccm-aliases.sh`。
>    ⇒ **下文凡是提到 `shared/ccm` 的地方都是历史**，留着是为了解释「今天为什么长这样」，
>    不是现状。看到它请读成「当年那个 bash 启动器」。
> 2. **目录名**：2026-09-17 重组之后顶层**只有 `src/` 与 `tests/`**。
>    `src-tauri/` → `src/bridge/` · `remote-daemon-proto/` → `src/backend/` ·
>    `doc/` → `src/doc/` · `e2e/`·`evidence/`·`scripts/`·`hooks/` → `tests/` 下。
>    下文若出现旧名，同样按历史读。

---

## 1. monitor 零侵入 Claude Code 数据源

`monitor` 对 `<claude_dir>/projects/**/*.jsonl` 和 `<claude_dir>/sessions/<PID>.json` **只读**。

**例外（穷举，各自路径白名单防越界）**：
1. **删历史会话（本机 ＋ 远端，同一条路）**：`history::delete_history_session`（吃 `origin`）—— 用户**显式**点删除、二次确认后，
   交给**那台机器的后端**一条明确的命令 `files-delete-session`：**只收 sid**，落点由后端按 sid 在它自己的记录树里找
   （`agents::claudecode::paths::session_file_for_delete`：解到底必须恰是 `projects/<项目>/<sid>.jsonl`，链接出界不跟），
   删之前再过一次它自己的围栏（`files_write::fenced_session_file`）。它是后端文件管理写面里**会话文件围栏唯一的例外**（§41.6 第三层）。
   〔FN1 · 第四波 4C · 2026-09-25 · 用户 V119「文件管理器全部都可以改. 不需要任何围栏」〕文件管理写面的会话文件围栏拿掉了，
   「唯一的例外」这个说法随之作废；这一条本身（只收 sid、自己那一道、二次确认）一个字没动。
   monitor 这一侧只剩一道一致性闸：界面给的 sid 必须恰是那一行文件名的 stem，对不上一个请求都不发。
   〔RW1 · 第四波 2026-09-24：用户裁「只允许后端的文件管理部分写文件」也管本机 ⇒ 从前本机 `fs::remove_file` ＋ `validate_delete_target`〔散文墓碑〕
   与远端 SFTP 直删两条路合成这一条。〕
2. **远端后端自部署（issue #29，`sftp::ensure_backend_deployed`）**：经 SFTP 写远端 `~/.cc-monitor/bin/`（后端二进制 + `.build_id` 标记）—— **非用户数据、幂等、版本门控**；只写 cc-monitor 自己的 bin 目录，**绝不碰** `~/.claude/`。
3. **远端历史删除（issue F11）**：〔RW1 · 第四波 2026-09-24〕**已并进第 1 条**（按用户裁「按推荐改」，远端也经那台机器后端的 `files-delete-session`，只收 sid）。
   原文留档：**远端历史删除（issue F11，`history::delete_history_session` 带远端 `origin` → 远端分支 `remote_history::delete_remote_history_session` → `sftp::remove_remote_file`；〔步 12·C 09-20〕本机与远端已合成一条命令）**：用户**主动**点删除 + 前端**二次确认**后，经 SFTP 移除远端 `~/.claude/projects/` 下的 jsonl；**双重路径守卫**（`is_safe_remote_jsonl`〔散文墓碑〕：须 `.jsonl` + 含 `/projects/` + 无 `..`；并 SFTP `canonicalize` 解 symlink 后再校验）。〔RW1 · 第四波 2026-09-24：本条的实现已换 —— 两侧都经那台机器后端的 `files-delete-session`（**只收 sid**，会话文件围栏唯一的例外，落点由后端按 sid 找、解到底必须恰是 `<项目>/<sid>.jsonl`）；SFTP 直删与这道守卫一起走了。〕**注**：标星 / 重命名 / 隐藏是 **monitor 本地元数据**（`history-metadata.json` 按 sid），**不写远端**——唯一写远端的用户数据操作就是删除 jsonl。
4. **profile 写（F10，本机 `profile_installer` cc 集成 ＋ 别名文件 `~/.cc-monitor/aliases.sh`（〔TL1 · 4C〕接上它的那一行 source 只住别名块里，`设计/71 §6.1`；从前另有一处代装进 rc 的，退役）＋ 远端 `sftp::install_remote_alias_block`/`uninstall_remote_alias_block`，〔MC1 2026-09-24〕从前叫「装/卸 ccm 助手」）**：写用户自己的 shell profile（本机 `~/.bashrc` / `$PROFILE` / **远端 `~/.bashrc`**）装/卸 cc(m) 助手——用户显式触发、BEGIN/END 块 + 备份 + 写后校验回滚。
   〔RW1 · 第四波 2026-09-24〕**落盘不在 monitor 进程**：本机与远端都经那台机器的后端（`user_files::edit` → `files-peek` / `files-put`），
   monitor 只算新内容（`fenced_block::splice_in/out`）；备份 · 替换 · 回读 · 回滚那一份规则住后端（见 §4）。**注（batch20 审计修）**：F10 **含远端 `~/.bashrc` 写**（原 `sftp.rs` 头「非远端」措辞已订正）。
5. **MCP 项目配置写（F87 本机 / F89a 远端；〔RW1 · 第四波 2026-09-24〕**两侧落盘都经那台机器的后端**（`mcp::edit_project_mcp` → `files-peek` / `files-put`），下文「本机 `ReplaceFileW` / 远端 `sftp::upload_atomic`」两处是原文留档；写面仍只 `<dir>/.mcp.json`、坏 JSON 拒覆盖、不再留 `.json.bak`；`mcp::write_project_mcp_server` / `mcp::remove_project_mcp_server` —— 〔步 12·C 收尾 09-20〕**两侧各已合成一条吃 `origin` 的命令**，远端那半 `write_remote_mcp_server` / `remove_remote_mcp_server` 今天是它们的**分支函数**，不再是 IPC）**：用户**显式**点「添加/更新」或「删除」（删带二次确认）后，写**项目** `.mcp.json`（本机 `mcp_json_path` 硬编码 / 远端 `is_safe_remote_mcp_json` 守卫：绝对 + 尾 `/.mcp.json` + 无 `..` + 非裸；远端经 `sftp::upload_atomic` 原子 tmp+备份+rename）。**SS-14**：写面**只** `.mcp.json`，**绝不**写 `~/.claude.json`/settings.json。`.mcp.json` 是**用户项目配置**（决定项目用哪些 MCP server），**非** Claude 会话数据（jsonl/pidfile）——与本约正交（同 F47 SFTP 面板性质），非驱动运行中会话。
6. **公钥推送（F50，`pubkey::push_public_key`）**：用户显式点推送后，经 SSH-exec 把本地公钥**追加**到远端 `~/.ssh/authorized_keys`（`build_authorized_keys_cmd`：key 经 `shell_quote` 防注入、`grep -qxF` 幂等去重、只 append 不删）——用户自己的免密配置、非 Claude 数据。
   〔RW1 · 第四波 2026-09-24 · 主会话裁〕它写的是**用户文件**，但它是「建立 SSH 信任」那一跳，**按构造发生在那台机器的后端可达之前**
   ⇒ 与 F08（部署后端）同档，**留在 SSH**，不走后端写面。这是「用户文件只经后端写」唯一登记在案的 monitor 侧远端例外。

**〔RW1 · 第四波 2026-09-24〕上面这几条今天的实现形状（用户裁「只允许后端的文件管理部分写文件」**只管用户的文件**、**也管本机**）**：
monitor 进程**一个字节都不直接写用户文件**。rc / `$PROFILE` / 别名文件 / 项目 `.mcp.json` / skill 收件箱 / `<claude_dir>/skills/cc-bus/` /
删历史会话 / 本机分叉，本机与远端**同一条路**：经那台机器的后端（`user_files::BackendDoor{origin}` 或 `--fork-session`），
后端没连上 ⇒ 明确报错、不回落（`D11`）。守着这件事的判据：
`write_site_registry_tests::every_monitor_write_site_lands_outside_the_users_files`（monitor 写盘落点分类闭集、**没有「用户文件」一档**）·
`::the_files_that_used_to_write_users_files_write_nothing_now`（搬走写盘的那几份零写原语，带正控）·
`remote_write_registry_tests::every_remaining_sftp_write_lands_outside_the_users_files`（剩下的 SFTP 写只剩部署物 / 自有暂存区 / 指名待收）。
唯一指名待收的是 `sftp_pool::download_inner`（下载落到本机用户选的路径）→ SR1b（SFTP 进本机常驻后端）。

**为什么不能松动**：cc-monitor 的核心价值主张是 "看 claude 的输出不破坏它"。一旦允许 monitor 在用户数据上**非显式**写，用户对 "数据源 = 我自己的命令痕迹" 的信任就崩了。上述豁免要么是**非用户数据**（自部署 bin），要么是**用户显式动作**（删除 / metadata），且各带独立 realpath 白名单。

**F47 SFTP 文件面板不在本约管辖内（澄清，非例外/非松动）**：Batch14-F47 起 cc-monitor 挂了一个**用户亲自驱动的通用 SFTP 文件传输面板**（浏览/上传/下载/改名/删除任意用户文件）。它是**独立文件传输功能**，与本约「monitor 作为监视器只读 Claude 数据源」**正交**——它写的是用户浏览到的普通文件，不是 Claude 的 jsonl/pidfile，且每次写都是面板内一次直接用户手势（绝无自动/后台写）。**防误伤守卫**（`claude_data_fence::is_protected_claude_data_path` —— 〔步 H2 2026-09-21，用户裁「拆」〕它已从 `sftp_pool` 搬成**独立一族**，本段与下面 F03b 段共用它这**一个**判定；判定的射程一个字没动）:SFTP 写命令**拒碰** `~/.claude/projects/**/*.jsonl` 与 `~/.claude/sessions/*.json`（往正被 Claude 打开的会话文件写会损坏会话；要管这些用历史浏览器）。SFTP 面板走独立 utility 连接池，与数据源流连接分离。
> **〔订正 · FN1 · 第四波 4C · 2026-09-25 · 用户 V119〕** 用户原话「**文件管理器全部都可以改. 不需要任何围栏**」。这一段里的「防误伤守卫」**对文件管理器不再成立**：
> 今天的文件面板就是原生文件窗口 ＋ 后端文件管理写面（`files-*`），会话文件 · 项目目录 · subagent · tasks 都能改名 / 删 / 改权限 / 覆盖；
> 窗口的本地预判、传输台开下载单那一判、后端写面那一问都删了。「每次写都是一次直接用户手势、绝无自动/后台写」**照旧成立**（那是这一段的正题，不是围栏）。
> `claude_data_fence::is_protected_claude_data_path` 今天只剩下面 F03b 那一个用户（守它的判据：`claude_data_fence_tests::the_file_manager_no_longer_asks_the_fence_and_only_the_inbox_editor_does`）。

**F03b 收件箱编辑不在本约管辖内（澄清，非例外/非松动）**〔RW1 · 第四波 2026-09-24：本机 ＋ 远端项目都能编辑（用户裁），读写经那台机器的后端（`files-peek` / `files-put`，写带打开时读到的那一份当 CAS 期望，agent 改过 ⇒ 一个字节不写）；下文 `verified_write` 那一跳已并进后端规则〕：devbench-F03b 起 cc-monitor 挂了一个**收件箱编辑面板**（读写用户自己项目里的 `.claude/planned-build/INBOX.txt` —— planned-build skill 的「结构化注入」进件口）。**口径与 F47 逐条对齐**：它写的是**用户自己项目的普通文本文件**，不是 Claude 的 jsonl/pidfile；每次写都是**面板内一次直接用户手势**（点「保存」，绝无自动/后台写）。**围栏三道**（`skill_host::resolve_editable`）：① 路径 `canonicalize` **之后**做**集合判定**，集合来自声明表 `skill_host::SKILLS` 的 `editable`（**不是一串 `if`**，也不是判字符串——`..` 与符号链接都已解开）② 过 `claude_data_fence::is_protected_claude_data_path`（**纵深**：即使声明写歪也不许碰 Claude 数据；与上面 F47 段**同一个**判定，住址见那一段）③ 目标**必须已存在**（本功能是「编辑收件箱」不是「创建任意文件」）。写本身走 `verified_write::verify_and_rollback`（备份 → 写 → 读回**逐字节**比对 → 不符即回滚），**没有自造第四份写入实现**。⇒ 写面严格等于「声明里那几个真实文件」，今天恰好一个文件名。⚠ **远端项目的收件箱不在此列**：`parity_ledger` 里 `skill.inbox` 记 `Undecided`——「要不要能编辑」没人裁定过，且远端版的第①道（`canonicalize`）在那边不成立。

**A2 多账号只读查询是本约的「读」面延伸（澄清，非例外/非松动）**：`src/backend/observe/accounts_query.rs`（monitor 那一侧〔C4d〕已无文件：界面经通道问、后端出成品）为「按会话切账号」新增三条**纯只读**远端查询（`--list-accounts` / `--session-accounts` / `--account-trust`），**零写入**、**不 shell out**。它把后端的读面从 `<claude_dir>` 扩到三处新位置，各自有硬边界:

1. **`$ACCTS_DIR/accounts.json`**（cc-acct-iso 的 manifest，契约 v1）—— 只读整份 JSON;`configDir` 视为**不可信字符串**，逐条过 shell-safe 白名单（与 cc-acct-iso 的 `path_shell_safe` 同一套字符集），不合格的账号直接丢弃。
2. **`/proc/<pid>/environ`** —— **只抠三个写死的键**（`CLAUDE_CONFIG_DIR` · `CCM_LAUNCH_ID` ·〔HX1 · D-f〕`ANTHROPIC_BASE_URL` —— 最后那个的值带中转钥匙，只折成「走不走本机中转」一个布尔、值本身不出参），绝不回传整个环境快照（那里面有用户全部的密钥类环境变量）。pid 来自 `<claude_dir>/sessions/<PID>.json` 的文件名。⚠ **第二个键是 `K-P5f` 加的**（会话身份 token，写侧住 `history.rs::LAUNCH_ID_VAR`）；**变的只是那个计数词，这条铁律一格都没松**：键名**不是参数**（后端侧两个常量），所以这条查询仍然不是「任意环境变量读」原语，也仍然绝不回传整个快照。⚠ 这句话在盘上**散着好几份副本**（`src/doc/IPC-PROTOCOL.md` 的 `--session-accounts` 那一行、后端侧 `accounts_query.rs` 头注、monitor 侧 `local_accounts.rs`）——`K-P5f` 改了其中三处、**漏了本处**，`K-P5g` 补上并把这一族副本登记进 `doc_claim_registry.rs`（那张表的判据从生产代码里数出今天真读几个键，再与每一份副本的计数词对拍）。
3. **`<configDir>/.claude.json`** —— **只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔**，绝不回传文件内容（内含 `mcpServers` 的环境变量，可能有 API key）。且 `configDir` **必须逐字等于 manifest 里某个账号的 configDir**，否则拒绝——否则 `--account-trust` 就退化成任意文件读原语。

**`.credentials.json` 只 stat 存在性、永不读内容**（`loggedIn` 字段就是这么来的）。**动凭据的部署操作（`cc-acct-iso … --apply`）绝不经后端**——那会往只读组件里塞写权限;一律由 cc-monitor 拼好命令后弹一个**用户可见的终端窗口**执行（`launch_remote_terminal`，同时也是 `/login` 必须走 TTY 的唯一出路）。见 `.claude/planned-build/account-isolation/DESIGN-account-switching.md` §6。

**`sessions/` 必须留在 cc-acct-iso 的共享集**：后端靠 `<claude_dir>/sessions/<PID>.json` 判活并拿 pid，进而探测账号。若哪天把 `sessions/` 挪进隔离集，各账号的 pidfile 会散到各自 config-dir，cc-monitor 会看不见非默认账号的会话。

**★★ 第 7 条例外：往 `<claude_dir>/skills/cc-bus/` 装 cc-monitor 自带的 skill（用户 2026-08-13 裁定「开」）**：
`PS1` 摸底逐条读完上面那 6 条例外，**没有一条覆盖它** —— 于是它当时停成一条待裁（`U10b`）。
用户 08-13 裁「开」。⇒ 本条是**例外**，不是澄清：它**真的往 `<claude_dir>` 写**。
落点唯一：`<claude_dir>/skills/cc-bus/`（17 个文件，`include_bytes!` 内嵌自 `src/shared/cc-bus/`）。
四个配套要求**一条都不许省**，实现在 `src/bridge/src/cc_bus_deploy.rs`
（〔RW1 · 第四波 2026-09-24〕**落盘经本机后端**：读 `files-peek` · 备份改名 `files-rename` · 写 `files-put` 带 `parents` · 可执行位 `files-chmod`；
`fenced_dest` 只读判、后端那道围栏是第二道 —— 四个配套一条没省）：
1. **用户显式动作** —— 只由设置页那个按钮调，**绝不**在启动/后台路径上跑；
2. **独立 realpath 白名单** —— `fenced_dest`：`canonicalize` 之后必须仍在 `claude_dir` 底下，
   挡「`skills` 是个指向别处的软链」；判据 `a_symlinked_skills_dir_is_refused` 钉着；
3. **幂等** —— 逐文件比内容，一致就**一个字节都不写、也不备份**；
4. **可撤销** —— 覆盖前把旧目录整个 `rename` 成 `cc-bus.bak-<ts>`（原子、不留半份）。
⚠ 为什么值得开这个口子：不开的话，仓内那份 cc-bus 的修复**永远到不了本机**
（`P4b` 删掉「spawn 复用活会话」那一刀实测就卡在这里 —— 两份差的正是它改的那 2 个文件）。
⚠ `uninstallable` 仍是 `false`：**卸载没做**，如实声明。

**P8a 插件面枚举是本约「读」面的又一次延伸（澄清，非例外/非松动）**：P8a 为
「有哪些 Claude Code marketplace」新增一条**纯只读**的本机查询（〔RM1b〕读法搬进后端 `observe/plugins_query.rs`；
〔C4b〕monitor 那一侧的模块与那条命令已删，界面经通道直接问帧命令 `plugins-marketplaces`），
**零写入**、**不 shell out**、**不轮询**（按需一次）。它把本机读面从 `projects/` + `sessions/`
扩到 `<claude_dir>/plugins/`，边界两条：
① 只读 `<claude_dir>/plugins/known_marketplaces.json` 与各 marketplace 落点下的 `<落点>/.claude-plugin/marketplace.json`
   两种文件，**各带字节上限**（`byte_cap_registry` 里逐条登记了超限怎么办）；
② **不解释、不校验 `.gcs-sha`**，也**不去数** `marketplaces/<id>/plugins/` 那个目录 ——
   那是上游下载器的快照，把它当成「用户装了几个」是**说假话**（本机实测：manifest 声明 276 个、
   快照目录里 39 个）。**「装了/启用了哪些插件」今天在盘上没有真相源**，界面明说这一点，
   不猜（待决 `U10d`）。
⚠ 这条读面**是新增的直读点**，已在 `local_read_surface_registry` 的递减棘轮上登记并写明退役条件
（后端补 `--list-marketplaces` 后随 F10 一起退役，与 `parity_ledger` 里那笔远端欠账**同一条**）。

**F62 从历史某轮建分支不在本约管辖内（澄清，非例外/非松动，用户 2026-07-12 拍板）**〔RW1 · 第四波 2026-09-24：**本机那一支也交给后端写了** —— exec 本机后端的 `--fork-session`（`control/fork_write.rs`，与下面 G6 远端同一条子命令、同一份结果解释 `remote_branch::interpret_fork_exec`；〔LOC1a 2026-09-25〕exec 那一趟与这份解释都删了，本机远端同走那台后端的帧命令 `session-fork`，本体仍是 `fork_write.rs` 那一份），monitor 进程不再 `O_EXCL` 写会话文件；本机后端不在 ⇒ 明确报错、不回落。下面的性质（只增不减 · 只收 sid · 绝不覆盖）一格没变〕：`history::create_branch_session` 在用户**显式**点历史查看器里某条消息的 `⑂` 时，把 `[根…该消息]` 前缀**复制**成一个**全新** `<new-sid>.jsonl`（原生 `/branch` 的 `forkedFrom` 格式）。这与本约**正交**——本约防的是 monitor **改坏/覆盖/后台写**它正在监视的**现存**会话文件；建分支是**纯新增产出**（用户框定："复制产出一个文件，而非侵入式改动"），**原会话一字节不改**，且只写**新生成、collision-check 过的 sid**（`out_path.exists()` 则拒，绝不覆盖任何现存会话）。防越界守卫〔`K-R88` 2026-09-13 换形状〕：入参从**路径**收成 **sid**，由两侧共用的 `branch_core::find_session_file` 在记录树里枚举出那份文件（sid 先过 `[A-Za-z0-9-]` 白名单，符号链接不算命中）—— 界外那种入参**连表达都表达不出来**。〔散文墓碑〕原措辞逐字留档：「`validate_branch_source`（canonicalize + `starts_with(projects)` + `.jsonl`）与 delete 同构」，那个函数**今天已经不在了**。破坏性上它比已放行的「显式删除」更弱（只增不减）。

**G6 远端分叉：本约的写面从「monitor 写远端」扩到「后端在远端写」，故单列一段（澄清 + 收窄，用户 2026-07-30 拍板「要对远端也 branch」）**：
远端会话的 jsonl 在另一台机器上，monitor 够不着 ⇒ 分叉这件事由 **后端自己在那台机器上做**
（`src/backend/control/fork_write.rs`）。monitor 侧入口〔步 12·C 09-20〕**已与本机那条合并**：
一条 `history::create_branch_session` 吃 `origin`，远端那一支是 `remote_branch::create_remote_branch_session`
（**不再是 IPC 命令**，是那条命令的远端分支）。
它与上面 F62 那段是**同一件事的远端形态**（用户显式点 `⑂` → 复制 `[根…该消息]` 前缀成一个全新
`<new-sid>.jsonl`，**原会话一字节不改**），但因为写的人从 monitor 变成了后端，多出三条收窄：

1. **后端的写面被守卫钉死在一个模块**（`readonly_guard`，E50 两层收窄）：默认层禁 11 类写操作，
   白名单层**只放行 `control/fork_write.rs` 这一个路径**（U3 起是**路径**不是文件名，见 §41.6），
   且该文件必须含 `.create_new(true)`、
   不得含 remove/rename/truncate/append/overwrite/`create(true)`/`set_len`。
   ⇒ 「后端会写盘」这件事**不可能悄悄扩散到第二个模块**。
2. **`create_new(true)` = `O_EXCL`**：目标已存在直接失败。既消掉 `exists()→write` 的 TOCTOU 窗口，
   也自证「绝不覆盖任何现存会话」——两个 monitor 同时分叉同一会话，后到的拿到错误而不是把先到的盖掉。
3. **只收 sid、不收路径**（`branch_core::find_session_file` —— 〔`K-R88` 2026-09-13〕**两侧同一份**，本机那条命令也收 sid 了）。后端是被 ssh 远程调起来的，
   少一个可被构造的路径入参就少一条路径穿越面；sid 先过 `[A-Za-z0-9-]` 白名单，再**只在
   `<claude_dir>/projects` 下按文件名匹配**。monitor 侧 `remote_branch::validate_fork_id` 同一字符集
   再拦一道（fail-fast，不是最后一道）。

#### ★ D1 裁决（U8a-2，2026-08-02）：铁律收窄为「**后端进程自身**不许写用户既有数据」

**问题**：后端一旦起 `ccm` / `claude`，用户既有数据**一定会被改**（CC 写 jsonl、重写 pidfile；
`shared/ccm` 头注还写明 `--tmux` 会顺带写 `~/.claude.json`）。
而 `readonly_guard` 只认**文件系统写模式**、**不认 `Command` / `spawn`** ——
散文铁律的字面意思不放行这件事，**而 CI 永远不会红**（§0.2 登记过：「护栏与散文说的不是一件事」）。

**裁决**：取主计划 §5 的选项 ① —— 铁律收窄，**间接写不算违反**。边界如下，**这三句就是边界本身**：

1. **责任在被起的那个程序。** CC 写它自己的 jsonl / pidfile 是 CC 的行为，不是后端的写。
2. **后端的责任是「不越权替它决定写什么」。** 起一个程序、让它按用户的意图去跑 = 允许；
   替用户决定往它的配置里塞什么 = 不允许（除非走下面那条受管例外）。
3. **收窄不许退化成「隔一层 exec 就绕过」** —— 所以它**必须**配一份逐条清单，见下。

**强制条件（与裁决同时生效，不是建议）**：起进程的面**逐条登记**，且登记是**机检**不是散文：
`readonly_guard::spawn_registry::every_process_spawn_in_production_is_registered`
—— 生产段每一处 `Command::new` 都必须在清单里并写明「做什么、为什么不违反收窄后的铁律」。
**清单本身就是家**（`readonly_guard.rs` 的 `ALLOWED`），本节**刻意不复制它有几条、是哪几处** ——
这一句原先存了一个固定处数并逐个列出，而当时真值已经比它多两处
（漏了 `control/gate.rs` 与 `control/kill.rs`）。**清单是对的，错的是它旁边这段散文**（audit-0805 V6 逐行核出）。
新增一处而不登记 ⇒ **红**（已变异复验）。

**U8a-2b 的写面（第 4 处）逐条**：`tmux new-session -d` / `set-option @ccm_sid` /
`set-option set-titles[-string]` / `send-keys` / `has-session`。改的是 **tmux server 的运行期状态**，
加上把一条载荷敲进那个会话的交互 shell —— 之后是**那个 claude 进程**在写它自己的 jsonl。
按裁决第 1 条，那是被起程序的行为；按第 2 条，后端没有替用户决定往任何配置里塞东西
（载荷是上游给的、`@ccm_sid` 是本方自建会话的身份标记）。⇒ **不违反收窄后的铁律**。

⚠ **U8a-2b 顺带修掉了这条机检自己的一个洞**：`every_process_spawn_in_production_is_registered`
原先扫的是一张**手写的文件清单**，新文件不在表里就根本扫不到 —— 实测新增
`control/launch.rs`（起 tmux）时它全绿。已改成**递归遍历 `src/`** + 文件数自检。
（同文件上方的 `scan()` 早在 Phase G 审计时就因同样理由改成递归了，这里没跟上。
这是本仓「扫描面画小了」那一族的第五次，而且是 D1 那轮我自己埋的。）

⚠ **它挡什么、不挡什么**（如实登记，别以为它保证了更多）：挡「悄悄新增一个起进程点」；
**不挡**「已登记那条改成起别的东西」—— 登记的是**文件名**不是完整 argv，
而 argv 里有格式化变量（`tmux_probe_script()` 拼的脚本），钉不住也不该钉死。

**预信任那条是单列的受管例外**：后端 **主动**让 `ccm` 去写 `~/.claude.json`（首次进某目录的信任确认）
—— 那是第 2 条里「替用户决定写什么」的一个**明确例外**，因为不做它自动化会卡在弹窗上。
它今天还没落到后端侧（`--account-trust` 只**读**、只回三个布尔）；真落地时要在这里再列一行写面。

**为什么不算松动**：破坏性上它与已放行的「远端历史删除」（例外 3）不在一个量级——那条真的会让
用户的会话消失，这条只增不减。且它**没有引入新的写入者**：后端早就在写远端
（`~/.cc-monitor/bin/` 自部署，例外 2），G6 只是让它多写一个 `projects/` 下的**新** jsonl，
并第一次给它的写面套上了机检守卫。

**A5 tmux 会话名契约是跨语言隐性耦合，改一端必须同步另一端**：本工具建的远端 tmux 会话名恒为 `cc-<sid8>[-N]`（前端 `deriveTmuxName`/`pickFreshTmuxName` at `src/remote-launch.ts` 生成）。Rust 侧 `tmux::is_ccm_tmux_name`（`src/bridge/src/backend/control/tmux.rs`）用 `cc-` 前缀 + `[A-Za-z0-9_-]` 白名单**门控 `tmux_send_keys`**（A5 换号重启在旧号 send `/compact`），**绝不向用户自己的其它 tmux 会话发按键**。两端各写一份该契约、仅靠测试对齐（跨语言无法共享函数）。**若改了前端的 tmux 名前缀/字符集，必须同步 Rust 白名单**，否则 send-keys 会被静默拒绝、compact 悄悄失效（不阻断重启，但优化白丢）。注：`kill_remote_tmux`（F79）**曾**沿用既有行为无此白名单 —— ⚠ **F04b 2026-08-04 订正：这句自 F04 起就假了**（F04 给 kill 补了 Gate 2 union，F04a 又加了 Gate 3；**F04b 起它的主路是后端的 `kill` 命令**，三道门在后端侧复现）。原文留痕是因为下面那半仍然成立且仍在生产：A5 破坏性重启在 `restartTabWithAccount` 里用 `live.sid === sid` 精确守卫兜底——只精确命中 `@ccm_sid` 才 kill，绝不按 cwd 回退猜（防杀错会话 + 双进程）。**A5+**：`tmux_send_keys` 加了可选形参 `enter`（`Option<bool>`，**缺省 true**）——`enter=false` 时命令省去尾 `Enter`（优雅退出发 `Escape` 打断当前回合时用，防误提交输入框队列文本），`/compact`、`/exit` 等仍附回车。前端旧调用不传 `enter` → 逐字节等价旧行为，向后兼容。

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
| `history-metadata.json` | **真相** | 〔C4d〕本机常驻后端 `history_annotations.rs::answer_annotate`（路径仍由 `history.rs::metadata_path` 算、起后端时交过去；文件原地不动） | 按 sid 的 star/重命名/隐藏——用户策展意图 |
| `auto-launch.json` | **混（良性）** | `auto_launch.rs` | `enabled`=真相；`monitor_exe_path`=派生(每次启动 `current_exe()` 自愈改写) |
| `sid-hwnd-cache.json` | **缓存** | `bind.rs` | sid→HWND，能从 PS 握手重建 |
| `ps-registry/` `ps-await/` | **缓存/IPC** | `bind.rs` | 跨进程握手，启动重扫 |
| `logs/` | **缓存/派生** | `logging.rs` | 诊断日志，滚动保留 3 天（§15） |

- **真相** = 用户手写/意图，**删了丢东西、要备份、要迁移友好**。
- **缓存/派生** = 能从别处重建，**随便删**。
- **规矩**：**新增任何 data dir 文件，必须在 `data_paths.rs` 的枚举里声明它是哪类**（那里是逐个 data dir 文件的唯一权威枚举点，带 description）。truth 的格式要迁移友好；cache 允许随手删。
- **两笔边界别误读**：① `auto-launch.json` 同文件混真相+派生，是**良性**的（派生位自愈，整体迁移不坏）；② `ps-registry/`/`ps-await/`/`logs/` 在子目录，那是**按用途/IPC 对端分**的，**不是按真相/缓存分**——`sid-hwnd-cache.json` 这个纯缓存反而在根、跟 `config.json` 平级。
- **机器强制形态（已落地）**：`data_paths.rs::DataPathInfo` 带一个**非可选**的 `class: DataClass`（`truth` / `cache`）枚举字段 ⇒ 「新文件必须选类」由类型系统兜住；设置页「数据位置」每行据它显示「删了会丢 / 可随手删」。每一项的类与本节上面那张表两向相等（异源判据）。〔用户 2026-09-24 裁「提前做」，推翻 2026-07-16 F65「现在不做」；第四波 ST2 落地。〕

---

## 3. 所有跨进程 JSON 文件 = UTF-8 无 BOM

- 写入端：Rust 用 `std::fs::write` 直接 UTF-8；PS 用 `[System.IO.File]::WriteAllText` + `UTF8Encoding($false)`（**禁用** `Out-File -Encoding utf8` —— PS 5.1 那个写 BOM）。
- 读取端：Rust 解析前 `raw.trim_start_matches('\u{feff}')` 兜底剥任何 BOM。

**为什么不能松动**：v1.7.0-1.7.7 整套 cc 集成"装上没用"7 个版本的真凶就是 BOM。`serde_json` 不剥 BOM 直接解析失败 → 早 return → 后续逻辑全跑不到。

---

## 4. profile 等用户文件写入 = 后端 `files-put`：CAS ＋ backup ＋ 原子替换 ＋ 写后校验 ＋ 回滚

〔RW1 · 第四波 2026-09-24 · 现措辞〕**用户文件（rc / `$PROFILE` / `.mcp.json` / skill 收件箱 / skills 目录）只由后端写**，
规则只有一份：`src/backend/control/files_write.rs::put_text`（线上 `files-put`）——
① `expect` 必给（读改写之间盘上那份被改过 ⇒ `stale`、一个字节不写）· ② 与盘上逐字节相同 ⇒ 不写 ·
③ 要备份 ⇒ `O_EXCL` 另存 `<名>.ccm-backup-<毫秒>-<序号>`（沿用原权限位）· ④ 替换：
**unix** 同目录 `O_EXCL` 暂存旁名写满、沿用原权限位、换名上位（原子）；**Windows** 已在的目标**就地覆盖写**
（保住 dst 的 ACL / ADS / 创建时间 —— 后端没有 `ReplaceFileW` 那条平台原语；**代价：非原子**，兜底是 ③ 的备份 ＋ ⑤ 回读回滚）·
⑤ 回读**逐字节**比对，不符回滚（原来在 ⇒ 原文换回；原来不在 ⇒ 删掉刚建的）· 盘上有字节却读到空 ⇒ 停（OneDrive / 杀软那一形）。
最后一段是链接 ⇒ 改真文件、链接留着。monitor 侧只「读 · 算 · 交」（`user_files::edit`）。
判据：`files_write_tests.rs` 的 `put_*` 一族（含 Windows 那条 `put_keeps_explicit_acl_entries_on_windows`，⚠ 只在 Windows 上跑、本机门禁只编不跑）。

下面是这一条的原文（monitor 进程里写 profile 的那一版，`profile_installer` 的原子写原语已删），**理由那一半今天照样成立**，
只是承担它的从 `ReplaceFileW` 换成了「Windows 上就地覆盖 ＋ 备份回读兜底」：

不能用 `std::fs::rename` / `MoveFileExW` 直接覆盖用户文件。必须：

1. 备份原文件到 `<path>.ccm-backup-<ms>`
2. 写 `.tmp` → `ReplaceFileW(dst, tmp, NULL, REPLACEFILE_WRITE_THROUGH, ...)` 替换
3. `read_to_string` 回读校验长度
4. 不匹配 → 从 backup 恢复

**为什么不能松动**：
- `MoveFileExW(tmp, dst)` 用 tmp 的 ACL（继承父目录）覆盖 dst → 用户 explicit ACE 丢失 → Documents 重定向到非默认盘的用户读不了自己 profile。**ReplaceFileW 专门设计来保留 dst 的 ACL/ADS/创建时间**。
- OneDrive online-only placeholder / 杀软可能让 `read_to_string` 返 `Ok("")` 即"磁盘有内容读到空"，纯写就是清空用户内容 → backup + 校验是双保险。

详 [`files_write.rs`](../../src/backend/control/files_write.rs)（〔RW1〕原来指 `profile_installer.rs`，那份原语已删）。

---

## 5. JSONL 单一时序由 seq 字段 + RecordTimeline binary insert 共同保证（v2.6 B 重构）

**后端契约**：后端的 `watcher.rs::read_new_lines`（`src/backend/observe/`）给每读出的一行分配 per-file 单调递增的
`seq: u64`（`SeqCounter` 跨调用累加、截断不重置；`--tail-only` 下起点是当前完整行数 ⇒ seq 就是行号，
与 monitor 旁路快照的编号同一个空间，§25a）；〔CF1 · 2026-09-24〕**本机与远端同一个来源**：monitor 自己那套
jsonl watcher 与它的第二套游标 / seq 已删，本机会话的行也是本机后端的 `line` 帧（`ssh_source·rs::LineIntake`）。
`bridge::JsonlLinePayload` 携带该 seq 字段；所有交付路径（〔CF2 · 第四波 4B〕会话流经通道 `subscribe`：
实时逐行 / 成批切块 / 就绪点重放，一格的体是 `bridge::SessionStreamFrame`）都透传 seq 不变。seq 保证**时序**、不保证**投递次数**——投递语义是 at-least-once
（截断重读会换新 seq 重投整个文件），详 § 25。

**前端契约**：每个 Tab / SessionViewer 持一个 `RecordTimeline`，
`insert(seq, element)` 用 binary search 找位置 → `stream.insertNode(element, anchor)`
按 seq 单调维护 DOM 顺序。后端 emit 顺序、chunked emit 块到达顺序、live / batch
路径混合，**对前端视觉顺序都无影响**。

**为什么不能松动**：
- 之前用"多 flag 协调"路径（PayloadSource batch/live + inPrependMode + pendingPrependFragment
  + EventReplay.replaying 等 5 个 flag）反复出 inter-flag 相位 bug。
  v2.6 B 重构把所有 flag 替换为 seq + binary insert。
- 重放期间新到的真新行直接当场交出去（〔CF2〕过了就绪点的订阅收实时行）；前端 timeline
  按 seq 把它们放到正确位置——不再需要"replaying 期间 push 等末块后 catch-up"的
  特殊路径。

**演进**：v2.3 加 chunked emit / v2.4 加 PayloadSource / v2.5 加 replaying flag + catch-up tail —— 都在试图修补"多 flag
状态机相位 bug"。v2.6 B 重构是一次性消除整套机制。

---

## 6. session 探活双重校验（PID + procStart）

〔LOC1b · 第四波 4D〕这一格今天住**后端**（本机远端同一份）：`observe/watcher.rs` 宣告前的冒名检查（procStart 逐位相等 / 容差）＋
`platform/pidwatch`（Linux pidfd · Windows 死亡事件）。monitor 那份 `is_session_active`〔散文墓碑〕（`OpenProcess(QUERY_LIMITED) + GetExitCodeProcess == STILL_ACTIVE`
＋ `GetProcessTimes` creation FILETIME 与 sessions/<PID>.json 里 `procStart` 字段 100ms 容差比对）随本机判活改由本机后端的帧来删了；
原则不变：判「同一个进程」要 PID ＋ 启动时刻两样。

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
- **async task 内禁止 `std::thread::sleep` / 同步阻塞**（issue #20 增补）：`tauri::async_runtime::spawn` 的 task 里节流用 `tokio::time::sleep(..).await`，真长阻塞走 spawn_blocking。一次同步 sleep 压住一个 tokio worker，worker 数有限，攒多了饿死全部 async 任务（F5 的重放为此 async 化 —— 今天是 `event_replay·rs::ready_point`；重放缓冲的入口 `on_line_batch_awaited` 大 batch 路径块间用 `tokio::time::sleep`、**发完才返回** —— 行 emit 因此严格先于随后的 SessionRemoved 归档。〔CF1 · 2026-09-24〕原先还有一份把块序列 spawn 出去、「返回≠emit 完成」的孪生入口，只供本机 watcher 用，随它一起删了）

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

详 [`logging.rs`](../../src/bridge/src/logging.rs) 的 `init()` 函数。

---

## 16. monitor 单实例运行

同 user / 同机器同时只允许一个 cc-monitor 进程。由 [`tauri-plugin-single-instance`](https://v2.tauri.app/plugin/single-instance/) 强制 —— **必须是 Builder 链上第一个 plugin**（plugin 文档约束）。第二个实例启动时：

1. plugin 通过 OS mutex 检测到第一个实例存在
2. 通知第一个实例的回调（在 [`lib.rs::run()`](../../src/bridge/src/lib.rs) 里 `unminimize + show + set_focus` 主窗口）
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

[`src/events.ts`](../events.ts) 的 `drain` 循环每次 `handlers.onLine(p)` 必须 try/catch。单条 record 处理抛错时**只**记 log + 跳过该条，**不能**让异常逃出 while 循环 —— 否则后续上千条 record 永远滞留 queue 不被处理，前端看上去就停止刷新了。

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

**实测（F63 当时，2026-07-16）**：本机 771 会话 / 16 万行，7 个未知 type 共 ~8,800 条以前被静默丢弃（占 5.6%），**uuid 全为 0**——即此刻并没有在误折叠，F63 是**保险**（不再丢 + Claude 发带链身份新类型时自动扛住）。`cc-monitor-unrecognized` 是**本地自造信封**（前缀防撞真类型），**不是** Claude 真实 jsonl 类型、不参与两端 schema 对账。

**★ 复测（U-CC1，2026-08-02，17 天后）**：本机 **1,904 会话 / 472,115 行 / 非法 JSON 1 条**；
**20 种 type**，monitor 认识 11 种 ⇒ **未知 10 种 / 27,747 条 / 5.88%**（`uuid` 仍全为 0，逐种核过）。
也就是说 CC 在 17 天里**新增了 3 个记录类型**：`started` · `result` · `fork-context-ref`
（前两个来自新的 `subagents/workflows/wf_*/journal.jsonl`，第三个说明 CC 侧长出了
**第二套分叉记账**，与我们仿的 `forkedFrom` 不是同一套）。

⚠ **这次复测是人手工扫语料才发现的 —— 那正是问题所在。**
「未知 type 不 warn」是**对的**（20,526 条 `mode` 会刷屏），但**宽容 ≠ 无声**：
U-CC1 起，四个降级点各记一笔有界的账（`src/bridge/src/drift_ledger.rs`），
经设置面板「机器 → <那台机器> → 足迹 → 未识别的数据」按需查看（〔ST2 / ST3 · 第四波〕顶层「改动足迹」页已并进机器页；账本第一层键是 origin，每台机器只看得到自己那一份）。
**以后靠那一页看，别再靠人扫语料**（也别再往这段散文里手抄数字 —— 它已经过期过一次）。

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

`MessageStream`（[`src/stream.ts`](../stream.ts)）维持贴底时必须遵守三条，违反任一条都会让启动重放期间"最新消息整行高频上下微抖"回归：

1. **`snap()` 必须守卫**：只在 `scrollHeight - clientHeight - scrollTop > 1`（确实落后底部）时才写 `scrollTop`。**禁止**每帧无脑 `scrollTop = scrollHeight`。
2. **「视口上方」插入不手动补偿 scrollTop**：`insertNode` 对 anchor≠null（插到中间/上方）的情况不调整 scrollTop，交给浏览器原生 CSS `overflow-anchor`（默认 auto，**禁止**给 `.stream` 设 `overflow-anchor: none`——补批在**同一同步任务内**的临时关闭是唯一豁免类——两个实现点：`tabs.fillAbove`（F40b）与 `session-viewer.maybeFillAbove`（F39），见第 3 条，且还原必须在 finally）维持视觉稳定。手动补偿 + anchoring 会 double-shift。
3. **重放期「视口上方」旧内容不建 DOM（Batch13-F40a 尾部优先收纳）**：`TabManager.onLine` 按 seq 门控——`seq < tab.window.floorSeq` 的旧记录只进 `TailWindow` 账本（meta/branch 数据经 `routeMetaAndBranch` 照喂），**根本不建卡不挂 DOM**；后台 virgin tab 在 `onBatchEnd` 空闲物化尾段 / `switchTo` 时同步物化。**启动重放**的上方插入从源头消失——严格强于历史方案 deferMode（延后到一帧批量挂载，2026-07 F40a 退役）。**大增量批**（>600 行落已渲染 tab → 后端切块、末块先发）的老块中部插入由 F40b 关闭：批期「`seq ≥ floor` 且 `< timeline.maxSeq`」的记录进 per-tab `midBatchBuffer`，`onBatchEnd` 排序后一次 `batchInsert` 挂载——逐帧中部插入为 0。**上翻补批（F40b fillAbove）**：临时 `overflow-anchor:none`（防 WebView2 原生锚定与手动补偿 double-shift）、测量→渲染→`scrollTop += ΔscrollHeight` 回写必须在**同一同步任务**内完成（不许 await/rAF 打断）；选区进行中暂缓补批（unwrap/rebuild 会杀选区）。**物化/补批插卡前必须 `branchFolder.unwrapAll()` 摊平、插完 `rebuildNow()` 无条件重折**（`flushPending` 的 setsEqual 短路会把摊平永久化），批间孤儿 tool_result 由 `reconcilePendingToolResults` 回填——注意 fallback 单元是**组内实体**（timeline entry 是组 root），reconcile 摘单元后若组壳已空会连根摘壳并返回 root，**调用方必须对返回元素 `timeline.removeByElement` 出账**（否则账上挂已离场的组 root）；`MessageStream.insertNode` 对「anchor 不是 contentEl 直接子节点」有爬升/降级防御（防 NotFoundError 丢记录）。

   **3b. 〔2026-09-24 · `设计/10` 骨架〕接上骨架的 tab / 查看器：「单洞后缀」不再成立，换成下面这组。** 上面第 3 条那句「已渲染集恒为按 seq 连续的尾后缀」只对**没接上骨架**的（拿不到索引：本机后端不在 / 老后端 / Codex / seq 对不上）逐字成立。接上之后：
   - **已渲染集 = 尾后缀 ∪ 若干岛**；「哪些 seq 还没物化」的真相源是 `SkeletonView` 的**占位集**（`isPending`），不再是 `floor`。占位与已渲染卡**不相交**：占位 `[lo,hi)` 里一张卡都没有。
   - **往占位里建卡只许经骨架**（`fillVisible` / `ensure`）—— 骨架先交给宿主建、再把占位切开。绕开它在占位中间直接 `renderRange` / `renderPayloadsBatch` ＝ 卡落进占位里、总高算两遍。
   - **只物化与视口相交的那一段**（±0.5 屏，每轮 ≤300 行、≤4 轮）；一次滚动不从尾巴往上一批批补。
   - 占位本身是 timeline 条目（`seq = lo − 0.5`、`kind: "card"`、无 `data-uuid`）⇒ 二分插入的锚照常、工具组不跨空洞合并、`BranchFolder` 把它当断 run。
   - **视口稳定**：物化与 `attachGaps` 钉住**视口里最上面那张已渲染卡**的屏幕位置（占位可能同时插在它上下两侧，ΔscrollHeight 补偿只对「全在上方」成立）；视口整个落在占位里就不补偿。同一同步任务内测 → 改 → 回写、临时 `overflow-anchor:none`、`finally` 还原（与第 3 条同一豁免类）。
   - **接之前对拍 seq 空间**（抽几条 uuid → 索引里必须同 seq），对不上就不接；接上之后再发生截断重读**不会被发现**（没有持续对拍）。
   - **正文不再驻留**（`设计/10` 步 8）：接上之后前端账本只留离尾巴最近的 `FILL_BATCH` 条、monitor 重放缓冲只留尾巴 `REPLAY_TAIL_KEEP` 条；其余滚到时按偏移要回来（`read_session_range`）——**没见过**的行走 `onLine` 全套（按重放语义：不触发自动切 tab / 轮次通知），**见过**的只建卡（旁路账早记过、去重会拒）。岛里迟到的 live 行就地建卡，占位里的照旧收纳。

**为什么不能松动**：根因实测定位 —— 末块先发的重放把旧消息逐条插到"贴底视口的上方"，持续约 60 帧；每次上方插入都触发浏览器重排 + 重做 scroll anchoring，而 HiDPI / 高刷屏分数像素下，整数 `scrollHeight` 与分数布局的舍入误差**每帧不同** → 整块内容逐帧 ±0.5px 高频重绘。deferMode 时代压成一帧后实测抖动帧数 66 → 1；F40a 后上方插入次数为 0。注意：`scrollTop` 本身并不震荡（单调增长），所以**只测 `scrollTop` 发现不了这个 bug**，要测可见元素 `getBoundingClientRect().top` 的逐帧反转。

---

## 22. 独立窗口契约（viewer #10 / settings F82a）

独立窗口（`viewer-<sid>` `bootstrapViewer` / `settings` `bootstrapSettings`）依赖下列契约，违反任一条都会让窗口白屏 / 卡死 / 收不到数据 / 关不掉——**全是"静默失败"**（不报错，只是白屏 / 收不到 / 卡死 / 点了没反应），极难凭看代码发现，都是实测踩出来的。**任何新独立窗口照抄这套脚手架，逐条对照适用性。**

1. **开窗 IPC 必须 `async`**（viewer + settings 都适用）：`open_session_in_new_window` / `open_settings_window` 等创建 `WebviewWindow` 的命令必须是 `async fn`。Tauri 2 同步 `fn` 命令在**主线程**执行，而 `WebviewWindowBuilder::build()` 内部把创建派发到主线程并阻塞等 → 在主线程等主线程 = 死锁（新窗口白屏 + 整个 app 卡死连关闭都点不了）。
2. **定向事件 target-kind 对齐**（viewer 适用；settings **N/A**——无会话流、跨窗同步用广播）：给单个窗口定向投递（〔CF2 · 第四波 4B〕今天是会话流的交格：`chan/webview.rs::WebviewSink` 发 `chan-items`，主窗口与 viewer 都是定向）用 Rust `emit_to(EventTarget::webview_window(label))` ↔ 前端 `getCurrentWebviewWindow().listen`（`src/ipc/chan.ts` 那一处；起停事件仍是广播，viewer 照旧 `bindEvents({windowScoped:true})`）。**禁止**用 `&str` 目标（→`EventTarget::AnyLabel`）配模块级 `listen`（→`Any`）—— Tauri 2 按 kind 匹配，`Any` 监听命不中 `AnyLabel` 发射，事件静默丢弃。**广播**（前端 `emit()` / Rust `AppHandle::emit`，`Any`）是通配，模块级 `listen`（`Any`）收得到——settings 的 `settings-applied` 跨窗同步正走广播↔模块级 listen（Any↔Any），恰好避开该坑。
   〔S5 · 第四波 · 2026-09-24〕第 1、2 条从此有会红的判据（从前只写在这里）：`tests/bridge/lib_window_lifecycle_tests.rs::every_window_building_command_is_async`（建窗点集合两向相等 ＋ 每处 `async fn` ＋ 紧挨 `#[tauri::command]`）· `…::every_emit_to_targets_a_webview_window_not_a_bare_label`（`emit_to` 调用点两向相等 ＋ 目标由 `EventTarget::webview_window(` 绑定）。前端那半（`windowScoped: true`）仍由 `tests/invariants-frontend-guard.vitest.ts` 钉。
3. **异步 `listen`/`bindEvents` 必须先注册再触发 emit**（viewer 适用；settings 因 emit 只在用户开窗后保存才发生、远晚于主窗口启动注册，无竞态）：`listen()` 异步注册，注册完成前后端 emit 的事件会丢。〔CF2〕会话流的订阅（`bindEvents` 的 `streams`）在起停监听全部注册完之后才登记 —— 订阅一登记，句柄就可能开始交格。
4. **精简模式 CSS 不能塌 grid 行**（viewer + settings 都适用，解法不同）：`display:none` 一个 grid **item**（如 viewer 的 `#tab-bar`）会把它从 grid 移除、剩余 item 前移落行 → viewer 必须只为剩余 item 定义对应行数（`auto 1fr 24px`）。settings 换了个更稳的解法：`body.settings-window-mode` 直接 `display:none` 隐藏 grid **容器** `#app` 整块（非其内 item，无前移塌缩），面板 `position:fixed` 脱流铺满。
   〔订正 · U1 · 2026-09-24〕**settings 那一半已不适用**：三入口拆分之后设置窗是独立入口 `settings.html`（`entry-settings.ts`），页面里**没有** `#app`、也没有 `body.settings-window-mode` ⇒ 不再是「精简模式」，不存在要塌的 grid。本项今天只约束 viewer（`viewer.html` ＋ `entry-viewer.ts`）。三窗各自的模块图与 CSS 清单由 `tests/entry-graphs.vitest.ts` 钉。
5. **关窗要 `core:window:allow-close` 能力**（settings 适用；任何前端调 `getCurrentWindow().close()` 的窗口都适用）：该 JS API 走 `plugin:window|close`，受 ACL 门控，而 `core:window:default` **只含 getter 类权限、不含 `allow-close`**（同理 minimize/set-fullscreen 也得显式加）。capability 的 `windows` 列了该窗口标签还不够，**必须**把 `core:window:allow-close` 加进 `permissions`，否则 ×/取消/Esc 关窗被 ACL 拒、`void` 吞掉 → 点了没反应（系统标题栏原生 X 仍可关，更隐蔽）。
6. **复用 `dispatcher` 的独立窗口必须自调 `dispatcher.start()` + `applyOverrides`**（settings 适用；任何含 overlay / 快捷键录制的独立窗口都适用）：设置窗有**自己的** dispatcher 实例；`dispatcher.start()` 是唯一挂 window keydown 的地方（快捷键录制的按键捕获 + Esc 经 overlay LIFO 逐层关都在其中）。不调 → 窗内快捷键编辑器录制收不到键、Esc 无法关嵌套 overlay。**别手搓 window 级 Esc 监听**——它会与栈内 overlay 的 Esc 双触发（既关 overlay 又关整窗）。让面板作 overlay 栈底（`pushOverlay`），其 `handleEsc`→关窗。

---

## 23. 复用 `.stream` 容器的非-Tab 视图必须显式恢复可见

基类 `.stream`（[`src/styles.css`](../styles.css)）默认 `visibility: hidden` + `pointer-events: none`，仅 `.stream.active` 才可见——这是**多 Tab 机制**：N 个 tab 各持一个 `.stream`，TabManager 只给当前 tab 加 `.active`。

任何**复用 `.stream` 样式但不归 TabManager 管的视图**（`SessionViewer` 应用内只读查看器、未来别的只读流）**必须在自己的 CSS 里显式 `visibility: visible`**，因为它们的元素永远不会拿到 `.active` 类。

**为什么不能松动**：v2.8.1 的"历史会话点进去空白"就是这个坑——`SessionViewer` 流元素 class 是 `stream session-viewer-stream` 没有 `.active`，命中基类 `visibility: hidden`，2000+ 张卡片全渲染进 DOM 却不可见，而状态栏（不在 `.stream` 内）照常显示记录数 → "有记录却空白"的迷惑现象。独立 viewer 窗口（§ 22）复用 TabManager 所以有 `.active`，不受影响；只有自建流的 `SessionViewer` 中招。**禁止**给非-Tab 视图的流元素只复用 `.stream` 而不补 `visibility: visible`。

---

## 24. 远端活跃集 `remote_active` 恒等于"前端当前应视为 live 的远端 sid"（issue #20）

`lib.rs` 的 `remote_active`（`Arc<Mutex<HashSet<String>>>`）**唯一写者**是 `remote-session-emitter` 线程：后端的 session added/removed 与断连 flush 都经同一 `remote_tx` 通道到达，且**先维护集合、再做 emit 等副作用**。`frontend-ready` 对账用"sid 在 EventReplay buffer 里、但不在集合里"判死、补发 `session-ended`。

两条派生约束：

1. **任何让远端行进入 EventReplay buffer 的路径，其 session-added 必须先于（或同批于）行到达该通道**——目前由后端协议保证（added 帧先于该会话的行帧）。绕过集合注入远端行（多 host 扩展、远端历史 #16、测试灌数据）会让 F5 把活会话误归档。
2. **前端必须把 `session-ended` 与行事件同序处理**（`events.ts` 的 queue，#20 一并改）。ended 若抢在积压重放行之前执行，归档会被后续远端行的 un-archive（`tabs.ts` ensureTab，仅远端）翻回 live，对账等于无效——这正是 #20 初版后端-only 方案被审计打回的原因。

**为什么不能松动**：对账是把"一次性 ended 信号"在重载后重建出来的唯一机制；集合不准 = 要么僵尸 live Tab 复现（漏归档），要么活会话被误杀且无后续行救活（误归档）。断连窗口期的误归档是**有意取舍**（重连后后端重发 added + 重放行 → un-archive 自愈）。
〔GP1 · 第四波〕上一句那个取舍**收窄了**：断连 flush 送进通道的 removed 一律 `RemovalCause::Unseen` ⇒ emitter 发 `session-unseen`（说不清），不再发 `session-ended`；F5 对账里「buffer 里有、集合里没有」的远端 sid 按所在的那台分 —— 那台此刻**报完了**活会话清单（收到过 `sessions_replayed`，`ssh_source::listed_origins`）⇒ 补 `session-ended`（原样）；**没报完**（断着 / 还在初扫）⇒ 补 `session-unseen`。`设计/30 §3.5.7a`「`Unseen` 不许被显示成『已结束』」；判据 `ssh_source_f032_idle_tests::gp1_*`。集合本身的写法一字未改（removed 照旧先从集合里摘）。

**F74c(#60-A) 补充——tmux 存活对账是 `remote_tx` 的第三生产者**：tmux 存活收割器（带外杀 tmux 后端 → 变灰）与后端帧、断连 flush **并列**为 `remote_tx` 的生产者，**必须**把 retire 的 sid 当 `SessionChange{removed}` 经该通道下发，**绝不**直接写 `remote_active`、**绝不**让前端直接 archive——唯一写者仍是 `remote-session-emitter`。误判防线（`ever_bound` 门 + debounce + 漂移靠 announced_live 剔除 + 空 backend/NO_TMUX 跳过）在 `tmux_reconcile::reconcile_step`（纯函数、source-agnostic），阈值真机标定。**后人给收割器接线时不许把它直连前端或直写集合。**

> **`zero-poll-liveness` 更新（P5）——事件路同样只经 emitter**：后端现在会主动推正向死亡帧
> `TmuxSessionClosed { name }`（tmux hook → SIGUSR1 → 差分算出消失的会话名，见 §41）。
> 它是 `remote_tx` 的**第四个**生产者，与前三者一样**必须**走 `SessionChange{removed}`
> ——`ssh_source.rs` 收到该帧后拿 name 反查 sid，然后送 emitter，**零新写点**。
> 查不到 sid 时（never-bound 会话 / 快照还没到）**不猜**，交回收割器兜底。
> 事件路只是**绕过 miss 计数**的快路径，`RETIRE_MISS_THRESHOLD >= 2` 与快照对账路径一字未动。

> **audit-fixes F03.2 更新——收割器已从 8s poller 改为「收帧驱动」**：原 `tmux_reconcile.rs::run_tmux_reconcile_poller`（8s 定时轮询 + `snapshot_announced_by_origin`）**已删**——cc-monitor 侧零轮询（唯一周期=后端内部 tmux ls）。收割逻辑现内联在 `ssh_source.rs::stream_loop` 的 `InboundFrame::TmuxSessions` 帧臂：后端每次推该 origin 的 `tmux ls` 原文即对账一次（⚠ **F12 2026-08-04 订正**：原写「每约八秒推一次」（那个字面串不再写在这里，否则本守卫会命中我自己的订正文案）——**P5 之后那句是假的**，后端侧的周期 ticker 已删，推帧由 tmux hook 事件驱动。这句是 F01 那次「四处『每 ~8s』」普查**漏掉的第五处** —— 那次只扫了 Rust，没扫 `doc/`；F12 起 `frame_cadence_guard` 的扫描面已含 `doc/**.md`，本句再写回去就会红），`tracked = 本连接 announced（keys=live sids）∪ 本 origin idle 会话`，调 `reconcile_step` 去抖 retire → 送 `removed` 给 emitter。`reconcile_step` 与 `ReconcileState` 保留为纯决策（F90 可 lift）。

## 24bis. 远端 idle-tmux 灰灯（audit-fixes F03.2）：`REMOTE_IDLE` 与 `remote_active` 正交、同一 emitter 单写

远端会话三态：**live**（claude 在跑）/ **idle-tmux**（claude 退出但 tmux 会话仍在 → 灰灯、可 attach 复用）/ **archived**（tmux 也没了）。承接 §24 单写者不变量，落地约束：

1. **`REMOTE_IDLE` 是独立账本，唯一写者仍是 emitter**：`ssh_source.rs` 的 `REMOTE_IDLE`（origin → idle sid 集）与 `remote_active` **正交**——`mark_idle`/`clear_idle` 只在 `lib.rs` 的会话 emitter 里调（〔U4b · 第四波〕远端那条 `remote-session-emitter` 的 removed/added 臂，外加本机 emitter 的 `classify_removed` 分流与 `session_facts` 出口 —— 都在 `lib.rs`，单写者判据照旧），其余路径（收割器、F5 对账）只 `snapshot_idle_*` 读。**绝不**给 `SessionChange` 加字段承载 idle（收割器仍只发 `{removed}`）。
2. **emitter removed 臂据 tmux 存活分流**（`classify_removed` 纯函数 + `find_tmux_origin_for_sid`）——
   ★ **S0（2026-07-31）加了一道前置：`cause` 先于快照裁决**。后端现在在 `session_removed` 帧上
   带 `cause`（additive，`Gone` 不上线、缺省即 `Gone`）：`Superseded` = 同一个 pidfile **原地换了 sid**
   （`/branch`、`/clear` —— claude 进程不重启，只是 sessionId 变了）⇒ **恒归档，根本不查快照**。
   为什么必须绕开快照：那个入参对这个场景**恒错** —— 旧 sid 的 tmux 格子确实还在，但它现在挂的是
   **新** sid；而这份快照在 P5 删掉 8s ticker 之后，`/branch` 不触发任何事件路径去刷新它（§41.3 盲区 ④）
   ⇒ 判成 idle 就是一个**永远消不掉、也 attach 不上**的灰点（用户 2026-07-30 实测「杀不掉」）。
   `Gone` 维持下述原语义：`Some(origin)`=tmux 会话尚在 → `mark_idle` + emit `SESSION_IDLE` + **不 forget**（不进归档、`remote_active` 早已在上方移出该 sid，idle 天然在集合外，**不新增 `remote_active` 写点**）；`None`=tmux 也没了 → `clear_idle` + `forget` + emit `SESSION_ENDED`（原归档路径）。判据 **command-agnostic**：`TmuxSessions` 帧可能是陈旧的（⚠ **F12 订正**：原写「最长 8s 陈旧」，那是 P5 前那个 8s ticker 的口径；今天推帧由 hook 驱动，**陈旧上界取决于 hook 覆盖面**——F02 量清后端只装 `session-created`/`closed`/`renamed` 三条，覆盖不到的变化可能**永不刷新**），退出瞬间 command 列可能仍是 claude，故「claude 死」由 backend-removed 边沿判、「tmux 在」由 `@ccm_sid` present 判（见 `tmux_origin_for_sid`）。
3. **idle→archived 的产出者 = 收帧收割器**：idle sid 并入收割器 `tracked`；且因 `@ccm_sid` 铁证其绑过 tmux，作 `reconcile_step` 的 `pre_bound` 直接播种 `ever_bound`——否则「SessionRemoved 删 announced」与「emitter mark_idle」之间的跨线程缝里那帧会漏置 `ever_bound`，令 idle sid 永不累计缺失 = 连接内卡灰关不掉。tmux 真消失 → 收割器去抖后 retire → emitter 走 `None` 归档。
4. **前端灰灯与 §24 第 2 条同源**：`session-idle` 与 `session-ended` 同进 `events.ts` 的 queue（对同一 sid 二者互斥、emitter 择一）。〔U4 · 第四波起〕tab 的会话状态是**两轴**的 `Tab.state`（`tab-session-state.ts::SessionState`，活性 × 可恢复性）：`session-idle` ⇒ **可重连**（死了、容器还在 —— 不再算活，状态栏「活跃」数不含它），`session-ended` ⇒ **已结束**（只能 resume）；转移只在 `nextState` 一处，行为只经 `isResumeOnly` / `hasTerminal` / `isLive` 三个谓词读。（旧的 `Tab.tmuxIdle` ＋ `TabStatus` 一轴半写法已退役。）清灰**主**信号 = `ensureTab`（远端 tab 又收后端重宣告/行 = claude 复活，queue 内与行保序）；`session-activity` 为次要（非 queue、null-activity 后端下不可靠，**不可**作唯一清灰路径）。

**单写者已机器化**（Phase G）：第 1 条「`mark_idle`/`clear_idle` 唯一写者=emitter」原靠注释约定、`cargo check` 抓不住；现有 `ssh_source.rs::f032_idle_tests::remote_idle_single_writer_guard` 扫源码断言这两个写函数**只被 lib.rs 调用**，emitter 之外新增写者即测红（同 F08 后端只读护栏的机器化思路）。

**原「已知残留」已修（2026-07-30，`zero-poll-liveness` P1；用户当日松了「后端零改」红线）**：收帧收割器原先对**空 backend 一律保守跳过**（`ssh_source.rs` 的 `!backend.is_empty()` 门），代价是当**被杀的是该 origin 最后一个 tmux 会话**时（tmux server 随之退出、`run_tmux_ls` 回空串）收割器整段跳过 ⇒ 该 idle-tmux 灰灯**卡到断连 flush 才清**。

**修法**（比原记档设想的更细，因为 P0 实测把状态空间量清了）：
1. **后端让 rc 透出**——`run_tmux_ls` 原先 `tmux ls … 2>/dev/null || true` 把 tmux 的 rc **吞掉**，五种观测压成"空串/有内容"两种；现改为 `exec tmux …`（rc 原样成为 `sh` 的 rc）+ 一个约定 rc 表示"PATH 里无 tmux"，折成四态 `Sessions / ZeroSessions / NoTmux / Unobservable`（`watcher.rs::classify_tmux_probe`）。
2. **P0 实测订正了原记档的措辞**：原文说的「命令成功但零会话」在默认 `exit-empty on` 下**不出现**（server 随最后一个会话退出、rc=1）；但 `exit-empty off` 下**确实出现**且 **rc=0 + stdout 空**。两者对 retire 决策等价 ⇒ 合成一个 `ZeroSessions`（区别只对 P3 的复活监视有意义 ⇒ 将来加细分**不必改帧契约**）。
3. **wire additive**：`TmuxSessions` 帧加 `observation: Option<String>`（`"zero_sessions"` / `"no_tmux"` / `"unobservable"`），**有会话时省略** ⇒ `raw` 载荷与之前逐字节一致 ⇒ **旧 monitor 行为零变化**（空 raw 照旧保守跳过）。**不 bump `PROTO_VERSION`**。取值集是 monitor↔后端的**第三个双写点**（前两个：`TMUX_LS_FMT` · `NO_TMUX`），由 `tmux_tests.rs::observation_tokens_double_write_point_stays_in_sync` 钉住。
4. **monitor 把那条内联 if 提成纯函数** `tmux::classify_tmux_observation`（原判断住在需要真远端连接的 `async fn` 里、单测碰不到）。`ZeroSessions` ⇒ 返回**空集但有效**的 `Backend(∅)` ⇒ 照常进 `reconcile_step` 累计缺失；`NoTmux`/`Unobservable` 才跳过。

**修完后的延迟**：该场景从「永不（卡到断连）」变成 **`RETIRE_MISS_THRESHOLD`(2) × **当时**的推帧节拍（P5 前那个 ticker，已删）≈ 16s**——**是有界化，不是即时化**。

> **已落地（2026-07-30，P3/P5）**：8s 推帧间隔本身**已删**，四路信号全部事件驱动 ⇒ 该场景走 tmux server 的 `pidfd`，**实测 27ms**（跨 cgroup 整锅 SIGKILL 30ms）。「多个中杀一个」走 hook→SIGUSR1 + 正向死亡帧，**实测 126ms**（对照组：拆掉 hook 5042ms）。详见 **§41**。

**一处刻意的保守**：`rc=1` 直接判 `ZeroSessions`，意味着"socket 权限异常"这类罕见情形也会被判成零会话（理论上可能误 retire）。缓解：socket 路径 uid 隔离。~~**P3 落地后收紧**~~ **✅ P3 已收紧**（`watcher.rs::classify_tmux_probe`）：「server 活着但 `tmux ls` rc=1」= 真异常 ⇒ 归 `Unobservable`，帧契约未改。**★ 一处比原设想更细的地方**：判据**刻意不依赖「pidfd 是否醒过」**——那会在 pidfd 路失效时把 rc=1 **永久**压成 `Unobservable` ⇒ 永不 retire；改成直接查 `/proc` 里 server pid 还在不在（一次存在性读，无挂死风险），有专门的变异钉住。

~~**尚未真机生效**~~ **✅ `BUILD_ID` 已 bump 为 `p1r-event-liveness`**（2026-07-30，P7）——在此之前它一直是 `p1q-accounts`，已部署的旧后端不会被判 stale、不自动重装 ⇒ 本修复在远端**休眠**。**如实记**：这次 bump 原计划排在 P5，**P5 漏做了**，是 P7 开工复测时才抓出来的（「有些遗漏不会红任何测试」的又一例：BUILD_ID 是个字符串常量，改不改都全绿）。真机生效仍需重部署。

---

## 25. 行事件投递是 at-least-once —— 按 uuid 累积状态的前端模块必须自行幂等（issue #25）

后端到前端的 jsonl 行投递**不保证 exactly-once**。已知重投路径：

- **后端截断重读**（`src/backend` 的 `watcher.rs::read_new_lines`：`len < cursor.seen_len → start=0`）：整个文件**换新 seq** 重投（seq 不重置，见 § 5）；触发时有 `jsonl truncated` warn 留痕。**已知静默缺口**：截断到空（len==0）那一轮刻意不喊（无重读发生），文件随后重新长出内容的全量重投也无 warn——排查重投时「日志无 truncated」不能排除此路径。〔CF1 · 2026-09-24〕本机与远端是**同一份**实现（monitor 自己那套本地 watcher 已删，原先它与后端各一份、靠注释对齐）；
- **远端后端重连重放**（issue #17）：从 seq 0 重发整个活跃会话（**通常同 seq**——后端 SeqCounter 是进程内存态，重连即新进程从 0 起编号。**已知缺口**：若断连前发生过远端截断重读，旧 seq 已爬过文件行数，重连后前端 `tab.seenSeqs`（重连不清空）会把断连期间新增行的 seq 误判为已见而拒渲染，直到 seq 超过旧高水位——三条件叠加的低概率场景，后端转正前需修：重连时按 origin 清 seenSeqs 或 uuid 去重前置，见 Batch4 Phase G 验收记录）。

Batch4-F14 起两端只消费以 `\n` 结尾的**完整行**（torn tail 延迟到补全，offset 按实际消费推进，截断判定用 seen_len 高水位）——"读中文件增长导致 offset 回退换 seq 重投"这条历史路径已消除。**已接受的取舍**：①最后一行是完整 JSON 但永远等不到尾 `\n`（写端在两次 write 之间被 kill 且文件从此不再增长）→ 该行永不投递、无日志；实测 Claude Code 每条记录以 `\n` 收尾（2026-07-03 抽查 8/8），此情形视为非标准写端。②长度基截断检测的固有盲区：两次事件之间文件先长到 X > seen_len 再被重写为 Y ∈ [seen_len, X) → 任何仅凭长度的方案都检不出（pre-F14 同样检不出，非回归）；需内容指纹才能封死，成本不值。

`tab.seenSeqs`（#17）只挡**同 seq** 重投；换新 seq 的重投在 seq 层不可见。因此：**任何按 uuid（记录身份）累积状态或构建拓扑的前端模块，必须对"同一记录再来一遍"幂等**——入口按 uuid 拒重（保首见），不得假设上游只投一次。现有履约点：`tabs.ts onLine` 的 `processedUuids`（入口整体拒重——渲染与 trackAgents 等副作用一并挡住；无 uuid 的元信息记录放行，issue #26）+ `computeMainBranch` 入口去重 + `BranchFolder.seenUuids`（issue #25）三层。新增"消费行事件"的模块（如 viewer 新路径、#16 远端历史）必须同样履约。

**为什么不能松动**：实测 1 条重复 attachment 即把 1541/4331 条主线误折成「已被 ESC 回退」、全文件重投折掉 4137/4331 且首条 user root 出局（issue #25 两次实锤）。重复记录毒化 Kahn 拓扑的 remaining 计数 → 重复点全部祖先落 leftover fallback（latestDescTs/hasAssistant 全错）→ 被 fork 赢家 / 多 root 分类（#22）放大成整段历史折叠。且重复常是 attachment/isMeta 等**不渲染 DOM 的记录**——肉眼不可见、每次重算复现、进了 event_replay buffer 后 F5 带毒，不自愈。

---

## 25a. 远端 seq 是行号空间（Batch8 起）——重连碰撞在无截断前提下源头已除

Batch8-F25/26 起（p1f 后端 + tail-only）：后端连接时把各文件 seq 计数器
初始化为**当前完整行数**、新行 seq=行号；monitor 旁路快照按 0..L'-1 行号编 seq。
推论：**无截断前提下**重连后新行 seq ≥ 断连前高水位（文件只增长）——§25 留档的
"重连 seq 从 0 重数 → seenSeqs 碰撞吞行"的**常规形态**源头消失（旧后端全量
推流路径仍存在，随后端升级自然消亡）。**截断残余**（审计 D 收窄措辞）：
断连期间 /clear 截断 → 新后端 prime 到 L_new < 旧高水位；或上一连接内截断
重读把计数器抬超实际行数——两者仍可短暂吞行，靠快照重拉 + uuid 幂等（§25）
兜底，属 §25 三条件缺口的收窄而非消除。快照重拉（每次连接重建队列）整体幂等：
重复 (sid,seq) 行被去重吸收，代价只是带宽（增量协商留 backlog）。

## 26. bg 会话门是数据层配置门；后端流模式 flag 必须先于查询模式判定剥离（Batch7-F24）

`kind:"bg"` 的取舍史：F21 一刀切不算会话 → 用户实测"工作跑在 bg 里但 tab 停住"（可观测性洞）→ F24 反转为**标注而非过滤**。三条子规则：

- **kind 缺失恒视为交互**（旧 CC 兼容），双端一字一致。
- **bg 门在数据层生效**（本地 scan_dir 过滤 / 远端后端 `--with-bg` 参数），不是前端隐藏——关掉 = bg 数据完全不流（省带宽与 buffer，bg 历史可达 10MB+）。开（默认）= bg 建 Tab 带 ⚙，与普通 Tab 平铺（〔BG1 · V125「删掉树」〕原「树状挂同 (cwd, origin) 交互宿主后」删）。
- **后端任何新增流模式 flag 必须在一次性查询模式判定之前从 args 剥离**（`lib.rs::split_stream_flags` 先 `retain` 再判 `!args.is_empty()`）——否则 flag 落进 query 分支，后端打印查询结果退出，monitor 无 hello 死循环。既有实例：`--with-bg`（F24）、`--tail-only`（Batch8-F25）。
- **monitor 只对「声明了对应能力」的后端发该 flag**（F66/#58③ 起，**取代**原来的「build_id 精确匹配」门控）：后端在 hello 帧自报 `capabilities` token 集，monitor 按声明发 flag。**护栏靠「声明 ⟹ 会剥离」成立**——只有会先 `split_stream_flags` 剥离某 flag 的后端才声明对应能力（老到不剥离未知 flag 的后端也老到不声明）。这条约定**由 `every_capability_token_is_strippable` 测试代码强制**（后端侧）：`CAPABILITIES` 每个 token 的 flag 必须被 `split_stream_flags` 剥离，否则测试红。**加新能力 token = 同时加剥离分支**，不然埋死循环。
  - **能力 ≠ 身份（两轴正交，呼应 §28）**：`build_id`（身份，SS-B 单源）管 staleness / 重部署提示；`capabilities`（能力，加法式）管发什么 flag；`v`（proto version）只留破坏性变更（F66 **绝不 bump**）。**2026-07-09 事故的根因正是把「能干什么」错编码成「是不是那个精确构建」**——身份链一环断（发布流水线漏拷清单）就全能力静默关。F66 拆开三者：能力由后端自报，即使身份确认不了也照开。**新原则：绝不用身份匹配代理能力声明。**

## 27. 远端会话生命周期信号的两条载荷型约束（Batch9）

- **F5 重发必须先于 replay**：`remote-session-added` 不进 replay buffer，F5 后远端
  骨架/bg 元数据/初始灯全靠 frontend-ready 时 `ssh_source::reannounce_all` 重发；
  "宣告先于该会话的行"契约在 F5 路径的唯一保证是 lib.rs frontend-ready 处理器里
  reannounce 调用**先于** `event_replay·rs::ready_point`（〔CF2〕F5 重放的就绪点）的顺序（同一 task 内顺序 emit +
  前端同 queue FIFO）。改动该顺序 = 破坏骨架先行契约。
- **status 缺失恒为"未知"**：pidfile 无 `status`（旧 CC）→ 帧不带 → 前端 `act=null`
  不加灯类——双端一字一致（与 §26 "kind 缺失恒视为交互"同族的保守缺省规则）。
- **capabilities 缺失恒为空集（最小能力集）**（F66/#58③）：hello 无 `capabilities` 字段
  （旧后端）→ monitor 解析为**空 Vec** → 不发任何流模式 flag（保守降级 = 2.18.0 行为，
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
- 会话表 / 历史 metadata / 窗口句柄缓存 key = Claude Code `sessionId`（`session_map.rs`、〔C4d〕后端 `history_annotations.rs::Table`、`bind.rs::SidHwndBinding`）。
- ps-registry key = OS `pid`（`bind.rs`）。
- 唯一自铸的 opaque token = bind 握手 marker `ccm-bind-{PID}-{随机8字符UUID}`（`bind.rs`）——**瞬时握手、用完即删、不从路径算**，不当持久身份，合规。
- panorama 进程内选 Engine 的 key 用仓根路径，但**纯内存、绝不落盘**（保持现状，别存盘）。持久的节点身份由 **code-picture-core** 写进侧车 DB、守它自己的 uuid 规矩，cc-monitor 只消费不自铸。〔RM1f 09-25：monitor 进程内那个 Engine 池随内嵌引擎删了（本机全景也经本机后端起全景小程序，一问一进程）⇒ 前半句今天**没有对象**；后半句照旧成立 —— 身份仍由小程序里那份 core 写、monitor 只消费。〕

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
- **F90**（#48 后端登记表）：会话/后端登记表主键必须 opaque + 稳定，用 Claude Code
  `sessionId` 顶着，**不许拿 tmux 会话名 / 主机名 / 路径当持久主键**——否则 §SS-12
  「一端起的会话另一端必须能接」当场崩（换后端 / 换机名字变了就对不上）。

## 29. 会话生命周期解析规则以权威规格为准，两端不许各自发明（F67 / issue #58 单向门⑤）

Claude Code 的 JSONL 会话生命周期解析规则（未知记录抢救、ESC 回退主线检测、end_turn
判定、queue 折叠豁免、api_error 双形态、attachment/isMeta 链、pidfile 保守缺省、PID
复用防护）有**唯一权威规格**：`code-picture/doc/agents/claude-code.md` §9「会话生命周期
解析规则」。

**为什么是单向门**：cc-monitor（Rust）与 aterm（Kotlin）各自解析同一批 JSONL，各自推导
略不同的规则 → **语义漂移**。重复本身可逆（阶段② 抽进后端），但漂移贵——「抽成一个」
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

**契约**（历史：这条身份信道 F02 起住 `shared/ccm` 内部，取代了已删除的
`shared/ccm-wrapper.sh` / `__ccm_rbind` —— 留着这句是为了解释「今天为什么没有 wrapper」）：
tmux user option **`@ccm_sid`** 记「这个 tmux 此刻在跑哪个 sid」（随 `/branch`、`/clear`
实时更新）。选 user option 而非 pane title：**title 会被 Claude 自己的活动标题（`⠂ …`）抢写、
不可靠；user option Claude 碰不到** = 权威带外信号。后端 `tmux.rs::TMUX_LS_FMT` 末列
`#{@ccm_sid}` 读它，`TmuxSession.sid` 承载；空串（未装 ccm CLI / 未经它启动）→ `None`。

**⚠ 谁来写它，`U-NP④`（2026-08-14）换过一次 —— 这段原文写的是「身份回填 poller（住
`shared/ccm` 内部）**每秒**从 pidfile 读当前 sid」，那句话今天是假的。** 用户裁定逐字
「**可以动ccm. 不要轮询**」＋「**ccm做到必须走daemon**」⇒ 那条**每会话一条、与会话同寿、
跑在远端**的每秒循环被**整条删除，不留轮询退路**（连同它的解析器 `_ccm_sid_from_file`）。

今天的写者是 **后端**：`src/backend/control/identity_tag.rs`。
- **触发**：后端本来就在 inotify `<claude_dir>/sessions/`（pidfile 目录）。看到
  `<PID>.json` 的那一刻 pid 与 sid 同时在手；`/clear`、`/branch` 原地重写同一个 pidfile
  ⇒ modify 事件照样送到，跟着重打。**零新增节拍**。
- **定位**（「该打到哪个 tmux 会话上」）：读 `/proc/<pid>/environ` 的 **`TMUX_PANE`**，
  再 `display-message -p -t %N '#{session_id}'` 取句柄，对句柄 `set-option`。
  选它是因为**没有陈旧的可能**：那个值属于进程自己（tmux 起 pane 时注入、`exec` 原样继承），
  与旧 poller「自己读自己、写自己的会话」是同一条自指性质。
  ⚠ 已排除的替代：让 ccm 写一个 `@ccm_pid=$$` 当 join 键 —— 跨平台但**有陈旧面**
  （ccm 退出后值留在会话里，PID 复用时会把新 sid 打到旧会话上 ⇒ kill 杀错）。
- **账号感知**：由后端的 `claude_dir` 决定（原 poller 读
  `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/sessions/`；默认布局下同一 inode）。
- **前置**：没有后端时 `ccm` **响亮失败**（在 tmux 里起有身份的 agent ⇒ exit 2），
  逃生口 `CCM_NO_BACKEND=1` 明示放弃身份、**照样往 stderr 说一句**。不许静默降级 ——
  静默的后果是「会话起来了、monitor 绑不上、点 ↗ 弹『未绑定窗口』而没人知道为什么」。
- **窗口标题**：`ccm` 把 `set-titles-string` 设成 `#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}`，
  标题由 **tmux 自己按 `@ccm_sid` 合成** ⇒ 打标者是不是那个会话自己无关紧要，
  也不需要谁周期性重打（旧 poller 里那句「每 20 秒自愈」随它一起删）。

**铁律**（守 SS-5/SS-9「tab 身份钉在会话身份，找不到就报『不存在』，绝不静默换一个」）：
- **attach / resume 定位后端，一律先按 `sid===@ccm_sid` 精确匹配**（`tabs.ts::findClaudeTmux`）。
- **`@ccm_sid` 已知却无一命中 → 判「目标会话不在任何 tmux」，绝不回退按 cwd 抓同目录别的 claude**
  （那正是撞错会话的老 bug）。只有**整张列表都无 `@ccm_sid`**（老 wrapper）才回退旧 cwd 匹配，
  向后兼容。
- **灰会话 resume 找不到活后端 → 起全新 `--resume`，tmux 名用 `pickFreshTmuxName` 挑不撞名**
  （避免复用被漂移占着的 `cc-<sid8>`），保证落进原会话而非 attach 漂移的别人。
- **`@ccm_sid` 是阶段② 后端 `session.status()` RPC 的先声**（SS-13：tmux 每条能力都是一个
  后端 RPC 的 shell 仿真）；别把它固化成"只有 tmux 能这样"，它是"后端自报当前 sid"的通用形态。

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
这只是声明，resume 可能瞬间失败（会话已不存在/网络抖动），从未被独立确认过。只有通道B
独立读到 Claude Code 自己的会话文件、确认这个 sid 真的在跑之后，才写 `@ccm_sid`。
⚠ **`U-NP④`（08-14）之后通道B 的执行者是后端**（见上「谁来写它」那段），不再是 ccm 里
那条每秒 poller —— 分离**更硬**了：事实的写者从「那个会话自己」变成**独立第三方**，
而且后端打标前已经过了 pidfile 的 `procStart` 冒名检查。两个 key 的语义一个字没变。**任何破坏性判断（Gate 2 远端半支、kill/send-keys 的身份核验）只认 `@ccm_sid`，绝不认
`@ccm_sid_expect`**——否则一个从未真正跑起来过的声明会永久冒充"事实"，被后续的身份核验采信。
两个 key 都遵循"只写不清"的既有约定（见上）；`_expect` 不进 `TMUX_LS_FMT`（守后端零改动的
范围排除），只在窄场景（idle-tmux 置信度判断，F04 本轮未做，留待以后按需评估）按需惰性查。

## 31. 一端起的会话另一端必须能接——前端绝不硬编码会话后端命令（F90 / #48 / SS-12）

**用户 2026-07-15 原话**：「我不能接受这边产生的会话那边看不到。」

**为什么硬**：**会话后端（多路复用器）是机器的属性，不是界面的属性**。桌面用 abduco 起的会话、手机只会
tmux → **接不上**，会话池当场劈成两半。而「桌面起、手机接着用」正是要两个界面的全部理由。**注意**：
路线图把「tmux→abduco」列为「可逆、尽管拖」——那对「哪天整体换」成立，对「两端各用各的」**不成立**；
后者不是可逆决策，是当场把自己劈开。

**会话后端 vs 后台程序（SS-11）**：会话后端（tmux/abduco/dtach）**扶着**跑着的交互程序、握命脉；后台
程序（后端）是**旁观者**，读文件流回来、回答问题、不扶任何东西。**协议可合、进程不能合**。

**最终形态**（三条）：
1. **前端绝不硬编码后端命令**（不准出现可执行的字面 `tmux attach` / `tmux new-session` / `tmux send-keys`）
   → **问一层要**（阶段②问后端，SS-11 保证它永远在；阶段①问前端座 `src/session-backend.ts`）。
2. **某机有哪些后端靠能力探测**（阶段②，接 SS-8 能力协商 §26）。
3. **任何后端变更，动手前必须先答「另一端还接得上吗」，答不上不准做。**

**阶段① 落地（F90，2026-07-17）**：唯一后端 = tmux。命令语法已从 `remote-launch.ts` 收敛进纯座
`src/session-backend.ts`（`SessionBackend` 接口 + `TMUX_BACKEND` 实现 + `SESSION_BACKEND` 活跃句柄，照
`agent-profile.ts` 两轴正交范式：agent-profile=哪个 AI、session-backend=哪个多路复用器）。`remote-launch.ts`
正文**无 tmux 命令字面量**（只留 doc 注释）。

⚠⚠ **那条门禁腐过一次，且腐得毫无声息**〔`P9` 08-12 实测〕：本节原写的是一条**手工 grep**
`grep -nE "tmux (new-session|send-keys|attach)" src/remote-launch.ts` —— 它**只盯一个文件**。
而 `remote-launch-run.ts` 是后来从那个文件拆出去的，**门禁没跟着拆** ⇒ 那边躺着一句手写的
`tmux attach -t '=<name>:'`，违反第①条而无人报警（已改成问座要）。
⇒ 现在它是**机检**：`tests/session-backend-gate.vitest.ts` 扫**整个前端生产段**（排掉座本身、
测试文件与注释行），并反向自检「座里那些语法还在」（否则会因为「哪儿都没有」而假绿）。
★ 教训比这条规则本身通用：**门禁写成「盯某个文件」，就会在文件被拆的那天失效**，
而拆文件的人不会想到去改一段散文。**本阶段不做后端探测/协商**（`SESSION_BACKEND` 恒等 `TMUX_BACKEND`、
无运行时选择）——那是阶段②（§9 轨道二后端在场，才补得了 abduco/dtach 缺的 `send-keys`）。

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
2. `src/bridge/src/backend/control/tmux.rs` 的 `exact_target()` —— IPC 控制面
3. `tests/e2e/restart-shims/core.mjs` —— Tauri IPC 边界的 mock，**结构上无法 import Rust，去重不可能**；
   必须**与生产同构**，否则 e2e 对这条假绿
4. `shared/ccm`（F02 统一启动 CLI）—— 独立的 tmux 命令构造器（不复用前三处，语言/执行环境不同）；
   守卫见 `src/bridge/src/sftp.rs` 的 `ccm_cli_has_required_elements`：**结构性扫描**每个 `-t ` 目标
   （不是固定 needle——固定 needle 版本实测空转，把 `=名:` 全改回裸目标三门禁仍全绿）

e2e 的 shell 探针（`has-session` / `set-option` / `kill-session`）同样要用 `=名:`——**探针本身前缀匹配会说谎**
（只剩 `X-2` 时 `has-session -t X` 返 0，"会话还在"假阳；`set-option -t $S` 会把 `@ccm_sid` 写到错的会话上、
直接污染 fixture）。F01 的整个论点就是"前缀匹配会说谎"，探针不能例外。

**漂移守卫**：`session-backend.test.ts` 有一条读 `tests/e2e/restart-shims/core.mjs` 的断言把 shim 形态与座钉在一起；
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

**验证**：`tests/launch-render-cli.test.ts` 的 #76 防线测试组——通过临时删除 `canRenderCli` 里的
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

**验证**：`tests/launch-render-cli.test.ts` 的 R04① 两条测试——用真实可达的
"账号有 configDir 但无名字"（老式直调路径，见 `launch-requests.ts::accountOf` 头注）
断言 `ok:false` 且**结果里没有 `cmd` 字段**。

### LR1 更新（2026-09-25，U8c-3 前一半）：TS 那份 CLI 渲染器删了，本条今天住在 Rust

上面三段里的 `canRenderCli` / `renderCli` / `tryRenderCli` 与 `tests/launch-render-cli.test.ts`
都是**当时**的住址 —— 那份 TS 渲染器从 U8c-2c-2 起就零生产调用，LR1 把它连同套件一起删了。
**不变量本身一个字没松**，今天的住址与验证：

- 实现：`src/bridge/src/backend/control/ccm_invocation.rs::render_ccm_invocation`（生产入口
  `launch_wire.rs::render_ccm_launch`）。说不出的维度 ⇒ `Refusal::DimensionCannotSpeak`（整条放弃，
  不跳过）；`send-into` ⇒ `Refusal::SendIntoHasNoCliForm`；attach 分支在维度循环之前 return（豁免照旧）。
- 验证：`tests/bridge/backend/control/ccm_invocation_tests.rs` 的
  `a_dimension_that_cannot_speak_abandons_the_whole_line` · `send_into_is_refused_before_any_dimension_runs` ·
  `attach_reads_the_container_name_and_no_modifiers_at_all`（attach 豁免三道闸一次钉）· `a_triggered_dimension_carries_its_own_capability_requirement`；
  外加入库夹具 `fixtures/cli-golden.json`（`req` 由生产的 TS 请求构造现产、`out` 是 `src/launch-cli-golden.ts`
  用例表里的手写期望，`launch_cli_parity.rs` 跑生产命令逐字节比，ok / refusal 两类各自条数恒等）。
- ⚠ 「两种语言各一份、逐字节对拍」这一层没了（已知代价，理由见 `src/launch-cli-golden.ts` 头注）；
  「渲染器该做什么」的独立说法只剩上面两份手写期望。

## 33a. `ccm --print` 是平价预言机——它对**环境变量**说的必须逐条等于真跑做的（U9a / unified-backend）

**它是什么**：`shared/ccm` 的 `--print` 打印「将要执行的命令」而不执行。整个仓把它当
**离线预言机**用——`tests/e2e/ccm-print-parity.sh` 的头注写着，这是「唯一能在没有真远端机器的
场景下验证 CLI 渲染器真的会让 ccm 干对事」的手段。`ccm-cli` 的 44 条契约断言也全建立在它上面。

**病根**：`--print` 那段与真 exec 那段（`do_print` 分支 vs 其后的「非容器路径」段）是
**两份手写副本**。两份副本各自演化，而**没有任何判据比对过它们** —— 既有五套 ccm e2e 里，
`ccm-cli` 只跑 `--print`、`ccm-print-parity` 只验「渲染器意图能被接住」、
另三套验真 tmux 行为，**没有一套把「说的」与「做的」放在一起比**。

**实测到的第一例（U9a 2026-08-02）**：codex + 已在 tmux 内时，exec 路 `export CC_BUS_ID=<会话名>`
（cc-bus 身份，`agent_needs_bus_id`），而 `--print` 只字未提。预言机对那一格说得不全，
**而且不会红**。

### 铁律（范围严格限定在「ccm 自己决定的环境变量」）

1. 凡是真 exec 路会设置的、**ccm 自己决定的环境变量**，`--print` **必须**说出来，且**顺序对齐**。
2. **`--print` 仍然必须是纯的** —— 不查实时 tmux 状态、不写文件、输出对宿主环境逐字节稳定
   （这条比本节更老，写在 `shared/ccm` 的 `--tmux` 撞名段与预信任段头注里）。
   两条铁律**不冲突**，因为：值不知道就**打印配方**（把那段判断原样搬进串里、执行时才求值）。
   `BUS_ID_RECIPE` 就是这么做的。
   ⚠ **反面教材（U9a 自己先踩了一次）**：第一版让 `--print` 直接查 tmux 把值烘进去，
   于是 `ccm-cli` 的 codex 黄金串在开发者的 tmux 里与 CI 上不一样 ——
   我当时的「修法」是给测试加 `env -u TMUX`，**那是为实现让路去改判据**。改成配方形态后，
   那条补丁不再需要，已撤销。
3. 判据是 `tests/e2e/ccm-contract-parity.sh`（A 组差分 + 绝对断言 + B 组 `CCM_ENV` + C 组 probe 契约）。
   **差分不能单独用**（三条独立理由，都是审计变异实证的）：
   - 两份副本**一起**坏掉时差分是绿的 ⇒ 每条保住项**同时**要有一条绝对断言；
   - 两边**都为空**时差分也是绿的 ⇒ 每条 `pair` 先跑一条「真跑确实产出了环境」自检；
   - 比较前做了 `sort` ⇒ 差分对**顺序**结构性失明 ⇒ 顺序类断言必须 exec 侧、print 侧**各钉一条**
     （只钉 exec 侧时，把 print 里的 `$CCM_ENV` 挪到会话级 env 之后 ⇒ 预言机吐出一条
     **落错账号**的命令串而全绿）。

### `--print` **刻意**不说的两件事（别去「修」它们）

铁律 1 的范围是**环境变量**，不是「exec 路发生的一切」。以下两件 `--print` 故意不反映，
理由都是铁律 2（保持纯）：

| 它不说什么 | 为什么 |
|---|---|
| **撞名避让**（`--tmux` 无名时真跑会退到 `-2`/`-3`） | 要知道退到第几个必须 `tmux has-session` 查实时状态。`--print` 展示**基名**；真行为由 `ccm-acceptance` 场景 5/5bis 在真 tmux 里钉住 |
| **预信任副作用**（新目录写 `~/.claude.json` / `~/.codex/config.toml`） | `--print` 不许写文件。真行为由 `ccm-pretrust-acceptance` 钉住 |

**为什么不把两份副本合成一份代码**：exec 那条路是真正跑用户会话的路径，
为了消副本给它引入 `eval` 是拿生产路径换整洁。且配方（文本）与函数（代码）本就无法
真正共用一处 —— 那条串跑在别人的 shell 里，看不见 ccm 的函数。
**副本留着，但它们不一致时会红。**

### 顺带钉住的跨语言契约（同一套件 C 组）

`--ccm-probe` 的首行必须逐字 `name=ccm`（`src/bridge/src/ccm_probe.rs::parse_probe_output`
靠它判「装没装」），`capabilities=` 必须**覆盖** `src/bridge/src/backend/control/ccm_invocation.rs::CLI_REQUIRED_CAPS`
（〔LR1〕原先是 TS `launch-render-cli.ts` 里那一份，随 TS 渲染器删了）
（少一项 ⇒ app 静默退到兜底渲染器，用户看不见）。覆盖是 `⊇` 不是 `==`
（ccm 多声明能力是允许的，今天就多 6 项），别有人把它收紧成相等。

⚠ **精确说法**：这两处的消费方此前只对**手写 fixture** 测过，但真脚本的 probe 输出
**并非全无覆盖** —— `cc-spawn-uplift` 主流程不设 `CCM_BIN`，`cc-spawn` 因而解析到真
`shared/ccm` 并对 `detach`/`tmux-size` 两项 fail-closed。**真正零覆盖的是**：
首行 `name=ccm` · `version=` · `agents=` · 渲染器那 7 项 `CLI_REQUIRED_CAPS`。

## 33b. 载荷编译器搬进 Rust 是**三步**，六条渲染器不变量各自的命运写在这里（U8c / unified-backend）

**背景**：unified-backend 要把「起会话」的决策移到 backend，于是 TS 那两个渲染器要退役。
这件事一轮吞不下（四个文件 614 行 + 三个下游依赖 321 行 + **本节以下六条不变量**），
U8c-1 摸底后拆成三步：

> 🔴🔴 **2026-09-20（`设计/99 §4` 步 22b·B）订正 —— 本节最大的那一句变了：
> `session-backend.ts` 与 `launch-render-fallback.ts` 今天的生产调用方是 0。**
>
> `设计/90 §4 E`「launch 渲染链搬后端」收官：22b·A 在
> `backend::control::payload::render_tmux_outer` 补出了**外层 tmux 那三格**
>（`container:tmux` 的 `create` / `send-into` ＋ `action:attach`）
> 并立了一份入库的逐字节金标准（`fixtures/tmux-outer-golden.json`，13 条）；
> **22b·B 把生产接过去了** —— `remote-launch-run.ts::renderLaunchCommand` 最后那一格
> 改问 `commands.render_launch_payload` 要（请求带 `outer`），
> `renderFallback` 的 `import` 与调用一起退役，**不留回落、不留开关、不留双写**。
>
> **本节受影响的副本，逐处点名**（本节自己写着「订正一句假话时，先把它的全部副本找出来」——
> 这一条由 F01 的四处「每 ~8s」与 F07 漏掉的那一格逼出来的，这次一次点全）：
>
> | # | 哪一处 | 原文哪半假了 | 处置 |
> |---|---|---|---|
> | 1 | 「外层容器那半」下面那张**产出方表**的 `session-backend.ts` 行 | 「**生产远端主路**，天天在跑」 | 就地订正（见那一格） |
> | 2 | 三问表 ② 的「没装 ccm 的远端」那一条 | 「仍是 `session-backend.ts::attach`，经 `launch-render-fallback.ts` 那一支」 | 就地订正。⚠ **判词 `〔现打②〕` 不动**，理由见那一格的〔尺子〕小节 |
> | 3 | `K-R89` 第五次订正段末 | 「**真正让 `renderFallback` 站在生产路上的只有 `renderLaunchCommand` 最后那一行**」 | 就地订正 |
> | 4 | `K-R106` 第六次订正段第 2 条 | 「`launch-render-fallback.ts` 那一处……**是差一个承接方**」＋「那时按定义就没有后端命令行入口可问」 | 就地订正（那句话把**远端的 ccm** 与 **monitor 自己进程里的 Rust 渲染器**压成了一个，正是 `R61` 裁定三禁的那一形） |
> | 5 | 同段末「**U8c-3 的真前置不是文档，是 U8a-2c 与 U12 两件功能**」 | U8c-3 的前置今天与那两件功能无关了 | 就地订正 |
> | 6 | 08-14 复裁表 ② 行 · `K-R105` 复裁表 ② 行 | 「`renderFallback` 的三格全在 TS」 | **不动 —— 那两张表逐字标着「原文留档」**，本节自己的纪律是「读它们要连日期一起读」。改留档等于把过去的话改成假话 |
>
> ⚠ **两处「没变」也要写清，免得下一个人以为顺手该改**：
> · 三问 ① 的判词仍是 `〔现打①〕部分切` —— 它量的是「生产段发不发 `create-or-attach`」，
>   而 22b·B 搬的是**渲染**（monitor 自己渲一条 shell 串交给用户的终端跑），
>   **一次 `launch` 帧都没多发** ⇒ 那把尺子上读数一个字没变。
>   🔴 **但别把「读数没变」读成「什么都没变」**：monitor 那条 `↗` 今天**是 Rust 渲的**了，
>   而这一问问的是「切到后端的 `launch` 了吗」—— **两件事，两把尺子**（`K-R105` 那条一般化：
>   「一句真话摆错了尺子」）。
> · `U8c-3` 状态仍是**待做**：那两个 TS 文件**没删**，它们今天是两份入库夹具的**左边**
>  （「另一种语言的独立说法」）；删了就把跨语言对拍降级成「Rust 没变」的冻结快照
>  —— `launch-render-cli.ts` 曾按同一条先例留着；〔LR1 2026-09-25〕它已删（U8c-3 前一半），
>  夹具的左边换成了用例表里的手写期望（`src/launch-cli-golden.ts` 头注）。兜底这一族仍待做。
>   逐处住址与「还站不站在生产路上」两把尺子见
>   `tests/bridge/backend/control/launch_wire_f07_main_path_tests.rs` 的
>   `TS_FALLBACK_KEEPERS`（尺子A：处数）与 `TS_FALLBACK_REACH`（尺子B：有没有生产调用方），
>   **两张本拍都重裁过**（座那一格 `On` → `Off`），**从源码派生，别在本节抄一份数**。

| 件 | 内容 | 状态 |
|---|---|---|
| **U8c-1** | 载荷编译器进共享 crate `launch-core` + 跨语言逐字节对拍；`history.rs` POSIX 分支改调内核 | **2026-08-02 已交付**。⚠ **P4b 起内核不在共享 crate 里了** —— 它在 `src/bridge/src/backend/control/payload.rs`（后端对它零引用，放共享 crate 的真实原因是 monitor 当时没有 `backend/` 边界）；那个 crate P4c 改名 `shell-quote-core`，只剩 `posix_quote` |
| **U8c-2b-0** ✅ | 账本 S5：POSIX quote 五处合一 + 零命中守卫 | 2026-08-02 |
| **U8c-2c-1** ✅ | **ccm 调用行**进内核（**P4b 起**在 `backend::control::ccm_invocation::render_ccm_invocation`；交付时在共享 crate 的 `launch_core::cli`）+ 跨语言对拍。**不切生产** | 2026-08-02 |
| **U8c-2c-2** | 生产切换：`remote-launch-run.ts` 改调 Rust（需 tauri 命令 + IR 上线形状） | **已交付**（F07 2026-08-04 实测订正：本列此前写「待做」，是**过期陈述** —— 实测两条 tauri 命令 `render_ccm_launch`/`render_launch_payload` 都已注册，生产 TS 三处在调（`remote-launch-run.ts:72,96,301`），`parity_ledger` 也有 `launch.render-cli`/`launch.render-payload` 两条能力） |
| **U8c-3** | 删 TS 渲染器 + IR，收敛下面六条 | 待做 |

### 外层容器那半为什么本轮不搬（**不是因为它没了** —— 我第一版就是这么写的，被工程审计证伪）

一条完整命令分两层：外层 tmux（`new-session … ; send-keys … ; attach`）+ 内层载荷
（`env 前缀 → cd → argv`）。U8c-1 只搬内层，**理由是「先做被依赖的」，不是「外层已经不需要了」**。

⚠ **实测：外层当时有四个产出方，一个都没退役**（2026-08-02 逐条核过）。
🔴 **2026-09-13（`K-R104`）订正：这句话今天假了 —— 四个里退役了一个。**
用量探针那条（`account_usage.rs::build_usage_probe_cmd`）随「探针编排整条搬上后端帧面」
而**整个不存在了**：今天 monitor 一个 shell 字符都不渲染，那几步各发一条帧命令
（`oneshot-session` / `launch send-into` / `capture-pane` / `kill`）。
⇒ 当时是**三个产出方 ＋ 一个已退役的墓碑**；那一格曾由
`doc_claim_registry::the_outer_layer_producers_are_in_the_state_the_doc_claims`
**翻面**钉着（从「必须在」变成「必须不在」）。
🔴 **2026-09-18（`设计/50` 删用量）第二次订正**：用量 ②③ 两轴整轴退役 ⇒
**`account_usage.rs` 这份文件本身也没了**，那条翻面的量法（读那份文件 ＋ 断言函数不在）
会因为读不到文件而恒红 ⇒ 已整删。**今天是三个产出方，没有墓碑那一格。**
⚠ **如实登记为射程边界**：挡「有人重新在 monitor 里拼一条 tmux 编排串」的，
今天只有「那个功能整个不存在」这个事实，**没有判据**。
⚠ **它不改本节的结论**：U8c-3 的两条硬障碍（`create-or-attach` 与 attach 两格未切）
与用量探针无关，一个字都没动。

| 产出方 | 实况 |
|---|---|
| `session-backend.ts`（TS） | 🔴 **〔步 22b·B 2026-09-20〕生产调用方 0。** 原文逐字「**生产远端主路**，天天在跑」—— `设计/90 §4 E` 收官之后那条 `↗` 主路改问 `commands.render_launch_payload` 要（带 `outer`），承接方是 `payload::render_tmux_outer`。座今天只被 `launch-render-fallback.ts` 引，而后者只被两个**金样本发生器**引 ⇒ 它站的是「逐字节对拍的左边」这个位置，不是生产路。⚠ **没退役**：它仍在盘上、仍是那份独立说法，`the_ts_fallback_renderer_now_stands_on_its_own_consumers` 的 ④ 反过来钉着「承接方那两个文件本身还在」 |
| `control/launch.rs`（Rust argv，U8a-2b 建的） | ⚠ **F11 2026-08-04 订正：这一格原写「零生产调用方 —— 全仓 `.call("launch", …)` 只有一处且在 `#[cfg(test)]` 里」，那句已经假了。**〔机检〕生产段 `.call("launch")` 处数：0 处（〔C4e · 第四波 4C〕原来那两处 —— 就地 resume 的 `send-into` = U8a-2c-1 · 送键 = F04c —— 连同 monitor 那两个发送端迁到界面，今天由 `src/tmux-control.ts` 经通道直接说 `launch`）。⚠ **那两处当年都不是「又切了一格起会话」**——`create-or-attach` 与 attach 两格仍未切。⚠ `ssh_source.rs` 那条 `!client.accepts("launch")` 仍在，但它断言的是「某个 hello 没声明 launch」，**不是「生产不调 launch」**（F07 已订正过同一句话在三问表里的那一份 —— **这一格当时漏了**）。🔴〔`K-R105` 09-13〕**这里原来钉着 `:2208` 这个行号 —— 现打那一行是 `SNAPSHOT_MAX_BYTES`，那条断言今天住在别处、而且住在 `#[cfg(test)]` 里**。⇒ 行号撤掉，改指符号（`src/doc/INVARIANTS.md` 里指进本树的行号，要么带逐字校验位，要么别写）。⚠ 那个数**只有这一个家**：`doc_claim_registry::the_doc_number_for_production_launch_calls_matches_reality` 从这里把它读出来与现场数比，多一处调用而不改这里就红 |
| `shared/ccm` | 用户终端那条路 |

且 `control/launch.rs` **结构上不覆盖 attach** —— 它的模块头注逐字写着「本模块**不 attach**」（平面 ③）。
而 `session-backend.ts::createRunAttach`/`attach()` 产出的串尾巴就是 `tmux attach -t …`。

⇒ **U8c-3 删 `session-backend.ts` 之前必须先回答三件事。2026-08-03 逐条实测过了**：

🔴 **2026-09-13（`K-R105`）：下面这张表的三格已经按今天的现打重写过一遍**，
每一格都带一个 `〔现打…〕` 判词，由 `doc_claim_registry::the_three_questions_in_33b_have_todays_answers`
**逐问与现场对拍**（改一问所依赖的行为而不改那问的答案 ⇒ 当场红）。
判词的闭集只有一个家（那条判据旁边的 `THIRTY_THREE_B_QUESTIONS`）——
**别在这一节里复述闭集，也别把判词写进散文**，那会让「恰好一个」那条断言当场失效。
表格后面那几行 08-04 / 08-14 的原文**留档**，读它们要连日期一起读。

| # | 问题 | 答案 |
|---|---|---|
| ① | 生产切到后端的 `launch` 了吗 | 🔴 **〔现打①〕部分切**〔`K-R105` 2026-09-13 第五次订正〕。**两棵树各算一格，08-14 那版只量了前一棵**：<br>· **monitor 自己那条 `↗` 路**（`src/bridge/src/**.rs` 生产段）——**一次都不发** `create-or-attach`；`.call("launch")` 那两处发的是 `send-into` 与 `send-keys-raw`，**都不是「起会话」**（〔C4e〕两处连同发送端迁到界面，今天 monitor 生产段一处都没有；界面经 `src/tmux-control.ts` 只说这两个 mode）。<br>· **后端自带的 CLI 面**（`src/backend/control/ccm/`）——**在发**。`K-P2` `D3`（2026-09-03）把 `ccm --tmux` 接到了后端那条一次性口上，后端侧 `control/launch.rs` 的 `Mode::CreateOrAttach` 分支就是承接方。<br>⇒ **起会话这一格已经有 Rust 承接方、而且真在跑**，只是 monitor 自己那条 `↗` 路没走它。**① 的剩余面从「没有承接方」变成了「monitor 没接过去」** —— 那是两件很不一样的事，而 08-14 之后没人回来改这一格：`launch_wire.rs` 里逐字记着「三问的答案① 变了」，**这张表一个字没动**。<br>⚠ attach 那一格与本问无关（后端结构上不 attach，见 ②）。<br>〔以下 08-04 原文留档〕⚠ **F07 2026-08-04 订正为「部分是」**（原写「否 —— 全仓只有一处且在 `cfg(test)` 里」，那句**已过期**）：实测**生产段有一处** monitor 的 `backend_send_into`（U8a-2c-1 交付；〔C4e〕已迁到界面）🔴〔`K-R105` 09-13〕**原文钉的是 `:111`，现打那一行是 `SendIntoResponse::refused` 的头注，那处调用今天在别的行上** ⇒ 行号撤掉，改指符号⇒ **`send-into` 那一格已切**；`create-or-attach` 与 **attach** 两格未切。`ssh_source.rs` 那条 `!accepts("launch")` 仍在（🔴 原文钉的 `:2208` 已漂，`K-R105` 09-13 撤掉行号），但它断言的是「某个 hello 没声明 launch」，**不是「生产不调 launch」** —— 两件事。〔原文续〕~~U8a-2c 未做~~ ⚠ **F11 2026-08-04 再订正：这半句也已过期** —— **U8a-2c-1 已交付**（`backend_send_into`，`send-into` 那一格），F04c 又接了 `send-keys`（不是「起会话」的格）。仍未切的是 **`create-or-attach` 与 attach 两格** ⇒ 该说「U8a-2c **未做完**」，不是「未做」 |
| ② | attach 那条串归谁产 | 🔴 **〔现打②〕前端仍产 attach**〔`K-R105` 2026-09-13 重量〕，而且**这一问今天是删 `session-backend.ts` 唯一的硬障碍**（① 有承接方了、③ 退役了）。逐处现打：<br>· **装了 ccm 的主机** —— Rust 产（`backend/control/ccm_invocation.rs` 的 `Action::Attach` ⇒ `ccm attach <名>`），U8c-2c-2 起就是这样；<br>· **没装 ccm 的远端** —— 🔴 **〔步 22b·B 2026-09-20〕这一条今天是 Rust 产。** 原文逐字「仍是 `session-backend.ts::attach`，经 `launch-render-fallback.ts` 那一支」；`设计/90 §4 E` 收官之后，`action:attach` 那一格走 `commands.render_launch_payload`（`outer:{mode:"attach"}`）⇒ `payload::render_tmux_outer`。**这条路走得到的前提一个字没变**（探测 `unknown` ⇒ `caps:null` ⇒ CLI 渲染器诚实降级），变的是**接手的是谁**：接手的是 **monitor 自己进程里那份 Rust 渲染器**，不是远端那台机器上的 ccm。<br>〔尺子〕⚠ **`〔现打②〕` 那个判词因此仍然是「前端仍产 attach」，而这不是疏漏**：量法② 量的是「生产 TS 里还有没有人问座要 `SESSION_BACKEND.attach`」，而 `launch-render-fallback.ts` 仍在问（它是金样本发生器那条链的一环）⇒ **那把尺子上读数没变**。🔴 但它已经答不了这一问的**生产面**了 —— 「前端有没有**代码**产 attach」与「那条 `↗` 上 attach 由谁产」是两件事（`K-R105` 那条一般化：一句真话摆错了尺子）。⇒ **登记为判不了 ＋ 缺什么**：要让 ② 的判词跟上生产面，量法得换成「生产**路径**上谁产」，而那要一条**行为**判据（走生产入口真跑一遍），今天它在 `tests/remote-launch-run.vitest.ts` 的 `W22B` 组里；把 `doc_claim_registry` 的量法② 改挂到那一侧不在步 22b 的写区 ⇒ 交回报给 PM。🔴 **`K-R109` 09-13 补一句，别把这一格读成幽灵态**：`R64`〔用@09-13〕逐字裁「不存在什么没装 ccm 装了后端的情况」，但同一条**逐字划出了 `unknown`**（「探不到」与「探到了、没装」不是同一件事，本条只否掉后者）。而 `ccm-probe.ts` 的三态在 wire 上被压成两态（`caps: null`，`K-R95` 登记的缺口）⇒ **一次 ssh 抖动就走到这一支**。⇒ 这一支今天**走得到，而且走到它的不是那个幽灵态**；判据在 `tests/remote-launch-run.vitest.ts` 的 `KR109D3` 两条（`〔现打②〕` 那个判词因此**不动**）；<br>· **本机就地 resume** —— 🔴 **`K-R109` 2026-09-13 订正：这一处接过去了。** 原文写「`remote-launch-run.ts::runLocalResumeIntoExistingTmux` **今天仍直接问座要**」，那句今天是假的：`K-R106` 留下的三处注册面（`generate_handler!` · `parity_ledger::LEDGER` · `src/ipc/commands.ts`）本轮同一拍落地，那条 `↗` 现在 `await commands.render_local_attach({ tmuxName })`，渲不出来就诚实失败、**不回落到前端拼串**。⚠ 用户 2026-08-12 那条裁定（「attach 暂时就用纯 linux bash 以及 windows 的 PowerShell + Windows Terminal」）**一个字都没被推翻** —— 变的是「那一串由谁产」，不是「用什么把它跑起来」（跑它的仍是 `launch_remote_terminal` 那条既有分档路）。<br>🔴🔴 **`K-R106` 2026-09-13 第六次订正 —— 上面那句「后端结构上产不出它」被用户当场推翻了一半，而这一格今天有承接方了。**<br>原文逐字写着「**daemon 结构上产不出它**：`control/launch.rs` 头注逐字『本模块**不 attach**，一次都不』…… ⇒ 这一问不是『还没做』，是**要先有一个产品决定**」。⚠ **那句头注今天仍然对，而它的射程是「远端」** —— 它自己的理由逐字是「在远端，**开不了你面前的窗**」。**本机后端就在用户面前那台机器上**，那条讲位置的约束在这一侧不成立。用户 09-13 亲裁（`DECISIONS.md#R61` 裁定三，逐字「**归本机后端就好了啊**」）并同拍立下：**不许再用「daemon」这个词把「远端常驻的那份」与「后端」压成一个** —— 那正是这句话被读宽的成因。<br>⇒ **本机那半今天产得出了**：`src/bridge/src/history.rs::render_local_attach` ⇒ `ccm attach <名>`，走的是本机 `new`/`resume` 同一条渲染路（`render_local_ccm` → `render_local_ccm_with` → `ccm_invocation`），由 `history.rs::tests::the_local_backend_renders_an_attach_that_lands_on_the_session_it_just_created` 驱动着钉（连「接的是不是刚建的那个会话名」「那个名字过不过 Gate 2」一起）。<br>⚠ **判词没变，而且不许提前改**：前端那条 `↗`（`runLocalResumeIntoExistingTmux`）**还没改成问它要**，接线要动的四处里有两处不在 `K-R106` 的写区（`src/bridge/src/lib.rs` 的命令注册 ＋ `src/ipc/commands.ts`）。⇒ 这一问的剩余面从「**要先有一个产品决定**」变成「**只差把前端那条 `↗` 接过去**」，与 ① 那一格今天是同一种形状（有承接方、没接过去）。<br>⚠ **远端那半不在 `K-R106` 的射程里**：没装 ccm 的远端仍走 `session-backend.ts::attach`，那是「还没装」不是「没路走」（见下方 `K-R89` 那一段）。<br>〔以下 08-03 原文留档〕**一半有答案**：装了 ccm 的主机 U8c-2c-2 起已是 Rust 产（`ccm attach <名>`）；**没装 ccm 的仍靠 `session-backend.ts::attach`** |
| ③ | daemonless 的远端还要不要能起会话 | 🔴 **2026-09-19 第五次订正（`设计/99 §4` 步 8 · 条 80「不要管旧配置」）：这一问退出机检那张表了。** 用户裁了不再管旧配置 ⇒ `remote-config.ts` 里最后那块墓碑（`LEGACY_NO_BACKEND_KEY` ＋ `legacyNoBackendHosts` ＋ `readiness.ts` 那条指名告知）**整块删除**。⇒ 量法③ 的三个载体在盘上**全部不存在**，三格恒 false ⇒ 判词恒为「已退役」⇒ 它与本行**永远对得上**，那是一条恒绿的 ⇒ `doc_claim_registry::THIRTY_THREE_B_QUESTIONS` 删掉 ③ 行、量法③ 与三条 `carriers` 同拍收掉（另有五条只为这个词存在的判据一起退役，逐条点名在那一拍的报告里）。**本行不删 —— 它是沿革。**<br>〔以下 09-13 原文留档〕🔴 **〔现打③〕已退役**〔`K-R105` 2026-09-13 复量，`K-R59` 09-11 落的〕—— **这一问今天不挡任何东西**。量的是那一档的**三个载体**（落盘字段 `REMOTE_HOST_FIELDS` 里那一项 · 机器卡片那个 input · `ssh_source.rs` 那条轮询回落），现打**三个都不在**。⚠ 刻意**不数 `daemonless` 这个词**：`remote-config.ts` 里还留着一处认旧配置的墓碑（`LEGACY_NO_BACKEND_KEY`），数名字会把它读成回潮。<br>⚠ **「这一问退役」不等于「那条路退役」** —— 兜底渲染器今天靠自己的消费者站着，逐处与两把尺子住 `launch_wire.rs` 的 `TS_FALLBACK_KEEPERS`（处数）与 `TS_FALLBACK_REACH`（有没有生产调用方），**两张都从源码派生**。<br>〔以下 08-14 原文留档，读它要连日期一起读〕⚠ **2026-08-14 第三次订正：已决，答案是「要」**（原写「**未决** —— U12 仍是待做项」）。`U12` 那个**件**确实被 `C7` 关掉了，但 `C7` 逐字裁的是「**本机**也要有后端进程」；而 `daemonless` 今天仍是**每台远端主机的用户开关**（`src/settings/machine-card.ts` 的 checkbox「daemonless 降级读取（无需 daemon）」→ `src/remote-config.ts` 的 `RemoteHostConfig.daemonless`，前端生产段 7 个文件 31 处）⇒ 那种主机**存在**，且它的 `↗` 走纯 SSH（`launch_remote_terminal` 不经后端）⇒ 没装 ccm 时命令只能由 monitor 自己渲染。**⇒ ③ 从软障碍（未决所以不敢删）变成硬障碍（已决为「要」所以确定不能删）**。〔散文墓碑〕当年的判据叫 `the_daemonless_remote_still_needs_the_ts_fallback_renderer`（开关哪天真没了它主动红）。🔴 **2026-09-11 第四次订正：那一天到了。** 用户定框 `K35` 逐字「不要有 daemonless。没有没有后端的情况。前端应该就是去调用远程后端的。」⇒ 那个每机开关**整格删除**（字段 · 顶层二选一 · 轮询段 · 界面那一格 · `readiness.ts` 里那条本机豁免，五处一起走，`K-R59`），**「daemonless 的远端」这一类主机从此不存在**。⚠ **而那条判据红完之后的答案不是它自己预写的那句「兜底渲染器少了一类必须服务的主机」**：**那条路另有消费者** ⇒ **前提退役，那条路不退役**。🔴〔`K-R105` 09-13〕这里原来写着两个数（「3 个」「2 个」）——**那是尺子A（标识符出现处数）的读数，而读它的人一律读成尺子B（有几条生产路在跑它）**，已撤；两把尺子各有一个家，都从源码派生。新的存续理由与逐处住址住在 `launch_wire_f07_main_path_tests.rs::the_ts_fallback_renderer_now_stands_on_its_own_consumers` 与它旁边的 `TS_FALLBACK_KEEPERS`（处数从源码派生，少一处就红）；那份手续本身由 `launch_wire_f07_main_path_tests.rs::the_retired_premise_left_a_tombstone_that_is_still_on_the_board` 看着，撕掉它也红 |

⇒ ⚠ **F11 2026-08-04 订正这条推论的依据**：原写「①「否」+ ③「未决」」，而 ① 早在 F07 就订正成了「**部分是**」（`send-into` 那一格已切）。**结论没变**，但依据要换成还量得准的那两条：**`create-or-attach` 与 attach 两格仍未切**（①的剩余面）**＋ ③「未决」** ⇒ 今天删不得：硬删会把「没装 ccm 的远端」与「daemonless 的远端」
两类主机的起会话能力直接删掉，而那两类今天都还成立。

🔴 **`K-R89` 2026-09-13 第五次订正 —— 上面那句「那两类今天都还成立」今天只剩一类，而且拦路的已经不是「哪类主机没路走」。**
本节自己写着「订正一句假话时，先把它的全部副本找出来」，而 `K-R59`（09-11）那一拍的订正**只落在下面那张表的 ③ 行里** ⇒ 08-04 这一段又活了一轮。逐条现打（量于 `89ce650`，量具 `tests/evidence/K-R89-ruler.py`，人群 = `src/**` 去掉 `*.test.ts`/`*.vitest.ts`，剥法与 `launch_wire::production_ts` 同口径）：
- 「daemonless 的远端」**那类主机不存在了**（定框 `K35` ＋ `K-R59` 整格删除，三条回潮闸钉着）；
- 「没装 ccm 的远端」**存在**，而**产品自带装它的路**（`sftp::deploy_remote_backend` —— 〔MC1〕部署后端那一颗按钮连同 `ccm` 入口一起放，`K27`/`K34`）⇒ 它今天也不是「没路走」，是「还没装」；
- **今天真正撑着那两个文件的是消费者，不是主机类别**：**尺子A**（剥完注释的生产段里那个标识符出现几次）逐处住 `launch_wire_f07_main_path_tests.rs::TS_FALLBACK_KEEPERS`，登记表与现打逐格相同 —— 🔴〔`K-R105` 09-13〕**这里原来抄着两个基数（11 / 7），撤了**：那张表由判据从源码派生，散文抄一份就是第二个家。而**尺子B**（有没有生产调用方）今天也有自己的家 `launch_wire_f07_main_path_tests.rs::TS_FALLBACK_REACH`，同样派生。两把尺子的读数差着一个数量级，下面这一段说的就是尺子B：`remote-launch.ts` 那 5 个 builder **生产调用方是 0**（只有 `tests/e2e/resume-cmd-driver.ts` · `tests/e2e/tmux-target-emit.mts` · `remote-launch.test.ts` 在调）⇒ 〔**步 22b·B 2026-09-20 订正：这半句今天假了 —— 那一行退役了，`renderFallback` 的生产调用方是 0。** 原文续如下，留档：〕**真正让 `renderFallback` 站在生产路上的只有 `remote-launch-run.ts::renderLaunchCommand` 最后那一行**，加上同文件里那处 `SESSION_BACKEND.attach`（把 `↗` 交给用户自己的终端那一跳，`K-R59 §0c` 明写**不做**：后端在远端开不了你面前的窗，那是结构不是退路）。🔴 **`K-R109` 09-13 订正后半句**：那条「明写不做」讲的是**远端**（`R61` 裁定三之后不许再用「后端」把两侧压平）；**本机**就地 resume 那一处后端就在用户面前那台机器上 ⇒ 本轮接过去了，同文件里那处 `SESSION_BACKEND.attach` **不在了**。前半句（`renderLaunchCommand` 最后那一行）**没变**，它今天仍是那条路唯一的生产入口。
- ⚠ 顺带一条**分母订正**：`backend_kill_tests.rs::CREATION_PATHS` **今天是 3 条**（`K-R104` 09-13 把 `account_usage.rs` 那行删了，留着墓碑）⇒ 删得动 `session-backend.ts` 的话是 **3 → 2**，不是「4 → 3」。🔴〔`K-R106` 09-13 复量，量具 `tests/evidence/K-R106-ruler.py`〕**登记 3 条、遍历实得 3 条、两边逐格相同** —— 这个数**别在这里抄第二遍**，它的家是那张表。

🔴 **`K-R106` 2026-09-13 第六次订正 —— 「删不删得掉」这一问的今天版：仍然删不掉，而拦路的换成了一条可以指名道姓的链。**
`R61` 裁定三之后，②「attach 归谁产」那一格**本机那半有承接方了**（`history.rs::render_local_attach`）。
于是「删 `session-backend.ts`」这件事第一次可以问得很具体 —— 现打（量具 `tests/evidence/K-R106-ruler.py`，人群 = `src/**` 去掉 `*.test.ts`/`*.vitest.ts`，剥法与 `launch_wire::production_ts` 同口径）：
- 🔴 **`K-R109` 09-13 订正：这两条今天各只对一半。** 原文写「座今天有**两个**生产消费者文件：`launch-render-fallback.ts` 与 `remote-launch-run.ts`」＋「`remote-launch-run.ts` 那一处差的只是把前端接过去」。**接过去了** —— 那一处现在问 `commands.render_local_attach` 要（注册面三处同一拍落地）⇒ **`remote-launch-run.ts` 不再是座的消费者**。⚠ **数字不在这儿抄**：逐格处数的家是 `launch_wire_f07_main_path_tests.rs::TS_FALLBACK_KEEPERS`，而「有没有多出一个没登记的消费者」由同一条判据的第 ⑤ 格（`K-R109` 加的**反向闭合**）从源码派生 —— 本行只留住址；
- 🔴🔴 **〔步 22b·B 2026-09-20〕这一条被证伪了，而且它错在哪很值得写下来。** 原文逐字：「**`launch-render-fallback.ts` 那一处不是「差接线」，是差一个承接方**：它要座产的是 `container: tmux` 的 **`create` / `send-into` / `attach`** 三格外层 tmux 命令，而它被走到的**前提**恰恰是「后端那条渲染器拒了」（探测 unknown / 没装 ccm / 有维度说不出 CLI 语法）—— **那时按定义就没有后端命令行入口可问**。」<br>⚠ **「按定义就没有」那一步是错的，而且是一个有名字的错**：那句话里的「后端」指的是**远端那台机器上的 ccm**（拒的正是它），而承接方本来可以是、今天就是 **monitor 自己进程里的那份 Rust 渲染器**（`backend::control::payload`，一个 tauri 命令，不经网络、不依赖远端）。⇒ 把两侧压成一个「后端」——**这正是 `R61` 裁定三禁的那一形**（那条裁定立下的纪律是：不许再用那个旧词把「远端常驻的那份」与「后端」压成一个；逐字原文在同一节 ② 那一格里），而本节 ② 那一格早就为它挨过一次（`K-R106` 第六次订正：「那句头注今天仍然对，而它的射程是**远端**」）。**同一个混淆，在同一节里犯了两次。**<br>⇒ 现打：22b·A 补出承接方（`render_tmux_outer` ＋ 13 条入库金标准），22b·B 接线，**「差一个承接方」这句话今天不成立**。
⇒ 🔴 **「删掉座」的代价也换人了。** 原文逐字：「**删掉座 = 把那条兜底路整条删掉**，而那是「没装 ccm 的远端」那一格的事（六格表第 ⑤ 格，`K27`/`K34` 判它是**部署面**），不是 attach 这一格的事。」—— 今天删掉座**不会**动「没装 ccm 的远端」那一格的能力（那一格已经由 Rust 渲染器服务），它动的是**两份入库夹具的左边**：`payload-golden.json` 与 `tmux-outer-golden.json` 的左边都是 TS 的真渲染器 + 真座，删了它们，跨语言逐字节对拍就退化成「Rust 没变」的冻结快照。⇒ **代价从「部署面」变成「独立说法」**，逐字见 `the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold` 的**依据一 c**（本拍新立，两个发生器 ＋ 两份夹具逐个钉在盘上）。
⚠ **两件事别再压平**：`K-R54` 表第 3 行问的是「attach 归谁产」，它今天在本机这一侧**答完了**；而「座能不能删」还压着**另一格**（部署）。
⇒ **结论仍然没变（今天删不得），但理由第五次换人了。** 逐处读数与量法住 `tests/evidence/K-R89-deathvalue.md`；六格今天版住 `src/bridge/src/history.rs::tests::THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`（由 `every_one_of_the_six_cells_is_measured_not_narrated` 逐格**真去驱动**，改了行为不改说法当场红）。
🔴 **〔步 22b·B 2026-09-20〕这一句今天不成立了。** 原文逐字：「**U8c-3 的真前置不是文档，是 U8a-2c 与 U12 两件功能** —— ⚠ **F11 订正：U8a-2c 是「未做完」不是「未做」**（`send-into` 那一格 U8a-2c-1 已交付；剩 `create-or-attach` 与 attach）。」<br>⚠ 它预设的是「TS 那两个渲染器要等 monitor 把 `launch` 帧那条路走通才删得掉」。**`设计/90 §4 E` 走的是另一条**：不经后端的 `launch` 帧，monitor 自己进程里的 Rust 渲染器直接产那条 shell 串（与 CLI 那支 U8c-2c-2 同一种做法）。⇒ **前置已经满足了，而 U8c-3 仍然不做** —— 今天挡它的是**依据一 c**（那两份入库夹具的左边要是「另一种语言的独立说法」），不是功能。⚠ **那不是「等不到」，是「不划算」** —— 权衡逐字见 `launch-render-cli.ts` 那一份的复裁（同一先例，已复裁两次）；真要删，回 `设计/00 §2.5 ④` 结账时一起裁，别顺手删。
⚠ **本节这三处（914 · 925 末 · 本段）与 F07 订正的那一处说的是同一句话。**F07 只订正了手头那一处 ⇒ **同族的三处又活了一轮**。**订正一句假话时，先把它的全部副本找出来**（F01 的四处「每 ~8s」是同一个病）——这条纪律由 `doc_claim_registry` 把可数的那部分变成机检。

### 2026-08-14 第三次复裁（U8c-3-r2）：结论第三次不变，而这次**多了一条硬依据、少了一个假绿的量具**

🔴 **2026-09-13（`K-R105`）：下面这句「没有一条过期到可以放行」是 08-14 的读数，今天不成立。**
现打 **③ 已退役、① 变过一次**（逐条见上面那张表的 `〔现打…〕` 判词，由
`doc_claim_registry::the_three_questions_in_33b_have_todays_answers` 钉着）。
**照抄下面这一句就是把 08-14 的读数当成今天的** —— 那正是本节犯过五次的那个病。

〔以下 08-14 原文留档〕三问逐条重量，**没有一条过期到可以放行**：

| # | 08-14 实测 | 它今天挡的是什么 |
|---|---|---|
| ① | **仍是「部分是」** —— 生产段发 `create-or-attach` **0 处**（`launch_wire.rs` 那条判据在量）；`attach` 结构上不归后端（`control/launch.rs` 头注「本模块**不 attach**，一次都不」） | 起会话的两格（create / attach）没有 Rust 承接方 |
| ② | **仍挡着，而且不止 attach** —— `renderFallback` 的**三格**（tmux `create` / `send-into` / `attach`）全在 TS；`container:"none"` 那格 U8a-2c-pre 已切走，`ccm` 那条 U8c-2c-2 已切走，**剩下的正好就是要外层 tmux 命令的那三格** | 没装 ccm 的远端 |
| ③ | ~~**已决：要**~~ ⇒ 🔴 **2026-09-11 `K-R59` 后：这一问退役了** —— 那一类主机不再存在（定框 `K35`）。**但那条路没退役**：它另有消费者，逐处与处数见 `TS_FALLBACK_KEEPERS`，「还站不站在生产路上」见 `TS_FALLBACK_REACH`（🔴〔`K-R105` 09-13〕这里原来写着两个数，那是尺子A 的读数被当成尺子B 读，已撤） | ~~daemonless 的远端~~ ⇒ 换成「那几个消费者」 |

⚠ **本轮真正修掉的是量具，不是结论。** F07 立的前提触发器里，依据一原式是
`fallback.contains("session-backend") || run.contains("session-backend")` ——
**整份文件的子串、含注释、而且是 `||`**。而 `remote-launch-run.ts` 的注释里逐字提了 4 次
`session-backend.ts`（那些注释干的正是「解释这一格为什么还在 TS」这件事）⇒
**把生产调用点删干净，那条依然绿**。一条**前提触发器**在它被造出来要报的那个方向上是瞎的，
与没有判据是同一件东西。现在量的是「生产段里那个调用还在不在」。

⚠ **「件关掉了」不等于「约束消失了」** —— ③ 这次栽的就是这个。本节前两次栽的是
「结论对所以没人查理由」；这次是「**件关了所以没人查约束**」。
两者的处置相同：**把结论留住，把依据换成还量得准的那个**，并且给它配一条会红的判据。

### 2026-09-13 第四次复裁（`K-R105`）：**结论第四次不变，而这次三问里过期了两问**

前三次复裁每次都以「一条都没过期到可以放行」收尾。**这一次不是。**
逐条现打（量于主干 `38168d7`，量具 `tests/evidence/K-R105-ruler.py`，人群与剥法与
`launch_wire::production_ts` 同口径；判词由 `doc_claim_registry` 现算并与上面那张表对拍）：

| # | 08-14 说的 | 09-13 现打 | 差在哪 |
|---|---|---|---|
| ① | 「仍是『部分是』—— 生产段发 `create-or-attach` **0 处**」 | 承接方在、**而且真在跑**（`control/ccm/` 那棵树在发，后端侧 `Mode::CreateOrAttach` 接着） | 🔴 **那句话在它自己的尺子上今天仍然是对的** —— 它只量了 monitor 那一棵树。`K-P2` `D3`（09-03）翻正的是**另一棵**。⇒ **不是读错了数，是那把尺子够不着这一问**（本工作区最高频的一类错） |
| ② | 「仍挡着，而且不止 attach —— `renderFallback` 的三格全在 TS」 | **仍挡着，而且它今天是唯一的硬障碍** | 三格里 `create` / `send-into` 今天在后端侧**有承接方**（只是 monitor 没接过去），**只有 attach 是结构上没有** |
| ③ | 「已决：要」 | **已退役** | `K35`〔用@09-11〕⇒ `K-R59` 整格删除。这一条 09-11 就该落到本表，**只落进了下面那张 08-14 表的 ③ 行** —— 08-04 这张表上又活了两天 |

⇒ **`U8c-3` 今天仍然删不得，而理由第四次换人，且这次只剩一条**：
**② 那条 attach**。它不是「还没做」，是**要先有一个产品决定**
（monitor 的 `↗` 路要不要一律要求对端 —— 含**本机** —— 装着后端 / ccm）。
`K-R105` 把它交回定框那一侧，**不由判据自裁**。

🔴 **本轮的一般化，与前三次都不同**：前三次是「依据/度量过期而结论仍对」，
处置是「把结论留住、把依据换成还量得准的那个」。**这一次是「一句真话摆错了尺子」** ——
① 那个 `0 处` 与「三个生产消费者」那个 `3` 都**没有量错**，
它们错在**被拿去回答另一把尺子的问题**，而两把尺子的差别没写在数的旁边。
⇒ 处置多一条：**报一个数就要同句写清它的尺子**；两把尺子都要有自己的家
（`TS_FALLBACK_KEEPERS` / `TS_FALLBACK_REACH`，两张都从源码派生），
散文里**一个字面量都不许留**。

⚠ **诚实边界，别把这一节读大**：`doc_claim_registry::the_three_questions_in_33b_have_todays_answers`
钉的是每一问旁边那个 `〔现打…〕` **判词**，不是这几段散文。
一段与判词相符、其余全说反了的答案，它静默。它买到的是
**「三问的答案不会静默地过期」**，不是「答案写得对」。

### 六条不变量各自的命运（U8c-1 逐条判定，别到 U8c-3 才现想）

| 条 | 讲什么 | U8c-1 动了吗 | 后两件会怎么动 |
|---|---|---|---|
| **§33** | 双渲染器；CLI 渲染器表达不了就必须放弃、不许近似 | **没动** | U8c-2 后「两个渲染器」变成「一个 Rust 渲染器 + 一条 ccm 调用形态」；**「表达不了就放弃」这条纪律必须原样继承**，不许因为换了语言就默许近似。U8c-3 改写本条 |
| **§35** | 维度的 `applies` 不许条件性跳过 `cliFlags` 的 `null` 安全网 | **U8c-2c-1 已在 Rust 侧兑现** | `backend::control::ccm_invocation` 里 `cliFlags` 是 `Option<Vec<String>>`，`None` **直接短路成 `Refusal::DimensionCannotSpeak`** —— 「拿不到命令」而不是「渲染出一条丢了修饰的命令」。夹具里有专门一条 refusal 用例钉住它，变异（改成 `Some(vec![])`）当场红。U8c-3 删 TS 那份 |
| **§36** | 本地（Windows）路径**不经 IR** 产出命令 | **没动，而且 U8c-1 刻意维持它** | Windows 分支（`config_dir_prefix_ps` / `validate_config_dir_ps`）一个字节没碰。它要不要并进内核，取决于 `acct-core` 已裁决过的「`\` 与盘符」问题 ⇒ **U4b（真机）或 U8c-3**，登记在案 |
| **§37** | 新维度的 `applies` 该不该恒真，看「沉默」是否等价于用户期望 | **U8c-2c-1 已在 Rust 侧兑现** | `DIMENSION_ORDER` 里 `account` 恒真（沉默 = 意外身份切换）、`model` 条件式（沉默 = 用户期望）；**能力只向已触发的维度收集**，且检查与 flags **逐维度交错**（初版我把检查提到循环外，那会在两种失败同时成立时给出另一个 reason —— 而 reason 是生产侧唯一的降级线索）|
| **§38** | 一条新正交轴进注册表还是做一等字段（三条 checklist） | **没动** | 这条是**设计判据**不是实现，跨语言仍然成立 ⇒ U8c-3 只需把例子里的 TS 符号名换成 Rust 的 |
| **§39** | `WrapSpec` 是纯数据不是闭包 | **已在 Rust 侧兑现**：`backend::control::payload::WrapSpec { order, prelude }` 就是纯数据，Rust 里连闭包这个选项都没给 | U8c-3 删 TS 那份 |

### 变严的代价：它在两种语言之间**开了一条新缝**（U8c-1 如实登记）

`history.rs` 的 POSIX 校验换成 `acct-core` 并集之后，同一个含 `U+3000` 的 configDir：
**本机 Rust 拉起拒绝、远端 TS 拉起放行**（TS `shell-quote.ts::isValidConfigDir` 仍是旧集合）。
迁移前两侧都用旧集合、是一致的。⇒ **这是变严的诚实代价**，U8c-2/U8c-3 收编 TS 时一并收口。

### 跨语言一致性靠什么保住（U8c-1 的核心交付）

**入库夹具 + 两侧各自与它比**，不是注释：

```text
  src/launch-payload-golden.ts（真 renderFallback）
        │ npm run gen:payload-golden
        ▼
  src/bridge/src/backend/control/fixtures/payload-golden.json   ← 入库（P4b 起）
        ▲                                    ▲
        │ launch-payload-golden.vitest.ts    │ launch_payload_parity.rs
        │ 「入库的 == 现场渲染的」            │ 「Rust 渲染的 == 入库的」
```

⚠ **两侧都必须有计数自检**（`MIN_CASES`）：夹具被清空/截断时，「逐条循环」在两种语言里
都会零命中零失败地绿。⚠ **绝不能让 Rust 侧去调 TS 现场生成** —— 那就成了自洽夹具
（U7-4 的病根正是「写侧读侧同一个常量」）。

## 34. tmux 破坏性/半破坏性命令三道门 + 原子 verify+act（F04 / unify-launch / R10）

**背景**：F04 根治 R10——过去 `kill_remote_tmux`/`tmux_send_keys` 只有一道门（`is_ccm_tmux_name`
名字前缀判据），且"查一次状态、再发一条动作命令"是两次独立远端往返，中间留 TOCTOU 窗口。

🔴 **`K-R72`（2026-09-12）：这一节的「两处」今天只剩一处 —— 别照旧读。**
`kill_remote_tmux` / `tmux_send_keys` 的 **C7 过渡期 SSH 回落已经删掉**
（`K-R54` 逐处裁定表第 1 · 2 · 5 处，`K-R56` 先把两条路的「探了没有」补齐才删得掉）。
⇒ 今天 monitor 侧**不拼任何破坏性 tmux 命令串**，那两条命令只走后端通道；
`build_guarded_tmux_cmd` / `build_kill_session_cmd` / `build_send_keys_remote_cmd`  〔散文墓碑〕
连同 `gate_guard_expr` 一并不在盘上了。**下面三道门的实现住址随之只剩一个家**，  〔散文墓碑〕
逐条标在各自那一行。回潮闸：`src/bridge/src/tmux_backend_gate_guard.rs` 的
`the_monitor_has_no_second_path_that_kills_a_session` / `the_front_end_speaks_the_tmux_control_ops_only_through_one_module`
——〔C4e · 第四波 4C〕那两条命令连同 monitor 那一跳迁到界面（`src/tmux-control.ts` 经通道直接说后端的 `kill` / `launch`）之后，
回潮闸改钉「monitor 生产段里一处 `kill-session` 都没有」＋「界面只经那一处说这几条」。

**三道门**（⚠ **F04b 2026-08-04**：`kill` 主路切到后端；**F04c** 切 `send-keys`；
**`K-R72` 2026-09-12**：两条回落删净）：
1. **Gate 1（恒强制）** —— 家在调用方那一侧（〔C4e〕`src/tmux-control.ts::rejectEmptyTarget`，抓屏 · 送键 · 杀会话三条共用；
   monitor 的 `src/bridge/src/backend/control/tmux.rs::gate1_reject_empty` 同一判定，今天只剩跨轨锚点 `exact_target` 在用）：只拒**空** target
   （`=:` 会被 tmux 解析成「当前会话」，是唯一真正危险的默认值）。**不额外收紧字符集**——
   glob/元字符交给 `shell_quote` 安全引号化，字符集收紧是 TS 侧 `isValidNewTmuxName`
   （仅创建路径）/`isValidTmuxName`（attach 故意宽松）的职责，见 §31a「第二道防线」。
   ⚠ **`K-R72` 把这个谓词从 `exact_target` 里分出来（不是复制一份）**：`exact_target` 产的是
   给 shell 用的精确串 `'=<名>:'`，而送键 / 杀会话今天不拼 shell 串 —— 让它们为一次校验去要
   一个用不上的串就是「一个值装了两件事」。〔C4e〕三条路（capture-pane · send-keys · kill）迁到界面之后
   调的是界面那一个谓词、报的是同一句话（文案表 `tmuxControl.target.empty`）。
2. **Gate 2（identity，union）** —— 判定本体的唯一家是 `gate-core`（`gate_singleton_guard`
   钉着「全仓只有一份」）；**执行面今天只在后端** 的 `control/gate.rs::admit`。
   `is_ccm_tmux_name`（本地、零 IO，前缀命中）**或** `@ccm_sid` 已设（远端核验）。**`is_ccm_tmux_name` 不删除**——F02 之前的老 `cc-*` 会话没有 `@ccm_sid`，
   仍必须可 kill/send-keys，否则是向后兼容回归；F02 之后 `--tmux=<自定义名>` 建的会话没有前缀，
   必须靠远端 `@ccm_sid` 核验才放行，不能被一刀切拒绝（那本身是 F02 引入的真实网开一面缺口）。
3. **Gate 3（仅破坏性动作，即 kill）** —— 家在后端 `control/gate.rs::admit_destructive`
   （`K-R72` 之前 monitor 侧那条 shell 串里还有第二份）：远端 `session_windows==1`——拒绝 kill 一个已长出额外
   window 的会话（signal：有独立于本工具的用户活动，不该被这一个 kill 动作连坐端掉）。send-keys
   不删任何东西，不受此门。

**原子 verify+act**：后端的 `control/gate.rs::probe` 一次 `tmux display-message -p -t
<target> '<fmt>'` 取回 `session_id`/`@ccm_sid`/`session_windows` 三个字段，`admit` /
`admit_destructive` 判完之后**对拿回来的 `#{session_id}` 句柄**下命令——不是"先查一次、
再另发一条按名字的动作命令"（那正是 R10/#76 的共同根因：两次远端往返之间的窗口可被抢跑）。
用 `display-message` 而非 `show-options`：后者对未设置的 option 是 `rc=1` + stderr、需要脆弱的
rc/stderr 联合判断；`display-message` 走这个仓库已验证的格式串插值惯例（`TMUX_LS_FMT` 同款），
未设置的 option 静默展开成空串，`session_windows` 恒为存在会话的正整数、天然当"目标是否存在"
的判据（空捕获串 = 目标不存在）。

⚠ **`K-R72` 之前这里有两份 verify+act**：monitor 侧还有一条把 Gate 2 远端半支 + Gate 3 + 动作
折成一条 shell 串的 `build_guarded_tmux_cmd`。两份**不等价**——那条对 `=name:`（名字）下手，
TOCTOU 窗口没关干净；`K-R54` 表第 1 · 2 处据此判「留后端」。今天只剩上面这一份。

⚠ **一条代价，写在这里免得下一个人以为是 bug**：后端通道不在时，送键与杀会话从
「悄悄走另一条路做掉」变成**明确失败**。出口是〔C4e〕界面 `src/tmux-control.ts` 的通道那几句（文案表
`tmuxControl.channel.*`：本机 / 远端两句不同的话，各自说得出下一步）；它会被 `src/account-restart.ts` 原样弹成 toast。

**性能纪律**：Gate 2 本地命中（`cc-*` 前缀）时跳过的是**远端半支的判定**，
**不是存在性探测**（`K-R56` 2026-09-11 订正：那两件事此前被压成了一件，于是
`cc-*` 名的 send-keys 成了全仓唯一一条不探会话就动手的写路径）。今天所有形态都恒先 probe 一次。

**验证**：后端侧的真机验收 `tests/e2e/backend-gate2-acceptance.sh`（真后端二进制 + 真 tmux
server，隔离 `-L` socket，用例逐行来自唯一那张判定表 `gate2-golden.tsv`）。
Rust 单测只锁判定，真机验收锁"真后端收到请求之后在真 tmux 上到底干了什么"
（R1 教训：门禁全绿过仍放行过一个让 send-keys 完全失效的改动，字符串断言测不出真实行为）。
🔴 **`K-R72` 同拍清掉的一颗哑弹，如实写**：monitor 侧那套真机验收
`tests/e2e/tmux-guarded-acceptance.sh`（14 项）的输入源就是那个已被删掉的 builder
⇒ **它今天取不到命令串、跑不起来**。⇒ **整套删了**，连同它在 `tests/e2e/README.md` ·
`package.json` · `.github/workflows/ci.yml`（清单 + 调用步骤）·
`src/bridge/src/shared_crate_registry.rs`（`dormant_e2e_suites_keep_their_assertions` 的棘轮行）·
`src/bridge/src/capability_registry.rs` 五处的登记。
⚠ **不是「三道门少了一层真机验收」**：`backend-gate2-acceptance.sh` 打的是同一张
`gate2-golden.tsv`，而且打在**今天真的那条路**（后端）上；删掉的那套打的是一条已经不存在的路。

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

（**当时**的机制：`NESTED_ENV_RESET_DIMENSION`（issue #24：清 Claude 自己的嵌套会话标记 `CLAUDECODE`/`CLAUDE_CODE_SESSION_ID` 等）的 `applies` 只看 `ctx.action.kind==="new"||"resume"`、不看 `transport`——local 场景走到这里恒真，`plan.env` 会真的被塞进一条 `unset` `EnvOp`，而本地渲染器 `src/bridge/src/history.rs::build_local_ps_command` **故意完全不读**它。这条"注册表对 local 也会产出 env op"的事实**今天依然成立**，只是本地路径不再去调它了——证据见 `tests/launch-requests.vitest.ts` 的「维度注册表在 transport:local 下的行为」那组测试，它直接冲 `buildLaunchPlan` 去验，不借道任何生产函数。）

**为什么不消费是对的**：`NESTED_ENV_RESET_DIMENSION` 保护的攻击面是"tmux **持久 server** 进程的环境表跨多次 resume 累积污染"——远端场景里，同一个 tmux server 可能存活很久，每次新 resume 进去的 shell 都从 server 环境继承，之前一次 `claude` 进程留下的 `CLAUDECODE=1` 等标记会一直挂在那，必须每次显式 `unset`。本地 Windows 场景没有这个"持久 server"概念——`launch_powershell_window`（`src/bridge/src/launch.rs`）每次都是全新 `Command::new("wt.exe"/"powershell.exe").spawn()`，唯一可能的污染源是"cc-monitor.exe 自己被某个带毒环境启动"（如从一个嵌套的 Claude 会话终端里启动 cc-monitor 自身）——这条攻击面已经在**进程启动阶段一次性堵死**：`src/bridge/src/lib.rs::run()` 里 `scrub_env_vars(adapter::active().nested_env_to_scrub())` 是 Tauri `Builder` 构造之前就跑的第一批实质语句，直接 `std::env::remove_var` 清掉 cc-monitor.exe 自己进程的环境；`Command::new(...)` 默认继承（已清洗过的）父进程环境，无需每次 launch 前再清一次。

**铁律**：**给本地渲染器补一段读 `plan.env`、把 `unset` 翻成 PowerShell `Remove-Item Env:\X` 的代码，是错的"修复"**——两层保护本来就分工不同（远端：渲染期逐次清；本地：启动期一次清），本地补一层不会更安全，只会引入一段从未有真机（Windows/`pwsh`）验证过的新 PowerShell 语法，纯增加风险不增加收益。若未来真的发现本地场景存在启动期清洗覆盖不到的污染路径（例如 cc-monitor 在自己生命周期内某处被重新 exec、绕开了 `run()` 的这次清洗），应该去修**启动期清洗本身的覆盖面**，而不是在本地渲染器里加一段渗透式的补丁。

**验证**：`tests/launch-requests.vitest.ts` 的「维度注册表在 transport:local 下的行为」组锁死本地 `LaunchContext` 经 `buildLaunchPlan` 产出的 `plan.env` 对 new/resume 两个动作恒非空（证明维度确实触发了）、account 维度对 base 态是 no-op、`cwd: null` 原样透传（R07 Phase D 审计发现拆分中丢过这条，已补回；它是**共享代码**，远端路径也吃），且 `history.rs` 侧未新增任何消费 `plan.env`/`unset`/`Remove-Item` 的代码路径（Phase D 审计已核对 `scrub_env_vars` 的调用时点严格早于任何窗口 spawn，且全仓无绕开它的自重启路径）。

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

**L2 复测确认（2026-07-30，`local-as-remote`）**：本节铁律**仍然成立**，且 `local-as-remote` 主计划原本的 L2（「PowerShell 渲染器 honour `plan.env`」）**正是它禁止的那件事** ⇒ **已否决，不做**。启动期清洗的时序也实证过：`lib.rs:124` 的 `scrub_env_vars` 早于 `:161` 的 `tauri::Builder`。L2 改做的是「别让本地/远端静默漂移」这个真意图落在真实漂移点上 —— Rust `adapter` ↔ TS `AGENT_PROFILE`（`tests/agent-profile-parity.vitest.ts`）。**那条守卫不违反本铁律**：它只读、不给任何渲染器加读 `plan.env` 的代码。

**P3t 裁定（2026-08-11，`control-parity`）：本节只绑 Windows，别拿它当「本机不许用 CLI 渲染器」。**

实测本节被**转述得越来越宽**：`launch_wire.rs` 的字段注释写成「本机不走 CLI 渲染器（§36），Rust 侧也照样拒」，
`parity_ledger` 两行也写成「本地路径不经 IR 产出命令（§36 + R07）」——**代码就按注释的宽度实现了**（一律拒本机）。
而本节标题后半句就是它的全部内容（「嵌套 env 污染保护已在进程启动期做完，别在本地渲染器里重复实现」），
铁律段逐字禁的是「给本地渲染器补一段读 `plan.env`、把 `unset` 翻成 PowerShell `Remove-Item Env:\X` 的代码」，
论证从头到尾是 Windows 分支（`config_dir_prefix_ps` / `validate_config_dir_ps` / 「`\` 与盘符」）。
⇒ 采信「**代码窄了**」：放行 POSIX 本机走 ccm 调用行渲染器**是在兑现本节的原意**，不是破例。

上面 R07 补充那句「不接是因为**接了也拿不到新东西**」，在 **CLI 渲染器**这一侧已被 P3t-Y2 证伪：
接上去拿到的是 `--tmux`，也就是本机旧路结构上产不出来的**会话容器**。
（那句话对**载荷 IR** 仍然成立 —— `plan.action`/`plan.cwd` 恒等于输入。两者别混着引。）
不放行的代价不是「不够对齐」，是本机产出的是一个**无 tty、无 tmux** 的进程，
`src/doc/IPC-PROTOCOL.md` 逐字：「`stdin` 不接键盘 ⇒ 用户敲进去的字会被脚本吃掉」。

**机检**：`arch_doc_shape_guard::every_citation_of_invariant_36_says_which_platform_it_binds` ——
全树每一处引 `§36` 的**句子**必须在同一句里写出 `Windows`。
引一段文字证明不了「今天的代码就是那个意思」，所以这条裁定配了一条机器能重跑的检查。

**另注（同一审计发现）**：`F06-local-path-ir.md` §1 有一条**已勾 `[x]`** 的 DoD 逐字要求
"从产出的 `LaunchPlan` 取 `action`/`cwd`/`launcher` 三个字段映射回现有 Tauri 调用参数"
——**它从未实现**，且已在同文件 §3.2 被撤回（理由即上述"无信息增量"）。那条勾已就地标注撤回。

**R07 为什么连 `buildLaunchPlan` 那一遍也删了**：初稿保留它并声称是"一道便宜的一致性检查"，
但审计实测该声称**零门禁守护**（删掉整段 ctx 构造 + 调用、只留 `void cwd;` → `tsc` 与
`npm test` 705 全绿；改造前同一变异红 5 条，因为那时返回类型让它在**类型层**承重）。
而它想验的东西 `launch-render-cli.test.ts` 已在验（`ctxOf({transport:{kind:"local"}})` → `buildLaunchPlan`；
〔LR1〕那份套件已删，这一格今天由 `tests/launch-requests.vitest.ts`「维度注册表在 transport:local 下的行为」管）。
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
这条降级有专门的端到端测试锁定（`launch-render-cli.test.ts` 的两条 `modelOverride` 用例；
〔LR1〕那份套件已删，今天由 `ccm_invocation_tests.rs::model_dimension_is_conditional_by_design` 与夹具
「已触发的 model 维度要的能力缺失」那条用例管），
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
`src/backend` 的 `--resolve` RPC，完全不经过 `LaunchPlan`/`ccm` 管线——agent 轴未来
真正的落点很可能根本不在 `LaunchDimension` 这个接口体系里，现在塞进去是给自己挖了一个将来要
迁移出去的坑）。

**R12 风险登记状态**：本轮决策**不是**"root cause fixed"，而是"open → accepted with
documented rationale"——三条轴两种机制的不对称依然存在，但现在有据可查，不再是每次重新审视
的开放问题。MASTERPLAN §6 R12 行照此措辞。

**给 UI 层"枚举可用修饰"的启示**：即便 `account`/`model` 已注册进 `LAUNCH_DIMENSIONS`，
`LaunchDimension` 接口本身也从未回答过"这个维度当前有哪些可选值"——`ACCOUNT_DIMENSION`
能在 UI 上显示成列表，靠的是 `src/account-reads.ts::fetchAccounts`/`isSelectable` 现查，不是遍历
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
2. 在 `src/` 或 `src/bridge/` 对应模块的 doc comment 里加引用 `// 违反此约束见 `src/doc/INVARIANTS.md` § N`
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

- `src/bridge/src/history.rs:930` `build_local_ps_command` **不引用任何 IR 类型**
  （`grep -n "LaunchPlan\|launch_plan" src/bridge/src/history.rs` 为空）
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
- **后端零改**：本条只涉及启动路径，不涉及会话监视。
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
- **后端部署 / 版本协商** —— 〔CF1 · 2026-09-24 订正〕原话「本地会话由 monitor 直接读 jsonl，不需要后端」今天不成立：本机会话内容就是本机后端的 `line` 帧。真正的不对称只剩「本机那份后端不经一条 IPC 命令部署」（宿主启动时自己释放），账在 `parity_ledger` 的 `backend.deploy` 那一行
- **多地址故障切换（happy-eyeballs 竞速）** —— 本地没有地址

**机制（让这条原则有牙，不只是一句好话）**

单靠人记不住。落地要求一条**钉死的对账表 + 计数自检**，形状照 `config_surface.rs` 的
`every_host_declaration_is_pinned`（T02 建立）：**枚举全部 Tauri 命令，每条要么两侧都有、
要么在白名单表里且带理由；新增命令不登记就红。** 具体做法归 `local-as-remote` 工作区 **L5**。

## 41. 后端的判活信号全部由内核事件驱动 —— 四路事件、零定时器、四个盲区如实分类（zero-poll-liveness P0-P7）

用户 2026-07-29 原话「我要把轮询杀掉」。后端里原有 **A/B 两条**轮询（2s 判活 tick + 8s
`tmux ls` tick），**两条都已删除**，生产段现在零定时器（`no_timer_guard.rs` 钉住，见下）。

### 41.1 四路事件（延迟均为真机实测，标明测法）

| 场景 | 事件源 | 实测延迟 | 谁发 |
|---|---|---|---|
| claude 进程退出 / 被强杀 | pidfile inotify **+ `pidfd`**（绑进程实例本身） | **~18ms**（P2 端到端） | `WatchEvent::PidDied` |
| 杀掉某 origin **仅剩的**会话（server 随之退出） | tmux server 的 `pidfd` | **27ms**；跨 cgroup 整锅 SIGKILL **30ms** | `WatchEvent::TmuxServerGone` |
| server 复活 | socket **所在目录**的 inotify `IN_CREATE` | **153ms**（含 `DEBOUNCE_MS` 100ms） | `WatchEvent::TmuxObserved` |
| **多个会话里杀掉其中一个** | tmux `session-created/closed/renamed[<槽>]` hook → `--tmux-notify` → SIGUSR1（〔HX2 · D-b〕槽在段 `[50, 100)` 里**每个后端实例一格**、起时清死槽；从前是固定 `[50]`） | **126ms**（**对照组：拆掉 hook = 5042ms**） | `WatchEvent::Poke` |

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
> CI 里 `graylight-backend-frames` 报的那个毫秒数是**上界**（`wait_line` 0.5s 轮询 ⇒ 粒度 500ms），
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
| ③ | user manager / 整台机器挂掉 | 机器内部无解，靠 monitor **断连自愈**（既有路径：重连后后端重发 added + 重放行 → un-archive） |
| ④ | **已存在的会话上 `@ccm_sid` 变了**（`/branch`、`/clear`：claude 进程不重启、tmux 会话不动，只是换了个 sid） | **S0 解决，但换了条路**：这个盲区**四路事件一个都不响** —— 会话没建、没关、没改名，socket 没动，进程没死。P5 删掉 8s ticker 之前，是那个 ticker 在兜它；删掉之后它变成**永久**盲区，表现为用户实测的「/branch 后原 tab 永久灰点、杀不掉」。**修法不是补一条事件路径**（`@ccm_sid` 由 `shared/ccm` 的 1 秒 poller 回填，任何"变化后立刻探"都会撞进那 1s 窗口），而是**让 monitor 不再需要这份快照**：后端在 `session_removed` 帧上带 `cause=superseded` 明说"这个 sid 是被顶替不是死了"，monitor 直接归档。见 §24bis |

**另有一条既有盲区（不是本区引入）**：`notify-debouncer-mini` 静默吞掉 inotify 队列溢出
（`add_event` 只读 `event.paths`，而溢出事件 `paths` 为空）⇒ 溢出时事件永久丢失。
两端都中招，登记为 BACKLOG **E39**。**`pidfd` 对溢出免疫**（内核直通）⇒ 本工作区让情况变好。
**绝不为它补定时器** —— 那等于零轮询造假。

### 41.4 红线：绝不为了让守卫变绿而删掉唯一信号源

`no_timer_guard.rs`（后端 crate，全在 `#[cfg(test)]` 内）扫**全 crate 生产段**
（**2026-08-01 U-1 之前这句话是假的**，见下面第 3 条派生纪律）：

- **判据落在「周期性唤醒」，不落在「出现过 `Duration`」** —— `Duration` 有大量非定时器的正当
  用途（去抖窗口、超时上限）。禁的是 `thread::sleep` / `time::sleep` / `recv_timeout` /
  `time::interval` / `Instant::now` / `Duration::from_secs` 这类**会让线程自己醒来**的构件。
- **非定时器的 `Duration` 用途逐条登记带理由**，处数必须**恰好等于**登记表条数 ⇒
  用 `from_millis(8000)` 偷渡节拍也会红。**登记表不是豁免清单。**
- 范围**只钉后端生产段**：monitor 侧有 UI 刷新、重连退避等正当周期行为，扩过去会变成噪音。

**三条派生纪律**（都是实测踩出来的）：

1. **后端源码的散文里不许逐字引用守卫的禁用模式** —— `readonly_guard`（铁律 I7，见 §41.6）
   与 `no_timer_guard` 都连注释一起扫，是 fail-closed 的设计。**改措辞，不许为自己方便去改红线守卫。**
   （2026-07-31 G2 又栽了一次：新写的 `fork_write.rs` 头注里列了那几个禁用函数名，
   护栏第一次跑就把这个白名单模块自己判成违规。已改措辞并在该文件里写明「别改护栏」。）
2. **删掉一个周期性信号时，光跑单元 + 集成门禁不够** —— 还要跑依赖那个节拍的 e2e。
   P5 删 8s ticker 时漏了这一步，`graylight-backend-frames` 第 2 段（它靠 ticker 重发快照）
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
     ⇒ **护栏的公共机件必须收敛**，否则「已修好的教训」会在隔壁文件里原样复发。
     **进度（U8a-2a，2026-08-02）**：剥法已从后端私有的 `guard_support.rs` 搬进
     **共享 crate `src/bridge/crates/guard-core`**（后端的 `guard_support.rs` 降为
     `pub(crate) use` 再导出；monitor 侧同为 `[dev-dependencies]`）——
     搬家的直接动因是 monitor 够不着它，于是那边的守卫各写便宜近似
     （`src.split("\n#[cfg(test)]").next()`），在 `ssh_source.rs` 上会把扫描面砍掉三分之二。
     **遍历收敛了一份**：`guard_core::assert_tree_strips_clean` 两侧共用（后端
     `every_backend_file_strips_clean` + monitor `every_monitor_file_strips_clean`，
     两条的 `min_files` 地板同时棘到实测值 34 / 52）。
     **仍未收敛**：后端侧 6 处 `read_dir`（`readonly_guard`×2 / `no_timer_guard`×2 /
     `layering_guard`×2 / `protocol_doc_guard`×1）+ monitor `parity_ledger.rs` 1 处，登记 U1a。
     顺带订正一条：`cfg_is_test_only` 现在认**复合 cfg**（`#[cfg(all(test, target_os = "linux"))]`）——
     只认逐字 `#[cfg(test)]` 时，`session_map.rs` 当年那个 Linux 判活测试模块（〔LOC1b〕随 monitor 自己那份判活删了）的 5 个 `#[test]`
     一直留在「生产段」里，是新加的 monitor 全树自检第一次跑就逮出来的。
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

### 41.6 后端的写盘边界（铁律 I7，2026-07-31 G2 起收窄）

**原措辞**：「daemon 对被观测文件系统必须只读，绝不写。」

**现措辞**：**后端不许改动用户既有数据；新增文件须 `O_EXCL` 且限于白名单模块。**

**为什么改**：那条铁律的真实意图从来不是「不许碰文件系统」，而是「不许改动既有数据」。
此前后端一个字都不用写，于是用「全面禁写」近似它 —— 够用且实现简单。
`--fork-session`（远端分叉，几十 MB 的 jsonl 不该为了分叉拉过 ssh）要的能力恰好落在
这个近似的**误差**里：用 `O_EXCL` 新建一个此前不存在的文件，不修改、不覆盖、不删除任何既有文件。

**护栏因此分两层，整体比原来更强**（`readonly_guard.rs`）：

| 层 | 范围 | 判据 |
|---|---|---|
| 默认层 | 除白名单外**所有** 后端生产源码 | 原来那 11 条写模式一条都不许出现（未变） |
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

**为什么原来更弱**：原护栏对「后端将来要写盘」没有任何设计，一旦有人要写就只能**整条删掉**。
现在写的能力被钉死在一个可审计的洞里，洞口还额外挡住了截断 / 追加 / 改名 / 删除。

**必需 token 带前导点是有意的**：护栏是子串扫描、**不剥注释**。只要求裸 `create_new(true)` 的话，
模块文档里那句「`create_new(true)` = O_EXCL」就能把要求喂饱 —— 实测过（G2 的 N5 变异）：
把代码换成 `.create(true)` 之后那条要求**照样通过**，只有行为测试红。带上点就只能由**调用**满足。

> **〔订正 · F1 · 2026-09-24 · 用户裁「现在只允许后端的文件管理部分写文件」＋「文件管理器可以改 `~/.claude` 里的东西」〕**
> 上面的「现措辞」与两层表**已不是全貌**：步 23b（09-19）起白名单层多了 `control/files_write.rs`，
> F1（09-24）又把它移到**第三层**。今天的护栏是**三层**（`readonly_guard.rs`）：
>
> | 层 | 范围 | 判据 |
> |---|---|---|
> | 默认层 | 除下面两层外所有后端生产源码 | 写模式一条都不许出现（未变） |
> | 白名单层 | `control/fork_write.rs` | 只许 `O_EXCL` 新建（同上表） |
> | **第三层（文件管理写面）** | **恰好两个模块**：`control/files_write.rs`（写面）· `control/files_commit.rs`（上传的提交，F7c 09-24：SFTP 只写暂存区 `~/.cc-monitor/staging/`，挪进用户目标的那一下在这里） | 改动动词是**闭集**（建目录 · 删文件 · 删空目录 · 改名 · 改权限 · 覆盖写；〔FW5〕递归删不是新动词，由计划趟逐条目过围栏 ＋ 删文件 / 删空目录拼成）· **每一处改动之前先过围栏** · **只从文件管理面来**（后端里引用得到这两个模块的文件 == `{inbound.rs}`，够得到它们的命令 == 两张命令表登记的那几条之并）。〔RW1 · 第四波 09-24〕**用户文件的读改写**也在这一层：`files-peek`（读那一半，与写同一道围栏）· `files-put`（CAS ＋ 备份 ＋ 原子替换 ＋ 回读 ＋ 回滚，由 `O_EXCL` 新建 ＋ 写 ＋ 改名 ＋ 改权限拼成，**闭集一个动词没加**，规则见 §4）；以及**会话文件围栏唯一的例外** `files-delete-session`（见下一段） |
>
> ⇒ 本节的**措辞**因此改为（〔FN1 · V119〕这句已再改，见下面「订正 · FN1」那一段）：**后端只有文件管理那一面可以改动用户的文件，且每一处先过会话文件围栏；
> 其余后端代码仍不许写，`fork_write` 仍只许 `O_EXCL` 新建。**
> 围栏（`is_session_record_file`，〔FN1〕今天已不是写侧围栏，见下面「订正 · FN1」那一段）只拦正在用的会话记录（`projects/<proj>/<sid>.jsonl`、`sessions/<x>.json`），
> `~/.claude` 里的 skills / 配置 / 账号库**可以改**——这是用户那条裁决的原意，不是放松。
> ⚠ TOCTOU 未闭合（判定与动手之间有窗；改名「目标已在就拒」是先看再改）—— 如实登记，不当成已解。
>
> **〔RW1 · 第四波 2026-09-24 · 用户裁「只允许后端的文件管理部分写文件」**只管用户的文件**、**也管本机**〕**
> ① **monitor 进程不再直接写用户文件**：本机与远端的 rc / `$PROFILE` / 别名文件 / `.mcp.json` / skill 收件箱 /
> `<claude_dir>/skills/cc-bus/` 都经那台机器后端的这一层写（`files-peek` 读 → monitor 算 → `files-put` 带 `expect` 写），
> 写的规则只有 `files_write::put_text` 这一份（§4）。
> ② **会话文件围栏唯一的例外**：`files-delete-session`（删历史会话，用户显式点删、二次确认）。
> 它**只收 sid**（多给一个键 ⇒ `bad_args`），落点由适配层按 sid 在后端自己的记录树里找
> （`agents::claudecode::paths::session_file_for_delete`：解到底必须恰是 `projects/<项目>/<sid>.jsonl`、链接出界不跟），
> 删之前过它**自己的**围栏 `fenced_session_file`（必须是会话形状、文件名恰是 `<sid>.jsonl`）。
> 这根围栏针在后端生产树里**恰好一处调用**（`readonly_guard::the_session_file_exception_lives_in_exactly_one_place`：
> 一处调用 · 定位器只写面引用 · `args == ["sid"]` · 多给 `path` 真的回 `bad_args`）—— 例外借给第二个函数当场红。
> ⇒ 本节措辞再收一句：**用户的文件只有文件管理那一面能改，每一处先过会话文件围栏；唯一的例外是删历史会话那一条，
> 它只收 sid、只删恰是那一形的那一份。**（〔FN1 · V119〕这句已再改，见下面「订正 · FN1」那一段）

> **〔订正 · FN1 · 第四波 4C · 2026-09-25 · 用户 V119「文件管理器全部都可以改. 不需要任何围栏」〕第三层拿掉会话文件围栏。**
> 文件管理器（后端文件管理写面 ＋ 文件窗口）**不再有任何 Claude 数据围栏**：会话文件（`projects/<proj>/<sid>.jsonl` · `sessions/<x>.json`）、
> 项目目录、subagent 记录、tasks 都能新建 · 改名 · 删 · 改权限 · 覆盖 · 复制 · 读改写；递归删不再因树里藏着一份会话文件就整趟拒。
> **拿掉的是「不许改这些东西」的限制；保留的是路径解析的正确性**（`files_write::resolve_in_root`：`rel` 逐段只许普通段、拼出来仍在 `root` 下 ＋
> 父目录解链接后仍在根下；跟链接的动词用 `resolve_existing_in_root`：解到底仍在根下）—— 那不限制改什么，只保证改的就是 `root ＋ rel` 那一格。
>
> ⇒ 第三层那一格的判据 ③ 改成：**每一处改动之前先过路径解析**（针 `readonly_guard::RESOLVE_CALLS`，形状一个字没变）。
> ⇒ 本节措辞今天读成：**用户的文件只有文件管理那一面能改，每一处先过路径解析；那一面不问数据是谁的。**
> 删历史会话（`files-delete-session`）不是文件管理器，它那一道（只收 sid、必须**是**一份会话记录、`fenced_session_file` 恰好一处调用）一个字节没动；
> 「唯一的例外」这个说法作废（没有被它例外的围栏了）。
>
> | 判据 | 钉什么 |
> |---|---|
> | `readonly_guard::the_file_manager_face_never_asks_the_session_shape` | 后端生产树里问会话形状（`is_session_record_*`）的 `(文件, 函数)` == 删会话那一条要的三处（两向，带合成正控）；写面哪个函数再伸手问 ⇒ 红 |
> | `claude_data_fence_tests::the_file_manager_no_longer_asks_the_fence_and_only_the_inbox_editor_does` | monitor 侧调那道判定的生产文件 == `{skill_host.rs}`（F03b 收件箱纵深，不是文件管理器）；文件窗口 / 传输台零命中 |
> | `files_write_tests`（行为） | 会话文件那一侧逐动词**做得成**、盘上逐字节核；阴性换成「链接指到根外 ⇒ `refused`、根外一个字节没动」 |
> | `filewin::shell_tests::a_real_click_on_delete_walks_the_whole_chain_even_on_a_session_file` | 真点会话文件那一行的「删除」⇒ 问一次 ⇒ 答做 ⇒ 后端真收到 `files-delete` |
>
> ⚠ 后端那份会话形状判定改名 `is_session_record_file` / `is_session_record_path`（「protected」在后端从此是假的），与桥那一侧仍逐字节相同。

> **〔订正 · B2 · 2026-09-24 · 用户裁「只允许后端的文件管理部分写文件」管的是**用户的文件**〕** 第四层：
> | 层 | 范围 | 判据 |
> |---|---|---|
> | **第四层（后端自有状态文件）** | **按文件登记**（`readonly_guard` 第四层表，相等断言）。今天三份：`control/exit_policy.rs` 只写 `~/.cc-monitor/backend.json`（B2）· `accounts/upstream/file_face.rs` 只写上游选择那份凭据文件（〔RM1a〕远端那台由它写）· `asset_catalog.rs` 只写 `~/.cc-monitor/assets-catalog.json`（〔AS2〕资产目录，V113） | 动词只许建目录 · `O_EXCL` 临时文件 ＋ 原子改名 · 失败删自己的临时文件；写入口只从 `inbound.rs` 进 |
>
> ⇒ 上面那段措辞改读成：**用户的文件只有文件管理那一面能改；后端自己的状态文件另立一档、恰好一份。** 两档互不借用。

> **〔订正 · SR1b · 2026-09-24 · 用户 V89「SFTP 怎么进单一常驻后端」选「进本机常驻后端，只写暂存区」〕远端那一半改写。**
> 此前 `readonly_guard::remote_write_layer` 判「后端生产段**一处远端写都没有**」：它把「在 SSH 连接上请求 sftp 子系统」
> 判作远端文件传输能力（`KU31`「远端 rc 能不能替用户写」没裁）⇒ SFTP 只能留在界面进程（`inproc_dial.rs`）。
> V89 裁了：SFTP 连接由本机常驻后端管、与其它 SSH 复用；**只往远端暂存区写**，落进用户目录仍只经远端后端 `files-commit-upload`；
> F08 自部署（后端还不在时只能靠它放上去）一起进本机后端。⇒ 远端那一半的措辞改成：
> **本机常驻后端可以请求 sftp 子系统，只许往远端 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`（部署）写** —— 两处都是我们自己的目录，
> 不是用户数据；`KU31` 那一问（替用户写远端 rc）**仍然是「不」**：别名块 / `.mcp.json` / 删会话走远端后端的文件管理面（RW1）。
>
> | 判据（`readonly_guard::remote_write_layer`） | 钉什么 |
> |---|---|
> | `the_remote_write_lives_in_exactly_one_file_and_its_roots_are_exactly_staging_and_bin` | 命中远端写能力网 / 动词网的后端文件 == `{dial/sftp.rs}`（两向）；它声明的写根 == `{.cc-monitor/staging, .cc-monitor/bin}`（期望取自 V89 题面，异源） |
> | `every_remote_mutation_in_that_file_is_fenced_first` | 那一份里每个含远端改动（协议改动动词 ∪ 开写标志）的函数，第一个改动之前先有 `fenced_remote(`（形状照第三层 ③） |
> | `dial_sftp_tests`（行为） | 合成 SFTP 服务端逐条记改动路径：一趟部署 ＋ 暂存区写跑完，改动落在的根 **== 两处**；越界四形（rc · `~/.local/bin` · `/etc` · `..`）一律 `fenced`、改动表零增长；根底下一条指到根外的目录链接照拒 |
>
> 围栏（`dial/sftp.rs::fenced_remote`）：绝对路径归一成相对 SFTP 起始目录；词法只许两个根（建目录另放行根自己与唯一的祖先 `.cc-monitor`）；
> 父目录 `realpath` 之后仍在那个根的 `realpath` 之下；开写之前 `lstat` 拒链接。⚠ TOCTOU 同第三层，未闭合，如实登记。
> 远端 exec 那个洞（`channel_open_session` ＋ 一条会写的 shell 命令）照旧是本层**认不出**的一格（`the_remote_exec_hole_is_a_registered_counterexample_and_still_passes`）。

### 41.5 兼容与部署

- **wire 两处 additive，`PROTO_VERSION` 不 bump**：`TmuxSessions` 加 `observation`
  （有会话时**省略** ⇒ `raw` 载荷逐字节不变）；新帧 `TmuxSessionClosed { name }` 进 `EMITS`。
  旧 monitor 遇未知 kind 走 `warn` 后跳过（`unknown_kind_returns_none` 钉住），行为退回
  「快照 + miss 计数」。
- **`BUILD_ID` 必须 bump**（现 `p1r-event-liveness`）：不 bump ⇒ 旧后端报同一个 id
  ⇒ 不被判 stale ⇒ 不自动重装 ⇒ **整轮改动在已部署的远端休眠**。
  单一事实源：`build.rs::emit_backend_build_id` 从后端源码抠出，emit 成 monitor 的
  `EXPECTED_BACKEND_BUILD_ID`。
- **死亡帧只带 name、不带 sid**：`#{@ccm_sid}` 在 hook 上下文会解析到**别的会话**
  （P0 实测；照直觉写会把活着的会话变灰）。name→sid 的映射 monitor 本来就有
  （最新那份 `tmux ls` 原文）⇒ **让知道的人去查，比让不知道的人硬传更稳。**
- **`RETIRE_MISS_THRESHOLD >= 2` 与快照对账路径一字未动** —— 死亡帧是**绕过** miss 计数的
  快路径，不是替换兜底。查不到 sid 时（never-bound / 快照还没到）**不猜**，交回兜底路。

---

## 42. `src/doc/IPC-PROTOCOL.md` 是**权威契约**，代码与它不许漂（`U6a` 升格 · 用户 2026-09-22 拍板）

那份文档是 backend↔monitor（以及 aterm）之间的线上契约，而**它的读者在仓外** ——
照它写的客户端拿到的必须就是线上真有的东西。

**性质**：`wire.rs` 里每个 serde 字段名、每条子命令、每条入方向命令，
都必须在那份文档里**落进它该落的那一节**（不是「全文出现过就行」）。

**为什么不能松动**：漂开的后果不是「文档旧了」，是**照文档写的客户端静默读错**。
首跑机检的现打（不是手工估）：**7 个线上字段**从没进过文档、**2 个子命令**全文零出现，
还有一处比漏写更糟 —— 文档里叫 `classification`，而线上真名是 `observation`
⇒ 照文档写的客户端**永远读到 `None`**、退回「保守跳过」，
正是那个字段当初要修的 idle 灰灯。
⚠ 「这次补齐」不解决问题：这些本来也都是一条条加进代码时忘了同步文档的。
**没有机检，补完就会重新开始漂。**

**谁在守**：`tests/backend/protocol_doc_guard.rs`（13 条）。

**射程（〔JA1 · 第四波〕按标题「代码与它不许漂」读宽，主会话 2026-09-24 定）**：不只字段名与命令名落进哪一节 ——
那份文档里的**行为句**（一条命令收什么、回什么、拒什么、失败时说什么、帧何时发）同样是契约。
`protocol_doc_guard` 只机检名字那一半；行为那一半由各命令 / 帧自己那一族行为判据守
（链路四条 · 凭据读写 · 搜索 · 足迹 · 插件 · 任务 · 会话快照 · 线上帧 ⋯⋯ 这些族的头注以本节为主住址，经它落到 IPC 那一节）。
⚠ 行为句与判据之间没有机检的对拍 —— 行为句改了、判据没跟 ⇒ 靠头注里那条住址让人找得到。

🔴 **为什么它要升格成条**（2026-09-22 `P20` 现打）：在此之前这条性质**只有一个工单号**
（`U6a`，2026-08-02），`INVARIANTS` 里一条都没有。
**工单号与条不是一回事** —— 工单是「我们那次做了这件事」，条是「这件事必须一直成立」。
一个只有工单号的性质，工单关掉之后就没人替它说话；
而那 13 条判据因此**点不到任何要求住址**（`设计/99 §4.10.1`）。

---

## 43. **monitor 侧**的周期性唤醒也要逐条登记 —— 它与 `§41.4` 的范围差写在这里（`P20` 升格 · 用户 2026-09-22 拍板）

**性质**：monitor 生产段里每一处**会让线程自己醒来**的构件，都要在
`rust_timer_registry` 上登记，并写明它属哪一类（ticker / wait-for-condition /
throttle / startup-delay），`ticker` 还要写明事件源与退役归属。

🔴 **它与 `§41.4` 的关系必须写清，否则这一条会被读成那一条的扩张**：
`§41.4` 逐字划了范围 ——

> 范围**只钉后端生产段**：monitor 侧有 UI 刷新、重连退避等正当周期行为，扩过去会变成噪音。

⇒ **那一条明说不管 monitor，而本条管。** 两条**不是同一件事**：
- `§41.4` 的正题是「**后端零定时器**」—— 它禁的是那一族构件本身（判活必须由内核事件驱动）。
- 本条的正题是「**monitor 侧的节拍不许无声无息地长出来**」—— 它**不禁**周期行为
  （UI 刷新、重连退避都是正当的），它要的是**每一处都说得出自己是什么、谁退役它**。

**为什么不能松动**：那张表的判准不是「有没有 `Duration`」而是「会不会自己醒来」，
因为 `Duration` 有大量正当用途（去抖、超时上限）。而没有这条登记，
「用 `from_millis(8000)` 偷渡一个节拍」在 monitor 侧会**一条都发不出来** ——
而 monitor 是那个**跑在用户机器上、一直开着**的进程，一个偷渡进来的节拍
就是一份永久的电与 CPU。

**谁在守**：`tests/bridge/rust_timer_registry_tests.rs`（两向相等：盘上现打的处数
== 登记表条数）。

⚠ 升格的来历（`设计/99 §4.10.2`）：这张表**一直在管**，却点不到任何要求
—— 而 2026-09-22 它**还在长**（当天加了 `filewin/shell.rs` 那个 10ms 轮询）
⇒ 那是一个活着的缺口，不是历史遗留。

---

## 44. **monitor 侧**起进程的面也要逐条登记 —— 而它**不是**「防写盘」（`P20` 升格 · 用户 2026-09-22 拍板）

**性质**：monitor 生产段里每一处起进程的地方都要在 `exec_site_registry` 上登记，
并写明**它起的是什么、谁是它的唯一出口**。

🔴 **它与 `§41.6` D1 那条附加条件的差别，是这一条存在的全部理由**：
那一条逐字要求「起进程的面**逐条登记**…写明『做什么、**为什么不违反收窄后的铁律**』」，
而它是**后端**那条只读铁律收窄时的强制条件 —— 主语是后端。

而 **monitor 侧起进程不违反任何铁律**（monitor 本来就能写用户的东西）
⇒ 那个「为什么不违反铁律」的理由栏**对这一侧没有意义**。

⇒ 本条要的是**另一件事**：**起进程这件事不许悄悄扩散，而且每一族只许有一个出口。**
它防的不是写盘，是：
- **第二条出路** —— 同一件事（起 `ccm` / 起终端 / 起 agent）出现两处起法，
  两处的参数拼装、环境擦洗、错误归因**一定会漂**（`D1`）；
- **没人认领的子进程** —— 起了之后谁收它、它挂了算谁的，登记表要求当场答。

**为什么不能松动**：起进程是 monitor **对用户机器影响最大**的那一族动作
（它能起任何东西），而「多一处起法」在代码审查里**看不出来** ——
它长得跟正常调用一模一样，只有一张两向相等的表看得见。

**谁在守**：`tests/bridge/exec_site_registry_tests.rs`。

⚠ 升格的来历见 `设计/99 §4.10.3`。⚠ 本条**不放宽**任何既有红线：
它是一条**新增的登记要求**，与 `§41.6` 那条铁律正交（一条管后端不许写，
一条管 monitor 起进程不许扩散）。

---

## 45. webview 的权限清单**默认拒绝** —— 它是「前端碰不到机器」那一族登记表的共同前提（`99 §4.23` ④ 升格 · 用户 2026-09-24 拍板）

**性质**：webview 拿得到的 Tauri 权限**只许是登记过的那一批**，清单里多一条没登记的就红；
同时 webview 里跑的代码只许是我们自己的（`withGlobalTauri` 关着、CSP 不放开脚本执行面）。

**它守的是一个前提，不是一处实现**：本仓那三张「谁能碰这台机器」的登记表 ——
写盘（`write_site_registry`）· 远端执行（`exec_site_registry`）· 本机起进程（`write_site_registry` 的 `spawn_sites`）——
**都只扫 Rust 源码**。它们共享一个此前没写下来的前提：**前端碰不到机器，只能 `invoke` 我们自己的命令。**
往 webview 的清单里加一条 `fs:*` / `shell:*`，或者放开脚本执行面让注入的会话文本能 `invoke`，
webview 就**同时绕过上面每一张表** —— 而那三张表一条都不会红，因为它们根本不看这一侧。
⇒ 本条是那三张表的**共同前提**：它不成立，那三张表的绿就不再说明任何事。
（本产品渲染的是**不受信的会话文本**：Claude 的输出、远端 `capture-pane` —— 脚本执行面放开的代价不是理论。）

**谁在守**：`capability_registry_tests.rs::every_webview_permission_is_registered`（清单 ↔ 登记表**两向**：
没登记的权限 · 登记了而清单里已没有的死行 · 继承这套权限的窗口模式逐字相等）＋
`capability_registry_tests.rs::the_webview_execution_surface_stays_closed`（`withGlobalTauri` 为 `false` ·
CSP 兜底源是 `'self'` · 脚本执行面的几种放开形逐个禁 ＋ 那条刻意允许的样式豁免必须在场，免得禁词在一份被整个换掉的 CSP 上空转）。
`capability_registry_tests.rs::tauri_loads_capabilities_from_exactly_one_source`（**能力只有一个来源**：
`capabilities/` 目录的文件集两向等于 `{default.json}` · 主配置与 sidecar 配置里 `capabilities` 键零处 ·
会被合并进主配置的平台配置文件零存在 · 生产源码零运行期加能力 —— 没有它，前两条只守住了四个来源里的一个）。
同一个文件里的 `the_build_time_execution_surface_stays_registered` 管的是**构建／安装期**的执行面，与本条是邻居、不是本条。

**违反过几次**：**真违反 0 次**（清单今天干净）。但**「没人守」被量到过两次**，都在 08-08：
往清单加一条权限，monitor 与前端两侧的全部测试**全绿**；把 CSP 放开成 `default-src 'self' 'unsafe-inline' *`
并打开 `withGlobalTauri`，**同样全绿**。两条判据就是那天立的；本条把它们从「一张没有要求背书的表」升成红线
（缺条的原账：`设计/99 §4.11.2` 甲类 ④）。

⚠ **它买不到的，如实登记**：
- **只认得今天现打到的四个来源**（`tauri-build` 默认目录模式 · `app.security.capabilities` · 平台配置合并 · 运行期加能力）。
  上游再开第五条路（或 `build.rs` 改用自定义的能力目录模式），本条要跟着加一格 —— 它不会自己知道。
  〔升格当天这一格原是个洞：判据只读 `default.json` 一个文件，另放一份能力文件一条都不红；同日补上。〕
- **不判一条已登记的权限本身危不危险**（那要读语义）；`core:default` 在上游展开成了什么也不判 ——
  上游升级时由「默认拒绝」在登记表那里当场提问。
- 文件管理器窗口是**独立进程、不是 webview**，拿不到这份清单 ⇒ 不在本条射程里；它碰得到什么由它自己那一族判据管，不由本条背书。

---

## 46. 每一套检查**要么进门禁，要么登记为什么不进** —— 人群从盘上全集派生，默认拒绝（`99 §4.23` ⑤ 升格 · 主会话判 2026-09-24）

**性质**：仓里写好的每一套检查 —— e2e 套件、shell 脚本的 lint、共享 crate 的测试、CI 里的每一步、
每一条 `#[ignore]` 的判据 —— 都必须落进下面两格之一：**进门禁**（有具体的一步真的跑它），
或者**登记在册并写清为什么进不了**（豁免行要写理由，而且豁免行不许变成死行）。
「有人想起来就跑」不是第三种状态。

**为什么不能松动**：一套**不在执行链上**的检查，与**不存在**没有区别 —— 而它比不存在更糟，
因为它的存在会让人以为那件事有人在守。这一形在本仓**反复**出现，而且每一次都是**人**发现的，
没有一次是被判据红出来的（逐条见下）。

**与 `设计/01 §5 D5` 的关系**：`D5`（「判据的人群要从文件系统全集来，不从配置里已经承认的那批来」）
是**横切纪律**，管的是**每一条判据**怎么取人群。本条是它落在「**检查本身**」这一族上的**红线**：
守本条的每一张表都照 `D5` 取人群（`package.json` 里真在跑 `tests/e2e/` 的脚本 · 盘上全部 shell 脚本 ·
`crates/*` · `ci.yml` 的每一条 `run:` · 源码里每一个 `#[ignore]`），**不从「已经接上的那批」里取** ——
否则「第 N+1 套根本没登记」会以「N 套全对得上」的样子绿过去。
⇒ `D5` 是方法，本条是这个方法在这一族上必须一直成立的结果；`D5` 不因本条升格而变成条。

**谁在守**（一族，按它们各自管的那一类检查）：

| 管哪一类 | 判据 |
|---|---|
| e2e 套件 | `e2e_gate_registry_tests.rs::every_e2e_suite_is_either_gated_or_registered_as_exempt` ＋ `every_exemption_still_points_at_a_real_ungated_suite`（豁免不许变死行）＋ `the_suite_count_is_the_same_number_in_all_four_places`（同一个套数的几份副本对拍） |
| shell 脚本的 lint | `shell_lint_registry_tests.rs::every_shell_script_is_either_linted_or_registered_as_exempt` ＋ `every_exemption_still_points_at_a_real_unlinted_script` ＋ `the_coverage_floor_equals_what_is_actually_covered_today`（地板钉成**等于**，落后当场红） |
| 共享 crate 的测试 | `shared_crate_registry_tests.rs::every_shared_crate_is_a_workspace_member` ＋ `the_gate_package_count_tracks_the_number_of_shared_crates` |
| CI 的每一步 | `shared_crate_registry_tests.rs::every_ci_run_step_is_classified_as_local_or_unrunnable`（本地跑，或写清结构上为什么跑不了） |
| `package.json` 的测试脚本 | `shared_crate_registry_tests.rs::every_test_script_is_either_run_by_ci_or_registered_as_manual` |
| `#[ignore]` 的判据 | `shared_crate_registry_tests.rs::every_ignored_test_still_has_someone_who_triggers_it`（e2e 脚本点名 · 判据 spawn · 手动登记三档；手动那一档要写清谁、什么时候跑） |

**违反过几次**（每一次都是人或审计发现的，出处在各判据的模块头注）：
1. G2 新增的那套 e2e **只有 npm 脚本、没进任何门禁** —— `ci.yml` 自陈「当时忘了接线」；
2. shellcheck 的手写分组：08-08 实测全仓 46 个 shell 脚本**只覆盖 44**；同一条计数地板**落后过三次**（每次都是事后补）；
3. 共享 crate 漏跑**两次**：`branch-core` 漏了 fmt/clippy；`usage-core`／`acct-core` 三样全漏、漏了两轮（「它们的测试在 CI 里等于不存在」）；
4. 08-06 第一次把 `cargo fmt --all --check` 补进本地门禁，**两侧当场都红** —— CI 的第一个 Rust 步骤已经红了很久，而每一轮结论都写着「全绿」；次日又发现守它的那条判据自己只看得见带名字的步骤，漏了一条真门禁命令；
5. `graylight-suite` 那一整套跨进程 e2e：CI 从不跑它、发版手测清单里零处提到它 ⇒ 唯一的触发条件是「有人想起来」。

⚠ **它买不到的**：
- **不买「进了门禁的那一步真的在跑」** —— 登记的是接线，不是执行；`#[ignore]` 那一档的手动登记尤其如此
  （它守的是「链还连着」，不是「它们跑过了」）。
- **不买新的一类检查**：上表每一行管一类；出现第七类检查载体（比如一种新的测试运行器）而没人给它立表，本条对它不说话 ——
  那时要做的是给它立一张同形的表，不是指望现有六张覆盖它。
- 设计篇那一族（`design_doc_registry`）是 2026-09-24 才补齐三档的；它里面那条对真 `调研/` 的 `#[ignore]`
  正是按本条登记的（手动档，写清了解锁条件）。

---

## 47. 外部来的值拼进 shell、或交给对端之前，**本侧**先过放行判定 —— 不许拿「对端会校验」「这是我们自己的数据」免检（`V121` 升格 · 用户 2026-09-25 拍板）

**性质**：一个值只要**从本进程外面来**（盘上文件 · manifest · 对端回话 · 用户输入 · 远端目录名），
在它被**拼进 shell 命令串**、或被**交给对端去执行 / 去寻址**之前，**本侧**先过一道按这个值的种类写成的放行判定；
判不过就在本侧拒，带着「哪个值、为什么」出声，**一个请求都不发出去**。放行判定分两形，按值的种类选，不按口味：

- **① 标识符类**（cc-bus agent id · 账号名 · 分叉的 sid / 消息 uuid · ccm 建的 tmux 会话名 · ssh 别名 · 端口转发规格）：
  **字符集白名单**（闭集，默认拒）＋ 不许 `-` 开头（选项注入）＋ 有长度上界的就钉上界。
- **② 自由文本类**（路径 · 用户已有 tmux 会话的 attach 目标）：字符集**放不成闭集**（路径里中文、空格是合法的；已有会话名里真有 glob 字符）
  ⇒ 走**唯一的 quote**（`设计/00 §1.2`）＋ 这一种值的**形式判定**（绝对路径 · 不含 `..` · 不空）＋ 那一张**拒绝集**权威表
  （控制字符 · shell 元字符 · 视觉欺骗字符，住 `payload.rs::config_dir_command_safe` 那一族）。
  🔴 这一形**是拒绝集、不是白名单**，如实写在这里：别把「②」读成「白名单已经覆盖了」。

**本侧判的是形状，不是成员资格**：「这个 agent 种类认不认」「这个账号存不存在」归**拥有那份名单的一方**
（cc-spawn · 上游选择的那张表），本侧不维护第二份合法值名单 —— 后端那条「后端不许再白名单 agent 种类」判据钉的正是这条边。

**为什么不能松动**：
- **「对端会校验」不是理由**：对端校验的是它自己的入口；本侧不判，拼错的那一段就已经在路上了 ——
  而收掉一个 agent 的后果是**杀掉一棵进程树**（守它的判据逐字「**不能靠对端校验**，这一条的后果是杀掉一棵进程树」）。
  把命令串换成后端原语之后，**注入面没了，这一条没作废**：校验从「拼命令之前」挪到「交给后端之前」（BS1b · `K-R98` · `K-R112` 三次搬家，判据逐次跟着换住址）；
  〔C4e〕第四次搬家：cc-bus 写面改由界面经通道直接说后端，校验挪到**界面发出之前**（`src/cc-bus-control.ts`，判据 `tests/cc-bus-control.vitest.ts`：坏 id 一个字节都不发、好 id 发得出去）。
- **「这是我们自己的数据」不是理由**：盘上真出现过没人预料的 id —— `~/.cc-bus/inbox/--help.jsonl`（188 字节）是真实存在的文件，
  那个 id 一拼进 `cc-send` 就被当成一个 flag（B03 审计）。manifest 是我们自己维护的，账号名照样要过字符集。
- **拒过头也算违反**：放行判定写错成「恒拒」，功能在那个平台上就整条没了（见下「违反过几次」第 3 条）。
  ⇒ 守本条的判据都要**正反各一格**（坏值拒、真实好值放），只断「坏的被拒」的判据，把判定焊成恒拒也能绿。

**谁在守**（按值；每一格都是那个值自己那一族判据，**没有一张全集登记表**，见「买不到」）：

| 值 | 本侧放行判定 | 判据 |
|---|---|---|
| cc-bus agent id（①） | `cc_bus.rs::is_valid_bus_id`（读收件箱那一条）· `src/cc-bus-control.ts` 的 `isValidBusId`（查在线 · 发消息 · 收掉，〔C4e〕） | `cc_bus_tests.rs::rejects_leading_dash_ids_from_real_disk` · `cc_bus_tests.rs::rejects_shell_metachars_and_control` · `cc_bus_tests.rs::accepts_real_ids` · `cc_bus_tests.rs::builders_reject_bad_ids_at_the_call_site` · `cc_bus_tests.rs::the_bus_id_rule_agrees_with_the_front_end_on_the_shared_samples`（两份实现读同一份金样 `tests/__fixtures__/cc-bus-control.golden.json` 的 `ids`）· `tests/cc-bus-control.vitest.ts`（真调界面入口，拒在问通道之前，并断合法 id 走得过去） |
| 派生时的账号名（①，交后端之前） | `src/cc-bus-control.ts` 的 `checkSpawnShape`（〔C4e〕从 monitor 挪到界面） | `tests/cc-bus-control.vitest.ts`（含「不许白名单 agent 种类」那一格） |
| 唯一的 quote（②） | `ssh_source.rs::shell_quote` | `cc_bus_tests.rs::quote_roundtrip_is_the_real_property` |
| ccm 建的 tmux 会话名（①，后端） | `plan.rs::validate_tmux_name` | `plan_tests.rs::a_session_name_that_would_confuse_tmux_is_refused` |
| 分叉的 sid / 消息 uuid（①） | `remote_branch.rs::validate_fork_id` | `remote_branch_tests.rs::fork_ids_are_whitelisted` |
| ssh 别名（①；`-` 开头另挡） | `ssh_source.rs::is_safe_alias` | `ssh_source_tier1_tests.rs::is_safe_alias_allowlist` |
| 端口转发规格（①） | `port_forward.rs::validate_spec` | `port_forward_tests.rs::validate_spec_guards` |
| 账号配置目录（②） | `payload.rs::config_dir_command_safe` · `history.rs::validate_config_dir_ps` · 后端 `accounts_query.rs::is_safe_config_dir` | `history_tests.rs::the_config_dir_validator_rejects_every_injection_shape` · `accounts_query_tests.rs::unsafe_config_dirs_are_dropped` · `accounts_query_tests.rs::every_group_of_deceptive_characters_is_rejected_in_a_config_dir` |
| 远端落点路径（②） | `mcp.rs::is_safe_remote_mcp_json` · `acct_iso_deploy.rs::is_safe_remote_acct_iso_dir` | `mcp_tests.rs::remote_mcp_path_guard_rejects_traversal_and_nonabsolute` · `acct_iso_deploy_tests.rs::safe_dir_rejects_dangerous` |
| tmux attach 目标（②，只拒空） | `tmux.rs::is_safe_tmux_target` | `tmux_tests.rs::gate1_rejects_only_empty_target` |

**与邻居的关系**：`设计/00 §1.2` 管「quote 只有一份」—— 它是②形的一半，不管①形，也不管「本侧先判」；
`§34` 管 tmux 破坏性命令的三道门 —— 那是**动作**的门，本条是**值**的门；`§31a` 管 tmux 目标的精确形态 `=<名>:`。三条都不是本条。

**违反过几次**（每一次都是人或审计发现的）：
1. **`--help` 当 id**：盘上真有 `--help.jsonl`，拼进 `cc-send` 就是一个 flag（B03 审计；此后「真实盘面数据」进了判据夹具）。
2. **拒绝集那张表漂过两次**：`payload.rs` 的不可见字符表、`history.rs` 的旧 `SPOOFABLE`（18 项）都缺 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`，
   拼命令那条路漏了 U7-3 给读 manifest 那两处的并集（U8c-1 · audit-0805 收成一张权威表）。定级是纵深防御缺口，不是当时可利用的洞。
3. **拒过头**：配置目录的判定照抄 `starts_with('/')`，Windows 上每个真实账号目录（`C:\Users\…`）必被拒 ⇒「本机分叉时选一个具名账号」在主平台 100% 失败（Phase G 审计）；
   `N-F1c` 让 monitor 的本机账号清单也来问后端时，后端 `is_safe_config_dir` 面对同一形（不拆就清单恒空），拆成「平台无关的安全性质 ＋ 平台相关的形式」两半。

⚠ **它买不到的**：
- **没有人群判据**。上表每一格是那个值自己那一族判据；**第 N+1 个外部值新长出来、没过任何判定就拼进了命令串，一条都不会红** ——
  本条今天是「每个已知入口各有人守」，不是「全部入口都被数过」（`设计/01 §5 D5` 那条纪律在这里没落地）。
- **②形不是白名单**：拒绝集只挡表里有的；表外的新危险字符（新的 Unicode 视觉欺骗段）要人补表。
- **不判「这个值是不是外部来的」**：判据按已知入口写，一个被误认成「内部值」而免检的值，本条看不见。
- **消息正文**（cc-bus 发的那段话）不在本条的放行判定里 —— 它经原语交给后端、不拼命令串，唯一的要求是「不空」。

---

## 48. 本机常驻后端的宿主三条：**监听口要钥匙 · 脱离后不留僵尸 · 测试里起真后端必须 fail-closed 地隔离用户 tmux**（`V121` 升格 · 用户 2026-09-25 拍板）

三条是同一件事的三个面：常驻后端（`设计/01 §3.3`「前端不在也活着，前端起来能**接回去**」· V107）一旦**脱离**了起它的那个 monitor，
**它能做的事就不再有一个父进程看着** —— 谁能连上它、它死了谁收、测试里起的那一个会不会碰到用户的东西，必须各有一条硬规矩。

### 48.1 监听口要钥匙

**性质**：常驻后端对外开的**控制口**（回环 TCP；`listen.rs`）只有出示了钥匙的连接才能拿到**流**（能发 `launch` / `kill` 的那一档）。
「有口没钥匙」⇒ **拒绝起**；空钥匙 ⇒ 按「没设」算；钥匙逐字节全等才算对（前缀 / 后缀 / 大小写都不算）；
钥匙不对、形状不对、口被占着 —— 三种拒法**出声且彼此可分**。钥匙由宿主生成（新生成时 128 位随机）、写进 `0600` 的文件、经 env 交给后端（后端只读铁律不许它自己写文件）。
不认证的只有 hello 那一档（只读、读完即关），它泄露 `claude_dir` / `build_id` / 能力集，这是有意的取舍（`listen.rs` 头注「诚实边界」第 1 条）。

**为什么不能松动**：回环 TCP **没有权限位** —— 同机任何本地进程（**含别的用户**）连得上那个口，
没有钥匙就能以本账号的身份起会话、杀会话。Unix socket 的文件权限在这条路上没有，补回来的**只有这一把钥匙**。

**谁在守**：后端 `listen_tests.rs::a_port_without_a_token_is_refused` · `listen_tests.rs::a_token_without_a_port_is_refused_loudly` ·
`listen_tests.rs::empty_strings_count_as_unset` · `listen_tests.rs::an_empty_token_never_matches` · `listen_tests.rs::tokens_match_is_exact` ·
`listen_tests.rs::attach_verdicts_are_three_distinct_faces` · `listen_tests.rs::the_two_tier_split_is_pinned_cell_by_cell`；
宿主那一半 `local_backend_host_tests.rs::every_token_is_fresh_and_long_enough` · `local_backend_host_tests.rs::the_listen_token_file_is_pinned_cell_by_cell`（`0600` · 竞态支不覆盖 · 空文件支）·
`local_backend_host_tests.rs::a_stranger_on_our_port_is_refused_out_loud_not_silently_reused`（口被别人占着 ⇒ 出声拒，不静默复用）。

**射程**：上面几段说的是**控制口**。常驻后端今天还绑两类口，如实列：
- **中转口**（`relay/listen.rs`，V107 起住常驻后端；远端是 `relay-ensure` 起的脱离 `--relay`）**也要钥匙**〔`RK1` · 主会话 2026-09-25 判「缺口，不是取舍」〕，见下面 48.1a。
- **端口转发**（`dial/uses.rs`）是用户自己配的 `ssh -L` 语义，本就不设钥匙。
- 文件管理器那条回环通道（`chan/host.rs`）有钥匙，但住 monitor，由 `设计/05 §10.5` 管，是本条的同形邻居。

#### 48.1a 中转口的钥匙

**性质**：中转口（回环；本机常驻后端进程内那一形与远端脱离 `--relay` 那一形同一份代码）每一条请求在读请求体、问上游选择**之前**过三问：
带 `Origin` ⇒ **403**（浏览器页面发的请求）；`Host` 不是回环字面量（`127.0.0.1` · `localhost` · `[::1]`，可带口）、缺或不止一个 ⇒ **421**（防 DNS rebinding）；
路径第一段不是钥匙（没有 / 错 / 前缀 / 多一截 / 大小写不同 / 空段）⇒ **403**，比对定长时间（与控制口同一份 `listen::tokens_match`）。
过了才剥掉那一段交给路由（`/s/` 与 `/t/` 一样要过）⇒ 「钥匙对、表里没这一行」仍是 **404**，与 403 **可分**；三种拒法各带一句说得清是哪一问的话。
钥匙 256 位（OS 密码学随机数），住**中转所在那台机器**的 `~/.cc-monitor/relay-key`（`0600`），由中转自己在**绑上口之后**读回或铸（只有绑上口的那一个会写）；
拿不到钥匙 ⇒ **不起**（`--relay` 退 2，进程内那一形出声、后端照常）。钥匙**跨中转重起不变** —— 端口是固定常量，老会话手里的 URL 重起后本来就还有效，换钥匙会打断每一条活会话。
**钥匙只从那份文件进 agent 进程自己的 env**：注入的 URL 本身不带钥匙，渲染器把钥匙段写成 `$(cat "$HOME/.cc-monitor/relay-key")`、在那台机器的 pane shell 里展开
⇒ 载荷、`tmux send-keys` 的 argv、shell 历史、终端回滚、webview 里都没有它；后端 / monitor 自己的 argv、env、日志、tee、上游、`relay-status` 应答里也没有它。
远端 `relay-status` / `relay-ensure` 认「口上是不是**我们的**中转」用差分探针（对的钥匙 ⇒ 404、同形错钥匙 ⇒ 403），对不上 ⇒ `not_ours`，不抢口。

**为什么不能松动**：回环 TCP 没有权限位；中转会**代入账号的凭据**去打上游 —— 门开着，同机任何进程（别的 OS 用户、浏览器里的一张网页）就能以这个账号的额度与身份发请求。
路由键第三段（会话 id / nonce）是公开可铸的标签，**不是**认证。

**谁在守**：`door_tests.rs::only_the_exact_key_as_the_first_segment_gets_in` · `door_tests.rs::any_origin_header_is_refused_before_the_key_is_looked_at` ·
`door_tests.rs::only_a_loopback_literal_host_gets_in` · `door_tests.rs::the_three_refusals_are_distinct_faces` ·
`door_tests.rs::the_key_file_is_minted_once_private_and_read_back_across_restarts`（`0600` · 跨重起同一把 · 坏文件换新）· `door_tests.rs::the_key_file_and_the_key_shape_come_from_the_shared_crate`（〔US1〕钥匙路径与形状只住共享 crate `relay-route-core`，两半同一个 const）·
`server_tests.rs::rk1_the_door_refuses_without_the_key_and_that_is_not_a_404` · `server_tests.rs::rk1_browser_and_rebinding_requests_are_refused_but_the_cli_shape_passes` ·
`server_tests.rs::rk1_the_minted_key_never_shows_up_in_logs_tee_argv_env_or_upstream`（真子进程 · 零命中带正控）·
`machine_tests.rs::ensure_starts_nothing_when_our_relay_already_listens` · `machine_tests.rs::a_port_held_by_something_else_is_not_ours_and_ensure_says_so`；
monitor 那一半 `payload_tests.rs::the_rendered_relay_export_carries_no_key_and_a_real_shell_expands_it_from_home`；
写口登记 `readonly_guard` 第四层（`relay/door.rs`，门 `relay/listen.rs`）。设计与读数住 `调研/第四波记录/RK1.md`。

**诚实边界**：钥匙挡的是**读不到那份 `0600` 文件**的人 —— 能读你家目录的（root、你自己的进程、你起的 agent）本来就能以你的身份跑东西。
钥匙文件在中转跑着时被删 / 改 ⇒ 新会话每一发 403（出声），重起中转就好。会话里的 `ccm` 把**继承来的**（已展开、带钥匙的）`ANTHROPIC_BASE_URL` 原样转进新 pane 的载荷，那一跳钥匙会进一次 `tmux send-keys` 的 argv（`control/ccm/plan.rs`，未修，报主会话）。

### 48.2 脱离后不留僵尸

**性质**：脱离起的后端（Linux 上 `process_group`，**不改父子关系**）无论怎么死 —— 被外面结束、自己崩 —— 进程表里那个 pid 都要**消失**，
不许停在 `Z`；收尸走宿主自己那条线（`local_backend_host.rs::reap_detached`），收完之后 pid 与二进制路径仍留在句柄里（「停」那一步还有凭据核身份）。

**为什么不能松动**：不 `wait` 就留僵尸，而我们自己的事件（EOF）**早就到了** —— 断言落在「消费者收到 EOF」上会绿着放过整个缺陷；
僵尸要等 monitor 退出才被 init 收走，而 monitor 是一直开着的那个进程。

**谁在守**：`local_backend_host_tests.rs::e2e_a_detached_backend_that_dies_leaves_no_zombie`（断言落在 `/proc/<pid>/stat` 的状态字上，不落在自己的事件上；
`#[ignore]`，由 `tests/e2e/local-backend-supervise.sh` 第二趟拉起，CI「本机后端监护真进程验收」一步地板 14 ⇒ 在执行链上，按 `§46` 登记）。

### 48.3 测试里起真后端必须 fail-closed 地隔离用户 tmux

**性质**：任何一条起**真后端二进制**的测试 / e2e 套件，都必须经**唯一的口**拿到私有 tmux 隔离（shim 放进后端 `PATH` 最前、强插 `-L`），
**拿不到就炸，不许降级裸跑**。人群按「那个二进制从哪来」派生，不按测试属性（`#[ignore]` 是可以不写的）。
只 `env_remove("TMUX")` 不算（`TMUX` 一空就回落到默认 socket，**正是**用户那台）；靠 `TMUX_TMPDIR` 不算（`$TMUX` 一有值就压过它）。

**为什么不能松动**：后端一上来就**无条件**往它连得到的 tmux server 装三条全局 hook（〔HX2〕今天是段 `[50, 100)` 里自己那一格，并摘掉段内的死槽；从前是固定槽位 `[50]`；没有关掉的开关）⇒ 不隔离就是去改用户真实 tmux 的状态。

**谁在守**：`local_backend_host_tests.rs::every_test_that_starts_the_real_backend_demands_a_private_tmux`（正题：人群按二进制来历派生，两个文件一起扫）·
`local_backend_host_tests.rs::the_one_shim_gate_really_fails_closed`（那个口在默认门禁里真跑一遍：缺变量必炸、有值原样交出）·
`local_backend_host_tests.rs::every_ignored_test_here_that_spawns_a_real_backend_demands_private_tmux` · `local_backend_tests.rs::every_real_backend_e2e_demands_a_private_tmux_dir`（两条窄的，守 `#[ignore]` 那一族的形状）；
shell 套件那一侧 `e2e_gate_registry_tests.rs::no_e2e_suite_isolates_with_tmux_tmpdir` · `e2e_gate_registry_tests.rs::the_tmux_shim_primitive_has_exactly_one_home`。

**违反过几次**（三条合计）：
1. **tmux 隔离**：08-11 同族事故打没了用户 **9 个**真实会话（`TMUX_TMPDIR=… tmux kill-server`，被 `$TMUX` 压过）；08-13 `backend-cc-bus.sh` 照着过期注释省掉 shim，
   两个夹具会话落到用户默认 socket；08-26 实现常驻后端期间手工起真后端做冒烟，把用户那台 server 的 `[50]` 整个盖成了那个进程的 pid；08-27 / 08-29 各又盖过一次。
2. **钥匙**：真放进来过的 0 次；「没人守」被量到过一次（`K-P1` 回修 `阻-4`）：钥匙文件的 `0600` · 竞态支 · 空文件支三格零覆盖，
   空文件那一支两条路合成闭环、每次都交出空钥匙（后端按「有口没钥匙」拒起 —— 方向是 fail-closed，代价是常驻起不来且自己好不了）。
3. **僵尸**：未见真违反的记录；这一条是 `KPY3` 立的 DoD。

⚠ **它买不到的**：
- **中转口没有钥匙**（见 48.1 射程），本条对它不说话。
- **只量了 Linux**：僵尸那条是 `#[cfg(target_os = "linux")]`；Windows 上脱离的形态与收尸没有真机判据（`RT1` 虚拟机那一路的射程）。
- 48.3 的人群是「起**真后端二进制**的测试」。直接起 tmux server 而不起后端的测试（例：`watcher_tests.rs` 那条 pidfd 判据自带 `-L`）不在人群里，各自隔离、没有一条人群判据管。
- 48.3 自己的头注登记着若干条「手滑」形状的诚实边界（块注释包住取 shim 那一句 · `catch_unwind` 接住那声炸 ⋯⋯）—— 它防的是手滑，不是恶意。

---

## 49. tmux 的**打印通道必须是 UTF-8**，按 TAB 切出来的段数**下溢必须出声**（`V121` 升格 · 用户 2026-09-25 拍板）

**性质**：本仓每一处**按格式串读 tmux 打印通道**的调用点（`list-sessions -F` · `ls -F` · `display-message -p`），
起的 tmux 客户端都必须是 UTF-8 客户端（`capture-pane -p` 实测吐原始字节、不在这个人群里；后端那一处照样带旗，由 `readonly_guard.rs::the_argv_this_site_emits_is_read_only_element_by_element` 逐元素钉着 —— 那是邻居，不是本条的判据）：
- 本机 argv 直传与跨 SSH 的命令串用**旗**（`UTF8_CLIENT_FLAG`，且**必须排在子命令之前**）；
- `sh -c '<可能有多条分支的脚本>'` 用 **env**（`UTF8_CLIENT_ENV`，挂在起 `sh` 的那个 `Command` 上，一行盖住全部分支）；
- 按 TAB 切的，切出的段数 **< 预期 N** ⇒ 出声 ＋ 这一行**不许当好数据**（`tab_underflow`）。
口径只有一个家：后端 `common/tmux_utf8.rs`；monitor 是另一个二进制，它那一份经跨仓对拍焊住。

**为什么不能松动**：tmux 判「客户端是不是 UTF-8」**从不问 glibc**，只对 `LC_ALL`→`LC_CTYPE`→`LANG` 第一个非空值做 `UTF-8` 子串匹配 ⇒
**从配置推不出结果**。一旦是非 UTF-8 客户端，输出里的 TAB 与非 ASCII **全部**被改写成 `_`，**退出码仍是 0** ——
会话名变成整行、会话从列表里静默消失、字段读成空，没有一处报错。改写是**每客户端全有全无**的，所以下溢是完备检测器：脏通道必然把 N 段塌成 1 段。
（与 `§3` 不是一件事：`§3` 管跨进程 JSON 文件的编码，本条管一个外部程序的打印通道。）

**谁在守**：
- 调用点带没带 UTF-8：`gate_tests.rs::both_tmux_call_sites_ask_for_a_utf8_client_before_the_subcommand` ·
  `watcher_tests.rs::every_sh_call_site_in_this_module_carries_the_utf8_env` ·
  `session_snapshot_tests.rs::the_one_list_sessions_call_asks_for_a_utf8_client_before_the_subcommand` ·
  monitor 跨 SSH 那一处 `tmux_tests.rs::the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand`。
- 下溢出声：`gate_tests.rs::the_underflow_predicate_catches_the_real_dirty_bytes` · `watcher_tests.rs::the_underflow_predicate_only_fires_downward` ·
  `session_snapshot_tests.rs::a_tab_starved_line_is_dropped_instead_of_becoming_a_session` · `tmux_tests.rs::a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today`。
- 口径一个家：`tmux_utf8_tests.rs::each_kou_jing_has_exactly_one_home_and_it_is_this_file` · `tmux_utf8_tests.rs::both_consumer_layers_reference_the_home_instead_of_declaring_their_own` ·
  `tmux_utf8_tests.rs::the_one_home_scan_actually_bites`（量具）· monitor 侧 `tmux_tests.rs::utf8_client_kou_jing_has_one_home_and_this_side_matches_it`（跨仓对拍）。

**违反过几次**：`K-R12`（09-04）在无挂载容器里量：当时**六处**读 tmux 的调用点，在 POSIX 客户端 locale 下**六处全部被改写**，一处不剩（`tests/evidence/K-R12-deathvalue.md`）。
同一趟逮到一条真缺陷：monitor 送键前远端核验那条串用 `cut -f` 取 sid，没有分隔符的整行被原样放行 ⇒ sid 变成整行 ⇒ `[ -n "$sid" ]` 恒真 ⇒ **远端核验被静默绕过**（fail-open）。
都是量出来的，不是用户报的。

⚠ **它买不到的**：
- **没有人群判据**：每条判据只管它自己那个模块里的调用点；**新长一处读 tmux 的调用点不带 UTF-8，一条都不会红**。
  〔`IV1` 升格当天现打〕后端 `ccm/plan.rs` 拼给终端的那段脚本里就有一条 `tmux list-panes … -F '#{pane_id}'` 不带旗 ——
  今天无害（单列、`pane_id` 是 ASCII、不按 TAB 切），但它正是「人群外」那一形的活样本。
- **上溢今天仍被丢弃**：`pane_current_path` 里的真 TAB 会多切一段，monitor 的 `!= N` 判法会把那个会话静默丢掉（`tmux_tests.rs::a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today` 的名字就写着「今天仍丢」）。本条只要求下溢出声，不管上溢。
- 旗放错位置是 `rc=1 + unknown flag -u` 的**响错**，而几处调用点刻意不看退出码 ⇒ 那一声在生产里会被压成「一个会话都没有」。位置由各调用点判据单独钉，不由本条的家管。
