//! 中转的 `observe` 口走到账号域那一头：真中转 ＋ 生产段的上游选择 ＋ 一个按鉴权头作答的假上游。

use super::listen::{listen, serve, DOWNSTREAM_DEADLINE, UPSTREAM_DEADLINE};
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::upstream_select::{table::RoutingTable, Accounts, Upstreams};
use crate::stream::listen::LOOPBACK;
use comms_outward::{Relay, TapEvent, TapPort, TeeSink};
use std::io::BufRead;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
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
        super::key::key_tests::test_keys(),
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
impl TapPort for Mute {
    fn offer(&self, _ev: TapEvent) -> bool {
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
        super::key::key_tests::TEST_KEY,
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

// ── `retry` 口本身（中转的契约，不经账号域）：一个照本子作答的假上游选择 ─────────────────

/// 第一发原样转发（标签 `t0`）；回包被拒（或 `always`）⇒ 换头、换整份请求体再发（标签 `t1`、`t2` …）。
struct Scripted {
    up: comms_outward::Base,
    retry_to: comms_outward::Base,
    body: Vec<u8>,
    always: bool,
    asked: Mutex<Vec<Vec<String>>>,
}

impl comms_outward::Destinations for Scripted {
    fn resolve(
        &self,
        _mode: comms_outward::Mode,
        _key: &comms_outward::RouteKey,
        _ask: &comms_outward::Ask<'_>,
        act: &mut dyn FnMut(comms_outward::Destination<'_>),
    ) {
        act(comms_outward::Destination::Passthrough {
            upstream: &self.up,
            tag: "t0",
        });
    }

    fn retry(
        &self,
        _mode: comms_outward::Mode,
        _key: &comms_outward::RouteKey,
        _ask: &comms_outward::Ask<'_>,
        seen: &comms_outward::Heard<'_>,
        tried: &[&str],
        act: &mut dyn FnMut(comms_outward::Destination<'_>),
    ) {
        self.asked
            .lock()
            .expect("lock")
            .push(tried.iter().map(|t| t.to_string()).collect());
        if seen.status != 429 && !self.always {
            return;
        }
        let tag = format!("t{}", tried.len());
        act(comms_outward::Destination::Substitute {
            upstream: &self.retry_to,
            auth: comms_outward::AuthSwap {
                clear: &["Authorization", "x-api-key"],
                write: Some(("Authorization", "Bearer swapped")),
            },
            body: Some(&self.body),
            tag: &tag,
        });
    }

    fn stream_label_headers(&self) -> Vec<&'static str> {
        Vec::new()
    }

    fn stream_owner_headers(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

fn base_of(up: SocketAddr) -> comms_outward::Base {
    comms_outward::Base::parse(&format!("http://127.0.0.1:{}", up.port())).expect("base")
}

fn refused_429() -> String {
    "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{}".to_string()
}

fn relay_over(d: Arc<dyn comms_outward::Destinations>) -> SocketAddr {
    let relay = Arc::new(Relay::new(
        d,
        super::key::key_tests::test_keys(),
        TeeSink::to_port(Arc::new(Mute)),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || serve(listener, relay, Default::default()));
    addr
}

/// ★ 被拒 ⇒ 问 `retry`，换头 ＋ 换整份请求体重发；下游只拿到第二发的回包（第一发一个字节都没下去）。
#[test]
fn a_refused_answer_is_resent_elsewhere_and_only_the_second_answer_goes_down() {
    fn answer(auth: Option<&str>) -> String {
        if auth == Some("Bearer swapped") {
            sse_200("x-which: second\r\n")
        } else {
            refused_429()
        }
    }
    let (up, got) = spawn_judging_upstream(answer);
    let d = Arc::new(Scripted {
        up: base_of(up),
        retry_to: base_of(up),
        body: b"{\"swapped\":1}".to_vec(),
        always: false,
        asked: Mutex::new(Vec::new()),
    });
    let relay = relay_over(d.clone());
    let resp = shoot(
        relay,
        "/t/x/y/v1/messages",
        "authorization: Bearer mine\r\n",
        "{\"orig\":1}",
    );
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    assert!(resp.contains("x-which: second"));
    assert!(!resp.contains("429"));
    let got = got.lock().expect("lock").clone();
    assert_eq!(
        got,
        [
            Got {
                auth: Some("Bearer mine".into()),
                body: b"{\"orig\":1}".to_vec()
            },
            Got {
                auth: Some("Bearer swapped".into()),
                body: b"{\"swapped\":1}".to_vec()
            },
        ]
    );
    assert_eq!(
        *d.asked.lock().expect("lock"),
        [vec!["t0".to_string()], vec!["t0".into(), "t1".into()]]
    );
}

/// ★ 不打转：上游选择每次都说「再换」，中转也至多重发 [`RETRY_HARD_CAP`] 次，然后把手上那个回包原样交下去。
#[test]
fn the_relay_stops_resending_at_its_hard_cap() {
    fn answer(_auth: Option<&str>) -> String {
        refused_429()
    }
    let (up, got) = spawn_judging_upstream(answer);
    let d = Arc::new(Scripted {
        up: base_of(up),
        retry_to: base_of(up),
        body: b"{}".to_vec(),
        always: true,
        asked: Mutex::new(Vec::new()),
    });
    let resp = shoot(relay_over(d), "/t/x/y/v1/messages", "", "{}");
    assert!(resp.starts_with("HTTP/1.1 429"), "{resp}");
    let cap = comms_outward::test_support::server::RETRY_HARD_CAP;
    assert_eq!(got.lock().expect("lock").len(), cap + 1);
}

/// 重发那一发连不上 ⇒ 不再换，第一发那个拒绝原样交下去（不吞成 502）。
#[test]
fn a_resend_that_cannot_connect_hands_down_the_answer_in_hand() {
    fn answer(_auth: Option<&str>) -> String {
        refused_429()
    }
    let (up, got) = spawn_judging_upstream(answer);
    let closed = {
        let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
        l.local_addr().expect("addr")
    };
    let d = Arc::new(Scripted {
        up: base_of(up),
        retry_to: base_of(closed),
        body: b"{}".to_vec(),
        always: false,
        asked: Mutex::new(Vec::new()),
    });
    let resp = shoot(relay_over(d), "/t/x/y/v1/messages", "", "{}");
    assert!(resp.starts_with("HTTP/1.1 429"), "{resp}");
    assert_eq!(got.lock().expect("lock").len(), 1);
}

// ── 换号：真中转 ＋ 生产段的上游选择（装上换号）＋ 按鉴权头作答的假上游；假凭据、临时家目录 ─────────────────

use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::upstream_select::rotate::{Hop, LibAccount, Library};

const UUID_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const UUID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
/// 起会话那个号的 claude 自己带的令牌（下游原样送来的）。
const AGENT_TOKEN: &str = "Bearer fake-token-of-a";
const B_TOKEN: &str = "fake-access-of-b";
const B_REFRESH: &str = "fake-refresh-of-b";

/// 判据里续令牌一律发到这里：本机一个刚关掉的口（连不上 ⇒ 续不上），绝不发到那一家的真端点。
fn dead_token_endpoint() -> crate::accounts::oauth::TokenEndpoint {
    let port = TcpListener::bind(SocketAddr::new(LOOPBACK, 0))
        .and_then(|l| l.local_addr())
        .expect("bind")
        .port();
    crate::accounts::oauth::TokenEndpoint {
        base: comms_outward::Base::parse(&format!("http://127.0.0.1:{port}")).expect("base"),
        rest: "/v1/oauth/token".into(),
        deadline: std::time::Duration::from_millis(2_000),
    }
}

fn request_body(uuid: &str) -> String {
    let user_id = format!(
        "{{\"device_id\":\"{}\",\"account_uuid\":\"{uuid}\",\"session_id\":\"s-1\"}}",
        "d".repeat(64)
    );
    serde_json::to_string(&serde_json::json!({
        "model": "m",
        "messages": [{"role": "user", "content": "你好"}],
        "metadata": {"user_id": user_id},
        "stream": true,
    }))
    .expect("json")
}

fn refused_with_quota() -> String {
    "HTTP/1.1 429 Too Many Requests\r\n\
     anthropic-ratelimit-unified-status: rejected\r\n\
     anthropic-ratelimit-unified-reset: 4000000000\r\n\
     anthropic-ratelimit-unified-representative-claim: five_hour\r\n\
     anthropic-ratelimit-unified-5h-utilization: 1.02\r\n\
     anthropic-ratelimit-unified-5h-reset: 4000000000\r\n\
     Content-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
        .to_string()
}

/// 一个家目录：账号 a（起会话的号）、b 两个订阅号，各有假凭据与假身份。
struct Home {
    root: std::path::PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = temp_dir(&format!("rot-{tag}"));
        for (id, uuid, token) in [("a", UUID_A, "fake-access-of-a"), ("b", UUID_B, B_TOKEN)] {
            let dir = root.join("accts").join(id);
            std::fs::create_dir_all(&dir).expect("mkdir");
            let far = (crate::accounts::quota::now_unix() + 3600) * 1000;
            std::fs::write(
                dir.join(".credentials.json"),
                format!(
                    "{{\"claudeAiOauth\":{{\"accessToken\":\"{token}\",\"refreshToken\":\"{B_REFRESH}\",\"expiresAt\":{far},\"scopes\":[\"user:inference\"]}}}}"
                ),
            )
            .expect("creds");
            std::fs::write(
                dir.join(".claude.json"),
                format!("{{\"oauthAccount\":{{\"accountUuid\":\"{uuid}\"}}}}"),
            )
            .expect("identity");
        }
        Self { root }
    }

    fn library(&self) -> crate::accounts::upstream_select::rotate::LibraryRead {
        let accts = self.root.join("accts");
        Arc::new(move |_| Library {
            enabled: true,
            accounts: ["a", "b"]
                .iter()
                .map(|id| LibAccount {
                    id: id.to_string(),
                    dir: Some(accts.join(id)),
                    api: false,
                })
                .collect(),
        })
    }

    fn rotation_path(&self) -> std::path::PathBuf {
        self.root.join(rotation::FILE_NAME)
    }

    fn set_default(&self, enabled: &[&str], when: serde_json::Value) {
        self.set_default_at(enabled, when, "continue");
    }

    fn set_default_at(&self, enabled: &[&str], when: serde_json::Value, at_limit: &str) {
        let mut order = vec![serde_json::json!({"start": true})];
        order.extend(["a", "b"].iter().map(|a| serde_json::json!(a)));
        let r = rotation::rotation_from(
            &serde_json::json!({"order": order, "enabled": enabled, "when": when, "atLimit": at_limit}),
            1..=1,
            &|a| a != "_",
            &|_| false,
            None,
        )
        .expect("rotation");
        rotation::face_change(&RotationStore::at(Some(self.rotation_path())), |b| {
            let id = b.default_rule.clone();
            if let Some(rule) = b.rules.get_mut(&id) {
                rule.rotation = r;
            }
        })
        .expect("write");
    }

    /// 起一个中转（一个后端进程那一份）：额度账 ＋ 轮换都落在这个家里。
    fn relay(&self, up: SocketAddr) -> SocketAddr {
        let quota = Arc::new(Ledger::at(Some(self.root.join(ledger::FILE_NAME))));
        let hop = Hop::new(
            Arc::new(RotationStore::at(Some(self.rotation_path()))),
            Arc::clone(&quota),
            Some(self.root.clone()),
            self.library(),
            Some(dead_token_endpoint()),
        );
        let accounts = Accounts::new(RoutingTable::build(std::iter::empty()), upstreams_at(up))
            .recording_to(quota)
            .rotating_with(hop);
        spawn_relay_over(accounts)
    }

    fn session(&self, sid: &str) -> rotation::SessionEntry {
        RotationStore::at(Some(self.rotation_path())).now().sessions[sid].clone()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn send_as_a(relay: SocketAddr) -> String {
    shoot(
        relay,
        "/t/claude-code/a/v1/messages",
        &format!("authorization: {AGENT_TOKEN}\r\nx-claude-code-session-id: s-1\r\n"),
        &request_body(UUID_A),
    )
}

/// a 被拒、b 接得住。
fn a_refused_b_serves(auth: Option<&str>) -> String {
    if auth == Some(&format!("Bearer {B_TOKEN}")) {
        sse_200("x-from: b\r\n")
    } else {
        refused_with_quota()
    }
}

/// ★ A 号 429 ＋ 额度头已拒、B 号 200 ⇒ 下游拿到 B 的 200；上游先后收到 A、B 两发，请求体只差身份格；
/// B 那一发的鉴权与身份格是 B 的；钉在 B、记一条（满了 · 5h · 原号几点重置）；同一会话下一发直接走 B。
#[test]
fn a_refused_account_is_swapped_for_the_next_one_without_the_agent_noticing() {
    let home = Home::new("swap");
    home.set_default(&["b"], serde_json::json!("full"));
    let (up, got) = spawn_judging_upstream(a_refused_b_serves);
    let relay = home.relay(up);
    let resp = send_as_a(relay);
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    assert!(resp.contains("x-from: b"));
    {
        let got = got.lock().expect("lock");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].auth.as_deref(), Some(AGENT_TOKEN));
        assert_eq!(got[0].body, request_body(UUID_A).into_bytes());
        assert_eq!(got[1].auth, Some(format!("Bearer {B_TOKEN}")));
        assert_eq!(got[1].body, request_body(UUID_B).into_bytes());
    }
    let s = home.session("s-1");
    assert_eq!((s.start.as_str(), s.current.as_str()), ("a", "b"));
    assert_eq!(
        s.history,
        [rotation::SwitchRecord {
            at_text: None,
            from_resets_at_text: None,
            at: s.since,
            from: "a".into(),
            to: "b".into(),
            why: rotation::SwitchWhy::Full {
                w: Some("5h".into())
            },
            from_resets_at: Some(4_000_000_000),
        }]
    );
    let resp = send_as_a(relay);
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    let got = got.lock().expect("lock");
    assert_eq!(got.len(), 3, "下一发直接走 B，不先撞一次 A");
    assert_eq!(got[2].auth, Some(format!("Bearer {B_TOKEN}")));
}

/// ★ 后端重启 / 会话 resume 之后，同一会话第一发就走钉着的号（钉号在盘上）。
#[test]
fn a_restarted_backend_sends_the_first_request_of_a_pinned_session_to_the_pinned_account() {
    let home = Home::new("resume");
    home.set_default(&["b"], serde_json::json!("full"));
    let (up, got) = spawn_judging_upstream(a_refused_b_serves);
    assert!(send_as_a(home.relay(up)).starts_with("HTTP/1.1 200"));
    let fresh = home.relay(up);
    assert!(send_as_a(fresh).starts_with("HTTP/1.1 200"));
    let got = got.lock().expect("lock");
    assert_eq!(got.len(), 3);
    assert_eq!(got[2].auth, Some(format!("Bearer {B_TOKEN}")));
}

/// 缺省池（只有起会话的号）⇒ 被拒原样交回、不换；池里都被拒 ⇒ 原样交回上游的拒绝，一发至多试池子大小次。
#[test]
fn nothing_to_swap_to_hands_down_the_refusal_as_is() {
    fn all_refused(_auth: Option<&str>) -> String {
        refused_with_quota()
    }
    let home = Home::new("none");
    let (up, got) = spawn_judging_upstream(all_refused);
    let relay = home.relay(up);
    let resp = send_as_a(relay);
    assert_eq!(resp, refused_with_quota());
    assert_eq!(got.lock().expect("lock").len(), 1);
    assert_eq!(home.session("s-1").current, "a");

    // 下一发：a 在额度账上被拒到重置时刻 ⇒ 直接走 b（不先撞 a），b 也拒 ⇒ 原样交回，不回到 a。
    home.set_default(&["b"], serde_json::json!("full"));
    assert_eq!(send_as_a(relay), refused_with_quota());
    assert_eq!(got.lock().expect("lock").len(), 2);

    // 一发之内：a 拒 ⇒ 换 b，b 也拒 ⇒ 原样交回；a、b 各一发，不打转。
    let fresh = Home::new("none-2");
    fresh.set_default(&["b"], serde_json::json!("full"));
    let (up, got) = spawn_judging_upstream(all_refused);
    assert_eq!(send_as_a(fresh.relay(up)), refused_with_quota());
    let auths: Vec<Option<String>> = got
        .lock()
        .expect("lock")
        .iter()
        .map(|g| g.auth.clone())
        .collect();
    assert_eq!(
        auths,
        [
            Some(AGENT_TOKEN.to_string()),
            Some(format!("Bearer {B_TOKEN}"))
        ]
    );
}

/// ★ 真换号与「下一个」同一处判：b 刚吃了一发不带限额头、也没说几点再试的 429（额度账上被拒着，短期限内）⇒
/// a 被拒时不换到 b，原样交回 a 的拒绝（上游只收到一发）。
#[test]
fn a_refused_account_is_not_swapped_onto_one_refused_without_a_reset() {
    let home = Home::new("brief");
    home.set_default(&["b"], serde_json::json!("full"));
    let now = crate::accounts::quota::now_unix();
    let side = Ledger::at(Some(home.root.join(ledger::FILE_NAME)));
    let r = crate::agents::claudecode::quota::read(429, &[], now).expect("被拒");
    ledger::record_seen(&side, "claude-code", "b", r, now);
    let (up, got) = spawn_judging_upstream(a_refused_b_serves);
    let resp = send_as_a(home.relay(up));
    assert_eq!(resp, refused_with_quota());
    assert_eq!(got.lock().expect("lock").len(), 1, "不先撞一次被拒着的 b");
    assert_eq!(home.session("s-1").current, "a");
}

/// ★ 阈值模式：200 回包里过了阈值 ⇒ 这一发照常交下去，下一发换到 b。
#[test]
fn threshold_mode_moves_the_next_request() {
    fn a_at_95(auth: Option<&str>) -> String {
        if auth == Some(AGENT_TOKEN) {
            sse_200(
                "anthropic-ratelimit-unified-status: allowed_warning\r\n\
                 anthropic-ratelimit-unified-representative-claim: five_hour\r\n\
                 anthropic-ratelimit-unified-5h-utilization: 0.95\r\n\
                 anthropic-ratelimit-unified-5h-reset: 4000000000\r\n",
            )
        } else {
            sse_200("x-from: b\r\n")
        }
    }
    let home = Home::new("threshold");
    home.set_default(&["b"], serde_json::json!({"threshold": {"n": 90}}));
    let (up, got) = spawn_judging_upstream(a_at_95);
    let relay = home.relay(up);
    assert!(!send_as_a(relay).contains("x-from: b"));
    assert!(send_as_a(relay).contains("x-from: b"));
    assert_eq!(got.lock().expect("lock").len(), 2);
    let s = home.session("s-1");
    assert_eq!(s.history[0].why, rotation::SwitchWhy::Threshold { n: 90 });
}

/// b 那个窗口几点重置（比 a 的早 ⇒ 池里最早回到阈值以下的是 b）。
const B_BACK: u64 = 3_999_999_000;

/// a 用到 95%；b 用到 92%（或被拒，`b_refused`）；两个都过了 90%。
fn both_past_ninety(auth: Option<&str>, b_refused: bool) -> String {
    if auth == Some(AGENT_TOKEN) {
        return sse_200(
            "anthropic-ratelimit-unified-status: allowed_warning\r\n\
             anthropic-ratelimit-unified-representative-claim: five_hour\r\n\
             anthropic-ratelimit-unified-5h-utilization: 0.95\r\n\
             anthropic-ratelimit-unified-5h-reset: 4000000000\r\n",
        );
    }
    if b_refused {
        return format!(
            "HTTP/1.1 429 Too Many Requests\r\n\
             anthropic-ratelimit-unified-status: rejected\r\n\
             anthropic-ratelimit-unified-reset: {B_BACK}\r\n\
             anthropic-ratelimit-unified-representative-claim: five_hour\r\n\
             anthropic-ratelimit-unified-5h-utilization: 1.0\r\n\
             anthropic-ratelimit-unified-5h-reset: {B_BACK}\r\n\
             Content-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        );
    }
    sse_200(&format!(
        "x-from: b\r\n\
         anthropic-ratelimit-unified-status: allowed_warning\r\n\
         anthropic-ratelimit-unified-representative-claim: five_hour\r\n\
         anthropic-ratelimit-unified-5h-utilization: 0.92\r\n\
         anthropic-ratelimit-unified-5h-reset: {B_BACK}\r\n"
    ))
}

/// 下游拿到的是不是我们回的那份「用满」回包（重置时刻 `at`）。
fn is_held_reply(resp: &str, at: u64) -> bool {
    resp.starts_with("HTTP/1.1 429 Too Many Requests\r\n")
        && resp.contains("anthropic-ratelimit-unified-status: rejected\r\n")
        && resp.contains(&format!("anthropic-ratelimit-unified-reset: {at}\r\n"))
        && resp.contains("anthropic-ratelimit-unified-representative-claim: five_hour\r\n")
        && resp.contains("\r\nretry-after: ")
        && resp.contains("X-Cc-Monitor-Reason: at-limit\r\n")
        && resp.contains("\"rate_limit_error\"")
}

/// ★ 硬上限：池里两个号都过了 90% ⇒ 第三发不发上游，回 claude 自己认得的「用满」回包，重置时刻 ＝ 池里最早回到 90% 以下的那一刻（b 的）；
/// 会话记一条「卡住」；我们回的那份不进额度账（b 在账上仍是没被拒的 92%）。软阈值同一情形 ⇒ 第三发照发。
#[test]
fn a_hard_limit_answers_the_agent_with_its_own_limit_reply_instead_of_sending() {
    fn answer(auth: Option<&str>) -> String {
        both_past_ninety(auth, false)
    }
    let home = Home::new("hold");
    home.set_default_at(&["b"], serde_json::json!({"threshold": {"n": 90}}), "stop");
    let (up, got) = spawn_judging_upstream(answer);
    let relay = home.relay(up);
    assert!(!send_as_a(relay).contains("x-from: b"));
    assert!(send_as_a(relay).contains("x-from: b"), "第二发换到 b");
    let resp = send_as_a(relay);
    assert!(is_held_reply(&resp, B_BACK), "{resp}");
    assert_eq!(got.lock().expect("lock").len(), 2, "第三发不该到上游");
    let s = home.session("s-1");
    let last = s.history.last().expect("记了一条");
    assert_eq!(
        (last.why.clone(), last.from_resets_at, last.from == last.to),
        (rotation::SwitchWhy::Held { n: 90 }, Some(B_BACK), true)
    );
    let quota = Ledger::at(Some(home.root.join(ledger::FILE_NAME)));
    assert!(
        !quota
            .entry("claude-code", "b")
            .expect("b 有账")
            .reading
            .refused,
        "我们回的那份进了额度账"
    );

    let soft = Home::new("hold-soft");
    soft.set_default(&["b"], serde_json::json!({"threshold": {"n": 90}}));
    let (up, got) = spawn_judging_upstream(answer);
    let relay = soft.relay(up);
    for _ in 0..3 {
        send_as_a(relay);
    }
    assert_eq!(got.lock().expect("lock").len(), 3, "软阈值：第三发照发");
}

/// ★ 硬上限、换过去的号当场被拒（重发那一路）⇒ 不把上游那份拒绝交下去，回我们那份（重置时刻 ＝ 池里最早回到 90% 以下的那一刻）。
#[test]
fn a_hard_limit_also_holds_when_the_resend_is_refused() {
    fn answer(auth: Option<&str>) -> String {
        both_past_ninety(auth, true)
    }
    let home = Home::new("hold-refused");
    home.set_default_at(&["b"], serde_json::json!({"threshold": {"n": 90}}), "stop");
    let (up, got) = spawn_judging_upstream(answer);
    let relay = home.relay(up);
    send_as_a(relay);
    let resp = send_as_a(relay);
    assert!(is_held_reply(&resp, B_BACK), "{resp}");
    assert_eq!(got.lock().expect("lock").len(), 2);
}

/// 「到上限」此刻实际照哪一档办：给得出「用满」回包的那一家照说的办；给不出的那一家 `stop` 按 `continue`。
#[test]
fn stop_holds_only_for_an_agent_that_has_a_limit_reply() {
    use crate::accounts::quota::rotation::AtLimit;
    use crate::accounts::upstream_select::rotate::at_limit_in_effect;
    assert_eq!(
        at_limit_in_effect("claude-code", AtLimit::Stop),
        AtLimit::Stop
    );
    assert_eq!(
        at_limit_in_effect("no-such-agent", AtLimit::Stop),
        AtLimit::Continue
    );
    assert_eq!(
        at_limit_in_effect("claude-code", AtLimit::Continue),
        AtLimit::Continue
    );
}

/// ★ 超额在兜（200、限流器说已拒、超额在用）且轮换里还有未满的订阅号 ⇒ 下一发换。
#[test]
fn overage_in_use_moves_the_next_request_to_a_subscription_with_room() {
    fn a_on_overage(auth: Option<&str>) -> String {
        if auth == Some(AGENT_TOKEN) {
            sse_200(
                "anthropic-ratelimit-unified-status: rejected\r\n\
                 anthropic-ratelimit-unified-reset: 4000000000\r\n\
                 anthropic-ratelimit-unified-representative-claim: five_hour\r\n\
                 anthropic-ratelimit-unified-overage-status: allowed\r\n\
                 anthropic-ratelimit-unified-overage-in-use: true\r\n",
            )
        } else {
            sse_200("x-from: b\r\n")
        }
    }
    let home = Home::new("overage");
    home.set_default(&["b"], serde_json::json!("full"));
    let (up, _got) = spawn_judging_upstream(a_on_overage);
    let relay = home.relay(up);
    assert!(!send_as_a(relay).contains("x-from: b"));
    assert!(send_as_a(relay).contains("x-from: b"));
}

/// ★ 凭据不外漏：换过号之后盘上的轮换配置、额度账、下游拿到的回包里一处令牌 / 刷新令牌都没有。
#[test]
fn no_token_lands_in_the_books_or_the_answer() {
    let home = Home::new("leak");
    home.set_default(&["b"], serde_json::json!("full"));
    let (up, _got) = spawn_judging_upstream(a_refused_b_serves);
    let resp = send_as_a(home.relay(up));
    let books = [
        std::fs::read_to_string(home.rotation_path()).expect("rotation"),
        std::fs::read_to_string(home.root.join(ledger::FILE_NAME)).expect("quota"),
        resp,
    ];
    assert!(books[0].contains("\"current\":\"b\""), "正控：换过号了");
    for text in &books {
        for secret in [B_TOKEN, B_REFRESH, "fake-access-of-a", "fake-token-of-a"] {
            assert!(!text.contains(secret), "{secret} 漏进了 {text}");
        }
    }
}

/// ★ 测试档不许出网：对那一家真令牌端点续一次令牌 ⇒ 出网那一道当场 panic（不连、不解析名字），续期拿不到任何回包。
#[test]
fn in_tests_a_request_to_a_real_host_panics_before_any_connection() {
    let face = crate::agents::login_of("claude-code").expect("登记了");
    let ep =
        crate::accounts::oauth::TokenEndpoint::of(&face, std::time::Duration::from_millis(2_000))
            .expect("真端点");
    assert_eq!(
        ep.base.host, "platform.claude.com",
        "正控：量的就是那一家真端点"
    );
    let hit = std::panic::catch_unwind(|| {
        comms_outward::fetch(&ep.base, "POST", &ep.rest, &[], b"{}", ep.deadline, 1024)
    });
    let why = hit.expect_err("应当 panic");
    let said = why.downcast_ref::<String>().cloned().unwrap_or_default();
    assert_eq!(said, "测试档不许出网：platform.claude.com");
}

/// ★ 被拒那一行日志：号名 · 状态码 · 有没有限额头 · 代表窗 · retry-after；两种 429 各一形；只用这几格 ⇒ 令牌与回包体无从进来。
#[test]
fn a_refusal_logs_one_line_of_who_and_how() {
    use crate::accounts::upstream_select::refusal_line;
    let h = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    };
    let now = 1_800_000_000;
    let bare = crate::agents::claudecode::quota::read(429, &h(&[]), now).expect("被拒");
    assert_eq!(
        refusal_line("y", 429, &bare, None),
        "[quota] 被拒：号 y · 状态码 429 · 限额头 无 · 代表窗 — · retry-after —"
    );
    let full = crate::agents::claudecode::quota::read(
        429,
        &h(&[
            ("anthropic-ratelimit-unified-status", "rejected"),
            (
                "anthropic-ratelimit-unified-representative-claim",
                "five_hour",
            ),
            ("anthropic-ratelimit-unified-reset", "1800003600"),
        ]),
        now,
    )
    .expect("被拒");
    assert_eq!(
        refusal_line("z", 429, &full, Some("3600")),
        "[quota] 被拒：号 z · 状态码 429 · 限额头 有 · 代表窗 five_hour · retry-after 3600"
    );
}
