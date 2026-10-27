//! 每条 SSH 连接的读数（`connect::Linked::facts` · `pool::Pool::links` · `pool::Pool::moved`）。
//!
//! 假对端：进程内 russh 服务端，起在回环口上（不出网，不碰真机器）；服务端那一侧的 TCP 由判据自己套一层数字节，
//! 拿它当对拍的另一边 —— 连接自己报的进 · 出字节必须与对端量到的线上字节逐一相等。

use crate::connect::{establish, Linked};
use crate::guard_support::{register, test_host, TestCred};
use crate::pool::{Lane, Pool};
use crate::{DialRequest, StageSink};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 这一份判据那台假对端的口令编号与口令。
const KEY_ID: &str = "link-meter-pw";
const PASSWORD: &str = "pw";
/// exec 回的那一大段（不可压的伪随机字节，压了也小不了多少；这里只验「数到了」）。
const REPLY_LEN: usize = 48 * 1024;

/// 服务端那一侧的 TCP 套一层：读到的记「对端收」，写出去的记「对端发」。
struct Count<S> {
    inner: S,
    got: Arc<AtomicU64>,
    sent: Arc<AtomicU64>,
}

impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Count<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = buf.filled().len() - before;
        self.got.fetch_add(n as u64, Ordering::SeqCst);
        r
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Count<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if let std::task::Poll::Ready(Ok(n)) = r {
            self.sent.fetch_add(n as u64, Ordering::SeqCst);
        }
        r
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

#[derive(Clone)]
struct Peer;

impl russh::server::Handler for Peer {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _user: &str,
        password: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        Ok(if password == PASSWORD {
            russh::server::Auth::Accept
        } else {
            russh::server::Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<russh::server::Msg>,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn exec_request(
        &mut self,
        channel: russh::ChannelId,
        _data: &[u8],
        session: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let body: Vec<u8> = (0..REPLY_LEN)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
        session.data(channel, body)?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)
    }

    /// 当跳板：只许开到回环（测试档不许出网）。
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: russh::Channel<russh::server::Msg>,
        host: &str,
        port: u32,
        _oa: &str,
        _op: u32,
        _session: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        assert_eq!(host, "127.0.0.1", "测试档不许出网：跳板被要求开到 {host}");
        let Ok(port) = u16::try_from(port) else {
            return Ok(false);
        };
        let Ok(mut tcp) = tokio::net::TcpStream::connect((host, port)).await else {
            return Ok(false);
        };
        tokio::spawn(async move {
            let mut ch = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut ch, &mut tcp).await;
        });
        Ok(true)
    }
}

/// 对端量到的：它收了多少 · 发了多少（线上字节）。
struct Tally {
    got: Arc<AtomicU64>,
    sent: Arc<AtomicU64>,
}

/// 在回环口上起一台假对端；回口与它的记账。
async fn peer() -> (u16, Tally) {
    test_host();
    register(KEY_ID, TestCred::Password(PASSWORD.to_string()));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("绑回环口");
    let port = listener.local_addr().expect("口").port();
    let cfg = Arc::new(russh::server::Config {
        keys: vec![russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[77u8; 32]),
        )],
        auth_rejection_time: std::time::Duration::ZERO,
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        inactivity_timeout: None,
        keepalive_interval: None,
        ..Default::default()
    });
    let got = Arc::new(AtomicU64::new(0));
    let sent = Arc::new(AtomicU64::new(0));
    let (g, s) = (Arc::clone(&got), Arc::clone(&sent));
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let cfg = Arc::clone(&cfg);
            let road = Count {
                inner: tcp,
                got: Arc::clone(&g),
                sent: Arc::clone(&s),
            };
            tokio::spawn(async move {
                if let Ok(running) = russh::server::run_stream(cfg, road, Peer).await {
                    let _ = running.await;
                }
            });
        }
    });
    (port, Tally { got, sent })
}

fn request(port: u16) -> DialRequest {
    serde_json::from_value(serde_json::json!({
        "host": "127.0.0.1",
        "port": port,
        "user": "u",
        "key_path": KEY_ID,
        "probe": true,
    }))
    .expect("拨号请求读不动")
}

async fn link(port: u16) -> Linked {
    establish(&request(port), &StageSink::new(false))
        .await
        .map_err(|(e, _)| e.said)
        .expect("连假对端没连上")
}

/// 在这条连接上 exec 一次，把回的字节收全。
async fn exec_once(l: &Linked) -> usize {
    let mut ch = l.session.channel_open_session().await.expect("开通道");
    ch.exec(true, "x").await.expect("exec");
    let mut n = 0;
    while let Some(m) = ch.wait().await {
        match m {
            russh::ChannelMsg::Data { data } => n += data.len(),
            russh::ChannelMsg::Close => break,
            _ => {}
        }
    }
    n
}

/// 等到 `f` 成立（两头的字节在路上）；到兜底上限还不成立 ⇒ 回最后一次的读数让断言说清。
async fn settle<T>(mut f: impl FnMut() -> (bool, T)) -> T {
    let deadline = guard_core::wall::CEILING;
    let step = std::time::Duration::from_millis(10);
    let mut waited = std::time::Duration::ZERO;
    loop {
        let (ok, v) = f();
        if ok || waited >= deadline {
            return v;
        }
        tokio::time::sleep(step).await;
        waited += step;
    }
}

