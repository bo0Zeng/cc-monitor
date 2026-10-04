//! 额度账：回包读数 → 按号记账 → 落盘 → 读帧，逐字段对得上；只在显示变了时推。

use super::*;
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-quota-ledger-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("mkdir");
    p
}

fn reading(used_5h: f64, refused: bool) -> QuotaReading {
    QuotaReading {
        status: Some(if refused {
            QuotaStatus::Rejected
        } else {
            QuotaStatus::Allowed
        }),
        refused,
        limiting: Some("five_hour".into()),
        resets_at: Some(1_800_003_600),
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(used_5h),
            resets_at: Some(1_800_003_600),
            warned_at: None,
        }],
        overage: None,
    }
}

fn ringing(path: Option<PathBuf>) -> (Ledger, tokio::sync::watch::Receiver<u64>) {
    let tx = std::sync::Arc::new(tokio::sync::watch::channel::<u64>(0).0);
    let rx = tx.subscribe();
    (Ledger::at(path).ringing(tx), rx)
}

fn on_disk(path: &std::path::Path) -> Book {
    match read_at(path) {
        Read::Present(b) => b,
        other => panic!("盘上应当读得懂，实得 {other:?}"),
    }
}

#[test]
fn a_reading_lands_under_its_account_on_disk_and_on_the_wire_field_for_field() {
    let d = temp_dir("land");
    let path = d.join(FILE_NAME);
    let (l, rx) = ringing(Some(path.clone()));
    record_seen(&l, "claude-code", "q", reading(0.42, false), 1_800_000_000);
    let want = Observed {
        agent: "claude-code".into(),
        account: "q".into(),
        seen_at: 1_800_000_000,
        reading: reading(0.42, false),
    };
    assert_eq!(on_disk(&path).accounts, vec![want.clone()]);
    assert_eq!(l.entry("claude-code", "q"), Some(want.clone()));
    assert!(
        rx.has_changed().expect("通道在"),
        "第一次看到这个号 ⇒ 推一下"
    );
    let w = answer_of(Some(&path), 1_800_000_100);
    assert_eq!(w["state"], "present");
    assert_eq!(w["now"], 1_800_000_100u64);
    assert_eq!(
        w["accounts"],
        serde_json::json!([{
            "agent": "claude-code",
            "account": "q",
            "seenAt": 1_800_000_000u64,
            "reading": {
                "status": "allowed",
                "refused": false,
                "limiting": "five_hour",
                "resetsAt": 1_800_003_600u64,
                "windows": [{"name": "five_hour", "used": 0.42, "resetsAt": 1_800_003_600u64}],
            },
        }])
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn only_a_change_you_could_see_pushes_and_quiet_repeats_reach_disk_at_most_once_a_minute() {
    let d = temp_dir("quiet");
    let path = d.join(FILE_NAME);
    let (l, mut rx) = ringing(Some(path.clone()));
    record_seen(&l, "claude-code", "q", reading(0.420, false), 1_000);
    rx.mark_unchanged();
    // 取整后的百分比没变（42%）⇒ 不推、盘上的时刻不动。
    record_seen(&l, "claude-code", "q", reading(0.4204, false), 1_010);
    assert!(!rx.has_changed().expect("通道在"));
    assert_eq!(on_disk(&path).accounts[0].seen_at, 1_000);
    assert_eq!(l.entry("claude-code", "q").map(|e| e.seen_at), Some(1_010));
    // 够久了 ⇒ 落一次盘，仍不推。
    record_seen(
        &l,
        "claude-code",
        "q",
        reading(0.4204, false),
        1_000 + PERSIST_EVERY,
    );
    assert!(!rx.has_changed().expect("通道在"));
    assert_eq!(on_disk(&path).accounts[0].seen_at, 1_000 + PERSIST_EVERY);
    // 43% ⇒ 推、立刻落。
    record_seen(
        &l,
        "claude-code",
        "q",
        reading(0.43, false),
        1_000 + PERSIST_EVERY + 1,
    );
    assert!(rx.has_changed().expect("通道在"));
    assert_eq!(
        on_disk(&path).accounts[0].seen_at,
        1_000 + PERSIST_EVERY + 1
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn two_accounts_keep_two_rows_and_a_restart_remembers_both() {
    let d = temp_dir("two");
    let path = d.join(FILE_NAME);
    let (l, _rx) = ringing(Some(path.clone()));
    record_seen(&l, "claude-code", "q", reading(0.1, false), 10);
    record_seen(&l, "claude-code", "a", reading(1.0, true), 11);
    let book = on_disk(&path);
    let names: Vec<&str> = book.accounts.iter().map(|e| e.account.as_str()).collect();
    assert_eq!(names, ["a", "q"]);
    let again = Ledger::at(Some(path.clone()));
    assert_eq!(
        again.entry("claude-code", "a").map(|e| e.reading.refused),
        Some(true)
    );
    assert_eq!(again.entry("claude-code", "q").map(|e| e.seen_at), Some(10));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_file_it_cannot_read_is_left_alone_and_said_so() {
    let d = temp_dir("bad");
    let path = d.join(FILE_NAME);
    std::fs::write(&path, "not json").expect("write");
    let (l, _rx) = ringing(Some(path.clone()));
    record_seen(&l, "claude-code", "q", reading(0.1, false), 10);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "not json");
    assert_eq!(answer_of(Some(&path), 0)["state"], "unreadable");
    assert_eq!(
        answer_of(Some(&d.join("absent.json")), 0)["state"],
        "absent"
    );
    assert_eq!(answer_of(None, 0)["state"], "unreadable");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_ledger_path_follows_the_data_home() {
    let got = path_from(&|k| match k {
        "CCM_DATA_DIR" => Some("/data/home".into()),
        _ => None,
    });
    assert_eq!(got, Some(PathBuf::from("/data/home").join(FILE_NAME)));
}
