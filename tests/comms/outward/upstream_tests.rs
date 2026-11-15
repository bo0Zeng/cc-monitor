use super::*;

/// 一个基址（串怎么读成它是宿主的事，判据在后端 `table_tests`；这里只要读好的那几格）。
fn base(tls: bool, host: &str, port: u16, path: &str) -> Base {
    Base {
        tls,
        host: host.to_string(),
        port,
        path: path.to_string(),
    }
}

#[test]
fn host_header_omits_the_default_port_only() {
    let d = base(true, "api.example.com", 443, "");
    assert_eq!(d.host_header(), "api.example.com");
    let n = base(false, "127.0.0.1", 18789, "");
    assert_eq!(n.host_header(), "127.0.0.1:18789");
    // ★ `Host:` 头里**没有**路径前缀那一段（它属于请求行，不属于这里）。
    let p = base(true, "api.example.com", 443, "/anthropic");
    assert_eq!(p.host_header(), "api.example.com");
}

/// TLS 那条路**没有任何行为验证**（不许打真 API，本机也没有 HTTPS 夹具）。
///
/// # 名字只承诺它证得了的那一半
///
/// 只断 crate 常量 `webpki_roots::TLS_SERVER_ROOTS` 非空的话是**空真**：那**不是** `tls_config()`
/// 真把根装了进去 ⇒ 把装配点的根证书集换成空 `Vec::new()`，它照样全绿。
///
/// 它断的是**生产段的装配函数** `root_store()` 的**根证书条数 > 0**，
/// 而 `tls_config()` 里那一份就是它返回的那一份（唯一调用点，就在上面几行）。
///
/// **它仍然不证明**：握手成功 · 证书校验真的按这套根做 · 逐块透传在 TLS 上成立。
/// 那三样要真 TLS 行为验，这里禁打真 API ⇒ 登记为 `判不了`，别再让名字替它们背书。
#[test]
fn tls_client_config_builds_from_a_non_empty_root_store() {
    // ★ 真查数量（>0），而且查的是**装进去的那一份**，不是 crate 常量。
    let n = root_store().roots.len();
    assert!(
        n > 0,
        "装进 ClientConfig 的根证书集不该是空的（实测 {n} 条）"
    );
    // 非空对照：空的根证书集在 rustls 自己看来连服务端校验器都建不起来
    //（`VerifierBuilderError::NoRootAnchors`）⇒ 这一条把「非空」与「它真能当校验根用」连起来。
    assert!(
        rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(root_store()),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .is_ok(),
        "非空的根证书集应当建得起服务端校验器"
    );
    let name = rustls::pki_types::ServerName::try_from("api.example.com".to_string())
        .expect("域名应当合法");
    let conn = rustls::ClientConnection::new(tls_config(), name);
    assert!(conn.is_ok(), "TLS 客户端连接对象应当建得起来");
}

/// 沙箱 CA（`tests/__fixtures__/relay-tls-ca.der` 自签 CA ＋ 它签的 `localhost` 叶子 `relay-tls-leaf.der` / `relay-tls-leaf-key.der`，有效期一百年，只在判据里用）。
pub(crate) fn sandbox_ca() -> rustls::pki_types::CertificateDer<'static> {
    rustls::pki_types::CertificateDer::from(
        &include_bytes!("../../__fixtures__/relay-tls-ca.der")[..],
    )
}

/// TLS 假上游那一侧的配置：沙箱 CA 签的 `localhost` 叶子。
pub(crate) fn sandbox_server_config() -> Arc<rustls::ServerConfig> {
    let leaf = rustls::pki_types::CertificateDer::from(
        &include_bytes!("../../__fixtures__/relay-tls-leaf.der")[..],
    );
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        &include_bytes!("../../__fixtures__/relay-tls-leaf-key.der")[..],
    ));
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("rustls 默认协议版本集应当可用")
    .with_no_client_auth()
    .with_single_cert(vec![leaf], key)
    .expect("沙箱叶子证书与私钥应当配得上");
    Arc::new(cfg)
}

/// 进程里只有一份 TLS 客户端配置：两次取到的是同一个 `Arc`（rustls 的会话缓存挂在它上面，每发新建一份就等于没有缓存）。
#[test]
fn the_tls_client_config_is_built_once_per_process() {
    let a = tls_config();
    let b = tls_config();
    assert!(
        Arc::ptr_eq(&a, &b),
        "每取一次都新建了一份 ClientConfig ⇒ 会话缓存跨发不起作用"
    );
}

