//! Claude 一行 ⇒ 通用记录：各类全格 ＋ 最少格（金样 `tests/__fixtures__/record.golden.jsonl`，手机端照它比）· 自动应答按型号认 ·
//! 报错带原因与状态码 · 不进界面的几类不出记录。夹具是结构占位，不采真正文。
//! 骨架行那一条经观测面 ⇒ 整份挂在 `observe` 底下（适配层是独立的包、够不到观测面）。
use crate::agents::claudecode::record_of::*;
use crate::agents::record::{Block, Body, Record};
use crate::agents::record::{BodyExt as _, RecordExt as _};

fn one(raw: &str) -> Option<Record> {
    let rec = crate::agents::claudecode::parse::parse_line(&crate::agents::Line::new(raw, 0))
        .unwrap()
        .unwrap();
    record_of(rec, 4096).map(|mut r| {
        // 钟面随这台的时区变；金样里写成固定的样子（形状是「HH:MM」）。
        if r.time_text.is_some() {
            r.time_text = Some("09:30".into());
        }
        r
    })
}

/// (名, 原文行)：每类全格 ＋ 最少格。
const CASES: &[(&str, &str)] = &[
    (
        "said-full",
        r#"{"type":"user","uuid":"u-full","parentUuid":"p0","timestamp":"2026-10-09T01:30:00.000Z","cwd":"/w/proj","sessionId":"s","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"line a\nline b","is_error":false},{"type":"text","text":"look at this"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]},"toolUseResult":{"filePath":"/w/a.txt","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":2,"lines":["-a","+b","+c"]}]}}"#,
    ),
    (
        "said-min",
        r#"{"type":"user","uuid":"u-min","timestamp":"2026-10-09T01:30:01.000Z","message":{"role":"user","content":"hello"}}"#,
    ),
    (
        "reply-full",
        r#"{"type":"assistant","uuid":"a-full","parentUuid":"u-full","timestamp":"2026-10-09T01:30:02.000Z","requestId":"req-1","message":{"role":"assistant","model":"model-x","stop_reason":"end_turn","usage":{"input_tokens":1},"content":[{"type":"thinking","thinking":"plan it"},{"type":"text","text":"done"},{"type":"tool_use","id":"call-2","name":"Task","input":{"description":"scan","subagent_type":"Explore","prompt":"p"}},{"type":"tool_use","id":"call-3","name":"Edit","input":{"file_path":"/w/a.txt","old_string":"a","new_string":"b"}},{"type":"tool_use","id":"call-4","name":"mcp__x__y","input":{}}]}}"#,
    ),
    (
        "reply-min",
        r#"{"type":"assistant","uuid":"a-min","timestamp":"2026-10-09T01:30:03.000Z","message":{"role":"assistant","content":[{"type":"text","text":"ok"}]}}"#,
    ),
    (
        "reply-error",
        r#"{"type":"assistant","uuid":"a-err","timestamp":"2026-10-09T01:30:04.000Z","isApiErrorMessage":true,"apiErrorStatus":429,"error":"rate_limit","message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"API Error: 429 rate limited"}]}}"#,
    ),
    (
        "reply-auto",
        r#"{"type":"assistant","uuid":"a-auto","timestamp":"2026-10-09T01:30:05.000Z","message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]}}"#,
    ),
    (
        "retry-full",
        r#"{"type":"system","subtype":"api_error","uuid":"r-full","timestamp":"2026-10-09T01:30:06.000Z","level":"error","retryAttempt":2,"maxRetries":10,"error":{"status":529,"error":{"type":"overloaded_error"}}}"#,
    ),
    (
        "retry-min",
        r#"{"type":"system","subtype":"api_error","timestamp":"2026-10-09T01:30:07.000Z"}"#,
    ),
    (
        "title-agent",
        r#"{"type":"ai-title","aiTitle":"Fix the parser","sessionId":"s"}"#,
    ),
    (
        "title-user",
        r#"{"type":"custom-title","customTitle":"my name","sessionId":"s"}"#,
    ),
    (
        // 一轮中途插的一句：它的正式位置是那一行 `attachment queued_command`（照 2.1.303 的键；原话是编的）。
        "queued",
        r#"{"type":"attachment","parentUuid":"a-full","isSidechain":false,"attachment":{"type":"queued_command","prompt":"also do this","source_uuid":"c-1","delivery_id":"d-1","commandMode":"prompt","origin":{"kind":"human"},"timestamp":"2026-10-09T01:30:08.000Z","humanTurn":true},"rendered":[{"content":"x"}],"renderedRole":"system","uuid":"q-1","timestamp":"2026-10-09T01:30:08.000Z","sessionId":"s","cwd":"/w/proj","version":"2.1.303","userType":"external","entrypoint":"cli","gitBranch":"main"}"#,
    ),
    (
        "said-pasted",
        r#"{"type":"user","uuid":"u-said-pasted","timestamp":"2026-10-09T01:31:09.000Z","message":{"role":"user","content":"see <pasted_content id=\"p1\">\nA\nB\n</pasted_content id=\"p1\"> end"}}"#,
    ),
    (
        "said-read",
        r#"{"type":"user","uuid":"u-said-read","timestamp":"2026-10-09T01:31:10.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-4","content":[{"type":"text","text":"x"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}],"is_error":false}]},"toolUseResult":{"file":{"numLines":3},"numFiles":2}}"#,
    ),
    (
        "said-exit",
        r#"{"type":"user","uuid":"u-said-exit","timestamp":"2026-10-09T01:31:11.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-5","content":"Exit code 2\nboom","is_error":true}]}}"#,
    ),
    (
        "said-rejected",
        r#"{"type":"user","uuid":"u-said-rejected","timestamp":"2026-10-09T01:31:12.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-6","content":"User rejected tool use","is_error":false}]}}"#,
    ),
    (
        "said-plan",
        r#"{"type":"user","uuid":"u-said-plan","timestamp":"2026-10-09T01:31:13.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-7","content":"ok","is_error":false}]},"toolUseResult":{"plan":"p"}}"#,
    ),
    (
        "said-picked",
        r#"{"type":"user","uuid":"u-said-picked","timestamp":"2026-10-09T01:31:14.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-8","content":"ok","is_error":false}]},"toolUseResult":{"answers":{"Q":"A"}}}"#,
    ),
    (
        "said-patch-partial",
        r#"{"type":"user","uuid":"u-said-patch-partial","timestamp":"2026-10-09T01:31:15.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-9","content":"ok","is_error":false}]},"toolUseResult":{"filePath":"/w/b.txt","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-a","+b"]},{"lines":["+c"]}]}}"#,
    ),
    (
        "said-slash",
        r#"{"type":"user","uuid":"u-said-slash","timestamp":"2026-10-09T01:31:16.000Z","message":{"role":"user","content":"<command-name>/review</command-name><command-args>x</command-args>"}}"#,
    ),
    (
        "said-bash-input",
        r#"{"type":"user","uuid":"u-said-bash-input","timestamp":"2026-10-09T01:31:17.000Z","message":{"role":"user","content":"<bash-input>ls</bash-input>"}}"#,
    ),
    (
        "said-bash-output",
        r#"{"type":"user","uuid":"u-said-bash-output","timestamp":"2026-10-09T01:31:18.000Z","message":{"role":"user","content":"<bash-stdout>o</bash-stdout><bash-stderr>e</bash-stderr>"}}"#,
    ),
    (
        "said-command-output",
        r#"{"type":"user","uuid":"u-said-command-output","timestamp":"2026-10-09T01:31:19.000Z","message":{"role":"user","content":"<local-command-stdout>done</local-command-stdout>"}}"#,
    ),
    (
        "said-task-notification",
        r#"{"type":"user","uuid":"u-said-task-notification","timestamp":"2026-10-09T01:31:20.000Z","message":{"role":"user","content":"<task-notification><task-id>t1</task-id><tool-use-id>call-1</tool-use-id><status>completed</status><summary>finished</summary></task-notification>"},"origin":{"kind":"task-notification"}}"#,
    ),
    (
        "said-agent-message",
        r#"{"type":"user","uuid":"u-said-agent-message","timestamp":"2026-10-09T01:31:21.000Z","message":{"role":"user","content":"<agent-message from=\"w1\">report</agent-message>"},"origin":{"kind":"peer","from":"w1","name":"worker","handback":true,"body":"report"}}"#,
    ),
    (
        "said-peer-session",
        r#"{"type":"user","uuid":"u-said-peer-session","timestamp":"2026-10-09T01:31:22.000Z","message":{"role":"user","content":"<cross-session-message from=\"s2\">hi</cross-session-message>"},"origin":{"kind":"peer","from":"s2","body":"hi"}}"#,
    ),
    (
        "said-coordinator",
        r#"{"type":"user","uuid":"u-said-coordinator","timestamp":"2026-10-09T01:31:23.000Z","message":{"role":"user","content":"x"},"origin":{"kind":"coordinator","body":"check"}}"#,
    ),
    (
        "said-agent-task",
        r#"{"type":"user","uuid":"u-said-agent-task","timestamp":"2026-10-09T01:31:24.000Z","message":{"role":"user","content":"go scan"},"isSidechain":true}"#,
    ),
    (
        "said-system",
        r#"{"type":"user","uuid":"u-said-system","timestamp":"2026-10-09T01:31:25.000Z","message":{"role":"user","content":"<system-reminder>r</system-reminder>"},"isMeta":true}"#,
    ),
    (
        "said-compact-summary",
        r#"{"type":"user","uuid":"u-said-compact-summary","timestamp":"2026-10-09T01:31:26.000Z","message":{"role":"user","content":"This session is being continued from a previous conversation. Summary."},"isCompactSummary":true}"#,
    ),
    (
        "said-interrupt",
        r#"{"type":"user","uuid":"u-said-interrupt","timestamp":"2026-10-09T01:31:27.000Z","message":{"role":"user","content":"[Request interrupted by user]"}}"#,
    ),
    (
        "reply-step-note",
        r#"{"type":"assistant","uuid":"a-note","timestamp":"2026-10-09T01:31:28.000Z","message":{"role":"assistant","model":"model-x","content":[{"type":"tool_use","id":"call-10","name":"Bash","input":{"command":"ls","description":"list"}}]}}"#,
    ),
    (
        "queued-pasted",
        r#"{"type":"attachment","parentUuid":"a-note","isSidechain":false,"attachment":{"type":"queued_command","prompt":"see <pasted_content id=\"p2\">\nA\n</pasted_content id=\"p2\">","source_uuid":"c-2","delivery_id":"d-2","commandMode":"prompt","origin":{"kind":"human"},"timestamp":"2026-10-09T01:31:29.000Z","humanTurn":true},"uuid":"q-2","timestamp":"2026-10-09T01:31:29.000Z","sessionId":"s"}"#,
    ),
    (
        "unread-unknown",
        r#"{"type":"brand-new-kind","uuid":"n-1","timestamp":"2026-10-09T01:31:30.000Z","x":1}"#,
    ),
    (
        "unread-parse-failed",
        r#"{"type":"ai-title","sessionId":"s"}"#,
    ),
    (
        "reply-ask",
        r#"{"type":"assistant","uuid":"a-ask","timestamp":"2026-10-09T01:30:02.500Z","message":{"role":"assistant","model":"model-x","content":[{"type":"tool_use","id":"call-ask","name":"AskUserQuestion","input":{"questions":[{"header":"h","question":"q?","multiSelect":true,"options":[{"label":"a","description":"d"},{"label":"b"}]}]}},{"type":"tool_use","id":"call-plan","name":"ExitPlanMode","input":{"plan":"1. p"}}]}}"#,
    ),
    // 上面那一问一计划各自答了（截图台架的提问卡 / 计划卡照这三条画，不另造）。
    (
        "said-ask-answered",
        r#"{"type":"user","uuid":"u-ask-answered","timestamp":"2026-10-09T01:30:02.600Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-ask","content":"User has answered your questions: \"q?\"=\"a\". You can now continue with the user's answers in mind.","is_error":false}]},"toolUseResult":{"answers":{"q?":"a"}}}"#,
    ),
    // 后台派出子 agent 那次调用当场拿到的回执（`async_launched`）：结果那一格说它是回执（`kind`），不是交回。
    (
        "said-agent-launched",
        r#"{"type":"user","uuid":"u-agent-launched","timestamp":"2026-10-09T01:30:02.800Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-agent","content":[{"type":"text","text":"Async agent launched successfully."}],"is_error":false}]},"toolUseResult":{"status":"async_launched","agentId":"w1"}}"#,
    ),
    (
        "said-plan-approved",
        r#"{"type":"user","uuid":"u-plan-approved","timestamp":"2026-10-09T01:30:02.700Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-plan","content":"User has approved your plan. You can now start coding.","is_error":false}]},"toolUseResult":{"plan":"1. p"}}"#,
    ),
];

