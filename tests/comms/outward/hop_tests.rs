//! 中转去上游那一跳的读数：真中转（回环）打一个假上游，读那一份读数与那一句。

use super::*;
use crate::upstream::{Base, OpenFor, Opened};
use crate::{
    door, Ask, Destination, Destinations, Mode, Relay, RouteKey, TapEvent, TapPort, TeeSink,
};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;

const LOOPBACK: std::net::IpAddr = std::net::IpAddr::V4(Ipv4Addr::LOCALHOST);
const FULL_KEY: &str = "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";
const PASS_KEY: &str = "6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f";
const BODIES: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 1 << 20,
    over: relay_route_core::Over::Reject,
};
const ANSWER: &str = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";

/// 明文假上游：每条连接读一个请求头 ＋ 体，答 [`ANSWER`] 再关。
fn spawn_upstream() -> u16 {
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 假上游");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut tcp) = s else { continue };
            std::thread::spawn(move || {
                let _ = tcp.set_read_timeout(Some(guard_core::wall::CEILING));
                let Ok(Some(raw)) = crate::http1::read_head(&mut tcp, 64 * 1024) else {
                    return;
                };
                let head = crate::http1::parse_request(&raw).expect("假上游收到的该是 HTTP 请求");
                let len = match head.content_length() {
                    crate::http1::BodyLen::Exact(n) => n,
                    crate::http1::BodyLen::Absent | crate::http1::BodyLen::Unparsable => 0,
                };
                let mut body = vec![0u8; len];
                if tcp.read_exact(&mut body).is_err() {
                    return;
                }
                let _ = tcp.write_all(ANSWER.as_bytes());
            });
        }
    });
    port
}

struct To(Base);
impl Destinations for To {
    fn resolve(
        &self,
        _mode: Mode,
        key: &RouteKey,
        _ask: &Ask<'_>,
        act: &mut dyn FnMut(Destination<'_>),
    ) {
        act(Destination::Passthrough {
            upstream: &self.0,
            tag: &key.seg2,
        });
    }
    fn stream_label_headers(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn upstream_idle_reuse(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

struct NoTap;
impl TapPort for NoTap {
    fn offer(&self, _ev: TapEvent) -> bool {
        true
    }
}

/// 起一个中转，上游是 `http://127.0.0.1:<port>`，开 TCP 那一口是 `open`。交回中转地址与它那一份记账。
fn spawn_relay(port: u16, open: Arc<OpenFor>) -> (SocketAddr, Arc<HopMeter>) {
    let base = Base::parse(&format!("http://127.0.0.1:{port}")).expect("base");
    let relay = Arc::new(Relay::new(
        Arc::new(To(base)),
        door::Keys {
            full: Box::new(|s| s == FULL_KEY),
            pass: Box::new(|s| s == PASS_KEY),
        },
        TeeSink::to_port(Arc::new(NoTap)),
        open,
        guard_core::wall::CEILING,
        guard_core::wall::CEILING,
        BODIES,
    ));
    let hop = relay.hop();
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 中转");
    let addr = l.local_addr().expect("中转地址");
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s) = s else { continue };
            let r = Arc::clone(&relay);
            std::thread::spawn(move || {
                let _ = crate::serve_one(s, &r);
            });
        }
    });
    (addr, hop)
}

/// 打一发，读回下游收到的全部字节。
fn send(relay: SocketAddr, body: &str) -> String {
    let mut c = TcpStream::connect(relay).expect("连中转");
    c.set_read_timeout(Some(guard_core::wall::CEILING))
        .expect("读期限");
    let req = format!(
        "POST /{FULL_KEY}/t/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    c.write_all(req.as_bytes()).expect("写请求");
    let mut got = Vec::new();
    c.read_to_end(&mut got).expect("读下游响应");
    String::from_utf8(got).expect("utf8")
}

/// 出网口说经代理开的那一条 ⇒ 读数里经代理；进出字节照线上数；上次连上了。
#[test]
fn a_relayed_send_reports_the_proxy_the_bytes_and_that_it_got_through() {
    let port = spawn_upstream();
    let open: Arc<OpenFor> = Arc::new(move |_from, _h, _p, _d| {
        TcpStream::connect(SocketAddr::new(LOOPBACK, port)).map(|tcp| Opened {
            tcp,
            via_proxy: true,
        })
    });
    let (relay, hop) = spawn_relay(port, open);
    assert_eq!(
        hop.facts(),
        HopFacts::default(),
        "一发都没走过 ⇒ 读数全是缺省"
    );
    let body = "x".repeat(300);
    let got = send(relay, &body);
    assert!(
        got.starts_with("HTTP/1.1 200"),
        "下游该拿到上游那条应答：{got}"
    );
    let f = hop.facts();
    assert_eq!(
        f.endpoint.as_deref(),
        Some(format!("127.0.0.1:{port}").as_str())
    );
    assert!(f.via_proxy, "出网口说经代理 ⇒ 读数里经代理");
    assert_eq!(f.last, Some(Last::Up));
    assert!(
        f.bytes_out >= 300,
        "发出的字节至少有那份请求体：{}",
        f.bytes_out
    );
    assert_eq!(
        f.bytes_in,
        ANSWER.len() as u64,
        "收到的字节就是上游答的那一段"
    );
}

/// 上游连接失败 ⇒ 读数记下卡在建立连接，那一句说出卡在哪。
#[test]
fn an_upstream_that_cannot_be_reached_reports_the_connect_step() {
    let open: Arc<OpenFor> = Arc::new(|_from, _h, _p, _d| {
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        ))
    });
    let (relay, hop) = spawn_relay(9, open);
    let got = send(relay, "{}");
    assert!(got.starts_with("HTTP/1.1 502"), "连接失败 ⇒ 502：{got}");
    let f = hop.facts();
    assert_eq!(f.endpoint.as_deref(), Some("127.0.0.1:9"));
    assert_eq!(f.last, Some(Last::Failed(FailedAt::Connect)));
    assert_eq!((f.bytes_in, f.bytes_out), (0, 0));
    let line = f.line().expect("走过一发 ⇒ 有那一句");
    assert!(line.contains("127.0.0.1:9"), "{line}");
    assert!(
        line.contains(FailedAt::Connect.words().1),
        "说出卡在哪一步：{line}"
    );
}

/// 那一句：经代理那一形照出网口的写法；收发两格照大小的写法；连上了 ⇒ 不说卡在哪。
#[test]
fn the_line_names_the_upstream_the_proxy_and_both_byte_counts() {
    let f = HopFacts {
        endpoint: Some("api.example.test:443".to_string()),
        via_proxy: true,
        bytes_in: 2048,
        bytes_out: 5 << 20,
        last: Some(Last::Up),
    };
    let line = f.line().expect("有上游 ⇒ 有那一句");
    let proxied = copy_core::copy_text(
        "beLinks.route.proxy",
        &[("endpoint", "api.example.test:443")],
    );
    assert!(line.contains(&proxied), "{line}");
    assert!(line.contains(&copy_core::size_text(2048)), "{line}");
    assert!(line.contains(&copy_core::size_text(5 << 20)), "{line}");
    for at in [
        FailedAt::Connect,
        FailedAt::Send,
        FailedAt::NoAnswer,
        FailedAt::NotHttp,
    ] {
        assert!(!line.contains(at.words().1), "连上了不说卡在哪：{line}");
    }
    assert_eq!(
        HopFacts::default().line(),
        None,
        "一发都没走过 ⇒ 没有那一句"
    );
}
