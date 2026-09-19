use super::*;
use serde_json::json;

fn event(sub: &str, extra: Value) -> Value {
    let mut payload = json!({ "type": sub });
    if let (Some(o), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, val) in e {
            o.insert(k.clone(), val.clone());
        }
    }
    json!({ "timestamp": "2026-07-19T08:00:00Z", "type": "event_msg", "payload": payload })
}

/// 基准正例：task_complete + turn_id → turn-end，uuid=turn_id。
#[test]
fn task_complete_is_turn_end_uuid_is_turn_id() {
    let v = event("task_complete", json!({ "turn_id": "t-1" }));
    assert!(is_codex_turn_end(&v));
    assert_eq!(codex_turn_end_uuid(&v), Some("t-1"));
}

/// v1 alias：turn_complete 归一为 task_complete → 同样是 turn-end。
#[test]
fn turn_complete_alias_is_turn_end() {
    let v = event("turn_complete", json!({ "turn_id": "t-2" }));
    assert!(is_codex_turn_end(&v));
    assert_eq!(codex_turn_end_uuid(&v), Some("t-2"));
}

/// trap③ 回退：turn_id 缺 → uuid 回退到 envelope timestamp（否则漏报最新完成轮）。
#[test]
fn missing_turn_id_falls_back_to_envelope_timestamp() {
    let v = event("task_complete", json!({}));
    assert!(is_codex_turn_end(&v));
    assert_eq!(codex_turn_end_uuid(&v), Some("2026-07-19T08:00:00Z"));
}

/// turn_aborted 明确不算 turn-end（aterm 决策：中止轮静默不发）。
#[test]
fn turn_aborted_is_not_turn_end() {
    let v = event("turn_aborted", json!({ "turn_id": "t-3" }));
    assert!(!is_codex_turn_end(&v));
    assert_eq!(codex_turn_end_uuid(&v), None);
}

/// 其它 event_msg 子型（token_count/task_started/agent_message…）非 turn-end。
#[test]
fn other_events_are_not_turn_end() {
    for sub in [
        "token_count",
        "task_started",
        "turn_started",
        "agent_message",
        "user_message",
    ] {
        assert!(
            !is_codex_turn_end(&event(sub, json!({}))),
            "{sub} 不该是 turn-end"
        );
    }
}

/// 非 event_msg 顶层（response_item/session_meta/…）+ 坏信封 → 非 turn-end、不崩。
#[test]
fn non_event_and_malformed_are_not_turn_end() {
    assert!(!is_codex_turn_end(&json!({
        "type": "response_item", "payload": { "type": "message", "role": "assistant" }
    })));
    assert!(!is_codex_turn_end(
        &json!({ "type": "session_meta", "payload": { "cwd": "/p" } })
    ));
    // 坏信封：无 payload / payload 非对象 / 无 type。
    assert!(!is_codex_turn_end(&json!({ "type": "event_msg" })));
    assert!(!is_codex_turn_end(
        &json!({ "type": "event_msg", "payload": "x" })
    ));
    assert!(!is_codex_turn_end(
        &json!({ "payload": { "type": "task_complete" } })
    ));
    assert!(!is_codex_turn_end(&json!("not even an object")));
}

/// **不是** Claude turn-end 路：Claude 的 assistant+end_turn 形状在 Codex 探测下 → false
/// （per-kind 隔离，两路各判各的）。
#[test]
fn claude_shape_is_not_codex_turn_end() {
    let claude = json!({
        "type": "assistant", "uuid": "u", "message": { "stop_reason": "end_turn" }
    });
    assert!(!is_codex_turn_end(&claude));
}

/// Phase D 审计修：sid 提取校验对齐 monitor（rollout- 前缀 + 末36 UUID 形），畸形名 → None（跳过、
/// 不吐幽灵行），合法名 → 末36 UUID。补 daemon 侧此前缺的畸形名覆盖。
#[test]
fn codex_sid_from_path_validates_like_monitor() {
    let p = |n: &str| PathBuf::from(n);
    assert_eq!(
        codex_sid_from_path(&p(
            "rollout-2026-07-18T08-00-00-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl"
        ))
        .as_deref(),
        Some("019f75dd-875c-7c81-9eda-32f866b2c60f")
    );
    for bad in [
        "rollout-garbage.jsonl",                                 // 剥前缀后 <36
        "notrollout-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl", // 无 rollout- 前缀
        "rollout-2026-07-18T08-00-00-zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz.jsonl", // 末36 结构对但非 hex
    ] {
        assert!(codex_sid_from_path(&p(bad)).is_none(), "畸形名应跳: {bad}");
    }
}

#[test]
fn is_uuid_matches_8_4_4_4_12_hex() {
    assert!(is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60f"));
    assert!(!is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60")); // 末段 11 位
    assert!(!is_uuid("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz")); // 非 hex
    assert!(!is_uuid("019f75dd875c7c819eda32f866b2c60f")); // 无分隔
}
