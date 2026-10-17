//! `history-lines` 的字节位置与 `history-tail` 怎么接：按行号补完之后手上的续点（路径 · 字节 · 行号）三样对得上，
//! 续传 · 变短 · 被改写 · 缺得多各回一形。夹具只造结构（占位字）。

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

/// `history-tail` 一问交回「怎么接」：续得上 ⇒ 从交来的 `held.next` 那一行起；变短 · 被改写 · 换了一份 ⇒ 尾段并说为什么；
/// 缺得比 `n` 多 ⇒ 那一截不读；换了路径 ⇒ 尾段。每一形都带第一页正文与读到头之后要存的 `held`。扫描图留着那一支给同一个答案。
#[test]
fn the_tail_says_how_to_go_on_from_what_the_caller_holds() {
    let home = scratch("tail");
    let tz = crate::Tz::default();
    let text =
        |ks: std::ops::Range<usize>| ks.map(|k| format!("{}\n", user(k))).collect::<String>();
    let path = write(&home, "s-tail", &format!("\n{}", text(0..4)));
    let ask = |n: u64, held: Option<Held>| {
        tail_at(
            &home,
            HistoryTailReq {
                path: path.clone(),
                n,
                held,
            },
            &tz,
        )
        .unwrap()
    };
    let seqs = |t: &HistoryTail| (t.from..t.from + t.rows.len() as u64).collect::<Vec<_>>();

    // 手上什么都没有 ⇒ 尾段；读到头之后存的那一份停在文件尾。
    let first = ask(2, None);
    assert_eq!(
        (first.how, first.from, seqs(&first)),
        (TailHow::Tail, 2, vec![2, 3])
    );
    assert!(first.eof);
    assert_eq!((first.held.next, first.held.next_byte), (4, first.plan.end));
    assert!(!first.held.mark.is_empty());

    // 又长了两行 ⇒ 从第 4 行接着读，一行不多。
    std::fs::write(&path, format!("\n{}", text(0..6))).unwrap();
    let resumed = ask(2, Some(first.held.clone()));
    assert_eq!((resumed.how, seqs(&resumed)), (TailHow::Resume, vec![4, 5]));
    // 扫描图留着那一支：同一个答案。
    let _ = crate::observe::history_query::cold_scan(&home, &path);
    let again = ask(2, Some(first.held.clone()));
    assert_eq!(
        (again.how, seqs(&again), &again.held),
        (TailHow::Resume, vec![4, 5], &resumed.held)
    );

    // 缺的比 `n` 多 ⇒ 那一截不读，从文件尾接着来。
    let far = ask(1, Some(first.held.clone()));
    assert_eq!(
        (far.how, far.from, far.rows.len()),
        (TailHow::Handoff, 6, 0)
    );
    assert_eq!(far.held, resumed.held);

    // 换了路径 ⇒ 尾段，不说变了。
    let other = Held {
        path: "/elsewhere.jsonl".into(),
        ..first.held.clone()
    };
    assert_eq!(ask(2, Some(other)).how, TailHow::Tail);

    // 变短了（比交来的那一份短）⇒ 尾段、说变短。
    let held6 = resumed.held.clone();
    std::fs::write(&path, format!("\n{}", text(0..3))).unwrap();
    let short = ask(2, Some(held6.clone()));
    assert_eq!((short.how, seqs(&short)), (TailHow::Truncated, vec![1, 2]));

    // 同样长、记号那一行换了内容（被改写）⇒ 尾段、说被改写。
    let same_len = format!("\n{}{}", text(0..5), user(9) + "\n");
    assert_eq!(same_len.len(), format!("\n{}", text(0..6)).len());
    std::fs::write(&path, &same_len).unwrap();
    assert_eq!(ask(2, Some(held6.clone())).how, TailHow::Rewritten);

    // 换了一份、而且更长（头一行起就不同）⇒ 同上。
    std::fs::write(
        &path,
        format!(
            "\n{}",
            (10..20)
                .map(|k| format!("{}\n", user(k)))
                .collect::<String>()
        ),
    )
    .unwrap();
    let replaced = ask(2, Some(held6));
    assert_eq!(
        (replaced.how, seqs(&replaced)),
        (TailHow::Rewritten, vec![8, 9])
    );

    // 一行都没有 ⇒ 记号是空串、下一次不核。
    std::fs::write(&path, "\n\n").unwrap();
    let empty = ask(2, None);
    assert_eq!(
        (empty.how, empty.held.next, empty.held.mark.as_str()),
        (TailHow::Tail, 0, "")
    );
}
