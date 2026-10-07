//! 上游那一跳：连出去、把请求原样递上去。
//!
//! 上游是 `https://…`，TLS 不能手写；CONNECT 隧道（不解密直通）tee 不到明文，而 tee 正是这件事的意义 ⇒ TLS 是必要条件。
//! 选 `rustls`（`ring` provider）＋ 手写 HTTP/1.1：硬要求是「逐块透传绝不缓冲」，`rustls` 不替你做 HTTP（`StreamOwned` 就是一个 `Read`，每次 `read` 拿多少给多少），
//! 没有任何一层会替你把响应体攒完。`hyper` / `reqwest` 把「一个块」变成一层框架；起 `curl` 子进程会给任意远端加一条外部二进制依赖。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

/// 一条 `base_url` 进不了 `Base` 的理由。带一句话而不是一个 `None`：「不是个 URL」「协议不认识」「端口读不懂」「路径里带查询串」挤在一个 `None` 里，
/// 调用方只能印一句万能的话，在后三形上都是假的指引。那句话是 `&'static str`（不含文件内容）⇒ 进日志安全（与 `table::Rejected::why` 同一条理由）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseIssue(pub &'static str);

/// 上游基址。`https` 走 TLS，`http` 走明文。本地部署（回环上的 http 明文）是一等公民；「明文 + 非回环」= 一把 key 明着过网线 ⇒ 由 `table::build` 拒掉并出声
/// （判据 `a_plaintext_upstream_is_only_allowed_on_loopback`）。本结构体自己不判这一条：它只答「这个串长什么样」，不答「许不许用」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    /// 基址里那一段路径前缀，`""` = 没有。带前导 `/`、不带尾随 `/`。
    /// 第三方把「Anthropic 兼容」挂在一个前缀底下是常见做法；只取 authority、丢掉路径的话，`https://host/v1` 碰巧对（客户端自己带着 `/v1`），
    /// `https://host/<网关前缀>` 静默打到别的地方。它会改变字节：拼法见 [`Base::upstream_target`]，由 `the_path_prefix_from_the_base_url_really_reaches_the_request_line` 钉住。
    pub path: String,
}

impl Base {
    /// 「base URL 能不能用」全仓只有一份，住共享 crate `upstream_url_core`（协议闭集 `SCHEMES` 也在那儿）；这里只把它的形状结论装成 `Base`、把理由翻成一句话。
    pub fn parse(url: &str) -> Result<Base, BaseIssue> {
        let u = upstream_url_core::parse(url).map_err(BaseIssue::of)?;
        Ok(Base {
            tls: u.tls,
            host: u.host,
            port: u.port,
            path: u.path,
        })
    }

    /// `Host:` 头该写什么（默认端口不写端口，非默认端口要写）。
    pub fn host_header(&self) -> String {
        let default = if self.tls { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// 上游请求行的目标 = 这个基址的路径前缀 + 客户端那份真路径。会改变发出去的字节的两处之一（另一处是 `server.rs` 换头那一行，`auth_header_of`）。
    ///
    /// - `rest` 是下游原样的「真路径 + 查询串」（`route::parse` 保证它以 `/` 打头），一个字节都不改，只在前面接上这个基址自己的前缀。
    /// - 前缀是 `""` 时，返回值与 `rest` 逐字节相同 ⇒ 没配前缀的那一路零字节改动。
    /// - 不查重、不合并重复的段：配 `https://h/v1` 而客户端发 `/v1/messages` 会得到 `/v1/v1/messages`。猜「哪一段是重复」猜错的症状是静默打到另一个地方
    ///   ⇒ 处置是出声（装表时给这一行记一条 `Note`，见 `table::NOTE_PATH_PREFIX`），不替人重写他写下的东西。
    /// 住 `Base` 不住 `table::Row`：「前缀 + 真路径」是传输原语，中转拿到的是 `Destination` 里那个 `&Base`，够不到 `Row`。
    pub fn upstream_target(&self, rest: &str) -> String {
        format!("{}{}", self.path, rest)
    }
}

impl BaseIssue {
    /// 形状那几形各一句（句子住文案表；判定住 `upstream_url_core::parse`）。
    pub fn of(i: upstream_url_core::ShapeIssue) -> BaseIssue {
        use upstream_url_core::ShapeIssue as S;
        BaseIssue(match i {
            S::Whitespace => copy_core::copy_static!("beRelayUpstream.parse.whitespace"),
            S::NotUrl => copy_core::copy_static!("beRelayUpstream.parse.notUrl"),
            S::BadScheme => copy_core::copy_static!("beRelayUpstream.parse.badScheme"),
            S::NoHost => copy_core::copy_static!("beRelayUpstream.parse.noHost"),
            S::HasQuery => copy_core::copy_static!("beRelayUpstream.parse.hasQuery"),
            S::BadPort => copy_core::copy_static!("beRelayUpstream.parse.badPort"),
            S::DoubleSlash => {
                copy_core::copy_static!("beRelayUpstream.normalizePrefix.doubleSlash")
            }
        })
    }
}

/// 一条上游连接。两个变体都实现 `Read`/`Write` ⇒ 转发循环对 TLS 与否**一无所知**。
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
        }
    }
}

