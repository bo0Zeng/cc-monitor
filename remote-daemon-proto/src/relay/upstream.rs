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
use std::time::Duration;

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

/// 装进 `ClientConfig` 的那一份根证书集 —— 用 `webpki-roots` 内嵌的那套
/// （不读机器上的证书目录：本 crate 会被推到任意远端，那台机器上有什么我们不知道）。
///
/// ★ 它为什么被抽成一个**有名字的生产段**（回修轮 08-25，D1 `重要-2`）：
/// 先前这一步是 `tls_config()` 里的一个匿名字面量，而判据断的是 **crate 常量**
/// `webpki_roots::TLS_SERVER_ROOTS` 非空 —— 那是**另一个东西**。
/// ⇒ 把这里的根证书集整个换成空 `Vec::new()`，384 条判据**全绿**（审计 `CS` 实测）。
/// 抽出来之后，判据打得到的就是**真正装进去的那一份**。
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

/// 上游那条 socket 的**读写期限**〔回修轮之六 08-25，D3 `阻-3(D3)` 的**后半段**〕。
///
/// # 它不是定时器（这句话就是 `no_timer_guard` 那张表里登记的那一行）
///
/// `SO_RCVTIMEO` / `SO_SNDTIMEO` 说的是「**这一次**阻塞的读/写最多等多久」：
/// 有字节就**立刻**返回，没字节就**报错**返回。它不会让任何线程**自己醒来**，
/// 也不产生任何节拍 —— 这正是零定时器护栏禁的那一类与它的分界。
/// ⭐ 配套的硬约束：**期限到了就把连接结掉，不允许任何一层重试** ——
/// 一重试它就从「阻塞有上限」变成「轮询」，而轮询正是护栏要防的东西。
/// 今天这一条靠的是：`pump` 与 `http1` 里的读循环**只**对 `Interrupted`（EINTR）`continue`，
/// 其余错误一律 `return Err`；`rustls` 的 `complete_io` 同形（只重试 `Interrupted`）。
///
/// # 值为什么是 600 秒，而不是下游那个数
///
/// 这一跳等的是**模型在想** —— 上游几十秒不发一个字节是 **SSE 长流的正常形态**，
/// 不是卡死。600 秒这个数**不是我拍的**：`super` 头注逐字记着「参考实现给上游 600 秒」，
/// 说的正是同一跳。
///
/// ⚙ **设错会怎样**：把它改小（比如照抄下游那 30 秒）会把一条**正在正常吐字、
/// 只是中间想了 40 秒**的长流从中间提断，客户端拿到半条回答
/// ⇒ **比不设期限更坏**（不设的话那条流是能走完的）。这是本格最贵的一种错。
/// 改大则是：一条死掉但没发 FIN 的上游（NAT/conntrack 丢连接）会多钉住一条线程那么久。
///
/// ⚙ **我刻意不拿「Anthropic 的 SSE 会周期发 `ping`」当依据**：那要打真 API 才量得到，
/// 而 `C7` 逐字禁「绝不起真 claude」⇒ 这个数必须在「上游合法地整段沉默」的前提下也站得住。
///
/// # 它**没**盖住的那一步：`connect` 本身
///
/// 下面 `TcpStream::connect` **没有**连接期限。本轮故意不做：读写是**真无界**
/// （对端不发就永远不返回），而 connect 那一步有内核 SYN 重试上限与解析器自己的上限兜着。
/// ⚙ **那个上限具体多少我没量** ⇒ 只敢说「不是无界」，不敢说「够小」。
pub(crate) const UPSTREAM_DEADLINE: Duration = Duration::from_millis(600_000);

pub(crate) fn connect(base: &Base) -> std::io::Result<Conn> {
    let tcp = TcpStream::connect((base.host.as_str(), base.port))?;
    // ★ Nagle 两个方向都要关。参考实现登记过：没关会让 p95 塌到 3504ms。
    tcp.set_nodelay(true)?;
    // ★★ 读写期限：没有它，一条死了但没发 FIN 的上游会把一条连接线程**永久**钉在
    //    `pump` 里那句 `up.read`（或 `read_response_head`）上。两个方向都要：
    //    写那半钉的是 `up.write_all(&body)`（请求体最大 `BODY_CAP` = 64 MiB，远超 socket 发送缓冲）。
    tcp.set_read_timeout(Some(UPSTREAM_DEADLINE))?;
    tcp.set_write_timeout(Some(UPSTREAM_DEADLINE))?;
    if !base.tls {
        return Ok(Conn::Plain(tcp));
    }
    let name = rustls::pki_types::ServerName::try_from(base.host.clone())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let conn =
        rustls::ClientConnection::new(tls_config(), name).map_err(|e| std::io::Error::other(e))?;
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
        for bad in [
            "ftp://x",
            "api.example.com",
            "https://",
            "://x",
            "https://:443",
        ] {
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
    ///
    /// # 名字只承诺它证得了的那一半（回修轮 08-25 改名，D1 `重要-2`）
    ///
    /// 旧名 `tls_client_config_builds_and_carries_roots` 里的 **carries roots** 是**空真**：
    /// 它断的是 crate 常量 `webpki_roots::TLS_SERVER_ROOTS` 非空，**不是** `tls_config()`
    /// 真把根装了进去 ⇒ 把装配点的根证书集换成空 `Vec::new()`，它照样全绿（审计 `CS`）。
    ///
    /// 今天它断的是**生产段的装配函数** `root_store()` 的**根证书条数 > 0**，
    /// 而 `tls_config()` 里那一份就是它返回的那一份（唯一调用点，就在上面几行）。
    ///
    /// **它仍然不证明**：握手成功 · 证书校验真的按这套根做 · 逐块透传在 TLS 上成立。
    /// 那三样要真 TLS 行为验，本轮禁打真 API ⇒ 登记为 `判不了`，别再让名字替它们背书。
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
}
