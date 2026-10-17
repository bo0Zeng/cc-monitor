//! 握手那两行的线上字节：attach 行只认三格、`attach` 恒为 `true`；回话没带 `mine` 时与只回「可以」那一行逐字节相同。
use super::*;

#[test]
fn the_attach_line_is_three_cells_and_nothing_else() {
    let l = AttachLine {
        attach: Yes,
        tz: Some("Asia/Shanghai".into()),
        mine: Some("p1a-x".into()),
    };
    assert_eq!(
        l.line(),
        "{\"attach\":true,\"tz\":\"Asia/Shanghai\",\"mine\":\"p1a-x\"}\n"
    );
    assert_eq!(AttachLine::default().line(), "{\"attach\":true}\n");
    for bad in [
        r#"{"attach":false}"#,
        r#"{"attach":"yes"}"#,
        r#"{"tz":"UTC"}"#,
        r#"{"attach":true,"view":{}}"#,
        r#"{"attach":true,"flags":[]}"#,
    ] {
        assert!(serde_json::from_str::<AttachLine>(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_reply_carries_the_relation_only_when_asked() {
    assert_eq!(
        AttachReply::Ok { relation: None }.line(),
        "{\"attach\":\"ok\"}\n"
    );
    assert_eq!(
        AttachReply::Ok {
            relation: Some(Relation::Older)
        }
        .line(),
        "{\"attach\":\"ok\",\"relation\":\"older\"}\n"
    );
    assert_eq!(
        AttachReply::of(r#"{"attach":"refused","reason":"absent"}"#),
        Some(AttachReply::Refused {
            reason: "absent".into()
        })
    );
    assert_eq!(
        AttachReply::of(r#"{"kind":"hello","build_id":"p1a-x"}"#),
        None
    );
}
