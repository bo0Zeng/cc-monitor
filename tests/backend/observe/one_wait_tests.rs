//! 一次性等待：真文件、真 inotify（期限内长出来 ⇒ 等到、不拖；不长 ⇒ 到点回、不早退；已有的不算；记录被改写也认得出）。

use super::*;
use std::io::Write as _;
use std::time::{Duration, Instant};

/// 等的那一条（带 `mark`）。
fn marked() -> String {
    r#"{"type":"user","mark":true,"uuid":"u-m"}"#.to_string()
}

fn said(n: usize) -> String {
    format!(r#"{{"type":"user","message":{{"role":"user","content":"x{n}"}},"uuid":"u{n}"}}"#)
}

fn append(path: &std::path::Path, line: &str) {
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .expect("开夹具");
    writeln!(f, "{line}").expect("写夹具");
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("起运行时")
}

/// 等带 `mark` 的那一条。
fn arm_mark(rec: &std::path::Path) -> Armed<()> {
    record_pick(rec, |v| {
        v.get("mark")
            .and_then(serde_json::Value::as_bool)
            .filter(|b| *b)
            .map(|_| ())
    })
    .expect("装")
}

/// 期限内写进那一条 ⇒ 等到，且写进去之后很快就回（不拖到期限）。装之前就在的那一条不算。
#[test]
fn a_line_written_in_time_is_seen_promptly_and_an_old_one_does_not_count() {
    let d = test_home::scratch("seen");
    let rec = d.join("s.jsonl");
    append(&rec, &said(1));
    append(&rec, &marked());
    let armed = arm_mark(&rec);
    let writer = {
        let rec = rec.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            append(&rec, &said(2));
            append(&rec, &marked());
        })
    };
    let t0 = Instant::now();
    // 期限放到兜底上限的两倍：等到了却没当场回，就要等满它、撞下面那道（兜底的数）。
    let got = rt().block_on(armed.fetch(2_000 * guard_core::wall::CEILING_SECS));
    let took = t0.elapsed();
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(got.is_some(), "写进了那一条却没等到");
    assert!(took < guard_core::wall::CEILING, "等到了却拖到 {took:?}");

    // 只有旧的那条、期限内没有新的 ⇒ 没等到。
    let d = test_home::scratch("old");
    let rec = d.join("s.jsonl");
    append(&rec, &marked());
    let armed = arm_mark(&rec);
    let got = rt().block_on(armed.fetch(300));
    let _ = std::fs::remove_dir_all(&d);
    assert!(got.is_none(), "装之前就在的那条被当成了新的");
}

/// 期限内没写 ⇒ 到点回「没等到」：不早退，也不拖。
#[test]
fn nothing_written_returns_at_the_deadline() {
    let d = test_home::scratch("late");
    let rec = d.join("s.jsonl");
    append(&rec, &said(1));
    let armed = arm_mark(&rec);
    let writer = {
        let rec = rec.clone();
        std::thread::spawn(move || append(&rec, &said(2)))
    };
    let t0 = Instant::now();
    let got = rt().block_on(armed.fetch(600));
    let took = t0.elapsed();
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(got.is_none());
    assert!(took >= Duration::from_millis(600), "早退：{took:?}");
    assert!(took < guard_core::wall::CEILING, "拖了：{took:?}");
}

/// 记录被整份换掉（改名换进来一份新的）⇒ 照流式那一套从头重读，新内容里的那一条认得出。
#[test]
fn a_rewritten_record_is_reread_from_the_start() {
    let d = test_home::scratch("rewrite");
    let rec = d.join("s.jsonl");
    for n in 0..20 {
        append(&rec, &said(n));
    }
    let armed = arm_mark(&rec);
    let writer = {
        let (rec, tmp) = (rec.clone(), d.join("s.jsonl.tmp"));
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            append(&tmp, &marked());
            std::fs::rename(&tmp, &rec).unwrap();
        })
    };
    // 期限放到兜底上限的两倍：等到了却没当场回，就要等满它、撞下面那道（兜底的数）。
    let got = rt().block_on(armed.fetch(2_000 * guard_core::wall::CEILING_SECS));
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(got.is_some(), "换过的那份里有那一条，没认出来");
}

/// ★ 等到的是取出来的那一份：此刻之后长出的、第一条取得出的；装之前就在的不算。
#[test]
fn a_pick_hands_back_what_the_new_line_carries() {
    let d = test_home::scratch("pick");
    let rec = d.join("s.jsonl");
    append(&rec, r#"{"n":1}"#);
    let armed = record_pick(&rec, |v| v.get("n").and_then(serde_json::Value::as_u64)).expect("装");
    let writer = {
        let rec = rec.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            append(&rec, r#"{"other":true}"#);
            append(&rec, r#"{"n":7}"#);
        })
    };
    let got = rt().block_on(armed.fetch(2_000 * guard_core::wall::CEILING_SECS));
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(got, Some(7));
}
