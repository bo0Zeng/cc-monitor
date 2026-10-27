//! 拨 SSH 那一跳经出网口：没写跳板、出网描述指到沙箱里的假代理 ⇒ 拨号的 TCP 经假代理到那台的 22 口。
//! 直连那一路被测试档断言断着（出网口真开 TCP 的那个目的地不是回环 ⇒ panic），所以假代理收到那一问 ＝ 这一跳经了代理。

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// 假 HTTP 代理：接一条连接，把请求头交出来，回 200，再送一行 SSH 版本头（隧道那头「那台」说的第一句）。
fn http_proxy() -> (std::net::SocketAddr, mpsc::Receiver<String>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let a = l.local_addr().expect("addr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut s, _)) = l.accept() else { return };
        let mut got = Vec::new();
        let mut b = [0u8; 1];
        while !got.ends_with(b"\r\n\r\n") {
            if s.read(&mut b).unwrap_or(0) == 0 {
                return;
            }
            got.push(b[0]);
        }
        let _ = tx.send(String::from_utf8_lossy(&got).into_owned());
        let _ = s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\nSSH-2.0-far\r\n");
        let mut sink = [0u8; 64];
        while s.read(&mut sink).unwrap_or(0) > 0 {}
    });
    (a, rx)
}

/// 假 SOCKS5 代理（不要鉴权）：记下要去的名字与端口，回成功，再送一行 SSH 版本头。
fn socks_proxy() -> (std::net::SocketAddr, mpsc::Receiver<(String, u16)>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let a = l.local_addr().expect("addr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut s, _)) = l.accept() else { return };
        let mut h = [0u8; 3];
        s.read_exact(&mut h).expect("hello");
        s.write_all(&[5, 0]).expect("pick");
        let mut r = [0u8; 5];
        s.read_exact(&mut r).expect("req");
        let mut name = vec![0u8; r[4] as usize];
        s.read_exact(&mut name).expect("name");
        let mut p = [0u8; 2];
        s.read_exact(&mut p).expect("port");
        let _ = tx.send((
            String::from_utf8_lossy(&name).into_owned(),
            u16::from_be_bytes(p),
        ));
        let _ = s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 22]);
        let _ = s.write_all(b"SSH-2.0-far\r\n");
        let mut sink = [0u8; 64];
        while s.read(&mut sink).unwrap_or(0) > 0 {}
    });
    (a, rx)
}

fn far() -> Endpoint {
    Endpoint {
        host: "devbox.test".to_string(),
        port: 22,
    }
}

async fn first_line(tcp: &mut tokio::net::TcpStream) -> String {
    use tokio::io::AsyncReadExt;
    let mut got = vec![0u8; 13];
    tcp.read_exact(&mut got).await.expect("读那台的版本头");
    String::from_utf8_lossy(&got).into_owned()
}

/// ★ 经 HTTP 代理：`CONNECT devbox.test:22`；之后这条异步 socket 上读到的就是那台的字节；经代理 ⇒ 压缩照「往返读不到」那一格（压）。
#[tokio::test]
async fn the_ssh_hop_goes_through_an_http_proxy() {
    let (p, asks) = http_proxy();
    let via = egress::Egress::via(&format!("http://127.0.0.1:{}", p.port()), "");
    let (mut tcp, compress) = tcp_hop(&far(), &via).await.expect("经代理拨");
    let head = asks.recv().expect("代理收到一问");
    assert!(
        head.starts_with("CONNECT devbox.test:22 HTTP/1.1\r\n"),
        "{head:?}"
    );
    assert_eq!(first_line(&mut tcp).await, "SSH-2.0-far\r\n");
    assert_eq!(
        compress, RUSSH_ZLIB_SOUND,
        "量到的是到代理（回环）的往返，不是到那台的 ⇒ 不能判成不压"
    );
}

/// 经 SOCKS5（`socks5h://`）：名字交给代理。
#[tokio::test]
async fn the_ssh_hop_goes_through_a_socks5_proxy() {
    let (p, asks) = socks_proxy();
    let via = egress::Egress::via(&format!("socks5h://127.0.0.1:{}", p.port()), "");
    let (mut tcp, _) = tcp_hop(&far(), &via).await.expect("经 SOCKS5 拨");
    assert_eq!(
        asks.recv().expect("代理收到一问"),
        ("devbox.test".to_string(), 22)
    );
    assert_eq!(first_line(&mut tcp).await, "SSH-2.0-far\r\n");
}

/// 免代理表命中、目的地是回环 ⇒ 直连，代理一问都收不到；压缩照回环那一格（不压）。
#[tokio::test]
async fn a_loopback_sshd_is_dialed_direct_even_with_a_proxy_set() {
    let (p, asks) = http_proxy();
    let sshd = TcpListener::bind("127.0.0.1:0").expect("bind");
    let ep = Endpoint {
        host: "127.0.0.1".to_string(),
        port: sshd.local_addr().unwrap().port(),
    };
    let via = egress::Egress::via(&format!("http://127.0.0.1:{}", p.port()), "");
    let (_tcp, compress) = tcp_hop(&ep, &via).await.expect("直连回环");
    sshd.accept().expect("那台自己接到");
    assert!(!compress);
    assert!(asks.try_recv().is_err(), "代理不该收到一问");
}

/// 代理拒 ⇒ 拨号报 `tcp` 那一步（没连上），不是卡住。
#[tokio::test]
async fn a_refusing_proxy_fails_the_hop_at_tcp() {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let a = l.local_addr().unwrap();
    std::thread::spawn(move || {
        let Ok((mut s, _)) = l.accept() else { return };
        let mut b = [0u8; 256];
        let _ = s.read(&mut b);
        let _ = s.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n");
    });
    let via = egress::Egress::via(&format!("http://127.0.0.1:{}", a.port()), "");
    let (stage, e) = tcp_hop(&far(), &via).await.expect_err("代理拒了");
    assert_eq!(stage, "tcp", "{e}");
}
