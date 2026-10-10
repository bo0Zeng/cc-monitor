//! # 阶段 C
//!
//! 核原文：「**『这个会话到目前为止是什么样』＝ 全会话事实 ＝ 读 json** …… 后端要提供的查询都是『读一遍文件』级别：
//! …… 改动文件集 / agent 列表 / 分叉血缘（没做）」（agent 列表今天是运行表，不在会话事实里）；「旁路记账员改读 json」。
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
    scan_facts(text.as_bytes(), SessionFacts::default(), &Vec::new(), None).unwrap()
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

/// 一份把三格都走到的夹具（结构占位；派出子运行的调用与它的结果也在，会话事实不认它们）。
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
        retry("rA1", 1),
        json!({"type": "system", "subtype": "local_command", "uuid": "x"}),
        retry("rA2", 2),
        json!({"type": "assistant", "uuid": "a9", "message": {"content": []}}),
        retry("rB1", 1),
    ]
}

/// 一次 API 重试（CLI 写的 `system` · `api_error` 那一形；结构占位）。
fn retry(uuid: &str, attempt: u32) -> Value {
    json!({"type": "system", "subtype": "api_error", "uuid": uuid, "retryAttempt": attempt, "maxRetries": 10, "error": {"status": 529}})
}

/// ★ 三格逐格：分叉只认 user/assistant 且两个键都在、首条锁定 · 改动文件近因序去重 · usage 取最后一条有效的。
#[test]
fn the_three_facts_follow_the_moved_rules() {
    let f = scan_all(&jsonl(&mixed()));
    assert_eq!(f.forked_from.as_deref(), Some("src-1"));
    assert_eq!(f.touched_files, vec!["/p/n.ipynb", "/p/b.ts", "/p/a.ts"]);
    assert_eq!(
        f.usage,
        Some(UsageFact {
            prompt_tokens: 12,
            model: Some("m-x".into()),
            peak_prompt_tokens: 12,
            limit: CONTEXT_EXTENDED,
            limit_from: LimitFrom::Assumed,
        }),
        "全 0 的那条不算；model 缺 ⇒ null 的那条没出现在最后"
    );
}

/// 上界：改动文件超上界丢最久没碰的。
#[test]
fn every_list_is_bounded() {
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
        let resumed = scan_facts(&text.as_bytes()[cut..], prior, &Vec::new(), None).unwrap();
        assert_eq!(resumed, whole, "在字节 {cut} 处接力，结果与一次扫完不同");
    }
}

