use std::collections::{BTreeMap, BTreeSet};

/// 一条命令服务哪一侧。`Both` = 这条命令自己就把两侧都办了。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Side {
    Local,
    Remote,
    Both,
}

/// monitor 自己的事分几类。`may_touch` 那三类碰后端是本分；其余碰了就是没迁干净（`command_home_registry` 判）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Own {
    /// 开窗 / 拉前 monitor 自己的窗口。
    Window,
    /// 拉前 · 绑窗口（把用户桌面上那个终端窗口拉到前面）。
    Front,
    /// 本机 monitor 配置（`config.json` · `auto-launch.json` · 机器表）。
    Config,
    /// 日志 · 诊断 · 数据位置。
    Log,
    /// 重放缓冲（monitor 进程内的界面状态）。
    Replay,
    /// 本机后端的起停与引导（含「这台后端通道在不在」）。
    Lifecycle,
    /// 通信层面 A 客户端本身。
    Channel,
    /// 放字节：照那台后端判好的计划往那台放 / 删 monitor 带着的字节。
    Place,
    /// 足迹里 monitor 自己那台那几行的事实（判定在后端）。
    Footprint,
}

impl Own {
    pub(crate) fn may_touch(self) -> bool {
        matches!(self, Own::Lifecycle | Own::Channel | Own::Place)
    }
}

