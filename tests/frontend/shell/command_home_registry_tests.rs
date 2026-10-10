//! 「Tauri 命令分两张封闭表「monitor 自己的事」（逐行理由）与「待迁」（逐行卡在哪），命令实现经 `inbound_client` / `BackendDoor` 碰到后端的必须在「待迁」」。
//!
//! 分母只数「前端对后端」的 IPC（产物来自某台后端的）；窗口 · 本机 monitor 配置 · 日志 · 拉前 · 本机后端的起停与引导
//! 是 monitor 自己的事，外加通信层面 A 客户端本身与「放字节」。
//!
//! 三条判据：
//! 1. 两表不相交、并集 == `lib.rs` 的 `generate_handler!`（两向）。
//! 2. 「monitor 自己的事」里不许碰后端的那几类（窗口 / 开终端 / 拉前 / 配置 / 日志 / 重放缓冲），
//!    实现传递可达 `client_for` / `BackendDoor` ⇒ 红（它该进「待迁」）。通道 · 起停 · 放字节三类碰后端就是它们的本分。
//! 3. 可达性量得出：真树上 `chan_call`（经 `chan::host` 跨文件到 `client_for`）必须被量到、`load_config` 必须量不到；
//!    合成夹具上「`use` 进来的函数 → 别的文件 → 类型的方法 → `BackendDoor`」必须量到。
//!
//! 可达性是**静态近似**：函数级，边按「同文件自由函数 · `use` 进来的名字 · `模块::函数` · `类型::方法` · 提到同文件 /
//! `use` 进来的类型 ⇒ 它的全部方法」解析；`.方法(` 不解析（量少不量多）。量少 ⇒ 判据 2 可能漏一条，不会冤枉一条。

use std::collections::{BTreeMap, BTreeSet};

/// monitor 自己的事分几类。`may_touch` 那三类碰后端是本分；其余碰了就是没迁干净。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Own {
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
    /// 放字节：照那台后端判好的计划往那台放 / 删 monitor 带着的字节（「monitor 只放字节」）。
    Place,
    /// 足迹里 monitor 自己那台那几行的**事实**（判定在后端）。
    Footprint,
}

impl Own {
    fn may_touch(self) -> bool {
        matches!(self, Own::Lifecycle | Own::Channel | Own::Place)
    }
}

// 「哪一路负责迁走它」那个闭集（`Lane`）收了：「待迁」最后一行 `launch_remote_terminal`〔散文墓碑〕随 ssh 外壳
//   进本机后端（帧命令 `terminal-ssh`）清掉，再没有没迁完的行 ⇒ 变体一个不剩、枚举本身删掉。「待迁」表留着（今天为空，
//   判据 1 仍两向：新长一条碰后端的 Tauri 命令 ⇒ 要么进「monitor 自己的事」写理由、要么进「待迁」写卡在哪）。

