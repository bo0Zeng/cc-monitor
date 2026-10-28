//! `history-read`（连同借它出第一页的 `history-tail`）一页按**交出去的字节**封顶，不按读过的源字节：
//! 尾段里夹着几十 MB 不进界面的大行，冷开照样一页交完；行多到一页装不下时照 `next` 接起来与一次读完逐行相同，
//! 每一条应答不过上限。夹具只造结构（uuid · 类型 · 行长），正文是占位字。

use super::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("a4522-cap-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects").join("-p")).unwrap();
    d
}

fn write(home: &Path, sid: &str, text: &str) -> String {
    let p = home
        .join("projects")
        .join("-p")
        .join(format!("{sid}.jsonl"));
    std::fs::write(&p, text).unwrap();
    p.to_string_lossy().into_owned()
}

fn user(sid: &str, k: usize, pad: usize) -> String {
    json!({"type":"user","sessionId":sid,"uuid":format!("u-{k}"),"timestamp":"2026-10-12T00:00:00.000Z","cwd":"/w","message":{"role":"user","content":format!("p{k} {}", "x".repeat(pad))}})
        .to_string()
}

/// 照真记录里那一形造的 `api-request`：请求参数 ＋ 一大截占位（真记录里是整份请求体的摘要与尾巴）。
fn api_request(sid: &str, k: usize, size: usize) -> String {
    json!({"type":"api-request","sessionId":sid,"id":format!("req-{k}"),"timestamp":"2026-10-12T00:00:00.000Z","version":"2.1.0","querySource":"repl_main_thread","params":{"model":"m","max_tokens":1},"tail":["y".repeat(size)]})
        .to_string()
}

/// 一条应答那一行的字节数（同生产写出去的那一形：回包正文 ＋ 信封余量）。
fn line_len<T: serde::Serialize>(v: &T) -> usize {
    serde_json::to_vec(v).unwrap().len() + ENVELOPE_BYTES
}

/// 壳冷开的问法：`history-tail`，没读到头就照 `next` 用 `history-read` 接到 `plan.end`。回：每一页的行。
fn cold_open(home: &Path, path: &str, n: u64) -> Vec<Vec<Value>> {
    let tz = crate::Tz::default();
    let t = tail_at(
        home,
        HistoryTailReq {
            path: path.into(),
            n,
            held: None,
        },
        &tz,
    )
    .unwrap();
    assert!(
        t.rows.len() <= 1 || line_len(&t) <= READ_PAGE_BYTES,
        "history-tail 一条应答 {} 字节，过了上限",
        line_len(&t)
    );
    let mut pages = vec![t.rows.iter().map(|r| json!(r)).collect::<Vec<_>>()];
    let (mut at, mut eof, end) = (t.next, t.eof, t.plan.end);
    while !eof && at < end {
        let r = read_at(
            home,
            HistoryReadReq {
                path: path.into(),
                offset: Some(at),
                until: Some(end),
            },
            &tz,
        )
        .unwrap();
        assert!(
            r.rows.len() <= 1 || line_len(&r) <= READ_PAGE_BYTES,
            "history-read 一条应答 {} 字节，过了上限",
            line_len(&r)
        );
        assert!(r.next > at, "续点没往前走");
        pages.push(r.rows.iter().map(|r| json!(r)).collect());
        (at, eof) = (r.next, r.eof);
    }
    pages
}

/// 尾段 600 行里夹 20 行各 1 MB 的 `api-request`（不进界面，照占号）：交出去的不到一页 ⇒ 冷开一页交完。
#[test]
fn a_tail_holding_big_hidden_lines_comes_in_one_page() {
    let home = scratch("tail");
    let sid = "s-cap-tail";
    let mut text = String::new();
    for k in 0..700 {
        let line = if k >= 100 && k % 30 == 0 {
            api_request(sid, k, 1 << 20)
        } else {
            user(sid, k, 40)
        };
        text.push_str(&line);
        text.push('\n');
    }
    let path = write(&home, sid, &text);
    let pages = cold_open(&home, &path, 600);
    assert_eq!(
        pages.len(),
        1,
        "冷开尾段 {} 页（交出去的不到一页，应当一页）",
        pages.len()
    );
    assert_eq!(pages[0].len(), 600);
}

/// 交出去的比一页多：照 `next` 一页一页接，每页不过上限，拼起来与一次读完逐行相同（字节位置 · 摘要 · 记录）。
#[test]
fn pages_cut_by_delivered_bytes_add_up_to_one_whole_read() {
    let home = scratch("whole");
    let sid = "s-cap-whole";
    let mut text = String::new();
    for k in 0..400 {
        let line = if k % 50 == 7 {
            api_request(sid, k, 300 << 10)
        } else {
            user(sid, k, 8 << 10)
        };
        text.push_str(&line);
        text.push('\n');
    }
    let path = write(&home, sid, &text);
    let pages = cold_open(&home, &path, 400);
    assert!(pages.len() > 1, "夹具要比一页大");
    let got: Vec<Value> = pages.into_iter().flatten().collect();

    let tz = crate::Tz::default();
    let (_, face) = record_face(&home, &path).unwrap();
    let whole = body::read_page(&home, &path, 0, None, usize::MAX >> 1, LINE_CAP_BYTES).unwrap();
    let want: Vec<Value> = body::rows_of(&mut reader(&face, &tz), 0, &whole.bytes)
        .iter()
        .map(|r| json!(r))
        .collect();
    assert_eq!(got.len(), want.len());
    assert_eq!(got, want);
}

/// 一行单独比一页大 ⇒ 那一页只有它，续点照常往前走。
#[test]
fn a_row_bigger_than_a_page_goes_alone() {
    let home = scratch("alone");
    let sid = "s-cap-alone";
    let text = [user(sid, 0, 10), user(sid, 1, 2 << 20), user(sid, 2, 10)]
        .map(|l| l + "\n")
        .concat();
    let path = write(&home, sid, &text);
    let tz = crate::Tz::default();
    let ask = |at: u64| {
        read_at(
            &home,
            HistoryReadReq {
                path: path.clone(),
                offset: Some(at),
                until: None,
            },
            &tz,
        )
        .unwrap()
    };
    let a = ask(0);
    assert_eq!(a.rows.len(), 1);
    let b = ask(a.next);
    assert_eq!(b.rows.len(), 1, "大的那一行单独一页");
    let c = ask(b.next);
    assert_eq!((c.rows.len(), c.eof), (1, true));
}