/// **壳的全部 Tauri 命令：命令 → 能力 · 服务哪一侧 · monitor 自己的哪一类事 · 理由**（一张表，两份判据读它）。
///
/// 前两列由本模块判（两向等于 `generate_handler!`、不对称要写理由）；后两列由 `command_home_registry` 判
/// （不许碰后端的那几类，实现传递可达 `local_client` / `BackendDoor` ⇒ 红）。改 `generate_handler!` 就必须来改这张表。
pub(crate) const LEDGER: &[(&str, &str, Side, Own, &str)] = &[
    (
        "open_session_in_new_window",
        "app.window.session",
        Side::Both,
        Own::Window,
        "开窗",
    ),
    (
        "open_terminal_in_new_window",
        "app.window.session",
        Side::Both,
        Own::Window,
        "开窗",
    ),
    (
        "open_settings_window",
        "app.window.settings",
        Side::Both,
        Own::Window,
        "开窗",
    ),
    (
        "bring_monitor_to_front",
        "app.window.self",
        Side::Both,
        Own::Window,
        "拉前 monitor 自己",
    ),
    (
        "cc_get_auto_launch",
        "app.auto-launch",
        Side::Both,
        Own::Config,
        "`auto-launch.json` 是 monitor 自己写的小文件",
    ),
    (
        "cc_set_auto_launch",
        "app.auto-launch",
        Side::Both,
        Own::Config,
        "`auto-launch.json` 是 monitor 自己写的小文件",
    ),
    (
        "frontend_perf_log",
        "app.diagnostics",
        Side::Both,
        Own::Log,
        "日志",
    ),
    (
        "get_diagnostics_config",
        "app.diagnostics",
        Side::Both,
        Own::Log,
        "诊断开关",
    ),
    (
        "set_diagnostics_config",
        "app.diagnostics",
        Side::Both,
        Own::Log,
        "诊断开关",
    ),
    (
        "get_log_file_info",
        "app.logs",
        Side::Both,
        Own::Log,
        "日志",
    ),
    (
        "diagnostics_report",
        "app.logs",
        Side::Both,
        Own::Lifecycle,
        "日志页「复制诊断信息」那一段：各台此刻的状态成品（读这台后端通道在不在，与 `backend_status` 同一个读面）\
         ＋ 界面交来的记录账 ＋ 日志位置，只读、不经后端发东西",
    ),
    (
        "open_log_file",
        "app.logs",
        Side::Both,
        Own::Log,
        "日志",
    ),
    (
        "restart_app",
        "app.config",
        Side::Both,
        Own::Lifecycle,
        "设置窗「现在重启」：重起 cc-monitor 自己（走退出臂，本机后端照它自己那份退出行为去留）",
    ),
    (
        "filewin_interrupts",
        "app.config",
        Side::Both,
        Own::Window,
        "退出 / 重启 cc-monitor 之前现读各文件窗口说的「会打断什么」（那几扇窗是 monitor 起的子进程，表住 monitor 进程里）",
    ),
    (
        "clipboard_write",
        "app.window.self",
        Side::Both,
        Own::Window,
        "写系统剪贴板（全产品的复制）：剪贴板是 monitor 面前这台桌面的，经系统接口写、回真成败（`clipboard.rs`）",
    ),
    (
        "clipboard_read",
        "app.window.self",
        Side::Both,
        Own::Window,
        "读系统剪贴板（登录抽屉［粘贴］）：剪贴板是 monitor 面前这台桌面的，经系统接口读（`clipboard.rs`）",
    ),
    (
        "open_log_dir",
        "app.logs",
        Side::Both,
        Own::Log,
        "日志",
    ),
    (
        "backend_status",
        "backend.status",
        Side::Both,
        Own::Lifecycle,
        "这台后端通道在不在 ＋ 本机后端 pid / 重启次数 —— 起停引导那一格的读面",
    ),
    (
        "backend_machines",
        "backend.lifecycle",
        Side::Both,
        Own::Config,
        "机器表 = monitor 配置里登记的远端 ＋ 本机，读的是 monitor 自己的注册表",
    ),
    (
        "backend_start",
        "backend.lifecycle",
        Side::Both,
        Own::Lifecycle,
        "起本机后端（自释放 ＋ 监护）/ 起远端常驻",
    ),
    (
        "backend_stop",
        "backend.lifecycle",
        Side::Both,
        Own::Lifecycle,
        "停后端（SIGTERM → 等 → 超时才 SIGKILL）",
    ),
    (
        "load_config",
        "app.config",
        Side::Both,
        Own::Config,
        "本机 monitor 配置",
    ),
    (
        "patch_config",
        "app.config",
        Side::Both,
        Own::Config,
        "本机 monitor 配置",
    ),
    (
        "get_data_paths",
        "app.data-paths",
        Side::Both,
        Own::Log,
        "设置页「数据」区列 monitor 自己的持久路径",
    ),
    (
        "forget_session",
        "session.forget",
        Side::Both,
        Own::Replay,
        "只丢 monitor 进程内那条会话的重放缓冲",
    ),
    (
        "bring_terminal_to_front",
        "terminal.focus",
        Side::Both,
        Own::Front,
        "拉前会话的终端窗口（本机后端答过它在这台电脑上的哪儿）",
    ),
    (
        "front_missed",
        "terminal.focus",
        Side::Both,
        Own::Front,
        "↗ 出口那一跳没成的那一张，照 face_core::front 写（只写字，不碰后端）",
    ),
    (
        "open_local_terminal",
        "session.launch",
        Side::Local,
        Own::Window,
        "开一个本机终端窗口跑那一行（`ccm …` 由本机后端 `session-open` 出成品；这里不判不拼，只开窗）",
    ),
    (
        "open_terminal_window",
        "terminal.window",
        Side::Both,
        Own::Window,
        "开一个终端窗口跑交来的那一串（Windows：PowerShell 窗口；Linux：挑到的终端）；不拼 ssh、不判命令",
    ),
    (
        "terminal_choices",
        "terminal.window",
        Side::Both,
        Own::Window,
        "开窗用哪个终端（设置里那一格 · 本机探到的终端）：窗口开在 monitor 面前这台，挑终端是开窗那一侧的事",
    ),
    (
        "footprint_client_facts",
        "audit.monitor-own",
        Side::Local,
        Own::Footprint,
        "足迹里 monitor 自己那台那几行（`HostScope::Client`）只有 monitor 知道的事实：它自己进程的家目录 · agent 家 · PATH（stat 与判定在本机后端 `footprint-report`）",
    ),
    (
        "open_file_window",
        "sftp.file-panel",
        Side::Remote,
        Own::Channel,
        "起文件窗口进程、把它的 stdin / stdout 接进通道（窗口的 call 经通道到后端）、读它那一行就绪 / 原话（第一屏由窗口进程经通道自己列）",
    ),
    (
        "deploy_remote_backend",
        "backend.deploy",
        Side::Remote,
        Own::Place,
        "把 monitor 带着的那几格后端字节交本机后端 `deploy-install`（判定 · 放字节 · 换进落点都在后端）",
    ),
    (
        "uninstall_remote_backend",
        "backend.deploy",
        Side::Remote,
        Own::Place,
        "删落点那一份后端字节（交本机后端 `deploy-uninstall`）",
    ),
    (
        "list_remote_mcp_origins",
        "mcp.list-origins",
        Side::Remote,
        Own::Config,
        "已配置且启用的远端标签：读的是 monitor 自己的配置（名字里的 mcp 是第一个用户留下的）",
    ),
    (
        "chan_call",
        "comm.face-a.call",
        Side::Both,
        Own::Channel,
        "通信层面 A：webview 说 `call` 的那一跳",
    ),
    (
        "chan_offer",
        "comm.face-a.call",
        Side::Both,
        Own::Channel,
        "通信层面 A：`subscribe` 的续订",
    ),
    (
        "chan_subscribe",
        "comm.face-a.subscribe",
        Side::Both,
        Own::Channel,
        "通信层面 A：`subscribe`",
    ),
    (
        "watch_sessions",
        "comm.face-a.subscribe",
        Side::Both,
        Own::Replay,
        "主窗口在看的会话记进会话流句柄那张表（与查看窗的订阅合成每台的「在看」名单）",
    ),
    (
        "kept_sessions",
        "comm.face-a.subscribe",
        Side::Both,
        Own::Replay,
        "主窗口手上的 tab 记进会话流句柄那张表（与留存合成每台手上留着的，随名单报给那台）",
    ),
    (
        "chan_want",
        "comm.face-a.subscribe",
        Side::Both,
        Own::Channel,
        "通信层面 A：credit",
    ),
    (
        "chan_stop",
        "comm.face-a.subscribe",
        Side::Both,
        Own::Channel,
        "通信层面 A：撤订",
    ),
];

