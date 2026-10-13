//! Claude 一行 ⇒ 链事实：哪几类进链、进不进界面、打断与原文、排队原文；没身份的不进。
use super::*;

fn node(raw: &str) -> Link {
    match chain_fact(&crate::agents::Line::new(raw, 0)) {
        Some(ChainFact::Node(l)) => l,
        other => panic!("not a node: {other:?}"),
    }
}

#[test]
fn user_and_assistant_lines_are_shown_nodes() {
    let u = node(
        r#"{"type":"user","uuid":"u1","parentUuid":"a0","timestamp":"t1","message":{"role":"user","content":"  hello there  "}}"#,
    );
    assert_eq!(
        u,
        Link {
            id: "u1".into(),
            parent: Some("a0".into()),
            at: "t1".into(),
            said: true,
            reply: false,
            interrupt: false,
            text: Some("hello there".into()),
            shown: true,
        }
    );
    let a = node(
        r#"{"type":"assistant","uuid":"a1","parentUuid":null,"timestamp":"t2","message":{"role":"assistant","content":[{"type":"text","text":"x"}]}}"#,
    );
    assert!(a.reply && a.shown && !a.said && a.parent.is_none() && a.text.is_none());
}

#[test]
fn an_interrupt_marker_is_flagged() {
    let u = node(
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"t3","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
    );
    assert!(u.interrupt);
}

#[test]
fn placeholders_join_the_chain_but_are_not_shown() {
    for raw in [
        r#"{"type":"attachment","uuid":"x1","parentUuid":"a1","timestamp":"t4"}"#,
        r#"{"type":"system","subtype":"local_command","uuid":"x2","parentUuid":"x1","timestamp":"t5"}"#,
        r#"{"type":"some-new-kind","uuid":"x3","parentUuid":"x2","timestamp":"t6"}"#,
    ] {
        let l = node(raw);
        assert!(!l.shown && !l.said && !l.reply, "{raw}");
    }
    let retry = node(
        r#"{"type":"system","subtype":"api_error","uuid":"x4","parentUuid":"x3","timestamp":"t7"}"#,
    );
    assert!(retry.shown);
}

#[test]
fn queued_text_comes_from_enqueue_only() {
    assert_eq!(
        chain_fact(&crate::agents::Line::new(r#"{"type":"queue-operation","operation":"enqueue","content":" later words ","timestamp":"t8"}"#, 0)),
        Some(ChainFact::Queued("later words".into()))
    );
    assert_eq!(
        chain_fact(&crate::agents::Line::new(r#"{"type":"queue-operation","operation":"remove","content":"later words","timestamp":"t9"}"#, 0)),
        None
    );
}

#[test]
fn lines_without_identity_or_time_are_off_the_chain() {
    for raw in [
        r#"{"type":"ai-title","aiTitle":"x","sessionId":"s"}"#,
        r#"{"type":"user","timestamp":"t1","message":{"role":"user","content":"x"}}"#,
        r#"{"type":"user","uuid":"u9","message":{"role":"user","content":"x"}}"#,
        r#"{"type":"mode","mode":"x"}"#,
        "not json",
    ] {
        assert_eq!(chain_fact(&crate::agents::Line::new(raw, 0)), None, "{raw}");
    }
}

/// read8b（瓶颈 #8）：一行只解析一次文本 —— user 行要整行（判打断、取原文），那几格短串就从那一份里取，不再先解一遍短串再解一遍整行；
/// 别的行只解那几格短串。出的链事实与改前一样（上面几条钉着）。
#[test]
fn a_line_is_parsed_from_text_at_most_once() {
    let count = |raw: &str| {
        crate::agents::LINE_PARSES.with(|c| c.set(0));
        let _ = chain_fact(&crate::agents::Line::new(raw, 0));
        crate::agents::LINE_PARSES.with(std::cell::Cell::get)
    };
    let user = r#"{"type":"user","uuid":"u1","parentUuid":"a0","timestamp":"t1","message":{"role":"user","content":"hello"}}"#;
    let reply = r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"t2","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#;
    assert_eq!(count(user), 1, "user 行解析了不止一遍");
    assert_eq!(count(reply), 1, "回复行解析了不止一遍");
    assert!(node(user).said && node(user).text.as_deref() == Some("hello"));
}
