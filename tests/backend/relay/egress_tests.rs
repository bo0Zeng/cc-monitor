//! 中转去上游经出网口：真中转 ＋ 生产段的上游选择，上游是内置的 `https://api.anthropic.com`，出网描述指到沙箱里的假 HTTP CONNECT 代理。
//! 直连那一路被测试档断言断着（出网口真开 TCP 的那个目的地不是回环 ⇒ panic），所以假代理收到那一问 ＝ 这一发经了代理。

use super::listen::{listen, serve, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE};
use super::two_form_tests::Taps;
use crate::accounts::upstream_select::{table::RoutingTable, Accounts, Upstreams};
use comms_outward::{Relay, TeeSink};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{mpsc, Arc};

/// 假 HTTP 代理：每接一条连接，把请求头（到空行为止）交出来，回 200，然后关掉（这一跳之后的 TLS 握手不成，中转回 502）。
fn connect_proxy() -> (SocketAddr, mpsc::Receiver<String>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let a = l.local_addr().expect("addr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut got = Vec::new();
            let mut b = [0u8; 1];
            while !got.ends_with(b"\r\n\r\n") {
                if s.read(&mut b).unwrap_or(0) == 0 {
                    break;
                }
                got.push(b[0]);
            }
            let _ = tx.send(String::from_utf8_lossy(&got).into_owned());
            let _ = s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
        }
    });
    (a, rx)
}

/// ★ 经中转的一发（`/t/claude-code/…`，上游是内置默认那一行）⇒ 假代理收到 `CONNECT api.anthropic.com:443`；
/// 中转开上游走的是交给它的那一口（出网口），不是自己开 TCP。
#[test]
fn a_relayed_request_reaches_the_upstream_through_the_configured_proxy() {
    let (proxy, asks) = connect_proxy();
    let via = Arc::new(egress::Egress::via(
        &format!("http://127.0.0.1:{}", proxy.port()),
        "",
    ));
    let relay = Arc::new(Relay::new(
        Arc::new(Accounts::new(
            RoutingTable::build(std::iter::empty()),
            Upstreams::from_env(&|_| None).expect("内置默认上游"),
            crate::platform::fs::Own::for_tests(),
        )),
        super::key::key_tests::test_keys(),
        TeeSink::to_port(Arc::new(Taps::default())),
        Arc::new(move |_: Option<&str>, h: &str, p: u16, d| {
            egress::open(&via, h, p, Some(d))
                .map(|o| o.tcp)
                .map_err(Into::into)
        }),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Default::default()));

    let mut c = TcpStream::connect(addr).expect("connect relay");
    c.set_read_timeout(Some(guard_core::wall::CEILING))
        .expect("read deadline");
    c.write_all(
        format!(
            "POST /{}/t/claude-code/0/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{{}}",
            super::key::key_tests::TEST_KEY
        )
        .as_bytes(),
    )
    .expect("write");
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);

    let head = asks
        .recv_timeout(guard_core::wall::CEILING)
        .unwrap_or_else(|_| panic!("假代理一问都没收到；中转回的是：{out:?}"));
    assert!(
        head.starts_with("CONNECT api.anthropic.com:443 HTTP/1.1\r\n"),
        "{head:?}"
    );
    // 代理那一跳之后 TLS 握手不成 ⇒ 中转出声回 502（不静默挂着）。
    assert!(out.starts_with("HTTP/1.1 502"), "{out:?}");
}

/// ★ 第一条会红（后半句）：`ccm` 起会话时留下的那一份（来处 → 假代理）在沙箱的数据目录里，中转开上游那一口是生产段那一形
/// （上游选择递进来的那只手读描述）；地址带着那个来处的一发 ⇒ 假代理收到 `CONNECT api.anthropic.com:443`。
/// 这台的出网描述是直连（没装 ⇒ 直连；测试档直连非回环当场 panic）⇒ 收到那一问 ＝ 照会话的描述走了。
#[test]
fn a_session_launched_with_a_proxy_reaches_the_upstream_through_that_proxy() {
    use crate::accounts::upstream_select::session_upstream::{egress_of, leave, UpstreamDesc};
    let (proxy, asks) = connect_proxy();
    let data = std::env::temp_dir().join(format!("relay-session-egress-{}", std::process::id()));
    std::fs::remove_dir_all(&data).ok();
    std::fs::create_dir_all(&data).expect("sandbox");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let desc = UpstreamDesc {
        egress: vec![(
            "HTTPS_PROXY".to_string(),
            format!("http://127.0.0.1:{}", proxy.port()),
        )],
        base: None,
        pid: std::process::id(),
        at: 1,
    };
    leave(&own, "0123abcd", &desc, &|_| true).expect("留得下");
    let relay = Arc::new(Relay::new(
        Arc::new(Accounts::new(
            RoutingTable::build(std::iter::empty()),
            Upstreams::from_env(&|_| None).expect("内置默认上游"),
            crate::platform::fs::Own::for_tests(),
        )),
        super::key::key_tests::test_keys(),
        TeeSink::to_port(Arc::new(Taps::default())),
        super::open_for(egress_of(&own)),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Default::default()));

    let mut c = TcpStream::connect(addr).expect("connect relay");
    c.set_read_timeout(Some(guard_core::wall::CEILING))
        .expect("read deadline");
    c.write_all(
        format!(
            "POST /{}/t/claude-code/0/~0123abcd/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{{}}",
            super::key::key_tests::TEST_KEY
        )
        .as_bytes(),
    )
    .expect("write");
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    let head = asks
        .recv_timeout(guard_core::wall::CEILING)
        .unwrap_or_else(|_| panic!("假代理一问都没收到；中转回的是：{out:?}"));
    assert!(
        head.starts_with("CONNECT api.anthropic.com:443 HTTP/1.1\r\n"),
        "{head:?}"
    );
    std::fs::remove_dir_all(&data).ok();
}