/// ★ 快路只省时间、不改结果：同一批行，「先过 `could_matter` 再解析」与「每行都解析」逐格相等。
/// 样本特意放了会骗过粗糙过滤的行：工具结果里夹着 `"tool_use"` 字样、分叉锁定后又来一条 `forkedFrom`、
/// 工具结果。
#[test]
fn the_fast_path_never_changes_the_answer() {
    let mut recs = mixed();
    recs.push(json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "zz", "content": "\"tool_use\" \"usage\""}]}}));
    recs.push(result("g2"));
    recs.push(result("g2"));
    recs.push(json!({"type": "attachment", "x": 1}));
    recs.push(handback("fp-1", true));
    recs.push(mcp_delta(
        json!({"failedMcpServers": [{"name": "m-f", "error": "e"}]}),
    ));
    let text = jsonl(&recs);
    let fast = scan_all(&text);
    assert_eq!(fast.handed_back, ["fp-1"], "交回那一行没漏过快路");
    assert_eq!(fast.mcp.len(), 1, "MCP 那一行没漏过快路");
    let mut slow = SessionFacts::default();
    for line in text.lines() {
        slow.end += line.len() as u64 + 1;
        if let Some(v) = crate::observe::record_scan::parse_record(line.as_bytes()) {
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
    let mut usage_extra = good.clone();
    usage_extra["usage"]["x"] = json!(0);
    for (what, v) in [
        ("缺一格", missing),
        ("多一格", extra),
        ("类型不对", bad_type),
        ("usage 多一格", usage_extra),
        ("不是对象", json!([1])),
    ] {
        assert!(prior_from(&v).is_err(), "{what} 的 prior 被收下了");
    }
}

#[test]
fn the_edit_tool_lookup_answers_from_its_table() {
    for (name, key) in EDIT_TOOL_PATH_KEYS {
        assert_eq!(edit_path_key(name), Some(*key));
    }
    assert_eq!(edit_path_key("Read"), None);
}

/// 写类工具的口径 —— **逐条搬自前端被删的 `tests/frontend/ui/panorama/session-files.test.ts`**（七条，被测对象
/// `collectEditedFiles` 随搬家删了、口径住进本文件）：Edit / Write / MultiEdit 取 `file_path` · NotebookEdit 取
/// `notebook_path` · 非写类不收 · 多个全收保序 · 非 assistant 不收 · 畸形静默跳过 · Windows 路径原样收。
/// （单条记录内的顺序就是块序；去重 / 近因序那一格在 [`the_three_facts_follow_the_moved_rules`]。）
#[test]
fn edit_tools_rules_moved_from_the_frontend_suite() {
    let files = |recs: Vec<Value>| scan_all(&jsonl(&recs)).touched_files;
    let one = |name: &str, input: Value| files(vec![assistant(vec![tool_use("x", name, input)])]);
    assert_eq!(one("Edit", json!({"file_path": "/a.ts"})), vec!["/a.ts"]);
    assert_eq!(one("Write", json!({"file_path": "/b.rs"})), vec!["/b.rs"]);
    assert_eq!(
        one("MultiEdit", json!({"file_path": "/c.py"})),
        vec!["/c.py"]
    );
    assert_eq!(
        one("NotebookEdit", json!({"notebook_path": "/n.ipynb"})),
        vec!["/n.ipynb"]
    );
    assert!(files(vec![assistant(vec![
        tool_use("1", "Bash", json!({"command": "ls"})),
        tool_use("2", "Read", json!({"file_path": "/r.ts"})),
        tool_use("3", "Grep", json!({"pattern": "x"})),
    ])])
    .is_empty());
    assert_eq!(
        files(vec![assistant(vec![
            tool_use("1", "Edit", json!({"file_path": "/a"})),
            tool_use("2", "Bash", json!({"command": "x"})),
            tool_use("3", "Write", json!({"file_path": "/b"})),
        ])]),
        vec!["/a", "/b"]
    );
    assert!(files(vec![json!({"type": "user", "message": {"content": [tool_use("x", "Edit", json!({"file_path": "/a"}))]}})]).is_empty());
    for (what, rec) in [
        (
            "无 file_path",
            assistant(vec![tool_use("x", "Edit", json!({}))]),
        ),
        (
            "非字符串",
            assistant(vec![tool_use("x", "Edit", json!({"file_path": 123}))]),
        ),
        (
            "空串",
            assistant(vec![tool_use("x", "Edit", json!({"file_path": ""}))]),
        ),
        (
            "非 tool_use",
            assistant(vec![json!({"type": "text", "text": "hi"})]),
        ),
        ("无 content", json!({})),
        ("null", json!(null)),
        (
            "content 非数组",
            json!({"type": "assistant", "message": {"content": "notarray"}}),
        ),
    ] {
        assert!(files(vec![rec]).is_empty(), "{what}：应当静默跳过");
    }
    assert_eq!(
        one("Write", json!({"file_path": "C:\\proj\\a.ts"})),
        vec!["C:\\proj\\a.ts"]
    );
}

fn usage_rec(model: &str, prompt: u64) -> Value {
    json!({"type": "assistant", "message": {"model": model, "usage": {"input_tokens": prompt}, "content": []}})
}

/// ★ 上下文上限：记录里的模型名不带 `[1m]` ⇒ 中转没看见过时多数判不出（占位 1M、`Assumed`，界面不算百分比）；见过超过 200k 的一轮 ⇒ 必是 1M；
/// 设置里的上限表最长匹配胜，但给的数小于见过的最大一轮 ⇒ 那一档不对；任何情形上限都不小于见过的最大一轮。
#[test]
fn the_context_limit_is_decided_here_and_never_below_what_was_seen() {
    let none: ContextLimits = Vec::new();
    assert_eq!(
        context_limit(Some("claude-opus-5-5"), 350_000, &none, None),
        (CONTEXT_EXTENDED, LimitFrom::Observed)
    );
    assert_eq!(
        context_limit(Some("claude-opus-5-5"), 90_000, &none, None),
        (CONTEXT_EXTENDED, LimitFrom::Assumed)
    );
    assert_eq!(
        context_limit(Some("claude-opus-4-8[1m]"), 90_000, &none, None),
        (CONTEXT_EXTENDED, LimitFrom::Model)
    );
    assert_eq!(
        context_limit(None, 10, &none, None),
        (CONTEXT_EXTENDED, LimitFrom::Assumed)
    );
    let set: ContextLimits = vec![
        ("haiku".into(), 200_000),
        ("claude-haiku-9".into(), 150_000),
    ];
    assert_eq!(
        context_limit(Some("Claude-Haiku-9-x"), 90_000, &set, None),
        (150_000, LimitFrom::Setting),
        "最长匹配胜、不分大小写"
    );
    assert_eq!(
        context_limit(Some("claude-haiku-4"), 90_000, &set, None),
        (200_000, LimitFrom::Setting)
    );
    assert_eq!(
        context_limit(Some("claude-haiku-4"), 250_000, &set, None),
        (CONTEXT_EXTENDED, LimitFrom::Observed),
        "设置给小了"
    );
    assert_eq!(
        context_limit(Some("m"), 1_200_000, &none, None),
        (1_200_000, LimitFrom::Observed)
    );
}

/// ★ 一份会话：最新一轮掉下来了（压缩过），见过的最大一轮照样记着 ⇒ 上限仍是 1M；续传接力也一样（peak 跟着 prior 走）。
/// 设置表随每一次问交来：同一份 `prior` 换一张表再问 ⇒ 上限跟着变。
#[test]
fn the_peak_survives_a_compaction_and_a_resume_and_the_setting_applies_each_time() {
    let text = jsonl(&[
        usage_rec("claude-opus-5-5", 350_000),
        usage_rec("claude-opus-5-5", 40_000),
    ]);
    let f = scan_all(&text);
    let u = f.usage.clone().unwrap();
    assert_eq!(
        (u.prompt_tokens, u.peak_prompt_tokens, u.limit, u.limit_from),
        (40_000, 350_000, CONTEXT_EXTENDED, LimitFrom::Observed)
    );
    let cut = text.find('\n').unwrap() + 1;
    let head = prior_from(&serde_json::to_value(scan_all(&text[..cut])).unwrap()).unwrap();
    assert_eq!(
        scan_facts(&text.as_bytes()[cut..], head, &Vec::new(), None).unwrap(),
        f
    );

    let small = jsonl(&[usage_rec("claude-haiku-4", 90_000)]);
    let prior = prior_from(&serde_json::to_value(scan_all(&small)).unwrap()).unwrap();
    let again = scan_facts(&b""[..], prior, &vec![("haiku".into(), 200_000)], None).unwrap();
    let u = again.usage.unwrap();
    assert_eq!((u.limit, u.limit_from), (200_000, LimitFrom::Setting));
}

/// `limits` 入参：缺席 / null ⇒ 空表；不是「串 → 正整数」⇒ 拒。
#[test]
fn the_limits_argument_is_strict() {
    assert_eq!(limits_from(None).unwrap(), Vec::new());
    assert_eq!(limits_from(Some(&Value::Null)).unwrap(), Vec::new());
    assert_eq!(
        limits_from(Some(&json!({" Haiku ": 200000}))).unwrap(),
        vec![("haiku".to_string(), 200_000)]
    );
    for bad in [
        json!([1]),
        json!({"x": 0}),
        json!({"x": "1"}),
        json!({"x": -5}),
        json!({" ": 5}),
    ] {
        assert!(limits_from(Some(&bad)).is_err(), "{bad} 被收下了");
    }
}

/// ★ 真来源是中转：看见过这个会话的请求 ⇒ 带过扩展上下文那一项是 1M、没带过是默认 200k（压过设置表与模型名）；
/// 没看见过 ⇒ 照设置表 > `[1m]` > 见过超过 200k > 判不出。上限仍不低于见过的最大一轮。
#[test]
fn the_relay_tells_the_context_window_when_it_saw_the_session() {
    let none: ContextLimits = Vec::new();
    let set: ContextLimits = vec![("opus".into(), 500_000)];
    assert_eq!(
        context_limit(Some("claude-opus-5-5"), 90_000, &set, Some(true)),
        (CONTEXT_EXTENDED, LimitFrom::Relay)
    );
    assert_eq!(
        context_limit(Some("claude-opus-5-5[1m]"), 90_000, &none, Some(false)),
        (CONTEXT_STANDARD, LimitFrom::Relay)
    );
    assert_eq!(
        context_limit(Some("claude-opus-5-5"), 90_000, &set, None),
        (500_000, LimitFrom::Setting)
    );
    assert_eq!(
        context_limit(Some("claude-opus-5-5"), 350_000, &none, Some(false)),
        (CONTEXT_EXTENDED, LimitFrom::Observed),
        "中转说默认、却见过超过 200k 的一轮（中转起来之前的那几轮）⇒ 不可能是 200k"
    );
}

fn user_text(t: &str) -> Value {
    json!({"type": "user", "message": {"content": t}})
}

/// ★ 没结果的调用：结果来了按 id 摘、你又发一句全摘；文件序；主参数按工具取（提问取第一问，没登记的不给）。
#[test]
fn pending_calls_follow_their_results() {
    let recs = vec![
        assistant(vec![
            tool_use(
                "b1",
                "Bash",
                json!({"command": "rm -rf build/ && npm run build\nsecond"}),
            ),
            tool_use("r1", "Read", json!({"file_path": "/p/a.ts"})),
            tool_use("z1", "Unlisted", json!({"x": 1})),
        ]),
        result("r1"),
    ];
    let f = scan_all(&jsonl(&recs));
    let names: Vec<(&str, Option<&str>)> = f
        .pending
        .iter()
        .map(|p| (p.name.as_str(), p.what.as_deref()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Bash", Some("rm -rf build/ && npm run build")),
            ("Unlisted", None)
        ]
    );
    assert_eq!(f.pending[0].at.as_deref(), Some("t-a"));
    // 你又发了一句 ⇒ 那一轮过去了，没结果的全摘。
    let mut more = recs.clone();
    more.push(user_text("next"));
    assert!(scan_all(&jsonl(&more)).pending.is_empty());
    // 提问取第一问。
    let ask = vec![assistant(vec![tool_use(
        "q1",
        "AskUserQuestion",
        json!({"questions": [{"question": "release 也重试？"}, {"question": "second"}]}),
    )])];
    assert_eq!(
        scan_all(&jsonl(&ask)).pending[0].what.as_deref(),
        Some("release 也重试？")
    );
    // 上界：只留最近的 PENDING_KEEP 条。
    let many: Vec<Value> = (0..PENDING_KEEP + 3)
        .map(|i| {
            assistant(vec![tool_use(
                &format!("m{i}"),
                "Bash",
                json!({"command": "x"}),
            )])
        })
        .collect();
    let f = scan_all(&jsonl(&many));
    assert_eq!(f.pending.len(), PENDING_KEEP);
    assert_eq!(f.pending[0].id, "m3");
}

/// ★ 最后一句：文件序最后一段正文的头一个非空行，截到 SAY_CHARS。
#[test]
fn last_say_is_the_first_line_of_the_last_text() {
    let long = "字".repeat(SAY_CHARS + 5);
    let recs = vec![
        json!({"type": "assistant", "timestamp": "t1", "message": {"content": [{"type": "text", "text": "first"}]}}),
        json!({"type": "assistant", "timestamp": "t2", "message": {"content": [{"type": "text", "text": "\n\n  结论一行  \n细节"}]}}),
        assistant(vec![tool_use("b1", "Bash", json!({"command": "x"}))]),
    ];
    let f = scan_all(&jsonl(&recs));
    assert_eq!(
        f.last_say,
        Some(LastSay {
            text: "结论一行".into(),
            at: Some("t2".into())
        })
    );
    let f = scan_all(&jsonl(&[
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": long}]}}),
    ]));
    let t = f.last_say.unwrap().text;
    assert_eq!(t.chars().count(), SAY_CHARS + 1);
    assert!(t.ends_with('…'));
}

/// ★ 需手动：那台说在等才有；种类由「在等什么」（适配层翻好的 [`WaitOn`]）配记录里没结果的那一步判，判不出不猜。
/// 那一家的六个词逐个落到一种（不再有五个落进判不出）。
#[test]
fn needs_is_decided_from_the_wait_and_the_pending_call() {
    use crate::agents::WaitOn as W;
    let call = |id: &str, name: &str, what: Option<&str>| PendingCall {
        id: id.into(),
        name: name.into(),
        what: what.map(str::to_string),
        at: None,
        state: StepWait::Unclear,
        why: None,
    };
    let wait = |w: Option<W>| PidWait {
        waiting_for: w,
        since_ms: Some(42),
    };
    let bash = vec![call("b", "Bash", Some("rm -rf build/"))];
    let ask = vec![call("q", "AskUserQuestion", Some("要不要？"))];
    let plan = vec![call("p", "ExitPlanMode", None)];
    // 不在等 ⇒ 没有。
    assert_eq!(needs_of(&bash, None), None);
    // 批准框 ＋ 一步没结果 ⇒ 批准那一步。
    assert_eq!(
        needs_of(&bash, Some(&wait(Some(W::Permission)))),
        Some(Needs {
            kind: NeedsKind::Approve,
            tool: Some("Bash".into()),
            call: Some("b".into()),
            what: Some("rm -rf build/".into()),
            since_ms: Some(42),
            text: crate::common::cells::Words(copy_core::copy_text("beSession.needs.approve", &[])),
            tone: crate::common::cells::Tone::Need,
        })
    );
    // 批准框里是提问 ⇒ 回答；是计划 ⇒ 批准计划；那台没说是哪种框时同样认这两个工具。
    for w in [Some(W::Permission), None] {
        let n = needs_of(&ask, Some(&wait(w))).unwrap();
        assert_eq!(
            (n.kind, n.what.as_deref()),
            (NeedsKind::Answer, Some("要不要？"))
        );
        let n = needs_of(&plan, Some(&wait(w))).unwrap();
        assert_eq!(
            (n.kind, n.tool.as_deref(), n.call.as_deref(), n.what),
            (NeedsKind::Plan, Some("ExitPlanMode"), Some("p"), None)
        );
    }
    // 每个「在等什么」一种；表一行一个，`call` 列是那一种认不认记录里没结果的那一步。
    let kinds = [
        (W::Permission, NeedsKind::Approve, true),
        (W::Network, NeedsKind::Network, true),
        (W::Worker, NeedsKind::Worker, false),
        (W::Goal, NeedsKind::Goal, false),
        (W::Input, NeedsKind::Answer, true),
        (W::Dialog, NeedsKind::Choose, false),
    ];
    for (w, kind, with_call) in kinds {
        let n = needs_of(&bash, Some(&wait(Some(w)))).unwrap();
        assert_eq!(n.kind, kind, "{w:?}");
        assert_eq!(n.call.is_some(), with_call, "{w:?}");
        // 记录里没有没结果的调用：种类照旧，只是说不出是哪一步。
        let n = needs_of(&[], Some(&wait(Some(w)))).unwrap();
        assert_eq!((n.kind, n.call), (kind, None), "{w:?}");
    }
    // 弹着别的框（选项框 · 联网 · 协作 · 目标）时，底下那个提问 / 计划轮不到：先答的是顶上那个框。
    for (w, kind) in [
        (W::Dialog, NeedsKind::Choose),
        (W::Network, NeedsKind::Network),
        (W::Worker, NeedsKind::Worker),
        (W::Goal, NeedsKind::Goal),
    ] {
        assert_eq!(
            needs_of(&ask, Some(&wait(Some(w)))).unwrap().kind,
            kind,
            "{w:?}"
        );
    }
    // 那台没说在等什么（或说了认不出的词）· 也没有提问 / 计划 ⇒ 判不出（不猜成批准）。
    for pending in [bash.clone(), vec![]] {
        let n = needs_of(&pending, Some(&wait(None))).unwrap();
        assert_eq!(
            (n.kind, n.tool, n.call, n.what),
            (NeedsKind::Unknown, None, None, None)
        );
    }
}

/// 快路对拍补上新三格：没结果的调用在时，它的结果 · 你发的一句都得过滤器放行；别人的结果照旧拦。
#[test]
fn the_fast_path_keeps_pending_and_last_say_exact() {
    let recs = vec![
        assistant(vec![
            tool_use("b1", "Bash", json!({"command": "x"})),
            tool_use("b2", "Bash", json!({"command": "y"})),
        ]),
        json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "other", "content": "big"}]}}),
        result("b1"),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "no usage here"}]}}),
        user_text("next"),
        assistant(vec![tool_use("b3", "Bash", json!({"command": "z"}))]),
    ];
    let text = jsonl(&recs);
    let fast = scan_all(&text);
    let mut slow = SessionFacts::default();
    for line in text.lines() {
        slow.end += line.len() as u64 + 1;
        if let Some(v) = crate::observe::record_scan::parse_record(line.as_bytes()) {
            note_record(&mut slow, &v);
        }
    }
    assert_eq!(fast, slow);
    assert_eq!(fast.pending.len(), 1);
    // 正控：有没结果的调用时，别人的工具结果照旧不解析。
    let f = scan_all(&jsonl(&recs[..1]));
    assert!(!could_matter(
        br#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"other"}]}}"#,
        &f
    ));
    assert!(could_matter(
        br#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b2"}]}}"#,
        &f
    ));
}

