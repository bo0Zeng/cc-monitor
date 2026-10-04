//! 中转的 `observe` 口走到账号域那一头：真中转 ＋ 生产段的上游选择 ＋ 一个按鉴权头作答的假上游。

use super::super::listen::{listen, serve, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE};
use super::*;
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::upstream_select::{table::RoutingTable, Accounts, Upstreams};
use std::io::BufRead;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Mutex;

/// 假上游收到的一发：鉴权头 · 请求体。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Got {
    pub(super) auth: Option<String>,
    pub(super) body: Vec<u8>,
}

/// 按鉴权头作答的假上游：`answer(鉴权头) -> 整段回包字节`。收到的每一发按序记下。
pub(super) fn spawn_judging_upstream(
    answer: fn(Option<&str>) -> String,
) -> (SocketAddr, Arc<Mutex<Vec<Got>>>) {
    let listener = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
    let addr = listener.local_addr().expect("addr");
    let got = Arc::new(Mutex::new(Vec::new()));
    let g2 = Arc::clone(&got);
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = std::io::BufReader::new(s.try_clone().expect("clone"));
            let mut line = String::new();
            if r.read_line(&mut line).is_err() {
                continue;
            }
            let (mut clen, mut auth) = (0usize, None);
            loop {
                let mut h = String::new();
                let n = r.read_line(&mut h).unwrap_or(0);
                if n == 0 || h == "\r\n" {
                    break;
                }
                let (k, v) = h.split_once(':').unwrap_or((&h, ""));
                let v = v.trim().to_string();
                if k.eq_ignore_ascii_case("content-length") {
                    clen = v.parse().unwrap_or(0);
                }
                if k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("x-api-key") {
                    auth = Some(v);
                }
            }
            let mut body = vec![0u8; clen];
            let _ = r.read_exact(&mut body);
            let reply = answer(auth.as_deref());
            g2.lock().expect("lock").push(Got { auth, body });
            let _ = s.write_all(reply.as_bytes());
            let _ = s.flush();
        }
    });
    (addr, got)
}

/// 每 agent 一行的默认上游，Claude 那一家指到假上游（走生产段那条解析，只把旋钮换掉）。
pub(super) fn upstreams_at(up: SocketAddr) -> Upstreams {
    let url = format!("http://127.0.0.1:{}", up.port());
    Upstreams::from_env(&move |k| (k == "CCM_AGENT_UPSTREAM_CLAUDE_CODE").then(|| url.clone()))
        .expect("默认上游")
}

pub(super) fn spawn_relay_over(accounts: Accounts) -> SocketAddr {
    let relay = Arc::new(Relay::new(
        Arc::new(accounts),
        door::Key::for_tests(),
        TeeSink::to_port(Arc::new(Mute)),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Default::default()));
    addr
}

struct Mute;
impl super::super::TapPort for Mute {
    fn offer(&self, _ev: super::super::TapEvent) -> bool {
        false
    }
}

/// 发一整发，读回整段回包。
pub(super) fn shoot(addr: SocketAddr, target: &str, headers: &str, body: &str) -> String {
    let mut c = TcpStream::connect(addr).expect("connect relay");
    c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("read deadline");
    let req = format!(
        "POST /{}{target} HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Content-Length: {}\r\n\r\n{body}",
        door::door_tests::TEST_KEY,
        body.len()
    );
    c.write_all(req.as_bytes()).expect("write");
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out
}

pub(super) fn sse_200(extra: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n{extra}Connection: close\r\n\r\ndata: {{\"type\":\"message_stop\"}}\n\n"
    )
}

pub(super) fn temp_dir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-relay-acct-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("mkdir");
    p
}

/// ★ 用量：回包头 → 上游选择按那一家的读法 → 额度账记在**答这一发的那个号**名下；下游拿到的回包一个字节没少。
#[test]
fn the_quota_headers_of_an_answer_land_in_the_ledger_under_the_account_that_answered() {
    fn answer(_auth: Option<&str>) -> String {
        sse_200(
            "anthropic-ratelimit-unified-status: allowed\r\n\
             anthropic-ratelimit-unified-5h-utilization: 0.25\r\n\
             anthropic-ratelimit-unified-5h-reset: 1800003600\r\n\
             anthropic-ratelimit-unified-representative-claim: five_hour\r\n",
        )
    }
    let (up, got) = spawn_judging_upstream(answer);
    let d = temp_dir("observe");
    let book = Arc::new(Ledger::at(Some(d.join(ledger::FILE_NAME))));
    let accounts = Accounts::new(RoutingTable::build(std::iter::empty()), upstreams_at(up))
        .recording_to(Arc::clone(&book));
    let relay = spawn_relay_over(accounts);
    let resp = shoot(
        relay,
        "/t/claude-code/q/v1/messages",
        "authorization: Bearer from-the-agent\r\nx-claude-code-session-id: s-1\r\n",
        "{}",
    );
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    assert!(resp.contains("anthropic-ratelimit-unified-5h-utilization: 0.25"));
    assert_eq!(got.lock().expect("lock").len(), 1);
    let e = book.entry("claude-code", "q").expect("记在 q 名下");
    assert_eq!(
        (
            e.reading.windows[0].name.as_str(),
            e.reading.windows[0].used
        ),
        ("five_hour", Some(0.25))
    );
    assert!(book.entry("claude-code", "_").is_none());
    let on_disk = ledger::answer_of(Some(&d.join(ledger::FILE_NAME)), 0);
    assert_eq!(on_disk["accounts"][0]["account"], "q");
    let _ = std::fs::remove_dir_all(&d);
}
