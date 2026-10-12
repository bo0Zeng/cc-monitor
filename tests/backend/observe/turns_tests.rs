//! 一轮的摘要（`turns.rs`）：子运行的记录不算进主线的轮 · 从某一轮的 `at` 接着取只出那一轮起的 · 你那句与回复头的截法（只取正文行：代码块整块不算，只有代码 ⇒「仅代码」）。
//! 划段（露着的是哪几条 · 「工具 · 正文 · 工具」是两段 · 连续的工具是一段）与每段那一行（各样计数 · 字与语气 · 右端那一截 · 正在跑 / 在等你时那一截）。
//! 夹具只造结构（占位词），不采会话正文。成品的整形由跨语言金样管（`read_face_tests.rs`）。

use super::*;

/// 扫出来的轮按 UTC 写好钟面（同答 `history-turns` 那一下：先 [`TurnRow::stamp`]，再 [`dress_live`]）。
fn scan(body: &str, from: u64) -> Vec<TurnRow> {
    raw_scan(body, from)
        .into_iter()
        .map(|mut t| {
            t.stamp(&crate::Tz::default());
            t
        })
        .collect()
}

/// 扫描那一层交出来的原样（进索引缓存的那一份）。
fn raw_scan(body: &str, from: u64) -> Vec<TurnRow> {
    let mut out = Vec::new();
    let r = std::io::Cursor::new(body.as_bytes()[from as usize..].to_vec());
    scan_turns(r, from, |t| {
        out.push(t.clone());
        Ok(())
    })
    .unwrap();
    out
}

/// 钟面不进扫描那一份（索引缓存里那一份给几个时区不同的看的人共用）：答的那一下按看的那一台的时区写，右端那一截跟着改。
#[test]
fn clock_faces_are_written_at_the_answer_by_the_viewer_zone() {
    let line = |t: &str, ty: &str, u: &str| {
        format!(r#"{{"type":"{ty}","uuid":"{u}","timestamp":"{t}","message":{{"content":"q"}}}}"#)
    };
    let body = format!(
        "{}\n{}\n{}\n",
        line("2026-10-07T20:30:00.000Z", "user", "a"),
        r#"{"type":"assistant","uuid":"b","timestamp":"2026-10-07T20:31:00.000Z","message":{"content":[{"type":"tool_use","id":"x","name":"Read","input":{}}]}}"#,
        line("2026-10-07T20:40:00.000Z", "user", "c"),
    );
    let raw = raw_scan(&body, 0);
    assert!(
        raw[0].start_text.is_empty() && raw[0].end_text.is_empty(),
        "扫描那一层不写钟面"
    );
    let mut sh = raw[0].clone();
    sh.stamp(&crate::Tz::named("Asia/Shanghai").unwrap());
    assert_eq!(
        (sh.start_text.as_str(), sh.end_text.as_str()),
        ("04:30", "04:31")
    );
    assert!(
        sh.folds[0].span.text.contains("04:31"),
        "右端那一截随钟面写：{}",
        sh.folds[0].span.text
    );
    let mut utc = raw[0].clone();
    utc.stamp(&crate::Tz::default());
    assert_eq!(utc.start_text, "20:30");
    assert!(
        utc.folds[0].span.text.contains("20:31"),
        "{}",
        utc.folds[0].span.text
    );
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
    assert_eq!(all[1].shown, ["e"]);
    assert!(all[1].folds.is_empty(), "只有结论 ⇒ 不折");
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
    let face = |t: &str| crate::common::time::iso_hm(t, &crate::Tz::default()).unwrap();
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

fn texts(fold: &Fold) -> Vec<(String, Tone)> {
    fold.parts
        .iter()
        .map(|p| (p.text.clone(), p.tone))
        .collect()
}

fn afters(row: &TurnRow) -> Vec<&str> {
    row.folds.iter().map(|f| f.after.as_str()).collect()
}

fn n(key: &str, v: u32) -> String {
    copy_text(key, &[("n", &v.to_string())])
}

fn head() -> (String, Tone) {
    (copy_text("rsTurns.proc.head", &[]), Tone::Plain)
}

fn tools(v: u32) -> (String, Tone) {
    (n("rsTurns.proc.tools", v), Tone::Plain)
}

fn text(s: &str) -> serde_json::Value {
    serde_json::json!([{"type":"text","text":s}])
}

fn calls(id: &str) -> serde_json::Value {
    serde_json::json!([call(id, "Read", serde_json::json!({}))])
}

/// ★ 只有连续的工具 / agent 段才折，中间说的话露在外面。「工具 · 正文 · 工具 · 正文」⇒ 两段过程，
/// 第一段跟在你那句后面、第二段跟在那段正文后面；两段正文都露着（结尾那段照旧在外面）。
#[test]
fn prose_between_tools_stays_outside_and_splits_the_process() {
    let t = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t, "q"),
        said("a1", t, calls("t1")),
        result("r1", "t1", false),
        said("a2", t, text("中间的话")),
        said("a3", t, calls("t2")),
        result("r2", "t2", true),
        said("a4", t, text("结论")),
    ];
    let row = scan(&body_of(&lines), 0).remove(0);
    assert_eq!(row.shown, ["a2", "a4"], "正文都露着");
    assert_eq!(afters(&row), ["u1", "a2"], "工具 · 正文 · 工具 ⇒ 两段");
    assert_eq!(texts(&row.folds[0]), [head(), tools(1)]);
    assert_eq!(
        texts(&row.folds[1]),
        [head(), tools(1), (n("rsTurns.proc.fails", 1), Tone::Fail)]
    );
    assert_eq!((row.tools, row.fails), (2, 1), "整轮合计照旧");
    assert_eq!(row.reply, "结论", "回复头取最后一次工具调用之后的那段");
}

