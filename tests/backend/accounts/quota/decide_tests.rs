//! 换号的唯一判定：触发 · 按序取首个能接的 · 超额规矩 · 不打转。

use super::{decide, Back, Facts, Kind, Verdict};
use crate::accounts::quota::rotation::{
    AtLimit, Baseline, Caps, RotationWhen, Stints, SwitchWhy, Unready,
};
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

fn key(n: &str) -> Option<String> {
    crate::agents::claudecode::quota::key_of(n)
}

struct World {
    pool: Vec<String>,
    when: RotationWhen,
    at_limit: AtLimit,
    seen: BTreeMap<String, QuotaReading>,
    api: Vec<String>,
    unready: BTreeMap<String, Unready>,
    // 积木那几格（缺省 ＝ 今天的行为：没有每号上限 · 没有单段预算 · 不切回）。
    cap: Caps,
    stint: Stints,
    preempt: bool,
    base: Baseline,
    /// 换进此刻的号那一刻挡在它前面的号（`preempt` 只等它们回来）。
    above: Vec<String>,
    now: u64,
    offset: i64,
    /// 兜底的号与切兜底前最多等几分钟（缺省没有兜底 ⇒ 不等：上面那些判据钉的是不等时的行为）。
    fallback: Vec<String>,
    wait: u8,
    can_hold: bool,
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
            cap: Caps::new(),
            stint: Stints::new(),
            preempt: false,
            base: Baseline::new(),
            above: Vec::new(),
            now: NOW,
            offset: 0,
            fallback: Vec::new(),
            wait: 0,
            can_hold: true,
        }
    }

    fn judge(
        &self,
        current: &str,
        heard: Option<&QuotaReading>,
        tried: &[&str],
    ) -> (Verdict, Vec<String>) {
        let mut asked = Vec::new();
        let v = self.with(current, heard, tried, |f| {
            decide(f, &mut |a| {
                asked.push(a.to_string());
                self.unready.get(a).map_or(Ok(()), |u| Err(*u))
            })
        });
        (v, asked)
    }

    /// 换进 `target` 那一刻：记下挡在它前面的号（同上游选择换号时做的那一步）。
    fn enter(&mut self, target: &str) {
        self.above = self.with(target, None, &[], |f| super::blocked_above(f, target));
    }

    fn with<R>(
        &self,
        current: &str,
        heard: Option<&QuotaReading>,
        tried: &[&str],
        run: impl FnOnce(&Facts<'_>) -> R,
    ) -> R {
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
            cap: &self.cap,
            stint: &self.stint,
            preempt: self.preempt,
            at_limit: self.at_limit,
            fallback: &self.fallback,
            wait: self.wait,
            can_hold: self.can_hold,
            current,
            base: &self.base,
            above: &self.above,
            now: self.now,
            offset: self.offset,
            heard,
            seen: &seen,
            kind: &kind,
            slot: &slot,
            key: &key,
            tried: &tried,
        };
        run(&f)
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

// ── 积木：每号上限（按窗口 · 按时段）· 单段预算 · 切回 ─────────────────────────────

use crate::accounts::quota::rotation::{Base, CapSlot, CapValue};

/// 一个号只有 5h 一个窗口的快照（没被拒）。
fn at(used5h: f64, resets: u64) -> QuotaReading {
    reading(false, used5h, Some(resets))
}

fn caps(rows: &[(&str, &str, CapValue)]) -> Caps {
    let mut c = Caps::new();
    for (a, k, v) in rows {
        c.entry(a.to_string())
            .or_default()
            .insert(k.to_string(), v.clone());
    }
    c
}

fn slots(at: &str, n: u8) -> CapValue {
    CapValue::Slots(vec![CapSlot { at: at.into(), n }])
}

fn to_of(v: &Verdict) -> Option<&str> {
    match v {
        Verdict::Switch { to, .. } => Some(to),
        _ => None,
    }
}

/// UTC 这一天的零点（`NOW` 是 08:00 UTC）。
const DAY: u64 = NOW - 8 * 3600;

/// 本地钟 `hh:mm`（东八区）那一刻。
fn local(w: &mut World, hh: u64, mm: u64) {
    w.offset = 8 * 3600;
    w.now = DAY + (hh * 3600 + mm * 60) - 8 * 3600 + 86_400;
}

/// ★ 用户例子一「换到 b 之后再用 5 个点就换」：`stint: {b: {5h: 5}}`。按换进来那一刻的基线算；
/// 用到 5 个点 ⇒ 换到首个能用的；没有能用的 ⇒ 留着（软的）。
#[test]
fn a_stint_moves_on_after_its_points_and_stays_when_nobody_can_take_over() {
    let mut w = World::new(&["a", "b", "c"]);
    w.seen
        .insert("a".into(), reading(true, 1.0, Some(NOW + 3600)));
    w.stint.insert("b".into(), [("5h".to_string(), 5u8)].into());
    w.base.insert(
        "5h".into(),
        Base {
            used: 0.30,
            resets_at: Some(NOW + 3600),
        },
    );
    w.seen.insert("b".into(), at(0.34, NOW + 3600));
    assert_eq!(w.judge("b", None, &[]).0, Verdict::Stay, "才用了 4 个点");
    w.seen.insert("b".into(), at(0.35, NOW + 3600));
    assert_eq!(
        w.judge("b", None, &[]).0,
        Verdict::Switch {
            to: "c".into(),
            why: SwitchWhy::Stint {
                w: "5h".into(),
                n: 5
            },
            from_resets_at: None,
            skipped: vec![]
        },
        "用够 5 个点 ⇒ 换到首个能用的（a 被拒，跳过）"
    );
    // 软的：别的都不能用 ⇒ 留在 b 上照发（不卡住、不记）。
    w.seen
        .insert("c".into(), reading(true, 1.0, Some(NOW + 3600)));
    assert_eq!(w.judge("b", None, &[]).0, Verdict::Stay);
    // `*` ＝ 这个号的所有窗口。
    w.seen.insert("c".into(), at(0.1, NOW + 3600));
    w.stint.insert("b".into(), [("*".to_string(), 5u8)].into());
    assert_eq!(to_of(&w.judge("b", None, &[]).0), Some("c"));
    // 基线那一期已过（窗口重置过）⇒ 只算重置之后的：重置后才用了 3 个点 ⇒ 不换。
    w.base.insert(
        "5h".into(),
        Base {
            used: 0.30,
            resets_at: Some(NOW - 1),
        },
    );
    w.seen.insert("b".into(), at(0.03, NOW + 18_000));
    assert_eq!(w.judge("b", None, &[]).0, Verdict::Stay);
}

/// ★ 单段预算是软的、上限是硬的：同样「没有别的能用」，过了上限照 `atLimit`（`continue` ⇒ 卡住照发 · `stop` ⇒ 不发），
/// 用完单段预算只是留着。
#[test]
fn a_stint_is_soft_and_a_cap_is_hard() {
    let mut w = World::new(&["a", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.seen.insert("a".into(), at(0.95, NOW + 600));
    w.stint.insert("b".into(), [("5h".to_string(), 5u8)].into());
    w.base.insert(
        "5h".into(),
        Base {
            used: 0.10,
            resets_at: Some(NOW + 600),
        },
    );
    w.seen.insert("b".into(), at(0.50, NOW + 600));
    assert_eq!(
        w.judge("b", None, &[]).0,
        Verdict::Stay,
        "预算用完、a 过上限 ⇒ 留着"
    );
    w.seen.insert("b".into(), at(0.92, NOW + 900));
    assert!(matches!(
        w.judge("b", None, &[]).0,
        Verdict::Stuck {
            why: SwitchWhy::Threshold { n: 90 },
            ..
        }
    ));
    w.at_limit = AtLimit::Stop;
    assert!(matches!(
        w.judge("b", None, &[]).0,
        Verdict::Hold { n: 90, back: Back { ref account, at, .. }, .. } if account == "a" && at == NOW + 600
    ));
}

/// ★ 用户例子二「其他三个都过 90% 才用 b，一重置就切回去」：`order: [work, personal, team, b]` · `when: ≥90%` · `preempt: true`。
/// 「备胎」不是一格：排在最后 ＋ 切回，就是它。
#[test]
fn the_last_in_order_with_preempt_is_the_spare_and_hands_back_on_reset() {
    let mut w = World::new(&["work", "personal", "team", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.seen.insert("work".into(), at(0.91, NOW + 3000));
    w.seen.insert("personal".into(), at(0.93, NOW + 1000));
    w.seen.insert("team".into(), at(0.90, NOW + 2000));
    w.seen.insert("b".into(), at(0.20, NOW + 9000));
    assert_eq!(
        w.judge("work", None, &[]).0,
        Verdict::Switch {
            to: "b".into(),
            why: SwitchWhy::Threshold { n: 90 },
            from_resets_at: Some(NOW + 3000),
            skipped: vec![]
        }
    );
    w.enter("b");
    assert_eq!(
        w.above,
        ["work", "personal", "team"],
        "换进 b 那一刻挡在前面的三个"
    );
    assert_eq!(
        w.judge("b", None, &[]).0,
        Verdict::Stay,
        "前面的都过上限 ⇒ 留在 b"
    );
    // personal 的 5h 先重置（重置时刻过了、之后没再看到 ⇒ 已重置未计时）⇒ 下一发切回 personal。
    w.now = NOW + 1000;
    assert_eq!(
        w.judge("b", None, &[]).0,
        Verdict::Switch {
            to: "personal".into(),
            why: SwitchWhy::Preempt,
            from_resets_at: None,
            skipped: vec![]
        }
    );
    // 在 personal 上：前面的 work 还过着上限 ⇒ 不换。
    assert_eq!(w.judge("personal", None, &[]).0, Verdict::Stay);
    // `preempt` 关着 ⇒ b 能用就一直用 b。
    w.preempt = false;
    assert_eq!(w.judge("b", None, &[]).0, Verdict::Stay);
}

/// ★ 不会来回抖：同一组事实判多少次都是同一个结论；换过去之后在新号上判 ⇒ 不换回来；
/// 只有重置（或时段换了上限）才改结论。
#[test]
fn the_verdict_never_flaps_between_two_accounts() {
    let mut w = World::new(&["work", "personal", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.stint.insert("b".into(), [("5h".to_string(), 5u8)].into());
    w.base.insert(
        "5h".into(),
        Base {
            used: 0.20,
            resets_at: Some(NOW + 9000),
        },
    );
    w.seen.insert("work".into(), at(0.95, NOW + 3000));
    w.seen.insert("personal".into(), at(0.95, NOW + 5000));
    w.seen.insert("b".into(), at(0.40, NOW + 9000));
    let mut current = "work".to_string();
    let mut path = vec![current.clone()];
    for _ in 0..10 {
        if let Verdict::Switch { to, .. } = w.judge(&current, None, &[]).0 {
            w.enter(&to);
            current = to;
            path.push(current.clone());
        }
    }
    assert_eq!(
        path,
        ["work", "b"],
        "到 b 之后：预算用完但没有别的能用 ⇒ 一直留在 b"
    );
    w.now = NOW + 3000;
    for _ in 0..10 {
        if let Verdict::Switch { to, .. } = w.judge(&current, None, &[]).0 {
            w.enter(&to);
            current = to;
            path.push(current.clone());
        }
    }
    assert_eq!(
        path,
        ["work", "b", "work"],
        "work 一重置就回 work，之后不再动"
    );
}

/// ★ 一份典型配置：`order: [work, personal, team, b]` · `when: ≥90%` · `personal: {cap: [{at: "01:00-20:00", n: 99}]}` ·
/// `preempt: true` · `atLimit: continue`。personal 在 1 点到 20 点能用到 99%：19:59 personal 在 95% 照用；20:01 上限回到 90% ⇒ 下一发换走。
#[test]
fn a_daytime_cap_lets_personal_run_to_ninety_nine_until_eight_pm() {
    let mut w = World::new(&["work", "personal", "team", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.at_limit = AtLimit::Continue;
    w.cap = caps(&[("personal", "*", slots("01:00-20:00", 99))]);
    let far = NOW + 3 * 86_400;
    w.seen.insert("work".into(), at(0.96, far));
    w.seen.insert("personal".into(), at(0.95, far));
    w.seen.insert("team".into(), at(0.97, far));
    w.seen.insert("b".into(), at(0.10, far));
    local(&mut w, 19, 59);
    assert_eq!(
        w.judge("personal", None, &[]).0,
        Verdict::Stay,
        "19:59：personal 的上限是 99%"
    );
    assert_eq!(
        to_of(&w.judge("work", None, &[]).0),
        Some("personal"),
        "19:59：work 过上限 ⇒ 首个能用的是 personal（不是 b）"
    );
    w.above = vec!["work".into(), "personal".into(), "team".into()];
    assert_eq!(
        to_of(&w.judge("b", None, &[]).0),
        Some("personal"),
        "19:59：在 b 上、personal 能用（换进 b 时它挡在前面）⇒ 切回 personal"
    );
    local(&mut w, 20, 1);
    assert_eq!(
        w.judge("personal", None, &[]).0,
        Verdict::Switch {
            to: "b".into(),
            why: SwitchWhy::Threshold { n: 90 },
            from_resets_at: Some(far),
            skipped: vec![]
        },
        "20:01：personal 的上限回到 90% ⇒ 换到 b"
    );
    assert_eq!(w.judge("b", None, &[]).0, Verdict::Stay);
    local(&mut w, 0, 59);
    assert_eq!(
        to_of(&w.judge("personal", None, &[]).0),
        Some("b"),
        "00:59 还不在那一段里"
    );
    local(&mut w, 1, 0);
    assert_eq!(
        w.judge("personal", None, &[]).0,
        Verdict::Stay,
        "01:00 起又是 99%"
    );
}

/// ★ 跨午夜的一段（`22:00-06:00`）：含起不含止；落不进那一段 ⇒ 落回这个号 `*` 的，再落回 `when`。按窗口写的压过 `*`。
#[test]
fn a_slot_across_midnight_and_the_fallthrough_order_of_caps() {
    let mut w = World::new(&["q", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.cap = caps(&[
        ("q", "5h", slots("22:00-06:00", 99)),
        ("q", "*", CapValue::N(80)),
    ]);
    let far = NOW + 3 * 86_400;
    w.seen.insert("q".into(), at(0.95, far));
    w.seen.insert("b".into(), at(0.10, far));
    for (hh, mm, stays) in [
        (22, 0, true),
        (23, 30, true),
        (0, 0, true),
        (5, 59, true),
        (6, 0, false),
        (12, 0, false),
        (21, 59, false),
    ] {
        local(&mut w, hh, mm);
        assert_eq!(
            w.judge("q", None, &[]).0 == Verdict::Stay,
            stays,
            "{hh:02}:{mm:02}"
        );
    }
    // 12:00 落回 `*` 的 80%：用到 85% 就过了（`when` 的 90% 不管它）。
    w.seen.insert("q".into(), at(0.85, far));
    local(&mut w, 12, 0);
    assert!(matches!(
        w.judge("q", None, &[]).0,
        Verdict::Switch {
            why: SwitchWhy::Threshold { n: 80 },
            ..
        }
    ));
}

/// 「满了才换」模式下给一个号设的上限照样算（`when` 只是缺省上限）。
#[test]
fn a_per_account_cap_applies_in_full_mode_too() {
    let mut w = World::new(&["a", "b"]);
    w.cap = caps(&[("a", "5h", CapValue::N(90))]);
    w.seen.insert("a".into(), at(0.91, NOW + 600));
    assert!(matches!(
        w.judge("a", None, &[]).0,
        Verdict::Switch { ref to, why: SwitchWhy::Threshold { n: 90 }, .. } if to == "b"
    ));
    w.seen.insert("a".into(), at(0.89, NOW + 600));
    assert_eq!(w.judge("a", None, &[]).0, Verdict::Stay);
}

/// ★ 按模型的周额度是它自己的窗口键（`7d:opus`），不并进 `7d`：卡在它上面的硬上限回来时刻照它、窗口键照实带出
/// （适配层据此在「用满」回包里标 `seven_day_opus`）。
#[test]
fn a_per_model_week_is_its_own_window_key() {
    let mut w = World::new(&["a", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.at_limit = AtLimit::Stop;
    let opus = |used: f64, resets: u64| QuotaReading {
        status: None,
        refused: false,
        limiting: Some("seven_day_opus".into()),
        resets_at: Some(resets),
        windows: vec![
            QuotaWindow {
                name: "five_hour".into(),
                used: Some(0.1),
                resets_at: Some(NOW + 600),
                warned_at: None,
            },
            QuotaWindow {
                name: "seven_day_opus".into(),
                used: Some(used),
                resets_at: Some(resets),
                warned_at: None,
            },
        ],
        overage: None,
    };
    w.seen.insert("a".into(), opus(0.95, NOW + 50_000));
    w.seen.insert("b".into(), opus(0.97, NOW + 80_000));
    assert_eq!(
        w.judge("a", None, &[]).0,
        Verdict::Hold {
            n: 90,
            back: Back {
                account: "a".into(),
                at: NOW + 50_000,
                slot: Some("7d:opus".into()),
            },
            skipped: vec![],
        }
    );
    // 给 opus 那一档单独放宽 ⇒ 只有它一个窗口过的号又能用了。
    w.cap = caps(&[("b", "7d:opus", CapValue::N(99))]);
    assert_eq!(to_of(&w.judge("a", None, &[]).0), Some("b"));
}

/// ★ personal 20:00 换走、01:00 回来：不靠重置，靠时段换了上限 —— 换进 b 那一刻 personal 挡在前面（过了 90%），
/// 01:00 起它的上限回到 99% ⇒ 又能用了 ⇒ 下一发切回 personal（前面的 work 还过着上限）。
#[test]
fn personal_comes_back_at_one_am_through_its_slot_not_a_reset() {
    let mut w = World::new(&["work", "personal", "team", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.cap = caps(&[("personal", "*", slots("01:00-20:00", 99))]);
    let far = NOW + 3 * 86_400;
    w.seen.insert("work".into(), at(0.96, far));
    w.seen.insert("personal".into(), at(0.95, far));
    w.seen.insert("team".into(), at(0.97, far));
    w.seen.insert("b".into(), at(0.10, far));
    local(&mut w, 20, 1);
    assert_eq!(to_of(&w.judge("personal", None, &[]).0), Some("b"));
    w.enter("b");
    assert_eq!(w.above, ["work", "personal", "team"]);
    local(&mut w, 23, 0);
    assert_eq!(
        w.judge("b", None, &[]).0,
        Verdict::Stay,
        "23:00 personal 还过着 90%"
    );
    w.now += 2 * 3600; // 01:00（同一个窗口，没有重置）
    assert_eq!(
        w.judge("b", None, &[]).0,
        Verdict::Switch {
            to: "personal".into(),
            why: SwitchWhy::Preempt,
            from_resets_at: None,
            skipped: vec![]
        }
    );
}

/// ★ 手动换号不会被切回：z 能用时手动换到 b ⇒ 换进 b 那一刻没有挡在前面的号 ⇒ 开着 `preempt` 也一直留在 b；
/// 直到 b 自己不能用（或用完单段预算）才走。
#[test]
fn a_manual_switch_is_not_undone_by_preempt() {
    let mut w = World::new(&["z", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.seen.insert("z".into(), at(0.20, NOW + 3000));
    w.seen.insert("b".into(), at(0.30, NOW + 3000));
    w.enter("b"); // 手动换过去
    assert!(w.above.is_empty());
    for _ in 0..3 {
        assert_eq!(w.judge("b", None, &[]).0, Verdict::Stay);
    }
    w.seen.insert("b".into(), at(0.91, NOW + 3000));
    assert_eq!(
        to_of(&w.judge("b", None, &[]).0),
        Some("z"),
        "b 自己过上限 ⇒ 照常换走"
    );
}

/// ★ 上限 0 ＝ 这一时段不用这个号：一份配置 `q: {"*": [17:00-02:00 → 0, 02:00-17:00 → 99]}`。
/// 17 点到次日 2 点 q 不能用 —— 刚重置的 0%、从没见过都不选；「下一个」与真换号同一处判（`next_of` 也跳过它）；
/// 跨午夜那一段 01:59 仍不用、02:00 起又能用；q 被挡时换走的会话在 02:00 经 `preempt` 切回（与「时段换了上限」同一条）。
#[test]
fn a_cap_of_zero_keeps_an_account_out_for_its_slot_and_preempt_brings_it_back() {
    let mut w = World::new(&["q", "z"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.cap = caps(&[(
        "q",
        "*",
        CapValue::Slots(vec![
            CapSlot {
                at: "17:00-02:00".into(),
                n: 0,
            },
            CapSlot {
                at: "02:00-17:00".into(),
                n: 99,
            },
        ]),
    )]);
    let far = NOW + 3 * 86_400;
    w.seen.insert("z".into(), at(0.10, far));
    // 刚重置的 0%（上一窗的重置时刻已过）也不选。
    local(&mut w, 18, 0);
    w.seen.insert("q".into(), at(0.0, w.now - 60));
    let (v, _) = w.judge("q", None, &[]);
    assert_eq!(to_of(&v), Some("z"), "18:00 q 不用 ⇒ 换走：{v:?}");
    assert!(
        matches!(
            &v,
            Verdict::Switch {
                why: SwitchWhy::Threshold { n: 0 },
                ..
            }
        ),
        "{v:?}"
    );
    // 「下一个」同一处判：z 当前时下一个不会是 q。
    let next = w.with("z", None, &[], |f| super::next_of(f, &mut |_| Ok(())));
    assert_eq!(next, None, "q 此刻不用，不该排成下一个");
    // 从没见过也不选。
    w.seen.remove("q");
    assert_eq!(to_of(&w.judge("q", None, &[]).0), Some("z"));
    // 回来的时刻 ＝ 那一段的止（次日 02:00 本地）。
    let back = w.with("z", None, &[], |f| super::back_at("q", f));
    let two_am = DAY + 86_400 + 2 * 3600 - 8 * 3600 + 86_400;
    assert_eq!(back, Some(two_am));
    // 换到 z；跨午夜：01:59 仍不用，不切回；02:00 起 q 又能用 ⇒ 切回。
    w.seen.insert("q".into(), at(0.0, w.now - 60));
    w.enter("z");
    assert_eq!(w.above, ["q"]);
    local(&mut w, 1, 59);
    w.now += 86_400; // 次日 01:59
    assert_eq!(w.judge("z", None, &[]).0, Verdict::Stay, "01:59 q 还不用");
    w.now += 60; // 02:00
    assert_eq!(w.now, two_am);
    assert_eq!(
        w.judge("z", None, &[]).0,
        Verdict::Switch {
            to: "q".into(),
            why: SwitchWhy::Preempt,
            from_resets_at: None,
            skipped: vec![]
        }
    );
    // 写死的 0：一直不用、说不出几点回来。
    w.cap = caps(&[("q", "5h", CapValue::N(0))]);
    assert!(!w.with("z", None, &[], |f| super::standing("q", f)).usable());
    assert_eq!(w.with("z", None, &[], |f| super::back_at("q", f)), None);
}

// ── 切兜底前等一等：非兜底的号很快回来就停着等，不为几十分钟切到兜底号 ─────────────────────

fn back_of(v: &Verdict) -> Option<(&str, u64, &str)> {
    match v {
        Verdict::Wait { instead, back, .. } => {
            Some((back.account.as_str(), back.at, instead.as_str()))
        }
        _ => None,
    }
}

/// 用户那套：`[a, b]`，b 是兜底，等 40 分钟。
fn with_fallback(pool: &[&str], fallback: &[&str]) -> World {
    let mut w = World::new(pool);
    w.fallback = fallback.iter().map(|s| s.to_string()).collect();
    w.wait = 40;
    w
}

/// ★ 非兜底的号 39 分钟后重置 ⇒ 不切兜底 b、停着等；41 分钟 ⇒ 切 b；不等（0）⇒ 立刻切。
#[test]
fn a_non_fallback_account_back_within_the_wait_keeps_the_fallback_unused() {
    let mut w = with_fallback(&["a", "b"], &["b"]);
    let at39 = reading(true, 1.0, Some(NOW + 39 * 60));
    let (v, _) = w.judge("a", Some(&at39), &["a"]);
    assert_eq!(back_of(&v), Some(("a", NOW + 39 * 60, "b")), "{v:?}");
    let at41 = reading(true, 1.0, Some(NOW + 41 * 60));
    assert_eq!(to_of(&w.judge("a", Some(&at41), &["a"]).0), Some("b"));
    w.wait = 0;
    assert_eq!(to_of(&w.judge("a", Some(&at39), &["a"]).0), Some("b"));
}

/// ★ 往非兜底号切 ⇒ 不等（哪怕此刻的号 1 分钟后就回来）；没标兜底 ⇒ 等于不等（今天的行为）。
#[test]
fn switching_to_a_non_fallback_account_never_waits() {
    let w = with_fallback(&["a", "c", "b"], &["b"]);
    let soon = reading(true, 1.0, Some(NOW + 60));
    assert_eq!(to_of(&w.judge("a", Some(&soon), &["a"]).0), Some("c"));
    let none = with_fallback(&["a", "b"], &[]);
    assert_eq!(to_of(&none.judge("a", Some(&soon), &["a"]).0), Some("b"));
}

/// 抢回开着也一样：不出现「切到兜底 → 一会儿又切回」。
#[test]
fn preempt_on_does_not_flip_to_the_fallback_for_a_while() {
    let mut w = with_fallback(&["a", "b"], &["b"]);
    w.preempt = true;
    let soon = reading(true, 1.0, Some(NOW + 60));
    assert_eq!(
        back_of(&w.judge("a", Some(&soon), &["a"]).0),
        Some(("a", NOW + 60, "b"))
    );
}

/// 等的是任一非兜底的号，不只此刻的：已在 c 上、c 一小时后才回来，a 2 分钟后回来 ⇒ 等 a，不切兜底 b。
#[test]
fn any_non_fallback_account_counts() {
    let w = with_fallback(&["a", "c", "b"], &["b"]);
    let mut w = w;
    w.seen
        .insert("a".into(), reading(true, 1.0, Some(NOW + 120)));
    let c = reading(true, 1.0, Some(NOW + 3600));
    assert_eq!(
        back_of(&w.judge("c", Some(&c), &["c"]).0),
        Some(("a", NOW + 120, "b"))
    );
}

/// 此刻的号只是过了阈值（还发得出去）：软上限 ⇒ 留着照发（不切、也不停）；硬上限 ⇒ 停着等。
#[test]
fn over_a_soft_threshold_stays_and_a_hard_one_waits() {
    let mut w = with_fallback(&["a", "b"], &["b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.seen
        .insert("a".into(), reading(false, 0.95, Some(NOW + 180)));
    assert_eq!(w.judge("a", None, &[]).0, Verdict::Stay);
    w.at_limit = AtLimit::Stop;
    assert_eq!(
        back_of(&w.judge("a", None, &[]).0),
        Some(("a", NOW + 180, "b"))
    );
}

/// 这一家给不出「用满」回包 ⇒ 停不了，照旧切。
#[test]
fn without_a_limit_reply_it_switches_as_before() {
    let mut w = with_fallback(&["a", "b"], &["b"]);
    w.can_hold = false;
    let soon = reading(true, 1.0, Some(NOW + 60));
    assert_eq!(to_of(&w.judge("a", Some(&soon), &["a"]).0), Some("b"));
}

/// ★ 换法「单段预算 N 点」写成 `stint: {"*": {"*": n}}`：所有号都照它让位（号自己那一行优先）。
#[test]
fn a_stint_for_all_accounts_applies_to_whoever_is_current() {
    let mut w = World::new(&["a", "b"]);
    w.stint
        .entry("*".into())
        .or_default()
        .insert("*".into(), 10);
    w.base.insert(
        "5h".into(),
        crate::accounts::quota::rotation::Base {
            used: 0.20,
            resets_at: Some(NOW + 3600),
        },
    );
    w.seen
        .insert("b".into(), reading(false, 0.0, Some(NOW + 3600)));
    w.seen
        .insert("a".into(), reading(false, 0.31, Some(NOW + 3600)));
    assert!(
        matches!(w.judge("a", None, &[]).0, Verdict::Switch { ref to, why: SwitchWhy::Stint { n: 10, .. }, .. } if to == "b")
    );
    w.stint
        .entry("a".into())
        .or_default()
        .insert("*".into(), 20);
    assert_eq!(w.judge("a", None, &[]).0, Verdict::Stay, "号自己那一行优先");
}

// ── 预览（`rotation-plan` 的 plan · lanes · effective）──────────────────────────────

use super::{effective_cap, lane, plan, CapAt, CapLayer, LaneSpan, LaneState, PlanStep};

impl World {
    fn plan(&self, current: &str, until: u64) -> Vec<PlanStep> {
        self.with(current, None, &[], |f| {
            plan(f, until, &mut |a| {
                self.unready.get(a).map_or(Ok(()), |u| Err(*u))
            })
        })
    }
}

fn step(from: u64, to: u64, account: Option<&str>, why: Option<SwitchWhy>) -> PlanStep {
    PlanStep {
        from,
        to,
        account: account.map(str::to_string),
        why,
    }
}

/// ★ 预览照 `decide` 走：此刻 work 过 90% ⇒ 换到 b；抢回开着 ⇒ 前面的号一重置就切回（personal 先、work 后）；
/// 用量只按此刻的算 ⇒ 只在重置时刻变。
#[test]
fn the_plan_follows_decide_through_resets_and_preempt() {
    let mut w = World::new(&["work", "personal", "team", "b"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.preempt = true;
    w.seen.insert("work".into(), at(0.91, NOW + 3000));
    w.seen.insert("personal".into(), at(0.93, NOW + 1000));
    w.seen.insert("team".into(), at(0.90, NOW + 2000));
    w.seen.insert("b".into(), at(0.20, NOW + 9000));
    let end = NOW + 4 * 3600;
    assert_eq!(
        w.plan("work", end),
        vec![
            step(
                NOW,
                NOW + 1000,
                Some("b"),
                Some(SwitchWhy::Threshold { n: 90 })
            ),
            step(
                NOW + 1000,
                NOW + 3000,
                Some("personal"),
                Some(SwitchWhy::Preempt)
            ),
            step(NOW + 3000, end, Some("work"), Some(SwitchWhy::Preempt)),
        ]
    );
    // 抢回关着 ⇒ 换到 b 之后一直 b。
    w.preempt = false;
    assert_eq!(
        w.plan("work", end),
        vec![step(
            NOW,
            end,
            Some("b"),
            Some(SwitchWhy::Threshold { n: 90 })
        )]
    );
}

/// ★ 时段停用：work 17:00–02:00 上限 0（本地钟）⇒ 17:00 换到 personal，02:00 抢回；lanes 里 work 那一段是 `off`。
#[test]
fn a_slot_off_shows_in_the_plan_and_the_lane() {
    let mut w = World::new(&["work", "personal"]);
    w.preempt = true;
    w.cap = caps(&[("work", "*", slots("17:00-02:00", 0))]);
    local(&mut w, 16, 0);
    let (five, two) = (w.now + 3600, w.now + 10 * 3600);
    let end = w.now + 12 * 3600;
    assert_eq!(
        w.plan("work", end),
        vec![
            step(w.now, five, Some("work"), None),
            step(
                five,
                two,
                Some("personal"),
                Some(SwitchWhy::Threshold { n: 0 })
            ),
            step(two, end, Some("work"), Some(SwitchWhy::Preempt)),
        ]
    );
    let lanes = w.with("work", None, &[], |f| {
        (lane(f, "work", end), lane(f, "personal", end))
    });
    assert_eq!(
        lanes,
        (
            vec![LaneSpan {
                from: five,
                to: two,
                state: LaneState::Off,
                n: None
            }],
            vec![]
        )
    );
}

/// ★ 都不能用 ＋ 停 ⇒ 预览里那一段不发上游（`account: null` · `held`），到最早回来的号起接着走；被拒的号 lanes 里是 `refused`。
#[test]
fn a_hold_is_a_gap_in_the_plan_until_the_earliest_is_back() {
    let mut w = World::new(&["work", "personal"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.at_limit = AtLimit::Stop;
    w.seen
        .insert("work".into(), reading(true, 1.0, Some(NOW + 1800)));
    w.seen.insert("personal".into(), at(0.95, NOW + 600));
    let end = NOW + 3600;
    assert_eq!(
        w.plan("work", end),
        vec![
            step(NOW, NOW + 600, None, Some(SwitchWhy::Held { n: 90 })),
            step(NOW + 600, end, Some("personal"), Some(full5h())),
        ]
    );
    let work = w.with("work", None, &[], |f| lane(f, "work", end));
    assert_eq!(
        work,
        vec![LaneSpan {
            from: NOW,
            to: NOW + 1800,
            state: LaneState::Refused,
            n: None
        }]
    );
}

/// ★ 此刻实际取的上限与来自哪一层，和「这一格不算」时往下一层取到的（封顶浮层「其余时段 ＝ …」）。
#[test]
fn effective_caps_name_their_layer_and_the_one_below() {
    let mut w = World::new(&["work", "personal"]);
    w.when = RotationWhen::Threshold { n: 90 };
    w.cap = caps(&[
        ("work", "*", CapValue::N(99)),
        ("work", "5h", slots("17:00-02:00", 0)),
    ]);
    local(&mut w, 20, 0);
    let at = |v: Option<u8>, layer| CapAt { v, layer };
    let got = w.with("work", None, &[], |f| {
        (
            effective_cap(f, "work", "5h"),
            effective_cap(f, "work", "*"),
            effective_cap(f, "personal", "5h"),
        )
    });
    assert_eq!(
        got,
        (
            (at(Some(0), CapLayer::Window), at(Some(99), CapLayer::All)),
            (at(Some(99), CapLayer::All), at(Some(90), CapLayer::Trigger)),
            (
                at(Some(90), CapLayer::Trigger),
                at(Some(90), CapLayer::Trigger)
            ),
        )
    );
    // 时段外（10:00）⇒ 5h 那一格落不进 ⇒ 取全部窗口；满了才换、没设 ⇒ 不封顶。
    local(&mut w, 10, 0);
    w.when = RotationWhen::Full;
    let got = w.with("work", None, &[], |f| {
        (
            effective_cap(f, "work", "5h"),
            effective_cap(f, "personal", "*"),
        )
    });
    assert_eq!(
        got,
        (
            (at(Some(99), CapLayer::All), at(Some(99), CapLayer::All)),
            (at(None, CapLayer::None), at(None, CapLayer::None)),
        )
    );
}
