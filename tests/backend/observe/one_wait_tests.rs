//! 一次性等待：真文件、真 inotify（期限内长出来 ⇒ 等到、不拖；不长 ⇒ 到点回、不早退；已有的不算；记录被改写也认得出）。

use super::*;
use std::io::Write as _;
use std::time::{Duration, Instant};

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-one-wait-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建夹具目录");
    d
}

/// 一条压缩摘要（结构同那一家写的那种，正文是占位）。
fn summary() -> String {
    r#"{"type":"user","isCompactSummary":true,"message":{"role":"user","content":"S"},"uuid":"u-s"}"#.to_string()
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

/// 夹具那一条确实是适配层认的压缩摘要（否则下面几条的「等到」是空的）。
#[test]
fn the_fixture_line_is_what_the_adapter_calls_a_compact_summary() {
    let v: serde_json::Value = serde_json::from_str(&summary()).unwrap();
    assert!(crate::agents::is_compact_summary(&v));
    let v: serde_json::Value = serde_json::from_str(&said(1)).unwrap();
    assert!(!crate::agents::is_compact_summary(&v));
}

/// 期限内写进一条压缩摘要 ⇒ 等到，且写进去之后很快就回（不拖到期限）。装之前就在的那一条摘要不算。
#[test]
fn a_summary_written_in_time_is_seen_promptly_and_an_old_one_does_not_count() {
    let d = scratch("seen");
    let rec = d.join("s.jsonl");
    append(&rec, &said(1));
    append(&rec, &summary());
    let armed = record_line(&rec, crate::agents::is_compact_summary).expect("装");
    let writer = {
        let rec = rec.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            append(&rec, &said(2));
            append(&rec, &summary());
        })
    };
    let t0 = Instant::now();
    let got = rt().block_on(armed.within(10_000));
    let took = t0.elapsed();
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(got, "写进了压缩摘要却没等到");
    assert!(took < Duration::from_millis(5_000), "等到了却拖到 {took:?}");

    // 只有旧的那条、期限内没有新的 ⇒ 没等到。
    let d = scratch("old");
    let rec = d.join("s.jsonl");
    append(&rec, &summary());
    let armed = record_line(&rec, crate::agents::is_compact_summary).expect("装");
    let got = rt().block_on(armed.within(300));
    let _ = std::fs::remove_dir_all(&d);
    assert!(!got, "装之前就在的那条被当成了新的");
}

/// 期限内没写 ⇒ 到点回「没等到」：不早退，也不拖。
#[test]
fn nothing_written_returns_at_the_deadline() {
    let d = scratch("late");
    let rec = d.join("s.jsonl");
    append(&rec, &said(1));
    let armed = record_line(&rec, crate::agents::is_compact_summary).expect("装");
    let writer = {
        let rec = rec.clone();
        std::thread::spawn(move || append(&rec, &said(2)))
    };
    let t0 = Instant::now();
    let got = rt().block_on(armed.within(600));
    let took = t0.elapsed();
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(!got);
    assert!(took >= Duration::from_millis(600), "早退：{took:?}");
    assert!(took < Duration::from_millis(3_000), "拖了：{took:?}");
}

/// 记录被整份换掉（改名换进来一份新的）⇒ 照流式那一套从头重读，新内容里的摘要认得出。
#[test]
fn a_rewritten_record_is_reread_from_the_start() {
    let d = scratch("rewrite");
    let rec = d.join("s.jsonl");
    for n in 0..20 {
        append(&rec, &said(n));
    }
    let armed = record_line(&rec, crate::agents::is_compact_summary).expect("装");
    let writer = {
        let (rec, tmp) = (rec.clone(), d.join("s.jsonl.tmp"));
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            append(&tmp, &summary());
            std::fs::rename(&tmp, &rec).unwrap();
        })
    };
    let got = rt().block_on(armed.within(10_000));
    writer.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(got, "换过的那份里有摘要，没认出来");
}

/// 会话由一个新进程报出：此刻已在跑的那个不算；新起的那个（活的、pidfile 认得是它）写下 pidfile ⇒ 等到。
#[cfg(target_os = "linux")]
#[test]
fn a_session_reported_by_a_new_process_is_seen_and_the_old_one_is_not() {
    let home = scratch("arrive");
    let dir = crate::observe::watcher::pidfile_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "aaaaaaaa-1111-2222-3333-444444444444";
    let pidfile = |child: &std::process::Child| {
        let pid = child.id();
        let ticks = crate::platform::proc::proc_starttime(pid).expect("读得到起始时刻");
        let body = format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}"}}"#
        );
        std::fs::write(dir.join(format!("{pid}.json")), body).unwrap();
    };
    let mut old = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    pidfile(&old);
    // 只有旧的那个 ⇒ 没等到。
    let armed = session_arrival(&home, sid).expect("装");
    assert!(!rt().block_on(armed.within(300)), "旧进程被当成了新报出的");
    // 新起一个、写下它的 pidfile ⇒ 等到。
    let armed = session_arrival(&home, sid).expect("装");
    let mut new = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    pidfile(&new);
    let got = rt().block_on(armed.within(10_000));
    for c in [&mut old, &mut new] {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = std::fs::remove_dir_all(&home);
    assert!(got, "新进程报出了却没等到");
}
