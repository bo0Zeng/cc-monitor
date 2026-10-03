//! 要求：Claude Code 的子运行形状只住适配层，几格各答各的（对账键 · 归属与终局 · 派出链接 · 子运行记录住址）。期望手写，夹具只采结构。

use super::*;
use crate::agents::{RunDid, RunEnd};
use serde_json::json;

#[test]
fn response_id_is_the_assistant_message_id_only() {
    assert_eq!(
        response_id(&json!({"type":"assistant","message":{"id":"m1"}})).as_deref(),
        Some("m1")
    );
    assert_eq!(
        response_id(&json!({"type":"user","message":{"id":"m1"}})),
        None
    );
    assert_eq!(
        response_id(&json!({"type":"assistant","message":{"id":""}})),
        None
    );
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
    assert_eq!(
        (m.run.as_str(), m.end, m.did),
        (
            "a1",
            None,
            Some(RunDid::Tool {
                name: "Bash".into()
            })
        )
    );
    assert_eq!(run_of(&rec(json!({"isSidechain": false}))), None);
    assert_eq!(run_of(&rec(json!({"agentId": ""}))), None);
    let done =
        rec(json!({"message":{"content":[{"type":"text","text":"x"}],"stop_reason":"end_turn"}}));
    assert_eq!(run_of(&done).unwrap().end, Some(RunEnd::Done));
    let mut failed = done.clone();
    failed["isApiErrorMessage"] = json!(true);
    assert_eq!(run_of(&failed).unwrap().end, Some(RunEnd::Failed));
    let cut = json!({"type":"user","isSidechain":true,"agentId":"a1","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}});
    assert_eq!(
        run_of(&cut).unwrap().end,
        Some(RunEnd::Stopped),
        "被打断 ⇒ 不会再有终局，按被叫停收"
    );
    let mut last_tool = rec(json!({}));
    last_tool["message"]["content"] = json!([{"type":"tool_use","name":"Handback"}]);
    assert_eq!(
        run_of(&last_tool).unwrap().end,
        None,
        "以一次工具调用收尾不是终局（收场看派出那一方）"
    );
}

#[test]
fn child_link_reads_the_call_and_its_result() {
    let long: String = "字".repeat(100);
    let call = json!({"type":"assistant","message":{"content":[
        {"type":"tool_use","id":"t1","name":"Agent","input":{"description":"  scan  ","subagent_type":"Explore"}},
        {"type":"tool_use","id":"t2","name":"Task","input":{"prompt":"first\nsecond"}},
        {"type":"tool_use","id":"t3","name":"Bash","input":{}},
        {"type":"tool_use","id":"t4","name":"Task","input":{"description":"   ","prompt":long}},
        {"type":"tool_use","id":"t5","name":"Task","input":{}}
    ]}});
    let l = child_link(&call);
    let eighty: String = "字".repeat(LABEL_PROMPT_CHARS);
    assert_eq!(
        l.iter()
            .map(|x| (
                x.tool.as_deref(),
                x.label.as_deref(),
                x.kind.as_deref(),
                x.run.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some("t1"), Some("scan"), Some("Explore"), None),
            (Some("t2"), Some("first"), None, None),
            (Some("t4"), Some(eighty.as_str()), None, None),
            (Some("t5"), Some("Task"), None, None),
        ],
        "标签：description ‖ prompt 首行前 80 字 ‖ 工具名"
    );
    let result = |status: &str, is_error: bool| json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":is_error}]},"toolUseResult":{"status":status,"agentId":"a1"}});
    let got = |v: &serde_json::Value| {
        child_link(v)
            .into_iter()
            .map(|l| (l.tool, l.run, l.end))
            .collect::<Vec<_>>()
    };
    let t1 = || Some("t1".to_string());
    let a1 = || Some("a1".to_string());
    assert_eq!(
        got(&result("async_launched", false)),
        vec![(t1(), a1(), None)],
        "后台派出当场回的那次：对上是哪个，不算收场"
    );
    assert_eq!(
        got(&result("completed", false)),
        vec![(t1(), a1(), Some(RunEnd::Done))],
        "前台：拿到结果 ⇒ 完成"
    );
    assert_eq!(
        got(&result("completed", true)),
        vec![(t1(), a1(), Some(RunEnd::Failed))]
    );
    let plain =
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t9"}]}});
    assert!(child_link(&plain).is_empty());
}

/// 后台派出的收场通知：三处住址（排队 · 附件 · user 字符串正文），只读 `task-id` 与 `status`；正文里夹的同名标签不算，不是通知打头的不算。
#[test]
fn a_task_notice_ends_the_run_it_names() {
    let n = |status: &str| {
        format!("<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>t1</tool-use-id>\n<status>{status}</status>\n<summary>s</summary>\n<result><status>completed</status></result>\n</task-notification>")
    };
    let ends = |v: serde_json::Value| {
        child_link(&v)
            .into_iter()
            .map(|l| (l.tool, l.run, l.end))
            .collect::<Vec<_>>()
    };
    let one = |e| vec![(None, Some("a1".to_string()), Some(e))];
    assert_eq!(
        ends(json!({"type":"queue-operation","operation":"enqueue","content":n("completed")})),
        one(RunEnd::Done)
    );
    assert_eq!(
        ends(
            json!({"type":"attachment","attachment":{"type":"queued_command","prompt":n("failed")}})
        ),
        one(RunEnd::Failed)
    );
    assert_eq!(
        ends(json!({"type":"user","message":{"role":"user","content":n("killed")}})),
        one(RunEnd::Stopped)
    );
    assert_eq!(
        ends(json!({"type":"queue-operation","content":n("stopped")})),
        one(RunEnd::Stopped)
    );
    assert!(
        ends(json!({"type":"queue-operation","content":n("running")})).is_empty(),
        "认不得的状态不算收场"
    );
    assert!(
        ends(json!({"type":"queue-operation","content":"<task-notification><task-id>a1</task-id><summary>s</summary><event>e</event></task-notification>"}))
            .is_empty(),
        "没有状态那一格的（事件）不算收场"
    );
    assert!(
        ends(json!({"type":"queue-operation","content":format!("x {}", n("completed"))}))
            .is_empty(),
        "不是通知打头的不算"
    );
    assert!(
        ends(json!({"type":"assistant","message":{"content":[{"type":"text","text":n("completed")}]}}))
            .is_empty(),
        "回复里引用的通知不算"
    );
}

/// 预筛只许放过、不许误拦：上面每一种会答出东西的记录，它都认；一行普通记录它不认。
#[test]
fn the_parent_hint_covers_every_line_child_link_answers() {
    let lines = [
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{}}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Task","input":{}}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]},"toolUseResult":{"status":"completed","agentId":"a1"}}"#,
        r#"{"type":"queue-operation","content":"<task-notification><task-id>a1</task-id><status>completed</status></task-notification>"}"#,
    ];
    for l in lines {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        assert!(!child_link(&v).is_empty(), "夹具本身该答出东西：{l}");
        assert!(hint(l), "预筛拦了一行会答出东西的：{l}");
    }
    assert!(!hint(
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#
    ));
}

#[test]
fn sources_are_the_agent_records_under_the_parents_own_directory() {
    let d = std::env::temp_dir().join(format!("cc-runs-src-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let parent = d.join("s1.jsonl");
    let sub = d.join("s1").join("subagents");
    std::fs::create_dir_all(sub.join("deep")).unwrap();
    for f in [
        sub.join("agent-a.jsonl"),
        sub.join("deep").join("agent-b.jsonl"),
        sub.join("agent-a.meta.json"),
        sub.join("other.jsonl"),
    ] {
        std::fs::write(f, "{}\n").unwrap();
    }
    assert_eq!(
        sources(&parent),
        vec![
            sub.join("agent-a.jsonl"),
            sub.join("deep").join("agent-b.jsonl")
        ]
    );
    assert!(sources(&d.join("none.jsonl")).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_api_error_record_ends_the_run_as_failed_whatever_its_stop_reason() {
    // API 报错的那条是合成的 assistant 记录，stop_reason 不是 end_turn（现场见过 stop_sequence 与 refusal）。
    for stop in ["stop_sequence", "refusal", "end_turn"] {
        let v = json!({"type":"assistant","isSidechain":true,"agentId":"a1","isApiErrorMessage":true,
            "message":{"id":"m1","content":[{"type":"text","text":"x"}],"stop_reason":stop}});
        assert_eq!(
            run_of(&v).unwrap().end,
            Some(RunEnd::Failed),
            "stop_reason={stop}"
        );
    }
    let ok = json!({"type":"assistant","isSidechain":true,"agentId":"a1",
        "message":{"id":"m1","content":[{"type":"text","text":"x"}],"stop_reason":"stop_sequence"}});
    assert_eq!(
        run_of(&ok).unwrap().end,
        None,
        "不是 API 报错的 stop_sequence 不算收场"
    );
}

/// `owner` 是 `sources` 的反方向：嵌套几层都认得回父记录；别的形状（元数据 · 父记录本身 · 不在子 agent 目录里）一律不认。
#[test]
fn owner_maps_a_child_record_back_to_its_parent() {
    let p = |s: &str| Path::new(s).to_path_buf();
    assert_eq!(
        owner(&p("/h/projects/-p/s1/subagents/agent-a.jsonl")),
        Some(p("/h/projects/-p/s1.jsonl"))
    );
    assert_eq!(
        owner(&p("/h/projects/-p/s1/subagents/x/y/agent-b.jsonl")),
        Some(p("/h/projects/-p/s1.jsonl"))
    );
    for other in [
        "/h/projects/-p/s1/subagents/agent-a.meta.json",
        "/h/projects/-p/s1.jsonl",
        "/h/projects/-p/s1/tool-results/agent-a.jsonl",
        "/h/projects/-p/s1/subagents/a/b/c/d/e/agent-z.jsonl",
    ] {
        assert_eq!(owner(&p(other)), None, "{other}");
    }
}
