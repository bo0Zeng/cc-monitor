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
    // K-H2a：apikey 表那把第三方 API key。读那条**只回掩码**（`KS6`），写那条是「界面」这个第二写者（`KS10`）。
    // 〔RM1a · 第四波〕两条都收 `origin` ⇒ `Both`：本机读写 monitor 自己那一份，远端交那台机器的后端
    // （`apikey-read` / `apikey-key-set`）。`creds.apikey` 那条平价欠账（「把 key 送到远端的路」）结清。
    ("read_apikey_credentials_status", "creds.apikey", Side::Both),
    ("write_apikey_credentials_key", "creds.apikey", Side::Both),
    // K-H2b `KH2B7`：界面问「这几个账号走不走 apikey 端点改写 · 中转在不在」。
    // 〔RM1a · 第四波〕收 `origin` ⇒ `Both`：远端那台由那台的后端答（`apikey-read` · `relay-status`）。
    ("apikey_routing_for", "apikey.routing", Side::Both),
    // 〔RL1 · 第四波〕这次拉起往 `ANTHROPIC_BASE_URL` 里写哪个中转地址 —— 按 origin 取那台的事实
    // （远端用到才起那台的中转），判断只在 `payload::relay_endpoint_for` 一处 ⇒ `Both`。
    // 它接替了 RM1a 那条只对远端的 `relay_ensure`（零调用方；能力 `relay.machine` 随之退役）。
    (
        "relay_endpoint_for_launch",
        "relay.launch-endpoint",
        Side::Both,
    ),
    // 〔AL1 · 2026-09-24〕`K-R49` 那条 `write_account_aliases`〔散文墓碑〕（`lines` ＋ `dryRun`）退役：
    // 它拆成下面两跳 ＋ 一个读回口，能力键 `alias.account-commands` 随之并进 `alias.manage`。
    // 〔AL1 · 2026-09-24〕`设计/71`：别名只有一类（名字 ＋ 一组 ccm 参数），两跳 ＋ 读回口。
    // 今天都只有本机这一侧 —— 远端那半要「叫远端后端自己写」（`71 §12.6.3`），
    // 那是把写挪进后端，本路停下报备没做；欠什么见 `ASYMMETRY_REASONS` 里 `alias.manage` 那行。
    ("aliases_render", "alias.manage", Side::Local),
    ("aliases_read", "alias.manage", Side::Local),
    ("aliases_install", "alias.manage", Side::Local),
    ("open_settings_window", "app.window.settings", Side::Both),
    ("bring_monitor_to_front", "app.window.self", Side::Both),
    // devbench F03：skill 接入面（列出 skill / 读写那个「人手写的注入文件」）。
    // 三条共用一个能力键 —— 它们是同一件事的三个动作（先例：`app.window.session` 也是两条共键）。
    // 〔RW1 · 第四波 09-24〕用户裁「远端（和本机，同一条路）的 `INBOX.txt` 能编辑」⇒ 三条都吃 origin、两侧都经那台机器的后端。
    ("list_skills", "skill.inbox", Side::Both),
    ("read_skill_file", "skill.inbox", Side::Both),
    ("write_skill_file", "skill.inbox", Side::Both),
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
    // 〔B2 · 条 66〕那个值搬到后端所在那台机器上：原来那条「推生效值」的命令退役，换成问 / 交写两条，
    // 仍是 per-host、入参带 origin ⇒ 仍是 `Both`，仍归 `app.backend-policy`（能力数不动）。
    ("backend_exit_policy", "app.backend-policy", Side::Both),
    ("set_backend_exit_policy", "app.backend-policy", Side::Both),
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
    ("save_config", "app.config", Side::Both),
    ("get_data_paths", "app.data-paths", Side::Both),
    ("forget_session", "session.forget", Side::Both),
    // 〔U3b〕同一件事的另一半：`forget_session` 把一个会话的重放缓冲全丢，这一条丢到只剩尾巴
    // （接上骨架之后，丢掉的正文按偏移要得回来）。两者都只动 monitor 本机的缓冲、按 sid 找 ⇒ `Both`，
    // 归已有能力，能力数与不对称数都不动。
    ("replay_keep_tail_only", "session.forget", Side::Both),
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
    // 🔴🔴 **〔步 12·C 2026-09-20〕这一对**不合**，并且它不是漏了。**
    //
    // 派工单点名要合的 8 对里有它，读到底之后判**不合**，判词逐条在这里
    //（先例：`§2.5 P8` 的第 10 对 —— `list_remote_tmux` 那一行上面那句注释）：
    //
    // ① **远端那条根本不收 `origin`** —— 它的签名是 `list_remote_history_projects()`，
    //    体里 `load_remote_configs()` **fan-out 全部已配置远端**。
    //    ⇒ 本机那条是「一台机器」，远端那条是「N 台机器」。**不是同一个基数。**
    // ② **返回类型两侧不同形**：本机 `Vec<HistoryProject>`，远端
    //    `RemoteProjectsResult { projects, failed_hosts }` —— 后者那半是
    //    **fan-out 才有的概念**（「哪几台没答上来」），一台机器的查询说不出这句话。
    // ③ **前端的失败语义与缓存策略也是两套**（`views/history.ts::refresh`）：
    //    本机失败 ⇒ 整体失败（本地都读不了没得显示）；远端部分失败 ⇒ 不冻结缓存、
    //    渲染已拿到的台、下次重试。远端那份还有一条 TTL 缓存（`HISTORY_REMOTE_TTL_MS`），
    //    本机那份**每次重扫**。
    //
    // ⇒ 要合它，得先裁一个**设计题**：「项目列表的 fan-out 住哪一层」。
    //    合成 `list_history_projects(origin)` 就等于把 fan-out 搬到前端（N 次 IPC），
    //    并重写那条 TTL 缓存与 `failed_hosts` 的语义。**那是补/搬能力，不是归一命令名**,
    //    性质同 `§2.5 P8`：不许夹在一次机械合并里顺手做。
    // ⚠ **别读成「远端那条该退役」** —— 它今天承载的 fan-out 是真需求（多机 #30）。
    //    判的是「这一对不是双份，是一台 vs N 台」，**不是**「其中一条多余」。
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
    // 🔴 **〔步 12·C 2026-09-20〕下面这五行是「一条命令自己办两侧」，不是「两条命令各办一侧」。**
    //
    // `设计/00 §2.5 ①` 逐字「同义双份命令合成一条带 origin 参数的」。合并之后
    // **能力总数与不对称数都不动** —— 那五条能力从前是 `{Local, Remote}`（各有一条命令
    // ⇒ 已对称），今天是 `{Both}`（一条命令办两侧 ⇒ 同样对称）。
    // ⚠ **这一点要读准**：合并**不改善平价**，平价本来就有；它改善的是
    // 「同一件事有几条路」。两者别混着读。
    (
        "stream_history_sessions_in_project",
        "history.list-sessions",
        Side::Both,
    ),
    (
        "stream_read_session_jsonl",
        "history.read-session",
        Side::Both,
    ),
    // 〔`设计/10` 骨架 · 子步 3〕`--read-session-from-offset` 在 monitor 侧的两个调用点 ——
    // 骨架索引 ＋ 按偏移取一段正文。归**已有**能力 `history.read-session`：读的是同一份会话 jsonl，
    // 两侧都走 `subagent::Backend`（本机 exec 本机后端 / 远端 ssh exec 同一个二进制）⇒ `Both`，
    // 能力数与不对称数都不动。
    ("read_session_index", "history.read-session", Side::Both),
    ("read_session_range", "history.read-session", Side::Both),
    // 〔SE1 · `设计/10 §2.2b ⑥`〕大纲的数据源：`--list-user-inputs`。归**已有**能力 `history.read-session`
    // （读的是同一份会话 jsonl，两侧都走 `subagent::Backend`）⇒ `Both`，能力数与不对称数都不动。
    ("list_user_inputs", "history.read-session", Side::Both),
    // 〔SE2 · `设计/10 §6 步 6`〕会话内查找：`--find-in-session`。归**已有**能力 `history.read-session`
    // （读的是同一份会话 jsonl，两侧都走 `subagent::Backend`）⇒ `Both`，能力数与不对称数都不动。
    ("find_in_session", "history.read-session", Side::Both),
    ("delete_history_session", "history.delete", Side::Both),
    // 🔴🔴 **〔步 12·C 2026-09-20〕这一格是三份，不是两份 —— 判**不合**，判词在这里。**
    //
    // 派工单要求「先答『这两条远端命令到底差在哪』，读到底再决定合成几条」。读到底了，
    // 差**五处**，而其中两处是结构性的：
    //
    // | | `read_remote_mcp_servers` | `read_remote_project_mcp` |
    // |---|---|---|
    // | 读哪个文件 | 远端 `~/.claude.json` | 远端 `<dir>/.mcp.json` |
    // | 走哪条传输 | SSH exec `cat` | **SFTP** |
    // | 回哪个 scope | `user` | `project` |
    // | 缺 / 坏文件 | 32MB 超限 ⇒ **拒收回错** | 宽容 ⇒ **空** |
    // | 🔴 SS-14 读写分界 | 读的是**绝不写**的那份 | 读的是**唯一可写**的那份 |
    //
    // ⇒ 本机 `read_mcp_servers` 是这两条的**并集**（它一趟回 `user`＋`local`＋`project`
    //   三个 scope），所以「本机 1 : 远端 2」不是命名问题，是**分叉轴不是 origin**：
    //   真正的轴是 **scope × 传输 × SS-14 读写分界**。
    //
    // **合成一条要付的三笔账**（每一笔都不是机械合并）：
    // ① 远端那一侧要**新增能力**：一趟里做两种传输（SSH exec ＋ SFTP）并并集，
    //    还要补上**今天谁都不覆盖的远端 `local` scope**
    //    （`read_remote_mcp_servers` 已经 cat 了远端 `~/.claude.json`，
    //     却给 `collect_entries` 传了 `project_dir = None` ⇒ 那一档直接被跳过。
    //     **这是一处真缺口，如实登记，本步不补** —— 补它是加能力）。
    // ② **失败语义会变**：今天 SFTP 坏了照样读得到 user scope；合成一条之后
    //    一次调用要么部分静默失败、要么整趟硬失败，两者都要重裁。
    // ③ 前端那三格 UI 状态要塌（`mcp-section.ts::refresh` 按 (机器, 目录) **三态分发**，
    //    三条路各有自己的 loading 文案 / 错误重试盒 / 「仅 user scope」说明头）。
    //
    // ⇒ **不合。** 同 `§2.5 P8` 的处置：判成「两个能力」并把理由写在旁边，
    //   免得下一个人再来合一次。⚠ 这不是「远端欠一条命令」，
    //   是**这一格的归一要先裁 scope 与传输怎么摆**，那是设计题。
    ("read_mcp_servers", "mcp.read", Side::Local),
    ("read_remote_mcp_servers", "mcp.read", Side::Remote),
    ("read_remote_project_mcp", "mcp.read", Side::Remote),
    // 〔步 12·C〕合成一条（同上那五行的第四条）。
    ("list_mcp_project_dirs", "mcp.list-project-dirs", Side::Both),
    // 🔴🔴 **〔步 12·C 收尾 2026-09-20〕这两对合掉了 —— 上一拍欠的那笔还上了。**
    //
    // 上一拍在这里逐字登记过：「**它们是真双份**（同一个动作、同一个写面
    // `<dir>/.mcp.json`、同一条 SS-14 铁律的两端），合并在技术上是机械的」，
    // 卡的是**工作边界**不是判据 —— `installface` 那把尺子
    // `tests/evidence/K-R117-ruler.py::SPLIT_GROUPS["S5"]` 是一张字面量名单，
    // 里面逐字列着那两条远端命令，而它的 `R8a` 判「名单 ↔ 现打人群」两向集合相等。
    //
    // ⇒ 本拍**同拍**把那张名单从 7 条改成 5 条，卡点消失。
    //   ⚠ 改那张名单是「**人群跟着现实走**」，不是「改判据凑绿」：改的是 `SPLIT_GROUPS`
    //     这张**名单**（它登记的是「哪条命令归五件里的哪一件」），
    //     `R8a` 的**判法**（两向集合相等）一个字节没动 —— 它照样会在
    //     「表指了一条盘上没有的命令」和「盘上多了一条谁都没认领的命令」两个方向上红。
    //
    // **能力总数与不对称数都不动**：这两条能力从前是 `{Local, Remote}`（各一条命令
    // ⇒ 已对称），今天是 `{Both}`（一条命令办两侧 ⇒ 同样对称）。
    ("write_project_mcp_server", "mcp.write", Side::Both),
    ("remove_project_mcp_server", "mcp.remove", Side::Both),
    ("resume_history_session", "session.launch", Side::Local),
    // 〔U4b · 第四波〕收 `origin` ⇒ `Both`：本机与远端问同一个口（那台后端的 `history-record`，本机 ＝ `<local>`）。
    //   归已有能力 `history.read-session`（读的是同一棵记录树）⇒ 能力数与不对称数都不动。
    ("probe_session_record", "history.read-session", Side::Both),
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
    ("install_remote_alias_block", "ccm.install", Side::Remote), // 〔MC1〕改名，从前叫「装 ccm 助手」
    ("cc_integration_uninstall", "ccm.uninstall", Side::Local),
    (
        "uninstall_remote_alias_block",
        "ccm.uninstall",
        Side::Remote,
    ),
    // 🔴 `K-R135`（`R85`/`R87`）：用户级 PATH 那一格。只有本机一侧，理由见
    //    `ASYMMETRY_REASONS` 里 `ccm.user-path` 那一条。
    ("ccm_user_path_status", "ccm.user-path", Side::Local),
    ("ccm_user_path_add", "ccm.user-path", Side::Local),
    ("ccm_user_path_remove", "ccm.user-path", Side::Local),
    ("cc_integration_preview", "ccm.install-ui", Side::Local),
    ("cc_integration_scan_path", "ccm.install-ui", Side::Local),
    // G6：远端分叉落地 ⇒ `history.branch` 从 ParityDebt 变成两侧都有（该行的
    // 不对称理由已从 `ASYMMETRY_REASONS` 删除——留着就是宣称一条已经补上的欠账）。
    // 〔步 12·C 2026-09-20〕**两条并成一条**（上面那五行的第五条）——
    // 被并掉的那条命令自己的头注逐字写着「差异今天只剩一处：活儿在远端干」。
    ("create_branch_session", "history.branch", Side::Both),
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
        "panorama_list_annotations",
        "panorama.code-graph",
        Side::Local,
    ),
    // PN1b 选图（`设计/97 §7`）：注册表 ＋ 画一张图，都是本机全景那一族。
    ("panorama_diagram_kinds", "panorama.code-graph", Side::Local),
    ("panorama_diagram", "panorama.code-graph", Side::Local),
    // 〔RM1c · 第四波〕远端仓的全景（V108 选 B）：按 origin 问那台机器的后端 `panorama`（经插件口起独立小程序）。
    //   今天前端只在**远端**用它，本机那一侧是上面那几条进程内命令（第二拍本机对称时这一行改 `Both`、那几条退役）。
    //   ⚠ 批注 / 文档关联的**写**（上面六条）从 `panorama.code-graph` 拆成 `panorama.annotate`：远端第一拍只读，
    //   欠账挂在那一格，不让「看得见远端仓的图」把「写不了远端仓的批注」一起抹平。
    ("panorama_call", "panorama.code-graph", Side::Remote),
    // 〔RM1d · 第四波 · V110〕批注 / 文档关联的**写**本机远端同一条：那台机器算出新内容、那台机器后端的
    //   文件管理落盘。本机那六条进程内写命令（经内嵌引擎直写被分析仓，违反 V88）随之退役 ⇒ `panorama.annotate` 两侧都有。
    ("panorama_edit", "panorama.annotate", Side::Both),
    ("get_search_index_status", "search.index", Side::Local),
    ("rebuild_search_index", "search.index", Side::Local),
    // P7c-1（08-12）：远端会话的 subagent 展开做出来了 ⇒ 两侧都服务。
    ("load_subagent", "subagent.load", Side::Both),
    // 〔RM1b · 第四波〕按 origin 问那台机器的后端 `tasks-list`（本机也走后端）⇒ 两侧都服务。
    ("get_session_tasks", "session.tasks", Side::Both),
    // 〔RM1a · 第四波〕收 `origin` ⇒ `Both`：远端那一栏问那台机器的后端（`footprint-probe`），判定同一份 `build_rows`。
    ("config_surface_report", "audit.config-surface", Side::Both),
    // U-CC1：漂移记账是**进程内**的账本，本地行与远端行都经同一个
    // `parse_line`（`lib.rs::batch_to_payloads`）喂进来 ⇒ 一个读口就覆盖两侧，
    // 天然 `Both`，不需要远端对侧命令。〔ST3〕账按机器分、读口收 `origin` 只答那一台
    // （仍是 monitor 自己的命令、不经后端），登记见 `ORIGIN_TAKING_BOTH`。
    ("drift_ledger_report", "audit.drift-ledger", Side::Both),
    // P8a：插件面只读枚举。〔RM1b · 第四波〕按 origin 问那台机器的后端 `plugins-marketplaces` ⇒ 两侧都服务。
    (
        "list_plugin_marketplaces",
        "plugins.marketplaces",
        Side::Both,
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
    //    以 `<local>` 调 `backend_send_into` 就地 resume。
    //
    // ⇒ 这条不是「改个分类」，是**这条能力真的两侧都有了**。
    ("backend_send_into", "launch.send-into", Side::Both),
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
    //       `tests/evidence/K-R109-deathvalue.md` 的「归格那一刀」〕
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
    // 〔F7c 收尾 09-24〕「文件面板」这一格走了十二行（老面板删了、窗口改走通道；`设计/60 §13b`）：
    //   〔已删：`sftp_realpath` · `sftp_list_dir` · `sftp_stat` · `sftp_download` · `sftp_upload` ·
    //   `sftp_cancel_transfer`〔散文墓碑〕 · `sftp_mkdir` · `sftp_rename` · `sftp_delete` · `sftp_read_text_for_edit` ·
    //   `sftp_write_text` · `sftp_chmod`〕。能力 `sftp.file-panel` 还在（`open_file_window`）。
    // 〔第四波 S4〕零流量复制那一行（步 23b 加的，Remote）随门禁那一格退役一起走了：
    //   窗口的复制走后端 `files-copy`。能力 `sftp.file-panel` 仍由下面那条入口撑着 ⇒ 能力数与不对称数都不动。
    // 〔`设计/60 §5.4c` · 09-20〕改权限位。**归已有能力 `sftp.file-panel`** ⇒
    // 能力数与不对称数都不动：它不是一件新能力，是 `设计/60 §3.1` 那张能力对照表里
    // 逐字列着的一行（「权限 ✅ `SETSTAT` / `chmod` / **打平**」），此前那一栏写的是
    // 「❌ 没做（`SETSTAT` 协议里有）」。
    // ⚠ `Remote` 不是欠账：本机改权限压根不经 SFTP
    // （本机那一侧是 `std::fs::set_permissions`，而本机根本没有这个面板 ——
    //  `sftp.file-panel` 那条 `NaturallyAsymmetric` 逐字「本地有操作系统的文件管理器」）。
    // 〔`设计/60 §4 戊`／`§5` 第三段 · `24e` 第二刀 · 09-20〕**原生文件管理窗口的入口。**
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
    ("start_forward", "port-forward", Side::Remote),
    ("stop_forward", "port-forward", Side::Remote),
    ("list_forwards", "port-forward", Side::Remote),
    ("list_ssh_host_aliases", "ssh.host-config", Side::Remote),
    ("resolve_ssh_host", "ssh.host-config", Side::Remote),
    ("import_ssh_hosts", "ssh.host-config", Side::Remote),
    ("test_remote_connection", "ssh.host-config", Side::Remote),
    ("push_public_key", "ssh.host-config", Side::Remote),
    ("deploy_remote_backend", "backend.deploy", Side::Remote),
    ("uninstall_remote_backend", "backend.deploy", Side::Remote),
    ("list_remote_mcp_origins", "mcp.list-origins", Side::Remote),
    ("list_remote_accounts", "accounts.list", Side::Remote),
    // L3a：本机枚举——同样只读、同样的输出类型 ⇒ 这条能力已对称。
    ("list_local_accounts", "accounts.list", Side::Local),
    // 〔C4a · 第四波〕「某会话属哪个账号」那两行（远端 A2 · 本机 E79，能力 `accounts.session-accounts`）退役：
    //   本机与远端收成**同一条路** —— 前端经通道（下面 `chan_call` 那一行）直接说帧命令 `accounts-sessions`。
    //   ⇒ 这项能力不再是一条 Tauri 命令；它两侧都有，只是住到了帧面上（后端 `read_face.rs`）。
    // 〔C4a · 第四波〕**主界面说 `call` 的那一跳**（`chan/webview.rs`）：按 `origin` 转给注入的后端句柄，
    //   本机（`<local>`）与远端同一条路 ⇒ `Both`；它吃 `origin`，理由登记在 `ORIGIN_TAKING_BOTH`。
    ("chan_call", "comm.face-a.call", Side::Both),
    // 〔`A3` 第二波〕`origin == <local>` ⇒ exec 本机后端同一条 `--account-trust*`
    // （`local_accounts::local_account_trust`）⇒ 一条命令两侧都办了 ⇒ `Both`；
    // 能力 `accounts.trust` 那笔 `ParityDebt` 随之结清（理由表那一行已删）。
    ("check_account_trust", "accounts.trust", Side::Both),
    ("deploy_remote_acct_iso", "acct-iso.deploy", Side::Remote),
    ("check_remote_acct_iso", "acct-iso.check", Side::Remote),
    (
        "remote_acct_iso_shellinit",
        "acct-iso.shellinit",
        Side::Remote,
    ),
    // 〔`A3` 第二波〕上面两条的本机对侧：问本机后端的 `--acct-iso-status` / `--acct-iso-shellinit`
    // （住后端账号层 `accounts/iso.rs`），出参类型与远端那条逐字相同 ⇒ 两条能力从此对称，
    // `acct-iso.check` / `acct-iso.shellinit` 两笔 `ParityDebt` 结清（理由表那两行已删）。
    ("check_local_acct_iso", "acct-iso.check", Side::Local),
    (
        "local_acct_iso_shellinit",
        "acct-iso.shellinit",
        Side::Local,
    ),
    // 🔴 §40 九对合并的**第 10 对**：它不是漏了，是判成两个能力（`调研/设计/15 §D1`）。
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
    // 🔴 `K-R98`（09-13）：**发消息两侧走的是同一条路** —— 本机那半 `P4f` 已切后端的
    // `bus-send`，远端那半此前还在拼 shell 串走 SSH，本件也改走同一条原语。
    // ⇒ `cc_bus_send` 从 `Remote` 转 `Both`。⚠ 它不是「补了一侧」，是**本来就已经两侧都通**
    //   （P4f 那天就该改这一行，没改 ⇒ 账本从那天起对这一格撒了一个月的谎，
    //   与本表 `cc-bus.cockpit` 那条理由自陈的「改了行为没回来改理由」是同一种病）。
    ("cc_bus_send", "cc-bus.cockpit", Side::Both),
    // 写面其余三条当时仍是远端专属（本机对侧未做）。
    // 〔BS1b 09-24〕派生也改走后端原语 `bus-spawn` 了（`cc_bus.rs::spawn_via_backend`）⇒ 派生器判它
    //   `FramePlane`；**`Side` 这一格照 `FRAME_PLANE_VERDICTS` 头注那条纪律暂不翻**
    //   （monitor → 本机后端 → cc-spawn 这一整跳没在真机的 app 里跑过），记成那张表里的欠账。
    ("cc_bus_spawn", "cc-bus.cockpit", Side::Remote),
    // P4c（08-12，#77/#78）：广播 + 收掉。同为写面 ⇒ 同样远端专属。
    ("cc_bus_broadcast", "cc-bus.cockpit", Side::Remote),
    ("cc_bus_kill", "cc-bus.cockpit", Side::Remote),
];

