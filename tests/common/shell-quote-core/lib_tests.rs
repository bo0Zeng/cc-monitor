use super::*;

#[test]
fn posix_quote_breaks_single_quotes_the_posix_way() {
    assert_eq!(posix_quote("/p"), "'/p'");
    assert_eq!(posix_quote("a'b"), "'a'\\''b'");
    assert_eq!(posix_quote(""), "''");
}

/// 〔`INVARIANTS §47` ②〕自由文本的拒绝集：恰好 NUL / CR / LF（正反各一格 —— §47「拒过头也算违反」）。
/// 要求：`INVARIANTS §47` ②「走唯一的 quote ＋ 形式判定 ＋ 拒绝集」；自由文本路径的拒绝集只收控制字符（NUL / CR / LF），不拒 shell 元字符。
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

/// 〔`INVARIANTS §47` ②〕交给 agent 的参数 / 登记备注的拒绝集：恰好 NUL / CR ——
/// 多行初始任务正着放（拒 LF 就是 §47「拒过头也算违反」），元字符交给 quote（正反各一格）。
#[test]
fn an_agent_argument_may_span_lines_but_never_carries_nul_or_cr() {
    for good in [
        "第一行\n第二行",
        "\n",
        "a & b; c | d $x `y` *?!<>\"'",
        "tab\tinside",
        "",
    ] {
        assert!(arg_text_ok(good), "真实好值被拒了：{good:?}");
    }
    for bad in ["a\0b", "a\rb", "a\r\nb", "\r"] {
        assert!(!arg_text_ok(bad), "坏值放行了：{bad:?}");
    }
    assert_eq!(ARG_TEXT_REFUSED, ['\0', '\r']);
}

/// 〔`INVARIANTS §47` ②〕POSIX 自由文本路径：形式（绝对 · 无 `..` 段）＋ 拒绝集，**正反各一格**。
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

/// 〔`INVARIANTS §47` ①〕sid：今天各处规则的交集，**正反各一格**（§47「拒过头也算违反」）。
/// 要求：「凡是有对应 `*-core` crate 的判定，TS 侧零实现」· `INVARIANTS §47` ①「字符集白名单（闭集，默认拒）＋ 不许 `-` 开头（选项注入）＋ 有长度上界的就钉上界」。
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

/// 〔`INVARIANTS §47` ①〕模型名：真实模型名全过，**正反各一格**。
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

/// 〔`INVARIANTS §47` ①〕账号名：与建账号库的后端（`accounts/manage/model.rs::name_ok`）同一份判，**正反各一格**。
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

/// 〔`INVARIANTS §47` ③〕启动器（命令片段）：真实用法全过（带参数 · alias · 路径 · 家目录下），
/// POSIX 与 PowerShell 两边的元字符一个都不进，`~` 只许打头紧跟 `/`；拒的时候回**那一个字符**。正反各一格。
#[test]
fn a_launcher_is_a_command_fragment_from_one_whitelist() {
    for good in [
        "claude",
        "cct",
        "ccr code",
        "claude --dangerously-skip-permissions",
        "cc -p 8",
        "/usr/local/bin/claude",
        "~/bin/claude --x",
        "./node_modules/.bin/claude",
        "my_agent-2.1",
    ] {
        assert_eq!(
            launcher_refused_char(good),
            None,
            "真实启动器被拒了：{good:?}"
        );
    }
    for (bad, c) in [
        ("claude;rm -rf ~", ';'),
        ("claude | tee x", '|'),
        ("claude && x", '&'),
        ("$(evil)", '$'),
        ("`evil`", '`'),
        ("claude > x", '>'),
        ("claude < x", '<'),
        ("cla'ude", '\''),
        ("cla\"ude", '"'),
        ("claude\nrm", '\n'),
        ("claude\rrm", '\r'),
        ("claude\0", '\0'),
        ("claude\t-x", '\t'),
        ("(claude)", '('),
        ("{claude}", '{'),
        ("@claude", '@'),
        ("claude,x", ','),
        ("claude #x", '#'),
        ("a~b", '~'),
        ("~x", '~'),
        ("~", '~'),
        ("~/a~", '~'),
        ("clé", 'é'),
        ("c*", '*'),
    ] {
        assert_eq!(launcher_refused_char(bad), Some(c), "{bad:?}");
    }
}

/// 〔`INVARIANTS §47` ②〕唯一的 quote 的**字节形**（`### FILES2`
/// 「开终端的 `cd` 用 POSIX `$'\xNN'` 转义（唯一 quote 那一处加这一形）」）。
/// 异源：期望是**真 bash** 把那一串解回来的字节（`printf %s` 原样吐），不是本函数自己的逆运算；1..=255 每个字节各一格。
#[test]
#[cfg(unix)]
fn the_byte_quote_is_read_back_by_a_real_bash_byte_for_byte() {
    use std::os::unix::ffi::OsStrExt as _;
    let mut samples: Vec<Vec<u8>> = (1u8..=255).map(|b| vec![b'/', b'a', b, b'z']).collect();
    samples.push(b"/srv/d\xff/\xe4\xbd\xa0 it's \\ $x `y`".to_vec());
    samples.push(b"/p/\xffa\xfe0".to_vec());
    for s in &samples {
        let q = posix_quote_bytes(s);
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(std::ffi::OsStr::from_bytes(
                format!("printf %s {q}").as_bytes(),
            ))
            .output()
            .expect("起 bash");
        assert_eq!(&out.stdout, s, "bash 解回来的字节不对：{q}");
    }
}

/// 字节形的判定与字符串形逐条同规则：正（真实的乱码目录名）放、反（相对 · `..` 段 · NUL / CR / LF）拒；合法 UTF-8 两形答得一样。
#[test]
fn the_byte_path_rule_agrees_with_the_string_rule() {
    for ok in [&b"/srv/d\xff"[..], b"/a b/(1)/it's", b"/"] {
        assert!(posix_free_path_bytes_ok(ok), "{ok:?}");
    }
    for bad in [
        &b"srv/d\xff"[..],
        b"/a/../b",
        b"/a\x00",
        b"/a\nb",
        b"/a\rb",
        b"",
    ] {
        assert!(!posix_free_path_bytes_ok(bad), "{bad:?}");
    }
    for s in ["/srv/x", "rel", "/a/../b", "/a\nb", "/中文 (2)"] {
        assert_eq!(
            posix_free_path_bytes_ok(s.as_bytes()),
            posix_free_path_ok(s),
            "{s:?}"
        );
    }
}
