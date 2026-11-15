//! `control/launch_account.rs` 起会话挑号的规则表（逐条行为）：夹具的账号清单成品照账号库那一份的规则补
//! （`accounts::manage::library::test_support`），挂主 crate（`tests/backend/cross/control.rs`）。
//! 那份记录（便条 · 账本）的判据住 `launch_account_tests.rs`。

use crate::accounts::manage::library::test_support::{list, row_with, unreadable};
use crate::accounts::manage::library::{AccountRow, ScannedAccount};
use crate::control::launch_account::*;
use cmd_wire::accounts::AccountsList;
use cmd_wire::session::LaunchedAccount;
use cmd_wire::AgentKind;
use std::collections::BTreeMap;

/// 这一趟起的那一家（替身）；[`OTHER`] 是另一家替身。号各带各的家（成品那一格 `scanned.agent`）。
const HOME: &str = "alpha";
const OTHER: &str = "beta";

fn here() -> AgentKind {
    AgentKind::new(HOME)
}

/// 起会话那一家的一个号（能起会话）。
fn ok(name: &str) -> AccountRow {
    acct(name, |_| {})
}

/// 同 [`ok`]，再按 `f` 改几格（家缺省是这一趟起的那一家，`f` 可改）。
fn acct(name: &str, f: impl FnOnce(&mut ScannedAccount)) -> AccountRow {
    row_with(name, |a| {
        a.agent = Some(here());
        f(a);
    })
}

/// 另一家的号（本身能起会话）。
fn other(name: &str, f: impl FnOnce(&mut ScannedAccount)) -> AccountRow {
    acct(name, |a| {
        a.agent = Some(AgentKind::new(OTHER));
        f(a);
    })
}

/// 判定照这一趟起的那一家挑号。
fn pick(asked: &AccountAsk, list: &AccountsList, last: Option<&str>) -> Got {
    crate::control::launch_account::pick(asked, list, last, &here())
}

/// 成品那两格（`selectable` · `meta.effectiveDefault`）由夹具照成品的规则补上（成品真写这两格由
/// `library_tests::the_list_product_says_who_is_selectable_and_who_is_the_default` 钉）。
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
        login: false,
        other_agent: None,
    })
}
/// 要的号是别家的：拒，且说得出是哪一家（替代只给同家的）。
fn refused_other(requested: &str, pinned: bool, alternative: Option<&str>) -> Got {
    Err(AccountUnavailable {
        requested: requested.into(),
        pinned,
        list_known: true,
        alternative: alternative.map(Into::into),
        login: false,
        other_agent: Some(AgentKind::new(OTHER)),
    })
}
fn named(n: &str) -> AccountAsk {
    AccountAsk::Named { name: n.into() }
}
fn default_(name: &str) -> AccountRow {
    acct(name, |a| a.is_default = true)
}

// ───────── 点名 ─────────

/// ［登录］那一格：只给同家、那一行可登录的号；别家的 · 清单里没有的 · 登不了的都不给。
#[test]
fn login_is_offered_only_for_a_same_agent_row_that_can_log_in() {
    let mut l = lib(vec![
        acct("z", |a| a.auth_ready = false),
        other("y", |a| a.auth_ready = false),
    ]);
    for a in &mut l.accounts {
        a.login = true;
    }
    let login = |n: &str, l: &AccountsList| pick(&named(n), l, None).unwrap_err().login;
    assert!(login("z", &l), "同家、那一行可登录 ⇒ 给");
    assert!(!login("y", &l), "别家的号 ⇒ 不给");
    assert!(!login("gone", &l), "清单里没有 ⇒ 不给");
    l.accounts[0].login = false;
    assert!(!login("z", &l), "那一行登不了 ⇒ 不给");
    l.accounts[0].login = true;
    assert!(
        pick(&AccountAsk::Follow, &l, Some("z")).unwrap_err().login,
        "跟随上次的号同一判"
    );
}

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
        agent: here(),
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
        agent: here(),
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
        agent: here(),
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
        agent: here(),
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

// ───────── 看家：起哪一家只认那一家的号 ─────────

#[test]
fn a_named_account_of_another_agent_is_refused_and_says_whose_it_is() {
    let l = lib(vec![other("b", |_| {}), default_("z")]);
    assert_eq!(
        pick(&named("b"), &l, None),
        refused_other("b", false, Some("z"))
    );
}

#[test]
fn follow_with_the_default_from_another_agent_is_unsaid() {
    let l = lib(vec![other("b", |a| a.is_default = true), ok("z")]);
    assert_eq!(pick(&AccountAsk::Follow, &l, None), Ok(Settled::Unsaid));
}

#[test]
fn follow_with_a_last_account_from_another_agent_is_refused() {
    let l = lib(vec![other("b", |_| {})]);
    assert_eq!(
        pick(&AccountAsk::Follow, &l, Some("b")),
        refused_other("b", true, None)
    );
}

#[test]
fn accounts_of_the_same_agent_are_taken_as_before_beside_another_agents() {
    let l = lib(vec![other("b", |_| {}), default_("z"), ok("y")]);
    assert_eq!(pick(&named("y"), &l, None), take("y"));
    assert_eq!(pick(&AccountAsk::Follow, &l, Some("y")), take("y"));
    assert_eq!(pick(&AccountAsk::Follow, &l, None), take("z"));
    // 替代也只给同家的：别家的默认号不拿来替。
    let l = lib(vec![
        other("b", |a| a.is_default = true),
        acct("z", |a| a.auth_ready = false),
    ]);
    assert_eq!(pick(&named("z"), &l, None), refused("z", false, true, None));
}

#[test]
fn settle_hands_the_launching_agent_to_pick() {
    let library = || lib(vec![other("b", |_| {})]);
    let facts = Facts {
        agent: here(),
        has_accounts: true,
        library: &library,
        last: &|_| None,
    };
    assert_eq!(
        settle(&named("b"), None, &Default::default(), &facts),
        refused_other("b", false, None)
    );
}
