//! 配置文件（`profiles.toml`）：每一段是一组 ccm 选项、可写「基于」另一段；继承链父 → 子合并，命令行当场给的最后盖。
//! 词与命令行同一张表、同一个解析器（`control/ccm/argv.rs`）。

use super::*;
use crate::control::ccm::argv::{flag, Parsed};

const BOOK: &str = r#"# 我的 ccm 配置（手写的注释要留住）
[cc]
cwd-if = [["~", "~/projects/notes"]]

[cct]
from = "cc"   # 在 cc 上加 tmux
ccm-tmux = true

[teamcct]
from = "cct"
account = "team"
"#;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn resolved(book: &str, name: &str, args: &[&str]) -> Resolved {
    let b = parse_book(book);
    match resolve(&b, name, &v(args)) {
        Ok(r) => r,
        Err(e) => panic!("{name} 应当能用，却报：{e}"),
    }
}

fn opts_of(r: &Resolved) -> &crate::control::ccm::argv::Opts {
    match &r.parsed {
        Parsed::Opts(o) => o,
        Parsed::Early(e) => panic!("不该是立即结束的那一形：{e:?}"),
    }
}

fn refused(book: &str, name: &str) -> String {
    match resolve(&parse_book(book), name, &[]) {
        Ok(_) => panic!("{name} 应当被拒"),
        Err(e) => e,
    }
}

fn origin_of<'a>(r: &'a Resolved, f: &str) -> &'a str {
    r.origins
        .iter()
        .find(|o| o.flag == f)
        .map(|o| o.layer.as_str())
        .unwrap_or_else(|| panic!("合并结果里没有 {f}"))
}

#[test]
fn teamcct_is_the_dir_rule_of_cc_plus_the_tmux_of_cct_plus_its_own_account() {
    let r = resolved(BOOK, "teamcct", &[]);
    let o = opts_of(&r);
    assert_eq!(
        o.cwd_if,
        vec![("~".to_string(), "~/projects/notes".to_string())]
    );
    assert!(o.use_tmux, "cct 的 tmux 要传下来");
    assert_eq!(o.account, "team");
    assert!(o.passthru.is_empty());
    assert_eq!(origin_of(&r, flag::CWD_IF), "cc");
    assert_eq!(origin_of(&r, flag::TMUX), "cct");
    assert_eq!(origin_of(&r, flag::ACCOUNT), "teamcct");
}

#[test]
fn words_after_the_name_reach_claude_untouched_and_options_after_dashdash_win() {
    let r = resolved(
        BOOK,
        "teamcct",
        &["你好", "--model", "opus", "--", "--account", "x"],
    );
    let o = opts_of(&r);
    assert_eq!(o.passthru, v(&["你好", "--model", "opus"]));
    assert_eq!(o.account, "x", "命令行当场给的盖住配置");
    assert_eq!(origin_of(&r, flag::ACCOUNT), "", "空名 ＝ 命令行那一层");
    assert!(o.use_tmux);
}

#[test]
fn agent_words_in_the_file_go_first_parent_before_child_then_the_command_line() {
    let book = "[a]\nargs = [\"--model\", \"opus\"]\n[b]\nfrom = \"a\"\nargs = [\"--verbose\"]\n";
    let r = resolved(book, "b", &["hi"]);
    assert_eq!(
        opts_of(&r).passthru,
        v(&["--model", "opus", "--verbose", "hi"])
    );
}

#[test]
fn the_childs_dir_rules_are_tried_before_the_parents() {
    let book = "[a]\ncwd-if = [\"~\", \"/a\"]\n[b]\nfrom = \"a\"\ncwd-if = [[\"~\", \"/b\"], [\"/x\", \"/y\"]]\n";
    let r = resolved(book, "b", &[]);
    assert_eq!(
        opts_of(&r).cwd_if,
        vec![
            ("~".to_string(), "/b".to_string()),
            ("/x".to_string(), "/y".to_string()),
            ("~".to_string(), "/a".to_string()),
        ]
    );
}

#[test]
fn account_and_base_are_one_slot_so_the_child_replaces_the_parent() {
    let book = "[a]\nbase = true\n[b]\nfrom = \"a\"\naccount = \"z\"\n";
    let o = resolved(book, "b", &[]);
    let o = opts_of(&o);
    assert_eq!(o.account, "z");
    assert!(!o.use_base);
}

