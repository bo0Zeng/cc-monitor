//! # 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8）· `INVARIANTS §47`（外部值先过放行判定）
//!
//! 核原文：`§49` 逐字「本仓**每一处**按格式串读 tmux 打印通道的调用点（`list-sessions -F` · `ls -F` · `display-message -p`），
//! 起的 tmux 客户端都必须是 UTF-8 客户端」；`§47` 逐字「一个值只要从本进程外面来 …… 在它被拼进 shell 命令串、
//! 或被交给对端去执行 / 去寻址之前，本侧先过一道按这个值的种类写成的放行判定」；`D5` 逐字「判据的人群要从文件系统全集来，
//! 不从『配置里已经承认的那批』来」。
//!
//! 〔IV1 余〕两节升格时（IV1）都在「买不到」里写着同一句：**没有人群判据** —— 每条判据只管它自己那个模块 /
//! 那个值，新长一处读 tmux 的调用点、新长一处把值拼进 shell 的地方，一条都不会红。本文件补的就是这两个人群：
//! 盘上现扫、与登记表两向相等。登记表里每一行都得**说清它是哪一形**；说不清的（违反、或只靠 quote）如实登记，
//! 不写成豁免 —— 新长出来的那一处因此当场红，逼人回来表态。
//!
//! ⚠ 它们**不替**各调用点自己的判据判「带没带对」：`§49` 那几条按调用点钉旗的位置、`§47` 那几族钉放行判定的正反两格，
//! 都还在原处。这里只管**人群不许漏**。

use std::collections::{BTreeMap, BTreeSet};

/// 仓里要扫的生产源码：`src/` 下（摘掉 `vendor/`）的 Rust · TS · shell（`.sh` 与无后缀带 shebang 的脚本）。
/// 返回 `(仓内相对路径, 剥掉注释与测试段之后的正文)`。
fn production_sources() -> Vec<(String, String)> {
    let root = crate::guard_support::repo_root();
    let src = root.join("src");
    // 遍历口径只有一份（`guard_core`）：带扩展名的那几种 ＋ 整棵 `src/shared/`（cc-bus 脚本没有扩展名）。
    let mut files = guard_core::scan_tree_excluding(&src, &["rs", "ts", "sh"], &[]);
    files.extend(guard_core::scan_tree_excluding(
        &src.join("shared"),
        &[],
        &[],
    ));
    let mut out = Vec::new();
    for (p, raw) in files {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.contains("/vendor/") || out.iter().any(|(r, _): &(String, String)| *r == rel) {
            continue;
        }
        let name = rel.rsplit('/').next().unwrap_or_default().to_string();
        let body = if name.ends_with(".rs") {
            guard_core::strip_comment_lines(&guard_core::production_code(&raw))
        } else if name.ends_with(".ts") && !name.ends_with(".d.ts") {
            guard_core::strip_comment_lines(&raw)
        } else if name.ends_with(".sh")
            || (!name.contains('.')
                && raw
                    .lines()
                    .next()
                    .is_some_and(|l| l.get(..2) == Some("#!") && l.contains("sh")))
        {
            guard_core::strip_hash_comment_lines(&raw)
        } else {
            continue;
        };
        out.push((rel, body));
    }
    out.sort();
    assert!(
        out.iter()
            .any(|(r, _)| r == "src/backend/observe/watcher.rs")
            && out
                .iter()
                .any(|(r, _)| r == "src/shared/cc-bus/scripts/cc-kill"),
        "扫到的生产源码只有 {} 份、或缺 watcher.rs / cc-kill —— 根没对上，下面的相等会空真",
        out.len()
    );
    out
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// `hay` 里 `needle` 作为一个**词**出现的所有起点（两侧都不是词字符）。
fn word_hits(hay: &str, needle: &str) -> Vec<usize> {
    let b = hay.as_bytes();
    hay.match_indices(needle)
        .map(|(i, _)| i)
        .filter(|&i| {
            let before = i == 0 || !is_word_byte(b[i - 1]);
            let j = i + needle.len();
            let after = j >= b.len() || !is_word_byte(b[j]);
            before && after
        })
        .collect()
}

// ═══════════════════════════════ §49 ═══════════════════════════════

/// `§49` 人群里的 tmux 子命令 ⇒ 它读打印通道用的那个旗。
const PRINT_SUBCOMMANDS: &[(&str, &str)] = &[
    ("ls", "-F"),
    ("list-sessions", "-F"),
    ("list-panes", "-F"),
    ("list-windows", "-F"),
    ("list-clients", "-F"),
    ("display-message", "-p"),
    ("display", "-p"),
];

/// 一处调用点的 UTF-8 怎么带。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Carry {
    /// 子命令之前有 `-u`（`UTF8_CLIENT_FLAG` 或字面 `-u`）。
    Flag,
    /// 没有旗，而这份文件经 `UTF8_CLIENT_ENV` 给起 `sh` 的那个 `Command` 挂 env（`§49`「`sh -c` 用 env」那一形）。
    Env,
    /// 两样都没有。
    None,
}

