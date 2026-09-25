//! # 要求住址：用户裁决 `V23`（`设计/99 §1`）· `设计/15 §3.3` · `§5.5`
//!
//! 核原文：V23 逐字「今天每台机器只有一条连接可以看情况多开. 智能一点. 这是属于 ssh 优化的部分. 智能多开链接\压缩等等」；
//! `15 §3.3` 那张表「SSH 传输层 | 没显式开；russh `client::Config` 零处设 `preferred`」；
//! `§5.5`「默认已经压上 ⇒ 别重复投资；没有 ⇒ 一行 `preferred` 覆盖全部 SSH 跳」。
//! NT1 题面：「跨互联网那一跳按需开 SSH 压缩 …… 什么时候开（本机回环 / 局域网不开）写清判准，判准只有一处」。
//!
//! 〔NT1 · 2026-09-24〕`dial/connect.rs` 的压缩判准（[`compression_for`]）与它的接法。四件：
//! Z1 真值表 · Z2 两张偏好序 · Z3 判准只有一处、只有一个调用点 · Z4 回环上内核真量得到往返时间、判准答「不压」。
//! 真 sshd 上「强制压 ⇒ 协商出 zlib、线上字节变少」那一维是 [`ZR`] 那条 `#[ignore]` 读数
//! （由 `tests/evidence/NT1-net-loopback.py --compress` 带环境变量来跑）。

use super::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// ★ Z1：判准的真值表。**期望逐格取自 `NT1.md §1.1` 那三行字**（异源于实现）：
/// 回环 ⇒ 不压；读得到 ⇒ 到门槛才压；读不到 ⇒ 压。
#[test]
fn the_compression_judge_answers_exactly_this_table() {
    let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
    let floor = COMPRESS_RTT_FLOOR_US;
    let rows: &[(IpAddr, Option<u32>, bool, &str)] = &[
        (v4(127, 0, 0, 1), None, false, "回环、读不到"),
        (
            v4(127, 0, 0, 1),
            Some(u32::MAX),
            false,
            "回环、往返极大（回环一律不压）",
        ),
        (IpAddr::V6(Ipv6Addr::LOCALHOST), Some(floor), false, "::1"),
        (
            IpAddr::V6(Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped()),
            None,
            false,
            "IPv4 映射的回环",
        ),
        (
            v4(10, 144, 144, 72),
            Some(12_810),
            true,
            "私网段、跨互联网的覆盖网（现打 12.8 ms）",
        ),
        (v4(10, 144, 144, 72), Some(floor), true, "恰在门槛上 ⇒ 压"),
        (
            v4(10, 144, 144, 72),
            Some(floor - 1),
            false,
            "门槛下一微秒 ⇒ 不压",
        ),
        (v4(192, 168, 1, 226), Some(300), false, "局域网 0.3 ms"),
        (v4(47, 245, 114, 38), Some(186_190), true, "公网 186 ms"),
        (v4(47, 245, 114, 38), None, true, "公网、读不到 ⇒ 压"),
        (
            v4(192, 168, 1, 226),
            None,
            true,
            "局域网、读不到 ⇒ 也压（宁可多花 CPU）",
        ),
    ];
    for (peer, rtt, want, why) in rows {
        assert_eq!(
            compression_for(*peer, *rtt),
            *want,
            "{why}：peer={peer} rtt={rtt:?}"
        );
    }
}

/// ★ Z2：两张偏好序。压 ⇒ `zlib@openssh.com` 在首（OpenSSH 服务端 `Compression yes` 今天只给它）、`none` 垫底
/// （远端关了压缩照样连得上）；不压 ⇒ 只有 `none`。`config()` 真把它们装进了 `preferred`。
#[test]
fn the_two_preference_lists_are_exactly_these_and_reach_the_config() {
    let names = |xs: &[russh::compression::Name]| -> Vec<String> {
        xs.iter().map(|n| n.as_ref().to_string()).collect()
    };
    assert_eq!(
        names(COMPRESS_ON),
        ["zlib@openssh.com", "zlib", "none"],
        "压的那张偏好序变了"
    );
    assert_eq!(names(COMPRESS_OFF), ["none"], "不压的那张偏好序变了");
    for (compress, want) in [(true, COMPRESS_ON), (false, COMPRESS_OFF)] {
        for probe in [true, false] {
            let c = config(probe, compress);
            assert_eq!(
                names(&c.preferred.compression),
                names(want),
                "config(probe={probe}, compress={compress}) 没把偏好序装进去"
            );
            // 其余偏好一个不动（只覆盖压缩那一栏）。
            assert_eq!(
                format!("{:?}", c.preferred.cipher),
                format!("{:?}", russh::Preferred::DEFAULT.cipher)
            );
        }
    }
}

