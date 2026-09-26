//! 〔TL3 · 审计 F 🔴-2〕**同步 IPC 命令里零 `block_on` / 零同步连后端** —— 人群现扫、名字级调用闭包、例外逐条登记。
//!
//! # 守的要求（住址）
//!
//! - `src/doc/INVARIANTS.md §10`，逐字：「Tauri 的 `#[tauri::command] fn`（非 async）跑在 IPC 派发线程上。
//!   一个慢命令阻塞期间，其他 IPC 全部排队 → 整个 UI 没反应（切设置 / 拉前 / 切 Tab 全失灵）」；
//!   「**实施口诀**：IPC 命令默认写 `pub async fn`，函数体包 `tokio::task::spawn_blocking(move || { ... }).await.map_err(...)?`」。
//! - 出处：审计 F 🔴-2（`history.rs::resume_history_session` · `new_local_session` 两条同步命令里经
//!   `relay_prefix_for_launch` `block_on` 等本机后端，最长 `apikey_remote::BUDGET` 10 s）。
//!
//! # 判据（两向相等）
//!
//! **人群**：`src/bridge/src/**.rs`（`guard_core::scan_tree_excluding` 现扫，剥注释 / 测试段 / 字面量内容）里
//! 每一个 `#[tauri::command]` 且**不带 `async`** 的 fn —— 现派生，不手写名单。
//!
//! **性质**：每一条同步命令的**名字级调用闭包**里，「在 IPC 派发线程上等外面」的针所在的 fn ——
//! [`NEEDLES`]：`block_on(`（同步里等一次 async 往返的唯一桥）· `TcpStream::connect`（同步连后端口）——
//! 恒等于 [`PENDING`] 那张例外表：`{同步命令 → 针所在 fn 集合}` 两向相等。
//!
//! **异源**：左边是现扫源码现算的闭包；右边是手写的例外表与手写的合成夹具。
//!
//! **正控**：① [`the_reach_analyzer_sees_a_three_hop_chain_and_only_that`]：合成的几份源码（三跳链 · 方法调用 ·
//! `async` 命令 · 字面量里的针 · 同名歧义）⇒ 分析器恰好报出那一条；② 真仓：本机起会话那两条被认成 **async** 命令
//! （扫描器看得见属性 ＋ `async` 那一格）；③ 例外表非空的日子里，它自己就是真仓上的阳性对照。
//!
//! # 名字级闭包怎么算（规则写全，改规则就是改判据）
//!
//! - 一个 fn 的体里「标识符 ＋（可选 `::<…>`）＋ `(`」算一次调用；前面紧挨 `.` 的算方法调用。
//! - 解析：非方法调用 ⇒ 同文件有同名定义就连同文件那几个；否则全 crate 恰好一个同名定义才连。
//!   方法调用 ⇒ 全 crate 恰好一个同名定义才连。其余（同名歧义）**不连**。
//! - 闭包 / `async` 块 / `thread::spawn(move || …)` 的体算进外层 fn（**过近似**：真在别的线程上等的也算进来）。
//!
//! # 买不到（逐条）
//!
//! - **间接调用**：经函数指针 / 闭包值 / trait 对象的那一跳看不见（例：`history.rs::relay_endpoint_on`
//!   里 `(facts.endpoint)(…)` 那一跳）；同名歧义的调用不连。
//! - **针只认两样**。`§10` 同样点名的「同步命令里起进程（`Command::spawn` / `output` / `wait`）· 读写文件」
//!   **不在射程**：TL3 现打（名字级闭包原型）今天这一类还有 `list_local_tmux`（`tmux ls`）·
//!   `local_ccm_entry_status`（跑 `ccm --ccm-probe`）· `backend_stop`（`kill` ＋ `wait`）·
//!   `load_config` / `save_config` 等 —— 列在 `调研/第四波记录/TL3.md §1.4`，交主会话裁。
//! - 只看 monitor 这一个 crate（`#[tauri::command]` 全在这里）；共享 crate 里的阻塞看不见（今天零处）。

use std::collections::{BTreeMap, BTreeSet};

/// 在 IPC 派发线程上「等外面」的两种写法（量的是剥掉注释与字面量内容之后的源码）。
const NEEDLES: &[&str] = &["block_on(", "TcpStream::connect"];

