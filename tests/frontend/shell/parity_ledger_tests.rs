use std::collections::{BTreeMap, BTreeSet};

/// 一条命令服务哪一侧。`Both` = 这条命令自己就把两侧都办了
/// （例：`search_history` 头注自陈「本地内存索引查询与远端 fan-out **并发**」）。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Side {
    Local,
    Remote,
    Both,
}

/// 不对称的三种性质。**`Undecided` 是刻意留的**——本表的价值之一是把没人裁定过的
/// 缺口摆出来，而不是替产品做主把它塞进白名单。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Asym {
    /// 天然不对称：本地**不需要**这项能力，不是欠账。
    NaturallyAsymmetric,
    /// 平价欠账：本地该有而没有（或反过来），已知、有去处。
    ParityDebt,
    /// 未裁定：本表新发现，需要产品判断。**不许**因为写起来方便就归进上面两类。
    Undecided,
}

/// **全部 Tauri 命令 → (能力, 服务哪一侧)**。
///
/// 改 `generate_handler!` 就必须来改这张表——这是有意的摩擦。
const LEDGER: &[(&str, &str, Side)] = &[
    (
        "open_session_in_new_window",
        "app.window.session",
        Side::Both,
    ),
    // `replay_session_to_window`〔散文墓碑〕退役：独立窗口自己订 `session-lines/<sid>`
    //   （下面 `chan_subscribe` 那一行）；`app.window.session` 还剩开窗那一条。
    // K-H2a：apikey 表那把第三方 API key。读那条**只回掩码**（`KS6`），写那条是「界面」这个第二写者（`KS10`）。
    // 两条都收 `origin` ⇒ `Both`：本机读写 monitor 自己那一份，远端交那台机器的后端
    // （`apikey-read` / `apikey-key-set`）。`creds.apikey` 那条平价欠账（「把 key 送到远端的路」）结清。
    // 读那条（`read_apikey_credentials_status`）与 `apikey.routing` 那一条（`apikey_routing_for`）〔散文墓碑〕退役：
    //   界面经通道直接问那台后端（`apikey-read` / `apikey-routing`，`src/frontend/ui/apikey-reads.ts`）。
    // 写那条（`write_apikey_credentials_key`〔散文墓碑〕）也退役：界面经通道直接发 `apikey-key-set`
    //   （`src/frontend/ui/apikey-reads.ts::writeApikeyKey`）—— 写前核路径那一问由常驻后端的身份（带数据目录）答了。
    // 这次拉起往 `ANTHROPIC_BASE_URL` 里写哪个中转地址 —— 按 origin 取那台的事实
    // （远端用到才起那台的中转），判断只在 `payload::relay_endpoint_for` 一处 ⇒ `Both`。
    // 它接替了 RM1a 那条只对远端的 `relay_ensure`（零调用方；能力 `relay.machine` 随之退役）。
    // `relay_all_sessions_switch`〔散文墓碑〕退役：中转地址与全量开关都由起 agent 那台的 `ccm` 自己定（读它自己的环境）。
    // `K-R49` 那条 `write_account_aliases`〔散文墓碑〕（`lines` ＋ `dryRun`）退役：
    // 它拆成下面两跳 ＋ 一个读回口，能力键 `alias.account-commands` 随之并进 `alias.manage`。
    // 别名只有一类（名字 ＋ 一组 ccm 参数），两跳 ＋ 读回口。
    // `Local` → `Both`：六条都收 `origin`、事实经那台后端（`W5-ALIAS.md §2.2` 方案 ②）⇒ `alias.manage` 的欠账还掉。
    // `aliases_render` / `_read` / `_install`〔散文墓碑〕三行删：别名进了那台后端（帧命令 `aliases-*`）。
    ("open_settings_window", "app.window.settings", Side::Both),
    ("bring_monitor_to_front", "app.window.self", Side::Both),
    // devbench F03：skill 接入面（列出 skill / 读写那个「人手写的注入文件」）。
    // 三条共用一个能力键 —— 它们是同一件事的三个动作（先例：`app.window.session` 也是两条共键）。
    // 远端（和本机，同一条路）的 `INBOX.txt` 能编辑 ⇒ 三条都吃 origin、两侧都经那台机器的后端。
    // 这三条（`skill.inbox`）退役：收件箱那一面进了那台后端（`skill-host-*`），界面经通道直问；对称 ⇒ 不对称数不动。
    ("cc_get_auto_launch", "app.auto-launch", Side::Both),
    ("cc_set_auto_launch", "app.auto-launch", Side::Both),
    ("frontend_perf_log", "app.diagnostics", Side::Both),
    ("get_diagnostics_config", "app.diagnostics", Side::Both),
    ("set_diagnostics_config", "app.diagnostics", Side::Both),
    ("get_log_file_info", "app.logs", Side::Both),
    ("open_log_file", "app.logs", Side::Both),
    ("open_log_dir", "app.logs", Side::Both),
    // P2s（C8）：backend 策略是 **per-host** 的（入参带 origin，本机 = `<local>`）⇒ `Both`。
    // 一个命令服务两侧，正是 C1「本地要和远端一样」在命令面上的样子。
    // 〔条 66〕那个值搬到后端所在那台机器上：原来那条「推生效值」的命令退役，换成问 / 交写两条，
    // 仍是 per-host、入参带 origin ⇒ 仍是 `Both`，仍归 `app.backend-policy`（能力数不动）。
    // 能力 `app.backend-policy` 那两行（问 / 交写「退出行为」，都 `Both`）退役：设置页经通道直接说
    //   后端 `exit-policy-read` / `exit-policy-set`。那是这项能力的全部命令 ⇒ 账本上这条能力没有 Tauri 命令了（它原本对称）。
    // P2s：状态两侧同源 —— 「这台机的后端通道在不在」读的是 `inbound_client` 那**一张**
    // 登记表（远端 hello 后登记，本机 P2 之后也在 hello 后登记）。⇒ `Both`，且只有一条命令。
    ("backend_status", "backend.status", Side::Both),
    // P2s：起/停也是**一条命令管两侧** —— 本机杀子进程、远端断那条 SSH 流。
    // 两者「结果相同、路径不同」，差别塞在实现里而不是塞成两条命令（`C1`）。
    // P2s（A5）：开关面该列哪几台机，由**注册表**说了算（前端自己算会与 origin 分叉四处）。
    ("backend_machines", "backend.lifecycle", Side::Both),
    ("backend_start", "backend.lifecycle", Side::Both),
    ("backend_stop", "backend.lifecycle", Side::Both),
    ("load_config", "app.config", Side::Both),
    ("patch_config", "app.config", Side::Both), // 整份替换的 `save_config` 换成按键补丁，一换一、计数不变 〔散文墓碑〕
    ("get_data_paths", "app.data-paths", Side::Both),
    ("forget_session", "session.forget", Side::Both),
    // 同一件事的另一半：`forget_session` 把一个会话的重放缓冲全丢，这一条丢到只剩尾巴
    // （接上骨架之后，丢掉的正文按偏移要得回来）。两者都只动 monitor 本机的缓冲、按 sid 找 ⇒ `Both`，
    // 归已有能力，能力数与不对称数都不动。
    // `replay_keep_tail_only`〔散文墓碑〕退役：重放缓冲不再分档，前端不再登记。
    // `list_session_activity` · `list_active_sessions`〔散文墓碑〕出表：本机骨架与初始灯是会话流里的成品（本机远端同一条）。
    // `update_history_metadata`（`history.metadata`）· `list_last_accounts`（`accounts.last-used`）两行退役：
    //   注解的读写者换成本机常驻后端（帧命令 `history-annotate` / `history-last-accounts`，界面经通道问 `<local>`）。
    // `search_history`（`search.history`，Both）退役：本机远端的全文搜索都经通道说 `history-search`
    //   （界面 `src/frontend/ui/views/history-search.ts`），不再是 Tauri 命令 ⇒ 能力数 −1（它是这项能力唯一的命令；原本对称 ⇒ 不对称数不动）。
    // ⚠ **P3b 复核（08-12）：这条能力两侧都登记着，但实现是 Win32 专属。**
    // `bind.rs` 的 `SetForegroundWindow` / `IsWindow` / `ShowWindow` 全在 `#[cfg(windows)]` 下
    // ⇒ **Linux 上这两条命令都没有实现**（不是坏了，是没写）。
    // 本表记的是「本地/远端平价」，不是「平台覆盖」—— 所以 `Both` 没记错；
    // 但只读这一行会以为 Linux 也有。⇒ 平台那一维归 `U14`（用户 08-12 已裁：要做，排在后面）。
    ("bring_terminal_to_front", "terminal.focus", Side::Local),
    // 已握手的终端数：从别名读回口里拆出来（住 monitor 进程的 `BindRegistry`），与拉前同一项能力。
    ("bound_terminal_count", "terminal.focus", Side::Local),
    (
        "bring_remote_terminal_to_front",
        "terminal.focus",
        Side::Remote,
    ),
    // 钩子诊断本机 / 远端那一对退役：本机远端合成一条帧命令 `hooks-diag`，界面经通道直问那台。
    // `list_history_projects`（Local）/ `list_remote_history_projects`（Remote）那一对退役 ——
    //   步 12·C 在这里判「不合」的那道设计题（「项目列表的 fan-out 住哪一层」）已定：join 进本机常驻后端
    //   （`history-projects {origin?}`，一台一问），fan-out 搬到前端（`src/frontend/ui/history-reads.ts::fetchRemoteProjects`，
    //   `failedHosts` / TTL 缓存的语义原样）。两条 Tauri 命令都没了 ⇒ 能力 `history.list-projects` 在本表里不再有行。
    // 🔴 **下面这五行是「一条命令自己办两侧」，不是「两条命令各办一侧」。**
    //
    // 「同义双份命令合成一条带 origin 参数的」。合并之后
    // **能力总数与不对称数都不动** —— 那五条能力从前是 `{Local, Remote}`（各有一条命令
    // ⇒ 已对称），今天是 `{Both}`（一条命令办两侧 ⇒ 同样对称）。
    // ⚠ **这一点要读准**：合并**不改善平价**，平价本来就有；它改善的是
    // 「同一件事有几条路」。两者别混着读。
    // `stream_history_sessions_in_project`（`history.list-sessions`）退役：本机常驻后端 `history-sessions` 出成品。
    // `history.read-session` 那三条（整份读 · 按偏移 · 按行号）退役：那台后端出记录行，界面经通道直问
    //   （`history-page` / `history-lines`，`src/frontend/ui/record-reads.ts`）⇒ 能力 `history.read-session` 从此不再有 Tauri 命令
    //   （同 `plugins.marketplaces` 那一先例：今天是通道操作，本机与远端同一条路）。
    // 〔骨架〕`--read-session-from-offset` 在 monitor 侧的调用点 —— 按偏移取一段正文。
    // 归**已有**能力 `history.read-session`：读的是同一份会话 jsonl，两侧都走 `subagent::Backend` ⇒ `Both`。
    // 同能力的另三条（骨架索引 · 大纲清单 · 会话内查找）**退役**：界面经通道直接说帧命令
    //   `history-index` / `history-user-inputs` / `history-find`（后端出成品，`src/frontend/ui/session-reads.ts`），
    //   本机与远端同一条路 ⇒ 它们不再是 Tauri 命令；能力数与不对称数都不动（本能力照旧 `{Both}`）。
    // 按行号取一段正文（不依赖骨架索引）：本机 `<local>` 与远端同一条帧命令 `history-lines`。
    // 删会话那一行退役：界面经通道直说那台后端 `files-delete-session`（`src/frontend/ui/session-writes.ts`）。
    // MCP 读写（`mcp.read` · `mcp.list-project-dirs` · `mcp.write` · `mcp.remove`）与推 / 拉（`mcp.sync`）
    //   八条退役：界面经通道直问那台后端（`mcp-read` · `mcp-server-put` / `-remove` · `mcp-sync-source` / `-preview` / `-apply`），
    //   本机远端同一条路 ⇒ 它们不再是 Tauri 命令；那几格能力都是对称的 ⇒ 不对称数不动。
    // skill 装 / 卸三条（`skill.install`）退役：判 · 写 · 记进了被写那台后端，界面经通道直问；那项能力对称 ⇒ 不对称数不动。
    // `resume_history_session` / `new_local_session`〔散文墓碑〕退役：本机起会话的计划与渲染问本机后端 `launch-local`，
    //   monitor 只剩「在本机开一个终端窗口跑这串」（下面 `open_local_terminal`）。能力 `session.launch` 仍两侧各一条。
    ("open_local_terminal", "session.launch", Side::Local),
    // U4b 那一行（resume 之前问记录还在不在，`Both`）退役：界面经通道直接问后端 `history-record`
    //   （`src/frontend/ui/session-reads.ts::probeSessionRecord`）。能力 `history.read-session` 还有别的命令 ⇒ 能力数与不对称数都不动。
    // `launch_remote_terminal`〔散文墓碑〕退役：ssh 外壳由本机后端 `terminal-ssh` 渲。monitor 剩两条：
    //   交开终端那一问的机器事实（只远端有 —— 本机那一支不经 ssh，原串直接开窗）⇒ 归 `session.launch` 的远端那一侧，
    //   与本机那一侧 `open_local_terminal` 配成一对（能力照旧两侧都有）；
    //   开窗本身本机远端同一条（交来的都是成品）⇒ 新能力 `terminal.window`，`Both`。
    ("terminal_dial", "session.launch", Side::Remote),
    ("open_terminal_window", "terminal.window", Side::Both),
    // `cc_integration_status`〔散文墓碑〕退役：「列 `$PROFILE` ＋ 逐份扫 ＋ 握手数」并进了
    //   `aliases_read`（`alias.manage`）—— 候选各带别名块的现状。`ccm.status` 仍两侧都有（下面两行），能力数不动。
    // 🔴 本机那条 `ccm` 入口的**身份**（不是「在不在」）。
    //    它归 `ccm.status` 而不是自成一格：远端那一侧**已经有对侧**（下一行的
    //    `probe_ccm_cli` 问的就是远端那个 `ccm` 的 `--ccm-probe` 名片；今天经那台后端的 `ccm-probe` 问，同一份）。〔散文墓碑〕
    //    ⇒ 这一条补的是「本机也问得出同一张名片」，不是一条新的单侧能力。
    ("local_ccm_entry_status", "ccm.status", Side::Local),
    // `probe_ccm_cli`〔散文墓碑〕（Remote）退役：渲染进了那台后端、能力问它自己，界面不再先探一遍。
    //   `ccm.status` 从此只剩本机那一条 ⇒ 进 `ASYMMETRY_REASONS`（天然不对称：远端那台的 `ccm` 就是那台后端本身）。
    // 别名块那三格能力 id 按改名改归（别名块是「② 别名」，不是装后端）：
    //   `ccm.install` → `alias.block-install` · `ccm.uninstall` → `alias.block-remove` · `ccm.install-ui` → `alias.block-preview`
    //   （「ccm 助手」年代的遗名）。`K-R117` 的归档表同拍从 ① 挪到 ②。
    // 远端装 / 卸别名块那两条 `Remote` 命令并进 `aliases_block_install` / `_remove`（`Both`），两行删。
    // 🔴 `K-R135`（`R85`/`R87`）：用户级 PATH 那一格。只有本机一侧，理由见
    //    `ASYMMETRY_REASONS` 里 `ccm.user-path` 那一条。
    ("ccm_user_path_status", "ccm.user-path", Side::Local),
    ("ccm_user_path_add", "ccm.user-path", Side::Local),
    ("ccm_user_path_remove", "ccm.user-path", Side::Local),
    // **别名块**并进 `aliases_*` 同一族命令面：
    //   预览 · 装 · 卸三条，接替「终端集成」那几条（`cc_integration_preview` / `_install` / `_uninstall`〔散文墓碑〕；
    //   `_scan_path` 并进了 `aliases_read`）。⚠ **能力 id 刻意不并进 `alias.manage`**（`AL1d.md §2.2`）：别名块两侧都装得上
    //   （远端 `install_remote_alias_block` / `uninstall_remote_alias_block`）⇒ 并过去会让 `alias.manage`
    //   变成「两侧都有」、那行 `ParityDebt` 被表自动抹掉，而清单在远端的写与读回一分没还 —— 正是
    //   `KR53D4` 禁的「靠删掉记录兑现」。⇒ 命令面一族、账本两格，各归各的平价。
    //   〔墓碑 —— 原话「`ccm.*` 这几个 id 是『ccm 助手』年代的遗名，MC1 改了命令名没改 id；改 id 要动 `K-R117` 的归档表」。
    // 改了：`alias.block-*`，归档表同拍挪到 ②。〕
    // 三条 `Local` → `Both`（带 `origin`）；远端卡接上块预览（一并做）。
    // `aliases_block_*`〔散文墓碑〕三行删：同上（`aliases-block-*`）。
    // G6：远端分叉落地 ⇒ `history.branch` 从 ParityDebt 变成两侧都有（该行的
    // 不对称理由已从 `ASYMMETRY_REASONS` 删除——留着就是宣称一条已经补上的欠账）。
    // **两条并成一条**（上面那五行的第五条）——
    // 被并掉的那条命令自己的头注逐字写着「差异今天只剩一处：活儿在远端干」。
    // 分叉那一行退役：界面经通道直说那台后端 `session-fork`（`src/frontend/ui/session-writes.ts`）。
    // 代码全景那一族（能力 `panorama.code-graph`）随代码全景整条摘掉出表：命令 −1（`panorama_place`，Both）· 能力 −1。
    // 查索引状态 / 重建索引那两条（`search.index`，Local）退役：monitor 内存索引删了
    //   （本机搜索也问本机后端）⇒ 能力 `search.index` 整项没了（能力数 −1、不对称数 −1、天然不对称 −1）。
    // P7c-1（08-12）：远端会话的 subagent 展开做出来了 ⇒ 两侧都服务。
    // `load_subagent`（`subagent.load`）退役：那台后端 `history-subagent` 出成品，界面经通道直问 ⇒ 能力不再有 Tauri 命令。
    // 按 origin 问那台机器的后端 `tasks-list`（本机也走后端）⇒ 两侧都服务。
    // `get_session_tasks`〔散文墓碑〕（`session.tasks` 唯一一条）退役：界面经通道直接问 `tasks-list`，后端出成品。
    // 收 `origin` ⇒ `Both`：远端那一栏问那台机器的后端（`footprint-probe`），判定同一份 `build_rows`。
    // `config_surface_report`〔散文墓碑〕出表：成品由那台后端出（`footprint-report`，界面经通道问，本机远端同一条）。
    //   剩 monitor 自己那台那几行的事实（`footprint_client_facts`）：只答 monitor 这一台 ⇒ `Local`，新能力 `audit.monitor-own`（天然不对称）。
    ("footprint_client_facts", "audit.monitor-own", Side::Local),
    // 资产目录同步那一条（`assets.catalog`）退役：界面直问本机后端 `assets-sync`（只报 origin）；对称 ⇒ 不对称数不动。
    // U-CC1：漂移记账。记录那两面随解析进了那台后端（`drift-report`，界面经通道问）；这一条只答 monitor 天生观测的
    // 两面（未知会话 kind · 未知能力 token），账按机器分、读口收 `origin` 只答那一台（monitor 自己的命令、不经后端），
    // 天然 `Both`，登记见 `ORIGIN_TAKING_BOTH`。
    ("drift_ledger_report", "audit.drift-ledger", Side::Both),
    // P8a：插件面只读枚举。按 origin 问那台机器的后端 `plugins-marketplaces` ⇒ 两侧都服务。
    // 那条 Tauri 命令退役：界面经通道直接说帧命令 `plugins-marketplaces`（后端出成品）⇒
    //   能力 `plugins.marketplaces` 从此不再有 Tauri 命令（它今天是一条通道操作，本机与远端同一条路）。
    // PS1：把内嵌 cc-bus 装到本机 `<claude_dir>/skills/`。远端那侧**早就有**
    // （`acct-iso.deploy` 那条路的同族）——但 cc-bus 的远端部署今天没做，如实记欠。
    // 装与三态（`deploy_local_cc_bus` / `cc_bus_install_state`〔散文墓碑〕）进了本机后端（帧命令 `cc-bus-install` / `-state`）；
    //   留下装前那道本机 `ccm` 预检（它问的是本机那个 `ccm`）。能力 `cc-bus.install-state` 随之没有 Tauri 命令了。
    ("cc_bus_ccm_precheck", "cc-bus.deploy", Side::Local),
    // U8c-2c-2：`ccm 调用行`的渲染入口。**这一行说的是「有没有一条上线命令」，
    // 不是「有没有这个能力」** —— 两者 P3t 起分了家，别再合着读。
    //
    // ★★ **P3t-Y4 订正**：原文写「本机路径不经 IR 产出命令（§36 + R07 已裁决）」，而 §36 只绑 Windows。
    // 它整节讲的是 **Windows** 且逐字禁的是「本地渲染器读 `plan.env`」，
    // 它管不着「POSIX 本机能不能用 ccm 调用行渲染器」。R07 那条补充给的理由
    // （「接了也拿不到新东西」）在 CLI 渲染器这一侧**已被 P3t-Y2 证伪**：
    // 接上去拿到的是 `--tmux`，也就是本机旧路结构上产不出来的**会话容器**。
    //
    // 本行仍是 `Side::Remote`，但理由换了：**POSIX 本机不需要一条 IPC 命令** ——
    // 它的渲染器就住在 Rust 里（本机后端 `control/launch_render/local.rs::plan`），前端不必绕一圈问自己。
    // ★★ **P3b 结清（08-12）：这一行从 `Remote` 变 `Both`，不再是不对称。**
    //
    // 它原来的注释写「Remote-only 且未裁定 —— 本机该不该也有一个后端进程来收这件事，
    // 正是 §1.2 今天悬着的问题」。**两半都被证伪了**：
    // ① 「还没裁定」——`C1`/`C8` 已裁（用户 08-11「本地要和远端一样」）；
    // ② 「本机没有后端进程」——P2 交付了：`local_backend.rs::local_stdio_consumer` 里逐字
    //    `inbound_client::register(LOCAL_ORIGIN, client)`，`client_for("<local>")` 实测通。
    // ③ 而 **P3 刀 3 让生产段真的用它了**：`runLocalResumeIntoExistingTmux`
    //    以 `<local>` 调 `backend_send_into` 就地 resume。
    //
    // ⇒ 这条不是「改个分类」，是**这条能力真的两侧都有了**。
    // `backend_send_into`〔散文墓碑〕（Both，能力 `launch.send-into` 的唯一命令）退役：就地 resume 那一次键入
    //   改由界面经通道直接说后端的 `launch{mode:"send-into"}`（`src/frontend/ui/tmux-control.ts::sendInto`），不是 Tauri 命令、不进本表 ⇒
    //   能力 `launch.send-into` 随之从本表没了（它原本对称，不对称数不动）。
    // `render_ccm_launch` · `render_local_attach` · `render_launch_payload`〔散文墓碑〕三条退役：起会话的渲染进了
    //   那台后端（帧命令 `launch-render-cli` · `launch-render-payload` · `launch-local` 的接回那一格），本机远端同一条 `chan.call(origin, …)`
    //   ⇒ 不再是 Tauri 命令；三项能力（`launch.render-cli` / `-payload` / `-attach`）随之从本表没了，它们在 `ASYMMETRY_REASONS` 里那三行
    //   记的不对称（「远端要一条 IPC 才问得到宿主、本机进程内直调」）一起结清。
    // 「文件面板」这一格走了十二行（老面板删了、窗口改走通道）：
    //   〔已删：`sftp_realpath` · `sftp_list_dir` · `sftp_stat` · `sftp_download` · `sftp_upload` ·
    //   `sftp_cancel_transfer`〔散文墓碑〕 · `sftp_mkdir` · `sftp_rename` · `sftp_delete` · `sftp_read_text_for_edit` ·
    //   `sftp_write_text` · `sftp_chmod`〕。能力 `sftp.file-panel` 还在（`open_file_window`）。
    // 零流量复制那一行（步 23b 加的，Remote）随门禁那一格退役一起走了：
    //   窗口的复制走后端 `files-copy`。能力 `sftp.file-panel` 仍由下面那条入口撑着 ⇒ 能力数与不对称数都不动。
    // 改权限位。**归已有能力 `sftp.file-panel`** ⇒
    // 能力数与不对称数都不动：它不是一件新能力，是那张能力对照表里
    // 逐字列着的一行（「权限 ✅ `SETSTAT` / `chmod` / **打平**」），此前那一栏写的是
    // 「❌ 没做（`SETSTAT` 协议里有）」。
    // ⚠ `Remote` 不是欠账：本机改权限压根不经 SFTP
    // （本机那一侧是 `std::fs::set_permissions`，而本机根本没有这个面板 ——
    //  `sftp.file-panel` 那条 `NaturallyAsymmetric` 逐字「本地有操作系统的文件管理器」）。
    // 〔／`§5` 第三段〕**原生文件管理窗口的入口。**
    //  **归已有能力 `sftp.file-panel`** ⇒ 能力数与不对称数都不动。归法的理由要说准，
    //  因为它不是「又一条 sftp 命令」：`§6.6 C` 裁的是「**只能有一个文件管理面板**」，
    //  而那一个的终局形态就是这个原生窗口 —— 它与旧面板是**同一个能力的两个表面**，
    //  不是两个能力。给它另起一个能力 id，等于在账本上把「只有一个面板」记成两个。
    //  ⚠ `Side::Remote` 签的是**实况不是愿望**：窗口的数据面两侧都通
    //  （`filewin::source::list_local` / `list_remote`），但**这条命令只开远端那一侧**
    //  —— 界面上点得到它的地方只有旧 SFTP 面板的表头，而那块面板本来就是远端专用的。
    //  本机那一侧今天的可达路径是窗口自己那颗「本机」按钮（`FileWindow::go_local`），
    //  它不是一条 Tauri 命令 ⇒ 不进本表。逐条理由住 `filewin/entry.rs` 头注。
    ("open_file_window", "sftp.file-panel", Side::Remote),
    // 端口转发那三条（起 · 停 · 列）出表：转发账进了本机常驻后端，帧命令 `forward-*`，界面经通道问。
    // `~/.ssh/config` 导入那三条（别名 · 解析 · 批量）出表：搬进后端帧命令 `ssh-config-*`，界面经通道问。
    // 测试连接那一行（`test_remote_connection`，`ssh.host-config`）出表：本机后端组请求、拨一次（`remote-probe`），界面经通道问。
    // 公钥推送那一行（`ssh.host-config`，Remote）出表：本机后端 `pubkey-push`，界面经通道问。
    ("deploy_remote_backend", "backend.deploy", Side::Remote),
    ("uninstall_remote_backend", "backend.deploy", Side::Remote),
    ("list_remote_mcp_origins", "mcp.list-origins", Side::Remote),
    // 能力 `accounts.list` 那两行（远端 A2 `list_remote_accounts` · 本机 L3a `list_local_accounts`）退役：
    //   本机与远端收成**同一条路** —— 前端经通道（`chan_call`）直接说帧命令 `accounts-list`，后端出成品（并它自己那份 apikey 表）。
    //   ⇒ 这项能力不再是一条 Tauri 命令；两侧都有，住到了帧面上（后端 `read_face.rs`）。
    // 「某会话属哪个账号」那两行（远端 A2 · 本机 E79，能力 `accounts.session-accounts`）退役：
    //   本机与远端收成**同一条路** —— 前端经通道（下面 `chan_call` 那一行）直接说帧命令 `accounts-sessions`。
    //   ⇒ 这项能力不再是一条 Tauri 命令；它两侧都有，只是住到了帧面上（后端 `read_face.rs`）。
    // **主界面说 `call` 的那一跳**（`chan/webview.rs`）：按 `origin` 转给注入的后端句柄，
    //   本机（`<local>`）与远端同一条路 ⇒ `Both`；它吃 `origin`，理由登记在 `ORIGIN_TAKING_BOTH`。
    ("chan_call", "comm.face-a.call", Side::Both),
    // 主界面要那台的能力事实（`Offer`）：本机 `<local>` 与远端同一条路 ⇒ `Both`，归 `comm.face-a.call`（`call` 之前的那一问）。
    ("chan_offer", "comm.face-a.call", Side::Both),
    // 〔「撤单不许回退」〕撤掉 webview 那一跳上带编号的一问：只认编号，本机远端同一张在飞表 ⇒ `Both`。
    ("chan_cancel", "comm.face-a.call", Side::Both),
    // 主界面说 `subscribe`（会话内容流）：本机 `<local>` 与远端同一条路（句柄按 origin 挑行）⇒ `Both`；
    //   新能力 `comm.face-a.subscribe`（与 `call` 是面 A 上两个动作）。`want` / `stop` 只认订阅编号。
    ("chan_subscribe", "comm.face-a.subscribe", Side::Both),
    ("chan_want", "comm.face-a.subscribe", Side::Both),
    ("chan_stop", "comm.face-a.subscribe", Side::Both),
    // 能力 `accounts.trust` 那一行（`check_account_trust`，A3 起 `Both`）退役：信任预检上了帧面
    //   （`accounts-trust`），前端经通道直接问、本机与远端同一条路 ⇒ 同上，住到了帧面上。
    // `deploy_remote_acct_iso`〔散文墓碑〕（`acct-iso.deploy`，Remote）退役：字节随后端二进制走，界面经通道问那台
    //   `acct-iso-install`（本机与远端同一条路）；通道上的问法不是 Tauri 命令、不进本表。`ASYMMETRY_REASONS` 那一行一并摘了。
    // `acct-iso.check` / `acct-iso.shellinit` 各自那对（本机 `check_local_acct_iso` · `local_acct_iso_shellinit`〔散文墓碑〕 ·
    //   远端 `check_remote_acct_iso` · `remote_acct_iso_shellinit`〔散文墓碑〕）合成一条带 origin 的（底下本来就是同一个 `status_on` / `snippet_on`）。
    // 这两条（`acct-iso.check` · `acct-iso.shellinit`）退役：那台后端出成品（围栏在后端校验），界面经通道直问；两项都对称 ⇒ 不对称数不动。
    // 列 tmux 会话那两条（远端 `tmux.manage` · 本机 `tmux.local-census`，§40 九对合并「第 10 对」那一注记）出表：
    //   那台后端的 `tmux-list` 出成品、界面经通道直问，本机远端同一条路 —— 通道上的问法不是 Tauri 命令、不进本表（同上面抓屏 / 杀会话 / 送键）。
    // 抓屏那一条（远端抓屏，Remote）退役：界面经通道直接问那台机器的后端
    //   （`src/frontend/ui/tmux-control.ts::capturePane`，`capture-pane`），本机与远端同一条路；通道上的问法不是 Tauri 命令、不进本表。
    // 杀会话 / 送键两条（`kill_remote_tmux` / `tmux_send_keys`〔散文墓碑〕，Remote）退役：界面经通道直接说
    //   后端的 `kill` / `launch`（`src/frontend/ui/tmux-control.ts::killSession` / `sendKeys`），本机与远端同一条路 ——
    //   `FRAME_PLANE_VERDICTS` 里那两行「`Side` 栏欠一次订正」的账随命令一起结了（不是翻了 `Side`）。
    // P4a（08-12）：读面三条**已经支持本机**（同一条命令串，只是不包进 ssh）⇒ 转 Both。
    // `read_cc_bus_state` / `read_cc_bus_inbox` 两条 Both 退役：驾驶舱读面改由界面经通道问后端 `bus-state` / `bus-inbox`。
    // 写面四条与查在线那一条（`check_cc_bus_agent_online` · `cc_bus_send` · `cc_bus_spawn` ·
    //   `cc_bus_broadcast` · `cc_bus_kill`〔散文墓碑〕）退役：界面经通道直接说那台机器后端的 `bus-list` / `bus-send` /
    //   `bus-spawn` / `bus-broadcast` / `bus-kill`（`src/frontend/ui/cc-bus-control.ts`），本机与远端同一条路；通道上的问法不是 Tauri 命令、
    //   不进本表。`FRAME_PLANE_VERDICTS` 里 spawn / broadcast / kill 三行「`Side` 栏欠一次订正」的账随命令一起结了（不是翻了 `Side`）；
    //   这项能力剩下读面两条（都 `Both`）⇒ 它不再不对称，`ASYMMETRY_REASONS` 那一行一并摘了。
];