/// 生产段里「命中 `needle` 的文件 → 处数」。
fn production_hits(needle: &str) -> Vec<(String, usize)> {
    let root = crate::guard_support::src_root();
    let mut out = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let code = crate::guard_support::production_code(&src);
        let n = code.matches(needle).count();
        if n > 0 {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, n));
        }
    }
    out.sort();
    out
}

/// ★ Z3：**判准只有一处**。① 碰 SSH 压缩偏好的生产文件 == `{dial/connect.rs}`（两向）；
/// ② 判准函数在生产段恰好被调一次（`tcp_hop` —— 直连与跳板都经它）；③ 往返时间只从 `platform::tcp_rtt` 那一个口读。
/// 针运行时拼（本文件自己不被扫到的那一半由 `production_code` 剥测试段保证；拼接是第二道）。
#[test]
fn the_compression_judge_lives_in_exactly_one_place() {
    let pref = format!("{}::{}", "russh", "compression");
    // 6 = 两张表的类型名各 1 ＋ COMPRESS_ON 三项 ＋ COMPRESS_OFF 一项。
    assert_eq!(
        production_hits(&pref),
        vec![("dial/connect.rs".to_string(), 6)],
        "碰 SSH 压缩偏好的地方不止 connect.rs 那两张表"
    );
    let call = format!("{}(", "compression_for");
    // connect.rs 里：定义 1 ＋ 调用 1。
    assert_eq!(
        production_hits(&call),
        vec![("dial/connect.rs".to_string(), 2)],
        "判准函数的调用点不是恰好一处（tcp_hop）"
    );
    let rtt = format!("{}::{}(", "tcp_rtt", "rtt_us");
    assert_eq!(
        production_hits(&rtt),
        vec![("dial/connect.rs".to_string(), 1)],
        "往返时间不止从 tcp_hop 那一处读"
    );
    // 正控：同一个计数器喂一段合成代码，三根针各中一次（针没写坏）。
    let synthetic = format!("{pref} {call} {rtt}");
    assert_eq!(
        (
            synthetic.matches(&pref).count(),
            synthetic.matches(&call).count(),
            synthetic.matches(&rtt).count()
        ),
        (1, 1, 1)
    );
}

/// ★ Z4：回环上**内核真量得到**这一跳的往返时间（读不到就是平台那一半坏了），而且它在门槛之下、判准答「不压」。
/// 异源 = 内核。Linux 之外平台那一支回 `None`，本条只在 Linux 上判得动。
#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_kernel_measures_the_loopback_hop_and_the_judge_says_no() {
    let ls = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = ls.local_addr().unwrap();
    let accept = tokio::spawn(async move { ls.accept().await.map(|(s, _)| s) });
    let ep = Endpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
    };
    let (tcp, compress) = tcp_hop(&ep).await.expect("回环拨不通");
    let rtt = crate::platform::tcp_rtt::rtt_us(&tcp);
    let us = rtt.expect("回环 socket 上 TCP_INFO 读不到往返时间 —— 平台那一半坏了");
    assert!(
        us < COMPRESS_RTT_FLOOR_US,
        "回环往返 {us} µs 竟然不在门槛之下"
    );
    assert!(!compress, "回环上判准答了「压」");
    drop(accept);
}

/// russh 自己的 zlib 一来一回：用它的 `Compress`（`Z_PARTIAL_FLUSH`，与 OpenSSH 每包的冲刷同形）压一包、再用它的 `Decompress` 解，
/// 字节逐字相等才算对。`payload` 取可压的（压缩比远大于 2）。
fn russh_zlib_round_trips(payload: &[u8]) -> bool {
    use russh::compression::{Compress, Compression, Decompress};
    let mut c = Compress::None;
    Compression::Zlib.init_compress(&mut c);
    let mut d = Decompress::None;
    Compression::Zlib.init_decompress(&mut d);
    let mut buf = Vec::new();
    let Ok(packet) = c.compress(payload, &mut buf) else {
        return false;
    };
    let packet = packet.to_vec();
    let mut out = Vec::new();
    d.decompress(&packet, &mut out)
        .is_ok_and(|got| got == payload)
}

/// ★ Z5：**闸 == russh 的解压今天对不对**（两向相等）。异源 = russh 自己的编解码。
///
/// NT1 现打：一包 1000 字节的可压载荷（压成 39 字节）解回来只有 78 字节 —— 解压器最多交出 ≈ 2 × 包长。
/// 正控：压缩比 < 2 的一包（短、近乎不可压）照样一来一回全对 —— 量具本身没用错，坏的只是「解出来比输入多一倍以上」那一形。
#[test]
fn the_gate_matches_what_russh_really_does() {
    let compressible: Vec<u8> = (0..1000).map(|i| b"abcabcabd"[i % 9]).collect();
    let sound = russh_zlib_round_trips(&compressible);
    assert_eq!(
        RUSSH_ZLIB_SOUND, sound,
        "闸（RUSSH_ZLIB_SOUND = {RUSSH_ZLIB_SOUND}）与 russh 的解压实况（一来一回对不对 = {sound}）不一致 —— \
         russh 修好了就开闸（压缩判准的答案才落到连接上）；还坏着就别开（开了每条远端连接都会在第一条通道上卡死）"
    );
    assert!(
        russh_zlib_round_trips(b"q7#Kx"),
        "正控：一包近乎不可压的短载荷都一来一回不对 —— 量具用错了，不是 russh 的那一形"
    );
}

