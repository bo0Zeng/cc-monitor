//! 中转对「地址写在 agent 自己配置里、钥匙另经环境交」那一形与「先试协议升级、被拒再发普通请求」那一形的组合判据：
//! 真中转 ＋ 生产段的上游选择 ＋ 记头名的假上游（只记请求行与头名，不记值）。

use super::listen::{listen, serve, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE};
use crate::accounts::upstream_select::{table::RoutingTable, Accounts, Upstreams};
use crate::stream::listen::LOOPBACK;
use comms_outward::{Relay, TapEvent, TapPort, TeeSink};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

/// 假上游收到的一发：请求行 ＋ 头名（小写）。
#[derive(Debug, Clone)]
pub(super) struct Seen {
    pub(super) line: String,
    pub(super) names: Vec<String>,
}

/// 记头名的假上游：每一发回 `reply`（整条响应原文）。
pub(super) struct NameUpstream {
    pub(super) addr: SocketAddr,
    pub(super) seen: Arc<Mutex<Vec<Seen>>>,
}

pub(super) fn spawn_name_upstream(reply: &'static str) -> NameUpstream {
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
    let addr = l.local_addr().expect("addr");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_c = Arc::clone(&seen);
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().expect("clone"));
            let mut line = String::new();
            if r.read_line(&mut line).is_err() {
                continue;
            }
            let mut names = Vec::new();
            let mut clen = 0usize;
            loop {
                let mut h = String::new();
                match r.read_line(&mut h) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if h == "\r\n" => break,
                    Ok(_) => {}
                }
                if let Some((k, v)) = h.split_once(':') {
                    let k = k.trim().to_ascii_lowercase();
                    if k == "content-length" {
                        clen = v.trim().parse().unwrap_or(0);
                    }
                    names.push(k);
                }
            }
            let mut body = vec![0u8; clen];
            let _ = r.read_exact(&mut body);
            seen_c.lock().expect("lock").push(Seen {
                line: line.trim_end().to_string(),
                names,
            });
            let _ = s.write_all(reply.as_bytes());
            let _ = s.flush();
        }
    });
    NameUpstream { addr, seen }
}

/// 一条最小的 SSE 响应（事件内容与本族判据无关）。
pub(super) const SSE_OK: &str =
    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {}\n\n";

/// tap 口收到的每一件。
#[derive(Default)]
pub(super) struct Taps(pub(super) Mutex<Vec<TapEvent>>);
impl TapPort for Taps {
    fn offer(&self, ev: TapEvent) -> bool {
        self.0.lock().expect("lock").push(ev);
        true
    }
}

/// 起一个中转：上游选择是生产段那一份（空表 ＋ `upstreams`），tee 落 `taps`。
pub(super) fn spawn_relay_with(upstreams: Upstreams, taps: Arc<Taps>) -> SocketAddr {
    let relay = Arc::new(Relay::new(
        Arc::new(Accounts::new(
            RoutingTable::build(std::iter::empty()),
            upstreams,
        )),
        super::key::key_tests::test_key(),
        TeeSink::to_port(taps),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Arc::new(AtomicUsize::new(0))));
    addr
}

/// 发一条原文请求，读到对端关为止（带期限：挂住换成红）。
pub(super) fn send_raw(addr: SocketAddr, raw: &str) -> String {
    let mut c = TcpStream::connect(addr).expect("connect relay");
    c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("read deadline");
    c.write_all(raw.as_bytes()).expect("write");
    let mut out = Vec::new();
    let _ = c.read_to_end(&mut out);
    String::from_utf8_lossy(&out).to_string()
}

/// 每家都发到 `up` 的那一份默认上游（旋钮盖掉内置默认）。
pub(super) fn all_to(up: SocketAddr) -> Upstreams {
    let url = format!("http://127.0.0.1:{}", up.port());
    Upstreams::from_env(&|k| k.starts_with("CCM_AGENT_UPSTREAM_").then(|| url.clone()))
        .expect("回环明文是合法上游")
}

/// ★ 钥匙在钥匙头里（路径没有钥匙段）的那一发：过门、到上游，而上游收到的请求里**没有**那个头（钥匙不出中转）。
#[test]
fn a_key_carried_in_the_key_header_gets_in_and_is_not_forwarded() {
    let up = spawn_name_upstream(SSE_OK);
    let relay = spawn_relay_with(all_to(up.addr), Arc::new(Taps::default()));
    let h = relay_route_core::KEY_HEADER;
    let key = super::key::key_tests::TEST_KEY;
    let got = send_raw(
        relay,
        &format!(
            "POST /t/claude-code/0/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n{h}: {key}\r\nX-Probe: 1\r\nContent-Length: 2\r\n\r\n{{}}"
        ),
    );
    assert!(
        got.starts_with("HTTP/1.1 200"),
        "钥匙头那一形没过门：{got:?}"
    );
    let seen = up.seen.lock().expect("lock").clone();
    assert_eq!(seen.len(), 1, "该恰好到上游一发：{seen:?}");
    assert_eq!(seen[0].line, "POST /v1/messages HTTP/1.1");
    assert!(
        seen[0].names.iter().any(|n| n == "x-probe"),
        "量具坏了：别的头也没记下：{:?}",
        seen[0].names
    );
    assert!(
        !seen[0].names.iter().any(|n| n.eq_ignore_ascii_case(h)),
        "钥匙头被转到了上游：{:?}",
        seen[0].names
    );
}
