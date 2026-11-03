//! `control/launch_account.rs` 起会话挑号的规则表（逐条行为）：夹具的账号清单成品照观测面那一份的规则补
//! （`observe::accounts_query::test_support`）⇒ 挂主 crate（`tests/backend/cross/control.rs`）：控制面是独立的包、够不到观测面。
//! 那份记录（便条 · 账本）的判据住 `launch_account_tests.rs`。

use crate::control::launch_account::*;
use crate::observe::accounts_query::test_support::{list, row as ok, row_with as acct, unreadable};
use crate::observe::accounts_query::{AccountRow, ScannedAccount};
use cmd_wire::accounts::AccountsList;
use cmd_wire::session::LaunchedAccount;
use std::collections::BTreeMap;

/// 成品那两格（`selectable` · `meta.effectiveDefault`）由夹具照成品的规则补上（成品真写这两格由
/// `accounts_query_tests::the_list_product_says_who_is_selectable_and_who_is_the_default` 钉）。
fn lib(accounts: Vec<AccountRow>) -> AccountsList {
    list(accounts)
}
type Got = Result<Settled, AccountUnavailable>;
fn take(name: &str) -> Got {
    Ok(Settled::Account(LaunchedAccount {
        name: name.into(),
        config_dir: format!("/h/{name}"),
        model: None,
    }))
}
fn refused(requested: &str, pinned: bool, list_known: bool, alternative: Option<&str>) -> Got {
    Err(AccountUnavailable {
        requested: requested.into(),
        pinned,
        list_known,
        alternative: alternative.map(Into::into),
    })
}
fn named(n: &str) -> AccountAsk {
    AccountAsk::Named { name: n.into() }
}
fn default_(name: &str) -> AccountRow {
    acct(name, |a| a.is_default = true)
}

// ───────── 点名 ─────────

#[test]
fn a_named_usable_account_is_used() {
    assert_eq!(pick(&named("z"), &lib(vec![ok("z")]), None), take("z"));
}

#[test]
fn a_named_account_that_cannot_be_used_is_refused_with_the_default_as_alternative() {
    let l = lib(vec![acct("z", |a| a.auth_ready = false), default_("b")]);
    assert_eq!(
        pick(&named("z"), &l, None),
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
        ("account 0", |a| {
            a.config_dir = None;
            a.mode = "bare".into();
        }),
    ] {
        let got = pick(&named("z"), &lib(vec![acct("z", f)]), None);
        assert_eq!(got, refused("z", false, true, None), "{why}");
    }
}

#[test]
fn an_api_key_account_without_a_credentials_file_is_usable() {
    let l = lib(vec![acct("k", |a| {
        a.auth_kind = "apikey".into();
        a.logged_in = false;
    })]);
    assert_eq!(pick(&named("k"), &l, None), take("k"));
}

#[test]
fn account_zero_asked_for_is_base_and_needs_no_library() {
    let lib = || -> AccountsList { panic!("账号 0 不用读账号库") };
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
    let boom = || -> AccountsList { panic!("没有账号这一维还去读账号库") };
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
    assert_eq!(pick(&AccountAsk::Follow, &l, Some("z")), take("z"));
}

#[test]
fn follow_never_swaps_a_last_account_that_cannot_be_used() {
    let l = lib(vec![acct("z", |a| a.auth_ready = false), default_("b")]);
    assert_eq!(
        pick(&AccountAsk::Follow, &l, Some("z")),
        refused("z", true, true, Some("b"))
    );
}

#[test]
fn follow_with_a_last_account_gone_from_the_list_is_refused_too() {
    let l = lib(vec![default_("b")]);
    assert_eq!(
        pick(&AccountAsk::Follow, &l, Some("gone")),
        refused("gone", true, true, Some("b"))
    );
}

#[test]
fn follow_without_a_last_account_takes_the_default() {
    let l = lib(vec![ok("b"), default_("z")]);
    assert_eq!(pick(&AccountAsk::Follow, &l, None), take("z"));
}

#[test]
fn the_default_is_the_first_account_when_none_is_marked() {
    let l = lib(vec![ok("b"), ok("z")]);
    assert_eq!(pick(&AccountAsk::Follow, &l, None), take("b"));
}

#[test]
fn follow_without_a_last_account_and_an_unusable_default_is_base() {
    let l = lib(vec![acct("z", |a| a.auth_ready = false)]);
    assert_eq!(pick(&AccountAsk::Follow, &l, None), Ok(Settled::Unsaid));
}

#[test]
fn no_alternative_when_the_default_is_the_requested_one_or_unusable() {
    let l = lib(vec![acct("z", |a| {
        a.auth_ready = false;
        a.is_default = true;
    })]);
    assert_eq!(
        pick(&AccountAsk::Follow, &l, Some("z")),
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
        pick(&named("gone"), &l, None),
        refused("gone", false, true, None)
    );
}

#[test]
fn an_unreadable_list_refuses_named_and_pinned_and_offers_nothing() {
    let l = unreadable("坏了");
    assert_eq!(
        pick(&named("z"), &l, None),
        refused("z", false, false, None)
    );
    assert_eq!(
        pick(&AccountAsk::Follow, &l, Some("z")),
        refused("z", true, false, None)
    );
    assert_eq!(pick(&AccountAsk::Follow, &l, None), Ok(Settled::Unsaid));
}
