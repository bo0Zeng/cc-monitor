//! 整台一张活会话表：现扫那一份（pidfile ＋ 判活）与几个读口的判法。

use super::*;
use crate::agents::SessionActivity as A;
use std::fs;

#[cfg(unix)]
#[test]
fn the_scan_keeps_the_pidfiles_whose_process_is_still_there_as_live() {
    let home = std::env::temp_dir().join(format!("ccm-live-table-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let sessions = home.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let me = std::process::id();
    fs::write(
        sessions.join(format!("{me}.json")),
        r#"{"sessionId":"live-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("4194304.json"),
        r#"{"sessionId":"dead-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("notapid.json"),
        r#"{"sessionId":"ignored","cwd":"/w"}"#,
    )
    .unwrap();
    let t = LiveTable::scan(&home);
    assert_eq!(
        t.sids(),
        ["live-sid".to_string()].into_iter().collect(),
        "活着的那一个要在、死了的那一个不许在"
    );
    assert_eq!(
        t.rows().len(),
        2,
        "死了的那一行也在表里（账号归属要说它死了）"
    );
    // 没有 sessionId 的活进程不算（它说不出是哪个会话）。
    fs::write(sessions.join(format!("{me}.json")), r#"{"cwd":"/w"}"#).unwrap();
    assert!(LiveTable::scan(&home).sids().is_empty());
    let _ = fs::remove_dir_all(&home);
}

fn row(pid: u32, sid: &str, activity: Option<A>, frozen: bool) -> LiveRow {
    LiveRow {
        pid,
        fact: crate::agents::ProcFact {
            sid: Some(sid.to_string()),
            activity,
            ..Default::default()
        },
        alive: true,
        start_checked: true,
        frozen,
    }
}

#[test]
fn a_pending_ask_makes_a_live_session_need_you_whatever_its_pidfile_says() {
    let t = of_rows(vec![row(1, "s", Some(A::Working), false)], &["s", "gone"]);
    assert_eq!(t.doing_one("s"), Some(Some(A::NeedsYou)));
    assert_eq!(
        t.needs(),
        vec!["s".to_string()],
        "挂着请求、已经不活的那条不算"
    );
    assert!(t.waiting("s"));
    assert_eq!(t.doing_one("gone"), None);
}

#[test]
fn a_frozen_busy_process_is_not_said_to_be_working() {
    let t = of_rows(
        vec![
            row(1, "s", Some(A::Working), true),
            row(2, "s", Some(A::Idle), false),
        ],
        &[],
    );
    assert_eq!(
        t.doing_one("s"),
        Some(Some(A::Idle)),
        "冻住的那一个不说在跑，取另一个"
    );
    assert_eq!(t.doing().get("s"), Some(&Some(A::Idle)));
}

#[test]
fn several_processes_take_the_most_urgent_state() {
    let t = of_rows(
        vec![
            row(1, "s", Some(A::Idle), false),
            row(2, "s", Some(A::NeedsYou), false),
        ],
        &[],
    );
    assert_eq!(t.doing_one("s"), Some(Some(A::NeedsYou)));
    assert_eq!(t.writers("s"), vec![1, 2]);
}

fn of_rows(rows: Vec<LiveRow>, asked: &[&str]) -> LiveTable {
    LiveTable {
        home: std::path::PathBuf::new(),
        rows,
        asked: asked.iter().map(|s| (*s).to_string()).collect(),
        reconnectable: Default::default(),
    }
}
