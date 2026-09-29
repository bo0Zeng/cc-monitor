//! 〔DUP3 · 主会话 09-26 裁 J9〕`upstream-url-core` 的判据。要求住址：`设计/01 §5` D1「一个判定只有一个家」。
//! 与界面生成物逐条对的共用金样那一条住 monitor（`tests/frontend/shell/payload_judgment_rules.rs`，本 crate 零依赖、不读 JSON）。

use super::*;

/// 解析出的几格（`Base` 装的就是它们）· 回环判定（整个 `127/8` 与 `::1`）· `usable` 的明文那一格，正反各一格。
#[test]
fn parse_keeps_the_parts_and_plaintext_is_only_usable_on_loopback() {
    assert_eq!(
        parse("https://h:8443/a/b/").unwrap(),
        UpstreamUrl {
            tls: true,
            host: "h".into(),
            port: 8443,
            path: "/a/b".into()
        }
    );
    assert_eq!(parse("http://h").unwrap().port, 80);
    assert_eq!(parse("https://h").unwrap().path, "");
    for yes in [
        "127.0.0.1",
        "127.9.9.9",
        "[::1]",
        "::1",
        "localhost",
        "LocalHost",
    ] {
        assert!(upstream_is_loopback(yes), "{yes:?}");
    }
    for no in ["128.0.0.1", "[::2]", "localhost.evil", "0.0.0.0", ""] {
        assert!(!upstream_is_loopback(no), "{no:?}");
    }
    assert!(usable("http://127.0.0.1:1").is_ok());
    assert_eq!(
        usable("http://h").unwrap_err(),
        Unusable::PlaintextOffLoopback
    );
    assert_eq!(
        usable("ftp://h").unwrap_err(),
        Unusable::Shape(ShapeIssue::BadScheme)
    );
}
