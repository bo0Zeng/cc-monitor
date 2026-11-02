//! # 撞名：`--` 右边只认 ccm 表
//!
//! 核原文：「撞名判据改为『右边只认 ccm 表』」。异源：一侧是真解析器 `argv::parse_layered` ＋ 真分流 `route` 的行为，另一侧是 claude 自己的 `--help` ——
//! 固定快照 `tests/__fixtures__/claude-help.snapshot.txt`（首行记版本）；PATH 上有 `claude` 时再加读真的一份（只许跑 `--help`）。

use super::tests::parse;
use super::*;
use std::collections::BTreeSet;

const SNAPSHOT: &str = include_str!("../../__fixtures__/claude-help.snapshot.txt");

/// `--help` 里的旗标：选项行是两格缩进、`-` 开头；取名字那一栏（到两个空格为止），逗号分开的每一形。
fn flags_of(help: &str) -> BTreeSet<String> {
    help.lines()
        .filter(|l| l.starts_with("  -"))
        .flat_map(|l| {
            let name_col = l.trim_start().split("  ").next().unwrap_or("");
            name_col
                .split(", ")
                .filter_map(|t| t.split([' ', '=', '<', '[']).next())
                .filter(|t| t.starts_with('-'))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 要求：「命令格式 `ccm [交给 claude 的…] -- [ccm 自己的…]`：没有 `--` ⇒ 整行原样交 claude；
/// 有 ⇒ 按最后一个 `--` 切……右边全归 ccm……认不得的词直接报错、不猜」「撞名判据改为『右边只认 ccm 表』」。
///
/// 语料异源：claude `--help` 快照（＋ PATH 上真 claude）∪ 后端的两张表 ∪ ccm 自己的词（问真解析器认不认）。
/// ① 没有 `--` ⇒ 零拦截：每个词（单独、与整串混写）都原样进透传，分流也不进后端。
/// ② 右边只认 ccm 表：claude 的词放右边一律报错；ccm 的词放右边都认得。
/// ③ 后端词只紧跟打头的 `--`：`ccm -- <后端词>` 进后端，左边有 claude 参数时报错（不猜）。ccm 的词 ∩ 后端词 = ∅。
#[test]
fn only_the_right_of_the_last_end_is_ccm_and_nothing_is_intercepted_without_it() {
    let version = SNAPSHOT.lines().next().unwrap_or_default().to_string();
    let mut claude = flags_of(SNAPSHOT);
    if let Ok(out) = std::process::Command::new("claude").arg("--help").output() {
        if out.status.success() {
            claude.extend(flags_of(&String::from_utf8_lossy(&out.stdout)));
        }
    }
    for want in [
        "--resume",
        "-r",
        "--continue",
        "--model",
        "-p",
        "--print",
        "--help",
        "--version",
        "--fork-session",
        "--tmux",
        "--agent",
    ] {
        assert!(claude.contains(want), "{version}：抽不出 {want}，抽取坏了");
    }
    let backend: BTreeSet<String> = crate::stream::cli::tests::subcommands()
        .iter()
        .chain(crate::wire::cli_words::STREAM_FLAGS.iter())
        .map(|s| s.to_string())
        .collect();
    let ours: BTreeSet<String> = [
        flag::TMUX,
        flag::TMUX_BASE,
        flag::TMUX_SIZE,
        flag::DETACH,
        flag::ACCOUNT,
        flag::BASE,
        flag::CWD,
        flag::AGENT,
        flag::LAUNCHER,
        flag::ATTACH,
        flag::CCM_PRINT,
        flag::CCM_HELP,
        flag::CCM_VERSION,
        flag::CCM_PROBE,
        flag::CCM_SID,
        flag::NEW, //
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let route =
        |l: &[String]| crate::control::ccm::route("ccm", l, crate::stream::entry::is_backend_word);
    let passes = |line: &[String]| {
        route(line) == crate::control::ccm::Entry::Ccm(line.to_vec())
            && matches!(parse(line), Ok(Parsed::Opts(o)) if o.passthru == line)
    };
    // ①
    let all: Vec<String> = claude
        .iter()
        .chain(&backend)
        .chain(&ours)
        .cloned()
        .collect();
    let intercepted: Vec<&String> = all.iter().filter(|w| !passes(&[w.to_string()])).collect();
    assert_eq!(
        intercepted,
        Vec::<&String>::new(),
        "没有 `--` 时这些词被拦下了（没原样交 claude）"
    );
    assert!(passes(&all), "没有 `--` 的一整串没有原样交 claude");
    // ②
    let end = || flag::END.to_string();
    for w in &ours {
        assert!(is_ccm_word(w), "ccm 自己的词 {w} 放在 `--` 右边不认");
    }
    let taken: Vec<&String> = claude
        .iter()
        .filter(|f| !ours.contains(*f) && is_ccm_word(f))
        .collect();
    assert_eq!(
        taken,
        Vec::<&String>::new(),
        "这些 claude 的词放在 `--` 右边被 ccm 认了（没报错）"
    );
    // `ccm -p -- -x --`：claude 自己的 `--` 照写，末尾空 `--` 表示没有 ccm 部分。
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(
        matches!(parse(&v(&["-p", "--", "-x", "--"])), Ok(Parsed::Opts(o)) if o.passthru == v(&["-p", "--", "-x"]))
    );
    // ③
    for b in &backend {
        assert!(!is_ccm_word(b), "后端词 {b} 也是 ccm 的词");
        assert_eq!(
            route(&[end(), b.clone(), "x".into()]),
            crate::control::ccm::Entry::Backend(vec![b.clone(), "x".into()]),
            "`ccm -- {b}` 没进后端"
        );
        let mixed = vec!["-p".to_string(), end(), b.clone()];
        assert_eq!(
            route(&mixed),
            crate::control::ccm::Entry::Ccm(mixed.clone())
        );
        assert!(parse(&mixed).is_err(), "`ccm -p -- {b}` 没报错");
    }
}

/// 「`ccm -- <词>` 交不交后端」只听入口交下来的那一问：同一行，判定说不认 ⇒ ccm 这一趟；说认 ⇒ 后端、`--` 之后那一串原样交过去。
#[test]
fn route_asks_the_entry_whether_a_word_belongs_to_the_backend() {
    use crate::control::ccm::{route, Entry};
    let line: Vec<String> = ["--", "--commands"].iter().map(|s| s.to_string()).collect();
    assert_eq!(route("ccm", &line, |_| false), Entry::Ccm(line.clone()));
    assert_eq!(
        route("ccm", &line, |w| w == "--commands"),
        Entry::Backend(vec!["--commands".to_string()])
    );
}

/// 自己的名字认的是这份二进制的名字（`ccm` · `cc-monitor-backend` · 落盘的 `cc-monitor-backend-<build>`），
/// 不是编进这一段的那个包叫什么：这一段换了包，认的名字也不跟着变。
#[test]
fn own_names_are_the_binary_names_whatever_package_this_code_sits_in() {
    use crate::ccm_line::is_own_name;
    for own in ["ccm", "cc-monitor-backend", "cc-monitor-backend-p11b"] {
        assert!(is_own_name(own), "`{own}` 没认成自己");
    }
    for other in ["claude", "backend-host", "cc-monitor"] {
        assert!(!is_own_name(other), "`{other}` 被认成了自己");
    }
}