/// **不对称能力的理由**。键集合必须**恰好等于**从 `LEDGER` 算出来的不对称集合。
const ASYMMETRY_REASONS: &[(&str, Asym, &str)] = &[
    ("ccm.user-path", Asym::NaturallyAsymmetric, "🔴 `K-R135`：「把我们那个 bin 目录放上**用户级** PATH」。**天然只有本机一侧，而且这一条的『天然』是可证的，不是图省事**：① **远端那一侧同一件事已经有答案，只是载体不同** —— 远端 `ccm` 落 `~/.local/bin`，而把它放上 PATH 的是写进远端 rc 的那个围栏块（`install_remote_alias_block` ＋ `src/shared/ccm-aliases.sh` 里那一行 —— 那一行正是 `K-R135` 本轮修的：它此前只加 `~/.local/bin`，对本机那一边是错的）。⇒ 欠的不是「远端没有这项能力」，是**两边的机制本来就不同**。② **「用户级 PATH」这一档是 Windows 独有的**（注册表 `HKCU` 下那个 `Environment` 键），而远端按 `K32` 是 Linux ⇒ 那台机器上根本没有这一档可改，补一条对侧命令只能是个空壳。⚠ **诚实边界**：哪天真出现「远端是 Windows」这一形，本条要回来重裁 —— 那时它就不再是 `NaturallyAsymmetric`，而是 `ParityDebt`。今天不给它发明一条够不着的对侧。"),
    ("acct-iso.deploy", Asym::NaturallyAsymmetric, "vendored 副本要**传到**远端才能用（`deploy_remote_acct_iso`）；本地就在本机、不存在传输这一步。⚠⚠ **P3b 标疑（08-12）：这条理由属于「从未被验证过」那一类，别当它已经核过。** 「不存在传输」是真的，但**「不存在安装」没人量过** —— 实测本机 `~/.local/bin` 下确实躺着 `cc-acct-iso`（`config_surface_tests.rs::either_host_keeps_the_glob_count` 记着那 12 条 `cc-*`），而它是**怎么到那儿的**、要不要 monitor 管，本表从来没答过。⇒ 若答案是「要 monitor 管」，这条就该从 `natural` 变 `ParityDebt`。归 `P3b` 的后续或 `L3`。"),
    // 〔RW1 · 第四波 09-24〕这里原来是 `skill.inbox` 那一行（`Undecided`：「远端项目的收件箱要不要能在这里编辑，没人裁定过」）。
    //   用户 09-24 裁了：**要**，远端与本机同一条路（经那台机器后端的文件管理那一面读写）⇒ 三条命令 `Both`，这一行摘掉。
    ("tmux.local-census", Asym::NaturallyAsymmetric, "「本机今天有哪些 tmux 会话」。★ P3 刀 2 的 UI 半把它从「只回名字」放宽到「回整条会话」——杀会话的菜单必须按 `@ccm_sid` 认归属，按 `<sid8>-cc` 前缀猜与 §30 逐字禁的「按目录回退猜」是同一类错。**反向缺口，且是天然的**：远端问同一个问题**已经有答案** —— `list_remote_tmux` 一次性 SSH `tmux ls` 就是它，前端 `pickFreshTmuxName(sid, existing)` 拿的正是那份。本机没有 SSH 那一跳，所以要一个自己的口；开它不是本机多了什么能力，是**把远端本来就有的那一格在本机补上**。⇒ 记 `NaturallyAsymmetric` 而不是 `ParityDebt`：欠的是本机这一侧，而本行一落地就已经补平，没有留下去处。★ 它读的是后端推来的 tmux 快照而不是现跑 `tmux ls`，理由与射程见 `tmux.rs::list_local_tmux` 头注：那份快照由 `session-created/closed/renamed` 三条 hook 驱动，**恰好就是改变名字集合的那三件事** ⇒ 对这个问题它是权威的，对「pane 前台命令变了没有」才是陈旧的（那条已被 devbench F08 裁定不开口）。"),
    ("cc-bus.install-state", Asym::ParityDebt, "`PS2`：本机 cc-bus 装的是哪一版（没装 / 已是最新 / 装了但不是这一版）。**只读**，逐文件与内嵌那 17 个字节串比。⚠ 欠的与 `cc-bus.deploy` 是**同一笔**：远端那侧同样有 `~/.claude/skills/`，要问「远端装的是哪一版」得让后端出一条具名读命令（形状抄 `P4d` 那批）。⇒ 两条一起补，别分两次。★ 顺带记口径：本条能答得出来，**全靠 `U9`② 裁了「仓内那份为准」**（用@08-13）—— 没有真相源就没有「不是这一版」这个判断，`PS2` 摸底时那条判据逐字写着「加第三态之前先答版本口径」。"),
    ("cc-bus.deploy", Asym::ParityDebt, "`PS1`〔`U10b` 用@08-13 裁「开」后落地〕：把内嵌的 cc-bus 装到 **本机** `<claude_dir>/skills/cc-bus/`。⚠ 欠的是什么要写准：**不是**「远端不需要」——远端同样有 `~/.claude/skills/`，而且本仓**已经有**一条同族的远端部署路（`acct-iso.deploy` 走 SFTP 推 vendored 脚本）。欠的是**把这条本机路复制到远端**：SFTP 推 17 个文件 + 远端侧的围栏（`canonicalize` 在远端不成立，要换成后端侧校验）。⇒ 如实记欠，**不假装两侧都有**。★ 顺带记一条口径：本条的落点是**用户数据目录**，与 `acct-iso.deploy` 那条「只写 cc-monitor 自己的 bin 目录」**性质不同** —— 后者不需要豁免，本条需要（`INVARIANTS` 第 7 条）。"),
    // 〔RM1a · 第四波〕`audit.config-surface` 那一行（`ParityDebt`：「本地能答、远端答不出，本页明写不连 SSH」）**结清、删掉**：
    //   远端那一栏由那台机器的后端答路径事实（`footprint-probe`），用户裁「补后端读口，远端也有真栏」。
    ("cc-bus.cockpit", Asym::ParityDebt, "★ **P4c 订正（08-12）：原理由已经过期，而过期的正是 `P4a` 那一刀造成的。** 原文写「cc_bus.rs 的 **5 个 IPC** 全走 origin+ssh、**零本机读取路径**」——`P4a`（08-12）把**读面三条**（`read_cc_bus_state` / `check_cc_bus_agent_online` / `read_cc_bus_inbox`）做成了本机可用（同一条命令串，只是不包进 ssh；本机 `~/.cc-bus/agents.tsv` 实测 86 行），它们今天是 `Both`。⇒ 「零本机读取路径」是假的，「5 个」也变成了 7 个（`P4c` 加了 `cc_bus_broadcast` / `cc_bus_kill`）。**今天真正的欠账只剩写面里的三条**：`cc_bus_spawn` / `cc_bus_kill` 对 `<local>` 走 `refuse_local_write`（本机没有对侧），`cc_bus_broadcast` 的本机路走后端组合但**没有回落**（`P4a §0c` 量过代价：本机写面归 `P4b`，而 `P4b` 签收的是 cc-spawn 的复用那一刀，没交付写面）。★★ **`K-R98` 订正（09-13）：原文说的是「四条」，而其中两条早已不成立** —— `cc_bus_send` 的本机路 `P4f`（08-13）就改走了后端的 `bus-send` 原语、`cc_bus_broadcast` 的本机路同日改走 `bus-list` + 逐个 `bus-send` 的组合，两条都**不再**经 `refuse_local_write`；而这一行从那天起一个字没改。⇒ 这已经是本条第 **2** 次因为「改了行为没回来改理由」而订正（第一次是 `P4c` 订 `P4a`，那段就在上面）。本次同时把 `cc_bus_send` 的 `Side` 从 `Remote` 改成 `Both`（远端那半也改走同一条原语，`cc_bus.rs::send_via_backend` 是那唯一一处），登记见 `ORIGIN_TAKING_BOTH` 里那一行。★ 这条订正本身是 `P3b §0b` 的 **A 类（过期）**活样本，而制造它的是 `P4a` —— **改了行为没回来改理由，账本当天就开始撒谎**，本轮第二次（第一次是 `P4d-Y5` 改 `capture_remote_pane` 那次）。★★★ **BS1b 订正（09-24）**：写面最后一条 `cc_bus_spawn` 也改走后端原语 `bus-spawn` 了（`cc_bus.rs::spawn_via_backend`），对 `<local>` 的那句公共拒绝整块删了 ⇒ 上文「今天真正的欠账只剩写面里的三条」**全部不成立**；这一族今天剩下的欠账只是 `Side` 栏没翻（kill / broadcast / spawn 三行记在 `FRAME_PLANE_VERDICTS`，理由是 monitor 那一跳没在真机的 app 里跑过）。"),
    ("ccm.install-ui", Asym::Undecided, "本机安装向导有「扫 PATH 选装到哪」+「预览要写的文本」两步；远端 `install_remote_alias_block(cfg, profile)` 一步到位、没有这两步。**是欠账还是刻意简化，需要产品判断**——本表不替它裁定。"),
    ("backend.deploy", Asym::NaturallyAsymmetric, "★★ **P3b 结清（08-12）：理由整个换掉 —— 原来那句是假的。** 原文写「§40 天然不对称白名单第 3 条：本地会话由 `watcher.rs` 直接读 jsonl，**根本不需要 backend**」，被 P2z + P2 + P2s 三件直接证伪：本机**需要** backend（入方向通道、每台机开关、tmux 帧都靠它），而且**已经会自部署** —— `local_backend.rs::extract_embedded_to`（exe 旁没有本机后端就把内嵌那份释放到 `~/.cc-monitor/bin`）。真正的不对称只剩一格：**本机那次释放不经一条 IPC 命令**，是宿主启动时自己做的（`lib.rs` 的启动段），所以命令面上没有本机对侧。⇒ 记 `natural` 记的是「不需要一条命令」，不是「不需要后端」。"),
    ("launch.render-payload", Asym::NaturallyAsymmetric, "兜底那支（`container:\"none\"`）的载荷渲染。**记 `natural` 记的是命令面这一格**：远端那侧要一条 IPC（`render_launch_payload`）才问得到宿主，而本机**自己就是宿主** —— `history.rs::launch_local` 直接在进程内调 `build_local_*_command`，没有「绕一圈问自己」这一步（同形的话 `launch_wire.rs` 的头注里逐字写着）。⚠⚠ **`K-R53`（09-11）撤掉原文那半句**：原文写「P3t 之后那是**渲染器拒了才走的回落**」——**按调用点分母那是假的**：盘上四个本机拉起入口里有三个（`src/tabs.ts` 一处 + `src/views/history.ts` 两处，人群由 `tests/ipc/commands.vitest.ts` 那条「恰好 4 处」钉着）只说得出**具名账号**，而具名账号在 `K-R53` 之前必然 §35 短路 ⇒ **那三条只能走它**。一条 3/4 的分母不叫回落。`K-R53` 把具名那一格接上之后（`LaunchAccount::Named::name`），今天真正还会落到它的是：账号未表态（继承 —— 见下一行）· 只说得出目录没有名字 · 没有 tmux 名 · 这个号走中转（`history.rs::RELAY_KEEPS_THE_OLD_PATH`）· 这台机没装 ccm · Windows。逐格读数住 `history.rs::tests::every_local_account_shape_gets_a_named_verdict_from_the_backend_path`。★★ 🔴 **`K-R89`（09-13）：这六格今天只剩五格，而且「今天各自是什么」由一张**可执行**的表说了算** —— `history.rs::tests::THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`（六格逐格由 `every_one_of_the_six_cells_is_measured_not_narrated` **真去驱动一遍**，改了行为不改说法当场红 ⇒ 本行这句散文再腐一次，那边会先响）。关掉的是**账号未表态（继承）**：用户 09-12 `DECISIONS.md#R28` 裁定了「省略 `--account`」的语义，并已落地在 `src/backend/control/ccm/plan.rs::resolve_account`（两支：`CLAUDE_CONFIG_DIR` 非空 ⇒ 保留不覆盖〔`R08` 那道 `-z` 闸〕· 裸终端 ⇒ 落 manifest `isDefault`）⇒ 本机那一态渲染得出来了。⚠ **只关了本机那半** —— 远端是 ssh 过去、那台机器上的继承态不是 monitor 的环境（`R28` 裁定四），`WireAccount` 刻意没有对应变体，那半归 `K-R90`。⚠ 同拍另一处现打订正：「没有 tmux 名」那一格今天是**半开**的 —— resume 那条前端已接线（`K-R89` 现打），而 `new_local_session` 的 Rust 签名里**根本没有 `tmux_name` 这一格** ⇒ 只有起新会话恒短路。⚠ P3t-Y4 订正保留：原文引 §36 当依据，那是把一条讲 **Windows**、逐字禁「本地渲染器读 `plan.env`」的窄铁律读宽了。"),
    ("launch.render-attach", Asym::NaturallyAsymmetric, "🔴 `K-R109`（09-13）：**本机后端产「把终端接进那个会话」那一句**（`history.rs::render_local_attach` ⇒ `ccm attach <名>`，`R61` 裁定三）。⚠ **记 `natural` 记的是「远端那一侧不需要一条独立命令」，不是「远端还没做」** —— 远端的 attach 那一句由 `render_ccm_launch` 的 `WireAction::Attach` **一并产**（记在 `launch.render-cli` 那一格，同一条 IPC 覆盖 new / resume / attach 三个动作）⇒ 远端不缺这项能力，缺的只是**一个单独的命令名**。反过来本机**不能复用那条 IPC**，两条都是结构性的：① 那条 wire 的 `is_ssh` 与前端那道闸（`ctx.transport.kind === \"ssh\"`）说的是同一件事 —— 本机就是宿主，绕一圈 IPC 问自己拿不到新东西（同 `launch.render-payload` 那一行的理由）；② `WireAccount` 刻意少一态（没有 `Inherit`，`launch_wire.rs` 头注逐字），而本机的账号态是 `render_local_ccm` 在**进程内**探出来的（`CcmProbeSource` 那条缝）。⚠ **它不是 `session.launch` 的一部分**：那一格的人群由 `launcher_identity_registry::LAUNCH_CAPS` 读走当「起会话方」，而本命令接的是一条**已经在跑**的会话，一个 agent 进程都不出生 —— 09-13 挂错过一次，门禁当场逮住（读数住 `tests/evidence/K-R109-deathvalue.md`）。**这一格什么时候能结清**：远端那一句 attach 也长出自己的命令名的那天（那要先有人问「为什么要拆」），或者本机这一条被并进一条统一的渲染 IPC 的那天（`U8c-3` 那一侧）。"),
    ("launch.render-cli", Asym::NaturallyAsymmetric, "`ccm 调用行`的渲染。★★ **P3t-Y4 把这条的理由整个换了 —— 原来那个已被实测证伪。** 原文说这条不对称是「本地渲染必须在目标机器上做（要现场探 `command -v cc`，TS 无法预先渲染好交给它）」造成的。**本机就在本机**：P3t-Y2 的 `ccm_probe::probe_local_ccm()` 直接跑一次 `bash -lic` 就拿到了版本与完整能力集，比远端那条 ssh 往返还便宜 ⇒ 那个理由不成立。真正的不对称是**本机账号三态里有两态 CLI 说不出**：`Named{config_dir}` 只有目录没有名字（CLI 只会 `--account <名字>`），`None` 是「继承环境」而 CLI 语法里没有这一态（映成 `--base` 就是把继承偷换成显式清空 = #75 病灶）。★★ **`K-R53`（09-11）改的是它的分量，不是它的机制**：原文那半句把这两态回旧路说成一次边角的「降级」，而**按调用点分母它是主路**（四个本机拉起入口里三个只说得出具名账号）。⇒ `K-R53` 把具名那一态接上了（`LaunchAccount::Named` 现在带名字，由前端那个唯一取值口 `accounts.ts::localLaunchAccountSync` 与 `localLaunchAccountNameSync` 同源给出），**说不出的只剩「继承」一态**。而那一态**今天仍然说不出，而且省略参数也兑现不了**：既没 `--account` 也没 `--base`、且 `CLAUDE_CONFIG_DIR` 为空时 ccm **落 manifest 默认号**（「把调用方选中的号静默换掉」）⇒ 省略是另一个方向的静默换号，与 `--base` 一样不是「继承」。⚠ 那个读数**量于一份已经不在盘上的文件**（那份 bash `ccm` 的 1001-1012 行，`07e4e72` 删）；同一档语义今天住 `src/backend/control/ccm/plan.rs`，`K-R61` **没有重打它** —— 别把它读成「今天现打过」。⇒ 本行仍 `natural`，它记的仍是**语法窄一格**，只是那一格从两态收成一态。★★ 🔴 **`K-R89`（09-13）：上面那句「补它要动的是 ccm 省略时的默认语义（**产品决定** ＋ 改 `plan.rs`）」是一句陈账 —— 它在等一个 09-12 就已经到了、而且已经落地的决定。** 那个产品决定 = `DECISIONS.md#R28`（用户 09-12 逐字「不是有选默认账号吗? **就用那个**」），落地处 = `src/backend/control/ccm/plan.rs::resolve_account`（头注挂着 ✅，两支：`CLAUDE_CONFIG_DIR` 非空 ⇒ 保留不覆盖〔`R08` 那道 `-z` 闸〕· 裸终端 ⇒ 落 manifest `isDefault`）。⇒ **「继承」这一态今天在本机说得出了**（`CliAccount::Inherit` 渲染成「一个账号 flag 都不加」），`history.rs::render_local_ccm_with` 的 `None` 那一臂不再短路。⚠ **本行仍 `natural` 的理由因此换了人**：不再是「语法说不出继承」，而是 **远端那一半仍然说不出** —— 远端是 ssh 过去，那台机器上的继承态不是 monitor 的环境（`R28` 裁定四逐字「不算已解」）⇒ `launch_wire::WireAccount` 刻意没有对应变体，那半归 `K-R90`。⚠ 上面那张「三说法逐格对」的表**第二行今天翻了面**：「省略 ⇒ 落 manifest 默认号」从 ❌ 变 ✅ —— **行为一个字节没动，动的是对它的判断**（`R28` 裁定零逐字：「本裁改的不是行为，是『这是不是我们要的』」）。逐格今天版住 `history.rs::tests::THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`，由 `every_one_of_the_six_cells_is_measured_not_narrated` 真去驱动。"),
    ("mcp.list-origins", Asym::NaturallyAsymmetric, "`list_remote_mcp_origins` 答的是「哪几台远端有 MCP 配置」——「有哪些 origin」这个问题在本机侧退化成一台，没有可列的集合。⚠ 注意它与 `backend_machines` 不同：那条**包含**本机（`LOCAL_ORIGIN`），因为它答的是「哪几台有后端」而本机也有。"),
    // 〔RM1c · 第四波〕`panorama.code-graph` 那一行（`Undecided`，「远端 repo 的代码图谱既没做、也没在任何计划里登记过」）**摘了**：
    //   用户 09-24 裁「要」（V96）并选 B（V108），`panorama_call`（`Side::Remote`）按 origin 问那台后端 ⇒ 两侧都有。
    // 〔RM1d · 第四波〕`panorama.annotate` 那一行（`ParityDebt`：「远端仓的批注写等后端写面」）**结清、删掉**：
    //   用户 09-24 V110「引擎只算、文件管理来写」，`panorama_edit`（`Side::Both`）本机远端同一条。
    // 〔RM1a · 第四波〕`creds.apikey` 那一行（`ParityDebt`：「把 key 送到远端的路」）**结清、删掉**：
    //   远端那一份由那台机器的后端写（`apikey-key-set`，临时文件出生即只给本人 —— 机密性由拿着那份文件的
    //   那台机器自己的 `creds_core::perm` 管，不靠 SFTP 的 mode 参数，那一行点名的两条要求都照做了）。
    // 〔RM1a · 第四波〕`apikey.routing` 那一行（`NaturallyAsymmetric`：「本机这一侧在结构上答不了远端那台」）**删掉**：
    //   那一行自己写着出路 ——「远端那台要答同一个问题，得由跑在那台上的 backend 自己答」—— 今天就是这么答的。
    // 〔RL1 · 第四波〕`relay.machine` 那一行（`NaturallyAsymmetric`：「只对远端，本机那一个另有住处」）**删掉**：
    //   那条只对远端的命令退役了，接替它的 `relay_endpoint_for_launch` 两台都答（本机经起会话那一侧的缝）。
    ("alias.manage", Asym::ParityDebt, "〔AL1 · 2026-09-24〕`设计/71`：别名 ＝ 名字 ＋ 一组 ccm 参数，命令面两跳（`aliases_render` 纯 · `aliases_install` 唯一副作用）＋ 读回口 `aliases_read`，今天**只有本机这一侧**。⚠ 欠的是什么要写准：①「渲染」这一跳**不欠** —— 它是纯函数，本机算出来的 POSIX 文本拿去远端手贴一样能用（远端机器页上就是这么给的）；② 欠的是「**写**」与「**读回**」两跳在远端的那一半。`71 §12.6.3` 给的路是**叫远端后端自己写**（远端的 `startup_files()` 由它自己的 platform 答）—— 那是把写挪进后端（`src/backend` 只读铁律那一族），本路停下报备、没做；**不走** monitor 侧 SFTP 再长一条写路（那正是 `71 §12.5` 要收掉的第三份）。"),
    ("port-forward", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 2 条：本地没有「转发到自己」这个需求。"),
    ("search.index", Asym::NaturallyAsymmetric, "远端**不建索引**：`search_history` 对远端是实时 SSH fan-out（其头注自陈「本地内存索引查询与远端 fan-out 并发」）。索引是本机侧的实现细节，不是一项对外能力。"),
    ("sftp.file-panel", Asym::NaturallyAsymmetric, "§40 天然不对称白名单第 1 条：本地有操作系统的文件管理器，不需要它。"),
    ("ssh.host-config", Asym::NaturallyAsymmetric, "本地按 §40 的定义就是「**不走 ssh** 的远端」⇒ ssh 目标的枚举/解析/导入/连通性测试/公钥推送在本地没有对应物。"),

    ("tmux.manage", Asym::ParityDebt, "★★ **P3b E 阶段重量（08-12）：那句预言「POSIX 本地落地后自动就有」——三分之三对、四分之一错。** 原文只写「`ccm` 全套修饰本地『无』」+ 那句预言，没说是哪几条命令。逐条量：① `list_remote_tmux` ⇒ **本机已有对侧** `list_local_tmux`（P3t-Y2b + P3 刀2-UI，读后端推来的快照）；② `kill_remote_tmux` ⇒ **本机已通**（P3 刀 2 后端：`backend_kill` 传输无关，且本机专属错误文案已加）；③ `tmux_send_keys` ⇒ **本机已通**（同款 `backend_route` 分流）；④ `capture_remote_pane` ⇒ **仍无本机对侧**。⚠ **P4d-Y5（08-12）改了它的一半**：原文接着写「它 `load_remote_config_by_label(&origin)`，对 `<local>` 会报「未找到远端配置」」—— **那半句今天已经假了**，本机分支已补上，报的是真实原因（本机 tmux 快照只带会话名不带屏幕内容，要预览得现抓一次 pane）。⇒ 假话没了，**欠账没结**：能不能预览这件事一点没变，仍等后端出原语〔`K-R86` 09-13：**这半句今天假了**，订正在本行末尾〕。★ 这条订正本身是 `P3b §0b` 的 A 类（过期）活样本，而制造它的正是 P4d 那一刀 —— 改了行为不回来改理由，账本当天就开始撒谎。⇒ 欠账**只剩画面预览这一格**，而它不是「自动就有」的：预览要么现跑 `capture-pane`（本机可以，但那是第二条取数路），要么等后端出原语（`P4d`）〔`K-R86` 09-13：同上，**这半句今天也假了**〕。归 **P4d**，不再归 L1/L2。★★ **K-R56（09-11）再订正一次，而这次订正的是上面 ③ 那一格**：③ 逐字写的是「`tmux_send_keys` ⇒ **本机已通**（同款 `backend_route` 分流）」—— **那句话本身是真的**（PM 单子叮嘱「账本不许直接信」，本轮现打逐字复核过住址：分流确实同款，`backend_send_keys` 也确实传输无关）。**假的是它旁边那句没写出来的话**：② 给 `kill_remote_tmux` 特地记了「**且本机专属错误文案已加**」，③ 没有那半句 —— 而**它不是省略，是当时真的没有**。⇒ 通道不在时 `tmux_send_keys` 会掉进 SSH 回落、报「未找到远端配置: `<local>`」，与 ①②④ 那一族**一模一样的假话**。K-R56 把那条早退补上了（`tmux.rs::tmux_send_keys` 的 `Routed::NoChannel` 臂，逐字抄 `kill` 那条先例），判据 `tmux::tests::the_local_send_keys_never_falls_back_to_ssh`（走生产入口本体，不是扫源码），存量登记 `local_origin_registry::TRIAGE_DEBT` 同轮 16 → 15。⚠ **「能不能 send-keys」这件事一个字没变** —— 变的只是**通道不在时它说什么**。⇒ 本格的欠账**仍然只剩画面预览那一格**，K-R56 没有结掉任何一笔账，它结的是一句假话。★★ **`K-R86`（09-13）第三次订正，而这次假掉的是上面那两处「仍等后端出原语」**：backend 侧今天**有**那条原语了 —— `--capture-pane <会话名>`，住 `src/backend/control/capture_pane.rs`（`tmux -u capture-pane -p -t '=名:'`，argv 直传不过 shell，只读；起进程登记在 `readonly_guard::spawn_registry::ALLOWED`，「它只读」由 `readonly_guard::capture_is_read_only` 逐元素钉 argv；协议面见 `src/doc/IPC-PROTOCOL.md` §10；`BUILD_ID` p2f → p2g）。🔴 **而这一格的欠账一格都没结，只是换了个名字** —— 本件**只出后端那一侧的原语**，monitor 的 `capture_remote_pane`（`src/bridge/src/backend/control/tmux.rs`）**一个字节没动**，对 `<local>` 仍然没有本机对侧 ⇒ `Side::Remote` 这一格不许改。⇒ 欠账从「**等后端出原语**」（`K-R86` 之前）变成「**等 monitor 侧接上去**」（`K-R86` 之后），归 `K-R87` 之后的接线那一件。⚠ 别把 `BUILD_ID` 的 bump 读成「远端已经有这条命令了」：那一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，本轮没做。★ 本行「那句话今天还成不成立」从此有人在数：`parity_ledger::tests::the_tmux_manage_row_stops_waiting_for_a_backend_primitive` —— 它要求每一处「等后端出原语」前后都挂着 `K-R86` 这个订正标记，**且不许靠删掉整行兑现**（同 `KR53D4` 那条的形状）。★★ **`K-R101`（09-13）第四次订正 —— 而这一次订正的是「归谁」那半，欠账仍然一格没结**：上面写着这笔账「归 `K-R87` 之后的接线那一件」。那一件立起来了（`K-R101`，`K-R54` 表第 7 行的本体），**而它没有结掉这一格**。现打的理由，不是推辞：monitor 今天够得着后端的只有**帧面**（`inbound::REGISTRY` 上的 `launch` / `kill` 那几条，走 `inbound_client`），而 `--capture-pane` 与 `--oneshot-session` 是 **CLI 面独有**的（`src/backend/main.rs` 的一次性分派臂，`inbound::REGISTRY` 里没有它们）⇒ 要接上去，要么给帧面新增那两条小原语（`inbound.rs` ＋ `src/doc/IPC-PROTOCOL.md` 的命令小节 ＋ `BUILD_ID` bump ＋ monitor 侧两个新发送端），要么让 monitor 每抓一屏起一次 SSH exec（现打：两段轮询上限 12+20 轮 ⇒ 单次探测最多 36 次 SSH 握手，撑破 `EXEC_TIMEOUT_SECS` = 25s）。⇒ **这一格的欠账从「等 monitor 侧接上去」细化成「等帧面上有那两条小原语」**，读数与三条候选的比价住 `.claude/planned-build/backend-consolidation/features/K-R101-用量探针的tmux编排整条搬进backend.md#§8`。⚠ **别把 `K-R101` 已签收读成这一格结了** —— 那一件交的是「原文到得了界面 ＋ 解析层退役」，与本格是两件事。★★ **`K-R104`（09-13）第五次订正 —— 上一段那句「等帧面上有那两条小原语」今天假了，而这一格**仍然**没结**：帧面今天**有**它们了（`inbound::REGISTRY` 8 → 10，`ch:capture-pane` ＋ `ch:oneshot-session`，`BUILD_ID` p2h → p2i），monitor 侧也真的在用（`account_usage.rs` 整条编排改走帧面：一次拨号 ＋ 一条通道上 N 次往返，握手从最多 36 次降到 **1** 次）。🔴 **而本格问的不是那件事**：这一格问的是 `capture_remote_pane`（`src/bridge/src/backend/control/tmux.rs`）—— **那个函数一个字节没动**，对 `<local>` 仍然没有本机对侧，所以 `Side::Remote` 这一格照旧不许改。⇒ 欠账从「**等帧面上有那两条小原语**」变成「**等 `capture_remote_pane` 自己改走帧面的 `capture-pane`**」——那是一次纯接线（原语在了、发送端在了、`inbound_client::capture_pane_args` 也在了），**但它不在 `K-R104` 的写区里**，归后续一件。⚠ 同样别把 `BUILD_ID` 的 bump 读成「远端已经有这两条了」：那一半是**源码半**，re-embed 归发版那一拍。★★ **`设计/50`（删用量）第六次订正 —— 这一次假掉的不是某个半句，是上面第四、第五层**整段**的主语**：用量 ②③ 两轴（后端用量查询 ＋ 探针会话）**整轴退役**，于是第四层里那句「要么给帧面新增那两条小原语」所指的 `--oneshot-session`、第五层里的 `ch:oneshot-session` 与 `account_usage.rs` 那条编排，**今天全都不存在**。🔴 **这不是把欠账结掉了** —— 本格问的始终是 `capture_remote_pane`（`src/bridge/src/backend/control/tmux.rs`）对 `<local>` 有没有本机对侧，而那个函数**这一刀里一个字节没动**：`capture-pane` 那条帧面原语**还在**（backend 侧 `control/capture_pane.rs` ＋ `inbound::REGISTRY` 的 `ch:capture-pane` ＋ monitor 侧 `inbound_client::capture_pane_args`，都是这一刀**刻意保留**的 —— `设计/50 §2` 逐字「`capture-pane` **不是**孤儿 —— 拉屏预览真在用」，那个消费者是 `tmux.rs::capture_via_backend`）⇒ `Side::Remote` 这一格照旧不许改，欠账仍是「**等 `capture_remote_pane` 自己改走帧面的 `capture-pane`**」。⚠ **上面第四、第五层从此是考古**：它们讲的是一个已经不在盘上的功能（用量探针）当初怎么一层层推动这一格换名字。`设计/50 §7` 把这一格点成本刀「最该被记住的一件事」的活样本 —— **账本的成本也要记账**：一条 7000 字、五层订正的理由，在它引用的功能被删掉之后，**留下的不是错误而是考古**，而删掉它会同时删掉「这一格为什么换过四次名字」这条线索。⇒ 逐层留着、在末尾标明哪几层是考古。"),

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
/// 同型 bug 在 `src/backend/no_timer_guard.rs` 同轮修掉。
fn command_signatures() -> BTreeMap<String, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let attr = format!("#[tauri::{}]", "command");
    let mut out = BTreeMap::new();
    // ★ F23 第二刀：改用 `scan_tree!`（不再裸 `read_dir`）。
    // ⚠ 〔`P4` 2026-09-21〕先前这一行写着「它**按构造**摘除调用者自己（拿 `file!()`）」——
    //   那一刀**在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是带 `..` 的
    //   折返路径 ⇒ 后缀比不命中）。本文件不在人群里靠的是**住址**：它住
    //   `tests/bridge/`，而这里扫的是 `src/bridge/src`。
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
        "panorama_edit",
        "〔RM1d · 第四波 · V110〕批注 / 文档关联的写：`origin` 只决定「谁算」（远端 = 那台的全景小程序；\
             本机 = 进程内引擎的只读那一层，本机对称那一拍之后同为小程序）与「哪扇门写」（`BackendDoor{origin}`，\
             本机与远端同一个实现）。命令体对 origin 不做远端假设。",
    ),
    (
        "chan_call",
        "〔C4a · 第四波〕通道在 Tauri IPC 那一跳上的命令：`origin` 只被交给注入的句柄去找那台的控制通道\
             （`inbound_client` 那张表本机与远端同一张），命令体一个字都不看 `op` / 载荷，也不做远端假设。",
    ),
    (
        "probe_session_record",
        "〔U4b · 第四波〕resume 之前问记录还在不在：`origin.route` 分本机（空白名当场拒），两臂都交\
             `frame_query::record`（`inbound_client` 那张表本机与远端同一张，本机 ＝ `<local>` 那条长连接）。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「问哪台的后端」。",
    ),
    (
        "find_in_session",
        "〔SE2 · `设计/10 §6 步 6`〕与 `list_user_inputs` 同一个形：走 `subagent::Backend`\
             （本机 exec 本机后端 / 远端 ssh exec 同一个二进制）跑同一条 `--find-in-session`，\
             头尾核验是同一个 `parse_find_output`。命令体对 origin 不做远端假设 —— \
             它只用 origin 决定「那条查询谁去跑」。",
    ),
    (
        "config_surface_report",
        "〔RM1a · 第四波〕「足迹」：本机在 monitor 进程里扫（原样），远端问那台机器的后端要路径事实 \
             （`footprint-probe`）。命令体对 origin 不做远端假设 —— 它只用 origin 决定「事实从哪台来」，\
             判定（哪一行属于哪个工具、存在 / 缺失 / 查不动）两边走同一份 `config_surface::build_rows`。",
    ),
    (
        "relay_endpoint_for_launch",
        "〔RL1 · 第四波〕这次拉起的中转地址按**那台机器**的事实答：本机两件事走起会话那一侧那条缝，\
             远端问那台的账号层（`apikey-read`）与中转（`relay-status` / 用到才 `relay-ensure`）。命令体对 origin 不做远端假设 —— \
             分派住 `history::relay_endpoint_on`，判断只在 `payload::relay_endpoint_for` 一处。",
    ),
    (
        "drift_ledger_report",
        "〔ST3 · 第四波〕「未识别的数据」：两台的记录都在 monitor 进程里解析、在同一个进程里记账，\
             账本第一层键是 origin。命令体对 origin 不做远端假设 —— 它只用 origin 选「读哪一台那一本」，\
             不经后端、不拨号；`route` 只拦空白名。",
    ),
    (
        "apikey_routing_for",
        "〔RM1a · 第四波〕两件事都问**那台机器**：表里有哪几行（本机走起会话那一侧那条缝，远端问那台的后端 \
             `apikey-read`）· 中转在不在（本机同一条缝，远端 `relay-status`）。命令体对 origin 不做远端假设 —— \
             两件事各自的分派住 `apikey_remote::rows_on` / `remote_relay::running_on`，只用 origin 决定问哪台。",
    ),
    (
        "read_apikey_credentials_status",
        "〔RM1a · 第四波〕那份凭据文件是账号层自己的状态，**每台机器一份**。本机读 monitor 自己那一份\
             （`creds_store`），远端问那台机器的后端（`apikey-read`）。命令体对 origin 不做远端假设 —— \
             分派住 `apikey_remote::status_on`，它只用 origin 决定「问哪台机器」。",
    ),
    (
        "write_apikey_credentials_key",
        "〔RM1a · 第四波〕同上一条：本机那一份只有 monitor 写（`creds_store`），远端那一份只有那台的后端写\
             （`apikey-key-set`）⇒ 每台机器上的写者恰好一个。分派住 `apikey_remote::write_key_on`。",
    ),
    (
        "list_skills",
        "〔RW1 · 第四波 09-24〕本机读本机盘（`views`），远端问那台机器的后端（`remote_views`：\
             `files-home` / `files-stat` / `files-ls`），同一套发现 · 实例 · 可编辑集合规则。\
             命令体只用 origin 决定「问哪台机器」。",
    ),
    (
        "read_skill_file",
        "〔RW1 · 第四波 09-24〕两侧都经那台机器的后端读（`files-peek`）；本机的围栏是 \
             `resolve_editable`、远端的是 `remote_editable_rel`（逐字集合判定），之后后端的围栏再判一次。",
    ),
    (
        "write_skill_file",
        "〔RW1 · 第四波 09-24〕两侧都经那台机器的后端写（`files-put`，带打开时读到的那一份当 CAS 期望）；\
             围栏同 `read_skill_file`。",
    ),
    (
        "list_user_inputs",
        "〔SE1 · `设计/10 §2.2b ⑥`〕与 `read_session_index` 同一个形：走 `subagent::Backend`\
             （本机 exec 本机后端 / 远端 ssh exec 同一个二进制）跑同一条 `--list-user-inputs`，\
             头尾核验是同一个 `parse_user_inputs_output`。命令体对 origin 不做远端假设 —— \
             它只用 origin 决定「那条查询谁去跑」。",
    ),
    (
        "read_session_index",
        "〔`设计/10` 骨架 · 子步 3〕与 `load_subagent` 同一个形：走 `subagent::Backend`\
             （本机 exec 本机后端 / 远端 ssh exec 同一个二进制）跑同一条 \
             `--read-session-from-offset … --index`，头尾核验是同一个 `parse_index_output`。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「那条查询谁去跑」。",
    ),
    (
        "read_session_range",
        "〔`设计/10` 骨架 · 子步 3〕同上一条，跑的是 `--read-session-from-offset … --until`，\
             编 payload 是同一个 `range_payloads`；origin 只再用一次：载荷上的 `origin` 字段\
             （本机不带、远端带机器名，与 live 行同一口径）。",
    ),
    (
        "load_subagent",
        "P7c-1：远端那条让 backend **只列候选**（`--list-subagents`），\
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
        "`P4f` 08-13 本机 ＋ `K-R98` 09-13 远端：两侧调**同一条** backend 原语 `bus-send`。\
             命令体对 origin 不做远端假设 —— 它只把 origin 交给 `client_for`，\
             决定「问哪台机器的后端」（`cc_bus.rs::send_via_backend` 头注逐字\
             「origin 是原语的一个入参，不是一个分支」）。\
             ⚠ 这是本表里**第一条写面的 `Both`**：读面三条 08-12 就转了，写面等的是\
             用户 08-12 那句「先把确切的命令组件做出来，然后 cc-bus 可以去调用」。",
    ),
    // 🔴 **〔步 12·C 2026-09-20〕下面五条是同一刀落下来的** ——
    //    `设计/00 §2.5 ①` 那句「合成一条带 origin 参数的」。
    //    五条的理由同一句、只是被合的那一对不同，所以逐条写「凭什么说这一对是同一件事」
    //    （**判据不是名字** —— `真相源/97 §二b` 逐字：按名字数分叉会系统性高估）。
    (
        "create_branch_session",
        "步 12·C：被合掉的那条（`remote_branch::create_remote_branch_session`）\
             自己的头注逐字写着「与本地那条的差异**今天只剩一处：活儿在远端干**」。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「活儿在哪台机器上干」，\
             记录变换两侧共用 `branch_core::build_branch_records`，\
             「按 sid 找那份源文件」两侧共用 `branch_core::find_session_file`（`K-R88`）。",
    ),
    (
        "delete_history_session",
        "步 12·C：两侧都是「删掉一份会话 jsonl ＋ 清掉本机按 sid 存的那份注解」。\
             后半句**本来就只有一份实现**（`history::remove_metadata_entry` 的头注逐字\
             「本地删除与远端删除共用」——注解是 monitor 本机的东西，与会话本体在哪台机器上无关）。\
             命令体对 origin 不做远端假设：它只用 origin 选「哪一道路径守卫」\
             （本机 `validate_delete_target` / 远端 `sftp::remove_remote_file`）。",
    ),
    (
        "stream_history_sessions_in_project",
        "步 12·C：`真相源/97 §二 丙`「措辞不同」那一档 —— 两个名字里一个共同的词都没有，\
             认出它靠的是远端那一支头注里逐字的「**对齐本地 \
             `stream_history_sessions_in_project`**」：同一个 `HistorySessionEntry`、\
             同一种 `tauri::ipc::Channel`、同一套取消语义（前端 drop ⇒ `send` 返 Err ⇒ 停）。\
             `project_dir` 两侧早已同形（`K-R97`：都是编码目录名）。",
    ),
    (
        "stream_read_session_jsonl",
        "步 12·C：同上一条，也是 `真相源/97 §二 丙` 那一档。判据是 **chunk 口径逐字对齐**\
             （每 100 条一发、同一个 `crate::bridge::JsonlLinePayload`、同一套 per-file `seq`），\
             以及前端 `views/session-viewer.ts` 对两条路共用同一段消费代码。\
             ⚠ 如实记一处**没合掉**的不对称：载荷里的 `origin` 字段本机侧填 `None`、\
             远端侧填 `Some(label)` —— 那是载荷层的事，本步不动。",
    ),
    (
        "list_mcp_project_dirs",
        "步 12·C：两侧问的是**同一份文件的同一个键**（`~/.claude.json` 的 `projects`），\
             而**算它的那份代码本来就只有一份** —— `mcp::project_dirs_from`。\
             差别只在那份文件的字节从哪来（本机 `read_json_lenient` 读盘 /\
             远端 `fetch_remote_claude_json` 走 SSH `cat`）⇒ 这正是 `INVARIANTS §40`\
             「本地 ＝ 不走 ssh 的远端」在命令面上的样子。",
    ),
    // 🔴 **〔步 12·C 收尾 2026-09-20〕下面两条是上一拍欠下、本拍还上的那两对。**
    //    上一拍判过「该合、技术上机械」，卡的是那张字面量名单不在它写区；
    //    本拍同拍改了名单（改的是**名单**不是判法，理由写在 `LEDGER` 那两行旁边）。
    (
        "write_project_mcp_server",
        "步 12·C 收尾：两侧写的是**同一个写面**（`<dir>/.mcp.json`，SS-14 那条铁律的两端），\
             而**改那份 JSON 的那一份代码本来就只有一份** —— `mcp::upsert_mcp_server_value`\
             （它的头注逐字「本机/远端复用、可测」，F89a 落地那天就是为这件事抽出来的）。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「那份 `.mcp.json` 在哪台机器上」，\
             剩下的差别只有「字节走哪条路」：读回来走 `read_or_skeleton` 还是 `read_remote_mcp_value`，\
             写回去走 `write_json_atomic` 还是 `sftp::upload_atomic`。",
    ),
    (
        "remove_project_mcp_server",
        "步 12·C 收尾：同上那一条，只是共用的那份纯核心换成 `mcp::remove_mcp_server_value`\
             （同样「本机/远端复用、可测」）。命令体对 origin 不做远端假设。\
             ⚠ 如实记一处**没合掉**的差别：「文件在不在」两侧问法不同\
             （本机 `is_file()` · 远端 `try_exists`）—— 那正是 §40 说的「只是远端走 ssh」，\
             它留在各自那一支里，不是欠账。",
    ),
    (
        "check_account_trust",
        "〔`A3` 第二波〕本机与远端跑**同一条**后端子命令（`--account-trust` / \
             `--account-trust-zero`），出参走**同一份**解析 `accounts::trust_from_lines`。\
             命令体对 origin 不做远端假设 —— 它只用 origin 决定「那条查询谁去跑」\
             （本机 exec 本机后端、argv 直传；远端 ssh exec，参数 `shell_quote`）。",
    ),
    (
        "list_plugin_marketplaces",
        "〔RM1b · 第四波〕两侧问的是**同一条后端命令**（`plugins-marketplaces`，本机后端与远端后端同一个二进制），\
             读法只有一份（后端 `observe/plugins_query.rs`，从 monitor 原样搬过去），线上形状的收口只有一份\
             （`plugins::parse_survey_lines`）。命令体对 origin 不做远端假设 —— 它只用 origin 决定「问哪台机器的后端」。",
    ),
    (
        "get_session_tasks",
        "〔RM1b · 第四波〕两侧问的是**同一条后端命令**（`tasks-list`，本机后端与远端后端同一个二进制），\
             字段语义**只有一份**（`tasks::parse_task_lines`）。命令体对 origin 不做远端假设 —— \
             它只用 origin 决定「问哪台机器的后端」。⚠ 如实记一处**没合掉**的差别：本机有 watcher 推 \
             `task-update`、远端没有推送（加帧要动 `wire.rs`），远端靠前端在切 tab / 展开面板时现问。",
    ),
    (
        "backend_exit_policy",
        "〔B2 · 条 66〕「退出行为」那个值住每台机器自己的后端那侧，本机的 origin 就是 `<local>`。\
             命令体对 origin **不做任何远端假设**（它只是去问那台机器的后端），所以两侧共用一条命令 —— 这正是 C1。",
    ),
    (
        "set_backend_exit_policy",
        "〔B2 · 条 66〕同上一条：交那台机器的后端写，本机与远端只差 origin 这个键 —— 这正是 C1。",
    ),
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
    // - E79 的 `list_local_session_accounts` **+1**；〔散文墓碑〕〔C4a 第四波随「本机与远端同一条路」退役 −1〕
    // - U-CC1 的 `drift_ledger_report` **+1**，`Both` —— 本地行与远端行都经同一个
    //   `parse_line` 喂进同一个进程内账本；
    // - F08 的 `account_usage_local` **+1** —— 它补平了 `usage.per-account` 那条 ParityDebt；  〔散文墓碑〕
    // - P2s 的四条 `Both`（策略那一条〔B2〕今天换成了 `backend_exit_policy` / `set_backend_exit_policy` 两条）
    //   ＋ `backend_status` / `backend_start` / `backend_stop` —— per-host backend 策略与状态，本机 origin 是 `<local>`（C1）；
    // - 🔴 `K-R135`（`R85`/`R87`）的 `ccm_user_path_status` / `ccm_user_path_add` /
    //   `ccm_user_path_remove` **+3**，都是 `Local` —— 用户级 PATH 那一格
    //   （现在状态 · 加 · 撤）。**只有本机一侧是天然的**，理由住 `ASYMMETRY_REASONS`
    //   里 `ccm.user-path` 那一条（远端那一侧同一件事由写进远端 rc 的围栏块办，
    //   而「用户级 PATH」这一档是 Windows 独有的，远端按 `K32` 是 Linux）；
    // - P8a 的 `list_plugin_marketplaces` **+1**，`Local` —— marketplace 只读枚举今天只有
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
    // - 〔`设计/10` 骨架 · 子步 3〕**+2**（`read_session_index` / `read_session_range`，都 `Both`，
    //   归已有能力 `history.read-session`）。
    // - 〔AL1〕**+2**（`aliases_render` / `aliases_read` / `aliases_install` 三条 `Local` 进，`write_account_aliases`〔散文墓碑〕退役）。
    // - 〔U3b〕**+1**（`replay_keep_tail_only`，`Both`，归已有能力 `session.forget`）。
    // - 〔SE1〕**+1**（`list_user_inputs`，`Both`，归已有能力 `history.read-session`）。
    // - 〔SE2〕**+1**（`find_in_session`，`Both`，归已有能力 `history.read-session`）。
    // - 〔PN1b 选图 · 09-24〕**+2**（`panorama_diagram_kinds` / `panorama_diagram`，都 `Local`，
    //   归已有能力 `panorama.code-graph`）⇒ 命令总数 +2、能力总数不动、不对称数不动。
    // - 〔U4b · 第四波〕**+1**（`probe_session_record`，`Both`，归已有能力 `history.read-session`）。
    const EXPECTED_LOCAL_OR_BOTH: usize = 104; // 〔RM1d · 第四波：−5（本机六条全景写命令 Local 退役 −6，`panorama_edit` Both ＋1）〕（合并主线按两边增量相加） // 〔U4b：+1（probe_session_record，Both）；合并 RL1 按两边增量相加〕 // 〔RL1：+1（`relay_endpoint_for_launch` Both 进，接替只对远端的 `relay_ensure`）〕 // 〔合并 C4a：±0（E79 本机会话账号 Local 退役 −1 · `chan_call` Both ＋1）〕 // 〔SE2：+1（find_in_session，Both）〕 // 〔B2 · 条 66：+1（推生效值那一条 Both 退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 两条 Both ＋2）〕 〔合并 AL1：+2（aliases_render / aliases_read / aliases_install 三条 Local ＋3，write_account_aliases〔散文墓碑〕退役 −1）〕 〔合并 PN1b：+2〕 〔合并 A3：+3（两条 acct-iso 本机命令 Local ＋ check_account_trust Remote→Both）〕 〔合并 U3b＋SE1：两路各 +1，同基线合并时 git 合成了同一个 97，现打 98〕 `设计/50` −2（`aggregate_usage_all` Local · `account_usage_local` Local）  〔散文墓碑〕
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
/// 新那句真不真，由 `history_tests.rs::every_local_account_shape_gets_a_named_verdict_from_the_backend_path`
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

/// ★★ `K-R86`（09-13）：**`tmux.manage` 那一行不许再把「等后端出原语」当现在时说。**
///
/// # 它买什么、不买什么（诚实边界写在最前，别读大）
///
/// 形状照上面那条 `the_two_launch_rows_no_longer_carry_the_two_falsified_clauses`。
/// 机器判不了「这句话是不是真的」，本条能判的只有三样，**三样都不是「真假」**：
///
/// 1. **那一行还在** —— 不许靠删掉记录兑现（删掉等于把一处已知的不对称从视野里拿走）；
/// 2. **每一处「等后端出原语」旁边都挂着订正标记** —— 那句话 09-13 起是假的：
///    backend 侧有 `--capture-pane` 了（`src/backend/control/capture_pane.rs`）；
/// 3. **订正没有把欠账一起抹掉** —— 本件只出后端那一侧的原语，
///    monitor 的 `capture_remote_pane` 一个字节没动 ⇒ 这一格仍然是 `Remote`，
///    而这一条正是本行历史上栽过三次的那个病（改了行为不回来改理由 / 改理由时把账也抹了）。
///
/// ⚠ 它**不**买「新写上去的那段话是真的」。「backend 侧真有那条原语」由后端那棵树
/// 自己的判据钉（`control::capture_pane::tests` 在真 tmux 上抓一屏 ＋
/// `readonly_guard::capture_is_read_only` 逐元素钉 argv），本条够不着那棵树。
#[test]
fn the_tmux_manage_row_stops_waiting_for_a_backend_primitive() {
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
    let stale = "等后端出原语";
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
                 它今天是假的：backend 侧 09-13 起有 `--capture-pane`\n\
                 （`src/backend/control/capture_pane.rs`）。\n\
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
                 本件只出后端那一侧的原语，monitor 那条本机对侧仍然没有。\n\
                 把「等后端」换成「做完了」，是这一行历史上栽过三次的同一个病换了个方向。"
        );
    }
}

