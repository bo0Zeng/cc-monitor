//! 「Tauri 命令只剩 monitor 自己的事（逐行理由）；命令实现经 `inbound_client` / `BackendDoor` 碰到后端的不许留在 monitor」。
//!
//! 分母只数「前端对后端」的 IPC（产物来自某台后端的）；窗口 · 本机 monitor 配置 · 日志 · 拉前 · 本机后端的起停与引导
//! 是 monitor 自己的事，外加通信层面 A 客户端本身与「放字节」。
//!
//! 命令表只有一张：`parity_ledger` 的 `LEDGER`（命令 → 能力 · 侧 · 哪一类 · 理由），它与 `generate_handler!` 两向相等由那边判。
//! 这里两条判据：
//! 1. 「monitor 自己的事」里不许碰后端的那几类（窗口 / 开终端 / 拉前 / 配置 / 日志 / 重放缓冲），
//!    实现传递可达 `local_client` / `BackendDoor` ⇒ 红（判定该进后端）。通道 · 起停 · 放字节三类碰后端就是它们的本分。
//! 2. 可达性量得出：真树上 `chan_call`（经 `chan::host` 跨文件到 `local_client`）必须被量到、`load_config` 必须量不到；
//!    合成夹具上「`use` 进来的函数 → 别的文件 → 类型的方法 → `BackendDoor`」必须量到。
//!
//! 可达性是**静态近似**：函数级，边按「同文件自由函数 · `use` 进来的名字 · `模块::函数` · `类型::方法` · 提到同文件 /
//! `use` 进来的类型 ⇒ 它的全部方法」解析；`.方法(` 不解析（量少不量多）。量少 ⇒ 判据 2 可能漏一条，不会冤枉一条。

use std::collections::{BTreeMap, BTreeSet};

use crate::parity_ledger::tests::{Own, LEDGER};

/// 通道上**由 monitor 自己接**、不按 `origin` 转给那台后端的 op（`chan/host.rs::HOST_OPS`）：
/// op · 哪一类 · 理由。它们不是 Tauri 命令，却是同一个问题（是不是「monitor 自己的事」），照 `parity_ledger` 那张命令表的写法两向登记。
const CHANNEL_OWN: &[(&str, Own, &str)] = &[
    (
        "transfer-upload",
        Own::Channel,
        "传输台开单：转给本机常驻后端的传输台（经中继 `transfer_relay.rs`），不按寻址去那台",
    ),
    (
        "transfer-download",
        Own::Channel,
        "同上（下载那一形）",
    ),
    (
        "transfer-copy",
        Own::Channel,
        "两台之间复制一组：monitor 补上源与落点两台的拨号请求，转给本机常驻后端（它经中转做完整组），不按寻址去那台",
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
        "plan-open",
        Own::Window,
        "文件窗口「在计划里看」只交意图 `{workspace, slice, id}`：monitor 把主窗口拉到前面、发事件给它开计划页选中那一格（主窗口住 monitor）",
    ),
    (
        "filewin-busy",
        Own::Window,
        "文件窗口说它手上会被打断的数（没存的 · 在跑的）：那扇窗是 monitor 起的子进程、退出 / 重启 cc-monitor 才断它，表就住窗口那条通道的宿主（`chan/host.rs::busy`）",
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

    /// 每个函数：可达 `local_client` / `BackendDoor` 的那条链（函数名，由近及远）。
    fn reach(&self) -> BTreeMap<usize, Vec<String>> {
        let sink = |f: &Func| {
            f.toks
                .iter()
                .any(|t| matches!(t, Tok::Id(s) if s == "local_client" || s == "BackendDoor"))
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
    let root = guard_core::addr::shell_src_root();
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
fn a_monitor_own_command_of_a_local_kind_does_not_reach_a_backend() {
    let files = monitor_tree();
    let reg = registered(&files["lib.rs"], &files);
    let touching = Graph::build(&files).touching(&reg);
    let wrong: Vec<String> = LEDGER
        .iter()
        .filter(|(_, _, _, kind, _)| !kind.may_touch())
        .filter_map(|(c, _, _, kind, _)| {
            touching
                .get(*c)
                .map(|chain| format!("{c}（{kind:?}）：{}", chain.join(" → ")))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "这几条记成 monitor 自己的事，实现却经 `local_client` / `BackendDoor` 碰到后端 ⇒ 判定该进后端：\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_reach_meter_sees_a_real_backend_path_and_not_a_config_read() {
    let files = monitor_tree();
    let reg = registered(&files["lib.rs"], &files);
    let touching = Graph::build(&files).touching(&reg);
    // 正控：`chan_call` 经 `chan::host` 跨文件到 `local_client`（通信层本分，永久在「monitor 自己的事」）。
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
        "use crate::a::helper;\nfn cmd_hit() { let s = \"local_client\"; helper(); }\nfn cmd_miss() { other(); }\nfn other() {}\n"
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
        "字符串里的 `local_client` 被当成调用了"
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
