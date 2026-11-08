//! 出口那一下写记录的钟面那几格：照看的那一台的时区写 `timeText` · `atMs`，重试那一行与收场通知那一行照钟面拼。
use super::*;
use crate::agents::record::record;
use cmd_wire::record::{Speaker, TaskEnd, UserText};

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
    let mut r = said(ts);
    assert!(
        r.time_text.is_none() && r.at_ms.is_none(),
        "造记录那一下不写钟面"
    );
    r.stamp(&Tz::named("Asia/Shanghai").unwrap());
    assert_eq!(r.time_text.as_ref().map(|w| w.0.as_str()), Some("04:30"));
    assert_eq!(r.at_ms, parse_iso8601_ms(ts));
    assert!(r.at_ms.is_some());
    r.stamp(&Tz::default());
    assert_eq!(r.time_text.as_ref().map(|w| w.0.as_str()), Some("20:30"));

    let mut bad = said("soon");
    bad.stamp(&Tz::named("Asia/Shanghai").unwrap());
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
    let row = |mut r: Record| {
        r.stamp(&Tz::named("Asia/Shanghai").unwrap());
        match r.body {
            Body::Said { who, .. } => match who.speaker {
                Speaker::TaskNotification { row, .. } => row.map(|w| w.0),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
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