/// 改前改后行数之积刚过逐行比的格数上界的一次改动（改前 2001 行空行 · 改后 2000 行 `x`）：行差出整删 ＋ 整增、带降级那一句。
/// 原文在这里现拼（太长，不进 [`CASES`]）。
fn coarse_edit() -> String {
    let input = serde_json::json!({
        "file_path": "/w/big.txt",
        "old_string": "\n".repeat(2001),
        "new_string": "x\n".repeat(2000),
    });
    serde_json::json!({
        "type": "assistant", "uuid": "a-coarse", "timestamp": "2026-10-09T01:30:03.000Z",
        "message": {"role": "assistant", "model": "model-x",
                    "content": [{"type": "tool_use", "id": "call-coarse", "name": "Edit", "input": input}]}
    })
    .to_string()
}

#[test]
fn every_kind_matches_the_golden_full_and_minimal() {
    let coarse = coarse_edit();
    let got: String = CASES
        .iter()
        .copied()
        .chain([("reply-coarse-diff", coarse.as_str())])
        .map(|(name, raw)| {
            // 金样是线上那一形：出口那一下按看的那一台的时区写好钟面（这里按 UTC）。
            let mut r = one(raw).unwrap_or_else(|| panic!("{name} 没出记录"));
            r.stamp(&crate::common::time::Tz::default());
            format!(
                "{}\n",
                serde_json::to_string(&serde_json::json!({ "case": name, "record": r })).unwrap()
            )
        })
        .collect();
    crate::golden::assert_golden(
        "tests/__fixtures__/record.golden.jsonl",
        &got,
        "通用记录与金样不一致",
    );
}

