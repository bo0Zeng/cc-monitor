//! 中转逐块转发多出来的时延：假上游逐块吐 SSE，经生产的绑口 · 接受循环 · 期限值（`listen.rs`）与真中转（`comms_outward`）
//! 到客户端，量每一块「客户端收到 − 上游吐出」，与请求那一向「上游收齐请求头 − 客户端发出」。
//!
//! 块与块之间由判据隔开：上游每吐一段（头 · 事件 · 终止块）之前都等判据放行，判据等客户端收到上一段才放行下一段；
//! 吐完终止块，上游再等一次放行才关连接。
//! 判什么：每一段到客户端的差都在兜底的那一个数（`wall::CEILING`，客户端读期限）之内。中转攒住任何一段
//! （连终止块：攒到上游关连接才放，而上游要等终止块到了才关）⇒ 客户端再也等不到它 ⇒ 读期限到 ⇒ 红。
//! 上限不取一个更紧的毫秒数：机器忙时回环一跳要多久说不准，按墙钟卡紧了忙机就误红（墙钟判据的规矩）；
//! 每段的差与请求那一向印成一行 `[中转时延]`，只印不判，数进验收记录。
//!
//! 上游选择是判据自己的一只手（一律直通到假上游）；不连真上游、不读凭据。

use crate::relay::test_support::{
    listen, serve, test_keys, DOWNSTREAM_DEADLINE, INFLIGHT_BODY_CAP, TEST_KEY, UPSTREAM_DEADLINE,
};
use comms_outward::{
    Ask, Base, Destination, Destinations, Mode, Relay, RouteKey, TapEvent, TapPort, TeeSink,
};
use guard_core::wall::CEILING;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// 事件块几块（头与终止块另算）。
const EVENTS: usize = 4;
/// 一共几段：头 ＋ 事件 ＋ 终止块。
const PARTS: usize = EVENTS + 2;

/// 一律直通到假上游。
struct ToFake(Base);
impl Destinations for ToFake {
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
    fn upstream_idle_reuse(&self) -> Duration {
        CEILING
    }
}

struct NoTap;
impl TapPort for NoTap {
    fn offer(&self, _ev: TapEvent) -> bool {
        true
    }
}

/// 上游这一侧记下的时刻：收齐请求头那一刻 · 每一段吐出之前那一刻。
#[derive(Default)]
struct Emitted {
    request_in: Option<Instant>,
    parts: Vec<Instant>,
}

const HEAD: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
const LAST: &[u8] = b"0\r\n\r\n";

/// 第 `i` 个事件块（chunked 编码），载荷里带 `{"i":i}` 当记号。
fn event_chunk(i: usize) -> Vec<u8> {
    let data = format!("event: tick\ndata: {{\"i\":{i}}}\n\n");
    format!("{:x}\r\n{data}\r\n", data.len()).into_bytes()
}

/// 起假上游：接一条连接，读齐请求；每吐一段之前等一次放行，吐完终止块再等一次放行才关。
fn spawn_fake_upstream(log: Arc<Mutex<Emitted>>, go: mpsc::Receiver<()>) -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind 假上游");
    let addr = l.local_addr().expect("假上游地址");
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().expect("假上游 accept");
        s.set_nodelay(true).expect("nodelay");
        s.set_read_timeout(Some(CEILING)).expect("读期限");
        // 读齐请求头与请求体（体按 Content-Length）：不读体的话收尾是 RST 不是 FIN。
        let mut got = Vec::new();
        let mut one = [0u8; 4096];
        let head_end = loop {
            let n = s.read(&mut one).expect("读请求");
            assert!(n > 0, "请求没读齐就断了");
            got.extend_from_slice(&one[..n]);
            if let Some(p) = got.windows(4).position(|w| w == b"\r\n\r\n") {
                break p + 4;
            }
        };
        log.lock().expect("lock").request_in = Some(Instant::now());
        let head = String::from_utf8_lossy(&got[..head_end]).to_ascii_lowercase();
        let len: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .map_or(0, |v| v.trim().parse().expect("Content-Length"));
        while got.len() < head_end + len {
            let n = s.read(&mut one).expect("读请求体");
            assert!(n > 0, "请求体没读齐就断了");
            got.extend_from_slice(&one[..n]);
        }
        let mut parts: Vec<Vec<u8>> = vec![HEAD.to_vec()];
        parts.extend((1..=EVENTS).map(event_chunk));
        parts.push(LAST.to_vec());
        // 放行不带期限：上游自己等到期限就关连接的话，攒到上游关才放的中转会在那一刻把攒着的一起放出去，
        // 跟客户端的读期限赛跑。判据收场（连 panic）时发端丢掉 ⇒ `recv` 出错 ⇒ 这条线程跟着收。
        for part in &parts {
            if go.recv().is_err() {
                return;
            }
            log.lock().expect("lock").parts.push(Instant::now());
            s.write_all(part).expect("上游写");
            s.flush().expect("上游 flush");
        }
        let _ = go.recv();
    });
    addr
}

