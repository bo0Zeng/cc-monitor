use super::*;

/// 应答 `{from: [{sid, path, seq}]}` 原样翻成会话流那一格的体；形状不对的那一格不收，别的照收；没有 `from` ⇒ 空。
#[test]
fn the_watch_reply_turns_into_stream_cells_and_a_bad_row_is_left_out() {
    let data = serde_json::json!({ "from": [
        { "sid": "a", "path": "/p/a.jsonl", "seq": 7 },
        { "sid": "b", "path": "/p/b.jsonl" },
        { "sid": "c", "path": "/p/c.jsonl", "seq": 0 },
    ]});
    let got: Vec<(String, String, u64)> = from_of(Some(&data))
        .into_iter()
        .map(|w| (w.session_id, w.path, w.seq))
        .collect();
    assert_eq!(
        got,
        vec![
            ("a".to_string(), "/p/a.jsonl".to_string(), 7),
            ("c".to_string(), "/p/c.jsonl".to_string(), 0),
        ]
    );
    assert!(from_of(None).is_empty());
    assert!(from_of(Some(&serde_json::json!({}))).is_empty());
}
