//! 设计/99 §2.1 ⑬：「Tauri 命令分两张封闭表「monitor 自己的事」（逐行理由）与「待迁」（逐行卡在哪），命令实现经 `inbound_client` / `BackendDoor` 碰到后端的必须在「待迁」」。
//!
//! 分母只数「前端对后端」的 IPC（产物来自某台后端的）；窗口 · 本机 monitor 配置 · 日志 · 拉前 · 本机后端的起停与引导
//! 是 monitor 自己的事（`99 §2.1 ⑬`），外加通信层面 A 客户端本身与「放字节」（`4d-lanes` C 段共同目标）。
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
    /// 通信层面 A 客户端本身（`05 §14.2`）。
    Channel,
}

impl Own {
    fn may_touch(self) -> bool {
        matches!(self, Own::Lifecycle | Own::Channel)
    }
}

/// 哪一路负责迁走它（闭集：不许有没主的行）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lane {
    Mig1,
    Mig2,
    Mig3a,
    Mig3b,
}

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
    ("load_config", Own::Config, "本机 monitor 配置"),
    ("patch_config", Own::Config, "本机 monitor 配置"),
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
        "open_file_window",
        Own::Window,
        "〔MIG-3a · 09-28 裁 3〕起文件窗口进程、读它那一行就绪 / 原话（第一屏由窗口进程经通道自己列；本侧只拿交接件）",
    ),
    ("bring_monitor_to_front", Own::Window, "拉前 monitor 自己"),
    ("bring_terminal_to_front", Own::Front, "拉前本机终端窗口"),
    (
        "bound_terminal_count",
        Own::Front,
        "〔MIG-3a〕已跟 monitor 完成拉前握手的终端数（本进程 `BindRegistry`；从前夹在别名读回口里）",
    ),
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
    ("open_log_file", Own::Log, "日志"),
    ("open_log_dir", Own::Log, "日志"),
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
        "〔MIG-3a〕装 cc-bus 之前问一次本机 `ccm` 够不够新（本机后端的引导那一族：探本机 ccm 入口）",
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
    // 〔MIG-2〕起会话的计划与渲染进了后端之后，monitor 在这件事上只剩下面两格。
    (
        "open_local_terminal",
        Own::Window,
        "开一个本机终端窗口跑那一串（串由本机后端 `launch-local` 出成品；这里不判不拼，只开窗）",
    ),
    (
        "relay_all_sessions_switch",
        Own::Config,
        "全量注入开关 = monitor 进程环境 `CCM_RELAY_ALL_SESSIONS`（monitor 自己的配置，界面带给那台后端）",
    ),
];