/// 一条 agent 来话（记录级 `origin`，同 Claude Code 写的那一形）：`handback` ＝ 是不是交回。
fn handback(from: &str, handback: bool) -> Value {
    json!({"type": "user", "isMeta": true,
           "origin": {"kind": "peer", "from": from, "handback": handback, "body": "b"},
           "message": {"content": format!("<agent-message from=\"{from}\">b</agent-message>")}})
}

/// ★ 交回了的子运行：只记交回（途中来话不算）、去重、文件序、有上界；接力扫 == 一次扫完；成品能原样回传。
/// 同一个子运行的收场通知以交回为准 —— 界面按这一格收起那条通知（通知本身不进这一格）。
#[test]
fn handed_back_runs_are_the_ones_that_handed_back() {
    let recs = vec![
        handback("a1", false),
        json!({"type": "user", "origin": {"kind": "task-notification"},
               "message": {"content": "<task-notification><task-id>a1</task-id><status>completed</status></task-notification>"}}),
        handback("a2", true),
        handback("a1", true),
        handback("a2", true),
        // 没有记录级字段、只有框的那一形：认不出是交回 ⇒ 不记。
        json!({"type": "user", "message": {"content": "<agent-message from=\"a3\">b</agent-message>"}}),
    ];
    let text = jsonl(&recs);
    let whole = scan_all(&text);
    assert_eq!(whole.handed_back, ["a2", "a1"]);
    let cut = jsonl(&recs[..3]).len();
    let head = scan_all(&text[..cut]);
    assert_eq!(head.handed_back, ["a2"]);
    let prior =
        prior_from(&serde_json::to_value(&head).unwrap()).expect("带着交回那一格的成品能原样回传");
    assert_eq!(
        scan_facts(&text.as_bytes()[cut..], prior, &Vec::new(), None).unwrap(),
        whole
    );

    let many: Vec<Value> = (0..HANDED_BACK_KEEP + 3)
        .map(|i| handback(&format!("r{i}"), true))
        .collect();
    let f = scan_all(&jsonl(&many));
    assert_eq!(f.handed_back.len(), HANDED_BACK_KEEP);
    assert_eq!(f.handed_back[0], "r3", "超了丢最早的");
}

