//! 〔SE1〕monitor 侧大纲数据源的纯函数判据（`session_outline`）。
//!
//! 买到：真清单解析得出、顺序不动 · 老后端（0 字节 / 透传 jsonl / 回 hello）**认得出来**而不是当清单解析 ·
//! 截断的不当全量 · argv 选项在前（钉住顺序）· 头尾 kind 与后端源码里写出的那两个串一致（异源：读后端源码）。
//! **买不到**：真 SSH / 真本机后端那一圈（transport 本身由 `subagent::Backend` 的既有判据管；
//! 本机真后端那一趟现打在交付报告里）。

use super::*;

fn l(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_real_list_parses_in_order() {
    let out = l(&[
        r#"{"kind":"user_inputs","v":1,"from":0}"#,
        r#"{"uuid":"b","timestamp":"t2","excerpt":"second in file"}"#,
        r#"{"uuid":"a","timestamp":"","excerpt":"x"}"#,
        r#"{"kind":"user_inputs_end","count":2,"end":99}"#,
    ]);
    let (from, end, entries) = parse_user_inputs_output(&out).unwrap();
    assert_eq!((from, end), (0, 99));
    let uuids: Vec<&str> = entries.iter().map(|e| e.uuid.as_str()).collect();
    assert_eq!(uuids, ["b", "a"], "顺序是文件顺序，本侧不许重排");
    assert_eq!(entries[0].excerpt, "second in file");
    // 空清单（会话里一句话都没说过）是合法的全量，不是「拿不到」
    let empty = l(&[
        r#"{"kind":"user_inputs","v":1,"from":7}"#,
        r#"{"kind":"user_inputs_end","count":0,"end":7}"#,
    ]);
    assert_eq!(parse_user_inputs_output(&empty).unwrap(), (7, 7, vec![]));
}

/// 🔴 老后端：不认这条子命令 ⇒ 0 字节退出（本机那条路报错先走了，远端那条读到 0 行）；
/// 更老的会进流模式回 hello；或者对面干脆吐了 jsonl。三种都**不许**当清单。
#[test]
fn an_old_backend_is_recognised_not_parsed() {
    assert_eq!(
        parse_user_inputs_output(&[]),
        Err(OutlineUnavailable::OldBackend)
    );
    let hello = l(&[r#"{"kind":"hello","v":1,"build_id":"p2o"}"#]);
    assert_eq!(
        parse_user_inputs_output(&hello),
        Err(OutlineUnavailable::OldBackend)
    );
    let jsonl = l(&[
        r#"{"type":"user","uuid":"x","message":{"content":"hi"}}"#,
        r#"{"kind":"user_inputs_end","count":0,"end":1}"#,
    ]);
    assert_eq!(
        parse_user_inputs_output(&jsonl),
        Err(OutlineUnavailable::OldBackend)
    );
}

#[test]
fn a_truncated_list_is_not_taken_as_complete() {
    let head = r#"{"kind":"user_inputs","v":1,"from":0}"#;
    let row = r#"{"uuid":"a","timestamp":"","excerpt":"x"}"#;
    // 有头没尾
    assert_eq!(
        parse_user_inputs_output(&l(&[head, row])),
        Err(OutlineUnavailable::Truncated { got: 0 })
    );
    // 只有头
    assert_eq!(
        parse_user_inputs_output(&l(&[head])),
        Err(OutlineUnavailable::Truncated { got: 0 })
    );
    // 尾行条数对不上
    let tail5 = r#"{"kind":"user_inputs_end","count":5,"end":1}"#;
    assert_eq!(
        parse_user_inputs_output(&l(&[head, row, tail5])),
        Err(OutlineUnavailable::Truncated { got: 1 })
    );
    // 中间一行坏了
    let tail1 = r#"{"kind":"user_inputs_end","count":1,"end":1}"#;
    assert_eq!(
        parse_user_inputs_output(&l(&[head, r#"{"uuid":"a""#, tail1])),
        Err(OutlineUnavailable::Truncated { got: 1 })
    );
    // 对照：同样三行、中间那行是好的 ⇒ 过
    assert!(parse_user_inputs_output(&l(&[head, row, tail1])).is_ok());
}

/// 🔴 选项写在位置参数前面，逐字钉住（同 `session_skeleton_tests` 钉 `index_argv` 那一条）。
#[test]
fn the_argv_puts_the_option_before_the_path() {
    assert_eq!(
        user_inputs_argv("/h/projects/p/s.jsonl", 42),
        l(&[
            "--list-user-inputs",
            "--from",
            "42",
            "/h/projects/p/s.jsonl"
        ])
    );
}

/// 异源：本侧认的头尾 kind、行上的三个键，必须是后端**源码里真写出去**的那几个串。
/// 两侧各写一遍字面量而不对拍 ⇒ 后端改一个字，本侧永远认成「老后端」而判据全绿。
#[test]
fn the_wire_words_match_what_the_backend_source_writes() {
    let backend = include_str!("../../src/backend/observe/user_inputs.rs");
    for needle in [
        r#"{{\"kind\":\"user_inputs\",\"v\":1,\"from\":{from}}}"#,
        r#"{{\"kind\":\"user_inputs_end\",\"count\":{count},\"end\":{end}}}"#,
        "pub(crate) uuid: String,",
        "pub(crate) timestamp: String,",
        "pub(crate) excerpt: String,",
    ] {
        assert_eq!(
            backend.matches(needle).count(),
            1,
            "后端源码里没有恰好一处 {needle:?} —— 线上词对不上了"
        );
    }
    let hist = include_str!("../../src/backend/observe/history_query.rs");
    assert_eq!(
        hist.matches(r#"Some("--list-user-inputs") =>"#).count(),
        1,
        "后端分派里的子命令名与 argv 对不上"
    );
    assert_eq!(
        hist.matches(r#""--from" =>"#).count(),
        1,
        "后端认的选项名与 argv 对不上"
    );
}

/// 〔SE1 回修〕**分类只有 `outline_result` 一个住址**：三种要不到各落各档，要到了没有种类。
/// 前端只 match 这个种类（结构性 ⇒ 不再要；瞬时 ⇒ 下一次触发再要），不解析 `reason` 的文字。
#[test]
fn every_failure_lands_in_its_own_kind() {
    let head = r#"{"kind":"user_inputs","v":1,"from":0}"#;
    let tail0 = r#"{"kind":"user_inputs_end","count":0,"end":5}"#;
    use crate::subagent::{QueryError, QueryFailure};
    let kind = |q: Result<Vec<String>, QueryError>| outline_result(q, 0).failure;
    // 结构性：老后端（0 字节 / hello / 别的形状）
    assert_eq!(kind(Ok(vec![])), Some(OutlineFailure::OldBackend));
    assert_eq!(
        kind(Ok(l(&[r#"{"kind":"hello","v":1}"#]))),
        Some(OutlineFailure::OldBackend)
    );
    // 瞬时：截断
    assert_eq!(kind(Ok(l(&[head]))), Some(OutlineFailure::Truncated));
    // 查询自己带出种类 ⇒ 逐档照搬（〔C2〕此前一律 `transport`，那是 SE1 登记的两处分错档）
    assert_eq!(
        kind(Err(QueryError::transport("随便一句话".into()))),
        Some(OutlineFailure::Transport)
    );
    assert_eq!(
        kind(Err(QueryError::old_backend("随便一句话".into()))),
        Some(OutlineFailure::OldBackend)
    );
    assert_eq!(
        kind(Err(QueryError::truncated("随便一句话".into()))),
        Some(OutlineFailure::Truncated)
    );
    assert_eq!(
        kind(Err(QueryError::transport(
            "unknown argument: --list-user-inputs".into()
        ))),
        Some(OutlineFailure::Transport),
        "错误文字里像「老后端」也不许据文字改判 —— 分类只看查询自己带出来的种类"
    );
    // 三档映射逐档相等（两向：每个查询种类都落到同名的大纲种类，且三个大纲种类都被落到）
    let all = [
        QueryFailure::OldBackend,
        QueryFailure::Truncated,
        QueryFailure::Transport,
    ];
    let mapped: std::collections::BTreeSet<String> = all
        .iter()
        .map(|k| format!("{:?}", outline_kind(*k)))
        .collect();
    let names: std::collections::BTreeSet<String> = all.iter().map(|k| format!("{k:?}")).collect();
    assert_eq!(mapped, names, "查询种类 ⇒ 大纲种类不是逐名一一对应");
    // 要到了：没有种类、没有原因
    let ok = outline_result(Ok(l(&[head, tail0])), 0);
    assert!(ok.available);
    assert_eq!(ok.failure, None);
    assert_eq!(ok.reason, None);
    // 要不到时 end 原样退回请求的 from（前端据此不动续点）
    let bad = outline_result(Err(crate::subagent::QueryError::transport("x".into())), 42);
    assert_eq!((bad.from, bad.end, bad.available), (42, 42, false));
}

/// 线上形状：种类是 camelCase 的字面量（前端生成物是字面量联合）；要到了就不出这个键。
#[test]
fn the_failure_kind_is_on_the_wire_only_when_unavailable() {
    let v = serde_json::to_value(outline_result(
        Err(crate::subagent::QueryError::transport("x".into())),
        0,
    ))
    .unwrap();
    assert_eq!(v["failure"], "transport");
    let v = serde_json::to_value(outline_result(Ok(vec![]), 0)).unwrap();
    assert_eq!(v["failure"], "oldBackend");
    let head = r#"{"kind":"user_inputs","v":1,"from":0}"#;
    let v = serde_json::to_value(outline_result(Ok(l(&[head])), 0)).unwrap();
    assert_eq!(v["failure"], "truncated");
    let tail0 = r#"{"kind":"user_inputs_end","count":0,"end":0}"#;
    let v = serde_json::to_value(outline_result(Ok(l(&[head, tail0])), 0)).unwrap();
    assert!(v.get("failure").is_none());
}
