//! 过程一步一行（`steps.rs`）：主参数与说明 · 结果一句 · 提问 / 计划答了什么 · 报错原因。夹具只造结构（占位路径与词），不采会话正文。

use super::*;
use crate::agents::claudecode::parse::parse_line;
use crate::agents::claudecode::schema::JsonlRecord;
use crate::common::cells::Words;
use serde_json::json;

#[test]
fn a_step_is_tool_main_arg_and_note() {
    let s = step_of(
        "Bash",
        &json!({"command": "npm  run\nbuild", "description": "构建"}),
    );
    assert_eq!(
        (s.arg.as_deref(), s.note.as_deref(), s.path, s.known),
        (Some("npm run build"), Some("构建"), false, true)
    );
    let s = step_of(
        "Edit",
        &json!({"file_path": "/w/a.ts", "old_string": "x", "new_string": "y"}),
    );
    assert_eq!(
        (s.arg.as_deref(), s.path, s.note),
        (Some("/w/a.ts"), true, None)
    );
    // 派 agent：说明就是主参数，不再出一遍说明。
    let s = step_of("Agent", &json!({"description": "查甲", "prompt": "乙"}));
    assert_eq!((s.arg.as_deref(), s.note), (Some("查甲"), None));
    // 认得、没有主参数的；认不出的（MCP 等）⇒ `known: false`，不从入参里瞎挑。
    assert!(step_of("TodoWrite", &json!({"todos": []})).known);
    let s = step_of("mcp__x__y", &json!({"q": "甲"}));
    assert_eq!((s.known, s.arg), (false, None));
    // 主参数一行、截长。
    let long = "a".repeat(ARG_MAX + 50);
    assert!(step_of("Grep", &json!({"pattern": long}))
        .arg
        .unwrap()
        .ends_with('…'));
}

/// 主参数是协议上的定长一行：至多 200 字（按字符，不切半个字），截了带省略号；不到 200 原样、不带省略号。
/// 两个前端都不再截（完整内容在卡片里）。
#[test]
fn the_main_arg_is_one_line_of_at_most_200_chars() {
    let arg = |s: &str| step_of("Bash", &json!({ "command": s })).arg.unwrap();
    let wide = "汉".repeat(500);
    let got = arg(&wide);
    assert_eq!(got.chars().count(), 201);
    assert_eq!(got, format!("{}…", "汉".repeat(200)));
    let exact = "字".repeat(200);
    assert_eq!(arg(&exact), exact);
    assert_eq!(arg("a\n  b\tc"), "a b c");
    let mixed = format!("{}{}", "x".repeat(199), "😀😀😀");
    assert_eq!(arg(&mixed), format!("{}😀…", "x".repeat(199)));
}

#[test]
fn a_result_reads_counts_from_the_structured_result() {
    let ok = |c: &str| json!({"type": "tool_result", "tool_use_id": "t", "content": c});
    let r = result_of(
        &ok("…"),
        Some(&json!({"type": "text", "file": {"filePath": "/w/a", "numLines": 120}})),
    );
    assert_eq!((r.ok, r.lines), (true, Some(120)));
    let patch = json!({"filePath": "/w/a", "structuredPatch": [{"lines": [" a", "-b", "+c", "+d"]}, {"lines": ["-e"]}]});
    let r = result_of(&ok("…"), Some(&patch));
    assert_eq!((r.added, r.removed), (Some(2), Some(2)));
    let r = result_of(
        &ok("…"),
        Some(&json!({"type": "create", "filePath": "/w/b", "content": "1\n2\n3"})),
    );
    assert_eq!((r.added, r.removed), (Some(3), Some(0)));
    let r = result_of(
        &ok("…"),
        Some(&json!({"stdout": "1\n2", "stderr": "3", "interrupted": false})),
    );
    assert_eq!(r.lines, Some(3));
    let r = result_of(&ok("…"), Some(&json!({"numFiles": 4, "filenames": []})));
    assert_eq!(r.files, Some(4));
    // 失败：说失败与退出码（结果里写着才有）；人拒了：`rejected`。
    let err = json!({"type": "tool_result", "tool_use_id": "t", "content": "Exit code 127\nbash: x: not found", "is_error": true});
    assert_eq!(
        result_of(&err, Some(&json!("Error: Exit code 127"))),
        StepResult {
            ok: false,
            exit_code: Some(127),
            text: Words(copy_core::copy_text("stream.step.failed", &[])),
            timed: Some(Words(copy_core::copy_text(
                "stream.step.failedFor",
                &[("dur", "{dur}")]
            ))),
            preview: Some("Exit code 127".into()),
            ..Default::default()
        }
    );
    let err = json!({"type": "tool_result", "tool_use_id": "t", "content": "File does not exist.", "is_error": true});
    assert_eq!(
        result_of(&err, None).exit_code,
        None,
        "没写退出码就不编一个"
    );
    let rej = json!({"type": "tool_result", "tool_use_id": "t", "is_error": true,
        "content": "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed."});
    let r = result_of(&rej, Some(&json!("User rejected tool use")));
    assert_eq!((r.ok, r.rejected), (false, true));
}