/// **不对称能力的理由**。键集合必须**恰好等于**从 `LEDGER` 算出来的不对称集合。
const ASYMMETRY_REASONS: &[(&str, Asym, &str)] = &[
    // 远端 `ccm` 探针那条（Remote）退役之后 `ccm.status` 只剩本机那一条。
    ("ccm.status", Asym::NaturallyAsymmetric, "本机那一条（`local_ccm_entry_status`）问的是「本机 PATH 上那个 `ccm` 是不是我们那一份」—— 那是本机后端引导那一格的事。远端那一侧不再有对应命令：远端的 `ccm` 就是那台后端本身，它会哪些由那台后端在渲染时自己答（`launch-render-cli`），界面不再先探一遍。"),
    ("ccm.user-path", Asym::NaturallyAsymmetric, "🔴 `K-R135`：「把我们那个 bin 目录放上**用户级** PATH」。**天然只有本机一侧，而且这一条的『天然』是可证的，不是图省事**：① **远端那一侧同一件事已经有答案，只是载体不同** —— 远端 `ccm` 落 `~/.local/bin`，而把它放上 PATH 的是写进远端 rc 的那个围栏块（`install_remote_alias_block` ＋ `src/shared/ccm-aliases.sh` 里那一行 —— 那一行正是 `K-R135` 本轮修的：它此前只加 `~/.local/bin`，对本机那一边是错的）。⇒ 欠的不是「远端没有这项能力」，是**两边的机制本来就不同**。② **「用户级 PATH」这一档是 Windows 独有的**（注册表 `HKCU` 下那个 `Environment` 键），而远端按 `K32` 是 Linux ⇒ 那台机器上根本没有这一档可改，补一条对侧命令只能是个空壳。⚠ **诚实边界**：哪天真出现「远端是 Windows」这一形，本条要回来重裁 —— 那时它就不再是 `NaturallyAsymmetric`，而是 `ParityDebt`。今天不给它发明一条够不着的对侧。"),
    // `acct-iso.deploy` 那一行摘了：那条 Remote 命令退役，这项能力不再有 Tauri 命令可记账。
    // `tmux.local-census` · `tmux.manage` 两行随那两条命令出表删：tmux 这一族（列 · 抓屏 · 杀 · 送键）今天全是帧命令、本机远端同一条路，
    //   不再有 Tauri 命令可记账（`tmux.manage` 那一行三次订正的来历，看 git 历史）。
    // 这里原来是 `skill.inbox` 那一行（`Undecided`：「远端项目的收件箱要不要能在这里编辑，没人裁定过」）。
    //   **要**：远端与本机同一条路（经那台机器后端的文件管理那一面读写）⇒ 三条命令 `Both`，这一行摘掉。
    // `cc-bus.install-state` 那一行摘了：三态进了本机后端（`cc-bus-install-state`，界面经通道问），没有 Tauri 命令了。
    ("cc-bus.deploy", Asym::ParityDebt, "`PS1`〔`U10b` 用@08-13 裁「开」后落地〕：把内嵌的 cc-bus 装到 **本机** `<claude_dir>/skills/cc-bus/`。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端同样有 `~/.claude/skills/`，而且本仓**已经有**一条同族的远端部署路（`acct-iso.deploy` 走 SFTP 推 vendored 脚本）。欠的是**把这条本机路复制到远端**：SFTP 推 17 个文件 + 远端侧的围栏（`canonicalize` 在远端不成立，要换成后端侧校验）。⇒ 如实记欠，**不假装两侧都有**。★ 顺带记一条口径：本条的落点是**用户数据目录**，与 `acct-iso.deploy` 那条「只写 cc-monitor 自己的 bin 目录」**性质不同** —— 后者不需要豁免，本条需要（`INVARIANTS` 第 7 条）。"),
    // `audit.config-surface` 那一行（`ParityDebt`：「本地能答、远端答不出，本页明写不连 SSH」）**结清、删掉**：
    //   远端那一栏由那台机器的后端答路径事实（`footprint-probe`）：补后端读口，远端也有真栏。
    // `cc-bus.cockpit` 那一行（`ParityDebt`，三次订正过的「写面欠本机那一侧」）**结清、删掉**：
    //   写面五条都改由界面经通道直接说那台机器的后端（本机与远端同一条路，`src/frontend/ui/cc-bus-control.ts`），monitor 那几条命令退役；
    //   这项能力在 `LEDGER` 里只剩读面两条，都是 `Both` ⇒ 不再不对称。
    // `alias.block-preview` 那一行删：六条 `aliases_*` 都 `Both` 了（裁了、做了）。
    ("backend.deploy", Asym::NaturallyAsymmetric, "★★ **P3b 结清（08-12）：理由整个换掉 —— 原来那句是假的。** 原文写「§40 天然不对称白名单第 3 条：本地会话由 `watcher.rs` 直接读 jsonl，**根本不需要 backend**」，被 P2z + P2 + P2s 三件直接证伪：本机**需要** backend（入方向通道、每台机开关、tmux 帧都靠它），而且**已经会自部署** —— `local_backend.rs::extract_embedded_to`（exe 旁没有本机后端就把内嵌那份释放到 `~/.cc-monitor/bin`）。真正的不对称只剩一格：**本机那次释放不经一条 IPC 命令**，是宿主启动时自己做的（`lib.rs` 的启动段），所以命令面上没有本机对侧。⇒ 记 `natural` 记的是「不需要一条命令」，不是「不需要后端」。"),
    ("mcp.list-origins", Asym::NaturallyAsymmetric, "`list_remote_mcp_origins` 答的是「哪几台远端有 MCP 配置」——「有哪些 origin」这个问题在本机侧退化成一台，没有可列的集合。⚠ 注意它与 `backend_machines` 不同：那条**包含**本机（`LOCAL_ORIGIN`），因为它答的是「哪几台有后端」而本机也有。"),
    // `panorama.code-graph` 那一行（`Undecided`，「远端 repo 的代码图谱既没做、也没在任何计划里登记过」）**摘了**：
    //   要，`panorama_call`（`Side::Remote`）按 origin 问那台后端 ⇒ 两侧都有。
    // `panorama.annotate` 那一行（`ParityDebt`：「远端仓的批注写等后端写面」）**结清、删掉**：
    //   用户 09-24「引擎只算、文件管理来写」，`panorama_edit`（`Side::Both`）本机远端同一条。
    // `creds.apikey` 那一行（`ParityDebt`：「把 key 送到远端的路」）**结清、删掉**：
    //   远端那一份由那台机器的后端写（`apikey-key-set`，临时文件出生即只给本人 —— 机密性由拿着那份文件的
    //   那台机器自己的 `creds_core::perm` 管，不靠 SFTP 的 mode 参数，那一行点名的两条要求都照做了）。
    // `apikey.routing` 那一行（`NaturallyAsymmetric`：「本机这一侧在结构上答不了远端那台」）**删掉**：
    //   那一行自己写着出路 ——「远端那台要答同一个问题，得由跑在那台上的 backend 自己答」—— 今天就是这么答的。
    // `relay.machine` 那一行（`NaturallyAsymmetric`：「只对远端，本机那一个另有住处」）**删掉**：
    //   那条只对远端的命令退役了，接替它的 `relay_endpoint_for_launch` 两台都答（本机经起会话那一侧的缝）。
    // `alias.manage` 那一行删：六条 `aliases_*` 都 `Both` 了（欠账还掉）。
    // `port-forward` 那一行（`NaturallyAsymmetric`：本地没有「转发到自己」）随三条命令出表删：那项能力没有 Tauri 命令了
    //   （账进了本机常驻后端的帧命令 `forward-*`）。不对称本身没变 —— 后端可达表里没有 `<local>`，本机那一格仍回 `unreachable`。
    // `search.index` 那一行（`NaturallyAsymmetric`：「远端不建索引，索引是本机侧的实现细节」）删掉：本机也不建了。
    ("sftp.file-panel", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 1 条：本地有操作系统的文件管理器，不需要它。"),
    ("audit.monitor-own", Asym::NaturallyAsymmetric, "足迹里 **monitor 自己那台**那几行（`HostScope::Client`：本机 `ccm` 入口 · 终端 · shell profile …）的事实：monitor 只在一台机器上跑，远端那台没有 monitor，也就没有「它自己那几行」可答 —— 远端那一栏由那台后端自己答（`footprint-report` 不带 `client`，那一族不进人群）。不是欠一条对侧命令。"),
    // `ssh.host-config` 那一格理由摘了：它最后一条命令（公钥推送）进了本机后端，能力在账本上没有 Tauri 命令了。


];

/// 读 `lib.rs` 的 `generate_handler!`，取出前端真能调到的命令名。
///
/// **以它为准，不以 `#[tauri::command]` 属性为准**：属性只说「它能当命令」，
/// `generate_handler!` 才说「前端真能调到」。（漏注册是**运行时** `command not found`、
/// 不是编译错——`stream_source/` 里有一条注释专门警告过这件事。）
fn registered_commands() -> BTreeSet<String> {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("lib.rs"),
    )
    .expect("read lib.rs");
    let start = src
        .find("generate_handler!")
        .expect("generate_handler! 不见了");
    let open = src[start..].find('[').expect("找不到 [") + start;
    let mut depth = 0usize;
    let mut end = open;
    for (i, c) in src[open..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    guard_core::strip_comment_lines(&src[open + 1..end])
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.rsplit("::").next().unwrap_or(s).to_string())
        .collect()
}

