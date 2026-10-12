//! 在看名单：计数到零才出名单 · 应答里形状不对的那一格不收。
use super::*;

#[test]
fn a_session_leaves_the_list_only_when_the_last_holder_lets_go() {
    let mut w = Watched::default();
    assert!(w.hold("a"), "第一处在看 ⇒ 名单变了");
    assert!(!w.hold("a"), "第二处在看 ⇒ 名单没变");
    assert!(!w.release("a"));
    assert_eq!(w.sids().len(), 1);
    assert!(w.release("a"), "最后一处放手 ⇒ 名单变了");
    assert!(w.sids().is_empty());
    assert!(!w.release("a"), "本来就没在看 ⇒ 不变");
}

#[test]
fn the_reply_turns_into_rows_and_a_bad_row_is_left_out() {
    let data = serde_json::json!({ "from": [
        { "sid": "a", "path": "/p/a.jsonl", "seq": 7 },
        { "sid": "b", "path": "/p/b.jsonl" },
    ]});
    assert_eq!(
        from_of(Some(&data)),
        vec![WatchFrom {
            sid: "a".into(),
            path: "/p/a.jsonl".into(),
            seq: 7
        }]
    );
    assert!(from_of(None).is_empty());
}
