//! 通用记录的线上形状由 `claudecode/record_of_tests.rs` 的金样钉（各类全格 ＋ 最少格）；这里只钉公共格的写法。
use super::*;

#[test]
fn the_common_fields_are_camel_case_and_the_kind_is_t() {
    let mut r = record(
        "a".into(),
        "i".into(),
        None,
        Body::Title {
            text: "x".into(),
            by: TitleBy::User,
        },
    );
    r.time_text = Some("09:30".into());
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"agent":"a","id":"i","timeText":"09:30","draw":"none","human":false,"t":"title","text":"x","by":"user"}"#
    );
}

/// 画成哪一形 · 人说没说：逐种来源与回复的样子（判定只在 [`record`]，两家适配层造记录都经它）。
#[test]
fn draw_and_human_are_worked_out_once_from_the_body() {
    use cmd_wire::record::{Draw, Speaker, ToolCard, UserText};
    use std::collections::BTreeMap;
    let said = |speaker: Speaker, text: &str, blocks: Vec<Block>| Body::Said {
        who: UserText {
            speaker,
            text: text.into(),
            pasted: Vec::new(),
        },
        blocks,
        results: BTreeMap::new(),
        cwd: None,
        attachments: Vec::new(),
    };
    let reply = |blocks: Vec<Block>, cards: &[(&str, ToolCard)]| Body::Reply {
        blocks,
        model: None,
        auto_reply: false,
        cards: cards.iter().map(|(k, c)| (k.to_string(), *c)).collect(),
        steps: BTreeMap::new(),
        runs: BTreeMap::new(),
        error: None,
    };
    let text = |s: &str| Block::text(s.into());
    let call = |id: &str| Block::ToolUse {
        id: id.into(),
        name: "X".into(),
        input: serde_json::json!({}),
    };
    let result = || Block::tool_result("c".into(), vec![], false);
    let image = || Block::Image {
        media_type: "image/png".into(),
        bytes: 1,
        at: 0,
        n: 0,
        data: String::new(),
    };
    let quiet = |r: Body| match r {
        Body::Reply {
            blocks,
            cards,
            steps,
            runs,
            model,
            error,
            ..
        } => Body::Reply {
            blocks,
            model,
            auto_reply: true,
            cards,
            steps,
            runs,
            error,
        },
        b => b,
    };
    let failed = |r: Body| match r {
        Body::Reply {
            blocks,
            cards,
            steps,
            runs,
            model,
            auto_reply,
            ..
        } => Body::Reply {
            blocks,
            model,
            auto_reply,
            cards,
            steps,
            runs,
            error: Some(ReplyError {
                reason: cmd_wire::record::ApiReason::Unknown,
                status: None,
            }),
        },
        b => b,
    };
    let cases: Vec<(&str, Body, Draw, bool)> = vec![
        (
            "人说的",
            said(Speaker::Human, "hi", vec![]),
            Draw::Card,
            true,
        ),
        (
            "人说的、只带图",
            said(Speaker::Human, "", vec![image()]),
            Draw::Card,
            true,
        ),
        (
            "人说的、空",
            said(Speaker::Human, "", vec![]),
            Draw::None,
            true,
        ),
        (
            "斜杠命令",
            said(
                Speaker::SlashCommand {
                    name: "/x".into(),
                    args: String::new(),
                },
                "",
                vec![],
            ),
            Draw::Card,
            true,
        ),
        (
            "! 命令",
            said(
                Speaker::BashInput {
                    command: "ls".into(),
                },
                "",
                vec![],
            ),
            Draw::Card,
            true,
        ),
        (
            "! 命令的输出",
            said(
                Speaker::BashOutput {
                    stdout: "a".into(),
                    stderr: String::new(),
                },
                "",
                vec![],
            ),
            Draw::Card,
            false,
        ),
        (
            "本地命令回显",
            said(Speaker::CommandOutput, "", vec![]),
            Draw::None,
            false,
        ),
        (
            "系统注入",
            said(
                Speaker::System {
                    body: Some("x".into()),
                },
                "",
                vec![],
            ),
            Draw::Strip,
            false,
        ),
        (
            "系统注入、没有正文",
            said(Speaker::System { body: None }, "", vec![]),
            Draw::None,
            false,
        ),
        (
            "派的活",
            said(Speaker::AgentTask, "do", vec![]),
            Draw::Card,
            false,
        ),
        (
            "派的活、空",
            said(Speaker::AgentTask, "", vec![]),
            Draw::None,
            false,
        ),
        (
            "压缩摘要、空",
            said(
                Speaker::CompactSummary {
                    chars_text: cmd_wire::cells::Words("0".into()),
                },
                "",
                vec![],
            ),
            Draw::None,
            false,
        ),
        (
            "interrupt",
            said(Speaker::Interrupt, "", vec![]),
            Draw::Card,
            false,
        ),
        (
            "工具结果",
            said(Speaker::ToolResult, "", vec![result()]),
            Draw::Steps,
            false,
        ),
        (
            "工具结果、空",
            said(Speaker::ToolResult, "", vec![]),
            Draw::None,
            false,
        ),
        (
            "回复有字",
            reply(vec![text("a"), call("c")], &[]),
            Draw::Card,
            false,
        ),
        (
            "回复只有工具",
            reply(vec![call("c"), text("  ")], &[]),
            Draw::Steps,
            false,
        ),
        (
            "回复里有交互等待",
            reply(vec![call("c")], &[("c", ToolCard::Interactive)]),
            Draw::Card,
            false,
        ),
        ("回复空正文", reply(vec![text(" ")], &[]), Draw::None, false),
        (
            "思考没写进记录（空串）照画",
            reply(
                vec![Block::Thinking {
                    text: String::new(),
                }],
                &[],
            ),
            Draw::Steps,
            false,
        ),
        (
            "自动应答",
            quiet(reply(vec![text("a")], &[])),
            Draw::None,
            false,
        ),
        ("reply-error", failed(reply(vec![], &[])), Draw::Card, false),
        (
            "retry",
            Body::Retry {
                reason: cmd_wire::record::ApiReason::Unknown,
                attempt: None,
                max: None,
            },
            Draw::Card,
            false,
        ),
        (
            "标题",
            Body::Title {
                text: "x".into(),
                by: TitleBy::Agent,
            },
            Draw::None,
            false,
        ),
        (
            "认不出",
            Body::unread(UnreadWhy::UnknownType, None, "{}"),
            Draw::Card,
            false,
        ),
    ];
    for (what, body, draw, human) in cases {
        let r = record("a".into(), "i".into(), None, body);
        assert_eq!((r.draw, r.human), (draw, human), "{what}");
    }
}

/// 收场通知那一行的字由出口写（照看的那一台的时区写钟面）：后台任务 · 做的那件事 · 怎么收场 · 几点；界面照抄。
#[test]
fn stamp_writes_the_notice_row() {
    use cmd_wire::record::{Speaker, TaskEnd, UserText};
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
                    pasted: Vec::new(),
                },
                blocks: Vec::new(),
                results: std::collections::BTreeMap::new(),
                cwd: None,
                attachments: Vec::new(),
            },
        )
    };
    let row = |mut r: Record| {
        r.stamp(&crate::Tz::named("Asia/Shanghai").unwrap());
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
