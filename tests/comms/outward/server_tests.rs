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

/// ★★ `阻-3(D3)` **后半段**的行为格㈡〔回修轮之六 08-25〕：**机制那一半**。
///
/// 它买两样，都是行为：
/// 1. socket 上有读期限时，`read_head` 面对一条**半开**连接（只发半个请求头、
///    **不关**连接）会**报错返回**，而不是永久挂住；
/// 2. 那条错误**没有被任何一层当成「再试一次」** —— `read_head` 直接把它交出来。
///    ⭐ 这第 2 条是本轮最要紧的一格：一旦哪层重试，期限就从「阻塞有上限」
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
    //    ⇒ 把读搬到一条工作线程上，用 `recv_timeout` 给它一个**远宽于**期限的上界（20 倍），
    //      超了就 `panic!` ⇒ **把挂住换成红**。这条纪律本文件 `送 5x` 那几处已经在用。
    //    〔本轮 `MU5` 实测：把 `http1::read_head` 的读循环改成「`WouldBlock` 也 `continue`」
    //      —— 没有这一层的话它整条判据挂死，有了这一层它**红**。〕
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let r = http1::read_head(&mut srv, HEAD_CAP);
        let _ = tx.send((r, t0.elapsed()));
    });
    let (r, waited) = rx
        .recv_timeout(deadline * 20)
        .expect("`read_head` 在 20 倍期限之内一个字都没返回 —— 期限没起作用，或者有哪一层在**重试**（那就成了轮询）");
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

    // ── ㈡ 非空对照：同一把尺子、同样的期限，一条**发全了**的连接必须走通且明显快。
    let mut whole = TcpStream::connect(a).expect("connect 完整");
    whole
        .write_all(b"POST /7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57/s/agentA/acctA/v1/messages HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write 完整头");
    whole.flush().expect("flush");
    let (mut srv2, _p2) = l.accept().expect("accept 完整");
    srv2.set_read_timeout(Some(deadline)).expect("装期限");
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
        fast < deadline,
        "非空对照：数据已经在那儿了，`read_head` 该**立刻**返回而不是等满 {deadline:?}（实测 {fast:?}）\
             —— 等满了说明这把尺子把正常流量也判成了超时"
    );
    drop(whole);
}