/// **不对称能力的理由**：每一条都是天然不对称（本地不需要这项能力，不是欠账）。键集合必须**恰好等于**从 `LEDGER` 算出来的不对称集合。
const ASYMMETRY_REASONS: &[(&str, &str)] = &[
    ("session.launch", "远端那一侧不是一条 Tauri 命令：界面只交那台的名字、经通道直问本机后端 `terminal-ssh` 渲那一行，开窗走同一条 `open_terminal_window`；本机这一侧要带起始目录，才有 `open_local_terminal`。"),
    ("backend.deploy", "本机也要后端（入方向通道、每台机开关、tmux 帧都靠它），而且会自部署 —— `local_backend.rs::extract_embedded_to`（exe 旁没有本机后端就把内嵌那份释放到 `~/.cc-monitor/bin`）。不对称只在一格：**本机那次释放不经一条 IPC 命令**，是宿主启动时自己做的（`lib.rs` 的启动段），所以命令面上没有本机对侧。天然不对称说的是「不需要一条命令」，不是「不需要后端」。"),
    ("mcp.list-origins", "`list_remote_mcp_origins` 答的是「哪几台远端有 MCP 配置」——「有哪些 origin」这个问题在本机侧退化成一台，没有可列的集合。⚠ 注意它与 `backend_machines` 不同：那条**包含**本机（`cmd_wire::LOCAL`），因为它答的是「哪几台有后端」而本机也有。"),
    ("sftp.file-panel", "§40 天然不对称白名单第 1 条：本地有操作系统的文件管理器，不需要它。"),
    ("audit.monitor-own", "足迹里 **monitor 自己那台**那几行（`HostScope::Client`：本机 `ccm` 入口 · 终端 · shell profile …）的事实：monitor 只在一台机器上跑，远端那台没有 monitor，也就没有「它自己那几行」可答 —— 远端那一栏由那台后端自己答（`footprint-report` 不带 `client`，那一族不进人群）。不是欠一条对侧命令。"),
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
/// - 但 `local_or_both_commands_take_no_remote_only_parameter` 的 `checked == 68` **会红**
///   —— 前提是漏掉的那条恰好是 `Local`/`Both`。漏掉纯 `Remote` 的命令则两条都不响。
///
/// 同型 bug 在 `src/backend/no_timer_guard.rs` 同轮修掉。
fn command_signatures() -> BTreeMap<String, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let attr = format!("#[tauri::{}]", "command");
    let mut out = BTreeMap::new();
    // 扫的是 `src/frontend/shell/src`（`scan_tree!` 收整棵树）；判据住 `tests/frontend/shell/`，两棵树不相交，
    // 判据散文里的 `#[tauri::command]` 字样读不进来。
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
    for (_, cap, side, ..) in LEDGER {
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
    // 正控（按符号认，不数个数）：一条今天确实注册着的命令解析得出来 —— 解析器坏了，下面两向相等会在空集上成立。
    assert!(
        registered.contains("diagnostics_report"),
        "从 generate_handler! 解析不出 `diagnostics_report`（共解析出 {} 条）—— 解析器坏了（或那条命令真退役了：换一条点名）",
        registered.len()
    );
    let declared: BTreeSet<String> = LEDGER.iter().map(|(c, ..)| c.to_string()).collect();
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
    let reasoned: BTreeSet<&str> = ASYMMETRY_REASONS.iter().map(|(c, _)| *c).collect();
    assert_eq!(
        reasoned.len(),
        ASYMMETRY_REASONS.len(),
        "理由表里有重复的能力名"
    );
    let no_reason: Vec<_> = asym.difference(&reasoned).collect();
    assert!(
        no_reason.is_empty(),
        "这些能力只有一侧，却没写为什么：{no_reason:?}\n\
             要么把缺的那侧补上，要么到 ASYMMETRY_REASONS 里说清为什么它天然不对称。"
    );
    let rotten: Vec<_> = reasoned.difference(&asym).collect();
    assert!(
        rotten.is_empty(),
        "这些能力已经两侧都有了，理由却还留在表里（表在腐烂）：{rotten:?}"
    );
    for (cap, why) in ASYMMETRY_REASONS {
        assert!(
            why.chars().count() > 20,
            "{cap} 的理由太短，说不清为什么它只有一侧"
        );
    }
}

/// ★★ **P2 之后 `origin` 不再是「远端专用」** —— 本条因此从一刀切禁改成**登记制**。
///
/// `cmd_wire::LOCAL`（`"<local>"`）让本机也成为一个 origin，
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
        "open_terminal_in_new_window",
        "开单独终端窗口：`origin` 只随 URL 交给那扇窗（它自己问那台抓屏 · 订画面要寻址键），\
             本机 `<local>` 与远端同一个处置；命令体不做远端假设。",
    ),
    (
        "open_session_in_new_window",
        "开独立窗口：`origin` 只随 URL 交给那个窗口（它自己订 `session-lines/<sid>` 要寻址键），\
             本机 `<local>` 与远端同一个处置；命令体不做远端假设。",
    ),
    (
        "relay_endpoint_for_launch",
        "这次拉起的中转地址按**那台机器**的事实答：本机两件事走起会话那一侧那条缝，\
             远端问那台后端的成品（`launch-endpoint`，中转在不在也是它答）。命令体对 origin 不做远端假设 —— \
             分派住 `history::relay_endpoint_on`，判断只在 `payload::relay_endpoint_for` 一处。",
    ),
    // 下面三条各是本机一条、远端一条合成的一条带 origin 参数的命令，
    //    逐条写「凭什么说这一对是同一件事」（**判据不是名字**：按名字数分叉会系统性高估）。
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
    // 手抄的分解式会与 `checked` 各自漂，一红就把人指到不存在的数上。
    let mut n_local = 0usize;
    let mut n_both = 0usize;
    for (cmd, _, side, ..) in LEDGER {
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
    // 反向自检：一条都没检到 = 签名采集坏了。**等号而不是 `>=`**：`>= N` 恰好容忍一次静默降级。
    //
    // ★★ **报文里的每一个数都从这个常量或现算量渲染出来，一个手抄的都没有**：
    //   断言的数跟着代码走、手抄的数停在写下那一刻，两者之间没有东西钉住它们相等 ——
    //   一红，人照报文去查的是一个不存在的事实。⇒ 只有一个值：改 `EXPECTED_LOCAL_OR_BOTH`，
    //   报文里印出来的数**结构上不可能不跟着变**。
    //
    // `LEDGER` 里一行进出 `Local` / `Both`（新增、删掉、`Side` 改了），这个数就跟着变；
    //   同一条命令两侧收成一条路（`Remote` 改成 `Both`）也算，命令总数不动、这个数照样变。
    const EXPECTED_LOCAL_OR_BOTH: usize = 38;
    assert_eq!(
        checked, EXPECTED_LOCAL_OR_BOTH,
        "检到 {checked} 条 Local/Both 命令（Local {n_local} + Both {n_both}），\
             而本条期望 {EXPECTED_LOCAL_OR_BOTH} 条。\n\
             改 LEDGER 就要来确认这个数：把 `EXPECTED_LOCAL_OR_BOTH` 改成新值。"
    );
}

