//! # 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8）· `INVARIANTS §47`（外部值先过放行判定）· `设计/01 §5 D5`
//!
//! 核原文：`§49` 逐字「本仓**每一处**按格式串读 tmux 打印通道的调用点（`list-sessions -F` · `ls -F` · `display-message -p`），
//! 起的 tmux 客户端都必须是 UTF-8 客户端」；`§47` 逐字「一个值只要从本进程外面来 …… 在它被拼进 shell 命令串、
//! 或被交给对端去执行 / 去寻址之前，本侧先过一道按这个值的种类写成的放行判定」；`D5` 逐字「判据的人群要从文件系统全集来，
//! 不从『配置里已经承认的那批』来」。
//!
//! 〔TL2 · 4D · IV1 余〕两节升格时（IV1）都在「买不到」里写着同一句：**没有人群判据** —— 每条判据只管它自己那个模块 /
//! 那个值，新长一处读 tmux 的调用点、新长一处把值拼进 shell 的地方，一条都不会红。本文件补的就是这两个人群：
//! 盘上现扫、与登记表两向相等。登记表里每一行都得**说清它是哪一形**；说不清的（违反、或只靠 quote）如实登记，
//! 不写成豁免 —— 新长出来的那一处因此当场红，逼人回来表态。
//!
//! ⚠ 它们**不替**各调用点自己的判据判「带没带对」：`§49` 那几条按调用点钉旗的位置、`§47` 那几族钉放行判定的正反两格，
//! 都还在原处。这里只管**人群不许漏**。

use std::collections::BTreeMap;

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
        out.len() > 200
            && out
                .iter()
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

/// 〔TL2〕`§49` 的人群登记：`(文件, 子命令, 怎么带, 处数, 为什么)`。
///
/// `Carry::None` 的每一行都是**人群外的真实调用点**：`§49` 字面上它们都算违反。按它读的是什么分两类写清 ——
/// 读出来的字段只可能是 ASCII（pid · 窗口数 · `%N` 形 pane id）⇒ 改写成 `_` 也不改一个字节，今天无害；
/// 读的是**会话名 / 地址**（用户起的会话名可以是中文）⇒ 非 UTF-8 客户端下会被改写成 `_`，**是真违反，待裁**
/// （改它们是后端载荷字节 / 随部署脚本字节的变更，不在本件写区，报主会话）。
/// 〔SH1 · 09-26〕那五处（`BUS_ID_RECIPE` · `cc-register` 登记地址 · `cc-whoami` ×3）按 V121 加了旗、翻成 `Flag`；
/// 今天 `Carry::None` 只剩读 ASCII 的那几处。
const TMUX_PRINT_SITES: &[(&str, &str, Carry, usize, &str)] = &[
    ("src/backend/common/session_snapshot.rs", "list-sessions", Carry::Flag, 1, "argv；`session_snapshot_tests.rs` 钉旗在子命令前"),
    ("src/backend/control/gate.rs", "display-message", Carry::Flag, 1, "argv；`gate_tests.rs` 钉旗在子命令前"),
    ("src/backend/observe/watcher.rs", "ls", Carry::Env, 2, "`sh -c` 一段脚本两支（带 / 不带 timeout），env 一行盖住；`watcher_tests.rs` 钉 env"),
    ("src/backend/observe/watcher.rs", "display-message", Carry::Env, 1, "`sh -c`，同上"),
    // 〔SH1〕monitor `tmux.rs` 那条跨 SSH `ls`（Flag）删了：`list_remote_tmux` 改问那台后端 `tmux-list`（那一趟 `ls` 住 `watcher.rs`，env 形）。
    ("src/backend/control/ccm/plan.rs", "list-panes", Carry::None, 1, "无害：只读 `#{pane_id}`（`%N`，ASCII，单列、不按 TAB 切）—— IV1 报过"),
    ("src/backend/control/ccm/mod.rs", "display-message", Carry::Flag, 1, "〔SH1〕`BUS_ID_RECIPE` 读 `#S`（会话名，可以非 ASCII）⇒ 拼进 pane 的命令串，按表用旗；真跑判据 `backend-cc-bus.sh` [SH1-a]"),
    ("src/shared/cc-bus/scripts/cc-bus-adapt-posix.sh", "display-message", Carry::None, 1, "无害：只读 `#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-kill", "display-message", Carry::None, 2, "无害：`#{pane_pid}` · `#{session_windows}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-kill", "list-panes", Carry::None, 1, "无害：`#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-register", "display-message", Carry::Flag, 1, "〔SH1〕读 `#{session_name}:…` 当登记地址（会话名可以非 ASCII）⇒ 旗；真跑判据 `backend-cc-bus.sh` [SH1-a]"),
    ("src/shared/cc-bus/scripts/cc-register", "display-message", Carry::None, 1, "无害：`#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-agents", "display-message", Carry::None, 1, "无害：`#{pane_pid}`（数字）"),
    ("src/shared/cc-bus/scripts/cc-whoami", "display-message", Carry::Flag, 1, "〔SH1〕读 `#{session_name}` 当身份 ⇒ 旗（非 UTF-8 客户端下中文被改写成 `_`，消毒之后与 UTF-8 客户端下的身份不同）；真跑判据 `backend-cc-bus.sh` [SH1-a]"),
    ("src/shared/cc-bus/scripts/cc-whoami", "list-sessions", Carry::Flag, 1, "〔SH1〕`#{session_id} #{session_name}` ⇒ 旗（同上）"),
    ("src/shared/cc-bus/scripts/cc-whoami", "list-panes", Carry::Flag, 1, "〔SH1〕`#{pane_pid} #{session_name}` ⇒ 旗（同上）"),
    ("src/backend/control/kill.rs", "list-panes", Carry::Flag, 1, "〔SH1 · D-g〕argv；杀之前读这个会话全部 pane 的根进程 pid（数字，照表仍带旗）"),
];

