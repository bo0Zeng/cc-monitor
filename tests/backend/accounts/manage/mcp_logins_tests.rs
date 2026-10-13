//! 账号之间同步 MCP 登录的三态对照（纯）：一个号登录 ⇒ 别的号也有；一个号续期 ⇒ 别的号拿到新的；
//! 两边同时续 ⇒ 没有一份副本留着作废的刷新令牌；在一个号里退出 ⇒ 所有号都撤；失效的那一条不往外传。
//! 令牌全是现编的假值（`at-*` · `rt-*`）。

use super::*;
use serde_json::json;

const C: Cells = Cells {
    access: "accessToken",
    refresh: "refreshToken",
    expires_ms: "expiresAt",
};
const K: &str = "fake|0123456789abcdef";

fn entry(at: &str, rt: &str, exp: u64) -> Value {
    json!({
        "serverName": "fake", "serverUrl": "http://127.0.0.1:1/mcp",
        "accessToken": at, "refreshToken": rt, "expiresAt": exp,
        "clientId": "client-x", "discoveryState": {"authorizationServerUrl": "http://127.0.0.1:1"}
    })
}

/// 那一家续期被拒之后清掉的样子：键还在，令牌都空。
fn dead() -> Value {
    json!({"serverName": "fake", "serverUrl": "http://127.0.0.1:1/mcp", "accessToken": "", "expiresAt": 0, "clientId": "client-x"})
}

fn dir(n: &str) -> String {
    format!("/h/.cc-monitor/accounts/{n}")
}

fn seen(n: &str, now: Option<Value>) -> SeenLogins {
    SeenLogins {
        dir: dir(n),
        now: Some(match now {
            Some(e) => Entries::from_iter([(K.to_string(), e)]),
            None => Entries::new(),
        }),
    }
}

fn base(pairs: &[(&str, &Value)]) -> LoginBase {
    pairs
        .iter()
        .map(|(n, e)| {
            (
                dir(n),
                BTreeMap::from([(K.to_string(), fingerprint(e, &C))]),
            )
        })
        .collect()
}

fn write_to<'a>(p: &'a LoginPlan, n: &str) -> Option<&'a LoginWrite> {
    p.writes.iter().find(|w| w.dir == dir(n))
}

#[test]
fn a_login_in_one_account_is_written_into_the_others() {
    let a = entry("at-1", "rt-1", 2_000);
    let p = plan(
        &LoginBase::new(),
        &[seen("a", Some(a.clone())), seen("b", None), seen("c", None)],
        &C,
    );
    for n in ["b", "c"] {
        let w = write_to(&p, n).unwrap_or_else(|| panic!("{n} 没拿到 a 的登录"));
        assert_eq!(
            w.to.as_ref(),
            Some(&a),
            "{n} 拿到的不是 a 那一条（整条照抄，客户端 id 一起）"
        );
        assert_eq!(w.expect, None, "{n} 那时没有这一条");
    }
    assert!(write_to(&p, "a").is_none(), "a 自己不用写");
    for n in ["a", "b", "c"] {
        assert_eq!(p.base[&dir(n)][K], fingerprint(&a, &C), "{n} 的底没记上");
    }
}

#[test]
fn a_renewal_in_one_account_reaches_the_others() {
    let old = entry("at-1", "rt-1", 2_000);
    let new = entry("at-2", "rt-2", 5_000);
    let b0 = base(&[("a", &old), ("b", &old)]);
    let p = plan(
        &b0,
        &[seen("a", Some(new.clone())), seen("b", Some(old.clone()))],
        &C,
    );
    let w = write_to(&p, "b").expect("b 没拿到新令牌");
    assert_eq!(w.to.as_ref(), Some(&new));
    assert_eq!(
        w.expect,
        Some(fingerprint(&old, &C)),
        "写前比对要的是计划时 b 那一条"
    );
    assert!(write_to(&p, "a").is_none());
}

