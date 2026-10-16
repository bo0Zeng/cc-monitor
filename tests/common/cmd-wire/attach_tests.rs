//! 握手那两行的线上字节：attach 行只认四格、`attach` 恒为 `true`、`watch` 与 `stream-watch` 的入参同形；回话没带 `mine` 时与只回「可以」那一行逐字节相同。
use super::*;

#[test]
fn the_attach_line_is_four_cells_and_nothing_else() {
    let l = AttachLine {
        attach: Yes,
        tz: Some("Asia/Shanghai".into()),
        mine: Some("p1a-x".into()),
        watch: Some(crate::watch::WatchAsk {
            sids: vec!["s1".into()],
            branch: vec![crate::watch::BranchHeld {
                sid: "s1".into(),
                ..Default::default()
            }],
        }),
    };
    assert_eq!(
        l.line(),
        "{\"attach\":true,\"tz\":\"Asia/Shanghai\",\"mine\":\"p1a-x\",\"watch\":{\"sids\":[\"s1\"],\"branch\":[{\"sid\":\"s1\",\"at\":0}]}}\n"
    );
    assert_eq!(
        serde_json::from_str::<AttachLine>(r#"{"attach":true,"watch":{"sids":[]}}"#)
            .unwrap()
            .watch,
        Some(crate::watch::WatchAsk::default()),
        "空名单 ＝ 一个都不看（不是「没报过」）"
    );
    assert_eq!(AttachLine::default().line(), "{\"attach\":true}\n");
    for bad in [
        r#"{"attach":false}"#,
        r#"{"attach":"yes"}"#,
        r#"{"tz":"UTC"}"#,
        r#"{"attach":true,"view":{}}"#,
        r#"{"attach":true,"flags":[]}"#,
        r#"{"attach":true,"watch":{"sids":[],"extra":1}}"#,
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