/// 盘上现扫：`(文件, 子命令, 怎么带) → 处数`。两形调用点：
/// - **命令串形**（shell 脚本、Rust / TS 里拼的命令串）：同一行里 `tmux` 这个词之后出现子命令词，子命令之后同一行里有那个旗；
///   旗认的是离子命令最近的那个 `tmux` 与子命令之间的那一段。
/// - **argv 形**（Rust `Command::args`）：字面量 `"子命令"` 之后隔着逗号紧跟字面量 `"-F"` / `"-p"`；旗认的是前 80 字节里的 `UTF8_CLIENT_FLAG,`。
fn tmux_print_sites() -> BTreeMap<(String, String, Carry), usize> {
    let mut out: BTreeMap<(String, String, Carry), usize> = BTreeMap::new();
    for (rel, body) in production_sources() {
        let env_here = body.contains("UTF8_CLIENT_ENV");
        let carry = |flag: bool| {
            if flag {
                Carry::Flag
            } else if env_here {
                Carry::Env
            } else {
                Carry::None
            }
        };
        for line in body.lines() {
            let tmuxes = word_hits(line, "tmux");
            if tmuxes.is_empty() {
                continue;
            }
            for &(sub, opt) in PRINT_SUBCOMMANDS {
                for at in word_hits(line, sub) {
                    let Some(&t) = tmuxes.iter().filter(|&&t| t < at).max() else {
                        continue;
                    };
                    // 子命令之后、同一条命令之内（到 `;` / `|` / `&&` 为止）要有那个旗。
                    let rest = &line[at + sub.len()..];
                    let end = rest
                        .find([';', '|'])
                        .unwrap_or(rest.len())
                        .min(rest.find("&&").unwrap_or(rest.len()));
                    if !word_hits(&rest[..end], opt)
                        .iter()
                        .any(|&i| i > 0 && rest.as_bytes()[i - 1] == b' ')
                    {
                        continue;
                    }
                    let between = &line[t + 4..at];
                    let flag = between.contains("UTF8_CLIENT_FLAG")
                        || word_hits(between, "-u")
                            .iter()
                            .any(|&i| i > 0 && between.as_bytes()[i - 1] == b' ');
                    *out.entry((rel.clone(), sub.to_string(), carry(flag)))
                        .or_default() += 1;
                }
            }
        }
        for &(sub, opt) in PRINT_SUBCOMMANDS {
            let lit = format!("\"{sub}\"");
            for (at, _) in body.match_indices(&lit) {
                let rest = body[at + lit.len()..].trim_start();
                let Some(rest) = rest.strip_prefix(',') else {
                    continue;
                };
                if !rest.trim_start().starts_with(&format!("\"{opt}\"")) {
                    continue;
                }
                let from = at.saturating_sub(80);
                let from = (0..=from)
                    .rev()
                    .find(|&i| body.is_char_boundary(i))
                    .unwrap_or(0);
                let flag = body[from..at].contains("UTF8_CLIENT_FLAG,");
                *out.entry((rel.clone(), sub.to_string(), carry(flag)))
                    .or_default() += 1;
            }
        }
    }
    out
}

