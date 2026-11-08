//! 额度显示态：每一态一条（假额度账 ＋ 假时钟）；「快满」落 80%（线是策略的设置，后端说不出「到线」）；数旧 30 分钟边界两侧各一条；订阅标识不含原值。

use super::*;
use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};

const NOW: u64 = 1_800_000_000;
const LATER: u64 = NOW + 3_600;
const EARLIER: u64 = NOW - 60;

fn slot(w: &str) -> Option<&'static str> {
    crate::agents::claudecode::quota::slot_of(w)
}

fn win(name: &str, used: f64, resets_at: u64) -> QuotaWindow {
    QuotaWindow {
        name: name.into(),
        used: Some(used),
        resets_at: Some(resets_at),
        warned_at: None,
    }
}

/// 一份订阅号的快照：5h 用 `used_5h`、7d 两个窗口（取用得多的那个）。
fn sub(used_5h: f64, resets_5h: u64) -> QuotaReading {
    QuotaReading {
        status: Some(QuotaStatus::Allowed),
        refused: false,
        limiting: Some("five_hour".into()),
        resets_at: Some(resets_5h),
        windows: vec![
            win("five_hour", used_5h, resets_5h),
            win("seven_day", 0.41, NOW + 5 * 86_400),
            win("seven_day_overage_included", 0.12, NOW + 5 * 86_400),
        ],
        overage: None,
    }
}

fn facts(kind: Kind) -> Facts {
    Facts {
        kind,
        login: LoginState::Ok,
    }
}

fn state_of(r: &QuotaReading, kind: Kind) -> QuotaState {
    show(Some((r, NOW)), facts(kind), NOW, &slot).state
}

/// ★ 每一态各一条。
#[test]
fn every_state_comes_out_of_its_own_facts() {
    assert_eq!(
        show(None, facts(Kind::Sub), NOW, &slot).state,
        QuotaState::Unseen
    );
    assert_eq!(state_of(&sub(0.63, LATER), Kind::Sub), QuotaState::Ok);
    assert_eq!(state_of(&sub(0.86, LATER), Kind::Sub), QuotaState::Near);
    let mut refused = sub(1.0, LATER);
    refused.refused = true;
    refused.status = Some(QuotaStatus::Rejected);
    assert_eq!(state_of(&refused, Kind::Sub), QuotaState::Refused);
    let mut overage = sub(1.0, LATER);
    overage.status = Some(QuotaStatus::Rejected);
    overage.overage = Some(QuotaOverage {
        status: Some(QuotaStatus::Allowed),
        resets_at: None,
        disabled: None,
        in_use: true,
    });
    assert_eq!(state_of(&overage, Kind::Sub), QuotaState::OverageInUse);
    assert_eq!(
        state_of(&sub(0.63, EARLIER), Kind::Sub),
        QuotaState::ResetSinceSeen,
        "按钮那个窗口看到之后已重置"
    );
    refused.resets_at = Some(EARLIER);
    assert_eq!(
        state_of(&refused, Kind::Sub),
        QuotaState::Near,
        "被拒到的那一刻已过、窗口还没重置 ⇒ 不再画被拒，照窗口的数判"
    );
    let mut over = sub(1.0, EARLIER);
    over.refused = true;
    over.resets_at = Some(EARLIER);
    assert_eq!(
        state_of(&over, Kind::Sub),
        QuotaState::ResetSinceSeen,
        "被拒到的那一刻与卡着的窗口都已过"
    );
    // 按量号：没有分窗口的数；被拒照它说的回来时刻。
    let mut api_refused = QuotaReading {
        status: None,
        refused: true,
        limiting: None,
        resets_at: Some(LATER),
        windows: Vec::new(),
        overage: None,
    };
    let s = show(Some((&api_refused, NOW)), facts(Kind::Api), NOW, &slot);
    assert_eq!(
        (s.state, s.limiting, s.slots.len()),
        (QuotaState::Refused, None, 0)
    );
    // 说不出回来时刻的旧账（适配层今天不再出这一形）⇒ 不算被拒，与轮换同一个判法。
    api_refused.resets_at = None;
    assert_eq!(state_of(&api_refused, Kind::Api), QuotaState::Ok);
}

/// ★ 数旧：看到距今恰好 30 分钟不旧，多一秒就旧；与态叠着出。
#[test]
fn stale_flips_one_second_past_thirty_minutes() {
    let r = sub(0.63, NOW + 7_200);
    let at = |dt: u64| show(Some((&r, NOW)), facts(Kind::Sub), NOW + dt, &slot);
    assert!(!at(STALE_AFTER).stale);
    let s = at(STALE_AFTER + 1);
    assert!(s.stale);
    assert_eq!(s.state, QuotaState::Ok);
}

