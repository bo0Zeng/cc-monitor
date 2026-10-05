//! 换号的唯一判定：触发 · 按序取首个能接的 · 超额规矩 · 不打转。

use super::{decide, Back, Facts, Kind, Verdict};
use crate::accounts::quota::rotation::{AtLimit, RotationWhen, SwitchWhy, Unready};
use crate::agents::{QuotaOverage, QuotaReading, QuotaWindow};
use std::collections::BTreeMap;

const NOW: u64 = 1_800_000_000;

fn reading(refused: bool, used5h: f64, resets: Option<u64>) -> QuotaReading {
    QuotaReading {
        status: None,
        refused,
        limiting: Some("five_hour".into()),
        resets_at: resets,
        windows: vec![QuotaWindow {
            name: "five_hour".into(),
            used: Some(used5h),
            resets_at: resets,
            warned_at: None,
        }],
        overage: None,
    }
}

fn overage(r: QuotaReading) -> QuotaReading {
    QuotaReading {
        overage: Some(QuotaOverage {
            status: None,
            resets_at: None,
            disabled: None,
            in_use: true,
        }),
        ..r
    }
}

fn slot(n: &str) -> Option<&'static str> {
    match n {
        "five_hour" => Some("5h"),
        "seven_day" => Some("7d"),
        _ => None,
    }
}

struct World {
    pool: Vec<String>,
    when: RotationWhen,
    at_limit: AtLimit,
    seen: BTreeMap<String, QuotaReading>,
    api: Vec<String>,
    unready: BTreeMap<String, Unready>,
}

impl World {
    fn new(pool: &[&str]) -> Self {
        Self {
            pool: pool.iter().map(|s| s.to_string()).collect(),
            when: RotationWhen::Full,
            at_limit: AtLimit::Continue,
            seen: BTreeMap::new(),
            api: Vec::new(),
            unready: BTreeMap::new(),
        }
    }

    fn judge(
        &self,
        current: &str,
        heard: Option<&QuotaReading>,
        tried: &[&str],
    ) -> (Verdict, Vec<String>) {
        let tried: Vec<String> = tried.iter().map(|s| s.to_string()).collect();
        let seen = |a: &str| self.seen.get(a).cloned();
        let kind = |a: &str| {
            if self.api.iter().any(|x| x == a) {
                Kind::Api
            } else {
                Kind::Sub
            }
        };
        let f = Facts {
            pool: &self.pool,
            when: self.when,
            at_limit: self.at_limit,
            current,
            now: NOW,
            heard,
            seen: &seen,
            kind: &kind,
            slot: &slot,
            tried: &tried,
        };
        let mut asked = Vec::new();
        let v = decide(&f, &mut |a| {
            asked.push(a.to_string());
            self.unready.get(a).map_or(Ok(()), |u| Err(*u))
        });
        (v, asked)
    }
}

fn full5h() -> SwitchWhy {
    SwitchWhy::Full {
        w: Some("5h".into()),
    }
}

/// ★ 被拒的回包 ⇒ 按序取首个能接的；原号几点重置一并记下。
#[test]
fn a_refused_answer_switches_to_the_first_usable_account_in_order() {
    let w = World::new(&["a", "b", "c"]);
    let r = reading(true, 1.0, Some(NOW + 600));
    let (v, asked) = w.judge("a", Some(&r), &["a"]);
    assert_eq!(
        v,
        Verdict::Switch {
            to: "b".into(),
            why: full5h(),
            from_resets_at: Some(NOW + 600),
            skipped: vec![]
        }
    );
    assert_eq!(asked, ["b"]);
}

/// 不被拒的回包、额度账上也没满 ⇒ 不换（一个号都不问）。
#[test]
fn an_answer_that_was_served_never_switches() {
    let w = World::new(&["a", "b"]);
    let (v, asked) = w.judge("a", Some(&reading(false, 0.99, Some(NOW + 600))), &["a"]);
    assert_eq!(v, Verdict::Stay);
    assert!(asked.is_empty());
    assert_eq!(w.judge("a", None, &[]).0, Verdict::Stay);
}

/// 缺省池（只有起始账号）⇒ 被拒也没得换：原样交回。
#[test]
fn the_default_pool_of_one_never_switches() {
    let w = World::new(&["a"]);
    let (v, _) = w.judge("a", Some(&reading(true, 1.0, Some(NOW + 60))), &["a"]);
    assert!(matches!(v, Verdict::Stuck { why, .. } if why == full5h()));
}

