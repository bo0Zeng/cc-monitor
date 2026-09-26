//! # 要求住址：`设计/10 §2.2` ＋ `设计/90 §4` 阶段 C
//!
//! 核原文：`10 §2.2`「**『这个会话到目前为止是什么样』＝ 全会话事实 ＝ 读 json** …… 后端要提供的查询都是『读一遍文件』级别：
//! …… 改动文件集 / agent 列表 / 分叉血缘（没做）」；`90 §4` C「旁路记账员改读 json（`设计/10 §2.2` 步 5）」。
//! 本族钉的是**口径**（逐格、从前端旧实现逐字搬来的那几条）与**续传**（接力扫 == 一次扫完）。
//!
//! 夹具只造**结构**（记录类型、工具名、键名、占位串），不采任何真会话正文。

use super::*;
use serde_json::json;

/// 一份夹具会话：每条记录一行（`\n` 收尾）。
fn jsonl(records: &[Value]) -> String {
    records.iter().map(|r| format!("{r}\n")).collect()
}

fn scan_all(text: &str) -> SessionFacts {
    scan_facts(text.as_bytes(), SessionFacts::default()).unwrap()
}

fn tool_use(id: &str, name: &str, input: Value) -> Value {
    json!({"type": "tool_use", "id": id, "name": name, "input": input})
}

fn assistant(blocks: Vec<Value>) -> Value {
    json!({"type": "assistant", "timestamp": "t-a", "message": {"content": blocks}})
}

fn result(id: &str) -> Value {
    json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": "x"}]}})
}

/// 一份把四格都走到的夹具（结构占位）。
fn mixed() -> Vec<Value> {
    vec![
        json!({"type": "system", "forkedFrom": {"sessionId": "sys-not-counted", "messageUuid": "m0"}}),
        json!({"type": "user", "forkedFrom": {"sessionId": "only-sid"}, "message": {"content": "q"}}),
        json!({"type": "user", "forkedFrom": {"sessionId": "src-1", "messageUuid": "m1"}, "message": {"content": "q"}}),
        assistant(vec![
            tool_use("e1", "Edit", json!({"file_path": "/p/a.ts"})),
            tool_use(
                "g1",
                "Task",
                json!({"description": "  look around  ", "subagent_type": "Explore"}),
            ),
        ]),
        json!({"type": "assistant", "message": {"model": "m-x", "usage": {"input_tokens": 3, "cache_creation_input_tokens": 4, "cache_read_input_tokens": 5}, "content": [{"type": "text", "text": "y"}]}}),
        assistant(vec![tool_use(
            "n1",
            "NotebookEdit",
            json!({"notebook_path": "/p/n.ipynb"}),
        )]),
        result("g1"),
        assistant(vec![
            tool_use("w1", "Write", json!({"file_path": "/p/b.ts"})),
            tool_use("e2", "Edit", json!({"file_path": "/p/a.ts"})),
            tool_use("g2", "Agent", json!({"prompt": "first line\nsecond"})),
        ]),
        json!({"type": "assistant", "message": {"usage": {"input_tokens": 0}, "content": []}}),
        json!({"type": "user", "forkedFrom": {"sessionId": "src-2", "messageUuid": "m2"}, "message": {"content": "q"}}),
    ]
}

/// ★ 四格逐格：分叉只认 user/assistant 且两个键都在、首条锁定 · 改动文件近因序去重 · agent 配对 · usage 取最后一条有效的。
#[test]
fn the_four_facts_follow_the_moved_rules() {
    let f = scan_all(&jsonl(&mixed()));
    assert_eq!(f.forked_from.as_deref(), Some("src-1"));
    assert_eq!(f.touched_files, vec!["/p/n.ipynb", "/p/b.ts", "/p/a.ts"]);
    assert_eq!(
        f.agents,
        vec![
            AgentFact {
                id: "g1".into(),
                label: "look around".into(),
                agent_type: Some("Explore".into()),
                status: AgentStatus::Done,
                timestamp: "t-a".into(),
                desc: "look around".into(),
            },
            AgentFact {
                id: "g2".into(),
                label: "first line".into(),
                agent_type: None,
                status: AgentStatus::Running,
                timestamp: "t-a".into(),
                desc: String::new(),
            },
        ]
    );
    assert_eq!(
        f.usage,
        Some(UsageFact {
            prompt_tokens: 12,
            model: Some("m-x".into())
        }),
        "全 0 的那条不算；model 缺 ⇒ null 的那条没出现在最后"
    );
}