/// 起中转：生产的绑口与接受循环、生产的两个期限值与体量上界、生产的开上游那一口（这台的出网描述，直连回环）。
fn spawn_relay(up: SocketAddr) -> SocketAddr {
    let base = crate::relay::base_of(&format!("http://127.0.0.1:{}", up.port())).expect("base");
    let relay = Arc::new(Relay::new(
        Arc::new(ToFake(base)),
        test_keys(),
        TeeSink::to_port(Arc::new(NoTap)),
        crate::relay::open_for(Arc::new(|_| None)),
        DOWNSTREAM_DEADLINE,
        UPSTREAM_DEADLINE,
        INFLIGHT_BODY_CAP,
    ));
    let listener = listen(0).expect("listen");
    let addr = listener.local_addr().expect("中转地址");
    std::thread::spawn(move || serve(listener, relay, Arc::new(AtomicUsize::new(0))));
    addr
}

/// 第 `k` 段到了没有：0 ＝ 响应头收尾 · 1..=EVENTS ＝ 那个事件 · EVENTS+1 ＝ 终止块。
fn landed(acc: &[u8], k: usize) -> bool {
    let has = |m: &[u8]| acc.windows(m.len()).any(|w| w == m);
    match k {
        0 => has(b"\r\n\r\n"),
        k if k <= EVENTS => has(format!("{{\"i\":{k}}}").as_bytes()),
        _ => has(format!("{{\"i\":{EVENTS}}}").as_bytes()) && acc.ends_with(LAST),
    }
}

#[test]
fn every_chunk_reaches_the_client_before_upstream_is_let_to_emit_the_next() {
    let log = Arc::new(Mutex::new(Emitted::default()));
    let (go, gate) = mpsc::channel();
    let up = spawn_fake_upstream(Arc::clone(&log), gate);
    let relay = spawn_relay(up);

    let mut c = TcpStream::connect(relay).expect("连中转");
    c.set_nodelay(true).expect("nodelay");
    c.set_read_timeout(Some(CEILING)).expect("读期限");
    let body = r#"{"stream":true}"#;
    let req = format!(
        "POST /{TEST_KEY}/t/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let sent = Instant::now();
    c.write_all(req.as_bytes()).expect("写请求");

    let mut arrived: Vec<Instant> = Vec::new();
    let mut acc = Vec::new();
    let mut buf = [0u8; 4096];
    for k in 0..PARTS {
        go.send(()).expect("放行上游吐下一段");
        while !landed(&acc, k) {
            let n = c.read(&mut buf).unwrap_or_else(|e| {
                panic!(
                    "第 {k} 段（0 ＝ 头，{} ＝ 终止块）上游吐出之后一直没到客户端（{e}）⇒ 中转攒住了它",
                    EVENTS + 1
                )
            });
            assert!(
                n > 0,
                "第 {k} 段没到就断了：{:?}",
                String::from_utf8_lossy(&acc)
            );
            acc.extend_from_slice(&buf[..n]);
        }
        arrived.push(Instant::now());
    }
    // 放上游关连接。
    go.send(()).expect("放行上游收尾");

    let log = log.lock().expect("lock");
    let request_in = log.request_in.expect("上游收到了请求");
    // 非空对照：上游真的一段一段吐了 PARTS 段。
    assert_eq!(
        log.parts.len(),
        PARTS,
        "夹具自检：上游吐了头 ＋ {EVENTS} 个事件 ＋ 终止块"
    );
    let lags: Vec<Duration> = log
        .parts
        .iter()
        .zip(&arrived)
        .map(|(emit, got)| got.saturating_duration_since(*emit))
        .collect();
    let mut chunks = lags[1..].to_vec();
    chunks.sort();
    println!(
        "[中转时延] 请求 {:?} · 头 {:?} · 各块 {:?} · 块中位 {:?} · 块最大 {:?}",
        request_in.saturating_duration_since(sent),
        lags[0],
        &lags[1..],
        chunks[chunks.len() / 2],
        chunks[chunks.len() - 1],
    );
}
