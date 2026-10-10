//! 中转门牌（`relay-route-core`）的判据：拼与拆互逆 · 段闸 · 两个前缀闭集。
//! 守的要求：「端口 · 路由渲染 · 段的字符闸收进共享 crate，一份实现两侧 use」·
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
                    origin: None,
                    rest: "v1/messages?beta=true"
                })
            );
        }
    }
}

/// 注入的地址**不随会话变**：恰好两段，退役的第 3 段（先前装 sid / 启动器铸的 nonce）那一形逆一侧拒。
/// 守的要求：「启动器不造会话 id、不往环境变量 / 中转地址里塞任何会话身份」。
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
        format!("{head}{key}/{tail}/k1"),               // 退役的会话段
        format!("http://localhost:8788/{key}/{tail}"),  // 非字面回环
        format!("https://127.0.0.1:8788/{key}/{tail}"),
        "https://api.example.com/v1".to_string(), // 用户自己的端点
    ] {
        assert_eq!(split_keyed_base_url(&bad), None, "{bad}");
    }
    assert!(key_shape_ok(&key) && !key_shape_ok(&key[..63]) && !key_shape_ok(&key.to_uppercase()));
}

/// 插钥匙是切钥匙的逆：插出来的切得回原来两半、钥匙段恰是那一把；地址或钥匙形状不对 ⇒ 不插。
#[test]
fn keying_a_url_is_the_inverse_of_splitting_it() {
    let key = "0123456789abcdef".repeat(4);
    for (m, a, b) in [
        (RouteMode::Passthrough, "claude-code", "_"),
        (RouteMode::Substitute, "claude-code", "work"),
    ] {
        let url = base_url(PORT, m, a, b).unwrap();
        let keyed = keyed_base_url(&url, &key).expect("合法地址 ＋ 合法钥匙插不进去");
        let (head, tail) = split_keyed_base_url(&keyed).expect("插出来的切不开");
        assert_eq!(format!("{head}{}", &tail[1..]), url, "去掉钥匙段 == 原地址");
        assert_eq!(
            &keyed[head.len()..head.len() + key.len()],
            key,
            "钥匙段就是那一把"
        );
    }
    let url = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    assert_eq!(keyed_base_url(&url, &key[..63]), None, "短钥匙");
    assert_eq!(keyed_base_url(&url, &key.to_uppercase()), None, "大写钥匙");
    assert_eq!(
        keyed_base_url("https://api.example.com/v1", &key),
        None,
        "别人的地址"
    );
    let already = keyed_base_url(&url, &key).unwrap();
    assert_eq!(
        keyed_base_url(&already, &key),
        None,
        "已经带钥匙的不再插一次"
    );
}

/// 来处段（会话血缘）：拼上去的 `~<来处>[~<父>]` 被逆一侧收、被切的一侧还原；钥匙插拔前后尾段原样。
#[test]
fn an_origin_tail_round_trips_through_build_shape_split_and_parse() {
    let key = "0123456789abcdef".repeat(4);
    let parent = "3f2a9c1e-7d44-4c3b-9a55-0e6b2f1d8c77";
    for m in RouteMode::ALL {
        let url = base_url(PORT, m, "claude-code", "work").unwrap();
        for (p, tail) in [
            (None, "~0a1b2c3d4e5f6071".to_string()),
            (Some(parent), format!("~0a1b2c3d4e5f6071~{parent}")),
        ] {
            let with = with_origin(&url, "0a1b2c3d4e5f6071", p).expect("合法来处拼不上");
            assert_eq!(with, format!("{url}/{tail}"));
            assert!(base_url_shape_ok(&with), "带来处的被逆拒了：{with}");
            let keyed = keyed_base_url(&with, &key).expect("带来处的插不进钥匙");
            let (head, rest) = split_keyed_base_url(&keyed).expect("带来处的切不开");
            assert_eq!(
                format!("{head}{}", &rest[1..]),
                with,
                "去掉钥匙段 == 原地址"
            );
            let target = format!("{}/v1/messages", &rest);
            let got = parse_target(&target).expect("带来处的请求目标切不出");
            assert_eq!(
                got,
                Parsed {
                    mode: m,
                    seg1: "claude-code",
                    seg2: "work",
                    origin: Some(Origin {
                        token: "0a1b2c3d4e5f6071",
                        parent: p,
                    }),
                    rest: "v1/messages",
                }
            );
            assert_eq!(
                with_origin(&with, "0a1b2c3d4e5f6071", None),
                None,
                "已经带来处的不再拼一次"
            );
        }
    }
}

/// 来处段坏形：拼的一侧拒（段闸外 · 空），认与切的一侧拒（空 · 三截 · 段闸外 · 不在尾上）。
#[test]
fn bad_origin_tails_are_refused_on_every_side() {
    let url = base_url(PORT, RouteMode::Passthrough, "claude-code", "_").unwrap();
    for (t, p) in [
        ("", None),
        ("a.b", None),
        ("ok", Some("")),
        ("ok", Some("a b")),
    ] {
        assert_eq!(with_origin(&url, t, p), None, "{t:?} {p:?} 拼上了");
    }
    assert_eq!(with_origin("https://api.example.com/v1", "ok", None), None);
    for bad in ["~", "~~p", "~t~", "~t~p~q", "~t.x", "~t~p.q"] {
        assert!(!base_url_shape_ok(&format!("{url}/{bad}")), "{bad} 被收了");
        assert_eq!(
            parse_target(&format!("/t/claude-code/_/{bad}/v1/messages")),
            None,
            "{bad} 被切出来了"
        );
    }
    assert!(
        !base_url_shape_ok(&format!("{url}/~t/x")),
        "来处之后还有段的被收了"
    );
    assert_eq!(
        parse_target("/t/claude-code/_/v1/~t"),
        Some(Parsed {
            mode: RouteMode::Passthrough,
            seg1: "claude-code",
            seg2: "_",
            origin: None,
            rest: "v1/~t",
        }),
        "不紧跟在第 2 段之后的不是来处，原样交上游"
    );
}