/// `label` 的三级回退：description ‖ prompt 首行前 80 字 ‖ 工具名；同一个 id 再来一次原位替换（位置不变、状态回 running）。
#[test]
fn label_falls_back_and_a_repeated_id_is_replaced_in_place() {
    let long: String = "字".repeat(100);
    let f = scan_all(&jsonl(&[
        assistant(vec![
            tool_use("a", "Task", json!({})),
            tool_use("b", "Task", json!({"prompt": long.clone()})),
            tool_use(
                "c",
                "Task",
                json!({"description": "   ", "prompt": "\nsecond"}),
            ),
        ]),
        result("a"),
        assistant(vec![tool_use("a", "Task", json!({"description": "again"}))]),
    ]));
    let labels: Vec<(&str, &str, AgentStatus)> = f
        .agents
        .iter()
        .map(|a| (a.id.as_str(), a.label.as_str(), a.status))
        .collect();
    let eighty: String = "字".repeat(LABEL_PROMPT_CHARS);
    assert_eq!(
        labels,
        vec![
            ("a", "again", AgentStatus::Running),
            ("b", eighty.as_str(), AgentStatus::Running),
            ("c", "Task", AgentStatus::Running),
        ]
    );
}

/// 上界：agent 软上界从最老删非 running · 硬上界删最老 · 改动文件超上界丢最久没碰的。
#[test]
fn every_list_is_bounded() {
    // 40 个 agent，前 20 个有结果 ⇒ 软上界删掉最老的 10 个 done，留 10 done ＋ 20 running。
    let mut recs = Vec::new();
    for i in 0..40 {
        recs.push(assistant(vec![tool_use(
            &format!("s{i}"),
            "Task",
            json!({}),
        )]));
        if i < 20 {
            recs.push(result(&format!("s{i}")));
        }
    }
    recs.push(assistant(vec![]));
    let f = scan_all(&jsonl(&recs));
    assert_eq!(f.agents.len(), AGENTS_SOFT_KEEP);
    assert_eq!(f.agents[0].id, "s10");
    assert_eq!(
        f.agents
            .iter()
            .filter(|a| a.status == AgentStatus::Running)
            .count(),
        20
    );

    // 全是 running ⇒ 软上界删不动，硬上界删最老的。
    let recs: Vec<Value> = (0..AGENTS_HARD_KEEP + 5)
        .map(|i| assistant(vec![tool_use(&format!("r{i}"), "Agent", json!({}))]))
        .collect();
    let f = scan_all(&jsonl(&recs));
    assert_eq!(f.agents.len(), AGENTS_HARD_KEEP);
    assert_eq!(f.agents[0].id, "r5");

    let recs: Vec<Value> = (0..TOUCHED_FILES_KEEP + 3)
        .map(|i| {
            assistant(vec![tool_use(
                "x",
                "Write",
                json!({"file_path": format!("/f{i}")}),
            )])
        })
        .collect();
    let f = scan_all(&jsonl(&recs));
    assert_eq!(f.touched_files.len(), TOUCHED_FILES_KEEP);
    assert_eq!(f.touched_files[0], "/f3");
    assert_eq!(
        f.touched_files.last().unwrap(),
        &format!("/f{}", TOUCHED_FILES_KEEP + 2)
    );
}

/// torn 残尾不看、不计进 `end`；空行、坏行照样推进 `end`。
#[test]
fn only_complete_lines_count() {
    let body = format!(
        "{}\n\nnot json\n{}",
        assistant(vec![tool_use("e", "Edit", json!({"file_path": "/p/a"}))]),
        "{\"type\":\"assistant\",\"message\":{\"usage\":{\"input_tokens\":9"
    );
    let f = scan_all(&body);
    assert_eq!(f.touched_files, vec!["/p/a"]);
    assert_eq!(f.usage, None, "残尾那条带 usage，但没写完 ⇒ 不算");
    assert_eq!(f.end as usize, body.rfind('\n').unwrap() + 1);
}

/// ★★ **续传 == 一次扫完**：夹具在**每一个**行边界切成两段，前一段的成品（过一遍 JSON，与线上回传同形）
/// 当 `prior` 接着扫后一段 ⇒ 与一次扫完逐格相等（两向：`assert_eq` 比整个结构）。
#[test]
fn resuming_from_any_line_boundary_equals_one_pass() {
    let mut recs = mixed();
    recs.extend((0..35).map(|i| assistant(vec![tool_use(&format!("k{i}"), "Task", json!({}))])));
    recs.push(result("k3"));
    recs.push(assistant(vec![]));
    let text = jsonl(&recs);
    let whole = scan_all(&text);
    let mut cuts = vec![0usize];
    cuts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    assert!(cuts.len() > recs.len(), "切点没取到");
    for cut in cuts {
        let head = scan_all(&text[..cut]);
        assert_eq!(head.end as usize, cut);
        let wire = serde_json::to_value(&head).unwrap();
        let prior = prior_from(&wire).expect("自己出的成品必须能原样回传");
        let resumed = scan_facts(&text.as_bytes()[cut..], prior).unwrap();
        assert_eq!(resumed, whole, "在字节 {cut} 处接力，结果与一次扫完不同");
    }
}

