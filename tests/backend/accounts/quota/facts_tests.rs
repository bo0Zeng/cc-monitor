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

/// ★ 那一家不归我方续的账号 0 令牌过期了（[`LoginState::BaseUntouched`]）⇒ 用不了、原因单列（不当要重新登录），
/// 那一句是后端写好的「不由 cc-monitor 续」；两窗有数也一样。
#[test]
fn an_account_zero_left_to_its_family_is_blocked_with_its_own_why() {
    let none_f = |_: &str| None;
    let none_u = |_: &str| None;
    let r = reading(vec![win("five_hour", 0.1, NOW + 600)]);
    let f = fact_of(
        "0",
        Some(&r),
        Kind::Sub,
        LoginState::BaseUntouched,
        None,
        NOW,
        &crate::agents::claudecode::quota::slot_of,
        &Measured {
            rate: &none_f,
            per_pct: &none_u,
        },
        &|t| t.to_string(),
    );
    assert_eq!(f.login, LoginState::BaseUntouched);
    let b = f.blocked.expect("用不了");
    assert_eq!(b.why, FactWhy::BaseUntouched);
    assert_eq!(b.until, None);
    assert_eq!(
        b.text,
        copy_core::copy_text("beRotation.fact.baseUntouched", &[])
    );
}

/// 账上一个号那一条：`seen` ＝ 每个窗口几点、从哪看到的（没列的 ＝ 与 `seen_at` 同一刻、来自回包头）。
fn observed(
    windows: Vec<QuotaWindow>,
    seen_at: u64,
    seen: &[(&str, u64, crate::accounts::quota::ledger::Source)],
) -> crate::accounts::quota::ledger::Observed {
    crate::accounts::quota::ledger::Observed {
        agent: "claude-code".into(),
        account: "d".into(),
        seen_at,
        reading: reading(windows),
        windows_seen: seen
            .iter()
            .map(|&(n, at, from)| {
                (
                    n.to_string(),
                    crate::accounts::quota::ledger::WindowSeen { at, from },
                )
            })
            .collect(),
        trend: Default::default(),
        per_pct: Default::default(),
        history: Default::default(),
        reread_failed: Default::default(),
    }
}

/// ★ 周额度号照此刻取到的窗口判，不照整本账：账上留着一格过了重置的 7d（早先一发回包头说的）、此刻回包头只说 5 小时 ⇒ 不算周额度号；
/// 对照：此刻那份回包头说着 7d（窗已过了重置、照固定时刻往后推）⇒ 照算；报用量那一档最近一次说的周窗也照算。
#[test]
fn a_week_left_on_the_books_but_absent_from_the_reading_now_does_not_make_a_weekly_account() {
    use crate::accounts::quota::ledger::Source;
    let stale = observed(
        vec![
            win("five_hour", 0.0, NOW + 3_600),
            win("seven_day", 0.27, NOW - 600),
        ],
        NOW - 60,
        &[("seven_day", NOW - 3 * 86_400, Source::Headers)],
    );
    let f = fact(Some(&current(&stale, NOW).reading), NOW);
    assert!(!f.weekly, "{f:?}");
    assert!(!f.w7d.open, "{f:?}");

    let idle = observed(
        vec![
            win("five_hour", 0.3, NOW - 3_600),
            win("seven_day", 0.27, NOW - 600),
        ],
        NOW - 86_400,
        &[],
    );
    let f = fact(Some(&current(&idle, NOW).reading), NOW);
    assert!(f.weekly, "{f:?}");
    assert_eq!(f.w7d.resets, Some(NOW - 600 + WEEK));

    let probed = observed(
        vec![
            win("five_hour", 0.3, NOW + 3_600),
            win("seven_day_fable", 0.1, NOW + 86_400),
        ],
        NOW - 60,
        &[("seven_day_fable", NOW - 600, Source::Usage)],
    );
    assert!(fact(Some(&current(&probed, NOW).reading), NOW).weekly);
}

/// ★ 同一来源先报 5 小时 ＋ 周窗、再报只带 5 小时 ⇒ 周窗照上一份留，直到过了它自己的重置时刻；过了 ⇒ 事实里不出（不算周额度号）。
#[test]
fn a_week_its_source_left_out_of_the_latest_report_stays_until_its_own_reset() {
    use crate::accounts::quota::ledger::Source;
    let reset_7d = NOW + 86_400;
    let o = observed(
        vec![
            win("five_hour", 0.2, NOW + 3_600),
            win("seven_day", 0.27, reset_7d),
        ],
        NOW - 60,
        &[("seven_day", NOW - 600, Source::Headers)],
    );
    let f = fact(Some(&current(&o, NOW).reading), NOW);
    assert!(f.weekly, "{f:?}");
    assert!(f.w7d.open, "{f:?}");
    assert_eq!(f.w7d.resets, Some(reset_7d));

    let later = reset_7d + 60;
    let r = current(&o, later).reading;
    assert!(
        !r.windows.iter().any(|w| w.name == "seven_day"),
        "{:?}",
        r.windows
    );
    assert!(!fact(Some(&r), later).weekly);

    // 说不出几点重置的那一格无从「留到它自己的重置」⇒ 同来源最近一份没带就不留。
    let mut blind = win("seven_day", 0.27, 0);
    blind.resets_at = None;
    let o = observed(
        vec![win("five_hour", 0.2, NOW + 3_600), blind],
        NOW - 60,
        &[("seven_day", NOW - 600, Source::Headers)],
    );
    let r = current(&o, NOW).reading;
    assert!(
        !r.windows.iter().any(|w| w.name == "seven_day"),
        "{:?}",
        r.windows
    );
}

/// ★ 出口那一下：后端行造线上的事实，每一格照原样搬过去（两窗各归各位），只给后端看的重置的字留下不上线。
#[test]
fn the_exit_carries_every_wire_cell_and_leaves_the_backend_cell_behind() {
    let none_f = |_: &str| None;
    let none_u = |_: &str| Some(1_234);
    let r = reading(vec![
        win("five_hour", 0.42, NOW + 600),
        win("seven_day", 0.27, NOW + 86_400),
    ]);
    let f = fact_of(
        "a",
        Some(&r),
        Kind::Sub,
        LoginState::Ok,
        None,
        NOW,
        &crate::agents::claudecode::quota::slot_of,
        &Measured {
            rate: &none_f,
            per_pct: &none_u,
        },
        &|t| t.to_string(),
    );
    assert_eq!(f.w5h.resets_text.as_deref(), Some("1793355000"));
    let win_of = |w: &WinFact| cmd_wire::rotation::WinFact {
        open: w.open,
        used: w.used,
        resets: w.resets,
        rate: w.rate,
        tokens_per_pct: w.tokens_per_pct,
    };
    let want = cmd_wire::rotation::Fact {
        account: f.account.clone(),
        kind: f.kind,
        w5h: win_of(&f.w5h),
        w7d: win_of(&f.w7d),
        weekly: f.weekly,
        login: f.login,
        blocked: f.blocked.clone(),
    };
    assert_ne!(want.w5h, want.w7d, "两窗得不一样，搬错位才看得出");
    assert_eq!(f.wire(), want);
}
