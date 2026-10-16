//! 读行那一处（`observe::lines`）：一行行读到最后一个完整行、算不算一行、一行怎么解析。

use super::{counts, parse, Lines};

/// 读出来的每一行：（起点 · 不含换行的那一截）。
fn rows(bytes: &[u8], from: u64, upto: Option<u64>) -> (Vec<(u64, Vec<u8>)>, u64) {
    let mut l = Lines::new(&bytes[from as usize..], from);
    if let Some(u) = upto {
        l = l.upto(u);
    }
    let mut out = Vec::new();
    while let Some(row) = l.next().expect("内存里读不会错") {
        out.push((row.at, row.body().to_vec()));
    }
    (out, l.end())
}

#[test]
fn only_whole_lines_come_out_and_end_stops_before_the_torn_tail() {
    let bytes = b"{\"a\":1}\n\n{\"b\":2}\r\n{\"c\":";
    let (got, end) = rows(bytes, 0, None);
    assert_eq!(
        got,
        vec![
            (0, b"{\"a\":1}".to_vec()),
            (8, b"".to_vec()),
            (9, b"{\"b\":2}\r".to_vec()),
        ]
    );
    assert_eq!(end, 18, "残尾不计进 end");
}

#[test]
fn reading_from_the_middle_keeps_file_offsets_and_upto_stops_there() {
    let bytes = b"aa\nbbb\ncccc\ndd\n";
    let (got, end) = rows(bytes, 3, None);
    assert_eq!(got.first().map(|r| r.0), Some(3));
    assert_eq!(end, bytes.len() as u64);
    // 读到 12 为止：第三行（7..12）在里面，第四行不读。
    let (got, end) = rows(bytes, 3, Some(12));
    assert_eq!(got, vec![(3, b"bbb".to_vec()), (7, b"cccc".to_vec())]);
    assert_eq!(end, 12);
    // 读到行中间为止：那半行不出、不计。
    let (got, end) = rows(bytes, 0, Some(5));
    assert_eq!(got, vec![(0, b"aa".to_vec())]);
    assert_eq!(end, 3);
}

#[test]
fn a_line_counts_unless_it_is_only_bom_and_whitespace() {
    for blank in [
        "",
        " ",
        "\t\r",
        "\u{feff}",
        "\u{feff}  \u{000b}",
        "\u{3000}",
    ] {
        assert!(!counts(blank.as_bytes()), "{blank:?} 不算一行");
    }
    for line in ["{}", " x ", "\u{feff}{}", "中"] {
        assert!(counts(line.as_bytes()), "{line:?} 算一行");
    }
}

/// 行帧的行号（watcher 数的）与尾段图的总行数（扫描图数的）是同一个判定：同一份字节两边数出来一样。
#[test]
fn the_live_line_numbers_and_the_scan_count_agree() {
    let bytes = "{\"a\":1}\n\n\u{feff} \n\u{000b}\r\n{\"b\":2}\r\n\u{3000}\n{\"c\":3}\n".as_bytes();
    let (got, _) = rows(bytes, 0, None);
    let scanned = got.iter().filter(|(_, b)| counts(b)).count();
    let mut seqs = crate::wire::SeqCounter::new();
    let (live, _) = crate::observe::watcher::count_new_lines_at(
        bytes,
        0,
        bytes.len() as u64,
        crate::observe::watcher::ReadCursor::default(),
        "k",
        &mut seqs,
    );
    assert_eq!(scanned, 3);
    assert_eq!(live, scanned);
}

#[test]
fn parse_strips_bom_and_whitespace_and_refuses_what_is_not_json() {
    assert_eq!(
        parse(b"\xef\xbb\xbf {\"a\":1} \r"),
        Some(serde_json::json!({"a": 1}))
    );
    assert_eq!(parse(b"{\"a\":"), None);
    assert_eq!(parse(b""), None);
}
