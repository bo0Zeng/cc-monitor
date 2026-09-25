//! 〔SE2〕monitor 侧会话内查找的纯函数判据（`session_find`）。
//!
//! 买到：真命中解析得出、顺序不动、全量数原样带回 · 老后端（0 字节 / hello / 别的形状）**认得出来**而不是当命中解析 ·
//! 截断的不当全量 · argv 选项全在前、查询是 `--query` 的值（钉住）· 头尾 kind、行上的键、子命令与选项名
//! 与后端源码里写出的那几个串一致（异源：读后端源码）。
//! **买不到**：真 SSH / 真本机后端那一圈（transport 由 `subagent::Backend` 的既有判据管）。

use super::*;

fn l(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

const HEAD: &str = r#"{"kind":"session_find","v":1}"#;

#[test]
fn real_hits_parse_in_order_with_the_full_count() {
    let out = l(&[
        HEAD,
        r#"{"uuid":"b","kind":"user","before":"x ","matched":"Ab","after":" y"}"#,
        r#"{"uuid":"a","kind":"tool","before":"","matched":"ab","after":""}"#,
        r#"{"kind":"session_find_end","count":2,"total":9}"#,
    ]);
    let (total, hits) = parse_find_output(&out).unwrap();
    assert_eq!(total, 9, "全量数原样带回（> 条数 ⇒ 被上限砍过）");
    let uuids: Vec<&str> = hits.iter().map(|h| h.uuid.as_str()).collect();
    assert_eq!(uuids, ["b", "a"], "顺序是文件顺序，本侧不许重排");
    assert_eq!(hits[0].matched, "Ab");
    assert_eq!(hits[1].kind, "tool");
    // 零命中是合法的全量，不是「拿不到」
    let empty = l(&[HEAD, r#"{"kind":"session_find_end","count":0,"total":0}"#]);
    assert_eq!(parse_find_output(&empty).unwrap(), (0, vec![]));
}

/// 🔴 老后端：不认这条子命令 ⇒ 0 字节；更老的回 hello；或者对面吐了 jsonl。三种都**不许**当命中。
#[test]
fn an_old_backend_is_recognised_not_parsed() {
    assert_eq!(parse_find_output(&[]), Err(FindUnavailable::OldBackend));
    let hello = l(&[r#"{"kind":"hello","v":1,"build_id":"p2t"}"#]);
    assert_eq!(parse_find_output(&hello), Err(FindUnavailable::OldBackend));
    let jsonl = l(&[
        r#"{"type":"user","uuid":"x","message":{"content":"hi"}}"#,
        r#"{"kind":"session_find_end","count":0,"total":0}"#,
    ]);
    assert_eq!(parse_find_output(&jsonl), Err(FindUnavailable::OldBackend));
    // 回包层：老后端 ⇒ 不可用 ＋ 原因，零命中
    let r = find_result(Ok(vec![]));
    assert!(!r.available && r.hits.is_empty() && r.reason.is_some());
}

#[test]
fn truncated_output_is_not_taken_as_complete() {
    let row = r#"{"uuid":"a","kind":"user","before":"","matched":"q","after":""}"#;
    assert_eq!(
        parse_find_output(&l(&[HEAD, row])),
        Err(FindUnavailable::Truncated { got: 0 }),
        "有头没尾"
    );
    assert_eq!(
        parse_find_output(&l(&[HEAD])),
        Err(FindUnavailable::Truncated { got: 0 }),
        "只有头"
    );
    let tail5 = r#"{"kind":"session_find_end","count":5,"total":5}"#;
    assert_eq!(
        parse_find_output(&l(&[HEAD, row, tail5])),
        Err(FindUnavailable::Truncated { got: 1 }),
        "尾行条数对不上"
    );
    let tail1 = r#"{"kind":"session_find_end","count":1,"total":1}"#;
    assert_eq!(
        parse_find_output(&l(&[HEAD, r#"{"uuid":"a""#, tail1])),
        Err(FindUnavailable::Truncated { got: 1 }),
        "中间一行坏了"
    );
    let no_total = r#"{"kind":"session_find_end","count":1}"#;
    assert_eq!(
        parse_find_output(&l(&[HEAD, row, no_total])),
        Err(FindUnavailable::Truncated { got: 1 }),
        "尾行缺全量数"
    );
    // 对照：同样三行、中间那行是好的 ⇒ 过
    assert!(parse_find_output(&l(&[HEAD, row, tail1])).is_ok());
}

/// 🔴 argv 逐字：选项全在位置参数前面；查询是 `--query` 的值（以 `--` 起头也照样放在值位上）。
#[test]
fn the_argv_puts_every_option_before_the_path_and_the_query_in_a_value_slot() {
    assert_eq!(
        find_argv("/h/projects/p/s.jsonl", "--force", false),
        l(&[
            "--find-in-session",
            "--limit",
            "500",
            "--query",
            "--force",
            "/h/projects/p/s.jsonl"
        ])
    );
    assert_eq!(
        find_argv("/p.jsonl", "q", true),
        l(&[
            "--find-in-session",
            "--include-tools",
            "--limit",
            "500",
            "--query",
            "q",
            "/p.jsonl"
        ])
    );
}

/// 异源：本侧认的头尾 kind、行上的键、子命令名与选项名，必须是后端**源码里真写出去 / 真认**的那几个串。
/// 两侧各写一遍字面量而不对拍 ⇒ 后端改一个字，本侧永远认成「老后端」而判据全绿。
#[test]
fn the_wire_words_match_what_the_backend_source_writes() {
    let search = include_str!("../../src/backend/observe/search_query.rs");
    for needle in [
        r#"{{\"kind\":\"session_find\",\"v\":1}}"#,
        r#"{{\"kind\":\"session_find_end\",\"count\":{count},\"total\":{total}}}"#,
        r#""uuid": uuid,"#,
        r#""kind": kind,"#,
        r#""before": before,"#,
        r#""matched": matched,"#,
        r#""after": after,"#,
        "pub(crate) const FIND_DEFAULT_LIMIT: usize = 500;",
    ] {
        assert!(
            search.contains(needle),
            "后端源码里没有 {needle:?} —— 线上词对不上了"
        );
    }
    assert_eq!(FIND_LIMIT, 500, "与后端缺省同值（上面那条从后端源码抠的）");
    let hist = include_str!("../../src/backend/observe/history_query.rs");
    for needle in [
        r#"Some("--find-in-session") =>"#,
        r#""--query" =>"#,
        r#""--include-tools" =>"#,
        r#""--limit" =>"#,
    ] {
        assert_eq!(
            hist.matches(needle).count(),
            1,
            "后端分派 / argv 里没有恰好一处 {needle:?}"
        );
    }
}

/// 查询自己失败（起不了本机后端 / ssh 连不上 / 老后端退出 2）⇒ 不可用、原因原样带给前端。
#[test]
fn a_failed_query_is_unavailable_with_its_reason() {
    let r = find_result(Err(crate::subagent::QueryError::transport("连不上".into())));
    assert!(!r.available);
    assert_eq!(r.reason.as_deref(), Some("连不上"));
    assert_eq!((r.total, r.hits.len()), (0, 0));
    let v = serde_json::to_value(find_result(Ok(l(&[
        HEAD,
        r#"{"kind":"session_find_end","count":0,"total":0}"#,
    ]))))
    .unwrap();
    assert_eq!(v["available"], true);
    assert!(v.get("reason").is_none(), "要到了就不出 reason 这个键");
}
