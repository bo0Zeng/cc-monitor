use super::*;

#[test]
fn posix_quote_breaks_single_quotes_the_posix_way() {
    assert_eq!(posix_quote("/p"), "'/p'");
    assert_eq!(posix_quote("a'b"), "'a'\\''b'");
    assert_eq!(posix_quote(""), "''");
}

/// 〔TL3 · `INVARIANTS §47` ②〕自由文本的拒绝集：恰好 NUL / CR / LF（正反各一格 —— §47「拒过头也算违反」）。
/// 要求住址：`INVARIANTS §47` ②「走唯一的 quote ＋ 形式判定 ＋ 拒绝集」；主会话 09-26 按 V131 裁「自由文本路径的拒绝集只收控制字符（NUL / CR / LF）……不拒 shell 元字符」。
#[test]
fn free_text_refuses_exactly_nul_cr_lf_and_lets_real_names_through() {
    for good in [
        "/home/u/Bob's notes",
        "/data/照片 (2019)",
        "a & b; c | d $x `y` *?!<>\"",
        "tab\tinside",
        "",
    ] {
        assert!(free_text_ok(good), "真实好值被拒了：{good:?}");
    }
    for bad in ["a\0b", "a\rb", "a\nb", "\n"] {
        assert!(!free_text_ok(bad), "坏值放行了：{bad:?}");
    }
    assert_eq!(FREE_TEXT_REFUSED, ['\0', '\r', '\n']);
}

/// 〔TL3 · `INVARIANTS §47` ②〕POSIX 自由文本路径：形式（绝对 · 无 `..` 段）＋ 拒绝集，**正反各一格**。
#[test]
fn a_posix_free_path_is_absolute_without_parent_segments_and_nothing_quote_cannot_hold() {
    for good in [
        "/",
        "/home/u/Bob's notes",
        "/data/照片 (2019)",
        "/a..b/c...",
        "/srv/a&b;c",
    ] {
        assert!(posix_free_path_ok(good), "真实好值被拒了：{good:?}");
    }
    for bad in [
        "", "rel", "~/x", "/a/../b", "/a/..", "../a", "/a\nb", "/a\rb", "/a\0b",
    ] {
        assert!(!posix_free_path_ok(bad), "坏值放行了：{bad:?}");
    }
}

/// 〔DUP1 · `INVARIANTS §47` ①〕sid：今天各处规则的交集，**正反各一格**（§47「拒过头也算违反」）。
/// 要求住址：`设计/90 §3` 判据 2「凡是有对应 `*-core` crate 的判定，TS 侧零实现」· `INVARIANTS §47` ①「字符集白名单（闭集，默认拒）＋ 不许 `-` 开头（选项注入）＋ 有长度上界的就钉上界」。
#[test]
fn a_session_id_is_a_short_plain_token_that_never_starts_with_a_dash() {
    for good in [
        "0473c3a0-1111-2222-3333-444455556666",
        "a",
        "A1-b2",
        &"a".repeat(SESSION_ID_MAX),
    ] {
        assert!(session_id_ok(good), "真实好值被拒了：{good:?}");
    }
    for bad in [
        "",
        "-abc",
        "--dangerously-skip-permissions",
        "a_b",
        "a.b",
        "a/b",
        "../etc",
        "a b",
        "a;b",
        "会话",
        &"a".repeat(SESSION_ID_MAX + 1),
    ] {
        assert!(!session_id_ok(bad), "坏值放行了：{bad:?}");
    }
}

/// 〔DUP1 · `INVARIANTS §47` ①〕模型名：真实模型名全过（主会话 09-26「真实模型名都放行」），**正反各一格**。
#[test]
fn real_model_names_pass_and_option_or_shell_shapes_do_not() {
    for good in [
        "opus",
        "sonnet[1m]",
        "claude-opus-4-5-20260101",
        "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
        "arn:aws:bedrock:us-east-1:123456789012:application-inference-profile/abcdef123456",
        "claude-sonnet-4-5@20250929",
        "anthropic/claude-sonnet-4-5",
    ] {
        assert!(model_name_ok(good), "真实模型名被拒了：{good:?}");
    }
    for bad in [
        "",
        "-x",
        "--model",
        "[1m]",
        ".opus",
        "opus 4",
        "a;b",
        "a$b",
        "a`b",
        "a'b",
        "a\nb",
        "模型",
        &"a".repeat(MODEL_NAME_MAX + 1),
    ] {
        assert!(!model_name_ok(bad), "坏值放行了：{bad:?}");
    }
    assert!(model_name_ok(&"a".repeat(MODEL_NAME_MAX)));
}

/// 〔DUP1 · `INVARIANTS §47` ①〕账号名：与建账号的那个工具（`cc-acct-iso` 的 `name_check`）逐字同，**正反各一格**。
#[test]
fn an_account_name_is_what_the_account_tool_would_have_created() {
    for good in [
        "work",
        "z",
        "acct-a",
        "a_b",
        "A1",
        &"a".repeat(ACCOUNT_NAME_MAX),
    ] {
        assert!(account_name_ok(good), "真实账号名被拒了：{good:?}");
    }
    for bad in [
        "",
        "-a",
        "_a",
        "a.b",
        "a b",
        "a/b",
        "账号",
        &"a".repeat(ACCOUNT_NAME_MAX + 1),
    ] {
        assert!(!account_name_ok(bad), "坏值放行了：{bad:?}");
    }
}