/// 〔TL2〕`INVARIANTS §49` 人群判据：盘上每一处「tmux 打印子命令 ＋ 读打印通道的旗」== 登记表（两向，含处数与带法）。
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
    // 〔FILES2〕唯一的 quote 多了一个字节形（`posix_quote_bytes`，`$'…'`）—— 同一份 quote 的另一形，同样算拼接点。
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

/// 盘上现扫：Rust 生产段里每份文件「调 quote（含纯转发别名）」的处数 —— 去掉声明（`fn 名(`）与别名本体里那一句转发。
fn quote_sites() -> (BTreeMap<String, usize>, Vec<String>) {
    let srcs = production_sources();
    let (names, alias_files) = quote_aliases(&srcs);
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for (rel, body) in &srcs {
        if !rel.ends_with(".rs")
            || rel
                .split('/')
                .take(4)
                .eq(["src", "bridge", "crates", "shell-quote-core"])
        {
            continue;
        }
        let mut n = 0usize;
        for name in &names {
            for at in word_hits(body, name) {
                let after = &body[at + name.len()..];
                if !after.starts_with('(') {
                    continue;
                }
                if body[..at].ends_with("fn ") {
                    continue; // 声明
                }
                n += 1;
            }
        }
        // 别名本体里那一句转发不算调用点（它就是 quote 本身）。
        n -= alias_files.iter().filter(|f| *f == rel).count();
        if n > 0 {
            out.insert(rel.clone(), n);
        }
    }
    (out, names)
}