/// ★ 骨架行的类 · 画法 · 人说没说就是整条翻译给的那一条的（骨架行从通用记录取，不另判）：金样每一条 · 不进界面的几类 ·
/// 各条去掉任意一格顶层字段（必填格缺了盘上形状就认不出、不出记录）· 秤的原文语料（`scale2-height-records.jsonl`）每一行。
#[test]
fn the_skeleton_row_is_what_the_record_says() {
    let root = guard_core::addr::repo_root();
    let scale =
        std::fs::read_to_string(root.join("tests/__fixtures__/scale2-height-records.jsonl"))
            .unwrap();
    let mut lines: Vec<String> = CASES.iter().map(|(_, raw)| raw.to_string()).collect();
    lines.extend(
        [
            r#"{"type":"attachment","uuid":"x","timestamp":"t","parentUuid":"p"}"#,
            r#"{"type":"system","subtype":"local_command","uuid":"x","timestamp":"t"}"#,
            r#"{"type":"permission-mode","permissionMode":"plan","sessionId":"s"}"#,
            r#"{"type":"queue-operation","operation":"enqueue","content":"x","timestamp":"t"}"#,
            r#"{"type":"queue-operation","operation":"remove","content":"<bash-input>ls</bash-input>"}"#,
            r#"{"type":"some-new-kind","uuid":"x","timestamp":"t"}"#,
        ]
        .map(str::to_string),
    );
    for (_, raw) in CASES {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        for k in v.as_object().unwrap().keys().filter(|k| *k != "type") {
            let mut cut = v.clone();
            cut.as_object_mut().unwrap().remove(k);
            lines.push(cut.to_string());
        }
    }
    lines.extend(
        scale
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string),
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut drawn = std::collections::BTreeSet::new();
    for raw in &lines {
        let rec = crate::agents::claudecode::parse::parse_line(&crate::agents::Line::new(raw, 0))
            .unwrap()
            .and_then(|r| record_of(r, 0));
        let want = rec
            .as_ref()
            .map_or((None, crate::agents::record::Draw::None, false), |r| {
                (Some(r.body.class()), r.draw, r.human)
            });
        let row = crate::observe::recognize::index_row::test_support::index_row(
            crate::agents::claudecode::AGENT_KIND,
            raw.as_bytes(),
            0,
            raw.len() as u64 + 1,
        );
        assert_eq!((row.t, row.dr, row.hu), want, "{raw}");
        seen.insert(format!("{:?}", want.0));
        drawn.insert(format!("{:?}", want.1));
    }
    assert_eq!(drawn.len(), 4, "四形都得见到：{drawn:?}");
    // 这一家不再出 `queued`（插话是那一行 attachment 出的 `said`）：五类 ＋ 不出记录。
    assert_eq!(seen.len(), 6, "五类 ＋ 不出记录都得见到：{seen:?}");
}

