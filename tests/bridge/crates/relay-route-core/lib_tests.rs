//! 〔US1〕中转门牌（`relay-route-core`）的判据：拼与拆互逆 · 段闸 · 两个前缀闭集。
//! 守的要求：`设计/20 §5`「端口 · 路由渲染 · 段的字符闸收进共享 crate，一份实现两侧 use」·
//! `§3.3`「值 fail-closed 校验：必须是构造口产得出的形状」。

use super::*;

/// 拼出来的每一形都被 [`base_url_shape_ok`] 收、被 [`parse_target`] 还原成同样的三格（两个前缀 × 若干段）。
#[test]
fn what_base_url_builds_the_inverse_accepts_and_parse_restores() {
    let segs = [("claude-code", "acct-a"), ("claude-code", "0"), ("a", "_")];
    for m in RouteMode::ALL {
        for (a, b) in segs {
            let url = base_url(PORT, m, a, b).expect("合法段拼不出来");
            assert_eq!(url, format!("http://127.0.0.1:8788{}{a}/{b}", m.prefix()));
            assert!(base_url_shape_ok(&url), "构造口的产物被逆拒了：{url}");
            let path = route_path(m, a, b).unwrap();
            let target = format!("{path}/v1/messages?beta=true");
            assert_eq!(
                parse_target(&target),
                Some(Parsed {
                    mode: m,
                    seg1: a,
                    seg2: b,
                    rest: "v1/messages?beta=true"
                })
            );
        }
    }
}

/// 〔V141 · R2〕注入的地址**不随会话变**：恰好两段，退役的第 3 段（先前装 sid / 启动器铸的 nonce）那一形逆一侧拒。
/// 守的要求：用户裁决 V141「启动器不造会话 id、不往环境变量 / 中转地址里塞任何会话身份」。
#[test]
fn the_injected_address_carries_no_session_segment() {
    let two = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    assert!(base_url_shape_ok(&two), "正控：两段那一形该收");
    assert!(
        !base_url_shape_ok(&format!("{two}/3f2a9c1e-7d44-4c3b-9a55-0e6b2f1d8c77")),
        "带会话段的老三段形被当成注入地址收了"
    );
}

/// 坏形一律拒（逆那一侧）；过不了闸的段一律拼不出来（构造那一侧）。
#[test]
fn bad_shapes_and_bad_segments_are_refused_on_both_sides() {
    for bad in [
        "http://localhost:8788/s/a/b",
        "https://127.0.0.1:8788/s/a/b",
        "http://127.0.0.1:8788/s/a/b/",
        "http://127.0.0.1:8788/s/a",
        "http://127.0.0.1:8788/s/a/b/c",
        "http://127.0.0.1:8788/x/a/b",
        "http://127.0.0.1:0/s/a/b",
        "http://127.0.0.1:70000/s/a/b",
        "http://127.0.0.1:/s/a/b",
        "http://127.0.0.1:8788/s/a/b?x=1",
        "http://127.0.0.1:8788/s/a/..",
        "http://127.0.0.1:8788",
    ] {
        assert!(!base_url_shape_ok(bad), "坏形被收了：{bad}");
    }
    for seg in ["", "a.b", "a/b", "..", "a b", "é", &"x".repeat(129)] {
        assert!(!segment_is_safe(seg), "{seg:?} 过了闸");
        for m in RouteMode::ALL {
            assert_eq!(route_path(m, seg, "b"), None);
            assert_eq!(route_path(m, "a", seg), None);
        }
    }
    assert!(
        segment_is_safe(&"x".repeat(128)),
        "正好 128 字节该过（正控）"
    );
    assert_eq!(base_url(0, RouteMode::Substitute, "a", "b"), None);
    for bad in ["/s/a", "/q/a/b/x", "/s/a/b", "/s/a/b./x", "s/a/b/x"] {
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

/// 展开过钥匙的地址：切得开的恰是「构造口产物 ＋ 一段形状对的钥匙」；别的都切不开。
#[test]
fn a_keyed_url_splits_only_when_the_key_and_the_route_both_have_our_shape() {
    let key = "0123456789abcdef".repeat(4);
    let url = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    let (head, tail) = url.split_at("http://127.0.0.1:8788/".len());
    let keyed = format!("{head}{key}/{tail}");
    assert_eq!(
        split_keyed_base_url(&keyed),
        Some((head, &format!("/{tail}")[..]))
    );
    assert_eq!(
        format!("{head}{key}/{tail}"),
        keyed,
        "两半拼回去（插回钥匙）== 原串"
    );
    for bad in [
        url.clone(),                                    // 没有钥匙段
        format!("{head}{}/{tail}", &key[..63]),         // 短一个字符
        format!("{head}{}/{tail}", key.to_uppercase()), // 大写
        format!("{head}{key}/x/a/b"),                   // 前缀不认得
        format!("{head}{key}/{tail}/k1"),               // 退役的会话段（V141）
        format!("http://localhost:8788/{key}/{tail}"),  // 非字面回环
        format!("https://127.0.0.1:8788/{key}/{tail}"),
        "https://api.example.com/v1".to_string(), // 用户自己的端点
    ] {
        assert_eq!(split_keyed_base_url(&bad), None, "{bad}");
    }
    assert!(key_shape_ok(&key) && !key_shape_ok(&key[..63]) && !key_shape_ok(&key.to_uppercase()));
}
