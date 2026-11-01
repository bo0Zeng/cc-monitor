//! 一步用时的配对表：调用记下时刻，结果来了配上、写进那一格；配不上 · 结果早于调用 ⇒ 不写；表封顶丢最早的。
//!
//! 夹具只造结构（记录类型、调用 id、时刻），不采任何真会话正文。

use super::*;
use crate::agents::record::{Block, Body, Draw, Record};
use crate::agents::{Speaker, StepResult, StepResultExt, UserText};
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
            model: None,
            auto_reply: false,
            cards: BTreeMap::new(),
            steps: BTreeMap::new(),
            runs: BTreeMap::new(),
            error: None,
        },
    }
}

/// 一条带这几次结果的记录（都说不上的那一形：只有耗时那一句），时刻 `at_ms`。
fn results(at_ms: Option<i64>, of: &[&str]) -> Record {
    let plain = StepResult {
        ok: true,
        ..Default::default()
    }
    .written(false);
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
                pasted: Vec::new(),
            },
            blocks: of
                .iter()
                .map(|id| Block::tool_result(id.to_string(), Vec::new(), false))
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

/// 那一格的 `(text, timed)`。
fn cell(r: &Record, id: &str) -> (String, Option<String>) {
    match &r.body {
        Body::Said { results, .. } => {
            let c = &results[id];
            (c.text.0.clone(), c.timed.as_ref().map(|w| w.0.clone()))
        }
        _ => panic!("不是带结果的那一条"),
    }
}

const T: i64 = 1_700_000_000_000;

/// ★ 调用 `c1` 在 t，隔 90 秒来了它的结果 ⇒ 那一格写好用时、带耗时那一形清掉。
#[test]
fn a_result_ninety_seconds_after_its_call_gets_its_duration_written() {
    let mut clock = StepClock::default();
    clock.note(&reply(Some(T), &["c1"]));
    let mut r = results(Some(T + 90_000), &["c1"]);
    clock.time(&mut r);
    assert_eq!(cell(&r, "c1"), (copy_core::short_duration(90_000), None));
    assert_eq!(clock.len(), 0, "配上了就从表里摘掉");
}

/// ★ 结果的时刻早于调用（时钟回拨）· 没有记下的调用 · 结果没有时刻 ⇒ 当配不上，那一格不动。
#[test]
fn a_result_earlier_than_its_call_or_unmatched_is_left_unwritten() {
    let untouched = cell(&results(None, &["c1"]), "c1");
    assert_eq!(untouched, (String::new(), Some("{dur}".to_string())));

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
    assert_eq!(
        cell(&r, "c0"),
        (String::new(), Some("{dur}".to_string())),
        "最早那一条已丢"
    );
    assert_eq!(cell(&r, "c1"), (copy_core::short_duration(59_999), None));
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
    assert_eq!(cell(&r, "c2"), (copy_core::short_duration(60_000), None));
}