/// ★★ **每条不对称理由都得**可追问**。**
///
/// 假理由有**两种假法**：
/// **A 过期**（写下时是真的，事实变了理由没跟）· **B 从未成立**（写下时就没验证过）。
/// 按时间线扫只扫得到 A；B 读着有道理，扫不出来，只有去量才知道。
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
    for (cap, why) in ASYMMETRY_REASONS {
        let has_code_anchor = why.matches('`').count() >= 2;
        let has_section = why.contains('§');
        let has_issue = why.contains('#');
        if !(has_code_anchor || has_section || has_issue) {
            bare.push(format!("  {cap}"));
        }
    }
    // 完备性自检：人群空了「全过」与「没测」长得一样。
    assert!(
        !ASYMMETRY_REASONS.is_empty(),
        "一条不对称理由都没有 —— 本条此刻无效"
    );
    assert!(
        bare.is_empty(),
        "这些不对称理由**没有任何可以去核的东西**（无代码锚 / 无 § 段号 / 无 issue 号）：\n{}\n\n\
             ⚠ 本条不判理由真假，只判它**可不可追问**。\n\
             「读着有道理但无从复核」正是 B 类假理由（从未成立）的共同外观 ——\n\
             例：`launch.render-cli` 的「本地渲染必须在目标机器上做」，\
             量一次 `bash -lic` 就看得出本地渲染比远端还便宜。",
        bare.join("\n")
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 **`Side` 这一栏靠人签字，而签字的触发是派生出来的**
// ════════════════════════════════════════════════════════════════════════
//
// # 防的是什么
//
// `LEDGER` 的 `Side` 栏本身没有机检。一条命令的派发跳改走了「origin 无关」的
// 帧面（`inbound_client::local_client()` / `backend_route`）之后，它对
// `<local>` 也走得通，而这一行还写着 `Side::Remote` —— **表在撒谎，而且不红**。
//
// # 为什么**不是**「让 `Side` 从源码派生」（甲），而是「签字 ＋ 派生的触发器」（乙）
//
// 甲在这张表上**不成立**：
//
// 1. **本模块头注自己裁过**：「这项能力在两侧都有吗」是**判断**，不是能从代码推出来的
//    东西（没有任何语法说明「本地不需要这项」）。
//    同一段还写着**反向那条刻意没做**（`Remote` ⇒ 必须吃远端参数），合法例外不少。
// 2. **正向也会假阳**：一条命令派生出来是「帧面·origin 无关」，而它的 `Side::Remote`
//    可以不说假话 —— 本机那一侧另有一条命令。所以候选要人签，机器不直接判红。
//
// ⇒ 落 **乙**：这一栏**明写靠人签字**，而**「什么时候必须回来签」由机器算**：
//   · **没签字红** —— 一条 `Side::Remote` 的行不在签字表里（新增的、或改成 `Remote` 的）；
//   · **过期红**   —— 签的那个派发类与**今天现打的**对不上（源码改了、没回来重签）；
//   · **陈账红**   —— 签字表里有一行今天已经不是 `Side::Remote` 了（`Side` 被改过）。
//
// # ⚠ 它买不到什么（诚实边界，写死）
//
// - **不判 `Side` 对不对**。它买的是「派发跳一变，签字当场作废」，
//   不是「表说的是真话」；签着「说假话·欠一次订正」的行见 `FRAME_PLANE_VERDICTS`。
// - **射程只有 `Side::Remote` 那一栏**（各档几行见 `REMOTE_SIDE_SIGNOFF`）。`Local` ↔ `Both`
//   之间改来改去本条看不见 —— 理由是上面第 2 条：那个方向的派生假阳太多。
// - **派生器只跟同一份文件里的调用，深度 2**。跨文件的 helper 看不见 ⇒ 落 `Unclassified`。
//   `Unclassified` **不是「安全」**，是「这把尺子够不着」。
// - 它**不证明**「本机真的抓得到一屏 / 真的杀得掉」—— 那要跑真机。
//   它只证明**这一跳走的是哪条路**。

/// 一条命令的**派发跳**是什么形状 —— 从源码派生，不是人写的。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Derived {
    /// 生产段里出现了**只有远端才用得上**的东西（SSH / 远端配置 / 拒绝 `<local>`）。
    RemoteOnly,
    /// 只走 **origin 无关的帧面**（`local_client()` / 共用分流器）⇒ `<local>` 也走得通。
    FramePlane,
    /// 两样都有 —— 它自己分了两条路。本条不判它（人签）。
    Mixed,
    /// 两样都没有 ⇒ **这把尺子够不着**，不是「它安全」。
    Unclassified,
}

