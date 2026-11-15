//! Codex 按轮分叉：叉在的那一条消息认成它所在的那一轮 · 发给 app-server 的那一条 · 回包里新会话的 id。
//! 记录行照 codex-rs `rust-v0.167.0` 的 rollout 形手写（值是编的）；不起真 codex。

use super::*;
use serde_json::{json, Value};

fn line(kind: &str, payload: Value) -> String {
    json!({"timestamp": "2026-10-01T00:00:00Z", "type": kind, "payload": payload}).to_string()
}

fn turn(id: &str) -> String {
    line("event_msg", json!({"type": "task_started", "turn_id": id}))
}

fn user(text: &str, id: Option<&str>) -> String {
    let mut p = json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]});
    if let Some(id) = id {
        p["id"] = json!(id);
    }
    line("response_item", p)
}

/// 第 1 轮里那条没有 `id` 的（按起点字节偏移合成）· 第 2 轮里那条带 `id` 的 ⇒ 各认成自己那一轮；
/// 第一轮之前的那一条 · 记录里没有的那一条 ⇒ 认不出。
#[test]
fn a_message_is_forked_through_the_turn_it_sits_in() {
    let home = test_home::scratch("codex-branch");
    let file = home.join("rollout.jsonl");
    let head = line("session_meta", json!({"id": "s1", "cwd": "/w"}));
    let before = user("too early", Some("m0"));
    let first = user("hello", None);
    let lines = [
        head.clone(),
        before.clone(),
        turn("t1"),
        first.clone(),
        turn("t2"),
        user("again", Some("m2")),
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let start_of_first = (head.len() + before.len() + turn("t1").len() + 3) as u64;
    let first_id = crate::agents::line_id(start_of_first);
    assert_eq!(turn_of(&file, &first_id).as_deref(), Some("t1"));
    assert_eq!(turn_of(&file, "m2").as_deref(), Some("t2"));
    assert_eq!(turn_of(&file, "m0"), None);
    assert_eq!(turn_of(&file, "nope"), None);
    let _ = std::fs::remove_dir_all(&home);
}

/// 发的是 `thread/fork{threadId, lastTurnId}`，不带那几个实验字段。
#[test]
fn the_fork_request_names_the_thread_and_the_last_turn_only() {
    assert_eq!(
        fork_params("s1", "t2"),
        json!({"threadId": "s1", "lastTurnId": "t2"})
    );
}

/// 回包里 `thread.id` 就是新会话；缺那一格 · 不是串 · 空串 ⇒ 读不懂、不交 id。
#[test]
fn the_new_session_is_the_forked_thread_id() {
    assert_eq!(
        forked_id(&json!({"thread": {"id": "n1"}, "model": "m"})).as_deref(),
        Some("n1")
    );
    assert_eq!(forked_id(&json!({"thread": {}})), None);
    assert_eq!(forked_id(&json!({"thread": {"id": 3}})), None);
    assert_eq!(forked_id(&json!({"thread": {"id": ""}})), None);
}

/// 注册表那一行交的是自己叉那一形；源会话找不到 ⇒ 叉不成（不起进程）。
#[test]
fn the_registry_forks_codex_itself_and_a_missing_session_fails() {
    let Some(crate::agents::BranchFace::Own(own)) = super::super::RECORDS.branch else {
        panic!("Codex 是自己叉那一形");
    };
    let home = test_home::scratch("codex-branch-missing");
    let f = crate::agents::OwnFork {
        home: &home,
        sid: "0199aaaa-0000-7000-8000-000000000001",
        message: "m1",
    };
    assert_eq!(own(&f), Err(crate::agents::ForkFail::Failed));
    let _ = std::fs::remove_dir_all(&home);
}