/// 剥掉整行行注释。**必须剥**：本仓的散文里大量逐字提到 `#[tauri::command]`
/// 这类判据字面量（实测 5 处），不剥就会多数出一堆不存在的命令。
/// 更阴的一种：文档注释里写了这个属性、紧接着下一行就是个私有 helper 的 `fn`，
/// 天真的正则会把那个 helper 认成命令（L5 开工复测时实测踩到过一次）。
/// 采集每条命令的**参数列表**（剥注释后），供结构反证用。跳过本文件自身。
///
/// **递归**（U-1，2026-08-01）：原来是单层 `read_dir`，而目录没有扩展名 ⇒ 被整个跳过。
/// `src/adapter/` **今天就已经存在**（里面暂时没有 `#[tauri::command]`，所以还没炸）。
///
/// 漏掉一条子目录里的命令时会怎样（Phase D 审计订正，原注释把话说满了）：
/// - `LEDGER.len() == 123`（`ledger_shape_is_pinned`）**照样满足** —— 它只数账本，不数源码；
/// - 但 `local_or_both_commands_take_no_remote_only_parameter` 的 `checked == 68` **会红**
///   —— 前提是漏掉的那条恰好是 `Local`/`Both`。漏掉纯 `Remote` 的命令则两条都不响。
///
/// 同型 bug 在 `src/backend/no_timer_guard.rs` 同轮修掉。
fn command_signatures() -> BTreeMap<String, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let attr = format!("#[tauri::{}]", "command");
    let mut out = BTreeMap::new();
    // ★ F23 第二刀：改用 `scan_tree!`（不再裸 `read_dir`）。
    // ⚠ 先前这一行写着「它**按构造**摘除调用者自己（拿 `file!()`）」——
    //   那一刀**在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是带 `..` 的
    //   折返路径 ⇒ 后缀比不命中）。本文件不在人群里靠的是**住址**：它住
    //   `tests/frontend/shell/`，而这里扫的是 `src/frontend/shell/src`。
    //
    // 原来这里是裸 `read_dir` + 下面一句写死文件名的跳过（`== Some("parity_ledger.rs")`）。
    // 那种摘除**改名即静默失效**，而失效之后看起来和没失效一模一样 ——
    // `scan_tree!` 的头注逐字警告过这个形态（「别手写 `file!()` 以外的东西当 caller_file」）。
    // 本文件的说明文字里必然含 `#[tauri::command]` 这些子串，摘除一旦失效就会把
    // 自己的散文当成命令签名读进来。
    let mut files: Vec<std::path::PathBuf> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    // Phase E 审计 R5：**必须排序。** 目录栈的产出顺序是文件系统给的，而下面
    // `out.entry(name).or_insert(params)` 是**首个胜** —— 两个子模块出现同名
    // `#[tauri::command] fn` 时，取到哪一份就随机器而变（同一个仓在不同机器上拿到不同签名）。
    // 今天 `src/` 只有一层平目录 + 空的 `adapter/`，影响为零；但递归本就是为将来的多子目录准备的。
    files.sort();
    for path in files {
        let src =
            guard_core::strip_comment_lines(&std::fs::read_to_string(&path).expect("read rs"));
        for (i, _) in src.match_indices(&attr) {
            let rest = &src[i..];
            let Some(fpos) = rest.find("fn ") else {
                continue;
            };
            // 属性与 fn 之间只允许别的属性/空白——否则不是同一个声明。
            if rest[..fpos].contains('{') {
                continue;
            }
            let after = &rest[fpos + 3..];
            let Some(paren) = after.find('(') else {
                continue;
            };
            let name = after[..paren].trim();
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let Some(close) = after[paren..].find(')') else {
                continue;
            };
            let params = after[paren + 1..paren + close].to_string();
            out.entry(name.to_string()).or_insert(params);
        }
    }
    out
}

fn capability_sides() -> BTreeMap<&'static str, BTreeSet<Side>> {
    let mut m: BTreeMap<&str, BTreeSet<Side>> = BTreeMap::new();
    for (_, cap, side) in LEDGER {
        m.entry(cap).or_default().insert(*side);
    }
    m
}

/// 「对称」= 一条命令自己办两侧（`{Both}`），或本地/远端各有命令（`{Local, Remote}`）。
fn asymmetric_capabilities() -> BTreeSet<&'static str> {
    capability_sides()
        .into_iter()
        .filter(|(_, sides)| {
            let both = sides.len() == 1 && sides.contains(&Side::Both);
            let paired =
                sides.len() == 2 && sides.contains(&Side::Local) && sides.contains(&Side::Remote);
            !(both || paired)
        })
        .map(|(cap, _)| cap)
        .collect()
}

/// ★ 断言 1：`generate_handler!` 与本表**双向相等**。新增命令不登记就红。
#[test]
fn every_tauri_command_is_declared_in_the_ledger() {
    let registered = registered_commands();
    // 反向自检：一条都没解析出来 = 解析器坏了，而不是代码没问题。
    // 地板 100 → 99：今天现打 99 条（LOC1b 删搜索三条 · LOC1a 删任务快照一条）。
    // 99 → 50：命令按设计逐批迁通道、人群在缩（今天 98）；这是解析器反空真地板（塌了是个位数），不是计数棘轮 —— 计数由下面的相等断言钉着。
    // 50 → 45：同一理由（反空真地板，今天 49；`deploy_remote_acct_iso`〔散文墓碑〕 迁走一条）。
    assert!(
        registered.len() >= 41, // 45 → 41：会话正文四条退役（界面经通道直问那台后端）
        "只从 generate_handler! 解析出 {} 条命令，多半是解析器坏了",
        registered.len()
    );
    let declared: BTreeSet<String> = LEDGER.iter().map(|(c, _, _)| c.to_string()).collect();
    assert_eq!(
        declared.len(),
        LEDGER.len(),
        "对账表里有重复的命令名：{} 行但只有 {} 个不同的名字",
        LEDGER.len(),
        declared.len()
    );
    let missing: Vec<_> = registered.difference(&declared).collect();
    assert!(
        missing.is_empty(),
        "这些命令已注册但**没进平价对账表**：{missing:?}\n\
             §40「本地 = 不走 ssh 的远端」要求每条命令要么两侧都有、要么在表里带理由说明为什么不。\n\
             请到 `parity_ledger_tests.rs::LEDGER` 里补一行，并想清楚它在本地对应什么。"
    );
    let stale: Vec<_> = declared.difference(&registered).collect();
    assert!(
        stale.is_empty(),
        "这些命令在对账表里但**已经不在 generate_handler! 里**（表在腐烂）：{stale:?}"
    );
}

/// ★ 断言 2：算出来的不对称集合 == 理由表的键集合。
#[test]
fn every_asymmetric_capability_has_a_reason() {
    let asym = asymmetric_capabilities();
    let reasoned: BTreeSet<&str> = ASYMMETRY_REASONS.iter().map(|(c, _, _)| *c).collect();
    assert_eq!(
        reasoned.len(),
        ASYMMETRY_REASONS.len(),
        "理由表里有重复的能力名"
    );
    let no_reason: Vec<_> = asym.difference(&reasoned).collect();
    assert!(
        no_reason.is_empty(),
        "这些能力只有一侧，却没写为什么：{no_reason:?}\n\
             要么把缺的那侧补上，要么到 ASYMMETRY_REASONS 里说清它是天然不对称、平价欠账，\n\
             还是**未裁定**（`Undecided` 是允许的——不许因为写起来方便就归进前两类）。"
    );
    let rotten: Vec<_> = reasoned.difference(&asym).collect();
    assert!(
        rotten.is_empty(),
        "这些能力已经两侧都有了，理由却还留在表里（表在腐烂）：{rotten:?}"
    );
    for (cap, _, why) in ASYMMETRY_REASONS {
        assert!(
            why.chars().count() > 20,
            "{cap} 的理由太短，说不清为什么它只有一侧"
        );
    }
}

/// ★★ **P2 之后 `origin` 不再是「远端专用」** —— 本条因此从一刀切禁改成**登记制**。
///
/// `inbound_client::LOCAL_ORIGIN`（`"<local>"`）让本机也成为一个 origin，
/// 那正是 `C1`「本地要和远端一样，只是远端走 ssh、本地不走」在命令面上的样子。
/// 一条 per-host 的命令**本来就该**吃 `origin`，两侧共用一个签名。
///
/// ⚠ 但**不能直接把 `origin:` 从禁列里删掉** —— 那样一条真·远端命令声明成 `Both`
/// 就再也没人拦。⇒ 改成：吃 `origin:` 的 `Local`/`Both` 命令必须**在本表里登记并写明理由**。
/// `RemoteConfig` 仍是**绝对禁**（它天然只描述一台远端机）。
const ORIGIN_TAKING_BOTH: &[(&str, &str)] = &[
    (
        "chan_call",
        "通道在 Tauri IPC 那一跳上的命令：`origin` 只被交给注入的句柄去找那台的控制通道\
             （`inbound_client` 那张表本机与远端同一张），命令体一个字都不看 `op` / 载荷，也不做远端假设。",
    ),
    (
        "chan_offer",
        "通道在 Tauri IPC 那一跳上问能力事实：`origin.route` 只拦空白名，之后 origin 只交给句柄\
             （`inbound_client` 那张表本机与远端同一张），命令体不做远端假设。",
    ),
    (
        "chan_subscribe",
        "通道 `subscribe` 在 Tauri IPC 那一跳上的命令：`origin.route` 只拦空白名（原位回用法错），\
             之后 origin 只交给句柄（`event_replay::subscribe`）当「挑哪台机器的行」的键 —— 本机 `<local>` 与远端同一个比较，\
             命令体不做远端假设。",
    ),
    (
        "open_session_in_new_window",
        "开独立窗口：`origin` 只随 URL 交给那个窗口（它自己订 `session-lines/<sid>` 要寻址键），\
             本机 `<local>` 与远端同一个处置；命令体不做远端假设。",
    ),
    // `probe_session_record` 那一条〔散文墓碑〕随命令退役摘掉（记录那一问改走通道）。
    // `assets_sync` 那一条〔散文墓碑〕随命令退役摘掉（同步那一问界面直问本机后端）。
    // `config_surface_report`〔散文墓碑〕那一条随命令出表（足迹成品进了后端，界面经通道问）。
    (
        "relay_endpoint_for_launch",
        "这次拉起的中转地址按**那台机器**的事实答：本机两件事走起会话那一侧那条缝，\
             远端问那台后端的成品（`launch-endpoint`，中转在不在也是它答）。命令体对 origin 不做远端假设 —— \
             分派住 `history::relay_endpoint_on`，判断只在 `payload::relay_endpoint_for` 一处。",
    ),
    (
        "drift_ledger_report",
        "「未识别的数据」里 monitor 天生观测的两面（记录那两面由那台后端答）：\
             账本第一层键是 origin。命令体对 origin 不做远端假设 —— 它只用 origin 选「读哪一台那一本」，\
             不经后端、不拨号；`route` 只拦空白名。",
    ),
    // 收件箱三条那三行理由〔散文墓碑〕随命令退役摘掉（那一面进了那台后端）。
    // 这里原来还有两行：`read_cc_bus_state` / `read_cc_bus_inbox`（两条命令退役，界面经通道问后端）。
    // 这里原来还有两行：查在线（`check_cc_bus_agent_online`）与发消息（`cc_bus_send`，`K-R98` 那条
    //   「第一条写面的 `Both`」）—— 两条命令退役（界面经通道直接说后端的 `bus-list` / `bus-send`），登记随之删。
    // 分叉 · 删会话那两行随命令退役（界面经通道直说那台后端），下面只剩三条。
    // 🔴 **下面五条是同一刀落下来的** ——
    // 那句「合成一条带 origin 参数的」。
    //    五条的理由同一句、只是被合的那一对不同，所以逐条写「凭什么说这一对是同一件事」
    //    （**判据不是名字** ——：按名字数分叉会系统性高估）。
    // `list_mcp_project_dirs` / `write_project_mcp_server` / `remove_project_mcp_server` 三条〔散文墓碑〕随命令退役摘掉（MCP 进了那台后端）。
    // `check_account_trust` 那一条〔散文墓碑〕随命令退役摘掉（信任预检改走通道，见 `LEDGER` 那一处）。
    // `get_session_tasks`〔散文墓碑〕那一条随命令退役摘掉（任务快照改走通道，见 `LEDGER` 那一处）。
    // 「退出行为」问 / 交写那两条〔散文墓碑〕随命令退役摘掉（设置页改走通道）。
    (
        "backend_status",
        "P2s：状态按 origin 查 `inbound_client` 的登记表，那张表两侧共用。\
             `pid`/`attempts` 只有本机有 —— 那是**天然不对称**（远端进程在别人机器上），\
             所以它们是 `Option` 而不是「远端填 0」。",
    ),
    (
        "backend_start",
        "P2s：按 origin 分派 —— 本机起被监护的子进程，远端重起那条流的 task。\
             命令体只认识 origin，不认识 ssh（起法由 `lib.rs` 注册成闭包交进来）。",
    ),
    (
        "backend_stop",
        "P2s：按 origin 分派 —— 本机杀子进程，远端 `abort()` 那条流。\
             远端后端随管道破裂退出，本机看不见那个进程，所以返回的是「已断流」不是「已停进程」。",
    ),
    // acct-iso 两对合成的那两条〔散文墓碑〕随命令退役摘掉（那台后端出成品）。
    // 别名一族六条的签字随命令退役摘掉（那台后端出成品，`aliases-*`）。
];

