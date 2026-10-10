//! 拨了必被拒、又不会被别人捡走的本机口（测试用）。
//!
//! 从前各条判据「绑 `127.0.0.1:0` 拿个口 → 关掉 → 再去拨」：关掉之后那个口就回了池子，
//! 机器忙时同一进程或别的进程里的测试正好绑上它 ⇒ 拨通了别人的服务，原因词跟着变（`dial` 那条负载 36 时红过）。
//! 这里绑上、**不听**：拨它内核当场回拒绝，口一直占着，别人绑不上。占到进程结束。

/// 一个此刻起拨它必被拒的本机口。
pub(crate) fn refusing_port() -> u16 {
    let sock = tokio::net::TcpSocket::new_v4().expect("开 socket");
    sock.bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("绑口");
    let port = sock.local_addr().expect("口").port();
    // 不 listen：拨它 ⇒ 拒绝；一直占着 ⇒ 别人绑不上同一个口。
    std::mem::forget(sock);
    port
}

#[test]
fn the_port_refuses_and_stays_taken() {
    let port = refusing_port();
    let e = std::net::TcpStream::connect(("127.0.0.1", port)).expect_err("绑着不听的口居然拨通了");
    assert_eq!(e.kind(), std::io::ErrorKind::ConnectionRefused, "{e}");
    assert!(
        std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
        "占着的口别人还绑得上 —— 挡不住被捡走"
    );
}