/// 「monitor 自己的事」：命令 · 哪一类 · 理由。
const MONITOR_OWN: &[(&str, Own, &str)] = &[
    (
        "backend_machines",
        Own::Config,
        "机器表 = monitor 配置里登记的远端 ＋ 本机，读的是 monitor 自己的注册表",
    ),
    (
        "backend_status",
        Own::Lifecycle,
        "这台后端通道在不在 ＋ 本机后端 pid / 重启次数 —— 起停引导那一格的读面",
    ),
    (
        "backend_start",
        Own::Lifecycle,
        "起本机后端（自释放 ＋ 监护）/ 起远端常驻",
    ),
    (
        "backend_stop",
        Own::Lifecycle,
        "停后端（SIGTERM → 等 → 超时才 SIGKILL）",
    ),
    // 开终端：monitor 只开窗，交来的是成品（远端那一行由本机后端 `terminal-ssh` 渲）。
    (
        "open_terminal_window",
        Own::Window,
        "开一个终端窗口跑交来的那一串（Windows：PowerShell 窗口；Linux：挑到的终端）；不拼 ssh、不判命令",
    ),
    (
        "terminal_choices",
        Own::Window,
        "开窗用哪个终端（设置里那一格 · 本机探到的终端）：窗口开在 monitor 面前这台，挑终端是开窗那一侧的事",
    ),
    (
        "terminal_dial",
        Own::Config,
        "交开终端那一问要的机器事实（monitor 的机器表 ＋ 上次赢的那条，`dial_host::machine_facts`）；组请求与渲染在本机后端",
    ),
    ("load_config", Own::Config, "本机 monitor 配置"),
    ("patch_config", Own::Config, "本机 monitor 配置"),
    ("machine_table_try", Own::Config, "本机 monitor 配置（机器表试算，不写盘）"),
    (
        "cc_get_auto_launch",
        Own::Config,
        "`auto-launch.json` 是 monitor 自己写的小文件",
    ),
    (
        "cc_set_auto_launch",
        Own::Config,
        "`auto-launch.json` 是 monitor 自己写的小文件",
    ),
    ("open_session_in_new_window", Own::Window, "开窗"),
    ("open_settings_window", Own::Window, "开窗"),
    (
        "remote_reconcile",
        Own::Lifecycle,
        "机器表热加载：照 monitor 自己的机器表起 / 断 / 重起那几条远端流（流是 monitor 进程里的任务）",
    ),
    (
        "open_file_window",
        Own::Channel,
        "起文件窗口进程、把它的 stdin / stdout 接进通道（窗口的 call 经通道到后端）、读它那一行就绪 / 原话（第一屏由窗口进程经通道自己列）",
    ),
    ("bring_monitor_to_front", Own::Window, "拉前 monitor 自己"),
    ("bring_terminal_to_front", Own::Front, "拉前本机终端窗口"),
    (
        "bring_remote_terminal_to_front",
        Own::Front,
        "拉前那条远端会话对应的本机终端窗口",
    ),
    (
        "forget_session",
        Own::Replay,
        "只丢 monitor 进程内那条会话的重放缓冲",
    ),
    ("frontend_perf_log", Own::Log, "日志"),
    ("get_diagnostics_config", Own::Log, "诊断开关"),
    ("set_diagnostics_config", Own::Log, "诊断开关"),
    ("get_log_file_info", Own::Log, "日志"),
    (
        "diagnostics_report",
        Own::Lifecycle,
        "日志页「复制诊断信息」那一段：各台此刻的状态成品（读这台后端通道在不在，与 `backend_status` 同一个读面）\
         ＋ 界面交来的记录账 ＋ 日志位置，只读、不经后端发东西",
    ),
    ("open_log_file", Own::Log, "日志"),
    (
        "notify_desktop",
        Own::Window,
        "系统通知（「一轮完成」「需手动」）：通知出在 monitor 面前这台的桌面上，界面判要不要发，壳只发（`platform/notify.rs`）",
    ),
    (
        "clipboard_write",
        Own::Window,
        "写系统剪贴板（全产品的复制）：剪贴板是 monitor 面前这台桌面的，经系统接口写、回真成败（`clipboard.rs`）",
    ),
    (
        "restart_app",
        Own::Lifecycle,
        "设置窗「现在重启」：重起 cc-monitor 自己（走退出臂，本机后端照它自己那份退出行为去留）",
    ),
    ("open_log_dir", Own::Log, "日志"),
    (
        "drift_ledger_report",
        Own::Log,
        "漂移账里 monitor 天生观测的两面（未登记的会话 kind · 后端 hello 里不认识的能力 token）—— 诊断；\
         记录那两面随解析进了那台后端（`drift-report`，界面经通道问）",
    ),
    (
        "list_remote_mcp_origins",
        Own::Config,
        "已配置且启用的远端标签：读的是 monitor 自己的配置（名字里的 mcp 是第一个用户留下的）",
    ),
    (
        "get_data_paths",
        Own::Log,
        "设置页「数据」区列 monitor 自己的持久路径",
    ),
    (
        "local_ccm_entry_status",
        Own::Lifecycle,
        "本机 `ccm` 入口（本机后端的引导那一格）",
    ),
    (
        "ccm_user_path_status",
        Own::Lifecycle,
        "本机 `~/.cc-monitor/bin` 在不在用户级 PATH（本机后端的引导）",
    ),
    ("ccm_user_path_add", Own::Lifecycle, "同上：加"),
    (
        "cc_bus_ccm_precheck",
        Own::Lifecycle,
        "装 cc-bus 之前问一次本机 `ccm` 够不够新（本机后端的引导那一族：探本机 ccm 入口）",
    ),
    ("ccm_user_path_remove", Own::Lifecycle, "同上：撤"),
    (
        "chan_call",
        Own::Channel,
        "通信层面 A：webview 说 `call` 的那一跳",
    ),
    ("chan_offer", Own::Channel, "通信层面 A：`subscribe` 的续订"),
    ("chan_subscribe", Own::Channel, "通信层面 A：`subscribe`"),
    ("chan_want", Own::Channel, "通信层面 A：credit"),
    ("chan_stop", Own::Channel, "通信层面 A：撤订"),
    (
        "chan_cancel",
        Own::Channel,
        "通信层面 A：撤掉 webview 那一跳上带编号的一问（撤单过这一跳）",
    ),
    // 起会话的那一行进了后端之后，monitor 在这件事上只剩开窗这一格。
    (
        "open_local_terminal",
        Own::Window,
        "开一个本机终端窗口跑那一行（`ccm …` 由本机后端 `launch-local` 出成品；这里不判不拼，只开窗）",
    ),
    // 放字节：判定（该不该换 · 换成哪一格 · 落点那一份是谁）住本机常驻后端 `deploy-plan`。
    (
        "deploy_remote_backend",
        Own::Place,
        "照本机后端 `deploy-plan` 的计划放 monitor 带着的那一格后端字节（经 `files` 链路），并照计划删旧落点那一份",
    ),
    (
        "uninstall_remote_backend",
        Own::Place,
        "删落点那一份后端字节（固定落点，没有判定；经 `files` 链路）",
    ),
    (
        "footprint_client_facts",
        Own::Footprint,
        "足迹里 monitor 自己那台那几行（`HostScope::Client`）只有 monitor 知道的事实：它自己进程的家目录 · agent 家 · PATH（stat 与判定在本机后端 `footprint-report`）",
    ),
];

