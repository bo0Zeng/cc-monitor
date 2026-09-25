//! 〔US1〕中转门牌（`relay-route-core`）的判据：拼与拆互逆 · 段闸 · 两个前缀闭集。
//! 守的要求：`设计/20 §5`「端口 · 路由渲染 · 段的字符闸收进共享 crate，一份实现两侧 use」·
//! `§3.3`「值 fail-closed 校验：必须是构造口产得出的形状」。

use super::*;

/// 拼出来的每一形都被 [`base_url_shape_ok`] 收、被 [`parse_target`] 还原成同样的四格（两个前缀 × 若干段）。
#[test]
fn what_base_url_builds_the_inverse_accepts_and_parse_restores() {
    let segs = [
        ("claude-code", "acct-a", "k-0123456789abcdef"),
        ("claude-code", "0", "3f2a9c1e-7d44-4c3b-9a55-0e6b2f1d8c77"),
        ("a", "_", "Z"),
    ];
    for m in RouteMode::ALL {
        for (a, b, c) in segs {
            let url = base_url(PORT, m, a, b, c).expect("合法段拼不出来");
            assert_eq!(url, format!("http://127.0.0.1:8788{}{a}/{b}/{c}", m.prefix()));
            assert!(base_url_shape_ok(&url), "构造口的产物被逆拒了：{url}");
            let path = route_path(m, a, b, c).unwrap();
            let target = format!("{path}/v1/messages?beta=true");
            assert_eq!(
                parse_target(&target),
                Some(Parsed { mode: m, seg1: a, seg2: b, seg3: c, rest: "v1/messages?beta=true" })
            );
        }
    }
}

/// 坏形一律拒（逆那一侧）；过不了闸的段一律拼不出来（构造那一侧）。
#[test]
fn bad_shapes_and_bad_segments_are_refused_on_both_sides() {
    for bad in [
        "http://localhost:8788/s/a/b/c",
        "https://127.0.0.1:8788/s/a/b/c",
        "http://127.0.0.1:8788/s/a/b/c/",
        "http://127.0.0.1:8788/s/a/b",
        "http://127.0.0.1:8788/s/a/b/c/d",
        "http://127.0.0.1:8788/x/a/b/c",
        "http://127.0.0.1:0/s/a/b/c",
        "http://127.0.0.1:70000/s/a/b/c",
        "http://127.0.0.1:/s/a/b/c",
        "http://127.0.0.1:8788/s/a/b/c?x=1",
        "http://127.0.0.1:8788/s/a/../c",
        "http://127.0.0.1:8788",
    ] {
        assert!(!base_url_shape_ok(bad), "坏形被收了：{bad}");
    }
    for seg in ["", "a.b", "a/b", "..", "a b", "é", &"x".repeat(129)] {
        assert!(!segment_is_safe(seg), "{seg:?} 过了闸");
        for m in RouteMode::ALL {
            assert_eq!(route_path(m, seg, "b", "c"), None);
            assert_eq!(route_path(m, "a", seg, "c"), None);
            assert_eq!(route_path(m, "a", "b", seg), None);
        }
    }
    assert!(segment_is_safe(&"x".repeat(128)), "正好 128 字节该过（正控）");
    assert_eq!(base_url(0, RouteMode::Substitute, "a", "b", "c"), None);
    for bad in ["/s/a/b", "/q/a/b/c/x", "/s/a/b/c", "/s/a/b./c/x", "s/a/b/c/x"] {
        assert_eq!(parse_target(bad), None, "{bad} 被切出来了");
    }
}

/// 两个前缀两两不同、各自以 `/` 起止；闭集就这两个。
#[test]
fn the_two_prefixes_are_a_closed_distinct_set() {
    let p: Vec<&str> = RouteMode::ALL.iter().map(|m| m.prefix()).collect();
    assert_eq!(p, vec!["/s/", "/t/"]);
    assert_ne!(RouteMode::ALL[0], RouteMode::ALL[1]);
}
