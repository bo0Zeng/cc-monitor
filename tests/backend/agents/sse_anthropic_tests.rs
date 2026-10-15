//! 要求：上游原始流在后端按上游协议折成归一事件，界面只收归一事件（「原始 SSE 在后端折成归一事件，界面只收归一事件」）。
//!
//! 三条：① 四种认得的事件各折成哪一件、其余一律不出（期望手写）；② 台架夹具（`tests/__fixtures__/tap-bench.json`，
//! 合成的一轮）：折出来按对账键拼的正文 == 同一轮记录里同 id 的文字块按块序拼起来（流与记录对得上）；
//! ③ 每发用量：开头那一件按价比折输入 · 缓存读写，收尾前那一件折输出，别的事件不说。

use super::*;
use crate::agents::{BlockKind, StreamEv};

#[test]
fn the_four_known_events_fold_and_everything_else_is_silent() {
    assert_eq!(
        fold(r#"{"type":"message_start","message":{"id":"m1"}}"#),
        vec![StreamEv::Start { rid: "m1".into() }]
    );
    assert_eq!(
        fold(
            r#"{"type":"content_block_start","index":2,"content_block":{"type":"tool_use","name":"Bash"}}"#
        ),
        vec![StreamEv::Block {
            i: 2,
            kind: BlockKind::Tool,
            tool: Some("Bash".into())
        }]
    );
    assert_eq!(
        fold(r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}"#),
        vec![StreamEv::Block {
            i: 0,
            kind: BlockKind::Thinking,
            tool: None
        }]
    );
    assert_eq!(
        fold(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"hi"}}"#
        ),
        vec![StreamEv::Text {
            i: 1,
            s: "hi".into()
        }]
    );
    assert_eq!(
        fold(r#"{"type":"message_stop"}"#),
        vec![StreamEv::Stop { ok: true }]
    );
    assert_eq!(
        fold(r#"{"type":"error","error":{}}"#),
        vec![StreamEv::Stop { ok: false }]
    );
    for quiet in [
        r#"{"type":"ping"}"#,
        r#"{"type":"message_delta","delta":{}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        r#"{"type":"message_start","message":{"id":""}}"#,
        r#"{"type":"future_thing"}"#,
        "not json",
    ] {
        assert!(fold(quiet).is_empty(), "不该出事件：{quiet}");
    }
}

#[test]
fn the_bench_round_folds_into_the_same_text_its_records_carry() {
    let fx: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/tap-bench.json")).unwrap();
    let mut by_rid: std::collections::BTreeMap<String, String> = Default::default();
    let mut cur: Option<String> = None;
    for t in fx["taps"].as_array().unwrap() {
        let Some(d) = t["data"].as_str() else {
            continue;
        };
        for ev in fold(d) {
            match ev {
                StreamEv::Start { rid } => cur = Some(rid),
                StreamEv::Text { s, .. } => {
                    by_rid.entry(cur.clone().unwrap()).or_default().push_str(&s)
                }
                _ => {}
            }
        }
    }
    let mut recs: Vec<(String, u64, String)> = Vec::new();
    for r in fx["jsonl"].as_array().unwrap() {
        let Some(rid) = crate::agents::claudecode::runs::response_id(r) else {
            continue;
        };
        let text: String = r["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        recs.push((rid, r["apiBlockIndex"].as_u64().unwrap_or(0), text));
    }
    recs.sort();
    let mut from_records: std::collections::BTreeMap<String, String> = Default::default();
    for (rid, _, text) in recs {
        from_records.entry(rid).or_default().push_str(&text);
    }
    assert!(
        !from_records.is_empty(),
        "夹具里一条带对账键的记录都没有 —— 本条空转"
    );
    assert_eq!(by_rid, from_records, "流折出来的正文与记录对不上");
}

#[test]
fn usage_is_read_from_the_opening_and_closing_events_only() {
    let start = r#"{"type":"message_start","message":{"id":"m1","usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":1}}}"#;
    assert_eq!(
        spent(start),
        Some(Spent {
            units: 10.0 + 125.0 + 100.0,
            opens: true
        })
    );
    let tiers = r#"{"type":"message_start","message":{"usage":{"input_tokens":0,"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200},"cache_read_input_tokens":0}}}"#;
    assert_eq!(spent(tiers).map(|s| s.units), Some(125.0 + 400.0));
    let delta = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":10,"output_tokens":40}}"#;
    assert_eq!(
        spent(delta),
        Some(Spent {
            units: 200.0,
            opens: false
        })
    );
    for silent in [
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"\"usage\""}}"#,
        r#"{"type":"message_stop"}"#,
        "not json \"usage\"",
    ] {
        assert_eq!(spent(silent), None, "{silent}");
    }
}
