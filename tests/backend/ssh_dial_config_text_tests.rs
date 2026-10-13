//! 手机导入一段 `~/.ssh/config` 文本（手机上没有 `ssh -G`，桌面那一份照旧调它）：判据逐条搬自从前 Kotlin 那一份解析器的单测。

use super::{parse, ConfigHost};

fn one(text: &str) -> ConfigHost {
    let mut v = parse(text);
    assert_eq!(v.len(), 1, "{v:?}");
    v.remove(0)
}

#[test]
fn parses_a_full_host_block() {
    let h = one(
        "# my servers\nHost nano\n  HostName 198.51.100.36\n  Port 2222\n  User pi\n  IdentityFile ~/.ssh/id_ed25519\n",
    );
    assert_eq!(h.alias, "nano");
    assert_eq!(h.host_name, "198.51.100.36");
    assert_eq!(h.port, 2222);
    assert_eq!(h.user.as_deref(), Some("pi"));
    assert_eq!(h.identity_file.as_deref(), Some("~/.ssh/id_ed25519"));
}

#[test]
fn missing_hostname_falls_back_to_alias_and_missing_port_defaults_22() {
    let h = one("Host box\n  User root");
    assert_eq!(h.host_name, "box");
    assert_eq!(h.port, 22);
    assert_eq!(h.user.as_deref(), Some("root"));
    assert_eq!(h.identity_file, None);
}

#[test]
fn multiple_aliases_on_one_host_line_each_become_a_host() {
    let v = parse("Host a b c\n  HostName 198.51.100.1");
    assert_eq!(
        v.iter().map(|h| h.alias.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert!(v.iter().all(|h| h.host_name == "198.51.100.1"));
}

#[test]
fn wildcard_host_blocks_are_skipped() {
    let v = parse("Host *\n  User default\nHost real\n  HostName 192.0.2.1\n");
    assert_eq!(
        v.iter().map(|h| h.alias.as_str()).collect::<Vec<_>>(),
        ["real"]
    );
}

#[test]
fn key_equals_value_inline_comments_and_blank_lines() {
    let h = one("\nHost eq\nPort=2022   # non-standard port\nHostName=example.com\n\n");
    assert_eq!(h.port, 2022);
    assert_eq!(h.host_name, "example.com");
}

#[test]
fn two_host_blocks_parse_independently() {
    let v = parse("Host a\n HostName 192.0.2.1\nHost b\n HostName 192.0.2.2\n User x");
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].user, None, "a 不该继承 b 的 User");
    assert_eq!(v[1].user.as_deref(), Some("x"));
}

#[test]
fn empty_or_keyless_input_yields_nothing() {
    assert!(parse("").is_empty());
    assert!(parse("# just a comment\n\n").is_empty());
}