/// 连续的工具（中间没有正文）照旧收成一段；结尾的总结在外面、后面不再有一段。
#[test]
fn consecutive_tools_fold_into_one_and_the_summary_stays_out() {
    let t = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t, "q"),
        said(
            "a1",
            t,
            serde_json::json!([{"type":"thinking","thinking":"x"}]),
        ),
        said("a2", t, calls("t1")),
        result("r1", "t1", false),
        said("a3", t, calls("t2")),
        result("r2", "t2", false),
        serde_json::json!({"type":"system","uuid":"e1","subtype":"api_error","timestamp":t}),
        said("a4", t, text("总结")),
    ];
    let row = scan(&body_of(&lines), 0).remove(0);
    assert_eq!(row.shown, ["a4"]);
    assert_eq!(afters(&row), ["u1"], "一段");
    assert_eq!(
        texts(&row.folds[0]),
        [
            head(),
            tools(2),
            (n("rsTurns.proc.thinking", 1), Tone::Plain),
            (n("rsTurns.proc.retries", 1), Tone::Plain),
        ]
    );
}

/// 派 agent 不算工具；后台任务通知数一条算一条，交回过的那个子 agent 的收场通知不重复数；重试数条数；另一会话来话单数；
/// 失败 ＝ 工具失败 ＋ 后台任务失败。行上的字后端写好：有几样写几样，失败那一段是失败语气。
#[test]
fn counts_split_agents_background_retries_and_total_fails() {
    let t0 = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t0, "q"),
        said(
            "a0",
            "2026-10-07T08:00:01Z",
            serde_json::json!([{"type":"thinking","thinking":"x"}]),
        ),
        said("a1", "2026-10-07T08:00:01Z", text("先看看")),
        said(
            "a1b",
            "2026-10-07T08:00:01Z",
            serde_json::json!([call("t1", "Read", serde_json::json!({"file_path":"/p/a"}))]),
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
    assert_eq!(row.shown, ["a1", "a4"]);
    assert_eq!(afters(&row), ["u1", "a1"]);
    assert_eq!(
        texts(&row.folds[0]),
        [head(), (n("rsTurns.proc.thinking", 1), Tone::Plain)]
    );
    assert_eq!(
        texts(&row.folds[1]),
        [
            head(),
            tools(1),
            (n("rsTurns.proc.agents", 2), Tone::Plain),
            (n("rsTurns.proc.background", 2), Tone::Plain),
            (n("rsTurns.proc.retries", 2), Tone::Plain),
            (n("rsTurns.proc.peers", 1), Tone::Plain),
            (n("rsTurns.proc.fails", 2), Tone::Fail),
        ]
    );
}