/// 「待迁」：命令 · 卡在哪。今天为空（「哪一路」那一格随 `Lane` 一起收了）。
const PENDING: &[(&str, &str)] = &[
    // MIG-1：会话 / tmux 账本 ＋ ssh 配置解读进后端。
    // `~/.ssh/config` 导入那三条（别名 · `ssh -G` · 批量）迁走了：本机常驻后端帧命令 `ssh-config-*`（⑯）。
    // 测试连接迁走了：界面把表单那一台交给本机后端（`remote-probe`），后端组请求、拨一次、回结局。
    // 本机活会话表那两条（红绿灯快照 · 骨架清单）迁走了：会话账本进后端，骨架与灯是会话流里的 `live` / `activity` 成品（⑬）。
    // 列 tmux 会话两条（本机 · 远端）迁走了：那台后端的 `tmux-list` 出成品，界面经通道直问（`src/frontend/ui/tmux-reads.ts`）。
    // MIG-2：本机起会话 ＋ 载荷渲染 ＋ 历史查看器。迁走七条：`new_local_session` · `resume_history_session` ·
    //   `render_local_attach` · `render_ccm_launch` · `render_launch_payload` · `relay_endpoint_for_launch` · 远端 `ccm` 探针。
    // `launch_remote_terminal`〔散文墓碑〕迁走了：远端那一行（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）由本机后端
    //   `terminal-ssh` 渲（组请求走 `dial/machine.rs::resolve`），monitor 剩开窗 `open_terminal_window` 与交机器事实 `terminal_dial`
    //   两条，进「monitor 自己的事」。「待迁」从此为空。
    // MIG-3a：资产与 D 组。
    // 别名六条（`aliases_*`）已迁：规则 · 方言 · 围栏进了那台后端（`aliases-*`），从本表删。
    // `deploy_remote_acct_iso`〔散文墓碑〕 已迁：字节随后端二进制走（部署载荷只一种走法），装 · 链接 · 配置 · 记账
    //   全在那台后端（`acct-iso-install`），界面只问它一次 ⇒ 从本表删（命令本身也删了）。
    // cc-bus 装 · 三态那两条 Tauri 命令已迁：进了那台后端（`cc-bus-install` / `-state`，今天只经扩展页的枢纽问）。
    // MIG-3b：部署决策 · 诊断 · 足迹 · 删会话 / 分叉。钩子诊断两条 · 删会话 · 分叉已迁（界面经通道直说那台后端），行删了；
    //   部署后端 · 卸载后端两条挪进「monitor 自己的事」（放字节：判定进了本机常驻后端 `deploy-plan`）。
    // `config_surface_report`〔散文墓碑〕已迁：申报表 ＋ 判定进了后端（`footprint-report`，界面经通道直问），
    //   monitor 只答它自己那台那几行的事实（`footprint_client_facts`，进「monitor 自己的事」）。
    // 漂移账那一条迁了：两路进料（未知记录类型 · 已知类型解析失败）随记录解释进了那台后端（`drift-report`）；
    //   monitor 天生观测的那一面（hello 里不认识的能力 token）留在 `drift_ledger_report`，进「monitor 自己的事」（日志）。
    // 会话正文四条（查看器整份读那一条 · `load_subagent` · `read_session_range` ·
    //   `read_session_lines`〔散文墓碑〕）迁了：记录解释进了后端，界面经通道直问那台后端（`history-page` · `history-subagent` ·
    //   `history-lines`，`src/frontend/ui/record-reads.ts`）。
    // 原先 C 段没人点名的那几行已指派（MIG-1 端口转发 · MIG-2 会话读面 · MIG-3b 探针 / 公钥 / 全景 · MIG-3a 开文件窗）。
    // 公钥推送那一条已迁（界面经通道问本机后端 `pubkey-push`），行删了。
    // 端口转发三条（起 · 停 · 列）迁走：账住本机常驻后端（`dial/forwards.rs`），界面经通道问 `forward-*`。
    // 撤单那一条（`chan_cancel`）进 `Channel`。
    // `open_file_window` 已迁：开窗前那一屏（`files-home` / `files-ls`）进了窗口进程自己问，
    //   monitor 只起进程、读它那一行（`filewin/proc.rs::first_screen` · `Ready`）⇒ 从本表删，进 `MONITOR_OWN`（开窗）。
];

