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
        ("cc_get_auto_launch", "app.auto-launch", Side::Both),
        ("cc_set_auto_launch", "app.auto-launch", Side::Both),
        ("frontend_perf_log", "app.diagnostics", Side::Both),
        ("get_diagnostics_config", "app.diagnostics", Side::Both),
        ("set_diagnostics_config", "app.diagnostics", Side::Both),
        ("get_log_file_info", "app.logs", Side::Both),
        ("open_log_file", "app.logs", Side::Both),
        ("open_log_dir", "app.logs", Side::Both),
        ("load_config", "app.config", Side::Both),
        ("save_config", "app.config", Side::Both),
        ("get_data_paths", "app.data-paths", Side::Both),
        ("forget_session", "session.forget", Side::Both),
        ("list_session_activity", "session.activity", Side::Both),
        ("list_active_sessions", "session.list-active", Side::Both),
        ("update_history_metadata", "history.metadata", Side::Both),
        ("list_last_accounts", "accounts.last-used", Side::Both),
        ("search_history", "search.history", Side::Both),
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
        ("load_subagent", "subagent.load", Side::Local),
        ("get_session_tasks", "session.tasks", Side::Local),
        ("config_surface_report", "audit.config-surface", Side::Local),
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
        ("check_account_trust", "accounts.trust", Side::Remote),
        ("account_usage", "usage.per-account", Side::Remote),
        ("deploy_remote_acct_iso", "acct-iso.deploy", Side::Remote),
        ("check_remote_acct_iso", "acct-iso.check", Side::Remote),
        (
            "remote_acct_iso_shellinit",
            "acct-iso.shellinit",
            Side::Remote,
        ),
        ("list_remote_tmux", "tmux.manage", Side::Remote),
        ("capture_remote_pane", "tmux.manage", Side::Remote),
        ("kill_remote_tmux", "tmux.manage", Side::Remote),
        ("tmux_send_keys", "tmux.manage", Side::Remote),
        ("read_cc_bus_state", "cc-bus.cockpit", Side::Remote),
        ("check_cc_bus_agent_online", "cc-bus.cockpit", Side::Remote),
        ("read_cc_bus_inbox", "cc-bus.cockpit", Side::Remote),
        ("cc_bus_send", "cc-bus.cockpit", Side::Remote),
        ("cc_bus_spawn", "cc-bus.cockpit", Side::Remote),
    ];

    /// **不对称能力的理由**。键集合必须**恰好等于**从 `LEDGER` 算出来的不对称集合。
    const ASYMMETRY_REASONS: &[(&str, Asym, &str)] = &[
        ("accounts.session-accounts", Asym::ParityDebt, "同 accounts.list：按会话查账号只有远端有。归 L3。"),
        ("accounts.trust", Asym::ParityDebt, "同 accounts.list：预信任检查只有远端有。归 L3。"),
        ("acct-iso.check", Asym::ParityDebt, "本机同样需要「这台装没装 cc-acct-iso」的检测（切号要靠它），今天只能查远端。归 L3。"),
        ("acct-iso.deploy", Asym::NaturallyAsymmetric, "vendored 副本要**传到**远端才能用；本地就在本机、不存在传输这一步。这条不对称是传输本身造成的，不是能力缺失。"),
        ("acct-iso.shellinit", Asym::ParityDebt, "本机切号同样要 shellinit 文本，今天只能给远端生成。归 L3。"),
        ("audit.config-surface", Asym::ParityDebt, "**反向缺口**（本地能答、远端答不出）——§40 表里已逐行记明：本页明写不连 SSH，10 行里 7 行对远端恒返回「未确定」。"),
        ("cc-bus.cockpit", Asym::ParityDebt, "cc_bus.rs 的 5 个 IPC **全走 origin+ssh、零本机读取路径**（`config_surface.rs` 的钉死表已把 `~/.cc-bus/` 记为 Remote）。而本机 cc-bus 是存在的——`diagnose_local_cc_bus_hooks` 就在诊断它 ⇒ 驾驶舱管不了本机的 agent，是真欠账。"),
        ("ccm.install-ui", Asym::Undecided, "本机安装向导有「扫 PATH 选装到哪」+「预览要写的文本」两步；远端 `install_remote_ccm_helper(cfg, profile)` 一步到位、没有这两步。**是欠账还是刻意简化，需要产品判断**——本表不替它裁定。"),
        ("daemon.deploy", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 3 条：本地会话由 `watcher.rs` 直接读 jsonl，**根本不需要 daemon**。"),
        ("history.branch", Asym::ParityDebt, "远端历史会话不能分支。本地能分支（`create_branch_session`），远端没有对应命令。"),
        ("mcp.list-origins", Asym::NaturallyAsymmetric, "「有哪些 origin」这个概念在本地不存在——本地只有一台。"),
        ("panorama.code-graph", Asym::Undecided, "**本表交出的最大一处新发现**：21 条命令全部只吃本机 `repo` 路径。远端 repo 的代码图谱既没做、也没在任何计划里登记过。**不擅自判它是天然不对称**——那需要产品判断（远端开发是不是本工具的场景）。登记待裁定。"),
        ("port-forward", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 2 条：本地没有「转发到自己」这个需求。"),
        ("search.index", Asym::NaturallyAsymmetric, "远端**不建索引**：`search_history` 对远端是实时 SSH fan-out（其头注自陈「本地内存索引查询与远端 fan-out 并发」）。索引是本机侧的实现细节，不是一项对外能力。"),
        ("session.tasks", Asym::ParityDebt, "**实测**：`get_session_tasks` 走 `tasks_root_for_current_claude_dir()` → `paths::resolve_claude_dir()`，读的是**本机**目录。远端会话的任务在远端机器上 ⇒ 远端 tab 拿不到任务列表。"),
        ("sftp.file-panel", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 1 条：本地有操作系统的文件管理器，不需要它。"),
        ("ssh.host-config", Asym::NaturallyAsymmetric, "本地按 §40 的定义就是「**不走 ssh** 的远端」⇒ ssh 目标的枚举/解析/导入/连通性测试/公钥推送在本地没有对应物。"),
        ("subagent.load", Asym::ParityDebt, "**实测**：`load_subagent` 拿 `parent_jsonl_path` 建 `PathBuf` 并 `is_dir()`，读的是**本机**文件系统 ⇒ 远端会话的 subagent 展不开。"),
        ("tmux.manage", Asym::ParityDebt, "§40 表里已列：`ccm` 全套修饰本地「无」，并注明「**POSIX 本地（§40 主体）落地后自动就有**」。归 L1/L2。"),
        ("usage.per-account", Asym::ParityDebt, "per-account 用量窗口只有远端有——同 accounts.* 一族。归 L3。"),
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
        strip_line_comments(&src[open + 1..end])
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
    fn strip_line_comments(s: &str) -> String {
        s.lines()
            .map(|l| {
                if l.trim_start().starts_with("//") {
                    ""
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 采集每条命令的**参数列表**（剥注释后），供结构反证用。跳过本文件自身。
    fn command_signatures() -> BTreeMap<String, String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let attr = format!("#[tauri::{}]", "command");
        let mut out = BTreeMap::new();
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // 跳过本护栏自身：它的说明文字里必然含这些子串。
            if path.file_name().and_then(|n| n.to_str()) == Some("parity_ledger.rs") {
                continue;
            }
            let src = strip_line_comments(&std::fs::read_to_string(&path).expect("read rs"));
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

    /// ★ 断言 3（有牙的那条）：声明 `Local` / `Both` 的命令，签名里**不许**出现远端专用参数。
    ///
    /// 防的是「为了让表好看，把一条远端命令声明成两侧都有」。反过来那条
    ///（`Remote` ⇒ 必须吃远端参数）**刻意没做**，理由见模块头注。
    #[test]
    fn local_or_both_commands_take_no_remote_only_parameter() {
        let sigs = command_signatures();
        // 判据运行时拼：直接写字面量的话，本文件自己的说明文字会被扫到。
        let needles = [format!("{}:", "origin"), format!("Remote{}", "Config")];
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
                assert!(
                    !flat.contains(n.as_str()),
                    "{cmd} 在对账表里声明为 {side:?}，签名里却有远端专用参数 `{n}`：{flat}\n\
                     一条只能对远端起作用的命令，不该被记成「本地也有」——那会造出假的平价。"
                );
            }
        }
        // 反向自检：一条都没检到 = 签名采集坏了。**等号而不是 `>=`**（T04 审计重要 5：
        // 写 `>= N` 恰好容忍一次静默降级）。
        assert_eq!(
            checked, 67,
            "检到 {checked} 条 Local/Both 命令（真实应为 67 = Local 46 + Both 21）\
             ——改 LEDGER 就要来确认这个数"
        );
    }

    /// ★ 断言 4：表的形状钉死。改 `LEDGER` 就要来改这几个数。
    #[test]
    fn ledger_shape_is_pinned() {
        assert_eq!(LEDGER.len(), 121, "命令总数变了");
        let sides = capability_sides();
        assert_eq!(sides.len(), 50, "能力总数变了");
        let asym = asymmetric_capabilities();
        assert_eq!(asym.len(), 20, "不对称能力数变了");
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
        assert_eq!(kinds.get("natural"), Some(&7), "天然不对称条数变了");
        assert_eq!(kinds.get("debt"), Some(&11), "平价欠账条数变了");
        assert_eq!(kinds.get("undecided"), Some(&2), "未裁定条数变了");
    }
}
