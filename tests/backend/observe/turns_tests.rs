//! 一轮的摘要（`turns.rs`）：子运行的记录不算进主线的轮 · 从某一轮的 `at` 接着取只出那一轮起的 · 你那句与回复头的截法（只取正文行：代码块整块不算，只有代码 ⇒「仅代码」）。
//! 过程行那一行（各样计数 · 结尾是哪几条 · 整行的字与语气 · 右端那一截 · 正在跑 / 在等你时那一截）。
//! 夹具只造结构（占位词），不采会话正文。成品的整形由跨语言金样管（`read_face_tests.rs`）。

use super::*;

fn scan(body: &str, from: u64) -> Vec<TurnRow> {
    let mut out = Vec::new();
    let r = std::io::Cursor::new(body.as_bytes()[from as usize..].to_vec());
    scan_turns(r, from, |t| {
        out.push(t.clone());
        Ok(())
    })
    .unwrap();
    out
}

#[test]
fn subrun_records_do_not_count_and_from_resumes_at_a_turn() {
    let lines = [
        r#"{"type":"user","uuid":"a","timestamp":"t1","message":{"content":"q"}}"#.to_string(),
        r#"{"type":"assistant","uuid":"b","timestamp":"t2","isSidechain":true,"agentId":"ag","message":{"content":[{"type":"tool_use","id":"x","name":"Read","input":{}}]}}"#.to_string(),
        r#"{"type":"assistant","uuid":"c","timestamp":"t3","message":{"content":[{"type":"tool_use","id":"y","name":"Read","input":{}}]}}"#.to_string(),
        format!(r#"{{"type":"user","uuid":"d","timestamp":"t4","message":{{"content":"{}"}}}}"#, "字".repeat(SAID_MAX + 5)),
        r#"{"type":"assistant","uuid":"e","timestamp":"t5","message":{"content":[{"type":"text","text":"x"}]}}"#.to_string(),
    ];
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let all = scan(&body, 0);
    assert_eq!(
        all.iter()
            .map(|t| (t.uuid.as_str(), t.tools, t.done))
            .collect::<Vec<_>>(),
        [("a", 1, true), ("d", 0, false)]
    );
    assert_eq!(
        all[1].said.chars().count(),
        SAID_MAX + 1,
        "截到 50 字 ＋ 省略号"
    );
    assert_eq!(all[1].ending, ["e"]);
    let again = scan(&body, all[1].at);
    assert_eq!(again.len(), 1);
    assert_eq!((again[0].uuid.as_str(), again[0].at), ("d", all[1].at));
}

#[test]
fn reply_head_is_prose_only() {
    // 截图那一形：一句 · 空行 · 整块代码 · 再一句 ⇒ 代码整块不算（围栏与里面的行），空行不算。
    let rec = |text: &str| {
        let lines = [
            r#"{"type":"user","uuid":"a","timestamp":"t1","message":{"content":"q"}}"#.to_string(),
            serde_json::json!({"type":"assistant","uuid":"b","timestamp":"t2","message":{"content":[{"type":"text","text":text}]}}).to_string(),
        ];
        lines.iter().map(|l| format!("{l}\n")).collect::<String>()
    };
    let mixed = rec("改好了。小结：\n\n```python\nclient = X(retries=3)\n```\n\n全量测试通过。\n文档也加了。\n第四行");
    assert_eq!(
        scan(&mixed, 0)[0].reply,
        "改好了。小结：\n全量测试通过。\n文档也加了。"
    );
    // 行内排版记号去掉，只留字。
    let marked = rec("## 小结\n全量测试 **213 passed**。`docs/config.md` 里加了说明。\n> 引用一句");
    assert_eq!(
        scan(&marked, 0)[0].reply,
        "小结\n全量测试 213 passed。docs/config.md 里加了说明。\n引用一句"
    );
    // 只有代码 ⇒ 一个词，不露代码原文。
    let only = rec("```sh\nrm -rf build\n```");
    assert_eq!(
        scan(&only, 0)[0].reply,
        copy_text("rsTurns.reply.codeOnly", &[])
    );
}

#[test]
fn turn_at_counts_your_sentences_up_to_that_record() {
    let lines = [
        r#"{"type":"user","uuid":"u1","timestamp":"t1","message":{"role":"user","content":"第一句"}}"#,
        r#"{"type":"assistant","uuid":"a1","timestamp":"t1b","message":{"role":"assistant","content":[{"type":"text","text":"好"}]}}"#,
        r#"{"type":"user","uuid":"u2","timestamp":"t2","message":{"role":"user","content":"第二句"}}"#,
        r#"{"type":"assistant","uuid":"a2","timestamp":"t2b","message":{"role":"assistant","content":[{"type":"text","text":"行"}]}}"#,
    ]
    .join("\n");
    let at = |u: &str| turn_at(std::io::Cursor::new(lines.as_bytes()), u);
    assert_eq!(at("a1"), Some((1, "t1".to_string())));
    assert_eq!(at("u2"), Some((2, "t2".to_string())));
    assert_eq!(at("a2"), Some((2, "t2".to_string())));
    assert_eq!(at("nope"), None);
}

/// 起止时刻旁边各有一格钟面（这台本地钟 `HH:MM`，界面照抄）：起 ＝ 你那句、止 ＝ 这一轮最后一条主线记录；解不出 ⇒ 空串。
#[test]
fn start_and_end_carry_their_clock_faces() {
    let (a, b) = ("2026-10-07T08:01:00.000Z", "2026-10-07T09:42:30.000Z");
    let lines = [
        format!(r#"{{"type":"user","uuid":"a","timestamp":"{a}","message":{{"content":"q"}}}}"#),
        format!(
            r#"{{"type":"assistant","uuid":"b","timestamp":"{b}","message":{{"content":[{{"type":"text","text":"x"}}]}}}}"#
        ),
        r#"{"type":"user","uuid":"c","timestamp":"later","message":{"content":"q2"}}"#.to_string(),
    ];
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let all = scan(&body, 0);
    let face = |t: &str| crate::common::time::iso_hm_here(t).unwrap();
    assert_eq!(
        (all[0].start_text.as_str(), all[0].end_text.as_str()),
        (face(a).as_str(), face(b).as_str())
    );
    assert_eq!(
        (all[1].start_text.as_str(), all[1].end_text.as_str()),
        ("", ""),
        "解不出 ⇒ 空串"
    );
}

fn body_of(lines: &[serde_json::Value]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn user(uuid: &str, ts: &str, text: &str) -> serde_json::Value {
    serde_json::json!({"type":"user","uuid":uuid,"timestamp":ts,"message":{"content":text}})
}

fn said(uuid: &str, ts: &str, blocks: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type":"assistant","uuid":uuid,"timestamp":ts,"message":{"content":blocks}})
}

fn call(id: &str, name: &str, input: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type":"tool_use","id":id,"name":name,"input":input})
}

fn result(uuid: &str, id: &str, error: bool) -> serde_json::Value {
    serde_json::json!({"type":"user","uuid":uuid,"timestamp":"2026-10-07T08:00:30Z","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"r","is_error":error}]}})
}

fn texts(row: &TurnRow) -> Vec<(String, Tone)> {
    row.parts.iter().map(|p| (p.text.clone(), p.tone)).collect()
}

fn n(key: &str, v: u32) -> String {
    copy_text(key, &[("n", &v.to_string())])
}

/// 派 agent 不算工具；后台任务通知数一条算一条，交回过的那个子 agent 的收场通知不重复数；重试数条数；另一会话来话单数；
/// 失败 ＝ 工具失败 ＋ 后台任务失败。行上的字后端写好：有几样写几样，失败那一段是失败语气。
#[test]
fn counts_split_agents_background_retries_and_total_fails() {
    let t0 = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t0, "q"),
        said(
            "a1",
            "2026-10-07T08:00:01Z",
            serde_json::json!([{"type":"thinking","thinking":"x"},{"type":"text","text":"先看看"},call("t1","Read",serde_json::json!({"file_path":"/p/a"}))]),
        ),
        result("r1", "t1", true),
        said(
            "a2",
            "2026-10-07T08:00:02Z",
            serde_json::json!([call(
                "t2",
                "Task",
                serde_json::json!({"description":"查一下","prompt":"p"})
            )]),
        ),
        said(
            "a3",
            "2026-10-07T08:00:03Z",
            serde_json::json!([call(
                "t3",
                "Task",
                serde_json::json!({"description":"再查","prompt":"p"})
            )]),
        ),
        result("r2", "t2", false),
        result("r3", "t3", false),
        serde_json::json!({"type":"user","uuid":"h1","timestamp":t0,"origin":{"kind":"peer","from":"ag1","handback":true,"body":"b"},"message":{"content":"<agent-message from=\"ag1\">b</agent-message>"}}),
        serde_json::json!({"type":"user","uuid":"n1","timestamp":t0,"origin":{"kind":"task-notification"},"message":{"content":"<task-notification><task-id>ag1</task-id><status>completed</status></task-notification>"}}),
        serde_json::json!({"type":"user","uuid":"n2","timestamp":t0,"origin":{"kind":"task-notification"},"message":{"content":"<task-notification><task-id>bg1</task-id><status>completed</status></task-notification>"}}),
        serde_json::json!({"type":"user","uuid":"n3","timestamp":t0,"origin":{"kind":"task-notification"},"message":{"content":"<task-notification><task-id>bg2</task-id><status>failed</status></task-notification>"}}),
        serde_json::json!({"type":"user","uuid":"p1","timestamp":t0,"origin":{"kind":"peer","from":"s2"},"message":{"content":"<cross-session-message from=\"s2\">hi</cross-session-message>"}}),
        serde_json::json!({"type":"system","uuid":"e1","subtype":"api_error","timestamp":t0}),
        serde_json::json!({"type":"system","uuid":"e2","subtype":"api_error","timestamp":t0}),
        said(
            "a4",
            "2026-10-07T08:05:04Z",
            serde_json::json!([{"type":"text","text":"好了"}]),
        ),
    ];
    let row = scan(&body_of(&lines), 0).remove(0);
    assert_eq!(
        (
            row.tools,
            row.agents,
            row.thinking,
            row.background,
            row.retries,
            row.peers,
            row.fails
        ),
        (1, 2, 1, 2, 2, 1, 2)
    );
    assert_eq!(row.ending, ["a4"]);
    assert_eq!(
        texts(&row),
        [
            (copy_text("rsTurns.proc.head", &[]), Tone::Plain),
            (n("rsTurns.proc.tools", 1), Tone::Plain),
            (n("rsTurns.proc.thinking", 1), Tone::Plain),
            (n("rsTurns.proc.agents", 2), Tone::Plain),
            (n("rsTurns.proc.background", 2), Tone::Plain),
            (n("rsTurns.proc.retries", 2), Tone::Plain),
            (n("rsTurns.proc.peers", 1), Tone::Plain),
            (n("rsTurns.proc.fails", 2), Tone::Fail),
        ]
    );
}

/// 右端那一截：起止钟面 ＋ 用时（用时由界面按 `from`/`to` 填进 `{dur}`）；不满一秒不写用时；起止同一个钟面只写一个。
#[test]
fn span_is_written_with_a_duration_slot() {
    let (a, b) = ("2026-10-07T08:00:00Z", "2026-10-07T08:05:04Z");
    let lines = [
        user("u1", a, "q"),
        said(
            "a1",
            b,
            serde_json::json!([call("t1", "Read", serde_json::json!({}))]),
        ),
        user("u2", b, "q2"),
        said(
            "a2",
            b,
            serde_json::json!([call("t2", "Read", serde_json::json!({}))]),
        ),
    ];
    let rows = scan(&body_of(&lines), 0);
    let face = |t: &str| crate::common::time::iso_hm_here(t).unwrap();
    let ms = |t: &str| crate::common::time::parse_iso8601_ms(t).unwrap();
    let range = copy_text(
        "rsTurns.span.range",
        &[("from", &face(a)), ("to", &face(b))],
    );
    assert_eq!(
        rows[0].span,
        Span {
            text: copy_text("rsTurns.span.done", &[("when", &range), ("dur", "{dur}")]),
            from: Some(ms(a)),
            to: Some(ms(b)),
        }
    );
    assert_eq!(
        rows[1].span,
        Span {
            text: face(b),
            from: None,
            to: None
        },
        "不满一秒、同一个钟面"
    );
}

/// 结尾：有结论正文 ⇒ 那几条；没有、是被你打断 / 报错停下的 ⇒ 那一条中断标记 / 报错卡（中断了这一轮就算收尾）。
#[test]
fn ending_falls_back_to_the_interrupt_or_error_card() {
    let t = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t, "q"),
        said(
            "a1",
            t,
            serde_json::json!([{"type":"text","text":"先读"},call("t1","Read",serde_json::json!({}))]),
        ),
        result("r1", "t1", false),
        user("x1", t, "[Request interrupted by user]"),
    ];
    let row = scan(&body_of(&lines), 0).remove(0);
    assert_eq!(
        (row.ending.clone(), row.done),
        (vec!["x1".to_string()], true)
    );

    let lines = [
        user("u1", t, "q"),
        said(
            "a1",
            t,
            serde_json::json!([call("t1", "Read", serde_json::json!({}))]),
        ),
        result("r1", "t1", false),
        serde_json::json!({"type":"assistant","uuid":"z1","timestamp":t,"isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"API Error"}]}}),
    ];
    let row = scan(&body_of(&lines), 0).remove(0);
    assert_eq!(row.ending, ["z1"]);
}

/// 不出行：过程是空的（只有结论）· 过程只有一条压缩摘要（`/compact` 那一段，不套一层空壳）。段中自动压缩跟着别的一起进折叠。
#[test]
fn no_line_for_an_empty_process_or_a_lone_compact_summary() {
    let t = "2026-10-07T08:00:00Z";
    let compact = serde_json::json!({"type":"user","uuid":"c1","timestamp":t,"isCompactSummary":true,"message":{"content":"compact summary"}});
    let rows = scan(
        &body_of(&[
            user("u1", t, "q"),
            said("a1", t, serde_json::json!([{"type":"text","text":"好"}])),
            user("u2", t, "/compact"),
            compact.clone(),
            user("u3", t, "q3"),
            said(
                "a3",
                t,
                serde_json::json!([call("t1", "Read", serde_json::json!({}))]),
            ),
            compact,
        ]),
        0,
    );
    assert!(rows[0].parts.is_empty(), "只有结论");
    assert!(rows[1].parts.is_empty(), "只有一条压缩摘要");
    assert_eq!(texts(&rows[2]).len(), 2, "过程 · 工具 ×1（压缩不另计数）");
}

/// 正在跑的那一轮（后端从这台的会话事实拼进来）：在等批准 ⇒ 「等批准：那一步」需手动语气，右端「已等 {dur}」；
/// 在跑 ⇒ 「现在：那一步」，右端「HH:MM 起 · {dur}」；没有活进程 ⇒ 不写「现在」。收尾了的轮不动。
#[test]
fn live_turn_says_what_runs_now_or_what_waits_for_you() {
    use crate::observe::facts_query::{Needs, NeedsKind};
    let t = "2026-10-07T08:16:00Z";
    let lines = [
        user("u1", t, "q"),
        said(
            "a1",
            t,
            serde_json::json!([call(
                "t1",
                "Grep",
                serde_json::json!({"pattern":"from .a import"})
            )]),
        ),
    ];
    let base = scan(&body_of(&lines), 0).remove(0);
    assert!(!base.done);

    let mut running = base.clone();
    dress_live(&mut running, None, true);
    assert_eq!(running.phase, Phase::Running);
    assert_eq!(
        running.parts.last().map(|p| (p.text.clone(), p.tone)),
        Some((
            copy_text("rsTurns.proc.now", &[("step", "Grep from .a import")]),
            Tone::Now
        ))
    );
    let face = crate::common::time::iso_hm_here(t).unwrap();
    assert_eq!(
        running.span.text,
        copy_text("rsTurns.span.since", &[("hm", &face), ("dur", "{dur}")])
    );
    assert_eq!(
        (running.span.from, running.span.to),
        (crate::common::time::parse_iso8601_ms(t), None)
    );

    let mut awaiting = base.clone();
    let needs = Needs {
        kind: NeedsKind::Approve,
        tool: Some("Grep".into()),
        call: Some("t1".into()),
        what: Some("from .a import".into()),
        since_ms: Some(1234),
        text: crate::common::cells::Words(copy_core::copy_text("beSession.needs.approve", &[])),
        tone: crate::common::cells::Tone::Need,
    };
    dress_live(&mut awaiting, Some(&needs), true);
    assert_eq!(awaiting.phase, Phase::Awaiting);
    assert_eq!(
        awaiting.parts.last().map(|p| (p.text.clone(), p.tone)),
        Some((
            copy_text(
                "rsTurns.proc.awaitApprove",
                &[("step", "Grep from .a import")]
            ),
            Tone::Need
        ))
    );
    assert_eq!(
        awaiting.span,
        Span {
            text: copy_text("rsTurns.span.waited", &[("dur", "{dur}")]),
            from: Some(1234),
            to: None
        }
    );

    let mut idle = base.clone();
    dress_live(&mut idle, None, false);
    assert_eq!(
        (idle.phase, idle.parts.clone()),
        (Phase::Idle, base.parts.clone())
    );

    let mut done = scan(
        &body_of(&[
            user("u1", t, "q"),
            said(
                "a1",
                t,
                serde_json::json!([call("t1", "Read", serde_json::json!({}))]),
            ),
            user("u2", t, "q2"),
        ]),
        0,
    )
    .remove(0);
    let before = done.clone();
    dress_live(&mut done, Some(&needs), true);
    assert_eq!(done, before, "收尾了的轮不动");
}