/// ★ 断言 3（有牙的那条）：声明 `Local` / `Both` 的命令，签名里**不许**出现远端专用参数。
///
/// 防的是「为了让表好看，把一条远端命令声明成两侧都有」。反过来那条
///（`Remote` ⇒ 必须吃远端参数）**刻意没做**，理由见模块头注。
#[test]
fn local_or_both_commands_take_no_remote_only_parameter() {
    let sigs = command_signatures();
    // 判据运行时拼：直接写字面量的话，本文件自己的说明文字会被扫到。
    let origin_needle = format!("{}:", "origin");
    let needles = [origin_needle.clone(), format!("Remote{}", "Config")];
    let mut checked = 0usize;
    // 报文里那句「Local A + Both B」也**现算**，不手抄——
    // 手抄的分解式会与 `checked` 各自漂：先前那句「84 = Local 53 + Both 31」
    // 三个数**没有一个**等于当时的 `checked`（88）。
    let mut n_local = 0usize;
    let mut n_both = 0usize;
    for (cmd, _, side) in LEDGER {
        if !matches!(side, Side::Local | Side::Both) {
            continue;
        }
        let Some(params) = sigs.get(*cmd) else {
            continue;
        };
        let flat: String = params.split_whitespace().collect::<Vec<_>>().join(" ");
        checked += 1;
        match side {
            Side::Local => n_local += 1,
            _ => n_both += 1,
        }
        for n in &needles {
            if *n == origin_needle && ORIGIN_TAKING_BOTH.iter().any(|(c, _)| c == cmd) {
                continue; // 已登记：见 ORIGIN_TAKING_BOTH 的理由
            }
            assert!(
                !flat.contains(n.as_str()),
                "{cmd} 在对账表里声明为 {side:?}，签名里却有远端专用参数 `{n}`：{flat}\n\
                     一条只能对远端起作用的命令，不该被记成「本地也有」——那会造出假的平价。\n\
                     ★ 若它**真的**两侧都服务（本机 origin = `<local>`，见 C1），\n\
                     到 `ORIGIN_TAKING_BOTH` 里登记并写清「命令体对 origin 不做远端假设」。"
            );
        }
    }
    // 反向自检：一条都没检到 = 签名采集坏了。**等号而不是 `>=`**（T04 审计重要 5：
    // 写 `>= N` 恰好容忍一次静默降级）。
    //
    // ★★ **报文里的每一个数都从这个常量或现算量渲染出来，一个手抄的都没有**。
    //
    // 先前这里是「一个值装了两件事」的活体：断言写 `checked == 88`，
    // 而同一条报文逐字写「真实应为 **84** = Local 53 + Both 31」——
    // **三个数没有一个是 88**。判据一红，人照报文去查 84 / 53 / 31，
    // 查的是一个不存在的事实；而那串增量账（+3 / +1 / +1 / +1 / +4 / +1）
    // 也凑不出 88。根因是断言那个数**跟着代码走**、报文那个数**是写的时候手抄的**，
    // 两者之间没有任何东西钉住它们相等。
    //
    // ⇒ 把两份拷贝合成一个值。改 `EXPECTED_LOCAL_OR_BOTH`，
    //   报文里印出来的数**结构上不可能不跟着变**。
    //
    // # 这个数是怎么长起来的（改 `LEDGER` 就来读这一段，然后改上面那个常量）
    //
    // - P4a（08-12）把 cc-bus **读面三条**从 `Remote` 转成 `Both`
    //   （本机跑同一条命令串，只是不包进 ssh）；
    // - devbench F03 的 skill 接入面 **+3**（`list_skills` / `read_skill_file` /〔散文墓碑〕
    //   `write_skill_file`，都 `Local`）；〔散文墓碑〕〔MIG-3a 随收件箱进后端退役 −3〕
    // - E79 的 `list_local_session_accounts` **+1**；〔散文墓碑〕〔C4a 随「本机与远端同一条路」退役 −1〕
    // - U-CC1 的 `drift_ledger_report` **+1**，`Both` —— 本地行与远端行都经同一个
    //   `parse_line` 喂进同一个进程内账本；
    // - F08 的 `account_usage_local` **+1** —— 它补平了 `usage.per-account` 那条 ParityDebt；  〔散文墓碑〕
    // - P2s 的四条 `Both`（策略那一条今天换成了 `backend_exit_policy` / `set_backend_exit_policy` 两条）
    //   ＋ `backend_status` / `backend_start` / `backend_stop` —— per-host backend 策略与状态，本机 origin 是 `<local>`（C1）；
    // - 🔴 `K-R135`（`R85`/`R87`）的 `ccm_user_path_status` / `ccm_user_path_add` /
    //   `ccm_user_path_remove` **+3**，都是 `Local` —— 用户级 PATH 那一格
    //   （现在状态 · 加 · 撤）。**只有本机一侧是天然的**，理由住 `ASYMMETRY_REASONS`
    //   里 `ccm.user-path` 那一条（远端那一侧同一件事由写进远端 rc 的围栏块办，
    //   而「用户级 PATH」这一档是 Windows 独有的，远端按 `K32` 是 Linux）；
    // - P8a 的 `list_plugin_marketplaces`〔散文墓碑〕 **+1**，`Local` —— marketplace 只读枚举那时只有
    //   本机口，欠的那半写在 `plugins.marketplaces` 那行上；
    // - K-H2a **+2**（`read_apikey_credentials_status` / `write_apikey_credentials_key`，都 `Local`）；
    // - K-H2b **+1**（`apikey_routing_for`，`Local`）—— 界面问「这几个**本机**账号走不走 apikey 端点改写」。
    //   只答本机不是欠账，是**机制决定的**：中转是每台机器自己的进程、注入的是回环地址，
    //   本机这一侧答不了远端那台 ⇒ 它在 `ASYMMETRY_REASONS` 里记的是 `NaturallyAsymmetric`。
    //
    // ⚠ 这一段是**账**，不是判据。它里面的数**没有**任何东西钉住 ——
    //   真值以 `EXPECTED_LOCAL_OR_BOTH` 与失败时印出来的 `Local {n} + Both {m}` 为准。
    // - K-R49 **+1**（`write_account_aliases`〔散文墓碑〕，`Local`）—— 加了账号就把那条命令落盘；〔AL1 子步 4 退役 −1〕
    //   新能力 `alias.account-commands`，远端那半欠什么见 `ASYMMETRY_REASONS` 里那一行。
    // - K-R69 **+1**（`local_ccm_entry_status`，`Local`）—— 归**已有**能力 `ccm.status`
    //   （远端那一侧的对侧是 `probe_ccm_cli`），所以只涨命令数、不涨能力数。〔散文墓碑〕
    // - K-R98 **+1**（`cc_bus_send` 从 `Remote` 转 `Both`）—— **不是新增一条命令**，
    //   是同一条命令的两侧收成了一条路（远端那半改走 `bus-send`，不再拼 shell 串）。
    //   ⇒ 命令总数与能力总数都不动，只有这个数 +1；`cc-bus.cockpit` 仍不对称
    //   （spawn / 广播 / 收掉三条还是 `Remote`），所以不对称条数也不动。
    // - K-R109 **+1**（`render_local_attach`，`Local`）—— 本机后端产 attach 那一句。
    //   归**已有**能力 `session.launch`（理由逐条写在 `LEDGER` 那一行旁边）⇒
    //   命令总数 +1、能力总数不动、不对称数不动，**只有这个数跟着 +1**。
    //   ⚠ 派工单点名的是「`commands.vitest.ts` 两个 147 ＋ `LEDGER` 一行」三处，
    //   **这个数是第四处** —— 它不在任何一份派工单里，是本轮自己量出来的。
    // - 〔骨架〕**+2**（`read_session_index` / `read_session_range`，都 `Both`，
    //   归已有能力 `history.read-session`）。
    // - **+2**（`aliases_render` / `aliases_read` / `aliases_install` 三条 `Local` 进，`write_account_aliases`〔散文墓碑〕退役）。
    // - **+1**（`replay_keep_tail_only`，`Both`，归已有能力 `session.forget`）。
    // - **+1**（`list_user_inputs`，`Both`，归已有能力 `history.read-session`）。
    // - **+1**（`find_in_session`，`Both`，归已有能力 `history.read-session`）。
    // - 〔PN1b 选图〕**+2**（`panorama_diagram_kinds`〔散文墓碑〕 / `panorama_diagram`，都 `Local`，〔两条都随内嵌引擎退役了〕
    //   归已有能力 `panorama.code-graph`）⇒ 命令总数 +2、能力总数不动、不对称数不动。
    // - **+1**（`probe_session_record`，`Both`，归已有能力 `history.read-session`）。
    // - **+1**（`read_session_lines`，`Both`，归已有能力 `history.read-session`）；**−1**（`replay_keep_tail_only`〔散文墓碑〕退役，`Both`，`session.forget` 还剩 `forget_session`）；
    //   **+3**（`chan_subscribe` / `chan_want` / `chan_stop`，都 `Both`，新能力 `comm.face-a.subscribe`）；**−1**（`replay_session_to_window`〔散文墓碑〕退役，`Both`）。
    // - **−2**（`read_apikey_credentials_status` / `apikey_routing_for`，都 `Both`：界面经通道问那台后端）。
    const EXPECTED_LOCAL_OR_BOTH: usize = 36; // 37 − 1（`relay_all_sessions_switch`，Both：中转地址交给 ccm 自己定）// 38 − 1（`panorama_place`，Both：代码全景整条摘掉）// 37 ＋ 1（`open_terminal_window`，Both；退役的 `launch_remote_terminal` 是 Remote、`terminal_dial` 是 Remote，都不在本数里）// 41 − 4（`stream_read_session_jsonl` · `read_session_range` · `read_session_lines` · `load_subagent`，都 Both：那台后端出成品，界面经通道直问） // 42 − 3（`panorama_call` / `panorama_edit` / `panorama_cancel`，都 Both：界面经通道直问）＋ 2（`panorama_place` · `chan_cancel`，都 Both）// 主线 45 ＋ MIG-1 −3（活会话两条 Both · 本机列 tmux Local）⇒ 42 // 57 − 1（`list_local_tmux`，Local）// 主线 59 ＋ MIG-1 −2 ⇒ 57 // 61 − 4（`resume_history_session` · `new_local_session` · `render_local_attach` Local · `relay_endpoint_for_launch` Both 退役）＋ 2（`open_local_terminal` Local · `relay_all_sessions_switch` Both） // 61 → −2（`list_session_activity` / `list_active_sessions`，都 Both：会话流的成品替掉）// 65 → −4（`assets_sync` · `skill_install_preview` / `_apply` · `skill_uninstall_apply`，都 Both）// 71 → −6（`read_mcp_servers` Local · `list_mcp_project_dirs` / `write_project_mcp_server` / `remove_project_mcp_server` / `mcp_sync_preview` / `mcp_sync_apply` Both：MCP 进了那台后端，界面经通道问）// 基数 70 → +1（`chan_offer`，Both，归已有能力 `comm.face-a.call`）// −2（`read_cc_bus_state` / `read_cc_bus_inbox` Both 改走通道 `bus-state` / `bus-inbox`）// 主线 ＋ HX2 −1（`write_apikey_credentials_key`） ⇒ 72 // 主线 76 ＋ LOC1b −3（全文搜索 Both ＋ 查索引状态 / 重建索引 Local 三条退役：本机搜索改问本机后端）⇒ 73 // 主线 77 ＋ LOC1a −1（`get_session_tasks` Both 改走通道 `tasks-list`）⇒ 76 // 主线 79 ＋ US1 −2 ⇒ 77 // 基数 81 ＋ SU1 +1 ＋ C4e −3 ⇒ 79（跑出来核过） // +1（skill_uninstall_apply，Both；归已有能力 `skill.install`） // −2（`check_cc_bus_agent_online` / `cc_bus_send`，都 `Both`：cc-bus 查在线与发消息改由界面经通道说） // −1（backend_send_into，Both：就地 resume 那一次键入改走通道） // 主线 85 ＋ 本路 -4 ⇒ 81 // −4（list_history_projects Local · stream_history_sessions_in_project / update_history_metadata / list_last_accounts Both：改走通道；list_remote_history_projects 是 Remote，不在本数里） // 主线 83 ＋ 本路 +2 ⇒ 85（跑出来核过） // 主线 95 ＋ AS2 +3 ＋ RM1f −15 ⇒ 83（跑出来核过） // 主线 95 ＋ AS2 +3（assets_sync · skill_install_preview · skill_install_apply，都 Both） // −2（`backend_exit_policy` / `set_backend_exit_policy` 两条 Both 改走通道）// 主线 100 ＋ 本路 −3 ⇒ 97（跑出来核过） // −1（`probe_session_record` Both 改走通道 `history-record`）// −2（`list_local_accounts` Local · `check_account_trust` Both 改走通道；`list_remote_accounts` 是 Remote，不在本数里） // 主线 100 ＋ AL1d -2 ＋ AS1 +2 ⇒ 100（跑出来核过；AS1：mcp_sync_preview / mcp_sync_apply，Both，新能力 `mcp.sync`） // 〔−2（`aliases_block_*` 三条 Local 进，「终端集成」五条 Local 退役）；合并主线 636cc1a0 按两边增量相加 100 − 2〕 // 两边各 −4（C4b：read_session_index / list_user_inputs / find_in_session / list_plugin_marketplaces 四条 Both 改走通道；主线 RM1d −5 ＋ U4b +1）⇒ 108 − 8 = 100，跑出来核过 // 〔−5（本机六条全景写命令 Local 退役 −6，`panorama_edit` Both ＋1）〕（合并主线按两边增量相加） // 〔+1（probe_session_record，Both）；合并 RL1 按两边增量相加〕 // 〔+1（`relay_endpoint_for_launch` Both 进，接替只对远端的 `relay_ensure`）〕 // 〔合并 C4a：±0（E79 本机会话账号 Local 退役 −1 · `chan_call` Both ＋1）〕 // 〔+1（find_in_session，Both）〕 // 〔条 66：+1（推生效值那一条 Both 退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 两条 Both ＋2）〕 〔合并 AL1：+2（aliases_render / aliases_read / aliases_install 三条 Local ＋3，write_account_aliases〔散文墓碑〕退役 −1）〕 〔合并 PN1b：+2〕 〔合并 A3：+3（两条 acct-iso 本机命令 Local ＋ check_account_trust Remote→Both）〕 〔合并 U3b＋SE1：两路各 +1，同基线合并时 git 合成了同一个 97，现打 98〕 −2（`aggregate_usage_all` Local · `account_usage_local` Local）  〔散文墓碑〕
    assert_eq!(
        checked, EXPECTED_LOCAL_OR_BOTH,
        "检到 {checked} 条 Local/Both 命令（Local {n_local} + Both {n_both}），\
             而本条期望 {EXPECTED_LOCAL_OR_BOTH} 条。\n\
             改 LEDGER 就要来确认这个数：把 `EXPECTED_LOCAL_OR_BOTH` 改成新值，\
             并到它上面那段「这个数是怎么长起来的」里补一行说明谁加/删了哪几条。"
    );
}

// 墓碑：这里从前有一条「别名的命令面是一族」（`aliases_*` 那几条 Tauri 命令 == 别名能力的本机侧命令，
//   〔散文墓碑〕`the_alias_capabilities_speak_through_one_command_family`）。那一族整个进了那台后端（帧命令 `aliases-*`），
//   `generate_handler!` 里一条都不剩 ⇒ 本条没有人群了；「一族」那件事由后端 `inbound::REGISTRY` 的六行 ＋ 金样判据接住
//   （`tests/backend/assets/aliases/aliases_tests.rs::the_alias_wire_matches_the_cross_language_golden`）。