/// 通道上**由 monitor 自己接**、不按 `origin` 转给那台后端的 op（`chan/host.rs::HOST_OPS`）：
/// op · 哪一类 · 理由。它们不是 Tauri 命令，却是同一个问题（「monitor 自己的事」还是「待迁」），照 `MONITOR_OWN` 的写法两向登记。
const CHANNEL_OWN: &[(&str, Own, &str)] = &[
    (
        "transfer-upload",
        Own::Channel,
        "传输台开单：转给本机常驻后端的传输台（经中继 `sftp_pool.rs`），不按寻址去那台",
    ),
    (
        "transfer-download",
        Own::Channel,
        "同上（下载那一形）",
    ),
    (
        "terminal-open",
        Own::Window,
        "文件窗口「在此打开终端」只交意图 `{cwd}`：monitor 补机器事实 → 本机后端 `terminal-ssh` 渲那一行 → `open_terminal_window` 开窗（与主界面同一条路）",
    ),
    (
        "filewin-open",
        Own::Window,
        "文件窗口左栏「其他机器」点一台：窗口不起进程，monitor 按名字取那台的配置、照开窗入口同一条路另起一个窗口进程（一窗一机）",
    ),
    (
        "link-retry",
        Own::Window,
        "文件窗口断线条上「重新连接」：连接循环住 monitor（`stream_source::run`），叫醒它不等退避睡满；连没连上看 `link` 那条流",
    ),
    (
        "plan-open",
        Own::Window,
        "文件窗口「在计划里看」只交意图 `{workspace, slice, id}`：monitor 把主窗口拉到前面、发事件给它开计划页选中那一格（主窗口住 monitor）",
    ),
];

/// `HOST_OPS`（代码那一侧）== [`CHANNEL_OWN`]（登记那一侧），两向；每一行都写了理由。
#[test]
fn every_op_the_monitor_takes_off_the_channel_is_registered() {
    let code: BTreeSet<&str> = crate::chan::host::HOST_OPS.iter().copied().collect();
    let table: BTreeSet<&str> = CHANNEL_OWN.iter().map(|(op, _, _)| *op).collect();
    assert_eq!(
        table.len(),
        CHANNEL_OWN.len(),
        "`CHANNEL_OWN` 里有重复的 op"
    );
    assert_eq!(
        code, table,
        "通道上 monitor 自己接的 op（`chan/host.rs::HOST_OPS`）与登记表两向不等 —— 新截下一条就在这里写清它是哪一类、为什么不转给后端"
    );
    for (op, _, why) in CHANNEL_OWN {
        assert!(why.chars().count() >= 8, "`{op}` 的理由写得太短：{why:?}");
    }
}

// ---------------------------------------------------------------- 读 `generate_handler!`

/// `generate_handler!` 里每一条：命令名 → 它住的文件（`src/frontend/shell/src` 相对路径）。
fn registered(lib_rs: &str, files: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let start = lib_rs
        .find("generate_handler!")
        .expect("generate_handler! 不见了");
    let open = lib_rs[start..].find('[').expect("找不到 [") + start;
    let mut depth = 0usize;
    let mut end = open;
    for (i, c) in lib_rs[open..].char_indices() {
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
    let mut out = BTreeMap::new();
    for entry in guard_core::strip_comment_lines(&lib_rs[open + 1..end]).split(',') {
        let path: Vec<&str> = entry.trim().split("::").collect();
        if path == [""] {
            continue;
        }
        let (name, dirs) = path.split_last().unwrap();
        let file = if dirs.is_empty() {
            "lib.rs".to_string()
        } else {
            let base = dirs.join("/");
            [format!("{base}.rs"), format!("{base}/mod.rs")]
                .into_iter()
                .find(|f| files.contains_key(f))
                .unwrap_or_else(|| panic!("`{}` 的模块文件找不到", entry.trim()))
        };
        out.insert(name.to_string(), file);
    }
    out
}

// ---------------------------------------------------------------- 可达性（静态近似）

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Id(String),
    Path, // `::`
    Open, // `(`
    Dot,
    Other(char),
}