/// `§49` 的人群登记：`(文件, 子命令, 怎么带, 处数, 为什么)`。
///
/// `Carry::None` 的每一行都是**人群外的真实调用点**：`§49` 字面上它们都算违反。按它读的是什么分两类写清 ——
/// 读出来的字段只可能是 ASCII（pid · 窗口数 · `%N` 形 pane id）⇒ 改写成 `_` 也不改一个字节，今天无害；
/// 读的是**会话名 / 地址**（用户起的会话名可以是中文）⇒ 非 UTF-8 客户端下会被改写成 `_`，**是真违反，待裁**
/// （改它们是后端载荷字节 / 随部署脚本字节的变更，另做）。
/// 那五处（`BUS_ID_RECIPE` · `cc-register` 登记地址 · `cc-whoami` ×3）加了旗、翻成 `Flag`；
/// 今天 `Carry::None` 只剩读 ASCII 的那几处。
const TMUX_PRINT_SITES: &[(&str, &str, Carry, usize, &str)] = &[
    ("src/backend/common/session_snapshot.rs", "list-sessions", Carry::Flag, 1, "argv；`session_snapshot_tests.rs` 钉旗在子命令前"),
    ("src/backend/control/gate.rs", "display-message", Carry::Flag, 1, "argv；`gate_tests.rs` 钉旗在子命令前"),
    ("src/backend/observe/tmux_observe.rs", "ls", Carry::Env, 1, "`sh -c` 一段脚本（期限归起子进程原语），env 一行盖住；`watcher_tests.rs` 钉 env"),
    ("src/backend/observe/tmux_observe.rs", "display-message", Carry::Env, 1, "`sh -c`，同上"),
    // monitor `tmux.rs` 那条跨 SSH `ls`（Flag）删了：`list_remote_tmux` 改问那台后端 `tmux-list`（那一趟 `ls` 住 `watcher.rs`，env 形）。
    ("src/backend/control/ccm/plan.rs", "list-panes", Carry::None, 1, "无害：只读 `#{pane_id}`（`%N`，ASCII，单列、不按 TAB 切）—— IV1 报过"),
    ("src/backend/control/ccm/mod.rs", "display-message", Carry::Flag, 1, "`BUS_ID_RECIPE` 读 `#S`（会话名，可以非 ASCII）⇒ 拼进 pane 的命令串，按表用旗；真跑判据 `backend-cc-bus.sh` [SH1-a]"),
    ("src/shared/cc-bus/scripts/cc-bus-adapt-posix.sh", "display-message", Carry::None, 1, "无害：只读 `#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-kill", "display-message", Carry::None, 2, "无害：`#{pane_pid}` · `#{session_windows}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-kill", "list-panes", Carry::None, 1, "无害：`#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-register", "display-message", Carry::Flag, 1, "读 `#{session_name}:…` 当登记地址（会话名可以非 ASCII）⇒ 旗；真跑判据 `backend-cc-bus.sh` [SH1-a]"),
    ("src/shared/cc-bus/scripts/cc-register", "display-message", Carry::None, 1, "无害：`#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-agents", "display-message", Carry::None, 1, "无害：`#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-whoami", "display-message", Carry::Flag, 1, "读 `#{session_name}` 当身份 ⇒ 旗（非 UTF-8 客户端下中文被改写成 `_`，消毒之后与 UTF-8 客户端下的身份不同）；真跑判据 `backend-cc-bus.sh` [SH1-a]"),
    ("src/shared/cc-bus/scripts/cc-whoami", "list-sessions", Carry::Flag, 1, "`#{session_id} #{session_name}` ⇒ 旗（同上）"),
    ("src/shared/cc-bus/scripts/cc-whoami", "list-panes", Carry::Flag, 1, "`#{pane_pid} #{session_name}` ⇒ 旗（同上）"),
    ("src/backend/control/gate.rs", "list-panes", Carry::Flag, 1, "argv；按 sid 找窗格：读这个会话各窗格的句柄 · 根进程 pid · `@ccm_sid`（ASCII，照表仍带旗）"),
    ("src/backend/control/terminals.rs", "list-panes", Carry::Flag, 1, "argv；终端名单逐窗格读会话名 · 工作目录 · 窗格标题（可以非 ASCII）⇒ 旗"),
    ("src/backend/control/terminals.rs", "list-clients", Carry::Flag, 1, "argv；读会话 ID 与两个时刻（ASCII，照表仍带旗）"),
    ("src/backend/control/terminals.rs", "display-message", Carry::Flag, 2, "argv；预览问尺寸与光标 · 实时预览订上之前问 tmux 版本（数字与 ASCII，照表仍带旗）"),
];

