//! 退避的数（两端同一份）：首次只等下界 · 翻倍封顶 · 活过 30 s 才清零（只握上手不算）。
use super::*;

#[test]
fn the_backoff_climbs_from_the_floor_and_saturates_at_the_cap() {
    let mut b = RECONNECT_MIN;
    let mut seen = vec![b];
    for _ in 0..6 {
        b = next_backoff(b);
        seen.push(b);
    }
    let secs: Vec<u64> = seen.iter().map(Duration::as_secs).collect();
    assert_eq!(secs, vec![2, 4, 8, 16, 30, 30, 30]);
}

#[test]
fn only_a_link_that_lived_long_enough_resets_the_backoff() {
    assert_eq!(MIN_HEALTHY_UPTIME, Duration::from_secs(30));
    assert!(!should_reset_backoff(true, Duration::from_millis(900)));
    assert!(!should_reset_backoff(false, Duration::from_secs(300)));
    assert!(should_reset_backoff(true, MIN_HEALTHY_UPTIME));
}

/// 对端发完 hello 就死（每一轮都握上手、都活不过 30 s）：退避照样翻倍，不停在下界（F05 / I-1 那个自激循环）。
/// 站住过的那一轮之后从下界来；［重新连接］也从下界来。
#[test]
fn a_backend_that_dies_after_hello_every_round_still_backs_off() {
    let mut b = Backoff::new();
    let mut waits = Vec::new();
    for _ in 0..5 {
        let healthy = should_reset_backoff(true, Duration::from_millis(500));
        waits.push(b.before_wait(healthy).as_secs());
        b.after_wait(false, healthy);
    }
    assert_eq!(waits, vec![2, 4, 8, 16, 30]);
    let healthy = should_reset_backoff(true, MIN_HEALTHY_UPTIME);
    assert_eq!(b.before_wait(healthy), RECONNECT_MIN);
    b.after_wait(false, healthy);
    assert_eq!(
        b.before_wait(false),
        RECONNECT_MIN,
        "站住过的那一轮之后，下一次断先等下界"
    );
    b.after_wait(false, false);
    assert_eq!(b.before_wait(false).as_secs(), 4);
    b.after_wait(true, false);
    assert_eq!(b.before_wait(false), RECONNECT_MIN, "叫醒过 ⇒ 从下界来");
}
