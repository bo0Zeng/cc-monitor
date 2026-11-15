//! 出口那一下写记录的钟面那几格：照看的那一台的时区写 `timeText` · `atMs`，重试那一行与收场通知那一行照钟面拼。
use super::*;
use crate::agents::record::{record, retry, RetryFacts};
use cmd_wire::record::{Speaker, TaskEnd, UserText};

/// 不带私有格的那一行过出口。
fn stamped(r: Record, tz: &Tz) -> Record {
    BackRecord::from(r).stamp(tz)
}

fn retry_text(r: &Record) -> Option<&str> {
    match &r.body {
        Body::Retry { text } => text.as_ref().map(|w| w.0.as_str()),
        _ => None,
    }
}

/// ★ 重试那一行由出口照后端的行里那几格（原因 · 次数）与看的那一台的钟面写；造记录那一下不写，线上那一形里没有那几格。
#[test]
fn stamp_writes_the_retry_line_from_the_back_facts() {
    let facts = RetryFacts {
        reason: crate::agents::ApiReason::Overloaded,
        attempt: Some(2),
        max: Some(10),
    };
    let back = retry(
        "fake".into(),
        "r1".into(),
        Some("2026-01-01T06:02:00.000Z".into()),
        facts,
    );
    assert_eq!(retry_text(&back.rec), None, "造记录那一下不写那一行");
    let out = back.stamp(&Tz::named("Asia/Shanghai").unwrap());
    assert_eq!(
        retry_text(&out).map(str::to_string),
        Some(retry_line(facts.reason, facts.attempt, facts.max, "14:02"))
    );
}

/// ★ 不带私有格、那一行已写好的重试那一条（线上那一形包成后端的行）过出口：那一行原样，不再写一遍。
#[test]
fn a_retry_line_already_written_passes_the_exit_as_is() {
    let mut made = record(
        "fake".into(),
        "r2".into(),
        Some("2026-01-01T06:02:00.000Z".into()),
        Body::Retry { text: None },
    );
    made.body = Body::Retry {
        text: Some(Words("那边写好的一行".into())),
    };
    let out = stamped(made, &Tz::named("Asia/Shanghai").unwrap());
    assert_eq!(retry_text(&out), Some("那边写好的一行"));
}

fn said(at: &str) -> Record {
    record(
        "claude".into(),
        "s1".into(),
        Some(at.into()),
        Body::Said {
            who: UserText {
                speaker: Speaker::Human,
                text: "x".into(),
                parts: Vec::new(),
            },
            blocks: Vec::new(),
            results: std::collections::BTreeMap::new(),
            cwd: None,
            attachments: Vec::new(),
        },
    )
}

/// ★ 钟面按看的那一台的时区写（同一份记录给不同时区的人各写各的）；`at` 解不出 ⇒ 两格都不上线。
#[test]
fn the_clock_face_is_written_for_the_viewers_zone() {
    let ts = "2026-10-07T20:30:15.123Z";
    let made = said(ts);
    assert!(
        made.time_text.is_none() && made.at_ms.is_none(),
        "造记录那一下不写钟面"
    );
    let r = stamped(made.clone(), &Tz::named("Asia/Shanghai").unwrap());
    assert_eq!(r.time_text.as_ref().map(|w| w.0.as_str()), Some("04:30"));
    assert_eq!(r.at_ms, parse_iso8601_ms(ts));
    assert!(r.at_ms.is_some());
    let r = stamped(made, &Tz::default());
    assert_eq!(r.time_text.as_ref().map(|w| w.0.as_str()), Some("20:30"));

    let bad = stamped(said("soon"), &Tz::named("Asia/Shanghai").unwrap());
    assert!(
        bad.time_text.is_none() && bad.at_ms.is_none(),
        "解不出 ⇒ 不上线"
    );
}

/// ★ 收场通知那一行的字由出口写（照看的那一台的时区写钟面）：后台任务 · 做的那件事 · 怎么收场 · 几点；界面照抄。
#[test]
fn stamp_writes_the_notice_row() {
    let notice = |summary: Option<&str>, end: Option<TaskEnd>| {
        record(
            "claude".into(),
            "n1".into(),
            Some("2026-01-01T06:02:00.000Z".into()),
            Body::Said {
                who: UserText {
                    speaker: Speaker::TaskNotification {
                        task_id: Some("b7".into()),
                        status: None,
                        summary: summary.map(str::to_string),
                        tool_use_id: None,
                        end,
                        row: None,
                    },
                    text: String::new(),
                    parts: Vec::new(),
                },
                blocks: Vec::new(),
                results: std::collections::BTreeMap::new(),
                cwd: None,
                attachments: Vec::new(),
            },
        )
    };
    let row = |r: Record| match stamped(r, &Tz::named("Asia/Shanghai").unwrap()).body {
        Body::Said { who, .. } => match who.speaker {
            Speaker::TaskNotification { row, .. } => row.map(|w| w.0),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let want = |what: &str, state: &str| {
        copy_core::copy_text(
            "speaker.notice.row",
            &[
                ("what", what),
                ("state", &copy_core::copy_text(state, &[])),
                ("time", "14:02"),
            ],
        )
    };
    assert_eq!(
        row(notice(Some("扫目录"), Some(TaskEnd::Stopped))),
        Some(want("扫目录", "runs.state.stopped"))
    );
    assert_eq!(
        row(notice(Some("扫目录"), Some(TaskEnd::Failed))),
        Some(want("扫目录", "runs.state.failed"))
    );
    assert_eq!(
        row(notice(None, Some(TaskEnd::Done))),
        Some(want("b7", "runs.state.done")),
        "没有摘要 ⇒ 写任务号"
    );
}

/// ★ 压缩摘要的标题由出口写：字数照显示的正文按字数（不按字节 · UTF-16），每三位一个逗号；字数不另成一格上线。
#[test]
fn stamp_writes_the_compact_title_from_the_shown_text() {
    let r = record(
        "claude".into(),
        "c1".into(),
        Some("2026-01-01T06:02:00.000Z".into()),
        Body::Said {
            who: UserText {
                speaker: Speaker::CompactSummary { title_text: None },
                text: "字".repeat(1234),
                parts: Vec::new(),
            },
            blocks: Vec::new(),
            results: std::collections::BTreeMap::new(),
            cwd: None,
            attachments: Vec::new(),
        },
    );
    let Body::Said { who, .. } = stamped(r, &Tz::named("Asia/Shanghai").unwrap()).body else {
        unreachable!()
    };
    assert_eq!(
        serde_json::to_value(&who.speaker).unwrap(),
        serde_json::json!({
            "kind": "compactSummary",
            "titleText": copy_core::copy_text("compact.summary.title", &[("n", "1,234"), ("time", "14:02")]),
        })
    );
}