#[test]
fn plan_and_question_answers_come_out_as_facts_not_english() {
    let ok = |c: &str| json!({"type": "tool_result", "tool_use_id": "t", "content": c});
    let plan = result_of(
        &ok("User has approved your plan. You can now start coding."),
        Some(&json!({"plan": "甲", "isAgent": false})),
    );
    assert_eq!(plan.answer, Some(Answer::Approved));
    // 老版本：结果里没有结构，只有那句英文。
    assert_eq!(
        result_of(
            &ok("User has approved your plan. You can now start coding."),
            None
        )
        .answer,
        Some(Answer::Approved)
    );
    let asked = result_of(
        &ok("…"),
        Some(&json!({"questions": [], "answers": {"甲?": "只记日志"}})),
    );
    assert_eq!(
        asked.answer,
        Some(Answer::Picked {
            options: vec!["只记日志".into()]
        })
    );
    let asked = result_of(&ok("User has answered your questions: \"甲?\"=\"只记日志\", \"乙?\"=\"也重试\". You can now continue."), None);
    assert_eq!(
        asked.answer,
        Some(Answer::Picked {
            options: vec!["只记日志".into(), "也重试".into()]
        })
    );
    // 计划没批：被拒那一支，不出 `answer`。
    let rej = json!({"type": "tool_result", "tool_use_id": "t", "is_error": true, "content": "The user doesn't want to proceed with this tool use."});
    let r = result_of(&rej, None);
    assert_eq!((r.rejected, r.answer), (true, None));
}

#[test]
fn api_reason_maps_status_and_error_kinds() {
    use ApiReason::*;
    let cases: Vec<(Option<u32>, Option<serde_json::Value>, &str, ApiReason)> = vec![
        (Some(529), None, "API Error: Overloaded", Overloaded),
        (
            None,
            Some(json!({"status": 503, "error": {"error": {"message": "x"}}})),
            "",
            Overloaded,
        ),
        (
            None,
            Some(json!({"formatted": "529 overloaded_error"})),
            "",
            Overloaded,
        ),
        (Some(429), None, "", Quota),
        (
            None,
            None,
            "Claude AI usage limit reached|1760000000",
            Quota,
        ),
        (Some(401), Some(json!("authentication_failed")), "", Auth),
        (
            None,
            Some(json!("invalid_request")),
            "API Error: 400 prompt is too long: 210000 tokens > 200000 maximum",
            Context,
        ),
        (
            None,
            Some(json!({"connection": {"code": "ECONNRESET", "message": "x"}})),
            "",
            Network,
        ),
        (None, None, "API Error: Connection error.", Network),
        (
            Some(400),
            Some(json!("invalid_request")),
            "API Error: 400 甲",
            Unknown,
        ),
        (None, None, "", Unknown),
    ];
    for (st, e, text, want) in cases {
        assert_eq!(
            api_reason(st, e.as_ref(), text),
            want,
            "{st:?} {e:?} {text}"
        );
    }
}

