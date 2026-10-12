//! 引擎的积木：事实（E3）· 排（S6）· 手动调整（S6）· 挑与换（S7）· 出错兜底（S5）。

use super::*;
use crate::accounts::quota::rotation::{AcctRow, Hot, Place, Who};
use crate::accounts::quota::strategy::{self, builtins, find, File};
use crate::agents::{QuotaOverage, QuotaReading, QuotaWindow};
use serde_json::json;

fn slot(n: &str) -> Option<&'static str> {
    match n {
        "five_hour" => Some("5h"),
        "seven_day" | "seven_day_opus" => Some("7d"),
        _ => None,
    }
}

fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
    QuotaWindow {
        name: name.into(),
        used: Some(used),
        resets_at: Some(resets_at),
        warned_at: None,
    }
}

fn reading(windows: Vec<QuotaWindow>) -> QuotaReading {
    QuotaReading {
        status: None,
        refused: false,
        limiting: None,
        resets_at: None,
        windows,
        overage: None,
    }
}

const NOW: u64 = 1_760_187_780;

/// ★ E3：事实只有被拒 · 用满 · 超额在兜 · 要重新登录；到线不是事实（95% 照样能用）。
#[test]
fn facts_are_refused_full_overage_and_login_only() {
    let r = reading(vec![win("five_hour", 0.95, NOW + 600)]);
    assert_eq!(
        standing(Some(&r), false, Kind::Sub, true, NOW),
        None,
        "95% 不是事实上的不能用"
    );
    let full = reading(vec![
        win("five_hour", 1.0, NOW + 600),
        win("seven_day", 1.2, NOW + 9000),
    ]);
    assert_eq!(
        standing(Some(&full), false, Kind::Sub, true, NOW),
        Some((WhyNot::Full, Some(NOW + 9000)))
    );
    let mut refused = reading(vec![]);
    refused.refused = true;
    refused.resets_at = Some(NOW + 60);
    assert_eq!(
        standing(Some(&refused), false, Kind::Sub, true, NOW),
        Some((WhyNot::Refused, Some(NOW + 60)))
    );
    refused.resets_at = Some(NOW - 1);
    assert_eq!(
        standing(Some(&refused), false, Kind::Sub, true, NOW),
        None,
        "过了恢复时刻"
    );
    assert_eq!(
        standing(Some(&refused), true, Kind::Sub, true, NOW).map(|x| x.0),
        Some(WhyNot::Refused),
        "刚回来的回包被拒 ⇒ 不看时刻"
    );
    let mut over = reading(vec![]);
    over.resets_at = Some(NOW + 60);
    over.overage = Some(QuotaOverage {
        status: None,
        resets_at: None,
        disabled: None,
        in_use: true,
    });
    assert_eq!(
        standing(Some(&over), false, Kind::Sub, true, NOW).map(|x| x.0),
        Some(WhyNot::Overage)
    );
    assert_eq!(
        standing(Some(&over), false, Kind::Api, true, NOW),
        None,
        "按量号没有超额"
    );
    assert_eq!(
        standing(None, false, Kind::Sub, false, NOW),
        Some((WhyNot::Login, None))
    );
    let w = window_fact(Some(&full), "7d", NOW, &slot);
    assert!(w.open && (w.used - 120.0).abs() < 1e-9 && w.resets == Some(NOW + 9000));
    assert_eq!(window_fact(Some(&r), "7d", NOW, &slot), Win::CLOSED);
}

fn row(p: Option<f64>, back: Option<u64>) -> Given {
    Given {
        p,
        why: json!("w"),
        back,
    }
}

fn order(v: &[Ranked]) -> Vec<&str> {
    v.iter().map(|r| r.account.as_str()).collect()
}