/// 在等你的那一份（带 `needs.call`）也能原样回传当续传令牌（之前 `prior.needs` 的键集合少了 `call`，等你时每次都得从头扫）。
#[test]
fn a_product_that_is_waiting_on_you_round_trips_as_prior() {
    let f = SessionFacts {
        needs: Some(Needs {
            kind: NeedsKind::Approve,
            tool: Some("Bash".into()),
            call: Some("b1".into()),
            what: None,
            since_ms: Some(1),
            text: crate::common::cells::Words(copy_core::copy_text("beSession.needs.approve", &[])),
            tone: crate::common::cells::Tone::Need,
        }),
        ..SessionFacts::default()
    };
    assert_eq!(prior_from(&serde_json::to_value(&f).unwrap()), Ok(f));
}

/// ★ 一步还没结果时的样子只在这里判：那台说在等的正是它 ⇒ 在等你；有活进程 ⇒ 在跑；没有 ⇒ 状态不明、带原因码
/// （没有活进程持着 `noWriter` · 这一家不留 pidfile、判不了活 `untracked`）；每次现判、不随续传令牌留下来。
#[test]
fn a_step_without_a_result_is_running_only_when_a_live_process_holds_the_session() {
    let text = jsonl(&[assistant(vec![
        tool_use("s1", "Bash", json!({"command": "make"})),
        tool_use("s2", "Bash", json!({"command": "rm -rf build/"})),
    ])]);
    let states = |f: &SessionFacts| {
        f.pending
            .iter()
            .map(|p| (p.state, p.why))
            .collect::<Vec<_>>()
    };
    let unclear = |w| (StepWait::Unclear, Some(w));
    let mut f = scan_all(&text);
    settle_pending(&mut f, true);
    assert_eq!(
        states(&f),
        [unclear(UnclearWhy::NoWriter); 2],
        "没有活进程 ⇒ 状态不明（不当在跑）"
    );
    settle_pending(&mut f, false);
    assert_eq!(
        states(&f),
        [unclear(UnclearWhy::Untracked); 2],
        "这一家不留 pidfile ⇒ 判不了活"
    );
    f.writers = vec![4711];
    settle_pending(&mut f, true);
    assert_eq!(states(&f), [(StepWait::Running, None); 2]);
    f.needs = needs_of(
        &f.pending,
        Some(&PidWait {
            waiting_for: Some(crate::agents::WaitOn::Permission),
            since_ms: None,
        }),
    );
    assert_eq!(f.needs.as_ref().and_then(|n| n.call.as_deref()), Some("s1"));
    settle_pending(&mut f, true);
    assert_eq!(
        states(&f),
        [(StepWait::Awaiting, None), (StepWait::Running, None)]
    );
    // 续传：令牌里带着上一次的样子也收（形状恰好）。
    let wire = serde_json::to_value(&f).unwrap();
    assert_eq!(
        (
            wire["pending"][0]["state"].clone(),
            wire["pending"][0]["why"].clone()
        ),
        (json!("awaiting"), Value::Null)
    );
    let back = prior_from(&wire).expect("带着 state 的成品能原样回传");
    assert_eq!(back.pending[0].state, StepWait::Awaiting);
    f.writers.clear();
    f.needs = None;
    settle_pending(&mut f, true);
    assert_eq!(
        serde_json::to_value(&f).unwrap()["pending"][1]["why"],
        "noWriter"
    );
}

