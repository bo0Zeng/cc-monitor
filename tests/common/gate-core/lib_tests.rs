use super::*;

#[test]
fn old_cc_prefix_is_still_recognised() {
    // 删掉这一支 = 把用户正在跑的老会话变成失管会话（见 `is_ccm_tmux_name` 头注）。
    assert!(is_ccm_tmux_name("cc-abc12345"));
    assert!(is_ccm_tmux_name("cc-proj"));
    assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
    assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
}

#[test]
fn new_cc_suffix_shape_is_recognised() {
    assert!(is_ccm_tmux_name("abc12345-cc"));
    assert!(is_ccm_tmux_name("abc12345-cc-2"));
    assert!(is_ccm_tmux_name("my-proj-cc"));
    assert!(!is_ccm_tmux_name("-cc")); // `<X>` 为空的退化名
    assert!(!is_ccm_tmux_name("foo-ccx"));
    assert!(!is_ccm_tmux_name("foo-cc-bar")); // `-cc-` 后面不是纯数字
}

#[test]
fn other_peoples_sessions_are_not_ours() {
    assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
    assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
    assert!(!is_ccm_tmux_name("foo_cc")); // cc-bus 的 `_cc` 命名空间（ROADMAP §6 待决 #2）
}

#[test]
fn shell_metacharacters_never_pass_the_name_half() {
    // 这几条守的是「名字命中 ⇒ 跳过远端核验」那条零 IO 快路。
    assert!(!is_ccm_tmux_name("cc-a b")); // 空格
    assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
    assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
    assert!(!is_ccm_tmux_name("cc-a:b")); // tmux 目标语法
    assert!(!is_ccm_tmux_name("=cc-a")); // tmux 精确匹配前缀
}

#[test]
fn the_union_allows_by_name_without_asking_the_remote() {
    // 名字命中时**连 remote_sid 都不看** —— 三种取值结论必须一致。
    for sid in [None, Some(""), Some("whatever")] {
        assert_eq!(gate2("cc-abc12345", sid), Gate2::AllowedByName);
    }
    assert!(!needs_remote_sid("cc-abc12345"));
}

#[test]
fn the_union_allows_a_custom_name_only_when_the_remote_sid_is_set() {
    assert!(needs_remote_sid("e2e-custom"));
    assert_eq!(
        gate2("e2e-custom", Some("abc123")),
        Gate2::AllowedByRemoteSid
    );
    assert_eq!(gate2("e2e-custom", Some("")), Gate2::Rejected);
    assert_eq!(gate2("e2e-custom", None), Gate2::Rejected);
}

#[test]
fn unknown_state_fails_closed() {
    // 探测失败（`None`）绝不能等价于「允许」。这条是 fail-closed 的落点。
    assert!(!gate2("someones-session", None).allowed());
    assert!(!gate2("someones-session", Some("")).allowed());
}

#[test]
fn the_wire_names_are_pinned_not_derived_from_debug() {
    // 夹具与 e2e 都比这几个字面量；跟着 `Debug` 漂就成了「两侧一起漂」。
    assert_eq!(Gate2::AllowedByName.as_str(), "allowed_by_name");
    assert_eq!(Gate2::AllowedByRemoteSid.as_str(), "allowed_by_remote_sid");
    assert_eq!(Gate2::Rejected.as_str(), "rejected");
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔DUP2 · 主会话 09-26 裁 J6〕tmux 会话名的两条规则（新建 · 已有会话）—— 全仓唯一一份。
// 守的要求：`设计/90 §3` 判据 2 · `INVARIANTS §47` ①（新建）/ ②（attach 目标，V131）· F01「不把 glob 建进会话名」。
// 正反各一格（`§47`：「拒过头也算违反」—— 只断坏的被拒，焊成恒拒也能绿）。
// ═══════════════════════════════════════════════════════════════════════════

/// 新建：真实会走的名字全过；`-` 开头 · 目标语法 / glob · 控制符 · 欺骗字符 · 超长 各拒、各报对的那一类。
#[test]
fn a_new_session_name_passes_real_names_and_refuses_what_would_confuse_tmux() {
    let at_max = "a".repeat(NEW_TMUX_NAME_MAX);
    for ok in [
        "proj-cc",
        "proj-cc-2",
        "cc-abc12345",
        "my session",
        "项目 一",
        "a_b",
        at_max.as_str(),
    ] {
        assert_eq!(new_tmux_name_issue(ok), None, "{ok:?} 该建得出来");
    }
    let too_long = "a".repeat(NEW_TMUX_NAME_MAX + 1);
    let cases: Vec<(&str, TmuxNameIssue)> = vec![
        ("", TmuxNameIssue::Empty),
        ("-x", TmuxNameIssue::LeadingDash),
        ("--help", TmuxNameIssue::LeadingDash),
        ("a*b", TmuxNameIssue::TargetSyntax('*')),
        ("a?b", TmuxNameIssue::TargetSyntax('?')),
        ("a.b", TmuxNameIssue::TargetSyntax('.')),
        ("a:b", TmuxNameIssue::TargetSyntax(':')),
        ("proj=x", TmuxNameIssue::TargetSyntax('=')),
        ("a\tb", TmuxNameIssue::Control('\t')),
        ("a\nb", TmuxNameIssue::Control('\n')),
        ("a\u{85}b", TmuxNameIssue::Control('\u{85}')),
        ("a\u{202e}b", TmuxNameIssue::Deceptive('\u{202e}')),
        ("a\u{3000}b", TmuxNameIssue::Deceptive('\u{3000}')),
        (too_long.as_str(), TmuxNameIssue::TooLong),
    ];
    for (bad, want) in cases {
        assert_eq!(new_tmux_name_issue(bad), Some(want), "{bad:?}");
    }
    // 禁字集里恰好是这五个（跨轨判据 `backend_kill_tests.rs` 钉的就是这个表达式）。
    assert_eq!(NEW_TMUX_NAME_REFUSED, "*?.:=");
}

/// 已有会话（attach / 送进已在的会话）：只拒空 · 控制符 · 欺骗字符。用户真有带 glob / `=` 的会话名，照接；
/// 寻址由调用方走 `=<名>:`（`*` `?` 不被当通配、前导 `-` 不被当选项）。
#[test]
fn an_existing_session_name_is_refused_only_for_what_quote_and_exact_match_cannot_hold() {
    for ok in [
        "st*ar",
        "a?b",
        "a=b",
        "-x",
        "a.b",
        "my session",
        &"a".repeat(300),
    ] {
        assert_eq!(existing_tmux_name_issue(ok), None, "{ok:?} 该接得上");
    }
    assert_eq!(existing_tmux_name_issue(""), Some(TmuxNameIssue::Empty));
    assert_eq!(
        existing_tmux_name_issue("a\rb"),
        Some(TmuxNameIssue::Control('\r'))
    );
    assert_eq!(
        existing_tmux_name_issue("a\u{200b}b"),
        Some(TmuxNameIssue::Deceptive('\u{200b}'))
    );
}
