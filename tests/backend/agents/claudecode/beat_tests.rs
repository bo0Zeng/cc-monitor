//! Claude Code 的一条记录 ⇒ 一拍：每一格从哪读（表见 `agents/claudecode/beat.rs` 头注）。
use super::*;
use serde_json::json;

fn said_kind(b: &Beat) -> Option<&'static str> {
    b.said().map(|s| s.speaker.kind())
}

#[test]
fn a_reply_carries_its_calls_text_thinking_stop_and_usage_in_file_order() {
    let v = json!({
        "type": "assistant", "uuid": "a1", "timestamp": "2026-01-01T00:00:00Z", "requestId": "req-1",
        "message": {
            "model": "model-x", "stop_reason": "end_turn",
            "usage": {"input_tokens": 3, "output_tokens": 7, "cache_read_input_tokens": 11,
                      "cache_creation_input_tokens": 5,
                      "cache_creation": {"ephemeral_5m_input_tokens": 2, "ephemeral_1h_input_tokens": 3}},
            "content": [
                {"type": "thinking", "thinking": "…"},
                {"type": "text", "text": "先看看"},
                {"type": "tool_use", "id": "c1", "name": "Edit", "input": {"file_path": "/w/a.rs"}},
                {"type": "redacted_thinking"},
            ]
        }
    });
    let b = beat_of(&v).unwrap();
    assert_eq!(b.side, Side::Reply);
    assert_eq!(b.id.as_deref(), Some("a1"));
    assert_eq!(b.at.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert_eq!(
        b.acts,
        vec![
            Act::Thinking,
            Act::Text("先看看".into()),
            Act::Call {
                id: "c1".into(),
                name: "Edit".into(),
                what: Some("/w/a.rs".into()),
                edits: Some("/w/a.rs".into()),
            },
            Act::Thinking,
            Act::Stop(StopWhy::Done),
        ]
    );
    assert_eq!(
        b.usage,
        Some(Usage {
            input: 3,
            cache_read: 11,
            cache_write: 5,
            write_5m: Some(2),
            write_1h: Some(3),
            output: 7,
            request: Some("req-1".into()),
        })
    );
    assert_eq!(b.model.as_deref(), Some("model-x"));
    assert!(!b.injected);
}

#[test]
fn an_error_card_stops_failed_and_a_read_tool_has_no_edits() {
    let v = json!({"type": "assistant", "uuid": "a2", "isApiErrorMessage": true,
        "message": {"content": [{"type": "tool_use", "id": "c2", "name": "Read", "input": {"file_path": "/w/b"}}]}});
    let b = beat_of(&v).unwrap();
    assert!(b.stops(StopWhy::Failed));
    assert!(!b.stops(StopWhy::Done));
    assert!(matches!(&b.acts[0], Act::Call { edits: None, what: Some(w), .. } if w == "/w/b"));
    assert_eq!(b.usage, None, "没写用量就没有用量");
}

#[test]
fn an_input_carries_results_who_said_it_and_whether_the_host_injected_it() {
    let v = json!({"type": "user", "uuid": "u1", "isMeta": true,
    "message": {"content": [
        {"type": "tool_result", "tool_use_id": "c1", "is_error": true, "content": "boom"},
        {"type": "tool_result", "tool_use_id": "c2", "content": "ok"},
    ]}});
    let b = beat_of(&v).unwrap();
    assert_eq!(b.side, Side::Input);
    assert!(b.injected);
    assert!(b.has_result());
    assert_eq!(
        b.acts[..2],
        [
            Act::Result {
                of: Some("c1".into()),
                failed: true
            },
            Act::Result {
                of: Some("c2".into()),
                failed: false
            },
        ]
    );
    assert_eq!(said_kind(&b), Some("toolResult"));

    let human =
        json!({"type": "user", "uuid": "u2", "message": {"role": "user", "content": "你好"}});
    let b = beat_of(&human).unwrap();
    assert!(!b.injected && !b.has_result());
    assert_eq!(b.acts[0], Act::Text("你好".into()));
    assert_eq!(said_kind(&b), Some("human"));
    assert_eq!(b.usage, None, "用户那侧不出用量");
}

#[test]
fn bookkeeping_lines_say_retry_cost_and_mode_and_nothing_else() {
    let retry =
        json!({"type": "system", "subtype": "api_error", "uuid": "s1", "content": "上游失败"});
    let b = beat_of(&retry).unwrap();
    assert_eq!(b.side, Side::Note);
    assert!(b.acts.contains(&Act::Retry));
    assert_eq!(said_kind(&b), None);

    let cost = json!({"type": "cost-state", "totalCostUSD": 1.234567, "hasUnknownModelCost": true});
    assert_eq!(
        beat_of(&cost).unwrap().cost,
        Some(CostSaid {
            micros: 1_234_567,
            partial: true
        })
    );
    let bad = json!({"type": "cost-state", "totalCostUSD": -1.0});
    assert_eq!(beat_of(&bad).unwrap().cost, None, "负数不算");

    let mode = json!({"type": "permission-mode", "permissionMode": "plan"});
    assert_eq!(beat_of(&mode).unwrap().mode.as_deref(), Some("plan"));
    let other = json!({"type": "summary", "permissionMode": "plan", "totalCostUSD": 1.0});
    let b = beat_of(&other).unwrap();
    assert_eq!((b.mode, b.cost), (None, None), "别的类型带着同名格也不算");
}

#[test]
fn only_user_and_assistant_records_say_where_the_session_was_forked_from() {
    let f = json!({"sessionId": "src", "messageUuid": "m"});
    let u = json!({"type": "user", "forkedFrom": f, "message": {"content": "x"}});
    assert_eq!(beat_of(&u).unwrap().forked_from.as_deref(), Some("src"));
    let s = json!({"type": "system", "forkedFrom": f});
    assert_eq!(beat_of(&s).unwrap().forked_from, None);
}

#[test]
fn the_hint_lets_through_what_can_change_facts_and_only_mcp_when_asked_only_for_that() {
    let idle = Watch::default();
    let result = br#"{"type":"user","uuid":"u","message":{"content":[{"type":"tool_result","tool_use_id":"c9"}]}}"#;
    assert!(!could_matter(result, &idle), "别人的工具结果不解析");
    let mcp = br#"{"type":"attachment","uuid":"x","attachment":{"type":"deferred_tools_delta"}}"#;
    let only = Watch {
        mcp_only: true,
        ..Watch::default()
    };
    assert!(could_matter(mcp, &only));
    let fork = br#"{"type":"user","uuid":"u","forkedFrom":{}}"#;
    assert!(!could_matter(fork, &only), "只问 MCP 时别的都不放");
    assert!(could_matter(fork, &idle));
    let locked = Watch {
        fork_locked: true,
        ..Watch::default()
    };
    assert!(!could_matter(fork, &locked), "分叉已锁就不找它");
    let say = br#"{"type":"user","uuid":"u","message":{"content":"again"}}"#;
    assert!(!could_matter(say, &idle));
    let waiting = Watch {
        pending: true,
        ..Watch::default()
    };
    assert!(
        could_matter(say, &waiting),
        "有挂着的调用时人又说了一句 ⇒ 要看"
    );
}

#[test]
fn asks_and_plans_are_named_by_this_agents_tool_words() {
    assert_eq!(ask_of("AskUserQuestion"), CallAsk::Answer);
    assert_eq!(ask_of("ExitPlanMode"), CallAsk::Plan);
    assert_eq!(ask_of("Bash"), CallAsk::Plain);
}