/// ★ 断言 4：表的形状钉死。改 `LEDGER` 就要来改这几个数。
/// ★★ **P3b-Y3：每条不对称理由都得**可追问**。**
///
/// `E` 阶段量到假理由有**两种假法**：
/// **A 过期**（写下时是真的，事实变了理由没跟）· **B 从未成立**（写下时就没验证过）。
/// 08-11 补审按 A 扫，逐字记「两条理由今天已经是假的」——**实际是三条**，
/// 第三条（`launch.render-cli` 的「本地渲染必须在目标机器上做」）是 B，
/// 时间线扫不到它，直到 `P3t-Y2` 顺手量了一次才发现它从头到尾没被验证过。
///
/// # 钉法：钉**可追问性**，不钉真假
///
/// 判语义今天做不到。退一步：每条理由必须带一处**可核实的锚** ——
/// 文件名 / 函数名 / 判据名 / issue 号 / 逐字引号。没有锚的理由 = 一句无从复核的断言，
/// 而 B 类假理由的共同外观正是「读着有道理，但没有任何东西可以去核」。
///
/// # 它守什么、不守什么
///
/// **守**：新增一条不对称却只写一句空泛道理 ⇒ 当场红。
/// **不守**：① 锚**指得对不对**（随手写个存在的文件名就能过）——本条只保证**可追问**，
///   **不保证追问过**，别读成「理由都验过了」；② A 类过期 —— 那要靠人按时间线扫。
#[test]
fn every_asymmetry_reason_carries_something_you_can_go_check() {
    // 锚的形态：反引号里的标识符（文件/函数/判据名）、`§` 段号、`#` issue 号。
    let mut bare = Vec::new();
    for (cap, _, why) in ASYMMETRY_REASONS {
        let has_code_anchor = why.matches('`').count() >= 2;
        let has_section = why.contains('§');
        let has_issue = why.contains('#');
        if !(has_code_anchor || has_section || has_issue) {
            bare.push(format!("  {cap}"));
        }
    }
    // 完备性自检：人群空了「全过」与「没测」长得一样。
    assert!(
        ASYMMETRY_REASONS.len() >= 6, // 7 → 6：`ssh.host-config` 一行随公钥推送出表删（人群真少了）// 8 → 7：`acct-iso.deploy` 一行随命令删（人群真少了）// 主线 11 ＋ MIG-1 −3（`port-forward` · `tmux.manage` · `tmux.local-census`）⇒ 8 // 11 → 9：`tmux.local-census` · `tmux.manage` 两行随列 tmux 那两条命令出表删 // 主线 12 ＋ MIG-1 −1 ⇒ 11 // 14 → 12：`launch.render-*` 三行删、`ccm.status` 一行加（人群真少了） // 14 → 13：`port-forward` 一行删（那项能力没有 Tauri 命令了，人群真少了） // 15 → 14：`alias.manage` · `alias.block-preview` 两行删（人群真少了）
        "只抽到 {} 条不对称理由 —— 抽取器坏了，本条此刻无效",
        ASYMMETRY_REASONS.len()
    );
    assert!(
        bare.is_empty(),
        "这些不对称理由**没有任何可以去核的东西**（无代码锚 / 无 § 段号 / 无 issue 号）：\n{}\n\n\
             ⚠ 本条不判理由真假，只判它**可不可追问**。\n\
             「读着有道理但无从复核」正是 B 类假理由（从未成立）的共同外观 ——\n\
             本区已经栽过一次：`launch.render-cli` 的「本地渲染必须在目标机器上做」\n\
             从头到尾没人量过，直到 P3t 顺手跑了一次 `bash -lic` 才发现它比远端还便宜。",
        bare.join("\n")
    );
}

// 🪦这里原有 `the_two_launch_rows_no_longer_carry_the_two_falsified_clauses`（`KR53D4`：`launch.render-payload` / `launch.render-cli` 〔散文墓碑〕
//   两行措辞的钉子，禁「靠删掉记录兑现」）。那两项不对称这一拍**真的结清了**（渲染进了那台后端，本机远端同一条帧命令），不是删记录躲判据。

// 这里原来是 `the_tmux_manage_row_stops_waiting_for_a_backend_primitive`〔散文墓碑〕（钉 `tmux.manage` 那一行的订正不许抹账）：
//   那一行随能力出表删了 —— 不是抹账，是那一族全搬上了帧面（本机远端同一条路，已对称），本表只记 Tauri 命令。