/// 装进 `ClientConfig` 的那一份根证书集 —— 用 `webpki-roots` 内嵌的那套（不读机器上的证书目录：本 crate 会被推到任意远端）。
/// 抽成有名字的生产段，判据打得到真正装进去的那一份（断 crate 常量 `webpki_roots::TLS_SERVER_ROOTS` 非空是另一个东西）。
fn root_store() -> rustls::RootCertStore {
    rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    let roots = root_store();
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("rustls 默认协议版本集应当可用")
    .with_root_certificates(roots)
    .with_no_client_auth();
    Arc::new(cfg)
}

/// 期限的值由后端交进来（`UPSTREAM_DEADLINE` 住 `relay/listen.rs`）：值归后端 · 执行归通信层，本文件生产段零期限字面量。

pub fn connect(base: &Base, deadline: Duration) -> std::io::Result<Conn> {
    // 测试档不许出网：目的地不是本机回环（回环判定只有 `upstream_is_loopback` 那一个家）⇒ 当场 panic，不连、不解析名字。
    #[cfg(any(test, feature = "test-support"))]
    assert!(
        upstream_url_core::upstream_is_loopback(&base.host),
        "测试档不许出网：{}",
        base.host
    );
    let tcp = TcpStream::connect((base.host.as_str(), base.port))?;
    // ★ Nagle 两个方向都要关。参考实现登记过：没关会让 p95 塌到 3504ms。
    tcp.set_nodelay(true)?;
    // ★★ 读写期限：没有它，一条死了但没发 FIN 的上游会把一条连接线程**永久**钉在
    //    `pump` 里那句 `up.read`（或 `read_response_head`）上。两个方向都要：
    //    写那半钉的是 `up.write_all(&body)`（请求体最大 `BODY_CAP` = 64 MiB，远超 socket 发送缓冲）。
    tcp.set_read_timeout(Some(deadline))?;
    tcp.set_write_timeout(Some(deadline))?;
    if !base.tls {
        return Ok(Conn::Plain(tcp));
    }
    let name = rustls::pki_types::ServerName::try_from(base.host.clone())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let conn =
        rustls::ClientConnection::new(tls_config(), name).map_err(|e| std::io::Error::other(e))?;
    Ok(Conn::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

/// 一问一答读回来的整段回包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// 状态行里的三位数字；读不出 ⇒ 0。
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// 一问一答：发一整发（`Connection: close`）、读回整段回包。给后端自己要发的那几发用；
/// 期限（连接上每一次读写）与回包体上限都由调用方给。回包体超过上限 ⇒ 错，不截断。
pub fn fetch(
    base: &Base,
    method: &str,
    rest: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    deadline: Duration,
    cap: usize,
) -> std::io::Result<Fetched> {
    let mut up = connect(base, deadline)?;
    let mut head = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nAccept-Encoding: identity\r\nConnection: close\r\n",
        base.upstream_target(rest),
        base.host_header()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    up.write_all(head.as_bytes())?;
    up.write_all(body)?;
    up.flush()?;
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string());
    let raw = super::http1::read_response_head(&mut up, cap)?.ok_or_else(|| bad("no answer"))?;
    let (line, headers) = super::http1::parse_response(&raw).ok_or_else(|| bad("not http"))?;
    let mut view = super::http1::BodyView::for_response(&headers);
    let mut got = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = match up.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        got.extend_from_slice(&view.feed(&buf[..n], cap));
        if got.len() > cap || view.take_dropped() > 0 {
            return Err(bad("answer too large"));
        }
    }
    Ok(Fetched {
        status: super::http1::status_code(&line).unwrap_or(0),
        headers,
        body: got,
    })
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/upstream_tests.rs"]
mod tests;
