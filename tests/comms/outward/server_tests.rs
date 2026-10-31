//! 中转一来一回里**不需要后端**的那几格：回包头改写 · 下游期限装上之后半开连接的读会返回。
//! 走真中转 ＋ 生产段上游选择 / tap / 内存探针的组合判据住后端 `tests/backend/relay/server_tests.rs`。

use super::*;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::mpsc;

/// 判据里的假对端只听回环。
const LOOPBACK: std::net::IpAddr = std::net::IpAddr::V4(Ipv4Addr::LOCALHOST);

#[test]
fn response_head_keeps_framing_and_forces_close() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n";
    let out = String::from_utf8(rewrite_response_head(raw)).expect("utf8");
    assert!(out.contains("Transfer-Encoding: chunked\r\n"), "分帧不能丢");
    assert!(!out.contains("keep-alive"));
    assert_eq!(out.matches("Connection:").count(), 1);
    assert!(out.ends_with("\r\n\r\n"));
}

/// ★★ 读写期限的行为格㈡：**机制那一半**。
///
/// 它买两样，都是行为：
/// 1. socket 上有读期限时，`read_head` 面对一条**半开**连接（只发半个请求头、
///    **不关**连接）会**报错返回**，而不是永久挂住；
/// 2. 那条错误**没有被任何一层当成「再试一次」** —— `read_head` 直接把它交出来。
///    ⭐ 这第 2 条是最要紧的一格：一旦哪层重试，期限就从「阻塞有上限」
///    退化成「轮询」，而轮询正是零定时器护栏要防的东西。
///
/// # 为什么这里用**测试自己选的** 200ms，而不是生产那 30 秒
///
/// 端到端等满 `DOWNSTREAM_DEADLINE` 要跑 30 秒。⇒ 分工：
/// **「那个数装上了没有」**由上一条（`both_peers_really_carry_…`）用 `getsockopt` 买；
/// **「装上之后读会不会返回」**由这一条用一条短得多的同类期限买。
/// 测试段不受零定时器护栏管（`production_code()` 剥掉 `#[cfg(test)]`），所以这里能自由选值。
///
/// # 非空对照
///
/// 同一趟里再跑一条**发全了请求头**的连接：它必须 `Ok(Some(..))` 且**明显快过**那条期限
/// —— 否则「报错返回」只说明这把尺子把什么都判成超时。
#[test]
fn a_socket_deadline_makes_a_half_open_read_return_instead_of_wedging_the_thread() {
    let deadline = std::time::Duration::from_millis(200);
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind");
    let a = l.local_addr().expect("addr");

    // ── ㈠ 半开：只发半个请求头（**没有**结尾空行），且**不关**连接。
    let mut half = TcpStream::connect(a).expect("connect 半开");
    half.write_all(b"POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .expect("write 半个头");
    half.flush().expect("flush");
    let (mut srv, _p) = l.accept().expect("accept 半开");
    srv.set_read_timeout(Some(deadline)).expect("装期限");
    // ★★ 风险 `5x`：这条读**一旦有人重试就会无限自旋**，而自旋的表现是**挂住不是红**
    //    —— 那一屏与「跑完了、没有新红」几乎分不开（判定行会整条消失）。
    //    ⇒ 把读搬到一条工作线程上，用 `recv_timeout` 给它一个**远宽于**期限的上界（兜底的那一个数），
    //      超了就 `panic!` ⇒ **把挂住换成红**。这条纪律本文件 `送 5x` 那几处已经在用。
    //    〔`MU5` 实测：把 `http1::read_head` 的读循环改成「`WouldBlock` 也 `continue`」
    //      —— 没有这一层的话它整条判据挂死，有了这一层它**红**。〕
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let r = http1::read_head(&mut srv, HEAD_CAP);
        let _ = tx.send((r, t0.elapsed()));
    });
    let (r, waited) = rx
        .recv_timeout(guard_core::wall::CEILING)
        .expect("`read_head` 在兜底上限之内一个字都没返回 —— 期限没起作用，或者有哪一层在**重试**（那就成了轮询）");
    let e = r.expect_err("半开连接上的 `read_head` 必须**报错返回**；挂住的话本判据根本跑不完");
    assert!(
        matches!(
            e.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ),
        "报的该是期限到了那一族（Linux 上是 WouldBlock/EAGAIN，Windows 上是 TimedOut），\
             实测拿到的是 {:?}：{e}",
        e.kind()
    );
    // 它是**等满了期限**才返回的，不是立刻被别的错误（如 RST）弹回来的。
    assert!(
        waited >= deadline,
        "只等了 {waited:?} 就返回了（期限 {deadline:?}）—— 那不是期限在起作用，是别的错误"
    );
    drop(half);

    // ── ㈡ 非空对照：同一把尺子，一条**发全了**的连接必须走通、不等期限。
    //    期限用兜底的那一个数：数据已经在那儿了，「等满期限才回」就是等满五分钟 ⇒ 红；
    //    机器忙时立刻返回那一下也慢不到它（比 200 ms 的话，负载高时会误红）。
    let mut whole = TcpStream::connect(a).expect("connect 完整");
    whole
        .write_all(b"POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write 完整头");
    whole.flush().expect("flush");
    let (mut srv2, _p2) = l.accept().expect("accept 完整");
    srv2.set_read_timeout(Some(guard_core::wall::CEILING))
        .expect("装期限");
    let t1 = std::time::Instant::now();
    let got = http1::read_head(&mut srv2, HEAD_CAP)
        .expect("完整的请求头不该报错")
        .expect("完整的请求头该读得出来");
    let fast = t1.elapsed();
    assert!(
        got.ends_with(b"\r\n\r\n"),
        "读出来的该是一整个请求头：{:?}",
        String::from_utf8_lossy(&got)
    );
    assert!(
        fast < guard_core::wall::CEILING,
        "非空对照：数据已经在那儿了，`read_head` 该**立刻**返回而不是等满期限（实测 {fast:?}）\
             —— 等满了说明这把尺子把正常流量也判成了超时"
    );
    drop(whole);
}