/// `INVARIANTS §49` 人群判据：盘上每一处「tmux 打印子命令 ＋ 读打印通道的旗」== 登记表（两向，含处数与带法）。
/// 新长一处读 tmux 的调用点 ⇒ 红；它带了旗 / env 就登记成那一形，没带就得说清它读的是什么（ASCII 无害 / 待裁）。
#[test]
fn every_tmux_print_site_is_registered_with_how_it_carries_utf8() {
    let on_disk = tmux_print_sites();
    let registered: BTreeMap<(String, String, Carry), usize> = TMUX_PRINT_SITES
        .iter()
        .map(|&(f, s, c, n, _)| ((f.to_string(), s.to_string(), c), n))
        .collect();
    assert_eq!(registered.len(), TMUX_PRINT_SITES.len(), "登记表里有重复行");
    // 正控：带旗 / 带 env / 不带三形各至少一处真在盘上 —— 否则认法坏了也可能恰好两向相等（两边都空那一格）。
    for c in [Carry::Flag, Carry::Env, Carry::None] {
        assert!(
            on_disk.keys().any(|k| k.2 == c),
            "盘上一处 {c:?} 形都没扫到 —— 认法坏了"
        );
    }
    assert_eq!(
        on_disk, registered,
        "`INVARIANTS §49` 的人群与登记表对不上（两向）。\n\
         盘上多出来的 = 新长的读 tmux 打印通道的调用点：带上 `UTF8_CLIENT_FLAG` / `UTF8_CLIENT_ENV`（家在后端 `common/tmux_utf8.rs`）再登记，\n\
         实在不带就写清它读的是什么；登记了而盘上没有 = 那一处删了 / 改了，跟着改表。"
    );
}

// ═══════════════════════════════ §47 ═══════════════════════════════

