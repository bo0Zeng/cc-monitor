//! 要求：上游原始流在后端按上游协议折成归一事件，界面只收归一事件；Responses 那一种协议每类事件一条金样（手写片段，事件名照公开的 Responses 流），
//! 认不出的事件出空、不报错。

use super::*;
use crate::agents::{BlockKind, StreamEv};

#[test]
fn each_known_responses_event_folds_and_everything_else_is_silent() {
    assert_eq!(
        fold(
            r#"{"type":"response.created","sequence_number":0,"response":{"id":"resp_1","status":"in_progress"}}"#
        ),
        vec![StreamEv::Start {
            rid: "resp_1".into()
        }]
    );
    let block = |item: &str| {
        fold(&format!(
            r#"{{"type":"response.output_item.added","output_index":3,"item":{item}}}"#
        ))
    };
    assert_eq!(
        block(r#"{"type":"message","role":"assistant","content":[]}"#),
        vec![StreamEv::Block {
            i: 3,
            kind: BlockKind::Text,
            tool: None
        }]
    );
    assert_eq!(
        block(r#"{"type":"reasoning","summary":[]}"#),
        vec![StreamEv::Block {
            i: 3,
            kind: BlockKind::Thinking,
            tool: None
        }]
    );
    assert_eq!(
        block(r#"{"type":"function_call","name":"shell","arguments":""}"#),
        vec![StreamEv::Block {
            i: 3,
            kind: BlockKind::Tool,
            tool: Some("shell".into())
        }]
    );
    assert_eq!(
        block(r#"{"type":"custom_tool_call","name":"exec","input":""}"#),
        vec![StreamEv::Block {
            i: 3,
            kind: BlockKind::Tool,
            tool: Some("exec".into())
        }]
    );
    assert_eq!(
        block(r#"{"type":"web_search_call","status":"in_progress"}"#),
        vec![StreamEv::Block {
            i: 3,
            kind: BlockKind::Tool,
            tool: None
        }]
    );
    assert_eq!(
        block(r#"{"type":"image_generation_hint"}"#),
        vec![StreamEv::Block {
            i: 3,
            kind: BlockKind::Other,
            tool: None
        }]
    );
    for t in [
        "response.output_text.delta",
        "response.reasoning_summary_text.delta",
        "response.reasoning_text.delta",
    ] {
        assert_eq!(
            fold(&format!(
                r#"{{"type":"{t}","output_index":1,"content_index":0,"delta":"hi"}}"#
            )),
            vec![StreamEv::Text {
                i: 1,
                s: "hi".into()
            }],
            "{t}"
        );
    }
    assert_eq!(
        fold(r#"{"type":"response.completed","response":{"id":"resp_1"}}"#),
        vec![StreamEv::Stop { ok: true }]
    );
    for t in ["response.failed", "response.incomplete", "error"] {
        assert_eq!(
            fold(&format!(r#"{{"type":"{t}"}}"#)),
            vec![StreamEv::Stop { ok: false }],
            "{t}"
        );
    }
    for quiet in [
        r#"{"type":"response.in_progress","response":{"id":"resp_1"}}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":2,"delta":"{"}"#,
        r#"{"type":"response.custom_tool_call_input.delta","output_index":2,"delta":"ls"}"#,
        r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message"}}"#,
        r#"{"type":"response.created","response":{"id":""}}"#,
        r#"{"type":"response.output_text.delta","delta":"no index"}"#,
        r#"{"type":"message_start","message":{"id":"m1"}}"#,
        r#"{"type":"response.future_thing"}"#,
        "not json",
    ] {
        assert!(fold(quiet).is_empty(), "不该出事件：{quiet}");
    }
}

/// ★ 截断的那一件（开头 / 收尾那几件带整份应答对象、常超上限）：只从开头认类型与 `response.id`；
/// 认不出（id 在截断处之后 · 类型不是那几样 · 根本不是对象）⇒ 空，绝不出字。正文换成占位。
#[test]
fn a_clipped_event_yields_only_what_its_head_names() {
    let cap = comms_outward::test_support::tee::TAP_DATA_CAP;
    let pad = "x".repeat(cap * 2);
    let clip = |s: &str| s[..cap.min(s.len())].to_string();
    let created = format!(
        r#"{{"type":"response.created","sequence_number":0,"response":{{"id":"resp_9","object":"response","created_at":1,"status":"in_progress","instructions":"{pad}","tools":[]}}}}"#
    );
    assert_eq!(
        fold_clipped(&clip(&created)),
        vec![StreamEv::Start {
            rid: "resp_9".into()
        }]
    );
    let completed = format!(
        r#"{{"type":"response.completed","sequence_number":9,"response":{{"id":"resp_9","output":[{{"type":"message","content":[{{"type":"output_text","text":"{pad}"}}]}}]}}}}"#
    );
    assert_eq!(
        fold_clipped(&clip(&completed)),
        vec![StreamEv::Stop { ok: true }]
    );
    assert_eq!(
        fold_clipped(&clip(&format!(
            r#"{{"type":"response.failed","response":{{"error":"{pad}"}}}}"#
        ))),
        vec![StreamEv::Stop { ok: false }]
    );
    for quiet in [
        // id 落在截断处之后 ⇒ 认不出开始（不猜）。
        format!(
            r#"{{"type":"response.created","response":{{"instructions":"{pad}","id":"resp_9"}}}}"#
        ),
        // 字的增量被截断 ⇒ 半截内容不是内容，不出字。
        format!(r#"{{"type":"response.output_text.delta","output_index":0,"delta":"{pad}"}}"#),
        format!(r#"{{"pad":"{pad}","type":"response.created"}}"#),
        format!("[{pad}"),
    ] {
        assert!(
            fold_clipped(&clip(&quiet)).is_empty(),
            "不该出事件：{}",
            &quiet[..60]
        );
    }
}