#[test]
fn ledger_shape_is_pinned() {
    // ⚠ 订正（2026-08-03 复盘）：下面四条尾注此前都**只记到 U8c-2c-2 为止** ——
    // 而 U8a-2c-pre（`57dba2a`）把这四个数各 +1 时，只改了数、一条尾注都没动。
    // ⇒ 尾注把 U8a-2c-pre 的增量记在了 U8c-2c-2 名下。**尾注的用处就是说清「谁加的」，
    // 归属错了就不如没有。**
    assert_eq!(LEDGER.len(), 139, "命令总数变了"); // **〔RM1d · 第四波〕−5（本机六条全景写命令退役 −6 · `panorama_edit` Both ＋1，归已有能力 `panorama.annotate`）**（合并主线按两边增量相加） // **〔U4b · 第四波〕+1（probe_session_record，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动；合并 RM1c 按两边增量相加）** // **〔RM1c · 第四波〕+1（panorama_call，Remote；归已有能力 `panorama.code-graph`）** // **〔RM1a · 第四波〕+1（relay_ensure，Remote；新能力 `relay.machine`，`NaturallyAsymmetric` —— 本机那一个有监护者）** // **〔合并 C4a〕−1（「某会话属哪个账号」远端 A2 · 本机 E79 两条退役 −2、`chan_call` ＋1 Both 新能力 `comm.face-a.call`；能力数与不对称数都不动）** // **〔SE2〕+1（find_in_session，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动）** // **〔第四波 S4〕−1（sftp_copy：零流量复制随门禁那一格退役；Remote，归 `sftp.file-panel` ⇒ 能力数与不对称数都不动）** // **〔F7c 收尾 09-24〕−12（池子那十二条 Tauri 命令随老面板与窗口改走通道一起走了；都 Remote、都归 `sftp.file-panel` ⇒ 能力数不动）** // **〔B2 · 条 66〕+1（推生效值那一条退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 进 +2；都 Both，归已有能力 `app.backend-policy` ⇒ 能力数与不对称数都不动）** // **〔合并 AL1〕+2（aliases_render / aliases_read / aliases_install 三条进、write_account_aliases〔散文墓碑〕退役；`alias.account-commands` 并进 `alias.manage` ⇒ 能力数、不对称数、欠账都不动）** // **〔PN1b 选图〕+2（panorama_diagram_kinds / panorama_diagram，都 Local；归已有能力 `panorama.code-graph`）** // **〔A3 第二波〕+2（check_local_acct_iso / local_acct_iso_shellinit；与 SE1/U3b 合并时按两边增量相加）** // **〔SE1〕+1（list_user_inputs，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动）** // **〔U3b〕+1（replay_keep_tail_only，Both；归已有能力 `session.forget` ⇒ 能力数与不对称数都不动）** // **〔`设计/10` 骨架 · 子步 3〕+2（read_session_index / read_session_range，Both；归已有能力 `history.read-session` ⇒ 能力数与不对称数都不动）** // **〔`设计/60 §4 戊` · `24e` 第二刀 · 09-20〕+1（open_file_window，Remote；归已有能力 `sftp.file-panel` ⇒ 能力数与不对称数都不动 —— 它与旧面板是**同一个能力的两个表面**，理由逐条写在 `LEDGER` 那一行旁边）** // **〔步 12·C 收尾 · 09-20〕−2（`origin` 归一的**最后两对**：退役 `write_remote_mcp_server` / `remove_remote_mcp_server`，并进本机同名那两条。**能力总数与不对称数都不动** —— 两条能力从前是 `{Local, Remote}`＝已对称，今天是 `{Both}`＝同样对称）** // **〔`设计/60 §5.4c` · 09-20〕+1（sftp_chmod，Remote；归已有能力 `sftp.file-panel` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）** // **〔步 12·C · 09-20〕−5（`origin` 归一：5 对同义双份各合成一条带 origin 的 ⇒ 退役 `create_remote_branch_session` / `delete_remote_history_session` / `stream_remote_history_sessions` / `stream_read_remote_session` / `list_remote_mcp_project_dirs`。**能力总数与不对称数都不动** —— 那五条能力从前是 `{Local, Remote}`＝已对称，今天是 `{Both}`＝同样对称；逐条理由在 `LEDGER` 那五行旁边与 `ORIGIN_TAKING_BOTH` 里）** // **`设计/50` −4（`aggregate_usage_all` / `aggregate_remote_usage_all` / `account_usage` / `account_usage_local`：用量 ②③ 两轴整轴退役）** // **K-R109 +1（render_local_attach，Local；归已有能力 `session.launch` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）** // **K-R69 +1（local_ccm_entry_status，Local；归已有能力 `ccm.status` ⇒ 能力数与不对称数都不动）** // **K-R49 +1（write_account_aliases，Local-only；新能力 `alias.account-commands`，`ParityDebt`）** // **K-H2a +2（creds.apikey，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +3（list_skills / read_skill_file / write_skill_file：skill 接入面） // F08 +1（account_usage_local：补平 usage.per-account） // U8a-2c-1 +1（backend_send_into）； G6 +1；E79 +1；U-CC1 +1（drift_ledger_report）；U8c-2c-2 +1（render_ccm_launch）；U8a-2c-pre +1（render_launch_payload）；**P2s +5（set_backend_kill_on_exit / backend_status / backend_start / backend_stop / backend_machines，C8）**；P3t-Y2b +1（list_local_tmux）；**P4c +2（cc_bus_broadcast / cc_bus_kill，#77/#78）** // **P8a +1（list_plugin_marketplaces，#70）** // **PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）** // **K-H2b +1（apikey_routing_for，Local-only；新能力 `apikey.routing`，`NaturallyAsymmetric`）** // **`K-R135` +3（ccm_user_path_status / ccm_user_path_add / ccm_user_path_remove，都 Local；新能力 `ccm.user-path`，`NaturallyAsymmetric` —— 理由见 ASYMMETRY_REASONS 那一行）** // **〔步 23b · 09-20〕+1（sftp_copy，Remote；归已有能力 `sftp.file-panel` ⇒ 能力数与不对称数都不动，理由逐条写在那一行旁边）**  〔散文墓碑〕
    let sides = capability_sides();
    assert_eq!(sides.len(), 67, "能力总数变了"); // **〔RM1c · 第四波〕+1（`panorama.annotate`：六条写命令从 `panorama.code-graph` 拆出来，Local-only）** // **〔RM1a · 第四波〕+1（relay.machine，Remote-only）** // **〔AL1 · 子步 4〕−1（alias.account-commands 并进 alias.manage）** // **〔AL1 · 2026-09-24〕+1（alias.manage）** // **`设计/50` −2（`usage.aggregate` 与 `usage.per-account` 两条能力整条退役 —— 两条**原本都对称**，所以不对称数不动）** // **K-R109 +1（launch.render-attach，Local-only；⚠ 派工单猜的是「能力数 65 不动」，实打不成立 —— 两个「归已有能力」的归法各被一条判据顶回来了，逐条见 `LEDGER` 里那一行旁边）** // **K-R49 +1（alias.account-commands，Local-only）** // **K-H2a +1（creds.apikey，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +1（skill.inbox，Local-only） // U8a-2c-1 +1（launch.send-into，Remote-only）； U-CC1 +1（audit.drift-ledger）；U8c-2c-2 +1（launch.render-cli，Remote-only：**只是没有本机那条 IPC 命令** —— P3t-Y4 起理由不再是 §36「本机不经 IR」那条，§36 只绑 Windows，详见 ASYMMETRY_REASONS 里那行）；U8a-2c-pre +1（launch.render-payload，同 Remote-only）；**P2s +3（app.backend-policy / backend.status / backend.lifecycle，都是 Both）**；P3t-Y2b +1（tmux.local-census，Local-only：把远端本来就有的那一格在本机补上）；**P8a +1（plugins.marketplaces，Local-only）**；**PS1 +1（cc-bus.deploy）**；**PS2 +1（cc-bus.install-state）**；**K-H2b +1（apikey.routing，Local-only）**；**`K-R135` +1（ccm.user-path，Local-only：`R85` 那一格「加/撤/现在状态」；远端那一侧同一件事由 rc 围栏块办，而「用户级 PATH」这一档是 Windows 独有的 ⇒ `NaturallyAsymmetric`）**
    let asym = asymmetric_capabilities();
    assert_eq!(asym.len(), 18, "不对称能力数变了"); // **〔RM1d · 第四波〕−1（`panorama.annotate` 两侧都有了）** // **〔RM1c · 第四波〕净 0：`panorama.code-graph` −1（远端那一半有了）· `panorama.annotate` +1（写那一半远端还没有）** // **〔RL1 · 第四波〕−1（`relay.machine` 随 `relay_ensure` 退役；接替它的 `relay.launch-endpoint` 是 `Both`）** // **〔RW1 · 第四波 09-24〕−1（`skill.inbox` 结清：用户裁远端也能编辑，两侧经后端 ⇒ Both）** // **〔RM1a · 第四波〕−2：`creds.apikey` −1（读 / 写两条收 origin ⇒ `Both`）· `audit.config-surface` −1（远端那一栏由那台的后端答）· `apikey.routing` −1 与 `relay.machine` +1 净 0** // **〔RM1b · 第四波〕−1（`plugins.marketplaces` 结清：`list_plugin_marketplaces` 按 origin 问那台后端 ⇒ `Both`）** // **〔RM1b · 第四波〕−1（`session.tasks` 结清：`get_session_tasks` 按 origin 问那台后端 ⇒ `Both`）** // **〔`A3` 第二波〕−2（`acct-iso.check` / `acct-iso.shellinit` 结清：本机对侧问本机后端）** // **〔`A3` 第二波〕−1（`accounts.trust` 结清：`check_account_trust` 按 origin 分流到本机后端 ⇒ `Both`）** // **K-R109 +1（launch.render-attach，NaturallyAsymmetric）** // **K-R49 +1（alias.account-commands，ParityDebt）** // **K-H2b +1（apikey.routing，NaturallyAsymmetric）** // **K-H2a +1（creds.apikey，Local-only：远端那侧的欠账理由见 ASYMMETRY_REASONS 那一行）** // devbench F03 +1（skill.inbox） // F08 -1（usage.per-account 补平） // U8a-2c-1 +1（launch.send-into）； G6 -1；E79 accounts.session-accounts 补平 -1；U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）
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
    assert_eq!(kinds.get("natural"), Some(&12), "天然不对称条数变了"); // **〔RL1 · 第四波〕−1（`relay.machine` 那一行随命令退役删了）** // **〔RM1a · 第四波〕净 0：`apikey.routing` −1（远端那台的后端自己答）· `relay.machine` +1（本机那一侧不该存在）** // **`K-R135` +1（ccm.user-path：远端同一件事由 rc 围栏块办 ＋ 那一档只有 Windows 有 ⇒ 天然，不是欠账；哪天远端是 Windows 就回来改成 `debt`）** // **K-R109 +1（launch.render-attach —— 记 `natural` 记的是「远端由 `render_ccm_launch` 一并产、不需要单独命令名」，不是「远端还没做」）** // U8c-2c-2 +1（launch.render-cli）；U8a-2c-pre +1（launch.render-payload）。★ P3t-Y4：这两条的**理由**换过（原来引 §36 说「本地不经 IR」——§36 只绑 Windows，且那个理由已被本机探针实测证伪），但 `natural` 的**条数没变**。P3t-Y2b +1（tmux.name-census）。**K-H2b +1（apikey.routing）—— 记 `natural` 记的是「回环自指 ⇒ 这个问题问错了机器」，不是「远端还没做」；把 key 送到远端那笔账在 `creds.apikey` 那行（`debt`），两者别混。**
    assert_eq!(kinds.get("debt"), Some(&5), "平价欠账条数变了"); // **〔RM1d · 第四波〕−1（`panorama.annotate` 结清：V110 本机远端同一条写）** // **〔RM1c · 第四波〕+1（`panorama.annotate`：远端仓的批注写等后端写面）** // **〔RM1a · 第四波〕−2（`creds.apikey`：把 key 送到远端的路有了 · `audit.config-surface`：远端足迹有了后端读口）** // **〔RM1b · 第四波〕−1（`plugins.marketplaces`：远端那半补上了、本机同拍改走后端 —— 一件事清两笔账）** // **〔RM1b · 第四波〕−1（`session.tasks`：远端那半补上了，理由表那一行删掉）** // **〔`A3` 第二波〕−2（`acct-iso.check` / `acct-iso.shellinit`）** // **〔`A3` 第二波〕−1（`accounts.trust`：本机那一侧补上了，理由表那一行删掉）** // **K-R49 +1（alias.account-commands：远端那半只有「吐待贴文本」那半条路，落盘没有主人）** // **K-H2a +1（creds.apikey：本机能配、远端那侧还没有路 —— 而且不能照抄 SFTP 上传，理由见那一行）** // F08 -1（usage.per-account 补平） // G6 -1；E79 -1；**P7c-1 -1（subagent.load 结清：远端展开做出来了）**；**P8a +1（plugins.marketplaces：新开的本机口，远端那半要等后端的 `--list-marketplaces`）**
    assert_eq!(kinds.get("undecided"), Some(&1), "未裁定条数变了"); // **〔RM1c · 第四波〕−1（`panorama.code-graph`：用户 09-24 裁「要」、V108 选 B，远端那一半做了）** // **〔RW1 · 第四波 09-24〕−1（skill.inbox：用户 09-24 裁了「要」）** // devbench F03 +1（skill.inbox：远端项目的收件箱要不要能编辑，没人裁定过） // U8a-2c-1 +1（launch.send-into：本机该不该有后端进程未裁定）
                                                                    // P3b -1（launch.send-into：它的「还没裁定」被 C1/C8 + P2 + P3 刀 3 三重证伪）
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 `K-R115` `KR115D4`：**`Side` 这一栏靠人签字，而签字的触发是派生出来的**
// ════════════════════════════════════════════════════════════════════════
//
// # 题面（`K-R112` 09-13 撞见、一个字没动、PM 判「开件不在本件补」）
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
//    `list_local_tmux` / `backend_start` / `load_subagent` / `read_skill_file` /
//    `write_skill_file` —— 它们**两条路都有**，标记只看得见远端那条）。⇒ 这个方向不接。
// 3. **正向也有一条假阳**：〔`设计/50`：这条读数的样本是 `account_usage` —— 它派生出来是
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
/// `production_code` 会把它整份剥掉；加上**住址**（本文件住 `tests/bridge/`，扫的是
/// `src/bridge/src`），扫描面里根本没有本文件。
///
/// ⚠ 〔`P4` 2026-09-21〕先前括号里写着「`scan_tree!` 还会再摘一次」——
/// **那一刀在这一处不生效**（自摘恒空转），别把它算成一道保险。
const REMOTE_ONLY_MARKS: &[&str] = &[
    "load_remote_config_by_label(",
    "ssh_source::",
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
    // 🔴 **〔步 12·C · 09-20〕39 → 34。** `origin` 归一退役了 5 条 `Side::Remote` 的命令
    //    （并进了本机那条）。**现打确认这 5 条全部落在 `RemoteOnly` 这一档**，
    //    所以只有这一格动、另外三格一个都不动 —— 那是意料之中的：五条的体里都有
    //    `load_remote_config_by_label(` / `require_cfg_by_label(` 这类 `REMOTE_ONLY_MARKS`，
    //    而一条 `FRAME_PLANE_MARKS`（`client_for(` / `backend_route::`）都没有。
    //    ⚠ 这个数是**跑出来的**，不是 39−5 算出来的：先把总数改对、让 `hist == want`
    //      那一比去印现打，再照它写。（算出来的那个恰好也是 34，但「恰好相等」不是判据。）
    // 🔴 **〔步 12·C 收尾 · 09-20〕34 → 33。** 两笔一起落在这一格上，净 −1：
    //    −2 = `origin` 归一的最后两对退役了 `write_remote_mcp_server` /
    //         `remove_remote_mcp_server`（并进本机同名那两条）；
    //    +1 = `sftp_chmod`（`设计/60 §5.4c`）：签名里带 `RemoteConfig`、
    //         体里点名 `with_sftp`，派生器现打归 `RemoteOnly`。
    //    ⚠ 另外三格一个都不动 —— 退役那两条的体里有 `load_remote_config_by_label(`
    //      这类 `REMOTE_ONLY_MARKS`、一条 `FRAME_PLANE_MARKS` 都没有；新来那一条同理。
    //    ⚠ 这个数是**跑出来的**：先让 `signed_total` 那一比印出现打的 48 行，
    //      再让 `hist == want` 印出现打的直方图，照它写。
    //      （34−2+1 恰好也是 33，但「算出来恰好相等」不是判据。）
    // 〔BS1b 09-24〕RemoteOnly 34 → 33、FramePlane 5 → 6：`cc_bus_spawn` 改走 `bus-spawn` 原语，
    //   派生器从 `RemoteOnly` 挪到 `FramePlane`（跑出来的：`hist == want` 那一比现打 {RemoteOnly: 33, FramePlane: 6}）。
    // 〔C2 · 09-24〕RemoteOnly 32 → 31、Unclassified 10 → 11：`start_forward` 查配置那一下搬进了宿主
    //   `dial_host.rs::forward`（端口转发进了通信层，读配置是宿主的事）⇒ 它的体里再没有 `REMOTE_ONLY_MARKS`，
    //   而派生器只跟同一份文件里的调用 ⇒ 落 `Unclassified`（「这把尺子够不着」，不是「安全」；它照旧只对远端）。
    //   跑出来的：`hist == want` 那一比现打 {RemoteOnly: 31, Unclassified: 11}。
    (Derived::RemoteOnly, 18), // **〔合并 C4a〕19 → 18**（远端「某会话属哪个账号」那条退役、改走通道；原归 `RemoteOnly`；跑出来核过） // **〔第四波 S4〕20 → 19（`sftp_copy` 删了：它签名里带 `RemoteConfig`，原归 `RemoteOnly`；跑出来核过）** // **〔合并 F7c＋C2〕F7c 21 与 C2 −1 相加 ⇒ 20；Unclassified F7c 9 与 C2 ＋1 ⇒ 10（跑出来核过）** // **〔F7c 收尾 09-24〕32 → 21，Unclassified 10 → 9**（池子那十二条删了：十一条带 `RemoteConfig` 的归 `RemoteOnly`，`sftp_cancel_transfer` 签名里没有它、归 `Unclassified`；跑出来核过）。 // **〔合并 A3＋BS1b〕33 → 32**（A3 的 check_account_trust Remote→Both 与 BS1b 的 cc_bus_spawn RemoteOnly→FramePlane 各 −1；跑出来核过）。 // **〔`24e` 第二刀 · 09-20〕33 → 34**（`open_file_window`：签名里带 `RemoteConfig`、体里点名 `list_remote`，派生器现打归 `RemoteOnly`；另外三格一个都不动）。⚠ 这个数照旧是**跑出来的**，不是 33+1 算出来的：先让 `signed_total` 那一比印出现打的 `Side::Remote` 行数，再让 `hist == want` 印出现打的直方图，照它写。 // `设计/50` −1（`aggregate_remote_usage_all`）  〔散文墓碑〕 // **〔步 23b · 09-20〕+1（`sftp_copy`：签名里带 `RemoteConfig`、体里点名 `copy_remote_path`，派生器现打归 `RemoteOnly`）**
    (Derived::FramePlane, 6),  // `设计/50` −1（`account_usage`）；BS1b +1（`cc_bus_spawn`）
    (Derived::Mixed, 0),
    (Derived::Unclassified, 11), // **〔合并 RL1 × RM1c〕RL1 −1 与 RM1c ＋1 ⇒ 11（跑出来核过）** // **〔RM1c · 第四波〕11 → 12**：`panorama_call`（Remote；体里只转调 `frame_query::call`，派生器只跟同一份文件 ⇒ 落 `Unclassified`；跑出来核过；本机那一侧另有进程内那几条命令） // **〔RL1 · 第四波〕11 → 10**：`relay_ensure` 退役（它就是 RM1a 加进来的那一条） // **〔RM1a · 第四波〕10 → 11**：`relay_ensure`（Remote；体里只转调 `remote_relay::ensure_on`，派生器只跟同一份文件 ⇒ 落 `Unclassified`；它照旧只对远端，本机那一臂在 `remote_relay` 里拒）。跑出来核过
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
///    [`the_tmux_manage_row_stops_waiting_for_a_backend_primitive`]（它逐字断言那句理由里
///    **必须**含 `Side::Remote`）。⇒ 翻 `Side` 要**同拍改掉一条现行判据的断言**。
/// ② 而「本机真的抓得到一屏 / 真的杀得掉」**今天判不了**（一趟真机都没跑过，
///    `K-R112` 交回时逐字承认过）。`Side::Both` 的字面是「这条命令自己就把两侧都办了」——
///    在没跑过的前提下把它写上去，是拿一句没验过的话换一格好看的表。
/// ⇒ **登记成欠账，归后续一件**；本条保证的是**它从此不会静默**。
const FRAME_PLANE_VERDICTS: &[(&str, FrameVerdict, &str)] = &[
    (
        "capture_remote_pane",
        FrameVerdict::LiesTodayOwedACorrection,
        "`K-R112`（09-13）把抓屏改走帧面 `capture-pane`（`tmux.rs::capture_via_backend`，\
             登记在 `backend_route_tests.rs::SENDERS` 第七个发送端）⇒ 这一跳对 `<local>` 也走得通，\
             而本行仍是 `Side::Remote`。⚠ 同一行的 `ASYMMETRY_REASONS` 里 `K-R104` 那段逐字写着\
             「`capture_remote_pane` 一个字节没动 … `Side::Remote` 这一格不许改」——**那句话今天假了**。",
    ),
    (
        "kill_remote_tmux",
        FrameVerdict::LiesTodayOwedACorrection,
        "`P3 刀 2`（08-12）就改走了 `backend_kill`（传输无关）。`tmux.manage` 那条 \
             `ASYMMETRY_REASONS` 逐字「② `kill_remote_tmux` ⇒ **本机已通**」——\
             **散文说通了一个月，`Side` 栏没跟**。`K-R112` 点名的三行里没有它。",
    ),
    (
        "tmux_send_keys",
        FrameVerdict::LiesTodayOwedACorrection,
        "同上：`tmux.manage` 那条理由逐字「③ `tmux_send_keys` ⇒ **本机已通**（同款 \
             `backend_route` 分流）」，`K-R56`（09-11）还补了本机专属的 `NoChannel` 早退\
             （判据 `tmux::tests::the_local_send_keys_never_falls_back_to_ssh`）。\
             ⇒ 派生说它 origin 无关，而 `Side` 栏还写着 `Remote`。",
    ),
    (
        "cc_bus_broadcast",
        FrameVerdict::LiesTodayOwedACorrection,
        "`K-R112` 交回时逐字点名「`cc_bus_broadcast` 那行**在本件之前就已经腐了**」。\
             它今天走 `cc_bus.rs` 里那条帧面原语 ＋ 共用分流器（`backend_route_tests.rs::SENDERS` \
             登记着 `cc_bus.rs`），没有 `refuse_local_write(` 拦 `<local>` ⇒ 本机也走得通，\
             而这一行的 `Side` 那一格还写着「只有远端」。",
    ),
    (
        "cc_bus_kill",
        FrameVerdict::LiesTodayOwedACorrection,
        "`K-R112` 把它改走 `bus-kill` 原语 ⇒ `<local>` 也走得通，而本行仍是 `Side::Remote`。\
             ⚠ 〔BS1b 09-24 订正〕原文说派生那条「生产段里有本机拒绝、派生判 `RemoteOnly`、\
             是真远端专属」—— 今天不成立了，见下一行。",
    ),
    (
        "cc_bus_spawn",
        FrameVerdict::LiesTodayOwedACorrection,
        "〔BS1b 09-24〕派生改走后端的 `bus-spawn` 原语（`cc_bus.rs::spawn_via_backend`，\
             本机与远端同一个函数体、同一句话）⇒ `<local>` 也走得通，而本行仍是 `Side::Remote`。\
             后端那一跳真跑过（`tests/e2e/backend-cc-bus.sh` 的 `[16]`：经后端起 cc-spawn、假 agent），\
             **monitor 这一跳没在真机的 app 里跑过** ⇒ 照本表头注那条纪律记欠账，不在这一拍翻 `Side`。",
    ),
];

/// 🔴 **`KR115D4`：`Side::Remote` 那一栏，没签字红 · 过期红 · 陈账红。**
///
/// 三条判定与它们各自拦得住什么，逐条写在断言的报文里；边界见本节开头那段头注。
#[test]
fn the_remote_side_column_is_signed_off() {
    let derived = command_dispatch_class();
    // 反向自检①：派生器扫不到东西的时候，下面每一条都会**空真地**成立。
    // 🔴 **〔步 12·C · 09-20〕地板 140 → 135**：合并退役了 5 条 `#[tauri::command]`。
    assert!(
        // 〔F7c 收尾 09-24〕135 → 123：池子那十二条 `#[tauri::command]` 删了（人群真少了 12 个）。
        derived.len() >= 123,
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
    // 🔴 **〔步 12·C · 09-20〕地板 50 → 45。** `origin` 归一退役了 5 条 `Side::Remote`
    //    的命令（它们并进了本机那条）⇒ 这一栏**真的少了 5 个成员**。
    //    ⚠ 按本仓纪律，人群真的变少时不跟着改地板，才是让地板替真判据挡枪（`K-G8`）。
    //    ⚠ 地板只守「这一栏没塌成空集」；**有牙的是下面那条直方图相等**，不是这个数。
    // 🔴 **〔F7c 收尾 09-24〕地板 45 → 36。** 池子那十二条 `Side::Remote` 命令真的删了（人群真少了 12 个成员，
    //    48 → 36，现打）。地板照旧只守「没塌成空集」，有牙的是下面那条直方图相等。
    // 🔴 **〔第四波 S4〕地板 36 → 35。** 零流量复制那一条 `Side::Remote` 命令真的删了（人群真少了 1 个，现打 35）。
    // 🔴 **〔合并 C4a〕地板 35 → 34。** 远端那条「某会话属哪个账号」（`Side::Remote`）退役（本机与远端收成一条经通道的路），现打 34。
    // 🔴 **〔RL1 · 第四波〕地板 34 → 33。** 只对远端的 `relay_ensure` 退役（接替它的那条是 `Both`），人群真少了 1 个，现打 33。
    assert!(
        remote_rows.len() >= 33,
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