/// 回环上的一个假上游：接一条连接，读完请求头，回一段写好的字节，然后**连接留着不关**，直到放行。
fn answer_and_hold(
    reply: &'static [u8],
) -> (
    Base,
    std::sync::mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("绑回环");
    let port = l.local_addr().expect("地址").port();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let t = std::thread::spawn(move || {
        let (mut s, _) = l.accept().expect("接");
        let mut got = Vec::new();
        let mut b = [0u8; 1];
        while !got.ends_with(b"\r\n\r\n") {
            match s.read(&mut b) {
                Ok(1) => got.push(b[0]),
                _ => return,
            }
        }
        s.write_all(reply).expect("回包");
        // 不设期限：调用方在 `fetch` 返回之后才放行（出错 · panic 时放行的那一头被丢掉，这里也就醒了）。
        // 这一头若给了期限，到点先关了连接，`fetch` 读到关就照样拿到回包 ⇒ 等对端关的那一形也绿。
        let _ = held.recv();
    });
    (base(false, "127.0.0.1", port, ""), release, t)
}

fn open_loopback(host: &str, port: u16, _d: Duration) -> std::io::Result<Opened> {
    TcpStream::connect((host, port)).map(|tcp| Opened {
        tcp,
        via_proxy: false,
    })
}

/// 对着 [`answer_and_hold`] 发一问一答。假上游在 `fetch` 返回之后才放行、才关连接 ⇒
/// `fetch` 要是等对端关，只能等到读期限（兜底的墙钟上限）报错；收尾就停的才拿得到回包。不量快慢。
fn fetch_from_holder(reply: &'static [u8]) -> std::io::Result<Fetched> {
    let (base, release, t) = answer_and_hold(reply);
    let got = fetch(
        &open_loopback,
        &base,
        "GET",
        "/x",
        &[],
        b"",
        guard_core::wall::CEILING,
        1024,
    );
    let _ = release.send(());
    t.join().expect("假上游");
    got
}

/// 一问一答：回包按 `Content-Length` 收全就返回，不等对端关（对端不理 `Connection: close` 时不干等到期限）。
#[test]
fn fetch_returns_once_the_answer_is_complete_without_waiting_for_the_peer_to_close() {
    let got = fetch_from_holder(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
        .expect("回包早已收全，却在等对端关连接，等到期限报错");
    assert_eq!(got.status, 200);
    assert_eq!(got.body, b"ok", "回包体不对（多收或少收）");
}

/// 分块的回包同理：终止块到了就返回；空体（`Content-Length: 0`）一个字节都不等。
#[test]
fn fetch_returns_once_a_chunked_or_empty_answer_ends_without_waiting_for_the_peer_to_close() {
    let got = fetch_from_holder(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n",
    )
    .expect("分块回包已收尾，却在等对端关连接");
    assert_eq!(got.body, b"ok");
    let got = fetch_from_holder(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
        .expect("空体回包，却在等对端关连接");
    assert_eq!(got.body, b"");
}

/// 一条连上回环的空闲连接（对端一直开着、什么都不发）。
fn idle_conn() -> (Conn, std::net::TcpStream) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("绑回环");
    let c = TcpStream::connect(l.local_addr().expect("地址")).expect("连");
    let (peer, _) = l.accept().expect("接");
    let c = Metered {
        tcp: c,
        via_proxy: false,
        meter: Arc::default(),
    };
    (Conn::Plain(c), peer)
}

/// 空闲太久的连接不再复用（对端或路上的设备可能已经悄悄丢了它，探活看不出）：取的时候比一下还回来的时刻，
/// 过了期限就扔、由调用方新连。期限之内的照常复用（非空对照：不是一律不复用）。
/// 不睡：期限给零 ⇒ 还回来之后哪怕过了一纳秒也算过了期限；期限给墙钟上限 ⇒ 这几步之内一定没过。
#[test]
fn a_pooled_connection_idle_past_the_reuse_limit_is_not_handed_out() {
    let lane = Lane::new(None, "t", &base(false, "127.0.0.1", 1, ""));

    let roomy = Pool::reusing_within(guard_core::wall::CEILING);
    let (fresh, _p1) = idle_conn();
    roomy.put(lane.clone(), fresh);
    assert!(
        roomy.take(&lane, guard_core::wall::CEILING).is_some(),
        "期限之内的空闲连接该复用"
    );

    let strict = Pool::reusing_within(Duration::ZERO);
    let (stale, _p2) = idle_conn();
    strict.put(lane.clone(), stale);
    assert!(
        strict.take(&lane, guard_core::wall::CEILING).is_none(),
        "空闲超过期限的连接被拿去复用了：对端可能已悄悄丢了它，这一发会 502 / 挂到期限"
    );
}