#[test]
fn when_both_renew_at_once_no_copy_keeps_an_invalidated_refresh_token() {
    // a 与 b 同时拿 rt-1 去续：服务端只认先到的那一发（a 拿到 rt-2）；b 那一发被拒 ⇒ b 自己清成失效的样子。
    let old = entry("at-1", "rt-1", 2_000);
    let won = entry("at-2", "rt-2", 5_000);
    let b0 = base(&[("a", &old), ("b", &old)]);
    let p = plan(
        &b0,
        &[seen("a", Some(won.clone())), seen("b", Some(dead()))],
        &C,
    );
    assert_eq!(
        write_to(&p, "b").and_then(|w| w.to.as_ref()),
        Some(&won),
        "b 停在失效的那一条上"
    );
    // b 赶在被拒之前已经又续成一条更新的（它那一发先到）：a 那一条旧了，换成 b 的。
    let later = entry("at-3", "rt-3", 6_000);
    let p = plan(
        &b0,
        &[seen("a", Some(won.clone())), seen("b", Some(later.clone()))],
        &C,
    );
    assert_eq!(write_to(&p, "a").and_then(|w| w.to.as_ref()), Some(&later));
    assert!(write_to(&p, "b").is_none());
    // 还有一个号捏着更早的那一条（rt-1 已作废）：同样被换掉。
    let p = plan(
        &b0,
        &[
            seen("a", Some(won.clone())),
            seen("b", Some(dead())),
            seen("c", Some(old.clone())),
        ],
        &C,
    );
    for n in ["b", "c"] {
        assert_eq!(
            write_to(&p, n).and_then(|w| w.to.as_ref()),
            Some(&won),
            "{n}"
        );
    }
}

#[test]
fn an_invalidated_entry_never_spreads() {
    let old = entry("at-1", "rt-1", 2_000);
    let b0 = base(&[("a", &old), ("b", &old)]);
    let p = plan(
        &b0,
        &[seen("a", Some(old.clone())), seen("b", Some(dead()))],
        &C,
    );
    assert!(write_to(&p, "a").is_none(), "失效的那一条盖到了 a 上");
    assert_eq!(
        write_to(&p, "b").and_then(|w| w.to.as_ref()),
        Some(&old),
        "b 该拿回能用的那一条"
    );
    // 一条能用的都没有 ⇒ 谁都不写。
    let p = plan(&b0, &[seen("a", Some(dead())), seen("b", Some(dead()))], &C);
    assert!(p.writes.is_empty());
}

#[test]
fn logging_out_in_one_account_removes_it_everywhere() {
    let one = entry("at-1", "rt-1", 2_000);
    let b0 = base(&[("a", &one), ("b", &one), ("c", &one)]);
    let p = plan(
        &b0,
        &[
            seen("a", None),
            seen("b", Some(one.clone())),
            seen("c", Some(dead())),
        ],
        &C,
    );
    for n in ["b", "c"] {
        let w = write_to(&p, n).unwrap_or_else(|| panic!("{n} 没撤"));
        assert_eq!(w.to, None);
    }
    assert!(p.base.is_empty(), "撤完底里不该还记着这一条");
    // 退出的同时另一个号新登录了一条 ⇒ 新登录压过退出。
    let fresh = entry("at-9", "rt-9", 9_000);
    let p = plan(&b0, &[seen("a", None), seen("b", Some(fresh.clone()))], &C);
    assert_eq!(write_to(&p, "a").and_then(|w| w.to.as_ref()), Some(&fresh));
}

#[test]
fn a_new_account_that_never_had_it_is_not_a_logout() {
    let one = entry("at-1", "rt-1", 2_000);
    let b0 = base(&[("a", &one)]);
    let p = plan(&b0, &[seen("a", Some(one.clone())), seen("new", None)], &C);
    assert_eq!(write_to(&p, "new").and_then(|w| w.to.as_ref()), Some(&one));
    assert!(write_to(&p, "a").is_none());
}

#[test]
fn an_unreadable_account_sits_out_and_keeps_its_base() {
    let one = entry("at-1", "rt-1", 2_000);
    let b0 = base(&[("a", &one), ("b", &one)]);
    let p = plan(
        &b0,
        &[
            seen("a", Some(one.clone())),
            SeenLogins {
                dir: dir("b"),
                now: None,
            },
        ],
        &C,
    );
    assert!(p.writes.is_empty());
    assert_eq!(
        p.base.get(&dir("b")),
        b0.get(&dir("b")),
        "读不出来的号底不动"
    );
}

#[test]
fn equal_expiry_prefers_the_one_that_changed_since_the_base() {
    // 服务端不给过期时刻：两边都是 0，以自底以来变过的那条为新。
    let old = entry("at-1", "rt-1", 0);
    let new = entry("at-2", "rt-2", 0);
    let b0 = base(&[("a", &old), ("b", &old)]);
    let p = plan(
        &b0,
        &[seen("a", Some(old.clone())), seen("b", Some(new.clone()))],
        &C,
    );
    assert_eq!(write_to(&p, "a").and_then(|w| w.to.as_ref()), Some(&new));
}

#[test]
fn the_fingerprint_hides_the_tokens() {
    let e = entry("at-secret-1", "rt-secret-1", 2_000);
    let fp = fingerprint(&e, &C);
    assert_eq!(fp.len(), 16);
    assert!(!fp.contains("secret") && !fp.contains("at-") && !fp.contains("rt-"));
    assert_ne!(
        fp,
        fingerprint(&entry("at-secret-1", "rt-secret-2", 2_000), &C)
    );
}