#[test]
fn a_cycle_is_refused_and_names_the_loop() {
    let e = refused("[a]\nfrom = \"b\"\n[b]\nfrom = \"a\"\n", "a");
    assert!(e.contains("a") && e.contains("b"), "{e}");
    assert_eq!(e, refused("[a]\nfrom = \"b\"\n[b]\nfrom = \"a\"\n", "a"));
    let me = refused("[a]\nfrom = \"a\"\n", "a");
    assert!(me.contains('a'), "{me}");
}

#[test]
fn a_missing_base_is_refused_at_its_line() {
    let b = parse_book("# x\n[x]\nfrom = \"nope\"\n");
    let e = resolve(&b, "x", &[]).unwrap_err();
    assert!(e.contains("nope"), "{e}");
    assert!(e.contains('3'), "要报到第 3 行：{e}");
}

#[test]
fn an_unknown_name_is_refused() {
    let e = refused(BOOK, "zzz");
    assert!(e.contains("zzz"), "{e}");
}

#[test]
fn a_word_ccm_does_not_know_is_reported_at_its_line() {
    let b = parse_book("[x]\naccount = \"b\"\nbogus = true\n");
    let p = b
        .problems
        .iter()
        .find(|p| p.message.contains("bogus"))
        .expect("bogus 要报出来");
    assert_eq!(p.line, Some(3));
    assert_eq!(p.profile.as_deref(), Some("x"));
    assert!(resolve(&b, "x", &[]).is_err(), "写错的那一段此刻不能用");
}

#[test]
fn a_switch_written_false_is_refused_not_silently_ignored() {
    let b = parse_book("[x]\nccm-tmux = false\n");
    assert_eq!(b.problems.len(), 1, "{:?}", b.problems);
    assert_eq!(b.problems[0].line, Some(2));
}

#[test]
fn a_value_the_parser_refuses_is_reported_at_its_line() {
    // `--tmux-size` 只认 `<宽>x<高>`：同一个解析器拒、报到那一行。
    let b = parse_book("[x]\nccm-tmux = true\n\ntmux-size = \"big\"\n");
    let e = resolve(&b, "x", &[]).unwrap_err();
    assert!(e.contains('4'), "{e}");
    let rel = parse_book("[x]\ncwd = \"rel/dir\"\n");
    assert_eq!(rel.problems.len(), 1, "目录要钉住：{:?}", rel.problems);
}

#[test]
fn a_toml_syntax_error_is_reported_at_its_line_and_blocks_every_profile() {
    let b = parse_book("[cc]\naccount = \"b\"\n[cct\n");
    assert!(b.profiles.is_empty());
    assert_eq!(b.problems.len(), 1);
    assert_eq!(b.problems[0].line, Some(3));
}

/// 写错的那一句只说「格式错误」；TOML 解析器的原话不上句子，另带（进复制详情）。
#[test]
fn a_toml_syntax_error_keeps_the_parser_words_out_of_the_sentence() {
    let b = parse_book("[cc]\naccount = \"b\"\n[cct\n");
    let p = &b.problems[0];
    assert_eq!(p.message, copy_text("beProfile.file.syntax", &[]));
    let raw = p.raw.as_deref().expect("解析器原话另带");
    assert!(
        !raw.trim().is_empty() && !p.message.contains(raw.trim()),
        "{p:?}"
    );
}

#[test]
fn a_broken_profile_does_not_block_an_unrelated_one() {
    let b = parse_book("[ok]\naccount = \"b\"\n[bad]\nbogus = 1\n[child]\nfrom = \"bad\"\n");
    assert!(resolve(&b, "ok", &[]).is_ok());
    assert!(resolve(&b, "bad", &[]).is_err());
    assert!(
        resolve(&b, "child", &[]).is_err(),
        "基于写错的那一段 ⇒ 也不能用"
    );
}

#[test]
fn profile_names_must_be_command_names_and_not_the_backend_itself() {
    for bad in ["[\"a b\"]\n", "[ccm]\n", "[cc-monitor-backend]\n"] {
        let b = parse_book(bad);
        assert_eq!(b.problems.len(), 1, "{bad:?} ⇒ {:?}", b.problems);
    }
}