/// 一条候选（`Side::Remote` ＋ 派生 `FramePlane`）的**人裁**。闭集。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FrameVerdict {
    /// 🔴 **说假话**：本机没有别的命令名，而这一跳对 `<local>` 也走得通 —— 欠一次订正。
    LiesTodayOwedACorrection,
}

/// 生产段里「只有远端才用得上」的标记。运行时不拼 —— 本文件整份住在 `#[cfg(test)]` 里，
/// `production_code` 会把它整份剥掉；加上**住址**（本文件住 `tests/frontend/shell/`，扫的是
/// `src/frontend/shell/src`），扫描面里根本没有本文件。
///
/// ⚠ `scan_tree!` 的自摘在这一处恒空转，别把它算成一道保险。
const REMOTE_ONLY_MARKS: &[&str] = &[
    "machine_table::has(",
    "stream_source::",
    "refuse_local_write(",
    "transfer_relay::",
];

/// 生产段里「走 origin 无关帧面」的标记。
const FRAME_PLANE_MARKS: &[&str] = &[
    "local_client(",
    "offer_of(",
    "route_call_error",
    "backend_route::",
];

/// 派生器跟同文件调用的**深度**：2 / 3 / 4 三个深度认出的 `FramePlane` 逐字同一批
/// ⇒ 取最小的那个，别多扫。
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
/// 可见性那一截走 `guard_core::strip_visibility`：
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
/// 🔴 **为什么是直方图，不是一行一条的命令清单**：一行一条要把命令名再抄一遍，
/// 而那份抄写**会把一条现行棘轮顶红** —— `tool_registry` 那张旧名账按**文件**数
/// 「旧名的符号形出现几处」（`backend.deploy` 那两行的命令名里就带着它），
/// 抄一遍就翻倍；而那条账逐字禁「把上限调上去让今天好过」。
/// ⇒ 签的是**形状**：`Side::Remote` 那一栏里，每一档派生类各几行。
///
/// **它照样拦得住「改一行 `Side`」**：把一行 `Remote` 改掉 ⇒ 那一档少一个；
/// 把一行 `Local`/`Both` 改成 `Remote` ⇒ 某一档多一个。两边都不等 ⇒ 红。
/// ⚠ **它拦不住的那一形写死**：同一拍里一进一出、且**恰好同一档**
/// （改走一行 `RemoteOnly`、同时另加一行 `RemoteOnly`）⇒ 直方图不动，本条静默。
/// 候选那一档（`FramePlane`）**不吃这个亏** —— 它另有一张逐条点名的人裁表在对拍。
///
/// 每格的数是**跑出来的**：改了某条命令的 `Side` 或体之后，先让 `hist == want` 那一比印出现打的直方图，照它写，
/// 不按增减去算（算出来恰好相等不是判据）。
const REMOTE_SIDE_SIGNOFF: &[(Derived, usize)] = &[
    // 壳里一台远端只剩名字，装 · 卸先查 `machine_table::has`（`deploy.rs::remote_in_table`）。
    (Derived::RemoteOnly, 1),
    (Derived::FramePlane, 0),
    (Derived::Mixed, 1), // `deploy_remote_backend`：两支都走，不是失败退回
    (Derived::Unclassified, 2),
];