/// ★ 一串重试的结局按首条给（界面按卡上的 id 读）：后面来了正常回复 ⇒ 接上了 · 来了报错那条 ⇒ 没接上 ·
/// 你又说了一句 / 打断 ⇒ 中断了 · 还没下文 ⇒ 还在重试。中间夹的别的系统记录不算下文；接力扫 == 一次扫完。
#[test]
fn a_retry_run_is_settled_by_what_follows_it_and_keyed_by_its_first_record() {
    let api_error = json!({"type": "assistant", "uuid": "e1", "isApiErrorMessage": true, "message": {"content": []}});
    let recs = vec![
        retry("a1", 1),
        json!({"type": "system", "subtype": "local_command", "uuid": "x"}),
        retry("a2", 2),
        json!({"type": "assistant", "uuid": "ok", "message": {"content": []}}),
        retry("b1", 1),
        retry("b2", 2),
        api_error,
        retry("c1", 1),
        json!({"type": "user", "isMeta": true, "message": {"content": "meta"}}),
        user_text("[Request interrupted by user]"),
        retry("d1", 1),
    ];
    let text = jsonl(&recs);
    let whole = scan_all(&text);
    let got: Vec<(&str, RetryOutcome)> = whole
        .retries
        .iter()
        .map(|r| (r.id.as_str(), r.outcome))
        .collect();
    assert_eq!(
        got,
        [
            ("a1", RetryOutcome::Recovered),
            ("b1", RetryOutcome::Failed),
            ("c1", RetryOutcome::Interrupted),
            ("d1", RetryOutcome::Retrying),
        ]
    );
    let wire = serde_json::to_value(&whole).unwrap();
    assert_eq!(
        wire["retries"][0],
        json!({"id": "a1", "outcome": "recovered"})
    );
    let mut cuts = vec![0usize];
    cuts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    for cut in cuts {
        let prior = prior_from(&serde_json::to_value(scan_all(&text[..cut])).unwrap())
            .expect("带着重试那一格的成品能原样回传");
        assert_eq!(
            scan_facts(&text.as_bytes()[cut..], prior, &Vec::new(), None).unwrap(),
            whole,
            "在字节 {cut} 处接力"
        );
    }
    let many: Vec<Value> = (0..RETRY_KEEP + 3)
        .flat_map(|i| {
            [
                retry(&format!("r{i}"), 1),
                json!({"type": "assistant", "message": {"content": []}}),
            ]
        })
        .collect();
    let f = scan_all(&jsonl(&many));
    assert_eq!(
        (f.retries.len(), f.retries[0].id.as_str()),
        (RETRY_KEEP, "r3"),
        "超了丢最早的"
    );
}

