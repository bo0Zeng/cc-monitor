//! 中转口的门（`door.rs`）的纯判据：进门三问逐形 · 三种拒法两两可分 · 钥匙不打出值。
//!
//! 守的要求：`INVARIANTS §48.1a`（中转口的钥匙），逐字一句：「过了才剥掉那一段交给路由（`/s/` 与 `/t/` 一样要过）
//! ⇒ 「钥匙对、表里没这一行」仍是 **404**，与 403 **可分**；三种拒法各带一句说得清是哪一问的话。」
//! 钥匙文件那一半（住址 / 权限 / 跨重起不变）在后端 `key_tests.rs`；走真 socket 的那一半在后端 `server_tests.rs` 的 `rk1_*` 那几条。

use super::*;
use crate::http1;

/// 判据用的那一把（形状合法、一眼认得出是测试值；与后端 `key_tests::TEST_KEY` 同值）。
const TEST_KEY: &str = "7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57";

impl Key {
    /// 判据用：[`TEST_KEY`] 那一把。
    fn for_tests() -> Key {
        Key(TEST_KEY.to_string())
    }
}

/// 用一段请求头原文造 `RequestHead`（走生产段那一份解析，不手搓结构体）。
fn head(raw: &str) -> RequestHead {
    http1::parse_request(raw.as_bytes()).unwrap_or_else(|| panic!("夹具请求头不成形：{raw:?}"))
}

fn req(target: &str, headers: &str) -> RequestHead {
    head(&format!("POST {target} HTTP/1.1\r\n{headers}\r\n"))
}

const LOOP: &str = "Host: 127.0.0.1:8788\r\n";

/// ① 钥匙那一问：对的过、剥得干净；没有 / 错 / 前缀 / 多一截 / 大小写翻转 / 空段 ⇒ `BadKey`。
/// （绝对形式 `POST http://…` 到不了门：`http1::parse_request` 先拒，下游拿 400。）
#[test]
fn only_the_exact_key_as_the_first_segment_gets_in() {
    let k = Key::for_tests();
    let good = format!("/{TEST_KEY}/s/claude-code/acctA/sid/v1/messages?beta=true");
    assert_eq!(
        admit(&req(&good, LOOP), &k),
        Verdict::Pass("/s/claude-code/acctA/sid/v1/messages?beta=true".into()),
        "对的钥匙必须过，且只剥掉钥匙那一段"
    );
    assert_eq!(
        admit(
            &req(
                &format!("/{TEST_KEY}/t/claude-code/0/sid/v1/messages"),
                LOOP
            ),
            &k
        ),
        Verdict::Pass("/t/claude-code/0/sid/v1/messages".into()),
        "`/t/` 同样先过门"
    );
    let upper = TEST_KEY.to_ascii_uppercase();
    let wrong = format!("{}0", &TEST_KEY[..TEST_KEY.len() - 1]);
    for bad in [
        "/s/claude-code/acctA/sid/v1/messages".to_string(), // 没钥匙（今天的老形状）
        "/t/claude-code/0/sid/v1/messages".to_string(),
        format!("/{wrong}/s/claude-code/acctA/sid/v1/messages"), // 错一个字符
        format!("/{}/s/a/b/c/v1", &TEST_KEY[..32]),              // 前缀
        format!("/{TEST_KEY}0/s/a/b/c/v1"),                      // 多一截
        format!("/{upper}/s/a/b/c/v1"),                          // 大小写
        "//s/claude-code/acctA/sid/v1/messages".to_string(),     // 空段（钥匙文件不在时的展开形）
        format!("/{TEST_KEY}"),                                  // 只有钥匙、后面没有路径
    ] {
        assert_eq!(
            admit(&req(&bad, LOOP), &k),
            Verdict::BadKey,
            "{bad:?} 不该过门"
        );
    }
}

