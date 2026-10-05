//! `control/launch_account.rs`：起会话挑号的规则表（逐条行为）＋ 那份记录（临时目录上真写真读、并发写不丢）。

use super::*;
use serde_json::json;
use std::path::PathBuf;

fn acct(name: &str, f: impl FnOnce(&mut Value)) -> Value {
    let mut a = json!({
        "name": name, "email": format!("{name}@x.edu"), "configDir": format!("/h/{name}"),
        "isDefault": false, "mode": "isolated", "exists": true, "loggedIn": true,
        "authKind": "subscription", "authReady": true,
    });
    f(&mut a);
    a
}
fn ok(name: &str) -> Value {
    acct(name, |_| {})
}
fn lib(accounts: Vec<Value>) -> Library {
    Library::of_product(
        &json!({ "meta": { "enabled": true }, "accounts": accounts, "notice": null }),
    )
}
fn take(name: &str) -> Picked {
    Picked::Account {
        name: name.into(),
        config_dir: format!("/h/{name}"),
    }
}
fn refused(requested: &str, pinned: bool, list_known: bool, alternative: Option<&str>) -> Picked {
    Picked::Unavailable(AccountUnavailable {
        requested: requested.into(),
        pinned,
        list_known,
        alternative: alternative.map(Into::into),
    })
}
fn named(n: &str) -> Asked {
    Asked::Named(n.into())
}
fn default_(name: &str) -> Value {
    acct(name, |a| a["isDefault"] = json!(true))
}

// ───────── 点名 ─────────

#[test]
fn a_named_usable_account_is_used() {
    assert_eq!(
        pick(&named("z"), &lib(vec![ok("z")]), None, None),
        take("z")
    );
}

#[test]
fn a_named_account_that_cannot_be_used_is_refused_with_the_default_as_alternative() {
    let l = lib(vec![
        acct("z", |a| a["authReady"] = json!(false)),
        default_("b"),
    ]);
    assert_eq!(
        pick(&named("z"), &l, None, None),
        refused("z", false, true, Some("b"))
    );
}

#[test]
fn each_unusable_shape_is_refused() {
    for (why, f) in [
        (
            "in-place",
            (|a: &mut Value| a["mode"] = json!("in-place")) as fn(&mut Value),
        ),
        ("auth not ready", |a| a["authReady"] = json!(false)),
        ("dir missing", |a| a["exists"] = json!(false)),
        ("empty dir", |a| a["configDir"] = json!("")),
        ("account 0", |a| {
            a["configDir"] = Value::Null;
            a["mode"] = json!("bare");
        }),
    ] {
        let got = pick(&named("z"), &lib(vec![acct("z", f)]), None, None);
        assert_eq!(got, refused("z", false, true, None), "{why}");
    }
}

#[test]
fn an_api_key_account_without_a_credentials_file_is_usable() {
    let l = lib(vec![acct("k", |a| {
        a["authKind"] = json!("apikey");
        a["loggedIn"] = json!(false);
    })]);
    assert_eq!(pick(&named("k"), &l, None, None), take("k"));
}

#[test]
fn account_zero_asked_for_is_base_and_needs_no_library() {
    let lib = || -> Library { panic!("账号 0 不用读账号库") };
    let facts = Facts {
        has_accounts: true,
        library: &lib,
        last: &|_| Some("z".into()),
    };
    assert_eq!(
        settle(&AccountAsk::Base, Some("s"), &Default::default(), &facts),
        Ok(Settled::Base)
    );
}

