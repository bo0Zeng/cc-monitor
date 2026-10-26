//! 机器表的最近一份：本机后端那一答 ⇒ 照用的那几台（名字 · 连不连）；读不懂 ⇒ 空表 ＋ 那一句（一台都不连），不在 ⇒ 空表、不出声。
use super::*;
use serde_json::json;

#[test]
fn the_reply_says_which_hosts_to_use() {
    let h = json!({"label": "gpu", "host": "g.lan", "user": "pi", "port": 22, "connect": false});
    let (hosts, bad) =
        hosts_of(&json!({"state": "ok", "hosts": [{"name": "gpu", "host": h.clone()}]}));
    assert_eq!(
        (hosts, bad),
        (
            vec![Row {
                name: "gpu".into(),
                connect: false,
            }],
            None
        )
    );
    // 名字只认后端算好的那一格：没有它的那一行不认（本壳不再自己算）。
    let (hosts, _) = hosts_of(&json!({"state": "ok", "hosts": [{"host": {"host": "g.lan"}}]}));
    assert!(hosts.is_empty());
    let (hosts, bad) = hosts_of(&json!({"state": "absent", "hosts": []}));
    assert!(hosts.is_empty() && bad.is_none());
    let (hosts, bad) = hosts_of(
        &json!({"state": "unreadable", "reason": "读不懂", "path": "/x/machines.json", "hosts": [{"name": "x", "host": {}}]}),
    );
    assert!(hosts.is_empty());
    assert_eq!(
        bad,
        Some(("/x/machines.json".to_string(), "读不懂".to_string()))
    );
}