// ══ 上游那一跳复用连接 ═══════════════════════════════════════════════════
//
// TLS 假上游（沙箱 CA 签的 `localhost` 叶子，`upstream::tests::sandbox_server_config`）数它接下了几条 TCP 连接、收到了哪几份请求体。
// 中转经 `serve_one` 走全程：进门 · 路由 · 上游选择（判据自己的一张：去处标签就是路由第二段，即「哪个号」）· 连上游 · 逐块透传。

/// 全权钥匙与直通钥匙（形状照 `relay_route_core::key_shape_ok`：64 位小写十六进制）。
const POOL_FULL_KEY: &str = "7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57";
const POOL_PASS_KEY: &str = "ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12";

/// 假上游每一发怎么答。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// chunked 的 SSE，答完连接留着。
    KeepChunked,
    /// `Content-Length` 定长，答完连接留着。
    KeepLength,
    /// 应答头里说 `Connection: close`，但连接**不关**（中转要自己不再用它，否则下一发挂住）。
    SaysCloseButStaysOpen,
    /// 答完发 close_notify 再关掉（两发之间对端走了）。
    CloseAfter,
}

struct TlsStub {
    port: u16,
    accepted: Arc<std::sync::atomic::AtomicUsize>,
    bodies: Arc<std::sync::Mutex<Vec<String>>>,
}

fn spawn_tls_stub(answer: Answer) -> TlsStub {
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 假上游");
    let port = l.local_addr().expect("addr").port();
    let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (n, b) = (Arc::clone(&accepted), Arc::clone(&bodies));
    let cfg = upstream::tests::sandbox_server_config();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(tcp) = s else { continue };
            n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (b, cfg) = (Arc::clone(&b), Arc::clone(&cfg));
            std::thread::spawn(move || {
                let _ = tcp.set_read_timeout(Some(guard_core::wall::CEILING));
                let conn = rustls::ServerConnection::new(cfg).expect("服务端连接");
                let mut tls = rustls::StreamOwned::new(conn, tcp);
                loop {
                    let Ok(Some(raw)) = http1::read_head(&mut tls, HEAD_CAP) else {
                        return;
                    };
                    let head = http1::parse_request(&raw).expect("假上游收到的该是 HTTP 请求");
                    let len = match head.content_length() {
                        http1::BodyLen::Exact(n) => n,
                        _ => 0,
                    };
                    let mut body = vec![0u8; len];
                    if tls.read_exact(&mut body).is_err() {
                        return;
                    }
                    let text = String::from_utf8_lossy(&body).into_owned();
                    b.lock().expect("lock").push(text.clone());
                    let sse = format!("data: {{\"echo\":\"{text}\"}}\n\n");
                    let reply = match answer {
                        Answer::KeepChunked | Answer::CloseAfter => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{sse}\r\n0\r\n\r\n",
                            sse.len()
                        ),
                        Answer::KeepLength => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{sse}",
                            sse.len()
                        ),
                        Answer::SaysCloseButStaysOpen => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
                            sse.len()
                        ),
                    };
                    if tls
                        .write_all(reply.as_bytes())
                        .and_then(|()| tls.flush())
                        .is_err()
                    {
                        return;
                    }
                    // 照真上游：请求说了 `Connection: close` ⇒ 答完就关。
                    let asked_close = head
                        .header("connection")
                        .is_some_and(|v| v.to_ascii_lowercase().contains("close"));
                    if answer == Answer::CloseAfter || asked_close {
                        tls.conn.send_close_notify();
                        let _ = tls.flush();
                        return;
                    }
                }
            });
        }
    });
    TlsStub {
        port,
        accepted,
        bodies,
    }
}