#[test]
fn settle_maps_follow_and_named_onto_pick_and_attaches_the_model() {
    let library = || lib(vec![ok("z"), default_("b")]);
    let facts = Facts {
        has_accounts: true,
        library: &library,
        last: &|s| (s == "s1").then(|| "z".to_string()),
    };
    let models: BTreeMap<String, String> = [("z".to_string(), "opus".to_string())].into();
    let account = |n: &str, m: Option<&str>| {
        Ok(Settled::Account(LaunchedAccount {
            name: n.into(),
            config_dir: format!("/h/{n}"),
            model: m.map(Into::into),
        }))
    };
    // 跟随：有上次的号 ⇒ 它（带上表里它那一条模型）；新起（没有 sid）⇒ 默认号。
    assert_eq!(
        settle(&AccountAsk::Follow, Some("s1"), &models, &facts),
        account("z", Some("opus"))
    );
    assert_eq!(
        settle(&AccountAsk::Follow, None, &models, &facts),
        account("b", None)
    );
    // 点名：按名字判。
    let named = AccountAsk::Named { name: "b".into() };
    assert_eq!(
        settle(&named, Some("s1"), &models, &facts),
        account("b", None)
    );
    // 跟随却什么都选不上 ⇒ 不表态（不是显式账号 0）。
    let none = || lib(vec![acct("z", |a| a["authReady"] = json!(false))]);
    let f2 = Facts {
        has_accounts: true,
        library: &none,
        last: &|_| None,
    };
    assert_eq!(
        settle(&AccountAsk::Follow, None, &models, &f2),
        Ok(Settled::Unsaid)
    );
    // 这一家没有账号这一维 ⇒ 跟随不表态、不读账号库。
    let boom = || -> Library { panic!("没有账号这一维还去读账号库") };
    let f3 = Facts {
        has_accounts: false,
        library: &boom,
        last: &|_| None,
    };
    assert_eq!(
        settle(&AccountAsk::Follow, Some("s1"), &models, &f3),
        Ok(Settled::Unsaid)
    );
}

// ───────── 跟随 ─────────

#[test]
fn follow_uses_the_last_account_over_the_default() {
    let l = lib(vec![ok("z"), default_("b")]);
    assert_eq!(pick(&Asked::Follow, &l, Some("z"), None), take("z"));
}

#[test]
fn follow_never_swaps_a_last_account_that_cannot_be_used() {
    let l = lib(vec![
        acct("z", |a| a["authReady"] = json!(false)),
        default_("b"),
    ]);
    assert_eq!(
        pick(&Asked::Follow, &l, Some("z"), None),
        refused("z", true, true, Some("b"))
    );
}

#[test]
fn follow_with_a_last_account_gone_from_the_list_is_refused_too() {
    let l = lib(vec![default_("b")]);
    assert_eq!(
        pick(&Asked::Follow, &l, Some("gone"), None),
        refused("gone", true, true, Some("b"))
    );
}

#[test]
fn follow_without_a_last_account_takes_the_default() {
    let l = lib(vec![ok("b"), default_("z")]);
    assert_eq!(pick(&Asked::Follow, &l, None, None), take("z"));
}

#[test]
fn the_default_is_the_first_account_when_none_is_marked() {
    let l = lib(vec![ok("b"), ok("z")]);
    assert_eq!(pick(&Asked::Follow, &l, None, None), take("b"));
}

#[test]
fn follow_without_a_last_account_and_an_unusable_default_is_base() {
    let l = lib(vec![acct("z", |a| a["authReady"] = json!(false))]);
    assert_eq!(pick(&Asked::Follow, &l, None, None), Picked::Base);
}

#[test]
fn no_alternative_when_the_default_is_the_requested_one_or_unusable() {
    let l = lib(vec![acct("z", |a| {
        a["authReady"] = json!(false);
        a["isDefault"] = json!(true);
    })]);
    assert_eq!(
        pick(&Asked::Follow, &l, Some("z"), None),
        refused("z", true, true, None)
    );
    let l = lib(vec![
        ok("z"),
        acct("b", |a| {
            a["isDefault"] = json!(true);
            a["exists"] = json!(false);
        }),
    ]);
    assert_eq!(
        pick(&named("gone"), &l, None, None),
        refused("gone", false, true, None)
    );
}

