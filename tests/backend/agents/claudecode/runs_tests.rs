//! 要求：Claude Code 的子运行形状只住适配层，几格各答各的（对账键 · 归属与终局 · 派出链接 · 子运行记录住址）。期望手写，夹具只采结构。

use super::*;
use crate::agents::{RunDid, RunEnd};
use serde_json::json;

#[test]
fn response_id_is_the_assistant_message_id_only() {
    assert_eq!(response_id(&json!({"type":"assistant","message":{"id":"m1"}})).as_deref(), Some("m1"));
    assert_eq!(response_id(&json!({"type":"user","message":{"id":"m1"}})), None);
    assert_eq!(response_id(&json!({"type":"assistant","message":{"id":""}})), None);
}

#[test]
fn run_of_needs_both_marks_and_reads_the_end_and_the_last_thing() {
    let rec = |extra: serde_json::Value| {
        let mut v = json!({"type":"assistant","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"tool_use","name":"Bash"}],"stop_reason":"tool_use"}});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let m = run_of(&rec(json!({}))).unwrap();
    assert_eq!((m.run.as_str(), m.end, m.did), ("a1", None, Some(RunDid::Tool { name: "Bash".into() })));
    assert_eq!(run_of(&rec(json!({"isSidechain": false}))), None);
    assert_eq!(run_of(&rec(json!({"agentId": ""}))), None);
    let done = rec(json!({"message":{"content":[{"type":"text","text":"x"}],"stop_reason":"end_turn"}}));
    assert_eq!(run_of(&done).unwrap().end, Some(RunEnd::Done));
    let mut failed = done.clone();
    failed["isApiErrorMessage"] = json!(true);
    assert_eq!(run_of(&failed).unwrap().end, Some(RunEnd::Failed));
    let cut = json!({"type":"user","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}});
    assert_eq!(run_of(&cut).unwrap().end, Some(RunEnd::Failed), "被打断 ⇒ 不会再有终局，按失败收");
}

#[test]
fn child_link_reads_the_call_and_its_result() {
    let call = json!({"type":"assistant","message":{"content":[
        {"type":"tool_use","id":"t1","name":"Agent","input":{"description":"  scan  ","subagent_type":"Explore"}},
        {"type":"tool_use","id":"t2","name":"Task","input":{"prompt":"first\nsecond"}},
        {"type":"tool_use","id":"t3","name":"Bash","input":{}}
    ]}});
    let l = child_link(&call);
    assert_eq!(
        l.iter().map(|x| (x.tool.as_str(), x.label.as_deref(), x.kind.as_deref(), x.run.as_deref())).collect::<Vec<_>>(),
        vec![("t1", Some("scan"), Some("Explore"), None), ("t2", Some("first"), None, None)]
    );
    let launched = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]},"toolUseResult":{"status":"async_launched","agentId":"a1"}});
    let l = child_link(&launched);
    assert_eq!((l[0].tool.as_str(), l[0].run.as_deref(), l[0].end), ("t1", Some("a1"), None));
    let done = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]},"toolUseResult":{"status":"completed","agentId":"a1"}});
    assert_eq!(child_link(&done)[0].end, Some(RunEnd::Done));
    let plain = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t9"}]}});
    assert!(child_link(&plain).is_empty());
}

#[test]
fn sources_are_the_agent_records_under_the_parents_own_directory() {
    let d = std::env::temp_dir().join(format!("cc-runs-src-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let parent = d.join("s1.jsonl");
    let sub = d.join("s1").join("subagents");
    std::fs::create_dir_all(sub.join("deep")).unwrap();
    for f in [sub.join("agent-a.jsonl"), sub.join("deep").join("agent-b.jsonl"), sub.join("agent-a.meta.json"), sub.join("other.jsonl")] {
        std::fs::write(f, "{}\n").unwrap();
    }
    assert_eq!(sources(&parent), vec![sub.join("agent-a.jsonl"), sub.join("deep").join("agent-b.jsonl")]);
    assert!(sources(&d.join("none.jsonl")).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}
