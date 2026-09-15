//! `K-W2D` 接线那一拍：**真正那一跳** —— HTTP GET · 校验 · 写盘 · 起进程。
//!
//! [`super::fetch`] 那一半是**协议与判定**（纯函数，一个进程都不起、一个字节都不落盘）。
//! 本模块是它缺的另一半：**真的去拿、真的落盘、真的起起来**。两半的分界线只有一条 ——
//! 凡是「答一个问题」的都在那边，凡是「动世界」的都在这边。
//!
//! # 一 · 「对端」是入参，这是本模块唯一的架构决定
//!
//! 门禁沙箱一律断网（`--network none`），红线也不许起真 daemon
//! ⇒ 「这条路能不能真拉下来」**出了沙箱就验不了**。
//! 但「验不了」不等于「不测」：把**对端**做成入参（[`Origin`]）之后，
//! HTTP 那一跳可以打在**同机回环上的一台真服务器**上 —— 真开 socket、真写请求、
//! 真解析状态行与头、真按上限收字节。断网容器里 `lo` 是通的，本轮实测过。
//!
//! ⇒ 三跳各自被真的走到一次，用的都是**生产代码本身**，不是它的复刻：
//! HTTP 那一跳打回环、写盘那一跳打临时目录、起进程那一跳起一个临时可执行文件。
//!
//! ⚠ **它买不到什么，一句话写死**：**带网那一跳仍然一格都没买到。**
//! 回环上那台服务器是我们自己写的，它证不了 release 上真挂着那个资产、
//! 证不了 TLS 那一段握得起来、证不了 DNS 解得开。那一格归 PM 另排。
//!
//! # 二 · 上限那个数从这一拍起**有住址**了
//!
//! [`ASSET_BYTE_CAP`] 就是它，同轮登记进 monitor 那棵树的 `byte_cap_registry`。
//! [`super::fetch::within_cap`] 的头注逐字记着这笔债（「今天这条路上的上限没有住址」）——
//! 那句话从本拍起不成立，它的订正也写在那儿。
//!
//! # 三 · 三条**如实登记的代价**（都不是漏了，是今天做不到）
//!
//! 1. 🔴 **本层没有任何期限** —— 连不上会一直等，一台不说话的服务器会把这个线程钉住。
//!    加期限要 `Duration`，而 `no_timer_guard::REGISTERED_DURATION_USES` 是**相等断言**、
//!    那份文件不在本轮写区 ⇒ 加一处就红一条我修不了的判据。**走上报口。**
//! 2. 🔴 **落点目录不存在时本层建不出来** —— 只读护栏的白名单层逐字禁掉了建目录那个动词
//!    （它比默认层更严：**只准新增文件，不许改动既有数据**）。
//!    ⇒ 目录不在就只有一句 [`super::fetch::Face::LandingIo`]。建目录是**别人的活**。
//! 3. 🔴 **落点上躺着一份 0 字节的残骸时，本层修不好它** —— 判定那一半会说「去拉」，
//!    而拉回来落不下去：新建走的是 `O_EXCL`，撞上既有文件就失败，而删 / 改名 / 截断
//!    **三个动词全被白名单层禁着**。⇒ 出声（同上一格那张脸，那句话把这一种也说出来了）。
//!    这是「daemon 不许改动用户既有数据」这条性质的**直接代价**，不是缺陷。
//!
//! # 四 · 为什么这里自己开 socket，而不是去用中转那一层已有的那份
//!
//! 中转（`relay/`）里确实已经有一份「解析基址 · 开连接」。**不许拿过来用**，两条理由都是现打的：
//!
//! - `relay/table_guard` 有一条**相等断言**：开上游连接那个调用点全 crate **恰好一处**，
//!   而且必须在中转的路由表里。第二处当场红 —— 那条判据要的正是「连到哪儿这个决定收在表里」。
//! - `layering_guard` 那条「别处不许伸手进 `relay/` 内部」的人群只有 `observe/` 与 `control/`
//!   两层，**本层不在人群里** ⇒ 伸手进去不会红。**那是它自己写明的射程洞，不是许可。**
//!
//! ⚠ 代价如实记：本层因此**第二次**从 `webpki-roots` 那个 crate 常量装了一份根证书集。
//! 两处装的是**同一个**常量（那个常量本身仍是唯一住址），但「怎么装」有了两份。
//! 判据 [`crate::sidecar_fetch_guard`] 钉住本层这一份非空 —— 中转那份被人整个换成空集时
//! 384 条判据全绿，那次的教训在这里也算数。**真正的解法是把它抽进 `common/`，而
//! `common/` 不在本轮写区 ⇒ 上报口。**

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use crate::plugin::invoke::{self, Done, NotRun};

use super::fetch::{
    asset_url, classify_write_error, integrity_verdict, landing_verdict, transport_verdict,
    use_or_fetch, verify_bytes, within_cap, Checked, Face, Integrity, Landed, Landing, Pin, Step,
    Transport,
};

/// 一次拉取**最多**收多少字节。
///
/// # 它管的量、超限怎么办
///
/// 管的是「一趟 HTTP GET 收进内存的那一整份字节」；超了 **拒收 + 回错**
/// （[`Transport::Oversize`] ⇒ [`Face::Oversize`]，**一个字节都不落盘**）。
/// 同一句话在 monitor 那棵树的 `byte_cap_registry` 里登记着 —— 那张表与这里**对拍**，
/// 改这个数会让那条判据当场红，而那正是该重新想「这个量该多大」的时刻。
///
/// # 这个数怎么定的（**转述，我没重打**）
///
/// 件文件 `§4b` 记着那个二进制的实测体积（住址 `features/K-W2D-code-picture出sidecar.md#§4b`）。
/// 本上限取的是**留了几倍余量**的一档：它挡的不是「大了一点」，
/// 是「对面换了个东西」——一个 HTML 错误页、一份 tar、一次重定向到别的站。
/// ⚠ 这个数**不是**从那份读数算出来的公式值；它是一个有余量的挡板，
/// 真值哪天贴上来了，该做的是回来重想，不是把它调大。
pub const ASSET_BYTE_CAP: u64 = 64 * 1024 * 1024;

