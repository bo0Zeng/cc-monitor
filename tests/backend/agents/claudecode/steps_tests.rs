//! 过程一步一行（`steps.rs`）：主参数与说明 · 结果一句 · 提问 / 计划答了什么 · 报错原因。夹具只造结构（占位路径与词），不采会话正文。

use super::*;
use crate::agents::claudecode::parse::parse_line;
use crate::agents::claudecode::schema::JsonlRecord;
use crate::agents::{LineDiff, LineDiffKind, LineDiffRow, ResultKind, StepBody};
use crate::common::cells::Words;
use serde_json::json;
use serde_json::Value;

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
            chars: Some(32),
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
    let a = parse_line(&crate::agents::Line::new(r#"{"type":"assistant","uuid":"u1","timestamp":"t","message":{"role":"assistant","content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"file_path":"/w/a"}}]}}"#, 0)).unwrap().unwrap();
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
    let u = parse_line(&crate::agents::Line::new(r#"{"type":"user","uuid":"u2","timestamp":"t","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tu1","content":"…"}]},"toolUseResult":{"file":{"numLines":7}}}"#, 0)).unwrap().unwrap();
    let v = serde_json::to_value(&u).unwrap();
    assert_eq!(
        v["toolResults"]["tu1"],
        json!({"ok": true, "lines": 7, "preview": "…", "chars": 1, "text": copy_core::copy_text("stream.step.lines", &[("n", "7")])})
    );
    assert!(v.get("toolUseResult").is_none(), "原样那一格不上线");
    let e = parse_line(&crate::agents::Line::new(r#"{"type":"assistant","uuid":"u3","timestamp":"t","isApiErrorMessage":true,"apiErrorStatus":529,"error":"server_error","message":{"role":"assistant","content":[{"type":"text","text":"API Error: 529"}]}}"#, 0)).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&e).unwrap()["apiReason"], "overloaded");
    let s = parse_line(&crate::agents::Line::new(r#"{"type":"system","subtype":"api_error","level":"error","timestamp":"t","retryAttempt":3,"maxRetries":10,"error":{"status":529}}"#, 0)).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&s).unwrap()["apiReason"], "overloaded");
    let other = parse_line(&crate::agents::Line::new(
        r#"{"type":"system","subtype":"turn_duration","timestamp":"t","durationMs":5}"#,
        0,
    ))
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
    let heads = |r: &StepResult| {
        r.patch.as_ref().map_or(0, |d| {
            d.rows
                .iter()
                .filter(|x| x.kind == LineDiffKind::Head)
                .count()
        })
    };
    let small = json!({"filePath": "/w/a.rs", "structuredPatch": [hunk(3), hunk(3)]});
    let r = result_of(&block, Some(&small));
    assert_eq!((r.added, r.removed), (Some(2), Some(2)));
    assert_eq!(r.file.as_deref(), Some("/w/a.rs"));
    assert_eq!(heads(&r), 2);
    assert!(!r.patch.as_ref().unwrap().truncated);
    let big = json!({"filePath": "/w/a.rs", "structuredPatch": [hunk(3), hunk(40_000), hunk(3)]});
    let r = result_of(&block, Some(&big));
    assert_eq!((r.added, r.removed), (Some(3), Some(3)), "数照整份");
    let d = r.patch.as_ref().unwrap();
    assert_eq!((d.added, d.removed), (3, 3), "行差上的数也照整份");
    assert_eq!(d.rows.len(), 3, "只给了第一段（段头 ＋ 两行）");
    assert!(d.truncated);
    let huge_first = json!({"structuredPatch": [hunk(40_000)]});
    let r = result_of(&block, Some(&huge_first));
    assert!(r.patch.is_none(), "一段也放不下 ⇒ 不给，两端画调用那一份");
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
    assert_eq!(
        r.patch.as_ref().map(|d| d.stat.0.as_str()),
        Some("+2 −1"),
        "行差那一格与右侧那一句同一处写"
    );
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
            text: "## 计划\n1. 先做甲".into(),
            title: crate::agents::PlanTitle::written(),
        })
    );
    // 计划卡的标题三态都由核心写好（状态格打头，同钉条第一行「等批准 · 计划」）；界面照抄、不另写字。
    let t = crate::agents::PlanTitle::written();
    assert_eq!(
        t.waiting.0,
        copy_core::copy_text("needs.bar.plan", &[]),
        "等着批：与钉条第一行同一句"
    );
    assert!(
        t.approved
            .0
            .starts_with(&copy_core::copy_text("interactive.done.approved", &[])),
        "批了：「已批准」打头"
    );
    assert!(
        t.kept
            .0
            .starts_with(&copy_core::copy_text("stream.step.rejected", &[])),
        "没批：「未批准」打头（同过程那一步拒了的那一句）"
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

/// ★ 结果正文多少字（按字符）是核心出的一格（`chars`）：派出子运行那张卡写「交回结果 · N 字」读它 ——
/// 出口省掉 `blocks[type=tool_result].content` 之后界面手里只剩预览，拿预览数字就是假话。空的 ⇒ 缺。
#[test]
fn a_result_carries_how_many_chars_its_body_has() {
    let r = result_of(
        &json!({"type": "tool_result", "content": [{"type": "text", "text": "交回\n第二行"}]}),
        None,
    );
    assert_eq!(r.chars, Some(6));
    let r = result_of(&json!({"type": "tool_result", "content": ""}), None);
    assert_eq!(r.chars, None);
    let r = result_of(
        &json!({"type": "tool_result", "is_error": true, "content": "boom"}),
        None,
    );
    assert_eq!(r.chars, Some(4), "报错那一支也带");
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

/// ★ 派出子 agent 那次调用的结果是哪一种是核心出的一格（`kind`）：后台派出当场回的「已启动」回执（`status: async_launched`）
/// 不是交回 —— 见结果就写「交回结果 · N 字」的话，子 agent 还在跑卡上就说它交回了。
/// 带 `agentId` 的其余结果 ⇒ 交回；不是派出子 agent 的结果 ⇒ 缺。
#[test]
fn a_dispatch_result_says_whether_it_is_a_launch_receipt_or_a_handback() {
    let block = json!({"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "Async agent launched"}]});
    let launched = json!({"status": "async_launched", "agentId": "w1"});
    assert_eq!(
        result_of(&block, Some(&launched)).kind,
        Some(ResultKind::Launched)
    );
    let done = json!({"status": "completed", "agentId": "w1"});
    assert_eq!(
        result_of(&block, Some(&done)).kind,
        Some(ResultKind::Handback)
    );
    assert_eq!(result_of(&block, None).kind, None);
    assert_eq!(result_of(&block, Some(&json!({"stdout": "x"}))).kind, None);
    // 出口那一形：一个字段 `kind`。
    let v = serde_json::to_value(result_of(&block, Some(&launched))).unwrap();
    assert_eq!(v["kind"], json!("launched"));
}

/// 写类那几个工具展开画的改动（`body`）：一处改 · 新写整份（改前为空）· 一次改几处；入参形状不对 ⇒ 缺（界面照原文画入参）。
#[test]
fn a_write_step_carries_what_changed() {
    let row = |kind: LineDiffKind, text: &str| LineDiffRow {
        kind,
        text: text.into(),
        old_no: None,
        new_no: None,
    };
    let body = |name: &str, input: Value| step_of(name, &input).body;
    let rows = |name: &str, input: Value| match step_of(name, &input).body {
        Some(StepBody::Changes { diff }) => diff.rows,
        other => panic!("{name} 没出改动：{other:?}"),
    };
    use LineDiffKind::{Add, Ctx, Del, Head};
    assert_eq!(
        rows(
            "Edit",
            json!({"file_path": "/w/a", "old_string": "k\nx\nk2", "new_string": "k\ny\nk2"})
        ),
        vec![row(Ctx, "k"), row(Del, "x"), row(Add, "y"), row(Ctx, "k2")],
        "从入参算的那一份不知道在文件第几行 ⇒ 行号缺"
    );
    assert_eq!(
        rows(
            "Write",
            json!({"file_path": "/w/a", "content": "l1\r\nl2\n"})
        ),
        vec![row(Add, "l1"), row(Add, "l2")],
        "新写整份：整份都是加的；行尾 CRLF 与末尾一个换行不成行"
    );
    let seg = |n: &str| copy_core::copy_text("cards.diff.segment", &[("n", n)]);
    assert_eq!(
        rows(
            "MultiEdit",
            json!({"file_path": "/w/a", "edits": [
                {"old_string": "a", "new_string": "b"},
                {"old_string": "c", "new_string": "d"}
            ]})
        ),
        vec![
            row(Head, &seg("1")),
            row(Del, "a"),
            row(Add, "b"),
            row(Head, &seg("2")),
            row(Del, "c"),
            row(Add, "d")
        ]
    );
    // 形状不对：缺一格 · 非字符串 · 改几处那张表空 / 不是表 / 有一项不对 ⇒ 整格缺。
    for (name, input) in [
        ("Edit", json!({"file_path": "x", "replace_all": true})),
        ("Edit", json!({"old_string": 1, "new_string": "x"})),
        ("Write", json!({"content": 123})),
        ("MultiEdit", json!({})),
        ("MultiEdit", json!({"edits": "nope"})),
        ("MultiEdit", json!({"edits": []})),
        (
            "MultiEdit",
            json!({"edits": [{"old_string": "a", "new_string": "b"}, {"old_string": "c"}]}),
        ),
        ("NotebookEdit", json!({"new_source": "x"})),
        ("Read", json!({"file_path": "/w/a"})),
    ] {
        assert_eq!(body(name, input.clone()), None, "{name} {input}");
    }
}

/// 命令那一步展开画整行命令原样（多行、不截；收着那一行的 `arg` 才压成一行截 200 字）；没有命令 ⇒ 缺。
#[test]
fn a_command_step_carries_its_whole_line() {
    let long = format!("echo {}\n  && ls", "a".repeat(ARG_MAX + 10));
    let s = step_of("Bash", &json!({"command": long, "description": " 构建 "}));
    assert_eq!(s.body, Some(StepBody::Command { line: long.clone() }));
    assert_eq!(s.note.as_deref(), Some("构建"));
    assert_eq!(step_of("Bash", &json!({"command": ""})).body, None);
    assert_eq!(step_of("Bash", &json!({"description": "x"})).body, None);
}

/// 有 `body` 的那几步，卡型与它同类（改动 ⇒ diff 卡 · 命令 ⇒ 命令卡）；两张词表各认各的，这里钉它们不分叉。
#[test]
fn the_body_kind_agrees_with_the_card_kind() {
    use crate::agents::ToolCard;
    for (name, input) in [
        ("Edit", json!({"old_string": "a", "new_string": "b"})),
        ("Write", json!({"content": "a"})),
        (
            "MultiEdit",
            json!({"edits": [{"old_string": "a", "new_string": "b"}]}),
        ),
        ("Bash", json!({"command": "ls"})),
    ] {
        let card = super::super::cards::tool_card(name);
        match step_of(name, &input).body {
            Some(StepBody::Changes { .. }) => assert_eq!(card, Some(ToolCard::Diff), "{name}"),
            Some(StepBody::Command { .. }) => assert_eq!(card, Some(ToolCard::Command), "{name}"),
            None => panic!("{name} 没出 body"),
        }
    }
}

/// 这一步说的那份文件（`file`）：主参数是路径、且是绝对路径 ⇒ 原样（不压行、不截）；相对路径 · 不是路径的工具 ⇒ 缺。
#[test]
fn a_path_step_names_its_file_when_absolute() {
    let file = |name: &str, input: Value| step_of(name, &input).file;
    let deep = format!("/w/{}/a.ts", "d".repeat(ARG_MAX));
    assert_eq!(
        file("Read", json!({ "file_path": deep })),
        Some(deep.clone())
    );
    assert_eq!(
        file("NotebookEdit", json!({"notebook_path": "/w/n.ipynb"})),
        Some("/w/n.ipynb".into())
    );
    assert_eq!(file("Edit", json!({"file_path": "src/a.ts"})), None);
    assert_eq!(file("Edit", json!({"file_path": ""})), None);
    assert_eq!(file("Bash", json!({"command": "/bin/ls"})), None);
}

/// ★ 工具入参的键名是这一家的格式知识，只许住适配层：界面（`src/frontend/` 的 TS）与手机（`src/mobile/` 的 Kotlin，测试源除外）
/// 零出现；正控：同一把扫描在 `agents/claudecode/` 里把它们一个不少地找到。
#[test]
fn the_frontends_never_read_a_tool_input_key() {
    const KEYS: &[&str] = &["old_string", "new_string", "file_path", "notebook_path"];
    let root = guard_core::addr::repo_root();
    let word = |text: &str, k: &str| {
        let is_id = |c: char| c.is_ascii_alphanumeric() || c == '_';
        text.match_indices(k).any(|(i, _)| {
            !text[..i].chars().next_back().is_some_and(is_id)
                && !text[i + k.len()..].chars().next().is_some_and(is_id)
        })
    };
    let scan = |dir: &str, exts: &[&str]| -> Vec<(String, String)> {
        let files: Vec<_> = guard_core::scan_tree_excluding(&root.join(dir), exts, &[])
            .into_iter()
            .filter(|(p, _)| {
                let p = p.to_string_lossy().replace('\\', "/");
                !p.contains("/src/test/") && !p.contains("/src/androidTest/")
            })
            .collect();
        assert!(!files.is_empty(), "{dir} 下一份文件都没扫到 —— 扫描空转");
        files
            .iter()
            .flat_map(|(p, text)| {
                KEYS.iter()
                    .filter(|k| word(text, k))
                    .map(move |k| (k.to_string(), p.display().to_string()))
            })
            .collect()
    };
    let mut found = scan("src/frontend", &["ts"]);
    found.extend(scan("src/mobile", &["kt"]));
    assert!(
        found.is_empty(),
        "界面 / 手机读了工具入参的键名（改读 `steps` 里的通用格）：\n{}",
        found
            .iter()
            .map(|(k, p)| format!("  {k}  ←  {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let in_adapter: std::collections::BTreeSet<String> =
        scan("src/backend/agents/claudecode", &["rs"])
            .into_iter()
            .map(|(k, _)| k)
            .collect();
    assert_eq!(
        in_adapter,
        KEYS.iter().map(|k| k.to_string()).collect(),
        "正控：适配层里这几个键名该一个不少（扫描瞎了，或键名换了）"
    );
}

/// ★ 同一次改动的行差只在后端算一份：结果那一份照 structuredPatch 出行、带真行号，加 · 删的行数与 `added` / `removed` 恒等，
/// 「+N −M」与右侧那一句同字；调用那一份照入参算，两份对同一处改动的增删数一样。两端只照这两份画，不判行、不数数。
#[test]
fn one_edit_has_one_line_diff_written_by_the_core() {
    let block = json!({"type": "tool_result", "tool_use_id": "c", "content": "ok"});
    let tur = json!({"filePath": "/w/a.rs", "structuredPatch": [
        {"oldStart": 3, "oldLines": 3, "newStart": 3, "newLines": 3, "lines": [" k", "-x\r", "+y", " k2"]},
        {"oldStart": 20, "oldLines": 1, "newStart": 20, "newLines": 2, "lines": ["-p", "+q", "+r", "\\ No newline at end of file"]}
    ]});
    let r = result_of(&block, Some(&tur));
    let d: &LineDiff = r.patch.as_ref().expect("结果带行差");
    let count = |k: LineDiffKind| d.rows.iter().filter(|x| x.kind == k).count() as u32;
    assert_eq!(
        (count(LineDiffKind::Add), count(LineDiffKind::Del)),
        (r.added.unwrap(), r.removed.unwrap())
    );
    assert_eq!((d.added, d.removed), (3, 2));
    assert_eq!(d.stat, r.text);
    let nos: Vec<(LineDiffKind, &str, Option<u32>, Option<u32>)> = d
        .rows
        .iter()
        .map(|x| (x.kind, x.text.as_str(), x.old_no, x.new_no))
        .collect();
    use LineDiffKind::{Add, Ctx, Del, Head};
    assert_eq!(
        nos,
        vec![
            (Head, "@@ -3,3 +3,3 @@", None, None),
            (Ctx, "k", Some(3), Some(3)),
            (Del, "x", Some(4), None),
            (Add, "y", None, Some(4)),
            (Ctx, "k2", Some(5), Some(5)),
            (Head, "@@ -20,1 +20,2 @@", None, None),
            (Del, "p", Some(20), None),
            (Add, "q", None, Some(20)),
            (Add, "r", None, Some(21)),
        ],
        "真行号从段头起算；`\\` 那一行不占行"
    );
    assert_eq!((d.shown, d.truncated), (9, false));
    // 调用那一份：同一处改动、增删数一样，「+N −M」同一处写。
    let s = step_of(
        "Edit",
        &json!({"file_path": "/w/a.rs", "old_string": "k\nx\nk2", "new_string": "k\ny\nk2"}),
    );
    let Some(StepBody::Changes { diff }) = s.body else {
        panic!("没出改动")
    };
    assert_eq!(
        (diff.added, diff.removed, diff.stat.0.as_str()),
        (1, 1, "+1 −1")
    );
}

/// 行多了先画前几行（其余等展开）；改前改后都很长、逐行比的格数超了预算 ⇒ 仍出整删 ＋ 整增，数照这一形。
#[test]
fn a_long_diff_shows_its_head_and_keeps_whole_counts() {
    let many: String = (0..1000).map(|i| format!("l{i}\n")).collect();
    let s = step_of("Write", &json!({"file_path": "/w/a", "content": many}));
    let Some(StepBody::Changes { diff }) = s.body else {
        panic!("没出改动")
    };
    assert_eq!((diff.rows.len(), diff.added, diff.removed), (1000, 1000, 0));
    assert!((diff.shown as usize) < diff.rows.len(), "收着时只画前几行");
    assert!(!diff.truncated);
    let old: String = (0..3000).map(|i| format!("o{i}\n")).collect();
    let new: String = (0..3000).map(|i| format!("n{i}\n")).collect();
    let s = step_of("Edit", &json!({"old_string": old, "new_string": new}));
    let Some(StepBody::Changes { diff }) = s.body else {
        panic!("没出改动")
    };
    assert_eq!((diff.added, diff.removed), (3000, 3000));
    assert!(
        diff.notice.as_ref().is_some_and(|n| !n.0.is_empty()),
        "超了格数上界只画整删整增 ⇒ 带那一句"
    );
}

/// 没超格数上界 ⇒ 不带降级那一句；几处里有一处超了 ⇒ 整份带它。
#[test]
fn only_a_diff_past_the_cell_cap_says_it_was_not_compared_line_by_line() {
    let s = step_of(
        "Edit",
        &json!({"file_path": "/w/a.rs", "old_string": "k\nx", "new_string": "k\ny"}),
    );
    let Some(StepBody::Changes { diff }) = s.body else {
        panic!("没出改动")
    };
    assert_eq!(diff.notice, None);
    let old: String = (0..3000).map(|i| format!("o{i}\n")).collect();
    let new: String = (0..3000).map(|i| format!("n{i}\n")).collect();
    let s = step_of(
        "MultiEdit",
        &json!({"file_path": "/w/a.rs", "edits": [
            {"old_string": "a", "new_string": "b"},
            {"old_string": old, "new_string": new},
        ]}),
    );
    let Some(StepBody::Changes { diff }) = s.body else {
        panic!("没出改动")
    };
    assert!(diff.notice.is_some(), "几处里有一处降级 ⇒ 整份带那一句");
}