/// 从头按序取：已在 b 上、b 被拒，起始号 a 已过重置时刻 ⇒ 回到 a；还没过 ⇒ 跳到 c。
#[test]
fn order_is_preference_and_a_recovered_start_account_comes_back() {
    let mut w = World::new(&["a", "b", "c"]);
    w.seen.insert("a".into(), reading(true, 1.0, Some(NOW - 1)));
    let refused = reading(true, 1.0, Some(NOW + 60));
    assert!(
        matches!(w.judge("b", Some(&refused), &["b"]).0, Verdict::Switch { to, .. } if to == "a")
    );
    w.seen
        .insert("a".into(), reading(true, 1.0, Some(NOW + 60)));
    assert!(
        matches!(w.judge("b", Some(&refused), &["b"]).0, Verdict::Switch { to, .. } if to == "c")
    );
}

/// 接不上的号跳过并记下为什么；都接不上 ⇒ 没换成，跳过的照样交回。
#[test]
fn unready_accounts_are_skipped_with_their_reason() {
    let mut w = World::new(&["a", "b", "c"]);
    w.unready.insert("b".into(), Unready::NeedsLogin);
    let r = reading(true, 1.0, None);
    let (v, _) = w.judge("a", Some(&r), &["a"]);
    assert_eq!(
        v,
        Verdict::Switch {
            to: "c".into(),
            why: full5h(),
            from_resets_at: None,
            skipped: vec![("b".into(), Unready::NeedsLogin)]
        }
    );
    w.unready.insert("c".into(), Unready::NeedsKey);
    assert!(
        matches!(w.judge("a", Some(&r), &["a"]).0, Verdict::Stuck { skipped, .. } if skipped.len() == 2)
    );
}

/// ★ 不打转：池里都试过了 ⇒ 没得换（原样交回最后那个拒绝）；一发请求里问到的号不重样、至多池子大小个。
#[test]
fn tried_accounts_are_never_tried_again_within_one_request() {
    let w = World::new(&["a", "b", "c"]);
    let r = reading(true, 1.0, None);
    let (v, _) = w.judge("c", Some(&r), &["a", "b", "c"]);
    assert!(matches!(v, Verdict::Stuck { .. }));
    let mut tried = vec!["a".to_string()];
    let mut current = "a".to_string();
    while let (Verdict::Switch { to, .. }, _) = w.judge(
        &current,
        Some(&r),
        &tried.iter().map(String::as_str).collect::<Vec<_>>(),
    ) {
        assert!(!tried.contains(&to));
        tried.push(to.clone());
        current = to;
    }
    assert_eq!(tried, ["a", "b", "c"]);
}

/// 发之前：额度账上此刻的号被拒、未到重置 ⇒ 先换（不先撞一次 429）；满着的候选不进。
#[test]
fn before_sending_a_refused_current_account_switches_first() {
    let mut w = World::new(&["a", "b", "c"]);
    w.seen
        .insert("a".into(), reading(true, 1.0, Some(NOW + 60)));
    w.seen
        .insert("b".into(), reading(true, 1.0, Some(NOW + 60)));
    assert!(matches!(w.judge("a", None, &[]).0, Verdict::Switch { to, .. } if to == "c"));
}

