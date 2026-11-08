//! 出网口：沙箱里的假 HTTP CONNECT 代理与假 SOCKS5 代理（都在回环上）。直连那一路被测试档断言断着：
//! 目的地不是回环、又没经代理 ⇒ panic（`tcp_to` 那一句），所以「到了假代理」就是「经了代理」。

use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;

fn wait() -> Option<Duration> {
    Some(Duration::from_secs(10))
}

/// 假 HTTP 代理：接一条连接，读到空行为止，把请求头交出来；回 `answer`，之后当回声（隧道那头的「目的地」）。
fn http_proxy(answer: &'static str) -> (SocketAddr, mpsc::Receiver<String>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let a = l.local_addr().expect("addr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut s, _)) = l.accept() else { return };
        let mut got = Vec::new();
        let mut b = [0u8; 1];
        while !got.ends_with(b"\r\n\r\n") {
            if s.read(&mut b).unwrap_or(0) == 0 {
                return;
            }
            got.push(b[0]);
        }
        // 测试那一头已经走了 / 连接断了 ⇒ 这台假代理就此收工。
        if tx.send(String::from_utf8_lossy(&got).into_owned()).is_err()
            || s.write_all(answer.as_bytes()).is_err()
        {
            return;
        }
        echo(s);
    });
    (a, rx)
}

fn echo(mut s: std::net::TcpStream) {
    let mut buf = [0u8; 1024];
    loop {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if s.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
        }
    }
}

/// SOCKS5 代理收到的那一问。
#[derive(Debug, PartialEq)]
struct SocksAsk {
    methods: Vec<u8>,
    creds: Option<(String, String)>,
    atyp: u8,
    addr: Vec<u8>,
    port: u16,
}

/// 假 SOCKS5 代理：要鉴权（`want`）就挑 0x02 并核对；记下那一问，回成功，之后当回声。`reply` 是回包第二字节（0 = 成功）。
fn socks_proxy(
    want: Option<(&'static str, &'static str)>,
    reply: u8,
) -> (SocketAddr, mpsc::Receiver<SocksAsk>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let a = l.local_addr().expect("addr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut s, _)) = l.accept() else { return };
        let mut n = [0u8; 2];
        s.read_exact(&mut n).expect("hello");
        let mut methods = vec![0u8; n[1] as usize];
        s.read_exact(&mut methods).expect("methods");
        let mut creds = None;
        if want.is_some() {
            s.write_all(&[5, 2]).expect("pick");
            let mut v = [0u8; 2];
            s.read_exact(&mut v).expect("ulen");
            let mut u = vec![0u8; v[1] as usize];
            s.read_exact(&mut u).expect("user");
            let mut pl = [0u8; 1];
            s.read_exact(&mut pl).expect("plen");
            let mut p = vec![0u8; pl[0] as usize];
            s.read_exact(&mut p).expect("pass");
            let c = (String::from_utf8(u).unwrap(), String::from_utf8(p).unwrap());
            let ok = want.is_some_and(|(wu, wp)| c.0 == wu && c.1 == wp);
            creds = Some(c);
            s.write_all(&[1, if ok { 0 } else { 1 }])
                .expect("auth reply");
            if !ok {
                return;
            }
        } else {
            s.write_all(&[5, 0]).expect("pick");
        }
        let mut h = [0u8; 4];
        s.read_exact(&mut h).expect("req head");
        let addr = match h[3] {
            1 => {
                let mut v = vec![0u8; 4];
                s.read_exact(&mut v).unwrap();
                v
            }
            4 => {
                let mut v = vec![0u8; 16];
                s.read_exact(&mut v).unwrap();
                v
            }
            _ => {
                let mut l = [0u8; 1];
                s.read_exact(&mut l).unwrap();
                let mut v = vec![0u8; l[0] as usize];
                s.read_exact(&mut v).unwrap();
                v
            }
        };
        let mut p = [0u8; 2];
        s.read_exact(&mut p).unwrap();
        let asked = SocksAsk {
            methods,
            creds,
            atyp: h[3],
            addr,
            port: u16::from_be_bytes(p),
        };
        // 测试那一头已经走了 / 连接断了 ⇒ 这台假代理就此收工。
        if tx.send(asked).is_err()
            || s.write_all(&[5, reply, 0, 1, 127, 0, 0, 1, 0x1f, 0x90])
                .is_err()
        {
            return;
        }
        if reply == 0 {
            echo(s);
        }
    });
    (a, rx)
}

fn round_trip(o: &mut Opened) -> Vec<u8> {
    o.tcp.write_all(b"SSH-2.0-probe\r\n").expect("write");
    let mut back = [0u8; 15];
    o.tcp.read_exact(&mut back).expect("read back");
    back.to_vec()
}

