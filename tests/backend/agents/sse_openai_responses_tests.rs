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