/// ★ 快路只省时间、不改结果：同一批行，「先过 `could_matter` 再解析」与「每行都解析」逐格相等。
/// 样本特意放了会骗过粗糙过滤的行：工具结果里夹着 `"tool_use"` 字样、分叉锁定后又来一条 `forkedFrom`、
/// 没有 running agent 时来的 `tool_result`。
#[test]
fn the_fast_path_never_changes_the_answer() {
    let mut recs = mixed();
    recs.push(json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "zz", "content": "\"tool_use\" \"usage\""}]}}));
    recs.push(result("g2"));
    recs.push(result("g2"));
    recs.push(json!({"type": "attachment", "x": 1}));
    let text = jsonl(&recs);
    let fast = scan_all(&text);
    let mut slow = SessionFacts::default();
    for line in text.lines() {
        slow.end += line.len() as u64 + 1;
        if let Some(v) = parse_line(line.as_bytes()) {
            note_record(&mut slow, &v);
        }
    }
    assert_eq!(fast, slow);
    // 正控：过滤真的在拦东西（不然上面那条等式是恒真）。
    let f = SessionFacts {
        forked_from: Some("x".into()),
        ..SessionFacts::default()
    };
    assert!(!could_matter(
        br#"{"type":"user","forkedFrom":{"sessionId":"y"}}"#,
        &f
    ));
    assert!(!could_matter(
        br#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#,
        &f
    ));
}

/// `prior` 形状必须恰好是本文件出的那一形：缺格 / 多格 / 类型不对 ⇒ 拒（不把缺格读成「没有」）。
#[test]
fn a_prior_of_the_wrong_shape_is_refused() {
    let good = serde_json::to_value(scan_all(&jsonl(&mixed()))).unwrap();
    assert!(prior_from(&good).is_ok());
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("usage");
    let mut extra = good.clone();
    extra["more"] = json!(1);
    let mut bad_type = good.clone();
    bad_type["end"] = json!("12");
    let mut agent_missing = good.clone();
    agent_missing["agents"][0]
        .as_object_mut()
        .unwrap()
        .remove("agentType");
    let mut bad_status = good.clone();
    bad_status["agents"][0]["status"] = json!("aborted");
    let mut usage_extra = good.clone();
    usage_extra["usage"]["x"] = json!(0);
    for (what, v) in [
        ("缺一格", missing),
        ("多一格", extra),
        ("类型不对", bad_type),
        ("agent 缺一格", agent_missing),
        ("中止不是后端的态", bad_status),
        ("usage 多一格", usage_extra),
        ("不是对象", json!([1])),
    ] {
        assert!(prior_from(&v).is_err(), "{what} 的 prior 被收下了");
    }
}

/// 生成物里 `agent: "<名>"` 那一行之后第一个 `<键>: [...]` 的数组（取字面量里的串，按原序）。
fn generated_array(table: &str, agent: &str, key: &str) -> Vec<String> {
    let at = table
        .find(&format!("agent: \"{agent}\""))
        .unwrap_or_else(|| {
            panic!("生成物里找不到 agent {agent:?} 那一行 —— 生成物的形状变了，本对拍在空转")
        });
    let rest = &table[at..];
    let k = rest
        .find(&format!("{key}: ["))
        .unwrap_or_else(|| panic!("{agent:?} 那一行里找不到 `{key}: [` —— 生成物的形状变了"));
    let body = &rest[k + key.len() + 3..];
    let body = &body[..body.find(']').expect("数组没有收尾的 `]`")];
    body.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// ★ 后端这份 agent 工具名 == monitor 那份（经生成物 `src/generated/agent-profile-table.ts`，门禁 `generated` 那一格
/// 守着它 == `adapter.rs`）。两边任一边改了而另一边没跟 ⇒ 红（异源：一侧是本 crate 的常量，另一侧是 monitor 的生成物）。
#[test]
fn the_agent_tools_equal_the_generated_profile_row() {
    let table = include_str!("../../../src/generated/agent-profile-table.ts");
    let mut want = generated_array(table, "claude", "agentTools");
    want.sort();
    let mut got: Vec<String> = AGENT_TOOLS.iter().map(|s| s.to_string()).collect();
    got.sort();
    assert_eq!(
        got, want,
        "后端 `AGENT_TOOLS` 与生成物里 claude 的 `agentTools` 不相等"
    );
}

/// 正控：同一个取数函数对一段手写的样本取得出东西（取数坏了在这里先红，而不是让上一条恒等）。
#[test]
fn the_generated_array_reader_sees_a_synthetic_row() {
    let sample = "{ agent: \"x\", agentTools: null }, { agent: \"claude\", other: [\"no\"], agentTools: [\"B\", \"A\"], y: [] }";
    assert_eq!(
        generated_array(sample, "claude", "agentTools"),
        vec!["B", "A"]
    );
}

#[test]
fn the_two_lookups_answer_from_their_tables() {
    for t in AGENT_TOOLS {
        assert!(is_agent_tool(t));
    }
    assert!(!is_agent_tool("Bash"));
    assert!(!is_agent_tool("task"), "大小写敏感：工具名原样比对");
    for (name, key) in EDIT_TOOL_PATH_KEYS {
        assert_eq!(edit_path_key(name), Some(*key));
    }
    assert_eq!(edit_path_key("Read"), None);
}
