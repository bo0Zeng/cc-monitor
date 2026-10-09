//! 中转对「只许直通的那把钥匙」与「先试协议升级、被拒再发普通请求」那一形的组合判据：
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
        super::key::key_tests::test_keys(),
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

/// ★ 只许直通的那把钥匙：打 `/t/` 过门、到上游（钥匙段剥掉）；打 `/s/` ⇒ 403 `key-scope`、一个字节不到上游。
/// 全权那一把打 `/s/` 过得了门（表里没这一行 ⇒ 404，与 403 可分）。
#[test]
fn the_pass_key_reaches_passthrough_but_is_refused_on_substitute_routes() {
    let up = spawn_name_upstream(SSE_OK);
    let relay = spawn_relay_with(all_to(up.addr), Arc::new(Taps::default()));
    let pass = super::key::key_tests::TEST_PASS_KEY;
    let full = super::key::key_tests::TEST_KEY;
    let post = |key: &str, route: &str| {
        send_raw(
            relay,
            &format!("POST /{key}/{route}/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{{}}"),
        )
    };
    let got = post(pass, "t/claude-code/0");
    assert!(
        got.starts_with("HTTP/1.1 200"),
        "直通钥匙打 /t/ 没过：{got:?}"
    );
    let got = post(pass, "s/claude-code/acct");
    assert!(
        got.starts_with("HTTP/1.1 403 ")
            && got
                .to_ascii_lowercase()
                .contains("x-cc-monitor-reason: key-scope"),
        "直通钥匙打 /s/ 没按「钥匙管不到」拒：{got:?}"
    );
    let got = post(full, "s/claude-code/acct");
    assert!(
        got.starts_with("HTTP/1.1 404 "),
        "全权钥匙打 /s/ 该过门、落到没这一行：{got:?}"
    );
    let seen = up.seen.lock().expect("lock").clone();
    assert_eq!(seen.len(), 1, "只有 /t/ 那一发该到上游：{seen:?}");
    assert_eq!(seen[0].line, "POST /v1/messages HTTP/1.1");
}

/// 一条 Responses 流（一轮：开始 · 一块正文 · 一段字 · 说完）。开头与收尾那两件照真流的形带整份应答对象
/// （含整段系统提示，真读数 18 KB 量级 ⇒ 超 tee 的单件上限；正文换成占位）。
fn responses_ok() -> &'static str {
    let pad = "x".repeat(20 * 1024);
    Box::leak(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n\
data: {{\"type\":\"response.created\",\"sequence_number\":0,\"response\":{{\"id\":\"resp_1\",\"object\":\"response\",\"status\":\"in_progress\",\"instructions\":\"{pad}\",\"tools\":[]}}}}\n\n\
data: {{\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{{\"type\":\"message\"}}}}\n\n\
data: {{\"type\":\"response.output_text.delta\",\"output_index\":0,\"delta\":\"ok\"}}\n\n\
data: {{\"type\":\"response.completed\",\"sequence_number\":4,\"response\":{{\"id\":\"resp_1\",\"status\":\"completed\",\"instructions\":\"{pad}\"}}}}\n\n"
    ).into_boxed_str())
}