/// 剥字符串 / 字符字面量后切词（注释已由 `guard_core::production_code` 剥掉）。
fn tokens(src: &str) -> Vec<(Tok, usize)> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '"'
            || (c == 'r' && matches!(b.get(i + 1), Some('"') | Some('#')) && raw_start(&b, i))
        {
            i = skip_string(&b, i);
            continue;
        }
        if c == '\'' {
            // 字符字面量 `'x'` / `'\n'`；否则是生命周期。
            if b.get(i + 2) == Some(&'\'') {
                i += 3;
                continue;
            }
            if b.get(i + 1) == Some(&'\\') {
                let mut j = i + 2;
                while j < b.len() && b[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            out.push((Tok::Id(b[s..i].iter().collect()), s));
            continue;
        }
        if c == ':' && b.get(i + 1) == Some(&':') {
            out.push((Tok::Path, i));
            i += 2;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        out.push((
            match c {
                '(' => Tok::Open,
                '.' => Tok::Dot,
                o => Tok::Other(o),
            },
            i,
        ));
        i += 1;
    }
    out
}

fn raw_start(b: &[char], i: usize) -> bool {
    let prev_ident = i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_');
    let mut j = i + 1;
    while b.get(j) == Some(&'#') {
        j += 1;
    }
    !prev_ident && b.get(j) == Some(&'"')
}

fn skip_string(b: &[char], i: usize) -> usize {
    if b[i] == 'r' {
        let mut j = i + 1;
        let mut hashes = 0;
        while b[j] == '#' {
            hashes += 1;
            j += 1;
        }
        j += 1;
        while j < b.len() {
            if b[j] == '"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&'#')) {
                return j + 1 + hashes;
            }
            j += 1;
        }
        return b.len();
    }
    let mut j = i + 1;
    while j < b.len() && b[j] != '"' {
        if b[j] == '\\' {
            j += 1;
        }
        j += 1;
    }
    j + 1
}

struct Func {
    file: String,
    name: String,
    ty: Option<String>,
    toks: Vec<Tok>,
}

/// 文件 → 模块末段（`a/b.rs` → `b` · `a/mod.rs` → `a` · `lib.rs` → `crate`）。
fn stem(file: &str) -> String {
    let no_ext = file.trim_end_matches(".rs");
    let mut parts: Vec<&str> = no_ext.split('/').collect();
    if parts.last() == Some(&"mod") {
        parts.pop();
    }
    match parts.last() {
        Some(&"lib") | None => "crate".to_string(),
        Some(p) => p.to_string(),
    }
}