/// 一轮中途插话那一行 `attachment queued_command`（2.1.273 起每一句人插的话都配着它）：人说的 ⇒ 一条 `said`，
/// 身份 ＝ 它自己的 uuid、时刻 ＝ 它自己的（就是打字那一刻），谁说的按 `origin.kind` 认；没有 `origin` 的旧版按原话认。
#[test]
fn a_mid_turn_line_from_the_person_is_said_where_the_record_puts_it() {
    let r = one(CASES.iter().find(|(n, _)| *n == "queued").unwrap().1).expect("人插的话要出一条");
    assert_eq!(r.id, "q-1");
    assert_eq!(r.at.as_deref(), Some("2026-10-09T01:30:08.000Z"));
    let Body::Said { who, blocks, .. } = &r.body else {
        panic!("{r:?}")
    };
    assert!(
        matches!(who.speaker, crate::agents::Speaker::Human),
        "{who:?}"
    );
    assert_eq!(who.text, "also do this");
    assert_eq!(blocks, &vec![Block::text("also do this".into())]);
    // 旧版：没有 `origin`，原话不是哪种框 ⇒ 人说的。
    let old = r#"{"type":"attachment","parentUuid":"a1","attachment":{"type":"queued_command","prompt":"later please","commandMode":"prompt"},"uuid":"q-3","timestamp":"2026-10-09T01:30:09.000Z"}"#;
    assert!(matches!(one(old).map(|r| r.body), Some(Body::Said { .. })));
}