/// 〔TL2〕`§47` 的人群登记：`(文件, 处数, 本侧放行判定, 只靠 quote 的外部值, 本侧自己的值)`。
///
/// - 第三列点名的放行判定 `(文件, 函数名)` 逐个在盘上核得到（改名 / 搬家红）。
/// - 第四列非空 = 🔴 **有外部值只靠 quote**：`§47` ②形要的「拒绝集 ＋ 形式判定」这一层没有 —— 如实登记、**待裁**，不写成豁免。
/// - 第五列 = 拼进去的**本侧的值**（常量 · 本侧铸的 · 本进程自己的路径 · 本侧已渲染好的整串），`§47` 不管它们。
///
/// 人群 = 生产段里调 quote 的地方（`设计/00 §1.2`「quote 只有一份」⇒ 值进 shell 串的合法入口只有它）。
/// ⚠ 射程：它逮的是「新长一处拼接点」；**不经 quote 的裸插值**（`format!` 直接把值塞进命令串）它看不见；
/// 条件 quote 的包装（`plan.rs::qarg` · `ccm_invocation.rs` 的 `argv` · `shell_dialect.rs` 的 `word` · `payload.rs` 的 `token` / `exact`）
/// 在它自己的文件里算一处，经它的那些调用方不再逐个数；第四列是**按文件**写的，不是逐处。
/// TS 那一侧（`posixQuote`）不在人群里 —— LR2 在删那几份 TS 命令构造器，「TS 零 shell 串」是 `设计/90 §3` 那条机械判据的活。
type QuoteRow = (
    &'static str,
    usize,
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
);
const QUOTE_SITES: &[QuoteRow] = &[
    // 〔DUP2 · J4〕cc-acct-iso 步骤那一行（帧命令 `acct-iso-cmd`，原先界面自己拼）：账号名过 `account_name_ok`（§47 ①），
    //   凭据快照路径过 `check_snapshot_path`（规则照界面原样搬：非空 · 无 `"` · 无控制符 · 不以 `-` 开头），再走唯一的 quote。
    (
        "src/backend/accounts/iso.rs",
        6,
        &[
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "account_name_ok",
            ),
            ("src/backend/accounts/iso.rs", "check_snapshot_path"),
        ],
        "",
        "",
    ),
    // 〔TL3 · §47〕那台后端的路径在可达表唯一的写口 `remote_ask::register` 先过放行判定（后端那一份同族判定），第四列清空。
    // 〔W5-AUX · `设计/96 §3.6`〕3 → 1：推那一趟不再把载荷 quote 进命令行（改走 capture 的 stdin 一行，`--stdin-line`），
    //   后端路径那一格改由 `remote_ask::command_line` 拼（那一份文件的处数不变，它本来就逐格 quote argv）⇒ 本文件只剩拉那一趟的路径。
    // 〔E2 · V28〕1 → 0：拉那一趟也改由 `remote_ask::command_line` 拼（落点是固定常量，`backendPath` 那一格删了）⇒ 本行出列。
    // 〔TL3 · §47 · 主会话 09-26 按 V131 裁〕cwd（绝对 · 无 `..` 段）· 启动器 · 透传参数 · 登记备注 · 继承来的三个变量 → 过 `free_text_gate` /
    //   `inherited_gate`（拒绝集只收 NUL / CR / LF，住 `shell_quote_core::free_text_ok`）。剩下的见第四列。
    // 〔AL3 · V138〕29 → 28：`--model` 交给 claude 了，`export ANTHROPIC_MODEL=<quote>` 那一处删；模型名 / resume 的 sid 不再由 ccm 判。
    (
        "src/backend/control/ccm/plan.rs",
        28,
        &[
            ("src/backend/control/ccm/plan.rs", "validate_tmux_name"),
            ("src/backend/control/ccm/plan.rs", "free_text_gate"),
            ("src/backend/control/ccm/plan.rs", "inherited_gate"),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "free_text_ok",
            ),
            // 〔DUP1 · §47 ①〕`--ccm-sid` 在 `argv.rs::validate` 进门判（判定住共享 crate）。
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "session_id_ok",
            ),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "account_name_ok",
            ), // 〔DUP1〕`--account`
            // 〔DUP1 · §47 ②〕账号配置目录走全表：全表整份搬进共享 crate（`control → observe` 那条禁止边不用破）。
            ("src/bridge/crates/acct-core/src/lib.rs", "config_dir_ok"),
        ],
        "",
        "tmux 目标 `=名:` 的形 · 两个提示格式串常量 · cc-bus 脚本路径 · 本侧拼好的载荷",
    ),
    (
        "src/backend/control/tmux_hook.rs",
        2,
        &[],
        "",
        "本进程自己的可执行文件路径 · 本侧拼的 hook 命令",
    ),
    // 〔TL3 · §47〕路径那一格在 `register` 进门判；argv（自由文本）在 `ask_with` 拼之前过拒绝集（只收 NUL / CR / LF，主会话 09-26 按 V131 裁）。
    // 〔E2 · V28〕2 → 1：那台后端的路径不再登记（固定落点常量打头，不 quote）⇒ 只剩 argv 那一格。
    (
        "src/backend/remote_ask.rs",
        1,
        &[(
            "src/bridge/crates/shell-quote-core/src/lib.rs",
            "free_text_ok",
        )],
        "",
        "",
    ),
    (
        "src/bridge/src/acct_iso_deploy.rs",
        1,
        &[(
            "src/bridge/src/acct_iso_deploy.rs",
            "is_safe_remote_acct_iso_dir",
        )],
        "",
        "",
    ),
    // 〔TL3 · §47〕cwd（`shell_quote_core::posix_free_path_ok`）· 透传参数（`free_text_ok`）进门判；剩下的见第四列。
    (
        "src/bridge/src/backend/control/ccm_invocation.rs",
        1,
        &[
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "posix_free_path_ok",
            ),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "free_text_ok",
            ),
            // 〔DUP1 · §47 ①〕resume 的 sid · `--ccm-sid=` · `--model` · `--account`：`Refusal::IdentifierRefused`。
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "session_id_ok",
            ),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "model_name_ok",
            ),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "account_name_ok",
            ),
        ],
        "",
        "",
    ),
    // 〔E2 · V28〕local_backend.rs 1 → 0：远端三行入口的生成器 `ccm_entry_shim`〔散文墓碑〕删了 ⇒ 出列。
    (
        "src/bridge/src/backend/control/payload.rs",
        // 〔E2 · V146〕11 → 12：中转前缀里「用户自己设了端点」那一句话（文案表里的常量句）也过唯一的 quote。
        12,
        &[
            (
                "src/bridge/src/backend/control/payload.rs",
                "config_dir_command_safe",
            ),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "rbind_token_ok",
            ), // 〔DUP3 · J8〕payload.rs 里是 `pub use … as rbind_token_shape_ok`
            (
                "src/bridge/crates/relay-route-core/src/lib.rs",
                "base_url_shape_ok",
            ), // 〔US1〕payload.rs 里是 `pub use … as relay_base_url_shape_ok`
            ("src/bridge/src/backend/control/payload.rs", "check"),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "posix_free_path_ok",
            ), // 〔TL3 · §47〕cwd 两处
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "session_id_ok",
            ), // 〔DUP1 · §47 ①〕外层 `@ccm_sid`（原 `ccm_sid_safe` 收进来）
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "model_name_ok",
            ), // 〔DUP1 · §47 ①〕`export ANTHROPIC_MODEL=`（原「刻意宽容渲染」那一格）
        ],
        "",
        "本侧渲染好的载荷整串 · 〔US1〕中转前缀里的中转口地址与钥匙文件路径（本侧的）",
    ),
    // 〔DUP3〕Gate 1 并进 gate-core 的 tmux 名那一族（已有会话那一条），本侧那个私有谓词删了。
    (
        "src/bridge/src/backend/control/tmux.rs",
        1,
        &[(
            "src/bridge/crates/gate-core/src/lib.rs",
            "existing_tmux_name_issue",
        )],
        "",
        "",
    ),
    // 〔TL3 · §47〕文件窗口的当前目录（自由文本路径）拼进 `cd` 之前过 `posix_free_path_ok`（POSIX 绝对 · 无 `..` 段 · 不含 NUL / CR / LF）。
    // 〔FILES2〕1 → 2：非 UTF-8 的当前目录走字节形 `posix_quote_bytes`，拼之前过 `posix_free_path_bytes_ok`（同一组规则的字节形）。
    (
        "src/bridge/src/filewin/shell.rs",
        2,
        &[
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "posix_free_path_ok",
            ),
            (
                "src/bridge/crates/shell-quote-core/src/lib.rs",
                "posix_free_path_bytes_ok",
            ),
        ],
        "",
        "",
    ),
    ("src/bridge/src/history.rs", 1, &[], "", "本侧铸的启动 id"),
    (
        "src/bridge/src/launch.rs",
        1,
        &[],
        "",
        "本侧渲染好的整条远端命令（拼它的那几处各自判过；这里只包一层 `bash -lic`）",
    ),
    (
        "src/bridge/src/pubkey.rs",
        1,
        &[("src/bridge/src/pubkey.rs", "sanitize_public_key")],
        "",
        "",
    ),
    // 〔E2 · V28〕2 → 1：身份扫描那条命令的落点是固定常量的 shell 写法（不再 quote 一条外来路径）⇒ 只剩身份戳正则。
    (
        "src/bridge/src/sftp.rs",
        1,
        &[],
        "",
        "身份戳正则（构建期常量拼的）",
    ),
    // 〔TL3 · §47〕订正：词本身**是判了的** —— 渲染只经 `account_aliases::render`，它先过 `check_alias`（拒控制字符 · `arg_is_passable` · 旗表闭集），
    //   不过的那一条进 `problems`、不渲。那道拒绝集收的是全部控制字符（比主会话 09-26 裁的 NUL / CR / LF 宽一点，既有、没放松）。
    (
        "src/bridge/src/shell_dialect.rs",
        1,
        &[("src/bridge/src/account_aliases.rs", "check_alias")],
        "",
        "我们那份别名文件的路径",
    ),
    // 〔E2 · V28〕ssh_source.rs 2 → 0 · remote_resident.rs 2 → 0：远端后端落点是固定常量 `BACKEND_CMD`（`backendPath` 那一格删了），
    //   流 / 探针 / 常驻起停四条命令不再 quote 任何外来值 ⇒ 两行出列。
];

