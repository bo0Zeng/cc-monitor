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
        "queued",
        r#"{"type":"queue-operation","operation":"remove","content":"also do this","timestamp":"2026-10-09T01:30:08.000Z"}"#,
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
        r#"{"type":"queue-operation","operation":"remove","content":"see <pasted_content id=\"p2\">\nA\n</pasted_content id=\"p2\">","timestamp":"2026-10-09T01:31:29.000Z"}"#,
    ),
    (
        "unread-unknown",
        r#"{"type":"brand-new-kind","uuid":"n-1","timestamp":"2026-10-09T01:31:30.000Z","x":1}"#,
    ),
    (
        "unread-parse-failed",
        r#"{"type":"ai-title","sessionId":"s"}"#,
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

/// ★ 便宜的判类（骨架行的 `t`）与整条翻译给的类逐条一致：金样每一条 · 不进界面的几类 · 各条去掉任意一格顶层字段
/// （必填格缺了盘上形状就认不出、不出记录）· 秤的原文语料（`scale2-height-records.jsonl`）每一行。
#[test]
fn the_cheap_class_agrees_with_the_record() {
    let root = crate::guard_support::repo_root();
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
    for raw in &lines {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let want = crate::agents::claudecode::parse::parse_line(raw)
            .unwrap()
            .and_then(|r| record_of(r, "L1"))
            .map(|r| r.body.class());
        // 骨架不出认不出的行（`unread`）：它不在主线判定里起作用，骨架上也没有它的读者 ⇒ 便宜那一问对它答「不出」。
        let cheap = want.filter(|c| *c != RecordClass::Unread);
        assert_eq!(class_of(&v), cheap, "{raw}");
        seen.insert(format!("{want:?}"));
    }
    assert_eq!(seen.len(), 7, "六类 ＋ 不出记录都得见到：{seen:?}");
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
    ] {
        let rec = crate::agents::claudecode::parse::parse_line(raw)
            .unwrap()
            .unwrap();
        assert!(record_of(rec, "L1").is_none(), "{raw}");
    }
}

/// Claude 自己的状态行（07-16 语料里落在「未知类型」的那 7 种）：认识、不上屏 —— 不出记录，也不再记成漂移。
#[test]
fn claude_status_lines_are_known_and_stay_off_screen() {
    for t in [
        "mode",
        "agent-name",
        "file-history-delta",
        "pr-link",
        "relocated",
        "worktree-state",
        "frame-link",
    ] {
        let raw = format!(r#"{{"type":"{t}","sessionId":"s","x":1}}"#);
        let rec = crate::agents::claudecode::parse::parse_line(&raw)
            .unwrap()
            .unwrap();
        assert!(
            !matches!(
                rec,
                crate::agents::claudecode::schema::JsonlRecord::Unrecognized { .. }
            ),
            "{t} 还落在认不出那一支"
        );
        assert!(record_of(rec, "L1").is_none(), "{t}");
    }
}

/// 认不出的一行出 `unread`：没见过的类型 · 见过的类型形状变了；写好的一句、`warn`、原文摘录按字符截到 400 字节以内。
#[test]
fn an_unread_line_comes_out_as_unread() {
    use crate::agents::record::{UnreadWhy, EXCERPT_BYTES};
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
    let long = format!(r#"{{"type":"some-new-kind","pad":"{}"}}"#, "字".repeat(400));
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

/// 秤 1 / 2 / 6 喂界面的语料就是这一家翻译表对原文语料（`scale2-height-records.jsonl`）逐行的成品 ——
/// 界面那边的秤量的是后端真会给的东西，不是手改的形状。钟面随时区变 ⇒ 都不带 `timeText`。
/// 秤 1 的「膨胀」那一份也在这里出：膨胀的是**原文**（每个 ≥ [`INFLATE_MIN_LEN`] 字的串尾接换行 ＋ 填充），
/// 再过翻译表 ⇒ 后端从原文算出的那几格（过程一行的主参数 · 结果几行）照真的变，界面那边不再自己挑哪格不膨胀。
const REGEN_SCALE: &str = "CCM_REGEN_SCALE_RECORDS";
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
    let root = crate::guard_support::repo_root();
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
        let Some(rec) = crate::agents::claudecode::parse::parse_line(&line).unwrap() else {
            continue;
        };
        if let Some(mut r) = record_of(rec, &crate::agents::line_id(start)) {
            r.time_text = None;
            got.push_str(&serde_json::to_string(&r).unwrap());
            got.push('\n');
        }
    }
    got
}

#[test]
fn the_scale_corpora_are_what_this_translator_gives() {
    let root = crate::guard_support::repo_root();
    for (name, inflated) in [
        ("scale2-height-line-records.jsonl", false),
        ("scale1-inflated-line-records.jsonl", true),
    ] {
        let got = scale_records(inflated);
        let path = root.join("tests/__fixtures__").join(name);
        if std::env::var_os(REGEN_SCALE).is_some() {
            std::fs::write(&path, &got).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            got == want,
            "{name} 与翻译表现打的不一致（{REGEN_SCALE}=1 重写，再回界面那几杆秤改登记）"
        );
    }
}
