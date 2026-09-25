//! 〔RK1〕中转口的门（`relay/door.rs`）的纯判据：进门三问逐形 · 钥匙文件的住址 / 权限 / 跨重起不变 / 坏了就换。
//!
//! 守的要求：`INVARIANTS §48.1`「中转口」那一格 —— 逐字「中转口要钥匙：没钥匙 / 错钥匙 / 钥匙前缀 ⇒ 拒，
//! 与『表里没这一行 → 404』可分；带 `Origin` / `Host` 非回环 ⇒ 拒」。设计住 `调研/第四波记录/RK1.md §1`。
//! 走真 socket 的那一半（403/421 与 404 真可分 · 对的钥匙真转发 · 钥匙不出现在日志 / tee / argv / 上游）住
//! `server_tests.rs` 的 `rk1_*` 那几条。

use super::*;
use crate::relay::http1;

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
        assert!(host_is_loopback(h), "{h:?} 是回环字面量");
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
        assert!(!host_is_loopback(h), "{h:?} 不是回环字面量");
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
    let whys: std::collections::BTreeSet<_> = faces.iter().map(|f| f.1).collect();
    assert_eq!(whys.len(), 3, "三句为什么必须两两不同：{faces:?}");
    assert_eq!(Verdict::Pass("/".into()).refusal(), None);
}

/// ⑤ 钥匙文件：第一次铸（64 位小写十六进制 · `0600`）→ 第二次读回**同一把**（跨重起不变）；
///    坏文件（空 / 短 / 非十六进制 / 大写）⇒ 换一把新的、形状合法。
#[test]
fn the_key_file_is_minted_once_private_and_read_back_across_restarts() {
    let home = std::env::temp_dir().join(format!(
        "ccm-rk1-door-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).expect("建夹具家目录");
    let get = |k: &str| (k == "HOME").then(|| home.display().to_string());
    let p = key_path(&get).expect("有 HOME 就有路径");
    assert_eq!(p, home.join(".cc-monitor").join("relay-key"));
    assert!(!p.exists(), "夹具家目录一开始不该有钥匙");

    let first = ensure_key(&p).expect("第一次铸");
    let on_disk = std::fs::read_to_string(&p).expect("读回");
    assert_eq!(on_disk, first.expose(), "盘上就是那一把");
    assert!(
        key_shape_ok(first.expose()),
        "形状：{}",
        first.expose().len()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&p).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "钥匙文件必须只给本人");
    }
    let again = ensure_key(&p).expect("第二次");
    assert_eq!(
        again.expose(),
        first.expose(),
        "重起中转必须读回同一把（端口固定 ⇒ 活会话靠它）"
    );

    for junk in [
        "",
        "abc",
        &"z".repeat(64),
        &first.expose().to_ascii_uppercase(),
    ] {
        std::fs::write(&p, junk).expect("写坏文件");
        let fresh = ensure_key(&p).expect("坏文件 ⇒ 换一把");
        assert!(key_shape_ok(fresh.expose()), "换出来的形状要对");
        assert_ne!(fresh.expose(), junk, "坏的那一份不许被认下");
        assert_eq!(std::fs::read_to_string(&p).expect("读回"), fresh.expose());
    }
    // 两次铸出来的不是同一把（随机数真在转）。
    std::fs::write(&p, "").expect("清空");
    let a = ensure_key(&p).expect("a");
    std::fs::write(&p, "").expect("清空");
    let b = ensure_key(&p).expect("b");
    assert_ne!(a.expose(), b.expose(), "两次铸出同一把 —— 随机源是死的");
    // 目录里只剩那一份（临时文件都挪走了）。
    let left: Vec<_> = std::fs::read_dir(p.parent().expect("父"))
        .expect("列目录")
        .map(|e| e.expect("项").file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        left,
        vec!["relay-key".to_string()],
        "目录里留了临时文件：{left:?}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 家目录解析不出来 ⇒ 没有路径（上层据此**拒绝起**，不猜一个路径去写）。
#[test]
fn no_home_means_no_key_path() {
    assert_eq!(key_path(&|_| None), None);
    assert_eq!(
        key_path(&|k| (k == "HOME").then(String::new)),
        None,
        "空 HOME == 没有"
    );
    assert_eq!(
        key_path(&|k| (k == "USERPROFILE").then(|| "C:/Users/u".to_string())),
        Some(std::path::Path::new("C:/Users/u").join(KEY_FILE_REL)),
        "没有 HOME 退 USERPROFILE"
    );
}

/// `Key` 不许被 `{:?}` 打出值（谁把它塞进日志，打出来的也只有 `Key(…)`）。
#[test]
fn a_key_never_prints_its_value() {
    let shown = format!("{:?}", Key::for_tests());
    assert_eq!(shown, "Key(…)");
    assert!(!shown.contains(TEST_KEY));
}