/// ★ HTTP CONNECT：`CONNECT 主机:端口` ＋ Host 头；回 200 之后这条 TCP 两向照搬字节；对端是代理。
#[test]
fn an_http_proxy_gets_connect_and_the_tunnel_carries_bytes_both_ways() {
    let (p, asks) = http_proxy("HTTP/1.1 200 Connection established\r\n\r\n");
    let e = Egress::via(&format!("http://127.0.0.1:{}", p.port()), "");
    let mut o = open(&e, "api.anthropic.com", 443, wait()).expect("经代理开");
    let head = asks.recv().expect("代理收到一问");
    assert!(
        head.starts_with("CONNECT api.anthropic.com:443 HTTP/1.1\r\n"),
        "{head:?}"
    );
    assert!(
        head.contains("\r\nHost: api.anthropic.com:443\r\n"),
        "{head:?}"
    );
    assert!(
        !head.contains("Proxy-Authorization"),
        "没设鉴权就不带：{head:?}"
    );
    assert!(o.via_proxy);
    assert_eq!(round_trip(&mut o), b"SSH-2.0-probe\r\n");
}

/// 代理鉴权：URL 里的 `user:pass@`（百分号解码过）⇒ `Proxy-Authorization: Basic …`。
#[test]
fn http_proxy_credentials_go_as_basic_auth() {
    let (p, asks) = http_proxy("HTTP/1.1 200 OK\r\n\r\n");
    let e = Egress::via(
        &format!("http://al%40ce:s3cr%3At@127.0.0.1:{}/", p.port()),
        "",
    );
    open(&e, "devbox.test", 22, wait()).expect("经代理开");
    let head = asks.recv().expect("代理收到一问");
    // base64("al@ce:s3cr:t")
    assert!(
        head.contains("\r\nProxy-Authorization: Basic YWxAY2U6czNjcjp0\r\n"),
        "{head:?}"
    );
}

/// IPv6 目的地：CONNECT 行里带方括号。
#[test]
fn an_ipv6_destination_is_bracketed_in_the_connect_line() {
    let (p, asks) = http_proxy("HTTP/1.1 200 OK\r\n\r\n");
    let e = Egress::via(&format!("127.0.0.1:{}", p.port()), "");
    open(&e, "2001:db8::7", 22, wait()).expect("经代理开");
    assert!(asks
        .recv()
        .unwrap()
        .starts_with("CONNECT [2001:db8::7]:22 HTTP/1.1\r\n"));
}

/// 代理拒（407）⇒ 错在代理那一步，话里带状态行。
#[test]
fn a_refusing_http_proxy_fails_at_the_proxy_stage_with_its_status_line() {
    let (p, _asks) = http_proxy("HTTP/1.1 407 Proxy Authentication Required\r\n\r\n");
    let e = Egress::via(&format!("http://127.0.0.1:{}", p.port()), "");
    let f = open(&e, "api.anthropic.com", 443, wait()).expect_err("代理拒了");
    assert_eq!(f.stage, Stage::Proxy);
    assert!(f.io.to_string().contains("407"), "{}", f.io);
}

/// SOCKS5（`socks5h://`）：名字交给代理（ATYP 3）· 带鉴权时报两种方法、走用户名口令子协商；之后隧道照搬字节。
#[test]
fn a_socks5h_proxy_gets_the_name_and_the_credentials() {
    let (p, asks) = socks_proxy(Some(("u", "pw")), 0);
    let e = Egress::via(&format!("socks5h://u:pw@127.0.0.1:{}", p.port()), "");
    let mut o = open(&e, "devbox.test", 22, wait()).expect("经 SOCKS5 开");
    let ask = asks.recv().expect("代理收到一问");
    assert_eq!(
        ask,
        SocksAsk {
            methods: vec![0, 2],
            creds: Some(("u".into(), "pw".into())),
            atyp: 3,
            addr: b"devbox.test".to_vec(),
            port: 22,
        }
    );
    assert!(o.via_proxy);
    assert_eq!(round_trip(&mut o), b"SSH-2.0-probe\r\n");
}

/// `socks5://`：地址在这台解析（IP 字面就是它自己）、交给代理的是地址（ATYP 1）；没设鉴权只报「不要鉴权」。
#[test]
fn a_socks5_proxy_gets_an_address() {
    let (p, asks) = socks_proxy(None, 0);
    let e = Egress::via(&format!("socks5://127.0.0.1:{}", p.port()), "");
    open(&e, "192.0.2.7", 443, wait()).expect("经 SOCKS5 开");
    let ask = asks.recv().unwrap();
    assert_eq!(
        (ask.methods, ask.atyp, ask.addr, ask.port),
        (vec![0], 1, vec![192, 0, 2, 7], 443)
    );
}

/// SOCKS5 回非零 ⇒ 错在代理那一步。
#[test]
fn a_refusing_socks5_proxy_fails_at_the_proxy_stage() {
    let (p, _asks) = socks_proxy(None, 5);
    let e = Egress::via(&format!("socks5h://127.0.0.1:{}", p.port()), "");
    let f = open(&e, "devbox.test", 22, wait()).expect_err("代理拒了");
    assert_eq!(f.stage, Stage::Proxy);
}

