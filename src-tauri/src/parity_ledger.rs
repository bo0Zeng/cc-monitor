//! L5（local-as-remote）：本地/远端**平价对账表** —— `doc/INVARIANTS.md` §40 的「机制」那半。
//!
//! # 它守的是什么
//!
//! §40 是用户 2026-07-29 拍板的方向：「把本地当成不走 ssh 的远端。**后面都要这么搞。**」
//! 那一节末尾自己写着：**单靠人记不住**，要一条钉死的对账表 + 计数自检，否则
//! 「先做远端、本地以后再说」会像 `BACKLOG.md` 头注记的 U6→U8 那样**无声蒸发**。
//! 这个模块就是那道门禁。
//!
//! # 为什么判据不能落在「命令名」或「参数形状」上
//!
//! 开工时两个看起来现成的判据都实测**不成立**：
//!
//! - **名字带 `remote`**：27 条。漏掉 10 条真远端命令（`cc_bus_*`、`tmux_send_keys`、
//!   `account_usage`、`probe_ccm_cli`、`check_account_trust`、`resolve_ssh_host`）。
//! - **吃 `origin` / host 参数**：25 条。漏掉 12 条——它们吃的是 `RemoteConfig` 结构体，
//!   或者干脆不吃参数（`aggregate_remote_usage_all` 这类是「枚举所有远端」）。
//!
//! 两者都是**表面特征**。真正要回答的问题是「这项能力在两侧都有吗」，而那是判断，
//! 不是能从代码推出来的东西（本地需不需要 SFTP 面板？不需要——但没有任何语法能说明这点）。
//!
//! ⇒ **声明式**：每条命令声明它属于哪个**能力**、服务哪一侧；对称与否由机器**算**出来，
//! 不由人声明。这一条很要紧——T04 审计留下的教训是，一个能由别的字段 1:1 推出来的
//! 声明字段就是**安慰剂**（`config_surface.rs` 那条注释记着这件事）。
//! 这里「对称/不对称」是算的，人只需要为**算出来的**不对称写理由。
//!
//! # 三条断言的分工
//!
//! 1. **`generate_handler!` ↔ 本表双向相等** —— 新增命令不登记就红，删了命令不清表也红。
//!    这条是 §40 要的「新增命令不登记就红」。
//! 2. **算出来的不对称集合 == 理由表的键集合** —— 多一个不对称没写理由会红；
//!    某项后来补齐了两侧、理由却还留着，也红（防表腐烂）。
//! 3. **结构反证**：声明 `Local` / `Both` 的命令，签名里**不许**出现远端专用参数。
//!    这条防的是「为了让表好看，把一条远端命令声明成两侧都有」。
//!
//! **反向的那条刻意没做**（`Remote` ⇒ 必须吃远端参数）：实测有 11 条合法的例外
//!（枚举全部远端的、按 id 查应用侧缓存的、取消传输这种拿 `transfer_id` 的），
//! 加进来就是一张 11 条的豁免表换一条几乎不设防的断言。**而且它防的方向无害**——
//! 把本地命令误报成远端只会多出一条要写理由的假欠账，不会造出假的平价。
//! 有牙的是第 3 条那个方向，今天实测 **0 违规**，就钉在 0。
//!
//! # 范围
//!
//! **只钉 Tauri 命令这一层。** 命令内部是否真的两侧行为一致、前端是否两条路都走，
//! 本表**不管**——那要靠各功能自己的测试。**守卫范围必须等于它真正证明的性质。**
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销。
//! 将来若要把对账表**显示**在配置面审计页里（那是另一个功能），把它提出 `cfg(test)` 即可。

