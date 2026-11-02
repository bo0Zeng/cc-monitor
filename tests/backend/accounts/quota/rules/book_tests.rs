//! `rotation.json` 的形：策略副本按版本只存一份（会话只写键）· 读回来跟写出去的一样 · 引用了没有的那份 ⇒ 读不懂。
//! 纯内存，不碰盘。

use super::*;
use serde_json::{json, Value};

fn copy(id: &str, rev: &str) -> Copy {
    Copy {
        id: id.into(),
        rev: rev.into(),
        code: "\n\nfn decide(ctx) { #{ order: [] } }\n".repeat(50),
        settings: json!({"accounts": [{"id": "a"}]}),
        tz: None,
        time_varying: false,
        form: Value::Null,
    }
}

fn book_of(uses: &[(&str, Option<Copy>)]) -> Book {
    let mut b = Book::fresh();
    for (sid, c) in uses {
        b.saw(sid, "claude-code", "a", 100, || c.clone());
    }
    b
}

/// ★ 三个会话载入同一份（同一 `id@rev`）⇒ 写出的原文里代码只有一份；另一个版本另一份；没有会话引用的不出现。
#[test]
fn one_copy_per_version_on_disk() {
    let v1 = copy("mine", "h:1");
    let v2 = copy("mine", "h:2");
    let b = book_of(&[
        ("s1", Some(v1.clone())),
        ("s2", Some(v1.clone())),
        ("s3", Some(v1.clone())),
        ("s4", Some(v2.clone())),
        ("s5", None),
    ]);
    let raw = serde_json::to_string(&b).expect("写");
    assert_eq!(
        raw.matches("fn decide").count(),
        2 * 50,
        "两个版本各一份代码：{} 字节",
        raw.len()
    );
    let v: Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(v["copies"].as_object().map(|m| m.len()), Some(2), "{v:#}");
    assert_eq!(v["sessions"]["s1"]["uses"], v["sessions"]["s2"]["uses"]);
    assert_ne!(v["sessions"]["s1"]["uses"], v["sessions"]["s4"]["uses"]);
    assert!(v["sessions"]["s5"].get("uses").is_none());
    let back: Book = serde_json::from_str(&raw).expect("读回");
    assert_eq!(back, b, "读回来跟写出去的一样");
}

/// ★ 同一个 `id@rev`、内容却不同（内存里改过其中一份）⇒ 两份都留着，各会话读回各自那一份。
#[test]
fn same_version_with_different_content_keeps_both() {
    let mut b = book_of(&[
        ("s1", Some(copy("mine", "h:1"))),
        ("s2", Some(copy("mine", "h:1"))),
    ]);
    let s2 = b.sessions.get_mut("s2").expect("s2");
    std::sync::Arc::make_mut(s2.uses.as_mut().expect("载入了")).settings = json!({"n": 2});
    let raw = serde_json::to_string(&b).expect("写");
    let back: Book = serde_json::from_str(&raw).expect("读回");
    assert_eq!(back, b);
}

/// ★ 会话引用了 `copies` 里没有的那一份 ⇒ 读不懂（不当成不轮换）。
#[test]
fn a_missing_copy_is_unreadable() {
    let raw = json!({
        "sessions": {"s1": {"agent": "claude-code", "start": "a", "current": "a", "since": 1, "uses": "mine@h:1"}},
        "copies": {}
    });
    assert!(serde_json::from_value::<Book>(raw).is_err());
}

/// ★ 账本只装近来的会话：换过号 · 拖过顺序 · 去掉过号的会话，超过 [`DROP_AFTER`] 没被看见也清（下一次记新会话时）；
/// 近来还被看见的（换过号的也算）照留。
#[test]
fn long_gone_sessions_go_even_after_they_switched() {
    let mut b = Book::fresh();
    let log = |at| Log {
        at,
        from: "a".into(),
        to: "b".into(),
        cause: Cause::Order,
        why: None,
        saw: None,
    };
    for n in 0..20 {
        let sid = format!("old{n}");
        b.saw(&sid, "claude-code", "a", 100, || None);
        let s = b.sessions.get_mut(&sid).expect("记下了");
        match n % 3 {
            0 => s.push_log(log(150)),
            1 => s.temp = vec!["b".into()],
            _ => s.off = vec!["b".into()],
        }
    }
    let now = 100 + DROP_AFTER + 10;
    b.saw("recent", "claude-code", "a", now - 60, || None);
    b.sessions
        .get_mut("recent")
        .expect("记下了")
        .push_log(log(now - 30));
    b.saw("new", "claude-code", "a", now, || None);
    assert_eq!(
        b.sessions.keys().map(String::as_str).collect::<Vec<_>>(),
        ["new", "recent"]
    );
}

/// ★ 换号记录至多留 50 条：第 51 条把最早那条挤掉。
#[test]
fn the_log_keeps_the_latest_fifty() {
    let mut s = Session::fresh("claude-code", "a", 0);
    for at in 1..=51 {
        s.push_log(Log {
            at,
            from: "a".into(),
            to: "b".into(),
            cause: Cause::Order,
            why: None,
            saw: None,
        });
    }
    assert_eq!(s.log.len(), 50);
    assert_eq!(s.log.first().map(|l| l.at), Some(2));
    assert_eq!(s.log.last().map(|l| l.at), Some(51));
}
