//! 号的事实：每窗单独的被拒 · 周窗固定时刻往后推（读数全是现编的）。

use super::*;
use crate::agents::{QuotaReading, QuotaWindow};

/// 10-23 22:00 UTC：上次周重置。
const LAST_WEEK_RESET: u64 = 1_792_792_800;
/// 10-30 10:00 UTC：此刻。
const NOW: u64 = 1_793_354_400;
const WEEK: u64 = 7 * 86_400;

fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
    QuotaWindow {
        name: name.to_string(),
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

fn fact(r: Option<&QuotaReading>, now: u64) -> Fact {
    let none_f = |_: &str| None;
    let none_u = |_: &str| None;
    fact_of(
        "a",
        r,
        None,
        Kind::Sub,
        LoginState::Ok,
        None,
        now,
        &crate::agents::claudecode::quota::slot_of,
        &Measured {
            rate: &none_f,
            per_pct: &none_u,
        },
        &|t| t.to_string(),
    )
}

/// ★ 周窗每个号有固定的每周重置时刻、跟用不用无关：窗不在计时、读数里留着上次的重置 ⇒ 交下一次 ＝ 上次 ＋ 7 天的整数倍里
/// 第一个在此刻之后的；窗照旧算没开（用量 0）。5 小时窗由用触发，不推。
#[test]
fn a_closed_week_projects_its_fixed_reset_forward() {
    let r = reading(vec![
        win("five_hour", 0.8, NOW - 3_600),
        win("seven_day", 0.6, LAST_WEEK_RESET),
    ]);
    let f = fact(Some(&r), NOW);
    assert!(!f.w7d.open);
    assert_eq!(f.w7d.used, 0.0);
    assert_eq!(f.w7d.resets, Some(LAST_WEEK_RESET + WEEK), "10-30 22:00");
    assert_eq!(f.w7d.resets_text.as_deref(), Some("1793397600"));
    assert!(!f.w5h.open);
    assert_eq!(f.w5h.resets, None, "5 小时窗不推");
    // 隔了几周没读数 ⇒ 跳过整几周，落到此刻之后的第一个。
    let f = fact(Some(&r), NOW + 3 * WEEK);
    assert_eq!(f.w7d.resets, Some(LAST_WEEK_RESET + 4 * WEEK));
    // 正好落在此刻 ⇒ 那一次已经过了，交下一周。
    let f = fact(Some(&r), LAST_WEEK_RESET + WEEK);
    assert_eq!(f.w7d.resets, Some(LAST_WEEK_RESET + 2 * WEEK));
    // 在计时的照读到的交。
    let open = reading(vec![win("seven_day", 0.6, NOW + 600)]);
    let f = fact(Some(&open), NOW);
    assert!(f.w7d.open);
    assert_eq!(f.w7d.resets, Some(NOW + 600));
    // 读数里没留重置时刻 ⇒ 说不出，不编。
    let mut bare = reading(vec![win("seven_day", 0.6, 0)]);
    bare.windows[0].resets_at = None;
    assert_eq!(fact(Some(&bare), NOW).w7d.resets, None);
    assert_eq!(fact(None, NOW).w7d.resets, None);
}

/// ★ 推出来的重置不挡发：周窗没开 ⇒ 发得出去。
#[test]
fn a_projected_week_reset_does_not_block() {
    let r = reading(vec![win("seven_day", 0.99, LAST_WEEK_RESET)]);
    assert!(fact(Some(&r), NOW).blocked.is_none());
}

/// ★ 每窗单独被拒的号（周窗 97%、`7d-status: rejected`、没有总的 `reset`）：短期限过了以后仍被拒到周窗重置，不当成发得出去。
#[test]
fn a_week_refused_at_97_stays_refused_until_its_own_reset() {
    let week_reset = NOW + 2 * 86_400;
    let h = |pairs: &[(&str, String)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    };
    let r = crate::agents::claudecode::quota::read(
        429,
        &h(&[
            ("anthropic-ratelimit-unified-status", "rejected".into()),
            ("anthropic-ratelimit-unified-5h-status", "allowed".into()),
            ("anthropic-ratelimit-unified-7d-status", "rejected".into()),
            ("anthropic-ratelimit-unified-7d-utilization", "0.97".into()),
            (
                "anthropic-ratelimit-unified-7d-reset",
                week_reset.to_string(),
            ),
        ]),
        NOW,
    )
    .expect("被拒");
    let later = NOW + 3_600;
    let b = fact(Some(&r), later).blocked.expect("被拒着");
    assert_eq!(b.why, FactWhy::Refused);
    assert_eq!(b.until, Some(week_reset));
}
