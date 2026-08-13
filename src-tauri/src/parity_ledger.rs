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
        ("probe_ccm_cli", "ccm.status", Side::Remote),
        ("cc_integration_install", "ccm.install", Side::Local),
        ("install_remote_ccm_helper", "ccm.install", Side::Remote),
        ("cc_integration_uninstall", "ccm.uninstall", Side::Local),
        ("uninstall_remote_ccm_helper", "ccm.uninstall", Side::Remote),
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
        // ② 「本机没有后端进程」——P2 交付了：`local_backend.rs:795` 逐字
        //    `inbound_client::register(LOCAL_ORIGIN, client)`，`client_for("<local>")` 实测通。
        // ③ 而 **P3 刀 3 让生产段真的用它了**：`runLocalResumeIntoExistingTmux`
        //    以 `<local>` 调 `daemon_send_into` 就地 resume。
        //
        // ⇒ 这条不是「改个分类」，是**这条能力真的两侧都有了**。
        ("daemon_send_into", "launch.send-into", Side::Both),
        ("render_ccm_launch", "launch.render-cli", Side::Remote),
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
        // 写面仍是远端专属（本机对侧未做，见 `cc_bus.rs::refuse_local_write`）。
        ("cc_bus_send", "cc-bus.cockpit", Side::Remote),
        ("cc_bus_spawn", "cc-bus.cockpit", Side::Remote),
        // P4c（08-12，#77/#78）：广播 + 收掉。同为写面 ⇒ 同样远端专属。
        ("cc_bus_broadcast", "cc-bus.cockpit", Side::Remote),
        ("cc_bus_kill", "cc-bus.cockpit", Side::Remote),
    ];

    /// **不对称能力的理由**。键集合必须**恰好等于**从 `LEDGER` 算出来的不对称集合。
    const ASYMMETRY_REASONS: &[(&str, Asym, &str)] = &[
        ("accounts.trust", Asym::ParityDebt, "同 accounts.list：预信任检查只有远端有 —— 命令面实测只有 `check_account_trust`（`Side::Remote`），本机零对侧。归 L3。"),
        ("acct-iso.check", Asym::ParityDebt, "本机同样需要「这台装没装 cc-acct-iso」的检测（切号要靠它），今天只能查远端 —— 命令面实测只有 `check_remote_acct_iso`，本机零对侧；而本机确实**用得着**它：`local_accounts.rs` 头注逐字说这份数据有三个读者，其中写侧就是 `cc-acct-iso`。归 L3。"),
        ("acct-iso.deploy", Asym::NaturallyAsymmetric, "vendored 副本要**传到**远端才能用（`deploy_remote_acct_iso`）；本地就在本机、不存在传输这一步。⚠⚠ **P3b 标疑（08-12）：这条理由属于「从未被验证过」那一类，别当它已经核过。** 「不存在传输」是真的，但**「不存在安装」没人量过** —— 实测本机 `~/.local/bin` 下确实躺着 `cc-acct-iso`（`config_surface.rs:1062` 记着那 12 条 `cc-*`），而它是**怎么到那儿的**、要不要 monitor 管，本表从来没答过。⇒ 若答案是「要 monitor 管」，这条就该从 `natural` 变 `ParityDebt`。归 `P3b` 的后续或 `L3`。"),
        ("skill.inbox", Asym::Undecided, "devbench F03：skill 接入面今天只读写**本机工作目录**下的 `.claude/planned-build/INBOX.txt`。远端项目也可能有同一份结构（那边的 `.claude/` 一样在），技术上走 SFTP 就能读写 —— **但「远端项目的收件箱要不要能在这里编辑」没人裁定过**。⇒ 刻意记 `Undecided` 而不是 `NaturallyAsymmetric`：后者会替产品做主说「本地不需要」，而事实是**没想过**。⚠ 若将来要做，写面围栏那三道得先想清楚远端版怎么算（`canonicalize` 在远端不成立）。"),
        ("tmux.local-census", Asym::NaturallyAsymmetric, "「本机今天有哪些 tmux 会话」。★ P3 刀 2 的 UI 半把它从「只回名字」放宽到「回整条会话」——杀会话的菜单必须按 `@ccm_sid` 认归属，按 `<sid8>-cc` 前缀猜与 §30 逐字禁的「按目录回退猜」是同一类错。**反向缺口，且是天然的**：远端问同一个问题**已经有答案** —— `list_remote_tmux` 一次性 SSH `tmux ls` 就是它，前端 `pickFreshTmuxName(sid, existing)` 拿的正是那份。本机没有 SSH 那一跳，所以要一个自己的口；开它不是本机多了什么能力，是**把远端本来就有的那一格在本机补上**。⇒ 记 `NaturallyAsymmetric` 而不是 `ParityDebt`：欠的是本机这一侧，而本行一落地就已经补平，没有留下去处。★ 它读的是 daemon 推来的 tmux 快照而不是现跑 `tmux ls`，理由与射程见 `tmux.rs::local_tmux_names` 头注：那份快照由 `session-created/closed/renamed` 三条 hook 驱动，**恰好就是改变名字集合的那三件事** ⇒ 对这个问题它是权威的，对「pane 前台命令变了没有」才是陈旧的（那条已被 devbench F08 裁定不开口）。"),
        ("plugins.marketplaces", Asym::ParityDebt, "`P8a`：列 marketplace（来源 / 落点 / 更新时间 / 它**声明**的插件数）今天**只有本机**。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端的 `~/.claude/plugins/` 一样在，daemon 早就会读远端 claude 目录（`--list-projects` / `--list-sessions` / `--list-subagents` 三条现成的形状）。欠的是**一条 daemon 子命令**（`--list-marketplaces`）+ 一次 BUILD_ID/协议文档/内嵌重编，那是另一件事的体量，本件是梯队 5 的只读面 ⇒ 如实记欠，**不假装两侧都有**。★ 它与 `local_read_surface_registry` 里 `plugins.rs` 那条 `reader` 的退役条件是**同一条**：daemon 补上那条子命令，本机改走后端、远端这半一起补平 —— **一件事清两笔账**。⚠ 另记一条**本行答不了的**：本行说的是「有哪些 marketplace」，**不是**「装了/启用了哪些插件」——后者今天**两侧都没有真相源**（待决 `U10d`），那不是平价问题，是那份数据在盘上根本不存在。"),
        ("acct-iso.shellinit", Asym::ParityDebt, "本机切号同样要 shellinit 文本（`cc-acct-iso shellinit` 那句 `export CLAUDE_CONFIG_DIR=<默认账号>`），今天只能给远端生成 —— 命令面零本机对侧。归 L3。"),
        ("audit.config-surface", Asym::ParityDebt, "**反向缺口**（本地能答、远端答不出）——§40 表里已逐行记明：本页明写不连 SSH，10 行里 7 行对远端恒返回「未确定」。"),
        ("cc-bus.cockpit", Asym::ParityDebt, "★ **P4c 订正（08-12）：原理由已经过期，而过期的正是 `P4a` 那一刀造成的。** 原文写「cc_bus.rs 的 **5 个 IPC** 全走 origin+ssh、**零本机读取路径**」——`P4a`（08-12）把**读面三条**（`read_cc_bus_state` / `check_cc_bus_agent_online` / `read_cc_bus_inbox`）做成了本机可用（同一条命令串，只是不包进 ssh；本机 `~/.cc-bus/agents.tsv` 实测 86 行），它们今天是 `Both`。⇒ 「零本机读取路径」是假的，「5 个」也变成了 7 个（`P4c` 加了 `cc_bus_broadcast` / `cc_bus_kill`）。**今天真正的欠账只剩写面**：`cc_bus_send` / `cc_bus_spawn` / `cc_bus_broadcast` / `cc_bus_kill` 四条对 `<local>` 走 `refuse_local_write`，本机没有对侧（`P4a §0c` 量过代价：本机写面归 `P4b`，而 `P4b` 签收的是 cc-spawn 的复用那一刀，没交付写面）。★ 这条订正本身是 `P3b §0b` 的 **A 类（过期）**活样本，而制造它的是 `P4a` —— **改了行为没回来改理由，账本当天就开始撒谎**，本轮第二次（第一次是 `P4d-Y5` 改 `capture_remote_pane` 那次）。"),
        ("ccm.install-ui", Asym::Undecided, "本机安装向导有「扫 PATH 选装到哪」+「预览要写的文本」两步；远端 `install_remote_ccm_helper(cfg, profile)` 一步到位、没有这两步。**是欠账还是刻意简化，需要产品判断**——本表不替它裁定。"),
        ("daemon.deploy", Asym::NaturallyAsymmetric, "★★ **P3b 结清（08-12）：理由整个换掉 —— 原来那句是假的。** 原文写「§40 天然不对称白名单第 3 条：本地会话由 `watcher.rs` 直接读 jsonl，**根本不需要 daemon**」，被 P2z + P2 + P2s 三件直接证伪：本机**需要** daemon（入方向通道、每台机开关、tmux 帧都靠它），而且**已经会自部署** —— `local_backend.rs::extract_embedded_to`（exe 旁没有 sidecar 就把内嵌那份释放到 `~/.cc-monitor/bin`）。真正的不对称只剩一格：**本机那次释放不经一条 IPC 命令**，是宿主启动时自己做的（`lib.rs` 的启动段），所以命令面上没有本机对侧。⇒ 记 `natural` 记的是「不需要一条命令」，不是「不需要 daemon」。"),
        ("launch.render-payload", Asym::NaturallyAsymmetric, "兜底那支（`container:\"none\"`）的载荷渲染。本机走 `history.rs::build_local_*_command` —— P3t 之后那是**渲染器拒了才走的回落**，不是并列的第二条路。⚠ P3t-Y4 订正：原文引 §36 当依据，那是把一条讲 **Windows**、逐字禁「本地渲染器读 `plan.env`」的窄铁律读宽了。"),
        ("launch.render-cli", Asym::NaturallyAsymmetric, "`ccm 调用行`的渲染。★★ **P3t-Y4 把这条的理由整个换了 —— 原来那个已被实测证伪。** 原文说这条不对称是「本地渲染必须在目标机器上做（要现场探 `command -v cc`，TS 无法预先渲染好交给它）」造成的。**本机就在本机**：P3t-Y2 的 `ccm_probe::probe_local_ccm()` 直接跑一次 `bash -lic` 就拿到了版本与完整能力集，比远端那条 ssh 往返还便宜 ⇒ 那个理由不成立。真正的不对称是**本机账号三态里有两态 CLI 说不出**：`Named{config_dir}` 只有目录没有名字（CLI 只会 `--account <名字>`），`None` 是「继承环境」而 CLI 语法里没有这一态（映成 `--base` 就是把继承偷换成显式清空 = #75 病灶）。那两态诚实降级回旧路。⇒ 本行仍 `natural`，但它记的是**语法窄一格**，不是「渲染必须在目标机器上做」。补不补见 ROADMAP `U10`。"),
        ("mcp.list-origins", Asym::NaturallyAsymmetric, "`list_remote_mcp_origins` 答的是「哪几台远端有 MCP 配置」——「有哪些 origin」这个问题在本机侧退化成一台，没有可列的集合。⚠ 注意它与 `daemon_machines` 不同：那条**包含**本机（`LOCAL_ORIGIN`），因为它答的是「哪几台有 daemon」而本机也有。"),
        ("panorama.code-graph", Asym::Undecided, "**本表交出的最大一处新发现**：21 条命令全部只吃本机 `repo` 路径。远端 repo 的代码图谱既没做、也没在任何计划里登记过。**不擅自判它是天然不对称**——那需要产品判断（远端开发是不是本工具的场景）。登记待裁定。"),
        ("port-forward", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 2 条：本地没有「转发到自己」这个需求。"),
        ("search.index", Asym::NaturallyAsymmetric, "远端**不建索引**：`search_history` 对远端是实时 SSH fan-out（其头注自陈「本地内存索引查询与远端 fan-out 并发」）。索引是本机侧的实现细节，不是一项对外能力。"),
        ("session.tasks", Asym::ParityDebt, "**实测**：`get_session_tasks` 走 `tasks_root_for_current_claude_dir()` → `paths::resolve_claude_dir()`，读的是**本机**目录。远端会话的任务在远端机器上 ⇒ 远端 tab 拿不到任务列表。"),
        ("sftp.file-panel", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 1 条：本地有操作系统的文件管理器，不需要它。"),
        ("ssh.host-config", Asym::NaturallyAsymmetric, "本地按 §40 的定义就是「**不走 ssh** 的远端」⇒ ssh 目标的枚举/解析/导入/连通性测试/公钥推送在本地没有对应物。"),
        
        ("tmux.manage", Asym::ParityDebt, "★★ **P3b E 阶段重量（08-12）：那句预言「POSIX 本地落地后自动就有」——三分之三对、四分之一错。** 原文只写「`ccm` 全套修饰本地『无』」+ 那句预言，没说是哪几条命令。逐条量：① `list_remote_tmux` ⇒ **本机已有对侧** `list_local_tmux`（P3t-Y2b + P3 刀2-UI，读 daemon 推来的快照）；② `kill_remote_tmux` ⇒ **本机已通**（P3 刀 2 后端：`daemon_kill` 传输无关，且本机专属错误文案已加）；③ `tmux_send_keys` ⇒ **本机已通**（同款 `daemon_route` 分流）；④ `capture_remote_pane` ⇒ **仍无本机对侧**。⚠ **P4d-Y5（08-12）改了它的一半**：原文接着写「它 `load_remote_config_by_label(&origin)`，对 `<local>` 会报「未找到远端配置」」—— **那半句今天已经假了**，本机分支已补上，报的是真实原因（本机 tmux 快照只带会话名不带屏幕内容，要预览得现抓一次 pane）。⇒ 假话没了，**欠账没结**：能不能预览这件事一点没变，仍等 daemon 出原语。★ 这条订正本身是 `P3b §0b` 的 A 类（过期）活样本，而制造它的正是 P4d 那一刀 —— 改了行为不回来改理由，账本当天就开始撒谎。⇒ 欠账**只剩画面预览这一格**，而它不是「自动就有」的：预览要么现跑 `capture-pane`（本机可以，但那是第二条取数路），要么等 daemon 出原语（`P4d`）。归 **P4d**，不再归 L1/L2。"),

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
        for (cmd, _, side) in LEDGER {
            if !matches!(side, Side::Local | Side::Both) {
                continue;
            }
            let Some(params) = sigs.get(*cmd) else {
                continue;
            };
            let flat: String = params.split_whitespace().collect::<Vec<_>>().join(" ");
            checked += 1;
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
        assert_eq!(
            checked, 84,
            "检到 {checked} 条 Local/Both 命令（真实应为 84 = Local 53 + Both 31；\
             ★ P4a（08-12）把 cc-bus **读面三条**从 Remote 转成 Both（本机跑同一条命令串，\
             只是不包进 ssh）⇒ Both 28→31、总数 80→83；\
             devbench F03 的 skill 接入面是 +3（list_skills / read_skill_file / write_skill_file，\
             都 Local）；\
             E79 的 `list_local_session_accounts` 是 +1；U-CC1 的 `drift_ledger_report` 是 +1，\
             它是 Both —— 本地行与远端行都经同一个 `parse_line` 喂进同一个进程内账本；\
             **F08 的 `account_usage_local` 是 +1** —— 它补平了 `usage.per-account` 那条 ParityDebt；\
             **P2s 的 `set_daemon_kill_on_exit` / `daemon_status` / `daemon_start` / `daemon_stop` 是 +4**，都是 Both —— per-host daemon 策略与状态，本机 origin 是 `<local>`（C1））\
             ；**`P8a` 的 `list_plugin_marketplaces` 是 +1**，`Local` —— marketplace 只读枚举\n\
             今天只有本机口，欠的那半写在 `plugins.marketplaces` 那行上\n\
             ——改 LEDGER 就要来确认这个数"
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

    #[test]
    fn ledger_shape_is_pinned() {
        // ⚠ 订正（2026-08-03 复盘）：下面四条尾注此前都**只记到 U8c-2c-2 为止** ——
        // 而 U8a-2c-pre（`57dba2a`）把这四个数各 +1 时，只改了数、一条尾注都没动。
        // ⇒ 尾注把 U8a-2c-pre 的增量记在了 U8c-2c-2 名下。**尾注的用处就是说清「谁加的」，
        // 归属错了就不如没有。**
        assert_eq!(LEDGER.len(), 140, "命令总数变了"); // devbench F03 +3（list_skills / read_skill_file / write_skill_file：skill 接入面） // F08 +1（account_usage_local：补平 usage.per-account） // U8a-2c-1 +1（daemon_send_into）； G6 +1；E79 +1；U-CC1 +1（drift_ledger_report）；U8c-2c-2 +1（render_ccm_launch）；U8a-2c-pre +1（render_launch_payload）；**P2s +5（set_daemon_kill_on_exit / daemon_status / daemon_start / daemon_stop / daemon_machines，C8）**；P3t-Y2b +1（list_local_tmux）；**P4c +2（cc_bus_broadcast / cc_bus_kill，#77/#78）** // **P8a +1（list_plugin_marketplaces，#70）**
        let sides = capability_sides();
        assert_eq!(sides.len(), 60, "能力总数变了"); // devbench F03 +1（skill.inbox，Local-only） // U8a-2c-1 +1（launch.send-into，Remote-only）； U-CC1 +1（audit.drift-ledger）；U8c-2c-2 +1（launch.render-cli，Remote-only：**只是没有本机那条 IPC 命令** —— P3t-Y4 起理由不再是 §36「本机不经 IR」那条，§36 只绑 Windows，详见 ASYMMETRY_REASONS 里那行）；U8a-2c-pre +1（launch.render-payload，同 Remote-only）；**P2s +3（app.daemon-policy / daemon.status / daemon.lifecycle，都是 Both）**；P3t-Y2b +1（tmux.local-census，Local-only：把远端本来就有的那一格在本机补上）；**P8a +1（plugins.marketplaces，Local-only）**
        let asym = asymmetric_capabilities();
        assert_eq!(asym.len(), 21, "不对称能力数变了"); // devbench F03 +1（skill.inbox） // F08 -1（usage.per-account 补平） // U8a-2c-1 +1（launch.send-into）； G6 -1；E79 accounts.session-accounts 补平 -1；U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）
        // P3t-Y2b +1（tmux.local-census）；**P3b -1（launch.send-into 结清：P3 刀 3 让本机真的在用它 ⇒ Both，不再不对称）**
        // **P7c-1 -1（subagent.load 结清：远端展开做出来了 ⇒ Both）** —— daemon 只列候选，挑选留本侧（C1）
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
        assert_eq!(kinds.get("natural"), Some(&10), "天然不对称条数变了"); // U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）。★ P3t-Y4：这两条的**理由**换过（原来引 §36 说「本地不经 IR」——§36 只绑 Windows，且那个理由已被本机探针实测证伪），但 `natural` 的**条数没变**。P3t-Y2b +1（tmux.name-census）
        assert_eq!(kinds.get("debt"), Some(&8), "平价欠账条数变了"); // F08 -1（usage.per-account 补平） // G6 -1；E79 -1；**P7c-1 -1（subagent.load 结清：远端展开做出来了）**；**P8a +1（plugins.marketplaces：新开的本机口，远端那半要等 daemon 的 `--list-marketplaces`）**
        assert_eq!(kinds.get("undecided"), Some(&3), "未裁定条数变了"); // devbench F03 +1（skill.inbox：远端项目的收件箱要不要能编辑，没人裁定过） // U8a-2c-1 +1（launch.send-into：本机该不该有后端进程未裁定）
        // P3b -1（launch.send-into：它的「还没裁定」被 C1/C8 + P2 + P3 刀 3 三重证伪）
    }
}