/// ★ ZR（读数，`#[ignore]`）：**真 sshd 上**同一段可压的字节（`seq 1 400000`），强制不压 vs 强制压。
///
/// 由 `tests/evidence/NT1-net-loopback.py --compress` 起回环 sshd、带 `NT1_COMPRESS={host,port,user,key_path}` 来跑；
/// 协商结果（`compression: none` / `zlib@openssh.com`）由那份脚本读 sshd 日志核（异源）。本条自己判：
/// - 不压那趟：载荷收全（> 2 MB）；
/// - 压的那趟：**闸关着（russh 解压坏着）⇒ 30 秒内收不全**（现打的样子是卡在第一条通道上）；
///   闸开着 ⇒ 载荷逐字节同、线上字节 < 不压那趟的一半。
#[ignore = "要真 sshd：由 tests/evidence/NT1-net-loopback.py --compress 带环境变量来跑"]
#[tokio::test(flavor = "multi_thread")]
async fn zr_real_sshd_negotiates_zlib_and_moves_fewer_bytes_when_forced() {
    let Ok(raw) = std::env::var("NT1_COMPRESS") else {
        panic!("没有 NT1_COMPRESS —— 这条读数由 NT1-net-loopback.py --compress 来跑");
    };
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let host = v["host"].as_str().unwrap().to_string();
    let port = v["port"].as_u64().unwrap() as u16;
    let user = v["user"].as_str().unwrap().to_string();
    let key = v["key_path"].as_str().unwrap().to_string();
    let leg = |compress: bool| {
        let (host, user, key) = (host.clone(), user.clone(), key.clone());
        async move {
            let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
                .await
                .unwrap();
            let wire = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let counted = Counting {
                inner: tcp,
                read: std::sync::Arc::clone(&wire),
            };
            let checker = Checker {
                expected: None,
                observed: Default::default(),
                stages: StageSink::new(false),
                endpoint: format!("{host}:{port}"),
            };
            let mut h = russh::client::connect_stream(config(false, compress), counted, checker)
                .await
                .unwrap();
            authenticate(&mut h, &user, Some(&key), None).await.unwrap();
            let body = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                let mut ch = h.channel_open_session().await.ok()?;
                ch.exec(true, "seq 1 400000").await.ok()?;
                let mut body = Vec::new();
                while let Some(m) = ch.wait().await {
                    match m {
                        russh::ChannelMsg::Data { data } => body.extend_from_slice(&data),
                        russh::ChannelMsg::Close => break,
                        _ => {}
                    }
                }
                Some(body)
            })
            .await
            .ok()
            .flatten();
            let _ = h.disconnect(russh::Disconnect::ByApplication, "", "").await;
            (wire.load(std::sync::atomic::Ordering::SeqCst), body)
        }
    };
    let (wire_off, off) = leg(false).await;
    let (wire_on, on) = leg(true).await;
    let off = off.expect("不压那趟都没收全");
    println!(
        "NT1-COMPRESS payload={} wire_off={wire_off} wire_on={wire_on} on_complete={}",
        off.len(),
        on.as_ref().is_some_and(|b| *b == off)
    );
    assert!(off.len() > 2_000_000, "载荷太短：{}", off.len());
    if RUSSH_ZLIB_SOUND {
        assert_eq!(on.as_deref(), Some(off.as_slice()), "压的那趟载荷不同");
        assert!(
            wire_on * 2 < wire_off,
            "压的那趟线上字节 {wire_on} 不到不压那趟 {wire_off} 的一半"
        );
    } else {
        assert!(
            on.as_ref() != Some(&off),
            "闸关着（russh 解压坏着）而真 sshd 上压的那趟收全了 —— russh 那一形不在了？先看 the_gate_matches_what_russh_really_does"
        );
    }
}

/// 数读到的字节（线上字节 = TCP 上读到的，压缩在它之上）。
struct Counting {
    inner: tokio::net::TcpStream,
    read: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl tokio::io::AsyncRead for Counting {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = (buf.filled().len() - before) as u64;
        self.read.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
        r
    }
}

impl tokio::io::AsyncWrite for Counting {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