fn priced(req: &str, model: &str, usage: Value) -> Value {
    json!({"type": "assistant", "requestId": req, "timestamp": "t-a",
        "message": {"model": model, "usage": usage, "content": [{"type": "text", "text": "x"}]}})
}

/// 用量成品：同一次请求写出的几条只算一次（取最后一条）· 写缓存分 5m / 1h 两档（没分档的整份算 5m）· 成品串后端写好；
/// 续传接力（从任一行边界接着扫）== 一次扫完。
#[test]
fn tokens_count_each_request_once_with_two_cache_write_tiers() {
    let text = jsonl(&[
        priced(
            "r1",
            "m-x",
            json!({"input_tokens": 10, "output_tokens": 5,
            "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 300,
            "cache_creation": {"ephemeral_5m_input_tokens": 100, "ephemeral_1h_input_tokens": 200}}),
        ),
        priced(
            "r1",
            "m-x",
            json!({"input_tokens": 10, "output_tokens": 50,
            "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 300,
            "cache_creation": {"ephemeral_5m_input_tokens": 100, "ephemeral_1h_input_tokens": 200}}),
        ),
        priced(
            "r2",
            "m-x",
            json!({"input_tokens": 0, "output_tokens": 0,
            "cache_read_input_tokens": 0, "cache_creation_input_tokens": 1000}),
        ),
        priced("r3", "m-y", json!({"input_tokens": 7, "output_tokens": 7})),
    ]);
    let f = scan_all(&text);
    let t = f.tokens.clone().expect("tokens");
    assert_eq!(
        (
            t.input,
            t.output,
            t.cache_read,
            t.cache_write5m,
            t.cache_write1h,
            t.requests
        ),
        (17, 57, 1000, 1100, 200, 3)
    );
    assert_eq!(
        t.text.0,
        copy_core::copy_text(
            "beSpend.tokens.line",
            &[
                ("input", "17"),
                ("output", "57"),
                ("read", "1.0k"),
                ("write", "1.3k")
            ]
        )
    );
    assert_eq!(f.cost, None, "记录里没有花费那一条 ⇒ 不给花费");
    let cut = text.find('\n').unwrap() + 1;
    let first = scan_facts(
        &text.as_bytes()[..cut],
        SessionFacts::default(),
        &Vec::new(),
        None,
    )
    .unwrap();
    let prior = prior_from(&serde_json::to_value(&first).unwrap()).unwrap();
    let resumed = scan_facts(&text.as_bytes()[cut..], prior, &Vec::new(), None).unwrap();
    assert_eq!(resumed.tokens, f.tokens);
}