#[test]
fn an_unreadable_list_refuses_named_and_pinned_and_offers_nothing() {
    let l = Library::of_product(
        &json!({ "meta": { "enabled": false, "error": "坏了" }, "accounts": [] }),
    );
    assert_eq!(
        pick(&named("z"), &l, None, None),
        refused("z", false, false, None)
    );
    assert_eq!(
        pick(&Asked::Follow, &l, Some("z"), None),
        refused("z", true, false, None)
    );
    assert_eq!(pick(&Asked::Follow, &l, None, None), Picked::Base);
}

#[test]
fn the_pinned_now_slot_does_not_change_anything_today() {
    let l = lib(vec![ok("z"), default_("b")]);
    assert_eq!(pick(&Asked::Follow, &l, Some("z"), Some("b")), take("z"));
}

// ───────── 记录 ─────────

fn temp_home(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-launchacct-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn none_alive(_: u32) -> Option<Option<u64>> {
    None
}

#[test]
fn a_note_is_adopted_into_the_record_and_kept_while_the_process_lives() {
    let h = temp_home("adopt");
    leave_note(&h, 4242, "x", 1_000).unwrap();
    assert!(adopt(&h, 4242, "s1", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    // 同一个进程原地换了 sid（`/clear`）⇒ 新 sid 也记得到。
    assert!(adopt(&h, 4242, "s2", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s2").as_deref(), Some("x"));
    // 已经记着同一个号 ⇒ 不再写。
    assert!(!adopt(&h, 4242, "s2", Some(990), &none_alive).unwrap());
    assert!(h.join(NOTES_DIR).join("4242.json").exists());
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn no_note_no_record() {
    let h = temp_home("nonote");
    assert!(!adopt(&h, 7, "s1", None, &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    assert!(!h.join(FILE_NAME).exists());
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_note_older_than_the_process_is_a_reused_pid_and_does_not_count() {
    let h = temp_home("reused");
    leave_note(&h, 9, "x", 1_000).unwrap();
    assert!(!adopt(&h, 9, "s1", Some(1_001), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn stale_notes_of_other_processes_are_swept() {
    let h = temp_home("sweep");
    leave_note(&h, 11, "x", 1_000).unwrap(); // 进程不在了
    leave_note(&h, 12, "x", 1_000).unwrap(); // 在，但比便条晚起（pid 被复用）
    leave_note(&h, 13, "x", 1_000).unwrap(); // 在、对得上
    leave_note(&h, 14, "x", 1_000).unwrap(); // 在、起始时刻说不出
    let alive = |p: u32| match p {
        12 => Some(Some(1_005)),
        13 => Some(Some(999)),
        14 => Some(None),
        _ => None,
    };
    adopt(&h, 99, "s", None, &alive).unwrap();
    let left: Vec<u32> = [11, 12, 13, 14]
        .into_iter()
        .filter(|p| h.join(NOTES_DIR).join(format!("{p}.json")).exists())
        .collect();
    assert_eq!(left, vec![13, 14]);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn an_unreadable_record_is_never_overwritten() {
    let h = temp_home("unreadable");
    std::fs::write(h.join(FILE_NAME), "{坏").unwrap();
    leave_note(&h, 5, "x", 1_000).unwrap();
    assert!(adopt(&h, 5, "s1", None, &none_alive).is_err());
    assert_eq!(std::fs::read_to_string(h.join(FILE_NAME)).unwrap(), "{坏");
    let _ = std::fs::remove_dir_all(&h);
}

/// 两个进程同时记（这里用线程，锁是跨进程那一把、线程间同样互斥）⇒ 一条都不丢。
#[test]
fn concurrent_records_all_land() {
    let h = temp_home("concurrent");
    const N: u32 = 24;
    std::thread::scope(|s| {
        for i in 0..N {
            let h = h.clone();
            s.spawn(move || {
                leave_note(&h, 50_000 + i, "x", 1_000).unwrap();
                adopt(&h, 50_000 + i, &format!("s{i}"), None, &|_| Some(None)).unwrap();
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