/// 〔TL2〕`INVARIANTS §47` 人群判据：盘上每一份调 quote 的 Rust 生产文件 == 登记表（两向，含处数）；
/// 登记里点名的放行判定逐个在那份文件里声明着。
#[test]
fn every_file_that_quotes_a_value_into_a_shell_line_is_registered() {
    let (on_disk, names) = quote_sites();
    assert!(
        names.iter().any(|n| n == "sq") && names.iter().any(|n| n == "shell_quote"),
        "别名认法坏了：{names:?}"
    );
    assert!(
        on_disk.values().sum::<usize>() > 30,
        "只扫到 {} 处 quote 调用 —— 认法坏了",
        on_disk.values().sum::<usize>()
    );
    let registered: BTreeMap<String, usize> = QUOTE_SITES
        .iter()
        .map(|&(f, n, _, _, _)| (f.to_string(), n))
        .collect();
    assert_eq!(registered.len(), QUOTE_SITES.len(), "登记表里有重复文件");
    assert_eq!(
        on_disk, registered,
        "`INVARIANTS §47` 的人群（把值 quote 进 shell 串的生产文件）与登记表对不上（两向，含处数）。\n\
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
    // 读数（不是判据）：只靠 quote 的文件有几份 —— 报告里要写这个数，改了会在这里看到。
    let open = QUOTE_SITES.iter().filter(|r| !r.3.is_empty()).count();
    assert_eq!(open, 0, "「有外部值只靠 quote」的文件数变了（登记 0 份；〔DUP1 · §47 ①〕模型名 / sid 住进 shell-quote-core、三处接上 ⇒ payload.rs 出列 ⇒ 3 → 2；账号名进 shell-quote-core、配置目录全表搬进 acct-core ⇒ plan.rs · ccm_invocation.rs 出列 ⇒ 2 → 0；〔LOC1a 合入〕remote_branch.rs 的 exec 那一趟删了 ⇒ 10 → 9；\
        〔TL3 · §47〕`backendPath` 一族补上形式判定 ＋ 拒绝集 ⇒ asset_sync.rs · sftp.rs · ssh_source.rs 三份出列 ⇒ 9 → 6；\
        〔TL3 · 主会话 09-26 按 V131 裁〕自由文本那一层补上 ⇒ remote_ask.rs · filewin/shell.rs · shell_dialect.rs（订正）三份出列 ⇒ 6 → 3，\
        余下 plan.rs · payload.rs · ccm_invocation.rs 卡在模型名 / sid（交 DUP1）与账号名 / 配置目录的家）：多了是新缺口，少了是补上了 —— 改这个数并在提交信息里写清是哪份");
}

// 〔E2 · V28〕`every_read_of_the_backend_path_field_is_registered`〔散文墓碑〕 与它的登记表删了：`RemoteConfig` 没有 `backend_path` 这一格了
//   （落点恒是 `relay_route_core::BACKEND_LANDING_SHELL`），「裸读那个字段」这一形编译期就不存在。
