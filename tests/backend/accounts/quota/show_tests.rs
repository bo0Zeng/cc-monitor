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
        sub_id: None,
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
                slot: "5h".into(),
                pct: Some(63),
                resets_at: Some(LATER),
                full: false,
                text: copy_core::copy_text("acct.val.pct", &[("pct", "63")]).into(),
                tone: crate::common::cells::Tone::Plain,
                per_pct_text: None,
                window: "five_hour".into(),
            },
            SlotShow {
                resets_at_text: None,
                resets_at_rel_text: None,
                slot: "7d".into(),
                pct: Some(41),
                resets_at: Some(NOW + 5 * 86_400),
                full: false,
                text: copy_core::copy_text("acct.val.pct", &[("pct", "41")]).into(),
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
    let clock = crate::common::time::TextClock::new(NOW, &crate::Tz::default());
    let shown = |r: Option<&QuotaReading>, kind: Kind| {
        let mut q = show(r.map(|r| (r, NOW)), facts(kind), NOW, &slot);
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
            ("five_hour".to_string(), PerPct { tokens: 2_100_000, at: NOW }),
            ("seven_day_opus".to_string(), PerPct { tokens: 25_000, at: NOW }),
        ]
        .into(),
    }
}

/// ★ 「1% ≈ 多少」写好的字：每个量过的窗口一格（短写的数）；语义位那一格照它取的那个窗口（7d 取用得多的 opus），没量过 ⇒ 缺。
#[test]
fn per_pct_is_written_for_windows_and_the_slot_that_took_them() {
    let o = observed();
    let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
    let mut sh = show(Some((&o.reading, o.seen_at)), facts(Kind::Sub), NOW, &slot);
    sh.windows = windows_of(&o, &key, NOW);
    per_pct_of_slots(&mut sh);
    let pp = |n: &str| Some(Words(copy_core::copy_text("acct.val.perPct", &[("n", n)])));
    let win_pp: Vec<_> = sh.windows.iter().map(|w| (w.name.as_str(), w.per_pct_text.clone())).collect();
    assert_eq!(
        win_pp,
        vec![("five_hour", pp("2.1M")), ("seven_day", None), ("seven_day_opus", pp("25K"))]
    );
    let slot_pp: Vec<_> = sh.slots.iter().map(|s| (s.slot.as_str(), s.per_pct_text.clone())).collect();
    assert_eq!(slot_pp, vec![("5h", pp("2.1M")), ("7d", pp("25K"))]);
}

/// ★ 更细窗口那一行写好的字与语气：名字照窗口键（`7d:opus` ⇒「7d opus」、超额那一档 ⇒「超额」，原名不上屏）· 值 · 还在计时 ⇒ ↻时刻；
/// 快满 ⇒ warn、用满 ⇒ fail；重置已过 ⇒ 值写「—」、不写重置。
#[test]
fn each_window_line_is_written_with_its_tone() {
    let mut o = observed();
    o.reading.windows.push(win("overage", 1.0, LATER));
    o.reading.windows.push(win("seven_day_sonnet", 0.5, EARLIER));
    let key = |w: &str| crate::agents::claudecode::quota::key_of(w);
    let clock = crate::common::time::TextClock::new(NOW, &crate::Tz::default());
    let mut ws = windows_of(&o, &key, NOW);
    ws.iter_mut().for_each(|w| w.stamp(&clock));
    let line = |window: &str, value: &str, at: Option<u64>| match at {
        Some(t) => copy_core::copy_text(
            "acct.window.lineReset",
            &[("window", window), ("value", value), ("at", &clock.text(t).0)],
        ),
        None => copy_core::copy_text("acct.window.line", &[("window", window), ("value", value)]),
    };
    let opus = copy_core::copy_text("acct.window.tier", &[("slot", "7d"), ("tier", "opus")]);
    let sonnet = copy_core::copy_text("acct.window.tier", &[("slot", "7d"), ("tier", "sonnet")]);
    let got: Vec<_> = ws.iter().map(|w| (w.text.0.clone(), w.tone)).collect();
    assert_eq!(
        got,
        vec![
            (line("5h", "30%", Some(LATER)), Tone::Plain),
            (line("7d", "40%", Some(LATER + 86_400)), Tone::Plain),
            (line(&opus, "85%", Some(LATER + 86_400)), Tone::Warn),
            (
                line(
                    &copy_core::copy_text("acct.row.over", &[]),
                    &copy_core::copy_text("acct.val.full", &[]),
                    Some(LATER)
                ),
                Tone::Fail
            ),
            (line(&sonnet, "—", None), Tone::Plain),
        ]
    );
    assert!(
        ws.iter().all(|w| !w.text.0.contains(&w.name)
            && w.key.as_deref().is_none_or(|k| !k.contains(':') || !w.text.0.contains(k))),
        "原名或窗口键上了屏：{got:?}"
    );
}