/// 派生器在**整张** `LEDGER` 上认出的 `FramePlane` 命令数（不分 `Side`）—— 候选集（`Remote` 那一栏里的
/// `FramePlane`）今天是空集，这个数是派生器不瞎的异源正控（见 `the_remote_side_column_is_signed_off` 第 ③ 步）。
/// 两向相等（逐条点名，跑出来的）；人群：体里走 `local_client(` / `backend_route::` 的 `#[tauri::command]`。
const FRAME_PLANE_ANYWHERE: &[&str] = &[
    // `LEDGER` 里是 `Both`（那台的通道在不在，体里走 `offer_of(`）—— 不是候选，因为它不在 `Remote` 那一栏。
    "backend_status",
    // `LEDGER` 里是 `Both`（各台此刻的成品问本机后端，体里走 `local_client(`）。
    "diagnostics_report",
];

/// **候选（派生 = `FramePlane`）的人裁** —— 闭集见 [`FrameVerdict`]，理由必须可追问。
///
/// 今天是空表：`Side::Remote` 那一栏里没有一条走 origin 无关的帧面派发。
/// 一条命令的 `ASYMMETRY_REASONS` 散文说「本机已通」、而同一行的 `Side` 栏还写 `Remote`，
/// 就是这里要登的一行 —— 散文与 `Side` 栏打架，别的东西都不数它。
const FRAME_PLANE_VERDICTS: &[(&str, FrameVerdict, &str)] = &[];