/// 🔴 **例外表**：`(同步命令, 它的闭包里够得着的针所在 fn, 为什么)`。键形 `相对 src/bridge/src 的路径::fn 名`。
///
/// ⚠ **登记 ≠ 认可**。这里每一行都是**偏离 `INVARIANTS §10`**、由 TL3 报备、等主会话裁（改 `async` ＋
/// `spawn_blocking`，或在 `§10` 登记例外）。裁掉一行 ⇒ 从这里删那一行（本条两向相等，删了不改代码会红）。
const PENDING: &[(&str, &[&str], &str)] = &[(
    "backend/control/backend_control.rs::backend_start",
    &[
        "local_backend_host.rs::attach_stream",
        "local_backend_host.rs::probe_listen_port",
    ],
    "〔TL3 报备 · 待主会话裁〕机器页「起」：同步命令一路 `start_local_backend → start_detached → adopt_with`，\
     在 IPC 派发线程上连本机后端口（`probe_listen_port` 的 `TcpStream::connect`）、读 hello、\
     `thread::sleep` 等绑定、`attach_stream` 里 `block_on` —— 与 🔴-2 同形，不在 TL3 题面写区。",
)];

// ───────────────────────────── 分析器 ─────────────────────────────

/// 把注释换成空白、把字符串 / 字符字面量的**内容**换成空白（定界符留着），长度与换行不变。
///
/// ⚠ 为什么不只靠 `guard_core::production_code`：它剥注释与测试段，但**字面量内容还在** ——
/// 报错文案里的一个 `{` 就会把大括号配平带歪，文案里写着的 `block_on(` 也会被数成针（合成夹具里有这一格）。
fn mask(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let n = b.len();
    let mut out: Vec<char> = Vec::with_capacity(n);
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    while i < n {
        let c = b[i];
        // 行注释
        if c == '/' && i + 1 < n && b[i + 1] == '/' {
            while i < n && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        // 块注释（可嵌套）
        if c == '/' && i + 1 < n && b[i + 1] == '*' {
            let mut depth = 0usize;
            while i < n {
                if b[i] == '/' && i + 1 < n && b[i + 1] == '*' {
                    depth += 1;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else if b[i] == '*' && i + 1 < n && b[i + 1] == '/' {
                    depth -= 1;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
            continue;
        }
        // 原始字符串：r"…" / r#"…"# / br#"…"#（`r` 必须在词首，或紧跟词首的 `b`）
        let raw_start = c == 'r'
            && (i == 0 || !is_ident(b[i - 1]) || (b[i - 1] == 'b' && (i < 2 || !is_ident(b[i - 2]))));
        if raw_start {
            let mut j = i + 1;
            while j < n && b[j] == '#' {
                j += 1;
            }
            if j < n && b[j] == '"' {
                let hashes = j - i - 1;
                for &ch in &b[i..=j] {
                    out.push(ch);
                }
                i = j + 1;
                while i < n {
                    if b[i] == '"' && (0..hashes).all(|k| i + 1 + k < n && b[i + 1 + k] == '#') {
                        out.push('"');
                        for _ in 0..hashes {
                            out.push('#');
                        }
                        i += 1 + hashes;
                        break;
                    }
                    out.push(blank(b[i]));
                    i += 1;
                }
                continue;
            }
        }
        // 普通字符串（含 b"…"）
        if c == '"' {
            out.push('"');
            i += 1;
            while i < n && b[i] != '"' {
                if b[i] == '\\' && i + 1 < n {
                    out.push(' ');
                    out.push(blank(b[i + 1]));
                    i += 2;
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
            if i < n {
                out.push('"');
                i += 1;
            }
            continue;
        }
        // 字符字面量 vs 生命周期：'x' · '\n' · '\u{…}' 是字面量；'a（后面没有收尾引号）是生命周期
        if c == '\'' {
            if i + 1 < n && b[i + 1] == '\\' {
                let mut j = i + 2;
                while j < n && b[j] != '\'' {
                    j += 1;
                }
                out.push('\'');
                for _ in i + 1..j {
                    out.push(' ');
                }
                out.push('\'');
                i = j + 1;
                continue;
            }
            if i + 2 < n && b[i + 2] == '\'' {
                out.push('\'');
                out.push(' ');
                out.push('\'');
                i += 3;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out.into_iter().collect()
}

/// 一个 fn 定义：住哪份文件、叫什么、`fn` 关键字在哪、体（剥过的文本）。
struct Item {
    file: String,
    name: String,
    at: usize,
    body: String,
}

/// 一个 crate 的分析结果。
struct Analysis {
    /// `文件::名` —— 同步命令。
    sync_cmds: BTreeSet<String>,
    /// `文件::名` —— `async` 命令。
    async_cmds: BTreeSet<String>,
    /// 属性 `#[tauri::command` 出现的次数（自检：每一个都找得到它修饰的 fn）。
    attrs: usize,
    /// 属性修饰到的 fn 定义（按定义计，不按名字：`#[cfg]` 两支同名的各算一个）。
    cmd_items: usize,
    /// 同步命令 → 闭包里够得着的针所在 fn（空集不进表）。
    reach: BTreeMap<String, BTreeSet<String>>,
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// 从剥过的一份源码里抠出全部带体的 fn。
fn items_of(file: &str, masked: &str) -> Vec<Item> {
    let s = masked.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(k) = masked[i..].find("fn") {
        let at = i + k;
        i = at + 2;
        if at > 0 && is_ident_char(s[at - 1]) {
            continue;
        }
        if at + 2 >= s.len() || !s[at + 2].is_ascii_whitespace() {
            continue; // `fn(` 是函数指针类型，不是定义
        }
        let mut j = at + 2;
        while j < s.len() && s[j].is_ascii_whitespace() {
            j += 1;
        }
        let name_start = j;
        while j < s.len() && is_ident_char(s[j]) {
            j += 1;
        }
        if j == name_start {
            continue;
        }
        let name = masked[name_start..j].to_string();
        // 签名走到第一个深度 0 的 `{`（有体）或 `;`（无体）。
        let mut depth = 0i32;
        let mut body_start = None;
        while j < s.len() {
            match s[j] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b';' if depth <= 0 => break,
                b'{' if depth <= 0 => {
                    body_start = Some(j);
                    break;
                }
                _ => {}
            }
            j += 1;
        }
        let Some(start) = body_start else { continue };
        let mut d = 0i32;
        let mut end = start;
        while end < s.len() {
            match s[end] {
                b'{' => d += 1,
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        out.push(Item {
            file: file.to_string(),
            name,
            at,
            body: masked[start..=end.min(s.len() - 1)].to_string(),
        });
    }
    out
}

/// 一个体里的调用：`(是不是方法调用, 名字)`。
fn calls_of(body: &str) -> BTreeSet<(bool, String)> {
    let s = body.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i < s.len() {
        if !(s[i].is_ascii_alphabetic() || s[i] == b'_' || s[i] >= 0x80) || (i > 0 && is_ident_char(s[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < s.len() && is_ident_char(s[i]) {
            i += 1;
        }
        let name = &body[start..i];
        let mut j = i;
        while j < s.len() && s[j].is_ascii_whitespace() {
            j += 1;
        }
        // turbofish `::<…>`
        if body[j..].starts_with("::<") {
            let mut d = 0i32;
            j += 2;
            while j < s.len() {
                match s[j] {
                    b'<' => d += 1,
                    b'>' => {
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
            while j < s.len() && s[j].is_ascii_whitespace() {
                j += 1;
            }
        }
        if j < s.len() && s[j] == b'(' {
            let mut p = start;
            while p > 0 && s[p - 1].is_ascii_whitespace() {
                p -= 1;
            }
            let method = p > 0 && s[p - 1] == b'.';
            out.insert((method, name.to_string()));
        }
    }
    out
}

/// 把一批 `(相对路径, 原文)` 当成一个 crate 分析。
fn analyze(files: &[(String, String)]) -> Analysis {
    let mut items: Vec<Item> = Vec::new();
    let mut attrs = 0usize;
    let mut cmd_marks: Vec<(String, usize)> = Vec::new(); // (文件, 属性末尾)
    let mut texts: BTreeMap<String, String> = BTreeMap::new(); // 剥过的原文：属性与 fn 之间那一截要看有没有 `async`
    for (file, raw) in files {
        let masked = mask(&guard_core::production_code(raw));
        let mut from = 0;
        while let Some(k) = masked[from..].find("#[tauri::command") {
            let head = from + k;
            let tail = head + masked[head..].find(']').expect("属性没有收尾");
            attrs += 1;
            cmd_marks.push((file.clone(), tail));
            from = tail;
        }
        items.extend(items_of(file, &masked));
        texts.insert(file.clone(), masked);
    }
    let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (k, it) in items.iter().enumerate() {
        by_name.entry(it.name.as_str()).or_default().push(k);
    }
    let key = |k: usize| format!("{}::{}", items[k].file, items[k].name);
    let mut sync_roots = Vec::new();
    let mut cmd_items = BTreeSet::new();
    let mut sync_cmds = BTreeSet::new();
    let mut async_cmds = BTreeSet::new();
    for (file, tail) in &cmd_marks {
        let next = items
            .iter()
            .enumerate()
            .filter(|(_, it)| &it.file == file && it.at > *tail)
            .min_by_key(|(_, it)| it.at)
            .map(|(k, _)| k)
            .unwrap_or_else(|| panic!("{file}：`#[tauri::command` 后面找不到它修饰的 fn"));
        cmd_items.insert(next);
        let between = &texts[file][*tail..items[next].at];
        let is_async = between.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == "async");
        if is_async {
            async_cmds.insert(key(next));
        } else {
            sync_cmds.insert(key(next));
            sync_roots.push(next);
        }
    }
    let resolve = |cur: usize, method: bool, name: &str| -> Vec<usize> {
        let Some(defs) = by_name.get(name) else {
            return Vec::new();
        };
        if !method {
            let same: Vec<usize> = defs.iter().copied().filter(|&d| items[d].file == items[cur].file).collect();
            if !same.is_empty() {
                return same;
            }
        }
        if defs.len() == 1 {
            return defs.clone();
        }
        Vec::new()
    };
    let mut reach = BTreeMap::new();
    for root in sync_roots {
        let mut seen = BTreeSet::new();
        let mut stack = vec![root];
        let mut hits = BTreeSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            if NEEDLES.iter().any(|nd| items[cur].body.contains(nd)) {
                hits.insert(key(cur));
            }
            for (method, name) in calls_of(&items[cur].body) {
                stack.extend(resolve(cur, method, &name));
            }
        }
        if !hits.is_empty() {
            reach.insert(key(root), hits);
        }
    }
    Analysis {
        sync_cmds,
        async_cmds,
        attrs,
        cmd_items: cmd_items.len(),
        reach,
    }
}

/// 真仓：`src/bridge/src/**.rs` 全部（相对路径，`/` 分隔）。
fn this_crate() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files: Vec<(String, String)> = guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, text)| {
            let rel = p
                .strip_prefix(&root)
                .expect("扫到的文件不在 src 下")
                .to_string_lossy()
                .replace('\\', "/");
            (rel, text)
        })
        .collect();
    assert!(
        files.iter().any(|(f, _)| f == "history.rs") && files.iter().any(|(f, _)| f == "lib.rs"),
        "扫描面里连 `history.rs` / `lib.rs` 都没有 —— 根目录取歪了，本条此刻无效"
    );
    files
}

// ───────────────────────────── 判据 ─────────────────────────────

/// ★★★ **同步命令的调用闭包里，「在 IPC 派发线程上等外面」的针 == 例外表**（两向相等）。
#[test]
fn no_sync_command_waits_on_the_outside_except_the_registered_deviations() {
    let a = analyze(&this_crate());
    assert_eq!(
        a.attrs, a.cmd_items,
        "有两个 `#[tauri::command` 属性对上了同一个 fn 定义 —— 抽取器坏了，本条此刻无效\
         （`#[cfg]` 两支同名的命令是两个定义，各算一个：例 `lib.rs::bring_monitor_to_front`）"
    );
    let want: BTreeMap<String, BTreeSet<String>> = PENDING
        .iter()
        .map(|(cmd, hits, _)| {
            (
                (*cmd).to_string(),
                hits.iter().map(|h| (*h).to_string()).collect(),
            )
        })
        .collect();
    let extra: Vec<_> = a
        .reach
        .iter()
        .filter(|(k, v)| want.get(*k) != Some(*v))
        .collect();
    let stale: Vec<_> = want
        .iter()
        .filter(|(k, v)| a.reach.get(*k) != Some(*v))
        .collect();
    assert!(
        extra.is_empty() && stale.is_empty(),
        "\n🔴 **同步 `#[tauri::command]` 的调用闭包里够得着「等外面」的写法**（`INVARIANTS §10`）：\n\
         盘上有、例外表里没有（或针集合不同）：{extra:#?}\n\
         例外表里有、盘上已经没有（或针集合不同）：{stale:#?}\n\n\
         ⇒ 前一种：那条命令会在 IPC 派发线程上等（`block_on` / 同步连口），期间别的 IPC 全排队 ——\n\
            改 `pub async fn` ＋ 同步那一截包 `spawn_blocking`（`§10` 实施口诀；先例 `history.rs::launch_local_asking_backend`），\n\
            别往例外表里加一行了事（加一行 = 偏离设计，要主会话裁）。\n\
         ⇒ 后一种：修好了 ⇒ 从 `PENDING` 删掉那一行；只是换了针所在的 fn ⇒ 按实数改并写清为什么。"
    );
}

/// 正控（真仓）：本机起会话那两条命令被认成 **async** 命令 —— 扫描器看得见属性，也读得出 `async` 那一格。
#[test]
fn the_two_local_launch_commands_are_seen_as_async_commands() {
    let a = analyze(&this_crate());
    for cmd in [
        "history.rs::resume_history_session",
        "history.rs::new_local_session",
    ] {
        assert!(
            a.async_cmds.contains(cmd) && !a.sync_cmds.contains(cmd),
            "`{cmd}` 不在 async 命令里（sync 那一侧有它：{}）—— 要么它退回了同步（`INVARIANTS §10` 那一刀又回来了），\
             要么扫描器认不出属性 / `async`，上一条此刻无效",
            a.sync_cmds.contains(cmd)
        );
    }
}

/// 正控（合成）：分析器在一份已知答案的小 crate 上**恰好**报出那一条链 —— 三跳（命令 → 助手 → 方法 → 针）。
///
/// 同一批夹具里的阴性格：`async` 命令够得着针（不算 —— 它不在 IPC 派发线程上）· 字面量里写着针（剥掉了）·
/// 同名歧义的调用（按规则不连）· 一个干净的同步命令。
#[test]
fn the_reach_analyzer_sees_a_three_hop_chain_and_only_that() {
    let a_rs = r#"
#[tauri::command]
pub fn a_sync(x: String) -> Result<(), String> {
    helper_b(&x);
    Ok(())
}
fn helper_b(x: &str) {
    let s = S;
    s.method_c(x)
}
#[tauri::command]
pub async fn d_async() -> Result<(), String> {
    e_blocks();
    Ok(())
}
fn e_blocks() {
    tauri::async_runtime::block_on(async {})
}
#[tauri::command]
fn f_clean() -> u32 {
    let _s = "block_on(x) { TcpStream::connect";
    let _c = '{';
    g_ambiguous();
    1
}
"#;
    let b_rs = r#"
impl S {
    pub fn method_c(&self, x: &str) {
        let _ = std::net::TcpStream::connect(x);
    }
}
fn g_ambiguous() { rt.block_on(x) }
"#;
    let c_rs = "fn g_ambiguous() {}\n";
    let files = [
        ("a.rs".to_string(), a_rs.to_string()),
        ("b.rs".to_string(), b_rs.to_string()),
        ("c.rs".to_string(), c_rs.to_string()),
    ];
    let got = analyze(&files);
    assert_eq!((got.attrs, got.cmd_items), (3, 3));
    assert_eq!(
        got.sync_cmds,
        BTreeSet::from(["a.rs::a_sync".to_string(), "a.rs::f_clean".to_string()])
    );
    assert_eq!(got.async_cmds, BTreeSet::from(["a.rs::d_async".to_string()]));
    assert_eq!(
        got.reach,
        BTreeMap::from([(
            "a.rs::a_sync".to_string(),
            BTreeSet::from(["b.rs::method_c".to_string()])
        )]),
        "分析器没恰好报出那条三跳链（或把阴性格报了出来）"
    );
}

/// 剥法自检：注释、字符串（含原始串 / 转义引号）、字符字面量里的东西都剥掉，生命周期不当字面量。
#[test]
fn the_mask_blanks_comments_and_literal_contents_but_keeps_code() {
    let src = "fn a<'x>(s: &'x str) { /* block_on( */ let q = \"a\\\"{\"; let r = r#\"}\"#; let c = '}'; f(); }";
    let m = mask(src);
    assert_eq!(m.chars().count(), src.chars().count(), "剥法改了长度");
    assert!(!m.contains("block_on("), "块注释里的针没剥：{m}");
    assert_eq!(m.matches('{').count(), m.matches('}').count(), "字面量里的大括号漏了一只：{m}");
    assert!(m.contains("<'x>") && m.contains("&'x str") && m.contains("f();"), "代码被剥坏了：{m}");
}