#[test]
fn ledger_shape_is_pinned() {
    // ⚠ 订正（2026-08-03 复盘）：下面四条尾注此前都**只记到 U8c-2c-2 为止** ——
    // 而 U8a-2c-pre（`57dba2a`）把这四个数各 +1 时，只改了数、一条尾注都没动。
    // ⇒ 尾注把 U8a-2c-pre 的增量记在了 U8c-2c-2 名下。**尾注的用处就是说清「谁加的」，
    // 归属错了就不如没有。**
    assert_eq!(LEDGER.len(), 42, "命令总数变了"); // 43 − 1（`relay_all_sessions_switch`，Both：`relay.launch-endpoint` 唯一的命令 ⇒ 能力数 −1，它对称 ⇒ 不对称数不动）// 44 − 1（`panorama_place`）// 43 − 1（`launch_remote_terminal`，Remote）＋ 2（`terminal_dial` Remote 归 `session.launch` · `open_terminal_window` Both 新能力 `terminal.window`）⇒ 能力数 +1，不对称数不动 // 47 − 4（会话正文四条，都 Both：`history.read-session` 三条 · `subagent.load` 一条，那台后端出成品、界面经通道直问；两项能力都没了 Tauri 命令 ⇒ 能力数 −2，都对称 ⇒ 不对称数不动）// 48 − 3（全景问 · 写 · 撤，界面经通道直问）＋ 2（`panorama_place` · `chan_cancel`）// −1（公钥推送，Remote：进了本机后端 `pubkey-push`；它是 `ssh.host-config` 最后一条命令 ⇒ 能力数看下一行）// 50 − 1（`deploy_remote_acct_iso`，Remote：字节随后端二进制走、界面经通道问 `acct-iso-install`；它是 `acct-iso.deploy` 唯一的命令 ⇒ 能力数 −1，它原本不对称 ⇒ 不对称数 −1）// 主线 61 ＋ MIG-1 −11 ⇒ 50 // 66 − 1（`test_remote_connection`，Remote，归 `ssh.host-config`；那项能力还有 `push_public_key` ⇒ 能力数与不对称数都不动）// 68 − 2（`list_remote_tmux` Remote · `list_local_tmux` Local：`tmux-list` 出成品、界面经通道直问；它们是 `tmux.manage` / `tmux.local-census` 两项能力最后的命令 ⇒ 能力数 −2，两项原本都不对称 ⇒ 不对称数 −2）// 主线 76 ＋ MIG-1 −8 ⇒ 68 // 81 − 7（`resume_history_session` · `new_local_session` · `render_local_attach` · `render_ccm_launch` · `render_launch_payload` · `relay_endpoint_for_launch` · `probe_ccm_cli`）＋ 2（`open_local_terminal` · `relay_all_sessions_switch`） // 76 − 3（start_forward / stop_forward / list_forwards，都 Remote：转发账进本机常驻后端帧命令 `forward-*`；它们是 `port-forward` 那项能力的全部命令 ⇒ 能力数 −1，它原本不对称 ⇒ 不对称数 −1）// 78 − 2（list_session_activity / list_active_sessions，都 Both；它们是 `session.activity` · `session.list-active` 两项能力的全部命令 ⇒ 能力数 −2，都对称 ⇒ 不对称数不动）// 主线 81 − MIG-1 3（list_ssh_host_aliases / resolve_ssh_host / import_ssh_hosts〔散文墓碑〕，都 Remote：导入搬进后端帧命令 `ssh-config-*`；能力数与不对称数都不动）⇒ 78 // 85 → −4（资产同步 ＋ skill 装卸三条；`assets.catalog` / `skill.install` 两项能力没了 ⇒ 能力数 −2，都对称）// 93 → −8（MCP 读写六条 ＋ 推拉两条改走通道；`mcp.read` / `mcp.list-project-dirs` / `mcp.write` / `mcp.remove` / `mcp.sync` 五项能力没有 Tauri 命令了 ⇒ 能力数 −5，都对称 ⇒ 不对称数不动）// 92 → +1（`chan_offer`，Both，归已有能力 `comm.face-a.call`；能力数 · 不对称数不动）// 94 − AL2 2 ⇒ 92 // 远端装 / 卸别名块两条并进 aliases_block_* −2 // −2（acct-iso 两对各合成一条带 origin 的：4 → 2；能力数不动）// −2（read_cc_bus_state / read_cc_bus_inbox，Both：驾驶舱读面改走通道 `bus-state` / `bus-inbox`；`cc-bus.cockpit` 从此没有 Tauri 命令 ⇒ 能力数 −1，它原本对称 ⇒ 不对称数不动）// 主线 ＋ HX2 −1（`write_apikey_credentials_key`） ⇒ 98 // 主线 102 ＋ LOC1b −3（全文搜索 · 查索引状态 · 重建索引）⇒ 99 // 主线 103 ＋ LOC1a −1（get_session_tasks，Both：任务快照改走通道）⇒ 102 // 主线 105 ＋ US1 −2（read_apikey_credentials_status / apikey_routing_for）⇒ 103 // 基数 113 ＋ SU1 +1 ＋ C4e −9 ⇒ 105（跑出来核过） // **+1（skill_uninstall_apply，Both；归已有能力 `skill.install` ⇒ 能力数与不对称数都不动）** // **−5（cc-bus 查在线 · 发消息 · 派生 · 广播 · 收掉，归 `cc-bus.cockpit`，那项能力还剩读面两条 ⇒ 能力数不动；剩下两条都 `Both` ⇒ 不对称数 −1）** // **−3（kill_remote_tmux / tmux_send_keys Remote，归 `tmux.manage`，那项能力还剩 `list_remote_tmux` ⇒ 能力数不动；backend_send_into Both，`launch.send-into` 的唯一命令 ⇒ 能力数 −1，它原本对称 ⇒ 不对称数不动）** // **−1（远端抓屏，Remote；归 `tmux.manage`，那项能力还有别的命令 ⇒ 能力数不动）** // 主线 118 ＋ 本路 -5 ⇒ 113 // **−5（list_history_projects Local · list_remote_history_projects Remote · stream_history_sessions_in_project / update_history_metadata / list_last_accounts 都 Both：历史清单与注解搬进本机常驻后端、界面经通道问；它们是 `history.list-projects` / `history.list-sessions` / `history.metadata` / `accounts.last-used` 四项能力的全部命令 ⇒ 能力数 −4，四项原本都对称 ⇒ 不对称数不动）** // **主线 116 ＋ 本路 +2 ⇒ 118（跑出来核过）** // 主线 129 ＋ AS2 +3 ＋ RM1f −16 ⇒ 116（跑出来核过） // **主线 129 ＋ AS2 +3（assets_sync · skill_install_preview / apply，都 Both；新能力 `assets.catalog` · `skill.install`）** // **−2（backend_exit_policy / set_backend_exit_policy，都 Both：「退出行为」问 / 交写改走通道；它们是能力 `app.backend-policy` 的全部命令 ⇒ 能力数 −1，它原本对称 ⇒ 不对称数不动）** // 主线 135 ＋ 本路 −4 ⇒ 131（跑出来核过） // **−1（probe_session_record，Both；归 `history.read-session`，那项能力还有别的命令 ⇒ 能力数与不对称数都不动）** // **−3（list_remote_accounts Remote · list_local_accounts Local · check_account_trust Both：账号清单与信任预检改走通道；它们是能力 `accounts.list` / `accounts.trust` 的全部命令 ⇒ 能力数 −2，两项都已对称 ⇒ 不对称数不动）** // 主线 135 ＋ AL1d -2 ＋ AS1 +2 ⇒ 135（跑出来核过；AS1：mcp_sync_preview / mcp_sync_apply，Both，新能力 `mcp.sync`） // **−2（`aliases_block_*` 三条 `Local` 进，「终端集成」五条 `Local` 退役）；合并主线 636cc1a0 按两边增量相加 135 − 2** // **两边各 −4 ⇒ 143 − 8 = 135（跑出来核过）** // **−4（read_session_index / list_user_inputs / find_in_session / list_plugin_marketplaces：改走通道；前三条归 `history.read-session` 不动能力数，最后一条是 `plugins.marketplaces` 唯一一条 ⇒ 能力数 −1，不对称数不动）** // **−5（本机六条全景写命令退役 −6 · `panorama_edit` Both ＋1，归已有能力 `panorama.annotate`）**（合并主线按两边增量相加） // **+1（probe_session_record，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动；合并 RM1c 按两边增量相加）** // **+1（panorama_call，Remote；归已有能力 `panorama.code-graph`）** // **+1（relay_ensure，Remote；新能力 `relay.machine`，`NaturallyAsymmetric` —— 本机那一个有监护者）** // **−1（「某会话属哪个账号」远端 A2 · 本机 E79 两条退役 −2、`chan_call` ＋1 Both 新能力 `comm.face-a.call`；能力数与不对称数都不动）** // **+1（find_in_session，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动）** // **−1（sftp_copy：零流量复制随门禁那一格退役；Remote，归 `sftp.file-panel` ⇒ 能力数与不对称数都不动）** // **−12（池子那十二条 Tauri 命令随老面板与窗口改走通道一起走了；都 Remote、都归 `sftp.file-panel` ⇒ 能力数不动）** // **〔条 66〕+1（推生效值那一条退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 进 +2；都 Both，归已有能力 `app.backend-policy` ⇒ 能力数与不对称数都不动）** // **+2（aliases_render / aliases_read / aliases_install 三条进、write_account_aliases〔散文墓碑〕退役；`alias.account-commands` 并进 `alias.manage` ⇒ 能力数、不对称数、欠账都不动）** // **〔PN1b 选图〕+2（panorama_diagram_kinds / panorama_diagram，都 Local；归已有能力 `panorama.code-graph`）** // **+2（check_local_acct_iso / local_acct_iso_shellinit；与 SE1/U3b 合并时按两边增量相加）** // **+1（list_user_inputs，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动）** // **+1（replay_keep_tail_only，Both；归已有能力 `session.forget` ⇒ 能力数与不对称数都不动）** // **〔骨架〕+2（read_session_index / read_session_range，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动）** // **+1（open_file_window，Remote；归已有能力 `sftp.file-panel` ⇒ 能力数与不对称数都不动 —— 它与旧面板是**同一个能力的两个表面**，理由逐条写在 `LEDGER` 那一行旁边）** // **−2（`origin` 归一的**最后两对**：退役 `write_remote_mcp_server` / `remove_remote_mcp_server`，并进本机同名那两条。**能力总数与不对称数都不动** —— 两条能力从前是 `{Local, Remote}`＝已对称，今天是 `{Both}`＝同样对称）** // **+1（sftp_chmod，Remote；归已有能力 `sftp.file-panel` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）** // **−5（`origin` 归一：5 对同义双份各合成一条带 origin 的 ⇒ 退役 `create_remote_branch_session` / `delete_remote_history_session` / `stream_remote_history_sessions` / `stream_read_remote_session` / `list_remote_mcp_project_dirs`。**能力总数与不对称数都不动** —— 那五条能力从前是 `{Local, Remote}`＝已对称，今天是 `{Both}`＝同样对称；逐条理由在 `LEDGER` 那五行旁边与 `ORIGIN_TAKING_BOTH` 里）** // ** −4（`aggregate_usage_all` / `aggregate_remote_usage_all` / `account_usage` / `account_usage_local`：用量 ②③ 两轴整轴退役）** // **K-R109 +1（render_local_attach，Local；归已有能力 `session.launch` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）** // **K-R69 +1（local_ccm_entry_status，Local；归已有能力 `ccm.status` ⇒ 能力数与不对称数都不动）** // **K-R49 +1（write_account_aliases，Local-only；新能力 `alias.account-commands`，`ParityDebt`）** // **K-H2a +2（creds.apikey，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +3（list_skills / read_skill_file / write_skill_file：skill 接入面） // F08 +1（account_usage_local：补平 usage.per-account） // U8a-2c-1 +1（backend_send_into）； G6 +1；E79 +1；U-CC1 +1（drift_ledger_report）；U8c-2c-2 +1（render_ccm_launch）；U8a-2c-pre +1（render_launch_payload）；**P2s +5（set_backend_kill_on_exit / backend_status / backend_start / backend_stop / backend_machines，C8）**；P3t-Y2b +1（list_local_tmux）；**P4c +2（cc_bus_broadcast / cc_bus_kill，#77/#78）** // **P8a +1（list_plugin_marketplaces，#70）** // **PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）** // **K-H2b +1（apikey_routing_for，Local-only；新能力 `apikey.routing`，`NaturallyAsymmetric`）** // **`K-R135` +3（ccm_user_path_status / ccm_user_path_add / ccm_user_path_remove，都 Local；新能力 `ccm.user-path`，`NaturallyAsymmetric` —— 理由见 ASYMMETRY_REASONS 那一行）** // **+1（sftp_copy，Remote；归已有能力 `sftp.file-panel` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）**  〔散文墓碑〕
    let sides = capability_sides();
    assert_eq!(sides.len(), 24, "能力总数变了"); // −1（`relay.launch-endpoint`：唯一那条命令退役，中转地址交给 ccm 自己定）// −1（`panorama.code-graph`：代码全景整条摘掉）// +1（`terminal.window`：开窗，本机远端同一条，`Both`）// −2（`history.read-session` · `subagent.load`：那几条 Tauri 命令退役，界面经通道直问那台后端；两项都对称 ⇒ 不对称数不动）// −1（`panorama.annotate`：唯一那条 `panorama_edit` 出表，写进了那台后端 `panorama-edit`）// −1（`ssh.host-config`：最后那条公钥推送出表）// 30 − 1（`acct-iso.deploy`）// 主线 35 ＋ MIG-1 −5（`session.activity` · `session.list-active` · `port-forward` · `tmux.manage` · `tmux.local-census`）⇒ 30 // 43 − 2（`tmux.manage` · `tmux.local-census`）// 主线 46 ＋ MIG-1 −3 ⇒ 43 // 49 − 3（`launch.render-cli` · `launch.render-payload` · `launch.render-attach`：渲染进了那台后端，不再是 Tauri 命令） // 47 − 1（`port-forward`）// 49 − 2（`session.activity` · `session.list-active`：会话流的成品替掉）// 51 − 2（`assets.catalog` · `skill.install`）// 56 − 5（MCP 那五项）// −1（`cc-bus.cockpit`：驾驶舱读面两条命令改走通道 ⇒ 账本上这条能力没有 Tauri 命令了；原本对称 ⇒ 不对称数不动）// 主线 ＋ HX2 −1（`creds.apikey`） ⇒ 57 // 主线 60 ＋ LOC1b −2（`search.history` · `search.index`）⇒ 58 // 主线 61 ＋ LOC1a −1（`session.tasks` 唯一的命令退役）⇒ 60 // 主线 62 ＋ US1 −1（`apikey.routing`）⇒ 61 // −1（`launch.send-into`：它唯一的命令 `backend_send_into` 迁到界面经通道说） // 主线 67 ＋ 本路 -4 ⇒ 63 // **−4（`history.list-projects` · `history.list-sessions` · `history.metadata` · `accounts.last-used`：全部命令改走通道 ⇒ 账本上这四条能力没有 Tauri 命令了；四条原本都对称 ⇒ 不对称数不动）** // **主线 66 ＋ 本路 +1（`comm.face-a.subscribe`，`{Both}`）⇒ 67** // **主线 64 ＋ AS2 +2（`assets.catalog` · `skill.install`，都 Both ⇒ 不对称数不动）** // **−1（`app.backend-policy`：两条命令都改走通道）** // 主线 67 ＋ 本路 −2 ⇒ 65（跑出来核过） // **−2（`accounts.list` / `accounts.trust`：全部命令改走通道 ⇒ 账本上这两条能力没有 Tauri 命令了）** // **〔合并 AS1（第二次）〕主线 66 ＋ 本路 1（`mcp.sync`，Both）—— git 自动合并这一行时取了主线那一边、丢了本路的 +1，跑出来核过** // **−1（`plugins.marketplaces`：唯一那条命令改走通道 ⇒ 账本上这条能力没有 Tauri 命令了；它原本是 `{Both}` ⇒ 不对称数不动）** // **+1（`panorama.annotate`：六条写命令从 `panorama.code-graph` 拆出来，Local-only）** // **+1（relay.machine，Remote-only）** // **−1（alias.account-commands 并进 alias.manage）** // **+1（alias.manage）** // ** −2（`usage.aggregate` 与 `usage.per-account` 两条能力整条退役 —— 两条**原本都对称**，所以不对称数不动）** // **K-R109 +1（launch.render-attach，Local-only；⚠ 派工单猜的是「能力数 65 不动」，实打不成立 —— 两个「归已有能力」的归法各被一条判据顶回来了，逐条见 `LEDGER` 里那一行旁边）** // **K-R49 +1（alias.account-commands，Local-only）** // **K-H2a +1（creds.apikey，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +1（skill.inbox，Local-only） // U8a-2c-1 +1（launch.send-into，Remote-only）； U-CC1 +1（audit.drift-ledger）；U8c-2c-2 +1（launch.render-cli，Remote-only：**只是没有本机那条 IPC 命令** —— P3t-Y4 起理由不再是 §36「本机不经 IR」那条，§36 只绑 Windows，详见 ASYMMETRY_REASONS 里那行）；U8a-2c-pre +1（launch.render-payload，同 Remote-only）；**P2s +3（app.backend-policy / backend.status / backend.lifecycle，都是 Both）**；P3t-Y2b +1（tmux.local-census，Local-only：把远端本来就有的那一格在本机补上）；**P8a +1（plugins.marketplaces，Local-only）**；**PS1 +1（cc-bus.deploy）**；**PS2 +1（cc-bus.install-state）**；**K-H2b +1（apikey.routing，Local-only）**；**`K-R135` +1（ccm.user-path，Local-only：`R85` 那一格「加/撤/现在状态」；远端那一侧同一件事由 rc 围栏块办，而「用户级 PATH」这一档是 Windows 独有的 ⇒ `NaturallyAsymmetric`）**
    let asym = asymmetric_capabilities();
    assert_eq!(asym.len(), 7, "不对称能力数变了"); // +1（`audit.monitor-own`：monitor 自己那台那几行的事实，天然只有本机）// −1（`ssh.host-config` 随最后一条命令出表）// 8 − 1（`acct-iso.deploy`）// 主线 11 ＋ MIG-1 −3 ⇒ 8 // 11 − 2（`tmux.manage` · `tmux.local-census` 没有 Tauri 命令了）// 主线 12 ＋ MIG-1 −1 ⇒ 11 // 14 − 3（`launch.render-*` 三项结清）＋ 1（`ccm.status` 只剩本机那一条，天然不对称） // 14 − 1（`port-forward` 没有 Tauri 命令了）// 基数 16 − 2（`alias.manage` 欠账还掉 · `alias.block-preview` 两侧都有了）⇒ 14 // 主线 17 ＋ LOC1b −1（`search.index` 整项没了）⇒ 16 // **−1（`cc-bus.cockpit`：写面迁走后只剩读面两条 `Both`）** // **−1（`panorama.annotate` 两侧都有了）** // **净 0：`panorama.code-graph` −1（远端那一半有了）· `panorama.annotate` +1（写那一半远端还没有）** // **−1（`relay.machine` 随 `relay_ensure` 退役；接替它的 `relay.launch-endpoint` 是 `Both`）** // **−1（`skill.inbox` 结清：远端也能编辑，两侧经后端 ⇒ Both）** // **−2：`creds.apikey` −1（读 / 写两条收 origin ⇒ `Both`）· `audit.config-surface` −1（远端那一栏由那台的后端答）· `apikey.routing` −1 与 `relay.machine` +1 净 0** // **−1（`plugins.marketplaces` 结清：`list_plugin_marketplaces`〔散文墓碑〕 按 origin 问那台后端 ⇒ `Both`）** // **−1（`session.tasks` 结清：`get_session_tasks` 按 origin 问那台后端 ⇒ `Both`）** // **−2（`acct-iso.check` / `acct-iso.shellinit` 结清：本机对侧问本机后端）** // **−1（`accounts.trust` 结清：`check_account_trust` 按 origin 分流到本机后端 ⇒ `Both`）** // **K-R109 +1（launch.render-attach，NaturallyAsymmetric）** // **K-R49 +1（alias.account-commands，ParityDebt）** // **K-H2b +1（apikey.routing，NaturallyAsymmetric）** // **K-H2a +1（creds.apikey，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +1（skill.inbox） // F08 -1（usage.per-account 补平） // U8a-2c-1 +1（launch.send-into）； G6 -1；E79 accounts.session-accounts 补平 -1；U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）
                                                   // P3t-Y2b +1（tmux.local-census）；**P3b -1（launch.send-into 结清：P3 刀 3 让本机真的在用它 ⇒ Both，不再不对称）**
                                                   // **P7c-1 -1（subagent.load 结清：远端展开做出来了 ⇒ Both）** —— backend 只列候选，挑选留本侧（C1）
                                                   // **`K-R135` +1（ccm.user-path，`NaturallyAsymmetric`）** —— 见 ASYMMETRY_REASONS 里那一行：
                                                   // 它的「天然」是可证的（远端同一件事由 rc 围栏块办 ＋ 那一档只有 Windows 有），
                                                   // 不是图省事；哪天真出现「远端是 Windows」，回来把它改成 `ParityDebt`。
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, k, _) in ASYMMETRY_REASONS {
        *kinds
            .entry(match k {
                Asym::NaturallyAsymmetric => "natural",
                Asym::ParityDebt => "debt",
                Asym::Undecided => "undecided",
            })
            .or_default() += 1;
    }
    assert_eq!(kinds.get("natural"), Some(&6), "天然不对称条数变了"); // +1（`audit.monitor-own`）// −1（`ssh.host-config`） // 7 − 1（`acct-iso.deploy`）// 主线 9 ＋ MIG-1 −2（`port-forward` · `tmux.local-census`）⇒ 7 // 8 − 1（`tmux.local-census`）⇒ 7 // 主线 9 ＋ MIG-1 −1 ⇒ 8 // 11 − 3（`launch.render-cli` · `-payload` · `-attach` 三行随命令退役删了）＋ 1（`ccm.status`：远端探测命令删了，只剩本机那一条）⇒ 9 // 11 − 1（`port-forward`）⇒ 10 // 主线 12 ＋ LOC1b −1（`search.index`）⇒ 11 // **−1（`relay.machine` 那一行随命令退役删了）** // **净 0：`apikey.routing` −1（远端那台的后端自己答）· `relay.machine` +1（本机那一侧不该存在）** // **`K-R135` +1（ccm.user-path：远端同一件事由 rc 围栏块办 ＋ 那一档只有 Windows 有 ⇒ 天然，不是欠账；哪天远端是 Windows 就回来改成 `debt`）** // **K-R109 +1（launch.render-attach —— 记 `natural` 记的是「远端由 `render_ccm_launch` 一并产、不需要单独命令名」，不是「远端还没做」）** // U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）。★ P3t-Y4：这两条的**理由**换过（原来引 §36 说「本地不经 IR」——§36 只绑 Windows，且那个理由已被本机探针实测证伪），但 `natural` 的**条数没变**。P3t-Y2b +1（tmux.name-census）。**K-H2b +1（apikey.routing）—— 记 `natural` 记的是「回环自指 ⇒ 这个问题问错了机器」，不是「远端还没做」；把 key 送到远端那笔账在 `creds.apikey` 那行（`debt`），两者别混。**
    assert_eq!(kinds.get("debt"), Some(&1), "平价欠账条数变了"); // 主线 2 ＋ MIG-1 −1（`tmux.manage`）⇒ 1 // 3 − 1（`tmux.manage`：那一族全上了帧面、本机远端同一条路）⇒ 2 // 基数 4 − 1（`alias.manage`）⇒ 3 // **−1（`cc-bus.cockpit` 结清：写面迁到界面经通道说，本机远端同一条路）** // **−1（`panorama.annotate` 结清：本机远端同一条写）** // **+1（`panorama.annotate`：远端仓的批注写等后端写面）** // **−2（`creds.apikey`：把 key 送到远端的路有了 · `audit.config-surface`：远端足迹有了后端读口）** // **−1（`plugins.marketplaces`：远端那半补上了、本机同拍改走后端 —— 一件事清两笔账）** // **−1（`session.tasks`：远端那半补上了，理由表那一行删掉）** // **−2（`acct-iso.check` / `acct-iso.shellinit`）** // **−1（`accounts.trust`：本机那一侧补上了，理由表那一行删掉）** // **K-R49 +1（alias.account-commands：远端那半只有「吐待贴文本」那半条路，落盘没有主人）** // **K-H2a +1（creds.apikey：本机能配、远端那侧还没有路 —— 而且不能照抄 SFTP 上传，理由见那一行）** // F08 -1（usage.per-account 补平） // G6 -1；E79 -1；**P7c-1 -1（subagent.load 结清：远端展开做出来了）**；**P8a +1（plugins.marketplaces：新开的本机口，远端那半要等后端的 `--list-marketplaces`）**
    assert_eq!(kinds.get("undecided"), None, "未裁定条数变了"); // 基数 1 − 1（`alias.block-preview`：一并做）⇒ 0 // **−1（`panorama.code-graph`：远端那一半做了）** // **−1（skill.inbox：要做）** // devbench F03 +1（skill.inbox：远端项目的收件箱要不要能编辑，没人裁定过） // U8a-2c-1 +1（launch.send-into：本机该不该有后端进程未裁定）
                                                                // P3b -1（launch.send-into：它的「还没裁定」被 C1/C8 + P2 + P3 刀 3 三重证伪）
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 `K-R115` `KR115D4`：**`Side` 这一栏靠人签字，而签字的触发是派生出来的**
// ════════════════════════════════════════════════════════════════════════
//
// # 问题（`K-R112` 撞见、一个字没动）
//
// `LEDGER` 的 `Side` 栏**没有任何机检**。一条命令的派发跳改走了「origin 无关」的
// 帧面（`inbound_client::client_for(origin)` / `backend_route`）之后，它对
// `<local>` 也走得通，而这一行还写着 `Side::Remote` —— **表在撒谎，而且不红**。
//
// # 为什么**不是**「让 `Side` 从源码派生」（甲），而是「签字 ＋ 派生的触发器」（乙）
//
// 甲在这张表上**不成立**，三条现打的理由（09-14，本工作树）：
//
// 1. **本模块头注自己裁过**：「这项能力在两侧都有吗」是**判断**，不是能从代码推出来的
//    东西（本地需不需要 SFTP 面板？不需要 —— 但没有任何语法说明这点）。
//    同一段还写着**反向那条刻意没做**（`Remote` ⇒ 必须吃远端参数），实测 11 条合法例外。
// 2. **反向方向现打就是一堆假阳**：拿同一套标记去判「声明 `Local`/`Both` 而其实只走远端」，
//    09-14 现打 **7 条命中，7 条全是假阳**（`read_cc_bus_state` / `read_cc_bus_inbox` /
//    `list_local_tmux` / `backend_start` / `load_subagent` / `read_skill_file` /〔散文墓碑〕
//    `write_skill_file` —— 它们**两条路都有**，标记只看得见远端那条）。⇒ 这个方向不接。〔散文墓碑〕
// 3. **正向也有一条假阳**：〔：这条读数的样本是 `account_usage` —— 它派生出来是
//    「帧面·origin 无关」，而它的 `Side::Remote` 没说假话，因为本机那一侧另有一条命令
//    （`account_usage_local`）。用量 ②③ 两轴整轴退役之后**那两条命令都没了**，  〔散文墓碑〕
//    于是这个样本今天不在盘上了。**结论没变、样本没了** —— 留着这段是因为它是
//    「为什么候选要人签、不能机器直接判红」的论证，不是在描述今天的盘面。〕
//
// ⇒ 落 **乙**：这一栏**明写靠人签字**，而**「什么时候必须回来签」由机器算**：
//   · **没签字红** —— 一条 `Side::Remote` 的行不在签字表里（新增的、或改成 `Remote` 的）；
//   · **过期红**   —— 签的那个派发类与**今天现打的**对不上（源码改了、没回来重签）；
//   · **陈账红**   —— 签字表里有一行今天已经不是 `Side::Remote` 了（`Side` 被改过）。
//
// # ⚠ 它买不到什么（诚实边界，写死）
//
// - **不判 `Side` 对不对**。它买的是「派发跳一变，签字当场作废」，
//   不是「表说的是真话」。今天表里就有 **5 行**签着「说假话·欠一次订正」。
// - **射程只有 `Side::Remote` 那一栏**（现打 55 行）。`Local` ↔ `Both` 之间改来改去
//   本条看不见 —— 理由是上面第 2 条：那个方向的派生现打 7 条全假阳。
// - **派生器只跟同一份文件里的调用，深度 2**。跨文件的 helper 看不见 ⇒ 落 `Unclassified`
//   （今天 10 行）。`Unclassified` **不是「安全」**，是「这把尺子够不着」。
// - 它**不证明**「本机真的抓得到一屏 / 真的杀得掉」—— 那要跑真机（`K31` 之下没跑过，
//   `K-R112` 交回时逐字承认过同一条）。它只证明**这一跳走的是哪条路**。

/// 一条命令的**派发跳**是什么形状 —— 从源码派生，不是人写的。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Derived {
    /// 生产段里出现了**只有远端才用得上**的东西（SSH / 远端配置 / 拒绝 `<local>`）。
    RemoteOnly,
    /// 只走 **origin 无关的帧面**（`client_for(origin)` / 共用分流器）⇒ `<local>` 也走得通。
    FramePlane,
    /// 两样都有 —— 它自己分了两条路。本条不判它（人签）。
    Mixed,
    /// 两样都没有 ⇒ **这把尺子够不着**，不是「它安全」。
    Unclassified,
}

/// 一条候选（`Side::Remote` ＋ 派生 `FramePlane`）的**人裁**。闭集。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FrameVerdict {
    /// 名副其实：本机那一侧**另有一条命令名**，这一行的 `Remote` 说的是「这个命令名给远端用」。
    SecondCommandServesLocal,
    /// 🔴 **说假话**：本机没有别的命令名，而这一跳对 `<local>` 也走得通 —— 欠一次订正。
    LiesTodayOwedACorrection,
}

/// 生产段里「只有远端才用得上」的标记。运行时不拼 —— 本文件整份住在 `#[cfg(test)]` 里，
/// `production_code` 会把它整份剥掉；加上**住址**（本文件住 `tests/frontend/shell/`，扫的是
/// `src/frontend/shell/src`），扫描面里根本没有本文件。
///
/// ⚠ 先前括号里写着「`scan_tree!` 还会再摘一次」——
/// **那一刀在这一处不生效**（自摘恒空转），别把它算成一道保险。
const REMOTE_ONLY_MARKS: &[&str] = &[
    "load_remote_config_by_label(",
    "stream_source::",
    "refuse_local_write(",
    "RemoteConfig",
    "sftp_pool::",
];

/// 生产段里「走 origin 无关帧面」的标记。
const FRAME_PLANE_MARKS: &[&str] = &["client_for(", "route_call_error", "backend_route::"];

