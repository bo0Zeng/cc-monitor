//! `INVARIANTS §20`：CLI 注入的、agent 发来的都不算用户说的话；「谁说的」只有一份判定。
//!
//! 夹具的结构照本机记录的形状（记录级字段 · 具名框 · 固定句），正文全是编的。

use super::*;
use crate::agents::{Pasted, Speaker, UserText};
use serde_json::{json, Value};

#[test]
fn extract_text_blocks_string_and_array() {
    assert_eq!(extract_text_blocks(&Value::String("hi".into())), "hi");
    let arr = serde_json::json!([
        {"type":"text","text":"line1"},
        {"type":"tool_use","name":"Bash","input":{}},
        {"type":"text","text":"line2"}
    ]);
    assert_eq!(extract_text_blocks(&arr), "line1\nline2");
}

#[test]
fn extract_tool_text_assistant_and_user() {
    let asst = serde_json::json!([{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]);
    let t = extract_tool_text(&asst, true);
    assert!(t.contains("Bash") && t.contains("ls -la"));
    let user =
        serde_json::json!([{"type":"tool_result","content":[{"type":"text","text":"file out"}]}]);
    assert!(extract_tool_text(&user, false).contains("file out"));
}

/// 一条 user 记录（正文是字符串）；`extra` 是记录级字段。
fn rec(text: &str, extra: Value) -> Value {
    let mut v = json!({
        "type": "user",
        "uuid": "u1",
        "parentUuid": "p0",
        "timestamp": "2026-10-02T00:00:00Z",
        "message": {"role": "user", "content": text},
    });
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

/// 两个入口（通用层的已解析 JSON · 解析成品）判出来必须是同一个结果 —— 规则只有一份，喂法有两种。
fn said(v: &Value) -> UserText {
    let a = user_text_of_record(v).expect("user 记录");
    let line = serde_json::to_string(v).unwrap();
    let parsed = super::super::parse::parse_line(&line).unwrap().unwrap();
    let b = match parsed {
        super::super::schema::JsonlRecord::User { user_text, .. } => user_text,
        other => panic!("不是 user：{other:?}"),
    };
    assert_eq!(a, b, "两个入口判得不一样：{line}");
    a
}

fn kind(v: &Value) -> &'static str {
    said(v).speaker.kind()
}

fn human(text: &str) -> UserText {
    UserText {
        speaker: Speaker::Human,
        text: text.to_string(),
        pasted: Vec::new(),
    }
}

#[test]
fn record_fields_decide_first() {
    // 人：origin.kind = human；正文里贴着一段 agent 的框也还是人（记录级字段说了算）。
    let pasted_frame = "看看这个 <agent-message from=\"a1\">甲乙</agent-message>";
    assert_eq!(
        said(&rec(
            pasted_frame,
            json!({"origin": {"kind": "human"}, "promptSource": "typed"})
        )),
        human(pasted_frame)
    );
    // 子 agent 交回：origin 带 id / 名字 / handback；前导句与尾随说明跟着框一起是非人。
    let handback = "Another Claude session sent a message:\n<agent-message from=\"a9\">\n甲乙丙\n</agent-message>\n\nThat \"other Claude session\" is an agent working inside this same session — 丁戊。";
    assert_eq!(
        said(&rec(
            handback,
            json!({"isMeta": true, "origin": {"kind": "peer", "from": "a9", "senderTaskId": "a9", "body": "甲乙丙", "handback": true}})
        )),
        UserText::of(Speaker::AgentMessage {
            from: Some("a9".into()),
            name: None,
            handback: true,
            body: Some("甲乙丙".into())
        })
    );
    assert_eq!(
        said(&rec(
            handback,
            json!({"isMeta": true, "origin": {"kind": "peer", "from": "a9", "name": "乙路", "senderTaskId": "a9", "body": "甲"}})
        ))
        .speaker,
        Speaker::AgentMessage {
            from: Some("a9".into()),
            name: Some("乙路".into()),
            handback: false,
            body: Some("甲".into())
        }
    );
    // 后台任务通知：框里的几格抽出来；没有框只有字段的那一形也认。
    let notice = "<task-notification>\n<task-id>b7</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n<output-file>/x/y</output-file>\n<status>completed</status>\n<summary>甲完成了</summary>\n<result>乙丙丁</result>\n</task-notification>";
    assert_eq!(
        said(&rec(
            notice,
            json!({"origin": {"kind": "task-notification"}, "promptSource": "system"})
        ))
        .speaker,
        Speaker::TaskNotification {
            task_id: Some("b7".into()),
            status: Some("completed".into()),
            summary: Some("甲完成了".into()),
            tool_use_id: Some("toolu_1".into()),
        }
    );
    assert_eq!(
        said(&rec(
            "3 个后台任务甲乙",
            json!({"origin": {"kind": "task-notification"}})
        ))
        .speaker,
        Speaker::TaskNotification {
            task_id: None,
            status: None,
            summary: None,
            tool_use_id: None
        }
    );
    assert_eq!(
        kind(&rec(
            "Your claude.ai usage limit has reset. 甲",
            json!({"isMeta": true, "origin": {"kind": "auto-continuation"}})
        )),
        "system"
    );
    // 压缩摘要：字段说了算，正文照显示。
    assert_eq!(
        said(&rec(
            "甲乙摘要",
            json!({"isCompactSummary": true, "isVisibleInTranscriptOnly": true})
        )),
        UserText {
            speaker: Speaker::CompactSummary,
            text: "甲乙摘要".into(),
            pasted: vec![]
        }
    );
    // isMeta 没有别的字段 ⇒ 系统注入（技能展开 · 提醒 · 定时触发 · 续跑样板）。
    assert_eq!(
        kind(&rec(
            "Base directory for this skill: /x\n\n甲乙",
            json!({"isMeta": true})
        )),
        "system"
    );
    assert_eq!(
        kind(&rec(
            "甲乙",
            json!({"isMeta": true, "turnOrigin": "scheduled", "promptSource": "system"})
        )),
        "system"
    );
    // origin 说是人、但又标了 isMeta ⇒ 不是人话。
    assert_eq!(
        kind(&rec(
            "甲乙",
            json!({"isMeta": true, "origin": {"kind": "human"}})
        )),
        "system"
    );
    // 认不出的 origin.kind ⇒ 不当字段用，走正文那一路（这里认不出 ⇒ 人）。
    assert_eq!(
        kind(&rec("甲乙", json!({"origin": {"kind": "something-new"}}))),
        "human"
    );
}

#[test]
fn the_sub_agent_side() {
    let sc = |extra: Value| {
        let mut e = json!({"isSidechain": true, "agentId": "a1"});
        for (k, x) in extra.as_object().unwrap() {
            e[k] = x.clone();
        }
        e
    };
    // 主会话派的活：子 agent 记录里的第一条（没有 parentUuid、没有 origin）。
    let mut task = rec("去查甲乙丙", sc(json!({})));
    task["parentUuid"] = Value::Null;
    assert_eq!(
        said(&task),
        UserText {
            speaker: Speaker::AgentTask,
            text: "去查甲乙丙".into(),
            pasted: vec![]
        }
    );
    // 主会话后来发来的话。
    assert_eq!(
        kind(&rec(
            "The coordinator sent a message while you were working:\n甲乙\n\nAddress this before completing your current task.",
            sc(json!({"isMeta": true, "origin": {"kind": "coordinator"}}))
        )),
        "coordinator"
    );
    // 来话正文：记录级 `origin.body` 优先；没有就取前导句之后那段（CLI 附的收尾句不算）。
    let co = "The coordinator sent a message while you were working:\n甲乙\n\nAddress this before completing your current task.";
    assert_eq!(
        said(&rec(
            co,
            sc(json!({"isMeta": true, "origin": {"kind": "coordinator"}}))
        ))
        .speaker,
        Speaker::Coordinator {
            body: Some("甲乙".into())
        }
    );
    assert_eq!(
        said(&rec(
            co,
            sc(json!({"isMeta": true, "origin": {"kind": "coordinator", "body": "丙"}}))
        ))
        .speaker,
        Speaker::Coordinator {
            body: Some("丙".into())
        }
    );
    assert_eq!(
        said(&rec(co, sc(json!({"isMeta": true})))).speaker,
        Speaker::Coordinator {
            body: Some("甲乙".into())
        }
    );
    // 子 agent 自己的后台任务通知：前面多四句固定话。
    let pre = "[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated background-task event, NOT a message from the user.\nDo NOT interpret this as user acknowledgement, confirmation, or response to any pending question.\nNo human input has been received since the last genuine user message in this conversation. 甲。\n\n<task-notification>\n<task-id>c3</task-id>\n<status>failed</status>\n<summary>乙</summary>\n</task-notification>";
    assert_eq!(
        said(&rec(
            pre,
            sc(json!({"isMeta": true, "origin": {"kind": "task-notification"}}))
        ))
        .speaker,
        Speaker::TaskNotification {
            task_id: Some("c3".into()),
            status: Some("failed".into()),
            summary: Some("乙".into()),
            tool_use_id: None
        }
    );
    // 同一段没有记录级字段（老版本）也认得出：标记行与框都是具名的。
    assert_eq!(kind(&rec(pre, json!({}))), "taskNotification");
    // 提醒注入 · 打断标记。
    assert_eq!(
        kind(&rec(
            "<system-reminder>甲</system-reminder>",
            sc(json!({"isMeta": true}))
        )),
        "system"
    );
    assert_eq!(
        kind(&rec(
            "[Request interrupted by user for tool use]",
            sc(json!({}))
        )),
        "interrupt"
    );
    // 分叉出来的子 agent：工具结果 ＋ 派活说明同一条。
    let mut fork = rec("", sc(json!({})));
    fork["message"]["content"] = json!([
        {"type": "tool_result", "tool_use_id": "t1", "content": "甲"},
        {"type": "text", "text": "<fork-boilerplate>\n乙丙\n</fork-boilerplate>"}
    ]);
    assert_eq!(kind(&fork), "agentTask");
}

#[test]
fn named_frames_without_record_fields() {
    let plain = |t: &str| rec(t, json!({}));
    assert_eq!(
        said(&plain("<task-notification>\n<task-id>d4</task-id>\n<status>killed</status>\n</task-notification>")).speaker,
        Speaker::TaskNotification {
            task_id: Some("d4".into()),
            status: Some("killed".into()),
            summary: None,
            tool_use_id: None
        }
    );
    assert_eq!(
        said(&plain("Another Claude session sent a message while you were working:\n<agent-message from=\"e5\">甲</agent-message>\n\nThat \"other Claude session\" is 乙。")).speaker,
        Speaker::AgentMessage {
            from: Some("e5".into()),
            name: None,
            handback: false,
            body: Some("甲".into())
        }
    );
    assert_eq!(
        said(&plain(
            "<cross-session-message from=\"会话乙\">甲</cross-session-message>"
        ))
        .speaker,
        Speaker::PeerSession {
            from: Some("会话乙".into()),
            body: Some("甲".into())
        }
    );
    // 斜杠命令：三个标签顺序随版本漂，转义要解。
    for t in [
        "<command-name>/甲乙</command-name>\n<command-message>甲乙</command-message>\n<command-args>丙 &amp; 丁</command-args>",
        "<command-message>甲乙</command-message>\n<command-name>/甲乙</command-name>\n<command-args>丙 &amp; 丁</command-args>",
    ] {
        assert_eq!(
            said(&plain(t)).speaker,
            Speaker::SlashCommand {
                name: "/甲乙".into(),
                args: "丙 & 丁".into()
            },
            "{t}"
        );
    }
    // 带着别的字的不是斜杠命令（人话里提到了标签）⇒ 人。
    assert_eq!(
        kind(&plain("<command-name>/甲</command-name> 乙丙")),
        "human"
    );
    assert_eq!(
        said(&plain("<bash-input>ls &gt; 甲 &amp;&amp; pwd</bash-input>")).speaker,
        Speaker::BashInput {
            command: "ls > 甲 && pwd".into()
        }
    );
    assert_eq!(
        said(&plain(
            "<bash-stdout>甲\n乙</bash-stdout><bash-stderr>&lt;丙&gt;</bash-stderr>"
        ))
        .speaker,
        Speaker::BashOutput {
            stdout: "甲\n乙".into(),
            stderr: "<丙>".into()
        }
    );
    assert_eq!(
        kind(&plain("<local-command-stdout>甲乙</local-command-stdout>")),
        "commandOutput"
    );
    assert_eq!(
        kind(&plain(
            "<local-command-caveat>Caveat: 甲</local-command-caveat>"
        )),
        "system"
    );
    assert_eq!(kind(&plain("Continue from where you left off.")), "system");
    assert_eq!(
        kind(&plain(
            "This session is being continued from a previous conversation 甲乙"
        )),
        "compactSummary"
    );
    assert_eq!(kind(&plain("[Request interrupted by user]")), "interrupt");
    let mut tool = plain("");
    tool["message"]["content"] =
        json!([{"type": "tool_result", "tool_use_id": "t", "content": "甲"}]);
    assert_eq!(kind(&tool), "toolResult");
}

#[test]
fn what_is_not_recognized_stays_human() {
    let plain = |t: &str| rec(t, json!({}));
    // 人真会以 `<` 开头打字；只认具名框，而且框要有收尾。
    for t in [
        "<div>甲</div> 乙",
        "<agent-message 是什么意思",
        "<task-notification> 这个标签怎么来的",
        "<cross-session-message from=\"x\"> 没收尾",
        "[SYSTEM 甲乙]",
        "甲乙丙",
    ] {
        assert_eq!(said(&plain(t)), human(t), "{t}");
    }
    // 本来就没字（只有图片）⇒ 仍是人，正文空（界面不建卡）。
    let mut img = plain("");
    img["message"]["content"] = json!([{"type": "image", "source": {}}]);
    assert_eq!(said(&img), human(""));
}

#[test]
fn mixed_records_keep_the_human_part() {
    let plain = |t: &str| rec(t, json!({"origin": {"kind": "human"}}));
    let cases: [(&str, &str); 7] = [
        ("甲乙\n<system-reminder>丙</system-reminder>", "甲乙"),
        (
            "<local-command-stderr>丙</local-command-stderr>真话",
            "真话",
        ),
        (
            "先说一句\nNo response requested.\n再说一句",
            "先说一句\n\n再说一句",
        ),
        (
            "please continue from where you left off.",
            "please continue from where you left off.",
        ),
        (
            "[Request interrupted by user]\n接着说的真话",
            "[Request interrupted by user]\n接着说的真话",
        ),
        ("甲<task-notification>乙</task-notification>丙", "甲丙"),
        ("  甲  ", "甲"),
    ];
    for (input, text) in cases {
        assert_eq!(said(&plain(input)), human(text), "{input:?}");
    }
    assert_eq!(
        kind(&plain(
            "<system-reminder>x</system-reminder>  [Request interrupted by user]  "
        )),
        "interrupt"
    );
    assert_eq!(kind(&plain("no response requested")), "system");
}

#[test]
fn pasted_blocks_are_human_with_their_bounds() {
    // 下标是 UTF-16 的：「😀」占两格。
    let t = "😀看这段：<pasted_content id=\"p1\">甲\n乙</pasted_content id=\"p1\">\n再看<pasted_content>丙</pasted_content>";
    let u = said(&rec(t, json!({"origin": {"kind": "human"}})));
    assert_eq!(u.speaker, Speaker::Human);
    assert_eq!(u.text, t);
    let at = |s: &str| t[..t.find(s).unwrap()].encode_utf16().count() as u32;
    let first_end = at("\n再看");
    assert_eq!(
        u.pasted,
        vec![
            Pasted {
                id: Some("p1".into()),
                start: at("<pasted_content id"),
                end: first_end,
                body_start: at("甲"),
                body_end: at("</pasted_content id"),
                lines: 2,
            },
            Pasted {
                id: None,
                start: at("<pasted_content>"),
                end: t.encode_utf16().count() as u32,
                body_start: at("丙"),
                body_end: at("</pasted_content>"),
                lines: 1,
            },
        ]
    );
    let back: String = String::from_utf16(
        &t.encode_utf16().collect::<Vec<_>>()[u.pasted[0].start as usize..u.pasted[0].end as usize],
    )
    .unwrap();
    assert_eq!(
        back,
        "<pasted_content id=\"p1\">甲\n乙</pasted_content id=\"p1\">"
    );
    // 只粘贴、没打字 ⇒ 仍是人。
    assert_eq!(
        kind(&rec(
            "<pasted_content id=\"p2\">丁</pasted_content id=\"p2\">",
            json!({"origin": {"kind": "human"}})
        )),
        "human"
    );
}

#[test]
fn queued_messages_use_the_same_named_frames() {
    assert_eq!(queued_text("<task-notification>\n<task-id>f6</task-id>\n<status>completed</status>\n</task-notification>").speaker.kind(), "taskNotification");
    assert_eq!(
        queued_text("<agent-message from=\"g7\">\n甲\n</agent-message>").speaker,
        Speaker::AgentMessage {
            from: Some("g7".into()),
            name: None,
            handback: false,
            body: Some("甲".into())
        }
    );
    assert_eq!(
        queued_text("Your claude.ai usage limit has reset. 甲").speaker,
        Speaker::System {
            body: Some("Your claude.ai usage limit has reset. 甲".into())
        }
    );
    assert_eq!(queued_text("  往后排，先做乙  "), human("往后排，先做乙"));
    let p = queued_text("<pasted_content id=\"h8\">甲</pasted_content id=\"h8\">");
    assert_eq!((p.speaker.kind(), p.pasted.len()), ("human", 1));
}

#[test]
fn speech_is_what_the_human_said() {
    assert_eq!(human("甲").speech().as_deref(), Some("甲"));
    assert_eq!(human("").speech(), None);
    assert_eq!(
        UserText::of(Speaker::SlashCommand {
            name: "/甲".into(),
            args: String::new()
        })
        .speech()
        .as_deref(),
        Some("/甲")
    );
    assert_eq!(
        UserText::of(Speaker::BashInput {
            command: "ls".into()
        })
        .speech()
        .as_deref(),
        Some("!ls")
    );
    for s in [
        Speaker::CompactSummary,
        Speaker::AgentTask,
        Speaker::System { body: None },
        Speaker::Coordinator { body: None },
        Speaker::Interrupt,
        Speaker::ToolResult,
        Speaker::CommandOutput,
    ] {
        assert_eq!(
            UserText {
                speaker: s.clone(),
                text: "甲".into(),
                pasted: vec![]
            }
            .speech(),
            None,
            "{s:?}"
        );
    }
}

#[test]
fn task_notice_reads_the_first_frame() {
    assert_eq!(
        task_notice("甲 <task-notification><task-id>x</task-id></task-notification>"),
        None
    );
    assert_eq!(
        task_notice("\u{feff}  <task-notification><task-id> x </task-id><status>completed</status></task-notification><task-notification><task-id>y</task-id></task-notification>"),
        Some(Notice {
            task_id: Some("x".into()),
            status: Some("completed".into()),
            summary: None,
            tool_use_id: None
        })
    );
}

/// 斜杠命令与 `!` 输入 / 输出的解析（原先住界面，随「谁说的」搬进来）：标签内容是实体转义过的、斜杠三标签顺序随版本漂、
/// 多出一点别的字就整体回退成人话（不吞内容）。
#[test]
fn slash_and_bash_forms() {
    let k = |t: &str| said(&rec(t, json!({}))).speaker;
    let bi = |c: &str| Speaker::BashInput { command: c.into() };
    assert_eq!(k("<bash-input> sudo 甲 -y</bash-input>"), bi("sudo 甲 -y"));
    assert_eq!(
        k("<bash-input>echo 甲 2&gt;&amp;1</bash-input>"),
        bi("echo 甲 2>&1")
    );
    for t in [
        "普通消息提到 <bash-input> 这个词",
        "<bash-input>ls</bash-input> 以及别的话",
        "<bash-input></bash-input>",
        "<bash-input>   </bash-input>",
        "<bash-stdout>x</bash-stdout>还有别的",
        "<command-name>/a</command-name><command-name>/b</command-name>",
        "<command-message>x</command-message>",
        "<command-name></command-name><command-message>x</command-message>",
    ] {
        assert_eq!(k(t), Speaker::Human, "{t}");
    }
    let bo = |o: &str, e: &str| Speaker::BashOutput {
        stdout: o.into(),
        stderr: e.into(),
    };
    assert_eq!(k("<bash-stdout>甲\n\n乙</bash-stdout>"), bo("甲\n\n乙", ""));
    assert_eq!(
        k("<bash-stdout></bash-stdout><bash-stderr>丙</bash-stderr>"),
        bo("", "丙")
    );
    assert_eq!(k("<bash-stderr>丁</bash-stderr>"), bo("", "丁"));
    assert_eq!(
        k("<bash-stdout>&gt; 甲\n乙</bash-stdout>"),
        bo("> 甲\n乙", "")
    );
    // 单趟解码：字面的 `&lt;` 不会被解两次。
    assert_eq!(
        k("<bash-stdout>&amp;lt; a &lt; b &gt; c &quot;d&quot; &#39;e&#39; &amp;&amp;</bash-stdout>"),
        bo("&lt; a < b > c \"d\" 'e' &&", "")
    );
    let sc = |n: &str, a: &str| Speaker::SlashCommand {
        name: n.into(),
        args: a.into(),
    };
    assert_eq!(k("<command-name>/甲</command-name>\n<command-message>甲</command-message>\n<command-args></command-args>"), sc("/甲", ""));
    assert_eq!(
        k("<command-message>乙</command-message><command-name>/乙</command-name>"),
        sc("/乙", "")
    );
    assert_eq!(k("<command-message>x</command-message><command-name>/丙</command-name><command-args>a &gt; b &amp;&amp; c</command-args>"), sc("/丙", "a > b && c"));
}

/// 骨架索引的 `sp` 用 `Speaker::kind()`，记录成品用 serde 的 `kind`：两边必须是同一个词（界面拿同一张表认它们）。
#[test]
fn the_index_kind_is_the_wire_kind() {
    let all = [
        Speaker::Human,
        Speaker::SlashCommand {
            name: String::new(),
            args: String::new(),
        },
        Speaker::BashInput {
            command: String::new(),
        },
        Speaker::BashOutput {
            stdout: String::new(),
            stderr: String::new(),
        },
        Speaker::CommandOutput,
        Speaker::TaskNotification {
            task_id: None,
            status: None,
            summary: None,
            tool_use_id: None,
        },
        Speaker::AgentMessage {
            from: None,
            name: None,
            handback: false,
            body: None,
        },
        Speaker::PeerSession {
            from: None,
            body: None,
        },
        Speaker::Coordinator { body: None },
        Speaker::AgentTask,
        Speaker::System { body: None },
        Speaker::CompactSummary,
        Speaker::Interrupt,
        Speaker::ToolResult,
    ];
    let mut seen = std::collections::BTreeSet::new();
    for s in &all {
        assert_eq!(serde_json::to_value(s).unwrap()["kind"], s.kind(), "{s:?}");
        assert!(seen.insert(s.kind()), "重名：{}", s.kind());
    }
}

/// 系统注入带着它的原文（`speaker.body`，剥过两头空白）：`isMeta` 的那一条 · 字全是注入的那一条。界面开关开着才画。
#[test]
fn system_injections_carry_their_text() {
    let body = |r: &Value| match said(r).speaker {
        Speaker::System { body } => body,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        body(&rec(" 注入甲 ", json!({"isMeta": true}))).as_deref(),
        Some("注入甲")
    );
    assert_eq!(
        body(&rec("<system-reminder>注入乙</system-reminder>", json!({}))).as_deref(),
        Some("<system-reminder>注入乙</system-reminder>")
    );
}