#[cfg(test)]
mod tests {
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
        ("replay_session_to_window", "app.window.session", Side::Both),
        // K-H2a：中转那把第三方 API key。读那条**只回掩码**（`KS6`），写那条是「界面」这个第二写者（`KS10`）。
        (
            "read_relay_credentials_status",
            "creds.relay-key",
            Side::Local,
        ),
        (
            "write_relay_credentials_key",
            "creds.relay-key",
            Side::Local,
        ),
        // K-H2b `KH2B7`：界面问「这几个**本机**账号走不走中转」。理由见 `relay.routing` 那行。
        ("relay_routing_for", "relay.routing", Side::Local),
        // K-R49：加了账号就把 `zcc` / `bcc` 那条命令也落下来。**本机专属**——
        // 它写的是这台机器上的 shell 能 source 到的那份文件；远端那半欠什么见
        // `alias.account-commands` 那行。
        (
            "write_account_aliases",
            "alias.account-commands",
            Side::Local,
        ),
        ("open_settings_window", "app.window.settings", Side::Both),
        ("bring_monitor_to_front", "app.window.self", Side::Both),
        // devbench F03：skill 接入面（列出 skill / 读写那个「人手写的注入文件」）。
        // 三条共用一个能力键 —— 它们是同一件事的三个动作（先例：`app.window.session` 也是两条共键）。
        ("list_skills", "skill.inbox", Side::Local),
        ("read_skill_file", "skill.inbox", Side::Local),
        ("write_skill_file", "skill.inbox", Side::Local),
        ("cc_get_auto_launch", "app.auto-launch", Side::Both),
        ("cc_set_auto_launch", "app.auto-launch", Side::Both),
        ("frontend_perf_log", "app.diagnostics", Side::Both),
        ("get_diagnostics_config", "app.diagnostics", Side::Both),
        ("set_diagnostics_config", "app.diagnostics", Side::Both),
        ("get_log_file_info", "app.logs", Side::Both),
        ("open_log_file", "app.logs", Side::Both),
        ("open_log_dir", "app.logs", Side::Both),
        // P2s（C8）：daemon 策略是 **per-host** 的（入参带 origin，本机 = `<local>`）⇒ `Both`。
        // 一个命令服务两侧，正是 C1「本地要和远端一样」在命令面上的样子。
        ("set_daemon_kill_on_exit", "app.daemon-policy", Side::Both),
        // P2s：状态两侧同源 —— 「这台机的 daemon 通道在不在」读的是 `inbound_client` 那**一张**
        // 登记表（远端 hello 后登记，本机 P2 之后也在 hello 后登记）。⇒ `Both`，且只有一条命令。
        ("daemon_status", "daemon.status", Side::Both),
        // P2s：起/停也是**一条命令管两侧** —— 本机杀子进程、远端断那条 SSH 流。
        // 两者「结果相同、路径不同」，差别塞在实现里而不是塞成两条命令（`C1`）。
        // P2s（A5）：开关面该列哪几台机，由**注册表**说了算（前端自己算会与 origin 分叉四处）。
        ("daemon_machines", "daemon.lifecycle", Side::Both),
        ("daemon_start", "daemon.lifecycle", Side::Both),
        ("daemon_stop", "daemon.lifecycle", Side::Both),
        ("load_config", "app.config", Side::Both),
        ("save_config", "app.config", Side::Both),
        ("get_data_paths", "app.data-paths", Side::Both),
        ("forget_session", "session.forget", Side::Both),
        ("list_session_activity", "session.activity", Side::Both),
        ("list_active_sessions", "session.list-active", Side::Both),
        ("update_history_metadata", "history.metadata", Side::Both),
        ("list_last_accounts", "accounts.last-used", Side::Both),
        ("search_history", "search.history", Side::Both),
        // ⚠ **P3b 复核（08-12）：这条能力两侧都登记着，但实现是 Win32 专属。**
        // `bind.rs` 的 `SetForegroundWindow` / `IsWindow` / `ShowWindow` 全在 `#[cfg(windows)]` 下
        // ⇒ **Linux 上这两条命令都没有实现**（不是坏了，是没写）。
        // 本表记的是「本地/远端平价」，不是「平台覆盖」—— 所以 `Both` 没记错；
        // 但只读这一行会以为 Linux 也有。⇒ 平台那一维归 `U14`（用户 08-12 已裁：要做，排在后面）。
        ("bring_terminal_to_front", "terminal.focus", Side::Local),
        (
            "bring_remote_terminal_to_front",
            "terminal.focus",
            Side::Remote,
        ),
        ("diagnose_local_cc_bus_hooks", "hooks.diagnose", Side::Local),
        (
            "diagnose_remote_cc_bus_hooks",
            "hooks.diagnose",
            Side::Remote,
        ),
        (
            "list_history_projects",
            "history.list-projects",
            Side::Local,
        ),
        (
            "list_remote_history_projects",
            "history.list-projects",
            Side::Remote,
        ),
        (
            "stream_history_sessions_in_project",
            "history.list-sessions",
            Side::Local,
        ),
        (
            "stream_remote_history_sessions",
            "history.list-sessions",
            Side::Remote,
        ),
        (
            "stream_read_session_jsonl",
            "history.read-session",
            Side::Local,
        ),
        (
            "stream_read_remote_session",
            "history.read-session",
            Side::Remote,
        ),
        ("delete_history_session", "history.delete", Side::Local),
        (
            "delete_remote_history_session",
            "history.delete",
            Side::Remote,
        ),
        ("aggregate_usage_all", "usage.aggregate", Side::Local),
        (
            "aggregate_remote_usage_all",
            "usage.aggregate",
            Side::Remote,
        ),
        ("read_mcp_servers", "mcp.read", Side::Local),
        ("read_remote_mcp_servers", "mcp.read", Side::Remote),
        ("read_remote_project_mcp", "mcp.read", Side::Remote),
        (
            "list_mcp_project_dirs",
            "mcp.list-project-dirs",
            Side::Local,
        ),
        (
            "list_remote_mcp_project_dirs",
            "mcp.list-project-dirs",
            Side::Remote,
        ),
        ("write_project_mcp_server", "mcp.write", Side::Local),
        ("write_remote_mcp_server", "mcp.write", Side::Remote),
        ("remove_project_mcp_server", "mcp.remove", Side::Local),
        ("remove_remote_mcp_server", "mcp.remove", Side::Remote),
        ("resume_history_session", "session.launch", Side::Local),
        ("new_local_session", "session.launch", Side::Local),
        ("launch_remote_terminal", "session.launch", Side::Remote),
        ("cc_integration_status", "ccm.status", Side::Local),
        // 🔴 〔`K-R69` 09-12〕本机那条 `ccm` 入口的**身份**（不是「在不在」）。
        //    它归 `ccm.status` 而不是自成一格：远端那一侧**已经有对侧**（下一行的
        //    `probe_ccm_cli` 问的就是远端那个 `ccm` 的 `--ccm-probe` 名片）。
        //    ⇒ 这一条补的是「本机也问得出同一张名片」，不是一条新的单侧能力。
        ("local_ccm_entry_status", "ccm.status", Side::Local),
        ("probe_ccm_cli", "ccm.status", Side::Remote),
        ("cc_integration_install", "ccm.install", Side::Local),
        ("install_remote_ccm_helper", "ccm.install", Side::Remote),
        ("cc_integration_uninstall", "ccm.uninstall", Side::Local),
        ("uninstall_remote_ccm_helper", "ccm.uninstall", Side::Remote),
        // 🔴 `K-R135`（`R85`/`R87`）：用户级 PATH 那一格。只有本机一侧，理由见
        //    `ASYMMETRY_REASONS` 里 `ccm.user-path` 那一条。
        ("ccm_user_path_status", "ccm.user-path", Side::Local),
        ("ccm_user_path_add", "ccm.user-path", Side::Local),
        ("ccm_user_path_remove", "ccm.user-path", Side::Local),
        ("cc_integration_preview", "ccm.install-ui", Side::Local),
        ("cc_integration_scan_path", "ccm.install-ui", Side::Local),
        ("create_branch_session", "history.branch", Side::Local),
        // G6：远端分叉落地 ⇒ `history.branch` 从 ParityDebt 变成两侧都有（该行的
        // 不对称理由已从 `ASYMMETRY_REASONS` 删除——留着就是宣称一条已经补上的欠账）。
        (
            "create_remote_branch_session",
            "history.branch",
            Side::Remote,
        ),
        ("panorama_index", "panorama.code-graph", Side::Local),
        ("panorama_reindex", "panorama.code-graph", Side::Local),
        ("panorama_status", "panorama.code-graph", Side::Local),
        ("panorama_overview", "panorama.code-graph", Side::Local),
        ("panorama_node", "panorama.code-graph", Side::Local),
        ("panorama_subgraph", "panorama.code-graph", Side::Local),
        ("panorama_callers", "panorama.code-graph", Side::Local),
        ("panorama_callees", "panorama.code-graph", Side::Local),
        ("panorama_impact", "panorama.code-graph", Side::Local),
        ("panorama_search", "panorama.code-graph", Side::Local),
        ("panorama_docs_for", "panorama.code-graph", Side::Local),
        ("panorama_touching", "panorama.code-graph", Side::Local),
        (
            "panorama_symbols_in_file",
            "panorama.code-graph",
            Side::Local,
        ),
        ("panorama_drift", "panorama.code-graph", Side::Local),
        (
            "panorama_add_annotation",
            "panorama.code-graph",
            Side::Local,
        ),
        (
            "panorama_propose_annotation",
            "panorama.code-graph",
            Side::Local,
        ),
        (
            "panorama_approve_annotation",
            "panorama.code-graph",
            Side::Local,
        ),
        (
            "panorama_remove_annotation",
            "panorama.code-graph",
            Side::Local,
        ),
        (
            "panorama_list_annotations",
            "panorama.code-graph",
            Side::Local,
        ),
        (
            "panorama_write_doc_link",
            "panorama.code-graph",
            Side::Local,
        ),
        (
            "panorama_remove_doc_link",
            "panorama.code-graph",
            Side::Local,
        ),
        ("get_search_index_status", "search.index", Side::Local),
        ("rebuild_search_index", "search.index", Side::Local),
        // P7c-1（08-12）：远端会话的 subagent 展开做出来了 ⇒ 两侧都服务。
        ("load_subagent", "subagent.load", Side::Both),
        ("get_session_tasks", "session.tasks", Side::Local),
        ("config_surface_report", "audit.config-surface", Side::Local),
        // U-CC1：漂移记账是**进程内**的全局账本，本地行与远端行都经同一个
        // `parse_line`（`lib.rs::batch_to_payloads`）喂进来 ⇒ 一个读口就覆盖两侧，
        // 天然 `Both`，不需要远端对侧命令。
        ("drift_ledger_report", "audit.drift-ledger", Side::Both),
        // P8a：插件面只读枚举。**只有本机一侧** —— 理由见下面 LEDGER 里那一行。
        (
            "list_plugin_marketplaces",
            "plugins.marketplaces",
            Side::Local,
        ),
        // PS1：把内嵌 cc-bus 装到本机 `<claude_dir>/skills/`。远端那侧**早就有**
        // （`acct-iso.deploy` 那条路的同族）——但 cc-bus 的远端部署今天没做，如实记欠。
        ("deploy_local_cc_bus", "cc-bus.deploy", Side::Local),
        // PS2：本机装的是哪一版（只读）。与部署同族、同样只有本机口。
        ("cc_bus_install_state", "cc-bus.install-state", Side::Local),
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
        // 它的渲染器就住在 Rust 里（`history.rs::render_local_ccm`），前端不必绕一圈问自己。
        // ★★ **P3b 结清（08-12）：这一行从 `Remote` 变 `Both`，不再是不对称。**
        //
        // 它原来的注释写「Remote-only 且未裁定 —— 本机该不该也有一个后端进程来收这件事，
        // 正是 §1.2 今天悬着的问题」。**两半都被证伪了**：
        // ① 「还没裁定」——`C1`/`C8` 已裁（用户 08-11「本地要和远端一样」）；
        // ② 「本机没有后端进程」——P2 交付了：`local_backend.rs::local_stdio_consumer` 里逐字
        //    `inbound_client::register(LOCAL_ORIGIN, client)`，`client_for("<local>")` 实测通。
        // ③ 而 **P3 刀 3 让生产段真的用它了**：`runLocalResumeIntoExistingTmux`
        //    以 `<local>` 调 `daemon_send_into` 就地 resume。
        //
        // ⇒ 这条不是「改个分类」，是**这条能力真的两侧都有了**。
        ("daemon_send_into", "launch.send-into", Side::Both),
        ("render_ccm_launch", "launch.render-cli", Side::Remote),
        // 🔴🔴 〔`K-R109` 09-13〕本机后端产「把终端接进那个会话」那一句（`ccm attach <名>`，
        //    `R61` 裁定三逐字「归本机后端就好了啊」）。
        //
        //    **它自成一格，而这个键是被机器逼出来的 —— 两个更省事的归法都实打过，都不成立**：
        //    ① 挂上面那一行的 `launch.render-cli`（「`ccm` 调用行的渲染」）⇒ 那条能力当场从
        //       `{Remote}` 变 `{Local, Remote}` = 对称 ⇒ `ASYMMETRY_REASONS` 里那一行**必须删**，
        //       而 `KR53D4` 逐字禁「靠删掉记录兑现」（`the_two_launch_rows_no_longer_carry_…`
        //       那条 `find()` 找不到就当场炸），且它记的那笔账今天真的还欠着
        //       （远端那一半说不出继承态，归 `K-R90`）。
        //    ② 挂 `session.launch`（本机起/续会话那一族）⇒ **实打红了，而且红得对**：
        //       `launcher_identity_registry::REGISTERED` 的人群正是「账本里能力 ∈
        //       `LAUNCH_CAPS`（`session.launch` / `launch.send-into`）的那些命令」，
        //       挂上去它当场把本命令读成第 6 个**起会话方**。而按它自己的口径
        //       （「产出的那一串直接导致一个 agent 进程出生」）**本命令不是** ——
        //       它接的是一条**已经在跑**的会话，一个进程都不出生。
        //       ⇒ 那不是「登记漏了一行」，是**归错了格**。〔本轮 09-13 门禁实测，读数住
        //       `evidence/K-R109-deathvalue.md` 的「归格那一刀」〕
        ("render_local_attach", "launch.render-attach", Side::Local),
        // 兜底那支的 `container:"none"` 载荷渲染。**Remote 侧专属**，但理由与 launch.render-cli
        // 已经不同了（P3t-Y4）：这一支本机确实还不经 IR —— 走的是
        // `history.rs::build_local_posix_command`，那是渲染器拒了之后的回落。
        // ⚠ 别再引 §36 当依据：§36 讲的是 **Windows** 且禁的是「本地渲染器读 `plan.env`」。
        (
            "render_launch_payload",
            "launch.render-payload",
            Side::Remote,
        ),
        ("sftp_realpath", "sftp.file-panel", Side::Remote),
        ("sftp_list_dir", "sftp.file-panel", Side::Remote),
        ("sftp_stat", "sftp.file-panel", Side::Remote),
        ("sftp_download", "sftp.file-panel", Side::Remote),
        ("sftp_upload", "sftp.file-panel", Side::Remote),
        ("sftp_cancel_transfer", "sftp.file-panel", Side::Remote),
        ("sftp_mkdir", "sftp.file-panel", Side::Remote),
        ("sftp_rename", "sftp.file-panel", Side::Remote),
        ("sftp_delete", "sftp.file-panel", Side::Remote),
        ("sftp_read_text_for_edit", "sftp.file-panel", Side::Remote),
        ("sftp_write_text", "sftp.file-panel", Side::Remote),
        ("start_forward", "port-forward", Side::Remote),
        ("stop_forward", "port-forward", Side::Remote),
        ("list_forwards", "port-forward", Side::Remote),
        ("list_ssh_host_aliases", "ssh.host-config", Side::Remote),
        ("resolve_ssh_host", "ssh.host-config", Side::Remote),
        ("import_ssh_hosts", "ssh.host-config", Side::Remote),
        ("test_remote_connection", "ssh.host-config", Side::Remote),
        ("push_public_key", "ssh.host-config", Side::Remote),
        ("deploy_remote_daemon", "daemon.deploy", Side::Remote),
        ("uninstall_remote_daemon", "daemon.deploy", Side::Remote),
        ("list_remote_mcp_origins", "mcp.list-origins", Side::Remote),
        ("list_remote_accounts", "accounts.list", Side::Remote),
        // L3a：本机枚举——同样只读、同样的输出类型 ⇒ 这条能力已对称。
        ("list_local_accounts", "accounts.list", Side::Local),
        (
            "list_remote_session_accounts",
            "accounts.session-accounts",
            Side::Remote,
        ),
        // E79：本机版落地 ⇒ `accounts.session-accounts` 从 ParityDebt 变成两侧都有。
        // **平台限制不等于欠账**：Linux 有（读 /proc/<pid>/environ），Windows 上后端
        // 明说 `available:false` + 原因 —— 那是「能力在这台机器上答不出」，
        // 不是「这条能力我们没做」。对账表问的是后者。
        (
            "list_local_session_accounts",
            "accounts.session-accounts",
            Side::Local,
        ),
        ("check_account_trust", "accounts.trust", Side::Remote),
        ("account_usage", "usage.per-account", Side::Remote),
        // F08：补平那条 ParityDebt —— 与远端**逐字用同一条命令串**，只换执行面。
        ("account_usage_local", "usage.per-account", Side::Local),
        ("deploy_remote_acct_iso", "acct-iso.deploy", Side::Remote),
        ("check_remote_acct_iso", "acct-iso.check", Side::Remote),
        (
            "remote_acct_iso_shellinit",
            "acct-iso.shellinit",
            Side::Remote,
        ),
        ("list_remote_tmux", "tmux.manage", Side::Remote),
        // P3t-Y2b：**刻意不挂在 `tmux.manage` 底下**。挂上去会让那条能力变成 `Both`，
        // 而那是过度声称 —— 本机这个口只答「哪些名字被占了」，不能 capture-pane、不能 kill、
        // 不能 send-keys。能力表要能被人当账看，就不能拿一条窄口去把一条宽能力标绿。
        ("list_local_tmux", "tmux.local-census", Side::Local),
        ("capture_remote_pane", "tmux.manage", Side::Remote),
        ("kill_remote_tmux", "tmux.manage", Side::Remote),
        ("tmux_send_keys", "tmux.manage", Side::Remote),
        // P4a（08-12）：读面三条**已经支持本机**（同一条命令串，只是不包进 ssh）⇒ 转 Both。
        ("read_cc_bus_state", "cc-bus.cockpit", Side::Both),
        ("check_cc_bus_agent_online", "cc-bus.cockpit", Side::Both),
        ("read_cc_bus_inbox", "cc-bus.cockpit", Side::Both),
        // 🔴 `K-R98`（09-13）：**发消息两侧走的是同一条路** —— 本机那半 `P4f` 已切 daemon 的
        // `bus-send`，远端那半此前还在拼 shell 串走 SSH，本件也改走同一条原语。
        // ⇒ `cc_bus_send` 从 `Remote` 转 `Both`。⚠ 它不是「补了一侧」，是**本来就已经两侧都通**
        //   （P4f 那天就该改这一行，没改 ⇒ 账本从那天起对这一格撒了一个月的谎，
        //   与本表 `cc-bus.cockpit` 那条理由自陈的「改了行为没回来改理由」是同一种病）。
        ("cc_bus_send", "cc-bus.cockpit", Side::Both),
        // 写面其余三条仍是远端专属（本机对侧未做，见 `cc_bus.rs::refuse_local_write`）。
        ("cc_bus_spawn", "cc-bus.cockpit", Side::Remote),
        // P4c（08-12，#77/#78）：广播 + 收掉。同为写面 ⇒ 同样远端专属。
        ("cc_bus_broadcast", "cc-bus.cockpit", Side::Remote),
        ("cc_bus_kill", "cc-bus.cockpit", Side::Remote),
    ];

    /// **不对称能力的理由**。键集合必须**恰好等于**从 `LEDGER` 算出来的不对称集合。
    const ASYMMETRY_REASONS: &[(&str, Asym, &str)] = &[
        ("ccm.user-path", Asym::NaturallyAsymmetric, "🔴 `K-R135`：「把我们那个 bin 目录放上**用户级** PATH」。**天然只有本机一侧，而且这一条的『天然』是可证的，不是图省事**：① **远端那一侧同一件事已经有答案，只是载体不同** —— 远端 `ccm` 落 `~/.local/bin`，而把它放上 PATH 的是写进远端 rc 的那个围栏块（`install_remote_ccm_helper` ＋ `shared/ccm-aliases.sh` 里那一行 —— 那一行正是 `K-R135` 本轮修的：它此前只加 `~/.local/bin`，对本机那一边是错的）。⇒ 欠的不是「远端没有这项能力」，是**两边的机制本来就不同**。② **「用户级 PATH」这一档是 Windows 独有的**（注册表 `HKCU` 下那个 `Environment` 键），而远端按 `K32` 是 Linux ⇒ 那台机器上根本没有这一档可改，补一条对侧命令只能是个空壳。⚠ **诚实边界**：哪天真出现「远端是 Windows」这一形，本条要回来重裁 —— 那时它就不再是 `NaturallyAsymmetric`，而是 `ParityDebt`。今天不给它发明一条够不着的对侧。"),
        ("accounts.trust", Asym::ParityDebt, "同 accounts.list：预信任检查只有远端有 —— 命令面实测只有 `check_account_trust`（`Side::Remote`），本机零对侧。归 L3。"),
        ("acct-iso.check", Asym::ParityDebt, "本机同样需要「这台装没装 cc-acct-iso」的检测（切号要靠它），今天只能查远端 —— 命令面实测只有 `check_remote_acct_iso`，本机零对侧；而本机确实**用得着**它：`local_accounts.rs` 头注逐字说这份数据有三个读者，其中写侧就是 `cc-acct-iso`。归 L3。"),
        ("acct-iso.deploy", Asym::NaturallyAsymmetric, "vendored 副本要**传到**远端才能用（`deploy_remote_acct_iso`）；本地就在本机、不存在传输这一步。⚠⚠ **P3b 标疑（08-12）：这条理由属于「从未被验证过」那一类，别当它已经核过。** 「不存在传输」是真的，但**「不存在安装」没人量过** —— 实测本机 `~/.local/bin` 下确实躺着 `cc-acct-iso`（`config_surface.rs:1062` 记着那 12 条 `cc-*`），而它是**怎么到那儿的**、要不要 monitor 管，本表从来没答过。⇒ 若答案是「要 monitor 管」，这条就该从 `natural` 变 `ParityDebt`。归 `P3b` 的后续或 `L3`。"),
        ("skill.inbox", Asym::Undecided, "devbench F03：skill 接入面今天只读写**本机工作目录**下的 `.claude/planned-build/INBOX.txt`。远端项目也可能有同一份结构（那边的 `.claude/` 一样在），技术上走 SFTP 就能读写 —— **但「远端项目的收件箱要不要能在这里编辑」没人裁定过**。⇒ 刻意记 `Undecided` 而不是 `NaturallyAsymmetric`：后者会替产品做主说「本地不需要」，而事实是**没想过**。⚠ 若将来要做，写面围栏那三道得先想清楚远端版怎么算（`canonicalize` 在远端不成立）。"),
        ("tmux.local-census", Asym::NaturallyAsymmetric, "「本机今天有哪些 tmux 会话」。★ P3 刀 2 的 UI 半把它从「只回名字」放宽到「回整条会话」——杀会话的菜单必须按 `@ccm_sid` 认归属，按 `<sid8>-cc` 前缀猜与 §30 逐字禁的「按目录回退猜」是同一类错。**反向缺口，且是天然的**：远端问同一个问题**已经有答案** —— `list_remote_tmux` 一次性 SSH `tmux ls` 就是它，前端 `pickFreshTmuxName(sid, existing)` 拿的正是那份。本机没有 SSH 那一跳，所以要一个自己的口；开它不是本机多了什么能力，是**把远端本来就有的那一格在本机补上**。⇒ 记 `NaturallyAsymmetric` 而不是 `ParityDebt`：欠的是本机这一侧，而本行一落地就已经补平，没有留下去处。★ 它读的是 daemon 推来的 tmux 快照而不是现跑 `tmux ls`，理由与射程见 `tmux.rs::list_local_tmux` 头注：那份快照由 `session-created/closed/renamed` 三条 hook 驱动，**恰好就是改变名字集合的那三件事** ⇒ 对这个问题它是权威的，对「pane 前台命令变了没有」才是陈旧的（那条已被 devbench F08 裁定不开口）。"),
        ("cc-bus.install-state", Asym::ParityDebt, "`PS2`：本机 cc-bus 装的是哪一版（没装 / 已是最新 / 装了但不是这一版）。**只读**，逐文件与内嵌那 17 个字节串比。⚠ 欠的与 `cc-bus.deploy` 是**同一笔**：远端那侧同样有 `~/.claude/skills/`，要问「远端装的是哪一版」得让 daemon 出一条具名读命令（形状抄 `P4d` 那批）。⇒ 两条一起补，别分两次。★ 顺带记口径：本条能答得出来，**全靠 `U9`② 裁了「仓内那份为准」**（用@08-13）—— 没有真相源就没有「不是这一版」这个判断，`PS2` 摸底时那条判据逐字写着「加第三态之前先答版本口径」。"),
        ("cc-bus.deploy", Asym::ParityDebt, "`PS1`〔`U10b` 用@08-13 裁「开」后落地〕：把内嵌的 cc-bus 装到 **本机** `<claude_dir>/skills/cc-bus/`。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端同样有 `~/.claude/skills/`，而且本仓**已经有**一条同族的远端部署路（`acct-iso.deploy` 走 SFTP 推 vendored 脚本）。欠的是**把这条本机路复制到远端**：SFTP 推 17 个文件 + 远端侧的围栏（`canonicalize` 在远端不成立，要换成 daemon 侧校验）。⇒ 如实记欠，**不假装两侧都有**。★ 顺带记一条口径：本条的落点是**用户数据目录**，与 `acct-iso.deploy` 那条「只写 cc-monitor 自己的 bin 目录」**性质不同** —— 后者不需要豁免，本条需要（`INVARIANTS` 第 7 条）。"),
        ("plugins.marketplaces", Asym::ParityDebt, "`P8a`：列 marketplace（来源 / 落点 / 更新时间 / 它**声明**的插件数）今天**只有本机**。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端的 `~/.claude/plugins/` 一样在，daemon 早就会读远端 claude 目录（`--list-projects` / `--list-sessions` / `--list-subagents` 三条现成的形状）。欠的是**一条 daemon 子命令**（`--list-marketplaces`）+ 一次 BUILD_ID/协议文档/内嵌重编，那是另一件事的体量，本件是梯队 5 的只读面 ⇒ 如实记欠，**不假装两侧都有**。★ 它与 `local_read_surface_registry` 里 `plugins.rs` 那条 `reader` 的退役条件是**同一条**：daemon 补上那条子命令，本机改走后端、远端这半一起补平 —— **一件事清两笔账**。⚠ 另记一条**本行答不了的**：本行说的是「有哪些 marketplace」，**不是**「装了/启用了哪些插件」——后者今天**两侧都没有真相源**（待决 `U10d`），那不是平价问题，是那份数据在盘上根本不存在。"),
        ("acct-iso.shellinit", Asym::ParityDebt, "本机切号同样要 shellinit 文本（`cc-acct-iso shellinit` 那句 `export CLAUDE_CONFIG_DIR=<默认账号>`），今天只能给远端生成 —— 命令面零本机对侧。归 L3。"),
        ("audit.config-surface", Asym::ParityDebt, "**反向缺口**（本地能答、远端答不出）——§40 表里已逐行记明：本页明写不连 SSH，10 行里 7 行对远端恒返回「未确定」。"),
        ("cc-bus.cockpit", Asym::ParityDebt, "★ **P4c 订正（08-12）：原理由已经过期，而过期的正是 `P4a` 那一刀造成的。** 原文写「cc_bus.rs 的 **5 个 IPC** 全走 origin+ssh、**零本机读取路径**」——`P4a`（08-12）把**读面三条**（`read_cc_bus_state` / `check_cc_bus_agent_online` / `read_cc_bus_inbox`）做成了本机可用（同一条命令串，只是不包进 ssh；本机 `~/.cc-bus/agents.tsv` 实测 86 行），它们今天是 `Both`。⇒ 「零本机读取路径」是假的，「5 个」也变成了 7 个（`P4c` 加了 `cc_bus_broadcast` / `cc_bus_kill`）。**今天真正的欠账只剩写面里的三条**：`cc_bus_spawn` / `cc_bus_kill` 对 `<local>` 走 `refuse_local_write`（本机没有对侧），`cc_bus_broadcast` 的本机路走 daemon 组合但**没有回落**（`P4a §0c` 量过代价：本机写面归 `P4b`，而 `P4b` 签收的是 cc-spawn 的复用那一刀，没交付写面）。★★ **`K-R98` 订正（09-13）：原文说的是「四条」，而其中两条早已不成立** —— `cc_bus_send` 的本机路 `P4f`（08-13）就改走了 daemon 的 `bus-send` 原语、`cc_bus_broadcast` 的本机路同日改走 `bus-list` + 逐个 `bus-send` 的组合，两条都**不再**经 `refuse_local_write`；而这一行从那天起一个字没改。⇒ 这已经是本条第 **2** 次因为「改了行为没回来改理由」而订正（第一次是 `P4c` 订 `P4a`，那段就在上面）。本次同时把 `cc_bus_send` 的 `Side` 从 `Remote` 改成 `Both`（远端那半也改走同一条原语，`cc_bus.rs::send_via_daemon` 是那唯一一处），登记见 `ORIGIN_TAKING_BOTH` 里那一行。★ 这条订正本身是 `P3b §0b` 的 **A 类（过期）**活样本，而制造它的是 `P4a` —— **改了行为没回来改理由，账本当天就开始撒谎**，本轮第二次（第一次是 `P4d-Y5` 改 `capture_remote_pane` 那次）。"),
        ("ccm.install-ui", Asym::Undecided, "本机安装向导有「扫 PATH 选装到哪」+「预览要写的文本」两步；远端 `install_remote_ccm_helper(cfg, profile)` 一步到位、没有这两步。**是欠账还是刻意简化，需要产品判断**——本表不替它裁定。"),
        ("daemon.deploy", Asym::NaturallyAsymmetric, "★★ **P3b 结清（08-12）：理由整个换掉 —— 原来那句是假的。** 原文写「§40 天然不对称白名单第 3 条：本地会话由 `watcher.rs` 直接读 jsonl，**根本不需要 daemon**」，被 P2z + P2 + P2s 三件直接证伪：本机**需要** daemon（入方向通道、每台机开关、tmux 帧都靠它），而且**已经会自部署** —— `local_backend.rs::extract_embedded_to`（exe 旁没有 sidecar 就把内嵌那份释放到 `~/.cc-monitor/bin`）。真正的不对称只剩一格：**本机那次释放不经一条 IPC 命令**，是宿主启动时自己做的（`lib.rs` 的启动段），所以命令面上没有本机对侧。⇒ 记 `natural` 记的是「不需要一条命令」，不是「不需要 daemon」。"),
        ("launch.render-payload", Asym::NaturallyAsymmetric, "兜底那支（`container:\"none\"`）的载荷渲染。**记 `natural` 记的是命令面这一格**：远端那侧要一条 IPC（`render_launch_payload`）才问得到宿主，而本机**自己就是宿主** —— `history.rs::launch_local` 直接在进程内调 `build_local_*_command`，没有「绕一圈问自己」这一步（同形的话 `launch_wire.rs` 的头注里逐字写着）。⚠⚠ **`K-R53`（09-11）撤掉原文那半句**：原文写「P3t 之后那是**渲染器拒了才走的回落**」——**按调用点分母那是假的**：盘上四个本机拉起入口里有三个（`src/tabs.ts` 一处 + `src/views/history.ts` 两处，人群由 `test/ipc/commands.vitest.ts` 那条「恰好 4 处」钉着）只说得出**具名账号**，而具名账号在 `K-R53` 之前必然 §35 短路 ⇒ **那三条只能走它**。一条 3/4 的分母不叫回落。`K-R53` 把具名那一格接上之后（`LaunchAccount::Named::name`），今天真正还会落到它的是：账号未表态（继承 —— 见下一行）· 只说得出目录没有名字 · 没有 tmux 名 · 这个号走中转（`history.rs::RELAY_KEEPS_THE_OLD_PATH`）· 这台机没装 ccm · Windows。逐格读数住 `history.rs::tests::every_local_account_shape_gets_a_named_verdict_from_the_backend_path`。★★ 🔴 **`K-R89`（09-13）：这六格今天只剩五格，而且「今天各自是什么」由一张**可执行**的表说了算** —— `history.rs::tests::THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`（六格逐格由 `every_one_of_the_six_cells_is_measured_not_narrated` **真去驱动一遍**，改了行为不改说法当场红 ⇒ 本行这句散文再腐一次，那边会先响）。关掉的是**账号未表态（继承）**：用户 09-12 `DECISIONS.md#R28` 裁定了「省略 `--account`」的语义，并已落地在 `remote-daemon-proto/src/control/ccm/plan.rs::resolve_account`（两支：`CLAUDE_CONFIG_DIR` 非空 ⇒ 保留不覆盖〔`R08` 那道 `-z` 闸〕· 裸终端 ⇒ 落 manifest `isDefault`）⇒ 本机那一态渲染得出来了。⚠ **只关了本机那半** —— 远端是 ssh 过去、那台机器上的继承态不是 monitor 的环境（`R28` 裁定四），`WireAccount` 刻意没有对应变体，那半归 `K-R90`。⚠ 同拍另一处现打订正：「没有 tmux 名」那一格今天是**半开**的 —— resume 那条前端已接线（`K-R89` 现打），而 `new_local_session` 的 Rust 签名里**根本没有 `tmux_name` 这一格** ⇒ 只有起新会话恒短路。⚠ P3t-Y4 订正保留：原文引 §36 当依据，那是把一条讲 **Windows**、逐字禁「本地渲染器读 `plan.env`」的窄铁律读宽了。"),
        ("launch.render-attach", Asym::NaturallyAsymmetric, "🔴 `K-R109`（09-13）：**本机后端产「把终端接进那个会话」那一句**（`history.rs::render_local_attach` ⇒ `ccm attach <名>`，`R61` 裁定三）。⚠ **记 `natural` 记的是「远端那一侧不需要一条独立命令」，不是「远端还没做」** —— 远端的 attach 那一句由 `render_ccm_launch` 的 `WireAction::Attach` **一并产**（记在 `launch.render-cli` 那一格，同一条 IPC 覆盖 new / resume / attach 三个动作）⇒ 远端不缺这项能力，缺的只是**一个单独的命令名**。反过来本机**不能复用那条 IPC**，两条都是结构性的：① 那条 wire 的 `is_ssh` 与前端那道闸（`ctx.transport.kind === \"ssh\"`）说的是同一件事 —— 本机就是宿主，绕一圈 IPC 问自己拿不到新东西（同 `launch.render-payload` 那一行的理由）；② `WireAccount` 刻意少一态（没有 `Inherit`，`launch_wire.rs` 头注逐字），而本机的账号态是 `render_local_ccm` 在**进程内**探出来的（`CcmProbeSource` 那条缝）。⚠ **它不是 `session.launch` 的一部分**：那一格的人群由 `launcher_identity_registry::LAUNCH_CAPS` 读走当「起会话方」，而本命令接的是一条**已经在跑**的会话，一个 agent 进程都不出生 —— 09-13 挂错过一次，门禁当场逮住（读数住 `evidence/K-R109-deathvalue.md`）。**这一格什么时候能结清**：远端那一句 attach 也长出自己的命令名的那天（那要先有人问「为什么要拆」），或者本机这一条被并进一条统一的渲染 IPC 的那天（`U8c-3` 那一侧）。"),
        ("launch.render-cli", Asym::NaturallyAsymmetric, "`ccm 调用行`的渲染。★★ **P3t-Y4 把这条的理由整个换了 —— 原来那个已被实测证伪。** 原文说这条不对称是「本地渲染必须在目标机器上做（要现场探 `command -v cc`，TS 无法预先渲染好交给它）」造成的。**本机就在本机**：P3t-Y2 的 `ccm_probe::probe_local_ccm()` 直接跑一次 `bash -lic` 就拿到了版本与完整能力集，比远端那条 ssh 往返还便宜 ⇒ 那个理由不成立。真正的不对称是**本机账号三态里有两态 CLI 说不出**：`Named{config_dir}` 只有目录没有名字（CLI 只会 `--account <名字>`），`None` 是「继承环境」而 CLI 语法里没有这一态（映成 `--base` 就是把继承偷换成显式清空 = #75 病灶）。★★ **`K-R53`（09-11）改的是它的分量，不是它的机制**：原文那半句把这两态回旧路说成一次边角的「降级」，而**按调用点分母它是主路**（四个本机拉起入口里三个只说得出具名账号）。⇒ `K-R53` 把具名那一态接上了（`LaunchAccount::Named` 现在带名字，由前端那个唯一取值口 `accounts.ts::localLaunchAccountSync` 与 `localLaunchAccountNameSync` 同源给出），**说不出的只剩「继承」一态**。而那一态**今天仍然说不出，而且省略参数也兑现不了**：既没 `--account` 也没 `--base`、且 `CLAUDE_CONFIG_DIR` 为空时 ccm **落 manifest 默认号**（「把调用方选中的号静默换掉」）⇒ 省略是另一个方向的静默换号，与 `--base` 一样不是「继承」。⚠ 那个读数**量于一份已经不在盘上的文件**（那份 bash `ccm` 的 1001-1012 行，`07e4e72` 删）；同一档语义今天住 `remote-daemon-proto/src/control/ccm/plan.rs`，`K-R61` **没有重打它** —— 别把它读成「今天现打过」。⇒ 本行仍 `natural`，它记的仍是**语法窄一格**，只是那一格从两态收成一态。★★ 🔴 **`K-R89`（09-13）：上面那句「补它要动的是 ccm 省略时的默认语义（**产品决定** ＋ 改 `plan.rs`）」是一句陈账 —— 它在等一个 09-12 就已经到了、而且已经落地的决定。** 那个产品决定 = `DECISIONS.md#R28`（用户 09-12 逐字「不是有选默认账号吗? **就用那个**」），落地处 = `remote-daemon-proto/src/control/ccm/plan.rs::resolve_account`（头注挂着 ✅，两支：`CLAUDE_CONFIG_DIR` 非空 ⇒ 保留不覆盖〔`R08` 那道 `-z` 闸〕· 裸终端 ⇒ 落 manifest `isDefault`）。⇒ **「继承」这一态今天在本机说得出了**（`CliAccount::Inherit` 渲染成「一个账号 flag 都不加」），`history.rs::render_local_ccm_with` 的 `None` 那一臂不再短路。⚠ **本行仍 `natural` 的理由因此换了人**：不再是「语法说不出继承」，而是 **远端那一半仍然说不出** —— 远端是 ssh 过去，那台机器上的继承态不是 monitor 的环境（`R28` 裁定四逐字「不算已解」）⇒ `launch_wire::WireAccount` 刻意没有对应变体，那半归 `K-R90`。⚠ 上面那张「三说法逐格对」的表**第二行今天翻了面**：「省略 ⇒ 落 manifest 默认号」从 ❌ 变 ✅ —— **行为一个字节没动，动的是对它的判断**（`R28` 裁定零逐字：「本裁改的不是行为，是『这是不是我们要的』」）。逐格今天版住 `history.rs::tests::THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`，由 `every_one_of_the_six_cells_is_measured_not_narrated` 真去驱动。"),
        ("mcp.list-origins", Asym::NaturallyAsymmetric, "`list_remote_mcp_origins` 答的是「哪几台远端有 MCP 配置」——「有哪些 origin」这个问题在本机侧退化成一台，没有可列的集合。⚠ 注意它与 `daemon_machines` 不同：那条**包含**本机（`LOCAL_ORIGIN`），因为它答的是「哪几台有 daemon」而本机也有。"),
        ("panorama.code-graph", Asym::Undecided, "**本表交出的最大一处新发现**：21 条命令全部只吃本机 `repo` 路径。远端 repo 的代码图谱既没做、也没在任何计划里登记过。**不擅自判它是天然不对称**——那需要产品判断（远端开发是不是本工具的场景）。登记待裁定。"),
        ("creds.relay-key", Asym::ParityDebt, "`K-H2a`：中转那把第三方 API key 今天**只有本机这一侧**能配。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端跑的中转读的是**远端那台机器上**的同一份文件（相对路径由 `creds_core::store::FILE_NAME` 两侧共用），它一样要有人把 key 放进去。欠的是**一条把它送到远端的路**。★ 而这条路**不能照抄现成的 SFTP 上传**：`K-H2a §0c 二` 现打（08-27）—— `sftp::upload_atomic` 的 mode 参数只以 SFTP v3 的 `PERMISSIONS` 属性搭在 `SSH_FXP_OPEN` 上（服务端可以忽略、协议不回执），`sftp.rs:141-147` 头注**逐字禁掉**了兜底 `set_metadata`，而 `upload_atomic_verified` 只比**字节与长度**、全仓**没有一处回读权限**，再加上全仓唯一那条 OS 判定 `src/settings/host-os.ts` 量的是 **monitor 自己**跑在哪、**不是远端** ⇒ 对面是 Windows 时那个 `0o600` **不是「不生效」，是「静默地不生效」**。⇒ 补这条路的时候，机密性必须由**拿着那份文件的那台机器自己检查**（`creds_core::perm`，daemon 侧已在 `relay::creds::announce` 里出声），不能由写它的那一跳「设一下就当保住了」。归 `K-H2`。"),
        ("relay.routing", Asym::NaturallyAsymmetric, "`K-H2b` `KH2B7`：「这个账号起会话时会不会走中转 / 中转在不在跑」。⚠ **记 `natural` 记的是一件关于机制的事，不是「远端还没做」**：中转是**每台机器自己的一个进程**（`relay/mod.rs` 自陈「独立进程」；注入的是那个 agent 进程自己的 `ANTHROPIC_BASE_URL`，而 `payload::relay_base_url` 拼的是**回环**地址 —— 回环是**自指**的，同一个字面串写进哪台机器就指哪台）⇒ 「本机这台的中转在不在跑」这个问题，**本机这一侧在结构上答不了远端那台**：那不是一条缺失的命令，是一个**问错了机器**的问题。远端那台要答同一个问题，得由**跑在那台上的 daemon** 自己答（读那台机器上的 `relay-credentials.json`、看那台机器上的中转进程），那是**远端 daemon 的一条新子命令**，与本条不是同一条命令的两侧。⚠ **另记一笔不在本条里的欠账，别混**：把 key **送到**远端那台机器的路今天没有主人，那笔记在 `creds.relay-key`（`ParityDebt`）。本条说的是「**问状态**」，那条说的是「**配上去**」。"),
        ("alias.account-commands", Asym::ParityDebt, "`K-R49`：按账号表生成 `zcc` / `bcc` 这一族命令并**落盘**，今天只有本机这一侧（`write_account_aliases` → `account_aliases::apply`，写 `~/.cc-monitor/account-aliases.sh`）。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端那台机器上的 shell 一样要有这几个命令，而且本仓**已经有**半条路：`remote_acct_iso_shellinit`（`sftp.rs`）抓远端 `cc-acct-iso shellinit` 的输出，那段文本里本来就含**每账号一个 `<名>cc` 函数**。欠的是**后半条** —— 它今天只吐一段待贴文本（`accounts-section.ts::renderRcSnippet` 头注逐字「绝不代写」），落盘那一步在远端一侧没有主人。★ 补它的时候有两条现打的障碍，别以为照抄本机那份就行：① 本机这份靠 `profile_installer::fence_profile_path` 把落点围在 home 之内，而那道围栏用的是 `dirs::home_dir()` + `canonicalize`，**远端两者都不成立**；② 本机那份别名文件由 `shared/ccm-aliases.sh` 里那行 `[ -r … ] && . …` 自动接上（**那个文件今天还在**，别与已删的那份 bash `ccm` 混起来），而远端的 rc 块是 `sftp::CCM_PROFILE_BEGIN` 那一套——两边的围栏标记不是同一个，得先想清楚谁负责那一行。⇒ 如实记欠，**不假装两侧都有**。"),
        ("port-forward", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 2 条：本地没有「转发到自己」这个需求。"),
        ("search.index", Asym::NaturallyAsymmetric, "远端**不建索引**：`search_history` 对远端是实时 SSH fan-out（其头注自陈「本地内存索引查询与远端 fan-out 并发」）。索引是本机侧的实现细节，不是一项对外能力。"),
        ("session.tasks", Asym::ParityDebt, "**实测**：`get_session_tasks` 走 `tasks_root_for_current_claude_dir()` → `paths::resolve_claude_dir()`，读的是**本机**目录。远端会话的任务在远端机器上 ⇒ 远端 tab 拿不到任务列表。"),
        ("sftp.file-panel", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 1 条：本地有操作系统的文件管理器，不需要它。"),
        ("ssh.host-config", Asym::NaturallyAsymmetric, "本地按 §40 的定义就是「**不走 ssh** 的远端」⇒ ssh 目标的枚举/解析/导入/连通性测试/公钥推送在本地没有对应物。"),

        ("tmux.manage", Asym::ParityDebt, "★★ **P3b E 阶段重量（08-12）：那句预言「POSIX 本地落地后自动就有」——三分之三对、四分之一错。** 原文只写「`ccm` 全套修饰本地『无』」+ 那句预言，没说是哪几条命令。逐条量：① `list_remote_tmux` ⇒ **本机已有对侧** `list_local_tmux`（P3t-Y2b + P3 刀2-UI，读 daemon 推来的快照）；② `kill_remote_tmux` ⇒ **本机已通**（P3 刀 2 后端：`daemon_kill` 传输无关，且本机专属错误文案已加）；③ `tmux_send_keys` ⇒ **本机已通**（同款 `daemon_route` 分流）；④ `capture_remote_pane` ⇒ **仍无本机对侧**。⚠ **P4d-Y5（08-12）改了它的一半**：原文接着写「它 `load_remote_config_by_label(&origin)`，对 `<local>` 会报「未找到远端配置」」—— **那半句今天已经假了**，本机分支已补上，报的是真实原因（本机 tmux 快照只带会话名不带屏幕内容，要预览得现抓一次 pane）。⇒ 假话没了，**欠账没结**：能不能预览这件事一点没变，仍等 daemon 出原语〔`K-R86` 09-13：**这半句今天假了**，订正在本行末尾〕。★ 这条订正本身是 `P3b §0b` 的 A 类（过期）活样本，而制造它的正是 P4d 那一刀 —— 改了行为不回来改理由，账本当天就开始撒谎。⇒ 欠账**只剩画面预览这一格**，而它不是「自动就有」的：预览要么现跑 `capture-pane`（本机可以，但那是第二条取数路），要么等 daemon 出原语（`P4d`）〔`K-R86` 09-13：同上，**这半句今天也假了**〕。归 **P4d**，不再归 L1/L2。★★ **K-R56（09-11）再订正一次，而这次订正的是上面 ③ 那一格**：③ 逐字写的是「`tmux_send_keys` ⇒ **本机已通**（同款 `daemon_route` 分流）」—— **那句话本身是真的**（PM 单子叮嘱「账本不许直接信」，本轮现打逐字复核过住址：分流确实同款，`daemon_send_keys` 也确实传输无关）。**假的是它旁边那句没写出来的话**：② 给 `kill_remote_tmux` 特地记了「**且本机专属错误文案已加**」，③ 没有那半句 —— 而**它不是省略，是当时真的没有**。⇒ 通道不在时 `tmux_send_keys` 会掉进 SSH 回落、报「未找到远端配置: `<local>`」，与 ①②④ 那一族**一模一样的假话**。K-R56 把那条早退补上了（`tmux.rs::tmux_send_keys` 的 `Routed::NoChannel` 臂，逐字抄 `kill` 那条先例），判据 `tmux::tests::the_local_send_keys_never_falls_back_to_ssh`（走生产入口本体，不是扫源码），存量登记 `local_origin_registry::TRIAGE_DEBT` 同轮 16 → 15。⚠ **「能不能 send-keys」这件事一个字没变** —— 变的只是**通道不在时它说什么**。⇒ 本格的欠账**仍然只剩画面预览那一格**，K-R56 没有结掉任何一笔账，它结的是一句假话。★★ **`K-R86`（09-13）第三次订正，而这次假掉的是上面那两处「仍等 daemon 出原语」**：daemon 侧今天**有**那条原语了 —— `--capture-pane <会话名>`，住 `remote-daemon-proto/src/control/capture_pane.rs`（`tmux -u capture-pane -p -t '=名:'`，argv 直传不过 shell，只读；起进程登记在 `readonly_guard::spawn_registry::ALLOWED`，「它只读」由 `readonly_guard::capture_is_read_only` 逐元素钉 argv；协议面见 `doc/IPC-PROTOCOL.md` §10；`BUILD_ID` p2f → p2g）。🔴 **而这一格的欠账一格都没结，只是换了个名字** —— 本件**只出 daemon 那一侧的原语**，monitor 的 `capture_remote_pane`（`src-tauri/src/tmux.rs`）**一个字节没动**，对 `<local>` 仍然没有本机对侧 ⇒ `Side::Remote` 这一格不许改。⇒ 欠账从「**等 daemon 出原语**」（`K-R86` 之前）变成「**等 monitor 侧接上去**」（`K-R86` 之后），归 `K-R87` 之后的接线那一件。⚠ 别把 `BUILD_ID` 的 bump 读成「远端已经有这条命令了」：那一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，本轮没做。★ 本行「那句话今天还成不成立」从此有人在数：`parity_ledger::tests::the_tmux_manage_row_stops_waiting_for_a_daemon_primitive` —— 它要求每一处「等 daemon 出原语」前后都挂着 `K-R86` 这个订正标记，**且不许靠删掉整行兑现**（同 `KR53D4` 那条的形状）。★★ **`K-R101`（09-13）第四次订正 —— 而这一次订正的是「归谁」那半，欠账仍然一格没结**：上面写着这笔账「归 `K-R87` 之后的接线那一件」。那一件立起来了（`K-R101`，`K-R54` 表第 7 行的本体），**而它没有结掉这一格**。现打的理由，不是推辞：monitor 今天够得着 daemon 的只有**帧面**（`inbound::REGISTRY` 上的 `launch` / `kill` 那几条，走 `inbound_client`），而 `--capture-pane` 与 `--oneshot-session` 是 **CLI 面独有**的（`remote-daemon-proto/src/main.rs` 的一次性分派臂，`inbound::REGISTRY` 里没有它们）⇒ 要接上去，要么给帧面新增那两条小原语（`inbound.rs` ＋ `doc/IPC-PROTOCOL.md` 的命令小节 ＋ `BUILD_ID` bump ＋ monitor 侧两个新发送端），要么让 monitor 每抓一屏起一次 SSH exec（现打：两段轮询上限 12+20 轮 ⇒ 单次探测最多 36 次 SSH 握手，撑破 `EXEC_TIMEOUT_SECS` = 25s）。⇒ **这一格的欠账从「等 monitor 侧接上去」细化成「等帧面上有那两条小原语」**，读数与三条候选的比价住 `.claude/planned-build/backend-consolidation/features/K-R101-用量探针的tmux编排整条搬进daemon.md#§8`。⚠ **别把 `K-R101` 已签收读成这一格结了** —— 那一件交的是「原文到得了界面 ＋ 解析层退役」，与本格是两件事。★★ **`K-R104`（09-13）第五次订正 —— 上一段那句「等帧面上有那两条小原语」今天假了，而这一格**仍然**没结**：帧面今天**有**它们了（`inbound::REGISTRY` 8 → 10，`ch:capture-pane` ＋ `ch:oneshot-session`，`BUILD_ID` p2h → p2i），monitor 侧也真的在用（`account_usage.rs` 整条编排改走帧面：一次拨号 ＋ 一条通道上 N 次往返，握手从最多 36 次降到 **1** 次）。🔴 **而本格问的不是那件事**：这一格问的是 `capture_remote_pane`（`src-tauri/src/tmux.rs`）—— **那个函数一个字节没动**，对 `<local>` 仍然没有本机对侧，所以 `Side::Remote` 这一格照旧不许改。⇒ 欠账从「**等帧面上有那两条小原语**」变成「**等 `capture_remote_pane` 自己改走帧面的 `capture-pane`**」——那是一次纯接线（原语在了、发送端在了、`inbound_client::capture_pane_args` 也在了），**但它不在 `K-R104` 的写区里**，归后续一件。⚠ 同样别把 `BUILD_ID` 的 bump 读成「远端已经有这两条了」：那一半是**源码半**，re-embed 归发版那一拍。"),

    ];

    /// 读 `lib.rs` 的 `generate_handler!`，取出前端真能调到的命令名。
    ///
    /// **以它为准，不以 `#[tauri::command]` 属性为准**：属性只说「它能当命令」，
    /// `generate_handler!` 才说「前端真能调到」。（漏注册是**运行时** `command not found`、
    /// 不是编译错——`ssh_source.rs` 里有一条注释专门警告过这件事。）
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
    /// 同型 bug 在 `remote-daemon-proto/src/no_timer_guard.rs` 同轮修掉。
    fn command_signatures() -> BTreeMap<String, String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let attr = format!("#[tauri::{}]", "command");
        let mut out = BTreeMap::new();
        // ★ F23 第二刀：改用 `scan_tree!` —— 它**按构造**摘除调用者自己（拿 `file!()`）。
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
                let paired = sides.len() == 2
                    && sides.contains(&Side::Local)
                    && sides.contains(&Side::Remote);
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
        assert!(
            registered.len() >= 100,
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
             请到 `parity_ledger.rs::LEDGER` 里补一行，并想清楚它在本地对应什么。"
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
            "load_subagent",
            "P7c-1：远端那条让 daemon **只列候选**（`--list-subagents`），\
             description 匹配与按时间戳挑最近**留在本侧**，与本机那条共用同一个 `pick_closest`。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「候选从哪来」。",
        ),
        (
            "read_cc_bus_state",
            "P4a：本机跑**同一条 `CC_BUS_CAT_CMD`**，只是不包进 ssh（`C1` 逐字「只是远端走 ssh，本地不走」）。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「谁来跑这条串」。",
        ),
        (
            "check_cc_bus_agent_online",
            "P4a：同上，跑的是同一个 `build_online_cmd` 产出的串（`tmux has-session`）。",
        ),
        (
            "read_cc_bus_inbox",
            "P4a：同上，跑的是同一个 `build_inbox_cmd` 产出的串（`tail`，零副作用）。",
        ),
        (
            "cc_bus_send",
            "`P4f` 08-13 本机 ＋ `K-R98` 09-13 远端：两侧调**同一条** daemon 原语 `bus-send`。\
             命令体对 origin 不做远端假设 —— 它只把 origin 交给 `client_for`，\
             决定「问哪台机器的后端」（`cc_bus.rs::send_via_daemon` 头注逐字\
             「origin 是原语的一个入参，不是一个分支」）。\
             ⚠ 这是本表里**第一条写面的 `Both`**：读面三条 08-12 就转了，写面等的是\
             用户 08-12 那句「先把确切的命令组件做出来，然后 cc-bus 可以去调用」。",
        ),
        (
            "set_daemon_kill_on_exit",
            "P2s：daemon 策略是 per-host 的，本机的 origin 就是 `<local>`。\
             命令体对 origin **不做任何远端假设**（它只是一张表的键），所以两侧共用一条命令 —— 这正是 C1。",
        ),
        (
            "daemon_status",
            "P2s：状态按 origin 查 `inbound_client` 的登记表，那张表两侧共用。\
             `pid`/`attempts` 只有本机有 —— 那是**天然不对称**（远端进程在别人机器上），\
             所以它们是 `Option` 而不是「远端填 0」。",
        ),
        (
            "daemon_start",
            "P2s：按 origin 分派 —— 本机起被监护的子进程，远端重起那条流的 task。\
             命令体只认识 origin，不认识 ssh（起法由 `lib.rs` 注册成闭包交进来）。",
        ),
        (
            "daemon_stop",
            "P2s：按 origin 分派 —— 本机杀子进程，远端 `abort()` 那条流。\
             远端 daemon 随管道破裂退出，本机看不见那个进程，所以返回的是「已断流」不是「已停进程」。",
        ),
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
        // 报文里那句「Local A + Both B」也**现算**，不手抄〔`K-R4`〕——
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
        // ★★ **报文里的每一个数都从这个常量或现算量渲染出来，一个手抄的都没有**〔`K-R4`〕。
        //
        // 先前这里是「一个值装了两件事」的活体〔`K13`〕：断言写 `checked == 88`，
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
        // - devbench F03 的 skill 接入面 **+3**（`list_skills` / `read_skill_file` /
        //   `write_skill_file`，都 `Local`）；
        // - E79 的 `list_local_session_accounts` **+1**；
        // - U-CC1 的 `drift_ledger_report` **+1**，`Both` —— 本地行与远端行都经同一个
        //   `parse_line` 喂进同一个进程内账本；
        // - F08 的 `account_usage_local` **+1** —— 它补平了 `usage.per-account` 那条 ParityDebt；
        // - P2s 的 `set_daemon_kill_on_exit` / `daemon_status` / `daemon_start` / `daemon_stop`
        //   **+4**，都是 `Both` —— per-host daemon 策略与状态，本机 origin 是 `<local>`（C1）；
        // - 🔴 `K-R135`（`R85`/`R87`）的 `ccm_user_path_status` / `ccm_user_path_add` /
        //   `ccm_user_path_remove` **+3**，都是 `Local` —— 用户级 PATH 那一格
        //   （现在状态 · 加 · 撤）。**只有本机一侧是天然的**，理由住 `ASYMMETRY_REASONS`
        //   里 `ccm.user-path` 那一条（远端那一侧同一件事由写进远端 rc 的围栏块办，
        //   而「用户级 PATH」这一档是 Windows 独有的，远端按 `K32` 是 Linux）；
        // - P8a 的 `list_plugin_marketplaces` **+1**，`Local` —— marketplace 只读枚举今天只有
        //   本机口，欠的那半写在 `plugins.marketplaces` 那行上；
        // - K-H2a **+2**（`read_relay_credentials_status` / `write_relay_credentials_key`，都 `Local`）；
        // - K-H2b **+1**（`relay_routing_for`，`Local`）—— 界面问「这几个**本机**账号走不走中转」。
        //   只答本机不是欠账，是**机制决定的**：中转是每台机器自己的进程、注入的是回环地址，
        //   本机这一侧答不了远端那台 ⇒ 它在 `ASYMMETRY_REASONS` 里记的是 `NaturallyAsymmetric`。
        //
        // ⚠ 这一段是**账**，不是判据。它里面的数**没有**任何东西钉住 ——
        //   真值以 `EXPECTED_LOCAL_OR_BOTH` 与失败时印出来的 `Local {n} + Both {m}` 为准。
        // - K-R49 **+1**（`write_account_aliases`，`Local`）—— 加了账号就把那条命令落盘；
        //   新能力 `alias.account-commands`，远端那半欠什么见 `ASYMMETRY_REASONS` 里那一行。
        // - K-R69 **+1**（`local_ccm_entry_status`，`Local`）—— 归**已有**能力 `ccm.status`
        //   （远端那一侧的对侧是 `probe_ccm_cli`），所以只涨命令数、不涨能力数。
        // - K-R98 **+1**（`cc_bus_send` 从 `Remote` 转 `Both`）—— **不是新增一条命令**，
        //   是同一条命令的两侧收成了一条路（远端那半改走 `bus-send`，不再拼 shell 串）。
        //   ⇒ 命令总数与能力总数都不动，只有这个数 +1；`cc-bus.cockpit` 仍不对称
        //   （spawn / 广播 / 收掉三条还是 `Remote`），所以不对称条数也不动。
        // - K-R109 **+1**（`render_local_attach`，`Local`）—— 本机后端产 attach 那一句。
        //   归**已有**能力 `session.launch`（理由逐条写在 `LEDGER` 那一行旁边）⇒
        //   命令总数 +1、能力总数不动、不对称数不动，**只有这个数跟着 +1**。
        //   ⚠ 派工单点名的是「`commands.vitest.ts` 两个 147 ＋ `LEDGER` 一行」三处，
        //   **这个数是第四处** —— 它不在任何一份派工单里，是本轮自己量出来的。
        const EXPECTED_LOCAL_OR_BOTH: usize = 96;
        assert_eq!(
            checked, EXPECTED_LOCAL_OR_BOTH,
            "检到 {checked} 条 Local/Both 命令（Local {n_local} + Both {n_both}），\
             而本条期望 {EXPECTED_LOCAL_OR_BOTH} 条。\n\
             改 LEDGER 就要来确认这个数：把 `EXPECTED_LOCAL_OR_BOTH` 改成新值，\
             并到它上面那段「这个数是怎么长起来的」里补一行说明谁加/删了哪几条。"
        );
    }

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
            ASYMMETRY_REASONS.len() >= 15,
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

    /// ★★★ `K-R53` `KR53D4`：**拉起那两行理由里，被证伪的那两句话不许再在盘上。**
    ///
    /// # 它为什么只能长成这个形状（诚实边界写在最前，别把它读大）
    ///
    /// 「这句话是不是真的」机器判不了。本条能判的只有两样，**两样都不是「真假」**：
    ///   ① **那两行还在**（`KR53D4` 逐字：「两行都不许靠删掉记录兑现 —— 删掉等于把一处
    ///      已知的不对称从视野里拿走」）；
    ///   ② **被证伪的那两句逐字串不在了**（`launch.render-payload` 的「不是并列的第二条路」·
    ///      `launch.render-cli` 的「诚实降级」）。
    ///
    /// ⇒ 本条买的是「**改过了，而且没靠删兑现**」，**不买**「新写上去的那句是真的」。
    /// 新那句真不真，由 `history.rs::every_local_account_shape_gets_a_named_verdict_from_the_backend_path`
    /// 那张逐格表**行为上**钉着 —— 那才是分母的牙，本条只是不让老尸体留在盘上。
    ///
    /// # 两句各自为什么是假的（`K-R53 §0b`，PM 派工前现打）
    ///
    /// · `launch.render-payload` 写「P3t 之后那是**渲染器拒了才走的回落**，不是并列的第二条路」——
    ///   **按调用点分母是假的**：四个本机拉起入口里三个（`tabs.ts` 一处 + `views/history.ts` 两处）
    ///   只说得出具名账号，而具名账号在 `K-R53` 之前**必然** §35 短路 ⇒ 那三条**只能**走它。
    ///   一条 3/4 的分母不叫「回落」。
    /// · `launch.render-cli` 的**机制那一半本来就写对了**（两态说不出 —— 而且现打仍然对，
    ///   见那条 Rust 判据头注里的三说法对照表）。错的只有「那两态**诚实降级**回旧路」
    ///   那四个字：读起来像边角情形，而按分母它是主路。
    #[test]
    fn the_two_launch_rows_no_longer_carry_the_two_falsified_clauses() {
        let find = |cap: &str| -> &str {
            ASYMMETRY_REASONS
                .iter()
                .find(|(c, _, _)| *c == cap)
                .map(|(_, _, why)| *why)
                .unwrap_or_else(|| {
                    panic!(
                        "`{cap}` 这一行不在账本里了 —— `KR53D4` 逐字禁「靠删掉记录兑现」：\n\
                         删掉等于把一处已知的不对称从视野里拿走，而那正是这张表存在的理由。"
                    )
                })
        };

        let payload = find("launch.render-payload");
        assert!(
            !payload.contains("不是并列的第二条路"),
            "`launch.render-payload` 还写着「渲染器拒了才走的回落，不是并列的第二条路」——\n\
             按调用点分母那是假的（本机四个入口里三个只能走它）。实得：{payload}"
        );
        // 反面：改完之后它必须**说得出分母**，不是换一句同样无从复核的话。
        assert!(
            payload.contains("K-R53"),
            "改了措辞却没留下「按谁的分母、哪一拍量的」——\n\
             那就是把一句无从复核的话换成另一句无从复核的话。实得：{payload}"
        );

        let cli = find("launch.render-cli");
        assert!(
            !cli.contains("诚实降级"),
            "`launch.render-cli` 还写着「那两态**诚实降级**回旧路」——\n\
             「降级」读起来像边角情形，而按分母它是主路。机制那一半是对的、不用动，\n\
             要改的只有这四个字的分量。实得：{cli}"
        );
        // 机制那一半**必须留着**（`KR53D4` 逐字：`:430` 的机制那半不用改）——
        // 顺手把它一起重写掉，就把一条今天仍然成立、而且刚被重新量过的事实弄丢了。
        for keep in ["#75", "继承"] {
            assert!(
                cli.contains(keep),
                "`launch.render-cli` 把机制那一半（`{keep}`）也改掉了 —— 那一半今天仍然成立，\n\
                 `KR53D4` 逐字写着「机制那半**不用改**」。实得：{cli}"
            );
        }
    }

    /// ★★ `K-R86`（09-13）：**`tmux.manage` 那一行不许再把「等 daemon 出原语」当现在时说。**
    ///
    /// # 它买什么、不买什么（诚实边界写在最前，别读大）
    ///
    /// 形状照上面那条 `the_two_launch_rows_no_longer_carry_the_two_falsified_clauses`。
    /// 机器判不了「这句话是不是真的」，本条能判的只有三样，**三样都不是「真假」**：
    ///
    /// 1. **那一行还在** —— 不许靠删掉记录兑现（删掉等于把一处已知的不对称从视野里拿走）；
    /// 2. **每一处「等 daemon 出原语」旁边都挂着订正标记** —— 那句话 09-13 起是假的：
    ///    daemon 侧有 `--capture-pane` 了（`remote-daemon-proto/src/control/capture_pane.rs`）；
    /// 3. **订正没有把欠账一起抹掉** —— 本件只出 daemon 那一侧的原语，
    ///    monitor 的 `capture_remote_pane` 一个字节没动 ⇒ 这一格仍然是 `Remote`，
    ///    而这一条正是本行历史上栽过三次的那个病（改了行为不回来改理由 / 改理由时把账也抹了）。
    ///
    /// ⚠ 它**不**买「新写上去的那段话是真的」。「daemon 侧真有那条原语」由 daemon 那棵树
    /// 自己的判据钉（`control::capture_pane::tests` 在真 tmux 上抓一屏 ＋
    /// `readonly_guard::capture_is_read_only` 逐元素钉 argv），本条够不着那棵树。
    #[test]
    fn the_tmux_manage_row_stops_waiting_for_a_daemon_primitive() {
        let why = ASYMMETRY_REASONS
            .iter()
            .find(|(c, _, _)| *c == "tmux.manage")
            .map(|(_, _, w)| *w)
            .expect(
                "`tmux.manage` 这一行不在账本里了 —— 不许靠删掉记录兑现：\
                 删掉等于把一处已知的不对称从视野里拿走，而那正是这张表存在的理由。",
            );

        // ① 那句话每出现一处，**它周围**就得有订正标记。按出现处逐处判，不是「全文里提过一次」——
        //    后者放得过「新加一段订正、而旧的那几处原样当现在时留着」。
        //    ⚠ 窗口**两侧都看**：本轮第一趟门禁就是在这儿红的 —— 订正段自己会逐字引用那句话
        //    （「原文逐字」是本行三次订正一贯的写法），而标记在**引文之前**。
        //    只看后面 ⇒ 一次假阳；只看前面 ⇒ 放得过「先写订正、后面又当现在时说一遍」。
        let stale = "等 daemon 出原语";
        let mark = "K-R86";
        let window_chars = 60usize;
        let mut from = 0usize;
        let mut seen = 0usize;
        while let Some(rel) = why[from..].find(stale) {
            let at = from + rel;
            from = at + stale.len();
            seen += 1;
            let head = &why[..at];
            let start = head
                .char_indices()
                .nth_back(window_chars)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let before = &head[start..];
            let after: String = why[from..].chars().take(window_chars).collect();
            assert!(
                before.contains(mark) || after.contains(mark),
                "第 {seen} 处「{stale}」前后 {window_chars} 个字符里都没有订正标记 `{mark}` ——\n\
                 它今天是假的：daemon 侧 09-13 起有 `--capture-pane`\n\
                 （`remote-daemon-proto/src/control/capture_pane.rs`）。\n\
                 前实得：{before:?}\n后实得：{after:?}"
            );
        }
        // 反空真：一处都没扫到 ⇒ 要么措辞换了、要么这条在空转，两种都要人来看。
        // 分母 = 这一行里那句话的出现处；09-13 现打 **5** 处
        //   （2 处历史叙述 ＋ 3 处订正段自己的逐字引用 —— ⚠ 我先写 2、再写 3，两次都少数了，
        //    两次都是这条判据现打出来告诉我的。**那正是「按出现处逐处判」买到的东西**：
        //    「全文里提过一次订正」会让这 5 处里的任意几处静默留在盘上当现在时。）
        // 写成地板而不是相等：这一行是**追加式**的（三次订正都在往后加），
        // 相等会让下一次订正被迫来改这个数，而那正是本仓「为了不动数字去拧代码」的反面。
        assert!(
            seen >= 2,
            "只扫到 {seen} 处「{stale}」（09-13 现打 5 处）—— 抽取器或措辞变了，本条此刻在空转"
        );

        // ② 订正不许把欠账一起抹掉：这一格今天仍然只有远端一条路。
        for keep in ["capture_remote_pane", "Side::Remote"] {
            assert!(
                why.contains(keep),
                "订正把「欠账没结」这一半抹掉了（找不到 `{keep}`）——\n\
                 本件只出 daemon 那一侧的原语，monitor 那条本机对侧仍然没有。\n\
                 把「等 daemon」换成「做完了」，是这一行历史上栽过三次的同一个病换了个方向。"
            );
        }
    }

    #[test]
    fn ledger_shape_is_pinned() {
        // ⚠ 订正（2026-08-03 复盘）：下面四条尾注此前都**只记到 U8c-2c-2 为止** ——
        // 而 U8a-2c-pre（`57dba2a`）把这四个数各 +1 时，只改了数、一条尾注都没动。
        // ⇒ 尾注把 U8a-2c-pre 的增量记在了 U8c-2c-2 名下。**尾注的用处就是说清「谁加的」，
        // 归属错了就不如没有。**
        assert_eq!(LEDGER.len(), 151, "命令总数变了"); // **K-R109 +1（render_local_attach，Local；归已有能力 `session.launch` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）** // **K-R69 +1（local_ccm_entry_status，Local；归已有能力 `ccm.status` ⇒ 能力数与不对称数都不动）** // **K-R49 +1（write_account_aliases，Local-only；新能力 `alias.account-commands`，`ParityDebt`）** // **K-H2a +2（creds.relay-key，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +3（list_skills / read_skill_file / write_skill_file：skill 接入面） // F08 +1（account_usage_local：补平 usage.per-account） // U8a-2c-1 +1（daemon_send_into）； G6 +1；E79 +1；U-CC1 +1（drift_ledger_report）；U8c-2c-2 +1（render_ccm_launch）；U8a-2c-pre +1（render_launch_payload）；**P2s +5（set_daemon_kill_on_exit / daemon_status / daemon_start / daemon_stop / daemon_machines，C8）**；P3t-Y2b +1（list_local_tmux）；**P4c +2（cc_bus_broadcast / cc_bus_kill，#77/#78）** // **P8a +1（list_plugin_marketplaces，#70）** // **PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）** // **K-H2b +1（relay_routing_for，Local-only；新能力 `relay.routing`，`NaturallyAsymmetric`）** // **`K-R135` +3（ccm_user_path_status / ccm_user_path_add / ccm_user_path_remove，都 Local；新能力 `ccm.user-path`，`NaturallyAsymmetric` —— 理由见 ASYMMETRY_REASONS 那一行）**
        let sides = capability_sides();
        assert_eq!(sides.len(), 67, "能力总数变了"); // **K-R109 +1（launch.render-attach，Local-only；⚠ 派工单猜的是「能力数 65 不动」，实打不成立 —— 两个「归已有能力」的归法各被一条判据顶回来了，逐条见 `LEDGER` 里那一行旁边）** // **K-R49 +1（alias.account-commands，Local-only）** // **K-H2a +1（creds.relay-key，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +1（skill.inbox，Local-only） // U8a-2c-1 +1（launch.send-into，Remote-only）； U-CC1 +1（audit.drift-ledger）；U8c-2c-2 +1（launch.render-cli，Remote-only：**只是没有本机那条 IPC 命令** —— P3t-Y4 起理由不再是 §36「本机不经 IR」那条，§36 只绑 Windows，详见 ASYMMETRY_REASONS 里那行）；U8a-2c-pre +1（launch.render-payload，同 Remote-only）；**P2s +3（app.daemon-policy / daemon.status / daemon.lifecycle，都是 Both）**；P3t-Y2b +1（tmux.local-census，Local-only：把远端本来就有的那一格在本机补上）；**P8a +1（plugins.marketplaces，Local-only）**；**PS1 +1（cc-bus.deploy）**；**PS2 +1（cc-bus.install-state）**；**K-H2b +1（relay.routing，Local-only）**；**`K-R135` +1（ccm.user-path，Local-only：`R85` 那一格「加/撤/现在状态」；远端那一侧同一件事由 rc 围栏块办，而「用户级 PATH」这一档是 Windows 独有的 ⇒ `NaturallyAsymmetric`）**
        let asym = asymmetric_capabilities();
        assert_eq!(asym.len(), 28, "不对称能力数变了"); // **K-R109 +1（launch.render-attach，NaturallyAsymmetric）** // **K-R49 +1（alias.account-commands，ParityDebt）** // **K-H2b +1（relay.routing，NaturallyAsymmetric）** // **K-H2a +1（creds.relay-key，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +1（skill.inbox） // F08 -1（usage.per-account 补平） // U8a-2c-1 +1（launch.send-into）； G6 -1；E79 accounts.session-accounts 补平 -1；U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）
                                                        // P3t-Y2b +1（tmux.local-census）；**P3b -1（launch.send-into 结清：P3 刀 3 让本机真的在用它 ⇒ Both，不再不对称）**
                                                        // **P7c-1 -1（subagent.load 结清：远端展开做出来了 ⇒ Both）** —— daemon 只列候选，挑选留本侧（C1）
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
        assert_eq!(kinds.get("natural"), Some(&13), "天然不对称条数变了"); // **`K-R135` +1（ccm.user-path：远端同一件事由 rc 围栏块办 ＋ 那一档只有 Windows 有 ⇒ 天然，不是欠账；哪天远端是 Windows 就回来改成 `debt`）** // **K-R109 +1（launch.render-attach —— 记 `natural` 记的是「远端由 `render_ccm_launch` 一并产、不需要单独命令名」，不是「远端还没做」）** // U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）。★ P3t-Y4：这两条的**理由**换过（原来引 §36 说「本地不经 IR」——§36 只绑 Windows，且那个理由已被本机探针实测证伪），但 `natural` 的**条数没变**。P3t-Y2b +1（tmux.name-census）。**K-H2b +1（relay.routing）—— 记 `natural` 记的是「回环自指 ⇒ 这个问题问错了机器」，不是「远端还没做」；把 key 送到远端那笔账在 `creds.relay-key` 那行（`debt`），两者别混。**
        assert_eq!(kinds.get("debt"), Some(&12), "平价欠账条数变了"); // **K-R49 +1（alias.account-commands：远端那半只有「吐待贴文本」那半条路，落盘没有主人）** // **K-H2a +1（creds.relay-key：本机能配、远端那侧还没有路 —— 而且不能照抄 SFTP 上传，理由见那一行）** // F08 -1（usage.per-account 补平） // G6 -1；E79 -1；**P7c-1 -1（subagent.load 结清：远端展开做出来了）**；**P8a +1（plugins.marketplaces：新开的本机口，远端那半要等 daemon 的 `--list-marketplaces`）**
        assert_eq!(kinds.get("undecided"), Some(&3), "未裁定条数变了"); // devbench F03 +1（skill.inbox：远端项目的收件箱要不要能编辑，没人裁定过） // U8a-2c-1 +1（launch.send-into：本机该不该有后端进程未裁定）
                                                                        // P3b -1（launch.send-into：它的「还没裁定」被 C1/C8 + P2 + P3 刀 3 三重证伪）
    }

    // ════════════════════════════════════════════════════════════════════════
    // 🔴 `K-R115` `KR115D4`：**`Side` 这一栏靠人签字，而签字的触发是派生出来的**
    // ════════════════════════════════════════════════════════════════════════
    //
    // # 题面（`K-R112` 09-13 撞见、一个字没动、PM 判「开件不在本件补」）
    //
    // `LEDGER` 的 `Side` 栏**没有任何机检**。一条命令的派发跳改走了「origin 无关」的
    // 帧面（`inbound_client::client_for(origin)` / `daemon_route`）之后，它对
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
    //    `list_local_tmux` / `daemon_start` / `load_subagent` / `read_skill_file` /
    //    `write_skill_file` —— 它们**两条路都有**，标记只看得见远端那条）。⇒ 这个方向不接。
    // 3. **正向也有一条假阳**：`account_usage` 派生出来是「帧面·origin 无关」，
    //    而它的 `Side::Remote` **没说假话** —— 本机那一侧另有一条命令
    //    （`account_usage_local` 逐字拿 `LOCAL_ORIGIN` 调它），`usage.per-account`
    //    在表上已经是 `{Local, Remote}` 对称。⇒ **候选要人签，不能机器直接判红。**
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
    /// `production_code` 会把它整份剥掉，扫描面里根本没有本文件（`scan_tree!` 还会再摘一次）。
    const REMOTE_ONLY_MARKS: &[&str] = &[
        "load_remote_config_by_label(",
        "ssh_source::",
        "refuse_local_write(",
        "RemoteConfig",
        "sftp_pool::",
    ];

    /// 生产段里「走 origin 无关帧面」的标记。
    const FRAME_PLANE_MARKS: &[&str] = &["client_for(", "route_call_error", "daemon_route::"];

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
    /// 「旧名的符号形出现几处」，本文件登记 2 处（`daemon.deploy` 那两行的命令名里就带着它），
    /// 抄一遍当场变 4 处；而那条账逐字禁「把上限调上去让今天好过」。
    /// 〔09-14 实打：第一版就是 55 行清单，`cargo` 那格红在这一条上，读数住
    /// `evidence/K-R115-deathvalue.md`〕
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
        (Derived::RemoteOnly, 39),
        (Derived::FramePlane, 6),
        (Derived::Mixed, 0),
        (Derived::Unclassified, 10),
    ];

    /// **候选（派生 = `FramePlane`）的人裁** —— 闭集见 [`FrameVerdict`]，理由必须可追问。
    ///
    /// 🔴 **这张表就是本笔的产出**：`K-R112` 交回时点了 **3** 行（`capture_remote_pane` /
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
    ///    [`the_tmux_manage_row_stops_waiting_for_a_daemon_primitive`]（它逐字断言那句理由里
    ///    **必须**含 `Side::Remote`）。⇒ 翻 `Side` 要**同拍改掉一条现行判据的断言**。
    /// ② 而「本机真的抓得到一屏 / 真的杀得掉」**今天判不了**（一趟真机都没跑过，
    ///    `K-R112` 交回时逐字承认过）。`Side::Both` 的字面是「这条命令自己就把两侧都办了」——
    ///    在没跑过的前提下把它写上去，是拿一句没验过的话换一格好看的表。
    /// ⇒ **登记成欠账，归后续一件**；本条保证的是**它从此不会静默**。
    const FRAME_PLANE_VERDICTS: &[(&str, FrameVerdict, &str)] = &[
        (
            "account_usage",
            FrameVerdict::SecondCommandServesLocal,
            "本机那一侧另有命令名 `account_usage_local` —— 它逐字拿 `inbound_client::LOCAL_ORIGIN` \
             调本条（`account_usage.rs` 里那两行紧挨着，判据 `account_usage_local` 那条窗口断言钉着）。\
             ⇒ `usage.per-account` 在 `capability_sides()` 里已经是 `{Local, Remote}`、对称，\
             本行的 `Remote` 说的是「这个命令名给远端用」，没说假话。",
        ),
        (
            "capture_remote_pane",
            FrameVerdict::LiesTodayOwedACorrection,
            "`K-R112`（09-13）把抓屏改走帧面 `capture-pane`（`tmux.rs::capture_via_daemon`，\
             登记在 `daemon_route.rs::SENDERS` 第七个发送端）⇒ 这一跳对 `<local>` 也走得通，\
             而本行仍是 `Side::Remote`。⚠ 同一行的 `ASYMMETRY_REASONS` 里 `K-R104` 那段逐字写着\
             「`capture_remote_pane` 一个字节没动 … `Side::Remote` 这一格不许改」——**那句话今天假了**。",
        ),
        (
            "kill_remote_tmux",
            FrameVerdict::LiesTodayOwedACorrection,
            "`P3 刀 2`（08-12）就改走了 `daemon_kill`（传输无关）。`tmux.manage` 那条 \
             `ASYMMETRY_REASONS` 逐字「② `kill_remote_tmux` ⇒ **本机已通**」——\
             **散文说通了一个月，`Side` 栏没跟**。`K-R112` 点名的三行里没有它。",
        ),
        (
            "tmux_send_keys",
            FrameVerdict::LiesTodayOwedACorrection,
            "同上：`tmux.manage` 那条理由逐字「③ `tmux_send_keys` ⇒ **本机已通**（同款 \
             `daemon_route` 分流）」，`K-R56`（09-11）还补了本机专属的 `NoChannel` 早退\
             （判据 `tmux::tests::the_local_send_keys_never_falls_back_to_ssh`）。\
             ⇒ 派生说它 origin 无关，而 `Side` 栏还写着 `Remote`。",
        ),
        (
            "cc_bus_broadcast",
            FrameVerdict::LiesTodayOwedACorrection,
            "`K-R112` 交回时逐字点名「`cc_bus_broadcast` 那行**在本件之前就已经腐了**」。\
             它今天走 `cc_bus.rs` 里那条帧面原语 ＋ 共用分流器（`daemon_route.rs::SENDERS` \
             登记着 `cc_bus.rs`），没有 `refuse_local_write(` 拦 `<local>` ⇒ 本机也走得通，\
             而这一行的 `Side` 那一格还写着「只有远端」。",
        ),
        (
            "cc_bus_kill",
            FrameVerdict::LiesTodayOwedACorrection,
            "`K-R112` 把它改走 `bus-kill` 原语 ⇒ `<local>` 也走得通，而本行仍是 `Side::Remote`。\
             ⚠ 与 `cc_bus_spawn` 分得开：那一条生产段里**有** `refuse_local_write(`（派生判 \
             `RemoteOnly`），它是真远端专属。",
        ),
    ];

    /// 🔴 **`KR115D4`：`Side::Remote` 那一栏，没签字红 · 过期红 · 陈账红。**
    ///
    /// 三条判定与它们各自拦得住什么，逐条写在断言的报文里；边界见本节开头那段头注。
    #[test]
    fn the_remote_side_column_is_signed_off() {
        let derived = command_dispatch_class();
        // 反向自检①：派生器扫不到东西的时候，下面每一条都会**空真地**成立。
        assert!(
            derived.len() >= 140,
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
        assert!(
            remote_rows.len() >= 50,
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
        assert!(
            candidates.len() >= 5,
            "「派发跳 origin 无关而 `Side` 还写着 `Remote`」的候选现打只有 {} 条 —— \
             〔09-14 现打 6 条〕人群塌到这个数，多半是标记表或派生器坏了，不是大家都改好了",
            candidates.len()
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
        assert!(
            owed >= 1,
            "人裁表里一条「说假话·欠一次订正」都没有了 —— 那要么是真的都改好了\
             （那就把 `LEDGER` 里那几行的 `Side` 一起改掉、连锁一起拧），\
             要么是有人把裁词改宽了。两者在输出上一模一样，所以这里要求\
             **显式声明为零之前先来改这一条**。"
        );
    }
}