/// 一次响应的**头部**最多多少字节（不是体）。
///
/// # 🔴 它是本轮自己的测试逼出来的，不是设计出来的
///
/// 头一版只有**一个**上限，头与体共用它：`take(cap + 1)` 罩住整条响应。
/// 判据 [`tests::the_cap_bites_on_a_real_stream_too`] 当场红 —— 喂 100 字节上限 + 200 字节响应体，
/// 读回来的是 `Got(42)`：**头把体的额度吃掉了 158 字节，而剩下的 42 字节被当成了一份完整的资产。**
/// 那正是 monitor 那张表逐字排除的「**静默截断**」。
///
/// ⇒ 两个量分两个数：头有头的上限，体有体的上限（[`ASSET_BYTE_CAP`]）。
/// 一趟最多读 `本上限 + 体上限 + 1` 字节，多出来的那 1 个字节就是用来分辨
/// 「刚好读满」与「其实还有」的。
///
/// 超限怎么办：**拒收 + 回错**（[`Transport::Oversize`]，一个字节都不落盘）。
pub const RESPONSE_HEAD_BYTE_CAP: u64 = 8 * 1024;

// ───────────────────────────── 对端（入参） ─────────────────────────────

/// 「去这个地址拿一整份字节」这件事本身。**它是入参** —— 正是这一条让不出网也走得到 HTTP 那一跳。
///
/// ⚠ **契约两条，实现方必须守**：
/// ① 成功那一支回 `(Transport::Got(n), bytes)`，而 `n` 就是 `bytes.len()`；
///    `n` **只用来出声**，判定一律用 `bytes` —— 一个长度不许有两个住址。
/// ② 失败那三支回的字节**没有意义**，调用方一个都不看。
pub trait Origin {
    /// 拿一趟。`cap` 是这一趟的字节上限（生产调用方给 [`ASSET_BYTE_CAP`]）。
    fn get(&mut self, url: &str, cap: u64) -> (Transport, Vec<u8>);
}

/// 一个地址拆开之后的样子。
///
/// ⚠ 这是本 crate 里**第二份** URL 拆法（另一份在中转那一层，理由见模块头注第四节）。
/// 两份要的东西不一样：那边要的是「基址 + 前缀」，这边要的是「一整条资产地址」。
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Target {
    /// 要不要走 TLS。
    pub tls: bool,
    pub host: String,
    pub port: u16,
    /// 请求行里那一段，带前导 `/`。
    pub path: String,
}

/// 认得的两种协议。**闭集只有这一个住址**：报错文案与判据都从这里派生。
pub const SCHEMES: &[(&str, bool, u16)] = &[("https", true, 443), ("http", false, 80)];

impl Target {
    /// 把一条完整地址拆开。拆不动就回 `None` —— 本层**不猜、不补、不兜底**。
    ///
    /// ⚠ 拆不动这一格在 [`Net`] 里翻成 [`Transport::Offline`]，理由与射程写在那儿。
    pub fn parse(url: &str) -> Option<Target> {
        let (scheme, rest) = match url.split_once("://") {
            Some(v) => v,
            None => return None,
        };
        let (_, tls, default_port) = match SCHEMES.iter().find(|(s, ..)| *s == scheme) {
            Some(v) => *v,
            None => return None,
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].to_string()),
            None => (rest, "/".to_string()),
        };
        if authority.is_empty() || authority.contains('?') || authority.contains('#') {
            return None;
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => match p.parse::<u16>() {
                Ok(n) => (h.to_string(), n),
                Err(_) => return None,
            },
            None => (authority.to_string(), default_port),
        };
        if host.is_empty() {
            return None;
        }
        Some(Target {
            tls,
            host,
            port,
            path,
        })
    }

    /// `Host:` 头该写什么（默认端口不写端口）。
    pub fn host_header(&self) -> String {
        let default = match SCHEMES.iter().find(|(_, t, _)| *t == self.tls) {
            Some((_, _, p)) => *p,
            None => 0,
        };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// 这一层信谁签的证书 —— **从 `webpki-roots` 那个 crate 常量装**，不读机器上的证书目录
/// （本 crate 会被推到任意远端，那台机器上有什么我们不知道）。
///
/// 抽成一个有名字的函数，理由与中转那一层逐字同源：判据要打**真正装进去的那一份**，
/// 打那个 crate 常量是**另一个东西**（把这里换成空集，那条判据照样绿）。
pub fn trust_roots() -> rustls::RootCertStore {
    rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

/// 一条连接。两个变体都实现 `Read`/`Write` ⇒ 上面那层 HTTP 对 TLS 与否**一无所知**。
pub enum Wire {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Wire::Plain(s) => s.read(buf),
            Wire::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Wire::Plain(s) => s.write(buf),
            Wire::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Wire::Plain(s) => s.flush(),
            Wire::Tls(s) => s.flush(),
        }
    }
}

