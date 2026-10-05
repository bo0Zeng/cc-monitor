//! 自动起算的判定：开着且没窗 ⇒ 当场发；有窗 ⇒ 醒在重置那一刻、不早不晚；时段外不发、醒在时段起点；
//! 一个空闲期只试一次；关着 ⇒ 不醒。假钟（本地钟由判据给），不起进程。

use super::*;

const UTC: &dyn Fn(u64) -> i64 = &|_| 0;
/// 2026-10-05 00:00:00 UTC。
const DAY0: u64 = 1_791_158_400;

fn at(h: u64, m: u64) -> u64 {
    DAY0 + h * 3600 + m * 60
}

fn on(window: Option<(&str, &str)>) -> AutoConf {
    AutoConf {
        enabled: true,
        window: window.map(|(f, t)| DayWindow {
            from: f.into(),
            to: t.into(),
        }),
        ..AutoConf::default()
    }
}

fn reset_at(t: u64) -> Option<WindowSeen> {
    Some(WindowSeen {
        five_hour_reset: Some(t),
        refused_until: None,
    })
}

#[test]
fn enabled_and_never_seen_sends_at_once() {
    let p = plan(&on(None), None, None, at(9, 0), UTC);
    assert!(p.due);
    assert_eq!(p.next, Some(AutostartNext::Now));
    assert_eq!(p.wake, Some(at(9, 0)));
}

#[test]
fn a_running_window_wakes_exactly_at_its_reset() {
    let r = at(14, 10);
    let before = plan(&on(None), reset_at(r), None, r - 1, UTC);
    assert!(!before.due);
    assert_eq!(before.wake, Some(r));
    assert_eq!(before.next, Some(AutostartNext::At(r)));
    let then = plan(&on(None), reset_at(r), None, r, UTC);
    assert!(then.due, "到重置那一刻就该发");
    assert_eq!(then.episode, r);
}

#[test]
fn one_idle_stretch_is_tried_once() {
    let r = at(14, 10);
    let p = plan(&on(None), reset_at(r), Some(r), r + 60, UTC);
    assert!(!p.due);
    assert_eq!((p.wake, p.next), (None, None), "试过没成 ⇒ 不醒、不打转");
    // 额度账出了新的重置时刻（别处用了这个号）⇒ 新的一段，照常醒。
    let later = plan(&on(None), reset_at(r + 5 * 3600), Some(r), r + 120, UTC);
    assert_eq!(later.wake, Some(r + 5 * 3600));
}

#[test]
fn off_never_wakes() {
    for seen in [None, reset_at(at(14, 10)), reset_at(at(1, 0))] {
        let p = plan(&AutoConf::default(), seen, None, at(9, 0), UTC);
        assert_eq!((p.due, p.wake, p.next), (false, None, None));
    }
}

#[test]
fn outside_the_window_waits_for_its_start() {
    let w = Some(("07:00", "24:00"));
    let p = plan(&on(w), None, None, at(5, 0), UTC);
    assert!(!p.due);
    assert_eq!(p.wake, Some(at(7, 0)));
    assert_eq!(p.next, Some(AutostartNext::OutsideWindow { at: at(7, 0) }));
    assert!(plan(&on(w), None, None, at(7, 0), UTC).due);
    assert!(plan(&on(w), None, None, at(23, 59), UTC).due);
}

#[test]
fn a_window_across_midnight() {
    let w = Some(("22:00", "06:00"));
    assert!(plan(&on(w), None, None, at(23, 0), UTC).due);
    assert!(plan(&on(w), None, None, at(5, 59), UTC).due);
    assert_eq!(
        plan(&on(w), None, None, at(12, 0), UTC).wake,
        Some(at(22, 0))
    );
}

#[test]
fn a_reset_outside_the_window_wakes_at_the_next_start() {
    let w = Some(("07:00", "24:00"));
    let p = plan(&on(w), reset_at(at(2, 10)), None, at(1, 0), UTC);
    assert_eq!(p.wake, Some(at(7, 0)));
    assert_eq!(p.next, Some(AutostartNext::OutsideWindow { at: at(7, 0) }));
}

#[test]
fn the_window_follows_the_local_clock() {
    // 本地 = UTC+8：本地 07:00 是 UTC 前一天 23:00。
    let cst: &dyn Fn(u64) -> i64 = &|_| 8 * 3600;
    let w = Some(("07:00", "24:00"));
    let p = plan(&on(w), None, None, at(20, 0), cst); // 本地 04:00
    assert_eq!(p.wake, Some(at(23, 0)));
    assert!(plan(&on(w), None, None, at(23, 0), cst).due);
}

#[test]
fn refused_by_another_window_waits_for_that_reset() {
    let seen = Some(WindowSeen {
        five_hour_reset: Some(at(8, 0)),
        refused_until: Some(at(30, 0)),
    });
    let p = plan(&on(None), seen, None, at(9, 0), UTC);
    assert!(!p.due);
    assert_eq!(p.wake, Some(at(30, 0)));
}

#[test]
fn window_shapes() {
    let w = |f: &str, t: &str| window_from(&serde_json::json!({"from": f, "to": t}));
    assert_eq!(window_from(&Value::Null), Ok(None));
    assert!(w("07:00", "24:00").is_ok());
    assert!(w("22:00", "06:00").is_ok());
    for (f, t) in [
        ("07:00", "07:00"),
        ("00:00", "24:00"),
        ("24:00", "07:00"),
        ("7:00", "08:00"),
        ("07:60", "08:00"),
        ("25:00", "08:00"),
    ] {
        assert!(w(f, t).is_err(), "{f}–{t} 应拒");
    }
    assert!(window_from(&serde_json::json!({"from": "07:00", "to": "08:00", "x": 1})).is_err());
}
