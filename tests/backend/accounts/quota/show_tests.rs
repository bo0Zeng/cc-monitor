//! 额度显示态：每一态一条（假额度账 ＋ 假时钟）；「快满」按窗跟着这号这一窗的线走、没线落 80%；数旧 30 分钟边界两侧各一条；订阅标识不含原值。

use super::*;
use crate::accounts::quota::rotation::{CapValue, Caps};
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
        sub_id: None,
    }
}

/// 没有线（满了才换）：「快满」落 80%。
fn no_line(_: &str) -> Option<u8> {
    None
}

fn key(w: &str) -> Option<String> {
    crate::agents::claudecode::quota::key_of(w)
}

fn state_of(r: &QuotaReading, kind: Kind, line: &dyn Fn(&str) -> Option<u8>) -> QuotaState {
    show(Some((r, NOW)), facts(kind), line, NOW, &slot).state
}

/// ★ 每一态各一条。
#[test]
fn every_state_comes_out_of_its_own_facts() {
    assert_eq!(
        show(None, facts(Kind::Sub), &no_line, NOW, &slot).state,
        QuotaState::Unseen
    );
    assert_eq!(
        state_of(&sub(0.63, LATER), Kind::Sub, &no_line),
        QuotaState::Ok
    );
    assert_eq!(
        state_of(&sub(0.86, LATER), Kind::Sub, &no_line),
        QuotaState::Near
    );
    let mut refused = sub(1.0, LATER);
    refused.refused = true;
    refused.status = Some(QuotaStatus::Rejected);
    assert_eq!(state_of(&refused, Kind::Sub, &no_line), QuotaState::Refused);
    let mut overage = sub(1.0, LATER);
    overage.status = Some(QuotaStatus::Rejected);
    overage.overage = Some(QuotaOverage {
        status: Some(QuotaStatus::Allowed),
        resets_at: None,
        disabled: None,
        in_use: true,
    });
    assert_eq!(
        state_of(&overage, Kind::Sub, &no_line),
        QuotaState::OverageInUse
    );
    assert_eq!(
        state_of(&sub(0.63, EARLIER), Kind::Sub, &no_line),
        QuotaState::ResetSinceSeen,
        "按钮那个窗口看到之后已重置"
    );
    refused.resets_at = Some(EARLIER);
    assert_eq!(
        state_of(&refused, Kind::Sub, &no_line),
        QuotaState::Near,
        "被拒到的那一刻已过、窗口还没重置 ⇒ 不再画被拒，照窗口的数判"
    );
    let mut over = sub(1.0, EARLIER);
    over.refused = true;
    over.resets_at = Some(EARLIER);
    assert_eq!(
        state_of(&over, Kind::Sub, &no_line),
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
    let s = show(
        Some((&api_refused, NOW)),
        facts(Kind::Api),
        &no_line,
        NOW,
        &slot,
    );
    assert_eq!(
        (s.state, s.limiting, s.slots.len()),
        (QuotaState::Refused, None, 0)
    );
    // 说不出回来时刻的旧账（适配层今天不再出这一形）⇒ 不算被拒，与轮换同一个判法。
    api_refused.resets_at = None;
    assert_eq!(state_of(&api_refused, Kind::Api, &no_line), QuotaState::Ok);
}

/// ★ 「快满」按窗判：那一窗用到这号这一窗此刻的线（与轮换同一处取线）；那一窗没线 ⇒ 80%。回包说越过了预警线也算。
/// 到了线的那一窗另带 `atLine`（悬停卡写「到线」）；没线的窗不出。
#[test]
fn near_is_judged_per_window_against_its_own_line() {
    let r = sub(0.86, LATER);
    assert_eq!(
        state_of(&r, Kind::Sub, &no_line),
        QuotaState::Near,
        "没线 ⇒ 80"
    );
    // 触发只设 7d 95：5h 那一窗没线 ⇒ 落 80 ⇒ 86% 仍快满。
    let mut cap = Caps::new();
    cap.entry("*".into())
        .or_default()
        .insert("7d".into(), CapValue::N(95));
    assert_eq!(
        state_of(&r, Kind::Sub, &lines_of(&cap, "a", NOW, 0, &key, &slot)),
        QuotaState::Near
    );
    // 触发 5h 90：86% 不到线、也不落 80 ⇒ ok。
    cap.entry("*".into())
        .or_default()
        .insert("5h".into(), CapValue::N(90));
    assert_eq!(
        state_of(&r, Kind::Sub, &lines_of(&cap, "a", NOW, 0, &key, &slot)),
        QuotaState::Ok
    );
    // 这号自己 7d ≤40：7d 41% 到线 ⇒ 快满，7d 那一格 atLine，5h 那一格不。
    cap.entry("a".into())
        .or_default()
        .insert("7d".into(), CapValue::N(40));
    let lines = lines_of(&cap, "a", NOW, 0, &key, &slot);
    let s = show(Some((&r, NOW)), facts(Kind::Sub), &lines, NOW, &slot);
    assert_eq!(s.state, QuotaState::Near);
    assert_eq!(
        s.slots
            .iter()
            .map(|x| (x.slot.as_str(), x.at_line))
            .collect::<Vec<_>>(),
        vec![("5h", false), ("7d", true)]
    );
    let mut warned = sub(0.5, LATER);
    warned.status = Some(QuotaStatus::Warning);
    assert_eq!(state_of(&warned, Kind::Sub, &no_line), QuotaState::Near);
}

/// ★ 数旧：看到距今恰好 30 分钟不旧，多一秒就旧；与态叠着出。
#[test]
fn stale_flips_one_second_past_thirty_minutes() {
    let r = sub(0.63, NOW + 7_200);
    let at = |dt: u64| show(Some((&r, NOW)), facts(Kind::Sub), &no_line, NOW + dt, &slot);
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
        &no_line,
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
                slot: "5h".into(),
                pct: Some(63),
                resets_at: Some(LATER),
                full: false,
                at_line: false,
                text: copy_core::copy_text("acct.val.pct", &[("pct", "63")]).into(),
                tone: crate::common::cells::Tone::Plain,
            },
            SlotShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                slot: "7d".into(),
                pct: Some(41),
                resets_at: Some(NOW + 5 * 86_400),
                full: false,
                at_line: false,
                text: copy_core::copy_text("acct.val.pct", &[("pct", "41")]).into(),
                tone: crate::common::cells::Tone::Plain,
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
        show(Some((&r, NOW)), facts(Kind::Sub), &no_line, NOW, &slot).slots[0].clone()
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

/// ★ 同一订阅：同一身份 ⇒ 同一标识，不同身份 ⇒ 不同；标识里零处出现原值。
#[test]
fn the_subscription_id_is_stable_and_never_the_identity_itself() {
    let a = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
    let b = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
    assert_eq!(sub_id_of(a), sub_id_of(a));
    assert_ne!(sub_id_of(a), sub_id_of(b));
    for id in [a, b] {
        let got = sub_id_of(id);
        assert_eq!(got.len(), 24);
        for part in id.split('-') {
            assert!(!got.contains(part), "{got} 含 {part}");
        }
    }
}

/// ★ 一格的字与语气只住 `slot_words`：卡人的那一格按显示态换字；用满不管是不是卡人的那一格都 `✕`。
#[test]
fn a_slot_cell_is_written_once_by_state() {
    use crate::common::cells::Tone;
    let w = |state, here, pct, full| {
        let (t, tone) = slot_words(state, here, pct, full);
        (t.0, tone)
    };
    let c = |k: &str, a: &[(&str, &str)]| copy_core::copy_text(k, a);
    assert_eq!(
        w(QuotaState::OverageInUse, true, Some(40), false),
        (c("acct.val.over", &[]), Tone::Warn)
    );
    assert_eq!(
        w(QuotaState::ResetSinceSeen, true, Some(40), false),
        (c("acct.val.none", &[]), Tone::Plain)
    );
    assert_eq!(
        w(QuotaState::Ok, false, Some(100), true),
        (c("acct.val.full", &[]), Tone::Fail)
    );
    assert_eq!(
        w(QuotaState::Refused, true, Some(58), false),
        (c("acct.val.refusedPct", &[("pct", "58")]), Tone::Fail)
    );
    assert_eq!(
        w(QuotaState::Refused, true, None, false),
        (c("acct.val.refusedOnly", &[]), Tone::Fail)
    );
    assert_eq!(
        w(QuotaState::Refused, false, Some(10), false),
        (c("acct.val.pct", &[("pct", "10")]), Tone::Plain)
    );
    assert_eq!(
        w(QuotaState::Near, true, Some(85), false),
        (c("acct.val.pct", &[("pct", "85")]), Tone::Warn)
    );
    assert_eq!(
        w(QuotaState::Ok, true, None, false),
        (c("acct.val.none", &[]), Tone::Plain)
    );
}

/// ★★ 用量那一格（状态栏按钮 · 切号下拉 · 「下一个」）由核心写：窗口 · 值 · 重置 · 连成的一句 · 语气，出口照抄。
/// 回包没说卡在哪一窗 ⇒ 画 `5h`（这一判也只在这里）。
#[test]
fn the_usage_cell_is_written_by_the_core() {
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let clock = crate::common::time::TextClock {
        now: NOW as i64,
        tz_min: 0,
    };
    let shown = |r: Option<&QuotaReading>, kind: Kind| {
        let mut q = show(r.map(|r| (r, NOW)), facts(kind), &no_line, NOW, &slot);
        q.stamp(&clock);
        q.usage
    };
    let five = crate::accounts::quota::name_words::slot_text("5h");
    // 订阅 · 正常：`5h 50%`，常规色。
    let u = shown(Some(&sub(0.5, LATER)), Kind::Sub);
    assert_eq!(
        (
            u.slot.as_deref(),
            u.window.as_ref().map(|w| w.0.as_str()),
            u.reset.is_none(),
            u.tone
        ),
        (Some("5h"), Some(five.as_str()), true, Tone::Plain)
    );
    assert_eq!(
        u.text.0,
        copy_core::copy_text("acct.usage.wv", &[("w", &five), ("v", &u.value.0)])
    );
    // 快满 ⇒ 警示色。
    assert_eq!(shown(Some(&sub(0.9, LATER)), Kind::Sub).tone, Tone::Warn);
    // 用满 ⇒ 失败色。
    assert_eq!(shown(Some(&sub(1.0, LATER)), Kind::Sub).tone, Tone::Fail);
    // 被拒 ⇒ 失败色 ＋ 几点重置（照卡着那一窗的时刻），连成 `5h ✕ ↻hh:mm`。
    let mut refused = sub(1.0, LATER);
    refused.refused = true;
    let u = shown(Some(&refused), Kind::Sub);
    let reset = copy_core::copy_text("acct.reset.at", &[("at", &clock.text(LATER).0)]);
    assert_eq!(
        (u.tone, u.reset.as_ref().map(|r| r.0.clone())),
        (Tone::Fail, Some(reset.clone()))
    );
    assert_eq!(
        u.text.0,
        copy_core::copy_text(
            "acct.usage.wvr",
            &[("w", &five), ("v", &u.value.0), ("r", &reset)]
        )
    );
    // 超额在兜 ⇒ 警示色、不写重置。
    let mut over = sub(1.0, LATER);
    over.overage = Some(QuotaOverage {
        status: Some(QuotaStatus::Allowed),
        resets_at: None,
        disabled: None,
        in_use: true,
    });
    let u = shown(Some(&over), Kind::Sub);
    assert_eq!((u.tone, u.reset.is_none()), (Tone::Warn, true), "{u:?}");
    // 回包没说卡在哪一窗 ⇒ 画 5h。
    let mut free = sub(0.3, LATER);
    free.limiting = None;
    free.windows.retain(|w| w.name == "five_hour");
    assert_eq!(shown(Some(&free), Kind::Sub).slot.as_deref(), Some("5h"));
    // 没出过数 ⇒ `5h —`，常规色。
    let u = shown(None, Kind::Sub);
    assert_eq!(
        (u.value.0.as_str(), u.tone),
        (t("acct.val.none").as_str(), Tone::Plain)
    );
    // 按量 · 能发 ⇒ `按量`、没有窗口；按量 · 被拒 ⇒ `被拒 ↻hh:mm`、失败色。
    let u = shown(Some(&sub(0.0, LATER)), Kind::Api);
    assert_eq!(
        (u.window.is_none(), u.text.0.as_str(), u.tone),
        (true, t("acct.kind.api").as_str(), Tone::Plain)
    );
    let mut api = sub(0.0, LATER);
    api.refused = true;
    let u = shown(Some(&api), Kind::Api);
    assert_eq!(
        (u.value.0.as_str(), u.tone),
        (t("acct.val.refusedOnly").as_str(), Tone::Fail)
    );
    assert_eq!(
        u.text.0,
        copy_core::copy_text("acct.usage.wv", &[("w", &u.value.0), ("v", &reset)])
    );
}