/// 每窗取整、只出语义位；同一语义位取用得多的那个；按钮的窗口照回包说的那个。
#[test]
fn slots_are_rounded_and_named_by_their_semantic_position() {
    let s = show(
        Some((&sub(0.625, LATER), NOW)),
        facts(Kind::Sub),
        NOW,
        &slot,
    );
    assert_eq!(s.limiting.as_deref(), Some("5h"));
    assert_eq!(
        s.slots,
        vec![
            SlotShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                reset_text: None,
                slot: "5h".into(),
                pct: Some(63),
                resets_at: Some(LATER),
                full: false,
                text: Words::default(),
                tone: crate::common::cells::Tone::Plain,
                per_pct_text: None,
                window: "five_hour".into(),
            },
            SlotShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                reset_text: None,
                slot: "7d".into(),
                pct: Some(41),
                resets_at: Some(NOW + 5 * 86_400),
                full: false,
                text: Words::default(),
                tone: crate::common::cells::Tone::Plain,
                per_pct_text: None,
                window: "seven_day".into(),
            },
        ]
    );
}

/// ★ 用满与被拒分开：用到 100%、未重置才 `full`；被拒而没用满（短时限流）不 `full`；99.6% 取整成 100 也不算用满；窗口重置过了不算。
#[test]
fn full_is_a_window_at_a_hundred_not_a_refusal() {
    let full_of = |used: f64, resets: u64, refused: bool| {
        let mut r = sub(used, resets);
        r.refused = refused;
        r.resets_at = Some(LATER);
        show(Some((&r, NOW)), facts(Kind::Sub), NOW, &slot).slots[0].clone()
    };
    let s = full_of(1.0, LATER, true);
    assert_eq!((s.full, full_of(1.0, LATER, true).pct), (true, Some(100)));
    assert!(full_of(1.03, LATER, false).full, "用满不看被拒没有");
    let s = full_of(0.58, LATER, true);
    assert_eq!((s.full, s.pct), (false, Some(58)), "被拒而没用满");
    let s = full_of(0.996, LATER, false);
    assert_eq!((s.full, s.pct), (false, Some(100)));
    assert!(!full_of(1.0, EARLIER, false).full, "窗口已重置");
    let wire = serde_json::to_value(full_of(1.0, LATER, true)).expect("json");
    assert_eq!(wire["full"], true);
    assert!(
        serde_json::to_value(full_of(0.5, LATER, false))
            .expect("json")
            .get("full")
            .is_none(),
        "没用满 ⇒ 缺"
    );
}

/// 一份账：5h 用 30%、7d 两个窗口（`seven_day` 40% · `seven_day_opus` 85%），5h 与 opus 量过「1% ≈ 多少」。
fn observed() -> crate::accounts::quota::ledger::Observed {
    use crate::accounts::quota::ledger::{Observed, PerPct};
    let mut r = sub(0.3, LATER);
    r.windows = vec![
        win("five_hour", 0.3, LATER),
        win("seven_day", 0.4, LATER + 86_400),
        win("seven_day_opus", 0.85, LATER + 86_400),
    ];
    Observed {
        agent: "claude".into(),
        account: "a".into(),
        seen_at: NOW,
        reading: r,
        windows_seen: Default::default(),
        trend: Default::default(),
        per_pct: [
            (
                "five_hour".to_string(),
                PerPct {
                    tokens: 2_100_000,
                    at: NOW,
                },
            ),
            (
                "seven_day_opus".to_string(),
                PerPct {
                    tokens: 25_000,
                    at: NOW,
                },
            ),
        ]
        .into(),
        history: Default::default(),
    }
}

/// ★ 语义位取哪个窗口与事实同一个挑法：用得多的那个已经重置 ⇒ 取还在计时的那个，用满也不照重置前的数判。
#[test]
fn a_slot_takes_the_live_window_not_one_already_reset() {
    let o = observed();
    let mut r = o.reading.clone();
    r.windows[2].used = Some(1.0);
    r.windows[2].resets_at = Some(EARLIER);
    let sh = show(Some((&r, o.seen_at)), facts(Kind::Sub), NOW, &slot);
    let s = &sh.slots[1];
    assert_eq!(
        (s.window.as_str(), s.pct, s.full),
        ("seven_day", Some(40), false)
    );
}