/// ★ 一条连接报的进 · 出字节就是线上的字节：与对端量到的逐一相等，而且装得下 exec 回的那一大段。
#[tokio::test(flavor = "multi_thread")]
async fn a_link_reports_the_wire_bytes_both_ways() {
    let (port, tally) = peer().await;
    let l = link(port).await;
    let got = exec_once(&l).await;
    assert_eq!(got, REPLY_LEN, "exec 回的那一段没收全");
    let (wire, seen_in, seen_out) = settle(|| {
        let w = l.facts().link;
        let (i, o) = (
            tally.sent.load(Ordering::SeqCst),
            tally.got.load(Ordering::SeqCst),
        );
        (w.bytes_in == i && w.bytes_out == o, (w, i, o))
    })
    .await;
    assert_eq!(wire.bytes_in, seen_in, "进的字节与对端发出的对不上");
    assert_eq!(wire.bytes_out, seen_out, "出的字节与对端收到的对不上");
    assert!(
        wire.bytes_in >= REPLY_LEN as u64,
        "进的字节 {} 装不下 exec 回的 {REPLY_LEN}",
        wire.bytes_in
    );
    assert!(wire.bytes_out > 0, "握手 ＋ 鉴权 ＋ exec 都发过字节，出不该是 0");
}

/// ★ 压没压：回环那一跳判准答「不压」⇒ 这条连接报不压；地址是胜出的那一条。直连没有跳板那几项。
#[tokio::test(flavor = "multi_thread")]
async fn a_loopback_link_says_it_does_not_compress() {
    let (port, _) = peer().await;
    let l = link(port).await;
    let f = l.facts();
    assert!(!f.link.compressed, "回环那一跳不压，连接却报压");
    assert_eq!(f.link.endpoint, format!("127.0.0.1:{port}"));
    assert!(f.jumps.is_empty(), "直连那一条不该报跳板");
}

/// ★ 池的快照只列活着的连接、按身份分组；放进一条 · 最后一个用它的人走了，`moved` 各记一笔。
#[tokio::test(flavor = "multi_thread")]
async fn the_pool_lists_live_links_by_identity_and_says_when_they_move() {
    let (port, _) = peer().await;
    let pool: Pool<Linked> = Pool::new();
    let mut moved = pool.moved();
    assert!(pool.links().is_empty(), "空池不该列出连接");
    let placed = pool
        .place("k", Lane::Query, || async {
            establish(&request(port), &StageSink::new(false)).await
        })
        .await
        .map_err(|(e, _)| e.said)
        .expect("放置");
    assert!(
        moved.has_changed().expect("发端还在"),
        "放进一条连接，moved 没记一笔"
    );
    moved.borrow_and_update();
    let links = pool.links();
    assert_eq!(links.len(), 1, "一个身份一组");
    assert_eq!(links[0].0, "k");
    assert_eq!(links[0].1.len(), 1, "那一族一条连接");
    assert_eq!(links[0].1[0].link.endpoint, format!("127.0.0.1:{port}"));
    drop(placed);
    assert!(
        moved.has_changed().expect("发端还在"),
        "最后一个用它的人走了（连接收掉），moved 没记一笔"
    );
    assert!(pool.links().is_empty(), "收掉的连接还列着");
}

/// ★ 经跳板：目标那一条的地址是目标，跳板那一条另报（不压）；跳板那条 TCP 运着整条隧道 ⇒ 它的进出都不少于目标那一条。
#[tokio::test(flavor = "multi_thread")]
async fn a_jumped_link_reports_its_jump_hop_too() {
    let (jump_port, _) = peer().await;
    let (target_port, _) = peer().await;
    let mut v = serde_json::to_value(request(target_port)).expect("请求写不出");
    v["jump"] = serde_json::json!({
        "host": "127.0.0.1",
        "port": jump_port,
        "user": "u",
        "key_path": KEY_ID,
    });
    let req: DialRequest = serde_json::from_value(v).expect("拨号请求读不动");
    let l = establish(&req, &StageSink::new(false))
        .await
        .map_err(|(e, _)| e.said)
        .expect("经跳板没连上");
    assert_eq!(exec_once(&l).await, REPLY_LEN);
    let f = l.facts();
    assert_eq!(f.link.endpoint, format!("127.0.0.1:{target_port}"));
    assert_eq!(f.jumps.len(), 1, "一台跳板一项");
    let j = &f.jumps[0];
    assert_eq!(j.endpoint, format!("127.0.0.1:{jump_port}"));
    assert!(!j.compressed, "跳板那一条只运隧道，不压");
    assert!(
        j.bytes_in >= f.link.bytes_in && f.link.bytes_in >= REPLY_LEN as u64,
        "跳板那条进 {} · 目标那条进 {}：跳板运着整条隧道，不该比目标少",
        j.bytes_in,
        f.link.bytes_in
    );
}