/// 判据自己的上游选择：一律直通到那个 TLS 假上游，去处标签 ＝ 路由第二段（哪个号）。
struct ToStub(Base);
impl Destinations for ToStub {
    fn resolve(
        &self,
        _mode: super::super::Mode,
        key: &super::super::RouteKey,
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
    /// 判据里连着的几发都在墙钟上限之内 ⇒ 复用那几条判据量的是「复用」本身，不受空闲期限干扰。
    fn upstream_idle_reuse(&self) -> std::time::Duration {
        guard_core::wall::CEILING
    }
}

struct NoTap;
impl super::super::TapPort for NoTap {
    fn offer(&self, _ev: super::super::TapEvent) -> bool {
        true
    }
}

/// 起一个中转（监听回环），上游是 `https://localhost:<假上游端口>`；开 TCP 那一口一律连回环上的假上游。
fn spawn_pool_relay(stub_port: u16) -> SocketAddr {
    let base = Base::parse(&format!("https://localhost:{stub_port}")).expect("base");
    let open: Arc<OpenFor> =
        Arc::new(move |_from, _h, _p, _d| TcpStream::connect(SocketAddr::new(LOOPBACK, stub_port)));
    let relay = Arc::new(Relay::new(
        Arc::new(ToStub(base)),
        door::Keys {
            full: door::Key::from_text(POOL_FULL_KEY).expect("全权钥匙"),
            pass: door::Key::from_text(POOL_PASS_KEY).expect("直通钥匙"),
        },
        TeeSink::to_port(Arc::new(NoTap)),
        open,
        guard_core::wall::CEILING,
        guard_core::wall::CEILING,
        BODIES_WIDE,
    ));
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 中转");
    let addr = l.local_addr().expect("中转地址");
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s) = s else { continue };
            let r = Arc::clone(&relay);
            std::thread::spawn(move || {
                let _ = serve_one(s, &r);
            });
        }
    });
    addr
}

