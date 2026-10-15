//! `history-lines` 的字节位置与 `history-tail` 的文件身份：按行号补完之后手上的续点（路径 · 字节 · 行号）三样对得上，
//! 同一路径换了一份文件认得出来。夹具只造结构（占位字）。

use super::*;
use serde_json::json;
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mobbe2b-bytes-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects").join("-p")).unwrap();
    d
}

fn user(k: usize) -> String {
    json!({"type":"user","uuid":format!("u-{k}"),"timestamp":"2026-10-12T00:00:00.000Z","message":{"role":"user","content":format!("p{k}")}})
        .to_string()
}

fn write(home: &Path, sid: &str, text: &str) -> String {
    let p = home
        .join("projects")
        .join("-p")
        .join(format!("{sid}.jsonl"));
    std::fs::write(&p, text).unwrap();
    p.to_string_lossy().into_owned()
}

/// 每一行带它之后那个字节；回包的 `end` 是这一段最后一个可计行之后（中间的空行、不进界面的行照占位）。
#[test]
fn every_line_carries_its_end_and_the_reply_ends_after_the_last_counted_line() {
    let home = scratch("lines");
    // 第 1 行之后夹一个空行（不占号），第 3 行是不进界面的那一种（照占号），末尾一截残尾。
    let rows = [user(0), user(1), String::new(), user(2), json!({"type":"file-history-snapshot","messageId":"m","snapshot":{},"isSnapshotUpdate":false}).to_string(), user(3)];
    let mut text = rows.join("\n");
    text.push('\n');
    text.push_str("{\"torn\":");
    let path = write(&home, "s-lines", &text);
    let tz = crate::Tz::default();
    let page = lines_at(
        &home,
        HistoryLinesReq {
            path: path.clone(),
            from: Some(1),
            until: Some(4),
        },
        &tz,
    )
    .unwrap();
    let bytes = text.as_bytes();
    let after = |needle: &str| (text.find(needle).unwrap() + needle.len() + 1) as u64;
    assert_eq!(page.from, 1);
    assert_eq!(page.next, 4);
    for l in &page.lines {
        let end = l.end.expect("history-lines 的行都是完整行");
        assert_eq!(
            bytes[end as usize - 1],
            b'\n',
            "第 {} 行的 end 不在行尾之后",
            l.seq
        );
    }
    let ends: Vec<u64> = page.lines.iter().filter_map(|l| l.end).collect();
    assert!(
        ends.contains(&after(&rows[1])) && ends.contains(&after(&rows[3])),
        "{ends:?}"
    );
    assert_eq!(
        page.end,
        after(&rows[4]),
        "回包的 end ＝ 第 3 行（不进界面）之后"
    );

    // 从回包的 end 接着按字节读：第一条正好是第 4 行。
    let rest = read_at(
        &home,
        HistoryReadReq {
            path: path.clone(),
            offset: Some(page.end),
            until: None,
        },
        &tz,
    )
    .unwrap();
    let first = rest.rows.iter().find(|r| r.record.is_some()).unwrap();
    assert_eq!(first.end, Some(after(&rows[5])));

    // 起点越过了最后一行：一行都不交，`end` 停在最后一个可计行之后（残尾不算）。
    let past = lines_at(
        &home,
        HistoryLinesReq {
            path,
            from: Some(40),
            until: None,
        },
        &tz,
    )
    .unwrap();
    assert!(past.lines.is_empty());
    assert_eq!(past.end, after(&rows[5]));
}

/// 文件身份：头一个可计行没变 ⇒ 同一个；同一路径换成另一份（长度一样、头一行不同）⇒ 不同；一行都没有 ⇒ 空串。
/// 扫描图留着那一支（`peek` 命中）与现数那一支给同一个值。
#[test]
fn the_tail_names_the_file_by_its_first_line() {
    let home = scratch("tail");
    let a = format!("\n{}\n{}\n", user(0), user(1));
    let path = write(&home, "s-tail", &a);
    let req = |n| HistoryTailReq {
        path: path.clone(),
        n,
    };
    let first = tail_at(&home, req(1)).unwrap();
    assert_eq!(first.file.len(), 16, "{:?}", first.file);
    assert_eq!(
        tail_at(&home, req(5)).unwrap().file,
        first.file,
        "要几行不改身份"
    );
    std::fs::write(&path, format!("{a}{}\n", user(2))).unwrap();
    assert_eq!(
        tail_at(&home, req(1)).unwrap().file,
        first.file,
        "长了一行还是那一份"
    );
    // 同长的另一份：头一行换了。
    let b = format!("\n{}\n{}\n", user(7), user(1));
    assert_eq!(a.len(), b.len());
    std::fs::write(&path, &b).unwrap();
    let other = tail_at(&home, req(1)).unwrap();
    assert_ne!(other.file, first.file);
    // 扫描图那一支：扫过一遍留着之后再问，同一个值。
    let _ = crate::observe::history_query::cold_scan(&home, &path);
    assert_eq!(tail_at(&home, req(1)).unwrap().file, other.file);
    std::fs::write(&path, "\n\n").unwrap();
    assert_eq!(tail_at(&home, req(1)).unwrap().file, "");
}
