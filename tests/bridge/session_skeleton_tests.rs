//! 〔`设计/10` 骨架 · 子步 3〕monitor 侧「从偏移读」两条命令的纯函数判据。
//!
//! 买到：老后端（不认 `--index`、透传 jsonl）**认得出来**而不是被当成索引解析；截断的索引不当全量；
//! 按偏移取回的正文 seq 与索引对得上（空行/BOM 行不占号、不可显示的占号不出 payload）；
//! 老后端不认 `--until` 一路透传到 EOF 时**数够就停**。
//! **买不到**：真 SSH / 真本机后端那一圈（transport 本身由 `subagent::Backend` 的既有判据管）。

use super::*;

fn l(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_real_index_parses_head_rows_tail() {
    let out = l(&[
        r#"{"kind":"session_index","v":1,"from":0}"#,
        r#"{"o":0,"n":10,"t":"user","u":"a"}"#,
        r#"{"o":10,"n":5}"#,
        r#"{"kind":"session_index_end","count":2,"end":15}"#,
    ]);
    let (from, end, rows) = parse_index_output(&out).unwrap();
    assert_eq!((from, end, rows.len()), (0, 15, 2));
    assert_eq!(rows[0]["u"], "a");
}

/// 🔴 老后端：它不认 `--index`，照旧透传 jsonl 字节 —— 首行是一条**记录**，不是索引头。
#[test]
fn an_old_backend_dumping_jsonl_is_recognised_not_parsed_as_index() {
    let out = l(&[
        r#"{"type":"user","uuid":"x","message":{"role":"user","content":"hi"}}"#,
        r#"{"type":"assistant","uuid":"y"}"#,
    ]);
    assert_eq!(parse_index_output(&out), Err(IndexUnavailable::OldBackend));
    assert_eq!(
        parse_index_output(&[]),
        Err(IndexUnavailable::OldBackend),
        "空输出同档"
    );
}

#[test]
fn a_truncated_index_is_not_taken_as_complete() {
    // 有头没尾
    let no_tail = l(&[
        r#"{"kind":"session_index","v":1,"from":0}"#,
        r#"{"o":0,"n":1}"#,
    ]);
    assert_eq!(
        parse_index_output(&no_tail),
        Err(IndexUnavailable::Truncated {
            got: 0,
            claimed: None
        })
    );
    // 尾行条数对不上
    let short = l(&[
        r#"{"kind":"session_index","v":1,"from":0}"#,
        r#"{"o":0,"n":1}"#,
        r#"{"kind":"session_index_end","count":5,"end":1}"#,
    ]);
    assert_eq!(
        parse_index_output(&short),
        Err(IndexUnavailable::Truncated {
            got: 1,
            claimed: Some(5)
        })
    );
}

/// seq 对齐：第 k 个**可计行**是 `seq_base + k`；不可显示的占号不出 payload；空行 / BOM 行不占号。
#[test]
fn range_payload_seqs_line_up_with_the_index() {
    let lines = l(&[
        r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"hi"}}"#,
        "\u{feff}",
        r#"{"type":"permission-mode","permissionMode":"default"}"#,
        r#"{"type":"assistant","uuid":"a1","message":{"role":"assistant","content":[{"type":"text","text":"yo"}]}}"#,
    ]);
    let host = crate::origin::Origin("h".into());
    let got = range_payloads(&lines, 100, 3, "sid", "/p/sid.jsonl", &host);
    let seqs: Vec<u64> = got.iter().map(|p| p.seq).collect();
    assert_eq!(
        seqs,
        vec![100, 102],
        "permission-mode 占 101 但不出 payload"
    );
    assert_eq!(got[0].origin.as_deref(), Some("h"));
    assert_eq!(got[1].session_id, "sid");
}

/// 老后端不认 `--until`：一路透传到 EOF ⇒ **数够 `line_count` 就停**。
#[test]
fn range_stops_after_line_count_even_if_the_backend_overshoots() {
    let lines = l(&[
        r#"{"type":"user","uuid":"1","message":{"role":"user","content":"a"}}"#,
        r#"{"type":"user","uuid":"2","message":{"role":"user","content":"b"}}"#,
        r#"{"type":"user","uuid":"3","message":{"role":"user","content":"c"}}"#,
    ]);
    let got = range_payloads(
        &lines,
        0,
        2,
        "s",
        "/p/s.jsonl",
        &crate::origin::Origin::local(),
    );
    assert_eq!(got.len(), 2);
    assert_eq!(
        got[0].origin, None,
        "本机那条载荷不带 origin（与 live 行同一口径）"
    );
    assert_eq!(got[1].seq, 1);
}

#[test]
fn path_precheck_rejects_traversal_and_non_jsonl() {
    assert!(precheck("/a/../b.jsonl").is_err());
    assert!(precheck("/a/b.txt").is_err());
    assert!(precheck("/a/b.jsonl").is_ok());
}
