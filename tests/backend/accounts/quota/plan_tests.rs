//! 推演（S8）：有用速 ⇒ 在用号到线时刻 ＝ 余量 / 用速、换号点是估的；没用速 ⇒ 不外推；延迟照备忘在对的时刻到期。

use super::*;
use crate::accounts::quota::decide::Kind;
use crate::accounts::quota::rotation::{AcctRow, Who};
use crate::accounts::quota::strategy::{builtins, find};
use serde_json::json;
use std::collections::BTreeMap;

const NOW: u64 = 1_760_187_780;

fn fact(id: &str, used: f64, ok: bool, back: Option<u64>) -> Facts {
    Facts {
        id: id.into(),
        kind: Kind::Sub,
        ok,
        why_not: (!ok).then_some(WhyNot::Refused),
        back,
        w5h: Win {
            open: used > 0.0,
            used,
            resets: (used > 0.0).then_some(NOW + 5 * 3600),
        },
        w7d: Win::CLOSED,
        others: 0,
    }
}

fn body(params: serde_json::Value) -> ConfigBody {
    ConfigBody {
        name: "x".into(),
        strategy: "1".into(),
        accounts: Vec::<AcctRow>::new(),
        params: params.as_object().cloned().unwrap_or_default(),
        acct_init: Params::new(),
        parked: BTreeMap::new(),
        tz: Some("UTC".into()),
    }
}

fn line(n: u64) -> Params {
    [("line".to_string(), json!(n))].into_iter().collect()
}

fn switches(snaps: &[Snap]) -> Vec<(u64, Option<String>, bool)> {
    snaps
        .iter()
        .filter(|s| s.switched.is_some())
        .map(|s| (s.at, s.account.clone(), s.est))
        .collect()
}

fn run(
    accounts: Vec<(Facts, Params)>,
    rate: Option<f64>,
    params: serde_json::Value,
    until: u64,
) -> Vec<Snap> {
    let one = find(builtins(), "1").expect("1");
    let b = body(params);
    simulate(&Input {
        strategy: Some(&one),
        body: &b,
        accounts,
        now: NOW,
        until,
        current: "a".into(),
        stint: Stint {
            since: NOW,
            used: 0.0,
        },
        rate,
        memo: Value::Null,
        manual: Manual::default(),
        can_hold: true,
        ready: &|_| true,
    })
}

/// ★ 有用速：a 80% · 线 90 · 1 点/分 ⇒ 第 10 分钟到线、换到 b（估的）；没用速 ⇒ 视窗里不换。
#[test]
fn with_a_rate_the_line_is_reached_at_left_over_rate() {
    let accts = || {
        vec![
            (fact("a", 80.0, true, None), line(90)),
            (fact("b", 0.0, true, None), line(90)),
        ]
    };
    let got = switches(&run(accts(), Some(1.0), json!({}), NOW + 3600));
    assert_eq!(got, vec![(NOW + 600, Some("b".to_string()), true)]);
    assert!(
        switches(&run(accts(), None, json!({}), NOW + 3600)).is_empty(),
        "没用速不外推"
    );
}

/// ★ 延迟照备忘到期：b 写在前面、5 分钟后恢复，换回去 ＋ 延迟 10 分 ⇒ 第 15 分钟换过去（不是第 5、也不是 15 分格上随便哪一刻）。
#[test]
fn a_delay_expires_in_the_plan_at_the_right_time() {
    let accts = vec![
        (fact("b", 0.0, false, Some(NOW + 300)), line(90)),
        (fact("a", 10.0, true, None), line(90)),
    ];
    let got = switches(&run(
        accts,
        None,
        json!({"back": true, "delay": 10}),
        NOW + 3600,
    ));
    assert_eq!(got, vec![(NOW + 900, Some("b".to_string()), true)]);
}
