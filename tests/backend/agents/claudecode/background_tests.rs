//! 后台命令那几笔的读法（形状照上游真记录：起 · 当场回的结果带任务号 · 收场通知住三处）。

use super::*;
use serde_json::json;

fn notice(task: &str, call: &str, status: &str) -> String {
    format!(
        "<task-notification>\n<task-id>{task}</task-id>\n<tool-use-id>{call}</tool-use-id>\n<output-file>/x/{task}.output</output-file>\n<status>{status}</status>\n<summary>Background command \"Run the gate\" completed (exit code 0)</summary>\n</task-notification>"
    )
}

#[test]
fn a_background_call_starts_and_a_foreground_one_does_not() {
    let v = json!({"type":"assistant","message":{"content":[
        {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"make test-all","run_in_background":true}},
        {"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}},
        {"type":"tool_use","id":"t3","name":"Bash","input":{"command":"x","run_in_background":false}}
    ]}});
    assert_eq!(
        marks(&v),
        vec![BgMark::Started {
            call: "t1".into(),
            cmd: Some("make test-all".into())
        }]
    );
}

#[test]
fn the_launch_result_names_the_task_and_an_error_result_ends_it() {
    let ok = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Command running in background with ID: b1."}]},
        "toolUseResult":{"stdout":"","backgroundTaskId":"b1"}});
    assert_eq!(
        marks(&ok),
        vec![BgMark::Named {
            call: "t1".into(),
            task: "b1".into()
        }]
    );
    let bad = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"nope"}]}});
    assert_eq!(
        marks(&bad),
        vec![BgMark::Ended {
            call: Some("t1".into()),
            task: None
        }]
    );
    // 别的结果（没任务号）不说什么。
    let plain = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t9","content":"ok"}]}});
    assert!(marks(&plain).is_empty());
}

#[test]
fn the_end_notice_is_read_wherever_it_lives() {
    let want = vec![BgMark::Ended {
        call: Some("t1".into()),
        task: Some("b1".into()),
    }];
    for status in ["completed", "failed", "killed"] {
        let n = notice("b1", "t1", status);
        let q = json!({"type":"queue-operation","operation":"enqueue","content":n});
        let a = json!({"type":"attachment","attachment":{"type":"queued_command","prompt":n}});
        let u = json!({"type":"user","message":{"content":n}});
        let ub = json!({"type":"user","message":{"content":[{"type":"text","text":n}]}});
        for v in [q, a, u, ub] {
            assert_eq!(marks(&v), want, "{status}: {v}");
        }
    }
    // 不是通知的正文不说什么。
    assert!(marks(&json!({"type":"user","message":{"content":"hello"}})).is_empty());
}