/// 「待迁」：命令 · 哪一路 · 卡在哪。
const PENDING: &[(&str, Lane, &str)] = &[
    // MIG-1：会话 / tmux 账本 ＋ ssh 配置解读进后端。
    (
        "list_ssh_host_aliases",
        Lane::Mig1,
        "`~/.ssh/config` 还由 monitor 读（⑯）",
    ),
    (
        "resolve_ssh_host",
        Lane::Mig1,
        "`ssh -G` 还由 monitor 跑（⑯）",
    ),
    (
        "import_ssh_hosts",
        Lane::Mig1,
        "`ssh -G` 还由 monitor 跑（⑯）",
    ),
    (
        "test_remote_connection",
        Lane::Mig1,
        "拨号探针的判读还在 `ssh_source.rs`",
    ),
    (
        "list_session_activity",
        Lane::Mig1,
        "本机活会话表还在 monitor（`session_map::LocalTable`）",
    ),
    (
        "list_active_sessions",
        Lane::Mig1,
        "本机活会话表还在 monitor（`session_map::LocalTable`）",
    ),
    (
        "list_local_tmux",
        Lane::Mig1,
        "tmux 快照的解析还在 monitor（`parse_tmux_ls`）",
    ),
    (
        "list_remote_tmux",
        Lane::Mig1,
        "tmux 名单的解析还在 monitor（`parse_tmux_ls`）",
    ),
    // MIG-2：本机起会话 ＋ 载荷渲染 ＋ 历史查看器。〔MIG-2〕迁走七条：`new_local_session` · `resume_history_session` ·
    //   `render_local_attach` · `render_ccm_launch` · `render_launch_payload` · `relay_endpoint_for_launch` · `probe_ccm_cli`。
    (
        "stream_read_session_jsonl",
        Lane::Mig2,
        "历史查看器正文还经 monitor 转（㊱③）",
    ),
    (
        "launch_remote_terminal",
        Lane::Mig2,
        "远端拉起那串的 ssh 外壳（`ssh -t -J … host '<串>'` · PowerShell 窗口载荷）还在 monitor 拼：它读 monitor 的机器配置，\
         等 MIG-1 ⑯（`ssh -G` 与 `~/.ssh/config` 解读进本机后端）之后由本机后端渲（〔MIG-2〕报备：待主会话排）",
    ),
    // MIG-3a：资产与 D 组。
    // 〔MIG-3a〕别名六条（`aliases_*`）已迁：规则 · 方言 · 围栏进了那台后端（`aliases-*`），从本表删。
    (
        "deploy_remote_acct_iso",
        Lane::Mig3a,
        "〔09-28 裁 2〕落进用户目录 ＋ 软链 ＋ 记账已进那台后端（`acct-iso-install`，链接走写面 `files-link`）；\
         剩字节那一推：vendored 脚本住 monitor 二进制（`include_bytes!`），经本机后端 SFTP 推 —— 字节是否随 cc-bus 那样进后端二进制待主会话裁",
    ),
    // 〔MIG-3a · 子步 3〕`deploy_local_cc_bus` / `cc_bus_install_state` 已迁：cc-bus 装 · 三态 · 记账进了本机后端（`cc-bus-install` / `-state`）。
    // MIG-3b：部署决策 · 诊断 · 足迹 · 删会话 / 分叉。
    (
        "deploy_remote_backend",
        Lane::Mig3b,
        "该不该换 / 换成什么的判定在 `sftp.rs`",
    ),
    (
        "uninstall_remote_backend",
        Lane::Mig3b,
        "卸的判定在 `sftp.rs`",
    ),
    (
        "diagnose_local_cc_bus_hooks",
        Lane::Mig3b,
        "钩子诊断本机远端两份",
    ),
    (
        "diagnose_remote_cc_bus_hooks",
        Lane::Mig3b,
        "钩子诊断本机远端两份",
    ),
    (
        "config_surface_report",
        Lane::Mig3b,
        "足迹成品在 monitor 拼",
    ),
    ("drift_ledger_report", Lane::Mig3b, "足迹成品在 monitor 拼"),
    (
        "delete_history_session",
        Lane::Mig3b,
        "删会话是 monitor 里的组合",
    ),
    (
        "create_branch_session",
        Lane::Mig3b,
        "分叉是 monitor 里的组合",
    ),
    // 〔主会话 09-27 裁〕原先 C 段没人点名的那几行已指派（MIG-1 端口转发 · MIG-2 会话读面 · MIG-3b 探针 / 公钥 / 全景 · MIG-3a 开文件窗）。
    (
        "load_subagent",
        Lane::Mig2,
        "子 agent 读仍经 monitor 转（`05 §14.3` C 组）",
    ),
    (
        "read_session_range",
        Lane::Mig2,
        "骨架区间读仍经 monitor 转",
    ),
    ("read_session_lines", Lane::Mig2, "骨架行读仍经 monitor 转"),
    (
        "push_public_key",
        Lane::Mig3b,
        "`authorized_keys` 那串在 monitor 拼、经拨号面写",
    ),
    (
        "start_forward",
        Lane::Mig1,
        "端口转发经本机后端拨号面，账在 monitor",
    ),
    ("stop_forward", Lane::Mig1, "端口转发的账在 monitor"),
    ("list_forwards", Lane::Mig1, "端口转发的账在 monitor"),
    (
        "panorama_call",
        Lane::Mig3b,
        "全景一问经 monitor 转、装引擎字节由 monitor 放",
    ),
    ("panorama_edit", Lane::Mig3b, "全景一问经 monitor 转"),
    ("panorama_cancel", Lane::Mig3b, "全景撤单经 monitor 转"),
    // 〔MIG-3a · 主会话 09-28 裁 3〕`open_file_window` 已迁：开窗前那一屏（`files-home` / `files-ls`）进了窗口进程自己问，
    //   monitor 只起进程、读它那一行（`filewin/proc.rs::first_screen` · `Ready`）⇒ 从本表删，进 `MONITOR_OWN`（开窗）。
];

// ---------------------------------------------------------------- 读 `generate_handler!`

/// `generate_handler!` 里每一条：命令名 → 它住的文件（`src/bridge/src` 相对路径）。
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
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
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
    let pending: BTreeSet<String> = PENDING.iter().map(|(c, _, _)| c.to_string()).collect();
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
         迁走一条 ⇒ 从「待迁」删；新加一条 ⇒ 写进其中一张并写理由（`99 §2.1 ⑬`）"
    );
    let reasons = MONITOR_OWN
        .iter()
        .map(|(c, _, w)| (c, w))
        .chain(PENDING.iter().map(|(c, _, w)| (c, w)));
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