/// 花费照记录里那一家自己记的（最后一条为准），不按定价自己算；有定不了价的型号 ⇒ 串里带「约」；不到一分 ⇒ 另一句。
#[test]
fn cost_is_the_last_cost_record_as_written() {
    let cost = |usd: f64, unknown: bool| json!({"type": "cost-state", "totalCostUSD": usd, "modelUsage": {}, "hasUnknownModelCost": unknown});
    let f = scan_all(&jsonl(&[cost(0.5, true), cost(1.234, false)]));
    assert_eq!(
        f.cost,
        Some(Cost {
            micros: 1_234_000,
            partial: false,
            text: crate::common::cells::Words(copy_core::copy_text(
                "beSpend.cost.exact",
                &[("usd", "1.23")]
            ))
        })
    );
    let f = scan_all(&jsonl(&[cost(2.0, true)]));
    assert_eq!(
        f.cost.as_ref().map(|c| c.text.0.clone()),
        Some(copy_core::copy_text(
            "beSpend.cost.about",
            &[("usd", "2.00")]
        ))
    );
    let f = scan_all(&jsonl(&[cost(0.001, false)]));
    assert_eq!(
        f.cost.map(|c| c.text.0),
        Some(copy_core::copy_text("beSpend.cost.tiny", &[]))
    );
}

/// 许可档：最后一条许可档记录说的那一档（会话事实；记录流里不显示）。
#[test]
fn the_permission_mode_is_the_last_one_written() {
    let text = jsonl(&[
        json!({"type": "permission-mode", "permissionMode": "default", "sessionId": "s"}),
        json!({"type": "user", "message": {"content": "q"}}),
        json!({"type": "permission-mode", "permissionMode": "acceptEdits", "sessionId": "s"}),
    ]);
    assert_eq!(
        scan_all(&text).permission_mode.as_deref(),
        Some("acceptEdits")
    );
    assert_eq!(
        scan_all(&jsonl(&[
            json!({"type": "user", "message": {"content": "q"}})
        ]))
        .permission_mode,
        None
    );
}

