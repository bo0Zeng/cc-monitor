//! 一步用时的配对表：调用记下时刻，结果来了配上、写进那一格；配不上 · 结果早于调用 ⇒ 不写；表封顶丢最早的。
//!
//! 夹具只造结构（记录类型、调用 id、时刻），不采任何真会话正文。

use super::*;
use crate::agents::record::{Block, Body, Draw, Record};
use crate::agents::{Speaker, StepResult, UserText};
use std::collections::BTreeMap;

/// 一条回复，带这几次工具调用，时刻 `at_ms`。
fn reply(at_ms: Option<i64>, calls: &[&str]) -> Record {
    Record {
        agent: "claude".into(),
        id: format!("r{}", at_ms.unwrap_or(-1)),
        at: None,
        time_text: None,
        at_ms,
        draw: Draw::Steps,
        human: false,
        body: Body::Reply {
            blocks: calls
                .iter()
                .map(|id| Block::ToolUse {
                    id: id.to_string(),
                    name: "Bash".into(),
                    input: serde_json::json!({}),
                })
                .collect(),
            auto_reply: false,
            cards: BTreeMap::new(),
            steps: BTreeMap::new(),
            runs: BTreeMap::new(),
            error: None,
        },
    }
}

/// 一条带这几次结果的记录（都说不上的那一形：适配层交来的只有数与码，`text` 空），时刻 `at_ms`。
fn results(at_ms: Option<i64>, of: &[&str]) -> Record {
    with(
        at_ms,
        of,
        StepResult {
            ok: true,
            ..Default::default()
        },
    )
}

/// 同上，每一格都是适配层交来的 `plain`。
fn with(at_ms: Option<i64>, of: &[&str], plain: StepResult) -> Record {
    Record {
        agent: "claude".into(),
        id: format!("s{}", at_ms.unwrap_or(-1)),
        at: None,
        time_text: None,
        at_ms,
        draw: Draw::Steps,
        human: false,
        body: Body::Said {
            who: UserText {
                speaker: Speaker::ToolResult,
                text: String::new(),
                parts: Vec::new(),
            },
            blocks: of
                .iter()
                .map(|id| {
                    Block::tool_result(id.to_string(), Vec::new(), false, copy_core::size_text)
                })
                .collect(),
            results: of
                .iter()
                .map(|id| (id.to_string(), plain.clone()))
                .collect(),
            cwd: None,
            attachments: Vec::new(),
        },
    }
}

/// 那一格的 `text`。
fn cell(r: &Record, id: &str) -> String {
    match &r.body {
        Body::Said { results, .. } => results[id].text.0.clone(),
        _ => panic!("不是带结果的那一条"),
    }
}

const T: i64 = 1_700_000_000_000;

