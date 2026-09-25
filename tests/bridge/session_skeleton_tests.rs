//! 〔`设计/10` 骨架 · 子步 3〕monitor 侧「从偏移读」的纯函数判据。
//!
//! 〔C4b〕索引那一半（老后端认不认得出、截断的索引不当全量）随 `read_session_index` 改走通道删了：
//! 帧面一帧是原子的、后端直接出成品，「有头没尾」那一形在帧面上不存在。
//! 买到：按偏移取回的正文 seq 与索引对得上（空行/BOM 行不占号、不可显示的占号不出 payload）；
//! 老后端不认 `--until` 一路透传到 EOF 时**数够就停**。
//! **买不到**：真 SSH / 真本机后端那一圈（transport 本身由 `subagent::Backend` 的既有判据管）。

use super::*;

fn l(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
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
/// 〔C4b〕索引那一半随那条命令改走通道删了（帧面不走 argv）；取正文这一半照旧。
#[test]
fn argv_puts_options_first_so_old_backends_fail_with_zero_bytes() {
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

fn st3_booked(o: &crate::origin::Origin, face: crate::drift_ledger::DriftFace, key: &str) -> bool {
    crate::drift_ledger::snapshot(o)
        .into_iter()
        .any(|f| f.face == face && f.entries.iter().any(|e| e.key == key))
}

/// 〔ST3〕★ 接缝：按偏移取回的正文里看不懂的行，记在**所问那台**名下（本机 / 远端同一条路）。
#[test]
fn unreadable_range_lines_are_booked_under_the_asked_origin() {
    use crate::drift_ledger::DriftFace;
    let aya = crate::origin::Origin("st3-range-probe".into());
    let lines = l(&[r#"{"type":"st3-range-seam-probe"}"#]);
    let _ = range_payloads(&lines, 0, 1, "sid", "/p/sid.jsonl", &aya);
    assert!(
        st3_booked(&aya, DriftFace::UnknownRecordType, "st3-range-seam-probe"),
        "没记在所问那台名下"
    );
    assert!(
        !st3_booked(
            &crate::origin::Origin::local(),
            DriftFace::UnknownRecordType,
            "st3-range-seam-probe"
        ),
        "远端取回的行记进了本机那一本"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  〔CF2 · 第四波 4B〕按行号取回（`read_session_lines` / `frame_query::session_lines`）
//
//  要求住址：`设计/99 §4.4`「无索引会话的重放缓冲上界（要先有不依赖索引的取回路）」· `设计/05 §3.3.4`
//  「⇒ **级 3 是判据**：任何一个订阅侧缓冲都要有上界」—— 取回路的行号若与实时 seq 不在一个空间，
//  取回的正文就落错位置；那条上界也就只剩「丢」没有「回」。
// ════════════════════════════════════════════════════════════════════════════

/// ★ L2：后端交来的可计行（`from = 1` 起那一段）⇒ payload 的 seq == 行号，不可显示与非 JSON 的照占号不出。
/// 期望**手算**（同 `range_payloads_on_the_golden_fixture` 那一行注释：0 user · 1 permission-mode · 2 assistant ·
/// 3 system · 4 非 JSON · 5 user）；「哪些行可计」按金标准头注手写的四种空行剔掉，不借被测的 `LineNumberer`。
#[test]
fn lines_by_number_land_on_their_own_seq() {
    let text = String::from_utf8_lossy(include_bytes!("../__fixtures__/skeleton-seq-space.jsonl"));
    let (want, count) = seq_space_golden();
    let countable: Vec<String> = text
        .split('\n')
        .filter(|l| {
            let b = l.trim_start_matches('\u{feff}');
            !b.chars().all(|c| c.is_whitespace())
        })
        .map(str::to_string)
        .collect();
    // torn 残尾（最后一段没有 `\n`）后端不交 ⇒ 这里也不交。
    let countable = &countable[..count as usize];
    assert_eq!(
        countable.len(),
        want.len(),
        "夹具的可计行数与金标准对不上 —— 夹具变了"
    );
    let page = crate::backend::control::frame_query::LinesPage {
        from: 1,
        lines: countable[1..].to_vec(),
        next: count,
        eof: true,
    };
    let got = lines_page(page, "/p/s.jsonl", &crate::origin::Origin::local());
    assert_eq!(
        got.payloads.iter().map(|p| p.seq).collect::<Vec<_>>(),
        vec![2, 3, 5]
    );
    assert!(got.payloads.iter().all(|p| p.session_id == "s"));
    assert_eq!((got.from, got.next, got.eof), (1, count, true));
}

/// ★ L2：应答对不上就报错（不落错行号）：缺键 · `from` 不是问的 · `next` 与条数不符 · 没到头却一条没交。
/// 正控：一份自洽的应答照收。
#[test]
fn a_lines_answer_that_contradicts_itself_is_refused() {
    use crate::backend::control::frame_query::parse_session_lines;
    use serde_json::json;
    let o = crate::origin::Origin::local();
    let ok = json!({"from": 3, "next": 5, "eof": false, "lines": ["a", "b"]});
    let got = parse_session_lines(&o, 3, &ok).expect("自洽的应答被拒");
    assert_eq!(
        (got.from, got.next, got.eof, got.lines.len()),
        (3, 5, false, 2)
    );
    for (bad, why) in [
        (json!({"from": 3, "next": 5, "eof": false}), "缺 lines"),
        (
            json!({"from": 4, "next": 6, "eof": false, "lines": ["a", "b"]}),
            "from 不是问的",
        ),
        (
            json!({"from": 3, "next": 6, "eof": false, "lines": ["a", "b"]}),
            "next 与条数不符",
        ),
        (
            json!({"from": 3, "next": 3, "eof": false, "lines": []}),
            "没到头却一条没交",
        ),
        (
            json!({"from": 3, "next": 5, "eof": false, "lines": ["a", 7]}),
            "lines 里有非字符串",
        ),
    ] {
        assert!(parse_session_lines(&o, 3, &bad).is_err(), "{why} 被收下了");
    }
    // 到头了、一条没交 ⇒ 合法（问的那一行已经在末尾之后）
    assert!(parse_session_lines(
        &o,
        3,
        &json!({"from": 3, "next": 3, "eof": true, "lines": []})
    )
    .is_ok());
}