/// 开一条到这个目标的连接。
pub fn dial(t: &Target) -> std::io::Result<Wire> {
    let tcp = TcpStream::connect((t.host.as_str(), t.port))?;
    if !t.tls {
        return Ok(Wire::Plain(tcp));
    }
    let cfg = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(std::io::Error::other)?
    .with_root_certificates(trust_roots())
    .with_no_client_auth();
    let name = match rustls::pki_types::ServerName::try_from(t.host.clone()) {
        Ok(n) => n,
        Err(e) => return Err(std::io::Error::other(e)),
    };
    let conn = match rustls::ClientConnection::new(std::sync::Arc::new(cfg), name) {
        Ok(c) => c,
        Err(e) => return Err(std::io::Error::other(e)),
    };
    Ok(Wire::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

/// 状态行里那个码。读不出来就回 `None` —— **不许当成 200**。
pub fn status_of(head: &[u8]) -> Option<u16> {
    let line = match head.split(|b| *b == b'\r' || *b == b'\n').next() {
        Some(l) => l,
        None => return None,
    };
    let text = String::from_utf8_lossy(line);
    let mut parts = text.split(' ');
    match parts.next() {
        Some(v) if v.starts_with("HTTP/") => v,
        _ => return None,
    };
    match parts.next() {
        Some(c) => c.parse::<u16>().ok(),
        None => None,
    }
}

/// **HTTP/1.1 那一跳的本体** —— 在一条已经开好的流上发一次 GET，把响应体收回来。
///
/// 手写而不引 HTTP 客户端：本 crate 那条「不引 HTTP 框架」的白名单一个字没松，
/// 中转那一层的 HTTP/1.1 也是手写的（形状同源）。
///
/// ⚠ **它认得的响应形状写死在这里，别读大一格**：只认 `Content-Length` 与「读到 EOF 为止」
/// 这两种。`Transfer-Encoding: chunked` **不认** —— 认它要一个增量拆帧器，
/// 而 release 资产那条路上的服务端不会用它。真撞上了，收回来的字节过不了哈希那一关
/// ⇒ 出的是 [`Face::HashMismatch`]，**不会静默把一份坏字节当好的用**。
///
/// 体的上限是**入参**，头的上限是 [`RESPONSE_HEAD_BYTE_CAP`]；**两个量两个数，不许共用一个**
/// （共用的那一版会静默截断，读数在那个常量的头注里）。
/// 超了立刻停手并回 [`Transport::Oversize`]，一个字节都不落盘。
///
/// ⚠ [`Transport::Oversize`] 里那个数是**下界**（我们停在哪儿），不一定是对面那份东西的真长度 ——
/// 真长度要读完才知道，而读完正是上限要拦的事。[`Face::Oversize`] 那句话逐字说的就是「至少」。
pub fn get_over<S: Read + Write>(
    s: &mut S,
    host_header: &str,
    path: &str,
    cap: u64,
) -> (Transport, Vec<u8>) {
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: cc-monitor-remote\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    if s.write_all(req.as_bytes()).is_err() {
        return (Transport::Offline, Vec::new());
    }
    if s.flush().is_err() {
        return (Transport::Offline, Vec::new());
    }
    // 一趟最多读「头上限 + 体上限 + 1」。那个 `+ 1` 是「多读一个字节好分辨
    // 『刚好读满』与『其实还有』」那个惯用形态。
    // 🔴 **两个量必须分两个上限** —— 共用一个的那一版会让头把体的额度吃掉，
    // 而剩下的半份被当成完整的资产（本轮实测，读数写在 `RESPONSE_HEAD_BYTE_CAP` 头注里）。
    let ceiling = RESPONSE_HEAD_BYTE_CAP + cap + 1;
    let mut all = Vec::new();
    if s.take(ceiling).read_to_end(&mut all).is_err() {
        return (Transport::Offline, Vec::new());
    }
    let took = all.len() as u64;
    let sep = b"\r\n\r\n";
    let at = all
        .windows(sep.len())
        .position(|w| w == sep)
        .map(|i| i + sep.len());
    let cut = match at {
        // 头收完了，但它自己就超过了头上限 ⇒ 对面换了个东西。
        Some(i) if i as u64 > RESPONSE_HEAD_BYTE_CAP => {
            return (Transport::Oversize(i as u64), Vec::new())
        }
        Some(i) => i,
        // 读满了上限还没见到头的结尾 ⇒ 同上，也是「对面换了个东西」。
        None if took >= ceiling => return (Transport::Oversize(took), Vec::new()),
        // 没读满就没了 ⇒ 对面把连接掐了，或答了个不是 HTTP 的东西。
        None => return (Transport::Offline, Vec::new()),
    };
    let code = match status_of(&all[..cut]) {
        Some(c) => c,
        // ⚠ 「状态行读不懂」在这里与「连不上」**合成同一张脸**，如实记（同 `Net::get` 那一格）：
        // 失败面是个闭集，而给它加一个成员要**同轮**改 `doc/IPC-PROTOCOL.md`（判据双向对账钉着），
        // 那份文件不在本轮写区。⇒ 今天合着，**归 PM 裁要不要给它自己一张脸**。
        // 承重的那一半守住了：**它绝不会被当成 200**。
        None => return (Transport::Offline, Vec::new()),
    };
    if code != 200 {
        return (Transport::Status(code), Vec::new());
    }
    let body = all.split_off(cut);
    (within_cap(body.len() as u64, cap), body)
}

/// 生产那一份对端：真开 socket、真走 HTTP。
pub struct Net;

impl Origin for Net {
    fn get(&mut self, url: &str, cap: u64) -> (Transport, Vec<u8>) {
        // ⚠ 「地址拆不动」与「连不上」在这里合成同一张脸，如实记：
        // 那个地址来自**编译期的钉**（不是用户输入）⇒ 拆不动说明这份 daemon 的钉本身是坏的，
        // 而那一格已经由 `pin` 那一关挡在前面。到得了这里还拆不动，处置与连不上一致：
        // 都是「这台机器现在拿不到它」，用户能做的事也一样。
        let t = match Target::parse(url) {
            Some(t) => t,
            None => return (Transport::Offline, Vec::new()),
        };
        let mut wire = match dial(&t) {
            Ok(w) => w,
            Err(_) => return (Transport::Offline, Vec::new()),
        };
        get_over(&mut wire, &t.host_header(), &t.path, cap)
    }
}

// ───────────────────────────── 写盘那一跳 ─────────────────────────────

/// 落点上那个文件**今天是什么样** —— 每次现问，不缓存。
pub fn look(path: &Path) -> Landed {
    match std::fs::metadata(path) {
        Ok(m) if !m.is_file() => Landed::Unknown,
        Ok(m) if m.len() == 0 => Landed::Empty,
        Ok(_) => Landed::Present,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Landed::Missing,
        Err(_) => Landed::Unknown,
    }
}

/// 落点上那份字节 —— 读出来给哈希那一关算。
///
/// ⚠ 读不出来**不许读成「不符」**：那会让一台没有读权限的机器每次都重拉一遍。
pub fn read_landed(path: &Path, pinned_sha256: &str) -> Checked {
    match std::fs::read(path) {
        Ok(bytes) => verify_bytes(&bytes, pinned_sha256),
        Err(_) => Checked::Unreadable,
    }
}

/// **写盘那一跳** —— 只剩「那一跳没成时跟人怎么说」这一半。
///
/// 🔴 `K-R55`（09-11）：**平台原语搬进了 [`crate::platform::landing`]**（`K33` 裁定二）。
/// `K-R52` 给这一跳补 cfg 门时在这里逐字登记过为什么当时不搬 ——
/// 只读护栏的写面**按文件认**，那两个写动词只准住在签过字的模块里，而适配层不在那张表上
/// ⇒ 搬要同拍改 `readonly_guard.rs`，那份文件在它写区之外。**本件就是那一拍**：
/// 白名单从本文件改钉 `platform/landing.rs`，**仍然按文件认**
///（`KR55D2` 点名的失效方向正是「为了搬得动而放宽成按目录认」）。
///
/// ⇒ 本函数今天是**纯映射**：适配层那三种结果 → 本层这个闭集。没有 fs 动词、没有平台 cfg。
///
/// # 三格逐条，别合并
///
/// - `Ok(())` ⇒ [`Landing::Landed`]。
/// - 真去写了、撞上 IO 错 ⇒ 交给 [`classify_write_error`]（权限那一类单独一格）。
/// - 🔴 **这台机器上这一跳没有实现** ⇒ [`Landing::Unsupported`]。
///   `K-R52` 那一拍把它并进了 [`Landing::Io`]，并在 [`Landing`] 的头注里逐字登记了
///   那笔债（「不撒谎，但说得不够准」）：那一句用户文案说的是磁盘满 / 目录不存在 / 残骸，
///   **一个字都没提「本平台没实现」**。`K-R55` 把那一格开成独立成员 ——
///   把一个可判别的状态压进笼统状态，是本区最贵的那条病。
pub fn land(path: &Path, bytes: &[u8]) -> Landing {
    use crate::platform::landing::LandFailed;
    match crate::platform::landing::land_executable(path, bytes) {
        Ok(()) => Landing::Landed,
        Err(LandFailed::Unsupported) => Landing::Unsupported,
        Err(LandFailed::Io(kind)) => classify_write_error(kind),
    }
}

// ───────────────────────────── 起进程那一跳 ─────────────────────────────

/// 落下来那一份**跑不起来**的三种样子。
///
/// 🔴 **它刻意不进 [`Face`]**：`Face` 是「把那个二进制拿到手」这条路的闭集，
/// 而这一跳发生在那条路**之后** ——「拿到了、但它跑不起来」与「没拿到」是两件事，
/// 用户该做的也不一样。
/// ⚠ 而且给 `Face` 加成员要**同轮**改 `doc/IPC-PROTOCOL.md`（判据双向对账钉着），
/// 那份文件不在本轮写区 ⇒ 本轮如实把它放在 `Face` 之外。**归 PM 裁要不要合。**
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Unusable {
    /// 根本没起来（不存在 / 没权限 / 别的 IO 错），原文带上。
    NotStarted(String),
    /// 起来了，被那条期限收掉了。
    TimedOut,
    /// 起来了，退了个非零码。`code` 为 `None` = **被信号打断**（与「退了个码」不是一件事）。
    Refused {
        code: Option<i32>,
        diagnosis: String,
    },
}

impl Unusable {
    /// 那**一句**话。每个成员恰好一句。
    pub fn sentence(&self) -> String {
        match self {
            Unusable::NotStarted(why) => {
                format!("代码全景 sidecar 落到盘上了，但起不起来：{why}")
            }
            Unusable::TimedOut => {
                "代码全景 sidecar 起来了，但在期限内没答完 —— 这一趟按失败算。".to_string()
            }
            Unusable::Refused { code, diagnosis } => {
                let c = match code {
                    Some(n) => format!("退出码 {n}"),
                    None => "被信号打断（没有退出码）".to_string(),
                };
                format!("代码全景 sidecar 跑完了但没答应：{c}。它自己说：{diagnosis}")
            }
        }
    }
}

/// **起进程那一跳** —— `K6` 裁定三那四件事里的「传 argv」与「把退出码翻成语义」。
///
/// # ⚠ 它**没有**自己的 `Command::new`，这是本拍的一处**没做到**，理由现打
///
/// 件文件 `KW2D5` 预言的是「sidecar = 一个**新的** `Command::new` 起进程点 ⇒
/// 那张相等断言表当场红 ⇒ 有人签字」。本拍**没有**走那条路，两条理由：
///
/// 1. `ratchet_guard::PINS` 逐字钉着 `readonly_guard.rs` 里
///    `const SPAWN_SITES_TODAY: usize = 9;` **整行、恰好一次**。加一处起进程点就要把 9 改成 10,
///    而那一改会让 `ratchet_guard` 当场红 —— **那份文件不在本轮写区，我修不了。**
/// 2. 本 crate 已经有一个**通用调用口**（`K-W1A` 立的那一处，`readonly_guard::ALLOWED`
///    里单独登记着），而 `KW2D7` 09-04 选的正是「一问一 exec，**不改通用口**」。
///    ⇒ 走它是这两条裁定的合流，不是绕开判据。
///
/// ⇒ **起进程点表这一拍没签成字**，如实交回。
///
/// # 它借那一处口继承了什么（别读成「白拿」）
///
/// 期限靠 `timeout` 当命令前缀交给子进程（零定时器铁律的另一侧）；环境按白名单清空。
/// ⚠ 而那条登记的理由里逐字写着「这一处口从此是**通用**的：将来经它起的每一个插件，
/// 写面都落在这一条理由底下，而这条键**分不出**是哪个插件」—— **本层就是那句话点名的
/// 下一个使用者**，而没有任何机检会替我们想起。**走上报口，请 PM 把这一条理由补上。**
///
/// # 🔴 借那一处口的**第二笔代价**（本轮实测撞出来的，不是设想）
///
/// 真正被起的进程是那条**期限命令**，插件是它的参数 ⇒
/// 「**那个二进制根本不在**」不会走到 [`Unusable::NotStarted`]，
/// 它会以**期限命令自己的 127** 回来，落进 [`Unusable::Refused`]。
/// 判据 [`tests::the_spawn_hop_really_starts_a_process_and_reads_its_exit_code_back`]
/// 头一版断的是前者，**当场红**，红得对。
///
/// ⇒ [`Unusable::NotStarted`] 今天**只在期限命令本身也不在时**才走得到（那时通用口裸跑插件）。
/// ⚠ 这一格**没有被修掉，只是被登记**：要分开就得在起进程之前先 stat 一次那个二进制，
/// 而「那个文件在不在」已经有一个住址（[`super::fetch::Landed`]）——
/// 再取一次样就是**同一个事实两个住址**，正是 `K-W4` 那条病。
/// ⇒ 今天靠 [`Unusable::Refused`] 那句话把子进程的原话原样带出去（它逐字写着
/// 「No such file or directory」＋那个路径），**看得见，不静默**。
/// 真要分开，正确的做法是让调用方把 [`obtain`] 回的落点直接传进来 —— 那是接线那一拍的事。
pub fn ask(bin: &Path, args: &[&str], deadline_secs: u64) -> Result<Vec<u8>, Unusable> {
    let done: Done = match invoke::run(bin, args, deadline_secs, &[]) {
        Ok(d) => d,
        Err(NotRun::ArgListTooLong) => {
            return Err(Unusable::NotStarted(
                "这一条问句太长，塞不进一次命令调用".to_string(),
            ))
        }
        Err(NotRun::Failed(why)) => return Err(Unusable::NotStarted(why)),
    };
    if done.timed_out() {
        return Err(Unusable::TimedOut);
    }
    match done.code {
        Some(0) => Ok(done.stdout),
        code => Err(Unusable::Refused {
            code,
            diagnosis: done.diagnosis(),
        }),
    }
}

// ───────────────────────────── 三跳合起来 ─────────────────────────────

/// **拿到那个二进制**：盘上有就用盘上的，没有就去拉一趟。回的是它的落点。
///
/// 三个事实各自现问、各自独立取样，判定全部走 [`super::fetch`] 那边的纯函数 ——
/// 本函数只负责**按次序动世界**，一条判定都不自己写。
///
/// ⚠ `dir` 是入参，本层不知道「落点在哪」：那是 `inbound` 那一侧的知识（`E6`）。
pub fn obtain<O: Origin>(
    origin: &mut O,
    dir: &Path,
    pin: &Pin,
    arch: &str,
    cap: u64,
) -> Result<PathBuf, Face> {
    // ⚠ 解构而不是 `pin.` 取字段：禁词表里那条**目录级标记名**是子串匹配，
    // 而字段访问那一形恰好含着它。这不是洁癖，是那条判据今天真的会红。
    let Pin {
        base: _,
        tag: _,
        build_id,
        sha256,
    } = *pin;
    let path = dir.join(super::fetch::landing_name(build_id, arch));

    let next = match use_or_fetch(look(&path), Integrity::NotChecked) {
        // 盘上有一份 —— 现算一遍哈希，再问一次。**「没算过」不是「不符」。**
        Step::Verify => use_or_fetch(
            Landed::Present,
            Integrity::Checked(read_landed(&path, sha256)),
        ),
        other => other,
    };
    match next {
        Step::Use => return Ok(path),
        Step::GiveUp(f) => return Err(f),
        Step::Verify => return Err(Face::BytesUnreadable),
        Step::Fetch(_) => {}
    }

    let url = asset_url(pin, arch);
    let (hop, bytes) = origin.get(&url, cap);
    // `Got(n)` 里那个 `n` 只用来出声；判定用 `bytes` —— 一个长度不许有两个住址。
    transport_verdict(hop)?;
    integrity_verdict(verify_bytes(&bytes, sha256))?;
    landing_verdict(land(&path, &bytes))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecars::codepicture::fetch::landing_name;
    use std::io::BufRead;
    use std::net::TcpListener;

    /// 一台**真的**回环 HTTP 服务器 —— 断网容器里 `lo` 是通的，本轮实测过。
    ///
    /// 它一趟只服务一个连接，答一份写死的字节。**它不是 HTTP 客户端的复刻**：
    /// 被测的是生产那一份 `get_over`，这里只提供对端。
    ///
    /// 🔴 **那个读期限不是装饰**（死值验台逼出来的）：生产那一侧**没有任何期限**
    /// （代价整段登记在模块头注第三节第 1 条）⇒ 一刀把「真的发请求」那一步切掉之后，
    /// 两边会**互相等到天荒地老**：服务端等请求、客户端等响应。
    /// 死值验台 09-10 就这样挂死过一趟。⇒ 对端这一侧设期限，让那一刀能**红**而不是**挂**。
    /// ⚠ `Duration` 只出现在测试段：`no_timer_guard` 的人群是剥掉测试段之后的生产文本。
    fn serve_once(head: &'static str, body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").expect("回环上绑不上口 —— 台子坏了");
        let port = l.local_addr().expect("取不到端口").port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = match l.accept() {
                Ok(v) => v,
                Err(_) => return,
            };
            let _ = c.set_read_timeout(Some(std::time::Duration::from_secs(3)));
            let mut r = std::io::BufReader::new(c.try_clone().expect("clone"));
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 0 {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                line.clear();
            }
            let _ = c.write_all(head.as_bytes());
            let _ = c.write_all(&body);
            let _ = c.flush();
        });
        (format!("http://127.0.0.1:{port}/asset"), h)
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "kw2d-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("建临时目录");
        d
    }

    const OK_HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n\r\n";

    /// ★ **HTTP 那一跳被真的走到一次** —— 真 socket、真请求、真状态行、真响应体。
    #[test]
    fn the_http_hop_really_goes_over_a_socket_and_brings_the_bytes_back() {
        let want: Vec<u8> = (0u8..=255).cycle().take(5000).collect();
        let (url, h) = serve_once(OK_HEAD, want.clone());
        let (hop, got) = Net.get(&url, ASSET_BYTE_CAP);
        h.join().expect("服务端线程");
        assert_eq!(hop, Transport::Got(want.len() as u64));
        assert_eq!(got, want, "收回来的字节与对面发的不是同一份");
    }

    /// ★ 那台服务器**真的被问了一句 HTTP** —— 否则上面那条可以靠「对面无脑吐字节」过。
    #[test]
    fn the_request_we_write_is_a_real_get_with_a_host_header() {
        let l = TcpListener::bind("127.0.0.1:0").expect("绑口");
        let port = l.local_addr().expect("端口").port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().expect("accept");
            // 期限同 `serve_once`：客户端不发请求时，这一侧要**报错**而不是挂住。
            let _ = c.set_read_timeout(Some(std::time::Duration::from_secs(3)));
            let mut buf = [0u8; 512];
            let n = c
                .read(&mut buf)
                .expect("客户端一个字节都没发过来 —— 请求那一步没走");
            let _ = c.write_all(OK_HEAD.as_bytes());
            let _ = c.write_all(b"x");
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        let (hop, _) = Net.get(&format!("http://127.0.0.1:{port}/a/b"), ASSET_BYTE_CAP);
        let req = h.join().expect("服务端线程");
        assert_eq!(hop, Transport::Got(1));
        assert!(
            req.starts_with("GET /a/b HTTP/1.1\r\n"),
            "请求行不对：{req}"
        );
        assert!(
            req.contains(&format!("Host: 127.0.0.1:{port}\r\n")),
            "Host 头不对：{req}"
        );
    }

    /// ★ 非 200 有自己的一格，**不许被当成拿到了**。
    #[test]
    fn a_non_200_answer_is_its_own_face_and_carries_no_bytes() {
        let (url, h) = serve_once("HTTP/1.1 404 Not Found\r\n\r\n", b"nope".to_vec());
        let (hop, got) = Net.get(&url, ASSET_BYTE_CAP);
        h.join().expect("服务端线程");
        assert_eq!(hop, Transport::Status(404));
        assert!(got.is_empty(), "非 200 那一支不许把字节带出来");
        assert_eq!(transport_verdict(hop), Err(Face::HttpStatus(404)));
    }

    /// ★ 上限在**真流**上也咬人（不是只在 `within_cap` 那个纯函数上）。
    #[test]
    fn the_cap_bites_on_a_real_stream_too() {
        let (url, h) = serve_once(OK_HEAD, vec![7u8; 200]);
        let (hop, _) = Net.get(&url, 100);
        h.join().expect("服务端线程");
        assert_eq!(hop, Transport::Oversize(200));
        assert_eq!(transport_verdict(hop), Err(Face::Oversize(200)));
    }

    /// ★ 连不上有自己的一格：对着一个**没人听**的口。
    #[test]
    fn nobody_listening_is_offline_not_a_zero_length_success() {
        let l = TcpListener::bind("127.0.0.1:0").expect("绑口");
        let port = l.local_addr().expect("端口").port();
        drop(l);
        let (hop, got) = Net.get(&format!("http://127.0.0.1:{port}/x"), ASSET_BYTE_CAP);
        assert_eq!(hop, Transport::Offline);
        assert!(got.is_empty());
    }

    /// ★ 地址拆不动那一格：**不许被当成 200**（它今天与连不上同一张脸，理由写在 `Net` 里）。
    #[test]
    fn an_address_we_cannot_take_apart_never_becomes_a_success() {
        for bad in ["ftp://h/x", "https:///x", "http://h:99999/x", "not a url"] {
            assert!(Target::parse(bad).is_none(), "`{bad}` 竟然拆得动");
            let (hop, got) = Net.get(bad, ASSET_BYTE_CAP);
            assert_eq!(hop, Transport::Offline, "`{bad}` 出的不是那张脸");
            assert!(got.is_empty());
        }
        // 反向：真的拆得动的那几形要拆对，否则上面那条靠「什么都拆不动」恒真。
        let t = Target::parse("https://example.invalid/v9/code-picture-x86_64").expect("拆得动");
        assert_eq!(
            t,
            Target {
                tls: true,
                host: "example.invalid".to_string(),
                port: 443,
                path: "/v9/code-picture-x86_64".to_string(),
            }
        );
        assert_eq!(
            t.host_header(),
            "example.invalid",
            "默认端口不该写进 Host 头"
        );
        let p = Target::parse("http://h:8080/a").expect("拆得动");
        assert_eq!(p.host_header(), "h:8080", "非默认端口要写进 Host 头");
    }

    /// ★ 状态行读不懂**不许读成 200**（对面答了个不是 HTTP 的东西）。
    #[test]
    fn an_unreadable_status_line_is_not_read_as_success() {
        assert_eq!(status_of(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(status_of(b"HTTP/1.0 503 x\r\n"), Some(503));
        assert_eq!(status_of(b"<html>\r\n"), None);
        assert_eq!(status_of(b"HTTP/1.1 zzz\r\n"), None);
        assert_eq!(status_of(b""), None);
        let (url, h) = serve_once("<html>not http</html>\r\n\r\n", b"body".to_vec());
        let (hop, got) = Net.get(&url, ASSET_BYTE_CAP);
        h.join().expect("服务端线程");
        assert_eq!(hop, Transport::Offline);
        assert!(got.is_empty());
    }

    /// ★ 这一层信的根证书**不是空的** —— 中转那一层被整个换成空集时 384 条判据全绿，
    /// 那次的教训在这里也算数：打的是**真正装进去的那一份**，不是那个 crate 常量。
    #[test]
    fn the_trust_anchors_this_layer_installs_are_not_empty() {
        let n = trust_roots().roots.len();
        assert!(n > 0, "本层装了一份空的根证书集 —— 那等于谁的证书都不认");
        assert_eq!(
            n,
            webpki_roots::TLS_SERVER_ROOTS.len(),
            "本层装进去的根证书数与那个 crate 常量对不上 —— 中间被筛过一遍"
        );
    }

    /// ★ **写盘那一跳被真的走到一次**：真落到盘上、字节一样、而且**可执行**。
    ///
    /// 🔴 `K-R122`（09-14）**这一条 `#[cfg(unix)]` 不是为了让编译器闭嘴** —— 理由在语义上：
    /// 本条断言的 `Landing::Landed` ＋「落下来那份带可执行位」，在非 unix 上**根本不成立**：
    /// [`crate::platform::landing::land_executable`] 的 `#[cfg(not(unix))]` 那一臂
    /// **不写盘、直接回 `LandFailed::Unsupported`**（那条设计逐字写在它的头注里：
    /// 「照写并回成功」＝ 假装设置了可执行位，明禁）⇒ 本条在 Windows 上就算编得过也**必红**。
    /// ⇒ 本条的正确归宿是「unix 专属的一格」，非 unix 那一格由
    /// `platform/landing.rs` 里那条 `#[cfg(not(unix))]` 的对照测试买。
    #[cfg(unix)]
    #[test]
    fn the_landing_hop_really_writes_an_executable_file() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("land");
        let p = d.join(landing_name("p9z", "x86_64"));
        assert_eq!(look(&p), Landed::Missing);
        assert_eq!(land(&p, b"hello"), Landing::Landed);
        assert_eq!(look(&p), Landed::Present);
        assert_eq!(std::fs::read(&p).expect("读回来"), b"hello");
        let mode = std::fs::metadata(&p).expect("stat").permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "落下来的那一份没有可执行位：{mode:o}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ **只准新增**：撞上一份既有文件就失败，不覆盖、不截断。
    ///
    /// 🔴 顺带把模块头注第三节第 3 条那一格钉住：落点上躺着 0 字节残骸时，
    /// 判定说「去拉」而落盘落不下去 —— **本层今天修不好它，只能出声。**
    #[test]
    fn an_existing_file_is_never_overwritten_and_the_zero_byte_leftover_is_a_dead_end() {
        let d = tmp("excl");
        let p = d.join("x");
        assert_eq!(land(&p, b"first"), Landing::Landed);
        assert_ne!(
            land(&p, b"second"),
            Landing::Landed,
            "第二次落盘竟然成功了 —— 那就是覆盖既有数据"
        );
        assert_eq!(std::fs::read(&p).expect("读回来"), b"first");

        let z = d.join("z");
        assert_eq!(land(&z, b""), Landing::Landed);
        assert_eq!(look(&z), Landed::Empty, "0 字节该是自己一格");
        assert!(
            matches!(
                use_or_fetch(look(&z), Integrity::NotChecked),
                Step::Fetch(_)
            ),
            "0 字节残骸该判「去拉」"
        );
        assert_ne!(land(&z, b"again"), Landing::Landed, "残骸竟然被盖掉了");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 落点目录不存在 / 不可写，各自出声，**不许静默**。
    ///
    /// 🔴 `K-R122`（09-14）同上一条的理由，**外加一条它自己的**：本条造「不可写」靠的是
    /// `chmod 0o500`，那是 **POSIX 权限位**这个概念；Windows 上没有这个概念的等价物
    /// （要造同一个前提得走 ACL，那是另一套原语、另一条被测路）。
    /// 而它断的 `Landing::Io` / `Landing::Denied` 在非 unix 上也一律是 `Unsupported`。
    /// ⇒ **加 `cfg` 而不是加抽象**：给「chmod」发明一个跨平台抽象，等于替 Windows
    /// 编一个它没有的语义 —— 正是 `platform/fallback_guard` 头注点名的那一形。
    #[cfg(unix)]
    #[test]
    fn a_landing_that_cannot_happen_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("denied");
        assert_eq!(
            land(&d.join("no-such-dir").join("x"), b"a"),
            Landing::Io,
            "目录不存在该落到那一格（本层建不出目录，头注第三节第 2 条）"
        );
        let ro = d.join("ro");
        std::fs::create_dir_all(&ro).expect("建只读目录");
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o500)).expect("收权限");
        assert_eq!(land(&ro.join("x"), b"a"), Landing::Denied);
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o700)).expect("放回来");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 造一个**扮演 sidecar** 的可执行文件 —— 三跳里的第三跳要的对端。
    fn fake_bin(dir: &Path, name: &str, script: &str) -> PathBuf {
        let p = dir.join(name);
        assert_eq!(land(&p, script.as_bytes()), Landing::Landed);
        p
    }

    /// ★ **起进程那一跳被真的走到一次**：起一个真的进程、拿回它的 stdout 与退出码。
    #[test]
    fn the_spawn_hop_really_starts_a_process_and_reads_its_exit_code_back() {
        let d = tmp("spawn");
        let good = fake_bin(&d, "good", "#!/bin/sh\necho ok-from-sidecar\n");
        assert_eq!(
            ask(&good, &["--capabilities"], 20),
            Ok(b"ok-from-sidecar\n".to_vec())
        );

        let bad = fake_bin(&d, "bad", "#!/bin/sh\necho boom >&2\nexit 7\n");
        match ask(&bad, &[], 20) {
            Err(Unusable::Refused { code, diagnosis }) => {
                assert_eq!(code, Some(7));
                assert_eq!(diagnosis, "boom");
            }
            other => panic!("非零退出码没被翻成那一格：{other:?}"),
        }

        // 🔴 「那个二进制不在」**不会**走到 `NotStarted` —— 真正被起的是那条期限命令，
        // 它以自己的 127 回来。这一格是本轮实测撤回一句错断言之后的读数，
        // 理由整段住 `ask` 的头注。**买到的是「它不静默」：子进程的原话原样带出来。**
        let missing = d.join("not-there");
        match ask(&missing, &[], 20) {
            Err(Unusable::Refused { code, diagnosis }) => {
                assert_eq!(code, Some(127), "期限命令报「找不到被起的那个」不是 127 了");
                assert!(
                    diagnosis.contains("not-there"),
                    "那句诊断里没有说不见的是哪一个：{diagnosis}"
                );
                assert!(
                    Unusable::Refused { code, diagnosis }
                        .sentence()
                        .contains("not-there"),
                    "那一句话把子进程的原话吞了 —— 用户就看不见是哪个文件不在"
                );
            }
            other => panic!("文件不在时出的不是那一格：{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 三种坏法**各有各的一句话**，一句都不许空、不许重。
    #[test]
    fn every_way_it_can_be_unusable_says_something_of_its_own() {
        let all = [
            Unusable::NotStarted("x".to_string()),
            Unusable::TimedOut,
            Unusable::Refused {
                code: Some(7),
                diagnosis: "y".to_string(),
            },
        ];
        let mut said: Vec<String> = all.iter().map(|u| u.sentence()).collect();
        assert!(said.iter().all(|s| !s.is_empty()));
        let n = said.len();
        said.sort();
        said.dedup();
        assert_eq!(said.len(), n, "两种坏法说了同一句话");
        // 「被信号打断」与「退了个码」不许说成同一句。
        assert_ne!(
            Unusable::Refused {
                code: None,
                diagnosis: "y".to_string()
            }
            .sentence(),
            Unusable::Refused {
                code: Some(0),
                diagnosis: "y".to_string()
            }
            .sentence()
        );
    }

    /// ★★ **三跳串起来走一遍**（不出网）：拉 → 校验 → 落盘 → 起起来。
    ///
    /// 这一条是本拍的正题：`obtain` 里每一步都是生产代码，对端是入参。
    #[test]
    fn the_whole_hop_runs_end_to_end_without_a_network() {
        let d = tmp("e2e");
        let script = "#!/bin/sh\necho landed-and-ran\n";
        let sha = super::super::fetch::sha256_hex(script.as_bytes());
        let (url, h) = serve_once(OK_HEAD, script.as_bytes().to_vec());
        // 钉里的 base 直接指回环那台服务器；`asset_url` 会把 tag 与资产名拼上去，
        // 而那台服务器对任何路径都答同一份 —— 拼法本身由 `M1` 那一刀单独钉。
        let base: &'static str =
            Box::leak(url.trim_end_matches("/asset").to_string().into_boxed_str());
        let sha_s: &'static str = Box::leak(sha.clone().into_boxed_str());
        let pin = Pin {
            base,
            tag: "v0",
            build_id: "p9z",
            sha256: sha_s,
        };
        let got = obtain(&mut Net, &d, &pin, "x86_64", ASSET_BYTE_CAP).expect("三跳没走通");
        h.join().expect("服务端线程");
        assert_eq!(got, d.join(landing_name("p9z", "x86_64")));
        assert_eq!(ask(&got, &[], 20), Ok(b"landed-and-ran\n".to_vec()));

        // 第二趟：盘上已经有那一份、哈希也对 ⇒ **直接用，不再拉**（没有服务器在听了）。
        assert_eq!(
            obtain(&mut Net, &d, &pin, "x86_64", ASSET_BYTE_CAP),
            Ok(got.clone())
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 拉回来的字节与钉住的哈希不符 ⇒ **到此为止，一个字节都不落盘**。
    #[test]
    fn bytes_that_do_not_match_the_pin_never_reach_the_disk() {
        let d = tmp("mismatch");
        let (url, h) = serve_once(OK_HEAD, b"not-the-thing".to_vec());
        let base: &'static str =
            Box::leak(url.trim_end_matches("/asset").to_string().into_boxed_str());
        let pin = Pin {
            base,
            tag: "v0",
            build_id: "p9z",
            sha256: "00000000000000000000000000000000000000000000000000000000000000ff",
        };
        assert_eq!(
            obtain(&mut Net, &d, &pin, "x86_64", ASSET_BYTE_CAP),
            Err(Face::HashMismatch)
        );
        h.join().expect("服务端线程");
        assert_eq!(
            look(&d.join(landing_name("p9z", "x86_64"))),
            Landed::Missing,
            "哈希不符那一趟竟然落了盘"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 盘上那一份**不是**钉住的那一份 ⇒ 判定说重拉；而重拉落不下去（`O_EXCL`）⇒ 出声。
    ///
    /// 反向那半在这里很要紧：只断「哈希对就直接用」的话，一个**从来不校验**的实现照样绿。
    #[test]
    fn a_stale_copy_on_disk_is_not_silently_used() {
        let d = tmp("stale");
        let p = d.join(landing_name("p9z", "x86_64"));
        assert_eq!(land(&p, b"an-older-build"), Landing::Landed);
        let (url, h) = serve_once(OK_HEAD, b"the-right-one".to_vec());
        let base: &'static str =
            Box::leak(url.trim_end_matches("/asset").to_string().into_boxed_str());
        let sha: &'static str =
            Box::leak(super::super::fetch::sha256_hex(b"the-right-one").into_boxed_str());
        let pin = Pin {
            base,
            tag: "v0",
            build_id: "p9z",
            sha256: sha,
        };
        assert_eq!(
            obtain(&mut Net, &d, &pin, "x86_64", ASSET_BYTE_CAP),
            Err(Face::LandingIo),
            "盘上那份过期了：判定该说重拉，而落盘该撞上 O_EXCL 并出声"
        );
        let _ = h.join();
        assert_eq!(
            std::fs::read(&p).expect("读回来"),
            b"an-older-build",
            "既有文件被动过了"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 🔴 **`stat` 真的失败**那一支（不是「它在、但不是普通文件」那一支）。
    ///
    /// 这一格是本轮死值验 `W10` 逮到的**空刀**补出来的，如实记：
    /// 头一版只有「落点上是个目录」那个夹具，而那走的是 `!m.is_file()` 那一支
    /// ⇒ 把 `Err(_) => Unknown` 整个改成 `=> Missing`，**一条都不红**。
    /// 「问不出来不许读成一个确定答案」这句话当时只在注释里，机器一个字都没管。
    ///
    /// ⇒ 这一条造一个**读不进去的目录**，stat 它里面的东西必然报错（`EACCES`）。
    /// 那一格读成 `Missing` 的后果是**每次都重拉**；读成 `Present` 的后果是
    /// **拿一份来历不明的字节去 exec**。两条都不行，所以它必须是自己一格。
    ///
    /// 🔴 `K-R122`（09-14）**这一条与上面两条不同源，单独说**：被测的 [`look`] 本身
    /// 是跨平台的（只用 `std::fs::metadata`），红的是**造前提那一步** ——
    /// 「让 stat 必然失败」在 unix 上是 `chmod 0o000` 的目录，Windows 上没有同形原语。
    /// ⇒ 这一格在非 unix 上今天是**判不了**，不是「通过」：如实登记成 unix 专属，
    /// 别拿一个造得出来但形状不同的前提（比如删掉目录）冒充它 ——
    /// 那走的是 `Err(NotFound) => Missing` 那一支，正是本条要区分开的另一格。
    #[cfg(unix)]
    #[test]
    fn a_stat_that_fails_is_unknown_not_missing() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("noperm");
        let locked = d.join("locked");
        std::fs::create_dir_all(&locked).expect("建目录");
        let inside = locked.join("x");
        assert_eq!(look(&inside), Landed::Missing, "收权限之前它该是「不在」");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("收权限");
        let got = look(&inside);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).expect("放回来");
        assert_eq!(
            got,
            Landed::Unknown,
            "stat 失败被读成了一个确定答案 —— 读成「不在」就每次重拉，读成「在」就去 exec 一份来历不明的字节"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 落点问不出来 ⇒ **出声**，既不当它在、也不当它不在（一个字节都不拉）。
    #[test]
    fn a_landing_we_cannot_stat_stops_the_whole_thing() {
        let d = tmp("unknown");
        let sub = d.join("dir-not-file");
        std::fs::create_dir_all(&sub.join(landing_name("p9z", "x86_64"))).expect("造一个目录挡住");
        let pin = Pin {
            base: "http://127.0.0.1:1/nope",
            tag: "v0",
            build_id: "p9z",
            sha256: "00",
        };
        assert_eq!(
            obtain(&mut Net, &sub, &pin, "x86_64", ASSET_BYTE_CAP),
            Err(Face::LandingUnreadable)
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
