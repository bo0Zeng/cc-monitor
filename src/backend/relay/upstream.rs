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

/// 一条 `base_url` **进不了 `Base`** 的理由。
///
/// # ⚠ 它为什么必须带一句话，而不是一个 `None`〔`K-R1`〕
///
/// 先前 [`Base::parse`] 回 `Option`，于是「不是个 URL」「协议不认识」「端口读不懂」
/// 「路径里带查询串」全挤在同一个 `None` 里，而调用方只能印一句**万能的**
/// 「base_url 解析不了（要 https:// 或 http://）」——那句话在后三形上都是**假的指引**。
/// 〔`K-R21` 那一族逐字：一个 `None` 装了几件事，而它在生产路上。〕
///
/// ⚠ 里面那句话是 `&'static str`（**不含文件内容**）⇒ 它进日志是安全的，
/// 与 `table::Rejected::why` 同一条理由，也正是那个字段的类型能直接收它的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BaseIssue(pub(crate) &'static str);

/// 上游基址。`https` 走 TLS，`http` 走明文。
///
/// # ⚠⚠ 「明文只给本机夹具用」这句话，`K-R1` 之后**变了半格**
///
/// 〔用 09-04〕逐字要「api做成通用的, 还可以接本地部署的」⇒ 本机跑的推理服务
/// （回环上的 http 明文）从「夹具的特权」升成**一等公民**。
/// ⚠ 而 `裁-1`（PM 08-25）「只准 TLS」那一条**没有被推翻**：升的只有**回环**这一格。
/// 「明文 + 非回环」= 一把 key 明着过网线 ⇒ 由 `table::build` **拒掉并出声**
/// （判据 `a_plaintext_upstream_is_only_allowed_on_loopback`）。
/// 本结构体自己**不判**这一条：它只答「这个串长什么样」，不答「许不许用」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Base {
    pub(crate) tls: bool,
    pub(crate) host: String,
    pub(crate) port: u16,
    /// 基址里那一段**路径前缀**，`""` = 没有。带前导 `/`、**不带**尾随 `/`。
    ///
    /// # ★★ 它为什么必须存在（`K-R1` 摸底那一格，PM 09-04 复核过）
    ///
    /// 先前本结构体只有 `tls`/`host`/`port` 三个字段 ⇒ `Base::parse` 拿
    /// `rest.split('/').next()` **只取 authority**，路径**被丢掉、且照样回 `Some`**，
    /// 装表那一步不记 `Rejected`、`announce` 一个字都不说。
    ///
    /// 而它**只在第三种配法下才错**，这正是它危险的原因：
    /// `https://host` 对；`https://host/v1` 也**碰巧对**（丢掉的 `/v1` 客户端自己带着）；
    /// `https://host/<网关前缀>` **静默打到别的地方**。
    /// ⇒ 「在两种常见配法下都工作」的东西没有人会去怀疑。
    ///
    /// 而带前缀的形状**不是边角料**：第三方把「Anthropic 兼容」这一套挂在一个
    /// 前缀底下是常见做法（〔用 09-04〕点名的那几家里就有）⇒ 前缀装不下 =
    /// 那一类端点**根本配不出来**，而那正是本件要接的东西。
    ///
    /// ⚠ **它是承重的、会改变字节**：拼法与射程见 `Row::upstream_target`（住 `table.rs`），
    /// 拼出来的东西由 `the_path_prefix_from_the_base_url_really_reaches_the_request_line`
    /// 钉住。〔这里只存值，不拼。〕
    pub(crate) path: String,
}

