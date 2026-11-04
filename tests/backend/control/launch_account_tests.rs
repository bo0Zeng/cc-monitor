//! `control/launch_account.rs` 的那份记录（便条 · 账本：临时目录上真写真读、并发写不丢）。挑号的规则表要观测面的账号清单夹具，住 `launch_account_pick_tests.rs`（挂主 crate）。

use super::*;
use crate::history::launch_book::{last_of, FILE_NAME};

// ───────── 记录 ─────────

/// 写句柄：数据目录那一根就是这个临时家（便条目录与记录都落它底下）。
fn own_at(h: &std::path::Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(h)
}

fn none_alive(_: u32) -> Option<Option<u64>> {
    None
}

#[test]
fn a_note_is_adopted_into_the_record_and_kept_while_the_process_lives() {
    let h = test_home::scratch("adopt");
    leave_note(&own_at(&h), 4242, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 4242, "s1", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    // 同一个进程原地换了 sid（`/clear`）⇒ 新 sid 也记得到。
    assert!(adopt(&own_at(&h), 4242, "s2", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s2").as_deref(), Some("x"));
    // 已经记着同一个号 ⇒ 不再写。
    assert!(!adopt(&own_at(&h), 4242, "s2", Some(990), &none_alive).unwrap());
    assert!(h.join(NOTES_DIR).join("4242.json").exists());
    let _ = std::fs::remove_dir_all(&h);
}

/// 台账与便条都带声明的版本落盘；没有版本的旧台账照第 1 版读。
#[test]
fn the_ledger_and_the_note_carry_their_version() {
    let h = test_home::scratch("ver");
    std::fs::write(h.join(FILE_NAME), r#"{"sessions":{"s0":"old"}}"#).unwrap();
    leave_note(&own_at(&h), 4343, Some("x"), false, None, 1_000).unwrap();
    let note: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.join(NOTES_DIR).join("4343.json")).unwrap())
            .unwrap();
    assert_eq!(
        note["version"],
        serde_json::json!(1),
        "便条没带版本：{note}"
    );
    assert!(adopt(&own_at(&h), 4343, "s1", Some(990), &none_alive).unwrap());
    let book: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.join(FILE_NAME)).unwrap()).unwrap();
    assert_eq!(
        book["version"],
        serde_json::json!(1),
        "台账没带版本：{book}"
    );
    assert_eq!(last_of(Some(&h), "s0").as_deref(), Some("old"));
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    let _ = std::fs::remove_dir_all(&h);
}