/// 配置里的每一项就是一个 ccm 选项：键 ＝ `--` 后面那个词；除了「基于」（命令行上的 `@名`）与「交给 agent 的词」
/// （命令行上 `--` 左边那串），没有只在配置里有的项；ccm 能放在 `--` 右边的词，除了每次取值都不同的那几个，都能写进配置。
#[test]
fn the_file_speaks_exactly_the_command_lines_words() {
    use crate::control::ccm::argv::is_ccm_word;
    let per_call = [
        flag::NEW,
        flag::ATTACH,
        flag::CCM_PRINT,
        flag::CCM_HELP,
        flag::CCM_VERSION,
        flag::CCM_PROBE,
        flag::CCM_SID,
        flag::ACCOUNT_DIR,
    ];
    let keys: Vec<&str> = PROFILE_FLAGS.iter().map(|f| key_of(f)).collect();
    for k in &keys {
        assert!(is_ccm_word(&format!("--{k}")), "配置里的 {k} 不是 ccm 的词");
    }
    let all = crate::control::ccm::argv::RIGHT_WORDS;
    let want: Vec<String> = all
        .iter()
        .filter(|w| !per_call.contains(*w))
        .map(|w| w.trim_start_matches("--").to_string())
        .collect();
    let mut have: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
    let mut want = want;
    have.sort();
    want.sort();
    assert_eq!(have, want, "两边词表要相等");
    assert!(!have.contains(&FROM_KEY.to_string()) && !have.contains(&ARGS_KEY.to_string()));
}

#[test]
fn the_command_line_form_and_the_file_form_say_the_same_thing() {
    let by_file = resolved(BOOK, "teamcct", &["你好"]);
    let by_cli = crate::control::ccm::argv::parse(&v(&[
        "你好",
        "--",
        "--cwd-if",
        "~",
        "~/projects/notes",
        "--ccm-tmux",
        "--account",
        "team",
    ]))
    .unwrap();
    let (Parsed::Opts(a), Parsed::Opts(b)) = (&by_file.parsed, &by_cli) else {
        panic!()
    };
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn editing_one_profile_keeps_the_hand_written_comments_and_the_other_profiles_bytes() {
    let mut f = edit_start(BOOK).unwrap();
    set_profile(
        &mut f,
        &ProfileEdit {
            name: "cct".into(),
            from: Some("cc".into()),
            agent: Vec::new(),
            ccm: v(&["--ccm-tmux", "--account", "z"]),
        },
    )
    .unwrap();
    let out = f.to_string();
    assert!(
        out.starts_with(
            "# 我的 ccm 配置（手写的注释要留住）\n[cc]\ncwd-if = [[\"~\", \"~/projects/notes\"]]\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("from = \"cc\"   # 在 cc 上加 tmux"),
        "没动的那一项连同行尾注释原样：{out}"
    );
    assert!(out.contains("account = \"z\""), "{out}");
    assert!(
        out.contains("[teamcct]\nfrom = \"cct\"\naccount = \"team\"\n"),
        "{out}"
    );
    let back = parse_book(&out);
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert_eq!(opts_of(&resolved(&out, "teamcct", &[])).account, "team");
}

#[test]
fn a_new_profile_is_appended_and_a_removed_one_is_gone() {
    let mut f = edit_start(BOOK).unwrap();
    set_profile(
        &mut f,
        &ProfileEdit {
            name: "workcc".into(),
            from: Some("cc".into()),
            agent: Vec::new(),
            ccm: v(&["--account", "work", "--cwd-if", "/a", "/b"]),
        },
    )
    .unwrap();
    remove_profile(&mut f, "teamcct");
    let out = f.to_string();
    let b = parse_book(&out);
    assert!(b.problems.is_empty(), "{:?}\n{out}", b.problems);
    assert!(b.find("teamcct").is_none());
    let r = resolved(&out, "workcc", &[]);
    let o = opts_of(&r);
    assert_eq!(o.account, "work");
    assert_eq!(
        o.cwd_if,
        vec![
            ("/a".to_string(), "/b".to_string()),
            ("~".to_string(), "~/projects/notes".to_string())
        ]
    );
}

#[test]
fn a_profile_reads_back_as_its_own_layer_in_command_line_words() {
    let b = parse_book(BOOK);
    let p = b.find("teamcct").unwrap();
    assert_eq!(p.from.as_deref(), Some("cct"));
    assert_eq!(p.ccm_words(), v(&["--account", "team"]));
    let cc = b.find("cc").unwrap();
    assert_eq!(cc.ccm_words(), v(&["--cwd-if", "~", "~/projects/notes"]));
}
