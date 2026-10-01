//! 上游那一跳：连出去、把请求原样递上去。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔（ 的剩余），2026-09-22〕
//!
//! 这一枚标记是**盘上那一侧**的凭据（登记那一侧在
//! `tests/frontend/shell/comm_boundary_registry_tests.rs::REGISTERED`，两向集合相等）。
//! 盖上它 = **上锁**，不是放行：本文件从此被 `C1`–`C5` ＋ `X1`–`X6` 十一条一起管着。
//!
//! **凭什么它属于通信层**：那张图里**面 B（agent ↔ 上游 API）本来就在
//! 通信层内**，而本文件第一行逐字「上游那一跳：连出去、把请求**原样**递上去」——
//! 那就是**搬字节**。它既不是上游选择（那是 `accounts/`），也不是一张登记表
//! ⇒ 它不属于那个「十一条全绿而**不该**圈」的形状。
//!
//! **它先前差哪一条**：只差 `X2`（生产段零期限字面量）—— 那个 600 秒的**值**
//! 住在本文件里。`P16` 把它按（「**期限值**全部由后端交给它」）
//! 搬去 `listen.rs`，本文件只剩「收一个期限、装到 socket 上」⇒ 十一条现打全绿。
//! ⚠ **装它的那一手仍在本文件**（`connect` 收入参）——
//! 「**值归后端 · 执行归通信层**」，搬走的只有值。
//!
//! ⚠ **这里有 TLS 与 tee 明文，而十一条对它仍然成立，没有开任何豁免**（`C1` 的豁免
//! 必须为零）。两件事分开看：TLS 是**怎么搬**（`裁-1`：上游是 `https://`，而 TLS
//! 不能手写），tee 是**搬完顺手抄一份**（`§4.1`：本层四件事的第四件）——
//! 两者都不需要本文件认识会话/账号/agent，它的公开面上一个业务名都没有。
//! 🔴 **它买不到的**：本标记**不买**「TLS 配得对」（那由 `tls_config` 自己的判据与
//! `wire_golden` 负责），也**不买**「tee 到的明文不该落盘」（那是 `creds_guard` 的活）。
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
/// # ⚠ 它为什么必须带一句话，而不是一个 `None`
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
    /// 「base URL 能不能用」全仓只有一份，住共享 crate `upstream_url_core`（协议闭集 `SCHEMES` 也在那儿）；
    /// 这里只把它的形状结论装成 `Base`、把理由翻成一句话（`K-R1`：理由来自解析器，逐形一句）。
    pub(crate) fn parse(url: &str) -> Result<Base, BaseIssue> {
        let u = upstream_url_core::parse(url).map_err(BaseIssue::of)?;
        Ok(Base {
            tls: u.tls,
            host: u.host,
            port: u.port,
            path: u.path,
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

    /// **上游请求行的目标** = 这个基址的路径前缀 + 客户端那份真路径。
    ///
    /// # ⚠⚠ 它是本件**两处**会改变发出去的字节的地方之一，射程逐条写清
    ///
    /// 另一处是 `server.rs` 换头那一行（`auth_header_of` 那一支）。
    /// ⚠ **这一句是订正**：本节初稿逐字写的是「本件**唯一**一处会改变发出去的字节的地方」，
    /// 而那是一句**假的全称** —— 换头那一处也在改字节，而且就是本件另一半的正主。
    /// 〔`brief` 12：写「唯一 / 全部」这类话也是在报一个数，同句给分母。自查逮到，09-04。〕
    ///
    /// - `rest` 是**下游原样**的「真路径 + 查询串」（`route::parse` 保证它以 `/` 打头）。
    ///   本函数**一个字节都不改它**，只在**前面**接上这个基址自己的前缀。
    /// - 前缀是 `""` 时，返回值与 `rest` **逐字节相同** ⇒ 没配前缀的那一路**零字节改动**。
    ///   ⚠ **这里刻意不写「盘上有几条字面量是这一路」那个数**：初稿抄了摸底那一拍的
    ///   「全部 9 条」，而**那一件自己新加的判据里就有带路径的字面量** ⇒ 那个数在
    ///   写下它的同一个 commit 里就馊了。要现打就跑
    ///   `tests/evidence/K-R1-B1-auth-style-and-base-path-census.py` 的第 ⑦c/⑦d 格
    ///   （它自己印两个分母：生产段 / 含测试段）。〔`brief` 13：别抄快照，指住址。〕
    /// - 🔴 **它不查重、不合并重复的段**：配 `https://h/v1` 而客户端发 `/v1/messages`
    ///   的人会得到 `/v1/v1/messages`。**这是有意的** —— 「顺手把重复的段合掉」
    ///   要先猜出「哪一段是重复」，而猜错的症状是**静默打到另一个地方**。
    ///   ⇒ 处置是**出声**（装表时给这一行记一条 `Note`，见 `table::NOTE_PATH_PREFIX`），
    ///   不是替人重写他写下的东西。
    /// - ⚠ **`K-R1` 之前这个函数不存在**，前缀在 [`Base::parse`] 里就被丢掉了
    ///   ⇒ `https://h/v1` **碰巧是对的**。那一件把「碰巧对」换成「照写的做 + 出声」。
    ///
    /// ⚠⚠ **它先前是 `table::Row` 的方法**（搬到这里）。搬的理由：
    /// 「前缀 + 真路径」是**传输原语**，`Base` 自己就答得了；而 `Row` 是上游选择的东西，
    /// 中转拿到的是 `Destination` 里那个 `&Base`，够不到 `Row`。
    /// 实现仍然**只有这一份**（`Row` 那边不再有第二份）。
    pub(crate) fn upstream_target(&self, rest: &str) -> String {
        format!("{}{}", self.path, rest)
    }
}

impl BaseIssue {
    /// 形状那几形各一句（句子住文案表；判定住 `upstream_url_core::parse`）。
    pub(crate) fn of(i: upstream_url_core::ShapeIssue) -> BaseIssue {
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

/// **`UPSTREAM_DEADLINE` 的那个值搬去 `listen.rs` 了** —— 墓碑。
///
/// 理由与下游那条逐字同一条（把「期限值」算进「全部由后端交给它」·
/// 「值归后端 · 执行归通信层」）。装它的那一手还在 [`connect`] 里，
/// 只是改成收一个入参。
/// 🔴 **这一格是本文件圈进通信层边界的最后一道**：`X2`（生产段零期限字面量）先前
/// 只咬它这一处，搬走之后十一条对它全绿 ⇒ 本文件头注盖上了那枚成员标记（见文件第一节）。
///

pub(crate) fn connect(base: &Base, deadline: Duration) -> std::io::Result<Conn> {
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

#[cfg(test)]
#[path = "../../../tests/comms/outward/upstream_tests.rs"]
mod tests;
