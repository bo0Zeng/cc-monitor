//! 命令那一层里纯的几样（入参的形状由 typed 那一处收，判据在 `inbound_tests`）：别名事件按前后账号表之差 ·
//! 导入凭据的路径只收家目录底下 · 登录那一行的拼法。落盘那一半在 `faces/accounts_face_tests.rs` 真跑。夹具全是合成的。
use super::*;

/// 别名事件只跟着账号表之差：消失的按删号、新出现的按建号；没变 ⇒ 空。
#[test]
fn alias_events_follow_the_difference_of_the_two_tables() {
    let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        alias_events(&v(&["a", "b"]), &v(&["b", "c"])),
        [
            AliasEvent::Removed("a".into()),
            AliasEvent::Added("c".into())
        ]
    );
    assert!(alias_events(&v(&["a"]), &v(&["a"])).is_empty());
    assert_eq!(
        alias_events(&[], &v(&["x"])),
        [AliasEvent::Added("x".into())]
    );
}

/// 导入凭据的路径：`~/` 按家展开、尾部 `/` 去掉；家目录外 · 家目录本身 · 相对 · `..` 段 · 控制符一律 `refused`。
#[test]
fn a_credential_path_must_live_under_home() {
    assert_eq!(cred_path("/h", "~/c/cred.json").unwrap(), "/h/c/cred.json");
    assert_eq!(cred_path("/h", " /h/c/ ").unwrap(), "/h/c");
    // `~/` · `/h/` 是家目录本身（这一条补测试时逮到：尾部 `/` 去掉之前判「不是家」，放过去了）。
    for bad in [
        "/etc/cred",
        "/h",
        "/h/",
        "~/",
        "c/cred",
        "/h/../etc",
        "/h/a\nb",
        "/hx/cred",
    ] {
        assert_eq!(cred_path("/h", bad).unwrap_err().code, "refused", "{bad:?}");
    }
}

/// 登录那一行：`<家>/.cc-monitor/bin/ccm -- --account <名>`，名字过 quote（带空格 · 引号也不拆开）。
#[test]
fn the_login_line_quotes_the_name() {
    let q = shell_quote_core::posix_quote;
    assert_eq!(
        login_line("/h/", "z"),
        format!("{} -- --account {}", q("/h/.cc-monitor/bin/ccm"), q("z"))
    );
    let odd = login_line("/h", "a b'c");
    assert!(
        odd.ends_with(&format!(" --account {}", q("a b'c"))),
        "{odd}"
    );
    assert_ne!(shell_quote_core::posix_quote("a b'c"), "a b'c");
}
