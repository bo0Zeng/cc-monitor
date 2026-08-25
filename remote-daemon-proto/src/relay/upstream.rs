//! 上游那一跳：连出去、把请求原样递上去。
//!
//! # 为什么这里有 TLS，而服务端那半仍是手写的
//!
//! `裁-1`（PM 08-25）**只准 TLS**：上游是 `https://…`，而 **TLS 不能手写**。
//! 唯一能绕开它的形态是 CONNECT 隧道（不解密直通），但那样 **tee 不到明文**，
//! 而 tee 正是这件事的全部意义。⇒ TLS 是必要条件，不是一次自由选择。
//!
//! 选 `rustls`（`ring` provider）而不是 `ureq`/`hyper`/`reqwest` 的读数，逐条：
//!
//! | 候选 | 逐块透传做得到吗 | 传递依赖 | 离线构建 |
//! |---|---|---|---|
//! | `rustls` + 手写 HTTP/1.1 | ✔ **控制权最完整** —— `StreamOwned` 就是一个 `Read`，每次 `read` 拿多少给多少 | 本仓实测 +22 条锁条目，其中 10 条是 `windows*`（不在 Linux 上构建） | 破（`rustls` 本机缓存里没有）；`ring` **已在缓存里** |
//! | `ureq` | ✔（`into_reader()` 是流式） | 它自带一棵 `rustls` + URL/编码等 | 破，且更宽 |
//! | `hyper`/`reqwest` | ✔ 但 | ⛔ **被 `K8`/`D4` 明令排除**（把「一个块」变成一层框架） | — |
//! | 起 `curl` 子进程 | ✔ | ⛔ 给任意远端加一条外部二进制依赖 + 要动起进程登记表 | — |
//!
//! ⚠ **不拿流行度当理由**：卡口是 `K9` 裁定二那四条硬要求，尤其「逐块透传绝不缓冲」。
//! `rustls` 过这一条是因为它**不替你做 HTTP** —— 没有任何一层会替你把响应体攒完。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

/// 上游基址。`https` 走 TLS，`http` 走明文（**明文只给本机夹具用**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Base {
    pub(crate) tls: bool,
    pub(crate) host: String,
    pub(crate) port: u16,
}

impl Base {
    pub(crate) fn parse(url: &str) -> Option<Base> {
        let (scheme, rest) = url.split_once("://")?;
        let tls = match scheme {
            "https" => true,
            "http" => false,
            _ => return None,
        };
        let authority = rest.split('/').next()?;
        if authority.is_empty() {
            return None;
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()?),
            None => (authority.to_string(), if tls { 443u16 } else { 80u16 }),
        };
        if host.is_empty() {
            return None;
        }
        Some(Base { tls, host, port })
    }

    /// `Host:` 头该写什么（默认端口不写端口，非默认端口要写）。
    pub(crate) fn host_header(&self) -> String {
        let default = if self.tls { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// 一条上游连接。两个变体都实现 `Read`/`Write` ⇒ 转发循环对 TLS 与否**一无所知**。
pub(crate) enum Conn {
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

/// 系统根证书 —— 用 `webpki-roots` 内嵌的那套（不读机器上的证书目录：
/// 本 crate 会被推到任意远端，那台机器上有什么我们不知道）。
fn tls_config() -> Arc<rustls::ClientConfig> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("rustls 默认协议版本集应当可用")
    .with_root_certificates(roots)
    .with_no_client_auth();
    Arc::new(cfg)
}

pub(crate) fn connect(base: &Base) -> std::io::Result<Conn> {
    let tcp = TcpStream::connect((base.host.as_str(), base.port))?;
    // ★ Nagle 两个方向都要关。参考实现登记过：没关会让 p95 塌到 3504ms。
    tcp.set_nodelay(true)?;
    if !base.tls {
        return Ok(Conn::Plain(tcp));
    }
    let name = rustls::pki_types::ServerName::try_from(base.host.clone())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let conn = rustls::ClientConnection::new(tls_config(), name)
        .map_err(|e| std::io::Error::other(e))?;
    Ok(Conn::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_two_schemes_and_their_default_ports() {
        assert_eq!(
            Base::parse("https://api.example.com"),
            Some(Base {
                tls: true,
                host: "api.example.com".to_string(),
                port: 443
            })
        );
        assert_eq!(
            Base::parse("http://127.0.0.1:18789"),
            Some(Base {
                tls: false,
                host: "127.0.0.1".to_string(),
                port: 18789
            })
        );
    }

    #[test]
    fn rejects_shapes_it_does_not_understand() {
        // 分母 = 我列出的这 5 形。
        for bad in ["ftp://x", "api.example.com", "https://", "://x", "https://:443"] {
            assert!(Base::parse(bad).is_none(), "这一形不该被接受：{bad}");
        }
    }

    #[test]
    fn host_header_omits_the_default_port_only() {
        let d = Base::parse("https://api.example.com").expect("d");
        assert_eq!(d.host_header(), "api.example.com");
        let n = Base::parse("http://127.0.0.1:18789").expect("n");
        assert_eq!(n.host_header(), "127.0.0.1:18789");
    }

    /// TLS 那条路本轮**没有任何行为验证**（不许打真 API，本机也没有 HTTPS 夹具）。
    /// 这条只证明：根证书装得进去、provider 选得中、`ClientConnection` 建得起来。
    /// **它不证明**握手成功、证书校验正确、或逐块透传在 TLS 上成立。
    #[test]
    fn tls_client_config_builds_and_carries_roots() {
        assert!(
            !webpki_roots::TLS_SERVER_ROOTS.is_empty(),
            "内嵌根证书集不该是空的"
        );
        let name = rustls::pki_types::ServerName::try_from("api.example.com".to_string())
            .expect("域名应当合法");
        let conn = rustls::ClientConnection::new(tls_config(), name);
        assert!(conn.is_ok(), "TLS 客户端连接对象应当建得起来");
    }
}
