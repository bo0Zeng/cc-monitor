//! 桌面每台一张名单台：同一台拿到的是同一张、名单从空开始 · 本机两条载体（常驻套接字 · stdio）与远端都经它接上。
use super::*;

#[test]
fn one_desk_per_machine_and_it_starts_empty() {
    let a = desk("o-watch-test");
    let b = desk("o-watch-test");
    assert!(Arc::ptr_eq(&a, &b), "同一台两张名单台");
    assert!(a.sids().is_empty(), "冷起名单从空开始（不是全看）");
    assert!(!Arc::ptr_eq(&a, &desk("o-watch-test-2")), "两台共用了一张");
}

/// 三处连接都经名单台：本机常驻（attach 行带它的名单 · 接上交它）· 本机 stdio（没有 attach 行 ⇒ 接上交它、它当场报）· 远端（交给 `Feed`）。
/// 哪一处改回自己报（或漏接），这一条红；报名单本身只在通信层那一处（`resident-client` 的 `watch.rs`）。
#[test]
fn every_carrier_goes_through_the_desk() {
    let read = |src: &str| guard_core::production_code(src);
    let host = read(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    for pin in [
        "crate::stream_watch::desk(crate::inbound_client::LOCAL_ORIGIN).ask_for_attach();",
        "crate::stream_watch::desk(crate::inbound_client::LOCAL_ORIGIN).up(&client, Some(carried));",
    ] {
        guard_core::find_pinned(&host, pin).unwrap_or_else(|e| panic!("本机常驻：{e}"));
    }
    let stdio = read(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    guard_core::find_pinned(
        &stdio,
        "crate::stream_watch::desk(route.origin.as_wire_str()).up(&client, None);",
    )
    .unwrap_or_else(|e| panic!("本机 stdio：{e}"));
    let remote = read(include_str!(
        "../../../src/frontend/shell/src/stream_source/run.rs"
    ));
    guard_core::find_pinned(&remote, "crate::stream_watch::desk(&origin),")
        .unwrap_or_else(|e| panic!("远端：{e}"));
    let me = read(include_str!(
        "../../../src/frontend/shell/src/stream_watch.rs"
    ));
    for own in [".call(", "link_changes", "stream-watch\""] {
        assert!(!me.contains(own), "壳又自己报名单（`{own}`）");
    }
}
