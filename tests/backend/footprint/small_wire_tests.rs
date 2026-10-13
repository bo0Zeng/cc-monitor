//! 足迹那一族的五条小命令（`agent-home-check` · `first-run` · `chores-mark` · `last-seen-read` · `last-seen-write`）是界面读的线上形：
//! 换形之前用旧代码跑出回包字节、钉成字面量；换形之后逐字节比（经 `serde_json::Value` 那一跳，同帧面：键按字典序）。临时目录换成 `<D>`。
//! 没有重写开关：对不上就是线上形变了。

use serde_json::{json, Value};

/// 帧面那一跳的写法（键按字典序）。
fn wire(v: &impl serde::Serialize) -> String {
    serde_json::to_string(&serde_json::to_value(v).unwrap()).unwrap()
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fp-small-wire-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn agent_home_check_keeps_its_wire_bytes() {
    let d = tmp("home");
    let check = |p: &std::path::Path| {
        wire(&super::agent_home_check::answer_check(&json!({ "path": p })).unwrap())
    };
    assert_eq!(check(&d.join("nope")), r#"{"state":"missing"}"#);
    std::fs::write(d.join("f"), "x").unwrap();
    assert_eq!(check(&d.join("f")), r#"{"state":"not_dir"}"#);
    assert_eq!(check(&d), r#"{"state":"no_records"}"#);
    std::fs::create_dir_all(crate::agents::records_root(&d).unwrap()).unwrap();
    assert_eq!(check(&d), r#"{"state":"ok"}"#);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn first_run_keeps_its_wire_bytes() {
    assert_eq!(
        wire(&super::readiness::product(false, true, 2, true)),
        r#"{"left":1,"skipped":true,"steps":[{"done":false,"id":"terminal","required":true},{"done":true,"id":"named","required":false},{"done":true,"id":"remote","required":false}]}"#
    );
    assert_eq!(
        wire(&super::readiness::product(true, false, 0, false)),
        r#"{"left":0,"skipped":false,"steps":[{"done":true,"id":"terminal","required":true},{"done":false,"id":"named","required":false},{"done":false,"id":"remote","required":false}]}"#
    );
}

#[test]
fn chores_mark_keeps_its_wire_bytes() {
    let d = tmp("marks");
    let own = crate::platform::fs::Own::issue_at(&d);
    let mark = |a: Value| wire(&super::chores::marks::answer_mark(&a, &own).unwrap());
    assert_eq!(
        mark(json!({"op": "decline", "id": "relay"})),
        r#"{"declined":["relay"],"selfPaste":null,"startSkipped":false}"#
    );
    assert_eq!(
        mark(json!({"op": "selfPaste", "rc": "/h/.bashrc"})),
        r#"{"declined":["relay"],"selfPaste":"/h/.bashrc","startSkipped":false}"#
    );
    assert_eq!(
        mark(json!({"op": "skipStart"})),
        r#"{"declined":["relay"],"selfPaste":"/h/.bashrc","startSkipped":true}"#
    );
    assert_eq!(
        mark(json!({"op": "undecline", "id": "relay"})),
        r#"{"declined":[],"selfPaste":"/h/.bashrc","startSkipped":true}"#
    );
    assert_eq!(
        mark(json!({"op": "unselfPaste"})),
        r#"{"declined":[],"selfPaste":null,"startSkipped":true}"#
    );
    assert_eq!(
        mark(json!({"op": "unskipStart"})),
        r#"{"declined":[],"selfPaste":null,"startSkipped":false}"#
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn last_seen_pair_keeps_its_wire_bytes() {
    let d = tmp("seen");
    let own = crate::platform::fs::Own::issue_at(&d);
    let file = d.join(relay_route_core::LAST_SEEN_REL);
    let ls = &super::last_seen::write_at;
    let read = |o: &str| wire(&super::last_seen::read_at(&file, &json!({ "origin": o })).unwrap());
    assert_eq!(read("devbox"), r#"{"accounts":null,"data":null}"#);
    let v = json!({"meta": {"enabled": true}, "accounts": [{"name": "work"}]});
    assert_eq!(
        wire(
            &ls(
                &own,
                &json!({"origin": "devbox", "kind": "accounts", "value": v}),
                1000
            )
            .unwrap()
        ),
        r#"{"atMs":1000}"#
    );
    assert_eq!(
        wire(
            &ls(
                &own,
                &json!({"origin": "devbox", "kind": "data", "value": {"home": "/h"}}),
                2000
            )
            .unwrap()
        ),
        r#"{"atMs":2000}"#
    );
    assert_eq!(
        read("devbox"),
        r#"{"accounts":{"atMs":1000,"value":{"accounts":[{"name":"work"}],"meta":{"enabled":true}}},"data":{"atMs":2000,"value":{"home":"/h"}}}"#
    );
    assert_eq!(
        wire(&ls(&own, &json!({"origin": "devbox", "forget": true}), 3000).unwrap()),
        r#"{"atMs":3000}"#
    );
    assert_eq!(read("devbox"), r#"{"accounts":null,"data":null}"#);
    // 不认识的机器 · 盘上那份里形状不对的一项 ⇒ 照没记过（`null`）。
    assert_eq!(read("nobody"), r#"{"accounts":null,"data":null}"#);
    let _ = std::fs::remove_dir_all(&d);
}