/// 唯一那份 quote 的**纯转发别名**名（盘上现认）：`use … posix_quote as X` 的 `X`，以及函数体只有一句
/// `shell_quote_core::posix_quote(参数)` 的那几个函数名。返回 `(名字集, 别名本体所在的文件集)`。
fn quote_aliases(srcs: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    // 唯一的 quote 多了一个字节形（`posix_quote_bytes`，`$'…'`）—— 同一份 quote 的另一形，同样算拼接点。
    let mut names = vec!["posix_quote".to_string(), "posix_quote_bytes".to_string()];
    let mut bodies = Vec::new();
    for (rel, body) in srcs {
        if !rel.ends_with(".rs") {
            continue;
        }
        for (at, _) in body.match_indices("posix_quote as ") {
            let rest = &body[at + "posix_quote as ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        for (at, _) in body.match_indices("fn ") {
            let sig = &body[at + 3..];
            let name: String = sig
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let Some(open) = sig.find('{') else { continue };
            let Some(close) = sig[open..].find('}') else {
                continue;
            };
            let inner = sig[open + 1..open + close].trim();
            // 整个函数体（去空白）恰好是 `shell_quote_core::posix_quote(<一个标识符>)` —— 按段相等比，不做子串匹配。
            let forwards = inner.split_once('(').is_some_and(|(head, tail)| {
                head == "shell_quote_core::posix_quote"
                    && tail.len() > 1
                    && tail[..tail.len() - 1]
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_')
                    && tail.chars().last() == Some(')')
            });
            if forwards {
                if !names.contains(&name) {
                    names.push(name);
                }
                bodies.push(rel.clone());
            }
        }
    }
    (names, bodies)
}

/// 一处调用落在哪个函数里：它之前最近的那个 `fn <名>`（嵌套的具名函数写在调用之前、又已收尾的那一形会认错 —— 本仓生产段没有）。
fn enclosing_fn(body: &str, at: usize) -> String {
    let head = &body[..at];
    let mut best: Option<usize> = None;
    for (i, _) in head.match_indices("fn ") {
        let word_start = i == 0
            || !head[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if word_start {
            best = Some(i);
        }
    }
    best.map_or_else(String::new, |i| {
        head[i + 3..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    })
}

/// 盘上现扫：Rust 生产段里每份文件「调 quote（含纯转发别名）」的那几个**外层函数** —— 去掉声明（`fn 名(`）与别名本体里那一句转发。
fn quote_sites() -> (BTreeMap<String, BTreeSet<String>>, Vec<String>) {
    let srcs = production_sources();
    let (names, alias_files) = quote_aliases(&srcs);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (rel, body) in &srcs {
        if !rel.ends_with(".rs")
            || rel
                .split('/')
                .take(3)
                .eq(["src", "common", "shell-quote-core"])
        {
            continue;
        }
        for name in &names {
            for at in word_hits(body, name) {
                let after = &body[at + name.len()..];
                if !after.starts_with('(') {
                    continue;
                }
                if body[..at].ends_with("fn ") {
                    continue; // 声明
                }
                let owner = enclosing_fn(body, at);
                // 别名本体里那一句转发不算调用点（它就是 quote 本身）。
                if alias_files.contains(rel) && names.contains(&owner) {
                    continue;
                }
                out.entry(rel.clone()).or_default().insert(owner);
            }
        }
    }
    (out, names)
}

/// `§47` 的人群登记：`(文件, 调 quote 的外层函数, 本侧放行判定, 只靠 quote 的外部值, 本侧自己的值)`。
///
/// - 第三列点名的放行判定 `(文件, 函数名)` 逐个在盘上核得到（改名 / 搬家红）。
/// - 第四列非空 = 🔴 **有外部值只靠 quote**：`§47` ②形要的「拒绝集 ＋ 形式判定」这一层没有 —— 如实登记、**待裁**，不写成豁免。
/// - 第五列 = 拼进去的**本侧的值**（常量 · 本侧铸的 · 本进程自己的路径 · 本侧已渲染好的整串），`§47` 不管它们。
///
/// 人群 = 生产段里调 quote 的地方（「quote 只有一份」⇒ 值进 shell 串的合法入口只有它）。
/// ⚠ 射程：它逮的是「新长一处拼接点」；**不经 quote 的裸插值**（`format!` 直接把值塞进命令串）它看不见；
/// 条件 quote 的包装（`plan.rs::qarg` · `ccm_invocation.rs` 的 `argv` · `shell_dialect.rs` 的 `word`）
/// 在它自己的文件里算一处，经它的那些调用方不再逐个数；第四列是**按文件**写的，不是逐处。
/// TS 那一侧（`posixQuote`）不在人群里 —— LR2 在删那几份 TS 命令构造器，「TS 零 shell 串」是那条机械判据的活。
type QuoteRow = (
    &'static str,
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
);
const QUOTE_SITES: &[QuoteRow] = &[
    // 账号库管理：「在终端里登录这个号」那一行（`<家>/.cc-monitor/bin/ccm -- --account <名>`）。
    //   账号名先过 `account_name_ok`（且必须是清单里已有的号），ccm 那条路径是这台自己的家目录拼出来的，两者都经唯一的 quote。
    (
        "src/backend/accounts/manage/wire.rs",
        &["login_line"],
        &[(
            "src/common/shell-quote-core/src/lib.rs",
            "account_name_ok",
        )],
        "",
        "",
    ),
    // 公钥推送进了本机后端：一行公钥拼进那一次 exec 之前先过 `sanitize_public_key`（恰一行 · 无控制字符 · 类型前缀 ＋ base64 主体），原住 monitor `pubkey.rs`。
    (
        "src/backend/assets/pubkey.rs",
        &["authorized_keys_cmd"],
        &[("src/backend/assets/pubkey.rs", "sanitize_public_key")],
        "",
        "",
    ),
    // 〔§47〕那台后端的路径在可达表唯一的写口 `remote_ask::register` 先过放行判定（后端那一份同族判定），第四列清空。
    // 〔§47〕cwd（绝对 · 无 `..` 段）· 启动器 · 透传参数 · 登记备注 · 继承来的三个变量 → 过 `free_text_gate` /
    //   `inherited_gate`（拒绝集只收 NUL / CR / LF，住 `shell_quote_core::free_text_ok`）。剩下的见第四列。
    // resume 接上已在跑的那一个（`Plan::Rejoin`）：名字是 tmux 自己在快照里报的（不是外部输入），只经这一处 quote（同 `Plan::Attach` 那一形）。
    (
        "src/backend/control/ccm/plan.rs",
        &["build_among", "join_session", "qarg", "relay_word", "render", "render_container", "render_container_tail", "render_direct"],
        &[
            ("src/backend/control/ccm/plan.rs", "validate_tmux_name"),
            ("src/backend/control/ccm/plan.rs", "free_text_gate"),
            ("src/backend/control/ccm/plan.rs", "inherited_gate"),
            (
                "src/common/shell-quote-core/src/lib.rs",
                "free_text_ok",
            ),
            // 〔§47 ①〕`--ccm-sid` 在 `argv.rs::validate` 进门判（判定住共享 crate）。
            (
                "src/common/shell-quote-core/src/lib.rs",
                "session_id_ok",
            ),
            (
                "src/common/shell-quote-core/src/lib.rs",
                "account_name_ok",
            ), // `--account`
            // 〔§47 ②〕账号配置目录走全表：全表整份搬进共享 crate（`control → observe` 那条禁止边不用破）。
            ("src/common/acct-core/src/lib.rs", "config_dir_ok"),
        ],
        "",
        "tmux 目标 `=名:` 的形 · 两个提示格式串常量 · cc-bus 脚本路径 · 本侧拼好的载荷 · 本侧上游选择出的中转地址（口是常量，路由段过段闸）",
    ),
    (
        "src/backend/control/tmux_hook.rs",
        &["hook_set_args"],
        &[],
        "",
        "本进程自己的可执行文件路径 · 本侧拼的 hook 命令",
    ),
    // 〔§47〕路径那一格在 `register` 进门判；落点常量之后只跟本侧旗标。
    (
        "src/backend/dial/remote_ask.rs",
        &["command_line"],
        &[],
        "",
        "落点常量后只跟本侧旗标（`--<帧命令>` · `--stdin-line` · 资产目录两旗）",
    ),
    // 〔09-28 裁 2〕`acct_iso_deploy.rs` 那一行删了：它唯一一处拼 shell（跑安装脚本）随「落进用户目录进那台后端」退役。
    // 〔§47〕cwd（`shell_quote_core::posix_free_path_ok`）· 透传参数（`free_text_ok`）进门判；剩下的见第四列。
    // 就地 resume 回落那一形外层包一层 tmux —— 目标 `=名:`（名字过 `gate_rules::existing_tmux_name_issue`）· 键进 pane 的那一行直路 `ccm …`（本侧渲好的整串）。
    (
        "src/backend/control/launch_render/ccm_invocation.rs",
        &["argv", "render_parts"],
        &[
            (
                "src/common/shell-quote-core/src/lib.rs",
                "posix_free_path_ok",
            ),
            (
                "src/common/shell-quote-core/src/lib.rs",
                "free_text_ok",
            ),
            // 〔§47 ①〕resume 的 sid · `--ccm-sid=` · `--model` · `--account`：`Refusal::IdentifierRefused`。
            (
                "src/common/shell-quote-core/src/lib.rs",
                "session_id_ok",
            ),
            (
                "src/common/shell-quote-core/src/lib.rs",
                "model_name_ok",
            ),
            (
                "src/common/shell-quote-core/src/lib.rs",
                "account_name_ok",
            ),
            ("src/common/acct-core/src/lib.rs", "config_dir_ok"), // `--account-dir`
            ("src/backend/control/gate_rules.rs", "existing_tmux_name_issue"), // 就地 resume 的目标
        ],
        "",
        "本侧渲染好的那一行直路 `ccm …`（就地 resume 回落那一形键进 pane 的整串）",
    ),
    // `launch_render/payload.rs` 那一行（12）随文件删了：起会话只交一行 `ccm …`，载荷那一层整层删了。
    // monitor `tmux.rs` 那一行（Gate 1 前检 ＋ `exact_target` 的 quote，只剩跨轨锚点在用）随整份文件删了：门只在后端。
    // `src/frontend/filewin/src/shell.rs` 那一行出表：文件窗口只交意图（当前目录），`cd` 那一串随拼法搬进本机后端
    //   `dial/terminal.rs::command_for_cwd`（两道放行判定跟着过去）。
    // `src/frontend/shell/src/launch.rs` 那一行出表：远端那条 ssh 外壳（包一层 `bash -lic`）随渲染进了本机后端。
    // 文件窗口「在此打开终端」的当前目录（自由文本路径）在这里拼进 `cd`：拼之前过 `posix_free_path_ok`（POSIX 绝对 · 无 `..` 段 · 不含 NUL / CR / LF）；非 UTF-8 的走字节形 `posix_quote_bytes`，过 `posix_free_path_bytes_ok`。
    // 开终端那一行多一种方言（本机是 POSIX 时整串按 POSIX 单引号嵌，`literal`），拼进去的东西不变。
    (
        "src/backend/dial/terminal.rs",
        &["command_for_cwd", "literal", "render"],
        &[
            (
                "src/common/shell-quote-core/src/lib.rs",
                "posix_free_path_ok",
            ),
            (
                "src/common/shell-quote-core/src/lib.rs",
                "posix_free_path_bytes_ok",
            ),
        ],
        "",
        "开终端那一行里要在远端跑的整条命令（拼它的那几处各自判过；这里只包一层 `bash -lic`，本侧再判控制符 · 双引号 · 长度）",
    ),
    // 收一整串的终端（Tilix）：开窗那一跳把 `bash -lic <留窗脚本> bash <命令>` 拼成一个参数交给它（它按 shell 词法切回去）。
    //   命令是本机后端渲好的成品，开窗之前过 `validate_launch_cmd`（控制符 · 长度 · 空）；留窗脚本是常量。
    (
        "src/frontend/shell/src/platform/terminal.rs",
        &["build_local_posix_spawn"],
        &[("src/frontend/shell/src/launch.rs", "validate_launch_cmd")],
        "",
        "留窗脚本（常量）",
    ),
    // 那条命令随部署判定搬进共享的 `deploy-core`（本机常驻后端出计划时拼、在那台上跑）：`sftp.rs` 出列、这一行换住址。
    // `deploy-core` 拆开：扫戳命令是戳格式（契约），随契约那一半住 `deploy-contract`。
    (
        "src/common/deploy-contract/src/lib.rs",
        &["stamp_scan_cmd"],
        &[],
        "",
        "身份戳正则（构建期常量拼的）",
    ),
    // 别名文件里每条只有名字（`名字() { ccm @名字 "$@"; }`），规则住配置文件；名字先过 `profile::name_ok`
    //   （两种 shell 都认得的命令名 `[A-Za-z_][A-Za-z0-9_]*`），过不了的那一段不进别名文件。
    (
        "src/backend/platform/shell/dialect.rs",
        &["word"],
        &[("src/backend/assets/aliases/profile.rs", "name_ok")],
        "",
        "我们那份别名文件的路径",
    ),
    // 问起会话那个 shell 的 `PATH`：quote 进去的只有一个常量标记（`SESSION_PATH_MARK`），不收任何外来值。
    (
        "src/backend/platform/shell/mod.rs",
        &["login_shell_asking_path"],
        &[],
        "",
        "夹住 `PATH` 的那个常量标记",
    ),
    // cc-bus 钩子要加的内容：两条钩子指向这台 skills 根下那两个脚本；skills 根不在家目录底下时那条路径整份 quote（是这台后端自己的路径）。
    (
        "src/backend/observe/cc_bus_hooks.rs",
        &["snippet"],
        &[],
        "",
        "这台后端自己的 skills 根下那两个脚本的路径",
    ),
    // 「要你动手」旧 ccm 那一件：复制给人自己跑的 `rm <路径>`，路径是这台 PATH 上找到的那个文件（这台自己的路径，不进任何会跑的串）。
    (
        "src/backend/footprint/chores/mod.rs",
        &["chores"],
        &[],
        "",
        "这台 PATH 上先找到的那个 ccm 的路径（只复制给人，不拿去跑）",
    ),
    // stream_source/exec.rs 2 → 0 · remote_resident.rs 2 → 0：远端后端落点是固定常量 `BACKEND_CMD`（`backendPath` 那一格删了），
    //   流 / 探针 / 常驻起停四条命令不再 quote 任何外来值 ⇒ 两行出列。
];

/// `INVARIANTS §47` 人群判据：盘上每一份调 quote 的 Rust 生产文件 × 外层函数 == 登记表（两向）；
/// 登记里点名的放行判定逐个在那份文件里声明着。
#[test]
fn every_file_that_quotes_a_value_into_a_shell_line_is_registered() {
    let (on_disk, names) = quote_sites();
    assert!(
        // monitor `stream_source` 那层转调壳删了（零生产调用方）⇒ 认得出的别名只剩后端 `tmux_hook::sq`；本体名照旧在。
        names.iter().any(|n| n == "sq") && names.iter().any(|n| n == "posix_quote"),
        "别名认法坏了：{names:?}"
    );
    let registered: BTreeMap<String, BTreeSet<String>> = QUOTE_SITES
        .iter()
        .map(|&(f, fns, _, _, _)| (f.to_string(), fns.iter().map(|x| x.to_string()).collect()))
        .collect();
    assert_eq!(registered.len(), QUOTE_SITES.len(), "登记表里有重复文件");
    assert_eq!(
        on_disk, registered,
        "`INVARIANTS §47` 的人群（把值 quote 进 shell 串的生产文件 × 外层函数）与登记表对不上（两向）。\n\
         多出来的 = 新长的拼接点：说清拼进去的是什么、外部值靠哪个放行判定（`§47` ① / ②）；登记了而盘上没有 = 删了 / 改了，跟着改表。"
    );
    let root = crate::guard_support::repo_root();
    for &(f, _, checked, quote_only, ours) in QUOTE_SITES {
        assert!(
            !checked.is_empty() || !quote_only.is_empty() || !ours.is_empty(),
            "{f}：三列全空 —— 没说清拼进去的是什么"
        );
        for &(file, func) in checked {
            let text = std::fs::read_to_string(root.join(file))
                .unwrap_or_else(|e| panic!("{f} 点的判定住址 {file} 读不到：{e}"));
            assert!(
                text.contains(&format!("fn {func}(")),
                "{f} 点的放行判定 `{file}::{func}` 不在那份文件里 —— 改名 / 搬家之后登记没跟上"
            );
        }
    }
    // 「有外部值只靠 quote」的文件今天是零：外部值都先过放行判定（`§47` ① / ②）。多了是新缺口。
    let open: Vec<&str> = QUOTE_SITES
        .iter()
        .filter(|r| !r.3.is_empty())
        .map(|r| r.0)
        .collect();
    assert!(
        open.is_empty(),
        "这些文件有外部值只靠 quote、没过放行判定：{open:?}"
    );
}

// `every_read_of_the_backend_path_field_is_registered`〔散文墓碑〕 与它的登记表删了：`RemoteConfig` 没有 `backend_path` 这一格了
//   （落点恒是 `relay_route_core::BACKEND_LANDING_SHELL`），「裸读那个字段」这一形编译期就不存在。