/// 打一发（`acct` 是路由第二段，即哪个号；来处段固定一个），读回下游收到的全部字节。
fn pool_send(relay: SocketAddr, acct: &str, body: &str) -> String {
    let mut c = TcpStream::connect(relay).expect("连中转");
    c.set_read_timeout(Some(guard_core::wall::CEILING))
        .expect("读期限");
    let req = format!(
        "POST /{POOL_FULL_KEY}/t/agentA/{acct}/~tok1/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    c.write_all(req.as_bytes()).expect("写请求");
    let mut got = Vec::new();
    c.read_to_end(&mut got)
        .expect("读下游响应（挂住 ⇒ 中转在等一个不会关的上游）");
    String::from_utf8(got).expect("utf8")
}

fn accepted(stub: &TlsStub) -> usize {
    stub.accepted.load(std::sync::atomic::Ordering::SeqCst)
}

/// 同一个号连发 5 发 ⇒ 上游只接下 1 条 TCP 连接（chunked 与定长两种收尾各走一遍），5 份请求体各到一次、应答原样回到下游。
#[test]
fn five_sends_on_one_account_ride_one_upstream_connection() {
    for answer in [Answer::KeepChunked, Answer::KeepLength] {
        let stub = spawn_tls_stub(answer);
        let relay = spawn_pool_relay(stub.port);
        for i in 0..5 {
            let got = pool_send(relay, "acctA", &format!("m{i}"));
            assert!(got.starts_with("HTTP/1.1 200 OK\r\n"), "第 {i} 发：{got}");
            assert!(
                got.contains(&format!("data: {{\"echo\":\"m{i}\"}}\n\n")),
                "第 {i} 发的应答体没原样回到下游：{got}"
            );
        }
        assert_eq!(
            *stub.bodies.lock().expect("lock"),
            vec!["m0", "m1", "m2", "m3", "m4"],
            "每份请求体该恰好到上游一次"
        );
        assert_eq!(
            accepted(&stub),
            1,
            "同一个号连发 5 发，上游该只接下 1 条连接"
        );
    }
}

/// 连接随号分开：换一个号另开一条，两个号各自接着复用自己那一条（不串号）。
#[test]
fn another_account_gets_its_own_upstream_connection() {
    let stub = spawn_tls_stub(Answer::KeepChunked);
    let relay = spawn_pool_relay(stub.port);
    pool_send(relay, "acctA", "a0");
    pool_send(relay, "acctB", "b0");
    assert_eq!(accepted(&stub), 2, "另一个号该另开一条连接，不借前一个号的");
    pool_send(relay, "acctA", "a1");
    pool_send(relay, "acctB", "b1");
    assert_eq!(accepted(&stub), 2, "两个号各自复用自己那一条");
    assert_eq!(
        *stub.bodies.lock().expect("lock"),
        vec!["a0", "b0", "a1", "b1"]
    );
}

/// 上游说了 `Connection: close`（哪怕连接实际还开着）⇒ 那条不再用，下一发另开；也不挂住。
#[test]
fn an_upstream_that_says_close_is_not_reused() {
    let stub = spawn_tls_stub(Answer::SaysCloseButStaysOpen);
    let relay = spawn_pool_relay(stub.port);
    for i in 0..3 {
        let got = pool_send(relay, "acctA", &format!("m{i}"));
        assert!(got.starts_with("HTTP/1.1 200 OK\r\n"), "第 {i} 发：{got}");
    }
    assert_eq!(accepted(&stub), 3, "说了要关的连接不该还回池里");
}

/// 上游在两发之间关掉了留着的那条（close_notify ＋ FIN）：下一发在写请求头之前探活扔掉它、另开一条照常成功，
/// 每份请求体只到上游一次（没有「发出去了、失败了、再发一次」）。
#[test]
fn a_pooled_connection_the_upstream_closed_is_dropped_before_any_byte_is_sent() {
    let stub = spawn_tls_stub(Answer::CloseAfter);
    let relay = spawn_pool_relay(stub.port);
    for i in 0..3 {
        let got = pool_send(relay, "acctA", &format!("m{i}"));
        assert!(got.starts_with("HTTP/1.1 200 OK\r\n"), "第 {i} 发：{got}");
        assert!(
            got.contains(&format!("\"echo\":\"m{i}\"")),
            "第 {i} 发：{got}"
        );
    }
    assert_eq!(
        *stub.bodies.lock().expect("lock"),
        vec!["m0", "m1", "m2"],
        "每份请求体该恰好到上游一次"
    );
    assert_eq!(accepted(&stub), 3);
}

// ══ 在途请求体合计的账 ═══════════════════════════════════════════════════
//
// 上游选择（判据自己的一张）进门就先报一声、再压着不答，放一次答一发：在途的那几发一直攥着自己那一笔账。

/// 不量体量的那几条判据给的账：比判据里任何一发都宽得多。
const BODIES_WIDE: usize = 1 << 30;

/// 判据的账与每一发的体：3 发记不下、2 发记得下。
const BODIES_NARROW: usize = 8 * 1024 * 1024;
const ONE_BODY: usize = 3 * 1024 * 1024;

/// 进 `resolve` 先报一声（`entered`），再等放行（`release` 来一次答一发：现成回包 200）。
struct Held {
    entered: std::sync::Mutex<mpsc::Sender<()>>,
    release: std::sync::Mutex<mpsc::Receiver<()>>,
}
impl Destinations for Held {
    fn resolve(
        &self,
        _mode: super::super::Mode,
        _key: &super::super::RouteKey,
        _ask: &Ask<'_>,
        act: &mut dyn FnMut(Destination<'_>),
    ) {
        let _ = self.entered.lock().expect("lock").send(());
        let _ = self
            .release
            .lock()
            .expect("lock")
            .recv_timeout(guard_core::wall::CEILING);
        act(Destination::Reply {
            status: "200 OK",
            reason: "held",
            headers: &[],
            body: b"ok",
        });
    }
    fn stream_label_headers(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn upstream_idle_reuse(&self) -> std::time::Duration {
        guard_core::wall::CEILING
    }
}

/// 起一个账为 `bodies` 的中转，上游选择是 [`Held`]；回中转地址 ＋ 「进来了」的收端 ＋ 放行的发端。
fn spawn_held_relay(bodies: usize) -> (SocketAddr, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let open: Arc<OpenFor> =
        Arc::new(|_from, _h, _p, _d| Err(std::io::Error::other("判据的上游选择不发上游")));
    let relay = Arc::new(Relay::new(
        Arc::new(Held {
            entered: std::sync::Mutex::new(entered_tx),
            release: std::sync::Mutex::new(release_rx),
        }),
        door::Keys {
            full: door::Key::from_text(POOL_FULL_KEY).expect("全权钥匙"),
            pass: door::Key::from_text(POOL_PASS_KEY).expect("直通钥匙"),
        },
        TeeSink::to_port(Arc::new(NoTap)),
        open,
        guard_core::wall::CEILING,
        guard_core::wall::CEILING,
        bodies,
    ));
    let l = TcpListener::bind(SocketAddr::new(LOOPBACK, 0)).expect("bind 中转");
    let addr = l.local_addr().expect("中转地址");
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s) = s else { continue };
            let r = Arc::clone(&relay);
            std::thread::spawn(move || {
                let _ = serve_one(s, &r);
            });
        }
    });
    (addr, entered, release)
}