#[test]
fn the_record_product_carries_steps_results_and_reasons() {
    let a = parse_line(r#"{"type":"assistant","uuid":"u1","timestamp":"t","message":{"role":"assistant","content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"file_path":"/w/a"}}]}}"#).unwrap().unwrap();
    let JsonlRecord::Assistant {
        tool_steps,
        api_reason: why,
        ..
    } = a
    else {
        panic!()
    };
    assert_eq!(
        (
            tool_steps["tu1"].arg.as_deref(),
            tool_steps["tu1"].path,
            why
        ),
        (Some("/w/a"), true, None)
    );
    let u = parse_line(r#"{"type":"user","uuid":"u2","timestamp":"t","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tu1","content":"…"}]},"toolUseResult":{"file":{"numLines":7}}}"#).unwrap().unwrap();
    let v = serde_json::to_value(&u).unwrap();
    assert_eq!(
        v["toolResults"]["tu1"],
        json!({"ok": true, "lines": 7, "preview": "…", "text": copy_core::copy_text("stream.step.lines", &[("n", "7")])})
    );
    assert!(v.get("toolUseResult").is_none(), "原样那一格不上线");
    let e = parse_line(r#"{"type":"assistant","uuid":"u3","timestamp":"t","isApiErrorMessage":true,"apiErrorStatus":529,"error":"server_error","message":{"role":"assistant","content":[{"type":"text","text":"API Error: 529"}]}}"#).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&e).unwrap()["apiReason"], "overloaded");
    let s = parse_line(r#"{"type":"system","subtype":"api_error","level":"error","timestamp":"t","retryAttempt":3,"maxRetries":10,"error":{"status":529}}"#).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&s).unwrap()["apiReason"], "overloaded");
    let other =
        parse_line(r#"{"type":"system","subtype":"turn_duration","timestamp":"t","durationMs":5}"#)
            .unwrap()
            .unwrap();
    assert!(serde_json::to_value(&other)
        .unwrap()
        .get("apiReason")
        .is_none());
}

/// 新建整份文件：原文写的是**空的** `structuredPatch` ＋ `type: "create"`（真记录里新建文件全是这一形）。
/// 空表当没有 ⇒ 走「整份都是加的」，而不是报 `+0 −0`（桌面端那一行人话与手机端都读这两格）。
#[test]
fn creating_a_whole_file_counts_every_line_even_though_the_patch_is_empty() {
    let r = result_of(
        &json!({"type": "tool_result", "tool_use_id": "t", "content": "…"}),
        Some(&json!({
            "type": "create", "filePath": "/w/new.md",
            "content": "甲\n乙\n丙", "structuredPatch": [], "userModified": false
        })),
    );
    assert_eq!(
        (r.added, r.removed),
        (Some(3), Some(0)),
        "空的 structuredPatch 不是「没改动」，是「没有改之前」"
    );
}

/// 改动结果带逐段 diff ＋ 哪个文件：数照整份算；diff 过 32 KiB 在段的边界停下、说「不全」（整段不劈，第一段放不下也不给）；
/// 新建整份文件只说哪个文件、不给 diff。
#[test]
fn an_edit_result_carries_its_hunks_and_file_within_a_bound() {
    let hunk = |n: usize| json!({"oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 1, "lines": [format!("-{}", "a".repeat(n)), "+b"]});
    let block = json!({"type": "tool_result", "tool_use_id": "c", "content": "ok"});
    let small = json!({"filePath": "/w/a.rs", "structuredPatch": [hunk(3), hunk(3)]});
    let r = result_of(&block, Some(&small));
    assert_eq!((r.added, r.removed), (Some(2), Some(2)));
    assert_eq!(r.file.as_deref(), Some("/w/a.rs"));
    assert_eq!(r.patch.as_ref().map(Vec::len), Some(2));
    assert!(!r.patch_truncated);
    let big = json!({"filePath": "/w/a.rs", "structuredPatch": [hunk(3), hunk(40_000), hunk(3)]});
    let r = result_of(&block, Some(&big));
    assert_eq!((r.added, r.removed), (Some(3), Some(3)), "数照整份");
    assert_eq!(r.patch.as_ref().map(Vec::len), Some(1));
    assert!(r.patch_truncated);
    let huge_first = json!({"structuredPatch": [hunk(40_000)]});
    let r = result_of(&block, Some(&huge_first));
    assert!(r.patch.is_none() && r.patch_truncated);
    let create = json!({"type": "create", "filePath": "/w/n.rs", "content": "x\ny\n", "structuredPatch": []});
    let r = result_of(&block, Some(&create));
    assert_eq!(
        (r.added, r.removed, r.file.as_deref()),
        (Some(2), Some(0), Some("/w/n.rs"))
    );
    assert!(r.patch.is_none());
}

/// ★★ 过程那一行右侧那一句由核心写（`StepResult::written`）：改动 `+a −r` · 读了几行只给读文件那一类 · 几个文件 ·
/// 失败（带耗时那一形留 `{dur}`）· 未批准；都说不上 ⇒ 空串 ＋ 只有耗时那一形。
#[test]
fn the_right_hand_sentence_is_written_by_the_core() {
    let ok = |c: &str| json!({"type": "tool_result", "tool_use_id": "t", "content": c});
    let t = |r: &StepResult| (r.text.0.clone(), r.timed.as_ref().map(|w| w.0.clone()));
    let lines = |n: &str| copy_core::copy_text("stream.step.lines", &[("n", n)]);
    // 读文件：读了几行。
    let r = result_of(&ok("…"), Some(&json!({"file": {"numLines": 12}})));
    assert_eq!(t(&r), (lines("12"), None));
    // 命令输出几行、搜索命中几行：数照记（`lines`），右侧不说行数，只出耗时那一形。
    let r = result_of(&ok("…"), Some(&json!({"stdout": "1\n2", "stderr": ""})));
    assert_eq!(
        (r.lines, t(&r)),
        (Some(2), (String::new(), Some("{dur}".to_string())))
    );
    let r = result_of(&ok("…"), Some(&json!({"numLines": 5, "mode": "content"})));
    assert_eq!(t(&r).0, String::new(), "搜索命中几行不说成读了几行");
    // 改动：`+a −r`（为零的那一半不写）。
    let r = result_of(
        &ok("…"),
        Some(
            &json!({"filePath": "/w/a", "structuredPatch": [{"oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 2, "lines": ["-a", "+b", "+c"]}]}),
        ),
    );
    assert_eq!(t(&r), ("+2 −1".to_string(), None));
    // 几个文件。
    let r = result_of(&ok("…"), Some(&json!({"numFiles": 4, "filenames": []})));
    assert_eq!(
        t(&r).0,
        copy_core::copy_text("stream.step.files", &[("n", "4")])
    );
    // 未批准。
    let rej = json!({"type": "tool_result", "tool_use_id": "t", "is_error": true, "content": "User rejected tool use"});
    assert_eq!(
        t(&result_of(&rej, None)),
        (copy_core::copy_text("stream.step.rejected", &[]), None)
    );
}

/// ★ 提问 / 计划那几张卡要画的东西是核心出的一格（`ask`），界面不再从入参里自己解析：
/// 出口省掉 `blocks[type=tool_use].input` 之后交互卡照样建得出来。
#[test]
fn questions_and_plans_come_out_as_their_own_cell() {
    let s = step_of(
        "AskUserQuestion",
        &json!({"questions": [
            {"header": "范围", "question": "改哪一处？", "multiSelect": true,
             "options": [{"label": "甲", "description": "只改甲"}, {"label": "乙"}]},
            {"question": "还要吗？", "options": [{"label": "要"}]}
        ]}),
    );
    assert_eq!(
        s.ask,
        Some(crate::agents::StepAsk::Questions {
            questions: vec![
                crate::agents::AskQuestion {
                    header: Some("范围".into()),
                    question: "改哪一处？".into(),
                    multi: true,
                    options: vec![
                        crate::agents::AskOption {
                            label: "甲".into(),
                            description: Some("只改甲".into())
                        },
                        crate::agents::AskOption {
                            label: "乙".into(),
                            description: None
                        },
                    ],
                },
                crate::agents::AskQuestion {
                    header: None,
                    question: "还要吗？".into(),
                    multi: false,
                    options: vec![crate::agents::AskOption {
                        label: "要".into(),
                        description: None
                    }],
                },
            ]
        })
    );
    let p = step_of("ExitPlanMode", &json!({"plan": "## 计划\n1. 先做甲"}));
    assert_eq!(
        p.ask,
        Some(crate::agents::StepAsk::Plan {
            text: "## 计划\n1. 先做甲".into()
        })
    );
    // 形状不对 ⇒ 没有这一格（界面退成普通工具卡），别的工具也没有
    assert_eq!(
        step_of("AskUserQuestion", &json!({"questions": []})).ask,
        None
    );
    assert_eq!(
        step_of(
            "AskUserQuestion",
            &json!({"questions": [{"question": "q"}]})
        )
        .ask,
        None
    );
    assert_eq!(step_of("ExitPlanMode", &json!({"plan": "  "})).ask, None);
    assert_eq!(step_of("Bash", &json!({"command": "ls"})).ask, None);
}

/// ★ 结果那一行的首行预览是核心出的一格（`preview`）：第一条非空行、去掉两头空白、至多 60 字（按字符），截了以「…」收尾。
/// 出口省掉 `blocks[type=tool_result].content` 之后预览照样有。
#[test]
fn a_result_carries_its_first_line_preview() {
    let r = result_of(
        &json!({"type": "tool_result", "content": "\n   \n  第一行  \n第二行"}),
        None,
    );
    assert_eq!(r.preview.as_deref(), Some("第一行"));
    let long = "字".repeat(61);
    let r = result_of(
        &json!({"type": "tool_result", "content": [{"type": "text", "text": long}]}),
        None,
    );
    assert_eq!(r.preview, Some(format!("{}…", "字".repeat(59))));
    let exact = "字".repeat(60);
    let r = result_of(
        &json!({"type": "tool_result", "content": exact.clone()}),
        None,
    );
    assert_eq!(r.preview, Some(exact));
    let r = result_of(&json!({"type": "tool_result", "content": "  \n "}), None);
    assert_eq!(r.preview, None);
    // 报错那一支也带（结果行写「Error · …」）
    let r = result_of(
        &json!({"type": "tool_result", "is_error": true, "content": "boom\nexit 1"}),
        None,
    );
    assert_eq!(r.preview.as_deref(), Some("boom"));
}