/// 许可档照原值交、不翻译不收窄：六个真值与别名 `manual` 各走一遍。
#[test]
fn every_permission_mode_value_passes_through_as_written() {
    for m in [
        "default",
        "plan",
        "acceptEdits",
        "bypassPermissions",
        "dontAsk",
        "auto",
        "manual",
    ] {
        let text =
            jsonl(&[json!({"type": "permission-mode", "permissionMode": m, "sessionId": "s"})]);
        assert_eq!(scan_all(&text).permission_mode.as_deref(), Some(m));
    }
}

/// 一条「延后加载的工具变了」附件（Claude Code 写的那一形；名字与原因都是占位）。只放给了的那几格。
fn mcp_delta(fields: Value) -> Value {
    let mut a = json!({"type": "deferred_tools_delta", "addedNames": [], "addedLines": [], "removedNames": [], "wireHiddenNames": [], "readdedNames": []});
    for (k, v) in fields.as_object().unwrap() {
        a[k] = v.clone();
    }
    json!({"type": "attachment", "attachment": a})
}

/// 这个会话的 MCP 有毛病的那几个：那一家最后一次说的为准，一格一格地换（这一条没写的那一格沿用上一条）；
/// 写了空表 ⇒ 那一种清空。连不上的带原话。没列的不等于连上了 ⇒ 不出。
/// 要求：用户 10-09 定「乙：从会话记录的 deferred_tools_delta 读出这个会话的 MCP 状态，failed 带原因，作为会话事实里的一格」。
#[test]
fn the_sessions_mcp_trouble_is_the_last_thing_said_per_list() {
    use crate::agents::McpStatus;
    let recs = vec![
        mcp_delta(
            json!({"pendingMcpServers": ["m-p"], "failedMcpServers": [{"name": "m-f", "error": "e-1"}]}),
        ),
        mcp_delta(json!({"pendingMcpServers": [], "needsAuthMcpServers": ["m-n2", "m-n1"]})),
        assistant(vec![]),
    ];
    let got = scan_all(&jsonl(&recs)).mcp;
    let t = |n: &str, s, d: Option<&str>| McpTrouble {
        name: n.into(),
        status: s,
        detail: d.map(str::to_string),
    };
    assert_eq!(
        got,
        vec![
            t("m-f", McpStatus::Failed, Some("e-1")),
            t("m-n1", McpStatus::NeedsLogin, None),
            t("m-n2", McpStatus::NeedsLogin, None),
        ]
    );
    let later = [recs, vec![mcp_delta(json!({"failedMcpServers": []}))]].concat();
    assert_eq!(
        scan_all(&jsonl(&later)).mcp,
        vec![
            t("m-n1", McpStatus::NeedsLogin, None),
            t("m-n2", McpStatus::NeedsLogin, None),
        ],
        "清空连不上的那一种，要登录的沿用"
    );
}

/// 〔G2〕「需手动」带写好的字与语气：八种各一句（等批准 / 等回答 / 等批准（计划）/ 等放行 / 等批准（协作请求）/ 等确认 / 等选择 / 需手动）；语气恒 `need`。出口照抄。
#[test]
fn needs_carries_its_words_and_tone() {
    use crate::agents::WaitOn as W;
    let call = |name: &str| PendingCall {
        id: "c".into(),
        name: name.into(),
        what: None,
        at: None,
        state: StepWait::Running,
        why: None,
    };
    let wait = |w: Option<W>| PidWait {
        waiting_for: w,
        since_ms: None,
    };
    let cases = [
        (
            vec![call("Bash")],
            Some(W::Permission),
            NeedsKind::Approve,
            "beSession.needs.approve",
        ),
        (
            vec![call("AskUserQuestion")],
            None,
            NeedsKind::Answer,
            "beSession.needs.answer",
        ),
        (
            vec![call("ExitPlanMode")],
            None,
            NeedsKind::Plan,
            "beSession.needs.plan",
        ),
        (
            vec![call("Bash")],
            Some(W::Network),
            NeedsKind::Network,
            "beSession.needs.network",
        ),
        (
            vec![],
            Some(W::Worker),
            NeedsKind::Worker,
            "beSession.needs.worker",
        ),
        (
            vec![],
            Some(W::Goal),
            NeedsKind::Goal,
            "beSession.needs.goal",
        ),
        (
            vec![],
            Some(W::Dialog),
            NeedsKind::Choose,
            "beSession.needs.choose",
        ),
        (vec![], None, NeedsKind::Unknown, "beSession.needs.unknown"),
    ];
    for (pending, w, kind, key) in cases {
        let n = needs_of(&pending, Some(&wait(w))).unwrap();
        assert_eq!(n.kind, kind);
        let v = serde_json::to_value(&n).unwrap();
        assert_eq!(v["text"], copy_core::copy_text(key, &[]), "{kind:?}");
        assert_eq!(v["tone"], "need", "{kind:?}");
    }
}