/// ★ S6 rank：同分按号 id；`()` 最后、按 `back`；没给的在 `()` 之前。
#[test]
fn ranking_breaks_ties_by_id_and_puts_unpicked_last_by_back() {
    let rows: BTreeMap<String, Given> = [
        ("b".to_string(), row(Some(5.0), None)),
        ("a".to_string(), row(Some(5.0), None)),
        ("c".to_string(), row(Some(9.0), None)),
        ("x".to_string(), row(None, Some(300))),
        ("y".to_string(), row(None, Some(100))),
        ("z".to_string(), row(None, None)),
    ]
    .into();
    let ids: Vec<String> = ["a", "b", "c", "m", "x", "y", "z"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let v = rank(&rows, &ids);
    assert_eq!(order(&v), ["c", "a", "b", "m", "y", "x", "z"]);
    assert!(v[3].missing);
}

fn ranked(ids: &[&str]) -> Vec<Ranked> {
    ids.iter()
        .map(|a| Ranked {
            account: a.to_string(),
            p: Some(1.0),
            why: Value::Null,
            back: None,
            missing: false,
        })
        .collect()
}

/// ★ S6 overlay：`hot` 第一；`place` 越界取末尾、按 `at` 升序依次插。
#[test]
fn manual_adjustments_put_hot_first_and_places_where_dropped() {
    let m = Manual {
        hot: Some(Hot {
            account: "d".into(),
            since: 1,
        }),
        place: vec![
            Place {
                account: "a".into(),
                at: 99,
            },
            Place {
                account: "c".into(),
                at: 0,
            },
        ],
    };
    let v = overlay(ranked(&["a", "b", "c", "d"]), &m);
    assert_eq!(order(&v), ["d", "c", "b", "a"]);
}

fn facts(id: &str, ok: bool, back: Option<u64>) -> (Facts, Params) {
    (
        Facts {
            id: id.into(),
            kind: Kind::Sub,
            ok,
            why_not: (!ok).then_some(WhyNot::Refused),
            back,
            w5h: Win::CLOSED,
            w7d: Win::CLOSED,
            others: 0,
        },
        Params::new(),
    )
}

/// ★ S7：目标不是在用的 ⇒ 换；用不了的被顶上去也跳过；接不上的跳过；没有目标 ⇒ 停发、回来时刻 ＝ 最早 `back`。
#[test]
fn the_engine_switches_to_the_first_pickable_or_holds_until_the_earliest_back() {
    let fx = vec![
        facts("a", false, Some(500)),
        facts("b", true, None),
        facts("c", true, None),
        facts("d", true, None),
    ];
    let eff = ranked(&["a", "b", "c", "d"]);
    let k = Pickable {
        facts: &fx,
        current: "d",
        tried: &[],
        can_hold: true,
    };
    let mut asked = Vec::new();
    let v = decide(&eff, &k, "1", &mut |x| {
        asked.push(x.to_string());
        if x == "b" {
            Err(Unready::NeedsLogin)
        } else {
            Ok(())
        }
    });
    assert_eq!(asked, ["b", "c"], "被拒的不问、接不上的跳过");
    assert!(
        matches!(&v, Verdict::Switch { to, leave: Leave::Outranked, .. } if to == "c"),
        "{v:?}"
    );
    // 在用的号就是目标 ⇒ 不换。
    let k2 = Pickable { current: "b", ..k };
    assert_eq!(
        decide(&eff, &k2, "1", &mut |_| Ok(())),
        Verdict::Stay { overage: false }
    );
    // 全是 `()` ⇒ 停发，回来 ＝ 最早的 back（策略给的优先，没给取事实的）。
    let mut none = ranked(&["a", "b"]);
    none[0].p = None;
    none[1].p = None;
    none[1].back = Some(300);
    let v = decide(&none, &Pickable { current: "b", ..k }, "1", &mut |_| Ok(()));
    assert_eq!(
        v,
        Verdict::Hold {
            back: Some(("b".into(), 300)),
            w: None
        }
    );
    // 给不出「用满」回包 ⇒ 留着。
    let v = decide(
        &none,
        &Pickable {
            current: "b",
            can_hold: false,
            ..k
        },
        "1",
        &mut |_| Ok(()),
    );
    assert_eq!(v, Verdict::Stay { overage: false });
}

/// ★ S7：热换占着的号到线（策略给 `()`）或事实上不能用 ⇒ 清 `hot`、照策略排。
#[test]
fn a_hot_account_that_drops_out_is_cleared() {
    let fx = vec![facts("a", true, None), facts("b", true, None)];
    let m = Manual {
        hot: Some(Hot {
            account: "b".into(),
            since: 1,
        }),
        place: vec![],
    };
    let mut rows = ranked(&["a", "b"]);
    let (eff, clear) = effective(&rows, &m, &fx);
    assert_eq!((order(&eff), clear), (vec!["b", "a"], false));
    rows[1].p = None;
    let (eff, clear) = effective(&rows, &m, &fx);
    assert_eq!((order(&eff), clear), (vec!["a", "b"], true));
}

/// ★ S7：引擎里没有「线」「延迟」「都到上限后」的分支 —— 判定这一份源码里搜不到这几个参数名（兜底那一处照搬 `line` · `slots` 除外）。
#[test]
fn the_engine_has_no_branch_on_lines_delays_or_all_out() {
    let src = include_str!("../../../../src/backend/accounts/quota/decide.rs");
    let body = src.split("#[cfg(test)]").next().unwrap_or(src);
    for word in [
        "delay",
        "all_out",
        "DELAY",
        "ALL_OUT",
        "at_limit",
        "threshold",
        "stint_spent",
        "preempt",
        "fallback_api",
    ] {
        assert!(!body.contains(word), "decide.rs 里不该有 `{word}`");
    }
    let lines = body.matches("form::LINE").count();
    assert_eq!(lines, 1, "`line` 只在兜底照搬那一处");
}

fn body(strategy: &str, rows: Vec<AcctRow>) -> ConfigBody {
    ConfigBody {
        name: "x".into(),
        strategy: strategy.into(),
        accounts: rows,
        params: Params::new(),
        acct_init: Params::new(),
        parked: BTreeMap::new(),
        tz: Some("UTC".into()),
        tz_unknown: false,
    }
}

fn acct(id: &str, line: Value) -> AcctRow {
    AcctRow {
        who: Who::Id(id.into()),
        params: [("line".to_string(), line)].into_iter().collect(),
    }
}

/// ★ S5：编不过 / 跑出错 / 超时 / 形状不对 ⇒ 这一判与「内置策略 1 ＋ init 参数 ＋ 这份配置的 line · slots ＋ 写下的先后」逐号相同，且带出错。
#[test]
fn a_broken_strategy_falls_back_to_builtin_one_row_for_row() {
    let w = |used: f64| Facts {
        w5h: Win {
            open: true,
            used,
            resets: Some(NOW + 3600),
        },
        ..facts("x", true, None).0
    };
    let accounts: Vec<(Facts, Params)> = vec![
        (
            Facts {
                id: "a".into(),
                ..w(95.0)
            },
            [
                ("line".to_string(), json!(90)),
                ("mode".to_string(), json!("standby")),
            ]
            .into_iter()
            .collect(),
        ),
        (
            Facts {
                id: "b".into(),
                ..w(50.0)
            },
            [("line".to_string(), json!(99))].into_iter().collect(),
        ),
        (
            Facts {
                id: "c".into(),
                ..w(10.0)
            },
            Params::new(),
        ),
    ];
    let b = body(
        "bad",
        vec![
            acct("a", json!(90)),
            acct("b", json!(99)),
            acct("c", Value::Null),
        ],
    );
    let one = find(builtins(), "1").expect("1");
    let ref_body = ConfigBody {
        strategy: "1".into(),
        ..b.clone()
    };
    let ref_accounts: Vec<(Facts, Params)> = vec![
        (
            accounts[0].0.clone(),
            [("line".to_string(), json!(90))].into_iter().collect(),
        ),
        (
            accounts[1].0.clone(),
            [("line".to_string(), json!(99))].into_iter().collect(),
        ),
        (accounts[2].0.clone(), Params::new()),
    ];
    let reference = Judge {
        strategy: Some(&one),
        body: &ref_body,
        accounts: &ref_accounts,
        now: NOW,
        current: "b",
        stint: Stint {
            since: NOW,
            used: 0.0,
        },
        rate: None,
        memo: &Value::Null,
    };
    let want = judge(&reference);
    assert!(want.error.is_none());
    let tail = "fn texts() { #{ zh: #{}, en: #{} } }\nfn describe(p, a) { [] }\n";
    let meta = "fn meta() { #{ format: 1, params: [], acct_params: [] } }\n";
    let broken_sources = [
        ("compile", "fn meta( {".to_string()),
        (
            "throw",
            format!("{meta}{tail}fn priority(ctx) {{ throw \"x\"; }}"),
        ),
        (
            "spin",
            format!("{meta}{tail}fn priority(ctx) {{ loop {{}} }}"),
        ),
        (
            "shape",
            format!("{meta}{tail}fn priority(ctx) {{ #{{ a: #{{ p: \"x\" }} }} }}"),
        ),
    ];
    for (id, src) in broken_sources {
        let s = strategy::load(
            vec![File {
                name: format!("{id}.rhai"),
                path: None,
                text: Ok(src),
            }],
            false,
            &[],
        )
        .remove(0);
        let j = Judge {
            strategy: Some(&s),
            body: &b,
            accounts: &accounts,
            ..reference
        };
        let got = judge(&j);
        assert!(got.error.is_some(), "{id} 该带出错");
        assert_eq!(got.strategy, "1");
        assert_eq!(got.rows, want.rows, "{id} 兜底与内置 1 逐号相同");
    }
    // 策略没读到（文件没了）⇒ 同样兜底，出错是「没读到」。
    let j = Judge {
        strategy: None,
        body: &b,
        accounts: &accounts,
        ..reference
    };
    let got = judge(&j);
    assert_eq!(got.error.map(|e| e.kind), Some(FailKind::Gone));
    assert_eq!(got.rows, want.rows);
}