/// ★ 调用 `c1` 在 t，隔 90 秒来了它的结果 ⇒ 那一格写好用时。
#[test]
fn a_result_ninety_seconds_after_its_call_gets_its_duration_written() {
    let mut clock = StepClock::default();
    clock.note(&reply(Some(T), &["c1"]));
    let mut r = results(Some(T + 90_000), &["c1"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c1"), copy_core::short_duration(90_000));
    assert_eq!(clock.len(), 0, "配上了就从表里摘掉");
}

/// ★ 结果的时刻早于调用（时钟回拨）· 没有记下的调用 · 结果没有时刻 ⇒ 当配不上，那一格不动。
#[test]
fn a_result_earlier_than_its_call_or_unmatched_is_left_unwritten() {
    let untouched = cell(&results(None, &["c1"]), "c1");
    assert_eq!(untouched, String::new());

    let mut clock = StepClock::default();
    clock.note(&reply(Some(T), &["c1"]));
    let mut r = results(Some(T - 1), &["c1"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c1"), untouched, "结果早于调用：不猜");

    let mut r = results(Some(T + 5_000), &["c9"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c9"), untouched, "没记下的调用：不写");

    let mut clock = StepClock::default();
    clock.note(&reply(Some(T), &["c2"]));
    let mut r = results(None, &["c2"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c2"), untouched, "结果没有时刻：不写");
}

/// ★ 表封顶（同会话事实里没结果的调用那个数），满了丢最早的那一条。
#[test]
fn a_full_table_drops_its_oldest_call() {
    let mut clock = StepClock::default();
    let ids: Vec<String> = (0..=PENDING_KEEP).map(|i| format!("c{i}")).collect();
    for (i, id) in ids.iter().enumerate() {
        clock.note(&reply(Some(T + i as i64), &[id.as_str()]));
    }
    assert_eq!(clock.len(), PENDING_KEEP);

    let mut r = results(Some(T + 60_000), &["c0", "c1"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c0"), String::new(), "最早那一条已丢");
    assert_eq!(cell(&r, "c1"), copy_core::short_duration(59_999));
}

/// 没时刻的调用不进表（配上了也算不出用时）；同一 id 又记一次 ⇒ 照后一次的时刻。
#[test]
fn a_call_without_a_time_is_not_noted_and_a_repeat_takes_the_later_time() {
    let mut clock = StepClock::default();
    clock.note(&reply(None, &["c1"]));
    assert_eq!(clock.len(), 0);
    clock.note(&reply(Some(T), &["c2"]));
    clock.note(&reply(Some(T + 30_000), &["c2"]));
    assert_eq!(clock.len(), 1);
    let mut r = results(Some(T + 90_000), &["c2"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c2"), copy_core::short_duration(60_000));
}

/// ★ codex 那一家走同一张表：`function_call` 在 t、`function_call_output` 隔 90 秒 ⇒ 结果那一条的 `results.<call_id>` 在，
/// 配完 `text` 是写好的用时（经 `record_of` 端到端，时刻取信封上的 `timestamp`）。
#[test]
fn a_codex_call_output_ninety_seconds_later_gets_its_duration_written() {
    use crate::agents::codex::record::record_of;
    let call = serde_json::json!({"timestamp": "2026-10-07T20:30:00.000Z", "type": "response_item",
        "payload": {"type": "function_call", "call_id": "c1", "name": "shell", "arguments": "{}"}});
    let out = serde_json::json!({"timestamp": "2026-10-07T20:31:30.000Z", "type": "response_item",
        "payload": {"type": "function_call_output", "call_id": "c1", "output": "x"}});
    let mut clock = StepClock::default();
    clock.note(&record_of(&call, 0).unwrap());
    let mut r = record_of(&out, 100).unwrap();
    clock.time(&mut r);
    let Body::Said { results, .. } = &r.body else {
        panic!("不是带结果的那一条")
    };
    let c = results.get("c1").expect("codex 的结果也进 results");
    assert!(c.ok);
    assert_eq!(c.text.0, copy_core::short_duration(90_000));
}

/// ★ 适配层只交数与码（`text` 空）：失败的那一步配上了用时 ⇒「失败 · 用时」，配不上 ⇒「失败」；报错尾巴同一处写。
/// 写字只在这一处，不拿写好的字认事实。
#[test]
fn a_failed_step_says_failed_with_or_without_its_duration() {
    let failed = StepResult {
        ok: false,
        ..Default::default()
    };
    let mut clock = StepClock::default();
    clock.note(&reply(Some(T), &["c1"]));
    let mut r = with(Some(T + 90_000), &["c1", "c2"], failed);
    clock.time(&mut r);
    let dur = copy_core::short_duration(90_000);
    assert_eq!(
        cell(&r, "c1"),
        copy_core::copy_text("stream.step.failedFor", &[("dur", &dur)])
    );
    assert_eq!(
        cell(&r, "c2"),
        copy_core::copy_text("runs.state.failed", &[]),
        "配不上：照没有用时的那一形"
    );
    let Body::Said { results, .. } = &r.body else {
        panic!("不是带结果的那一条")
    };
    assert_eq!(
        results["c2"].error.as_ref().map(|w| w.0.clone()),
        Some(copy_core::copy_text("stream.result.error", &[]))
    );
}

/// ★★ 过程那一行右侧那一句只在这里写（`said`）：改动 `+a −r`（为零的那一半不写）· 读了几行只给读文件那一类 · 几个文件 ·
/// 未批准 · 答了什么；都说不上 ⇒ 空串，配上了用时 ⇒ 只写用时；别的那几形配上了也不带用时。
#[test]
fn the_right_hand_sentence_is_written_here_once() {
    use crate::agents::Answer;
    let t = |r: StepResult, ms: Option<u64>| said(r, ms).text.0;
    let ok = StepResult {
        ok: true,
        ..Default::default()
    };
    let dur = copy_core::short_duration(90_000);
    // 读文件：读了几行；命令输出几行 · 搜索命中几行不说。
    let read = StepResult {
        lines: Some(12),
        read: true,
        ..ok.clone()
    };
    assert_eq!(
        t(read.clone(), None),
        copy_core::copy_text("stream.step.lines", &[("n", "12")])
    );
    assert_eq!(
        t(read.clone(), Some(90_000)),
        t(read, None),
        "读了几行：不带用时"
    );
    let out = StepResult {
        lines: Some(2),
        ..ok.clone()
    };
    assert_eq!(t(out.clone(), None), "", "不是读文件那一类：行数不说");
    assert_eq!(t(out, Some(90_000)), dur, "都说不上：只剩用时");
    // 改动。
    let diff = StepResult {
        added: Some(2),
        removed: Some(1),
        ..ok.clone()
    };
    assert_eq!(t(diff, None), "+2 −1");
    let only_add = StepResult {
        added: Some(3),
        removed: Some(0),
        ..ok.clone()
    };
    assert_eq!(t(only_add, None), "+3");
    // 几个文件。
    let files = StepResult {
        files: Some(4),
        ..ok.clone()
    };
    assert_eq!(
        t(files, None),
        copy_core::copy_text("stream.step.files", &[("n", "4")])
    );
    // 答了什么。
    let approved = StepResult {
        answer: Some(Answer::Approved),
        ..ok.clone()
    };
    assert_eq!(
        t(approved, Some(90_000)),
        copy_core::copy_text("interactive.done.approved", &[])
    );
    // 未批准：不写报错尾巴、配上了也不带用时。
    let rejected = said(
        StepResult {
            ok: false,
            rejected: true,
            ..Default::default()
        },
        Some(90_000),
    );
    assert_eq!(
        rejected.text.0,
        copy_core::copy_text("stream.step.rejected", &[])
    );
    assert_eq!(rejected.error, None);
    // 失败带退出码 ⇒ 尾巴写退出码。
    let exit = said(
        StepResult {
            ok: false,
            exit_code: Some(2),
            ..Default::default()
        },
        None,
    );
    assert_eq!(
        exit.error.map(|w| w.0),
        Some(copy_core::copy_text(
            "stream.result.exitCode",
            &[("n", "2")]
        ))
    );
}
