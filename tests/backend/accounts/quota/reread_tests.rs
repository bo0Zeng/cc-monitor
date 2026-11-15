//! 到点重读：过了重置时刻（再过余量）只问一次 · 没到不问 · 没成的记账、之后不再问 · 查不了的那一家与 `_` 不问。
use super::*;
use crate::agents::{QuotaReading, QuotaStatus, QuotaWindow};

const NOW: u64 = 1_800_000_000;
const M: u64 = crate::accounts::quota::RESET_MARGIN_S;

fn win(name: &str, resets_at: u64) -> QuotaWindow {
    QuotaWindow {
        name: name.into(),
        used: Some(0.9),
        resets_at: Some(resets_at),
        warned_at: None,
    }
}

fn entry(account: &str, windows: Vec<QuotaWindow>) -> Observed {
    Observed {
        agent: "claude".into(),
        account: account.into(),
        seen_at: NOW - 3_600,
        reading: QuotaReading {
            status: Some(QuotaStatus::Allowed),
            refused: false,
            limiting: None,
            resets_at: None,
            windows,
            overage: None,
        },
        windows_seen: Default::default(),
        trend: Default::default(),
        per_pct: Default::default(),
        history: Default::default(),
        reread_failed: Default::default(),
    }
}

/// 跑一趟：交回（问了谁 · 记了哪几笔没成 · 下一次几点醒）。
fn run(
    entries: &[Observed],
    now: u64,
    can: bool,
    ok: bool,
    asked: &mut Asked,
) -> (Vec<String>, Vec<Due>, Option<u64>) {
    let mut probed = Vec::new();
    let mut failed = Vec::new();
    let next = tick(
        entries,
        now,
        &|_, _| can,
        asked,
        &mut |d: &Due| {
            probed.push(d.account.clone());
            ok
        },
        &mut |d: &Due| failed.push(d.clone()),
    );
    (probed, failed, next)
}

#[test]
fn a_lapsed_window_is_asked_once() {
    let e = vec![entry(
        "a",
        vec![win("five_hour", NOW - M), win("seven_day", NOW + 86_400)],
    )];
    let mut asked = Asked::new();
    let (probed, failed, next) = run(&e, NOW, true, true, &mut asked);
    assert_eq!(probed, vec!["a".to_string()]);
    assert!(failed.is_empty());
    assert_eq!(
        next,
        Some(NOW + 86_400 + M),
        "下一次醒在下一个到点的时刻（再过余量）"
    );
    let (again, stuck, _) = run(&e, NOW + 30, true, true, &mut asked);
    assert!(again.is_empty(), "同一窗同一刻只问一次");
    assert_eq!(
        stuck.iter().map(|d| d.lapsed.clone()).collect::<Vec<_>>(),
        vec![[("five_hour".to_string(), NOW - M)].into()],
        "问过了、账上仍是过了的那一刻 ⇒ 记没成"
    );
}

#[test]
fn nothing_is_asked_before_the_reset_and_its_margin() {
    let e = vec![entry("a", vec![win("five_hour", NOW + 600)])];
    let (probed, _, next) = run(&e, NOW, true, true, &mut Asked::new());
    assert!(probed.is_empty());
    assert_eq!(next, Some(NOW + 600 + M));
    // 刚过重置、还没过余量（上游可能还没换窗）⇒ 不问，到余量那一刻再醒。
    let just = vec![entry("a", vec![win("five_hour", NOW - 1)])];
    let (probed, _, next) = run(&just, NOW, true, true, &mut Asked::new());
    assert!(probed.is_empty());
    assert_eq!(next, Some(NOW - 1 + M));
}

#[test]
fn a_failed_reread_is_recorded_and_not_asked_again() {
    let e = vec![entry("a", vec![win("five_hour", NOW - M)])];
    let (probed, failed, _) = run(&e, NOW, true, false, &mut Asked::new());
    assert_eq!(probed, vec!["a".to_string()]);
    assert_eq!(
        failed,
        vec![Due {
            agent: "claude".into(),
            account: "a".into(),
            lapsed: [("five_hour".to_string(), NOW - M)].into(),
        }]
    );
    // 重启之后（进程里问过的没了）照账上那一记不再问。
    let mut marked = e.clone();
    marked[0].reread_failed.insert("five_hour".into(), NOW - M);
    let (again, _, _) = run(&marked, NOW + 30, true, true, &mut Asked::new());
    assert!(again.is_empty());
}

#[test]
fn a_family_without_usage_is_never_asked() {
    let e = vec![entry("a", vec![win("five_hour", NOW - M)])];
    let (probed, _, next) = run(&e, NOW, false, true, &mut Asked::new());
    assert!(probed.is_empty());
    assert_eq!(next, None);
}

/// 「会不会到点重读」只判一处：查得了用量的那一家、号库里的号才算；`_` 不顶任何一份登录，不算。
#[test]
fn only_a_named_account_of_a_family_with_usage_is_reread() {
    assert!(rereadable("claude-code", "a"));
    assert!(!rereadable("claude-code", "_"));
    assert!(!rereadable("no-such-family", "a"));
}