/// 发一条体长 `n` 的请求头；`with_body` ⇒ 接着把体写全。回那条连接（读应答由调用方做）。
fn held_send(relay: SocketAddr, n: usize, with_body: bool) -> TcpStream {
    let mut c = TcpStream::connect(relay).expect("连中转");
    c.set_read_timeout(Some(guard_core::wall::CEILING))
        .expect("读期限");
    let head = format!(
        "POST /{POOL_FULL_KEY}/t/agentA/acctA/~tok1/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {n}\r\n\r\n"
    );
    c.write_all(head.as_bytes()).expect("写请求头");
    if with_body {
        c.write_all(&vec![b'x'; n]).expect("写请求体");
    }
    c
}

fn read_all(mut c: TcpStream) -> String {
    let mut got = Vec::new();
    c.read_to_end(&mut got).expect("读下游应答");
    String::from_utf8_lossy(&got).into_owned()
}

/// ★ 在途请求体合计记不下 ⇒ 读体之前就回 503、原因头说是体量满了，一个体字节都不读（这一发只发了头）；
/// 在途的一发收场把账退回来，下一发同样大的照常转。
#[test]
fn bodies_past_the_in_flight_budget_get_busy_before_a_body_byte_is_read() {
    let (relay, entered, release) = spawn_held_relay(BODIES_NARROW);
    let (done_tx, done) = mpsc::channel();
    for _ in 0..2 {
        let done_tx = done_tx.clone();
        std::thread::spawn(move || {
            let _ = done_tx.send(read_all(held_send(relay, ONE_BODY, true)));
        });
    }
    for i in 0..2 {
        entered
            .recv_timeout(guard_core::wall::CEILING)
            .unwrap_or_else(|_| panic!("第 {i} 发没走到上游选择"));
    }

    let third = read_all(held_send(relay, ONE_BODY, false));
    assert!(
        third.starts_with("HTTP/1.1 503 "),
        "账上已记着 2 × 3 MiB（上限 8 MiB），第 3 发该回 503：{third}"
    );
    assert!(
        third.contains(&format!("{REASON_HEADER}: body-budget\r\n")),
        "503 的原因头该说是体量满了：{third}"
    );

    release.send(()).expect("放行一发");
    let first = done
        .recv_timeout(guard_core::wall::CEILING)
        .expect("放行的那一发该收场");
    assert!(first.starts_with("HTTP/1.1 200 "), "放行的那一发：{first}");

    let (late_tx, late) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = late_tx.send(read_all(held_send(relay, ONE_BODY, true)));
    });
    entered
        .recv_timeout(guard_core::wall::CEILING)
        .expect("账退回来之后，同样大的一发该照常走到上游选择");
    release.send(()).expect("放行");
    release.send(()).expect("放行");
    let fourth = late
        .recv_timeout(guard_core::wall::CEILING)
        .expect("第 4 发该收场");
    assert!(fourth.starts_with("HTTP/1.1 200 "), "第 4 发：{fourth}");
}

/// 请求体按声明的长度一次分配：读出来那一份的容量恰好就是长度（不按读到的字节翻倍增长）。
#[test]
fn a_body_is_allocated_once_at_its_declared_length() {
    let n = ONE_BODY + 7;
    let src = vec![b'x'; n];
    let mut r = std::io::Cursor::new(&src[..]);
    let got = http1::read_exact_body(&mut r, n, BODY_CAP)
        .expect("io")
        .expect("没超上限");
    assert_eq!(got.len(), n);
    assert_eq!(got.capacity(), n, "读出来那一份的容量该恰好是声明的长度");
}