/// ★★ Codex 那一家照真请求的形发（地址里带的是只许直通的那把钥匙）：
/// ① 先一发 WebSocket 升级 ⇒ 426、一个字节不到上游；② 改发 `POST …/responses` ⇒ 带 `ChatGPT-Account-ID` 的落 ChatGPT 那一支、
/// 不带的落 API 那一支（反向：API 形的那一发不许落到 ChatGPT 那一支）；③ tee 出的流标签 ＝ `session-id` 的值，
/// 主运行（`thread-id` ＝ `session-id`）归主运行、子 agent（`thread-id` 是它自己的）归它，流折得出开始 · 字 · 收尾（活卡要的那几件）——
/// 开头那一件超 tee 单件上限（截断形）也照样认得出开始。
/// 真上游换成两个假上游（路径前缀照真上游的形）。
#[test]
fn a_codex_shaped_round_gets_426_then_picks_its_upstream_by_login_form_and_routes_its_runs() {
    use crate::accounts::upstream_select::UpstreamPick;
    use crate::agents::StreamEv;
    use crate::stream::wire::Frame;
    let chatgpt = spawn_name_upstream(responses_ok());
    let api = spawn_name_upstream(responses_ok());
    let base = |up: &NameUpstream, path: &str| {
        comms_outward::Base::parse(&format!("http://127.0.0.1:{}{path}", up.addr.port())).unwrap()
    };
    let ups = crate::accounts::upstream_select::upstreams_testing::with_pick(
        Upstreams::from_env(&|_| None).unwrap(),
        "codex",
        UpstreamPick::ByHeader {
            header: "ChatGPT-Account-ID",
            present: base(&chatgpt, "/backend-api/codex"),
            absent: base(&api, "/v1"),
        },
    );
    let taps = Arc::new(Taps::default());
    let relay = spawn_relay_with(ups, Arc::clone(&taps));
    let key = super::key::key_tests::TEST_PASS_KEY;
    const SID: &str = "019a0000-0000-7000-8000-00000000c0de";
    const CHILD: &str = "019a0000-0000-7000-8000-0000000c41d0";

    let ws = send_raw(
        relay,
        &format!("GET /{key}/t/codex/_/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nsession-id: {SID}\r\nthread-id: {SID}\r\n\r\n"),
    );
    assert!(
        ws.starts_with("HTTP/1.1 426 "),
        "升级那一发没回 426：{ws:?}"
    );
    assert!(
        chatgpt.seen.lock().unwrap().is_empty() && api.seen.lock().unwrap().is_empty(),
        "升级那一发到了上游"
    );

    let post = |thread: &str, chatgpt_form: bool| {
        let account = if chatgpt_form {
            "ChatGPT-Account-ID: acct\r\n"
        } else {
            ""
        };
        send_raw(
            relay,
            &format!("POST /{key}/t/codex/_/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer x\r\n{account}session-id: {SID}\r\nthread-id: {thread}\r\nContent-Length: 2\r\n\r\n{{}}"),
        )
    };
    assert!(post(SID, true).starts_with("HTTP/1.1 200"));
    assert!(post(CHILD, true).starts_with("HTTP/1.1 200"));
    assert!(post(SID, false).starts_with("HTTP/1.1 200"));
    let lines = |u: &NameUpstream| -> Vec<String> {
        u.seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.line.clone())
            .collect()
    };
    assert_eq!(
        lines(&chatgpt),
        vec!["POST /backend-api/codex/responses HTTP/1.1"; 2],
        "ChatGPT 形的两发"
    );
    assert_eq!(
        lines(&api),
        vec!["POST /v1/responses HTTP/1.1"],
        "API 形那一发"
    );

    // tee 那一侧：三段流，每段 4 件数据 ＋ 收尾；标签 ＝ session-id 的值。
    let mut events = Vec::new();
    for _ in 0..200 {
        events = taps.0.lock().unwrap().clone();
        if events.len() >= 15 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(events.len(), 15, "tee 件数不对：{}", events.len());
    assert!(
        events.iter().all(|e| e.stream == SID),
        "流标签不是 session-id 的值"
    );
    let mut router = crate::stream::run_route::RunRouter::new(
        Arc::new(crate::observe::runs::RunBook::default()),
        crate::agents::stream_families(),
    );
    let mut by_resp: std::collections::BTreeMap<u64, (Option<String>, Vec<StreamEv>)> =
        Default::default();
    for e in events {
        for f in router.on_tap(e) {
            if let Frame::Tap { run, resp, ev, .. } = f {
                let slot = by_resp.entry(resp).or_insert((run.clone(), Vec::new()));
                assert_eq!(slot.0, run, "同一段流归了两个运行");
                slot.1.extend(ev);
            }
        }
    }
    let runs: Vec<Option<String>> = by_resp.values().map(|(r, _)| r.clone()).collect();
    assert_eq!(
        runs,
        vec![None, Some(CHILD.to_string()), None],
        "主运行 / 子 agent 归位不对"
    );
    for (_, evs) in by_resp.values() {
        assert_eq!(
            evs,
            &vec![
                StreamEv::Start {
                    rid: "resp_1".into()
                },
                StreamEv::Block {
                    i: 0,
                    kind: crate::agents::BlockKind::Text,
                    tool: None
                },
                StreamEv::Text {
                    i: 0,
                    s: "ok".into()
                },
                StreamEv::Stop { ok: true },
            ]
        );
    }
}
