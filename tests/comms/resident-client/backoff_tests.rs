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

/// 两端同一份阈值：退避的数只在本文件定义一次；用它的循环全仓只一条（`feed.rs`，monitor `one_loop_guard` 守着），
/// 手机（嵌的 Rust 核心 `src/mobile/core-rs`）与桌面（monitor `stream_source/run.rs`）都经本 crate 的 `Feed` 跑；Kotlin 那一侧一个退避的数都没有。
#[test]
fn both_ends_take_the_backoff_from_here() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let read = |rel: &str| {
        std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("读不了 {rel}：{e}"))
    };
    let here = guard_core::production_code(&read("src/comms/resident-client/backoff.rs"));
    for name in ["RECONNECT_MIN", "RECONNECT_MAX", "MIN_HEALTHY_UPTIME"] {
        assert_eq!(
            here.matches(&format!("pub const {name}: Duration")).count(),
            1,
            "{name} 在本 crate 里不是恰好定义一次"
        );
    }
    for (who, rel) in [
        ("桌面", "src/frontend/shell/src/stream_source/run.rs"),
        ("手机核心", "src/mobile/core-rs/rust/resident.rs"),
    ] {
        let src = guard_core::production_code(&read(rel));
        assert!(
            src.contains("resident_client::feed::") && src.contains("Feed::start("),
            "{who}不再经本 crate 的 `Feed` 跑重连循环（{rel}）"
        );
    }
    // Kotlin 那一侧：从前那一份（`BackendFeed.Backoff` · `HEALTHY_MS`）的名字一个都不许回来；逐文件扫在 `LinkLayerLivesInTheCoreScanTest`，这里只看落地那一份。
    let kt = read("src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/ResidentFeed.kt");
    for name in ["class Backoff", "HEALTHY_MS", "MIN_HEALTHY"] {
        assert!(
            !kt.contains(name),
            "Kotlin 落地那一份里又有了一份退避（`{name}`）"
        );
    }
}