/// 思考块只有签名、`thinking` 是空串或只剩空白（近来的记录几乎都这样）⇒ 交出 `text: ""`：线上约定「空 ＝ 没写进记录」，
/// 「有没有能看的」只在这里判一次，两端照它画、不各自去空白。有字的照原样。
#[test]
fn an_empty_thinking_block_says_it_was_not_recorded() {
    let raw = r#"{"type":"assistant","uuid":"a-th","timestamp":"2026-10-09T01:30:02.000Z","message":{"role":"assistant","model":"model-x","content":[{"type":"thinking","thinking":"","signature":"sig-1"},{"type":"thinking","thinking":" \n\t ","signature":"sig-2"},{"type":"thinking","thinking":"  plan it ","signature":"sig-3"},{"type":"thinking","signature":"sig-4"}]}}"#;
    let Body::Reply { blocks, .. } = one(raw).unwrap().body else {
        panic!()
    };
    let texts: Vec<&str> = blocks
        .iter()
        .map(|b| match b {
            Block::Thinking { text, .. } => text.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(texts, ["", "", "  plan it ", ""]);
}

/// 收着时那一行由后端写好（`preview`）：头一个非空行、去两头空白、至多 160 字（截了接「…」）；没写进记录 ⇒ 空串。
#[test]
fn a_thinking_block_carries_its_collapsed_line() {
    let long = "想".repeat(170);
    let raw = format!(
        r#"{{"type":"assistant","uuid":"a-tp","timestamp":"2026-10-09T01:30:02.000Z","message":{{"role":"assistant","model":"model-x","content":[{{"type":"thinking","thinking":"\n  先看一眼  \n再说","signature":"s"}},{{"type":"thinking","thinking":"{long}","signature":"s"}},{{"type":"thinking","thinking":" ","signature":"s"}}]}}}}"#
    );
    let Body::Reply { blocks, .. } = one(&raw).unwrap().body else {
        panic!()
    };
    let previews: Vec<String> = blocks
        .iter()
        .map(|b| match b {
            Block::Thinking { preview, .. } => preview.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        previews,
        [
            "先看一眼".to_string(),
            format!("{}…", "想".repeat(160)),
            String::new()
        ]
    );
}

/// 同一形的别人那几种（后台任务收场 · peer 交回）不出卡：它们照旧由自己那一形出（或不出），不当成人说的。
/// `remove` 那一行不再出记录（同一句有 attachment 那一行，再出一条就上屏两次）。
#[test]
fn others_mid_turn_lines_and_the_remove_line_stay_off_screen() {
    for raw in [
        r#"{"type":"attachment","parentUuid":"a1","attachment":{"type":"queued_command","prompt":"<task-notification>\n<task-id>t1</task-id>\n<status>completed</status>\n</task-notification>","commandMode":"task-notification","origin":{"kind":"task-notification","producer":"x"},"runId":"r1"},"uuid":"q-4","timestamp":"t"}"#,
        r#"{"type":"attachment","parentUuid":"a1","attachment":{"type":"queued_command","prompt":"<agent-message from=\"w1\">\nx\n</agent-message>","commandMode":"prompt","isMeta":true,"origin":{"kind":"peer","from":"w1","name":"w","body":"x","senderTaskId":"k"}},"uuid":"q-5","timestamp":"t"}"#,
        r#"{"type":"attachment","parentUuid":"a1","attachment":{"type":"queued_command","prompt":"<task-notification>\n<task-id>t2</task-id>\n</task-notification>"},"uuid":"q-6","timestamp":"t"}"#,
        r#"{"type":"queue-operation","operation":"remove","timestamp":"2026-10-09T01:30:10.000Z","sessionId":"s","content":"also do this","reason":"absorbed_mid_turn","commandUuid":"c-1","deliveryId":"d-1"}"#,
        r#"{"type":"queue-operation","operation":"remove","timestamp":"2026-10-09T01:30:10.000Z","sessionId":"s","content":"also do this","reason":"absorbed_mid_turn"}"#,
    ] {
        assert!(one(raw).is_none(), "{raw}");
    }
}

#[test]
fn an_auto_reply_is_known_by_its_model_not_its_text() {
    let Body::Reply {
        auto_reply,
        model,
        error,
        ..
    } = one(CASES[5].1).unwrap().body
    else {
        panic!()
    };
    assert!(auto_reply && model.is_none() && error.is_none());
    // 报错那条也写着同一个型号名，但它是报错、不是自动应答。
    let Body::Reply {
        auto_reply, error, ..
    } = one(CASES[4].1).unwrap().body
    else {
        panic!()
    };
    assert!(!auto_reply);
    assert_eq!(error.map(|e| e.status), Some(Some(429)));
}

#[test]
fn records_without_a_reader_do_not_come_out() {
    for raw in [
        r#"{"type":"attachment","uuid":"x","timestamp":"t","parentUuid":"p"}"#,
        r#"{"type":"system","subtype":"local_command","uuid":"x","timestamp":"t"}"#,
        r#"{"type":"permission-mode","permissionMode":"plan","sessionId":"s"}"#,
        r#"{"type":"queue-operation","operation":"enqueue","content":"x","timestamp":"t"}"#,
    ] {
        let rec = crate::agents::claudecode::parse::parse_line(&crate::agents::Line::new(raw, 0))
            .unwrap()
            .unwrap();
        assert!(record_of(rec, 0).is_none(), "{raw}");
    }
}

/// Claude 自己的状态行（07-16 语料里落在「未知类型」的那 7 种 · 10-14 语料里 Claude Code 2.1.29x 新写的 6 种）：认识、不上屏 ——
/// 不出记录，也不再记成漂移。10-14 那 6 种照真行的键名造（值全是占位；`bridge-session` 的账号 / 组织号用全零假值）。
#[test]
fn claude_status_lines_are_known_and_stay_off_screen() {
    let mut raws: Vec<String> = [
        "mode",
        "agent-name",
        "file-history-delta",
        "pr-link",
        "relocated",
        "worktree-state",
        "frame-link",
    ]
    .iter()
    .map(|t| format!(r#"{{"type":"{t}","sessionId":"s","x":1}}"#))
    .collect();
    raws.extend(
        [
            r#"{"type":"atis-latch","atis":"","sessionId":"s"}"#,
            r#"{"type":"history-suppression","sessionId":"s","cause":"migration","ts":"2026-10-14T00:00:00.000Z"}"#,
            r#"{"type":"bridge-session","sessionId":"s","bridgeSessionId":"cse_placeholder","lastSequenceNum":1,"noHistoryBackfill":true,"ownerAccountUuid":"00000000-0000-0000-0000-000000000000","ownerOrganizationUuid":"00000000-0000-0000-0000-000000000000"}"#,
            r#"{"type":"cost-state","sessionId":"s","totalCostUsd":0.5}"#,
            r#"{"type":"artifact-comment-monitor","sessionId":"s","x":1}"#,
            r#"{"type":"artifact-autoreact-ledger","sessionId":"s","x":1}"#,
        ]
        .map(str::to_string),
    );
    for raw in raws {
        let t = serde_json::from_str::<serde_json::Value>(&raw).unwrap()["type"]
            .as_str()
            .unwrap()
            .to_string();
        let rec = crate::agents::claudecode::parse::parse_line(&crate::agents::Line::new(&raw, 0))
            .unwrap()
            .unwrap();
        assert!(
            !matches!(
                rec,
                crate::agents::claudecode::schema::JsonlRecord::Unrecognized { .. }
            ),
            "{t} 还落在认不出那一支"
        );
        assert!(record_of(rec, 0).is_none(), "{t}");
    }
}

/// 认不出的一行出 `unread`：没见过的类型 · 见过的类型形状变了；写好的一句、`warn`、原文摘录按字符截到 400 字节以内。
#[test]
fn an_unread_line_comes_out_as_unread() {
    use crate::agents::record::UnreadWhy;
    use cmd_wire::record::EXCERPT_BYTES;
    let r = one(r#"{"type":"some-new-kind","uuid":"x","timestamp":"2026-10-09T01:30:00.000Z"}"#)
        .expect("没见过的类型也要出一条");
    let Body::Unread {
        why,
        kind,
        text,
        tone,
        excerpt,
        count,
    } = &r.body
    else {
        panic!("{r:?}")
    };
    assert_eq!(*why, UnreadWhy::UnknownType);
    assert_eq!(kind.as_deref(), Some("some-new-kind"));
    assert!(text.0.contains("some-new-kind"), "{text:?}");
    assert_eq!(*tone, crate::common::cells::Tone::Warn);
    assert!(excerpt.0.contains("some-new-kind"));
    assert_eq!(*count, 1);
    assert_eq!(r.id, "x");
    assert!(r.at.is_some());

    // 已知类型、必填格没了 ⇒ 形状变了。
    let r = one(r#"{"type":"ai-title","sessionId":"s"}"#).expect("形状变了也要出一条");
    assert!(
        matches!(
            r.body,
            Body::Unread {
                why: UnreadWhy::ParseFailed,
                ..
            }
        ),
        "{r:?}"
    );

    // 长行：按字符截，不截半个字。
    let long = format!(
        r#"{{"type":"some-new-kind","uuid":"x","pad":"{}"}}"#,
        "字".repeat(400)
    );
    let r = one(&long).unwrap();
    let Body::Unread { excerpt, .. } = &r.body else {
        panic!()
    };
    assert!(
        excerpt.0.len() <= EXCERPT_BYTES && excerpt.0.len() > EXCERPT_BYTES - 4,
        "{}",
        excerpt.0.len()
    );
    assert!(long.starts_with(&excerpt.0));
}

/// **没有链上身份的未知顶层类型不成卡**（10-14 起的规矩）：对话内容每行都带 `uuid` / `parentUuid`，没身份的只可能是那一家的簿记行 ——
/// 下一版再加几种也不刷屏。同一份记录里：没身份的新类型 ⇒ 不出记录；带身份的新类型 ⇒ 照旧 `unread`；
/// 认识的类型形状变了（没身份也算）⇒ 照旧 `unread`。不悄悄丢的那一半（会话事实里记几行、哪几种）在 `recognize::facts::tests`。
#[test]
fn a_bookkeeping_line_without_identity_is_no_card() {
    let lines = [
        (
            r#"{"type":"some-future-ledger","sessionId":"s","n":1}"#,
            false,
        ),
        (
            r#"{"type":"some-future-kind","uuid":"u-1","parentUuid":"p-1","timestamp":"2026-10-14T00:00:00.000Z"}"#,
            true,
        ),
        (r#"{"type":"some-future-kind","parentUuid":"p-2"}"#, true),
        (r#"{"type":"ai-title","sessionId":"s"}"#, true),
    ];
    for (raw, card) in lines {
        let t = crate::agents::claudecode::parse::translated(&crate::agents::Line::new(raw, 0))
            .unwrap()
            .expect("合法 JSON 的一行都有一份翻译");
        assert_eq!(
            matches!(
                t.record.as_ref().map(|r| &r.body),
                Some(Body::Unread { .. })
            ),
            card,
            "{raw}: {t:?}"
        );
    }
    // 判只一处：会话事实记数那一问（[`quiet_unread_of`]）与翻译表不出卡逐行一致 —— 认不出的行里，不成卡 ⇔ 它记数；
    // 认识的类型（有没有身份都）不记数。
    let more = [
        r#"{"type":"mode","sessionId":"s"}"#,
        r#"{"type":"atis-latch","atis":"","sessionId":"s"}"#,
        r#"{"type":"user","uuid":"u","timestamp":"t","message":{"role":"user","content":"q"}}"#,
        r#"{"type":"some-future-ledger","uuid":7}"#,
        r#"{"sessionId":"s"}"#,
    ];
    for raw in lines.iter().map(|(r, _)| *r).chain(more) {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let line = crate::agents::Line::new(raw, 0);
        let rec = crate::agents::claudecode::parse::parse_line(&line)
            .unwrap()
            .unwrap();
        let unrecognized = matches!(
            rec,
            crate::agents::claudecode::schema::JsonlRecord::Unrecognized { .. }
        );
        let carded = record_of(rec, 0).is_some();
        let quiet = quiet_unread_of(&v);
        assert_eq!(quiet.is_some(), unrecognized && !carded, "{raw}");
        if let Some(t) = quiet {
            assert_eq!(Some(t.as_str()), v["type"].as_str(), "{raw}");
        }
    }
}

/// 秤 1 / 2 / 6 喂界面的语料就是这一家翻译表对原文语料（`scale2-height-records.jsonl`）逐行的成品 ——
/// 界面那边的秤量的是后端真会给的东西，不是手改的形状。钟面随时区变 ⇒ 都不带 `timeText`。
/// 秤 1 的「膨胀」那一份也在这里出：膨胀的是**原文**（每个 ≥ [`INFLATE_MIN_LEN`] 字的串尾接换行 ＋ 填充），
/// 再过翻译表 ⇒ 后端从原文算出的那几格（过程一行的主参数 · 结果几行）照真的变，界面那边不再自己挑哪格不膨胀。
/// 膨胀：多长的串才接填充（短串 —— id · 时刻 · 类型 · 型号 —— 不动）。
const INFLATE_MIN_LEN: usize = 64;
/// 膨胀：每个够长的串尾部接多少个填充字。
const INFLATE_CHARS: usize = 1024;

fn inflate(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) if s.chars().count() >= INFLATE_MIN_LEN => {
            s.push('\n');
            s.push_str(&"膨".repeat(INFLATE_CHARS));
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(inflate),
        serde_json::Value::Object(o) => o.values_mut().for_each(inflate),
        _ => {}
    }
}

/// 原文语料（可先膨胀）⇒ 逐行成品，一行一条。
fn scale_records(inflated: bool) -> String {
    let root = guard_core::addr::repo_root();
    let raw = std::fs::read_to_string(root.join("tests/__fixtures__/scale2-height-records.jsonl"))
        .unwrap();
    let mut got = String::new();
    let mut off = 0u64;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let line = if inflated {
            let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
            inflate(&mut v);
            serde_json::to_string(&v).unwrap()
        } else {
            line.to_string()
        };
        let start = off;
        off += line.len() as u64 + 1;
        let Some(rec) =
            crate::agents::claudecode::parse::parse_line(&crate::agents::Line::new(&line, 0))
                .unwrap()
        else {
            continue;
        };
        if let Some(mut r) = record_of(rec, start) {
            r.time_text = None;
            got.push_str(&serde_json::to_string(&r).unwrap());
            got.push('\n');
        }
    }
    got
}

#[test]
fn the_scale_corpora_are_what_this_translator_gives() {
    let wrong: Vec<String> = [
        ("scale2-height-line-records.jsonl", false),
        ("scale1-inflated-line-records.jsonl", true),
    ]
    .into_iter()
    .filter_map(|(name, inflated)| {
        crate::golden::mismatch(
            &format!("tests/__fixtures__/{name}"),
            &scale_records(inflated),
        )
    })
    .collect();
    assert!(
        wrong.is_empty(),
        "秤的语料与翻译表现打的不一致（重打后回界面那几杆秤改登记）：\n{}",
        wrong.join("\n")
    );
}

// ── 人说的那一条里认附件 ────────────────────────────────────

/// 一条带一张图（开头 `[Image #1]` ＋ 图片块）、正文后面一行 `@.ccm-attach/…` 的人说的话；`cwd` 是那份附件所在的工作目录。
fn said_with_attachments(cwd: &str) -> String {
    serde_json::json!({
        "type": "user", "uuid": "u-att", "timestamp": "2026-10-23T07:31:00.000Z", "cwd": cwd,
        "origin": {"kind": "human"}, "imagePasteIds": [1],
        "message": {"role": "user", "content": [
            {"type": "text", "text": "[Image #1] 截图在这\n@.ccm-attach/1023-1529-7c01/login.css"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
        ]}
    })
    .to_string()
}

/// ★ 正文去掉 `[Image #N]` 与引用行，引用收进 `attachments`（名 · 相对路径 · 大小）；图照旧是图片块。那一份已不在 ⇒ `gone`。
#[test]
fn a_human_say_lists_its_attachments_and_keeps_only_the_words() {
    let d = std::env::temp_dir().join(format!("ccm-rec-att-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(d.join(".ccm-attach/1023-1529-7c01")).expect("建");
    std::fs::write(
        d.join(".ccm-attach/1023-1529-7c01/login.css"),
        b"0123456789",
    )
    .expect("写");
    let cwd = d.display().to_string();
    let Some(Record {
        body:
            Body::Said {
                who,
                blocks,
                attachments,
                ..
            },
        ..
    }) = one(&said_with_attachments(&cwd))
    else {
        panic!("该是人说的一条")
    };
    assert_eq!(who.text, "截图在这");
    assert!(matches!(blocks.last(), Some(Block::Image { .. })));
    assert_eq!(attachments.len(), 1);
    let a = &attachments[0];
    assert_eq!(
        (a.name.as_str(), a.rel.as_str(), a.bytes, a.gone),
        (
            "login.css",
            ".ccm-attach/1023-1529-7c01/login.css",
            Some(10),
            false
        )
    );
    assert!(a.size_text.is_some());
    // 类别照名字判（与 `session-attachment-commit` 回包同一个 `class_of_name`）。
    assert_eq!(a.class, cmd_wire::files::FileClass::Code);
    assert_eq!(
        a.class_text,
        copy_core::copy_text("beFilesKind.label.code", &[])
    );
    std::fs::remove_dir_all(&d).ok();
    let Some(Record {
        body: Body::Said { attachments, .. },
        ..
    }) = one(&said_with_attachments(&cwd))
    else {
        panic!("该是人说的一条")
    };
    assert!(attachments[0].gone && attachments[0].bytes.is_none());
}

/// 不指进 `.ccm-attach/` 的 `@` 行是人自己写的，不动；不带图片块时 `[Image #1]` 那几个字也不动。
#[test]
fn ordinary_mentions_and_bracket_words_stay_in_the_text() {
    let raw = serde_json::json!({
        "type": "user", "uuid": "u-plain", "timestamp": "2026-10-23T07:31:00.000Z", "cwd": "/w",
        "origin": {"kind": "human"},
        "message": {"role": "user", "content": "[Image #1] 看看\n@src/main.rs"}
    })
    .to_string();
    let Some(Record {
        body: Body::Said {
            who, attachments, ..
        },
        ..
    }) = one(&raw)
    else {
        panic!("该是人说的一条")
    };
    assert_eq!(who.text, "[Image #1] 看看\n@src/main.rs");
    assert!(attachments.is_empty());
}

/// ★ 图那一块翻成引用：类型 · 原图字节数 · 那一行的起点 · 这一行里第几张（深度优先，工具结果里的也数）；字节留在块里、不上线。
/// 来源不是内联 base64 图的那一块不出块（出口不再判画不画得出）。
#[test]
fn an_image_becomes_a_reference_to_its_line_and_place() {
    let raw = serde_json::json!({
        "type": "user", "uuid": "u-img", "timestamp": "2026-10-09T01:30:00.000Z", "cwd": "/w",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "c1", "content": [
                {"type": "text", "text": "shot"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAAAA=="}}
            ]},
            {"type": "image", "source": {"type": "url", "url": "https://example.invalid/a.png"}},
            {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "AAAA"}}
        ]}
    })
    .to_string();
    let Some(rec) = one(&raw) else {
        panic!("该出一条")
    };
    let Body::Said { ref blocks, .. } = rec.body else {
        panic!("该是人说的一条")
    };
    let inner = match &blocks[0] {
        Block::ToolResult { content, .. } => content[1].clone(),
        b => panic!("头一块该是工具结果：{b:?}"),
    };
    assert_eq!(
        inner,
        Block::Image {
            media_type: "image/png".into(),
            bytes: 4,
            at: 4096,
            n: 0,
            data: "AAAAAA==".into()
        }
    );
    assert_eq!(blocks.len(), 2, "url 那一形不出块：{blocks:?}");
    assert_eq!(
        blocks[1],
        Block::Image {
            media_type: "image/jpeg".into(),
            bytes: 3,
            at: 4096,
            n: 1,
            data: "AAAA".into()
        }
    );
    let wire = serde_json::to_string(&rec).unwrap();
    assert!(!wire.contains("AAAA"), "图的字节不上线：{wire}");
}

/// ★ 正文块照原文 UTF-8 字节写好「展开 {大小}」那一格（两端照放，不插值）· 工具结果块带着多少字节（内容各块之和）：不拿字符数当字节。
#[test]
fn text_and_result_blocks_carry_their_utf8_bytes() {
    let han = "字".repeat(1000);
    let reply = format!(
        r#"{{"type":"assistant","uuid":"b-1","timestamp":"2026-10-09T01:30:08.000Z","message":{{"role":"assistant","model":"m","content":[{{"type":"text","text":"{han}"}}]}}}}"#
    );
    let r = one(&reply).expect("出一条回复");
    let Body::Reply { blocks, .. } = &r.body else {
        panic!("{r:?}")
    };
    let want = copy_core::copy_text("cards.text.showAll", &[("size", &copy_core::size_text(3000))]);
    assert!(
        matches!(&blocks[0], Block::Prose { more, .. } if more.0 == want),
        "{:?}",
        blocks[0]
    );
    let said = format!(
        r#"{{"type":"user","uuid":"b-2","timestamp":"2026-10-09T01:30:09.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"c","content":[{{"type":"text","text":"{han}"}},{{"type":"text","text":"ab"}}]}}]}}}}"#
    );
    let r = one(&said).expect("出一条结果");
    let Body::Said { blocks, .. } = &r.body else {
        panic!("{r:?}")
    };
    assert!(
        matches!(blocks[0], Block::ToolResult { bytes: 3002, .. }),
        "{:?}",
        blocks[0]
    );
}
