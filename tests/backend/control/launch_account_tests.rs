//! `control/launch_account.rs`：起会话挑号的规则表（逐条行为）＋ 那份记录（临时目录上真写真读、并发写不丢）。

use super::*;
use crate::observe::accounts_query::test_support::{list, row as ok, row_with as acct, unreadable};
use crate::observe::accounts_query::{AccountRow, ScannedAccount};
use std::path::PathBuf;

/// 成品那两格（`selectable` · `meta.effectiveDefault`）由夹具照成品的规则补上（成品真写这两格由
/// `accounts_query_tests::the_list_product_says_who_is_selectable_and_who_is_the_default` 钉）。
fn lib(accounts: Vec<AccountRow>) -> Library {
    Library::of_product(&list(accounts))
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
fn default_(name: &str) -> AccountRow {
    acct(name, |a| a.is_default = true)
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
    let l = lib(vec![acct("z", |a| a.auth_ready = false), default_("b")]);
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
            (|a: &mut ScannedAccount| a.mode = "in-place".into()) as fn(&mut ScannedAccount),
        ),
        ("auth not ready", |a| a.auth_ready = false),
        ("dir missing", |a| a.exists = false),
        ("empty dir", |a| a.config_dir = Some(String::new())),
        ("account 0", |a| {
            a.config_dir = None;
            a.mode = "bare".into();
        }),
    ] {
        let got = pick(&named("z"), &lib(vec![acct("z", f)]), None, None);
        assert_eq!(got, refused("z", false, true, None), "{why}");
    }
}

#[test]
fn an_api_key_account_without_a_credentials_file_is_usable() {
    let l = lib(vec![acct("k", |a| {
        a.auth_kind = "apikey".into();
        a.logged_in = false;
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
    let none = || lib(vec![acct("z", |a| a.auth_ready = false)]);
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
    let l = lib(vec![acct("z", |a| a.auth_ready = false), default_("b")]);
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
    let l = lib(vec![acct("z", |a| a.auth_ready = false)]);
    assert_eq!(pick(&Asked::Follow, &l, None, None), Picked::Base);
}

#[test]
fn no_alternative_when_the_default_is_the_requested_one_or_unusable() {
    let l = lib(vec![acct("z", |a| {
        a.auth_ready = false;
        a.is_default = true;
    })]);
    assert_eq!(
        pick(&Asked::Follow, &l, Some("z"), None),
        refused("z", true, true, None)
    );
    let l = lib(vec![
        ok("z"),
        acct("b", |a| {
            a.is_default = true;
            a.exists = false;
        }),
    ]);
    assert_eq!(
        pick(&named("gone"), &l, None, None),
        refused("gone", false, true, None)
    );
}

#[test]
fn an_unreadable_list_refuses_named_and_pinned_and_offers_nothing() {
    let l = Library::of_product(&unreadable("坏了"));
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

/// 写句柄：数据目录那一根就是这个临时家（便条目录与记录都落它底下）。
fn own_at(h: &std::path::Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(h)
}

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

/// 起会话时给了 `--tab`（别名里写的带别名名）⇒ 便条带着它，认便条时记进启动台账的 `tabs`；没有号也照记（只为这一格留的便条）。
#[test]
fn a_note_carries_the_tab_into_the_launch_ledger() {
    let h = temp_home("tab");
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
    let h = temp_home("nonote");
    assert!(!adopt(&own_at(&h), 7, "s1", None, &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    assert!(!h.join(FILE_NAME).exists());
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_note_older_than_the_process_is_a_reused_pid_and_does_not_count() {
    let h = temp_home("reused");
    leave_note(&own_at(&h), 9, Some("x"), false, None, 1_000).unwrap();
    assert!(!adopt(&own_at(&h), 9, "s1", Some(1_001), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn stale_notes_of_other_processes_are_swept() {
    let h = temp_home("sweep");
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
    let h = temp_home("unreadable");
    std::fs::write(h.join(FILE_NAME), "{坏").unwrap();
    leave_note(&own_at(&h), 5, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 5, "s1", None, &none_alive).is_err());
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