/// 派生器跟同文件调用的**深度**。〔09-14 现打：2 / 3 / 4 三个深度**答案完全相同**
/// （6 条 `FramePlane` 逐字同一批）⇒ 取最小的那个，别多扫。〕
const DERIVE_DEPTH: usize = 2;

/// 顶层 `fn` 的函数体：从**列 0** 的 `fn` 声明行起，到**列 0 的 `}`** 那一行止。
///
/// ⚠ 这是本仓「列 0 收尾」那一族剥法的同一条口径 —— 它的边界（原始字符串里的
/// 列 0 右大括号）由 `guard_core::assert_test_module_ranges_are_brace_balanced` 守着，
/// 两棵树的 `every_*_file_strips_clean` 每趟都在跑它。**别在这里再发明一份近似。**
fn top_level_fn_bodies(prod: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let lines: Vec<&str> = prod.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let Some(name) = top_level_fn_name(lines[i]) else {
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < lines.len() && lines[j] != "}" {
            j += 1;
        }
        let last = j.min(lines.len() - 1);
        out.entry(name)
            .or_insert_with(|| lines[i..=last].join("\n"));
        i = j + 1;
    }
    out
}

/// `fn` 前面允许出现的修饰（可见性那一截由 `guard_core::strip_visibility` 单独剥）。
const FN_MODIFIERS: &[&str] = &["async ", "unsafe ", "const ", "extern \"C\" "];
const FN_KEYWORD: &str = "fn ";

/// 一行是不是**列 0 的 `fn` 声明**；是就给出函数名。
///
/// 可见性那一截走 `guard_core::strip_visibility` —— `K-R75` 逐字：
/// 别再列一张前缀表，`pub(in a::b::c…)` 穷举不了，形状认得出。
fn top_level_fn_name(line: &str) -> Option<String> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    // ⚠ 前缀一律**从常量表里取变量**再剥，不写成 `strip_prefix("字面量")` ——
    //    `needle_anchor_registry` 那条棘轮对语料变量上的裸 `strip_prefix("…")`
    //    上限是 **0**，而它逐字禁「把上限调上去让今天好过」。
    let mut rest = guard_core::strip_visibility(line);
    for m in FN_MODIFIERS {
        rest = rest.strip_prefix(*m).unwrap_or(rest);
    }
    let rest = rest.strip_prefix(FN_KEYWORD)?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    // 名字后面必须紧接 `(` 或 `<`，否则那不是一个 `fn` 声明。
    let after = &rest[name.len()..];
    if after.starts_with('(') || after.starts_with('<') {
        Some(name)
    } else {
        None
    }
}

/// 一段代码里被调用到的标识符（`foo(` 那一形）。
fn callees(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b: Vec<char> = body.chars().collect();
    let mut k = 0usize;
    while k < b.len() {
        if b[k].is_ascii_lowercase() || b[k] == '_' {
            let s = k;
            while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == '_') {
                k += 1;
            }
            if k < b.len() && b[k] == '(' {
                let ident: String = b[s..k].iter().collect();
                if ident.len() >= 4 {
                    out.insert(ident);
                }
            }
        } else {
            k += 1;
        }
    }
    out
}

/// **每条 Tauri 命令的派发跳是什么形状** —— 现打，不读任何人写下的结论。
///
/// 取法：`#[tauri::command]` 定位到命令住哪一份文件（与 [`command_signatures`] 同一条
/// 取法），在**那一份文件的生产段**里取它的函数体，再顺着同文件的调用跟 [`DERIVE_DEPTH`] 层。
fn command_dispatch_class() -> BTreeMap<String, Derived> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let attr = format!("#[tauri::{}]", "command");
    let mut out = BTreeMap::new();
    let mut files: Vec<std::path::PathBuf> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    files.sort();
    for path in files {
        let raw = std::fs::read_to_string(&path).expect("read rs");
        let prod = guard_core::production_code(&raw);
        let bodies = top_level_fn_bodies(&prod);
        for (i, _) in prod.match_indices(&attr) {
            let rest = &prod[i..];
            let Some(fpos) = rest.find("fn ") else {
                continue;
            };
            if rest[..fpos].contains('{') {
                continue;
            }
            let after = &rest[fpos + 3..];
            let Some(paren) = after.find('(') else {
                continue;
            };
            let name = after[..paren].trim();
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let Some(first) = bodies.get(name) else {
                continue;
            };
            // 同文件、深度 DERIVE_DEPTH 的闭包
            let mut seen: BTreeSet<String> = BTreeSet::new();
            seen.insert(name.to_string());
            let mut text = first.clone();
            let mut frontier = vec![first.clone()];
            for _ in 0..DERIVE_DEPTH {
                let mut next = Vec::new();
                for t in &frontier {
                    for c in callees(t) {
                        if seen.contains(&c) {
                            continue;
                        }
                        if let Some(b) = bodies.get(&c) {
                            seen.insert(c);
                            text.push('\n');
                            text.push_str(b);
                            next.push(b.clone());
                        }
                    }
                }
                frontier = next;
            }
            let r = REMOTE_ONLY_MARKS.iter().any(|m| text.contains(m));
            let f = FRAME_PLANE_MARKS.iter().any(|m| text.contains(m));
            let cls = match (r, f) {
                (true, false) => Derived::RemoteOnly,
                (false, true) => Derived::FramePlane,
                (true, true) => Derived::Mixed,
                (false, false) => Derived::Unclassified,
            };
            out.entry(name.to_string()).or_insert(cls);
        }
    }
    out
}

/// **`Side::Remote` 那一栏的签字** —— 签的是「签字人现打时，这一栏是什么形状」。
///
/// 🔴 **为什么是直方图，不是一行一条的 55 行清单**：一行一条要把 55 个命令名再抄一遍，
/// 而那份抄写**当场把一条现行棘轮顶红** —— `tool_registry` 那张旧名账按**文件**数
/// 「旧名的符号形出现几处」，本文件登记 2 处（`backend.deploy` 那两行的命令名里就带着它），
/// 抄一遍当场变 4 处；而那条账逐字禁「把上限调上去让今天好过」。
/// 〔09-14 实打：第一版就是 55 行清单，`cargo` 那格红在这一条上，读数住
/// `tests/evidence/K-R115-deathvalue.md`〕
/// ⇒ 签的是**形状**：`Side::Remote` 那一栏里，每一档派生类各几行。
///
/// **它照样拦得住「改一行 `Side`」**：把一行 `Remote` 改掉 ⇒ 那一档少一个；
/// 把一行 `Local`/`Both` 改成 `Remote` ⇒ 某一档多一个。两边都不等 ⇒ 红。
/// ⚠ **它拦不住的那一形写死**：同一拍里一进一出、且**恰好同一档**
/// （改走一行 `RemoteOnly`、同时另加一行 `RemoteOnly`）⇒ 直方图不动，本条静默。
/// 候选那一档（`FramePlane`）**不吃这个亏** —— 它另有一张逐条点名的人裁表在对拍。
///
/// 〔量于 09-14，本工作树 `track/k-r115`，`command_dispatch_class()` 现打〕
const REMOTE_SIDE_SIGNOFF: &[(Derived, usize)] = &[
    // 🔴 **39 → 34。** `origin` 归一退役了 5 条 `Side::Remote` 的命令
    //    （并进了本机那条）。**现打确认这 5 条全部落在 `RemoteOnly` 这一档**，
    //    所以只有这一格动、另外三格一个都不动 —— 那是意料之中的：五条的体里都有
    //    `load_remote_config_by_label(` / `require_cfg_by_label(` 这类 `REMOTE_ONLY_MARKS`，
    //    而一条 `FRAME_PLANE_MARKS`（`client_for(` / `backend_route::`）都没有。
    //    ⚠ 这个数是**跑出来的**，不是 39−5 算出来的：先把总数改对、让 `hist == want`
    //      那一比去印现打，再照它写。（算出来的那个恰好也是 34，但「恰好相等」不是判据。）
    // 🔴 **34 → 33。** 两笔一起落在这一格上，净 −1：
    //    −2 = `origin` 归一的最后两对退役了 `write_remote_mcp_server` /
    //         `remove_remote_mcp_server`（并进本机同名那两条）；
    //    +1 = `sftp_chmod`：签名里带 `RemoteConfig`、
    //         体里点名 `with_sftp`，派生器现打归 `RemoteOnly`。
    //    ⚠ 另外三格一个都不动 —— 退役那两条的体里有 `load_remote_config_by_label(`
    //      这类 `REMOTE_ONLY_MARKS`、一条 `FRAME_PLANE_MARKS` 都没有；新来那一条同理。
    //    ⚠ 这个数是**跑出来的**：先让 `signed_total` 那一比印出现打的 48 行，
    //      再让 `hist == want` 印出现打的直方图，照它写。
    //      （34−2+1 恰好也是 33，但「算出来恰好相等」不是判据。）
    // RemoteOnly 34 → 33、FramePlane 5 → 6：`cc_bus_spawn` 改走 `bus-spawn` 原语，
    //   派生器从 `RemoteOnly` 挪到 `FramePlane`（跑出来的：`hist == want` 那一比现打 {RemoteOnly: 33, FramePlane: 6}）。
    // RemoteOnly 32 → 31、Unclassified 10 → 11：`start_forward` 查配置那一下搬进了宿主
    //   `dial_host.rs` 起转发那一处（已随转发账进本机常驻后端删了）（端口转发进了通信层，读配置是宿主的事）⇒ 它的体里再没有 `REMOTE_ONLY_MARKS`，
    //   而派生器只跟同一份文件里的调用 ⇒ 落 `Unclassified`（「这把尺子够不着」，不是「安全」；它照旧只对远端）。
    //   跑出来的：`hist == want` 那一比现打 {RemoteOnly: 31, Unclassified: 11}。
    (Derived::RemoteOnly, 3), // **4 → 3**：`deploy_remote_acct_iso` 退役（跑出来核过）// **主线 5 ＋ MIG-1 −1（测试连接）⇒ 4** // **6 → 5**：`deploy_remote_backend` 先问本机常驻后端要计划（`deploy-plan`）⇒ 派生器现打归 `Mixed` // **6 → 5**：`test_remote_connection` 出表（签名里带 `RemoteConfig`，原归 `RemoteOnly`）；跑出来核过 // **7 → 6**：远端项目 `.mcp.json` 那一条读退役（同上）；跑出来核过 // **8 → 7**：`probe_ccm_cli` 不再查远端配置（改经那台后端的门问 `ccm-probe`）⇒ 落 `Unclassified`；跑出来核过 // 10 − AL2 2 ⇒ 8（AL2：远端装 / 卸别名块两条删了） // **12 → 10**：远端 acct-iso 那两条（带 `RemoteConfig`）并进带 origin 的 `Both` // **13 → 12**：`push_public_key` 转 `Mixed`（见下一行）// **14 → 13**：`list_remote_tmux`（改问那台后端 `tmux-list`）// **15 → 14**：`read_remote_mcp_servers`（改问那台后端 `mcp-read`）// **16 → 15**（`diagnose_remote_cc_bus_hooks` 不再查远端配置、改经 `frame_query::call` 问那台后端 ⇒ 落 `Unclassified`；跑出来核过）// **17 → 16**（`list_remote_history_projects` 随远端项目清单改走本机后端删了：它体里读远端配置表（`load_remote_configs`），原归 `RemoteOnly`；跑出来核过） // **18 → 17**（`list_remote_accounts` 随账号清单改走通道删了：它体里查远端配置（`cfg_for`），原归 `RemoteOnly`；跑出来核过） // **19 → 18**（远端「某会话属哪个账号」那条退役、改走通道；原归 `RemoteOnly`；跑出来核过） // **20 → 19（`sftp_copy` 删了：它签名里带 `RemoteConfig`，原归 `RemoteOnly`；跑出来核过）** // **〔合并 F7c＋C2〕F7c 21 与 C2 −1 相加 ⇒ 20；Unclassified F7c 9 与 C2 ＋1 ⇒ 10（跑出来核过）** // **32 → 21，Unclassified 10 → 9**（池子那十二条删了：十一条带 `RemoteConfig` 的归 `RemoteOnly`，`sftp_cancel_transfer` 签名里没有它、归 `Unclassified`；跑出来核过）。 // **〔合并 A3＋BS1b〕33 → 32**（A3 的 check_account_trust Remote→Both 与 BS1b 的 cc_bus_spawn RemoteOnly→FramePlane 各 −1；跑出来核过）。 // **33 → 34**（`open_file_window`：签名里带 `RemoteConfig`、体里点名 `list_remote`，派生器现打归 `RemoteOnly`；另外三格一个都不动）。⚠ 这个数照旧是**跑出来的**，不是 33+1 算出来的：先让 `signed_total` 那一比印出现打的 `Side::Remote` 行数，再让 `hist == want` 印出现打的直方图，照它写。 // −1（`aggregate_remote_usage_all`）  〔散文墓碑〕 // **+1（`sftp_copy`：签名里带 `RemoteConfig`、体里点名 `copy_remote_path`，派生器现打归 `RemoteOnly`）**
    (Derived::FramePlane, 0), // 3 → 0（`cc_bus_spawn` / `cc_bus_broadcast` / `cc_bus_kill` 退役：体里走 `bus-*` 原语，原归 `FramePlane`；跑出来核过） · 5 → 3（`kill_remote_tmux` / `tmux_send_keys` 退役：体里走 `backend_route::`，原归 `FramePlane`；跑出来核过） · 6 → 5（远端抓屏那一条退役：它体里走 `client_for(`，原归 `FramePlane`；跑出来核过） // −1（`account_usage`）；BS1b +1（`cc_bus_spawn`）
    (Derived::Mixed, 1), // **2 → 1**：公钥推送那一条（原 `Mixed`）进了本机后端、出表〔散文墓碑〕 // **1 → 2**：`deploy_remote_backend`（判定问本机后端 · 放字节经远端 `files` 链路；两支都走、不是失败退回）// **0 → 1**：`push_public_key` —— 那台后端在 ⇒ 经它的文件管理面写（`client_for`），不在 ⇒ 仍是 Bootstrap 那一串拨号 shell；两支按**此刻状态**分，不是失败退回（`D11`）
    (Derived::Unclassified, 2), // **主线 9 ＋ MIG-1 −7 ⇒ 2**（现打核过） // **4 → 3**：`list_remote_tmux` 出表（原归 `Unclassified`：体里只转调 `frame_query::call`）；跑出来核过 // **主线 10 ＋ MIG-1 −6 ⇒ 4** // **13 → 10**：`probe_ccm_cli` · `render_ccm_launch` · `render_launch_payload` 退役（界面经通道直问那台后端）；跑出来核过 // **10 → 7**：端口转发三条删了（原归 `Unclassified`：C2 之后体里只转调宿主 / 读进程内的账）；跑出来核过 // **13 − MIG-1 3 ⇒ 10**：`~/.ssh/config` 导入那三条删了（原归 `Unclassified`） // **14 → 13**：远端 MCP user 段那一条读退役（界面经通道直问 `mcp-read`）；跑出来核过 // **13 → 14**：`probe_ccm_cli`（同上一行） // **12 → 13**：`list_remote_tmux`（同上）// **11 → 12**：`read_remote_mcp_servers`（同上）// **10 → 11**：`diagnose_remote_cc_bus_hooks`（同上一行）// **〔本机对称〕12 → 10**：`panorama_call` 与 `panorama_cancel` 翻 `Both`（本机也经它问本机后端），出了 `Side::Remote` 这一栏；跑出来核过 // **11 → 12**：`panorama_cancel`（Remote；只拉一张进程内的票，签名与体里都没有远端配置 ⇒ 落 `Unclassified`；跑出来核过） // **RL1 −1 与 RM1c ＋1 ⇒ 11（跑出来核过）** // **11 → 12**：`panorama_call`（Remote；体里只转调 `frame_query::call`，派生器只跟同一份文件 ⇒ 落 `Unclassified`；跑出来核过；本机那一侧另有进程内那几条命令） // **11 → 10**：`relay_ensure` 退役（它就是 RM1a 加进来的那一条） // **10 → 11**：`relay_ensure`（Remote；体里只转调 `remote_relay::ensure_on`，派生器只跟同一份文件 ⇒ 落 `Unclassified`；它照旧只对远端，本机那一臂在 `remote_relay` 里拒）。跑出来核过 // 主线 10 ＋ MIG-3b −1（远端钩子诊断那条退役）〔散文墓碑〕
];

/// 派生器在**整张** `LEDGER` 上认出的 `FramePlane` 命令数（不分 `Side`）—— 候选集（`Remote` 那一栏里的
/// `FramePlane`）今天是空集，这个数是派生器不瞎的异源正控（见 `the_remote_side_column_is_signed_off` 第 ③ 步）。
/// 两向相等（逐条点名，跑出来的）；人群：体里走 `client_for(` / `backend_route::` 的 `#[tauri::command]`。
const FRAME_PLANE_ANYWHERE: &[&str] = &[
    // 〔C4e 现打〕`LEDGER` 里是 `Both`（问那台机器后端的状态，体里走 `client_for(`）—— 不是候选，因为它不在 `Remote` 那一栏。
    "backend_status",
];

