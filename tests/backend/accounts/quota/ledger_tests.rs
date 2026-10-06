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
        windows_seen: Default::default(),
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

// ── 第二个来源（`quota-probe`）：按窗口合并 · 两个写者谁也冲不掉谁 ─────────────────────

fn win(name: &str, used: f64, resets_at: Option<u64>) -> QuotaWindow {
    QuotaWindow {
        name: name.into(),
        used: Some(used),
        resets_at,
        warned_at: None,
    }
}

fn names(o: &Observed) -> Vec<&str> {
    o.reading.windows.iter().map(|w| w.name.as_str()).collect()
}

/// ★ 报用量记进同一条账：只并窗口（按模型那一档多出来、同名的盖掉），回包头那几格（状态 · 被拒 · 卡在哪）不动；
/// 每个窗口记自己几点、从哪看到的；下一份回包头只盖它带着的窗口，报用量那一档留着。
#[test]
fn a_probe_merges_windows_into_the_same_entry_and_headers_do_not_wash_them_out() {
    let d = temp_dir("probe-merge");
    let path = d.join(FILE_NAME);
    let (l, rx) = ringing(Some(path.clone()));
    record_seen(&l, "claude-code", "q", reading(0.42, true), 100);
    let mut rx = rx;
    rx.mark_unchanged();
    let got = record_probe(
        &l,
        "claude-code",
        "q",
        vec![
            win("five_hour", 0.45, Some(1_800_003_600)),
            win("seven_day_fable", 0.0, None),
        ],
        200,
    )
    .expect("落盘");
    assert_eq!(names(&got), ["five_hour", "seven_day_fable"]);
    assert!(got.reading.refused, "被拒那一格只随回包头变");
    assert_eq!(got.reading.windows[0].used, Some(0.45));
    assert_eq!(
        got.window_seen("seven_day_fable"),
        WindowSeen {
            at: 200,
            from: Source::Usage
        }
    );
    assert_eq!(got.seen_at, 200);
    assert!(rx.has_changed().expect("通道在"), "显示变了 ⇒ 推");
    assert_eq!(on_disk(&path).accounts, vec![got.clone()]);
    // 下一份回包头只带 five_hour：盖掉它、留着 seven_day_fable（与它几点、从哪看到的）。
    record_seen(&l, "claude-code", "q", reading(0.5, false), 300);
    let after = on_disk(&path).accounts.remove(0);
    assert_eq!(names(&after), ["five_hour", "seven_day_fable"]);
    assert!(!after.reading.refused);
    assert_eq!(after.window_seen("five_hour").from, Source::Headers);
    assert_eq!(
        after.window_seen("seven_day_fable"),
        WindowSeen {
            at: 200,
            from: Source::Usage
        }
    );
    // 线上：只有来源 / 时刻与这一条不同的窗口才列出来。
    let wire = answer_of(Some(&path), 300);
    assert_eq!(
        wire["accounts"][0]["windowsSeen"],
        serde_json::json!({"seven_day_fable": {"at": 200, "from": "usage"}})
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 两个写者交错（常驻里的中转那一份内存账 · 一次性 CLI 的报用量）：谁也冲不掉谁的窗口；
/// 常驻那份在用的那一刻比盘上的戳，别人写过就并进来（不定时）。
#[test]
fn two_writers_interleaving_never_lose_a_window() {
    let d = temp_dir("two-writers");
    let path = d.join(FILE_NAME);
    let resident = Ledger::at(Some(path.clone()));
    record_seen(&resident, "claude-code", "q", reading(0.10, false), 100);
    // 另一个进程（CLI 一次性的那一形）：自己开一份账、只写报用量。
    let cli = Ledger::at(Some(path.clone()));
    record_probe(
        &cli,
        "claude-code",
        "q",
        vec![win("seven_day_opus", 0.30, Some(1_800_500_000))],
        110,
    )
    .expect("落盘");
    // 常驻那份：下一次用的时候就看得见（比戳 ⇒ 并进来）。
    let seen = resident.entry("claude-code", "q").expect("有");
    assert_eq!(names(&seen), ["five_hour", "seven_day_opus"]);
    // 中转再记一份（显示变了 ⇒ 落盘）：报用量那一档留在盘上。
    record_seen(&resident, "claude-code", "q", reading(0.20, false), 120);
    // CLI 那份还拿着旧的内存账：它再报一次，也冲不掉中转刚写的 five_hour。
    record_probe(
        &cli,
        "claude-code",
        "q",
        vec![win("seven_day_opus", 0.31, Some(1_800_500_000))],
        130,
    )
    .expect("落盘");
    let disk = on_disk(&path).accounts.remove(0);
    assert_eq!(names(&disk), ["five_hour", "seven_day_opus"]);
    assert_eq!(disk.reading.windows[0].used, Some(0.20), "中转那一份留着");
    assert_eq!(disk.reading.windows[1].used, Some(0.31), "报用量那一份留着");
    // 中转的内存账：没变的观测不落盘，但下一次用的时候并进了 CLI 后来写的那份。
    record_seen(&resident, "claude-code", "q", reading(0.2001, false), 140);
    let mem = resident.entry("claude-code", "q").expect("有");
    assert_eq!(mem.reading.windows[1].used, Some(0.31));
    assert_eq!(mem.reading.windows[0].used, Some(0.2001));
    let _ = std::fs::remove_dir_all(&d);
}

/// 只经报用量见过的号：回包头那几格是空的（没被拒 · 没说状态），窗口照报的。家推不出 ⇒ 报用量那一路照实失败（不只记在内存）。
#[test]
fn a_probe_on_a_fresh_account_and_without_a_home() {
    let d = temp_dir("probe-fresh");
    let path = d.join(FILE_NAME);
    let l = Ledger::at(Some(path.clone()));
    let got = record_probe(
        &l,
        "claude-code",
        "b",
        vec![win("five_hour", 0.0, None)],
        50,
    )
    .expect("落盘");
    assert!(!got.reading.refused);
    assert_eq!(got.reading.status, None);
    assert_eq!(names(&got), ["five_hour"]);
    assert!(record_probe(&Ledger::at(None), "claude-code", "b", vec![], 50).is_err());
    let _ = std::fs::remove_dir_all(&d);
}