/// 起会话时给了 `--tab`（别名里写的带别名名）⇒ 便条带着它，认便条时记进启动台账的 `tabs`；没有号也照记（只为这一格留的便条）。
#[test]
fn a_note_carries_the_tab_into_the_launch_ledger() {
    let h = test_home::scratch("tab");
    let tab = LaunchTab {
        tab: cmd_wire::session::Tab::Hide,
        alias: Some("cct".into()),
    };
    leave_note(&own_at(&h), 77, None, false, Some(tab.clone()), 1_000).unwrap();
    assert!(adopt(&own_at(&h), 77, "s-tab", Some(990), &none_alive).unwrap());
    let book = read_book(&h).unwrap();
    assert_eq!(book.tabs.get("s-tab"), Some(&tab));
    assert_eq!(book.sessions.get("s-tab"), None, "没有号的便条记了一个号");
    // 已经记着同一格 ⇒ 不再写。
    assert!(!adopt(&own_at(&h), 77, "s-tab", Some(990), &none_alive).unwrap());
    // 没写 `--tab` 的那一份台账，盘上不多一格。
    leave_note(&own_at(&h), 78, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 78, "s-plain", Some(990), &none_alive).unwrap());
    assert_eq!(read_book(&h).unwrap().tabs.get("s-plain"), None);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn no_note_no_record() {
    let h = test_home::scratch("nonote");
    assert!(!adopt(&own_at(&h), 7, "s1", None, &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    assert!(!h.join(FILE_NAME).exists());
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_note_older_than_the_process_is_a_reused_pid_and_does_not_count() {
    let h = test_home::scratch("reused");
    leave_note(&own_at(&h), 9, Some("x"), false, None, 1_000).unwrap();
    assert!(!adopt(&own_at(&h), 9, "s1", Some(1_001), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn stale_notes_of_other_processes_are_swept() {
    let h = test_home::scratch("sweep");
    leave_note(&own_at(&h), 11, Some("x"), false, None, 1_000).unwrap(); // 进程不在了
    leave_note(&own_at(&h), 12, Some("x"), false, None, 1_000).unwrap(); // 在，但比便条晚起（pid 被复用）
    leave_note(&own_at(&h), 13, Some("x"), false, None, 1_000).unwrap(); // 在、对得上
    leave_note(&own_at(&h), 14, Some("x"), false, None, 1_000).unwrap(); // 在、起始时刻说不出
    let alive = |p: u32| match p {
        12 => Some(Some(1_005)),
        13 => Some(Some(999)),
        14 => Some(None),
        _ => None,
    };
    adopt(&own_at(&h), 99, "s", None, &alive).unwrap();
    let left: Vec<u32> = [11, 12, 13, 14]
        .into_iter()
        .filter(|p| h.join(NOTES_DIR).join(format!("{p}.json")).exists())
        .collect();
    assert_eq!(left, vec![13, 14]);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn an_unreadable_record_is_never_overwritten() {
    let h = test_home::scratch("unreadable");
    std::fs::write(h.join(FILE_NAME), "{坏").unwrap();
    leave_note(&own_at(&h), 5, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 5, "s1", None, &none_alive).is_err());
    assert_eq!(std::fs::read_to_string(h.join(FILE_NAME)).unwrap(), "{坏");
    let _ = std::fs::remove_dir_all(&h);
}

/// 两个进程同时记（这里用线程，锁是跨进程那一把、线程间同样互斥）⇒ 一条都不丢。
#[test]
fn concurrent_records_all_land() {
    let h = test_home::scratch("concurrent");
    const N: u32 = 24;
    std::thread::scope(|s| {
        for i in 0..N {
            let h = h.clone();
            s.spawn(move || {
                leave_note(&own_at(&h), 50_000 + i, Some("x"), false, None, 1_000).unwrap();
                adopt(&own_at(&h), 50_000 + i, &format!("s{i}"), None, &|_| {
                    Some(None)
                })
                .unwrap();
            });
        }
    });
    let book = read_book(&h).unwrap();
    assert_eq!(
        book.sessions.len(),
        N as usize,
        "并发写丢了几条：{:?}",
        book.sessions.keys()
    );
    let _ = std::fs::remove_dir_all(&h);
}

// ───────── 选不了号那一句 ─────────

/// 选不了号那一句由这里写全：跟随 / 点名 × 有替代 / 没替代 × 清单读不出，五种各一句、互不相同；
/// 都带要的号；有替代 ⇒ 带替代的名字；清单读不出 ⇒ 不提替代（那台说不出谁能用）。
#[test]
fn each_unavailable_shape_has_its_own_sentence() {
    let u = |pinned: bool, list_known: bool, alt: Option<&str>| AccountUnavailable {
        requested: "acct-gone".into(),
        pinned,
        list_known,
        alternative: alt.map(Into::into),
    };
    let shapes = [
        u(true, true, Some("acct-alt")),
        u(true, true, None),
        u(false, true, Some("acct-alt")),
        u(false, true, None),
        u(false, false, None),
    ];
    let said: Vec<String> = shapes.iter().map(unavailable_said).collect();
    let distinct: std::collections::BTreeSet<&String> = said.iter().collect();
    assert_eq!(distinct.len(), shapes.len(), "五种说成了同一句：{said:#?}");
    for (s, line) in shapes.iter().zip(&said) {
        assert!(line.contains("acct-gone"), "没说要的是哪个号：{line}");
        let alt_named = line.contains("acct-alt");
        assert_eq!(
            alt_named,
            s.list_known && s.alternative.is_some(),
            "替代那一半说得不对（{s:?}）：{line}"
        );
    }
    // 清单读不出时有替代也不提（那台的默认号未必还能用）。
    assert!(!unavailable_said(&u(true, false, Some("acct-alt"))).contains("acct-alt"));
}

/// 经别名起的那一发：便条带别名名字，认便条时记进启动台账的 `aliases`（没有号、没写 `--tab` 也照记）。
#[test]
fn a_note_carries_the_alias_into_the_launch_ledger() {
    let h = test_home::scratch("alias");
    leave(
        &own_at(&h),
        81,
        &Note {
            account: String::new(),
            at: 1_000,
            plugin: false,
            tab: None,
            alias: Some("dev".into()),
            host: None,
        },
    )
    .unwrap();
    assert!(adopt(&own_at(&h), 81, "s-alias", Some(990), &none_alive).unwrap());
    let book = read_book(&h).unwrap();
    assert_eq!(book.aliases.get("s-alias").map(String::as_str), Some("dev"));
    assert_eq!(book.sessions.get("s-alias"), None);
    // 已经记着 ⇒ 不再写。
    assert!(!adopt(&own_at(&h), 81, "s-alias", Some(990), &none_alive).unwrap());
    let _ = std::fs::remove_dir_all(&h);
}

fn host_note(account: &str, host: Option<cmd_wire::frame::TerminalHost>) -> Note {
    Note {
        account: account.to_string(),
        at: 1_000,
        plugin: false,
        tab: None,
        alias: None,
        host,
    }
}

/// 起在哪种宿主里：便条带宿主（不带号也认）⇒ 记进启动台账的 `hosts`；同一条后来经直路起（便条没宿主）⇒ 摘掉那一条。
#[test]
fn a_note_carries_the_host_into_the_launch_ledger_and_a_direct_launch_drops_it() {
    use cmd_wire::frame::TerminalHost;
    let h = test_home::scratch("host");
    leave(&own_at(&h), 91, &host_note("", Some(TerminalHost::Tmux))).unwrap();
    assert!(adopt(&own_at(&h), 91, "s-host", Some(990), &none_alive).unwrap());
    assert_eq!(
        crate::history::launch_book::host_of(Some(&h), "s-host"),
        Some(TerminalHost::Tmux)
    );
    assert!(
        !adopt(&own_at(&h), 91, "s-host", Some(990), &none_alive).unwrap(),
        "记着同一个还写"
    );
    leave(
        &own_at(&h),
        92,
        &host_note("x", Some(TerminalHost::Managed)),
    )
    .unwrap();
    assert!(adopt(&own_at(&h), 92, "s-host", Some(990), &none_alive).unwrap());
    assert_eq!(
        crate::history::launch_book::host_of(Some(&h), "s-host"),
        Some(TerminalHost::Managed)
    );
    leave(&own_at(&h), 93, &host_note("x", None)).unwrap();
    assert!(adopt(&own_at(&h), 93, "s-host", Some(990), &none_alive).unwrap());
    assert_eq!(
        crate::history::launch_book::host_of(Some(&h), "s-host"),
        None
    );
    let _ = std::fs::remove_dir_all(&h);
}