/// 🔴 **`Side::Remote` 那一栏，没签字红 · 过期红 · 陈账红。**
///
/// 三条判定与它们各自拦得住什么，逐条写在断言的报文里；边界见本节开头那段头注。
#[test]
fn the_remote_side_column_is_signed_off() {
    let derived = command_dispatch_class();
    // 反向自检①：派生器扫不到东西的时候，下面每一条都会**空真地**成立。
    //    地板只守「扫描面没塌」：`#[tauri::command]` 真的删了，就照现打的数把地板改低。
    assert!(
        derived.len() >= 39,
        "派生器只认出 {} 条命令的派发跳（`LEDGER` 现打 {} 行）—— **扫描面塌了**，\n\
             不是「命令变少了」。先修 `command_dispatch_class`，别信下面任何一条绿。",
        derived.len(),
        LEDGER.len()
    );

    let remote_rows: BTreeSet<&str> = LEDGER
        .iter()
        .filter(|(_, _, s, ..)| *s == Side::Remote)
        .map(|(c, ..)| *c)
        .collect();
    // 反向自检②：`Side::Remote` 那一栏本身不许塌成空集。
    //    ⚠ 地板只守「这一栏没塌成空集」；**有牙的是下面那条直方图相等**，不是这个数。
    //    人群真的变少（一条 `Side::Remote` 命令删了、或并进带 origin 的 `Both`），照现打的数把地板改低。
    assert!(
        remote_rows.len() >= 4,
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
                 这一种最要紧：不拦它，账本会一直说着假话而不红。"
    );

    // ③ 候选集（派生 = `FramePlane`）必须**恰好等于**人裁表的键集。
    let judged: BTreeSet<&str> = FRAME_PLANE_VERDICTS.iter().map(|(c, _, _)| *c).collect();
    // 反向自检④：候选集空了 ⇒ 下面那条相等是空真（`[] == []` 照样成立）。
    //   候选集今天**真的是空集**（`Side::Remote` 里没有一条走 origin 无关的帧面派发），
    //   「没塌成空集」这条地板立不住，换成**异源的正控**：
    //   同一个派生器在**整张** `LEDGER` 上（不只 `Remote` 那一栏）认得出多少条 `FramePlane` —— 那一群是 `Both` / `Local`
    //   命令（`chan_call` 那一族经 `local_client(` 问后端），人群不空 ⇒ 派生器没瞎，下面那条「候选 == 人裁」的空集相等才可信。
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
