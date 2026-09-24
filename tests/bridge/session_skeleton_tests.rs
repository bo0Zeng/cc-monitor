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

/// 🔴 选项必须在位置参数**前面**：老后端只看 `args[1]`（路径）/`args[2]`（offset），若那两格恰好是
/// `<path> <offset>`，它会把整份会话透传回来（现打 50 955 695 字节）。
#[test]
fn argv_puts_options_first_so_old_backends_fail_with_zero_bytes() {
    // 索引：老后端拿 args[2]（路径）当 offset ⇒ 解析失败、零字节退出 2
    let idx = index_argv("/p/s.jsonl", 7);
    assert_eq!(
        idx,
        vec!["--read-session-from-offset", "--index", "/p/s.jsonl", "7"]
    );
    assert!(
        idx[2].parse::<u64>().is_err(),
        "老后端会把 args[2] 当 offset 成功解析：{idx:?}"
    );
    // 取正文：老后端拿 args[1]（`--until`）当路径 ⇒ 围栏拒、零字节退出 2
    let rng = range_argv("/p/s.jsonl", 7, 99);
    assert_eq!(
        rng,
        vec![
            "--read-session-from-offset",
            "--until",
            "99",
            "/p/s.jsonl",
            "7"
        ]
    );
    assert!(
        rng[1].starts_with("--"),
        "args[1] 不是选项 ⇒ 老后端会真去读：{rng:?}"
    );
}

fn seq_space_golden() -> (Vec<(u64, Option<String>)>, u64) {
    let golden = include_str!("../__fixtures__/skeleton-seq-space.golden");
    let mut want = Vec::new();
    let mut count = 0;
    for l in golden.lines() {
        if let Some(v) = l.strip_prefix("#count\t") {
            count = v.parse().unwrap();
        } else if !l.starts_with('#') {
            let (s, u) = l.split_once('\t').unwrap();
            want.push((s.parse().unwrap(), (u != "-").then(|| u.to_string())));
        }
    }
    assert!(
        !want.is_empty(),
        "金标准一行都没抽到 —— 下面的相等在空集上绿"
    );
    (want, count)
}

fn uuid_of(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("uuid")?
        .as_str()
        .map(str::to_string)
}

/// 〔U3b〕**跨 crate 的 seq 空间对拍**（monitor 这一侧）：本机历史读的切行法（`BufRead::lines`）
/// 与远端历史读的切行法（按 `\n` 切、带着行尾）喂给同一个 `LineNumberer`，都必须对上**同一份**
/// 金标准（后端索引在 `history_query_index_tests.rs` 对的也是它）。torn 残尾：本侧会给它编下一个号
/// （读路不认 torn），索引不列它 —— 号仍在同一空间里（== count）。
#[test]
fn both_history_readers_number_lines_in_the_index_seq_space() {
    use std::io::BufRead;
    let data: &[u8] = include_bytes!("../__fixtures__/skeleton-seq-space.jsonl");
    let (want, count) = seq_space_golden();
    // 本机那一支：`BufRead::lines`
    let mut n = LineNumberer::default();
    let local: Vec<(u64, Option<String>)> = std::io::BufReader::new(data)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| n.number(&l).map(|(s, b)| (s, uuid_of(b))))
        .collect();
    // 远端那一支：lossy 解码、按 `\n` 切、行尾留着
    let text = String::from_utf8_lossy(data);
    let mut n = LineNumberer::default();
    let remote: Vec<(u64, Option<String>)> = text
        .split_inclusive('\n')
        .filter_map(|l| n.number(l).map(|(s, b)| (s, uuid_of(b))))
        .collect();
    for (name, got) in [("本机", local), ("远端", remote)] {
        let (complete, torn) = got.split_at(want.len());
        assert_eq!(complete, want.as_slice(), "{name}那一支的编号与索引对不上");
        // torn 那行是半截 JSON ⇒ 取不出 uuid；要紧的是它的号 == count（仍在同一空间）
        assert_eq!(torn, &[(count, None)], "{name}：torn 残尾");
    }
}

/// 按偏移取回的那一段也在同一空间：可显示的出 payload、不可显示与非 JSON 的占号不出。
#[test]
fn range_payloads_on_the_golden_fixture() {
    let data: &[u8] = include_bytes!("../__fixtures__/skeleton-seq-space.jsonl");
    let text = String::from_utf8_lossy(data);
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let (want, count) = seq_space_golden();
    let got = range_payloads(
        &lines,
        0,
        count,
        "s",
        "/p/s.jsonl",
        &crate::origin::Origin::local(),
    );
    let seqs: Vec<u64> = got.iter().map(|p| p.seq).collect();
    // 手算：0 user · 1 permission-mode（不可显示）· 2 assistant · 3 system · 4 非 JSON · 5 user
    assert_eq!(seqs, vec![0, 2, 3, 5]);
    assert!(seqs.iter().all(|s| want.iter().any(|(w, _)| w == s)));
}

/// 〔U3b〕两条历史读路**真的走**那个「先占号、后过滤」的住址，而且手里不再有自己的计数器。
/// 金标准那一格测的是住址本身；这一格钉的是读路没绕开它（绕开 = 回到「可显示序号」、与索引对不上）。
#[test]
fn both_history_readers_go_through_the_one_numbering_home() {
    for (name, src) in [
        (
            "history.rs",
            include_str!("../../src/bridge/src/history.rs"),
        ),
        (
            "remote_history.rs",
            include_str!("../../src/bridge/src/remote_history.rs"),
        ),
    ] {
        let prod = guard_core::production_code(src);
        assert!(
            guard_core::find_pinned(&prod, "session_skeleton::numbered_displayable(").is_ok(),
            "{name}：没走（或不止一处走）`numbered_displayable`"
        );
        assert!(
            !guard_core::contains_word(&prod, "next_seq"),
            "{name}：读路里又长出了自己的 `next_seq` 计数器"
        );
    }
}
