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

/// 来话（agent 交回 · 另一个会话 · 主会话后来的话）的正文原文住后端那一形（[`Heard`]），造记录那一下解成树
/// （与助手正文同一个 `md_core::parse`）；线上只交树（原文那一格线上类型里没有）；没有正文 ⇒ 树也缺。
#[test]
fn incoming_message_bodies_go_out_as_trees() {
    use cmd_wire::record::Speaker;
    let md = "交回：\\(a+b\\)\n\n- 一\n- 二";
    let said = |speaker: Speaker, body: Option<String>| {
        record(
            "a".into(),
            "i".into(),
            None,
            Body::Said {
                who: Heard::of(speaker, body).said(),
                blocks: Vec::new(),
                results: Default::default(),
                cwd: None,
                attachments: Vec::new(),
            },
        )
    };
    let speakers = [
        Speaker::AgentMessage {
            from: Some("a1".into()),
            name: None,
            handback: true,
            doc: None,
        },
        Speaker::PeerSession {
            from: None,
            doc: None,
            title: None,
        },
        Speaker::Coordinator { doc: None },
    ];
    for sp in speakers {
        let r = said(sp, Some(md.into()));
        let wire = serde_json::to_value(&r).unwrap();
        let s = &wire["who"]["speaker"];
        assert_eq!(
            s["doc"],
            serde_json::to_value(md_core::parse(md)).unwrap(),
            "{s}"
        );
    }
    let none = said(Speaker::Coordinator { doc: None }, None);
    let s = &serde_json::to_value(&none).unwrap()["who"]["speaker"];
    assert!(s.get("doc").is_none(), "{s}");
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
            parts: Vec::new(),
        },
        blocks,
        results: BTreeMap::new(),
        cwd: None,
        attachments: Vec::new(),
    };
    let reply = |blocks: Vec<Block>, cards: &[(&str, ToolCard)]| Body::Reply {
        blocks,
        auto_reply: false,
        cards: cards.iter().map(|(k, c)| (k.to_string(), *c)).collect(),
        steps: BTreeMap::new(),
        runs: BTreeMap::new(),
        error: None,
    };
    // 助手的正文照适配层那样解析成树（[`reply_blocks`]）。
    let prose = |s: &str| reply_blocks(vec![Block::text(s.into())]).remove(0);
    let call = |id: &str| Block::ToolUse {
        id: id.into(),
        name: "X".into(),
        input: serde_json::json!({}),
    };
    let result = || Block::tool_result("c".into(), vec![], false, copy_core::size_text);
    let image = || Block::Image {
        media_type: "image/png".into(),
        bytes: 1,
        at: 0,
        n: 0,
    };
    let quiet = |r: Body| match r {
        Body::Reply {
            blocks,
            cards,
            steps,
            runs,
            error,
            ..
        } => Body::Reply {
            blocks,
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
            auto_reply,
            ..
        } => Body::Reply {
            blocks,
            auto_reply,
            cards,
            steps,
            runs,
            error: Some(reply_error(cmd_wire::record::ApiReason::Unknown, None)),
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
                    stdout_fold: None,
                    stderr_fold: None,
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
            said(Speaker::CompactSummary { title_text: None }, "", vec![]),
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
            reply(vec![prose("a"), call("c")], &[]),
            Draw::Card,
            false,
        ),
        (
            "回复只有工具",
            reply(vec![call("c"), prose("  ")], &[]),
            Draw::Steps,
            false,
        ),
        (
            "回复里有交互等待",
            reply(vec![call("c")], &[("c", ToolCard::Interactive)]),
            Draw::Card,
            false,
        ),
        (
            "回复空正文",
            reply(vec![prose(" ")], &[]),
            Draw::None,
            false,
        ),
        (
            "思考没写进记录（空串）照画",
            reply(vec![crate::agents::beat::thinking(String::new())], &[]),
            Draw::Steps,
            false,
        ),
        (
            "自动应答",
            quiet(reply(vec![prose("a")], &[])),
            Draw::None,
            false,
        ),
        ("reply-error", failed(reply(vec![], &[])), Draw::Card, false),
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
            unread_body(UnreadWhy::UnknownType, None, "{}"),
            Draw::Card,
            false,
        ),
    ];
    for (what, body, draw, human) in cases {
        let r = record("a".into(), "i".into(), None, body);
        assert_eq!((r.draw, r.human), (draw, human), "{what}");
    }
    // 重试那一条照后端的行里的次数画：头一次（或没写次数）成卡，接在一串后面的不画。
    for (what, attempt, draw) in [
        ("retry", None, Draw::Card),
        ("接在一串后面的重试", Some(2), Draw::None),
    ] {
        let facts = RetryFacts {
            reason: cmd_wire::record::ApiReason::Unknown,
            attempt,
            max: attempt.map(|_| 10),
        };
        let b = retry("a".into(), "i".into(), None, facts);
        assert_eq!((b.rec.draw, b.rec.human), (draw, false), "{what}");
        assert_eq!(b.retry, Some(facts), "{what}");
    }
}