/// 回环目的地恒直连：设了代理也不经它（代理够不到这台的回环）。
#[test]
fn a_loopback_destination_never_goes_through_the_proxy() {
    let (p, asks) = http_proxy("HTTP/1.1 200 OK\r\n\r\n");
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let e = Egress::via(&format!("http://127.0.0.1:{}", p.port()), "");
    let o = open(&e, "127.0.0.1", target.local_addr().unwrap().port(), wait()).expect("直连回环");
    assert!(!o.via_proxy);
    target.accept().expect("目的地自己接到");
    assert!(asks.try_recv().is_err(), "代理不该收到一问");
}

/// 「留在这台」（`describe::stays_on_this_machine`）＝ 回环那一个事实（`upstream_url_core::upstream_is_loopback`）
/// 再加 `*.localhost`（RFC 6761 约定解到回环）与尾点：IPv4 映射的回环两份都认；`a.localhost` 回环判法否、这里是。
#[test]
fn staying_on_this_machine_is_the_loopback_fact_plus_localhost_names() {
    let e = Egress::via("http://127.0.0.1:3128", "");
    // (主机, 回环那一个事实, 留在这台)
    for (h, fact, stays) in [
        ("127.0.0.1", true, true),
        ("::ffff:127.0.0.1", true, true),
        ("[::ffff:127.0.0.1]", true, true),
        ("localhost", true, true),
        ("a.localhost", false, true),
        ("localhost.", false, true),
        ("::ffff:192.0.2.1", false, false),
        ("localhost.evil", false, false),
    ] {
        assert_eq!(upstream_url_core::upstream_is_loopback(h), fact, "{h}");
        assert_eq!(describe::stays_on_this_machine(h), stays, "{h}");
        assert_eq!(e.hop_for(h) == Hop::Direct, stays, "{h}");
    }
}

/// 免代理表：`*` · 名字与它的子域 · IP · CIDR ⇒ 直连；别的照代理。
#[test]
fn the_no_proxy_list_sends_matching_destinations_direct() {
    let e = Egress::via(
        "http://127.0.0.1:3128",
        "corp.example, .lan 198.51.100.0/24,192.0.2.9,[2001:db8::1]",
    );
    for h in [
        "corp.example",
        "git.corp.example",
        "nas.lan",
        "198.51.100.77",
        "192.0.2.9",
        "2001:db8::1",
        "[2001:db8::1]",
        "localhost",
    ] {
        assert_eq!(e.hop_for(h), Hop::Direct, "{h}");
    }
    for h in [
        "example",
        "notcorp.example",
        "lanx",
        "198.51.101.1",
        "192.0.2.10",
        "api.anthropic.com",
    ] {
        assert!(matches!(e.hop_for(h), Hop::Via(_)), "{h}");
    }
    assert_eq!(
        Egress::via("http://127.0.0.1:3128", "*").hop_for("api.anthropic.com"),
        Hop::Direct
    );
}

/// 环境读法：小写在前 · `https_proxy` 先于 `all_proxy` · 空串当没设 · 不认 `http_proxy` · 免代理表同读。
#[test]
fn the_environment_is_read_in_order() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        Egress::from_env(&move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        })
    };
    let proxy_of = |e: &Egress| match e.hop_for("api.anthropic.com") {
        Hop::Via(p) => Some((p.kind, p.host.clone(), p.port)),
        _ => None,
    };
    assert_eq!(proxy_of(&env(&[])), None);
    assert_eq!(proxy_of(&env(&[("http_proxy", "http://h:1")])), None);
    assert_eq!(
        proxy_of(&env(&[
            ("HTTPS_PROXY", "http://big:2"),
            ("https_proxy", "http://small:3")
        ])),
        Some((Kind::Http, "small".into(), 3))
    );
    assert_eq!(
        proxy_of(&env(&[("https_proxy", "  "), ("ALL_PROXY", "socks5h://s")])),
        Some((Kind::Socks5Remote, "s".into(), 1080))
    );
    let e = env(&[("https_proxy", "http://h:8"), ("NO_PROXY", "anthropic.com")]);
    assert_eq!(e.hop_for("api.anthropic.com"), Hop::Direct);
}

/// 代理那一格读不懂 ⇒ 一条都不连（不改直连），错在代理那一步、说出是哪一格。
#[test]
fn an_unreadable_proxy_setting_refuses_instead_of_going_direct() {
    let e =
        Egress::from_env(&|k| (k == "https_proxy").then(|| "https://tls-proxy:443".to_string()));
    assert!(matches!(
        e.hop_for("api.anthropic.com"),
        Hop::Refuse {
            from: "https_proxy",
            ..
        }
    ));
    let f = open(&e, "api.anthropic.com", 443, wait()).expect_err("不连");
    assert_eq!(f.stage, Stage::Proxy);
    assert!(f.io.to_string().contains("https_proxy"), "{}", f.io);
}

/// 描述进日志不带口令。
#[test]
fn the_description_never_prints_the_proxy_password() {
    let e = Egress::via("http://user:hunter2@127.0.0.1:3128", "");
    let shown = format!("{e:?}");
    assert!(
        shown.contains("user") && !shown.contains("hunter2"),
        "{shown}"
    );
}

/// 没装 ⇒ 直连。
#[test]
fn without_an_installed_description_it_is_direct() {
    assert_eq!(here().hop_for("api.anthropic.com"), Hop::Direct);
}