impl Base {
    /// 认得的两种协议。**闭集只有这一个住址**〔`brief` 13b〕：
    /// 报错文案与判据都从这里派生，不许再写第二份字面量。
    pub(crate) const SCHEMES: &'static [(&'static str, bool)] = &[("https", true), ("http", false)];

    pub(crate) fn parse(url: &str) -> Result<Base, BaseIssue> {
        let Some((scheme, rest)) = url.split_once("://") else {
            return Err(BaseIssue(
                "base_url 不是一个 URL（要 https:// 或 http:// 打头）",
            ));
        };
        let Some((_, tls)) = Base::SCHEMES.iter().find(|(s, _)| *s == scheme) else {
            return Err(BaseIssue(
                "base_url 的协议不认识（只认 https:// 与 http://）",
            ));
        };
        let tls = *tls;
        // ★ 这一行是本格的正主：authority 与**路径**从这里分家，
        //   而先前那一版把后半截整个扔了。
        let (authority, raw_path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        if authority.is_empty() {
            return Err(BaseIssue("base_url 里没有主机名"));
        }
        // ⚠ 查询串**没有路径也塞得进来**（`https://h?x=1` 里 authority 逐字是 `h?x=1`）
        //   ⇒ 这一格不查的话，那一形会被当成一个叫 `h?x=1` 的主机名接受下来。
        if authority.contains('?') || authority.contains('#') {
            return Err(BaseIssue(
                "base_url 里带了查询串或 # 片段 —— 基址只能是「协议 + 主机 + 可选的路径前缀」",
            ));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse::<u16>()
                    .map_err(|_| BaseIssue("base_url 的端口读不懂（要 0-65535 的十进制数）"))?,
            ),
            None => (authority.to_string(), if tls { 443u16 } else { 80u16 }),
        };
        if host.is_empty() {
            return Err(BaseIssue("base_url 里没有主机名"));
        }
        Ok(Base {
            tls,
            host,
            port,
            path: normalize_prefix(raw_path)?,
        })
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

    /// 这个基址指的是**本机回环**吗〔`K-R1`：本地部署那一格〕。
    ///
    /// # ⚠ 分母如实写 —— 它认两类，第二类是**约定**不是保证
    ///
    /// 1. **能解析成 IP 的**（含 `[::1]` 那种带方括号的写法）⇒ 走
    ///    `IpAddr::is_loopback`，那是标准库按 RFC 判的，`127.0.0.0/8` 与 `::1` 都算。
    /// 2. **逐字是 `localhost`** ⇒ 算。⚠ 它算回环靠的是「解析器把这个名字解到回环」，
    ///    那是一条**约定**（`/etc/hosts` 与各平台的内建规则），**不是**本条证得了的事实。
    ///    有人在 `hosts` 里把 `localhost` 指到别处，本条就说错了。**如实记，不假装。**
    ///
    /// ⇒ 别的名字（`my-box.local` / 一个真解到 `127.0.0.1` 的域名）本条一律说**不是**：
    /// 那要 DNS 才判得了，而这里在**装表**那一刻跑（没起任何网络）。
    /// 宁可把一条其实安全的配法拒掉并出声，不许把一条明文过网线的放行。
    pub(crate) fn host_is_loopback(&self) -> bool {
        let h = self.host.trim_start_matches('[').trim_end_matches(']');
        match h.parse::<std::net::IpAddr>() {
            Ok(ip) => ip.is_loopback(),
            Err(_) => h.eq_ignore_ascii_case("localhost"),
        }
    }
}

