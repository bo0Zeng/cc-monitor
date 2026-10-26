//! 会话 id 只有一个值域：解码就判（`shell_quote_core::session_id_ok`），判不过的形状进不了 [`Sid`]；线上照旧是那个串。

use super::Sid;

#[test]
fn a_real_session_id_decodes_and_goes_back_as_the_same_string() {
    let raw = r#""0473c3a0-1111-2222-3333-444455556666""#;
    let sid: Sid = serde_json::from_str(raw).unwrap();
    assert_eq!(sid.as_str(), "0473c3a0-1111-2222-3333-444455556666");
    assert_eq!(serde_json::to_string(&sid).unwrap(), raw);
    assert_eq!(
        "A1-b2".parse::<Sid>().map(|s| s.as_str().to_string()),
        Ok("A1-b2".to_string())
    );
}

#[test]
fn an_off_shape_session_id_does_not_decode() {
    for bad in [
        "",
        "-abc",
        "--dangerously-skip-permissions",
        "a_b",
        "a.b",
        "../etc",
        "a b",
        "a;b",
        &"a".repeat(shell_quote_core::SESSION_ID_MAX + 1),
    ] {
        let json = serde_json::to_string(bad).unwrap();
        assert!(
            serde_json::from_str::<Sid>(&json).is_err(),
            "坏形解开了：{bad:?}"
        );
        assert!(bad.parse::<Sid>().is_err(), "坏形收下了：{bad:?}");
    }
    assert!(serde_json::from_str::<Sid>("12").is_err(), "不是串也解不开");
}