/// 每段右端那一截：这一段第一条到最后一条记录的起止钟面 ＋ 用时（用时由界面按 `from`/`to` 填进 `{dur}`）；
/// 不满一秒不写用时；起止同一个钟面只写一个。
#[test]
fn span_is_written_with_a_duration_slot() {
    let (a, b) = ("2026-10-07T08:00:00Z", "2026-10-07T08:05:04Z");
    let done = serde_json::json!({"type":"user","uuid":"r1","timestamp":b,"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"r"}]}});
    let lines = [
        user("u1", a, "q"),
        said("a1", a, calls("t1")),
        done,
        user("u2", b, "q2"),
        said("a2", b, calls("t2")),
    ];
    let rows = scan(&body_of(&lines), 0);
    let face = |t: &str| crate::common::time::iso_hm(t, &crate::Tz::default()).unwrap();
    let ms = |t: &str| crate::common::time::parse_iso8601_ms(t).unwrap();
    let range = copy_text(
        "rsTurns.span.range",
        &[("from", &face(a)), ("to", &face(b))],
    );
    assert_eq!(
        rows[0].folds[0].span,
        Span {
            text: copy_text("rsTurns.span.done", &[("when", &range), ("dur", "{dur}")]),
            from: Some(ms(a)),
            to: Some(ms(b)),
        }
    );
    assert_eq!(
        rows[1].folds[0].span,
        Span {
            text: face(b),
            from: None,
            to: None
        },
        "不满一秒、同一个钟面"
    );
}

/// 停下这一轮的那一条（中断标记 / 报错卡）露着、把过程隔开；同一条里带正文又调了工具 ⇒ 整条露着（中断了这一轮就算收尾）。
#[test]
fn the_interrupt_or_error_card_stays_outside() {
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
        (row.shown.clone(), row.done),
        (vec!["a1".to_string(), "x1".to_string()], true)
    );
    assert!(
        row.folds.is_empty(),
        "a1 的那次调用在露着的卡里；r1 一条什么都不数 ⇒ 不出行"
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
    assert_eq!(row.shown, ["z1"]);
    assert_eq!(afters(&row), ["u1"]);
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
    assert!(rows[0].folds.is_empty(), "只有结论");
    assert!(rows[1].folds.is_empty(), "只有一条压缩摘要");
    assert_eq!(
        texts(&rows[2].folds[0]).len(),
        2,
        "过程 · 工具 ×1（压缩不另计数）"
    );
}

/// 正在跑的那一轮（后端从这台的会话事实拼进来），写在尾巴那一段上：在等批准 ⇒ 「等批准：那一步」需手动语气，右端「已等 {dur}」；
/// 在跑 ⇒ 「现在：那一步」，右端「HH:MM 起 · {dur}」（那一段的起点）；没有活进程 ⇒ 不写「现在」。收尾了的轮不动；
/// 尾巴是露着的正文（不是一段过程）⇒ 只改此刻，前面那几段不动。
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
    assert_eq!(running.folds[0].phase, Phase::Running);
    assert_eq!(
        running.folds[0]
            .parts
            .last()
            .map(|p| (p.text.clone(), p.tone)),
        Some((
            copy_text("rsTurns.proc.now", &[("step", "Grep from .a import")]),
            Tone::Now
        ))
    );
    let face = crate::common::time::iso_hm(t, &crate::Tz::default()).unwrap();
    assert_eq!(
        running.folds[0].span.text,
        copy_text("rsTurns.span.since", &[("hm", &face), ("dur", "{dur}")])
    );
    assert_eq!(
        (running.folds[0].span.from, running.folds[0].span.to),
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
        head: crate::common::cells::Words(String::new()),
        head_code: None,
        tone: crate::common::cells::Tone::Need,
        rank: 1,
        waited_ms: Some(1),
        waited_text: Some(crate::common::cells::Words(copy_core::short_duration(1))),
        ask: None,
        manual_text: None,
    };
    dress_live(&mut awaiting, Some(&needs), true);
    assert_eq!(awaiting.phase, Phase::Awaiting);
    assert_eq!(
        awaiting.folds[0]
            .parts
            .last()
            .map(|p| (p.text.clone(), p.tone)),
        Some((
            copy_text(
                "rsTurns.proc.awaitApprove",
                &[("step", "Grep from .a import")]
            ),
            Tone::Need
        ))
    );
    assert_eq!(
        awaiting.folds[0].span,
        Span {
            text: copy_text("rsTurns.span.waited", &[("dur", "{dur}")]),
            from: Some(1234),
            to: None
        }
    );

    let mut idle = base.clone();
    dress_live(&mut idle, None, false);
    assert_eq!(
        (idle.phase, idle.folds.clone()),
        (Phase::Idle, base.folds.clone())
    );

    // 尾巴是正文：工具 · 正文，还在跑 ⇒ 只改此刻，那一段不动。
    let mut said_last = scan(
        &body_of(&[
            user("u1", t, "q"),
            said("a1", t, calls("t1")),
            result("r1", "t1", false),
            said("a2", t, text("看完了，接着改")),
        ]),
        0,
    )
    .remove(0);
    let folds = said_last.folds.clone();
    dress_live(&mut said_last, None, true);
    assert_eq!(
        (said_last.phase, said_last.folds.clone()),
        (Phase::Running, folds)
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

/// ★ 排队插话落成了人话：中断之后那句排队的话又作为你的一句落进记录（下一句的原文与它一字不差）⇒ 排队那一条记进这一轮的
/// `landed`（界面不再单独出它，只露人话那一条）；还在排队、人话还没落 ⇒ 不记（插话卡照常露着）；下一句说的不是它 ⇒ 也不记。
#[test]
fn a_queued_line_that_lands_as_your_next_sentence_is_marked_landed() {
    let t = "2026-10-07T08:00:00Z";
    let queued = |s: &str| serde_json::json!({"type":"queue-operation","operation":"remove","timestamp":t,"content":s});
    let head = [
        user("u1", t, "q"),
        said("a1", t, calls("t1")),
        user("x1", t, "[Request interrupted by user]"),
        queued("用 functools"),
    ];
    let body = body_of(&head);
    let at = |k: usize| {
        head[..k]
            .iter()
            .map(|l| l.to_string().len() as u64 + 1)
            .sum::<u64>()
    };
    let qid = crate::agents::line_id(at(3));

    // 人话还没落：插话卡照常露着
    let rows = scan(&body, 0);
    assert!(rows[0].landed.is_empty(), "还在排队");

    // 落了（同一段文字）：记进排队那一条所在的那一轮
    let landed = format!("{body}{}\n", user("u2", t, "用 functools"));
    let rows = scan(&landed, 0);
    assert_eq!(rows[0].landed, [qid.clone()]);
    assert!(rows[1].landed.is_empty());

    // 下一句说的不是它：不记
    let other = format!("{body}{}\n", user("u2", t, "别的"));
    assert!(scan(&other, 0)[0].landed.is_empty());
}

fn untils(row: &TurnRow) -> Vec<Option<&str>> {
    row.folds.iter().map(|f| f.until.as_deref()).collect()
}

/// ★ 每段管到哪一条为止由核心给（`until`：段后面紧跟的那一条）：这一轮下一条露着的 · 下一轮你那句 · 还开着的最后一轮 ⇒ 没有。
/// 界面只按 `after` 与 `until` 在账本里取那几行，不自己找「下一个边界」。从某一轮的 `at` 续取也一样。
#[test]
fn each_fold_says_which_record_ends_it() {
    let t = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t, "q"),
        said("a1", t, calls("t1")),
        result("r1", "t1", false),
        said("a2", t, text("中间的话")),
        said("a3", t, calls("t2")),
        result("r2", "t2", false),
        user("u2", t, "q2"),
        said("a4", t, calls("t3")),
        result("r3", "t3", false),
    ];
    let body = body_of(&lines);
    let rows = scan(&body, 0);
    assert_eq!(
        untils(&rows[0]),
        [Some("a2"), Some("u2")],
        "隔开它的那条露着的 · 下一轮你那句"
    );
    assert_eq!(untils(&rows[1]), [None], "还开着的最后一轮：到账本尾");
    let at = rows[1].at;
    assert_eq!(untils(&scan(&body, at)[0]), [None], "续取同口径");
    let at0 = rows[0].at;
    assert_eq!(untils(&scan(&body, at0)[0]), [Some("a2"), Some("u2")]);
}

/// ★ 主管 10-10：回退掉的那一轮（主线外，`history-turns` 不出它）不被上一轮的最后一段吞掉：上一轮那一段停在被回退那一句之前。
#[test]
fn a_rewound_turn_is_not_swallowed_by_the_fold_before_it() {
    let t = "2026-10-07T08:00:00Z";
    let lines = [
        user("u1", t, "q"),
        said("a1", t, calls("t1")),
        result("r1", "t1", false),
        // 被回退掉的那一轮（之后的 u3 接回 u1 之前的那一处）：扫描那一层照常认它是一轮，冷扫按主线外清单把它删掉。
        user("u2", t, "rewound"),
        said("a2", t, calls("t2")),
        result("r2", "t2", false),
        user("u3", t, "again"),
    ];
    let rows = scan(&body_of(&lines), 0);
    assert_eq!(rows[0].uuid, "u1");
    assert_eq!(
        untils(&rows[0]),
        [Some("u2")],
        "上一轮的那一段停在被回退那一句之前，不管它之后在不在主线上"
    );
}

/// ★ 会话的 pidfile 说「在等批准」（那一步还没结果）时，后来续写进记录的那一轮回复照样露着，不收进过程。
/// 划段只看记录（[`Open::close`]）；「在等」只经 [`dress_live`] 改此刻那一格与尾巴那一段上的字，不动露着的是哪几条。
/// 三种续写：只续一段正文（没收尾）· 结果回来再说结论（`end_turn`）· 又说了一句、新的一轮回了话。
#[test]
fn a_reply_written_after_the_pidfile_says_waiting_stays_outside() {
    use crate::observe::facts_query::{Needs, NeedsKind};
    let t = "2026-10-07T08:16:00Z";
    let needs = Needs {
        kind: NeedsKind::Approve,
        tool: Some("Bash".into()),
        call: Some("b1".into()),
        what: Some("make".into()),
        since_ms: Some(1234),
        text: crate::common::cells::Words(copy_core::copy_text("beSession.needs.approve", &[])),
        head: crate::common::cells::Words(String::new()),
        head_code: None,
        tone: crate::common::cells::Tone::Need,
        rank: 1,
        waited_ms: Some(1),
        waited_text: Some(crate::common::cells::Words(copy_core::short_duration(1))),
        ask: None,
        manual_text: None,
    };
    let head = vec![
        user("u1", t, "q1"),
        said("a1", t, text("第一轮的结论")),
        user("u2", t, "q2"),
        said("a2", t, text("先说一句")),
        said(
            "a3",
            t,
            serde_json::json!([call("b1", "Bash", serde_json::json!({"command":"make"}))]),
        ),
    ];
    let end = |uuid: &str, s: &str| serde_json::json!({"type":"assistant","uuid":uuid,"timestamp":t,"message":{"content":text(s),"stop_reason":"end_turn"}});
    let cases: Vec<(&str, Vec<serde_json::Value>)> = vec![
        ("只续一段正文", vec![said("a4", t, text("续写的回复"))]),
        (
            "结果回来再说结论",
            vec![result("r1", "b1", false), end("a4", "续写的回复")],
        ),
        (
            "新的一轮回了话",
            vec![user("u3", t, "q3"), end("a4", "续写的回复")],
        ),
    ];
    for (name, more) in cases {
        let mut lines = head.clone();
        lines.extend(more);
        let mut rows = scan(&body_of(&lines), 0);
        let last = rows.last_mut().unwrap();
        dress_live(last, Some(&needs), true);
        let row = rows.iter().find(|r| r.shown.iter().any(|u| u == "a4"));
        assert!(row.is_some(), "[{name}] 续写的回复要露着：{rows:?}");
        let row = row.unwrap();
        assert!(
            row.folds.iter().all(|f| f.after != "a4"),
            "[{name}] 回复后面没有别的了，不该再起一段"
        );
    }
}