/// 把 `base_url` 里那一截原始路径收成一个**前缀**：带前导 `/`、不带尾随 `/`、`""` = 没有。
///
/// # ⚠ 它只做「去掉没有意义的尾巴」，不做任何**猜测**
///
/// 与 `store::read_key` 那条纪律同源（逐字：人写进去什么，上游就该收到什么）：
/// 尾随的 `/` 在一个**前缀**里不携带信息（拼上去只会多一个空段），去掉是安全的；
/// 而「顺手补一段 `/v1`」「把重复的段合掉」这类清理**一律不做** —— 猜错一次的代价
/// 是**静默打到另一个地方**，而那正是本格在治的病。
///
/// # 三形拒掉，逐形给理由（**都出声**，不许静默吞掉）
///
/// - **带 `?` 或 `#`**：查询串与片段是**这一次请求**的东西，不是基址的。
///   放进来的话它会被拼在客户端真路径的**前面** ⇒ 拼出一个谁都不认识的目标。
/// - **`//` 打头**：拼出来的请求行会以 `//` 起首，那在 HTTP 里读作 authority
///   ⇒ 一个基址里的手滑变成「把请求发到别处」。〔`K-R9` 那一族：剥法与 `//`〕
/// - ⚠ **中间的空段**（`/a//b`）**不拒**：那是人写下的东西，原样带着。如实记为射程外。
fn normalize_prefix(raw: &str) -> Result<String, BaseIssue> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    if raw.contains('?') || raw.contains('#') {
        return Err(BaseIssue(
            "base_url 里带了查询串或 # 片段 —— 基址只能是「协议 + 主机 + 可选的路径前缀」",
        ));
    }
    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() {
        // 整段就是一个或多个 `/` ⇒ 它说的是「根」，等价于没有前缀。
        return Ok(String::new());
    }
    if trimmed.starts_with("//") {
        return Err(BaseIssue(
            "base_url 的路径前缀以 // 打头 —— 那在 HTTP 请求行里读作另一个主机名",
        ));
    }
    Ok(trimmed.to_string())
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
            Ok(Base {
                tls: true,
                host: "api.example.com".to_string(),
                port: 443,
                path: String::new()
            })
        );
        assert_eq!(
            Base::parse("http://127.0.0.1:18789"),
            Ok(Base {
                tls: false,
                host: "127.0.0.1".to_string(),
                port: 18789,
                path: String::new()
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
            assert!(Base::parse(bad).is_err(), "这一形不该被接受：{bad}");
        }
        // ★ 非空对照排在后面也够（上面几形都是 `Err`，尺子不可能恒 `Err` 还让这一句过）。
        assert!(Base::parse("https://ok.example").is_ok(), "这把尺子是瞎的");
    }

    /// ★★★ **`K-R1` 的正主之一**：`base_url` 里那一段路径**装得下了**，
    /// 而先前它被 `rest.split('/').next()` 整个丢掉、且照样回 `Some`。
    ///
    /// # 死值验就在这条判据的第一格上
    ///
    /// 第一格逐字是 PM 补充里点名的那个读数：`Base::parse("https://h:443/v1")`。
    /// **改前**它回 `Some(Base{host:"h",port:443})`，`/v1` 静默消失、没有任何东西出声；
    /// **改后**那一段留在 `path` 里。⇒ 把 [`normalize_prefix`] 的返回改成
    /// 恒 `Ok(String::new())`（形状对、恒答「没有前缀」那张脸），本条当场红。
    #[test]
    fn the_path_part_of_a_base_url_is_kept_instead_of_being_silently_dropped() {
        // 分母 = 我列出的这 7 形，逐形手写期望值。
        let cases: &[(&str, &str)] = &[
            ("https://h:443/v1", "/v1"),
            ("https://vendor.example/anthropic", "/anthropic"),
            ("http://127.0.0.1:11434/v1", "/v1"),
            ("https://h/openai/v1", "/openai/v1"),
            // 尾随的 `/` 在一个前缀里不携带信息 ⇒ 去掉。
            ("https://h/v1/", "/v1"),
            // 整段就是一个 `/` ⇒ 说的是「根」，等价于没有前缀。
            ("https://h/", ""),
            ("https://h", ""),
        ];
        for (url, want) in cases {
            let b = Base::parse(url).unwrap_or_else(|e| panic!("{url} 该解析得了：{e:?}"));
            assert_eq!(&b.path, want, "这一形的前缀取错了：{url}");
        }
        // ★ 非空对照承重：这把尺子**分得出**「有前缀」与「没前缀」
        //   （没有这一格，上面那两条 `""` 可能只是因为它恒回空串）。
        assert_ne!(
            Base::parse("https://h/v1").expect("有前缀那一形").path,
            Base::parse("https://h").expect("没前缀那一形").path
        );
    }

    /// **`base_url` 里丢东西要出声** —— 三形各自的理由**不许挤进同一个 `None`**。
    #[test]
    fn a_base_url_that_carries_things_a_prefix_cannot_carry_says_why() {
        // 分母 = 我列出的这 4 形。
        for bad in [
            "https://h/v1?beta=true",
            "https://h/v1#frag",
            "https://h//v1",
            "https://h:99999/v1",
        ] {
            assert!(Base::parse(bad).is_err(), "这一形不该被接受：{bad}");
        }
        // ★★ 理由**逐形不同**：四个理由串放进集合去重之后必须还是 4 个。
        //    这一格才是「一个 None 装了几件事」被治掉的读数 —— 只断 `is_err()` 的话，
        //    把每一支的理由都换成同一句，本条照样全绿。
        let mut whys: Vec<&'static str> =
            ["ftp://x", "https://", "https://h/v1?x=1", "https://h//v1"]
                .iter()
                .map(|u| Base::parse(u).expect_err("这几形都该是 Err").0)
                .collect();
        let n = whys.len();
        whys.sort();
        whys.dedup();
        assert_eq!(
            whys.len(),
            n,
            "有两形共用了同一句理由 —— 那句话在其中一形上是假的指引：{whys:?}"
        );
    }

    /// `K-R1`：**本地部署那一格的谓词** —— 回环认得出来，别的一律说「不是」。
    ///
    /// ⚠ 它只是**谓词**；「明文非回环要不要拒」是 `table::build` 的决定，判据在那边。
    #[test]
    fn the_loopback_predicate_says_yes_only_to_the_local_machine() {
        // 分母 = 我列出的这 4 形回环写法。
        for yes in [
            "http://127.0.0.1:11434",
            "http://127.0.0.1",
            "http://localhost:8000/v1",
            "http://[::1]:11434/v1",
        ] {
            assert!(
                Base::parse(yes).expect("该解析得了").host_is_loopback(),
                "这一形是回环，却被说成不是：{yes}"
            );
        }
        // ★ 非空对照 + 单断：分母 = 我列出的这 4 形非回环写法。
        for no in [
            "http://1.2.3.4/v1",
            "https://api.example.com",
            "http://10.0.0.1:8000",
            // ⚠ 一个**名字**里含 localhost 不算 —— 它解到哪儿本条判不了。
            "http://localhost.evil.example",
        ] {
            assert!(
                !Base::parse(no).expect("该解析得了").host_is_loopback(),
                "这一形不是回环，却被说成是：{no}"
            );
        }
    }

    #[test]
    fn host_header_omits_the_default_port_only() {
        let d = Base::parse("https://api.example.com").expect("d");
        assert_eq!(d.host_header(), "api.example.com");
        let n = Base::parse("http://127.0.0.1:18789").expect("n");
        assert_eq!(n.host_header(), "127.0.0.1:18789");
        // ★ `K-R1`：`Host:` 头里**没有**路径前缀那一段（它属于请求行，不属于这里）。
        let p = Base::parse("https://api.example.com/anthropic").expect("p");
        assert_eq!(p.host_header(), "api.example.com");
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