/// ★ 阈值模式：200 里过了阈值的号，下一发（发之前）就换；过了阈值的候选不进；没过阈值不换。
#[test]
fn threshold_mode_switches_on_the_next_request() {
    let mut w = World::new(&["a", "b", "c"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.seen
        .insert("a".into(), reading(false, 0.91, Some(NOW + 60)));
    w.seen
        .insert("b".into(), reading(false, 0.95, Some(NOW + 60)));
    assert_eq!(
        w.judge("a", None, &[]).0,
        Verdict::Switch {
            to: "c".into(),
            why: SwitchWhy::Threshold { n: 90 },
            from_resets_at: Some(NOW + 60),
            skipped: vec![]
        }
    );
    w.seen
        .insert("a".into(), reading(false, 0.89, Some(NOW + 60)));
    assert_eq!(w.judge("a", None, &[]).0, Verdict::Stay);
    w.when = RotationWhen::Full;
    w.seen
        .insert("a".into(), reading(false, 0.99, Some(NOW + 60)));
    assert_eq!(w.judge("a", None, &[]).0, Verdict::Stay);
}

/// ★ 阈值模式、池里其余的号都过了阈值：此刻的号**被拒** ⇒ 退一步按序取首个没被拒的（过了阈值也取）；
/// 那几个也被拒了 ⇒ 不换（原样交回）；只是**过了阈值**（没被拒）⇒ 照旧留在此刻的号上照发。
#[test]
fn a_refusal_falls_back_to_the_first_unrefused_account_past_the_threshold() {
    let mut w = World::new(&["a", "b", "c"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.seen
        .insert("b".into(), reading(true, 1.0, Some(NOW + 60)));
    w.seen
        .insert("c".into(), reading(false, 0.92, Some(NOW + 60)));
    let refused = reading(true, 1.0, Some(NOW + 600));
    let (v, asked) = w.judge("a", Some(&refused), &["a"]);
    assert_eq!(
        v,
        Verdict::Switch {
            to: "c".into(),
            why: full5h(),
            from_resets_at: Some(NOW + 600),
            skipped: vec![]
        }
    );
    assert_eq!(asked, ["c"], "被拒的 b 不该被问");
    // 发之前：额度账上此刻的号被拒 ⇒ 同一条退路。
    w.seen.insert("a".into(), refused.clone());
    assert!(matches!(w.judge("a", None, &[]).0, Verdict::Switch { to, .. } if to == "c"));
    // 退路上的号也被拒 ⇒ 没得换。
    w.seen
        .insert("c".into(), reading(true, 1.0, Some(NOW + 60)));
    assert!(matches!(
        w.judge("a", Some(&refused), &["a"]).0,
        Verdict::Stuck { .. }
    ));
    // 只是过了阈值（没被拒）、其余的也都过了 ⇒ 留在此刻的号上，不退。
    w.seen
        .insert("a".into(), reading(false, 0.95, Some(NOW + 60)));
    w.seen
        .insert("c".into(), reading(false, 0.92, Some(NOW + 60)));
    assert!(matches!(
        w.judge("a", None, &[]).0,
        Verdict::Stuck {
            why: SwitchWhy::Threshold { n: 90 },
            ..
        }
    ));
}

/// ★ 硬上限：阈值以下没有能接的 ⇒ 不发（`Hold`），回来的时刻 ＝ 池里最早回到阈值以下的那一刻；
/// 一个号几个窗口都过了阈值 ⇒ 都重置了才算回来（取最晚那个窗口）；被拒触发也不退到过了阈值的号；软阈值同一情形照旧。
#[test]
fn a_hard_limit_holds_until_the_earliest_account_is_back_under_the_threshold() {
    let mut w = World::new(&["a", "b", "c"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.at_limit = AtLimit::Stop;
    w.seen
        .insert("a".into(), reading(false, 0.95, Some(NOW + 600)));
    // b：5h 过了阈值、几点重置早，但 7d 也过了、更晚 ⇒ b 要到 7d 重置才回来。
    let mut b = reading(false, 0.92, Some(NOW + 100));
    b.windows.push(QuotaWindow {
        name: "seven_day".into(),
        used: Some(0.93),
        resets_at: Some(NOW + 5000),
        warned_at: None,
    });
    w.seen.insert("b".into(), b);
    w.seen
        .insert("c".into(), reading(true, 1.0, Some(NOW + 900)));
    let want = Verdict::Hold {
        n: 90,
        back: Back {
            account: "a".into(),
            at: NOW + 600,
            slot: Some("5h".into()),
        },
        skipped: vec![],
    };
    assert_eq!(w.judge("a", None, &[]).0, want);
    // 此刻的号被拒：硬上限不退到过了阈值的号。
    let refused = reading(true, 1.0, Some(NOW + 600));
    assert!(matches!(
        w.judge("a", Some(&refused), &["a"]).0,
        Verdict::Hold { .. }
    ));
    // 说不出几点回来 ⇒ 照软阈值办（留在此刻的号上照发）。
    w.seen.insert("a".into(), reading(false, 0.95, None));
    w.seen.insert("c".into(), reading(true, 1.0, None));
    w.seen.insert("b".into(), reading(false, 0.95, None));
    assert!(matches!(w.judge("a", None, &[]).0, Verdict::Stuck { .. }));
    // 软阈值：同一情形不 `Hold`。
    w.at_limit = AtLimit::Continue;
    w.seen
        .insert("a".into(), reading(false, 0.95, Some(NOW + 600)));
    assert!(matches!(w.judge("a", None, &[]).0, Verdict::Stuck { .. }));
    // 「被拒才换」模式下 `stop` 不成立（没有 N%）：照常换到没被拒的 b。
    w.when = RotationWhen::Full;
    w.at_limit = AtLimit::Stop;
    assert!(
        matches!(w.judge("a", Some(&refused), &["a"]).0, Verdict::Switch { to, .. } if to == "b")
    );
}

/// ★ 超额规矩：订阅号在用付费超额 ⇒ 轮换里还有未满的订阅号就换（按量号不算）；都满才留在超额（`toOverage`）。
#[test]
fn overage_counts_as_full_and_only_subscriptions_take_over() {
    let mut w = World::new(&["a", "api", "b"]);
    w.api.push("api".into());
    w.seen
        .insert("a".into(), overage(reading(false, 1.0, Some(NOW + 60))));
    assert!(matches!(w.judge("a", None, &[]).0, Verdict::Switch { to, .. } if to == "b"));
    w.seen
        .insert("b".into(), overage(reading(false, 1.0, Some(NOW + 60))));
    assert_eq!(
        w.judge("a", None, &[]).0,
        Verdict::Stuck {
            why: SwitchWhy::ToOverage,
            from_resets_at: Some(NOW + 60),
            skipped: vec![]
        }
    );
}