/// ③ `Origin` 那一问：带了就拒，**不管值是什么**、不管钥匙对不对（排在钥匙之前）。
#[test]
fn any_origin_header_is_refused_before_the_key_is_looked_at() {
    let k = Key::for_tests();
    let good = format!("/{TEST_KEY}/s/claude-code/acctA/sid/v1/messages");
    for o in [
        "Origin: https://evil.example\r\n",
        "origin: null\r\n",
        "ORIGIN: http://127.0.0.1:8788\r\n",
    ] {
        assert_eq!(
            admit(&req(&good, &format!("{LOOP}{o}")), &k),
            Verdict::Browser,
            "{o:?}"
        );
    }
}

/// ③ `Host` 那一问：回环三形（可带口、大小写不敏感）放行；别的、缺、重复 ⇒ `NotLoopbackHost`。
#[test]
fn only_a_loopback_literal_host_gets_in() {
    let k = Key::for_tests();
    let good = format!("/{TEST_KEY}/s/claude-code/acctA/sid/v1/messages");
    for h in [
        "127.0.0.1",
        "127.0.0.1:8788",
        "localhost",
        "LOCALHOST:8788",
        "[::1]",
        "[::1]:8788",
    ] {
        assert!(host_header_is_loopback_literal(h), "{h:?} 是回环字面量");
        assert!(
            matches!(
                admit(&req(&good, &format!("Host: {h}\r\n")), &k),
                Verdict::Pass(_)
            ),
            "{h:?}"
        );
    }
    for h in [
        "evil.example",
        "evil.example:8788",
        "127.0.0.1.evil.example",
        "localhost.evil.example:8788",
        "127.0.0.2",
        "0.0.0.0:8788",
        "[::1].evil",
        "127.0.0.1:",
        "127.0.0.1:99999",
        "127.0.0.1:80:80",
        "",
    ] {
        assert!(!host_header_is_loopback_literal(h), "{h:?} 不是回环字面量");
        assert_eq!(
            admit(&req(&good, &format!("Host: {h}\r\n")), &k),
            Verdict::NotLoopbackHost,
            "{h:?}"
        );
    }
    assert_eq!(
        admit(&req(&good, ""), &k),
        Verdict::NotLoopbackHost,
        "没有 Host"
    );
    assert_eq!(
        admit(&req(&good, "Host: 127.0.0.1\r\nHost: evil.example\r\n"), &k),
        Verdict::NotLoopbackHost,
        "两个 Host"
    );
}

/// 拒绝那几格：三种结局的（状态行, 说法）两两不同；`Pass` 没有拒绝的说法。
#[test]
fn the_three_refusals_are_distinct_faces() {
    let faces: Vec<_> = [Verdict::Browser, Verdict::NotLoopbackHost, Verdict::BadKey]
        .iter()
        .map(|v| v.refusal().expect("拒绝那几格都有说法"))
        .collect();
    assert_eq!(faces[0].0, FORBIDDEN);
    assert_eq!(faces[1].0, MISDIRECTED);
    assert_eq!(faces[2].0, FORBIDDEN);
    // 原因头的值手写：两个 403 靠它分开。
    let reasons: Vec<_> = faces.iter().map(|f| f.1).collect();
    assert_eq!(reasons, ["browser-origin", "host-not-loopback", "bad-key"]);
    let whys: std::collections::BTreeSet<_> = faces.iter().map(|f| f.2).collect();
    assert_eq!(whys.len(), 3, "三句为什么必须两两不同：{faces:?}");
    assert_eq!(Verdict::Pass("/".into()).refusal(), None);
}

/// `Key` 不许被 `{:?}` 打出值（谁把它塞进日志，打出来的也只有 `Key(…)`）。
#[test]
fn a_key_never_prints_its_value() {
    let shown = format!("{:?}", Key::for_tests());
    assert_eq!(shown, "Key(…)");
    assert!(!shown.contains(TEST_KEY));
}