/// `use a::b::{c, d as e};` ⇒ 名字 → 它来自的模块末段。
fn uses_of(toks: &[(Tok, usize)]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i].0 == Tok::Id("use".into()) {
            let mut j = i + 1;
            let mut stack: Vec<Vec<String>> = vec![vec![]];
            let mut last: Option<String> = None;
            let mut prefix: Vec<String> = vec![];
            while j < toks.len() && toks[j].0 != Tok::Other(';') {
                match &toks[j].0 {
                    Tok::Id(s) if s == "as" => {
                        if let (Some(orig), Some(Tok::Id(alias))) =
                            (last.take(), toks.get(j + 1).map(|t| &t.0))
                        {
                            if let Some(m) = prefix.last() {
                                out.insert(alias.clone(), m.clone());
                            }
                            let _ = orig;
                            j += 1;
                        }
                    }
                    Tok::Id(s) => last = Some(s.clone()),
                    Tok::Path => {
                        if let Some(s) = last.take() {
                            prefix.push(s);
                        }
                    }
                    Tok::Other('{') => stack.push(prefix.clone()),
                    Tok::Other(',') | Tok::Other('}') => {
                        if let (Some(s), Some(m)) = (last.take(), prefix.last()) {
                            out.insert(s, m.clone());
                        }
                        prefix = stack.last().cloned().unwrap_or_default();
                        if toks[j].0 == Tok::Other('}') {
                            stack.pop();
                            prefix = stack.last().cloned().unwrap_or_default();
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if let (Some(s), Some(m)) = (last.take(), prefix.last()) {
                out.insert(s, m.clone());
            }
            i = j;
        }
        i += 1;
    }
    out
}

fn block_end(toks: &[(Tok, usize)], open: usize) -> usize {
    let mut depth = 0i32;
    for (k, (t, _)) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Other('{') => depth += 1,
            Tok::Other('}') => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    toks.len() - 1
}

/// 从某个 token 起，找到下一个 `{`（先撞上 `;` ⇒ 没有体）。
fn body_open(toks: &[(Tok, usize)], from: usize) -> Option<usize> {
    for (k, (t, _)) in toks.iter().enumerate().skip(from) {
        match t {
            Tok::Other('{') => return Some(k),
            Tok::Other(';') => return None,
            _ => {}
        }
    }
    None
}

struct Graph {
    funcs: Vec<Func>,
    by_name: BTreeMap<String, Vec<usize>>,
    by_type: BTreeMap<String, Vec<usize>>,
    uses: BTreeMap<String, BTreeMap<String, String>>,
    stems: BTreeMap<String, Vec<String>>,
}

impl Graph {
    fn build(files: &BTreeMap<String, String>) -> Self {
        let mut funcs = Vec::new();
        let mut uses = BTreeMap::new();
        let mut stems: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (file, raw) in files {
            stems.entry(stem(file)).or_default().push(file.clone());
            let toks = tokens(&guard_core::production_code(raw));
            uses.insert(file.clone(), uses_of(&toks));
            // `impl … [for] Type {` 的区间 → Type。
            let mut spans: Vec<(usize, usize, Option<String>)> = Vec::new();
            for (k, (t, _)) in toks.iter().enumerate() {
                if *t != Tok::Id("impl".into()) {
                    continue;
                }
                let Some(open) = body_open(&toks, k + 1) else {
                    continue;
                };
                let head = &toks[k + 1..open];
                let after_for = head
                    .iter()
                    .rposition(|(t, _)| *t == Tok::Id("for".into()))
                    .map_or(0, |p| p + 1);
                let ty = head[after_for..].iter().find_map(|(t, _)| match t {
                    Tok::Id(s) if s.starts_with(|c: char| c.is_ascii_uppercase()) => {
                        Some(s.clone())
                    }
                    _ => None,
                });
                spans.push((open, block_end(&toks, open), ty));
            }
            for (k, (t, _)) in toks.iter().enumerate() {
                if *t != Tok::Id("fn".into()) {
                    continue;
                }
                let Some((Tok::Id(name), _)) = toks.get(k + 1) else {
                    continue;
                };
                let Some(open) = body_open(&toks, k + 2) else {
                    continue;
                };
                let end = block_end(&toks, open);
                let ty = spans
                    .iter()
                    .filter(|(a, b, _)| *a < k && k < *b)
                    .max_by_key(|(a, _, _)| *a)
                    .and_then(|(_, _, t)| t.clone());
                funcs.push(Func {
                    file: file.clone(),
                    name: name.clone(),
                    ty,
                    toks: toks[open..=end].iter().map(|(t, _)| t.clone()).collect(),
                });
            }
        }
        let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_type: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, f) in funcs.iter().enumerate() {
            by_name.entry(f.name.clone()).or_default().push(i);
            if let Some(t) = &f.ty {
                by_type.entry(t.clone()).or_default().push(i);
            }
        }
        Graph {
            funcs,
            by_name,
            by_type,
            uses,
            stems,
        }
    }

    fn files_of(&self, seg: &str, here: &str) -> Vec<String> {
        match seg {
            "self" => vec![here.to_string()],
            "crate" => vec!["lib.rs".to_string()],
            s => self.stems.get(s).cloned().unwrap_or_default(),
        }
    }

    fn edges(&self, f: &Func) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        let uses = &self.uses[&f.file];
        let t = &f.toks;
        for k in 0..t.len() {
            let Tok::Id(name) = &t[k] else { continue };
            let qual = if k >= 2 && t[k - 1] == Tok::Path {
                match &t[k - 2] {
                    Tok::Id(q) => Some(q.as_str()),
                    _ => None,
                }
            } else {
                None
            };
            let after_dot = k >= 1 && t[k - 1] == Tok::Dot;
            // 调用：名字后面（可带 `::<…>`）是 `(`。
            let mut j = k + 1;
            if t.get(j) == Some(&Tok::Path) && t.get(j + 1) == Some(&Tok::Other('<')) {
                let mut d = 0;
                j += 1;
                while j < t.len() {
                    match t[j] {
                        Tok::Other('<') => d += 1,
                        Tok::Other('>') => {
                            d -= 1;
                            if d == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            let is_call = t.get(j) == Some(&Tok::Open);
            let is_type = name.starts_with(|c: char| c.is_ascii_uppercase());
            let named = self
                .by_name
                .get(name.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let typed = self
                .by_type
                .get(name.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let in_scope = |g: &Func, n: &str| {
                g.file == f.file
                    || uses
                        .get(n)
                        .is_some_and(|m| self.files_of(m, &f.file).contains(&g.file))
            };
            if is_call && !after_dot {
                for &idx in named {
                    let g = &self.funcs[idx];
                    let hit = match qual {
                        Some("Self") => g.ty == f.ty && g.file == f.file,
                        Some(q) if q.starts_with(|c: char| c.is_ascii_uppercase()) => {
                            g.ty.as_deref() == Some(q)
                        }
                        Some(q) => g.ty.is_none() && self.files_of(q, &f.file).contains(&g.file),
                        None => g.ty.is_none() && in_scope(g, name),
                    };
                    if hit {
                        out.insert(idx);
                    }
                }
            } else if is_type && !after_dot && f.ty.as_deref() != Some(name.as_str()) {
                // 提到一个类型 ⇒ 它的全部方法（trait 对象 / 泛型分派量不到调用点，只能按类型量）。
                for &idx in typed {
                    let g = &self.funcs[idx];
                    let hit = match qual {
                        Some(q) if !q.starts_with(|c: char| c.is_ascii_uppercase()) => {
                            self.files_of(q, &f.file).contains(&g.file)
                        }
                        _ => in_scope(g, name),
                    };
                    if hit {
                        out.insert(idx);
                    }
                }
            }
        }
        out
    }

    /// 每个函数：可达 `client_for` / `BackendDoor` 的那条链（函数名，由近及远）。
    fn reach(&self) -> BTreeMap<usize, Vec<String>> {
        let sink = |f: &Func| {
            f.toks
                .iter()
                .any(|t| matches!(t, Tok::Id(s) if s == "client_for" || s == "BackendDoor"))
        };
        let edges: Vec<BTreeSet<usize>> = self.funcs.iter().map(|f| self.edges(f)).collect();
        let mut chain: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (i, f) in self.funcs.iter().enumerate() {
            if sink(f) {
                chain.insert(i, vec![format!("{}::{}", stem(&f.file), f.name)]);
            }
        }
        loop {
            let mut grew = false;
            for (i, es) in edges.iter().enumerate() {
                if chain.contains_key(&i) {
                    continue;
                }
                if let Some(next) = es.iter().find(|e| chain.contains_key(e)) {
                    let mut c = vec![format!(
                        "{}::{}",
                        stem(&self.funcs[i].file),
                        self.funcs[i].name
                    )];
                    c.extend(chain[next].iter().cloned());
                    chain.insert(i, c);
                    grew = true;
                }
            }
            if !grew {
                return chain;
            }
        }
    }

    /// 命令（住在 `file` 的自由函数 `name`）够得着后端 ⇒ 那条链。
    fn touching(&self, commands: &BTreeMap<String, String>) -> BTreeMap<String, Vec<String>> {
        let reach = self.reach();
        let mut out = BTreeMap::new();
        for (cmd, file) in commands {
            let hit = self.funcs.iter().enumerate().find(|(i, f)| {
                f.name == *cmd && f.file == *file && f.ty.is_none() && reach.contains_key(i)
            });
            if let Some((i, _)) = hit {
                out.insert(cmd.clone(), reach[&i].clone());
            }
        }
        out
    }
}

fn monitor_tree() -> BTreeMap<String, String> {
    let root = crate::guard_support::crate_src_root();
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, s)| {
            // 按模块住址认：人群声明带进来的兄弟包（通信层 `comms-inward` 等）认作 `<包名>/…`。
            let rel = guard_core::module_address(&root, &p);
            (rel, s)
        })
        .collect()
}

// ---------------------------------------------------------------- 判据

#[test]
fn the_two_tables_are_disjoint_and_cover_every_tauri_command() {
    let files = monitor_tree();
    let reg: BTreeSet<String> = registered(&files["lib.rs"], &files).into_keys().collect();
    let own: BTreeSet<String> = MONITOR_OWN.iter().map(|(c, _, _)| c.to_string()).collect();
    let pending: BTreeSet<String> = PENDING.iter().map(|(c, _)| c.to_string()).collect();
    assert_eq!(own.len(), MONITOR_OWN.len(), "「monitor 自己的事」有重行");
    assert_eq!(pending.len(), PENDING.len(), "「待迁」有重行");
    let both: Vec<_> = own.intersection(&pending).collect();
    assert!(both.is_empty(), "同一条命令两张表都有：{both:?}");
    let listed: BTreeSet<String> = own.union(&pending).cloned().collect();
    let missing: Vec<_> = reg.difference(&listed).collect();
    let stale: Vec<_> = listed.difference(&reg).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "`generate_handler!` 与两表不等：注册了没登记 {missing:?} · 登记了已不在 {stale:?}\n\
         迁走一条 ⇒ 从「待迁」删；新加一条 ⇒ 写进其中一张并写理由"
    );
    let reasons = MONITOR_OWN
        .iter()
        .map(|(c, _, w)| (c, w))
        .chain(PENDING.iter().map(|(c, w)| (c, w)));
    for (c, why) in reasons {
        assert!(!why.trim().is_empty(), "`{c}` 那一行没写理由");
    }
}

#[test]
fn a_monitor_own_command_that_reaches_a_backend_belongs_in_pending() {
    let files = monitor_tree();
    let reg = registered(&files["lib.rs"], &files);
    let touching = Graph::build(&files).touching(&reg);
    let wrong: Vec<String> = MONITOR_OWN
        .iter()
        .filter(|(_, kind, _)| !kind.may_touch())
        .filter_map(|(c, kind, _)| {
            touching
                .get(*c)
                .map(|chain| format!("{c}（{kind:?}）：{}", chain.join(" → ")))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "这几条记成 monitor 自己的事，实现却经 `client_for` / `BackendDoor` 碰到后端 ⇒ 挪进「待迁」：\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_reach_meter_sees_a_real_backend_path_and_not_a_config_read() {
    let files = monitor_tree();
    let reg = registered(&files["lib.rs"], &files);
    let touching = Graph::build(&files).touching(&reg);
    // 正控：`chan_call` 经 `chan::host` 跨文件到 `client_for`（通信层本分，永久在「monitor 自己的事」）。
    assert!(
        touching.contains_key("chan_call"),
        "量不到 `chan_call` ⇒ 可达性量瞎了：{touching:?}"
    );
    // 反控：读 monitor 配置不碰后端。
    assert!(
        !touching.contains_key("load_config"),
        "`load_config` 被量成碰后端：{:?}",
        touching.get("load_config")
    );
}

#[test]
fn the_reach_meter_follows_uses_modules_and_type_methods() {
    let mut files = BTreeMap::new();
    files.insert(
        "lib.rs".to_string(),
        "use crate::a::helper;\nfn cmd_hit() { let s = \"client_for\"; helper(); }\nfn cmd_miss() { other(); }\nfn other() {}\n"
            .to_string(),
    );
    files.insert(
        "a.rs".to_string(),
        "pub fn helper() { crate::b::go(&Worker); }\n".to_string(),
    );
    files.insert(
        "b.rs".to_string(),
        "pub struct Worker;\npub fn go(w: &dyn Ask) { w.ask(); }\nimpl Ask for Worker { fn ask(&self) { let _ = user_files::BackendDoor::new(); } }\n"
            .to_string(),
    );
    // `Worker` 在 a.rs 里没 `use` ⇒ 按类型量不到；go 里只有 `.ask(` ⇒ 也量不到。补一行 use 才通。
    let cmds: BTreeMap<String, String> = [("cmd_hit", "lib.rs"), ("cmd_miss", "lib.rs")]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert!(
        Graph::build(&files).touching(&cmds).is_empty(),
        "字符串里的 `client_for` 被当成调用了"
    );
    files.insert(
        "a.rs".to_string(),
        "use crate::b::Worker;\npub fn helper() { crate::b::go(&Worker); }\n".to_string(),
    );
    let t = Graph::build(&files).touching(&cmds);
    assert_eq!(
        t.keys().cloned().collect::<Vec<_>>(),
        vec!["cmd_hit".to_string()],
        "use → 模块::函数 → 类型的方法 → BackendDoor 那条链没量到：{t:?}"
    );
    assert_eq!(t["cmd_hit"], vec!["crate::cmd_hit", "a::helper", "b::ask"]);
}
