//! Claude 一行 ⇒ 通用记录：各类全格 ＋ 最少格（金样 `tests/__fixtures__/record.golden.jsonl`，手机端照它比）· 自动应答按型号认 ·
//! 报错带原因与状态码 · 一轮结束与帧 `turn_end` 同判 · 不进界面的几类不出记录。夹具是结构占位，不采真正文。
use super::*;

/// 金样重写开关（人读过「现打」再落盘）。
const REGEN: &str = "CCM_REGEN_RECORD_GOLDEN";

fn one(raw: &str) -> Option<Record> {
    let rec = crate::agents::claudecode::parse::parse_line(raw)
        .unwrap()
        .unwrap();
    record_of(rec, &crate::agents::line_id(4096)).map(|mut r| {
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
        r#"{"type":"assistant","uuid":"a-full","parentUuid":"u-full","timestamp":"2026-10-09T01:30:02.000Z","requestId":"req-1","message":{"role":"assistant","model":"model-x","stop_reason":"end_turn","usage":{"input_tokens":1},"content":[{"type":"thinking","thinking":"plan it"},{"type":"text","text":"done"},{"type":"tool_use","id":"call-2","name":"Task","input":{"description":"scan","subagent_type":"Explore","prompt":"p"}},{"type":"tool_use","id":"call-3","name":"Edit","input":{"file_path":"/w/a.txt","old_string":"a","new_string":"b"}}]}}"#,
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
        "queued",
        r#"{"type":"queue-operation","operation":"remove","content":"also do this","timestamp":"2026-10-09T01:30:08.000Z"}"#,
    ),
];

#[test]
fn every_kind_matches_the_golden_full_and_minimal() {
    let got: String = CASES
        .iter()
        .map(|(name, raw)| {
            let r = one(raw).unwrap_or_else(|| panic!("{name} 没出记录"));
            format!(
                "{}\n",
                serde_json::to_string(&serde_json::json!({ "case": name, "record": r })).unwrap()
            )
        })
        .collect();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/__fixtures__/record.golden.jsonl"
    );
    if std::env::var_os(REGEN).is_some() {
        std::fs::write(path, &got).unwrap();
    }
    let want = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        got, want,
        "通用记录与金样不一致（{REGEN}=1 重写）。现打：\n{got}"
    );
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
fn ends_turn_agrees_with_the_turn_end_frame() {
    for (name, raw) in CASES {
        if let Some(Record {
            body: Body::Reply { ends_turn, .. },
            ..
        }) = one(raw)
        {
            assert_eq!(
                ends_turn,
                super::super::turn::turn_end_uuid_of(raw).is_some(),
                "{name}"
            );
        }
    }
}

#[test]
fn records_without_a_reader_do_not_come_out() {
    for raw in [
        r#"{"type":"attachment","uuid":"x","timestamp":"t","parentUuid":"p"}"#,
        r#"{"type":"system","subtype":"local_command","uuid":"x","timestamp":"t"}"#,
        r#"{"type":"permission-mode","permissionMode":"plan","sessionId":"s"}"#,
        r#"{"type":"queue-operation","operation":"enqueue","content":"x","timestamp":"t"}"#,
        r#"{"type":"some-new-kind","uuid":"x","timestamp":"t"}"#,
    ] {
        let rec = crate::agents::claudecode::parse::parse_line(raw)
            .unwrap()
            .unwrap();
        assert!(record_of(rec, "L1").is_none(), "{raw}");
    }
}