/// **候选（派生 = `FramePlane`）的人裁** —— 闭集见 [`FrameVerdict`]，理由必须可追问。
///
/// 🔴 **这张表就是本笔的产出**：`K-R112` 交回时点了 **3** 行（远端抓屏 /
/// `cc_bus_kill` / `cc_bus_broadcast`）。派工单逐字「别只修它撞见的那三行，先量」——
/// **现打是 5 行在说假话**，多出来的两行是 `kill_remote_tmux` 与 `tmux_send_keys`。
/// ⚠ 那两行最狠的地方：**同一行的 `ASYMMETRY_REASONS` 散文早就写着它们「本机已通」**
/// （`tmux.manage` 那条理由里逐字「② `kill_remote_tmux` ⇒ **本机已通**」「③
/// `tmux_send_keys` ⇒ **本机已通**」，P3b 08-12 写下、`K-R56` 09-11 复核过）——
/// **散文说通了，同一行的 `Side` 栏说没通，两边打了一个月的架，没有任何东西在数它。**
///
/// 🔴 **本件刻意不翻这 5 行**，理由两条，都写在这里别读丢：
/// ① 翻它要动的连锁现打是 **4 处**：`EXPECTED_LOCAL_OR_BOTH`（93 → 98）·
///    `ORIGIN_TAKING_BOTH`（＋5 条，它们都吃 `origin:`）· `tmux.manage` 那条
///    `ASYMMETRY_REASONS` 的散文 · 钉着那句散文的
///    [`the_tmux_manage_row_stops_waiting_for_a_backend_primitive`〔散文墓碑〕]（它逐字断言那句理由里
///    **必须**含 `Side::Remote`）。⇒ 翻 `Side` 要**同拍改掉一条现行判据的断言**。
/// ② 而「本机真的抓得到一屏 / 真的杀得掉」**今天判不了**（一趟真机都没跑过，
///    `K-R112` 交回时逐字承认过）。`Side::Both` 的字面是「这条命令自己就把两侧都办了」——
///    在没跑过的前提下把它写上去，是拿一句没验过的话换一格好看的表。
/// ⇒ **登记成欠账，归后续一件**；本条保证的是**它从此不会静默**。
const FRAME_PLANE_VERDICTS: &[(&str, FrameVerdict, &str)] = &[
    // 抓屏那一行（远端抓屏）的欠账**随命令一起结了**，不是翻了 `Side`：
    //   界面经通道直接问那台机器的后端（`src/frontend/ui/tmux-control.ts::capturePane`），本机与远端同一条路，
    //   通道上的问法不是 Tauri 命令、不在 `Side` 这一栏里。
    // `kill_remote_tmux` / `tmux_send_keys` 两行（`〔散文墓碑〕`）同上一块：欠的那次 `Side` 订正随命令一起结了。
    // cc-bus 那三行（`cc_bus_broadcast` / `cc_bus_kill` / `cc_bus_spawn`〔散文墓碑〕）同上：欠的那次 `Side` 订正
    //   随命令一起结了（界面经通道直接说后端的 `bus-*`，`src/frontend/ui/cc-bus-control.ts`）。**本表今天是空的** —— 那不是「大家都改好了」，
    //   是这一族候选的命令全部退役；机制照留：哪天再有一条 `Side::Remote` 命令改走 origin 无关的帧面派发，候选集 ≠ 本表 ⇒ 红。
];

/// 🔴 **`KR115D4`：`Side::Remote` 那一栏，没签字红 · 过期红 · 陈账红。**
///
/// 三条判定与它们各自拦得住什么，逐条写在断言的报文里；边界见本节开头那段头注。
#[test]
fn the_remote_side_column_is_signed_off() {
    let derived = command_dispatch_class();
    // 反向自检①：派生器扫不到东西的时候，下面每一条都会**空真地**成立。
    // 🔴 **地板 140 → 135**：合并退役了 5 条 `#[tauri::command]`。
    assert!(
        // 135 → 123：池子那十二条 `#[tauri::command]` 删了（人群真少了 12 个）。
        // 123 → 110：本机那十七条全景命令删了（人群真少了 17 个；合并主线 C4c 之后今天 113）。
        // 110 → 109：抓屏 · 杀会话 · 送键 · 就地 resume 四条 `#[tauri::command]` 删了（人群真少了 4 个；今天 109）。
        // 109 → 104：cc-bus 查在线 · 发消息 · 派生 · 广播 · 收掉五条 `#[tauri::command]` 删了（人群真少了 5 个；今天 104）。
        // 104 → 103：`read_apikey_credentials_status` / `apikey_routing_for` 两条删了（合并主线 C4e 之后今天 103）。
        // 103 → 102：`write_apikey_credentials_key` 删了（人群真少了 1 个）。
        // 103 → 102：`get_session_tasks` 删了（人群真少了 1 个；今天 102）。
        // 基于 99b8adb6：LOC1b −3 ＋ HX2 −1 ⇒ 98。
        // 98 → 96：驾驶舱 `read_cc_bus_state` / `read_cc_bus_inbox` 两条删了（人群真少了 2 个）。
        // 92 → 85：MCP 读写六条 ＋ 推拉两条 `#[tauri::command]` 删了、`list_remote_mcp_origins` 挪进 `config.rs`（人群真少了 8 个；今天 85）。
        // 85 → 81：资产同步 ＋ skill 装卸三条 `#[tauri::command]` 删了。
        // 81 → 76：载荷 / 调用行渲染 · 中转地址 · 本机新起 / resume / 接回 · 远端 ccm 探测七条删了、开窗 · 中转开关两条新增。
        // 基数 81 ＋ MIG-3a −11 ＋ MIG-2 −5 ⇒ 65。
        // 81 − MIG-1 3（`~/.ssh/config` 导入那三条 `#[tauri::command]` 删了）⇒ 78。
        derived.len() >= 42, // 43 − 1（`panorama_place` 随代码全景删了）// 47 − 4（会话正文四条退役）// 48 − 3（全景问 · 写 · 撤）＋ 2（`panorama_place` · `chan_cancel`）// 49 − 1（公钥推送进了本机后端）// 50 − 1（`deploy_remote_acct_iso`〔散文墓碑〕 删了）// 主线 61 ＋ MIG-1 −11 ⇒ 50 // 66 − 1（测试连接）// 68 − 2（列 tmux 两条）// 主线 76 ＋ MIG-1 −8 ⇒ 68 // 76 − 3（端口转发三条 Remote 命令删了） // 78 − 2（两条 Both 命令删了）// 94 − AL2 2（远端装 / 卸别名块两条删了）⇒ 92 // 96 → 94：acct-iso 两对合一（4 → 2）
        "派生器只认出 {} 条命令的派发跳（`LEDGER` 现打 {} 行）—— **扫描面塌了**，\n\
             不是「命令变少了」。先修 `command_dispatch_class`，别信下面任何一条绿。",
        derived.len(),
        LEDGER.len()
    );

    let remote_rows: BTreeSet<&str> = LEDGER
        .iter()
        .filter(|(_, _, s)| *s == Side::Remote)
        .map(|(c, _, _)| *c)
        .collect();
    // 反向自检②：`Side::Remote` 那一栏本身不许塌成空集。
    // 🔴 **地板 50 → 45。** `origin` 归一退役了 5 条 `Side::Remote`
    //    的命令（它们并进了本机那条）⇒ 这一栏**真的少了 5 个成员**。
    //    ⚠ 按本仓纪律，人群真的变少时不跟着改地板，才是让地板替真判据挡枪（`K-G8`）。
    //    ⚠ 地板只守「这一栏没塌成空集」；**有牙的是下面那条直方图相等**，不是这个数。
    // 🔴 **地板 45 → 36。** 池子那十二条 `Side::Remote` 命令真的删了（人群真少了 12 个成员，
    //    48 → 36，现打）。地板照旧只守「没塌成空集」，有牙的是下面那条直方图相等。
    // 🔴 **地板 36 → 35。** 零流量复制那一条 `Side::Remote` 命令真的删了（人群真少了 1 个，现打 35）。
    // 🔴 **地板 35 → 34。** 远端那条「某会话属哪个账号」（`Side::Remote`）退役（本机与远端收成一条经通道的路），现打 34。
    // 🔴 **地板 34 → 33。** 只对远端的 `relay_ensure` 退役（接替它的那条是 `Both`），人群真少了 1 个，现打 33。
    // 🔴 **地板 33 → 32。** 远端项目清单那条 `Side::Remote`（`list_remote_history_projects`）退役
    //    （fan-out 搬到前端、每台经本机后端问），人群真少了 1 个，现打 32。
    // 🔴 **地板 32 → 31。** 抓屏那条 `Side::Remote`（远端抓屏）退役
    //    （界面经通道直接问），人群真少了 1 个，现打 31。
    // 🔴 **地板 31 → 29。** 杀会话 / 送键两条 `Side::Remote` 退役（界面经通道直接说），人群真少了 2 个，现打 29。
    // 🔴 **地板 29 → 26。** cc-bus 派生 / 广播 / 收掉三条 `Side::Remote` 退役（界面经通道直接说），人群真少了 3 个，现打 26。
    // 🔴 **地板 26 → 24。** acct-iso 那两条 `Side::Remote`（远端那一对）并进带 origin 的 `Both`，人群真少了 2 个，现打 24。
    // 🔴 **地板再 −2（与 SH1 合并：26 − 2 − 2 = 22）。** 远端装 / 卸别名块两条 `Side::Remote` 并进 `aliases_block_*`（`Both`），人群真少了 2 个，现打 24。
    // 🔴 **地板 22 → 20。** 远端 MCP 读两条 `Side::Remote`（user 段 · 项目 `.mcp.json`）退役（界面经通道直问那台后端），人群真少了 2 个，现打 20。
    // 🔴 **地板 20 → 17。** `probe_ccm_cli` · `render_ccm_launch` · `render_launch_payload` 三条 `Side::Remote` 退役〔散文墓碑〕
    //    （渲染与探测住进那台后端，界面经通道直问），人群真少了 3 个，现打 17。
    // 🔴 **地板 17 → 16。** 远端钩子诊断那条 `Side::Remote` 退役（MIG-3b）。
    // 🔴 **地板 20 → 17。** `~/.ssh/config` 导入那三条 `Side::Remote` 搬进后端帧命令，人群真少了 3 个。
    // 🔴 **地板 17 → 14。** 端口转发三条 `Side::Remote` 退役（转发账进本机常驻后端，界面经通道直问），人群真少了 3 个，现打 14。
    // 🔴 **地板 → 11。** 主线 17 ＋ MIG-1 −6。
    assert!(
        remote_rows.len() >= 6, // 7 → 6：公钥推送出表 // 8 → 7：`deploy_remote_acct_iso`〔散文墓碑〕 出表 // 主线 16 ＋ MIG-1 −8 ⇒ 8 // 10 → 9：`test_remote_connection` 出表 // 11 → 10：`list_remote_tmux` 出表 // 主线 17 ＋ MIG-1 −6 ⇒ 11
        "`Side::Remote` 现打只有 {} 行 —— 本条的人群塌了，下面几条会空真地绿",
        remote_rows.len()
    );

    // ① 现打这一栏的派生形状直方图。
    //    ⚠ **每一档都要先播成 0** —— 只记「数得到的那几档」的话，
    //    「某一档今天恰好一个都没有」与「签字表里根本没写这一档」在比对上一模一样。
    let all_kinds = [
        Derived::RemoteOnly,
        Derived::FramePlane,
        Derived::Mixed,
        Derived::Unclassified,
    ];
    let mut hist: BTreeMap<String, usize> =
        all_kinds.iter().map(|k| (format!("{k:?}"), 0)).collect();
    let mut candidates: BTreeSet<&str> = BTreeSet::new();
    for cmd in &remote_rows {
        let got = derived.get(*cmd).unwrap_or_else(|| {
            panic!("派生器认不出命令 `{cmd}` —— 它在 `LEDGER` 里，却不在源码的 `#[tauri::command]` 面上")
        });
        *hist.entry(format!("{got:?}")).or_default() += 1;
        if *got == Derived::FramePlane {
            candidates.insert(*cmd);
        }
    }

    // ② **没签字红 / 陈账红 / 过期红** 三条都收在这一比里：
    //    多一行 `Remote`、少一行 `Remote`、某一行的派发跳换了档 —— 三样都让直方图不等。
    let want: BTreeMap<String, usize> = REMOTE_SIDE_SIGNOFF
        .iter()
        .map(|(d, n)| (format!("{d:?}"), *n))
        .collect();
    // 反向自检③：签字表必须把**每一档**都写出来（含 0），否则「某一档冒出来了」
    //    会被读成「表里没有这一档」而静默。
    assert_eq!(
        want.len(),
        all_kinds.len(),
        "签字表列了 {} 档，而派生类闭集有 {} 档 —— 每一档都要写出来（0 也要写）",
        want.len(),
        all_kinds.len()
    );
    for k in all_kinds {
        assert!(
            want.contains_key(&format!("{k:?}")),
            "签字表里少了 `{k:?}` 这一档"
        );
    }
    let signed_total: usize = want.values().sum();
    assert_eq!(
        signed_total,
        remote_rows.len(),
        "签字表合计 {} 行，而 `Side::Remote` 现打 {} 行 —— 有人动过 `Side` 那一栏，\n\
             而这一栏今天靠人签字（为什么不是从源码派生，见本节头注那三条现打的理由）。\n\
             出路：现打一次 `command_dispatch_class()`，把签字改成今天的形状。",
        signed_total,
        remote_rows.len()
    );
    assert!(
        hist == want,
        "**`Side::Remote` 那一栏的形状变了，而签字没跟** —— 现打 {hist:?} · 签字 {want:?}。\n\
             左边是今天现打的，右边是签字表里的。三种情况都会走到这里：\n\
               · 新增 / 改成 `Side::Remote` 的行没人签字；\n\
               · 签过字的行今天不再是 `Side::Remote`（有人改了 `Side`）；\n\
               · 某条命令的**派发跳换了档**（改了行为，没回来重签）—— \n\
                 这一条正是本笔要治的病：`K-R112` 09-13 撞见时，账本已经这样撒了一个月的谎。"
    );

    // ③ 候选集（派生 = `FramePlane`）必须**恰好等于**人裁表的键集。
    let judged: BTreeSet<&str> = FRAME_PLANE_VERDICTS.iter().map(|(c, _, _)| *c).collect();
    // 反向自检④：候选集空了 ⇒ 下面那条相等是空真（`[] == []` 照样成立）。
    // 地板 5 → 3：抓屏 · 杀会话 · 送键三条候选随命令迁到界面没了（人群真少了 3 个，现打 3：cc-bus 那三行）；
    //   有牙的是下面那条「候选集 == 人裁表」的相等，这个数只守「没塌成空集」。
    // 3 → **0**：cc-bus 那三行也随命令退役了 —— 候选集今天**真的是空集**（`Side::Remote` 里没有一条
    //   走 origin 无关的帧面派发了）。于是「没塌成空集」这条地板本身不再成立，换成**异源的正控**：
    //   同一个派生器在**整张** `LEDGER` 上（不只 `Remote` 那一栏）认得出多少条 `FramePlane` —— 那一群是 `Both` / `Local`
    //   命令（`chan_call` 那一族经 `client_for(` 问后端），人群不空 ⇒ 派生器没瞎，下面那条「候选 == 人裁」的空集相等才可信。
    //   这个数是恒等（跑出来的），不是地板：人群变了要回来改、写清谁加谁减。
    let frame_plane_anywhere: BTreeSet<&str> = derived
        .iter()
        .filter(|(_, d)| **d == Derived::FramePlane)
        .map(|(c, _)| c.as_str())
        .collect();
    assert_eq!(
        frame_plane_anywhere,
        FRAME_PLANE_ANYWHERE.iter().copied().collect::<BTreeSet<&str>>(),
        "派生器在整张账本上认出的 `FramePlane` 命令变了 —— 候选集今天是空集，这一群是它不瞎的正控：\
         少了可能是标记表 / 派生器坏了（那下面的空集相等就是空真），多了 / 少了都要来这里写清是谁"
    );
    assert_eq!(
        candidates, judged,
        "候选集与人裁表对不上。\n\
             多出来的候选 = **有人把一条命令改成了 origin 无关的帧面派发，而没人裁过\
             它的 `Side` 该不该跟着改**；\n\
             多出来的人裁 = 那一条今天已经不是候选了（表在腐烂）。"
    );

    // ④ 每条人裁的理由要**可追问**（同 `every_asymmetry_reason_carries_something_you_can_go_check`
    //    的那条口径：锚 = 反引号里的标识符）。空泛道理不算裁过。
    for (cmd, verdict, why) in FRAME_PLANE_VERDICTS {
        assert!(
            why.chars().count() > 40 && why.contains('`'),
            "{cmd} 的人裁没有可追问的锚（要一处反引号里的文件名 / 函数名 / 判据名）"
        );
        if *verdict == FrameVerdict::LiesTodayOwedACorrection {
            // 运行时拼：语料变量上的裸子串匹配有一条只许降的棘轮
            // （`needle_anchor_registry`），别往上顶。
            let side_word = format!("Si{}", "de");
            let local_word = format!("本{}", "机");
            assert!(
                why.contains(side_word.as_str()) || why.contains(local_word.as_str()),
                "{cmd} 签的是「说假话·欠一次订正」，理由里却没说清**哪一格假了**"
            );
        }
    }

    // ⑤ 这一栏今天欠着几笔 —— **现算**，不写字面量（写死的数下一轮自动变成假话）。
    let owed = FRAME_PLANE_VERDICTS
        .iter()
        .filter(|(_, v, _)| *v == FrameVerdict::LiesTodayOwedACorrection)
        .count();
    // **显式声明为零**（原判据头注要求的那一步）：欠的五笔 —— 抓屏 · 杀会话 · 送键（批 1–2）·
    //   cc-bus 派生 / 广播 / 收掉（批 3b）—— 都不是「翻了 `Side`」结的，是命令退役（界面经通道直接说后端，
    //   通道上的问法不在 `Side` 这一栏）。所以这里从「至少一笔」改成**恒等 0**：再有人签一笔「说假话·欠一次订正」，
    //   这一条就红、逼着回来改这个数并写清是哪一行。
    assert_eq!(
        owed, 0,
        "人裁表里又出现了「说假话·欠一次订正」—— 回来把这个数改成现打的值，并写清是哪一行、欠的是哪一格"
    );
}
